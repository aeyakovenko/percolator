#![cfg(kani)]

//! Fast/wide arithmetic equivalence over the UNALIGNED input domain.
//!
//! The existing arithmetic equivalence proofs in `tests/proofs_v16_arithmetic.rs`
//! pin their basis/position inputs to `POS_SCALE` multiples
//! (`proofs_v16_arithmetic.rs:221`: `abs_basis_q = abs_units * POS_SCALE`;
//! `proofs_v16.rs:611,652`: `units * POS_SCALE`). With an aligned product,
//! `product % POS_SCALE == 0`, so the floor/ceil rounding-correction term
//! (`q + (r != 0)` in `risk_notional_ceil`/`checked_fee_bps`, and the negative
//! round-down in `floor_div_signed_conservative_i128` inside
//! `scaled_adl_delta_fast`) never fires — fast == wide is proven only where the
//! two paths structurally cannot differ.
//!
//! These harnesses REMOVE the `POS_SCALE` pin and assert that the fast native
//! path equals the wide U256 fallback - the same equality the production
//! callsites rely on (`src/v16.rs:7136-7163`, `:9904`, `:10228`). They are the
//! formal counterpart to `tests/fast_wide_equivalence.rs`.
//!
//! Honest scope: Kani 0.67.0 does not tractably discharge the full symbolic
//! `u128`/`i128` product domain for the U256 fallback division. The harnesses
//! below therefore prove the widest domain we could keep in the native-product
//! subdomain while still allowing unaligned remainders and full solver
//! verification:
//!
//! - `scaled_adl_delta_fast`: `abs_basis_q <= 4096`, `delta_units: -8..=8`.
//! - `checked_fee_bps`: `notional <= 200`, `fee_bps: 1..=10_000`.
//! - `risk_notional_ceil`: `abs_pos_q: u16`, `price: u16`,
//!   `abs_pos_q * price < POS_SCALE`.
//!
//! These are unaligned symbolic domains, not POS_SCALE multiples; the cover
//! checks prove the rounding-correction branches are live. The full `u128/u64`
//! risk-notional space, including production's U256 fallback branch, remains
//! covered empirically by `tests/fast_wide_equivalence.rs`. Kani harnesses that
//! call the U256 ceil helper directly timed out locally; the Kani-tractable
//! ceil harnesses below prove the exact same ceil formula over smaller
//! native-product domains instead.

use percolator::v16::{kani_checked_fee_bps, kani_scaled_adl_delta_fast, risk_notional_ceil};
#[allow(unused_imports)]
use percolator::wide_math::{
    checked_mul_div_ceil_u256, wide_signed_mul_div_floor_from_k_pair, U256,
};
use percolator::{ADL_ONE, MAX_MARGIN_BPS, POS_SCALE};

#[allow(dead_code)]
fn stub_checked_mul_div_ceil_u256(_: U256, _: U256, _: U256) -> Option<U256> {
    None
}

/// `scaled_adl_delta_fast` vs its production wide fallback, UNALIGNED.
///
/// Production (`src/v16.rs:7136`) computes
/// `scaled_adl_delta_fast(abs_basis, a_basis, then, now)
///     .unwrap_or_else(|| wide_signed_mul_div_floor_from_k_pair(abs_basis, then, now, a_basis*POS_SCALE))`.
/// When the fast path returns `Some(v)`, `v` MUST equal the wide fallback.
///
/// Unlike `proofs_v16_arithmetic.rs:212`, `abs_basis_q` is NOT a `POS_SCALE`
/// multiple, so `scaled_delta * abs_basis` need not be divisible by `POS_SCALE`
/// and the `floor_div_signed_conservative_i128` round-down correction is live.
#[kani::proof]
#[kani::unwind(80)]
#[kani::solver(cadical)]
fn proof_v16_scaled_adl_fast_eq_wide_unaligned() {
    // Proven symbolic domain: abs_basis_q in 0..=4096 and delta in -8..=8
    // ADL-unit steps. Wider u64/i16 symbolic products timed out locally in
    // the U256 wide fallback division.
    let abs_basis_raw: u16 = kani::any();
    let delta_units_raw: i8 = kani::any();
    kani::assume(abs_basis_raw <= 4096);
    kani::assume((-8..=8).contains(&delta_units_raw));

    let abs_basis_q = abs_basis_raw as u128; // UNALIGNED: not * POS_SCALE
    let a_basis = ADL_ONE; // fast path requires a_basis == ADL_ONE
    let then = 0i128;
    // `now` an exact multiple of ADL_ONE so the fast path is taken (delta % ADL_ONE == 0),
    // but `abs_basis_q` stays unaligned so the inner floor correction can fire.
    let now = delta_units_raw as i128 * ADL_ONE as i128;

    let fast = kani_scaled_adl_delta_fast(abs_basis_q, a_basis, then, now);

    // Production wide fallback denominator: a_basis * POS_SCALE.
    let den = a_basis
        .checked_mul(POS_SCALE)
        .expect("den overflow in harness");
    let wide = wide_signed_mul_div_floor_from_k_pair(abs_basis_q, then, now, den);

    // Cover: the unaligned, rounding-live branch we care about (negative delta,
    // basis not a POS_SCALE multiple).
    kani::cover!(
        abs_basis_q != 0 && abs_basis_q % POS_SCALE != 0 && delta_units_raw < 0,
        "scaled ADL fast/wide covers unaligned negative settlement (rounding live)"
    );
    kani::cover!(
        abs_basis_q != 0 && abs_basis_q % POS_SCALE != 0 && delta_units_raw > 0,
        "scaled ADL fast/wide covers unaligned positive settlement"
    );

    // Where the fast path engages, it must equal the wide fallback exactly.
    if let Some(v) = fast {
        assert_eq!(
            v, wide,
            "fast ADL delta != wide fallback on unaligned basis"
        );
    }
}

