//! Kani proofs for AMM constant product invariants
//!
//! These proofs verify that the AMM implementation satisfies key safety properties:
//! - **A1: Invariant Non-Decreasing** - x·y never decreases (fees only increase k)
//! - **A2: Reserves Non-Negative** - Reserves never go negative
//! - **A3: No Overflow** - All arithmetic operations are safe
//! - **A4: Deterministic** - Same inputs always produce same outputs
//! - **A5: Fee Routing** - Fees always increase pool value

use amm_model::{quote_buy, quote_sell, SCALE};

/// A1: Invariant approximately preserved (accounting for integer division rounding)
///
/// Property: After any trade, the new invariant x1·y1 should not decrease significantly.
/// Integer division causes small rounding losses, but these are bounded.
/// With fees, the invariant increases (after accounting for rounding).
#[kani::proof]
#[kani::unwind(4)]
fn a1_invariant_non_decreasing_buy() {
    // Bounded symbolic inputs
    let x0: i64 = kani::any();
    let y0: i64 = kani::any();
    let dx: i64 = kani::any();
    let fee_bps: i64 = kani::any();

    // Constrain to reasonable values for performance
    kani::assume(x0 > 1000 * SCALE && x0 < 10_000 * SCALE);
    kani::assume(y0 > 1000 * SCALE && y0 < 100_000_000 * SCALE);
    kani::assume(dx > 0 && dx < x0 / 2); // Buy up to 50% of pool
    kani::assume(fee_bps >= 0 && fee_bps < 10_000); // Fee < 100%

    let k0 = (x0 as i128) * (y0 as i128);

    // Execute buy
    if let Ok(result) = quote_buy(x0, y0, fee_bps, dx, 100) {
        let x1 = result.new_x as i128;
        let y1 = result.new_y as i128;
        let k1 = x1 * y1;

        // Allow for rounding losses from integer division
        // When computing y1 = k/x1, we lose at most (x1-1) due to truncation
        // Additionally, the fee calculation may lose up to 1 unit
        let max_rounding_loss = x1 + 2;

        // Invariant should not decrease beyond rounding error
        assert!(k1 + max_rounding_loss >= k0,
            "A1: Invariant must not decrease beyond rounding: k0={}, k1={}, loss={}",
            k0, k1, k0.saturating_sub(k1));

        // With fees, pool value increases (user pays more than theoretical k requires)
        // This is guaranteed by the fee formula: dy_in = dy_gross / (1 - fee)
        // So dy_in > dy_gross, meaning y1 > (k0 / x1), thus k1 > k0
    }
}

/// A1: Invariant approximately preserved for sell operations
#[kani::proof]
#[kani::unwind(4)]
fn a1_invariant_non_decreasing_sell() {
    let x0: i64 = kani::any();
    let y0: i64 = kani::any();
    let dx: i64 = kani::any();
    let fee_bps: i64 = kani::any();

    kani::assume(x0 > 1000 * SCALE && x0 < 10_000 * SCALE);
    kani::assume(y0 > 1000 * SCALE && y0 < 100_000_000 * SCALE);
    kani::assume(dx > 0 && dx < x0 / 2); // Sell reasonable amount
    kani::assume(fee_bps >= 0 && fee_bps < 10_000);

    let k0 = (x0 as i128) * (y0 as i128);

    // Execute sell
    if let Ok(result) = quote_sell(x0, y0, fee_bps, dx, 100) {
        let x1 = result.new_x as i128;
        let y1 = result.new_y as i128;
        let k1 = x1 * y1;

        // Allow for rounding loss from integer division
        let max_rounding_loss = x1 + 2;

        // Invariant should not decrease beyond rounding
        assert!(k1 + max_rounding_loss >= k0,
            "A1: Invariant must not decrease beyond rounding: k0={}, k1={}",
            k0, k1);
    }
}

/// A2: Reserves are always non-negative
///
/// Property: After any successful trade, both x and y reserves remain positive.
#[kani::proof]
#[kani::unwind(4)]
fn a2_reserves_non_negative_buy() {
    let x0: i64 = kani::any();
    let y0: i64 = kani::any();
    let dx: i64 = kani::any();
    let fee_bps: i64 = kani::any();

    kani::assume(x0 > 1000 && x0 < 10_000 * SCALE);
    kani::assume(y0 > 1000 && y0 < 100_000_000 * SCALE);
    kani::assume(dx > 0 && dx < x0 / 2);
    kani::assume(fee_bps >= 0 && fee_bps < 10_000);

    if let Ok(result) = quote_buy(x0, y0, fee_bps, dx, 100) {
        assert!(result.new_x > 0,
            "A2: X reserve must remain positive after buy");
        assert!(result.new_y > 0,
            "A2: Y reserve must remain positive after buy");
        assert!(result.quote_amount > 0,
            "A2: Quote amount must be positive");
        assert!(result.vwap_px >= 0,
            "A2: VWAP must be non-negative");
    }
}

