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
//! vault >= C_tot + insurance
//!
//! With the ADL K-coefficient funding model, funding is applied via side-global
//! K coefficients and per-account snapshots. There is no lazy funding index.
//! Conservation is simply: vault >= c_tot + insurance.
//!
//! ## Suite Components
//! - Global invariants (conservation, aggregate consistency)
//! - Action-based state machine fuzzer with Solana rollback simulation
//! - Focused unit property tests
//! - Deterministic seeded fuzzer with logging

#![cfg(feature = "fuzz")]

use percolator::*;
use proptest::prelude::*;

// ============================================================================
// CONSTANTS
// ============================================================================

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
    engine.accounts.len()
}

// ============================================================================
// SECTION 2: GLOBAL INVARIANTS HELPER
// ============================================================================

/// Assert all global invariants hold
/// IMPORTANT: This function is PURE - it does NOT mutate the engine.
/// Invariant checks must reflect on-chain semantics (funding is lazy).
fn assert_global_invariants(engine: &RiskEngine, context: &str) {
    // 1. Primary conservation: vault >= C_tot + insurance
    // This is oracle-independent (no mark PnL). The extended check with mark PnL
    // requires a consistent oracle across all account entry_prices, which the fuzzer
    // cannot guarantee when trades happen at different prices.
    let vault = engine.vault.get();
    let c_tot = engine.c_tot.get();
    let insurance = engine.insurance_fund.balance.get();
    assert!(
        vault >= c_tot.saturating_add(insurance),
        "{}: Primary conservation violated: vault={} < c_tot={} + insurance={}",
        context,
        vault,
        c_tot,
        insurance,
    );

    // 2. Aggregate consistency: c_tot == sum(capital), pnl_pos_tot == sum(max(pnl,0))
    let mut sum_capital = 0u128;
    let mut sum_pnl_pos = 0u128;
    let n = account_count(engine);
    for i in 0..n {
        if is_account_used(engine, i as u16) {
            let acc = &engine.accounts[i];
            sum_capital += acc.capital.get();
            let pnl = acc.pnl.get();
            if pnl > 0 {
                sum_pnl_pos += pnl as u128;
            }
        }
    }
    assert_eq!(
        engine.c_tot.get(),
        sum_capital,
        "{}: c_tot={} != sum(capital)={}",
        context,
        engine.c_tot.get(),
        sum_capital
    );
    assert_eq!(
        engine.pnl_pos_tot.get(),
        sum_pnl_pos,
        "{}: pnl_pos_tot={} != sum(max(pnl,0))={}",
        context,
        engine.pnl_pos_tot.get(),
        sum_pnl_pos
    );

    // 3. Account local sanity (for each used account)
    for i in 0..n {
        if is_account_used(engine, i as u16) {
            let acc = &engine.accounts[i];

            // reserved_pnl <= max(0, pnl)
            let pnl = acc.pnl.get();
            let positive_pnl = if pnl > 0 { pnl as u128 } else { 0 };
            assert!(
                acc.reserved_pnl.get() <= positive_pnl,
                "{}: Account {} has reserved_pnl={} > positive_pnl={}",
                context,
                i,
                acc.reserved_pnl.get(),
                positive_pnl
            );
        }
    }
}

// ============================================================================
// SECTION 3: PARAMETER REGIMES
// ============================================================================

/// Regime A: Normal mode (small floors)
fn params_regime_a() -> RiskParams {
    RiskParams {
        maintenance_margin_bps: 500,
        initial_margin_bps: 1000,
        trading_fee_bps: 10,
        max_accounts: 32, // Small for speed
        new_account_fee: U128::new(0),
        max_crank_staleness_slots: u64::MAX,
        liquidation_fee_bps: 50,
        liquidation_fee_cap: U128::new(100_000),
        min_liquidation_abs: U128::new(100_000),
        min_initial_deposit: U128::new(2),
        min_nonzero_mm_req: U128::new(1),
        min_nonzero_im_req: U128::new(2),
        insurance_floor: U128::ZERO,
        h_min: 0,
        h_max: 100,
        resolve_price_deviation_bps: 1000,
    }
}

