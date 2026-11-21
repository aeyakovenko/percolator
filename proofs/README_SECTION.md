<!-- proofs:begin (generated from proofs/README_SECTION.md and the ledger; edit that file, not this block) -->
## Proof Status And How To Reproduce It

{{STATUS}}

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
  proved). `proofs/specs/kani_gap_map_{{REV}}.md` lists what no harness reaches. That code is where
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

### Expected Kani Results

On engine `{{REV}}`, Kani 0.66.0, with the three commands of "Run Kani":

| Run | Verified | Without a verdict |
|---|---|---|
{{KANI_ROWS}}

The table is from full runs. {{KANI_ADDED}} `proofs/kani_added.tsv` lists
them with their verdict in each run; add the ones marked verified to the numbers above.

A whole run takes one and a half to two hours on a 64-core machine at
`-j 12 --output-format terse`. Run them one after the other: the two harnesses without a verdict
use up to about 100 GB of memory each.

What the suite reaches and what it executes:

```bash
python3 proofs/kani_reach.py .                                        # functions no harness can reach
python3 proofs/kani_reach.py . --paths liquidate_account_not_atomic   # a call path from a harness
cargo kani --tests --features fuzz --coverage -Z source-coverage      # which code the harnesses execute
python3 proofs/kani_cov_report.py . {{REV}} target/kani               # report from those results
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
