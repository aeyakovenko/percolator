#!/usr/bin/env python3
"""gen_transition_spec.py <src/v16.rs> [<rev>] > specs/transition_shape_spec_<rev>.md
Layer T of the ledger for one engine revision: every public `*_not_atomic` entry point with the class
its Ok paths were found to have when read (transition_kinds.REVIEWED), labels T01.. in name order
(as make_ledger.py). Fails if a deployed entry point changed since it was read."""
import sys, os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from transition_kinds import classify
ROWS = classify(open(sys.argv[1]).read())
rev = sys.argv[2] if len(sys.argv) > 2 else "this revision"
dep = [r for r in ROWS if r["deployed"] == "yes"]
by = {}
for r in dep:
    by.setdefault(r["cls"], []).append(r["label"])
def L(cls):
    return ", ".join(by.get(cls, [])) or "none"
print(f"""# Specification: market shape is preserved by every public engine entry point (layer T, engine {rev})

`src/v16.rs` has {len(ROWS)} public entry points named `*_not_atomic` on `MarketGroupV16ViewMut` (a few on
the header account); {len(dep)} of them are compiled into a default build. The market shape predicate of
a default build is the function

    MarketGroupV16ViewMut::validate_shape(&self) -> V16Result<()>

It reads the market only (`&self`) and returns `Ok(())` exactly when eighteen checks on the market
header pass. They are listed as H0 to H17 in `aristotle/header_write_sets.md`: the configuration
decodes, the flags are booleans, the vault covers capital, insurance, provider earnings and
recoverable backing, the positive-PnL totals and their bound agree, slots and activation counters
are ordered. Write H(m) for "`validate_shape()` returns `Ok(())` on m".

`validate_shape` also calls `validate_shape_full_audit_scan`, the scan over every asset slot, but
only when the crate is built with the `audit-scan` feature (or under test or Kani). That scan is a
second, larger predicate. It is not part of a default build and not part of this layer's header
half: it is listed in the ledger as `Txx.scan`.

## Why H has to be proved and is not simply checked

Most entry points end in `self.validate_shape()`, and the program repeats the check after many
instructions. But not after all of them, and some entry points check only in audit builds
(`validate_shape_audit_scan` is an empty function in a default build). The program instructions
that rely on the engine alone are:

- PermissionlessCrank in Live and Recovery mode, whenever the account's position vector is unchanged
  (a refresh, a B-chunk settlement, a step of a pending close, a Recovery refresh): the handler
  validates the shape only `if position_changed`;
- RebalanceReduce: its handler passes `wrapper_post_validate = false`, with the comment that the
  engine transition owns the post-state contract and repeating the audit would exceed the compute
  budget;
- FinalizeResetSide, InitPortfolio, ClosePortfolio and CloseSlab, whose engine calls are of class
  frame or local below.

If one of those paths could return `Ok` and leave a header that fails a check, the next instruction
that does validate would fail, and so would every one after it. So for these paths the property
below is the only thing between a defect and a market that can no longer be used.

## What to prove

`Txx.header`: for all inputs and all starting states m with H(m), if the call returns `Ok(_)`, then
H holds of the market state the call leaves behind. How much work that is depends on the class of
the entry point. The class was established by reading every Ok path of every deployed entry point;
the column "Found when read" says what was seen. Confirm the class from the source before using it,
and report any entry point for which it is wrong.

- **final** ({len(by.get("final", []))}): every Ok path ends in `self.validate_shape()` after its last write to the
  market; an early `return Ok(..)` writes nothing. The property needs no assumption about the
  starting state. To prove: each Ok path either passes that final call with no market write after
  it, or writes nothing at all (then H at exit is H at entry).
- **delegates** ({len(by.get("delegates", []))}): the body is a call to an entry point of class final, plus read-only
  checks. To prove: the property of the callee, and that nothing is written after it returns.
- **local** ({len(by.get("local", []))}): no shape check. The checks the function can affect follow from guards inside
  the function itself, for every starting state with H. A real proof, but a small one.
- **frame** ({len(by.get("frame", []))}): no shape check. The function writes no header field that a check reads, so H
  at exit is H at entry. To prove: the set of header fields each Ok path writes.
- **mixed** ({len(by.get("mixed", []))}): final on most Ok paths; the listed paths end without a check and have the
  class given in the note.
- **real** ({len(by.get("real", []))}): no shape check in a default build. These move value between capital, insurance,
  provider earnings, backing and positive PnL, and the checks H6, H7, H10, H11 and H14 to H17 relate
  those totals. The proof needs the contracts of the kernels they call and, for H17 and for the
  totals that mirror per-domain state, invariants that relate the header to the market slots and to
  the accounts; H alone is not inductive for them. They are split by writer in
  `aristotle/header_write_sets.md` and are not part of the first tasks.
- **dispatch** ({len(by.get("dispatch", []))}): chooses a plan and calls other functions. Its property is the conjunction
  of the properties of the functions each branch calls; four of those are internal functions of
  class real (`refresh_account_and_certify_not_atomic`, `settle_account_b_chunk`,
  `advance_pending_close_residual_not_atomic`, `refresh_recovery_account_from_committed_state_not_atomic`).

`Txx.nopanic` is not asked for in this layer.

The column "In a default build" says whether the entry point is compiled into the deployed program.
Entry points declared under `cfg(any(kani, feature = "fuzz"))` (or with `test` added) are an API
for tests and proofs. Their properties are not applicable to the deployed program and are not asked
for; they are listed so the table matches the source.

## The entry points

| Label | Entry point | Line of `fn` in `src/v16.rs` | Class | Found when read | In a default build |
|---|---|---|---|---|---|""")
for r in ROWS:
    print(f"| {r['label']} | `{r['name']}` | {r['line']} | {r['cls']} | {r['note']} | {r['deployed']} |")