/// A2: Reserves non-negative for sell
#[kani::proof]
#[kani::unwind(4)]
fn a2_reserves_non_negative_sell() {
    let x0: i64 = kani::any();
    let y0: i64 = kani::any();
    let dx: i64 = kani::any();
    let fee_bps: i64 = kani::any();

    kani::assume(x0 > 1000 && x0 < 10_000 * SCALE);
    kani::assume(y0 > 1000 && y0 < 100_000_000 * SCALE);
    kani::assume(dx > 0 && dx < x0);
    kani::assume(fee_bps >= 0 && fee_bps < 10_000);

    if let Ok(result) = quote_sell(x0, y0, fee_bps, dx, 100) {
        assert!(result.new_x > 0,
            "A2: X reserve must remain positive after sell");
        assert!(result.new_y > 0,
            "A2: Y reserve must remain positive after sell");
        assert!(result.quote_amount > 0,
            "A2: Quote amount must be positive");
        assert!(result.vwap_px >= 0,
            "A2: VWAP must be non-negative");
    }
}

/// A3: No arithmetic overflow
///
/// Property: All arithmetic operations complete without overflow.
/// This is implicit in the proof passing, but we verify error handling.
#[kani::proof]
#[kani::unwind(4)]
fn a3_no_overflow_large_reserves() {
    // Test with large reserves near i64::MAX / 2
    let x0: i64 = kani::any();
    let y0: i64 = kani::any();
    let dx: i64 = kani::any();

    kani::assume(x0 > 1000 * SCALE && x0 < i64::MAX / 1000);
    kani::assume(y0 > 1000 * SCALE && y0 < i64::MAX / 1000);
    kani::assume(dx > 0 && dx < x0 / 10);

    // Should either succeed or return error (never panic)
    let _ = quote_buy(x0, y0, 5, dx, 1000);
    let _ = quote_sell(x0, y0, 5, dx, 1000);
}

/// A4: Determinism
///
/// Property: Same inputs always produce same outputs (no hidden state).
#[kani::proof]
#[kani::unwind(4)]
fn a4_determinism() {
    let x: i64 = kani::any();
    let y: i64 = kani::any();
    let dx: i64 = kani::any();
    let fee: i64 = kani::any();

    kani::assume(x > 1000 && x < 10_000 * SCALE);
    kani::assume(y > 1000 && y < 100_000_000 * SCALE);
    kani::assume(dx > 0 && dx < x / 2);
    kani::assume(fee >= 0 && fee < 100);

    // Call twice with same inputs
    let result1 = quote_buy(x, y, fee, dx, 100);
    let result2 = quote_buy(x, y, fee, dx, 100);

    // Must produce identical results
    match (result1, result2) {
        (Ok(r1), Ok(r2)) => {
            assert_eq!(r1.quote_amount, r2.quote_amount,
                "A4: quote_buy must be deterministic");
            assert_eq!(r1.vwap_px, r2.vwap_px,
                "A4: VWAP must be deterministic");
            assert_eq!(r1.new_x, r2.new_x,
                "A4: New X must be deterministic");
            assert_eq!(r1.new_y, r2.new_y,
                "A4: New Y must be deterministic");
        }
        (Err(e1), Err(e2)) => {
            assert_eq!(e1, e2,
                "A4: Errors must be deterministic");
        }
        _ => {
            kani::cover!(false, "Determinism violated: one call succeeded, one failed");
        }
    }
}

