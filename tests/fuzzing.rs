//! Comprehensive Fuzzing Suite for the Risk Engine
//!
//! ## Running Tests
//! - Quick: `cargo test --features fuzz` (100 proptest cases, 200 deterministic seeds)
//! - Deep: `PROPTEST_CASES=1000 cargo test --features fuzz fuzz_deterministic_extended`
//!
//! ## Atomicity Model (Solana)
//!
//! This program relies on Solana transaction atomicity: if any instruction returns Err,
//! the entire transaction is aborted and no account state changes are committed.
//! Therefore we do not require "no mutation on Err" inside a single instruction.
//!
//! All functions must still propagate errors (never ignore a Result and continue).
//! The fuzz suite simulates Solana atomicity by cloning engine state before each op
//! and restoring on Err. Invariants are only asserted after successful (Ok) operations.
//!
//! ## Invariant Definitions
//!
//! ### Conservation (check_conservation)
//! vault + loss_accum >= sum(capital) + sum(settled_pnl) + insurance
//!
//! Where settled_pnl accounts for lazy funding:
//!   settled_pnl = account.pnl - (global_funding_index - account.funding_index) * position / 1e6
//!
//! Slack rule: actual >= expected, and (actual - expected) <= MAX_ROUNDING_SLACK
//! This ensures vault has at least what is owed, with bounded dust.
//!
//! ### Warmup Budget
//! - W+ <= W- + raw_spendable (positive warmup bounded by losses + available insurance)
//! - reserved <= raw_spendable (reservations backed by insurance)
//!
//! ## Suite Components
//! - Global invariants (conservation, warmup budget, risk reduction mode)
//! - Action-based state machine fuzzer with Solana rollback simulation
//! - Focused unit property tests
//! - Deterministic seeded fuzzer with logging

#![cfg(feature = "fuzz")]

use percolator::*;
use proptest::prelude::*;

// ============================================================================
// CONSTANTS AND MATCHER
// ============================================================================

const MATCHER: NoOpMatcher = NoOpMatcher;

// Default oracle price for conservation checks
const DEFAULT_ORACLE: u64 = 1_000_000;

// ============================================================================
// SECTION 1: HELPER FUNCTIONS
// ============================================================================

/// Helper to check if an account slot is used by accessing the used bitmap
fn is_account_used(engine: &RiskEngine, idx: u16) -> bool {
    let idx = idx as usize;
    if idx >= engine.accounts.len() {
        return false;
    }
    // Access the used bitmap directly: used[w] bit b
    let w = idx >> 6; // word index (idx / 64)
    let b = idx & 63; // bit index (idx % 64)
    if w >= engine.used.len() {
        return false;
    }
    ((engine.used[w] >> b) & 1) == 1
}

/// Helper to get the safe upper bound for account iteration
#[inline]
fn account_count(engine: &RiskEngine) -> usize {
    core::cmp::min(engine.params.max_accounts as usize, engine.accounts.len())
}

/// Compute funding payment with vault-favoring rounding.
/// Round UP when account pays (raw > 0), truncate when account receives (raw < 0).
/// This matches the engine's settle_account_funding and ensures one-sided conservation.
#[inline]
fn funding_payment(position: i128, delta_f: i128) -> i128 {
    let raw = position.saturating_mul(delta_f);
    if raw > 0 {
        raw.saturating_add(999_999).saturating_div(1_000_000)
    } else {
        raw.saturating_div(1_000_000)
    }
}

// ============================================================================
// SECTION 2: GLOBAL INVARIANTS HELPER
// ============================================================================

/// Assert all global invariants hold
/// IMPORTANT: This function is PURE - it does NOT mutate the engine.
/// Invariant checks must reflect on-chain semantics (funding is lazy).
fn assert_global_invariants(engine: &RiskEngine, context: &str, oracle_price: u64) {
    // 1. Conservation
    // Note: check_conservation now accounts for lazy funding internally and includes mark PnL
    if !engine.check_conservation(oracle_price) {
        // Compute details for debugging (using settled PNL like check_conservation does)
        let mut total_capital = 0u128;
        let mut net_settled_pnl: i128 = 0;
        let mut account_details = Vec::new();
        let global_index = engine.funding_index_qpb_e6;

        let n = account_count(engine);
        for i in 0..n {
            if is_account_used(engine, i as u16) {
                let acc = &engine.accounts[i];
                total_capital += acc.capital;

                // Compute settled PNL using shared helper (matches engine rounding)
                let mut settled_pnl = acc.pnl;
                if acc.position_size != 0 {
                    let delta_f = global_index.saturating_sub(acc.funding_index);
                    if delta_f != 0 {
                        let payment = funding_payment(acc.position_size, delta_f);
                        settled_pnl = settled_pnl.saturating_sub(payment);
                    }
                }
                net_settled_pnl = net_settled_pnl.saturating_add(settled_pnl);

                account_details.push(format!(
                    "  acc[{}]: capital={}, pnl={}, settled_pnl={}, pos={}, fidx={}",
                    i, acc.capital, acc.pnl, settled_pnl, acc.position_size, acc.funding_index
                ));
            }
        }
        let base = total_capital + engine.insurance_fund.balance;
        let expected = if net_settled_pnl >= 0 {
            base + net_settled_pnl as u128
        } else {
            base.saturating_sub((-net_settled_pnl) as u128)
        };
        let actual = engine.vault + engine.loss_accum;

        let slack: i128 = if actual >= expected {
            (actual - expected) as i128
        } else {
            -((expected - actual) as i128)
        };
        panic!(
            "{}: Conservation invariant violated!\n\
             vault={}, loss_accum={}, actual={}\n\
             total_capital={}, insurance={}, net_settled_pnl={}, expected={}\n\
             global_funding_index={}, slack={}\n\
             Accounts:\n{}",
            context,
            engine.vault,
            engine.loss_accum,
            actual,
            total_capital,
            engine.insurance_fund.balance,
            net_settled_pnl,
            expected,
            global_index,
            slack,
            account_details.join("\n")
        );
    }

    // 2. Warmup budget & reservation invariants
    let raw_spendable = engine.insurance_spendable_raw();

    // W+ <= W- + raw_spendable
    assert!(
        engine.warmed_pos_total <= engine.warmed_neg_total.saturating_add(raw_spendable),
        "{}: Warmup budget invariant violated: W+={} > W-={} + raw_spendable={}",
        context,
        engine.warmed_pos_total,
        engine.warmed_neg_total,
        raw_spendable
    );

    // Reserved equality invariant: reserved == min(max(W+ - W-, 0), raw)
    let needed = engine
        .warmed_pos_total
        .saturating_sub(engine.warmed_neg_total);
    let expected_reserved = core::cmp::min(needed, raw_spendable);
    assert!(
        engine.warmup_insurance_reserved == expected_reserved,
        "{}: Reserved equality violated: reserved={} != expected={}",
        context,
        engine.warmup_insurance_reserved,
        expected_reserved
    );

    // insurance_balance >= floor + reserved (with rounding tolerance)
    let floor = engine.params.risk_reduction_threshold;
    let min_balance = floor.saturating_add(engine.warmup_insurance_reserved);
    // Allow 1 unit rounding tolerance
    assert!(
        engine.insurance_fund.balance + 1 >= min_balance,
        "{}: Insurance {} below floor+reserved={}",
        context,
        engine.insurance_fund.balance,
        min_balance
    );

    // 3. Risk reduction mode semantics
    if engine.risk_reduction_only {
        assert!(
            engine.warmup_paused,
            "{}: risk_reduction_only=true but warmup_paused=false",
            context
        );
    }

    if engine.warmup_paused {
        assert!(
            engine.warmup_pause_slot <= engine.current_slot,
            "{}: warmup_pause_slot {} > current_slot {}",
            context,
            engine.warmup_pause_slot,
            engine.current_slot
        );
    }

    // 4. Derived reserved check: reserved == min(max(W+ - W-, 0), raw)
    let expected_reserved = {
        let required = engine
            .warmed_pos_total
            .saturating_sub(engine.warmed_neg_total);
        core::cmp::min(required, raw_spendable)
    };
    assert!(
        engine.warmup_insurance_reserved == expected_reserved,
        "{}: Reserved invariant violated: actual={} expected=min(max({}-{},0),{})={}",
        context,
        engine.warmup_insurance_reserved,
        engine.warmed_pos_total,
        engine.warmed_neg_total,
        raw_spendable,
        expected_reserved
    );

    // 5. Account local sanity (for each used account)
    let n = account_count(engine);
    for i in 0..n {
        if is_account_used(engine, i as u16) {
            let acc = &engine.accounts[i];

            // reserved_pnl <= max(0, pnl)
            let positive_pnl = if acc.pnl > 0 { acc.pnl as u128 } else { 0 };
            assert!(
                acc.reserved_pnl <= positive_pnl,
                "{}: Account {} has reserved_pnl={} > positive_pnl={}",
                context,
                i,
                acc.reserved_pnl,
                positive_pnl
            );

            // Note: slope >= 1 when positive_pnl > 0 is enforced by debug_assert in
            // update_warmup_slope() itself. We don't check it here because accounts
            // can have positive PnL from trading without having called update_warmup_slope yet.
        }
    }
}

