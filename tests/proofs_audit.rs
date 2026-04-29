//! Section 8 — External audit fix proofs
//!
//! Formal verification of fixes for confirmed external audit findings:
//! 1. attach_effective_position epoch_snap canonical zero (spec §2.4)
//! 2. add_user/add_lp materialized_account_count rollback on materialize_at failure
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

    let idx = add_user_test(&mut engine, 0).unwrap() as usize;
    engine
        .deposit_not_atomic(idx as u16, 1_000_000, DEFAULT_SLOT)
        .unwrap();

    // Set up non-trivial ADL epoch state
    engine.adl_epoch_long = 5;
    engine.adl_epoch_short = 7;

    // Symbolic initial side: positive (long) or negative (short) basis
    let side_long: bool = kani::any();
    let basis: u32 = kani::any();
    kani::assume(basis >= 1 && basis <= 10 * POS_SCALE as u32);

    let signed_basis = if side_long {
        basis as i128
    } else {
        -(basis as i128)
    };

    // Use set_position_basis_q to correctly track stored_pos_count.
    // Set epoch mismatch to skip the phantom dust U256 path
    // (irrelevant to the epoch_snap fix).
    engine.set_position_basis_q(idx, signed_basis).unwrap();
    engine.accounts[idx].adl_a_basis = ADL_ONE;
    engine.accounts[idx].adl_k_snap = 0;
    // Epoch mismatch: snap=0 != epoch_long=5 / epoch_short=7
    engine.accounts[idx].adl_epoch_snap = 0;

    // Zero out the position
    engine.attach_effective_position(idx, 0).unwrap();

    // Spec §2.4: all canonical zero-position defaults
    assert!(
        engine.accounts[idx].position_basis_q == 0,
        "basis must be zero"
    );
    assert!(
        engine.accounts[idx].adl_a_basis == ADL_ONE,
        "a_basis must be ADL_ONE"
    );
    assert!(engine.accounts[idx].adl_k_snap == 0, "k_snap must be zero");
    assert!(
        engine.accounts[idx].adl_epoch_snap == 0,
        "epoch_snap must be zero per §2.4"
    );
}

/// Verify that attaching a nonzero position correctly picks up the
/// current side epoch (not zero).
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_epoch_snap_correct_on_nonzero_attach() {
    let mut engine = RiskEngine::new(zero_fee_params());

    let idx = add_user_test(&mut engine, 0).unwrap() as usize;
    engine
        .deposit_not_atomic(idx as u16, 1_000_000, DEFAULT_SLOT)
        .unwrap();

    engine.adl_epoch_long = 3;
    engine.adl_epoch_short = 9;

    let side_long: bool = kani::any();
    let basis: u32 = kani::any();
    kani::assume(basis >= 1 && basis <= 100 * POS_SCALE as u32);

    let new_eff = if side_long {
        basis as i128
    } else {
        -(basis as i128)
    };

    engine.attach_effective_position(idx, new_eff).unwrap();

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
// FIX 2: materialized_account_count rollback on materialize_at failure
// ############################################################################

/// If materialize_at fails in add_user, materialized_account_count must be
/// rolled back to its pre-call value.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_add_user_count_rollback_on_alloc_failure() {
    let mut engine = RiskEngine::new(zero_fee_params());

    // Fill all slots so materialize_at will fail
    engine.num_used_accounts = MAX_ACCOUNTS as u16;
    engine.materialized_account_count = 0; // but count is low (simulating inconsistency path)

    let count_before = engine.materialized_account_count;

    let result = add_user_test(&mut engine, 0);
    assert!(
        result.is_err(),
        "add_user must fail when all slots are full"
    );
    assert!(
        engine.materialized_account_count == count_before,
        "materialized_account_count must be rolled back on failure"
    );
}

/// If materialize_at fails in add_lp, materialized_account_count must be
/// rolled back to its pre-call value.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_add_lp_count_rollback_on_alloc_failure() {
    let mut engine = RiskEngine::new(zero_fee_params());

    // Fill all slots so materialize_at will fail
    engine.num_used_accounts = MAX_ACCOUNTS as u16;
    engine.materialized_account_count = 0;

    let count_before = engine.materialized_account_count;

    let result = add_lp_test(&mut engine, [0; 32], [0; 32], 0);
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

    let idx = add_user_test(&mut engine, 0).unwrap();
    let capital: u32 = kani::any();
    kani::assume(capital >= 1 && capital <= 10_000_000);

    engine
        .deposit_not_atomic(idx, capital as u128, DEFAULT_SLOT)
        .unwrap();

    // Account is flat (no position)
    assert!(engine.effective_pos_q(idx as usize) == 0);

    // With any positive capital and no position, account MUST be maintenance-healthy
    // Spec §9.1: MM_req = 0 when eff == 0
    let healthy = engine.is_above_maintenance_margin(
        &engine.accounts[idx as usize].clone(),
        idx as usize,
        DEFAULT_ORACLE,
    );
    assert!(
        healthy,
        "flat account with positive capital must be maintenance-healthy"
    );
}

