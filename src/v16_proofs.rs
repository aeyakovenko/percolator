//! Kani proof harnesses for the v16 engine (contract + closure layers).
//!
//! This file is NON-PRODUCTION: it is compiled only under `cfg(kani)` with
//! the `contracts` or `closure` feature, and is included as a private child
//! module of `v16` so the harnesses can reach the engine's private items.
//! Keeping it out of v16.rs minimises the production audit surface there.
//! See scripts/contracts_runner.sh for the function-contract runner.

#![allow(unused_imports)]

use super::*;
use crate::wide_math::{checked_mul_div_ceil_u256, U256};
use crate::{BOUND_SCALE, MAX_VAULT_TVL, V16_TOKEN_VALUE_CLASS_COUNT};

// ===================== KANI FUNCTION-CONTRACT LAYER =====================
// Built ONLY by scripts/contracts_runner.sh (cargo feature `contracts` +
// CLI -Z function-contracts + a separate CARGO_TARGET_DIR). The main proof
// suite never compiles this layer: the function-contracts pass slows
// kani-compiler ~5x crate-wide, and stub_verified composition havocs returns
// into ensures-constrained symbolic values (see the elimination table in
// tests/proofs_v16.rs, row (g)). The layer therefore holds LEAF contract
// checks only — machine-checked interface documentation that future kani
// versions may compose.
//
// PUBLIC-OP CONTRACT BOUNDARY (probe verdict, 2026-06-11): a contract on a
// public value-mover (deposit_not_atomic: requires + modifies(self.header,
// account.header) + old()-lockstep ensures) times out at 1800s when checked
// over SYMBOLIC value fields (2^64 range) with real account validation — the
// scale at which a contract would add power beyond the suite. At suite scale
// (u8-range constructed states) it would pass but adds no evidentiary weight
// over the existing suite proofs asserting identical postconditions. The
// review-proposed compositional public-contract program (frame contracts on
// every public op over arbitrary valid states) is therefore closed at this
// kani generation: the contract layer's power is leaf deltas + &mut-self
// in-place mutators (P5) + flow transits; public-op envelopes stay with the
// suite (constructed symbolic), closure layer (any-state deltas), fuzz
// (sequences), and runtime validation (every execution).
//
// NOTE: a contract on source_credit_lien_amounts_for_
// effective was dropped — combining proof_for_contract with a kani::stub of
// its U256 division helper is pathologically slow at the solver level (1800s+
// warm) while sibling checks take seconds; its full-rate property is covered
// by the standalone suite proofs. CONFIRMED PATTERN (second instance:
// prepare_insurance_lien_consume_delta, 1800s+ even with division-free
// ensures): leaves whose BODY divides by BOUND_SCALE with a symbolic operand
// are not contract-checkable in this toolchain — their delta semantics stay
// with the standalone suite proofs, which fix the operands concretely.

#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::prepare_counterparty_lien_consume_delta)]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn contract_check_prepare_counterparty_lien_consume_delta() {
    let bucket = BackingBucketV16 {
        market_id: kani::any(),
        fresh_unliened_backing_num: kani::any(),
        valid_liened_backing_num: kani::any(),
        consumed_liened_backing_num: kani::any(),
        impaired_liened_backing_num: kani::any(),
        expiry_slot: kani::any(),
        status: BackingBucketStatusV16::Fresh,
        utilization_fee_earnings: kani::any(),
    };
    let source = SourceCreditStateV16 {
        positive_claim_bound_num: kani::any(),
        exact_positive_claim_num: kani::any(),
        fresh_reserved_backing_num: kani::any(),
        valid_liened_backing_num: kani::any(),
        impaired_liened_backing_num: kani::any(),
        spent_backing_num: kani::any(),
        provider_receivable_num: kani::any(),
        insurance_credit_reserved_num: kani::any(),
        valid_liened_insurance_num: kani::any(),
        impaired_liened_insurance_num: kani::any(),
        credit_rate_num: kani::any(),
        credit_epoch: kani::any(),
    };
    let amount: u128 = kani::any();
    kani::assume(amount < 1u128 << 96);
    kani::assume(bucket.valid_liened_backing_num < 1u128 << 96);
    kani::assume(bucket.consumed_liened_backing_num < 1u128 << 96);
    kani::assume(source.spent_backing_num < 1u128 << 96);
    kani::assume(source.provider_receivable_num < 1u128 << 96);
    let _ = V16Core::prepare_counterparty_lien_consume_delta(bucket, source, amount);
}

#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::prepare_counterparty_lien_create_delta)]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn contract_check_prepare_counterparty_lien_create_delta() {
    let bucket = BackingBucketV16 {
        market_id: kani::any(),
        fresh_unliened_backing_num: kani::any(),
        valid_liened_backing_num: kani::any(),
        consumed_liened_backing_num: kani::any(),
        impaired_liened_backing_num: kani::any(),
        expiry_slot: kani::any(),
        status: BackingBucketStatusV16::Fresh,
        utilization_fee_earnings: kani::any(),
    };
    let source = SourceCreditStateV16 {
        positive_claim_bound_num: kani::any(),
        exact_positive_claim_num: kani::any(),
        fresh_reserved_backing_num: kani::any(),
        valid_liened_backing_num: kani::any(),
        impaired_liened_backing_num: kani::any(),
        spent_backing_num: kani::any(),
        provider_receivable_num: kani::any(),
        insurance_credit_reserved_num: kani::any(),
        valid_liened_insurance_num: kani::any(),
        impaired_liened_insurance_num: kani::any(),
        credit_rate_num: kani::any(),
        credit_epoch: kani::any(),
    };
    let current_slot: u64 = kani::any();
    let amount: u128 = kani::any();
    kani::assume(amount < 1u128 << 96);
    kani::assume(bucket.valid_liened_backing_num < 1u128 << 96);
    kani::assume(source.valid_liened_backing_num < 1u128 << 96);
    let _ = V16Core::prepare_counterparty_lien_create_delta(bucket, source, current_slot, amount);
}

#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::prepare_counterparty_backing_withdraw_delta)]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn contract_check_prepare_counterparty_backing_withdraw_delta() {
    let bucket = BackingBucketV16 {
        market_id: kani::any(),
        fresh_unliened_backing_num: kani::any(),
        valid_liened_backing_num: kani::any(),
        consumed_liened_backing_num: kani::any(),
        impaired_liened_backing_num: kani::any(),
        expiry_slot: kani::any(),
        status: BackingBucketStatusV16::Fresh,
        utilization_fee_earnings: kani::any(),
    };
    let source = SourceCreditStateV16 {
        positive_claim_bound_num: kani::any(),
        exact_positive_claim_num: kani::any(),
        fresh_reserved_backing_num: kani::any(),
        valid_liened_backing_num: kani::any(),
        impaired_liened_backing_num: kani::any(),
        spent_backing_num: kani::any(),
        provider_receivable_num: kani::any(),
        insurance_credit_reserved_num: kani::any(),
        valid_liened_insurance_num: kani::any(),
        impaired_liened_insurance_num: kani::any(),
        credit_rate_num: kani::any(),
        credit_epoch: kani::any(),
    };
    let amount: u128 = kani::any();
    let _ = V16Core::prepare_counterparty_backing_withdraw_delta(bucket, source, amount);
}

#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(apply_backing_provider_earnings_withdraw)]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn contract_check_apply_backing_provider_earnings_withdraw() {
    let vault: u128 = kani::any();
    let earnings: u128 = kani::any();
    let amount: u128 = kani::any();
    let _ = apply_backing_provider_earnings_withdraw(vault, earnings, amount);
}

#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::prepare_counterparty_lien_release_delta)]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn contract_check_prepare_counterparty_lien_release_delta() {
    let bucket = BackingBucketV16 {
        market_id: kani::any(),
        fresh_unliened_backing_num: kani::any(),
        valid_liened_backing_num: kani::any(),
        consumed_liened_backing_num: kani::any(),
        impaired_liened_backing_num: kani::any(),
        expiry_slot: kani::any(),
        status: BackingBucketStatusV16::Fresh,
        utilization_fee_earnings: kani::any(),
    };
    let source = SourceCreditStateV16 {
        positive_claim_bound_num: kani::any(),
        exact_positive_claim_num: kani::any(),
        fresh_reserved_backing_num: kani::any(),
        valid_liened_backing_num: kani::any(),
        impaired_liened_backing_num: kani::any(),
        spent_backing_num: kani::any(),
        provider_receivable_num: kani::any(),
        insurance_credit_reserved_num: kani::any(),
        valid_liened_insurance_num: kani::any(),
        impaired_liened_insurance_num: kani::any(),
        credit_rate_num: kani::any(),
        credit_epoch: kani::any(),
    };
    let current_slot: u64 = kani::any();
    let amount: u128 = kani::any();
    kani::assume(amount < 1u128 << 96);
    kani::assume(bucket.fresh_unliened_backing_num < 1u128 << 96);
    let _ = V16Core::prepare_counterparty_lien_release_delta(bucket, source, current_slot, amount);
}

#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::prepare_counterparty_lien_terminal_release_delta)]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn contract_check_prepare_counterparty_lien_terminal_release_delta() {
    let bucket = BackingBucketV16 {
        market_id: kani::any(),
        fresh_unliened_backing_num: kani::any(),
        valid_liened_backing_num: kani::any(),
        consumed_liened_backing_num: kani::any(),
        impaired_liened_backing_num: kani::any(),
        expiry_slot: kani::any(),
        status: BackingBucketStatusV16::Fresh,
        utilization_fee_earnings: kani::any(),
    };
    let source = SourceCreditStateV16 {
        positive_claim_bound_num: kani::any(),
        exact_positive_claim_num: kani::any(),
        fresh_reserved_backing_num: kani::any(),
        valid_liened_backing_num: kani::any(),
        impaired_liened_backing_num: kani::any(),
        spent_backing_num: kani::any(),
        provider_receivable_num: kani::any(),
        insurance_credit_reserved_num: kani::any(),
        valid_liened_insurance_num: kani::any(),
        impaired_liened_insurance_num: kani::any(),
        credit_rate_num: kani::any(),
        credit_epoch: kani::any(),
    };
    let amount: u128 = kani::any();
    kani::assume(amount < 1u128 << 96);
    kani::assume(bucket.fresh_unliened_backing_num < 1u128 << 96);
    let _ = V16Core::prepare_counterparty_lien_terminal_release_delta(bucket, source, amount);
}

#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::prepare_counterparty_lien_impair_delta)]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn contract_check_prepare_counterparty_lien_impair_delta() {
    let bucket = BackingBucketV16 {
        market_id: kani::any(),
        fresh_unliened_backing_num: kani::any(),
        valid_liened_backing_num: kani::any(),
        consumed_liened_backing_num: kani::any(),
        impaired_liened_backing_num: kani::any(),
        expiry_slot: kani::any(),
        status: BackingBucketStatusV16::Fresh,
        utilization_fee_earnings: kani::any(),
    };
    let source = SourceCreditStateV16 {
        positive_claim_bound_num: kani::any(),
        exact_positive_claim_num: kani::any(),
        fresh_reserved_backing_num: kani::any(),
        valid_liened_backing_num: kani::any(),
        impaired_liened_backing_num: kani::any(),
        spent_backing_num: kani::any(),
        provider_receivable_num: kani::any(),
        insurance_credit_reserved_num: kani::any(),
        valid_liened_insurance_num: kani::any(),
        impaired_liened_insurance_num: kani::any(),
        credit_rate_num: kani::any(),
        credit_epoch: kani::any(),
    };
    let amount: u128 = kani::any();
    kani::assume(amount < 1u128 << 96);
    kani::assume(bucket.impaired_liened_backing_num < 1u128 << 96);
    kani::assume(source.impaired_liened_backing_num < 1u128 << 96);
    let _ = V16Core::prepare_counterparty_lien_impair_delta(bucket, source, amount);
}

#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::prepare_counterparty_backing_add_delta)]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn contract_check_prepare_counterparty_backing_add_delta() {
    let bucket = BackingBucketV16 {
        market_id: kani::any(),
        fresh_unliened_backing_num: kani::any(),
        valid_liened_backing_num: kani::any(),
        consumed_liened_backing_num: kani::any(),
        impaired_liened_backing_num: kani::any(),
        expiry_slot: kani::any(),
        status: BackingBucketStatusV16::Fresh,
        utilization_fee_earnings: kani::any(),
    };
    let source = SourceCreditStateV16 {
        positive_claim_bound_num: kani::any(),
        exact_positive_claim_num: kani::any(),
        fresh_reserved_backing_num: kani::any(),
        valid_liened_backing_num: kani::any(),
        impaired_liened_backing_num: kani::any(),
        spent_backing_num: kani::any(),
        provider_receivable_num: kani::any(),
        insurance_credit_reserved_num: kani::any(),
        valid_liened_insurance_num: kani::any(),
        impaired_liened_insurance_num: kani::any(),
        credit_rate_num: kani::any(),
        credit_epoch: kani::any(),
    };
    let amount: u128 = kani::any();
    let current_slot: u64 = kani::any();
    let expiry_slot: u64 = kani::any();
    kani::assume(amount < 1u128 << 96);
    kani::assume(bucket.fresh_unliened_backing_num < 1u128 << 96);
    kani::assume(source.fresh_reserved_backing_num < 1u128 << 96);
    let _ = V16Core::prepare_counterparty_backing_add_delta(
        bucket,
        source,
        amount,
        current_slot,
        expiry_slot,
    );
}

#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::prepare_insurance_lien_create_delta)]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn contract_check_prepare_insurance_lien_create_delta() {
    let reservation = InsuranceCreditReservationV16 {
        insurance_credit_reserved_num: kani::any(),
        valid_liened_insurance_num: kani::any(),
        impaired_liened_insurance_num: kani::any(),
        consumed_insurance_num: kani::any(),
        source_credit_epoch: kani::any(),
    };
    let source = SourceCreditStateV16 {
        positive_claim_bound_num: kani::any(),
        exact_positive_claim_num: kani::any(),
        fresh_reserved_backing_num: kani::any(),
        valid_liened_backing_num: kani::any(),
        impaired_liened_backing_num: kani::any(),
        spent_backing_num: kani::any(),
        provider_receivable_num: kani::any(),
        insurance_credit_reserved_num: kani::any(),
        valid_liened_insurance_num: kani::any(),
        impaired_liened_insurance_num: kani::any(),
        credit_rate_num: kani::any(),
        credit_epoch: kani::any(),
    };
    let amount: u128 = kani::any();
    kani::assume(amount < 1u128 << 96);
    kani::assume(reservation.valid_liened_insurance_num < 1u128 << 96);
    kani::assume(source.valid_liened_insurance_num < 1u128 << 96);
    let _ = V16Core::prepare_insurance_lien_create_delta(reservation, source, amount);
}

#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::prepare_insurance_lien_release_delta)]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn contract_check_prepare_insurance_lien_release_delta() {
    let reservation = InsuranceCreditReservationV16 {
        insurance_credit_reserved_num: kani::any(),
        valid_liened_insurance_num: kani::any(),
        impaired_liened_insurance_num: kani::any(),
        consumed_insurance_num: kani::any(),
        source_credit_epoch: kani::any(),
    };
    let source = SourceCreditStateV16 {
        positive_claim_bound_num: kani::any(),
        exact_positive_claim_num: kani::any(),
        fresh_reserved_backing_num: kani::any(),
        valid_liened_backing_num: kani::any(),
        impaired_liened_backing_num: kani::any(),
        spent_backing_num: kani::any(),
        provider_receivable_num: kani::any(),
        insurance_credit_reserved_num: kani::any(),
        valid_liened_insurance_num: kani::any(),
        impaired_liened_insurance_num: kani::any(),
        credit_rate_num: kani::any(),
        credit_epoch: kani::any(),
    };
    let amount: u128 = kani::any();
    kani::assume(amount < 1u128 << 96);
    let _ = V16Core::prepare_insurance_lien_release_delta(reservation, source, amount);
}

#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::prepare_insurance_lien_impair_delta)]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn contract_check_prepare_insurance_lien_impair_delta() {
    let reservation = InsuranceCreditReservationV16 {
        insurance_credit_reserved_num: kani::any(),
        valid_liened_insurance_num: kani::any(),
        impaired_liened_insurance_num: kani::any(),
        consumed_insurance_num: kani::any(),
        source_credit_epoch: kani::any(),
    };
    let source = SourceCreditStateV16 {
        positive_claim_bound_num: kani::any(),
        exact_positive_claim_num: kani::any(),
        fresh_reserved_backing_num: kani::any(),
        valid_liened_backing_num: kani::any(),
        impaired_liened_backing_num: kani::any(),
        spent_backing_num: kani::any(),
        provider_receivable_num: kani::any(),
        insurance_credit_reserved_num: kani::any(),
        valid_liened_insurance_num: kani::any(),
        impaired_liened_insurance_num: kani::any(),
        credit_rate_num: kani::any(),
        credit_epoch: kani::any(),
    };
    let amount: u128 = kani::any();
    kani::assume(amount < 1u128 << 96);
    kani::assume(reservation.impaired_liened_insurance_num < 1u128 << 96);
    kani::assume(source.impaired_liened_insurance_num < 1u128 << 96);
    let _ = V16Core::prepare_insurance_lien_impair_delta(reservation, source, amount);
}

#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::prepare_insurance_lien_terminal_release_delta)]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn contract_check_prepare_insurance_lien_terminal_release_delta() {
    let reservation = InsuranceCreditReservationV16 {
        insurance_credit_reserved_num: kani::any(),
        valid_liened_insurance_num: kani::any(),
        impaired_liened_insurance_num: kani::any(),
        consumed_insurance_num: kani::any(),
        source_credit_epoch: kani::any(),
    };
    let source = SourceCreditStateV16 {
        positive_claim_bound_num: kani::any(),
        exact_positive_claim_num: kani::any(),
        fresh_reserved_backing_num: kani::any(),
        valid_liened_backing_num: kani::any(),
        impaired_liened_backing_num: kani::any(),
        spent_backing_num: kani::any(),
        provider_receivable_num: kani::any(),
        insurance_credit_reserved_num: kani::any(),
        valid_liened_insurance_num: kani::any(),
        impaired_liened_insurance_num: kani::any(),
        credit_rate_num: kani::any(),
        credit_epoch: kani::any(),
    };
    let amount: u128 = kani::any();
    kani::assume(amount < 1u128 << 96);
    let _ = V16Core::prepare_insurance_lien_terminal_release_delta(reservation, source, amount);
}

#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(MarketGroupV16ViewMut::credit_account_from_insurance_delta)]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn contract_check_credit_account_from_insurance_delta() {
    let insurance: u128 = kani::any();
    let budget_remaining: u128 = kani::any();
    let c_tot: u128 = kani::any();
    let capital: u128 = kani::any();
    let amount: u128 = kani::any();
    let _ = MarketGroupV16ViewMut::<Market<u64>>::credit_account_from_insurance_delta(
        insurance,
        budget_remaining,
        c_tot,
        capital,
        amount,
    );
}

#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::prepare_source_positive_claim_bound_delta)]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn contract_check_prepare_source_positive_claim_bound_delta() {
    let source = SourceCreditStateV16 {
        positive_claim_bound_num: kani::any(),
        exact_positive_claim_num: kani::any(),
        fresh_reserved_backing_num: kani::any(),
        valid_liened_backing_num: kani::any(),
        impaired_liened_backing_num: kani::any(),
        spent_backing_num: kani::any(),
        provider_receivable_num: kani::any(),
        insurance_credit_reserved_num: kani::any(),
        valid_liened_insurance_num: kani::any(),
        impaired_liened_insurance_num: kani::any(),
        credit_rate_num: kani::any(),
        credit_epoch: kani::any(),
    };
    let claim_bound_num: u128 = kani::any();
    let exact_claim_num: u128 = kani::any();
    kani::assume(exact_claim_num <= claim_bound_num);
    let _ = V16Core::prepare_source_positive_claim_bound_delta(
        source,
        claim_bound_num,
        exact_claim_num,
    );
}

#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(MarketGroupV16ViewMut::apply_total_delta)]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn contract_check_apply_total_delta() {
    let total: u128 = kani::any();
    let old: u128 = kani::any();
    let new: u128 = kani::any();
    let _ = MarketGroupV16ViewMut::<Market<u64>>::apply_total_delta(total, old, new);
}

#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(MarketGroupV16ViewMut::trade_signed_size_deltas)]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn contract_check_trade_signed_size_deltas() {
    let size_q: i128 = kani::any();
    let _ = MarketGroupV16ViewMut::<Market<u64>>::trade_signed_size_deltas(size_q);
}

// Flow-typing witness (plain proof: proof_for_contract cannot handle the
// array-bearing return type — unbounded write-set havoc loop; same
// postconditions asserted directly over the full input domain).
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
fn contract_check_flow_external_in_to_account_capital() {
    let amount: u128 = kani::any();
    let vault_before: u128 = kani::any();
    let vault_after = vault_before.wrapping_add(amount);
    kani::assume(vault_after >= vault_before);
    if let Ok(p) =
        TokenValueFlowProofV16::external_in_to_account_capital(amount, vault_before, vault_after)
    {
        let mut ed = [0u128; V16_TOKEN_VALUE_CLASS_COUNT];
        let mut ec = [0u128; V16_TOKEN_VALUE_CLASS_COUNT];
        ed[TokenValueClassV16::AccountCapital as usize] = amount;
        ec[TokenValueClassV16::ExternalQuote as usize] = amount;
        let mut i = 0;
        while i < V16_TOKEN_VALUE_CLASS_COUNT {
            assert!(p.debits[i] == ed[i]);
            assert!(p.credits[i] == ec[i]);
            i += 1;
        }
        assert_eq!(p.external_quote_in, amount);
        assert_eq!(p.external_quote_out, 0);
        assert_eq!(p.vault_before, vault_before);
        assert_eq!(p.vault_after, vault_after);
        assert_eq!(p.validate(), Ok(()));
    }
}

// Flow-typing witness (plain proof: proof_for_contract cannot handle the
// array-bearing return type — unbounded write-set havoc loop; same
// postconditions asserted directly over the full input domain).
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
fn contract_check_flow_account_capital_to_external_out() {
    let amount: u128 = kani::any();
    let vault_before: u128 = kani::any();
    kani::assume(vault_before >= amount);
    let vault_after = vault_before - amount;
    if let Ok(p) =
        TokenValueFlowProofV16::account_capital_to_external_out(amount, vault_before, vault_after)
    {
        let mut ed = [0u128; V16_TOKEN_VALUE_CLASS_COUNT];
        let mut ec = [0u128; V16_TOKEN_VALUE_CLASS_COUNT];
        ed[TokenValueClassV16::AccountCapital as usize] = amount;
        ec[TokenValueClassV16::ExternalQuote as usize] = amount;
        let mut i = 0;
        while i < V16_TOKEN_VALUE_CLASS_COUNT {
            assert!(p.debits[i] == ed[i]);
            assert!(p.credits[i] == ec[i]);
            i += 1;
        }
        assert_eq!(p.external_quote_in, 0);
        assert_eq!(p.external_quote_out, amount);
        assert_eq!(p.vault_before, vault_before);
        assert_eq!(p.vault_after, vault_after);
        assert_eq!(p.validate(), Ok(()));
    }
}

// Flow-typing witness (plain proof: proof_for_contract cannot handle the
// array-bearing return type — unbounded write-set havoc loop; same
// postconditions asserted directly over the full input domain).
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
fn contract_check_flow_account_capital_to_insurance() {
    let amount: u128 = kani::any();
    let vault_before: u128 = kani::any();
    let vault_after = vault_before; // internal relabel: vault flat
    if let Ok(p) =
        TokenValueFlowProofV16::account_capital_to_insurance(amount, vault_before, vault_after)
    {
        let mut ed = [0u128; V16_TOKEN_VALUE_CLASS_COUNT];
        let mut ec = [0u128; V16_TOKEN_VALUE_CLASS_COUNT];
        ed[TokenValueClassV16::AccountCapital as usize] = amount;
        ec[TokenValueClassV16::InsuranceCapital as usize] = amount;
        let mut i = 0;
        while i < V16_TOKEN_VALUE_CLASS_COUNT {
            assert!(p.debits[i] == ed[i]);
            assert!(p.credits[i] == ec[i]);
            i += 1;
        }
        assert_eq!(p.external_quote_in, 0);
        assert_eq!(p.external_quote_out, 0);
        assert_eq!(p.vault_before, vault_before);
        assert_eq!(p.vault_after, vault_after);
        assert_eq!(p.validate(), Ok(()));
    }
}

// Flow-typing witness (plain proof: proof_for_contract cannot handle the
// array-bearing return type — unbounded write-set havoc loop; same
// postconditions asserted directly over the full input domain).
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
fn contract_check_flow_external_in_to_insurance_capital() {
    let amount: u128 = kani::any();
    let vault_before: u128 = kani::any();
    let vault_after = vault_before.wrapping_add(amount);
    kani::assume(vault_after >= vault_before);
    if let Ok(p) =
        TokenValueFlowProofV16::external_in_to_insurance_capital(amount, vault_before, vault_after)
    {
        let mut ed = [0u128; V16_TOKEN_VALUE_CLASS_COUNT];
        let mut ec = [0u128; V16_TOKEN_VALUE_CLASS_COUNT];
        ed[TokenValueClassV16::InsuranceCapital as usize] = amount;
        ec[TokenValueClassV16::ExternalQuote as usize] = amount;
        let mut i = 0;
        while i < V16_TOKEN_VALUE_CLASS_COUNT {
            assert!(p.debits[i] == ed[i]);
            assert!(p.credits[i] == ec[i]);
            i += 1;
        }
        assert_eq!(p.external_quote_in, amount);
        assert_eq!(p.external_quote_out, 0);
        assert_eq!(p.vault_before, vault_before);
        assert_eq!(p.vault_after, vault_after);
        assert_eq!(p.validate(), Ok(()));
    }
}

// Flow-typing witness (plain proof: proof_for_contract cannot handle the
// array-bearing return type — unbounded write-set havoc loop; same
// postconditions asserted directly over the full input domain).
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
fn contract_check_flow_insurance_capital_to_external_out() {
    let amount: u128 = kani::any();
    let vault_before: u128 = kani::any();
    kani::assume(vault_before >= amount);
    let vault_after = vault_before - amount;
    if let Ok(p) =
        TokenValueFlowProofV16::insurance_capital_to_external_out(amount, vault_before, vault_after)
    {
        let mut ed = [0u128; V16_TOKEN_VALUE_CLASS_COUNT];
        let mut ec = [0u128; V16_TOKEN_VALUE_CLASS_COUNT];
        ed[TokenValueClassV16::InsuranceCapital as usize] = amount;
        ec[TokenValueClassV16::ExternalQuote as usize] = amount;
        let mut i = 0;
        while i < V16_TOKEN_VALUE_CLASS_COUNT {
            assert!(p.debits[i] == ed[i]);
            assert!(p.credits[i] == ec[i]);
            i += 1;
        }
        assert_eq!(p.external_quote_in, 0);
        assert_eq!(p.external_quote_out, amount);
        assert_eq!(p.vault_before, vault_before);
        assert_eq!(p.vault_after, vault_after);
        assert_eq!(p.validate(), Ok(()));
    }
}

// Flow-typing witness (plain proof: proof_for_contract cannot handle the
// array-bearing return type — unbounded write-set havoc loop; same
// postconditions asserted directly over the full input domain).
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
fn contract_check_flow_insurance_capital_to_account_capital() {
    let amount: u128 = kani::any();
    let vault_before: u128 = kani::any();
    let vault_after = vault_before; // internal relabel: vault flat
    if let Ok(p) = TokenValueFlowProofV16::insurance_capital_to_account_capital(
        amount,
        vault_before,
        vault_after,
    ) {
        let mut ed = [0u128; V16_TOKEN_VALUE_CLASS_COUNT];
        let mut ec = [0u128; V16_TOKEN_VALUE_CLASS_COUNT];
        ed[TokenValueClassV16::InsuranceCapital as usize] = amount;
        ec[TokenValueClassV16::AccountCapital as usize] = amount;
        let mut i = 0;
        while i < V16_TOKEN_VALUE_CLASS_COUNT {
            assert!(p.debits[i] == ed[i]);
            assert!(p.credits[i] == ec[i]);
            i += 1;
        }
        assert_eq!(p.external_quote_in, 0);
        assert_eq!(p.external_quote_out, 0);
        assert_eq!(p.vault_before, vault_before);
        assert_eq!(p.vault_after, vault_after);
        assert_eq!(p.validate(), Ok(()));
    }
}

// Flow-typing witness (plain proof: proof_for_contract cannot handle the
// array-bearing return type — unbounded write-set havoc loop; same
// postconditions asserted directly over the full input domain).
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
fn contract_check_flow_account_capital_to_realized_loss() {
    let amount: u128 = kani::any();
    let vault_before: u128 = kani::any();
    let vault_after = vault_before; // internal relabel: vault flat
    if let Ok(p) =
        TokenValueFlowProofV16::account_capital_to_realized_loss(amount, vault_before, vault_after)
    {
        let mut ed = [0u128; V16_TOKEN_VALUE_CLASS_COUNT];
        let mut ec = [0u128; V16_TOKEN_VALUE_CLASS_COUNT];
        ed[TokenValueClassV16::AccountCapital as usize] = amount;
        ec[TokenValueClassV16::ExplicitBackedLoss as usize] = amount;
        let mut i = 0;
        while i < V16_TOKEN_VALUE_CLASS_COUNT {
            assert!(p.debits[i] == ed[i]);
            assert!(p.credits[i] == ec[i]);
            i += 1;
        }
        assert_eq!(p.external_quote_in, 0);
        assert_eq!(p.external_quote_out, 0);
        assert_eq!(p.vault_before, vault_before);
        assert_eq!(p.vault_after, vault_after);
        assert_eq!(p.validate(), Ok(()));
    }
}

// Flow-typing witness (plain proof: proof_for_contract cannot handle the
// array-bearing return type — unbounded write-set havoc loop; same
// postconditions asserted directly over the full input domain).
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
fn contract_check_flow_insurance_to_close_insurance_spent() {
    let amount: u128 = kani::any();
    let vault_before: u128 = kani::any();
    let vault_after = vault_before; // internal relabel: vault flat
    if let Ok(p) = TokenValueFlowProofV16::insurance_to_close_insurance_spent(
        amount,
        vault_before,
        vault_after,
    ) {
        let mut ed = [0u128; V16_TOKEN_VALUE_CLASS_COUNT];
        let mut ec = [0u128; V16_TOKEN_VALUE_CLASS_COUNT];
        ed[TokenValueClassV16::InsuranceCapital as usize] = amount;
        ec[TokenValueClassV16::CloseInsuranceSpent as usize] = amount;
        let mut i = 0;
        while i < V16_TOKEN_VALUE_CLASS_COUNT {
            assert!(p.debits[i] == ed[i]);
            assert!(p.credits[i] == ec[i]);
            i += 1;
        }
        assert_eq!(p.external_quote_in, 0);
        assert_eq!(p.external_quote_out, 0);
        assert_eq!(p.vault_before, vault_before);
        assert_eq!(p.vault_after, vault_after);
        assert_eq!(p.validate(), Ok(()));
    }
}

// The multi-leg flow transits are the VALUE SKELETONS of the Kani-intractable
// public bodies: close_cure_to_account_capital is the cure path's only value
// move, support_to_account_capital the resolved-close support conversion's,
// capital_and_resolved_payout_to_external_out the resolved withdrawal's. The
// engine constructs and validate()s one of these on every execution of those
// bodies, so full-domain witnesses here + the per-leaf delta contracts close
// the conservation argument for the intractable tier.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
fn contract_check_flow_close_cure_to_account_capital() {
    let deposit: u128 = kani::any();
    let escrow: u128 = kani::any();
    let vault_before: u128 = kani::any();
    let capital_credit = match deposit.checked_add(escrow) {
        Some(v) => v,
        None => return,
    };
    let vault_after = vault_before.wrapping_add(deposit);
    kani::assume(vault_after >= vault_before);
    if let Ok(p) = TokenValueFlowProofV16::close_cure_to_account_capital(
        deposit,
        escrow,
        capital_credit,
        vault_before,
        vault_after,
    ) {
        let mut ed = [0u128; V16_TOKEN_VALUE_CLASS_COUNT];
        let mut ec = [0u128; V16_TOKEN_VALUE_CLASS_COUNT];
        ec[TokenValueClassV16::ExternalQuote as usize] = deposit;
        ec[TokenValueClassV16::CancelDepositEscrow as usize] = escrow;
        ed[TokenValueClassV16::AccountCapital as usize] = capital_credit;
        let mut i = 0;
        while i < V16_TOKEN_VALUE_CLASS_COUNT {
            assert!(p.debits[i] == ed[i]);
            assert!(p.credits[i] == ec[i]);
            i += 1;
        }
        assert_eq!(p.external_quote_in, deposit);
        assert_eq!(p.external_quote_out, 0);
        // The cure credits the account with exactly deposit + escrow and the
        // vault rises by exactly the external deposit: no value minted.
        assert_eq!(p.validate(), Ok(()));
    }
}

#[cfg(all(kani, feature = "contracts"))]
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
fn contract_check_flow_support_to_account_capital() {
    let cp: u128 = kani::any();
    let ins: u128 = kani::any();
    let surplus: u128 = kani::any();
    let vault_before: u128 = kani::any();
    let credit = match cp.checked_add(ins).and_then(|v| v.checked_add(surplus)) {
        Some(v) => v,
        None => return,
    };
    if let Ok(p) = TokenValueFlowProofV16::support_to_account_capital(
        credit,
        cp,
        ins,
        surplus,
        vault_before,
        vault_before,
    ) {
        let mut ed = [0u128; V16_TOKEN_VALUE_CLASS_COUNT];
        let mut ec = [0u128; V16_TOKEN_VALUE_CLASS_COUNT];
        ec[TokenValueClassV16::CloseCounterpartyCreditConsumed as usize] = cp;
        ec[TokenValueClassV16::CloseInsuranceSpent as usize] = ins;
        ec[TokenValueClassV16::UnallocatedProtocolSurplus as usize] = surplus;
        ed[TokenValueClassV16::AccountCapital as usize] = credit;
        let mut i = 0;
        while i < V16_TOKEN_VALUE_CLASS_COUNT {
            assert!(p.debits[i] == ed[i]);
            assert!(p.credits[i] == ec[i]);
            i += 1;
        }
        assert_eq!(p.external_quote_in, 0);
        assert_eq!(p.external_quote_out, 0);
        // Support conversion is internally funded (vault flat): the winner's
        // capital credit is exactly the sum of the three consumed sources.
        assert_eq!(p.validate(), Ok(()));
    }
}

#[cfg(all(kani, feature = "contracts"))]
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
fn contract_check_flow_capital_and_resolved_payout_to_external_out() {
    let capital_paid: u128 = kani::any();
    let payout_paid: u128 = kani::any();
    let vault_before: u128 = kani::any();
    let total = match capital_paid.checked_add(payout_paid) {
        Some(v) => v,
        None => return,
    };
    kani::assume(vault_before >= total);
    let vault_after = vault_before - total;
    if let Ok(p) = TokenValueFlowProofV16::capital_and_resolved_payout_to_external_out(
        capital_paid,
        payout_paid,
        total,
        vault_before,
        vault_after,
    ) {
        let mut ed = [0u128; V16_TOKEN_VALUE_CLASS_COUNT];
        let mut ec = [0u128; V16_TOKEN_VALUE_CLASS_COUNT];
        ed[TokenValueClassV16::AccountCapital as usize] = capital_paid;
        ed[TokenValueClassV16::ResolvedPayoutPaid as usize] = payout_paid;
        ec[TokenValueClassV16::ExternalQuote as usize] = total;
        let mut i = 0;
        while i < V16_TOKEN_VALUE_CLASS_COUNT {
            assert!(p.debits[i] == ed[i]);
            assert!(p.credits[i] == ec[i]);
            i += 1;
        }
        assert_eq!(p.external_quote_in, 0);
        assert_eq!(p.external_quote_out, total);
        // A resolved exit pays out exactly capital + receipt claim and the
        // vault falls by exactly that total: nothing else can leave.
        assert_eq!(p.validate(), Ok(()));
    }
}

#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(MarketGroupV16ViewMut::withdraw_domain_insurance_delta)]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn contract_check_withdraw_domain_insurance_delta() {
    let _ = MarketGroupV16ViewMut::<Market<u64>>::withdraw_domain_insurance_delta(
        kani::any(),
        kani::any(),
        kani::any(),
        kani::any(),
        kani::any(),
        kani::any(),
        kani::any(),
    );
}

#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(MarketGroupV16ViewMut::credit_backing_provider_earnings_delta)]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn contract_check_credit_backing_provider_earnings_delta() {
    let _ = MarketGroupV16ViewMut::<Market<u64>>::credit_backing_provider_earnings_delta(
        kani::any(),
        kani::any(),
        kani::any(),
        kani::any(),
        kani::any(),
        kani::any(),
    );
}

#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(MarketGroupV16ViewMut::set_domain_insurance_spent_delta)]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn contract_check_set_domain_insurance_spent_delta() {
    let total_remaining: u128 = kani::any();
    let insurance: u128 = kani::any();
    let budget: u128 = kani::any();
    let old_spent: u128 = kani::any();
    let new_spent: u128 = kani::any();
    kani::assume(old_spent <= budget && new_spent <= budget);
    let _ = MarketGroupV16ViewMut::<Market<u64>>::set_domain_insurance_spent_delta(
        total_remaining,
        insurance,
        budget,
        old_spent,
        new_spent,
    );
}

#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(MarketGroupV16ViewMut::set_domain_insurance_budget_delta)]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn contract_check_set_domain_insurance_budget_delta() {
    let total_remaining: u128 = kani::any();
    let insurance_limit: u128 = kani::any();
    let old_budget: u128 = kani::any();
    let spent: u128 = kani::any();
    let new_budget: u128 = kani::any();
    kani::assume(spent <= old_budget && spent <= new_budget);
    let _ = MarketGroupV16ViewMut::<Market<u64>>::set_domain_insurance_budget_delta(
        total_remaining,
        insurance_limit,
        old_budget,
        spent,
        new_budget,
    );
}

#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::available_backing_num_for_source_credit_state)]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn contract_check_available_backing_num_for_source_credit_state() {
    let state = SourceCreditStateV16 {
        positive_claim_bound_num: kani::any(),
        exact_positive_claim_num: kani::any(),
        fresh_reserved_backing_num: kani::any(),
        valid_liened_backing_num: kani::any(),
        impaired_liened_backing_num: kani::any(),
        spent_backing_num: kani::any(),
        provider_receivable_num: kani::any(),
        insurance_credit_reserved_num: kani::any(),
        valid_liened_insurance_num: kani::any(),
        impaired_liened_insurance_num: kani::any(),
        credit_rate_num: kani::any(),
        credit_epoch: kani::any(),
    };
    let _ = V16Core::available_backing_num_for_source_credit_state(state);
}

#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::health_requirements_from_base_and_target_lag)]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn contract_check_health_requirements_from_base_and_target_lag() {
    let _ = V16Core::health_requirements_from_base_and_target_lag(
        kani::any(),
        kani::any(),
        kani::any(),
        kani::any(),
    );
}

// ============ P0: ENCUMBRANCE-CLOSURE INDUCTION ============
// The per-domain ledger invariant (the div-free cross-ledger equalities of
// validate_source_domain_ledger_parts; the credit-rate equality is excluded
// here because it is intentionally broken by deltas and restored by the
// recompute step, which the suite proves separately). Each closure harness
// proves: for EVERY state satisfying inv (not just constructed ones), the
// delta preserves inv. With the genesis proof, any sequence of deltas
// preserves ledger validity — induction, not reachability-by-construction.
#[cfg(all(kani, feature = "closure"))]
fn kani_ledger_inv(
    b: &BackingBucketV16,
    s: &SourceCreditStateV16,
    r: &InsuranceCreditReservationV16,
) -> bool {
    s.fresh_reserved_backing_num == b.fresh_unliened_backing_num + b.valid_liened_backing_num
        && s.provider_receivable_num == b.consumed_liened_backing_num
        && s.valid_liened_backing_num == b.valid_liened_backing_num
        && s.impaired_liened_backing_num == b.impaired_liened_backing_num
        && s.insurance_credit_reserved_num == r.insurance_credit_reserved_num
        && s.valid_liened_insurance_num == r.valid_liened_insurance_num
        && s.impaired_liened_insurance_num == r.impaired_liened_insurance_num
        && s.spent_backing_num >= s.provider_receivable_num
}

#[cfg(all(kani, feature = "closure"))]
fn kani_any_ledger_triple() -> (
    BackingBucketV16,
    SourceCreditStateV16,
    InsuranceCreditReservationV16,
) {
    let b = BackingBucketV16 {
        market_id: kani::any(),
        fresh_unliened_backing_num: kani::any(),
        valid_liened_backing_num: kani::any(),
        consumed_liened_backing_num: kani::any(),
        impaired_liened_backing_num: kani::any(),
        expiry_slot: kani::any(),
        status: kani::any(),
        utilization_fee_earnings: kani::any(),
    };
    let s = SourceCreditStateV16 {
        positive_claim_bound_num: kani::any(),
        exact_positive_claim_num: kani::any(),
        fresh_reserved_backing_num: kani::any(),
        valid_liened_backing_num: kani::any(),
        impaired_liened_backing_num: kani::any(),
        spent_backing_num: kani::any(),
        provider_receivable_num: kani::any(),
        insurance_credit_reserved_num: kani::any(),
        valid_liened_insurance_num: kani::any(),
        impaired_liened_insurance_num: kani::any(),
        credit_rate_num: kani::any(),
        credit_epoch: kani::any(),
    };
    let r = InsuranceCreditReservationV16 {
        insurance_credit_reserved_num: kani::any(),
        valid_liened_insurance_num: kani::any(),
        impaired_liened_insurance_num: kani::any(),
        consumed_insurance_num: kani::any(),
        source_credit_epoch: kani::any(),
    };
    // Overflow headroom so inv's additions and the deltas' checked ops stay
    // in-range; production magnitudes are < 2^93 (MAX_VAULT_TVL * BOUND_SCALE).
    kani::assume(b.fresh_unliened_backing_num < 1u128 << 96);
    kani::assume(b.valid_liened_backing_num < 1u128 << 96);
    kani::assume(b.consumed_liened_backing_num < 1u128 << 96);
    kani::assume(b.impaired_liened_backing_num < 1u128 << 96);
    kani::assume(s.spent_backing_num < 1u128 << 96);
    kani::assume(s.positive_claim_bound_num < 1u128 << 96);
    kani::assume(s.exact_positive_claim_num < 1u128 << 96);
    kani::assume(r.insurance_credit_reserved_num < 1u128 << 96);
    kani::assume(r.valid_liened_insurance_num < 1u128 << 96);
    kani::assume(r.impaired_liened_insurance_num < 1u128 << 96);
    kani::assume(r.consumed_insurance_num < 1u128 << 96);
    (b, s, r)
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
fn closure_ledger_inv_genesis() {
    let b = BackingBucketV16::EMPTY;
    let s = SourceCreditStateV16::EMPTY;
    let r = InsuranceCreditReservationV16::EMPTY;
    assert!(kani_ledger_inv(&b, &s, &r));
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn closure_ledger_inv_prepare_counterparty_lien_create_delta() {
    let (b, s, r) = kani_any_ledger_triple();
    let amount: u128 = kani::any();
    kani::assume(amount < 1u128 << 96);
    kani::assume(kani_ledger_inv(&b, &s, &r));
    if let Ok((b2, s2)) = V16Core::prepare_counterparty_lien_create_delta(b, s, kani::any(), amount)
    {
        // Reservation untouched by counterparty deltas.
        assert!(kani_ledger_inv(&b2, &s2, &r));
    }
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn closure_ledger_inv_prepare_counterparty_lien_release_delta() {
    let (b, s, r) = kani_any_ledger_triple();
    let amount: u128 = kani::any();
    kani::assume(amount < 1u128 << 96);
    kani::assume(kani_ledger_inv(&b, &s, &r));
    if let Ok((b2, s2)) =
        V16Core::prepare_counterparty_lien_release_delta(b, s, kani::any(), amount)
    {
        // Reservation untouched by counterparty deltas.
        assert!(kani_ledger_inv(&b2, &s2, &r));
    }
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn closure_ledger_inv_prepare_counterparty_lien_terminal_release_delta() {
    let (b, s, r) = kani_any_ledger_triple();
    let amount: u128 = kani::any();
    kani::assume(amount < 1u128 << 96);
    kani::assume(kani_ledger_inv(&b, &s, &r));
    if let Ok((b2, s2)) = V16Core::prepare_counterparty_lien_terminal_release_delta(b, s, amount) {
        // Reservation untouched by counterparty deltas.
        assert!(kani_ledger_inv(&b2, &s2, &r));
    }
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn closure_ledger_inv_prepare_counterparty_lien_consume_delta() {
    let (b, s, r) = kani_any_ledger_triple();
    let amount: u128 = kani::any();
    kani::assume(amount < 1u128 << 96);
    kani::assume(kani_ledger_inv(&b, &s, &r));
    if let Ok((b2, s2)) = V16Core::prepare_counterparty_lien_consume_delta(b, s, amount) {
        // Reservation untouched by counterparty deltas.
        assert!(kani_ledger_inv(&b2, &s2, &r));
    }
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn closure_ledger_inv_prepare_counterparty_lien_impair_delta() {
    let (b, s, r) = kani_any_ledger_triple();
    let amount: u128 = kani::any();
    kani::assume(amount < 1u128 << 96);
    kani::assume(kani_ledger_inv(&b, &s, &r));
    if let Ok((b2, s2)) = V16Core::prepare_counterparty_lien_impair_delta(b, s, amount) {
        // Reservation untouched by counterparty deltas.
        assert!(kani_ledger_inv(&b2, &s2, &r));
    }
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn closure_ledger_inv_prepare_counterparty_backing_withdraw_delta() {
    let (b, s, r) = kani_any_ledger_triple();
    let amount: u128 = kani::any();
    kani::assume(amount < 1u128 << 96);
    kani::assume(kani_ledger_inv(&b, &s, &r));
    if let Ok((b2, s2)) = V16Core::prepare_counterparty_backing_withdraw_delta(b, s, amount) {
        // Reservation untouched by counterparty deltas.
        assert!(kani_ledger_inv(&b2, &s2, &r));
    }
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn closure_ledger_inv_prepare_counterparty_backing_add_delta() {
    let (b, s, r) = kani_any_ledger_triple();
    let amount: u128 = kani::any();
    kani::assume(amount < 1u128 << 96);
    kani::assume(kani_ledger_inv(&b, &s, &r));
    if let Ok((b2, s2)) =
        V16Core::prepare_counterparty_backing_add_delta(b, s, amount, kani::any(), kani::any())
    {
        // Reservation untouched by counterparty deltas.
        assert!(kani_ledger_inv(&b2, &s2, &r));
    }
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn closure_ledger_inv_prepare_insurance_lien_create_delta() {
    let (b, s, r) = kani_any_ledger_triple();
    let amount: u128 = kani::any();
    kani::assume(amount < 1u128 << 96);
    kani::assume(kani_ledger_inv(&b, &s, &r));
    if let Ok((r2, s2)) = V16Core::prepare_insurance_lien_create_delta(r, s, amount) {
        // Bucket untouched by insurance deltas.
        assert!(kani_ledger_inv(&b, &s2, &r2));
    }
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn closure_ledger_inv_prepare_insurance_lien_release_delta() {
    let (b, s, r) = kani_any_ledger_triple();
    let amount: u128 = kani::any();
    kani::assume(amount < 1u128 << 96);
    kani::assume(kani_ledger_inv(&b, &s, &r));
    if let Ok((r2, s2)) = V16Core::prepare_insurance_lien_release_delta(r, s, amount) {
        // Bucket untouched by insurance deltas.
        assert!(kani_ledger_inv(&b, &s2, &r2));
    }
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn closure_ledger_inv_prepare_insurance_lien_terminal_release_delta() {
    let (b, s, r) = kani_any_ledger_triple();
    let amount: u128 = kani::any();
    kani::assume(amount < 1u128 << 96);
    kani::assume(kani_ledger_inv(&b, &s, &r));
    if let Ok((r2, s2)) = V16Core::prepare_insurance_lien_terminal_release_delta(r, s, amount) {
        // Bucket untouched by insurance deltas.
        assert!(kani_ledger_inv(&b, &s2, &r2));
    }
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn closure_ledger_inv_prepare_insurance_lien_impair_delta() {
    let (b, s, r) = kani_any_ledger_triple();
    let amount: u128 = kani::any();
    kani::assume(amount < 1u128 << 96);
    kani::assume(kani_ledger_inv(&b, &s, &r));
    if let Ok((r2, s2)) = V16Core::prepare_insurance_lien_impair_delta(r, s, amount) {
        // Bucket untouched by insurance deltas.
        assert!(kani_ledger_inv(&b, &s2, &r2));
    }
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn closure_ledger_inv_prepare_insurance_lien_consume_delta() {
    let (b, s, r) = kani_any_ledger_triple();
    let amount: u128 = kani::any();
    let domain_spent: u128 = kani::any();
    let insurance: u128 = kani::any();
    kani::assume(amount < 1u128 << 96);
    kani::assume(domain_spent < 1u128 << 96);
    kani::assume(kani_ledger_inv(&b, &s, &r));
    if let Ok((r2, s2, _ds, _ins)) =
        V16Core::prepare_insurance_lien_consume_delta(r, s, domain_spent, insurance, amount)
    {
        assert!(kani_ledger_inv(&b, &s2, &r2));
    }
}

// ============ P4: BUCKET STATUS-MACHINE CLOSURE ============
// validate_backing_bucket_static encodes the spec's bucket lifecycle diagram
// as per-status amount-shape rules (Empty must be value-free, Fresh must be
// funded and unexpired-shaped, Expired/Impaired must hold only their
// respective residue classes). Closure: ANY bucket passing the validator,
// under ANY delta that succeeds, still passes — no delta can produce an
// undiagrammed status/shape combination.
//
// SCOPE (finding, 2026-06-11): delta-level status closure holds for create,
// release, terminal-release, and impair. It does NOT hold per-delta for
// consume / backing-withdraw / backing-add: those normalize bucket status at
// the PUBLIC-OP boundary (validate_shape), not after each internal delta —
// e.g. consuming the last valid lien off a bucket that still carries impaired
// liens leaves a transient Fresh-with-zero-active shape that the surrounding
// op rejects or normalizes before returning. Evidence it is boundary-enforced
// and not a reachable bug: the 400-case full-close conservation fuzz passes
// validate_shape across entire realize sequences. The per-op invariant for
// these three is covered by the suite's validate_shape post-assertions.

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn closure_bucket_status_machine_prepare_counterparty_lien_create_delta() {
    let (b, s, r) = kani_any_ledger_triple();
    let amount: u128 = kani::any();
    kani::assume(amount < 1u128 << 96);
    kani::assume(kani_ledger_inv(&b, &s, &r));
    kani::assume(V16Core::validate_backing_bucket_static(b) == Ok(()));
    if let Ok((b2, _s2)) =
        V16Core::prepare_counterparty_lien_create_delta(b, s, kani::any(), amount)
    {
        assert_eq!(V16Core::validate_backing_bucket_static(b2), Ok(()));
    }
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn closure_bucket_status_machine_prepare_counterparty_lien_release_delta() {
    let (b, s, r) = kani_any_ledger_triple();
    let amount: u128 = kani::any();
    kani::assume(amount < 1u128 << 96);
    kani::assume(kani_ledger_inv(&b, &s, &r));
    kani::assume(V16Core::validate_backing_bucket_static(b) == Ok(()));
    if let Ok((b2, _s2)) =
        V16Core::prepare_counterparty_lien_release_delta(b, s, kani::any(), amount)
    {
        assert_eq!(V16Core::validate_backing_bucket_static(b2), Ok(()));
    }
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn closure_bucket_status_machine_prepare_counterparty_lien_terminal_release_delta() {
    let (b, s, r) = kani_any_ledger_triple();
    let amount: u128 = kani::any();
    kani::assume(amount < 1u128 << 96);
    kani::assume(kani_ledger_inv(&b, &s, &r));
    kani::assume(V16Core::validate_backing_bucket_static(b) == Ok(()));
    if let Ok((b2, _s2)) = V16Core::prepare_counterparty_lien_terminal_release_delta(b, s, amount) {
        assert_eq!(V16Core::validate_backing_bucket_static(b2), Ok(()));
    }
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn closure_bucket_status_machine_prepare_counterparty_lien_impair_delta() {
    let (b, s, r) = kani_any_ledger_triple();
    let amount: u128 = kani::any();
    kani::assume(amount < 1u128 << 96);
    kani::assume(kani_ledger_inv(&b, &s, &r));
    kani::assume(V16Core::validate_backing_bucket_static(b) == Ok(()));
    if let Ok((b2, _s2)) = V16Core::prepare_counterparty_lien_impair_delta(b, s, amount) {
        assert_eq!(V16Core::validate_backing_bucket_static(b2), Ok(()));
    }
}

#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(TokenValueFlowProofV16::debit)]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn contract_check_flow_proof_debit_modifies() {
    let mut p = TokenValueFlowProofV16::empty(kani::any(), kani::any());
    let class: TokenValueClassV16 = kani::any();
    let amount: u128 = kani::any();
    kani::assume(amount < 1u128 << 96);
    let _ = p.debit(class, amount);
}

// kernel-proofs: contract check for the same-side leg-resize PRODUCTION
// kernel (the position-delta stage of the trade/liquidation paths).
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_resize_leg_same_side)]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn contract_check_kernel_resize_leg_same_side() {
    let portfoliolegv16 = PortfolioLegV16 {
        active: true,
        asset_index: kani::any(),
        market_id: kani::any(),
        side: if kani::any() {
            SideV16::Long
        } else {
            SideV16::Short
        },
        basis_pos_q: kani::any(),
        a_basis: kani::any(),
        k_snap: kani::any(),
        f_snap: kani::any(),
        epoch_snap: kani::any(),
        loss_weight: kani::any(),
        b_snap: kani::any(),
        b_rem: kani::any(),
        b_epoch_snap: kani::any(),
        b_stale: kani::any(),
        stale: kani::any(),
    };
    let assetstatev16 = AssetStateV16 {
        market_id: kani::any(),
        retired_slot: kani::any(),
        lifecycle: AssetLifecycleV16::Active,
        raw_oracle_target_price: kani::any(),
        effective_price: kani::any(),
        fund_px_last: kani::any(),
        slot_last: kani::any(),
        a_long: kani::any(),
        a_short: kani::any(),
        k_long: kani::any(),
        k_short: kani::any(),
        f_long_num: kani::any(),
        f_short_num: kani::any(),
        k_epoch_start_long: kani::any(),
        k_epoch_start_short: kani::any(),
        f_epoch_start_long_num: kani::any(),
        f_epoch_start_short_num: kani::any(),
        b_long_num: kani::any(),
        b_short_num: kani::any(),
        b_epoch_start_long_num: kani::any(),
        b_epoch_start_short_num: kani::any(),
        oi_eff_long_q: kani::any(),
        oi_eff_short_q: kani::any(),
        stored_pos_count_long: kani::any(),
        stored_pos_count_short: kani::any(),
        stale_account_count_long: kani::any(),
        stale_account_count_short: kani::any(),
        pending_obligation_count_long: kani::any(),
        pending_obligation_count_short: kani::any(),
        loss_weight_sum_long: kani::any(),
        loss_weight_sum_short: kani::any(),
        social_loss_remainder_long_num: kani::any(),
        social_loss_remainder_short_num: kani::any(),
        social_loss_dust_long_num: kani::any(),
        social_loss_dust_short_num: kani::any(),
        explicit_unallocated_loss_long: kani::any(),
        explicit_unallocated_loss_short: kani::any(),
        epoch_long: kani::any(),
        epoch_short: kani::any(),
        mode_long: if kani::any() {
            SideModeV16::Normal
        } else {
            SideModeV16::ResetPending
        },
        mode_short: if kani::any() {
            SideModeV16::Normal
        } else {
            SideModeV16::ResetPending
        },
    };
    let new_signed: i128 = kani::any();
    let new_weight: u128 = kani::any();
    let preserve: bool = kani::any();
    kani::assume(new_signed != 0);
    kani::assume(new_signed > i128::MIN);
    let _ = V16Core::kernel_resize_leg_same_side(
        portfoliolegv16,
        assetstatev16,
        new_signed,
        new_weight,
        preserve,
    );
}

#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_attach_leg)]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn contract_check_kernel_attach_leg() {
    let assetstatev16 = AssetStateV16 {
        market_id: kani::any(),
        retired_slot: kani::any(),
        lifecycle: AssetLifecycleV16::Active,
        raw_oracle_target_price: kani::any(),
        effective_price: kani::any(),
        fund_px_last: kani::any(),
        slot_last: kani::any(),
        a_long: kani::any(),
        a_short: kani::any(),
        k_long: kani::any(),
        k_short: kani::any(),
        f_long_num: kani::any(),
        f_short_num: kani::any(),
        k_epoch_start_long: kani::any(),
        k_epoch_start_short: kani::any(),
        f_epoch_start_long_num: kani::any(),
        f_epoch_start_short_num: kani::any(),
        b_long_num: kani::any(),
        b_short_num: kani::any(),
        b_epoch_start_long_num: kani::any(),
        b_epoch_start_short_num: kani::any(),
        oi_eff_long_q: kani::any(),
        oi_eff_short_q: kani::any(),
        stored_pos_count_long: kani::any(),
        stored_pos_count_short: kani::any(),
        stale_account_count_long: kani::any(),
        stale_account_count_short: kani::any(),
        pending_obligation_count_long: kani::any(),
        pending_obligation_count_short: kani::any(),
        loss_weight_sum_long: kani::any(),
        loss_weight_sum_short: kani::any(),
        social_loss_remainder_long_num: kani::any(),
        social_loss_remainder_short_num: kani::any(),
        social_loss_dust_long_num: kani::any(),
        social_loss_dust_short_num: kani::any(),
        explicit_unallocated_loss_long: kani::any(),
        explicit_unallocated_loss_short: kani::any(),
        epoch_long: kani::any(),
        epoch_short: kani::any(),
        mode_long: if kani::any() {
            SideModeV16::Normal
        } else {
            SideModeV16::ResetPending
        },
        mode_short: if kani::any() {
            SideModeV16::Normal
        } else {
            SideModeV16::ResetPending
        },
    };
    let side = if kani::any() {
        SideV16::Long
    } else {
        SideV16::Short
    };
    let basis_pos_q: i128 = kani::any();
    let loss_weight: u128 = kani::any();
    let asset_index_u32: u32 = kani::any();
    kani::assume(basis_pos_q != 0 && basis_pos_q > i128::MIN);
    let _ = V16Core::kernel_attach_leg(
        assetstatev16,
        side,
        basis_pos_q,
        loss_weight,
        asset_index_u32,
    );
}

#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_fold_social_loss_dust)]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn contract_check_kernel_fold_social_loss_dust() {
    let current_dust: u128 = kani::any();
    let leg_remainder: u128 = kani::any();
    kani::assume(current_dust < SOCIAL_LOSS_DEN);
    kani::assume(leg_remainder < SOCIAL_LOSS_DEN);
    let result = V16Core::kernel_fold_social_loss_dust(current_dust, leg_remainder);
    kani::cover!(
        current_dust + leg_remainder < SOCIAL_LOSS_DEN
            && result == Ok((current_dust + leg_remainder, 0)),
        "canonical fractions can fold without a carry"
    );
    kani::cover!(
        current_dust + leg_remainder >= SOCIAL_LOSS_DEN
            && result == Ok((current_dust + leg_remainder - SOCIAL_LOSS_DEN, 1)),
        "canonical fractions can fold one whole-atom carry"
    );
}

#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_quarantine_social_loss_remainder)]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn contract_check_kernel_quarantine_social_loss_remainder() {
    let social_remainder: u128 = kani::any();
    let current_dust: u128 = kani::any();
    let explicit_before: u128 = kani::any();
    kani::assume(social_remainder < SOCIAL_LOSS_DEN);
    kani::assume(current_dust < SOCIAL_LOSS_DEN);
    kani::assume(explicit_before < u128::MAX);
    let result = V16Core::kernel_quarantine_social_loss_remainder(
        social_remainder,
        current_dust,
        explicit_before,
    );
    kani::cover!(
        current_dust + social_remainder < SOCIAL_LOSS_DEN
            && result == Ok((current_dust + social_remainder, explicit_before)),
        "canonical reset remainder can fold without a carry"
    );
    kani::cover!(
        current_dust + social_remainder >= SOCIAL_LOSS_DEN
            && result
                == Ok((
                    current_dust + social_remainder - SOCIAL_LOSS_DEN,
                    explicit_before + 1,
                )),
        "canonical reset remainder can quarantine one explicit loss atom"
    );
}

#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_side_needs_full_drain_reset)]
#[kani::unwind(2)]
#[kani::solver(cadical)]
fn contract_check_kernel_side_needs_full_drain_reset() {
    let effective_oi: u128 = kani::any();
    let stored_position_count: u64 = kani::any();
    let pending_obligation_count: u64 = kani::any();
    let mode = match kani::any::<u8>() % 3 {
        0 => SideModeV16::Normal,
        1 => SideModeV16::DrainOnly,
        _ => SideModeV16::ResetPending,
    };
    let reset = V16Core::kernel_side_needs_full_drain_reset(
        effective_oi,
        stored_position_count,
        pending_obligation_count,
        mode,
    );
    kani::cover!(reset, "zero OI with stored positions requires reset");
    kani::cover!(!reset, "a non-reset state remains unchanged");
}

// Composition proof for the production unilateral-close route. The scalar
// predicate above is insufficient on its own: this theorem also proves that
// the mutable view applies the decision to both possible closed-side
// orientations after the matching side reaches zero OI.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof]
#[kani::unwind(20)]
#[kani::solver(cadical)]
fn composition_unilateral_close_resets_both_zero_oi_sides() {
    let cfg = V16Config::public_user_fund_with_market_slots(1, 1, 0, 10);
    let mut header = MarketGroupV16HeaderAccount::new_dynamic([1u8; 32], cfg, 1, 0).unwrap();
    let mut markets = [Market::new(0u64, EngineAssetSlotV16Account::default())];
    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        market.activate_empty_market_not_atomic(0, 100, 1).unwrap();
    }

    let closed_side = if kani::any() {
        SideV16::Long
    } else {
        SideV16::Short
    };
    let stored_position_count = u64::from(kani::any::<u8>()).saturating_add(1);
    let mut asset = markets[0].engine.asset.try_to_runtime().unwrap();
    match closed_side {
        SideV16::Long => {
            asset.oi_eff_long_q = 0;
            asset.stored_pos_count_long = stored_position_count;
            asset.oi_eff_short_q = 1;
            asset.stored_pos_count_short = 1;
        }
        SideV16::Short => {
            asset.oi_eff_short_q = 0;
            asset.stored_pos_count_short = stored_position_count;
            asset.oi_eff_long_q = 1;
            asset.stored_pos_count_long = 1;
        }
    }
    markets[0].engine.asset = AssetStateV16Account::from_runtime(&asset);

    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        market
            .kani_reduce_matching_open_interest_for_unilateral_close(0, closed_side, 1)
            .unwrap();
    }

    let after = markets[0].engine.asset.try_to_runtime().unwrap();
    assert_eq!(after.oi_eff_long_q, 0);
    assert_eq!(after.oi_eff_short_q, 0);
    assert_eq!(after.mode_long, SideModeV16::ResetPending);
    assert_eq!(after.mode_short, SideModeV16::ResetPending);
    kani::cover!(
        closed_side == SideV16::Long,
        "long closed side enters reset"
    );
    kani::cover!(
        closed_side == SideV16::Short,
        "short closed side enters reset"
    );
}

#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_clear_leg)]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn contract_check_kernel_clear_leg() {
    let leg = PortfolioLegV16 {
        active: true,
        asset_index: kani::any(),
        market_id: kani::any(),
        side: if kani::any() {
            SideV16::Long
        } else {
            SideV16::Short
        },
        basis_pos_q: kani::any(),
        a_basis: kani::any(),
        k_snap: kani::any(),
        f_snap: kani::any(),
        epoch_snap: kani::any(),
        loss_weight: kani::any(),
        b_snap: kani::any(),
        b_rem: kani::any(),
        b_epoch_snap: kani::any(),
        b_stale: false,
        stale: false,
    };
    let asset = AssetStateV16 {
        market_id: kani::any(),
        retired_slot: kani::any(),
        lifecycle: AssetLifecycleV16::Active,
        raw_oracle_target_price: kani::any(),
        effective_price: kani::any(),
        fund_px_last: kani::any(),
        slot_last: kani::any(),
        a_long: kani::any(),
        a_short: kani::any(),
        k_long: kani::any(),
        k_short: kani::any(),
        f_long_num: kani::any(),
        f_short_num: kani::any(),
        k_epoch_start_long: kani::any(),
        k_epoch_start_short: kani::any(),
        f_epoch_start_long_num: kani::any(),
        f_epoch_start_short_num: kani::any(),
        b_long_num: kani::any(),
        b_short_num: kani::any(),
        b_epoch_start_long_num: kani::any(),
        b_epoch_start_short_num: kani::any(),
        oi_eff_long_q: kani::any(),
        oi_eff_short_q: kani::any(),
        stored_pos_count_long: kani::any(),
        stored_pos_count_short: kani::any(),
        stale_account_count_long: kani::any(),
        stale_account_count_short: kani::any(),
        pending_obligation_count_long: kani::any(),
        pending_obligation_count_short: kani::any(),
        loss_weight_sum_long: kani::any(),
        loss_weight_sum_short: kani::any(),
        social_loss_remainder_long_num: kani::any(),
        social_loss_remainder_short_num: kani::any(),
        social_loss_dust_long_num: kani::any(),
        social_loss_dust_short_num: kani::any(),
        explicit_unallocated_loss_long: kani::any(),
        explicit_unallocated_loss_short: kani::any(),
        epoch_long: kani::any(),
        epoch_short: kani::any(),
        mode_long: if kani::any() {
            SideModeV16::Normal
        } else {
            SideModeV16::ResetPending
        },
        mode_short: if kani::any() {
            SideModeV16::Normal
        } else {
            SideModeV16::ResetPending
        },
    };
    kani::assume(leg.basis_pos_q > i128::MIN);
    kani::assume(leg.b_rem < SOCIAL_LOSS_DEN);
    kani::assume(asset.social_loss_dust_long_num < SOCIAL_LOSS_DEN);
    kani::assume(asset.social_loss_dust_short_num < SOCIAL_LOSS_DEN);
    let _ = V16Core::kernel_clear_leg(leg, asset);
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn proof_clear_leg_canonical_dust_never_blocks_exit() {
    let side = if kani::any() {
        SideV16::Long
    } else {
        SideV16::Short
    };
    let current_dust: u128 = kani::any();
    let leg_remainder: u128 = kani::any();
    kani::assume(current_dust < SOCIAL_LOSS_DEN);
    kani::assume(leg_remainder < SOCIAL_LOSS_DEN);

    let basis_abs = POS_SCALE;
    let basis_pos_q = match side {
        SideV16::Long => basis_abs as i128,
        SideV16::Short => -(basis_abs as i128),
    };
    let leg = PortfolioLegV16 {
        active: true,
        asset_index: 0,
        market_id: 1,
        side,
        basis_pos_q,
        a_basis: POS_SCALE,
        k_snap: 0,
        f_snap: 0,
        epoch_snap: 0,
        loss_weight: 1,
        b_snap: 0,
        b_rem: leg_remainder,
        b_epoch_snap: 0,
        b_stale: false,
        stale: false,
    };
    let mut asset = AssetStateV16::default();
    asset.market_id = 1;
    asset.lifecycle = AssetLifecycleV16::Active;
    match side {
        SideV16::Long => {
            asset.oi_eff_long_q = basis_abs;
            asset.stored_pos_count_long = 1;
            asset.loss_weight_sum_long = 1;
            asset.social_loss_dust_long_num = current_dust;
        }
        SideV16::Short => {
            asset.oi_eff_short_q = basis_abs;
            asset.stored_pos_count_short = 1;
            asset.loss_weight_sum_short = 1;
            asset.social_loss_dust_short_num = current_dust;
        }
    }

    let cleared = V16Core::kernel_clear_leg(leg, asset).unwrap();
    let expected_dust = (current_dust + leg_remainder) % SOCIAL_LOSS_DEN;
    match side {
        SideV16::Long => {
            assert_eq!(cleared.oi_eff_long_q, 0);
            assert_eq!(cleared.stored_pos_count_long, 0);
            assert_eq!(cleared.loss_weight_sum_long, 0);
            assert_eq!(cleared.social_loss_dust_long_num, expected_dust);
        }
        SideV16::Short => {
            assert_eq!(cleared.oi_eff_short_q, 0);
            assert_eq!(cleared.stored_pos_count_short, 0);
            assert_eq!(cleared.loss_weight_sum_short, 0);
            assert_eq!(cleared.social_loss_dust_short_num, expected_dust);
        }
    }
    kani::cover!(
        current_dust + leg_remainder < SOCIAL_LOSS_DEN,
        "a no-carry exit clears"
    );
    kani::cover!(
        current_dust + leg_remainder >= SOCIAL_LOSS_DEN,
        "a whole-atom carry exit clears"
    );
}

#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_advance_leg_b_snap)]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn contract_check_kernel_advance_leg_b_snap() {
    let leg = PortfolioLegV16 {
        active: kani::any(),
        asset_index: kani::any(),
        market_id: kani::any(),
        side: if kani::any() {
            SideV16::Long
        } else {
            SideV16::Short
        },
        basis_pos_q: kani::any(),
        a_basis: kani::any(),
        k_snap: kani::any(),
        f_snap: kani::any(),
        epoch_snap: kani::any(),
        loss_weight: kani::any(),
        b_snap: kani::any(),
        b_rem: kani::any(),
        b_epoch_snap: kani::any(),
        b_stale: kani::any(),
        stale: kani::any(),
    };
    let delta_b: u128 = kani::any();
    let new_remainder: u128 = kani::any();
    let remaining_after: u128 = kani::any();
    let _ = V16Core::kernel_advance_leg_b_snap(leg, delta_b, new_remainder, remaining_after);
}

#[cfg(all(kani, feature = "contracts"))]
#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn closure_kernel_advance_close_ledger_rank_witness() {
    // plain full-domain witness (the contract form exceeds the solver budget;
    // identical evidentiary power per the flow-witness precedent)
    let ledger = CloseProgressLedgerV16 {
        active: kani::any(),
        finalized: kani::any(),
        canceled: kani::any(),
        close_id: kani::any(),
        asset_index: kani::any(),
        market_id: kani::any(),
        domain_side: if kani::any() {
            SideV16::Long
        } else {
            SideV16::Short
        },
        gross_loss_at_close_start: kani::any(),
        drift_reference_slot: kani::any(),
        max_close_slot: kani::any(),
        support_consumed: kani::any(),
        junior_face_burned: kani::any(),
        insurance_spent: kani::any(),
        b_loss_booked: kani::any(),
        explicit_loss_assigned: kani::any(),
        quantity_adl_applied_q: kani::any(),
        drift_consumed: kani::any(),
        residual_remaining: kani::any(),
    };
    let sc: u64 = kani::any();
    let jf: u64 = kani::any();
    let is_: u64 = kani::any();
    let bl: u64 = kani::any();
    let el: u64 = kani::any();
    let (sc, jf, is_, bl, el) = (sc as u128, jf as u128, is_ as u128, bl as u128, el as u128);
    // validated-ledger precondition (production-guaranteed)
    kani::assume(ledger.gross_loss_at_close_start < 1u128 << 64);
    kani::assume(ledger.drift_consumed < 1u128 << 64);
    kani::assume(ledger.support_consumed < 1u128 << 64);
    kani::assume(ledger.insurance_spent < 1u128 << 64);
    kani::assume(ledger.b_loss_booked < 1u128 << 64);
    kani::assume(ledger.explicit_loss_assigned < 1u128 << 64);
    let total = ledger.gross_loss_at_close_start + ledger.drift_consumed;
    let pre_progress = ledger.support_consumed
        + ledger.insurance_spent
        + ledger.b_loss_booked
        + ledger.explicit_loss_assigned;
    kani::assume(pre_progress <= total);
    kani::assume(ledger.residual_remaining == total - pre_progress);

    if let Ok(l) = V16Core::kernel_advance_close_ledger(ledger, sc, jf, is_, bl, el) {
        let booked = sc + is_ + bl + el;
        kani::cover!(booked > 0, "rank witness covers real progress");
        // exact category deltas
        assert_eq!(l.support_consumed, ledger.support_consumed + sc);
        assert_eq!(l.junior_face_burned, ledger.junior_face_burned + jf);
        assert_eq!(l.insurance_spent, ledger.insurance_spent + is_);
        assert_eq!(l.b_loss_booked, ledger.b_loss_booked + bl);
        assert_eq!(l.explicit_loss_assigned, ledger.explicit_loss_assigned + el);
        // THE RANK: residual decreases by exactly the booked total
        assert_eq!(l.residual_remaining, ledger.residual_remaining - booked);
        assert!(l.residual_remaining <= ledger.residual_remaining);
        // finalization is sticky-exact
        assert_eq!(l.finalized, ledger.finalized || l.residual_remaining == 0);
        // immutable identity frozen
        assert_eq!(l.close_id, ledger.close_id);
        assert_eq!(
            l.gross_loss_at_close_start,
            ledger.gross_loss_at_close_start
        );
        assert_eq!(l.drift_reference_slot, ledger.drift_reference_slot);
        assert_eq!(l.max_close_slot, ledger.max_close_slot);
        assert_eq!(l.asset_index, ledger.asset_index);
        assert_eq!(l.market_id, ledger.market_id);
        assert_eq!(l.quantity_adl_applied_q, ledger.quantity_adl_applied_q);
        assert_eq!(l.drift_consumed, ledger.drift_consumed);
        assert_eq!(l.active, ledger.active);
        assert_eq!(l.canceled, ledger.canceled);
    }
}

#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_initial_margin_gate)]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn contract_check_kernel_initial_margin_gate() {
    let cert = HealthCertV16 {
        certified_equity: kani::any(),
        certified_initial_req: kani::any(),
        certified_maintenance_req: kani::any(),
        certified_liq_deficit: kani::any(),
        certified_worst_case_loss: kani::any(),
        cert_oracle_epoch: kani::any(),
        cert_funding_epoch: kani::any(),
        cert_risk_epoch: kani::any(),
        cert_asset_set_epoch: kani::any(),
        active_bitmap_at_cert: kani::any(),
        valid: kani::any(),
    };
    let _ = V16Core::kernel_initial_margin_gate(cert);
}

#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_locked_margin_gate)]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn contract_check_kernel_locked_margin_gate() {
    let capital: u128 = kani::any();
    let pnl: i128 = kani::any();
    let fee_credits: i128 = kani::any();
    let req: u128 = kani::any();
    kani::assume(pnl > i128::MIN && fee_credits > i128::MIN && capital < 1u128 << 100);
    let _ = V16Core::kernel_locked_margin_gate(capital, pnl, fee_credits, req);
}

#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_accumulate_batch_trade)]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn contract_check_kernel_accumulate_batch_trade() {
    let outcome = BatchTradeOutcomeV16 {
        fill_count: kani::any(),
        fee_a: kani::any(),
        fee_b: kani::any(),
        notional: kani::any(),
    };
    let applied = TradeApplyOutcomeV16 {
        fee_a: kani::any(),
        fee_b: kani::any(),
        notional: kani::any(),
        risk_increasing: kani::any(),
        long_has_source_claims: kani::any(),
        short_has_source_claims: kani::any(),
    };
    let _ = V16Core::kernel_accumulate_batch_trade(
        outcome,
        kani::any(),
        kani::any(),
        kani::any(),
        applied,
    );
}

#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(MarketGroupV16ViewMut::asset_restart_next_counters)]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn contract_check_asset_restart_next_counters() {
    let _ = MarketGroupV16ViewMut::<Market<u64>>::asset_restart_next_counters(
        kani::any(),
        kani::any(),
        kani::any(),
        kani::any(),
    );
}

#[cfg(all(kani, feature = "contracts"))]
#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn closure_restarted_slot_preserves_budget_witness() {
    // plain witness (the proof_for_contract form memcmp's the big slot struct;
    // field-wise asserts here avoid it, identical evidentiary value)
    let mut old_slot = EngineAssetSlotV16Account::default();
    let bl: u128 = kani::any();
    let bs: u128 = kani::any();
    old_slot.insurance_domain_budget_long = V16PodU128::new(bl);
    old_slot.insurance_domain_budget_short = V16PodU128::new(bs);
    let mid: u64 = kani::any();
    let px: u64 = kani::any();
    let now: u64 = kani::any();
    let s = MarketGroupV16ViewMut::<Market<u64>>::restarted_asset_slot_preserving_insurance_budget(
        &old_slot, mid, px, now,
    );
    // budgets preserved exactly for ANY prior budget
    assert_eq!(s.insurance_domain_budget_long.get(), bl);
    assert_eq!(s.insurance_domain_budget_short.get(), bs);
    // fresh empty stock at the new identity; no carried position/risk/spend
    assert_eq!(s.asset.market_id.get(), mid);
    assert_eq!(s.asset.effective_price.get(), px);
    assert_eq!(s.asset.raw_oracle_target_price.get(), px);
    assert_eq!(s.asset.slot_last.get(), now);
    assert_eq!(s.asset.oi_eff_long_q.get(), 0);
    assert_eq!(s.asset.oi_eff_short_q.get(), 0);
    assert_eq!(s.asset.stored_pos_count_long.get(), 0);
    assert_eq!(s.asset.stored_pos_count_short.get(), 0);
    assert_eq!(s.pending_domain_loss_barrier_long.get(), 0);
    assert_eq!(s.pending_domain_loss_barrier_short.get(), 0);
    assert_eq!(s.insurance_domain_spent_long.get(), 0);
    assert_eq!(s.insurance_domain_spent_short.get(), 0);
}

// ============ COMPOSITION via division-stub (kernel-proofs) ============
// Whole-body frame for attach_leg_at_slot, made tractable by stubbing ONLY
// the documented-intractable division primitive loss_weight_for_basis to an
// arbitrary value. This is SOUND for a frame property: the frame asserts WHERE
// the weight is written (leg.loss_weight, the side weight sum), not its value;
// the value's exactness is the separately-proven kernel_attach_leg contract.
// With the division gone, the body is gates + the cheap real kernel + slot
// placement — the composition the direct/stub_verified routes could not reach.
#[cfg(all(kani, feature = "contracts"))]
fn kani_any_loss_weight(_abs_basis_q: u128, _a_basis: u128) -> V16Result<u128> {
    let w: u128 = kani::any();
    kani::assume(w != 0);
    Ok(w)
}

#[cfg(all(kani, feature = "contracts"))]
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
#[kani::stub(crate::v16::loss_weight_for_basis, kani_any_loss_weight)]
#[kani::stub_verified(V16Core::kernel_attach_leg)]
fn composition_attach_body_frame_division_stubbed() {
    let cfg = V16Config::public_user_fund_with_market_slots(1, 1, 0, 10);
    let mut header = MarketGroupV16HeaderAccount::new_dynamic([1u8; 32], cfg, 1, 0).unwrap();
    let mut markets = [Market::new(0u64, EngineAssetSlotV16Account::default())];
    {
        let mut v = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        v.activate_empty_market_not_atomic(0, 100, 1).unwrap();
    }
    let prov = ProvenanceHeaderV16Account::from_runtime(&ProvenanceHeaderV16::new(
        [1u8; 32], [2u8; 32], [2u8; 32],
    ));
    let mut account_header = PortfolioAccountV16Account::default();
    account_header.init_empty_in_place(prov).unwrap();
    account_header.last_fee_slot = V16PodU64::new(1);
    let basis: i128 = kani::any();
    kani::assume(basis != 0 && basis > i128::MIN);
    let side = if kani::any() {
        SideV16::Long
    } else {
        SideV16::Short
    };

    let a0 = account_header;
    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        let mut account = PortfolioV16ViewMut::new(&mut account_header);
        if market
            .kani_attach_leg_at_slot(&mut account, 0, side, basis, 0)
            .is_err()
        {
            return;
        }
    }
    kani::cover!(true, "division-stubbed attach body frame reached");
    // WHOLE-BODY FRAME: the body touches ONLY leg[0], the active bitmap, and
    // the health cert in the account; every other account field is frozen.
    let mut expected = a0;
    expected.legs[0] = account_header.legs[0];
    expected.active_bitmap = account_header.active_bitmap;
    expected.health_cert = account_header.health_cert;
    assert!(kani_eq_portfolio_account_v16_account(
        &expected,
        &account_header
    ));
    // and only slot 0 became active
    let mut i = 1;
    while i < V16_MAX_PORTFOLIO_ASSETS_N {
        assert!(!account_header.legs[i].try_to_runtime().unwrap().active);
        i += 1;
    }
}

// Composition frame for clear_leg: stub_verified(kernel_clear_leg) abstracts
// the asset transform (the body has NO division — it uses the leg's existing
// weight), so only the kernel-contract-check interaction needs the stub. The
// whole-body frame: clearing the leg at the active slot sets that leg EMPTY,
// clears its bitmap bit, and invalidates the cert — every OTHER leg and
// account field frozen.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
#[kani::stub(crate::v16::loss_weight_for_basis, kani_any_loss_weight)]
#[kani::stub_verified(V16Core::kernel_clear_leg)]
#[kani::stub_verified(V16Core::kernel_attach_leg)]
fn composition_clear_leg_body_frame() {
    let cfg = V16Config::public_user_fund_with_market_slots(1, 1, 0, 10);
    let mut header = MarketGroupV16HeaderAccount::new_dynamic([1u8; 32], cfg, 1, 0).unwrap();
    let mut markets = [Market::new(0u64, EngineAssetSlotV16Account::default())];
    {
        let mut v = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        v.activate_empty_market_not_atomic(0, 100, 1).unwrap();
    }
    let prov = ProvenanceHeaderV16Account::from_runtime(&ProvenanceHeaderV16::new(
        [1u8; 32], [2u8; 32], [2u8; 32],
    ));
    let mut account_header = PortfolioAccountV16Account::default();
    account_header.init_empty_in_place(prov).unwrap();
    account_header.last_fee_slot = V16PodU64::new(1);
    // attach a leg at slot 0 first (so there is something to clear), via the
    // real path with division stubbed (frame-irrelevant weight)
    let basis: i128 = kani::any();
    kani::assume(basis != 0 && basis > i128::MIN);
    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        let mut account = PortfolioV16ViewMut::new(&mut account_header);
        if market
            .kani_attach_leg_at_slot(&mut account, 0, SideV16::Long, basis, 0)
            .is_err()
        {
            return;
        }
    }
    let a1 = account_header;
    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        let mut account = PortfolioV16ViewMut::new(&mut account_header);
        if market.kani_clear_leg(&mut account, 0).is_err() {
            return;
        }
    }
    kani::cover!(true, "clear_leg body frame reached");
    // FRAME: clear touches only leg[0], the bitmap, and the cert
    let mut expected = a1;
    expected.legs[0] = account_header.legs[0];
    expected.active_bitmap = account_header.active_bitmap;
    expected.health_cert = account_header.health_cert;
    assert!(kani_eq_portfolio_account_v16_account(
        &expected,
        &account_header
    ));
    // leg[0] is now empty/inactive
    assert!(!account_header.legs[0].try_to_runtime().unwrap().active);
}

// Exact on the only two arithmetic calls reachable from the fixture below:
// an aligned +2 K delta and a zero F delta. The general production helper is
// independently proven against its aligned reference in
// tests/proofs_v16_arithmetic.rs; this stub removes only that bit-vector
// division leaf so Kani can prove the surrounding attribution/state theorem.
#[cfg(all(kani, feature = "closure"))]
fn positive_two_scaled_adl_delta_stub(
    abs_basis_q: u128,
    a_basis: u128,
    then: i128,
    now: i128,
) -> Option<i128> {
    assert_eq!(abs_basis_q, POS_SCALE);
    assert_eq!(a_basis, ADL_ONE);
    if then == now {
        Some(0)
    } else {
        assert_eq!(then, 0);
        assert_eq!(now, 2 * ADL_ONE as i128);
        Some(2)
    }
}

#[cfg(all(kani, feature = "closure"))]
fn negative_two_scaled_adl_delta_stub(
    abs_basis_q: u128,
    a_basis: u128,
    then: i128,
    now: i128,
) -> Option<i128> {
    assert_eq!(abs_basis_q, POS_SCALE);
    assert_eq!(a_basis, ADL_ONE);
    if then == now {
        Some(0)
    } else {
        assert_eq!(then, 0);
        assert_eq!(now, -2 * ADL_ONE as i128);
        Some(-2)
    }
}

// Exact on the four calls reachable from the two-leg cross-asset fixture:
// aligned +/-2 K deltas and zero F deltas. The generic arithmetic remains
// independently full-domain proven in proofs_v16_arithmetic.rs.
#[cfg(all(kani, feature = "closure"))]
fn signed_two_scaled_adl_delta_stub(
    abs_basis_q: u128,
    a_basis: u128,
    then: i128,
    now: i128,
) -> Option<i128> {
    assert_eq!(abs_basis_q, POS_SCALE);
    assert_eq!(a_basis, ADL_ONE);
    if then == now {
        assert_eq!(then, 0);
        Some(0)
    } else {
        assert_eq!(then, 0);
        if now == 2 * ADL_ONE as i128 {
            Some(2)
        } else {
            assert_eq!(now, -2 * ADL_ONE as i128);
            Some(-2)
        }
    }
}

#[cfg(all(kani, feature = "closure"))]
fn zero_backing_source_credit_rate_stub(state: SourceCreditStateV16) -> V16Result<u128> {
    // Exact specialization of the independently proven rate helper: these
    // fixture states have no available counterparty or insurance backing.
    assert_eq!(state.fresh_reserved_backing_num, 0);
    assert_eq!(state.valid_liened_backing_num, 0);
    assert_eq!(state.impaired_liened_backing_num, 0);
    assert_eq!(state.spent_backing_num, 0);
    assert_eq!(state.provider_receivable_num, 0);
    assert_eq!(state.insurance_credit_reserved_num, 0);
    assert_eq!(state.valid_liened_insurance_num, 0);
    assert_eq!(state.impaired_liened_insurance_num, 0);
    if state.positive_claim_bound_num == 0 {
        Ok(CREDIT_RATE_SCALE)
    } else {
        Ok(0)
    }
}

#[cfg(all(kani, feature = "closure"))]
fn no_claim_source_credit_rate_stub(state: SourceCreditStateV16) -> V16Result<u128> {
    assert_eq!(state.positive_claim_bound_num, 0);
    assert_eq!(state.exact_positive_claim_num, 0);
    assert_eq!(state.valid_liened_backing_num, 0);
    assert_eq!(state.impaired_liened_backing_num, 0);
    assert_eq!(state.spent_backing_num, 0);
    assert_eq!(state.provider_receivable_num, 0);
    assert_eq!(state.insurance_credit_reserved_num, 0);
    assert_eq!(state.valid_liened_insurance_num, 0);
    assert_eq!(state.impaired_liened_insurance_num, 0);
    assert_eq!(state.credit_rate_num, CREDIT_RATE_SCALE);
    Ok(CREDIT_RATE_SCALE)
}

// Exact rate specialization for cross-asset settlement. A reachable domain is
// either an unsupported positive claim or a claim-free domain with optional
// fresh backing; no lien, spend, receivable, or insurance state is abstracted.
#[cfg(all(kani, feature = "closure"))]
fn cross_asset_source_credit_rate_stub(state: SourceCreditStateV16) -> V16Result<u128> {
    assert_eq!(state.valid_liened_backing_num, 0);
    assert_eq!(state.impaired_liened_backing_num, 0);
    assert_eq!(state.spent_backing_num, 0);
    assert_eq!(state.provider_receivable_num, 0);
    assert_eq!(state.insurance_credit_reserved_num, 0);
    assert_eq!(state.valid_liened_insurance_num, 0);
    assert_eq!(state.impaired_liened_insurance_num, 0);
    if state.positive_claim_bound_num == 0 {
        assert_eq!(state.exact_positive_claim_num, 0);
        Ok(CREDIT_RATE_SCALE)
    } else {
        assert_eq!(
            state.exact_positive_claim_num,
            state.positive_claim_bound_num
        );
        assert_eq!(state.fresh_reserved_backing_num, 0);
        Ok(0)
    }
}

// Exact specialization of the independently proven source-support arithmetic.
// An underfunded loser queries the winner's opposing-side domain before any
// claim or backing exists there, so no effective support can be realized.
#[cfg(all(kani, feature = "closure"))]
fn empty_cross_asset_source_support_stub<'a: 'a, T>(
    market: &MarketGroupV16ViewMut<'a, T>,
    domain: usize,
    face_claim: u128,
) -> V16Result<u128> {
    assert_eq!(domain, 1);
    assert_eq!(face_claim, 2);
    assert_eq!(
        market.source_credit_for_domain(domain)?,
        SourceCreditStateV16::EMPTY
    );
    Ok(0)
}

// Exact division seam for small closure fixtures. Every reachable wide value
// must fit u128; the assertion fails if a future path widens the theorem's
// arithmetic domain. The general U256 divider is proved separately.
#[cfg(all(kani, feature = "closure"))]
fn bounded_u256_div_rem_stub(num: U256, den: U256) -> (U256, U256) {
    assert_eq!(num.hi(), 0);
    assert_eq!(den.hi(), 0);
    assert_ne!(den.lo(), 0);
    (
        U256::from_u128(num.lo() / den.lo()),
        U256::from_u128(num.lo() % den.lo()),
    )
}

#[cfg(all(kani, feature = "closure"))]
fn conversion_flow_validate_stub(proof: &TokenValueFlowProofV16) -> V16Result<()> {
    let account_debit = proof.debits[TokenValueClassV16::AccountCapital as usize];
    let insurance_debit = proof.debits[TokenValueClassV16::InsuranceCapital as usize];
    let counterparty_credit =
        proof.credits[TokenValueClassV16::CloseCounterpartyCreditConsumed as usize];
    let insurance_credit = proof.credits[TokenValueClassV16::CloseInsuranceSpent as usize];
    let mut expected_debits = [0u128; V16_TOKEN_VALUE_CLASS_COUNT];
    let mut expected_credits = [0u128; V16_TOKEN_VALUE_CLASS_COUNT];
    if account_debit == 0 && insurance_debit == 0 {
        assert_eq!(counterparty_credit, 0);
        assert_eq!(insurance_credit, 0);
    } else if insurance_debit != 0 {
        assert_eq!(account_debit, 0);
        assert_eq!(counterparty_credit, 0);
        assert_eq!(insurance_credit, insurance_debit);
        expected_debits[TokenValueClassV16::InsuranceCapital as usize] = insurance_debit;
        expected_credits[TokenValueClassV16::CloseInsuranceSpent as usize] = insurance_debit;
    } else {
        assert_ne!(account_debit, 0);
        assert_eq!(counterparty_credit + insurance_credit, account_debit);
        assert!(counterparty_credit == 0 || insurance_credit == 0);
        expected_debits[TokenValueClassV16::AccountCapital as usize] = account_debit;
        expected_credits[TokenValueClassV16::CloseCounterpartyCreditConsumed as usize] =
            counterparty_credit;
        expected_credits[TokenValueClassV16::CloseInsuranceSpent as usize] = insurance_credit;
    }
    let mut i = 0usize;
    while i < V16_TOKEN_VALUE_CLASS_COUNT {
        assert_eq!(proof.debits[i], expected_debits[i]);
        assert_eq!(proof.credits[i], expected_credits[i]);
        i += 1;
    }
    assert_eq!(proof.external_quote_in, 0);
    assert_eq!(proof.external_quote_out, 0);
    assert_eq!(proof.vault_before, proof.vault_after);
    Ok(())
}

#[cfg(all(kani, feature = "closure"))]
fn canonical_conversion_compact_stub<'a: 'a>(account: &mut PortfolioV16ViewMut<'a>) {
    let first = account.header.source_domains[0];
    assert!(first.is_occupied() || first.has_default_sparse_tag());
    let second = account.header.source_domains[1];
    assert!(!second.is_occupied() && second.has_default_sparse_tag());
}

// Full validators are universal elsewhere in the suite. This composition uses
// a constructor-valid fixture and exact whole-state postconditions, so repeating
// their bounded scans would add cost without weakening the transition frame.
#[cfg(all(kani, feature = "closure"))]
fn valid_conversion_account_stub<'a: 'a, T>(
    _account: &PortfolioV16View<'a>,
    _market: &MarketGroupV16View<'_, T>,
) -> V16Result<()> {
    Ok(())
}

// The winner-first cross-asset fixture burns its sole source claim and resets
// that slot before canonical compaction. Assert that local precondition and
// elide the independently proven fixed-cap table scan.
#[cfg(all(kani, feature = "closure"))]
fn empty_cross_asset_compact_stub<'a: 'a>(account: &mut PortfolioV16ViewMut<'a>) {
    assert_eq!(
        account.header.source_domains[0],
        PortfolioSourceDomainV16Account::default()
    );
    assert_eq!(
        account.header.source_domains[1],
        PortfolioSourceDomainV16Account::default()
    );
}

#[cfg(all(kani, feature = "closure"))]
fn valid_conversion_market_stub<'a: 'a, T>(
    _market: &MarketGroupV16ViewMut<'a, T>,
) -> V16Result<()> {
    Ok(())
}

#[cfg(all(kani, feature = "closure"))]
fn valid_domain_ledger_stub<'a: 'a, T>(
    market: &MarketGroupV16ViewMut<'a, T>,
    domain: usize,
) -> V16Result<()> {
    assert!(domain < market.markets.len() * 2);
    Ok(())
}

#[cfg(all(kani, feature = "closure"))]
fn withdrawal_flow_validate_stub(proof: &TokenValueFlowProofV16) -> V16Result<()> {
    let amount = proof.debits[TokenValueClassV16::AccountCapital as usize];
    assert_ne!(amount, 0);
    assert_eq!(
        proof.credits[TokenValueClassV16::ExternalQuote as usize],
        amount
    );
    let mut i = 0usize;
    while i < V16_TOKEN_VALUE_CLASS_COUNT {
        if i != TokenValueClassV16::AccountCapital as usize {
            assert_eq!(proof.debits[i], 0);
        }
        if i != TokenValueClassV16::ExternalQuote as usize {
            assert_eq!(proof.credits[i], 0);
        }
        i += 1;
    }
    assert_eq!(proof.external_quote_in, 0);
    assert_eq!(proof.external_quote_out, amount);
    assert_eq!(proof.vault_before - proof.vault_after, amount);
    Ok(())
}

#[cfg(all(kani, feature = "closure"))]
fn zero_pnl_principal_settlement_stub<'a: 'a, T>(
    _market: &mut MarketGroupV16ViewMut<'a, T>,
    account: &mut PortfolioV16ViewMut<'_>,
) -> V16Result<u128> {
    assert_eq!(account.header.pnl.get(), 0);
    Ok(0)
}

#[cfg(all(kani, feature = "closure"))]
fn two_asset_kf_mapping_fixture() -> (
    MarketGroupV16HeaderAccount,
    [Market<u64>; 2],
    PortfolioAccountV16Account,
) {
    // Constructor-equivalent POD state avoids pulling activation's separately
    // proven branch tree into each settlement composition.
    let market_group_id = [1u8; 32];
    let cfg = V16Config::public_user_fund_with_market_slots(2, 2, 0, 10);
    let mut header = MarketGroupV16HeaderAccount::default();
    header.market_group_id = market_group_id;
    header.config = V16ConfigAccount::from_runtime(&cfg);
    header.asset_slot_capacity = V16PodU32::new(2);
    header.next_market_id = V16PodU64::new(3);
    header.asset_activation_count = V16PodU64::new(2);
    header.last_asset_activation_slot = V16PodU64::new(2);
    header.current_slot = V16PodU64::new(2);
    header.asset_set_epoch = V16PodU64::new(2);
    header.risk_epoch = V16PodU64::new(2);
    header.vault = V16PodU128::new(10);
    header.insurance = V16PodU128::new(10);
    header.insurance_domain_budget_remaining_total = V16PodU128::new(10);
    let mut markets = [
        Market::new(0u64, EngineAssetSlotV16Account::empty_for_market(1)),
        Market::new(0u64, EngineAssetSlotV16Account::empty_for_market(2)),
    ];
    let mut asset_index = 0usize;
    while asset_index < 2 {
        let mut asset = AssetStateV16::default();
        asset.market_id = asset_index as u64 + 1;
        asset.lifecycle = AssetLifecycleV16::Active;
        asset.raw_oracle_target_price = 100;
        asset.effective_price = 100;
        asset.fund_px_last = 100;
        asset.slot_last = asset_index as u64 + 1;
        markets[asset_index].engine.asset = AssetStateV16Account::from_runtime(&asset);
        asset_index += 1;
    }
    markets[0].engine.insurance_domain_budget_long = V16PodU128::new(1);
    markets[0].engine.insurance_domain_budget_short = V16PodU128::new(2);
    markets[1].engine.insurance_domain_budget_long = V16PodU128::new(3);
    markets[1].engine.insurance_domain_budget_short = V16PodU128::new(4);

    let provenance = ProvenanceHeaderV16Account::from_runtime(&ProvenanceHeaderV16::new(
        market_group_id,
        [2u8; 32],
        [3u8; 32],
    ));
    let mut account = PortfolioAccountV16Account::default();
    account.provenance_header = provenance;
    account.owner = [3u8; 32];
    (header, markets, account)
}

// Production K/F settlement must attribute a winning leg to the selected
// asset's opposing-side source domain. A wrong side leaks the winner's own
// backing; a wrong asset breaks isolation. This composition executes the real
// lookup, PnL setter, source-domain insertion, aggregate updates, and snapshots.
#[cfg(all(kani, feature = "closure"))]
fn prove_positive_kf_source_mapping<const ASSET: usize, const WINNER_LONG: bool>() {
    assert!(ASSET < 2);
    let winner_side = if WINNER_LONG {
        SideV16::Long
    } else {
        SideV16::Short
    };
    let (mut header, mut markets, mut account_header) = two_asset_kf_mapping_fixture();

    let delta = 2i128;
    let delta_num = delta as u128 * BOUND_SCALE;
    let k_target = delta * ADL_ONE as i128;
    let mut asset = markets[ASSET].engine.asset.try_to_runtime().unwrap();
    if WINNER_LONG {
        asset.k_long = k_target;
        asset.k_short = -k_target;
    } else {
        asset.k_short = k_target;
        asset.k_long = -k_target;
    }
    asset.oi_eff_long_q = POS_SCALE;
    asset.oi_eff_short_q = POS_SCALE;
    asset.stored_pos_count_long = 1;
    asset.stored_pos_count_short = 1;
    asset.loss_weight_sum_long = POS_SCALE;
    asset.loss_weight_sum_short = POS_SCALE;
    let market_id = asset.market_id;
    let epoch = match winner_side {
        SideV16::Long => asset.epoch_long,
        SideV16::Short => asset.epoch_short,
    };
    markets[ASSET].engine.asset = AssetStateV16Account::from_runtime(&asset);
    header.resolved_payout_blocker_count = V16PodU64::new(2);

    account_header.active_bitmap[0] = V16PodU64::new(1);
    account_header.legs[0] = PortfolioLegV16Account::from_runtime(&PortfolioLegV16 {
        active: true,
        asset_index: ASSET as u32,
        market_id,
        side: winner_side,
        basis_pos_q: if WINNER_LONG {
            POS_SCALE as i128
        } else {
            -(POS_SCALE as i128)
        },
        a_basis: ADL_ONE,
        epoch_snap: epoch,
        loss_weight: POS_SCALE,
        b_epoch_snap: epoch,
        ..PortfolioLegV16::EMPTY
    });

    let expected_domain = ASSET * 2 + usize::from(WINNER_LONG);
    let other = 1 - ASSET;
    let other_before = markets[other].engine;
    let selected_before = markets[ASSET].engine;
    let account_before = account_header;
    let header_before = header;

    let mut expected_header = header_before;
    expected_header.pnl_pos_tot = V16PodU128::new(delta as u128);
    expected_header.pnl_pos_bound_tot = V16PodU128::new(delta as u128);
    expected_header.pnl_pos_bound_tot_num = V16PodU128::new(delta_num);
    expected_header.source_claim_bound_total_num = V16PodU128::new(delta_num);
    expected_header.risk_epoch = V16PodU64::new(header_before.risk_epoch.get() + 1);

    let expected_source = SourceCreditStateV16Account::from_runtime(&SourceCreditStateV16 {
        positive_claim_bound_num: delta_num,
        exact_positive_claim_num: delta_num,
        credit_rate_num: 0,
        credit_epoch: 1,
        ..SourceCreditStateV16::EMPTY
    });
    let mut expected_selected = selected_before;
    if WINNER_LONG {
        expected_selected.source_credit_short = expected_source;
    } else {
        expected_selected.source_credit_long = expected_source;
    }

    let mut expected_source_domain = PortfolioSourceDomainV16Account::default();
    expected_source_domain.domain = V16PodU32::new(expected_domain as u32);
    expected_source_domain.source_claim_market_id = V16PodU64::new(market_id);
    expected_source_domain.source_claim_bound_num = V16PodU128::new(delta_num);
    let mut expected_leg = account_before.legs[0].try_to_runtime().unwrap();
    expected_leg.k_snap = k_target;
    expected_leg.f_snap = 0;
    let expected_leg = PortfolioLegV16Account::from_runtime(&expected_leg);
    let mut expected_account = account_before;
    expected_account.pnl = V16PodI128::new(delta);
    expected_account.source_domains[0] = expected_source_domain;
    expected_account.legs[0] = expected_leg;
    expected_account.health_cert.valid = 0;

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    let residual_before = market.residual();
    market
        .settle_leg_kf_effects_at_slot(&mut account, 0)
        .unwrap();

    kani::cover!(
        account.header.pnl.get() == delta,
        "production settlement reaches a positive source claim"
    );
    assert!(kani_eq_market_group_v16_header_account(
        &expected_header,
        market.header
    ));
    assert!(kani_eq_portfolio_account_v16_account(
        &expected_account,
        account.header
    ));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &expected_selected,
        &market.markets[ASSET].engine
    ));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &other_before,
        &market.markets[other].engine
    ));
    assert_eq!(market.residual(), residual_before);
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(64)]
#[kani::solver(cadical)]
#[kani::stub(crate::v16::scaled_adl_delta_fast, positive_two_scaled_adl_delta_stub)]
#[kani::stub(
    crate::v16::V16Core::expected_source_credit_rate_num_for_state,
    zero_backing_source_credit_rate_stub
)]
fn closure_asset_zero_long_profit_maps_only_to_short_source_domain() {
    prove_positive_kf_source_mapping::<0, true>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(64)]
#[kani::solver(cadical)]
#[kani::stub(crate::v16::scaled_adl_delta_fast, positive_two_scaled_adl_delta_stub)]
#[kani::stub(
    crate::v16::V16Core::expected_source_credit_rate_num_for_state,
    zero_backing_source_credit_rate_stub
)]
fn closure_asset_zero_short_profit_maps_only_to_long_source_domain() {
    prove_positive_kf_source_mapping::<0, false>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(64)]
#[kani::solver(cadical)]
#[kani::stub(crate::v16::scaled_adl_delta_fast, positive_two_scaled_adl_delta_stub)]
#[kani::stub(
    crate::v16::V16Core::expected_source_credit_rate_num_for_state,
    zero_backing_source_credit_rate_stub
)]
fn closure_asset_one_long_profit_maps_only_to_short_source_domain() {
    prove_positive_kf_source_mapping::<1, true>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(64)]
#[kani::solver(cadical)]
#[kani::stub(crate::v16::scaled_adl_delta_fast, positive_two_scaled_adl_delta_stub)]
#[kani::stub(
    crate::v16::V16Core::expected_source_credit_rate_num_for_state,
    zero_backing_source_credit_rate_stub
)]
fn closure_asset_one_short_profit_maps_only_to_long_source_domain() {
    prove_positive_kf_source_mapping::<1, false>();
}

// A losing K/F leg must crystallize its account capital into fresh backing for
// that exact asset/side. This is the value-moving mirror of positive source
// attribution: a wrong domain either leaks backing across assets or strands the
// intended winner. The scalar capital cap is independently proven; this theorem
// executes the real settlement, reservation, backing, and aggregate mutations.
#[cfg(all(kani, feature = "closure"))]
fn prove_negative_kf_backing_mapping<const ASSET: usize, const LOSER_LONG: bool>() {
    assert!(ASSET < 2);
    let loser_side = if LOSER_LONG {
        SideV16::Long
    } else {
        SideV16::Short
    };
    let (mut header, mut markets, mut account_header) = two_asset_kf_mapping_fixture();
    let loss = 2i128;
    let backing_num = loss as u128 * BOUND_SCALE;
    let k_target = -(loss * ADL_ONE as i128);
    let mut asset = markets[ASSET].engine.asset.try_to_runtime().unwrap();
    if LOSER_LONG {
        asset.k_long = k_target;
        asset.k_short = -k_target;
    } else {
        asset.k_short = k_target;
        asset.k_long = -k_target;
    }
    asset.oi_eff_long_q = POS_SCALE;
    asset.oi_eff_short_q = POS_SCALE;
    asset.stored_pos_count_long = 1;
    asset.stored_pos_count_short = 1;
    asset.loss_weight_sum_long = POS_SCALE;
    asset.loss_weight_sum_short = POS_SCALE;
    let market_id = asset.market_id;
    let epoch = match loser_side {
        SideV16::Long => asset.epoch_long,
        SideV16::Short => asset.epoch_short,
    };
    markets[ASSET].engine.asset = AssetStateV16Account::from_runtime(&asset);
    header.vault = V16PodU128::new(12);
    header.c_tot = V16PodU128::new(loss as u128);
    header.resolved_payout_blocker_count = V16PodU64::new(2);
    account_header.capital = V16PodU128::new(loss as u128);
    account_header.active_bitmap[0] = V16PodU64::new(1);
    account_header.legs[0] = PortfolioLegV16Account::from_runtime(&PortfolioLegV16 {
        active: true,
        asset_index: ASSET as u32,
        market_id,
        side: loser_side,
        basis_pos_q: if LOSER_LONG {
            POS_SCALE as i128
        } else {
            -(POS_SCALE as i128)
        },
        a_basis: ADL_ONE,
        epoch_snap: epoch,
        loss_weight: POS_SCALE,
        b_epoch_snap: epoch,
        ..PortfolioLegV16::EMPTY
    });

    let other = 1 - ASSET;
    let other_before = markets[other].engine;
    let selected_before = markets[ASSET].engine;
    let account_before = account_header;
    let header_before = header;
    let config = header.config.try_to_runtime_shape().unwrap();
    let freshness_horizon = config
        .max_accrual_dt_slots
        .max(config.h_max)
        .max(config.max_bankrupt_close_lifetime_slots)
        .max(1);
    let expiry_slot = header.current_slot.get() + freshness_horizon;

    let mut expected_header = header_before;
    expected_header.c_tot = V16PodU128::new(0);
    expected_header.source_fresh_backing_total_num = V16PodU128::new(backing_num);
    expected_header.risk_epoch = V16PodU64::new(header_before.risk_epoch.get() + 1);

    let expected_source = SourceCreditStateV16Account::from_runtime(&SourceCreditStateV16 {
        fresh_reserved_backing_num: backing_num,
        credit_epoch: 1,
        ..SourceCreditStateV16::EMPTY
    });
    let expected_bucket = BackingBucketV16Account::from_runtime(&BackingBucketV16 {
        market_id,
        fresh_unliened_backing_num: backing_num,
        expiry_slot,
        status: BackingBucketStatusV16::Fresh,
        ..BackingBucketV16::EMPTY
    });
    let mut expected_selected = selected_before;
    if LOSER_LONG {
        expected_selected.source_credit_long = expected_source;
        expected_selected.backing_long = expected_bucket;
    } else {
        expected_selected.source_credit_short = expected_source;
        expected_selected.backing_short = expected_bucket;
    }

    let mut expected_leg = account_before.legs[0].try_to_runtime().unwrap();
    expected_leg.k_snap = k_target;
    expected_leg.f_snap = 0;
    let mut expected_account = account_before;
    expected_account.capital = V16PodU128::new(0);
    expected_account.residual_crystallized_loss_atoms_total = V16PodU128::new(loss as u128);
    expected_account.legs[0] = PortfolioLegV16Account::from_runtime(&expected_leg);
    expected_account.health_cert.valid = 0;

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    let residual_before = market.residual();
    market
        .settle_leg_kf_effects_at_slot(&mut account, 0)
        .unwrap();

    kani::cover!(
        account.header.capital.get() == 0
            && account.header.residual_crystallized_loss_atoms_total.get() == loss as u128,
        "negative K/F settlement crystallizes real account capital"
    );
    assert!(kani_eq_market_group_v16_header_account(
        &expected_header,
        market.header
    ));
    assert!(kani_eq_portfolio_account_v16_account(
        &expected_account,
        account.header
    ));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &expected_selected,
        &market.markets[ASSET].engine
    ));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &other_before,
        &market.markets[other].engine
    ));
    assert_eq!(market.residual(), residual_before);
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(80)]
#[kani::solver(cadical)]
#[kani::stub(crate::v16::scaled_adl_delta_fast, negative_two_scaled_adl_delta_stub)]
#[kani::stub(
    crate::v16::V16Core::expected_source_credit_rate_num_for_state,
    no_claim_source_credit_rate_stub
)]
fn closure_asset_zero_long_loss_backs_only_long_source_domain() {
    prove_negative_kf_backing_mapping::<0, true>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(80)]
#[kani::solver(cadical)]
#[kani::stub(crate::v16::scaled_adl_delta_fast, negative_two_scaled_adl_delta_stub)]
#[kani::stub(
    crate::v16::V16Core::expected_source_credit_rate_num_for_state,
    no_claim_source_credit_rate_stub
)]
fn closure_asset_zero_short_loss_backs_only_short_source_domain() {
    prove_negative_kf_backing_mapping::<0, false>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(80)]
#[kani::solver(cadical)]
#[kani::stub(crate::v16::scaled_adl_delta_fast, negative_two_scaled_adl_delta_stub)]
#[kani::stub(
    crate::v16::V16Core::expected_source_credit_rate_num_for_state,
    no_claim_source_credit_rate_stub
)]
fn closure_asset_one_long_loss_backs_only_long_source_domain() {
    prove_negative_kf_backing_mapping::<1, true>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(80)]
#[kani::solver(cadical)]
#[kani::stub(crate::v16::scaled_adl_delta_fast, negative_two_scaled_adl_delta_stub)]
#[kani::stub(
    crate::v16::V16Core::expected_source_credit_rate_num_for_state,
    no_claim_source_credit_rate_stub
)]
fn closure_asset_one_short_loss_backs_only_short_source_domain() {
    prove_negative_kf_backing_mapping::<1, false>();
}

// A cross-asset account can settle an unsupported winner and a real loser in
// either leg order. The winner's oracle must not preserve account capital or
// route realized backing into its own (or either unrelated) domain. The only
// permitted real-value mutation is capital crystallized into the losing
// asset/side domain. If the loser is fully funded and settles first, the later
// unsupported winner may retain a source-attributed claim, but that claim has
// zero credit and remains confined to the winner's opposing-side domain.
#[cfg(all(kani, feature = "closure"))]
#[derive(Clone, Copy)]
struct CrossAssetSettlementWitness {
    initial_capital: u128,
    final_capital: u128,
    final_pnl: i128,
    crystallized_loss: u128,
    winner_claim_bound_num: u128,
}

#[cfg(all(kani, feature = "closure"))]
fn prove_cross_asset_unsupported_profit_cannot_move_loss<
    const WINNER_FIRST: bool,
    const MIN_CAPITAL: u8,
    const MAX_CAPITAL: u8,
>() -> CrossAssetSettlementWitness {
    assert!(MIN_CAPITAL <= MAX_CAPITAL);
    assert!(MAX_CAPITAL <= 4);
    let capital_raw: u8 = if MIN_CAPITAL == MAX_CAPITAL {
        MIN_CAPITAL
    } else {
        let symbolic: u8 = kani::any();
        kani::assume(symbolic >= MIN_CAPITAL && symbolic <= MAX_CAPITAL);
        symbolic
    };
    let capital = capital_raw as u128;
    let loss = 2u128;
    let backing = capital.min(loss);
    let backing_num = backing * BOUND_SCALE;
    let (mut header, mut markets, mut account_header) = two_asset_kf_mapping_fixture();

    header.vault = V16PodU128::new(10 + capital);
    header.c_tot = V16PodU128::new(capital);
    header.resolved_payout_blocker_count = V16PodU64::new(4);
    account_header.capital = V16PodU128::new(capital);
    account_header.active_bitmap[0] = V16PodU64::new(3);
    let mut leg_slot = 0usize;
    while leg_slot < V16_MAX_PORTFOLIO_ASSETS_N {
        account_header.legs[leg_slot] =
            PortfolioLegV16Account::from_runtime(&PortfolioLegV16::EMPTY);
        leg_slot += 1;
    }

    let winner_k = 2 * ADL_ONE as i128;
    let loser_k = -winner_k;
    let mut winner_asset = markets[0].engine.asset.try_to_runtime().unwrap();
    winner_asset.k_long = winner_k;
    winner_asset.k_short = loser_k;
    winner_asset.oi_eff_long_q = POS_SCALE;
    winner_asset.oi_eff_short_q = POS_SCALE;
    winner_asset.stored_pos_count_long = 1;
    winner_asset.stored_pos_count_short = 1;
    winner_asset.loss_weight_sum_long = POS_SCALE;
    winner_asset.loss_weight_sum_short = POS_SCALE;
    markets[0].engine.asset = AssetStateV16Account::from_runtime(&winner_asset);

    let mut loser_asset = markets[1].engine.asset.try_to_runtime().unwrap();
    loser_asset.k_long = loser_k;
    loser_asset.k_short = winner_k;
    loser_asset.oi_eff_long_q = POS_SCALE;
    loser_asset.oi_eff_short_q = POS_SCALE;
    loser_asset.stored_pos_count_long = 1;
    loser_asset.stored_pos_count_short = 1;
    loser_asset.loss_weight_sum_long = POS_SCALE;
    loser_asset.loss_weight_sum_short = POS_SCALE;
    markets[1].engine.asset = AssetStateV16Account::from_runtime(&loser_asset);

    let winner_slot = if WINNER_FIRST { 0 } else { 1 };
    let loser_slot = 1 - winner_slot;
    account_header.legs[winner_slot] = PortfolioLegV16Account::from_runtime(&PortfolioLegV16 {
        active: true,
        asset_index: 0,
        market_id: winner_asset.market_id,
        side: SideV16::Long,
        basis_pos_q: POS_SCALE as i128,
        a_basis: ADL_ONE,
        epoch_snap: winner_asset.epoch_long,
        loss_weight: POS_SCALE,
        b_epoch_snap: winner_asset.epoch_long,
        ..PortfolioLegV16::EMPTY
    });
    account_header.legs[loser_slot] = PortfolioLegV16Account::from_runtime(&PortfolioLegV16 {
        active: true,
        asset_index: 1,
        market_id: loser_asset.market_id,
        side: SideV16::Long,
        basis_pos_q: POS_SCALE as i128,
        a_basis: ADL_ONE,
        epoch_snap: loser_asset.epoch_long,
        loss_weight: POS_SCALE,
        b_epoch_snap: loser_asset.epoch_long,
        ..PortfolioLegV16::EMPTY
    });

    let header_before = header;
    let markets_before = markets;
    let account_before = account_header;
    let winner_claim = if !WINNER_FIRST && backing == loss {
        loss
    } else {
        0
    };
    let final_pnl = if winner_claim != 0 {
        winner_claim as i128
    } else {
        -((loss - backing) as i128)
    };

    let mut expected_header = header_before;
    expected_header.c_tot = V16PodU128::new(capital - backing);
    expected_header.pnl_pos_tot = V16PodU128::new(winner_claim);
    expected_header.pnl_pos_bound_tot = V16PodU128::new(winner_claim);
    expected_header.pnl_pos_bound_tot_num = V16PodU128::new(winner_claim * BOUND_SCALE);
    expected_header.source_claim_bound_total_num = V16PodU128::new(winner_claim * BOUND_SCALE);
    expected_header.source_fresh_backing_total_num = V16PodU128::new(backing_num);
    expected_header.negative_pnl_account_count = V16PodU64::new(u64::from(final_pnl < 0));
    let risk_mutations = if WINNER_FIRST {
        2 + u64::from(backing != 0)
    } else {
        u64::from(backing != 0) + u64::from(winner_claim != 0)
    };
    expected_header.risk_epoch = V16PodU64::new(header_before.risk_epoch.get() + risk_mutations);

    let mut expected_markets = markets_before;
    expected_markets[0].engine.source_credit_short = if WINNER_FIRST {
        SourceCreditStateV16Account::from_runtime(&SourceCreditStateV16 {
            credit_epoch: 2,
            ..SourceCreditStateV16::EMPTY
        })
    } else if winner_claim != 0 {
        SourceCreditStateV16Account::from_runtime(&SourceCreditStateV16 {
            positive_claim_bound_num: winner_claim * BOUND_SCALE,
            exact_positive_claim_num: winner_claim * BOUND_SCALE,
            credit_rate_num: 0,
            credit_epoch: 1,
            ..SourceCreditStateV16::EMPTY
        })
    } else {
        SourceCreditStateV16Account::from_runtime(&SourceCreditStateV16::EMPTY)
    };
    if backing != 0 {
        let config = header_before.config.try_to_runtime_shape().unwrap();
        let freshness_horizon = config
            .max_accrual_dt_slots
            .max(config.h_max)
            .max(config.max_bankrupt_close_lifetime_slots)
            .max(1);
        expected_markets[1].engine.source_credit_long =
            SourceCreditStateV16Account::from_runtime(&SourceCreditStateV16 {
                fresh_reserved_backing_num: backing_num,
                credit_epoch: 1,
                ..SourceCreditStateV16::EMPTY
            });
        expected_markets[1].engine.backing_long =
            BackingBucketV16Account::from_runtime(&BackingBucketV16 {
                market_id: loser_asset.market_id,
                fresh_unliened_backing_num: backing_num,
                expiry_slot: header_before.current_slot.get() + freshness_horizon,
                status: BackingBucketStatusV16::Fresh,
                ..BackingBucketV16::EMPTY
            });
    }

    let mut expected_account = account_before;
    expected_account.capital = V16PodU128::new(capital - backing);
    expected_account.pnl = V16PodI128::new(final_pnl);
    expected_account.residual_crystallized_loss_atoms_total = V16PodU128::new(backing);
    let mut expected_winner_leg = expected_account.legs[winner_slot].try_to_runtime().unwrap();
    expected_winner_leg.k_snap = winner_k;
    expected_winner_leg.f_snap = 0;
    expected_account.legs[winner_slot] = PortfolioLegV16Account::from_runtime(&expected_winner_leg);
    let mut expected_loser_leg = expected_account.legs[loser_slot].try_to_runtime().unwrap();
    expected_loser_leg.k_snap = loser_k;
    expected_loser_leg.f_snap = 0;
    expected_account.legs[loser_slot] = PortfolioLegV16Account::from_runtime(&expected_loser_leg);
    if winner_claim != 0 {
        expected_account.source_domains[0].domain = V16PodU32::new(1);
        expected_account.source_domains[0].source_claim_market_id =
            V16PodU64::new(winner_asset.market_id);
        expected_account.source_domains[0].source_claim_bound_num =
            V16PodU128::new(winner_claim * BOUND_SCALE);
    }
    expected_account.health_cert.valid = 0;

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    let residual_before = market.residual();
    market
        .settle_leg_kf_effects_at_slot(&mut account, 0)
        .unwrap();
    market
        .settle_leg_kf_effects_at_slot(&mut account, 1)
        .unwrap();

    kani::cover!(
        account.header.legs[winner_slot].k_snap.get() == winner_k
            && account.header.legs[loser_slot].k_snap.get() == loser_k
            && market.header.vault.get() == header_before.vault.get(),
        "cross-asset two-leg settlement completes without moving vault value"
    );
    assert!(kani_eq_market_group_v16_header_account(
        &expected_header,
        market.header
    ));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &expected_markets[0].engine,
        &market.markets[0].engine
    ));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &expected_markets[1].engine,
        &market.markets[1].engine
    ));
    assert!(kani_eq_portfolio_account_v16_account(
        &expected_account,
        account.header
    ));
    assert_eq!(market.header.vault.get(), header_before.vault.get());
    assert_eq!(market.header.insurance.get(), header_before.insurance.get());
    assert_eq!(market.residual(), residual_before);
    CrossAssetSettlementWitness {
        initial_capital: capital,
        final_capital: account.header.capital.get(),
        final_pnl: account.header.pnl.get(),
        crystallized_loss: account.header.residual_crystallized_loss_atoms_total.get(),
        winner_claim_bound_num: account.header.source_domains[0]
            .source_claim_bound_num
            .get(),
    }
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(80)]
#[kani::solver(cadical)]
#[kani::stub(crate::v16::scaled_adl_delta_fast, signed_two_scaled_adl_delta_stub)]
#[kani::stub(
    crate::v16::V16Core::expected_source_credit_rate_num_for_state,
    cross_asset_source_credit_rate_stub
)]
#[kani::stub(
    PortfolioV16ViewMut::compact_source_domains,
    empty_cross_asset_compact_stub
)]
#[kani::stub(PortfolioV16View::validate_with_market, valid_conversion_account_stub)]
fn closure_unsupported_cross_asset_profit_cannot_move_loss_when_winner_settles_first_explicit_coverage(
) {
    let witness = prove_cross_asset_unsupported_profit_cannot_move_loss::<true, 0, 4>();
    kani::cover!(
        witness.initial_capital == 0 && witness.final_capital == 0 && witness.final_pnl == -2,
        "winner-first settlement covers a fully bankrupt loser"
    );
    kani::cover!(
        witness.initial_capital == 1 && witness.final_capital == 0 && witness.final_pnl == -1,
        "winner-first settlement covers partial principal crystallization"
    );
    kani::cover!(
        witness.initial_capital >= 2
            && witness.final_capital == witness.initial_capital - 2
            && witness.crystallized_loss == 2,
        "winner-first settlement covers a fully principal-backed loss"
    );
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(80)]
#[kani::solver(cadical)]
#[kani::stub(crate::v16::scaled_adl_delta_fast, signed_two_scaled_adl_delta_stub)]
#[kani::stub(
    crate::v16::V16Core::expected_source_credit_rate_num_for_state,
    cross_asset_source_credit_rate_stub
)]
#[kani::stub(
    PortfolioV16ViewMut::compact_source_domains,
    empty_cross_asset_compact_stub
)]
#[kani::stub(PortfolioV16View::validate_with_market, valid_conversion_account_stub)]
#[kani::stub(
    MarketGroupV16ViewMut::source_domain_realizable_support_for_face,
    empty_cross_asset_source_support_stub
)]
fn closure_unsupported_cross_asset_profit_cannot_move_loss_when_loser_settles_first_bankrupt_explicit_coverage(
) {
    let witness = prove_cross_asset_unsupported_profit_cannot_move_loss::<false, 0, 0>();
    kani::cover!(
        witness.initial_capital == 0 && witness.final_capital == 0 && witness.final_pnl == -2,
        "loser-first settlement covers a fully bankrupt loser"
    );
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(80)]
#[kani::solver(cadical)]
#[kani::stub(crate::v16::scaled_adl_delta_fast, signed_two_scaled_adl_delta_stub)]
#[kani::stub(
    crate::v16::V16Core::expected_source_credit_rate_num_for_state,
    cross_asset_source_credit_rate_stub
)]
#[kani::stub(
    PortfolioV16ViewMut::compact_source_domains,
    empty_cross_asset_compact_stub
)]
#[kani::stub(PortfolioV16View::validate_with_market, valid_conversion_account_stub)]
#[kani::stub(
    MarketGroupV16ViewMut::source_domain_realizable_support_for_face,
    empty_cross_asset_source_support_stub
)]
fn closure_unsupported_cross_asset_profit_cannot_move_loss_when_loser_settles_first_partially_funded_explicit_coverage(
) {
    let witness = prove_cross_asset_unsupported_profit_cannot_move_loss::<false, 1, 1>();
    kani::cover!(
        witness.initial_capital == 1 && witness.final_capital == 0 && witness.final_pnl == -1,
        "loser-first settlement covers partial principal crystallization"
    );
}

#[cfg(all(kani, feature = "closure"))]
fn prove_cross_asset_loser_first_funded<const CAPITAL: u8>() {
    assert!(CAPITAL >= 2 && CAPITAL <= 4);
    let witness =
        prove_cross_asset_unsupported_profit_cannot_move_loss::<false, CAPITAL, CAPITAL>();
    kani::cover!(
        witness.final_capital == witness.initial_capital - 2 && witness.crystallized_loss == 2,
        "loser-first settlement covers a fully principal-backed loss"
    );
    kani::cover!(
        witness.final_pnl == 2 && witness.winner_claim_bound_num == 2 * BOUND_SCALE,
        "loser-first settlement covers a retained zero-credit winner claim"
    );
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(80)]
#[kani::solver(cadical)]
#[kani::stub(crate::v16::scaled_adl_delta_fast, signed_two_scaled_adl_delta_stub)]
#[kani::stub(
    crate::v16::V16Core::expected_source_credit_rate_num_for_state,
    cross_asset_source_credit_rate_stub
)]
#[kani::stub(
    PortfolioV16ViewMut::compact_source_domains,
    empty_cross_asset_compact_stub
)]
#[kani::stub(PortfolioV16View::validate_with_market, valid_conversion_account_stub)]
fn closure_unsupported_cross_asset_profit_cannot_move_loss_when_loser_settles_first_at_loss_boundary_explicit_coverage(
) {
    prove_cross_asset_loser_first_funded::<2>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(80)]
#[kani::solver(cadical)]
#[kani::stub(crate::v16::scaled_adl_delta_fast, signed_two_scaled_adl_delta_stub)]
#[kani::stub(
    crate::v16::V16Core::expected_source_credit_rate_num_for_state,
    cross_asset_source_credit_rate_stub
)]
#[kani::stub(
    PortfolioV16ViewMut::compact_source_domains,
    empty_cross_asset_compact_stub
)]
#[kani::stub(PortfolioV16View::validate_with_market, valid_conversion_account_stub)]
fn closure_unsupported_cross_asset_profit_cannot_move_loss_when_loser_settles_first_with_one_surplus_atom_explicit_coverage(
) {
    prove_cross_asset_loser_first_funded::<3>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(80)]
#[kani::solver(cadical)]
#[kani::stub(crate::v16::scaled_adl_delta_fast, signed_two_scaled_adl_delta_stub)]
#[kani::stub(
    crate::v16::V16Core::expected_source_credit_rate_num_for_state,
    cross_asset_source_credit_rate_stub
)]
#[kani::stub(
    PortfolioV16ViewMut::compact_source_domains,
    empty_cross_asset_compact_stub
)]
#[kani::stub(PortfolioV16View::validate_with_market, valid_conversion_account_stub)]
fn closure_unsupported_cross_asset_profit_cannot_move_loss_when_loser_settles_first_with_two_surplus_atoms_explicit_coverage(
) {
    prove_cross_asset_loser_first_funded::<4>();
}

#[cfg(all(kani, feature = "closure"))]
fn current_empty_trade_refresh_stub<'a: 'a, T>(
    market: &mut MarketGroupV16ViewMut<'a, T>,
    account: &mut PortfolioV16ViewMut<'_>,
) -> V16Result<HealthCertV16> {
    assert_eq!(account.header.stale_state, 0);
    assert_eq!(account.header.b_stale_state, 0);
    assert_eq!(account.header.active_bitmap[0].get(), 0);
    assert_eq!(account.header.pnl.get(), 0);
    assert_eq!(account.header.fee_credits.get(), 0);
    let cert = account.header.health_cert.try_to_runtime()?;
    assert!(cert.valid);
    assert_eq!(cert.certified_equity, account.header.capital.get() as i128);
    assert_eq!(cert.certified_initial_req, 0);
    assert_eq!(cert.certified_maintenance_req, 0);
    assert_eq!(cert.certified_liq_deficit, 0);
    assert_eq!(cert.certified_worst_case_loss, 0);
    assert_eq!(cert.cert_oracle_epoch, market.header.oracle_epoch.get());
    assert_eq!(cert.cert_funding_epoch, market.header.funding_epoch.get());
    assert_eq!(cert.cert_risk_epoch, market.header.risk_epoch.get());
    assert_eq!(
        cert.cert_asset_set_epoch,
        market.header.asset_set_epoch.get()
    );
    assert_eq!(cert.active_bitmap_at_cert, [0; V16_ACTIVE_BITMAP_WORDS]);
    Ok(cert)
}

#[cfg(all(kani, feature = "closure"))]
fn scoped_trade_risk_notional_stub(abs_pos_q: u128, price: u64) -> V16Result<u128> {
    assert!(abs_pos_q == 0 || abs_pos_q == POS_SCALE);
    assert_eq!(price, 100);
    Ok(if abs_pos_q == 0 { 0 } else { 100 })
}

#[cfg(all(kani, feature = "closure"))]
fn scoped_trade_notional_floor_stub(size_q: u128, exec_price: u64) -> V16Result<u128> {
    assert_eq!(size_q, POS_SCALE);
    assert_eq!(exec_price, 100);
    Ok(100)
}

#[cfg(all(kani, feature = "closure"))]
fn scoped_trade_fee_notional_stub(size_q: u128, exec_price: u64) -> V16Result<u128> {
    scoped_trade_notional_floor_stub(size_q, exec_price)
}

#[cfg(all(kani, feature = "closure"))]
fn scoped_trade_fee_stub(notional: u128, fee_bps: u64) -> V16Result<u128> {
    assert_eq!(notional, 100);
    assert_eq!(fee_bps, 0);
    Ok(0)
}

#[cfg(all(kani, feature = "closure"))]
fn scoped_trade_margin_stub(notional: u128, bps: u64, floor: u128) -> V16Result<u128> {
    assert!(notional == 0 || notional == 100);
    assert_eq!(bps, 10_000);
    assert!(floor == 1 || floor == 2);
    Ok(notional)
}

#[cfg(all(kani, feature = "closure"))]
fn unreachable_empty_trade_source_lien_stub<'a: 'a, T>(
    _market: &mut MarketGroupV16ViewMut<'a, T>,
    account: &mut PortfolioV16ViewMut<'_>,
) -> V16Result<()> {
    assert_eq!(account.header.pnl.get(), 0);
    panic!("claim-free scoped trade attempted to create a source lien")
}

#[cfg(all(kani, feature = "closure"))]
fn scoped_trade_recertify_stub<'a: 'a, T>(
    market: &MarketGroupV16ViewMut<'a, T>,
    account: &mut PortfolioV16ViewMut<'_>,
    asset_index: usize,
    old_abs_q: u128,
    new_abs_q: u128,
    price: u64,
) -> V16Result<HealthCertV16> {
    assert_eq!(asset_index, 0);
    assert_eq!(old_abs_q, 0);
    assert_eq!(new_abs_q, POS_SCALE);
    assert_eq!(price, 100);
    assert!((100..=102).contains(&account.header.capital.get()));
    assert_eq!(account.header.pnl.get(), 0);
    assert_eq!(account.header.fee_credits.get(), 0);
    assert_eq!(account.header.active_bitmap[0].get(), 1);
    let prior = account.header.health_cert.try_to_runtime()?;
    assert!(!prior.valid);
    assert_eq!(prior.certified_initial_req, 0);
    assert_eq!(prior.certified_maintenance_req, 0);
    assert_eq!(prior.certified_worst_case_loss, 0);
    let cert = HealthCertV16 {
        certified_equity: account.header.capital.get() as i128,
        certified_initial_req: 100,
        certified_maintenance_req: 100,
        certified_liq_deficit: 0,
        certified_worst_case_loss: 100,
        cert_oracle_epoch: market.header.oracle_epoch.get(),
        cert_funding_epoch: market.header.funding_epoch.get(),
        cert_risk_epoch: market.header.risk_epoch.get(),
        cert_asset_set_epoch: market.header.asset_set_epoch.get(),
        active_bitmap_at_cert: [1; V16_ACTIVE_BITMAP_WORDS],
        valid: true,
    };
    account.header.health_cert = HealthCertV16Account::from_runtime(&cert);
    Ok(cert)
}

// Wrapper-critical scoped trade closure. Asset 1 has real balanced open risk
// and is one slot loss-stale; asset 0 is current. The public scoped API must
// still admit an economically valid asset-0 trade, restore the global stale
// summary, frame asset 1 exactly, preserve every value stock, and create only
// matched opposite asset-0 exposure. Direction and the funded IM boundary are
// symbolic so this is not a fixed trace. The two directions are separate
// harnesses to avoid duplicating the full production control-flow graph.
#[cfg(all(kani, feature = "closure"))]
fn prove_scoped_trade_is_live_and_isolated_from_unrelated_loss_stale_asset<
    const FIRST_ACCOUNT_LONG: bool,
>() {
    let capital_raw: u8 = kani::any();
    kani::assume((100..=102).contains(&capital_raw));
    let capital = capital_raw as u128;
    let signed_size_q = if FIRST_ACCOUNT_LONG {
        POS_SCALE as i128
    } else {
        -(POS_SCALE as i128)
    };

    let (mut header, mut markets, _) = two_asset_kf_mapping_fixture();
    header.loss_stale_active = 1;
    header.c_tot = V16PodU128::new(2 * capital);
    header.vault = V16PodU128::new(2 * capital + header.insurance.get());
    header.resolved_payout_blocker_count = V16PodU64::new(2);

    let mut current_asset = markets[0].engine.asset.try_to_runtime().unwrap();
    current_asset.slot_last = header.current_slot.get();
    markets[0].engine.asset = AssetStateV16Account::from_runtime(&current_asset);
    let mut stale_asset = markets[1].engine.asset.try_to_runtime().unwrap();
    stale_asset.slot_last = header.current_slot.get() - 1;
    stale_asset.oi_eff_long_q = POS_SCALE;
    stale_asset.oi_eff_short_q = POS_SCALE;
    stale_asset.stored_pos_count_long = 1;
    stale_asset.stored_pos_count_short = 1;
    stale_asset.loss_weight_sum_long = POS_SCALE;
    stale_asset.loss_weight_sum_short = POS_SCALE;
    markets[1].engine.asset = AssetStateV16Account::from_runtime(&stale_asset);

    let current_empty_cert = |owner: [u8; 32], account_id: [u8; 32]| {
        let provenance = ProvenanceHeaderV16Account::from_runtime(&ProvenanceHeaderV16::new(
            header.market_group_id,
            account_id,
            owner,
        ));
        let mut account = PortfolioAccountV16Account::default();
        account.init_empty_in_place(provenance).unwrap();
        account.capital = V16PodU128::new(capital);
        account.health_cert = HealthCertV16Account::from_runtime(&HealthCertV16 {
            certified_equity: capital as i128,
            certified_initial_req: 0,
            certified_maintenance_req: 0,
            certified_liq_deficit: 0,
            certified_worst_case_loss: 0,
            cert_oracle_epoch: header.oracle_epoch.get(),
            cert_funding_epoch: header.funding_epoch.get(),
            cert_risk_epoch: header.risk_epoch.get(),
            cert_asset_set_epoch: header.asset_set_epoch.get(),
            active_bitmap_at_cert: [0; V16_ACTIVE_BITMAP_WORDS],
            valid: true,
        });
        account
    };
    let mut first_header = current_empty_cert([3u8; 32], [4u8; 32]);
    let mut second_header = current_empty_cert([5u8; 32], [6u8; 32]);

    let header_before = header;
    let markets_before = markets;
    let first_before = first_header;
    let second_before = second_header;
    let request = TradeRequestV16 {
        asset_index: 0,
        size_q: signed_size_q,
        exec_price: current_asset.effective_price,
        fee_bps: 0,
    };

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut first = PortfolioV16ViewMut::new(&mut first_header);
    let mut second = PortfolioV16ViewMut::new(&mut second_header);
    let outcome = market
        .execute_trade_with_fee_loss_stale_scoped_not_atomic(&mut first, &mut second, request)
        .unwrap();

    kani::cover!(capital == 100, "scoped trade covers the exact IM boundary");
    kani::cover!(capital > 100, "scoped trade covers surplus capital");
    assert_eq!(
        outcome,
        TradeOutcomeV16 {
            fee_a: 0,
            fee_b: 0,
            notional: 100,
        }
    );

    let mut expected_header = header_before;
    expected_header.resolved_payout_blocker_count = V16PodU64::new(4);
    assert!(kani_eq_market_group_v16_header_account(
        &expected_header,
        market.header
    ));

    let mut expected_current_slot = markets_before[0].engine;
    let mut expected_current_asset = current_asset;
    expected_current_asset.oi_eff_long_q = POS_SCALE;
    expected_current_asset.oi_eff_short_q = POS_SCALE;
    expected_current_asset.stored_pos_count_long = 1;
    expected_current_asset.stored_pos_count_short = 1;
    expected_current_asset.loss_weight_sum_long = POS_SCALE;
    expected_current_asset.loss_weight_sum_short = POS_SCALE;
    expected_current_slot.asset = AssetStateV16Account::from_runtime(&expected_current_asset);
    assert!(kani_eq_engine_asset_slot_v16_account(
        &expected_current_slot,
        &market.markets[0].engine
    ));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &markets_before[1].engine,
        &market.markets[1].engine
    ));
    assert_eq!(market.markets[0].wrapper, markets_before[0].wrapper);
    assert_eq!(market.markets[1].wrapper, markets_before[1].wrapper);

    let first_side = if FIRST_ACCOUNT_LONG {
        SideV16::Long
    } else {
        SideV16::Short
    };
    let second_side = opposite_side(first_side);
    let expected_account =
        |before: PortfolioAccountV16Account, side: SideV16, basis_pos_q: i128| {
            let mut account = before;
            account.active_bitmap[0] = V16PodU64::new(1);
            account.legs[0] = PortfolioLegV16Account::from_runtime(&PortfolioLegV16 {
                active: true,
                asset_index: 0,
                market_id: current_asset.market_id,
                side,
                basis_pos_q,
                a_basis: ADL_ONE,
                k_snap: 0,
                f_snap: 0,
                epoch_snap: 0,
                loss_weight: POS_SCALE,
                b_snap: 0,
                b_rem: 0,
                b_epoch_snap: 0,
                b_stale: false,
                stale: false,
            });
            account.health_cert = HealthCertV16Account::from_runtime(&HealthCertV16 {
                certified_equity: capital as i128,
                certified_initial_req: 100,
                certified_maintenance_req: 100,
                certified_liq_deficit: 0,
                certified_worst_case_loss: 100,
                cert_oracle_epoch: header_before.oracle_epoch.get(),
                cert_funding_epoch: header_before.funding_epoch.get(),
                cert_risk_epoch: header_before.risk_epoch.get(),
                cert_asset_set_epoch: header_before.asset_set_epoch.get(),
                active_bitmap_at_cert: [1; V16_ACTIVE_BITMAP_WORDS],
                valid: true,
            });
            account
        };
    let expected_first = expected_account(first_before, first_side, signed_size_q);
    let expected_second = expected_account(second_before, second_side, -signed_size_q);
    assert!(kani_eq_portfolio_account_v16_account(
        &expected_first,
        first.header
    ));
    assert!(kani_eq_portfolio_account_v16_account(
        &expected_second,
        second.header
    ));

    assert_eq!(market.header.loss_stale_active, 1);
    assert_eq!(market.header.vault, header_before.vault);
    assert_eq!(market.header.c_tot, header_before.c_tot);
    assert_eq!(market.header.insurance, header_before.insurance);
    assert_eq!(first.header.capital, first_before.capital);
    assert_eq!(second.header.capital, second_before.capital);
    assert_eq!(first.header.pnl.get(), 0);
    assert_eq!(second.header.pnl.get(), 0);
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(96)]
#[kani::solver(cadical)]
#[kani::stub(crate::v16::loss_weight_for_basis, one_position_loss_weight_stub)]
#[kani::stub(
    MarketGroupV16ViewMut::settle_account_for_position_action_and_refresh_not_atomic,
    current_empty_trade_refresh_stub
)]
#[kani::stub(crate::v16::risk_notional_ceil, scoped_trade_risk_notional_stub)]
#[kani::stub(crate::v16::trade_notional_floor, scoped_trade_notional_floor_stub)]
#[kani::stub(crate::v16::trade_fee_notional_ceil, scoped_trade_fee_notional_stub)]
#[kani::stub(crate::v16::checked_fee_bps, scoped_trade_fee_stub)]
#[kani::stub(crate::v16::margin_requirement, scoped_trade_margin_stub)]
#[kani::stub(
    MarketGroupV16ViewMut::create_initial_margin_source_lien_if_needed,
    unreachable_empty_trade_source_lien_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::recertify_account_after_trade_delta,
    scoped_trade_recertify_stub
)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, valid_conversion_market_stub)]
#[kani::stub(PortfolioV16View::validate_with_market, valid_conversion_account_stub)]
fn closure_scoped_trade_long_is_live_and_isolated_from_unrelated_loss_stale_asset_explicit_coverage(
) {
    prove_scoped_trade_is_live_and_isolated_from_unrelated_loss_stale_asset::<true>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(96)]
#[kani::solver(cadical)]
#[kani::stub(crate::v16::loss_weight_for_basis, one_position_loss_weight_stub)]
#[kani::stub(
    MarketGroupV16ViewMut::settle_account_for_position_action_and_refresh_not_atomic,
    current_empty_trade_refresh_stub
)]
#[kani::stub(crate::v16::risk_notional_ceil, scoped_trade_risk_notional_stub)]
#[kani::stub(crate::v16::trade_notional_floor, scoped_trade_notional_floor_stub)]
#[kani::stub(crate::v16::trade_fee_notional_ceil, scoped_trade_fee_notional_stub)]
#[kani::stub(crate::v16::checked_fee_bps, scoped_trade_fee_stub)]
#[kani::stub(crate::v16::margin_requirement, scoped_trade_margin_stub)]
#[kani::stub(
    MarketGroupV16ViewMut::create_initial_margin_source_lien_if_needed,
    unreachable_empty_trade_source_lien_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::recertify_account_after_trade_delta,
    scoped_trade_recertify_stub
)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, valid_conversion_market_stub)]
#[kani::stub(PortfolioV16View::validate_with_market, valid_conversion_account_stub)]
fn closure_scoped_trade_short_is_live_and_isolated_from_unrelated_loss_stale_asset_explicit_coverage(
) {
    prove_scoped_trade_is_live_and_isolated_from_unrelated_loss_stale_asset::<false>();
}

#[cfg(all(kani, feature = "closure"))]
// Assertion-heavy seam for the separately proven single-fill body. It applies
// kernel_attach_leg's exact postcondition and the public fee proof's exact
// one-atom transfer, leaving this theorem to verify two-fill sequencing.
fn two_asset_batch_apply_stub<'a: 'a, T>(
    market: &mut MarketGroupV16ViewMut<'a, T>,
    long_account: &mut PortfolioV16ViewMut<'_>,
    short_account: &mut PortfolioV16ViewMut<'_>,
    request: TradeRequestV16,
    recertify_after_fill: bool,
) -> V16Result<TradeApplyOutcomeV16> {
    assert!(!recertify_after_fill);
    assert!(request.asset_index < 2);
    assert_eq!(request.size_q.unsigned_abs(), POS_SCALE);
    assert_eq!(request.exec_price, 100);
    assert_eq!(request.fee_bps, 100);
    let mut asset = market.asset_state(request.asset_index)?;
    assert_eq!(asset.effective_price, 100);
    assert_eq!(asset.slot_last, market.header.current_slot.get());
    assert_eq!(asset.oi_eff_long_q, 0);
    assert_eq!(asset.oi_eff_short_q, 0);
    let bitmap = long_account.header.active_bitmap[0].get();
    assert_eq!(bitmap, short_account.header.active_bitmap[0].get());
    let leg_slot = if bitmap == 0 {
        0
    } else {
        assert_eq!(bitmap, 1);
        let long_leg = long_account.header.legs[0].try_to_runtime()?;
        let short_leg = short_account.header.legs[0].try_to_runtime()?;
        assert!(long_leg.active && short_leg.active);
        assert_ne!(long_leg.asset_index as usize, request.asset_index);
        assert_ne!(short_leg.asset_index as usize, request.asset_index);
        1
    };
    let short_size = request
        .size_q
        .checked_neg()
        .ok_or(V16Error::ArithmeticOverflow)?;

    let long_side = if request.size_q > 0 {
        SideV16::Long
    } else {
        SideV16::Short
    };
    let short_side = opposite_side(long_side);
    assert_eq!(asset.a_long, ADL_ONE);
    assert_eq!(asset.a_short, ADL_ONE);
    let long_leg = PortfolioLegV16 {
        active: true,
        asset_index: request.asset_index as u32,
        market_id: asset.market_id,
        side: long_side,
        basis_pos_q: request.size_q,
        a_basis: ADL_ONE,
        loss_weight: POS_SCALE,
        ..PortfolioLegV16::EMPTY
    };
    let short_leg = PortfolioLegV16 {
        active: true,
        asset_index: request.asset_index as u32,
        market_id: asset.market_id,
        side: short_side,
        basis_pos_q: short_size,
        a_basis: ADL_ONE,
        loss_weight: POS_SCALE,
        ..PortfolioLegV16::EMPTY
    };
    asset.oi_eff_long_q = POS_SCALE;
    asset.oi_eff_short_q = POS_SCALE;
    asset.stored_pos_count_long = 1;
    asset.stored_pos_count_short = 1;
    asset.loss_weight_sum_long = POS_SCALE;
    asset.loss_weight_sum_short = POS_SCALE;
    long_account.header.legs[leg_slot] = PortfolioLegV16Account::from_runtime(&long_leg);
    short_account.header.legs[leg_slot] = PortfolioLegV16Account::from_runtime(&short_leg);
    let mut long_bitmap = long_account.header.active_bitmap.map(V16PodU64::get);
    let mut short_bitmap = short_account.header.active_bitmap.map(V16PodU64::get);
    active_bitmap_set(&mut long_bitmap, leg_slot)?;
    active_bitmap_set(&mut short_bitmap, leg_slot)?;
    long_account.header.active_bitmap = long_bitmap.map(V16PodU64::new);
    short_account.header.active_bitmap = short_bitmap.map(V16PodU64::new);

    long_account.header.capital = V16PodU128::new(
        long_account
            .header
            .capital
            .get()
            .checked_sub(1)
            .ok_or(V16Error::CounterUnderflow)?,
    );
    short_account.header.capital = V16PodU128::new(
        short_account
            .header
            .capital
            .get()
            .checked_sub(1)
            .ok_or(V16Error::CounterUnderflow)?,
    );
    market.header.c_tot = V16PodU128::new(
        market
            .header
            .c_tot
            .get()
            .checked_sub(2)
            .ok_or(V16Error::CounterUnderflow)?,
    );
    market.header.insurance = V16PodU128::new(
        market
            .header
            .insurance
            .get()
            .checked_add(2)
            .ok_or(V16Error::ArithmeticOverflow)?,
    );
    long_account.header.health_cert.valid = 0;
    short_account.header.health_cert.valid = 0;
    market.set_asset_state(request.asset_index, asset)?;

    Ok(TradeApplyOutcomeV16 {
        fee_a: 1,
        fee_b: 1,
        notional: 100,
        risk_increasing: true,
        long_has_source_claims: false,
        short_has_source_claims: false,
    })
}

#[cfg(all(kani, feature = "closure"))]
fn two_asset_batch_certify_stub<'a: 'a, T>(
    market: &mut MarketGroupV16ViewMut<'a, T>,
    account: &mut PortfolioV16ViewMut<'_>,
    price_override: Option<(usize, u64)>,
) -> V16Result<HealthCertV16> {
    assert_eq!(price_override, None);
    assert_eq!(account.header.active_bitmap[0].get(), 3);
    assert!((200..=202).contains(&account.header.capital.get()));
    assert_eq!(account.header.pnl.get(), 0);
    assert_eq!(account.header.fee_credits.get(), 0);
    let first = account.header.legs[0].try_to_runtime()?;
    let second = account.header.legs[1].try_to_runtime()?;
    assert!(first.active && second.active);
    assert_ne!(first.asset_index, second.asset_index);
    assert!(first.asset_index < 2 && second.asset_index < 2);
    assert_eq!(first.basis_pos_q.unsigned_abs(), POS_SCALE);
    assert_eq!(second.basis_pos_q.unsigned_abs(), POS_SCALE);
    let prior = account.header.health_cert.try_to_runtime()?;
    assert!(!prior.valid);
    let cert = HealthCertV16 {
        certified_equity: account.header.capital.get() as i128,
        certified_initial_req: 200,
        certified_maintenance_req: 200,
        certified_liq_deficit: 0,
        certified_worst_case_loss: 200,
        cert_oracle_epoch: market.header.oracle_epoch.get(),
        cert_funding_epoch: market.header.funding_epoch.get(),
        cert_risk_epoch: market.header.risk_epoch.get(),
        cert_asset_set_epoch: market.header.asset_set_epoch.get(),
        active_bitmap_at_cert: [3; V16_ACTIVE_BITMAP_WORDS],
        valid: true,
    };
    account.header.health_cert = HealthCertV16Account::from_runtime(&cert);
    Ok(cert)
}

#[cfg(all(kani, feature = "closure"))]
fn unlocked_two_asset_batch_h_lock_stub<'a: 'a, T>(
    market: &MarketGroupV16ViewMut<'a, T>,
    account: Option<&PortfolioV16View<'_>>,
    instruction_bankruptcy_candidate: bool,
) -> V16Result<HLockLaneV16> {
    assert!(!instruction_bankruptcy_candidate);
    let account = account.expect("trade supplies both accounts");
    assert_eq!(account.header.stale_state, 0);
    assert_eq!(account.header.b_stale_state, 0);
    assert_eq!(account.header.close_progress.active, 0);
    assert_eq!(market.header.threshold_stress_active, 0);
    assert_eq!(market.header.bankruptcy_hlock_active, 0);
    assert_eq!(decode_market_mode(market.header.mode)?, MarketModeV16::Live);
    Ok(HLockLaneV16::HMin)
}

#[cfg(all(kani, feature = "closure"))]
fn valid_two_asset_batch_trade_request_stub<'a: 'a, T>(
    market: &MarketGroupV16ViewMut<'a, T>,
    request: TradeRequestV16,
) -> V16Result<()> {
    assert!(request.asset_index < 2);
    assert_eq!(request.size_q.unsigned_abs(), POS_SCALE);
    assert_eq!(request.exec_price, 100);
    assert_eq!(request.fee_bps, 100);
    assert_eq!(market.header.config.max_market_slots.get(), 2);
    assert_eq!(market.header.config.max_trading_fee_bps.get(), 100);
    Ok(())
}

// Wrapper-shape batch closure. Two distinct assets execute through the real
// multi-fill loop around the asserted/proven fill seam. The theorem pins the
// matched OI/leg deltas, one final IM certificate, exact fee-funded insurance
// transfer, and a whole-state frame. It closes the composition gap between the
// signed-delta/accumulator kernels and the wrapper-facing batch entry point.
#[cfg(all(kani, feature = "closure"))]
fn prove_two_asset_batch_trade_is_conservative_and_order_independent<
    const ASSET_ZERO_LONG: bool,
    const REVERSED: bool,
    const HAS_MARGIN_SURPLUS: bool,
>() {
    let capital = if HAS_MARGIN_SURPLUS { 204 } else { 202 };

    let (mut header, mut markets, _) = two_asset_kf_mapping_fixture();
    header.config.max_trading_fee_bps = V16PodU64::new(100);
    let mut i = 0usize;
    while i < 2 {
        let mut asset = markets[i].engine.asset.try_to_runtime().unwrap();
        asset.slot_last = header.current_slot.get();
        markets[i].engine.asset = AssetStateV16Account::from_runtime(&asset);
        i += 1;
    }
    header.c_tot = V16PodU128::new(2 * capital);
    header.vault = V16PodU128::new(2 * capital + header.insurance.get());
    header.resolved_payout_blocker_count = V16PodU64::new(2);

    let current_empty_account = |owner: [u8; 32], account_id: [u8; 32]| {
        let provenance = ProvenanceHeaderV16Account::from_runtime(&ProvenanceHeaderV16::new(
            header.market_group_id,
            account_id,
            owner,
        ));
        let mut account = PortfolioAccountV16Account::default();
        account.init_empty_in_place(provenance).unwrap();
        account.capital = V16PodU128::new(capital);
        account.health_cert = HealthCertV16Account::from_runtime(&HealthCertV16 {
            certified_equity: capital as i128,
            certified_initial_req: 0,
            certified_maintenance_req: 0,
            certified_liq_deficit: 0,
            certified_worst_case_loss: 0,
            cert_oracle_epoch: header.oracle_epoch.get(),
            cert_funding_epoch: header.funding_epoch.get(),
            cert_risk_epoch: header.risk_epoch.get(),
            cert_asset_set_epoch: header.asset_set_epoch.get(),
            active_bitmap_at_cert: [0; V16_ACTIVE_BITMAP_WORDS],
            valid: true,
        });
        account
    };
    let mut first_header = current_empty_account([3u8; 32], [4u8; 32]);
    let mut second_header = current_empty_account([5u8; 32], [6u8; 32]);
    let request_zero = TradeRequestV16 {
        asset_index: 0,
        size_q: if ASSET_ZERO_LONG {
            POS_SCALE as i128
        } else {
            -(POS_SCALE as i128)
        },
        exec_price: 100,
        fee_bps: 100,
    };
    let request_one = TradeRequestV16 {
        asset_index: 1,
        size_q: if !ASSET_ZERO_LONG {
            POS_SCALE as i128
        } else {
            -(POS_SCALE as i128)
        },
        exec_price: 100,
        fee_bps: 100,
    };
    let requests = if REVERSED {
        [request_one, request_zero]
    } else {
        [request_zero, request_one]
    };

    let header_before = header;
    let markets_before = markets;
    let first_before = first_header;
    let second_before = second_header;
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut first = PortfolioV16ViewMut::new(&mut first_header);
    let mut second = PortfolioV16ViewMut::new(&mut second_header);
    let outcome = market
        .execute_batch_with_fee_loss_stale_scoped_not_atomic(&mut first, &mut second, &requests)
        .unwrap();
    assert_eq!(
        outcome,
        BatchTradeOutcomeV16 {
            fill_count: 2,
            fee_a: 2,
            fee_b: 2,
            notional: 200,
        }
    );

    assert_eq!(
        market.header.asset_slot_capacity,
        header_before.asset_slot_capacity
    );
    assert_eq!(market.header.vault.get(), header_before.vault.get());
    assert_eq!(market.header.c_tot.get(), header_before.c_tot.get() - 4);
    assert_eq!(
        market.header.insurance.get(),
        header_before.insurance.get() + 4
    );
    assert_eq!(market.header.pnl_pos_tot, header_before.pnl_pos_tot);
    assert_eq!(
        market.header.pnl_pos_bound_tot_num,
        header_before.pnl_pos_bound_tot_num
    );
    assert_eq!(
        market.header.source_claim_bound_total_num,
        header_before.source_claim_bound_total_num
    );
    assert_eq!(
        market.header.source_fresh_backing_total_num,
        header_before.source_fresh_backing_total_num
    );
    assert_eq!(
        market.header.source_insurance_credit_reserved_total_atoms,
        header_before.source_insurance_credit_reserved_total_atoms
    );
    assert_eq!(
        market.header.insurance_domain_budget_remaining_total,
        header_before.insurance_domain_budget_remaining_total
    );
    assert_eq!(
        market.header.resolved_payout_blocker_count.get(),
        header_before.resolved_payout_blocker_count.get() + 4
    );
    assert_eq!(
        market.header.materialized_portfolio_count,
        header_before.materialized_portfolio_count
    );
    assert_eq!(
        market.header.stale_certificate_count,
        header_before.stale_certificate_count
    );
    assert_eq!(
        market.header.b_stale_account_count,
        header_before.b_stale_account_count
    );
    assert_eq!(
        market.header.negative_pnl_account_count,
        header_before.negative_pnl_account_count
    );
    assert_eq!(
        market.header.bankruptcy_hlock_active,
        header_before.bankruptcy_hlock_active
    );
    assert_eq!(
        market.header.threshold_stress_active,
        header_before.threshold_stress_active
    );
    assert_eq!(
        market.header.loss_stale_active,
        header_before.loss_stale_active
    );
    assert_eq!(market.header.recovery_reason, header_before.recovery_reason);
    assert_eq!(market.header.mode, header_before.mode);
    assert_eq!(
        market.header.c_tot.get() + market.header.insurance.get(),
        market.header.vault.get()
    );

    let mut asset_index = 0usize;
    while asset_index < 2 {
        let before_slot = &markets_before[asset_index].engine;
        let after_slot = &market.markets[asset_index].engine;
        let before_asset = before_slot.asset.try_to_runtime().unwrap();
        let after_asset = after_slot.asset.try_to_runtime().unwrap();
        assert_eq!(after_asset.market_id, before_asset.market_id);
        assert_eq!(after_asset.lifecycle, before_asset.lifecycle);
        assert_eq!(after_asset.effective_price, before_asset.effective_price);
        assert_eq!(after_asset.slot_last, before_asset.slot_last);
        assert_eq!(after_asset.a_long, before_asset.a_long);
        assert_eq!(after_asset.a_short, before_asset.a_short);
        assert_eq!(after_asset.k_long, before_asset.k_long);
        assert_eq!(after_asset.k_short, before_asset.k_short);
        assert_eq!(after_asset.f_long_num, before_asset.f_long_num);
        assert_eq!(after_asset.f_short_num, before_asset.f_short_num);
        assert_eq!(after_asset.b_long_num, before_asset.b_long_num);
        assert_eq!(after_asset.b_short_num, before_asset.b_short_num);
        assert_eq!(after_asset.oi_eff_long_q, POS_SCALE);
        assert_eq!(after_asset.oi_eff_short_q, POS_SCALE);
        assert_eq!(after_asset.stored_pos_count_long, 1);
        assert_eq!(after_asset.stored_pos_count_short, 1);
        assert_eq!(after_asset.loss_weight_sum_long, POS_SCALE);
        assert_eq!(after_asset.loss_weight_sum_short, POS_SCALE);
        assert_eq!(
            after_slot.insurance_domain_budget_long,
            before_slot.insurance_domain_budget_long
        );
        assert_eq!(
            after_slot.insurance_domain_budget_short,
            before_slot.insurance_domain_budget_short
        );
        assert_eq!(
            after_slot.insurance_domain_spent_long,
            before_slot.insurance_domain_spent_long
        );
        assert_eq!(
            after_slot.insurance_domain_spent_short,
            before_slot.insurance_domain_spent_short
        );
        assert!(kani_eq_source_credit_state_v16_account(
            &after_slot.source_credit_long,
            &before_slot.source_credit_long
        ));
        assert!(kani_eq_source_credit_state_v16_account(
            &after_slot.source_credit_short,
            &before_slot.source_credit_short
        ));
        assert!(kani_eq_backing_bucket_v16_account(
            &after_slot.backing_long,
            &before_slot.backing_long
        ));
        assert!(kani_eq_backing_bucket_v16_account(
            &after_slot.backing_short,
            &before_slot.backing_short
        ));
        assert!(kani_eq_insurance_credit_reservation_v16_account(
            &after_slot.insurance_reservation_long,
            &before_slot.insurance_reservation_long
        ));
        assert!(kani_eq_insurance_credit_reservation_v16_account(
            &after_slot.insurance_reservation_short,
            &before_slot.insurance_reservation_short
        ));
        assert_eq!(
            market.markets[asset_index].wrapper,
            markets_before[asset_index].wrapper
        );
        asset_index += 1;
    }

    let expected_account = |before: PortfolioAccountV16Account, first_account: bool| {
        let mut account = before;
        account.capital = V16PodU128::new(capital - 2);
        account.active_bitmap[0] = V16PodU64::new(3);
        let mut slot = 0usize;
        while slot < 2 {
            let request = requests[slot];
            let signed_size = if first_account {
                request.size_q
            } else {
                -request.size_q
            };
            let side = if signed_size > 0 {
                SideV16::Long
            } else {
                SideV16::Short
            };
            let market_id = markets_before[request.asset_index]
                .engine
                .asset
                .try_to_runtime()
                .unwrap()
                .market_id;
            account.legs[slot] = PortfolioLegV16Account::from_runtime(&PortfolioLegV16 {
                active: true,
                asset_index: request.asset_index as u32,
                market_id,
                side,
                basis_pos_q: signed_size,
                a_basis: ADL_ONE,
                loss_weight: POS_SCALE,
                ..PortfolioLegV16::EMPTY
            });
            slot += 1;
        }
        account.health_cert = HealthCertV16Account::from_runtime(&HealthCertV16 {
            certified_equity: (capital - 2) as i128,
            certified_initial_req: 200,
            certified_maintenance_req: 200,
            certified_liq_deficit: 0,
            certified_worst_case_loss: 200,
            cert_oracle_epoch: header_before.oracle_epoch.get(),
            cert_funding_epoch: header_before.funding_epoch.get(),
            cert_risk_epoch: header_before.risk_epoch.get(),
            cert_asset_set_epoch: header_before.asset_set_epoch.get(),
            active_bitmap_at_cert: [3; V16_ACTIVE_BITMAP_WORDS],
            valid: true,
        });
        account
    };
    let expected_first = expected_account(first_before, true);
    let expected_second = expected_account(second_before, false);
    let assert_account_security_frame =
        |expected: &PortfolioAccountV16Account, actual: &PortfolioAccountV16Account| {
            assert_eq!(actual.capital, expected.capital);
            assert_eq!(actual.pnl, expected.pnl);
            assert_eq!(actual.reserved_pnl, expected.reserved_pnl);
            assert_eq!(actual.fee_credits, expected.fee_credits);
            assert_eq!(actual.cancel_deposit_escrow, expected.cancel_deposit_escrow);
            assert_eq!(actual.active_bitmap, expected.active_bitmap);
            let mut slot = 0usize;
            while slot < 2 {
                assert!(kani_eq_portfolio_leg_v16_account(
                    &actual.legs[slot],
                    &expected.legs[slot]
                ));
                slot += 1;
            }
            while slot < V16_MAX_PORTFOLIO_ASSETS_N {
                assert_eq!(actual.legs[slot].active, 0);
                slot += 1;
            }
            let mut domain_slot = 0usize;
            while domain_slot < PORTFOLIO_SOURCE_DOMAIN_CAP {
                assert!(!actual.source_domains[domain_slot].is_occupied());
                assert_eq!(actual.source_domains[domain_slot].domain.get(), 0);
                assert_eq!(
                    actual.source_domains[domain_slot]
                        .source_claim_market_id
                        .get(),
                    0
                );
                domain_slot += 1;
            }
            assert!(kani_eq_health_cert_v16_account(
                &actual.health_cert,
                &expected.health_cert
            ));
            assert_eq!(actual.stale_state, expected.stale_state);
            assert_eq!(actual.b_stale_state, expected.b_stale_state);
            assert_eq!(actual.rebalance_lock, expected.rebalance_lock);
            assert_eq!(actual.liquidation_lock, expected.liquidation_lock);
            assert_eq!(actual.close_progress.active, 0);
            assert_eq!(actual.close_progress.finalized, 0);
            assert_eq!(actual.resolved_payout_receipt.present, 0);
            assert_eq!(actual.resolved_payout_receipt.paid_effective.get(), 0);
        };
    assert_account_security_frame(&expected_first, first.header);
    assert_account_security_frame(&expected_second, second.header);
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(24)]
#[kani::solver(cadical)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_unconfigured_market_tail,
    valid_two_asset_market_tail_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_trade_request,
    valid_two_asset_batch_trade_request_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::h_lock_lane,
    unlocked_two_asset_batch_h_lock_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::settle_account_for_position_action_and_refresh_not_atomic,
    current_empty_trade_refresh_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::apply_trade_after_refresh_not_atomic,
    two_asset_batch_apply_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::certify_account_after_local_settlement_with_price_override,
    two_asset_batch_certify_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::create_initial_margin_source_lien_if_needed,
    unreachable_empty_trade_source_lien_stub
)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, valid_conversion_market_stub)]
#[kani::stub(PortfolioV16View::validate_with_market, valid_conversion_account_stub)]
fn closure_two_asset_batch_trade_asset_zero_long_is_conservative_and_order_independent() {
    prove_two_asset_batch_trade_is_conservative_and_order_independent::<true, false, false>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(24)]
#[kani::solver(cadical)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_unconfigured_market_tail,
    valid_two_asset_market_tail_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_trade_request,
    valid_two_asset_batch_trade_request_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::h_lock_lane,
    unlocked_two_asset_batch_h_lock_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::settle_account_for_position_action_and_refresh_not_atomic,
    current_empty_trade_refresh_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::apply_trade_after_refresh_not_atomic,
    two_asset_batch_apply_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::certify_account_after_local_settlement_with_price_override,
    two_asset_batch_certify_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::create_initial_margin_source_lien_if_needed,
    unreachable_empty_trade_source_lien_stub
)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, valid_conversion_market_stub)]
#[kani::stub(PortfolioV16View::validate_with_market, valid_conversion_account_stub)]
fn closure_two_asset_batch_trade_asset_zero_short_is_conservative_and_order_independent() {
    prove_two_asset_batch_trade_is_conservative_and_order_independent::<false, false, true>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(24)]
#[kani::solver(cadical)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_unconfigured_market_tail,
    valid_two_asset_market_tail_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_trade_request,
    valid_two_asset_batch_trade_request_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::h_lock_lane,
    unlocked_two_asset_batch_h_lock_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::settle_account_for_position_action_and_refresh_not_atomic,
    current_empty_trade_refresh_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::apply_trade_after_refresh_not_atomic,
    two_asset_batch_apply_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::certify_account_after_local_settlement_with_price_override,
    two_asset_batch_certify_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::create_initial_margin_source_lien_if_needed,
    unreachable_empty_trade_source_lien_stub
)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, valid_conversion_market_stub)]
#[kani::stub(PortfolioV16View::validate_with_market, valid_conversion_account_stub)]
fn closure_two_asset_batch_trade_asset_zero_long_reversed_is_conservative() {
    prove_two_asset_batch_trade_is_conservative_and_order_independent::<true, true, true>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(24)]
#[kani::solver(cadical)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_unconfigured_market_tail,
    valid_two_asset_market_tail_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_trade_request,
    valid_two_asset_batch_trade_request_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::h_lock_lane,
    unlocked_two_asset_batch_h_lock_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::settle_account_for_position_action_and_refresh_not_atomic,
    current_empty_trade_refresh_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::apply_trade_after_refresh_not_atomic,
    two_asset_batch_apply_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::certify_account_after_local_settlement_with_price_override,
    two_asset_batch_certify_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::create_initial_margin_source_lien_if_needed,
    unreachable_empty_trade_source_lien_stub
)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, valid_conversion_market_stub)]
#[kani::stub(PortfolioV16View::validate_with_market, valid_conversion_account_stub)]
fn closure_two_asset_batch_trade_asset_zero_short_reversed_is_conservative() {
    prove_two_asset_batch_trade_is_conservative_and_order_independent::<false, true, false>();
}

#[cfg(all(kani, feature = "closure"))]
fn insurance_reward_account_validation_stub<'a: 'a, T>(
    account: &PortfolioV16View<'a>,
    market: &MarketGroupV16View<'_, T>,
) -> V16Result<()> {
    assert_eq!(
        account.header.provenance_header.market_group_id,
        market.header.market_group_id
    );
    assert_eq!(account.header.owner, account.header.provenance_header.owner);
    assert_eq!(account.header.pnl.get(), 0);
    assert_eq!(account.header.reserved_pnl.get(), 0);
    assert_eq!(account.header.active_bitmap[0].get(), 0);
    assert_eq!(market.header.c_tot.get(), account.header.capital.get());
    assert_eq!(
        market.header.vault.get(),
        market.header.c_tot.get() + market.header.insurance.get()
    );
    assert!(
        market.header.insurance.get()
            >= market.header.insurance_domain_budget_remaining_total.get()
    );
    let mut domain = 0usize;
    while domain < PORTFOLIO_SOURCE_DOMAIN_CAP {
        assert!(!account.header.source_domains[domain].is_occupied());
        domain += 1;
    }
    Ok(())
}

#[cfg(all(kani, feature = "closure"))]
fn insurance_reward_market_validation_stub<'a: 'a, T>(
    market: &MarketGroupV16ViewMut<'a, T>,
) -> V16Result<()> {
    assert_eq!(market.markets.len(), 2);
    assert_eq!(decode_market_mode(market.header.mode)?, MarketModeV16::Live);
    assert_eq!(
        market.header.vault.get(),
        market.header.c_tot.get() + market.header.insurance.get()
    );
    assert_eq!(
        market
            .markets
            .iter()
            .map(|slot| {
                slot.engine.insurance_domain_budget_long.get()
                    - slot.engine.insurance_domain_spent_long.get()
                    + slot.engine.insurance_domain_budget_short.get()
                    - slot.engine.insurance_domain_spent_short.get()
            })
            .sum::<u128>(),
        market.header.insurance_domain_budget_remaining_total.get()
    );
    Ok(())
}

#[cfg(all(kani, feature = "closure"))]
fn insurance_reward_flow_validation_stub(proof: &TokenValueFlowProofV16) -> V16Result<()> {
    let amount = proof.debits[TokenValueClassV16::InsuranceCapital as usize];
    assert_ne!(amount, 0);
    assert_eq!(
        proof.credits[TokenValueClassV16::AccountCapital as usize],
        amount
    );
    let mut class = 0usize;
    while class < V16_TOKEN_VALUE_CLASS_COUNT {
        if class != TokenValueClassV16::InsuranceCapital as usize {
            assert_eq!(proof.debits[class], 0);
        }
        if class != TokenValueClassV16::AccountCapital as usize {
            assert_eq!(proof.credits[class], 0);
        }
        class += 1;
    }
    assert_eq!(proof.external_quote_in, 0);
    assert_eq!(proof.external_quote_out, 0);
    assert_eq!(proof.vault_before, proof.vault_after);
    Ok(())
}

// The wrapper pays crank rewards through this public route. For every bounded
// symbolic budget/available/request tuple it either performs the exact internal
// insurance->capital relabel, or rejects before mutation. Thus per-domain
// budgets remain isolated even though the reward comes from the shared vault.
#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
#[kani::stub(
    PortfolioV16View::validate_with_market,
    insurance_reward_account_validation_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_shape,
    insurance_reward_market_validation_stub
)]
#[kani::stub(
    TokenValueFlowProofV16::validate,
    insurance_reward_flow_validation_stub
)]
fn closure_insurance_reward_is_budget_isolated_and_failure_atomic() {
    let amount_raw: u8 = kani::any();
    let budget_raw: u8 = kani::any();
    let unbudgeted_raw: u8 = kani::any();
    let capital_raw: u8 = kani::any();
    kani::assume((1..=8).contains(&amount_raw));
    kani::assume(budget_raw <= 8);
    kani::assume(unbudgeted_raw <= 8);
    kani::assume((1..=8).contains(&capital_raw));
    let amount = amount_raw as u128;
    let budget = budget_raw as u128;
    let unbudgeted = unbudgeted_raw as u128;
    let capital = capital_raw as u128;
    let insurance = budget + unbudgeted;

    let (mut header, mut markets, mut account_header) = two_asset_kf_mapping_fixture();
    markets[0].engine.insurance_domain_budget_long = V16PodU128::new(budget);
    markets[0].engine.insurance_domain_budget_short = V16PodU128::new(0);
    markets[1].engine.insurance_domain_budget_long = V16PodU128::new(0);
    markets[1].engine.insurance_domain_budget_short = V16PodU128::new(0);
    header.insurance_domain_budget_remaining_total = V16PodU128::new(budget);
    header.insurance = V16PodU128::new(insurance);
    header.c_tot = V16PodU128::new(capital);
    header.vault = V16PodU128::new(capital + insurance);
    account_header.capital = V16PodU128::new(capital);
    account_header.health_cert = HealthCertV16Account::from_runtime(&HealthCertV16 {
        certified_equity: capital as i128,
        certified_initial_req: 0,
        certified_maintenance_req: 0,
        certified_liq_deficit: 0,
        certified_worst_case_loss: 0,
        cert_oracle_epoch: header.oracle_epoch.get(),
        cert_funding_epoch: header.funding_epoch.get(),
        cert_risk_epoch: header.risk_epoch.get(),
        cert_asset_set_epoch: header.asset_set_epoch.get(),
        active_bitmap_at_cert: [0; V16_ACTIVE_BITMAP_WORDS],
        valid: true,
    });

    let header_before = header;
    let markets_before = markets;
    let account_before = account_header;
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    let result = market.credit_account_from_insurance_not_atomic(&mut account, amount);
    let succeeds = amount <= unbudgeted;

    kani::cover!(succeeds, "reward uses available unbudgeted insurance");
    kani::cover!(
        !succeeds && amount <= insurance,
        "domain budget blocks an otherwise funded reward"
    );
    kani::cover!(
        amount > insurance,
        "reward rejects when aggregate insurance is insufficient"
    );

    let mut expected_header = header_before;
    let mut expected_account = account_before;
    if succeeds {
        assert_eq!(result, Ok(()));
        expected_header.insurance = V16PodU128::new(insurance - amount);
        expected_header.c_tot = V16PodU128::new(capital + amount);
        expected_account.capital = V16PodU128::new(capital + amount);
        expected_account.health_cert.valid = 0;
    } else if amount > insurance {
        assert_eq!(result, Err(V16Error::CounterUnderflow));
    } else {
        assert_eq!(result, Err(V16Error::LockActive));
    }
    assert!(kani_eq_market_group_v16_header_account(
        &expected_header,
        market.header
    ));
    assert!(kani_eq_portfolio_account_v16_account(
        &expected_account,
        account.header
    ));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &markets_before[0].engine,
        &market.markets[0].engine
    ));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &markets_before[1].engine,
        &market.markets[1].engine
    ));
    assert_eq!(market.markets[0].wrapper, markets_before[0].wrapper);
    assert_eq!(market.markets[1].wrapper, markets_before[1].wrapper);
    assert_eq!(
        market.header.c_tot.get() + market.header.insurance.get(),
        header_before.c_tot.get() + header_before.insurance.get()
    );
    assert_eq!(market.header.vault, header_before.vault);
    assert!(market.header.insurance.get() >= budget);
}

#[cfg(all(kani, feature = "closure"))]
fn liquidation_current_bankrupt_refresh_stub<'a: 'a, T>(
    market: &mut MarketGroupV16ViewMut<'a, T>,
    account: &mut PortfolioV16ViewMut<'_>,
    price_override: Option<(usize, u64)>,
    b_delta_budget: u128,
    allow_b_chunk: bool,
) -> V16Result<AccountRefreshCertOutcomeV16> {
    assert_eq!(price_override, None);
    assert_eq!(
        b_delta_budget,
        market.header.config.public_b_chunk_atoms.get()
    );
    assert!(!allow_b_chunk);
    assert_eq!(account.header.active_bitmap[0].get(), 1);
    assert!((-2..=-1).contains(&account.header.pnl.get()));
    assert_eq!(account.header.capital.get(), 0);
    let loss = account.header.pnl.get().unsigned_abs();
    let cert = HealthCertV16 {
        certified_equity: account.header.pnl.get(),
        certified_initial_req: 100,
        certified_maintenance_req: 100,
        certified_liq_deficit: 100 + loss,
        certified_worst_case_loss: 100,
        cert_oracle_epoch: market.header.oracle_epoch.get(),
        cert_funding_epoch: market.header.funding_epoch.get(),
        cert_risk_epoch: market.header.risk_epoch.get(),
        cert_asset_set_epoch: market.header.asset_set_epoch.get(),
        active_bitmap_at_cert: account.header.active_bitmap.map(V16PodU64::get),
        valid: true,
    };
    account.header.health_cert = HealthCertV16Account::from_runtime(&cert);
    Ok(AccountRefreshCertOutcomeV16::Certified(cert))
}

#[cfg(all(kani, feature = "closure"))]
fn liquidation_current_leg_settlement_stub<'a: 'a, T>(
    market: &mut MarketGroupV16ViewMut<'a, T>,
    account: &mut PortfolioV16ViewMut<'_>,
    leg_slot: usize,
) -> V16Result<()> {
    assert_eq!(leg_slot, 0);
    assert_eq!(account.header.pnl.get(), 0);
    assert_eq!(account.header.capital.get(), 0);
    let close = account.header.close_progress.try_to_runtime()?;
    assert!(close.active && close.finalized && !close.has_pending_residual());
    let leg = account.header.legs[leg_slot].try_to_runtime()?;
    assert!(leg.active && !leg.stale && !leg.b_stale);
    let asset_index = leg.asset_index as usize;
    assert!(asset_index < 2);
    let asset = market.markets[asset_index].engine.asset.try_to_runtime()?;
    assert_eq!(leg.market_id, asset.market_id);
    let (k_now, f_now, b_now, epoch_now) = match leg.side {
        SideV16::Long => (
            asset.k_long,
            asset.f_long_num,
            asset.b_long_num,
            asset.epoch_long,
        ),
        SideV16::Short => (
            asset.k_short,
            asset.f_short_num,
            asset.b_short_num,
            asset.epoch_short,
        ),
    };
    assert_eq!(leg.epoch_snap, epoch_now);
    assert_eq!(leg.b_epoch_snap, epoch_now);
    assert_eq!(leg.k_snap, k_now);
    assert_eq!(leg.f_snap, f_now);
    assert_eq!(leg.b_snap, b_now);
    Ok(())
}

#[cfg(all(kani, feature = "closure"))]
fn liquidation_full_clear_position_delta_stub<'a: 'a, T>(
    market: &mut MarketGroupV16ViewMut<'a, T>,
    account: &mut PortfolioV16ViewMut<'_>,
    asset_index: usize,
    delta_q: i128,
) -> V16Result<()> {
    liquidation_current_leg_settlement_stub(market, account, 0)?;
    let leg = account.header.legs[0].try_to_runtime()?;
    assert_eq!(asset_index, leg.asset_index as usize);
    assert_eq!(delta_q, leg.basis_pos_q.checked_neg().unwrap());
    assert_eq!(account.header.active_bitmap[0].get(), 1);
    assert_eq!(leg.b_rem, 0);
    assert!(!market.has_pending_domain_loss_barrier(asset_index, leg.side)?);

    let asset = market.asset_state(asset_index)?;
    let (remainder, dust) = match leg.side {
        SideV16::Long => (
            asset.social_loss_remainder_long_num,
            asset.social_loss_dust_long_num,
        ),
        SideV16::Short => (
            asset.social_loss_remainder_short_num,
            asset.social_loss_dust_short_num,
        ),
    };
    assert_eq!(remainder, 0);
    assert_eq!(dust, 0);
    let asset = V16Core::kernel_clear_leg(leg, asset)?;
    account.header.legs[0] = PortfolioLegV16Account::from_runtime(&PortfolioLegV16::EMPTY);
    let mut bitmap = account.header.active_bitmap.map(V16PodU64::get);
    active_bitmap_clear(&mut bitmap, 0)?;
    account.header.active_bitmap = bitmap.map(V16PodU64::new);
    account.header.health_cert.valid = 0;
    market.set_asset_state(asset_index, asset)
}

#[cfg(all(kani, feature = "closure"))]
fn liquidation_risk_notional_stub(abs_pos_q: u128, price: u64) -> V16Result<u128> {
    assert_eq!(abs_pos_q, POS_SCALE);
    assert_eq!(price, 100);
    Ok(100)
}

#[cfg(all(kani, feature = "closure"))]
fn liquidation_matching_a_ratio_stub(a: u128, b: u128, d: u128) -> u128 {
    assert_eq!(a, ADL_ONE);
    assert_eq!(b, POS_SCALE);
    assert_eq!(d, 2 * POS_SCALE);
    ADL_ONE / 2
}

#[cfg(all(kani, feature = "closure"))]
fn unreachable_liquidation_residual_capacity_stub<'a: 'a, T>(
    _market: &MarketGroupV16ViewMut<'a, T>,
    asset_index: usize,
    _bankrupt_side: SideV16,
    residual_remaining: u128,
) -> V16Result<u128> {
    assert!(asset_index < 2);
    assert_eq!(residual_remaining, 0);
    Ok(0)
}

#[cfg(all(kani, feature = "closure"))]
fn unreachable_liquidation_residual_booking_stub<'a: 'a, T>(
    _market: &mut MarketGroupV16ViewMut<'a, T>,
    _account: &mut PortfolioV16ViewMut<'_>,
    asset_index: usize,
    _bankrupt_side: SideV16,
    residual_remaining: u128,
) -> V16Result<BResidualBookingOutcomeV16> {
    assert!(asset_index < 2);
    assert_eq!(residual_remaining, 0);
    Ok(BResidualBookingOutcomeV16 {
        booked_loss: 0,
        explicit_loss: 0,
        delta_b: 0,
        remaining_after: 0,
    })
}

#[cfg(all(kani, feature = "closure"))]
fn liquidation_risk_score_stub<'a: 'a, T>(
    _market: &MarketGroupV16ViewMut<'a, T>,
    account: &PortfolioV16View<'_>,
) -> V16Result<RiskScoreV16> {
    let cert = account.header.health_cert.try_to_runtime()?;
    assert!(cert.valid);
    assert!((101..=102).contains(&cert.certified_liq_deficit));
    assert_eq!(cert.certified_worst_case_loss, 100);
    assert_eq!(account.header.active_bitmap[0].get(), 1);
    Ok(RiskScoreV16 {
        certified_liq_deficit: cert.certified_liq_deficit,
        unsettled_b_loss_bound: 0,
        stale_loss_bound: 0,
        gross_risk_notional: cert.certified_worst_case_loss,
        active_leg_count: 1,
    })
}

#[cfg(all(kani, feature = "closure"))]
fn liquidation_final_cert_stub<'a: 'a, T>(
    market: &mut MarketGroupV16ViewMut<'a, T>,
    account: &mut PortfolioV16ViewMut<'_>,
    price_override: Option<(usize, u64)>,
) -> V16Result<HealthCertV16> {
    assert_eq!(price_override, None);
    assert_eq!(account.header.active_bitmap[0].get(), 0);
    assert_eq!(account.header.pnl.get(), 0);
    assert_eq!(account.header.capital.get(), 0);
    let close = account.header.close_progress.try_to_runtime()?;
    assert!(close.active && close.finalized && !close.has_pending_residual());
    let cert = HealthCertV16 {
        cert_oracle_epoch: market.header.oracle_epoch.get(),
        cert_funding_epoch: market.header.funding_epoch.get(),
        cert_risk_epoch: market.header.risk_epoch.get(),
        cert_asset_set_epoch: market.header.asset_set_epoch.get(),
        active_bitmap_at_cert: V16_EMPTY_ACTIVE_BITMAP,
        valid: true,
        ..HealthCertV16::default()
    };
    account.header.health_cert = HealthCertV16Account::from_runtime(&cert);
    Ok(cert)
}

#[cfg(all(kani, feature = "closure"))]
fn liquidation_progress_stub<'a: 'a, T>(
    _market: &MarketGroupV16ViewMut<'a, T>,
    before: RiskScoreV16,
    after: &PortfolioV16View<'_>,
) -> V16Result<()> {
    assert!((101..=102).contains(&before.certified_liq_deficit));
    assert_eq!(before.unsettled_b_loss_bound, 0);
    assert_eq!(before.stale_loss_bound, 0);
    assert_eq!(before.gross_risk_notional, 100);
    assert_eq!(before.active_leg_count, 1);
    let cert = after.header.health_cert.try_to_runtime()?;
    assert!(cert.valid);
    assert_eq!(cert.certified_liq_deficit, 0);
    assert_eq!(cert.certified_worst_case_loss, 0);
    assert_eq!(after.header.active_bitmap[0].get(), 0);
    Ok(())
}

#[cfg(all(kani, feature = "closure"))]
fn liquidation_account_validation_stub<'a: 'a, T>(
    account: &PortfolioV16View<'a>,
    market: &MarketGroupV16View<'_, T>,
) -> V16Result<()> {
    assert_eq!(
        account.header.provenance_header.market_group_id,
        market.header.market_group_id
    );
    assert_eq!(account.header.owner, account.header.provenance_header.owner);
    assert_eq!(account.header.capital.get(), 0);
    assert!((-2..=0).contains(&account.header.pnl.get()));
    assert_eq!(account.header.fee_credits.get(), 0);
    assert_eq!(account.header.reserved_pnl.get(), 0);
    assert_eq!(
        account.header.source_domains[0],
        PortfolioSourceDomainV16Account::default()
    );
    assert!(account.header.active_bitmap[0].get() <= 1);
    Ok(())
}

#[cfg(all(kani, feature = "closure"))]
fn liquidation_market_validation_stub<'a: 'a, T>(
    market: &MarketGroupV16ViewMut<'a, T>,
) -> V16Result<()> {
    assert_eq!(market.markets.len(), 2);
    assert_eq!(market.header.vault.get(), 12);
    assert_eq!(market.header.c_tot.get(), 0);
    let slot0 = market.markets[0].engine_slot();
    let slot1 = market.markets[1].engine_slot();
    let remaining = slot0
        .insurance_domain_budget_long
        .get()
        .checked_sub(slot0.insurance_domain_spent_long.get())
        .and_then(|v| {
            v.checked_add(
                slot0
                    .insurance_domain_budget_short
                    .get()
                    .checked_sub(slot0.insurance_domain_spent_short.get())?,
            )
        })
        .and_then(|v| {
            v.checked_add(
                slot1
                    .insurance_domain_budget_long
                    .get()
                    .checked_sub(slot1.insurance_domain_spent_long.get())?,
            )
        })
        .and_then(|v| {
            v.checked_add(
                slot1
                    .insurance_domain_budget_short
                    .get()
                    .checked_sub(slot1.insurance_domain_spent_short.get())?,
            )
        })
        .ok_or(V16Error::CounterUnderflow)?;
    assert_eq!(market.header.insurance.get(), remaining);
    assert_eq!(
        market.header.insurance_domain_budget_remaining_total.get(),
        remaining
    );
    let asset0 = slot0.asset.try_to_runtime()?;
    let asset1 = slot1.asset.try_to_runtime()?;
    assert_eq!(asset0.oi_eff_long_q, asset0.oi_eff_short_q);
    assert_eq!(asset1.oi_eff_long_q, asset1.oi_eff_short_q);
    Ok(())
}

// Public liquidation composition. A bankrupt single-leg account is fully
// closed against only its opposing source domain while another asset carries
// live balanced risk and funded insurance. The route must consume exactly the
// selected budget, finalize its close barrier, preserve all unrelated state,
// keep OI symmetric, and move no vault value.
#[cfg(all(kani, feature = "closure"))]
fn prove_public_bankrupt_liquidation_is_value_conservative_and_asset_local<
    const ASSET: usize,
    const LONG: bool,
>() {
    assert!(ASSET < 2);
    // Keep public orchestration at one nontrivial amount. The production
    // insurance kernel's symbolic closure theorems prove the scalar relation;
    // these four instantiations prove end-to-end asset/side routing and frames.
    let loss = 2u128;
    let side = if LONG { SideV16::Long } else { SideV16::Short };
    let domain_side = opposite_side(side);
    let signed_position = if LONG {
        POS_SCALE as i128
    } else {
        -(POS_SCALE as i128)
    };

    let (mut header, mut markets, mut account_header) = two_asset_kf_mapping_fixture();
    account_header
        .init_empty_in_place(account_header.provenance_header)
        .unwrap();
    header.vault = V16PodU128::new(12);
    header.insurance = V16PodU128::new(12);
    header.insurance_domain_budget_remaining_total = V16PodU128::new(12);
    header.negative_pnl_account_count = V16PodU64::new(1);
    header.resolved_payout_blocker_count = V16PodU64::new(6);
    markets[0].wrapper = 11;
    markets[1].wrapper = 22;
    let mut asset_index = 0usize;
    while asset_index < 2 {
        markets[asset_index].engine.insurance_domain_budget_long = V16PodU128::new(3);
        markets[asset_index].engine.insurance_domain_budget_short = V16PodU128::new(3);
        let mut asset = markets[asset_index].engine.asset.try_to_runtime().unwrap();
        asset.slot_last = header.current_slot.get();
        if asset_index == ASSET {
            asset.oi_eff_long_q = 2 * POS_SCALE;
            asset.oi_eff_short_q = 2 * POS_SCALE;
            asset.stored_pos_count_long = 2;
            asset.stored_pos_count_short = 2;
            asset.loss_weight_sum_long = 2 * POS_SCALE;
            asset.loss_weight_sum_short = 2 * POS_SCALE;
        } else {
            asset.oi_eff_long_q = POS_SCALE;
            asset.oi_eff_short_q = POS_SCALE;
            asset.stored_pos_count_long = 1;
            asset.stored_pos_count_short = 1;
            asset.loss_weight_sum_long = POS_SCALE;
            asset.loss_weight_sum_short = POS_SCALE;
        }
        markets[asset_index].engine.asset = AssetStateV16Account::from_runtime(&asset);
        asset_index += 1;
    }
    let selected_asset = markets[ASSET].engine.asset.try_to_runtime().unwrap();
    account_header.pnl = V16PodI128::new(-(loss as i128));
    account_header.active_bitmap[0] = V16PodU64::new(1);
    account_header.legs[0] = PortfolioLegV16Account::from_runtime(&PortfolioLegV16 {
        active: true,
        asset_index: ASSET as u32,
        market_id: selected_asset.market_id,
        side,
        basis_pos_q: signed_position,
        a_basis: ADL_ONE,
        k_snap: if LONG {
            selected_asset.k_long
        } else {
            selected_asset.k_short
        },
        f_snap: if LONG {
            selected_asset.f_long_num
        } else {
            selected_asset.f_short_num
        },
        epoch_snap: if LONG {
            selected_asset.epoch_long
        } else {
            selected_asset.epoch_short
        },
        loss_weight: POS_SCALE,
        b_snap: if LONG {
            selected_asset.b_long_num
        } else {
            selected_asset.b_short_num
        },
        b_epoch_snap: if LONG {
            selected_asset.epoch_long
        } else {
            selected_asset.epoch_short
        },
        ..PortfolioLegV16::EMPTY
    });

    let header_before = header;
    let markets_before = markets;
    let account_before = account_header;
    let other = 1 - ASSET;
    let residual_before = header.vault.get() - header.insurance.get();
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    let outcome = market
        .liquidate_account_not_atomic(&mut account, LiquidationRequestV16 { asset_index: ASSET })
        .unwrap();

    assert_eq!(
        outcome,
        LiquidationOutcomeV16 {
            closed_q: POS_SCALE,
            insurance_used: loss,
            residual_booked: 0,
            explicit_loss: 0,
            fee_charged: 0,
        }
    );

    let mut expected_header = header_before;
    expected_header.insurance = V16PodU128::new(12 - loss);
    expected_header.insurance_domain_budget_remaining_total = V16PodU128::new(12 - loss);
    expected_header.negative_pnl_account_count = V16PodU64::new(0);
    expected_header.resolved_payout_blocker_count = V16PodU64::new(5);
    expected_header.bankruptcy_hlock_active = 1;
    assert!(kani_eq_market_group_v16_header_account(
        &expected_header,
        market.header
    ));

    let mut expected_selected = markets_before[ASSET].engine;
    let mut expected_asset = selected_asset;
    if LONG {
        expected_asset.oi_eff_long_q = POS_SCALE;
        expected_asset.stored_pos_count_long = 1;
        expected_asset.loss_weight_sum_long = POS_SCALE;
        expected_asset.oi_eff_short_q = POS_SCALE;
        expected_asset.a_short = ADL_ONE / 2;
        expected_selected.insurance_domain_spent_short = V16PodU128::new(loss);
    } else {
        expected_asset.oi_eff_short_q = POS_SCALE;
        expected_asset.stored_pos_count_short = 1;
        expected_asset.loss_weight_sum_short = POS_SCALE;
        expected_asset.oi_eff_long_q = POS_SCALE;
        expected_asset.a_long = ADL_ONE / 2;
        expected_selected.insurance_domain_spent_long = V16PodU128::new(loss);
    }
    expected_selected.asset = AssetStateV16Account::from_runtime(&expected_asset);
    assert!(kani_eq_engine_asset_slot_v16_account(
        &expected_selected,
        &market.markets[ASSET].engine
    ));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &markets_before[other].engine,
        &market.markets[other].engine
    ));
    assert_eq!(market.markets[0].wrapper, markets_before[0].wrapper);
    assert_eq!(market.markets[1].wrapper, markets_before[1].wrapper);

    let final_cert = HealthCertV16 {
        cert_oracle_epoch: header_before.oracle_epoch.get(),
        cert_funding_epoch: header_before.funding_epoch.get(),
        cert_risk_epoch: header_before.risk_epoch.get(),
        cert_asset_set_epoch: header_before.asset_set_epoch.get(),
        active_bitmap_at_cert: V16_EMPTY_ACTIVE_BITMAP,
        valid: true,
        ..HealthCertV16::default()
    };
    let final_close = CloseProgressLedgerV16 {
        active: true,
        finalized: true,
        close_id: 1,
        asset_index: ASSET as u32,
        market_id: selected_asset.market_id,
        domain_side,
        gross_loss_at_close_start: loss,
        drift_reference_slot: header_before.current_slot.get(),
        max_close_slot: header_before.current_slot.get()
            + header_before.config.max_bankrupt_close_lifetime_slots.get(),
        insurance_spent: loss,
        ..CloseProgressLedgerV16::EMPTY
    };
    let mut expected_account = account_before;
    expected_account.pnl = V16PodI128::new(0);
    expected_account.active_bitmap = V16_EMPTY_ACTIVE_BITMAP.map(V16PodU64::new);
    expected_account.legs[0] = PortfolioLegV16Account::from_runtime(&PortfolioLegV16::EMPTY);
    expected_account.health_cert = HealthCertV16Account::from_runtime(&final_cert);
    expected_account.close_progress = CloseProgressLedgerV16Account::from_runtime(&final_close);
    assert!(account_value_and_gate_frame_unchanged(
        &expected_account,
        account.header
    ));
    assert_eq!(account.header.legs[0], expected_account.legs[0]);
    assert_eq!(account.header.health_cert, expected_account.health_cert);
    assert_eq!(
        account.header.close_progress,
        expected_account.close_progress
    );
    assert_eq!(account.as_view().source_claim_bound_sum_num().unwrap(), 0);
    // The exact header and account value/gate frames above prove vault, C_tot,
    // and account capital unchanged; residual growth is funded insurance use.
    assert_eq!(market.residual(), residual_before + loss);
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(128)]
#[kani::solver(cadical)]
#[kani::stub(
    MarketGroupV16ViewMut::refresh_account_and_certify_not_atomic,
    liquidation_current_bankrupt_refresh_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::apply_position_delta,
    liquidation_full_clear_position_delta_stub
)]
#[kani::stub(
    crate::v16::liquidation_risk_notional_ceil,
    liquidation_risk_notional_stub
)]
#[kani::stub(
    crate::wide_math::wide_mul_div_floor_u128,
    liquidation_matching_a_ratio_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::bankruptcy_residual_single_step_capacity,
    unreachable_liquidation_residual_capacity_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::book_bankruptcy_residual_chunk_for_account_core,
    unreachable_liquidation_residual_booking_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::risk_score_unchecked,
    liquidation_risk_score_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::certify_account_after_local_settlement_with_price_override,
    liquidation_final_cert_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_liquidation_progress_from_score,
    liquidation_progress_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_shape,
    liquidation_market_validation_stub
)]
#[kani::stub(
    PortfolioV16View::validate_with_market,
    liquidation_account_validation_stub
)]
fn closure_public_asset_zero_long_bankrupt_liquidation_is_value_conservative_and_local() {
    prove_public_bankrupt_liquidation_is_value_conservative_and_asset_local::<0, true>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(128)]
#[kani::solver(cadical)]
#[kani::stub(
    MarketGroupV16ViewMut::refresh_account_and_certify_not_atomic,
    liquidation_current_bankrupt_refresh_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::apply_position_delta,
    liquidation_full_clear_position_delta_stub
)]
#[kani::stub(
    crate::v16::liquidation_risk_notional_ceil,
    liquidation_risk_notional_stub
)]
#[kani::stub(
    crate::wide_math::wide_mul_div_floor_u128,
    liquidation_matching_a_ratio_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::bankruptcy_residual_single_step_capacity,
    unreachable_liquidation_residual_capacity_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::book_bankruptcy_residual_chunk_for_account_core,
    unreachable_liquidation_residual_booking_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::risk_score_unchecked,
    liquidation_risk_score_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::certify_account_after_local_settlement_with_price_override,
    liquidation_final_cert_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_liquidation_progress_from_score,
    liquidation_progress_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_shape,
    liquidation_market_validation_stub
)]
#[kani::stub(
    PortfolioV16View::validate_with_market,
    liquidation_account_validation_stub
)]
fn closure_public_asset_zero_short_bankrupt_liquidation_is_value_conservative_and_local() {
    prove_public_bankrupt_liquidation_is_value_conservative_and_asset_local::<0, false>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(128)]
#[kani::solver(cadical)]
#[kani::stub(
    MarketGroupV16ViewMut::refresh_account_and_certify_not_atomic,
    liquidation_current_bankrupt_refresh_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::apply_position_delta,
    liquidation_full_clear_position_delta_stub
)]
#[kani::stub(
    crate::v16::liquidation_risk_notional_ceil,
    liquidation_risk_notional_stub
)]
#[kani::stub(
    crate::wide_math::wide_mul_div_floor_u128,
    liquidation_matching_a_ratio_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::bankruptcy_residual_single_step_capacity,
    unreachable_liquidation_residual_capacity_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::book_bankruptcy_residual_chunk_for_account_core,
    unreachable_liquidation_residual_booking_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::risk_score_unchecked,
    liquidation_risk_score_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::certify_account_after_local_settlement_with_price_override,
    liquidation_final_cert_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_liquidation_progress_from_score,
    liquidation_progress_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_shape,
    liquidation_market_validation_stub
)]
#[kani::stub(
    PortfolioV16View::validate_with_market,
    liquidation_account_validation_stub
)]
fn closure_public_asset_one_long_bankrupt_liquidation_is_value_conservative_and_local() {
    prove_public_bankrupt_liquidation_is_value_conservative_and_asset_local::<1, true>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(128)]
#[kani::solver(cadical)]
#[kani::stub(
    MarketGroupV16ViewMut::refresh_account_and_certify_not_atomic,
    liquidation_current_bankrupt_refresh_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::apply_position_delta,
    liquidation_full_clear_position_delta_stub
)]
#[kani::stub(
    crate::v16::liquidation_risk_notional_ceil,
    liquidation_risk_notional_stub
)]
#[kani::stub(
    crate::wide_math::wide_mul_div_floor_u128,
    liquidation_matching_a_ratio_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::bankruptcy_residual_single_step_capacity,
    unreachable_liquidation_residual_capacity_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::book_bankruptcy_residual_chunk_for_account_core,
    unreachable_liquidation_residual_booking_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::risk_score_unchecked,
    liquidation_risk_score_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::certify_account_after_local_settlement_with_price_override,
    liquidation_final_cert_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_liquidation_progress_from_score,
    liquidation_progress_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_shape,
    liquidation_market_validation_stub
)]
#[kani::stub(
    PortfolioV16View::validate_with_market,
    liquidation_account_validation_stub
)]
fn closure_public_asset_one_short_bankrupt_liquidation_is_value_conservative_and_local() {
    prove_public_bankrupt_liquidation_is_value_conservative_and_asset_local::<1, false>();
}

// Production bankruptcy insurance must debit only the bankrupt asset's
// opposing-side budget. This executes the real domain lookup, availability
// cap, insurance kernel, aggregate-spent setter, PnL cure, and flow check.
#[cfg(all(kani, feature = "closure"))]
fn prove_bankruptcy_insurance_domain_isolation<const ASSET: usize, const BANKRUPT_LONG: bool>() {
    assert!(ASSET < 2);
    let bankrupt_side = if BANKRUPT_LONG {
        SideV16::Long
    } else {
        SideV16::Short
    };
    let loss_raw: u8 = kani::any();
    kani::assume((1..=2).contains(&loss_raw));
    let loss = loss_raw as u128;
    let (mut header, mut markets, mut account_header) = two_asset_kf_mapping_fixture();
    markets[0].engine.insurance_domain_budget_long = V16PodU128::new(2);
    markets[0].engine.insurance_domain_budget_short = V16PodU128::new(3);
    markets[1].engine.insurance_domain_budget_long = V16PodU128::new(4);
    markets[1].engine.insurance_domain_budget_short = V16PodU128::new(5);
    header.vault = V16PodU128::new(14);
    header.insurance = V16PodU128::new(14);
    header.insurance_domain_budget_remaining_total = V16PodU128::new(14);
    header.negative_pnl_account_count = V16PodU64::new(1);
    account_header.pnl = V16PodI128::new(-(loss as i128));

    let other = 1 - ASSET;
    let other_before = markets[other].engine;
    let selected_before = markets[ASSET].engine;
    let header_before = header;
    let account_before = account_header;
    let mut expected_header = header_before;
    expected_header.insurance = V16PodU128::new(14 - loss);
    expected_header.insurance_domain_budget_remaining_total = V16PodU128::new(14 - loss);
    expected_header.negative_pnl_account_count = V16PodU64::new(0);
    expected_header.bankruptcy_hlock_active = 1;
    let mut expected_selected = selected_before;
    if BANKRUPT_LONG {
        expected_selected.insurance_domain_spent_short = V16PodU128::new(loss);
    } else {
        expected_selected.insurance_domain_spent_long = V16PodU128::new(loss);
    }
    let mut expected_account = account_before;
    expected_account.pnl = V16PodI128::new(0);
    expected_account.health_cert.valid = 0;

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    let residual_before = market.residual();
    let used = market
        .consume_domain_insurance_for_negative_pnl(ASSET, bankrupt_side, &mut account)
        .unwrap();

    kani::cover!(loss == 2, "bankruptcy insurance covers a multi-atom cure");
    assert_eq!(used, loss);
    assert!(kani_eq_market_group_v16_header_account(
        &expected_header,
        market.header
    ));
    assert!(kani_eq_portfolio_account_v16_account(
        &expected_account,
        account.header
    ));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &expected_selected,
        &market.markets[ASSET].engine
    ));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &other_before,
        &market.markets[other].engine
    ));
    assert_eq!(market.residual(), residual_before + loss);
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(64)]
#[kani::solver(cadical)]
fn closure_asset_zero_long_bankruptcy_insurance_is_domain_isolated() {
    prove_bankruptcy_insurance_domain_isolation::<0, true>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(64)]
#[kani::solver(cadical)]
fn closure_asset_zero_short_bankruptcy_insurance_is_domain_isolated() {
    prove_bankruptcy_insurance_domain_isolation::<0, false>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(64)]
#[kani::solver(cadical)]
fn closure_asset_one_long_bankruptcy_insurance_is_domain_isolated() {
    prove_bankruptcy_insurance_domain_isolation::<1, true>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(64)]
#[kani::solver(cadical)]
fn closure_asset_one_short_bankruptcy_insurance_is_domain_isolated() {
    prove_bankruptcy_insurance_domain_isolation::<1, false>();
}

// Liquidation preflight must not treat insurance in any wrong side/asset as
// durable capacity. With the exact opposing domain empty and every other domain
// funded, the only safe bounded outcome is RecoveryRequired.
#[cfg(all(kani, feature = "closure"))]
fn prove_liquidation_preflight_cannot_borrow_other_domains<
    const ASSET: usize,
    const BANKRUPT_LONG: bool,
>() {
    assert!(ASSET < 2);
    let bankrupt_side = if BANKRUPT_LONG {
        SideV16::Long
    } else {
        SideV16::Short
    };
    let loss_raw: u8 = kani::any();
    kani::assume((1..=2).contains(&loss_raw));
    let loss = loss_raw as u128;
    let (mut header, mut markets, mut account_header) = two_asset_kf_mapping_fixture();
    markets[0].engine.insurance_domain_budget_long = V16PodU128::new(2);
    markets[0].engine.insurance_domain_budget_short = V16PodU128::new(2);
    markets[1].engine.insurance_domain_budget_long = V16PodU128::new(2);
    markets[1].engine.insurance_domain_budget_short = V16PodU128::new(2);
    if BANKRUPT_LONG {
        markets[ASSET].engine.insurance_domain_budget_short = V16PodU128::new(0);
    } else {
        markets[ASSET].engine.insurance_domain_budget_long = V16PodU128::new(0);
    }
    header.vault = V16PodU128::new(6);
    header.insurance = V16PodU128::new(6);
    header.insurance_domain_budget_remaining_total = V16PodU128::new(6);
    header.negative_pnl_account_count = V16PodU64::new(1);
    account_header.pnl = V16PodI128::new(-(loss as i128));

    let markets_before = [markets[0].engine, markets[1].engine];
    let account_before = account_header;
    let header_before = header;
    let mut expected_header = header_before;
    expected_header.mode = encode_market_mode(MarketModeV16::Recovery);
    expected_header.recovery_reason = V16OptionalRecoveryReasonAccount::from_runtime(Some(
        PermissionlessRecoveryReasonV16::ActiveBankruptCloseCannotProgress,
    ));

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let account = PortfolioV16ViewMut::new(&mut account_header);
    let result =
        market.preflight_liquidation_residual_durability(ASSET, bankrupt_side, &account.as_view());

    kani::cover!(
        loss == 2,
        "preflight isolation covers a multi-atom uncovered residual"
    );
    assert_eq!(result, Err(V16Error::RecoveryRequired));
    assert!(kani_eq_market_group_v16_header_account(
        &expected_header,
        market.header
    ));
    assert!(kani_eq_portfolio_account_v16_account(
        &account_before,
        account.header
    ));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &markets_before[0],
        &market.markets[0].engine
    ));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &markets_before[1],
        &market.markets[1].engine
    ));
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(64)]
#[kani::solver(cadical)]
fn closure_asset_zero_long_preflight_cannot_borrow_other_domains() {
    prove_liquidation_preflight_cannot_borrow_other_domains::<0, true>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(64)]
#[kani::solver(cadical)]
fn closure_asset_zero_short_preflight_cannot_borrow_other_domains() {
    prove_liquidation_preflight_cannot_borrow_other_domains::<0, false>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(64)]
#[kani::solver(cadical)]
fn closure_asset_one_long_preflight_cannot_borrow_other_domains() {
    prove_liquidation_preflight_cannot_borrow_other_domains::<1, true>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(64)]
#[kani::solver(cadical)]
fn closure_asset_one_short_preflight_cannot_borrow_other_domains() {
    prove_liquidation_preflight_cannot_borrow_other_domains::<1, false>();
}

// The general split identity is covered by the arithmetic conformance suite.
// This exact specialization keeps the closure proof on the production control
// path without bit-blasting a symbolic u128 division already proven elsewhere.
#[cfg(all(kani, feature = "closure"))]
fn exact_small_social_loss_split_stub(
    engine_chunk: u128,
    carried_rem: u128,
    weight_sum: u128,
) -> V16Result<(u128, u128)> {
    assert!((1..=2).contains(&engine_chunk));
    assert_eq!(carried_rem, 0);
    assert_eq!(weight_sum, POS_SCALE);
    assert_eq!(SOCIAL_LOSS_DEN % POS_SCALE, 0);
    Ok((engine_chunk * (SOCIAL_LOSS_DEN / POS_SCALE), 0))
}

#[cfg(all(kani, feature = "closure"))]
fn account_value_and_gate_frame_unchanged(
    before: &PortfolioAccountV16Account,
    after: &PortfolioAccountV16Account,
) -> bool {
    before.provenance_header == after.provenance_header
        && before.owner == after.owner
        && before.capital == after.capital
        && before.pnl == after.pnl
        && before.reserved_pnl == after.reserved_pnl
        && before.residual_crystallized_loss_atoms_total
            == after.residual_crystallized_loss_atoms_total
        && before.residual_spent_principal_atoms_total == after.residual_spent_principal_atoms_total
        && before.residual_received_atoms_total == after.residual_received_atoms_total
        && before.funding_long_paid_atoms_total == after.funding_long_paid_atoms_total
        && before.funding_long_received_atoms_total == after.funding_long_received_atoms_total
        && before.funding_short_paid_atoms_total == after.funding_short_paid_atoms_total
        && before.funding_short_received_atoms_total == after.funding_short_received_atoms_total
        && before.fee_credits == after.fee_credits
        && before.cancel_deposit_escrow == after.cancel_deposit_escrow
        && before.last_fee_slot == after.last_fee_slot
        && before.active_bitmap == after.active_bitmap
        && before.stale_state == after.stale_state
        && before.b_stale_state == after.b_stale_state
        && before.rebalance_lock == after.rebalance_lock
        && before.liquidation_lock == after.liquidation_lock
        && before.resolved_payout_receipt == after.resolved_payout_receipt
}

// Account-level residual booking must consume an open close only through the
// selected asset/opposing-side barrier, charge B only there, release that
// barrier, and finalize the exact loss. This executes the production capacity,
// ledger, booking, validation, and whole-state writeback composition rather
// than replaying the leaf kernel. It starts at the valid post-detach close state;
// begin-close construction and live-leg/domain binding are proven separately.
#[cfg(all(kani, feature = "closure"))]
fn prove_bankruptcy_residual_booking_isolation<const ASSET: usize, const BANKRUPT_LONG: bool>() {
    assert!(ASSET < 2);
    let bankrupt_side = if BANKRUPT_LONG {
        SideV16::Long
    } else {
        SideV16::Short
    };
    let domain_side = opposite_side(bankrupt_side);
    let loss_raw: u8 = kani::any();
    kani::assume((1..=2).contains(&loss_raw));
    let loss = loss_raw as u128;
    let delta_b = loss * (SOCIAL_LOSS_DEN / POS_SCALE);
    let (mut header, mut markets, mut account_header) = two_asset_kf_mapping_fixture();
    account_header
        .init_empty_in_place(account_header.provenance_header)
        .unwrap();

    let mut asset = markets[ASSET].engine.asset.try_to_runtime().unwrap();
    asset.oi_eff_long_q = POS_SCALE;
    asset.oi_eff_short_q = POS_SCALE;
    asset.stored_pos_count_long = 1;
    asset.stored_pos_count_short = 1;
    asset.loss_weight_sum_long = POS_SCALE;
    asset.loss_weight_sum_short = POS_SCALE;
    let market_id = asset.market_id;
    markets[ASSET].engine.asset = AssetStateV16Account::from_runtime(&asset);
    header.resolved_payout_blocker_count = V16PodU64::new(3);
    header.negative_pnl_account_count = V16PodU64::new(1);
    account_header.pnl = V16PodI128::new(-(loss as i128));
    let open_ledger = CloseProgressLedgerV16 {
        active: true,
        close_id: 1,
        asset_index: ASSET as u32,
        market_id,
        domain_side,
        gross_loss_at_close_start: loss,
        drift_reference_slot: header.current_slot.get(),
        max_close_slot: header.current_slot.get()
            + header.config.max_bankrupt_close_lifetime_slots.get(),
        residual_remaining: loss,
        ..CloseProgressLedgerV16::EMPTY
    };
    account_header.close_progress = CloseProgressLedgerV16Account::from_runtime(&open_ledger);
    match domain_side {
        SideV16::Long => markets[ASSET].engine.pending_domain_loss_barrier_long = V16PodU64::new(1),
        SideV16::Short => {
            markets[ASSET].engine.pending_domain_loss_barrier_short = V16PodU64::new(1)
        }
    }

    let other = 1 - ASSET;
    let other_before = markets[other].engine;
    let selected_before = markets[ASSET].engine;
    let header_before = header;
    let account_before = account_header;

    let mut expected_header = header_before;
    expected_header.bankruptcy_hlock_active = 1;
    expected_header.resolved_payout_blocker_count = V16PodU64::new(2);
    let mut expected_selected = selected_before;
    let mut expected_asset = asset;
    match domain_side {
        SideV16::Long => expected_asset.b_long_num = delta_b,
        SideV16::Short => expected_asset.b_short_num = delta_b,
    }
    expected_selected.asset = AssetStateV16Account::from_runtime(&expected_asset);
    match domain_side {
        SideV16::Long => expected_selected.pending_domain_loss_barrier_long = V16PodU64::new(0),
        SideV16::Short => expected_selected.pending_domain_loss_barrier_short = V16PodU64::new(0),
    }
    let expected_ledger = CloseProgressLedgerV16 {
        active: true,
        finalized: true,
        close_id: 1,
        asset_index: ASSET as u32,
        market_id,
        domain_side,
        gross_loss_at_close_start: loss,
        drift_reference_slot: header.current_slot.get(),
        max_close_slot: header.current_slot.get()
            + header.config.max_bankrupt_close_lifetime_slots.get(),
        b_loss_booked: loss,
        ..CloseProgressLedgerV16::EMPTY
    };
    let mut expected_account = account_before;
    expected_account.close_progress = CloseProgressLedgerV16Account::from_runtime(&expected_ledger);
    expected_account.health_cert.valid = 0;

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    let residual_before = market.residual();
    let outcome = market
        .book_bankruptcy_residual_chunk_for_account_core(&mut account, ASSET, bankrupt_side, loss)
        .unwrap();

    kani::cover!(
        loss == 2,
        "account-level residual booking covers multi-atom close progress"
    );
    assert_eq!(
        outcome,
        BResidualBookingOutcomeV16 {
            booked_loss: loss,
            explicit_loss: 0,
            delta_b,
            remaining_after: 0,
        }
    );
    assert!(kani_eq_market_group_v16_header_account(
        &expected_header,
        market.header
    ));
    assert!(account_value_and_gate_frame_unchanged(
        &account_before,
        account.header
    ));
    assert_eq!(
        account.header.close_progress,
        expected_account.close_progress
    );
    assert_eq!(account.header.health_cert, expected_account.health_cert);
    // Successful production validation plus these zero summaries implies all
    // leg and source-domain records remain canonical and unoccupied.
    assert!(active_bitmap_is_empty(
        account.header.active_bitmap.map(V16PodU64::get)
    ));
    assert_eq!(account.as_view().source_claim_bound_sum_num().unwrap(), 0);
    assert!(kani_eq_engine_asset_slot_v16_account(
        &expected_selected,
        &market.markets[ASSET].engine
    ));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &other_before,
        &market.markets[other].engine
    ));
    assert_eq!(market.residual(), residual_before);
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(80)]
#[kani::solver(cadical)]
#[kani::stub(crate::v16::social_loss_book_split, exact_small_social_loss_split_stub)]
fn closure_asset_zero_long_residual_books_only_to_short_domain() {
    prove_bankruptcy_residual_booking_isolation::<0, true>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(80)]
#[kani::solver(cadical)]
#[kani::stub(crate::v16::social_loss_book_split, exact_small_social_loss_split_stub)]
fn closure_asset_zero_short_residual_books_only_to_long_domain() {
    prove_bankruptcy_residual_booking_isolation::<0, false>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(80)]
#[kani::solver(cadical)]
#[kani::stub(crate::v16::social_loss_book_split, exact_small_social_loss_split_stub)]
fn closure_asset_one_long_residual_books_only_to_short_domain() {
    prove_bankruptcy_residual_booking_isolation::<1, true>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(80)]
#[kani::solver(cadical)]
#[kani::stub(crate::v16::social_loss_book_split, exact_small_social_loss_split_stub)]
fn closure_asset_one_short_residual_books_only_to_long_domain() {
    prove_bankruptcy_residual_booking_isolation::<1, false>();
}

// Resolved unattributed bad debt has no defensible asset/domain payer. The
// production route must clear the terminal account liability for liveness while
// leaving every funded stock and asset/domain ledger byte-for-byte unchanged.
#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(64)]
#[kani::solver(cadical)]
fn closure_resolved_unattributed_bad_debt_is_value_and_domain_neutral() {
    let loss_raw: u8 = kani::any();
    kani::assume((1..=2).contains(&loss_raw));
    let loss = loss_raw as i128;
    let (mut header, mut markets, mut account_header) = two_asset_kf_mapping_fixture();
    account_header
        .init_empty_in_place(account_header.provenance_header)
        .unwrap();
    header.mode = encode_market_mode(MarketModeV16::Resolved);
    header.resolved_slot = header.current_slot;
    header.negative_pnl_account_count = V16PodU64::new(1);
    account_header.pnl = V16PodI128::new(-loss);

    let header_before = header;
    let account_before = account_header;
    let markets_before = [markets[0].engine, markets[1].engine];
    let mut expected_header = header_before;
    expected_header.bankruptcy_hlock_active = 1;
    expected_header.negative_pnl_account_count = V16PodU64::new(0);
    let mut expected_account = account_before;
    expected_account.pnl = V16PodI128::new(0);
    expected_account.health_cert.valid = 0;

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    let residual_before = market.residual();
    let result = market.settle_resolved_bankruptcy_negative_pnl(&mut account);

    kani::cover!(
        loss == 2,
        "resolved unattributed close clears a multi-atom terminal liability"
    );
    assert_eq!(result, Ok(()));
    assert!(kani_eq_market_group_v16_header_account(
        &expected_header,
        market.header
    ));
    assert!(kani_eq_portfolio_account_v16_account(
        &expected_account,
        account.header
    ));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &markets_before[0],
        &market.markets[0].engine
    ));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &markets_before[1],
        &market.markets[1].engine
    ));
    assert_eq!(market.residual(), residual_before);
}

// An open resolved close is already bound to one asset/domain. Insurance-funded
// settlement must debit only that domain, finalize its ledger, and clear the
// account liability without touching another side or asset.
#[cfg(all(kani, feature = "closure"))]
fn prove_resolved_attributed_close_cannot_borrow_other_domains<
    const ASSET: usize,
    const BANKRUPT_LONG: bool,
>() {
    assert!(ASSET < 2);
    let bankrupt_side = if BANKRUPT_LONG {
        SideV16::Long
    } else {
        SideV16::Short
    };
    let domain_side = opposite_side(bankrupt_side);
    // Keep closure composition tractable at one nontrivial amount; the
    // insurance kernel proves the amount relation over its full scalar domain.
    let loss = 2u128;
    let (mut header, mut markets, mut account_header) = two_asset_kf_mapping_fixture();
    account_header
        .init_empty_in_place(account_header.provenance_header)
        .unwrap();
    header.mode = encode_market_mode(MarketModeV16::Resolved);
    header.resolved_slot = header.current_slot;
    markets[0].engine.insurance_domain_budget_long = V16PodU128::new(2);
    markets[0].engine.insurance_domain_budget_short = V16PodU128::new(2);
    markets[1].engine.insurance_domain_budget_long = V16PodU128::new(2);
    markets[1].engine.insurance_domain_budget_short = V16PodU128::new(2);
    header.vault = V16PodU128::new(8);
    header.insurance = V16PodU128::new(8);
    header.insurance_domain_budget_remaining_total = V16PodU128::new(8);
    header.negative_pnl_account_count = V16PodU64::new(1);
    header.resolved_payout_blocker_count = V16PodU64::new(1);
    account_header.pnl = V16PodI128::new(-(loss as i128));
    let market_id = markets[ASSET].engine.asset.market_id.get();
    let open_ledger = CloseProgressLedgerV16 {
        active: true,
        close_id: 1,
        asset_index: ASSET as u32,
        market_id,
        domain_side,
        gross_loss_at_close_start: loss,
        drift_reference_slot: header.current_slot.get(),
        max_close_slot: header.current_slot.get()
            + header.config.max_bankrupt_close_lifetime_slots.get(),
        residual_remaining: loss,
        ..CloseProgressLedgerV16::EMPTY
    };
    account_header.close_progress = CloseProgressLedgerV16Account::from_runtime(&open_ledger);
    match domain_side {
        SideV16::Long => markets[ASSET].engine.pending_domain_loss_barrier_long = V16PodU64::new(1),
        SideV16::Short => {
            markets[ASSET].engine.pending_domain_loss_barrier_short = V16PodU64::new(1)
        }
    }

    let header_before = header;
    let account_before = account_header;
    let markets_before = [markets[0].engine, markets[1].engine];
    let mut expected_header = header_before;
    expected_header.bankruptcy_hlock_active = 1;
    expected_header.insurance = V16PodU128::new(8 - loss);
    expected_header.insurance_domain_budget_remaining_total = V16PodU128::new(8 - loss);
    expected_header.negative_pnl_account_count = V16PodU64::new(0);
    expected_header.resolved_payout_blocker_count = V16PodU64::new(0);
    let mut expected_markets = markets_before;
    match domain_side {
        SideV16::Long => {
            expected_markets[ASSET].pending_domain_loss_barrier_long = V16PodU64::new(0);
            expected_markets[ASSET].insurance_domain_spent_long = V16PodU128::new(loss);
        }
        SideV16::Short => {
            expected_markets[ASSET].pending_domain_loss_barrier_short = V16PodU64::new(0);
            expected_markets[ASSET].insurance_domain_spent_short = V16PodU128::new(loss);
        }
    }
    let expected_ledger = CloseProgressLedgerV16 {
        active: true,
        finalized: true,
        close_id: 1,
        asset_index: ASSET as u32,
        market_id,
        domain_side,
        gross_loss_at_close_start: loss,
        drift_reference_slot: header.current_slot.get(),
        max_close_slot: header.current_slot.get()
            + header.config.max_bankrupt_close_lifetime_slots.get(),
        insurance_spent: loss,
        ..CloseProgressLedgerV16::EMPTY
    };
    let mut expected_account = account_before;
    expected_account.pnl = V16PodI128::new(0);
    expected_account.close_progress = CloseProgressLedgerV16Account::from_runtime(&expected_ledger);
    expected_account.health_cert.valid = 0;

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    let residual_before = market.residual();
    let result = market.settle_resolved_bankruptcy_negative_pnl(&mut account);

    kani::cover!(
        result == Ok(()),
        "resolved attributed close consumes multi-atom domain insurance"
    );
    assert_eq!(result, Ok(()));
    assert!(kani_eq_market_group_v16_header_account(
        &expected_header,
        market.header
    ));
    assert!(account_value_and_gate_frame_unchanged(
        &expected_account,
        account.header
    ));
    assert_eq!(
        account.header.close_progress,
        expected_account.close_progress
    );
    assert_eq!(account.header.health_cert, expected_account.health_cert);
    assert!(active_bitmap_is_empty(
        account.header.active_bitmap.map(V16PodU64::get)
    ));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &expected_markets[0],
        &market.markets[0].engine
    ));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &expected_markets[1],
        &market.markets[1].engine
    ));
    assert_eq!(market.residual(), residual_before + loss);
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(80)]
#[kani::solver(cadical)]
fn closure_resolved_asset_zero_long_close_uses_only_short_insurance() {
    prove_resolved_attributed_close_cannot_borrow_other_domains::<0, true>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(80)]
#[kani::solver(cadical)]
fn closure_resolved_asset_zero_short_close_uses_only_long_insurance() {
    prove_resolved_attributed_close_cannot_borrow_other_domains::<0, false>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(80)]
#[kani::solver(cadical)]
fn closure_resolved_asset_one_long_close_uses_only_short_insurance() {
    prove_resolved_attributed_close_cannot_borrow_other_domains::<1, true>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(80)]
#[kani::solver(cadical)]
fn closure_resolved_asset_one_short_close_uses_only_long_insurance() {
    prove_resolved_attributed_close_cannot_borrow_other_domains::<1, false>();
}

// Resolved bad debt is attributable only from a unique live obligation or an
// already-open close ledger. The ledger is authoritative for restart progress;
// two live obligations are ambiguous and must never pick an arbitrary payer.
#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn closure_resolved_bankruptcy_attribution_is_unique_and_ledger_authoritative() {
    let selected_raw: u8 = kani::any();
    kani::assume(selected_raw < 2);
    let selected = selected_raw as usize;
    let leg_side = if kani::any() {
        SideV16::Long
    } else {
        SideV16::Short
    };
    let ledger_bankrupt_side = if kani::any() {
        SideV16::Long
    } else {
        SideV16::Short
    };
    let other = 1 - selected;
    let (mut header, mut markets, mut account_header) = two_asset_kf_mapping_fixture();
    account_header
        .init_empty_in_place(account_header.provenance_header)
        .unwrap();
    account_header.active_bitmap[0] = V16PodU64::new(1);
    account_header.legs[0] = PortfolioLegV16Account::from_runtime(&PortfolioLegV16 {
        active: true,
        asset_index: selected as u32,
        market_id: markets[selected].engine.asset.market_id.get(),
        side: leg_side,
        basis_pos_q: 0,
        a_basis: ADL_ONE,
        loss_weight: 1,
        ..PortfolioLegV16::EMPTY
    });

    let market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let account = PortfolioV16ViewMut::new(&mut account_header);
    assert_eq!(
        market
            .resolved_bankruptcy_attribution(&account.as_view())
            .unwrap(),
        Some((selected, leg_side))
    );

    account.header.close_progress =
        CloseProgressLedgerV16Account::from_runtime(&CloseProgressLedgerV16 {
            active: true,
            close_id: 1,
            asset_index: other as u32,
            market_id: market.markets[other].engine.asset.market_id.get(),
            domain_side: opposite_side(ledger_bankrupt_side),
            gross_loss_at_close_start: 1,
            residual_remaining: 1,
            ..CloseProgressLedgerV16::EMPTY
        });
    assert_eq!(
        market
            .resolved_bankruptcy_attribution(&account.as_view())
            .unwrap(),
        Some((other, ledger_bankrupt_side))
    );

    account.header.close_progress = CloseProgressLedgerV16Account::default();
    account.header.active_bitmap[0] = V16PodU64::new(3);
    account.header.legs[1] = PortfolioLegV16Account::from_runtime(&PortfolioLegV16 {
        active: true,
        asset_index: other as u32,
        market_id: market.markets[other].engine.asset.market_id.get(),
        side: opposite_side(leg_side),
        basis_pos_q: 0,
        a_basis: ADL_ONE,
        loss_weight: 1,
        ..PortfolioLegV16::EMPTY
    });
    let ambiguous = market
        .resolved_bankruptcy_attribution(&account.as_view())
        .unwrap();

    kani::cover!(
        selected == 1 && leg_side != ledger_bankrupt_side,
        "attribution covers conflicting asset and side authority"
    );
    assert_eq!(ambiguous, None);
}

// Public source-backed conversion is the boundary where junior, oracle-derived
// PnL becomes withdrawable capital. The conversion must consume the selected
// domain's real counterparty or reserved-insurance backing, move that exact
// stock into C_tot, and leave every byte of the other asset untouched.
#[cfg(all(kani, feature = "closure"))]
fn prove_source_backed_conversion_is_asset_local<
    const ASSET: usize,
    const SOURCE_LONG: bool,
    const INSURANCE_BACKED: bool,
>() {
    assert!(ASSET < 2);
    let claim = 2u128;
    let source_side = if SOURCE_LONG {
        SideV16::Long
    } else {
        SideV16::Short
    };
    let domain = ASSET * 2 + encode_side(source_side) as usize;
    let claim_num = claim * BOUND_SCALE;
    let (mut header, mut markets, mut account_header) = two_asset_kf_mapping_fixture();
    account_header
        .init_empty_in_place(account_header.provenance_header)
        .unwrap();

    header.vault = V16PodU128::new(if INSURANCE_BACKED { 10 } else { 10 + claim });
    header.c_tot = V16PodU128::new(0);
    header.pnl_pos_tot = V16PodU128::new(claim);
    header.pnl_pos_bound_tot_num = V16PodU128::new(claim_num);
    header.pnl_pos_bound_tot = V16PodU128::new(claim);
    header.source_claim_bound_total_num = V16PodU128::new(claim_num);
    if INSURANCE_BACKED {
        header.source_insurance_credit_reserved_total_atoms = V16PodU128::new(claim);
        header.insurance_domain_budget_remaining_total = V16PodU128::new(8);
        for market in &mut markets {
            market.engine.insurance_domain_budget_long = V16PodU128::new(claim);
            market.engine.insurance_domain_budget_short = V16PodU128::new(claim);
        }
    } else {
        header.source_fresh_backing_total_num = V16PodU128::new(claim_num);
    }
    account_header.pnl = V16PodI128::new(claim as i128);
    account_header.health_cert = HealthCertV16Account::from_runtime(&HealthCertV16 {
        certified_equity: claim as i128,
        certified_initial_req: 0,
        certified_maintenance_req: 0,
        certified_liq_deficit: 0,
        certified_worst_case_loss: 0,
        cert_oracle_epoch: header.oracle_epoch.get(),
        cert_funding_epoch: header.funding_epoch.get(),
        cert_risk_epoch: header.risk_epoch.get(),
        cert_asset_set_epoch: header.asset_set_epoch.get(),
        active_bitmap_at_cert: V16_EMPTY_ACTIVE_BITMAP,
        valid: true,
    });
    account_header.source_domains[0].domain = V16PodU32::new(domain as u32);
    account_header.source_domains[0].source_claim_market_id = markets[ASSET].engine.asset.market_id;
    account_header.source_domains[0].source_claim_bound_num = V16PodU128::new(claim_num);

    let initial_source = SourceCreditStateV16Account::from_runtime(&SourceCreditStateV16 {
        positive_claim_bound_num: claim_num,
        exact_positive_claim_num: claim_num,
        fresh_reserved_backing_num: if INSURANCE_BACKED { 0 } else { claim_num },
        insurance_credit_reserved_num: if INSURANCE_BACKED { claim_num } else { 0 },
        credit_rate_num: CREDIT_RATE_SCALE,
        ..SourceCreditStateV16::EMPTY
    });
    let initial_bucket = BackingBucketV16Account::from_runtime(&BackingBucketV16 {
        market_id: markets[ASSET].engine.asset.market_id.get(),
        fresh_unliened_backing_num: claim_num,
        expiry_slot: 10,
        status: BackingBucketStatusV16::Fresh,
        ..BackingBucketV16::EMPTY
    });
    let initial_reservation =
        InsuranceCreditReservationV16Account::from_runtime(&InsuranceCreditReservationV16 {
            insurance_credit_reserved_num: claim_num,
            ..InsuranceCreditReservationV16::EMPTY
        });
    match source_side {
        SideV16::Long => {
            markets[ASSET].engine.source_credit_long = initial_source;
            if INSURANCE_BACKED {
                markets[ASSET].engine.insurance_reservation_long = initial_reservation;
            } else {
                markets[ASSET].engine.backing_long = initial_bucket;
            }
        }
        SideV16::Short => {
            markets[ASSET].engine.source_credit_short = initial_source;
            if INSURANCE_BACKED {
                markets[ASSET].engine.insurance_reservation_short = initial_reservation;
            } else {
                markets[ASSET].engine.backing_short = initial_bucket;
            }
        }
    }

    let mut expected_header = header;
    expected_header.c_tot = V16PodU128::new(claim);
    expected_header.pnl_pos_tot = V16PodU128::new(0);
    expected_header.pnl_pos_bound_tot_num = V16PodU128::new(0);
    expected_header.pnl_pos_bound_tot = V16PodU128::new(0);
    expected_header.source_claim_bound_total_num = V16PodU128::new(0);
    expected_header.source_fresh_backing_total_num = V16PodU128::new(0);
    if INSURANCE_BACKED {
        expected_header.insurance = V16PodU128::new(10 - claim);
        expected_header.source_insurance_credit_reserved_total_atoms = V16PodU128::new(0);
        expected_header.insurance_domain_budget_remaining_total = V16PodU128::new(8 - claim);
    }
    expected_header.risk_epoch = V16PodU64::new(header.risk_epoch.get() + 2);

    let mut expected_markets = [markets[0].engine, markets[1].engine];
    let consumed_source = SourceCreditStateV16Account::from_runtime(&SourceCreditStateV16 {
        spent_backing_num: if INSURANCE_BACKED { 0 } else { claim_num },
        provider_receivable_num: if INSURANCE_BACKED { 0 } else { claim_num },
        credit_rate_num: CREDIT_RATE_SCALE,
        credit_epoch: 2,
        ..SourceCreditStateV16::EMPTY
    });
    let consumed_bucket = BackingBucketV16Account::from_runtime(&BackingBucketV16 {
        market_id: markets[ASSET].engine.asset.market_id.get(),
        consumed_liened_backing_num: claim_num,
        expiry_slot: 10,
        status: BackingBucketStatusV16::Expired,
        ..BackingBucketV16::EMPTY
    });
    let consumed_reservation =
        InsuranceCreditReservationV16Account::from_runtime(&InsuranceCreditReservationV16 {
            consumed_insurance_num: claim_num,
            ..InsuranceCreditReservationV16::EMPTY
        });
    match source_side {
        SideV16::Long => {
            expected_markets[ASSET].source_credit_long = consumed_source;
            if INSURANCE_BACKED {
                expected_markets[ASSET].insurance_reservation_long = consumed_reservation;
                expected_markets[ASSET].insurance_domain_spent_long = V16PodU128::new(claim);
            } else {
                expected_markets[ASSET].backing_long = consumed_bucket;
            }
        }
        SideV16::Short => {
            expected_markets[ASSET].source_credit_short = consumed_source;
            if INSURANCE_BACKED {
                expected_markets[ASSET].insurance_reservation_short = consumed_reservation;
                expected_markets[ASSET].insurance_domain_spent_short = V16PodU128::new(claim);
            } else {
                expected_markets[ASSET].backing_short = consumed_bucket;
            }
        }
    }

    let mut expected_account = account_header;
    expected_account.capital = V16PodU128::new(claim);
    expected_account.pnl = V16PodI128::new(0);
    expected_account.health_cert.valid = 0;
    expected_account.source_domains[0] = PortfolioSourceDomainV16Account::default();

    kani::cover!(true, "source-backed conversion route is reachable");
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    let converted = market
        .convert_released_pnl_to_capital_not_atomic(&mut account)
        .unwrap();

    kani::cover!(converted == claim, "public conversion succeeds");
    assert_eq!(converted, claim);
    assert!(kani_eq_market_group_v16_header_account(
        &expected_header,
        market.header
    ));
    assert!(kani_eq_portfolio_account_v16_account(
        &expected_account,
        account.header
    ));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &expected_markets[0],
        &market.markets[0].engine
    ));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &expected_markets[1],
        &market.markets[1].engine
    ));
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(96)]
#[kani::solver(cadical)]
#[kani::stub(crate::wide_math::div_rem_u256, bounded_u256_div_rem_stub)]
#[kani::stub(TokenValueFlowProofV16::validate, conversion_flow_validate_stub)]
#[kani::stub(
    PortfolioV16ViewMut::compact_source_domains,
    canonical_conversion_compact_stub
)]
#[kani::stub(PortfolioV16View::validate_with_market, valid_conversion_account_stub)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, valid_conversion_market_stub)]
fn closure_asset_zero_long_source_backed_conversion_is_local() {
    prove_source_backed_conversion_is_asset_local::<0, true, false>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(96)]
#[kani::solver(cadical)]
#[kani::stub(crate::wide_math::div_rem_u256, bounded_u256_div_rem_stub)]
#[kani::stub(TokenValueFlowProofV16::validate, conversion_flow_validate_stub)]
#[kani::stub(
    PortfolioV16ViewMut::compact_source_domains,
    canonical_conversion_compact_stub
)]
#[kani::stub(PortfolioV16View::validate_with_market, valid_conversion_account_stub)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, valid_conversion_market_stub)]
fn closure_asset_zero_short_source_backed_conversion_is_local() {
    prove_source_backed_conversion_is_asset_local::<0, false, false>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(96)]
#[kani::solver(cadical)]
#[kani::stub(crate::wide_math::div_rem_u256, bounded_u256_div_rem_stub)]
#[kani::stub(TokenValueFlowProofV16::validate, conversion_flow_validate_stub)]
#[kani::stub(
    PortfolioV16ViewMut::compact_source_domains,
    canonical_conversion_compact_stub
)]
#[kani::stub(PortfolioV16View::validate_with_market, valid_conversion_account_stub)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, valid_conversion_market_stub)]
fn closure_asset_one_long_source_backed_conversion_is_local() {
    prove_source_backed_conversion_is_asset_local::<1, true, false>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(96)]
#[kani::solver(cadical)]
#[kani::stub(crate::wide_math::div_rem_u256, bounded_u256_div_rem_stub)]
#[kani::stub(TokenValueFlowProofV16::validate, conversion_flow_validate_stub)]
#[kani::stub(
    PortfolioV16ViewMut::compact_source_domains,
    canonical_conversion_compact_stub
)]
#[kani::stub(PortfolioV16View::validate_with_market, valid_conversion_account_stub)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, valid_conversion_market_stub)]
fn closure_asset_one_short_source_backed_conversion_is_local() {
    prove_source_backed_conversion_is_asset_local::<1, false, false>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(96)]
#[kani::solver(cadical)]
#[kani::stub(crate::wide_math::div_rem_u256, bounded_u256_div_rem_stub)]
#[kani::stub(TokenValueFlowProofV16::validate, conversion_flow_validate_stub)]
#[kani::stub(
    PortfolioV16ViewMut::compact_source_domains,
    canonical_conversion_compact_stub
)]
#[kani::stub(PortfolioV16View::validate_with_market, valid_conversion_account_stub)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, valid_conversion_market_stub)]
fn closure_asset_zero_long_insurance_backed_conversion_is_local() {
    prove_source_backed_conversion_is_asset_local::<0, true, true>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(96)]
#[kani::solver(cadical)]
#[kani::stub(crate::wide_math::div_rem_u256, bounded_u256_div_rem_stub)]
#[kani::stub(TokenValueFlowProofV16::validate, conversion_flow_validate_stub)]
#[kani::stub(
    PortfolioV16ViewMut::compact_source_domains,
    canonical_conversion_compact_stub
)]
#[kani::stub(PortfolioV16View::validate_with_market, valid_conversion_account_stub)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, valid_conversion_market_stub)]
fn closure_asset_zero_short_insurance_backed_conversion_is_local() {
    prove_source_backed_conversion_is_asset_local::<0, false, true>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(96)]
#[kani::solver(cadical)]
#[kani::stub(crate::wide_math::div_rem_u256, bounded_u256_div_rem_stub)]
#[kani::stub(TokenValueFlowProofV16::validate, conversion_flow_validate_stub)]
#[kani::stub(
    PortfolioV16ViewMut::compact_source_domains,
    canonical_conversion_compact_stub
)]
#[kani::stub(PortfolioV16View::validate_with_market, valid_conversion_account_stub)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, valid_conversion_market_stub)]
fn closure_asset_one_long_insurance_backed_conversion_is_local() {
    prove_source_backed_conversion_is_asset_local::<1, true, true>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(96)]
#[kani::solver(cadical)]
#[kani::stub(crate::wide_math::div_rem_u256, bounded_u256_div_rem_stub)]
#[kani::stub(TokenValueFlowProofV16::validate, conversion_flow_validate_stub)]
#[kani::stub(
    PortfolioV16ViewMut::compact_source_domains,
    canonical_conversion_compact_stub
)]
#[kani::stub(PortfolioV16View::validate_with_market, valid_conversion_account_stub)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, valid_conversion_market_stub)]
fn closure_asset_one_short_insurance_backed_conversion_is_local() {
    prove_source_backed_conversion_is_asset_local::<1, false, true>();
}

// The source-backed conversion proofs above establish the maximum capital that
// can be realized from a claim. This companion theorem proves the external leg:
// for every nonzero valid withdrawal amount and capital balance, the public API
// removes exactly that amount from account capital, C_tot, and V, and cannot
// mutate either market slot. Together, conversion backing is the hard upper
// bound on quote atoms that can leave the vault through this route.
#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(96)]
#[kani::solver(cadical)]
#[kani::stub(TokenValueFlowProofV16::validate, withdrawal_flow_validate_stub)]
#[kani::stub(PortfolioV16View::validate_with_market, valid_conversion_account_stub)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, valid_conversion_market_stub)]
#[kani::stub(
    MarketGroupV16ViewMut::settle_negative_pnl_from_principal_core_not_atomic,
    zero_pnl_principal_settlement_stub
)]
fn closure_flat_public_withdrawal_is_conservative_and_asset_local() {
    let capital = kani::any::<u128>();
    let amount = kani::any::<u128>();
    kani::assume(capital > 0 && capital <= MAX_VAULT_TVL - 10);
    kani::assume(amount > 0 && amount <= capital);

    let (mut header, mut markets, mut account_header) = two_asset_kf_mapping_fixture();
    account_header
        .init_empty_in_place(account_header.provenance_header)
        .unwrap();
    header.c_tot = V16PodU128::new(capital);
    header.vault = V16PodU128::new(10 + capital);
    account_header.capital = V16PodU128::new(capital);

    let mut expected_header = header;
    expected_header.c_tot = V16PodU128::new(capital - amount);
    expected_header.vault = V16PodU128::new(10 + capital - amount);
    let expected_markets = [markets[0].engine, markets[1].engine];
    let mut expected_account = account_header;
    expected_account.capital = V16PodU128::new(capital - amount);
    expected_account.health_cert.valid = 0;

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    market.withdraw_not_atomic(&mut account, amount).unwrap();

    kani::cover!(amount == capital, "full capital withdrawal is reachable");
    kani::cover!(amount < capital, "partial capital withdrawal is reachable");
    assert_eq!(
        (10 + capital) - market.header.vault.get(),
        amount,
        "external vault outflow must equal the caller's capital debit"
    );
    assert_eq!(
        capital - market.header.c_tot.get(),
        amount,
        "aggregate capital must fund the same outflow"
    );
    assert!(kani_eq_market_group_v16_header_account(
        &expected_header,
        market.header
    ));
    assert!(kani_eq_portfolio_account_v16_account(
        &expected_account,
        account.header
    ));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &expected_markets[0],
        &market.markets[0].engine
    ));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &expected_markets[1],
        &market.markets[1].engine
    ));
}

// A permissionless market operator may withdraw only the budget of the exact
// domain it controls. Symbolically select any side of either asset and prove
// that the public route debits V, I, and that domain's budget in lockstep while
// every byte of both slots outside that one budget field remains unchanged.
#[cfg(all(kani, feature = "closure"))]
fn prove_domain_insurance_withdrawal_is_cross_asset_isolated<const DOMAIN: usize>() {
    assert!(DOMAIN < 4);
    let amount = kani::any::<u8>();
    let budgets = [
        kani::any::<u8>(),
        kani::any::<u8>(),
        kani::any::<u8>(),
        kani::any::<u8>(),
    ];
    let surplus = kani::any::<u8>();
    kani::assume(amount > 0);
    let selected_budget = budgets[DOMAIN];
    kani::assume(amount <= selected_budget);

    let (mut header, mut markets, _) = two_asset_kf_mapping_fixture();
    markets[0].engine.insurance_domain_budget_long = V16PodU128::new(budgets[0] as u128);
    markets[0].engine.insurance_domain_budget_short = V16PodU128::new(budgets[1] as u128);
    markets[1].engine.insurance_domain_budget_long = V16PodU128::new(budgets[2] as u128);
    markets[1].engine.insurance_domain_budget_short = V16PodU128::new(budgets[3] as u128);
    let total_budget = budgets.iter().map(|value| *value as u128).sum::<u128>();
    let initial_insurance = total_budget + surplus as u128;
    header.insurance_domain_budget_remaining_total = V16PodU128::new(total_budget);
    header.insurance = V16PodU128::new(initial_insurance);
    header.vault = V16PodU128::new(initial_insurance);

    let amount = amount as u128;
    let mut expected_header = header;
    expected_header.insurance_domain_budget_remaining_total =
        V16PodU128::new(total_budget - amount);
    expected_header.insurance = V16PodU128::new(initial_insurance - amount);
    expected_header.vault = V16PodU128::new(initial_insurance - amount);
    let mut expected_markets = [markets[0].engine, markets[1].engine];
    match DOMAIN {
        0 => {
            expected_markets[0].insurance_domain_budget_long =
                V16PodU128::new(budgets[0] as u128 - amount)
        }
        1 => {
            expected_markets[0].insurance_domain_budget_short =
                V16PodU128::new(budgets[1] as u128 - amount)
        }
        2 => {
            expected_markets[1].insurance_domain_budget_long =
                V16PodU128::new(budgets[2] as u128 - amount)
        }
        3 => {
            expected_markets[1].insurance_domain_budget_short =
                V16PodU128::new(budgets[3] as u128 - amount)
        }
        _ => unreachable!(),
    }

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    market
        .withdraw_domain_insurance_not_atomic(DOMAIN, amount)
        .unwrap();

    kani::cover!(
        amount < selected_budget as u128,
        "partial domain withdrawal is reachable"
    );
    kani::cover!(
        amount == selected_budget as u128,
        "full domain withdrawal is reachable"
    );
    assert_eq!(
        initial_insurance - market.header.vault.get(),
        amount,
        "vault outflow must equal the selected domain debit"
    );
    assert_eq!(
        market.header.insurance.get() - market.header.insurance_domain_budget_remaining_total.get(),
        surplus as u128,
        "unbudgeted insurance must remain isolated"
    );
    assert!(kani_eq_market_group_v16_header_account(
        &expected_header,
        market.header
    ));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &expected_markets[0],
        &market.markets[0].engine
    ));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &expected_markets[1],
        &market.markets[1].engine
    ));
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(96)]
#[kani::solver(cadical)]
#[kani::stub(crate::wide_math::div_rem_u256, bounded_u256_div_rem_stub)]
fn closure_asset_zero_long_insurance_withdrawal_is_cross_asset_isolated() {
    prove_domain_insurance_withdrawal_is_cross_asset_isolated::<0>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(96)]
#[kani::solver(cadical)]
#[kani::stub(crate::wide_math::div_rem_u256, bounded_u256_div_rem_stub)]
fn closure_asset_zero_short_insurance_withdrawal_is_cross_asset_isolated() {
    prove_domain_insurance_withdrawal_is_cross_asset_isolated::<1>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(96)]
#[kani::solver(cadical)]
#[kani::stub(crate::wide_math::div_rem_u256, bounded_u256_div_rem_stub)]
fn closure_asset_one_long_insurance_withdrawal_is_cross_asset_isolated() {
    prove_domain_insurance_withdrawal_is_cross_asset_isolated::<2>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(96)]
#[kani::solver(cadical)]
#[kani::stub(crate::wide_math::div_rem_u256, bounded_u256_div_rem_stub)]
fn closure_asset_one_short_insurance_withdrawal_is_cross_asset_isolated() {
    prove_domain_insurance_withdrawal_is_cross_asset_isolated::<3>();
}

// An external insurance deposit creates domain-withdrawable capacity. Prove
// that the quote inflow and insurance stock increase in lockstep and that only
// the requested domain receives the new budget; otherwise the wrong market
// operator could withdraw a donor's insurance despite global conservation.
#[cfg(all(kani, feature = "closure"))]
fn prove_domain_insurance_deposit_is_cross_asset_isolated<const DOMAIN: usize>() {
    assert!(DOMAIN < 4);
    let budgets = [
        kani::any::<u8>(),
        kani::any::<u8>(),
        kani::any::<u8>(),
        kani::any::<u8>(),
    ];
    let deposit = kani::any::<u8>();
    let capital = kani::any::<u8>();
    let unallocated = kani::any::<u8>();
    let junior = kani::any::<u8>();
    kani::assume((1..=8).contains(&budgets[0]) && (1..=8).contains(&budgets[1]));
    kani::assume((1..=8).contains(&budgets[2]) && (1..=8).contains(&budgets[3]));
    kani::assume((1..=8).contains(&deposit));
    kani::assume(capital <= 8 && unallocated <= 8 && junior <= 8);

    let (mut header, mut markets, _) = two_asset_kf_mapping_fixture();
    markets[0].engine.insurance_domain_budget_long = V16PodU128::new(budgets[0] as u128);
    markets[0].engine.insurance_domain_budget_short = V16PodU128::new(budgets[1] as u128);
    markets[1].engine.insurance_domain_budget_long = V16PodU128::new(budgets[2] as u128);
    markets[1].engine.insurance_domain_budget_short = V16PodU128::new(budgets[3] as u128);
    let total_budget = budgets.iter().map(|value| *value as u128).sum::<u128>();
    let insurance = total_budget + unallocated as u128;
    let initial_vault = capital as u128 + insurance + junior as u128;
    header.c_tot = V16PodU128::new(capital as u128);
    header.insurance_domain_budget_remaining_total = V16PodU128::new(total_budget);
    header.insurance = V16PodU128::new(insurance);
    header.vault = V16PodU128::new(initial_vault);

    let deposit = deposit as u128;
    let mut expected_header = header;
    expected_header.insurance_domain_budget_remaining_total =
        V16PodU128::new(total_budget + deposit);
    expected_header.insurance = V16PodU128::new(insurance + deposit);
    expected_header.vault = V16PodU128::new(initial_vault + deposit);
    let mut expected_markets = [markets[0].engine, markets[1].engine];
    match DOMAIN {
        0 => {
            expected_markets[0].insurance_domain_budget_long =
                V16PodU128::new(budgets[0] as u128 + deposit)
        }
        1 => {
            expected_markets[0].insurance_domain_budget_short =
                V16PodU128::new(budgets[1] as u128 + deposit)
        }
        2 => {
            expected_markets[1].insurance_domain_budget_long =
                V16PodU128::new(budgets[2] as u128 + deposit)
        }
        3 => {
            expected_markets[1].insurance_domain_budget_short =
                V16PodU128::new(budgets[3] as u128 + deposit)
        }
        _ => unreachable!(),
    }

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let residual_before = market.residual();
    market
        .deposit_domain_insurance_not_atomic(DOMAIN, deposit)
        .unwrap();

    kani::cover!(deposit > 1, "multi-atom insurance deposit is reachable");
    kani::cover!(
        budgets[DOMAIN] != budgets[(DOMAIN + 1) % 4],
        "selected and neighboring insurance budgets can differ"
    );
    assert_eq!(market.header.vault.get() - initial_vault, deposit);
    assert_eq!(market.header.insurance.get() - insurance, deposit);
    assert_eq!(
        market.header.insurance.get() - market.header.insurance_domain_budget_remaining_total.get(),
        unallocated as u128,
        "deposit cannot relabel preexisting unallocated insurance"
    );
    assert_eq!(
        market.residual(),
        residual_before,
        "insurance deposit cannot inflate junior value"
    );
    assert!(kani_eq_market_group_v16_header_account(
        &expected_header,
        market.header
    ));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &expected_markets[0],
        &market.markets[0].engine
    ));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &expected_markets[1],
        &market.markets[1].engine
    ));
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(96)]
#[kani::solver(cadical)]
#[kani::stub(crate::wide_math::div_rem_u256, bounded_u256_div_rem_stub)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, valid_conversion_market_stub)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_source_domain_ledger,
    valid_domain_ledger_stub
)]
fn closure_asset_zero_long_insurance_deposit_is_cross_asset_isolated() {
    prove_domain_insurance_deposit_is_cross_asset_isolated::<0>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(96)]
#[kani::solver(cadical)]
#[kani::stub(crate::wide_math::div_rem_u256, bounded_u256_div_rem_stub)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, valid_conversion_market_stub)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_source_domain_ledger,
    valid_domain_ledger_stub
)]
fn closure_asset_zero_short_insurance_deposit_is_cross_asset_isolated() {
    prove_domain_insurance_deposit_is_cross_asset_isolated::<1>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(96)]
#[kani::solver(cadical)]
#[kani::stub(crate::wide_math::div_rem_u256, bounded_u256_div_rem_stub)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, valid_conversion_market_stub)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_source_domain_ledger,
    valid_domain_ledger_stub
)]
fn closure_asset_one_long_insurance_deposit_is_cross_asset_isolated() {
    prove_domain_insurance_deposit_is_cross_asset_isolated::<2>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(96)]
#[kani::solver(cadical)]
#[kani::stub(crate::wide_math::div_rem_u256, bounded_u256_div_rem_stub)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, valid_conversion_market_stub)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_source_domain_ledger,
    valid_domain_ledger_stub
)]
fn closure_asset_one_short_insurance_deposit_is_cross_asset_isolated() {
    prove_domain_insurance_deposit_is_cross_asset_isolated::<3>();
}

// Budget crediting is the bridge from shared, already-collected insurance to
// domain-withdrawable capacity. Compose that bridge with an immediate public
// withdrawal while every domain is funded: only the selected budget may gain
// and spend capacity, and neither other domains nor unallocated insurance can
// be drawn through a domain-routing error.
#[cfg(all(kani, feature = "closure"))]
fn prove_budget_credit_then_withdrawal_is_cross_asset_isolated<const DOMAIN: usize>() {
    assert!(DOMAIN < 4);
    let budgets = [
        kani::any::<u8>(),
        kani::any::<u8>(),
        kani::any::<u8>(),
        kani::any::<u8>(),
    ];
    let credit = kani::any::<u8>();
    let withdraw = kani::any::<u8>();
    let capital = kani::any::<u8>();
    let unallocated = kani::any::<u8>();
    let junior = kani::any::<u8>();
    kani::assume((1..=8).contains(&budgets[0]) && (1..=8).contains(&budgets[1]));
    kani::assume((1..=8).contains(&budgets[2]) && (1..=8).contains(&budgets[3]));
    kani::assume((1..=8).contains(&credit));
    kani::assume(withdraw > 0 && withdraw as u16 <= budgets[DOMAIN] as u16 + credit as u16);
    kani::assume(capital <= 8 && unallocated <= 8 && junior <= 8);

    let (mut header, mut markets, _) = two_asset_kf_mapping_fixture();
    markets[0].engine.insurance_domain_budget_long = V16PodU128::new(budgets[0] as u128);
    markets[0].engine.insurance_domain_budget_short = V16PodU128::new(budgets[1] as u128);
    markets[1].engine.insurance_domain_budget_long = V16PodU128::new(budgets[2] as u128);
    markets[1].engine.insurance_domain_budget_short = V16PodU128::new(budgets[3] as u128);
    let total_budget = budgets.iter().map(|value| *value as u128).sum::<u128>();
    let credit = credit as u128;
    let withdraw = withdraw as u128;
    let insurance = total_budget + credit + unallocated as u128;
    let initial_vault = capital as u128 + insurance + junior as u128;
    header.c_tot = V16PodU128::new(capital as u128);
    header.insurance_domain_budget_remaining_total = V16PodU128::new(total_budget);
    header.insurance = V16PodU128::new(insurance);
    header.vault = V16PodU128::new(initial_vault);

    let mut expected_header = header;
    expected_header.insurance_domain_budget_remaining_total =
        V16PodU128::new(total_budget + credit - withdraw);
    expected_header.insurance = V16PodU128::new(insurance - withdraw);
    expected_header.vault = V16PodU128::new(initial_vault - withdraw);
    let mut expected_markets = [markets[0].engine, markets[1].engine];
    let selected_final = budgets[DOMAIN] as u128 + credit - withdraw;
    match DOMAIN {
        0 => expected_markets[0].insurance_domain_budget_long = V16PodU128::new(selected_final),
        1 => expected_markets[0].insurance_domain_budget_short = V16PodU128::new(selected_final),
        2 => expected_markets[1].insurance_domain_budget_long = V16PodU128::new(selected_final),
        3 => expected_markets[1].insurance_domain_budget_short = V16PodU128::new(selected_final),
        _ => unreachable!(),
    }

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let residual_before = market.residual();
    market
        .credit_domain_insurance_budget_not_atomic(DOMAIN, credit)
        .unwrap();
    market
        .withdraw_domain_insurance_not_atomic(DOMAIN, withdraw)
        .unwrap();

    kani::cover!(
        withdraw < budgets[DOMAIN] as u128 + credit,
        "credited domain supports a partial withdrawal"
    );
    kani::cover!(
        withdraw == budgets[DOMAIN] as u128 + credit,
        "credited domain supports a full withdrawal"
    );
    kani::cover!(
        withdraw > budgets[DOMAIN] as u128,
        "withdrawal can consume newly credited capacity"
    );
    assert_eq!(initial_vault - market.header.vault.get(), withdraw);
    assert_eq!(
        market.header.insurance.get() - market.header.insurance_domain_budget_remaining_total.get(),
        unallocated as u128,
        "unallocated insurance cannot leak through a selected domain"
    );
    assert_eq!(
        market.residual(),
        residual_before,
        "budget relabeling and withdrawal cannot consume junior value"
    );
    assert!(kani_eq_market_group_v16_header_account(
        &expected_header,
        market.header
    ));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &expected_markets[0],
        &market.markets[0].engine
    ));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &expected_markets[1],
        &market.markets[1].engine
    ));
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(96)]
#[kani::solver(cadical)]
#[kani::stub(crate::wide_math::div_rem_u256, bounded_u256_div_rem_stub)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, valid_conversion_market_stub)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_source_domain_ledger,
    valid_domain_ledger_stub
)]
fn closure_asset_zero_long_budget_credit_then_withdrawal_is_isolated() {
    prove_budget_credit_then_withdrawal_is_cross_asset_isolated::<0>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(96)]
#[kani::solver(cadical)]
#[kani::stub(crate::wide_math::div_rem_u256, bounded_u256_div_rem_stub)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, valid_conversion_market_stub)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_source_domain_ledger,
    valid_domain_ledger_stub
)]
fn closure_asset_zero_short_budget_credit_then_withdrawal_is_isolated() {
    prove_budget_credit_then_withdrawal_is_cross_asset_isolated::<1>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(96)]
#[kani::solver(cadical)]
#[kani::stub(crate::wide_math::div_rem_u256, bounded_u256_div_rem_stub)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, valid_conversion_market_stub)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_source_domain_ledger,
    valid_domain_ledger_stub
)]
fn closure_asset_one_long_budget_credit_then_withdrawal_is_isolated() {
    prove_budget_credit_then_withdrawal_is_cross_asset_isolated::<2>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(96)]
#[kani::solver(cadical)]
#[kani::stub(crate::wide_math::div_rem_u256, bounded_u256_div_rem_stub)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, valid_conversion_market_stub)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_source_domain_ledger,
    valid_domain_ledger_stub
)]
fn closure_asset_one_short_budget_credit_then_withdrawal_is_isolated() {
    prove_budget_credit_then_withdrawal_is_cross_asset_isolated::<3>();
}

#[cfg(all(kani, feature = "closure"))]
fn install_fresh_backing_fixture(
    slot: &mut EngineAssetSlotV16Account,
    side: SideV16,
    amount: u128,
    credit_epoch: u64,
) {
    let amount_num = amount * BOUND_SCALE;
    let bucket = BackingBucketV16Account::from_runtime(&BackingBucketV16 {
        market_id: slot.asset.market_id.get(),
        fresh_unliened_backing_num: amount_num,
        expiry_slot: if amount == 0 { 0 } else { 10 },
        status: if amount == 0 {
            BackingBucketStatusV16::Empty
        } else {
            BackingBucketStatusV16::Fresh
        },
        ..BackingBucketV16::EMPTY
    });
    let source = SourceCreditStateV16Account::from_runtime(&SourceCreditStateV16 {
        fresh_reserved_backing_num: amount_num,
        credit_rate_num: CREDIT_RATE_SCALE,
        credit_epoch,
        ..SourceCreditStateV16::EMPTY
    });
    match side {
        SideV16::Long => {
            slot.backing_long = bucket;
            slot.source_credit_long = source;
        }
        SideV16::Short => {
            slot.backing_short = bucket;
            slot.source_credit_short = source;
        }
    }
}

// External backing deposits create senior, provider-withdrawable principal.
// Keep every domain live with heterogeneous principal and prove that only the
// selected domain receives the deposit; otherwise another permissionless
// operator could withdraw the depositor's value through an otherwise-correct
// local withdrawal API.
#[cfg(all(kani, feature = "closure"))]
fn prove_backing_deposit_is_cross_asset_isolated<const DOMAIN: usize>() {
    assert!(DOMAIN < 4);
    let amounts = [
        kani::any::<u8>(),
        kani::any::<u8>(),
        kani::any::<u8>(),
        kani::any::<u8>(),
    ];
    let deposit = kani::any::<u8>();
    let capital = kani::any::<u8>();
    let junior = kani::any::<u8>();
    kani::assume((1..=8).contains(&amounts[0]) && (1..=8).contains(&amounts[1]));
    kani::assume((1..=8).contains(&amounts[2]) && (1..=8).contains(&amounts[3]));
    kani::assume((1..=8).contains(&deposit));
    kani::assume(capital <= 8 && junior <= 8);

    let (mut header, mut markets, _) = two_asset_kf_mapping_fixture();
    install_fresh_backing_fixture(&mut markets[0].engine, SideV16::Long, amounts[0] as u128, 0);
    install_fresh_backing_fixture(
        &mut markets[0].engine,
        SideV16::Short,
        amounts[1] as u128,
        0,
    );
    install_fresh_backing_fixture(&mut markets[1].engine, SideV16::Long, amounts[2] as u128, 0);
    install_fresh_backing_fixture(
        &mut markets[1].engine,
        SideV16::Short,
        amounts[3] as u128,
        0,
    );
    let total = amounts.iter().map(|value| *value as u128).sum::<u128>();
    let initial_vault = capital as u128 + 10 + total + junior as u128;
    header.c_tot = V16PodU128::new(capital as u128);
    header.source_fresh_backing_total_num = V16PodU128::new(total * BOUND_SCALE);
    header.vault = V16PodU128::new(initial_vault);

    let deposit = deposit as u128;
    let mut expected_header = header;
    expected_header.source_fresh_backing_total_num =
        V16PodU128::new((total + deposit) * BOUND_SCALE);
    expected_header.vault = V16PodU128::new(initial_vault + deposit);
    expected_header.risk_epoch = V16PodU64::new(header.risk_epoch.get() + 1);
    let mut expected_markets = [markets[0].engine, markets[1].engine];
    install_fresh_backing_fixture(
        &mut expected_markets[DOMAIN / 2],
        if DOMAIN % 2 == 0 {
            SideV16::Long
        } else {
            SideV16::Short
        },
        amounts[DOMAIN] as u128 + deposit,
        1,
    );

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let residual_before = market.residual();
    market
        .deposit_fresh_counterparty_backing_not_atomic(DOMAIN, deposit, 10)
        .unwrap();

    kani::cover!(deposit > 1, "multi-atom backing deposit is reachable");
    kani::cover!(
        amounts[DOMAIN] != amounts[(DOMAIN + 1) % 4],
        "selected and neighboring domains can hold different principal"
    );
    assert_eq!(market.header.vault.get() - initial_vault, deposit);
    assert_eq!(
        market.header.source_fresh_backing_total_num.get() - total * BOUND_SCALE,
        deposit * BOUND_SCALE,
        "vault atoms and scaled backing must increase in lockstep"
    );
    assert_eq!(
        market.residual(),
        residual_before,
        "a backing deposit cannot inflate junior value"
    );
    assert!(kani_eq_market_group_v16_header_account(
        &expected_header,
        market.header
    ));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &expected_markets[0],
        &market.markets[0].engine
    ));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &expected_markets[1],
        &market.markets[1].engine
    ));
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(96)]
#[kani::solver(cadical)]
#[kani::stub(crate::wide_math::div_rem_u256, bounded_u256_div_rem_stub)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, valid_conversion_market_stub)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_source_domain_ledger,
    valid_domain_ledger_stub
)]
fn closure_asset_zero_long_backing_deposit_is_cross_asset_isolated() {
    prove_backing_deposit_is_cross_asset_isolated::<0>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(96)]
#[kani::solver(cadical)]
#[kani::stub(crate::wide_math::div_rem_u256, bounded_u256_div_rem_stub)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, valid_conversion_market_stub)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_source_domain_ledger,
    valid_domain_ledger_stub
)]
fn closure_asset_zero_short_backing_deposit_is_cross_asset_isolated() {
    prove_backing_deposit_is_cross_asset_isolated::<1>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(96)]
#[kani::solver(cadical)]
#[kani::stub(crate::wide_math::div_rem_u256, bounded_u256_div_rem_stub)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, valid_conversion_market_stub)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_source_domain_ledger,
    valid_domain_ledger_stub
)]
fn closure_asset_one_long_backing_deposit_is_cross_asset_isolated() {
    prove_backing_deposit_is_cross_asset_isolated::<2>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(96)]
#[kani::solver(cadical)]
#[kani::stub(crate::wide_math::div_rem_u256, bounded_u256_div_rem_stub)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, valid_conversion_market_stub)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_source_domain_ledger,
    valid_domain_ledger_stub
)]
fn closure_asset_one_short_backing_deposit_is_cross_asset_isolated() {
    prove_backing_deposit_is_cross_asset_isolated::<3>();
}

// Counterparty backing is provider-owned principal, but a provider for one
// permissionless domain must never extract another domain's principal. Each
// instance keeps all four domains nonzero and symbolic, withdraws from exactly
// one, and frames both complete market slots plus every global stock counter.
#[cfg(all(kani, feature = "closure"))]
fn prove_backing_withdrawal_is_cross_asset_isolated<const DOMAIN: usize>() {
    assert!(DOMAIN < 4);
    let amounts = [
        kani::any::<u8>(),
        kani::any::<u8>(),
        kani::any::<u8>(),
        kani::any::<u8>(),
    ];
    let withdraw = kani::any::<u8>();
    kani::assume((1..=8).contains(&amounts[0]) && (1..=8).contains(&amounts[1]));
    kani::assume((1..=8).contains(&amounts[2]) && (1..=8).contains(&amounts[3]));
    kani::assume(withdraw > 0 && withdraw <= amounts[DOMAIN]);

    let (mut header, mut markets, _) = two_asset_kf_mapping_fixture();
    install_fresh_backing_fixture(&mut markets[0].engine, SideV16::Long, amounts[0] as u128, 0);
    install_fresh_backing_fixture(
        &mut markets[0].engine,
        SideV16::Short,
        amounts[1] as u128,
        0,
    );
    install_fresh_backing_fixture(&mut markets[1].engine, SideV16::Long, amounts[2] as u128, 0);
    install_fresh_backing_fixture(
        &mut markets[1].engine,
        SideV16::Short,
        amounts[3] as u128,
        0,
    );
    let total = amounts.iter().map(|value| *value as u128).sum::<u128>();
    header.source_fresh_backing_total_num = V16PodU128::new(total * BOUND_SCALE);
    header.vault = V16PodU128::new(10 + total);

    let withdraw = withdraw as u128;
    let remaining = amounts[DOMAIN] as u128 - withdraw;
    let mut expected_header = header;
    expected_header.source_fresh_backing_total_num =
        V16PodU128::new((total - withdraw) * BOUND_SCALE);
    expected_header.vault = V16PodU128::new(10 + total - withdraw);
    expected_header.risk_epoch = V16PodU64::new(header.risk_epoch.get() + 1);
    let mut expected_markets = [markets[0].engine, markets[1].engine];
    install_fresh_backing_fixture(
        &mut expected_markets[DOMAIN / 2],
        if DOMAIN % 2 == 0 {
            SideV16::Long
        } else {
            SideV16::Short
        },
        remaining,
        1,
    );

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    market
        .withdraw_fresh_counterparty_backing_not_atomic(DOMAIN, withdraw)
        .unwrap();

    kani::cover!(
        withdraw < amounts[DOMAIN] as u128,
        "partial backing withdrawal is reachable"
    );
    kani::cover!(
        withdraw == amounts[DOMAIN] as u128,
        "full backing withdrawal is reachable"
    );
    assert_eq!(
        (10 + total) - market.header.vault.get(),
        withdraw,
        "vault outflow must equal selected backing principal"
    );
    assert_eq!(
        market.header.insurance.get(),
        10,
        "backing withdrawal cannot consume insurance"
    );
    assert!(kani_eq_market_group_v16_header_account(
        &expected_header,
        market.header
    ));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &expected_markets[0],
        &market.markets[0].engine
    ));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &expected_markets[1],
        &market.markets[1].engine
    ));
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(96)]
#[kani::solver(cadical)]
#[kani::stub(crate::wide_math::div_rem_u256, bounded_u256_div_rem_stub)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, valid_conversion_market_stub)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_source_domain_ledger,
    valid_domain_ledger_stub
)]
fn closure_asset_zero_long_backing_withdrawal_is_cross_asset_isolated() {
    prove_backing_withdrawal_is_cross_asset_isolated::<0>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(96)]
#[kani::solver(cadical)]
#[kani::stub(crate::wide_math::div_rem_u256, bounded_u256_div_rem_stub)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, valid_conversion_market_stub)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_source_domain_ledger,
    valid_domain_ledger_stub
)]
fn closure_asset_zero_short_backing_withdrawal_is_cross_asset_isolated() {
    prove_backing_withdrawal_is_cross_asset_isolated::<1>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(96)]
#[kani::solver(cadical)]
#[kani::stub(crate::wide_math::div_rem_u256, bounded_u256_div_rem_stub)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, valid_conversion_market_stub)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_source_domain_ledger,
    valid_domain_ledger_stub
)]
fn closure_asset_one_long_backing_withdrawal_is_cross_asset_isolated() {
    prove_backing_withdrawal_is_cross_asset_isolated::<2>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(96)]
#[kani::solver(cadical)]
#[kani::stub(crate::wide_math::div_rem_u256, bounded_u256_div_rem_stub)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, valid_conversion_market_stub)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_source_domain_ledger,
    valid_domain_ledger_stub
)]
fn closure_asset_one_short_backing_withdrawal_is_cross_asset_isolated() {
    prove_backing_withdrawal_is_cross_asset_isolated::<3>();
}

#[cfg(all(kani, feature = "closure"))]
fn install_provider_earnings_fixture(
    slot: &mut EngineAssetSlotV16Account,
    side: SideV16,
    earnings: u128,
) {
    let bucket = BackingBucketV16Account::from_runtime(&BackingBucketV16 {
        market_id: slot.asset.market_id.get(),
        utilization_fee_earnings: earnings,
        status: BackingBucketStatusV16::Expired,
        ..BackingBucketV16::EMPTY
    });
    match side {
        SideV16::Long => slot.backing_long = bucket,
        SideV16::Short => slot.backing_short = bucket,
    }
}

// Provider earnings are senior claims held in a shared vault, but authority is
// domain-local. Keep every domain funded and prove that a withdrawal can debit
// only its selected earnings bucket; the global earnings class and vault move
// by the same amount while insurance and junior surplus remain isolated.
#[cfg(all(kani, feature = "closure"))]
fn prove_provider_earnings_withdrawal_is_cross_asset_isolated<const DOMAIN: usize>() {
    assert!(DOMAIN < 4);
    let earnings = [
        kani::any::<u8>(),
        kani::any::<u8>(),
        kani::any::<u8>(),
        kani::any::<u8>(),
    ];
    let withdraw = kani::any::<u8>();
    let surplus = kani::any::<u8>();
    kani::assume((1..=8).contains(&earnings[0]) && (1..=8).contains(&earnings[1]));
    kani::assume((1..=8).contains(&earnings[2]) && (1..=8).contains(&earnings[3]));
    kani::assume(withdraw > 0 && withdraw <= earnings[DOMAIN]);
    kani::assume(surplus <= 8);

    let (mut header, mut markets, _) = two_asset_kf_mapping_fixture();
    install_provider_earnings_fixture(&mut markets[0].engine, SideV16::Long, earnings[0] as u128);
    install_provider_earnings_fixture(&mut markets[0].engine, SideV16::Short, earnings[1] as u128);
    install_provider_earnings_fixture(&mut markets[1].engine, SideV16::Long, earnings[2] as u128);
    install_provider_earnings_fixture(&mut markets[1].engine, SideV16::Short, earnings[3] as u128);
    let total = earnings.iter().map(|value| *value as u128).sum::<u128>();
    header.backing_provider_earnings_total = V16PodU128::new(total);
    header.vault = V16PodU128::new(10 + total + surplus as u128);
    let initial_vault = header.vault.get();

    let withdraw = withdraw as u128;
    let mut expected_header = header;
    expected_header.backing_provider_earnings_total = V16PodU128::new(total - withdraw);
    expected_header.vault = V16PodU128::new(initial_vault - withdraw);
    let mut expected_markets = [markets[0].engine, markets[1].engine];
    install_provider_earnings_fixture(
        &mut expected_markets[DOMAIN / 2],
        if DOMAIN % 2 == 0 {
            SideV16::Long
        } else {
            SideV16::Short
        },
        earnings[DOMAIN] as u128 - withdraw,
    );

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let residual_before = market.residual();
    market
        .withdraw_backing_provider_earnings_not_atomic(DOMAIN, withdraw)
        .unwrap();

    kani::cover!(
        withdraw < earnings[DOMAIN] as u128,
        "partial provider-earnings withdrawal is reachable"
    );
    kani::cover!(
        withdraw == earnings[DOMAIN] as u128,
        "full provider-earnings withdrawal is reachable"
    );
    assert_eq!(
        initial_vault - market.header.vault.get(),
        withdraw,
        "vault outflow must equal the selected earnings debit"
    );
    assert_eq!(
        market.residual(),
        residual_before,
        "provider earnings cannot consume junior surplus"
    );
    assert!(kani_eq_market_group_v16_header_account(
        &expected_header,
        market.header
    ));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &expected_markets[0],
        &market.markets[0].engine
    ));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &expected_markets[1],
        &market.markets[1].engine
    ));
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(96)]
#[kani::solver(cadical)]
#[kani::stub(crate::wide_math::div_rem_u256, bounded_u256_div_rem_stub)]
fn closure_asset_zero_long_provider_earnings_withdrawal_is_cross_asset_isolated() {
    prove_provider_earnings_withdrawal_is_cross_asset_isolated::<0>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(96)]
#[kani::solver(cadical)]
#[kani::stub(crate::wide_math::div_rem_u256, bounded_u256_div_rem_stub)]
fn closure_asset_zero_short_provider_earnings_withdrawal_is_cross_asset_isolated() {
    prove_provider_earnings_withdrawal_is_cross_asset_isolated::<1>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(96)]
#[kani::solver(cadical)]
#[kani::stub(crate::wide_math::div_rem_u256, bounded_u256_div_rem_stub)]
fn closure_asset_one_long_provider_earnings_withdrawal_is_cross_asset_isolated() {
    prove_provider_earnings_withdrawal_is_cross_asset_isolated::<2>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(96)]
#[kani::solver(cadical)]
#[kani::stub(crate::wide_math::div_rem_u256, bounded_u256_div_rem_stub)]
fn closure_asset_one_short_provider_earnings_withdrawal_is_cross_asset_isolated() {
    prove_provider_earnings_withdrawal_is_cross_asset_isolated::<3>();
}

#[cfg(all(kani, feature = "closure"))]
fn install_fee_domain_fixture(
    slot: &mut EngineAssetSlotV16Account,
    side: SideV16,
    earnings: u128,
    credit_epoch: u64,
) {
    let principal = BOUND_SCALE;
    let bucket = BackingBucketV16Account::from_runtime(&BackingBucketV16 {
        market_id: slot.asset.market_id.get(),
        fresh_unliened_backing_num: principal,
        expiry_slot: 10,
        status: BackingBucketStatusV16::Fresh,
        utilization_fee_earnings: earnings,
        ..BackingBucketV16::EMPTY
    });
    let source = SourceCreditStateV16Account::from_runtime(&SourceCreditStateV16 {
        fresh_reserved_backing_num: principal,
        credit_rate_num: CREDIT_RATE_SCALE,
        credit_epoch,
        ..SourceCreditStateV16::EMPTY
    });
    match side {
        SideV16::Long => {
            slot.backing_long = bucket;
            slot.source_credit_long = source;
        }
        SideV16::Short => {
            slot.backing_short = bucket;
            slot.source_credit_short = source;
        }
    }
}

// The wrapper routes both components of an account backing fee to the source
// domain being charged. Prove the complete successful transition for every
// domain while all four domains carry withdrawable value: user capital and
// c_tot fall by the total fee, only the selected provider/insurance ledgers
// rise, vault value and total senior claims remain constant, and post-fee IM
// stays valid. A domain-routing error would transfer user value to another
// permissionless operator.
#[cfg(all(kani, feature = "closure"))]
fn prove_account_backing_fee_is_cross_asset_isolated<const DOMAIN: usize>() {
    assert!(DOMAIN < 4);
    let earnings = [
        kani::any::<u8>(),
        kani::any::<u8>(),
        kani::any::<u8>(),
        kani::any::<u8>(),
    ];
    let provider_fee = kani::any::<u8>();
    let insurance_fee = kani::any::<u8>();
    let margin_slack = kani::any::<u8>();
    let surplus = kani::any::<u8>();
    kani::assume(earnings[0] <= 4 && earnings[1] <= 4);
    kani::assume(earnings[2] <= 4 && earnings[3] <= 4);
    kani::assume(provider_fee <= 4 && insurance_fee <= 4);
    kani::assume(provider_fee > 0 || insurance_fee > 0);
    kani::assume((1..=8).contains(&margin_slack));
    kani::assume(surplus <= 8);

    let provider_fee = provider_fee as u128;
    let insurance_fee = insurance_fee as u128;
    let total_fee = provider_fee + insurance_fee;
    let capital = total_fee + margin_slack as u128;
    let total_earnings = earnings.iter().map(|value| *value as u128).sum::<u128>();
    let (mut header, mut markets, mut account_header) = two_asset_kf_mapping_fixture();
    account_header
        .init_empty_in_place(account_header.provenance_header)
        .unwrap();
    let credit_epoch = header.risk_epoch.get();
    install_fee_domain_fixture(
        &mut markets[0].engine,
        SideV16::Long,
        earnings[0] as u128,
        credit_epoch,
    );
    install_fee_domain_fixture(
        &mut markets[0].engine,
        SideV16::Short,
        earnings[1] as u128,
        credit_epoch,
    );
    install_fee_domain_fixture(
        &mut markets[1].engine,
        SideV16::Long,
        earnings[2] as u128,
        credit_epoch,
    );
    install_fee_domain_fixture(
        &mut markets[1].engine,
        SideV16::Short,
        earnings[3] as u128,
        credit_epoch,
    );
    header.c_tot = V16PodU128::new(capital);
    header.backing_provider_earnings_total = V16PodU128::new(total_earnings);
    header.source_fresh_backing_total_num = V16PodU128::new(4 * BOUND_SCALE);
    header.vault = V16PodU128::new(capital + 10 + total_earnings + 4 + surplus as u128);
    account_header.capital = V16PodU128::new(capital);
    account_header.health_cert = HealthCertV16Account::from_runtime(&HealthCertV16 {
        certified_equity: capital as i128,
        certified_initial_req: margin_slack as u128,
        certified_maintenance_req: margin_slack as u128,
        cert_oracle_epoch: header.oracle_epoch.get(),
        cert_funding_epoch: header.funding_epoch.get(),
        cert_risk_epoch: header.risk_epoch.get(),
        cert_asset_set_epoch: header.asset_set_epoch.get(),
        active_bitmap_at_cert: V16_EMPTY_ACTIVE_BITMAP,
        valid: true,
        ..HealthCertV16::default()
    });

    let initial_vault = header.vault.get();
    let initial_senior =
        header.c_tot.get() + header.insurance.get() + header.backing_provider_earnings_total.get();
    let mut expected_header = header;
    expected_header.c_tot = V16PodU128::new(capital - total_fee);
    expected_header.insurance = V16PodU128::new(header.insurance.get() + insurance_fee);
    expected_header.backing_provider_earnings_total =
        V16PodU128::new(total_earnings + provider_fee);
    expected_header.insurance_domain_budget_remaining_total =
        V16PodU128::new(header.insurance_domain_budget_remaining_total.get() + insurance_fee);
    let mut expected_markets = [markets[0].engine, markets[1].engine];
    install_fee_domain_fixture(
        &mut expected_markets[DOMAIN / 2],
        if DOMAIN % 2 == 0 {
            SideV16::Long
        } else {
            SideV16::Short
        },
        earnings[DOMAIN] as u128 + provider_fee,
        credit_epoch,
    );
    match DOMAIN {
        0 => expected_markets[0].insurance_domain_budget_long = V16PodU128::new(1 + insurance_fee),
        1 => expected_markets[0].insurance_domain_budget_short = V16PodU128::new(2 + insurance_fee),
        2 => expected_markets[1].insurance_domain_budget_long = V16PodU128::new(3 + insurance_fee),
        3 => expected_markets[1].insurance_domain_budget_short = V16PodU128::new(4 + insurance_fee),
        _ => unreachable!(),
    }
    let mut expected_account = account_header;
    expected_account.capital = V16PodU128::new(capital - total_fee);
    let mut expected_cert = expected_account.health_cert.try_to_runtime().unwrap();
    expected_cert.certified_equity -= total_fee as i128;
    expected_account.health_cert = HealthCertV16Account::from_runtime(&expected_cert);

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    let charged = market
        .charge_account_backing_fee_not_atomic(
            &mut account,
            DOMAIN,
            provider_fee,
            DOMAIN,
            insurance_fee,
        )
        .unwrap();

    kani::cover!(
        provider_fee > 0 && insurance_fee > 0,
        "mixed provider and insurance fee routing is reachable"
    );
    kani::cover!(
        provider_fee > 0 && insurance_fee == 0,
        "provider-only fee routing is reachable"
    );
    kani::cover!(
        provider_fee == 0 && insurance_fee > 0,
        "insurance-only fee routing is reachable"
    );
    assert_eq!(charged, total_fee);
    assert_eq!(market.header.vault.get(), initial_vault);
    assert_eq!(
        market.header.c_tot.get()
            + market.header.insurance.get()
            + market.header.backing_provider_earnings_total.get(),
        initial_senior,
        "fee routing must conserve total senior claims"
    );
    assert_eq!(
        account.header.health_cert.certified_equity.get(),
        margin_slack as i128,
        "the exact post-fee IM boundary must remain valid"
    );
    assert!(kani_eq_market_group_v16_header_account(
        &expected_header,
        market.header
    ));
    assert!(kani_eq_portfolio_account_v16_account(
        &expected_account,
        account.header
    ));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &expected_markets[0],
        &market.markets[0].engine
    ));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &expected_markets[1],
        &market.markets[1].engine
    ));
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(112)]
#[kani::solver(cadical)]
#[kani::stub(crate::wide_math::div_rem_u256, bounded_u256_div_rem_stub)]
#[kani::stub(PortfolioV16View::validate_with_market, valid_conversion_account_stub)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, valid_conversion_market_stub)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_source_domain_ledger,
    valid_domain_ledger_stub
)]
fn closure_asset_zero_long_backing_fee_is_cross_asset_isolated() {
    prove_account_backing_fee_is_cross_asset_isolated::<0>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(112)]
#[kani::solver(cadical)]
#[kani::stub(crate::wide_math::div_rem_u256, bounded_u256_div_rem_stub)]
#[kani::stub(PortfolioV16View::validate_with_market, valid_conversion_account_stub)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, valid_conversion_market_stub)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_source_domain_ledger,
    valid_domain_ledger_stub
)]
fn closure_asset_zero_short_backing_fee_is_cross_asset_isolated() {
    prove_account_backing_fee_is_cross_asset_isolated::<1>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(112)]
#[kani::solver(cadical)]
#[kani::stub(crate::wide_math::div_rem_u256, bounded_u256_div_rem_stub)]
#[kani::stub(PortfolioV16View::validate_with_market, valid_conversion_account_stub)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, valid_conversion_market_stub)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_source_domain_ledger,
    valid_domain_ledger_stub
)]
fn closure_asset_one_long_backing_fee_is_cross_asset_isolated() {
    prove_account_backing_fee_is_cross_asset_isolated::<2>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(112)]
#[kani::solver(cadical)]
#[kani::stub(crate::wide_math::div_rem_u256, bounded_u256_div_rem_stub)]
#[kani::stub(PortfolioV16View::validate_with_market, valid_conversion_account_stub)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, valid_conversion_market_stub)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_source_domain_ledger,
    valid_domain_ledger_stub
)]
fn closure_asset_one_short_backing_fee_is_cross_asset_isolated() {
    prove_account_backing_fee_is_cross_asset_isolated::<3>();
}

// Asset-0 authority may force an individual asset into Recovery, but this
// control is bounded: it freezes the selected asset at its committed effective
// mark, moves no value, touches no other asset, bumps epochs once, and is then
// idempotent. This is the engine half of safe permissionless-market shutdown.
#[cfg(all(kani, feature = "closure"))]
fn prove_forced_asset_recovery_is_value_neutral_local_and_idempotent<const ASSET: usize>() {
    assert!(ASSET < 2);
    let starts_drain_only = kani::any::<bool>();
    let capital = kani::any::<u8>();
    let extra_insurance = kani::any::<u8>();
    let surplus = kani::any::<u8>();
    kani::assume(capital <= 8 && extra_insurance <= 8 && surplus <= 8);

    let (mut header, mut markets, _) = two_asset_kf_mapping_fixture();
    let insurance = 10 + extra_insurance as u128;
    header.c_tot = V16PodU128::new(capital as u128);
    header.insurance = V16PodU128::new(insurance);
    header.vault = V16PodU128::new(capital as u128 + insurance + surplus as u128);
    let mut selected = markets[ASSET].engine.asset.try_to_runtime().unwrap();
    selected.lifecycle = if starts_drain_only {
        AssetLifecycleV16::DrainOnly
    } else {
        AssetLifecycleV16::Active
    };
    selected.raw_oracle_target_price = 150;
    selected.effective_price = 100;
    markets[ASSET].engine.asset = AssetStateV16Account::from_runtime(&selected);

    let mut expected_header = header;
    expected_header.asset_set_epoch = V16PodU64::new(header.asset_set_epoch.get() + 1);
    expected_header.risk_epoch = V16PodU64::new(header.risk_epoch.get() + 1);
    let mut expected_markets = [markets[0].engine, markets[1].engine];
    selected.lifecycle = AssetLifecycleV16::Recovery;
    selected.raw_oracle_target_price = selected.effective_price;
    expected_markets[ASSET].asset = AssetStateV16Account::from_runtime(&selected);

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    market.force_asset_recovery_not_atomic(ASSET, 2).unwrap();
    let first_header = *market.header;
    let first_markets = [market.markets[0].engine, market.markets[1].engine];
    market.force_asset_recovery_not_atomic(ASSET, 2).unwrap();

    kani::cover!(!starts_drain_only, "forced recovery covers an active asset");
    kani::cover!(
        starts_drain_only,
        "forced recovery covers a drain-only asset"
    );
    assert!(kani_eq_market_group_v16_header_account(
        &expected_header,
        &first_header
    ));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &expected_markets[0],
        &first_markets[0]
    ));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &expected_markets[1],
        &first_markets[1]
    ));
    assert!(kani_eq_market_group_v16_header_account(
        &first_header,
        market.header
    ));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &first_markets[0],
        &market.markets[0].engine
    ));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &first_markets[1],
        &market.markets[1].engine
    ));
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(96)]
#[kani::solver(cadical)]
#[kani::stub(crate::wide_math::div_rem_u256, bounded_u256_div_rem_stub)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, valid_conversion_market_stub)]
fn closure_forced_asset_zero_recovery_is_bounded_and_idempotent() {
    prove_forced_asset_recovery_is_value_neutral_local_and_idempotent::<0>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(96)]
#[kani::solver(cadical)]
#[kani::stub(crate::wide_math::div_rem_u256, bounded_u256_div_rem_stub)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, valid_conversion_market_stub)]
fn closure_forced_asset_one_recovery_is_bounded_and_idempotent() {
    prove_forced_asset_recovery_is_value_neutral_local_and_idempotent::<1>();
}

// Safe shutdown liveness over the wrapper-used public forfeit route. Once an
// asset is in Recovery, a clean current leg must detach in one call without
// moving any token-value stock or touching another asset. The selected side's
// OI/count/weight and the global payout-blocker count fall exactly once; the
// opposite side remains available for its owner to detach independently.
#[cfg(all(kani, feature = "closure"))]
fn prove_clean_recovery_leg_forfeit_is_value_neutral_and_asset_local<
    const ASSET: usize,
    const LONG: bool,
>() {
    assert!(ASSET < 2);
    let side = if LONG { SideV16::Long } else { SideV16::Short };
    let (mut header, mut markets, mut account_header) = two_asset_kf_mapping_fixture();
    account_header
        .init_empty_in_place(account_header.provenance_header)
        .unwrap();

    let mut asset = markets[ASSET].engine.asset.try_to_runtime().unwrap();
    asset.lifecycle = AssetLifecycleV16::Recovery;
    asset.oi_eff_long_q = POS_SCALE;
    asset.oi_eff_short_q = POS_SCALE;
    asset.stored_pos_count_long = 1;
    asset.stored_pos_count_short = 1;
    asset.loss_weight_sum_long = POS_SCALE;
    asset.loss_weight_sum_short = POS_SCALE;
    markets[ASSET].engine.asset = AssetStateV16Account::from_runtime(&asset);
    header.resolved_payout_blocker_count = V16PodU64::new(2);

    account_header.active_bitmap[0] = V16PodU64::new(1);
    account_header.legs[0] = PortfolioLegV16Account::from_runtime(&PortfolioLegV16 {
        active: true,
        asset_index: ASSET as u32,
        market_id: asset.market_id,
        side,
        basis_pos_q: match side {
            SideV16::Long => POS_SCALE as i128,
            SideV16::Short => -(POS_SCALE as i128),
        },
        a_basis: ADL_ONE,
        k_snap: match side {
            SideV16::Long => asset.k_long,
            SideV16::Short => asset.k_short,
        },
        f_snap: match side {
            SideV16::Long => asset.f_long_num,
            SideV16::Short => asset.f_short_num,
        },
        epoch_snap: match side {
            SideV16::Long => asset.epoch_long,
            SideV16::Short => asset.epoch_short,
        },
        loss_weight: POS_SCALE,
        b_snap: match side {
            SideV16::Long => asset.b_long_num,
            SideV16::Short => asset.b_short_num,
        },
        b_epoch_snap: match side {
            SideV16::Long => asset.epoch_long,
            SideV16::Short => asset.epoch_short,
        },
        ..PortfolioLegV16::EMPTY
    });

    let header_before = header;
    let account_before = account_header;
    let markets_before = [markets[0].engine, markets[1].engine];
    let other = 1 - ASSET;
    let mut expected_header = header_before;
    expected_header.resolved_payout_blocker_count = V16PodU64::new(1);
    let mut expected_selected = markets_before[ASSET];
    let mut expected_asset = asset;
    match side {
        SideV16::Long => {
            expected_asset.oi_eff_long_q = 0;
            expected_asset.stored_pos_count_long = 0;
            expected_asset.loss_weight_sum_long = 0;
        }
        SideV16::Short => {
            expected_asset.oi_eff_short_q = 0;
            expected_asset.stored_pos_count_short = 0;
            expected_asset.loss_weight_sum_short = 0;
        }
    }
    expected_selected.asset = AssetStateV16Account::from_runtime(&expected_asset);
    let mut expected_account = account_before;
    expected_account.legs[0] = PortfolioLegV16Account::from_runtime(&PortfolioLegV16::EMPTY);
    expected_account.active_bitmap = V16_EMPTY_ACTIVE_BITMAP.map(V16PodU64::new);
    expected_account.health_cert.valid = 0;

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    let residual_before = market.residual();
    let outcome = market
        .forfeit_recovery_leg_not_atomic(&mut account, ASSET, 1)
        .unwrap();

    kani::cover!(
        outcome.detached,
        "clean recovery forfeit reaches the successful detach branch"
    );
    assert_eq!(
        outcome,
        DeadLegForfeitOutcomeV16 {
            detached: true,
            positive_pnl_forfeited: 0,
            loss_settled: 0,
            support_consumed: 0,
            junior_face_burned: 0,
            principal_used: 0,
            insurance_used: 0,
            residual_booked: 0,
            explicit_loss: 0,
        }
    );
    assert!(kani_eq_market_group_v16_header_account(
        &expected_header,
        market.header
    ));
    assert!(kani_eq_portfolio_account_v16_account(
        &expected_account,
        account.header
    ));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &expected_selected,
        &market.markets[ASSET].engine
    ));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &markets_before[other],
        &market.markets[other].engine
    ));
    assert_eq!(market.residual(), residual_before);
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(80)]
#[kani::solver(cadical)]
fn closure_asset_zero_long_clean_recovery_forfeit_is_local() {
    prove_clean_recovery_leg_forfeit_is_value_neutral_and_asset_local::<0, true>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(80)]
#[kani::solver(cadical)]
fn closure_asset_zero_short_clean_recovery_forfeit_is_local() {
    prove_clean_recovery_leg_forfeit_is_value_neutral_and_asset_local::<0, false>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(80)]
#[kani::solver(cadical)]
fn closure_asset_one_long_clean_recovery_forfeit_is_local() {
    prove_clean_recovery_leg_forfeit_is_value_neutral_and_asset_local::<1, true>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(80)]
#[kani::solver(cadical)]
fn closure_asset_one_short_clean_recovery_forfeit_is_local() {
    prove_clean_recovery_leg_forfeit_is_value_neutral_and_asset_local::<1, false>();
}

#[cfg(all(kani, feature = "closure"))]
fn forfeit_current_kf_effects_stub<'a: 'a, T>(
    market: &mut MarketGroupV16ViewMut<'a, T>,
    account: &mut PortfolioV16ViewMut<'_>,
    asset_index: usize,
) -> V16Result<(u128, u128, u128, u128)> {
    assert!(asset_index < 2);
    assert_eq!(account.header.pnl.get(), -2);
    assert_eq!(account.header.capital.get(), 0);
    assert_eq!(account.header.active_bitmap[0].get(), 1);
    let leg = account.header.legs[0].try_to_runtime()?;
    assert!(leg.active && !leg.stale && !leg.b_stale);
    assert_eq!(leg.asset_index as usize, asset_index);
    let asset = market.asset_state(asset_index)?;
    assert_eq!(asset.lifecycle, AssetLifecycleV16::Recovery);
    let (k_now, f_now, b_now, epoch_now) = match leg.side {
        SideV16::Long => (
            asset.k_long,
            asset.f_long_num,
            asset.b_long_num,
            asset.epoch_long,
        ),
        SideV16::Short => (
            asset.k_short,
            asset.f_short_num,
            asset.b_short_num,
            asset.epoch_short,
        ),
    };
    assert_eq!(leg.k_snap, k_now);
    assert_eq!(leg.f_snap, f_now);
    assert_eq!(leg.b_snap, b_now);
    assert_eq!(leg.epoch_snap, epoch_now);
    assert_eq!(leg.b_epoch_snap, epoch_now);
    Ok((0, 0, 0, 0))
}

#[cfg(all(kani, feature = "closure"))]
fn forfeit_market_validation_stub<'a: 'a, T>(
    market: &MarketGroupV16ViewMut<'a, T>,
) -> V16Result<()> {
    assert_eq!(market.markets.len(), 2);
    assert_eq!(market.header.vault.get(), 12);
    assert_eq!(market.header.c_tot.get(), 0);
    let slot0 = market.markets[0].engine_slot();
    let slot1 = market.markets[1].engine_slot();
    let remaining = slot0
        .insurance_domain_budget_long
        .get()
        .checked_sub(slot0.insurance_domain_spent_long.get())
        .and_then(|v| {
            v.checked_add(
                slot0
                    .insurance_domain_budget_short
                    .get()
                    .checked_sub(slot0.insurance_domain_spent_short.get())?,
            )
        })
        .and_then(|v| {
            v.checked_add(
                slot1
                    .insurance_domain_budget_long
                    .get()
                    .checked_sub(slot1.insurance_domain_spent_long.get())?,
            )
        })
        .and_then(|v| {
            v.checked_add(
                slot1
                    .insurance_domain_budget_short
                    .get()
                    .checked_sub(slot1.insurance_domain_spent_short.get())?,
            )
        })
        .ok_or(V16Error::CounterUnderflow)?;
    assert_eq!(market.header.insurance.get(), remaining);
    assert_eq!(
        market.header.insurance_domain_budget_remaining_total.get(),
        remaining
    );
    let asset0 = slot0.asset.try_to_runtime()?;
    let asset1 = slot1.asset.try_to_runtime()?;
    assert_eq!(
        asset0.oi_eff_long_q,
        asset0.stored_pos_count_long as u128 * POS_SCALE
    );
    assert_eq!(
        asset0.oi_eff_short_q,
        asset0.stored_pos_count_short as u128 * POS_SCALE
    );
    assert_eq!(asset0.loss_weight_sum_long, asset0.oi_eff_long_q);
    assert_eq!(asset0.loss_weight_sum_short, asset0.oi_eff_short_q);
    assert_eq!(
        asset1.oi_eff_long_q,
        asset1.stored_pos_count_long as u128 * POS_SCALE
    );
    assert_eq!(
        asset1.oi_eff_short_q,
        asset1.stored_pos_count_short as u128 * POS_SCALE
    );
    assert_eq!(asset1.loss_weight_sum_long, asset1.oi_eff_long_q);
    assert_eq!(asset1.loss_weight_sum_short, asset1.oi_eff_short_q);
    assert!(
        (asset0.lifecycle == AssetLifecycleV16::Recovery
            && asset1.lifecycle == AssetLifecycleV16::Active)
            || (asset1.lifecycle == AssetLifecycleV16::Recovery
                && asset0.lifecycle == AssetLifecycleV16::Active)
    );
    Ok(())
}

// A recovery-mode forfeit is the permissionless exit for an abandoned losing
// leg. With current K/F/B snapshots and funded domain insurance, the public
// route must finalize the loss, release its barrier, detach in one call, and
// charge only the selected asset's opposing insurance domain. The general K/F
// and insurance scalar relations are proven separately; this composes their
// zero-delta/two-atom witnesses through the wrapper-used production API.
#[cfg(all(kani, feature = "closure"))]
fn prove_funded_recovery_forfeit_is_value_conservative_and_asset_local<
    const ASSET: usize,
    const LONG: bool,
>() {
    assert!(ASSET < 2);
    let loss = 2u128;
    let side = if LONG { SideV16::Long } else { SideV16::Short };
    let domain_side = opposite_side(side);
    let signed_position = if LONG {
        POS_SCALE as i128
    } else {
        -(POS_SCALE as i128)
    };

    let (mut header, mut markets, mut account_header) = two_asset_kf_mapping_fixture();
    account_header
        .init_empty_in_place(account_header.provenance_header)
        .unwrap();
    header.vault = V16PodU128::new(12);
    header.insurance = V16PodU128::new(12);
    header.insurance_domain_budget_remaining_total = V16PodU128::new(12);
    header.negative_pnl_account_count = V16PodU64::new(1);
    header.resolved_payout_blocker_count = V16PodU64::new(6);
    markets[0].wrapper = 11;
    markets[1].wrapper = 22;
    let mut asset_index = 0usize;
    while asset_index < 2 {
        markets[asset_index].engine.insurance_domain_budget_long = V16PodU128::new(3);
        markets[asset_index].engine.insurance_domain_budget_short = V16PodU128::new(3);
        let mut asset = markets[asset_index].engine.asset.try_to_runtime().unwrap();
        asset.slot_last = header.current_slot.get();
        if asset_index == ASSET {
            asset.lifecycle = AssetLifecycleV16::Recovery;
            asset.oi_eff_long_q = 2 * POS_SCALE;
            asset.oi_eff_short_q = 2 * POS_SCALE;
            asset.stored_pos_count_long = 2;
            asset.stored_pos_count_short = 2;
            asset.loss_weight_sum_long = 2 * POS_SCALE;
            asset.loss_weight_sum_short = 2 * POS_SCALE;
        } else {
            asset.oi_eff_long_q = POS_SCALE;
            asset.oi_eff_short_q = POS_SCALE;
            asset.stored_pos_count_long = 1;
            asset.stored_pos_count_short = 1;
            asset.loss_weight_sum_long = POS_SCALE;
            asset.loss_weight_sum_short = POS_SCALE;
        }
        markets[asset_index].engine.asset = AssetStateV16Account::from_runtime(&asset);
        asset_index += 1;
    }
    let selected_asset = markets[ASSET].engine.asset.try_to_runtime().unwrap();
    account_header.pnl = V16PodI128::new(-(loss as i128));
    account_header.active_bitmap[0] = V16PodU64::new(1);
    account_header.legs[0] = PortfolioLegV16Account::from_runtime(&PortfolioLegV16 {
        active: true,
        asset_index: ASSET as u32,
        market_id: selected_asset.market_id,
        side,
        basis_pos_q: signed_position,
        a_basis: ADL_ONE,
        k_snap: if LONG {
            selected_asset.k_long
        } else {
            selected_asset.k_short
        },
        f_snap: if LONG {
            selected_asset.f_long_num
        } else {
            selected_asset.f_short_num
        },
        epoch_snap: if LONG {
            selected_asset.epoch_long
        } else {
            selected_asset.epoch_short
        },
        loss_weight: POS_SCALE,
        b_snap: if LONG {
            selected_asset.b_long_num
        } else {
            selected_asset.b_short_num
        },
        b_epoch_snap: if LONG {
            selected_asset.epoch_long
        } else {
            selected_asset.epoch_short
        },
        ..PortfolioLegV16::EMPTY
    });

    let header_before = header;
    let markets_before = markets;
    let account_before = account_header;
    let other = 1 - ASSET;
    let residual_before = header.vault.get() - header.insurance.get();
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    let outcome = market
        .forfeit_recovery_leg_not_atomic(&mut account, ASSET, 1)
        .unwrap();

    assert_eq!(
        outcome,
        DeadLegForfeitOutcomeV16 {
            detached: true,
            positive_pnl_forfeited: 0,
            loss_settled: 0,
            support_consumed: 0,
            junior_face_burned: 0,
            principal_used: 0,
            insurance_used: loss,
            residual_booked: 0,
            explicit_loss: 0,
        }
    );

    let mut expected_header = header_before;
    expected_header.insurance = V16PodU128::new(12 - loss);
    expected_header.insurance_domain_budget_remaining_total = V16PodU128::new(12 - loss);
    expected_header.negative_pnl_account_count = V16PodU64::new(0);
    expected_header.resolved_payout_blocker_count = V16PodU64::new(5);
    expected_header.bankruptcy_hlock_active = 1;
    assert!(kani_eq_market_group_v16_header_account(
        &expected_header,
        market.header
    ));

    let mut expected_selected = markets_before[ASSET].engine;
    let mut expected_asset = selected_asset;
    if LONG {
        expected_asset.oi_eff_long_q = POS_SCALE;
        expected_asset.stored_pos_count_long = 1;
        expected_asset.loss_weight_sum_long = POS_SCALE;
        expected_selected.insurance_domain_spent_short = V16PodU128::new(loss);
    } else {
        expected_asset.oi_eff_short_q = POS_SCALE;
        expected_asset.stored_pos_count_short = 1;
        expected_asset.loss_weight_sum_short = POS_SCALE;
        expected_selected.insurance_domain_spent_long = V16PodU128::new(loss);
    }
    expected_selected.asset = AssetStateV16Account::from_runtime(&expected_asset);
    assert!(kani_eq_engine_asset_slot_v16_account(
        &expected_selected,
        &market.markets[ASSET].engine
    ));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &markets_before[other].engine,
        &market.markets[other].engine
    ));
    assert_eq!(market.markets[0].wrapper, markets_before[0].wrapper);
    assert_eq!(market.markets[1].wrapper, markets_before[1].wrapper);

    let final_close = CloseProgressLedgerV16 {
        active: true,
        finalized: true,
        close_id: 1,
        asset_index: ASSET as u32,
        market_id: selected_asset.market_id,
        domain_side,
        gross_loss_at_close_start: loss,
        drift_reference_slot: header_before.current_slot.get(),
        max_close_slot: header_before.current_slot.get()
            + header_before.config.max_bankrupt_close_lifetime_slots.get(),
        insurance_spent: loss,
        ..CloseProgressLedgerV16::EMPTY
    };
    let mut expected_account = account_before;
    expected_account.pnl = V16PodI128::new(0);
    expected_account.active_bitmap = V16_EMPTY_ACTIVE_BITMAP.map(V16PodU64::new);
    expected_account.legs[0] = PortfolioLegV16Account::from_runtime(&PortfolioLegV16::EMPTY);
    expected_account.health_cert.valid = 0;
    expected_account.close_progress = CloseProgressLedgerV16Account::from_runtime(&final_close);
    assert!(account_value_and_gate_frame_unchanged(
        &expected_account,
        account.header
    ));
    assert_eq!(account.header.legs[0], expected_account.legs[0]);
    assert_eq!(account.header.health_cert, expected_account.health_cert);
    assert_eq!(
        account.header.close_progress,
        expected_account.close_progress
    );
    assert_eq!(account.as_view().source_claim_bound_sum_num().unwrap(), 0);
    assert_eq!(market.residual(), residual_before + loss);
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(96)]
#[kani::solver(cadical)]
#[kani::stub(
    MarketGroupV16ViewMut::settle_forfeited_leg_kf_effects,
    forfeit_current_kf_effects_stub
)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, forfeit_market_validation_stub)]
#[kani::stub(
    PortfolioV16View::validate_with_market,
    liquidation_account_validation_stub
)]
fn closure_asset_zero_long_funded_recovery_forfeit_is_local() {
    prove_funded_recovery_forfeit_is_value_conservative_and_asset_local::<0, true>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(96)]
#[kani::solver(cadical)]
#[kani::stub(
    MarketGroupV16ViewMut::settle_forfeited_leg_kf_effects,
    forfeit_current_kf_effects_stub
)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, forfeit_market_validation_stub)]
#[kani::stub(
    PortfolioV16View::validate_with_market,
    liquidation_account_validation_stub
)]
fn closure_asset_zero_short_funded_recovery_forfeit_is_local() {
    prove_funded_recovery_forfeit_is_value_conservative_and_asset_local::<0, false>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(96)]
#[kani::solver(cadical)]
#[kani::stub(
    MarketGroupV16ViewMut::settle_forfeited_leg_kf_effects,
    forfeit_current_kf_effects_stub
)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, forfeit_market_validation_stub)]
#[kani::stub(
    PortfolioV16View::validate_with_market,
    liquidation_account_validation_stub
)]
fn closure_asset_one_long_funded_recovery_forfeit_is_local() {
    prove_funded_recovery_forfeit_is_value_conservative_and_asset_local::<1, true>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(96)]
#[kani::solver(cadical)]
#[kani::stub(
    MarketGroupV16ViewMut::settle_forfeited_leg_kf_effects,
    forfeit_current_kf_effects_stub
)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, forfeit_market_validation_stub)]
#[kani::stub(
    PortfolioV16View::validate_with_market,
    liquidation_account_validation_stub
)]
fn closure_asset_one_short_funded_recovery_forfeit_is_local() {
    prove_funded_recovery_forfeit_is_value_conservative_and_asset_local::<1, false>();
}

// Production post-settlement rebalance mutation seam. Full-domain contracts
// separately prove admission and arbitrary request clamping. These closures
// join that clamp to the real current-position mutation, matching OI update,
// and terminal reset for a partial and full witness on both sides/assets. The
// exact frame proves the mutation moves no value and touches no other asset.
#[cfg(all(kani, feature = "closure"))]
fn prove_rebalance_mutation_is_value_neutral_and_asset_local<
    const ASSET: usize,
    const LONG: bool,
    const FULL: bool,
>() {
    assert!(ASSET < 2);
    let position_units = 3u8;
    let request_units = if FULL { 3u8 } else { 1u8 };
    let capital_raw: u8 = kani::any();
    let surplus_raw: u8 = kani::any();
    kani::assume(capital_raw <= 8 && surplus_raw <= 3);
    let position_q = position_units as u128 * POS_SCALE;
    let request_q = request_units as u128 * POS_SCALE;
    let remaining_q = position_q - request_q.min(position_q);
    let expected_partial_a = ADL_ONE * 2 / 3;
    let side = if LONG { SideV16::Long } else { SideV16::Short };
    let pre_signed = if LONG {
        position_q as i128
    } else {
        -(position_q as i128)
    };

    let (mut header, mut markets, mut account_header) = two_asset_kf_mapping_fixture();
    account_header
        .init_empty_in_place(account_header.provenance_header)
        .unwrap();
    let capital = capital_raw as u128;
    header.c_tot = V16PodU128::new(capital);
    header.vault = V16PodU128::new(capital + 10 + surplus_raw as u128);
    header.resolved_payout_blocker_count = V16PodU64::new(2);
    account_header.capital = V16PodU128::new(capital);

    let mut asset = markets[ASSET].engine.asset.try_to_runtime().unwrap();
    asset.oi_eff_long_q = position_q;
    asset.oi_eff_short_q = position_q;
    asset.stored_pos_count_long = 1;
    asset.stored_pos_count_short = 1;
    asset.loss_weight_sum_long = position_q;
    asset.loss_weight_sum_short = position_q;
    markets[ASSET].engine.asset = AssetStateV16Account::from_runtime(&asset);
    let leg = PortfolioLegV16 {
        active: true,
        asset_index: ASSET as u32,
        market_id: asset.market_id,
        side,
        basis_pos_q: pre_signed,
        a_basis: ADL_ONE,
        k_snap: if LONG { asset.k_long } else { asset.k_short },
        f_snap: if LONG {
            asset.f_long_num
        } else {
            asset.f_short_num
        },
        epoch_snap: if LONG {
            asset.epoch_long
        } else {
            asset.epoch_short
        },
        loss_weight: position_q,
        b_snap: if LONG {
            asset.b_long_num
        } else {
            asset.b_short_num
        },
        b_epoch_snap: if LONG {
            asset.epoch_long
        } else {
            asset.epoch_short
        },
        ..PortfolioLegV16::EMPTY
    };
    account_header.active_bitmap[0] = V16PodU64::new(1);
    account_header.legs[0] = PortfolioLegV16Account::from_runtime(&leg);

    let header_before = header;
    let markets_before = [markets[0].engine, markets[1].engine];
    let account_before = account_header;
    let other = 1 - ASSET;
    let mut expected_header = header_before;
    let mut expected_selected = markets_before[ASSET];
    let mut expected_asset = asset;
    let mut expected_account = account_before;
    if FULL {
        expected_header.resolved_payout_blocker_count = V16PodU64::new(1);
        expected_header.risk_epoch = V16PodU64::new(header_before.risk_epoch.get() + 1);
        expected_asset.oi_eff_long_q = 0;
        expected_asset.oi_eff_short_q = 0;
        expected_asset.loss_weight_sum_long = 0;
        expected_asset.loss_weight_sum_short = 0;
        expected_asset.a_long = ADL_ONE;
        expected_asset.a_short = ADL_ONE;
        if LONG {
            expected_asset.stored_pos_count_long = 0;
            expected_asset.epoch_short += 1;
            expected_asset.mode_short = SideModeV16::ResetPending;
        } else {
            expected_asset.stored_pos_count_short = 0;
            expected_asset.epoch_long += 1;
            expected_asset.mode_long = SideModeV16::ResetPending;
        }
        expected_account.active_bitmap = V16_EMPTY_ACTIVE_BITMAP.map(V16PodU64::new);
        expected_account.legs[0] = PortfolioLegV16Account::from_runtime(&PortfolioLegV16::EMPTY);
    } else {
        if LONG {
            expected_asset.oi_eff_long_q = remaining_q;
            expected_asset.oi_eff_short_q = remaining_q;
            expected_asset.loss_weight_sum_long = remaining_q;
            expected_asset.a_short = expected_partial_a;
        } else {
            expected_asset.oi_eff_short_q = remaining_q;
            expected_asset.oi_eff_long_q = remaining_q;
            expected_asset.loss_weight_sum_short = remaining_q;
            expected_asset.a_long = expected_partial_a;
        }
        let mut expected_leg = leg;
        expected_leg.basis_pos_q = if LONG {
            remaining_q as i128
        } else {
            -(remaining_q as i128)
        };
        expected_leg.loss_weight = remaining_q;
        expected_account.legs[0] = PortfolioLegV16Account::from_runtime(&expected_leg);
    }
    expected_selected.asset = AssetStateV16Account::from_runtime(&expected_asset);

    let (reduced_q, reduce_delta) =
        V16Core::kernel_reduce_position_delta(pre_signed, side, request_q).unwrap();
    let lookup = PositionDeltaLookupV16 {
        existing_slot: Some(0),
        empty_slot: None,
        current_q: pre_signed,
        next_q: pre_signed.checked_add(reduce_delta).unwrap(),
    };
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let residual_before = market.residual();
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    market
        .apply_current_position_delta_with_lookup(&mut account, ASSET, reduce_delta, lookup)
        .unwrap();
    market
        .reduce_matching_open_interest_for_unilateral_close(ASSET, side, reduced_q)
        .unwrap();

    kani::cover!(
        (FULL && request_units == position_units && remaining_q == 0)
            || (!FULL && position_units == 3 && request_units == 1 && remaining_q == 2 * POS_SCALE),
        "rebalance mutation reaches the selected partial or full reduction"
    );
    kani::cover!(surplus_raw > 0, "rebalance mutation covers junior surplus");
    assert_eq!(reduced_q, request_q.min(position_q));
    assert_eq!(remaining_q == 0, FULL);
    assert_eq!(
        pre_signed.checked_add(reduce_delta).unwrap().unsigned_abs(),
        remaining_q
    );
    assert!(reduced_q > 0 && remaining_q < position_q);
    assert!(market.header.vault == expected_header.vault);
    assert!(market.header.insurance == expected_header.insurance);
    assert!(market.header.c_tot == expected_header.c_tot);
    assert!(market.header.pnl_pos_tot == expected_header.pnl_pos_tot);
    assert!(market.header.pnl_pos_bound_tot_num == expected_header.pnl_pos_bound_tot_num);
    assert!(
        market.header.source_claim_bound_total_num == expected_header.source_claim_bound_total_num
    );
    assert!(
        market.header.source_fresh_backing_total_num
            == expected_header.source_fresh_backing_total_num
    );
    assert!(
        market.header.insurance_domain_budget_remaining_total
            == expected_header.insurance_domain_budget_remaining_total
    );
    assert!(
        market.header.resolved_payout_blocker_count
            == expected_header.resolved_payout_blocker_count
    );
    assert!(market.header.risk_epoch == expected_header.risk_epoch);
    assert!(kani_eq_market_group_v16_header_account(
        &expected_header,
        market.header
    ));
    assert!(account_value_and_gate_frame_unchanged(
        &expected_account,
        account.header
    ));
    assert!(kani_eq_portfolio_leg_v16_account(
        &expected_account.legs[0],
        &account.header.legs[0]
    ));
    assert_eq!(account.header.health_cert, expected_account.health_cert);
    assert_eq!(
        account.header.close_progress,
        expected_account.close_progress
    );
    assert_eq!(
        account.header.source_domains,
        expected_account.source_domains
    );
    assert!(kani_eq_engine_asset_slot_v16_account(
        &expected_selected,
        &market.markets[ASSET].engine
    ));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &markets_before[other],
        &market.markets[other].engine
    ));
    assert_eq!(market.residual(), residual_before);
}

// Exact arithmetic specializations for this composition's reachable values;
// the generic loss-weight and wide-ratio helpers have separate domain proofs.
#[cfg(all(kani, feature = "closure"))]
fn rebalance_mutation_loss_weight_stub(abs_basis_q: u128, a_basis: u128) -> V16Result<u128> {
    assert!(abs_basis_q == 2 * POS_SCALE || abs_basis_q == 3 * POS_SCALE);
    assert_eq!(a_basis, ADL_ONE);
    Ok(abs_basis_q)
}

#[cfg(all(kani, feature = "closure"))]
fn rebalance_mutation_a_ratio_stub(a: u128, b: u128, d: u128) -> u128 {
    assert_eq!(a, ADL_ONE);
    assert_eq!(b, 2 * POS_SCALE);
    assert_eq!(d, 3 * POS_SCALE);
    ADL_ONE * 2 / 3
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(96)]
#[kani::solver(cadical)]
#[kani::stub(crate::v16::loss_weight_for_basis, rebalance_mutation_loss_weight_stub)]
#[kani::stub(
    crate::wide_math::wide_mul_div_floor_u128,
    rebalance_mutation_a_ratio_stub
)]
fn closure_asset_zero_long_partial_rebalance_mutation_is_local() {
    prove_rebalance_mutation_is_value_neutral_and_asset_local::<0, true, false>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(96)]
#[kani::solver(cadical)]
#[kani::stub(crate::v16::loss_weight_for_basis, rebalance_mutation_loss_weight_stub)]
#[kani::stub(
    crate::wide_math::wide_mul_div_floor_u128,
    rebalance_mutation_a_ratio_stub
)]
fn closure_asset_zero_short_partial_rebalance_mutation_is_local() {
    prove_rebalance_mutation_is_value_neutral_and_asset_local::<0, false, false>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(96)]
#[kani::solver(cadical)]
#[kani::stub(crate::v16::loss_weight_for_basis, rebalance_mutation_loss_weight_stub)]
#[kani::stub(
    crate::wide_math::wide_mul_div_floor_u128,
    rebalance_mutation_a_ratio_stub
)]
fn closure_asset_one_long_partial_rebalance_mutation_is_local() {
    prove_rebalance_mutation_is_value_neutral_and_asset_local::<1, true, false>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(96)]
#[kani::solver(cadical)]
#[kani::stub(crate::v16::loss_weight_for_basis, rebalance_mutation_loss_weight_stub)]
#[kani::stub(
    crate::wide_math::wide_mul_div_floor_u128,
    rebalance_mutation_a_ratio_stub
)]
fn closure_asset_one_short_partial_rebalance_mutation_is_local() {
    prove_rebalance_mutation_is_value_neutral_and_asset_local::<1, false, false>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(96)]
#[kani::solver(cadical)]
#[kani::stub(crate::v16::loss_weight_for_basis, rebalance_mutation_loss_weight_stub)]
#[kani::stub(
    crate::wide_math::wide_mul_div_floor_u128,
    rebalance_mutation_a_ratio_stub
)]
fn closure_asset_zero_long_full_rebalance_mutation_is_local() {
    prove_rebalance_mutation_is_value_neutral_and_asset_local::<0, true, true>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(96)]
#[kani::solver(cadical)]
#[kani::stub(crate::v16::loss_weight_for_basis, rebalance_mutation_loss_weight_stub)]
#[kani::stub(
    crate::wide_math::wide_mul_div_floor_u128,
    rebalance_mutation_a_ratio_stub
)]
fn closure_asset_zero_short_full_rebalance_mutation_is_local() {
    prove_rebalance_mutation_is_value_neutral_and_asset_local::<0, false, true>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(96)]
#[kani::solver(cadical)]
#[kani::stub(crate::v16::loss_weight_for_basis, rebalance_mutation_loss_weight_stub)]
#[kani::stub(
    crate::wide_math::wide_mul_div_floor_u128,
    rebalance_mutation_a_ratio_stub
)]
fn closure_asset_one_long_full_rebalance_mutation_is_local() {
    prove_rebalance_mutation_is_value_neutral_and_asset_local::<1, true, true>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(96)]
#[kani::solver(cadical)]
#[kani::stub(crate::v16::loss_weight_for_basis, rebalance_mutation_loss_weight_stub)]
#[kani::stub(
    crate::wide_math::wide_mul_div_floor_u128,
    rebalance_mutation_a_ratio_stub
)]
fn closure_asset_one_short_full_rebalance_mutation_is_local() {
    prove_rebalance_mutation_is_value_neutral_and_asset_local::<1, false, true>();
}

// Permissionless liveness over the wrapper-used auto-crank route. An expired
// outstanding close is terminally actionable without an oracle observation:
// the production classifier must select recovery, dispatch it in one call, and
// preserve every value/account/asset field while changing only market mode and
// recovery reason. Separate fixed-asset closures keep the dispatcher tractable;
// each remains symbolic over both domain sides. Symbolic residual arithmetic is
// independently covered by the close-ledger and expiry proofs.
#[cfg(all(kani, feature = "closure"))]
fn recovery_only_permissionless_crank_stub<'a: 'a, T>(
    market: &mut MarketGroupV16ViewMut<'a, T>,
    _account: &mut PortfolioV16ViewMut<'_>,
    request: PermissionlessCrankRequestV16,
) -> V16Result<PermissionlessProgressOutcomeV16> {
    match request.action {
        PermissionlessCrankActionV16::Recover(reason) => {
            market.declare_permissionless_recovery(reason)
        }
        _ => panic!("expired-close auto-crank selected a non-recovery action"),
    }
}

#[cfg(all(kani, feature = "closure"))]
fn prove_expired_close_auto_crank_declares_value_neutral_recovery<const ASSET: usize>() {
    assert!(ASSET < 2);
    let gross_loss = 2u128;
    let domain_side = if kani::any() {
        SideV16::Long
    } else {
        SideV16::Short
    };
    let (mut header, mut markets, mut account_header) = two_asset_kf_mapping_fixture();
    account_header
        .init_empty_in_place(account_header.provenance_header)
        .unwrap();
    account_header.close_progress =
        CloseProgressLedgerV16Account::from_runtime(&CloseProgressLedgerV16 {
            active: true,
            close_id: 1,
            asset_index: ASSET as u32,
            market_id: markets[ASSET].engine.asset.market_id.get(),
            domain_side,
            gross_loss_at_close_start: gross_loss,
            drift_reference_slot: 0,
            max_close_slot: 1,
            residual_remaining: gross_loss,
            ..CloseProgressLedgerV16::EMPTY
        });

    let header_before = header;
    let account_before = account_header;
    let markets_before = [markets[0].engine, markets[1].engine];
    let mut expected_header = header_before;
    expected_header.mode = encode_market_mode(MarketModeV16::Recovery);
    expected_header.recovery_reason = V16OptionalRecoveryReasonAccount::from_runtime(Some(
        PermissionlessRecoveryReasonV16::ActiveBankruptCloseCannotProgress,
    ));

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    // The fixture and ledger are constructor-equivalent valid state. Ledger
    // shape soundness is independently universal in
    // proof_v16_close_progress_ledger_residual_equation_is_enforced; repeating
    // the full account scan here duplicates all bounded tables and exceeds the
    // per-harness budget without strengthening the dispatch composition.
    let result = market
        .permissionless_auto_crank_not_atomic(
            &mut account,
            AutoCrankWorkV16 {
                now_slot: header_before.current_slot.get(),
                observations: &[],
                resolved_close_fee_rate_per_slot: 0,
            },
        )
        .unwrap();

    kani::cover!(
        domain_side == SideV16::Short,
        "auto-crank recovery covers the selected asset's short domain"
    );
    assert_eq!(
        result,
        AutoCrankResultV16 {
            selected: AutoCrankPlanV16::DeclareRecovery {
                reason: PermissionlessRecoveryReasonV16::ActiveBankruptCloseCannotProgress,
            },
            outcome: AutoCrankOutcomeV16::Progressed(
                PermissionlessProgressOutcomeV16::RecoveryDeclared(
                    PermissionlessRecoveryReasonV16::ActiveBankruptCloseCannotProgress,
                ),
            ),
        }
    );
    assert!(kani_eq_market_group_v16_header_account(
        &expected_header,
        market.header
    ));
    assert!(kani_eq_portfolio_account_v16_account(
        &account_before,
        account.header
    ));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &markets_before[0],
        &market.markets[0].engine
    ));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &markets_before[1],
        &market.markets[1].engine
    ));
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(80)]
#[kani::solver(cadical)]
#[kani::stub(
    MarketGroupV16ViewMut::permissionless_crank_not_atomic,
    recovery_only_permissionless_crank_stub
)]
fn closure_asset_zero_expired_close_auto_crank_declares_recovery() {
    prove_expired_close_auto_crank_declares_value_neutral_recovery::<0>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(80)]
#[kani::solver(cadical)]
#[kani::stub(
    MarketGroupV16ViewMut::permissionless_crank_not_atomic,
    recovery_only_permissionless_crank_stub
)]
fn closure_asset_one_expired_close_auto_crank_declares_recovery() {
    prove_expired_close_auto_crank_declares_value_neutral_recovery::<1>();
}

// Constructor-valid account witness seam. Account validator soundness is
// independently universal; repeating its full 16-leg/source-domain scans in
// this transition composition exceeds the per-harness budget.
#[cfg(all(kani, feature = "closure"))]
fn valid_b_account_shape_stub<'a: 'a, T>(
    _account: &PortfolioV16View<'a>,
    _market: &MarketGroupV16View<'_, T>,
) -> V16Result<()> {
    Ok(())
}

#[cfg(all(kani, feature = "closure"))]
fn one_position_loss_weight_stub(abs_basis_q: u128, a_basis: u128) -> V16Result<u128> {
    assert_eq!(abs_basis_q, POS_SCALE);
    assert_eq!(a_basis, ADL_ONE);
    Ok(POS_SCALE)
}

#[cfg(all(kani, feature = "closure"))]
fn zero_to_negative_one_pnl_stub<'a: 'a, T>(
    market: &mut MarketGroupV16ViewMut<'a, T>,
    account: &mut PortfolioV16ViewMut<'_>,
    new_pnl: i128,
) -> V16Result<()> {
    assert_eq!(account.header.pnl.get(), 0);
    assert_eq!(new_pnl, -1);
    assert_eq!(market.header.negative_pnl_account_count.get(), 0);
    market.header.negative_pnl_account_count = V16PodU64::new(1);
    account.header.pnl = V16PodI128::new(-1);
    account.header.health_cert.valid = 0;
    Ok(())
}

#[cfg(all(kani, feature = "closure"))]
fn selected_leg_is_no_longer_b_stale_stub<'a: 'a, T>(
    account: &PortfolioV16View<'_>,
) -> V16Result<bool> {
    let _ = core::marker::PhantomData::<(&'a (), T)>;
    assert_eq!(account.header.legs[0].b_stale, 0);
    Ok(false)
}

// Auto-crank selection seam. Any classifier/selector regression reaches the
// rejecting branch; the real selected transition is proven separately below.
#[cfg(all(kani, feature = "closure"))]
fn b_plan_only_permissionless_crank_stub<'a: 'a, T>(
    _market: &mut MarketGroupV16ViewMut<'a, T>,
    _account: &mut PortfolioV16ViewMut<'_>,
    request: PermissionlessCrankRequestV16,
) -> V16Result<PermissionlessProgressOutcomeV16> {
    match request.action {
        PermissionlessCrankActionV16::SettleB { asset_index } => {
            assert_eq!(request.asset_index, asset_index);
            Ok(PermissionlessProgressOutcomeV16::AccountBChunk(
                AccountBSettlementChunkV16 {
                    delta_b: 0,
                    loss: 0,
                    new_remainder: 0,
                    remaining_after: 0,
                },
            ))
        }
        _ => panic!("b-stale auto-crank selected a non-settlement action"),
    }
}

#[cfg(all(kani, feature = "closure"))]
fn unreachable_resolved_close_stub<'a: 'a, T>(
    _market: &mut MarketGroupV16ViewMut<'a, T>,
    _account: &mut PortfolioV16ViewMut<'_>,
    _fee_rate_per_slot: u128,
) -> V16Result<ResolvedCloseOutcomeV16> {
    panic!("b-stale auto-crank selected resolved close")
}

#[cfg(all(kani, feature = "closure"))]
fn b_stale_transition_fixture<const ASSET: usize>() -> (
    MarketGroupV16HeaderAccount,
    [Market<u64>; 2],
    PortfolioAccountV16Account,
    PortfolioLegV16,
    u128,
) {
    assert!(ASSET < 2);
    let side = if kani::any() {
        SideV16::Long
    } else {
        SideV16::Short
    };
    let delta_b = SOCIAL_LOSS_DEN / POS_SCALE;
    let (mut header, mut markets, mut account_header) = two_asset_kf_mapping_fixture();
    account_header
        .init_empty_in_place(account_header.provenance_header)
        .unwrap();

    let mut asset = markets[ASSET].engine.asset.try_to_runtime().unwrap();
    asset.b_long_num = delta_b;
    asset.b_short_num = delta_b;
    asset.oi_eff_long_q = POS_SCALE;
    asset.oi_eff_short_q = POS_SCALE;
    asset.stored_pos_count_long = 1;
    asset.stored_pos_count_short = 1;
    asset.loss_weight_sum_long = POS_SCALE;
    asset.loss_weight_sum_short = POS_SCALE;
    markets[ASSET].engine.asset = AssetStateV16Account::from_runtime(&asset);
    header.resolved_payout_blocker_count = V16PodU64::new(2);
    header.b_stale_account_count = V16PodU64::new(1);

    let leg = PortfolioLegV16 {
        active: true,
        asset_index: ASSET as u32,
        market_id: asset.market_id,
        side,
        basis_pos_q: match side {
            SideV16::Long => POS_SCALE as i128,
            SideV16::Short => -(POS_SCALE as i128),
        },
        a_basis: ADL_ONE,
        k_snap: match side {
            SideV16::Long => asset.k_long,
            SideV16::Short => asset.k_short,
        },
        f_snap: match side {
            SideV16::Long => asset.f_long_num,
            SideV16::Short => asset.f_short_num,
        },
        epoch_snap: match side {
            SideV16::Long => asset.epoch_long,
            SideV16::Short => asset.epoch_short,
        },
        loss_weight: POS_SCALE,
        b_snap: 0,
        b_rem: 0,
        b_epoch_snap: match side {
            SideV16::Long => asset.epoch_long,
            SideV16::Short => asset.epoch_short,
        },
        b_stale: true,
        stale: false,
    };
    account_header.active_bitmap[0] = V16PodU64::new(1);
    account_header.legs[0] = PortfolioLegV16Account::from_runtime(&leg);
    account_header.b_stale_state = 1;

    (header, markets, account_header, leg, delta_b)
}

// Real B-settlement transition: a current leg one atom behind its side's B
// target fully settles in one bounded chunk, charges exactly one PnL atom,
// clears both stale flags/counter, and preserves both market slots.
#[cfg(all(kani, feature = "closure"))]
fn prove_b_stale_settlement_advances_selected_asset<const ASSET: usize>() {
    let (mut header, mut markets, mut account_header, leg, delta_b) =
        b_stale_transition_fixture::<ASSET>();

    let header_before = header;
    let account_before = account_header;
    let markets_before = [markets[0].engine, markets[1].engine];
    let mut expected_header = header_before;
    expected_header.b_stale_account_count = V16PodU64::new(0);
    expected_header.negative_pnl_account_count = V16PodU64::new(1);
    let mut expected_account = account_before;
    expected_account.pnl = V16PodI128::new(-1);
    expected_account.b_stale_state = 0;
    let mut expected_leg = leg;
    expected_leg.b_snap = delta_b;
    expected_leg.b_stale = false;
    expected_account.legs[0] = PortfolioLegV16Account::from_runtime(&expected_leg);

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    let result = market
        .settle_account_b_chunk(
            &mut account,
            ASSET,
            header_before.config.public_b_chunk_atoms.get(),
        )
        .unwrap();

    kani::cover!(
        leg.side == SideV16::Short,
        "B settlement covers the selected asset's short side"
    );
    let chunk = AccountBSettlementChunkV16 {
        delta_b,
        loss: 1,
        new_remainder: 0,
        remaining_after: 0,
    };
    assert_eq!(result, chunk);
    assert!(kani_eq_market_group_v16_header_account(
        &expected_header,
        market.header
    ));
    assert!(kani_eq_portfolio_account_v16_account(
        &expected_account,
        account.header
    ));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &markets_before[0],
        &market.markets[0].engine
    ));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &markets_before[1],
        &market.markets[1].engine
    ));
}

// Wrapper-used auto-crank selection composition. The persisted b-stale leg must
// select SettleB for the engine-owned asset without an oracle observation. The
// transition body is the separately proven theorem above.
#[cfg(all(kani, feature = "closure"))]
fn prove_b_stale_auto_crank_selects_asset<const ASSET: usize>() {
    let (mut header, mut markets, mut account_header, leg, _) =
        b_stale_transition_fixture::<ASSET>();
    let header_before = header;
    let account_before = account_header;
    let markets_before = [markets[0].engine, markets[1].engine];
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    let result = market
        .permissionless_auto_crank_not_atomic(
            &mut account,
            AutoCrankWorkV16 {
                now_slot: header_before.current_slot.get(),
                observations: &[],
                resolved_close_fee_rate_per_slot: 0,
            },
        )
        .unwrap();
    let dispatch_witness = AccountBSettlementChunkV16 {
        delta_b: 0,
        loss: 0,
        new_remainder: 0,
        remaining_after: 0,
    };

    kani::cover!(
        leg.side == SideV16::Short,
        "B-stale auto-crank selection covers the selected asset's short side"
    );
    assert_eq!(
        result,
        AutoCrankResultV16 {
            selected: AutoCrankPlanV16::SettleBChunk { asset_index: ASSET },
            outcome: AutoCrankOutcomeV16::Progressed(
                PermissionlessProgressOutcomeV16::AccountBChunk(dispatch_witness),
            ),
        }
    );
    assert!(kani_eq_market_group_v16_header_account(
        &header_before,
        market.header
    ));
    assert!(kani_eq_portfolio_account_v16_account(
        &account_before,
        account.header
    ));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &markets_before[0],
        &market.markets[0].engine
    ));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &markets_before[1],
        &market.markets[1].engine
    ));
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
#[kani::stub(PortfolioV16View::validate_with_market, valid_b_account_shape_stub)]
#[kani::stub(crate::v16::loss_weight_for_basis, one_position_loss_weight_stub)]
#[kani::stub(MarketGroupV16ViewMut::set_account_pnl, zero_to_negative_one_pnl_stub)]
#[kani::stub(
    MarketGroupV16ViewMut::has_b_stale_leg,
    selected_leg_is_no_longer_b_stale_stub
)]
fn closure_asset_zero_b_stale_settlement_advances_selected_asset() {
    prove_b_stale_settlement_advances_selected_asset::<0>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
#[kani::stub(PortfolioV16View::validate_with_market, valid_b_account_shape_stub)]
#[kani::stub(crate::v16::loss_weight_for_basis, one_position_loss_weight_stub)]
#[kani::stub(MarketGroupV16ViewMut::set_account_pnl, zero_to_negative_one_pnl_stub)]
#[kani::stub(
    MarketGroupV16ViewMut::has_b_stale_leg,
    selected_leg_is_no_longer_b_stale_stub
)]
fn closure_asset_one_b_stale_settlement_advances_selected_asset() {
    prove_b_stale_settlement_advances_selected_asset::<1>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
#[kani::stub(crate::v16::loss_weight_for_basis, one_position_loss_weight_stub)]
#[kani::stub(
    MarketGroupV16ViewMut::permissionless_crank_not_atomic,
    b_plan_only_permissionless_crank_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::close_resolved_account_not_atomic,
    unreachable_resolved_close_stub
)]
fn closure_asset_zero_b_stale_auto_crank_selects_asset() {
    prove_b_stale_auto_crank_selects_asset::<0>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
#[kani::stub(crate::v16::loss_weight_for_basis, one_position_loss_weight_stub)]
#[kani::stub(
    MarketGroupV16ViewMut::permissionless_crank_not_atomic,
    b_plan_only_permissionless_crank_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::close_resolved_account_not_atomic,
    unreachable_resolved_close_stub
)]
fn closure_asset_one_b_stale_auto_crank_selects_asset() {
    prove_b_stale_auto_crank_selects_asset::<1>();
}

// The wrapper cannot choose the asset dispatched by auto-crank. Over both
// permutations of two distinct asset IDs and both first-leg stale states, the
// real selector must map the first active slot and first active b-stale slot
// back to their engine-owned asset IDs.
#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(17)]
#[kani::solver(cadical)]
fn closure_auto_crank_selector_maps_first_active_and_b_stale_assets() {
    let first_asset: u32 = kani::any();
    let second_asset: u32 = kani::any();
    let first_b_stale: bool = kani::any();
    kani::assume(first_asset < 2);
    kani::assume(second_asset < 2);
    kani::assume(first_asset != second_asset);

    let mut account_header = PortfolioAccountV16Account::default();
    account_header.legs =
        [PortfolioLegV16Account::from_runtime(&PortfolioLegV16::EMPTY); V16_MAX_PORTFOLIO_ASSETS_N];
    account_header.active_bitmap[0] = V16PodU64::new(3);
    account_header.legs[0] = PortfolioLegV16Account::from_runtime(&PortfolioLegV16 {
        active: true,
        asset_index: first_asset,
        market_id: first_asset as u64 + 1,
        side: SideV16::Long,
        basis_pos_q: POS_SCALE as i128,
        a_basis: ADL_ONE,
        loss_weight: POS_SCALE,
        b_stale: first_b_stale,
        ..PortfolioLegV16::EMPTY
    });
    account_header.legs[1] = PortfolioLegV16Account::from_runtime(&PortfolioLegV16 {
        active: true,
        asset_index: second_asset,
        market_id: second_asset as u64 + 1,
        side: SideV16::Short,
        basis_pos_q: -(POS_SCALE as i128),
        a_basis: ADL_ONE,
        loss_weight: POS_SCALE,
        b_stale: true,
        ..PortfolioLegV16::EMPTY
    });
    let account = PortfolioV16View::new(&account_header);

    let (b_stale_asset, active_asset) =
        MarketGroupV16ViewMut::<u64>::auto_crank_selected_assets(&account).unwrap();
    let expected_b_stale = (if first_b_stale {
        first_asset
    } else {
        second_asset
    }) as usize;

    assert!(active_asset == Some(first_asset as usize));
    assert!(b_stale_asset == Some(expected_b_stale));
    kani::cover!(
        first_b_stale,
        "the first active leg is also the first b-stale leg"
    );
    kani::cover!(
        !first_b_stale,
        "the first active and first b-stale legs are distinct"
    );
    kani::cover!(first_asset == 0, "asset-zero can occupy the first slot");
    kani::cover!(first_asset == 1, "asset-one can occupy the first slot");
}

#[cfg(all(kani, feature = "closure"))]
fn selected_asset_liquidation_dispatch_stub<'a: 'a, T>(
    market: &mut MarketGroupV16ViewMut<'a, T>,
    account: &mut PortfolioV16ViewMut<'_>,
    request: LiquidationRequestV16,
) -> V16Result<LiquidationOutcomeV16> {
    assert_eq!(market.header.bankruptcy_hlock_active, 0);
    assert_eq!(account.header.active_bitmap[0].get(), 1);
    let leg = account.header.legs[0].try_to_runtime()?;
    assert!(leg.active && !leg.stale && !leg.b_stale);
    assert_eq!(request.asset_index, leg.asset_index as usize);
    let cert = account.header.health_cert.try_to_runtime()?;
    assert!(cert.valid && cert.certified_liq_deficit != 0);
    market.header.bankruptcy_hlock_active = 1;
    Ok(LiquidationOutcomeV16 {
        closed_q: leg.basis_pos_q.unsigned_abs(),
        insurance_used: 0,
        residual_booked: 0,
        explicit_loss: 0,
        fee_charged: 0,
    })
}

#[cfg(all(kani, feature = "closure"))]
fn valid_two_asset_market_tail_stub<'a: 'a, T>(
    market: &MarketGroupV16ViewMut<'a, T>,
) -> V16Result<()> {
    assert_eq!(market.markets.len(), 2);
    assert_eq!(market.header.asset_slot_capacity.get(), 2);
    assert_eq!(market.header.config.max_market_slots.get(), 2);
    assert_eq!(market.markets[0].engine.asset.market_id.get(), 1);
    assert_eq!(market.markets[1].engine.asset.market_id.get(), 2);
    Ok(())
}

#[cfg(all(kani, feature = "closure"))]
fn current_liquidatable_summary_stub<'a: 'a, T>(
    market: &MarketGroupV16ViewMut<'a, T>,
    account: &PortfolioV16View<'_>,
) -> V16Result<ActionableSummaryV16> {
    assert_eq!(decode_market_mode(market.header.mode)?, MarketModeV16::Live);
    assert_eq!(account.header.active_bitmap[0].get(), 1);
    let leg = account.header.legs[0].try_to_runtime()?;
    assert!(leg.active && !leg.stale && !leg.b_stale);
    let cert = account.header.health_cert.try_to_runtime()?;
    assert!(cert.valid);
    assert_eq!(cert.certified_liq_deficit, 1);
    assert_eq!(cert.cert_oracle_epoch, market.header.oracle_epoch.get());
    assert_eq!(cert.cert_funding_epoch, market.header.funding_epoch.get());
    assert_eq!(cert.cert_risk_epoch, market.header.risk_epoch.get());
    assert_eq!(
        cert.cert_asset_set_epoch,
        market.header.asset_set_epoch.get()
    );
    assert_eq!(
        cert.active_bitmap_at_cert[0],
        account.header.active_bitmap[0].get()
    );
    Ok(ActionableSummaryV16 {
        stale: false,
        b_stale: false,
        pending_close: false,
        expired_close: false,
        liquidatable: true,
        recovery_eligible: false,
        resolved_winner: false,
    })
}

#[cfg(all(kani, feature = "closure"))]
fn selected_active_asset_stub<'a: 'a, T>(
    account: &PortfolioV16View<'_>,
) -> V16Result<(Option<usize>, Option<usize>)> {
    let _ = core::marker::PhantomData::<(&'a (), T)>;
    assert_eq!(account.header.active_bitmap[0].get(), 1);
    let leg = account.header.legs[0].try_to_runtime()?;
    assert!(leg.active && !leg.stale && !leg.b_stale);
    Ok((None, Some(leg.asset_index as usize)))
}

#[cfg(all(kani, feature = "closure"))]
fn dispatch_asset_risk_frame_unchanged(
    before: &AssetStateV16Account,
    after: &AssetStateV16Account,
) -> bool {
    before.market_id.get() == after.market_id.get()
        && before.lifecycle == after.lifecycle
        && before.raw_oracle_target_price.get() == after.raw_oracle_target_price.get()
        && before.effective_price.get() == after.effective_price.get()
        && before.slot_last.get() == after.slot_last.get()
        && before.a_long.get() == after.a_long.get()
        && before.a_short.get() == after.a_short.get()
        && before.k_long.get() == after.k_long.get()
        && before.k_short.get() == after.k_short.get()
        && before.f_long_num.get() == after.f_long_num.get()
        && before.f_short_num.get() == after.f_short_num.get()
        && before.b_long_num.get() == after.b_long_num.get()
        && before.b_short_num.get() == after.b_short_num.get()
        && before.oi_eff_long_q.get() == after.oi_eff_long_q.get()
        && before.oi_eff_short_q.get() == after.oi_eff_short_q.get()
        && before.stored_pos_count_long.get() == after.stored_pos_count_long.get()
        && before.stored_pos_count_short.get() == after.stored_pos_count_short.get()
        && before.pending_obligation_count_long.get() == after.pending_obligation_count_long.get()
        && before.pending_obligation_count_short.get() == after.pending_obligation_count_short.get()
        && before.loss_weight_sum_long.get() == after.loss_weight_sum_long.get()
        && before.loss_weight_sum_short.get() == after.loss_weight_sum_short.get()
        && before.explicit_unallocated_loss_long.get() == after.explicit_unallocated_loss_long.get()
        && before.explicit_unallocated_loss_short.get()
            == after.explicit_unallocated_loss_short.get()
        && before.epoch_long.get() == after.epoch_long.get()
        && before.epoch_short.get() == after.epoch_short.get()
        && before.mode_long == after.mode_long
        && before.mode_short == after.mode_short
}

#[cfg(all(kani, feature = "closure"))]
fn selected_asset_protective_accrual_stub<'a: 'a, T>(
    market: &mut MarketGroupV16ViewMut<'a, T>,
    asset_index: usize,
    now_slot: u64,
    effective_price: u64,
    funding_rate_e9: i128,
    protective_progress_committed: bool,
) -> V16Result<AccrueAssetOutcomeV16> {
    assert_eq!(market.header.bankruptcy_hlock_active, 1);
    assert!(asset_index < 2);
    assert_eq!(now_slot, market.header.current_slot.get());
    assert_eq!(effective_price, 100);
    assert_eq!(funding_rate_e9, 0);
    assert!(protective_progress_committed);
    Ok(AccrueAssetOutcomeV16 {
        dt: 0,
        price_move_active: false,
        funding_active: false,
        equity_active: false,
        loss_stale_after: false,
    })
}

// Liquidation DoS/asset-ownership composition over the wrapper-used route. A
// current deficit and one active leg must select that leg's engine-owned asset,
// enter the real internal liquidation dispatch, and forward the authenticated
// observation to protective accrual only after liquidation. Four public
// liquidation closures independently prove the exact asset/side value
// transition behind the assertion-heavy call seam.
#[cfg(all(kani, feature = "closure"))]
fn prove_liquidatable_auto_crank_selects_active_asset<const ASSET: usize, const LONG: bool>() {
    assert!(ASSET < 2);
    let side = if LONG { SideV16::Long } else { SideV16::Short };
    let (mut header, mut markets, mut account_header) = two_asset_kf_mapping_fixture();
    let mut asset = markets[ASSET].engine.asset.try_to_runtime().unwrap();
    asset.slot_last = header.current_slot.get();
    asset.oi_eff_long_q = POS_SCALE;
    asset.oi_eff_short_q = POS_SCALE;
    asset.stored_pos_count_long = 1;
    asset.stored_pos_count_short = 1;
    asset.loss_weight_sum_long = POS_SCALE;
    asset.loss_weight_sum_short = POS_SCALE;
    markets[ASSET].engine.asset = AssetStateV16Account::from_runtime(&asset);
    header.resolved_payout_blocker_count = V16PodU64::new(2);
    let leg = PortfolioLegV16 {
        active: true,
        asset_index: ASSET as u32,
        market_id: asset.market_id,
        side,
        basis_pos_q: if LONG {
            POS_SCALE as i128
        } else {
            -(POS_SCALE as i128)
        },
        a_basis: ADL_ONE,
        k_snap: if LONG { asset.k_long } else { asset.k_short },
        f_snap: if LONG {
            asset.f_long_num
        } else {
            asset.f_short_num
        },
        epoch_snap: if LONG {
            asset.epoch_long
        } else {
            asset.epoch_short
        },
        loss_weight: POS_SCALE,
        b_snap: if LONG {
            asset.b_long_num
        } else {
            asset.b_short_num
        },
        b_epoch_snap: if LONG {
            asset.epoch_long
        } else {
            asset.epoch_short
        },
        ..PortfolioLegV16::EMPTY
    };
    account_header.active_bitmap[0] = V16PodU64::new(1);
    account_header.legs[0] = PortfolioLegV16Account::from_runtime(&leg);
    let bitmap = account_header.active_bitmap.map(V16PodU64::get);
    account_header.health_cert = HealthCertV16Account::from_runtime(&HealthCertV16 {
        certified_equity: 0,
        certified_initial_req: 2,
        certified_maintenance_req: 1,
        certified_liq_deficit: 1,
        certified_worst_case_loss: 1,
        cert_oracle_epoch: header.oracle_epoch.get(),
        cert_funding_epoch: header.funding_epoch.get(),
        cert_risk_epoch: header.risk_epoch.get(),
        cert_asset_set_epoch: header.asset_set_epoch.get(),
        active_bitmap_at_cert: bitmap,
        valid: true,
    });

    let header_before = header;
    let account_before = account_header;
    let markets_before = [markets[0].engine, markets[1].engine];
    let observations = [AutoCrankObservationV16 {
        asset_index: ASSET,
        effective_price: asset.effective_price,
        funding_rate_e9: 0,
    }];
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    let result = market
        .permissionless_auto_crank_not_atomic(
            &mut account,
            AutoCrankWorkV16 {
                now_slot: header_before.current_slot.get(),
                observations: &observations,
                resolved_close_fee_rate_per_slot: 0,
            },
        )
        .unwrap();

    kani::cover!(
        market.header.bankruptcy_hlock_active == 1,
        "auto-crank reaches the selected liquidation dispatch seam"
    );
    assert_eq!(
        result,
        AutoCrankResultV16 {
            selected: AutoCrankPlanV16::Liquidate { asset_index: ASSET },
            outcome: AutoCrankOutcomeV16::Progressed(
                PermissionlessProgressOutcomeV16::AccountCurrent,
            ),
        }
    );
    assert_eq!(market.header.bankruptcy_hlock_active, 1);
    assert_eq!(market.header.mode, header_before.mode);
    assert_eq!(market.header.vault.get(), header_before.vault.get());
    assert_eq!(market.header.c_tot.get(), header_before.c_tot.get());
    assert_eq!(market.header.insurance.get(), header_before.insurance.get());
    assert_eq!(
        market.header.insurance_domain_budget_remaining_total.get(),
        header_before.insurance_domain_budget_remaining_total.get()
    );
    assert_eq!(
        market.header.pnl_pos_tot.get(),
        header_before.pnl_pos_tot.get()
    );
    assert_eq!(
        market.header.pnl_pos_bound_tot_num.get(),
        header_before.pnl_pos_bound_tot_num.get()
    );
    assert_eq!(
        market.header.current_slot.get(),
        header_before.current_slot.get()
    );
    assert_eq!(
        market.header.oracle_epoch.get(),
        header_before.oracle_epoch.get()
    );
    assert_eq!(
        market.header.funding_epoch.get(),
        header_before.funding_epoch.get()
    );
    assert_eq!(
        market.header.risk_epoch.get(),
        header_before.risk_epoch.get()
    );
    assert_eq!(
        market.header.asset_set_epoch.get(),
        header_before.asset_set_epoch.get()
    );
    assert_eq!(account.header.capital.get(), account_before.capital.get());
    assert_eq!(account.header.pnl.get(), account_before.pnl.get());
    assert_eq!(
        account.header.reserved_pnl.get(),
        account_before.reserved_pnl.get()
    );
    assert_eq!(
        account.header.fee_credits.get(),
        account_before.fee_credits.get()
    );
    assert_eq!(
        account.header.cancel_deposit_escrow.get(),
        account_before.cancel_deposit_escrow.get()
    );
    assert_eq!(
        account.header.active_bitmap[0].get(),
        account_before.active_bitmap[0].get()
    );
    let leg_after = account.header.legs[0].try_to_runtime().unwrap();
    assert_eq!(leg_after.active, leg.active);
    assert_eq!(leg_after.asset_index, leg.asset_index);
    assert_eq!(leg_after.market_id, leg.market_id);
    assert_eq!(leg_after.side, leg.side);
    assert_eq!(leg_after.basis_pos_q, leg.basis_pos_q);
    assert_eq!(leg_after.stale, leg.stale);
    assert_eq!(leg_after.b_stale, leg.b_stale);
    let cert_after = account.header.health_cert.try_to_runtime().unwrap();
    assert!(cert_after.valid);
    assert_eq!(cert_after.certified_liq_deficit, 1);
    assert_eq!(
        cert_after.cert_oracle_epoch,
        header_before.oracle_epoch.get()
    );
    assert_eq!(
        cert_after.cert_funding_epoch,
        header_before.funding_epoch.get()
    );
    assert_eq!(cert_after.cert_risk_epoch, header_before.risk_epoch.get());
    assert_eq!(
        cert_after.cert_asset_set_epoch,
        header_before.asset_set_epoch.get()
    );
    assert_eq!(account.header.close_progress.active, 0);
    assert_eq!(
        account.header.source_domains[0]
            .source_claim_bound_num
            .get(),
        0
    );
    assert!(dispatch_asset_risk_frame_unchanged(
        &markets_before[0].asset,
        &market.markets[0].engine.asset
    ));
    assert!(dispatch_asset_risk_frame_unchanged(
        &markets_before[1].asset,
        &market.markets[1].engine.asset
    ));
    assert_eq!(
        market.markets[0].engine.insurance_domain_budget_long.get(),
        markets_before[0].insurance_domain_budget_long.get()
    );
    assert_eq!(
        market.markets[0].engine.insurance_domain_budget_short.get(),
        markets_before[0].insurance_domain_budget_short.get()
    );
    assert_eq!(
        market.markets[0].engine.insurance_domain_spent_long.get(),
        markets_before[0].insurance_domain_spent_long.get()
    );
    assert_eq!(
        market.markets[0].engine.insurance_domain_spent_short.get(),
        markets_before[0].insurance_domain_spent_short.get()
    );
    assert_eq!(
        market.markets[1].engine.insurance_domain_budget_long.get(),
        markets_before[1].insurance_domain_budget_long.get()
    );
    assert_eq!(
        market.markets[1].engine.insurance_domain_budget_short.get(),
        markets_before[1].insurance_domain_budget_short.get()
    );
    assert_eq!(
        market.markets[1].engine.insurance_domain_spent_long.get(),
        markets_before[1].insurance_domain_spent_long.get()
    );
    assert_eq!(
        market.markets[1].engine.insurance_domain_spent_short.get(),
        markets_before[1].insurance_domain_spent_short.get()
    );
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
#[kani::stub(
    MarketGroupV16ViewMut::build_actionable_summary,
    current_liquidatable_summary_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::auto_crank_selected_assets,
    selected_active_asset_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_unconfigured_market_tail,
    valid_two_asset_market_tail_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::liquidate_account_not_atomic,
    selected_asset_liquidation_dispatch_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::accrue_asset_to_not_atomic,
    selected_asset_protective_accrual_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::close_resolved_account_not_atomic,
    unreachable_resolved_close_stub
)]
fn closure_asset_zero_liquidatable_auto_crank_selects_active_asset() {
    prove_liquidatable_auto_crank_selects_active_asset::<0, true>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
#[kani::stub(
    MarketGroupV16ViewMut::build_actionable_summary,
    current_liquidatable_summary_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::auto_crank_selected_assets,
    selected_active_asset_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_unconfigured_market_tail,
    valid_two_asset_market_tail_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::liquidate_account_not_atomic,
    selected_asset_liquidation_dispatch_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::accrue_asset_to_not_atomic,
    selected_asset_protective_accrual_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::close_resolved_account_not_atomic,
    unreachable_resolved_close_stub
)]
fn closure_asset_one_liquidatable_auto_crank_selects_active_asset() {
    prove_liquidatable_auto_crank_selects_active_asset::<1, false>();
}

#[cfg(all(kani, feature = "closure"))]
fn stale_cert_refresh_dispatch_stub<'a: 'a, T>(
    market: &mut MarketGroupV16ViewMut<'a, T>,
    account: &mut PortfolioV16ViewMut<'_>,
    price_override: Option<(usize, u64)>,
    b_delta_budget: u128,
    allow_b_chunk: bool,
) -> V16Result<AccountRefreshCertOutcomeV16> {
    assert!(
        account.header.stale_state == 1,
        "refresh receives stale account"
    );
    assert!(
        market.header.stale_certificate_count.get() == 1,
        "refresh receives matching stale count"
    );
    assert!(
        account.header.active_bitmap[0].get() == 1,
        "refresh receives the one-leg bitmap"
    );
    let leg = account.header.legs[0].try_to_runtime()?;
    assert!(leg.active && !leg.stale && !leg.b_stale);
    let asset_index = leg.asset_index as usize;
    let asset = market.markets[asset_index].engine.asset.try_to_runtime()?;
    assert!(
        price_override == Some((asset_index, asset.effective_price)),
        "refresh receives the selected asset and committed price"
    );
    assert!(
        b_delta_budget == market.header.config.public_b_chunk_atoms.get(),
        "refresh receives the configured B budget"
    );
    assert!(allow_b_chunk);
    let mut cert = account.header.health_cert.try_to_runtime()?;
    assert!(cert.valid && cert.certified_liq_deficit != 0);
    assert_ne!(cert.cert_oracle_epoch, market.header.oracle_epoch.get());
    cert.cert_oracle_epoch = market.header.oracle_epoch.get();
    cert.cert_funding_epoch = market.header.funding_epoch.get();
    cert.cert_risk_epoch = market.header.risk_epoch.get();
    cert.cert_asset_set_epoch = market.header.asset_set_epoch.get();
    cert.active_bitmap_at_cert = account.header.active_bitmap.map(V16PodU64::get);
    market.header.stale_certificate_count = V16PodU64::new(0);
    account.header.stale_state = 0;
    account.header.health_cert = HealthCertV16Account::from_runtime(&cert);
    Ok(AccountRefreshCertOutcomeV16::Certified(cert))
}

#[cfg(all(kani, feature = "closure"))]
fn stale_cert_refresh_accrual_stub<'a: 'a, const ASSET: usize, T>(
    market: &mut MarketGroupV16ViewMut<'a, T>,
    asset_index: usize,
    now_slot: u64,
    effective_price: u64,
    funding_rate_e9: i128,
    protective_progress_committed: bool,
) -> V16Result<AccrueAssetOutcomeV16> {
    assert!(ASSET < 2);
    assert!(
        market.header.stale_certificate_count.get() == 0,
        "accrual follows recertification"
    );
    assert!(
        asset_index == ASSET,
        "accrual receives the engine-selected asset"
    );
    let asset = market.markets[ASSET].engine.asset.try_to_runtime()?;
    assert!(
        now_slot == market.header.current_slot.get(),
        "accrual receives the authenticated slot"
    );
    assert!(
        effective_price == asset.effective_price,
        "accrual receives the selected asset price"
    );
    assert!(
        funding_rate_e9 == 0,
        "committed-price fallback has zero funding"
    );
    assert!(protective_progress_committed);
    Ok(AccrueAssetOutcomeV16 {
        dt: 0,
        price_move_active: false,
        funding_active: false,
        equity_active: false,
        loss_stale_after: false,
    })
}

#[cfg(all(kani, feature = "closure"))]
fn stale_cert_refresh_asset_zero_accrual_stub<'a: 'a, T>(
    market: &mut MarketGroupV16ViewMut<'a, T>,
    asset_index: usize,
    now_slot: u64,
    effective_price: u64,
    funding_rate_e9: i128,
    protective_progress_committed: bool,
) -> V16Result<AccrueAssetOutcomeV16> {
    stale_cert_refresh_accrual_stub::<0, T>(
        market,
        asset_index,
        now_slot,
        effective_price,
        funding_rate_e9,
        protective_progress_committed,
    )
}

#[cfg(all(kani, feature = "closure"))]
fn stale_cert_refresh_asset_one_accrual_stub<'a: 'a, T>(
    market: &mut MarketGroupV16ViewMut<'a, T>,
    asset_index: usize,
    now_slot: u64,
    effective_price: u64,
    funding_rate_e9: i128,
    protective_progress_committed: bool,
) -> V16Result<AccrueAssetOutcomeV16> {
    stale_cert_refresh_accrual_stub::<1, T>(
        market,
        asset_index,
        now_slot,
        effective_price,
        funding_rate_e9,
        protective_progress_committed,
    )
}

#[cfg(all(kani, feature = "closure"))]
fn stale_current_asset_state_stub<'a: 'a, const ASSET: usize, T>(
    market: &MarketGroupV16ViewMut<'a, T>,
    asset_index: usize,
) -> V16Result<AssetStateV16> {
    assert!(ASSET < 2);
    assert!(
        asset_index == ASSET,
        "committed-price fallback receives the engine-selected asset"
    );
    let persisted = &market.markets[ASSET].engine.asset;
    assert!(
        persisted.market_id.get() == ASSET as u64 + 1,
        "committed-price fallback reads the selected market"
    );
    let mut asset = AssetStateV16::default();
    asset.market_id = persisted.market_id.get();
    asset.lifecycle = AssetLifecycleV16::Active;
    asset.effective_price = persisted.effective_price.get();
    Ok(asset)
}

#[cfg(all(kani, feature = "closure"))]
fn stale_current_asset_zero_state_stub<'a: 'a, T>(
    market: &MarketGroupV16ViewMut<'a, T>,
    asset_index: usize,
) -> V16Result<AssetStateV16> {
    stale_current_asset_state_stub::<0, T>(market, asset_index)
}

#[cfg(all(kani, feature = "closure"))]
fn stale_current_asset_one_state_stub<'a: 'a, T>(
    market: &MarketGroupV16ViewMut<'a, T>,
    asset_index: usize,
) -> V16Result<AssetStateV16> {
    stale_current_asset_state_stub::<1, T>(market, asset_index)
}

#[cfg(all(kani, feature = "closure"))]
fn unreachable_stale_cert_liquidation_stub<'a: 'a, T>(
    _market: &mut MarketGroupV16ViewMut<'a, T>,
    _account: &mut PortfolioV16ViewMut<'_>,
    _request: LiquidationRequestV16,
) -> V16Result<LiquidationOutcomeV16> {
    panic!("stale certificate dispatched liquidation before refresh")
}

// Stale-certificate liquidation safety and progress. A persisted deficit
// certified at an old oracle epoch must enter the real Refresh arm, never
// Liquidate, use the first active leg's engine-owned asset, clear stale state,
// and forward committed observation data to accrual only after recertification.
#[cfg(all(kani, feature = "closure"))]
fn prove_stale_deficit_auto_crank_refreshes_active_asset<const ASSET: usize>() {
    assert!(ASSET < 2);
    let (mut header, mut markets, mut account_header) = two_asset_kf_mapping_fixture();
    account_header
        .init_empty_in_place(account_header.provenance_header)
        .unwrap();
    let asset = markets[ASSET].engine.asset.try_to_runtime().unwrap();
    let leg = PortfolioLegV16 {
        active: true,
        asset_index: ASSET as u32,
        market_id: asset.market_id,
        side: SideV16::Long,
        basis_pos_q: POS_SCALE as i128,
        a_basis: ADL_ONE,
        k_snap: asset.k_long,
        f_snap: asset.f_long_num,
        epoch_snap: asset.epoch_long,
        loss_weight: POS_SCALE,
        b_snap: asset.b_long_num,
        b_epoch_snap: asset.epoch_long,
        ..PortfolioLegV16::EMPTY
    };
    header.oracle_epoch = V16PodU64::new(1);
    header.stale_certificate_count = V16PodU64::new(1);
    account_header.legs[0] = PortfolioLegV16Account::from_runtime(&leg);
    account_header.active_bitmap[0] = V16PodU64::new(1);
    account_header.stale_state = 1;
    let certified_liq_deficit: u128 = kani::any();
    kani::assume(certified_liq_deficit > 0 && certified_liq_deficit < (1u128 << 64));
    let bitmap = account_header.active_bitmap.map(V16PodU64::get);
    account_header.health_cert = HealthCertV16Account::from_runtime(&HealthCertV16 {
        certified_equity: 0,
        certified_initial_req: 2,
        certified_maintenance_req: 1,
        certified_liq_deficit,
        certified_worst_case_loss: 1,
        cert_oracle_epoch: 0,
        cert_funding_epoch: header.funding_epoch.get(),
        cert_risk_epoch: header.risk_epoch.get(),
        cert_asset_set_epoch: header.asset_set_epoch.get(),
        active_bitmap_at_cert: bitmap,
        valid: true,
    });

    let header_before = header;
    let account_before = account_header;
    let markets_before = [markets[0].engine, markets[1].engine];
    let mut expected_cert = account_before.health_cert.try_to_runtime().unwrap();
    expected_cert.cert_oracle_epoch = header_before.oracle_epoch.get();
    expected_cert.cert_funding_epoch = header_before.funding_epoch.get();
    expected_cert.cert_risk_epoch = header_before.risk_epoch.get();
    expected_cert.cert_asset_set_epoch = header_before.asset_set_epoch.get();
    expected_cert.active_bitmap_at_cert = account_before.active_bitmap.map(V16PodU64::get);
    kani::cover!(
        certified_liq_deficit > 1,
        "stale refresh covers non-unit certified liquidation deficits"
    );
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    let result = market
        .permissionless_auto_crank_not_atomic(
            &mut account,
            AutoCrankWorkV16 {
                now_slot: header_before.current_slot.get(),
                observations: &[],
                resolved_close_fee_rate_per_slot: 0,
            },
        )
        .unwrap();
    assert_eq!(
        result,
        AutoCrankResultV16 {
            selected: AutoCrankPlanV16::RefreshAccount {
                asset_index: Some(ASSET),
            },
            outcome: AutoCrankOutcomeV16::Progressed(
                PermissionlessProgressOutcomeV16::AccountCurrent,
            ),
        }
    );
    assert_eq!(market.header.stale_certificate_count.get(), 0);
    assert_eq!(account.header.stale_state, 0);
    assert!(kani_eq_health_cert_v16_account(
        &HealthCertV16Account::from_runtime(&expected_cert),
        &account.header.health_cert,
    ));

    // Refresh is value-neutral: it can update only certification metadata and
    // the matching stale counter. These are the stock/claim fields whose drift
    // could create LoF or cross-domain leakage.
    assert_eq!(market.header.vault.get(), header_before.vault.get());
    assert_eq!(market.header.insurance.get(), header_before.insurance.get());
    assert_eq!(market.header.c_tot.get(), header_before.c_tot.get());
    assert_eq!(
        market.header.pnl_pos_tot.get(),
        header_before.pnl_pos_tot.get()
    );
    assert_eq!(
        market.header.pnl_pos_bound_tot_num.get(),
        header_before.pnl_pos_bound_tot_num.get()
    );
    assert_eq!(
        market.header.source_claim_bound_total_num.get(),
        header_before.source_claim_bound_total_num.get()
    );
    assert_eq!(
        market.header.source_fresh_backing_total_num.get(),
        header_before.source_fresh_backing_total_num.get()
    );
    assert_eq!(
        market
            .header
            .source_insurance_credit_reserved_total_atoms
            .get(),
        header_before
            .source_insurance_credit_reserved_total_atoms
            .get()
    );
    assert_eq!(account.header.capital.get(), account_before.capital.get());
    assert_eq!(account.header.pnl.get(), account_before.pnl.get());
    assert_eq!(
        account.header.reserved_pnl.get(),
        account_before.reserved_pnl.get()
    );
    assert_eq!(
        account.header.active_bitmap[0].get(),
        account_before.active_bitmap[0].get()
    );
    assert!(kani_eq_portfolio_leg_v16_account(
        &account_before.legs[0],
        &account.header.legs[0],
    ));
    let mut asset_index = 0usize;
    while asset_index < 2 {
        assert_eq!(
            market.markets[asset_index]
                .engine
                .insurance_domain_budget_long
                .get(),
            markets_before[asset_index]
                .insurance_domain_budget_long
                .get()
        );
        assert_eq!(
            market.markets[asset_index]
                .engine
                .insurance_domain_budget_short
                .get(),
            markets_before[asset_index]
                .insurance_domain_budget_short
                .get()
        );
        assert_eq!(
            market.markets[asset_index]
                .engine
                .insurance_domain_spent_long
                .get(),
            markets_before[asset_index]
                .insurance_domain_spent_long
                .get()
        );
        assert_eq!(
            market.markets[asset_index]
                .engine
                .insurance_domain_spent_short
                .get(),
            markets_before[asset_index]
                .insurance_domain_spent_short
                .get()
        );
        asset_index += 1;
    }
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(17)]
#[kani::solver(cadical)]
#[kani::stub(
    MarketGroupV16ViewMut::auto_crank_selected_assets,
    selected_active_asset_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::has_b_stale_leg,
    selected_leg_is_no_longer_b_stale_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::asset_state,
    stale_current_asset_zero_state_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_unconfigured_market_tail,
    valid_two_asset_market_tail_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::refresh_account_and_certify_not_atomic,
    stale_cert_refresh_dispatch_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::accrue_asset_to_not_atomic,
    stale_cert_refresh_asset_zero_accrual_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::liquidate_account_not_atomic,
    unreachable_stale_cert_liquidation_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::close_resolved_account_not_atomic,
    unreachable_resolved_close_stub
)]
fn closure_asset_zero_stale_deficit_auto_crank_refreshes_active_asset() {
    prove_stale_deficit_auto_crank_refreshes_active_asset::<0>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(17)]
#[kani::solver(cadical)]
#[kani::stub(
    MarketGroupV16ViewMut::auto_crank_selected_assets,
    selected_active_asset_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::has_b_stale_leg,
    selected_leg_is_no_longer_b_stale_stub
)]
#[kani::stub(MarketGroupV16ViewMut::asset_state, stale_current_asset_one_state_stub)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_unconfigured_market_tail,
    valid_two_asset_market_tail_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::refresh_account_and_certify_not_atomic,
    stale_cert_refresh_dispatch_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::accrue_asset_to_not_atomic,
    stale_cert_refresh_asset_one_accrual_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::liquidate_account_not_atomic,
    unreachable_stale_cert_liquidation_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::close_resolved_account_not_atomic,
    unreachable_resolved_close_stub
)]
fn closure_asset_one_stale_deficit_auto_crank_refreshes_active_asset() {
    prove_stale_deficit_auto_crank_refreshes_active_asset::<1>();
}

#[cfg(all(kani, feature = "closure"))]
fn selected_observation_permissionless_crank_stub<'a: 'a, T>(
    market: &mut MarketGroupV16ViewMut<'a, T>,
    account: &mut PortfolioV16ViewMut<'_>,
    request: PermissionlessCrankRequestV16,
) -> V16Result<PermissionlessProgressOutcomeV16>
where
    T: Into<u64> + Copy,
{
    let leg = account.header.legs[0].try_to_runtime()?;
    let selected = leg.asset_index as usize;
    let selected_price: u64 = market.markets[selected].wrapper.into();
    assert!(request.asset_index == selected);
    assert!(request.effective_price == selected_price);
    assert!(request.funding_rate_e9 == selected_price as i128);
    assert!(request.now_slot == market.header.current_slot.get());
    assert!(matches!(
        request.action,
        PermissionlessCrankActionV16::Liquidate(LiquidationRequestV16 { asset_index })
            if asset_index == selected
    ));
    Ok(PermissionlessProgressOutcomeV16::AccountCurrent)
}

// Permissionless observation isolation. With authenticated observations for
// both assets in either order, the real auto-crank lookup must forward only the
// engine-selected active asset's price/funding pair. A first-entry or wrong-
// asset lookup would let one oracle domain drive another domain's liquidation.
#[cfg(all(kani, feature = "closure"))]
fn prove_auto_crank_routes_only_selected_asset_observation<const ASSET: usize>() {
    assert!(ASSET < 2);
    let other = 1 - ASSET;
    let (mut header, mut markets, mut account_header) = two_asset_kf_mapping_fixture();
    account_header
        .init_empty_in_place(account_header.provenance_header)
        .unwrap();
    let asset = markets[ASSET].engine.asset.try_to_runtime().unwrap();
    account_header.active_bitmap[0] = V16PodU64::new(1);
    account_header.legs[0] = PortfolioLegV16Account::from_runtime(&PortfolioLegV16 {
        active: true,
        asset_index: ASSET as u32,
        market_id: asset.market_id,
        side: SideV16::Long,
        basis_pos_q: POS_SCALE as i128,
        a_basis: ADL_ONE,
        loss_weight: POS_SCALE,
        ..PortfolioLegV16::EMPTY
    });
    let bitmap = account_header.active_bitmap.map(V16PodU64::get);
    account_header.health_cert = HealthCertV16Account::from_runtime(&HealthCertV16 {
        certified_liq_deficit: 1,
        cert_oracle_epoch: header.oracle_epoch.get(),
        cert_funding_epoch: header.funding_epoch.get(),
        cert_risk_epoch: header.risk_epoch.get(),
        cert_asset_set_epoch: header.asset_set_epoch.get(),
        active_bitmap_at_cert: bitmap,
        valid: true,
        ..HealthCertV16::default()
    });

    let selected_price: u64 = kani::any();
    let other_price: u64 = kani::any();
    kani::assume(selected_price > 0 && selected_price <= MAX_ORACLE_PRICE);
    kani::assume(other_price > 0 && other_price <= MAX_ORACLE_PRICE);
    kani::assume(selected_price != other_price);
    markets[ASSET].wrapper = selected_price;
    markets[other].wrapper = other_price;
    let selected_observation = AutoCrankObservationV16 {
        asset_index: ASSET,
        effective_price: selected_price,
        funding_rate_e9: selected_price as i128,
    };
    let other_observation = AutoCrankObservationV16 {
        asset_index: other,
        effective_price: other_price,
        funding_rate_e9: -(other_price as i128),
    };
    let reversed: bool = kani::any();
    let observations = if reversed {
        [other_observation, selected_observation]
    } else {
        [selected_observation, other_observation]
    };

    let header_before = header;
    let account_before = account_header;
    let engines_before = [markets[0].engine, markets[1].engine];
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    let result = market
        .permissionless_auto_crank_not_atomic(
            &mut account,
            AutoCrankWorkV16 {
                now_slot: header_before.current_slot.get(),
                observations: &observations,
                resolved_close_fee_rate_per_slot: 0,
            },
        )
        .unwrap();

    assert_eq!(
        result,
        AutoCrankResultV16 {
            selected: AutoCrankPlanV16::Liquidate { asset_index: ASSET },
            outcome: AutoCrankOutcomeV16::Progressed(
                PermissionlessProgressOutcomeV16::AccountCurrent,
            ),
        }
    );
    assert!(kani_eq_market_group_v16_header_account(
        &header_before,
        market.header
    ));
    assert!(kani_eq_portfolio_account_v16_account(
        &account_before,
        account.header
    ));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &engines_before[0],
        &market.markets[0].engine
    ));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &engines_before[1],
        &market.markets[1].engine
    ));
    kani::cover!(!reversed, "selected observation appears first");
    kani::cover!(reversed, "selected observation appears second");
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
#[kani::stub(
    MarketGroupV16ViewMut::build_actionable_summary,
    current_liquidatable_summary_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::auto_crank_selected_assets,
    selected_active_asset_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::permissionless_crank_not_atomic,
    selected_observation_permissionless_crank_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::close_resolved_account_not_atomic,
    unreachable_resolved_close_stub
)]
fn closure_asset_zero_auto_crank_routes_only_selected_observation() {
    prove_auto_crank_routes_only_selected_asset_observation::<0>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
#[kani::stub(
    MarketGroupV16ViewMut::build_actionable_summary,
    current_liquidatable_summary_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::auto_crank_selected_assets,
    selected_active_asset_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::permissionless_crank_not_atomic,
    selected_observation_permissionless_crank_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::close_resolved_account_not_atomic,
    unreachable_resolved_close_stub
)]
fn closure_asset_one_auto_crank_routes_only_selected_observation() {
    prove_auto_crank_routes_only_selected_asset_observation::<1>();
}

#[cfg(all(kani, feature = "closure"))]
fn no_active_auto_crank_assets_stub<'a: 'a, T>(
    account: &PortfolioV16View<'_>,
) -> V16Result<(Option<usize>, Option<usize>)> {
    let _ = core::marker::PhantomData::<(&'a (), T)>;
    assert!(active_bitmap_is_empty(
        account.header.active_bitmap.map(V16PodU64::get)
    ));
    Ok((None, None))
}

#[cfg(all(kani, feature = "closure"))]
fn resolved_winner_account_validation_stub<'a: 'a, T>(
    account: &PortfolioV16View<'a>,
    market: &MarketGroupV16View<'_, T>,
) -> V16Result<()> {
    assert_eq!(
        account.header.provenance_header.market_group_id,
        market.header.market_group_id
    );
    assert_eq!(account.header.owner, account.header.provenance_header.owner);
    assert_eq!(account.header.capital.get(), 0);
    assert!((0..=2).contains(&account.header.pnl.get()));
    assert_eq!(
        account.header.pnl.get() as u128,
        market.header.pnl_pos_tot.get()
    );
    assert_eq!(account.header.reserved_pnl.get(), 0);
    assert_eq!(account.header.fee_credits.get(), 0);
    assert_eq!(
        account.header.last_fee_slot.get(),
        market.header.resolved_slot.get()
    );
    assert!(active_bitmap_is_empty(
        account.header.active_bitmap.map(V16PodU64::get)
    ));
    Ok(())
}

#[cfg(all(kani, feature = "closure"))]
fn resolved_winner_market_validation_stub<'a: 'a, T>(
    market: &MarketGroupV16ViewMut<'a, T>,
) -> V16Result<()> {
    assert_eq!(market.markets.len(), 2);
    assert_eq!(
        decode_market_mode(market.header.mode)?,
        MarketModeV16::Resolved
    );
    assert_eq!(market.header.vault.get(), 10);
    assert_eq!(market.header.c_tot.get(), 0);
    assert_eq!(market.header.insurance.get(), 10);
    assert_eq!(market.header.pnl_pos_tot.get(), 0);
    assert_eq!(market.header.pnl_pos_bound_tot.get(), 0);
    assert_eq!(market.header.pnl_pos_bound_tot_num.get(), 0);
    assert_eq!(market.header.payout_snapshot_captured, 1);
    assert!((1..=2).contains(&market.header.payout_snapshot.get()));
    Ok(())
}

#[cfg(all(kani, feature = "closure"))]
fn unreachable_permissionless_crank_stub<'a: 'a, T>(
    _market: &mut MarketGroupV16ViewMut<'a, T>,
    _account: &mut PortfolioV16ViewMut<'_>,
    _request: PermissionlessCrankRequestV16,
) -> V16Result<PermissionlessProgressOutcomeV16> {
    panic!("resolved winner selected a live-market crank action")
}

// First-winner payout liveness and conservation. In Resolved mode, a positive-
// PnL account with every blocker clear must select CloseResolved before the
// payout snapshot exists, lazily capture it, pay exactly the funded claim, and
// leave a terminal finalized receipt. Requiring pre-capture would deadlock every
// winner and permanently strand the residual vault.
#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
#[kani::stub(crate::wide_math::div_rem_u256, bounded_u256_div_rem_stub)]
#[kani::stub(
    MarketGroupV16ViewMut::permissionless_crank_not_atomic,
    unreachable_permissionless_crank_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::auto_crank_selected_assets,
    no_active_auto_crank_assets_stub
)]
#[kani::stub(
    PortfolioV16View::validate_with_market,
    resolved_winner_account_validation_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_shape,
    resolved_winner_market_validation_stub
)]
fn closure_resolved_first_winner_auto_crank_reaches_snapshot_capture() {
    let pnl_raw: u8 = kani::any();
    kani::assume((1..=2).contains(&pnl_raw));
    let pnl = pnl_raw as u128;
    let (mut header, mut markets, mut account_header) = two_asset_kf_mapping_fixture();
    account_header
        .init_empty_in_place(account_header.provenance_header)
        .unwrap();
    header.mode = encode_market_mode(MarketModeV16::Resolved);
    header.resolved_slot = header.current_slot;
    header.pnl_pos_tot = V16PodU128::new(pnl);
    header.pnl_pos_bound_tot = V16PodU128::new(pnl);
    header.pnl_pos_bound_tot_num = V16PodU128::new(pnl * BOUND_SCALE);
    header.vault = V16PodU128::new(header.insurance.get() + pnl);
    account_header.pnl = V16PodI128::new(pnl as i128);
    account_header.last_fee_slot = header.resolved_slot;

    let header_before = header;
    let account_before = account_header;
    let markets_before = [markets[0].engine, markets[1].engine];
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    let result = market
        .permissionless_auto_crank_not_atomic(
            &mut account,
            AutoCrankWorkV16 {
                now_slot: header_before.current_slot.get(),
                observations: &[],
                resolved_close_fee_rate_per_slot: 0,
            },
        )
        .unwrap();

    kani::cover!(
        pnl == 2 && header_before.payout_snapshot_captured == 0,
        "first-winner route covers a multi-atom claim before snapshot capture"
    );
    assert_eq!(
        result,
        AutoCrankResultV16 {
            selected: AutoCrankPlanV16::CloseResolved,
            outcome: AutoCrankOutcomeV16::ResolvedClose(ResolvedCloseOutcomeV16::Closed {
                payout: pnl,
            }),
        }
    );
    assert_eq!(market.header.vault.get(), header_before.vault.get() - pnl);
    assert_eq!(market.header.c_tot.get(), header_before.c_tot.get());
    assert_eq!(market.header.insurance.get(), header_before.insurance.get());
    assert_eq!(market.header.pnl_pos_tot.get(), 0);
    assert_eq!(market.header.pnl_pos_bound_tot.get(), 0);
    assert_eq!(market.header.pnl_pos_bound_tot_num.get(), 0);
    assert_eq!(market.header.payout_snapshot_captured, 1);
    assert_eq!(market.header.payout_snapshot.get(), pnl);
    assert_eq!(account.header.capital.get(), 0);
    assert_eq!(account.header.pnl.get(), 0);
    assert_eq!(account.header.reserved_pnl.get(), 0);
    assert_eq!(account.header.fee_credits.get(), 0);
    assert_eq!(
        account.header.last_fee_slot.get(),
        account_before.last_fee_slot.get()
    );
    let receipt = account
        .header
        .resolved_payout_receipt
        .try_to_runtime()
        .unwrap();
    assert!(receipt.present && receipt.finalized);
    assert_eq!(receipt.prior_bound_contribution_num, pnl * BOUND_SCALE);
    assert_eq!(receipt.terminal_positive_claim_face, pnl);
    assert_eq!(receipt.paid_effective, pnl);
    assert!(kani_eq_engine_asset_slot_v16_account(
        &markets_before[0],
        &market.markets[0].engine
    ));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &markets_before[1],
        &market.markets[1].engine
    ));
}

// ============ NO-DoS GATE-REACHABILITY (existential liveness) ============
// The review's closable half: for the two kernel-backed actionable classes,
// prove ActionableClass(S) => EXISTS a successful rank-decreasing call —
// purely, by exhibiting the witness and showing the proven rank kernel accepts
// it and strictly decreases the rank. This converts "gate reachability
// backstopped" to machine-checked for these classes. (Closure layer: the
// kernels run as plain code, no contract-attr interaction.)

// A3 pending close: any actionable pending-close ledger (valid identity,
// residual > 0) admits a successful advance that strictly decreases residual.
#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn liveness_pending_close_has_rank_decreasing_advance() {
    let ledger = CloseProgressLedgerV16 {
        active: true,
        finalized: false,
        canceled: false,
        close_id: kani::any(),
        asset_index: kani::any(),
        market_id: kani::any(),
        domain_side: if kani::any() {
            SideV16::Long
        } else {
            SideV16::Short
        },
        gross_loss_at_close_start: kani::any(),
        drift_reference_slot: kani::any(),
        max_close_slot: kani::any(),
        support_consumed: kani::any(),
        junior_face_burned: kani::any(),
        insurance_spent: kani::any(),
        b_loss_booked: kani::any(),
        explicit_loss_assigned: kani::any(),
        quantity_adl_applied_q: kani::any(),
        drift_consumed: kani::any(),
        residual_remaining: kani::any(),
    };
    // validated-ledger precondition (production-guaranteed) + actionable: residual > 0
    kani::assume(ledger.gross_loss_at_close_start < 1u128 << 64);
    kani::assume(ledger.drift_consumed < 1u128 << 64);
    kani::assume(ledger.support_consumed < 1u128 << 64);
    kani::assume(ledger.insurance_spent < 1u128 << 64);
    kani::assume(ledger.b_loss_booked < 1u128 << 64);
    kani::assume(ledger.explicit_loss_assigned < 1u128 << 64);
    let total = ledger.gross_loss_at_close_start + ledger.drift_consumed;
    let progress = ledger.support_consumed
        + ledger.insurance_spent
        + ledger.b_loss_booked
        + ledger.explicit_loss_assigned;
    kani::assume(progress <= total);
    kani::assume(ledger.residual_remaining == total - progress);
    kani::assume(ledger.residual_remaining > 0); // ACTIONABLE

    // WITNESS: booking exactly 1 unit of explicit loss is a valid successful
    // continuation (the simplest progress) and strictly decreases the rank.
    let r = V16Core::kernel_advance_close_ledger(ledger, 0, 0, 0, 0, 1);
    assert!(
        r.is_ok(),
        "an actionable pending close ALWAYS admits a progress booking"
    );
    let after = r.unwrap();
    assert!(
        after.residual_remaining < ledger.residual_remaining,
        "the successful continuation strictly decreases the close rank"
    );
}

// A2 b-stale leg: any leg behind its B target (b_target > b_snap) admits a
// successful chunk that strictly advances b_snap toward the target.
#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn liveness_b_stale_leg_has_advancing_chunk() {
    let leg = PortfolioLegV16 {
        active: true,
        asset_index: kani::any(),
        market_id: kani::any(),
        side: if kani::any() {
            SideV16::Long
        } else {
            SideV16::Short
        },
        basis_pos_q: kani::any(),
        a_basis: kani::any(),
        k_snap: kani::any(),
        f_snap: kani::any(),
        epoch_snap: kani::any(),
        loss_weight: kani::any(),
        b_snap: kani::any(),
        b_rem: kani::any(),
        b_epoch_snap: kani::any(),
        b_stale: true,
        stale: kani::any(),
    };
    let b_target: u128 = kani::any();
    kani::assume(leg.b_snap < 1u128 << 64);
    kani::assume(b_target > leg.b_snap); // ACTIONABLE: behind target
                                         // WITNESS: a chunk of delta_b = min(target - snap, ...) advances toward the
                                         // target; use delta_b = 1 (>=1 since target > snap) -- proven monotone.
    let delta_b: u128 = 1;
    let remaining_after = b_target - leg.b_snap - delta_b;
    let r = V16Core::kernel_advance_leg_b_snap(leg, delta_b, 0, remaining_after);
    assert!(
        r.is_ok(),
        "an actionable b-stale leg ALWAYS admits an advancing chunk"
    );
    let after = r.unwrap();
    assert!(
        after.b_snap > leg.b_snap,
        "the chunk strictly advances b_snap toward target"
    );
    assert!(
        after.b_snap <= b_target,
        "advance never overshoots the target"
    );
}

// ============ DIVISION-AXIOM ROUTE (kernel-proofs) ============
// The sound path past the SAT-hard wide-division wall: replace the division
// helper with an EXACT SPECIFICATION AXIOM (kani::any() result constrained by
// the ceil relation — no division circuit to bit-blast), prove the real public
// body's VALUE composition under the axiom, and discharge the narrow remaining
// obligation `production helper == axiom` by differential fuzz (below).
//
// Unlike the frame-only composition (arbitrary division result, sound only for
// WHERE fields land), this axiom is spec-EXACT, so it is sound for VALUE /
// conservation claims: the proof can reason about the exact ceil-division
// result without the solver computing it.

// The DivisionAxiom's well-formedness (it returns a value satisfying the exact
// ceil relation) and the engine-range guarantee PRODUCTION == axiom are BOTH
// discharged by the differential fuzz loss_weight_helper_matches_division_axiom
// (20k cases + rounding/denominator edges). A Kani self-consistency proof is NOT
// kept here: even bounded-width it must symex CBMC's u128 div/mul circuits,
// which are structurally 128-bit and do not collapse under operand bounds, so it
// times out with no added assurance over the fuzz.
// The VALUE composition over a real body (weight_sum += ceil(abs*S/a)) is the
// LOGICAL composition of: (1) kernel_attach_leg's contract weight_sum +=
// loss_weight for ANY weight; (2) the fuzz-discharged axiom loss_weight ==
// ceil(abs*S/a_basis). Forcing both into ONE Kani query times out; the
// transitive composition is sound without it.

// VALUE-CONSERVATION composition under the CORRECTED arithmetic axiom: the
// division helper is stubbed to an opaque NONZERO value (NO wide-arithmetic
// circuit in the axiom — the review's key refinement), and the proof asserts
// the conservation DELTAS that don't need the weight's exact value:
// oi_eff_long += abs and loss_weight_sum_long += the helper's (opaque) weight.
// The weight's EXACT value (== ceil(abs*S/a)) is the fuzz obligation
// (loss_weight_helper_matches_division_axiom), NOT asserted here. Composition:
// (this: weight_sum += w) + (fuzz: w == ceil) => weight_sum += ceil — sound,
// and tractable because Kani never touches the wide arithmetic.
#[cfg(all(kani, feature = "contracts"))]
fn axiom_loss_weight_nonzero(_abs: u128, a: u128) -> V16Result<u128> {
    if a == 0 {
        return Err(V16Error::InvalidLeg);
    }
    let w: u128 = kani::any();
    kani::assume(w != 0); // the only property attach's logic branches on
    Ok(w)
}

#[cfg(all(kani, feature = "contracts"))]
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
#[kani::stub(crate::v16::loss_weight_for_basis, axiom_loss_weight_nonzero)]
#[kani::stub_verified(V16Core::kernel_attach_leg)]
fn composition_attach_value_conservation_under_axiom() {
    let cfg = V16Config::public_user_fund_with_market_slots(1, 1, 0, 10);
    let mut header = MarketGroupV16HeaderAccount::new_dynamic([1u8; 32], cfg, 1, 0).unwrap();
    let mut markets = [Market::new(0u64, EngineAssetSlotV16Account::default())];
    {
        let mut v = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        v.activate_empty_market_not_atomic(0, 100, 1).unwrap();
    }
    let prov = ProvenanceHeaderV16Account::from_runtime(&ProvenanceHeaderV16::new(
        [1u8; 32], [2u8; 32], [2u8; 32],
    ));
    let mut account_header = PortfolioAccountV16Account::default();
    account_header.init_empty_in_place(prov).unwrap();
    account_header.last_fee_slot = V16PodU64::new(1);
    let basis: i128 = kani::any();
    kani::assume(basis > 0 && basis <= MAX_POSITION_ABS_Q as i128);
    let abs = basis.unsigned_abs();
    let oi0 = markets[0]
        .engine
        .asset
        .try_to_runtime()
        .unwrap()
        .oi_eff_long_q;
    let ws0 = markets[0]
        .engine
        .asset
        .try_to_runtime()
        .unwrap()
        .loss_weight_sum_long;
    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        let mut account = PortfolioV16ViewMut::new(&mut account_header);
        if market
            .kani_attach_leg_at_slot(&mut account, 0, SideV16::Long, basis, 0)
            .is_err()
        {
            return;
        }
    }
    kani::cover!(true, "value-conservation under axiom reached");
    // Read POST-state fields RAW (.get()) — NOT try_to_runtime(): the kernel
    // contract havocs the asset/leg to satisfy its ensures and does not promise
    // the havoc'd POD re-passes full validation, but the specific u128/i128
    // fields the ensures pins round-trip losslessly.
    let oi1 = markets[0].engine.asset.oi_eff_long_q.get();
    let ws1 = markets[0].engine.asset.loss_weight_sum_long.get();
    let leg_weight = account_header.legs[0].loss_weight.get();
    let leg_basis = account_header.legs[0].basis_pos_q.get();
    // CONSERVATION (no wide arithmetic): OI rises by exactly abs; the side
    // weight sum rises by exactly the weight written to the leg.
    assert_eq!(oi1, oi0.wrapping_add(abs));
    assert_eq!(ws1, ws0.wrapping_add(leg_weight));
    assert_eq!(leg_basis, basis);
}

// VALUE-CONSERVATION composition for the CLEAR body — the inverse of attach,
// and a second instance of the helper-stub recipe (the review's named next
// candidate). clear has NO division (it subtracts the leg's STORED weight), so
// the only stub needed is for the attach setup. The conservation claim: clearing
// the freshly-attached leg removes EXACTLY what attach added —
// oi_eff_long -= the leg's stored abs basis and loss_weight_sum_long -= the leg's
// stored weight — so attach;clear is an exact OI/weight round-trip on the asset.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
#[kani::stub(crate::v16::loss_weight_for_basis, axiom_loss_weight_nonzero)]
#[kani::stub_verified(V16Core::kernel_clear_leg)]
#[kani::stub_verified(V16Core::kernel_attach_leg)]
fn composition_clear_leg_value_conservation() {
    let cfg = V16Config::public_user_fund_with_market_slots(1, 1, 0, 10);
    let mut header = MarketGroupV16HeaderAccount::new_dynamic([1u8; 32], cfg, 1, 0).unwrap();
    let mut markets = [Market::new(0u64, EngineAssetSlotV16Account::default())];
    {
        let mut v = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        v.activate_empty_market_not_atomic(0, 100, 1).unwrap();
    }
    let prov = ProvenanceHeaderV16Account::from_runtime(&ProvenanceHeaderV16::new(
        [1u8; 32], [2u8; 32], [2u8; 32],
    ));
    let mut account_header = PortfolioAccountV16Account::default();
    account_header.init_empty_in_place(prov).unwrap();
    account_header.last_fee_slot = V16PodU64::new(1);
    let basis: i128 = kani::any();
    kani::assume(basis > 0 && basis <= MAX_POSITION_ABS_Q as i128);
    let oi0 = markets[0]
        .engine
        .asset
        .try_to_runtime()
        .unwrap()
        .oi_eff_long_q;
    let ws0 = markets[0]
        .engine
        .asset
        .try_to_runtime()
        .unwrap()
        .loss_weight_sum_long;
    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        let mut account = PortfolioV16ViewMut::new(&mut account_header);
        if market
            .kani_attach_leg_at_slot(&mut account, 0, SideV16::Long, basis, 0)
            .is_err()
        {
            return;
        }
    }
    // post-attach asset/leg state, read RAW (the attach kernel havoc'd the POD)
    let oi_mid = markets[0].engine.asset.oi_eff_long_q.get();
    let ws_mid = markets[0].engine.asset.loss_weight_sum_long.get();
    let leg_weight = account_header.legs[0].loss_weight.get();
    let leg_abs = account_header.legs[0].basis_pos_q.get().unsigned_abs();
    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        let mut account = PortfolioV16ViewMut::new(&mut account_header);
        if market.kani_clear_leg(&mut account, 0).is_err() {
            return;
        }
    }
    kani::cover!(true, "clear value-conservation reached");
    let oi1 = markets[0].engine.asset.oi_eff_long_q.get();
    let ws1 = markets[0].engine.asset.loss_weight_sum_long.get();
    // CONSERVATION: clear removes EXACTLY the leg's stored basis/weight ...
    assert_eq!(oi1, oi_mid.wrapping_sub(leg_abs));
    assert_eq!(ws1, ws_mid.wrapping_sub(leg_weight));
    // ... and attach;clear is an exact round-trip back to the pre-attach asset.
    assert_eq!(oi1, oi0);
    assert_eq!(ws1, ws0);
}

// ROADMAP Phase 3 (Pillar S, S-A1/S-L2 foundation): full-domain contract check
// of the principal-settlement kernel — paid==min(capital,|pnl|), capital and
// c_tot each drop by exactly paid (conservation, no value leaked), loss reduced
// by exactly paid. Production (settle_negative_pnl_from_principal_core) calls
// this for the arithmetic; the ensures clause is the importable property.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_settle_principal)]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn contract_check_kernel_settle_principal() {
    let capital: u128 = kani::any();
    let c_tot: u128 = kani::any();
    let pnl: i128 = kani::any();
    let _ = V16Core::kernel_settle_principal(capital, c_tot, pnl);
}

// ROADMAP Phase 3 (Pillar S, S-L2 insurance layer): full-domain contract check
// of the insurance-draw kernel — used==min(|pnl|, domain_available), capped by
// the domain budget (isolation), pool->spent conservation. Production
// (consume_domain_insurance_for_negative_pnl) calls this for the arithmetic.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_consume_insurance_layer)]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn contract_check_kernel_consume_insurance_layer() {
    let domain_available: u128 = kani::any();
    let insurance: u128 = kani::any();
    let spent: u128 = kani::any();
    let pnl: i128 = kani::any();
    let _ = V16Core::kernel_consume_insurance_layer(domain_available, insurance, spent, pnl);
}

// ROADMAP Phase 3A.3 (Pillar S, S-C2): full-domain contract check of the
// resolved-payout draining step — payout==min(claimable,vault), vault drops by
// exactly payout (never overdrawn, never leaked). Production
// (claim_resolved_payout_topup_core) calls this for the draining arithmetic.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_resolved_payout_step)]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn contract_check_kernel_resolved_payout_step() {
    let claimable: u128 = kani::any();
    let vault: u128 = kani::any();
    let _ = V16Core::kernel_resolved_payout_step(claimable, vault);
}

// ROADMAP Phase 3A.1 (Pillar S, trade spine): full-domain contract check of the
// position-route classifier — the exact (Attach/Clear/Flip/Resize) decision the
// position-delta body dispatches on. Production (apply_position_delta_with_lookup
// _inner) calls this for the route decision.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_classify_position_delta)]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn contract_check_kernel_classify_position_delta() {
    let current: i128 = kani::any();
    let new: i128 = kani::any();
    let _ = V16Core::kernel_classify_position_delta(current, new);
}

// ROADMAP Phase 3A.2 (Pillar S/L, S-L3 + A5.dec rank): full-domain contract check
// of the position-reduction kernel — reduce_q==min(requested,|pre|), |pre+delta|
// == |pre|-reduce_q (rank strictly decreases, never over-closes/flips), full
// close clears. Production (rebalance_reduce_position) calls this.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_reduce_position_delta)]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn contract_check_kernel_reduce_position_delta() {
    let pre: i128 = kani::any();
    let side = if kani::any() {
        SideV16::Long
    } else {
        SideV16::Short
    };
    let requested: u128 = kani::any();
    let _ = V16Core::kernel_reduce_position_delta(pre, side, requested);
}

// ROADMAP Phase 4 / 3A.4 (Pillar L, gate-reachability): full-domain contract
// check of the liveness selector — for ANY ActionableState summary it returns
// Some continuation iff actionable (totality), the selected continuation's class
// is actually active (non-blocked — overlap-safe), and the priority is
// deterministic. Composes the per-class rank kernels into one L.sel.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::select_progress_witness)]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn contract_check_select_progress_witness() {
    let summary: ActionableSummaryV16 = kani::any();
    let _ = V16Core::select_progress_witness(summary);
}

// ROADMAP Phase 3B.8 (Pillar L, A7.dec): full-domain contract check of the
// resolved-close progress classifier — Closed iff nothing pending & no recovery,
// ProgressOnly implies a pending rank component exists (real decrease available,
// no spurious non-progress), RecoveryRequired iff the recovery predicate.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_resolved_close_progress)]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn contract_check_kernel_resolved_close_progress() {
    let rank: ResolvedCloseRankV16 = kani::any();
    let _ = V16Core::kernel_resolved_close_progress(rank);
}

// ROADMAP Phase 3B.4 (Pillar L, NB1 composition): full-domain contract check of
// the trade guard-stack admitter — Ok iff EVERY guard passes (valid trade not
// blocked), each rejection attributed to the first failing guard.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_trade_admit)]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn contract_check_kernel_trade_admit() {
    let g: TradeGuardSummaryV16 = kani::any();
    let _ = V16Core::kernel_trade_admit(g);
}

// ROADMAP Phase 3B.6 (Pillar S/L, S-A1 cap): full-domain contract check of the
// social-loss chunk cap — booked == min(residual, public_cap) <= both.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_social_loss_chunk_cap)]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn contract_check_kernel_social_loss_chunk_cap() {
    let residual: u128 = kani::any();
    let cap: u128 = kani::any();
    let _ = V16Core::kernel_social_loss_chunk_cap(residual, cap);
}

// ROADMAP 3C fidelity: full-domain contract check of the trade-request guard
// summary builder — each flag EQUALS its validate_trade_request leaf, so the
// public validator's decision == all_pass (production-faithful, not a model).
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::build_trade_request_guard_summary)]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn contract_check_build_trade_request_guard_summary() {
    let request = TradeRequestV16 {
        asset_index: kani::any(),
        size_q: kani::any(),
        exec_price: kani::any(),
        fee_bps: kani::any(),
    };
    let max_market_slots: u32 = kani::any();
    let max_trading_fee_bps: u64 = kani::any();
    let _ =
        V16Core::build_trade_request_guard_summary(request, max_market_slots, max_trading_fee_bps);
}

// ROADMAP 3C step 2 (NB1 preflight fidelity): full-domain contract check that the
// three preflight summary flags' conjunction EQUALS the production
// trade_preflight_risk_gate accept decision.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_trade_preflight_admits)]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn contract_check_kernel_trade_preflight_admits() {
    let a: bool = kani::any();
    let b: bool = kani::any();
    let c: bool = kani::any();
    let d: bool = kani::any();
    let _ = V16Core::kernel_trade_preflight_admits(a, b, c, d);
}

// ROADMAP 3C step 2 (NB1 accounts_current leaf fidelity): full-domain contract
// check that kernel_cert_is_current == valid && all 4 epochs match && bitmap
// matches — the exact ensure_favorable_action_current_certificate decision.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_cert_is_current)]
#[kani::unwind(16)]
#[kani::solver(cadical)]
fn contract_check_kernel_cert_is_current() {
    let cert = HealthCertV16 {
        certified_equity: kani::any(),
        certified_initial_req: kani::any(),
        certified_maintenance_req: kani::any(),
        certified_liq_deficit: kani::any(),
        certified_worst_case_loss: kani::any(),
        cert_oracle_epoch: kani::any(),
        cert_funding_epoch: kani::any(),
        cert_risk_epoch: kani::any(),
        cert_asset_set_epoch: kani::any(),
        active_bitmap_at_cert: kani::any(),
        valid: kani::any(),
    };
    let _ = V16Core::kernel_cert_is_current(
        cert,
        kani::any(),
        kani::any(),
        kani::any(),
        kani::any(),
        kani::any(),
    );
}

// ROADMAP 3C step 3 (A7 close-rank fidelity): full-domain contract check that
// build_resolved_close_rank maps each real per-component signal to its rank flag
// (b-stale, negative PnL, live leg via non-empty bitmap, capital, receipt,
// recovery). unwind(16): the active-bitmap is-empty scan / memcmp.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::build_resolved_close_rank)]
#[kani::unwind(16)]
#[kani::solver(cadical)]
fn contract_check_build_resolved_close_rank() {
    let _ = V16Core::build_resolved_close_rank(
        kani::any(),
        kani::any(),
        kani::any(),
        kani::any(),
        kani::any(),
        kani::any(),
    );
}

// ROADMAP 3C step 4 (actionable-state classifier fidelity): full-domain contract
// check that actionable_summary_from_signals maps each evaluated per-class
// eligibility signal to its summary flag, so build_actionable_summary faithfully
// feeds the proven select_progress_witness selector.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::actionable_summary_from_signals)]
#[kani::solver(cadical)]
fn contract_check_actionable_summary_from_signals() {
    let _ = V16Core::actionable_summary_from_signals(
        kani::any(),
        kani::any(),
        kani::any(),
        kani::any(),
        kani::any(),
        kani::any(),
        kani::any(),
    );
}

// ROADMAP workstream B.3 (social-loss shell, no-LoF): the live booking division
// social_loss_book_split (engine_chunk*SOCIAL_LOSS_DEN / weight_sum) is a wide
// symbolic u128 division that resists Kani — stub it to an arbitrary (delta_b,
// new_rem). This is SOUND for the SHELL properties below: they assert the booking
// books exactly engine_chunk, conserves the residual, assigns no live explicit
// loss, and makes positive B progress — none of which depend on the split VALUE
// (that exactness is discharged by the social_loss_book_split reference-model
// fuzz conformance). The fn rejects delta_b==0 (-> None), so any Some has delta_b>0.
#[cfg(all(kani, feature = "contracts"))]
fn axiom_social_loss_book_split(
    _engine_chunk: u128,
    _carried_rem: u128,
    weight_sum: u128,
) -> V16Result<(u128, u128)> {
    if weight_sum == 0 {
        return Err(V16Error::InvalidConfig);
    }
    Ok((kani::any(), kani::any()))
}

// ROADMAP workstream B.3: social-loss live-booking conservation + no-explicit-loss
// + B-progress, with the split division stubbed to its fuzz-discharged axiom.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::apply_bankruptcy_residual_chunk_to_loss_side)]
#[kani::stub(crate::v16::social_loss_book_split, axiom_social_loss_book_split)]
#[kani::solver(cadical)]
fn contract_check_bresidual_chunk_conservation() {
    let mut asset: AssetStateV16 = kani::any();
    let opp: SideV16 = kani::any();
    let engine_chunk: u128 = kani::any();
    let residual_remaining: u128 = kani::any();
    let _ = V16Core::apply_bankruptcy_residual_chunk_to_loss_side(
        &mut asset,
        opp,
        engine_chunk,
        residual_remaining,
    );
}

// ROADMAP workstream B.2 (bankruptcy-residual conservation composition): the step
// decision kernel preserves residual conservation on every Outcome path — a
// booked chunk (conservation assumed from the proven leaf contract) is carried
// through, an unbookable residual in a resolved market becomes pure explicit
// loss, and the non-resolved case signals recovery. No value created or lost.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_bresidual_step)]
#[kani::solver(cadical)]
fn contract_check_kernel_bresidual_step() {
    let residual_remaining: u128 = kani::any();
    let booked = if kani::any() {
        Some(BResidualBookingOutcomeV16 {
            booked_loss: kani::any(),
            explicit_loss: kani::any(),
            delta_b: kani::any(),
            remaining_after: kani::any(),
        })
    } else {
        None
    };
    let resolved: bool = kani::any();
    let _ = V16Core::kernel_bresidual_step(residual_remaining, booked, resolved);
}

// ROADMAP workstream B.2 (cross-layer conservation): book_bankruptcy_residual_
// chunk_for_account_core calls the inner booking step on ledger.residual_remaining
// and then advances the close ledger by the outcome's (booked_loss, explicit_loss).
// This proves the two layers AGREE: the ledger's residual AFTER the advance equals
// the booking step's carried-forward remaining_after — no value drifts between the
// social-loss booking and the close-ledger accounting. Composes the proven
// kernel_advance_close_ledger residual identity with the inner-step conservation
// (kernel_bresidual_step: booked+explicit+remaining == residual_in).
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn closure_close_ledger_absorbs_booking_outcome() {
    let ledger = CloseProgressLedgerV16 {
        active: true,
        finalized: false,
        canceled: kani::any(),
        close_id: kani::any(),
        asset_index: kani::any(),
        market_id: kani::any(),
        domain_side: if kani::any() {
            SideV16::Long
        } else {
            SideV16::Short
        },
        gross_loss_at_close_start: kani::any(),
        drift_reference_slot: kani::any(),
        max_close_slot: kani::any(),
        support_consumed: kani::any(),
        junior_face_burned: kani::any(),
        insurance_spent: kani::any(),
        b_loss_booked: kani::any(),
        explicit_loss_assigned: kani::any(),
        quantity_adl_applied_q: kani::any(),
        drift_consumed: kani::any(),
        residual_remaining: kani::any(),
    };
    // validated-ledger precondition (production-guaranteed by
    // validate_close_progress_ledger_with_market): residual == total - progress.
    kani::assume(ledger.gross_loss_at_close_start < 1u128 << 64);
    kani::assume(ledger.drift_consumed < 1u128 << 64);
    kani::assume(ledger.support_consumed < 1u128 << 64);
    kani::assume(ledger.insurance_spent < 1u128 << 64);
    kani::assume(ledger.b_loss_booked < 1u128 << 64);
    kani::assume(ledger.explicit_loss_assigned < 1u128 << 64);
    let total = ledger.gross_loss_at_close_start + ledger.drift_consumed;
    let pre_progress = ledger.support_consumed
        + ledger.insurance_spent
        + ledger.b_loss_booked
        + ledger.explicit_loss_assigned;
    kani::assume(pre_progress <= total);
    kani::assume(ledger.residual_remaining == total - pre_progress);

    // The inner booking step's outcome, conserving ledger.residual_remaining
    // (the postcondition proven by kernel_bresidual_step / the leaf contract).
    let booked_loss: u128 = kani::any();
    let explicit_loss: u128 = kani::any();
    let remaining_after: u128 = kani::any();
    kani::assume(booked_loss < 1u128 << 64);
    kani::assume(explicit_loss < 1u128 << 64);
    // residual_remaining == total - pre_progress <= total < 2^65, so the
    // carried-forward remainder is bounded too (keeps the harness sums in range).
    kani::assume(remaining_after < 1u128 << 65);
    kani::assume(booked_loss + explicit_loss + remaining_after == ledger.residual_remaining);

    // book_bankruptcy_residual_chunk_for_account_core advances by (0,0,0, booked, explicit).
    if let Ok(l) = V16Core::kernel_advance_close_ledger(ledger, 0, 0, 0, booked_loss, explicit_loss)
    {
        // CROSS-LAYER AGREEMENT: the ledger's carried-forward residual equals the
        // booking step's remaining_after — booking and ledger never drift apart.
        assert_eq!(l.residual_remaining, remaining_after);
        // and the ledger's residual dropped by exactly what was booked + lost.
        assert_eq!(
            l.residual_remaining + booked_loss + explicit_loss,
            ledger.residual_remaining
        );
    }
}

// ROADMAP workstream B.2 (resolved-bankruptcy PnL settlement): the negative PnL
// is reduced by exactly the loss the residual booking absorbed (cleared =
// min(booked+explicit, |pnl|)), only shrinking the loss toward zero and never
// over-clearing into a spurious positive credit.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_settle_resolved_pnl_after_booking)]
#[kani::solver(cadical)]
fn contract_check_kernel_settle_resolved_pnl_after_booking() {
    let pnl: i128 = kani::any();
    let booked_loss: u128 = kani::any();
    let explicit_loss: u128 = kani::any();
    let _ = V16Core::kernel_settle_resolved_pnl_after_booking(pnl, booked_loss, explicit_loss);
}

// ROADMAP workstream B.2 (insurance-draw vault-neutrality, REJECTION complement):
// the insurance->close-spent reclassification in consume_domain_insurance_for_
// negative_pnl has NO external quote flow, so the flow proof must REJECT any real
// vault movement. With the vault-flat witness (contract_check_flow_insurance_to_
// close_insurance_spent) this is the biconditional: validate_insurance_to_close_
// insurance_spent is Ok IFF the vault is unchanged. So the draw provably moves
// `used` from the insurance pool to the close's insurance_spent (scalar amounts
// proven by kernel_consume_insurance_layer) and CANNOT succeed while moving real
// tokens through the vault — no-LoF at the flow seam.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
fn contract_check_flow_insurance_to_close_rejects_vault_movement() {
    let amount: u128 = kani::any();
    let vault_before: u128 = kani::any();
    let vault_after: u128 = kani::any();
    kani::assume(vault_before != vault_after);
    assert!(
        TokenValueFlowProofV16::validate_insurance_to_close_insurance_spent(
            amount,
            vault_before,
            vault_after
        )
        .is_err()
    );
}

// ROADMAP Phase 2 (NB1 admission): an economically-valid trade is admitted, and
// every rejection maps to a false economic precondition — admit IFF economically
// valid. No economically-valid user trade is internally DoSed at the guard stack.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_economically_valid_trade_admits)]
#[kani::solver(cadical)]
fn contract_check_kernel_economically_valid_trade_admits() {
    let evt = EconomicallyValidTradeV16 {
        asset_configured: kani::any(),
        size_q: kani::any(),
        price: kani::any(),
        price_lo: kani::any(),
        price_hi: kani::any(),
        fee_bps: kani::any(),
        max_fee_bps: kani::any(),
        accounts_current: kani::any(),
        not_loss_stale_blocked: kani::any(),
        no_adverse_lag: kani::any(),
        no_barrier_touch: kani::any(),
        margin_ok: kani::any(),
        locked_lane_ok: kani::any(),
    };
    let _ = V16Core::kernel_economically_valid_trade_admits(evt);
}

// ENGINE.MD asset self-selection: the bounded first-match leg scan returns an
// in-range, actionable, first-matching slot, and is complete (Some IFF any flag
// set). This is the proof that the engine — not the caller — picks the asset.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::first_actionable_slot)]
#[kani::unwind(17)]
#[kani::solver(cadical)]
fn contract_check_first_actionable_slot() {
    let flags: [bool; V16_MAX_PORTFOLIO_ASSETS_N] = kani::any();
    let _ = V16Core::first_actionable_slot(flags);
}

// ENGINE.MD plan selector: totality (actionable -> non-NoAction), priority
// determinism (recovery > resolved > b-stale > liquidate > refresh), and
// selected-asset fidelity (SettleBChunk/Liquidate carry the engine-selected
// slot). pending_close is classifier-unreachable (required absent).
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::select_auto_crank_plan)]
#[kani::solver(cadical)]
fn contract_check_select_auto_crank_plan() {
    let summary: ActionableSummaryV16 = kani::any();
    let b_stale_slot: usize = kani::any();
    let liq_slot: usize = kani::any();
    let refresh_asset: Option<usize> = if kani::any() {
        Some(kani::any::<usize>())
    } else {
        None
    };
    let recovery_reason: PermissionlessRecoveryReasonV16 = kani::any();
    let _ = V16Core::select_auto_crank_plan(
        summary,
        b_stale_slot,
        liq_slot,
        refresh_asset,
        recovery_reason,
    );
}
