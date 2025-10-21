//! Percolator Oracle Program
//!
//! Minimal price oracle for Surfpool testing. Provides price feeds for instruments.
//!
//! ## Instructions
//!
//! - **Initialize** (0): Create a new price oracle for an instrument
//! - **UpdatePrice** (1): Update the price data (authority only)
//!
//! ## Account Structure
//!
//! ```text
//! PriceOracle (128 bytes):
//!   magic: u64           - Magic bytes for validation
//!   version: u8          - Version (currently 0)
//!   bump: u8             - PDA bump seed
//!   authority: Pubkey    - Who can update prices
//!   instrument: Pubkey   - Which instrument
//!   price: i64           - Current price (scaled)
//!   timestamp: i64       - Last update time
//!   confidence: i64      - Price confidence interval
//! ```

#![cfg_attr(target_os = "solana", no_std)]

#[cfg(target_os = "solana")]
mod entrypoint;

pub mod instructions;
pub mod state;

pub use state::{PriceOracle, PRICE_ORACLE_SIZE};
