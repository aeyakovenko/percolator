# Specification: states committed after an error (layer X, engine c030aa40, program 3a690205)

A `*_not_atomic` function may have written to the market or the account when it returns `Err`; its
caller is expected to discard the state (the program aborts the transaction). Four places keep the
state instead (X1 to X4). Each is sound only under a property that is nowhere checked at run time.
This file states those properties so they can be proved, or refuted with a state.

## X1. The crank commits a liquidation step that declared Recovery (engine)

`kernel_commit_declared_liquidation_recovery` (line 2747) turns `Err(RecoveryRequired)` into
`Ok(RecoveryDeclared(reason))` when the market mode is Recovery and a reason is recorded. It is used
after `liquidate_account_not_atomic` (line 17349) and after
`advance_pending_close_residual_not_atomic` (line 18056). Both callers run only when the mode
was not Recovery at entry, so a Recovery mode at that point was set by this call, through
`declare_permissionless_recovery`, which ends in `self.validate_shape()`.

- X1.header: on every path of the two callees that returns `Err(RecoveryRequired)` with the mode set
  to Recovery by this call, nothing is written to the market after the `validate_shape()` inside
  `declare_permissionless_recovery`. Then the header checks hold of the committed state.
- X1.account: on those paths the account passes `validate_with_market` at the point of return (the
  callee's own closing validations are skipped by the early return). List the paths.

## X2. The crank goes on after `clear_leg` returned `Stale` (engine)

At line 17932 the auto crank calls `clear_leg` for a released obligation and, on `Err(Stale)`,
continues with the ordinary refresh. `clear_leg_at_slot_inner` writes to the account before the
checks that can return `Stale`: `liquidation_lock = true` and `health_cert.valid = 0`
(line 19249), when the account has an uncovered loss and another active leg.

- X2: those are the only writes before a `Stale` return, and a state with them is one the engine
  accepts: `liquidation_lock` with negative PnL passes `validate_with_market`, and
  `sync_unattributed_loss_lock` gives the same value at the next PnL write whenever the account has
  more than one active leg. State whether the lock can be set here in a case where that rule would
  leave it clear.

## X3. The program commits after the engine's crank returned `NonProgress` (program)

The crank handler (program line 14492) treats `Err(NonProgress)` from
`permissionless_auto_crank_not_atomic` as "the account had nothing to do" when it has itself accrued
the market in the same instruction, and commits. That is sound exactly when a `NonProgress` return
leaves the market and the account unchanged, or in a state a successful refresh leaves.

`NonProgress` returns reachable from the auto crank in Live and Recovery mode, by what precedes them:

| Site | Line | What the call has done before it returns `NonProgress` |
|---|---|---|
| N1a | 20184 | `liquidate_account_not_atomic`: a complete `refresh_account_and_certify_not_atomic`; the account then has no deficit |
| N1b | 20214 | same refresh; the close budget is zero |
| N1c | 18565 | the progress check at the end of a liquidation step (fee charged, loss settled, insurance used, residual booked, position reduced) and of a rebalance reduction |
| N2 | 19921 | `advance_pending_close_residual_not_atomic`: principal settlement, insurance consumption and a residual booking, with no fall of the ledger's remaining residual |
| N3 | 21763 | `release_account_source_credit_liens_if_unneeded_core_not_atomic`: settlement of the account's side effects, with no lien released |
| N4 | 17370 | the accrual that follows a completed liquidation inside `permissionless_crank_not_atomic` |

All other `NonProgress` returns under the auto crank in those modes come before any write (the two
in `accrue_asset_to_not_atomic`, the first ones of `advance_pending_close_residual_not_atomic`, of
the lien release and of `refresh_recovery_account_from_committed_state_not_atomic`, and the one in
the auto crank's refresh branch). The auto crank has one more, in its CloseResolved branch after
`advance_resolved_slot_not_atomic` has moved the clock; that branch runs only in Resolved mode, and
the program's handler for that mode propagates the error.

- X3.before: confirm the list of `NonProgress` returns that come before any write.
- X3.refresh (N1a, N1b): the state at these returns is the state of a completed refresh with a valid
  certificate.
- X3.unreachable (N1c, N2, N3, N4): for each, either no execution reaches the return, or the state
  at the return is one the engine accepts and that charges nobody (in particular: no liquidation fee
  has been taken at N1c). N4 needs: with `now_slot` equal to the asset's `slot_last` and the price
  equal to its effective price, `accrual_activity_for_asset_segment` reports no activity
  (`funding_active` requires `segment_dt > 0`). N1c is the liveness claim that an engine-sized
  liquidation step always improves the risk score; if it can fail, the program would commit a
  liquidation the engine rejected, with its fee, and without the rule that a penalty caused by a
  trade-driven mark is not credited to a domain budget (that rule is applied only when the engine
  reports a liquidation).

## X4. The program goes on after `deregister_empty_materialized_portfolio_not_atomic` returned `LockActive` (program)

In SyncMaintenanceFee (program line 12003) a `LockActive` from that entry point (engine line
9591) means "the portfolio is not empty yet" and the instruction continues.

- X4: the entry point returns `LockActive` only before its single write (the check
  `is_empty_for_dematerialization` precedes the counter update), so nothing was written.

## How the list was made

The engine and the program were searched for every place where the result of a call is handled
instead of propagated with `?`: match arms on `Err` that do not return an error, `if let Ok/Err`,
`.is_ok()`, `.is_err()`, `.ok()`, `let _ =` and `unwrap_or*`. In the engine that leaves the two
call sites of X1 and the one of X2 (`set_asset_state` handles an error by restoring the slot it
wrote; the other hits are read-only or pure). In the program it leaves X3 and X4. A new site of
this kind has to be added here by hand: the search is not part of this script.

No state has been found that violates X1 to X4. The engine suites (515 tests) and the program's
suites were run against an engine copy that stops at N1c, N2, N3 and N4; see the ledger for the
result.
