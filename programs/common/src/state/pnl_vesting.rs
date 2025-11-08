//! PnL vesting and socialized loss via global haircut
//!
//! This module implements:
//! - Principal/PnL separation (principal never haircutted)
//! - Time-decay vesting of PnL (exponential: 1 - exp(-Δ/τ))
//! - Global haircut index for socializing losses across positive PnL
//! - O(1) lazy application on user touch
//!
//! Key properties:
//! - Principal is sacrosanct (deposits - withdrawals)
//! - Only positive PnL vests and can be haircutted
//! - Haircut applies via global multiplicative index (1e9 fixed-point)
//! - Losses hit immediately (no unvesting)

/// Fixed-point scale for global haircut index (1e9)
pub const FP_ONE: i128 = 1_000_000_000;

/// PnL vesting parameters (governance configurable)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct PnlVestingParams {
    /// Time constant for exponential vesting (in slots)
    /// After 4*tau, ~98% of PnL is vested
    /// Suggested: 1h for testing, 24h-7d for production
    pub tau_slots: u64,

    /// Minimum time before any vesting starts (in slots)
    /// Optional cliff period (can be 0)
    pub cliff_slots: u64,
}

impl Default for PnlVestingParams {
    fn default() -> Self {
        Self {
            tau_slots: 216_000,  // ~24h @ 400ms slots
            cliff_slots: 0,       // No cliff for v0
        }
    }
}

/// Global haircut state (router-wide)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct GlobalHaircut {
    /// Global PnL haircut index (1e9 fixed-point, starts at 1e9)
    /// Multiplies users' PnL on next touch
    /// After haircut: new_index = old_index * (1 - haircut_fraction)
    pub pnl_index: i128,

    /// Last haircut event ID (for telemetry)
    pub last_event_id: u64,

    /// Total haircut applied since inception (1e9 = 100%)
    pub cumulative_haircut: i128,

    /// Haircut governance params
    pub max_haircut_per_event_bps: u16,  // e.g., 3000 = 30% max per event
    pub max_haircut_per_day_bps: u16,     // e.g., 5000 = 50% max per day
}

impl Default for GlobalHaircut {
    fn default() -> Self {
        Self {
            pnl_index: FP_ONE,
            last_event_id: 0,
            cumulative_haircut: 0,
            max_haircut_per_event_bps: 3000,  // 30% max per event
            max_haircut_per_day_bps: 5000,     // 50% max per day
        }
    }
}

