//! Section 8 — External audit fix proofs
//!
//! Formal verification of fixes for confirmed external audit findings:
//! 1. attach_effective_position epoch_snap canonical zero (spec §2.4)
//! 2. add_user/add_lp materialized_account_count rollback on alloc_slot failure
//! 3. is_above_maintenance_margin / is_above_initial_margin eff==0 special case (spec §9.1)
//! 4. fee_debt_sweep checked_add (defensive, invariant-guaranteed safe)

#![cfg(kani)]

mod common;
use common::*;

// ############################################################################
// FIX 1: epoch_snap canonical zero on position zero-out (spec §2.4)
// ############################################################################

/// After attach_effective_position(idx, 0), epoch_snap MUST be 0 regardless
/// of prior position side. Spec §2.4: canonical zero-position defaults.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_epoch_snap_zero_on_position_zeroout() {
    let mut engine = RiskEngine::new(zero_fee_params());

    let idx = engine.add_user(0).unwrap() as usize;
    engine.deposit(idx as u16, 1_000_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    // Set up non-trivial ADL epoch state
    engine.adl_epoch_long = 5;
    engine.adl_epoch_short = 7;

    // Symbolic initial side: positive (long) or negative (short) basis
    let side_long: bool = kani::any();
    let basis: u32 = kani::any();
    kani::assume(basis >= 1 && basis <= 10 * POS_SCALE as u32);

    let signed_basis = if side_long { basis as i128 } else { -(basis as i128) };

    // Use set_position_basis_q to correctly track stored_pos_count.
    // Set epoch mismatch to skip the phantom dust U256 path
    // (irrelevant to the epoch_snap fix).
    engine.set_position_basis_q(idx, signed_basis);
    engine.accounts[idx].adl_a_basis = ADL_ONE;
    engine.accounts[idx].adl_k_snap = 0;
    // Epoch mismatch: snap=0 != epoch_long=5 / epoch_short=7
    engine.accounts[idx].adl_epoch_snap = 0;

    // Zero out the position
    engine.attach_effective_position(idx, 0);

    // Spec §2.4: all canonical zero-position defaults
    assert!(engine.accounts[idx].position_basis_q == 0, "basis must be zero");
    assert!(engine.accounts[idx].adl_a_basis == ADL_ONE, "a_basis must be ADL_ONE");
    assert!(engine.accounts[idx].adl_k_snap == 0, "k_snap must be zero");
    assert!(engine.accounts[idx].adl_epoch_snap == 0, "epoch_snap must be zero per §2.4");
}

/// Verify that attaching a nonzero position correctly picks up the
/// current side epoch (not zero).
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_epoch_snap_correct_on_nonzero_attach() {
    let mut engine = RiskEngine::new(zero_fee_params());

    let idx = engine.add_user(0).unwrap() as usize;
    engine.deposit(idx as u16, 1_000_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    engine.adl_epoch_long = 3;
    engine.adl_epoch_short = 9;

    let side_long: bool = kani::any();
    let basis: u32 = kani::any();
    kani::assume(basis >= 1 && basis <= 100 * POS_SCALE as u32);

    let new_eff = if side_long { basis as i128 } else { -(basis as i128) };

    engine.attach_effective_position(idx, new_eff);

    if side_long {
        assert!(engine.accounts[idx].adl_epoch_snap == engine.adl_epoch_long);
        assert!(engine.accounts[idx].adl_a_basis == engine.adl_mult_long);
        assert!(engine.accounts[idx].adl_k_snap == engine.adl_coeff_long);
    } else {
        assert!(engine.accounts[idx].adl_epoch_snap == engine.adl_epoch_short);
        assert!(engine.accounts[idx].adl_a_basis == engine.adl_mult_short);
        assert!(engine.accounts[idx].adl_k_snap == engine.adl_coeff_short);
    }
}

// ############################################################################
// FIX 2: materialized_account_count rollback on alloc_slot failure
// ############################################################################

/// If alloc_slot fails in add_user, materialized_account_count must be
/// rolled back to its pre-call value.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_add_user_count_rollback_on_alloc_failure() {
    let mut engine = RiskEngine::new(zero_fee_params());

    // Fill all slots so alloc_slot will fail
    for i in 0..MAX_ACCOUNTS {
        engine.accounts[i].account_id = 1; // mark as used
    }
    engine.num_used_accounts = MAX_ACCOUNTS as u16;
    engine.materialized_account_count = 0; // but count is low (simulating inconsistency path)

    let count_before = engine.materialized_account_count;

    let result = engine.add_user(0);
    assert!(result.is_err(), "add_user must fail when all slots are full");
    assert!(
        engine.materialized_account_count == count_before,
        "materialized_account_count must be rolled back on failure"
    );
}

/// If alloc_slot fails in add_lp, materialized_account_count must be
/// rolled back to its pre-call value.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_add_lp_count_rollback_on_alloc_failure() {
    let mut engine = RiskEngine::new(zero_fee_params());

    // Fill all slots so alloc_slot will fail
    for i in 0..MAX_ACCOUNTS {
        engine.accounts[i].account_id = 1;
    }
    engine.num_used_accounts = MAX_ACCOUNTS as u16;
    engine.materialized_account_count = 0;

    let count_before = engine.materialized_account_count;

    let result = engine.add_lp([0; 32], [0; 32], 0);
    assert!(result.is_err(), "add_lp must fail when all slots are full");
    assert!(
        engine.materialized_account_count == count_before,
        "materialized_account_count must be rolled back on failure"
    );
}