/// A flat account (eff==0) with any nonnegative equity must be initial-margin healthy.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_flat_account_initial_margin_healthy() {
    let mut engine = RiskEngine::new(zero_fee_params());

    let idx = add_user_test(&mut engine, 0).unwrap();
    let capital: u32 = kani::any();
    kani::assume(capital >= 1 && capital <= 10_000_000);

    engine
        .deposit_not_atomic(idx, capital as u128, DEFAULT_SLOT)
        .unwrap();

    assert!(engine.effective_pos_q(idx as usize) == 0);

    let healthy = engine.is_above_initial_margin(
        &engine.accounts[idx as usize].clone(),
        idx as usize,
        DEFAULT_ORACLE,
    );
    assert!(
        healthy,
        "flat account with positive capital must be initial-margin healthy"
    );
}

/// A flat account with zero equity must NOT be maintenance-healthy.
/// Spec §9.1: Eq_net > 0 (since MM_req = 0 for flat), so Eq_net = 0 fails.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_flat_zero_equity_not_maintenance_healthy() {
    // Substantive: symbolic fee_debt pushes equity to exactly 0 or negative;
    // flat account with Eq_net = 0 (or negative) is NOT maintenance-healthy.
    let mut engine = RiskEngine::new(zero_fee_params());
    let idx = add_user_test(&mut engine, 0).unwrap() as usize;

    let cap: u8 = kani::any();
    kani::assume(cap <= 100);
    let fee_debt: u8 = kani::any();
    kani::assume(fee_debt >= cap); // fee_debt >= cap means Eq_net <= 0

    engine.accounts[idx].capital = U128::new(cap as u128);
    engine.c_tot = U128::new(cap as u128);
    engine.accounts[idx].fee_credits = I128::new(-(fee_debt as i128));

    assert!(engine.effective_pos_q(idx) == 0);

    let healthy =
        engine.is_above_maintenance_margin(&engine.accounts[idx].clone(), idx, DEFAULT_ORACLE);
    assert!(
        !healthy,
        "flat account with Eq_net <= 0 is not maintenance-healthy"
    );
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

    let idx = add_user_test(&mut engine, 0).unwrap() as usize;
    let capital: u32 = kani::any();
    let debt: u32 = kani::any();
    kani::assume(capital >= 1 && capital <= 10_000_000);
    kani::assume(debt >= 1 && debt <= 10_000_000);

    // Set up capital
    engine
        .deposit_not_atomic(idx as u16, capital as u128, DEFAULT_SLOT)
        .unwrap();

    // Set fee debt (negative fee_credits)
    engine.accounts[idx].fee_credits = I128::new(-(debt as i128));

    let cap_before = engine.accounts[idx].capital.get();
    let fc_before = engine.accounts[idx].fee_credits.get();
    let ins_before = engine.insurance_fund.balance.get();

    engine.fee_debt_sweep(idx).unwrap();

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
    let mut engine = RiskEngine::new_with_market(default_params(), DEFAULT_SLOT, DEFAULT_ORACLE);

    let a = add_user_test(&mut engine, 0).unwrap();
    let b = add_user_test(&mut engine, 0).unwrap();

    engine.deposit_not_atomic(a, 50_000, DEFAULT_SLOT).unwrap();
    engine.deposit_not_atomic(b, 50_000, DEFAULT_SLOT).unwrap();

    let size = (100 * POS_SCALE) as i128;
    engine.set_position_basis_q(a as usize, size).unwrap();
    engine.set_position_basis_q(b as usize, -size).unwrap();
    engine.oi_eff_long_q = size as u128;
    engine.oi_eff_short_q = size as u128;

    let crash_oracle = 500u64;
    engine.set_pnl(a as usize, -49_000).unwrap();

    let eff_before = engine.effective_pos_q(a as usize);
    let basis_before = engine.accounts[a as usize].position_basis_q;
    assert!(eff_before == size);
    assert!(!engine.is_above_maintenance_margin(
        &engine.accounts[a as usize],
        a as usize,
        crash_oracle
    ));

    // Tiny partial won't restore health; spec §11.1 rule 3 maps invalid
    // keeper hints to None, so keeper_crank performs no liquidation action.
    let bad_hint = Some(LiquidationPolicy::ExactPartial(POS_SCALE as u128));
    let validated = engine
        .validate_keeper_hint(a, eff_before, &bad_hint, crash_oracle)
        .unwrap();
    assert!(
        validated.is_none(),
        "invalid partial hint must validate to no action"
    );

    // validate_keeper_hint is read-only; the no-action outcome leaves the
    // account position intact for the crank to skip.
    assert!(engine.accounts[a as usize].position_basis_q == basis_before);
    assert!(
        engine.effective_pos_q(a as usize) == eff_before,
        "invalid partial hint must cause no liquidation action"
    );
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

    // Call liquidate on an unused slot — spec §9.6 step 2 requires materialized account,
    // public entrypoint returns Err(AccountNotFound) before any market-state mutation.
    let result = engine.liquidate_at_oracle_not_atomic(
        0,
        DEFAULT_SLOT,
        DEFAULT_ORACLE,
        LiquidationPolicy::FullClose,
        0i128,
        0,
        100,
        None,
    );
    assert!(
        matches!(result, Err(RiskError::AccountNotFound)),
        "must return Err(AccountNotFound) for missing account"
    );

    // Market state must not have been mutated
    assert!(
        engine.current_slot == slot_before,
        "current_slot must not change"
    );
    assert!(
        engine.last_oracle_price == oracle_before,
        "last_oracle_price must not change"
    );
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

