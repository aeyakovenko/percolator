//! Section 3 — Pure math helper correctness proofs
//!
//! Arithmetic helper proofs: pure, loop-free, fast.

#![cfg(kani)]

mod common;
use common::*;

// ============================================================================
// T0.1: floor_div_signed_conservative_is_floor
// ============================================================================

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn t0_1_floor_div_signed_conservative_is_floor() {
    let n_raw: i8 = kani::any();
    let d_raw: u8 = kani::any();
    kani::assume(d_raw > 0);

    let n = I256::from_i128(n_raw as i128);
    let d = U256::from_u128(d_raw as u128);

    let result = floor_div_signed_conservative(n, d);

    let n_i32 = n_raw as i32;
    let d_i32 = d_raw as i32;
    let expected = if n_i32 >= 0 {
        n_i32 / d_i32
    } else {
        let abs_n = -n_i32;
        -((abs_n + d_i32 - 1) / d_i32)
    };

    let result_i128 = result.try_into_i128().unwrap();
    assert!(result_i128 == expected as i128, "floor_div mismatch");
}

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn t0_1_sat_negative_with_remainder() {
    let n_raw: i8 = kani::any();
    let d_raw: u8 = kani::any();
    kani::assume(d_raw > 1);
    kani::assume(n_raw < 0);
    let abs_n = -(n_raw as i32);
    kani::assume((abs_n as u32) % (d_raw as u32) != 0);

    let n = I256::from_i128(n_raw as i128);
    let d = U256::from_u128(d_raw as u128);
    let result = floor_div_signed_conservative(n, d);

    let trunc = (n_raw as i32) / (d_raw as i32);
    let result_i128 = result.try_into_i128().unwrap();
    assert!(result_i128 < trunc as i128);
}

// ============================================================================
// T0.2: mul_div_floor/ceil algebraic properties
// ============================================================================

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn t0_2_mul_div_floor_algebraic_identity() {
    let a: u8 = kani::any();
    let b: u8 = kani::any();
    let c: u8 = kani::any();
    // Constrain to 4-bit range to keep U256/U512 division tractable for SAT solver
    kani::assume(a <= 15 && b <= 15 && c > 0 && c <= 15);

    let a256 = U256::from_u128(a as u128);
    let b256 = U256::from_u128(b as u128);
    let c256 = U256::from_u128(c as u128);

    let (q, r) = mul_div_floor_u256_with_rem(a256, b256, c256);

    // Algebraic identity: q * c + r == a * b
    let lhs = q * c256 + r;
    let rhs = a256 * b256;
    assert!(lhs == rhs, "q * c + r must equal a * b");

    // Remainder must be strictly less than divisor
    assert!(r < c256, "remainder must be less than divisor");
}

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn t0_2_mul_div_ceil_algebraic_identity() {
    let a: u8 = kani::any();
    let b: u8 = kani::any();
    let c: u8 = kani::any();
    // Constrain to 4-bit range to keep U256/U512 division tractable for SAT solver
    kani::assume(a <= 15 && b <= 15 && c > 0 && c <= 15);

    let a256 = U256::from_u128(a as u128);
    let b256 = U256::from_u128(b as u128);
    let c256 = U256::from_u128(c as u128);

    let (floor, r) = mul_div_floor_u256_with_rem(a256, b256, c256);
    let ceil = mul_div_ceil_u256(a256, b256, c256);

    let expected_ceil = if r != U256::ZERO {
        floor + U256::from_u128(1)
    } else {
        floor
    };
    assert!(ceil == expected_ceil, "ceil must equal floor + (r != 0 ? 1 : 0)");
}

#[kani::proof]
#[kani::unwind(18)]
#[kani::solver(cadical)]
fn t0_2c_mul_div_floor_matches_reference() {
    let a: u8 = kani::any();
    let b: u8 = kani::any();
    let c: u8 = kani::any();
    kani::assume(c > 0);
    // Constrain to 4-bit range to keep U256 solver tractable
    kani::assume(a <= 15 && b <= 15 && c <= 15);

    let result = mul_div_floor_u256(
        U256::from_u128(a as u128),
        U256::from_u128(b as u128),
        U256::from_u128(c as u128),
    );

    let expected = ((a as u32) * (b as u32)) / (c as u32);
    let result_u128 = result.try_into_u128().unwrap();
    assert!(result_u128 == expected as u128, "mul_div_floor mismatch");
}

