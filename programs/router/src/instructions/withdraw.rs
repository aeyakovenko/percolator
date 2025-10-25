//! Withdraw instruction - withdraw collateral from vault

use crate::state::{Vault, Portfolio, SlabRegistry};
use percolator_common::*;

/// Process withdraw instruction
///
/// Withdraws collateral from the router vault to user's token account.
/// Ensures sufficient available (non-pledged) balance exists and respects
/// adaptive warmup throttling on PnL withdrawals.
pub fn process_withdraw(
    vault: &mut Vault,
    portfolio: &Portfolio,
    registry: &SlabRegistry,
    amount: u128,
) -> Result<(), PercolatorError> {
    // Validate amount
    if amount == 0 {
        return Err(PercolatorError::InvalidQuantity);
    }

    // Check adaptive warmup withdrawal limit
    // Principal is always withdrawable, but vested PnL is capped by unlocked_frac
    let max_withdrawable = portfolio.max_withdrawable_with_warmup(registry.warmup_state.unlocked_frac);

    // Convert to u128 for comparison (max with 0 to handle negative equity)
    let max_withdrawable_u128 = max_withdrawable.max(0) as u128;

    if amount > max_withdrawable_u128 {
        return Err(PercolatorError::InsufficientFunds);
    }

    // Attempt withdrawal from vault
    vault.withdraw(amount)
        .map_err(|_| PercolatorError::InsufficientFunds)?;

    Ok(())
}
