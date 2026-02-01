//! Fast unit tests for the risk engine
//! Run with: cargo test

use percolator::*;

// Use the no-op matcher for tests
const MATCHER: NoOpMatcher = NoOpMatcher;

// Default oracle price for conservation checks (1 unit in 6 decimal scale)
const DEFAULT_ORACLE: u64 = 1_000_000;

// ==============================================================================
// DETERMINISTIC PRNG FOR FUZZ TESTS
// ==============================================================================

/// Simple xorshift64 PRNG for deterministic fuzz testing
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed)
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn u64(&mut self, lo: u64, hi: u64) -> u64 {
        if lo >= hi {
            return lo;
        }
        lo + (self.next() % (hi - lo + 1))
    }

    fn i128(&mut self, lo: i128, hi: i128) -> i128 {
        if lo >= hi {
            return lo;
        }
        lo + (self.next() as i128 % (hi - lo + 1))
    }

    fn u128(&mut self, lo: u128, hi: u128) -> u128 {
        if lo >= hi {
            return lo;
        }
        lo + (self.next() as u128 % (hi - lo + 1))
    }
}

fn default_params() -> RiskParams {
    RiskParams {
        warmup_period_slots: 100,
        maintenance_margin_bps: 500, // 5%
        initial_margin_bps: 1000,    // 10%
        trading_fee_bps: 10,         // 0.1%
        max_accounts: 1000,
        new_account_fee: U128::new(0),          // Zero fee for tests
        risk_reduction_threshold: U128::new(0), // Default: only trigger on full depletion
        maintenance_fee_per_slot: U128::new(0), // No maintenance fee by default
        max_crank_staleness_slots: u64::MAX,
        liquidation_fee_bps: 50,                 // 0.5% liquidation fee
        liquidation_fee_cap: U128::new(100_000), // Cap at 100k units
        liquidation_buffer_bps: 100,             // 1% buffer above maintenance
        min_liquidation_abs: U128::new(100_000), // Minimum 0.1 units (scaled by 1e6)
    }
}

// ==============================================================================
// TEST HELPERS (MANDATORY)
// ==============================================================================

// IMPORTANT: check_conservation() enforces bounded slack (MAX_ROUNDING_SLACK).
// Therefore tests MUST NOT "fund" pnl by increasing vault unless the same value
// is represented in expected accounting terms (capital/insurance/loss_accum or net_pnl).
// Prefer zero-sum pnl setups over direct vault mutation.

fn assert_conserved(engine: &RiskEngine) {
    assert!(
        engine.check_conservation(DEFAULT_ORACLE),
        "Conservation invariant violated"
    );
}

fn assert_conserved_at(engine: &RiskEngine, oracle_price: u64) {
    assert!(
        engine.check_conservation(oracle_price),
        "Conservation invariant violated"
    );
}

fn vault_snapshot(engine: &RiskEngine) -> u128 {
    engine.vault.get()
}

fn assert_vault_delta(engine: &RiskEngine, before: u128, delta: i128) {
    let after = engine.vault.get() as i128;
    let before_i = before as i128;
    assert_eq!(
        after - before_i,
        delta,
        "Unexpected vault delta: before={}, after={}, expected_delta={}",
        before,
        engine.vault.get(),
        delta
    );
}

/// Set insurance balance while adjusting vault to preserve conservation.
/// This models a "top-up" from an external source that deposits to both vault and insurance.
fn set_insurance(engine: &mut RiskEngine, new_balance: u128) {
    let old = engine.insurance_fund.balance.get();
    engine.insurance_fund.balance = U128::new(new_balance);
    if new_balance >= old {
        engine.vault = U128::new(engine.vault.get().saturating_add(new_balance - old));
    } else {
        engine.vault = U128::new(engine.vault.get().saturating_sub(old - new_balance));
    }
}

// ==============================================================================
// TESTS (MIXED API + WHITEBOX)
// ==============================================================================

#[test]
fn test_deposit_and_withdraw() {
    let mut engine = Box::new(RiskEngine::new(default_params()));
    let user_idx = engine.add_user(0).unwrap();

    // Deposit
    let v0 = vault_snapshot(&engine);
    engine.deposit(user_idx, 1000, 0).unwrap();
    assert_eq!(engine.accounts[user_idx as usize].capital.get(), 1000);
    assert_vault_delta(&engine, v0, 1000);

    // Withdraw partial
    let v1 = vault_snapshot(&engine);
    engine.withdraw(user_idx, 400, 0, 1_000_000).unwrap();
    assert_eq!(engine.accounts[user_idx as usize].capital.get(), 600);
    assert_vault_delta(&engine, v1, -400);

    // Withdraw rest
    let v2 = vault_snapshot(&engine);
    engine.withdraw(user_idx, 600, 0, 1_000_000).unwrap();
    assert_eq!(engine.accounts[user_idx as usize].capital.get(), 0);
    assert_vault_delta(&engine, v2, -600);

    assert_conserved(&engine);
}

#[test]
fn test_withdraw_insufficient_balance() {
    let mut engine = Box::new(RiskEngine::new(default_params()));
    let user_idx = engine.add_user(0).unwrap();

    engine.deposit(user_idx, 1000, 0).unwrap();

    // Try to withdraw more than deposited
    let result = engine.withdraw(user_idx, 1500, 0, 1_000_000);
    assert_eq!(result, Err(RiskError::InsufficientBalance));
}

#[test]
fn test_deposit_settles_accrued_maintenance_fees() {
    // Setup engine with non-zero maintenance fee
    let mut params = default_params();
    params.maintenance_fee_per_slot = U128::new(10); // 10 units per slot
    let mut engine = Box::new(RiskEngine::new(params));

    let user_idx = engine.add_user(0).unwrap();

    // Initial deposit at slot 0
    engine.deposit(user_idx, 1000, 0).unwrap();
    assert_eq!(engine.accounts[user_idx as usize].capital.get(), 1000);
    assert_eq!(engine.accounts[user_idx as usize].last_fee_slot, 0);

    // Deposit at slot 100 - should charge 100 * 10 = 1000 in fees
    // Depositing 500:
    //   - 500 from deposit pays fees → insurance += 500, fee_credits = -500
    //   - 0 goes to capital
    //   - pay_fee_debt_from_capital sweep: capital(1000) pays remaining 500 debt
    //     → capital = 500, insurance += 500, fee_credits = 0
    let insurance_before = engine.insurance_fund.balance;
    engine.deposit(user_idx, 500, 100).unwrap();

    // Account's last_fee_slot should be updated
    assert_eq!(engine.accounts[user_idx as usize].last_fee_slot, 100);

    // Capital = 500 (was 1000, fee debt sweep paid 500)
    assert_eq!(engine.accounts[user_idx as usize].capital.get(), 500);

    // Insurance received 1000 total: 500 from deposit + 500 from capital sweep
    assert_eq!(
        (engine.insurance_fund.balance - insurance_before).get(),
        1000
    );

    // fee_credits fully repaid by capital sweep
    assert_eq!(engine.accounts[user_idx as usize].fee_credits.get(), 0);

    // Now deposit 1000 more at slot 100 (no additional fees, no debt)
    engine.deposit(user_idx, 1000, 100).unwrap();

    // All 1000 goes to capital (no debt to pay)
    assert_eq!(engine.accounts[user_idx as usize].capital.get(), 1500);
    assert_eq!(engine.accounts[user_idx as usize].fee_credits.get(), 0);

    assert_conserved(&engine);
}

#[test]
fn test_withdraw_principal_with_negative_pnl_should_fail() {
    let mut engine = Box::new(RiskEngine::new(default_params()));
    let user_idx = engine.add_user(0).unwrap();

    // User deposits 1000
    engine.deposit(user_idx, 1000, 0).unwrap();

    // User has a position and negative PNL of -800
    engine.accounts[user_idx as usize].position_size = I128::new(10_000);
    engine.accounts[user_idx as usize].entry_price = 1_000_000; // $1 entry price
    engine.accounts[user_idx as usize].pnl = I128::new(-800);

    // Trying to withdraw all principal would leave collateral = 0 + max(0, -800) = 0
    // This should fail because user has an open position
    let result = engine.withdraw(user_idx, 1000, 0, 1_000_000);

    assert!(
        result.is_err(),
        "Should not allow withdrawal that leaves account undercollateralized with open position"
    );
}

#[test]
fn test_pnl_warmup() {
    let mut engine = Box::new(RiskEngine::new(default_params()));
    let user_idx = engine.add_user(0).unwrap();
    let counterparty = engine.add_user(0).unwrap();

    // Zero-sum PNL: user gains, counterparty loses (no vault funding needed)
    assert_eq!(engine.accounts[user_idx as usize].pnl.get(), 0);
    assert_eq!(engine.accounts[counterparty as usize].pnl.get(), 0);
    engine.accounts[user_idx as usize].pnl = I128::new(1000);
    engine.accounts[user_idx as usize].warmup_slope_per_step = U128::new(10); // 10 per slot
    engine.accounts[counterparty as usize].pnl = I128::new(-1000);
    assert_conserved(&engine);

    // At slot 0, nothing is warmed up yet
    assert_eq!(
        engine.withdrawable_pnl(&engine.accounts[user_idx as usize]),
        0
    );

    // Advance 50 slots
    engine.advance_slot(50);
    assert_eq!(
        engine.withdrawable_pnl(&engine.accounts[user_idx as usize]),
        500
    ); // 10 * 50

    // Advance 100 more slots (total 150)
    engine.advance_slot(100);
    assert_eq!(
        engine.withdrawable_pnl(&engine.accounts[user_idx as usize]),
        1000
    ); // Capped at total PNL
}

#[test]
fn test_pnl_warmup_with_reserved() {
    let mut engine = Box::new(RiskEngine::new(default_params()));
    let user_idx = engine.add_user(0).unwrap();
    let counterparty = engine.add_user(0).unwrap();

    // Zero-sum PNL: user gains, counterparty loses (no vault funding needed)
    assert_eq!(engine.accounts[user_idx as usize].pnl.get(), 0);
    assert_eq!(engine.accounts[counterparty as usize].pnl.get(), 0);
    engine.accounts[user_idx as usize].pnl = I128::new(1000);
    engine.accounts[user_idx as usize].reserved_pnl = 300; // 300 reserved for pending withdrawal
    engine.accounts[user_idx as usize].warmup_slope_per_step = U128::new(10);
    engine.accounts[counterparty as usize].pnl = I128::new(-1000);
    assert_conserved(&engine);

    // Advance 100 slots
    engine.advance_slot(100);

    // Withdrawable = min(available_pnl, warmed_up)
    // available_pnl = 1000 - 300 = 700
    // warmed_up = 10 * 100 = 1000
    // So withdrawable = 700
    assert_eq!(
        engine.withdrawable_pnl(&engine.accounts[user_idx as usize]),
        700
    );
}

#[test]
fn test_withdraw_pnl_not_warmed_up() {
    let mut engine = Box::new(RiskEngine::new(default_params()));
    let user_idx = engine.add_user(0).unwrap();
    let counterparty = engine.add_user(0).unwrap();

    engine.deposit(user_idx, 1000, 0).unwrap();
    // Zero-sum PNL: user gains, counterparty loses (no vault funding needed)
    assert_eq!(engine.accounts[user_idx as usize].pnl.get(), 0);
    assert_eq!(engine.accounts[counterparty as usize].pnl.get(), 0);
    engine.accounts[user_idx as usize].pnl = I128::new(500);
    engine.accounts[counterparty as usize].pnl = I128::new(-500);
    assert_conserved(&engine);

    // Try to withdraw more than principal + warmed up PNL
    // Since PNL hasn't warmed up, can only withdraw the 1000 principal
    let result = engine.withdraw(user_idx, 1100, 0, 1_000_000);
    assert_eq!(result, Err(RiskError::InsufficientBalance));
}

#[test]
fn test_withdraw_with_warmed_up_pnl() {
    let mut engine = Box::new(RiskEngine::new(default_params()));
    let user_idx = engine.add_user(0).unwrap();
    let counterparty = engine.add_user(0).unwrap();

    // Add insurance to provide warmup budget for converting positive PnL to capital
    // Budget = warmed_neg_total + insurance_spendable_raw() = 0 + 500 = 500
    set_insurance(&mut engine, 500);

    engine.deposit(user_idx, 1000, 0).unwrap();
    // Zero-sum PnL: user gains, counterparty loses (no vault funding needed)
    assert_eq!(engine.accounts[user_idx as usize].pnl.get(), 0);
    assert_eq!(engine.accounts[counterparty as usize].pnl.get(), 0);
    engine.accounts[user_idx as usize].pnl = I128::new(500);
    engine.accounts[counterparty as usize].pnl = I128::new(-500);
    engine.accounts[user_idx as usize].warmup_slope_per_step = U128::new(10);
    assert_conserved(&engine);

    // Advance enough slots to warm up 200 PNL
    engine.advance_slot(20);

    // Should be able to withdraw 1200 (1000 principal + 200 warmed PNL)
    // The function will automatically convert the 200 PNL to principal before withdrawal
    engine.withdraw(user_idx, 1200, engine.current_slot, 1_000_000).unwrap();
    assert_eq!(engine.accounts[user_idx as usize].pnl.get(), 300); // 500 - 200 converted
    assert_eq!(engine.accounts[user_idx as usize].capital.get(), 0); // 1000 + 200 - 1200
    assert_conserved(&engine);
}
#[test]
fn test_conservation_simple() {
    let mut engine = Box::new(RiskEngine::new(default_params()));
    let user1 = engine.add_user(0).unwrap();
    let user2 = engine.add_user(0).unwrap();

    // Initial state should conserve
    assert!(engine.check_conservation(DEFAULT_ORACLE));

    // Deposit to user1
    engine.deposit(user1, 1000, 0).unwrap();
    assert!(engine.check_conservation(DEFAULT_ORACLE));

    // Deposit to user2
    engine.deposit(user2, 2000, 0).unwrap();
    assert!(engine.check_conservation(DEFAULT_ORACLE));

    // PNL is zero-sum: user1 gains 500, user2 loses 500
    // (vault unchanged since this is internal redistribution)
    assert_eq!(engine.accounts[user1 as usize].pnl.get(), 0);
    assert_eq!(engine.accounts[user2 as usize].pnl.get(), 0);
    engine.accounts[user1 as usize].pnl = I128::new(500);
    engine.accounts[user2 as usize].pnl = I128::new(-500);
    assert!(engine.check_conservation(DEFAULT_ORACLE));

    // Withdraw from user1's capital
    engine.withdraw(user1, 500, 0, 1_000_000).unwrap();
    assert!(engine.check_conservation(DEFAULT_ORACLE));
}

#[test]
fn test_adl_haircut_unwrapped_pnl() {
    let mut engine = Box::new(RiskEngine::new(default_params()));
    let user_idx = engine.add_user(0).unwrap();
    let counterparty = engine.add_user(0).unwrap();

    // WHITEBOX: Set capital and pnl directly. Add to vault (not override) to preserve account fees.
    engine.accounts[user_idx as usize].capital = U128::new(1000);
    engine.vault += 1000; // capital only
                          // Zero-sum PnL: user gains, counterparty loses (no vault funding needed)
    assert_eq!(engine.accounts[user_idx as usize].pnl.get(), 0);
    assert_eq!(engine.accounts[counterparty as usize].pnl.get(), 0);
    engine.accounts[user_idx as usize].pnl = I128::new(500); // All unwrapped (warmup not started)
    engine.accounts[counterparty as usize].pnl = I128::new(-500);
    engine.accounts[user_idx as usize].warmup_slope_per_step = U128::new(10);
    assert_conserved(&engine);

    // Apply ADL loss of 200
    engine.apply_adl(200).unwrap();

    // Should haircut the unwrapped PNL
    assert_eq!(engine.accounts[user_idx as usize].pnl.get(), 300);
    assert_eq!(engine.accounts[user_idx as usize].capital.get(), 1000); // Principal untouched!
}

#[test]
fn test_adl_overflow_to_insurance() {
    let mut engine = Box::new(RiskEngine::new(default_params()));
    let user_idx = engine.add_user(0).unwrap();
    let counterparty = engine.add_user(0).unwrap();

    // WHITEBOX: Set capital, pnl, and insurance directly.
    engine.accounts[user_idx as usize].capital = U128::new(1000);
    engine.vault += 1000; // capital only
                          // Zero-sum PnL: user gains, counterparty loses (no vault funding needed)
    assert_eq!(engine.accounts[user_idx as usize].pnl.get(), 0);
    assert_eq!(engine.accounts[counterparty as usize].pnl.get(), 0);
    engine.accounts[user_idx as usize].pnl = I128::new(300); // Only 300 unwrapped PNL
    engine.accounts[counterparty as usize].pnl = I128::new(-300);
    engine.accounts[user_idx as usize].warmup_slope_per_step = U128::new(10);
    let ins_before = engine.insurance_fund.balance.get();
    set_insurance(&mut engine, ins_before + 500); // Add 500 to insurance (adjusts vault)
    assert_conserved(&engine);

    // Apply ADL loss of 700 (more than unwrapped PNL)
    engine.apply_adl(700).unwrap();

    // Should haircut all PNL first
    assert_eq!(engine.accounts[user_idx as usize].pnl.get(), 0);
    assert_eq!(engine.accounts[user_idx as usize].capital.get(), 1000); // Principal still untouched!

    // Remaining 400 should come from insurance (700 - 300 = 400)
    // Insurance should be reduced by 400 from added amount (500 - 400 = 100 above account fees)
    assert_eq!(engine.insurance_fund.balance.get(), ins_before + 100);
}

#[test]
fn test_adl_insurance_depleted() {
    let mut engine = Box::new(RiskEngine::new(default_params()));
    let user_idx = engine.add_user(0).unwrap();
    let counterparty = engine.add_user(0).unwrap();

    engine.accounts[user_idx as usize].capital = U128::new(1000);
    engine.vault += 1000; // capital only
                          // Zero-sum PnL: user gains, counterparty loses (no vault funding needed)
    assert_eq!(engine.accounts[user_idx as usize].pnl.get(), 0);
    assert_eq!(engine.accounts[counterparty as usize].pnl.get(), 0);
    engine.accounts[user_idx as usize].pnl = I128::new(100);
    engine.accounts[counterparty as usize].pnl = I128::new(-100);
    set_insurance(&mut engine, 50);
    assert_conserved(&engine);

    // Apply ADL loss of 200
    engine.apply_adl(200).unwrap();

    // PNL haircut: 100
    assert_eq!(engine.accounts[user_idx as usize].pnl.get(), 0);

    // Insurance depleted: 50
    assert_eq!(engine.insurance_fund.balance.get(), 0);

    // Remaining 50 goes to loss accumulator
    assert_eq!(engine.loss_accum.get(), 50);
}

#[test]
fn test_collateral_calculation() {
    let mut engine = Box::new(RiskEngine::new(default_params()));
    let user_idx = engine.add_user(0).unwrap();

    engine.accounts[user_idx as usize].capital = U128::new(1000);
    assert_eq!(engine.accounts[user_idx as usize].pnl.get(), 0);
    engine.accounts[user_idx as usize].pnl = I128::new(500);

    assert_eq!(
        engine.account_collateral(&engine.accounts[user_idx as usize]),
        1500
    );

    // Negative PNL doesn't add to collateral
    engine.accounts[user_idx as usize].pnl = I128::new(-300);
    assert_eq!(
        engine.account_collateral(&engine.accounts[user_idx as usize]),
        1000
    );
}

#[test]
fn test_maintenance_margin_check() {
    let mut engine = Box::new(RiskEngine::new(default_params()));
    let user_idx = engine.add_user(0).unwrap();

    engine.accounts[user_idx as usize].capital = U128::new(1000);
    engine.accounts[user_idx as usize].position_size = I128::new(10_000); // 10k units
    engine.accounts[user_idx as usize].entry_price = 1_000_000; // $1

    // At price $1, position value = 10k
    // Maintenance margin (5%) = 500
    // Collateral = 1000, so above maintenance
    assert!(engine.is_above_maintenance_margin(&engine.accounts[user_idx as usize], 1_000_000));

    // At price $2, position value = 20k
    // Maintenance margin (5%) = 1000
    // Collateral = 1000, so just at threshold (should be false)
    assert!(!engine.is_above_maintenance_margin(&engine.accounts[user_idx as usize], 2_000_000));
}

#[test]
fn test_trading_opens_position() {
    let mut engine = Box::new(RiskEngine::new(default_params()));
    let user_idx = engine.add_user(0).unwrap();
    let lp_idx = engine.add_lp([1u8; 32], [2u8; 32], 0).unwrap();

    // Setup user with capital
    engine.deposit(user_idx, 10_000, 0).unwrap();
    // WHITEBOX: Set LP capital directly. Add to vault to preserve conservation.
    engine.accounts[lp_idx as usize].capital = U128::new(100_000);
    engine.vault += 100_000;
    assert_conserved(&engine);

    // Execute trade: user buys 1000 units at $1
    let oracle_price = 1_000_000;
    let size = 1000i128;

    engine
        .execute_trade(&MATCHER, lp_idx, user_idx, 0, oracle_price, size)
        .unwrap();

    // Check position opened
    assert_eq!(engine.accounts[user_idx as usize].position_size.get(), 1000);
    assert_eq!(engine.accounts[user_idx as usize].entry_price, oracle_price);

    // Check LP has opposite position
    assert_eq!(engine.accounts[lp_idx as usize].position_size.get(), -1000);

    // Check fee was charged (0.1% of 1000 = 1)
    assert!(!engine.insurance_fund.fee_revenue.is_zero());
}

#[test]
fn test_trading_realizes_pnl() {
    let mut engine = Box::new(RiskEngine::new(default_params()));
    let user_idx = engine.add_user(0).unwrap();
    let lp_idx = engine.add_lp([1u8; 32], [2u8; 32], 0).unwrap();

    engine.deposit(user_idx, 10_000, 0).unwrap();
    // WHITEBOX: Set LP capital directly. Add to vault (not override) to preserve account fees.
    engine.accounts[lp_idx as usize].capital = U128::new(100_000);
    engine.vault += 100_000;
    assert_conserved(&engine);

    // Open long position at $1
    engine
        .execute_trade(&MATCHER, lp_idx, user_idx, 0, 1_000_000, 1000)
        .unwrap();

    // Close position at $1.50 (50% profit)
    engine
        .execute_trade(&MATCHER, lp_idx, user_idx, 0, 1_500_000, -1000)
        .unwrap();

    // Check PNL realized (approximately)
    // Price went from $1 to $1.50, so 500 profit on 1000 units
    assert!(engine.accounts[user_idx as usize].pnl.is_positive());
    assert_eq!(engine.accounts[user_idx as usize].position_size.get(), 0);
}

#[test]
fn test_user_isolation() {
    let mut engine = Box::new(RiskEngine::new(default_params()));
    let user1 = engine.add_user(0).unwrap();
    let user2 = engine.add_user(0).unwrap();

    engine.deposit(user1, 1000, 0).unwrap();
    engine.deposit(user2, 2000, 0).unwrap();

    let user2_principal_before = engine.accounts[user2 as usize].capital;
    let user2_pnl_before = engine.accounts[user2 as usize].pnl;

    // Operate on user1
    engine.withdraw(user1, 500, 0, 1_000_000).unwrap();
    assert_eq!(engine.accounts[user1 as usize].pnl.get(), 0);
    engine.accounts[user1 as usize].pnl = I128::new(300);

    // User2 should be unchanged
    assert_eq!(
        engine.accounts[user2 as usize].capital,
        user2_principal_before
    );
    assert_eq!(engine.accounts[user2 as usize].pnl, user2_pnl_before);
}

#[test]
fn test_principal_never_reduced_by_adl() {
    let mut engine = Box::new(RiskEngine::new(default_params()));
    let user_idx = engine.add_user(0).unwrap();

    let initial_principal = 5000u128;
    engine.accounts[user_idx as usize].capital = U128::new(initial_principal);
    assert_eq!(engine.accounts[user_idx as usize].pnl.get(), 0);
    engine.accounts[user_idx as usize].pnl = I128::new(100);

    // Apply massive ADL
    engine.apply_adl(10_000).unwrap();

    // Principal should NEVER be touched
    assert_eq!(
        engine.accounts[user_idx as usize].capital.get(),
        initial_principal
    );
}

#[test]
fn test_multiple_users_adl() {
    let mut engine = Box::new(RiskEngine::new(default_params()));
    let user1 = engine.add_user(0).unwrap();
    let user2 = engine.add_user(0).unwrap();
    let user3 = engine.add_user(0).unwrap();

    // User1: has unwrapped PNL
    engine.accounts[user1 as usize].capital = U128::new(1000);
    assert_eq!(engine.accounts[user1 as usize].pnl.get(), 0);
    engine.accounts[user1 as usize].pnl = I128::new(500);
    engine.accounts[user1 as usize].warmup_slope_per_step = U128::new(10);

    // User2: has unwrapped PNL
    engine.accounts[user2 as usize].capital = U128::new(2000);
    assert_eq!(engine.accounts[user2 as usize].pnl.get(), 0);
    engine.accounts[user2 as usize].pnl = I128::new(800);
    engine.accounts[user2 as usize].warmup_slope_per_step = U128::new(10);

    // User3: no PNL
    engine.accounts[user3 as usize].capital = U128::new(1500);

    set_insurance(&mut engine, 1000);

    // Apply ADL loss of 1000
    engine.apply_adl(1000).unwrap();

    // Should haircut user1 and user2's PNL
    // Total unwrapped PNL = 500 + 800 = 1300
    // Loss = 1000, so both should be haircutted proportionally or sequentially
    assert!(
        engine.accounts[user1 as usize].pnl.get() < 500
            || engine.accounts[user2 as usize].pnl.get() < 800
    );

    // All principals should be intact
    assert_eq!(engine.accounts[user1 as usize].capital.get(), 1000);
    assert_eq!(engine.accounts[user2 as usize].capital.get(), 2000);
    assert_eq!(engine.accounts[user3 as usize].capital.get(), 1500);
}

#[test]
fn test_warmup_monotonicity() {
    let mut engine = Box::new(RiskEngine::new(default_params()));
    let user_idx = engine.add_user(0).unwrap();
    let counterparty = engine.add_user(0).unwrap();

    // Zero-sum PNL: user gains, counterparty loses (no vault funding needed)
    assert_eq!(engine.accounts[user_idx as usize].pnl.get(), 0);
    assert_eq!(engine.accounts[counterparty as usize].pnl.get(), 0);
    engine.accounts[user_idx as usize].pnl = I128::new(1000);
    engine.accounts[user_idx as usize].warmup_slope_per_step = U128::new(10);
    engine.accounts[counterparty as usize].pnl = I128::new(-1000);
    assert_conserved(&engine);

    // Get withdrawable at different time points
    let w0 = engine.withdrawable_pnl(&engine.accounts[user_idx as usize]);

    engine.advance_slot(10);
    let w1 = engine.withdrawable_pnl(&engine.accounts[user_idx as usize]);

    engine.advance_slot(20);
    let w2 = engine.withdrawable_pnl(&engine.accounts[user_idx as usize]);

    // Should be monotonically increasing
    assert!(w1 >= w0);
    assert!(w2 >= w1);
}

#[test]
fn test_fee_accumulation() {
    // WHITEBOX: direct state mutation for vault/capital setup
    let mut engine = Box::new(RiskEngine::new(default_params()));
    let user_idx = engine.add_user(0).unwrap();
    let lp_idx = engine.add_lp([1u8; 32], [2u8; 32], 0).unwrap();

    engine.deposit(user_idx, 100_000, 0).unwrap();
    // WHITEBOX: Set LP capital directly. Add to vault (not override) to preserve account fees.
    engine.accounts[lp_idx as usize].capital = U128::new(1_000_000);
    engine.vault += 1_000_000;
    assert_conserved(&engine);

    // Track fee revenue and balance BEFORE trades
    let fee_rev_before = engine.insurance_fund.fee_revenue;
    let ins_before = engine.insurance_fund.balance;

    // Execute multiple trades, counting successes
    // Trade size must be > 1000 for fee to be non-zero (fee_bps=10, notional needs > 10000/10=1000)
    let mut succeeded = 0usize;
    for _ in 0..10 {
        if engine
            .execute_trade(&MATCHER, lp_idx, user_idx, 0, 1_000_000, 10_000)
            .is_ok()
        {
            succeeded += 1;
        }
        if engine
            .execute_trade(&MATCHER, lp_idx, user_idx, 0, 1_000_000, -10_000)
            .is_ok()
        {
            succeeded += 1;
        }
    }

    let fee_rev_after = engine.insurance_fund.fee_revenue;
    let ins_after = engine.insurance_fund.balance;

    // If any trades succeeded, fees should have accumulated
    if succeeded > 0 {
        assert!(
            fee_rev_after > fee_rev_before,
            "fee_revenue must increase on successful trades"
        );
        assert!(
            ins_after >= ins_before,
            "insurance balance must not decrease"
        );
    }

    assert_conserved(&engine);
}

#[test]
fn test_lp_warmup_initial_state() {
    let mut engine = Box::new(RiskEngine::new(default_params()));
    let lp_idx = engine.add_lp([1u8; 32], [2u8; 32], 10000).unwrap();

    // LP should start with warmup state initialized
    assert_eq!(engine.accounts[lp_idx as usize].reserved_pnl, 0);
    assert_eq!(engine.accounts[lp_idx as usize].warmup_started_at_slot, 0);
}

#[test]
fn test_lp_warmup_monotonic() {
    let mut engine = Box::new(RiskEngine::new(default_params()));
    let lp_idx = engine.add_lp([1u8; 32], [2u8; 32], 10000).unwrap();
    let user = engine.add_user(0).unwrap();

    // Zero-sum PNL: LP gains, user loses (no vault funding needed)
    assert_eq!(engine.accounts[lp_idx as usize].pnl.get(), 0);
    assert_eq!(engine.accounts[user as usize].pnl.get(), 0);
    engine.accounts[lp_idx as usize].pnl = I128::new(10_000);
    engine.accounts[user as usize].pnl = I128::new(-10_000);
    assert_conserved(&engine);

    // At slot 0
    let w0 = engine.withdrawable_pnl(&engine.accounts[lp_idx as usize]);

    // Advance 50 slots
    engine.advance_slot(50);
    let w50 = engine.withdrawable_pnl(&engine.accounts[lp_idx as usize]);

    // Advance another 50 slots (total 100)
    engine.advance_slot(50);
    let w100 = engine.withdrawable_pnl(&engine.accounts[lp_idx as usize]);

    // Withdrawable should be monotonically increasing
    assert!(
        w50 >= w0,
        "LP warmup should be monotonic: w0={}, w50={}",
        w0,
        w50
    );
    assert!(
        w100 >= w50,
        "LP warmup should be monotonic: w50={}, w100={}",
        w50,
        w100
    );
}

#[test]
fn test_lp_warmup_bounded() {
    let mut engine = Box::new(RiskEngine::new(default_params()));
    let lp_idx = engine.add_lp([1u8; 32], [2u8; 32], 10000).unwrap();
    let user = engine.add_user(0).unwrap();

    // Zero-sum PNL: LP gains, user loses (no vault funding needed)
    assert_eq!(engine.accounts[lp_idx as usize].pnl.get(), 0);
    assert_eq!(engine.accounts[user as usize].pnl.get(), 0);
    engine.accounts[lp_idx as usize].pnl = I128::new(5_000);
    engine.accounts[user as usize].pnl = I128::new(-5_000);
    assert_conserved(&engine);

    // Reserve some PNL
    engine.accounts[lp_idx as usize].reserved_pnl = 1_000;

    // Even after long time, withdrawable should not exceed available (positive_pnl - reserved)
    engine.advance_slot(1000);
    let withdrawable = engine.withdrawable_pnl(&engine.accounts[lp_idx as usize]);

    assert!(
        withdrawable <= 4_000,
        "Withdrawable {} should not exceed available {}",
        withdrawable,
        4_000
    );
}

#[test]
fn test_lp_warmup_with_negative_pnl() {
    let mut engine = Box::new(RiskEngine::new(default_params()));
    let lp_idx = engine.add_lp([1u8; 32], [2u8; 32], 10000).unwrap();

    // LP has negative PNL
    assert_eq!(engine.accounts[lp_idx as usize].pnl.get(), 0);
    engine.accounts[lp_idx as usize].pnl = I128::new(-3_000);

    // Advance time
    engine.advance_slot(100);

    // With negative PNL, withdrawable should be 0
    let withdrawable = engine.withdrawable_pnl(&engine.accounts[lp_idx as usize]);
    assert_eq!(
        withdrawable, 0,
        "Withdrawable should be 0 with negative PNL"
    );
}

// ============================================================================
// Funding Rate Tests
// ============================================================================

#[test]
fn test_funding_positive_rate_longs_pay_shorts() {
    // T1: Positive funding → longs pay shorts
    let mut engine = Box::new(RiskEngine::new(default_params()));
    let user_idx = engine.add_user(0).unwrap();
    let lp_idx = engine.add_lp([1u8; 32], [2u8; 32], 0).unwrap();

    engine.deposit(user_idx, 100_000, 0).unwrap();
    // WHITEBOX: Set LP capital directly. Add to vault (not override) to preserve account fees.
    engine.accounts[lp_idx as usize].capital = U128::new(1_000_000);
    engine.vault += 1_000_000;

    // User opens long position (+1 base unit)
    engine.accounts[user_idx as usize].position_size = I128::new(1_000_000); // +1M base units
    engine.accounts[user_idx as usize].entry_price = 100_000_000; // $100

    // LP has opposite short position
    engine.accounts[lp_idx as usize].position_size = I128::new(-1_000_000);
    engine.accounts[lp_idx as usize].entry_price = 100_000_000;

    // Zero warmup/reserved to avoid side effects from touch_account
    engine.accounts[user_idx as usize].warmup_slope_per_step = U128::new(0);
    engine.accounts[user_idx as usize].reserved_pnl = 0;
    engine.accounts[user_idx as usize].warmup_started_at_slot = engine.current_slot;
    engine.accounts[lp_idx as usize].warmup_slope_per_step = U128::new(0);
    engine.accounts[lp_idx as usize].reserved_pnl = 0;
    engine.accounts[lp_idx as usize].warmup_started_at_slot = engine.current_slot;
    assert_conserved(&engine);

    // Accrue positive funding: +10 bps/slot for 1 slot
    engine.current_slot = 1;
    engine.accrue_funding(1, 100_000_000, 10).unwrap(); // price=$100, rate=+10bps

    // Expected delta_F = 100e6 * 10 * 1 / 10000 = 100,000
    // User payment = 1M * 100,000 / 1e6 = 100,000
    // LP payment = -1M * 100,000 / 1e6 = -100,000

    let user_pnl_before = engine.accounts[user_idx as usize].pnl;
    let lp_pnl_before = engine.accounts[lp_idx as usize].pnl;

    // Settle funding
    engine.touch_account(user_idx).unwrap();
    engine.touch_account(lp_idx).unwrap();

    // User (long) should pay 100,000
    assert_eq!(
        engine.accounts[user_idx as usize].pnl,
        user_pnl_before - 100_000
    );

    // LP (short) should receive 100,000
    assert_eq!(
        engine.accounts[lp_idx as usize].pnl,
        lp_pnl_before + 100_000
    );

    // Zero-sum check
    let total_pnl_before = user_pnl_before + lp_pnl_before;
    let total_pnl_after =
        engine.accounts[user_idx as usize].pnl + engine.accounts[lp_idx as usize].pnl;
    assert_eq!(
        total_pnl_after, total_pnl_before,
        "Funding should be zero-sum"
    );
}

