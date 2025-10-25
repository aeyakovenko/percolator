//! Percolator CLI - Comprehensive testing and deployment tool
//!
//! This CLI provides end-to-end testing and deployment of the Percolator
//! perpetual exchange protocol on Solana networks (localnet, devnet, mainnet).

use clap::{Parser, Subcommand};
use colored::Colorize;
use std::path::PathBuf;

mod config;
mod client;
mod deploy;
mod exchange;
mod matcher;
mod liquidity;
mod trading;
mod margin;
mod liquidation;
mod insurance;
mod crisis;
mod keeper;

use config::NetworkConfig;

#[derive(Parser)]
#[command(name = "percolator")]
#[command(about = "Percolator Protocol CLI - Deploy and test perpetual exchange", long_about = None)]
#[command(version)]
struct Cli {
    /// Network to connect to (localnet, devnet, mainnet-beta)
    #[arg(short, long, default_value = "localnet")]
    network: String,

    /// RPC URL (overrides network default)
    #[arg(short, long)]
    url: Option<String>,

    /// Path to keypair file
    #[arg(short, long)]
    keypair: Option<PathBuf>,

    /// Verbose output
    #[arg(short, long)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Deploy programs to the network
    Deploy {
        /// Deploy router program
        #[arg(long)]
        router: bool,

        /// Deploy slab (matcher) program
        #[arg(long)]
        slab: bool,

        /// Deploy AMM program
        #[arg(long)]
        amm: bool,

        /// Deploy oracle program
        #[arg(long)]
        oracle: bool,

        /// Deploy all programs
        #[arg(long)]
        all: bool,

        /// Program keypair file (for upgradeable deploys)
        #[arg(long)]
        program_keypair: Option<PathBuf>,
    },

    /// Initialize a new perp exchange
    Init {
        /// Exchange name/identifier
        #[arg(short, long)]
        name: String,

        /// Insurance fund initial balance (lamports)
        #[arg(short, long, default_value = "1000000000")]
        insurance_fund: u64,

        /// Maintenance margin ratio (basis points)
        #[arg(short, long, default_value = "500")]
        maintenance_margin: u16,

        /// Initial margin ratio (basis points)
        #[arg(long, default_value = "1000")]
        initial_margin: u16,
    },

    /// Matcher/slab operations
    Matcher {
        #[command(subcommand)]
        command: MatcherCommands,
    },

    /// Liquidity operations
    Liquidity {
        #[command(subcommand)]
        command: LiquidityCommands,
    },

    /// Trading operations
    Trade {
        #[command(subcommand)]
        command: TradeCommands,
    },

    /// Margin operations
    Margin {
        #[command(subcommand)]
        command: MarginCommands,
    },

    /// Liquidation operations
    Liquidation {
        #[command(subcommand)]
        command: LiquidationCommands,
    },

    /// Insurance fund operations
    Insurance {
        #[command(subcommand)]
        command: InsuranceCommands,
    },

    /// Crisis simulation and testing
    Crisis {
        #[command(subcommand)]
        command: CrisisCommands,
    },

    /// Keeper operations
    Keeper {
        #[command(subcommand)]
        command: KeeperCommands,
    },

    /// Run end-to-end test suite
    Test {
        /// Run quick smoke tests only
        #[arg(long)]
        quick: bool,

        /// Run crisis haircut tests
        #[arg(long)]
        crisis: bool,

        /// Run liquidation tests
        #[arg(long)]
        liquidations: bool,

        /// Run all tests
        #[arg(long)]
        all: bool,
    },

    /// Show protocol status and statistics
    Status {
        /// Exchange address
        exchange: String,

        /// Show detailed statistics
        #[arg(short, long)]
        detailed: bool,
    },
}

#[derive(Subcommand)]
enum MatcherCommands {
    /// Create a new matcher/slab
    Create {
        /// Exchange address
        exchange: String,

        /// Market symbol (e.g., BTC-USD)
        symbol: String,

        /// Tick size (price increment)
        #[arg(long)]
        tick_size: u64,

        /// Lot size (quantity increment)
        #[arg(long)]
        lot_size: u64,
    },

    /// List all matchers for an exchange
    List {
        /// Exchange address
        exchange: String,
    },

    /// Show matcher details
    Info {
        /// Matcher address
        matcher: String,
    },
}

#[derive(Subcommand)]
enum LiquidityCommands {
    /// Add liquidity to a matcher
    Add {
        /// Matcher address
        matcher: String,

        /// Amount to add (in base currency)
        amount: u64,

        /// Price for the liquidity
        price: Option<f64>,
    },

    /// Remove liquidity from a matcher
    Remove {
        /// Matcher address
        matcher: String,

        /// Amount to remove
        amount: u64,
    },