// ============================================================================
// SECTION 3: PARAMETER REGIMES
// ============================================================================

/// Regime A: Normal mode (floor = 0 or small)
fn params_regime_a() -> RiskParams {
    RiskParams {
        warmup_period_slots: 100,
        maintenance_margin_bps: 500,
        initial_margin_bps: 1000,
        trading_fee_bps: 10,
        max_accounts: 32, // Small for speed
        new_account_fee: 0,
        risk_reduction_threshold: 0,
        maintenance_fee_per_slot: 0,
        max_crank_staleness_slots: u64::MAX,
        liquidation_fee_bps: 50,
        liquidation_fee_cap: 100_000,
        liquidation_buffer_bps: 100,
        min_liquidation_abs: 100_000,
    }
}

/// Regime B: Floor + risk mode sensitivity (floor = 1000)
fn params_regime_b() -> RiskParams {
    RiskParams {
        warmup_period_slots: 100,
        maintenance_margin_bps: 500,
        initial_margin_bps: 1000,
        trading_fee_bps: 10,
        max_accounts: 32, // Small for speed
        new_account_fee: 0,
        risk_reduction_threshold: 1000,
        maintenance_fee_per_slot: 0,
        max_crank_staleness_slots: u64::MAX,
        liquidation_fee_bps: 50,
        liquidation_fee_cap: 100_000,
        liquidation_buffer_bps: 100,
        min_liquidation_abs: 100_000,
    }
}

// ============================================================================
// SECTION 4: SELECTOR-BASED ACTION ENUM AND STRATEGIES
// ============================================================================

/// Index selector - resolved at runtime against live state
/// This allows proptest to generate meaningful action sequences
/// even though it can't see runtime state during strategy generation.
#[derive(Clone, Debug)]
enum IdxSel {
    /// Pick any account from live_accounts (fallback to Random if empty)
    Existing,
    /// Pick an account that is NOT the LP (fallback to Random if impossible)
    ExistingNonLp,
    /// Use the LP index (fallback to 0 if no LP)
    Lp,
    /// Random index 0..64 (to test AccountNotFound paths)
    Random(u16),
}

/// Actions use selectors instead of concrete indices
/// Selectors are resolved at runtime in execute()
#[derive(Clone, Debug)]
enum Action {
    AddUser {
        fee_payment: u128,
    },
    AddLp {
        fee_payment: u128,
    },
    Deposit {
        who: IdxSel,
        amount: u128,
    },
    Withdraw {
        who: IdxSel,
        amount: u128,
    },
    AdvanceSlot {
        dt: u64,
    },
    AccrueFunding {
        dt: u64,
        oracle_price: u64,
        rate_bps: i64,
    },
    Touch {
        who: IdxSel,
    },
    ExecuteTrade {
        lp: IdxSel,
        user: IdxSel,
        oracle_price: u64,
        size: i128,
    },
    // Note: ApplyAdl removed - it's internal and tested via PanicSettleAll/ForceRealizeLosses
    PanicSettleAll {
        oracle_price: u64,
    },
    ForceRealizeLosses {
        oracle_price: u64,
    },
    TopUpInsurance {
        amount: u128,
    },
}

/// Strategy for generating index selectors
/// Weights: Existing=6, ExistingNonLp=2, Lp=1, Random=2
/// This ensures most actions target valid accounts while still testing error paths
fn idx_sel_strategy() -> impl Strategy<Value = IdxSel> {
    prop_oneof![
        6 => Just(IdxSel::Existing),
        2 => Just(IdxSel::ExistingNonLp),
        1 => Just(IdxSel::Lp),
        2 => (0u16..64).prop_map(IdxSel::Random),
    ]
}

/// Strategy for generating actions
/// Actions use selectors that are resolved at runtime
fn action_strategy() -> impl Strategy<Value = Action> {
    prop_oneof![
        // Account creation
        2 => (1u128..100).prop_map(|fee| Action::AddUser { fee_payment: fee }),
        1 => (1u128..100).prop_map(|fee| Action::AddLp { fee_payment: fee }),
        // Deposits/Withdrawals
        10 => (idx_sel_strategy(), 0u128..50_000).prop_map(|(who, amount)| Action::Deposit { who, amount }),
        5 => (idx_sel_strategy(), 0u128..50_000).prop_map(|(who, amount)| Action::Withdraw { who, amount }),
        // Time advancement
        5 => (0u64..10).prop_map(|dt| Action::AdvanceSlot { dt }),
        // Funding
        3 => (1u64..50, 100_000u64..10_000_000, -100i64..100).prop_map(|(dt, price, rate)| {
            Action::AccrueFunding { dt, oracle_price: price, rate_bps: rate }
        }),
        // Touch account
        5 => idx_sel_strategy().prop_map(|who| Action::Touch { who }),
        // Trades (LP vs non-LP user)
        8 => (100_000u64..10_000_000, -5_000i128..5_000).prop_map(|(oracle_price, size)| {
            Action::ExecuteTrade { lp: IdxSel::Lp, user: IdxSel::ExistingNonLp, oracle_price, size }
        }),
        // Panic settle
        1 => (100_000u64..10_000_000).prop_map(|price| Action::PanicSettleAll { oracle_price: price }),
        // Force realize
        1 => (100_000u64..10_000_000).prop_map(|price| Action::ForceRealizeLosses { oracle_price: price }),
        // Top up insurance
        2 => (0u128..10_000).prop_map(|amount| Action::TopUpInsurance { amount }),
    ]
}

// ============================================================================
// SECTION 5: STATE MACHINE FUZZER
// ============================================================================

/// State for tracking the fuzzer
struct FuzzState {
    engine: Box<RiskEngine>,
    live_accounts: Vec<u16>,
    lp_idx: Option<u16>,
    account_ids: Vec<u64>, // Track allocated account IDs for uniqueness
    rng_state: u64,        // For deterministic selector resolution
}

impl FuzzState {
    fn new(params: RiskParams) -> Self {
        FuzzState {
            engine: Box::new(RiskEngine::new(params)),
            live_accounts: Vec::new(),
            lp_idx: None,
            account_ids: Vec::new(),
            rng_state: 12345,
        }
    }

    /// Simple deterministic RNG for selector resolution
    fn next_rng(&mut self) -> u64 {
        self.rng_state ^= self.rng_state << 13;
        self.rng_state ^= self.rng_state >> 7;
        self.rng_state ^= self.rng_state << 17;
        self.rng_state
    }

    /// Resolve an index selector to a concrete index
    fn resolve_selector(&mut self, sel: &IdxSel) -> u16 {
        match sel {
            IdxSel::Existing => {
                if self.live_accounts.is_empty() {
                    // Fallback to random
                    (self.next_rng() % 64) as u16
                } else {
                    let idx = self.next_rng() as usize % self.live_accounts.len();
                    self.live_accounts[idx]
                }
            }
            IdxSel::ExistingNonLp => {
                // Single-pass selection to avoid Vec allocation:
                // 1. Count non-LP accounts
                // 2. Pick kth candidate
                let count = self
                    .live_accounts
                    .iter()
                    .filter(|&&x| Some(x) != self.lp_idx)
                    .count();
                if count == 0 {
                    // Fallback to random different from LP
                    let mut idx = (self.next_rng() % 64) as u16;
                    if Some(idx) == self.lp_idx && idx < 63 {
                        idx += 1;
                    }
                    idx
                } else {
                    let k = self.next_rng() as usize % count;
                    self.live_accounts
                        .iter()
                        .copied()
                        .filter(|&x| Some(x) != self.lp_idx)
                        .nth(k)
                        .unwrap_or(0)
                }
            }
            IdxSel::Lp => self.lp_idx.unwrap_or(0),
            IdxSel::Random(idx) => *idx,
        }
    }

