//! Kani proofs for PnL vesting Taylor series approximation
//!
//! These proofs verify the Taylor series implementation used for exponential vesting:
//! - **V1: Bounded Output** - Result is always in [0, FP_ONE]
//! - **V2: Monotonic in Time** - Output increases with dt
//! - **V3: Saturation** - Large dt approaches 1.0
//! - **V4: Zero at Origin** - f(0) = 0
//! - **V5: Withdrawable PnL Bounded** - Never exceeds vested amount

/// Fixed-point scale (1e9)
const FP_ONE: i128 = 1_000_000_000;

/// Taylor series approximation of 1 - e^(-dt/tau)
///
/// This is a simplified version extracted from router code for verification
fn one_minus_exp_neg(dt: u64, tau: u64) -> i128 {
    if tau == 0 {
        return FP_ONE;
    }

    // Saturate for large dt
    if dt >= 20 * tau {
        return FP_ONE;
    }

    // x = dt / tau (in fixed-point 1e9)
    let x = ((dt as i128) * FP_ONE) / (tau as i128);

    // Saturate for x >= 3
    if x >= 3 * FP_ONE {
        // For x >= 3: 1 - e^(-x) ≈ 0.95 (close to 1)
        return (FP_ONE * 95) / 100;
    }

    // Taylor series: x - x²/2 + x³/6
    // Rearranged: x * (1 - x/2 * (1 - x/3))
    let x_div_3 = x / 3;
    let term2 = FP_ONE - x_div_3;
    let x_div_2 = x / 2;
    let term1 = (x_div_2 * term2) / FP_ONE;
    let result = (x * (FP_ONE - term1)) / FP_ONE;

    result.min(FP_ONE).max(0)
}

