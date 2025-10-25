//! Insurance fund operations

use anyhow::Result;
use colored::Colorize;

use crate::config::NetworkConfig;

pub async fn add_funds(_config: &NetworkConfig, exchange: String, amount: u64) -> Result<()> {
    println!("{}", "=== Add to Insurance Fund ===".bright_green().bold());
    println!("{} {}", "Exchange:".bright_cyan(), exchange);
    println!("{} {} SOL", "Amount:".bright_cyan(), amount as f64 / 1e9);

    println!("\n{}", "Insurance funding not yet implemented".yellow());
    Ok(())
}

pub async fn show_balance(_config: &NetworkConfig, exchange: String) -> Result<()> {
    println!("{}", "=== Insurance Fund Balance ===".bright_green().bold());
    println!("{} {}", "Exchange:".bright_cyan(), exchange);

    println!("\n{}", "Balance: 0 SOL (not yet implemented)".dimmed());
    Ok(())
}

pub async fn show_history(_config: &NetworkConfig, exchange: String) -> Result<()> {
    println!("{}", "=== Insurance Fund History ===".bright_green().bold());
    println!("{} {}", "Exchange:".bright_cyan(), exchange);

    println!("\n{}", "No history available (not yet implemented)".dimmed());
    Ok(())
}
