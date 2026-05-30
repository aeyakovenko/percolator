#![cfg(kani)]
//! Inductive invariant proofs for the v16 engine.
//!
//! This file is the start of the "INDUCTIVE" tier described in
//! `scripts/audit-proof-strength.md` (Criterion 6). Unlike the STRONG harnesses
//! in `tests/proofs_v16.rs` — which build a concrete `one_market_view_fixture()`
//! and overwrite a few fields with small symbolic values — the harnesses here:
//!
//!   1. Decompose the global solvency invariant into loop-free components
//!      (`inv_accounting`, ...), each over the *cone of influence* of the
//!      transition under test (Criterion 6c/6e), and
//!   2. Start from a *fully symbolic* economic state over the full `u128`/`i128`
//!      field domains with `kani::assume(canonical_inv(s))`, apply the bare
//!      production transition `f`, and prove `canonical_inv(f(s))` (Criterion 6a).
//!
//! The economic scalars that the transition reads or writes are fully symbolic
//! (NO `<= 1000` range bound — Criterion 6f); fields outside the cone of
//! influence are left zeroed and pruned by the solver.
//!
//! Scope of the first flagship transition,
//! `settle_negative_pnl_from_principal_core_not_atomic`:
//!   - reads  : account.pnl, account.capital, header.c_tot, header.vault,
//!              header.insurance, header.negative_pnl_account_count
//!   - writes : account.capital -= paid, header.c_tot -= paid,
//!              account.pnl += paid, (maybe) negative_pnl_account_count -= 1,
//!              bankruptcy_hlock_active, health_cert.valid
//!     where paid = min(account.capital, |pnl|). vault and insurance are NEVER
//!     written. The core also emits a balanced `account_capital_to_realized_loss`
//!     TokenValueFlowProof with vault_before == vault_after.

use percolator::v16::{
    EngineAssetSlotV16Account, Market, MarketGroupV16HeaderAccount, MarketGroupV16ViewMut,
    PortfolioAccountV16Account, PortfolioSourceDomainV16Account, PortfolioV16ViewMut, SideV16,
    V16PodI128, V16PodU128, V16PodU32,
};
use percolator::MAX_VAULT_TVL;

// ---------------------------------------------------------------------------
// Decomposed, loop-free canonical invariant components.
//
// These are written *relative to the scalar aggregates* the transition under
// test touches. Each component is a pure predicate over a bounded, fixed set of
// fields — no scan over the `markets` slice or the account `legs` array — so
// `assume(component)` does not blow up the solver (Criterion 6d).
// ---------------------------------------------------------------------------

/// `inv_accounting`: the core protocol solvency invariant.
///
/// Senior obligations (account capital totals `c_tot`, plus protocol
/// `insurance`) must never exceed the quote `vault` backing them. This is the
/// loop-free projection of the production `validate_shape` senior-solvency
/// check (`v16.rs`: `c_tot + insurance + backing_provider_earnings <= vault`,
/// with `backing_provider_earnings == 0` in the zero-market cone here).
fn inv_accounting(vault: u128, c_tot: u128, insurance: u128) -> bool {
    // Componentwise <= vault (matches the two early `validate_shape` guards) ...
    c_tot <= vault
        && insurance <= vault
        // ... and the joint senior-solvency bound.
        && match c_tot.checked_add(insurance) {
            Some(senior) => senior <= vault,
            None => false,
        }
}

/// `inv_aggregates` (delta form): the header aggregate `c_tot` equals the sum
/// of the target account's `capital` and an abstract aggregate `rest_capital`
/// representing the capital of every OTHER account in the instance.
///
/// This is the loop-free, modular reformulation Criterion 6b asks for: rather
/// than scanning an unbounded account table to assert `c_tot == Σ capital[i]`,
/// we reason about ONE arbitrary target account plus a single abstract summary
/// value `rest_capital` for the rest of the system. A transition maintains the
/// aggregate iff it preserves this equation with `rest_capital` held constant.
fn inv_aggregates(c_tot: u128, capital: u128, rest_capital: u128) -> bool {
    match capital.checked_add(rest_capital) {
        Some(sum) => c_tot == sum,
        None => false,
    }
}

