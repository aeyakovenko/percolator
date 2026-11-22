# Writers of the effective price (INV-045, engine 71076137, program e1295564)

Restated from `specs/inv045_effective_price_writers_4e77481b.md` after the independent read of INV-045 (2026-11-22):
the reader's own enumeration found every writer of this list, and no writer of the mark of an asset with effective
open interest other than E1 and E2; its corrections are applied here. Batch 15 (engine `71076137`) changes no writer
of the mark: it makes the staging setter restart the price-cap anchor (`fund_px_last`) that E2 uses. Line numbers are
those of `71076137` and of the program's sources at `e1295564` (the same as `cd8a446a`).

INV-045 (program `INVARIANTS.md`, reworded in e1295564): while the asset has effective open interest on either side,
the mark moves toward its target by at most `anchor × cap` per elapsed slot, the anchor being the mark when the
current target took effect; with none it may move to its target in one step. This file lists every write of the
engine's effective price (`AssetStateV16::effective_price`) in the deployed build: the engine's `src/v16.rs` and the
program's `src/v16_program.rs`, excluding host-only code (`#[cfg(not(target_os = "solana"))]`) and proof/test code.

**Machine check.** `effective_price_writers.py <engine src/v16.rs> <program src/v16_program.rs>
specs/inv045_effective_price_writers_71076137.tsv` repeats the search on every function body (field assignments and
`.set(..)`, asset-record literals with the field, fresh records and empty slots, whole-slot assignments) and classifies
each site by its cfg. It also checks the write-back function `set_asset_state(i, rec)`: each of its 20 call sites must
pass a record read from the same slot in the same function, or be listed (three are, with the reading). It is a scan,
not a proof. What it does not establish, and the reader pointed out: that no writer of the mark runs between a
write-back's read and its store (true by the call graph today: the accruals are reached only from the program's direct
calls and from the permissionless crank, and none of the write-backs calls either; the scan should check this), raw
byte writes, and records copied between slots by other means. Raw byte writes that do touch the field, none of them on
an asset with effective open interest: the program zero-fills the whole account when it creates the market (P1), zero-
initializes new slots when an `UpdateAssetLifecycle` activation grows the capacity (`realloc` with zero-fill, program
13143), stores the result of E3 through its own byte view for an appended slot (program 2172), and CloseSlab truncates
the account to its header and zero-fills it (program 11996, 936) in Resolved mode with no portfolio left;
`set_asset_state` restores the bytes it read when its write fails (engine, the error path), a copy of the asset's own
price. Each is in one of the categories below.

