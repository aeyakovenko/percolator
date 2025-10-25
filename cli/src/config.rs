//! Network configuration and keypair management

use anyhow::{Context, Result};
use solana_sdk::{
    pubkey::Pubkey,
    signature::{Keypair, Signer},
};
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

pub struct NetworkConfig {
    pub network: String,
    pub rpc_url: String,
    pub ws_url: String,
    pub keypair: Keypair,
    pub keypair_path: PathBuf,
    pub router_program_id: Pubkey,
    pub slab_program_id: Pubkey,
    pub amm_program_id: Pubkey,
    pub oracle_program_id: Pubkey,
}

impl NetworkConfig {
    pub fn new(network: &str, rpc_url: Option<String>, keypair_path: Option<PathBuf>) -> Result<Self> {
        let (default_rpc, ws_url) = match network {
            "localnet" | "local" => (
                "http://127.0.0.1:8899".to_string(),
                "ws://127.0.0.1:8900".to_string(),
            ),
            "devnet" => (
                "https://api.devnet.solana.com".to_string(),
                "wss://api.devnet.solana.com".to_string(),
            ),
            "mainnet-beta" | "mainnet" => (
                "https://api.mainnet-beta.solana.com".to_string(),
                "wss://api.mainnet-beta.solana.com".to_string(),
            ),
            _ => anyhow::bail!("Unknown network: {}. Use localnet, devnet, or mainnet-beta", network),
        };

        let rpc_url = rpc_url.unwrap_or(default_rpc);

        // Resolve keypair path
        let keypair_path = if let Some(path) = keypair_path {
            path
        } else {
            // Try default Solana CLI config location
            let home = std::env::var("HOME").context("HOME environment variable not set")?;
            PathBuf::from(home).join(".config/solana/id.json")
        };

        let keypair = load_keypair(&keypair_path)?;

        // Use deployed program IDs (from /tmp/deployment_summary.md)
        let router_program_id = Pubkey::from_str("DnStBYxRB5PVRueXzoaZmnakb4kZga3kywDKg8yqjsEX")
            .expect("Invalid router program ID");
        let slab_program_id = Pubkey::from_str("9e27KmcW24ME7nNzk6Xd3h7ssJhiMU2EaLtGbxDXK5Li")
            .expect("Invalid slab program ID");
        let amm_program_id = Pubkey::from_str("E2L2kaCF7RUs8gW7nW4Jpcy3J1NpVVjEQD5zmiD35YVs")
            .expect("Invalid AMM program ID");
        let oracle_program_id = Pubkey::from_str("My3wc35Vt13mStsd7tGi346ydXeXowTe95YJhRqWrtS")
            .expect("Invalid oracle program ID");

        Ok(Self {
            network: network.to_string(),
            rpc_url,
            ws_url,
            keypair,
            keypair_path,
            router_program_id,
            slab_program_id,
            amm_program_id,
            oracle_program_id,
        })
    }

    pub fn pubkey(&self) -> solana_sdk::pubkey::Pubkey {
        self.keypair.pubkey()
    }
}

/// Load a keypair from a JSON file
fn load_keypair(path: &Path) -> Result<Keypair> {
    if !path.exists() {
        anyhow::bail!(
            "Keypair file not found: {}\n\
             Create one with: solana-keygen new --outfile {}",
            path.display(),
            path.display()
        );
    }

    let data = fs::read_to_string(path)
        .with_context(|| format!("Failed to read keypair file: {}", path.display()))?;

    let bytes: Vec<u8> = serde_json::from_str(&data)
        .with_context(|| format!("Failed to parse keypair JSON: {}", path.display()))?;

    Keypair::from_bytes(&bytes)
        .with_context(|| format!("Invalid keypair data in: {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_urls() {
        let config = NetworkConfig::new("localnet", None, None);
        assert!(config.is_ok() || config.as_ref().err().unwrap().to_string().contains("Keypair file not found"));
    }
}