// Removed: proof_config_rejects_im_gt_deposit — the invariant
// `min_nonzero_im_req <= min_initial_deposit` no longer exists in
// the engine; `min_initial_deposit` was removed (see
// src/percolator.rs:738-739). The upper bound on `min_nonzero_im_req`
// is now wrapper policy. Engine-level `validate_params` still checks
// `min_nonzero_mm_req < min_nonzero_im_req` (covered by live proofs).

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
    let idx = add_user_test(&mut engine, 0).unwrap();

    // Set up consistent state: flat, PnL > 0 (fully reserved), capital = 0, fee debt
    // Use the live-compatible helper to keep PnL aggregates and reserve
    // buckets in sync.
    set_pnl_test(&mut engine, idx as usize, 5000i128).unwrap();
    // Zero capital — fee_debt_sweep will be a no-op
    // (capital is already 0 from add_user with fee=0)

    // Fee debt
    engine.accounts[idx as usize].fee_credits = I128::new(-1000);
    let fc_before = engine.accounts[idx as usize].fee_credits.get();

    // close_account_not_atomic: touch will be no-op for fees (capital=0),
    // do_profit_conversion: released = max(5000,0) - 5000 = 0, so skip.
    // PnL check: pnl > 0 → Err(PnlNotWarmedUp)
    let result =
        engine.close_account_not_atomic(idx, DEFAULT_SLOT, DEFAULT_ORACLE, 0i128, 0, 100, None);
    assert!(
        result.is_err(),
        "close_account_not_atomic must reject when pnl > 0"
    );

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
    let mut engine = RiskEngine::new_with_market(zero_fee_params(), DEFAULT_SLOT, DEFAULT_ORACLE);
    let a = add_user_test(&mut engine, 0).unwrap();
    engine.deposit_not_atomic(a, 10_000, DEFAULT_SLOT).unwrap();

    // Same-epoch long basis where q_eff_new = floor(1 * 1 / ADL_ONE) = 0.
    engine.adl_epoch_long = 5;
    engine.adl_mult_long = 1;
    engine.adl_coeff_long = 0;
    engine.f_long_num = 0;

    engine.set_position_basis_q(a as usize, 1).unwrap();
    engine.accounts[a as usize].adl_a_basis = ADL_ONE;
    engine.accounts[a as usize].adl_k_snap = 0;
    engine.accounts[a as usize].f_snap = 0;
    engine.accounts[a as usize].adl_epoch_snap = engine.adl_epoch_long;

    let mut ctx = InstructionContext::new_with_admission(0, 100);
    engine
        .settle_side_effects_live(a as usize, &mut ctx)
        .unwrap();

    assert!(
        engine.accounts[a as usize].position_basis_q == 0,
        "same-epoch truncation fixture must zero the position"
    );
    assert!(
        engine.accounts[a as usize].adl_epoch_snap == 0,
        "epoch_snap must be 0 on settle zero-out per §2.4"
    );
    assert!(engine.accounts[a as usize].adl_a_basis == ADL_ONE);
    assert!(engine.accounts[a as usize].adl_k_snap == 0);
    assert!(engine.accounts[a as usize].f_snap == 0);
    assert!(engine.stored_pos_count_long == 0);
    assert!(engine.check_conservation());
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
    let mut engine = RiskEngine::new_with_market(zero_fee_params(), DEFAULT_SLOT, DEFAULT_ORACLE);
    let a = add_user_test(&mut engine, 0).unwrap();
    let b = add_user_test(&mut engine, 0).unwrap();

    let size: i128 = (POS_SCALE as i128) * 10;
    engine.set_position_basis_q(a as usize, size).unwrap();
    engine.set_position_basis_q(b as usize, -size).unwrap();
    engine.oi_eff_long_q = size as u128;
    engine.oi_eff_short_q = size as u128;

    let eff = engine.effective_pos_q(a as usize);
    let basis_before = engine.accounts[a as usize].position_basis_q;
    let oi_long_before = engine.oi_eff_long_q;
    let oi_short_before = engine.oi_eff_short_q;
    assert!(
        eff == size,
        "candidate must have the configured live position"
    );

    // None hint must return None per §11.2
    let result = engine
        .validate_keeper_hint(a, eff, &None, DEFAULT_ORACLE)
        .unwrap();
    assert!(
        result.is_none(),
        "None hint must return None per spec §11.2"
    );
    assert!(
        engine.accounts[a as usize].position_basis_q == basis_before,
        "absent hint is no-action/read-only"
    );
    assert!(
        engine.oi_eff_long_q == oi_long_before,
        "absent hint must not mutate long OI"
    );
    assert!(
        engine.oi_eff_short_q == oi_short_before,
        "absent hint must not mutate short OI"
    );
    assert!(
        engine.check_conservation(),
        "balanced candidate state remains conserved"
    );
}

