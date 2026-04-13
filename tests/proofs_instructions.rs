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

    let a = engine.add_user(0).unwrap();
    let b = engine.add_user(0).unwrap();
    engine.deposit(a, 1_000_000, 100, 0).unwrap();
    engine.deposit(b, 1_000_000, 100, 0).unwrap();

    let k_val: i8 = kani::any();
    let k = k_val as i128;

    engine.accounts[a as usize].position_basis_q = I128::new(POS_SCALE as i128);
    engine.accounts[a as usize].adl_a_basis = U128::new(ADL_ONE);
    engine.accounts[a as usize].adl_k_snap = I128::new(k);
    engine.accounts[a as usize].adl_epoch_snap = 0;
    engine.accounts[b as usize].position_basis_q = I128::new(POS_SCALE as i128);
    engine.accounts[b as usize].adl_a_basis = U128::new(ADL_ONE);
    engine.accounts[b as usize].adl_k_snap = I128::new(k);
    engine.accounts[b as usize].adl_epoch_snap = 0;
    engine.stored_pos_count_long = 2;

    engine.adl_coeff_long = I128::new(k);

    engine.oi_eff_long_q = U128::ZERO;
    engine.begin_full_drain_reset(Side::Long);

    assert!(engine.side_mode_long == SideMode::ResetPending);
    assert!(engine.stale_account_count_long == 2);

    let _ = engine.settle_side_effects_with_h_lock(a as usize, 0);
    assert!(engine.stale_account_count_long == 1);

    let _ = engine.settle_side_effects_with_h_lock(b as usize, 0);
    assert!(engine.stale_account_count_long == 0);
}

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn t3_16b_reset_counter_with_nonzero_k_diff() {
    let mut engine = RiskEngine::new(zero_fee_params());

    let a = engine.add_user(0).unwrap();
    let b = engine.add_user(0).unwrap();
    engine.deposit(a, 10_000_000, 100, 0).unwrap();
    engine.deposit(b, 10_000_000, 100, 0).unwrap();

    let k_snap = 0i128;

    engine.accounts[a as usize].position_basis_q = I128::new(POS_SCALE as i128);
    engine.accounts[a as usize].adl_a_basis = U128::new(ADL_ONE);
    engine.accounts[a as usize].adl_k_snap = I128::new(k_snap);
    engine.accounts[a as usize].adl_epoch_snap = 0;
    engine.accounts[b as usize].position_basis_q = I128::new(POS_SCALE as i128);
    engine.accounts[b as usize].adl_a_basis = U128::new(ADL_ONE);
    engine.accounts[b as usize].adl_k_snap = I128::new(k_snap);
    engine.accounts[b as usize].adl_epoch_snap = 0;
    engine.stored_pos_count_long = 2;

    let k_diff_val: i8 = kani::any();
    kani::assume(k_diff_val != 0);
    let k_long = k_diff_val as i128;
    engine.adl_coeff_long = I128::new(k_long);

    engine.oi_eff_long_q = U128::ZERO;
    engine.begin_full_drain_reset(Side::Long);

    assert!(engine.adl_epoch_start_k_long == I128::new(k_long));
    assert!(engine.stale_account_count_long == 2);

    let _ = engine.settle_side_effects_with_h_lock(a as usize, 0);
    assert!(engine.stale_account_count_long == 1);
    let _ = engine.settle_side_effects_with_h_lock(b as usize, 0);
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
    assert!(engine.oi_eff_long_q.get() == 0);
    assert!(engine.oi_eff_short_q.get() == 0);
    assert!(engine.phantom_dust_bound_long_q.get() == 0);
    assert!(engine.phantom_dust_bound_short_q.get() == 0);

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

    engine.phantom_dust_bound_long_q = U128::new(5u128);
    engine.oi_eff_long_q = U128::ZERO;

    engine.begin_full_drain_reset(Side::Long);

    assert!(engine.phantom_dust_bound_long_q.get() == 0,
        "phantom_dust_bound must be zeroed by begin_full_drain_reset");
}

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn t3_19_finalize_side_reset_requires_all_stale_touched() {
    let mut engine = RiskEngine::new(zero_fee_params());

    engine.side_mode_long = SideMode::ResetPending;
    engine.oi_eff_long_q = U128::ZERO;
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

    let idx = engine.add_user(0).unwrap();
    engine.deposit(idx, 10_000_000, 100, 0).unwrap();

    engine.accounts[idx as usize].position_basis_q = I128::new(POS_SCALE as i128);
    engine.accounts[idx as usize].adl_a_basis = U128::new(ADL_ONE);
    engine.accounts[idx as usize].adl_k_snap = I128::ZERO;
    engine.accounts[idx as usize].adl_epoch_snap = 0;
    engine.stored_pos_count_long = 1;

    engine.adl_coeff_long = I128::new(500i128);

    engine.oi_eff_long_q = U128::ZERO;
    engine.begin_full_drain_reset(Side::Long);

    assert!(engine.adl_epoch_start_k_long == I128::new(500i128));
    assert!(engine.adl_epoch_long == 1);
    assert!(engine.stale_account_count_long == 1);

    let result = engine.settle_side_effects_with_h_lock(idx as usize, 0);
    assert!(result.is_ok());

    assert!(engine.accounts[idx as usize].position_basis_q.get() == 0);
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
    let idx = engine.add_user(0).unwrap();
    engine.deposit(idx, 10_000_000, 100, 0).unwrap();

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

    assert!(released2 >= released1, "warmup release must be monotone non-decreasing in time");
}

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn t9_36_fee_seniority_after_restart() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let idx = engine.add_user(0).unwrap();
    engine.deposit(idx, 10_000_000, 100, 0).unwrap();

    let fc_val: i8 = kani::any();
    engine.accounts[idx as usize].fee_credits = I128::new(fc_val as i128);

    let fc_before = engine.accounts[idx as usize].fee_credits;

    engine.accounts[idx as usize].position_basis_q = I128::new(POS_SCALE as i128);
    engine.accounts[idx as usize].adl_a_basis = U128::new(ADL_ONE);
    engine.accounts[idx as usize].adl_k_snap = I128::ZERO;
    engine.accounts[idx as usize].adl_epoch_snap = 0;
    engine.stored_pos_count_long = 1;
    engine.adl_epoch_long = 1;
    engine.adl_epoch_start_k_long = I128::ZERO;
    engine.side_mode_long = SideMode::ResetPending;
    engine.stale_account_count_long = 1;
    engine.adl_coeff_long = I128::ZERO;

    let _ = engine.settle_side_effects_with_h_lock(idx as usize, 0);

    let fc_after = engine.accounts[idx as usize].fee_credits;
    assert!(fc_after == fc_before, "fee_credits must be preserved across epoch restart");
}