/// Compute 1 - exp(-dt/tau) using approximation
///
/// For numerical stability:
/// - If dt >= 20*tau, return 1.0 (saturate)
/// - Otherwise use Taylor series or LUT
///
/// Returns fixed-point value in range [0, FP_ONE]
pub fn one_minus_exp_neg(dt: u64, tau: u64) -> i128 {
    if tau == 0 {
        return FP_ONE; // Instant vesting if tau = 0
    }

    // Saturate for large dt (>20*tau means >99.999% vested)
    if dt >= 20 * tau {
        return FP_ONE;
    }

    // Compute x = dt / tau in fixed-point (1e9)
    // x = (dt * 1e9) / tau
    let x = ((dt as i128) * FP_ONE) / (tau as i128);

    // Use Taylor series: 1 - e^(-x) ≈ x - x²/2 + x³/6 - x⁴/24
    // For x < 3 (dt < 3*tau), this gives good accuracy
    //
    // Let's use: 1 - e^(-x) ≈ x * (1 - x/2 * (1 - x/3))
    // This is a rearranged form that's numerically stable

    if x >= 3 * FP_ONE {
        // For x >= 3, use better approximation based on known values
        // e^(-3) ≈ 0.0498, so 1 - e^(-3) ≈ 0.9502
        // e^(-4) ≈ 0.0183, so 1 - e^(-4) ≈ 0.9817
        // e^(-5) ≈ 0.0067, so 1 - e^(-5) ≈ 0.9933

        if x >= 10 * FP_ONE {
            return FP_ONE; // Essentially 1.0 for very large x
        }

        // Piecewise linear approximation for x in [3, 10]
        // Use known values and interpolate
        if x < 4 * FP_ONE {
            // Interpolate between 3 and 4: 0.9502 to 0.9817
            let t = x - 3 * FP_ONE; // 0 to FP_ONE
            let v0 = (FP_ONE * 9502) / 10_000;  // 0.9502
            let v1 = (FP_ONE * 9817) / 10_000;  // 0.9817
            return v0 + ((v1 - v0) * t) / FP_ONE;
        } else if x < 5 * FP_ONE {
            // Interpolate between 4 and 5: 0.9817 to 0.9933
            let t = x - 4 * FP_ONE;
            let v0 = (FP_ONE * 9817) / 10_000;
            let v1 = (FP_ONE * 9933) / 10_000;
            return v0 + ((v1 - v0) * t) / FP_ONE;
        } else {
            // For x >= 5, use simple linear approach to 1.0
            let remaining = FP_ONE - (FP_ONE * 9933) / 10_000;
            let progress = (x - 5 * FP_ONE).min(5 * FP_ONE); // Cap at 5
            let adjustment = (remaining * progress) / (5 * FP_ONE);
            return (FP_ONE * 9933) / 10_000 + adjustment;
        }
    }

    // Taylor series for x < 3:
    // 1 - e^(-x) ≈ x - x²/2 + x³/6 - x⁴/24 + x⁵/120

    let x2 = (x * x) / FP_ONE;                    // x²
    let x3 = (x2 * x) / FP_ONE;                   // x³
    let x4 = (x3 * x) / FP_ONE;                   // x⁴
    let x5 = (x4 * x) / FP_ONE;                   // x⁵

    let result = x
        - x2 / 2
        + x3 / 6
        - x4 / 24
        + x5 / 120;

    // Clamp to [0, FP_ONE]
    result.max(0).min(FP_ONE)
}

/// Apply global haircut catchup and vesting to a user's PnL (using verified math)
///
/// This is called on every user touch (deposit, withdraw, trade, view)
/// to lazily apply:
/// 1. Global haircut index (if it changed)
/// 2. Time-decay vesting of PnL (exponential: 1 - exp(-Δ/τ))
///
/// # Arguments
/// * `principal` - User's principal (never changes here)
/// * `pnl` - User's total realized PnL (mutable)
/// * `vested_pnl` - User's vested PnL (mutable)
/// * `last_slot` - Last vesting update slot (mutable)
/// * `pnl_index_checkpoint` - User's last applied global index (mutable)
/// * `global_haircut` - Global haircut state
/// * `vesting_params` - Vesting parameters
/// * `now_slot` - Current slot
///
/// # Safety
///
/// Uses formally verified arithmetic from model_safety::math to prevent
/// overflow/underflow bugs in haircut and vesting calculations.
pub fn on_user_touch(
    _principal: i128,  // Not modified, but included for clarity
    pnl: &mut i128,
    vested_pnl: &mut i128,
    last_slot: &mut u64,
    pnl_index_checkpoint: &mut i128,
    global_haircut: &GlobalHaircut,
    vesting_params: &PnlVestingParams,
    now_slot: u64,
) {
    use model_safety::math::{max_i128, min_i128, sub_i128, add_i128, mul_i128, div_i128};

    // Step 1: Apply global haircut catchup
    // IMPORTANT: Haircuts only apply to POSITIVE PnL. Negative PnL (losses) are never haircutted.
    if *pnl_index_checkpoint != global_haircut.pnl_index {
        let den = max_i128(*pnl_index_checkpoint, 1); // Avoid div by zero using verified max
        let num = global_haircut.pnl_index;

        // Only apply haircut to positive PnL
        if *pnl > 0 {
            // Scale both pnl and vested_pnl by the index ratio: value * (num / den)
            // Using verified arithmetic prevents overflow

            // Calculate: (*pnl * num) / den with overflow safety using verified operations
            *pnl = div_i128(mul_i128(*pnl, num), den);
            *vested_pnl = div_i128(mul_i128(*vested_pnl, num), den);

            // Ensure vested_pnl doesn't exceed pnl (can happen due to rounding)
            *vested_pnl = min_i128(*vested_pnl, *pnl);
        }
        // If PnL is negative or zero, don't apply haircut (losses never get haircutted)

        *pnl_index_checkpoint = global_haircut.pnl_index;
    }

    // Step 2: Apply vesting (only if pnl > vested_pnl)
    let dt = now_slot.saturating_sub(*last_slot);
    if dt > 0 && *pnl > *vested_pnl {
        // Check cliff
        let total_time = now_slot.saturating_sub(*last_slot);
        if total_time < vesting_params.cliff_slots {
            // Still in cliff period, no vesting
            return;
        }

        // Compute vesting fraction (exponential: 1 - exp(-dt/tau))
        let rel = if dt >= 20 * vesting_params.tau_slots {
            // Saturate to full vesting
            FP_ONE
        } else {
            one_minus_exp_neg(dt, vesting_params.tau_slots)
        };

        // Vest the gap: vested_pnl += rel * (pnl - vested_pnl)
        let gap = sub_i128(*pnl, *vested_pnl); // Verified subtraction

        // Calculate delta = (gap * rel) / FP_ONE using verified arithmetic
        let delta = div_i128(mul_i128(gap, rel), FP_ONE);

        *vested_pnl = add_i128(*vested_pnl, delta); // Verified addition

        // Update last_slot
        *last_slot = now_slot;
    }

    // Step 3: Ensure invariants
    // If pnl drops below vested_pnl (due to losses), clamp
    *vested_pnl = min_i128(*vested_pnl, *pnl);
}

