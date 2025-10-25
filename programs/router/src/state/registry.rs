//! Slab registry for governance and validation

use pinocchio::pubkey::Pubkey;
use percolator_common::MAX_SLABS;

/// Slab registration entry
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SlabEntry {
    /// Slab program ID
    pub slab_id: Pubkey,
    /// Version hash (for upgrade validation)
    pub version_hash: [u8; 32],
    /// Oracle program ID for price feeds
    pub oracle_id: Pubkey,
    /// Initial margin ratio (basis points)
    pub imr: u64,
    /// Maintenance margin ratio (basis points)
    pub mmr: u64,
    /// Maximum maker fee (basis points)
    pub maker_fee_cap: u64,
    /// Maximum taker fee (basis points)
    pub taker_fee_cap: u64,
    /// Latency SLA (milliseconds)
    pub latency_sla_ms: u64,
    /// Maximum exposure per user (per instrument)
    pub max_exposure: u128,
    /// Registered timestamp
    pub registered_ts: u64,
    /// Active flag
    pub active: bool,
    /// Padding
    pub _padding: [u8; 7],
}

/// Slab registry account
/// PDA: ["registry", router_id]
#[repr(C)]
pub struct SlabRegistry {
    /// Router program ID
    pub router_id: Pubkey,
    /// Governance authority (can update registry)
    pub governance: Pubkey,
    /// Number of registered slabs
    pub slab_count: u16,
    /// Bump seed
    pub bump: u8,
    /// Padding
    pub _padding: [u8; 5],

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
    /// Padding for alignment
    pub _padding2: [u8; 8],

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

    /// Registered slabs
    pub slabs: [SlabEntry; MAX_SLABS],
}

impl SlabRegistry {
    pub const LEN: usize = core::mem::size_of::<Self>();

    /// Initialize registry in-place (avoids stack allocation)
    ///
    /// This method initializes the registry fields directly without creating
    /// a large temporary struct on the stack (which would exceed BPF's 4KB limit).
    pub fn initialize_in_place(&mut self, router_id: Pubkey, governance: Pubkey, bump: u8) {
        self.router_id = router_id;
        self.governance = governance;
        self.slab_count = 0;
        self.bump = bump;
        self._padding = [0; 5];

        // Initialize liquidation parameters with defaults
        self.imr = 500;  // 5% initial margin
        self.mmr = 250;  // 2.5% maintenance margin
        self.liq_band_bps = 200;  // 2% liquidation band
        self.preliq_buffer = 10_000_000;  // $10 pre-liquidation buffer (1e6 scale)
        self.preliq_band_bps = 100;  // 1% pre-liquidation band (tighter)
        self.router_cap_per_slab = 1_000_000_000;  // 1000 units max per slab
        self.min_equity_to_quote = 100_000_000;  // $100 minimum equity
        self.oracle_tolerance_bps = 50;  // 0.5% oracle tolerance
        self._padding2 = [0; 8];

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

        // Zero out the slabs array using ptr::write_bytes (efficient and stack-safe)
        unsafe {
            core::ptr::write_bytes(
                self.slabs.as_mut_ptr(),
                0,
                MAX_SLABS,
            );
        }
    }

    /// Initialize new registry (for tests only - uses stack)
    /// Excluded from BPF builds to avoid stack overflow
    #[cfg(all(test, not(target_os = "solana")))]
    pub fn new(router_id: Pubkey, governance: Pubkey, bump: u8) -> Self {
        Self {
            router_id,
            governance,
            slab_count: 0,
            bump,
            _padding: [0; 5],
            imr: 500,
            mmr: 250,
            liq_band_bps: 200,
            preliq_buffer: 10_000_000,
            preliq_band_bps: 100,
            router_cap_per_slab: 1_000_000_000,
            min_equity_to_quote: 100_000_000,
            oracle_tolerance_bps: 50,
            _padding2: [0; 8],
            insurance_params: crate::state::insurance::InsuranceParams::default(),
            insurance_state: crate::state::insurance::InsuranceState::default(),
            pnl_vesting_params: crate::state::pnl_vesting::PnlVestingParams::default(),
            global_haircut: crate::state::pnl_vesting::GlobalHaircut::default(),
            warmup_config: model_safety::adaptive_warmup::AdaptiveWarmupConfig::default(),
            warmup_state: model_safety::adaptive_warmup::AdaptiveWarmupState::default(),
            total_deposits: 0,
            _padding3: [0; 8],
            slabs: [SlabEntry {
                slab_id: Pubkey::default(),
                version_hash: [0; 32],
                oracle_id: Pubkey::default(),
                imr: 0,
                mmr: 0,
                maker_fee_cap: 0,
                taker_fee_cap: 0,
                latency_sla_ms: 0,
                max_exposure: 0,
                registered_ts: 0,
                active: false,
                _padding: [0; 7],
            }; MAX_SLABS],
        }
    }