    /// Show liquidity provider positions
    Show {
        /// Optional user address (defaults to CLI keypair)
        user: Option<String>,
    },
}

#[derive(Subcommand)]
enum TradeCommands {
    /// Place a limit order
    Limit {
        /// Matcher address
        matcher: String,

        /// Buy or sell
        side: String,

        /// Order price
        price: f64,

        /// Order size
        size: u64,

        /// Post-only order
        #[arg(long)]
        post_only: bool,
    },

    /// Place a market order
    Market {
        /// Matcher address
        matcher: String,

        /// Buy or sell
        side: String,

        /// Order size
        size: u64,
    },

    /// Cancel an order
    Cancel {
        /// Order ID
        order_id: String,
    },

    /// List open orders
    Orders {
        /// Optional user address (defaults to CLI keypair)
        user: Option<String>,
    },

    /// Show order book
    Book {
        /// Matcher address
        matcher: String,

        /// Number of levels to show
        #[arg(short, long, default_value = "10")]
        depth: usize,
    },
}

#[derive(Subcommand)]
enum MarginCommands {
    /// Initialize portfolio account for the user
    Init,

    /// Deposit collateral
    Deposit {
        /// Amount in lamports
        amount: u64,

        /// Token mint address
        #[arg(long)]
        token: Option<String>,
    },

    /// Withdraw collateral
    Withdraw {
        /// Amount in lamports
        amount: u64,

        /// Token mint address
        #[arg(long)]
        token: Option<String>,
    },

    /// Show margin account
    Show {
        /// Optional user address (defaults to CLI keypair)
        user: Option<String>,
    },

    /// Calculate margin requirements
    Requirements {
        /// User address
        user: String,
    },
}

#[derive(Subcommand)]
enum LiquidationCommands {
    /// Trigger liquidation for an account
    Execute {
        /// User address to liquidate
        user: String,

        /// Max position size to liquidate
        #[arg(long)]
        max_size: Option<u64>,
    },

    /// List liquidatable accounts
    List {
        /// Exchange address
        exchange: String,
    },

    /// Show liquidation history
    History {
        /// Number of recent liquidations to show
        #[arg(short, long, default_value = "10")]
        limit: usize,
    },
}

#[derive(Subcommand)]
enum InsuranceCommands {
    /// Add funds to insurance fund
    Fund {
        /// Exchange address
        exchange: String,

        /// Amount in lamports
        amount: u64,
    },

    /// Show insurance fund balance
    Balance {
        /// Exchange address
        exchange: String,
    },

    /// Show insurance fund usage history
    History {
        /// Exchange address
        exchange: String,
    },
}

#[derive(Subcommand)]
enum CrisisCommands {
    /// Simulate crisis scenario
    Simulate {
        /// Exchange address
        exchange: String,

        /// Deficit amount to simulate
        deficit: u64,

        /// Dry run (don't execute)
        #[arg(long)]
        dry_run: bool,
    },

    /// Show crisis history
    History {
        /// Exchange address
        exchange: String,
    },

    /// Test haircut calculation
    TestHaircut {
        /// Total deficit
        deficit: u64,

        /// Warming PnL balance
        warming_pnl: u64,

        /// Insurance fund balance
        insurance: u64,

        /// Total equity
        equity: u64,
    },
}

#[derive(Subcommand)]
enum KeeperCommands {
    /// Start keeper bot
    Run {
        /// Exchange address to monitor
        exchange: String,

        /// Check interval in seconds
        #[arg(short, long, default_value = "5")]
        interval: u64,

        /// Only monitor, don't execute
        #[arg(long)]
        monitor_only: bool,
    },

