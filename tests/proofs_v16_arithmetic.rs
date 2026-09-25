#![cfg(kani)]

use percolator::v16::{
    kani_adjust_u128, kani_adl_effective_quantity_ceil, kani_checked_fee_bps,
    kani_raw_basis_for_adl_effective_quantity, kani_risk_notional_ceil, kani_scaled_adl_delta_fast,
};
use percolator::wide_math::{
    ceil_div_positive_checked, div_rem_u256, floor_div_signed_conservative_i128, mul_div_ceil_u256,
    mul_div_floor_u256, mul_div_floor_u256_with_rem, wide_signed_mul_div_floor,
    wide_signed_mul_div_floor_from_k_pair, I256, U256,
};
use percolator::{ADL_ONE, MIN_A_SIDE, POS_SCALE};

#[kani::proof]
#[kani::unwind(20)]
#[kani::solver(cadical)]
fn proof_v16_adl_effective_quantity_inverse_preserves_reachable_target() {
    let raw_abs_q = u128::from(kani::any::<u8>() % 41);
    let a_basis_units = kani::any::<u8>();
    let current_a_units = kani::any::<u8>();
    let sub_min_adl = kani::any::<bool>();
    kani::assume((1..=10).contains(&a_basis_units));
    let a_basis = u128::from(a_basis_units) * MIN_A_SIDE;
    let current_a = if sub_min_adl {
        match current_a_units % 3 {
            0 => 1,
            1 => MIN_A_SIDE / 2,
            _ => MIN_A_SIDE - 1,
        }
    } else {
        kani::assume((1..=a_basis_units).contains(&current_a_units));
        u128::from(current_a_units) * MIN_A_SIDE
    };
    let current_effective =
        kani_adl_effective_quantity_ceil(raw_abs_q, a_basis, current_a).unwrap();
    let target_effective = u128::from(kani::any::<u8>() % 41);
    kani::assume(
        target_effective < current_effective || (target_effective == 0 && current_effective == 0),
    );

    let target_raw =
        kani_raw_basis_for_adl_effective_quantity(target_effective, a_basis, current_a).unwrap();
    let round_trip = kani_adl_effective_quantity_ceil(target_raw, a_basis, current_a).unwrap();

    kani::cover!(
        current_a < a_basis && target_effective > 0,
        "non-unit ADL partial reduction"
    );
    kani::cover!(
        current_a < MIN_A_SIDE && target_effective > 0,
        "drain-only sub-minimum A reduction"
    );
    kani::cover!(
        target_effective == 0 && current_effective > 0,
        "full effective close"
    );
    assert!(target_raw <= raw_abs_q);
    assert_eq!(round_trip, target_effective);
}

fn small_signed_floor_reference(n: i128, d: u128) -> i128 {
    if n >= 0 {
        (n as u128 / d) as i128
    } else {
        let abs = n.unsigned_abs();
        let q = abs / d;
        let r = abs % d;
        -((q + u128::from(r != 0)) as i128)
    }
}

#[kani::proof]
#[kani::unwind(20)]
#[kani::solver(cadical)]
fn proof_v16_floor_div_signed_conservative_matches_small_reference() {
    let n_raw: i16 = kani::any();
    let d_raw: u8 = kani::any();
    kani::assume((-500..=500).contains(&n_raw));
    kani::assume((1..=50).contains(&d_raw));

    let n = n_raw as i128;
    let d = d_raw as u128;
    let got = floor_div_signed_conservative_i128(n, d);
    let expected = small_signed_floor_reference(n, d);

    kani::cover!(
        n < 0 && n.unsigned_abs() % d != 0,
        "negative rounded-down branch"
    );
    kani::cover!(n >= 0, "nonnegative floor branch");
    assert_eq!(got, expected);
}

