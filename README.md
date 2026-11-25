# Percolator

**Educational research project. Not production ready. Not audited. Do not use
with real funds.**

Normative protocol spec: [`spec.md`](spec.md), v16.9.1. The spec is frozen; this
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
- **Exact side exposure:** each side keeps the sum of its legs' raw basis and
  derives its ADL index `A` from that sum and its effective open interest after
  every position change and every haircut. A side's exposure therefore never
  exceeds its matched open interest and is short of it by less than a tenth of
  a raw unit, whatever the history; a same-side reduction is rounded against
  the reducing leg, and an index that rises retires every health certificate
  issued before it (`spec.md` §5, "Side A index"; percolator-prog #449).
- **Provider losses are recorded once:** each source domain keeps the lowest
  provider protection it has had (provider principal that neither a claim nor an
  unrepaid consumption has taken), moved only by the provider's own deposits and
  withdrawals. A loss that a loser later refills stays recorded, and spending the
  refill again down to that low records nothing, so a program that keeps
  depositor shares over a provider position charges each loss exactly once
  whenever it looks. Read it through `provider_protection_floor_num`; the stored
  value is brought up to date once per mutable view, and nothing about views is
  stored (`spec.md` §2.2, "Provider-protection floor"; percolator-prog #382).
- **Arithmetic discipline:** wide arithmetic that is too expensive for Kani is
  isolated behind small helpers and checked by differential/property tests under
  `--features fuzz`.

Trusted base and limits:

- SVM rollback on failed instructions.
- Wrapper correctness for auth, oracle normalization, account loading, token
  custody, and CU limits.
- Kani proves bounded symbolic harnesses and function contracts; large
  route-level properties are proven by decomposition plus runtime/fuzz tests.
- No Kani proof executes a trade: applying one fill to two accounts exceeds the
  solver budget even with concrete inputs (the "INTRACTABLE-TIER" notes in
  `tests/proofs_v16.rs`, which record the same for the full account refresh
  and for the full resolved close). For the trade, the rebalance reduction,
  the paired Recovery close and the Recovery forfeit, Kani proves gates: a
  call that must be refused is refused with its documented error and leaves
  the market and the accounts unchanged, and a refused trade restores the
  loss-stale flag it had suspended. For a trade that covers the market mode,
  the size guards, an unconfigured asset and an empty batch; a request that
  fails only the price or fee guards, and an over-long batch, could not be
  shown end to end (they reach the fill) and rest on the contract of the
  guard summary. What those routes do once they run rests on the component
  proofs, the tests and the fuzzing.
- Payments of a resolved market do not depend on the order of the claims once
  the payout snapshot is captured. Before that, the order of the permissionless
  closes can still decide what one account is paid, in five cases that
  `spec.md` lists with measurements (§5.1.1, "What close order can still
  decide"; #132, #175). No account is kept from closing. This is measured over
  random histories, not proved.

## Current Verification Inventory

Source-checkable counts in this checkout:

| Class | Count |
| --- | ---: |
| Plain Kani proofs in `tests/proofs_v16.rs` | 364 |
| Plain Kani proofs in `tests/proofs_v16_arithmetic.rs` | 12 |
| Plain Kani proofs in `src/v16_proofs.rs` | 75 |
| Function-contract proofs in `src/v16_proofs.rs` | 99 |
| Production `kernel_*` helpers in `src/v16.rs` | 69 |
| Public `*_not_atomic` engine APIs in `src/v16.rs` | 63 |

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

In the `contracts` run Kani 0.66 also asserts every contract inside the plain
proofs that call a contracted function directly. Forty-seven plain proofs cannot
pass there and are compiled out of that run (`#[cfg(not(feature = "contracts"))]`
in `tests/proofs_v16.rs`, with the reasons at the top of that file): forty-six
call a kernel that takes a record by value as `mut`, where the `ensures` closure
reads the argument after the body has mutated it, and one passes inputs outside a
`requires` on purpose. All forty-seven verify in the main run, and the contracts
they touch are verified for all inputs by their `contract_check_*` harnesses. With
them out, every harness of the contracts run is expected to verify.

Two lib harnesses have not produced a verdict within a one-hour harness timeout
on any tree measured so far and count as unverified, not as passing:
`composition_positive_to_negative_pnl_consumes_complete_source_lien_under_rate_axiom`
in the closure run and `composition_clear_leg_value_conservation` in the
contracts run.
Each was also run alone with a four-hour limit on `d1fa39f6` (2026-11-21), and
both timed out.

For long audits, the shell runners in `scripts/` run harnesses in isolation and
kill stray Kani/CBMC processes between proofs:

```bash
bash scripts/isolated_full_audit.sh
bash scripts/contracts_runner.sh
FEATURES=fuzz,closure KANI_Z="" LOG_DIR=kani_closure CARGO_TARGET_DIR=target/closure bash scripts/contracts_runner.sh
```

<!-- proofs:begin (generated from proofs/README_SECTION.md and the ledger; edit that file, not this block) -->
## Proof Status And How To Reproduce It

As of 2026-11-25, for engine `7a712baa` and program `121d73ae`: **384 of 826 pieces of the deployed build are done (215 proved by Aristotle, 122 verified by Kani, 47 proved by a local rule checked on the source; 337 without the rule); 0 of 89 charter invariants are closed.**

| Phase | What | Proved |
|---|---|---|
| 1 | Arithmetic and kernels | 250 of 250 |
| 2 | The engine cannot panic or overflow and keeps the header | 126 of 195 |
| 3 | The engine keeps the whole market state | 8 of 220 |
| 4 | The program's instructions | 0 of 56 |
| 5 | Each safety and history invariant closed | 0 of 71 |
| 6 | Liveness | 0 of 23 |
| 7 | Final check | 0 of 11 |

`proofs/ROADMAP.md` is the plan and `proofs/proof_ledger.md` lists every piece, one row each, with
its evidence. The ledger says for every piece which of three kinds of evidence it has:

| Kind | What it establishes | Where it lives | Re-run |
|---|---|---|---|
| **Kani** (bounded model checking of the Rust source) | A harness's assertions hold for every value of its symbolic inputs, on the fixture the harness builds. | `tests/proofs_v16.rs`, `tests/proofs_v16_arithmetic.rs`, `src/v16_proofs.rs` | Locally, see "Run Kani". |
| **Aristotle** ([Harmonic](https://aristotle.harmonic.fun): proofs in Lean about the compiled Rust) | A stated property holds of the deployed function for all inputs and all starting states. | Statements in `proofs/aristotle/statements/`, task texts in `proofs/prompts/`, property specs in `proofs/specs/` | With an account, see below. |
| **Tests and fuzzing** | Behaviour on the cases run. | `tests/` | Locally, see "Run Tests". |

What the numbers do not say:

- No charter invariant is closed yet. A proved piece is a piece of an invariant.
- No Kani proof executes a trade, and the proof file records the trade, the full account refresh
  and the full resolved close as beyond the solver budget (a resolved close of a flat account is
  proved). `proofs/specs/kani_gap_map_7a712baa.md` lists what no harness reaches. That code is where
  the Aristotle work is aimed now.
- An Aristotle proof is about one function at one revision. A proof made on an earlier revision
  counts for this one only when a function-by-function comparison shows the function, everything
  it calls and every type it uses unchanged (`proofs/changed_fns.py`,
  `proofs/specs/changed_since_*.md`). Entry-point proofs whose types changed are listed as "to be
  established again".
- The Aristotle proofs so far are of pure functions: the arithmetic and the kernels. The two
  attempts on methods that hold the market and an account (a batch of small entry points and a
  pilot on the trade) proved 3 of 32 properties. The cost was the byte-level model of the
  9,419-byte account and the borrow bookkeeping of large bodies, not the logic.
  `proofs/ROADMAP.md` lists the ways forward.
- Nothing proves termination or compute bounds; those are measured.
- Every proof was made with overflow checks on. The deployed program checks its own arithmetic
  but builds the engine without overflow checks (with them, the 14-leg atomic exit exceeds the
  compute limit), so an overflow in the engine would continue with a wrapped value. A no-panic proof
  rules that out; an "if it returns" statement holds on-chain only together with the `PO.*` pieces,
  one per engine function with unchecked arithmetic
  (`proofs/specs/engine_wrapping_arithmetic_7a712baa.md`).

### Expected Kani Results

On engine `7a712baa`, Kani 0.66.0, with the three commands of "Run Kani":

| Run | Verified | Without a verdict |
|---|---|---|
| `--features fuzz` | 377 | none |
| `--features fuzz,closure` | 425 | `composition_positive_to_negative_pnl_consumes_complete_source_lien_under_rate_axiom` |
| `--features fuzz,contracts -Z function-contracts` | 454 | `composition_clear_leg_value_conservation` |

How the table was obtained for this revision. Engine `7a712baa` removed a field from the market header type, so all three runs were made again in full on its source (every harness then in the source, the proofs added one by one since `71076137` included): 377, 426 and 455 harnesses. The full runs were made at `-j 12` in a container capped at 160 GB, which is less than the heaviest proofs need when twelve run at once: 18 of them ran out of memory there (7 in the main run, 9 in the closure run, 2 in the contracts run; no check failed) and were run again four at a time under a 240 GB cap, where every one verified. Every harness has a verdict except the two named in the table, which have none on earlier revisions either. A harness named `proof_v16_adjust_u128_applies_exact_delta_or_fails_closed` exists in two modules and is counted twice. Since then 36 proofs were added and verified one by one: 0 with `--features fuzz`, 36 in the closure run, 0 in the contracts run. `proofs/kani_added.tsv` lists them with their verdict in each run; add the ones marked verified to the numbers above.

A whole run takes one and a half to two hours on a 64-core machine at
`-j 12 --output-format terse`. Run them one after the other: the two harnesses without a verdict
use up to about 100 GB of memory each.

What the suite reaches and what it executes:

```bash
python3 proofs/kani_reach.py .                                        # functions no harness can reach
python3 proofs/kani_reach.py . --paths liquidate_account_not_atomic   # a call path from a harness
cargo kani --tests --features fuzz --coverage -Z source-coverage      # which code the harnesses execute
python3 proofs/kani_cov_report.py . 7a712baa target/kani               # report from those results
```

### Reproduce The Aristotle Proofs

In `proofs/`:

- `prompts/stage_*.txt`: the exact text of every task that was run.
- `specs/`: the property specifications the tasks refer to, generated from the source by the
  `gen_*.py` scripts.
- `aristotle/statements/`: the Lean statements Aristotle wrote (`Spec/*.lean`; they import
  `SepLogic.Public`, the vocabulary that comes with every Aristotle project download). Each ledger
  row names its declaration, for example `Spec.KernelC.kernel_side_adl_index_contract`. Reading a
  statement against the Rust function is how to check that the right thing was proved.
- `aristotle/summaries/`: Aristotle's report of each finished task, with every assumption a proof
  makes about a library function.
- `properties.json`: the service's list of properties and their status; `make_ledger.py` builds
  the ledger from it.

The proof terms are not here. Aristotle keeps them and reports a property as proved after its own
final check: the proof builds with no `sorry`, proves the published statement and uses the standard
axioms only. Reproducing therefore means running the same task on the same source and comparing
the verdicts. A new run is a new proof search; it is expected to reach the same verdicts, not
guaranteed to.

```bash
pipx install aristotlelib && aristotle login          # an account and credits are needed

# 1. A clean export of the engine at the task's revision, with the specs under aristotle/
git worktree add /tmp/engine-export <revision>
mkdir -p /tmp/engine-export/aristotle
cp proofs/specs/wide_math_spec.md proofs/specs/kernel_contracts_spec.md /tmp/engine-export/aristotle/

# 2. The first task creates the project
aristotle submit --project-dir /tmp/engine-export --prompt-file proofs/prompts/stage_a1_loop_free.txt --json

# 3. Each later task continues the same project, one at a time
export ARISTOTLE_PROJECT=<project id printed by step 2>
aristotle continue "$ARISTOTLE_PROJECT" --prompt-file proofs/prompts/stage_b2_scalar_kernels.txt \
    --files /tmp/engine-export/aristotle/kernel_contracts_spec.md --project-dir /tmp/engine-export \
    --json --no-questions        # --files: changed or new files, paths under the project folder

# 4. Read the verdicts and compare them with proofs/proof_ledger.md
aristotle properties "$ARISTOTLE_PROJECT" --status ALL --json
aristotle download "$ARISTOTLE_PROJECT" --destination /tmp/aristotle-result
```

`proofs/submit_batch.sh` and `proofs/watch_run.sh` wrap steps 3 and 4; `proofs/stage_rev.sh`
prepares the export for a new engine revision.

| Task text | Engine revision | Result |
|---|---|---|
| `stage_a1_loop_free.txt` | `add1d027` | 4 of 4 proved: the 128-bit widening product, the 256-bit product, and that neither division returns on a zero divisor. |
| `stage_b1_five_kernels.txt`, `stage_b1_finish.txt` | `add1d027` | 2 kernels proved (contract and no panic); 3 left for a later task. |
| `stage_b2_scalar_kernels.txt` | `add1d027` | 8 kernel contracts proved. The 8 "no panic" properties were worded as "the call returns", which cannot be stated; restated in the next task. |
| `stage_b3_restate_nopanic_and_eleven_kernels.txt` | `add1d027` | 11 more kernels and the 8 restated properties proved. |
| `stage_b4_lien_and_backing_kernels.txt` | `add1d027` | 13 of 16 kernels proved. The other 3 exist only under `cfg(any(kani, feature = "fuzz"))`; they were proved on that build and are listed as not in the deployed build. |
| `stage_b5_remaining_kernels.txt` | `add1d027` | All 34 properties proved. |
| `stage_a2_loop_free_arithmetic.txt` | `add1d027` | Every loop-free arithmetic property proved. |
| `stage_a3_division.txt` | `add1d027` | The two long-division loops and the rest of the arithmetic: 45 of 45. One property refuted as predicted: `wide_signed_mul_div_floor` wraps when the exact result is at least 2^255. The engine does not call that function. |
| `stage_b6_side_index_revision.txt` | `c030aa40` | All 20 properties proved: the two side-index kernels with their exposure and progress properties, and every kernel that the revision changed. |
| `stage_t1_small_unchecked_entry_points.txt` | `c030aa40` | Stopped before it finished. 2 of its 11 properties proved (`advance_resolved_slot_not_atomic`, `grow_asset_slot_capacity_not_atomic`); the others have no verdict. |

| `stage_s2_trade_pilot.txt` | `d1fa39f6` | Stopped by itself. 21 properties of the trade stated (the margin gate, the application of one fill, the entry point, nine callee contracts): 1 proved (the side-cap gate, a kernel), 1 proved under two unverified callee assumptions, 19 open. It corrected three of the statements and answered one question from the source (`aristotle/summaries/s2.md`). |

`stage_a0_calibration.txt` was stopped without a result, and a first pilot on two settlement
methods was cancelled before it stated anything. The other files in `proofs/prompts/` are prepared
and not yet run.

Regenerate the ledger and the roadmap (the engine checkout must be at the revision the ledger
names; `transition_kinds.py` stops if a deployed entry point changed since it was last reviewed):

```bash
cd proofs
LEDGER_WRAPPER_SRC=/path/to/percolator-prog/src/v16_program.rs ./regen.sh                 # from properties.json
LEDGER_WRAPPER_SRC=... ARISTOTLE_PROJECT=<id> ARISTOTLE_API_KEY=<key> ./regen.sh --online  # from the service
```
<!-- proofs:end -->

## Repository Scope

This repository is the engine library. It does not define a deployed Solana
program, account registry, oracle adapter, or custody layer. Those are wrapper
responsibilities.

## License

Apache-2.0.
