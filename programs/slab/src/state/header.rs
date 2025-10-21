//! Slab header - v0 minimal metadata

use pinocchio::pubkey::Pubkey;

/// Slab header - v0 simplified for single-account slab
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SlabHeader {
    /// Magic bytes for validation (b"PERP10\0\0")
    pub magic: [u8; 8],
    /// Version (=1 for v0)
    pub version: u32,
    /// Sequence number (incremented on any book/state change)
    pub seqno: u32,

    /// Slab program ID
    pub program_id: Pubkey,
    /// LP owner pubkey
    pub lp_owner: Pubkey,
    /// Router program ID (only router can call commit_fill)
    pub router_id: Pubkey,

    /// Shared instrument ID (agreed with router)
    pub instrument: Pubkey,
    /// Contract size (1e6 fixed)
    pub contract_size: i64,
    /// Tick size (1e6 fixed)
    pub tick: i64,
    /// Lot size (1e6 fixed)
    pub lot: i64,
    /// Mark price from shared oracle (1e6 scale)
    pub mark_px: i64,

    /// Taker fee (basis points, 1e6 scale)
    pub taker_fee_bps: i64,

    /// Byte offset to BookArea (from start of account)
    pub off_book: u32,
    /// Byte offset to QuoteCache (from start of account)
    pub off_quote_cache: u32,
    /// Byte offset to receipt area (from start of account)
    pub off_receipt_area: u32,

    /// Bump seed
    pub bump: u8,
    /// Padding
    pub _padding: [u8; 3],
}

impl SlabHeader {
    pub const MAGIC: &'static [u8; 8] = b"PERP10\0\0";
    pub const VERSION: u32 = 1;
    pub const LEN: usize = core::mem::size_of::<Self>();

    /// Initialize new slab header (v0 minimal)
    pub fn new(
        program_id: Pubkey,
        lp_owner: Pubkey,
        router_id: Pubkey,
        instrument: Pubkey,
        mark_px: i64,
        taker_fee_bps: i64,
        contract_size: i64,
        bump: u8,
    ) -> Self {
        // Calculate byte offsets
        let off_quote_cache = Self::LEN as u32;
        let off_book = off_quote_cache + super::quote_cache::QuoteCache::LEN as u32;
        let off_receipt_area = off_book + 3072; // 3KB for book

        Self {
            magic: *Self::MAGIC,
            version: Self::VERSION,
            seqno: 0,
            program_id,
            lp_owner,
            router_id,
            instrument,
            contract_size,
            tick: 1_000_000,           // $1 tick
            lot: 1_000_000,            // 1.0 lot
            mark_px,
            taker_fee_bps,
            off_book,
            off_quote_cache,
            off_receipt_area,
            bump,
            _padding: [0; 3],
        }
    }

    /// Validate magic and version
    pub fn validate(&self) -> bool {
        &self.magic == Self::MAGIC && self.version == Self::VERSION
    }

    /// Increment sequence number (on any book change)
    pub fn increment_seqno(&mut self) -> u32 {
        self.seqno = self.seqno.wrapping_add(1);
        self.seqno
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_header_validation() {
        let header = SlabHeader::new(
            Pubkey::default(),
            Pubkey::default(),
            Pubkey::default(),
            Pubkey::default(),
            50_000_000_000, // $50,000 mark price
            20,             // 0.2% taker fee
            1_000_000,      // contract size
            255,
        );

        assert!(header.validate());
        assert_eq!(header.seqno, 0);
        assert_eq!(header.version, 1);
        assert_eq!(header.magic, *SlabHeader::MAGIC);
    }

    #[test]
    fn test_seqno_increment() {
        let mut header = SlabHeader::new(
            Pubkey::default(),
            Pubkey::default(),
            Pubkey::default(),
            Pubkey::default(),
            50_000_000_000,
            20,
            1_000_000,
            255,
        );

        assert_eq!(header.seqno, 0);
        assert_eq!(header.increment_seqno(), 1);
        assert_eq!(header.increment_seqno(), 2);
        assert_eq!(header.seqno, 2);
    }

    #[test]
    fn test_offsets() {
        let header = SlabHeader::new(
            Pubkey::default(),
            Pubkey::default(),
            Pubkey::default(),
            Pubkey::default(),
            50_000_000_000,
            20,
            1_000_000,
            255,
        );

        // Verify offsets are non-zero and ordered correctly
        assert!(header.off_quote_cache > 0);
        assert!(header.off_book > header.off_quote_cache);
        assert!(header.off_receipt_area > header.off_book);
    }
}