/// `inv_per_account`: per-account economic well-formedness needed for the
/// transition to be meaningful. `pnl == i128::MIN` is rejected everywhere as a
/// non-canonical persisted value (`validate_non_min_i128`), and account capital
/// is part of `c_tot` so it cannot exceed it. We keep this tight to exactly the
/// account-local facts the transition relies on.
fn inv_per_account(capital: u128, pnl: i128, c_tot: u128) -> bool {
    pnl != i128::MIN && capital <= c_tot
}

/// Global well-formedness bound used by `validate_shape`.
fn inv_vault_bound(vault: u128) -> bool {
    vault <= MAX_VAULT_TVL
}

// ---------------------------------------------------------------------------
// Symbolic state construction (cone-of-influence only).
// ---------------------------------------------------------------------------

/// Build a fully-symbolic minimal engine + account state where ONLY the five
/// economic scalars in the transition's cone of influence are symbolic; every
/// other field is zeroed (outside the cone, pruned by the solver).
///
/// We deliberately do NOT call `one_market_view_fixture()`: that pins the
/// config, market slot, provenance, and asset lifecycle to concrete values,
/// which is exactly the "constructed state" pattern Criterion 6a flags as
/// STRONG-not-INDUCTIVE. The zero-market header here means
/// `backing_provider_earnings == 0`, so `inv_accounting` is the complete
/// senior-solvency obligation for this cone.
struct SymbolicState {
    header: MarketGroupV16HeaderAccount,
    markets: [Market<u64>; 0],
    account_header: PortfolioAccountV16Account,
    source_domains: [PortfolioSourceDomainV16Account; 0],
}

fn symbolic_state(
    vault: u128,
    c_tot: u128,
    insurance: u128,
    capital: u128,
    pnl: i128,
    negative_pnl_account_count: u64,
) -> SymbolicState {
    // Zeroed Pod base: all fields outside the economic cone are concrete-zero.
    let mut header = MarketGroupV16HeaderAccount::default();
    header.vault = V16PodU128::new(vault);
    header.c_tot = V16PodU128::new(c_tot);
    header.insurance = V16PodU128::new(insurance);
    header.negative_pnl_account_count =
        percolator::v16::V16PodU64::new(negative_pnl_account_count);

    let mut account_header = PortfolioAccountV16Account::default();
    account_header.capital = V16PodU128::new(capital);
    account_header.pnl = V16PodI128::new(pnl);

    SymbolicState {
        header,
        markets: [],
        account_header,
        source_domains: [],
    }
}

// ---------------------------------------------------------------------------
// Flagship inductive proof:
//   INV(s)  =>  INV(settle_negative_pnl_from_principal_core(s))
//
// over a fully-symbolic economic state.
// ---------------------------------------------------------------------------