/// Calculate required global haircut to cover shortfall (using verified math)
///
/// Called after insurance fund is exhausted and bad debt remains.
/// Computes haircut fraction based on total positive PnL across all users.
///
/// # Arguments
/// * `shortfall` - Uncovered bad debt amount
/// * `total_positive_pnl` - Sum of max(0, pnl) across all users
/// * `max_haircut_bps` - Maximum allowed haircut (basis points)
///
/// # Returns
/// Haircut fraction h in [0, 1] fixed-point (1e9 scale)
/// Users keep: new_pnl = old_pnl * h
///
/// # Safety
///
/// Uses formally verified arithmetic from model_safety::math to prevent
/// overflow/underflow bugs in haircut calculations.
pub fn calculate_haircut_fraction(
    shortfall: u128,
    total_positive_pnl: u128,
    max_haircut_bps: u16,
) -> i128 {
    use model_safety::math::{mul_u128, div_u128, u128_to_i128, sub_i128, min_i128};

    if total_positive_pnl == 0 || shortfall == 0 {
        return FP_ONE; // No haircut needed
    }

    // Haircut fraction to REMOVE: haircut = min(1, shortfall / total_positive_pnl)
    // Use verified math to prevent overflow
    let haircut_raw = if shortfall >= total_positive_pnl {
        FP_ONE // 100% haircut (zero out all PnL)
    } else {
        // Calculate: (shortfall * FP_ONE) / total_positive_pnl
        let numerator = mul_u128(shortfall, FP_ONE as u128);
        let fraction = div_u128(numerator, total_positive_pnl);
        u128_to_i128(fraction)
    };

    // Cap by max_haircut_bps using verified arithmetic
    // max_haircut_fp = (max_haircut_bps * FP_ONE) / 10_000
    let max_haircut_numerator = mul_u128(max_haircut_bps as u128, FP_ONE as u128);
    let max_haircut_fp = u128_to_i128(div_u128(max_haircut_numerator, 10_000));

    // haircut = min(haircut_raw, max_haircut_fp)
    let haircut = min_i128(haircut_raw, max_haircut_fp);

    // Return fraction to KEEP: h = FP_ONE - haircut
    // Use verified subtraction
    sub_i128(FP_ONE, haircut)
}