    /// Execute an action and verify invariants
    /// Simulates Solana atomicity: clone before, restore on Err, only assert invariants on Ok
    fn execute(&mut self, action: &Action, step: usize) {
        let context = format!("Step {} ({:?})", step, action);

        match action {
            Action::AddUser { fee_payment } => {
                // Snapshot engine and harness state for rollback
                let before = (*self.engine).clone();
                let live_before = self.live_accounts.clone();
                let ids_before = self.account_ids.clone();
                let num_used_before = self.count_used();
                let next_id_before = self.engine.next_account_id;

                let result = self.engine.add_user(*fee_payment);

                match result {
                    Ok(idx) => {
                        // Postconditions for Ok
                        assert!(
                            is_account_used(&self.engine, idx),
                            "{}: account not marked used",
                            context
                        );
                        assert_eq!(
                            self.count_used(),
                            num_used_before + 1,
                            "{}: num_used didn't increment",
                            context
                        );
                        assert_eq!(
                            self.engine.next_account_id,
                            next_id_before + 1,
                            "{}: next_account_id didn't increment",
                            context
                        );

                        // Account ID should be unique
                        let new_id = self.engine.accounts[idx as usize].account_id;
                        assert!(
                            !self.account_ids.contains(&new_id),
                            "{}: duplicate account_id {}",
                            context,
                            new_id
                        );
                        self.account_ids.push(new_id);
                        self.live_accounts.push(idx);
                        assert_global_invariants(&self.engine, &context, DEFAULT_ORACLE);
                    }
                    Err(_) => {
                        // Simulate Solana rollback - restore engine and harness state
                        *self.engine = before;
                        self.live_accounts = live_before;
                        self.account_ids = ids_before;
                    }
                }
            }

            Action::AddLp { fee_payment } => {
                // Snapshot engine and harness state for rollback
                let before = (*self.engine).clone();
                let live_before = self.live_accounts.clone();
                let ids_before = self.account_ids.clone();
                let lp_before = self.lp_idx;
                let num_used_before = self.count_used();

                let result = self.engine.add_lp([0u8; 32], [0u8; 32], *fee_payment);

                match result {
                    Ok(idx) => {
                        assert!(
                            is_account_used(&self.engine, idx),
                            "{}: LP not marked used",
                            context
                        );
                        assert_eq!(
                            self.count_used(),
                            num_used_before + 1,
                            "{}: num_used didn't increment",
                            context
                        );

                        let new_id = self.engine.accounts[idx as usize].account_id;
                        assert!(
                            !self.account_ids.contains(&new_id),
                            "{}: duplicate LP account_id",
                            context
                        );
                        self.account_ids.push(new_id);
                        self.live_accounts.push(idx);
                        if self.lp_idx.is_none() {
                            self.lp_idx = Some(idx);
                        }
                        assert_global_invariants(&self.engine, &context, DEFAULT_ORACLE);
                    }
                    Err(_) => {
                        // Simulate Solana rollback - restore engine and harness state
                        *self.engine = before;
                        self.live_accounts = live_before;
                        self.account_ids = ids_before;
                        self.lp_idx = lp_before;
                    }
                }
            }

            Action::Deposit { who, amount } => {
                let idx = self.resolve_selector(who);
                let before = (*self.engine).clone();
                let vault_before = self.engine.vault;

                let result = self.engine.deposit(idx, *amount, 0);

                match result {
                    Ok(()) => {
                        // vault_after == vault_before + amount
                        assert_eq!(
                            self.engine.vault,
                            vault_before + amount,
                            "{}: vault didn't increase correctly",
                            context
                        );
                        assert_global_invariants(&self.engine, &context, DEFAULT_ORACLE);
                    }
                    Err(_) => {
                        // Simulate Solana rollback
                        *self.engine = before;
                    }
                }
            }

            Action::Withdraw { who, amount } => {
                let idx = self.resolve_selector(who);
                let before = (*self.engine).clone();
                let vault_before = self.engine.vault;

                let result = self.engine.withdraw(idx, *amount, 0, 1_000_000);

                match result {
                    Ok(()) => {
                        // vault_after == vault_before - amount
                        assert_eq!(
                            self.engine.vault,
                            vault_before - amount,
                            "{}: vault didn't decrease correctly",
                            context
                        );
                        assert_global_invariants(&self.engine, &context, DEFAULT_ORACLE);
                    }
                    Err(_) => {
                        // Simulate Solana rollback
                        *self.engine = before;
                    }
                }
            }

            Action::AdvanceSlot { dt } => {
                // advance_slot is infallible - no rollback needed
                let slot_before = self.engine.current_slot;
                self.engine.advance_slot(*dt);
                assert!(
                    self.engine.current_slot >= slot_before,
                    "{}: current_slot went backwards",
                    context
                );
                assert_global_invariants(&self.engine, &context, DEFAULT_ORACLE);
            }

            Action::AccrueFunding {
                dt,
                oracle_price,
                rate_bps,
            } => {
                let before = (*self.engine).clone();
                let last_slot_before = self.engine.last_funding_slot;
                let now_slot = self.engine.current_slot.saturating_add(*dt);

                let result = self
                    .engine
                    .accrue_funding(now_slot, *oracle_price, *rate_bps);

                match result {
                    Ok(()) => {
                        // Only expect last_funding_slot to update if now_slot > old value
                        if now_slot > last_slot_before {
                            assert_eq!(
                                self.engine.last_funding_slot, now_slot,
                                "{}: last_funding_slot not updated",
                                context
                            );
                        }
                        assert_global_invariants(&self.engine, &context, DEFAULT_ORACLE);
                    }
                    Err(_) => {
                        // Simulate Solana rollback
                        *self.engine = before;
                    }
                }
            }

            Action::Touch { who } => {
                let idx = self.resolve_selector(who);
                let before = (*self.engine).clone();

                let result = self.engine.touch_account(idx);

                match result {
                    Ok(()) => {
                        // funding_index should equal global index
                        assert_eq!(
                            self.engine.accounts[idx as usize].funding_index,
                            self.engine.funding_index_qpb_e6,
                            "{}: funding_index not synced",
                            context
                        );
                        assert_global_invariants(&self.engine, &context, DEFAULT_ORACLE);
                    }
                    Err(_) => {
                        // Simulate Solana rollback
                        *self.engine = before;
                    }
                }
            }

            Action::ExecuteTrade {
                lp,
                user,
                oracle_price,
                size,
            } => {
                let lp_idx = self.resolve_selector(lp);
                let user_idx = self.resolve_selector(user);

                // Skip if LP and user are the same account (invalid trade)
                if lp_idx == user_idx {
                    return;
                }

                let before = (*self.engine).clone();

                let result =
                    self.engine
                        .execute_trade(&MATCHER, lp_idx, user_idx, 0, *oracle_price, *size);

                match result {
                    Ok(_) => {
                        // Trade succeeded - positions modified, that's fine
                        assert_global_invariants(&self.engine, &context, DEFAULT_ORACLE);
                    }
                    Err(_) => {
                        // Simulate Solana rollback
                        *self.engine = before;
                    }
                }
            }

            Action::PanicSettleAll { oracle_price } => {
                let before = (*self.engine).clone();

                let result = self.engine.panic_settle_all(*oracle_price);

                match result {
                    Ok(()) => {
                        // risk_reduction_only should be true
                        assert!(
                            self.engine.risk_reduction_only,
                            "{}: risk_reduction_only not set after panic_settle",
                            context
                        );
                        // warmup_paused should be true
                        assert!(
                            self.engine.warmup_paused,
                            "{}: warmup_paused not set after panic_settle",
                            context
                        );
                        // All positions should be 0 - scan ALL used accounts, not just live_accounts
                        let n = account_count(&self.engine);
                        for idx in 0..n {
                            if is_account_used(&self.engine, idx as u16) {
                                assert_eq!(
                                    self.engine.accounts[idx].position_size, 0,
                                    "{}: position not closed for account {}",
                                    context, idx
                                );
                            }
                        }
                        assert_global_invariants(&self.engine, &context, DEFAULT_ORACLE);
                    }
                    Err(_) => {
                        // Simulate Solana rollback
                        *self.engine = before;
                    }
                }
            }

            Action::ForceRealizeLosses { oracle_price } => {
                let before = (*self.engine).clone();

                let result = self.engine.force_realize_losses(*oracle_price);

                match result {
                    Ok(()) => {
                        // risk_reduction_only and warmup_paused should be true
                        assert!(
                            self.engine.risk_reduction_only,
                            "{}: risk_reduction_only not set after force_realize",
                            context
                        );
                        assert!(
                            self.engine.warmup_paused,
                            "{}: warmup_paused not set after force_realize",
                            context
                        );
                        // All positions should be 0 - scan ALL used accounts
                        let n = account_count(&self.engine);
                        for idx in 0..n {
                            if is_account_used(&self.engine, idx as u16) {
                                assert_eq!(
                                    self.engine.accounts[idx].position_size, 0,
                                    "{}: position not closed for account {}",
                                    context, idx
                                );
                            }
                        }
                        assert_global_invariants(&self.engine, &context, DEFAULT_ORACLE);
                    }
                    Err(_) => {
                        // Simulate Solana rollback
                        *self.engine = before;
                    }
                }
            }

            Action::TopUpInsurance { amount } => {
                let before = (*self.engine).clone();
                let vault_before = self.engine.vault;
                let loss_accum_before = self.engine.loss_accum;

                let result = self.engine.top_up_insurance_fund(*amount);

                match result {
                    Ok(exited_risk_mode) => {
                        // vault should increase
                        assert_eq!(
                            self.engine.vault,
                            vault_before + amount,
                            "{}: vault didn't increase",
                            context
                        );
                        // loss_accum should decrease first
                        if loss_accum_before > 0 {
                            assert!(
                                self.engine.loss_accum <= loss_accum_before,
                                "{}: loss_accum didn't decrease",
                                context
                            );
                        }
                        // If exited risk mode, verify conditions
                        if exited_risk_mode {
                            assert!(
                                !self.engine.risk_reduction_only,
                                "{}: still in risk mode after exit",
                                context
                            );
                        }
                        assert_global_invariants(&self.engine, &context, DEFAULT_ORACLE);
                    }
                    Err(_) => {
                        // Simulate Solana rollback
                        *self.engine = before;
                    }
                }
            }
        }
    }

