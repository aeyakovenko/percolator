//! Section 4 — A/K refinement, events, settlement
//!
//! One-event A/K semantics, composition, epoch settlement, non-compounding.

#![cfg(kani)]

mod common;
use common::*;

// ############################################################################
// T1: ONE-EVENT A/K SEMANTICS
// ############################################################################

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn t1_7_adl_quantity_only_lazy_conservative() {
    let q_base: u8 = kani::any();
    kani::assume(q_base > 0 && q_base <= 15);
    let oi: u8 = kani::any();
    kani::assume(oi > 0 && oi <= 15);
    let q_close: u8 = kani::any();
    kani::assume(q_close > 0 && q_close <= oi);
    let oi_post = oi - q_close;

    let a_old = S_ADL_ONE;
    let basis_q = (q_base as u16) * S_POS_SCALE;

    let eager_q = ((q_base as u16) * (oi_post as u16)) / (oi as u16);

    let a_new = a_after_adl(a_old, oi_post as u16, oi as u16);
    let lazy_q = lazy_eff_q(basis_q, a_new, a_old);
    let lazy_q_base = lazy_q / S_POS_SCALE;

    assert!(lazy_q_base <= eager_q, "ADL lazy must not exceed eager quantity");
    assert!(eager_q - lazy_q_base <= 1, "ADL lazy error must be bounded by 1 base unit");
}

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn t1_8_adl_deficit_only_lazy_equals_eager() {
    let q_base: u8 = kani::any();
    kani::assume(q_base > 0 && q_base <= 15);
    let oi: u8 = kani::any();
    kani::assume(oi > 0 && oi <= 15);
    let d: u8 = kani::any();
    kani::assume(d > 0 && d <= 15);

    let a_side = S_ADL_ONE;
    let k_init: i32 = 0;
    let basis_q = (q_base as u16) * S_POS_SCALE;

    let eager_loss = ((q_base as i32) * (d as i32)) / (oi as i32);

    let delta_k_abs = ((d as u16) * (a_side as u16) + (oi as u16) - 1) / (oi as u16);
    let delta_k = -(delta_k_abs as i32);
    let k_after = k_init + delta_k;
    let k_diff = k_after - k_init;

    let lazy_loss_raw = lazy_pnl(basis_q, k_diff, a_side);

    let lazy_loss = -lazy_loss_raw;
    assert!(lazy_loss >= eager_loss, "ADL deficit lazy must be at least as large as eager");
    assert!(lazy_loss <= eager_loss + (q_base as i32),
        "ADL deficit lazy overshoot must be bounded by q_base");
}

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn t1_9_adl_quantity_plus_deficit_lazy_conservative() {
    let q_base: u8 = kani::any();
    kani::assume(q_base > 0 && q_base <= 15);
    let oi: u8 = kani::any();
    kani::assume(oi > 0 && oi >= q_base && oi <= 15);
    let q_close: u8 = kani::any();
    kani::assume(q_close > 0 && q_close <= oi);
    let d: u8 = kani::any();
    kani::assume(d > 0 && d <= 15);

    let oi_post = oi - q_close;
    let a_old = S_ADL_ONE;
    let basis_q = (q_base as u16) * S_POS_SCALE;

    let eager_q = ((q_base as u16) * (oi_post as u16)) / (oi as u16);

    let a_new = a_after_adl(a_old, oi_post as u16, oi as u16);
    let lazy_q = lazy_eff_q(basis_q, a_new, a_old) / S_POS_SCALE;

    assert!(lazy_q <= eager_q, "lazy must not exceed eager quantity");
    assert!(eager_q - lazy_q <= 1, "lazy error bounded by 1 base unit");

    let delta_k_abs = ((d as u16) * (a_old as u16) + (oi as u16) - 1) / (oi as u16);
    let delta_k = -(delta_k_abs as i32);
    let lazy_loss = -lazy_pnl(basis_q, delta_k, a_old);
    let eager_loss = ((q_base as i32) * (d as i32)) / (oi as i32);

    assert!(lazy_loss >= eager_loss, "ADL PnL: lazy loss must be >= eager loss (conservative)");
    assert!(lazy_loss <= eager_loss + (q_base as i32),
        "ADL PnL: lazy overshoot must be bounded by q_base");
}

// ============================================================================
// T1.8b: symbolic a_basis generalization
// ============================================================================