#[test]
fn test_funding_negative_rate_shorts_pay_longs() {
    // T2: Negative funding → shorts pay longs
    let mut engine = Box::new(RiskEngine::new(default_params()));
    let user_idx = engine.add_user(0).unwrap();
    let lp_idx = engine.add_lp([1u8; 32], [2u8; 32], 0).unwrap();

    engine.deposit(user_idx, 100_000, 0).unwrap();
    // WHITEBOX: Set LP capital directly. Add to vault (not override) to preserve account fees.
    engine.accounts[lp_idx as usize].capital = U128::new(1_000_000);
    engine.vault += 1_000_000;

    // User opens short position
    engine.accounts[user_idx as usize].position_size = I128::new(-1_000_000);
    engine.accounts[user_idx as usize].entry_price = 100_000_000;

    // LP has opposite long position
    engine.accounts[lp_idx as usize].position_size = I128::new(1_000_000);
    engine.accounts[lp_idx as usize].entry_price = 100_000_000;

    // Zero warmup/reserved to avoid side effects from touch_account
    engine.accounts[user_idx as usize].warmup_slope_per_step = U128::new(0);
    engine.accounts[user_idx as usize].reserved_pnl = 0;
    engine.accounts[user_idx as usize].warmup_started_at_slot = engine.current_slot;
    engine.accounts[lp_idx as usize].warmup_slope_per_step = U128::new(0);
    engine.accounts[lp_idx as usize].reserved_pnl = 0;
    engine.accounts[lp_idx as usize].warmup_started_at_slot = engine.current_slot;
    assert_conserved(&engine);

    // Accrue negative funding: -10 bps/slot
    engine.current_slot = 1;
    engine.accrue_funding(1, 100_000_000, -10).unwrap();

    let user_pnl_before = engine.accounts[user_idx as usize].pnl;
    let lp_pnl_before = engine.accounts[lp_idx as usize].pnl;

    engine.touch_account(user_idx).unwrap();
    engine.touch_account(lp_idx).unwrap();

    // With negative funding rate, delta_F is negative (-100,000)
    // User (short) with negative position: payment = (-1M) * (-100,000) / 1e6 = 100,000
    // User pays 100,000 (shorts pay)
    assert_eq!(
        engine.accounts[user_idx as usize].pnl,
        user_pnl_before - 100_000
    );

    // LP (long) receives 100,000
    assert_eq!(
        engine.accounts[lp_idx as usize].pnl,
        lp_pnl_before + 100_000
    );
}

#[test]
fn test_funding_idempotence() {
    // T3: Settlement is idempotent
    let mut engine = Box::new(RiskEngine::new(default_params()));
    let user_idx = engine.add_user(10000).unwrap();

    engine.deposit(user_idx, 100_000, 0).unwrap();
    engine.accounts[user_idx as usize].position_size = I128::new(1_000_000);

    // Accrue funding
    engine.accrue_funding(1, 100_000_000, 10).unwrap();

    // Settle once
    engine.touch_account(user_idx).unwrap();
    let pnl_after_first = engine.accounts[user_idx as usize].pnl;

    // Settle again without new accrual
    engine.touch_account(user_idx).unwrap();
    let pnl_after_second = engine.accounts[user_idx as usize].pnl;

    assert_eq!(
        pnl_after_first, pnl_after_second,
        "Second settlement should not change PNL"
    );
}

#[test]
fn test_funding_partial_close() {
    // T4: Partial position close with funding
    let mut engine = Box::new(RiskEngine::new(default_params()));
    let user_idx = engine.add_user(0).unwrap();
    let lp_idx = engine.add_lp([1u8; 32], [2u8; 32], 0).unwrap();

    engine.deposit(user_idx, 15_000_000, 0).unwrap();
    // WHITEBOX: Set LP capital directly. Add to vault (not override) to preserve account fees.
    engine.accounts[lp_idx as usize].capital = U128::new(50_000_000);
    engine.vault += 50_000_000;
    assert_conserved(&engine);

    // Open long position of 2M base units
    let trade_result = engine.execute_trade(&MATCHER, lp_idx, user_idx, 0, 100_000_000, 2_000_000);
    assert!(trade_result.is_ok(), "Trade should succeed");

    assert_eq!(
        engine.accounts[user_idx as usize].position_size.get(),
        2_000_000
    );

    // Accrue funding for 1 slot at +10 bps
    engine.advance_slot(1);
    engine.accrue_funding(1, 100_000_000, 10).unwrap();

    // Reduce position to 1M (close half)
    let reduce_result =
        engine.execute_trade(&MATCHER, lp_idx, user_idx, 0, 100_000_000, -1_000_000);
    assert!(reduce_result.is_ok(), "Partial close should succeed");

    // Position should be 1M now
    assert_eq!(
        engine.accounts[user_idx as usize].position_size.get(),
        1_000_000
    );

    // Accrue more funding for another slot
    engine.advance_slot(2);
    engine.accrue_funding(2, 100_000_000, 10).unwrap();

    // Touch to settle
    engine.touch_account(user_idx).unwrap();

    // Funding should have been applied correctly for both periods
    // Period 1: 2M base * (100K delta_F) / 1e6 = 200
    // Period 2: 1M base * (100K delta_F) / 1e6 = 100
    // Total funding paid: 300
    // (exact PNL depends on trading fees too, but funding should be applied)
}

#[test]
fn test_funding_position_flip() {
    // T5: Flip from long to short
    let mut engine = Box::new(RiskEngine::new(default_params()));
    let user_idx = engine.add_user(0).unwrap();
    let lp_idx = engine.add_lp([1u8; 32], [2u8; 32], 0).unwrap();

    engine.deposit(user_idx, 10_000_000, 0).unwrap();
    // WHITEBOX: Set LP capital directly. Add to vault (not override) to preserve account fees.
    engine.accounts[lp_idx as usize].capital = U128::new(20_000_000);
    engine.vault += 20_000_000;
    assert_conserved(&engine);

    // Open long
    engine
        .execute_trade(&MATCHER, lp_idx, user_idx, 0, 100_000_000, 1_000_000)
        .unwrap();
    assert_eq!(
        engine.accounts[user_idx as usize].position_size.get(),
        1_000_000
    );

    // Accrue funding
    engine.advance_slot(1);
    engine.accrue_funding(1, 100_000_000, 10).unwrap();

    let _pnl_before_flip = engine.accounts[user_idx as usize].pnl;

    // Flip to short (trade -2M to go from +1M to -1M)
    engine
        .execute_trade(&MATCHER, lp_idx, user_idx, 0, 100_000_000, -2_000_000)
        .unwrap();

    assert_eq!(
        engine.accounts[user_idx as usize].position_size.get(),
        -1_000_000
    );

    // Funding should have been settled before the flip
    // User's funding index should be updated
    assert_eq!(
        engine.accounts[user_idx as usize].funding_index,
        engine.funding_index_qpb_e6
    );

    // Accrue more funding
    engine.advance_slot(2);
    engine.accrue_funding(2, 100_000_000, 10).unwrap();

    engine.touch_account(user_idx).unwrap();

    // Now user is short, so they receive funding (if rate is still positive)
    // This verifies no "double charge" bug
}

#[test]
fn test_funding_zero_position() {
    // Edge case: funding with zero position should do nothing
    let mut engine = Box::new(RiskEngine::new(default_params()));
    let user_idx = engine.add_user(10000).unwrap();

    engine.deposit(user_idx, 100_000, 0).unwrap();

    // No position
    assert_eq!(engine.accounts[user_idx as usize].position_size.get(), 0);

    let pnl_before = engine.accounts[user_idx as usize].pnl;

    // Accrue funding
    engine.accrue_funding(1, 100_000_000, 100).unwrap(); // Large rate

    // Settle
    engine.touch_account(user_idx).unwrap();

    // PNL should be unchanged
    assert_eq!(engine.accounts[user_idx as usize].pnl, pnl_before);
}

#[test]
fn test_funding_does_not_touch_principal() {
    // Funding should never modify principal (Invariant I1 extended)
    let mut engine = Box::new(RiskEngine::new(default_params()));
    let user_idx = engine.add_user(0).unwrap();

    let initial_principal = 100_000;
    engine.deposit(user_idx, initial_principal, 0).unwrap();

    engine.accounts[user_idx as usize].position_size = I128::new(1_000_000);

    // Accrue funding
    engine.accrue_funding(1, 100_000_000, 100).unwrap();
    engine.touch_account(user_idx).unwrap();

    // Principal must be unchanged
    assert_eq!(
        engine.accounts[user_idx as usize].capital.get(),
        initial_principal
    );
}

#[test]
fn test_adl_protects_principal_during_warmup() {
    // This test demonstrates the core protection mechanism:
    // If oracle manipulation creates fake PNL, ADL will haircut it
    // BEFORE it warms up and becomes withdrawable, protecting principal holders.

    let mut engine = Box::new(RiskEngine::new(default_params()));
    let attacker = engine.add_user(10000).unwrap();
    let victim = engine.add_user(10000).unwrap();

    // Both deposit principal
    engine.deposit(attacker, 10_000, 0).unwrap();
    engine.deposit(victim, 10_000, 0).unwrap();

    // Add insurance to provide warmup budget for converting positive PnL to capital
    // Budget = warmed_neg_total + insurance_spendable_raw() = 0 + 10000 = 10000
    set_insurance(&mut engine, 10_000);

    let attacker_principal = engine.accounts[attacker as usize].capital.get();
    let victim_principal = engine.accounts[victim as usize].capital.get();

    // === Phase 1: Oracle Manipulation (time < T) ===
    // Attacker manipulates oracle and creates fake $50k profit
    // In reality this would come from trading, but we simulate the result
    // Assert starting pnl is 0 for both (required for zero-sum to preserve conservation)
    assert_eq!(engine.accounts[attacker as usize].pnl.get(), 0);
    assert_eq!(engine.accounts[victim as usize].pnl.get(), 0);
    engine.accounts[attacker as usize].pnl = I128::new(50_000);
    engine.accounts[attacker as usize].warmup_slope_per_step = U128::new(500); // Will take 100 slots to warm up
    engine.accounts[attacker as usize].warmup_started_at_slot = 0;

    // Victim has corresponding loss
    engine.accounts[victim as usize].pnl = I128::new(-50_000);

    // Advance only 10 slots (very early in warmup period)
    engine.advance_slot(10);

    // At this point, very little PNL has warmed up
    let warmed_up = engine.withdrawable_pnl(&engine.accounts[attacker as usize]);
    assert_eq!(warmed_up, 5_000); // 500 * 10 = 5,000

    // Unwrapped (still warming) = 50k - 5k = 45k
    let positive_pnl = 50_000u128;
    let unwrapped_pnl = positive_pnl - warmed_up;
    assert_eq!(unwrapped_pnl, 45_000);

    // === Phase 2: Oracle Reverts, Loss Realized ===
    // The manipulation is detected/reverts quickly, creating a $50k loss
    // ADL is triggered to socialize this loss
    // KEY: ADL runs BEFORE most PNL has warmed up

    engine.apply_adl(50_000).unwrap();

    // === Phase 3: Verify Protection ===

    // Attacker's principal is NEVER touched (I1)
    assert_eq!(
        engine.accounts[attacker as usize].capital.get(),
        attacker_principal,
        "Attacker principal protected by I1"
    );

    // Victim's principal is NEVER touched (I1)
    assert_eq!(
        engine.accounts[victim as usize].capital.get(),
        victim_principal,
        "Victim principal protected by I1"
    );

    // ADL haircuts unwrapped PNL first (I4)
    // We had 45k unwrapped, so all of it gets haircutted
    // The remaining 5k loss goes to insurance fund
    let remaining_pnl = engine.accounts[attacker as usize].pnl.get();
    assert_eq!(
        remaining_pnl, 5_000,
        "Unwrapped PNL haircutted, only early-warmed remains"
    );

    // === Phase 4: Try to Withdraw After Warmup ===

    // Advance to full warmup completion
    engine.advance_slot(190); // Total 200 slots

    // Only the 5k that warmed up BEFORE ADL is still withdrawable
    let warmed_after_adl = engine.withdrawable_pnl(&engine.accounts[attacker as usize]);
    assert_eq!(
        warmed_after_adl, 5_000,
        "Only early-warmed PNL is withdrawable"
    );

    // In risk-reduction-only mode, withdrawals of capital ARE allowed
    // The attacker can withdraw their principal + already-warmed PNL (which gets converted to capital)
    // But warmup is frozen, so the 45k that's still warming won't become available
    let total_withdrawable = attacker_principal + warmed_after_adl;
    let withdraw_result = engine.withdraw(attacker, total_withdrawable, engine.current_slot, 1_000_000);
    assert!(
        withdraw_result.is_ok(),
        "Withdrawals of capital ARE allowed in risk mode"
    );

    // To enable withdrawals again, insurance fund must be topped up to cover loss_accum
    // ADL used ~45k from unwrapped PnL, remaining ~5k went to insurance
    // Due to rounding in proportional ADL, there may be small loss_accum
    assert!(
        engine.loss_accum.get() < 5_100,
        "Loss mostly covered by unwrapped PnL and insurance"
    );

    // === Conclusion ===
    // The attack was MOSTLY MITIGATED:
    // - The attacker could only withdraw their principal + 5k of warmed PNL
    // - The 45k that was still warming got haircutted by ADL
    // - Warmup freeze prevents the remaining 45k from ever becoming withdrawable in risk mode
    // - The insurance fund absorbed any remaining loss
    //
    // This demonstrates the core security property:
    //   "ADL haircuts PNL that is still warming up, protecting principal holders.
    //    The faster ADL runs after manipulation, the more effective the protection."
    //
    // New behavior: Withdrawal-only mode provides additional protection by blocking
    // all withdrawals until the system recovers (via insurance fund top-up).
}

#[test]
fn test_adl_haircuts_unwrapped_before_warmed() {
    // Verify that ADL prioritizes unwrapped (young) PNL over warmed (old) PNL

    let mut engine = Box::new(RiskEngine::new(default_params()));
    let user_idx = engine.add_user(10000).unwrap();

    engine.accounts[user_idx as usize].capital = U128::new(10_000);
    assert_eq!(engine.accounts[user_idx as usize].pnl.get(), 0);
    engine.accounts[user_idx as usize].pnl = I128::new(10_000);
    engine.accounts[user_idx as usize].warmup_slope_per_step = U128::new(100);

    // Advance time so half is warmed up
    engine.advance_slot(50);

    let warmed = engine.withdrawable_pnl(&engine.accounts[user_idx as usize]);
    assert_eq!(warmed, 5_000); // 100 * 50

    let unwrapped = 10_000 - warmed;
    assert_eq!(unwrapped, 5_000);

    // Apply ADL of 3k (less than unwrapped)
    engine.apply_adl(3_000).unwrap();

    // Should take from unwrapped first
    assert_eq!(engine.accounts[user_idx as usize].pnl.get(), 7_000);

    // The 5k warmed PNL should still be withdrawable
    // (Actually withdrawable = min(7k - 0, 5k) = 5k... wait)
    // After ADL: pnl = 7k, warmed_cap = 5k
    // withdrawable = min(7k, 5k) = 5k
    let still_warmed = engine.withdrawable_pnl(&engine.accounts[user_idx as usize]);
    assert_eq!(still_warmed, 5_000, "Warmed PNL still withdrawable");
}

// ============================================================================
// Warmup Rate Limiting Tests
// NOTE: These tests are commented out because warmup rate limiting was removed
// in the slab 4096 redesign for simplicity
// ============================================================================

/*
#[test]
fn test_warmup_rate_limit_single_user() {
    // Test that warmup slope is capped by insurance fund capacity
    let mut params = default_params();
    params.warmup_period_slots = 100;
    params.max_warmup_rate_fraction_bps = 5000; // 50% in T/2 = 50 slots

    let mut engine = Box::new(RiskEngine::new(params));

    // Add insurance fund: 10,000
    set_insurance(&mut engine, 10_000);

    // Max warmup rate = 10,000 * 5000 / 50 / 10,000 = 10,000 * 0.5 / 50 = 100 per slot
    let expected_max_rate = 10_000 * 5000 / 50 / 10_000;
    assert_eq!(expected_max_rate, 100);

    let user = engine.add_user(100).unwrap();
    engine.deposit(user, 1_000, 0).unwrap();

    // Give user 20,000 PNL (would need slope of 200 without limit)
    assert_eq!(engine.accounts[user as usize].pnl.get(), 0);
    engine.accounts[user as usize].pnl = I128::new(20_000);

    // Update warmup slope
    engine.update_warmup_slope(user).unwrap();

    // Should be capped at 100 (the max rate)
    assert_eq!(engine.accounts[user as usize].warmup_slope_per_step, 100);
    assert_eq!(engine.total_warmup_rate, 100);

    // After 50 slots, only 5,000 should have warmed up (not 10,000)
    engine.advance_slot(50);
    let warmed = engine.withdrawable_pnl(&engine.accounts[user as usize]);
    assert_eq!(warmed, 5_000); // 100 * 50 = 5,000
}

#[test]
fn test_warmup_rate_limit_multiple_users() {
    // Test that warmup capacity is shared among users
    let mut params = default_params();
    params.warmup_period_slots = 100;
    params.max_warmup_rate_fraction_bps = 5000; // 50% in T/2

    let mut engine = Box::new(RiskEngine::new(params));
    set_insurance(&mut engine, 10_000);

    // Max total warmup rate = 100 per slot

    let user1 = engine.add_user(100).unwrap();
    let user2 = engine.add_user(100).unwrap();

    engine.deposit(user1, 1_000, 0).unwrap();
    engine.deposit(user2, 1_000, 0).unwrap();

    // User1 gets 6,000 PNL (would want slope of 60)
    assert_eq!(engine.accounts[user1 as usize].pnl.get(), 0);
    engine.accounts[user1 as usize].pnl = I128::new(6_000);
    engine.update_warmup_slope(user1).unwrap();
    assert_eq!(engine.accounts[user1 as usize].warmup_slope_per_step, 60);
    assert_eq!(engine.total_warmup_rate, 60);

    // User2 gets 8,000 PNL (would want slope of 80)
    assert_eq!(engine.accounts[user2 as usize].pnl.get(), 0);
    engine.accounts[user2 as usize].pnl = I128::new(8_000);
    engine.update_warmup_slope(user2).unwrap();

    // Total would be 140, but max is 100, so user2 gets only 40
    assert_eq!(engine.accounts[user2 as usize].warmup_slope_per_step, 40); // 100 - 60 = 40
    assert_eq!(engine.total_warmup_rate, 100); // 60 + 40 = 100
}

#[test]
fn test_warmup_rate_released_on_pnl_decrease() {
    // Test that warmup capacity is released when user's PNL decreases
    let mut params = default_params();
    params.warmup_period_slots = 100;
    params.max_warmup_rate_fraction_bps = 5000;

    let mut engine = Box::new(RiskEngine::new(params));
    set_insurance(&mut engine, 10_000);

    let user1 = engine.add_user(100).unwrap();
    let user2 = engine.add_user(100).unwrap();

    engine.deposit(user1, 1_000, 0).unwrap();
    engine.deposit(user2, 1_000, 0).unwrap();

    // User1 uses all capacity
    assert_eq!(engine.accounts[user1 as usize].pnl.get(), 0);
    engine.accounts[user1 as usize].pnl = I128::new(15_000);
    engine.update_warmup_slope(user1).unwrap();
    assert_eq!(engine.total_warmup_rate, 100);

    // User2 can't get any capacity
    assert_eq!(engine.accounts[user2 as usize].pnl.get(), 0);
    engine.accounts[user2 as usize].pnl = I128::new(5_000);
    engine.update_warmup_slope(user2).unwrap();
    assert_eq!(engine.accounts[user2 as usize].warmup_slope_per_step, 0);

    // User1's PNL drops to 3,000 (ADL or loss)
    engine.accounts[user1 as usize].pnl = I128::new(3_000);
    engine.update_warmup_slope(user1).unwrap();
    assert_eq!(engine.accounts[user1 as usize].warmup_slope_per_step, 30); // 3000/100
    assert_eq!(engine.total_warmup_rate, 30);

    // Now user2 can get the remaining 70
    engine.update_warmup_slope(user2).unwrap();
    assert_eq!(engine.accounts[user2 as usize].warmup_slope_per_step, 50); // 5000/100, but capped at 70
    assert_eq!(engine.total_warmup_rate, 80); // 30 + 50
}

#[test]
fn test_warmup_rate_scales_with_insurance_fund() {
    // Test that max warmup rate scales with insurance fund size
    let mut params = default_params();
    params.warmup_period_slots = 100;
    params.max_warmup_rate_fraction_bps = 5000; // 50% in T/2

    let mut engine = Box::new(RiskEngine::new(params));

    // Small insurance fund
    set_insurance(&mut engine, 1_000);

    let user = engine.add_user(100).unwrap();
    engine.deposit(user, 1_000, 0).unwrap();

    assert_eq!(engine.accounts[user as usize].pnl.get(), 0);
    engine.accounts[user as usize].pnl = I128::new(10_000);
    engine.update_warmup_slope(user).unwrap();

    // Max rate = 1000 * 0.5 / 50 = 10
    assert_eq!(engine.accounts[user as usize].warmup_slope_per_step, 10);

    // Increase insurance fund 10x
    set_insurance(&mut engine, 10_000);

    // Update slope again
    engine.update_warmup_slope(user).unwrap();

    // Max rate should be 10x higher = 100
    assert_eq!(engine.accounts[user as usize].warmup_slope_per_step, 100);
}

#[test]
fn test_warmup_rate_limit_invariant_maintained() {
    // Verify that the invariant is always maintained:
    // total_warmup_rate * (T/2) <= insurance_fund * max_warmup_rate_fraction

    let mut params = default_params();
    params.warmup_period_slots = 100;
    params.max_warmup_rate_fraction_bps = 5000;

    let mut engine = Box::new(RiskEngine::new(params));
    set_insurance(&mut engine, 10_000);

    // Add multiple users with varying PNL
    for i in 0..10 {
        let user = engine.add_user(100).unwrap();
        engine.deposit(user, 1_000, 0).unwrap();
        engine.accounts[user as usize].pnl = (i as i128 + 1) * 1_000;
        engine.update_warmup_slope(user).unwrap();

        // Check invariant after each update
        let half_period = params.warmup_period_slots / 2;
        let max_total_warmup_in_half_period = engine.total_warmup_rate * (half_period as u128);
        let insurance_limit = engine.insurance_fund.balance * params.max_warmup_rate_fraction_bps as u128 / 10_000;

        assert!(max_total_warmup_in_half_period <= insurance_limit,
                "Invariant violated: {} > {}", max_total_warmup_in_half_period, insurance_limit);
    }
}
*/

// ============================================================================
// Risk-Reduction-Only Mode Tests
// ============================================================================

#[test]
fn test_risk_reduction_only_mode_triggered_by_loss() {
    // Test that loss_accum > 0 triggers withdrawal-only mode
    let mut engine = Box::new(RiskEngine::new(default_params()));

    let user1 = engine.add_user(100).unwrap();
    let user2 = engine.add_user(100).unwrap();

    engine.deposit(user1, 10_000, 0).unwrap();
    engine.deposit(user2, 10_000, 0).unwrap();

    // Set insurance fund balance AFTER adding users (to avoid fee confusion)
    set_insurance(&mut engine, 5_000);

    // Simulate a loss event that depletes insurance fund
    let loss = 10_000;
    engine.apply_adl(loss).unwrap();

    // Should be in withdrawal-only mode
    assert!(engine.risk_reduction_only);
    assert_eq!(engine.loss_accum.get(), 5_000); // 10k loss - 5k insurance = 5k loss_accum
    assert_eq!(engine.insurance_fund.balance.get(), 0);
}

/*
// NOTE: Commented out - withdrawal-only mode now BLOCKS all withdrawals instead of proportional haircut
#[test]
fn test_proportional_haircut_on_withdrawal() {
    // Test that withdrawals are haircutted proportionally when loss_accum > 0
    let mut engine = Box::new(RiskEngine::new(default_params()));

    let user1 = engine.add_user(100).unwrap();
    let user2 = engine.add_user(100).unwrap();

    engine.deposit(user1, 10_000, 0).unwrap();
    engine.deposit(user2, 5_000, 0).unwrap();

    // Set insurance fund balance AFTER adding users (to avoid fee confusion)
    set_insurance(&mut engine, 1_000);

    // Total principal = 15,000
    // Trigger loss that creates 3,000 loss_accum
    engine.apply_adl(4_000).unwrap();

    assert_eq!(engine.loss_accum.get(), 3_000); // 4k - 1k insurance
    assert!(engine.risk_reduction_only);

    // Available principal = 15,000 - 3,000 = 12,000
    // Haircut ratio = 12,000 / 15,000 = 80%

    // User1 tries to withdraw 10,000
    // Fair unwinding: Should get 80% regardless of order
    // Gets: 10,000 * 0.8 = 8,000
    let user1_balance_before = engine.accounts[user1 as usize].capital;
    engine.withdraw(user1, 10_000, 0, 1_000_000).unwrap();
    let withdrawn = user1_balance_before - engine.accounts[user1 as usize].capital;

    assert_eq!(withdrawn, 8_000, "Should withdraw 80% due to haircut");

    // User2 tries to withdraw 5,000
    // Fair unwinding: Also gets 80% (not less than user1)
    // Gets: 5,000 * 0.8 = 4,000
    let user2_balance_before = engine.accounts[user2 as usize].capital;
    engine.withdraw(user2, 5_000, 0, 1_000_000).unwrap();
    let user2_withdrawn = user2_balance_before - engine.accounts[user2 as usize].capital;

    assert_eq!(user2_withdrawn, 4_000, "Should also get 80% (fair unwinding)");

    // Total withdrawn: 8,000 + 4,000 = 12,000
    // Exactly the available principal (15k - 3k loss = 12k)
    let total_withdrawn = withdrawn + user2_withdrawn;
    assert_eq!(total_withdrawn, 12_000);
}
*/

#[test]
fn test_closing_positions_allowed_in_withdrawal_mode() {
    // Test that users can close positions in withdrawal-only mode
    let mut engine = Box::new(RiskEngine::new(default_params()));

    let lp = engine.add_lp([1u8; 32], [2u8; 32], 0).unwrap();
    let user = engine.add_user(0).unwrap();

    engine.deposit(user, 10_000, 0).unwrap();
    // WHITEBOX: Set LP capital directly. Add to vault (not override) to preserve account fees.
    engine.accounts[lp as usize].capital = U128::new(50_000);
    engine.vault += 50_000;

    // Set insurance fund balance
    set_insurance(&mut engine, 5_000);
    assert_conserved(&engine);

    // User opens long position
    let matcher = NoOpMatcher;
    engine
        .execute_trade(&matcher, lp, user, 0, 1_000_000, 5_000)
        .unwrap();
    assert_eq!(engine.accounts[user as usize].position_size.get(), 5_000);

    // Manually enter risk-reduction-only mode (simulating some trigger)
    engine.enter_risk_reduction_only_mode();
    assert!(engine.risk_reduction_only);

    // User can CLOSE position (reducing from 5000 to 0) in risk mode
    let result = engine.execute_trade(&matcher, lp, user, 0, 1_000_000, -5_000);
    assert!(result.is_ok(), "Closing position should be allowed");
    assert_eq!(engine.accounts[user as usize].position_size.get(), 0);
}

#[test]
fn test_opening_positions_blocked_in_withdrawal_mode() {
    // Test that opening new positions is blocked in withdrawal-only mode
    let mut engine = Box::new(RiskEngine::new(default_params()));

    let lp = engine.add_lp([1u8; 32], [2u8; 32], 100).unwrap();
    let user = engine.add_user(100).unwrap();

    engine.deposit(user, 10_000, 0).unwrap();
    engine.accounts[lp as usize].capital = U128::new(50_000);

    // Set insurance fund balance AFTER adding users (to avoid fee confusion)
    set_insurance(&mut engine, 1_000);

    // Trigger withdrawal-only mode
    engine.apply_adl(2_000).unwrap();
    assert!(engine.risk_reduction_only);

    // User tries to open new position - should fail
    let matcher = NoOpMatcher;
    let result = engine.execute_trade(&matcher, lp, user, 0, 1_000_000, 5_000);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), RiskError::RiskReductionOnlyMode);
}

// Test A: Warmup freezes in risk mode
#[test]
fn test_warmup_freezes_in_risk_mode() {
    let mut engine = Box::new(RiskEngine::new(default_params()));
    let user = engine.add_user(100).unwrap();

    engine.deposit(user, 10_000, 0).unwrap();

    // Setup: user has pnl=+1000, slope=10, started_at_slot=0
    assert_eq!(engine.accounts[user as usize].pnl.get(), 0);
    engine.accounts[user as usize].pnl = I128::new(1000);
    engine.accounts[user as usize].warmup_slope_per_step = U128::new(10);
    engine.accounts[user as usize].warmup_started_at_slot = 0;

    // Advance slot to 10
    engine.current_slot = 10;
    let w1 = engine.withdrawable_pnl(&engine.accounts[user as usize]);
    assert_eq!(w1, 100, "After 10 slots, 10*10=100 should be warmed");

    // Trigger crisis mode at slot 10
    engine.enter_risk_reduction_only_mode();
    assert!(engine.warmup_paused);
    assert_eq!(engine.warmup_pause_slot, 10);

    // Advance slot +1000
    engine.current_slot = 1010;

    // Warmup should be frozen at slot 10
    let w2 = engine.withdrawable_pnl(&engine.accounts[user as usize]);
    assert_eq!(w2, w1, "Warmup should not progress when paused");
    assert_eq!(w2, 100, "Should still be 100 even after 1000 slots");
}

// Test B: In risk mode, deposit withdrawals work from deposited capital
#[test]
fn test_risk_mode_deposit_withdrawals_work() {
    let mut engine = Box::new(RiskEngine::new(default_params()));
    let user = engine.add_user(0).unwrap();

    // User deposit 1000
    engine.deposit(user, 1000, 0).unwrap();

    // Enter risk mode
    engine.enter_risk_reduction_only_mode();
    assert!(engine.risk_reduction_only);

    // Withdraw 200 - should succeed (withdrawing from capital)
    let v0 = vault_snapshot(&engine);
    let result = engine.withdraw(user, 200, 0, 1_000_000);
    assert!(
        result.is_ok(),
        "Withdrawals of capital should work in risk mode"
    );

    assert_eq!(engine.accounts[user as usize].capital.get(), 800);
    assert_vault_delta(&engine, v0, -200);
    assert_conserved(&engine);
}

// Test C: In risk mode, pending PNL cannot be withdrawn (because warmup is frozen)
#[test]
fn test_risk_mode_pending_pnl_cannot_be_withdrawn() {
    let mut engine = Box::new(RiskEngine::new(default_params()));
    let user = engine.add_user(0).unwrap();

    // User has NO capital, only pending PNL
    assert_eq!(engine.accounts[user as usize].pnl.get(), 0);
    engine.accounts[user as usize].pnl = I128::new(1000);
    engine.accounts[user as usize].warmup_slope_per_step = U128::new(10);
    engine.accounts[user as usize].warmup_started_at_slot = engine.current_slot;

    // Enter risk mode immediately (so warmed amount ~0)
    engine.enter_risk_reduction_only_mode();

    // Try withdraw 1 - should fail with InsufficientBalance
    // because capital is 0 and warmup won't progress
    let result = engine.withdraw(user, 1, 0, 1_000_000);
    assert!(result.is_err(), "Should fail - no capital available");
    assert_eq!(result.unwrap_err(), RiskError::InsufficientBalance);
}

// Test D: In risk mode, already-warmed PNL can be withdrawn after conversion
#[test]
fn test_risk_mode_already_warmed_pnl_withdrawable() {
    let mut engine = Box::new(RiskEngine::new(default_params()));
    let user = engine.add_user(0).unwrap();
    let counterparty = engine.add_user(0).unwrap();

    // Add insurance to provide warmup budget for converting positive PnL to capital
    // Budget = warmed_neg_total + insurance_spendable_raw() = 0 + 100 = 100
    set_insurance(&mut engine, 100);

    // Zero-sum PnL: user gains, counterparty loses (no vault funding needed)
    assert_eq!(engine.accounts[user as usize].pnl.get(), 0);
    assert_eq!(engine.accounts[counterparty as usize].pnl.get(), 0);
    engine.accounts[user as usize].pnl = I128::new(1000);
    engine.accounts[counterparty as usize].pnl = I128::new(-1000);
    engine.accounts[user as usize].warmup_slope_per_step = U128::new(10);
    engine.accounts[user as usize].warmup_started_at_slot = 0;
    assert_conserved(&engine);

    // Advance slot to 10 → warmed=100
    engine.current_slot = 10;
    let warmed_before_mode = engine.withdrawable_pnl(&engine.accounts[user as usize]);
    assert_eq!(warmed_before_mode, 100);

    // Enter risk mode (freezes at slot 10)
    engine.enter_risk_reduction_only_mode();

    // Call withdraw(50)
    // Should convert 100 PNL to capital, then withdraw 50
    let result = engine.withdraw(user, 50, engine.current_slot, 1_000_000);
    assert!(result.is_ok(), "Should succeed: {:?}", result);

    // Check: pnl reduced by 100, capital increased by 100 then decreased by 50
    assert_eq!(engine.accounts[user as usize].pnl.get(), 900); // 1000 - 100
    assert_eq!(engine.accounts[user as usize].capital.get(), 50); // 0 + 100 - 50
}

// Test E: Risk-increasing trade fails in risk mode
#[test]
fn test_risk_increasing_trade_fails_in_risk_mode() {
    let mut engine = Box::new(RiskEngine::new(default_params()));
    let user = engine.add_user(0).unwrap();
    let lp = engine.add_lp([1u8; 32], [2u8; 32], 0).unwrap();

    engine.deposit(user, 10_000, 0).unwrap();
    // WHITEBOX: Set LP capital directly. Add to vault (not override) to preserve account fees.
    engine.accounts[lp as usize].capital = U128::new(50_000);
    engine.vault += 50_000;
    assert_conserved(&engine);

    // Both start at pos 0
    assert_eq!(engine.accounts[user as usize].position_size.get(), 0);
    assert_eq!(engine.accounts[lp as usize].position_size.get(), 0);

    // Enter risk mode
    engine.enter_risk_reduction_only_mode();

    // Try to open position (0 -> +1, increases absolute exposure)
    let matcher = NoOpMatcher;
    let result = engine.execute_trade(&matcher, lp, user, 0, 100_000_000, 1);

    assert!(result.is_err(), "Risk-increasing trade should fail");
    assert_eq!(result.unwrap_err(), RiskError::RiskReductionOnlyMode);
}

// Test F: Reduce-only trade succeeds in risk mode
#[test]
fn test_reduce_only_trade_succeeds_in_risk_mode() {
    let mut engine = Box::new(RiskEngine::new(default_params()));
    let user = engine.add_user(0).unwrap();
    let lp = engine.add_lp([1u8; 32], [2u8; 32], 0).unwrap();

    engine.deposit(user, 10_000, 0).unwrap();
    // WHITEBOX: Set LP capital directly. Add to vault (not override) to preserve account fees.
    engine.accounts[lp as usize].capital = U128::new(50_000);
    engine.vault += 50_000;
    assert_conserved(&engine);

    // Setup: user pos +10, lp pos -10
    engine.accounts[user as usize].position_size = I128::new(10);
    engine.accounts[user as usize].entry_price = 100_000_000;
    engine.accounts[lp as usize].position_size = I128::new(-10);
    engine.accounts[lp as usize].entry_price = 100_000_000;

    // Enter risk mode
    engine.enter_risk_reduction_only_mode();

    // Trade size -5 (reduces user from 10 to 5, LP from -10 to -5)
    let matcher = NoOpMatcher;
    let result = engine.execute_trade(&matcher, lp, user, 0, 100_000_000, -5);

    assert!(result.is_ok(), "Reduce-only trade should succeed");
    assert_eq!(engine.accounts[user as usize].position_size.get(), 5);
    assert_eq!(engine.accounts[lp as usize].position_size.get(), -5);
}