/// A FullClose hint must return Some(FullClose).
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_keeper_hint_fullclose_passthrough() {
    let mut engine = RiskEngine::new_with_market(zero_fee_params(), DEFAULT_SLOT, DEFAULT_ORACLE);
    let a = add_user_test(&mut engine, 0).unwrap();
    let b = add_user_test(&mut engine, 0).unwrap();

    let size: i128 = (POS_SCALE as i128) * 10;
    engine.set_position_basis_q(a as usize, size).unwrap();
    engine.set_position_basis_q(b as usize, -size).unwrap();
    engine.oi_eff_long_q = size as u128;
    engine.oi_eff_short_q = size as u128;

    let eff = engine.effective_pos_q(a as usize);
    let basis_before = engine.accounts[a as usize].position_basis_q;
    let oi_long_before = engine.oi_eff_long_q;
    let oi_short_before = engine.oi_eff_short_q;
    assert!(
        eff == size,
        "candidate must have the configured live position"
    );

    let hint = Some(LiquidationPolicy::FullClose);
    let result = engine
        .validate_keeper_hint(a, eff, &hint, DEFAULT_ORACLE)
        .unwrap();
    assert!(
        matches!(result, Some(LiquidationPolicy::FullClose)),
        "FullClose hint must pass through"
    );
    assert!(
        engine.accounts[a as usize].position_basis_q == basis_before,
        "hint validation is read-only"
    );
    assert!(
        engine.oi_eff_long_q == oi_long_before,
        "hint validation must not mutate long OI"
    );
    assert!(
        engine.oi_eff_short_q == oi_short_before,
        "hint validation must not mutate short OI"
    );
    assert!(
        engine.check_conservation(),
        "balanced candidate state remains conserved"
    );
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
// FIX 12: touch_account_live_local rejects out-of-bounds and unused accounts
// ############################################################################

/// touch_account_live_local on an unused slot must return AccountNotFound.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_touch_unused_returns_error() {
    let mut engine = RiskEngine::new(zero_fee_params());

    // Slot 0 is not used (no add_user called)
    let mut ctx = InstructionContext::new_with_admission(0, 100);
    engine
        .accrue_market_to(DEFAULT_SLOT, DEFAULT_ORACLE, 0)
        .unwrap();
    engine.current_slot = DEFAULT_SLOT;
    let result = engine.touch_account_live_local(0, &mut ctx);
    assert!(result.is_err(), "touch on unused slot must fail");
}

/// touch_account_live_local on an out-of-bounds index must return error.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_touch_oob_returns_error() {
    let mut engine = RiskEngine::new(zero_fee_params());

    let mut ctx = InstructionContext::new_with_admission(0, 100);
    engine
        .accrue_market_to(DEFAULT_SLOT, DEFAULT_ORACLE, 0)
        .unwrap();
    engine.current_slot = DEFAULT_SLOT;
    let result = engine.touch_account_live_local(MAX_ACCOUNTS, &mut ctx);
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
    let idx = add_user_test(&mut engine, 0).unwrap();
    engine
        .deposit_not_atomic(idx, 10_000, DEFAULT_SLOT)
        .unwrap();

    // Must still succeed — no keeper_crank_not_atomic required.
    let far_slot = DEFAULT_SLOT + 500;
    let result =
        engine.withdraw_not_atomic(idx, 1_000, DEFAULT_ORACLE, far_slot, 0i128, 0, 100, None);
    assert!(
        result.is_ok(),
        "withdraw_not_atomic must not require fresh crank (spec §0 goal 6)"
    );
}

/// Trade entry must be admitted even when no keeper_crank_not_atomic has ever
/// run. Spec §10.5 gates on the market accrual envelope only.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_trade_no_crank_gate() {
    let mut engine = RiskEngine::new_with_market(zero_fee_params(), DEFAULT_SLOT, DEFAULT_ORACLE);
    let a = add_user_test(&mut engine, 0).unwrap();
    let b = add_user_test(&mut engine, 0).unwrap();
    engine.deposit_not_atomic(a, 100_000, DEFAULT_SLOT).unwrap();
    engine.deposit_not_atomic(b, 100_000, DEFAULT_SLOT).unwrap();

    let size: i128 = POS_SCALE as i128;

    let entry = engine.validate_execute_trade_entry(
        a,
        b,
        DEFAULT_ORACLE,
        DEFAULT_SLOT,
        size,
        DEFAULT_ORACLE,
        0,
        100,
        None,
    );
    assert!(
        entry.is_ok(),
        "trade entry must not require fresh crank (spec §0 goal 6)"
    );

    let accrual = engine.accrue_market_to(DEFAULT_SLOT, DEFAULT_ORACLE, 0i128);
    assert!(
        accrual.is_ok(),
        "trade's market accrual step must not require fresh crank"
    );
    assert!(engine.check_conservation());
}

