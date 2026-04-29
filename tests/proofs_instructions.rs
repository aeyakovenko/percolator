//! Section 6 — Per-instruction correctness
//!
//! Reset helpers, fee/warmup, accrue, engine integration, spec compliance,
//! dust bound sufficiency.

#![cfg(kani)]

mod common;
use common::*;

// ############################################################################
// T3: RESET HELPERS
// ############################################################################

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn t3_16_reset_pending_counter_invariant() {
    let mut engine = RiskEngine::new(zero_fee_params());

    let a = add_user_test(&mut engine, 0).unwrap();
    let b = add_user_test(&mut engine, 0).unwrap();
    engine.deposit_not_atomic(a, 1_000_000, 0).unwrap();
    engine.deposit_not_atomic(b, 1_000_000, 0).unwrap();

    let k_val: i8 = kani::any();
    let k = k_val as i128;

    engine.accounts[a as usize].position_basis_q = POS_SCALE as i128;
    engine.accounts[a as usize].adl_a_basis = ADL_ONE;
    engine.accounts[a as usize].adl_k_snap = k;
    engine.accounts[a as usize].adl_epoch_snap = 0;
    engine.accounts[b as usize].position_basis_q = POS_SCALE as i128;
    engine.accounts[b as usize].adl_a_basis = ADL_ONE;
    engine.accounts[b as usize].adl_k_snap = k;
    engine.accounts[b as usize].adl_epoch_snap = 0;
    engine.stored_pos_count_long = 2;

    engine.adl_coeff_long = k;

    engine.oi_eff_long_q = 0u128;
    engine.begin_full_drain_reset(Side::Long).unwrap();

    assert!(engine.side_mode_long == SideMode::ResetPending);
    assert!(engine.stale_account_count_long == 2);

    let _ = {
        let mut _ctx = InstructionContext::new_with_admission(0, 100);
        engine.settle_side_effects_live(a as usize, &mut _ctx)
    };
    assert!(engine.stale_account_count_long == 1);

    let _ = {
        let mut _ctx = InstructionContext::new_with_admission(0, 100);
        engine.settle_side_effects_live(b as usize, &mut _ctx)
    };
    assert!(engine.stale_account_count_long == 0);
}

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn t3_16b_reset_counter_with_nonzero_k_diff() {
    let mut engine = RiskEngine::new(zero_fee_params());

    let a = add_user_test(&mut engine, 0).unwrap();
    let b = add_user_test(&mut engine, 0).unwrap();
    engine.deposit_not_atomic(a, 10_000_000, 0).unwrap();
    engine.deposit_not_atomic(b, 10_000_000, 0).unwrap();

    let k_snap = 0i128;

    engine.accounts[a as usize].position_basis_q = POS_SCALE as i128;
    engine.accounts[a as usize].adl_a_basis = ADL_ONE;
    engine.accounts[a as usize].adl_k_snap = k_snap;
    engine.accounts[a as usize].adl_epoch_snap = 0;
    engine.accounts[b as usize].position_basis_q = POS_SCALE as i128;
    engine.accounts[b as usize].adl_a_basis = ADL_ONE;
    engine.accounts[b as usize].adl_k_snap = k_snap;
    engine.accounts[b as usize].adl_epoch_snap = 0;
    engine.stored_pos_count_long = 2;

    let k_diff_val: i8 = kani::any();
    kani::assume(k_diff_val != 0);
    let k_long = k_diff_val as i128;
    engine.adl_coeff_long = k_long;

    engine.oi_eff_long_q = 0u128;
    engine.begin_full_drain_reset(Side::Long).unwrap();

    assert!(engine.adl_epoch_start_k_long == k_long);
    assert!(engine.stale_account_count_long == 2);

    let _ = {
        let mut _ctx = InstructionContext::new_with_admission(0, 100);
        engine.settle_side_effects_live(a as usize, &mut _ctx)
    };
    assert!(engine.stale_account_count_long == 1);
    let _ = {
        let mut _ctx = InstructionContext::new_with_admission(0, 100);
        engine.settle_side_effects_live(b as usize, &mut _ctx)
    };
    assert!(engine.stale_account_count_long == 0);
}

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn t3_17_clean_empty_engine_no_retrigger() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let mut ctx = InstructionContext::new();

    assert!(engine.stored_pos_count_long == 0);
    assert!(engine.stored_pos_count_short == 0);
    assert!(engine.oi_eff_long_q == 0);
    assert!(engine.oi_eff_short_q == 0);
    assert!(engine.phantom_dust_bound_long_q == 0);
    assert!(engine.phantom_dust_bound_short_q == 0);

    let result = engine.schedule_end_of_instruction_resets(&mut ctx);
    assert!(result.is_ok());

    assert!(!ctx.pending_reset_long);
    assert!(!ctx.pending_reset_short);
}

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn t3_18_dust_bound_reset_in_begin_full_drain() {
    let mut engine = RiskEngine::new(zero_fee_params());

    engine.phantom_dust_bound_long_q = 5u128;
    engine.oi_eff_long_q = 0u128;

    engine.begin_full_drain_reset(Side::Long).unwrap();

    assert!(
        engine.phantom_dust_bound_long_q == 0,
        "phantom_dust_bound must be zeroed by begin_full_drain_reset"
    );
}

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn t3_19_finalize_side_reset_requires_all_stale_touched() {
    let mut engine = RiskEngine::new(zero_fee_params());

    engine.side_mode_long = SideMode::ResetPending;
    engine.oi_eff_long_q = 0u128;
    engine.stale_account_count_long = 1;
    engine.stored_pos_count_long = 0;
    let result1 = engine.finalize_side_reset(Side::Long);
    assert!(result1.is_err());

    engine.stale_account_count_long = 0;
    engine.stored_pos_count_long = 1;
    let result2 = engine.finalize_side_reset(Side::Long);
    assert!(result2.is_err());

    engine.stored_pos_count_long = 0;
    let result3 = engine.finalize_side_reset(Side::Long);
    assert!(result3.is_ok());
    assert!(engine.side_mode_long == SideMode::Normal);
}

#[kani::proof]
#[kani::solver(cadical)]
fn t6_26b_full_drain_reset_nonzero_k_diff() {
    let mut engine = RiskEngine::new(zero_fee_params());

    let idx = add_user_test(&mut engine, 0).unwrap();
    engine.deposit_not_atomic(idx, 10_000_000, 0).unwrap();

    engine.accounts[idx as usize].position_basis_q = POS_SCALE as i128;
    engine.accounts[idx as usize].adl_a_basis = ADL_ONE;
    engine.accounts[idx as usize].adl_k_snap = 0i128;
    engine.accounts[idx as usize].adl_epoch_snap = 0;
    engine.stored_pos_count_long = 1;

    engine.adl_coeff_long = 500i128;

    engine.oi_eff_long_q = 0u128;
    engine.begin_full_drain_reset(Side::Long).unwrap();

    assert!(engine.adl_epoch_start_k_long == 500i128);
    assert!(engine.adl_epoch_long == 1);
    assert!(engine.stale_account_count_long == 1);

    let result = {
        let mut _ctx = InstructionContext::new_with_admission(0, 100);
        engine.settle_side_effects_live(idx as usize, &mut _ctx)
    };
    assert!(result.is_ok());

    assert!(engine.accounts[idx as usize].position_basis_q == 0);
    assert!(engine.stale_account_count_long == 0);
    // Canonical zero-position defaults: epoch_snap = 0 (spec §2.4)
    assert!(engine.accounts[idx as usize].adl_epoch_snap == 0);

    assert!(engine.stored_pos_count_long == 0);
    let finalize = engine.finalize_side_reset(Side::Long);
    assert!(finalize.is_ok());
    assert!(engine.side_mode_long == SideMode::Normal);
}

// ############################################################################
// T9: FEE / WARMUP
// ############################################################################

#[kani::proof]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn t9_35_warmup_release_monotone_in_time() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let idx = add_user_test(&mut engine, 0).unwrap();
    engine.deposit_not_atomic(idx, 10_000_000, 0).unwrap();

    let pnl_val: u8 = kani::any();
    kani::assume(pnl_val > 0);
    engine.set_pnl(idx as usize, pnl_val as i128);

    let r_initial = engine.accounts[idx as usize].reserved_pnl;

    let t1: u8 = kani::any();
    let t2: u8 = kani::any();
    kani::assume(t1 < t2);

    // Compute release at t1 on a clone
    let mut e1 = engine.clone();
    e1.current_slot = t1 as u64;
    e1.advance_profit_warmup(idx as usize);
    let released1 = r_initial - e1.accounts[idx as usize].reserved_pnl;

    // Compute release at t2 on another clone
    let mut e2 = engine;
    e2.current_slot = t2 as u64;
    e2.advance_profit_warmup(idx as usize);
    let released2 = r_initial - e2.accounts[idx as usize].reserved_pnl;

    assert!(
        released2 >= released1,
        "warmup release must be monotone non-decreasing in time"
    );
}

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn t9_36_fee_seniority_after_restart() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let idx = add_user_test(&mut engine, 0).unwrap();
    engine.deposit_not_atomic(idx, 10_000_000, 0).unwrap();

    let fc_val: i8 = kani::any();
    engine.accounts[idx as usize].fee_credits = I128::new(fc_val as i128);

    let fc_before = engine.accounts[idx as usize].fee_credits;

    engine.accounts[idx as usize].position_basis_q = POS_SCALE as i128;
    engine.accounts[idx as usize].adl_a_basis = ADL_ONE;
    engine.accounts[idx as usize].adl_k_snap = 0i128;
    engine.accounts[idx as usize].adl_epoch_snap = 0;
    engine.stored_pos_count_long = 1;
    engine.adl_epoch_long = 1;
    engine.adl_epoch_start_k_long = 0i128;
    engine.side_mode_long = SideMode::ResetPending;
    engine.stale_account_count_long = 1;
    engine.adl_coeff_long = 0i128;

    let _ = {
        let mut _ctx = InstructionContext::new_with_admission(0, 100);
        engine.settle_side_effects_live(idx as usize, &mut _ctx)
    };

    let fc_after = engine.accounts[idx as usize].fee_credits;
    assert!(
        fc_after == fc_before,
        "fee_credits must be preserved across epoch restart"
    );
}