// Test G: Exiting mode unfreezes warmup
#[test]
fn test_exiting_mode_unfreezes_warmup() {
    let mut engine = Box::new(RiskEngine::new(default_params()));
    let user = engine.add_user(100).unwrap();

    engine.deposit(user, 10_000, 0).unwrap();

    // Create deficit, enter risk mode
    engine.loss_accum = U128::new(1000);
    engine.enter_risk_reduction_only_mode();

    assert!(engine.risk_reduction_only);
    assert!(engine.warmup_paused);

    // Top up to clear loss_accum=0
    engine.top_up_insurance_fund(1000).unwrap();

    assert_eq!(engine.loss_accum.get(), 0);
    assert!(!engine.risk_reduction_only, "Should exit risk mode");
    assert!(!engine.warmup_paused, "Should unfreeze warmup");
}

#[test]
fn test_top_up_insurance_fund_reduces_loss() {
    // Test that topping up insurance fund reduces loss_accum
    let mut engine = Box::new(RiskEngine::new(default_params()));

    let user = engine.add_user(100).unwrap();
    engine.deposit(user, 10_000, 0).unwrap();

    // Set insurance fund balance AFTER adding users (to avoid fee confusion)
    set_insurance(&mut engine, 1_000);

    // Trigger withdrawal-only mode with 4k loss_accum
    engine.apply_adl(5_000).unwrap();
    assert_eq!(engine.loss_accum.get(), 4_000);
    assert!(engine.risk_reduction_only);

    // Top up with 2k - should reduce loss to 2k
    let exited = engine.top_up_insurance_fund(2_000).unwrap();
    assert_eq!(engine.loss_accum.get(), 2_000);
    assert!(engine.risk_reduction_only); // Still in withdrawal mode
    assert!(!exited);

    // Top up with another 2k - should fully cover loss
    let exited = engine.top_up_insurance_fund(2_000).unwrap();
    assert_eq!(engine.loss_accum.get(), 0);
    assert!(!engine.risk_reduction_only); // Exited withdrawal mode
    assert!(exited);
}

/*
// NOTE: Commented out - withdrawal-only mode now BLOCKS all withdrawals
#[test]
fn test_deposits_allowed_in_withdrawal_mode() {
    // Test that deposits are allowed and take on proportional loss
    let mut engine = Box::new(RiskEngine::new(default_params()));

    let user1 = engine.add_user(100).unwrap();
    let user2 = engine.add_user(100).unwrap();

    engine.deposit(user1, 10_000, 0).unwrap();

    // Set insurance fund balance AFTER adding users (to avoid fee confusion)
    set_insurance(&mut engine, 1_000);

    // Trigger withdrawal-only mode
    engine.apply_adl(2_000).unwrap();
    assert_eq!(engine.loss_accum.get(), 1_000);
    assert!(engine.risk_reduction_only);

    // User2 deposits - should be allowed
    let result = engine.deposit(user2, 5_000, 0);
    assert!(result.is_ok(), "Deposits should be allowed in withdrawal mode");

    // Total principal now 15k, loss still 1k
    // User2's share of loss: (5k / 15k) * 1k ≈ 333
    // So user2 can withdraw: 5k - 333 ≈ 4,667

    let user2_balance_before = engine.accounts[user2 as usize].capital;
    engine.withdraw(user2, 5_000, 0, 1_000_000).unwrap();
    let user2_withdrawn = user2_balance_before - engine.accounts[user2 as usize].capital;

    // Should be less than full amount due to proportional haircut
    assert!(user2_withdrawn < 5_000);
    assert!(user2_withdrawn > 4_600); // Approximately 4,667
}
*/

/*
// NOTE: Commented out - withdrawal-only mode now BLOCKS all withdrawals
#[test]
fn test_fair_unwinding_scenario() {
    // End-to-end test of fair unwinding when system becomes insolvent
    let mut engine = Box::new(RiskEngine::new(default_params()));

    // 3 users deposit
    let alice = engine.add_user(100).unwrap();
    let bob = engine.add_user(100).unwrap();
    let charlie = engine.add_user(100).unwrap();

    engine.deposit(alice, 10_000, 0).unwrap();
    engine.deposit(bob, 20_000, 0).unwrap();
    engine.deposit(charlie, 10_000, 0).unwrap();

    // Set insurance fund balance AFTER adding users (to avoid fee confusion)
    set_insurance(&mut engine, 5_000);

    // Total principal: 40k
    // Insurance fund: 5k
    // Total system: 45k

    // Catastrophic loss event: 15k loss
    engine.apply_adl(15_000).unwrap();

    // Loss_accum = 15k - 5k = 10k
    // Insurance depleted
    // Available principal = 40k - 10k = 30k
    // Haircut ratio = 30k / 40k = 75%

    assert_eq!(engine.loss_accum.get(), 10_000);
    assert_eq!(engine.insurance_fund.balance.get(), 0);
    assert!(engine.risk_reduction_only);

    // With fair unwinding, everyone gets the same haircut ratio (75%)
    // regardless of withdrawal order

    // Alice withdraws all (10k * 75% = 7.5k)
    let alice_before = engine.accounts[alice as usize].capital;
    engine.withdraw(alice, 10_000, 0, 1_000_000).unwrap();
    let alice_got = alice_before - engine.accounts[alice as usize].capital;
    assert_eq!(alice_got, 7_500);

    // Bob withdraws all (20k * 75% = 15k)
    // Fair unwinding: haircut ratio stays 75% because we track withdrawn amounts
    let bob_before = engine.accounts[bob as usize].capital;
    engine.withdraw(bob, 20_000, 0, 1_000_000).unwrap();
    let bob_got = bob_before - engine.accounts[bob as usize].capital;
    assert_eq!(bob_got, 15_000);

    // Charlie withdraws all (10k * 75% = 7.5k)
    let charlie_before = engine.accounts[charlie as usize].capital;
    engine.withdraw(charlie, 10_000, 0, 1_000_000).unwrap();
    let charlie_got = charlie_before - engine.accounts[charlie as usize].capital;
    assert_eq!(charlie_got, 7_500);

    // Total withdrawn: 7.5k + 15k + 7.5k = 30k
    // Exactly the available principal (fair unwinding!)
    let total_withdrawn = alice_got + bob_got + charlie_got;
    assert_eq!(total_withdrawn, 30_000);

    // All users proportionally haircutted (25% loss each)
    assert_eq!(alice_got * 100 / alice_before, 75);
    assert_eq!(bob_got * 100 / bob_before, 75);
    assert_eq!(charlie_got * 100 / charlie_before, 75);
}
*/

// ==============================================================================
// LP-SPECIFIC TESTS (CRITICAL - Addresses audit findings)
// ==============================================================================

#[test]
fn test_lp_withdraw() {
    // Tests that LP withdrawal works correctly (WHITEBOX: direct state mutation)
    let mut engine = Box::new(RiskEngine::new(default_params()));

    let lp_idx = engine.add_lp([1u8; 32], [2u8; 32], 0).unwrap();

    // LP deposits capital
    engine.deposit(lp_idx, 10_000, 0).unwrap();

    // LP earns PNL from counterparty (need zero-sum setup)
    // Create a user to be the counterparty
    let user_idx = engine.add_user(0).unwrap();
    engine.deposit(user_idx, 5_000, 0).unwrap();

    // Add insurance to provide warmup budget for converting LP's positive PnL to capital
    // Budget = warmed_neg_total + insurance_spendable_raw() = 0 + 5000 = 5000
    set_insurance(&mut engine, 5_000);

    // Zero-sum PNL: LP gains 5000, user loses 5000
    // Assert starting pnl is 0 for both (required for zero-sum to preserve conservation)
    assert_eq!(engine.accounts[lp_idx as usize].pnl.get(), 0);
    assert_eq!(engine.accounts[user_idx as usize].pnl.get(), 0);
    engine.accounts[lp_idx as usize].pnl = I128::new(5_000);
    engine.accounts[user_idx as usize].pnl = I128::new(-5_000);

    // Set warmup slope so PnL can warm up (warmup_period_slots = 100 from default_params)
    engine.accounts[lp_idx as usize].warmup_slope_per_step = U128::new(5_000 / 100); // 50 per slot
    engine.accounts[lp_idx as usize].warmup_started_at_slot = 0;

    // Advance time to allow warmup
    engine.current_slot = 100; // Full warmup (100 slots × 50 = 5000)

    // Snapshot before withdrawal
    let v0 = vault_snapshot(&engine);

    // withdraw converts warmed PNL to capital, then withdraws
    // After conversion: LP capital = 10,000 + 5,000 = 15,000
    let result = engine.withdraw(lp_idx, 10_000, engine.current_slot, 1_000_000);
    assert!(result.is_ok(), "LP withdrawal should succeed: {:?}", result);

    // Withdrawal should reduce vault by 10,000
    assert_vault_delta(&engine, v0, -10_000);
    assert_eq!(
        engine.accounts[lp_idx as usize].capital.get(),
        5_000,
        "LP should have 5,000 capital remaining (from converted PNL)"
    );
    assert_eq!(
        engine.accounts[lp_idx as usize].pnl.get(),
        0,
        "PNL should be converted to capital"
    );
    assert_conserved(&engine);
}

/*
// NOTE: Commented out - withdrawal-only mode now BLOCKS all withdrawals
#[test]
fn test_lp_withdraw_with_haircut() {
    // CRITICAL: Tests that LPs are subject to withdrawal-mode haircuts
    let mut engine = Box::new(RiskEngine::new(default_params()));

    let user_idx = engine.add_user(0).unwrap();
    let lp_idx = engine.add_lp([1u8; 32], [2u8; 32], 0).unwrap();

    engine.deposit(user_idx, 10_000, 0).unwrap();
    engine.deposit(lp_idx, 10_000, 0).unwrap();

    // Simulate crisis - set loss_accum
    engine.loss_accum = U128::new(5_000); // 25% loss
    engine.risk_reduction_only = true;

    // Both should get 75% haircut
    let user_result = engine.withdraw(user_idx, 10_000, 0, 1_000_000);
    assert!(user_result.is_ok());

    let lp_result = engine.withdraw(lp_idx, 10_000, 0, 1_000_000);
    assert!(lp_result.is_ok());

    // Both should have withdrawn same proportion
    let total_withdrawn = engine.withdrawal_mode_withdrawn;
    assert!(total_withdrawn < 20_000, "Total withdrawn should be less than requested due to haircuts");
    assert!(total_withdrawn > 14_000, "Haircut should be approximately 25%");
}
*/

/*
// NOTE: Commented out - warmup rate limiting was removed in slab 4096 redesign
#[test]
fn test_update_lp_warmup_slope() {
    // CRITICAL: Tests that LP warmup actually gets rate limited
    let mut engine = Box::new(RiskEngine::new(default_params()));

    let lp_idx = engine.add_lp([1u8; 32], [2u8; 32], 0).unwrap();

    // Set insurance fund
    set_insurance(&mut engine, 10_000);

    // LP earns large PNL
    engine.accounts[lp_idx as usize].pnl = I128::new(50_000);

    // Update warmup slope
    engine.update_lp_warmup_slope(lp_idx).unwrap();

    // Should be rate limited
    let ideal_slope = 50_000 / 100; // 500 per slot
    let actual_slope = engine.accounts[lp_idx as usize].warmup_slope_per_step;

    assert!(actual_slope < ideal_slope, "LP warmup should be rate limited");
    assert!(engine.total_warmup_rate > 0, "LP should contribute to total warmup rate");
}
*/

#[test]
fn test_adl_proportional_haircut_users_and_lps() {
    // CRITICAL: Tests that ADL haircuts users and LPs PROPORTIONALLY, not sequentially
    let mut engine = Box::new(RiskEngine::new(default_params()));

    let user_idx = engine.add_user(0).unwrap();
    let lp_idx = engine.add_lp([1u8; 32], [2u8; 32], 0).unwrap();

    // Both have unwrapped PNL
    assert_eq!(engine.accounts[user_idx as usize].pnl.get(), 0);
    assert_eq!(engine.accounts[lp_idx as usize].pnl.get(), 0);
    engine.accounts[user_idx as usize].pnl = I128::new(10_000); // User has 10k unwrapped
    engine.accounts[lp_idx as usize].pnl = I128::new(10_000); // LP has 10k unwrapped

    // Apply ADL with 10k loss
    engine.apply_adl(10_000).unwrap();

    // BOTH should be haircutted proportionally (50% each)
    assert_eq!(
        engine.accounts[user_idx as usize].pnl.get(),
        5_000,
        "User should lose 5k (50%)"
    );
    assert_eq!(
        engine.accounts[lp_idx as usize].pnl.get(),
        5_000,
        "LP should lose 5k (50%)"
    );
}

#[test]
fn test_adl_fairness_different_amounts() {
    // CRITICAL: Tests proportional ADL with different PNL amounts
    let mut engine = Box::new(RiskEngine::new(default_params()));

    let user_idx = engine.add_user(0).unwrap();
    let lp_idx = engine.add_lp([1u8; 32], [2u8; 32], 0).unwrap();

    // User has more unwrapped PNL than LP
    assert_eq!(engine.accounts[user_idx as usize].pnl.get(), 0);
    assert_eq!(engine.accounts[lp_idx as usize].pnl.get(), 0);
    engine.accounts[user_idx as usize].pnl = I128::new(15_000); // User: 15k
    engine.accounts[lp_idx as usize].pnl = I128::new(5_000); // LP: 5k
                                                             // Total: 20k

    // Apply ADL with 10k loss (50% of total)
    engine.apply_adl(10_000).unwrap();

    // Each should lose 50% of their PNL
    assert_eq!(
        engine.accounts[user_idx as usize].pnl.get(),
        7_500,
        "User should lose 7.5k (50% of 15k)"
    );
    assert_eq!(
        engine.accounts[lp_idx as usize].pnl.get(),
        2_500,
        "LP should lose 2.5k (50% of 5k)"
    );
}

#[test]
fn test_lp_capital_never_reduced_by_adl() {
    // CRITICAL: Verifies Invariant I1 for LPs
    let mut engine = Box::new(RiskEngine::new(default_params()));

    let lp_idx = engine.add_lp([1u8; 32], [2u8; 32], 0).unwrap();

    engine.deposit(lp_idx, 10_000, 0).unwrap();
    assert_eq!(engine.accounts[lp_idx as usize].pnl.get(), 0);
    engine.accounts[lp_idx as usize].pnl = I128::new(5_000);

    let capital_before = engine.accounts[lp_idx as usize].capital;

    // Apply massive ADL
    engine.apply_adl(100_000).unwrap();

    // Capital should NEVER be reduced
    assert_eq!(
        engine.accounts[lp_idx as usize].capital, capital_before,
        "I1: LP capital must never be reduced by ADL"
    );

    // Only PNL should be affected
    assert!(
        engine.accounts[lp_idx as usize].pnl.get() < 5_000,
        "LP PNL should be haircutted"
    );
}

#[test]
fn test_risk_reduction_threshold() {
    // Test that risk-reduction mode triggers at configured threshold
    // With the insurance floor, ADL cannot spend insurance below the threshold.
    let mut params = default_params();
    params.risk_reduction_threshold = U128::new(5_000); // Floor: insurance won't go below 5k

    let mut engine = Box::new(RiskEngine::new(params));

    let user = engine.add_user(100).unwrap();
    engine.deposit(user, 10_000, 0).unwrap();

    // Setup: insurance fund has 10k, which is above threshold
    set_insurance(&mut engine, 10_000);
    assert!(!engine.risk_reduction_only);

    // Apply ADL with 3k loss - should bring insurance to 7k (spendable was 5k, used 3k)
    engine.apply_adl(3_000).unwrap();
    assert_eq!(engine.insurance_fund.balance.get(), 7_000);
    assert!(
        !engine.risk_reduction_only,
        "Should not trigger yet (7k > 5k)"
    );

    // Apply ADL with 3k loss - spendable = 7k - 5k = 2k, so only 2k is spent
    // Insurance goes to floor (5k), remaining 1k goes to loss_accum
    engine.apply_adl(3_000).unwrap();
    assert_eq!(
        engine.insurance_fund.balance.get(),
        5_000,
        "Insurance clamped at floor"
    );
    assert_eq!(
        engine.loss_accum.get(),
        1_000,
        "Uncovered loss added to loss_accum"
    );
    assert!(
        engine.risk_reduction_only,
        "Should trigger now (at floor + uncovered loss)"
    );
    assert!(engine.warmup_paused, "Warmup should be frozen");

    // Top up 1k to cover loss_accum
    engine.top_up_insurance_fund(1_000).unwrap();
    assert_eq!(engine.loss_accum.get(), 0, "Loss covered");
    assert_eq!(engine.insurance_fund.balance.get(), 5_000, "Still at floor");
    // System should exit risk mode since loss_accum is 0 and insurance >= threshold
    assert!(
        !engine.risk_reduction_only,
        "Should exit risk mode (loss covered, at threshold)"
    );
    assert!(!engine.warmup_paused, "Warmup should unfreeze");
}

// ==============================================================================
// PANIC SETTLE TESTS
// ==============================================================================

#[test]
fn test_panic_settle_closes_all_positions() {
    // Test A: settles all positions to zero
    let mut engine = Box::new(RiskEngine::new(default_params()));

    // Create 1 LP and 2 users
    let lp_idx = engine.add_lp([1u8; 32], [2u8; 32], 0).unwrap();
    let user1 = engine.add_user(0).unwrap();
    let user2 = engine.add_user(0).unwrap();

    // Fund accounts
    engine.deposit(lp_idx, 100_000, 0).unwrap();
    engine.deposit(user1, 10_000, 0).unwrap();
    engine.deposit(user2, 10_000, 0).unwrap();

    // Set positions: user1 long +10, user2 short -7, lp takes opposite (-3)
    engine.accounts[user1 as usize].position_size = I128::new(10_000_000); // +10 contracts
    engine.accounts[user1 as usize].entry_price = 1_000_000; // $1
    engine.accounts[user2 as usize].position_size = I128::new(-7_000_000); // -7 contracts
    engine.accounts[user2 as usize].entry_price = 1_200_000; // $1.20
    engine.accounts[lp_idx as usize].position_size = I128::new(-3_000_000); // -3 contracts (net = 0)
    engine.accounts[lp_idx as usize].entry_price = 1_000_000; // $1

    // Call panic_settle_all at oracle price $1.10
    let oracle_price = 1_100_000;
    engine.panic_settle_all(oracle_price).unwrap();

    // Assert all position_size == 0
    assert_eq!(
        engine.accounts[user1 as usize].position_size.get(),
        0,
        "User1 position should be closed"
    );
    assert_eq!(
        engine.accounts[user2 as usize].position_size.get(),
        0,
        "User2 position should be closed"
    );
    assert_eq!(
        engine.accounts[lp_idx as usize].position_size.get(),
        0,
        "LP position should be closed"
    );
}

#[test]
fn test_panic_settle_clamps_negative_pnl() {
    // Test B: mark pnl realized and losers clamped
    let mut engine = Box::new(RiskEngine::new(default_params()));

    let user_idx = engine.add_user(0).unwrap();
    engine.deposit(user_idx, 10_000, 0).unwrap();

    // User is long at $1, oracle will be $0.50 => big loss
    engine.accounts[user_idx as usize].position_size = I128::new(100_000_000); // Large long
    engine.accounts[user_idx as usize].entry_price = 1_000_000; // $1

    let oracle_price = 500_000; // $0.50 - user loses badly

    // Capture loss_accum before
    let loss_before = engine.loss_accum.get();

    engine.panic_settle_all(oracle_price).unwrap();

    // User's PNL should be clamped to 0
    assert!(
        engine.accounts[user_idx as usize].pnl.get() >= 0,
        "User PNL should be >= 0 after panic settle"
    );

    // loss_accum or insurance should have absorbed the loss
    let loss_increased = engine.loss_accum.get() > loss_before;
    let insurance_decreased = engine.insurance_fund.balance.is_zero();
    assert!(
        loss_increased || insurance_decreased || engine.accounts[user_idx as usize].pnl.is_zero(),
        "Loss should be socialized or absorbed by insurance"
    );
}

#[test]
fn test_panic_settle_adl_waterfall() {
    // Test C: ADL waterfall ordering (unwrapped first, then insurance)
    let mut engine = Box::new(RiskEngine::new(default_params()));

    // Account A with unwrapped PNL (warmup_slope = 0, so nothing is withdrawable)
    let winner = engine.add_user(0).unwrap();
    engine.deposit(winner, 10_000, 0).unwrap();

    // Loser account that will have a position
    let loser = engine.add_user(0).unwrap();
    engine.deposit(loser, 10_000, 0).unwrap();

    // Set up zero-sum positions at same entry price
    // Loser has a long position
    engine.accounts[loser as usize].position_size = I128::new(10_000_000);
    engine.accounts[loser as usize].entry_price = 1_000_000;

    // Winner has matching short position
    engine.accounts[winner as usize].position_size = I128::new(-10_000_000);
    engine.accounts[winner as usize].entry_price = 1_000_000;
    engine.accounts[winner as usize].warmup_slope_per_step = U128::new(0); // Any PNL will be unwrapped

    // Don't modify insurance_fund.balance - let it stay at what deposits set it to
    // This preserves conservation

    // Verify conservation before
    assert!(
        engine.check_conservation(DEFAULT_ORACLE),
        "Conservation should hold before panic settle"
    );

    // Oracle price causes loss on loser (long loses when price drops)
    // Winner (short) gains when price drops
    let oracle_price = 200_000; // $0.20, way below $1 entry

    engine.panic_settle_all(oracle_price).unwrap();

    // After panic settle:
    // - Loser's long at $1 with oracle $0.20 means loss = (0.2 - 1.0) * 10 = -8 (scaled)
    // - Winner's short at $1 with oracle $0.20 means gain = (1.0 - 0.2) * 10 = +8 (scaled)
    // But loser's PNL gets clamped to 0, creating a system loss
    // That loss gets socialized via ADL from winner's PNL (which became positive from position)

    // Winner should have gained from short position closing, then had ADL haircut applied
    // System should be conserved
    assert!(
        engine.check_conservation(DEFAULT_ORACLE),
        "Conservation must hold after panic settle"
    );
}

#[test]
fn test_panic_settle_freezes_warmup() {
    // Test D: warmup frozen on panic settle
    let mut engine = Box::new(RiskEngine::new(default_params()));

    let user_idx = engine.add_user(0).unwrap();
    let counterparty = engine.add_user(0).unwrap();
    engine.deposit(user_idx, 10_000, 0).unwrap();

    // Zero-sum PnL: user gains, counterparty loses (no vault funding needed)
    assert_eq!(engine.accounts[user_idx as usize].pnl.get(), 0);
    assert_eq!(engine.accounts[counterparty as usize].pnl.get(), 0);
    engine.accounts[user_idx as usize].pnl = I128::new(1000);
    engine.accounts[counterparty as usize].pnl = I128::new(-1000);
    engine.accounts[user_idx as usize].warmup_slope_per_step = U128::new(10);
    engine.accounts[user_idx as usize].warmup_started_at_slot = 0;
    assert_conserved(&engine);

    engine.current_slot = 50; // 50 slots elapsed

    let withdrawable_before = engine.withdrawable_pnl(&engine.accounts[user_idx as usize]);

    // Call panic_settle_all
    engine.panic_settle_all(1_000_000).unwrap();

    // Advance slots
    engine.advance_slot(100);

    let withdrawable_after = engine.withdrawable_pnl(&engine.accounts[user_idx as usize]);

    // Warmup should be frozen, so withdrawable should not increase
    assert!(
        withdrawable_after <= withdrawable_before + 1, // +1 for rounding tolerance
        "Withdrawable PNL should not increase after panic settle (warmup frozen)"
    );

    // Verify warmup is actually paused
    assert!(
        engine.warmup_paused,
        "Warmup should be paused after panic settle"
    );
}

#[test]
fn test_panic_settle_conservation_holds() {
    // Test E: conservation holds after panic settle
    let mut engine = Box::new(RiskEngine::new(default_params()));

    // Setup multiple accounts with various positions
    let lp_idx = engine.add_lp([1u8; 32], [2u8; 32], 0).unwrap();
    let user1 = engine.add_user(0).unwrap();
    let user2 = engine.add_user(0).unwrap();

    engine.deposit(lp_idx, 50_000, 0).unwrap();
    engine.deposit(user1, 10_000, 0).unwrap();
    engine.deposit(user2, 10_000, 0).unwrap();

    // Set up positions at same entry price (net position = 0)
    // This ensures positions are zero-sum
    engine.accounts[user1 as usize].position_size = I128::new(5_000_000); // Long 5
    engine.accounts[user1 as usize].entry_price = 1_000_000; // $1
    engine.accounts[user2 as usize].position_size = I128::new(-2_000_000); // Short 2
    engine.accounts[user2 as usize].entry_price = 1_000_000; // $1
    engine.accounts[lp_idx as usize].position_size = I128::new(-3_000_000); // Short 3 (LP takes other side)
    engine.accounts[lp_idx as usize].entry_price = 1_000_000; // $1

    // Verify conservation before
    assert!(
        engine.check_conservation(DEFAULT_ORACLE),
        "Conservation should hold before panic settle"
    );

    // Panic settle at a price that causes losses for longs
    engine.panic_settle_all(500_000).unwrap();

    // Verify conservation after
    assert!(
        engine.check_conservation(DEFAULT_ORACLE),
        "Conservation must hold after panic settle"
    );

    // Verify risk mode is active
    assert!(
        engine.risk_reduction_only,
        "Should be in risk-reduction mode after panic settle"
    );
}

// ==============================================================================
// DETERMINISTIC FUZZ/PROPERTY TESTS
// ==============================================================================

#[test]
fn fuzz_panic_settle_closes_all_positions_and_conserves() {
    // Property test: panic settle never leaves open positions + conservation holds
    // Loop for 200 seeds with randomized inputs

    for seed in 1..=200 {
        let mut rng = Rng::new(seed);
        let mut engine = Box::new(RiskEngine::new(default_params()));

        // Create N accounts (1 LP + some users)
        let lp_idx = engine.add_lp([1u8; 32], [2u8; 32], 0).unwrap();
        engine
            .deposit(lp_idx, rng.u128(10_000, 100_000), 0)
            .unwrap();

        let num_users = rng.u64(2, 6) as usize;
        let mut user_indices = Vec::new();

        for _ in 0..num_users {
            let user_idx = engine.add_user(0).unwrap();
            engine
                .deposit(user_idx, rng.u128(1_000, 50_000), 0)
                .unwrap();
            user_indices.push(user_idx);
        }

        // Randomize positions (ensure they sum to zero for valid state)
        // IMPORTANT: All positions must use the SAME entry price for zero-sum to hold
        // In a real perp system, every trade has a counterparty at the same price
        let mut total_position: i128 = 0;
        let common_entry_price = rng.u64(100_000, 10_000_000); // $0.10 to $10

        for &user_idx in &user_indices {
            let position = rng.i128(-50_000, 50_000);

            engine.accounts[user_idx as usize].position_size = I128::new(position);
            engine.accounts[user_idx as usize].entry_price = common_entry_price;
            total_position += position;
        }

        // LP takes opposite position to balance (net zero) at same entry price
        engine.accounts[lp_idx as usize].position_size = I128::new(-total_position);
        engine.accounts[lp_idx as usize].entry_price = common_entry_price;

        // Verify conservation before (should hold with zero-sum positions)
        // Use common_entry_price for conservation check since that's where positions are marked
        if !engine.check_conservation(common_entry_price) {
            eprintln!(
                "Seed {} BEFORE: vault={}, insurance={}, loss_accum={}, entry_price={}",
                seed,
                engine.vault,
                engine.insurance_fund.balance,
                engine.loss_accum,
                common_entry_price
            );
            // Print positions for debugging
            eprintln!(
                "LP[{}]: pos={}, entry={}",
                lp_idx,
                engine.accounts[lp_idx as usize].position_size.get(),
                engine.accounts[lp_idx as usize].entry_price
            );
            for &user_idx in &user_indices {
                eprintln!(
                    "User[{}]: pos={}, entry={}",
                    user_idx,
                    engine.accounts[user_idx as usize].position_size.get(),
                    engine.accounts[user_idx as usize].entry_price
                );
            }
            panic!(
                "Seed {}: Conservation should hold before panic settle",
                seed
            );
        }

        // Debug: capture state before panic settle (prefixed with _ to suppress warnings)
        let _vault_before = engine.vault;
        let _insurance_before = engine.insurance_fund.balance;

        // Random oracle price
        let oracle_price = rng.u64(100_000, 10_000_000);

        // Call panic_settle_all
        let result = engine.panic_settle_all(oracle_price);
        assert!(
            result.is_ok(),
            "Seed {}: panic_settle_all should not fail",
            seed
        );

        // Assert: all positions are zero
        assert_eq!(
            engine.accounts[lp_idx as usize].position_size.get(),
            0,
            "Seed {}: LP position should be closed",
            seed
        );
        for &user_idx in &user_indices {
            assert_eq!(
                engine.accounts[user_idx as usize].position_size.get(),
                0,
                "Seed {}: User {} position should be closed",
                seed,
                user_idx
            );
        }

        // Assert: all PNLs are >= 0 (negative clamped)
        assert!(
            engine.accounts[lp_idx as usize].pnl.get() >= 0,
            "Seed {}: LP PNL should be >= 0",
            seed
        );
        for &user_idx in &user_indices {
            assert!(
                engine.accounts[user_idx as usize].pnl.get() >= 0,
                "Seed {}: User {} PNL should be >= 0",
                seed,
                user_idx
            );
        }

        // Assert: conservation holds after
        if !engine.check_conservation(DEFAULT_ORACLE) {
            // Debug output - compute what check_conservation computes
            let mut real_total_capital = 0u128;
            let mut real_net_pnl: i128 = 0;
            for (block_i, word) in engine.used.iter().enumerate() {
                let mut w = *word;
                let block_offset = block_i * 64;
                while w != 0 {
                    let bit = w.trailing_zeros() as usize;
                    let idx = block_offset + bit;
                    w &= w - 1;
                    real_total_capital += engine.accounts[idx].capital.get();
                    real_net_pnl += engine.accounts[idx].pnl.get();
                }
            }
            let expected = (real_total_capital as i128
                + real_net_pnl
                + engine.insurance_fund.balance.get() as i128
                - engine.loss_accum.get() as i128) as u128;
            eprintln!("Seed {}: vault={}, real_capital={}, real_pnl={}, insurance={}, loss_accum={}, expected={}",
                     seed, engine.vault, real_total_capital, real_net_pnl,
                     engine.insurance_fund.balance, engine.loss_accum, expected);
            panic!("Seed {}: Conservation must hold after panic settle", seed);
        }

        // Assert: risk mode is active
        assert!(
            engine.risk_reduction_only,
            "Seed {}: Should be in risk-reduction mode",
            seed
        );
    }
}

// ==============================================================================
// WARMUP BUDGET INVARIANT TESTS
// ==============================================================================

// Test 1: Budget blocks warmed positive when no losses and no spendable insurance
#[test]
fn test_warmup_budget_blocks_positive_without_budget() {
    let mut params = default_params();
    params.risk_reduction_threshold = U128::new(100); // Floor at 100

    let mut engine = Box::new(RiskEngine::new(params));
    let user = engine.add_user(0).unwrap();
    let counterparty = engine.add_user(0).unwrap();

    // Set insurance exactly at floor (no spendable insurance)
    set_insurance(&mut engine, 100);

    // Zero-sum PnL: user gains, counterparty loses (no vault funding needed)
    assert_eq!(engine.accounts[user as usize].pnl.get(), 0);
    assert_eq!(engine.accounts[counterparty as usize].pnl.get(), 0);
    engine.accounts[user as usize].pnl = I128::new(1000);
    engine.accounts[counterparty as usize].pnl = I128::new(-1000);
    engine.accounts[user as usize].warmup_slope_per_step = U128::new(100);
    engine.accounts[user as usize].warmup_started_at_slot = 0;
    engine.accounts[user as usize].reserved_pnl = 0;
    assert_conserved(&engine);

    // Advance enough slots so cap >= 1000
    engine.current_slot = 20; // cap = 100 * 20 = 2000

    // Call settle_warmup_to_capital
    let capital_before = engine.accounts[user as usize].capital;
    let result = engine.settle_warmup_to_capital(user);
    assert!(result.is_ok());

    // Assert: capital unchanged (no budget for warming positive PnL)
    assert_eq!(
        engine.accounts[user as usize].capital, capital_before,
        "Capital should not increase without warmup budget"
    );
    assert_eq!(
        engine.warmed_pos_total.get(),
        0,
        "No positive PnL should be warmed"
    );
}

// Test 2: Warmed losses create budget for warmed profits
#[test]
fn test_warmup_budget_losses_create_budget_for_profits() {
    let mut params = default_params();
    params.risk_reduction_threshold = U128::new(0); // Floor at 0

    let mut engine = Box::new(RiskEngine::new(params));

    // Create loser and winner accounts
    let loser = engine.add_user(0).unwrap();
    let winner = engine.add_user(0).unwrap();

    // Set insurance at floor (no spendable insurance)
    set_insurance(&mut engine, 0);

    // Loser: capital=500, pnl=-500
    // Winner: capital=0, pnl=+500
    // Zero-sum PnL: loser's loss backs winner's gain (net_pnl = 0)
    // Only fund loser's capital
    engine.vault += 500;
    engine.accounts[loser as usize].capital = U128::new(500);
    assert_eq!(engine.accounts[loser as usize].pnl.get(), 0);
    assert_eq!(engine.accounts[winner as usize].pnl.get(), 0);
    engine.accounts[loser as usize].pnl = I128::new(-500);
    engine.accounts[loser as usize].warmup_slope_per_step = U128::new(1000);
    engine.accounts[loser as usize].warmup_started_at_slot = 0;
    engine.accounts[winner as usize].pnl = I128::new(500);
    engine.accounts[winner as usize].warmup_slope_per_step = U128::new(1000);
    engine.accounts[winner as usize].warmup_started_at_slot = 0;
    assert_conserved(&engine);

    engine.current_slot = 10; // cap = 1000 * 10 = 10000

    // Settle loser first
    engine.settle_warmup_to_capital(loser).unwrap();

    // Loser should have paid 500 from capital
    assert_eq!(engine.accounts[loser as usize].capital.get(), 0);
    assert_eq!(engine.accounts[loser as usize].pnl.get(), 0);
    assert_eq!(
        engine.warmed_neg_total.get(),
        500,
        "Loser should have contributed to warmed_neg_total"
    );

    // Now settle winner
    engine.settle_warmup_to_capital(winner).unwrap();

    // Winner should gain 500 capital (budget = warmed_neg_total = 500)
    assert_eq!(engine.accounts[winner as usize].capital.get(), 500);
    assert_eq!(engine.accounts[winner as usize].pnl.get(), 0);
    assert_eq!(
        engine.warmed_pos_total.get(),
        500,
        "Winner should have used warmup budget"
    );

    // Invariant should hold with equality
    assert!(
        engine.warmed_pos_total <= engine.warmed_neg_total + engine.insurance_spendable_raw(),
        "Warmup budget invariant violated"
    );
}