#[kani::proof]
#[kani::unwind(18)]
#[kani::solver(cadical)]
fn t0_2d_mul_div_ceil_matches_reference() {
    let a: u8 = kani::any();
    let b: u8 = kani::any();
    let c: u8 = kani::any();
    kani::assume(c > 0);
    kani::assume(a <= 15 && b <= 15 && c <= 15);

    let result = mul_div_ceil_u256(
        U256::from_u128(a as u128),
        U256::from_u128(b as u128),
        U256::from_u128(c as u128),
    );

    let product = (a as u32) * (b as u32);
    let expected = (product + (c as u32) - 1) / (c as u32);
    let result_u128 = result.try_into_u128().unwrap();
    assert!(result_u128 == expected as u128, "mul_div_ceil mismatch");
}

// ============================================================================
// T0.4: safe_fee_debt_and_cap_math
// ============================================================================

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn t0_4_fee_debt_no_overflow() {
    let fc: i128 = kani::any();
    let debt = fee_debt_u128_checked(fc);
    if fc < 0 {
        assert!(debt > 0);
        assert!(debt == fc.unsigned_abs());
    } else {
        assert!(debt == 0);
    }
}

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn t0_4_saturating_mul_no_panic() {
    let a: u8 = kani::any();
    let b: u8 = kani::any();

    let a256 = U256::from_u128(a as u128);
    let result = saturating_mul_u256_u64(a256, b as u64);
    let expected = (a as u128) * (b as u128);
    assert!(result == U256::from_u128(expected));

    kani::assume(b > 1);
    let result_max = saturating_mul_u256_u64(U256::MAX, b as u64);
    assert!(result_max == U256::MAX, "must saturate at U256::MAX");
}

#[kani::proof]
#[kani::unwind(1)]
#[kani::solver(cadical)]
fn t0_4_fee_debt_i128_min() {
    // Per spec §2.1: "fee_credits == i128::MIN is forbidden".
    // The engine must never allow fee_credits to reach i128::MIN.
    // Verify fee_debt_u128_checked handles all valid inputs correctly:
    // for any valid fee_credits (not i128::MIN), negative credits produce
    // the correct unsigned debt, and non-negative credits produce 0.
    let fc: i8 = kani::any();
    kani::assume(fc != i8::MIN); // mirrors the i128::MIN prohibition at small scale
    let debt = fee_debt_u128_checked(fc as i128);
    if fc >= 0 {
        assert!(debt == 0, "non-negative fee_credits must have zero debt");
    } else {
        assert!(debt == (-(fc as i128)) as u128,
            "negative fee_credits debt must equal abs(fee_credits)");
    }
}

// ============================================================================
// From kani.rs: notional proofs
// ============================================================================

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_notional_flat_is_zero() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let idx = engine.add_user(0).unwrap();

    let oracle: u16 = kani::any();
    kani::assume(oracle > 0 && oracle <= 1000);

    let notional = engine.notional(idx as usize, oracle as u64);
    assert!(notional == 0);
}

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_notional_scales_with_price() {
    // Use the engine's actual notional() function to verify monotonicity
    // through the floor(abs(eff_pos_q) * price / POS_SCALE) formula.
    let mut engine = RiskEngine::new(zero_fee_params());
    let idx = engine.add_user(0).unwrap();
    engine.deposit(idx, 10_000_000, 100, 0).unwrap();

    // Give the account a non-zero position
    let q_mul: u8 = kani::any();
    kani::assume(q_mul > 0 && q_mul <= 10);
    engine.accounts[idx as usize].position_basis_q = I128::new((POS_SCALE * (q_mul as u128)) as i128);
    engine.accounts[idx as usize].adl_a_basis = U128::new(ADL_ONE);
    engine.accounts[idx as usize].adl_k_snap = I128::ZERO;
    engine.accounts[idx as usize].adl_epoch_snap = 0;
    engine.adl_epoch_long = 0;
    engine.adl_mult_long = U128::new(ADL_ONE);
    engine.stored_pos_count_long = 1;
    engine.oi_eff_long_q = U128::new(POS_SCALE * (q_mul as u128));

    let p1: u8 = kani::any();
    let p2: u8 = kani::any();
    kani::assume(p1 > 0);
    kani::assume(p2 >= p1);

    let n1 = engine.notional(idx as usize, p1 as u64);
    let n2 = engine.notional(idx as usize, p2 as u64);
    assert!(n2 >= n1, "notional must be monotone in price");
}