#[kani::proof]
#[kani::solver(cadical)]
fn proof_v16_inductive_settle_negative_pnl_preserves_inv_accounting() {
    // Fully symbolic over the FULL field domains — no `<= 1000` bound.
    let vault: u128 = kani::any();
    let c_tot: u128 = kani::any();
    let insurance: u128 = kani::any();
    let capital: u128 = kani::any();
    let pnl: i128 = kani::any();
    let negative_pnl_account_count: u64 = kani::any();

    // assume(canonical_inv(s)) — decomposed, loop-free.
    kani::assume(inv_vault_bound(vault));
    kani::assume(inv_accounting(vault, c_tot, insurance));
    kani::assume(inv_per_account(capital, pnl, c_tot));
    // The production transition decrements `negative_pnl_account_count` iff a
    // negative account is cured to exactly zero; assume it is a faithful count
    // (>= 1 when this account is itself negative) so the checked_sub cannot be
    // a spurious underflow unrelated to the accounting invariant. This is an
    // honest precondition: a real engine state with a negative-PnL account has
    // counted it. We let it stay symbolic above 1.
    kani::assume(pnl >= 0 || negative_pnl_account_count >= 1);

    let pnl_before = pnl;
    let capital_before = capital;
    let c_tot_before = c_tot;

    let mut st = symbolic_state(
        vault,
        c_tot,
        insurance,
        capital,
        pnl,
        negative_pnl_account_count,
    );
    let mut market = MarketGroupV16ViewMut::new(&mut st.header, &mut st.markets);
    let mut account = PortfolioV16ViewMut::new(&mut st.account_header, &mut st.source_domains);

    // Cover: the interesting partial-settlement branch (capital < |loss|) is
    // reachable — guards against a vacuous proof where the solver only ever
    // takes the early `pnl >= 0` return (Criterion 4).
    kani::cover!(
        pnl < 0 && capital > 0 && capital < pnl.unsigned_abs(),
        "partial principal settlement: capital strictly below the loss"
    );
    kani::cover!(
        pnl < 0 && capital >= pnl.unsigned_abs() && pnl.unsigned_abs() > 0,
        "full principal settlement: capital covers the whole loss"
    );

    let result = market.kani_settle_negative_pnl_from_principal_core_not_atomic(&mut account);

    let vault_after = market.header.vault.get();
    let c_tot_after = market.header.c_tot.get();
    let insurance_after = market.header.insurance.get();
    let capital_after = account.header.capital.get();
    let pnl_after = account.header.pnl.get();

    // ----- The inductive conclusion: INV(f(s)) -----

    // (1) The accounting invariant is preserved on EVERY path (Ok and Err).
    //     This is the heart of the proof: senior solvency cannot be broken by
    //     this transition over ANY valid symbolic input.
    assert!(
        inv_accounting(vault_after, c_tot_after, insurance_after),
        "settle_negative_pnl must preserve inv_accounting"
    );

    // (2) vault and insurance are invariant under this transition (cone of
    //     influence claim — they are read for the value-flow proof but never
    //     written).
    assert_eq!(vault_after, vault, "vault is not mutated by principal settlement");
    assert_eq!(
        insurance_after, insurance,
        "insurance is not mutated by principal settlement"
    );

    // (3) The exact transition semantics, expressed as a state-delta law,
    //     proven over the full domain (not pinned to a fixture):
    //       paid = if pnl < 0 && min(capital,|pnl|) > 0 { min(capital,|pnl|) } else 0
    //       c_tot'   = c_tot - paid
    //       capital' = capital - paid
    //       pnl'     = pnl + paid
    //     and senior solvency is preserved precisely because c_tot only ever
    //     decreases while vault/insurance hold.
    let paid = if pnl_before < 0 {
        let loss = pnl_before.unsigned_abs();
        capital_before.min(loss)
    } else {
        0
    };
    if result.is_ok() {
        assert_eq!(*result.as_ref().unwrap(), paid, "returned paid matches the delta law");
        assert_eq!(c_tot_after, c_tot_before - paid, "c_tot decreases by exactly paid");
        assert_eq!(capital_after, capital_before - paid, "capital decreases by exactly paid");
        assert_eq!(
            pnl_after,
            pnl_before + paid as i128,
            "pnl increases by exactly paid"
        );
        // c_tot is monotonically non-increasing -> with vault, insurance held,
        // inv_accounting can only become "more true". Re-stated as an explicit
        // monotonicity fact:
        assert!(c_tot_after <= c_tot_before, "c_tot is non-increasing");
    }
}

// ---------------------------------------------------------------------------
// Second inductive proof — a transition that MOVES value between two senior
// buckets rather than just shrinking one.
//
//   charge_account_fee_current_core: when pnl >= 0,
//     charged   = min(requested_fee, capital)
//     c_tot'    = c_tot - charged
//     capital'  = capital - charged
//     insurance'= insurance + charged
//   so c_tot + insurance is INVARIANT (value moves capital -> insurance), and
//   inv_accounting is preserved because:
//     - c_tot' <= c_tot <= vault
//     - insurance' = insurance + charged <= insurance + capital
//                  <= insurance + c_tot <= vault   (needs inv_per_account)
//     - c_tot' + insurance' = c_tot + insurance <= vault
//
// This is a strictly stronger accounting test than #1: it exercises the branch
// where `insurance` is WRITTEN, so the proof genuinely depends on the
// `capital <= c_tot` component of the assumed invariant — not just monotonicity.
// ---------------------------------------------------------------------------

