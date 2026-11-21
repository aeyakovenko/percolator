# Roadmap to a fully proved program

Generated 2026-11-21 from `proof_ledger.tsv`. **180 of 623 pieces of the deployed build are done
(170 proved by Aristotle, 10 verified by Kani); 0 of 89 charter invariants are closed.**

## The end goal

For one exact revision of the program and the engine, three theorems and a short list of things
that are assumed or measured. The scope is the program against its charter of invariants;
deployment is outside it.

1. **Safety.** A global invariant holds in every state the program can reach from `InitMarket`:
   the header checks (H), the per-asset scan (S), every portfolio account valid against the market
   (V), every market total equal to the sum over the accounts it mirrors (M), and custody, the vault
   token accounts holding what the header says (C). Every safety invariant of the charter is proved
   from it, and every instruction is proved to check its accounts and authorities and to move tokens
   by exactly what the engine booked.
2. **History.** The invariants about order, route, splitting and identity are proved as statements
   about two executions.
3. **Liveness.** In every non-terminal state a step is enabled, and every successful step lowers a
   fixed rank, so a user who is live can exit.
4. **Assumed or measured, in writing:** the Solana runtime, the SPL Token program, the oracle account
   formats, the prover's model of Rust; compute bounds and loop termination are measured, not
   proved.

An invariant is **closed** when its formal statement has been checked against the charter text by
someone other than its author and is proved from the layers below it.

**Oracles and authorities are adversaries inside the proofs.** Nothing is assumed about what an
oracle reports or what a market authority, asset admin, oracle authority, insurance or backing
authority signs: every theorem quantifies over their inputs. The charter already bounds what they
can do to a user's balance (INV-005 with its role matrix, INV-014, INV-045, INV-046, INV-064,
INV-018, the isolation invariants INV-034, INV-035 and INV-074, and the exit rule with INV-057,
INV-073 and INV-078), so proving those invariants is proving those bounds.

## Where we are

| Phase | What | Proved |
|---|---|---|
| 1 | Arithmetic and kernels | 171 of 171 |
| 2 | The engine cannot panic and keeps the header | 9 of 95 |
| 3 | The engine keeps the whole market state | 0 of 196 |
| 4 | The program's instructions | 0 of 56 |
| 5 | Each safety and history invariant closed | 0 of 71 |
| 6 | Liveness | 0 of 23 |
| 7 | Final check | 0 of 11 |

## Phase 0. Make every invariant provable

A statement that is false cannot be proved, and an open design question means the statement is not
fixed yet. These come first; most are decisions.

