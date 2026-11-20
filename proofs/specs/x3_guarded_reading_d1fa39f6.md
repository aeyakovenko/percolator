# X3: what the crank can commit after `NonProgress` (engine d1fa39f6, program 78ad2c53)

My reading of 2026-11-20, by site. It is a reading of the source, not a proof; each site names the
lemma a proof needs. Sites and line numbers are those of `specs/error_commit_paths_d1fa39f6.md`.

The guard (program, crank handler): after `Err(NonProgress)` from
`permissionless_auto_crank_not_atomic` the instruction commits only if it accrued the market itself
and both `header.insurance` and the account's position vector (per leg slot: active flag, asset,
market id, side, raw basis) are what they were before the call. The header is validated before
every commit.

| Site | Result | Why |
|---|---|---|
| N1c (progress check at the end of a liquidation step) | The guard always refuses. | Both paths of `liquidate_account_not_atomic` call `reduce_position(account, asset, close_q)` before the check, with `close_q >= 1` (`close_budget >= 1` after N1b, and `kernel_reduce_position_delta` returns `min(budget, abs(raw basis)) >= 1` for an active leg). Every Ok route of the position change rewrites the leg: a resize stores a different raw basis, a clear deactivates the leg, a clear behind a loss barrier keeps the leg active with raw basis 0. So the position vector differs. |
| N4 (accrual after a completed liquidation) | The guard always refuses. | It follows an Ok return of the same liquidation, so the same reduction happened. |
| N2 (pending-close step, no fall of the ledger's remaining residual) | Committed only with the account's own writes; probably never reached after a write. | The remaining residual did not fall, so no insurance was spent, the clamp changed nothing, and nothing was booked (`advance_close_progress_ledger` lowers the remaining residual by what was spent or booked). In Live mode a residual that cannot be booked declares Recovery and returns `RecoveryRequired`, not `NonProgress`. What can be left is the settlement of the account's negative PnL from its own capital. |
| N3 (lien release for a flat account, nothing released) | Committed with a completed settlement and certificate only. | Every loop branch that writes counts a released domain, and the return needs zero. Before the loop the call settled the flat account's side effects and certified it. |

Lemmas a proof needs:

- R1: an Ok return of `reduce_position` with `close_q >= 1` leaves the leg's (active flag, raw basis)
  different from before the call. (Resize: K60.progress, proved, gives a strictly smaller raw basis
  after an ADL reduction; without ADL the raw basis falls by `close_q`.)
- R2: `kernel_advance_close_ledger` lowers `residual_remaining` by exactly what it is given, and
  `kernel_bresidual_step` in Live mode returns an outcome only with `booked + explicit > 0`.
- R3: `settle_account_side_effects_not_atomic` on an account with no active leg debits no other
  account.

Nothing here changes the program: the guard is what makes N1c and N4 safe, and it was added in
program #485.
