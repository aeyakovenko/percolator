# Percolator Rust SDK

A high-performance, type-safe Rust SDK for building on top of **Percolator** — the formally verified risk engine for perpetual DEXs on Solana.

## Overview

[Percolator](https://github.com/aeyakovenko/percolator) handles the core "math" of a perpetual exchange: margin requirements, funding accruals, liquidations, and PnL socialization. 

This SDK allows developers to build:
- **Wrapper Programs**: Call into the Percolator engine using CPI.
- **Trading Bots**: Execute trades and manage positions via RPC.
- **Keepers**: Run maintenance cranks to keep the system healthy.
- **Frontends**: Fetch and decode complex engine state with zero-overhead casting.

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
percolator-sdk = { path = "../percolator-sdk" }
solana-sdk = "1.18"
solana-client = "1.18"
```

## Quick Start

### 1. Initialize the Client
```rust
use percolator_sdk::{PercolatorClient, Pubkey};

let rpc_url = "https://api.mainnet-beta.solana.com";
let program_id = Pubkey::from_str("YourWrapperProgramId1111111111111111111111")?;
let engine_state = Pubkey::from_str("YourEngineStateAccount1111111111111111111")?;

let client = PercolatorClient::new(rpc_url, program_id, engine_state);
```

### 2. Fetch Protocol State
```rust
let engine = client.get_engine_state()?;
println!("Total Open Interest: {}", engine.total_open_interest.get());
println!("Insurance Balance: {}", engine.insurance_fund.balance.get());

// Fetch a specific user's metrics
if let Some(user) = client.get_account(0)? {
    println!("User Capital: {}", user.capital.get());
    println!("User PnL: {}", user.pnl.get());
}
```

### 3. Build and Send Instructions
```rust
use solana_sdk::signature::Keypair;

let payer = Keypair::from_bytes(&[...])?;
let tx_sig = client.send_deposit(&payer, 0, 1_000_000, current_slot)?;
println!("Deposit Transaction: {}", tx_sig);
```

## Architecture

Percolator is designed as a `no_std` logic library. The SDK interacts with a **Wrapper Program** on-chain which forwards commands to the engine after validating signatures and oracle inputs.

### Instruction Layout
The SDK follows the standard Percolator wire format (Borsh-compatible):
- `0x00`: Deposit
- `0x01`: Withdraw
- `0x02`: ExecuteTrade
- `0x03`: KeeperCrank

### High-Performance Decoding
Unlike many Solana SDKs that rely on heavy serialization, the Percolator Rust SDK uses **raw memory casting** for `#[repr(C)]` engine state. This provides instantaneous decoding of the 4096-account slab, essential for high-frequency trading bots and real-time dashboards.

## License

Apache-2.0