#[kani::proof]
#[kani::unwind(1)]
#[kani::solver(cadical)]
fn t1_8b_adl_deficit_lazy_conservative_symbolic_a_basis() {
    let q_base: u8 = kani::any();
    kani::assume(q_base > 0 && q_base <= 15);
    let oi: u8 = kani::any();
    kani::assume(oi > 0 && oi <= 15);
    let d: u8 = kani::any();
    kani::assume(d > 0 && d <= 15);

    let a_basis: u16 = kani::any();
    kani::assume(a_basis > 0 && a_basis <= S_ADL_ONE);

    let k_init: i32 = 0;
    let basis_q = (q_base as u16) * S_POS_SCALE;

    let eager_loss = ((q_base as i32) * (d as i32)) / (oi as i32);

    let delta_k_abs = ((d as u16) * (a_basis as u16) + (oi as u16) - 1) / (oi as u16);
    let delta_k = -(delta_k_abs as i32);
    let lazy_loss_raw = lazy_pnl(basis_q, delta_k, a_basis);

    let lazy_loss = -lazy_loss_raw;
    assert!(lazy_loss >= eager_loss,
        "ADL deficit lazy must be at least as large as eager for symbolic a_basis");
}

// ############################################################################
// T2: COMPOSITION PROOFS
// ############################################################################

#[kani::proof]
#[kani::unwind(1)]
#[kani::solver(cadical)]
fn t2_12_floor_shift_lemma() {
    let n: i8 = kani::any();
    let m: i8 = kani::any();
    let d: u8 = kani::any();
    kani::assume(d > 0);

    let d32 = d as i32;
    let n32 = n as i32;
    let m32 = m as i32;
    let shifted = n32 + m32 * d32;

    let floor_n = if n32 >= 0 { n32 / d32 } else { -((-n32 + d32 - 1) / d32) };
    let floor_shifted = if shifted >= 0 { shifted / d32 } else { -((-shifted + d32 - 1) / d32) };

    assert!(floor_shifted == floor_n + m32,
        "floor(n + m*d, d) must equal floor(n, d) + m");
}

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn t2_12_fold_step_case() {
    let q_base: u8 = kani::any();
    kani::assume(q_base > 0);
    let dp: i8 = kani::any();
    let a = S_ADL_ONE;
    let den = (a as i32) * (S_POS_SCALE as i32);
    let basis_q = (q_base as u16) * S_POS_SCALE;

    let exact = (basis_q as i32) * (a as i32);
    assert!(exact % den == 0, "basis_q * A must be divisible by den");
    assert!(exact / den == q_base as i32, "quotient must equal q_base");

    let k_prefix: i8 = kani::any();
    let k_new = (k_prefix as i32) + (a as i32) * (dp as i32);
    let eager_step = (q_base as i32) * (dp as i32);
    let lazy_total = lazy_pnl(basis_q, k_new, a);
    let lazy_prefix = lazy_pnl(basis_q, k_prefix as i32, a);
    let lazy_step = lazy_total - lazy_prefix;

    assert!(lazy_step == eager_step, "fold step: lazy increment must equal eager step");
}

