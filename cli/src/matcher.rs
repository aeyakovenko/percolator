//! Matcher/slab management operations

use anyhow::{Context, Result};
use colored::Colorize;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::Keypair,
    signer::Signer,
    transaction::Transaction,
};
use std::str::FromStr;

use crate::{client, config::NetworkConfig};

/// Register a slab in the router registry
///
/// This allows the router to route orders to the slab
pub async fn register_slab(
    config: &NetworkConfig,
    registry_address: String,
    slab_id: String,
    oracle_id: String,
    imr_bps: u64,           // Initial margin ratio in basis points (e.g., 500 = 5%)
    mmr_bps: u64,           // Maintenance margin ratio in basis points
    maker_fee_bps: u64,     // Maker fee cap in basis points
    taker_fee_bps: u64,     // Taker fee cap in basis points
    latency_sla_ms: u64,    // Latency SLA in milliseconds
    max_exposure: u128,     // Maximum position exposure
) -> Result<()> {
    println!("{}", "=== Register Slab ===".bright_green().bold());
    println!("{} {}", "Network:".bright_cyan(), config.network);
    println!("{} {}", "Registry:".bright_cyan(), registry_address);
    println!("{} {}", "Slab ID:".bright_cyan(), slab_id);
    println!("{} {}", "Oracle ID:".bright_cyan(), oracle_id);
    println!("{} {}bps ({}%)", "IMR:".bright_cyan(), imr_bps, imr_bps as f64 / 100.0);
    println!("{} {}bps ({}%)", "MMR:".bright_cyan(), mmr_bps, mmr_bps as f64 / 100.0);

    // Parse addresses
    let registry = Pubkey::from_str(&registry_address)
        .context("Invalid registry address")?;
    let slab = Pubkey::from_str(&slab_id)
        .context("Invalid slab ID")?;
    let oracle = Pubkey::from_str(&oracle_id)
        .context("Invalid oracle ID")?;

    // Get RPC client and governance keypair (payer)
    let rpc_client = client::create_rpc_client(config);
    let governance = &config.keypair;

    println!("\n{} {}", "Governance:".bright_cyan(), governance.pubkey());

    // Build instruction data: [discriminator(8), slab_id(32), version_hash(32), oracle_id(32),
    //                           imr(8), mmr(8), maker_fee(8), taker_fee(8), latency(8), exposure(16)]
    let mut instruction_data = Vec::with_capacity(153);
    instruction_data.push(8u8); // RegisterSlab discriminator
    instruction_data.extend_from_slice(&slab.to_bytes());
    instruction_data.extend_from_slice(&[0u8; 32]); // version_hash (placeholder)
    instruction_data.extend_from_slice(&oracle.to_bytes());
    instruction_data.extend_from_slice(&imr_bps.to_le_bytes());
    instruction_data.extend_from_slice(&mmr_bps.to_le_bytes());
    instruction_data.extend_from_slice(&maker_fee_bps.to_le_bytes());
    instruction_data.extend_from_slice(&taker_fee_bps.to_le_bytes());
    instruction_data.extend_from_slice(&latency_sla_ms.to_le_bytes());
    instruction_data.extend_from_slice(&max_exposure.to_le_bytes());

    // Build RegisterSlab instruction
    let register_ix = Instruction {
        program_id: config.router_program_id,
        accounts: vec![
            AccountMeta::new(registry, false),            // Registry account (writable)
            AccountMeta::new(governance.pubkey(), true),  // Governance (signer, writable)
        ],
        data: instruction_data,
    };

    // Send transaction
    let recent_blockhash = rpc_client.get_latest_blockhash()?;
    let transaction = Transaction::new_signed_with_payer(
        &[register_ix],
        Some(&governance.pubkey()),
        &[governance],
        recent_blockhash,
    );

    println!("{}", "Sending RegisterSlab transaction...".bright_green());
    let signature = rpc_client
        .send_and_confirm_transaction(&transaction)
        .context("Failed to send RegisterSlab transaction")?;

    println!("\n{} {}", "Success!".bright_green().bold(), "✓".bright_green());
    println!("{} {}", "Signature:".bright_cyan(), signature);
    println!("{}", "Slab registered successfully".bright_green());

    Ok(())
}