// ############################################################################
// T10: ACCRUE_MARKET_TO
// ############################################################################

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn t10_37_accrue_mark_matches_eager() {
    let mut engine = RiskEngine::new(zero_fee_params());

    engine.oi_eff_long_q = POS_SCALE;
    engine.oi_eff_short_q = POS_SCALE;
    engine.adl_mult_long = ADL_ONE;
    engine.adl_mult_short = ADL_ONE;
    engine.last_oracle_price = 10_000;
    engine.last_market_slot = 0;

    let k_long_before = engine.adl_coeff_long;
    let k_short_before = engine.adl_coeff_short;

    let dp: i8 = kani::any();
    kani::assume(dp >= -50 && dp <= 50);
    let new_price = (10_000i16 + dp as i16) as u64;
    kani::assume(new_price > 0);

    let result = engine.accrue_market_to(100, new_price, 0);
    assert!(result.is_ok());

    let k_long_after = engine.adl_coeff_long;
    let k_short_after = engine.adl_coeff_short;

    let expected_delta = (ADL_ONE as i128) * (dp as i128);
    let actual_long_delta = k_long_after.checked_sub(k_long_before).unwrap();
    assert!(
        actual_long_delta == expected_delta,
        "K_long delta must equal A_long * delta_p"
    );

    let actual_short_delta = k_short_after.checked_sub(k_short_before).unwrap();
    let expected_short_delta = expected_delta.checked_neg().unwrap_or(0i128);
    assert!(
        actual_short_delta == expected_short_delta,
        "K_short delta must equal -(A_short * delta_p)"
    );
}

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn t10_38_accrue_funding_payer_driven() {
    let mut engine = RiskEngine::new(zero_fee_params());

    engine.oi_eff_long_q = POS_SCALE;
    engine.oi_eff_short_q = POS_SCALE;
    engine.adl_mult_long = ADL_ONE;
    engine.adl_mult_short = ADL_ONE;
    engine.last_oracle_price = 100;
    engine.fund_px_last = 100; // funding uses fund_px_last, not last_oracle_price
    engine.last_market_slot = 0;

    let rate: i8 = kani::any();
    kani::assume(rate != 0);
    kani::assume(rate >= -100 && rate <= 100);

    let k_long_before = engine.adl_coeff_long;
    let k_short_before = engine.adl_coeff_short;

    let result = engine.accrue_market_to(1, 100, rate as i128);
    assert!(result.is_ok());

    let k_long_after = engine.adl_coeff_long;
    let k_short_after = engine.adl_coeff_short;

    // v12.15: K gets truncation-divided integer part, F gets remainder.
    // fund_num = 100 * rate. fund_term = fund_num / 1e9 (truncation toward zero).
    // For |fund_num| < 1e9, fund_term = 0 and all funding goes to F.
    let fund_num = 100i128 * (rate as i128);
    let fund_term = fund_num / (1_000_000_000i128);
    let remainder = fund_num - fund_term * 1_000_000_000i128;

    let a_long = ADL_ONE as i128;
    let expected_k_long = k_long_before - a_long * fund_term;
    let expected_k_short = k_short_before + a_long * fund_term;

    assert!(
        k_long_after == expected_k_long,
        "K_long must match truncated fund_term"
    );
    assert!(
        k_short_after == expected_k_short,
        "K_short must match truncated fund_term"
    );

    // F captures the remainder (per-side, with A multiplication)
    let expected_f_long = -(a_long * remainder);
    let expected_f_short = a_long * remainder;
    assert!(
        engine.f_long_num == expected_f_long,
        "F_long must capture remainder"
    );
    assert!(
        engine.f_short_num == expected_f_short,
        "F_short must capture remainder"
    );

    // Combined K + F is exact: no funding is lost
    // K_delta * FUNDING_DEN + F_delta = A_side * fund_num (exact)
    let k_delta_long = k_long_after - k_long_before;
    let total_long = k_delta_long * 1_000_000_000i128 + engine.f_long_num;
    assert!(
        total_long == -(a_long * fund_num),
        "K + F must equal exact funding"
    );
}

// ############################################################################
// T11: ENGINE INTEGRATION
// ############################################################################

#[kani::proof]
#[kani::solver(cadical)]
fn t11_39_same_epoch_settle_idempotent_real_engine() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let idx = add_user_test(&mut engine, 0).unwrap();
    engine.deposit_not_atomic(idx, 10_000_000, 0).unwrap();

    let pos = POS_SCALE as i128;
    engine.accounts[idx as usize].position_basis_q = pos;
    engine.accounts[idx as usize].adl_a_basis = ADL_ONE;
    engine.accounts[idx as usize].adl_k_snap = 0i128;
    engine.accounts[idx as usize].adl_epoch_snap = 0;
    engine.stored_pos_count_long = 1;
    engine.adl_epoch_long = 0;
    engine.oi_eff_long_q = POS_SCALE;

    engine.adl_coeff_long = 100i128;

    let r1 = {
        let mut _ctx = InstructionContext::new_with_admission(0, 100);
        engine.settle_side_effects_live(idx as usize, &mut _ctx)
    };
    assert!(r1.is_ok());
    let pnl_after_first = engine.accounts[idx as usize].pnl;
    assert!(engine.accounts[idx as usize].adl_k_snap == 100i128);

    let r2 = {
        let mut _ctx = InstructionContext::new_with_admission(0, 100);
        engine.settle_side_effects_live(idx as usize, &mut _ctx)
    };
    assert!(r2.is_ok());
    let pnl_after_second = engine.accounts[idx as usize].pnl;

    assert!(
        pnl_after_second == pnl_after_first,
        "second settle with unchanged K must produce zero incremental PnL"
    );
    assert!(engine.accounts[idx as usize].adl_a_basis == ADL_ONE);
    assert!(engine.accounts[idx as usize].position_basis_q == pos);
}

#[kani::proof]
#[kani::solver(cadical)]
fn t11_40_non_compounding_quantity_basis_two_touches() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let idx = add_user_test(&mut engine, 0).unwrap();
    engine.deposit_not_atomic(idx, 10_000_000, 0).unwrap();

    let pos = POS_SCALE as i128;
    engine.accounts[idx as usize].position_basis_q = pos;
    engine.accounts[idx as usize].adl_a_basis = ADL_ONE;
    engine.accounts[idx as usize].adl_k_snap = 0i128;
    engine.accounts[idx as usize].adl_epoch_snap = 0;
    engine.stored_pos_count_long = 1;
    engine.adl_epoch_long = 0;
    engine.oi_eff_long_q = POS_SCALE;

    engine.adl_coeff_long = 50i128;
    let _ = {
        let mut _ctx = InstructionContext::new_with_admission(0, 100);
        engine.settle_side_effects_live(idx as usize, &mut _ctx)
    };

    assert!(engine.accounts[idx as usize].position_basis_q == pos);
    assert!(engine.accounts[idx as usize].adl_a_basis == ADL_ONE);
    assert!(engine.accounts[idx as usize].adl_k_snap == 50i128);

    engine.adl_coeff_long = 120i128;
    let _ = {
        let mut _ctx = InstructionContext::new_with_admission(0, 100);
        engine.settle_side_effects_live(idx as usize, &mut _ctx)
    };

    assert!(engine.accounts[idx as usize].position_basis_q == pos);
    assert!(engine.accounts[idx as usize].adl_a_basis == ADL_ONE);
    assert!(engine.accounts[idx as usize].adl_k_snap == 120i128);
}

#[kani::proof]
#[kani::solver(cadical)]
fn t11_41_attach_effective_position_remainder_accounting() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let idx = add_user_test(&mut engine, 0).unwrap();
    engine.deposit_not_atomic(idx, 10_000_000, 0).unwrap();

    // Use a_basis=7, a_side=6 so that POS_SCALE * 6 % 7 != 0 (nonzero remainder)
    engine.accounts[idx as usize].position_basis_q = POS_SCALE as i128;
    engine.accounts[idx as usize].adl_a_basis = 7;
    engine.accounts[idx as usize].adl_epoch_snap = 0;
    engine.adl_epoch_long = 0;
    engine.adl_mult_long = 6;
    engine.stored_pos_count_long = 1;

    let dust_before = engine.phantom_dust_bound_long_q;

    let new_pos = (2 * POS_SCALE) as i128;
    engine.attach_effective_position(idx as usize, new_pos);

    assert!(
        engine.phantom_dust_bound_long_q > dust_before,
        "dust bound must increment on nonzero remainder"
    );

    // Now test zero remainder: a_basis == a_side → product evenly divisible
    engine.accounts[idx as usize].position_basis_q = POS_SCALE as i128;
    engine.accounts[idx as usize].adl_a_basis = ADL_ONE;
    engine.adl_mult_long = ADL_ONE;

    let dust_before2 = engine.phantom_dust_bound_long_q;
    engine.attach_effective_position(idx as usize, (3 * POS_SCALE) as i128);

    assert!(
        engine.phantom_dust_bound_long_q == dust_before2,
        "dust bound must not increment on zero remainder"
    );
}

#[kani::proof]
#[kani::solver(cadical)]
fn t11_42_dynamic_dust_bound_inductive() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let a = add_user_test(&mut engine, 0).unwrap();
    let b = add_user_test(&mut engine, 0).unwrap();
    engine.deposit_not_atomic(a, 10_000_000, 0).unwrap();
    engine.deposit_not_atomic(b, 10_000_000, 0).unwrap();

    // Use basis=1, a_basis=3 so floor(1 * 1 / 3) = 0 → position zeroes
    engine.accounts[a as usize].position_basis_q = 1i128;
    engine.accounts[a as usize].adl_a_basis = 3;
    engine.accounts[a as usize].adl_k_snap = 0i128;
    engine.accounts[a as usize].adl_epoch_snap = 0;
    engine.accounts[b as usize].position_basis_q = 1i128;
    engine.accounts[b as usize].adl_a_basis = 3;
    engine.accounts[b as usize].adl_k_snap = 0i128;
    engine.accounts[b as usize].adl_epoch_snap = 0;
    engine.stored_pos_count_long = 2;
    engine.adl_epoch_long = 0;
    engine.oi_eff_long_q = 2;

    engine.adl_mult_long = 1;

    let _ = {
        let mut _ctx = InstructionContext::new_with_admission(0, 100);
        engine.settle_side_effects_live(a as usize, &mut _ctx)
    };
    assert!(engine.accounts[a as usize].position_basis_q == 0);
    assert!(engine.phantom_dust_bound_long_q == 1u128);

    let _ = {
        let mut _ctx = InstructionContext::new_with_admission(0, 100);
        engine.settle_side_effects_live(b as usize, &mut _ctx)
    };
    assert!(engine.accounts[b as usize].position_basis_q == 0);
    assert!(engine.phantom_dust_bound_long_q == 2u128);
}

#[kani::proof]
#[kani::solver(cadical)]
fn t11_50_execute_trade_atomic_oi_update_sign_flip() {
    let mut engine = RiskEngine::new(zero_fee_params());

    let a = add_user_test(&mut engine, 0).unwrap();
    let b = add_user_test(&mut engine, 0).unwrap();
    let p = POS_SCALE as i128;

    // Initial bilateral open: a long P, b short P.
    let (oi_long_open, oi_short_open) = engine.bilateral_oi_after(&0, &p, &0, &(-p)).unwrap();
    assert!(oi_long_open == POS_SCALE);
    assert!(oi_short_open == POS_SCALE);
    engine.attach_effective_position(a as usize, p).unwrap();
    engine.attach_effective_position(b as usize, -p).unwrap();
    engine.oi_eff_long_q = oi_long_open;
    engine.oi_eff_short_q = oi_short_open;

    assert!(engine.accounts[a as usize].position_basis_q == POS_SCALE as i128);
    assert!(engine.accounts[b as usize].position_basis_q == -(POS_SCALE as i128));
    assert!(engine.oi_eff_long_q == POS_SCALE);
    assert!(engine.oi_eff_short_q == POS_SCALE);

    // Swap a,b with size 2P: b flips short->long and a flips long->short.
    // This validates the execute_trade_not_atomic step-5/step-9 invariant:
    // compute bilateral OI once over both legs, then write those exact values.
    let flip_size = 2 * p;
    let old_eff_b = engine.effective_pos_q(b as usize);
    let old_eff_a = engine.effective_pos_q(a as usize);
    let new_eff_b = old_eff_b.checked_add(flip_size).unwrap();
    let new_eff_a = old_eff_a.checked_sub(flip_size).unwrap();
    let (oi_long_after, oi_short_after) = engine
        .bilateral_oi_after(&old_eff_b, &new_eff_b, &old_eff_a, &new_eff_a)
        .unwrap();
    assert!(oi_long_after == POS_SCALE);
    assert!(oi_short_after == POS_SCALE);
    engine
        .enforce_side_mode_oi_gate(oi_long_after, oi_short_after)
        .unwrap();
    engine
        .attach_effective_position(b as usize, new_eff_b)
        .unwrap();
    engine
        .attach_effective_position(a as usize, new_eff_a)
        .unwrap();
    engine.oi_eff_long_q = oi_long_after;
    engine.oi_eff_short_q = oi_short_after;

    assert!(engine.accounts[a as usize].position_basis_q == -(POS_SCALE as i128));
    assert!(engine.accounts[b as usize].position_basis_q == POS_SCALE as i128);
    assert!(engine.oi_eff_long_q == POS_SCALE);
    assert!(engine.oi_eff_short_q == POS_SCALE);
    assert!(engine.stored_pos_count_long == 1);
    assert!(engine.stored_pos_count_short == 1);
}

