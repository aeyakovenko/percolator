//! Matcher/slab management operations

use anyhow::Result;
use colored::Colorize;

use crate::config::NetworkConfig;

pub async fn create_matcher(
    _config: &NetworkConfig,
    exchange: String,
    symbol: String,
    tick_size: u64,
    lot_size: u64,
) -> Result<()> {
    println!("{}", "=== Create Matcher ===".bright_green().bold());
    println!("{} {}", "Exchange:".bright_cyan(), exchange);
    println!("{} {}", "Symbol:".bright_cyan(), symbol);
    println!("{} {}", "Tick Size:".bright_cyan(), tick_size);
    println!("{} {}", "Lot Size:".bright_cyan(), lot_size);

    println!("\n{}", "Matcher creation not yet implemented".yellow());
    Ok(())
}

pub async fn list_matchers(_config: &NetworkConfig, exchange: String) -> Result<()> {
    println!("{}", "=== List Matchers ===".bright_green().bold());
    println!("{} {}", "Exchange:".bright_cyan(), exchange);

    println!("\n{}", "No matchers found (not yet implemented)".dimmed());
    Ok(())
}

pub async fn show_matcher_info(_config: &NetworkConfig, matcher: String) -> Result<()> {
    println!("{}", "=== Matcher Info ===".bright_green().bold());
    println!("{} {}", "Matcher:".bright_cyan(), matcher);

    println!("\n{}", "Matcher info not yet implemented".yellow());
    Ok(())
}