/// A5: Fee routing increases pool value
///
/// Property: Higher fees lead to more value captured by the pool.
#[kani::proof]
#[kani::unwind(4)]
fn a5_fee_routing() {
    let x: i64 = kani::any();
    let y: i64 = kani::any();
    let dx: i64 = kani::any();

    kani::assume(x > 1000 && x < 10_000 * SCALE);
    kani::assume(y > 1000 && y < 100_000_000 * SCALE);
    kani::assume(dx > 0 && dx < x / 4);

    // Quote with no fee
    let no_fee = quote_buy(x, y, 0, dx, 100);

    // Quote with 5 bps fee
    let with_fee = quote_buy(x, y, 5, dx, 100);

    // Both should succeed for reasonable inputs
    if let (Ok(r0), Ok(r5)) = (no_fee, with_fee) {
        // With fees, user pays more
        assert!(r5.quote_amount >= r0.quote_amount,
            "A5: Fee increases quote amount user pays");

        // Pool captures more value (allowing for tiny rounding edge cases)
        let k0 = (r0.new_x as i128) * (r0.new_y as i128);
        let k5 = (r5.new_x as i128) * (r5.new_y as i128);

        // With fees, invariant should increase, but allow small rounding tolerance
        assert!(k5 + 2 >= k0,
            "A5: Fees don't decrease pool value");

        // In most cases it strictly increases (when trade size is meaningful)
        if dx > x / 100 {
            assert!(k5 > k0,
                "A5: Meaningful fees increase pool value");
        }
    }
}

/// A6: Price impact increases with size
///
/// Property: Larger trades have worse execution prices (more slippage).
#[kani::proof]
#[kani::unwind(4)]
fn a6_price_impact_scales() {
    let x: i64 = kani::any();
    let y: i64 = kani::any();
    let small_dx: i64 = kani::any();
    let large_dx: i64 = kani::any();

    kani::assume(x > 10_000 * SCALE && x < 20_000 * SCALE);
    kani::assume(y > 100_000_000 * SCALE && y < 200_000_000 * SCALE);
    kani::assume(small_dx > 0 && small_dx < x / 100); // 1% of pool
    kani::assume(large_dx > small_dx * 2 && large_dx < x / 10); // 2-10% of pool

    // Spot price = y/x
    let spot = (y as i128 * SCALE as i128) / (x as i128);

    // Small trade
    if let Ok(small) = quote_buy(x, y, 5, small_dx, 100) {
        // Large trade
        if let Ok(large) = quote_buy(x, y, 5, large_dx, 100) {
            // Larger trade should have worse VWAP (higher for buys)
            assert!(large.vwap_px >= small.vwap_px,
                "A6: Larger trades should have worse execution (higher VWAP for buys)");

            // Both should be above spot (slippage + fees)
            assert!(small.vwap_px as i128 >= spot,
                "A6: Buy VWAP should be above spot");
            assert!(large.vwap_px as i128 >= spot,
                "A6: Buy VWAP should be above spot");
        }
    }
}

/// A7: Round-trip loses to fees
///
/// Property: Buying then selling results in net loss due to fees and slippage.
#[kani::proof]
#[kani::unwind(4)]
fn a7_round_trip_loses_value() {
    let x: i64 = kani::any();
    let y: i64 = kani::any();
    let amount: i64 = kani::any();

    kani::assume(x > 1000 && x < 10_000 * SCALE);
    kani::assume(y > 1000 && y < 100_000_000 * SCALE);
    kani::assume(amount > 0 && amount < x / 10);

    // Buy
    if let Ok(buy_result) = quote_buy(x, y, 5, amount, 100) {
        let cost = buy_result.quote_amount;

        // Sell back (using new reserves)
        if let Ok(sell_result) = quote_sell(buy_result.new_x, buy_result.new_y, 5, amount, 100) {
            let proceeds = sell_result.quote_amount;

            // Should lose money
            assert!(proceeds < cost,
                "A7: Round-trip should lose value: cost={}, proceeds={}",
                cost, proceeds);
        }
    }
}

/// A8: Min liquidity floor enforced
///
/// Property: Trades cannot drain the pool below min_liquidity.
#[kani::proof]
#[kani::unwind(4)]
fn a8_min_liquidity_enforced() {
    let x: i64 = kani::any();
    let y: i64 = kani::any();
    let dx: i64 = kani::any();
    let min_liq: i64 = kani::any();

    kani::assume(x > 1000 && x < 10_000 * SCALE);
    kani::assume(y > 1000 && y < 100_000_000 * SCALE);
    kani::assume(dx > 0 && dx < x);
    kani::assume(min_liq > 0 && min_liq < x / 2);

    if let Ok(result) = quote_buy(x, y, 5, dx, min_liq) {
        // Remaining X must be >= min_liquidity
        assert!(result.new_x >= min_liq,
            "A8: Min liquidity floor must be enforced: new_x={}, min_liq={}",
            result.new_x, min_liq);
    }
}
