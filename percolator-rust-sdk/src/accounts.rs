use solana_client::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use borsh::BorshDeserialize;
pub use percolator::{RiskEngine, Account, MAX_ACCOUNTS};

/// Fetches and decodes the `RiskEngine` state from a Solana account.
/// 
/// This uses unsage memory casting for maximum performance, assuming the account
/// data follows the `#[repr(C)]` layout of the engine.
pub fn fetch_risk_engine(
    client: &RpcClient,
    address: &Pubkey,
) -> Result<RiskEngine, Box<dyn std::error::Error>> {
    let account_data = client.get_account_data(address)?;
    
    if account_data.len() < std::mem::size_of::<RiskEngine>() {
        return Err("Account data too small for RiskEngine".into());
    }

    // SAFETY: RiskEngine is #[repr(C)] and we check the size.
    // Alignment is handled by the allocator because it's been copied into a Vec<u8> by solana-client.
    let engine = unsafe {
        let ptr = account_data.as_ptr() as *const RiskEngine;
        ptr.read_unaligned()
    };

    Ok(engine)
}

pub fn get_account_from_engine(
    engine: &RiskEngine,
    index: usize,
) -> Option<&Account> {
    if index < MAX_ACCOUNTS && engine.is_used(index) {
        Some(&engine.accounts[index])
    } else {
        None
    }
}
