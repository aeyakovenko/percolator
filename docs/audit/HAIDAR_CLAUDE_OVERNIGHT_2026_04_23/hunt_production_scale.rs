//! Production-scale hunts for math functions that Kani only proved on bounded domains.
//!
//! Per proof-strength-audit-results.md:
//!   - `kani_invert_nonzero_computes_correctly`: bounded to raw <= 8192
//!   - `kani_invert_monotonic`: bounded to raw1 <= 16384
//!   - `kani_scale_price_e6_valid_result`: bounded to ~1M
//!   - `kani_units_roundtrip`: bounded to units <= 16384
//!
//! Production SOL price is ~87_000_000 (87e6 raw, which is SOL/USD = $87).
//! Production unit_scale goes up to MAX_UNIT_SCALE = 1_000_000_000.
//!
//! We run 100k+ cases per property across the FULL u64 range to hunt for:
//!   - Wrong results at production-scale inputs
//!   - Unexpected None returns
//!   - Roundtrip breaks
//!   - Monotonicity violations in the unproven region
//!
//! Any failure here is a finding — either a bug or a Kani coverage gap worth reporting.

use percolator_prog::verify::{
    invert_price_e6, scale_price_e6, to_engine_price, INVERSION_CONSTANT,
};
use proptest::prelude::*;

// Production parameters (from mainnet-market.json)
const MAX_UNIT_SCALE: u32 = 1_000_000_000;
const PROD_SOL_PRICE_E6_LO: u64 = 10_000_000;      // $10
const PROD_SOL_PRICE_E6_HI: u64 = 1_000_000_000;   // $1000

// ============================================================================
// invert_price_e6 — hunts at production scale
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100_000))]

    /// Spec: if invert==1 and raw>0, result is floor(1e12 / raw) when <= u64::MAX, else None.
    #[test]
    fn prop_invert_matches_spec_full_u64(raw: u64) {
        let got = invert_price_e6(raw, 1);
        if raw == 0 {
            prop_assert_eq!(got, None, "raw=0 must return None");
        } else {
            let expected_u128 = INVERSION_CONSTANT / (raw as u128);
            if expected_u128 == 0 {
                prop_assert_eq!(got, None, "result underflow to 0 -> None, raw={}", raw);
            } else if expected_u128 > u64::MAX as u128 {
                prop_assert_eq!(got, None, "result > u64::MAX -> None, raw={}", raw);
            } else {
                prop_assert_eq!(got, Some(expected_u128 as u64),
                    "raw={} expected={} got={:?}", raw, expected_u128, got);
            }
        }
    }

    /// Roundtrip: invert(invert(x)) should equal x *when* both inversions succeed.
    /// Due to floor division, exact roundtrip only holds when x divides 1e12 exactly.
    /// We test the error bound.
    #[test]
    fn prop_invert_roundtrip_error_bounded(raw in 1u64..=1_000_000_000_000u64) {
        let first = invert_price_e6(raw, 1);
        prop_assume!(first.is_some());
        let f = first.unwrap();
        prop_assume!(f > 0);

        let back = invert_price_e6(f, 1);
        prop_assume!(back.is_some());
        let b = back.unwrap();

        // After two floor divisions, the result should be within 1 of raw for "normal" values.
        // We check: |raw - b| / raw should be bounded.
        // If f = 1e12/raw, b = 1e12/f = 1e12 / (1e12/raw) ~= raw for large raw.
        // For tiny raw (raw=1), f=1e12, b=1. The roundtrip fails by huge margin — expected.
        // We only care about production range.
        // Floor-div roundtrip error bound: |raw - b| <= raw/f + 1
        // where f = floor(1e12/raw). This is a mathematical property, not a vulnerability.
        // We assert this exact bound to catch any implementation drift.
        let diff = if raw > b { raw - b } else { b - raw };
        let max_err = (raw / f) + 1;
        prop_assert!(diff <= max_err,
            "roundtrip error raw={} f={} b={} diff={} max_err={}",
            raw, f, b, diff, max_err);
    }

    /// Monotonicity: raw1 > raw2 > 0 implies invert(raw1) <= invert(raw2)
    /// Kani only verified raw1 <= 16384. Test at production scale.
    #[test]
    fn prop_invert_monotonic_production(
        raw1 in 1u64..=u64::MAX,
        raw2 in 1u64..=u64::MAX,
    ) {
        prop_assume!(raw1 != raw2);
        let (small, large) = if raw1 < raw2 { (raw1, raw2) } else { (raw2, raw1) };

        let i_small = invert_price_e6(small, 1);
        let i_large = invert_price_e6(large, 1);

        match (i_small, i_large) {
            (Some(a), Some(b)) => {
                prop_assert!(a >= b,
                    "monotonic violation: small={} large={} inv_small={} inv_large={}",
                    small, large, a, b);
            }
            (Some(_), None) => {
                // small<large but large's inversion went to 0 or overflow.
                // Underflow (inverted == 0) only happens when raw > 1e12.
                // So this is consistent if large > 1e12 and small <= 1e12.
                prop_assert!(large as u128 > INVERSION_CONSTANT,
                    "large={} should be > 1e12 if its inversion is None while small is Some", large);
            }
            (None, Some(_)) => {
                // small's inversion is None but large's is Some — impossible under spec
                prop_assert!(false,
                    "small={} (inv=None) but large={} (inv=Some) — monotonicity broken", small, large);
            }
            (None, None) => {
                // both beyond 1e12 — consistent
            }
        }
    }

    /// Boundary probe: inputs near INVERSION_CONSTANT (1e12)
    /// Kani did not cover this boundary directly.
    #[test]
    fn prop_invert_boundary_1e12(offset in -100i64..=100i64) {
        let base = INVERSION_CONSTANT as u64; // 1e12
        let raw = if offset >= 0 {
            base.saturating_add(offset as u64)
        } else {
            base.saturating_sub((-offset) as u64)
        };
        prop_assume!(raw > 0);
        let got = invert_price_e6(raw, 1);
        let expected = INVERSION_CONSTANT / (raw as u128);
        if expected == 0 {
            prop_assert_eq!(got, None);
        } else if expected > u64::MAX as u128 {
            prop_assert_eq!(got, None);
        } else {
            prop_assert_eq!(got, Some(expected as u64));
        }
    }
}

