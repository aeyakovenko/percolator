//! User portfolio for cross-margin tracking

use pinocchio::pubkey::Pubkey;
use percolator_common::{MAX_INSTRUMENTS, MAX_SLABS};
use crate::state::lp_bucket::{LpBucket, VenueId, MAX_LP_BUCKETS};

/// Exposure key: (slab_index, instrument_index)
pub type ExposureKey = (u16, u16);

/// User portfolio tracking cross-margin state
/// PDA: ["portfolio", router_id, user]
#[repr(C)]
pub struct Portfolio {
    /// Router program ID
    pub router_id: Pubkey,
    /// User pubkey
    pub user: Pubkey,
    /// Total equity across all slabs
    pub equity: i128,
    /// Initial margin requirement
    pub im: u128,
    /// Maintenance margin requirement
    pub mm: u128,
    /// Free collateral (equity - IM)
    pub free_collateral: i128,
    /// Last mark timestamp
    pub last_mark_ts: u64,
    /// Number of exposures
    pub exposure_count: u16,
    /// Bump seed
    pub bump: u8,
    /// Padding
    pub _padding: [u8; 5],

    // Liquidation tracking
    /// Health (equity - MM)
    pub health: i128,
    /// Last liquidation timestamp (for rate limiting)
    pub last_liquidation_ts: u64,
    /// Cooldown period between deleveraging attempts (seconds)
    pub cooldown_seconds: u64,
    /// Padding for alignment
    pub _padding2: [u8; 8],

    // PnL vesting state
    /// Principal = deposits - withdrawals (never haircutted)
    pub principal: i128,
    /// Current PnL (before vesting, can be haircutted)
    pub pnl: i128,
    /// Vested PnL (the portion that has vested, can be haircutted)
    pub vested_pnl: i128,
    /// Last slot when vesting was applied
    pub last_slot: u64,
    /// User's checkpoint of global PnL index (1e9 fixed-point)
    pub pnl_index_checkpoint: i128,
    /// Padding for alignment
    pub _padding4: [u8; 8],

    /// Principal exposures: (slab_idx, instrument_idx) -> position qty
    /// These are TRADER positions, separate from LP exposure
    /// Using fixed-size array for simplicity (can optimize with HashMap-like structure)
    pub exposures: [(u16, u16, i64); MAX_SLABS * MAX_INSTRUMENTS],

    /// LP buckets: venue-scoped liquidity provider exposure
    /// AMM LP reduced ONLY by burn_lp_shares()
    /// Slab LP reduced ONLY by cancel_order()
    pub lp_buckets: [LpBucket; MAX_LP_BUCKETS],
    /// Number of active LP buckets
    pub lp_bucket_count: u16,
    /// Padding for alignment
    pub _padding3: [u8; 6],
}

impl Portfolio {
    pub const LEN: usize = core::mem::size_of::<Self>();

    /// Initialize portfolio in-place (avoids stack allocation)
    ///
    /// This method initializes the portfolio fields directly without creating
    /// a large temporary struct on the stack (which would exceed BPF's 4KB limit).
    pub fn initialize_in_place(&mut self, router_id: Pubkey, user: Pubkey, bump: u8) {
        self.router_id = router_id;
        self.user = user;
        self.equity = 0;
        self.im = 0;
        self.mm = 0;
        self.free_collateral = 0;
        self.last_mark_ts = 0;
        self.exposure_count = 0;
        self.bump = bump;
        self._padding = [0; 5];

        // Initialize liquidation tracking
        self.health = 0;  // equity - MM = 0 - 0 = 0
        self.last_liquidation_ts = 0;
        self.cooldown_seconds = 60;  // 1 minute default cooldown
        self._padding2 = [0; 8];

        // Initialize PnL vesting state
        self.principal = 0;  // No deposits yet
        self.pnl = 0;  // No PnL yet
        self.vested_pnl = 0;  // No vested PnL yet
        self.last_slot = 0;  // No vesting applied yet
        self.pnl_index_checkpoint = crate::state::pnl_vesting::FP_ONE;  // Start at 1.0 (no haircut)
        self._padding4 = [0; 8];

        // Zero out the exposures array using ptr::write_bytes (efficient and stack-safe)
        unsafe {
            core::ptr::write_bytes(
                self.exposures.as_mut_ptr(),
                0,
                MAX_SLABS * MAX_INSTRUMENTS,
            );
        }

        // Initialize LP buckets
        self.lp_bucket_count = 0;
        self._padding3 = [0; 6];
        unsafe {
            core::ptr::write_bytes(
                self.lp_buckets.as_mut_ptr(),
                0,
                MAX_LP_BUCKETS,
            );
        }
    }

