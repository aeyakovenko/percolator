//! Crisis loss socialization with O(1) complexity
//!
//! This module implements a formally-verified crisis management system for
//! decentralized exchanges that can socialize losses across all users without
//! requiring O(N) iteration over user accounts.
//!
//! ## Key Features
//!
//! - **O(1) Crisis Resolution**: Uses global scale factors instead of per-user updates
//! - **Lazy Materialization**: Users reconcile losses on their next action
//! - **Loss Waterfall**: warming → insurance → equity (principal + realized)
//! - **Formally Verified**: Kani proofs for all critical invariants
//! - **No_std Compatible**: Works in constrained environments (Solana BPF)
//!
//! ## Architecture
//!
//! ```text
//! Crisis Event:
//! 1. Calculate deficit (liabilities - assets)
//! 2. Update global scales (WARMING_SCALE, EQUITY_SCALE)
//! 3. Update aggregates (Σ) immediately
//! 4. Increment epoch
//! [O(1) complexity]
//!
//! User Touch:
//! 1. Check if user.epoch < global.epoch
//! 2. If behind, apply scale deltas to user balances
//! 3. Vest warming → realized (time-based)
//! 4. Update user.epoch
//! [O(1) per user, lazy]
//! ```
//!
//! ## Usage Example
//!
//! ```rust
//! use model_safety::crisis::*;
//!
//! // Initialize system
//! let mut accums = Accums::new();
//! accums.sigma_principal = 1_000_000;
//! accums.sigma_collateral = 800_000; // Deficit of 200k
//!
//! // Crisis occurs
//! let outcome = crisis_apply_haircuts(&mut accums);
//! assert!(outcome.is_solvent);
//!
//! // Later, user touches system
//! let mut user = UserPortfolio::new();
//! user.principal = 100_000;
//!
//! let params = MaterializeParams::default();
//! materialize_user(&mut user, &mut accums, params);
//!
//! // User's balance is now haircut to match global scale
//! assert!(user.principal < 100_000);
//! ```
//!
//! ## Invariants
//!
//! The following invariants are maintained and verified by Kani proofs:
//!
//! 1. **Solvency**: After crisis, deficit == 0 (if equity exists)
//! 2. **Monotonic Scales**: Scales never increase, only decrease
//! 3. **Bounded Burns**: Never burn more than available
//! 4. **Aggregate Consistency**: Σ(user balances) == Σ fields
//! 5. **Idempotent Materialization**: Calling twice doesn't double-apply
//!
//! ## Integration with Adaptive Warmup
//!
//! This crisis module complements the existing adaptive warmup system:
//!
//! - **Normal**: Warmup throttles PnL withdrawals based on deposit drain
//! - **Stress**: Warmup freezes when drain ≥25% + tripwires
//! - **Insolvency**: Crisis socializes losses via haircuts
//!
//! The `UserPortfolio::warming` field maps to `Portfolio::vested_pnl` in the Router.

pub mod amount;
pub mod accums;
pub mod haircut;
pub mod materialize;

#[cfg(kani)]
pub mod proofs;

// Re-export core types for convenience
pub use amount::Q64x64;
pub use accums::{Accums, UserPortfolio};
pub use haircut::{crisis_apply_haircuts, CrisisOutcome};
pub use materialize::{materialize_user, MaterializeParams};

#[cfg(test)]
mod tests {
    use super::*;

    /// Integration test: Full crisis workflow with multiple users
    #[test]
    fn test_full_crisis_workflow() {
        // Setup: 3 users with different balances
        let mut accums = Accums::new();

        let mut user1 = UserPortfolio::new();
        user1.principal = 1_000_000;
        user1.realized = 200_000;
        user1.warming = 100_000;

        let mut user2 = UserPortfolio::new();
        user2.principal = 500_000;
        user2.realized = 50_000;

        let mut user3 = UserPortfolio::new();
        user3.principal = 200_000;
        user3.warming = 50_000;

        // Update aggregates
        accums.sigma_principal = user1.principal + user2.principal + user3.principal;
        accums.sigma_realized = user1.realized + user2.realized;
        accums.sigma_warming = user1.warming + user3.warming;

        // Set collateral and insurance
        accums.sigma_collateral = 1_500_000;
        accums.sigma_insurance = 200_000;

        // Calculate deficit: 2100k liabilities - 1700k assets = 400k deficit
        let initial_deficit = accums.deficit();
        assert_eq!(initial_deficit, 400_000);

        // Apply crisis
        let outcome = crisis_apply_haircuts(&mut accums);

        // Should burn all warming (150k) and all insurance (200k)
        // Remaining deficit: 0 (covered by equity haircut of 3.57% approx)
        assert_eq!(outcome.burned_warming, 150_000);
        assert_eq!(outcome.insurance_draw, 200_000);
        assert!(outcome.is_solvent);

        // Check that aggregates were updated
        assert_eq!(accums.sigma_warming, 0);
        assert_eq!(accums.sigma_insurance, 0);
        assert!(accums.sigma_principal < 1_700_000);

        // Now materialize each user
        let params = MaterializeParams::default();

        materialize_user(&mut user1, &mut accums, params);
        materialize_user(&mut user2, &mut accums, params);
        materialize_user(&mut user3, &mut accums, params);

        // All users should have their warming burned
        assert_eq!(user1.warming, 0);
        assert_eq!(user3.warming, 0);

        // All users should have equity haircut
        assert!(user1.principal + user1.realized < 1_200_000);
        assert!(user2.principal + user2.realized < 550_000);
        assert!(user3.principal < 200_000);

        // System should be solvent
        assert_eq!(accums.deficit(), 0);
    }

    /// Integration test: Vesting conservation
    #[test]
    fn test_vesting_conserves_total() {
        let mut accums = Accums::new();
        accums.sigma_warming = 1_000_000;
        accums.sigma_realized = 0;

        let mut user = UserPortfolio::new();
        user.warming = 1_000_000;
        user.realized = 0;
        user.last_touch_slot = 0;

        let sum_before_user = user.warming + user.realized;
        let sum_before_accums = accums.sigma_warming + accums.sigma_realized;

        // Vest 50%
        let mut params = MaterializeParams::default();
        params.now_slot = params.tau_slots / 2;

        materialize_user(&mut user, &mut accums, params);

        let sum_after_user = user.warming + user.realized;
        let sum_after_accums = accums.sigma_warming + accums.sigma_realized;

        // Sums should be preserved
        assert_eq!(sum_before_user, sum_after_user);
        assert_eq!(sum_before_accums, sum_after_accums);
    }

    /// Integration test: No deficit, no crisis
    #[test]
    fn test_solvent_system_no_haircut() {
        let mut accums = Accums::new();
        accums.sigma_principal = 1_000_000;
        accums.sigma_collateral = 1_500_000;

        let initial_equity_scale = accums.equity_scale;
        let initial_warming_scale = accums.warming_scale;

        let outcome = crisis_apply_haircuts(&mut accums);

        assert_eq!(outcome.burned_warming, 0);
        assert_eq!(outcome.insurance_draw, 0);
        assert_eq!(outcome.equity_haircut_ratio, Q64x64::ZERO);
        assert!(outcome.is_solvent);

        // Scales should not have changed
        assert_eq!(accums.equity_scale, initial_equity_scale);
        assert_eq!(accums.warming_scale, initial_warming_scale);

        // Epoch should not have incremented (no crisis)
        assert_eq!(accums.epoch, 0);
    }
}
