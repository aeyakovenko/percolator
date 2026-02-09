use percolator_sdk::{PercolatorClient, Pubkey};
use std::str::FromStr;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Initialize the client
    // Replace with actual program and state addresses
    let program_id = Pubkey::from_str("11111111111111111111111111111111")?;
    let engine_state_address = Pubkey::from_str("22222222222222222222222222222222")?;
    
    // Using a public devnet RPC for demonstration
    let rpc_url = "https://api.devnet.solana.com";
    
    println!("Initializing Percolator Client...");
    let client = PercolatorClient::new(rpc_url, program_id, engine_state_address);
    
    // 2. Fetch global state
    println!("Fetching engine state from {}...", engine_state_address);
    // Note: This will fail if the account doesn't exist at the address, 
    // but demonstrates the API usage.
    match client.get_engine_state() {
        Ok(engine) => {
            println!("Engine State Fetched Successfully!");
            println!("Total Vault Balance: {}", engine.vault.get());
            println!("Insurance Balance: {}", engine.insurance_fund.balance.get());
            println!("Num Used Accounts: {}", engine.num_used_accounts);
            
            // 3. Fetch specific user account
            if let Some(user) = client.get_account(0)? {
                println!("Account 0 exists:");
                println!("  Capital: {}", user.capital.get());
                println!("  Position Size: {}", user.position_size.get());
            } else {
                println!("Account 0 is empty/unused.");
            }
        },
        Err(e) => {
            println!("Note: Could not fetch real state (expected if address is mock): {}", e);
        }
    }

    println!("\nSDK structure is valid and ready for use.");
    Ok(())
}
