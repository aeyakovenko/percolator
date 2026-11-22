# Global invariant, part M: the market totals that mirror account state (engine 71076137, program e1295564)

Status: DRAFT, written 2026-11-22 from the engine source and from the program's executable census
(`tests/support/fuzz_model.rs`: `assert_market_stock_census`, `assert_source_claim_bound_attribution`,
`assert_public_encumbrance_census`), which the stateful suite checks after every public transition. Lines marked
**exact** are the census's own equalities. Lines marked **to confirm** state what the engine maintains by reading its
update sites; they need the same treatment as any statement here (an independent read against the code) before the
T*.mirror pieces are stated against them.

## Scope

One market account `m` (its header, its asset slots and its per-domain records) and `P(m)`, the set of portfolio
accounts materialized against `m` (an account is materialized from `InitPortfolio` until `ClosePortfolio` or the
terminal close dematerializes it). The engine never sees all of `P(m)` at once, so part M cannot be checked at run
time; it is a statement about the reachable states, proved by induction from `InitMarket` (G.reach) using, for each
entry point, its local form (T*.mirror): the total minus the contribution of the accounts the call is given is
unchanged, and the contribution of those accounts changes exactly as the total does.

Notation: for an account `a`, `pnl(a)`, `capital(a)`, `legs(a)` (its active legs), `src(a)` (its occupied source-domain
slots). `[x]` is 1 if `x` holds, else 0. `BS` is `BOUND_SCALE`.

## M.H: header totals over the accounts (exact; the census's aggregate and count checks)

| Line | Header field | Equals |
| --- | --- | --- |
| M.H1 | `c_tot` | Σ_{a ∈ P(m)} `capital(a)` |
| M.H2 | `pnl_pos_tot` | Σ_a `max(pnl(a), 0)` |
| M.H3 | `pnl_pos_bound_tot` | Σ_a `max(pnl(a), 0)` (the same sum, in atoms) |
| M.H4 | `pnl_pos_bound_tot_num` | `BS ×` Σ_a `max(pnl(a), 0)` |
| M.H5 | `pnl_matured_pos_tot` | 0 (no deployed path matures positive PnL into this total; the census pins it) |
| M.H6 | `materialized_portfolio_count` | `|P(m)|` |
| M.H7 | `stale_certificate_count` | Σ_a `[stale_state(a)]` |
| M.H8 | `b_stale_account_count` | Σ_a `[b_stale_state(a)]` |
| M.H9 | `negative_pnl_account_count` | Σ_a `[pnl(a) < 0]` |

The census also sums `cancel_deposit_escrow(a)` over the accounts; which header or stock class that sum must equal
is part of part C (custody, the stock reconciliation of engine spec section 5.1.1), not of M.

## M.D: per-domain totals over the accounts (exact for claims; to confirm for the lien and impaired counters)

For every source domain `d` of `m`:

| Line | Market field | Equals |
| --- | --- | --- |
| M.D1 | `source_credit[d].positive_claim_bound_num` | Σ_a Σ_{s ∈ src(a), s.domain = d} `s.source_claim_bound_num` (exact: the census's attribution) |
| M.D2 | `source_credit[d]` liened claim counters | Σ_a Σ_{s ∈ src(a), s.domain = d} `s.source_claim_liened_num` (to confirm: name the market field and its update sites) |
| M.D3 | `source_credit[d]` impaired claim counters | Σ_a Σ_{s ∈ src(a), s.domain = d} `s.source_claim_impaired_num` (to confirm, as M.D2) |

And for every account in Live mode (an account-local fact, part V, stated here because the census checks it with
M.D1): Σ_{s ∈ src(a)} `s.source_claim_bound_num = BS × max(pnl(a), 0)`.

Where M.D2 and M.D3 are defined operationally: `assert_reservation_encumbrance_census`
(program `tests/support/fuzz_model.rs`, from line 1924 at e1295564). Per account source slot it splits the liened face
into `source_claim_counterparty_liened_num` and `source_claim_insurance_liened_num` (their sum must equal
`source_claim_liened_num`), and it relates them to the domain's `valid_liened_backing_num` / `valid_liened_insurance_num`
and to the domain reservations (`insurance_credit_reservations`), including the faces held by pending close ledgers.
Those relations are the lines M.D2 and M.D3 are to state.

## M.A: per-asset side totals over the legs (to confirm)

For every asset slot `i` of `m` and side `σ ∈ {long, short}`, with `L(i, σ)` the active legs of all accounts in `P(m)`
on asset `i`, side `σ`, in the side's current epoch (legs of an earlier epoch are pending reset; how they count is
part of what must be confirmed):

| Line | Asset field | Relation | Engine sites that move it with a leg |
| --- | --- | --- | --- |
| M.A1 | `stored_pos_count_σ` | = `|L(i, σ)|` | `add_open_interest_for_new_position` (+1), leg detach and reset paths |
| M.A2 | `raw_basis_sum_σ_q` | = Σ_{ℓ ∈ L} `|ℓ.basis_pos_q|` | as M.A1 (+ `abs_q` at open) |
| M.A3 | `loss_weight_sum_σ` | = Σ_{ℓ ∈ L} `ℓ.loss_weight` | as M.A1 |
| M.A4 | `oi_eff_σ_q` | ≤ Σ_{ℓ ∈ L} `ceil(|ℓ.basis_pos_q| × A_σ / ℓ.a_basis)`, with the difference bounded by the side's rounding residue; equality at `A_σ = ADL_ONE` | open: +`abs_q`; ADL and reduction paths write it from kernels; the #132 residue (a side can reach `oi_eff = 0` with raw basis left, `A_σ = 1`) shows it is not a plain sum |
| M.A5 | `stale_account_count_σ` | = the legs of `L(i, σ)` not yet settled to the side's current K/F cohort | `kernel_mark_kf_stale_cohorts` sets it to `stored_pos_count_σ` when the side's K/F target moves; each settlement lowers it |
| M.A6 | `pending_obligation_count_σ` | = the legs of `L(i, σ)` (and of accounts whose leg was detached) that still owe a pending loss obligation | the obligation create and settle paths |

## What has to happen before the T*.mirror pieces use this

1. Confirm each "to confirm" line against the engine (every write site of the field, by the write-set scan of
   `header_writes.py` extended to asset and domain fields), and state M.A4 exactly (the residue term).
2. An independent read of this file against the engine and the census, as for INV-045's statement.
3. Then each entry point's `T*.mirror` piece is stated as: for the accounts the call is given, the change of each
   line's right side over those accounts equals the change of its left side.