| # | Writer | Line | What it writes | Guard | INV-045 case |
| --- | --- | --- | --- | --- | --- |
| E1 | `accrue_asset_to_admitted_not_atomic`, the body of `accrue_asset_to_not_atomic` after its prologue (engine) | 18551 | the caller's price for one segment | when the asset has effective open interest and the price changes, the segment is equity active (`proof_exposed_price_move_is_equity_active`) and `kernel_accrual_price_move_within_cap` refuses a move beyond `cap * dt / 10000` of the mark before it | exposed: inside the envelope measured from the mark before the segment (K76, full width, Aristotle task k4); the segment's anchor is that mark |
| E2 | `accrue_asset_path_to_admitted_not_atomic`, the body of `accrue_asset_path_to_not_atomic` after its prologue (engine) | 18750 | each step's price | every step must equal `canonical_accrual_price_step_v16(mark, target, anchor, cap, exposed, remainder)`, the anchor being `fund_px_last` (the mark when the stored target last changed; the path itself sets it when it is given a new target, and the staging setter when it changes the target) or the mark when the call sees a new target | exposed: each step at most `(anchor * cap + remainder) / 10_000` and never past the target, in total `n * anchor * cap + r0` over `n` steps toward one target (K93, Aristotle k2), `r0 < 10_000` being the remainder the caller passes into the first step; not exposed: the target in one step |
| E3 | `activate_empty_market_slot_not_atomic` (engine) | 10594 | the `initial_price` of the activation instruction, chosen by the market authority or by anyone who pays the permissionless activation fee; the engine checks only `1..=MAX_ORACLE_PRICE` | Live mode, the activation cooldown, the asset state empty for activation (counts, open interest, raw basis, K/F/B and loss state zero), lifecycle Disabled with market id 0 or Retired past the cooldown, empty source, backing and insurance state | initialization: no position exists on the slot (a reused Retired slot had a mark, but no position survives) |
| E4 | `restart_empty_asset_preserving_insurance_budget_not_atomic` via `restarted_asset_slot_preserving_insurance_budget` (engine) | 19889 | the asset admin's `initial_price` (RestartAssetOracle) | lifecycle Retired or Recovery and `require_empty_asset_lifecycle_state_with_policy` | initialization of a new generation: no position of the old one survives |
| E5 | `reset_empty_asset_oracle_anchor_not_atomic` (engine) | 18409 | an authenticated price | lifecycle Active and `group_has_position_or_loss_state_for_oracle_reset` false: no effective open interest, stored position, stale count, B or loss state, side reset or loss barrier on any asset of the group, and none of the header's loss flags | not exposed. No program instruction calls it today |
| E6 | `canonical_retired_asset_slot` (engine) | 19905 | the old asset's own price | | no change. Its caller `canonicalize_retired_empty_asset_slot_not_atomic` is called by no program instruction today (the program uses P3) |
| P1 | `init_market_account_zero_copy` (program) | 2800 | the initial price of every configured asset (1 to 14 of them) | market creation | initialization |
| P2 | `reset_empty_asset_oracle_anchor_view` (program) | 7487 | an authenticated price | `require_asset_active_for_oracle_reconfiguration_view`: lifecycle Active, no h-lock, threshold stress or recovery reason, and `asset_local_has_position_or_loss_state_view` false (no effective open interest, stored legs, stale accounts, B or loss state, side reset or loss barrier on the asset); asset 0 also needs the whole market quiet. It does not test raw basis; the per-asset scan gives no raw basis without stored legs | not exposed. It moves the mark and not K, which is harmless with no raw basis |
| P3 | `canonicalize_retired_asset_slot_view` (program) | 12960 | the old asset's own price | | no change |

Every program route that changes the effective price of an asset with effective open interest does it through E1 or E2
(the cranks call the engine's accrual); the program's own clamp (`oracle_v16::effective_price_from_target`,
`clamp_toward_engine_dt`) only chooses the price or target it asks for, and the engine refuses one outside its
envelope. A price that a trade reports is clamped by `accepted_reported_trade_price_view` from the program's price
average (`mark_ewma_e6`) over the slots since that average last moved, with no condition on open interest; both trade
routes (single and batch, with or without CPI) call it, and the clamped price can become a staged target, which (since
batch 15) restarts the anchor of E2 at the current mark.

Supporting Kani lemmas (engine `src/v16_proofs.rs`; the first four since c65bb9a1, the last three since 4e77481b):

- `proof_unexposed_accrual_segment_moves_no_equity`: with no effective open interest on either side
  a segment is neither price-move nor funding active, and the funding amount booked is zero.
- `proof_exposed_price_move_is_equity_active`: with effective open interest, a segment that moves
  the price is equity active, so E1 runs the cap kernel.
- `proof_index_update_without_funding_leaves_funding_indices`: a zero funding amount leaves both
  funding indices unchanged, full width.
- `proof_scan_side_with_basis_and_no_open_interest_has_unit_a_index`: a side the scan admits with raw
  basis and no effective open interest has A index 1 (the residue of engine issue #132 carries at
  most `raw_basis_sum / ADL_ONE` quantity units). It is a statement about states that pass the scan;
  that every reachable state does is the global invariant's part S. `kernel_side_adl_index`'s contract
  (K59, full width) gives A index 1 when it sets the index of a side with no open interest.
- `proof_accrual_prologue_refuses_every_price_outside_the_oracle_range` and
  `proof_accrual_prologue_refuses_every_price_outside_the_oracle_range_for_every_header`: E1's and E2's
  entry points refuse every price of 0 or above `MAX_ORACLE_PRICE` before the body runs (a stand-in for the
  body fails the proof if called), on a Live market and for every header, leaving the market unchanged.
- `proof_accrual_prologue_admits_every_price_inside_the_oracle_range`: every price from 1 to
  `MAX_ORACLE_PRICE` reaches the body.
- `proof_v16_changed_raw_target_restarts_the_cap_anchor` (since 71076137): a target staged by
  `set_asset_raw_oracle_targets_not_atomic` that changes sets the anchor to the mark; an unchanged one leaves the slot
  as it was; a target of 0 or above `MAX_ORACLE_PRICE` is refused with no write.