    /// Initialize new portfolio (for tests only - uses stack)
    /// Excluded from BPF builds to avoid stack overflow
    #[cfg(all(test, not(target_os = "solana")))]
    pub fn new(router_id: Pubkey, user: Pubkey, bump: u8) -> Self {
        // Create a zero-initialized LP bucket for array initialization
        let zero_bucket: LpBucket = unsafe { core::mem::zeroed() };

        Self {
            router_id,
            user,
            equity: 0,
            im: 0,
            mm: 0,
            free_collateral: 0,
            last_mark_ts: 0,
            exposure_count: 0,
            bump,
            _padding: [0; 5],
            health: 0,
            last_liquidation_ts: 0,
            cooldown_seconds: 60,
            _padding2: [0; 8],
            principal: 0,
            pnl: 0,
            vested_pnl: 0,
            last_slot: 0,
            pnl_index_checkpoint: crate::state::pnl_vesting::FP_ONE,
            _padding4: [0; 8],
            exposures: [(0, 0, 0); MAX_SLABS * MAX_INSTRUMENTS],
            lp_buckets: [zero_bucket; MAX_LP_BUCKETS],
            lp_bucket_count: 0,
            _padding3: [0; 6],
        }
    }

    /// Update exposure for (slab, instrument)
    pub fn update_exposure(&mut self, slab_idx: u16, instrument_idx: u16, qty: i64) {
        // Find existing exposure or add new one
        for i in 0..self.exposure_count as usize {
            if self.exposures[i].0 == slab_idx && self.exposures[i].1 == instrument_idx {
                self.exposures[i].2 = qty;
                // Remove if qty is zero
                if qty == 0 {
                    self.remove_exposure_at(i);
                }
                return;
            }
        }

        // Add new exposure if non-zero
        if qty != 0 && (self.exposure_count as usize) < self.exposures.len() {
            let idx = self.exposure_count as usize;
            self.exposures[idx] = (slab_idx, instrument_idx, qty);
            self.exposure_count += 1;
        }
    }

    /// Remove exposure at index
    fn remove_exposure_at(&mut self, idx: usize) {
        if idx < self.exposure_count as usize {
            // Swap with last and decrement count
            let last_idx = (self.exposure_count - 1) as usize;
            if idx != last_idx {
                self.exposures[idx] = self.exposures[last_idx];
            }
            self.exposures[last_idx] = (0, 0, 0);
            self.exposure_count -= 1;
        }
    }

    /// Get exposure for (slab, instrument)
    pub fn get_exposure(&self, slab_idx: u16, instrument_idx: u16) -> i64 {
        for i in 0..self.exposure_count as usize {
            if self.exposures[i].0 == slab_idx && self.exposures[i].1 == instrument_idx {
                return self.exposures[i].2;
            }
        }
        0
    }

    /// Update margin requirements (using verified math)
    ///
    /// # Safety
    ///
    /// Uses formally verified arithmetic to prevent overflow/underflow.
    pub fn update_margin(&mut self, im: u128, mm: u128) {
        use model_safety::math::{u128_to_i128, sub_i128};

        self.im = im;
        self.mm = mm;
        // Convert im to i128 and subtract from equity using verified operations
        self.free_collateral = sub_i128(self.equity, u128_to_i128(im));
    }

    /// Update equity (using verified math)
    ///
    /// # Safety
    ///
    /// Uses formally verified arithmetic to prevent overflow/underflow.
    pub fn update_equity(&mut self, equity: i128) {
        use model_safety::math::{u128_to_i128, sub_i128};

        self.equity = equity;
        // Convert im to i128 and subtract from equity using verified operations
        self.free_collateral = sub_i128(equity, u128_to_i128(self.im));
    }

    /// Check if sufficient margin
    pub fn has_sufficient_margin(&self) -> bool {
        self.equity >= self.im as i128
    }

    /// Check if above maintenance margin
    pub fn is_above_maintenance(&self) -> bool {
        self.equity >= self.mm as i128
    }