    fn count_used(&self) -> u32 {
        let mut count = 0;
        let n = account_count(&self.engine);
        for i in 0..n {
            if is_account_used(&self.engine, i as u16) {
                count += 1;
            }
        }
        count
    }
}

// State machine proptest
proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn fuzz_state_machine_regime_a(
        initial_insurance in 0u128..50_000,
        actions in prop::collection::vec(action_strategy(), 50..100)
    ) {
        let mut state = FuzzState::new(params_regime_a());

        // Setup: Add initial LP and users
        let lp_result = state.engine.add_lp([0u8; 32], [0u8; 32], 1);
        if let Ok(idx) = lp_result {
            state.live_accounts.push(idx);
            state.lp_idx = Some(idx);
            state.account_ids.push(state.engine.accounts[idx as usize].account_id);
        }

        for _ in 0..2 {
            if let Ok(idx) = state.engine.add_user(1) {
                state.live_accounts.push(idx);
                state.account_ids.push(state.engine.accounts[idx as usize].account_id);
            }
        }

        // Initial deposits
        for &idx in &state.live_accounts.clone() {
            let _ = state.engine.deposit(idx, 10_000, 0);
        }

        // Top up insurance using proper API (maintains conservation)
        let current_insurance = state.engine.insurance_fund.balance;
        if initial_insurance > current_insurance {
            let _ = state.engine.top_up_insurance_fund(initial_insurance - current_insurance);
        }

        // Execute actions - selectors resolved at runtime against live state
        for (step, action) in actions.iter().enumerate() {
            state.execute(action, step);
        }
    }

    #[test]
    fn fuzz_state_machine_regime_b(
        initial_insurance in 1000u128..50_000, // Above floor
        actions in prop::collection::vec(action_strategy(), 50..100)
    ) {
        let mut state = FuzzState::new(params_regime_b());

        // Setup: Add initial LP and users
        let lp_result = state.engine.add_lp([0u8; 32], [0u8; 32], 1);
        if let Ok(idx) = lp_result {
            state.live_accounts.push(idx);
            state.lp_idx = Some(idx);
            state.account_ids.push(state.engine.accounts[idx as usize].account_id);
        }

        for _ in 0..2 {
            if let Ok(idx) = state.engine.add_user(1) {
                state.live_accounts.push(idx);
                state.account_ids.push(state.engine.accounts[idx as usize].account_id);
            }
        }

        // Initial deposits
        for &idx in &state.live_accounts.clone() {
            let _ = state.engine.deposit(idx, 10_000, 0);
        }

        // Top up insurance using proper API (maintains conservation)
        let floor = state.engine.params.risk_reduction_threshold;
        let target_insurance = initial_insurance.max(floor + 100);
        let current_insurance = state.engine.insurance_fund.balance;
        if target_insurance > current_insurance {
            let _ = state.engine.top_up_insurance_fund(target_insurance - current_insurance);
        }

        // Execute actions
        for (step, action) in actions.iter().enumerate() {
            state.execute(action, step);
        }
    }
}