// ############################################################################
// T2.14: COMPOSITION ACROSS A-CHANGING EVENT (mark → ADL → mark)
// ############################################################################
//
// Spec §5.1 says events compose: A_new = A_old * α, K_new = K_old + A_old * β.
// This test verifies the algebraic identity when A is modified by an ADL event
// between two mark events. None of the existing t2_1x proofs change A.

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn t2_14_compose_mark_adl_mark() {
    // Symbolic inputs — small-model scale
    let q_base: u8 = kani::any();
    kani::assume(q_base > 0 && q_base <= 10);
    let dp1: i8 = kani::any();
    kani::assume(dp1 >= -10 && dp1 <= 10);
    let dp2: i8 = kani::any();
    kani::assume(dp2 >= -10 && dp2 <= 10);

    // ADL parameters: remove q_adl from OI, shrinking A
    let oi: u8 = kani::any();
    kani::assume(oi >= 2 && oi <= 15);
    let q_adl: u8 = kani::any();
    kani::assume(q_adl > 0 && q_adl < oi);
    let oi_post = oi - q_adl;

    let a0 = S_ADL_ONE;
    let basis_q = (q_base as u16) * S_POS_SCALE;

    // Eager sequence: mark1 → ADL → mark2
    // Mark 1: PnL = q_base * dp1
    let eager_mark1 = (q_base as i32) * (dp1 as i32);

    // ADL: A changes from a0 to a_new = floor(a0 * oi_post / oi)
    let a_new = a_after_adl(a0, oi_post as u16, oi as u16);
    kani::assume(a_new > 0);

    // After ADL, effective q shrinks: q_eff_new = floor(basis_q * a_new / a0)
    let q_eff_new = lazy_eff_q(basis_q, a_new, a0);
    kani::assume(q_eff_new > 0);

    // Mark 2: idealized eager PnL = floor(q_base * a_new * dp2 / a0)
    // No intermediate position rounding — matches how K accumulates in the lazy model
    let mark2_num = (q_base as i32) * (a_new as i32) * (dp2 as i32);
    let mark2_den = a0 as i32;
    let eager_mark2 = if mark2_num >= 0 {
        mark2_num / mark2_den
    } else {
        let abs_num = -mark2_num;
        -((abs_num + mark2_den - 1) / mark2_den)
    };
    let eager_total = eager_mark1 + eager_mark2;

    // Lazy sequence: K accumulates both marks, but the ADL changes A mid-stream
    let k0: i32 = 0;
    // K after first mark (at A = a0)
    let k1 = k_after_mark_long(k0, a0, dp1 as i32);
    // K after ADL: K doesn't change from ADL quantity socialization with D=0
    let k_after_adl = k1;
    // K after second mark (at A = a_new, post-ADL)
    let k2 = k_after_mark_long(k_after_adl, a_new, dp2 as i32);

    // Lazy PnL: evaluate at original basis with k_diff = k2 - k0
    let k_diff = k2 - k0;
    let lazy_total = lazy_pnl(basis_q, k_diff, a0);

    assert!(eager_total == lazy_total,
        "composition across A-changing ADL event: eager != lazy");
}

// ############################################################################
// T3: EPOCH SETTLEMENT (subset)
// ############################################################################

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn t3_14_epoch_mismatch_forces_terminal_close() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let idx = engine.add_user(0).unwrap();
    engine.deposit(idx, 1_000_000, 100, 0).unwrap();

    let pos_mul: u8 = kani::any();
    kani::assume(pos_mul > 0);
    let pos = (POS_SCALE * (pos_mul as u128)) as i128;
    engine.accounts[idx as usize].position_basis_q = I128::new(pos);
    engine.accounts[idx as usize].adl_a_basis = U128::new(ADL_ONE);
    let k_snap_val: i8 = kani::any();
    let k_snap = k_snap_val as i128;
    engine.accounts[idx as usize].adl_k_snap = I128::new(k_snap);
    engine.accounts[idx as usize].adl_epoch_snap = 0;
    engine.stored_pos_count_long = 1;

    // Use a DIFFERENT k_epoch_start so k_diff is non-trivial (not always 0)
    let k_start_val: i8 = kani::any();
    let k_epoch_start = k_start_val as i128;

    engine.adl_epoch_long = 1;
    engine.adl_epoch_start_k_long = I128::new(k_epoch_start);
    engine.side_mode_long = SideMode::ResetPending;
    engine.stale_account_count_long = 1;

    let pnl_before = engine.accounts[idx as usize].pnl;

    let result = engine.settle_side_effects_with_h_lock(idx as usize, 0);
    assert!(result.is_ok());

    assert!(engine.accounts[idx as usize].position_basis_q.get() == 0);
    assert!(engine.stale_account_count_long == 0);
    // Canonical zero-position defaults: epoch_snap = 0 (spec §2.4)
    assert!(engine.accounts[idx as usize].adl_epoch_snap == 0);

    // PnL assertion: the settlement must credit the correct amount
    let abs_basis = pos as u128;
    let den = ADL_ONE * POS_SCALE;
    let expected_pnl_delta = wide_signed_mul_div_floor_from_k_pair(abs_basis, k_snap, k_epoch_start, den);
    assert!(engine.accounts[idx as usize].pnl == pnl_before + expected_pnl_delta,
        "epoch mismatch PnL must match wide_signed_mul_div_floor_from_k_pair");
}

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn t3_14b_epoch_mismatch_with_nonzero_k_diff() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let idx = engine.add_user(0).unwrap();
    engine.deposit(idx, 10_000_000, 100, 0).unwrap();

    let pos = POS_SCALE as i128;
    engine.accounts[idx as usize].position_basis_q = I128::new(pos);
    engine.accounts[idx as usize].adl_a_basis = U128::new(ADL_ONE);
    engine.accounts[idx as usize].adl_epoch_snap = 0;
    engine.stored_pos_count_long = 1;

    let k_snap_val: i8 = kani::any();
    let k_snap = k_snap_val as i128;
    engine.accounts[idx as usize].adl_k_snap = I128::new(k_snap);

    let k_diff_val: i8 = kani::any();
    kani::assume(k_diff_val != 0);
    let k_epoch_start_val = (k_snap_val as i16) + (k_diff_val as i16);
    kani::assume(k_epoch_start_val >= -120 && k_epoch_start_val <= 120);
    let k_epoch_start = k_epoch_start_val as i128;

    engine.adl_coeff_long = I128::ZERO;
    engine.adl_epoch_long = 1;
    engine.adl_epoch_start_k_long = I128::new(k_epoch_start);
    engine.side_mode_long = SideMode::ResetPending;
    engine.stale_account_count_long = 1;

    let pnl_before = engine.accounts[idx as usize].pnl;

    let result = engine.settle_side_effects_with_h_lock(idx as usize, 0);
    assert!(result.is_ok());

    assert!(engine.accounts[idx as usize].position_basis_q.get() == 0);
    assert!(engine.stale_account_count_long == 0);
    // Canonical zero-position defaults: epoch_snap = 0 (spec §2.4)
    assert!(engine.accounts[idx as usize].adl_epoch_snap == 0);

    // PnL assertion: the settlement must credit the correct amount
    let abs_basis = pos as u128;
    let den = ADL_ONE * POS_SCALE;
    let expected_pnl_delta = wide_signed_mul_div_floor_from_k_pair(abs_basis, k_snap, k_epoch_start, den);
    assert!(engine.accounts[idx as usize].pnl == pnl_before + expected_pnl_delta,
        "epoch mismatch PnL must match wide_signed_mul_div_floor_from_k_pair");
}

