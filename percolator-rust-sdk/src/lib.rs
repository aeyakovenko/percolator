//! # Percolator Rust SDK
//! 
//! A high-performance, type-safe SDK for interacting with the Percolator risk engine on Solana.
//! 
//! ## Features
//! - **Instruction Builders**: Type-safe construction of Percolator instructions.
//! - **Account Decoding**: Fast, size-checked decoding of the `RiskEngine` state.
//! - **High-Level Client**: Simplified RPC interaction for trading, deposits, and maintenance.

pub mod accounts;
pub mod client;
pub mod instructions;
pub mod percolator;

pub use accounts::*;
pub use client::*;
pub use instructions::*;

pub use solana_sdk::{pubkey::Pubkey, signature::{Keypair, Signer}};

/// Re-export core types from the engine for convenience
pub use percolator::{RiskEngine, Account, RiskParams, InsuranceFund, MAX_ACCOUNTS};