// ############################################################################
// FIX 3: margin requirement is zero when effective position is zero (§9.1)
// ############################################################################

/// A flat account (eff==0) with any nonnegative equity must be maintenance-healthy.
/// Before the fix, min_nonzero_mm_req created a false requirement for flat accounts.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_flat_account_maintenance_healthy() {
    let mut engine = RiskEngine::new(zero_fee_params());

    let idx = engine.add_user(0).unwrap();
    let capital: u32 = kani::any();
    kani::assume(capital >= 1 && capital <= 10_000_000);

    engine.deposit(idx, capital as u128, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    // Account is flat (no position)
    assert!(engine.effective_pos_q(idx as usize) == 0);

    // With any positive capital and no position, account MUST be maintenance-healthy
    // Spec §9.1: MM_req = 0 when eff == 0
    let healthy = engine.is_above_maintenance_margin(
        &engine.accounts[idx as usize].clone(),
        idx as usize,
        DEFAULT_ORACLE,
    );
    assert!(healthy, "flat account with positive capital must be maintenance-healthy");
}

/// A flat account (eff==0) with any nonnegative equity must be initial-margin healthy.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_flat_account_initial_margin_healthy() {
    let mut engine = RiskEngine::new(zero_fee_params());

    let idx = engine.add_user(0).unwrap();
    let capital: u32 = kani::any();
    kani::assume(capital >= 1 && capital <= 10_000_000);

    engine.deposit(idx, capital as u128, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    assert!(engine.effective_pos_q(idx as usize) == 0);

    let healthy = engine.is_above_initial_margin(
        &engine.accounts[idx as usize].clone(),
        idx as usize,
        DEFAULT_ORACLE,
    );
    assert!(healthy, "flat account with positive capital must be initial-margin healthy");
}

/// A flat account with zero equity must NOT be maintenance-healthy.
/// Spec §9.1: Eq_net > 0 (since MM_req = 0 for flat), so Eq_net = 0 fails.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_flat_zero_equity_not_maintenance_healthy() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let idx = engine.add_user(0).unwrap();
    // No deposit, capital = 0, pnl = 0 → equity = 0

    assert!(engine.effective_pos_q(idx as usize) == 0);

    let healthy = engine.is_above_maintenance_margin(
        &engine.accounts[idx as usize].clone(),
        idx as usize,
        DEFAULT_ORACLE,
    );
    // Eq_net = 0, MM_req = 0, 0 > 0 is false → not healthy
    assert!(!healthy, "flat account with zero equity is NOT maintenance-healthy");
}

// ############################################################################
// FIX 4: fee_debt_sweep uses checked_add (invariant: pay <= |fee_credits|)
// ############################################################################

/// fee_debt_sweep: after sweep, fee_credits is closer to zero and
/// insurance fund increases by exactly pay. Symbolic capital and debt.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_fee_debt_sweep_checked_arithmetic() {
    let mut engine = RiskEngine::new(zero_fee_params());

    let idx = engine.add_user(0).unwrap() as usize;
    let capital: u32 = kani::any();
    let debt: u32 = kani::any();
    kani::assume(capital >= 1 && capital <= 10_000_000);
    kani::assume(debt >= 1 && debt <= 10_000_000);

    // Set up capital
    engine.deposit(idx as u16, capital as u128, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    // Set fee debt (negative fee_credits)
    engine.accounts[idx].fee_credits = I128::new(-(debt as i128));

    let cap_before = engine.accounts[idx].capital.get();
    let fc_before = engine.accounts[idx].fee_credits.get();
    let ins_before = engine.insurance_fund.balance.get();

    engine.fee_debt_sweep(idx);

    let cap_after = engine.accounts[idx].capital.get();
    let fc_after = engine.accounts[idx].fee_credits.get();
    let ins_after = engine.insurance_fund.balance.get();

    let pay = core::cmp::min(debt as u128, capital as u128);

    // Capital decreases by pay
    assert!(cap_after == cap_before - pay);
    // fee_credits increases by pay (moves toward zero)
    assert!(fc_after == fc_before + pay as i128);
    // Insurance increases by pay
    assert!(ins_after == ins_before + pay);
    // fee_credits is still <= 0
    assert!(fc_after <= 0);
    // Conservation: total capital moved from account to insurance
    assert!(engine.check_conservation());
}

// ############################################################################
// FIX 5: keeper_crank_not_atomic pre-flight validates partial hints (no griefing)
// ############################################################################

