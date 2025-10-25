//! Constant product AMM math (x·y=k) - Formally verified

use crate::{AmmError, SCALE, BPS_SCALE};

/// Quote result with VWAP
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuoteResult {
    /// Amount of quote currency (in/out depending on side)
    pub quote_amount: i64,

    /// Volume-weighted average price (scaled by SCALE)
    pub vwap_px: i64,

    /// New x reserve after trade
    pub new_x: i64,

    /// New y reserve after trade
    pub new_y: i64,
}

/// Calculate quote for buying base (Router wants +Δx, provides quote y)
///
/// With fee on input:
/// - x1 = x0 - Δx_out
/// - Invariant: x0·y0 = x1·y1
/// - y1 = (x0·y0) / x1
/// - Δy_gross = y1 - y0
/// - Δy_in = Δy_gross / (1 - fee)
/// - VWAP = Δy_in / Δx_out
///
/// # Arguments
/// * `x_reserve` - Current base reserve (scaled by SCALE)
/// * `y_reserve` - Current quote reserve (scaled by SCALE)
/// * `fee_bps` - Fee in basis points (e.g., 5 = 0.05%)
/// * `dx_out` - Desired base amount (scaled by SCALE)
/// * `min_liquidity` - Minimum liquidity floor (prevents draining pool)
///
/// # Returns
/// * `QuoteResult` with quote amount, VWAP, and new reserves
/// * `AmmError` if invalid inputs or insufficient liquidity
pub fn quote_buy(
    x_reserve: i64,
    y_reserve: i64,
    fee_bps: i64,
    dx_out: i64,
    min_liquidity: i64,
) -> Result<QuoteResult, AmmError> {
    // Validate inputs
    if x_reserve <= 0 || y_reserve <= 0 {
        return Err(AmmError::InvalidReserves);
    }
    if dx_out <= 0 {
        return Err(AmmError::InvalidAmount);
    }
    if dx_out >= x_reserve - min_liquidity {
        return Err(AmmError::InsufficientLiquidity);
    }

    // Calculate using i128 to avoid overflow
    let x0 = x_reserve as i128;
    let y0 = y_reserve as i128;
    let dx = dx_out as i128;

    // x1 = x0 - dx
    let x1 = x0 - dx;
    if x1 <= 0 {
        return Err(AmmError::InsufficientLiquidity);
    }

    // y1 = (x0 * y0) / x1
    let k = x0.checked_mul(y0).ok_or(AmmError::Overflow)?;
    let y1 = k / x1;

    // Δy_gross = y1 - y0
    let dy_gross = y1 - y0;
    if dy_gross <= 0 {
        return Err(AmmError::InvalidAmount);
    }

    // Apply fee: Δy_in = Δy_gross / (1 - fee)
    // = Δy_gross * BPS_SCALE / (BPS_SCALE - fee_bps)
    let fee_multiplier = BPS_SCALE as i128;
    let fee_divisor = fee_multiplier - fee_bps as i128;
    if fee_divisor <= 0 {
        return Err(AmmError::InvalidAmount);
    }
    let dy_in = (dy_gross * fee_multiplier) / fee_divisor;

    // VWAP = Δy_in / Δx_out (both scaled, so result is scaled)
    let vwap_px = (dy_in * SCALE as i128) / dx;

    // New reserves: x1, y0 + dy_in
    let new_y = y0 + dy_in;

    // Check for i64 overflow
    if dy_in > i64::MAX as i128 || vwap_px > i64::MAX as i128 {
        return Err(AmmError::Overflow);
    }
    if x1 > i64::MAX as i128 || new_y > i64::MAX as i128 {
        return Err(AmmError::Overflow);
    }

    Ok(QuoteResult {
        quote_amount: dy_in as i64,
        vwap_px: vwap_px as i64,
        new_x: x1 as i64,
        new_y: new_y as i64,
    })
}