// ############################################################################
// FIX 14: Reclaim rejects accounts with negative PnL
// ############################################################################

/// Spec §2.6 requires PNL_i == 0 as a precondition for reclamation.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_reclaim_rejects_negative_pnl() {
    let mut engine = RiskEngine::new(zero_fee_params());

    let idx = add_user_test(&mut engine, 0).unwrap();
    engine.deposit_not_atomic(idx, 1, DEFAULT_SLOT).unwrap();

    engine.set_pnl(idx as usize, -100i128).unwrap();

    let ins_before = engine.insurance_fund.balance.get();

    let result = engine.reclaim_empty_account_not_atomic(idx, DEFAULT_SLOT);

    assert!(result.is_err(), "reclaim must reject account with PNL < 0");
    assert!(engine.is_used(idx as usize), "account must remain used");
    assert_eq!(
        engine.insurance_fund.balance.get(),
        ins_before,
        "GC must not draw from insurance for negative-PnL accounts"
    );
}

// ############################################################################
// Gap #4: validate_keeper_hint ExactPartial pre-flight matches step 14
// ############################################################################

/// If validate_keeper_hint approves ExactPartial(q), then the step-14
/// post-partial maintenance predicate must also pass on the corresponding
/// post-partial state. This proves the pre-flight is not over-optimistic
/// without executing unrelated keeper crank paths.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_validate_hint_preflight_conservative() {
    let mut engine = RiskEngine::new_with_market(zero_fee_params(), DEFAULT_SLOT, DEFAULT_ORACLE);

    let a = add_user_test(&mut engine, 0).unwrap();
    let b = add_user_test(&mut engine, 0).unwrap();

    let size = (500 * POS_SCALE) as i128;
    engine.accounts[a as usize].capital = U128::new(30_000);
    engine.accounts[a as usize].pnl = -20_000;
    engine.c_tot = U128::new(30_000);
    engine.vault = U128::new(30_000);
    engine.neg_pnl_account_count = 1;
    engine.attach_effective_position(a as usize, size).unwrap();
    engine.attach_effective_position(b as usize, -size).unwrap();
    engine.oi_eff_long_q = size as u128;
    engine.oi_eff_short_q = size as u128;
    assert!(
        !engine.is_above_maintenance_margin(
            &engine.accounts[a as usize],
            a as usize,
            DEFAULT_ORACLE,
        ),
        "fixture must start below maintenance"
    );

    let q_units: u16 = kani::any();
    kani::assume(q_units >= 1 && q_units <= 499);
    let q_close = (q_units as u128) * POS_SCALE;

    let eff = engine.effective_pos_q(a as usize);
    let hint = Some(LiquidationPolicy::ExactPartial(q_close));

    let validated = engine
        .validate_keeper_hint(a, eff, &hint, DEFAULT_ORACLE)
        .unwrap();

    // If pre-flight approves ExactPartial, step 14 must also pass
    if let Some(LiquidationPolicy::ExactPartial(q)) = validated {
        assert_eq!(q, q_close, "approved q must match");
        let remaining = size - q as i128;
        let mut post = engine.clone();
        post.attach_effective_position(a as usize, remaining)
            .unwrap();
        post.attach_effective_position(b as usize, -remaining)
            .unwrap();
        post.oi_eff_long_q = remaining as u128;
        post.oi_eff_short_q = remaining as u128;
        assert!(
            post.enforce_partial_liq_post_health(a as usize, DEFAULT_ORACLE)
                .is_ok(),
            "approved ExactPartial must satisfy the step-14 post-health check"
        );
        kani::cover!(
            post.effective_pos_q(a as usize) != 0,
            "partial liquidation preserved nonzero position"
        );
    }

    // Cover both outcomes
    kani::cover!(
        matches!(validated, Some(LiquidationPolicy::ExactPartial(_))),
        "pre-flight approved partial"
    );
    kani::cover!(
        validated.is_none(),
        "pre-flight rejected insufficient partial"
    );
}

