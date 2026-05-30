# Inductive Invariant Proof — Progress Report

Status: in progress. This is the start of the **INDUCTIVE** tier defined in
`scripts/audit-proof-strength.md` (Criterion 6), which
`scripts/proof-strength-audit-results.md` correctly concludes the existing v16
suite has **not** reached ("a strong production-code safety/liveness harness set,
not a complete arbitrary-state inductive proof of the whole engine").

All harnesses live in `tests/proofs_v16_inductive.rs` (`#![cfg(kani)]`). The only
production change is one `#[cfg(kani)] pub` wrapper exposing an existing private
core transition (mirroring the existing `kani_charge_account_fee_current_not_atomic`
precedent) — no production logic changed. `cargo test` stays green.

---

## 1. The canonical invariant decomposition

The codebase had **no** `canonical_inv` — `validate_shape` is the only invariant
check, and it is a single monolithic O(N) scan over the `markets` slice (and the
account `legs` array), with two `while` loops. That is exactly what Criterion 6d
flags as making `assume(canonical_inv(engine))` expensive.

This work decomposes the senior-solvency part of `validate_shape` into loop-free
components, each over the **cone of influence** of the transition under test:

| Component | Predicate (loop-free) | Source in `validate_shape` |
|---|---|---|
| `inv_accounting` | `c_tot <= vault ∧ insurance <= vault ∧ c_tot + insurance <= vault` | the two early `<= vault` guards + the `senior = c_tot + insurance + backing_provider_earnings <= vault` check (with `backing_provider_earnings == 0` in a zero-market cone) |
| `inv_aggregates` (delta form) | `c_tot == capital + rest_capital` for one arbitrary target account + an abstract aggregate `rest_capital` | the modular reformulation Criterion 6b asks for, replacing `c_tot == Σ capital[i]` |
| `inv_per_account` | `pnl != i128::MIN ∧ capital <= c_tot` | `validate_non_min_i128` + the fact that an account's capital is part of `c_tot` |
| `inv_domain_budget` | `spent <= budget` per source domain | the per-domain `spent + reserved_atoms <= budget` leg of `validate_domain_shape_for_view` (the budget leg of the audit's `inv_source_domain`) |
| `inv_vault_bound` | `vault <= MAX_VAULT_TVL` | the `vault > MAX_VAULT_TVL` guard |

**Does this correctly capture the invariant?** For the senior-solvency /
aggregate-consistency / per-domain-budget properties: yes, and provably so for
the transitions proven — the cone-of-influence claim is exact (see §3). What it
does **not** yet capture: the source-credit *rate* / *lien* leg of
`inv_source_domain` (`usable_positive_credit <= realizable_backing`), the
`pnl_pos_tot` / `pnl_pos_bound_tot` junior-claim aggregates, OI symmetry, and the
close-progress / recovery-mode structural invariants. Those are future
components.

---

## 2. Transitions proven inductively (fully-symbolic initial state)

Every harness below starts from a **fully symbolic** economic state over the
**full `u128`/`i128` domains** (no `<= 1000` range bound — Criterion 6f), with
`assume(<decomposed components>)`, applies the bare production transition, and
asserts the post-state satisfies the relevant components — `INV(s) ⟹ INV(f(s))`.

| Harness | Transition | Symbolic-ness | Result | Time |
|---|---|---|---|---|
| `..._settle_negative_pnl_preserves_inv_accounting` | `settle_negative_pnl_from_principal_core` | 5 economic scalars, full domain, **zero markets** | SUCCESSFUL, 2/2 cover | 16.7s |
| `..._charge_fee_preserves_inv_accounting` | `charge_account_fee_current_core` | 5 economic scalars + fee, full domain, zero markets | SUCCESSFUL, 2/2 cover | 12.7s |
| `..._settle_negative_pnl_maintains_aggregate_c_tot` | `settle_negative_pnl_from_principal_core` | + abstract `rest_capital` aggregate (any topology) | SUCCESSFUL, 1/1 cover | 12.2s |
| `..._consume_domain_insurance_preserves_domain_and_accounting` | `consume_domain_insurance_for_negative_pnl` | **markets-slice-touching**, full-symbolic economics, 1-market topology | SUCCESSFUL, 1/1 cover | 20.5s |

Comparison point: the closest existing STRONG harness over the same code,
`proof_v16_view_deposit_preserves_c_tot_vault_capital_sum` (concrete fixture +
one `<= 1000` symbolic value, full `validate_shape`), takes **468s**. The
decomposed loop-free `assume` is both **more symbolic** and **~28× faster** —
direct confirmation of Criterion 6d's prediction that the loop-based invariant
assume is the cost driver.

Non-vacuity: each harness has `kani::cover!` checks for the *interesting* branch
(partial vs full settlement; capped vs full fee; non-empty rest-of-system;
nonzero insurance consumed). All satisfied — the solver provably reaches the
real work, not just an early `pnl >= 0` return (Criterion 4).

---

## 3. Why the cone-of-influence / zero-market construction is sound (not a cheat)

`settle_negative_pnl_from_principal_core` and `charge_account_fee_current_core`
read/write **only header scalars** (`vault`, `c_tot`, `insurance`,
`bankruptcy_hlock_active`, `negative_pnl_account_count`) and account-header
scalars (`pnl`, `capital`, `health_cert.valid`). They contain **no
`self.markets[...]` access whatsoever** (verified by reading the full bodies).
So zeroing the `markets` slice does not under-constrain the proof — the
transition provably cannot read those fields, and `backing_provider_earnings`
(the only market-derived term in `inv_accounting`) is identically zero. The
fields left zeroed are precisely the ones Criterion 6e says should be left out of
the cone.

The fourth harness deliberately steps **into** the markets slice to find the
wall, and keeps the economics symbolic there too.

---

## 4. The tractability wall (honest)

The wall is **topology, not economics**.

- **Economics scale freely.** Full-domain `u128`/`i128` symbolic values for
  vault/c_tot/insurance/capital/pnl/budget/spent cost nothing extra once the
  invariant is decomposed and loop-free — the solver prunes everything outside
  the cone. None of the four harnesses needed a range bound.

- **Arbitrary topology does not.** The fourth harness fixes
  `config.max_market_slots = 1` (one asset, two domains) and zeroes the
  source-credit / insurance reservations. The transition's helpers
  (`insurance_domain_index`, `available_domain_insurance` with its
  `while d < configured_domains` loop) require a *configured, indexable* slot, so
  the **shape** must be concrete. A genuinely topology-symbolic proof — e.g. the
  cross-account coupling Criterion 6b names, where settling account `i` changes a
  `haircut_ratio`/source-credit rate that affects account `j` — needs:
  1. a symbolic `EngineAssetSlotV16Account` (which today does **not** derive
     `kani::Arbitrary`; it is a hand-rolled Pod, so `kani::any()` cannot
     synthesize one without an `Arbitrary` impl or a byte-buffer `from_bytes`
     over a fully-symbolic `[u8; size_of]`), and
  2. the source-credit *rate* leg of `inv_source_domain`
     (`usable_positive_credit <= realizable_backing`) expressed loop-free, which
     couples the per-domain bucket/lien/reservation aggregates that
     `validate_source_domain_ledger_parts` checks.

  The `aggregate_c_tot` harness already demonstrates the *modular* half of 6b
  (one target account + abstract `rest_capital`) for the capital aggregate. The
  unmet half is the *cross-domain coupling* through the haircut/source-credit
  rate, which is where the real multi-account interaction lives.

So the current frontier is **not** solver blow-up on large numbers — it is that
the harder invariant legs (source-credit rate realizability) have not yet been
written as loop-free decomposed predicates, and a topology-symbolic state needs a
`kani::Arbitrary` (or symbolic-byte-buffer) path for the slot Pod structs.

---

## 5. Concrete next steps (in order)

1. **Add `kani::Arbitrary` (cfg(kani)) for the slot/domain Pod structs**, or a
   `from_symbolic_bytes` helper, so a single market slot can be made
   *fully* symbolic (not just its two budget scalars). This unblocks
   topology-symbolic per-domain proofs.
2. **Decompose `inv_source_domain`'s rate leg** loop-free: express
   `credit_rate_num <= available_backing / positive_claim_bound` over one
   domain's `(SourceCreditState, BackingBucket, InsuranceCreditReservation)`
   triple, and prove the lien create/consume/release transitions
   (`kani_apply_counterparty_source_credit_lien_delta` et al., already exposed)
   preserve it. These are the transitions whose STRONG proofs already exist and
   run 60–580s — the inductive versions should be both stronger and faster.
3. **Two-account coupling**: instantiate the `aggregate_c_tot` modular pattern
   with a *second concrete account* whose source-credit depends on the first's
   domain, and prove settling account 1 cannot raise account 2's usable credit
   above the post-settlement realizable backing. This is the literal "settling
   account i changes haircut_ratio affecting account j" property of 6b.
4. **Compose the components** into a single `canonical_inv(s)` and re-run each
   transition proving the *whole* predicate is preserved, once all legs exist.

---

## 6. Bottom line

Four transitions now have genuine inductive proofs over fully-symbolic economic
state with a loop-free decomposed invariant — including one that touches the
markets slice and one that reasons over arbitrary surrounding topology via an
abstract aggregate. This is real INDUCTIVE-tier coverage of the
senior-solvency / aggregate-consistency / per-domain-budget invariants, not a
dressed-up fixture. The remaining gap to a *complete* engine-wide inductive proof
is the source-credit rate realizability leg and a topology-symbolic slot
construction (Arbitrary), which §5 sequences.
