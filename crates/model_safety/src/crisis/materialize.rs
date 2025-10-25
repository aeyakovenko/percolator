//! Lazy user materialization after crisis events
//!
//! When a crisis occurs, global scales are updated but individual users are NOT
//! updated immediately. Instead, users reconcile the next time they "touch" the
//! system (deposit, withdraw, trade, etc). This is called "lazy materialization".
//!
//! ## Why Lazy?
//!
//! - **O(1) crisis**: Updating millions of users would be O(N) and infeasible on-chain
//! - **Pay for what you use**: Only active users pay the reconciliation cost
//! - **Aggregate authority**: Global Σ fields are authoritative, users sync to them
//!
//! ## Reconciliation Steps
//!
//! 1. **Apply equity scale delta**: Haircut principal + realized (if scale changed)
//! 2. **Apply warming scale delta**: Burn warming PnL (if scale changed)
//! 3. **Vest warming → realized**: Time-based vesting (updates both user and Σ)
//!
//! ## Key Invariant
//!
//! After materialization:
//! ```text
//! Σ_users(user.principal + user.realized + user.warming)
//! == a.sigma_principal + a.sigma_realized + a.sigma_warming
//! ```

use crate::crisis::amount::Q64x64;
use crate::crisis::accums::{Accums, UserPortfolio};

/// Parameters for user materialization
#[derive(Copy, Clone, Debug)]
pub struct MaterializeParams {
    /// Current slot number (for vesting calculation)
    pub now_slot: u64,

    /// Vesting time constant in slots
    /// - For linear vesting: time for 100% unlock
    /// - For exponential: tau in exp(-t/tau)
    /// Suggested: 30 min @ 400ms/slot = 4500 slots
    pub tau_slots: u64,

    /// Burn order when equity is haircut
    /// - true: Burn principal first, then realized
    /// - false: Burn realized first, then principal
    /// Recommended: false (preserve deposits over PnL)
    pub burn_principal_first: bool,
}

impl Default for MaterializeParams {
    fn default() -> Self {
        MaterializeParams {
            now_slot: 0,
            tau_slots: 4500, // 30 min @ 400ms/slot
            burn_principal_first: false, // Preserve principal
        }
    }
}

/// Materialize user portfolio after crisis events
///
/// Applies any pending global scale changes to the user's balances and
/// performs time-based vesting from warming → realized.
///
/// ## Important Notes
///
/// - **Idempotent**: Safe to call multiple times (no-op if already current)
/// - **Updates Σ for vesting only**: Scale deltas do NOT update Σ (already reflected)
/// - **Vesting DOES update Σ**: Moving warming → realized changes both user and global
///
/// # Arguments
/// * `u` - Mutable reference to user portfolio
/// * `a` - Mutable reference to global accumulators
/// * `p` - Materialization parameters
///
/// # Guarantees
/// - User scales are brought current with global scales
/// - Aggregate consistency is maintained
/// - No underflow (uses saturating arithmetic)
/// - Vesting is deterministic and time-based
///
/// # Example
/// ```
/// use model_safety::crisis::{Accums, UserPortfolio, materialize_user, MaterializeParams};
///
/// let mut a = Accums::new();
/// let mut u = UserPortfolio::new();
/// u.principal = 1_000_000;
/// u.warming = 100_000;
///
/// // ... crisis occurs, a.equity_scale decreases ...
///
/// let params = MaterializeParams::default();
/// materialize_user(&mut u, &mut a, params);
///
/// // User's principal is now haircut to match global scale
/// // Warming has vested partially to realized
/// ```
pub fn materialize_user(
    u: &mut UserPortfolio,
    a: &mut Accums,
    p: MaterializeParams,
) {
    // Step 1: Apply equity scale delta (if scales diverged)
    if u.equity_scale_snap != a.equity_scale {
        apply_equity_scale_delta(u, a, p.burn_principal_first);
    }

    // Step 2: Apply warming scale delta (if scales diverged)
    if u.warming_scale_snap != a.warming_scale {
        apply_warming_scale_delta(u, a);
    }

    // Step 3: Vest warming → realized (time-based)
    vest_warming_to_realized(u, a, p.now_slot, p.tau_slots);

    // Update user's slot timestamp
    u.last_touch_slot = p.now_slot;
}