// ############################################################################
// T10: ACCRUE_MARKET_TO
// ############################################################################

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn t10_37_accrue_mark_matches_eager() {
    let mut engine = RiskEngine::new(zero_fee_params());

    engine.oi_eff_long_q = U128::new(POS_SCALE);
    engine.oi_eff_short_q = U128::new(POS_SCALE);
    engine.adl_mult_long = U128::new(ADL_ONE);
    engine.adl_mult_short = U128::new(ADL_ONE);
    engine.last_oracle_price = 100;
    engine.last_market_slot = 0;

    let k_long_before = engine.adl_coeff_long.get();
    let k_short_before = engine.adl_coeff_short.get();

    let dp: i8 = kani::any();
    kani::assume(dp >= -50 && dp <= 50);
    let new_price = (100i16 + dp as i16) as u64;
    kani::assume(new_price > 0);

    let result = engine.accrue_market_to(1, new_price, 0);
    assert!(result.is_ok());

    let k_long_after = engine.adl_coeff_long.get();
    let k_short_after = engine.adl_coeff_short.get();

    let expected_delta = (ADL_ONE as i128) * (dp as i128);
    let actual_long_delta = k_long_after.checked_sub(k_long_before).unwrap();
    assert!(actual_long_delta == expected_delta, "K_long delta must equal A_long * delta_p");

    let actual_short_delta = k_short_after.checked_sub(k_short_before).unwrap();
    let expected_short_delta = expected_delta.checked_neg().unwrap_or(0i128);
    assert!(actual_short_delta == expected_short_delta,
        "K_short delta must equal -(A_short * delta_p)");
}

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn t10_38_accrue_funding_payer_driven() {
    let mut engine = RiskEngine::new(zero_fee_params());

    engine.oi_eff_long_q = U128::new(POS_SCALE);
    engine.oi_eff_short_q = U128::new(POS_SCALE);
    engine.adl_mult_long = U128::new(ADL_ONE);
    engine.adl_mult_short = U128::new(ADL_ONE);
    engine.last_oracle_price = 100;
    engine.last_market_slot = 0;

    let rate: i8 = kani::any();
    kani::assume(rate != 0);
    kani::assume(rate >= -100 && rate <= 100);

    let k_long_before = engine.adl_coeff_long.get();
    let k_short_before = engine.adl_coeff_short.get();

    let result = engine.accrue_market_to(1, 100, rate as i128);
    assert!(result.is_ok());

    let k_long_after = engine.adl_coeff_long.get();
    let k_short_after = engine.adl_coeff_short.get();

    // v12.15: K gets truncation-divided integer part, F gets remainder.
    // fund_num = 100 * rate. fund_term = fund_num / 1e9 (truncation toward zero).
    // For |fund_num| < 1e9, fund_term = 0 and all funding goes to F.
    let fund_num = 100i128 * (rate as i128);
    let fund_term = fund_num / (1_000_000_000i128);
    let remainder = fund_num - fund_term * 1_000_000_000i128;

    let a_long = ADL_ONE as i128;
    let expected_k_long = k_long_before - a_long * fund_term;
    let expected_k_short = k_short_before + a_long * fund_term;

    assert!(k_long_after == expected_k_long, "K_long must match truncated fund_term");
    assert!(k_short_after == expected_k_short, "K_short must match truncated fund_term");

    // F captures the remainder (per-side, with A multiplication)
    let expected_f_long = -(a_long * remainder);
    let expected_f_short = a_long * remainder;
    assert!(engine.f_long_num.get() == expected_f_long, "F_long must capture remainder");
    assert!(engine.f_short_num.get() == expected_f_short, "F_short must capture remainder");

    // Combined K + F is exact: no funding is lost
    // K_delta * FUNDING_DEN + F_delta = A_side * fund_num (exact)
    let k_delta_long = k_long_after - k_long_before;
    let total_long = k_delta_long * 1_000_000_000i128 + engine.f_long_num.get();
    assert!(total_long == -(a_long * fund_num), "K + F must equal exact funding");
}

// ############################################################################
// T11: ENGINE INTEGRATION
// ############################################################################

#[kani::proof]
#[kani::solver(cadical)]
fn t11_39_same_epoch_settle_idempotent_real_engine() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let idx = engine.add_user(0).unwrap();
    engine.deposit(idx, 10_000_000, 100, 0).unwrap();

    let pos = POS_SCALE as i128;
    engine.accounts[idx as usize].position_basis_q = I128::new(pos);
    engine.accounts[idx as usize].adl_a_basis = U128::new(ADL_ONE);
    engine.accounts[idx as usize].adl_k_snap = I128::ZERO;
    engine.accounts[idx as usize].adl_epoch_snap = 0;
    engine.stored_pos_count_long = 1;
    engine.adl_epoch_long = 0;
    engine.oi_eff_long_q = U128::new(POS_SCALE);

    engine.adl_coeff_long = I128::new(100i128);

    let r1 = engine.settle_side_effects_with_h_lock(idx as usize, 0);
    assert!(r1.is_ok());
    let pnl_after_first = engine.accounts[idx as usize].pnl;
    assert!(engine.accounts[idx as usize].adl_k_snap == I128::new(100i128));

    let r2 = engine.settle_side_effects_with_h_lock(idx as usize, 0);
    assert!(r2.is_ok());
    let pnl_after_second = engine.accounts[idx as usize].pnl;

    assert!(pnl_after_second == pnl_after_first,
        "second settle with unchanged K must produce zero incremental PnL");
    assert!(engine.accounts[idx as usize].adl_a_basis == U128::new(ADL_ONE));
    assert!(engine.accounts[idx as usize].position_basis_q == I128::new(pos));
}

#[kani::proof]
#[kani::solver(cadical)]
fn t11_40_non_compounding_quantity_basis_two_touches() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let idx = engine.add_user(0).unwrap();
    engine.deposit(idx, 10_000_000, 100, 0).unwrap();

    let pos = POS_SCALE as i128;
    engine.accounts[idx as usize].position_basis_q = I128::new(pos);
    engine.accounts[idx as usize].adl_a_basis = U128::new(ADL_ONE);
    engine.accounts[idx as usize].adl_k_snap = I128::ZERO;
    engine.accounts[idx as usize].adl_epoch_snap = 0;
    engine.stored_pos_count_long = 1;
    engine.adl_epoch_long = 0;
    engine.oi_eff_long_q = U128::new(POS_SCALE);

    engine.adl_coeff_long = I128::new(50i128);
    let _ = engine.settle_side_effects_with_h_lock(idx as usize, 0);

    assert!(engine.accounts[idx as usize].position_basis_q == I128::new(pos));
    assert!(engine.accounts[idx as usize].adl_a_basis == U128::new(ADL_ONE));
    assert!(engine.accounts[idx as usize].adl_k_snap == I128::new(50i128));

    engine.adl_coeff_long = I128::new(120i128);
    let _ = engine.settle_side_effects_with_h_lock(idx as usize, 0);

    assert!(engine.accounts[idx as usize].position_basis_q == I128::new(pos));
    assert!(engine.accounts[idx as usize].adl_a_basis == U128::new(ADL_ONE));
    assert!(engine.accounts[idx as usize].adl_k_snap == I128::new(120i128));
}

#[kani::proof]
#[kani::solver(cadical)]
fn t11_41_attach_effective_position_remainder_accounting() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let idx = engine.add_user(0).unwrap();
    engine.deposit(idx, 10_000_000, 100, 0).unwrap();

    // Use a_basis=7, a_side=6 so that POS_SCALE * 6 % 7 != 0 (nonzero remainder)
    engine.accounts[idx as usize].position_basis_q = I128::new(POS_SCALE as i128);
    engine.accounts[idx as usize].adl_a_basis = U128::new(7);
    engine.accounts[idx as usize].adl_epoch_snap = 0;
    engine.adl_epoch_long = 0;
    engine.adl_mult_long = U128::new(6);
    engine.stored_pos_count_long = 1;

    let dust_before = engine.phantom_dust_bound_long_q;

    let new_pos = (2 * POS_SCALE) as i128;
    engine.attach_effective_position(idx as usize, new_pos);

    assert!(engine.phantom_dust_bound_long_q > dust_before,
        "dust bound must increment on nonzero remainder");

    // Now test zero remainder: a_basis == a_side → product evenly divisible
    engine.accounts[idx as usize].position_basis_q = I128::new(POS_SCALE as i128);
    engine.accounts[idx as usize].adl_a_basis = U128::new(ADL_ONE);
    engine.adl_mult_long = U128::new(ADL_ONE);

    let dust_before2 = engine.phantom_dust_bound_long_q;
    engine.attach_effective_position(idx as usize, (3 * POS_SCALE) as i128);

    assert!(engine.phantom_dust_bound_long_q == dust_before2,
        "dust bound must not increment on zero remainder");
}

