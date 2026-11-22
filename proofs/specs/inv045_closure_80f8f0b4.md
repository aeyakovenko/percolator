# INV-045 (no free mark movement): the statement in pieces (engine 80f8f0b4, program ebcded58)

The charter text (program `INVARIANTS.md`, reworded in a406ce3b): while the asset has effective open
interest on either side, every accepted mark update, including raw target 0, 1 and MAX, stays inside
the elapsed-time movement envelope from the prior nonzero mark; a price that a trade reports is
clamped to that envelope before it is accepted, whatever the open interest; no route bypasses the
cap. While the asset has no effective open interest, an accepted update may move the mark to its
accepted target in one step; such an update accrues no funding, and its mark-to-market reaches only
the residue of a side whose effective open interest was consumed before the side could reset (engine
issue #132), at most `raw_basis_sum_side / ADL_ONE` quantity units per side, moved as any capped path
to the same price would move it.

"Mark" below is the engine's effective price (`AssetStateV16::effective_price`), the price every
position is marked at. "Envelope": a move from `p` over a segment of `dt` slots is inside it when
`|move| * 10_000 <= max_price_move_bps_per_slot * dt * p`.

## Clauses and what proves each

| Clause | Statement | Pieces | Status |
| --- | --- | --- | --- |
| C1 writers | every write of the effective price in the deployed build is one of: the single-segment accrual, the path accrual, the initialization of a new asset or generation, an anchor reset of an asset with no position or loss state, or a copy of the old price | `specs/inv045_effective_price_writers_80f8f0b4.md` (E1-E6, P1-P3) | read from the source, and checked by a scan (`effective_price_writers.py` against `specs/inv045_effective_price_writers_80f8f0b4.tsv`: every deployed write site and every asset write-back; a scan, not a proof) |
| C2 single segment, exposed | in `accrue_asset_to_not_atomic`, a price change of an asset with effective open interest is equity active, and an equity-active segment returns `Ok` only inside the envelope | AC.exposed (Kani, every input); K76.contract (`kernel_accrual_price_move_within_cap`) | AC.exposed done; K76 bounded Kani, full width in Aristotle task k4 (KA.contract) |
| C3 path step, exposed | in `accrue_asset_path_to_not_atomic`, every step equals `canonical_accrual_price_step_v16`, which moves toward the target by at most the carried envelope when exposed | K93.contract, K93.envelope, K93.reaches (Aristotle task k2) | done |
| C4 trade print | a price a trade reports is clamped from the mark average by `clamp_toward_engine_dt` before it is accepted, on every trade route | `accepted_reported_trade_price_view` has no open-interest condition and is the only acceptance of a reported price (both trade routes call it; the program's source-complete composition test inventories the routes); KC.envelope, KC.reaches, KC.select (`clamp_toward_engine_dt` lies between anchor and target and inside the envelope; `effective_price_from_target` is the clamp when exposed and the target otherwise) | open: Aristotle task k4 Part 4 (model checking gave no verdict in 50 minutes even with 16-bit caps and segment lengths, so there is no Kani evidence); the route inventory is test evidence |
| C5 unexposed, no funding | with no effective open interest on either side a segment is neither price-move nor funding active and books a zero funding amount, and a zero funding amount leaves both funding indices unchanged | AC.unexposed, AC.nofunding (Kani, every input) | done |
| C6 unexposed, residue | a side the scan admits with raw basis and no effective open interest has A index 1; the sync kernel sets that value | AC.residue (Kani, every input); `kernel_side_adl_index` contract | done |
| C7 unexposed, linear K | the K indices move by `A_side * price_delta` in one update, so one jump and any capped path to the same price leave the same K | the K clauses of the index-update contract (`kernel_accrual_index_update`'s ensures pins `kl == k_long + price_delta * a_long` and `ks == k_short - price_delta * a_short`, K78.contract) and KI.path (the same as exact integers, and the two-step composition), task k4 | bounded Kani; full width in task k4 |
| C0 price range | neither accrual entry point accepts a price of 0 or above `MAX_ORACLE_PRICE` (this is what makes "raw target 0, 1 and MAX" safe: the program's clamp passes a zero target through, and the engine refuses it) | `accrue_asset_to_not_atomic` returns `InvalidConfig` for `effective_price == 0 \|\| effective_price > MAX_ORACLE_PRICE` before any write (src/v16.rs 18460-18466 at 80f8f0b4); `accrue_asset_path_to_not_atomic` does the same for `raw_oracle_target_price` (18601-18607), and `canonical_accrual_price_step_v16` refuses a zero or out-of-range current, target or anchor (8108-8115; K93.contract) | open, bounded evidence: AC.price_range, Kani (`proof_v16_public_accrual_refuses_a_price_outside_the_oracle_range_and_writes_nothing`, engine 3f56c204, batch 11) for the prices 0, `MAX_ORACLE_PRICE + 1` and `u64::MAX`, every slot, rate, flag and step slice, on the one-market fixture: InvalidConfig and the header and slot unchanged; every other price and every market state rest on reading the single comparison |
| C8 composition | C0-C7 give the charter statement for every route: every route that moves the effective price of an exposed asset goes through C2 or C3; every other writer is an initialization, a copy, or an unexposed reset | this file, read against the charter by someone other than its author | open |

## What is left

- Aristotle task k4: KA.contract (C2 at full width), KI.path and the K clauses of KI.contract (C7),
  KC.envelope / KC.reaches / KC.select (C4).
- C1 is checked by a scan (above) and the route inventory of C4 by the program's tests; neither is a
  proof.
- C8: an independent read of this statement against the charter, then the composition proof.
