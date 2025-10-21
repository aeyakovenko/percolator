//! Initialize instruction tests
//!
//! NOTE: Old complex design tests removed for v0.
//! See tests/v0_*.rs for v0-specific tests.

#[cfg(test)]
mod initialize_v0_tests {
    use crate::state::{SlabHeader, SlabState};
    use pinocchio::pubkey::Pubkey;

    #[test]
    fn test_slab_header_v0_initialization() {
        // v0 simplified initialization
        let program_id = Pubkey::default();
        let lp_owner = Pubkey::from([1; 32]);
        let router_id = Pubkey::from([2; 32]);
        let instrument = Pubkey::from([3; 32]);
        let mark_px = 50_000_000_000i64; // $50,000
        let taker_fee_bps = 20i64; // 0.2%
        let bump = 255;

        let contract_size = 1_000_000i64; // 1.0

        let header = SlabHeader::new(
            program_id,
            lp_owner,
            router_id,
            instrument,
            mark_px,
            taker_fee_bps,
            contract_size,
            bump,
        );

        // Verify magic bytes and version
        assert_eq!(header.magic, *SlabHeader::MAGIC);
        assert_eq!(header.version, SlabHeader::VERSION);
        assert!(header.validate());

        // Verify parameters
        assert_eq!(header.program_id, program_id);
        assert_eq!(header.lp_owner, lp_owner);
        assert_eq!(header.router_id, router_id);
        assert_eq!(header.instrument, instrument);
        assert_eq!(header.mark_px, mark_px);
        assert_eq!(header.taker_fee_bps, taker_fee_bps);
        assert_eq!(header.contract_size, contract_size);
        assert_eq!(header.bump, bump);

        // Verify seqno starts at 0
        assert_eq!(header.seqno, 0);
    }

    #[test]
    fn test_slab_state_size_v0() {
        // v0: SlabState should be ~4KB (not 7MB like complex design)
        use core::mem::size_of;
        let actual_size = size_of::<SlabState>();

        // Should be around 4KB for v0
        assert!(actual_size > 3_000, "SlabState too small: {} bytes", actual_size);
        assert!(actual_size < 5_000, "SlabState too large: {} bytes", actual_size);
    }

    #[test]
    fn test_header_size_matches() {
        use core::mem::size_of;
        let actual_size = size_of::<SlabHeader>();
        assert_eq!(actual_size, SlabHeader::LEN);
    }
}