pub async fn create_matcher(
    config: &NetworkConfig,
    exchange: String,
    symbol: String,
    tick_size: u64,
    lot_size: u64,
) -> Result<()> {
    println!("{}", "=== Create Matcher (Slab) ===".bright_green().bold());
    println!("{} {}", "Network:".bright_cyan(), config.network);
    println!("{} {}", "Exchange:".bright_cyan(), exchange);
    println!("{} {}", "Symbol:".bright_cyan(), symbol);
    println!("{} {}", "Tick Size:".bright_cyan(), tick_size);
    println!("{} {}", "Lot Size:".bright_cyan(), lot_size);

    // Get RPC client and payer
    let rpc_client = client::create_rpc_client(config);
    let payer = &config.keypair;

    println!("\n{} {}", "Payer:".bright_cyan(), payer.pubkey());
    println!("{} {}", "Slab Program:".bright_cyan(), config.slab_program_id);

    // Generate new keypair for the slab account
    let slab_keypair = Keypair::new();
    let slab_pubkey = slab_keypair.pubkey();

    println!("{} {}", "Slab Address:".bright_cyan(), slab_pubkey);

    // Calculate rent for ~4KB account
    const SLAB_SIZE: usize = 4096;
    let rent = rpc_client
        .get_minimum_balance_for_rent_exemption(SLAB_SIZE)
        .context("Failed to get rent exemption amount")?;

    println!("{} {} lamports", "Rent Required:".bright_cyan(), rent);

    // Build CreateAccount instruction to allocate the slab account
    let create_account_ix = solana_sdk::system_instruction::create_account(
        &payer.pubkey(),
        &slab_pubkey,
        rent,
        SLAB_SIZE as u64,
        &config.slab_program_id,
    );

    // Build initialization instruction data
    // Format: [discriminator(1), lp_owner(32), router_id(32), instrument(32),
    //          mark_px(8), taker_fee_bps(8), contract_size(8), bump(1)]
    let mut instruction_data = Vec::with_capacity(122);
    instruction_data.push(0u8); // Initialize discriminator

    // lp_owner: Use payer as the LP owner
    instruction_data.extend_from_slice(&payer.pubkey().to_bytes());

    // router_id: Use router authority PDA (for CPI signing)
    // The slab will verify this PDA signs the CommitFill CPI
    let (router_authority_pda, _) = Pubkey::find_program_address(
        &[b"authority"],
        &config.router_program_id
    );
    instruction_data.extend_from_slice(&router_authority_pda.to_bytes());

    // instrument: Use a dummy instrument ID (system program for now)
    let instrument = solana_sdk::system_program::id();
    instruction_data.extend_from_slice(&instrument.to_bytes());

    // mark_px: Use tick_size * 100 as initial mark price (e.g., $1.00 if tick_size=1)
    let mark_px = (tick_size as i64) * 100;
    instruction_data.extend_from_slice(&mark_px.to_le_bytes());

    // taker_fee_bps: Default to 20 bps (0.2%)
    let taker_fee_bps = 20i64;
    instruction_data.extend_from_slice(&taker_fee_bps.to_le_bytes());

    // contract_size: Use lot_size as contract size
    let contract_size = lot_size as i64;
    instruction_data.extend_from_slice(&contract_size.to_le_bytes());

    // bump: Not using PDA, so 0
    instruction_data.push(0u8);

    // Build Initialize instruction
    let initialize_ix = Instruction {
        program_id: config.slab_program_id,
        accounts: vec![
            AccountMeta::new(slab_pubkey, false),      // Slab account (writable)
            AccountMeta::new(payer.pubkey(), true),    // Payer (signer, writable for fees)
        ],
        data: instruction_data,
    };

    // Send transaction with both instructions
    println!("\n{}", "Creating slab account and initializing...".bright_green());

    let recent_blockhash = rpc_client.get_latest_blockhash()?;
    let transaction = Transaction::new_signed_with_payer(
        &[create_account_ix, initialize_ix],
        Some(&payer.pubkey()),
        &[payer, &slab_keypair], // Both payer and slab must sign
        recent_blockhash,
    );

    let signature = rpc_client
        .send_and_confirm_transaction(&transaction)
        .context("Failed to create and initialize slab")?;

    println!("\n{} {}", "Success!".bright_green().bold(), "✓".bright_green());
    println!("{} {}", "Transaction:".bright_cyan(), signature);
    println!("{} {}", "Slab Address:".bright_cyan(), slab_pubkey);
    println!("\n{}", "Next step: Register this slab with the router using:".dimmed());
    println!("  {}", format!("percolator matcher register-slab --slab-id {}", slab_pubkey).dimmed());

    Ok(())
}