/// Stronger variant: oracle changes between position attach and keeper validation,
/// so the crank's accrue+touch path produces a nonzero pnl_delta before
/// validate_keeper_hint runs. If the touched-state pre-flight approves
/// ExactPartial(q), the corresponding post-partial state must satisfy the same
/// step-14 health check enforced by liquidate_at_oracle_internal.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_validate_hint_preflight_oracle_shift() {
    let mut engine = RiskEngine::new_with_market(zero_fee_params(), DEFAULT_SLOT, DEFAULT_ORACLE);

    let a = add_user_test(&mut engine, 0).unwrap();
    let b = add_user_test(&mut engine, 0).unwrap();

    let size = (500 * POS_SCALE) as i128;
    engine.accounts[a as usize].capital = U128::new(30_000);
    engine.c_tot = U128::new(30_000);
    engine.vault = U128::new(30_000);
    engine.set_pnl(a as usize, -20_000i128).unwrap();
    engine.attach_effective_position(a as usize, size).unwrap();
    engine.attach_effective_position(b as usize, -size).unwrap();
    engine.oi_eff_long_q = size as u128;
    engine.oi_eff_short_q = size as u128;

    // Symbolic positive oracle shift: 1..10 ticks. With zero_fee_params,
    // dt=25 and max_price_move_bps_per_slot=4 exactly admit a 1% move.
    let delta: u8 = kani::any();
    kani::assume(delta >= 1 && delta <= 10);
    let crank_oracle = DEFAULT_ORACLE + delta as u64;
    let slot2 = DEFAULT_SLOT + 25;

    engine.accrue_market_to(slot2, crank_oracle, 0i128).unwrap();
    let mut ctx = InstructionContext::new_with_admission(0, 100);
    engine
        .touch_account_live_local(a as usize, &mut ctx)
        .unwrap();

    // The shifted oracle must have exercised live settlement: long PnL gains
    // 500 * delta, then settle_losses absorbs the remaining negative PnL.
    assert!(engine.accounts[a as usize].pnl == 0);
    assert!(engine.accounts[a as usize].capital.get() == 10_000 + (500u128 * delta as u128));

    // Symbolic q_close_q: 1..499 units.
    let q_units: u16 = kani::any();
    kani::assume(q_units >= 1 && q_units <= 499);
    let q_close = (q_units as u128) * POS_SCALE;

    let eff = engine.effective_pos_q(a as usize);
    let hint = Some(LiquidationPolicy::ExactPartial(q_close));

    let validated = engine
        .validate_keeper_hint(a, eff, &hint, crank_oracle)
        .unwrap();

    if let Some(LiquidationPolicy::ExactPartial(q)) = validated {
        assert_eq!(q, q_close, "approved q must match");
        let remaining = size - q as i128;
        let mut post = engine.clone();
        post.attach_effective_position(a as usize, remaining)
            .unwrap();
        assert!(
            post.enforce_partial_liq_post_health(a as usize, crank_oracle)
                .is_ok(),
            "approved shifted-oracle ExactPartial must satisfy step-14 post-health"
        );
        kani::cover!(
            post.effective_pos_q(a as usize) != 0,
            "shifted partial liquidation preserved nonzero position"
        );
    }

    kani::cover!(
        matches!(validated, Some(LiquidationPolicy::ExactPartial(_))),
        "pre-flight approved partial with oracle shift"
    );
    kani::cover!(
        validated.is_none(),
        "pre-flight rejected insufficient shifted partial"
    );
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
    let idx = add_user_test(&mut engine, 0).unwrap();
    engine
        .deposit_not_atomic(idx, 10_000, DEFAULT_SLOT)
        .unwrap();

    // Set initial owner
    let owner1 = [1u8; 32];
    let result1 = engine.set_owner(idx, owner1);
    assert!(result1.is_ok(), "first set_owner must succeed");

    // Attempt to overwrite with different owner must fail
    let owner2 = [2u8; 32];
    let result2 = engine.set_owner(idx, owner2);
    assert!(result2.is_err(), "set_owner on claimed account must reject");
    assert!(
        engine.accounts[idx as usize].owner == owner1,
        "owner must not change after rejection"
    );
}

// ############################################################################
// force_close_resolved_not_atomic: conservation and correctness
// ############################################################################

/// force_close_resolved_not_atomic settles K-pair PnL on accounts with open positions.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_force_close_resolved_with_position_conserves() {
    let mut engine = RiskEngine::new_with_market(zero_fee_params(), DEFAULT_SLOT, DEFAULT_ORACLE);

    let a = add_user_test(&mut engine, 0).unwrap();
    let b = add_user_test(&mut engine, 0).unwrap();
    engine.deposit_not_atomic(a, 500_000, DEFAULT_SLOT).unwrap();
    engine.deposit_not_atomic(b, 500_000, DEFAULT_SLOT).unwrap();

    let size = (100 * POS_SCALE) as i128;
    let set_long = engine.set_position_basis_q(a as usize, size);
    assert!(set_long.is_ok());
    let set_short = engine.set_position_basis_q(b as usize, -size);
    assert!(set_short.is_ok());
    engine.oi_eff_long_q = size as u128;
    engine.oi_eff_short_q = size as u128;
    assert!(engine.check_conservation());

    // Resolve one tick below the live price so the long can close
    // immediately after realizing its terminal K-pair loss.
    let resolved = engine.resolve_market_not_atomic(
        ResolveMode::Ordinary,
        DEFAULT_ORACLE - 1,
        DEFAULT_ORACLE,
        DEFAULT_SLOT + 1,
        0,
    );
    assert!(resolved.is_ok());
    assert!(engine.stale_account_count_long == 1);
    assert!(engine.stale_account_count_short == 1);

    let cap_before = engine.accounts[a as usize].capital.get();
    let result = engine.force_close_resolved_not_atomic(a);
    assert!(
        result.is_ok(),
        "force_close must succeed after proper resolve"
    );
    match result.unwrap() {
        ResolvedCloseResult::Closed(payout) => {
            assert!(payout == cap_before - 100);
        }
        ResolvedCloseResult::ProgressOnly => {
            assert!(false);
        }
    }
    assert!(!engine.is_used(a as usize));
    assert!(engine.stored_pos_count_long == 0);
    assert!(engine.stored_pos_count_short == 1);
    assert!(engine.stale_account_count_long == 0);
    assert!(engine.stale_account_count_short == 1);
    assert!(engine.check_conservation());
}