// ============================================================================
// SECTION 6: UNIT PROPERTY FUZZ TESTS (FOCUSED)
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    // 1. withdrawable_pnl monotone in slot for positive pnl
    #[test]
    fn fuzz_prop_withdrawable_monotone(
        pnl in 1i128..100_000,
        slope in 1u128..10_000,
        slot1 in 0u64..500,
        slot2 in 0u64..500
    ) {
        let mut engine = Box::new(RiskEngine::new(params_regime_a()));
        let user_idx = engine.add_user(1).unwrap();

        engine.accounts[user_idx as usize].pnl = pnl;
        engine.accounts[user_idx as usize].warmup_slope_per_step = slope;
        engine.accounts[user_idx as usize].warmup_started_at_slot = 0;

        let earlier = slot1.min(slot2);
        let later = slot1.max(slot2);

        engine.current_slot = earlier;
        let w1 = engine.withdrawable_pnl(&engine.accounts[user_idx as usize]);

        engine.current_slot = later;
        let w2 = engine.withdrawable_pnl(&engine.accounts[user_idx as usize]);

        prop_assert!(w2 >= w1, "Withdrawable not monotone: {} -> {} at slots {} -> {}",
                     w1, w2, earlier, later);
    }

    // 2. withdrawable_pnl == 0 if pnl<=0 or slope==0 or elapsed==0
    #[test]
    fn fuzz_prop_withdrawable_zero_conditions(
        principal in 0u128..100_000,
        pnl in -100_000i128..0, // Non-positive PnL
        slope in 0u128..10_000,
        slot in 0u64..500
    ) {
        let mut engine = Box::new(RiskEngine::new(params_regime_a()));
        let user_idx = engine.add_user(1).unwrap();

        engine.accounts[user_idx as usize].capital = principal;
        engine.accounts[user_idx as usize].pnl = pnl;
        engine.accounts[user_idx as usize].warmup_slope_per_step = slope;
        engine.accounts[user_idx as usize].warmup_started_at_slot = 0;
        engine.current_slot = slot;

        let withdrawable = engine.withdrawable_pnl(&engine.accounts[user_idx as usize]);

        // If pnl <= 0, withdrawable must be 0
        if pnl <= 0 {
            prop_assert_eq!(withdrawable, 0, "Withdrawable should be 0 for non-positive pnl");
        }
    }

    #[test]
    fn fuzz_prop_withdrawable_zero_slope(
        pnl in 1i128..100_000,
        slot in 1u64..500
    ) {
        let mut engine = Box::new(RiskEngine::new(params_regime_a()));
        let user_idx = engine.add_user(1).unwrap();

        engine.accounts[user_idx as usize].pnl = pnl;
        engine.accounts[user_idx as usize].warmup_slope_per_step = 0; // Zero slope
        engine.accounts[user_idx as usize].warmup_started_at_slot = 0;
        engine.current_slot = slot;

        let withdrawable = engine.withdrawable_pnl(&engine.accounts[user_idx as usize]);
        prop_assert_eq!(withdrawable, 0, "Withdrawable should be 0 for zero slope");
    }

    // 3. warmup_paused freezes progress
    #[test]
    fn fuzz_prop_warmup_pause_freezes(
        pnl in 1i128..10_000,
        slope in 1u128..1000,
        pause_slot in 1u64..100,
        extra_slots in 1u64..200
    ) {
        let mut engine = Box::new(RiskEngine::new(params_regime_a()));
        let user_idx = engine.add_user(1).unwrap();

        engine.accounts[user_idx as usize].pnl = pnl;
        engine.accounts[user_idx as usize].warmup_slope_per_step = slope;
        engine.accounts[user_idx as usize].warmup_started_at_slot = 0;

        // Pause at pause_slot
        engine.warmup_paused = true;
        engine.warmup_pause_slot = pause_slot;

        // Get withdrawable at pause_slot
        engine.current_slot = pause_slot;
        let w_at_pause = engine.withdrawable_pnl(&engine.accounts[user_idx as usize]);

        // Get withdrawable after more time passes
        engine.current_slot = pause_slot + extra_slots;
        let w_after_pause = engine.withdrawable_pnl(&engine.accounts[user_idx as usize]);

        prop_assert_eq!(w_at_pause, w_after_pause,
                        "Withdrawable should not increase after pause");
    }

    // 4. settle_warmup_to_capital idempotent at same slot
    #[test]
    fn fuzz_prop_settle_idempotent(
        capital in 100u128..10_000,
        pnl in 1i128..5_000,
        slope in 1u128..1000,
        slot in 1u64..200
    ) {
        let mut engine = Box::new(RiskEngine::new(params_regime_b()));
        let user_idx = engine.add_user(1).unwrap();

        engine.insurance_fund.balance = 100_000;
        engine.vault = 100_000;
        engine.deposit(user_idx, capital, 0).unwrap();
        engine.accounts[user_idx as usize].pnl = pnl;
        engine.accounts[user_idx as usize].warmup_slope_per_step = slope;
        engine.accounts[user_idx as usize].warmup_started_at_slot = 0;
        engine.current_slot = slot;

        // First settlement
        let _ = engine.settle_warmup_to_capital(user_idx);
        let state1 = (
            engine.accounts[user_idx as usize].capital,
            engine.accounts[user_idx as usize].pnl,
            engine.warmed_pos_total,
            engine.warmed_neg_total,
            engine.warmup_insurance_reserved,
        );

        // Second settlement at same slot
        let _ = engine.settle_warmup_to_capital(user_idx);
        let state2 = (
            engine.accounts[user_idx as usize].capital,
            engine.accounts[user_idx as usize].pnl,
            engine.warmed_pos_total,
            engine.warmed_neg_total,
            engine.warmup_insurance_reserved,
        );

        prop_assert_eq!(state1, state2, "Settlement should be idempotent");
    }

    // 5. settle_warmup_to_capital: warmed totals monotone non-decreasing
    #[test]
    fn fuzz_prop_warmed_totals_monotone(
        capital in 100u128..10_000,
        pnl in 1i128..5_000,
        slope in 1u128..1000,
        slots in prop::collection::vec(1u64..50, 1..5)
    ) {
        let mut engine = Box::new(RiskEngine::new(params_regime_b()));
        let user_idx = engine.add_user(1).unwrap();

        engine.insurance_fund.balance = 100_000;
        engine.vault = 100_000;
        engine.deposit(user_idx, capital, 0).unwrap();
        engine.accounts[user_idx as usize].pnl = pnl;
        engine.accounts[user_idx as usize].warmup_slope_per_step = slope;
        engine.accounts[user_idx as usize].warmup_started_at_slot = 0;

        let mut prev_pos = engine.warmed_pos_total;
        let mut prev_neg = engine.warmed_neg_total;
        let mut current = 0u64;

        for &dt in &slots {
            current += dt;
            engine.current_slot = current;
            let _ = engine.settle_warmup_to_capital(user_idx);

            prop_assert!(engine.warmed_pos_total >= prev_pos,
                         "warmed_pos_total decreased");
            prop_assert!(engine.warmed_neg_total >= prev_neg,
                         "warmed_neg_total decreased");

            prev_pos = engine.warmed_pos_total;
            prev_neg = engine.warmed_neg_total;
        }
    }

    // 6. apply_adl never changes any capital
    #[test]
    fn fuzz_prop_adl_preserves_capital(
        capitals in prop::collection::vec(0u128..50_000, 2..5),
        pnls in prop::collection::vec(-10_000i128..10_000, 2..5),
        loss in 0u128..20_000
    ) {
        let mut engine = Box::new(RiskEngine::new(params_regime_a()));

        let mut indices = Vec::new();
        for i in 0..capitals.len().min(pnls.len()) {
            let idx = if i == 0 {
                engine.add_lp([0u8; 32], [0u8; 32], 1).unwrap()
            } else {
                engine.add_user(1).unwrap()
            };
            engine.accounts[idx as usize].capital = capitals[i];
            engine.accounts[idx as usize].pnl = pnls[i];
            indices.push(idx);
        }

        engine.insurance_fund.balance = 100_000;
        engine.vault = capitals.iter().sum::<u128>() + 100_000;

        let capitals_before: Vec<_> = indices
            .iter()
            .map(|&idx| engine.accounts[idx as usize].capital)
            .collect();

        let _ = engine.apply_adl(loss);

        for (i, &idx) in indices.iter().enumerate() {
            prop_assert_eq!(
                engine.accounts[idx as usize].capital,
                capitals_before[i],
                "ADL changed capital for account {}",
                idx
            );
        }
    }

    // 7. touch_account idempotent if global index unchanged
    #[test]
    fn fuzz_prop_touch_idempotent(
        position in -100_000i128..100_000,
        pnl in -50_000i128..50_000,
        funding_delta in -1_000_000i128..1_000_000
    ) {
        let mut engine = Box::new(RiskEngine::new(params_regime_a()));
        let user_idx = engine.add_user(1).unwrap();

        engine.accounts[user_idx as usize].position_size = position;
        engine.accounts[user_idx as usize].pnl = pnl;
        engine.funding_index_qpb_e6 = funding_delta;

        // First touch
        let _ = engine.touch_account(user_idx);
        let state1 = (
            engine.accounts[user_idx as usize].pnl,
            engine.accounts[user_idx as usize].funding_index,
        );

        // Second touch without changing global index
        let _ = engine.touch_account(user_idx);
        let state2 = (
            engine.accounts[user_idx as usize].pnl,
            engine.accounts[user_idx as usize].funding_index,
        );

        prop_assert_eq!(state1, state2, "Touch should be idempotent");
    }

    // 8. accrue_funding with dt=0 is no-op
    #[test]
    fn fuzz_prop_funding_zero_dt_noop(
        price in 100_000u64..10_000_000,
        rate in -1000i64..1000
    ) {
        let mut engine = Box::new(RiskEngine::new(params_regime_a()));

        let index_before = engine.funding_index_qpb_e6;
        let slot_before = engine.last_funding_slot;

        // Accrue with same slot (dt=0)
        let _ = engine.accrue_funding(slot_before, price, rate);

        prop_assert_eq!(engine.funding_index_qpb_e6, index_before,
                        "Funding index changed with dt=0");
    }

    // 9. Collateral calculation is consistent
    #[test]
    fn fuzz_prop_collateral_calculation(
        capital in 0u128..100_000,
        pnl in -50_000i128..50_000
    ) {
        let mut engine = Box::new(RiskEngine::new(params_regime_a()));
        let user_idx = engine.add_user(1).unwrap();

        engine.accounts[user_idx as usize].capital = capital;
        engine.accounts[user_idx as usize].pnl = pnl;

        let collateral = engine.account_collateral(&engine.accounts[user_idx as usize]);

        // Collateral = capital + max(0, pnl)
        let expected = if pnl >= 0 {
            capital.saturating_add(pnl as u128)
        } else {
            capital
        };

        prop_assert_eq!(collateral, expected,
                        "Collateral calculation incorrect: got {}, expected {}",
                        collateral, expected);
    }

    // 10. add_user/add_lp fails when at max capacity
    #[test]
    fn fuzz_prop_add_fails_at_capacity(num_to_add in 1usize..10) {
        let mut params = params_regime_a();
        params.max_accounts = 4; // Very small
        let mut engine = Box::new(RiskEngine::new(params));

        // Fill up
        for _ in 0..4 {
            let _ = engine.add_user(1);
        }

        // Additional adds should fail
        for _ in 0..num_to_add {
            let result = engine.add_user(1);
            prop_assert!(result.is_err(), "add_user should fail at capacity");
        }
    }

    // 11. Zero position pays no funding
    #[test]
    fn fuzz_prop_zero_position_no_funding(
        pnl in -100_000i128..100_000,
        funding_delta in -10_000_000i128..10_000_000
    ) {
        let mut engine = Box::new(RiskEngine::new(params_regime_a()));
        let user_idx = engine.add_user(1).unwrap();

        engine.accounts[user_idx as usize].position_size = 0;
        engine.accounts[user_idx as usize].pnl = pnl;
        engine.funding_index_qpb_e6 = funding_delta;

        let _ = engine.touch_account(user_idx);

        prop_assert_eq!(engine.accounts[user_idx as usize].pnl, pnl,
                        "Zero position should not pay funding");
    }

    // 12. Funding is zero-sum between opposite positions
    #[test]
    fn fuzz_prop_funding_zero_sum(
        position in 1i128..100_000,
        funding_delta in -1_000_000i128..1_000_000
    ) {
        let mut engine = Box::new(RiskEngine::new(params_regime_a()));
        let user_idx = engine.add_user(1).unwrap();
        let lp_idx = engine.add_lp([0u8; 32], [0u8; 32], 1).unwrap();

        // Opposite positions
        engine.accounts[user_idx as usize].position_size = position;
        engine.accounts[lp_idx as usize].position_size = -position;

        let total_pnl_before = engine.accounts[user_idx as usize].pnl
            + engine.accounts[lp_idx as usize].pnl;

        engine.funding_index_qpb_e6 = funding_delta;

        let _ = engine.touch_account(user_idx);
        let _ = engine.touch_account(lp_idx);

        let total_pnl_after = engine.accounts[user_idx as usize].pnl
            + engine.accounts[lp_idx as usize].pnl;

        // Funding payments round UP when account pays, so total PNL may decrease
        // (vault keeps rounding dust). This ensures one-sided conservation slack.
        // The change should never be positive (no value created from thin air).
        let change = total_pnl_after - total_pnl_before;
        prop_assert!(change <= 0,
                     "Funding should not create value: change={}", change);
        // The absolute change should be bounded by rounding (at most 2 per account pair)
        prop_assert!(change >= -2,
                     "Funding change should be bounded: change={}", change);
    }
}