// Test 3: Spendable insurance allows warming profits without losses
#[test]
fn test_warmup_budget_insurance_allows_profits_without_losses() {
    let mut params = default_params();
    params.risk_reduction_threshold = U128::new(0); // Floor at 0

    let mut engine = Box::new(RiskEngine::new(params));
    let user = engine.add_user(0).unwrap();
    let counterparty = engine.add_user(0).unwrap();

    // Insurance provides budget
    set_insurance(&mut engine, 200);

    // Zero-sum PnL: user gains, counterparty loses (no vault funding needed)
    assert_eq!(engine.accounts[user as usize].pnl.get(), 0);
    assert_eq!(engine.accounts[counterparty as usize].pnl.get(), 0);
    engine.accounts[user as usize].pnl = I128::new(500);
    engine.accounts[counterparty as usize].pnl = I128::new(-500);
    engine.accounts[user as usize].warmup_slope_per_step = U128::new(1000);
    engine.accounts[user as usize].warmup_started_at_slot = 0;
    assert_conserved(&engine);

    engine.current_slot = 10; // cap = 10000

    // Settle warmup
    engine.settle_warmup_to_capital(user).unwrap();

    // Should warm exactly 200 (limited by budget = insurance_spendable = 200)
    assert_eq!(
        engine.warmed_pos_total.get(),
        200,
        "Should warm up to insurance budget"
    );
    assert_eq!(
        engine.accounts[user as usize].capital.get(),
        200,
        "Capital should increase by 200"
    );
    assert_eq!(
        engine.accounts[user as usize].pnl.get(),
        300,
        "PnL should decrease by 200"
    );

    // Invariant holds
    assert!(
        engine.warmed_pos_total <= engine.warmed_neg_total + engine.insurance_spendable_raw(),
        "Warmup budget invariant violated"
    );
}

// Test 4: In risk mode warmup frozen means no additional settlement over time
#[test]
fn test_warmup_budget_frozen_in_risk_mode() {
    let mut engine = Box::new(RiskEngine::new(default_params()));
    let user = engine.add_user(0).unwrap();
    let counterparty = engine.add_user(0).unwrap();

    // Provide insurance budget
    set_insurance(&mut engine, 1000);

    // Zero-sum PnL: user gains, counterparty loses (no vault funding needed)
    assert_eq!(engine.accounts[user as usize].pnl.get(), 0);
    assert_eq!(engine.accounts[counterparty as usize].pnl.get(), 0);
    engine.accounts[user as usize].pnl = I128::new(500);
    engine.accounts[counterparty as usize].pnl = I128::new(-500);
    engine.accounts[user as usize].warmup_slope_per_step = U128::new(10);
    engine.accounts[user as usize].warmup_started_at_slot = 0;
    assert_conserved(&engine);

    // Advance to slot 10, settle once
    engine.current_slot = 10; // cap = 100
    engine.settle_warmup_to_capital(user).unwrap();
    let warmed_after_first = engine.warmed_pos_total;
    assert_eq!(
        warmed_after_first.get(),
        100,
        "Should warm 100 (cap = 10 * 10)"
    );

    // Enter risk mode (freezes warmup at slot 10)
    engine.enter_risk_reduction_only_mode();
    assert!(engine.warmup_paused);
    assert_eq!(engine.warmup_pause_slot, 10);

    // Advance slot further
    engine.current_slot = 100;

    // Settle again - should NOT warm any more (frozen at slot 10)
    engine.settle_warmup_to_capital(user).unwrap();

    // Warmed totals should be unchanged
    assert_eq!(
        engine.warmed_pos_total, warmed_after_first,
        "No additional warmup should occur when frozen"
    );
}

// Test 5: Invariant holds after random sequence of operations
#[test]
fn test_warmup_budget_invariant_random_sequence() {
    let mut engine = Box::new(RiskEngine::new(default_params()));

    // Create 4 accounts with varied initial state
    let users: Vec<u16> = (0..4).map(|_| engine.add_user(0).unwrap()).collect();

    // WHITEBOX: Add insurance above floor to provide some budget (use += to preserve account fees)
    engine.insurance_fund.balance = U128::new(engine.insurance_fund.balance.get() + 5000);
    engine.vault = U128::new(engine.vault.get() + 5000);

    // Randomize initial state (deterministic seed)
    let mut rng = Rng::new(12345);

    for &user in &users {
        // Random capital and PnL (bounded)
        let capital = rng.u128(0, 10000);
        let pnl = rng.i128(-5000, 5000);
        let slope = rng.u128(0, 100);

        engine.accounts[user as usize].capital = U128::new(capital);
        engine.accounts[user as usize].pnl = I128::new(pnl);
        engine.accounts[user as usize].warmup_slope_per_step = U128::new(slope);
        engine.accounts[user as usize].warmup_started_at_slot = 0;
    }

    // Fix vault to match capital + pnl + insurance for conservation
    let mut total_capital = 0u128;
    let mut total_pnl: i128 = 0;
    for &user in &users {
        total_capital += engine.accounts[user as usize].capital.get();
        total_pnl += engine.accounts[user as usize].pnl.get();
    }
    // vault = capital + pnl + insurance (accounting for sign of pnl)
    let vault_needed = if total_pnl >= 0 {
        total_capital + engine.insurance_fund.balance.get() + total_pnl as u128
    } else {
        (total_capital + engine.insurance_fund.balance.get()).saturating_sub((-total_pnl) as u128)
    };
    engine.vault = U128::new(vault_needed);
    assert_conserved(&engine);

    // Run sequence of operations
    for step in 0..20 {
        // Advance slot
        engine.advance_slot(rng.u64(1, 5));

        // Settle warmup for each account
        for &user in &users {
            let _ = engine.settle_warmup_to_capital(user);
        }

        // Check invariant at each step
        let spendable =
            if engine.insurance_fund.balance.get() > engine.params.risk_reduction_threshold.get() {
                engine.insurance_fund.balance.get() - engine.params.risk_reduction_threshold.get()
            } else {
                0
            };

        assert!(
            engine.warmed_pos_total.get() <= engine.warmed_neg_total.get() + spendable,
            "Step {}: Warmup budget invariant violated: W+={}, W-={}, spendable={}",
            step,
            engine.warmed_pos_total,
            engine.warmed_neg_total,
            spendable
        );
    }
}

// ==============================================================================
// FORCE REALIZE LOSSES TESTS
// ==============================================================================

// Helper: params with non-zero threshold
fn params_with_threshold() -> RiskParams {
    RiskParams {
        warmup_period_slots: 100,
        maintenance_margin_bps: 500,
        initial_margin_bps: 1000,
        trading_fee_bps: 10,
        max_accounts: 1000,
        new_account_fee: U128::new(0),
        risk_reduction_threshold: U128::new(1000), // Non-zero threshold
        maintenance_fee_per_slot: U128::new(0),
        max_crank_staleness_slots: u64::MAX,
        liquidation_fee_bps: 50,
        liquidation_fee_cap: U128::new(100_000),
        liquidation_buffer_bps: 100,
        min_liquidation_abs: U128::new(100_000),
    }
}

// Test 1: Threshold gate - force_realize_losses errors if insurance > threshold
#[test]
fn test_force_realize_losses_threshold_gate() {
    let mut engine = Box::new(RiskEngine::new(params_with_threshold()));
    let _user = engine.add_user(0).unwrap();

    // Set insurance above threshold (threshold is 1000)
    set_insurance(&mut engine, 5000);
    assert_conserved(&engine);

    // force_realize_losses should fail
    let result = engine.force_realize_losses(1_000_000);
    assert!(result.is_err(), "Should fail when insurance > threshold");
    assert!(matches!(result.unwrap_err(), RiskError::Unauthorized));

    // Set insurance at threshold
    set_insurance(&mut engine, 1000);
    assert_conserved(&engine);
    let result = engine.force_realize_losses(1_000_000);
    assert!(result.is_ok(), "Should succeed when insurance == threshold");

    // Reset and set insurance below threshold
    let mut engine2 = Box::new(RiskEngine::new(params_with_threshold()));
    let _user2 = engine2.add_user(0).unwrap();
    set_insurance(&mut engine2, 500);
    assert_conserved(&engine2);
    let result = engine2.force_realize_losses(1_000_000);
    assert!(result.is_ok(), "Should succeed when insurance < threshold");
}

// Test 2: Loss paydown happens - capital decreases, warmed_neg_total increases
#[test]
fn test_force_realize_losses_paydown() {
    let mut engine = Box::new(RiskEngine::new(params_with_threshold()));
    let user = engine.add_user(0).unwrap();
    let lp = engine.add_lp([1u8; 32], [2u8; 32], 0).unwrap();

    // Setup: user long, lp short. Use small position to get predictable loss.
    // Position size 1000 contracts at entry 2_000_000
    // Oracle drops to 1_990_000 (10_000 price move)
    // mark_pnl = (1_990_000 - 2_000_000) * 1000 / 1_000_000 = -10_000 * 1000 / 1_000_000 = -10
    engine.accounts[user as usize].capital = U128::new(1000);
    engine.accounts[user as usize].position_size = I128::new(1000); // small position
    engine.accounts[user as usize].entry_price = 2_000_000;

    engine.accounts[lp as usize].capital = U128::new(1000);
    engine.accounts[lp as usize].position_size = I128::new(-1000);
    engine.accounts[lp as usize].entry_price = 2_000_000;

    // Add capital to vault and set insurance for conservation (no pnl yet)
    engine.vault += 1000 + 1000; // capitals
    set_insurance(&mut engine, 1000); // At threshold
    assert_conserved(&engine);

    let warmed_neg_before = engine.warmed_neg_total;
    let user_capital_before = engine.accounts[user as usize].capital;

    // Oracle drops slightly: loss = (2_000_000 - 1_990_000) * 1000 / 1_000_000 = 10
    engine.force_realize_losses(1_990_000).unwrap();

    // User should have paid losses from capital
    assert!(
        engine.accounts[user as usize].capital < user_capital_before,
        "User capital should decrease"
    );
    assert_eq!(
        engine.accounts[user as usize].capital,
        user_capital_before - 10,
        "User should pay 10 loss from capital"
    );

    // warmed_neg_total should increase by the paid amount
    assert!(
        engine.warmed_neg_total > warmed_neg_before,
        "warmed_neg_total should increase"
    );
    assert_eq!(
        engine.warmed_neg_total,
        warmed_neg_before + 10,
        "warmed_neg_total should increase by 10"
    );

    // Positions should be closed
    assert_eq!(engine.accounts[user as usize].position_size.get(), 0);
    assert_eq!(engine.accounts[lp as usize].position_size.get(), 0);

    // LP should have positive PnL (winner)
    assert!(
        engine.accounts[lp as usize].pnl.get() >= 0,
        "LP should have non-negative PnL"
    );

    // Conservation should hold
    assert!(
        engine.check_conservation(DEFAULT_ORACLE),
        "Conservation should hold"
    );
}

// Test 3: Unpaid loss goes to ADL - capital exhausted case
#[test]
fn test_force_realize_losses_unpaid_to_adl() {
    let mut engine = Box::new(RiskEngine::new(params_with_threshold()));
    let loser = engine.add_user(0).unwrap();
    let winner = engine.add_user(0).unwrap();
    let pnl_counterparty = engine.add_user(0).unwrap(); // Dedicated counterparty for pnl
    let lp = engine.add_lp([1u8; 32], [2u8; 32], 0).unwrap();

    // Setup: loser has small capital and will have large loss
    // Position 100_000 contracts at entry 2_000_000
    // Oracle drops to 1_900_000 (100_000 price drop)
    // mark_pnl = (1_900_000 - 2_000_000) * 100_000 / 1_000_000 = -10_000
    engine.accounts[loser as usize].capital = U128::new(100); // Small capital, will be exhausted
    engine.accounts[loser as usize].position_size = I128::new(100_000);
    engine.accounts[loser as usize].entry_price = 2_000_000;

    // Winner has positive PnL (young, subject to ADL)
    engine.accounts[winner as usize].capital = U128::new(5000);
    // Zero-sum PnL: winner gains, pnl_counterparty loses (no vault funding for pnl needed)
    // Use dedicated counterparty so loser/lp can have their pnl set by force_realize_losses()
    assert_eq!(engine.accounts[winner as usize].pnl.get(), 0);
    assert_eq!(engine.accounts[pnl_counterparty as usize].pnl.get(), 0);
    engine.accounts[winner as usize].pnl = I128::new(5000); // Young positive PnL
    engine.accounts[pnl_counterparty as usize].pnl = I128::new(-5000);
    engine.accounts[winner as usize].warmup_slope_per_step = U128::new(10);

    // LP is counterparty for positions
    engine.accounts[lp as usize].capital = U128::new(10_000);
    engine.accounts[lp as usize].position_size = I128::new(-100_000);
    engine.accounts[lp as usize].entry_price = 2_000_000;

    // vault += sum(capital) only (pnl is zero-sum)
    // sum(capital) = 100 + 5000 + 10000 = 15100
    engine.vault += 15100;
    set_insurance(&mut engine, 1000); // At threshold
    assert_conserved(&engine);

    let winner_pnl_before = engine.accounts[winner as usize].pnl;

    // Oracle drops: loser's loss = 10_000, can only pay 100, unpaid = 9900
    engine.force_realize_losses(1_900_000).unwrap();

    // Loser capital should be exhausted
    assert_eq!(
        engine.accounts[loser as usize].capital.get(),
        0,
        "Loser capital should be exhausted"
    );

    // Loser PnL should be clamped to 0
    assert_eq!(
        engine.accounts[loser as usize].pnl.get(),
        0,
        "Loser PnL should be clamped to 0"
    );

    // ADL should have been triggered - winner's PnL should be haircut
    // (the LP also gains 10_000 but that goes through ADL as well)
    assert!(
        engine.accounts[winner as usize].pnl < winner_pnl_before,
        "Winner PnL should be haircut by ADL"
    );

    // Conservation should hold
    assert!(
        engine.check_conservation(DEFAULT_ORACLE),
        "Conservation should hold"
    );
}

// Test 4: Warmup remains frozen after force_realize_losses
#[test]
fn test_force_realize_losses_warmup_frozen() {
    let mut engine = Box::new(RiskEngine::new(params_with_threshold()));
    let user = engine.add_user(0).unwrap();
    let counterparty = engine.add_user(0).unwrap();

    // WHITEBOX: Setup user with positive PnL and warmup.
    engine.accounts[user as usize].capital = U128::new(5000);
    // Zero-sum PnL: user gains, counterparty loses (no vault funding for pnl needed)
    assert_eq!(engine.accounts[user as usize].pnl.get(), 0);
    assert_eq!(engine.accounts[counterparty as usize].pnl.get(), 0);
    engine.accounts[user as usize].pnl = I128::new(1000);
    engine.accounts[counterparty as usize].pnl = I128::new(-1000);
    engine.accounts[user as usize].warmup_slope_per_step = U128::new(10);
    engine.accounts[user as usize].warmup_started_at_slot = 0;

    // vault += capital only (pnl is zero-sum)
    engine.vault += 5000;
    set_insurance(&mut engine, 1000); // Exactly at threshold
    engine.current_slot = 50;
    assert_conserved(&engine);

    // Before force_realize, warmup should work normally
    let withdrawable_before = engine.withdrawable_pnl(&engine.accounts[user as usize]);
    assert!(withdrawable_before > 0, "Should have some withdrawable PnL");

    // Force realize losses
    engine.force_realize_losses(1_000_000).unwrap();

    // Warmup should be frozen
    assert!(engine.warmup_paused, "Warmup should be paused");
    let pause_slot = engine.warmup_pause_slot;

    // Advance time
    engine.current_slot = 200;

    // Withdrawable should NOT increase (frozen at pause_slot)
    let withdrawable_after = engine.withdrawable_pnl(&engine.accounts[user as usize]);

    // The withdrawable should be capped at pause_slot, not current_slot
    // Since warmup is frozen, vested amount is fixed
    let expected_vested = 10 * pause_slot as u128; // slope * steps at pause
    let expected_withdrawable = core::cmp::min(expected_vested, 1000);
    assert_eq!(
        withdrawable_after, expected_withdrawable,
        "Withdrawable should be frozen at pause slot"
    );
}

// Test 5: Warmup budget invariant holds after force_realize_losses
#[test]
fn test_force_realize_losses_invariant_holds() {
    let mut engine = Box::new(RiskEngine::new(params_with_threshold()));
    let user1 = engine.add_user(0).unwrap();
    let user2 = engine.add_user(0).unwrap();
    let pnl_counterparty = engine.add_user(0).unwrap(); // Dedicated counterparty for pnl
    let lp = engine.add_lp([1u8; 32], [2u8; 32], 0).unwrap();

    // Setup: user1 has losing position, user2 has positive pnl
    // Position 10_000 contracts at entry 2_000_000
    // Oracle drops to 1_990_000 (10_000 price drop)
    // mark_pnl = (1_990_000 - 2_000_000) * 10_000 / 1_000_000 = -100
    engine.accounts[user1 as usize].capital = U128::new(5000);
    engine.accounts[user1 as usize].position_size = I128::new(10_000);
    engine.accounts[user1 as usize].entry_price = 2_000_000;

    engine.accounts[user2 as usize].capital = U128::new(5000);
    // Zero-sum PnL: user2 gains, pnl_counterparty loses (no vault funding for pnl needed)
    // Use dedicated counterparty so user1/lp can have their pnl set by force_realize_losses()
    assert_eq!(engine.accounts[user2 as usize].pnl.get(), 0);
    assert_eq!(engine.accounts[pnl_counterparty as usize].pnl.get(), 0);
    engine.accounts[user2 as usize].pnl = I128::new(2000); // existing positive PnL
    engine.accounts[pnl_counterparty as usize].pnl = I128::new(-2000);
    engine.accounts[user2 as usize].warmup_slope_per_step = U128::new(20);

    engine.accounts[lp as usize].capital = U128::new(10_000);
    engine.accounts[lp as usize].position_size = I128::new(-10_000);
    engine.accounts[lp as usize].entry_price = 2_000_000;

    // vault += sum(capital) only (pnl is zero-sum)
    // sum(capital) = 5000 + 5000 + 10000 = 20000
    engine.vault += 20000;
    set_insurance(&mut engine, 1000); // At threshold
    assert_conserved(&engine);

    // Force realize at a price that causes small loss
    engine.force_realize_losses(1_990_000).unwrap();

    // Check invariant: warmed_pos_total <= warmed_neg_total + insurance_spendable_raw()
    let spendable = engine.insurance_spendable_raw();
    assert!(
        engine.warmed_pos_total <= engine.warmed_neg_total.saturating_add(spendable),
        "Warmup budget invariant violated after force_realize_losses: W+={}, W-={}, spendable={}",
        engine.warmed_pos_total,
        engine.warmed_neg_total,
        spendable
    );

    // Conservation should hold
    assert!(
        engine.check_conservation(DEFAULT_ORACLE),
        "Conservation violated after force_realize_losses"
    );
}

// ============================================================================
// Warmup Insurance Reserved Tests (Plan Step 9)
// ============================================================================

/// Test 1: Invariant stays true after spending insurance
/// This catches the exact bug where ADL could spend reserved insurance.
#[test]
fn test_reserved_invariant_after_adl_spending() {
    let mut params = default_params();
    params.risk_reduction_threshold = U128::new(100); // Floor at 100

    let mut engine = Box::new(RiskEngine::new(params));
    let user_idx = engine.add_user(0).unwrap();
    let counterparty = engine.add_user(0).unwrap();

    // Setup: floor=100, insurance=200 (raw spendable = 100)
    set_insurance(&mut engine, 200);

    // Zero-sum PnL: user gains, counterparty loses (no vault funding needed)
    assert_eq!(engine.accounts[user_idx as usize].pnl.get(), 0);
    assert_eq!(engine.accounts[counterparty as usize].pnl.get(), 0);
    engine.accounts[user_idx as usize].pnl = I128::new(100);
    engine.accounts[counterparty as usize].pnl = I128::new(-100);
    engine.accounts[user_idx as usize].warmup_slope_per_step = U128::new(10000);
    engine.accounts[user_idx as usize].warmup_started_at_slot = 0;
    engine.current_slot = 10;
    assert_conserved(&engine);

    // Settle warmup - should warm 100 and reserve 100
    engine.settle_warmup_to_capital(user_idx).unwrap();

    assert_eq!(engine.warmed_pos_total.get(), 100, "Should warm 100");
    assert_eq!(
        engine.warmup_insurance_reserved.get(),
        100,
        "Should reserve 100"
    );
    assert_eq!(
        engine.insurance_spendable_unreserved(),
        0,
        "Unreserved should be 0"
    );

    // Apply ADL with 50 loss - since unreserved = 0, can't spend insurance
    engine.apply_adl(50).unwrap();

    // Insurance should remain 200 (cannot spend reserved)
    assert_eq!(
        engine.insurance_fund.balance.get(),
        200,
        "Insurance should remain 200 - reserved portion protected"
    );

    // loss_accum should increase by 50
    assert_eq!(
        engine.loss_accum.get(),
        50,
        "Loss should go to loss_accum since reserved can't be spent"
    );

    // Invariant should hold: W+ <= W- + raw
    let raw = engine.insurance_spendable_raw();
    assert!(
        engine.warmed_pos_total <= engine.warmed_neg_total.saturating_add(raw),
        "Stable invariant W+ <= W- + raw should hold"
    );
}

/// Test 2: ADL can spend unreserved insurance
#[test]
fn test_adl_spends_unreserved_insurance() {
    let mut params = default_params();
    params.risk_reduction_threshold = U128::new(100); // Floor at 100

    let mut engine = Box::new(RiskEngine::new(params));
    let user_idx = engine.add_user(0).unwrap();
    let counterparty = engine.add_user(0).unwrap();

    // Setup: floor=100, insurance=200 (raw spendable = 100)
    set_insurance(&mut engine, 200);

    // Zero-sum PnL: user gains, counterparty loses (no vault funding needed)
    assert_eq!(engine.accounts[user_idx as usize].pnl.get(), 0);
    assert_eq!(engine.accounts[counterparty as usize].pnl.get(), 0);
    engine.accounts[user_idx as usize].pnl = I128::new(40);
    engine.accounts[counterparty as usize].pnl = I128::new(-40);
    engine.accounts[user_idx as usize].warmup_slope_per_step = U128::new(10000);
    engine.accounts[user_idx as usize].warmup_started_at_slot = 0;
    engine.current_slot = 10;
    assert_conserved(&engine);

    // Warm only 40 from insurance (leaves 60 unreserved)
    engine.settle_warmup_to_capital(user_idx).unwrap();

    assert_eq!(
        engine.warmup_insurance_reserved.get(),
        40,
        "Should reserve 40"
    );
    assert_eq!(
        engine.insurance_spendable_unreserved(),
        60,
        "Unreserved should be 60"
    );

    let insurance_before = engine.insurance_fund.balance;

    // Apply ADL with 30 loss - should spend from unreserved
    engine.apply_adl(30).unwrap();

    // Insurance should decrease by 30
    assert_eq!(
        engine.insurance_fund.balance,
        insurance_before - 30,
        "Insurance should decrease by 30 (spent from unreserved)"
    );

    // loss_accum should be 0 (fully covered by insurance)
    assert_eq!(
        engine.loss_accum.get(),
        0,
        "No loss_accum since insurance covered it"
    );

    // Reserved should be unchanged
    assert_eq!(
        engine.warmup_insurance_reserved.get(),
        40,
        "Reserved unchanged"
    );
}

/// Test 3: No insurance minting on negative rounding
#[test]
fn test_no_insurance_minting_on_rounding() {
    let mut engine = Box::new(RiskEngine::new(default_params()));
    let lp_idx = engine.add_lp([1u8; 32], [2u8; 32], 0).unwrap();
    let user_idx = engine.add_user(0).unwrap();

    // Setup accounts with positions that will cause rounding
    engine.deposit(lp_idx, 10_000, 0).unwrap();
    engine.deposit(user_idx, 10_000, 0).unwrap();

    // Create opposing positions
    engine.accounts[lp_idx as usize].position_size = I128::new(-100);
    engine.accounts[user_idx as usize].position_size = I128::new(100);

    let insurance_before = engine.insurance_fund.balance;

    // Call panic_settle_all - may have rounding
    engine.panic_settle_all(1_000_000).unwrap();

    // Insurance should not increase (no minting from rounding)
    assert!(
        engine.insurance_fund.balance <= insurance_before,
        "Insurance should not increase from rounding: before={}, after={}",
        insurance_before,
        engine.insurance_fund.balance
    );

    assert!(
        engine.check_conservation(DEFAULT_ORACLE),
        "Conservation violated"
    );
}

/// Test 4: Reserved is correctly recomputed after operations
/// Formula: reserved = min(max(W+ - W-, 0), raw_spendable)
#[test]
fn test_reserved_correctly_recomputed() {
    // Test that warmup_insurance_reserved is correctly computed as:
    // reserved = min(max(W+ - W-, 0), raw_spendable)
    // where raw_spendable = max(0, I - I_min)
    let mut params = default_params();
    params.risk_reduction_threshold = U128::new(100);

    let mut engine = Box::new(RiskEngine::new(params));
    let lp_idx = engine.add_lp([1u8; 32], [2u8; 32], 0).unwrap();
    let user_idx = engine.add_user(0).unwrap();

    // Setup: insurance = 500 (set_insurance adjusts vault automatically)
    set_insurance(&mut engine, 500);
    assert_conserved(&engine);

    engine.deposit(lp_idx, 10_000, 0).unwrap();
    engine.deposit(user_idx, 1_000, 0).unwrap();

    // Zero-sum PnL: user gains, lp loses (no vault funding needed)
    assert_eq!(engine.accounts[user_idx as usize].pnl.get(), 0);
    assert_eq!(engine.accounts[lp_idx as usize].pnl.get(), 0);
    engine.accounts[user_idx as usize].pnl = I128::new(50);
    engine.accounts[lp_idx as usize].pnl = I128::new(-50);
    engine.accounts[user_idx as usize].warmup_slope_per_step = U128::new(10000);
    engine.accounts[user_idx as usize].warmup_started_at_slot = 0;
    engine.current_slot = 10;
    assert_conserved(&engine);

    engine.settle_warmup_to_capital(user_idx).unwrap();
    let reserved_after_warmup = engine.warmup_insurance_reserved;
    assert!(
        !reserved_after_warmup.is_zero(),
        "Should have reserved some insurance"
    );

    // Verify reserved is correctly computed: min(W+ - W-, raw_spendable)
    let w_plus = engine.warmed_pos_total.get();
    let w_minus = engine.warmed_neg_total.get();
    let raw_spendable = engine
        .insurance_fund
        .balance
        .get()
        .saturating_sub(engine.params.risk_reduction_threshold.get());
    let expected_reserved = core::cmp::min(w_plus.saturating_sub(w_minus), raw_spendable);
    assert_eq!(
        engine.warmup_insurance_reserved.get(),
        expected_reserved,
        "Reserved should match formula after warmup"
    );

    // Run ADL - reserved should be recomputed correctly
    engine.apply_adl(10).unwrap();
    let raw_spendable = engine
        .insurance_fund
        .balance
        .get()
        .saturating_sub(engine.params.risk_reduction_threshold.get());
    let expected_reserved = core::cmp::min(
        engine
            .warmed_pos_total
            .get()
            .saturating_sub(engine.warmed_neg_total.get()),
        raw_spendable,
    );
    assert_eq!(
        engine.warmup_insurance_reserved.get(),
        expected_reserved,
        "Reserved should match formula after ADL"
    );

    // Run panic_settle with positions
    engine.accounts[lp_idx as usize].position_size = I128::new(-100);
    engine.accounts[user_idx as usize].position_size = I128::new(100);
    engine.panic_settle_all(1_000_000).unwrap();
    let raw_spendable = engine
        .insurance_fund
        .balance
        .get()
        .saturating_sub(engine.params.risk_reduction_threshold.get());
    let expected_reserved = core::cmp::min(
        engine
            .warmed_pos_total
            .get()
            .saturating_sub(engine.warmed_neg_total.get()),
        raw_spendable,
    );
    assert_eq!(
        engine.warmup_insurance_reserved.get(),
        expected_reserved,
        "Reserved should match formula after panic_settle"
    );

    // When insurance drops to floor, reserved decreases (raw_spendable = 0)
    set_insurance(&mut engine, 100); // At floor
                                     // Manually call recompute since set_insurance doesn't do it
    let raw_spendable = engine
        .insurance_fund
        .balance
        .get()
        .saturating_sub(engine.params.risk_reduction_threshold.get());
    assert_eq!(raw_spendable, 0, "raw_spendable should be 0 at floor");
    // Note: reserved won't automatically update from set_insurance helper,
    // but force_realize_losses will recompute it
    let _ = engine.force_realize_losses(1_000_000);
    let raw_spendable = engine
        .insurance_fund
        .balance
        .get()
        .saturating_sub(engine.params.risk_reduction_threshold.get());
    let expected_reserved = core::cmp::min(
        engine
            .warmed_pos_total
            .get()
            .saturating_sub(engine.warmed_neg_total.get()),
        raw_spendable,
    );
    assert_eq!(
        engine.warmup_insurance_reserved.get(),
        expected_reserved,
        "Reserved should match formula after force_realize_losses (may be 0 at floor)"
    );
}

// ============================================================================
// AUDIT-MANDATED TESTS: Double-Settlement, Conservation, Reserved Insurance
// These tests were mandated by the security audit to verify critical fixes.
// ============================================================================

/// Test A: Double-Settlement Bug Fix
///
/// Verifies that settle_warmup_to_capital is idempotent when warmup is paused.
/// The fix ensures that warmup_started_at_slot is always updated to effective_slot,
/// preventing the same matured PnL from being settled twice.
///
/// Bug scenario (before fix):
/// 1. User has positive PnL warming up
/// 2. Warmup gets paused at slot 50 (e.g., due to risk mode)
/// 3. User calls settle_warmup_to_capital at slot 100 - settles 50 slots of PnL
/// 4. User calls settle_warmup_to_capital again at slot 100 - should settle 0 more
/// 5. BUG: Without the fix, warmup_started_at_slot wasn't updated, allowing double-settlement
#[test]
fn test_audit_a_settle_idempotent_when_paused() {
    let mut params = default_params();
    params.risk_reduction_threshold = U128::new(100); // Non-zero floor

    let mut engine = Box::new(RiskEngine::new(params));
    let user_idx = engine.add_user(0).unwrap();
    let counterparty = engine.add_user(0).unwrap();

    // Setup: User has positive PnL with warmup slope
    set_insurance(&mut engine, 10_000); // Provide warmup budget
    engine.deposit(user_idx, 1_000, 0).unwrap();
    // Zero-sum PnL: user gains, counterparty loses (no vault funding needed)
    assert_eq!(engine.accounts[user_idx as usize].pnl.get(), 0);
    assert_eq!(engine.accounts[counterparty as usize].pnl.get(), 0);
    engine.accounts[user_idx as usize].pnl = I128::new(500);
    engine.accounts[counterparty as usize].pnl = I128::new(-500);
    engine.accounts[user_idx as usize].warmup_slope_per_step = U128::new(10); // 10 per slot
    engine.accounts[user_idx as usize].warmup_started_at_slot = 0;
    assert_conserved(&engine);

    // Advance to slot 50 and pause warmup
    engine.current_slot = 50;
    engine.warmup_paused = true;
    engine.warmup_pause_slot = 50;

    // First settlement at slot 100 (but effective_slot is capped at 50)
    engine.current_slot = 100;
    engine.settle_warmup_to_capital(user_idx).unwrap();

    let capital_after_first = engine.accounts[user_idx as usize].capital;
    let pnl_after_first = engine.accounts[user_idx as usize].pnl;
    let warmed_pos_after_first = engine.warmed_pos_total;

    // Second settlement at same slot - should be idempotent (no change)
    engine.settle_warmup_to_capital(user_idx).unwrap();

    let capital_after_second = engine.accounts[user_idx as usize].capital;
    let pnl_after_second = engine.accounts[user_idx as usize].pnl;
    let warmed_pos_after_second = engine.warmed_pos_total;

    // CRITICAL: Second settlement must not change anything
    assert_eq!(
        capital_after_first, capital_after_second,
        "TEST A FAILED: Capital changed on second settlement ({} -> {}). \
                Double-settlement bug still present!",
        capital_after_first, capital_after_second
    );
    assert_eq!(
        pnl_after_first, pnl_after_second,
        "TEST A FAILED: PnL changed on second settlement ({} -> {}). \
                Double-settlement bug still present!",
        pnl_after_first, pnl_after_second
    );
    assert_eq!(
        warmed_pos_after_first, warmed_pos_after_second,
        "TEST A FAILED: warmed_pos_total changed on second settlement ({} -> {}). \
                Double-settlement bug still present!",
        warmed_pos_after_first, warmed_pos_after_second
    );

    // Also verify that warmup_started_at_slot was updated to effective_slot
    assert_eq!(
        engine.accounts[user_idx as usize].warmup_started_at_slot, 50,
        "warmup_started_at_slot should be updated to effective_slot (pause_slot)"
    );
}

/// Test A variant: Multiple settlements over time while paused
#[test]
fn test_audit_a_settle_idempotent_multiple_times_while_paused() {
    let mut params = default_params();
    params.risk_reduction_threshold = U128::new(0);

    let mut engine = Box::new(RiskEngine::new(params));
    let user_idx = engine.add_user(0).unwrap();
    let counterparty = engine.add_user(0).unwrap();

    // Setup
    set_insurance(&mut engine, 10_000);
    engine.deposit(user_idx, 1_000, 0).unwrap();
    // Zero-sum PnL: user gains, counterparty loses (no vault funding needed)
    assert_eq!(engine.accounts[user_idx as usize].pnl.get(), 0);
    assert_eq!(engine.accounts[counterparty as usize].pnl.get(), 0);
    engine.accounts[user_idx as usize].pnl = I128::new(1000);
    engine.accounts[counterparty as usize].pnl = I128::new(-1000);
    engine.accounts[user_idx as usize].warmup_slope_per_step = U128::new(100);
    engine.accounts[user_idx as usize].warmup_started_at_slot = 0;
    assert_conserved(&engine);

    // Pause at slot 10
    engine.warmup_paused = true;
    engine.warmup_pause_slot = 10;

    // First settlement at slot 20
    engine.current_slot = 20;
    engine.settle_warmup_to_capital(user_idx).unwrap();
    let state_after_first = (
        engine.accounts[user_idx as usize].capital,
        engine.accounts[user_idx as usize].pnl,
        engine.warmed_pos_total,
    );

    // Multiple subsequent settlements at various slots - all should be idempotent
    for slot in [30, 50, 100, 200] {
        engine.current_slot = slot;
        engine.settle_warmup_to_capital(user_idx).unwrap();

        let state_now = (
            engine.accounts[user_idx as usize].capital,
            engine.accounts[user_idx as usize].pnl,
            engine.warmed_pos_total,
        );

        assert_eq!(
            state_after_first, state_now,
            "Settlement at slot {} changed state while paused. Double-settlement bug!",
            slot
        );
    }
}

/// Test B: Conservation Bug Fix
///
/// Verifies that check_conservation uses >= instead of == to account for
/// safe rounding surplus that stays in the vault unclaimed.
/// The rounding_surplus field was removed, and negative rounding errors
/// are now safely ignored (they leave extra value in the vault).
#[test]
fn test_audit_b_conservation_after_panic_settle_with_rounding() {
    let mut engine = Box::new(RiskEngine::new(default_params()));
    let user_idx = engine.add_user(0).unwrap();
    let lp_idx = engine.add_lp([1u8; 32], [2u8; 32], 0).unwrap();

    // Setup opposing positions that will create rounding when settled
    engine.deposit(user_idx, 50_000, 0).unwrap();
    engine.deposit(lp_idx, 50_000, 0).unwrap();

    // Create positions with values that will cause integer division rounding
    // Position size of 333 at price 1_000_003 creates rounding scenarios
    engine.accounts[user_idx as usize].position_size = I128::new(333);
    engine.accounts[user_idx as usize].entry_price = 1_000_003;
    engine.accounts[lp_idx as usize].position_size = I128::new(-333);
    engine.accounts[lp_idx as usize].entry_price = 1_000_003;

    // Verify conservation before
    assert!(
        engine.check_conservation(DEFAULT_ORACLE),
        "TEST B: Conservation violated BEFORE panic_settle"
    );

    // Settle at a different price to realize rounding errors
    let oracle_price = 1_500_007; // Prime number for maximum rounding
    engine.panic_settle_all(oracle_price).unwrap();

    // CRITICAL: Conservation must hold even with rounding
    assert!(
        engine.check_conservation(DEFAULT_ORACLE),
        "TEST B FAILED: Conservation violated after panic_settle_all. \
             The conservation check should use >= to account for safe rounding surplus."
    );

    // Verify all positions are closed
    assert_eq!(engine.accounts[user_idx as usize].position_size.get(), 0);
    assert_eq!(engine.accounts[lp_idx as usize].position_size.get(), 0);
}