pub async fn list_matchers(config: &NetworkConfig, registry_address: String) -> Result<()> {
    println!("{}", "=== List Registered Slabs ===".bright_green().bold());
    println!("{} {}", "Network:".bright_cyan(), config.network);
    println!("{} {}", "Registry:".bright_cyan(), registry_address);

    // Parse registry address
    let registry = Pubkey::from_str(&registry_address)
        .context("Invalid registry address")?;

    // Get RPC client
    let rpc_client = client::create_rpc_client(config);

    // Fetch account data
    let account = rpc_client
        .get_account(&registry)
        .context("Failed to fetch registry account")?;

    // Verify ownership
    if account.owner != config.router_program_id {
        anyhow::bail!("Account is not owned by router program");
    }

    // Deserialize registry data
    const REGISTRY_SIZE_BPF: usize = 43688;
    if account.data.len() != REGISTRY_SIZE_BPF {
        println!("\n{} Registry size: {} bytes", "Warning:".yellow(), account.data.len());
    }

    let registry_data = unsafe {
        &*(account.data.as_ptr() as *const percolator_common::state::SlabRegistry)
    };

    // Note: Slab whitelist removed - slabs are now permissionless
    println!("\n{}", "=== Slab Architecture ===".bright_yellow());
    println!("{}", "  Slabs are permissionless - no whitelist required".bright_green());
    println!("{}", "  Users can trade on any slab that implements the adapter interface".dimmed());
    println!("{}", "\n  To view a specific slab, use: percolator matcher info <slab_id>".bright_cyan());

    println!("\n{} {}\n", "Status:".bright_green(), "OK ✓".bright_green());
    Ok(())
}

pub async fn show_matcher_info(config: &NetworkConfig, slab_id: String) -> Result<()> {
    println!("{}", "=== Slab Info ===".bright_green().bold());
    println!("{} {}", "Network:".bright_cyan(), config.network);
    println!("{} {}", "Slab ID:".bright_cyan(), slab_id);

    // Parse slab address
    let slab_pubkey = Pubkey::from_str(&slab_id)
        .context("Invalid slab address")?;

    // Get RPC client
    let rpc_client = client::create_rpc_client(config);

    // Check if account exists
    match rpc_client.get_account(&slab_pubkey) {
        Ok(account) => {
            println!("\n{}", "=== Account Info ===".bright_yellow());
            println!("{} {}", "Owner:".bright_cyan(), account.owner);
            println!("{} {} bytes", "Data Size:".bright_cyan(), account.data.len());
            println!("{} {} lamports", "Balance:".bright_cyan(), account.lamports);
            println!("{} {}", "Executable:".bright_cyan(), account.executable);

            // Note: Full slab account deserialization would require slab program types
            println!("\n{}", "Note: Full slab details require slab program deployed".dimmed());
        }
        Err(_) => {
            println!("\n{} Slab account not found - this may be a test address", "Warning:".yellow());
        }
    }

    Ok(())
}

