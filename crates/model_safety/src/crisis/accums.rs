//! Global accumulators and user portfolio state for crisis management
//!
//! This module defines the core data structures for O(1) crisis loss socialization:
//! - `Accums`: Global aggregates tracking total system liabilities and assets
//! - `UserPortfolio`: Per-user state with lazy scale reconciliation

use crate::crisis::amount::Q64x64;

/// Global system accumulators
///
/// Tracks aggregate balances across all users. These are the "source of truth"
/// and are updated immediately during crisis events. Individual users reconcile
/// lazily when they next touch the system.
///
/// # Invariants
/// - `equity_scale` and `warming_scale` are monotone non-increasing
/// - `equity_scale` and `warming_scale` are in range [0, 1]
/// - Σ fields represent the sum of all user balances (after applying scales)
/// - `assets()` >= 0
/// - `liabilities()` >= 0
#[derive(Copy, Clone, Debug)]
pub struct Accums {
    /// Sum of all user principal (user deposits in numéraire, e.g., USDC 1e6)
    pub sigma_principal: i128,

    /// Sum of all user realized PnL (fully vested gains/losses)
    pub sigma_realized: i128,

    /// Sum of all user warming PnL (unvested gains, subject to warmup)
    pub sigma_warming: i128,

    /// Total collateral held in vault (assets backing liabilities)
    pub sigma_collateral: i128,

    /// Insurance fund balance (secondary loss absorber)
    pub sigma_insurance: i128,

    /// Global equity scale factor (applied to principal + realized)
    /// - Starts at 1.0 (Q64x64::ONE)
    /// - Decreases during crisis when equity is haircut
    /// - Never increases (monotone non-increasing)
    pub equity_scale: Q64x64,

    /// Global warming scale factor (applied to warming PnL)
    /// - Starts at 1.0 (Q64x64::ONE)
    /// - Decreases during crisis when warming is burned
    /// - Never increases (monotone non-increasing)
    pub warming_scale: Q64x64,

    /// Epoch counter (incremented on each crisis event)
    /// Used to track which users have reconciled the latest crisis
    pub epoch: u64,
}

impl Accums {
    /// Create new accumulators with default values
    ///
    /// # Returns
    /// Accums with all Σ fields at 0, scales at 1.0, epoch at 0
    pub const fn new() -> Self {
        Accums {
            sigma_principal: 0,
            sigma_realized: 0,
            sigma_warming: 0,
            sigma_collateral: 0,
            sigma_insurance: 0,
            equity_scale: Q64x64::ONE,
            warming_scale: Q64x64::ONE,
            epoch: 0,
        }
    }

    /// Calculate total assets
    ///
    /// # Returns
    /// Sum of collateral and insurance fund
    #[inline]
    pub fn assets(&self) -> i128 {
        self.sigma_collateral.saturating_add(self.sigma_insurance)
    }

    /// Calculate total liabilities
    ///
    /// # Returns
    /// Sum of principal, realized PnL, and warming PnL
    #[inline]
    pub fn liabilities(&self) -> i128 {
        self.sigma_principal
            .saturating_add(self.sigma_realized)
            .saturating_add(self.sigma_warming)
    }

    /// Calculate deficit (liabilities - assets)
    ///
    /// # Returns
    /// If liabilities > assets, returns the deficit (positive value)
    /// Otherwise returns 0 (system is solvent)
    #[inline]
    pub fn deficit(&self) -> i128 {
        let d = self.liabilities().saturating_sub(self.assets());
        if d > 0 { d } else { 0 }
    }

    /// Check if system is solvent
    ///
    /// # Returns
    /// true if assets >= liabilities, false otherwise
    #[inline]
    pub fn is_solvent(&self) -> bool {
        self.deficit() == 0
    }
}

impl Default for Accums {
    fn default() -> Self {
        Self::new()
    }
}

/// Per-user portfolio state
///
/// Tracks individual user balances and scale snapshots for lazy reconciliation.
/// When global scales change (during crisis), users do not update immediately.
/// Instead, they reconcile the next time they touch the system via `materialize_user()`.
///
/// # Invariants
/// - `equity_scale_snap` <= global `equity_scale` (user may be behind)
/// - `warming_scale_snap` <= global `warming_scale` (user may be behind)
/// - `last_epoch_applied` <= global `epoch` (user may be behind)
/// - All balance fields use same numéraire as Accums (e.g., USDC 1e6)
#[derive(Copy, Clone, Debug)]
pub struct UserPortfolio {
    /// User's principal (original deposits, always withdrawable)
    pub principal: i128,

    /// User's realized PnL (fully vested gains/losses)
    pub realized: i128,