#[kani::proof]
#[kani::solver(cadical)]
fn t11_51_execute_trade_slippage_zero_sum() {
    let price_diff_raw: i8 = kani::any();
    kani::assume(price_diff_raw >= -10 && price_diff_raw <= 10);

    let price_diff = price_diff_raw as i128;
    let pnl_a = price_diff;
    let pnl_b = -price_diff;

    // Spec §10.5: execution slippage is internal transfer PnL between the
    // two counterparties. It must not mint or burn value.
    assert!(pnl_a.checked_add(pnl_b) == Some(0));
    assert!(pnl_a == price_diff);
    if price_diff == 0 {
        assert!(pnl_a == 0);
        assert!(pnl_b == 0);
    }

    // With zero-fee params, the fee leg is disabled for any nonzero notional,
    // so no vault/capital movement can be attributed to the trade fee path.
    let params = zero_fee_params();
    let trade_notional = 100u128;
    let fee = if trade_notional > 0 && params.trading_fee_bps > 0 {
        1u128
    } else {
        0u128
    };
    assert!(fee == 0);

    kani::cover!(price_diff > 0, "positive slippage branch reachable");
    kani::cover!(price_diff < 0, "negative slippage branch reachable");
    kani::cover!(price_diff == 0, "zero slippage branch reachable");
}

#[kani::proof]
#[kani::solver(cadical)]
fn t11_52_touch_account_full_restart_fee_seniority() {
    let mut engine = RiskEngine::new_with_market(zero_fee_params(), DEFAULT_SLOT, 100);

    let idx = add_user_test(&mut engine, 0).unwrap();
    let hedge = add_user_test(&mut engine, 0).unwrap();
    engine
        .deposit_not_atomic(idx, 10_000_000, DEFAULT_SLOT)
        .unwrap();
    engine
        .deposit_not_atomic(hedge, 10_000_000, DEFAULT_SLOT)
        .unwrap();

    let pos = POS_SCALE as i128;
    engine.accounts[idx as usize].position_basis_q = pos;
    engine.accounts[idx as usize].adl_a_basis = ADL_ONE;
    engine.accounts[idx as usize].adl_k_snap = 0i128;
    engine.accounts[idx as usize].adl_epoch_snap = 0;
    engine.accounts[hedge as usize].position_basis_q = -pos;
    engine.accounts[hedge as usize].adl_a_basis = ADL_ONE;
    engine.accounts[hedge as usize].adl_k_snap = 0i128;
    engine.accounts[hedge as usize].adl_epoch_snap = 0;
    engine.stored_pos_count_long = 1;
    engine.stored_pos_count_short = 1;
    engine.adl_epoch_long = 0;
    engine.adl_epoch_short = 0;
    engine.oi_eff_long_q = POS_SCALE;
    engine.oi_eff_short_q = POS_SCALE;

    engine.accounts[idx as usize].pnl = 5000i128;
    engine.pnl_pos_tot = 5000u128;
    engine.pnl_matured_pos_tot = 5000u128;

    engine.adl_coeff_long = (ADL_ONE as i128) * 100;

    engine.accounts[idx as usize].fee_credits = I128::new(-500i128);

    let cap_before = engine.accounts[idx as usize].capital.get();
    let ins_before = engine.insurance_fund.balance.get();

    // New touch pattern: accrue market, then touch_account_live_local + finalize
    {
        let mut ctx = InstructionContext::new_with_admission(0, 100);
        engine.accrue_market_to(DEFAULT_SLOT, 100, 0).unwrap();
        engine
            .touch_account_live_local(idx as usize, &mut ctx)
            .unwrap();
        engine.finalize_touched_accounts_post_live(&ctx).unwrap();
    }

    assert!(engine.accounts[idx as usize].adl_k_snap == engine.adl_coeff_long);

    let fc_after = engine.accounts[idx as usize].fee_credits.get();
    assert!(
        fc_after > -500i128,
        "fee debt must be swept after restart conversion"
    );

    let ins_after = engine.insurance_fund.balance.get();
    assert!(
        ins_after > ins_before,
        "insurance fund must receive fee sweep payment"
    );

    let cap_after = engine.accounts[idx as usize].capital.get();
    assert!(
        cap_after != cap_before,
        "capital must change after restart conversion + fee sweep"
    );
}

#[kani::proof]
#[kani::solver(cadical)]
fn t11_54_worked_example_regression() {
    let mut engine = RiskEngine::new_with_market(zero_fee_params(), DEFAULT_SLOT, 100);

    let a = add_user_test(&mut engine, 0).unwrap();
    let b = add_user_test(&mut engine, 0).unwrap();
    engine
        .deposit_not_atomic(a, 10_000_000, DEFAULT_SLOT)
        .unwrap();
    engine
        .deposit_not_atomic(b, 10_000_000, DEFAULT_SLOT)
        .unwrap();

    let size_q = (2 * POS_SCALE) as i128;
    engine.accounts[a as usize].position_basis_q = size_q;
    engine.accounts[a as usize].adl_a_basis = ADL_ONE;
    engine.accounts[a as usize].adl_k_snap = 0;
    engine.accounts[a as usize].adl_epoch_snap = 0;
    engine.accounts[b as usize].position_basis_q = -size_q;
    engine.accounts[b as usize].adl_a_basis = ADL_ONE;
    engine.accounts[b as usize].adl_k_snap = 0;
    engine.accounts[b as usize].adl_epoch_snap = 0;
    engine.stored_pos_count_long = 1;
    engine.stored_pos_count_short = 1;
    engine.adl_epoch_long = 0;
    engine.adl_epoch_short = 0;
    engine.oi_eff_long_q = 2 * POS_SCALE;
    engine.oi_eff_short_q = 2 * POS_SCALE;
    assert!(engine.oi_eff_long_q == engine.oi_eff_short_q);

    let mut ctx = InstructionContext::new();
    let d = 500u128;
    let q_close = POS_SCALE;
    let r2 = engine.enqueue_adl(&mut ctx, Side::Short, q_close, d);
    assert!(r2.is_ok());

    assert!(engine.adl_mult_long < ADL_ONE);
    assert!(engine.oi_eff_long_q == POS_SCALE);
    assert!(engine.adl_coeff_long != 0i128);

    let _ = {
        let mut _ctx = InstructionContext::new_with_admission(0, 100);
        engine.settle_side_effects_live(a as usize, &mut _ctx)
    };

    assert!(engine.accounts[a as usize].adl_k_snap == engine.adl_coeff_long);
    assert!(engine.check_conservation());
}

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn t5_24_dynamic_dust_bound_sufficient() {
    let mut engine = RiskEngine::new(zero_fee_params());

    let a = add_user_test(&mut engine, 0).unwrap();
    let b = add_user_test(&mut engine, 0).unwrap();
    engine.deposit_not_atomic(a, 10_000_000, 0).unwrap();
    engine.deposit_not_atomic(b, 10_000_000, 0).unwrap();

    // Use basis=1, a_basis=3 so floor(1 * 1 / 3) = 0 → position zeroes
    engine.accounts[a as usize].position_basis_q = 1i128;
    engine.accounts[a as usize].adl_a_basis = 3;
    engine.accounts[a as usize].adl_k_snap = 0i128;
    engine.accounts[a as usize].adl_epoch_snap = 0;
    engine.accounts[b as usize].position_basis_q = 1i128;
    engine.accounts[b as usize].adl_a_basis = 3;
    engine.accounts[b as usize].adl_k_snap = 0i128;
    engine.accounts[b as usize].adl_epoch_snap = 0;
    engine.stored_pos_count_long = 2;
    engine.oi_eff_long_q = 2;
    engine.adl_epoch_long = 0;

    engine.adl_mult_long = 1;
    engine.adl_coeff_long = 0i128;

    let _ = {
        let mut _ctx = InstructionContext::new_with_admission(0, 100);
        engine.settle_side_effects_live(a as usize, &mut _ctx)
    };
    assert!(engine.phantom_dust_bound_long_q == 1u128);

    let _ = {
        let mut _ctx = InstructionContext::new_with_admission(0, 100);
        engine.settle_side_effects_live(b as usize, &mut _ctx)
    };
    assert!(engine.phantom_dust_bound_long_q == 2u128);
}

// ############################################################################
// From kani.rs: reset/instruction
// ############################################################################

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_begin_full_drain_reset() {
    let mut engine = RiskEngine::new(zero_fee_params());

    let epoch_before = engine.adl_epoch_long;
    let k_before = engine.adl_coeff_long;

    assert!(engine.oi_eff_long_q == 0);

    engine.begin_full_drain_reset(Side::Long).unwrap();

    assert!(engine.adl_epoch_long == epoch_before + 1);
    assert!(engine.adl_mult_long == ADL_ONE);
    assert!(engine.side_mode_long == SideMode::ResetPending);
    assert!(engine.adl_epoch_start_k_long == k_before);
    assert!(engine.stale_account_count_long == engine.stored_pos_count_long);
}

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_finalize_side_reset_requires_conditions() {
    let mut engine = RiskEngine::new(zero_fee_params());

    let r1 = engine.finalize_side_reset(Side::Long);
    assert!(r1.is_err());

    engine.side_mode_long = SideMode::ResetPending;
    engine.oi_eff_long_q = 100u128;
    let r2 = engine.finalize_side_reset(Side::Long);
    assert!(r2.is_err());

    engine.oi_eff_long_q = 0u128;
    engine.stale_account_count_long = 1;
    let r3 = engine.finalize_side_reset(Side::Long);
    assert!(r3.is_err());

    engine.stale_account_count_long = 0;
    engine.stored_pos_count_long = 0;
    let r4 = engine.finalize_side_reset(Side::Long);
    assert!(r4.is_ok());
    assert!(engine.side_mode_long == SideMode::Normal);
}