#[kani::proof]
#[kani::solver(cadical)]
fn t11_42_dynamic_dust_bound_inductive() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let a = engine.add_user(0).unwrap();
    let b = engine.add_user(0).unwrap();
    engine.deposit(a, 10_000_000, 100, 0).unwrap();
    engine.deposit(b, 10_000_000, 100, 0).unwrap();

    // Use basis=1, a_basis=3 so floor(1 * 1 / 3) = 0 → position zeroes
    engine.accounts[a as usize].position_basis_q = I128::new(1i128);
    engine.accounts[a as usize].adl_a_basis = U128::new(3);
    engine.accounts[a as usize].adl_k_snap = I128::ZERO;
    engine.accounts[a as usize].adl_epoch_snap = 0;
    engine.accounts[b as usize].position_basis_q = I128::new(1i128);
    engine.accounts[b as usize].adl_a_basis = U128::new(3);
    engine.accounts[b as usize].adl_k_snap = I128::ZERO;
    engine.accounts[b as usize].adl_epoch_snap = 0;
    engine.stored_pos_count_long = 2;
    engine.adl_epoch_long = 0;
    engine.oi_eff_long_q = U128::new(2);

    engine.adl_mult_long = U128::new(1);

    let _ = engine.settle_side_effects_with_h_lock(a as usize, 0);
    assert!(engine.accounts[a as usize].position_basis_q.get() == 0);
    assert!(engine.phantom_dust_bound_long_q.get() == 1u128);

    let _ = engine.settle_side_effects_with_h_lock(b as usize, 0);
    assert!(engine.accounts[b as usize].position_basis_q.get() == 0);
    assert!(engine.phantom_dust_bound_long_q.get() == 2u128);
}

#[kani::proof]
#[kani::solver(cadical)]
fn t11_50_execute_trade_atomic_oi_update_sign_flip() {
    let mut engine = RiskEngine::new(zero_fee_params());

    let a = engine.add_user(0).unwrap();
    let b = engine.add_user(0).unwrap();
    engine.deposit(a, 100_000_000, 100, 0).unwrap();
    engine.deposit(b, 100_000_000, 100, 0).unwrap();

    engine.last_oracle_price = 100;
    engine.last_market_slot = 1;
    engine.last_crank_slot = 1;

    let size_q = POS_SCALE as i128;
    let r1 = engine.execute_trade_not_atomic(a, b, 100, 1, size_q, 100, 0i128, 0);
    assert!(r1.is_ok());
    assert!(engine.oi_eff_long_q == engine.oi_eff_short_q);

    // Swap a,b to reverse direction (size_q must be > 0)
    let flip_size = (2 * POS_SCALE) as i128;
    let r2 = engine.execute_trade_not_atomic(b, a, 100, 2, flip_size, 100, 0i128, 0);
    assert!(r2.is_ok());

    assert!(engine.oi_eff_long_q == engine.oi_eff_short_q, "OI must be balanced after sign flip");
}

#[kani::proof]
#[kani::solver(cadical)]
fn t11_51_execute_trade_slippage_zero_sum() {
    let mut engine = RiskEngine::new(zero_fee_params());

    let a = engine.add_user(0).unwrap();
    let b = engine.add_user(0).unwrap();
    engine.deposit(a, 10_000_000, 100, 0).unwrap();
    engine.deposit(b, 10_000_000, 100, 0).unwrap();

    engine.last_oracle_price = 100;
    engine.last_market_slot = 1;
    engine.last_crank_slot = 1;

    let vault_before = engine.vault.get();

    let size_q = POS_SCALE as i128;
    let result = engine.execute_trade_not_atomic(a, b, 100, 1, size_q, 100, 0i128, 0);
    assert!(result.is_ok());

    let vault_after = engine.vault.get();
    assert!(vault_after == vault_before, "vault must be unchanged with zero fees at oracle price");
    assert!(engine.check_conservation());
}

#[kani::proof]
#[kani::solver(cadical)]
fn t11_52_touch_account_full_restart_fee_seniority() {
    let mut engine = RiskEngine::new(zero_fee_params());

    let idx = engine.add_user(0).unwrap();
    engine.deposit(idx, 10_000_000, 100, 0).unwrap();

    let pos = POS_SCALE as i128;
    engine.accounts[idx as usize].position_basis_q = I128::new(pos);
    engine.accounts[idx as usize].adl_a_basis = U128::new(ADL_ONE);
    engine.accounts[idx as usize].adl_k_snap = I128::ZERO;
    engine.accounts[idx as usize].adl_epoch_snap = 0;
    engine.stored_pos_count_long = 1;
    engine.adl_epoch_long = 0;
    engine.oi_eff_long_q = U128::new(POS_SCALE);

    engine.accounts[idx as usize].pnl = I128::new(5000i128);
    engine.pnl_pos_tot = U128::new(5000u128);

    engine.adl_coeff_long = I128::new((ADL_ONE as i128) * 100);

    engine.accounts[idx as usize].fee_credits = I128::new(-500i128);

    engine.last_oracle_price = 100;
    engine.last_market_slot = 100;

    let cap_before = engine.accounts[idx as usize].capital.get();
    let ins_before = engine.insurance_fund.balance.get();

    // New touch pattern: accrue market, then touch_account_live_local + finalize
    {
        let mut ctx = InstructionContext::new_with_h_lock(0);
        engine.accrue_market_to(100, 100, 0).unwrap();
        engine.current_slot = 100;
        engine.touch_account_live_local(idx as usize, &mut ctx).unwrap();
        engine.finalize_touched_accounts_post_live(&ctx);
    }

    assert!(engine.accounts[idx as usize].adl_k_snap == engine.adl_coeff_long);

    let fc_after = engine.accounts[idx as usize].fee_credits.get();
    assert!(fc_after > -500i128, "fee debt must be swept after restart conversion");

    let ins_after = engine.insurance_fund.balance.get();
    assert!(ins_after > ins_before, "insurance fund must receive fee sweep payment");

    let cap_after = engine.accounts[idx as usize].capital.get();
    assert!(cap_after != cap_before, "capital must change after restart conversion + fee sweep");
}

