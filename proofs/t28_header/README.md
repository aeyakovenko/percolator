# Checking the deposit header postcondition

On a normal return from `deposit_not_atomic`, a market that passed the default
`validate_shape()` before the call must still pass it if the deposit returned
`Ok(())`. This directory contains two local checks for the
[`T28.header` obligation](../proof_ledger.tsv). The reference engine revision
is `421f31908328846823af8fea676a8ff51528ee83`.

The zero-amount path matters. After the read-only portfolio check, it returns
`Ok(())` before the first store. The positive path can write to both accounts.
After its last write and the final portfolio check, it returns the result of
`self.validate_shape()` directly. A successful positive deposit therefore
supports a fresh header check even without the initial-header precondition.
The regression in `tests/v16_spec_tests.rs` exercises zero, exactly the vault
cap, and the first unit over the cap.

## Generic control flow in Lean

With Python 3, Rust 1.95.0, and Lean 4.33.1 installed under Elan, run from
the repository root:

~~~sh
proofs/t28_header/check.sh
~~~

The script emits the default-feature library's textual MIR. The extractor
pins `src/v16.rs`, the deposit MIR and the default header validator MIR,
checks the mutable validator wrapper
for direct forwarding, and emits a graph with 45 blocks. It accounts for
all four explicit stores, explores every switch successor, and discards
knowledge of header validity at each store and opaque accounting call.
Lean checks a graph traversal and its soundness theorem. Two rejected graph
variants model a store before the early success and an unchecked success
on the late path. [Theorem and controls](Deposit.lean) · [Semantics](MirContracts.lean)

The theorem quantifies over traces satisfying the stated callee contracts.
Those contracts include preservation of header validity by the shared-view
operations, propagation of `Err` by `FromResidual`, and the success meaning
of the final validator on the same market. The proof is about normal control
flow. Its trusted translation boundary includes rustc and the MIR extractor.

To use the [Agave platform-tools v1.58](https://github.com/anza-xyz/platform-tools/releases/tag/v1.58)
Linux x86-64 distribution, set `PLATFORM_TOOLS` to the extracted directory.
The SBF compiler marks unwind edges unreachable. On this revision that is the
only difference in the deposit and forwarding wrapper MIR from the host
dumps. Both produce the same Lean input.

~~~sh
work=$(mktemp -d)
RUSTC="$PLATFORM_TOOLS/rust/bin/rustc" CARGO_TARGET_DIR="$work/target" \
  "$PLATFORM_TOOLS/rust/bin/cargo" rustc --locked --lib --jobs 1 \
  --target sbpf-solana-solana -- --emit="mir=$work/percolator.mir"
proofs/t28_header/check.sh --mir "$work/percolator.mir"
~~~

## Checking a fresh validator call in LLVM IR

The two [Rust probes](ir/probe.rs) return true only on a counterexample.
They call the real default-feature deposit and validator with input references
and a caller-supplied positive-path amount. For each probe the checker
requires at least one return and requires the operand of every return to be
the literal `i1 false` after standard LLVM passes. It supplies an inlining
hint and lets LLVM infer memory effects. A control build writes zero to
`next_market_id` after a successful deposit. The driver requires rejection
of **both** control probes. [Return checker](ir/check_returns.cpp)

The IR driver needs Linux x86-64, Rust 1.95.0's LLVM 22.1 shared library,
a C++17 compiler and LLVM C API headers. Set `LLVM_C_INCLUDE` if the
headers are outside `/usr/lib/llvm-18/include`. Cargo uses a locked scratch
crate and prints the working directory. The default layout is `Market<()>`.
The full matrix covers `()`, `u8`, the 38 byte-array types with in-tree
`MarketWrapperPod` implementations, and the integration fixture's `u64`.

~~~sh
proofs/t28_header/ir/check.sh --all-layouts
PLATFORM_TOOLS=/path/to/platform-tools proofs/t28_header/ir/check.sh --all-layouts
cargo test --locked --test v16_spec_tests v16_deposit_t28_header_success_paths_and_vault_cap
~~~

The second command cross-compiles the probes for `sbpf-solana-solana` and
checks their target-specific IR on the host. The inference back to Rust
assumes defined execution, valid references, and correct compiler and LLVM
optimizations. Execution of a deployed Solana program, custody, rollback on
errors, and the separate aggregate-mirror obligation require other evidence.