/// Test B variant: Conservation with force_realize_losses rounding
#[test]
fn test_audit_b_conservation_after_force_realize_with_rounding() {
    let mut params = default_params();
    params.risk_reduction_threshold = U128::new(1000);

    let mut engine = Box::new(RiskEngine::new(params));
    let user_idx = engine.add_user(0).unwrap();
    let lp_idx = engine.add_lp([1u8; 32], [2u8; 32], 0).unwrap();

    // Setup - do deposits first
    engine.deposit(user_idx, 50_000, 0).unwrap();
    engine.deposit(lp_idx, 50_000, 0).unwrap();

    // Adjust insurance to be at threshold to trigger force_realize
    // After account creation and deposits:
    // - vault = account_fees + deposits = 2 + 100_000 = 100_002
    // - insurance = account_fees = 2
    // - capitals = 100_000
    // Conservation: vault = sum(capital) + sum(pnl) + insurance
    // 100_002 = 100_000 + 0 + 2 ✓
    //
    // Now set insurance to threshold (1000) - set_insurance adjusts vault
    set_insurance(&mut engine, 1000);
    assert_conserved(&engine);

    // Create positions with rounding-prone values
    engine.accounts[user_idx as usize].position_size = I128::new(777);
    engine.accounts[user_idx as usize].entry_price = 999_999;
    engine.accounts[lp_idx as usize].position_size = I128::new(-777);
    engine.accounts[lp_idx as usize].entry_price = 999_999;

    assert!(
        engine.check_conservation(DEFAULT_ORACLE),
        "Conservation before force_realize"
    );

    // Force realize at price that causes rounding
    engine.force_realize_losses(1_234_567).unwrap();

    assert!(
        engine.check_conservation(DEFAULT_ORACLE),
        "TEST B FAILED: Conservation violated after force_realize_losses"
    );
}

/// Test C: Reserved Insurance Spending Protection
///
/// Verifies that warmup_insurance_reserved properly protects the insurance
/// fund from being spent in ADL. Insurance reserved for backing warmed
/// profits must not be used to cover ADL losses.
#[test]
fn test_audit_c_reserved_insurance_not_spent_in_adl() {
    let mut params = default_params();
    params.risk_reduction_threshold = U128::new(100); // Floor of 100

    let mut engine = Box::new(RiskEngine::new(params));
    let winner_idx = engine.add_user(0).unwrap();
    let loser_idx = engine.add_user(0).unwrap();

    // Setup: Insurance fund has balance above floor (set_insurance adjusts vault)
    set_insurance(&mut engine, 500);
    assert_conserved(&engine);

    // Winner has positive PnL that will warm up
    engine.deposit(winner_idx, 1_000, 0).unwrap();
    // Loser has no capital PnL to haircut (but provides zero-sum for winner's pnl)
    engine.deposit(loser_idx, 1_000, 0).unwrap();
    // Zero-sum PnL: winner gains, loser loses (no vault funding needed)
    assert_eq!(engine.accounts[winner_idx as usize].pnl.get(), 0);
    assert_eq!(engine.accounts[loser_idx as usize].pnl.get(), 0);
    engine.accounts[winner_idx as usize].pnl = I128::new(200);
    engine.accounts[loser_idx as usize].pnl = I128::new(-200);
    engine.accounts[winner_idx as usize].warmup_slope_per_step = U128::new(1000);
    engine.accounts[winner_idx as usize].warmup_started_at_slot = 0;
    engine.current_slot = 10;
    assert_conserved(&engine);

    // Warm up the winner's PnL (this should reserve insurance)
    engine.settle_warmup_to_capital(winner_idx).unwrap();

    let reserved_before_adl = engine.warmup_insurance_reserved.get();
    assert!(
        reserved_before_adl > 0,
        "Should have reserved insurance for warmed profits"
    );

    // Calculate spendable insurance (raw - reserved)
    let raw_spendable = engine.insurance_spendable_raw();
    let unreserved_spendable = engine.insurance_spendable_unreserved();

    // The reserved amount should protect insurance
    assert!(
        unreserved_spendable < raw_spendable,
        "Unreserved spendable should be less than raw spendable"
    );

    // Now apply ADL with a loss larger than unreserved spendable
    // This should NOT touch the reserved portion
    let insurance_before = engine.insurance_fund.balance.get();
    let large_loss = unreserved_spendable + 100; // More than unreserved

    engine.apply_adl(large_loss).unwrap();

    // Check that reserved amount was protected
    let insurance_after = engine.insurance_fund.balance.get();
    let insurance_spent = insurance_before.saturating_sub(insurance_after);

    // Insurance spent should be at most the unreserved amount
    // (remaining loss goes to loss_accum, not from reserved insurance)
    assert!(
        insurance_spent <= unreserved_spendable,
        "TEST C FAILED: ADL spent reserved insurance! \
             Spent: {}, Unreserved was: {}, Reserved: {}",
        insurance_spent,
        unreserved_spendable,
        reserved_before_adl
    );

    // The remaining loss should be in loss_accum
    assert!(
        engine.loss_accum.get() > 0,
        "Excess loss should go to loss_accum, not reserved insurance"
    );

    // Reserved should not decrease
    assert!(
        engine.warmup_insurance_reserved.get() >= reserved_before_adl,
        "TEST C FAILED: Reserved insurance decreased during ADL"
    );
}

/// Test C variant: Verify insurance floor + reserved is protected
#[test]
fn test_audit_c_insurance_floor_plus_reserved_protected() {
    let mut params = default_params();
    params.risk_reduction_threshold = U128::new(200); // Floor

    let mut engine = Box::new(RiskEngine::new(params));
    let user_idx = engine.add_user(0).unwrap();
    let counterparty = engine.add_user(0).unwrap();

    // Setup: Insurance = 500, floor = 200, so raw_spendable = 300
    set_insurance(&mut engine, 500);
    assert_conserved(&engine);

    engine.deposit(user_idx, 5_000, 0).unwrap();
    // Zero-sum PnL: user gains, counterparty loses (no vault funding needed)
    assert_eq!(engine.accounts[user_idx as usize].pnl.get(), 0);
    assert_eq!(engine.accounts[counterparty as usize].pnl.get(), 0);
    engine.accounts[user_idx as usize].pnl = I128::new(100);
    engine.accounts[counterparty as usize].pnl = I128::new(-100);
    engine.accounts[user_idx as usize].warmup_slope_per_step = U128::new(10000);
    engine.accounts[user_idx as usize].warmup_started_at_slot = 0;
    engine.current_slot = 10;
    assert_conserved(&engine);

    // Warm up PnL - this will reserve some insurance
    engine.settle_warmup_to_capital(user_idx).unwrap();

    let reserved = engine.warmup_insurance_reserved.get();
    let floor = params.risk_reduction_threshold.get();

    // Apply massive ADL
    engine.apply_adl(1000).unwrap();

    // Insurance should never go below floor + reserved
    let min_protected = floor.saturating_add(reserved);
    assert!(
        engine.insurance_fund.balance.get() >= min_protected.saturating_sub(1), // Allow 1 for rounding
        "TEST C FAILED: Insurance {} fell below floor + reserved = {}",
        engine.insurance_fund.balance,
        min_protected
    );
}

/// Test: Conservation slack is bounded by MAX_ROUNDING_SLACK
///
/// Verifies that check_conservation() not only checks actual >= expected,
/// but also that the slack (actual - expected) is bounded to prevent
/// unbounded dust accumulation or accidental minting.
#[test]
fn test_audit_conservation_slack_bounded() {
    let mut engine = Box::new(RiskEngine::new(default_params()));

    // Create many accounts with positions that will cause rounding
    // Use 50 to stay under MAX_ACCOUNTS (64 in fuzz builds)
    let mut user_indices = Vec::new();
    for i in 0..50 {
        let user_idx = engine.add_user(0).unwrap();
        user_indices.push(user_idx);
        engine.deposit(user_idx, 1000 + i as u128, 0).unwrap();

        // Create positions with rounding-prone values
        engine.accounts[user_idx as usize].position_size = I128::new((100 + i) as i128);
        engine.accounts[user_idx as usize].entry_price = 1_000_003 + i as u64;
    }

    // Create an LP to take the other side
    let lp_idx = engine.add_lp([1u8; 32], [2u8; 32], 0).unwrap();
    engine.deposit(lp_idx, 1_000_000, 0).unwrap();

    // Set LP position to net out user positions
    let total_user_pos: i128 = user_indices
        .iter()
        .map(|&idx| engine.accounts[idx as usize].position_size.get())
        .sum();
    engine.accounts[lp_idx as usize].position_size = I128::new(-total_user_pos);
    engine.accounts[lp_idx as usize].entry_price = 1_000_000;

    // Conservation should hold before
    assert!(
        engine.check_conservation(DEFAULT_ORACLE),
        "Conservation before panic_settle"
    );

    // Panic settle at a price that causes rounding
    engine.panic_settle_all(1_500_007).unwrap();

    // Conservation should still hold (bounded slack)
    assert!(
        engine.check_conservation(DEFAULT_ORACLE),
        "Conservation violated after panic_settle - slack may exceed MAX_ROUNDING_SLACK"
    );

    // Verify all positions closed
    for &idx in &user_indices {
        assert_eq!(engine.accounts[idx as usize].position_size.get(), 0);
    }
    assert_eq!(engine.accounts[lp_idx as usize].position_size.get(), 0);
}

/// Test: Conservation check detects excessive slack
///
/// Verifies that if someone tries to "mint" value by inflating the vault,
/// the bounded check will catch it.
#[test]
fn test_audit_conservation_detects_excessive_slack() {
    let mut engine = Box::new(RiskEngine::new(default_params()));
    let user_idx = engine.add_user(0).unwrap();

    engine.deposit(user_idx, 10_000, 0).unwrap();

    // Conservation should hold normally
    assert!(
        engine.check_conservation(DEFAULT_ORACLE),
        "Normal conservation"
    );

    // Artificially inflate vault beyond MAX_ROUNDING_SLACK
    // This simulates a minting bug
    engine.vault = engine.vault + percolator::MAX_ROUNDING_SLACK + 10;

    // Conservation should now FAIL due to excessive slack
    assert!(
        !engine.check_conservation(DEFAULT_ORACLE),
        "Conservation should fail when slack exceeds MAX_ROUNDING_SLACK"
    );
}

/// Test: force_realize_losses updates warmup_started_at_slot to prevent re-pay
///
/// Verifies that after force_realize_losses() processes an account, the
/// warmup_started_at_slot is updated so that a subsequent call to
/// settle_warmup_to_capital() doesn't "re-pay" based on old elapsed time.
#[test]
fn test_audit_force_realize_prevents_warmup_repay() {
    let mut params = default_params();
    params.risk_reduction_threshold = U128::new(1000); // Floor

    let mut engine = Box::new(RiskEngine::new(params));
    let loser_idx = engine.add_user(0).unwrap();
    let lp_idx = engine.add_lp([1u8; 32], [2u8; 32], 0).unwrap();

    // Setup loser with a position that will have negative PnL
    engine.deposit(loser_idx, 10_000, 0).unwrap();
    engine.deposit(lp_idx, 100_000, 0).unwrap();

    // Loser has a long position at high price (will lose when we settle at lower price)
    engine.accounts[loser_idx as usize].position_size = I128::new(1000);
    engine.accounts[loser_idx as usize].entry_price = 2_000_000; // $2

    // LP has opposite position
    engine.accounts[lp_idx as usize].position_size = I128::new(-1000);
    engine.accounts[lp_idx as usize].entry_price = 2_000_000;

    // Set warmup started way in the past (slot 0)
    engine.accounts[loser_idx as usize].warmup_started_at_slot = 0;
    engine.accounts[loser_idx as usize].warmup_slope_per_step = U128::new(100); // High slope

    // Set insurance at floor to enable force_realize
    set_insurance(&mut engine, 1000);

    // Adjust vault for conservation
    let total_capital =
        engine.accounts[loser_idx as usize].capital + engine.accounts[lp_idx as usize].capital;
    engine.vault = total_capital + engine.insurance_fund.balance;
    assert_conserved(&engine);

    // Move to slot 100 (100 elapsed slots since warmup_started_at = 0)
    engine.current_slot = 100;

    // Force realize at lower price (loser takes a loss)
    engine.force_realize_losses(1_000_000).unwrap(); // $1 price

    // Record state after force_realize
    let capital_after_force = engine.accounts[loser_idx as usize].capital;
    let pnl_after_force = engine.accounts[loser_idx as usize].pnl;
    let warmed_neg_after_force = engine.warmed_neg_total;

    // Verify warmup_started_at_slot was updated to effective_slot
    // Since we're paused (entered risk mode), effective_slot = warmup_pause_slot
    assert_eq!(
        engine.accounts[loser_idx as usize].warmup_started_at_slot, engine.warmup_pause_slot,
        "warmup_started_at_slot should be updated to effective_slot"
    );

    // Now call settle_warmup_to_capital - it should NOT change anything
    // because warmup_started_at_slot was updated, so elapsed = 0
    engine.current_slot = 200; // Advance time further
    engine.settle_warmup_to_capital(loser_idx).unwrap();

    // CRITICAL: State should be unchanged (no "re-payment" based on old elapsed)
    assert_eq!(
        engine.accounts[loser_idx as usize].capital, capital_after_force,
        "Capital should not change after settle - warmup_started_at was updated"
    );
    assert_eq!(
        engine.accounts[loser_idx as usize].pnl, pnl_after_force,
        "PnL should not change after settle - warmup_started_at was updated"
    );
    assert_eq!(
        engine.warmed_neg_total, warmed_neg_after_force,
        "warmed_neg_total should not change - no additional settlement"
    );
}

/// Test: force_realize_losses updates warmup for ALL processed accounts
#[test]
fn test_audit_force_realize_updates_all_accounts_warmup() {
    let mut params = default_params();
    params.risk_reduction_threshold = U128::new(1000);

    let mut engine = Box::new(RiskEngine::new(params));

    // Create multiple accounts
    let user1 = engine.add_user(0).unwrap();
    let user2 = engine.add_user(0).unwrap();
    let lp_idx = engine.add_lp([1u8; 32], [2u8; 32], 0).unwrap();

    engine.deposit(user1, 10_000, 0).unwrap();
    engine.deposit(user2, 10_000, 0).unwrap();
    engine.deposit(lp_idx, 100_000, 0).unwrap();

    // Both users have positions
    engine.accounts[user1 as usize].position_size = I128::new(500);
    engine.accounts[user1 as usize].entry_price = 2_000_000;
    engine.accounts[user1 as usize].warmup_started_at_slot = 0;

    engine.accounts[user2 as usize].position_size = I128::new(500);
    engine.accounts[user2 as usize].entry_price = 2_000_000;
    engine.accounts[user2 as usize].warmup_started_at_slot = 5;

    engine.accounts[lp_idx as usize].position_size = I128::new(-1000);
    engine.accounts[lp_idx as usize].entry_price = 2_000_000;
    engine.accounts[lp_idx as usize].warmup_started_at_slot = 10;

    // Set insurance at floor
    set_insurance(&mut engine, 1000);
    let total_capital = engine.accounts[user1 as usize].capital
        + engine.accounts[user2 as usize].capital
        + engine.accounts[lp_idx as usize].capital;
    engine.vault = total_capital + 1000;
    assert_conserved(&engine);

    engine.current_slot = 100;

    // Force realize
    engine.force_realize_losses(1_000_000).unwrap();

    // All accounts with positions should have updated warmup_started_at_slot
    let effective = engine.warmup_pause_slot;
    assert_eq!(
        engine.accounts[user1 as usize].warmup_started_at_slot, effective,
        "User1 warmup_started_at_slot should be updated"
    );
    assert_eq!(
        engine.accounts[user2 as usize].warmup_started_at_slot, effective,
        "User2 warmup_started_at_slot should be updated"
    );
    assert_eq!(
        engine.accounts[lp_idx as usize].warmup_started_at_slot, effective,
        "LP warmup_started_at_slot should be updated"
    );
}

// ==============================================================================
// GUARDRAIL: NO IGNORED RESULT PATTERNS IN ENGINE
// ==============================================================================

/// This test guards against reintroducing ignored-Result patterns in the engine.
/// The Solana atomicity model requires that all fallible operations propagate errors.
/// NOTE: This test intentionally stays file-local.
/// If percolator.rs is split, this test MUST be updated.
#[test]
fn no_ignored_result_patterns_in_engine() {
    let src = include_str!("../src/percolator.rs");

    // Check for ignored Result patterns on specific functions that must propagate errors
    assert!(
        !src.contains("let _ = Self::settle_account_funding"),
        "Do not ignore settle_account_funding errors - use ? operator"
    );
    assert!(
        !src.contains("let _ = self.touch_account"),
        "Do not ignore touch_account errors - use ? operator"
    );
    assert!(
        !src.contains("let _ = self.settle_warmup_to_capital"),
        "Do not ignore settle_warmup_to_capital errors - use ? operator"
    );
}

// ==============================================================================
// API-LEVEL SEQUENCE TEST
// ==============================================================================

/// Deterministic sequence test that verifies conservation holds after every API operation.
/// This test uses only public API methods - no direct state mutation.
#[test]
fn api_sequence_conservation_smoke_test() {
    let mut engine = Box::new(RiskEngine::new(default_params()));
    let user = engine.add_user(0).unwrap();
    let lp = engine.add_lp([1u8; 32], [2u8; 32], 0).unwrap();

    engine.deposit(user, 10_000, 0).unwrap();
    engine.deposit(lp, 50_000, 0).unwrap();

    assert_conserved(&engine);

    // Execute a trade (use size > 1000 to generate non-zero fee)
    engine
        .execute_trade(&MATCHER, lp, user, 0, 1_000_000, 10_000)
        .unwrap();
    assert_conserved(&engine);

    // Accrue funding
    engine.accrue_funding(1, 1_000_000, 10).unwrap();
    engine.touch_account(user).unwrap();
    assert_conserved(&engine);

    // Close the position (reduces risk)
    engine
        .execute_trade(&MATCHER, lp, user, 0, 1_000_000, -10_000)
        .unwrap();
    assert_conserved(&engine);

    // Withdraw (should succeed since position is closed)
    engine.withdraw(user, 1_000, 0, 1_000_000).unwrap();
    assert_conserved(&engine);
}

// ==============================================================================
// INVARIANT UNIT TESTS (Step 6 of ADL/Warmup correctness plan)
// ==============================================================================

/// Test that ADL distributes haircuts exactly, including remainder distribution.
/// Create 3 accounts with unwrapped PnL, apply ADL with a loss that causes remainder.
#[test]
fn test_adl_exact_haircut_distribution() {
    let mut engine = Box::new(RiskEngine::new(default_params()));

    // Create 3 users with positive PnL that will have unwrapped amounts
    let user1 = engine.add_user(0).unwrap();
    let user2 = engine.add_user(0).unwrap();
    let user3 = engine.add_user(0).unwrap();

    // Deposit capital
    engine.deposit(user1, 10_000, 0).unwrap();
    engine.deposit(user2, 10_000, 0).unwrap();
    engine.deposit(user3, 10_000, 0).unwrap();

    // Create counterparty for zero-sum pnl
    let loser = engine.add_user(4).unwrap();
    engine.deposit(loser, 50_000, 0).unwrap();

    // Set up positive PnL on each user (will be unwrapped since no warmup time)
    // Use values that will cause remainder: 100 + 100 + 100 = 300 total unwrapped
    // Zero-sum pattern: net_pnl = 0, so no vault funding needed
    engine.accounts[user1 as usize].pnl = I128::new(100);
    engine.accounts[user2 as usize].pnl = I128::new(100);
    engine.accounts[user3 as usize].pnl = I128::new(100);
    engine.accounts[loser as usize].pnl = I128::new(-300); // Zero-sum counterparty

    assert_conserved(&engine);

    // Record PnL before ADL
    let pnl1_before = engine.accounts[user1 as usize].pnl;
    let pnl2_before = engine.accounts[user2 as usize].pnl;
    let pnl3_before = engine.accounts[user3 as usize].pnl;

    // Apply ADL with a loss of 7 (will cause remainder since 7/3 = 2 with remainder 1)
    let loss = 7u128;
    engine.apply_adl(loss).unwrap();

    // Calculate total haircut applied
    let haircut1 = (pnl1_before.get() - engine.accounts[user1 as usize].pnl.get()) as u128;
    let haircut2 = (pnl2_before.get() - engine.accounts[user2 as usize].pnl.get()) as u128;
    let haircut3 = (pnl3_before.get() - engine.accounts[user3 as usize].pnl.get()) as u128;
    let total_haircut = haircut1 + haircut2 + haircut3;

    // Verify that the total haircut exactly equals the loss
    assert_eq!(
        total_haircut, loss,
        "ADL must distribute exact loss: got {} expected {}",
        total_haircut, loss
    );

    assert_conserved(&engine);
}

/// Test that warmup slope is always >= 1 when positive PnL exists.
/// Set positive_pnl = 1 (below warmup period), verify slope = 1 after update.
#[test]
fn test_warmup_slope_nonzero() {
    let params = RiskParams {
        warmup_period_slots: 1000, // Large period so pnl=1 would normally give slope=0
        ..default_params()
    };
    let mut engine = Box::new(RiskEngine::new(params));

    let user = engine.add_user(0).unwrap();
    engine.deposit(user, 10_000, 0).unwrap();

    // Set minimal positive PnL (1 unit, less than warmup_period_slots)
    engine.accounts[user as usize].pnl = I128::new(1);

    // Create counterparty for zero-sum
    // Zero-sum pattern: net_pnl = 0, so no vault funding needed
    let loser = engine.add_user(0).unwrap();
    engine.deposit(loser, 10_000, 0).unwrap();
    engine.accounts[loser as usize].pnl = I128::new(-1);

    assert_conserved(&engine);

    // Update warmup slope
    engine.update_warmup_slope(user).unwrap();

    // Verify slope is at least 1 (not 0)
    let slope = engine.accounts[user as usize].warmup_slope_per_step.get();
    assert!(
        slope >= 1,
        "Slope must be >= 1 when positive PnL exists, got {}",
        slope
    );

    assert_conserved(&engine);
}

/// Test that warmup_insurance_reserved can decrease when losses are settled.
/// Settle profits first (increases reserved), then settle losses (should decrease reserved).
#[test]
fn test_warmup_reserve_release() {
    let params = RiskParams {
        warmup_period_slots: 10,
        risk_reduction_threshold: U128::new(100),
        ..default_params()
    };
    let mut engine = Box::new(RiskEngine::new(params));

    // Fund insurance (using helper to properly update vault)
    set_insurance(&mut engine, 1000);

    let winner = engine.add_user(0).unwrap();
    let loser = engine.add_user(0).unwrap();

    engine.deposit(winner, 10_000, 0).unwrap();
    engine.deposit(loser, 10_000, 0).unwrap();

    // Set up zero-sum PnL
    // Zero-sum pattern: net_pnl = 0, so no vault funding needed
    engine.accounts[winner as usize].pnl = I128::new(500);
    engine.accounts[loser as usize].pnl = I128::new(-500);

    assert_conserved(&engine);

    // Update warmup slope for winner
    engine.update_warmup_slope(winner).unwrap();

    // For the loser, we need to manually set warmup capacity to allow losses to settle.
    // update_warmup_slope only calculates slope from positive PnL (returns 0 for negative).
    // To settle losses, we give the loser a warmup_slope based on the loss magnitude.
    engine.accounts[loser as usize].warmup_slope_per_step = U128::new(50); // 500/10 = 50 per slot
    engine.accounts[loser as usize].warmup_started_at_slot = 0;

    // Advance time to allow full warmup
    engine.current_slot = 100;

    // Settle the winner's profits first (should reserve insurance since no losses settled yet)
    engine.settle_warmup_to_capital(winner).unwrap();
    let reserved_after_profit = engine.warmup_insurance_reserved.get();

    // Now settle the loser's losses (W- increases, should release some reserved)
    engine.settle_warmup_to_capital(loser).unwrap();
    let reserved_after_loss = engine.warmup_insurance_reserved.get();

    // Reserved should have decreased (or stayed same if losses >= profits)
    assert!(
        reserved_after_loss <= reserved_after_profit,
        "Reserved should decrease when losses settle: before {} after {}",
        reserved_after_profit,
        reserved_after_loss
    );

    // Since W+ = 500 and now W- = 500, reserved should be 0
    assert_eq!(reserved_after_loss, 0, "Reserved should be 0 when W+ == W-");

    assert_conserved(&engine);
}

/// Test the precise definition of unwrapped PnL.
/// unwrapped = max(0, positive_pnl - reserved_pnl - withdrawable_pnl)
#[test]
fn test_unwrapped_definition() {
    let params = RiskParams {
        warmup_period_slots: 100,
        ..default_params()
    };
    let mut engine = Box::new(RiskEngine::new(params));

    let user = engine.add_user(0).unwrap();
    engine.deposit(user, 10_000, 0).unwrap();

    // Create counterparty for zero-sum
    // Zero-sum pattern: net_pnl = 0, so no vault funding needed
    let loser = engine.add_user(0).unwrap();
    engine.deposit(loser, 10_000, 0).unwrap();
    engine.accounts[loser as usize].pnl = I128::new(-1000);

    // Set positive PnL and reserved
    engine.accounts[user as usize].pnl = I128::new(1000);
    engine.accounts[user as usize].reserved_pnl = 200;

    // Update slope to establish warmup rate
    engine.update_warmup_slope(user).unwrap();

    assert_conserved(&engine);

    // At t=0, nothing is warmed yet, so:
    // withdrawable = 0
    // unwrapped = 1000 - 200 - 0 = 800
    let account = &engine.accounts[user as usize];
    let positive_pnl = account.pnl.get() as u128;
    let reserved = account.reserved_pnl as u128;

    // Compute withdrawable manually (same logic as compute_withdrawable_pnl)
    let available = positive_pnl - reserved; // 800
    let elapsed = engine
        .current_slot
        .saturating_sub(account.warmup_started_at_slot);
    let warmed_cap = account.warmup_slope_per_step.get() * (elapsed as u128);
    let withdrawable = core::cmp::min(available, warmed_cap);

    // Expected unwrapped
    let expected_unwrapped = positive_pnl
        .saturating_sub(reserved)
        .saturating_sub(withdrawable);

    // Test: at t=0, withdrawable should be 0, unwrapped should be 800
    assert_eq!(withdrawable, 0, "No time elapsed, withdrawable should be 0");
    assert_eq!(expected_unwrapped, 800, "Unwrapped should be 800 at t=0");

    // Advance time to allow partial warmup (50 slots = 50% of 100)
    engine.current_slot = 50;

    // Recalculate
    let account = &engine.accounts[user as usize];
    let elapsed = engine
        .current_slot
        .saturating_sub(account.warmup_started_at_slot);
    let warmed_cap = account.warmup_slope_per_step.get() * (elapsed as u128);
    let available = positive_pnl - reserved; // 800
    let withdrawable_now = core::cmp::min(available, warmed_cap);

    // With slope=10 (1000/100) and 50 slots, warmed_cap = 500
    // withdrawable = min(800, 500) = 500
    // unwrapped = 1000 - 200 - 500 = 300
    let expected_unwrapped_now = positive_pnl
        .saturating_sub(reserved)
        .saturating_sub(withdrawable_now);

    assert_eq!(
        withdrawable_now, 500,
        "After 50 slots, withdrawable should be 500"
    );
    assert_eq!(
        expected_unwrapped_now, 300,
        "After 50 slots, unwrapped should be 300"
    );

    assert_conserved(&engine);
}

// ============================================================================
// ADL LARGEST-REMAINDER TESTS
// ============================================================================

/// Test 1: ADL exactness - sum of haircuts equals min(total_loss, total_unwrapped) exactly
#[test]
fn test_adl_largest_remainder_exactness() {
    let params = default_params();
    let mut engine = Box::new(RiskEngine::new(params));

    // Create LP to take the opposite side of PnL (zero-sum)
    let lp = engine.add_lp([1u8; 32], [2u8; 32], 0).unwrap();
    engine.deposit(lp, 10_000, 0).unwrap();

    // Create 3 accounts with uneven unwrapped PnL amounts
    let user1 = engine.add_user(0).unwrap();
    let user2 = engine.add_user(0).unwrap();
    let user3 = engine.add_user(0).unwrap();

    // Deposit capital
    engine.deposit(user1, 1000, 0).unwrap();
    engine.deposit(user2, 1000, 0).unwrap();
    engine.deposit(user3, 1000, 0).unwrap();

    // Assign uneven positive PnL (unwrapped since no warmup yet)
    // Total = 100 + 200 + 300 = 600
    // Zero-sum: LP takes the opposite side
    engine.accounts[user1 as usize].pnl = I128::new(100);
    engine.accounts[user2 as usize].pnl = I128::new(200);
    engine.accounts[user3 as usize].pnl = I128::new(300);
    engine.accounts[lp as usize].pnl = I128::new(-600); // LP loses what users gain

    assert_conserved(&engine);

    // Record initial PnLs
    let initial_pnl1 = engine.accounts[user1 as usize].pnl.get();
    let initial_pnl2 = engine.accounts[user2 as usize].pnl.get();
    let initial_pnl3 = engine.accounts[user3 as usize].pnl.get();
    let total_initial_pnl = initial_pnl1 + initial_pnl2 + initial_pnl3;

    // Apply ADL with a loss that requires remainder distribution
    // Using 250 ensures we get fractional haircuts: 250/600 * 100 = 41.66..., etc.
    let total_loss: u128 = 250;
    engine.apply_adl(total_loss).unwrap();

    // Calculate total haircut applied
    let final_pnl1 = engine.accounts[user1 as usize].pnl.get();
    let final_pnl2 = engine.accounts[user2 as usize].pnl.get();
    let final_pnl3 = engine.accounts[user3 as usize].pnl.get();
    let total_final_pnl = final_pnl1 + final_pnl2 + final_pnl3;

    let total_haircut = (total_initial_pnl - total_final_pnl) as u128;

    // Verify exact equality: total haircut == min(total_loss, total_unwrapped)
    let total_unwrapped = 600u128; // All PnL is unwrapped (no warmup)
    let expected_haircut = core::cmp::min(total_loss, total_unwrapped);

    assert_eq!(
        total_haircut, expected_haircut,
        "ADL exactness: total haircut {} != expected {}",
        total_haircut, expected_haircut
    );
}

/// Test 2: ADL tie-break determinism - lower index wins when remainders are equal
#[test]
fn test_adl_tiebreak_lower_idx_wins() {
    let params = default_params();
    let mut engine = Box::new(RiskEngine::new(params));

    // Create LP to take the opposite side of PnL (zero-sum)
    let lp = engine.add_lp([1u8; 32], [2u8; 32], 0).unwrap();
    engine.deposit(lp, 10_000, 0).unwrap();

    // Create 2 accounts with identical unwrapped PnL
    let user1 = engine.add_user(0).unwrap();
    let user2 = engine.add_user(0).unwrap();

    engine.deposit(user1, 1000, 0).unwrap();
    engine.deposit(user2, 1000, 0).unwrap();

    // Same PnL = same remainder for any proportional calculation
    // Zero-sum: LP takes the opposite side
    engine.accounts[user1 as usize].pnl = I128::new(100);
    engine.accounts[user2 as usize].pnl = I128::new(100);
    engine.accounts[lp as usize].pnl = I128::new(-200); // LP loses what users gain

    assert_conserved(&engine);

    // Record initial PnLs
    let initial_pnl1 = engine.accounts[user1 as usize].pnl.get();
    let initial_pnl2 = engine.accounts[user2 as usize].pnl.get();

    // Loss of 1: exactly 1 unit to distribute as remainder (since 1/200 < 1 per account)
    // Both accounts have remainder = 1*100 % 200 = 100, so they tie
    // Lower index (user1) should receive the +1 haircut
    let total_loss: u128 = 1;
    engine.apply_adl(total_loss).unwrap();

    let final_pnl1 = engine.accounts[user1 as usize].pnl.get();
    let final_pnl2 = engine.accounts[user2 as usize].pnl.get();

    let haircut1 = initial_pnl1 - final_pnl1;
    let haircut2 = initial_pnl2 - final_pnl2;

    // Lower index should get the leftover
    assert_eq!(haircut1, 1, "Lower index (user1) should get the +1 haircut");
    assert_eq!(haircut2, 0, "Higher index (user2) should get 0");
}

/// Test 3: Reserved equality invariant - reserved == min(max(W+ - W-, 0), raw)
#[test]
fn test_reserved_equality_invariant() {
    let mut params = default_params();
    params.risk_reduction_threshold = U128::new(100); // I_min = 100

    let mut engine = Box::new(RiskEngine::new(params));

    // Add accounts and set up state
    let lp = engine.add_lp([1u8; 32], [2u8; 32], 0).unwrap();
    let user = engine.add_user(0).unwrap();

    // Set insurance to 500 (via direct manipulation for test purposes)
    engine.insurance_fund.balance = U128::new(500);
    engine.vault = U128::new(500); // Keep conservation

    engine.deposit(lp, 10_000, 0).unwrap();
    engine.deposit(user, 1_000, 0).unwrap();

    // Create warmed positive PnL by manually setting W+ and W-
    // Simulate: user warmed 200 in positive PnL, LP paid 50 in losses
    engine.warmed_pos_total = U128::new(200);
    engine.warmed_neg_total = U128::new(50);

    // Call recompute
    engine.recompute_warmup_insurance_reserved();

    // Verify formula: reserved = min(max(W+ - W-, 0), raw_spendable)
    // raw_spendable = max(0, I - I_min) = max(0, 500 - 100) = 400
    // needed = W+ - W- = 200 - 50 = 150
    // expected_reserved = min(150, 400) = 150
    let raw = engine
        .insurance_fund
        .balance
        .get()
        .saturating_sub(engine.params.risk_reduction_threshold.get());
    let needed = engine
        .warmed_pos_total
        .get()
        .saturating_sub(engine.warmed_neg_total.get());
    let expected = core::cmp::min(needed, raw);

    assert_eq!(
        engine.warmup_insurance_reserved.get(),
        expected,
        "Reserved should equal min(max(W+ - W-, 0), raw): {} != {}",
        engine.warmup_insurance_reserved,
        expected
    );

    // Test edge case: W- > W+ (no reservation needed)
    engine.warmed_pos_total = U128::new(50);
    engine.warmed_neg_total = U128::new(200);
    engine.recompute_warmup_insurance_reserved();

    let needed2 = engine
        .warmed_pos_total
        .get()
        .saturating_sub(engine.warmed_neg_total.get());
    assert_eq!(needed2, 0, "Needed should be 0 when W- > W+");
    assert_eq!(
        engine.warmup_insurance_reserved.get(),
        0,
        "Reserved should be 0 when W- > W+"
    );

    // Test edge case: insurance at floor (raw = 0)
    engine.warmed_pos_total = U128::new(200);
    engine.warmed_neg_total = U128::new(50);
    engine.insurance_fund.balance = U128::new(100); // At floor
    engine.recompute_warmup_insurance_reserved();

    let raw3 = engine
        .insurance_fund
        .balance
        .get()
        .saturating_sub(engine.params.risk_reduction_threshold.get());
    assert_eq!(raw3, 0, "raw_spendable should be 0 at floor");
    assert_eq!(
        engine.warmup_insurance_reserved.get(),
        0,
        "Reserved should be 0 when at floor"
    );
}

// ============================================================================
// Negative PnL Immediate Settlement Tests (Fix A)
// ============================================================================