/// Regime B: Floor + risk mode sensitivity (floor = 1000)
fn params_regime_b() -> RiskParams {
    RiskParams {
        maintenance_margin_bps: 500,
        initial_margin_bps: 1000,
        trading_fee_bps: 10,
        max_accounts: 32, // Small for speed
        new_account_fee: U128::new(0),
        max_crank_staleness_slots: u64::MAX,
        liquidation_fee_bps: 50,
        liquidation_fee_cap: U128::new(100_000),
        min_liquidation_abs: U128::new(100_000),
        min_initial_deposit: U128::new(1000),
        min_nonzero_mm_req: U128::new(1),
        min_nonzero_im_req: U128::new(2),
        insurance_floor: U128::ZERO,
        h_min: 0,
        h_max: 100,
        resolve_price_deviation_bps: 1000,
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
    rng_state: u64,        // For deterministic selector resolution
    last_oracle_price: u64, // Track last oracle price for conservation checks with mark PnL
}

impl FuzzState {
    fn new(params: RiskParams) -> Self {
        FuzzState {
            engine: Box::new(RiskEngine::new(params)),
            live_accounts: Vec::new(),
            lp_idx: None,
            rng_state: 12345,
            last_oracle_price: DEFAULT_ORACLE,
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
        let oracle = self.last_oracle_price; // Track for mark PnL consistency

        match action {
            Action::AddUser { fee_payment } => {
                // Snapshot engine and harness state for rollback
                let before = (*self.engine).clone();
                let live_before = self.live_accounts.clone();
                let num_used_before = self.count_used();

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

                        self.live_accounts.push(idx);
                        assert_global_invariants(&self.engine, &context);
                    }
                    Err(_) => {
                        // Simulate Solana rollback - restore engine and harness state
                        *self.engine = before;
                        self.live_accounts = live_before;
                    }
                }
            }

            Action::AddLp { fee_payment } => {
                // Snapshot engine and harness state for rollback
                let before = (*self.engine).clone();
                let live_before = self.live_accounts.clone();
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

                        self.live_accounts.push(idx);
                        if self.lp_idx.is_none() {
                            self.lp_idx = Some(idx);
                        }
                        assert_global_invariants(&self.engine, &context);
                    }
                    Err(_) => {
                        // Simulate Solana rollback - restore engine and harness state
                        *self.engine = before;
                        self.live_accounts = live_before;
                        self.lp_idx = lp_before;
                    }
                }
            }

            Action::Deposit { who, amount } => {
                let idx = self.resolve_selector(who);
                let before = (*self.engine).clone();
                let vault_before = self.engine.vault;

                let result = self.engine.deposit(idx, *amount, oracle, 0);

                match result {
                    Ok(()) => {
                        // vault_after == vault_before + amount
                        assert_eq!(
                            self.engine.vault,
                            vault_before + *amount,
                            "{}: vault didn't increase correctly",
                            context
                        );
                        assert_global_invariants(&self.engine, &context);
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

                let now_slot = self.engine.current_slot;
                let result = self.engine.withdraw_not_atomic(idx, *amount, oracle, now_slot, 0i128, 0);

                match result {
                    Ok(()) => {
                        // vault_after == vault_before - amount
                        assert_eq!(
                            self.engine.vault,
                            vault_before - *amount,
                            "{}: vault didn't decrease correctly",
                            context
                        );
                        assert_global_invariants(&self.engine, &context);
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
                assert_global_invariants(&self.engine, &context);
            }

            Action::AccrueFunding {
                dt,
                oracle_price,
                rate_bps,
            } => {
                let before = (*self.engine).clone();
                let now_slot = self.engine.current_slot.saturating_add(*dt);

                // v12.16.4: pass funding rate directly to accrue_market_to
                let result = self
                    .engine
                    .accrue_market_to(now_slot, *oracle_price, *rate_bps as i128);

                match result {
                    Ok(()) => {
                        self.last_oracle_price = *oracle_price;
                        assert_global_invariants(&self.engine, &context);
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
                let now_slot = self.engine.current_slot;

                let result = (|| -> Result<()> {
                    let mut ctx = InstructionContext::new_with_h_lock(0);
                    self.engine.accrue_market_to(now_slot, oracle, 0)?;
                    self.engine.current_slot = now_slot;
                    self.engine.touch_account_live_local(idx as usize, &mut ctx)?;
                    self.engine.finalize_touched_accounts_post_live(&ctx);
                    Ok(())
                })();

                match result {
                    Ok(()) => {
                        assert_global_invariants(&self.engine, &context);
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
                let now_slot = self.engine.current_slot;

                let result =
                    self.engine
                        .execute_trade_not_atomic(lp_idx, user_idx, *oracle_price, now_slot, *size, *oracle_price, 0i128, 0);

                match result {
                    Ok(_) => {
                        // Trade succeeded - update oracle price for mark PnL checks
                        self.last_oracle_price = *oracle_price;
                        assert_global_invariants(&self.engine, &context);
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

                let now_slot = self.engine.current_slot;
                let result = self.engine.top_up_insurance_fund(*amount, now_slot);

                match result {
                    Ok(_above_threshold) => {
                        // vault should increase
                        assert_eq!(
                            self.engine.vault,
                            vault_before + *amount,
                            "{}: vault didn't increase",
                            context
                        );
                        assert_global_invariants(&self.engine, &context);
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
        }

        for _ in 0..2 {
            if let Ok(idx) = state.engine.add_user(1) {
                state.live_accounts.push(idx);
                }
        }

        // Initial deposits
        for &idx in &state.live_accounts.clone() {
            let _ = state.engine.deposit(idx, 10_000, DEFAULT_ORACLE, 0);
        }

        // Top up insurance using proper API (maintains conservation)
        let current_insurance = state.engine.insurance_fund.balance.get();
        if initial_insurance > current_insurance {
            let now_slot = state.engine.current_slot;
            let _ = state.engine.top_up_insurance_fund(initial_insurance - current_insurance, now_slot);
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
        }

        for _ in 0..2 {
            if let Ok(idx) = state.engine.add_user(1) {
                state.live_accounts.push(idx);
                }
        }

        // Initial deposits
        for &idx in &state.live_accounts.clone() {
            let _ = state.engine.deposit(idx, 10_000, DEFAULT_ORACLE, 0);
        }

        // Top up insurance using proper API (maintains conservation)
        let floor = state.engine.params.insurance_floor.get();
        let target_insurance = initial_insurance.max(floor + 100);
        let current_insurance = state.engine.insurance_fund.balance.get();
        if target_insurance > current_insurance {
            let now_slot = state.engine.current_slot;
            let _ = state.engine.top_up_insurance_fund(target_insurance - current_insurance, now_slot);
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
    let action_type = rng.usize(0, 8);

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
        _ => Action::TopUpInsurance {
            amount: rng.u128(0, 10_000),
        },
    };

    let desc = format!("{:?}", action);
    (action, desc)
}

/// Compute conservation slack without panicking
/// Compute conservation slack: vault - (c_tot + insurance).
/// With the ADL K-coefficient funding model, there is no lazy funding index to settle.
fn compute_conservation_slack(engine: &RiskEngine) -> (i128, u128, i128, u128, u128) {
    let total_capital = engine.c_tot.get();
    let insurance = engine.insurance_fund.balance.get();
    let base = total_capital + insurance;
    let actual = engine.vault.get();
    let slack = actual as i128 - base as i128;
    (
        slack,
        total_capital,
        0i128, // net_settled_pnl no longer computed separately
        insurance,
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
        }

        for _ in 0..2 {
            if let Ok(idx) = state.engine.add_user(1) {
                state.live_accounts.push(idx);
            }
        }

        // Initial deposits
        for &idx in &state.live_accounts.clone() {
            let _ = state.engine.deposit(idx, rng.u128(5_000, 50_000), DEFAULT_ORACLE, 0);
        }

        // Top up insurance using proper API (maintains conservation)
        let floor = state.engine.params.insurance_floor.get();
        let target_ins = floor + rng.u128(5_000, 100_000);
        let current_ins = state.engine.insurance_fund.balance.get();
        if target_ins > current_ins {
            let now_slot = state.engine.current_slot;
            let _ = state.engine.top_up_insurance_fund(target_ins - current_ins, now_slot);
        }

        // Verify conservation after setup
        if !state.engine.check_conservation() {
            eprintln!("Conservation failed after setup for seed {}", seed);
            eprintln!(
                "  vault={}, insurance={}",
                state.engine.vault.get(), state.engine.insurance_fund.balance.get()
            );
            eprintln!("  live_accounts={:?}", state.live_accounts);
            let mut total_cap = 0u128;
            for &idx in &state.live_accounts {
                eprintln!(
                    "  account[{}]: capital={}",
                    idx, state.engine.accounts[idx as usize].capital.get()
                );
                total_cap += state.engine.accounts[idx as usize].capital.get();
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

proptest! {
    // Test that deposit always increases vault and principal
    #[test]
    fn fuzz_deposit_increases_balance(amount in amount_strategy()) {
        let mut engine = Box::new(RiskEngine::new(params_regime_a()));
        let user_idx = engine.add_user(1).unwrap();

        let vault_before = engine.vault;
        let principal_before = engine.accounts[user_idx as usize].capital;

        let _ = engine.deposit(user_idx, amount, DEFAULT_ORACLE, 0);

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

        engine.deposit(user_idx, deposit_amount, DEFAULT_ORACLE, 0).unwrap();

        // Snapshot for rollback simulation
        let before = (*engine).clone();

        let result = engine.withdraw_not_atomic(user_idx, withdraw_amount, DEFAULT_ORACLE, 0, 0i128, 0);

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
            let _ = engine.deposit(user_idx, amount, DEFAULT_ORACLE, 0);
        }

        prop_assert!(engine.check_conservation());

        for amount in withdrawals {
            let _ = engine.withdraw_not_atomic(user_idx, amount, DEFAULT_ORACLE, 0, 0i128, 0);
        }

        prop_assert!(engine.check_conservation());
    }
}

// ============================================================================
// SECTION 9: CONSERVATION REGRESSION TESTS
// These verify that conservation invariant holds under various conditions
// ============================================================================

/// Verify check_conservation holds after trades and market accrual.
/// Conservation: vault >= c_tot + insurance.
#[test]
fn conservation_after_trade_and_funding_regression() {
    let mut engine = Box::new(RiskEngine::new(params_regime_a()));

    // Create LP and user with positions
    let lp_idx = engine.add_lp([0u8; 32], [0u8; 32], 1).unwrap();
    let user_idx = engine.add_user(1).unwrap();
    engine.deposit(lp_idx, 100_000, DEFAULT_ORACLE, 0).unwrap();
    engine.deposit(user_idx, 100_000, DEFAULT_ORACLE, 0).unwrap();

    // Make crank fresh
    engine.last_crank_slot = 0;
    engine.last_market_slot = 0;
    engine.last_oracle_price = DEFAULT_ORACLE;

    // Execute trade to create positions
    engine
        .execute_trade_not_atomic(lp_idx, user_idx, DEFAULT_ORACLE, 0, 1000, DEFAULT_ORACLE, 0i128, 0)
        .unwrap();

    // Accrue market with funding (rate passed directly)
    engine.advance_slot(1000);
    let slot = engine.current_slot;
    engine.accrue_market_to(slot, DEFAULT_ORACLE, 500).unwrap();

    // Verify conservation
    assert!(
        engine.check_conservation(),
        "check_conservation failed after trade and market accrual"
    );

    // Also verify manually: vault >= c_tot + insurance
    let vault = engine.vault.get();
    let c_tot = engine.c_tot.get();
    let insurance = engine.insurance_fund.balance.get();
    assert!(
        vault >= c_tot + insurance,
        "Manual conservation check: vault={} < c_tot={} + insurance={}",
        vault,
        c_tot,
        insurance
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
    engine.deposit(user_idx, 1000, DEFAULT_ORACLE, 0).unwrap();

    // Accrue market to create state that could be mutated (rate passed directly)
    engine.last_oracle_price = DEFAULT_ORACLE;
    engine.advance_slot(100);
    let slot = engine.current_slot;
    engine.accrue_market_to(slot, DEFAULT_ORACLE, 100).unwrap();

    // Capture complete state before failed operation (deep clone of RiskEngine)
    let before = (*engine).clone();

    // Capture expected values before any operation
    let expected_vault = engine.vault;
    let expected_capital = engine.accounts[user_idx as usize].capital;
    let expected_pnl = engine.accounts[user_idx as usize].pnl;

    // Try to withdraw_not_atomic more than available - will fail
    let result = engine.withdraw_not_atomic(user_idx, 999_999, DEFAULT_ORACLE, slot, 0i128, 0);
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

    // Conservation must still hold after rollback
    assert!(
        engine.check_conservation(),
        "Conservation must hold after harness rollback"
    );
}
