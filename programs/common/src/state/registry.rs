//! Protocol registry for global state and governance

use pinocchio::pubkey::Pubkey;

/// Protocol registry account (formerly SlabRegistry, now whitelist-free)
/// PDA: ["registry", router_id]
///
/// Stores global protocol parameters and state. Users permissionlessly choose
/// which matchers to interact with - no whitelist needed.
#[repr(C)]
pub struct SlabRegistry {
    /// Router program ID
    pub router_id: Pubkey,
    /// Governance authority (can update registry)
    pub governance: Pubkey,
    /// Insurance withdrawal authority (can withdraw insurance surplus and top-up)
    pub insurance_authority: Pubkey,
    /// Bump seed
    pub bump: u8,
    /// Padding
    pub _padding: [u8; 7],

    // Liquidation parameters (global)
    /// Initial margin ratio (basis points, e.g., 500 = 5%)
    pub imr: u64,
    /// Maintenance margin ratio (basis points, e.g., 250 = 2.5%)
    pub mmr: u64,
    /// Liquidation price band (basis points, e.g., 200 = 2%)
    pub liq_band_bps: u64,
    /// Pre-liquidation buffer (equity > MM but < MM + buffer triggers pre-liq)
    pub preliq_buffer: i128,
    /// Pre-liquidation tighter band (basis points, e.g., 100 = 1%)
    pub preliq_band_bps: u64,
    /// Maximum size router can execute per slab in one tx
    pub router_cap_per_slab: u64,
    /// Minimum equity required to provide quotes
    pub min_equity_to_quote: i128,
    /// Oracle price tolerance (basis points, e.g., 50 = 0.5%)
    pub oracle_tolerance_bps: u64,
    /// Maximum oracle staleness (seconds, e.g., 60 = 1 minute)
    pub max_oracle_staleness_secs: i64,

    // Insurance fund parameters and state
    /// Insurance parameters (configurable by governance)
    pub insurance_params: crate::state::insurance::InsuranceParams,
    /// Insurance state (runtime tracking)
    pub insurance_state: crate::state::insurance::InsuranceState,

    // PnL vesting parameters and global haircut state
    /// PnL vesting parameters (configurable by governance)
    pub pnl_vesting_params: crate::state::pnl_vesting::PnlVestingParams,
    /// Global haircut state (runtime tracking)
    pub global_haircut: crate::state::pnl_vesting::GlobalHaircut,

    // Adaptive warmup configuration and state
    /// Adaptive warmup configuration (configurable by governance)
    pub warmup_config: model_safety::adaptive_warmup::AdaptiveWarmupConfig,
    /// Adaptive warmup state (runtime tracking of deposit drain and unlock fraction)
    pub warmup_state: model_safety::adaptive_warmup::AdaptiveWarmupState,
    /// Total deposits across all portfolios (used for warmup drain calculation)
    /// Updated on deposit/withdraw operations
    pub total_deposits: i128,
    /// Padding for alignment
    pub _padding3: [u8; 8],
}

impl SlabRegistry {
    pub const LEN: usize = core::mem::size_of::<Self>();

    /// Initialize registry in-place (avoids stack allocation)
    ///
    /// This method initializes the registry fields directly without creating
    /// a large temporary struct on the stack (which would exceed BPF's 4KB limit).
    pub fn initialize_in_place(&mut self, router_id: Pubkey, governance: Pubkey, insurance_authority: Pubkey, bump: u8) {
        self.router_id = router_id;
        self.governance = governance;
        self.insurance_authority = insurance_authority;
        self.bump = bump;
        self._padding = [0; 7];

        // Initialize liquidation parameters with defaults
        self.imr = 500;  // 5% initial margin
        self.mmr = 250;  // 2.5% maintenance margin
        self.liq_band_bps = 200;  // 2% liquidation band
        self.preliq_buffer = 10_000_000;  // $10 pre-liquidation buffer (1e6 scale)
        self.preliq_band_bps = 100;  // 1% pre-liquidation band (tighter)
        self.router_cap_per_slab = 1_000_000_000;  // 1000 units max per slab
        self.min_equity_to_quote = 100_000_000;  // $100 minimum equity
        self.oracle_tolerance_bps = 50;  // 0.5% oracle tolerance
        self.max_oracle_staleness_secs = 60;  // 60 seconds staleness threshold

        // Initialize insurance with defaults
        self.insurance_params = crate::state::insurance::InsuranceParams::default();
        self.insurance_state = crate::state::insurance::InsuranceState::default();

        // Initialize PnL vesting with defaults
        self.pnl_vesting_params = crate::state::pnl_vesting::PnlVestingParams::default();
        self.global_haircut = crate::state::pnl_vesting::GlobalHaircut::default();

        // Initialize adaptive warmup with defaults
        self.warmup_config = model_safety::adaptive_warmup::AdaptiveWarmupConfig::default();
        self.warmup_state = model_safety::adaptive_warmup::AdaptiveWarmupState::default();
        self.total_deposits = 0;
        self._padding3 = [0; 8];
    }