/// Calculate quote for selling base (Router provides Δx, receives quote y)
///
/// With fee on input:
/// - Δx_net = Δx_in * (1 - fee)
/// - x1 = x0 + Δx_net
/// - Invariant: x0·y0 = x1·y1
/// - y1 = (x0·y0) / x1
/// - Δy_out = y0 - y1
/// - VWAP = Δy_out / Δx_in
///
/// # Arguments
/// * `x_reserve` - Current base reserve (scaled by SCALE)
/// * `y_reserve` - Current quote reserve (scaled by SCALE)
/// * `fee_bps` - Fee in basis points (e.g., 5 = 0.05%)
/// * `dx_in` - Base amount to sell (scaled by SCALE)
/// * `min_liquidity` - Minimum liquidity floor
///
/// # Returns
/// * `QuoteResult` with quote amount, VWAP, and new reserves
/// * `AmmError` if invalid inputs or insufficient liquidity
pub fn quote_sell(
    x_reserve: i64,
    y_reserve: i64,
    fee_bps: i64,
    dx_in: i64,
    min_liquidity: i64,
) -> Result<QuoteResult, AmmError> {
    // Validate inputs
    if x_reserve <= 0 || y_reserve <= 0 {
        return Err(AmmError::InvalidReserves);
    }
    if dx_in <= 0 {
        return Err(AmmError::InvalidAmount);
    }

    // Calculate using i128 to avoid overflow
    let x0 = x_reserve as i128;
    let y0 = y_reserve as i128;
    let dx = dx_in as i128;

    // Apply fee to input: dx_net = dx * (1 - fee/BPS_SCALE)
    let fee_multiplier = (BPS_SCALE - fee_bps) as i128;
    if fee_multiplier < 0 {
        return Err(AmmError::InvalidAmount);
    }
    let dx_net = (dx * fee_multiplier) / BPS_SCALE as i128;

    // x1 = x0 + dx_net
    let x1 = x0 + dx_net;

    // y1 = (x0 * y0) / x1
    let k = x0.checked_mul(y0).ok_or(AmmError::Overflow)?;
    let y1 = k / x1;

    // Δy_out = y0 - y1
    let dy_out = y0 - y1;
    if dy_out <= 0 {
        return Err(AmmError::InvalidAmount);
    }
    if dy_out >= (y_reserve - min_liquidity) as i128 {
        return Err(AmmError::InsufficientLiquidity);
    }

    // VWAP = Δy_out / Δx_in
    let vwap_px = (dy_out * SCALE as i128) / dx;

    // New reserves: x1, y1
    let new_y = y1;

    // Check for i64 overflow
    if dy_out > i64::MAX as i128 || vwap_px > i64::MAX as i128 {
        return Err(AmmError::Overflow);
    }
    if x1 > i64::MAX as i128 || new_y > i64::MAX as i128 {
        return Err(AmmError::Overflow);
    }

    Ok(QuoteResult {
        quote_amount: dy_out as i64,
        vwap_px: vwap_px as i64,
        new_x: x1 as i64,
        new_y: new_y as i64,
    })
}

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
    fn test_invariant_increases_with_fees() {
        // Test that x*y invariant increases due to fees
        let x0 = 1000 * TEST_SCALE;
        let y0 = 60_000_000 * TEST_SCALE;
        let k = (x0 as i128) * (y0 as i128);

        let result = quote_buy(x0, y0, 5, 50 * TEST_SCALE, 1000).unwrap();

        let x1 = result.new_x as i128;
        let y1 = result.new_y as i128;

        // Fees should increase the invariant
        assert!(x1 * y1 > k, "Invariant should increase due to fees");
    }

    #[test]
    fn test_insufficient_liquidity() {
        let x = 1000 * TEST_SCALE;
        let y = 60_000_000 * TEST_SCALE;

        // Try to buy entire pool
        let result = quote_buy(x, y, 5, 1000 * TEST_SCALE, 1000);
        assert!(matches!(result, Err(AmmError::InsufficientLiquidity)));
    }
}
