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
    let _ = V16Core::kernel_clear_leg(leg, asset);
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