// ============================================================================
// SECTION 7: DETERMINISTIC SEEDED FUZZER
// ============================================================================

/// xorshift64 PRNG for deterministic randomness
struct Rng {
    state: u64,
}

impl Rng {
    fn new(seed: u64) -> Self {
        Rng {
            state: if seed == 0 { 1 } else { seed },
        }
    }

    fn next(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    fn u64(&mut self, lo: u64, hi: u64) -> u64 {
        if lo >= hi {
            return lo;
        }
        lo + (self.next() % (hi - lo + 1))
    }

    fn u128(&mut self, lo: u128, hi: u128) -> u128 {
        if lo >= hi {
            return lo;
        }
        lo + ((self.next() as u128) % (hi - lo + 1))
    }

    fn i128(&mut self, lo: i128, hi: i128) -> i128 {
        if lo >= hi {
            return lo;
        }
        // Avoid overflow: use u64 directly and cast safely
        let range = (hi - lo + 1) as u128;
        lo + ((self.next() as u128 % range) as i128)
    }

    fn i64(&mut self, lo: i64, hi: i64) -> i64 {
        if lo >= hi {
            return lo;
        }
        // Avoid overflow: use u64 directly and cast safely
        let range = (hi - lo + 1) as u64;
        lo + ((self.next() % range) as i64)
    }

    fn usize(&mut self, lo: usize, hi: usize) -> usize {
        if lo >= hi {
            return lo;
        }
        lo + ((self.next() as usize) % (hi - lo + 1))
    }
}

/// Generate a random selector using RNG
fn random_selector(rng: &mut Rng) -> IdxSel {
    match rng.usize(0, 3) {
        0 => IdxSel::Existing,
        1 => IdxSel::ExistingNonLp,
        2 => IdxSel::Lp,
        _ => IdxSel::Random(rng.u64(0, 63) as u16),
    }
}

/// Generate a random action using the RNG (selector-based)
fn random_action(rng: &mut Rng) -> (Action, String) {
    // Note: ApplyAdl removed - it's internal and tested via settlement ops
    let action_type = rng.usize(0, 10);

    let action = match action_type {
        0 => Action::AddUser {
            fee_payment: rng.u128(1, 100),
        },
        1 => Action::AddLp {
            fee_payment: rng.u128(1, 100),
        },
        2 => Action::Deposit {
            who: random_selector(rng),
            amount: rng.u128(0, 50_000),
        },
        3 => Action::Withdraw {
            who: random_selector(rng),
            amount: rng.u128(0, 50_000),
        },
        4 => Action::AdvanceSlot { dt: rng.u64(0, 10) },
        5 => Action::AccrueFunding {
            dt: rng.u64(1, 50),
            oracle_price: rng.u64(100_000, 10_000_000),
            rate_bps: rng.i64(-100, 100),
        },
        6 => Action::Touch {
            who: random_selector(rng),
        },
        7 => Action::ExecuteTrade {
            lp: IdxSel::Lp,
            user: IdxSel::ExistingNonLp,
            oracle_price: rng.u64(100_000, 10_000_000),
            size: rng.i128(-5_000, 5_000),
        },
        8 => Action::PanicSettleAll {
            oracle_price: rng.u64(100_000, 10_000_000),
        },
        9 => Action::ForceRealizeLosses {
            oracle_price: rng.u64(100_000, 10_000_000),
        },
        _ => Action::TopUpInsurance {
            amount: rng.u128(0, 10_000),
        },
    };

    let desc = format!("{:?}", action);
    (action, desc)
}

/// Compute conservation slack without panicking
fn compute_conservation_slack(engine: &RiskEngine) -> (i128, u128, i128, u128, u128) {
    let mut total_capital = 0u128;
    let mut net_settled_pnl: i128 = 0;
    let global_index = engine.funding_index_qpb_e6;

    let n = account_count(engine);
    for i in 0..n {
        if is_account_used(engine, i as u16) {
            let acc = &engine.accounts[i];
            total_capital += acc.capital;

            // Compute settled PNL using shared helper (matches engine rounding)
            let mut settled_pnl = acc.pnl;
            if acc.position_size != 0 {
                let delta_f = global_index.saturating_sub(acc.funding_index);
                if delta_f != 0 {
                    let payment = funding_payment(acc.position_size, delta_f);
                    settled_pnl = settled_pnl.saturating_sub(payment);
                }
            }
            net_settled_pnl = net_settled_pnl.saturating_add(settled_pnl);
        }
    }
    let base = total_capital + engine.insurance_fund.balance;
    let expected = if net_settled_pnl >= 0 {
        base + net_settled_pnl as u128
    } else {
        base.saturating_sub((-net_settled_pnl) as u128)
    };
    let actual = engine.vault + engine.loss_accum;
    let slack = actual as i128 - expected as i128;
    (
        slack,
        total_capital,
        net_settled_pnl,
        engine.insurance_fund.balance,
        actual,
    )
}

/// Run deterministic fuzzer for a single regime
fn run_deterministic_fuzzer(
    params: RiskParams,
    regime_name: &str,
    seeds: std::ops::Range<u64>,
    steps: usize,
) {
    for seed in seeds {
        let mut rng = Rng::new(seed);
        let mut state = FuzzState::new(params.clone());

        // Track last N actions for repro
        let mut action_history: Vec<String> = Vec::with_capacity(10);

        // Setup: create LP and 2 users
        if let Ok(idx) = state.engine.add_lp([0u8; 32], [0u8; 32], 1) {
            state.live_accounts.push(idx);
            state.lp_idx = Some(idx);
            state
                .account_ids
                .push(state.engine.accounts[idx as usize].account_id);
        }

        for _ in 0..2 {
            if let Ok(idx) = state.engine.add_user(1) {
                state.live_accounts.push(idx);
                state
                    .account_ids
                    .push(state.engine.accounts[idx as usize].account_id);
            }
        }

        // Initial deposits
        for &idx in &state.live_accounts.clone() {
            let _ = state.engine.deposit(idx, rng.u128(5_000, 50_000), 0);
        }

        // Top up insurance using proper API (maintains conservation)
        let floor = state.engine.params.risk_reduction_threshold;
        let target_ins = floor + rng.u128(5_000, 100_000);
        let current_ins = state.engine.insurance_fund.balance;
        if target_ins > current_ins {
            let _ = state.engine.top_up_insurance_fund(target_ins - current_ins);
        }

        // Verify conservation after setup
        if !state.engine.check_conservation(DEFAULT_ORACLE) {
            eprintln!("Conservation failed after setup for seed {}", seed);
            eprintln!(
                "  vault={}, insurance={}",
                state.engine.vault, state.engine.insurance_fund.balance
            );
            eprintln!("  live_accounts={:?}", state.live_accounts);
            let mut total_cap = 0u128;
            for &idx in &state.live_accounts {
                eprintln!(
                    "  account[{}]: capital={}",
                    idx, state.engine.accounts[idx as usize].capital
                );
                total_cap += state.engine.accounts[idx as usize].capital;
            }
            eprintln!("  total_capital={}", total_cap);
            panic!("Conservation failed after setup");
        }

        // Track slack before starting
        let mut _last_slack: i128 = 0;
        let verbose = false; // Disable verbose for now

        // Run steps
        for step in 0..steps {
            let (slack_before, _, _, _, _) = compute_conservation_slack(&state.engine);
            // Use selector-based random_action (no live/lp args needed)
            let (action, desc) = random_action(&mut rng);

            // Keep last 10 actions
            if action_history.len() >= 10 {
                action_history.remove(0);
            }
            action_history.push(desc.clone());

            // Execute with panic catching for better error messages
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                state.execute(&action, step);
            }));

