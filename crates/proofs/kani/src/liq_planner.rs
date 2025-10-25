//! Kani proofs for liquidation planner logic
//!
//! These proofs verify price band calculation and oracle alignment:
//! - **LP1: Price Band Symmetry** - Band is symmetric around oracle price
//! - **LP2: Band Width** - Band width proportional to bps parameter
//! - **LP3: Oracle Alignment** - Validates price within tolerance
//! - **LP4: Cap Enforcement** - Per-slab caps never exceeded

/// Calculate price band: [oracle * (1 - bps/10000), oracle * (1 + bps/10000)]
fn calculate_price_band(oracle_price: i64, band_bps: u16) -> (i64, i64) {
    let bps = band_bps as i64;

    // Calculate discount: oracle * bps / 10000
    let discount = (oracle_price * bps) / 10_000;

    let band_low = oracle_price - discount;
    let band_high = oracle_price + discount;

    (band_low, band_high)
}

/// Validate oracle alignment: check if mark_price is within tolerance of oracle_price
fn validate_oracle_alignment(mark_price: i64, oracle_price: i64, tolerance_bps: u16) -> bool {
    let tolerance = tolerance_bps as i64;

    // Calculate acceptable range
    let max_diff = (oracle_price * tolerance) / 10_000;
    let diff = (mark_price - oracle_price).abs();

    diff <= max_diff
}