#[kani::proof]
#[kani::solver(cadical)]
fn proof_v16_inductive_charge_fee_preserves_inv_accounting() {
    let vault: u128 = kani::any();
    let c_tot: u128 = kani::any();
    let insurance: u128 = kani::any();
    let capital: u128 = kani::any();
    let pnl: i128 = kani::any();
    let requested_fee: u128 = kani::any();

    kani::assume(inv_vault_bound(vault));
    kani::assume(inv_accounting(vault, c_tot, insurance));
    kani::assume(inv_per_account(capital, pnl, c_tot));

    let c_tot_before = c_tot;
    let insurance_before = insurance;
    let capital_before = capital;

    // negative_pnl_account_count is outside this transition's cone; leave it 0.
    let mut st = symbolic_state(vault, c_tot, insurance, capital, pnl, 0);
    let mut market = MarketGroupV16ViewMut::new(&mut st.header, &mut st.markets);
    let mut account = PortfolioV16ViewMut::new(&mut st.account_header, &mut st.source_domains);

    kani::cover!(
        pnl >= 0 && capital > 0 && requested_fee > 0 && requested_fee <= capital,
        "fee fully charged from capital"
    );
    kani::cover!(
        pnl >= 0 && capital > 0 && requested_fee > capital,
        "fee capped at available capital"
    );

    let result = market.kani_charge_account_fee_current_not_atomic(&mut account, requested_fee);

    let vault_after = market.header.vault.get();
    let c_tot_after = market.header.c_tot.get();
    let insurance_after = market.header.insurance.get();
    let capital_after = account.header.capital.get();

    // (1) inv_accounting preserved on every path — including the branch that
    //     grows `insurance`. This is where the assumed `capital <= c_tot`
    //     component is load-bearing.
    assert!(
        inv_accounting(vault_after, c_tot_after, insurance_after),
        "charge_fee must preserve inv_accounting"
    );

    // (2) vault is invariant; senior total c_tot + insurance is invariant
    //     (value moved between senior buckets, not created or destroyed).
    assert_eq!(vault_after, vault, "vault unchanged by fee charge");
    assert_eq!(
        c_tot_after + insurance_after,
        c_tot_before + insurance_before,
        "senior total (c_tot + insurance) conserved across capital->insurance move"
    );

    // (3) exact delta law on the Ok path.
    let charged = if pnl >= 0 {
        let c = requested_fee.min(capital_before);
        c
    } else {
        0
    };
    if result.is_ok() {
        assert_eq!(*result.as_ref().unwrap(), charged, "returned charged matches delta law");
        assert_eq!(c_tot_after, c_tot_before - charged, "c_tot decreases by charged");
        assert_eq!(
            insurance_after,
            insurance_before + charged,
            "insurance increases by charged"
        );
        assert_eq!(capital_after, capital_before - charged, "capital decreases by charged");
    }
}

// ---------------------------------------------------------------------------
// Third inductive proof — MODULAR MULTI-ACCOUNT AGGREGATE MAINTENANCE.
//
// This is the property Criterion 6b names as the real gap: a 1-account proof
// makes `c_tot == Σ capital[i]` trivially true. Here we reason over ONE
// arbitrary target account plus an abstract aggregate `rest_capital` standing
// in for the (unbounded) rest of the account table, and prove the transition
// maintains the aggregate delta:
//
//   inv_aggregates(c_tot, capital, rest_capital):  c_tot == capital + rest_capital
//
// settle_negative_pnl decrements BOTH `c_tot` and the target `capital` by the
// same `paid`, while never touching any other account. Therefore, with
// `rest_capital` held constant, the aggregate equation is preserved — i.e. the
// transition cannot make the global capital total drift relative to the sum of
// account capitals, for ANY value of the rest-of-system aggregate.
//
// `rest_capital` is fully symbolic, so the proof holds for every possible
// surrounding topology (any number of other accounts with any capitals summing
// to `rest_capital`). This is the abstract "rest of the system" of 6b.
// ---------------------------------------------------------------------------