print("""
## The real class, split into local statements

The checks an entry point of class real can affect fall into six groups. Each group has a statement
that mentions only the header and the one account the call works on, so it can be proved one
function at a time; none needs a sum over all accounts. Write `stack` for
`c_tot + insurance + backing_provider_earnings_total + source_fresh_backing_total_num / BOUND_SCALE`
(integer division), the left side of H15.

- **R-a (H2, H3):** every write of a flag is 0, 1 or `encode_bool(..)`, and `recovery_reason` is
  written only by `from_runtime(Some(reason))`.
- **R-b (H5, H6, H14, H15):** `stack` after the call is at most `stack` before it plus the change of
  `vault`. None of the real entry points writes `vault`, so for them: `stack` does not increase.
  This is the value-conservation statement; it follows from the kernels that move value (each
  lowers one component by at least what it adds to another) and has to account for the rounding of
  backing held in `BOUND_SCALE` units. H5 and H6 follow from H15 and an unchanged `vault`.
- **R-c (H16):** `insurance - insurance_domain_budget_remaining_total` does not decrease, and
  `source_insurance_credit_reserved_total_atoms <= insurance_domain_budget_remaining_total` is
  preserved. (The second needs the per-domain relation between a domain's reserved credit and its
  remaining budget, which the audit scan checks: state it as a hypothesis on the touched domain.)
- **R-d (H7, H10, H11):** `pnl_pos_tot`, `pnl_pos_bound_tot_num` and `pnl_pos_bound_tot` are written
  only by `set_account_pnl_inner`, which keeps `pnl_pos_bound_tot == amount_from_bound_num(num)` and
  `num >= bound_num_from_amount(pnl_pos_tot)` by construction (a sum on an increase, a `max` on a
  decrease) and clamps `pnl_matured_pos_tot` to `pnl_pos_tot`. `amount_from_bound_num` rounds up, so
  `pnl_pos_tot <= pnl_pos_bound_tot` follows.
- **R-e (H17):** H17 is not inductive on its own. Let `U(account)` be
  `bound_num_from_amount(max(pnl, 0))` minus the sum of the account's `source_claim_bound_num` over
  its source domains. The local statement is: `U(account) >= 0` is preserved, and
  `(pnl_pos_bound_tot_num - source_claim_bound_total_num) - U(account)` does not decrease. With that
  quantity non-negative at the start (it is the slack owed to all other accounts), H17 follows for
  every reachable state. The step that needs care is a fall of positive PnL in
  `set_account_pnl_inner` with a nonzero `preburned_source_claim_num`: the caller must have taken
  exactly that amount out of the account's claims and out of the domain totals before the call.
- **R-f (H4, H12):** the payout ledger is written only through `from_runtime` of a ledger value, and
  only after `payout_snapshot_captured` was set, except in `initialize_resolved_payout_ledger_if_needed`,
  which sets the flag and the ledger together.

For each of T32, T37, T42, T45, and for the four internal functions the dispatcher reaches, the
header property is the conjunction of R-a to R-f over the writers that function can reach
(`aristotle/header_write_sets.md` lists them). R-b and R-e carry the weight.""")
first = [r["label"] for r in ROWS if r["first_task"]]
print(f"""
## Tasks

- Structural ({len(first)} entry points, classes final and delegates): {", ".join(first)}.
- Local ({len(by.get("local", []))}): {L("local")}; and through them the two unchecked paths of T10.
- Frame ({len(by.get("frame", []))}): {L("frame")}; and the unchecked path of T36.
- Real ({len(by.get("real", []))}): {L("real")}; the unchecked paths of T15; and the four internal functions named
  under dispatch.
- Dispatch: {L("dispatch")}, once the functions it calls are done.""")
