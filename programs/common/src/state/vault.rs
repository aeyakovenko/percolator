//! Vault account for holding collateral

use pinocchio::pubkey::Pubkey;

/// Vault account storing collateral for a specific mint
/// PDA: ["vault", router_id, mint]
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Vault {
    /// Router program ID
    pub router_id: Pubkey,
    /// Mint pubkey
    pub mint: Pubkey,
    /// Token account holding the funds
    pub token_account: Pubkey,
    /// Total balance
    pub balance: u128,
    /// Total pledged to escrows
    pub total_pledged: u128,
    /// Bump seed
    pub bump: u8,
    /// Padding
    pub _padding: [u8; 7],
}

impl Vault {
    pub const LEN: usize = core::mem::size_of::<Self>();

    /// Get available balance (not pledged)
    ///
    /// # Safety
    ///
    /// Uses formally verified arithmetic to prevent underflow.
    pub fn available(&self) -> u128 {
        use model_safety::math::sub_u128;
        sub_u128(self.balance, self.total_pledged)
    }

    /// Pledge amount to escrow
    ///
    /// # Safety
    ///
    /// Uses formally verified arithmetic to prevent overflow.
    pub fn pledge(&mut self, amount: u128) -> Result<(), ()> {
        use model_safety::math::add_u128;

        if self.available() < amount {
            return Err(());
        }
        self.total_pledged = add_u128(self.total_pledged, amount);
        Ok(())
    }

    /// Unpledge amount from escrow
    ///
    /// # Safety
    ///
    /// Uses formally verified arithmetic to prevent underflow.
    pub fn unpledge(&mut self, amount: u128) {
        use model_safety::math::sub_u128;
        self.total_pledged = sub_u128(self.total_pledged, amount);
    }

    /// Deposit to vault
    ///
    /// # Safety
    ///
    /// Uses formally verified arithmetic to prevent overflow.
    pub fn deposit(&mut self, amount: u128) {
        use model_safety::math::add_u128;
        self.balance = add_u128(self.balance, amount);
    }

    /// Withdraw from vault
    ///
    /// # Safety
    ///
    /// Uses formally verified arithmetic to prevent underflow.
    pub fn withdraw(&mut self, amount: u128) -> Result<(), ()> {
        use model_safety::math::sub_u128;

        if self.available() < amount {
            return Err(());
        }
        self.balance = sub_u128(self.balance, amount);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vault_pledge() {
        let mut vault = Vault {
            router_id: Pubkey::default(),
            mint: Pubkey::default(),
            token_account: Pubkey::default(),
            balance: 1000,
            total_pledged: 0,
            bump: 0,
            _padding: [0; 7],
        };

        assert_eq!(vault.available(), 1000);
        assert!(vault.pledge(500).is_ok());
        assert_eq!(vault.available(), 500);
        assert_eq!(vault.total_pledged, 500);

        assert!(vault.pledge(600).is_err());
        assert!(vault.pledge(500).is_ok());
        assert_eq!(vault.available(), 0);

        vault.unpledge(300);
        assert_eq!(vault.available(), 300);
    }
}