/// Update funding rate for a slab
///
/// Calls the UpdateFunding instruction (discriminator = 5) on the slab program.
/// This updates the cumulative funding index based on mark-oracle price deviation.
///
/// # Arguments
/// * `config` - Network configuration
/// * `slab_address` - Slab pubkey as string
/// * `oracle_price` - Oracle price (scaled by 1e6, e.g., 100_000_000 for price 100)
/// * `wait_time` - Optional time to wait before calling (simulates time passage)
///
/// # Returns
/// * Ok(()) on success
pub async fn update_funding(
    config: &NetworkConfig,
    slab_address: String,
    oracle_price: i64,
    wait_time: Option<u64>,
) -> Result<()> {
    println!("{}", "=== Update Funding ===".bright_green().bold());
    println!("{} {}", "Network:".bright_cyan(), config.network);
    println!("{} {}", "Slab:".bright_cyan(), slab_address);
    println!("{} {} ({})", "Oracle Price:".bright_cyan(), oracle_price, oracle_price as f64 / 1_000_000.0);

    // Wait if requested (simulates time passage for funding accrual)
    if let Some(seconds) = wait_time {
        println!("\n{} Waiting {} seconds to simulate funding accrual...", "⏱".bright_yellow(), seconds);
        std::thread::sleep(std::time::Duration::from_secs(seconds));
    }

    // Parse slab address
    let slab_pubkey = Pubkey::from_str(&slab_address)
        .context("Invalid slab address")?;

    // Get RPC client
    let rpc_client = client::create_rpc_client(config);
    let authority = &config.keypair;

    // Use slab program ID from config
    let slab_program_id = config.slab_program_id;

    // Build instruction data:
    // - Byte 0: discriminator = 5 (UpdateFunding)
    // - Bytes 1-8: oracle_price (i64 little-endian)
    let mut instruction_data = Vec::with_capacity(9);
    instruction_data.push(5); // UpdateFunding discriminator
    instruction_data.extend_from_slice(&oracle_price.to_le_bytes());

    // Build UpdateFunding instruction
    // Accounts:
    // 0. [writable] slab_account
    // 1. [signer] authority (LP owner)
    let instruction = Instruction {
        program_id: slab_program_id,
        accounts: vec![
            AccountMeta::new(slab_pubkey, false),
            AccountMeta::new_readonly(authority.pubkey(), true),
        ],
        data: instruction_data,
    };

    // Create and send transaction
    let recent_blockhash = rpc_client
        .get_latest_blockhash()
        .context("Failed to get recent blockhash")?;

    let transaction = Transaction::new_signed_with_payer(
        &[instruction],
        Some(&authority.pubkey()),
        &[authority],
        recent_blockhash,
    );

    println!("\n{}", "Sending transaction...".dimmed());
    let signature = rpc_client
        .send_and_confirm_transaction(&transaction)
        .context("Failed to send UpdateFunding transaction")?;

    println!("\n{} {}", "✓ Funding updated! Signature:".bright_green(), signature);

    Ok(())
}

