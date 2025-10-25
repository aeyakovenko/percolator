//! LP Bucket model for venue isolation formal verification
//!
//! This module models the core LP bucket accounting to prove:
//! - V1: LP buckets are isolated (no cross-bucket transfers)
//! - V2: Principal positions never reduced by LP operations
//! - V3: AMM LP only reduced by explicit burn
//! - V4: Slab LP only reduced by explicit cancel

use crate::math::*;

/// Maximum LP buckets per portfolio
pub const MAX_LP_BUCKETS: usize = 4; // Reduced for Kani performance

/// Venue type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VenueKind {
    Slab = 0,
    Amm = 1,
}

/// AMM LP position
#[derive(Debug, Clone, Copy)]
pub struct AmmLp {
    /// LP shares owned
    pub lp_shares: u64,
}

impl AmmLp {
    pub fn new(shares: u64) -> Self {
        Self { lp_shares: shares }
    }

    pub fn is_empty(&self) -> bool {
        self.lp_shares == 0
    }
}

/// Slab LP position (open orders)
#[derive(Debug, Clone, Copy)]
pub struct SlabLp {
    /// Reserved quote for buy orders
    pub reserved_quote: u128,
    /// Reserved base for sell orders
    pub reserved_base: u128,
}

impl SlabLp {
    pub fn new(quote: u128, base: u128) -> Self {
        Self {
            reserved_quote: quote,
            reserved_base: base,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.reserved_quote == 0 && self.reserved_base == 0
    }
}

/// LP bucket - venue-specific position
#[derive(Debug, Clone, Copy)]
pub enum LpBucket {
    Empty,
    Amm(AmmLp),
    Slab(SlabLp),
}

impl LpBucket {
    pub fn is_empty(&self) -> bool {
        match self {
            LpBucket::Empty => true,
            LpBucket::Amm(amm) => amm.is_empty(),
            LpBucket::Slab(slab) => slab.is_empty(),
        }
    }

    pub fn venue_kind(&self) -> Option<VenueKind> {
        match self {
            LpBucket::Empty => None,
            LpBucket::Amm(_) => Some(VenueKind::Amm),
            LpBucket::Slab(_) => Some(VenueKind::Slab),
        }
    }
}

/// Portfolio with principal and LP positions
#[derive(Debug, Clone)]
pub struct Portfolio {
    /// Principal trading position (not affected by LP operations)
    pub principal: u128,
    /// LP buckets (per-venue)
    pub lp_buckets: [LpBucket; MAX_LP_BUCKETS],
}

impl Portfolio {
    pub fn new(principal: u128) -> Self {
        Self {
            principal,
            lp_buckets: [LpBucket::Empty; MAX_LP_BUCKETS],
        }
    }

    /// Find bucket index for a venue, or first empty slot
    fn find_bucket(&self, venue_kind: VenueKind) -> Option<usize> {
        // First, try to find existing bucket of same kind
        for (i, bucket) in self.lp_buckets.iter().enumerate() {
            if bucket.venue_kind() == Some(venue_kind) {
                return Some(i);
            }
        }

        // Otherwise, find first empty
        for (i, bucket) in self.lp_buckets.iter().enumerate() {
            if bucket.is_empty() {
                return Some(i);
            }
        }

        None
    }
}

/// Mint AMM LP shares (add liquidity to AMM)
pub fn mint_amm_lp(portfolio: Portfolio, bucket_idx: usize, shares: u64) -> Portfolio {
    let mut result = portfolio;

    if bucket_idx >= MAX_LP_BUCKETS {
        return result; // Out of bounds, no-op
    }

    match result.lp_buckets[bucket_idx] {
        LpBucket::Empty => {
            result.lp_buckets[bucket_idx] = LpBucket::Amm(AmmLp::new(shares));
        }
        LpBucket::Amm(ref mut amm) => {
            amm.lp_shares = amm.lp_shares.saturating_add(shares);
        }
        LpBucket::Slab(_) => {
            // Wrong venue type, no-op
        }
    }

    result
}

/// Burn AMM LP shares (remove liquidity from AMM)
pub fn burn_amm_lp(portfolio: Portfolio, bucket_idx: usize, shares: u64) -> Portfolio {
    let mut result = portfolio;

    if bucket_idx >= MAX_LP_BUCKETS {
        return result;
    }

    match result.lp_buckets[bucket_idx] {
        LpBucket::Amm(ref mut amm) => {
            amm.lp_shares = amm.lp_shares.saturating_sub(shares);
            if amm.lp_shares == 0 {
                result.lp_buckets[bucket_idx] = LpBucket::Empty;
            }
        }
        _ => {
            // Not an AMM bucket, no-op
        }
    }

    result
}

/// Place order on slab (reserves collateral)
pub fn place_slab_order(
    portfolio: Portfolio,
    bucket_idx: usize,
    reserve_quote: u128,
    reserve_base: u128,
) -> Portfolio {
    let mut result = portfolio;

    if bucket_idx >= MAX_LP_BUCKETS {
        return result;
    }

    match result.lp_buckets[bucket_idx] {
        LpBucket::Empty => {
            result.lp_buckets[bucket_idx] = LpBucket::Slab(SlabLp::new(reserve_quote, reserve_base));
        }
        LpBucket::Slab(ref mut slab) => {
            slab.reserved_quote = add_u128(slab.reserved_quote, reserve_quote);
            slab.reserved_base = add_u128(slab.reserved_base, reserve_base);
        }
        LpBucket::Amm(_) => {
            // Wrong venue type, no-op
        }
    }

    result
}

/// Cancel slab order (releases collateral)
pub fn cancel_slab_order(
    portfolio: Portfolio,
    bucket_idx: usize,
    release_quote: u128,
    release_base: u128,
) -> Portfolio {
    let mut result = portfolio;

    if bucket_idx >= MAX_LP_BUCKETS {
        return result;
    }

    match result.lp_buckets[bucket_idx] {
        LpBucket::Slab(ref mut slab) => {
            slab.reserved_quote = sub_u128(slab.reserved_quote, release_quote);
            slab.reserved_base = sub_u128(slab.reserved_base, release_base);
            if slab.is_empty() {
                result.lp_buckets[bucket_idx] = LpBucket::Empty;
            }
        }
        _ => {
            // Not a slab bucket, no-op
        }
    }

    result
}