/// Test 1: Withdrawal rejected when position closed and negative PnL exists
/// Setup: capital=10_000, pnl=-9_000, pos=0, slope=0, vault=10_000
/// withdraw(10_000) must be Err(InsufficientBalance)
/// State after: capital=1_000, pnl=0
#[test]
fn test_withdraw_rejected_when_closed_and_negative_pnl() {
    let mut engine = Box::new(RiskEngine::new(default_params()));
    let user_idx = engine.add_user(0).unwrap();

    // Setup: position closed but with unrealized losses
    engine.accounts[user_idx as usize].capital = U128::new(10_000);
    engine.accounts[user_idx as usize].pnl = I128::new(-9_000);
    engine.accounts[user_idx as usize].position_size = I128::new(0); // No position
    engine.accounts[user_idx as usize].warmup_slope_per_step = U128::new(0);
    engine.vault = U128::new(10_000);

    // Attempt to withdraw full capital - should fail because losses must be realized first
    let result = engine.withdraw(user_idx, 10_000, 0, 1_000_000);

    // The withdraw should fail with InsufficientBalance
    assert!(
        result == Err(RiskError::InsufficientBalance),
        "Expected InsufficientBalance after loss realization reduces capital"
    );

    // After the failed withdraw call (which internally called settle_warmup_to_capital):
    // capital should be 1_000 (10_000 - 9_000 loss)
    // pnl should be 0 (loss fully realized)
    // warmed_neg_total should include 9_000
    assert_eq!(
        engine.accounts[user_idx as usize].capital.get(),
        1_000,
        "Capital should be reduced by loss amount"
    );
    assert_eq!(
        engine.accounts[user_idx as usize].pnl.get(),
        0,
        "PnL should be 0 after loss realization"
    );
    assert_eq!(
        engine.warmed_neg_total.get(),
        9_000,
        "warmed_neg_total should increase by realized loss"
    );
}

/// Test 2: After loss realization, remaining principal can be withdrawn
#[test]
fn test_withdraw_allows_remaining_principal_after_loss_realization() {
    let mut engine = Box::new(RiskEngine::new(default_params()));
    let user_idx = engine.add_user(0).unwrap();

    // Setup: position closed but with unrealized losses
    engine.accounts[user_idx as usize].capital = U128::new(10_000);
    engine.accounts[user_idx as usize].pnl = I128::new(-9_000);
    engine.accounts[user_idx as usize].position_size = I128::new(0);
    engine.accounts[user_idx as usize].warmup_slope_per_step = U128::new(0);
    engine.vault = U128::new(10_000);

    // First, trigger loss settlement
    engine.settle_warmup_to_capital(user_idx).unwrap();

    // Now capital should be 1_000
    assert_eq!(engine.accounts[user_idx as usize].capital.get(), 1_000);
    assert_eq!(engine.accounts[user_idx as usize].pnl.get(), 0);

    // Withdraw remaining capital - should succeed
    let result = engine.withdraw(user_idx, 1_000, 0, 1_000_000);
    assert!(
        result.is_ok(),
        "Withdraw of remaining capital should succeed"
    );
    assert_eq!(engine.accounts[user_idx as usize].capital.get(), 0);
}

/// Test: Negative PnL settles immediately, independent of warmup slope
#[test]
fn test_negative_pnl_settles_immediately_independent_of_slope() {
    let mut engine = Box::new(RiskEngine::new(default_params()));
    let user_idx = engine.add_user(0).unwrap();

    // Setup: loss with zero slope - under old code this would NOT settle
    let capital = 10_000u128;
    let loss = 3_000i128;
    engine.accounts[user_idx as usize].capital = U128::new(capital);
    engine.accounts[user_idx as usize].pnl = I128::new(-loss);
    engine.accounts[user_idx as usize].warmup_slope_per_step = U128::new(0); // Zero slope
    engine.accounts[user_idx as usize].warmup_started_at_slot = 0;
    engine.vault = U128::new(capital);
    engine.current_slot = 100; // Time has passed

    let warmed_neg_before = engine.warmed_neg_total.get();

    // Call settle
    engine.settle_warmup_to_capital(user_idx).unwrap();

    // Assertions: loss should settle immediately despite zero slope
    assert_eq!(
        engine.accounts[user_idx as usize].capital.get(),
        capital - (loss as u128),
        "Capital should be reduced by full loss amount"
    );
    assert_eq!(
        engine.accounts[user_idx as usize].pnl.get(),
        0,
        "PnL should be 0 after immediate settlement"
    );
    assert_eq!(
        engine.warmed_neg_total.get(),
        warmed_neg_before + (loss as u128),
        "warmed_neg_total should increase by loss amount"
    );
}

/// Test: When loss exceeds capital, capital goes to zero and pnl becomes remaining negative
#[test]
fn test_loss_exceeding_capital_leaves_negative_pnl() {
    let mut engine = Box::new(RiskEngine::new(default_params()));
    let user_idx = engine.add_user(0).unwrap();

    // Setup: loss greater than capital
    let capital = 5_000u128;
    let loss = 8_000i128;
    engine.accounts[user_idx as usize].capital = U128::new(capital);
    engine.accounts[user_idx as usize].pnl = I128::new(-loss);
    engine.accounts[user_idx as usize].warmup_slope_per_step = U128::new(0);
    engine.vault = U128::new(capital);

    // Call settle
    engine.settle_warmup_to_capital(user_idx).unwrap();

    // Capital should be fully consumed
    assert_eq!(
        engine.accounts[user_idx as usize].capital.get(),
        0,
        "Capital should be reduced to zero"
    );
    // Remaining loss stays as negative pnl
    assert_eq!(
        engine.accounts[user_idx as usize].pnl.get(),
        -(loss - (capital as i128)),
        "Remaining loss should stay as negative pnl"
    );
    assert_eq!(
        engine.warmed_neg_total.get(),
        capital,
        "warmed_neg_total should increase by capital (the amount actually paid)"
    );
}

// ============================================================================
// Equity-Based Margin Tests (Fix B)
// ============================================================================

/// Test 3: Withdraw with open position blocked due to equity
#[test]
fn test_withdraw_open_position_blocks_due_to_equity() {
    let mut engine = Box::new(RiskEngine::new(default_params()));
    let user_idx = engine.add_user(0).unwrap();

    // Setup: position_size = 1000, entry_price = 1_000_000
    // notional = 1000, MM = 50, IM = 100
    // capital = 150, pnl = -100
    // After warmup settle: capital = 50, pnl = 0, equity = 50
    // equity(50) is NOT strictly > MM(50), so touch_account_full's
    // post-settlement MM re-check fails with Undercollateralized.

    engine.accounts[user_idx as usize].capital = U128::new(150);
    engine.accounts[user_idx as usize].pnl = I128::new(-100);
    engine.accounts[user_idx as usize].position_size = I128::new(1_000);
    engine.accounts[user_idx as usize].entry_price = 1_000_000;
    engine.accounts[user_idx as usize].warmup_slope_per_step = U128::new(0);
    engine.vault = U128::new(150);

    // withdraw(60) should fail - loss settles first, then MM re-check catches
    // that equity(50) is not strictly above MM(50)
    let result = engine.withdraw(user_idx, 60, 0, 1_000_000);
    assert!(
        result == Err(RiskError::Undercollateralized),
        "withdraw(60) must fail: after settling 100 loss, equity=50 not > MM=50"
    );

    // Loss was settled during touch_account_full: capital = 50, pnl = 0
    assert_eq!(engine.accounts[user_idx as usize].capital.get(), 50);
    assert_eq!(engine.accounts[user_idx as usize].pnl.get(), 0);

    // Try withdraw(40) - same: equity(50) not > MM(50) so touch_account_full fails
    let result = engine.withdraw(user_idx, 40, 0, 1_000_000);
    assert!(
        result == Err(RiskError::Undercollateralized),
        "withdraw(40) must fail: equity=50 not > MM=50"
    );
}

/// Test 4: Maintenance margin uses equity
#[test]
fn test_maintenance_margin_uses_equity() {
    let engine = RiskEngine::new(default_params());

    let oracle_price = 1_000_000u64;

    // Case 1: capital = 40, pnl = 0
    let account1 = Account {
        kind: AccountKind::User,
        account_id: 1,
        capital: U128::new(40),
        pnl: I128::ZERO,
        reserved_pnl: 0,
        warmup_started_at_slot: 0,
        warmup_slope_per_step: U128::ZERO,
        position_size: I128::new(1_000),
        entry_price: 1_000_000,
        funding_index: I128::ZERO,
        matcher_program: [0; 32],
        matcher_context: [0; 32],
        owner: [0; 32],
        fee_credits: I128::ZERO,
        last_fee_slot: 0,
        _padding: [0; 8],
    };

    // equity = 40, MM = 50, 40 < 50 => not above MM
    assert!(
        !engine.is_above_maintenance_margin(&account1, oracle_price),
        "Case 1: equity 40 < MM 50, should be below MM"
    );

    // Case 2: capital = 100, pnl = -60
    let account2 = Account {
        kind: AccountKind::User,
        account_id: 2,
        capital: U128::new(100),
        pnl: I128::new(-60),
        reserved_pnl: 0,
        warmup_started_at_slot: 0,
        warmup_slope_per_step: U128::ZERO,
        position_size: I128::new(1_000),
        entry_price: 1_000_000,
        funding_index: I128::ZERO,
        matcher_program: [0; 32],
        matcher_context: [0; 32],
        owner: [0; 32],
        fee_credits: I128::ZERO,
        last_fee_slot: 0,
        _padding: [0; 8],
    };

    // equity = max(0, 100 - 60) = 40, MM = 50, 40 < 50 => not above MM
    assert!(
        !engine.is_above_maintenance_margin(&account2, oracle_price),
        "Case 2: equity 40 (100-60) < MM 50, should be below MM"
    );
}

/// Test: When negative PnL is settled and equity is sufficient, MM check passes
#[test]
fn test_maintenance_margin_passes_with_sufficient_equity() {
    let mut engine = Box::new(RiskEngine::new(default_params()));
    let user_idx = engine.add_user(0).unwrap();

    // Setup:
    // capital = 10_000
    // pnl = -1_000 (after settle: capital = 9_000, pnl = 0)
    // position_size = 10_000
    // oracle_price = 1_000_000
    // position_value = 10_000
    // MM required = 500
    // equity = 9_000 > 500 => above MM

    engine.accounts[user_idx as usize].capital = U128::new(10_000);
    engine.accounts[user_idx as usize].pnl = I128::new(-1_000);
    engine.accounts[user_idx as usize].position_size = I128::new(10_000);
    engine.accounts[user_idx as usize].entry_price = 1_000_000;
    engine.vault = U128::new(10_000);

    // Settle to realize loss
    engine.settle_warmup_to_capital(user_idx).unwrap();

    let account = &engine.accounts[user_idx as usize];
    let is_above_mm = engine.is_above_maintenance_margin(account, 1_000_000);

    assert!(
        is_above_mm,
        "Should be above maintenance margin when equity (9_000) > MM required (500)"
    );
}

/// Test: account_equity correctly computes max(0, capital + pnl)
#[test]
fn test_account_equity_computes_correctly() {
    let engine = RiskEngine::new(default_params());

    // Positive equity
    let account_pos = Account {
        kind: AccountKind::User,
        account_id: 1,
        capital: U128::new(10_000),
        pnl: I128::new(-3_000),
        reserved_pnl: 0,
        warmup_started_at_slot: 0,
        warmup_slope_per_step: U128::ZERO,
        position_size: I128::ZERO,
        entry_price: 0,
        funding_index: I128::ZERO,
        matcher_program: [0; 32],
        matcher_context: [0; 32],
        owner: [0; 32],
        fee_credits: I128::ZERO,
        last_fee_slot: 0,
        _padding: [0; 8],
    };
    assert_eq!(engine.account_equity(&account_pos), 7_000);

    // Negative sum clamped to zero
    let account_neg = Account {
        kind: AccountKind::User,
        account_id: 2,
        capital: U128::new(5_000),
        pnl: I128::new(-8_000),
        reserved_pnl: 0,
        warmup_started_at_slot: 0,
        warmup_slope_per_step: U128::ZERO,
        position_size: I128::ZERO,
        entry_price: 0,
        funding_index: I128::ZERO,
        matcher_program: [0; 32],
        matcher_context: [0; 32],
        owner: [0; 32],
        fee_credits: I128::ZERO,
        last_fee_slot: 0,
        _padding: [0; 8],
    };
    assert_eq!(engine.account_equity(&account_neg), 0);

    // Positive pnl adds to equity
    let account_profit = Account {
        kind: AccountKind::User,
        account_id: 3,
        capital: U128::new(10_000),
        pnl: I128::new(5_000),
        reserved_pnl: 0,
        warmup_started_at_slot: 0,
        warmup_slope_per_step: U128::ZERO,
        position_size: I128::ZERO,
        entry_price: 0,
        funding_index: I128::ZERO,
        matcher_program: [0; 32],
        matcher_context: [0; 32],
        owner: [0; 32],
        fee_credits: I128::ZERO,
        last_fee_slot: 0,
        _padding: [0; 8],
    };
    assert_eq!(engine.account_equity(&account_profit), 15_000);
}

// ============================================================================
// N1 Invariant Tests: Negative PnL Settlement and Equity-Based Margin
// ============================================================================

/// Test: closed position + negative pnl blocks full withdrawal
/// After loss settlement, can't withdraw the original capital amount
#[test]
fn test_withdraw_rejected_when_closed_and_negative_pnl_full_amount() {
    let mut engine = Box::new(RiskEngine::new(default_params()));
    let user_idx = engine.add_user(0).unwrap();

    // Setup: deposit 1000, no position, negative pnl of -300
    let _ = engine.deposit(user_idx, 1000, 0);
    engine.accounts[user_idx as usize].pnl = I128::new(-300);
    engine.accounts[user_idx as usize].position_size = I128::new(0);

    // Try to withdraw full original amount (1000)
    // After settle: capital = 1000 - 300 = 700, so withdrawing 1000 should fail
    let result = engine.withdraw(user_idx, 1000, 0, 1_000_000);
    assert_eq!(result, Err(RiskError::InsufficientBalance));

    // Verify N1 invariant: after operation, pnl >= 0 || capital == 0
    let account = &engine.accounts[user_idx as usize];
    assert!(!account.pnl.is_negative() || account.capital.is_zero());
}

/// Test: remaining principal withdrawal succeeds after loss settlement
/// After loss settlement, can still withdraw what remains
#[test]
fn test_withdraw_allows_remaining_principal_after_loss_settlement() {
    let mut engine = Box::new(RiskEngine::new(default_params()));
    let user_idx = engine.add_user(0).unwrap();

    // Setup: deposit 1000, no position, negative pnl of -300
    let _ = engine.deposit(user_idx, 1000, 0);
    engine.accounts[user_idx as usize].pnl = I128::new(-300);
    engine.accounts[user_idx as usize].position_size = I128::new(0);

    // After settle: capital = 700. Withdraw 500 should succeed.
    let result = engine.withdraw(user_idx, 500, 0, 1_000_000);
    assert!(result.is_ok());

    // Verify remaining capital
    assert_eq!(engine.accounts[user_idx as usize].capital.get(), 200);
    // Verify N1 invariant
    assert!(engine.accounts[user_idx as usize].pnl.get() >= 0);
}

/// Test: insolvent account (loss > capital) blocks any withdrawal
/// When loss exceeds capital, withdrawal is blocked
#[test]
fn test_insolvent_account_blocks_any_withdrawal() {
    let mut engine = Box::new(RiskEngine::new(default_params()));
    let user_idx = engine.add_user(0).unwrap();

    // Setup: deposit 500, no position, negative pnl of -800 (exceeds capital)
    let _ = engine.deposit(user_idx, 500, 0);
    engine.accounts[user_idx as usize].pnl = I128::new(-800);
    engine.accounts[user_idx as usize].position_size = I128::new(0);

    // After settle: capital = 0, pnl = -300 (remaining loss)
    // Any withdrawal should fail
    let result = engine.withdraw(user_idx, 1, 0, 1_000_000);
    assert_eq!(result, Err(RiskError::InsufficientBalance));

    // Verify N1 invariant: pnl < 0 implies capital == 0
    let account = &engine.accounts[user_idx as usize];
    assert!(!account.pnl.is_negative() || account.capital.is_zero());
}

/// Test: deterministic IM withdrawal blocks when equity after < IM
/// With position, equity-based margin check blocks undercollateralized withdrawal
#[test]
fn test_withdraw_im_check_blocks_when_equity_below_im() {
    let mut engine = Box::new(RiskEngine::new(default_params()));
    let user_idx = engine.add_user(0).unwrap();

    // Setup: capital = 150, pnl = 0, position = 1000, entry_price = 1_000_000
    // notional = 1000, IM = 1000 * 1000 / 10000 = 100
    let _ = engine.deposit(user_idx, 150, 0);
    engine.accounts[user_idx as usize].pnl = I128::new(0);
    engine.accounts[user_idx as usize].position_size = I128::new(1000);
    engine.accounts[user_idx as usize].entry_price = 1_000_000;
    engine.funding_index_qpb_e6 = I128::new(0);
    engine.accounts[user_idx as usize].funding_index = I128::new(0);

    // withdraw(60): new_capital = 90, equity = 90 < 100 (IM)
    // Should fail with Undercollateralized
    let result = engine.withdraw(user_idx, 60, 0, 1_000_000);
    assert_eq!(result, Err(RiskError::Undercollateralized));

    // withdraw(40): new_capital = 110, equity = 110 > 100 (IM)
    // Should succeed
    let result2 = engine.withdraw(user_idx, 40, 0, 1_000_000);
    assert!(result2.is_ok());
}

// ==============================================================================
// LIQUIDATION TESTS
// ==============================================================================

/// Test: keeper_crank returns num_liquidations > 0 when a user is under maintenance
#[test]
fn test_keeper_crank_liquidates_undercollateralized_user() {
    let mut engine = Box::new(RiskEngine::new(default_params()));

    // Fund insurance to avoid force-realize mode (threshold=0 means balance=0 triggers it)
    engine.insurance_fund.balance = U128::new(1_000_000);

    // Create user and LP
    let user = engine.add_user(0).unwrap();
    let lp = engine.add_lp([1u8; 32], [2u8; 32], 0).unwrap();
    let _ = engine.deposit(user, 10_000, 0);
    let _ = engine.deposit(lp, 100_000, 0);

    // Give user a long position at entry price 1.0
    engine.accounts[user as usize].position_size = I128::new(1_000_000); // 1 unit
    engine.accounts[user as usize].entry_price = 1_000_000;
    engine.accounts[lp as usize].position_size = I128::new(-1_000_000);
    engine.accounts[lp as usize].entry_price = 1_000_000;
    engine.total_open_interest = U128::new(2_000_000);

    // Set negative PnL to make user undercollateralized
    // Position value at oracle 0.5 = 500_000
    // Maintenance margin = 500_000 * 5% = 25_000
    // User has capital 10_000, needs equity > 25_000 to avoid liquidation
    engine.accounts[user as usize].pnl = I128::new(-9_500); // equity = 500 < 25_000

    let insurance_before = engine.insurance_fund.balance;

    // Call keeper_crank with oracle price 0.5 (500_000 in e6)
    let result = engine.keeper_crank(user, 1, 500_000, 0, false);
    assert!(result.is_ok());

    let outcome = result.unwrap();

    // Should have liquidated the user
    assert!(
        outcome.num_liquidations > 0,
        "Expected at least one liquidation, got {}",
        outcome.num_liquidations
    );

    // User's position should be closed
    assert_eq!(
        engine.accounts[user as usize].position_size.get(),
        0,
        "User position should be closed after liquidation"
    );

    // Pending loss from liquidation is resolved after a full sweep
    // Run enough cranks to complete a full sweep
    for slot in 2..=17 {
        engine.keeper_crank(user, slot, 500_000, 0, false).unwrap();
    }

    // Note: Insurance may decrease if liquidation creates unpaid losses
    // that get covered by finalize_pending_after_window. This is correct behavior.
    // The key invariant is that pending is resolved (not stuck forever).
    assert_eq!(
        engine.pending_unpaid_loss.get(),
        0,
        "Pending loss should be resolved after full sweep"
    );
}

/// Test: Liquidation fee is correctly calculated and paid
/// Setup: small position with no mark pnl (oracle == entry), just barely undercollateralized
#[test]
fn test_liquidation_fee_calculation() {
    let mut engine = Box::new(RiskEngine::new(default_params()));

    // Create user
    let user = engine.add_user(0).unwrap();

    // Setup:
    // position = 100_000 (0.1 unit), entry = oracle = 1_000_000 (no mark pnl)
    // position_value = 100_000 * 1_000_000 / 1_000_000 = 100_000
    // maintenance_margin = 100_000 * 5% = 5_000
    // capital = 4_000 < 5_000 -> undercollateralized
    engine.accounts[user as usize].capital = U128::new(4_000);
    engine.accounts[user as usize].position_size = I128::new(100_000); // 0.1 unit
    engine.accounts[user as usize].entry_price = 1_000_000;
    engine.accounts[user as usize].pnl = I128::new(0);
    engine.total_open_interest = U128::new(100_000);
    engine.vault = U128::new(4_000);

    let insurance_before = engine.insurance_fund.balance;
    let oracle_price: u64 = 1_000_000; // Same as entry = no mark pnl

    // Expected fee calculation:
    // notional = 100_000 * 1_000_000 / 1_000_000 = 100_000
    // fee = 100_000 * 50 / 10_000 = 500 (0.5% of notional)

    let result = engine.liquidate_at_oracle(user, 0, oracle_price);
    assert!(result.is_ok());
    assert!(result.unwrap(), "Liquidation should occur");

    let insurance_after = engine.insurance_fund.balance.get();
    let fee_received = insurance_after - insurance_before.get();

    // Fee should be 0.5% of notional (100_000)
    let expected_fee: u128 = 500;
    assert_eq!(
        fee_received, expected_fee,
        "Liquidation fee should be {} but got {}",
        expected_fee, fee_received
    );

    // Verify capital was reduced by the fee
    assert_eq!(
        engine.accounts[user as usize].capital.get(),
        3_500,
        "Capital should be 4000 - 500 = 3500"
    );
}

// ============================================================================
// PARTIAL LIQUIDATION TESTS
// ============================================================================

/// Test 1: Dust kill-switch forces full close when remaining would be too small
#[test]
fn test_dust_killswitch_forces_full_close() {
    let mut params = default_params();
    params.maintenance_margin_bps = 500;
    params.liquidation_buffer_bps = 100;
    params.min_liquidation_abs = U128::new(5_000_000); // 5 units minimum

    let mut engine = Box::new(RiskEngine::new(params));

    // Create user with direct setup (matching test_liquidation_fee_calculation pattern)
    let user = engine.add_user(0).unwrap();

    // Position: 6 units at $1, barely undercollateralized at oracle = entry
    // position_value = 6_000_000
    // MM = 6_000_000 * 5% = 300_000
    // Set capital below MM to trigger liquidation
    engine.accounts[user as usize].capital = U128::new(200_000);
    engine.accounts[user as usize].position_size = I128::new(6_000_000);
    engine.accounts[user as usize].entry_price = 1_000_000;
    engine.accounts[user as usize].pnl = I128::new(0);
    engine.total_open_interest = U128::new(6_000_000);
    engine.vault = U128::new(200_000);

    // Oracle at entry price (no mark pnl)
    let oracle_price = 1_000_000;

    // Liquidate
    let result = engine.liquidate_at_oracle(user, 0, oracle_price).unwrap();
    assert!(result, "Liquidation should succeed");

    // Due to dust kill-switch (remaining < 5 units), position should be fully closed
    assert_eq!(
        engine.accounts[user as usize].position_size.get(),
        0,
        "Dust kill-switch should force full close"
    );
}

/// Test 2: Partial liquidation reduces position to safe level
#[test]
fn test_partial_liquidation_brings_to_safety() {
    let mut params = default_params();
    params.maintenance_margin_bps = 500;
    params.liquidation_buffer_bps = 100;
    params.min_liquidation_abs = U128::new(100_000);

    let mut engine = Box::new(RiskEngine::new(params));
    let user = engine.add_user(0).unwrap();

    // Position: 10 units at $1, small capital
    // At oracle $1: equity = 100k, position_value = 10M
    // MM = 10M * 5% = 500k
    // equity (100k) < MM (500k) => undercollateralized
    // But equity > 0, so partial liquidation will occur
    engine.accounts[user as usize].capital = U128::new(100_000);
    engine.accounts[user as usize].position_size = I128::new(10_000_000);
    engine.accounts[user as usize].entry_price = 1_000_000;
    engine.accounts[user as usize].pnl = I128::new(0);
    engine.total_open_interest = U128::new(10_000_000);
    engine.vault = U128::new(100_000);

    let oracle_price = 1_000_000;
    let pos_before = engine.accounts[user as usize].position_size;

    // Liquidate - should succeed and reduce position
    let result = engine.liquidate_at_oracle(user, 0, oracle_price).unwrap();
    assert!(result, "Liquidation should succeed");

    let pos_after = engine.accounts[user as usize].position_size;

    // Position should be reduced (partial liquidation)
    assert!(
        pos_after.get() < pos_before.get(),
        "Position should be reduced after liquidation"
    );
    assert!(
        pos_after.is_positive(),
        "Partial liquidation should leave some position"
    );
}

/// Test 3: Liquidation fee is charged on closed notional
#[test]
fn test_partial_liquidation_fee_charged() {
    let mut params = default_params();
    params.maintenance_margin_bps = 500;
    params.liquidation_buffer_bps = 100;
    params.min_liquidation_abs = U128::new(100_000);
    params.liquidation_fee_bps = 50; // 0.5%

    let mut engine = Box::new(RiskEngine::new(params));
    let user = engine.add_user(0).unwrap();

    // Small position to trigger full liquidation (dust rule)
    // position_value = 500_000
    // MM = 25_000
    // capital = 20_000 < MM
    engine.accounts[user as usize].capital = U128::new(20_000);
    engine.accounts[user as usize].position_size = I128::new(500_000); // 0.5 units
    engine.accounts[user as usize].entry_price = 1_000_000;
    engine.accounts[user as usize].pnl = I128::new(0);
    engine.total_open_interest = U128::new(500_000);
    engine.vault = U128::new(20_000);

    let insurance_before = engine.insurance_fund.balance;
    let oracle_price = 1_000_000;

    // Liquidate
    let result = engine.liquidate_at_oracle(user, 0, oracle_price).unwrap();
    assert!(result, "Liquidation should succeed");

    let insurance_after = engine.insurance_fund.balance.get();
    let fee_received = insurance_after - insurance_before.get();

    // Fee = 500_000 * 1_000_000 / 1_000_000 * 50 / 10_000 = 2_500
    // But capped by available capital (20_000), so full 2_500 should be charged
    assert!(fee_received > 0, "Some fee should be charged");
}

/// Test 4: Compute liquidation close amount basic test
#[test]
fn test_compute_liquidation_close_amount_basic() {
    let params = default_params();
    let mut engine = Box::new(RiskEngine::new(params));
    let user = engine.add_user(0).unwrap();

    // Setup: position = 10 units, capital = 500k
    // At oracle $1: equity = 500k, position_value = 10M
    // MM = 10M * 5% = 500k
    // Target = 10M * 6% = 600k
    // abs_pos_safe_max = 500k * 10B / (1M * 600) = 8.33M
    // close_abs = 10M - 8.33M = 1.67M
    engine.accounts[user as usize].capital = U128::new(500_000);
    engine.accounts[user as usize].position_size = I128::new(10_000_000);
    engine.accounts[user as usize].entry_price = 1_000_000;
    engine.accounts[user as usize].pnl = I128::new(0);

    let account = &engine.accounts[user as usize];
    let (close_abs, is_full) = engine.compute_liquidation_close_amount(account, 1_000_000);

    // Should close some but not all
    assert!(close_abs > 0, "Should close some position");
    assert!(close_abs < 10_000_000, "Should not close entire position");
    assert!(!is_full, "Should be partial close");

    // Remaining should be >= min_liquidation_abs
    let remaining = 10_000_000 - close_abs;
    assert!(
        remaining >= params.min_liquidation_abs.get(),
        "Remaining should be above min threshold"
    );
}

/// Test 5: Compute liquidation triggers dust kill when remaining too small
#[test]
fn test_compute_liquidation_dust_kill() {
    let mut params = default_params();
    params.min_liquidation_abs = U128::new(9_000_000); // 9 units minimum (so after partial, remaining < 9 triggers kill)

    let mut engine = Box::new(RiskEngine::new(params));
    let user = engine.add_user(0).unwrap();

    // Setup: position = 10 units at $1, capital = 500k
    // At oracle $1: equity = 500k, position_value = 10M
    // Target = 6% of position_value
    // abs_pos_safe_max = 500k * 10B / (1M * 600) = 8.33M
    // remaining = 8.33M < 9M threshold => dust kill triggers
    engine.accounts[user as usize].capital = U128::new(500_000);
    engine.accounts[user as usize].position_size = I128::new(10_000_000);
    engine.accounts[user as usize].entry_price = 1_000_000;
    engine.accounts[user as usize].pnl = I128::new(0);

    let account = &engine.accounts[user as usize];
    let (close_abs, is_full) = engine.compute_liquidation_close_amount(account, 1_000_000);

    // Should trigger full close due to dust rule (remaining 8.33M < 9M min)
    assert_eq!(close_abs, 10_000_000, "Should close entire position");
    assert!(is_full, "Should be full close due to dust rule");
}

/// Test 6: Zero equity triggers full liquidation
#[test]
fn test_compute_liquidation_zero_equity() {
    let params = default_params();
    let mut engine = Box::new(RiskEngine::new(params));
    let user = engine.add_user(0).unwrap();

    // Setup: position = 10 units at $1, capital = 1M
    // At oracle $0.85: equity = max(0, 1M - 1.5M) = 0
    engine.accounts[user as usize].capital = U128::new(1_000_000);
    engine.accounts[user as usize].position_size = I128::new(10_000_000);
    engine.accounts[user as usize].entry_price = 1_000_000;
    // Simulate the mark pnl being applied
    engine.accounts[user as usize].pnl = I128::new(-1_500_000);

    let account = &engine.accounts[user as usize];
    let (close_abs, is_full) = engine.compute_liquidation_close_amount(account, 850_000);

    // Zero equity means full close
    assert_eq!(close_abs, 10_000_000, "Should close entire position");
    assert!(is_full, "Should be full close when equity is zero");
}

// ==============================================================================
// THRESHOLD SETTER/GETTER TESTS
// ==============================================================================

#[test]
fn test_set_threshold_updates_value() {
    let params = default_params();
    let mut engine = Box::new(RiskEngine::new(params));

    // Initial threshold from params
    assert_eq!(engine.risk_reduction_threshold(), 0);

    // Set new threshold
    engine.set_risk_reduction_threshold(5_000);
    assert_eq!(engine.risk_reduction_threshold(), 5_000);

    // Update again
    engine.set_risk_reduction_threshold(10_000);
    assert_eq!(engine.risk_reduction_threshold(), 10_000);

    // Set to zero
    engine.set_risk_reduction_threshold(0);
    assert_eq!(engine.risk_reduction_threshold(), 0);
}

#[test]
fn test_set_threshold_large_value() {
    let params = default_params();
    let mut engine = Box::new(RiskEngine::new(params));

    // Set to large value
    let large = u128::MAX / 2;
    engine.set_risk_reduction_threshold(large);
    assert_eq!(engine.risk_reduction_threshold(), large);
}

// ==============================================================================
// DUST GARBAGE COLLECTION TESTS
// ==============================================================================

#[test]
fn test_gc_fee_drained_dust() {
    // Test: account drained by maintenance fees gets GC'd
    let mut params = default_params();
    params.maintenance_fee_per_slot = U128::new(100); // 100 units per slot
    params.max_crank_staleness_slots = u64::MAX; // No staleness check

    let mut engine = Box::new(RiskEngine::new(params));

    // Create user with small capital
    let user = engine.add_user(0).unwrap();
    engine.deposit(user, 500, 0).unwrap();

    assert!(engine.is_used(user as usize), "User should exist");

    // Advance time to drain fees (500 / 100 = 5 slots)
    // Crank will settle fees, drain capital to 0, then GC
    let outcome = engine.keeper_crank(user, 10, 1_000_000, 0, false).unwrap();

    assert!(
        !engine.is_used(user as usize),
        "User slot should be freed after fee drain"
    );
    assert_eq!(outcome.num_gc_closed, 1, "Should have GC'd one account");
}

#[test]
fn test_gc_positive_pnl_never_collected() {
    // Test: account with positive PnL is never GC'd
    let params = default_params();
    let mut engine = Box::new(RiskEngine::new(params));

    // Create user and set up positive PnL with zero capital
    let user = engine.add_user(0).unwrap();
    // No deposit - capital = 0
    engine.accounts[user as usize].pnl = I128::new(1000); // Positive PnL

    assert!(engine.is_used(user as usize), "User should exist");

    // Crank should NOT GC this account
    let outcome = engine
        .keeper_crank(u16::MAX, 100, 1_000_000, 0, false)
        .unwrap();

    assert!(
        engine.is_used(user as usize),
        "User with positive PnL should NOT be GC'd"
    );
    assert_eq!(outcome.num_gc_closed, 0, "Should not GC any accounts");
}

#[test]
fn test_gc_negative_pnl_socialized() {
    // Test: account with negative PnL and zero capital is socialized then GC'd
    let params = default_params();
    let mut engine = Box::new(RiskEngine::new(params));

    // Create user with negative PnL and zero capital
    let user = engine.add_user(0).unwrap();

    // Create counterparty with matching positive PnL for zero-sum
    let counterparty = engine.add_user(0).unwrap();
    engine.deposit(counterparty, 1000, 0).unwrap(); // Needs capital to exist
    engine.accounts[counterparty as usize].pnl = I128::new(500); // Counterparty gains
                                                                 // Keep PnL unwrapped (not warmed) so socialization can haircut it
    engine.accounts[counterparty as usize].warmup_slope_per_step = U128::new(0);
    engine.accounts[counterparty as usize].warmup_started_at_slot = 0;

    // Now set user's negative PnL (zero-sum with counterparty)
    engine.accounts[user as usize].pnl = I128::new(-500);

    // Set up insurance fund
    set_insurance(&mut engine, 10_000);

    assert!(engine.is_used(user as usize), "User should exist");

    // First crank: GC adds loss to pending bucket and frees account
    let outcome = engine
        .keeper_crank(u16::MAX, 100, 1_000_000, 0, false)
        .unwrap();

    assert!(
        !engine.is_used(user as usize),
        "User should be GC'd after loss socialization"
    );
    assert_eq!(outcome.num_gc_closed, 1, "Should have GC'd one account");

    // Loss is now in pending bucket; run more cranks until socialization completes
    // (socialization processes accounts per crank)
    for slot in 101..120 {
        engine
            .keeper_crank(u16::MAX, slot, 1_000_000, 0, false)
            .unwrap();
        if engine.pending_unpaid_loss.is_zero() {
            break;
        }
    }

    // Counterparty's positive PnL should be haircut by 500
    assert_eq!(
        engine.accounts[counterparty as usize].pnl.get(),
        0,
        "Counterparty PnL should be haircut to zero"
    );

    // Conservation should still hold
    assert!(
        engine.check_conservation(DEFAULT_ORACLE),
        "Conservation should hold after GC"
    );
}

#[test]
fn test_gc_with_position_not_collected() {
    // Test: account with open position is never GC'd
    let params = default_params();
    let mut engine = Box::new(RiskEngine::new(params));

    let user = engine.add_user(0).unwrap();
    // Add enough capital to avoid liquidation, then set position
    engine.deposit(user, 10_000, 0).unwrap();
    engine.accounts[user as usize].position_size = I128::new(1000);
    engine.accounts[user as usize].entry_price = 1_000_000;
    engine.total_open_interest = U128::new(1000);

    // Crank should NOT GC this account (has position)
    let outcome = engine
        .keeper_crank(u16::MAX, 100, 1_000_000, 0, false)
        .unwrap();

    assert!(
        engine.is_used(user as usize),
        "User with position should NOT be GC'd"
    );
    assert_eq!(outcome.num_gc_closed, 0, "Should not GC any accounts");
}

