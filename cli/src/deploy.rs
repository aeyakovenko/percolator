//! Program deployment logic

use anyhow::{Context, Result};
use colored::Colorize;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    program_pack::Pack,
    pubkey::Pubkey,
    signature::Signer,
    system_program,
};
use std::path::PathBuf;
use std::process::Command;

use crate::config::NetworkConfig;

const ROUTER_SO: &str = "target/deploy/percolator_router.so";
const SLAB_SO: &str = "target/deploy/percolator_slab.so";
const AMM_SO: &str = "target/deploy/percolator_amm.so";
const ORACLE_SO: &str = "target/deploy/percolator_oracle.so";

pub async fn deploy_programs(
    config: &NetworkConfig,
    router: bool,
    slab: bool,
    amm: bool,
    oracle: bool,
    all: bool,
    program_keypair: Option<PathBuf>,
) -> Result<()> {
    println!("{}", "=== Program Deployment ===".bright_green().bold());
    println!("{} {}", "Network:".bright_cyan(), config.network);
    println!("{} {}\n", "Deployer:".bright_cyan(), config.pubkey());

    // Build programs first
    build_programs()?;

    if all || router {
        println!("\n{}", "Deploying Router Program...".bright_yellow());
        deploy_program(config, ROUTER_SO, "Router", program_keypair.as_deref()).await?;
    }

    if all || slab {
        println!("\n{}", "Deploying Slab (Matcher) Program...".bright_yellow());
        deploy_program(config, SLAB_SO, "Slab", program_keypair.as_deref()).await?;
    }

    if all || amm {
        println!("\n{}", "Deploying AMM Program...".bright_yellow());
        deploy_program(config, AMM_SO, "AMM", program_keypair.as_deref()).await?;
    }

    if all || oracle {
        println!("\n{}", "Deploying Oracle Program...".bright_yellow());
        deploy_program(config, ORACLE_SO, "Oracle", program_keypair.as_deref()).await?;
    }

    println!("\n{}", "=== Deployment Complete ===".bright_green().bold());

    Ok(())
}

fn build_programs() -> Result<()> {
    println!("{}", "Building Solana programs with cargo build-sbf...".dimmed());

    let output = Command::new("cargo")
        .arg("build-sbf")
        .output()
        .context("Failed to execute cargo build-sbf. Is Solana CLI installed?")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Build failed:\n{}", stderr);
    }

    println!("{}", "Build successful!".bright_green());

    Ok(())
}

async fn deploy_program(
    config: &NetworkConfig,
    program_path: &str,
    name: &str,
    _program_keypair: Option<&std::path::Path>,
) -> Result<()> {
    use std::fs;

    // Check if program file exists
    if !std::path::Path::new(program_path).exists() {
        anyhow::bail!(
            "Program file not found: {}\nRun 'cargo build-sbf' first",
            program_path
        );
    }

    let program_data = fs::read(program_path)
        .with_context(|| format!("Failed to read program file: {}", program_path))?;

    println!("{} Program size: {} bytes", "  ├─".dimmed(), program_data.len());

    // Use solana program deploy command for now
    // In a production tool, you'd use solana_program_test or similar
    let output = Command::new("solana")
        .arg("program")
        .arg("deploy")
        .arg(program_path)
        .arg("--url")
        .arg(&config.rpc_url)
        .arg("--keypair")
        .arg(&config.keypair_path)
        .output()
        .context("Failed to execute solana program deploy")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Deployment failed:\n{}", stderr);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Extract program ID from output
    if let Some(line) = stdout.lines().find(|l| l.contains("Program Id:")) {
        println!("{} {}", "  └─".dimmed(), line.bright_green());
    } else {
        println!("{} {}", "  └─".dimmed(), "Deployed successfully".bright_green());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_program_paths() {
        // Just verify constants are defined
        assert!(!ROUTER_SO.is_empty());
        assert!(!SLAB_SO.is_empty());
    }
}
