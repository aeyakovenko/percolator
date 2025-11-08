//! Venue PnL state for LP adapter pattern
//!
//! VenuePnl tracks aggregate PnL metrics across all LP seats for a given venue
//! (matcher). This provides venue-level accounting for fee credits, venue fees,
//! and realized PnL.

use pinocchio::pubkey::Pubkey;

/// Venue PnL tracking
///
/// PDA: ["venue_pnl", router_id, matcher_state]
#[repr(C)]
pub struct VenuePnl {
    /// Router program ID
    pub router_id: Pubkey,
    /// Matcher state account
    pub matcher_state: Pubkey,
    /// Accumulated maker fee credits across all seats
    pub maker_fee_credits: i128,
    /// Accumulated venue fees across all seats
    pub venue_fees: i128,
    /// Accumulated realized PnL across all seats
    pub realized_pnl: i128,
    /// PDA bump seed
    pub bump: u8,
    /// Padding for alignment
    pub _padding: [u8; 7],
}

impl VenuePnl {
    pub const LEN: usize = core::mem::size_of::<Self>();

    /// PDA seeds prefix
    pub const SEED_PREFIX: &'static [u8] = b"venue_pnl";

    /// Initialize venue PnL in-place
    pub fn initialize_in_place(
        &mut self,
        router_id: Pubkey,
        matcher_state: Pubkey,
        bump: u8,
    ) {
        self.router_id = router_id;
        self.matcher_state = matcher_state;
        self.maker_fee_credits = 0;
        self.venue_fees = 0;
        self.realized_pnl = 0;
        self.bump = bump;
        self._padding = [0; 7];
    }

    /// Apply liquidity result deltas to venue PnL
    pub fn apply_deltas(
        &mut self,
        maker_fee_credits_delta: i128,
        venue_fees_delta: i128,
        realized_pnl_delta: i128,
    ) -> Result<(), ()> {
        self.maker_fee_credits = self.maker_fee_credits
            .checked_add(maker_fee_credits_delta)
            .ok_or(())?;

        self.venue_fees = self.venue_fees
            .checked_add(venue_fees_delta)
            .ok_or(())?;

        self.realized_pnl = self.realized_pnl
            .checked_add(realized_pnl_delta)
            .ok_or(())?;

        Ok(())
    }