/// Place a limit order on the order book
///
/// Calls the PlaceOrder instruction (discriminator = 2) on the slab program.
///
/// # Arguments
/// * `config` - Network configuration
/// * `slab_address` - Slab pubkey as string
/// * `side` - "buy" or "sell"
/// * `price` - Order price (scaled by 1e6, e.g., 100_000_000 for price 100)
/// * `qty` - Order quantity (scaled by 1e6, e.g., 1_000_000 for quantity 1.0)
///
/// # Returns
/// * Ok(()) on success, prints order_id from transaction logs
pub async fn place_order(
    config: &NetworkConfig,
    slab_address: String,
    side: String,
    price: i64,
    qty: i64,
    post_only: bool,
    reduce_only: bool,
) -> Result<()> {
    println!("{}", "=== Place Order ===".bright_green().bold());
    println!("{} {}", "Network:".bright_cyan(), config.network);
    println!("{} {}", "Slab:".bright_cyan(), slab_address);
    println!("{} {}", "Side:".bright_cyan(), side);
    println!("{} {} ({})", "Price:".bright_cyan(), price, price as f64 / 1_000_000.0);
    println!("{} {} ({})", "Quantity:".bright_cyan(), qty, qty as f64 / 1_000_000.0);
    if post_only {
        println!("{} {}", "Post-only:".bright_cyan(), "true");
    }
    if reduce_only {
        println!("{} {}", "Reduce-only:".bright_cyan(), "true");
    }

    // Parse side
    let side_u8 = match side.to_lowercase().as_str() {
        "buy" | "bid" => 0u8,
        "sell" | "ask" => 1u8,
        _ => anyhow::bail!("Invalid side '{}'. Use 'buy' or 'sell'", side),
    };

    // Validate inputs
    if price <= 0 {
        anyhow::bail!("Price must be > 0");
    }
    if qty <= 0 {
        anyhow::bail!("Quantity must be > 0");
    }

    // Parse slab address
    let slab_pubkey = Pubkey::from_str(&slab_address)
        .context("Invalid slab address")?;

    // Get RPC client
    let rpc_client = client::create_rpc_client(config);
    let authority = &config.keypair;

    // Use slab program ID from config
    let slab_program_id = config.slab_program_id;

    // Build instruction data:
    // - Byte 0: discriminator = 3 (PlaceOrder - TESTING ONLY, use router LP for production)
    // - Byte 1: side (u8)
    // - Bytes 2-9: price (i64 little-endian)
    // - Bytes 10-17: qty (i64 little-endian)
    // - Byte 18: post_only (u8, optional)
    // - Byte 19: reduce_only (u8, optional)
    let mut instruction_data = Vec::with_capacity(20);
    instruction_data.push(3); // PlaceOrder discriminator (testing only)
    instruction_data.push(side_u8);
    instruction_data.extend_from_slice(&price.to_le_bytes());
    instruction_data.extend_from_slice(&qty.to_le_bytes());
    instruction_data.push(post_only as u8);
    instruction_data.push(reduce_only as u8);

    // Build PlaceOrder instruction
    // Accounts:
    // 0. [writable] slab_account
    // 1. [signer] authority (trader)
    let instruction = Instruction {
        program_id: slab_program_id,
        accounts: vec![
            AccountMeta::new(slab_pubkey, false),
            AccountMeta::new_readonly(authority.pubkey(), true),
        ],
        data: instruction_data,
    };

    // Create and send transaction
    let recent_blockhash = rpc_client
        .get_latest_blockhash()
        .context("Failed to get recent blockhash")?;

    let transaction = Transaction::new_signed_with_payer(
        &[instruction],
        Some(&authority.pubkey()),
        &[authority],
        recent_blockhash,
    );

    println!("\n{}", "Sending transaction...".dimmed());
    let signature = rpc_client
        .send_and_confirm_transaction(&transaction)
        .context("Failed to send PlaceOrder transaction")?;

    println!("\n{} {}", "✓ Order placed! Signature:".bright_green(), signature);
    println!("{}", "Note: order_id can be extracted from transaction logs".dimmed());

    Ok(())
}

/// Cancel an order from the order book
///
/// Calls the CancelOrder instruction (discriminator = 3) on the slab program.
///
/// # Arguments
/// * `config` - Network configuration
/// * `slab_address` - Slab pubkey as string
/// * `order_id` - Order ID to cancel
///
/// # Returns
/// * Ok(()) on success
pub async fn cancel_order(
    config: &NetworkConfig,
    slab_address: String,
    order_id: u64,
) -> Result<()> {
    println!("{}", "=== Cancel Order ===".bright_green().bold());
    println!("{} {}", "Network:".bright_cyan(), config.network);
    println!("{} {}", "Slab:".bright_cyan(), slab_address);
    println!("{} {}", "Order ID:".bright_cyan(), order_id);

    // Parse slab address
    let slab_pubkey = Pubkey::from_str(&slab_address)
        .context("Invalid slab address")?;

    // Get RPC client
    let rpc_client = client::create_rpc_client(config);
    let authority = &config.keypair;

    // Use slab program ID from config
    let slab_program_id = config.slab_program_id;

    // Build instruction data:
    // - Byte 0: discriminator = 4 (CancelOrder - TESTING ONLY, use router LP for production)
    // - Bytes 1-8: order_id (u64 little-endian)
    let mut instruction_data = Vec::with_capacity(9);
    instruction_data.push(4); // CancelOrder discriminator (testing only)
    instruction_data.extend_from_slice(&order_id.to_le_bytes());

    // Build CancelOrder instruction
    // Accounts:
    // 0. [writable] slab_account
    // 1. [signer] authority (order owner)
    let instruction = Instruction {
        program_id: slab_program_id,
        accounts: vec![
            AccountMeta::new(slab_pubkey, false),
            AccountMeta::new_readonly(authority.pubkey(), true),
        ],
        data: instruction_data,
    };

    // Create and send transaction
    let recent_blockhash = rpc_client
        .get_latest_blockhash()
        .context("Failed to get recent blockhash")?;

    let transaction = Transaction::new_signed_with_payer(
        &[instruction],
        Some(&authority.pubkey()),
        &[authority],
        recent_blockhash,
    );

    println!("\n{}", "Sending transaction...".dimmed());
    let signature = rpc_client
        .send_and_confirm_transaction(&transaction)
        .context("Failed to send CancelOrder transaction")?;

    println!("\n{} {}", "✓ Order cancelled! Signature:".bright_green(), signature);

    Ok(())
}

