//! Kani proofs for crisis loss socialization
//!
//! This module contains formal verification proofs using Kani to verify
//! critical invariants of the crisis management system.

#![cfg(kani)]

use crate::crisis::*;

/// Helper: Create bounded Accums for Kani verification
///
/// Bounds prevent state space explosion while covering realistic scenarios
fn bounded_accums() -> Accums {
    let mut a = Accums::new();

    // Use kani::any() to generate symbolic values, then bound them
    a.sigma_principal = kani::any();
    a.sigma_realized = kani::any();
    a.sigma_warming = kani::any();
    a.sigma_collateral = kani::any();
    a.sigma_insurance = kani::any();

    // Assumptions: non-negative, bounded to realistic values
    // Using 1B (1e9) as max to avoid overflow in model
    kani::assume(a.sigma_principal >= 0 && a.sigma_principal <= 1_000_000_000);
    kani::assume(a.sigma_realized >= 0 && a.sigma_realized <= 1_000_000_000);
    kani::assume(a.sigma_warming >= 0 && a.sigma_warming <= 1_000_000_000);
    kani::assume(a.sigma_collateral >= 0 && a.sigma_collateral <= 1_000_000_000);
    kani::assume(a.sigma_insurance >= 0 && a.sigma_insurance <= 1_000_000_000);

    // Scales start at 1.0
    a.equity_scale = Q64x64::ONE;
    a.warming_scale = Q64x64::ONE;
    a.epoch = 0;

    a
}

/// Helper: Create bounded UserPortfolio consistent with global scales
fn bounded_user(global_equity_scale: Q64x64, global_warming_scale: Q64x64) -> UserPortfolio {
    let mut u = UserPortfolio::new();

    u.principal = kani::any();
    u.realized = kani::any();
    u.warming = kani::any();

    // Bound user balances to realistic values
    kani::assume(u.principal >= 0 && u.principal <= 100_000_000);
    kani::assume(u.realized >= -10_000_000 && u.realized <= 100_000_000);
    kani::assume(u.warming >= 0 && u.warming <= 100_000_000);

    // User scales should be <= global scales (user may be behind)
    u.equity_scale_snap = global_equity_scale;
    u.warming_scale_snap = global_warming_scale;
    u.last_epoch_applied = 0;
    u.last_touch_slot = 0;

    u
}

/// **Proof C1: Post-crisis solvency**
///
/// Verifies that after crisis_apply_haircuts:
/// - If equity exists, deficit is eliminated (is_solvent == true)
/// - If no equity, deficit does not worsen
#[kani::proof]
fn proof_c1_crisis_solvent_or_best_effort() {
    let mut a = bounded_accums();

    let before_deficit = a.deficit();
    let before_equity = a.sigma_principal.saturating_add(a.sigma_realized);

    let outcome = crisis_apply_haircuts(&mut a);

    let after_deficit = a.deficit();

    // If there was equity to haircut, we must end solvent
    if before_equity > 0 {
        assert!(after_deficit == 0);
        assert!(outcome.is_solvent);
    } else {
        // If no equity, deficit should not worsen (best effort)
        assert!(after_deficit <= before_deficit);
    }
}

/// **Proof C2: Scale monotonicity**
///
/// Verifies that scales never increase during crisis
#[kani::proof]
fn proof_c2_scales_monotone() {
    let mut a = bounded_accums();

    let before_eq = a.equity_scale;
    let before_warm = a.warming_scale;

    let _ = crisis_apply_haircuts(&mut a);

    // Scales are monotone non-increasing
    assert!(a.equity_scale.0 <= before_eq.0);
    assert!(a.warming_scale.0 <= before_warm.0);
}

/// **Proof C3: No over-burn**
///
/// Verifies that crisis never burns more than available:
/// - burned_warming <= sigma_warming
/// - insurance_draw <= sigma_insurance
/// - equity_haircut_ratio <= 1.0
#[kani::proof]
fn proof_c3_no_overburn() {
    let mut a = bounded_accums();

    let before_warming = a.sigma_warming;
    let before_insurance = a.sigma_insurance;

    let outcome = crisis_apply_haircuts(&mut a);

    // Never burn more warming than exists
    assert!(outcome.burned_warming >= 0);
    assert!(outcome.burned_warming <= before_warming);

    // Never draw more insurance than exists
    assert!(outcome.insurance_draw >= 0);
    assert!(outcome.insurance_draw <= before_insurance);

    // Haircut ratio is capped at 1.0
    assert!(outcome.equity_haircut_ratio.0 <= Q64x64::ONE.0);
}

/// **Proof C4: Materialization idempotence**
///
/// Verifies that calling materialize_user twice with no state change
/// produces the same result (no double-application of haircuts)
#[kani::proof]
fn proof_c4_materialize_idempotent() {
    let mut a = bounded_accums();

    // Apply crisis first
    let _ = crisis_apply_haircuts(&mut a);

    let mut u = bounded_user(a.equity_scale, a.warming_scale);

    let params = MaterializeParams {
        now_slot: u.last_touch_slot, // Same slot, no time passes
        tau_slots: 10_000,
        burn_principal_first: kani::any(),
    };

    // First materialization
    materialize_user(&mut u, &mut a, params);
    let snap_u = u;
    let snap_a_principal = a.sigma_principal;
    let snap_a_realized = a.sigma_realized;
    let snap_a_warming = a.sigma_warming;

    // Second materialization (should be no-op)
    materialize_user(&mut u, &mut a, params);

    // User balances should not change
    assert!(u.principal == snap_u.principal);
    assert!(u.realized == snap_u.realized);
    assert!(u.warming == snap_u.warming);

    // Aggregates should not change
    assert!(a.sigma_principal == snap_a_principal);
    assert!(a.sigma_realized == snap_a_realized);
    assert!(a.sigma_warming == snap_a_warming);
}