    /// Register a new slab
    pub fn register_slab(
        &mut self,
        slab_id: Pubkey,
        version_hash: [u8; 32],
        oracle_id: Pubkey,
        imr: u64,
        mmr: u64,
        maker_fee_cap: u64,
        taker_fee_cap: u64,
        latency_sla_ms: u64,
        max_exposure: u128,
        current_ts: u64,
    ) -> Result<u16, ()> {
        if (self.slab_count as usize) >= MAX_SLABS {
            return Err(());
        }

        let idx = self.slab_count;
        self.slabs[idx as usize] = SlabEntry {
            slab_id,
            version_hash,
            oracle_id,
            imr,
            mmr,
            maker_fee_cap,
            taker_fee_cap,
            latency_sla_ms,
            max_exposure,
            registered_ts: current_ts,
            active: true,
            _padding: [0; 7],
        };
        self.slab_count += 1;

        Ok(idx)
    }

    /// Find slab by ID
    pub fn find_slab(&self, slab_id: &Pubkey) -> Option<(u16, &SlabEntry)> {
        for i in 0..self.slab_count as usize {
            if &self.slabs[i].slab_id == slab_id && self.slabs[i].active {
                return Some((i as u16, &self.slabs[i]));
            }
        }
        None
    }

    /// Validate slab version hash
    pub fn validate_version(&self, slab_id: &Pubkey, version_hash: &[u8; 32]) -> bool {
        if let Some((_, entry)) = self.find_slab(slab_id) {
            &entry.version_hash == version_hash
        } else {
            false
        }
    }

    /// Deactivate a slab
    pub fn deactivate_slab(&mut self, slab_id: &Pubkey) -> Result<(), ()> {
        if let Some((idx, _)) = self.find_slab(slab_id) {
            self.slabs[idx as usize].active = false;
            Ok(())
        } else {
            Err(())
        }
    }

    /// Update slab risk params
    pub fn update_risk_params(&mut self, slab_id: &Pubkey, imr: u64, mmr: u64) -> Result<(), ()> {
        if let Some((idx, _)) = self.find_slab(slab_id) {
            self.slabs[idx as usize].imr = imr;
            self.slabs[idx as usize].mmr = mmr;
            Ok(())
        } else {
            Err(())
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
    fn test_registry_operations() {
        let mut registry = SlabRegistry::new(Pubkey::default(), Pubkey::default(), 0);

        let slab_id = Pubkey::from([1; 32]);
        let version_hash = [42; 32];

        let idx = registry
            .register_slab(
                slab_id,
                version_hash,
                Pubkey::default(),
                500,  // 5% IMR
                250,  // 2.5% MMR
                10,   // 0.1% maker fee cap
                20,   // 0.2% taker fee cap
                1000, // 1s latency SLA
                1_000_000,
                12345,
            )
            .unwrap();

        assert_eq!(idx, 0);
        assert_eq!(registry.slab_count, 1);

        let (found_idx, entry) = registry.find_slab(&slab_id).unwrap();
        assert_eq!(found_idx, 0);
        assert_eq!(entry.imr, 500);

        assert!(registry.validate_version(&slab_id, &version_hash));
        assert!(!registry.validate_version(&slab_id, &[0; 32]));

        registry.deactivate_slab(&slab_id).unwrap();
        assert!(registry.find_slab(&slab_id).is_none());
    }
}
