#!/usr/bin/env python3
"""make_roadmap.py > ROADMAP.md
The road from the current proof ledger to the end goal: every charter invariant closed for the
deployed revision. Counts come from proof_ledger.tsv (run make_ledger.py first); the list of
decisions and fixes in phase 0 is kept by hand in PHASE0 below."""
import csv, os, re, datetime
HERE = os.path.dirname(os.path.abspath(__file__))
rows = list(csv.DictReader(open(os.path.join(HERE, "proof_ledger.tsv")), delimiter="\t", quoting=csv.QUOTE_NONE))
ND = "not in the deployed build"
dep = [r for r in rows if r["status"] != ND]
DONE = ("proved", "verified by Kani", "proved by rule")   # by Aristotle, by a Kani harness (the row names it and its
# input width), or by a local rule checked on the source at every regen (the row names the rule and the lines)
def sel(pred):
    ps = [r for r in dep if pred(r["id"])]
    return sum(1 for r in ps if r["status"] in DONE), len(ps)
# the header pieces of entry points whose Ok paths end in the runtime check (classes final and delegates)
_HDR_TAIL = [r for r in dep if r["id"].endswith(".header") and r["statement"].startswith(("[final]", "[delegates]"))]
_HDR_TAIL_N = len(_HDR_TAIL)
_HDR_TAIL_DONE = sum(1 for r in _HDR_TAIL if r["status"] in DONE)
_HDR_TAIL_KANI = sum(1 for r in _HDR_TAIL if r["status"] == "verified by Kani")
# the header pieces of entry points that end in no check (classes local and frame)
_HDR_SMALL = [r for r in dep if r["id"].endswith(".header") and r["statement"].startswith(("[local]", "[frame]"))]
_HDR_SMALL_N = len(_HDR_SMALL)
_HDR_SMALL_DONE = sum(1 for r in _HDR_SMALL if r["status"] in DONE)
_HDR_TAIL_RULE = sum(1 for r in _HDR_TAIL if r["status"] == "proved by rule")
_HDR_TAIL_HOW = (f"{_HDR_TAIL_DONE} of them done: their path structure, "
                 + " and ".join(x for x in (f"{_HDR_TAIL_KANI} verified by Kani with every callee replaced" if _HDR_TAIL_KANI else "",
                                            f"{_HDR_TAIL_RULE} proved by a rule checked on the source" if _HDR_TAIL_RULE else "") if x))


def frac(pred):
    d, n = sel(pred)
    return f"{d} of {n}"
kind_of = {}
for r in rows:
    if r["id"].startswith("I") and r["layer"].startswith("I"):
        m = re.search(r"\(([^)]*)\) holds", r["statement"])
        kind_of[r["id"]] = m.group(1) if m else "safety"
live = {i for i, k in kind_of.items() if "liveness" in k or "termination" in k}
evid = {i for i, k in kind_of.items() if k in ("process", "compute")}
PH = [
    ("1", "Arithmetic and kernels", lambda i: re.match(r"[A-G]\d", i) is not None and not i.startswith("G.") or i.startswith("K")),
    ("2", "The engine cannot panic or overflow and keeps the header", lambda i: i.startswith("P") or re.match(r"T\d+\.header$", i) is not None or i.startswith("TR.") or i.startswith("X")),
    ("3", "The engine keeps the whole market state", lambda i: re.match(r"T\d+\.(scan|account|mirror)$", i) is not None or re.match(r"S\d+\.", i) is not None or re.match(r"T[FAB]\.|TC\d+$", i) is not None or i.startswith("AC.") or i in ("G.M", "G.other")),
    ("4", "The program's instructions", lambda i: re.match(r"W\d", i) is not None or i in ("G.C", "G.init", "G.reach", "Y.runtime", "Y.spl", "Y.oracle")),
    ("5", "Each safety and history invariant closed", lambda i: i in kind_of and i not in live and i not in evid),
    ("6", "Liveness", lambda i: i.startswith("L") or i in live),
    ("7", "Final check", lambda i: i in evid or i in ("Y.tool", "Y.compute", "Y.terminates", "Y.statements")),
]
seen = set()
for _, _, pred in PH:
    for r in dep:
        if pred(r["id"]):
            assert r["id"] not in seen, r["id"]
            seen.add(r["id"])
