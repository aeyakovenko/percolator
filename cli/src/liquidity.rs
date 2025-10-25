//! Liquidity provider operations

use anyhow::Result;
use colored::Colorize;

use crate::config::NetworkConfig;

pub async fn add_liquidity(
    _config: &NetworkConfig,
    matcher: String,
    amount: u64,
    price: Option<f64>,
) -> Result<()> {
    println!("{}", "=== Add Liquidity ===".bright_green().bold());
    println!("{} {}", "Matcher:".bright_cyan(), matcher);
    println!("{} {}", "Amount:".bright_cyan(), amount);
    if let Some(p) = price {
        println!("{} {}", "Price:".bright_cyan(), p);
    }

    println!("\n{}", "Add liquidity not yet implemented".yellow());
    Ok(())
}

pub async fn remove_liquidity(
    _config: &NetworkConfig,
    matcher: String,
    amount: u64,
) -> Result<()> {
    println!("{}", "=== Remove Liquidity ===".bright_green().bold());
    println!("{} {}", "Matcher:".bright_cyan(), matcher);
    println!("{} {}", "Amount:".bright_cyan(), amount);

    println!("\n{}", "Remove liquidity not yet implemented".yellow());
    Ok(())
}

pub async fn show_positions(_config: &NetworkConfig, user: Option<String>) -> Result<()> {
    println!("{}", "=== Liquidity Positions ===".bright_green().bold());
    if let Some(u) = user {
        println!("{} {}", "User:".bright_cyan(), u);
    }

    println!("\n{}", "No positions found (not yet implemented)".dimmed());
    Ok(())
}
