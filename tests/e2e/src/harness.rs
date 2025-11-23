//! Test harness for E2E tests with solana-test-validator

use anyhow::{Context, Result};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transaction::Transaction,
};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::thread;
use std::time::Duration;

/// Find solana binary in standard locations
fn find_solana_binary(name: &str) -> Result<PathBuf> {
    // Try standard Solana install location
    let home = env::var("HOME").context("HOME not set")?;
    let standard_path = PathBuf::from(&home)
        .join(".local/share/solana/install/active_release/bin")
        .join(name);

    if standard_path.exists() {
        return Ok(standard_path);
    }

    // Fallback to PATH
    Ok(PathBuf::from(name))
}

/// Test validator process handle
pub struct TestValidator {
    _process: Child,
    rpc_url: String,
}

impl TestValidator {
    /// Start a new test validator
    pub fn start() -> Result<Self> {
        println!("Starting solana-test-validator...");

        let validator_bin = find_solana_binary("solana-test-validator")?;
        let process = Command::new(&validator_bin)
            .arg("--reset")
            .arg("--quiet")
            .spawn()
            .context("Failed to start solana-test-validator")?;

        // Wait for validator to start
        thread::sleep(Duration::from_secs(3));

        Ok(Self {
            _process: process,
            rpc_url: "http://localhost:8899".to_string(),
        })
    }

    /// Get RPC client
    pub fn rpc_client(&self) -> RpcClient {
        RpcClient::new_with_commitment(&self.rpc_url, CommitmentConfig::confirmed())
    }
}

impl Drop for TestValidator {
    fn drop(&mut self) {
        println!("Stopping test validator...");
        if let Ok(validator_bin) = find_solana_binary("solana-test-validator") {
            let _ = Command::new(&validator_bin).arg("exit").output();
        }
    }
}

/// Test context with deployed programs
pub struct TestContext {
    pub validator: TestValidator,
    pub client: RpcClient,
    pub payer: Keypair,
    pub slab_program_id: Pubkey,
    pub amm_program_id: Pubkey,
    pub router_program_id: Pubkey,
    pub oracle_program_id: Pubkey,
}

impl TestContext {
    /// Initialize test context with deployed programs
    pub async fn new() -> Result<Self> {
        let validator = TestValidator::start()?;
        let client = validator.rpc_client();

        // Request airdrop for payer (100 SOL to cover deployments and large account creation)
        let payer = Keypair::new();
        println!("Requesting airdrop for payer: {}", payer.pubkey());

        let _signature = client
            .request_airdrop(&payer.pubkey(), 100_000_000_000)
            .context("Failed to request airdrop")?;

        // Wait for airdrop
        for _ in 0..30 {
            if let Ok(balance) = client.get_balance(&payer.pubkey()) {
                if balance > 0 {
                    break;
                }
            }
            thread::sleep(Duration::from_millis(500));
        }

        println!("Payer balance: {} SOL", client.get_balance(&payer.pubkey())? as f64 / 1e9);

        // Deploy programs
        println!("Deploying programs...");

        // Get workspace root - go up from tests/e2e to workspace root
        let mut workspace_root = env::current_dir()
            .context("Failed to get current directory")?;

        // If we're in tests/e2e, go up two levels to workspace root
        if workspace_root.ends_with("tests/e2e") {
            workspace_root = workspace_root.parent()
                .and_then(|p| p.parent())
                .ok_or_else(|| anyhow::anyhow!("Failed to find workspace root"))?
                .to_path_buf();
        }

        let slab_path = workspace_root.join("target/deploy/percolator_slab.so");
        let amm_path = workspace_root.join("target/deploy/percolator_amm.so");
        let router_path = workspace_root.join("target/deploy/percolator_router.so");
        let oracle_path = workspace_root.join("target/deploy/percolator_oracle.so");

        let slab_keypair_path = workspace_root.join("keys/percolator_slab-keypair.json");
        let amm_keypair_path = workspace_root.join("keys/percolator_amm-keypair.json");
        let router_keypair_path = workspace_root.join("keys/percolator_router-keypair.json");
        let oracle_keypair_path = workspace_root.join("keys/percolator_oracle-keypair.json");

        let slab_program_id = Self::deploy_program(&client, &payer, slab_path.to_str().unwrap(), slab_keypair_path.to_str().unwrap())?;
        let amm_program_id = Self::deploy_program(&client, &payer, amm_path.to_str().unwrap(), amm_keypair_path.to_str().unwrap())?;
        let router_program_id = Self::deploy_program(&client, &payer, router_path.to_str().unwrap(), router_keypair_path.to_str().unwrap())?;
        let oracle_program_id = Self::deploy_program(&client, &payer, oracle_path.to_str().unwrap(), oracle_keypair_path.to_str().unwrap())?;

        println!("Programs deployed:");
        println!("  Slab:   {}", slab_program_id);
        println!("  AMM:    {}", amm_program_id);
        println!("  Router: {}", router_program_id);
        println!("  Oracle: {}", oracle_program_id);

        Ok(Self {
            validator,
            client,
            payer,
            slab_program_id,
            amm_program_id,
            router_program_id,
            oracle_program_id,
        })
    }

