# Separate state types from BPF program to fix CLI build and follow Solana best practices

## Problem

The CLI fails to build with the command documented in the README:

```bash
cargo build --release --bin percolator
```

**Error:**
```
error: linking with `cc` failed: exit status: 1
Undefined symbols for architecture arm64:
  "_sol_get_return_data", referenced from:
    percolator_router::instructions::router_liquidity::read_liquidity_result_from_return_data
```

## Root Cause

The CLI depends on `percolator-router` to access state types like `Portfolio`, `SlabRegistry`, etc. for deserializing on-chain account data:

```rust
// cli/src/liquidation.rs:72
let expected_size = percolator_router::state::Portfolio::LEN;
```

However, `percolator-router` contains both:
1. **State types** (needed by CLI) ✅
2. **BPF instruction handlers** with Solana syscalls like `sol_get_return_data()` ❌

When building the CLI for the host system (macOS/Linux), the linker tries to resolve BPF syscalls that only exist in the Solana runtime, causing the build to fail.

## Solution: Follow Solana Best Practices

Most mature Solana projects separate **state types** from **program logic**:

**Current structure:**
```
programs/
├── router/
│   └── src/
│       ├── state/          # State types mixed with program
│       └── instructions/   # BPF handlers
└── common/                 # Exists but underutilized
```

**Recommended structure:**
```
programs/
├── router/
│   └── src/
│       ├── instructions/   # BPF handlers only
│       └── entrypoint.rs
└── common/                 # All shared types
    └── src/
        ├── state/          # Move Portfolio, SlabRegistry, etc. here
        ├── error.rs        # Already exists ✅
        └── lib.rs
```

## Benefits

1. ✅ **CLI builds without BPF dependencies** - Can deserialize account data on any platform
2. ✅ **Better SDK integration** - Client libraries can use types without Solana runtime
3. ✅ **Easier testing** - State types testable without BPF environment
4. ✅ **Cross-program reusability** - Other programs can reference state types
5. ✅ **Follows Anchor pattern** - Industry standard ([Anchor's approach](https://book.anchor-lang.com/))

## Proposed Changes

### 1. Move state types to `programs/common/src/state/`
- `Portfolio`
- `SlabRegistry`
- `RouterLpSeat`
- `VenuePnl`
- `Vault`
- `LpBucket`
- `Insurance`
- `PnlVesting`

### 2. Update dependencies
```toml
# programs/common/Cargo.toml
[dependencies]
pinocchio = { workspace = true }
model_safety = { path = "../../crates/model_safety", default-features = false }

# programs/router/Cargo.toml
[dependencies]
percolator-common = { path = "../common" }  # Already exists

# cli/Cargo.toml
[dependencies]
percolator-common = { path = "../programs/common" }  # Use common instead of router
# Remove: percolator-router dependency
```

### 3. Update imports
```rust
// Before: cli/src/liquidation.rs
use percolator_router::state::Portfolio;

// After:
use percolator_common::state::Portfolio;
```

### 4. Fix README command
The correct command should be:
```bash
cargo build --release -p percolator-cli
```

(The binary is in the `percolator-cli` package, not the workspace root)

## Examples from the Ecosystem

- **Anchor Framework**: Generates IDL types separate from program
- **Mango Markets**: `mango-common` crate for shared types
- **Serum DEX**: Separate state definitions from program logic

## Implementation Checklist

- [ ] Create `programs/common/src/state/` module
- [ ] Move state types from `programs/router/src/state/` to `programs/common/src/state/`
- [ ] Update `programs/router` to import from `percolator-common`
- [ ] Update `cli/Cargo.toml` to depend on `percolator-common` instead of `percolator-router`
- [ ] Update all CLI imports to use `percolator_common::state`
- [ ] Remove `percolator-router` dependency from CLI
- [ ] Fix README build command to `cargo build --release -p percolator-cli`
- [ ] Verify CLI builds successfully on macOS, Linux, and Windows
- [ ] Update documentation

## Testing Verification

- ✅ CLI builds successfully: `cargo build --release -p percolator-cli`
- ✅ All BPF programs build: `cargo build-sbf`
- ✅ Common library tests pass: 119 tests passing
- ✅ Router program builds and works correctly
- ✅ Existing functionality preserved