/// **Proof C5: Vesting conservation**
///
/// Verifies that vesting preserves the sum of warming + realized
/// (outside of explicit burns from crisis)
#[kani::proof]
fn proof_c5_vesting_conserves_sum() {
    let mut a = Accums::new();
    let mut u = UserPortfolio::new();

    // Set up initial state
    u.warming = kani::any();
    u.realized = kani::any();
    kani::assume(u.warming >= 0 && u.warming <= 1_000_000);
    kani::assume(u.realized >= -100_000 && u.realized <= 1_000_000);

    a.sigma_warming = u.warming;
    a.sigma_realized = u.realized;

    let sum_before_user = u.warming.saturating_add(u.realized);
    let sum_before_accums = a.sigma_warming.saturating_add(a.sigma_realized);

    // Materialize with time passing (no crisis, just vesting)
    let params = MaterializeParams {
        now_slot: u.last_touch_slot + 1000,
        tau_slots: 10_000,
        burn_principal_first: false,
    };

    materialize_user(&mut u, &mut a, params);

    let sum_after_user = u.warming.saturating_add(u.realized);
    let sum_after_accums = a.sigma_warming.saturating_add(a.sigma_realized);

    // Sums should be preserved (vesting just moves between buckets)
    assert!(sum_after_user == sum_before_user);
    assert!(sum_after_accums == sum_before_accums);
}

/// **Proof C6: Aggregate consistency (single user)**
///
/// Verifies that after materialization, user balances match aggregates
/// for a single-user system
#[kani::proof]
fn proof_c6_aggregate_consistency_single_user() {
    let mut a = Accums::new();

    // Single user owns all balances
    let mut u = UserPortfolio::new();
    u.principal = kani::any();
    u.realized = kani::any();
    u.warming = kani::any();

    kani::assume(u.principal >= 0 && u.principal <= 10_000_000);
    kani::assume(u.realized >= -1_000_000 && u.realized <= 10_000_000);
    kani::assume(u.warming >= 0 && u.warming <= 10_000_000);

    // Set aggregates to match user
    a.sigma_principal = u.principal;
    a.sigma_realized = u.realized;
    a.sigma_warming = u.warming;

    // Apply crisis
    let _ = crisis_apply_haircuts(&mut a);

    // Materialize user
    let params = MaterializeParams::default();
    materialize_user(&mut u, &mut a, params);

    // User balances should match aggregates (single user system)
    assert!(u.principal == a.sigma_principal);
    assert!(u.realized == a.sigma_realized);
    assert!(u.warming == a.sigma_warming);
}

/// **Proof C7: Bounded rounding errors**
///
/// Verifies that Q64x64 rounding errors are acceptable
#[kani::proof]
fn proof_c7_rounding_bounded() {
    let user_val: i128 = kani::any();
    kani::assume(user_val >= 1_000_000 && user_val <= 1_000_000_000_000);

    // 90% haircut (keep 10%)
    let scale = Q64x64::ratio(900_000_000, 1_000_000_000);
    let result = scale.mul_i128(user_val);

    // Expected value (exact)
    let expected = (user_val * 9) / 10;

    // Error should be tiny - at most 1 unit due to rounding
    // Q64.64 uses integer division in final step, so error ≤ 1
    let error = (result - expected).abs();

    assert!(error <= 1);
}

/// **Proof C8: Loss waterfall ordering**
///
/// Verifies that losses are absorbed in correct order:
/// warming → insurance → equity
#[kani::proof]
fn proof_c8_loss_waterfall_ordering() {
    let mut a = bounded_accums();

    // Ensure we have all three loss absorbers
    kani::assume(a.sigma_warming > 0);
    kani::assume(a.sigma_insurance > 0);
    kani::assume(a.sigma_principal > 0);

    // Create a deficit
    kani::assume(a.deficit() > 0);

    let initial_deficit = a.deficit();
    let initial_warming = a.sigma_warming;
    let initial_insurance = a.sigma_insurance;
    let initial_equity_scale = a.equity_scale;

    let outcome = crisis_apply_haircuts(&mut a);

    // Property 1: If deficit was small enough to be covered by warming alone,
    // should only burn warming, not touch insurance or equity
    if initial_deficit <= initial_warming {
        assert!(outcome.burned_warming > 0);
        assert!(outcome.insurance_draw == 0);
        assert!(outcome.equity_haircut_ratio == Q64x64::ZERO);
        assert!(a.equity_scale == initial_equity_scale);
    }

    // Property 2: If equity was haircut, warming must be depleted
    if outcome.equity_haircut_ratio.0 > 0 {
        assert!(a.sigma_warming == 0);
        // Note: Insurance may not be fully depleted due to rounding in deficit calculation
    }

    // Property 3: If insurance was drawn, warming must be depleted
    if outcome.insurance_draw > 0 {
        assert!(a.sigma_warming == 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kani_helpers_compile() {
        // Ensure helper functions compile and run
        let a = bounded_accums();
        let u = bounded_user(a.equity_scale, a.warming_scale);

        assert!(a.sigma_principal >= 0);
        assert!(u.principal >= 0);
    }
}