    /// Deploy a program from .so file with specified program keypair
    fn deploy_program(_client: &RpcClient, payer: &Keypair, program_path: &str, program_keypair_path: &str) -> Result<Pubkey> {
        println!("Deploying {}...", program_path);

        // Write payer keypair to temporary file
        let keypair_path = "/tmp/test-deployer-keypair.json";
        let keypair_bytes = payer.to_bytes();
        let keypair_json = format!("[{}]", keypair_bytes.iter().map(|b| b.to_string()).collect::<Vec<_>>().join(","));
        fs::write(keypair_path, keypair_json)
            .context("Failed to write deployer keypair")?;

        // Use solana program deploy command with explicit program-id
        let solana_bin = find_solana_binary("solana")?;
        let output = Command::new(&solana_bin)
            .args(&[
                "program", "deploy",
                "--url", "http://localhost:8899",
                "--keypair", keypair_path,
                "--program-id", program_keypair_path,
                "--upgrade-authority", keypair_path,
                program_path
            ])
            .output()
            .context("Failed to deploy program")?;

        // Clean up keypair file
        let _ = fs::remove_file(keypair_path);

        if !output.status.success() {
            anyhow::bail!(
                "Program deployment failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        // Parse program ID from output
        let output_str = String::from_utf8_lossy(&output.stdout);
        for line in output_str.lines() {
            if line.contains("Program Id:") || line.contains("Program ID:") {
                if let Some(id_str) = line.split_whitespace().last() {
                    return id_str.parse().context("Failed to parse program ID");
                }
            }
        }

        anyhow::bail!("Failed to find program ID in deployment output")
    }

    /// Create and fund a new account
    pub fn create_account(
        &self,
        size: usize,
        owner: &Pubkey,
    ) -> Result<Keypair> {
        let account = Keypair::new();
        let rent = self.client.get_minimum_balance_for_rent_exemption(size)?;

        let instruction = solana_sdk::system_instruction::create_account(
            &self.payer.pubkey(),
            &account.pubkey(),
            rent,
            size as u64,
            owner,
        );

        let recent_blockhash = self.client.get_latest_blockhash()?;
        let transaction = Transaction::new_signed_with_payer(
            &[instruction],
            Some(&self.payer.pubkey()),
            &[&self.payer, &account],
            recent_blockhash,
        );

        self.client.send_and_confirm_transaction(&transaction)?;

        Ok(account)
    }

    /// Get account data
    pub fn get_account_data(&self, pubkey: &Pubkey) -> Result<Vec<u8>> {
        let account = self
            .client
            .get_account(pubkey)
            .context("Failed to get account")?;
        Ok(account.data)
    }
}
