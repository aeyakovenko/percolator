//! Oracle state structures
//!
//! Minimal price oracle for Surfpool testing. Stores price data for instruments.

use pinocchio::pubkey::Pubkey;

/// Size of PriceOracle account: 128 bytes
pub const PRICE_ORACLE_SIZE: usize = 128;

/// Price oracle account state
///
/// Stores current price data for an instrument. Similar to Pyth but simplified.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PriceOracle {
    /// Magic bytes: "PRCL" + "ORCL" = 0x4C43525044444C52
    pub magic: u64,

    /// Version (currently 0)
    pub version: u8,

    /// Bump seed for PDA
    pub bump: u8,

    /// Padding for alignment
    pub _padding: [u8; 6],

    /// Authority that can update prices
    pub authority: Pubkey,

    /// Instrument this oracle is for
    pub instrument: Pubkey,

    /// Current price (scaled by 1_000_000)
    pub price: i64,

    /// Last update timestamp (Unix timestamp)
    pub timestamp: i64,

    /// Price confidence interval (scaled by 1_000_000)
    pub confidence: i64,

    /// Reserved for future use
    pub _reserved: [u8; 16],
}

impl PriceOracle {
    /// Magic bytes for validation
    pub const MAGIC: &'static [u8; 8] = b"PRCLORCL";

    /// Current version
    pub const VERSION: u8 = 0;

    /// Create a new price oracle
    pub fn new(authority: Pubkey, instrument: Pubkey, price: i64, bump: u8) -> Self {
        Self {
            magic: u64::from_le_bytes(*Self::MAGIC),
            version: Self::VERSION,
            bump,
            _padding: [0; 6],
            authority,
            instrument,
            price,
            timestamp: 0,
            confidence: 0,
            _reserved: [0; 16],
        }
    }

    /// Validate the oracle account
    pub fn validate(&self) -> bool {
        self.magic == u64::from_le_bytes(*Self::MAGIC) && self.version == Self::VERSION
    }

    /// Update the price
    pub fn update_price(&mut self, price: i64, timestamp: i64, confidence: i64) {
        self.price = price;
        self.timestamp = timestamp;
        self.confidence = confidence;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_price_oracle_size() {
        use core::mem::size_of;
        assert_eq!(size_of::<PriceOracle>(), PRICE_ORACLE_SIZE);
    }

    #[test]
    fn test_price_oracle_creation() {
        let authority = Pubkey::default();
        let instrument = Pubkey::default();
        let oracle = PriceOracle::new(authority, instrument, 60_000_000_000, 0);

        assert!(oracle.validate());
        assert_eq!(oracle.price, 60_000_000_000);
        assert_eq!(oracle.authority, authority);
        assert_eq!(oracle.instrument, instrument);
    }

    #[test]
    fn test_price_update() {
        let authority = Pubkey::default();
        let instrument = Pubkey::default();
        let mut oracle = PriceOracle::new(authority, instrument, 60_000_000_000, 0);

        oracle.update_price(61_000_000_000, 1234567890, 100_000);

        assert_eq!(oracle.price, 61_000_000_000);
        assert_eq!(oracle.timestamp, 1234567890);
        assert_eq!(oracle.confidence, 100_000);
    }
}