#[kani::proof]
#[kani::solver(cadical)]
fn proof_v16_inductive_settle_negative_pnl_maintains_aggregate_c_tot() {
    let vault: u128 = kani::any();
    let insurance: u128 = kani::any();
    let capital: u128 = kani::any();
    let pnl: i128 = kani::any();
    let rest_capital: u128 = kani::any(); // abstract aggregate of all OTHER accounts
    let negative_pnl_account_count: u64 = kani::any();

    // c_tot is DERIVED from the aggregate, not independently symbolic: the
    // header aggregate is, by inv_aggregates, exactly target + rest.
    let c_tot = match capital.checked_add(rest_capital) {
        Some(v) => v,
        None => return, // not a valid state; nothing to prove
    };

    kani::assume(inv_vault_bound(vault));
    kani::assume(inv_accounting(vault, c_tot, insurance));
    kani::assume(inv_aggregates(c_tot, capital, rest_capital));
    kani::assume(inv_per_account(capital, pnl, c_tot));
    kani::assume(pnl >= 0 || negative_pnl_account_count >= 1);

    let capital_before = capital;

    let mut st = symbolic_state(
        vault,
        c_tot,
        insurance,
        capital,
        pnl,
        negative_pnl_account_count,
    );
    let mut market = MarketGroupV16ViewMut::new(&mut st.header, &mut st.markets);
    let mut account = PortfolioV16ViewMut::new(&mut st.account_header, &mut st.source_domains);

    kani::cover!(
        pnl < 0 && rest_capital > 0 && capital > 0 && capital < pnl.unsigned_abs(),
        "aggregate maintenance with non-empty rest-of-system and partial settle"
    );

    let result = market.kani_settle_negative_pnl_from_principal_core_not_atomic(&mut account);

    let c_tot_after = market.header.c_tot.get();
    let capital_after = account.header.capital.get();

    // The inductive aggregate conclusion: c_tot' == capital' + rest_capital.
    // rest_capital is unchanged (no other account was touched), so the global
    // capital aggregate stays exactly consistent with the per-account sum.
    assert!(
        inv_aggregates(c_tot_after, capital_after, rest_capital),
        "settle_negative_pnl maintains c_tot == capital + rest_capital for arbitrary rest"
    );

    if result.is_ok() {
        // The target account's capital and the global c_tot moved by the SAME
        // delta — the precise reason the aggregate is maintained.
        let target_delta = capital_before - capital_after;
        let c_tot_delta = c_tot - c_tot_after;
        assert_eq!(
            target_delta, c_tot_delta,
            "target capital delta equals c_tot delta (no aggregate drift)"
        );
    }
}

// ---------------------------------------------------------------------------
// Fourth inductive proof — MARKETS-SLICE-TOUCHING transition.
//
// This pushes past the zero-market cone into the per-domain insurance ledger,
// which is exactly the territory Criterion 6b/6d describe as harder: the
// transition `consume_domain_insurance_for_negative_pnl` READS and WRITES the
// `markets` slice (the asset slot's domain budget/spent counters) and decrements
// `header.insurance`, with a `while d < configured_domains` loop inside
// `available_domain_insurance`.
//
// HONEST symbolic-vs-bounded breakdown for this harness:
//   * SYMBOLIC (full domain) : header.vault, header.insurance, account.pnl,
//                              account.capital, slot.domain_budget,
//                              slot.domain_spent.
//   * FIXED (topology, not economics): config.max_market_slots = 1 (one asset,
//     two domains) and zeroed source-credit / insurance reservations. The
//     transition's helpers require a configured slot to index, so the *shape*
//     (1 market) is concrete while the *economic scalars* on it are symbolic.
//     This is still strictly more symbolic than the STRONG harnesses, which fix
//     the budget/spent/insurance via a fixture and only vary one small value.
//
// Invariants proven preserved over this fully-symbolic-economics state:
//   inv_accounting: insurance' <= vault          (insurance only decreases)
//   inv_domain_budget: spent' <= budget          (the per-domain ledger bound
//                                                  the audit names as
//                                                  inv_source_domain's budget leg)
// ---------------------------------------------------------------------------