#[kani::proof]
#[kani::unwind(20)]
#[kani::solver(cadical)]
fn proof_v16_mul_div_floor_u256_matches_small_reference() {
    let a_raw: u8 = kani::any();
    let b_raw: u8 = kani::any();
    let d_raw: u8 = kani::any();
    kani::assume(a_raw <= 40);
    kani::assume(b_raw <= 40);
    kani::assume((1..=40).contains(&d_raw));

    let a = a_raw as u128;
    let b = b_raw as u128;
    let d = d_raw as u128;
    let got = mul_div_floor_u256(U256::from_u128(a), U256::from_u128(b), U256::from_u128(d));

    kani::cover!(a != 0 && b != 0 && d > 1, "nontrivial mul-div floor branch");
    assert_eq!(got.try_into_u128(), Some((a * b) / d));
}

#[kani::proof]
#[kani::unwind(20)]
#[kani::solver(cadical)]
fn proof_v16_mul_div_ceil_u256_is_floor_plus_remainder_indicator() {
    let a_raw: u8 = kani::any();
    let b_raw: u8 = kani::any();
    let d_raw: u8 = kani::any();
    kani::assume(a_raw <= 40);
    kani::assume(b_raw <= 40);
    kani::assume((1..=40).contains(&d_raw));

    let a = U256::from_u128(a_raw as u128);
    let b = U256::from_u128(b_raw as u128);
    let d = U256::from_u128(d_raw as u128);
    let (floor, rem) = mul_div_floor_u256_with_rem(a, b, d);
    let ceil = mul_div_ceil_u256(a, b, d);
    let floor_u128 = floor.try_into_u128().unwrap();
    let rem_u128 = rem.try_into_u128().unwrap();
    let expected = if rem_u128 == 0 {
        floor_u128
    } else {
        floor_u128 + 1
    };

    kani::cover!(rem_u128 == 0, "exact mul-div branch");
    kani::cover!(rem_u128 != 0, "ceil adds one branch");
    assert_eq!(ceil.try_into_u128(), Some(expected));
}

#[kani::proof]
#[kani::unwind(20)]
#[kani::solver(cadical)]
fn proof_v16_ceil_div_positive_checked_matches_small_reference() {
    let n_raw: u8 = kani::any();
    let d_raw: u8 = kani::any();
    kani::assume(d_raw > 0);

    let n = n_raw as u128;
    let d = d_raw as u128;
    let got = ceil_div_positive_checked(U256::from_u128(n), U256::from_u128(d));
    let expected = n / d + u128::from(n % d != 0);

    kani::cover!(n % d == 0, "ceil-div positive exact branch");
    kani::cover!(n % d != 0, "ceil-div positive remainder branch");
    assert_eq!(got.try_into_u128(), Some(expected));
}

#[kani::proof]
#[kani::unwind(20)]
#[kani::solver(cadical)]
fn proof_v16_wide_signed_mul_div_floor_matches_small_reference() {
    let abs_basis_raw: u8 = kani::any();
    let k_diff_raw: i8 = kani::any();
    let den_raw: u8 = kani::any();
    kani::assume(abs_basis_raw <= 16);
    kani::assume((-16..=16).contains(&k_diff_raw));
    kani::assume((1..=16).contains(&den_raw));

    let abs_basis = abs_basis_raw as u128;
    let k_diff = k_diff_raw as i128;
    let den = den_raw as u128;
    let got = wide_signed_mul_div_floor(
        U256::from_u128(abs_basis),
        I256::from_i128(k_diff),
        U256::from_u128(den),
    );
    let expected = small_signed_floor_reference(abs_basis as i128 * k_diff, den);

    kani::cover!(k_diff < 0, "negative wide signed branch");
    kani::cover!(k_diff > 0, "positive wide signed branch");
    assert_eq!(got.try_into_i128(), Some(expected));
}