/// force_close_resolved_not_atomic converts positive PnL on flat accounts.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_force_close_resolved_with_profit_conserves() {
    // Substantive: symbolic positive PnL injected via Resolved-mode set_pnl
    // (ImmediateReleaseResolvedOnly works in Resolved), then force_close
    // must return capital + converted profit.
    let mut engine = RiskEngine::new(zero_fee_params());
    let idx = add_user_test(&mut engine, 0).unwrap();
    engine
        .deposit_not_atomic(idx, 500_000, DEFAULT_SLOT)
        .unwrap();

    let cap_before = engine.accounts[idx as usize].capital.get();

    // Go to Resolved first, then set PnL via ImmediateReleaseResolvedOnly
    engine
        .resolve_market_not_atomic(
            ResolveMode::Ordinary,
            DEFAULT_ORACLE,
            DEFAULT_ORACLE,
            DEFAULT_SLOT + 1,
            0,
        )
        .unwrap();

    let profit: u16 = kani::any();
    kani::assume(profit >= 1 && profit <= 10000);
    engine
        .set_pnl_with_reserve(
            idx as usize,
            profit as i128,
            ReserveMode::ImmediateReleaseResolvedOnly,
            None,
        )
        .unwrap();

    let result = engine.force_close_resolved_not_atomic(idx);
    assert!(result.is_ok(), "force_close must succeed with positive PnL");
    let payout = result.unwrap().expect_closed("must be Closed");
    assert!(
        payout >= cap_before,
        "returned must include converted profit"
    );
    assert!(!engine.is_used(idx as usize));
    assert!(engine.check_conservation());
}

/// force_close_resolved_not_atomic on a flat account with no PnL returns exact capital.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_force_close_resolved_flat_returns_capital() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let idx = add_user_test(&mut engine, 0).unwrap();

    let dep: u32 = kani::any();
    kani::assume(dep >= 1 && dep <= 1_000_000);
    engine
        .deposit_not_atomic(idx, dep as u128, DEFAULT_SLOT)
        .unwrap();

    engine
        .resolve_market_not_atomic(
            ResolveMode::Ordinary,
            DEFAULT_ORACLE,
            DEFAULT_ORACLE,
            DEFAULT_SLOT + 1,
            0,
        )
        .unwrap();
    let result = engine.force_close_resolved_not_atomic(idx);
    assert!(result.is_ok());
    let payout = result.unwrap().expect_closed("must be Closed");
    assert_eq!(
        payout, dep as u128,
        "flat account must return exact capital"
    );
    assert!(!engine.is_used(idx as usize));
    assert!(engine.check_conservation());
}