/// Match orders using CommitFill instruction (for testing)
///
/// NOTE: This instruction requires router authority. In production, this is called
/// by the router program via CPI. For testing, initialize the slab with your keypair
/// as the router_id.
///
/// # Arguments
/// * `config` - Network configuration
/// * `slab_address` - Slab pubkey as string
/// * `side` - "buy" or "sell"
/// * `qty` - Quantity to match (1e6 scale)
/// * `limit_px` - Limit price (1e6 scale)
/// * `time_in_force` - "GTC", "IOC", or "FOK"
/// * `self_trade_prevention` - "None", "CancelNewest", "CancelOldest", or "DecrementAndCancel"
///
/// # Returns
/// * Ok(()) on success
pub async fn match_order(
    config: &NetworkConfig,
    slab_address: String,
    side: String,
    qty: i64,
    limit_px: i64,
    time_in_force: String,
    self_trade_prevention: String,
) -> Result<()> {
    println!("{}", "=== Match Order (CommitFill) ===".bright_green().bold());
    println!("{} {}", "Network:".bright_cyan(), config.network);
    println!("{} {}", "Slab:".bright_cyan(), slab_address);
    println!("{} {}", "Side:".bright_cyan(), side);
    println!("{} {} ({})", "Quantity:".bright_cyan(), qty, qty as f64 / 1_000_000.0);
    println!("{} {} ({})", "Limit Price:".bright_cyan(), limit_px, limit_px as f64 / 1_000_000.0);
    println!("{} {}", "Time-in-Force:".bright_cyan(), time_in_force);
    println!("{} {}", "Self-Trade Prevention:".bright_cyan(), self_trade_prevention);

    // Parse side
    let side_u8 = match side.to_lowercase().as_str() {
        "buy" | "bid" => 0u8,
        "sell" | "ask" => 1u8,
        _ => anyhow::bail!("Invalid side '{}'. Use 'buy' or 'sell'", side),
    };

    // Parse time-in-force
    let tif_u8 = match time_in_force.to_uppercase().as_str() {
        "GTC" => 0u8,
        "IOC" => 1u8,
        "FOK" => 2u8,
        _ => anyhow::bail!("Invalid time-in-force '{}'. Use 'GTC', 'IOC', or 'FOK'", time_in_force),
    };

    // Parse self-trade prevention
    let stp_u8 = match self_trade_prevention.as_str() {
        "None" => 0u8,
        "CancelNewest" => 1u8,
        "CancelOldest" => 2u8,
        "DecrementAndCancel" => 3u8,
        _ => anyhow::bail!("Invalid self-trade prevention '{}'. Use 'None', 'CancelNewest', 'CancelOldest', or 'DecrementAndCancel'", self_trade_prevention),
    };

    // Validate inputs
    if qty <= 0 {
        anyhow::bail!("Quantity must be > 0");
    }
    if limit_px <= 0 {
        anyhow::bail!("Limit price must be > 0");
    }

    // Parse slab address
    let slab_pubkey = Pubkey::from_str(&slab_address)
        .context("Invalid slab address")?;

    // Get RPC client
    let rpc_client = client::create_rpc_client(config);
    let authority = &config.keypair;

    // Use slab program ID from config
    let slab_program_id = config.slab_program_id;

    // Fetch slab account to get current seqno
    let slab_account = rpc_client
        .get_account(&slab_pubkey)
        .context("Failed to fetch slab account")?;

    // Read seqno from slab header (at offset 12 after magic[8] + version[4])
    if slab_account.data.len() < 16 {
        anyhow::bail!("Slab account data too small");
    }
    let expected_seqno = u32::from_le_bytes([
        slab_account.data[12],
        slab_account.data[13],
        slab_account.data[14],
        slab_account.data[15],
    ]);

    println!("{} {}", "Expected Seqno:".bright_cyan(), expected_seqno);

    // Create receipt account (temp keypair)
    let receipt_keypair = Keypair::new();
    let receipt_pubkey = receipt_keypair.pubkey();

    println!("{} {}", "Receipt Account:".bright_cyan(), receipt_pubkey);

    // Calculate rent for receipt account (~256 bytes)
    const RECEIPT_SIZE: usize = 256;
    let rent = rpc_client
        .get_minimum_balance_for_rent_exemption(RECEIPT_SIZE)
        .context("Failed to get rent exemption amount")?;

    // Create receipt account
    let create_receipt_ix = solana_sdk::system_instruction::create_account(
        &authority.pubkey(),
        &receipt_pubkey,
        rent,
        RECEIPT_SIZE as u64,
        &slab_program_id,
    );

    // Build CommitFill instruction data:
    // - Byte 0: discriminator = 1 (CommitFill)
    // - Bytes 1-4: expected_seqno (u32)
    // - Byte 5: side (u8)
    // - Bytes 6-13: qty (i64)
    // - Bytes 14-21: limit_px (i64)
    // - Byte 22: time_in_force (u8, optional)
    // - Byte 23: self_trade_prevention (u8, optional)
    let mut instruction_data = Vec::with_capacity(24);
    instruction_data.push(1); // CommitFill discriminator
    instruction_data.extend_from_slice(&expected_seqno.to_le_bytes());
    instruction_data.push(side_u8);
    instruction_data.extend_from_slice(&qty.to_le_bytes());
    instruction_data.extend_from_slice(&limit_px.to_le_bytes());
    instruction_data.push(tif_u8);
    instruction_data.push(stp_u8);

    // Build CommitFill instruction
    // Accounts:
    // 0. [writable] slab_account
    // 1. [writable] receipt_account
    // 2. [signer] router_signer (authority in test mode)
    // 3. taker_owner (authority in test mode)
    let commit_fill_ix = Instruction {
        program_id: slab_program_id,
        accounts: vec![
            AccountMeta::new(slab_pubkey, false),
            AccountMeta::new(receipt_pubkey, false),
            AccountMeta::new_readonly(authority.pubkey(), true),
            AccountMeta::new_readonly(authority.pubkey(), false), // taker_owner
        ],
        data: instruction_data,
    };

    // Create and send transaction
    let recent_blockhash = rpc_client
        .get_latest_blockhash()
        .context("Failed to get recent blockhash")?;

    let transaction = Transaction::new_signed_with_payer(
        &[create_receipt_ix, commit_fill_ix],
        Some(&authority.pubkey()),
        &[authority, &receipt_keypair],
        recent_blockhash,
    );

    println!("\n{}", "Sending transaction...".dimmed());
    let signature = rpc_client
        .send_and_confirm_transaction(&transaction)
        .context("Failed to send CommitFill transaction")?;

    println!("\n{} {}", "✓ Match executed! Signature:".bright_green(), signature);
    println!("{}", "Note: Fill details written to receipt account".dimmed());

    Ok(())
}