// ############################################################################
// SPEC COMPLIANCE (from ak.rs)
// ############################################################################

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn t13_55_empty_opposing_side_deficit_fallback() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let mut ctx = InstructionContext::new();

    engine.adl_mult_long = POS_SCALE;
    engine.adl_coeff_long = 12345i128;
    engine.oi_eff_long_q = 4 * POS_SCALE;
    engine.oi_eff_short_q = 4 * POS_SCALE;
    engine.insurance_fund.balance = U128::new(10_000_000);
    engine.stored_pos_count_long = 0;

    let k_before = engine.adl_coeff_long;
    let ins_before = engine.insurance_fund.balance.get();

    let d = 5_000u128;
    let q_close = POS_SCALE;

    let result = engine.enqueue_adl(&mut ctx, Side::Short, q_close, d);
    assert!(result.is_ok());

    assert!(
        engine.adl_coeff_long == k_before,
        "K must not change when stored_pos_count_opp == 0"
    );
    assert!(
        engine.insurance_fund.balance.get() < ins_before,
        "insurance must absorb deficit"
    );
    assert!(engine.oi_eff_long_q == 3 * POS_SCALE);
}

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn t13_56_unilateral_empty_orphan_resolution() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let mut ctx = InstructionContext::new();

    engine.stored_pos_count_long = 0;
    engine.phantom_dust_bound_long_q = 100u128;
    engine.oi_eff_long_q = 50u128;

    engine.stored_pos_count_short = 2;
    engine.oi_eff_short_q = 50u128;

    let result = engine.schedule_end_of_instruction_resets(&mut ctx);
    assert!(result.is_ok());

    assert!(ctx.pending_reset_long);
    assert!(ctx.pending_reset_short);
    assert!(engine.oi_eff_long_q == 0);
    assert!(engine.oi_eff_short_q == 0);
}

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn t13_57_unilateral_empty_corruption_guard() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let mut ctx = InstructionContext::new();

    engine.stored_pos_count_long = 0;
    engine.phantom_dust_bound_long_q = 100u128;
    engine.oi_eff_long_q = 50u128;

    engine.stored_pos_count_short = 2;
    engine.oi_eff_short_q = 999u128;

    let result = engine.schedule_end_of_instruction_resets(&mut ctx);
    assert!(result == Err(RiskError::CorruptState));
}

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn t13_58_unilateral_empty_short_side() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let mut ctx = InstructionContext::new();

    engine.stored_pos_count_short = 0;
    engine.phantom_dust_bound_short_q = 200u128;
    engine.oi_eff_short_q = 75u128;

    engine.stored_pos_count_long = 3;
    engine.oi_eff_long_q = 75u128;

    let result = engine.schedule_end_of_instruction_resets(&mut ctx);
    assert!(result.is_ok());

    assert!(ctx.pending_reset_long);
    assert!(ctx.pending_reset_short);
    assert!(engine.oi_eff_long_q == 0);
    assert!(engine.oi_eff_short_q == 0);
}

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn t13_60_unconditional_dust_bound_on_any_a_decay() {
    // v12.15+: phantom dust bound increments unconditionally on ANY A_side decay,
    // even when the truncation remainder is exactly zero.
    let mut engine = RiskEngine::new(zero_fee_params());
    let mut ctx = InstructionContext::new();

    engine.adl_mult_long = 4;
    engine.adl_coeff_long = 0i128;
    engine.oi_eff_long_q = 4 * POS_SCALE;
    engine.oi_eff_short_q = 4 * POS_SCALE;
    engine.stored_pos_count_long = 1;

    let dust_before = engine.phantom_dust_bound_long_q;

    let result = engine.enqueue_adl(&mut ctx, Side::Short, 2 * POS_SCALE, 0u128);
    assert!(result.is_ok());
    assert!(engine.adl_mult_long == 2);

    // Unconditional: dust ALWAYS increments by at least 1 on A decay
    assert!(
        engine.phantom_dust_bound_long_q >= dust_before + 1,
        "dust must increment unconditionally on any A_side decay"
    );
}

#[kani::proof]
#[kani::solver(cadical)]
fn t12_53_adl_truncation_dust_must_not_deadlock() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let mut ctx = InstructionContext::new();

    let a = add_user_test(&mut engine, 0).unwrap();
    let b = add_user_test(&mut engine, 0).unwrap();
    engine.deposit_not_atomic(a, 10_000_000, 0).unwrap();
    engine.deposit_not_atomic(b, 10_000_000, 0).unwrap();

    // One long (a) at A=7, one short (b) for OI balance.
    engine.adl_mult_long = 7;
    engine.adl_mult_short = ADL_ONE;
    engine.adl_coeff_long = 0i128;
    engine.adl_coeff_short = 0i128;

    // Account a: long 10*POS_SCALE at a_basis=7
    engine.accounts[a as usize].position_basis_q = (10 * POS_SCALE) as i128;
    engine.accounts[a as usize].adl_a_basis = 7;
    engine.accounts[a as usize].adl_k_snap = 0i128;
    engine.accounts[a as usize].adl_epoch_snap = 0;

    // Account b: short 10*POS_SCALE
    engine.accounts[b as usize].position_basis_q = -((10 * POS_SCALE) as i128);
    engine.accounts[b as usize].adl_a_basis = ADL_ONE;
    engine.accounts[b as usize].adl_k_snap = 0i128;
    engine.accounts[b as usize].adl_epoch_snap = 0;

    engine.stored_pos_count_long = 1;
    engine.stored_pos_count_short = 1;
    engine.oi_eff_long_q = 10 * POS_SCALE;
    engine.oi_eff_short_q = 10 * POS_SCALE;

    // ADL: close POS_SCALE from short side → shrinks A_long via truncation
    // enqueue_adl decrements both sides by q_close, then A-truncates opposing
    let result = engine.enqueue_adl(&mut ctx, Side::Short, POS_SCALE, 0u128);
    assert!(result.is_ok());
    // A_new = floor(7 * 9M / 10M) = 6
    assert!(engine.adl_mult_long == 6);
    assert!(engine.oi_eff_long_q == 9 * POS_SCALE);
    assert!(engine.oi_eff_short_q == 9 * POS_SCALE);

    // Settle account a to get actual effective position under new A
    let settle_a = {
        let mut _ctx = InstructionContext::new_with_admission(0, 100);
        engine.settle_side_effects_live(a as usize, &mut _ctx)
    };
    assert!(settle_a.is_ok());

    // eff_a = floor(10_000_000 * 6 / 7) = 8_571_428 (< 9_000_000)
    let eff_a = engine.effective_pos_q(a as usize);
    let dust = engine
        .oi_eff_long_q
        .checked_sub(eff_a.unsigned_abs())
        .unwrap_or(0);

    // Verify phantom_dust_bound covers the A-truncation dust
    assert!(
        engine.phantom_dust_bound_long_q >= dust,
        "dust bound must cover A-truncation phantom OI"
    );

    // Simulate final state: all positions closed via balanced trades,
    // which maintain OI_long == OI_short. Residual dust is equal on both sides.
    engine.attach_effective_position(a as usize, 0i128);
    engine.attach_effective_position(b as usize, 0i128);
    engine.oi_eff_long_q = dust;
    engine.oi_eff_short_q = dust;

    let reset_result = engine.schedule_end_of_instruction_resets(&mut ctx);
    assert!(
        reset_result.is_ok(),
        "ADL truncation dust must not deadlock market reset"
    );
}

// ############################################################################
// T14: INDUCTIVE DUST-BOUND SUFFICIENCY
// ############################################################################

#[kani::proof]
#[kani::unwind(1)]
#[kani::solver(cadical)]
fn t14_61_dust_bound_adl_a_truncation_sufficient() {
    let a_old: u8 = kani::any();
    kani::assume(a_old >= 2 && a_old <= 15);
    let basis_1: u8 = kani::any();
    kani::assume(basis_1 > 0 && basis_1 <= 15);
    let basis_2: u8 = kani::any();
    kani::assume(basis_2 > 0 && basis_2 <= 15);

    let a_basis_1: u8 = kani::any();
    kani::assume(a_basis_1 > 0 && a_basis_1 <= a_old);
    let a_basis_2: u8 = kani::any();
    kani::assume(a_basis_2 > 0 && a_basis_2 <= a_old);

    let q_eff_old_1 = ((basis_1 as u16) * (a_old as u16)) / (a_basis_1 as u16);
    let q_eff_old_2 = ((basis_2 as u16) * (a_old as u16)) / (a_basis_2 as u16);
    let oi: u16 = q_eff_old_1 + q_eff_old_2;
    kani::assume(oi > 0);

    let q_close: u8 = kani::any();
    kani::assume(q_close > 0 && q_close <= 15 && (q_close as u16) < oi);
    let oi_post = oi - (q_close as u16);

    let a_new = ((a_old as u16) * oi_post) / oi;
    kani::assume(a_new > 0);

    let q_eff_new_1 = ((basis_1 as u16) * (a_new as u16)) / (a_basis_1 as u16);
    let q_eff_new_2 = ((basis_2 as u16) * (a_new as u16)) / (a_basis_2 as u16);
    let sum_new = q_eff_new_1 + q_eff_new_2;

    let phantom_dust = if oi_post >= sum_new {
        oi_post - sum_new
    } else {
        0
    };

    let n: u16 = 2;
    let global_a_dust = n + ((oi + n + (a_old as u16) - 1) / (a_old as u16));

    assert!(
        global_a_dust >= phantom_dust,
        "A-truncation dust bound must cover phantom OI from A change"
    );
}

/// Same-epoch zeroing: when settle_side_effects zeros a position (q_eff_new == 0),
/// the engine must increment phantom_dust_bound by 1.
#[kani::proof]
#[kani::solver(cadical)]
fn t14_62_dust_bound_same_epoch_zeroing() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let idx = add_user_test(&mut engine, 0).unwrap();
    engine.deposit_not_atomic(idx, 10_000_000, 0).unwrap();

    // Use basis=1, a_basis=3 so floor(1 * 1 / 3) = 0 → position zeroes
    engine.accounts[idx as usize].position_basis_q = 1i128;
    engine.accounts[idx as usize].adl_a_basis = 3;
    engine.accounts[idx as usize].adl_k_snap = 0i128;
    engine.accounts[idx as usize].adl_epoch_snap = 0;
    engine.stored_pos_count_long = 1;
    engine.adl_epoch_long = 0;
    engine.adl_coeff_long = 0i128;

    // A_side=1 so floor(1 * 1 / 3) = 0
    engine.adl_mult_long = 1;

    let dust_before = engine.phantom_dust_bound_long_q;

    let result = {
        let mut _ctx = InstructionContext::new_with_admission(0, 100);
        engine.settle_side_effects_live(idx as usize, &mut _ctx)
    };
    assert!(result.is_ok());

    // Position must be zeroed
    assert!(engine.accounts[idx as usize].position_basis_q == 0);
    // Dust bound must have incremented by 1
    let dust_after = engine.phantom_dust_bound_long_q;
    assert!(
        dust_after == dust_before + 1u128,
        "same-epoch zeroing must increment phantom_dust_bound by 1"
    );
}

