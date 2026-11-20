#!/usr/bin/env python3
"""gen_error_commit_spec.py <engine src/v16.rs> <wrapper src/v16_program.rs> <engine rev> <wrapper rev>
   > specs/error_commit_paths_<engine rev>.md
Layer X of the ledger: the places where state is COMMITTED although a `*_not_atomic` call returned an
error. Every site is located by a text anchor; the script stops if an anchor is missing or no longer
unique, so the list is read again when the code around it changes."""
import sys
e, w, erev, wrev = open(sys.argv[1]).read(), open(sys.argv[2]).read(), sys.argv[3], sys.argv[4]
def line(src, anchor, what):
    if src.count(anchor) != 1:
        sys.exit(f"anchor for {what} occurs {src.count(anchor)} times; read the site again")
    return src.count("\n", 0, src.index(anchor)) + 1
S = {
 "E1a": line(e, "if let Err(error) = self.liquidate_account_not_atomic(account, liq) {", "E1a"),
 "E1b": line(e, "let progressed = match self.advance_pending_close_residual_not_atomic(account) {", "E1b"),
 "E1k": line(e, "fn kernel_commit_declared_liquidation_recovery(", "E1 kernel"),
 "E2":  line(e, "match self.clear_leg(account, asset_index) {", "E2"),
 "E2w": line(e, "account.header.liquidation_lock = encode_bool(true);\n            account.header.health_cert.valid = 0;\n        }\n        let close_progress", "E2 write"),
 "E3":  line(w, "Err(V16Error::NonProgress) if market_accrual_performed => {", "E3"),
 "E3g": line(w, "if group.header.insurance.get() != insurance_before\n                        || positions_before != portfolio_position_vector_view(&portfolio)", "E3 guard"),
 "E4":  line(w, "match group.deregister_empty_materialized_portfolio_not_atomic(&portfolio.as_view())\n                {\n                    Ok(()) => true,\n                    Err(V16Error::LockActive) => false,", "E4"),
 "E4e": line(e, "pub fn deregister_empty_materialized_portfolio_not_atomic(", "E4 engine"),
 "N1a": line(e, "if cert.certified_liq_deficit == 0 {\n            return Err(V16Error::NonProgress);", "N1a"),
 "N1b": line(e, "if close_budget == 0 {\n            return Err(V16Error::NonProgress);", "N1b"),
 "N1c": line(e, "fn validate_liquidation_progress_from_score(", "N1c"),
 "N2":  line(e, "if ledger_after.residual_remaining >= ledger_before.residual_remaining {\n            return Err(V16Error::NonProgress);", "N2"),
 "N3":  line(e, "if released_domains == 0 {\n            return Err(V16Error::NonProgress);", "N3"),
 "N4":  line(e, "            protective_progress,\n        )?;\n        Ok(PermissionlessProgressOutcomeV16::AccountCurrent)", "N4"),
}
print(f"""# Specification: states committed after an error (layer X, engine {erev}, program {wrev})

A `*_not_atomic` function may have written to the market or the account when it returns `Err`; its
caller is expected to discard the state (the program aborts the transaction). Four places keep the
state instead (X1 to X4). Each is sound only under a property that is nowhere checked at run time.
This file states those properties so they can be proved, or refuted with a state.

## X1. The crank commits a liquidation step that declared Recovery (engine)

`kernel_commit_declared_liquidation_recovery` (line {S['E1k']}) turns `Err(RecoveryRequired)` into
`Ok(RecoveryDeclared(reason))` when the market mode is Recovery and a reason is recorded. It is used
after `liquidate_account_not_atomic` (line {S['E1a']}) and after
`advance_pending_close_residual_not_atomic` (line {S['E1b']}). Both callers run only when the mode
was not Recovery at entry, so a Recovery mode at that point was set by this call, through
`declare_permissionless_recovery`, which ends in `self.validate_shape()`.

- X1.header: on every path of the two callees that returns `Err(RecoveryRequired)` with the mode set
  to Recovery by this call, nothing is written to the market after the `validate_shape()` inside
  `declare_permissionless_recovery`. Then the header checks hold of the committed state.
- X1.account: on those paths the account passes `validate_with_market` at the point of return (the
  callee's own closing validations are skipped by the early return). List the paths.

## X2. The crank goes on after `clear_leg` returned `Stale` (engine)

At line {S['E2']} the auto crank calls `clear_leg` for a released obligation and, on `Err(Stale)`,
continues with the ordinary refresh. `clear_leg_at_slot_inner` writes to the account before the
checks that can return `Stale`: `liquidation_lock = true` and `health_cert.valid = 0`
(line {S['E2w']}), when the account has an uncovered loss and another active leg.

- X2: those are the only writes before a `Stale` return, and a state with them is one the engine
  accepts: `liquidation_lock` with negative PnL passes `validate_with_market`, and
  `sync_unattributed_loss_lock` gives the same value at the next PnL write whenever the account has
  more than one active leg. State whether the lock can be set here in a case where that rule would
  leave it clear.

## X3. The program commits after the engine's crank returned `NonProgress` (program)

The crank handler (program line {S['E3']}) treats `Err(NonProgress)` from
`permissionless_auto_crank_not_atomic` as "the account had nothing to do" when it has itself accrued
the market in the same instruction. Since program #485 it commits in that case only under a guard
(line {S['E3g']}): the market's `insurance` and the account's position vector (for every leg slot
the active flag, asset, market id, side and raw basis) must be what they were before the engine call; otherwise the instruction fails
with `EngineNonProgress` and the transaction aborts. Every `Ok` exit of the handler is also preceded
by `group.validate_shape()`, so the header checks hold of the committed state by a runtime check.

That is sound exactly when a `NonProgress` return that leaves `insurance` and the positions
unchanged leaves the rest of the market and the account unchanged, or in a state a successful
refresh leaves.

`NonProgress` returns reachable from the auto crank in Live and Recovery mode, by what precedes them:

| Site | Line | What the call has done before it returns `NonProgress` |
|---|---|---|
| N1a | {S['N1a']} | `liquidate_account_not_atomic`: a complete `refresh_account_and_certify_not_atomic`; the account then has no deficit |
| N1b | {S['N1b']} | same refresh; the close budget is zero |
| N1c | {S['N1c']} | the progress check at the end of a liquidation step (fee charged, loss settled, insurance used, residual booked, position reduced) and of a rebalance reduction |
| N2 | {S['N2']} | `advance_pending_close_residual_not_atomic`: principal settlement, insurance consumption and a residual booking, with no fall of the ledger's remaining residual |
| N3 | {S['N3']} | `release_account_source_credit_liens_if_unneeded_core_not_atomic`: settlement of the account's side effects, with no lien released |
| N4 | {S['N4']} | the accrual that follows a completed liquidation inside `permissionless_crank_not_atomic` |

All other `NonProgress` returns under the auto crank in those modes come before any write (the two
in `accrue_asset_to_not_atomic`, the first ones of `advance_pending_close_residual_not_atomic`, of
the lien release and of `refresh_recovery_account_from_committed_state_not_atomic`, and the one in
the auto crank's refresh branch). The auto crank has one more, in its CloseResolved branch after
`advance_resolved_slot_not_atomic` has moved the clock; that branch runs only in Resolved mode, and
the program's handler for that mode propagates the error.

- X3.before: confirm the list of `NonProgress` returns that come before any write.
- X3.refresh (N1a, N1b): the state at these returns is the state of a completed refresh with a valid
  certificate.
- X3.guarded (N1c, N2, N3, N4): for each of these returns, in every execution in which
  `header.insurance` and the account's position vector equal their values before the auto crank
  call: the state at the return passes the per-asset scan and `validate_with_market` (the header is
  checked at run time), and it differs from the state before the call only by writes that a
  successful step of the same kind makes and that debit no account other than the cranked one (no
  bankruptcy residual booked onto the other side, no B index moved). If no such execution reaches a
  return, say so: the piece then holds for that return because the guard refuses every state that
  does reach it. The guard already excludes a liquidation fee, which is a rise of `insurance`, unless
  insurance was consumed by the same amount in the same call; state whether that can happen. N4
  needs: with `now_slot` equal to the asset's `slot_last` and the price equal to its effective price,
  `accrual_activity_for_asset_segment` reports no activity (`funding_active` requires
  `segment_dt > 0`). N1c is the progress check at the end of a liquidation step: with the positions
  unchanged the step reduced nothing, and what it can still have written (a loss settled from the
  account's capital, insurance consumed, a residual booked) is what this piece has to bound.

## X4. The program goes on after `deregister_empty_materialized_portfolio_not_atomic` returned `LockActive` (program)

In SyncMaintenanceFee (program line {S['E4']}) a `LockActive` from that entry point (engine line
{S['E4e']}) means "the portfolio is not empty yet" and the instruction continues.

- X4: the entry point returns `LockActive` only before its single write (the check
  `is_empty_for_dematerialization` precedes the counter update), so nothing was written.

## How the list was made

The engine and the program were searched for every place where the result of a call is handled
instead of propagated with `?`: match arms on `Err` that do not return an error, `if let Ok/Err`,
`.is_ok()`, `.is_err()`, `.ok()`, `let _ =` and `unwrap_or*`. In the engine that leaves the two
call sites of X1 and the one of X2 (`set_asset_state` handles an error by restoring the slot it
wrote; the other hits are read-only or pure). In the program it leaves X3 and X4. A new site of
this kind has to be added here by hand: the search is not part of this script.

No state has been found that violates X1 to X4. On engine c030aa40 and program 3a690205 (before the
guard) the engine suites (515 tests) and the program's suites were run against an engine copy that
stops at N1c, N2, N3 and N4; see the ledger for the result. That run has not been repeated on the
revisions of this file.""")
