# Percolator

**Educational research project. Not production ready. Not audited. Do not use
with real funds.**

Normative protocol spec: [`spec.md`](spec.md), v16.9.0. The spec is frozen; this
README is only the short operator/auditor guide.

Percolator is a zero-copy perpetual-futures risk-engine library. It is built
around account-local progress: a crank or user action touches one bounded
portfolio view, not a global account table. Positive PnL is junior and source
domain aware; it can be used only to the extent the engine can prove realizable
backing or insurance for that domain.

## High-Level Model

- `MarketGroupV16ViewMut` is the production market view over a fixed asset slab.
- `PortfolioV16ViewMut` is the production portfolio view over bounded account
  storage.
- Asset 0 is the base asset. Assets 1..N are isolated source domains inside the
  same market group.
- Wrapper code owns account loading, authorization, oracle authentication, token
  movement, and CU policy. The engine owns risk state transitions and invariant
  checks.
- Failed engine calls are assumed to run under SVM transaction rollback: an
  `Err` must not commit partial mutation.

## What The Proofs Guarantee

The proof suite is a decomposition over production v16 code, not one monolithic
"prove the whole engine" query. The important guarantees are:

- **No value creation / no cross-domain steal:** value-bearing transitions are
  covered by token-flow checks, stock reconciliation, frame checks, or production
  kernel contracts. Source-domain credit and insurance are bounded to the domain
  that funds them.
- **Account-local safety:** provenance, active bitmap shape, stale/B-stale
  gates, margin gates, close state, and payout receipts are checked before
  favorable actions can commit.
- **Bounded progress:** crank, close, recovery, B-settlement, and payout progress
  are decomposed into bounded production kernels and public-route tests. A
  non-progressing path must either leave state unchanged or route to a terminal
  progress/recovery path.
- **Arithmetic discipline:** wide arithmetic that is too expensive for Kani is
  isolated behind small helpers and checked by differential/property tests under
  `--features fuzz`.

Trusted base and limits:

- SVM rollback on failed instructions.
- Wrapper correctness for auth, oracle normalization, account loading, token
  custody, and CU limits.
- Kani proves bounded symbolic harnesses and function contracts; large
  route-level properties are proven by decomposition plus runtime/fuzz tests.

## Current Verification Inventory

Source-checkable counts in this checkout:

| Class | Count |
| --- | ---: |
| Plain Kani proofs in `tests/proofs_v16.rs` | 244 |
| Plain Kani proofs in `tests/proofs_v16_arithmetic.rs` | 11 |
| Plain Kani proofs in `src/v16_proofs.rs` | 38 |
| Function-contract proofs in `src/v16_proofs.rs` | 51 |
| Production `kernel_*` helpers in `src/v16.rs` | 17 |
| Public `*_not_atomic` engine APIs in `src/v16.rs` | 55 |

Spot-check the inventory directly:

```bash
rg '^#\[kani::proof\]' tests/proofs_v16.rs tests/proofs_v16_arithmetic.rs src/v16_proofs.rs | wc -l
rg '^#\[kani::proof_for_contract' src/v16_proofs.rs | wc -l
rg 'pub\(crate\) fn kernel_' src/v16.rs | wc -l
rg 'pub fn [A-Za-z0-9_]+_not_atomic\(' src/v16.rs | wc -l
```

## Run Tests

```bash
cargo test
cargo test --features fuzz
```

`cargo test --features fuzz` includes the reference-model and property tests
that discharge the wide-arithmetic helper assumptions used by the Kani proofs.

## Run Kani

Install once:

```bash
cargo install --locked kani-verifier
cargo kani setup
```

Plain test proofs:

```bash
cargo kani --tests --features fuzz
```

Closure-layer plain proofs:

```bash
cargo kani --tests --features fuzz,closure
```

Function contracts and contract-gated kernel proofs:

```bash
cargo kani --tests --features fuzz,contracts -Z function-contracts
```

Single harness:

```bash
cargo kani --tests --features fuzz --harness HARNESS_NAME
```

The `contracts` run also compiles plain proofs under a different feature set, so
plain-proof `kani::cover!` reachability should be judged under
`--features fuzz`. Contract obligations should be judged under
`--features fuzz,contracts -Z function-contracts`.

For long audits, the shell runners in `scripts/` run harnesses in isolation and
kill stray Kani/CBMC processes between proofs:

```bash
bash scripts/isolated_full_audit.sh
bash scripts/contracts_runner.sh
FEATURES=fuzz,closure KANI_Z="" LOG_DIR=kani_closure CARGO_TARGET_DIR=target/closure bash scripts/contracts_runner.sh
```

## Repository Scope

This repository is the engine library. It does not define a deployed Solana
program, account registry, oracle adapter, or custody layer. Those are wrapper
responsibilities.

## License

Apache-2.0.