/// Get order book state from a slab
///
/// Fetches and displays the current order book state.
///
/// # Arguments
/// * `config` - Network configuration
/// * `slab_address` - Slab pubkey as string
///
/// # Returns
/// * Ok(()) on success
pub async fn get_orderbook(
    config: &NetworkConfig,
    slab_address: String,
) -> Result<()> {
    println!("{}", "=== Order Book ===".bright_green().bold());
    println!("{} {}", "Network:".bright_cyan(), config.network);
    println!("{} {}", "Slab:".bright_cyan(), slab_address);

    // Parse slab address
    let slab_pubkey = Pubkey::from_str(&slab_address)
        .context("Invalid slab address")?;

    // Get RPC client
    let rpc_client = client::create_rpc_client(config);

    // Fetch account data
    let account = rpc_client
        .get_account(&slab_pubkey)
        .context("Failed to fetch slab account")?;

    // Verify ownership
    if account.owner != config.slab_program_id {
        anyhow::bail!("Account is not owned by slab program");
    }

    println!("\n{}", "=== Account Info ===".bright_yellow());
    println!("{} {} bytes", "Data Size:".bright_cyan(), account.data.len());
    println!("{} {} lamports", "Balance:".bright_cyan(), account.lamports);

    // Note: Full deserialization would require importing slab program types
    // For now, we just show the account exists and is owned correctly
    println!("\n{}", "Order book data:".bright_yellow());
    println!("{}", "  (Full deserialization requires slab program types)".dimmed());
    println!("{}", "  Account exists and is owned by slab program".bright_green());

    Ok(())
}

