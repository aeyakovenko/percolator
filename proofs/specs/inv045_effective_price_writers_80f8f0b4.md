# Writers of the effective price (INV-045, engine 80f8f0b4, program ebcded58)

INV-045 (program `INVARIANTS.md`, reworded in program a406ce3b) bounds every accepted mark update
by the elapsed-time envelope while the asset has effective open interest on either side, and lets
an asset with none move to its accepted target in one step. This file lists every write of the
engine's effective price (`AssetStateV16::effective_price`, the price positions are marked at) in
the deployed build: the engine's `src/v16.rs` and the program's `src/v16_program.rs`, excluding
host-only code (`#[cfg(not(target_os = "solana"))]`) and proof/test code. Found by searching both
files for `effective_price =`, `AssetStateV16::default()`, `AssetStateV16 {` and whole-slot
assignments, then reading each site.

**Machine check.** `effective_price_writers.py <engine src/v16.rs> <program src/v16_program.rs>
specs/inv045_effective_price_writers_80f8f0b4.tsv` repeats the search on every function body (field
assignments and `.set(..)`, asset-record literals with the field, fresh records and empty slots,
whole-slot assignments; locals and literals of other structs are listed, not checked) and classifies
each site by its cfg. It also checks the one write-back function, `set_asset_state(i, rec)`: each of
its 21 callers must pass a record read from the same slot in the same function, or be listed (three
are, with the reading). It exits non-zero if a deployed site or write-back is not in the table, or a
table row has no site. At 80f8f0b4 / ebcded58 it passes (23 deployed sites in 16 functions: the nine
writers below, the fresh record and empty slot they start from, the two conversions between the
account and runtime forms, and the three listed write-backs). Injected writers are caught: a new
method assigning the field, a new asset-record literal with the field, and a write-back to another
slot. It is a scan, not a proof: it does not follow raw byte writes (the slab is zeroed only when the
account is created) or a record copied between slots by means other than `set_asset_state`, and a
reading found none.

| # | Writer | Line | What it writes | Guard | INV-045 case |
| --- | --- | --- | --- | --- | --- |
| E1 | `accrue_asset_to_not_atomic` (engine) | 18527 | the caller's price for one segment | when the asset has effective open interest and the price changes, the segment is equity active (`proof_exposed_price_move_is_equity_active`) and `kernel_accrual_price_move_within_cap` refuses a move beyond `cap * dt / 10000` of the old price | exposed: inside the envelope (K76, the contract of `kernel_accrual_price_move_within_cap`: bounded Kani; full width pending, Aristotle k4 KA.contract) |
| E2 | `accrue_asset_path_to_not_atomic` (engine) | 18703 | each step's price | every step must equal `canonical_accrual_price_step_v16(effective, target, anchor, cap, exposed, remainder)` | exposed: inside the envelope (K93, Aristotle k2 KP.contract / KP.envelope, full width); not exposed: the target in one step |
| E3 | `activate_empty_market_slot_not_atomic` (engine) | 10604 | the authenticated price of a new asset | the slot is empty | initialization: no prior mark |
| E4 | `restart_empty_asset_preserving_insurance_budget_not_atomic` via `restarted_asset_slot_preserving_insurance_budget` (engine) | 19849 | the authenticated price of a new asset generation | lifecycle Retired or Recovery and `require_empty_asset_lifecycle_state_with_policy` | initialization of a new generation: no position of the old one survives |
| E5 | `reset_empty_asset_oracle_anchor_not_atomic` (engine) | 18410 | an authenticated price | lifecycle Active and `group_has_position_or_loss_state_for_oracle_reset` false (no position or loss state anywhere in the group) | not exposed |
| E6 | `canonical_retired_asset_slot` (engine) | 19865 | the old asset's own price | | no change |
| P1 | `init_market_account_zero_copy` (program) | 2800 | the initial price of asset 0 | market creation | initialization |
| P2 | `reset_empty_asset_oracle_anchor_view` (program) | 7491 | an authenticated price | `require_asset_active_for_oracle_reconfiguration_view`: lifecycle Active and `asset_local_has_position_or_loss_state_view` false (no effective open interest, stored legs, stale accounts, B or loss state, side reset or loss barrier on the asset); asset 0 also needs the whole market quiet | not exposed |
| P3 | `canonicalize_retired_asset_slot_view` (program) | 12968 | the old asset's own price | | no change |

Every program route that changes the effective price of an asset with effective open interest
does it through E1 or E2 (the cranks and the trade paths call the engine's accrual); the program's
own clamp (`oracle_v16::effective_price_from_target`, `clamp_toward_engine_dt`) only chooses the
price it asks for, and the engine refuses one outside its envelope. A price that a trade reports is
clamped from the mark average by `accepted_reported_trade_price_view` before it can become a target,
with no condition on open interest; both trade routes (single and batch, with or without CPI) call it.

Supporting Kani lemmas (engine `src/v16_proofs.rs` at c65bb9a1, proofs only on top of 80f8f0b4):

- `proof_unexposed_accrual_segment_moves_no_equity`: with no effective open interest on either side
  a segment is neither price-move nor funding active, and the funding amount booked is zero.
- `proof_exposed_price_move_is_equity_active`: with effective open interest, a segment that moves
  the price is equity active, so E1 runs the cap kernel.
- `proof_index_update_without_funding_leaves_funding_indices`: a zero funding amount leaves both
  funding indices unchanged, full width.
- `proof_scan_side_with_basis_and_no_open_interest_has_unit_a_index`: a side the scan admits with raw
  basis and no effective open interest has A index 1 (the residue of engine issue #132 carries at
  most `raw_basis_sum / ADL_ONE` quantity units); `kernel_side_adl_index`'s contract states the same
  for every transition that sets the index.