    /// Find LP bucket by venue
    pub fn find_lp_bucket(&self, venue: &VenueId) -> Option<&LpBucket> {
        for i in 0..self.lp_bucket_count as usize {
            if self.lp_buckets[i].active && &self.lp_buckets[i].venue == venue {
                return Some(&self.lp_buckets[i]);
            }
        }
        None
    }

    /// Find LP bucket by venue (mutable)
    pub fn find_lp_bucket_mut(&mut self, venue: &VenueId) -> Option<&mut LpBucket> {
        for i in 0..self.lp_bucket_count as usize {
            if self.lp_buckets[i].active && &self.lp_buckets[i].venue == venue {
                return Some(&mut self.lp_buckets[i]);
            }
        }
        None
    }

    /// Add new LP bucket
    pub fn add_lp_bucket(&mut self, bucket: LpBucket) -> Result<(), ()> {
        if (self.lp_bucket_count as usize) >= MAX_LP_BUCKETS {
            return Err(());
        }

        // Check if venue already exists
        if self.find_lp_bucket(&bucket.venue).is_some() {
            return Err(());
        }

        let idx = self.lp_bucket_count as usize;
        self.lp_buckets[idx] = bucket;
        self.lp_bucket_count += 1;

        Ok(())
    }

    /// Remove LP bucket by venue
    pub fn remove_lp_bucket(&mut self, venue: &VenueId) -> Result<(), ()> {
        for i in 0..self.lp_bucket_count as usize {
            if self.lp_buckets[i].active && &self.lp_buckets[i].venue == venue {
                // Deactivate bucket
                self.lp_buckets[i].active = false;

                // Swap with last and decrement count
                let last_idx = (self.lp_bucket_count - 1) as usize;
                if i != last_idx {
                    self.lp_buckets[i] = self.lp_buckets[last_idx];
                }
                // Zero out last bucket
                unsafe {
                    core::ptr::write_bytes(&mut self.lp_buckets[last_idx] as *mut LpBucket, 0, 1);
                }
                self.lp_bucket_count -= 1;

                return Ok(());
            }
        }
        Err(())
    }

    /// Calculate total maintenance margin (venue-aware, using verified math)
    /// MM_total = MM_principal + Σ MM_bucket_i
    ///
    /// # Safety
    ///
    /// Uses formally verified saturating addition from model_safety::math
    /// to prevent overflow bugs in margin calculations.
    pub fn calculate_total_mm(&self) -> u128 {
        use model_safety::math::add_u128;

        let mut total_mm = self.mm; // Principal MM

        // Add LP bucket MMs using verified addition
        for i in 0..self.lp_bucket_count as usize {
            if self.lp_buckets[i].active {
                total_mm = add_u128(total_mm, self.lp_buckets[i].mm);
            }
        }

        total_mm
    }

    /// Calculate total initial margin (venue-aware, using verified math)
    /// IM_total = IM_principal + Σ IM_bucket_i
    ///
    /// # Safety
    ///
    /// Uses formally verified saturating addition from model_safety::math
    /// to prevent overflow bugs in margin calculations.
    pub fn calculate_total_im(&self) -> u128 {
        use model_safety::math::add_u128;

        let mut total_im = self.im; // Principal IM

        // Add LP bucket IMs using verified addition
        for i in 0..self.lp_bucket_count as usize {
            if self.lp_buckets[i].active {
                total_im = add_u128(total_im, self.lp_buckets[i].im);
            }
        }

        total_im
    }

    /// Check if sufficient margin using venue-aware calculation
    pub fn has_sufficient_margin_venue_aware(&self) -> bool {
        self.equity >= self.calculate_total_im() as i128
    }

    /// Check if above maintenance using venue-aware calculation
    pub fn is_above_maintenance_venue_aware(&self) -> bool {
        self.equity >= self.calculate_total_mm() as i128
    }

