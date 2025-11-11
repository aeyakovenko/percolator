# Separate state types from BPF program to common library

Fixes #[ISSUE_NUMBER]

## Summary

This PR moves state type definitions from `programs/router` to `programs/common` to follow Solana best practices and fix CLI build issues on host systems.

## Problem

The CLI failed to build because it depended on `percolator-router` for state types, but the router also contained BPF-specific code with Solana syscalls that can't link on macOS/Linux/Windows.

## Solution

Separate state types (data structures) from BPF program logic (instruction handlers), following the pattern used by Anchor and other mature Solana projects.

## Changes

### State Types Moved to `programs/common/src/state/`
- ✅ `vault.rs` - Vault account state
- ✅ `portfolio.rs` - User portfolio tracking
- ✅ `registry.rs` - Slab registry state
- ✅ `lp_bucket.rs` - LP bucket data structures
- ✅ `lp_seat.rs` - LP seat state
- ✅ `venue_pnl.rs` - Venue PnL tracking
- ✅ `insurance.rs` - Insurance fund state
- ✅ `pnl_vesting.rs` - PnL vesting logic

### Dependency Updates
- Added `model_safety` dependency to `programs/common/Cargo.toml`
- Removed `percolator-router` dependency from `cli/Cargo.toml`
- Router's `state/mod.rs` now re-exports from `percolator_common`
- Only `model_bridge.rs` remains in router (contains BPF-specific conversion logic)

### CLI Updates
- Updated all CLI files to import from `percolator_common::state::*`
- Files modified:
  - `cli/src/exchange.rs`
  - `cli/src/keeper.rs`
  - `cli/src/liquidation.rs`
  - `cli/src/margin.rs`
  - `cli/src/matcher.rs`
  - `cli/src/tests.rs`
  - `cli/src/trading.rs`

### Documentation
- Fixed README build command: `cargo build --release --bin percolator` → `cargo build --release -p percolator-cli`
- Updated Quick Start section
- Updated Building & Deployment section

## Benefits

1. ✅ **CLI builds without BPF dependencies** - Works on macOS, Linux, Windows
2. ✅ **State types are reusable** - Other programs can import from `percolator_common`
3. ✅ **Better separation of concerns** - Data structures vs. business logic
4. ✅ **Follows Solana best practices** - Same pattern as Anchor, Mango, Serum
5. ✅ **Easier testing** - State types can be tested without BPF environment
6. ✅ **Better SDK support** - Client libraries can use types without Solana runtime

## Testing

### Build Verification
```bash
# CLI builds successfully
$ cargo build --release -p percolator-cli
    Finished `release` profile [optimized] target(s)

# All BPF programs build successfully
$ cargo build-sbf
    Finished `release` profile [optimized] target(s)

# Common library tests pass
$ cargo test --lib -p percolator-common
test result: ok. 119 passed; 0 failed; 0 ignored; 0 measured
```

### Functionality Verification
- ✅ Router program builds for BPF
- ✅ Slab program builds for BPF
- ✅ AMM program builds for BPF
- ✅ CLI binary created successfully (6.8M)
- ✅ All existing tests pass
- ✅ No breaking changes to program logic

## Migration Notes

**For downstream consumers:**

If your code imports state types from `percolator-router`, update to:

```rust
// Before
use percolator_router::state::{Portfolio, SlabRegistry, Vault};

// After
use percolator_common::state::{Portfolio, SlabRegistry, Vault};
```

**Router still works as before:**
```rust
// In router code, both work (re-exported)
use crate::state::Portfolio;  // ✅ Still works
use percolator_common::state::Portfolio;  // ✅ Also works
```

## Checklist

- [x] State types moved to `programs/common/src/state/`
- [x] Dependencies updated in all `Cargo.toml` files
- [x] Router imports updated to re-export from common
- [x] CLI imports updated to use `percolator_common`
- [x] README updated with correct build command
- [x] CLI builds successfully on host system
- [x] All BPF programs build successfully
- [x] Tests pass
- [x] No breaking changes to program logic
- [x] Documentation updated

## Review Notes

This is a pure refactoring - **no logic changes**, just moving files and updating imports. The programs behave identically after this change.

Key files to review:
- `programs/common/src/state/mod.rs` - New state module
- `programs/router/src/state/mod.rs` - Now re-exports from common
- `cli/Cargo.toml` - Dependency changes
- `README.md` - Build command fix