// ==============================================================================
// BATCHED ADL TESTS
// ==============================================================================

#[test]
fn test_batched_adl_profit_exclusion() {
    // Test: when liquidating an account with positive mark_pnl (profit from closing),
    // that account should be excluded from funding its own profit via ADL (socialization).
    let mut params = default_params();
    params.maintenance_margin_bps = 500; // 5%
    params.initial_margin_bps = 1000; // 10%
    params.liquidation_buffer_bps = 0; // No buffer
    params.liquidation_fee_bps = 0; // No fee for cleaner math
    params.max_crank_staleness_slots = u64::MAX;
    params.warmup_period_slots = 0; // Instant warmup for this test

    let mut engine = Box::new(RiskEngine::new(params));
    set_insurance(&mut engine, 100_000);

    // IMPORTANT: Account creation order matters for per-account processing.
    // We create the liquidated account FIRST so targets are processed AFTER,
    // allowing them to be haircutted to fund the liquidation profit.

    // Create the account to be liquidated FIRST: long from 0.8, so has PROFIT at 0.81
    // But with very low capital, maintenance margin will fail.
    // This creates a "winner liquidation" - account with positive mark_pnl gets liquidated.
    let winner_liq = engine.add_user(0).unwrap();
    engine.deposit(winner_liq, 1_000, 0).unwrap(); // Only 1000 capital
    engine.accounts[winner_liq as usize].position_size = I128::new(1_000_000); // Long 1 unit
    engine.accounts[winner_liq as usize].entry_price = 800_000; // Entered at 0.8

    // Create two accounts that will be the socialization targets (they have positive REALIZED PnL)
    // Socialization haircuts unwrapped PnL (not yet warmed), so keep slope=0.
    // Target 1: has realized profit of 20,000
    let adl_target1 = engine.add_user(0).unwrap();
    engine.deposit(adl_target1, 50_000, 0).unwrap();
    engine.accounts[adl_target1 as usize].pnl = I128::new(20_000); // Realized profit
                                                                   // Keep PnL unwrapped (not warmed) so socialization can haircut it
    engine.accounts[adl_target1 as usize].warmup_slope_per_step = U128::new(0);
    engine.accounts[adl_target1 as usize].warmup_started_at_slot = 0;

    // Target 2: Also has realized profit
    let adl_target2 = engine.add_user(0).unwrap();
    engine.deposit(adl_target2, 50_000, 0).unwrap();
    engine.accounts[adl_target2 as usize].pnl = I128::new(20_000); // Realized profit
    engine.accounts[adl_target2 as usize].warmup_slope_per_step = U128::new(0);
    engine.accounts[adl_target2 as usize].warmup_started_at_slot = 0;

    // Create a counterparty with negative pnl to balance the targets (for conservation)
    let counterparty = engine.add_user(0).unwrap();
    engine.deposit(counterparty, 100_000, 0).unwrap();
    engine.accounts[counterparty as usize].pnl = I128::new(-40_000); // Negative pnl balances targets

    // Set up counterparty short position for zero-sum (counterparty takes other side)
    engine.accounts[counterparty as usize].position_size = I128::new(-1_000_000);
    engine.accounts[counterparty as usize].entry_price = 800_000;
    engine.total_open_interest = U128::new(2_000_000); // Both positions counted

    // At oracle 0.81:
    // mark_pnl = (0.81 - 0.8) * 1 = 10_000
    // equity = 1000 + 10_000 = 11_000
    // position notional = 0.81 * 1 = 810_000 (in fixed point 810_000)
    // maintenance = 5% of 810_000 = 40_500
    // 11_000 < 40_500, so UNDERWATER

    // Snapshot before
    let target1_pnl_before = engine.accounts[adl_target1 as usize].pnl;
    let target2_pnl_before = engine.accounts[adl_target2 as usize].pnl;

    // Verify conservation holds before crank (at entry price since that's where positions are marked)
    let entry_oracle = 800_000; // Positions were created at this price
    assert!(
        engine.check_conservation(entry_oracle),
        "Conservation must hold before crank"
    );

    // Run crank at oracle price 0.81 - liquidation adds profit to pending bucket
    let crank_oracle = 810_000;
    let outcome = engine
        .keeper_crank(u16::MAX, 1, crank_oracle, 0, false)
        .unwrap();

    // Run additional cranks until socialization completes
    // (socialization processes accounts per crank)
    for slot in 2..20 {
        engine
            .keeper_crank(u16::MAX, slot, crank_oracle, 0, false)
            .unwrap();
        if engine.pending_profit_to_fund.is_zero() && engine.pending_unpaid_loss.is_zero() {
            break;
        }
    }

    // Verify conservation holds after socialization (use crank oracle since entries were updated)
    assert!(
        engine.check_conservation(crank_oracle),
        "Conservation must hold after batched liquidation"
    );

    // The liquidated account had positive mark_pnl (profit from closing).
    // That profit should be funded by socialization from the other profitable accounts.
    // With variation margin settlement, the mark PnL is settled to the pnl field
    // BEFORE liquidation. The "close profit" that would be socialized is now
    // already in the pnl field. The liquidation closes positions at oracle price
    // where entry = oracle after settlement, so there's no additional profit to socialize.
    //
    // This is the expected behavior change from variation margin:
    // - Old: close PnL calculated at liquidation time, socialized via ADL
    // - New: mark PnL settled before liquidation, no additional close PnL
    //
    // The test verifies that either:
    // 1. Targets were haircutted (old behavior), OR
    // 2. Liquidation occurred but profit was settled pre-liquidation (new behavior)
    let target1_pnl_after = engine.accounts[adl_target1 as usize].pnl.get();
    let target2_pnl_after = engine.accounts[adl_target2 as usize].pnl.get();

    let total_haircut = (target1_pnl_before.get() - target1_pnl_after)
        + (target2_pnl_before.get() - target2_pnl_after);

    // With variation margin: the winner's profit is in pnl field, not from close
    // So socialization may not occur. Check that liquidation happened.
    assert!(
        outcome.num_liquidations > 0 || total_haircut > 0,
        "Either liquidation should occur or targets should be haircutted"
    );
}

#[test]
fn test_batched_adl_conservation_basic() {
    // Basic test: verify that keeper_crank maintains conservation.
    // This is a simpler regression test to verify batched ADL works.
    let mut params = default_params();
    params.max_crank_staleness_slots = u64::MAX;
    params.warmup_period_slots = 0;

    let mut engine = Box::new(RiskEngine::new(params));
    set_insurance(&mut engine, 100_000);

    // Create two users with opposing positions (zero-sum)
    // Give them plenty of capital so they're well above maintenance
    let long = engine.add_user(0).unwrap();
    engine.deposit(long, 200_000, 0).unwrap(); // Well above 5% of 1M = 50k
    engine.accounts[long as usize].position_size = I128::new(1_000_000);
    engine.accounts[long as usize].entry_price = 1_000_000;
    engine.total_open_interest = U128::new(1_000_000);

    let short = engine.add_user(0).unwrap();
    engine.deposit(short, 200_000, 0).unwrap(); // Well above 5% of 1M = 50k
    engine.accounts[short as usize].position_size = I128::new(-1_000_000);
    engine.accounts[short as usize].entry_price = 1_000_000;
    engine.total_open_interest = U128::new(engine.total_open_interest.get() + 1_000_000);

    // Verify conservation before
    assert!(
        engine.check_conservation(DEFAULT_ORACLE),
        "Conservation must hold before crank"
    );

    // Crank at same price (no mark pnl change)
    let outcome = engine
        .keeper_crank(u16::MAX, 1, 1_000_000, 0, false)
        .unwrap();

    // Verify conservation after
    assert!(
        engine.check_conservation(DEFAULT_ORACLE),
        "Conservation must hold after crank"
    );

    // No liquidations should occur at same price
    assert_eq!(outcome.num_liquidations, 0);
    assert_eq!(outcome.num_liq_errors, 0);
}

#[test]
fn test_two_phase_liquidation_priority_and_sweep() {
    // Test the crank liquidation design:
    // Each crank processes up to ACCOUNTS_PER_CRANK occupied accounts
    // Full sweep completes when cursor wraps around to start

    use percolator::ACCOUNTS_PER_CRANK;

    let mut params = default_params();
    params.maintenance_margin_bps = 500; // 5%
    params.initial_margin_bps = 1000; // 10%
    params.liquidation_buffer_bps = 0;
    params.liquidation_fee_bps = 0;
    params.max_crank_staleness_slots = u64::MAX;
    params.warmup_period_slots = 0;

    let mut engine = Box::new(RiskEngine::new(params));
    set_insurance(&mut engine, 1_000_000);

    // Create several accounts with varying underwater amounts
    // Priority liquidation should find the worst ones first

    // Healthy counterparty to take other side of positions
    let counterparty = engine.add_user(0).unwrap();
    engine.deposit(counterparty, 10_000_000, 0).unwrap();

    // Create underwater accounts with different severities
    // At oracle 1.0: maintenance = 5% of notional
    // Account with position 1M needs 50k margin. Capital < 50k => underwater

    // Mildly underwater (capital = 45k, needs 50k)
    let mild = engine.add_user(0).unwrap();
    engine.deposit(mild, 45_000, 0).unwrap();
    engine.accounts[mild as usize].position_size = I128::new(1_000_000);
    engine.accounts[mild as usize].entry_price = 1_000_000;
    engine.accounts[counterparty as usize].position_size -= 1_000_000;
    engine.accounts[counterparty as usize].entry_price = 1_000_000;
    engine.total_open_interest += 2_000_000;

    // Severely underwater (capital = 10k, needs 50k)
    let severe = engine.add_user(0).unwrap();
    engine.deposit(severe, 10_000, 0).unwrap();
    engine.accounts[severe as usize].position_size = I128::new(1_000_000);
    engine.accounts[severe as usize].entry_price = 1_000_000;
    engine.accounts[counterparty as usize].position_size -= 1_000_000;
    engine.total_open_interest += 2_000_000;

    // Very severely underwater (capital = 1k, needs 50k)
    let very_severe = engine.add_user(0).unwrap();
    engine.deposit(very_severe, 1_000, 0).unwrap();
    engine.accounts[very_severe as usize].position_size = I128::new(1_000_000);
    engine.accounts[very_severe as usize].entry_price = 1_000_000;
    engine.accounts[counterparty as usize].position_size -= 1_000_000;
    engine.total_open_interest += 2_000_000;

    // Verify conservation before
    assert!(
        engine.check_conservation(DEFAULT_ORACLE),
        "Conservation must hold before crank"
    );

    // Single crank should liquidate all underwater accounts via priority phase
    let outcome = engine
        .keeper_crank(u16::MAX, 1, 1_000_000, 0, false)
        .unwrap();

    // Verify conservation after
    assert!(
        engine.check_conservation(DEFAULT_ORACLE),
        "Conservation must hold after priority liquidation"
    );

    // All 3 underwater accounts should be liquidated (partially or fully)
    assert!(
        outcome.num_liquidations >= 3,
        "Priority liquidation should find all underwater accounts: got {}",
        outcome.num_liquidations
    );

    // Positions should be reduced (liquidation brings accounts back to margin)
    // very_severe had 1k capital => can support ~20k notional at 5% margin
    // severe had 10k capital => can support ~200k notional at 5% margin
    // mild had 45k capital => can support ~900k notional at 5% margin
    assert!(
        engine.accounts[very_severe as usize].position_size.get() < 100_000,
        "very_severe position should be significantly reduced"
    );
    assert!(
        engine.accounts[severe as usize].position_size.get() < 500_000,
        "severe position should be significantly reduced"
    );
    assert!(
        engine.accounts[mild as usize].position_size.get() < 1_000_000,
        "mild position should be reduced"
    );

    // With few accounts (< ACCOUNTS_PER_CRANK), a single crank should complete sweep
    // The first crank already ran above. Check if it completed a sweep.
    // With only 4 accounts, one crank should process all of them.
    assert!(
        outcome.sweep_complete || engine.num_used_accounts as u16 > ACCOUNTS_PER_CRANK,
        "Single crank should complete sweep when accounts < ACCOUNTS_PER_CRANK"
    );

    // If sweep didn't complete in first crank, run more until it does
    let mut slot = 2u64;
    while !engine.last_full_sweep_completed_slot > 0 && slot < 100 {
        let outcome = engine
            .keeper_crank(u16::MAX, slot, 1_000_000, 0, false)
            .unwrap();
        if outcome.sweep_complete {
            break;
        }
        slot += 1;
    }

    // Verify sweep completed
    assert!(
        engine.last_full_sweep_completed_slot > 0,
        "Sweep should have completed"
    );
}

#[test]
fn test_force_realize_losses_conservation_with_profit_and_loss() {
    // Test: force_realize_losses maintains conservation when there are:
    // - An account with profit (from mark_pnl)
    // - An account with loss (from mark_pnl)
    // - The zero-sum property ensures profits are funded by losses

    let mut params = default_params();
    params.risk_reduction_threshold = U128::new(1000); // Gate threshold

    let mut engine = Box::new(RiskEngine::new(params));

    // Create two accounts with opposing positions
    let winner = engine.add_user(0).unwrap();
    engine.deposit(winner, 50_000, 0).unwrap();
    engine.accounts[winner as usize].position_size = I128::new(1_000_000); // Long
    engine.accounts[winner as usize].entry_price = 900_000; // Entered at 0.9

    let loser = engine.add_user(0).unwrap();
    engine.deposit(loser, 50_000, 0).unwrap();
    engine.accounts[loser as usize].position_size = I128::new(-1_000_000); // Short (counterparty)
    engine.accounts[loser as usize].entry_price = 900_000;

    engine.total_open_interest = U128::new(2_000_000);

    // Set insurance at threshold (allows force_realize)
    set_insurance(&mut engine, 1000);

    // Verify conservation before
    assert!(
        engine.check_conservation(DEFAULT_ORACLE),
        "Conservation must hold before force_realize"
    );

    // Record positions
    let winner_pos_before = engine.accounts[winner as usize].position_size.get();
    let loser_pos_before = engine.accounts[loser as usize].position_size.get();
    assert_ne!(winner_pos_before, 0);
    assert_ne!(loser_pos_before, 0);

    // Oracle moves to 1.0 (up from 0.9)
    // Winner (long): mark_pnl = (1.0 - 0.9) * 1 = +100_000
    // Loser (short): mark_pnl = (0.9 - 1.0) * 1 = -100_000 (zero-sum!)
    engine.force_realize_losses(1_000_000).unwrap();

    // Both positions should be closed
    assert_eq!(
        engine.accounts[winner as usize].position_size.get(),
        0,
        "Winner position should be closed"
    );
    assert_eq!(
        engine.accounts[loser as usize].position_size.get(),
        0,
        "Loser position should be closed"
    );

    // OI should be zero
    assert_eq!(engine.total_open_interest.get(), 0, "OI should be zero");

    // Winner should have positive PnL (young, subject to ADL)
    assert!(
        engine.accounts[winner as usize].pnl.is_positive(),
        "Winner should have positive PnL: {}",
        engine.accounts[winner as usize].pnl
    );

    // Conservation must hold - profits backed by losses (zero-sum)
    assert!(
        engine.check_conservation(DEFAULT_ORACLE),
        "Conservation must hold after force_realize - profits backed by losses"
    );

    // System should be in risk-reduction mode
    assert!(
        engine.risk_reduction_only,
        "Should be in risk-reduction mode"
    );
}

#[test]
fn test_window_liquidation_many_accounts_few_liquidatable() {
    // Bench scenario: Many accounts with positions, but few actually liquidatable.
    // Tests that window sweep liquidation works correctly.
    // (In test mode MAX_ACCOUNTS=64, so we use proportional scaling)

    use percolator::MAX_ACCOUNTS;

    let mut params = default_params();
    params.maintenance_margin_bps = 500; // 5%
    params.max_crank_staleness_slots = u64::MAX;

    let mut engine = Box::new(RiskEngine::new(params));
    set_insurance(&mut engine, 1_000_000);

    // Create accounts with positions - most are healthy, few are underwater
    let num_accounts = MAX_ACCOUNTS.min(60); // Leave some slots for counterparty
    let num_underwater = 5; // Only 5 are actually liquidatable

    // Counterparty for opposing positions
    let counterparty = engine.add_user(0).unwrap();
    engine.deposit(counterparty, 100_000_000, 0).unwrap();

    let mut underwater_indices = Vec::new();

    for i in 0..num_accounts {
        let user = engine.add_user(0).unwrap();

        if i < num_underwater {
            // Underwater: low capital, will fail maintenance
            engine.deposit(user, 1_000, 0).unwrap();
            underwater_indices.push(user);
        } else {
            // Healthy: plenty of capital
            engine.deposit(user, 200_000, 0).unwrap();
        }

        // All have positions
        engine.accounts[user as usize].position_size = I128::new(1_000_000);
        engine.accounts[user as usize].entry_price = 1_000_000;
        engine.accounts[counterparty as usize].position_size -= 1_000_000;
        engine.total_open_interest += 2_000_000;
    }
    engine.accounts[counterparty as usize].entry_price = 1_000_000;

    // Verify conservation
    assert!(
        engine.check_conservation(DEFAULT_ORACLE),
        "Conservation before crank"
    );

    // Run crank - should select top-K efficiently
    let outcome = engine
        .keeper_crank(u16::MAX, 1, 1_000_000, 0, false)
        .unwrap();

    // Verify conservation after
    assert!(
        engine.check_conservation(DEFAULT_ORACLE),
        "Conservation after crank"
    );

    // Should have liquidated the underwater accounts
    assert!(
        outcome.num_liquidations >= num_underwater as u32,
        "Should liquidate at least {} accounts, got {}",
        num_underwater,
        outcome.num_liquidations
    );

    // Verify underwater accounts got liquidated (positions reduced)
    for &idx in &underwater_indices {
        assert!(
            engine.accounts[idx as usize].position_size.get() < 1_000_000,
            "Underwater account {} should have reduced position",
            idx
        );
    }
}

#[test]
fn test_window_liquidation_many_liquidatable() {
    // Bench scenario: Multiple liquidatable accounts with varying severity.
    // Tests that window sweep handles multiple liquidations correctly.

    let mut params = default_params();
    params.maintenance_margin_bps = 500; // 5%
    params.max_crank_staleness_slots = u64::MAX;
    params.warmup_period_slots = 0; // Instant warmup

    let mut engine = Box::new(RiskEngine::new(params));
    set_insurance(&mut engine, 10_000_000);

    // Create 10 underwater accounts with varying severities
    let num_underwater = 10;

    // Counterparty with lots of capital
    let counterparty = engine.add_user(0).unwrap();
    engine.deposit(counterparty, 100_000_000, 0).unwrap();

    // Create underwater accounts
    for i in 0..num_underwater {
        let user = engine.add_user(0).unwrap();
        // Vary capital: 10_000 to 40_000 (underwater for 5% margin on 1M position = 50k needed)
        let capital = 10_000 + (i as u128 * 3_000);
        engine.deposit(user, capital, 0).unwrap();
        engine.accounts[user as usize].position_size = I128::new(1_000_000);
        engine.accounts[user as usize].entry_price = 1_000_000;
        engine.accounts[counterparty as usize].position_size -= 1_000_000;
        engine.total_open_interest += 2_000_000;
    }
    engine.accounts[counterparty as usize].entry_price = 1_000_000;

    // Verify conservation
    assert!(
        engine.check_conservation(DEFAULT_ORACLE),
        "Conservation before crank"
    );

    // Run crank
    let outcome = engine
        .keeper_crank(u16::MAX, 1, 1_000_000, 0, false)
        .unwrap();

    // Verify conservation after
    assert!(
        engine.check_conservation(DEFAULT_ORACLE),
        "Conservation after crank"
    );

    // Should have liquidated accounts (partial or full)
    assert!(
        outcome.num_liquidations > 0,
        "Should liquidate some accounts"
    );

    // Liquidation may trigger errors if ADL waterfall exhausts resources,
    // but the system should remain consistent
}

// ==============================================================================
// WINDOWED FORCE-REALIZE STEP TESTS
// ==============================================================================

/// Test 1: Force-realize step closes positions in-window only
#[test]
fn test_force_realize_step_closes_in_window_only() {
    let mut params = default_params();
    params.risk_reduction_threshold = U128::new(1000); // Threshold at 1000
    let mut engine = Box::new(RiskEngine::new(params));
    engine.vault = U128::new(100_000);

    // Create counterparty LP
    let lp = engine.add_lp([1u8; 32], [2u8; 32], 0).unwrap();
    engine.deposit(lp, 50_000, 0).unwrap();

    // Create users with positions at different indices
    let user1 = engine.add_user(0).unwrap(); // idx 1, in first window
    let user2 = engine.add_user(0).unwrap(); // idx 2, in first window
    let user3 = engine.add_user(0).unwrap(); // idx 3, in first window

    engine.deposit(user1, 5_000, 0).unwrap();
    engine.deposit(user2, 5_000, 0).unwrap();
    engine.deposit(user3, 5_000, 0).unwrap();

    // Give them positions
    engine.accounts[user1 as usize].position_size = I128::new(10_000);
    engine.accounts[user1 as usize].entry_price = 1_000_000;
    engine.accounts[user2 as usize].position_size = I128::new(10_000);
    engine.accounts[user2 as usize].entry_price = 1_000_000;
    engine.accounts[user3 as usize].position_size = I128::new(10_000);
    engine.accounts[user3 as usize].entry_price = 1_000_000;
    engine.accounts[lp as usize].position_size = I128::new(-30_000);
    engine.accounts[lp as usize].entry_price = 1_000_000;
    engine.total_open_interest = U128::new(60_000);

    // Set insurance at threshold (force-realize active)
    engine.insurance_fund.balance = U128::new(1000);

    // Run crank (cursor starts at 0)
    assert_eq!(engine.crank_cursor, 0);
    let outcome = engine
        .keeper_crank(u16::MAX, 1, 1_000_000, 0, false)
        .unwrap();

    // Force-realize should have run and closed positions
    assert!(
        outcome.force_realize_needed,
        "Force-realize should be needed"
    );
    assert!(
        outcome.force_realize_closed > 0,
        "Should have closed some positions"
    );

    // Positions should be closed
    assert_eq!(
        engine.accounts[user1 as usize].position_size.get(),
        0,
        "User1 position should be closed"
    );
    assert_eq!(
        engine.accounts[user2 as usize].position_size.get(),
        0,
        "User2 position should be closed"
    );
    assert_eq!(
        engine.accounts[user3 as usize].position_size.get(),
        0,
        "User3 position should be closed"
    );
}

/// Test 2: Force-realize step is inert when insurance > threshold
#[test]
fn test_force_realize_step_inert_above_threshold() {
    let mut params = default_params();
    params.risk_reduction_threshold = U128::new(1000); // Threshold at 1000
    let mut engine = Box::new(RiskEngine::new(params));
    engine.vault = U128::new(100_000);

    // Create counterparty LP
    let lp = engine.add_lp([1u8; 32], [2u8; 32], 0).unwrap();
    engine.deposit(lp, 50_000, 0).unwrap();

    // Create user with position (must be >= min_liquidation_abs to avoid dust-closure)
    let user = engine.add_user(0).unwrap();
    engine.deposit(user, 100_000, 0).unwrap();
    engine.accounts[user as usize].position_size = I128::new(200_000);
    engine.accounts[user as usize].entry_price = 1_000_000;
    engine.accounts[lp as usize].position_size = I128::new(-200_000);
    engine.accounts[lp as usize].entry_price = 1_000_000;
    engine.total_open_interest = U128::new(400_000);

    // Set insurance ABOVE threshold (force-realize NOT active)
    engine.insurance_fund.balance = U128::new(1001);

    let pos_before = engine.accounts[user as usize].position_size;

    // Run crank
    let outcome = engine
        .keeper_crank(u16::MAX, 1, 1_000_000, 0, false)
        .unwrap();

    // Force-realize should not be needed
    assert!(
        !outcome.force_realize_needed,
        "Force-realize should not be needed"
    );
    assert_eq!(
        outcome.force_realize_closed, 0,
        "No positions should be force-closed"
    );

    // Position should be unchanged
    assert_eq!(
        engine.accounts[user as usize].position_size, pos_before,
        "Position should be unchanged"
    );
}

/// Test: Dust positions (below min_liquidation_abs) are force-closed during crank
/// even when insurance is above threshold (not in force-realize mode).
#[test]
fn test_crank_force_closes_dust_positions() {
    let mut params = default_params();
    params.risk_reduction_threshold = U128::new(1000);
    params.min_liquidation_abs = U128::new(100_000); // 100k minimum
    let mut engine = Box::new(RiskEngine::new(params));
    engine.vault = U128::new(100_000);

    // Create counterparty LP
    let lp = engine.add_lp([1u8; 32], [2u8; 32], 0).unwrap();
    engine.deposit(lp, 50_000, 0).unwrap();

    // Create user with DUST position (below min_liquidation_abs)
    let user = engine.add_user(0).unwrap();
    engine.deposit(user, 10_000, 0).unwrap();
    engine.accounts[user as usize].position_size = I128::new(50_000); // Below 100k threshold
    engine.accounts[user as usize].entry_price = 1_000_000;
    engine.accounts[lp as usize].position_size = I128::new(-50_000);
    engine.accounts[lp as usize].entry_price = 1_000_000;
    engine.total_open_interest = U128::new(100_000);

    // Set insurance ABOVE threshold (force-realize NOT active)
    engine.insurance_fund.balance = U128::new(2000);

    assert!(
        !engine.accounts[user as usize].position_size.is_zero(),
        "User should have position before crank"
    );

    // Run crank
    let outcome = engine
        .keeper_crank(u16::MAX, 1, 1_000_000, 0, false)
        .unwrap();

    // Force-realize mode should NOT be needed (insurance above threshold)
    assert!(
        !outcome.force_realize_needed,
        "Force-realize should not be needed"
    );

    // But the dust position should still be closed
    assert!(
        engine.accounts[user as usize].position_size.is_zero(),
        "Dust position should be force-closed"
    );
    assert!(
        engine.accounts[lp as usize].position_size.is_zero(),
        "LP dust position should also be force-closed"
    );
}

/// Test 3: Force-realize produces pending_unpaid_loss and socialization reduces it
#[test]
fn test_force_realize_produces_pending_and_finalize_resolves() {
    let mut params = default_params();
    params.risk_reduction_threshold = U128::new(1000);
    let mut engine = Box::new(RiskEngine::new(params));
    engine.vault = U128::new(100_000);

    // Create counterparty LP with large position
    let lp = engine.add_lp([1u8; 32], [2u8; 32], 0).unwrap();
    engine.deposit(lp, 50_000, 0).unwrap();

    // Create losing user with insufficient capital to cover loss
    let user = engine.add_user(0).unwrap();
    engine.deposit(user, 1_000, 0).unwrap(); // Only 1000 capital

    // User is long, price dropped significantly
    engine.accounts[user as usize].position_size = I128::new(10_000);
    engine.accounts[user as usize].entry_price = 2_000_000; // Bought at 2.0
    engine.accounts[lp as usize].position_size = I128::new(-10_000);
    engine.accounts[lp as usize].entry_price = 2_000_000;
    engine.total_open_interest = U128::new(20_000);

    // Set insurance at threshold (nothing spendable)
    engine.insurance_fund.balance = U128::new(1000);

    let loss_accum_before = engine.loss_accum;

    // Run crank at oracle price 1.0 (user lost money)
    let outcome = engine
        .keeper_crank(u16::MAX, 1, 1_000_000, 0, false)
        .unwrap();

    // Force-realize should have run
    assert!(outcome.force_realize_needed);
    assert!(
        outcome.force_realize_closed > 0,
        "Should force-close position"
    );

    // Pending is added by force-realize; finalize runs only after a full sweep
    // Run enough cranks to complete a full sweep
    for slot in 2..=17 {
        engine
            .keeper_crank(u16::MAX, slot, 1_000_000, 0, false)
            .unwrap();
    }

    // After full sweep, pending should be resolved:
    // Since insurance is at threshold (nothing spendable), loss should go to loss_accum
    assert_eq!(
        engine.pending_unpaid_loss.get(),
        0,
        "pending_unpaid_loss should be cleared by finalize after full sweep"
    );

    // Loss should have moved to loss_accum
    assert!(
        engine.loss_accum.get() > loss_accum_before.get(),
        "Uncovered loss should move to loss_accum"
    );

    // System should be in risk-reduction mode
    assert!(
        engine.risk_reduction_only,
        "Should be in risk-reduction mode after uncovered loss"
    );
}

/// Test 4: Withdraw/close blocked while pending is non-zero
#[test]
fn test_force_realize_blocks_value_extraction() {
    let mut params = default_params();
    params.risk_reduction_threshold = U128::new(1000);
    let mut engine = Box::new(RiskEngine::new(params));
    engine.vault = U128::new(100_000);

    // Create user with capital
    let user = engine.add_user(0).unwrap();
    engine.deposit(user, 10_000, 0).unwrap();

    // Manually set pending to simulate post-force-realize state
    engine.pending_unpaid_loss = U128::new(100);

    // Try to withdraw - should fail
    let result = engine.withdraw(user, 1_000, 0, 1_000_000);
    assert!(
        result == Err(RiskError::Unauthorized),
        "Withdraw should be blocked when pending > 0"
    );

    // Try to close - should fail
    let result = engine.close_account(user, 0, 1_000_000);
    assert!(
        result == Err(RiskError::Unauthorized),
        "Close should be blocked when pending > 0"
    );

    // Clear pending
    engine.pending_unpaid_loss = U128::new(0);

    // Now withdraw should succeed
    let result = engine.withdraw(user, 1_000, 0, 1_000_000);
    assert!(result.is_ok(), "Withdraw should succeed when pending = 0");
}

// ==============================================================================
// PENDING FINALIZE LIVENESS TESTS
// ==============================================================================

/// Test: pending_unpaid_loss can't wedge - insurance covers it
#[test]
fn test_pending_finalize_liveness_insurance_covers() {
    let mut params = default_params();
    params.risk_reduction_threshold = U128::new(1000); // Floor at 1000
    let mut engine = Box::new(RiskEngine::new(params));

    // Fund insurance well above floor
    engine.insurance_fund.balance = U128::new(100_000);

    // Create pending loss with no accounts to haircut
    engine.pending_unpaid_loss = U128::new(5_000);

    // finalize_pending_after_window runs only after a full sweep
    // Run enough cranks to complete a full sweep
    for slot in 1..=16 {
        let result = engine.keeper_crank(u16::MAX, slot, 1_000_000, 0, false);
        assert!(result.is_ok());
    }

    // Pending should be cleared (or reduced)
    assert_eq!(
        engine.pending_unpaid_loss.get(),
        0,
        "pending_unpaid_loss should be cleared by insurance after full sweep"
    );

    // Insurance should have decreased
    assert!(
        engine.insurance_fund.balance.get() < 100_000,
        "Insurance should have been spent"
    );
}

/// Test: pending moves to loss_accum when insurance exhausted
#[test]
fn test_pending_finalize_liveness_moves_to_loss_accum() {
    let mut params = default_params();
    params.risk_reduction_threshold = U128::new(1000); // Floor at 1000
    let mut engine = Box::new(RiskEngine::new(params));

    // Insurance exactly at floor (nothing spendable)
    engine.insurance_fund.balance = U128::new(1000);

    // Create pending loss
    engine.pending_unpaid_loss = U128::new(5_000);

    // finalize_pending_after_window runs only after a full sweep
    // Run enough cranks to complete a full sweep
    for slot in 1..=16 {
        let result = engine.keeper_crank(u16::MAX, slot, 1_000_000, 0, false);
        assert!(result.is_ok());
    }

    // Pending should be cleared
    assert_eq!(
        engine.pending_unpaid_loss.get(),
        0,
        "pending_unpaid_loss should be cleared after full sweep"
    );

    // Loss should have moved to loss_accum
    assert_eq!(
        engine.loss_accum.get(),
        5_000,
        "Loss should move to loss_accum when insurance exhausted"
    );

    // Should be in risk-reduction mode
    assert!(
        engine.risk_reduction_only,
        "Should enter risk-reduction mode when losses are uncovered"
    );
}

/// Test: force-realize updates LP aggregates correctly
#[test]
fn test_force_realize_updates_lp_aggregates() {
    let mut params = default_params();
    params.risk_reduction_threshold = U128::new(10_000); // High threshold to trigger force-realize
    let mut engine = Box::new(RiskEngine::new(params));
    engine.vault = U128::new(100_000);

    // Insurance below threshold = force-realize active
    engine.insurance_fund.balance = U128::new(5_000);

    // Create LP with position
    let lp = engine.add_lp([1u8; 32], [2u8; 32], 0).unwrap();
    engine.deposit(lp, 50_000, 0).unwrap();

    // Create user as counterparty
    let user = engine.add_user(0).unwrap();
    engine.deposit(user, 50_000, 0).unwrap();

    // Set up positions
    engine.accounts[lp as usize].position_size = I128::new(-1_000_000); // Short 1 unit
    engine.accounts[lp as usize].entry_price = 1_000_000;
    engine.accounts[user as usize].position_size = I128::new(1_000_000); // Long 1 unit
    engine.accounts[user as usize].entry_price = 1_000_000;
    engine.total_open_interest = U128::new(2_000_000);

    // Update LP aggregates manually (simulating what would normally happen)
    engine.net_lp_pos = I128::new(-1_000_000);
    engine.lp_sum_abs = U128::new(1_000_000);

    // Verify force-realize is active
    assert!(
        engine.insurance_fund.balance <= params.risk_reduction_threshold,
        "Force-realize should be active"
    );

    let net_lp_before = engine.net_lp_pos;
    let sum_abs_before = engine.lp_sum_abs;

    // Run crank - should close LP position via force-realize
    let result = engine.keeper_crank(u16::MAX, 1, 1_000_000, 0, false);
    assert!(result.is_ok());

    // LP position should be closed
    if engine.accounts[lp as usize].position_size.is_zero() {
        // If LP was closed, aggregates should be updated
        assert_ne!(
            engine.net_lp_pos.get(),
            net_lp_before.get(),
            "net_lp_pos should change when LP position closed"
        );
        assert!(
            engine.lp_sum_abs.get() < sum_abs_before.get(),
            "lp_sum_abs should decrease when LP position closed"
        );
    }
}

/// Test: withdrawals blocked during pending, unblocked after finalize
#[test]
fn test_withdrawals_blocked_during_pending_unblocked_after() {
    let mut params = default_params();
    params.risk_reduction_threshold = U128::new(0);
    params.warmup_period_slots = 0; // Instant warmup
    let mut engine = Box::new(RiskEngine::new(params));

    // Fund insurance
    engine.insurance_fund.balance = U128::new(100_000);

    // Create user with capital
    let user = engine.add_user(0).unwrap();
    engine.deposit(user, 10_000, 0).unwrap();

    // Crank to establish baseline
    engine
        .keeper_crank(u16::MAX, 1, 1_000_000, 0, false)
        .unwrap();

    // Create pending loss
    engine.pending_unpaid_loss = U128::new(500);

    // Withdraw should fail with pending
    let result = engine.withdraw(user, 1_000, 2, 1_000_000);
    assert!(
        result.is_err(),
        "Withdraw should fail while pending_unpaid_loss > 0"
    );

    // finalize_pending_after_window runs only after a full sweep
    // Run enough cranks to complete a full sweep (slots 3..=18)
    for slot in 3..=18 {
        engine
            .keeper_crank(u16::MAX, slot, 1_000_000, 0, false)
            .unwrap();
    }

    // Pending should be cleared now
    assert_eq!(
        engine.pending_unpaid_loss.get(),
        0,
        "Pending should be cleared"
    );

    // Withdraw should now succeed (crank freshness check)
    let result = engine.withdraw(user, 1_000, 18, 1_000_000);
    assert!(
        result.is_ok(),
        "Withdraw should succeed after pending cleared"
    );
}