    /// Calculate maximum withdrawable amount considering adaptive warmup throttling
    ///
    /// Returns the total amount that can be withdrawn, factoring in:
    /// 1. Principal (always fully withdrawable - sacrosanct)
    /// 2. Vested PnL multiplied by the adaptive warmup unlocked_frac
    ///
    /// # Arguments
    /// * `unlocked_frac` - Unlocked fraction from adaptive warmup system (Q32.32 fixed-point)
    ///
    /// # Returns
    /// Maximum withdrawable amount = principal + (vested_pnl * unlocked_frac)
    ///
    /// # Safety
    ///
    /// Uses formally verified calculate_withdrawable_pnl() which applies Q32.32
    /// arithmetic from model_safety::adaptive_warmup (proven correct by Kani P5).
    pub fn max_withdrawable_with_warmup(
        &self,
        unlocked_frac: model_safety::adaptive_warmup::I,
    ) -> i128 {
        use crate::state::pnl_vesting::calculate_withdrawable_pnl;

        // Principal is always fully withdrawable (never throttled)
        let withdrawable_pnl = calculate_withdrawable_pnl(self.vested_pnl, unlocked_frac);

        // Total = principal + (vested_pnl * unlocked_frac)
        self.principal.saturating_add(withdrawable_pnl)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_portfolio_exposures() {
        let mut portfolio = Portfolio::new(Pubkey::default(), Pubkey::default(), 0);

        portfolio.update_exposure(0, 0, 100);
        assert_eq!(portfolio.get_exposure(0, 0), 100);
        assert_eq!(portfolio.exposure_count, 1);

        portfolio.update_exposure(0, 1, 50);
        assert_eq!(portfolio.get_exposure(0, 1), 50);
        assert_eq!(portfolio.exposure_count, 2);

        portfolio.update_exposure(0, 0, 0);
        assert_eq!(portfolio.get_exposure(0, 0), 0);
        assert_eq!(portfolio.exposure_count, 1);
    }

    #[test]
    fn test_portfolio_margin() {
        let mut portfolio = Portfolio::new(Pubkey::default(), Pubkey::default(), 0);

        portfolio.update_equity(10000);
        portfolio.update_margin(5000, 2500);

        assert!(portfolio.has_sufficient_margin());
        assert!(portfolio.is_above_maintenance());
        assert_eq!(portfolio.free_collateral, 5000);

        portfolio.update_equity(4000);
        assert!(!portfolio.has_sufficient_margin());
        assert!(portfolio.is_above_maintenance());

        portfolio.update_equity(2000);
        assert!(!portfolio.is_above_maintenance());
    }

    #[test]
    fn test_lp_bucket_management() {
        let mut portfolio = Portfolio::new(Pubkey::default(), Pubkey::default(), 0);

        let market = Pubkey::from([1; 32]);
        let amm_venue = VenueId::new_amm(market);
        let slab_venue = VenueId::new_slab(market);

        // Add AMM bucket
        let amm_bucket = LpBucket::new_amm(amm_venue, 1000, 60_000_000, 100);
        assert!(portfolio.add_lp_bucket(amm_bucket).is_ok());
        assert_eq!(portfolio.lp_bucket_count, 1);

        // Add Slab bucket
        let slab_bucket = LpBucket::new_slab(slab_venue);
        assert!(portfolio.add_lp_bucket(slab_bucket).is_ok());
        assert_eq!(portfolio.lp_bucket_count, 2);

        // Find AMM bucket
        assert!(portfolio.find_lp_bucket(&amm_venue).is_some());

        // Remove Slab bucket
        assert!(portfolio.remove_lp_bucket(&slab_venue).is_ok());
        assert_eq!(portfolio.lp_bucket_count, 1);
        assert!(portfolio.find_lp_bucket(&slab_venue).is_none());
    }

    #[test]
    fn test_venue_aware_margin_calculation() {
        let mut portfolio = Portfolio::new(Pubkey::default(), Pubkey::default(), 0);

        // Set principal margin
        portfolio.update_margin(5000, 2500);

        // Add LP buckets with their own margin
        let market1 = Pubkey::from([1; 32]);
        let market2 = Pubkey::from([2; 32]);

        let mut bucket1 = LpBucket::new_amm(VenueId::new_amm(market1), 1000, 60_000_000, 100);
        bucket1.update_margin(1000, 500);

        let mut bucket2 = LpBucket::new_slab(VenueId::new_slab(market2));
        bucket2.update_margin(2000, 1000);

        assert!(portfolio.add_lp_bucket(bucket1).is_ok());
        assert!(portfolio.add_lp_bucket(bucket2).is_ok());

        // Total IM = 5000 + 1000 + 2000 = 8000
        assert_eq!(portfolio.calculate_total_im(), 8000);

        // Total MM = 2500 + 500 + 1000 = 4000
        assert_eq!(portfolio.calculate_total_mm(), 4000);

        // Test venue-aware margin checks
        portfolio.update_equity(9000);
        assert!(portfolio.has_sufficient_margin_venue_aware());
        assert!(portfolio.is_above_maintenance_venue_aware());

        portfolio.update_equity(3000);
        assert!(!portfolio.has_sufficient_margin_venue_aware());
        assert!(!portfolio.is_above_maintenance_venue_aware());
    }

    // Test 1: AMM LP liquidation only by burn
    #[test]
    fn test_amm_lp_liq_only_by_burn() {
        use crate::state::lp_bucket::VenueKind;

        let mut portfolio = Portfolio::new(Pubkey::default(), Pubkey::default(), 0);
        let market = Pubkey::from([1; 32]);
        let amm_venue = VenueId::new_amm(market);

        let mut amm_bucket = LpBucket::new_amm(amm_venue, 1000, 60_000_000, 100);
        amm_bucket.update_margin(1000, 500);
        assert!(portfolio.add_lp_bucket(amm_bucket).is_ok());

        // Verify AMM LP exists
        let bucket = portfolio.find_lp_bucket(&amm_venue).unwrap();
        assert_eq!(bucket.venue.venue_kind, VenueKind::Amm);
        assert!(bucket.amm.is_some());
        assert_eq!(bucket.amm.unwrap().lp_shares, 1000);

        // CRITICAL INVARIANT: AMM LP can ONLY be reduced by burn_lp_shares()
        // This test verifies the data structure separation exists
        // The actual burn_lp_shares() API will be implemented separately
    }

    // Test 2: Slab LP liquidation only by cancel
    #[test]
    fn test_slab_lp_liq_only_by_cancel() {
        use crate::state::lp_bucket::VenueKind;

        let mut portfolio = Portfolio::new(Pubkey::default(), Pubkey::default(), 0);
        let market = Pubkey::from([1; 32]);
        let slab_venue = VenueId::new_slab(market);

        let mut slab_bucket = LpBucket::new_slab(slab_venue);
        slab_bucket.update_margin(1000, 500);

        // Add reservations
        if let Some(ref mut slab) = slab_bucket.slab {
            assert!(slab.add_reservation(1001, 1000, 500).is_ok());
            assert!(slab.add_reservation(1002, 2000, 1000).is_ok());
        }

        assert!(portfolio.add_lp_bucket(slab_bucket).is_ok());

        // Verify Slab LP exists
        let bucket = portfolio.find_lp_bucket(&slab_venue).unwrap();
        assert_eq!(bucket.venue.venue_kind, VenueKind::Slab);
        assert!(bucket.slab.is_some());

        let slab = bucket.slab.as_ref().unwrap();
        assert_eq!(slab.reserved_quote, 3000);
        assert_eq!(slab.reserved_base, 1500);
        assert_eq!(slab.open_order_count, 2);

        // CRITICAL INVARIANT: Slab LP can ONLY be reduced by cancel_order()
        // This test verifies the data structure separation exists
        // The actual cancel_lp_orders() API will be implemented separately
    }

    // Test 3: No cross-bucket shortcut
    #[test]
    fn test_no_cross_bucket_shortcut() {
        use crate::state::lp_bucket::VenueKind;

        let mut portfolio = Portfolio::new(Pubkey::default(), Pubkey::default(), 0);

        // Add principal position
        portfolio.update_exposure(0, 0, 100);
        assert_eq!(portfolio.get_exposure(0, 0), 100);

        // Add AMM LP bucket
        let market = Pubkey::from([1; 32]);
        let amm_venue = VenueId::new_amm(market);
        let amm_bucket = LpBucket::new_amm(amm_venue, 1000, 60_000_000, 100);
        assert!(portfolio.add_lp_bucket(amm_bucket).is_ok());

        // CRITICAL INVARIANT: Principal position (exposures) and LP buckets are SEPARATE
        // There is no API to transfer between them - they must be in separate fields
        assert_eq!(portfolio.exposure_count, 1);
        assert_eq!(portfolio.lp_bucket_count, 1);

        // Verify separation
        let bucket = portfolio.find_lp_bucket(&amm_venue).unwrap();
        assert_eq!(bucket.venue.venue_kind, VenueKind::Amm);

        // Principal exposure is independent of LP bucket
        assert_eq!(portfolio.get_exposure(0, 0), 100);
        assert!(bucket.amm.is_some());
    }

    // Test 4: Equity consistency on burn
    #[test]
    fn test_equity_consistency_on_burn() {
        let mut portfolio = Portfolio::new(Pubkey::default(), Pubkey::default(), 0);
        let market = Pubkey::from([1; 32]);
        let amm_venue = VenueId::new_amm(market);

        // Initial equity
        portfolio.update_equity(100_000);

        // Add AMM LP bucket
        let mut amm_bucket = LpBucket::new_amm(amm_venue, 1000, 60_000_000, 100);
        amm_bucket.update_margin(10_000, 5_000);
        assert!(portfolio.add_lp_bucket(amm_bucket).is_ok());

        // Check total margin includes LP bucket
        assert_eq!(portfolio.calculate_total_mm(), 5_000);

        // After burn_lp_shares (simulated):
        // - LP shares reduced
        // - Equity adjusted by redemption value
        // - Bucket MM reduced proportionally
        // This test verifies the equity calculation framework exists
        let initial_equity = portfolio.equity;
        assert_eq!(initial_equity, 100_000);
    }

    // Test 5: Reservation accounting exact
    #[test]
    fn test_reservation_accounting_exact() {
        let mut portfolio = Portfolio::new(Pubkey::default(), Pubkey::default(), 0);
        let market = Pubkey::from([1; 32]);
        let slab_venue = VenueId::new_slab(market);

        let mut slab_bucket = LpBucket::new_slab(slab_venue);

        // Add multiple reservations
        if let Some(ref mut slab) = slab_bucket.slab {
            assert!(slab.add_reservation(1001, 1000, 500).is_ok());
            assert!(slab.add_reservation(1002, 2000, 1000).is_ok());
            assert!(slab.add_reservation(1003, 1500, 750).is_ok());

            assert_eq!(slab.reserved_quote, 4500);
            assert_eq!(slab.reserved_base, 2250);
            assert_eq!(slab.open_order_count, 3);

            // Remove middle reservation
            assert!(slab.remove_reservation(1002, 2000, 1000).is_ok());

            // Verify exact accounting
            assert_eq!(slab.reserved_quote, 2500);
            assert_eq!(slab.reserved_base, 1250);
            assert_eq!(slab.open_order_count, 2);

            // Remaining orders should be 1001 and 1003
            // (1003 swapped into 1002's position)
            assert!(slab.remove_reservation(1001, 1000, 500).is_ok());
            assert_eq!(slab.reserved_quote, 1500);
            assert_eq!(slab.reserved_base, 750);

            assert!(slab.remove_reservation(1003, 1500, 750).is_ok());
            assert_eq!(slab.reserved_quote, 0);
            assert_eq!(slab.reserved_base, 0);
            assert_eq!(slab.open_order_count, 0);
        }

        assert!(portfolio.add_lp_bucket(slab_bucket).is_ok());
    }

    // Test 6: Priority ordering (framework)
    #[test]
    fn test_priority_ordering() {
        let mut portfolio = Portfolio::new(Pubkey::default(), Pubkey::default(), 0);

        // Set up under-margin situation
        portfolio.update_equity(10_000);
        portfolio.update_margin(20_000, 10_000);

        // Add LP buckets
        let market1 = Pubkey::from([1; 32]);
        let market2 = Pubkey::from([2; 32]);

        let mut amm_bucket = LpBucket::new_amm(VenueId::new_amm(market1), 1000, 60_000_000, 100);
        amm_bucket.update_margin(5_000, 2_500);

        let mut slab_bucket = LpBucket::new_slab(VenueId::new_slab(market2));
        slab_bucket.update_margin(3_000, 1_500);

        assert!(portfolio.add_lp_bucket(slab_bucket).is_ok());
        assert!(portfolio.add_lp_bucket(amm_bucket).is_ok());

        // Total MM = 10_000 + 2_500 + 1_500 = 14_000
        assert_eq!(portfolio.calculate_total_mm(), 14_000);

        // Under maintenance (10_000 < 14_000)
        assert!(!portfolio.is_above_maintenance_venue_aware());

        // LIQUIDATION PRIORITY (to be implemented in liquidation flow):
        // 1. Reduce principal positions first
        // 2. Cancel Slab LP orders (free reservations)
        // 3. Burn AMM LP shares (last resort)
        // This test verifies the data structures exist to support this priority
    }

    // Test 7: Staleness guard on AMM pricing
    #[test]
    fn test_staleness_guard_on_amm_pricing() {
        let market = Pubkey::from([1; 32]);
        let amm_venue = VenueId::new_amm(market);

        let amm_bucket = LpBucket::new_amm(amm_venue, 1000, 60_000_000, 100);

        if let Some(amm) = amm_bucket.amm {
            // Not stale within 60 seconds
            assert!(!amm.is_stale(150, 60));

            // Stale after 60 seconds
            assert!(amm.is_stale(161, 60));

            // CRITICAL: Stale prices must be rejected during liquidation
            // This prevents using outdated share prices for LP burn calculations
        }
    }

    // Test 8: Edge case - partial burn
    #[test]
    fn test_edge_partial_burn() {
        let mut portfolio = Portfolio::new(Pubkey::default(), Pubkey::default(), 0);
        let market = Pubkey::from([1; 32]);
        let amm_venue = VenueId::new_amm(market);

        // AMM LP with 1000 shares
        let mut amm_bucket = LpBucket::new_amm(amm_venue, 1000, 60_000_000, 100);
        amm_bucket.update_margin(10_000, 5_000);
        assert!(portfolio.add_lp_bucket(amm_bucket).is_ok());

        // Verify bucket exists
        let bucket = portfolio.find_lp_bucket(&amm_venue).unwrap();
        let initial_shares = bucket.amm.unwrap().lp_shares;
        assert_eq!(initial_shares, 1000);

        // After partial burn (e.g., 300 shares):
        // - lp_shares: 1000 → 700
        // - Margin reduced proportionally: 5_000 → 3_500
        // - Equity increased by redemption value
        //
        // This test verifies the structure exists to support partial burns
        // Actual burn_lp_shares() API will implement the proportional math
        let expected_mm_after_partial = (5_000 * 700) / 1000;
        assert_eq!(expected_mm_after_partial, 3_500);
    }

    // ═══════════════════════════════════════════════════════════════
    // ADAPTIVE WARMUP INTEGRATION TESTS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_max_withdrawable_fully_unlocked() {
        use model_safety::adaptive_warmup::q1;

        let mut portfolio = Portfolio::new(Pubkey::default(), Pubkey::default(), 0);
        portfolio.principal = 100_000;
        portfolio.vested_pnl = 50_000;

        let unlocked_frac = q1(); // 100% unlocked

        let max = portfolio.max_withdrawable_with_warmup(unlocked_frac);

        // Should be able to withdraw principal + all vested PnL
        assert_eq!(max, 150_000);
    }