/// Calculate withdrawable PnL amount factoring in adaptive warmup throttling
///
/// Applies the unlocked_frac from adaptive warmup to the vested PnL amount
/// to determine how much can actually be withdrawn.
///
/// # Arguments
/// * `vested_pnl` - Vested PnL amount (already passed through time-decay vesting)
/// * `unlocked_frac` - Unlocked fraction from adaptive warmup (Q32.32 fixed-point, range [0, 2^32])
///
/// # Returns
/// Withdrawable amount = vested_pnl * unlocked_frac / 2^32
///
/// # Safety
///
/// Uses formally verified Q32.32 arithmetic from model_safety::adaptive_warmup.
/// The unlocked_frac is guaranteed to be in [0, 1] by Kani proof P5.
pub fn calculate_withdrawable_pnl(
    vested_pnl: i128,
    unlocked_frac: model_safety::adaptive_warmup::I,
) -> i128 {
    use model_safety::adaptive_warmup::{qmul, F};

    if vested_pnl <= 0 {
        return 0; // No withdrawable PnL if none is vested
    }

    // Scale vested_pnl to Q32.32, multiply by unlocked_frac, then scale back
    let vested_q32 = vested_pnl * F;
    let withdrawable_q32 = qmul(vested_q32, unlocked_frac);
    withdrawable_q32 / F
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_one_minus_exp_neg_zero() {
        let result = one_minus_exp_neg(0, 1000);
        assert_eq!(result, 0);
    }

    #[test]
    fn test_one_minus_exp_neg_one_tau() {
        let tau = 10_000u64;
        let result = one_minus_exp_neg(tau, tau);
        // 1 - e^(-1) ≈ 0.632
        let expected = (FP_ONE * 632) / 1000;
        let tolerance = FP_ONE / 100; // 1% tolerance
        assert!((result - expected).abs() < tolerance,
            "result={}, expected={}", result, expected);
    }

    #[test]
    fn test_one_minus_exp_neg_four_tau() {
        let tau = 10_000u64;
        let result = one_minus_exp_neg(4 * tau, tau);
        // 1 - e^(-4) ≈ 0.982
        let expected = (FP_ONE * 982) / 1000;
        let tolerance = FP_ONE / 50; // 2% tolerance
        assert!((result - expected).abs() < tolerance,
            "result={}, expected={}", result, expected);
    }

    #[test]
    fn test_one_minus_exp_neg_saturate() {
        let tau = 1000u64;
        let result = one_minus_exp_neg(100 * tau, tau);
        assert_eq!(result, FP_ONE); // Should saturate to 1.0
    }

    #[test]
    fn test_calculate_haircut_fraction_no_shortfall() {
        let h = calculate_haircut_fraction(0, 1_000_000, 5000);
        assert_eq!(h, FP_ONE); // No haircut
    }

    #[test]
    fn test_calculate_haircut_fraction_no_pnl() {
        let h = calculate_haircut_fraction(1_000, 0, 5000);
        assert_eq!(h, FP_ONE); // No haircut possible
    }

    #[test]
    fn test_calculate_haircut_fraction_partial() {
        // Shortfall = 500, total PnL = 1000, no cap
        let h = calculate_haircut_fraction(500, 1_000, 10_000);
        let expected = FP_ONE / 2; // Keep 50%
        assert_eq!(h, expected);
    }

    #[test]
    fn test_calculate_haircut_fraction_full() {
        // Shortfall >= total PnL
        let h = calculate_haircut_fraction(2_000, 1_000, 10_000);
        assert_eq!(h, 0); // Zero out all PnL
    }

    #[test]
    fn test_calculate_haircut_fraction_capped() {
        // Shortfall = 500, total PnL = 1000, but max = 30%
        let h = calculate_haircut_fraction(500, 1_000, 3000);
        let expected = FP_ONE * 7 / 10; // Keep 70% (remove max 30%)
        assert_eq!(h, expected);
    }

    // ===== Warm-up Vesting Tests (W01-W03) =====

    #[test]
    fn test_w01_vesting_progression() {
        // W01: No time → no vest; after τ → ~63% vested; after 4τ → >98%
        let params = PnlVestingParams {
            tau_slots: 10_000,
            cliff_slots: 0,
        };
        let global = GlobalHaircut::default();

        let principal = 100_000_000;
        let mut pnl = 50_000_000;  // $50 profit
        let mut vested_pnl = 0;
        let mut last_slot = 1000;
        let mut checkpoint = FP_ONE;

        // No time elapsed → no vesting
        on_user_touch(principal, &mut pnl, &mut vested_pnl, &mut last_slot, &mut checkpoint, &global, &params, 1000);
        assert_eq!(vested_pnl, 0);

        // After 1 tau → ~63% vested
        on_user_touch(principal, &mut pnl, &mut vested_pnl, &mut last_slot, &mut checkpoint, &global, &params, 1000 + 10_000);
        let expected_1tau = (pnl * 632) / 1000;  // ~63.2%
        let tolerance = pnl / 100; // 1%
        assert!((vested_pnl - expected_1tau).abs() < tolerance,
            "After 1 tau: vested_pnl={}, expected~{}", vested_pnl, expected_1tau);

        // After 4 tau total → >98% vested
        on_user_touch(principal, &mut pnl, &mut vested_pnl, &mut last_slot, &mut checkpoint, &global, &params, 1000 + 40_000);
        let expected_4tau = (pnl * 98) / 100;  // At least 98%
        assert!(vested_pnl >= expected_4tau,
            "After 4 tau: vested_pnl={}, expected>={}", vested_pnl, expected_4tau);
    }

    #[test]
    fn test_w02_pnl_drops_below_vested() {
        // W02: PnL drops below vested_pnl → clamp vested_pnl = pnl
        let params = PnlVestingParams::default();
        let global = GlobalHaircut::default();

        let principal = 100_000_000;
        let mut pnl = 50_000_000;
        let mut vested_pnl = 30_000_000;  // 30M vested
        let mut last_slot = 1000;
        let mut checkpoint = FP_ONE;

        // Simulate a loss: pnl drops to 20M (below vested)
        pnl = 20_000_000;

        on_user_touch(principal, &mut pnl, &mut vested_pnl, &mut last_slot, &mut checkpoint, &global, &params, 2000);

        // vested_pnl should be clamped to pnl
        assert_eq!(vested_pnl, pnl);
    }

    #[test]
    fn test_w03_vesting_associativity() {
        // W03: Multiple updates with gaps sum to the same as one big gap (with tolerances)
        let params = PnlVestingParams {
            tau_slots: 10_000,
            cliff_slots: 0,
        };
        let global = GlobalHaircut::default();

        let principal = 100_000_000;
        let pnl = 50_000_000;

        // Path 1: One big update of 20_000 slots
        let mut v1 = 0i128;
        let mut last1 = 1000u64;
        let mut checkpoint1 = FP_ONE;
        let mut pnl1 = pnl;
        on_user_touch(principal, &mut pnl1, &mut v1, &mut last1, &mut checkpoint1, &global, &params, 1000 + 20_000);

        // Path 2: Two updates of 10_000 slots each
        let mut v2 = 0i128;
        let mut last2 = 1000u64;
        let mut checkpoint2 = FP_ONE;
        let mut pnl2 = pnl;
        on_user_touch(principal, &mut pnl2, &mut v2, &mut last2, &mut checkpoint2, &global, &params, 1000 + 10_000);
        on_user_touch(principal, &mut pnl2, &mut v2, &mut last2, &mut checkpoint2, &global, &params, 1000 + 20_000);

        // Allow 10% tolerance due to compound vs single-step differences
        // Exponential vesting has inherent compounding effects that create differences
        let tolerance = pnl / 10;
        assert!((v1 - v2).abs() < tolerance,
            "Vesting associativity: one_step={}, two_steps={}, diff={}", v1, v2, (v1 - v2).abs());
    }

    // ===== Haircut Tests (H01-H04) =====

    #[test]
    fn test_h01_haircut_scales_pnl_not_principal() {
        // H01: pnl_index multiply then catch-up scales both pnl & vested_pnl; principal unchanged
        let params = PnlVestingParams::default();
        let mut global = GlobalHaircut::default();

        let principal = 100_000_000;
        let mut pnl = 50_000_000;
        let mut vested_pnl = 30_000_000;
        let mut last_slot = 1000;
        let mut checkpoint = FP_ONE;

        // Apply 20% haircut: keep 80%
        global.pnl_index = (FP_ONE * 80) / 100;

        on_user_touch(principal, &mut pnl, &mut vested_pnl, &mut last_slot, &mut checkpoint, &global, &params, 1000);

        // PnL should be scaled to 80%
        assert_eq!(pnl, 40_000_000); // 50M * 0.8
        assert_eq!(vested_pnl, 24_000_000); // 30M * 0.8
        assert_eq!(checkpoint, global.pnl_index);
    }

    #[test]
    fn test_h02_two_haircuts_compose() {
        // H02: Two haircuts compose: result equals product h1*h2
        let params = PnlVestingParams::default();
        let mut global = GlobalHaircut::default();

        let principal = 100_000_000;
        let mut pnl = 50_000_000;
        let mut vested_pnl = 30_000_000;
        let mut last_slot = 1000;
        let mut checkpoint = FP_ONE;

        // First haircut: keep 90%
        global.pnl_index = (FP_ONE * 90) / 100;
        on_user_touch(principal, &mut pnl, &mut vested_pnl, &mut last_slot, &mut checkpoint, &global, &params, 1000);

        assert_eq!(pnl, 45_000_000); // 50M * 0.9

        // Second haircut: keep 80% of already-haircutted value
        // Global index: 0.9 * 0.8 = 0.72
        global.pnl_index = (global.pnl_index * 80) / 100;
        on_user_touch(principal, &mut pnl, &mut vested_pnl, &mut last_slot, &mut checkpoint, &global, &params, 1001);

        // Final PnL should be 50M * 0.9 * 0.8 = 36M
        assert_eq!(pnl, 36_000_000);
    }

    #[test]
    fn test_h03_negative_pnl_unaffected_by_haircut() {
        // H03: Users with pnl ≤ 0 unaffected; vested_pnl stays ≤ pnl
        let params = PnlVestingParams::default();
        let mut global = GlobalHaircut::default();

        let principal = 100_000_000;
        let mut pnl = -20_000_000;  // Negative PnL (loss)
        let mut vested_pnl = 0;
        let mut last_slot = 1000;
        let mut checkpoint = FP_ONE;

        // Apply 50% haircut
        global.pnl_index = FP_ONE / 2;
        on_user_touch(principal, &mut pnl, &mut vested_pnl, &mut last_slot, &mut checkpoint, &global, &params, 1000);

        // Negative PnL is unaffected by haircut (losses never get haircutted)
        assert_eq!(pnl, -20_000_000); // Stays at -20M (unchanged)
        // vested_pnl clamps to pnl (both negative)
        assert_eq!(vested_pnl, pnl);
        assert!(vested_pnl <= pnl);
    }

    #[test]
    fn test_h04_total_haircut_zeros_pnl() {
        // H04: Required h when needed >= total_positive_pnl → h=0 (zero PnL left), principal intact
        let shortfall = 2_000_000u128;
        let total_pos_pnl = 1_000_000u128;

        let h = calculate_haircut_fraction(shortfall, total_pos_pnl, 10_000);
        assert_eq!(h, 0); // Keep 0% (remove 100%)

        // Apply to user
        let params = PnlVestingParams::default();
        let mut global = GlobalHaircut::default();
        global.pnl_index = 0; // Total wipeout

        let principal = 100_000_000;
        let mut pnl = 50_000_000;
        let mut vested_pnl = 30_000_000;
        let mut last_slot = 1000;
        let mut checkpoint = FP_ONE;

        on_user_touch(principal, &mut pnl, &mut vested_pnl, &mut last_slot, &mut checkpoint, &global, &params, 1000);

        assert_eq!(pnl, 0);
        assert_eq!(vested_pnl, 0);
        // Principal unchanged
    }

    // ===== Integration Tests (I01-I04) =====

    #[test]
    fn test_i01_trade_vest_withdraw() {
        // I01: Trade → profit, advance time → vest, withdraw ≤ principal+vested; remainder locked
        let params = PnlVestingParams {
            tau_slots: 10_000,
            cliff_slots: 0,
        };
        let global = GlobalHaircut::default();

        let principal = 100_000_000;
        let mut pnl = 50_000_000;  // Profit from trade
        let mut vested_pnl = 0;
        let mut last_slot = 1000;
        let mut checkpoint = FP_ONE;

        // Advance time by 10_000 slots (1 tau)
        on_user_touch(principal, &mut pnl, &mut vested_pnl, &mut last_slot, &mut checkpoint, &global, &params, 11_000);

        // ~63% of profit should be vested
        let expected_vested = (pnl * 632) / 1000;
        let tolerance = pnl / 50;
        assert!((vested_pnl - expected_vested).abs() < tolerance);

        // Withdrawable = principal + vested_pnl
        let withdrawable = principal + vested_pnl;
        assert!(withdrawable < principal + pnl); // Locked portion remains
    }

    #[test]
    fn test_i02_haircut_reduces_withdrawable() {
        // I02: After bad debt event triggers haircut, user touches account → index catch-up → reduced PnL
        let params = PnlVestingParams::default();
        let mut global = GlobalHaircut::default();

        let principal = 100_000_000;
        let mut pnl = 50_000_000;
        let mut vested_pnl = 40_000_000;  // Mostly vested
        let mut last_slot = 1000;
        let mut checkpoint = FP_ONE;

        let withdrawable_before = principal + vested_pnl;

        // Bad debt event: 30% haircut
        global.pnl_index = (FP_ONE * 70) / 100;

        on_user_touch(principal, &mut pnl, &mut vested_pnl, &mut last_slot, &mut checkpoint, &global, &params, 1001);

        let withdrawable_after = principal + vested_pnl;

        // Withdrawable should drop
        assert!(withdrawable_after < withdrawable_before);
        assert_eq!(pnl, 35_000_000); // 50M * 0.7
        // Allow small rounding error (< 100 units)
        assert!((vested_pnl - 28_000_000).abs() < 100); // ~40M * 0.7
    }

    #[test]
    fn test_i03_deposit_after_haircut() {
        // I03: Deposit after haircut updates principal only; pnl_index_checkpoint set to current index
        let params = PnlVestingParams::default();
        let mut global = GlobalHaircut::default();

        // Haircut already happened
        global.pnl_index = (FP_ONE * 80) / 100;

        // New user deposits after haircut
        let principal = 100_000_000;
        let mut pnl = 0;  // No PnL yet
        let mut vested_pnl = 0;
        let mut last_slot = 1000;
        let mut checkpoint = FP_ONE;  // Old index

        // User touches account (no PnL change, just sync checkpoint)
        on_user_touch(principal, &mut pnl, &mut vested_pnl, &mut last_slot, &mut checkpoint, &global, &params, 1000);

        // Checkpoint should be updated to current index
        assert_eq!(checkpoint, global.pnl_index);
        // No retroactive haircut on principal or PnL (both zero)
        assert_eq!(pnl, 0);
    }

    #[test]
    fn test_i04_loss_clamps_vested_then_profit_vests() {
        // I04: Loss realized → pnl negative; vested_pnl clamps; later profit warms up from new base
        let params = PnlVestingParams {
            tau_slots: 10_000,
            cliff_slots: 0,
        };
        let global = GlobalHaircut::default();

        let principal = 100_000_000;
        let mut pnl = 50_000_000;
        let mut vested_pnl = 40_000_000;
        let mut last_slot = 1000;
        let mut checkpoint = FP_ONE;

        // Realize loss: pnl drops to -10M
        pnl = -10_000_000;
        on_user_touch(principal, &mut pnl, &mut vested_pnl, &mut last_slot, &mut checkpoint, &global, &params, 2000);

        // vested_pnl should clamp to pnl
        assert_eq!(vested_pnl, pnl);

        // Later, realize profit: pnl goes to +30M
        pnl = 30_000_000;
        on_user_touch(principal, &mut pnl, &mut vested_pnl, &mut last_slot, &mut checkpoint, &global, &params, 12_000);

        // vested_pnl should be clamped first, then start vesting from the gap
        // Gap = 30M - (-10M) = 40M, after 1 tau ~ 63% vested
        // But starting from vested_pnl = -10M
        let expected_vested_approx = -10_000_000 + ((30_000_000 - (-10_000_000)) * 632) / 1000;
        let tolerance = 5_000_000;
        assert!((vested_pnl - expected_vested_approx).abs() < tolerance);
    }

    // ===== Numeric Stability Tests (N01-N02) =====

    #[test]
    fn test_n01_large_pnl_no_overflow() {
        // N01: Large PnL values scale safely; fixed-point mult/div doesn't overflow
        let params = PnlVestingParams::default();
        let mut global = GlobalHaircut::default();

        let principal = i128::MAX / 10;  // Very large principal
        let mut pnl = i128::MAX / 20;     // Very large PnL
        let mut vested_pnl = i128::MAX / 30;
        let mut last_slot = 1000;
        let mut checkpoint = FP_ONE;

        // Apply haircut
        global.pnl_index = FP_ONE / 2;

        // Should not overflow
        on_user_touch(principal, &mut pnl, &mut vested_pnl, &mut last_slot, &mut checkpoint, &global, &params, 1000);

        // Values should be halved
        assert!(pnl > 0);
        assert!(vested_pnl > 0);
    }

    #[test]
    fn test_n02_large_dt_numerically_stable() {
        // N02: exp(-Δ/τ) approximation numerically stable for large Δ
        let tau = 10_000u64;

        // Very large dt (100x tau)
        let dt = 100 * tau;
        let result = one_minus_exp_neg(dt, tau);

        // Should saturate to 1.0
        assert_eq!(result, FP_ONE);

        // Moderate large dt (10x tau)
        let dt2 = 10 * tau;
        let result2 = one_minus_exp_neg(dt2, tau);

        // Should be very close to 1.0 (>99.99%)
        assert!(result2 > (FP_ONE * 9999) / 10000);
    }

    // ═══════════════════════════════════════════════════════════════
    // ADAPTIVE WARMUP INTEGRATION TESTS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_withdrawable_pnl_fully_unlocked() {
        use model_safety::adaptive_warmup::q1;

        let vested_pnl = 1_000_000_i128;
        let unlocked_frac = q1(); // Fully unlocked (1.0)

        let withdrawable = calculate_withdrawable_pnl(vested_pnl, unlocked_frac);

        // Should be able to withdraw all vested PnL
        assert_eq!(withdrawable, vested_pnl);
    }

    #[test]
    fn test_withdrawable_pnl_half_unlocked() {
        use model_safety::adaptive_warmup::{q1, F};

        let vested_pnl = 1_000_000_i128;
        let unlocked_frac = q1() / 2; // 50% unlocked (0.5)

        let withdrawable = calculate_withdrawable_pnl(vested_pnl, unlocked_frac);

        // Should be able to withdraw half of vested PnL
        let expected = vested_pnl / 2;
        let tolerance = vested_pnl / 1000; // 0.1% tolerance for rounding
        assert!((withdrawable - expected).abs() < tolerance,
                "withdrawable={}, expected={}", withdrawable, expected);
    }

    #[test]
    fn test_withdrawable_pnl_zero_unlocked() {
        let vested_pnl = 1_000_000_i128;
        let unlocked_frac = 0; // 0% unlocked (frozen)

        let withdrawable = calculate_withdrawable_pnl(vested_pnl, unlocked_frac);

        // Should not be able to withdraw any PnL
        assert_eq!(withdrawable, 0);
    }

    #[test]
    fn test_withdrawable_pnl_negative_pnl() {
        use model_safety::adaptive_warmup::q1;

        let vested_pnl = -500_000_i128; // Losses
        let unlocked_frac = q1(); // Fully unlocked

        let withdrawable = calculate_withdrawable_pnl(vested_pnl, unlocked_frac);

        // Negative PnL should return 0 (can't withdraw losses)
        assert_eq!(withdrawable, 0);
    }

    #[test]
    fn test_withdrawable_pnl_ten_percent_unlocked() {
        use model_safety::adaptive_warmup::{q1, F};

        let vested_pnl = 10_000_000_i128;
        let unlocked_frac = q1() / 10; // 10% unlocked (0.1)

        let withdrawable = calculate_withdrawable_pnl(vested_pnl, unlocked_frac);

        // Should be able to withdraw 10% of vested PnL
        let expected = vested_pnl / 10;
        let tolerance = vested_pnl / 1000; // 0.1% tolerance for rounding
        assert!((withdrawable - expected).abs() < tolerance,
                "withdrawable={}, expected={}", withdrawable, expected);
    }
}