    /// User's warming PnL (unvested gains, subject to warmup throttling)
    pub warming: i128,

    /// Snapshot of global equity_scale when user last reconciled
    /// Used to detect if equity haircuts have occurred since last touch
    pub equity_scale_snap: Q64x64,

    /// Snapshot of global warming_scale when user last reconciled
    /// Used to detect if warming burns have occurred since last touch
    pub warming_scale_snap: Q64x64,

    /// Last epoch this user reconciled
    /// If < global epoch, user needs to materialize crisis losses
    pub last_epoch_applied: u64,

    /// Last slot when user performed an action (for vesting calculation)
    pub last_touch_slot: u64,
}

impl UserPortfolio {
    /// Create new user portfolio with default values
    ///
    /// # Returns
    /// UserPortfolio with all balances at 0, scales at 1.0, epochs at 0
    pub const fn new() -> Self {
        UserPortfolio {
            principal: 0,
            realized: 0,
            warming: 0,
            equity_scale_snap: Q64x64::ONE,
            warming_scale_snap: Q64x64::ONE,
            last_epoch_applied: 0,
            last_touch_slot: 0,
        }
    }

    /// Calculate user's total equity (principal + realized)
    ///
    /// Note: Does NOT include warming (unvested) PnL
    ///
    /// # Returns
    /// Sum of principal and realized PnL
    #[inline]
    pub fn equity(&self) -> i128 {
        self.principal.saturating_add(self.realized)
    }

    /// Calculate user's total balance (equity + warming)
    ///
    /// # Returns
    /// Sum of principal, realized, and warming
    #[inline]
    pub fn total_balance(&self) -> i128 {
        self.equity().saturating_add(self.warming)
    }

    /// Check if user needs to reconcile crisis losses
    ///
    /// # Arguments
    /// * `global_epoch` - Current global epoch from Accums
    ///
    /// # Returns
    /// true if user is behind and needs materialization
    #[inline]
    pub fn needs_materialization(&self, global_epoch: u64) -> bool {
        self.last_epoch_applied < global_epoch
    }
}

impl Default for UserPortfolio {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_accums_default() {
        let a = Accums::default();
        assert_eq!(a.sigma_principal, 0);
        assert_eq!(a.sigma_realized, 0);
        assert_eq!(a.sigma_warming, 0);
        assert_eq!(a.equity_scale, Q64x64::ONE);
        assert_eq!(a.warming_scale, Q64x64::ONE);
        assert_eq!(a.epoch, 0);
    }

    #[test]
    fn test_accums_assets_liabilities() {
        let mut a = Accums::new();
        a.sigma_collateral = 1_000_000;
        a.sigma_insurance = 500_000;
        a.sigma_principal = 800_000;
        a.sigma_realized = 300_000;
        a.sigma_warming = 100_000;

        assert_eq!(a.assets(), 1_500_000);
        assert_eq!(a.liabilities(), 1_200_000);
        assert_eq!(a.deficit(), 0); // Solvent
        assert!(a.is_solvent());
    }

    #[test]
    fn test_accums_deficit() {
        let mut a = Accums::new();
        a.sigma_collateral = 800_000;
        a.sigma_insurance = 100_000;
        a.sigma_principal = 1_000_000;
        a.sigma_realized = 200_000;

        assert_eq!(a.assets(), 900_000);
        assert_eq!(a.liabilities(), 1_200_000);
        assert_eq!(a.deficit(), 300_000); // Insolvent by 300k
        assert!(!a.is_solvent());
    }

    #[test]
    fn test_user_portfolio_default() {
        let u = UserPortfolio::default();
        assert_eq!(u.principal, 0);
        assert_eq!(u.realized, 0);
        assert_eq!(u.warming, 0);
        assert_eq!(u.equity_scale_snap, Q64x64::ONE);
        assert_eq!(u.warming_scale_snap, Q64x64::ONE);
        assert_eq!(u.last_epoch_applied, 0);
    }

    #[test]
    fn test_user_portfolio_balances() {
        let mut u = UserPortfolio::new();
        u.principal = 1_000_000;
        u.realized = 200_000;
        u.warming = 50_000;

        assert_eq!(u.equity(), 1_200_000);
        assert_eq!(u.total_balance(), 1_250_000);
    }

    #[test]
    fn test_user_needs_materialization() {
        let mut u = UserPortfolio::new();
        u.last_epoch_applied = 5;

        assert!(!u.needs_materialization(5)); // Current
        assert!(!u.needs_materialization(4)); // Ahead (shouldn't happen but handle gracefully)
        assert!(u.needs_materialization(6));  // Behind, needs update
        assert!(u.needs_materialization(10)); // Far behind
    }
}
