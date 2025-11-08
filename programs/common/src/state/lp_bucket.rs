//! LP bucket structures for venue-scoped risk management
//!
//! This module implements strict separation between principal trading positions
//! and LP (liquidity provider) exposure across different venues (Slab orderbooks, AMM pools).
//!
//! Key invariants:
//! - Principal positions are NEVER reduced by LP operations
//! - AMM LP exposure is reduced ONLY by burn_lp_shares()
//! - Slab LP exposure is reduced ONLY by cancel_order()
//! - Cross-bucket transfers are FORBIDDEN

use pinocchio::pubkey::Pubkey;

/// Maximum number of LP buckets per portfolio
pub const MAX_LP_BUCKETS: usize = 16;

/// Maximum number of open orders per Slab LP bucket
pub const MAX_OPEN_ORDERS: usize = 8;

/// Venue type identifier
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VenueKind {
    /// Orderbook slab
    Slab = 0,
    /// AMM pool
    Amm = 1,
}

/// Venue identifier: (market_id, venue_kind)
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VenueId {
    /// Market pubkey (slab or AMM)
    pub market_id: Pubkey,
    /// Venue type
    pub venue_kind: VenueKind,
    /// Padding for alignment
    pub _padding: [u8; 7],
}

impl VenueId {
    pub fn new_slab(market_id: Pubkey) -> Self {
        Self {
            market_id,
            venue_kind: VenueKind::Slab,
            _padding: [0; 7],
        }
    }

    pub fn new_amm(market_id: Pubkey) -> Self {
        Self {
            market_id,
            venue_kind: VenueKind::Amm,
            _padding: [0; 7],
        }
    }
}

/// AMM LP share tracking
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct AmmLp {
    /// Number of LP shares owned
    pub lp_shares: u64,
    /// Cached share price at last update (scaled by 1e6)
    /// Used for stale price detection
    pub share_price_cached: i64,
    /// Last update timestamp
    pub last_update_ts: u64,
    /// Padding for alignment
    pub _padding: [u8; 8],
}

impl AmmLp {
    pub fn new(lp_shares: u64, share_price: i64, timestamp: u64) -> Self {
        Self {
            lp_shares,
            share_price_cached: share_price,
            last_update_ts: timestamp,
            _padding: [0; 8],
        }
    }

    /// Check if share price is stale (older than max_age_seconds)
    pub fn is_stale(&self, current_ts: u64, max_age_seconds: u64) -> bool {
        current_ts.saturating_sub(self.last_update_ts) > max_age_seconds
    }
}

/// Slab LP order reservation tracking
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SlabLp {
    /// Reserved quote (for buy orders)
    pub reserved_quote: u128,
    /// Reserved base (for sell orders)
    pub reserved_base: u128,
    /// Number of open orders
    pub open_order_count: u16,
    /// Padding for alignment
    pub _padding: [u8; 6],
    /// Open order IDs (for cancellation)
    pub open_order_ids: [u64; MAX_OPEN_ORDERS],
}

impl SlabLp {
    pub fn new() -> Self {
        Self {
            reserved_quote: 0,
            reserved_base: 0,
            open_order_count: 0,
            _padding: [0; 6],
            open_order_ids: [0; MAX_OPEN_ORDERS],
        }
    }

    /// Add an order reservation
    pub fn add_reservation(&mut self, order_id: u64, quote: u128, base: u128) -> Result<(), ()> {
        if (self.open_order_count as usize) >= MAX_OPEN_ORDERS {
            return Err(());
        }

        self.reserved_quote = self.reserved_quote.saturating_add(quote);
        self.reserved_base = self.reserved_base.saturating_add(base);

        let idx = self.open_order_count as usize;
        self.open_order_ids[idx] = order_id;
        self.open_order_count += 1;

        Ok(())
    }

    /// Remove an order reservation
    pub fn remove_reservation(&mut self, order_id: u64, quote: u128, base: u128) -> Result<(), ()> {
        // Find the order
        let mut found_idx: Option<usize> = None;
        for i in 0..self.open_order_count as usize {
            if self.open_order_ids[i] == order_id {
                found_idx = Some(i);
                break;
            }
        }

        let idx = found_idx.ok_or(())?;

        // Remove reservation
        self.reserved_quote = self.reserved_quote.saturating_sub(quote);
        self.reserved_base = self.reserved_base.saturating_sub(base);

        // Remove order ID (swap with last)
        let last_idx = (self.open_order_count - 1) as usize;
        if idx != last_idx {
            self.open_order_ids[idx] = self.open_order_ids[last_idx];
        }
        self.open_order_ids[last_idx] = 0;
        self.open_order_count -= 1;

        Ok(())
    }
}