/// advance_profit_warmup releases at most reserved_pnl (§4.9)
#[kani::proof]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn proof_warmup_release_bounded_by_reserved() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let idx = engine.add_user(0).unwrap();
    engine.deposit(idx, 100_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    let pnl_val: u16 = kani::any();
    kani::assume(pnl_val > 0 && pnl_val <= 10_000);
    engine.set_pnl(idx as usize, pnl_val as i128);
    // After set_pnl, reserved_pnl tracks the positive PnL increase
    let r_before = engine.accounts[idx as usize].reserved_pnl;

    engine.advance_profit_warmup(idx as usize);
    let r_after = engine.accounts[idx as usize].reserved_pnl;

    // reserved can only decrease or stay the same
    assert!(r_after <= r_before, "advance_profit_warmup must not increase reserve");
}

// ============================================================================
// T13.59: fused_delta_k_no_double_rounding
// ============================================================================

#[kani::proof]
#[kani::unwind(1)]
#[kani::solver(cadical)]
fn t13_59_fused_delta_k_no_double_rounding() {
    let d: u8 = kani::any();
    kani::assume(d > 0 && d <= 15);
    let oi: u8 = kani::any();
    kani::assume(oi > 0 && oi <= 15);
    let a: u8 = kani::any();
    kani::assume(a > 0 && a <= 15);

    let beta_abs = ((d as u32) + (oi as u32) - 1) / (oi as u32);
    let old_delta_k = (a as u32) * beta_abs;

    let new_delta_k = ((d as u32) * (a as u32) + (oi as u32) - 1) / (oi as u32);

    assert!(new_delta_k <= old_delta_k,
        "fused formula must not exceed old two-step formula");

    let exact_times_oi = (d as u32) * (a as u32);
    assert!(new_delta_k * (oi as u32) >= exact_times_oi,
        "fused ceiling must be >= exact value");
}

// ============================================================================
// NEW: proof_ceil_div_positive_checked
// ============================================================================

/// ceil helper matches reference for u8
#[kani::proof]
#[kani::unwind(18)]
#[kani::solver(cadical)]
fn proof_ceil_div_positive_checked() {
    let n: u8 = kani::any();
    let d: u8 = kani::any();
    kani::assume(d > 0);

    let result = ceil_div_positive_checked(
        U256::from_u128(n as u128),
        U256::from_u128(d as u128),
    );

    let expected = ((n as u32) + (d as u32) - 1) / (d as u32);
    let result_u128 = result.try_into_u128().unwrap();
    assert!(result_u128 == expected as u128, "ceil_div_positive_checked mismatch");
}

// ============================================================================
// NEW: proof_haircut_mul_div_conservative
// ============================================================================

/// haircut uses floor, never overshoots
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_haircut_mul_div_conservative() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let idx = engine.add_user(0).unwrap();

    let pnl_val: u16 = kani::any();
    kani::assume(pnl_val > 0 && pnl_val <= 10_000);
    engine.set_pnl(idx as usize, pnl_val as i128);

    // Set vault > c_tot so residual is positive
    let cap: u16 = kani::any();
    kani::assume(cap >= 100 && cap <= 10_000);
    engine.set_capital(idx as usize, cap as u128);
    engine.vault = U128::new((cap as u128) + (pnl_val as u128));

    let (h_num, h_den) = engine.haircut_ratio();
    assert!(h_num <= h_den, "h_num must be <= h_den");
    assert!(h_den != 0, "h_den must not be zero");

    // effective_pnl = floor(pnl * h_num / h_den) <= pnl
    let effective = mul_div_floor_u128(pnl_val as u128, h_num, h_den);
    assert!(effective <= pnl_val as u128,
        "floor haircut must not overshoot pnl");
}

