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

#[cfg(all(kani, feature = "closure"))]
fn liquidation_plan_only_permissionless_crank_stub<'a: 'a, T>(
    _market: &mut MarketGroupV16ViewMut<'a, T>,
    _account: &mut PortfolioV16ViewMut<'_>,
    request: PermissionlessCrankRequestV16,
) -> V16Result<PermissionlessProgressOutcomeV16> {
    match request.action {
        PermissionlessCrankActionV16::Liquidate(liquidation) => {
            assert_eq!(request.asset_index, liquidation.asset_index);
            Ok(PermissionlessProgressOutcomeV16::AccountCurrent)
        }
        _ => panic!("liquidatable auto-crank selected a non-liquidation action"),
    }
}

// Liquidation DoS/asset-ownership seam over persisted state. A current health
// certificate with a real deficit and one active leg must select liquidation
// for that leg's engine-owned asset, without an oracle observation or mutation
// before dispatch. Real liquidation value/isolation is proven by the dedicated
// preflight, residual-booking, and domain-insurance closure theorems.
#[cfg(all(kani, feature = "closure"))]
fn prove_liquidatable_auto_crank_selects_active_asset<const ASSET: usize>() {
    let (mut header, mut markets, mut account_header, mut leg, _) =
        b_stale_transition_fixture::<ASSET>();
    let mut asset = markets[ASSET].engine.asset.try_to_runtime().unwrap();
    asset.b_long_num = 0;
    asset.b_short_num = 0;
    markets[ASSET].engine.asset = AssetStateV16Account::from_runtime(&asset);
    header.b_stale_account_count = V16PodU64::new(0);
    leg.b_stale = false;
    account_header.legs[0] = PortfolioLegV16Account::from_runtime(&leg);
    account_header.b_stale_state = 0;
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
        leg.side == SideV16::Short,
        "liquidation selection covers the active asset's short side"
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
#[kani::stub(crate::v16::loss_weight_for_basis, one_position_loss_weight_stub)]
#[kani::stub(
    MarketGroupV16ViewMut::permissionless_crank_not_atomic,
    liquidation_plan_only_permissionless_crank_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::close_resolved_account_not_atomic,
    unreachable_resolved_close_stub
)]
fn closure_asset_zero_liquidatable_auto_crank_selects_active_asset() {
    prove_liquidatable_auto_crank_selects_active_asset::<0>();
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
#[kani::stub(crate::v16::loss_weight_for_basis, one_position_loss_weight_stub)]
#[kani::stub(
    MarketGroupV16ViewMut::permissionless_crank_not_atomic,
    liquidation_plan_only_permissionless_crank_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::close_resolved_account_not_atomic,
    unreachable_resolved_close_stub
)]
fn closure_asset_one_liquidatable_auto_crank_selects_active_asset() {
    prove_liquidatable_auto_crank_selects_active_asset::<1>();
}

#[cfg(all(kani, feature = "closure"))]
fn refresh_plan_only_permissionless_crank_stub<'a: 'a, T>(
    _market: &mut MarketGroupV16ViewMut<'a, T>,
    account: &mut PortfolioV16ViewMut<'_>,
    request: PermissionlessCrankRequestV16,
) -> V16Result<PermissionlessProgressOutcomeV16> {
    match request.action {
        PermissionlessCrankActionV16::Refresh => {
            assert_eq!(
                request.asset_index,
                account.header.legs[0].asset_index.get() as usize
            );
            Ok(PermissionlessProgressOutcomeV16::AccountCurrent)
        }
        _ => panic!("stale-certificate auto-crank selected a non-refresh action"),
    }
}