#[kani::proof]
#[kani::unwind(80)]
#[kani::solver(cadical)]
fn proof_v16_k_pair_mul_div_floor_matches_small_reference() {
    let abs_basis_raw: u8 = kani::any();
    let k_then_raw: i8 = kani::any();
    let k_now_raw: i8 = kani::any();
    let den_raw: u8 = kani::any();
    kani::assume(abs_basis_raw <= 16);
    kani::assume((-16..=16).contains(&k_then_raw));
    kani::assume((-16..=16).contains(&k_now_raw));
    kani::assume((1..=16).contains(&den_raw));

    let abs_basis = abs_basis_raw as u128;
    let k_then = k_then_raw as i128;
    let k_now = k_now_raw as i128;
    let den = den_raw as u128;
    let got = wide_signed_mul_div_floor_from_k_pair(abs_basis, k_then, k_now, den);
    let expected = small_signed_floor_reference(abs_basis as i128 * (k_now - k_then), den);

    kani::cover!(k_now < k_then, "negative K-diff pair branch");
    kani::cover!(k_now > k_then, "positive K-diff pair branch");
    assert_eq!(got, expected);
}

#[kani::proof]
#[kani::unwind(20)]
#[kani::solver(cadical)]
fn proof_v16_k_pair_zero_cases_return_zero() {
    let den_raw: u8 = kani::any();
    kani::assume(den_raw > 0);
    let den = den_raw as u128;

    kani::cover!(den > 1, "K-pair zero-delta and zero-basis branches");
    assert_eq!(wide_signed_mul_div_floor_from_k_pair(0, -7, 11, den), 0);
    assert_eq!(wide_signed_mul_div_floor_from_k_pair(9, 3, 3, den), 0);
}

#[kani::proof]
#[kani::unwind(20)]
#[kani::solver(cadical)]
fn proof_v16_checked_trade_fee_is_ceil_bps_and_never_exceeds_notional() {
    let notional_raw: u8 = kani::any();
    let fee_bps_raw: u8 = kani::any();
    let max_fee_case: bool = kani::any();
    kani::assume(notional_raw <= 200);
    kani::assume(fee_bps_raw <= 250);

    let notional = notional_raw as u128;
    let fee_bps = if max_fee_case {
        10_000
    } else {
        fee_bps_raw as u64
    };
    let fee = kani_checked_fee_bps(notional, fee_bps).unwrap();
    let product = notional * fee_bps as u128;
    let expected = product / 10_000 + u128::from(product % 10_000 != 0);

    kani::cover!(
        product != 0 && product % 10_000 != 0,
        "checked fee proof covers rounded-up fee branch"
    );
    kani::cover!(
        fee_bps == 10_000 && notional > 0,
        "checked fee proof covers max-fee equality branch"
    );
    assert_eq!(fee, expected);
    assert!(fee <= notional);
    assert_eq!(fee == 0, notional == 0 || fee_bps == 0);
}

#[kani::proof]
#[kani::unwind(20)]
#[kani::solver(cadical)]
fn proof_v16_scaled_adl_delta_fast_matches_aligned_reference_and_fails_closed() {
    let abs_units_raw: u8 = kani::any();
    let delta_units_raw: i8 = kani::any();
    let a_basis_is_adl_one: bool = kani::any();
    let unaligned_extra_raw: u8 = kani::any();
    kani::assume(abs_units_raw <= 32);
    kani::assume((-32..=32).contains(&delta_units_raw));
    kani::assume(unaligned_extra_raw <= 1);

    let abs_basis_q = abs_units_raw as u128 * POS_SCALE;
    let a_basis = if a_basis_is_adl_one {
        ADL_ONE
    } else {
        ADL_ONE - 1
    };
    let then = 0i128;
    let now = delta_units_raw as i128 * ADL_ONE as i128 + unaligned_extra_raw as i128;
    let got = kani_scaled_adl_delta_fast(abs_basis_q, a_basis, then, now);

    let expected = if abs_units_raw == 0 {
        Some(0)
    } else if !a_basis_is_adl_one || unaligned_extra_raw != 0 {
        None
    } else {
        Some(delta_units_raw as i128 * abs_units_raw as i128)
    };

    kani::cover!(
        abs_units_raw > 0 && a_basis_is_adl_one && unaligned_extra_raw == 0 && delta_units_raw < 0,
        "scaled ADL fast path covers aligned negative settlement"
    );
    kani::cover!(
        abs_units_raw > 0 && (!a_basis_is_adl_one || unaligned_extra_raw != 0),
        "scaled ADL fast path covers fail-closed non-fast-path input"
    );
    assert_eq!(got, expected);
}

