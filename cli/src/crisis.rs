//! Crisis haircut simulation and testing

use anyhow::Result;
use colored::Colorize;
use model_safety::crisis::{Accums, crisis_apply_haircuts};

use crate::config::NetworkConfig;

pub async fn simulate_crisis(
    _config: &NetworkConfig,
    exchange: String,
    deficit: u64,
    dry_run: bool,
) -> Result<()> {
    println!("{}", "=== Simulate Crisis Scenario ===".bright_green().bold());
    println!("{} {}", "Exchange:".bright_cyan(), exchange);
    println!("{} {} lamports", "Deficit:".bright_cyan(), deficit);
    println!("{} {}", "Dry Run:".bright_cyan(), if dry_run { "Yes" } else { "No" });

    if dry_run {
        println!("\n{}", "Dry run mode - no transactions will be sent".yellow());
    }

    println!("\n{}", "Crisis simulation not yet implemented".yellow());
    println!("{}", "This will:".dimmed());
    println!("  {} Apply haircuts via loss waterfall", "├─".dimmed());
    println!("  {} Update global scale factors", "├─".dimmed());
    println!("  {} Log crisis event", "└─".dimmed());

    Ok(())
}

pub async fn show_history(_config: &NetworkConfig, exchange: String) -> Result<()> {
    println!("{}", "=== Crisis History ===".bright_green().bold());
    println!("{} {}", "Exchange:".bright_cyan(), exchange);

    println!("\n{}", "No crisis events (not yet implemented)".dimmed());
    Ok(())
}

pub fn test_haircut_calculation(
    deficit: u64,
    warming_pnl: u64,
    insurance: u64,
    equity: u64,
) -> Result<()> {
    println!("{}", "=== Test Haircut Calculation ===".bright_green().bold());
    println!("\n{}", "Input Parameters:".bright_yellow());
    println!("  {} {} lamports", "Deficit:".bright_cyan(), deficit);
    println!("  {} {} lamports", "Warming PnL:".bright_cyan(), warming_pnl);
    println!("  {} {} lamports", "Insurance:".bright_cyan(), insurance);
    println!("  {} {} lamports", "Equity:".bright_cyan(), equity);

    // Use the actual crisis module to calculate haircuts
    let mut accums = Accums::new();
    accums.sigma_principal = equity as i128;
    accums.sigma_collateral = (equity as i128) - (deficit as i128);
    accums.sigma_insurance = insurance as i128;

    println!("\n{}", "Running crisis haircut calculation...".dimmed());
    
    let outcome = crisis_apply_haircuts(&mut accums);

    println!("\n{}", "Results:".bright_yellow());
    println!("  {} {}", "Warming PnL burned:".bright_cyan(), outcome.burned_warming);
    println!("  {} {}", "Insurance drawn:".bright_cyan(), outcome.insurance_draw);
    println!("  {} {:.6}%", "Equity haircut ratio:".bright_cyan(), (outcome.equity_haircut_ratio.0 as f64 / (1u128 << 64) as f64) * 100.0);
    println!("  {} {}", "Is solvent:".bright_cyan(), if outcome.is_solvent { "Yes" } else { "No" });

    let total_covered = outcome.burned_warming + outcome.insurance_draw;
    if total_covered < deficit as i128 {
        let remaining = deficit as i128 - total_covered;
        println!("\n  {} {} lamports haircut from equity", "⚠️".yellow(), remaining);
    } else {
        println!("\n  {} No equity haircut required", "✓".green());
    }

    Ok(())
}