// ============================================================================
// scale_price_e6 — hunts at production scale (up to MAX_UNIT_SCALE = 1e9)
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100_000))]

    /// Spec: scale=0 or 1 -> identity; scale>1 -> floor-div, None if result=0.
    #[test]
    fn prop_scale_price_matches_spec(price: u64, scale in 0u32..=MAX_UNIT_SCALE) {
        let got = scale_price_e6(price, scale);
        if scale <= 1 {
            prop_assert_eq!(got, Some(price));
        } else {
            let expected = price / (scale as u64);
            if expected == 0 {
                prop_assert_eq!(got, None, "price={} scale={} should be None", price, scale);
            } else {
                prop_assert_eq!(got, Some(expected),
                    "price={} scale={} expected={} got={:?}", price, scale, expected, got);
            }
        }
    }

    /// Scale > price and scale > 1 always returns None (price/scale == 0 in u64).
    #[test]
    fn prop_scale_larger_than_price_returns_none(
        price in 1u64..=1_000u64,
        scale in 2_000u32..=MAX_UNIT_SCALE,
    ) {
        let got = scale_price_e6(price, scale);
        prop_assert_eq!(got, None,
            "price={} scale={} > price, should be None, got={:?}", price, scale, got);
    }
}

// ============================================================================
// to_engine_price — composition
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100_000))]

    /// to_engine_price(raw, invert, scale) == scale(invert(raw))
    #[test]
    fn prop_to_engine_price_is_composition(
        raw: u64,
        invert: u8,
        scale in 0u32..=MAX_UNIT_SCALE,
    ) {
        let direct = to_engine_price(raw, invert, scale);
        let composed = invert_price_e6(raw, invert).and_then(|i| scale_price_e6(i, scale));
        prop_assert_eq!(direct, composed,
            "raw={} invert={} scale={} direct={:?} composed={:?}",
            raw, invert, scale, direct, composed);
    }

    /// Specifically: production SOL/USD prices in inverted markets.
    #[test]
    fn prop_inverted_sol_usd_production(raw in PROD_SOL_PRICE_E6_LO..=PROD_SOL_PRICE_E6_HI) {
        let got = to_engine_price(raw, 1, 0);
        prop_assert!(got.is_some(),
            "SOL/USD inverted at production range should always produce a value, raw={}", raw);
        let engine = got.unwrap();
        // engine price = 1e12 / raw. For raw in [1e7, 1e9], engine is in [1e3, 1e5].
        prop_assert!(engine >= 1_000 && engine <= 100_000,
            "raw={} engine={} out of expected bounds", raw, engine);
    }
}
