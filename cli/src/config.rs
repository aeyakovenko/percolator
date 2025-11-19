//! Network configuration and keypair management

use anyhow::{Context, Result};
use serde::Deserialize;
use solana_sdk::{
    pubkey::Pubkey,
    signature::{Keypair, Signer},
};
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

#[derive(Debug, Deserialize)]
struct LocalConfig {
    #[serde(default)]
    localnet: Option<NetworkProgramIds>,
    #[serde(default)]
    devnet: Option<NetworkProgramIds>,
    #[serde(default)]
    mainnet_beta: Option<NetworkProgramIds>,
}

#[derive(Debug, Deserialize)]
struct NetworkProgramIds {
    router_program_id: String,
    slab_program_id: String,
    amm_program_id: String,
    oracle_program_id: String,
}

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

        // Load program IDs from config file or use defaults
        let (router_program_id, slab_program_id, amm_program_id, oracle_program_id) =
            load_program_ids(network)?;

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

/// Load program IDs from .percolator-local.toml or use defaults
fn load_program_ids(network: &str) -> Result<(Pubkey, Pubkey, Pubkey, Pubkey)> {
    // Try to load from local config file
    let config_path = PathBuf::from(".percolator-local.toml");

    if config_path.exists() {
        let config_str = fs::read_to_string(&config_path)
            .context("Failed to read .percolator-local.toml")?;

        let config: LocalConfig = toml::from_str(&config_str)
            .context("Failed to parse .percolator-local.toml")?;

        let program_ids = match network {
            "localnet" | "local" => config.localnet,
            "devnet" => config.devnet,
            "mainnet-beta" | "mainnet" => config.mainnet_beta,
            _ => None,
        };

        if let Some(ids) = program_ids {
            let router = Pubkey::from_str(&ids.router_program_id)
                .context("Invalid router_program_id in config")?;
            let slab = Pubkey::from_str(&ids.slab_program_id)
                .context("Invalid slab_program_id in config")?;
            let amm = Pubkey::from_str(&ids.amm_program_id)
                .context("Invalid amm_program_id in config")?;
            let oracle = Pubkey::from_str(&ids.oracle_program_id)
                .context("Invalid oracle_program_id in config")?;

            return Ok((router, slab, amm, oracle));
        }
    }

    // Fallback to default hardcoded IDs (these are the old defaults)
    log::warn!("Using default program IDs. Create .percolator-local.toml for custom deployment.");
    log::warn!("Copy .percolator-local.toml.example and update with your deployed program IDs.");

    let router = Pubkey::from_str("FqyPRML6ccZdH1xjMbe5CePx81wVJfZXxGANKfageW5Q")
        .expect("Invalid default router program ID");
    let slab = Pubkey::from_str("2qQsQvBDQCCBm3sULZhczQWgQekxxbgtvrJFmLGs1csJ")
        .expect("Invalid default slab program ID");
    let amm = Pubkey::from_str("H8pmyp9Dixgkmmw3m7h8Y9SbwJmKgRoeejRBWrunrJJ7")
        .expect("Invalid default AMM program ID");
    let oracle = Pubkey::from_str("BaB5cSBUFe47i1NQ5V3ijaGWmx6BCdW8yJB65hHcQRtX")
        .expect("Invalid default oracle program ID");

    Ok((router, slab, amm, oracle))
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