#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn proof_v16_adjust_u128_applies_exact_delta_or_fails_closed() {
    let current_raw: u8 = kani::any();
    let old_raw: u8 = kani::any();
    let new_raw: u8 = kani::any();
    let current = current_raw as u128;
    let old = old_raw as u128;
    let new = new_raw as u128;
    let result = kani_adjust_u128(current, old, new);

    kani::cover!(new > old, "adjust_u128 proof covers positive delta");
    kani::cover!(
        new < old && old - new > current,
        "adjust_u128 proof covers fail-closed underflow"
    );
    if new >= old {
        assert_eq!(result, Ok(current + (new - old)));
    } else if old - new <= current {
        assert_eq!(result, Ok(current - (old - new)));
    } else {
        assert!(result.is_err());
    }
}

// Clean-room unaligned ceil-correctness for risk_notional_ceil (independent of PR #72).
//
// risk_notional_ceil's fast path computes `product/POS_SCALE + (product % POS_SCALE != 0)`.
// The existing equivalence proofs pin inputs to POS_SCALE multiples (e.g.
// proof_v16_scaled_adl_delta_fast_matches_aligned_reference uses abs_units * POS_SCALE),
// so product % POS_SCALE == 0 and the +(rem != 0) correction NEVER fires -- ceil
// correctness was only checked where rounding is a no-op. This drives UNALIGNED inputs
// and checks the result against an INDEPENDENT ceil formula (round-up-by-add), with a
// cover ensuring the rounding-correction branch actually executes (non-vacuous).
// Inputs kept u16 (low symbolic width) so the proof is solver-tractable; product spans
// both sides of POS_SCALE so the >1-unit ceil regime is exercised.
#[kani::proof]
#[kani::unwind(20)]
#[kani::solver(cadical)]
fn proof_v16_risk_notional_ceil_unaligned_ceil_is_correct() {
    let q_raw: u16 = kani::any();
    let price_raw: u16 = kani::any();
    kani::assume((1..=4000).contains(&q_raw));
    kani::assume((1..=4000).contains(&price_raw));
    let abs_pos_q = q_raw as u128;
    let price = price_raw as u64;

    let got = kani_risk_notional_ceil(abs_pos_q, price);

    // Independent ceil via round-up-by-add (distinct from the fast path's
    // divide-then-correct), computed in u128 (product <= 4000*4000 < u128).
    let product = abs_pos_q * price as u128;
    let want = Ok((product + POS_SCALE - 1) / POS_SCALE);

    kani::cover!(
        product % POS_SCALE != 0,
        "unaligned: ceil rounding-correction branch fires"
    );
    kani::cover!(
        product > POS_SCALE,
        "product exceeds one full unit (q >= 1 regime)"
    );

    assert_eq!(got, want);
}

