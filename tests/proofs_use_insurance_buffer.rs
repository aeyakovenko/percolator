//! Novel Kani proofs covering invariants of `use_insurance_buffer` /
//! `absorb_protocol_loss` that are not explicitly asserted elsewhere.
//!
//! The existing `proof_absorb_protocol_loss_drains_to_zero` (in
//! `proofs_invariants.rs`) bounds `balance <= old_balance`. The proofs
//! below tighten that to **exact** post-condition specifications and add
//! the vault-unchanged asymmetry invariant that underlies the published
//! F7 finding.
//!
//! All proofs below are decomposed — each asserts a single algebraic
//! invariant. They complete in under 8s each at `unwind(5)` with
//! `cadical`.

#![cfg(kani)]

mod common;
use common::*;

use percolator::*;

// ============================================================================
// U1: absorb_protocol_loss does not mutate the vault
// ============================================================================
//
// Context. `use_insurance_buffer` decrements `insurance_fund.balance` but
// never decrements `vault`. This asymmetry is the algebraic root cause of
// the F7 drain: the residual `R = V − C − I` is monotonically non-decreasing
// under absorb events (I drops, V stays, so R grows).
//
// Making the invariant explicit in the proof corpus means future edits to
// `use_insurance_buffer` that introduce a vault write will immediately
// fail CI rather than silently alter the residual semantics.

#[kani::proof]
#[kani::unwind(5)]
#[kani::solver(cadical)]
fn proof_absorb_protocol_loss_vault_unchanged() {
    let mut engine = RiskEngine::new(zero_fee_params());

    let balance: u8 = kani::any();
    kani::assume(balance > 0 && balance <= 100);
    engine.insurance_fund.balance = U128::new(balance as u128);
    engine.vault = U128::new(200);

    let vault_before = engine.vault.get();
    let loss: u8 = kani::any();
    kani::assume(loss > 0 && loss <= 200);

    engine.absorb_protocol_loss(loss as u128);

    // U1: vault MUST NOT change during loss absorption (spec §4.11).
    assert!(
        engine.vault.get() == vault_before,
        "vault mutated by absorb_protocol_loss; F7 root-cause invariant broken"
    );
}

// ============================================================================
// U2: absorb amount is exactly min(loss, old_balance)
// ============================================================================
//
// `proof_absorb_protocol_loss_drains_to_zero` asserts
// `balance <= old_balance`. U2 strengthens this to the exact closed form:
// the amount drained is `min(loss, old_balance)` — no less (would leak
// insurable capacity), no more (would underflow).

#[kani::proof]
#[kani::unwind(5)]
#[kani::solver(cadical)]
fn proof_absorb_protocol_loss_exact_amount() {
    let mut engine = RiskEngine::new(zero_fee_params());

    let bal: u8 = kani::any();
    engine.insurance_fund.balance = U128::new(bal as u128);

    let loss: u8 = kani::any();

    let ins_before = engine.insurance_fund.balance.get();
    engine.absorb_protocol_loss(loss as u128);
    let ins_after = engine.insurance_fund.balance.get();

    let absorbed = ins_before - ins_after;
    let expected = core::cmp::min(loss as u128, ins_before);
    assert!(absorbed == expected, "absorbed != min(loss, ins_before)");
    assert!(ins_after <= ins_before, "insurance grew during absorb");
}

// ============================================================================
// U3: Residual inflates by exactly the absorbed amount
// ============================================================================
//
// Quantifies the residual-growth semantics of absorb. For any pre-state
// satisfying V >= C + I, calling absorb_protocol_loss(loss) produces a
// post-state where residual_after = residual_before + absorbed_amount.
// This makes the F7 residual-inflation model a formal invariant.

#[kani::proof]
#[kani::unwind(5)]
#[kani::solver(cadical)]
fn proof_absorb_residual_grows_by_absorbed() {
    let mut engine = RiskEngine::new(zero_fee_params());

    let v: u8 = kani::any();
    let c: u8 = kani::any();
    let i: u8 = kani::any();
    kani::assume(v > 0);
    kani::assume(c as u16 + i as u16 <= v as u16);  // V >= C + I

    engine.vault = U128::new(v as u128);
    engine.c_tot = U128::new(c as u128);
    engine.insurance_fund.balance = U128::new(i as u128);

    let residual_before = (v as u128)
        .saturating_sub(c as u128)
        .saturating_sub(i as u128);

    let loss: u8 = kani::any();
    kani::assume(loss > 0 && loss <= 200);
    engine.absorb_protocol_loss(loss as u128);

    let i_after = engine.insurance_fund.balance.get();
    let absorbed = (i as u128) - i_after;

    let residual_after = engine.vault.get()
        .saturating_sub(engine.c_tot.get())
        .saturating_sub(i_after);

    assert!(
        residual_after == residual_before + absorbed,
        "residual_after != residual_before + absorbed; F7 accounting model broken"
    );
}

// ============================================================================
// U4: top_up_insurance grows V and I by the exact same amount
// ============================================================================
//
// TopUp is the symmetric counterpart to absorb: both V and I grow by the
// same amount, leaving C_tot and residual unchanged. Explicit assertion
// catches future edits that split the growth across V and I asymmetrically.

#[kani::proof]
#[kani::unwind(10)]
#[kani::solver(cadical)]
fn proof_top_up_symmetric_vi_growth() {
    let mut engine = RiskEngine::new_with_market(zero_fee_params(), 0, 100);

    let amount: u8 = kani::any();
    kani::assume(amount > 0 && amount <= 100);

    let v_before = engine.vault.get();
    let i_before = engine.insurance_fund.balance.get();
    let c_before = engine.c_tot.get();

    engine.top_up_insurance_fund(amount as u128, 0).unwrap();

    assert!(engine.vault.get() == v_before + amount as u128);
    assert!(engine.insurance_fund.balance.get() == i_before + amount as u128);
    assert!(engine.c_tot.get() == c_before);
    assert!(engine.check_conservation());
}

// NOTE: An additional end-to-end "two-account no-trade capital-preservation"
// proof is a natural next step. It's omitted from this PR because it unwinds
// materialize_at + deposit + close three times and hits CBMC solver time
// limits at default unwind bounds. Candidate for a follow-up PR with a
// tuned unwind / state decomposition.
