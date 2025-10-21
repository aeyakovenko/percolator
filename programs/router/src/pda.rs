//! Program Derived Address (PDA) helpers for Router program
//!
//! PDAs are deterministic addresses derived from seeds and the program ID.
//! They allow the program to own and control accounts without needing a private key.

use pinocchio::pubkey::{find_program_address, Pubkey};

/// Seed prefix for vault accounts (one per mint)
pub const VAULT_SEED: &[u8] = b"vault";

/// Seed prefix for escrow accounts (per user, slab, mint)
pub const ESCROW_SEED: &[u8] = b"escrow";

/// Seed prefix for capability token accounts
pub const CAP_SEED: &[u8] = b"cap";

/// Seed prefix for portfolio accounts (per user)
pub const PORTFOLIO_SEED: &[u8] = b"portfolio";

/// Seed prefix for slab registry
pub const REGISTRY_SEED: &[u8] = b"registry";

/// Seed prefix for router authority (used for CPI signing)
pub const AUTHORITY_SEED: &[u8] = b"authority";

/// Derive router authority PDA
///
/// This PDA is used as the router's signing authority for CPIs to slabs.
/// Slabs should be initialized with this PDA as their router_id.
///
/// # Arguments
/// * `program_id` - The router program ID
///
/// # Returns
/// * `(Pubkey, u8)` - The derived PDA and its bump seed
pub fn derive_authority_pda(program_id: &Pubkey) -> (Pubkey, u8) {
    find_program_address(&[AUTHORITY_SEED], program_id)
}

/// Derive vault PDA for a given mint
///
/// Vault stores collateral for a specific mint (e.g., USDC, SOL)
///
/// # Arguments
/// * `mint` - The mint pubkey for which to derive the vault
/// * `program_id` - The router program ID
///
/// # Returns
/// * `(Pubkey, u8)` - The derived PDA and its bump seed
pub fn derive_vault_pda(mint: &Pubkey, program_id: &Pubkey) -> (Pubkey, u8) {
    find_program_address(&[VAULT_SEED, mint.as_ref()], program_id)
}

/// Derive escrow PDA for a user on a specific slab with a specific mint
///
/// Escrow holds user funds pledged to a specific slab
///
/// # Arguments
/// * `user` - The user's pubkey
/// * `slab` - The slab program's pubkey
/// * `mint` - The mint pubkey
/// * `program_id` - The router program ID
///
/// # Returns
/// * `(Pubkey, u8)` - The derived PDA and its bump seed
pub fn derive_escrow_pda(
    user: &Pubkey,
    slab: &Pubkey,
    mint: &Pubkey,
    program_id: &Pubkey,
) -> (Pubkey, u8) {
    find_program_address(
        &[ESCROW_SEED, user.as_ref(), slab.as_ref(), mint.as_ref()],
        program_id,
    )
}

/// Derive capability token PDA
///
/// Capability tokens authorize scoped debits from escrows
///
/// # Arguments
/// * `user` - The user's pubkey
/// * `slab` - The slab program's pubkey
/// * `mint` - The mint pubkey
/// * `nonce` - Unique nonce to allow multiple concurrent caps
/// * `program_id` - The router program ID
///
/// # Returns
/// * `(Pubkey, u8)` - The derived PDA and its bump seed
pub fn derive_cap_pda(
    user: &Pubkey,
    slab: &Pubkey,
    mint: &Pubkey,
    nonce: u64,
    program_id: &Pubkey,
) -> (Pubkey, u8) {
    find_program_address(
        &[
            CAP_SEED,
            user.as_ref(),
            slab.as_ref(),
            mint.as_ref(),
            &nonce.to_le_bytes(),
        ],
        program_id,
    )
}

/// Derive portfolio PDA for a user
///
/// Portfolio aggregates user's positions and margin across all slabs
///
/// # Arguments
/// * `user` - The user's pubkey
/// * `program_id` - The router program ID
///
/// # Returns
/// * `(Pubkey, u8)` - The derived PDA and its bump seed
pub fn derive_portfolio_pda(user: &Pubkey, program_id: &Pubkey) -> (Pubkey, u8) {
    find_program_address(&[PORTFOLIO_SEED, user.as_ref()], program_id)
}

/// Derive slab registry PDA
///
/// Registry maintains list of approved slabs
///
/// # Arguments
/// * `program_id` - The router program ID
///
/// # Returns
/// * `(Pubkey, u8)` - The derived PDA and its bump seed
pub fn derive_registry_pda(program_id: &Pubkey) -> (Pubkey, u8) {
    find_program_address(&[REGISTRY_SEED], program_id)
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "solana")]
    use super::*;

    // Note: PDA tests only run on Solana target due to syscall requirements
    #[test]
    #[cfg(target_os = "solana")]
    fn test_vault_pda_derivation() {
        let program_id = Pubkey::default();
        let mint = Pubkey::default();

        let (pda1, bump1) = derive_vault_pda(&mint, &program_id);
        let (pda2, bump2) = derive_vault_pda(&mint, &program_id);

        // Same inputs should produce same output
        assert_eq!(pda1, pda2);
        assert_eq!(bump1, bump2);
    }

    #[test]
    #[cfg(target_os = "solana")]
    fn test_escrow_pda_derivation() {
        let program_id = Pubkey::default();
        let user = Pubkey::default();
        let slab = Pubkey::default();
        let mint = Pubkey::default();

        let (pda1, bump1) = derive_escrow_pda(&user, &slab, &mint, &program_id);
        let (pda2, bump2) = derive_escrow_pda(&user, &slab, &mint, &program_id);

        // Same inputs should produce same output
        assert_eq!(pda1, pda2);
        assert_eq!(bump1, bump2);
    }

    #[test]
    #[cfg(target_os = "solana")]
    fn test_cap_pda_unique_nonces() {
        let program_id = Pubkey::default();
        let user = Pubkey::default();
        let slab = Pubkey::default();
        let mint = Pubkey::default();

        let (pda1, _) = derive_cap_pda(&user, &slab, &mint, 0, &program_id);
        let (pda2, _) = derive_cap_pda(&user, &slab, &mint, 1, &program_id);

        // Different nonces should produce different PDAs
        assert_ne!(pda1, pda2);
    }

    #[test]
    #[cfg(target_os = "solana")]
    fn test_portfolio_pda_derivation() {
        let program_id = Pubkey::default();
        let user = Pubkey::default();

        let (pda1, bump1) = derive_portfolio_pda(&user, &program_id);
        let (pda2, bump2) = derive_portfolio_pda(&user, &program_id);

        // Same inputs should produce same output
        assert_eq!(pda1, pda2);
        assert_eq!(bump1, bump2);
    }

    #[test]
    #[cfg(target_os = "solana")]
    fn test_registry_pda_derivation() {
        let program_id = Pubkey::default();

        let (pda1, bump1) = derive_registry_pda(&program_id);
        let (pda2, bump2) = derive_registry_pda(&program_id);

        // Same program ID should produce same output
        assert_eq!(pda1, pda2);
        assert_eq!(bump1, bump2);
    }
}
