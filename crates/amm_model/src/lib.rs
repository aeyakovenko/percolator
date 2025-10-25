//! AMM Model - Pure constant product math (x·y=k) for formal verification
//!
//! This crate contains the core AMM constant product formulas extracted
//! from the production AMM program for formal verification with Kani.
//!
//! **Zero Duplication**: Production `programs/amm` will import and use these
//! verified functions directly.

#![no_std]

pub mod math;

pub use math::{QuoteResult, quote_buy, quote_sell};

/// Scaling factor (1e6)
pub const SCALE: i64 = 1_000_000;

/// Basis points scale (10,000 bps = 100%)
pub const BPS_SCALE: i64 = 10_000;

/// Error types for AMM operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmmError {
    /// Invalid reserves (zero or negative)
    InvalidReserves,
    /// Invalid amount (zero or negative)
    InvalidAmount,
    /// Insufficient liquidity in pool
    InsufficientLiquidity,
    /// Arithmetic overflow
    Overflow,
}