missing = [r["id"] for r in dep if r["id"] not in seen]
assert not missing, f"pieces in no phase: {missing[:8]}"
total_d = sum(1 for r in dep if r["status"] in DONE)
total_kani = sum(1 for r in dep if r["status"] == "verified by Kani")
total_rule = sum(1 for r in dep if r["status"] == "proved by rule")
closed = sum(1 for r in dep if r["id"] in kind_of and r["status"] == "proved")
# decisions and fixes without which some invariant cannot be proved as written; edit by hand
PHASE0 = [
    ("done", "Funding across a report gap", "Cadence is an input to funding, bounded by the rate cap times the notional per slot of "
     "the gap; INV-045 states it and the funded cadence test is restated (program #485)."),
    ("done", "Commit-time header check and the crank's commit after `NonProgress`", "Program #485: the header is validated before "
     "every crank and RebalanceReduce commits, and a crank that accrued the market commits after `NonProgress` only if the engine "
     "moved no fee and no position. Part H holds at every commit by a runtime check; what is left of the `NonProgress` case is "
     "piece X3.guarded."),
    ("done", "Program issue #382 (the owner's blocker)", "Engine percolator#271, program #486: a per-domain provider-protection floor, "
     "read by `state::read_provider_protection`. Nothing about instruction counts is stored. The per-asset slot grows by 32 bytes: "
     "5,492 assets per 10 MiB market instead of 5,585. Open design question: the floor records exposure (a winner cranked before "
     "its loser), as the issue specifies; charging only realized losses would be a different invariant."),
    ("done", "Program issue #462", "Program #486: composite oracle legs enter the composition at twelve decimals and the composite "
     "is one exact ratio floored once (61,500 and 61,499 where main stored 61,500 and 61,351)."),
    ("done", "Program issues #403 and #455", "#403: documented that every rate and timer is per slot and what a shorter slot time "
     "changes (program #486); a timestamp-based time model is a redesign that was not done. #455: closed as not planned "
     "(authority-pushed funding is a new trust surface)."),
    ("done", "Remaining cases of engine issues #132 and #175", "Restated, not fixed: the engine specification lists the five cases "
     "in which close order can decide a payment before the payout snapshot, with measurements (1 of 8,000 random insolvent "
     "histories, 13 of 8,000 with unsettled price gaps; the difference stays in the vault), and INV-066 and INV-041 are scoped to "
     "the snapshot. Order independence before the snapshot is therefore not among the things to prove."),
    ("done", "Clean baselines", "Program: no failing test since #485. Engine (percolator#271): the audit-scan fixtures are repaired "
     "and the plain proofs that cannot verify in the contracts run are compiled out of it; every Kani layer is now as on master "
     "except the two harnesses that have no verdict there either (one per layer; each was run alone with a four-hour limit on "
     "2026-11-21 and both timed out)."),
    ("done", "#449, #448, keeper reward during catch-up, #477, #453", "Landed 2026-11-19 and 2026-11-20."),
    ("done", "Charter consistency", "No two charter statements contradict each other (audit of 2026-11-19; the one real conflict it found, two base-unit mints against a single quote unit, was resolved by program #480 and #481)."),
    ("done", "INV-046 first independent read (2026-11-23)", "Two program defects fixed in program aac89b9e: a reduction on a "
     "Recovery asset whose mark follows trades reverted on any print off the frozen mark (the trade staged a target the "
     "engine refuses for a Recovery asset); `BatchTradeNoCpi` dropped the oracle-profile write-back of legs whose print does "
     "not move the mark, after their zero-move funding segment. The statement is restated (specs/inv046_closure_71076137.md) "
     "and needs a second read."),
    ("done", "INV-045: catch-up after idle slots (third read, finding 3)", "The owner chose (b) on 2026-11-23, program "
     "f6d0bc57: a target change that nobody pays for reaches back at most one window (`max_accrual_dt_slots`). PushAuthMark and "
     "PushEwmaMark refuse a changed target on an exposed asset further behind, and the crank holds such a reading back; trade "
     "prints are unchanged and pay the mark-movement fee. Measured before: +48% after 200 idle slots at 24 bps. New clause "
     "C11 of the INV-045 statement, pieces AC.catchup_push and AC.catchup_crank (test evidence)."),
    ("done", "INV-064 wording", "Reworded to the code on 2026-11-23 (the owner's decision, program 609e557b): one budget per "
     "asset, the asset's two loss-domain budgets, never more than the unreserved insurance or the vault, debited by every "
     "withdrawal; no Live withdrawal while any loss is active, stale or lagging; no proportional cap and no cooldown."),
    ("done, one case open", "INV-077: a fill at the full source shape with a maintenance fee", "Engine 06b7e5be, program "
     "81a8a8ec: a fee sync with nothing to settle keeps a current, healthy certificate; the fill that opens the LP's "
     "fourteenth leg costs 1,118,713 CU and the full-shape maintenance test runs again. Not measured: an account at the full "
     "shape whose fee leaves it liquidatable (Y.compute, AUDIT-077)."),
    ("done", "Engine issue #274: crank order and the B share", "An outside report, reproduced and fixed clean-room (engine "
     f"263c34d4, program 0cb5794e): the crank's B chunk now settles the account's pending K/F first, so crank order no longer "
     "decides what a backing provider or a winner keeps (before: a provider got back 1 of 300,000 atoms; a winner was paid "
     "9,471,279 instead of 11,000,000; through the program two atoms ended owned by nobody). Piece AC.settle_b_kf_first."),
    ("open, owner decision", "Gain valuation of an account that is already insolvent (issue #274 residual)", "An account "
     "insolvent when a gain arrives is valued against the support its gain's source domain holds at that moment, so whether "
     "its refresh comes before or after its loser pays in still changes its terminal value; with a provider's bucket on "
     "that domain lapsed, a winner refreshed first lost its whole 852,131 gain and the domain's backing went to its "
     "withdrawer. Needs a rule (for example: value the gain once its loser has paid, or never let the order decide), then "
     "a proof. Piece AC.insolvent_gain_order."),
    ("done, third read open", "INV-046 second read (2026-11-23 and 2026-11-24)", "Findings 1 to 5 fixed in program "
     "257db166. Findings 6 and 7 fixed in program 121d73ae as the owner decided: every trade route values its fee at the "
     "engine's effective price, at most the signed rate (CPI: the smaller of the taker's rate and the LP's matcher cap), "
     "so an oracle authority's average no longer sets what a trade costs (before: closing one unit took 965,500,035 atoms "
     "from an unsigned LP); and a soft-stale hybrid's switch to its trade average is no longer held back, so the first "
     "writer does not choose the mark's path. A third read of the corrected clauses remains. Pieces AC.fee_basis and "
     "AC.quiet_hybrid_switch (test evidence)."),
    ("done", "The dead threshold-stress flag", "Engine 7a712baa, program 121d73ae: `threshold_stress_active` was read as a "
     "market-wide lock on withdrawals and conversions, but nothing ever set it (a default-only control, INV-087). The "
     "owner had it removed from the engine header and the wrapper."),
]
now = datetime.datetime.utcnow().strftime("%Y-%m-%d")  # the date only: a publish with no change leaves no diff
_p1_done, _p1_all = sel(PH[0][2])
_p1_kani = sum(1 for r in dep if PH[0][2](r["id"]) and r["status"] == "verified by Kani")
if _p1_done == _p1_all:
    PHASE1_NOTE = (f"Done for the deployed build: {_p1_done - _p1_kani} proved by Aristotle, {_p1_kani} (the provider-protection "
                   "kernels) verified by Kani for all inputs, one of the ten at 64-bit width. No more prover work goes here: Kani and the differential tests "
                   "already check this layer.")