/// Halt trading on a slab
///
/// Only the LP owner can call this. When halted, all PlaceOrder and CommitFill
/// operations will be rejected with TradingHalted error.
///
/// # Arguments
/// * `config` - Network configuration
/// * `slab_address` - Slab pubkey as string
///
/// # Returns
/// * Ok(()) on success
pub async fn halt_trading(
    config: &NetworkConfig,
    slab_address: String,
) -> Result<()> {
    println!("{}", "=== Halt Trading ===".bright_red().bold());
    println!("{} {}", "Network:".bright_cyan(), config.network);
    println!("{} {}", "Slab:".bright_cyan(), slab_address);

    // Parse slab address
    let slab_pubkey = Pubkey::from_str(&slab_address)
        .context("Invalid slab address")?;

    // Use configured keypair (LP owner)
    let authority = &config.keypair;

    // Build instruction data: [discriminator=6]
    let mut instruction_data = Vec::with_capacity(1);
    instruction_data.push(6); // HaltTrading discriminator

    // Build halt instruction
    let halt_ix = Instruction {
        program_id: config.slab_program_id,
        accounts: vec![
            AccountMeta::new(slab_pubkey, false),         // Slab account (writable)
            AccountMeta::new_readonly(authority.pubkey(), true), // LP owner (signer)
        ],
        data: instruction_data,
    };

    // Create RPC client
    let rpc_client = client::create_rpc_client(config);

    // Get recent blockhash
    let recent_blockhash = rpc_client.get_latest_blockhash()
        .context("Failed to get recent blockhash")?;

    // Create and sign transaction
    let transaction = Transaction::new_signed_with_payer(
        &[halt_ix],
        Some(&authority.pubkey()),
        &[&authority],
        recent_blockhash,
    );

    // Send transaction
    let signature = rpc_client.send_and_confirm_transaction(&transaction)
        .context("Failed to send transaction")?;

    println!("{} {}", "✓ Trading halted".bright_green(), signature);
    Ok(())
}

/// Resume trading on a slab
///
/// Only the LP owner can call this. Restores normal trading operations.
///
/// # Arguments
/// * `config` - Network configuration
/// * `slab_address` - Slab pubkey as string
///
/// # Returns
/// * Ok(()) on success
pub async fn resume_trading(
    config: &NetworkConfig,
    slab_address: String,
) -> Result<()> {
    println!("{}", "=== Resume Trading ===".bright_green().bold());
    println!("{} {}", "Network:".bright_cyan(), config.network);
    println!("{} {}", "Slab:".bright_cyan(), slab_address);

    // Parse slab address
    let slab_pubkey = Pubkey::from_str(&slab_address)
        .context("Invalid slab address")?;

    // Use configured keypair (LP owner)
    let authority = &config.keypair;

    // Build instruction data: [discriminator=7]
    let mut instruction_data = Vec::with_capacity(1);
    instruction_data.push(7); // ResumeTrading discriminator

    // Build resume instruction
    let resume_ix = Instruction {
        program_id: config.slab_program_id,
        accounts: vec![
            AccountMeta::new(slab_pubkey, false),         // Slab account (writable)
            AccountMeta::new_readonly(authority.pubkey(), true), // LP owner (signer)
        ],
        data: instruction_data,
    };

    // Create RPC client
    let rpc_client = client::create_rpc_client(config);

    // Get recent blockhash
    let recent_blockhash = rpc_client.get_latest_blockhash()
        .context("Failed to get recent blockhash")?;

    // Create and sign transaction
    let transaction = Transaction::new_signed_with_payer(
        &[resume_ix],
        Some(&authority.pubkey()),
        &[&authority],
        recent_blockhash,
    );

    // Send transaction
    let signature = rpc_client.send_and_confirm_transaction(&transaction)
        .context("Failed to send transaction")?;

    println!("{} {}", "✓ Trading resumed".bright_green(), signature);
    Ok(())
}