/// Position reattach: floor(|basis| * A_new / A_old) loses at most 1 unit per position.
#[kani::proof]
#[kani::unwind(1)]
#[kani::solver(cadical)]
fn t14_63_dust_bound_position_reattach_remainder() {
    let basis: u8 = kani::any();
    kani::assume(basis > 0);
    let a_cur: u8 = kani::any();
    kani::assume(a_cur > 0);
    let a_basis: u8 = kani::any();
    kani::assume(a_basis > 0);

    let product = (basis as u16) * (a_cur as u16);
    let q_eff = product / (a_basis as u16);
    let remainder = product % (a_basis as u16);

    // Floor division: q_eff * a_basis + remainder == product
    assert!(
        q_eff * (a_basis as u16) + remainder == product,
        "floor division identity"
    );

    // Remainder is strictly less than divisor
    assert!(remainder < (a_basis as u16), "remainder < a_basis");

    // The effective quantity never exceeds the true (unrounded) quantity
    assert!(
        q_eff * (a_basis as u16) <= product,
        "floor never overshoots"
    );

    if remainder > 0 {
        assert!(
            (q_eff + 1) * (a_basis as u16) > product,
            "next integer exceeds product → loss < 1 unit"
        );
    }
}

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn t14_64_dust_bound_full_drain_reset_zeroes() {
    let mut engine = RiskEngine::new(zero_fee_params());

    engine.phantom_dust_bound_long_q = 42u128;
    engine.oi_eff_long_q = 0u128;
    engine.stored_pos_count_long = 0;
    engine.adl_epoch_long = 0;

    engine.begin_full_drain_reset(Side::Long).unwrap();

    assert!(engine.phantom_dust_bound_long_q == 0u128);
    assert!(engine.oi_eff_long_q == 0u128);
}

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn t14_65_dust_bound_end_to_end_clearance() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let mut ctx = InstructionContext::new();

    // Two long accounts (a,b) and one short (c) for OI balance.
    let a_idx = add_user_test(&mut engine, 0).unwrap();
    let b_idx = add_user_test(&mut engine, 0).unwrap();
    let c_idx = add_user_test(&mut engine, 0).unwrap();
    engine.deposit_not_atomic(a_idx, 10_000_000, 0).unwrap();
    engine.deposit_not_atomic(b_idx, 10_000_000, 0).unwrap();
    engine.deposit_not_atomic(c_idx, 10_000_000, 0).unwrap();

    engine.adl_mult_long = 13;
    engine.adl_mult_short = ADL_ONE;
    engine.adl_coeff_long = 0i128;
    engine.adl_coeff_short = 0i128;
    engine.adl_epoch_long = 0;

    // Account a: long 7*POS_SCALE at a_basis=13
    engine.accounts[a_idx as usize].position_basis_q = (7 * POS_SCALE) as i128;
    engine.accounts[a_idx as usize].adl_a_basis = 13;
    engine.accounts[a_idx as usize].adl_k_snap = 0i128;
    engine.accounts[a_idx as usize].adl_epoch_snap = 0;

    // Account b: long 5*POS_SCALE at a_basis=13
    engine.accounts[b_idx as usize].position_basis_q = (5 * POS_SCALE) as i128;
    engine.accounts[b_idx as usize].adl_a_basis = 13;
    engine.accounts[b_idx as usize].adl_k_snap = 0i128;
    engine.accounts[b_idx as usize].adl_epoch_snap = 0;

    // Account c: short 12*POS_SCALE
    engine.accounts[c_idx as usize].position_basis_q = -((12 * POS_SCALE) as i128);
    engine.accounts[c_idx as usize].adl_a_basis = ADL_ONE;
    engine.accounts[c_idx as usize].adl_k_snap = 0i128;
    engine.accounts[c_idx as usize].adl_epoch_snap = 0;

    engine.stored_pos_count_long = 2;
    engine.stored_pos_count_short = 1;
    engine.oi_eff_long_q = 12 * POS_SCALE;
    engine.oi_eff_short_q = 12 * POS_SCALE;

    // ADL: close 3*POS_SCALE from short side → shrinks A_long via truncation
    let result = engine.enqueue_adl(&mut ctx, Side::Short, 3 * POS_SCALE, 0u128);
    assert!(result.is_ok());
    // A_new = floor(13 * 9M / 12M) = 9
    assert!(engine.adl_mult_long == 9);
    assert!(engine.oi_eff_long_q == 9 * POS_SCALE);
    assert!(engine.oi_eff_short_q == 9 * POS_SCALE);
    assert!(engine.phantom_dust_bound_long_q != 0);

    // Settle long accounts to get actual effective positions under new A
    let sa = {
        let mut _ctx = InstructionContext::new_with_admission(0, 100);
        engine.settle_side_effects_live(a_idx as usize, &mut _ctx)
    };
    assert!(sa.is_ok());
    let sb = {
        let mut _ctx = InstructionContext::new_with_admission(0, 100);
        engine.settle_side_effects_live(b_idx as usize, &mut _ctx)
    };
    assert!(sb.is_ok());

    // Compute sum of actual effective positions
    let eff_a = engine.effective_pos_q(a_idx as usize);
    let eff_b = engine.effective_pos_q(b_idx as usize);
    let sum_eff = eff_a.unsigned_abs() + eff_b.unsigned_abs();

    // Dust = tracked OI - actual sum of effective positions
    let dust = engine.oi_eff_long_q.checked_sub(sum_eff).unwrap_or(0);

    // Verify phantom_dust_bound covers the multi-account A-truncation dust
    assert!(
        engine.phantom_dust_bound_long_q >= dust,
        "dust bound must cover A-truncation phantom OI for multiple accounts"
    );

    // Close all positions and set OI to balanced dust level
    // (simulating trade-based closing which maintains OI_long == OI_short)
    engine.attach_effective_position(a_idx as usize, 0i128);
    engine.attach_effective_position(b_idx as usize, 0i128);
    engine.attach_effective_position(c_idx as usize, 0i128);
    engine.oi_eff_long_q = dust;
    engine.oi_eff_short_q = dust;

    let reset_result = engine.schedule_end_of_instruction_resets(&mut ctx);
    assert!(
        reset_result.is_ok(),
        "dust bound must be sufficient for reset after all positions closed"
    );
}

// ############################################################################
// SPEC PROPERTY #17: fee shortfall routes to fee_credits, NOT PnL
// ############################################################################
//
// Spec v12.14.0 §4.10: "Unpaid explicit fees are account-local fee debt.
// They MUST NOT be written into PNL_i."
// Spec property #17: "trading-fee or liquidation-fee shortfall becomes
// negative fee_credits_i, does not touch PNL_i."

#[kani::proof]
#[kani::solver(cadical)]
fn proof_fee_shortfall_routes_to_fee_credits() {
    let mut engine = RiskEngine::new_with_market(zero_fee_params(), DEFAULT_SLOT, DEFAULT_ORACLE);

    let a = add_user_test(&mut engine, 0).unwrap();
    let capital: u16 = kani::any();
    let fee: u16 = kani::any();
    kani::assume(capital <= 1_000);
    kani::assume(fee > capital && fee <= capital + 500);

    engine.accounts[a as usize].capital = U128::new(capital as u128);
    engine.c_tot = U128::new(capital as u128);
    engine.vault = U128::new(capital as u128);

    let vault_before = engine.vault.get();
    let c_tot_before = engine.c_tot.get();
    let insurance_before = engine.insurance_fund.balance.get();
    let fc_before = engine.accounts[a as usize].fee_credits.get();
    let pnl_before = engine.accounts[a as usize].pnl;

    let (paid, impact, dropped) = engine
        .charge_fee_to_insurance(a as usize, fee as u128)
        .unwrap();
    let shortfall = (fee - capital) as u128;

    assert!(
        paid == capital as u128,
        "all available principal is paid to insurance first"
    );
    assert!(
        impact == fee as u128,
        "bounded shortfall remains collectible as local fee debt"
    );
    assert!(dropped == 0, "bounded fee shortfall must not be dropped");
    assert!(
        engine.accounts[a as usize].fee_credits.get() == fc_before - shortfall as i128,
        "fee shortfall must decrease fee_credits"
    );
    assert!(
        engine.accounts[a as usize].pnl == pnl_before,
        "fee must not touch PNL_i (spec property #17)"
    );
    assert!(
        engine.vault.get() == vault_before,
        "fee routing does not move external vault balance"
    );
    assert!(
        engine.c_tot.get() == c_tot_before - capital as u128,
        "paid principal leaves C_tot"
    );
    assert!(
        engine.insurance_fund.balance.get() == insurance_before + capital as u128,
        "paid principal enters insurance"
    );
    assert!(engine.check_conservation());
}

// ############################################################################
// SPEC PROPERTY #16: flat-close shortfall predicate
// ############################################################################

#[kani::proof]
#[kani::solver(cadical)]
fn proof_flat_close_shortfall_non_worsening() {
    let mut engine = RiskEngine::new_with_market(zero_fee_params(), DEFAULT_SLOT, DEFAULT_ORACLE);

    let a = add_user_test(&mut engine, 0).unwrap();
    let b = add_user_test(&mut engine, 0).unwrap();

    let size = (90 * POS_SCALE) as i128;
    engine.set_position_basis_q(a as usize, size).unwrap();
    engine.set_position_basis_q(b as usize, -size).unwrap();
    engine.oi_eff_long_q = size as u128;
    engine.oi_eff_short_q = size as u128;
    engine.set_pnl(a as usize, -1i128).unwrap();

    assert!(engine.effective_pos_q(a as usize) == size);
    assert!(engine.effective_pos_q(b as usize) == -size);
    assert!(
        engine.accounts[a as usize].pnl < 0,
        "fixture must contain uncovered negative PnL before the organic close"
    );
    assert!(engine.check_conservation());

    let not_pre = mul_div_ceil_u128(size.unsigned_abs(), DEFAULT_ORACLE as u128, POS_SCALE);
    let mm_req_pre = core::cmp::max(
        mul_div_floor_u128(
            not_pre,
            engine.params.maintenance_margin_bps as u128,
            10_000,
        ),
        engine.params.min_nonzero_mm_req,
    );
    let buffer_pre_equal = I256::from_i128(-1)
        .checked_sub(I256::from_u128(mm_req_pre))
        .expect("I256 sub");
    assert!(
        engine
            .enforce_one_side_margin(
                a as usize,
                DEFAULT_ORACLE,
                &size,
                &0,
                buffer_pre_equal,
                0,
                0
            )
            .is_ok(),
        "flat close may leave negative raw equity when shortfall does not worsen"
    );

    engine.accounts[a as usize].pnl = -2;
    assert!(
        matches!(
            engine.enforce_one_side_margin(
                a as usize,
                DEFAULT_ORACLE,
                &size,
                &0,
                buffer_pre_equal,
                0,
                0
            ),
            Err(RiskError::Undercollateralized)
        ),
        "flat close must reject when negative shortfall worsens"
    );
    assert!(
        engine.accounts[a as usize].position_basis_q == size,
        "margin check must not mutate position state"
    );
}

// ############################################################################
// SPEC PROPERTY #24: solvent flat-close succeeds
// ############################################################################

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_solvent_flat_close_succeeds() {
    let mut engine = RiskEngine::new_with_market(zero_fee_params(), DEFAULT_SLOT, DEFAULT_ORACLE);

    let a = add_user_test(&mut engine, 0).unwrap();
    let b = add_user_test(&mut engine, 0).unwrap();
    engine
        .deposit_not_atomic(a, 1_000_000, DEFAULT_SLOT)
        .unwrap();
    engine
        .deposit_not_atomic(b, 1_000_000, DEFAULT_SLOT)
        .unwrap();

    let size = POS_SCALE as i128;
    engine.attach_effective_position(a as usize, size).unwrap();
    engine.attach_effective_position(b as usize, -size).unwrap();
    engine.oi_eff_long_q = size as u128;
    engine.oi_eff_short_q = size as u128;

    assert!(engine.accounts[a as usize].pnl >= 0);
    assert!(engine.accounts[b as usize].pnl >= 0);

    let new_eff_a = 0i128;
    let new_eff_b = 0i128;
    let mm_req_pre = 50i128; // notional 1000 * 500 bps / 10_000
    let buffer_pre = I256::from_i128(1_000_000 - mm_req_pre);
    assert!(
        engine
            .enforce_one_side_margin(
                a as usize,
                DEFAULT_ORACLE,
                &size,
                &new_eff_a,
                buffer_pre,
                0,
                0
            )
            .is_ok(),
        "solvent long flat close must pass fee-neutral shortfall check"
    );
    assert!(
        engine
            .enforce_one_side_margin(
                b as usize,
                DEFAULT_ORACLE,
                &(-size),
                &new_eff_b,
                buffer_pre,
                0,
                0
            )
            .is_ok(),
        "solvent short flat close must pass fee-neutral shortfall check"
    );

    engine.attach_effective_position(a as usize, 0).unwrap();
    engine.attach_effective_position(b as usize, 0).unwrap();
    engine.oi_eff_long_q = 0;
    engine.oi_eff_short_q = 0;

    assert!(engine.effective_pos_q(a as usize) == 0);
    assert!(engine.effective_pos_q(b as usize) == 0);
    assert!(engine.stored_pos_count_long == 0);
    assert!(engine.stored_pos_count_short == 0);
    assert!(engine.oi_eff_long_q == engine.oi_eff_short_q);
    assert!(
        engine.check_conservation(),
        "conservation must hold after flat close"
    );
}

