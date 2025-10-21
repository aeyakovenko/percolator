//! Slab state - v0 minimal single-account orderbook

use super::{SlabHeader, QuoteCache};

/// Book area - simplified price-time orderbook
/// In v0, this is a stub placeholder for future book implementation
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct BookArea {
    /// Placeholder data for book (3KB)
    pub data: [u8; 3072],
}

impl BookArea {
    pub fn new() -> Self {
        Self {
            data: [0; 3072],
        }
    }
}

/// Main slab state - v0 minimal structure (~4KB)
/// Layout: Header (256B) + QuoteCache (256B) + BookArea (3KB)
#[repr(C)]
pub struct SlabState {
    /// Header with metadata and offsets
    pub header: SlabHeader,
    /// Quote cache (router-readable)
    pub quote_cache: QuoteCache,
    /// Book area (price-time queues)
    pub book: BookArea,
}

impl SlabState {
    /// Size of the slab state
    pub const LEN: usize = core::mem::size_of::<Self>();

    /// Create new slab state
    pub fn new(header: SlabHeader) -> Self {
        Self {
            header,
            quote_cache: QuoteCache::new(),
            book: BookArea::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pinocchio::pubkey::Pubkey;

    #[test]
    fn test_slab_size() {
        use core::mem::size_of;
        let actual_size = size_of::<SlabState>();

        // Should be around 4KB for v0
        assert!(actual_size < 5000, "SlabState is {} bytes, should be < 5KB", actual_size);
        assert!(actual_size > 3000, "SlabState is {} bytes, should be > 3KB", actual_size);
    }

    #[test]
    fn test_slab_creation() {
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

        let slab = SlabState::new(header);
        assert_eq!(slab.header.seqno, 0);
        assert_eq!(slab.quote_cache.seqno_snapshot, 0);
    }
}
