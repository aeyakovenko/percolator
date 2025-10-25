//! Fixed-point math utilities

/// Fixed-point precision (6 decimals)
pub const PRICE_DECIMALS: u32 = 6;
pub const PRICE_MULTIPLIER: u64 = 1_000_000;

/// Multiply two u64 values and return u128
#[inline]
pub fn mul_u64(a: u64, b: u64) -> u128 {
    (a as u128) * (b as u128)
}

/// Multiply u64 by u128
#[inline]
pub fn mul_u64_u128(a: u64, b: u128) -> u128 {
    (a as u128) * b
}

/// Divide u128 by u64, rounding up
#[inline]
pub fn div_ceil_u128(numerator: u128, denominator: u64) -> u128 {
    let denom = denominator as u128;
    (numerator + denom - 1) / denom
}

/// Divide u128 by u64, rounding down
#[inline]
pub fn div_floor_u128(numerator: u128, denominator: u64) -> u128 {
    numerator / (denominator as u128)
}

/// Calculate VWAP: (total_notional / total_qty)
#[inline]
pub fn calculate_vwap(total_notional: u128, total_qty: u64) -> u64 {
    if total_qty == 0 {
        return 0;
    }
    (total_notional / (total_qty as u128)) as u64
}

/// Update VWAP with new fill
/// Returns new (total_qty, total_notional)
#[inline]
pub fn update_vwap(
    current_qty: u64,
    current_notional: u128,
    fill_qty: u64,
    fill_price: u64,
) -> (u64, u128) {
    let new_qty = current_qty + fill_qty;
    let new_notional = current_notional + mul_u64(fill_qty, fill_price);
    (new_qty, new_notional)
}

/// Calculate position PnL
/// PnL = qty * (current_price - entry_price)
#[inline]
pub fn calculate_pnl(qty: i64, entry_price: u64, current_price: u64) -> i128 {
    let qty_i128 = qty as i128;
    let entry_i128 = entry_price as i128;
    let current_i128 = current_price as i128;
    qty_i128 * (current_i128 - entry_i128)
}

/// Calculate funding payment
/// Payment = qty * (cum_funding_current - cum_funding_entry)
#[inline]
pub fn calculate_funding_payment(qty: i64, cum_funding_current: i128, cum_funding_entry: i128) -> i128 {
    let qty_i128 = qty as i128;
    qty_i128 * (cum_funding_current - cum_funding_entry)
}

/// Check if price is within tick alignment
#[inline]
pub fn is_tick_aligned(price: u64, tick: u64) -> bool {
    price % tick == 0
}

/// Check if quantity is within lot alignment
#[inline]
pub fn is_lot_aligned(qty: u64, lot: u64) -> bool {
    qty % lot == 0
}

/// Round price to tick
#[inline]
pub fn round_to_tick(price: u64, tick: u64) -> u64 {
    (price / tick) * tick
}

/// Round quantity to lot
#[inline]
pub fn round_to_lot(qty: u64, lot: u64) -> u64 {
    (qty / lot) * lot
}

/// Calculate IM requirement: |qty| * contract_size * mark_price * imr
#[inline]
pub fn calculate_im(qty: i64, contract_size: u64, mark_price: u64, imr_bps: u64) -> u128 {
    let abs_qty = qty.abs() as u64;
    let notional = mul_u64(abs_qty, contract_size);
    let notional_value = mul_u64_u128(mark_price, notional);
    // imr_bps is in basis points (1 bp = 0.01%)
    (notional_value * (imr_bps as u128)) / 10_000
}

/// Calculate MM requirement: |qty| * contract_size * mark_price * mmr
#[inline]
pub fn calculate_mm(qty: i64, contract_size: u64, mark_price: u64, mmr_bps: u64) -> u128 {
    let abs_qty = qty.abs() as u64;
    let notional = mul_u64(abs_qty, contract_size);
    let notional_value = mul_u64_u128(mark_price, notional);
    // mmr_bps is in basis points (1 bp = 0.01%)
    (notional_value * (mmr_bps as u128)) / 10_000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vwap_calculation() {
        let (qty, notional) = update_vwap(0, 0, 100, 50_000);
        assert_eq!(qty, 100);
        assert_eq!(notional, 5_000_000);
        assert_eq!(calculate_vwap(notional, qty), 50_000);

        let (qty, notional) = update_vwap(qty, notional, 50, 51_000);
        assert_eq!(qty, 150);
        let vwap = calculate_vwap(notional, qty);
        // VWAP should be (100*50000 + 50*51000) / 150 = 50333.33...
        assert!(vwap >= 50_333 && vwap <= 50_334);
    }

    #[test]
    fn test_pnl_calculation() {
        // Long position profit
        let pnl = calculate_pnl(10, 50_000, 51_000);
        assert_eq!(pnl, 10_000);

        // Long position loss
        let pnl = calculate_pnl(10, 50_000, 49_000);
        assert_eq!(pnl, -10_000);

        // Short position profit
        let pnl = calculate_pnl(-10, 50_000, 49_000);
        assert_eq!(pnl, 10_000);

        // Short position loss
        let pnl = calculate_pnl(-10, 50_000, 51_000);
        assert_eq!(pnl, -10_000);
    }
}