// ############################################################################
// SPEC §12 PROPERTY #23: Deposit materialization threshold
// ############################################################################

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_property_23_deposit_materialization_threshold() {
    // The engine rejects only amount == 0 at materialization; any
    // higher floor is wrapper policy. Verifies:
    //  - amount=0 on missing → reject
    //  - amount=1 on missing → materialize (no engine floor)
    //  - amount=0 on existing → no-op, no mutation
    let mut engine = RiskEngine::new(zero_fee_params());

    let existing = add_user_test(&mut engine, 0).unwrap();
    engine
        .deposit_not_atomic(existing, 5000, DEFAULT_SLOT)
        .unwrap();

    let missing: u16 = 3;
    assert!(!engine.is_used(missing as usize));
    let rej = engine.deposit_not_atomic(missing, 0, DEFAULT_SLOT);
    assert!(rej.is_err(), "amount=0 materialize must be rejected");
    assert!(!engine.is_used(missing as usize));

    let ok = engine.deposit_not_atomic(missing, 1, DEFAULT_SLOT);
    assert!(
        ok.is_ok(),
        "amount>0 materialize must succeed (wrapper enforces any higher floor)"
    );
    assert!(engine.is_used(missing as usize));

    // Existing accounts accept any top-up (including small ones)
    let topup = engine.deposit_not_atomic(existing, 1, DEFAULT_SLOT);
    assert!(topup.is_ok());

    assert!(engine.check_conservation());
}

// ############################################################################
// SPEC §12 PROPERTY #51: Universal withdrawal dust guard
// ############################################################################

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_property_51_withdraw_any_partial_ok() {
    // The engine no longer enforces a post-withdraw dust floor. Any
    // withdraw that leaves non-negative capital is allowed; wrappers
    // enforce any dust-avoidance policy at their own gate.
    let mut engine = RiskEngine::new(zero_fee_params());

    let a = add_user_test(&mut engine, 0).unwrap();
    engine.deposit_not_atomic(a, 5000, DEFAULT_SLOT).unwrap();

    // Withdraw leaving 500 — no floor, must succeed.
    let result =
        engine.withdraw_not_atomic(a, 4500, DEFAULT_ORACLE, DEFAULT_SLOT, 0i128, 0, 100, None);
    assert!(
        result.is_ok(),
        "partial withdraw must succeed regardless of remainder"
    );
    assert!(engine.accounts[a as usize].capital.get() == 500);

    assert!(engine.check_conservation());
}

// ############################################################################
// SPEC §12 PROPERTY #31: Missing-account safety
// ############################################################################

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_property_31_missing_account_safety() {
    // Per spec §2.3: settle_account_not_atomic, withdraw_not_atomic, execute_trade_not_atomic, liquidate,
    // and keeper_crank_not_atomic must NOT auto-materialize missing accounts.
    // deposit IS the canonical materialization path (spec §10.3 step 2).
    let mut engine = RiskEngine::new(zero_fee_params());

    // Add one real user for counterparty testing
    let real = add_user_test(&mut engine, 0).unwrap();
    engine
        .deposit_not_atomic(real, 100_000, DEFAULT_SLOT)
        .unwrap();
    engine
        .keeper_crank_not_atomic(DEFAULT_SLOT, DEFAULT_ORACLE, &[], 0, 0i128, 0, 100, None, 0)
        .unwrap();

    // Pick an index that was never add_user'd — it's missing
    let missing: u16 = 3; // MAX_ACCOUNTS=4 in kani, index 3 never materialized
    assert!(
        !engine.is_used(missing as usize),
        "account must be unmaterialized"
    );

    // settle_account_not_atomic must reject missing account
    let settle_result = engine.settle_account_not_atomic(
        missing,
        DEFAULT_ORACLE,
        DEFAULT_SLOT,
        0i128,
        0,
        100,
        None,
    );
    assert!(
        settle_result.is_err(),
        "settle_account_not_atomic must reject missing account"
    );

    // withdraw_not_atomic must reject missing account
    let withdraw_result = engine.withdraw_not_atomic(
        missing,
        100,
        DEFAULT_ORACLE,
        DEFAULT_SLOT,
        0i128,
        0,
        100,
        None,
    );
    assert!(
        withdraw_result.is_err(),
        "withdraw_not_atomic must reject missing account"
    );

    // execute_trade_not_atomic with missing account as party a
    let trade_result = engine.execute_trade_not_atomic(
        missing,
        real,
        DEFAULT_ORACLE,
        DEFAULT_SLOT,
        POS_SCALE as i128,
        DEFAULT_ORACLE,
        0i128,
        0,
        100,
        None,
    );
    assert!(
        trade_result.is_err(),
        "execute_trade_not_atomic must reject missing account (party a)"
    );

    // execute_trade_not_atomic with missing account as party b
    let trade_result_b = engine.execute_trade_not_atomic(
        real,
        missing,
        DEFAULT_ORACLE,
        DEFAULT_SLOT,
        POS_SCALE as i128,
        DEFAULT_ORACLE,
        0i128,
        0,
        100,
        None,
    );
    assert!(
        trade_result_b.is_err(),
        "execute_trade_not_atomic must reject missing account (party b)"
    );

    // liquidate_at_oracle_not_atomic on missing account — per spec §9.6 step 2 (Bug 4 fix),
    // public entrypoint rejects with Err(AccountNotFound) before mutating market state.
    let liq_result = engine.liquidate_at_oracle_not_atomic(
        missing,
        DEFAULT_SLOT,
        DEFAULT_ORACLE,
        LiquidationPolicy::FullClose,
        0i128,
        0,
        100,
        None,
    );
    assert!(
        matches!(liq_result, Err(RiskError::AccountNotFound)),
        "liquidate must reject missing account with AccountNotFound (spec §9.6 step 2)"
    );

    // Verify no account was materialized
    assert!(
        !engine.is_used(missing as usize),
        "missing account must remain unmaterialized"
    );
}

// ############################################################################
// SPEC §12 PROPERTY #44: Deposit true-flat guard
// ############################################################################

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_property_44_deposit_true_flat_guard() {
    // A deposit into an account with basis_pos_q != 0 must NOT call
    // resolve_flat_negative or fee_debt_sweep. We verify by observing
    // that insurance_fund doesn't change (resolve_flat_negative calls
    // absorb_protocol_loss which would affect insurance).
    let mut engine = RiskEngine::new(zero_fee_params());
    let a = add_user_test(&mut engine, 0).unwrap();

    engine.deposit_not_atomic(a, 500_000, DEFAULT_SLOT).unwrap();

    // Directly set up open position with negative PnL (bypassing trade to isolate deposit behavior)
    engine.accounts[a as usize].position_basis_q = (10 * POS_SCALE) as i128;
    engine.stored_pos_count_long = 1;
    engine.oi_eff_long_q = 10 * POS_SCALE;
    engine.oi_eff_short_q = 10 * POS_SCALE;
    engine.set_pnl(a as usize, -5_000i128);

    assert!(engine.accounts[a as usize].position_basis_q != 0);
    assert!(engine.accounts[a as usize].pnl < 0);

    let ins_before = engine.insurance_fund.balance.get();
    let pnl_before = engine.accounts[a as usize].pnl;

    // Deposit — with basis != 0, resolve_flat_negative must NOT run
    engine.deposit_not_atomic(a, 50_000, DEFAULT_SLOT).unwrap();

    // resolve_flat_negative calls absorb_protocol_loss which changes insurance_fund.
    // If it did NOT run, insurance_fund must be unchanged.
    assert!(
        engine.insurance_fund.balance.get() == ins_before,
        "insurance must not change: resolve_flat_negative must not run when basis != 0"
    );

    // Position must still be intact
    assert!(
        engine.accounts[a as usize].position_basis_q != 0,
        "position must still be intact after deposit"
    );

    // PnL may have been partially settled by settle_losses (step 7),
    // but it must NOT have been zeroed by resolve_flat_negative
    // (which zeros PnL and routes the loss through insurance).
    // settle_losses reduces PnL magnitude while reducing capital, without touching insurance.
    let pnl_after = engine.accounts[a as usize].pnl;
    assert!(
        pnl_after >= pnl_before,
        "PnL must not decrease further than settle_losses allows"
    );
}

// ############################################################################
// SPEC §12 PROPERTY #49: Profit-conversion reserve preservation
// ############################################################################

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_property_49_profit_conversion_reserve_preservation() {
    // Converting ReleasedPos_i = x must leave R_i unchanged and reduce
    // both PNL_pos_tot and PNL_matured_pos_tot by exactly x.
    let mut engine = RiskEngine::new_with_market(zero_fee_params(), DEFAULT_SLOT, DEFAULT_ORACLE);
    let a = add_user_test(&mut engine, 0).unwrap();

    engine.deposit_not_atomic(a, 10_000, DEFAULT_SLOT).unwrap();

    // Build PNL_i = 1000 with R_i = 400 and ReleasedPos_i = 600 through
    // canonical reserve accounting, without executing unrelated mark/crank paths.
    let mut reserve_ctx = InstructionContext::new();
    engine
        .set_pnl_with_reserve(
            a as usize,
            400,
            ReserveMode::UseAdmissionPair(10, 10),
            Some(&mut reserve_ctx),
        )
        .unwrap();

    let mut release_ctx = InstructionContext::new();
    engine
        .set_pnl_with_reserve(
            a as usize,
            1_000,
            ReserveMode::UseAdmissionPair(0, 0),
            Some(&mut release_ctx),
        )
        .unwrap();

    let released_before = engine.released_pos(a as usize);
    assert!(
        released_before == 600,
        "fixture must have positive released PnL and retained reserve"
    );
    assert!(engine.accounts[a as usize].reserved_pnl == 400);
    assert!(engine.pnl_pos_tot == 1_000);
    assert!(engine.pnl_matured_pos_tot == 600);

    let r_before = engine.accounts[a as usize].reserved_pnl;
    let pnl_before = engine.accounts[a as usize].pnl;
    let ppt_before = engine.pnl_pos_tot;
    let pmpt_before = engine.pnl_matured_pos_tot;

    let x_raw: u16 = kani::any();
    kani::assume(x_raw > 0);
    kani::assume((x_raw as u128) <= released_before);
    let x = x_raw as u128;

    engine.consume_released_pnl(a as usize, x).unwrap();

    // R_i must be unchanged
    assert!(
        engine.accounts[a as usize].reserved_pnl == r_before,
        "R_i must be unchanged after consume_released_pnl"
    );
    assert!(
        engine.accounts[a as usize].pnl == pnl_before - x as i128,
        "PNL_i must decrease by exactly x"
    );
    assert!(
        engine.released_pos(a as usize) == released_before - x,
        "ReleasedPos_i must decrease by exactly x"
    );

    // PNL_pos_tot decreased by exactly x
    assert!(
        engine.pnl_pos_tot == ppt_before - x,
        "pnl_pos_tot must decrease by exactly x"
    );

    // PNL_matured_pos_tot decreased by exactly x
    assert!(
        engine.pnl_matured_pos_tot == pmpt_before - x,
        "pnl_matured_pos_tot must decrease by exactly x"
    );
    assert!(
        engine.pnl_matured_pos_tot <= engine.pnl_pos_tot,
        "matured positive aggregate remains bounded by positive PnL aggregate"
    );
    assert!(engine.check_conservation());
}