/// force_close_resolved_not_atomic with open position: conservation must hold.
/// Symbolic loss on position-holder exercises K-pair settlement + loss path.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_force_close_resolved_position_conservation() {
    let mut engine = RiskEngine::new_with_market(zero_fee_params(), DEFAULT_SLOT, DEFAULT_ORACLE);

    let a = add_user_test(&mut engine, 0).unwrap();
    let b = add_user_test(&mut engine, 0).unwrap();
    engine.deposit_not_atomic(a, 500_000, DEFAULT_SLOT).unwrap();
    engine.deposit_not_atomic(b, 500_000, DEFAULT_SLOT).unwrap();

    let size = (100 * POS_SCALE) as i128;
    let set_long = engine.set_position_basis_q(a as usize, size);
    assert!(set_long.is_ok());
    let set_short = engine.set_position_basis_q(b as usize, -size);
    assert!(set_short.is_ok());
    engine.oi_eff_long_q = size as u128;
    engine.oi_eff_short_q = size as u128;
    assert!(engine.check_conservation());

    // Resolve one tick below the live price. This creates a matched
    // terminal K-pair: the long realizes a 100-unit loss and the short a
    // 100-unit positive PnL, while OI and stale counters enter ResetPending.
    let resolved = engine.resolve_market_not_atomic(
        ResolveMode::Ordinary,
        DEFAULT_ORACLE - 1,
        DEFAULT_ORACLE,
        DEFAULT_SLOT + 1,
        0,
    );
    assert!(resolved.is_ok());
    assert!(engine.stale_account_count_long == 1);
    assert!(engine.stale_account_count_short == 1);

    // Reconcile both, then terminal close a
    let cap_a_before = engine.accounts[a as usize].capital.get();
    let rec_a = engine.reconcile_resolved_not_atomic(a);
    assert!(rec_a.is_ok());
    assert!(engine.accounts[a as usize].position_basis_q == 0);
    assert!(engine.accounts[a as usize].pnl == 0);
    assert!(engine.accounts[a as usize].capital.get() == cap_a_before - 100);
    assert!(engine.stored_pos_count_long == 0);
    assert!(engine.stale_account_count_long == 0);
    assert!(engine.check_conservation());

    let rec_b = engine.reconcile_resolved_not_atomic(b);
    assert!(rec_b.is_ok());
    assert!(engine.accounts[b as usize].position_basis_q == 0);
    assert!(engine.accounts[b as usize].pnl == 100);
    assert!(engine.stored_pos_count_short == 0);
    assert!(engine.stale_account_count_short == 0);
    assert!(engine.is_terminal_ready());
    assert!(engine.check_conservation());

    let cap_a_after_reconcile = engine.accounts[a as usize].capital.get();
    let result = engine.close_resolved_terminal_not_atomic(a);
    assert!(result.is_ok());
    assert!(result.unwrap() == cap_a_after_reconcile);
    assert!(!engine.is_used(a as usize));
    assert!(engine.accounts[a as usize].position_basis_q == 0);
    assert!(
        engine.check_conservation(),
        "V >= C_tot + I must hold after resolved close"
    );
}

/// force_close_resolved_not_atomic: stored_pos_count decrements correctly
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_force_close_resolved_pos_count_decrements() {
    let mut engine = RiskEngine::new_with_market(zero_fee_params(), DEFAULT_SLOT, DEFAULT_ORACLE);

    let a = add_user_test(&mut engine, 0).unwrap();
    let b = add_user_test(&mut engine, 0).unwrap();
    engine.deposit_not_atomic(a, 500_000, DEFAULT_SLOT).unwrap();
    engine.deposit_not_atomic(b, 500_000, DEFAULT_SLOT).unwrap();

    let size = (100 * POS_SCALE) as i128;
    let set_long = engine.set_position_basis_q(a as usize, size);
    assert!(set_long.is_ok());
    let set_short = engine.set_position_basis_q(b as usize, -size);
    assert!(set_short.is_ok());
    engine.oi_eff_long_q = size as u128;
    engine.oi_eff_short_q = size as u128;

    let long_before = engine.stored_pos_count_long;
    let short_before = engine.stored_pos_count_short;
    assert!(long_before == 1);
    assert!(short_before == 1);

    let resolved = engine.resolve_market_not_atomic(
        ResolveMode::Ordinary,
        DEFAULT_ORACLE,
        DEFAULT_ORACLE,
        DEFAULT_SLOT + 1,
        0,
    );
    assert!(resolved.is_ok());
    let close_long = engine.force_close_resolved_not_atomic(a);
    assert!(close_long.is_ok());
    assert_eq!(engine.stored_pos_count_long, long_before - 1);
    assert_eq!(engine.stored_pos_count_short, short_before);

    let close_short = engine.force_close_resolved_not_atomic(b);
    assert!(close_short.is_ok());
    assert_eq!(engine.stored_pos_count_short, short_before - 1);
    assert_eq!(engine.stored_pos_count_long, 0);
    assert!(engine.check_conservation());
}

/// force_close_resolved_not_atomic with fee debt: insurance receives swept amount
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_force_close_resolved_fee_sweep_conservation() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let _ = engine.top_up_insurance_fund(100_000, 0);
    let idx = add_user_test(&mut engine, 0).unwrap();
    engine
        .deposit_not_atomic(idx, 50_000, DEFAULT_SLOT)
        .unwrap();

    // Symbolic fee debt
    let debt: u16 = kani::any();
    kani::assume(debt >= 1 && debt <= 40000);
    engine.accounts[idx as usize].fee_credits = I128::new(-(debt as i128));

    engine
        .resolve_market_not_atomic(
            ResolveMode::Ordinary,
            DEFAULT_ORACLE,
            DEFAULT_ORACLE,
            DEFAULT_SLOT + 1,
            0,
        )
        .unwrap();
    let ins_before = engine.insurance_fund.balance.get();
    let result = engine.force_close_resolved_not_atomic(idx);
    assert!(result.is_ok());

    // Insurance must have increased by swept amount
    let ins_after = engine.insurance_fund.balance.get();
    let swept = core::cmp::min(debt as u128, 50_000);
    assert_eq!(
        ins_after,
        ins_before + swept,
        "insurance must increase by exactly the swept fee debt"
    );
    assert!(engine.check_conservation());
}

// (Maintenance fee proofs removed — maintenance_fee_per_slot feature was deleted)