            // Track slack changes
            let (slack_after, total_cap, net_pnl, ins, actual) =
                compute_conservation_slack(&state.engine);
            let slack_delta = slack_after - slack_before;
            if verbose && slack_delta != 0 {
                eprintln!(
                    "Step {}: {} -> slack delta={}, total slack={} (cap={}, pnl={}, ins={}, actual={})",
                    step, desc, slack_delta, slack_after, total_cap, net_pnl, ins, actual
                );
            }
            _last_slack = slack_after;

            if result.is_err() {
                eprintln!("\n=== DETERMINISTIC FUZZER FAILURE ===");
                eprintln!("Regime: {}", regime_name);
                eprintln!("Seed: {}", seed);
                eprintln!("Step: {}", step);
                eprintln!("Action: {}", desc);
                eprintln!("Slack before: {}, after: {}", slack_before, slack_after);
                eprintln!("\nLast 10 actions:");
                for (i, act) in action_history.iter().enumerate() {
                    eprintln!("  {}: {}", step.saturating_sub(9) + i, act);
                }
                eprintln!(
                    "\nTo reproduce: run with seed={}, stop at step={}",
                    seed, step
                );
                panic!("Deterministic fuzzer failed - see above for repro");
            }
            // Note: live_accounts tracking is now handled inside execute() via the returned idx
            // when AddUser/AddLp succeeds. No need for separate tracking here.
        }
    }
}

#[test]
fn fuzz_deterministic_regime_a() {
    run_deterministic_fuzzer(params_regime_a(), "A (floor=0)", 1..501, 200);
}

#[test]
fn fuzz_deterministic_regime_b() {
    run_deterministic_fuzzer(params_regime_b(), "B (floor=1000)", 1..501, 200);
}

// Extended deterministic test with more seeds
#[test]
#[ignore] // Run with: cargo test --features fuzz fuzz_deterministic_extended -- --ignored
fn fuzz_deterministic_extended() {
    run_deterministic_fuzzer(params_regime_a(), "A extended", 1..2001, 500);
    run_deterministic_fuzzer(params_regime_b(), "B extended", 1..2001, 500);
}

// ============================================================================
// SECTION 8: LEGACY PROPTEST TESTS (PRESERVED FROM ORIGINAL)
// ============================================================================

// Strategy helpers
fn amount_strategy() -> impl Strategy<Value = u128> {
    0u128..1_000_000
}

fn position_strategy() -> impl Strategy<Value = i128> {
    -100_000i128..100_000
}

proptest! {
    // Test that deposit always increases vault and principal
    #[test]
    fn fuzz_deposit_increases_balance(amount in amount_strategy()) {
        let mut engine = Box::new(RiskEngine::new(params_regime_a()));
        let user_idx = engine.add_user(1).unwrap();

        let vault_before = engine.vault;
        let principal_before = engine.accounts[user_idx as usize].capital;

        let _ = engine.deposit(user_idx, amount, 0);

        prop_assert_eq!(engine.vault, vault_before + amount);
        prop_assert_eq!(engine.accounts[user_idx as usize].capital, principal_before + amount);
    }

    // Test that withdrawal never increases balance (uses Solana rollback simulation on Err)
    #[test]
    fn fuzz_withdraw_decreases_or_fails(
        deposit_amount in amount_strategy(),
        withdraw_amount in amount_strategy()
    ) {
        let mut engine = Box::new(RiskEngine::new(params_regime_a()));
        let user_idx = engine.add_user(1).unwrap();

        engine.deposit(user_idx, deposit_amount, 0).unwrap();

        // Snapshot for rollback simulation
        let before = (*engine).clone();

        let result = engine.withdraw(user_idx, withdraw_amount, 0, 1_000_000);

        if result.is_ok() {
            prop_assert!(engine.vault <= before.vault);
            prop_assert!(engine.accounts[user_idx as usize].capital <= before.accounts[user_idx as usize].capital);
        } else {
            // Simulate Solana rollback then verify state is restored
            *engine = before.clone();
            prop_assert_eq!(engine.vault, before.vault);
            prop_assert_eq!(engine.accounts[user_idx as usize].capital, before.accounts[user_idx as usize].capital);
        }
    }

    // Test conservation after operations
    #[test]
    fn fuzz_conservation_after_operations(
        deposits in prop::collection::vec(amount_strategy(), 1..10),
        withdrawals in prop::collection::vec(amount_strategy(), 1..10)
    ) {
        let mut engine = Box::new(RiskEngine::new(params_regime_a()));
        let user_idx = engine.add_user(1).unwrap();

        for amount in deposits {
            let _ = engine.deposit(user_idx, amount, 0);
        }

        prop_assert!(engine.check_conservation(DEFAULT_ORACLE));

        for amount in withdrawals {
            let _ = engine.withdraw(user_idx, amount, 0, 1_000_000);
        }

        prop_assert!(engine.check_conservation(DEFAULT_ORACLE));
    }

    // Test funding idempotence
    #[test]
    fn fuzz_funding_idempotence(
        position in position_strategy(),
        index_delta in -1_000_000i128..1_000_000
    ) {
        let mut engine = Box::new(RiskEngine::new(params_regime_a()));
        let user_idx = engine.add_user(1).unwrap();

        engine.accounts[user_idx as usize].position_size = position;
        engine.funding_index_qpb_e6 = index_delta;

        let _ = engine.touch_account(user_idx);
        let pnl_first = engine.accounts[user_idx as usize].pnl;

        let _ = engine.touch_account(user_idx);
        let pnl_second = engine.accounts[user_idx as usize].pnl;

        prop_assert_eq!(pnl_first, pnl_second, "Funding settlement should be idempotent");
    }

    // Test funding preserves principal
    #[test]
    fn fuzz_funding_preserves_principal(
        principal in amount_strategy(),
        position in position_strategy(),
        funding_delta in -10_000_000i128..10_000_000
    ) {
        let mut engine = Box::new(RiskEngine::new(params_regime_a()));
        let user_idx = engine.add_user(1).unwrap();

        engine.accounts[user_idx as usize].capital = principal;
        engine.accounts[user_idx as usize].position_size = position;
        engine.funding_index_qpb_e6 = funding_delta;

        let _ = engine.touch_account(user_idx);

        prop_assert_eq!(engine.accounts[user_idx as usize].capital, principal,
                       "Funding must never modify principal");
    }

    // Test ADL insurance failover
    #[test]
    fn fuzz_adl_insurance_failover(
        user_pnl in 0i128..10_000,
        insurance_balance in 0u128..5_000,
        loss in 5_000u128..20_000
    ) {
        let mut engine = Box::new(RiskEngine::new(params_regime_a()));
        let user_idx = engine.add_user(1).unwrap();

        engine.accounts[user_idx as usize].pnl = user_pnl;
        engine.insurance_fund.balance = insurance_balance;

        let _ = engine.apply_adl(loss);

        let total_available = (user_pnl as u128) + insurance_balance;
        if loss > total_available {
            prop_assert!(engine.loss_accum > 0);
        }
    }

    // Conservation after panic settle
    #[test]
    fn fuzz_conservation_after_panic_settle(
        user_capital in 1000u128..100_000,
        lp_capital in 1000u128..100_000,
        position in 1i128..10_000,
        entry_price in 100_000u64..10_000_000,
        oracle_price in 100_000u64..10_000_000,
        insurance in 0u128..10_000
    ) {
        let mut engine = Box::new(RiskEngine::new(params_regime_a()));
        let user_idx = engine.add_user(1).unwrap();
        let lp_idx = engine.add_lp([0u8; 32], [0u8; 32], 1).unwrap();

        engine.deposit(user_idx, user_capital, 0).unwrap();
        engine.deposit(lp_idx, lp_capital, 0).unwrap();

        engine.accounts[user_idx as usize].position_size = position;
        engine.accounts[user_idx as usize].entry_price = entry_price;
        engine.accounts[lp_idx as usize].position_size = -position;
        engine.accounts[lp_idx as usize].entry_price = entry_price;

        let total_capital = user_capital + lp_capital;
        engine.insurance_fund.balance = insurance;
        engine.vault = total_capital + insurance;

        prop_assert!(engine.check_conservation(DEFAULT_ORACLE), "Before panic_settle");

        let _ = engine.panic_settle_all(oracle_price);

        prop_assert!(engine.check_conservation(DEFAULT_ORACLE), "After panic_settle");

        prop_assert_eq!(engine.accounts[user_idx as usize].position_size, 0);
        prop_assert_eq!(engine.accounts[lp_idx as usize].position_size, 0);
    }
}