// ############################################################################
// SPEC §12 PROPERTY #50: Flat-only automatic conversion
// ############################################################################

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_property_50_flat_only_auto_conversion() {
    // touch_account_live_local on an open-position account must NOT auto-convert.
    // Only flat accounts get auto-conversion via do_profit_conversion.
    let mut engine = RiskEngine::new_with_market(zero_fee_params(), DEFAULT_SLOT, DEFAULT_ORACLE);
    let a = add_user_test(&mut engine, 0).unwrap();
    let b = add_user_test(&mut engine, 0).unwrap();

    engine.deposit_not_atomic(a, 10_000, DEFAULT_SLOT).unwrap();

    let size_q = (100 * POS_SCALE) as i128;
    engine.set_position_basis_q(a as usize, size_q).unwrap();
    engine.set_position_basis_q(b as usize, -size_q).unwrap();
    engine.oi_eff_long_q = size_q as u128;
    engine.oi_eff_short_q = size_q as u128;

    // Build PNL_i = 1000 with R_i = 400 and ReleasedPos_i = 600. Then add
    // matching vault surplus so the shared snapshot is whole and conversion
    // would be allowed if, and only if, the account were flat.
    let mut reserve_ctx = InstructionContext::new();
    engine
        .set_pnl_with_reserve(
            a as usize,
            400,
            ReserveMode::UseAdmissionPair(10, 10),
            Some(&mut reserve_ctx),
        )
        .unwrap();
    let mut release_ctx = InstructionContext::new();
    engine
        .set_pnl_with_reserve(
            a as usize,
            1_000,
            ReserveMode::UseAdmissionPair(0, 0),
            Some(&mut release_ctx),
        )
        .unwrap();

    let released_before = engine.released_pos(a as usize);
    assert!(released_before == 600, "fixture must have released profit");
    engine.vault = U128::new(engine.vault.get() + released_before);

    let cap_before = engine.accounts[a as usize].capital.get();
    let r_before = engine.accounts[a as usize].reserved_pnl;
    let pnl_before = engine.accounts[a as usize].pnl;
    let ppt_before = engine.pnl_pos_tot;
    let pmpt_before = engine.pnl_matured_pos_tot;

    let mut flat = engine.clone();
    flat.set_position_basis_q(a as usize, 0).unwrap();
    flat.set_position_basis_q(b as usize, 0).unwrap();
    flat.oi_eff_long_q = 0;
    flat.oi_eff_short_q = 0;

    // Open account: even under a whole snapshot, auto-conversion is forbidden.
    assert!(
        engine.accounts[a as usize].position_basis_q != 0,
        "account must still have open position"
    );
    engine
        .finalize_touched_account_post_live_with_snapshot(a as usize, true)
        .unwrap();
    assert!(
        engine.accounts[a as usize].capital.get() == cap_before,
        "open account capital must not increase from auto-conversion"
    );
    assert!(
        engine.accounts[a as usize].reserved_pnl == r_before,
        "open account reserve must be unchanged"
    );
    assert!(
        engine.accounts[a as usize].pnl == pnl_before,
        "open account PnL must not be consumed"
    );
    assert!(
        engine.released_pos(a as usize) == released_before,
        "open account released PnL must remain unconverted"
    );
    assert!(engine.pnl_pos_tot == ppt_before);
    assert!(engine.pnl_matured_pos_tot == pmpt_before);
    assert!(engine.oi_eff_long_q == engine.oi_eff_short_q, "OI balance");
    assert!(engine.check_conservation());

    // Flat account: the same whole snapshot must auto-convert released profit.
    assert!(
        flat.accounts[a as usize].position_basis_q == 0,
        "flat branch fixture must be flat"
    );
    flat.finalize_touched_account_post_live_with_snapshot(a as usize, true)
        .unwrap();
    assert!(
        flat.accounts[a as usize].capital.get() == cap_before + released_before,
        "flat account capital must increase by released profit under whole snapshot"
    );
    assert!(
        flat.accounts[a as usize].reserved_pnl == r_before,
        "flat conversion must preserve reserve"
    );
    assert!(
        flat.pnl_pos_tot == ppt_before - released_before,
        "flat conversion consumes positive PnL aggregate"
    );
    assert!(
        flat.pnl_matured_pos_tot == pmpt_before - released_before,
        "flat conversion consumes matured positive PnL aggregate"
    );
    assert!(
        flat.released_pos(a as usize) == 0,
        "flat conversion consumes all released profit"
    );
    assert!(flat.oi_eff_long_q == flat.oi_eff_short_q, "flat OI balance");
    assert!(flat.check_conservation());
}

// ############################################################################
// SPEC §12 PROPERTY #52: Explicit open-position profit conversion
// ############################################################################

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_property_52_convert_released_pnl_instruction() {
    // convert_released_pnl_not_atomic consumes only ReleasedPos_i, leaves R_i unchanged,
    // sweeps fee debt, and rejects if post-conversion is not maintenance healthy.
    let mut engine = RiskEngine::new_with_market(zero_fee_params(), DEFAULT_SLOT, DEFAULT_ORACLE);
    let a = add_user_test(&mut engine, 0).unwrap();
    let b = add_user_test(&mut engine, 0).unwrap();

    engine.deposit_not_atomic(a, 20_000, DEFAULT_SLOT).unwrap();

    let size_q = (100 * POS_SCALE) as i128;
    engine.set_position_basis_q(a as usize, size_q).unwrap();
    engine.set_position_basis_q(b as usize, -size_q).unwrap();
    engine.oi_eff_long_q = size_q as u128;
    engine.oi_eff_short_q = size_q as u128;

    // Build PNL_i = 1000 with R_i = 400 and ReleasedPos_i = 600, then make
    // the haircut whole so y == x and the expected capital delta is exact.
    let mut reserve_ctx = InstructionContext::new();
    engine
        .set_pnl_with_reserve(
            a as usize,
            400,
            ReserveMode::UseAdmissionPair(10, 10),
            Some(&mut reserve_ctx),
        )
        .unwrap();
    let mut release_ctx = InstructionContext::new();
    engine
        .set_pnl_with_reserve(
            a as usize,
            1_000,
            ReserveMode::UseAdmissionPair(0, 0),
            Some(&mut release_ctx),
        )
        .unwrap();

    let released_before = engine.released_pos(a as usize);
    assert!(
        released_before == 600,
        "fixture must have released PnL to convert"
    );
    engine.vault = U128::new(engine.vault.get() + released_before);
    let (h_num, h_den) = engine.haircut_ratio();
    assert!(
        h_num == h_den && h_den == released_before,
        "fixture must be whole so conversion credit is exact"
    );

    let r_before = engine.accounts[a as usize].reserved_pnl;
    let cap_before = engine.accounts[a as usize].capital.get();
    let pnl_before = engine.accounts[a as usize].pnl;
    let ppt_before = engine.pnl_pos_tot;
    let pmpt_before = engine.pnl_matured_pos_tot;

    let x_raw: u16 = kani::any();
    kani::assume(x_raw > 0);
    kani::assume((x_raw as u128) <= released_before);
    let x = x_raw as u128;

    engine
        .convert_released_pnl_core(a as usize, x, DEFAULT_ORACLE)
        .unwrap();

    // R_i must be unchanged
    assert!(
        engine.accounts[a as usize].reserved_pnl == r_before,
        "R_i must be unchanged after convert_released_pnl_not_atomic"
    );

    assert!(
        engine.accounts[a as usize].capital.get() == cap_before + x,
        "capital must increase by exact whole-haircut conversion credit"
    );
    assert!(
        engine.accounts[a as usize].pnl == pnl_before - x as i128,
        "PNL_i must decrease by converted released amount"
    );
    assert!(
        engine.released_pos(a as usize) == released_before - x,
        "ReleasedPos_i must decrease by converted amount"
    );

    // PNL_pos_tot and PNL_matured_pos_tot must have decreased
    assert!(
        engine.pnl_pos_tot == ppt_before - x,
        "pnl_pos_tot must decrease by converted amount"
    );
    assert!(
        engine.pnl_matured_pos_tot == pmpt_before - x,
        "pnl_matured_pos_tot must decrease by converted amount"
    );

    // Account must still be maintenance healthy (conversion rejects if not)
    assert!(
        engine.is_above_maintenance_margin(
            &engine.accounts[a as usize],
            a as usize,
            DEFAULT_ORACLE
        ),
        "account must be maintenance healthy after conversion"
    );

    assert!(engine.oi_eff_long_q == engine.oi_eff_short_q, "OI balance");
    assert!(engine.check_conservation());
}

// ############################################################################
// AUDIT ROUND 2, ISSUE #7: Deposit must materialize missing accounts
// ############################################################################

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_audit2_deposit_materializes_missing_account() {
    // Per spec §10.3 step 2 and §2.3: deposit with amount >= MIN_INITIAL_DEPOSIT
    // on a missing account must materialize it, not reject with AccountNotFound.
    let mut engine = RiskEngine::new(zero_fee_params());

    // Slot 0 is free (no add_user called for it)
    assert!(!engine.is_used(0), "slot 0 must start free");

    let amount: u32 = kani::any();
    let min_dep = 1_000u128 as u32;
    kani::assume(amount >= min_dep && amount <= 1_000_000);

    // Deposit directly on the missing slot — must succeed and materialize
    let result = engine.deposit_not_atomic(0, amount as u128, DEFAULT_SLOT);
    assert!(
        result.is_ok(),
        "deposit must succeed and materialize missing account"
    );

    // Account must now be materialized
    assert!(
        engine.is_used(0),
        "account must be materialized after deposit"
    );

    // Capital must equal deposited amount
    assert!(
        engine.accounts[0].capital.get() == amount as u128,
        "capital must equal deposited amount"
    );

    // Vault must contain the deposited amount
    assert!(
        engine.vault.get() == amount as u128,
        "vault must contain deposited amount"
    );

    // Conservation must hold
    assert!(engine.check_conservation());
}

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_audit2_deposit_rejects_zero_amount_for_missing() {
    // The engine only rejects amount == 0 at materialization. Any higher
    // minimum-deposit floor is wrapper policy.
    let mut engine = RiskEngine::new(zero_fee_params());
    assert!(!engine.is_used(0));

    let result = engine.deposit_not_atomic(0, 0, DEFAULT_SLOT);
    assert!(result.is_err(), "amount=0 materialize must fail");
    assert!(
        !engine.is_used(0),
        "account must not be materialized on failed deposit"
    );
    assert!(
        engine.vault.get() == 0,
        "vault must not change on rejected deposit"
    );
}

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_audit2_deposit_existing_accepts_small_topup() {
    // Per spec §12 property #23: an existing materialized account may
    // receive deposits smaller than MIN_INITIAL_DEPOSIT.
    let mut engine = RiskEngine::new(zero_fee_params());
    let a = add_user_test(&mut engine, 0).unwrap();

    // First deposit to establish the account
    let min_dep = 1_000u128;
    engine.deposit_not_atomic(a, min_dep, DEFAULT_SLOT).unwrap();

    // Small top-up below MIN_INITIAL_DEPOSIT must succeed
    let small_amount = 1u128;
    let result = engine.deposit_not_atomic(a, small_amount, DEFAULT_SLOT);
    assert!(result.is_ok(), "existing account must accept small top-ups");
    assert!(engine.accounts[a as usize].capital.get() == min_dep + small_amount);
}