/// Apply equity scale delta to user balances
///
/// Haircuts user's principal and realized to match global equity_scale.
/// Does NOT update aggregates (they were already scaled during crisis).
fn apply_equity_scale_delta(
    u: &mut UserPortfolio,
    a: &Accums,
    burn_principal_first: bool,
) {
    // Calculate scale ratio: global_scale / user_snap
    // This tells us what fraction of the user's balance to keep
    let scale_num = a.equity_scale.0 as u128;
    let scale_den = core::cmp::max(u.equity_scale_snap.0, 1) as u128;

    // If scales are equal, no-op (avoid division)
    if scale_num == scale_den {
        return;
    }

    // scale_delta represents the new scale relative to user's snapshot
    // If global < user_snap, this will be < 1.0 (haircut)
    // If global > user_snap, this would be > 1.0 (should never happen, but cap at 1.0)
    let scale_delta = if scale_num >= scale_den {
        Q64x64::ONE // Cap at 1.0 (scales are monotone non-increasing)
    } else {
        Q64x64((scale_num << 64) / scale_den)
    };

    let pre_equity = u.principal.saturating_add(u.realized);
    let post_equity = scale_delta.mul_i128(pre_equity);
    let burn = pre_equity.saturating_sub(post_equity);

    // Apply burn to principal and/or realized based on policy
    if burn > 0 {
        if burn_principal_first {
            // Burn principal first
            let burn_p = core::cmp::min(burn, u.principal);
            u.principal = u.principal.saturating_sub(burn_p);
            let burn_r = burn.saturating_sub(burn_p);
            u.realized = u.realized.saturating_sub(burn_r);
        } else {
            // Burn realized first (default, preserves deposits)
            let burn_r = core::cmp::min(burn, u.realized);
            u.realized = u.realized.saturating_sub(burn_r);
            let burn_p = burn.saturating_sub(burn_r);
            u.principal = u.principal.saturating_sub(burn_p);
        }
    }

    // Update user's snapshot to current global scale
    u.equity_scale_snap = a.equity_scale;
    u.last_epoch_applied = a.epoch;

    // IMPORTANT: Do NOT update Σ here - aggregates were already adjusted during crisis
}

/// Apply warming scale delta to user balances
///
/// Burns user's warming PnL to match global warming_scale.
/// Does NOT update aggregates (they were already scaled during crisis).
fn apply_warming_scale_delta(u: &mut UserPortfolio, a: &Accums) {
    // Calculate scale ratio: global_scale / user_snap
    let scale_num = a.warming_scale.0 as u128;
    let scale_den = core::cmp::max(u.warming_scale_snap.0, 1) as u128;

    // If scales are equal, no-op
    if scale_num == scale_den {
        return;
    }

    // Scale delta (should be <= 1.0 due to monotonicity)
    let scale_delta = if scale_num >= scale_den {
        Q64x64::ONE
    } else {
        Q64x64((scale_num << 64) / scale_den)
    };

    let pre_warming = u.warming;
    let post_warming = scale_delta.mul_i128(pre_warming);
    let burned = pre_warming.saturating_sub(post_warming);

    if burned > 0 {
        u.warming = post_warming;
    }

    // Update user's snapshot
    u.warming_scale_snap = a.warming_scale;

    // IMPORTANT: Do NOT update Σ_warming here - already adjusted during crisis
}