    /// Initialize new registry (for tests only - uses stack)
    /// Excluded from BPF builds to avoid stack overflow
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn new(router_id: Pubkey, governance: Pubkey, insurance_authority: Pubkey, bump: u8) -> Self {
        Self {
            router_id,
            governance,
            insurance_authority,
            bump,
            _padding: [0; 7],
            imr: 500,
            mmr: 250,
            liq_band_bps: 200,
            preliq_buffer: 10_000_000,
            preliq_band_bps: 100,
            router_cap_per_slab: 1_000_000_000,
            min_equity_to_quote: 100_000_000,
            oracle_tolerance_bps: 50,
            max_oracle_staleness_secs: 60,
            insurance_params: crate::state::insurance::InsuranceParams::default(),
            insurance_state: crate::state::insurance::InsuranceState::default(),
            pnl_vesting_params: crate::state::pnl_vesting::PnlVestingParams::default(),
            global_haircut: crate::state::pnl_vesting::GlobalHaircut::default(),
            warmup_config: model_safety::adaptive_warmup::AdaptiveWarmupConfig::default(),
            warmup_state: model_safety::adaptive_warmup::AdaptiveWarmupState::default(),
            total_deposits: 0,
            _padding3: [0; 8],
        }
    }

    /// Update global liquidation parameters (governance only)
    pub fn update_liquidation_params(
        &mut self,
        imr: u64,
        mmr: u64,
        liq_band_bps: u64,
        preliq_buffer: i128,
        preliq_band_bps: u64,
        router_cap_per_slab: u64,
        oracle_tolerance_bps: u64,
    ) {
        self.imr = imr;
        self.mmr = mmr;
        self.liq_band_bps = liq_band_bps;
        self.preliq_buffer = preliq_buffer;
        self.preliq_band_bps = preliq_band_bps;
        self.router_cap_per_slab = router_cap_per_slab;
        self.oracle_tolerance_bps = oracle_tolerance_bps;
    }

    /// Track deposit (increment total_deposits)
    pub fn track_deposit(&mut self, amount: i128) {
        self.total_deposits = self.total_deposits.saturating_add(amount);
    }

    /// Track withdrawal (decrement total_deposits)
    pub fn track_withdrawal(&mut self, amount: i128) {
        self.total_deposits = self.total_deposits.saturating_sub(amount);
    }

    /// Update adaptive warmup state using current total deposits
    ///
    /// Convenience method that uses the tracked total_deposits value.
    /// Call this periodically (e.g., once per slot on first user interaction).
    ///
    /// # Arguments
    /// * `oracle_spread_bps` - Current oracle spread in basis points
    /// * `insurance_utilization_bps` - Current insurance utilization in basis points (0-10000)
    pub fn update_warmup_from_current_state(
        &mut self,
        oracle_spread_bps: u64,
        insurance_utilization_bps: u64,
    ) {
        use model_safety::adaptive_warmup::q32;

        // Convert total deposits to Q32.32
        // Clamp to i64 range (should never overflow in practice - would require >9 trillion dollars)
        let total_deposits_i64: i64 = self.total_deposits.max(0)
            .try_into()
            .unwrap_or(i64::MAX);
        let total_deposits_q32 = q32(total_deposits_i64);

        // Check tripwires
        let oracle_gap_large = oracle_spread_bps > 50;
        let insurance_util_high = insurance_utilization_bps > 8000; // 80%

        // Update warmup state
        model_safety::adaptive_warmup::step(
            &mut self.warmup_state,
            &self.warmup_config,
            total_deposits_q32,
            oracle_gap_large,
            insurance_util_high,
        );
    }

    /// Update adaptive warmup state (called once per slot)
    ///
    /// Uses the formally verified adaptive_warmup::step() function to update
    /// the PnL unlock fraction based on deposit drain stress.
    ///
    /// # Arguments
    /// * `total_deposits_q32` - Total system deposits in Q32.32 fixed-point format
    /// * `oracle_gap_large` - True if oracle spread > threshold (e.g., 50 bps)
    /// * `insurance_util_high` - True if insurance utilization > 80%
    pub fn update_warmup_state(
        &mut self,
        total_deposits_q32: model_safety::adaptive_warmup::I,
        oracle_gap_large: bool,
        insurance_util_high: bool,
    ) {
        model_safety::adaptive_warmup::step(
            &mut self.warmup_state,
            &self.warmup_config,
            total_deposits_q32,
            oracle_gap_large,
            insurance_util_high,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_initialization() {
        let router_id = Pubkey::from([1; 32]);
        let governance = Pubkey::from([2; 32]);
        let insurance_authority = Pubkey::from([3; 32]);
        let registry = SlabRegistry::new(router_id, governance, insurance_authority, 42);

        // Verify basic fields
        assert_eq!(registry.router_id, router_id);
        assert_eq!(registry.governance, governance);
        assert_eq!(registry.bump, 42);

        // Verify default liquidation parameters
        assert_eq!(registry.imr, 500);  // 5%
        assert_eq!(registry.mmr, 250);  // 2.5%
        assert_eq!(registry.liq_band_bps, 200);  // 2%
        assert_eq!(registry.max_oracle_staleness_secs, 60);

        // Verify initial state
        assert_eq!(registry.total_deposits, 0);
    }

    #[test]
    fn test_registry_deposit_tracking() {
        let mut registry = SlabRegistry::new(Pubkey::default(), Pubkey::default(), Pubkey::default(), 0);

        // Track deposits
        registry.track_deposit(1_000_000);
        assert_eq!(registry.total_deposits, 1_000_000);

        registry.track_deposit(500_000);
        assert_eq!(registry.total_deposits, 1_500_000);

        // Track withdrawals
        registry.track_withdrawal(300_000);
        assert_eq!(registry.total_deposits, 1_200_000);
    }
}