// ============================================================================
// wide_signed_mul_div_floor correctness (spec §1.5 item 11)
// ============================================================================
//
// This is the critical 512-bit intermediate path used for PnL delta
// computation. Verifies:
//   floor(abs_basis * k_diff / denom) with correct sign and rounding.

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_wide_signed_mul_div_floor_sign_and_rounding() {
    let basis: u8 = kani::any();
    let k_val: i8 = kani::any();
    let denom: u8 = kani::any();

    kani::assume(basis > 0 && basis <= 15);
    kani::assume(denom > 0 && denom <= 15);
    kani::assume(k_val != i8::MIN); // I256::MIN excluded by impl
    kani::assume(k_val >= -15 && k_val <= 15);

    let abs_basis = U256::from_u128(basis as u128);
    let k_diff = I256::from_i128(k_val as i128);
    let denominator = U256::from_u128(denom as u128);

    let result = wide_signed_mul_div_floor(abs_basis, k_diff, denominator);

    // Reference: compute in i32 to avoid overflow at u8 scale
    let numerator = (basis as i32) * (k_val as i32);
    // Floor division: toward negative infinity
    let expected = if numerator >= 0 {
        numerator / (denom as i32)
    } else {
        // floor for negative: -((-numerator + denom - 1) / denom)
        let abs_num = (-numerator) as u32;
        let d = denom as u32;
        -(((abs_num + d - 1) / d) as i32)
    };

    let result_i128 = if result.is_negative() {
        -(result.abs_u256().lo() as i128)
    } else {
        result.abs_u256().lo() as i128
    };

    assert!(result_i128 == expected as i128,
        "wide_signed_mul_div_floor must match reference floor division");
}

// ============================================================================
// wide_signed_mul_div_floor_from_k_pair correctness (spec §4.8)
// ============================================================================
//
// This is the spec-normative K-pair variant used in settle_side_effects (§5.3).
// It performs the K-difference in a wide intermediate, then multiplies and divides.
// Verifies that wide subtraction, sign handling, and floor rounding are correct
// even when k_now < k_then (negative K-difference).

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_k_pair_variant_sign_and_rounding() {
    let basis: u8 = kani::any();
    let k_now_val: i8 = kani::any();
    let k_then_val: i8 = kani::any();
    let denom: u8 = kani::any();

    kani::assume(basis > 0 && basis <= 15);
    kani::assume(denom > 0 && denom <= 15);
    kani::assume(k_now_val >= -15 && k_now_val <= 15);
    kani::assume(k_then_val >= -15 && k_then_val <= 15);

    let abs_basis = basis as u128;
    let k_now = k_now_val as i128;
    let k_then = k_then_val as i128;
    let den = denom as u128;

    let result = wide_signed_mul_div_floor_from_k_pair(abs_basis, k_then, k_now, den);

    // Reference: compute in i32 to avoid overflow at u8 scale
    let k_diff = (k_now_val as i32) - (k_then_val as i32);
    let numerator = (basis as i32) * k_diff;
    // Floor division: toward negative infinity
    let expected = if numerator >= 0 {
        numerator / (denom as i32)
    } else {
        let abs_num = (-numerator) as u32;
        let d = denom as u32;
        -(((abs_num + d - 1) / d) as i32)
    };

    assert!(result == expected as i128,
        "K-pair variant must match reference floor division");
}

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_k_pair_variant_zero_diff() {
    let basis: u8 = kani::any();
    let k_val: i8 = kani::any();
    let denom: u8 = kani::any();
    kani::assume(basis > 0);
    kani::assume(denom > 0);

    // k_now == k_then → result must be 0
    let result = wide_signed_mul_div_floor_from_k_pair(
        basis as u128, k_val as i128, k_val as i128, denom as u128,
    );
    assert!(result == 0, "K-pair with equal k_now and k_then must return 0");
}

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_wide_signed_mul_div_floor_zero_inputs() {
    // Zero basis → zero result
    let result = wide_signed_mul_div_floor(U256::ZERO, I256::from_i128(42), U256::from_u128(1));
    assert!(result == I256::ZERO);

    // Zero k_diff → zero result
    let result2 = wide_signed_mul_div_floor(U256::from_u128(42), I256::ZERO, U256::from_u128(1));
    assert!(result2 == I256::ZERO);
}
