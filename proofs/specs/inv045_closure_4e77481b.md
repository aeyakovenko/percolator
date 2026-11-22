# INV-045 (no free mark movement): the statement in pieces (engine 4e77481b, program c8ddbb6e)

Rolled from `specs/inv045_closure_80f8f0b4.md`. Since then, Aristotle task k4 proved K76, K78 (its corrected
contract, which engine `08e8f7f0` carries) and the program clamp at full width, and engine batch 13
(`4e77481b`) let Kani prove the price range for every price. The statement and the clauses are unchanged.

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
| C0 price range | neither accrual entry point accepts a price of 0 or above `MAX_ORACLE_PRICE` (this is what makes "raw target 0, 1 and MAX" safe: the program's clamp passes a zero target through, and the engine refuses it) | AC.price_range: `proof_accrual_prologue_refuses_every_price_outside_the_oracle_range` (a Live market; every asset index, slot, funding rate, flag and step slice of up to one step), `..._for_every_header` (the header is arbitrary bytes) and `proof_accrual_prologue_admits_every_price_inside_the_oracle_range`; both entry points; the accrual body is replaced by a stand-in that fails the proof if called, and the prologue assigns nothing (checked on the text by `roll_to_4e77481b.py`) | verified by Kani for every price (engine 4e77481b) |
| C1 writers | every write of the effective price in the deployed build is one of: the single-segment accrual, the path accrual, the initialization of a new asset or generation, an anchor reset of an asset with no position or loss state, or a copy of the old price | `specs/inv045_effective_price_writers_4e77481b.md` (E1-E6, P1-P3) | read from the source, and checked by a scan (`effective_price_writers.py` against `specs/inv045_effective_price_writers_4e77481b.tsv`: every deployed write site and every asset write-back; it passes at 4e77481b / c8ddbb6e). A scan, not a proof |
| C2 single segment, exposed | in the single-segment accrual, a price change of an asset with effective open interest is equity active, and an equity-active segment returns `Ok` only inside the envelope | AC.exposed (Kani, every input); K76.contract (`kernel_accrual_price_move_within_cap`, Aristotle task k4, full width) | done |
| C3 path step, exposed | in the path accrual, every step equals `canonical_accrual_price_step_v16`, which moves toward the target by at most the carried envelope when exposed | K93.contract, K93.envelope, K93.reaches (Aristotle task k2) | done |
| C4 trade print | a price a trade reports is clamped from the mark average by `clamp_toward_engine_dt` before it is accepted, on every trade route | AC.clamp (Aristotle task k4, every input: `clamp_toward_engine_dt` lies between anchor and target and inside the envelope; `effective_price_from_target` is the clamp when exposed and the target otherwise); `accepted_reported_trade_price_view` has no open-interest condition and is the only acceptance of a reported price (both trade routes call it; the program's source-complete composition test inventories the routes) | the arithmetic is done; the route inventory is test evidence |
| C5 unexposed, no funding | with no effective open interest on either side a segment is neither price-move nor funding active and books a zero funding amount, and a zero funding amount leaves both funding indices unchanged | AC.unexposed, AC.nofunding (Kani, every input) | done |
| C6 unexposed, residue | a side the scan admits with raw basis and no effective open interest has A index 1; the sync kernel sets that value | AC.residue (Kani, every input); `kernel_side_adl_index` contract | done |
| C7 unexposed, linear K | the K indices move by `A_side * price_delta` in one update, so one jump and any capped path to the same price leave the same K | K78.contract (the corrected contract) and K78.path (the two-step composition), Aristotle task k4 | done |
| C8 composition | C0-C7 give the charter statement for every route: every route that moves the effective price of an exposed asset goes through C2 or C3; every other writer is an initialization, a copy, or an unexposed reset | this file, read against the charter by someone other than its author | open |

## What is left

- C1 and the route inventory of C4 rest on a scan and on the program's tests; neither is a proof.
  Making C1 a proof needs either a frame proof per entry point and per instruction (the effective
  price after the call is the one before, except in the listed writers), which is part of phases 3
  and 4 of the roadmap, or the compiler: the field private to one module whose functions are the
  listed writers. The second does not cover the program as it is, because P1-P3 write the field
  through the program's own byte views of the account; they would have to call the engine instead.
- C8: an independent read of this statement against the charter, then the composition proof.