| State | Item | Why it blocks a proof |
|---|---|---|
| done | Funding across a report gap | Cadence is an input to funding, bounded by the rate cap times the notional per slot of the gap; INV-045 states it and the funded cadence test is restated (program #485). |
| done | Commit-time header check and the crank's commit after `NonProgress` | Program #485: the header is validated before every crank and RebalanceReduce commits, and a crank that accrued the market commits after `NonProgress` only if the engine moved no fee and no position. Part H holds at every commit by a runtime check; what is left of the `NonProgress` case is piece X3.guarded. |
| done | Program issue #382 (the owner's blocker) | Engine percolator#271, program #486: a per-domain provider-protection floor, read by `state::read_provider_protection`. Nothing about instruction counts is stored. The per-asset slot grows by 32 bytes: 5,492 assets per 10 MiB market instead of 5,585. Open design question: the floor records exposure (a winner cranked before its loser), as the issue specifies; charging only realized losses would be a different invariant. |
| done | Program issue #462 | Program #486: composite oracle legs enter the composition at twelve decimals and the composite is one exact ratio floored once (61,500 and 61,499 where main stored 61,500 and 61,351). |
| done | Program issues #403 and #455 | #403: documented that every rate and timer is per slot and what a shorter slot time changes (program #486); a timestamp-based time model is a redesign that was not done. #455: closed as not planned (authority-pushed funding is a new trust surface). |
| done | Remaining cases of engine issues #132 and #175 | Restated, not fixed: the engine specification lists the five cases in which close order can decide a payment before the payout snapshot, with measurements (1 of 8,000 random insolvent histories, 13 of 8,000 with unsettled price gaps; the difference stays in the vault), and INV-066 and INV-041 are scoped to the snapshot. Order independence before the snapshot is therefore not among the things to prove. |
| done | Clean baselines | Program: no failing test since #485. Engine (percolator#271): the audit-scan fixtures are repaired and the plain proofs that cannot verify in the contracts run are compiled out of it; every Kani layer is now as on master except the two harnesses that have no verdict there either (one per layer; four-hour runs of them alone were cut short and are still owed). |
| done | #449, #448, keeper reward during catch-up, #477, #453 | Landed 2026-11-19 and 2026-11-20. |
| done | Charter consistency | No two charter statements contradict each other (audit of 2026-11-19; the one real conflict it found, two base-unit mints against a single quote unit, was resolved by program #480 and #481). |

## The phases

**Phase 1. Arithmetic and kernels (171 of 171).** Done for the deployed build: 161 proved by Aristotle, 10 (the provider-protection kernels) verified by Kani for all inputs, one of the ten at 64-bit width. No more prover work goes here: Kani and the differential tests already check this layer.

**Phase 2. The engine cannot panic and keeps the header (9 of 95).**
- Every call into the wide-math module is outside its callee's panic condition (20 pieces; spec and
  prompt ready).
- Every entry point preserves the header checks: 33 that end in the runtime check, 9 small ones
  without a check, the four value-moving ones and the crank dispatcher as six local statements,
  and the four places where state is kept after an error. The first batch on the small ones proved
  two before its spending cap stopped it; those two have to be established again on the new engine
  revision, whose market view and asset slot gained fields.
- Exit: no engine call can panic, and no successful engine call can leave a header that fails.

**Phase 3. The engine keeps the whole market state (0 of 196).**
- Write part M of the global invariant, one line per mirrored total. H, S and V are functions of
  the code already.
- Prove for each of the 50 deployed entry points the other three halves: the per-asset scan, the
  validity of the accounts it is given, and the mirrored totals in their local form (the total
  minus the given accounts' contribution is unchanged).
- Prove the frame: an entry point cannot invalidate an account it is not given.
- Exit: every successful engine call preserves H, S, V and M.

**Phase 4. The program's instructions (0 of 56).**
- First a trial on one instruction (Deposit): whether the prover can take the program's code with
  the Solana types in it. If not, the instruction bodies are proved as functions over account bytes
  and the thin shell around them (account loading, signer flags, the token transfer) is covered by
  the written runtime assumptions. This is the largest unknown on the road; the trial runs
  as soon as the prover is free.
- Then, per instruction: accounts, signers and bindings checked before any write; tokens moved by
  the booked amount (part C); the global invariant preserved; errors abort.
- Exit: the induction closes. The global invariant holds in every reachable state.

**Phase 5. Each safety and history invariant closed (0 of 71).**
- One formal statement per invariant, read against the charter by a second reader, and its proof
  from the global invariant and the instruction properties. The seven history invariants need their
  own two-execution theorems. The two features that are switched off (INV-042, INV-043) are closed
  by proving they have no reachable consumer.

**Phase 6. Liveness (0 of 23).**
- A rank for each of the ten crank plans, the proof that the selector always returns an enabled
  plan in a non-terminal state, and from them the exit, progress and no-permanent-lock invariants.
  Depends on phase 0: the known stranding cases have to be gone first.

**Phase 7. Final check (0 of 11).**
- Every piece proved for one and the same revision: after the last code change the
  function-by-function comparison names what has to be proved again, and it is.
- The assumptions are written down; the compute bounds are measured at the maximum shape; each
  formal statement has been read against the charter text by someone other than its author.
- All tests and all three Kani layers are green with nothing tolerated.

## Order of work

The prover goes where Kani has gaps, business logic before arithmetic (the owner's direction of
2026-11-20). `specs/kani_gap_map_<rev>.md` measures the gap by the call graph: 62 deployed
functions and 14 of the 50 deployed entry points are reached by no Kani harness at all. They are
the execution of a trade (22 functions), recovery and forfeit (10), three crank steps with the
auto-crank dispatcher (5) and the terminal routes (9). A second, softer gap is code that a harness
reaches but may never execute (the refresh and the liquidation are reached only through harnesses
that prove something else); Kani's coverage mode measures that and its run is the second input.

1. Now: a pilot on the trade (the margin gate at its end, the application of one fill, the public
   entry point), for the margin rule, value conservation and the exact write set. It tells what
   such a proof costs.
2. Then the rest of the trade (the loop over fills with the position planning, the margin lien),
   recovery and forfeit, the crank steps, the terminal routes; then what the coverage run shows
   Kani never executes. In parallel, new Kani harnesses for the gaps that are only omissions: each
   goes to master when it verifies.
3. Then the Deposit trial and the program's instructions (phase 4), and phases 5 and 6.
4. Last: what Kani or a runtime check already covers (the header check of entry points that end
   in it, the call sites into the arithmetic).

Every engine change on the way costs a Kani run (about five hours) and re-proving the functions it
touches; the comparison tool keeps that to the changed functions.

## Cost and time

The 180 pieces done so far took about 2,300 prover credits, roughly 14 a piece, on small
functions. The 443 left are mostly whole entry points and instructions. The first batch of that
kind (eleven entry points in one task) proved two for about 1,560 credits before it was stopped, so
there is no supported estimate for the rest yet; the trade pilot is cut smaller to get one. The
prover runs one task at a time, so the floor is about two weeks of continuous running; with
statements to write, refuted attempts and fixes in between, four to eight weeks is a plan, not a
measurement.
