//! Pure Rust safety model for Kani verification
//! No Solana dependencies, no unwrap/panic, all functions total
//!
//! This crate is no_std compatible for use in Solana programs.

#![no_std]
#![forbid(unsafe_code)]

#[cfg(kani)]
extern crate kani;

pub mod state;
pub mod math;
pub mod warmup;
pub mod helpers;
pub mod transitions;
pub mod lp_bucket;
pub mod adaptive_warmup;
pub mod crisis;

// Re-export commonly used types
pub use state::*;
pub use helpers::*;
pub use transitions::*;
