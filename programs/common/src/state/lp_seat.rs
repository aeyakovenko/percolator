//! LP Seat state for adapter pattern
//!
//! RouterLpSeat is a PDA that tracks LP exposure for a specific
//! (portfolio × matcher × context_id) combination.
//!
//! PDA Derivation: ["lp_seat", router_id, matcher_state, portfolio, context_id]

use pinocchio::pubkey::Pubkey;

/// Maximum number of LP seats per portfolio
pub const MAX_LP_SEATS: usize = 8;

/// LP Seat flags
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeatFlags {
    /// No flags
    None = 0,
    /// Seat is frozen (no new operations)
    Frozen = 1 << 0,
}

/// Exposure tracking (base and quote)
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Exposure {
    /// Base asset exposure in Q64 fixed-point
    pub base_q64: i128,
    /// Quote asset exposure in Q64 fixed-point
    pub quote_q64: i128,
}

/// Router LP Seat
///
/// PDA: ["lp_seat", router_id, matcher_state, portfolio, context_id]
#[repr(C)]
pub struct RouterLpSeat {
    /// Router program ID
    pub router_id: Pubkey,
    /// Matcher state account
    pub matcher_state: Pubkey,
    /// Portfolio account
    pub portfolio: Pubkey,
    /// Context ID (allows multiple seats per portfolio × matcher)
    pub context_id: u32,
    /// Seat flags
    pub flags: u32,
    /// LP shares (AMM only; 0 for OB)
    pub lp_shares: u128,
    /// Current exposure
    pub exposure: Exposure,
    /// Reserved base collateral (locked from portfolio.free_collateral)
    pub reserved_base_q64: u128,
    /// Reserved quote collateral
    pub reserved_quote_q64: u128,
    /// Risk class (for haircut calculation)
    pub risk_class: u8,
    /// Initial margin requirement for this seat
    pub im: u128,
    /// Maintenance margin requirement for this seat
    pub mm: u128,
    /// Optional operator (delegate) who can manage this seat
    /// If set to default (all zeros), only portfolio owner can operate
    pub operator: Pubkey,
    /// PDA bump seed
    pub bump: u8,
    /// Padding for alignment
    pub _padding: [u8; 7],
}

impl RouterLpSeat {
    pub const LEN: usize = core::mem::size_of::<Self>();

    /// PDA seeds prefix
    pub const SEED_PREFIX: &'static [u8] = b"lp_seat";

    /// Initialize seat in-place
    pub fn initialize_in_place(
        &mut self,
        router_id: Pubkey,
        matcher_state: Pubkey,
        portfolio: Pubkey,
        context_id: u32,
        bump: u8,
    ) {
        self.router_id = router_id;
        self.matcher_state = matcher_state;
        self.portfolio = portfolio;
        self.context_id = context_id;
        self.flags = SeatFlags::None as u32;
        self.lp_shares = 0;
        self.exposure = Exposure::default();
        self.reserved_base_q64 = 0;
        self.reserved_quote_q64 = 0;
        self.risk_class = 0;
        self.im = 0;
        self.mm = 0;
        self.operator = Pubkey::default(); // No operator by default
        self.bump = bump;
        self._padding = [0; 7];
    }

    /// Set operator (delegate) for this seat
    pub fn set_operator(&mut self, operator: Pubkey) {
        self.operator = operator;
    }

    /// Clear operator
    pub fn clear_operator(&mut self) {
        self.operator = Pubkey::default();
    }

    /// Check if a signer is authorized to operate this seat
    ///
    /// Returns true if signer is either:
    /// - The portfolio owner
    /// - The designated operator (if set)
    pub fn is_authorized(&self, signer: &Pubkey, portfolio_owner: &Pubkey) -> bool {
        // Portfolio owner is always authorized
        if signer == portfolio_owner {
            return true;
        }

        // If operator is set and matches signer, authorized
        let has_operator = self.operator != Pubkey::default();
        if has_operator && signer == &self.operator {
            return true;
        }

        false
    }

    /// Check if seat is frozen
    pub fn is_frozen(&self) -> bool {
        (self.flags & (SeatFlags::Frozen as u32)) != 0
    }

    /// Freeze seat
    pub fn freeze(&mut self) {
        self.flags |= SeatFlags::Frozen as u32;
    }

    /// Unfreeze seat
    pub fn unfreeze(&mut self) {
        self.flags &= !(SeatFlags::Frozen as u32);
    }

    /// Reserve collateral
    pub fn reserve(&mut self, base_q64: u128, quote_q64: u128) -> Result<(), ()> {
        self.reserved_base_q64 = self.reserved_base_q64.checked_add(base_q64).ok_or(())?;
        self.reserved_quote_q64 = self.reserved_quote_q64.checked_add(quote_q64).ok_or(())?;
        Ok(())
    }

    /// Release collateral
    pub fn release(&mut self, base_q64: u128, quote_q64: u128) -> Result<(), ()> {
        if self.reserved_base_q64 < base_q64 || self.reserved_quote_q64 < quote_q64 {
            return Err(());
        }
        self.reserved_base_q64 -= base_q64;
        self.reserved_quote_q64 -= quote_q64;
        Ok(())
    }

    /// Update margin requirements
    pub fn update_margin(&mut self, im: u128, mm: u128) {
        self.im = im;
        self.mm = mm;
    }