else:
    PHASE1_NOTE = f"{_p1_all - _p1_done} pieces open."
print(f"""# Roadmap to a fully proved program

Generated {now} from `proof_ledger.tsv`. **{total_d} of {len(dep)} pieces of the deployed build are done
({total_d - total_kani - total_rule} proved by Aristotle, {total_kani} verified by Kani, {total_rule} proved by a local rule checked on the source; {total_d - total_rule} without the rule); {closed} of {len(kind_of)} charter invariants are closed.**

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
|---|---|---|""")
for num, name, pred in PH:
    print(f"| {num} | {name} | {frac(pred)} |")
print(f"""
## Phase 0. Make every invariant provable

A statement that is false cannot be proved, and an open design question means the statement is not
fixed yet. These come first; most are decisions.

| State | Item | Why it blocks a proof |
|---|---|---|""")
for st, item, why in PHASE0:
    print(f"| {st} | {item} | {why} |")
print(f"""
## The phases

**Phase 1. Arithmetic and kernels ({frac(PH[0][2])}).** {PHASE1_NOTE}

**Phase 2. The engine cannot panic or overflow and keeps the header ({frac(PH[1][2])}).**
- Every call into the wide-math module is outside its callee's panic condition (20 pieces; spec and
  prompt ready).
- Every entry point preserves the header checks: {_HDR_TAIL_N} that end in the runtime check ({_HDR_TAIL_HOW}),
  {_HDR_SMALL_N} small ones without a check ({_HDR_SMALL_DONE} of them verified by Kani: the predicate itself
  before and after the call, with its costly pure parts memoized),
  the four value-moving ones and the crank dispatcher as six local statements,
  and the four places where state is kept after an error.
- The engine's unchecked arithmetic, `PO.*` ({sum(1 for r in dep if r['id'].startswith('PO.'))} pieces): the
  deployed program checks its own arithmetic but builds the engine without overflow checks (with
  them, the 14-leg atomic exit exceeds the compute limit). Every proof was made with checks on, so a
  statement of the form "if the call returns" holds on-chain only where the operations it runs
  cannot overflow. One piece per engine function whose unchecked `+`, `-`, `*`, shift or negation no
  proved no-panic piece covers (`specs/engine_wrapping_arithmetic_<rev>.md`). Many are loop counters
  and differences guarded just above them; a piece can also close by turning the operation into
  checked arithmetic.
- Exit: no engine call can panic or overflow, and no successful engine call can leave a header that fails.

**Phase 3. The engine keeps the whole market state ({frac(PH[2][2])}).**
- Write part M of the global invariant, one line per mirrored total. H, S and V are functions of
  the code already.
- Prove for each of the 50 deployed entry points the other three halves: the per-asset scan, the
  validity of the accounts it is given, and the mirrored totals in their local form (the total
  minus the given accounts' contribution is unchanged).
- Prove the frame: an entry point cannot invalidate an account it is not given.
- Exit: every successful engine call preserves H, S, V and M.

**Phase 4. The program's instructions ({frac(PH[3][2])}).**
- The trial on one instruction (Deposit, task w0 on 2026-11-22, 34 credits) answered the largest
  unknown on the road. The prover takes the program's code, Solana types included, from source:
  `AccountInfo` and its `RefCell` borrows, SPL Token decoding and the engine views. Only the runtime's
  system calls are assumed: program-address derivation, `invoke_signed`, `realloc` and `Clock::get`.
  The cost is not the Solana types. It is the absence of undefined behaviour on every run, which every
  result includes. So even the smallest binding property of one handler pays for the safety of
  everything that handler reaches, its engine entry point included (`aristotle/summaries/w0.md`).
- So the instruction properties are composed, not proved one handler at a time. First come contracts,
  proved once: for the shared helpers (the account checks, the config read, the two views, the token
  verifiers, the token transfer, the sequence binding) and for each engine entry point a handler calls,
  which phases 2 and 3 produce. Then, per instruction: accounts, signers and bindings checked before
  any write; tokens moved by the booked amount (part C); the global invariant preserved; errors abort.
- Exit: the induction closes. The global invariant holds in every reachable state.

**Phase 5. Each safety and history invariant closed ({frac(PH[4][2])}).**
- One formal statement per invariant, read against the charter by a second reader, and its proof
  from the global invariant and the instruction properties. The seven history invariants need their
  own two-execution theorems. The two features that are switched off (INV-042, INV-043) are closed
  by proving they have no reachable consumer.

**Phase 6. Liveness ({frac(PH[5][2])}).**
- A rank for each of the ten crank plans, the proof that the selector always returns an enabled
  plan in a non-terminal state, and from them the exit, progress and no-permanent-lock invariants.
  Depends on phase 0: the known stranding cases have to be gone first.

**Phase 7. Final check ({frac(PH[6][2])}).**
- Every piece proved for one and the same revision: after the last code change the
  function-by-function comparison names what has to be proved again, and it is.
- The assumptions are written down; the compute bounds are measured at the maximum shape; each
  formal statement has been read against the charter text by someone other than its author.
- All tests and all three Kani layers are green with nothing tolerated.

## Order of work

The prover goes where Kani has gaps, business logic before arithmetic (the owner's direction of
2026-11-20). `specs/kani_gap_map_<rev>.md` measures the gap by the call graph: at `d1fa39f6`, 62
deployed functions and 14 of the 50 deployed entry points were reached by no Kani harness: the
execution of a trade, recovery and forfeit, three crank steps with the auto-crank dispatcher, and
the terminal routes. The proof file itself records the trade, the full refresh and the full
resolved close as beyond Kani's budget.

**What the trade pilot showed (2026-11-21).** The first Aristotle task on that code stated 21
properties of the trade (the margin gate, the application of one fill, the entry point, nine callee
contracts) and stopped by itself after about 410 credits with one proved, and that one is a
kernel. With the earlier batch on small entry points that makes 3 of 32 properties about methods
that hold the market and an account, for about 1,970 credits. The prover's own account of the cost
(`aristotle/summaries/s2.md`): the byte-level model of the 9,419-byte account, bodies with many
`?` exits, and shared borrows that have to be given back on every exit. It is not the logic. The
pilot was still useful: it corrected three of the statements and answered a question about the
order of the margin checks from the source.

**So the plan for phases 2 to 4 had to change. Of three ways forward, the owner chose the first on 2026-11-21:**

1. Move the logic out of the methods into pure kernels over small records. The engine already has
   this pattern: a kernel decides, the method reads fields, calls it and writes fields. A kernel
   costs about 14 credits a piece and Kani verifies it for all inputs. What remains in a method is
   a short read-call-write shell, the only thing left to prove at the byte level. This is a
   refactor with no change of behaviour, checked by the existing suites and oracles.
2. Keep the code and prove a model: write each entry point's transition as a pure function over
   the runtime records, prove the invariants of the model, and tie model and code together by
   differential tests. Cheaper and weaker: the link is tested, not proved.
3. Keep paying for byte-level proofs of whole methods. On the numbers so far, not affordable.

**The refactor so far.** Engine `7a712baa` (the INV-077 fee-sync fix, the issue #274 crank fix and the removal of the dead threshold-stress flag on top of `71076137`) carries batches 1 to 13 (batches 10 and 11 add proofs only; batch 13 splits the accrual entry points so that their price check is proved for every price), batch 14 (no-overflow proofs) and batch 15 (a staged target restarts the price-cap anchor, INV-045): the arithmetic of the
trade fee, a loss paid from capital, the initial-margin lien need, the liquidation deficit, the
incremental certificate, the residual reward credit, the PnL totals and bounds, the start of a
bankrupt-close ledger, the residual, both equity measures, the gain-against-loss rule, the
capital-backed loss, the Resolved loss booking, the terminal refill, the loss split, the claim burn,
the account side of a new lien, the stranded loser backing, one B-settlement step, one
bankruptcy-residual step and the accrual's price cap, funding and K/F index update is in kernels
K64 to K93; the methods only read fields, call them and write the results. Kani verifies fourteen
of the twenty batch 3-9 kernels for every input; the other six it checks for bounded inputs, and the
prover has proved all six at full width (K76 to K78 in task k4, where K78's contract as written
turned out false in a corner the code handles right and was restated). Each batch lands only when the engine's suites pass, its differential oracles are
byte-identical to the previous revision, the Kani harnesses that can reach the changed code verify,
and the program's full gate passes against it. The work also found and closed one hole: a market
could be created with a public B chunk large enough that every B-settlement step on a side
overflowed after a social loss, freezing its positions. Admission first refused a chunk above
MAX_VAULT_TVL; batch 12 then made the chunk the constant MAX_VAULT_TVL, so no market chooses it. What remains is mostly loops that read and write state as they go (the loss
settlement, the lien consumption, the refresh): there the method has to be split into a pure plan
over a snapshot and an apply step.

Meanwhile Kani closes what it can reach: since `d1fa39f6`, proofs were added for entry points no
harness called (the gates of the trade, the rebalance and the Recovery routes, the resolved clock,
force recovery, the public crank finalizing Recovery, the terminal routes). Each goes to master
when it verifies; `kani_added.tsv` lists them.

Every engine change on the way costs a Kani run (about five hours) and re-proving the functions it
touches; the comparison tool keeps that to the changed functions.

## Cost and time

The pieces about pure functions took about 2,300 prover credits, roughly 14 a piece. The
{len(dep) - total_d} left are mostly whole entry points and instructions, and the two attempts at that kind
proved 3 of 32 properties for about 1,970 credits. At that price the rest is out of reach, so
there is no estimate until the way forward above is chosen: under the first one the work is the
refactor plus kernels at the known price; under the second it is models and differential tests.""")