// ############################################################################
// T7: NON-COMPOUNDING BASIS PROOFS
// ############################################################################

#[kani::proof]
#[kani::unwind(1)]
#[kani::solver(cadical)]
fn t7_28a_noncompounding_floor_inequality_correct_direction() {
    let basis: u8 = kani::any();
    kani::assume(basis > 0 && basis <= 15);
    let a_basis: u8 = kani::any();
    kani::assume(a_basis > 0 && a_basis <= 15);

    let k1: i8 = kani::any();
    kani::assume(k1 >= -15 && k1 <= 15);
    let k2_delta: i8 = kani::any();
    kani::assume(k2_delta >= -15 && k2_delta <= 15);
    let k2_val = (k1 as i16) + (k2_delta as i16);
    kani::assume(k2_val >= -15 && k2_val <= 15);

    const S_POS_SCALE_LOCAL: i32 = 4;
    let den = (a_basis as i32) * S_POS_SCALE_LOCAL;
    kani::assume(den > 0);

    let floor_div = |num: i32, d: i32| -> i32 {
        if num >= 0 { num / d } else { (num - d + 1) / d }
    };

    let pnl_1 = floor_div((basis as i32) * (k1 as i32), den);
    let pnl_2 = floor_div((basis as i32) * (k2_delta as i32), den);
    let total_two_touch = pnl_1 + pnl_2;

    let pnl_single = floor_div((basis as i32) * (k2_val as i32), den);

    assert!(total_two_touch <= pnl_single,
        "two-touch sum must be <= single-touch (floor splits lose fractional parts)");
    assert!(pnl_single <= total_two_touch + 1,
        "single-touch must be at most 1 unit above two-touch sum");
}

#[kani::proof]
#[kani::unwind(1)]
#[kani::solver(cadical)]
fn t7_28b_noncompounding_exact_additivity_divisible_increments() {
    let basis: u8 = kani::any();
    kani::assume(basis > 0 && basis <= 12);
    // In the real engine, position_basis_q is always POS_SCALE-aligned
    kani::assume(basis % 4 == 0);
    let a_basis: u8 = kani::any();
    kani::assume(a_basis > 0 && a_basis <= 15);

    let dp1: i8 = kani::any();
    kani::assume(dp1 >= -15 && dp1 <= 15);
    let dp2: i8 = kani::any();
    kani::assume(dp2 >= -15 && dp2 <= 15);
    let dp_total = (dp1 as i16) + (dp2 as i16);
    kani::assume(dp_total >= -15 && dp_total <= 15);

    const S_POS_SCALE_LOCAL: i32 = 4;
    let den = (a_basis as i32) * S_POS_SCALE_LOCAL;
    kani::assume(den > 0);

    let k1 = (a_basis as i32) * (dp1 as i32);
    let k2_delta = (a_basis as i32) * (dp2 as i32);
    let k_total = (a_basis as i32) * (dp_total as i32);

    let floor_div = |num: i32, d: i32| -> i32 {
        if num >= 0 { num / d } else { (num - d + 1) / d }
    };

    let pnl_1 = floor_div((basis as i32) * k1, den);
    let pnl_2 = floor_div((basis as i32) * k2_delta, den);
    let total_two_touch = pnl_1 + pnl_2;

    let pnl_single = floor_div((basis as i32) * k_total, den);

    assert!(total_two_touch == pnl_single,
        "exact additivity when K increments are multiples of a_basis");
}