// ============================================================================
// SECTION 9: CONSERVATION REGRESSION TESTS
// These verify that conservation invariant holds under various conditions
// ============================================================================

/// Verify panic_settle_all preserves conservation with lazy funding
/// Conservation uses settled_pnl which accounts for unsettled funding payments
#[test]
fn panic_settle_preserves_conservation_with_lazy_funding() {
    let mut engine = Box::new(RiskEngine::new(params_regime_a()));

    // Create LP and user with positions
    let lp_idx = engine.add_lp([0u8; 32], [0u8; 32], 1).unwrap();
    let user_idx = engine.add_user(1).unwrap();
    engine.deposit(lp_idx, 100_000, 0).unwrap();
    engine.deposit(user_idx, 100_000, 0).unwrap();

    // Execute a trade to create positions
    engine
        .execute_trade(&MATCHER, lp_idx, user_idx, 1_000_000, 1000)
        .unwrap();

    // Accrue significant funding WITHOUT touching accounts
    engine.accrue_funding(1000, 1_000_000, 1000).unwrap();

    // Verify conservation holds before panic settle (uses settled_pnl)
    assert!(
        engine.check_conservation(DEFAULT_ORACLE),
        "Conservation should hold before panic_settle"
    );

    // Panic settle
    engine.panic_settle_all(1_000_000).unwrap();

    // Verify conservation still holds
    assert!(
        engine.check_conservation(DEFAULT_ORACLE),
        "Conservation must hold after panic_settle"
    );

    // All positions should be closed
    assert_eq!(engine.accounts[user_idx as usize].position_size, 0);
    assert_eq!(engine.accounts[lp_idx as usize].position_size, 0);
}

/// Verify check_conservation uses settled_pnl (accounts for lazy funding)
/// This prevents "docs drift" - ensures engine matches documented formula
#[test]
fn conservation_uses_settled_pnl_regression() {
    let mut engine = Box::new(RiskEngine::new(params_regime_a()));

    // Create LP and user with positions
    let lp_idx = engine.add_lp([0u8; 32], [0u8; 32], 1).unwrap();
    let user_idx = engine.add_user(1).unwrap();
    engine.deposit(lp_idx, 100_000, 0).unwrap();
    engine.deposit(user_idx, 100_000, 0).unwrap();

    // Execute trade to create positions
    engine
        .execute_trade(&MATCHER, lp_idx, user_idx, 0, 1_000_000, 1000)
        .unwrap();

    // Accrue significant funding WITHOUT touching accounts
    // This creates a gap between account.pnl and settled_pnl
    engine.accrue_funding(1000, 1_000_000, 500).unwrap();

    // Manually compute conservation using settled_pnl formula
    let global_index = engine.funding_index_qpb_e6;
    let mut total_capital = 0u128;
    let mut net_settled_pnl: i128 = 0;

    for i in 0..account_count(&engine) {
        if is_account_used(&engine, i as u16) {
            let acc = &engine.accounts[i];
            total_capital += acc.capital;

            // Compute settled PNL using shared helper (matches engine rounding)
            let mut settled_pnl = acc.pnl;
            if acc.position_size != 0 {
                let delta_f = global_index.saturating_sub(acc.funding_index);
                if delta_f != 0 {
                    let payment = funding_payment(acc.position_size, delta_f);
                    settled_pnl = settled_pnl.saturating_sub(payment);
                }
            }
            net_settled_pnl = net_settled_pnl.saturating_add(settled_pnl);
        }
    }

    // Compute expected: sum(capital) + sum(settled_pnl) + insurance
    let base = total_capital + engine.insurance_fund.balance;
    let expected = if net_settled_pnl >= 0 {
        base + (net_settled_pnl as u128)
    } else {
        base.saturating_sub((-net_settled_pnl) as u128)
    };

    // Compute actual: vault + loss_accum
    let actual = engine.vault + engine.loss_accum;

    // Verify our manual computation matches engine's check
    assert!(
        engine.check_conservation(DEFAULT_ORACLE),
        "check_conservation failed: actual={}, expected={}, diff={}",
        actual,
        expected,
        (actual as i128) - (expected as i128)
    );

    // Also verify our formula matches (within rounding tolerance)
    let diff = if actual >= expected {
        actual - expected
    } else {
        expected - actual
    };
    assert!(
        diff <= 100, // MAX_ROUNDING_SLACK is typically small
        "Manual settled_pnl formula doesn't match engine: actual={}, expected={}, diff={}",
        actual,
        expected,
        diff
    );
}

/// Verify the test harness correctly simulates Solana atomicity
/// When an operation returns Err, the harness must restore the engine to pre-call state
/// This ensures the fuzz suite accurately models on-chain behavior
#[test]
fn harness_rollback_simulation_test() {
    let mut engine = Box::new(RiskEngine::new(params_regime_a()));

    // Create user with some capital
    let user_idx = engine.add_user(1).unwrap();
    engine.deposit(user_idx, 1000, 0).unwrap();

    // Accrue some funding to create state that could be mutated
    engine.accrue_funding(100, 1_000_000, 100).unwrap();

    // Capture complete state before failed operation (deep clone of RiskEngine)
    let before = (*engine).clone();

    // Capture expected values before any operation
    let expected_vault = engine.vault;
    let expected_capital = engine.accounts[user_idx as usize].capital;
    let expected_pnl = engine.accounts[user_idx as usize].pnl;
    let expected_funding_index = engine.accounts[user_idx as usize].funding_index;
    let expected_warmed_pos = engine.warmed_pos_total;
    let expected_warmed_neg = engine.warmed_neg_total;
    let expected_warmup_reserved = engine.warmup_insurance_reserved;

    // Try to withdraw more than available - will fail
    let result = engine.withdraw(user_idx, 999_999, 0, 1_000_000);
    assert!(
        result.is_err(),
        "Withdraw should fail with insufficient balance"
    );

    // Simulate Solana rollback (this is what the harness does)
    // Deep restore of RiskEngine contents
    *engine = before;

    // Verify state is exactly restored
    assert_eq!(engine.vault, expected_vault, "vault must be restored");
    assert_eq!(
        engine.accounts[user_idx as usize].capital, expected_capital,
        "capital must be restored"
    );
    assert_eq!(
        engine.accounts[user_idx as usize].pnl, expected_pnl,
        "pnl must be restored"
    );
    assert_eq!(
        engine.accounts[user_idx as usize].funding_index, expected_funding_index,
        "funding_index must be restored"
    );
    assert_eq!(
        engine.warmed_pos_total, expected_warmed_pos,
        "warmed_pos_total must be restored"
    );
    assert_eq!(
        engine.warmed_neg_total, expected_warmed_neg,
        "warmed_neg_total must be restored"
    );
    assert_eq!(
        engine.warmup_insurance_reserved, expected_warmup_reserved,
        "warmup_insurance_reserved must be restored"
    );

    // Conservation must still hold after rollback
    assert!(
        engine.check_conservation(DEFAULT_ORACLE),
        "Conservation must hold after harness rollback"
    );
}