/// keeper_crank_not_atomic with a bad partial hint (too small to restore health) must NOT
/// Invalid partial hint → no liquidation action (spec §11.1 rule 3).
/// The crank succeeds but the account retains its position.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_keeper_crank_invalid_partial_no_action() {
    let mut engine = RiskEngine::new(default_params());

    let a = engine.add_user(1000).unwrap();
    let b = engine.add_user(1000).unwrap();

    engine.deposit(a, 50_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.deposit(b, 50_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    let size = 100 * POS_SCALE as i128;
    engine.execute_trade_not_atomic(a, b, DEFAULT_ORACLE, DEFAULT_SLOT, size, DEFAULT_ORACLE, 0i64).unwrap();

    let crash_oracle = 500u64;

    // Tiny partial — won't restore health, pre-flight returns None → no action
    let bad_hint = Some(LiquidationPolicy::ExactPartial(POS_SCALE as u128));
    let candidates = [(a, bad_hint)];
    let result = engine.keeper_crank_not_atomic(DEFAULT_SLOT + 1, crash_oracle, &candidates, 10, 0i64);
    assert!(result.is_ok(), "keeper_crank_not_atomic must not revert on invalid partial hint");

    // Invalid hint means no liquidation — account still has position
    assert!(engine.effective_pos_q(a as usize) != 0,
        "invalid partial hint must cause no liquidation action");
    assert!(engine.check_conservation());
}

// ############################################################################
// FIX 6: liquidate_at_oracle_not_atomic rejects missing accounts before touch
// ############################################################################

/// liquidate_at_oracle_not_atomic on a missing account must return Ok(false) without
/// mutating market state (no accrue_market_to side effects).
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_liquidate_missing_account_no_market_mutation() {
    let mut engine = RiskEngine::new(zero_fee_params());

    let slot_before = engine.current_slot;
    let oracle_before = engine.last_oracle_price;

    // Call liquidate on an unused slot
    let result = engine.liquidate_at_oracle_not_atomic(0, DEFAULT_SLOT, DEFAULT_ORACLE, LiquidationPolicy::FullClose, 0i64);
    assert!(matches!(result, Ok(false)), "must return Ok(false) for missing account");

    // Market state must not have been mutated
    assert!(engine.current_slot == slot_before, "current_slot must not change");
    assert!(engine.last_oracle_price == oracle_before, "last_oracle_price must not change");
}

// ############################################################################
// FIX 7: config validation — max_accounts <= MAX_ACCOUNTS
// ############################################################################

/// new() with max_accounts > MAX_ACCOUNTS must panic.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
#[kani::should_panic]
fn proof_config_rejects_oversized_max_accounts() {
    let mut params = zero_fee_params();
    params.max_accounts = (MAX_ACCOUNTS as u64) + 1;
    let _engine = RiskEngine::new(params);
}

/// new() with max_accounts == 0 must panic.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
#[kani::should_panic]
fn proof_config_rejects_zero_max_accounts() {
    let mut params = zero_fee_params();
    params.max_accounts = 0;
    let _engine = RiskEngine::new(params);
}

/// new() with BPS > 10_000 must panic.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
#[kani::should_panic]
fn proof_config_rejects_invalid_bps() {
    let mut params = zero_fee_params();
    params.initial_margin_bps = 10_001;
    let _engine = RiskEngine::new(params);
}

/// new() with min_nonzero_im_req > min_initial_deposit must panic (spec §1.4).
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
#[kani::should_panic]
fn proof_config_rejects_im_gt_deposit() {
    let mut params = zero_fee_params();
    params.min_nonzero_im_req = 100;
    params.min_initial_deposit = U128::new(50); // im > deposit violates §1.4
    let _engine = RiskEngine::new(params);
}

// ############################################################################
// FIX 8: close_account_not_atomic checks PnL before forgiving fee debt
// ############################################################################

/// close_account_not_atomic must not forgive fee debt if PnL > 0 (warmup not complete).
/// The PnL check must come BEFORE fee forgiveness.
///
/// Setup: flat account with positive reserved PnL (warmup incomplete),
/// zero capital (so fee_debt_sweep is a no-op), and fee debt.
/// After the failed close, fee_credits must remain negative (not forgiven).
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_close_account_pnl_check_before_fee_forgive() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let idx = engine.add_user(0).unwrap();

    // Set up consistent state: flat, PnL > 0 (fully reserved), capital = 0, fee debt
    // Use set_pnl to keep pnl_pos_tot in sync
    engine.set_pnl(idx as usize, 5000i128);
    // All PnL is reserved (warmup not complete)
    engine.accounts[idx as usize].reserved_pnl = 5000;
    // Zero capital — fee_debt_sweep will be a no-op
    // (capital is already 0 from add_user with fee=0)

    // Fee debt
    engine.accounts[idx as usize].fee_credits = I128::new(-1000);
    let fc_before = engine.accounts[idx as usize].fee_credits.get();

    // close_account_not_atomic: touch will be no-op for fees (capital=0),
    // do_profit_conversion: released = max(5000,0) - 5000 = 0, so skip.
    // PnL check: pnl > 0 → Err(PnlNotWarmedUp)
    let result = engine.close_account_not_atomic(idx, DEFAULT_SLOT, DEFAULT_ORACLE, 0i64);
    assert!(result.is_err(), "close_account_not_atomic must reject when pnl > 0");

    // fee_credits must NOT have been zeroed by forgiveness (PnL check is first)
    assert!(
        engine.accounts[idx as usize].fee_credits.get() == fc_before,
        "fee_credits must not be forgiven on Err path"
    );
}

// ############################################################################
// FIX 9: settle_side_effects epoch_snap = 0 on zero-out (spec §2.4)
// ############################################################################

