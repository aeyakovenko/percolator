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
//! These harnesses REMOVE the `POS_SCALE` pin (basis/positions are fully
//! symbolic) and assert that the fast native path equals the wide U256 fallback
//! — the same equality the production callsites rely on
//! (`src/v16.rs:7136-7163`, `:9904`, `:10228`). This is the formal counterpart
//! to the `tests/fast_wide_equivalence.rs` proptest: where the proptest covers
//! the unaligned domain empirically, these harnesses attempt to discharge it.

use percolator::v16::{kani_checked_fee_bps, kani_scaled_adl_delta_fast};
use percolator::wide_math::{checked_mul_div_ceil_u256, wide_signed_mul_div_floor_from_k_pair, U256};
use percolator::{ADL_ONE, MAX_MARGIN_BPS, POS_SCALE};

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
    // Fully symbolic, small magnitudes for tractability — but crucially NOT
    // pinned to POS_SCALE multiples.
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
        assert_eq!(v, wide, "fast ADL delta != wide fallback on unaligned basis");
    }
}

/// `checked_fee_bps` fast (`q + (r != 0)`) vs wide (`checked_mul_div_ceil_u256`),
/// UNALIGNED.
///
/// `proofs_v16_arithmetic.rs:179` checks the fast path against a closed-form
/// `product / 10_000 + (product % 10_000 != 0)` reference but never against the
/// U256 wide branch, and its inputs keep the product small. Here `notional` and
/// `fee_bps` are symbolic with `notional * fee_bps` deliberately allowed to make
/// `product % MAX_MARGIN_BPS != 0` (the ceil correction live), and we assert the
/// fast result equals the independent U256 ceil — the wide branch.
#[kani::proof]
#[kani::unwind(20)]
#[kani::solver(cadical)]
fn proof_v16_checked_fee_bps_fast_eq_wide_unaligned() {
    let notional_raw: u32 = kani::any();
    let fee_bps_raw: u16 = kani::any();
    kani::assume(notional_raw <= 200_000);
    kani::assume((1..=10_000).contains(&fee_bps_raw));

    let notional = notional_raw as u128;
    let fee_bps = fee_bps_raw as u64;

    let fast = kani_checked_fee_bps(notional, fee_bps).unwrap();

    // Independent wide reference = the production wide branch (src/v16.rs:12247).
    let wide = checked_mul_div_ceil_u256(
        U256::from_u128(notional),
        U256::from_u128(fee_bps as u128),
        U256::from_u128(MAX_MARGIN_BPS as u128),
    )
    .and_then(|v| v.try_into_u128())
    .expect("wide ceil fits in u128 for harness bounds");

    let product = notional * fee_bps as u128;
    kani::cover!(
        product % MAX_MARGIN_BPS as u128 != 0,
        "checked_fee_bps fast/wide covers ceil-correction-live (r != 0) branch"
    );

    assert_eq!(fast, wide, "fast fee != wide U256 ceil");
}