    /// Get net PnL (maker fee credits + realized PnL - venue fees)
    pub fn net_pnl(&self) -> i128 {
        self.maker_fee_credits
            .saturating_add(self.realized_pnl)
            .saturating_sub(self.venue_fees)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_venue_pnl_initialization() {
        let mut pnl = unsafe { core::mem::zeroed::<VenuePnl>() };

        let router = Pubkey::from([1; 32]);
        let matcher = Pubkey::from([2; 32]);

        pnl.initialize_in_place(router, matcher, 255);

        assert_eq!(pnl.router_id, router);
        assert_eq!(pnl.matcher_state, matcher);
        assert_eq!(pnl.maker_fee_credits, 0);
        assert_eq!(pnl.venue_fees, 0);
        assert_eq!(pnl.realized_pnl, 0);
        assert_eq!(pnl.bump, 255);
    }

    #[test]
    fn test_apply_positive_deltas() {
        let mut pnl = unsafe { core::mem::zeroed::<VenuePnl>() };
        pnl.initialize_in_place(Pubkey::default(), Pubkey::default(), 255);

        assert!(pnl.apply_deltas(1000, 100, 500).is_ok());

        assert_eq!(pnl.maker_fee_credits, 1000);
        assert_eq!(pnl.venue_fees, 100);
        assert_eq!(pnl.realized_pnl, 500);
    }

    #[test]
    fn test_apply_negative_deltas() {
        let mut pnl = unsafe { core::mem::zeroed::<VenuePnl>() };
        pnl.initialize_in_place(Pubkey::default(), Pubkey::default(), 255);

        // Set initial values
        pnl.maker_fee_credits = 1000;
        pnl.venue_fees = 200;
        pnl.realized_pnl = 500;

        // Apply negative deltas
        assert!(pnl.apply_deltas(-500, -100, -200).is_ok());

        assert_eq!(pnl.maker_fee_credits, 500);
        assert_eq!(pnl.venue_fees, 100);
        assert_eq!(pnl.realized_pnl, 300);
    }

    #[test]
    fn test_apply_mixed_deltas() {
        let mut pnl = unsafe { core::mem::zeroed::<VenuePnl>() };
        pnl.initialize_in_place(Pubkey::default(), Pubkey::default(), 255);

        pnl.maker_fee_credits = 1000;
        pnl.venue_fees = 200;
        pnl.realized_pnl = -100;

        assert!(pnl.apply_deltas(-200, 50, 300).is_ok());

        assert_eq!(pnl.maker_fee_credits, 800);
        assert_eq!(pnl.venue_fees, 250);
        assert_eq!(pnl.realized_pnl, 200);
    }

    #[test]
    fn test_net_pnl_calculation() {
        let mut pnl = unsafe { core::mem::zeroed::<VenuePnl>() };
        pnl.initialize_in_place(Pubkey::default(), Pubkey::default(), 255);

        // Net PnL = maker_fee_credits + realized_pnl - venue_fees
        pnl.maker_fee_credits = 1000;
        pnl.venue_fees = 200;
        pnl.realized_pnl = 500;

        // Net = 1000 + 500 - 200 = 1300
        assert_eq!(pnl.net_pnl(), 1300);
    }

    #[test]
    fn test_net_pnl_with_negative_values() {
        let mut pnl = unsafe { core::mem::zeroed::<VenuePnl>() };
        pnl.initialize_in_place(Pubkey::default(), Pubkey::default(), 255);

        pnl.maker_fee_credits = 500;
        pnl.venue_fees = 1000;
        pnl.realized_pnl = -200;

        // Net = 500 + (-200) - 1000 = -700
        assert_eq!(pnl.net_pnl(), -700);
    }

    #[test]
    fn test_overflow_protection() {
        let mut pnl = unsafe { core::mem::zeroed::<VenuePnl>() };
        pnl.initialize_in_place(Pubkey::default(), Pubkey::default(), 255);

        // Try to overflow maker_fee_credits
        pnl.maker_fee_credits = i128::MAX;
        assert!(pnl.apply_deltas(1, 0, 0).is_err());

        // Try to overflow venue_fees
        pnl.venue_fees = i128::MAX;
        assert!(pnl.apply_deltas(0, 1, 0).is_err());

        // Try to overflow realized_pnl
        pnl.realized_pnl = i128::MAX;
        assert!(pnl.apply_deltas(0, 0, 1).is_err());
    }

    #[test]
    fn test_underflow_protection() {
        let mut pnl = unsafe { core::mem::zeroed::<VenuePnl>() };
        pnl.initialize_in_place(Pubkey::default(), Pubkey::default(), 255);

        // Try to underflow maker_fee_credits
        pnl.maker_fee_credits = i128::MIN;
        assert!(pnl.apply_deltas(-1, 0, 0).is_err());

        // Try to underflow venue_fees
        pnl.venue_fees = i128::MIN;
        assert!(pnl.apply_deltas(0, -1, 0).is_err());

        // Try to underflow realized_pnl
        pnl.realized_pnl = i128::MIN;
        assert!(pnl.apply_deltas(0, 0, -1).is_err());
    }

    #[test]
    fn test_accumulation_over_multiple_operations() {
        let mut pnl = unsafe { core::mem::zeroed::<VenuePnl>() };
        pnl.initialize_in_place(Pubkey::default(), Pubkey::default(), 255);

        // Simulate multiple LP operations
        assert!(pnl.apply_deltas(100, 10, 50).is_ok());
        assert!(pnl.apply_deltas(200, 20, -30).is_ok());
        assert!(pnl.apply_deltas(-50, 5, 100).is_ok());

        assert_eq!(pnl.maker_fee_credits, 250);
        assert_eq!(pnl.venue_fees, 35);
        assert_eq!(pnl.realized_pnl, 120);
        assert_eq!(pnl.net_pnl(), 335); // 250 + 120 - 35
    }
}