/// When settle_side_effects zeroes a position (same-epoch truncation),
/// epoch_snap must be set to 0, not epoch_side.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_settle_epoch_snap_zero_on_truncation() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let a = engine.add_user(0).unwrap();
    let b = engine.add_user(0).unwrap();

    engine.deposit(a, 1_000_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.deposit(b, 1_000_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    // Set non-trivial ADL epoch
    engine.adl_epoch_long = 5;
    engine.adl_epoch_short = 5;

    // Open a tiny position (1 unit of basis)
    let tiny = 1i128;
    engine.execute_trade_not_atomic(a, b, DEFAULT_ORACLE, DEFAULT_SLOT, tiny, DEFAULT_ORACLE, 0i64).unwrap();

    // Trigger an ADL that sets a_long to a value that would truncate the position to 0.
    // The simplest way: directly manipulate adl_mult_long to 0 (below MIN_A_SIDE).
    // But that's invalid. Instead, set a very small a_mult to make floor(basis * a / a_basis) = 0.
    // With basis=1, a_basis=ADL_ONE=1_000_000, if a_mult < 1_000_000 the floor gives 0.
    engine.adl_mult_long = 1; // Very small — floor(1 * 1 / 1_000_000) = 0

    // Now touch the account — settle_side_effects should zero the position
    let _ = engine.touch_account_full_not_atomic(a as usize, DEFAULT_ORACLE, DEFAULT_SLOT);

    // If position was zeroed, epoch_snap must be 0 per §2.4
    if engine.accounts[a as usize].position_basis_q == 0 {
        assert!(
            engine.accounts[a as usize].adl_epoch_snap == 0,
            "epoch_snap must be 0 on settle zero-out per §2.4"
        );
    }
}

// ############################################################################
// FIX 9: validate_keeper_hint maps None → None (spec §11.2)
// ############################################################################

/// A None hint must produce None (no liquidation), not FullClose.
/// Spec §11.2: absent hint = no liquidation action for this candidate.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_keeper_hint_none_returns_none() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let a = engine.add_user(0).unwrap();
    let b = engine.add_user(0).unwrap();
    engine.deposit(a, 100_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.deposit(b, 100_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    // Open a position so eff != 0
    let size: i128 = (POS_SCALE as i128) * 10;
    engine.execute_trade_not_atomic(a, b, DEFAULT_ORACLE, DEFAULT_SLOT, size, DEFAULT_ORACLE, 0i64).unwrap();

    let eff = engine.effective_pos_q(a as usize);
    assert!(eff != 0);

    // None hint must return None per §11.2
    let result = engine.validate_keeper_hint(a, eff, &None, DEFAULT_ORACLE);
    assert!(result.is_none(), "None hint must return None per spec §11.2");
}

/// A FullClose hint must return Some(FullClose).
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_keeper_hint_fullclose_passthrough() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let a = engine.add_user(0).unwrap();
    let b = engine.add_user(0).unwrap();
    engine.deposit(a, 100_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.deposit(b, 100_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    let size: i128 = (POS_SCALE as i128) * 10;
    engine.execute_trade_not_atomic(a, b, DEFAULT_ORACLE, DEFAULT_SLOT, size, DEFAULT_ORACLE, 0i64).unwrap();

    let eff = engine.effective_pos_q(a as usize);
    let hint = Some(LiquidationPolicy::FullClose);
    let result = engine.validate_keeper_hint(a, eff, &hint, DEFAULT_ORACLE);
    assert!(
        matches!(result, Some(LiquidationPolicy::FullClose)),
        "FullClose hint must pass through"
    );
}

// ############################################################################
// FIX 10: GC cursor advances by actual scan count, not max_scan
// ############################################################################

/// After garbage_collect_dust with no dust accounts, gc_cursor must still
/// advance by the number of slots scanned (all MAX_ACCOUNTS when no early break).
/// With zero used accounts, scanned == min(ACCOUNTS_PER_CRANK, MAX_ACCOUNTS)
/// and gc_cursor wraps around accordingly.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_gc_cursor_advances_by_scanned() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let cursor_before = engine.gc_cursor;

    // No accounts → nothing to GC, but cursor must advance by scanned count
    let num_freed = engine.garbage_collect_dust();
    assert_eq!(num_freed, 0, "no accounts to GC");

    let cursor_after = engine.gc_cursor;
    let max_scan = core::cmp::min(ACCOUNTS_PER_CRANK as usize, MAX_ACCOUNTS);
    let mask = MAX_ACCOUNTS - 1;
    let expected = ((cursor_before as usize + max_scan) & mask) as u16;
    assert_eq!(
        cursor_after, expected,
        "gc_cursor must advance by actual scanned count"
    );
}

/// When some dust accounts exist, gc_cursor advances by exactly the number
/// of offsets scanned (not max_scan). Under Kani (MAX_ACCOUNTS=4),
/// GC_CLOSE_BUDGET=32 > MAX_ACCOUNTS so the budget never triggers early break,
/// but the scanned-count tracking is still exercised.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_gc_cursor_with_dust_accounts() {
    let mut engine = RiskEngine::new(zero_fee_params());

    // Create 2 dust accounts (< MAX_ACCOUNTS=4 under Kani)
    let a = engine.add_user(0).unwrap();
    engine.deposit(a, 1, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    let b = engine.add_user(0).unwrap();
    engine.deposit(b, 1, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    engine.gc_cursor = 0;
    let num_freed = engine.garbage_collect_dust();

    // Both accounts are dust (capital=1 < min_initial_deposit=2, flat, pnl=0)
    assert_eq!(num_freed, 2, "both dust accounts should be freed");

    // Cursor advances by min(ACCOUNTS_PER_CRANK, MAX_ACCOUNTS) = full scan
    // (no early break since GC_CLOSE_BUDGET=32 > 2 freed)
    let max_scan = core::cmp::min(ACCOUNTS_PER_CRANK as usize, MAX_ACCOUNTS);
    let mask = MAX_ACCOUNTS - 1;
    assert_eq!(engine.gc_cursor, ((0 + max_scan) & mask) as u16);
}

// ############################################################################
// FIX 11: validate_params rejects min_liquidation_abs > liquidation_fee_cap
// ############################################################################

/// validate_params must panic when min_liquidation_abs > liquidation_fee_cap.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
#[kani::should_panic]
fn proof_config_rejects_liq_fee_inversion() {
    let mut params = zero_fee_params();
    params.liquidation_fee_bps = 100;
    params.liquidation_fee_cap = U128::new(100);
    params.min_liquidation_abs = U128::new(200); // > cap → must panic
    let _ = RiskEngine::new(params);
}

/// validate_params must panic when liquidation_fee_cap > MAX_PROTOCOL_FEE_ABS.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
#[kani::should_panic]
fn proof_config_rejects_fee_cap_exceeds_max() {
    let mut params = zero_fee_params();
    params.liquidation_fee_cap = U128::new(MAX_PROTOCOL_FEE_ABS + 1);
    params.min_liquidation_abs = U128::new(0);
    let _ = RiskEngine::new(params);
}

// ############################################################################
// FIX 12: touch_account_full_not_atomic rejects out-of-bounds and unused accounts
// ############################################################################

/// touch_account_full_not_atomic on an unused slot must return AccountNotFound.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_touch_unused_returns_error() {
    let mut engine = RiskEngine::new(zero_fee_params());

    // Slot 0 is not used (no add_user called)
    let result = engine.touch_account_full_not_atomic(0, DEFAULT_ORACLE, DEFAULT_SLOT);
    assert!(result.is_err(), "touch on unused slot must fail");
}

/// touch_account_full_not_atomic on an out-of-bounds index must return error.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_touch_oob_returns_error() {
    let mut engine = RiskEngine::new(zero_fee_params());

    let result = engine.touch_account_full_not_atomic(MAX_ACCOUNTS, DEFAULT_ORACLE, DEFAULT_SLOT);
    assert!(result.is_err(), "touch on OOB index must fail");
}

// ############################################################################
// FIX 13: withdraw_not_atomic and execute_trade_not_atomic do not require fresh crank (spec §0 goal 6)
// ############################################################################

/// Withdraw must succeed even when no keeper_crank_not_atomic has ever run.
/// Spec §10.4 does not gate withdraw_not_atomic on keeper liveness.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_withdraw_no_crank_gate() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let idx = engine.add_user(0).unwrap();
    engine.deposit(idx, 10_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    // last_crank_slot is 0, now_slot is far ahead. Must still succeed.
    let far_slot = DEFAULT_SLOT + 100_000;
    let result = engine.withdraw_not_atomic(idx, 1_000, DEFAULT_ORACLE, far_slot, 0i64);
    assert!(result.is_ok(), "withdraw_not_atomic must not require fresh crank (spec §0 goal 6)");
}

/// execute_trade_not_atomic must succeed even when no keeper_crank_not_atomic has ever run.
/// Spec §10.5 does not gate execute_trade_not_atomic on keeper liveness.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_trade_no_crank_gate() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let a = engine.add_user(0).unwrap();
    let b = engine.add_user(0).unwrap();
    engine.deposit(a, 100_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.deposit(b, 100_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    // last_crank_slot is 0, now_slot is far ahead. Must still succeed.
    let far_slot = DEFAULT_SLOT + 100_000;
    let size: i128 = POS_SCALE as i128;
    let result = engine.execute_trade_not_atomic(a, b, DEFAULT_ORACLE, far_slot, size, DEFAULT_ORACLE, 0i64);
    assert!(result.is_ok(), "trade must not require fresh crank (spec §0 goal 6)");
}

// ############################################################################
// FIX 14: GC skips accounts with negative PnL (spec §2.6 precondition)
// ############################################################################

/// garbage_collect_dust must NOT free an account with PNL < 0.
/// Spec §2.6 requires PNL_i == 0 as a precondition for reclamation.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_gc_skips_negative_pnl() {
    let mut engine = RiskEngine::new(zero_fee_params());

    let idx = engine.add_user(0).unwrap();
    // Deposit 1 token (below min_initial_deposit=2), making it a dust candidate
    engine.deposit(idx, 1, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    // Directly set negative PnL to simulate a flat account with unresolved loss.
    // In production this arises when a position is closed at a loss but
    // touch_account_full_not_atomic → §7.3 hasn't run yet.
    engine.set_pnl(idx as usize, -100i128);

    let ins_before = engine.insurance_fund.balance.get();

    engine.gc_cursor = 0;
    let num_freed = engine.garbage_collect_dust();

    // GC must skip the account (PNL != 0 per §2.6 precondition)
    assert_eq!(num_freed, 0, "GC must not free account with PNL < 0");
    assert!(engine.is_used(idx as usize), "account must remain used");
    assert_eq!(engine.insurance_fund.balance.get(), ins_before,
        "GC must not draw from insurance for negative-PnL accounts");
}

// ############################################################################
// FIX 15: insurance_floor from RiskParams (spec §1.4)
// ############################################################################

/// A nonzero insurance_floor in RiskParams must be reflected in engine state.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_insurance_floor_from_params() {
    let mut params = zero_fee_params();
    params.insurance_floor = U128::new(5000);
    let engine = RiskEngine::new(params);
    assert_eq!(engine.params.insurance_floor.get(), 5000,
        "insurance_floor must come from RiskParams");
}

/// insurance_floor > MAX_VAULT_TVL must be rejected.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
#[kani::should_panic]
fn proof_config_rejects_excessive_insurance_floor() {
    let mut params = zero_fee_params();
    params.insurance_floor = U128::new(MAX_VAULT_TVL + 1);
    let _ = RiskEngine::new(params);
}

// ############################################################################
// Gap #4: validate_keeper_hint ExactPartial pre-flight matches step 14
// ############################################################################

/// If validate_keeper_hint approves ExactPartial(q), then the actual
/// liquidation step 14 (post-partial maintenance check) must also pass.
/// This proves the pre-flight is not over-optimistic.
///
/// We construct an underwater account, call validate_keeper_hint with a
/// symbolic q_close_q, and if the hint passes through (returns ExactPartial),
/// run the actual keeper_crank_not_atomic and verify it succeeds (doesn't fall back
/// to FullClose due to step 14 rejection).
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_validate_hint_preflight_conservative() {
    let mut engine = RiskEngine::new(zero_fee_params());

    let a = engine.add_user(0).unwrap();
    let b = engine.add_user(0).unwrap();

    engine.deposit(a, 1_000_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.deposit(b, 1_000_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    // Open position
    let size = (500 * POS_SCALE) as i128;
    engine.execute_trade_not_atomic(a, b, DEFAULT_ORACLE, DEFAULT_SLOT, size, DEFAULT_ORACLE, 0i64).unwrap();

    // Inject loss to make a underwater
    engine.set_pnl(a as usize, -800_000i128);

    // Symbolic q_close_q: 1..499 units (must be < abs(eff))
    let q_units: u16 = kani::any();
    kani::assume(q_units >= 1 && q_units <= 499);
    let q_close = (q_units as u128) * POS_SCALE;

    let eff = engine.effective_pos_q(a as usize);
    let hint = Some(LiquidationPolicy::ExactPartial(q_close));

    let validated = engine.validate_keeper_hint(a, eff, &hint, DEFAULT_ORACLE);

    // If pre-flight approves ExactPartial, step 14 must also pass
    if let Some(LiquidationPolicy::ExactPartial(q)) = validated {
        assert_eq!(q, q_close, "approved q must match");

        // Run actual liquidation via keeper_crank_not_atomic
        let slot2 = DEFAULT_SLOT + 1;
        let candidates = [(a, Some(LiquidationPolicy::ExactPartial(q)))];
        let result = engine.keeper_crank_not_atomic(slot2, DEFAULT_ORACLE, &candidates, 10, 0i64);

        // Crank must succeed (step 14 must pass if pre-flight said OK)
        assert!(result.is_ok(), "keeper_crank_not_atomic must succeed when pre-flight approved ExactPartial");

        // And the account must still have a position (partial, not converted to full close)
        let eff_after = engine.effective_pos_q(a as usize);
        kani::cover!(eff_after != 0, "partial liquidation preserved nonzero position");
    }

    // Cover both outcomes
    kani::cover!(matches!(validated, Some(LiquidationPolicy::ExactPartial(_))), "pre-flight approved partial");
    kani::cover!(matches!(validated, Some(LiquidationPolicy::FullClose)), "pre-flight escalated to full close");
}

/// Stronger variant: oracle changes between trade and crank, so settle_side_effects
/// produces nonzero pnl_delta. The pre-flight is called on the pre-crank engine
/// (before touch), but the crank's internal path touches the account first.
/// The pre-flight must still be conservative: if it approves ExactPartial,
/// step 14 must also pass. This exercises the settle_side_effects path.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_validate_hint_preflight_oracle_shift() {
    let mut engine = RiskEngine::new(zero_fee_params());

    let a = engine.add_user(0).unwrap();
    let b = engine.add_user(0).unwrap();

    engine.deposit(a, 1_000_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.deposit(b, 1_000_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    // Open position at DEFAULT_ORACLE (1000)
    let size = (500 * POS_SCALE) as i128;
    engine.execute_trade_not_atomic(a, b, DEFAULT_ORACLE, DEFAULT_SLOT, size, DEFAULT_ORACLE, 0i64).unwrap();

    // Inject loss to make a underwater
    engine.set_pnl(a as usize, -800_000i128);

    // Symbolic oracle shift: 900..1100 (±10% from DEFAULT_ORACLE=1000)
    let crank_oracle: u16 = kani::any();
    kani::assume(crank_oracle >= 900 && crank_oracle <= 1100);
    let crank_oracle = crank_oracle as u64;

    // Symbolic q_close_q: 1..499 units
    let q_units: u16 = kani::any();
    kani::assume(q_units >= 1 && q_units <= 499);
    let q_close = (q_units as u128) * POS_SCALE;

    let eff = engine.effective_pos_q(a as usize);
    let hint = Some(LiquidationPolicy::ExactPartial(q_close));

    // Pre-flight on pre-touch state with shifted oracle
    let validated = engine.validate_keeper_hint(a, eff, &hint, crank_oracle);

    // If pre-flight approves ExactPartial, run the actual crank with shifted oracle
    if let Some(LiquidationPolicy::ExactPartial(q)) = validated {
        let slot2 = DEFAULT_SLOT + 1;
        let candidates = [(a, Some(LiquidationPolicy::ExactPartial(q)))];
        // Crank uses the shifted oracle — touch will run settle_side_effects
        // producing nonzero pnl_delta from K-pair settlement
        let result = engine.keeper_crank_not_atomic(slot2, crank_oracle, &candidates, 10, 0i64);

        assert!(result.is_ok(),
            "keeper_crank_not_atomic must succeed when pre-flight approved ExactPartial (oracle-shifted)");
    }

    kani::cover!(matches!(validated, Some(LiquidationPolicy::ExactPartial(_))),
        "pre-flight approved partial with oracle shift");
    kani::cover!(matches!(validated, Some(LiquidationPolicy::FullClose)),
        "pre-flight escalated with oracle shift");
}

// ############################################################################
// set_owner defense-in-depth: owner-already-claimed guard
// ############################################################################

/// set_owner on an account whose owner is already set (non-zero) must reject.
/// This is a defense-in-depth guard — authorization is the wrapper's job,
/// but the engine should not silently overwrite an existing owner.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_set_owner_rejects_claimed() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let idx = engine.add_user(0).unwrap();
    engine.deposit(idx, 10_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    // Set initial owner
    let owner1 = [1u8; 32];
    let result1 = engine.set_owner(idx, owner1);
    assert!(result1.is_ok(), "first set_owner must succeed");

    // Attempt to overwrite with different owner must fail
    let owner2 = [2u8; 32];
    let result2 = engine.set_owner(idx, owner2);
    assert!(result2.is_err(), "set_owner on claimed account must reject");
    assert!(engine.accounts[idx as usize].owner == owner1,
        "owner must not change after rejection");
}

// ############################################################################
// force_close_resolved_not_atomic: conservation and correctness
// ############################################################################

/// force_close_resolved_not_atomic settles K-pair PnL on accounts with open positions.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_force_close_resolved_with_position_conserves() {
    let mut engine = RiskEngine::new(zero_fee_params());

    let a = engine.add_user(0).unwrap();
    let b = engine.add_user(0).unwrap();
    engine.deposit(a, 500_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.deposit(b, 500_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    let size = (100 * POS_SCALE) as i128;
    engine.execute_trade_not_atomic(a, b, DEFAULT_ORACLE, DEFAULT_SLOT, size, DEFAULT_ORACLE, 0i64).unwrap();

    // Symbolic loss on the position holder
    let loss: u32 = kani::any();
    kani::assume(loss >= 1 && loss <= 400_000);
    engine.set_pnl(a as usize, -(loss as i128));

    let result = engine.force_close_resolved_not_atomic(a, 100, 0);
    assert!(result.is_ok(), "force_close must succeed with open position");
    assert!(!engine.is_used(a as usize), "account must be freed");
    assert!(engine.check_conservation());
}

/// force_close_resolved_not_atomic converts positive PnL on flat accounts.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_force_close_resolved_with_profit_conserves() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let idx = engine.add_user(0).unwrap();
    engine.deposit(idx, 500_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    let profit: u16 = kani::any();
    kani::assume(profit >= 1 && profit <= 10000);
    engine.set_pnl(idx as usize, profit as i128);

    let cap_before = engine.accounts[idx as usize].capital.get();
    let result = engine.force_close_resolved_not_atomic(idx, 100, 0);
    assert!(result.is_ok(), "force_close must succeed with positive PnL");
    assert!(result.unwrap() >= cap_before, "returned must include converted profit");
    assert!(!engine.is_used(idx as usize));
    assert!(engine.check_conservation());
}

/// force_close_resolved_not_atomic on a flat account with no PnL returns exact capital.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_force_close_resolved_flat_returns_capital() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let idx = engine.add_user(0).unwrap();

    let dep: u32 = kani::any();
    kani::assume(dep >= 1 && dep <= 1_000_000);
    engine.deposit(idx, dep as u128, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    let result = engine.force_close_resolved_not_atomic(idx, 100, 0);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), dep as u128, "flat account must return exact capital");
    assert!(!engine.is_used(idx as usize));
    assert!(engine.check_conservation());
}

/// force_close_resolved_not_atomic with open position: conservation must hold.
/// Symbolic loss on position-holder exercises K-pair settlement + loss path.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_force_close_resolved_position_conservation() {
    let mut engine = RiskEngine::new(zero_fee_params());

    let a = engine.add_user(0).unwrap();
    let b = engine.add_user(0).unwrap();
    engine.deposit(a, 500_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.deposit(b, 500_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    let size = (100 * POS_SCALE) as i128;
    engine.execute_trade_not_atomic(a, b, DEFAULT_ORACLE, DEFAULT_SLOT, size, DEFAULT_ORACLE, 0i64).unwrap();

    // Advance K via price movement
    engine.keeper_crank_not_atomic(DEFAULT_SLOT + 1, 1500, &[], 64, 0i64).unwrap();

    let oi_long_before = engine.oi_eff_long_q;
    let result = engine.force_close_resolved_not_atomic(a, DEFAULT_SLOT + 1, 0);
    assert!(result.is_ok());
    assert!(!engine.is_used(a as usize));
    assert!(engine.accounts[a as usize].position_basis_q == 0);
    // OI must decrease (a was long)
    assert!(engine.oi_eff_long_q < oi_long_before,
        "OI long must decrease after force_close of long position");
    assert!(engine.check_conservation(),
        "V >= C_tot + I must hold after force_close with position");
}

/// force_close_resolved_not_atomic: stored_pos_count decrements correctly
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_force_close_resolved_pos_count_decrements() {
    let mut engine = RiskEngine::new(zero_fee_params());

    let a = engine.add_user(0).unwrap();
    let b = engine.add_user(0).unwrap();
    engine.deposit(a, 500_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.deposit(b, 500_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    let size = (100 * POS_SCALE) as i128;
    engine.execute_trade_not_atomic(a, b, DEFAULT_ORACLE, DEFAULT_SLOT, size, DEFAULT_ORACLE, 0i64).unwrap();

    let long_before = engine.stored_pos_count_long;
    let short_before = engine.stored_pos_count_short;

    engine.force_close_resolved_not_atomic(a, 100, 0).unwrap(); // a was long
    assert_eq!(engine.stored_pos_count_long, long_before - 1);
    assert_eq!(engine.stored_pos_count_short, short_before);

    engine.force_close_resolved_not_atomic(b, 100, 0).unwrap(); // b was short
    assert_eq!(engine.stored_pos_count_short, short_before - 1);
}

/// force_close_resolved_not_atomic with fee debt: insurance receives swept amount
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_force_close_resolved_fee_sweep_conservation() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let _ = engine.top_up_insurance_fund(100_000, 0);
    let idx = engine.add_user(0).unwrap();
    engine.deposit(idx, 50_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    // Symbolic fee debt
    let debt: u16 = kani::any();
    kani::assume(debt >= 1 && debt <= 40000);
    engine.accounts[idx as usize].fee_credits = I128::new(-(debt as i128));

    let ins_before = engine.insurance_fund.balance.get();
    let result = engine.force_close_resolved_not_atomic(idx, 100, 0);
    assert!(result.is_ok());

    // Insurance must have increased by swept amount
    let ins_after = engine.insurance_fund.balance.get();
    let swept = core::cmp::min(debt as u128, 50_000);
    assert_eq!(ins_after, ins_before + swept,
        "insurance must increase by exactly the swept fee debt");
    assert!(engine.check_conservation());
}

// ############################################################################
// Maintenance fee: conservation, fee debt, validate_params
// ############################################################################

/// Spec §8.2: maintenance fees enabled — touch charges dt * fee_per_slot.
/// Conservation holds with symbolic dt.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_maintenance_fee_conservation() {
    let mut params = zero_fee_params();
    params.maintenance_fee_per_slot = U128::new(100);
    let mut engine = RiskEngine::new(params);

    let a = engine.add_user(0).unwrap();
    engine.deposit(a, 500_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.last_oracle_price = DEFAULT_ORACLE;
    engine.last_market_slot = DEFAULT_SLOT;
    // Align last_fee_slot with DEFAULT_SLOT so dt_fee = dt exactly
    engine.accounts[a as usize].last_fee_slot = DEFAULT_SLOT;

    let cap_before = engine.accounts[a as usize].capital.get();

    let dt: u16 = kani::any();
    kani::assume(dt >= 1 && dt <= 1000);

    let slot2 = DEFAULT_SLOT + (dt as u64);
    let result = engine.touch_account_full_not_atomic(a as usize, DEFAULT_ORACLE, slot2);
    assert!(result.is_ok());

    // Fee = dt * 100, fully covered by 500k capital
    let expected_fee = (dt as u128) * 100;
    assert_eq!(cap_before - engine.accounts[a as usize].capital.get(), expected_fee,
        "capital must decrease by dt * fee_per_slot");
    assert!(engine.check_conservation());
}

/// Liveness: maintenance fee realization must NOT revert for large dt
/// with max fee_per_slot. With MAX_MAINTENANCE_FEE_PER_SLOT = 10^16 and
/// dt up to ~10^20 slots, fee_due can reach ~10^36 which must stay under
/// MAX_PROTOCOL_FEE_ABS = 10^36.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_maintenance_fee_large_dt_no_revert() {
    let mut params = zero_fee_params();
    params.maintenance_fee_per_slot = U128::new(MAX_MAINTENANCE_FEE_PER_SLOT);
    let mut engine = RiskEngine::new(params);

    let a = engine.add_user(0).unwrap();
    engine.deposit(a, 500_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.last_oracle_price = DEFAULT_ORACLE;
    engine.last_market_slot = DEFAULT_SLOT;

    // 100_000 slots of inactivity — would have bricked under old 10^20 cap
    let large_dt = 100_000u64;
    let slot2 = DEFAULT_SLOT + large_dt;
    let result = engine.touch_account_full_not_atomic(a as usize, DEFAULT_ORACLE, slot2);
    assert!(result.is_ok(), "large dt must not revert with correct MAX_PROTOCOL_FEE_ABS");
    assert!(engine.check_conservation());
}
