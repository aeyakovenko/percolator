//! Constant product AMM math (x·y=k) - Imports VERIFIED functions
//!
//! ⚠️ CRITICAL: This module imports formally verified functions from amm_model.
//! All AMM math is verified by 8 Kani proofs (A1-A8).
//! DO NOT duplicate code - always use the verified functions.

use percolator_common::PercolatorError;

/// Re-export verified functions and types
pub use amm_model::{self, quote_buy, quote_sell, QuoteResult, SCALE, BPS_SCALE};

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_SCALE: i64 = SCALE;

    #[test]
    fn test_quote_buy_small() {
        // x=1000 contracts, y=60M quote units (spot ~60k), fee=5bps
        let x = 1000 * TEST_SCALE;
        let y = 60_000_000 * TEST_SCALE;

        // Buy 1 contract
        let result = quote_buy(x, y, 5, 1 * TEST_SCALE, 1000).unwrap();

        // Should cost slightly more than spot due to slippage + fee
        assert!(result.vwap_px > 60_000 * TEST_SCALE);
        assert!(result.vwap_px < 61_000 * TEST_SCALE);

        // Reserves should update correctly
        assert_eq!(result.new_x, x - 1 * TEST_SCALE);
        assert!(result.new_y > y);
    }

    #[test]
    fn test_quote_sell_small() {
        let x = 1000 * TEST_SCALE;
        let y = 60_000_000 * TEST_SCALE;

        // Sell 1 contract
        let result = quote_sell(x, y, 5, 1 * TEST_SCALE, 1000).unwrap();

        // VWAP should be slightly less than spot
        assert!(result.vwap_px < 60_000 * TEST_SCALE);
        assert!(result.vwap_px > 59_000 * TEST_SCALE);

        // Reserves should update correctly
        assert!(result.new_x > x);
        assert!(result.new_y < y);
    }

    #[test]
    fn test_insufficient_liquidity() {
        let x = 1000 * TEST_SCALE;
        let y = 60_000_000 * TEST_SCALE;

        // Try to buy entire pool
        let result = quote_buy(x, y, 5, 1000 * TEST_SCALE, 1000);
        assert!(result.is_err());
    }

    #[test]
    fn test_fee_accounting() {
        let x = 1000 * TEST_SCALE;
        let y = 60_000_000 * TEST_SCALE;

        // Quote with fee
        let with_fee = quote_buy(x, y, 5, 10 * TEST_SCALE, 1000).unwrap();

        // Quote without fee
        let no_fee = quote_buy(x, y, 0, 10 * TEST_SCALE, 1000).unwrap();

        // With fee should cost more
        assert!(with_fee.quote_amount > no_fee.quote_amount);
        assert!(with_fee.vwap_px > no_fee.vwap_px);
    }

    #[test]
    fn test_invariant_preservation_buy() {
        // Test that x*y invariant is preserved (accounting for fees)
        let x0 = 1000 * TEST_SCALE;
        let y0 = 60_000_000 * TEST_SCALE;
        let k = (x0 as i128) * (y0 as i128);

        let result = quote_buy(x0, y0, 5, 50 * TEST_SCALE, 1000).unwrap();

        // After buy: x decreases, y increases (by more than quote due to fee)
        let x1 = result.new_x as i128;
        let y1 = result.new_y as i128;

        // The pool receives more than k requires due to fee
        assert!(x1 * y1 > k, "Invariant should increase due to fees");
    }

    #[test]
    fn test_round_trip_loses_to_fees() {
        // Buy then sell should lose money due to fees
        let x = 1000 * TEST_SCALE;
        let y = 60_000_000 * TEST_SCALE;
        let amount = 10 * TEST_SCALE;

        // Buy 10 contracts
        let buy_result = quote_buy(x, y, 5, amount, 1000).unwrap();
        let cost = buy_result.quote_amount;

        // Sell 10 contracts back (using new reserves)
        let sell_result = quote_sell(buy_result.new_x, buy_result.new_y, 5, amount, 1000).unwrap();
        let proceeds = sell_result.quote_amount;

        // Should lose money due to fees and slippage
        assert!(proceeds < cost, "Round-trip should lose to fees");
    }
}