/// Vest warming PnL to realized based on time elapsed
///
/// Moves a portion of warming → realized based on deterministic vesting schedule.
/// This DOES update aggregates (both user and Σ are modified).
fn vest_warming_to_realized(
    u: &mut UserPortfolio,
    a: &mut Accums,
    now_slot: u64,
    tau_slots: u64,
) {
    if u.warming <= 0 {
        return; // No warming to vest
    }

    if now_slot <= u.last_touch_slot {
        return; // No time has passed
    }

    let dt = now_slot.saturating_sub(u.last_touch_slot);

    // Simple linear vesting: v = warming * min(dt / tau, 1.0)
    // For exponential vesting: v = warming * (1 - exp(-dt/tau))
    // Using linear for simplicity (deterministic, no float math)
    let vested = if dt >= tau_slots {
        // Fully vested
        u.warming
    } else {
        // Partial vesting: (warming * dt) / tau
        let f_num = dt as i128;
        let f_den = tau_slots.max(1) as i128;
        (u.warming * f_num) / f_den
    };

    if vested > 0 {
        // Move from warming → realized
        u.warming = u.warming.saturating_sub(vested);
        u.realized = u.realized.saturating_add(vested);

        // Update aggregates (this IS reflected in Σ, unlike scale deltas)
        a.sigma_warming = a.sigma_warming.saturating_sub(vested);
        a.sigma_realized = a.sigma_realized.saturating_add(vested);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_materialize_no_crisis_no_op() {
        let mut a = Accums::new();
        let mut u = UserPortfolio::new();
        u.principal = 1_000_000;

        let params = MaterializeParams::default();
        materialize_user(&mut u, &mut a, params);

        // Nothing should change (scales match, no time passed)
        assert_eq!(u.principal, 1_000_000);
    }

    #[test]
    fn test_materialize_equity_haircut() {
        let mut a = Accums::new();
        a.sigma_principal = 1_000_000;
        a.equity_scale = Q64x64(Q64x64::ONE.0 / 2); // 50% haircut
        a.epoch = 1;

        let mut u = UserPortfolio::new();
        u.principal = 1_000_000;
        u.equity_scale_snap = Q64x64::ONE; // User hasn't reconciled yet

        let params = MaterializeParams::default();
        materialize_user(&mut u, &mut a, params);

        // User's principal should be haircut by 50%
        assert_eq!(u.principal, 500_000);
        assert_eq!(u.equity_scale_snap, a.equity_scale);
        assert_eq!(u.last_epoch_applied, 1);
    }

    #[test]
    fn test_materialize_warming_burn() {
        let mut a = Accums::new();
        a.sigma_warming = 500_000;
        a.warming_scale = Q64x64(Q64x64::ONE.0 * 3 / 4); // 25% burn
        a.epoch = 1;

        let mut u = UserPortfolio::new();
        u.warming = 1_000_000;
        u.warming_scale_snap = Q64x64::ONE;

        let params = MaterializeParams::default();
        materialize_user(&mut u, &mut a, params);

        // User's warming should be reduced by 25%
        assert_eq!(u.warming, 750_000);
        assert_eq!(u.warming_scale_snap, a.warming_scale);
    }

    #[test]
    fn test_materialize_vesting() {
        let mut a = Accums::new();
        a.sigma_warming = 1_000_000;
        a.sigma_realized = 0;

        let mut u = UserPortfolio::new();
        u.warming = 1_000_000;
        u.realized = 0;
        u.last_touch_slot = 0;

        let mut params = MaterializeParams::default();
        params.now_slot = params.tau_slots / 2; // 50% of vesting period

        materialize_user(&mut u, &mut a, params);

        // 50% should have vested
        assert_eq!(u.warming, 500_000);
        assert_eq!(u.realized, 500_000);

        // Aggregates should be updated
        assert_eq!(a.sigma_warming, 500_000);
        assert_eq!(a.sigma_realized, 500_000);
    }

    #[test]
    fn test_materialize_full_vesting() {
        let mut a = Accums::new();
        a.sigma_warming = 1_000_000;

        let mut u = UserPortfolio::new();
        u.warming = 1_000_000;
        u.last_touch_slot = 0;

        let mut params = MaterializeParams::default();
        params.now_slot = params.tau_slots; // Full vesting period elapsed

        materialize_user(&mut u, &mut a, params);

        // 100% should have vested
        assert_eq!(u.warming, 0);
        assert_eq!(u.realized, 1_000_000);
        assert_eq!(a.sigma_warming, 0);
        assert_eq!(a.sigma_realized, 1_000_000);
    }

    #[test]
    fn test_materialize_idempotent() {
        let mut a = Accums::new();
        a.sigma_principal = 1_000_000;

        let mut u = UserPortfolio::new();
        u.principal = 1_000_000;

        let params = MaterializeParams::default();

        // First call
        materialize_user(&mut u, &mut a, params);
        let principal_after_first = u.principal;

        // Second call (should be no-op)
        materialize_user(&mut u, &mut a, params);
        assert_eq!(u.principal, principal_after_first);
    }

    #[test]
    fn test_burn_realized_first() {
        let mut a = Accums::new();
        a.equity_scale = Q64x64(Q64x64::ONE.0 / 2); // 50% haircut

        let mut u = UserPortfolio::new();
        u.principal = 1_000_000;
        u.realized = 500_000;
        u.equity_scale_snap = Q64x64::ONE;

        let mut params = MaterializeParams::default();
        params.burn_principal_first = false; // Preserve principal

        materialize_user(&mut u, &mut a, params);

        // Total equity was 1.5M, haircut to 750k
        // Should burn realized first: 500k realized + 250k principal
        assert_eq!(u.realized, 0);
        assert_eq!(u.principal, 750_000);
    }

    #[test]
    fn test_burn_principal_first() {
        let mut a = Accums::new();
        a.equity_scale = Q64x64(Q64x64::ONE.0 / 2); // 50% haircut

        let mut u = UserPortfolio::new();
        u.principal = 1_000_000;
        u.realized = 500_000;
        u.equity_scale_snap = Q64x64::ONE;

        let mut params = MaterializeParams::default();
        params.burn_principal_first = true;

        materialize_user(&mut u, &mut a, params);

        // Should burn principal first
        assert_eq!(u.principal, 250_000);
        assert_eq!(u.realized, 500_000);
    }
}
