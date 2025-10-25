//! Kani safety proofs for percolator safety module
//!
//! This crate provides formal verification of 6 core safety invariants
//! using the Kani model checker:
//!
//! - **I1: Principal Inviolability** - User principals never decrease during loss socialization
//! - **I2: Conservation** - Vault accounting balances across all state transitions
//! - **I3: Authorization** - Only authorized router can mutate balances
//! - **I4: Bounded Socialization** - Haircuts only affect winners, total bounded correctly
//! - **I5: Throttle Safety** - PnL withdrawals respect warmup period limits
//! - **I6: Matcher Isolation** - Matcher operations cannot move funds
//!
//! ## Module Organization
//!
//! - **`minimal`** - Fast concrete proofs (7 proofs, <10s total)
//! - **`medium`** - Parameterized symbolic proofs (11 proofs, <40s total)
//! - **`edge`** - Edge cases and boundary conditions (16 proofs, ~60s total)
//! - **`safety`** - Original complex proofs (intractable, not recommended)

pub mod sanitizer;
pub mod generators;
pub mod adversary;

#[cfg(kani)]
pub mod safety;

#[cfg(kani)]
pub mod minimal;

#[cfg(kani)]
pub mod medium;

#[cfg(kani)]
pub mod edge;

#[cfg(kani)]
pub mod liquidation;

#[cfg(kani)]
pub mod properties;

#[cfg(kani)]
pub mod adaptive_warmup;

#[cfg(kani)]
pub mod amm;

#[cfg(kani)]
pub mod venue_isolation;

#[cfg(kani)]
pub mod pnl_vesting;

#[cfg(kani)]
pub mod portfolio;

#[cfg(kani)]
pub mod liq_planner;