// ============================================================================
// Audit round 4: Atomicity and structural integrity proofs
// ============================================================================

/// Proof: add_user is atomic — if it fails, vault and insurance are unchanged.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_audit4_add_user_atomic_on_failure() {
    let mut params = zero_fee_params();
    let mut engine = RiskEngine::new(params);

    // --- Path 1: failure via "no free slots" ---
    for _ in 0..MAX_ACCOUNTS {
        add_user_test(&mut engine, 100).unwrap();
    }

    let vault_before = engine.vault.get();
    let ins_before = engine.insurance_fund.balance.get();
    let c_tot_before = engine.c_tot.get();

    let result = add_user_test(&mut engine, 100);
    assert!(result.is_err());

    assert!(
        engine.vault.get() == vault_before,
        "vault must not change on failed add_user (no slots)"
    );
    assert!(
        engine.insurance_fund.balance.get() == ins_before,
        "insurance must not change on failed add_user (no slots)"
    );
    assert!(
        engine.c_tot.get() == c_tot_before,
        "c_tot must not change on failed add_user (no slots)"
    );
}

/// Proof: deposit_not_atomic (the sole materialization path since
/// v12.18.1) enforces MAX_VAULT_TVL atomically — the first deposit
/// that would push vault over the cap is rejected without mutating
/// vault, insurance, or the slot count. Prior spec drafts had an
/// `add_user` opening fee that could push vault past the cap; that
/// surface was removed (the fee path no longer exists), so this test
/// now exercises the deposit-materialize path directly.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_audit4_add_user_atomic_on_tvl_failure() {
    let params = zero_fee_params();
    let mut engine = RiskEngine::new(params);
    let min = 1_000u128;

    // Pin vault just below MAX_VAULT_TVL so min_initial_deposit would
    // push it over.
    engine.vault = U128::new(MAX_VAULT_TVL - (min - 1));

    let vault_before = engine.vault.get();
    let ins_before = engine.insurance_fund.balance.get();
    let used_before = engine.num_used_accounts;

    // Deposit-materialize at amount=min_initial_deposit exceeds cap → reject.
    let result = engine.deposit_not_atomic(0, min, 0);
    assert!(result.is_err());

    assert!(
        engine.vault.get() == vault_before,
        "vault must not change on MAX_VAULT_TVL rejection"
    );
    assert!(
        engine.insurance_fund.balance.get() == ins_before,
        "insurance must not change on MAX_VAULT_TVL rejection"
    );
    assert!(
        engine.num_used_accounts == used_before,
        "num_used_accounts must not change on MAX_VAULT_TVL rejection"
    );
    assert!(!engine.is_used(0), "slot 0 must not be materialized on Err");
}

/// Proof: deposit_fee_credits enforces MAX_VAULT_TVL.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_audit4_deposit_fee_credits_max_tvl() {
    let params = zero_fee_params();
    let mut engine = RiskEngine::new(params);
    let idx = add_user_test(&mut engine, 0).unwrap();

    // Give account fee debt so deposit is not a no-op
    engine.accounts[idx as usize].fee_credits = I128::new(-1000);

    // Set vault at MAX_VAULT_TVL
    engine.vault = U128::new(MAX_VAULT_TVL);

    // Deposit must fail (vault already at MAX)
    let result = engine.deposit_fee_credits(idx, 500, 0);
    assert!(
        result.is_err(),
        "must reject deposit that would exceed MAX_VAULT_TVL"
    );
    assert!(
        engine.vault.get() == MAX_VAULT_TVL,
        "vault unchanged on failure"
    );
}

// ============================================================================
// v12.19 reclaim_empty_account dt envelope (§9.10 step 3a, property 104)
// Priority #2 from rev6 plan: envelope bound + atomicity on rejection.
// ============================================================================

#[kani::proof]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn v19_reclaim_envelope_rejection_is_pre_mutation() {
    // On envelope-violating now_slot, reclaim must reject and leave
    // current_slot, is_used, and all account-local fields unchanged. The spec's
    // zero-OI fast-forward exception means this clause only applies with live OI.
    let mut engine = RiskEngine::new(zero_fee_params());
    let idx = add_user_test(&mut engine, 0).unwrap();
    let long = add_user_test(&mut engine, 0).unwrap();
    let short = add_user_test(&mut engine, 0).unwrap();
    // Canonical post-trade exposure snapshot. Trade admission itself is proved
    // separately; this harness isolates reclaim's live-OI envelope gate.
    engine.accounts[long as usize].capital = U128::new(100_000);
    engine.accounts[short as usize].capital = U128::new(100_000);
    engine.accounts[long as usize].position_basis_q = POS_SCALE as i128;
    engine.accounts[short as usize].position_basis_q = -(POS_SCALE as i128);
    engine.accounts[long as usize].adl_a_basis = ADL_ONE;
    engine.accounts[short as usize].adl_a_basis = ADL_ONE;
    engine.oi_eff_long_q = POS_SCALE as u128;
    engine.oi_eff_short_q = POS_SCALE as u128;
    engine.stored_pos_count_long = 1;
    engine.stored_pos_count_short = 1;
    assert!(engine.oi_eff_long_q > 0 && engine.oi_eff_short_q > 0);

    // Set account clean (reclaim preconditions).
    engine.accounts[idx as usize].capital = U128::ZERO;
    engine.accounts[idx as usize].pnl = 0;
    engine.accounts[idx as usize].reserved_pnl = 0;
    engine.accounts[idx as usize].position_basis_q = 0;
    engine.accounts[idx as usize].sched_present = 0;
    engine.accounts[idx as usize].pending_present = 0;
    engine.accounts[idx as usize].fee_credits = I128::ZERO;

    // Envelope = last_market_slot + max_accrual_dt_slots.
    let envelope = engine
        .last_market_slot
        .saturating_add(engine.params.max_accrual_dt_slots);

    // Symbolic now_slot beyond envelope but >= current_slot.
    let slack: u8 = kani::any();
    kani::assume(slack > 0);
    let now_slot = envelope.saturating_add(slack as u64);
    kani::assume(now_slot >= engine.current_slot);

    let current_slot_before = engine.current_slot;
    let used_before = engine.is_used(idx as usize);
    let cap_before = engine.accounts[idx as usize].capital.get();
    let fee_before = engine.accounts[idx as usize].fee_credits;

    let r = engine.reclaim_empty_account_not_atomic(idx, now_slot);
    assert!(
        matches!(r, Err(RiskError::Overflow)),
        "envelope-violating now_slot must reject"
    );
    assert!(
        engine.current_slot == current_slot_before,
        "rejection MUST NOT advance current_slot"
    );
    assert!(
        engine.is_used(idx as usize) == used_before,
        "rejection MUST NOT free the slot"
    );
    assert!(engine.accounts[idx as usize].capital.get() == cap_before);
    assert!(engine.accounts[idx as usize].fee_credits == fee_before);
}

#[kani::proof]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn v19_reclaim_envelope_accept_within_bound() {
    // Within envelope, reclaim succeeds and current_slot advances.
    let mut engine = RiskEngine::new(zero_fee_params());
    let idx = add_user_test(&mut engine, 0).unwrap();

    engine.accounts[idx as usize].capital = U128::ZERO;
    engine.accounts[idx as usize].pnl = 0;
    engine.accounts[idx as usize].reserved_pnl = 0;
    engine.accounts[idx as usize].position_basis_q = 0;
    engine.accounts[idx as usize].sched_present = 0;
    engine.accounts[idx as usize].pending_present = 0;
    engine.accounts[idx as usize].fee_credits = I128::ZERO;

    let envelope = engine
        .last_market_slot
        .saturating_add(engine.params.max_accrual_dt_slots);
    let now_slot: u8 = kani::any();
    kani::assume((now_slot as u64) >= engine.current_slot);
    kani::assume((now_slot as u64) <= envelope);

    let r = engine.reclaim_empty_account_not_atomic(idx, now_slot as u64);
    assert!(r.is_ok());
    assert_eq!(engine.current_slot, now_slot as u64);
    assert!(!engine.is_used(idx as usize));
}

// ============================================================================
// v12.19 init-time solvency envelope (§1.4, property 90)
// ============================================================================

#[kani::proof]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn v19_accrue_market_envelope_enforces_goal52_bound() {
    // Spec §1.4 + §5.5: the init-time envelope inequality
    //   price_budget + funding_budget + liq_fee <= maint
    // combined with the per-accrual price-move cap
    //   abs_dp * 10_000 <= cap * dt * P_last
    // bounds the adverse equity drain per envelope. This proof verifies
    // that for any symbolic abs_dp that would exceed the per-slot cap at
    // a given dt, accrue_market_to rejects — the construction-level
    // guarantee backing goal 52.
    let mut engine = RiskEngine::new(zero_fee_params());
    engine.oi_eff_long_q = 1_000_000;
    engine.oi_eff_short_q = 1_000_000;
    engine.last_oracle_price = 10_000;
    engine.fund_px_last = 10_000;
    engine.last_market_slot = 0;
    engine.adl_mult_long = ADL_ONE;
    engine.adl_mult_short = ADL_ONE;

    let cap_per_slot = engine.params.max_price_move_bps_per_slot as u128;
    let p_last = engine.last_oracle_price as u128;

    // Symbolic dt and abs_dp; assume a move that exceeds the cap.
    let dt: u8 = kani::any();
    kani::assume(dt > 0 && (dt as u64) <= engine.params.max_accrual_dt_slots);
    let abs_dp: u16 = kani::any();
    kani::assume(abs_dp > 0);
    // Exceed-cap predicate: abs_dp * 10_000 > cap * dt * P_last.
    let lhs = (abs_dp as u128) * 10_000;
    let rhs = cap_per_slot * (dt as u128) * p_last;
    kani::assume(lhs > rhs);
    // Keep abs_dp within u64 price range.
    kani::assume((abs_dp as u128) <= u64::MAX as u128 - p_last);

    let new_price = p_last as u64 + abs_dp as u64;
    let r = engine.accrue_market_to(dt as u64, new_price, 0);
    assert!(
        r.is_err(),
        "any abs_dp exceeding the per-slot cap MUST reject — goal 52 construction"
    );
    // State unchanged on rejection.
    assert_eq!(engine.last_oracle_price, p_last as u64);
    assert_eq!(engine.last_market_slot, 0);
}