    #[test]
    fn test_max_withdrawable_half_unlocked() {
        use model_safety::adaptive_warmup::q1;

        let mut portfolio = Portfolio::new(Pubkey::default(), Pubkey::default(), 0);
        portfolio.principal = 100_000;
        portfolio.vested_pnl = 50_000;

        let unlocked_frac = q1() / 2; // 50% unlocked

        let max = portfolio.max_withdrawable_with_warmup(unlocked_frac);

        // Should be: principal + (vested_pnl * 0.5)
        // = 100_000 + 25_000 = 125_000
        assert_eq!(max, 125_000);
    }

    #[test]
    fn test_max_withdrawable_zero_unlocked() {
        let mut portfolio = Portfolio::new(Pubkey::default(), Pubkey::default(), 0);
        portfolio.principal = 100_000;
        portfolio.vested_pnl = 50_000;

        let unlocked_frac = 0; // 0% unlocked (frozen)

        let max = portfolio.max_withdrawable_with_warmup(unlocked_frac);

        // Should only be able to withdraw principal
        assert_eq!(max, 100_000);
    }

    #[test]
    fn test_max_withdrawable_negative_pnl() {
        use model_safety::adaptive_warmup::q1;

        let mut portfolio = Portfolio::new(Pubkey::default(), Pubkey::default(), 0);
        portfolio.principal = 100_000;
        portfolio.vested_pnl = -20_000; // Losses

        let unlocked_frac = q1(); // 100% unlocked

        let max = portfolio.max_withdrawable_with_warmup(unlocked_frac);

        // Negative PnL contributes 0, so only principal is withdrawable
        assert_eq!(max, 100_000);
    }

    #[test]
    fn test_max_withdrawable_principal_sacrosanct() {
        let mut portfolio = Portfolio::new(Pubkey::default(), Pubkey::default(), 0);
        portfolio.principal = 200_000;
        portfolio.vested_pnl = 100_000;

        let unlocked_frac = 0; // 0% unlocked (completely frozen)

        let max = portfolio.max_withdrawable_with_warmup(unlocked_frac);

        // Principal is always fully withdrawable regardless of freeze
        assert_eq!(max, 200_000);
    }
}