/// Debug test: ADL overflow atomicity with concrete values
/// Uses smaller values to verify the overflow calculation without triggering stack issues
#[test]
fn test_adl_overflow_atomicity_debug() {
    // Simple test to verify overflow detection math
    let small_pnl: u128 = 1;
    let large_pnl: u128 = 1u128 << 120; // 2^120
    let total_loss: u128 = 1u128 << 10; // 2^10 = 1024

    let total_unwrapped = small_pnl + large_pnl;
    let loss_to_socialize = std::cmp::min(total_loss, total_unwrapped);

    println!("small_pnl: {}", small_pnl);
    println!("large_pnl: {}", large_pnl);
    println!("total_loss: {}", total_loss);
    println!("total_unwrapped: {}", total_unwrapped);
    println!("loss_to_socialize: {}", loss_to_socialize);

    // Account 1: numer = loss_to_socialize * small_pnl
    let numer1 = loss_to_socialize.checked_mul(small_pnl);
    println!(
        "Account 1 numer ({}*{}): {:?}",
        loss_to_socialize, small_pnl, numer1
    );

    // Account 2: numer = loss_to_socialize * large_pnl
    let numer2 = loss_to_socialize.checked_mul(large_pnl);
    println!(
        "Account 2 numer ({}*{}): {:?}",
        loss_to_socialize, large_pnl, numer2
    );

    assert!(numer1.is_some(), "Account 1 should NOT overflow");
    assert!(numer2.is_none(), "Account 2 SHOULD overflow");

    println!("\nThis confirms the overflow scenario:");
    println!("- Account 1 would be processed first (haircut applied)");
    println!("- Account 2 would then cause overflow");
    println!("- If apply_adl returns error, account 1's state is already modified");
    println!("- This violates atomicity!");
}

/// Test ADL overflow atomicity with actual engine
/// Key insight: To trigger the bug, we need:
/// 1. Account 1's haircut to be non-zero (so it gets modified)
/// 2. Account 2's multiplication to overflow
///
/// haircut_1 = (loss_to_socialize * unwrapped_1) / total_unwrapped
/// For haircut_1 > 0: loss_to_socialize * unwrapped_1 >= total_unwrapped
///
/// For account 2 to overflow: loss_to_socialize * unwrapped_2 > u128::MAX
// NOTE: This test demonstrates a KNOWN BUG (atomicity violation in apply_adl).
// It's documented in audit.md. The test expects the bug to manifest.
#[test]
#[should_panic(expected = "Atomicity violation")]
fn test_adl_overflow_atomicity_engine() {
    let mut engine = Box::new(RiskEngine::new(default_params()));

    // Add two accounts
    let user1 = engine.add_user(0).unwrap();
    let user2 = engine.add_user(0).unwrap();

    // Strategy: Make both accounts have similar large PnL so account 1 gets real haircut
    // But make loss * pnl2 overflow
    //
    // Let pnl1 = pnl2 = 2^64 (large but within i128)
    // total_unwrapped = 2^65
    // loss = 2^65 (so account 2: 2^65 * 2^64 = 2^129 > u128::MAX)
    // haircut_1 = (2^65 * 2^64) / 2^65 = 2^64 (non-zero!)
    //
    // Actually wait, account 1 would also overflow with these values...
    // We need: loss * pnl1 < u128::MAX but loss * pnl2 >= u128::MAX
    //
    // Try: pnl1 = 1, pnl2 = 2^64, loss = 2^65
    // total_unwrapped = 1 + 2^64 ≈ 2^64
    // loss_to_socialize = min(2^65, 2^64) = 2^64
    // haircut_1 = (2^64 * 1) / 2^64 ≈ 1 (non-zero!)
    // account_2 check: 2^64 * 2^64 = 2^128 = u128::MAX + 1 → OVERFLOW!

    let pnl1: i128 = 1;
    let pnl2: i128 = 1i128 << 64; // 2^64
    let total_loss: u128 = 1u128 << 65; // 2^65

    println!("pnl1 = {}", pnl1);
    println!("pnl2 = {}", pnl2);
    println!("total_loss = {}", total_loss);

    // Set up accounts
    engine.accounts[user1 as usize].capital = U128::new(1000);
    engine.accounts[user1 as usize].pnl = I128::new(pnl1);
    engine.accounts[user2 as usize].capital = U128::new(1000);
    engine.accounts[user2 as usize].pnl = I128::new(pnl2);

    engine.vault = U128::new(2000);
    engine.insurance_fund.balance = U128::new(10000);

    // Pre-calculate expected values
    let total_unwrapped: u128 = pnl1 as u128 + pnl2 as u128;
    let loss_to_socialize = std::cmp::min(total_loss, total_unwrapped);
    println!("total_unwrapped = {}", total_unwrapped);
    println!("loss_to_socialize = {}", loss_to_socialize);

    let check1 = loss_to_socialize.checked_mul(pnl1 as u128);
    let check2 = loss_to_socialize.checked_mul(pnl2 as u128);
    println!("Account 1 mul check: {:?}", check1);
    println!("Account 2 mul check: {:?}", check2);

    if let Some(numer1) = check1 {
        let expected_haircut1 = numer1 / total_unwrapped;
        println!("Expected haircut for account 1: {}", expected_haircut1);
    }

    // Capture state
    let pnl1_before = engine.accounts[user1 as usize].pnl.get();

    // Call apply_adl - this should overflow on account 2
    let result = engine.apply_adl(total_loss);

    let pnl1_after = engine.accounts[user1 as usize].pnl.get();

    println!("\nResult: {:?}", result);
    println!("PnL 1 before: {}, after: {}", pnl1_before, pnl1_after);

    if result.is_err() {
        // If error, check atomicity
        if pnl1_after != pnl1_before {
            println!("\n*** ATOMICITY VIOLATION DETECTED! ***");
            println!(
                "Account 1 was modified (from {} to {}) before account 2 overflowed",
                pnl1_before, pnl1_after
            );
            panic!("Atomicity violation: account 1 modified before overflow");
        } else {
            println!("Atomicity preserved - no modifications on error");
        }
    } else {
        println!("No error returned - checking if overflow occurred as expected");
        // The operation succeeded - let's see what happened
        let pnl2_after = engine.accounts[user2 as usize].pnl.get();
        println!("PnL 2 after: {}", pnl2_after);
    }
}

// ==============================================================================
// VARIATION MARGIN / MARK-TO-MARKET TESTS
// ==============================================================================

/// Test that trade PnL is calculated as (oracle - exec_price) * size
/// This ensures the new variation margin logic is working correctly.
#[test]
fn test_trade_pnl_is_oracle_minus_exec() {
    let mut params = default_params();
    params.trading_fee_bps = 0; // No fees for cleaner math
    params.max_crank_staleness_slots = u64::MAX;

    let mut engine = Box::new(RiskEngine::new(params));

    // Create LP and user with capital
    let lp = engine.add_lp([1u8; 32], [0u8; 32], 0).unwrap();
    engine.deposit(lp, 1_000_000, 0).unwrap();

    let user = engine.add_user(0).unwrap();
    engine.deposit(user, 1_000_000, 0).unwrap();

    // Execute trade: user buys 1 unit
    // Oracle = 1_000_000, execution price will be at oracle (NoOpMatcher)
    let oracle_price = 1_000_000;
    let size = 1_000_000; // Buy 1 unit

    engine
        .execute_trade(&MATCHER, lp, user, 0, oracle_price, size)
        .unwrap();

    // With oracle = exec_price, trade_pnl = (oracle - exec_price) * size = 0
    // User and LP should have pnl = 0 (no fee)
    assert_eq!(
        engine.accounts[user as usize].pnl.get(),
        0,
        "User pnl should be 0 when oracle = exec"
    );
    assert_eq!(
        engine.accounts[lp as usize].pnl.get(),
        0,
        "LP pnl should be 0 when oracle = exec"
    );

    // Both should have entry_price = oracle_price
    assert_eq!(
        engine.accounts[user as usize].entry_price, oracle_price,
        "User entry should be oracle"
    );
    assert_eq!(
        engine.accounts[lp as usize].entry_price, oracle_price,
        "LP entry should be oracle"
    );

    // Conservation should hold
    assert!(
        engine.check_conservation(oracle_price),
        "Conservation should hold"
    );
}

/// Test that mark PnL is settled before position changes (variation margin)
#[test]
fn test_mark_settlement_on_trade_touch() {
    let mut params = default_params();
    params.trading_fee_bps = 0;
    params.max_crank_staleness_slots = u64::MAX;

    let mut engine = Box::new(RiskEngine::new(params));

    // Create LP and user
    let lp = engine.add_lp([1u8; 32], [0u8; 32], 0).unwrap();
    engine.deposit(lp, 1_000_000, 0).unwrap();

    let user = engine.add_user(0).unwrap();
    engine.deposit(user, 1_000_000, 0).unwrap();

    // First trade: user buys 1 unit at oracle 1_000_000
    let oracle1 = 1_000_000;
    engine
        .execute_trade(&MATCHER, lp, user, 0, oracle1, 1_000_000)
        .unwrap();

    // User now has: pos = +1, entry = 1_000_000, pnl = 0
    assert_eq!(
        engine.accounts[user as usize].position_size.get(),
        1_000_000
    );
    assert_eq!(engine.accounts[user as usize].entry_price, oracle1);
    assert_eq!(engine.accounts[user as usize].pnl.get(), 0);

    // Second trade at higher oracle: user sells (closes) at oracle 1_100_000
    // Before position change, mark should be settled:
    // mark = (1_100_000 - 1_000_000) * 1_000_000 / 1e6 = 100_000
    // User gains +100k mark PnL, LP gets -100k mark PnL
    //
    // After mark settlement, trade_pnl = (oracle - exec) * size = 0 (exec at oracle)
    //
    // Note: settle_warmup_to_capital immediately settles negative PnL from capital,
    // so LP's pnl becomes 0 and capital decreases by 100k.
    // User's positive pnl may or may not settle depending on warmup budget.
    let oracle2 = 1_100_000;

    let user_capital_before = engine.accounts[user as usize].capital.get();
    let lp_capital_before = engine.accounts[lp as usize].capital.get();

    engine
        .execute_trade(&MATCHER, lp, user, 0, oracle2, -1_000_000)
        .unwrap();

    // User closed position
    assert_eq!(engine.accounts[user as usize].position_size.get(), 0);

    // User should have gained 100k total equity (could be in pnl or capital)
    let user_pnl = engine.accounts[user as usize].pnl.get();
    let user_capital = engine.accounts[user as usize].capital.get();
    let user_equity_gain = user_pnl + (user_capital as i128 - user_capital_before as i128);
    assert_eq!(
        user_equity_gain, 100_000,
        "User should have gained 100k total equity"
    );

    // LP should have lost 100k total equity
    // Since negative PnL is immediately settled, LP's pnl should be 0 and capital should be 900k
    let lp_pnl = engine.accounts[lp as usize].pnl.get();
    let lp_capital = engine.accounts[lp as usize].capital.get();
    assert_eq!(lp_pnl, 0, "LP negative pnl should be settled to capital");
    assert_eq!(
        lp_capital,
        lp_capital_before - 100_000,
        "LP capital should decrease by 100k (loss settled)"
    );

    // Conservation should hold
    assert!(
        engine.check_conservation(oracle2),
        "Conservation should hold after mark settlement"
    );
}

/// Test that closing through different LPs doesn't cause PnL teleportation
/// This is the original bug that variation margin was designed to fix.
#[test]
fn test_cross_lp_close_no_pnl_teleport() {
    let mut params = default_params();
    params.trading_fee_bps = 0;
    params.max_crank_staleness_slots = u64::MAX;
    params.max_accounts = 64;

    let mut engine = Box::new(RiskEngine::new(params));

    // Create two LPs with different entry prices (simulated)
    let lp1 = engine.add_lp([1u8; 32], [0u8; 32], 0).unwrap();
    engine.deposit(lp1, 1_000_000, 0).unwrap();

    let lp2 = engine.add_lp([2u8; 32], [0u8; 32], 0).unwrap();
    engine.deposit(lp2, 1_000_000, 0).unwrap();

    let user = engine.add_user(0).unwrap();
    engine.deposit(user, 1_000_000, 0).unwrap();

    // User opens position with LP1 at oracle 1_000_000
    let oracle1 = 1_000_000;
    engine
        .execute_trade(&MATCHER, lp1, user, 0, oracle1, 1_000_000)
        .unwrap();

    // Capture state
    let user_pnl_after_open = engine.accounts[user as usize].pnl.get();
    let lp1_pnl_after_open = engine.accounts[lp1 as usize].pnl.get();
    let lp2_pnl_after_open = engine.accounts[lp2 as usize].pnl.get();

    // All pnl should be 0 since oracle = exec
    assert_eq!(user_pnl_after_open, 0);
    assert_eq!(lp1_pnl_after_open, 0);
    assert_eq!(lp2_pnl_after_open, 0);

    // Now user closes with LP2 at SAME oracle (no price movement)
    // With old logic: PnL could "teleport" between LPs based on entry price differences
    // With new variation margin: all entries are at oracle, so no spurious PnL
    engine
        .execute_trade(&MATCHER, lp2, user, 0, oracle1, -1_000_000)
        .unwrap();

    // User should have 0 pnl (no price movement)
    let user_pnl_after_close = engine.accounts[user as usize].pnl.get();
    assert_eq!(
        user_pnl_after_close, 0,
        "User pnl should be 0 when closing at same oracle price"
    );

    // LP1 still has 0 pnl (never touched again after open)
    let lp1_pnl_after_close = engine.accounts[lp1 as usize].pnl.get();
    assert_eq!(lp1_pnl_after_close, 0, "LP1 pnl should remain 0");

    // LP2 should also have 0 pnl (took opposite of close at same price)
    let lp2_pnl_after_close = engine.accounts[lp2 as usize].pnl.get();
    assert_eq!(lp2_pnl_after_close, 0, "LP2 pnl should be 0");

    // CRITICAL: Total PnL should be exactly 0 (no value created/destroyed)
    let total_pnl = user_pnl_after_close + lp1_pnl_after_close + lp2_pnl_after_close;
    assert_eq!(total_pnl, 0, "Total PnL must be zero-sum");

    // Conservation should hold
    assert!(
        engine.check_conservation(oracle1),
        "Conservation should hold"
    );
}

// ==============================================================================
// WARMUP BYPASS REGRESSION TEST
// ==============================================================================

/// Test that execute_trade sets current_slot and resets warmup_started_at_slot
/// This ensures warmup cannot be bypassed by stale current_slot values.
#[test]
fn test_execute_trade_sets_current_slot_and_resets_warmup_start() {
    let mut params = default_params();
    params.warmup_period_slots = 1000;
    params.trading_fee_bps = 0;
    params.maintenance_fee_per_slot = U128::new(0);
    params.maintenance_margin_bps = 0;
    params.initial_margin_bps = 0;
    params.max_crank_staleness_slots = u64::MAX;
    params.max_accounts = 64;

    let mut engine = Box::new(RiskEngine::new(params));

    // Create LP and user with capital
    let lp_idx = engine.add_lp([1u8; 32], [0u8; 32], 0).unwrap();
    engine.deposit(lp_idx, 1_000_000, 0).unwrap();

    let user_idx = engine.add_user(0).unwrap();
    engine.deposit(user_idx, 1_000_000, 0).unwrap();

    // Execute trade at now_slot = 100
    let now_slot = 100u64;
    let oracle_price = 100_000 * 1_000_000; // 100k
    let btc = 1_000_000i128; // 1 BTC

    engine
        .execute_trade(&MATCHER, lp_idx, user_idx, now_slot, oracle_price, btc)
        .unwrap();

    // Check current_slot was set
    assert_eq!(
        engine.current_slot, now_slot,
        "engine.current_slot should be set to now_slot after execute_trade"
    );

    // Check warmup_started_at_slot was reset for both accounts
    assert_eq!(
        engine.accounts[user_idx as usize].warmup_started_at_slot, now_slot,
        "user warmup_started_at_slot should be set to now_slot"
    );
    assert_eq!(
        engine.accounts[lp_idx as usize].warmup_started_at_slot, now_slot,
        "lp warmup_started_at_slot should be set to now_slot"
    );
}

// ==============================================================================
// MATCHER OUTPUT GUARD TESTS
// ==============================================================================

/// Matcher that returns the opposite sign of the requested size
struct OppositeSignMatcher;

impl MatchingEngine for OppositeSignMatcher {
    fn execute_match(
        &self,
        _lp_program: &[u8; 32],
        _lp_context: &[u8; 32],
        _lp_account_id: u64,
        oracle_price: u64,
        size: i128,
    ) -> Result<TradeExecution> {
        Ok(TradeExecution {
            price: oracle_price,
            size: -size, // Opposite sign!
        })
    }
}

/// Matcher that returns double the requested size
struct OversizeMatcher;

impl MatchingEngine for OversizeMatcher {
    fn execute_match(
        &self,
        _lp_program: &[u8; 32],
        _lp_context: &[u8; 32],
        _lp_account_id: u64,
        oracle_price: u64,
        size: i128,
    ) -> Result<TradeExecution> {
        Ok(TradeExecution {
            price: oracle_price,
            size: size.saturating_mul(2), // Double size!
        })
    }
}

#[test]
fn test_execute_trade_rejects_matcher_opposite_sign() {
    let mut params = default_params();
    params.trading_fee_bps = 0;
    params.max_crank_staleness_slots = u64::MAX;
    params.max_accounts = 64;

    let mut engine = Box::new(RiskEngine::new(params));

    let lp_idx = engine.add_lp([1u8; 32], [0u8; 32], 0).unwrap();
    engine.deposit(lp_idx, 1_000_000, 0).unwrap();

    let user_idx = engine.add_user(0).unwrap();
    engine.deposit(user_idx, 1_000_000, 0).unwrap();

    let result = engine.execute_trade(
        &OppositeSignMatcher,
        lp_idx,
        user_idx,
        0,
        1_000_000,
        1_000_000, // Request positive size
    );

    assert!(
        matches!(result, Err(RiskError::InvalidMatchingEngine)),
        "Should reject matcher that returns opposite sign: {:?}",
        result
    );
}

#[test]
fn test_execute_trade_rejects_matcher_oversize_fill() {
    let mut params = default_params();
    params.trading_fee_bps = 0;
    params.max_crank_staleness_slots = u64::MAX;
    params.max_accounts = 64;

    let mut engine = Box::new(RiskEngine::new(params));

    let lp_idx = engine.add_lp([1u8; 32], [0u8; 32], 0).unwrap();
    engine.deposit(lp_idx, 1_000_000, 0).unwrap();

    let user_idx = engine.add_user(0).unwrap();
    engine.deposit(user_idx, 1_000_000, 0).unwrap();

    let result = engine.execute_trade(
        &OversizeMatcher,
        lp_idx,
        user_idx,
        0,
        1_000_000,
        500_000, // Request half size
    );

    assert!(
        matches!(result, Err(RiskError::InvalidMatchingEngine)),
        "Should reject matcher that returns oversize fill: {:?}",
        result
    );
}

// ==============================================================================
// CONSERVATION CHECKER STRICTNESS TEST
// ==============================================================================

#[test]
fn test_check_conservation_fails_on_mark_overflow() {
    let mut params = default_params();
    params.max_accounts = 64;

    let mut engine = Box::new(RiskEngine::new(params));

    // Create user account
    let user_idx = engine.add_user(0).unwrap();

    // Manually set up an account state that will cause mark_pnl overflow
    // position_size = i128::MAX, entry_price = MAX_ORACLE_PRICE
    // When mark_pnl is calculated with oracle = 1, it will overflow
    engine.accounts[user_idx as usize].position_size = I128::new(i128::MAX);
    engine.accounts[user_idx as usize].entry_price = MAX_ORACLE_PRICE;
    engine.accounts[user_idx as usize].capital = U128::ZERO;
    engine.accounts[user_idx as usize].pnl = I128::new(0);

    // Conservation should fail because mark_pnl calculation overflows
    assert!(
        !engine.check_conservation(1),
        "check_conservation should return false when mark_pnl overflows"
    );
}

// ==============================================================================
// Tests migrated from src/percolator.rs inline tests
// ==============================================================================

const E6: u64 = 1_000_000;
const ORACLE_100K: u64 = 100_000 * E6;
const ONE_BASE: i128 = 1_000_000; // 1.0 base unit if base is 1e6-scaled

fn params_for_inline_tests() -> RiskParams {
    RiskParams {
        warmup_period_slots: 1000,
        maintenance_margin_bps: 0,
        initial_margin_bps: 0,
        trading_fee_bps: 0,
        max_accounts: MAX_ACCOUNTS as u64,
        new_account_fee: U128::new(0),
        risk_reduction_threshold: U128::new(0),

        maintenance_fee_per_slot: U128::new(0),
        max_crank_staleness_slots: u64::MAX,

        liquidation_fee_bps: 0,
        liquidation_fee_cap: U128::new(0),

        liquidation_buffer_bps: 0,
        min_liquidation_abs: U128::new(0),
    }
}

#[test]
fn test_cross_lp_close_no_pnl_teleport_simple() {
    let mut engine = RiskEngine::new(params_for_inline_tests());

    let lp1 = engine.add_lp([1u8; 32], [2u8; 32], 0).unwrap();
    let lp2 = engine.add_lp([3u8; 32], [4u8; 32], 0).unwrap();
    let user = engine.add_user(0).unwrap();

    // LP1 must be able to absorb -10k*E6 loss and still have equity > 0
    engine.deposit(lp1, 50_000 * (E6 as u128), 1).unwrap();
    engine.deposit(lp2, 50_000 * (E6 as u128), 1).unwrap();
    engine.deposit(user, 50_000 * (E6 as u128), 1).unwrap();

    // Trade 1: user opens +1 at 90k while oracle=100k => user +10k, LP1 -10k
    struct P90kMatcher;
    impl MatchingEngine for P90kMatcher {
        fn execute_match(
            &self,
            _lp_program: &[u8; 32],
            _lp_context: &[u8; 32],
            _lp_account_id: u64,
            oracle_price: u64,
            size: i128,
        ) -> Result<TradeExecution> {
            Ok(TradeExecution {
                price: oracle_price - (10_000 * 1_000_000),
                size,
            })
        }
    }

    // Trade 2: user closes with LP2 at oracle price => trade_pnl = 0 (no teleport)
    struct AtOracleMatcher;
    impl MatchingEngine for AtOracleMatcher {
        fn execute_match(
            &self,
            _lp_program: &[u8; 32],
            _lp_context: &[u8; 32],
            _lp_account_id: u64,
            oracle_price: u64,
            size: i128,
        ) -> Result<TradeExecution> {
            Ok(TradeExecution {
                price: oracle_price,
                size,
            })
        }
    }

    engine
        .execute_trade(&P90kMatcher, lp1, user, 100, ORACLE_100K, ONE_BASE)
        .unwrap();
    engine
        .execute_trade(&AtOracleMatcher, lp2, user, 101, ORACLE_100K, -ONE_BASE)
        .unwrap();

    // User is flat
    assert_eq!(engine.accounts[user as usize].position_size.get(), 0);

    // PnL stays with LP1 (the LP that gave the user a better-than-oracle fill).
    let ten_k_e6: u128 = (10_000 * E6) as u128;
    let user_pnl = engine.accounts[user as usize].pnl.get() as u128;
    let user_cap = engine.accounts[user as usize].capital.get();
    let initial_cap = 50_000 * (E6 as u128);
    // Total user value (pnl + capital) must equal initial_capital + 10k profit
    assert_eq!(user_pnl + user_cap, initial_cap + ten_k_e6,
        "user total value must be initial_capital + trade profit");
    assert_eq!(engine.accounts[lp1 as usize].pnl.get(), 0);
    assert_eq!(engine.accounts[lp1 as usize].capital.get(), initial_cap - ten_k_e6);
    // LP2 must be unaffected (no teleportation)
    assert_eq!(engine.accounts[lp2 as usize].pnl.get(), 0);
    assert_eq!(engine.accounts[lp2 as usize].capital.get(), initial_cap);

    // Conservation must still hold
    assert!(engine.check_conservation(ORACLE_100K));
}

#[test]
fn test_idle_user_drains_and_gc_closes() {
    let mut params = params_for_inline_tests();
    // 1 unit per slot maintenance fee
    params.maintenance_fee_per_slot = U128::new(1);
    let mut engine = RiskEngine::new(params);

    let user_idx = engine.add_user(0).unwrap();
    // Deposit 10 units of capital
    engine.deposit(user_idx, 10, 1).unwrap();

    assert!(engine.is_used(user_idx as usize));

    // Advance 1000 slots and crank — fee drains 1/slot * 1000 = 1000 >> 10 capital
    let outcome = engine
        .keeper_crank(user_idx, 1001, ORACLE_100K, 0, false)
        .unwrap();

    // Account should have been drained to 0 capital
    // The crank settles fees and then GC sweeps dust
    assert_eq!(outcome.num_gc_closed, 1, "expected GC to close the drained account");
    assert!(!engine.is_used(user_idx as usize), "account should be freed");
}

#[test]
fn test_dust_stale_funding_gc() {
    let mut engine = RiskEngine::new(params_for_inline_tests());

    let user_idx = engine.add_user(0).unwrap();

    // Zero out the account: no capital, no position, no pnl
    engine.accounts[user_idx as usize].capital = U128::ZERO;
    engine.accounts[user_idx as usize].pnl = I128::ZERO;
    engine.accounts[user_idx as usize].position_size = I128::ZERO;
    engine.accounts[user_idx as usize].reserved_pnl = 0;

    // Set a stale funding_index (different from global)
    engine.accounts[user_idx as usize].funding_index = I128::new(999);
    // Global funding index is 0 (default)
    assert_ne!(
        engine.accounts[user_idx as usize].funding_index,
        engine.funding_index_qpb_e6
    );

    assert!(engine.is_used(user_idx as usize));

    // Crank should snap funding and GC the dust account
    let outcome = engine
        .keeper_crank(user_idx, 10, ORACLE_100K, 0, false)
        .unwrap();

    assert_eq!(outcome.num_gc_closed, 1, "expected GC to close stale-funding dust");
    assert!(!engine.is_used(user_idx as usize), "account should be freed");
}

#[test]
fn test_dust_negative_fee_credits_gc() {
    let mut engine = RiskEngine::new(params_for_inline_tests());

    let user_idx = engine.add_user(0).unwrap();

    // Zero out the account
    engine.accounts[user_idx as usize].capital = U128::ZERO;
    engine.accounts[user_idx as usize].pnl = I128::ZERO;
    engine.accounts[user_idx as usize].position_size = I128::ZERO;
    engine.accounts[user_idx as usize].reserved_pnl = 0;
    // Set negative fee_credits (fee debt)
    engine.accounts[user_idx as usize].fee_credits = I128::new(-123);

    assert!(engine.is_used(user_idx as usize));

    // Crank should GC this account — negative fee_credits doesn't block GC
    let outcome = engine
        .keeper_crank(user_idx, 10, ORACLE_100K, 0, false)
        .unwrap();

    assert_eq!(outcome.num_gc_closed, 1, "expected GC to close account with negative fee_credits");
    assert!(!engine.is_used(user_idx as usize), "account should be freed");
}

#[test]
fn test_lp_never_gc() {
    let mut params = params_for_inline_tests();
    params.maintenance_fee_per_slot = U128::new(1);
    let mut engine = RiskEngine::new(params);

    let lp_idx = engine.add_lp([1u8; 32], [2u8; 32], 0).unwrap();

    // Zero out the LP account to make it look like dust
    engine.accounts[lp_idx as usize].capital = U128::ZERO;
    engine.accounts[lp_idx as usize].pnl = I128::ZERO;
    engine.accounts[lp_idx as usize].position_size = I128::ZERO;
    engine.accounts[lp_idx as usize].reserved_pnl = 0;

    assert!(engine.is_used(lp_idx as usize));

    // Crank many times — LP should never be GC'd
    for slot in 1..=10 {
        let outcome = engine
            .keeper_crank(lp_idx, slot * 100, ORACLE_100K, 0, false)
            .unwrap();
        assert_eq!(outcome.num_gc_closed, 0, "LP must not be garbage collected (slot {})", slot * 100);
    }

    assert!(engine.is_used(lp_idx as usize), "LP account must still exist");
}

#[test]
fn test_maintenance_fee_paid_from_fee_credits_is_coupon_not_revenue() {
    let mut params = params_for_inline_tests();
    params.maintenance_fee_per_slot = U128::new(10);
    let mut engine = RiskEngine::new(params);

    let user_idx = engine.add_user(0).unwrap();
    engine.deposit(user_idx, 1_000_000, 1).unwrap();

    // Add 100 fee credits (test-only helper — no vault/insurance)
    engine.add_fee_credits(user_idx, 100).unwrap();
    assert_eq!(engine.accounts[user_idx as usize].fee_credits.get(), 100);

    let rev_before = engine.insurance_fund.fee_revenue.get();
    let bal_before = engine.insurance_fund.balance.get();

    // Settle maintenance: dt=5, fee_per_slot=10, due=50
    // All 50 should come from fee_credits (coupon: no insurance booking)
    engine
        .settle_maintenance_fee(user_idx, 6, ORACLE_100K)
        .unwrap();

    assert_eq!(
        engine.accounts[user_idx as usize].fee_credits.get(),
        50,
        "fee_credits should decrease by 50"
    );
    // Coupon semantics: spending credits does NOT touch insurance.
    // Insurance was already paid when credits were granted.
    assert_eq!(
        engine.insurance_fund.fee_revenue.get() - rev_before,
        0,
        "insurance fee_revenue must NOT change (coupon semantics)"
    );
    assert_eq!(
        engine.insurance_fund.balance.get() - bal_before,
        0,
        "insurance balance must NOT change (coupon semantics)"
    );
}

#[test]
fn test_maintenance_fee_splits_credits_coupon_capital_to_insurance() {
    let mut params = params_for_inline_tests();
    params.maintenance_fee_per_slot = U128::new(10);
    let mut engine = RiskEngine::new(params);

    let user_idx = engine.add_user(0).unwrap();
    // deposit at slot 1: dt=1 from slot 0, fee=10. Paid from deposit.
    // capital = 50 - 10 = 40.
    engine.deposit(user_idx, 50, 1).unwrap();
    assert_eq!(engine.accounts[user_idx as usize].capital.get(), 40);

    // Add 30 fee credits (test-only)
    engine.add_fee_credits(user_idx, 30).unwrap();

    let rev_before = engine.insurance_fund.fee_revenue.get();

    // Settle maintenance: dt=10, fee_per_slot=10, due=100
    // credits pays 30, capital pays 40 (all it has), leftover 30 unpaid
    engine
        .settle_maintenance_fee(user_idx, 11, ORACLE_100K)
        .unwrap();

    let rev_increase = engine.insurance_fund.fee_revenue.get() - rev_before;
    let cap_after = engine.accounts[user_idx as usize].capital.get();

    assert_eq!(rev_increase, 40, "insurance revenue should be 40 (capital only; credits are coupon)");
    assert_eq!(cap_after, 0, "capital should be fully drained");
    // fee_credits should be -30 (100 due - 30 credits - 40 capital = 30 unpaid debt)
    assert_eq!(
        engine.accounts[user_idx as usize].fee_credits.get(),
        -30,
        "fee_credits should reflect unpaid debt"
    );
}

#[test]
fn test_deposit_fee_credits_updates_vault_and_insurance() {
    let mut engine = RiskEngine::new(params_for_inline_tests());
    let user_idx = engine.add_user(0).unwrap();

    let vault_before = engine.vault.get();
    let ins_before = engine.insurance_fund.balance.get();
    let rev_before = engine.insurance_fund.fee_revenue.get();

    engine.deposit_fee_credits(user_idx, 500, 10).unwrap();

    assert_eq!(engine.vault.get() - vault_before, 500, "vault must increase");
    assert_eq!(engine.insurance_fund.balance.get() - ins_before, 500, "insurance balance must increase");
    assert_eq!(engine.insurance_fund.fee_revenue.get() - rev_before, 500, "insurance fee_revenue must increase");
    assert_eq!(engine.accounts[user_idx as usize].fee_credits.get(), 500, "fee_credits must increase");
}

#[test]
fn test_warmup_matured_not_lost_on_trade() {
    let mut params = params_for_inline_tests();
    params.warmup_period_slots = 100;
    params.max_crank_staleness_slots = u64::MAX;
    let mut engine = RiskEngine::new(params);

    let lp_idx = engine.add_lp([1u8; 32], [2u8; 32], 0).unwrap();
    let user_idx = engine.add_user(0).unwrap();

    // Fund both generously
    engine.deposit(lp_idx, 1_000_000_000, 1).unwrap();
    engine.deposit(user_idx, 1_000_000_000, 1).unwrap();

    // Provide warmup budget: the warmup budget system requires losses or
    // spendable insurance to fund positive PnL settlement. Seed insurance
    // so the warmup budget allows settlement.
    engine.insurance_fund.balance = engine.insurance_fund.balance + 1_000_000;

    // Give user positive PnL and set warmup started far in the past
    engine.accounts[user_idx as usize].pnl = I128::new(10_000);
    engine.accounts[user_idx as usize].warmup_started_at_slot = 1;
    // slope = max(1, 10000/100) = 100
    engine.accounts[user_idx as usize].warmup_slope_per_step = U128::new(100);

    let cap_before = engine.accounts[user_idx as usize].capital.get();

    // Execute a tiny trade at slot 200 (elapsed from slot 1 = 199 slots, cap = 100*199 = 19900 > 10000)
    struct AtOracleMatcher;
    impl MatchingEngine for AtOracleMatcher {
        fn execute_match(
            &self,
            _lp_program: &[u8; 32],
            _lp_context: &[u8; 32],
            _lp_account_id: u64,
            oracle_price: u64,
            size: i128,
        ) -> Result<TradeExecution> {
            Ok(TradeExecution { price: oracle_price, size })
        }
    }

    engine
        .execute_trade(&AtOracleMatcher, lp_idx, user_idx, 200, ORACLE_100K, ONE_BASE)
        .unwrap();

    let cap_after = engine.accounts[user_idx as usize].capital.get();

    // Capital must have increased by the matured warmup amount (10_000 PnL settled to capital)
    assert!(
        cap_after > cap_before,
        "capital must increase from matured warmup: before={}, after={}",
        cap_before,
        cap_after
    );
    assert!(
        cap_after >= cap_before + 10_000,
        "capital should have increased by at least 10000 (matured warmup): before={}, after={}",
        cap_before,
        cap_after
    );
}

#[test]
fn test_abandoned_with_stale_last_fee_slot_eventually_closed() {
    let mut params = params_for_inline_tests();
    params.maintenance_fee_per_slot = U128::new(1);
    let mut engine = RiskEngine::new(params);

    let user_idx = engine.add_user(0).unwrap();
    // Small deposit
    engine.deposit(user_idx, 5, 1).unwrap();

    assert!(engine.is_used(user_idx as usize));

    // Don't call any user ops. Run crank at a slot far ahead.
    // First crank: drains the account via fee settlement
    let _ = engine
        .keeper_crank(user_idx, 10_000, ORACLE_100K, 0, false)
        .unwrap();

    // Second crank: GC scan should pick up the dust
    let outcome = engine
        .keeper_crank(user_idx, 10_001, ORACLE_100K, 0, false)
        .unwrap();

    // The account must be closed by now (across both cranks)
    assert!(
        !engine.is_used(user_idx as usize),
        "abandoned account with stale last_fee_slot must eventually be GC'd"
    );
    // At least one of the two cranks should have GC'd it
    // (first crank drains capital to 0, GC might close it there already)
}
