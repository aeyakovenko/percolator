//! Crisis haircut logic - O(1) loss socialization
//!
//! This module implements the core crisis resolution algorithm that socializes
//! losses across all users by updating global scale factors. The algorithm runs
//! in O(1) time complexity, making it suitable for on-chain execution.
//!
//! ## Loss Waterfall
//!
//! Losses are absorbed in the following order:
//! 1. **Warming PnL**: Burn unvested PnL first (least "sacred")
//! 2. **Insurance Fund**: Draw from insurance reserves
//! 3. **Equity (Principal + Realized)**: Haircut user deposits and vested PnL
//!
//! ## Key Properties
//!
//! - **O(1) complexity**: No iteration over users
//! - **Atomic**: All updates happen in a single transaction
//! - **Monotonic scales**: Scales never increase, only decrease
//! - **Conservative**: Never burns more than available
//! - **Solvency**: If equity exists, deficit is eliminated

use crate::crisis::amount::Q64x64;
use crate::crisis::accums::Accums;

/// Outcome of a crisis haircut operation
///
/// Returned by `crisis_apply_haircuts()` to provide transparency about
/// what actions were taken during the crisis resolution.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct CrisisOutcome {
    /// Amount of warming PnL burned (numeraire units, e.g., USDC 1e6)
    pub burned_warming: i128,

    /// Amount drawn from insurance fund (numeraire units)
    pub insurance_draw: i128,

    /// Equity haircut ratio applied (in Q64.64 format)
    /// - 0.0 means no haircut
    /// - 0.1 means 10% haircut (users keep 90%)
    /// - 1.0 means 100% haircut (total wipeout, should never happen)
    pub equity_haircut_ratio: Q64x64,

    /// Whether the system is fully solvent after the crisis
    pub is_solvent: bool,
}