/// LP bucket for a specific venue
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LpBucket {
    /// Venue identifier
    pub venue: VenueId,

    /// AMM LP data (if venue_kind == Amm)
    pub amm: Option<AmmLp>,

    /// Slab LP data (if venue_kind == Slab)
    pub slab: Option<SlabLp>,

    /// Margin requirements for this bucket
    pub im: u128,
    pub mm: u128,

    /// Active flag
    pub active: bool,

    /// Padding for alignment
    pub _padding: [u8; 7],
}

impl LpBucket {
    /// Create new AMM LP bucket
    pub fn new_amm(venue_id: VenueId, lp_shares: u64, share_price: i64, timestamp: u64) -> Self {
        Self {
            venue: venue_id,
            amm: Some(AmmLp::new(lp_shares, share_price, timestamp)),
            slab: None,
            im: 0,
            mm: 0,
            active: true,
            _padding: [0; 7],
        }
    }

    /// Create new Slab LP bucket
    pub fn new_slab(venue_id: VenueId) -> Self {
        Self {
            venue: venue_id,
            amm: None,
            slab: Some(SlabLp::new()),
            im: 0,
            mm: 0,
            active: true,
            _padding: [0; 7],
        }
    }

    /// Update margin requirements for this bucket
    pub fn update_margin(&mut self, im: u128, mm: u128) {
        self.im = im;
        self.mm = mm;
    }

    /// Check if bucket is AMM
    pub fn is_amm(&self) -> bool {
        self.venue.venue_kind == VenueKind::Amm
    }

    /// Check if bucket is Slab
    pub fn is_slab(&self) -> bool {
        self.venue.venue_kind == VenueKind::Slab
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_venue_id_creation() {
        let market = Pubkey::from([1; 32]);

        let slab_venue = VenueId::new_slab(market);
        assert_eq!(slab_venue.venue_kind, VenueKind::Slab);

        let amm_venue = VenueId::new_amm(market);
        assert_eq!(amm_venue.venue_kind, VenueKind::Amm);
    }

    #[test]
    fn test_amm_lp_staleness() {
        let amm = AmmLp::new(1000, 60_000_000, 100);

        // Not stale within 60 seconds
        assert!(!amm.is_stale(150, 60));

        // Stale after 60 seconds
        assert!(amm.is_stale(161, 60));
    }

    #[test]
    fn test_slab_lp_reservations() {
        let mut slab = SlabLp::new();

        // Add reservation
        assert!(slab.add_reservation(1001, 1000, 500).is_ok());
        assert_eq!(slab.reserved_quote, 1000);
        assert_eq!(slab.reserved_base, 500);
        assert_eq!(slab.open_order_count, 1);

        // Add another
        assert!(slab.add_reservation(1002, 2000, 1000).is_ok());
        assert_eq!(slab.reserved_quote, 3000);
        assert_eq!(slab.reserved_base, 1500);
        assert_eq!(slab.open_order_count, 2);

        // Remove first reservation
        assert!(slab.remove_reservation(1001, 1000, 500).is_ok());
        assert_eq!(slab.reserved_quote, 2000);
        assert_eq!(slab.reserved_base, 1000);
        assert_eq!(slab.open_order_count, 1);

        // Try to remove non-existent order
        assert!(slab.remove_reservation(9999, 100, 100).is_err());
    }

    #[test]
    fn test_lp_bucket_types() {
        let market = Pubkey::from([1; 32]);

        let amm_bucket = LpBucket::new_amm(VenueId::new_amm(market), 1000, 60_000_000, 100);
        assert!(amm_bucket.is_amm());
        assert!(!amm_bucket.is_slab());
        assert!(amm_bucket.amm.is_some());
        assert!(amm_bucket.slab.is_none());

        let slab_bucket = LpBucket::new_slab(VenueId::new_slab(market));
        assert!(slab_bucket.is_slab());
        assert!(!slab_bucket.is_amm());
        assert!(slab_bucket.slab.is_some());
        assert!(slab_bucket.amm.is_none());
    }

    #[test]
    fn test_max_open_orders_limit() {
        let mut slab = SlabLp::new();

        // Add MAX_OPEN_ORDERS orders
        for i in 0..MAX_OPEN_ORDERS {
            assert!(slab.add_reservation(i as u64, 100, 50).is_ok());
        }

        // Should fail to add one more
        assert!(slab.add_reservation(999, 100, 50).is_err());
        assert_eq!(slab.open_order_count, MAX_OPEN_ORDERS as u16);
    }
}
