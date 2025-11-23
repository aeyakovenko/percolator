# Program Keypairs Directory

This directory contains the keypair files for all Percolator programs. These keypairs determine the on-chain program IDs.

**IMPORTANT:** Keypair files are **NOT** committed to version control. Each organization/developer must generate their own keypairs locally.

## Required Keypair Files

You need to generate the following keypairs in this directory:

| Program | Keypair File |
|---------|--------------|
| Router  | `percolator_router-keypair.json` |
| Slab    | `percolator_slab-keypair.json` |
| AMM     | `percolator_amm-keypair.json` |
| Oracle  | `percolator_oracle-keypair.json` |

## Quick Setup

Generate all required keypairs with these commands:

```bash
# From the project root directory
solana-keygen new -o keys/percolator_router-keypair.json --no-bip39-passphrase --force
solana-keygen new -o keys/percolator_slab-keypair.json --no-bip39-passphrase --force
solana-keygen new -o keys/percolator_amm-keypair.json --no-bip39-passphrase --force
solana-keygen new -o keys/percolator_oracle-keypair.json --no-bip39-passphrase --force
```

Then update your `.percolator-local.toml` with the generated program IDs:

```bash
# Get the program IDs
echo "router_program_id = \"$(solana-keygen pubkey keys/percolator_router-keypair.json)\""
echo "slab_program_id = \"$(solana-keygen pubkey keys/percolator_slab-keypair.json)\""
echo "amm_program_id = \"$(solana-keygen pubkey keys/percolator_amm-keypair.json)\""
echo "oracle_program_id = \"$(solana-keygen pubkey keys/percolator_oracle-keypair.json)\""
```

Copy the output and update your `.percolator-local.toml` file accordingly.

## Usage

### Getting a Program ID
```bash
solana-keygen pubkey keys/percolator_<program>-keypair.json
```

### Deploying with a Keypair
```bash
solana program deploy target/deploy/percolator_<program>.so \
  --program-id keys/percolator_<program>-keypair.json \
  --url <network>
```

## Security Notes

### All Keypairs are Local
- **No keypairs are committed to version control** - each organization/developer generates their own
- This ensures each deployment is independent and secure
- Keypair files in this directory are ignored by git (see `keys/.gitignore`)

### Production Keypairs
For production/mainnet deployments, store keypairs securely:
- Hardware wallets (Ledger, etc.)
- Secret management systems (AWS Secrets Manager, HashiCorp Vault)
- Encrypted storage with restricted access
- Multi-signature wallets for critical programs

### Development/Testing
For localnet/devnet/testnet:
- Generate fresh keypairs for each environment
- Keep separate keypairs per network
- Use descriptive naming: `percolator_<program>-<network>-keypair.json`
- Never reuse development keypairs for production

## Why This Directory?

Previously, keypairs were stored in `target/deploy/`, which is problematic because:
1. `target/` is a build artifact directory that gets cleaned with `cargo clean`
2. `target/` is typically in `.gitignore`, making keypairs easy to lose
3. Inconsistent across machines and developers

By moving keypairs to a dedicated `keys/` directory:
1. They survive `cargo clean` operations
2. Development keypairs can be committed for team consistency
3. Clear separation between dev and production keypairs via `.gitignore`