/// Apply per-slab cap to quantity
fn apply_slab_cap(qty: i64, cap: u64) -> i64 {
    let cap_i64 = cap as i64;
    qty.min(cap_i64)
}

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    /// LP1: Price band is symmetric around oracle price
    ///
    /// Property: oracle - low == high - oracle
    #[kani::proof]
    #[kani::unwind(3)]
    fn lp1_band_symmetry() {
        let oracle_price: i64 = kani::any();
        let band_bps: u16 = kani::any();

        kani::assume(oracle_price > 10_000 && oracle_price < 1_000_000_000);
        kani::assume(band_bps > 0 && band_bps < 10_000); // Max 100%

        let (low, high) = calculate_price_band(oracle_price, band_bps);

        let low_dist = oracle_price - low;
        let high_dist = high - oracle_price;

        // Band should be symmetric (allowing for rounding error of ±1)
        let diff = (low_dist - high_dist).abs();
        assert!(diff <= 1, "LP1: Price band should be symmetric");
    }

    /// LP2: Price band width proportional to bps
    ///
    /// Property: Larger bps gives wider band
    #[kani::proof]
    #[kani::unwind(3)]
    fn lp2_band_width_monotonic() {
        let oracle_price: i64 = kani::any();
        let bps1: u16 = kani::any();
        let bps2: u16 = kani::any();

        kani::assume(oracle_price > 10_000 && oracle_price < 1_000_000_000);
        kani::assume(bps1 > 0 && bps1 < 5_000);
        kani::assume(bps2 > bps1 && bps2 < 5_000); // bps2 > bps1

        let (low1, high1) = calculate_price_band(oracle_price, bps1);
        let (low2, high2) = calculate_price_band(oracle_price, bps2);

        let width1 = high1 - low1;
        let width2 = high2 - low2;

        // Larger bps should give wider band
        assert!(width2 >= width1, "LP2: Band width should increase with bps");
    }

    /// LP3: Oracle price always within band
    ///
    /// Property: low <= oracle <= high
    #[kani::proof]
    #[kani::unwind(3)]
    fn lp3_oracle_within_band() {
        let oracle_price: i64 = kani::any();
        let band_bps: u16 = kani::any();

        kani::assume(oracle_price > 10_000 && oracle_price < 1_000_000_000);
        kani::assume(band_bps > 0 && band_bps < 10_000);

        let (low, high) = calculate_price_band(oracle_price, band_bps);

        // Oracle price must be within its own band
        assert!(low <= oracle_price, "LP3: Low bound must be <= oracle");
        assert!(oracle_price <= high, "LP3: Oracle must be <= high bound");
    }

    /// LP4: Price band bounds are positive
    ///
    /// Property: Both bounds remain positive
    #[kani::proof]
    #[kani::unwind(3)]
    fn lp4_positive_bounds() {
        let oracle_price: i64 = kani::any();
        let band_bps: u16 = kani::any();

        kani::assume(oracle_price > 10_000 && oracle_price < 1_000_000_000);
        kani::assume(band_bps > 0 && band_bps < 10_000);

        let (low, high) = calculate_price_band(oracle_price, band_bps);

        // Both bounds must remain positive
        assert!(low > 0, "LP4: Low bound must be positive");
        assert!(high > 0, "LP4: High bound must be positive");
    }

    /// LP5: Oracle alignment is reflexive
    ///
    /// Property: A price is always aligned with itself
    #[kani::proof]
    #[kani::unwind(3)]
    fn lp5_alignment_reflexive() {
        let price: i64 = kani::any();
        let tolerance_bps: u16 = kani::any();

        kani::assume(price > 0 && price < 1_000_000_000);
        kani::assume(tolerance_bps > 0 && tolerance_bps < 10_000);

        let aligned = validate_oracle_alignment(price, price, tolerance_bps);

        // A price should always be aligned with itself
        assert!(aligned, "LP5: Price should be aligned with itself");
    }

    /// LP6: Oracle alignment is symmetric
    ///
    /// Property: If A is aligned with B, then B is aligned with A
    #[kani::proof]
    #[kani::unwind(3)]
    fn lp6_alignment_symmetric() {
        let price_a: i64 = kani::any();
        let price_b: i64 = kani::any();
        let tolerance_bps: u16 = kani::any();

        kani::assume(price_a > 10_000 && price_a < 1_000_000_000);
        kani::assume(price_b > 10_000 && price_b < 1_000_000_000);
        kani::assume(tolerance_bps > 0 && tolerance_bps < 10_000);

        let a_to_b = validate_oracle_alignment(price_a, price_b, tolerance_bps);
        let b_to_a = validate_oracle_alignment(price_b, price_a, tolerance_bps);

        // Alignment should be symmetric
        assert!(a_to_b == b_to_a, "LP6: Alignment should be symmetric");
    }

    /// LP7: Tighter tolerance is more restrictive
    ///
    /// Property: If aligned at tol1, also aligned at tol2 > tol1
    #[kani::proof]
    #[kani::unwind(3)]
    fn lp7_tolerance_monotonic() {
        let mark_price: i64 = kani::any();
        let oracle_price: i64 = kani::any();
        let tol1: u16 = kani::any();
        let tol2: u16 = kani::any();

        kani::assume(mark_price > 10_000 && mark_price < 1_000_000_000);
        kani::assume(oracle_price > 10_000 && oracle_price < 1_000_000_000);
        kani::assume(tol1 > 0 && tol1 < 5_000);
        kani::assume(tol2 > tol1 && tol2 < 5_000);

        let aligned_tight = validate_oracle_alignment(mark_price, oracle_price, tol1);
        let aligned_loose = validate_oracle_alignment(mark_price, oracle_price, tol2);

        // If aligned at tight tolerance, must be aligned at loose tolerance
        if aligned_tight {
            assert!(aligned_loose, "LP7: Loose tolerance should accept what tight tolerance accepts");
        }
    }

    /// LP8: Slab cap never exceeded
    ///
    /// Property: apply_slab_cap(qty, cap) <= cap
    #[kani::proof]
    #[kani::unwind(3)]
    fn lp8_cap_enforced() {
        let qty: i64 = kani::any();
        let cap: u64 = kani::any();

        kani::assume(qty > 0 && qty < i64::MAX / 2);
        kani::assume(cap > 0 && cap < u64::MAX / 2);

        let capped = apply_slab_cap(qty, cap);

        // Result must not exceed cap
        assert!(capped <= (cap as i64), "LP8: Capped qty must not exceed cap");
    }

    /// LP9: Slab cap is idempotent
    ///
    /// Property: Capping twice gives same result as capping once
    #[kani::proof]
    #[kani::unwind(3)]
    fn lp9_cap_idempotent() {
        let qty: i64 = kani::any();
        let cap: u64 = kani::any();

        kani::assume(qty > 0 && qty < i64::MAX / 2);
        kani::assume(cap > 0 && cap < u64::MAX / 2);

        let capped_once = apply_slab_cap(qty, cap);
        let capped_twice = apply_slab_cap(capped_once, cap);

        // Capping twice should equal capping once
        assert!(capped_once == capped_twice, "LP9: Cap should be idempotent");
    }

    /// LP10: Small qty unchanged by large cap
    ///
    /// Property: If qty < cap, result == qty
    #[kani::proof]
    #[kani::unwind(3)]
    fn lp10_small_qty_unchanged() {
        let qty: i64 = kani::any();
        let cap: u64 = kani::any();

        kani::assume(qty > 0 && qty < 100_000);
        kani::assume(cap > (qty as u64) && cap < u64::MAX / 2);

        let capped = apply_slab_cap(qty, cap);

        // If qty < cap, it should be unchanged
        assert!(capped == qty, "LP10: Small qty should be unchanged by large cap");
    }

    /// LP11: Zero tolerance only accepts exact match
    ///
    /// Property: tolerance = 0 means prices must be equal
    #[kani::proof]
    #[kani::unwind(3)]
    fn lp11_zero_tolerance() {
        let mark_price: i64 = kani::any();
        let oracle_price: i64 = kani::any();

        kani::assume(mark_price > 10_000 && mark_price < 1_000_000_000);
        kani::assume(oracle_price > 10_000 && oracle_price < 1_000_000_000);
        kani::assume(mark_price != oracle_price); // Different prices

        let aligned = validate_oracle_alignment(mark_price, oracle_price, 0);

        // With zero tolerance, different prices cannot be aligned
        assert!(!aligned, "LP11: Zero tolerance should reject different prices");
    }

    /// LP12: Band width is zero at zero bps
    ///
    /// Property: band_bps = 0 => low == high == oracle
    #[kani::proof]
    #[kani::unwind(3)]
    fn lp12_zero_band() {
        let oracle_price: i64 = kani::any();

        kani::assume(oracle_price > 10_000 && oracle_price < 1_000_000_000);

        let (low, high) = calculate_price_band(oracle_price, 0);

        // Zero band should collapse to oracle price
        assert!(low == oracle_price, "LP12: Zero band low should equal oracle");
        assert!(high == oracle_price, "LP12: Zero band high should equal oracle");
    }
}