// ═══════════════════════════════════════════════════════════════
// KANI FORMAL VERIFICATION PROOFS
// ═══════════════════════════════════════════════════════════════

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    /// M1: VWAP is always between min and max fill price
    ///
    /// Property: For any sequence of fills, VWAP lies within [min_price, max_price]
    #[kani::proof]
    #[kani::unwind(3)]
    fn m1_vwap_bounded() {
        let price1: u64 = kani::any();
        let price2: u64 = kani::any();
        let qty1: u64 = kani::any();
        let qty2: u64 = kani::any();

        // Constrain to reasonable values
        kani::assume(price1 > 0 && price1 < 1_000_000_000); // Max 1B
        kani::assume(price2 > 0 && price2 < 1_000_000_000);
        kani::assume(qty1 > 0 && qty1 < 1_000_000); // Max 1M qty
        kani::assume(qty2 > 0 && qty2 < 1_000_000);

        // Two fills
        let (total_qty, total_notional) = update_vwap(0, 0, qty1, price1);
        let (total_qty, total_notional) = update_vwap(total_qty, total_notional, qty2, price2);

        let vwap = calculate_vwap(total_notional, total_qty);

        let min_price = price1.min(price2);
        let max_price = price1.max(price2);

        // VWAP must be within [min, max]
        assert!(vwap >= min_price, "M1: VWAP below min price");
        assert!(vwap <= max_price, "M1: VWAP above max price");
    }

    /// M2: PnL symmetry - long profit equals short loss
    ///
    /// Property: For same qty magnitude and price movement, long profit = -short loss
    #[kani::proof]
    #[kani::unwind(3)]
    fn m2_pnl_symmetry() {
        let qty: i64 = kani::any();
        let entry_price: u64 = kani::any();
        let current_price: u64 = kani::any();

        kani::assume(qty > 0 && qty < 1_000_000);
        kani::assume(entry_price > 0 && entry_price < 1_000_000_000);
        kani::assume(current_price > 0 && current_price < 1_000_000_000);

        let long_pnl = calculate_pnl(qty, entry_price, current_price);
        let short_pnl = calculate_pnl(-qty, entry_price, current_price);

        // Long PnL + Short PnL = 0 (symmetry)
        assert!(long_pnl == -short_pnl, "M2: PnL symmetry violated");
    }

    /// M3: PnL sign correctness for long positions
    ///
    /// Property: Long position has positive PnL when price increases
    #[kani::proof]
    #[kani::unwind(3)]
    fn m3_pnl_sign_long() {
        let qty: i64 = kani::any();
        let entry_price: u64 = kani::any();
        let current_price: u64 = kani::any();

        kani::assume(qty > 0 && qty < 1_000_000);
        kani::assume(entry_price > 1000 && entry_price < 1_000_000_000);
        kani::assume(current_price > entry_price); // Price went up

        let pnl = calculate_pnl(qty, entry_price, current_price);

        // Long position with price increase must have positive PnL
        assert!(pnl > 0, "M3: Long PnL should be positive when price rises");
    }

    /// M4: PnL sign correctness for short positions
    ///
    /// Property: Short position has positive PnL when price decreases
    #[kani::proof]
    #[kani::unwind(3)]
    fn m4_pnl_sign_short() {
        let qty: i64 = kani::any();
        let entry_price: u64 = kani::any();
        let current_price: u64 = kani::any();

        kani::assume(qty > 0 && qty < 1_000_000);
        kani::assume(entry_price > 1000 && entry_price < 1_000_000_000);
        kani::assume(current_price < entry_price); // Price went down

        let pnl = calculate_pnl(-qty, entry_price, current_price);

        // Short position with price decrease must have positive PnL
        assert!(pnl > 0, "M4: Short PnL should be positive when price falls");
    }

    /// M5: Margin requirements scale linearly with quantity
    ///
    /// Property: IM(2*qty) = 2 * IM(qty)
    #[kani::proof]
    #[kani::unwind(3)]
    fn m5_margin_linearity() {
        let qty: i64 = kani::any();
        let contract_size: u64 = kani::any();
        let mark_price: u64 = kani::any();
        let imr_bps: u64 = kani::any();

        kani::assume(qty > 0 && qty < 100_000); // Keep small for overflow
        kani::assume(contract_size > 0 && contract_size < 10_000);
        kani::assume(mark_price > 0 && mark_price < 100_000);
        kani::assume(imr_bps > 0 && imr_bps < 10_000); // Max 100%

        let im1 = calculate_im(qty, contract_size, mark_price, imr_bps);
        let im2 = calculate_im(2 * qty, contract_size, mark_price, imr_bps);

        // IM should scale linearly (allowing for rounding error of ±1)
        let expected = im1 * 2;
        let diff = if im2 > expected { im2 - expected } else { expected - im2 };

        assert!(diff <= 1, "M5: Margin should scale linearly with quantity");
    }

    /// M6: Maintenance margin always less than or equal to initial margin
    ///
    /// Property: For same position, MM <= IM (if mmr_bps <= imr_bps)
    #[kani::proof]
    #[kani::unwind(3)]
    fn m6_mm_less_than_im() {
        let qty: i64 = kani::any();
        let contract_size: u64 = kani::any();
        let mark_price: u64 = kani::any();
        let imr_bps: u64 = kani::any();
        let mmr_bps: u64 = kani::any();

        kani::assume(qty != 0 && qty.abs() < 1_000_000);
        kani::assume(contract_size > 0 && contract_size < 10_000);
        kani::assume(mark_price > 0 && mark_price < 100_000);
        kani::assume(imr_bps > 0 && imr_bps < 10_000);
        kani::assume(mmr_bps > 0 && mmr_bps <= imr_bps); // MM rate <= IM rate

        let im = calculate_im(qty, contract_size, mark_price, imr_bps);
        let mm = calculate_mm(qty, contract_size, mark_price, mmr_bps);

        // MM must be <= IM when mmr_bps <= imr_bps
        assert!(mm <= im, "M6: Maintenance margin must be <= initial margin");
    }

    /// M7: Division rounding modes behave correctly
    ///
    /// Property: div_ceil >= div_floor, and difference is at most 1
    #[kani::proof]
    #[kani::unwind(3)]
    fn m7_rounding_modes() {
        let numerator: u128 = kani::any();
        let denominator: u64 = kani::any();

        kani::assume(numerator > 0 && numerator < u128::MAX / 2);
        kani::assume(denominator > 0);

        let ceil = div_ceil_u128(numerator, denominator);
        let floor = div_floor_u128(numerator, denominator);

        // Ceiling must be >= floor
        assert!(ceil >= floor, "M7: Ceiling must be >= floor");

        // Difference must be at most 1
        assert!(ceil - floor <= 1, "M7: Ceiling and floor differ by at most 1");

        // If evenly divisible, they should be equal
        if numerator % (denominator as u128) == 0 {
            assert!(ceil == floor, "M7: Exact division should have ceil == floor");
        }
    }

    /// M8: Tick and lot alignment is idempotent
    ///
    /// Property: Rounding already-aligned value returns same value
    #[kani::proof]
    #[kani::unwind(3)]
    fn m8_alignment_idempotent() {
        let price: u64 = kani::any();
        let tick: u64 = kani::any();

        kani::assume(tick > 0 && tick < 1_000_000);
        kani::assume(price > 0 && price < u64::MAX / 2);

        let rounded = round_to_tick(price, tick);

        // Rounding twice should give same result
        let rounded_twice = round_to_tick(rounded, tick);
        assert!(rounded == rounded_twice, "M8: Tick rounding is idempotent");

        // Rounded value must be tick-aligned
        assert!(is_tick_aligned(rounded, tick), "M8: Rounded value must be aligned");
    }

    /// M9: Wide multiplication doesn't overflow
    ///
    /// Property: Multiplying two u64 values and storing in u128 never overflows
    #[kani::proof]
    #[kani::unwind(3)]
    fn m9_wide_mul_safe() {
        let a: u64 = kani::any();
        let b: u64 = kani::any();

        // This should never panic
        let result = mul_u64(a, b);

        // Result should equal mathematical product
        assert!(result == (a as u128) * (b as u128), "M9: Wide multiply correctness");
    }

    /// M10: Funding payment symmetry
    ///
    /// Property: Long payment = -Short payment for same funding rate
    #[kani::proof]
    #[kani::unwind(3)]
    fn m10_funding_symmetry() {
        let qty: i64 = kani::any();
        let cum_current: i128 = kani::any();
        let cum_entry: i128 = kani::any();

        kani::assume(qty > 0 && qty < 1_000_000);
        kani::assume(cum_current > i128::MIN / 2 && cum_current < i128::MAX / 2);
        kani::assume(cum_entry > i128::MIN / 2 && cum_entry < i128::MAX / 2);

        let long_payment = calculate_funding_payment(qty, cum_current, cum_entry);
        let short_payment = calculate_funding_payment(-qty, cum_current, cum_entry);

        // Long and short funding payments should sum to zero
        assert!(long_payment == -short_payment, "M10: Funding payment symmetry");
    }
}