/// `inv_domain_budget`: per-source-domain insurance accounting bound, the
/// budget leg of the audit's `inv_source_domain` (`Σ liens <= reserved
/// backing`). Loop-free: stated for one domain's (spent, budget) pair.
fn inv_domain_budget(spent: u128, budget: u128) -> bool {
    spent <= budget
}

#[kani::proof]
#[kani::solver(cadical)]
fn proof_v16_inductive_consume_domain_insurance_preserves_domain_and_accounting() {
    let vault: u128 = kani::any();
    let insurance: u128 = kani::any();
    let capital: u128 = kani::any();
    let pnl: i128 = kani::any();
    let domain_budget: u128 = kani::any();
    let domain_spent: u128 = kani::any();

    // assume the relevant decomposed invariant components.
    kani::assume(inv_vault_bound(vault));
    kani::assume(insurance <= vault); // inv_accounting leg in the cone
    kani::assume(pnl != i128::MIN);
    kani::assume(inv_domain_budget(domain_spent, domain_budget));
    // Live source-credit insurance reservations are zero in this state
    // (reservations zeroed), so the validate_shape coupling
    // `domain_spent + reserved_atoms <= budget` reduces to `domain_spent <=
    // budget`, already assumed. budget_remaining math below cannot underflow.

    // Build a one-market state: topology fixed at 1 asset / 2 domains, economic
    // scalars on the slot symbolic.
    let mut header = MarketGroupV16HeaderAccount::default();
    header.config.max_market_slots = V16PodU32::new(1);
    header.vault = V16PodU128::new(vault);
    header.insurance = V16PodU128::new(insurance);

    let mut slot = EngineAssetSlotV16Account::default();
    // bankrupt_side = Long => opposite_side = Short => domain index = asset*2+1
    // => the SHORT domain ledger is the one consumed.
    slot.insurance_domain_budget_short = V16PodU128::new(domain_budget);
    slot.insurance_domain_spent_short = V16PodU128::new(domain_spent);
    let mut markets = [Market::new(0u64, slot)];

    let mut account_header = PortfolioAccountV16Account::default();
    account_header.capital = V16PodU128::new(capital);
    account_header.pnl = V16PodI128::new(pnl);
    let mut source_domains: [PortfolioSourceDomainV16Account; 0] = [];

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header, &mut source_domains);

    kani::cover!(
        pnl < 0 && domain_budget > domain_spent && insurance > 0,
        "domain insurance actually consumed (nonzero used)"
    );

    let result =
        market.kani_consume_domain_insurance_for_negative_pnl(0, SideV16::Long, &mut account);

    let vault_after = market.header.vault.get();
    let insurance_after = market.header.insurance.get();
    let slot_after = &market.markets[0].engine;
    let spent_after = slot_after.insurance_domain_spent_short.get();
    let budget_after = slot_after.insurance_domain_budget_short.get();

    // (1) inv_accounting leg: insurance never rises above vault.
    assert!(
        insurance_after <= vault_after,
        "consume_domain_insurance preserves insurance <= vault"
    );
    // (2) per-domain budget bound preserved: the ledger cannot over-spend the
    //     configured domain budget, for ANY symbolic budget/spent/insurance.
    assert!(
        inv_domain_budget(spent_after, budget_after),
        "consume_domain_insurance preserves spent <= budget"
    );
    // (3) vault is invariant; budget is invariant (only `spent` moves up).
    assert_eq!(vault_after, vault, "vault unchanged by insurance consumption");
    assert_eq!(budget_after, domain_budget, "domain budget unchanged");
    // (4) insurance is non-increasing and spent is non-decreasing.
    assert!(insurance_after <= insurance, "insurance non-increasing");
    assert!(spent_after >= domain_spent, "domain spent non-decreasing");
}