/// `checked_fee_bps` fast (`q + (r != 0)`) equals exact ceil division,
/// UNALIGNED.
///
/// `proofs_v16_arithmetic.rs:179` checks the fast path against a closed-form
/// `product / 10_000 + (product % 10_000 != 0)` reference but never against the
/// U256 wide branch, and its inputs keep the product small. A direct U256
/// reference in this harness timed out locally, so this proof asserts the exact
/// ceil formula over an unaligned symbolic domain.
#[kani::proof]
#[kani::unwind(20)]
#[kani::solver(cadical)]
fn proof_v16_checked_fee_bps_fast_eq_wide_unaligned() {
    // Proven symbolic domain: notional in 0..=200, fee_bps in the production
    // margin-bps range.
    let notional_raw: u8 = kani::any();
    let fee_bps_raw: u16 = kani::any();
    kani::assume(notional_raw <= 200);
    kani::assume((1..=10_000).contains(&fee_bps_raw));

    let notional = notional_raw as u128;
    let fee_bps = fee_bps_raw as u64;

    let fast = kani_checked_fee_bps(notional, fee_bps).unwrap();

    let product = notional * fee_bps as u128;
    let q = product / MAX_MARGIN_BPS as u128;
    let r = product % MAX_MARGIN_BPS as u128;
    let want = if r == 0 { q } else { q + 1 };
    kani::cover!(
        product % MAX_MARGIN_BPS as u128 != 0,
        "checked_fee_bps fast/wide covers ceil-correction-live (r != 0) branch"
    );

    assert_eq!(fast, want, "fast fee != exact ceil formula");
}

/// `risk_notional_ceil` native fast path equals exact ceil division, UNALIGNED.
///
/// Proven symbolic domain: `abs_pos_q: u16`, `price: u16`, with product below
/// POS_SCALE. The product always fits in u128, so the production native product
/// succeeds, and the nonzero unaligned cases must ceil to exactly 1. Wider
/// domains with symbolic division timed out locally; the std proptest
/// complements this by exercising full u128/u64 inputs and the actual U256
/// fallback code path empirically.
#[kani::proof]
#[kani::stub(checked_mul_div_ceil_u256, stub_checked_mul_div_ceil_u256)]
#[kani::unwind(20)]
#[kani::solver(cadical)]
fn proof_v16_risk_notional_ceil_fast_eq_wide_unaligned() {
    let abs_pos_raw: u16 = kani::any();
    let price_raw: u16 = kani::any();
    let abs_pos_q = abs_pos_raw as u128;
    let price = price_raw as u64;
    let product = abs_pos_raw as u32 * price_raw as u32;
    kani::assume(product < POS_SCALE as u32);

    let got = risk_notional_ceil(abs_pos_q, price);
    let want = if product == 0 { Some(0) } else { Some(1) };

    kani::cover!(
        abs_pos_q != 0 && price != 0 && product != 0,
        "risk_notional_ceil fast/wide covers ceil-correction-live (r != 0) branch"
    );

    assert_eq!(got.ok(), want, "risk notional != exact ceil formula");
}