    /// Show keeper statistics
    Stats {
        /// Exchange address
        exchange: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let cli = Cli::parse();

    // Initialize network configuration
    let config = NetworkConfig::new(
        &cli.network,
        cli.url.clone(),
        cli.keypair.clone(),
    )?;

    if cli.verbose {
        println!("{} {}", "Network:".bright_cyan(), config.network);
        println!("{} {}", "RPC URL:".bright_cyan(), config.rpc_url);
        println!("{} {}", "Keypair:".bright_cyan(), config.keypair_path.display());
    }

    // Execute command
    match cli.command {
        Commands::Deploy { router, slab, amm, oracle, all, program_keypair } => {
            deploy::deploy_programs(&config, router, slab, amm, oracle, all, program_keypair).await?;
        }
        Commands::Init { name, insurance_fund, maintenance_margin, initial_margin } => {
            exchange::initialize_exchange(&config, name, insurance_fund, maintenance_margin, initial_margin).await?;
        }
        Commands::Matcher { command } => {
            match command {
                MatcherCommands::Create { exchange, symbol, tick_size, lot_size } => {
                    matcher::create_matcher(&config, exchange, symbol, tick_size, lot_size).await?;
                }
                MatcherCommands::List { exchange } => {
                    matcher::list_matchers(&config, exchange).await?;
                }
                MatcherCommands::Info { matcher } => {
                    matcher::show_matcher_info(&config, matcher).await?;
                }
            }
        }
        Commands::Liquidity { command } => {
            match command {
                LiquidityCommands::Add { matcher, amount, price } => {
                    liquidity::add_liquidity(&config, matcher, amount, price).await?;
                }
                LiquidityCommands::Remove { matcher, amount } => {
                    liquidity::remove_liquidity(&config, matcher, amount).await?;
                }
                LiquidityCommands::Show { user } => {
                    liquidity::show_positions(&config, user).await?;
                }
            }
        }
        Commands::Trade { command } => {
            match command {
                TradeCommands::Limit { matcher, side, price, size, post_only } => {
                    trading::place_limit_order(&config, matcher, side, price, size, post_only).await?;
                }
                TradeCommands::Market { matcher, side, size } => {
                    trading::place_market_order(&config, matcher, side, size).await?;
                }
                TradeCommands::Cancel { order_id } => {
                    trading::cancel_order(&config, order_id).await?;
                }
                TradeCommands::Orders { user } => {
                    trading::list_orders(&config, user).await?;
                }
                TradeCommands::Book { matcher, depth } => {
                    trading::show_order_book(&config, matcher, depth).await?;
                }
            }
        }
        Commands::Margin { command } => {
            match command {
                MarginCommands::Init => {
                    margin::initialize_portfolio(&config).await?;
                }
                MarginCommands::Deposit { amount, token } => {
                    margin::deposit_collateral(&config, amount, token).await?;
                }
                MarginCommands::Withdraw { amount, token } => {
                    margin::withdraw_collateral(&config, amount, token).await?;
                }
                MarginCommands::Show { user } => {
                    margin::show_margin_account(&config, user).await?;
                }
                MarginCommands::Requirements { user } => {
                    margin::show_margin_requirements(&config, user).await?;
                }
            }
        }
        Commands::Liquidation { command } => {
            match command {
                LiquidationCommands::Execute { user, max_size } => {
                    liquidation::execute_liquidation(&config, user, max_size).await?;
                }
                LiquidationCommands::List { exchange } => {
                    liquidation::list_liquidatable(&config, exchange).await?;
                }
                LiquidationCommands::History { limit } => {
                    liquidation::show_history(&config, limit).await?;
                }
            }
        }
        Commands::Insurance { command } => {
            match command {
                InsuranceCommands::Fund { exchange, amount } => {
                    insurance::add_funds(&config, exchange, amount).await?;
                }
                InsuranceCommands::Balance { exchange } => {
                    insurance::show_balance(&config, exchange).await?;
                }
                InsuranceCommands::History { exchange } => {
                    insurance::show_history(&config, exchange).await?;
                }
            }
        }
        Commands::Crisis { command } => {
            match command {
                CrisisCommands::Simulate { exchange, deficit, dry_run } => {
                    crisis::simulate_crisis(&config, exchange, deficit, dry_run).await?;
                }
                CrisisCommands::History { exchange } => {
                    crisis::show_history(&config, exchange).await?;
                }
                CrisisCommands::TestHaircut { deficit, warming_pnl, insurance, equity } => {
                    crisis::test_haircut_calculation(deficit, warming_pnl, insurance, equity)?;
                }
            }
        }
        Commands::Keeper { command } => {
            match command {
                KeeperCommands::Run { exchange, interval, monitor_only } => {
                    keeper::run_keeper(&config, exchange, interval, monitor_only).await?;
                }
                KeeperCommands::Stats { exchange } => {
                    keeper::show_stats(&config, exchange).await?;
                }
            }
        }
        Commands::Test { quick, crisis: test_crisis, liquidations, all } => {
            println!("{}", "Running test suite...".bright_green().bold());
            if all || quick {
                println!("\n{}", "=== Smoke Tests ===".bright_yellow());
                // Run smoke tests
            }
            if all || test_crisis {
                println!("\n{}", "=== Crisis Haircut Tests ===".bright_yellow());
                // Run crisis tests
            }
            if all || liquidations {
                println!("\n{}", "=== Liquidation Tests ===".bright_yellow());
                // Run liquidation tests
            }
        }
        Commands::Status { exchange, detailed } => {
            exchange::query_registry_status(&config, exchange, detailed).await?;
        }
    }

    Ok(())
}