// Stale-certificate liquidation safety. A persisted deficit certified at an
// old oracle epoch must select RefreshAccount, never Liquidate, and must use the
// first active leg's engine-owned asset without requiring a caller observation.
#[cfg(all(kani, feature = "closure"))]
fn prove_stale_deficit_auto_crank_refreshes_active_asset<const ASSET: usize>() {
    let (mut header, mut markets, mut account_header, mut leg, _) =
        b_stale_transition_fixture::<ASSET>();
    let mut asset = markets[ASSET].engine.asset.try_to_runtime().unwrap();
    asset.b_long_num = 0;
    asset.b_short_num = 0;
    markets[ASSET].engine.asset = AssetStateV16Account::from_runtime(&asset);
    header.b_stale_account_count = V16PodU64::new(0);
    header.oracle_epoch = V16PodU64::new(1);
    header.stale_certificate_count = V16PodU64::new(1);
    leg.b_stale = false;
    account_header.legs[0] = PortfolioLegV16Account::from_runtime(&leg);
    account_header.b_stale_state = 0;
    account_header.stale_state = 1;
    let bitmap = account_header.active_bitmap.map(V16PodU64::get);
    account_header.health_cert = HealthCertV16Account::from_runtime(&HealthCertV16 {
        certified_equity: 0,
        certified_initial_req: 2,
        certified_maintenance_req: 1,
        certified_liq_deficit: 1,
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
    kani::cover!(
        leg.side == SideV16::Short,
        "stale-deficit refresh covers the active asset's short side"
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
#[kani::stub(crate::v16::loss_weight_for_basis, one_position_loss_weight_stub)]
#[kani::stub(
    MarketGroupV16ViewMut::permissionless_crank_not_atomic,
    refresh_plan_only_permissionless_crank_stub
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
#[kani::unwind(40)]
#[kani::solver(cadical)]
#[kani::stub(crate::v16::loss_weight_for_basis, one_position_loss_weight_stub)]
#[kani::stub(
    MarketGroupV16ViewMut::permissionless_crank_not_atomic,
    refresh_plan_only_permissionless_crank_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::close_resolved_account_not_atomic,
    unreachable_resolved_close_stub
)]
fn closure_asset_one_stale_deficit_auto_crank_refreshes_active_asset() {
    prove_stale_deficit_auto_crank_refreshes_active_asset::<1>();
}

#[cfg(all(kani, feature = "closure"))]
fn first_winner_close_stub<'a: 'a, T>(
    market: &mut MarketGroupV16ViewMut<'a, T>,
    account: &mut PortfolioV16ViewMut<'_>,
    _fee_rate_per_slot: u128,
) -> V16Result<ResolvedCloseOutcomeV16> {
    assert_eq!(market.header.payout_snapshot_captured, 0);
    assert!(account.header.pnl.get() > 0);
    Ok(ResolvedCloseOutcomeV16::ProgressOnly)
}

#[cfg(all(kani, feature = "closure"))]
fn unreachable_permissionless_crank_stub<'a: 'a, T>(
    _market: &mut MarketGroupV16ViewMut<'a, T>,
    _account: &mut PortfolioV16ViewMut<'_>,
    _request: PermissionlessCrankRequestV16,
) -> V16Result<PermissionlessProgressOutcomeV16> {
    panic!("resolved winner selected a live-market crank action")
}

// First-winner payout liveness. In Resolved mode, a positive-PnL account with
// every blocker clear must select CloseResolved even before the payout snapshot
// exists; the selected close call is the only path that captures that snapshot.
// Requiring pre-capture would deadlock every winner and permanently strand the
// residual vault. The stub asserts this is specifically the first-winner call.
#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
#[kani::stub(
    MarketGroupV16ViewMut::permissionless_crank_not_atomic,
    unreachable_permissionless_crank_stub
)]
#[kani::stub(
    MarketGroupV16ViewMut::close_resolved_account_not_atomic,
    first_winner_close_stub
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
    account_header.pnl = V16PodI128::new(pnl as i128);

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
            outcome: AutoCrankOutcomeV16::ResolvedClose(ResolvedCloseOutcomeV16::ProgressOnly,),
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