    /// Check seat credit discipline
    ///
    /// Verifies that exposure is within reserved limits after haircuts
    pub fn check_limits(&self, haircut_base_bps: u16, haircut_quote_bps: u16) -> bool {
        // Calculate required reserves after haircuts
        let abs_base = if self.exposure.base_q64 < 0 {
            (-self.exposure.base_q64) as u128
        } else {
            self.exposure.base_q64 as u128
        };

        let abs_quote = if self.exposure.quote_q64 < 0 {
            (-self.exposure.quote_q64) as u128
        } else {
            self.exposure.quote_q64 as u128
        };

        // Required = |exposure| * (1 + haircut)
        let required_base = abs_base.saturating_mul(10_000 + haircut_base_bps as u128) / 10_000;
        let required_quote = abs_quote.saturating_mul(10_000 + haircut_quote_bps as u128) / 10_000;

        // Check limits
        required_base <= self.reserved_base_q64 && required_quote <= self.reserved_quote_q64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_seat_initialization() {
        let mut seat = unsafe { core::mem::zeroed::<RouterLpSeat>() };

        let router = Pubkey::from([1; 32]);
        let matcher = Pubkey::from([2; 32]);
        let portfolio = Pubkey::from([3; 32]);

        seat.initialize_in_place(router, matcher, portfolio, 0, 255);

        assert_eq!(seat.router_id, router);
        assert_eq!(seat.matcher_state, matcher);
        assert_eq!(seat.portfolio, portfolio);
        assert_eq!(seat.context_id, 0);
        assert_eq!(seat.lp_shares, 0);
        assert!(!seat.is_frozen());
    }

    #[test]
    fn test_seat_freeze_unfreeze() {
        let mut seat = unsafe { core::mem::zeroed::<RouterLpSeat>() };
        seat.initialize_in_place(
            Pubkey::default(),
            Pubkey::default(),
            Pubkey::default(),
            0,
            255,
        );

        assert!(!seat.is_frozen());

        seat.freeze();
        assert!(seat.is_frozen());

        seat.unfreeze();
        assert!(!seat.is_frozen());
    }

    #[test]
    fn test_seat_reserve_release() {
        let mut seat = unsafe { core::mem::zeroed::<RouterLpSeat>() };
        seat.initialize_in_place(
            Pubkey::default(),
            Pubkey::default(),
            Pubkey::default(),
            0,
            255,
        );

        // Reserve
        assert!(seat.reserve(1000, 2000).is_ok());
        assert_eq!(seat.reserved_base_q64, 1000);
        assert_eq!(seat.reserved_quote_q64, 2000);

        // Reserve more
        assert!(seat.reserve(500, 1000).is_ok());
        assert_eq!(seat.reserved_base_q64, 1500);
        assert_eq!(seat.reserved_quote_q64, 3000);

        // Release partial
        assert!(seat.release(500, 1000).is_ok());
        assert_eq!(seat.reserved_base_q64, 1000);
        assert_eq!(seat.reserved_quote_q64, 2000);

        // Try to release too much
        assert!(seat.release(2000, 1000).is_err());

        // Release all
        assert!(seat.release(1000, 2000).is_ok());
        assert_eq!(seat.reserved_base_q64, 0);
        assert_eq!(seat.reserved_quote_q64, 0);
    }

    #[test]
    fn test_seat_limit_checking() {
        let mut seat = unsafe { core::mem::zeroed::<RouterLpSeat>() };
        seat.initialize_in_place(
            Pubkey::default(),
            Pubkey::default(),
            Pubkey::default(),
            0,
            255,
        );

        // Reserve 10000 base, 20000 quote
        seat.reserve(10000, 20000).unwrap();

        // Haircuts: 10% base, 5% quote
        let haircut_base_bps = 1000; // 10%
        let haircut_quote_bps = 500; // 5%

        // Exposure within limits (with haircut):
        // Base: 8000 * 1.1 = 8800 < 10000 ✓
        // Quote: 15000 * 1.05 = 15750 < 20000 ✓
        seat.exposure = Exposure {
            base_q64: 8000,
            quote_q64: 15000,
        };
        assert!(seat.check_limits(haircut_base_bps, haircut_quote_bps));

        // Exposure exceeds limits:
        // Base: 9500 * 1.1 = 10450 > 10000 ✗
        seat.exposure = Exposure {
            base_q64: 9500,
            quote_q64: 15000,
        };
        assert!(!seat.check_limits(haircut_base_bps, haircut_quote_bps));

        // Negative exposure works the same
        seat.exposure = Exposure {
            base_q64: -8000,
            quote_q64: -15000,
        };
        assert!(seat.check_limits(haircut_base_bps, haircut_quote_bps));
    }

    #[test]
    fn test_seat_overflow_protection() {
        let mut seat = unsafe { core::mem::zeroed::<RouterLpSeat>() };
        seat.initialize_in_place(
            Pubkey::default(),
            Pubkey::default(),
            Pubkey::default(),
            0,
            255,
        );

        // Try to overflow reserves
        seat.reserved_base_q64 = u128::MAX;
        assert!(seat.reserve(1, 0).is_err());

        seat.reserved_quote_q64 = u128::MAX;
        assert!(seat.reserve(0, 1).is_err());
    }
}