// ############################################################################
// T6: FOCUSED SCENARIO PROOFS (REGRESSIONS)
// ############################################################################

#[kani::proof]
#[kani::unwind(1)]
#[kani::solver(cadical)]
fn t6_24_worked_example_regression() {
    let a_init = S_ADL_ONE;
    let pos_scale = S_POS_SCALE;

    let q_l1: u16 = 8;
    let basis_l1 = q_l1 * pos_scale;
    let a_basis_l1 = a_init;
    let k_snap_l1: i32 = 0;

    let oi: u16 = 8;
    let mut k_long: i32 = 0;
    let a_long = a_init;

    let dp = 10i32;
    k_long = k_after_mark_long(k_long, a_long, dp);

    let l1_pnl_pre = lazy_pnl(basis_l1, k_long - k_snap_l1, a_basis_l1);
    assert!(l1_pnl_pre == 80, "L1 pre-ADL PnL should be 80");

    let q_close: u16 = 5;
    let d: u16 = 2;
    let oi_post = oi - q_close;
    assert!(oi_post > 0);

    let delta_k_abs = ((d as u16) * (a_long as u16) + (oi as u16) - 1) / (oi as u16);
    assert!(delta_k_abs == 64);
    let delta_k = -(delta_k_abs as i32);
    k_long = k_long + delta_k;

    let a_long_new = a_after_adl(a_long, oi_post, oi);
    assert!(a_long_new == 96);

    let k_diff = k_long - k_snap_l1;
    let q_eff = lazy_eff_q(basis_l1, a_long_new, a_basis_l1);
    assert!(q_eff == 12, "L1 effective quantity after ADL");
    let l1_pnl_post = lazy_pnl(basis_l1, k_diff, a_basis_l1);
    assert!(l1_pnl_post == 78, "L1 post-ADL PnL includes deficit");

    assert!(l1_pnl_post < l1_pnl_pre, "deficit must reduce PnL");
    assert!(l1_pnl_post > 0, "PnL still positive from mark gain");
}

#[kani::proof]
#[kani::unwind(1)]
#[kani::solver(cadical)]
fn t6_25_pure_pnl_bankruptcy_regression() {
    let oi: u8 = kani::any();
    kani::assume(oi > 0);
    let d: u8 = kani::any();
    kani::assume(d > 0);
    let q_base: u8 = kani::any();
    kani::assume(q_base > 0 && q_base <= oi);

    let a_opp = S_ADL_ONE;
    let basis_q = (q_base as u16) * S_POS_SCALE;

    let delta_k_abs = ((d as u16) * (a_opp as u16) + (oi as u16) - 1) / (oi as u16);
    assert!(delta_k_abs > 0);

    let delta_k = -(delta_k_abs as i32);
    assert!(delta_k < 0);

    let pnl = lazy_pnl(basis_q, delta_k, a_opp);
    assert!(pnl <= 0);

    let eager_loss = ((q_base as i32) * (d as i32)) / (oi as i32);
    assert!(-pnl >= eager_loss, "lazy loss must be >= eager floor loss (conservative)");
}

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn t6_26_full_drain_reset_regression() {
    let mut engine = RiskEngine::new(zero_fee_params());

    let idx = engine.add_user(0).unwrap();
    engine.deposit(idx, 1_000_000, 100, 0).unwrap();

    let k_snap_val: i8 = kani::any();
    let k_snap = k_snap_val as i128;
    let pos_mul: u8 = kani::any();
    kani::assume(pos_mul > 0);

    // Use a different k_epoch_start so settlement PnL is nonzero
    let k_start_val: i8 = kani::any();
    kani::assume(k_start_val != k_snap_val);
    let k_epoch_start = k_start_val as i128;

    engine.accounts[idx as usize].position_basis_q = I128::new((POS_SCALE * (pos_mul as u128)) as i128);
    engine.accounts[idx as usize].adl_a_basis = U128::new(ADL_ONE);
    engine.accounts[idx as usize].adl_k_snap = I128::new(k_snap);
    engine.accounts[idx as usize].adl_epoch_snap = 0;
    engine.stored_pos_count_long = 1;

    // Set adl_coeff_long to k_epoch_start so begin_full_drain_reset captures it
    engine.adl_coeff_long = I128::new(k_epoch_start);

    engine.oi_eff_long_q = U128::ZERO;
    engine.begin_full_drain_reset(Side::Long);

    assert!(engine.side_mode_long == SideMode::ResetPending);
    assert!(engine.adl_epoch_long == 1);
    assert!(engine.stale_account_count_long == 1);
    assert!(engine.adl_epoch_start_k_long == I128::new(k_epoch_start));

    let pnl_before = engine.accounts[idx as usize].pnl;

    let result = engine.settle_side_effects_with_h_lock(idx as usize, 0);
    assert!(result.is_ok());

    assert!(engine.accounts[idx as usize].position_basis_q.get() == 0);
    assert!(engine.stale_account_count_long == 0);

    // PnL assertion: settlement must credit the correct amount
    let abs_basis = (POS_SCALE * (pos_mul as u128)) as u128;
    let den = ADL_ONE * POS_SCALE;
    let expected_pnl_delta = wide_signed_mul_div_floor_from_k_pair(abs_basis, k_snap, k_epoch_start, den);
    assert!(engine.accounts[idx as usize].pnl == pnl_before + expected_pnl_delta,
        "full drain reset PnL must match wide_signed_mul_div_floor_from_k_pair");

    assert!(engine.stored_pos_count_long == 0);
    let finalize = engine.finalize_side_reset(Side::Long);
    assert!(finalize.is_ok());
    assert!(engine.side_mode_long == SideMode::Normal);
}