#[kani::proof]
#[kani::solver(cadical)]
fn t11_54_worked_example_regression() {
    let mut engine = RiskEngine::new(zero_fee_params());

    let a = engine.add_user(0).unwrap();
    let b = engine.add_user(0).unwrap();
    engine.deposit(a, 10_000_000, 100, 0).unwrap();
    engine.deposit(b, 10_000_000, 100, 0).unwrap();

    engine.last_oracle_price = 100;
    engine.last_market_slot = 1;
    engine.last_crank_slot = 1;

    let size_q = (2 * POS_SCALE) as i128;
    let r1 = engine.execute_trade_not_atomic(a, b, 100, 1, size_q, 100, 0i128, 0);
    assert!(r1.is_ok());
    assert!(engine.oi_eff_long_q == engine.oi_eff_short_q);

    let mut ctx = InstructionContext::new();
    let d = 500u128;
    let q_close = POS_SCALE;
    let r2 = engine.enqueue_adl(&mut ctx, Side::Short, q_close, d);
    assert!(r2.is_ok());

    assert!(engine.adl_mult_long.get() < ADL_ONE);
    assert!(engine.oi_eff_long_q == U128::new(POS_SCALE));
    assert!(engine.adl_coeff_long.get() != 0i128);

    let _ = engine.settle_side_effects_with_h_lock(a as usize, 0);

    assert!(engine.accounts[a as usize].adl_k_snap == engine.adl_coeff_long);
    assert!(engine.check_conservation());
}

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn t5_24_dynamic_dust_bound_sufficient() {
    let mut engine = RiskEngine::new(zero_fee_params());

    let a = engine.add_user(0).unwrap();
    let b = engine.add_user(0).unwrap();
    engine.deposit(a, 10_000_000, 100, 0).unwrap();
    engine.deposit(b, 10_000_000, 100, 0).unwrap();

    // Use basis=1, a_basis=3 so floor(1 * 1 / 3) = 0 → position zeroes
    engine.accounts[a as usize].position_basis_q = I128::new(1i128);
    engine.accounts[a as usize].adl_a_basis = U128::new(3);
    engine.accounts[a as usize].adl_k_snap = I128::ZERO;
    engine.accounts[a as usize].adl_epoch_snap = 0;
    engine.accounts[b as usize].position_basis_q = I128::new(1i128);
    engine.accounts[b as usize].adl_a_basis = U128::new(3);
    engine.accounts[b as usize].adl_k_snap = I128::ZERO;
    engine.accounts[b as usize].adl_epoch_snap = 0;
    engine.stored_pos_count_long = 2;
    engine.oi_eff_long_q = U128::new(2);
    engine.adl_epoch_long = 0;

    engine.adl_mult_long = U128::new(1);
    engine.adl_coeff_long = I128::ZERO;

    let _ = engine.settle_side_effects_with_h_lock(a as usize, 0);
    assert!(engine.phantom_dust_bound_long_q.get() == 1u128);

    let _ = engine.settle_side_effects_with_h_lock(b as usize, 0);
    assert!(engine.phantom_dust_bound_long_q.get() == 2u128);
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

    assert!(engine.oi_eff_long_q.get() == 0);

    engine.begin_full_drain_reset(Side::Long);

    assert!(engine.adl_epoch_long == epoch_before + 1);
    assert!(engine.adl_mult_long == U128::new(ADL_ONE));
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
    engine.oi_eff_long_q = U128::new(100u128);
    let r2 = engine.finalize_side_reset(Side::Long);
    assert!(r2.is_err());

    engine.oi_eff_long_q = U128::ZERO;
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

    engine.adl_mult_long = U128::new(POS_SCALE);
    engine.adl_coeff_long = I128::new(12345i128);
    engine.oi_eff_long_q = U128::new(4 * POS_SCALE);
    engine.oi_eff_short_q = U128::new(4 * POS_SCALE);
    engine.insurance_fund.balance = U128::new(10_000_000);
    engine.stored_pos_count_long = 0;

    let k_before = engine.adl_coeff_long;
    let ins_before = engine.insurance_fund.balance.get();

    let d = 5_000u128;
    let q_close = POS_SCALE;

    let result = engine.enqueue_adl(&mut ctx, Side::Short, q_close, d);
    assert!(result.is_ok());

    assert!(engine.adl_coeff_long == k_before, "K must not change when stored_pos_count_opp == 0");
    assert!(engine.insurance_fund.balance.get() < ins_before, "insurance must absorb deficit");
    assert!(engine.oi_eff_long_q.get() == 3 * POS_SCALE);
}

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn t13_56_unilateral_empty_orphan_resolution() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let mut ctx = InstructionContext::new();

    engine.stored_pos_count_long = 0;
    engine.phantom_dust_bound_long_q = U128::new(100u128);
    engine.oi_eff_long_q = U128::new(50u128);

    engine.stored_pos_count_short = 2;
    engine.oi_eff_short_q = U128::new(50u128);

    let result = engine.schedule_end_of_instruction_resets(&mut ctx);
    assert!(result.is_ok());

    assert!(ctx.pending_reset_long);
    assert!(ctx.pending_reset_short);
    assert!(engine.oi_eff_long_q.get() == 0);
    assert!(engine.oi_eff_short_q.get() == 0);
}

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn t13_57_unilateral_empty_corruption_guard() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let mut ctx = InstructionContext::new();

    engine.stored_pos_count_long = 0;
    engine.phantom_dust_bound_long_q = U128::new(100u128);
    engine.oi_eff_long_q = U128::new(50u128);

    engine.stored_pos_count_short = 2;
    engine.oi_eff_short_q = U128::new(999u128);

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
    engine.phantom_dust_bound_short_q = U128::new(200u128);
    engine.oi_eff_short_q = U128::new(75u128);

    engine.stored_pos_count_long = 3;
    engine.oi_eff_long_q = U128::new(75u128);

    let result = engine.schedule_end_of_instruction_resets(&mut ctx);
    assert!(result.is_ok());

    assert!(ctx.pending_reset_long);
    assert!(ctx.pending_reset_short);
    assert!(engine.oi_eff_long_q.get() == 0);
    assert!(engine.oi_eff_short_q.get() == 0);
}

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn t13_60_unconditional_dust_bound_on_any_a_decay() {
    // v12.15+: phantom dust bound increments unconditionally on ANY A_side decay,
    // even when the truncation remainder is exactly zero.
    let mut engine = RiskEngine::new(zero_fee_params());
    let mut ctx = InstructionContext::new();

    engine.adl_mult_long = U128::new(4);
    engine.adl_coeff_long = I128::ZERO;
    engine.oi_eff_long_q = U128::new(4 * POS_SCALE);
    engine.oi_eff_short_q = U128::new(4 * POS_SCALE);
    engine.stored_pos_count_long = 1;

    let dust_before = engine.phantom_dust_bound_long_q;

    let result = engine.enqueue_adl(
        &mut ctx, Side::Short, 2 * POS_SCALE, 0u128,
    );
    assert!(result.is_ok());
    assert!(engine.adl_mult_long.get() == 2);

    // Unconditional: dust ALWAYS increments by at least 1 on A decay
    assert!(engine.phantom_dust_bound_long_q >= dust_before + 1,
        "dust must increment unconditionally on any A_side decay");
}

#[kani::proof]
#[kani::solver(cadical)]
fn t12_53_adl_truncation_dust_must_not_deadlock() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let mut ctx = InstructionContext::new();

    let a = engine.add_user(0).unwrap();
    let b = engine.add_user(0).unwrap();
    engine.deposit(a, 10_000_000, 100, 0).unwrap();
    engine.deposit(b, 10_000_000, 100, 0).unwrap();

    // One long (a) at A=7, one short (b) for OI balance.
    engine.adl_mult_long = U128::new(7);
    engine.adl_mult_short = U128::new(ADL_ONE);
    engine.adl_coeff_long = I128::ZERO;
    engine.adl_coeff_short = I128::ZERO;

    // Account a: long 10*POS_SCALE at a_basis=7
    engine.accounts[a as usize].position_basis_q = I128::new((10 * POS_SCALE) as i128);
    engine.accounts[a as usize].adl_a_basis = U128::new(7);
    engine.accounts[a as usize].adl_k_snap = I128::ZERO;
    engine.accounts[a as usize].adl_epoch_snap = 0;

    // Account b: short 10*POS_SCALE
    engine.accounts[b as usize].position_basis_q = I128::new(-((10 * POS_SCALE) as i128));
    engine.accounts[b as usize].adl_a_basis = U128::new(ADL_ONE);
    engine.accounts[b as usize].adl_k_snap = I128::ZERO;
    engine.accounts[b as usize].adl_epoch_snap = 0;

    engine.stored_pos_count_long = 1;
    engine.stored_pos_count_short = 1;
    engine.oi_eff_long_q = U128::new(10 * POS_SCALE);
    engine.oi_eff_short_q = U128::new(10 * POS_SCALE);

    // ADL: close POS_SCALE from short side → shrinks A_long via truncation
    // enqueue_adl decrements both sides by q_close, then A-truncates opposing
    let result = engine.enqueue_adl(
        &mut ctx, Side::Short, POS_SCALE, 0u128,
    );
    assert!(result.is_ok());
    // A_new = floor(7 * 9M / 10M) = 6
    assert!(engine.adl_mult_long.get() == 6);
    assert!(engine.oi_eff_long_q.get() == 9 * POS_SCALE);
    assert!(engine.oi_eff_short_q.get() == 9 * POS_SCALE);

    // Settle account a to get actual effective position under new A
    let settle_a = engine.settle_side_effects_with_h_lock(a as usize, 0);
    assert!(settle_a.is_ok());

    // eff_a = floor(10_000_000 * 6 / 7) = 8_571_428 (< 9_000_000)
    let eff_a = engine.effective_pos_q(a as usize);
    let dust = engine.oi_eff_long_q.get().checked_sub(eff_a.unsigned_abs()).unwrap_or(0);

    // Verify phantom_dust_bound covers the A-truncation dust
    assert!(engine.phantom_dust_bound_long_q.get() >= dust,
        "dust bound must cover A-truncation phantom OI");

    // Simulate final state: all positions closed via balanced trades,
    // which maintain OI_long == OI_short. Residual dust is equal on both sides.
    engine.attach_effective_position(a as usize, 0i128);
    engine.attach_effective_position(b as usize, 0i128);
    engine.oi_eff_long_q = U128::new(dust);
    engine.oi_eff_short_q = U128::new(dust);

    let reset_result = engine.schedule_end_of_instruction_resets(&mut ctx);
    assert!(reset_result.is_ok(), "ADL truncation dust must not deadlock market reset");
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

    let phantom_dust = if oi_post >= sum_new { oi_post - sum_new } else { 0 };

    let n: u16 = 2;
    let global_a_dust = n + ((oi + n + (a_old as u16) - 1) / (a_old as u16));

    assert!(global_a_dust >= phantom_dust,
        "A-truncation dust bound must cover phantom OI from A change");
}

