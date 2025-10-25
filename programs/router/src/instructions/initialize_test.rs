//! Unit tests for initialize instruction
//!
//! Tests both positive and negative cases for registry initialization with comprehensive
//! security checks as requested.

#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::pda::derive_registry_pda;
    use crate::state::SlabRegistry;
    use percolator_common::PercolatorError;
    use pinocchio::{account_info::AccountInfo, pubkey::Pubkey, sysvars::rent::Rent};

    // Helper to create a Pubkey from seed for testing
    fn pubkey_from_seed(seed: u8) -> Pubkey {
        let mut bytes = [0u8; 32];
        bytes[0] = seed;
        Pubkey::from(bytes)
    }

    // Mock account info helper for testing
    fn create_account_info<'a>(
        key: &'a Pubkey,
        is_signer: bool,
        is_writable: bool,
        lamports: &'a mut u64,
        data: &'a mut [u8],
        owner: &'a Pubkey,
    ) -> AccountInfo<'a> {
        AccountInfo {
            key,
            is_signer,
            is_writable,
            lamports,
            data,
            owner,
            rent_epoch: 0,
        }
    }

    /// Test: Basic SlabRegistry struct initialization
    #[test]
    fn test_registry_struct_initialization() {
        let program_id = Pubkey::default();
        let governance = Pubkey::from([1; 32]);
        let bump = 255;

        let registry = SlabRegistry::new(program_id, governance, bump);

        assert_eq!(registry.router_id, program_id);
        assert_eq!(registry.governance, governance);
        assert_eq!(registry.slab_count, 0);
        assert_eq!(registry.bump, bump);

        // Verify slabs array is zero-initialized
        for i in 0..percolator_common::MAX_SLABS {
            assert_eq!(registry.slabs[i].slab_id, Pubkey::default());
            assert!(!registry.slabs[i].active);
        }
    }

    /// Test: SlabRegistry size matches expected layout
    #[test]
    fn test_registry_size() {
        use core::mem::size_of;
        let actual_size = size_of::<SlabRegistry>();
        assert_eq!(actual_size, SlabRegistry::LEN);

        // Size should be reasonable
        let header_size = 32 + 32 + 2 + 1 + 5;
        let slab_entry_size = core::mem::size_of::<crate::state::SlabEntry>();
        let expected_min = header_size + (slab_entry_size * percolator_common::MAX_SLABS);
        assert!(actual_size >= expected_min);
    }

    /// POSITIVE TEST: Successful initialization when account already exists with correct ownership
    #[test]
    fn test_positive_initialize_existing_account() {
        let program_id = pubkey_from_seed(1);
        let (registry_pda, _bump) = derive_registry_pda(&program_id);
        let governance = pubkey_from_seed(10);
        let payer_key = pubkey_from_seed(20);

        let rent = Rent::default();
        let required_lamports = rent.minimum_balance(SlabRegistry::LEN);

        let mut registry_lamports = required_lamports;
        let mut registry_data = vec![0u8; SlabRegistry::LEN]; // All zeros = not initialized
        let mut payer_lamports = 1_000_000_000u64;
        let mut payer_data = vec![];
        let mut system_lamports = 0u64;
        let mut system_data = vec![];

        let registry_account = create_account_info(
            &registry_pda,
            false,
            true,
            &mut registry_lamports,
            &mut registry_data,
            &program_id, // Owned by program
        );

        let payer_account = create_account_info(
            &payer_key,
            true, // is_signer
            true,
            &mut payer_lamports,
            &mut payer_data,
            &pinocchio::ID,
        );

        let system_account = create_account_info(
            &pinocchio::ID,
            false,
            false,
            &mut system_lamports,
            &mut system_data,
            &pinocchio::ID,
        );

        let result = process_initialize_registry(
            &program_id,
            &registry_account,
            &payer_account,
            &system_account,
            &governance,
        );

        assert!(result.is_ok(), "Should successfully initialize existing account");

        // Verify the registry was initialized with correct values
        assert_eq!(registry_data[0..32], program_id.as_ref()[..]);
    }

    /// NEGATIVE TEST: Wrong PDA address
    #[test]
    fn test_negative_wrong_pda_address() {
        let program_id = pubkey_from_seed(1);
        let wrong_pda = pubkey_from_seed(99); // WRONG ADDRESS
        let governance = pubkey_from_seed(10);
        let payer_key = pubkey_from_seed(20);

        let mut registry_lamports = 0u64;
        let mut registry_data = vec![0u8; SlabRegistry::LEN];
        let mut payer_lamports = 1_000_000_000u64;
        let mut payer_data = vec![];
        let mut system_lamports = 0u64;
        let mut system_data = vec![];

        let registry_account = create_account_info(
            &wrong_pda,
            false,
            true,
            &mut registry_lamports,
            &mut registry_data,
            &program_id,
        );

        let payer_account = create_account_info(
            &payer_key,
            true,
            true,
            &mut payer_lamports,
            &mut payer_data,
            &pinocchio::ID,
        );

        let system_account = create_account_info(
            &pinocchio::ID,
            false,
            false,
            &mut system_lamports,
            &mut system_data,
            &pinocchio::ID,
        );

        let result = process_initialize_registry(
            &program_id,
            &registry_account,
            &payer_account,
            &system_account,
            &governance,
        );

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), PercolatorError::InvalidAccount);
    }

    /// NEGATIVE TEST: Non-signer payer
    #[test]
    fn test_negative_non_signer_payer() {
        let program_id = pubkey_from_seed(1);
        let (registry_pda, _bump) = derive_registry_pda(&program_id);
        let governance = pubkey_from_seed(10);
        let payer_key = pubkey_from_seed(20);

        let mut registry_lamports = 0u64;
        let mut registry_data = vec![0u8; SlabRegistry::LEN];
        let mut payer_lamports = 1_000_000_000u64;
        let mut payer_data = vec![];
        let mut system_lamports = 0u64;
        let mut system_data = vec![];

        let registry_account = create_account_info(
            &registry_pda,
            false,
            true,
            &mut registry_lamports,
            &mut registry_data,
            &program_id,
        );

        let payer_account = create_account_info(
            &payer_key,
            false, // NOT A SIGNER
            true,
            &mut payer_lamports,
            &mut payer_data,
            &pinocchio::ID,
        );

        let system_account = create_account_info(
            &pinocchio::ID,
            false,
            false,
            &mut system_lamports,
            &mut system_data,
            &pinocchio::ID,
        );

        let result = process_initialize_registry(
            &program_id,
            &registry_account,
            &payer_account,
            &system_account,
            &governance,
        );

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), PercolatorError::Unauthorized);
    }

    /// NEGATIVE TEST: Governance is zero address
    #[test]
    fn test_negative_governance_is_zero() {
        let program_id = pubkey_from_seed(1);
        let (registry_pda, _bump) = derive_registry_pda(&program_id);
        let governance = Pubkey::default(); // ZERO ADDRESS
        let payer_key = pubkey_from_seed(20);

        let mut registry_lamports = 0u64;
        let mut registry_data = vec![0u8; SlabRegistry::LEN];
        let mut payer_lamports = 1_000_000_000u64;
        let mut payer_data = vec![];
        let mut system_lamports = 0u64;
        let mut system_data = vec![];

        let registry_account = create_account_info(
            &registry_pda,
            false,
            true,
            &mut registry_lamports,
            &mut registry_data,
            &program_id,
        );

        let payer_account = create_account_info(
            &payer_key,
            true,
            true,
            &mut payer_lamports,
            &mut payer_data,
            &pinocchio::ID,
        );

        let system_account = create_account_info(
            &pinocchio::ID,
            false,
            false,
            &mut system_lamports,
            &mut system_data,
            &pinocchio::ID,
        );

        let result = process_initialize_registry(
            &program_id,
            &registry_account,
            &payer_account,
            &system_account,
            &governance,
        );

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), PercolatorError::InvalidAccount);
    }

    /// NEGATIVE TEST: Governance is system program
    #[test]
    fn test_negative_governance_is_system_program() {
        let program_id = pubkey_from_seed(1);
        let (registry_pda, _bump) = derive_registry_pda(&program_id);
        let governance = pinocchio::ID; // SYSTEM PROGRAM
        let payer_key = pubkey_from_seed(20);

        let mut registry_lamports = 0u64;
        let mut registry_data = vec![0u8; SlabRegistry::LEN];
        let mut payer_lamports = 1_000_000_000u64;
        let mut payer_data = vec![];
        let mut system_lamports = 0u64;
        let mut system_data = vec![];

        let registry_account = create_account_info(
            &registry_pda,
            false,
            true,
            &mut registry_lamports,
            &mut registry_data,
            &program_id,
        );

        let payer_account = create_account_info(
            &payer_key,
            true,
            true,
            &mut payer_lamports,
            &mut payer_data,
            &pinocchio::ID,
        );

        let system_account = create_account_info(
            &pinocchio::ID,
            false,
            false,
            &mut system_lamports,
            &mut system_data,
            &pinocchio::ID,
        );

        let result = process_initialize_registry(
            &program_id,
            &registry_account,
            &payer_account,
            &system_account,
            &governance,
        );

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), PercolatorError::InvalidAccount);
    }

    /// NEGATIVE TEST: Wrong system program (when creating new account)
    #[test]
    fn test_negative_wrong_system_program() {
        let program_id = pubkey_from_seed(1);
        let (registry_pda, _bump) = derive_registry_pda(&program_id);
        let governance = pubkey_from_seed(10);
        let payer_key = pubkey_from_seed(20);
        let fake_system = pubkey_from_seed(99); // WRONG SYSTEM PROGRAM

        let mut registry_lamports = 0u64; // Triggers account creation path
        let mut registry_data = vec![0u8; SlabRegistry::LEN];
        let mut payer_lamports = 1_000_000_000u64;
        let mut payer_data = vec![];
        let mut system_lamports = 0u64;
        let mut system_data = vec![];

        let registry_account = create_account_info(
            &registry_pda,
            false,
            true,
            &mut registry_lamports,
            &mut registry_data,
            &program_id,
        );

        let payer_account = create_account_info(
            &payer_key,
            true,
            true,
            &mut payer_lamports,
            &mut payer_data,
            &pinocchio::ID,
        );

        let system_account = create_account_info(
            &fake_system, // WRONG SYSTEM PROGRAM KEY
            false,
            false,
            &mut system_lamports,
            &mut system_data,
            &pinocchio::ID,
        );

        let result = process_initialize_registry(
            &program_id,
            &registry_account,
            &payer_account,
            &system_account,
            &governance,
        );

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), PercolatorError::InvalidAccount);
    }

    /// NEGATIVE TEST: Wrong account owner
    #[test]
    fn test_negative_wrong_account_owner() {
        let program_id = pubkey_from_seed(1);
        let (registry_pda, _bump) = derive_registry_pda(&program_id);
        let governance = pubkey_from_seed(10);
        let payer_key = pubkey_from_seed(20);
        let wrong_owner = pubkey_from_seed(99);

        let rent = Rent::default();
        let required_lamports = rent.minimum_balance(SlabRegistry::LEN);

        let mut registry_lamports = required_lamports; // Account exists
        let mut registry_data = vec![0u8; SlabRegistry::LEN];
        let mut payer_lamports = 1_000_000_000u64;
        let mut payer_data = vec![];
        let mut system_lamports = 0u64;
        let mut system_data = vec![];

        let registry_account = create_account_info(
            &registry_pda,
            false,
            true,
            &mut registry_lamports,
            &mut registry_data,
            &wrong_owner, // WRONG OWNER
        );

        let payer_account = create_account_info(
            &payer_key,
            true,
            true,
            &mut payer_lamports,
            &mut payer_data,
            &pinocchio::ID,
        );

        let system_account = create_account_info(
            &pinocchio::ID,
            false,
            false,
            &mut system_lamports,
            &mut system_data,
            &pinocchio::ID,
        );

        let result = process_initialize_registry(
            &program_id,
            &registry_account,
            &payer_account,
            &system_account,
            &governance,
        );

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), PercolatorError::InvalidAccount);
    }

    /// NEGATIVE TEST: Wrong account size
    #[test]
    fn test_negative_wrong_account_size() {
        let program_id = pubkey_from_seed(1);
        let (registry_pda, _bump) = derive_registry_pda(&program_id);
        let governance = pubkey_from_seed(10);
        let payer_key = pubkey_from_seed(20);

        let rent = Rent::default();
        let required_lamports = rent.minimum_balance(SlabRegistry::LEN);

        let mut registry_lamports = required_lamports; // Account exists
        let mut registry_data = vec![0u8; 100]; // WRONG SIZE
        let mut payer_lamports = 1_000_000_000u64;
        let mut payer_data = vec![];
        let mut system_lamports = 0u64;
        let mut system_data = vec![];

        let registry_account = create_account_info(
            &registry_pda,
            false,
            true,
            &mut registry_lamports,
            &mut registry_data,
            &program_id,
        );

        let payer_account = create_account_info(
            &payer_key,
            true,
            true,
            &mut payer_lamports,
            &mut payer_data,
            &pinocchio::ID,
        );

        let system_account = create_account_info(
            &pinocchio::ID,
            false,
            false,
            &mut system_lamports,
            &mut system_data,
            &pinocchio::ID,
        );

        let result = process_initialize_registry(
            &program_id,
            &registry_account,
            &payer_account,
            &system_account,
            &governance,
        );

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), PercolatorError::InvalidAccount);
    }

    /// NEGATIVE TEST: Double initialization attempt
    #[test]
    fn test_negative_double_initialization() {
        let program_id = pubkey_from_seed(1);
        let (registry_pda, _bump) = derive_registry_pda(&program_id);
        let governance = pubkey_from_seed(10);
        let payer_key = pubkey_from_seed(20);

        let rent = Rent::default();
        let required_lamports = rent.minimum_balance(SlabRegistry::LEN);

        let mut registry_lamports = required_lamports; // Account exists
        let mut registry_data = vec![0u8; SlabRegistry::LEN];
        // Mark as initialized by setting first byte non-zero (router_id field)
        registry_data[0] = 1; // ALREADY INITIALIZED

        let mut payer_lamports = 1_000_000_000u64;
        let mut payer_data = vec![];
        let mut system_lamports = 0u64;
        let mut system_data = vec![];

        let registry_account = create_account_info(
            &registry_pda,
            false,
            true,
            &mut registry_lamports,
            &mut registry_data,
            &program_id,
        );

        let payer_account = create_account_info(
            &payer_key,
            true,
            true,
            &mut payer_lamports,
            &mut payer_data,
            &pinocchio::ID,
        );

        let system_account = create_account_info(
            &pinocchio::ID,
            false,
            false,
            &mut system_lamports,
            &mut system_data,
            &pinocchio::ID,
        );

        let result = process_initialize_registry(
            &program_id,
            &registry_account,
            &payer_account,
            &system_account,
            &governance,
        );

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), PercolatorError::AlreadyInitialized);
    }

    /// Test: PDA derivation is deterministic
    #[test]
    fn test_pda_derivation_deterministic() {
        let program_id = pubkey_from_seed(1);

        let (pda1, bump1) = derive_registry_pda(&program_id);
        let (pda2, bump2) = derive_registry_pda(&program_id);

        assert_eq!(pda1, pda2, "PDA should be deterministic");
        assert_eq!(bump1, bump2, "Bump should be deterministic");
    }

    /// Test: Different programs get different PDAs
    #[test]
    fn test_pda_different_for_different_programs() {
        let program_id1 = pubkey_from_seed(1);
        let program_id2 = pubkey_from_seed(2);

        let (pda1, _) = derive_registry_pda(&program_id1);
        let (pda2, _) = derive_registry_pda(&program_id2);

        assert_ne!(pda1, pda2, "Different programs should have different PDAs");
    }
}