/// Apply crisis haircuts to resolve insolvency
///
/// This is the main entry point for crisis resolution. It performs the
/// following steps in O(1) time:
///
/// 1. Calculate system deficit (liabilities - assets)
/// 2. If deficit > 0, burn warming PnL proportionally via `warming_scale`
/// 3. If deficit remains, draw from insurance fund
/// 4. If deficit still remains, haircut equity via `equity_scale`
/// 5. Update global aggregates immediately
/// 6. Increment epoch (users reconcile lazily on next touch)
///
/// # Arguments
/// * `a` - Mutable reference to global Accums
///
/// # Returns
/// `CrisisOutcome` describing actions taken
///
/// # Guarantees
/// - If equity_total > 0, deficit will be 0 after this call
/// - Scales are monotone non-increasing
/// - Aggregates are updated atomically
/// - No overflow/underflow (uses saturating arithmetic)
///
/// # Example
/// ```
/// use model_safety::crisis::{Accums, crisis_apply_haircuts};
///
/// let mut a = Accums::new();
/// a.sigma_principal = 1_000_000;
/// a.sigma_collateral = 800_000; // Deficit of 200k
///
/// let outcome = crisis_apply_haircuts(&mut a);
/// assert_eq!(outcome.burned_warming, 0); // No warming to burn
/// assert_eq!(outcome.insurance_draw, 0); // No insurance
/// assert_eq!(a.deficit(), 0); // Deficit eliminated via equity haircut
/// ```
pub fn crisis_apply_haircuts(a: &mut Accums) -> CrisisOutcome {
    let initial_deficit = a.deficit();

    // If solvent, no action needed
    if initial_deficit == 0 {
        return CrisisOutcome {
            burned_warming: 0,
            insurance_draw: 0,
            equity_haircut_ratio: Q64x64::ZERO,
            is_solvent: true,
        };
    }

    let mut burned_warming = 0i128;
    let mut insurance_draw = 0i128;
    let mut rho = Q64x64::ZERO;

    // Step 1: Burn warming PnL proportionally via global scale
    if a.sigma_warming > 0 {
        let deficit_after_warming = a.deficit();
        if deficit_after_warming > 0 {
            let burn = core::cmp::min(deficit_after_warming, a.sigma_warming);

            // Calculate gamma = burn / sigma_warming (fraction to burn)
            let gamma = Q64x64::ratio(burn, a.sigma_warming);

            // New warming_scale = old * (1 - gamma)
            // This applies the burn proportionally to all users
            let new_scale = a.warming_scale.mul_scale(gamma.one_minus());
            a.warming_scale = new_scale;

            // Update aggregate immediately (authoritative)
            a.sigma_warming = a.sigma_warming.saturating_sub(burn);

            burned_warming = burn;
        }
    }

    // Step 2: Draw from insurance fund
    // Recalculate deficit after warming burn
    let deficit_after_insurance_check = a.deficit();
    if deficit_after_insurance_check > 0 && a.sigma_insurance > 0 {
        let draw = core::cmp::min(deficit_after_insurance_check, a.sigma_insurance);
        a.sigma_insurance = a.sigma_insurance.saturating_sub(draw);

        insurance_draw = draw;
    }

    // Step 3: Haircut equity (principal + realized) via global scale
    // Recalculate deficit after insurance draw
    let deficit_after_equity_check = a.deficit();
    if deficit_after_equity_check > 0 {
        let equity_total = a.sigma_principal.saturating_add(a.sigma_realized);

        // If equity_total == 0, we cannot cover deficit
        // System remains insolvent (should trigger emergency halt)
        if equity_total > 0 {
            // Calculate rho = deficit / equity_total (haircut fraction)
            // Cap at 1.0 to prevent over-burning
            rho = Q64x64::ratio(deficit_after_equity_check, equity_total);

            // New equity_scale = old * (1 - rho)
            let one_minus_rho = rho.one_minus();
            a.equity_scale = a.equity_scale.mul_scale(one_minus_rho);

            // Update aggregates immediately
            // Each aggregate is multiplied by (1 - rho)
            let new_principal = one_minus_rho.mul_i128(a.sigma_principal);
            let new_realized = one_minus_rho.mul_i128(a.sigma_realized);

            a.sigma_principal = new_principal;
            a.sigma_realized = new_realized;
        }
    }

    // Increment epoch to signal that scales have changed
    // Users will reconcile when they next touch the system
    a.epoch = a.epoch.wrapping_add(1);

    let final_deficit = a.deficit();

    CrisisOutcome {
        burned_warming,
        insurance_draw,
        equity_haircut_ratio: rho,
        is_solvent: final_deficit == 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_deficit_no_action() {
        let mut a = Accums::new();
        a.sigma_principal = 1_000_000;
        a.sigma_collateral = 1_500_000;

        let outcome = crisis_apply_haircuts(&mut a);

        assert_eq!(outcome.burned_warming, 0);
        assert_eq!(outcome.insurance_draw, 0);
        assert_eq!(outcome.equity_haircut_ratio, Q64x64::ZERO);
        assert!(outcome.is_solvent);
        assert_eq!(a.epoch, 0); // No epoch increment if no crisis
    }

    #[test]
    fn test_burn_warming_only() {
        let mut a = Accums::new();
        a.sigma_principal = 1_000_000;
        a.sigma_warming = 500_000;
        a.sigma_collateral = 1_200_000; // Deficit of 300k

        let initial_warming_scale = a.warming_scale;
        let outcome = crisis_apply_haircuts(&mut a);

        assert_eq!(outcome.burned_warming, 300_000);
        assert_eq!(outcome.insurance_draw, 0);
        assert_eq!(outcome.equity_haircut_ratio, Q64x64::ZERO);
        assert!(outcome.is_solvent);

        // Warming scale should have decreased
        assert!(a.warming_scale.0 < initial_warming_scale.0);

        // Aggregate should be reduced
        assert_eq!(a.sigma_warming, 200_000);

        // Epoch should increment
        assert_eq!(a.epoch, 1);
    }

    #[test]
    fn test_burn_all_warming_then_insurance() {
        let mut a = Accums::new();
        a.sigma_principal = 1_000_000;
        a.sigma_warming = 200_000;
        a.sigma_insurance = 500_000;
        a.sigma_collateral = 0;
        // Initial deficit: (1M principal + 200k warming) - (0 collateral + 500k insurance) = 700k
        // After burning warming: (1M principal + 0) - 500k insurance = 500k deficit
        // After insurance draw: 1M principal - 0 = 1M deficit
        // Must haircut 100% of equity to eliminate deficit

        let outcome = crisis_apply_haircuts(&mut a);

        assert_eq!(outcome.burned_warming, 200_000); // All warming burned
        assert_eq!(outcome.insurance_draw, 500_000); // All insurance used
        assert_eq!(outcome.equity_haircut_ratio, Q64x64::ONE); // 100% equity haircut
        assert!(outcome.is_solvent); // Solvent after wiping equity

        assert_eq!(a.sigma_warming, 0);
        assert_eq!(a.sigma_insurance, 0);
        assert_eq!(a.sigma_principal, 0); // Principal wiped out
        assert_eq!(a.deficit(), 0);
    }

    #[test]
    fn test_full_waterfall_with_equity_haircut() {
        let mut a = Accums::new();
        a.sigma_principal = 1_000_000;
        a.sigma_realized = 200_000;
        a.sigma_warming = 100_000;
        a.sigma_insurance = 200_000;
        a.sigma_collateral = 500_000; // Deficit of 1_000_000

        let outcome = crisis_apply_haircuts(&mut a);

        // Warming burned: 100k
        assert_eq!(outcome.burned_warming, 100_000);

        // Insurance drawn: 200k
        assert_eq!(outcome.insurance_draw, 200_000);

        // Remaining deficit: 700k to be covered by 1_200k equity
        // Haircut ratio: 700k / 1_200k ≈ 58.33%
        assert!(outcome.equity_haircut_ratio.0 > 0);
        assert!(outcome.is_solvent);

        // All warming and insurance should be depleted
        assert_eq!(a.sigma_warming, 0);
        assert_eq!(a.sigma_insurance, 0);

        // Equity should be reduced
        assert!(a.sigma_principal < 1_000_000);
        assert!(a.sigma_realized < 200_000);

        // System should be solvent
        assert_eq!(a.deficit(), 0);
    }

    #[test]
    fn test_scale_monotonicity() {
        let mut a = Accums::new();
        a.sigma_principal = 1_000_000;
        a.sigma_warming = 500_000;
        a.sigma_collateral = 800_000;

        let initial_eq_scale = a.equity_scale;
        let initial_warm_scale = a.warming_scale;

        let _ = crisis_apply_haircuts(&mut a);

        // Scales should never increase
        assert!(a.equity_scale.0 <= initial_eq_scale.0);
        assert!(a.warming_scale.0 <= initial_warm_scale.0);
    }

    #[test]
    fn test_zero_principal_partial_burn() {
        let mut a = Accums::new();
        a.sigma_principal = 0;
        a.sigma_realized = 0;
        a.sigma_warming = 100_000;
        a.sigma_insurance = 50_000;
        a.sigma_collateral = 0;
        // Liabilities = 0 + 100k warming = 100k
        // Assets = 0 + 50k insurance = 50k
        // Deficit = 50k
        // Will burn min(50k, 100k) = 50k warming to eliminate deficit

        let outcome = crisis_apply_haircuts(&mut a);

        // Should burn enough warming to cover deficit
        assert_eq!(outcome.burned_warming, 50_000);
        // No insurance draw needed (warming covered it)
        assert_eq!(outcome.insurance_draw, 0);
        // No equity haircut (warming covered deficit)
        assert_eq!(outcome.equity_haircut_ratio, Q64x64::ZERO);
        // System becomes solvent after burning warming
        assert!(outcome.is_solvent);

        assert_eq!(a.sigma_warming, 50_000); // 50k warming remains
        assert_eq!(a.sigma_insurance, 50_000); // Untouched
        assert_eq!(a.deficit(), 0);
    }
}