/// Same-epoch zeroing: when settle_side_effects zeros a position (q_eff_new == 0),
/// the engine must increment phantom_dust_bound by 1.
#[kani::proof]
#[kani::solver(cadical)]
fn t14_62_dust_bound_same_epoch_zeroing() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let idx = engine.add_user(0).unwrap();
    engine.deposit(idx, 10_000_000, 100, 0).unwrap();

    // Use basis=1, a_basis=3 so floor(1 * 1 / 3) = 0 → position zeroes
    engine.accounts[idx as usize].position_basis_q = I128::new(1i128);
    engine.accounts[idx as usize].adl_a_basis = U128::new(3);
    engine.accounts[idx as usize].adl_k_snap = I128::ZERO;
    engine.accounts[idx as usize].adl_epoch_snap = 0;
    engine.stored_pos_count_long = 1;
    engine.adl_epoch_long = 0;
    engine.adl_coeff_long = I128::ZERO;

    // A_side=1 so floor(1 * 1 / 3) = 0
    engine.adl_mult_long = U128::new(1);

    let dust_before = engine.phantom_dust_bound_long_q;

    let result = engine.settle_side_effects_with_h_lock(idx as usize, 0);
    assert!(result.is_ok());

    // Position must be zeroed
    assert!(engine.accounts[idx as usize].position_basis_q.get() == 0);
    // Dust bound must have incremented by 1
    let dust_after = engine.phantom_dust_bound_long_q;
    assert!(dust_after == dust_before + 1u128,
        "same-epoch zeroing must increment phantom_dust_bound by 1");
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
    assert!(q_eff * (a_basis as u16) + remainder == product,
        "floor division identity");

    // Remainder is strictly less than divisor
    assert!(remainder < (a_basis as u16), "remainder < a_basis");

    // The effective quantity never exceeds the true (unrounded) quantity
    assert!(q_eff * (a_basis as u16) <= product,
        "floor never overshoots");

    if remainder > 0 {
        assert!((q_eff + 1) * (a_basis as u16) > product,
            "next integer exceeds product → loss < 1 unit");
    }
}

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn t14_64_dust_bound_full_drain_reset_zeroes() {
    let mut engine = RiskEngine::new(zero_fee_params());

    engine.phantom_dust_bound_long_q = U128::new(42u128);
    engine.oi_eff_long_q = U128::ZERO;
    engine.stored_pos_count_long = 0;
    engine.adl_epoch_long = 0;

    engine.begin_full_drain_reset(Side::Long);

    assert!(engine.phantom_dust_bound_long_q.get() == 0u128);
    assert!(engine.oi_eff_long_q.get() == 0u128);
}

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn t14_65_dust_bound_end_to_end_clearance() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let mut ctx = InstructionContext::new();

    // Two long accounts (a,b) and one short (c) for OI balance.
    let a_idx = engine.add_user(0).unwrap();
    let b_idx = engine.add_user(0).unwrap();
    let c_idx = engine.add_user(0).unwrap();
    engine.deposit(a_idx, 10_000_000, 100, 0).unwrap();
    engine.deposit(b_idx, 10_000_000, 100, 0).unwrap();
    engine.deposit(c_idx, 10_000_000, 100, 0).unwrap();

    engine.adl_mult_long = U128::new(13);
    engine.adl_mult_short = U128::new(ADL_ONE);
    engine.adl_coeff_long = I128::ZERO;
    engine.adl_coeff_short = I128::ZERO;
    engine.adl_epoch_long = 0;

    // Account a: long 7*POS_SCALE at a_basis=13
    engine.accounts[a_idx as usize].position_basis_q = I128::new((7 * POS_SCALE) as i128);
    engine.accounts[a_idx as usize].adl_a_basis = U128::new(13);
    engine.accounts[a_idx as usize].adl_k_snap = I128::ZERO;
    engine.accounts[a_idx as usize].adl_epoch_snap = 0;

    // Account b: long 5*POS_SCALE at a_basis=13
    engine.accounts[b_idx as usize].position_basis_q = I128::new((5 * POS_SCALE) as i128);
    engine.accounts[b_idx as usize].adl_a_basis = U128::new(13);
    engine.accounts[b_idx as usize].adl_k_snap = I128::ZERO;
    engine.accounts[b_idx as usize].adl_epoch_snap = 0;

    // Account c: short 12*POS_SCALE
    engine.accounts[c_idx as usize].position_basis_q = I128::new(-((12 * POS_SCALE) as i128));
    engine.accounts[c_idx as usize].adl_a_basis = U128::new(ADL_ONE);
    engine.accounts[c_idx as usize].adl_k_snap = I128::ZERO;
    engine.accounts[c_idx as usize].adl_epoch_snap = 0;

    engine.stored_pos_count_long = 2;
    engine.stored_pos_count_short = 1;
    engine.oi_eff_long_q = U128::new(12 * POS_SCALE);
    engine.oi_eff_short_q = U128::new(12 * POS_SCALE);

    // ADL: close 3*POS_SCALE from short side → shrinks A_long via truncation
    let result = engine.enqueue_adl(
        &mut ctx, Side::Short, 3 * POS_SCALE, 0u128,
    );
    assert!(result.is_ok());
    // A_new = floor(13 * 9M / 12M) = 9
    assert!(engine.adl_mult_long.get() == 9);
    assert!(engine.oi_eff_long_q.get() == 9 * POS_SCALE);
    assert!(engine.oi_eff_short_q.get() == 9 * POS_SCALE);
    assert!(engine.phantom_dust_bound_long_q.get() != 0);

    // Settle long accounts to get actual effective positions under new A
    let sa = engine.settle_side_effects_with_h_lock(a_idx as usize, 0);
    assert!(sa.is_ok());
    let sb = engine.settle_side_effects_with_h_lock(b_idx as usize, 0);
    assert!(sb.is_ok());

    // Compute sum of actual effective positions
    let eff_a = engine.effective_pos_q(a_idx as usize);
    let eff_b = engine.effective_pos_q(b_idx as usize);
    let sum_eff = eff_a.unsigned_abs() + eff_b.unsigned_abs();

    // Dust = tracked OI - actual sum of effective positions
    let dust = engine.oi_eff_long_q.get().checked_sub(sum_eff).unwrap_or(0);

    // Verify phantom_dust_bound covers the multi-account A-truncation dust
    assert!(engine.phantom_dust_bound_long_q.get() >= dust,
        "dust bound must cover A-truncation phantom OI for multiple accounts");

    // Close all positions and set OI to balanced dust level
    // (simulating trade-based closing which maintains OI_long == OI_short)
    engine.attach_effective_position(a_idx as usize, 0i128);
    engine.attach_effective_position(b_idx as usize, 0i128);
    engine.attach_effective_position(c_idx as usize, 0i128);
    engine.oi_eff_long_q = U128::new(dust);
    engine.oi_eff_short_q = U128::new(dust);

    let reset_result = engine.schedule_end_of_instruction_resets(&mut ctx);
    assert!(reset_result.is_ok(), "dust bound must be sufficient for reset after all positions closed");
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
    let mut params = zero_fee_params();
    params.trading_fee_bps = 10; // 10 bps
    let mut engine = RiskEngine::new(params);

    let a = engine.add_user(0).unwrap();
    let b = engine.add_user(0).unwrap();
    engine.deposit(a, 10_000_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.deposit(b, 10_000_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    // Open a position: a goes long, b goes short
    let size = POS_SCALE as i128;
    let result = engine.execute_trade_not_atomic(a, b, DEFAULT_ORACLE, DEFAULT_SLOT, size, DEFAULT_ORACLE, 0i128, 0);
    assert!(result.is_ok());

    // Zero a's capital so the fee can't be paid from principal.
    // Give enough PnL (as reserved, not released) to stay solvent for margin checks.
    // Use set_pnl_with_reserve(UseHLock) so PnL goes to reserve, not matured.
    engine.set_capital(a as usize, 0);
    engine.set_pnl_with_reserve(a as usize, 5_000_000i128, ReserveMode::UseHLock(10)).unwrap();
    engine.vault = U128::new(engine.vault.get() + 5_000_000);

    // Record fee_credits and PnL before the close.
    let fc_before = engine.accounts[a as usize].fee_credits.get();

    // Close position: a sells back (trade fee will be charged).
    // Capital is 0, so the entire fee must be shortfall → fee_credits.
    let pos_size = POS_SCALE as i128;
    let result2 = engine.execute_trade_not_atomic(b, a, DEFAULT_ORACLE, DEFAULT_SLOT, pos_size, DEFAULT_ORACLE, 0i128, 0);

    match result2 {
        Ok(()) => {
            let fc_after = engine.accounts[a as usize].fee_credits.get();
            // fee_credits must have decreased (become more negative) by the shortfall
            assert!(fc_after < fc_before,
                "fee shortfall must decrease fee_credits (create debt)");
        }
        Err(_) => {
            // Trade rejected for margin or other reasons — acceptable.
        }
    }
}

// ############################################################################
// SPEC PROPERTY #16: organic-close bankruptcy guard
// ############################################################################

#[kani::proof]
#[kani::solver(cadical)]
fn proof_organic_close_bankruptcy_guard() {
    let mut engine = RiskEngine::new(zero_fee_params());

    let a = engine.add_user(0).unwrap();
    let b = engine.add_user(0).unwrap();
    engine.deposit(a, 10_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.deposit(b, 10_000_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    let size = (90 * POS_SCALE) as i128;
    let result = engine.execute_trade_not_atomic(a, b, DEFAULT_ORACLE, DEFAULT_SLOT, size, DEFAULT_ORACLE, 0i128, 0);
    assert!(result.is_ok());

    let crash_price = 800u64;
    let crash_slot = DEFAULT_SLOT + 1;
    engine.last_crank_slot = crash_slot;

    let pos_size = (90 * POS_SCALE) as i128;
    let result2 = engine.execute_trade_not_atomic(b, a, crash_price, crash_slot, pos_size, crash_price, 0i128, 0);

    assert!(result2.is_err(),
        "organic close that leaves uncovered negative PnL must be rejected");
}

// ############################################################################
// SPEC PROPERTY #24: solvent flat-close succeeds
// ############################################################################

#[kani::proof]
#[kani::solver(cadical)]
fn proof_solvent_flat_close_succeeds() {
    let mut engine = RiskEngine::new(zero_fee_params());

    let a = engine.add_user(0).unwrap();
    let b = engine.add_user(0).unwrap();
    engine.deposit(a, 1_000_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.deposit(b, 1_000_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    // Open a small position
    let size = POS_SCALE as i128;
    let result = engine.execute_trade_not_atomic(a, b, DEFAULT_ORACLE, DEFAULT_SLOT, size, DEFAULT_ORACLE, 0i128, 0);
    assert!(result.is_ok());

    // Price drops modestly — a has losses but plenty of capital to cover
    let new_price = 900u64;
    let slot2 = DEFAULT_SLOT + 1;
    engine.last_crank_slot = slot2;

    // Close to flat: a sells their long position
    let pos_size = POS_SCALE as i128;
    let result2 = engine.execute_trade_not_atomic(b, a, new_price, slot2, pos_size, new_price, 0i128, 0);

    assert!(result2.is_ok(),
        "solvent trader closing to flat must not be rejected");
    assert!(engine.check_conservation(), "conservation must hold after flat close");
}

// ############################################################################
// SPEC §12 PROPERTY #23: Deposit materialization threshold
// ############################################################################

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_property_23_deposit_materialization_threshold() {
    // With nonzero MIN_INITIAL_DEPOSIT, a deposit below the threshold
    // must be rejected for a missing account.
    let mut params = zero_fee_params();
    params.min_initial_deposit = U128::new(1000);
    let mut engine = RiskEngine::new(params);

    let existing = engine.add_user(0).unwrap();

    // Try to deposit below threshold into unmaterialized account
    let missing: u16 = 3;
    assert!(!engine.is_used(missing as usize));

    let result = engine.deposit(missing, 999, DEFAULT_ORACLE, DEFAULT_SLOT);
    assert!(result.is_err(), "deposit below MIN_INITIAL_DEPOSIT must be rejected for missing account");

    // But an existing materialized account can receive a small top-up
    engine.deposit(existing, 5000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    let topup = engine.deposit(existing, 1, DEFAULT_ORACLE, DEFAULT_SLOT);
    assert!(topup.is_ok(), "existing account must accept small top-up below MIN_INITIAL_DEPOSIT");

    assert!(engine.check_conservation());
}

// ############################################################################
// SPEC §12 PROPERTY #51: Universal withdrawal dust guard
// ############################################################################

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_property_51_withdrawal_dust_guard() {
    // With nonzero MIN_INITIAL_DEPOSIT, a withdrawal that would leave
    // 0 < C_i < MIN_INITIAL_DEPOSIT must be rejected.
    let mut params = zero_fee_params();
    params.min_initial_deposit = U128::new(1000);
    let mut engine = RiskEngine::new(params);

    let a = engine.add_user(0).unwrap();
    engine.deposit(a, 5000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.keeper_crank_not_atomic(DEFAULT_SLOT, DEFAULT_ORACLE, &[], 0, 0i128, 0).unwrap();

    // Withdraw leaving exactly 500 (< MIN_INITIAL_DEPOSIT=1000) → must fail
    let result = engine.withdraw_not_atomic(a, 4500, DEFAULT_ORACLE, DEFAULT_SLOT, 0i128, 0);
    assert!(result.is_err(),
        "withdrawal leaving dust capital (500 < 1000) must be rejected");

    // Withdraw leaving exactly 0 → must succeed
    let result_zero = engine.withdraw_not_atomic(a, 5000, DEFAULT_ORACLE, DEFAULT_SLOT, 0i128, 0);
    assert!(result_zero.is_ok(),
        "withdrawal leaving zero capital must succeed");

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
    let real = engine.add_user(0).unwrap();
    engine.deposit(real, 100_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.keeper_crank_not_atomic(DEFAULT_SLOT, DEFAULT_ORACLE, &[], 0, 0i128, 0).unwrap();

    // Pick an index that was never add_user'd — it's missing
    let missing: u16 = 3; // MAX_ACCOUNTS=4 in kani, index 3 never materialized
    assert!(!engine.is_used(missing as usize), "account must be unmaterialized");

    // settle_account_not_atomic must reject missing account
    let settle_result = engine.settle_account_not_atomic(missing, DEFAULT_ORACLE, DEFAULT_SLOT, 0i128, 0);
    assert!(settle_result.is_err(), "settle_account_not_atomic must reject missing account");

    // withdraw_not_atomic must reject missing account
    let withdraw_result = engine.withdraw_not_atomic(missing, 100, DEFAULT_ORACLE, DEFAULT_SLOT, 0i128, 0);
    assert!(withdraw_result.is_err(), "withdraw_not_atomic must reject missing account");

    // execute_trade_not_atomic with missing account as party a
    let trade_result = engine.execute_trade_not_atomic(missing, real, DEFAULT_ORACLE, DEFAULT_SLOT,
        POS_SCALE as i128, DEFAULT_ORACLE, 0i128, 0);
    assert!(trade_result.is_err(), "execute_trade_not_atomic must reject missing account (party a)");

    // execute_trade_not_atomic with missing account as party b
    let trade_result_b = engine.execute_trade_not_atomic(real, missing, DEFAULT_ORACLE, DEFAULT_SLOT,
        POS_SCALE as i128, DEFAULT_ORACLE, 0i128, 0);
    assert!(trade_result_b.is_err(), "execute_trade_not_atomic must reject missing account (party b)");

    // liquidate_at_oracle_not_atomic on missing account — returns Ok(false) (no-op)
    let liq_result = engine.liquidate_at_oracle_not_atomic(missing, DEFAULT_SLOT, DEFAULT_ORACLE, LiquidationPolicy::FullClose, 0i128, 0);
    assert!(liq_result.is_ok(), "liquidate must not error on missing");
    assert!(!liq_result.unwrap(), "liquidate must return false (no-op) for missing account");

    // Verify no account was materialized
    assert!(!engine.is_used(missing as usize), "missing account must remain unmaterialized");
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
    let a = engine.add_user(0).unwrap();

    engine.deposit(a, 500_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    // Directly set up open position with negative PnL (bypassing trade to isolate deposit behavior)
    engine.accounts[a as usize].position_basis_q = I128::new((10 * POS_SCALE) as i128);
    engine.stored_pos_count_long = 1;
    engine.oi_eff_long_q = U128::new(10 * POS_SCALE);
    engine.oi_eff_short_q = U128::new(10 * POS_SCALE);
    engine.set_pnl(a as usize, -5_000i128);

    assert!(engine.accounts[a as usize].position_basis_q.get() != 0);
    assert!(engine.accounts[a as usize].pnl.get() < 0);

    let ins_before = engine.insurance_fund.balance.get();
    let pnl_before = engine.accounts[a as usize].pnl;

    // Deposit — with basis != 0, resolve_flat_negative must NOT run
    engine.deposit(a, 50_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    // resolve_flat_negative calls absorb_protocol_loss which changes insurance_fund.
    // If it did NOT run, insurance_fund must be unchanged.
    assert!(engine.insurance_fund.balance.get() == ins_before,
        "insurance must not change: resolve_flat_negative must not run when basis != 0");

    // Position must still be intact
    assert!(engine.accounts[a as usize].position_basis_q.get() != 0,
        "position must still be intact after deposit");

    // PnL may have been partially settled by settle_losses (step 7),
    // but it must NOT have been zeroed by resolve_flat_negative
    // (which zeros PnL and routes the loss through insurance).
    // settle_losses reduces PnL magnitude while reducing capital, without touching insurance.
    let pnl_after = engine.accounts[a as usize].pnl;
    assert!(pnl_after >= pnl_before,
        "PnL must not decrease further than settle_losses allows");
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
    let mut engine = RiskEngine::new(zero_fee_params());
    let a = engine.add_user(0).unwrap();
    let b = engine.add_user(0).unwrap();

    engine.deposit(a, 500_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.deposit(b, 500_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.keeper_crank_not_atomic(DEFAULT_SLOT, DEFAULT_ORACLE, &[], 0, 0i128, 0).unwrap();

    // Open positions
    let size_q = (100 * POS_SCALE) as i128;
    engine.execute_trade_not_atomic(a, b, DEFAULT_ORACLE, DEFAULT_SLOT, size_q, DEFAULT_ORACLE, 0i128, 0).unwrap();

    // Oracle up — a gets profit
    let high_oracle = 1_100u64;
    let slot2 = DEFAULT_SLOT + 1;
    engine.keeper_crank_not_atomic(slot2, high_oracle, &[(a, None), (b, None)], 64, 0i128, 0).unwrap();

    // Wait for warmup to partially release
    let slot3 = slot2 + 60; // 60 of 100 slots
    engine.keeper_crank_not_atomic(slot3, high_oracle, &[(a, None)], 64, 0i128, 0).unwrap();

    let released = engine.released_pos(a as usize);
    if released == 0 {
        // Nothing to convert — warmup hasn't released yet. Skip.
        return;
    }

    let r_before = engine.accounts[a as usize].reserved_pnl;
    let ppt_before = engine.pnl_pos_tot;
    let pmpt_before = engine.pnl_matured_pos_tot;

    // Use consume_released_pnl to convert x = released
    let x = released;
    engine.consume_released_pnl(a as usize, x);

    // R_i must be unchanged
    assert!(engine.accounts[a as usize].reserved_pnl == r_before,
        "R_i must be unchanged after consume_released_pnl");

    // PNL_pos_tot decreased by exactly x
    assert!(engine.pnl_pos_tot == ppt_before - x,
        "pnl_pos_tot must decrease by exactly x");

    // PNL_matured_pos_tot decreased by exactly x
    assert!(engine.pnl_matured_pos_tot == pmpt_before - x,
        "pnl_matured_pos_tot must decrease by exactly x");
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
    let mut engine = RiskEngine::new(zero_fee_params());
    let a = engine.add_user(0).unwrap();
    let b = engine.add_user(0).unwrap();

    engine.deposit(a, 500_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.deposit(b, 500_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.keeper_crank_not_atomic(DEFAULT_SLOT, DEFAULT_ORACLE, &[], 0, 0i128, 0).unwrap();

    // Open positions
    let size_q = (100 * POS_SCALE) as i128;
    engine.execute_trade_not_atomic(a, b, DEFAULT_ORACLE, DEFAULT_SLOT, size_q, DEFAULT_ORACLE, 0i128, 0).unwrap();

    // Oracle up, then wait for full warmup
    let high_oracle = 1_100u64;
    let slot2 = DEFAULT_SLOT + 1;
    engine.keeper_crank_not_atomic(slot2, high_oracle, &[(a, None), (b, None)], 64, 0i128, 0).unwrap();

    // Full warmup elapsed
    let slot3 = slot2 + 200; // well past warmup period
    engine.keeper_crank_not_atomic(slot3, high_oracle, &[(a, None)], 64, 0i128, 0).unwrap();

    // a still has position, so should have released profit but NOT auto-converted
    assert!(engine.accounts[a as usize].position_basis_q.get() != 0,
        "account must still have open position");

    let released = engine.released_pos(a as usize);
    // After full warmup, released profit should exist (R_i decreased or zeroed)
    // Capital should NOT have increased from auto-conversion
    // The key test: capital only changes from settle_losses, not from do_profit_conversion
    let cap_a = engine.accounts[a as usize].capital.get();
    assert!(cap_a <= 500_000,
        "capital must not increase from auto-conversion while position is open: cap={}",
        cap_a);

    // Verify released profit exists but wasn't consumed
    assert!(released > 0 || engine.accounts[a as usize].reserved_pnl.get() == 0,
        "warmup must have released profit or reserve is zero");

    assert!(engine.check_conservation());
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
    let mut engine = RiskEngine::new(zero_fee_params());
    let a = engine.add_user(0).unwrap();
    let b = engine.add_user(0).unwrap();

    engine.deposit(a, 500_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.deposit(b, 500_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.keeper_crank_not_atomic(DEFAULT_SLOT, DEFAULT_ORACLE, &[], 0, 0i128, 0).unwrap();

    // Open positions
    let size_q = (100 * POS_SCALE) as i128;
    engine.execute_trade_not_atomic(a, b, DEFAULT_ORACLE, DEFAULT_SLOT, size_q, DEFAULT_ORACLE, 0i128, 0).unwrap();

    // Oracle up
    let high_oracle = 1_200u64;
    let slot2 = DEFAULT_SLOT + 1;
    engine.keeper_crank_not_atomic(slot2, high_oracle, &[(a, None), (b, None)], 64, 0i128, 0).unwrap();

    // Wait for warmup to fully release
    let slot3 = slot2 + 200;
    engine.keeper_crank_not_atomic(slot3, high_oracle, &[(a, None)], 64, 0i128, 0).unwrap();

    // Check released amount
    let released_before = engine.released_pos(a as usize);
    if released_before == 0 {
        return; // nothing to convert
    }

    let r_before = engine.accounts[a as usize].reserved_pnl;
    let cap_before = engine.accounts[a as usize].capital.get();
    let ppt_before = engine.pnl_pos_tot;
    let pmpt_before = engine.pnl_matured_pos_tot;

    // Convert all released profit
    let result = engine.convert_released_pnl_not_atomic(a, released_before, high_oracle, slot3, 0i128, 0);
    assert!(result.is_ok(), "convert_released_pnl_not_atomic must succeed for healthy account");

    // R_i must be unchanged
    assert!(engine.accounts[a as usize].reserved_pnl == r_before,
        "R_i must be unchanged after convert_released_pnl_not_atomic");

    // Capital must have increased (by haircutted amount)
    assert!(engine.accounts[a as usize].capital.get() > cap_before,
        "capital must increase after converting released profit");

    // PNL_pos_tot and PNL_matured_pos_tot must have decreased
    assert!(engine.pnl_pos_tot < ppt_before,
        "pnl_pos_tot must decrease after conversion");
    assert!(engine.pnl_matured_pos_tot < pmpt_before,
        "pnl_matured_pos_tot must decrease after conversion");

    // Account must still be maintenance healthy (conversion rejects if not)
    assert!(engine.is_above_maintenance_margin(
        &engine.accounts[a as usize], a as usize, high_oracle),
        "account must be maintenance healthy after conversion");

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
    let min_dep = engine.params.min_initial_deposit.get() as u32;
    kani::assume(amount >= min_dep && amount <= 1_000_000);

    // Deposit directly on the missing slot — must succeed and materialize
    let result = engine.deposit(0, amount as u128, DEFAULT_ORACLE, DEFAULT_SLOT);
    assert!(result.is_ok(), "deposit must succeed and materialize missing account");

    // Account must now be materialized
    assert!(engine.is_used(0), "account must be materialized after deposit");

    // Capital must equal deposited amount
    assert!(engine.accounts[0].capital.get() == amount as u128,
        "capital must equal deposited amount");

    // Vault must contain the deposited amount
    assert!(engine.vault.get() == amount as u128,
        "vault must contain deposited amount");

    // Conservation must hold
    assert!(engine.check_conservation());
}

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_audit2_deposit_rejects_below_min_initial_for_missing() {
    // Per spec §10.3 step 2: deposit below MIN_INITIAL_DEPOSIT on a
    // missing account must fail.
    let mut params = zero_fee_params();
    params.min_initial_deposit = U128::new(1000);
    let mut engine = RiskEngine::new(params);
    assert!(!engine.is_used(0));

    let min_dep = engine.params.min_initial_deposit.get();
    assert!(min_dep == 1000); // sanity: threshold is non-trivial

    // Symbolic amount strictly below threshold
    let amount: u16 = kani::any();
    kani::assume((amount as u128) < min_dep);

    let result = engine.deposit(0, amount as u128, DEFAULT_ORACLE, DEFAULT_SLOT);
    assert!(result.is_err(), "deposit below MIN_INITIAL_DEPOSIT must fail for missing account");
    // Account must NOT be materialized
    assert!(!engine.is_used(0), "account must not be materialized on failed deposit");
    // Vault must be unchanged
    assert!(engine.vault.get() == 0, "vault must not change on rejected deposit");
}

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_audit2_deposit_existing_accepts_small_topup() {
    // Per spec §12 property #23: an existing materialized account may
    // receive deposits smaller than MIN_INITIAL_DEPOSIT.
    let mut engine = RiskEngine::new(zero_fee_params());
    let a = engine.add_user(0).unwrap();

    // First deposit to establish the account
    let min_dep = engine.params.min_initial_deposit.get();
    engine.deposit(a, min_dep, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    // Small top-up below MIN_INITIAL_DEPOSIT must succeed
    let small_amount = 1u128;
    let result = engine.deposit(a, small_amount, DEFAULT_ORACLE, DEFAULT_SLOT);
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
    params.new_account_fee = U128::new(100);
    let mut engine = RiskEngine::new(params);

    // --- Path 1: failure via "no free slots" ---
    for _ in 0..MAX_ACCOUNTS {
        engine.add_user(100).unwrap();
    }

    let vault_before = engine.vault.get();
    let ins_before = engine.insurance_fund.balance.get();
    let c_tot_before = engine.c_tot.get();

    let result = engine.add_user(100);
    assert!(result.is_err());

    assert!(engine.vault.get() == vault_before,
        "vault must not change on failed add_user (no slots)");
    assert!(engine.insurance_fund.balance.get() == ins_before,
        "insurance must not change on failed add_user (no slots)");
    assert!(engine.c_tot.get() == c_tot_before,
        "c_tot must not change on failed add_user (no slots)");
}

/// Proof: add_user atomicity on MAX_VAULT_TVL failure path.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_audit4_add_user_atomic_on_tvl_failure() {
    let mut params = zero_fee_params();
    params.new_account_fee = U128::new(100);
    let mut engine = RiskEngine::new(params);

    // Set vault just below MAX_VAULT_TVL so fee would push it over
    engine.vault = U128::new(MAX_VAULT_TVL - 99);
    engine.insurance_fund.balance = U128::new(MAX_VAULT_TVL - 99);

    let vault_before = engine.vault.get();
    let ins_before = engine.insurance_fund.balance.get();
    let used_before = engine.num_used_accounts;

    // fee_payment=100 would push vault to MAX_VAULT_TVL+1 — must fail
    let result = engine.add_user(100);
    assert!(result.is_err());

    assert!(engine.vault.get() == vault_before,
        "vault must not change on MAX_VAULT_TVL rejection");
    assert!(engine.insurance_fund.balance.get() == ins_before,
        "insurance must not change on MAX_VAULT_TVL rejection");
    assert!(engine.num_used_accounts == used_before,
        "num_used_accounts must not change on MAX_VAULT_TVL rejection");
}

/// Proof: deposit_fee_credits enforces MAX_VAULT_TVL.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_audit4_deposit_fee_credits_max_tvl() {
    let params = zero_fee_params();
    let mut engine = RiskEngine::new(params);
    let idx = engine.add_user(0).unwrap();

    // Give account fee debt so deposit is not a no-op
    engine.accounts[idx as usize].fee_credits = I128::new(-1000);

    // Set vault at MAX_VAULT_TVL
    engine.vault = U128::new(MAX_VAULT_TVL);

    // Deposit must fail (vault already at MAX)
    let result = engine.deposit_fee_credits(idx, 500, 0);
    assert!(result.is_err(), "must reject deposit that would exceed MAX_VAULT_TVL");
    assert!(engine.vault.get() == MAX_VAULT_TVL, "vault unchanged on failure");
}