// ============================================================================
// div_rem_u256: the binary long-division path
//
// div_rem_u256 is the arithmetic core the rest of this file sits on --
// mul_div_floor_u256, mul_div_ceil_u256, mul_div_floor_u256_with_rem and
// wide_mul_div_floor_u128 all bottom out in it. It has two regimes:
//
//   * num.hi() == 0 && den.hi() == 0  -> native u128 `/` and `%`
//   * otherwise                       -> a shift/subtract long-division loop
//
// Every arithmetic harness above bounds its symbolic inputs to <= 500, so both
// operands always have hi() == 0 and ONLY the native path is ever proven. The
// loop -- the part with no rustc backing underneath it -- is never entered, so
// mutating it leaves all of these proofs green.
//
// The two harnesses below drive that loop. Both operands are given a small
// nonzero hi(), which skips the fast path; concretely that holds
// shift = leading_zeros(den) - leading_zeros(num) to at most 3, so the loop
// executes at most 4 times.
//
// The unwind bound is nevertheless 260, not 4. leading_zeros() lowers to a ctlz
// intrinsic that CBMC does not relate back to the assumed input range, so it
// cannot bound `shift` symbolically and must unroll the loop to its full 256-bit
// worst case; at a smaller bound the harness fails on
// `div_rem_u256.unwind.0` rather than on any property. Results are compared
// limb-wise rather than with assert_eq! on whole U256 values, because struct
// equality lowers to a 32-byte memcmp whose own loop would then need unwinding
// too.
// ============================================================================

// Exact small-reference check. Both operands are exact multiples of 2^128
// (lo == 0), so the quotient reduces to a native u128 division of the hi limbs:
// (hi_n * 2^128) / (hi_d * 2^128) == hi_n / hi_d, remainder (hi_n % hi_d) * 2^128.
// That gives a reference value the loop must reproduce bit-for-bit.
#[kani::proof]
#[kani::unwind(260)]
#[kani::solver(cadical)]
fn proof_v16_div_rem_u256_long_division_matches_small_reference() {
    let hi_n_raw: u8 = kani::any();
    let hi_d_raw: u8 = kani::any();
    kani::assume((1..=15).contains(&hi_n_raw));
    kani::assume((1..=15).contains(&hi_d_raw));
    // den > num would take the early return instead of the loop.
    kani::assume(hi_n_raw >= hi_d_raw);

    let hi_n = hi_n_raw as u128;
    let hi_d = hi_d_raw as u128;
    let num = U256::new(0, hi_n);
    let den = U256::new(0, hi_d);

    let (q, r) = div_rem_u256(num, den);

    kani::cover!(hi_n % hi_d != 0, "non-exact division through the loop");
    kani::cover!(hi_n / hi_d > 1, "multi-bit quotient through the loop");

    assert_eq!(q.lo(), hi_n / hi_d);
    assert_eq!(q.hi(), 0);
    assert_eq!(r.lo(), 0);
    assert_eq!(r.hi(), hi_n % hi_d);
}

// Full division identity with a symbolic low limb. (q, r) with r < den and
// q * den + r == num is unique, so pinning the identity pins the result. This
// covers dividends the reference harness above cannot express, including the
// borrow-across-limb subtractions the loop performs when lo != 0.
#[kani::proof]
#[kani::unwind(260)]
#[kani::solver(cadical)]
fn proof_v16_div_rem_u256_long_division_satisfies_division_identity() {
    let lo_n_raw: u8 = kani::any();
    let lo_d_raw: u8 = kani::any();
    let hi_n_raw: u8 = kani::any();
    let hi_d_raw: u8 = kani::any();
    kani::assume((1..=7).contains(&hi_n_raw));
    kani::assume((1..=7).contains(&hi_d_raw));
    kani::assume(hi_n_raw >= hi_d_raw);

    let num = U256::new(lo_n_raw as u128, hi_n_raw as u128);
    let den = U256::new(lo_d_raw as u128, hi_d_raw as u128);

    let (q, r) = div_rem_u256(num, den);

    kani::cover!(
        hi_n_raw > hi_d_raw && lo_n_raw < lo_d_raw,
        "loop entered with a borrow out of the low limb"
    );
    kani::cover!(q != U256::ZERO, "nonzero quotient through the loop");

    assert!(r < den, "remainder must be below the divisor");
    let back = q
        .checked_mul(den)
        .and_then(|p| p.checked_add(r))
        .expect("q * den + r must not overflow, since it equals num");
    assert_eq!(back.lo(), num.lo(), "q * den + r must reconstruct num");
    assert_eq!(back.hi(), num.hi(), "q * den + r must reconstruct num");
}