/// Calculate withdrawable PnL with adaptive warmup throttling
///
/// Returns min(vested_pnl * unlocked_frac, vested_pnl) if positive, else 0
fn calculate_withdrawable_pnl(vested_pnl: i128, unlocked_frac: i128) -> i128 {
    if vested_pnl <= 0 {
        return 0;
    }

    // Q32.32 multiplication: (vested_pnl * unlocked_frac) >> 32
    let product = (vested_pnl as i128) * (unlocked_frac as i128);
    let withdrawable = product >> 32;

    // Cap at vested_pnl (in case unlocked_frac > 1.0)
    withdrawable.min(vested_pnl).max(0)
}

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    /// V1: Output is always in [0, FP_ONE]
    ///
    /// Property: 1 - e^(-x) ∈ [0, 1] for all x >= 0
    #[kani::proof]
    #[kani::unwind(4)]
    fn v1_bounded_output() {
        let dt: u64 = kani::any();
        let tau: u64 = kani::any();

        kani::assume(tau > 0);
        kani::assume(dt > 0);

        let result = one_minus_exp_neg(dt, tau);

        // Result must be in [0, 1] range
        assert!(result >= 0, "V1: Result must be non-negative");
        assert!(result <= FP_ONE, "V1: Result must not exceed 1.0");
    }

    /// V2: Monotonic in time - longer dt gives larger result
    ///
    /// Property: dt1 < dt2 implies f(dt1) <= f(dt2)
    #[kani::proof]
    #[kani::unwind(4)]
    fn v2_monotonic_in_time() {
        let dt1: u64 = kani::any();
        let dt2: u64 = kani::any();
        let tau: u64 = kani::any();

        kani::assume(tau > 100 && tau < 1_000_000);
        kani::assume(dt1 > 0 && dt1 < 1_000_000);
        kani::assume(dt2 > dt1 && dt2 < 1_000_000); // dt2 > dt1

        let result1 = one_minus_exp_neg(dt1, tau);
        let result2 = one_minus_exp_neg(dt2, tau);

        // More time vested should give >= amount
        assert!(result2 >= result1, "V2: Vesting should be monotonic in time");
    }

    /// V3: Saturation for large dt
    ///
    /// Property: For dt >> tau, result approaches 1.0
    #[kani::proof]
    #[kani::unwind(4)]
    fn v3_saturation() {
        let tau: u64 = kani::any();

        kani::assume(tau > 1000 && tau < 100_000);

        // Test dt = 20*tau (saturation threshold)
        let dt = 20 * tau;
        let result = one_minus_exp_neg(dt, tau);

        // Should return exactly FP_ONE at saturation
        assert!(result == FP_ONE, "V3: Should saturate to 1.0 for large dt");
    }

    /// V4: Zero at origin
    ///
    /// Property: f(0) = 0 (no vesting at t=0)
    #[kani::proof]
    #[kani::unwind(4)]
    fn v4_zero_at_origin() {
        let tau: u64 = kani::any();

        kani::assume(tau > 0 && tau < 1_000_000);

        let result = one_minus_exp_neg(0, tau);

        // At dt=0, no vesting should occur
        assert!(result == 0, "V4: Should be zero at t=0");
    }

    /// V5: Withdrawable PnL never exceeds vested amount
    ///
    /// Property: For any unlocked_frac, withdrawable <= vested_pnl
    #[kani::proof]
    #[kani::unwind(4)]
    fn v5_withdrawable_bounded() {
        let vested_pnl: i128 = kani::any();
        let unlocked_frac: i128 = kani::any();

        kani::assume(vested_pnl > 0 && vested_pnl < i128::MAX / 2);
        kani::assume(unlocked_frac >= 0 && unlocked_frac <= (1i128 << 32)); // Q32.32 [0, 1]

        let withdrawable = calculate_withdrawable_pnl(vested_pnl, unlocked_frac);

        // Withdrawable should never exceed vested
        assert!(withdrawable >= 0, "V5: Withdrawable must be non-negative");
        assert!(withdrawable <= vested_pnl, "V5: Withdrawable cannot exceed vested");
    }

    /// V6: Negative PnL yields zero withdrawable
    ///
    /// Property: Losses are not withdrawable
    #[kani::proof]
    #[kani::unwind(4)]
    fn v6_negative_pnl_zero() {
        let vested_pnl: i128 = kani::any();
        let unlocked_frac: i128 = kani::any();

        kani::assume(vested_pnl < 0);
        kani::assume(unlocked_frac >= 0 && unlocked_frac <= (1i128 << 32));

        let withdrawable = calculate_withdrawable_pnl(vested_pnl, unlocked_frac);

        // Negative PnL should never be withdrawable
        assert!(withdrawable == 0, "V6: Negative PnL should yield zero withdrawable");
    }

    /// V7: Full unlock allows full withdrawal
    ///
    /// Property: unlocked_frac = 1.0 => withdrawable = vested_pnl
    #[kani::proof]
    #[kani::unwind(4)]
    fn v7_full_unlock() {
        let vested_pnl: i128 = kani::any();

        kani::assume(vested_pnl > 0 && vested_pnl < i128::MAX / 2);

        let full_unlock = 1i128 << 32; // Q32.32 representation of 1.0
        let withdrawable = calculate_withdrawable_pnl(vested_pnl, full_unlock);

        // Full unlock should allow full vested withdrawal
        assert!(withdrawable == vested_pnl, "V7: Full unlock should give full vested amount");
    }

    /// V8: Zero unlock allows zero withdrawal
    ///
    /// Property: unlocked_frac = 0 => withdrawable = 0
    #[kani::proof]
    #[kani::unwind(4)]
    fn v8_zero_unlock() {
        let vested_pnl: i128 = kani::any();

        kani::assume(vested_pnl > 0 && vested_pnl < i128::MAX / 2);

        let withdrawable = calculate_withdrawable_pnl(vested_pnl, 0);

        // Zero unlock should prevent all withdrawal
        assert!(withdrawable == 0, "V8: Zero unlock should give zero withdrawable");
    }

    /// V9: Taylor approximation bounded error
    ///
    /// Property: For small x, error is bounded (compared to exact exponential)
    #[kani::proof]
    #[kani::unwind(4)]
    fn v9_approximation_reasonable() {
        let dt: u64 = kani::any();
        let tau: u64 = kani::any();

        kani::assume(tau > 1000 && tau < 100_000);
        kani::assume(dt > 0 && dt < tau); // Small x = dt/tau < 1

        let result = one_minus_exp_neg(dt, tau);

        // For dt < tau, result should be less than dt/tau (since e^(-x) > 1 - x)
        let linear_approx = ((dt as i128) * FP_ONE) / (tau as i128);

        // 1 - e^(-x) < x for x > 0
        // So our result should be <= linear approximation
        assert!(result <= linear_approx, "V9: Taylor series should be bounded by linear term");
    }

    /// V10: Monotonic in unlock fraction
    ///
    /// Property: Higher unlock fraction gives more withdrawable
    #[kani::proof]
    #[kani::unwind(4)]
    fn v10_monotonic_in_unlock() {
        let vested_pnl: i128 = kani::any();
        let frac1: i128 = kani::any();
        let frac2: i128 = kani::any();

        kani::assume(vested_pnl > 10000 && vested_pnl < i128::MAX / 4);
        kani::assume(frac1 > 0 && frac1 < (1i128 << 32));
        kani::assume(frac2 > frac1 && frac2 < (1i128 << 32)); // frac2 > frac1

        let w1 = calculate_withdrawable_pnl(vested_pnl, frac1);
        let w2 = calculate_withdrawable_pnl(vested_pnl, frac2);

        // Higher fraction should allow more withdrawal
        assert!(w2 >= w1, "V10: Withdrawable should be monotonic in unlock fraction");
    }
}