// ############################################################################
// SPEC §12 PROPERTY #43: K-pair chronology correctness
// ############################################################################

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_property_43_k_pair_chronology_correctness() {
    // Same-epoch settlement must pass (k_side, k_snap) in chronological order
    // such that a known loss is not settled as a gain.
    // We set up: k_snap < k_side (K has increased), and a long position.
    // For a long, k_side > k_snap means positive PnL (price went up).
    // If arguments were swapped, PnL would flip sign.
    let mut engine = RiskEngine::new(zero_fee_params());
    let idx = engine.add_user(0).unwrap();
    engine.deposit(idx, 1_000_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    // Set up a long position with k_snap = 100
    let pos = 10 * POS_SCALE as i128;
    engine.accounts[idx as usize].position_basis_q = I128::new(pos);
    engine.accounts[idx as usize].adl_a_basis = U128::new(ADL_ONE);
    engine.accounts[idx as usize].adl_k_snap = I128::new(100);
    engine.accounts[idx as usize].adl_epoch_snap = 0;
    engine.stored_pos_count_long = 1;
    engine.oi_eff_long_q = U128::new(10 * POS_SCALE);
    engine.oi_eff_short_q = U128::new(10 * POS_SCALE);

    // Set k_side > k_snap (price moved up => long should profit)
    engine.adl_coeff_long = I128::new(500);

    let pnl_before = engine.accounts[idx as usize].pnl;

    // settle_side_effects uses the real engine ordering
    let result = engine.settle_side_effects_with_h_lock(idx as usize, 0);
    assert!(result.is_ok());

    let pnl_after = engine.accounts[idx as usize].pnl;
    let pnl_delta = pnl_after - pnl_before;

    // With k_snap=100, k_side=500, the correct chronological call is
    // wide_signed_mul_div_floor_from_k_pair(abs_basis, k_then=100, k_now=500, den)
    // = floor(abs_basis * (500 - 100) / den) per spec §4.8
    let abs_basis = pos as u128;
    let den = ADL_ONE * POS_SCALE;
    let expected = wide_signed_mul_div_floor_from_k_pair(abs_basis, 100i128, 500i128, den);
    assert!(pnl_delta == expected,
        "settle PnL must match chronological k_pair computation");

    // The WRONG order would give the negation:
    let wrong = wide_signed_mul_div_floor_from_k_pair(abs_basis, 500i128, 100i128, den);
    // If expected != 0, wrong must have opposite sign
    if expected != 0 {
        assert!(wrong == -expected || (expected > 0 && wrong < 0) || (expected < 0 && wrong > 0),
            "swapped arguments must produce opposite-sign PnL");
    }
}
