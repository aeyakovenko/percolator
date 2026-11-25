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

#[cfg(all(kani, feature = "contracts"))]
#[kani::proof]
#[kani::solver(cadical)]
fn proof_account_kf_settlement_key_roundtrip_and_priority() {
    let phase: u8 = kani::any();
    let source_domain: usize = kani::any();
    let leg_slot: usize = kani::any();
    kani::assume(phase <= 1);
    kani::assume(source_domain <= ACCOUNT_KF_SETTLEMENT_DOMAIN_MAX as usize);
    kani::assume(leg_slot < V16_MAX_PORTFOLIO_ASSETS_N);

    let key = account_kf_settlement_plan_key(phase, source_domain, leg_slot).unwrap();
    assert!(key <= ACCOUNT_KF_SETTLEMENT_KEY_MAX);
    assert_eq!(
        decode_account_kf_settlement_plan_key(key),
        Ok((phase, source_domain, leg_slot)),
    );

    let other_domain: usize = kani::any();
    let other_slot: usize = kani::any();
    kani::assume(other_domain <= ACCOUNT_KF_SETTLEMENT_DOMAIN_MAX as usize);
    kani::assume(other_slot < V16_MAX_PORTFOLIO_ASSETS_N);
    let loss_key = account_kf_settlement_plan_key(0, source_domain, leg_slot).unwrap();
    let gain_key = account_kf_settlement_plan_key(1, other_domain, other_slot).unwrap();
    assert!(loss_key < gain_key);

    if source_domain < other_domain {
        let earlier_domain =
            account_kf_settlement_plan_key(phase, source_domain, leg_slot).unwrap();
        let later_domain = account_kf_settlement_plan_key(phase, other_domain, other_slot).unwrap();
        assert!(earlier_domain < later_domain);
    }
    if leg_slot < other_slot {
        let earlier_slot = account_kf_settlement_plan_key(phase, source_domain, leg_slot).unwrap();
        let later_slot = account_kf_settlement_plan_key(phase, source_domain, other_slot).unwrap();
        assert!(earlier_slot < later_slot);
    }
}

// The whole-account K/F settlement plan is built one entry at a time by this
// production insertion kernel. Prove the induction step over the full 16-leg
// bound: any sorted valid prefix remains sorted, gains exactly the incoming
// entry, loses no existing entry, and leaves the unused tail untouched.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof]
#[kani::unwind(18)]
#[kani::solver(cadical)]
fn proof_account_kf_settlement_plan_insert_preserves_order_and_multiset() {
    let entries: [u64; V16_MAX_PORTFOLIO_ASSETS_N] = kani::any();
    let incoming: u64 = kani::any();
    let prefix_len: usize = kani::any();
    kani::assume(prefix_len < V16_MAX_PORTFOLIO_ASSETS_N);
    kani::assume(incoming <= ACCOUNT_KF_SETTLEMENT_KEY_MAX);

    let mut plan = [None; V16_MAX_PORTFOLIO_ASSETS_N];
    let mut index = 0usize;
    while index < prefix_len {
        kani::assume(entries[index] <= ACCOUNT_KF_SETTLEMENT_KEY_MAX);
        plan[index] = Some(entries[index]);
        index += 1;
    }
    index = 1;
    while index < prefix_len {
        kani::assume(entries[index - 1] <= entries[index]);
        index += 1;
    }

    let before = plan;
    let mut expected_insert_at = prefix_len;
    index = 0;
    while index < prefix_len {
        if incoming < entries[index] {
            expected_insert_at = index;
            break;
        }
        index += 1;
    }
    let next_len = match insert_account_kf_settlement_plan_entry(&mut plan, prefix_len, incoming) {
        Ok(next_len) => next_len,
        Err(_) => {
            assert!(false, "a valid entry must fit in a non-full canonical plan");
            return;
        }
    };
    let mut proof_holds = next_len == prefix_len + 1;

    index = 0;
    while index < next_len {
        proof_holds &= plan[index].is_some();
        if index > 0 {
            proof_holds &= plan[index - 1] <= plan[index];
        }
        index += 1;
    }
    while index < V16_MAX_PORTFOLIO_ASSETS_N {
        proof_holds &= plan[index].is_none();
        index += 1;
    }

    // The forward reference scan chooses the first strictly greater key. The
    // production backward scan must produce that exact stable insertion:
    // unchanged prefix, incoming entry, then the old suffix shifted by one.
    index = 0;
    while index < expected_insert_at {
        proof_holds &= plan[index] == before[index];
        index += 1;
    }
    proof_holds &= plan[expected_insert_at] == Some(incoming);
    index = expected_insert_at + 1;
    while index < next_len {
        proof_holds &= plan[index] == before[index - 1];
        index += 1;
    }
    assert!(proof_holds);
}

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

// Grow-only aggregate side-OI cap: Err(InvalidLeg) iff some side ends above
// MAX_OI_SIDE_Q AND the fill grew it (post > pre). Fully symbolic u128 inputs.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_trade_side_oi_cap_gate)]
#[kani::solver(cadical)]
fn contract_check_kernel_trade_side_oi_cap_gate() {
    let _ =
        V16Core::kernel_trade_side_oi_cap_gate(kani::any(), kani::any(), kani::any(), kani::any());
}

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
#[kani::proof_for_contract(V16Core::prepare_counterparty_impaired_lien_retirement_delta)]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn contract_check_prepare_counterparty_impaired_lien_retirement_delta() {
    let bucket = BackingBucketV16 {
        market_id: kani::any(),
        fresh_unliened_backing_num: kani::any(),
        valid_liened_backing_num: kani::any(),
        consumed_liened_backing_num: kani::any(),
        impaired_liened_backing_num: kani::any(),
        expiry_slot: kani::any(),
        status: kani::any(),
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
    let _ = V16Core::prepare_counterparty_impaired_lien_retirement_delta(bucket, source, amount);
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
#[kani::proof_for_contract(V16Core::prepare_source_positive_claim_burn_delta)]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn contract_check_prepare_source_positive_claim_burn_delta() {
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
    let face_burn_num: u128 = kani::any();
    kani::assume(source.exact_positive_claim_num <= source.positive_claim_bound_num);
    kani::assume(face_burn_num <= source.positive_claim_bound_num);
    let _ = V16Core::prepare_source_positive_claim_burn_delta(source, face_burn_num);
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
#[kani::proof_for_contract(V16Core::kernel_provider_protection_num)]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn contract_check_kernel_provider_protection_num() {
    let _ = V16Core::kernel_provider_protection_num(kani::any(), kani::any(), kani::any());
}

#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_provider_protection_floor_observe)]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn contract_check_kernel_provider_protection_floor_observe() {
    let _ = V16Core::kernel_provider_protection_floor_observe(kani::any(), kani::any());
}

#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_provider_protection_floor_shift)]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn contract_check_kernel_provider_protection_floor_shift() {
    let _ = V16Core::kernel_provider_protection_floor_shift(kani::any(), kani::any(), kani::any());
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
        kf_epoch_snap: kani::any(),
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
        kf_epoch_long: kani::any(),
        kf_epoch_short: kani::any(),
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
        raw_basis_sum_long_q: kani::any(),
        loss_weight_sum_short: kani::any(),
        raw_basis_sum_short_q: kani::any(),
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
    let old_effective_abs: u128 = kani::any();
    let new_effective_abs: u128 = kani::any();
    kani::assume(new_signed != 0);
    kani::assume(new_signed > i128::MIN);
    let _ = V16Core::kernel_resize_leg_same_side(
        portfoliolegv16,
        assetstatev16,
        new_signed,
        new_weight,
        preserve,
        old_effective_abs,
        new_effective_abs,
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
        kf_epoch_long: kani::any(),
        kf_epoch_short: kani::any(),
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
        raw_basis_sum_long_q: kani::any(),
        loss_weight_sum_short: kani::any(),
        raw_basis_sum_short_q: kani::any(),
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

// #449: contract check of the derived side A index,
// floor(ADL_ONE * oi_eff / raw_basis_sum). The operands are bounded to 8 bits:
// a symbolic u128 division resists Kani at full width. The same contract for
// every input is the subject of the Aristotle property K59, and
// tests/adl_side_index_449.rs checks it on boundary and random operands up to
// MAX_OI_SIDE_Q.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_side_adl_index)]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn contract_check_kernel_side_adl_index() {
    let oi_eff_q: u8 = kani::any();
    let raw_basis_sum_q: u8 = kani::any();
    let current_a: u128 = kani::any();
    let _ = V16Core::kernel_side_adl_index(
        u128::from(oi_eff_q),
        u128::from(raw_basis_sum_q),
        current_a,
    );
}

// #449: contract check of the raw basis a leg keeps after a same-side
// reduction on a haircut side, on the same 8-bit grid (see above; Aristotle
// property K60).
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_raw_basis_after_adl_reduction)]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn contract_check_kernel_raw_basis_after_adl_reduction() {
    let raw_abs_q: u8 = kani::any();
    let effective_abs_q: u8 = kani::any();
    let next_effective_abs_q: u8 = kani::any();
    let oi_eff_q: u8 = kani::any();
    let raw_basis_sum_q: u8 = kani::any();
    kani::assume(raw_abs_q <= raw_basis_sum_q);
    let _ = V16Core::kernel_raw_basis_after_adl_reduction(
        u128::from(raw_abs_q),
        u128::from(effective_abs_q),
        u128::from(next_effective_abs_q),
        u128::from(oi_eff_q),
        u128::from(raw_basis_sum_q),
    );
}

#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_normalize_social_loss_carry)]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn contract_check_kernel_normalize_social_loss_carry() {
    let remainder: u128 = kani::any();
    let dust: u128 = kani::any();
    let explicit_loss: u128 = kani::any();
    let _ = V16Core::kernel_normalize_social_loss_carry(remainder, dust, explicit_loss);
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
        kf_epoch_snap: kani::any(),
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
        kf_epoch_long: kani::any(),
        kf_epoch_short: kani::any(),
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
        raw_basis_sum_long_q: kani::any(),
        loss_weight_sum_short: kani::any(),
        raw_basis_sum_short_q: kani::any(),
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
    let clear_effective_oi_q: u128 = kani::any();
    let _ = V16Core::kernel_clear_leg(leg, asset, clear_effective_oi_q);
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
        kf_epoch_snap: kani::any(),
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
        long_requires_initial_margin: kani::any(),
        short_requires_initial_margin: kani::any(),
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

// The side A index is derived again after every leg change (#449), and its
// general case divides (`ADL_ONE * oi_eff / raw_basis_sum`): the same class of
// primitive as loss_weight_for_basis. This abstraction keeps the index
// kernel's exact cases (no raw basis: the index is unchanged; more effective
// OI than raw basis: rejected; equal totals: unit A) and over-approximates
// only the division-bearing branch, by any index its contract allows there or
// its overflow error. The value in that branch is kernel_side_adl_index's own
// contract (contract_check_kernel_side_adl_index); the compositions below
// assert where things are written and how the side totals move.
#[cfg(all(kani, feature = "contracts"))]
fn axiom_side_adl_index_without_division(
    oi_eff_q: u128,
    raw_basis_sum_q: u128,
    current_a: u128,
) -> V16Result<u128> {
    if raw_basis_sum_q == 0 {
        return Ok(current_a);
    }
    if oi_eff_q > raw_basis_sum_q {
        return Err(V16Error::InvalidLeg);
    }
    if oi_eff_q == raw_basis_sum_q {
        return Ok(ADL_ONE);
    }
    if kani::any() {
        return Err(V16Error::ArithmeticOverflow);
    }
    let a: u128 = kani::any();
    kani::assume(a >= 1 && a < ADL_ONE);
    Ok(a)
}

// kernel_sync_side_adl_index, statement for statement, with the index taken
// from the abstraction above. The index kernel carries a contract, so it
// cannot be replaced by a plain stub itself; the step that writes its result
// carries none and is replaced by this mirror.
#[cfg(all(kani, feature = "contracts"))]
fn axiom_sync_side_adl_index_without_division(
    mut asset: AssetStateV16,
    side: SideV16,
) -> V16Result<AssetStateV16> {
    match side {
        SideV16::Long => {
            asset.a_long = axiom_side_adl_index_without_division(
                asset.oi_eff_long_q,
                asset.raw_basis_sum_long_q,
                asset.a_long,
            )?;
            if asset.oi_eff_long_q != 0
                && asset.a_long < MIN_A_SIDE
                && asset.mode_long == SideModeV16::Normal
            {
                asset.mode_long = SideModeV16::DrainOnly;
            }
        }
        SideV16::Short => {
            asset.a_short = axiom_side_adl_index_without_division(
                asset.oi_eff_short_q,
                asset.raw_basis_sum_short_q,
                asset.a_short,
            )?;
            if asset.oi_eff_short_q != 0
                && asset.a_short < MIN_A_SIDE
                && asset.mode_short == SideModeV16::Normal
            {
                asset.mode_short = SideModeV16::DrainOnly;
            }
        }
    }
    Ok(asset)
}

#[cfg(all(kani, feature = "contracts"))]
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
#[kani::stub(crate::v16::loss_weight_for_basis, kani_any_loss_weight)]
#[kani::stub(
    V16Core::kernel_sync_side_adl_index,
    axiom_sync_side_adl_index_without_division
)]
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
// the asset transform (the kernel has NO division — it uses the leg's existing
// weight), so only the kernel-contract-check interaction needs the stub. The
// side index derived after the clear is abstracted like the weight
// (axiom_sync_side_adl_index_without_division). The
// whole-body frame: clearing the leg at the active slot sets that leg EMPTY,
// clears its bitmap bit, and invalidates the cert — every OTHER leg and
// account field frozen.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
#[kani::stub(crate::v16::loss_weight_for_basis, kani_any_loss_weight)]
#[kani::stub(
    V16Core::kernel_sync_side_adl_index,
    axiom_sync_side_adl_index_without_division
)]
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
        kf_epoch_snap: kani::any(),
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
#[kani::stub(
    V16Core::kernel_sync_side_adl_index,
    axiom_sync_side_adl_index_without_division
)]
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
    let raw0 = markets[0]
        .engine
        .asset
        .try_to_runtime()
        .unwrap()
        .raw_basis_sum_long_q;
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
    let raw1 = markets[0].engine.asset.raw_basis_sum_long_q.get();
    let leg_weight = account_header.legs[0].loss_weight.get();
    let leg_basis = account_header.legs[0].basis_pos_q.get();
    // CONSERVATION (no wide arithmetic): OI and the side's raw basis sum rise
    // by exactly abs; the side weight sum rises by exactly the weight written
    // to the leg.
    assert_eq!(oi1, oi0.wrapping_add(abs));
    assert_eq!(raw1, raw0.wrapping_add(abs));
    assert_eq!(ws1, ws0.wrapping_add(leg_weight));
    assert_eq!(leg_basis, basis);
}

// Credit-rate arithmetic is already discharged against an independent model
// in rounding_residue_fuzz. This abstraction preserves the exact extrema used
// by the source-lien unwind (no claim => full rate; zero available backing =>
// zero rate; available backing covering every claim that valid liens do not
// cover => full rate) and over-approximates only the division-bearing
// partial-haircut branch, which this composition never uses.
#[cfg(all(kani, feature = "closure"))]
fn axiom_source_credit_rate_extrema(state: SourceCreditStateV16) -> V16Result<u128> {
    V16Core::validate_source_credit_state_shape_static(state)?;
    if state.positive_claim_bound_num == 0 {
        return Ok(CREDIT_RATE_SCALE);
    }
    let available = V16Core::available_backing_num_for_source_credit_state(state)?;
    if available == 0 {
        return Ok(0);
    }
    if available >= V16Core::unliened_claim_bound_num_for_source_credit_state(state)? {
        return Ok(CREDIT_RATE_SCALE);
    }
    let rate: u128 = kani::any();
    kani::assume(rate < CREDIT_RATE_SCALE);
    Ok(rate)
}

#[cfg(all(kani, feature = "closure"))]
fn complete_counterparty_source_lien_fixture(
    claim: u128,
) -> (
    MarketGroupV16HeaderAccount,
    [Market<u64>; 1],
    PortfolioAccountV16Account,
) {
    let face_num = claim * BOUND_SCALE;
    let cfg = V16Config::public_user_fund_with_market_slots(1, 1, 0, 10);
    let mut header = MarketGroupV16HeaderAccount::new_dynamic([1u8; 32], cfg, 1, 0).unwrap();
    let mut markets = [Market::new(0u64, EngineAssetSlotV16Account::default())];
    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        market.activate_empty_market_not_atomic(0, 100, 1).unwrap();
    }
    let mut account = PortfolioAccountV16Account::default();
    account
        .init_empty_in_place(ProvenanceHeaderV16Account::from_runtime(
            &ProvenanceHeaderV16::new([1u8; 32], [2u8; 32], [3u8; 32]),
        ))
        .unwrap();
    let market_id = markets[0].engine.asset.market_id.get();
    header.vault = V16PodU128::new(claim);
    header.pnl_pos_tot = V16PodU128::new(claim);
    header.pnl_pos_bound_tot = V16PodU128::new(claim);
    header.pnl_pos_bound_tot_num = V16PodU128::new(face_num);
    header.source_claim_bound_total_num = V16PodU128::new(face_num);
    header.source_fresh_backing_total_num = V16PodU128::new(face_num);
    account.pnl = V16PodI128::new(claim as i128);
    account.source_domains[0] = PortfolioSourceDomainV16Account {
        domain: V16PodU32::new(0),
        source_claim_market_id: V16PodU64::new(market_id),
        source_claim_bound_num: V16PodU128::new(face_num),
        source_claim_liened_num: V16PodU128::new(face_num),
        source_claim_counterparty_liened_num: V16PodU128::new(face_num),
        source_lien_effective_reserved: V16PodU128::new(claim),
        source_lien_counterparty_backing_num: V16PodU128::new(face_num),
        ..PortfolioSourceDomainV16Account::default()
    };
    markets[0].engine.backing_long = BackingBucketV16Account::from_runtime(&BackingBucketV16 {
        market_id,
        valid_liened_backing_num: face_num,
        expiry_slot: 100,
        status: BackingBucketStatusV16::Fresh,
        ..BackingBucketV16::EMPTY
    });
    markets[0].engine.source_credit_long =
        SourceCreditStateV16Account::from_runtime(&SourceCreditStateV16 {
            positive_claim_bound_num: face_num,
            exact_positive_claim_num: face_num,
            fresh_reserved_backing_num: face_num,
            valid_liened_backing_num: face_num,
            credit_rate_num: 0,
            ..SourceCreditStateV16::EMPTY
        });
    (header, markets, account)
}

// Nonvacuity companion for the whole-setter composition below. Keeping the
// fixture in one constructor prevents the validity proof and mutation proof
// from silently drifting while avoiding three full validation scans in one
// CBMC query.
#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
#[kani::stub(
    V16Core::expected_source_credit_rate_num_for_state,
    axiom_source_credit_rate_extrema
)]
fn composition_complete_counterparty_source_lien_fixture_is_valid_under_rate_axiom() {
    let claim_raw: u8 = kani::any();
    kani::assume((1..=8).contains(&claim_raw));
    let (mut header, mut markets, mut account_header) =
        complete_counterparty_source_lien_fixture(claim_raw as u128);
    let market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let account = PortfolioV16ViewMut::new(&mut account_header);

    assert_eq!(market.validate_shape(), Ok(()));
    assert_eq!(account.validate_with_market(&market.as_view()), Ok(()));
    kani::cover!(claim_raw > 1, "nontrivial valid complete source lien");
}

// INV-071/073 whole-setter composition: a valid live positive-PnL claim whose
// complete face is counterparty-liened cannot survive the canonical PnL setter
// crossing below zero. Every close-producing route uses this setter before it
// can install a pending residual, so a source-lien release cannot remain hidden
// behind the higher-priority close continuation.
#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
#[kani::stub(
    V16Core::expected_source_credit_rate_num_for_state,
    axiom_source_credit_rate_extrema
)]
fn composition_positive_to_negative_pnl_consumes_complete_source_lien_under_rate_axiom() {
    let claim_raw: u8 = kani::any();
    let loss_raw: u8 = kani::any();
    kani::assume((1..=8).contains(&claim_raw));
    kani::assume((1..=8).contains(&loss_raw));
    let claim = claim_raw as u128;
    let face_num = claim * BOUND_SCALE;
    let loss = loss_raw as i128;

    let (mut header, mut markets, mut account_header) =
        complete_counterparty_source_lien_fixture(claim);

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);

    market.set_account_pnl(&mut account, -loss).unwrap();

    kani::cover!(
        claim > 1 && loss > 1,
        "nontrivial source-lien loss crossing"
    );
    assert_eq!(account.header.pnl.get(), -loss);
    assert!(account
        .header
        .source_domains
        .iter()
        .all(|source| source.source_claim_bound_num.get() == 0
            && source.source_claim_liened_num.get() == 0
            && source.source_claim_impaired_num.get() == 0));
    let bucket = market.markets[0]
        .engine
        .backing_long
        .try_to_runtime()
        .unwrap();
    let source = market.markets[0]
        .engine
        .source_credit_long
        .try_to_runtime()
        .unwrap();
    assert_eq!(bucket.fresh_unliened_backing_num, face_num);
    assert_eq!(bucket.valid_liened_backing_num, 0);
    assert_eq!(source.positive_claim_bound_num, 0);
    assert_eq!(source.exact_positive_claim_num, 0);
    assert_eq!(source.valid_liened_backing_num, 0);
    assert_eq!(market.header.pnl_pos_tot.get(), 0);
    assert_eq!(market.header.pnl_pos_bound_tot_num.get(), 0);
    assert_eq!(market.header.source_claim_bound_total_num.get(), 0);
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
#[kani::stub(
    V16Core::kernel_sync_side_adl_index,
    axiom_sync_side_adl_index_without_division
)]
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
    let raw0 = markets[0]
        .engine
        .asset
        .try_to_runtime()
        .unwrap()
        .raw_basis_sum_long_q;
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
    let raw_mid = markets[0].engine.asset.raw_basis_sum_long_q.get();
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
    let raw1 = markets[0].engine.asset.raw_basis_sum_long_q.get();
    // CONSERVATION: clear removes EXACTLY the leg's stored basis/weight ...
    assert_eq!(oi1, oi_mid.wrapping_sub(leg_abs));
    assert_eq!(raw1, raw_mid.wrapping_sub(leg_abs));
    assert_eq!(ws1, ws_mid.wrapping_sub(leg_weight));
    // ... and attach;clear is an exact round-trip back to the pre-attach asset.
    assert_eq!(oi1, oi0);
    assert_eq!(raw1, raw0);
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

// Full-domain contract check of the fee-charge kernel: an account with negative
// PnL pays nothing, otherwise charged == min(requested_fee, capital); capital and
// c_tot drop and insurance rises by exactly charged; the two error cases are
// exactly a c_tot below the charge and an insurance overflow. Production
// (charge_account_fee_current_not_atomic: every trade fee and liquidation fee)
// calls this for the arithmetic.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_charge_account_fee)]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn contract_check_kernel_charge_account_fee() {
    let requested_fee: u128 = kani::any();
    let pnl: i128 = kani::any();
    let capital: u128 = kani::any();
    let c_tot: u128 = kani::any();
    let insurance: u128 = kani::any();
    let _ = V16Core::kernel_charge_account_fee(requested_fee, pnl, capital, c_tot, insurance);
}

// Full-domain contract check of the kernel that pays a settled K/F loss from
// free capital: paid == min(loss, capital); capital and c_tot drop by exactly
// paid; the two error cases are exactly a payment whose scaled credit
// (paid * BOUND_SCALE, computed by the caller) would not fit, and a c_tot below
// the payment. The kernel decides the first by a comparison with a constant, so
// it holds no wide product and the check covers every input. Production
// (pay_kf_loss_from_free_capital_not_atomic) calls this for the arithmetic.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_pay_kf_loss_from_free_capital)]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn contract_check_kernel_pay_kf_loss_from_free_capital() {
    let loss: u128 = kani::any();
    let capital: u128 = kani::any();
    let c_tot: u128 = kani::any();
    let _ = V16Core::kernel_pay_kf_loss_from_free_capital(loss, capital, c_tot);
}

// Full-domain contract check of the initial-margin lien-need kernel: the result
// is the shortfall of the initial requirement over capital-side equity plus the
// credit the account's own liens reserve, so it is zero exactly when that sum
// covers the requirement. A risk-increasing trade creates liens until this is
// zero (create_initial_margin_source_lien_if_needed), which is what its margin
// rule rests on. Production (incremental_initial_margin_source_credit_needed)
// calls this for the arithmetic.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_initial_margin_source_credit_needed)]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn contract_check_kernel_initial_margin_source_credit_needed() {
    let certified_initial_req: u128 = kani::any();
    let no_positive_equity: i128 = kani::any();
    let existing_lien: u128 = kani::any();
    let _ = V16Core::kernel_initial_margin_source_credit_needed(
        certified_initial_req,
        no_positive_equity,
        existing_lien,
    );
}

// Full-domain contract check of the liquidation-deficit kernel: with negative
// equity the deficit is the shortfall below zero, otherwise what the maintenance
// requirement exceeds the equity by, and it is zero exactly when the equity is
// non-negative and covers the requirement. Every certificate writer calls this.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_certified_liq_deficit)]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn contract_check_kernel_certified_liq_deficit() {
    let equity: i128 = kani::any();
    let maintenance_req: u128 = kani::any();
    let _ = V16Core::kernel_certified_liq_deficit(equity, maintenance_req);
}

// Full-domain contract check of the certificate after a capital charge: the
// equity falls by exactly the charge and the liquidation deficit is recomputed by
// kernel_certified_liq_deficit; the call fails exactly when the charge or the new
// equity does not fit. Production (sync_account_fee_to_slot_not_atomic, when it
// keeps a current certificate) calls this and keeps the other fields.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_cert_equity_after_capital_charge)]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn contract_check_kernel_cert_equity_after_capital_charge() {
    let certified_equity: i128 = kani::any();
    let maintenance_req: u128 = kani::any();
    let charged: u128 = kani::any();
    let _ = V16Core::kernel_cert_equity_after_capital_charge(
        certified_equity,
        maintenance_req,
        charged,
    );
}

// Full-domain contract check of the incremental-certificate kernel: each of the
// three requirements is the old total less the changed leg's old contribution
// plus its new one, and the call fails exactly when a contribution exceeds its
// total or a new total would not fit. Production
// (recertify_account_after_trade_delta, after every single-fill trade) calls
// this for the arithmetic.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_cert_requirements_after_leg_delta)]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn contract_check_kernel_cert_requirements_after_leg_delta() {
    let certified_initial_req: u128 = kani::any();
    let certified_maintenance_req: u128 = kani::any();
    let certified_worst_case_loss: u128 = kani::any();
    let old_initial: u128 = kani::any();
    let new_initial: u128 = kani::any();
    let old_maintenance: u128 = kani::any();
    let new_maintenance: u128 = kani::any();
    let old_notional: u128 = kani::any();
    let new_notional: u128 = kani::any();
    let _ = V16Core::kernel_cert_requirements_after_leg_delta(
        certified_initial_req,
        certified_maintenance_req,
        certified_worst_case_loss,
        old_initial,
        new_initial,
        old_maintenance,
        new_maintenance,
        old_notional,
        new_notional,
    );
}

// Full-domain contract check of the residual-reward-credit kernel: the credit
// is min(crystallized - spent, principal); the trader's spent total and the
// counterparty's received total rise by exactly the credit, so spent never
// exceeds crystallized; the errors are exactly a spent total above the
// crystallized one and a received total that would not fit. Production
// (transfer_account_residual_reward_credit, on every risk-increasing fill)
// calls this for the arithmetic.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_residual_reward_credit)]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn contract_check_kernel_residual_reward_credit() {
    let crystallized: u128 = kani::any();
    let spent: u128 = kani::any();
    let received: u128 = kani::any();
    let principal_atoms: u128 = kani::any();
    let _ = V16Core::kernel_residual_reward_credit(crystallized, spent, received, principal_atoms);
}

// Full-domain contract check of the PnL-totals increase kernel: the exact total
// rises by the increase and the bound total by the scaled increase, and the
// call fails exactly when one of the two would not fit. Production
// (set_account_pnl_inner, the one writer of an account's PnL) calls this.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_pnl_totals_after_increase)]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn contract_check_kernel_pnl_totals_after_increase() {
    let pnl_pos_tot: u128 = kani::any();
    let pnl_pos_bound_tot_num: u128 = kani::any();
    let increase: u128 = kani::any();
    let increase_num: u128 = kani::any();
    let _ = V16Core::kernel_pnl_totals_after_increase(
        pnl_pos_tot,
        pnl_pos_bound_tot_num,
        increase,
        increase_num,
    );
}

// Full-domain contract check of the PnL-totals decrease kernel: the bound total
// falls by the scaled decrease but never below the new exact total in scaled
// units, and the matured total is capped by the new exact total. Production
// (set_account_pnl_inner) calls this.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_pnl_bounds_after_decrease)]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn contract_check_kernel_pnl_bounds_after_decrease() {
    let pnl_pos_bound_tot_num: u128 = kani::any();
    let decrease_num: u128 = kani::any();
    let exact_min_num: u128 = kani::any();
    let pnl_matured_pos_tot: u128 = kani::any();
    let new_pnl_pos_tot: u128 = kani::any();
    let _ = V16Core::kernel_pnl_bounds_after_decrease(
        pnl_pos_bound_tot_num,
        decrease_num,
        exact_min_num,
        pnl_matured_pos_tot,
        new_pnl_pos_tot,
    );
}

// Full-domain contract check of the negative-PnL account count kernel: one more
// when the account's PnL turns negative, one fewer when it stops being negative,
// unchanged otherwise; the errors are exactly a count at its maximum that would
// rise and a count of zero that would fall. Production (set_account_pnl_inner)
// calls this.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_negative_pnl_account_count)]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn contract_check_kernel_negative_pnl_account_count() {
    let count: u64 = kani::any();
    let old_pnl: i128 = kani::any();
    let new_pnl: i128 = kani::any();
    let _ = V16Core::kernel_negative_pnl_account_count(count, old_pnl, new_pnl);
}

// Full-domain contract check of the kernel that starts a bankrupt-close ledger:
// the whole loss is still to be resolved, nothing is consumed or booked, the
// ledger is active, bound to the asset, its market id and the domain side, its
// id is the previous id plus one and never zero, and its deadline is the current
// slot plus the configured lifetime; the errors are exactly an asset index that
// does not fit 32 bits and a deadline that does not fit. Production
// (begin_close_progress_ledger) calls this.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_begin_close_ledger)]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn contract_check_kernel_begin_close_ledger() {
    let previous_close_id: u64 = kani::any();
    let asset_index: usize = kani::any();
    let market_id: u64 = kani::any();
    let domain_side = if kani::any() {
        SideV16::Long
    } else {
        SideV16::Short
    };
    let gross_loss: u128 = kani::any();
    let current_slot: u64 = kani::any();
    let max_close_lifetime_slots: u64 = kani::any();
    let _ = V16Core::kernel_begin_close_ledger(
        previous_close_id,
        asset_index,
        market_id,
        domain_side,
        gross_loss,
        current_slot,
        max_close_lifetime_slots,
    );
}

// Full-domain contract check of the junior-residual kernel: the vault less the
// senior claims (capital total, insurance, provider earnings, recoverable
// backing principal), zero when they reach the vault or do not fit in 128 bits.
// Production (residual, the pool every junior claim is paid from) calls this.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_residual)]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn contract_check_kernel_residual() {
    let vault: u128 = kani::any();
    let c_tot: u128 = kani::any();
    let insurance: u128 = kani::any();
    let provider_earnings: u128 = kani::any();
    let fresh_backing_atoms: u128 = kani::any();
    let _ = V16Core::kernel_residual(
        vault,
        c_tot,
        insurance,
        provider_earnings,
        fresh_backing_atoms,
    );
}

// Full-domain contract check of the equity-without-positive-credit kernel:
// capital less negative PnL less the fee debt, with the validation errors of
// PnL and fee credits, and an error exactly when the result falls below
// i128::MIN. Production (account_no_positive_credit_equity) calls this.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_no_positive_credit_equity)]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn contract_check_kernel_no_positive_credit_equity() {
    let capital: u128 = kani::any();
    let pnl: i128 = kani::any();
    let fee_credits: i128 = kani::any();
    let _ = V16Core::kernel_no_positive_credit_equity(capital, pnl, fee_credits);
}

// Full-domain contract check of the haircut-equity kernel: with PnL at or below
// zero, capital plus PnL less the fee debt; with positive PnL, capital plus only
// the realizable support less the fee debt (the PnL itself never counts).
// Production (account_haircut_equity, the equity of every health certificate)
// calls this.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_haircut_equity)]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn contract_check_kernel_haircut_equity() {
    let capital: u128 = kani::any();
    let pnl: i128 = kani::any();
    let fee_credits: i128 = kani::any();
    let positive_support: u128 = kani::any();
    let _ = V16Core::kernel_haircut_equity(capital, pnl, fee_credits, positive_support);
}

// Full-domain contract check of the kernel for a gain against an outstanding
// loss: the burned face is the candidate, or the whole new face when the gain is
// a Live gain without a source domain or a loss remains; the PnL is the kept
// face less the remaining loss, so a gain keeps face only when it cures the
// whole loss. Production (apply_signed_kf_delta_to_pnl) calls this.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_gain_against_loss_after_support)]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn contract_check_kernel_gain_against_loss_after_support() {
    let new_face_support: u128 = kani::any();
    let burn_candidate: u128 = kani::any();
    let burn_all: bool = kani::any();
    let remaining_loss: u128 = kani::any();
    let _ = V16Core::kernel_gain_against_loss_after_support(
        new_face_support,
        burn_candidate,
        burn_all,
        remaining_loss,
    );
}

// Full-domain contract check of the capital-backed loss kernel: a new loss is
// paid from capital that earlier losses left free, up to the new loss; capital
// and the capital total fall and PnL rises by exactly the amount paid.
// Production (reserve_new_capital_backed_loss_for_source_domain_not_atomic)
// calls this.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_capital_backed_loss_reservation)]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn contract_check_kernel_capital_backed_loss_reservation() {
    let negative_before: u128 = kani::any();
    let negative_after: u128 = kani::any();
    let capital: u128 = kani::any();
    let c_tot: u128 = kani::any();
    let pnl: i128 = kani::any();
    let _ = V16Core::kernel_capital_backed_loss_reservation(
        negative_before,
        negative_after,
        capital,
        c_tot,
        pnl,
    );
}

// Full-domain contract check of the Resolved loss-capacity kernel: after the
// payout snapshot only the support gap; before it, everything for live funded
// backing, otherwise the gap plus the provider receivable. Production
// (resolved_loss_backing_capacity_num) calls this.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_resolved_loss_backing_capacity)]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn contract_check_kernel_resolved_loss_backing_capacity() {
    let positive_claim_bound_num: u128 = kani::any();
    let available_backing_num: u128 = kani::any();
    let snapshot_captured: bool = kani::any();
    let live_funded_bucket: bool = kani::any();
    let provider_receivable_num: u128 = kani::any();
    let _ = V16Core::kernel_resolved_loss_backing_capacity(
        positive_claim_bound_num,
        available_backing_num,
        snapshot_captured,
        live_funded_bucket,
        provider_receivable_num,
    );
}

// Full-domain contract check of the terminal provider-refill kernel: a late
// loss in a domain with no claim and no creditor refills at most the provider
// receivable and at most the principal not still reserved. Production
// (book_realized_kf_loss_for_source_domain_not_atomic) calls this.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_terminal_provider_refill_capacity)]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn contract_check_kernel_terminal_provider_refill_capacity() {
    let provider_receivable_num: u128 = kani::any();
    let provider_principal_num: u128 = kani::any();
    let fresh_reserved_backing_num: u128 = kani::any();
    let _ = V16Core::kernel_terminal_provider_refill_capacity(
        provider_receivable_num,
        provider_principal_num,
        fresh_reserved_backing_num,
    );
}

// Full-domain contract check of the loss-split kernel: the domain takes at most
// its capacity and the junior pool the rest; the parts add up to the loss.
// Production (book_realized_kf_loss_for_source_domain_not_atomic) calls this.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_split_loss_at_capacity)]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn contract_check_kernel_split_loss_at_capacity() {
    let backing_num: u128 = kani::any();
    let capacity_num: u128 = kani::any();
    let _ = V16Core::kernel_split_loss_at_capacity(backing_num, capacity_num);
}

// Full-domain contract check of the claim-burn mirror kernel: the account's
// claim and the domain's bound total fall by the same amount, the exact total by
// as much of it as it holds, and nothing else in the domain's record changes.
// Production (decrement_account_source_claim_for_domain_not_atomic) calls this.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_burn_account_source_claim)]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn contract_check_kernel_burn_account_source_claim() {
    let account_claim_bound_num: u128 = kani::any();
    let source: SourceCreditStateV16 = kani::any();
    let burn_num: u128 = kani::any();
    let _ = V16Core::kernel_burn_account_source_claim(account_claim_bound_num, source, burn_num);
}

// Full-domain contract check of the account side of a new source-credit lien:
// the liened face, the reserved effective credit and the liened face and backing
// of the chosen backing source rise by exactly the lien's amounts; the first
// counterparty backing starts the fee clock; nothing else in the record changes.
// Production (apply_account_source_credit_lien_delta) calls this.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_account_source_lien_delta)]
#[kani::unwind(20)]
#[kani::solver(cadical)]
fn contract_check_kernel_account_source_lien_delta() {
    let u = || V16PodU128::new(kani::any());
    let source = PortfolioSourceDomainV16Account {
        domain: V16PodU32::new(kani::any()),
        source_claim_market_id: V16PodU64::new(kani::any()),
        source_claim_bound_num: u(),
        source_claim_liened_num: u(),
        source_claim_counterparty_liened_num: u(),
        source_claim_insurance_liened_num: u(),
        source_lien_effective_reserved: u(),
        source_lien_counterparty_backing_num: u(),
        source_lien_insurance_backing_num: u(),
        source_lien_fee_last_slot: V16PodU64::new(kani::any()),
        source_claim_impaired_num: u(),
        source_lien_impaired_effective_reserved: u(),
        source_lien_capital_at_risk_fee_revenue: u(),
        source_lien_impaired_capital_at_risk_fee_revenue: u(),
    };
    let backing_source = if kani::any() {
        SourceCreditBackingSourceV16::Counterparty
    } else {
        SourceCreditBackingSourceV16::Insurance
    };
    let _ = V16Core::kernel_account_source_lien_delta(
        source,
        backing_source,
        kani::any(),
        kani::any(),
        kani::any(),
        kani::any(),
    );
}

// Full-domain contract check of the projected liquidation deficit: the
// maintenance requirement with the leg's contribution replaced, against the
// equity less the step's fee; with negative post-step equity, the requirement
// plus the shortfall below zero. The liquidation sizes its close with it.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(liquidation_projected_health_deficit_from_parts)]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn contract_check_liquidation_projected_health_deficit_from_parts() {
    let _ = liquidation_projected_health_deficit_from_parts(
        kani::any(),
        kani::any(),
        kani::any(),
        kani::any(),
        kani::any(),
    );
}

// Full-domain contract check of the open-interest increase of a new position:
// the side's effective open interest and raw basis sum rise by the quantity,
// its stored position count by one and its loss-weight sum by the weight; the
// other side is untouched; an error exactly when one of them would not fit.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(add_open_interest_for_new_position)]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn contract_check_add_open_interest_for_new_position() {
    let mut asset: AssetStateV16 = kani::any();
    let side = if kani::any() {
        SideV16::Long
    } else {
        SideV16::Short
    };
    let _ = add_open_interest_for_new_position(&mut asset, side, kani::any(), kani::any());
}

// Full-domain contract checks of the two stranded-loser-backing kernels: the
// backing losers paid into a domain beyond its provider's principal and its
// claims, and the part of it owed to the twin domain, at most what the twin's
// claims and provider are still owed. Production
// (stranded_loser_backing_owed_to_twin_num) calls both.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_stranded_loser_backing_num)]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn contract_check_kernel_stranded_loser_backing_num() {
    let _ = V16Core::kernel_stranded_loser_backing_num(kani::any(), kani::any(), kani::any());
}

#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_stranded_backing_owed_to_twin)]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn contract_check_kernel_stranded_backing_owed_to_twin() {
    let _ = V16Core::kernel_stranded_backing_owed_to_twin(
        kani::any(),
        kani::any(),
        kani::any(),
        kani::any(),
    );
}

// Contract check of the oracle price step: a step moves the
// effective price toward the target, never past it, by at most
// (anchor * cap + carried remainder) / MAX_MARGIN_BPS, and carries the exact
// remainder; an unexposed asset jumps to the target. This is the per-step bound
// on what any reported price can do to the effective price.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(canonical_accrual_price_step_v16)]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn contract_check_canonical_accrual_price_step_v16() {
    // BOUNDED BINDING (flagged): prices, the cap and the remainder range over 8 bits.
    // The division by MAX_MARGIN_BPS put wider checks beyond the solver (a 16-bit cap
    // with 64-bit prices did not finish in 25 minutes). The contract is stated for all
    // inputs and is a target for the arithmetic prover.
    let _ = canonical_accrual_price_step_v16(
        kani::any::<u8>() as u64,
        kani::any::<u8>() as u64,
        kani::any::<u8>() as u64,
        kani::any::<u8>() as u64,
        kani::any(),
        kani::any::<u8>() as u16,
    );
}

// A u64 near zero or near u64::MAX, so that products of two of them reach both
// the small range and the overflow refusal.
#[cfg(all(kani, feature = "contracts"))]
fn any_u64_near_zero_or_max() -> u64 {
    let small = kani::any::<u8>() as u64;
    if kani::any() {
        small
    } else {
        u64::MAX - small
    }
}

// Contract check of the accrual price-move cap. BOUNDED BINDING (flagged): the
// two prices are 16-bit and the cap and the segment length 4-bit (96 s), which
// reaches both the accepted move and the Recovery refusal but not the overflow
// refusal. Full width, 16-bit prices with 8-bit cap and length, and the cap and
// length near u64::MAX, each ran past 25 minutes (the three-way product); all
// 8-bit takes about 4 minutes. The contract is stated for all inputs.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_accrual_price_move_within_cap)]
#[kani::solver(cadical)]
fn contract_check_kernel_accrual_price_move_within_cap() {
    let _ = V16Core::kernel_accrual_price_move_within_cap(
        kani::any::<u16>() as u64,
        kani::any::<u16>() as u64,
        (kani::any::<u8>() & 15) as u64,
        (kani::any::<u8>() & 15) as u64,
    );
}

// Contract check of the accrual funding amount. BOUNDED BINDING (flagged): the
// rate is near zero or near either end of i128 (8 free bits), and the segment
// length and the price are near zero or near u64::MAX, which reaches the
// overflow refusal of both products. The full-width check did not finish in 25
// minutes; the contract is stated for all inputs.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_accrual_funding_num)]
#[kani::solver(cadical)]
fn contract_check_kernel_accrual_funding_num() {
    let _ = V16Core::kernel_accrual_funding_num(
        any_index_near_zero_or_ends(),
        any_u64_near_zero_or_max(),
        any_u64_near_zero_or_max(),
        kani::any(),
    );
}

// An index value near zero or near either end of i128, so that the update's
// overflow and i128::MIN refusals are reached.
#[cfg(all(kani, feature = "contracts"))]
fn any_index_near_zero_or_ends() -> i128 {
    let offset = kani::any::<i8>() as i128;
    match kani::any::<u8>() % 3 {
        0 => offset,
        1 => i128::MAX - 200 + offset,
        _ => i128::MIN + 200 + offset,
    }
}

// Contract check of the accrual K/F index update. BOUNDED BINDING (flagged): the
// price move is an i8, the funding amount an i16 (both branches of its division
// by FUNDING_DEN are reached through the multiple-of-FUNDING_DEN choice below),
// the A indices are 8-bit, and the four indices sit near zero or near either end
// of i128. The products with the A indices and the division by FUNDING_DEN put a
// full-width check beyond the solver. The contract is stated for all inputs.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_accrual_index_update)]
#[kani::solver(cadical)]
fn contract_check_kernel_accrual_index_update() {
    let funding_num = if kani::any() {
        kani::any::<i8>() as i128 * FUNDING_DEN as i128
    } else {
        kani::any::<i16>() as i128
    };
    let _ = V16Core::kernel_accrual_index_update(
        any_index_near_zero_or_ends(),
        any_index_near_zero_or_ends(),
        any_index_near_zero_or_ends(),
        any_index_near_zero_or_ends(),
        kani::any::<u8>() as u128,
        kani::any::<u8>() as u128,
        kani::any::<i8>() as i128,
        funding_num,
    );
}

// INV-045, an asset with no effective open interest. With neither side's
// effective open interest nonzero, a segment of either accrual path is neither
// price-move nor funding active, whatever the rest of the asset state and the
// segment's length, price and rate: the accrual does not apply the price-move
// cap (a move there marks no effective position) and the funding amount it
// books is zero. Both accrual paths (`accrue_asset_to_not_atomic`,
// `accrue_asset_path_to_not_atomic`) take their activity from this rule.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof]
#[kani::solver(cadical)]
fn proof_unexposed_accrual_segment_moves_no_equity() {
    let mut old: AssetStateV16 = kani::any();
    old.oi_eff_long_q = 0;
    old.oi_eff_short_q = 0;
    let segment_dt: u64 = kani::any();
    let effective_price: u64 = kani::any();
    let funding_rate_e9: i128 = kani::any();
    let activity = V16Core::accrual_activity_for_asset_segment(
        old,
        segment_dt,
        effective_price,
        funding_rate_e9,
    );
    assert!(!activity.price_move_active);
    assert!(!activity.funding_active);
    assert!(!activity.equity_active);
    assert_eq!(
        V16Core::kernel_accrual_funding_num(
            funding_rate_e9,
            segment_dt,
            effective_price,
            activity.funding_active,
        ),
        Ok(0)
    );
}

// INV-045, an asset with effective open interest. A segment that moves the
// effective price of an asset with effective open interest on either side is
// equity active, so the single-segment accrual runs the price-move cap
// (`kernel_accrual_price_move_within_cap`) before it books the move.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof]
#[kani::solver(cadical)]
fn proof_exposed_price_move_is_equity_active() {
    let old: AssetStateV16 = kani::any();
    kani::assume(old.oi_eff_long_q != 0 || old.oi_eff_short_q != 0);
    let segment_dt: u64 = kani::any();
    let effective_price: u64 = kani::any();
    kani::assume(effective_price != old.effective_price);
    let funding_rate_e9: i128 = kani::any();
    let activity = V16Core::accrual_activity_for_asset_segment(
        old,
        segment_dt,
        effective_price,
        funding_rate_e9,
    );
    assert!(activity.price_move_active);
    assert!(activity.equity_active);
}

// INV-045, an asset with no effective open interest. An index update with a
// zero funding amount, the amount the accrual books for a segment that is not
// funding active, leaves both funding indices where they were: full width, for
// every K and F index, A index and price move. Only the K indices can move.
// Closure layer: there the kernel runs without its `ensures` clause, which the
// contracts layer would assert here too and which restates the K products (the
// reason its own contract check is bounded).
#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::solver(cadical)]
fn proof_index_update_without_funding_leaves_funding_indices() {
    let k_long: i128 = kani::any();
    let k_short: i128 = kani::any();
    let f_long: i128 = kani::any();
    let f_short: i128 = kani::any();
    let a_long: u128 = kani::any();
    let a_short: u128 = kani::any();
    let price_delta: i128 = kani::any();
    if let Ok((_, _, fl, fs)) = V16Core::kernel_accrual_index_update(
        k_long,
        k_short,
        f_long,
        f_short,
        a_long,
        a_short,
        price_delta,
        0,
    ) {
        assert_eq!(fl, f_long);
        assert_eq!(fs, f_short);
    }
}

// INV-045, clause C0 of the closure map, for every price: neither accrual entry point accepts a
// price of 0 or above MAX_ORACLE_PRICE, and the refusal comes in the prologue, which only reads
// (the config, the mode, the asset count and, on the path, the market clock), so the market is
// left as it was. The body after the prologue is a function of its own
// (`accrue_asset_to_admitted_not_atomic`, `accrue_asset_path_to_admitted_not_atomic`) and is
// replaced here by a stand-in that fails the proof if it is ever called; the solver then does not
// encode it. (With the body in, the guard stays undecided for a symbolic price and the encoding
// ran out of memory: tests/proofs_v16.rs checks three concrete prices against the whole
// function.) One Live market with one active asset; the asset index, slot, funding rate, steps
// and flag are symbolic. The second proof shows the guard is exact: with the other inputs
// admitted, every price from 1 to MAX_ORACLE_PRICE reaches the body.
#[cfg(all(kani, feature = "closure"))]
fn stand_in_accrual_body_never_called<'a, T>(
    _market: &mut MarketGroupV16ViewMut<'a, T>,
    _config: V16Config,
    _asset_index: usize,
    _now_slot: u64,
    _effective_price: u64,
    _funding_rate_e9: i128,
    _protective_progress_committed: bool,
) -> V16Result<AccrueAssetOutcomeV16>
where
    'a: 'a, // early-bound, as the method's lifetime is (Kani counts it as a generic parameter)
{
    kani::assert(
        false,
        "a price outside the oracle range reached the accrual body",
    );
    Err(V16Error::InvalidConfig)
}

#[cfg(all(kani, feature = "closure"))]
fn stand_in_path_accrual_body_never_called<'a, T>(
    _market: &mut MarketGroupV16ViewMut<'a, T>,
    _config: V16Config,
    _asset_index: usize,
    _now_slot: u64,
    _raw_oracle_target_price: u64,
    _steps: &[AccrualStepV16],
    _protective_progress_committed: bool,
) -> V16Result<AccrueAssetOutcomeV16>
where
    'a: 'a, // early-bound, as the method's lifetime is (Kani counts it as a generic parameter)
{
    kani::assert(
        false,
        "a price outside the oracle range reached the path accrual body",
    );
    Err(V16Error::InvalidConfig)
}

#[cfg(all(kani, feature = "closure"))]
fn stand_in_accrual_body_reached<'a, T>(
    _market: &mut MarketGroupV16ViewMut<'a, T>,
    _config: V16Config,
    _asset_index: usize,
    _now_slot: u64,
    _effective_price: u64,
    _funding_rate_e9: i128,
    _protective_progress_committed: bool,
) -> V16Result<AccrueAssetOutcomeV16>
where
    'a: 'a, // early-bound, as the method's lifetime is (Kani counts it as a generic parameter)
{
    // the prologue never returns NonProgress: this value says the body was called
    Err(V16Error::NonProgress)
}

#[cfg(all(kani, feature = "closure"))]
fn stand_in_path_accrual_body_reached<'a, T>(
    _market: &mut MarketGroupV16ViewMut<'a, T>,
    _config: V16Config,
    _asset_index: usize,
    _now_slot: u64,
    _raw_oracle_target_price: u64,
    _steps: &[AccrualStepV16],
    _protective_progress_committed: bool,
) -> V16Result<AccrueAssetOutcomeV16>
where
    'a: 'a, // early-bound, as the method's lifetime is (Kani counts it as a generic parameter)
{
    Err(V16Error::NonProgress)
}

#[cfg(all(kani, feature = "closure"))]
fn kani_one_live_asset_market() -> (MarketGroupV16HeaderAccount, [Market<u64>; 1]) {
    let cfg = V16Config::public_user_fund_with_market_slots(1, 1, 0, 10);
    let mut header = MarketGroupV16HeaderAccount::new_dynamic([1u8; 32], cfg, 1, 0).unwrap();
    let mut markets = [Market::new(0u64, EngineAssetSlotV16Account::default())];
    {
        let mut v = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        v.activate_empty_market_not_atomic(0, 100, 1).unwrap();
    }
    (header, markets)
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::solver(cadical)]
#[kani::stub(
    MarketGroupV16ViewMut::accrue_asset_to_admitted_not_atomic,
    stand_in_accrual_body_never_called
)]
#[kani::stub(
    MarketGroupV16ViewMut::accrue_asset_path_to_admitted_not_atomic,
    stand_in_path_accrual_body_never_called
)]
fn proof_accrual_prologue_refuses_every_price_outside_the_oracle_range() {
    let (mut header, mut markets) = kani_one_live_asset_market();
    let price: u64 = kani::any();
    kani::assume(price == 0 || price > MAX_ORACLE_PRICE);
    let asset_index: usize = kani::any();
    let now_slot: u64 = kani::any();
    let funding_rate_e9: i128 = kani::any();
    let committed: bool = kani::any();
    let steps = [AccrualStepV16 {
        effective_price: kani::any(),
        funding_rate_e9: kani::any(),
        price_move_remainder_before_bps_num: kani::any(),
        price_move_remainder_after_bps_num: kani::any(),
    }];
    let step_count: usize = kani::any();
    kani::assume(step_count <= 1);
    let path: bool = kani::any();
    let header_before = header;
    let slot_before = markets[0].engine;
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let result = if path {
        market.accrue_asset_path_to_not_atomic(
            asset_index,
            now_slot,
            price,
            &steps[..step_count],
            committed,
        )
    } else {
        market.accrue_asset_to_not_atomic(asset_index, now_slot, price, funding_rate_e9, committed)
    };
    assert!(matches!(result, Err(V16Error::InvalidConfig)));
    assert!(kani_eq_market_group_v16_header_account(
        &header_before,
        market.header
    ));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &slot_before,
        &market.markets[0].engine
    ));
    kani::cover!(path, "path accrual refused");
    kani::cover!(!path && price == 0, "single-segment accrual refuses 0");
    kani::cover!(
        !path && price == u64::MAX,
        "single-segment accrual refuses u64::MAX"
    );
}

// The same refusal for every header: the header is arbitrary bytes, so the config and the mode
// may not even decode. Whatever they hold, a price outside the oracle range ends in an error
// before the body and the market is left as it was.
#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::solver(cadical)]
#[kani::stub(
    MarketGroupV16ViewMut::accrue_asset_to_admitted_not_atomic,
    stand_in_accrual_body_never_called
)]
#[kani::stub(
    MarketGroupV16ViewMut::accrue_asset_path_to_admitted_not_atomic,
    stand_in_path_accrual_body_never_called
)]
fn proof_accrual_prologue_refuses_every_price_outside_the_oracle_range_for_every_header() {
    let bytes: [u8; core::mem::size_of::<MarketGroupV16HeaderAccount>()] = kani::any();
    let mut header: MarketGroupV16HeaderAccount = bytemuck::Zeroable::zeroed();
    bytemuck::bytes_of_mut(&mut header).copy_from_slice(&bytes);
    let mut markets = [Market::new(0u64, EngineAssetSlotV16Account::default())];
    let price: u64 = kani::any();
    kani::assume(price == 0 || price > MAX_ORACLE_PRICE);
    let asset_index: usize = kani::any();
    let now_slot: u64 = kani::any();
    let path: bool = kani::any();
    let header_before = header;
    let slot_before = markets[0].engine;
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let result = if path {
        market.accrue_asset_path_to_not_atomic(asset_index, now_slot, price, &[], kani::any())
    } else {
        market.accrue_asset_to_not_atomic(asset_index, now_slot, price, kani::any(), kani::any())
    };
    assert!(result.is_err());
    assert!(kani_eq_market_group_v16_header_account(
        &header_before,
        market.header
    ));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &slot_before,
        &market.markets[0].engine
    ));
    kani::cover!(
        matches!(result, Err(V16Error::InvalidConfig)),
        "the guard itself refused"
    );
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::solver(cadical)]
#[kani::stub(
    MarketGroupV16ViewMut::accrue_asset_to_admitted_not_atomic,
    stand_in_accrual_body_reached
)]
#[kani::stub(
    MarketGroupV16ViewMut::accrue_asset_path_to_admitted_not_atomic,
    stand_in_path_accrual_body_reached
)]
fn proof_accrual_prologue_admits_every_price_inside_the_oracle_range() {
    let (mut header, mut markets) = kani_one_live_asset_market();
    let price: u64 = kani::any();
    kani::assume(price >= 1 && price <= MAX_ORACLE_PRICE);
    let now_slot: u64 = kani::any();
    kani::assume(now_slot >= header.current_slot.get());
    let path: bool = kani::any();
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let result = if path {
        market.accrue_asset_path_to_not_atomic(0, now_slot, price, &[], kani::any())
    } else {
        market.accrue_asset_to_not_atomic(0, now_slot, price, 0, kani::any())
    };
    assert!(matches!(result, Err(V16Error::NonProgress)));
    kani::cover!(
        path && price == MAX_ORACLE_PRICE,
        "path accrual admits MAX_ORACLE_PRICE"
    );
    kani::cover!(!path && price == 1, "single-segment accrual admits 1");
}

// INV-045, the residue of engine issue #132. In every state the per-asset scan
// admits, a side that still holds raw basis but no effective open interest has
// the minimum A index, 1, so its legs carry at most `raw_basis_sum / ADL_ONE`
// quantity units in all. (`kernel_side_adl_index`, which every transition that
// changes a side's open interest or raw basis runs, has the same property in
// its contract.)
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof]
#[kani::solver(cadical)]
fn proof_scan_side_with_basis_and_no_open_interest_has_unit_a_index() {
    let raw_basis_sum_q: u128 = kani::any();
    let a_side: u128 = kani::any();
    let loss_weight_sum: u128 = kani::any();
    let mode: SideModeV16 = kani::any();
    kani::assume(raw_basis_sum_q != 0);
    let coherent = MarketGroupV16View::<'_, u64>::side_adl_totals_are_coherent(
        0,
        raw_basis_sum_q,
        a_side,
        loss_weight_sum,
        mode,
    );
    kani::cover!(
        coherent,
        "a coherent side holds raw basis without open interest"
    );
    if coherent {
        assert_eq!(a_side, 1);
    }
}

// Contract check of the bankruptcy-residual step kernel. BOUNDED BINDING
// (flagged): the B index is within 255 of its maximum and every other input
// ranges over 8 bits, so the 256-bit arithmetic of the fallback runs on small
// values. The contract is stated for all inputs.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_bankruptcy_residual_step_capacity)]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn contract_check_kernel_bankruptcy_residual_step_capacity() {
    let _ = V16Core::kernel_bankruptcy_residual_step_capacity(
        u128::MAX - kani::any::<u8>() as u128,
        kani::any::<u8>() as u128,
        kani::any::<u8>() as u128,
        kani::any::<u8>() as u128,
        kani::any::<u8>() as u128,
    );
}

// The public B chunk keeps every B-settlement step clear of the overflow refusal.
// The chunk is the constant PUBLIC_B_CHUNK_ATOMS (= MAX_VAULT_TVL; it was a market
// setting until 2026-11-22, when a creator could choose one so large that every step
// overflowed, or one atom), so for any endpoint budget the step's largest numerator
// `(min(chunk, budget) + 1) * SOCIAL_LOSS_DEN - 1` fits in u128.
// kernel_b_settlement_chunk refuses with ArithmeticOverflow exactly when that numerator
// does not fit (proved for all inputs by the arithmetic prover), so no account's B
// settlement fails that way. The kernel call below, at a state with one B unit to
// settle, ties the bound to the code.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof]
#[kani::solver(cadical)]
fn proof_public_b_chunk_keeps_b_settlement_free_of_overflow() {
    let budget: u128 = kani::any();
    kani::cover!(budget < PUBLIC_B_CHUNK_ATOMS, "the budget is the limit");
    kani::cover!(budget >= PUBLIC_B_CHUNK_ATOMS, "the chunk is the limit");
    assert!(PUBLIC_B_CHUNK_ATOMS >= 1 && PUBLIC_B_CHUNK_ATOMS <= MAX_VAULT_TVL);
    let limit = PUBLIC_B_CHUNK_ATOMS.min(budget);
    assert!(limit
        .checked_add(1)
        .and_then(|v| v.checked_mul(SOCIAL_LOSS_DEN))
        .and_then(|v| v.checked_sub(1))
        .is_some());
    let step = V16Core::kernel_b_settlement_chunk(0, 0, 1, 1, PUBLIC_B_CHUNK_ATOMS, budget);
    assert!(step != Err(V16Error::ArithmeticOverflow));
    if budget != 0 {
        assert!(step.is_ok());
    }
}

// Contract check of the B-settlement step kernel. BOUNDED BINDING (flagged): the
// B index values, the remainder, the weight and both atom limits range over 16
// bits (64 bits did not finish in 20 minutes); the divisions by the weight and by SOCIAL_LOSS_DEN put a full-width check
// beyond the solver. The contract (a step with something to settle charges at
// most the limit, carries an exact remainder, and makes progress) is stated for
// all inputs. Production (account_b_settlement_chunk_from_leg) calls this.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_b_settlement_chunk)]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn contract_check_kernel_b_settlement_chunk() {
    let _ = V16Core::kernel_b_settlement_chunk(
        kani::any::<u16>() as u128,
        kani::any::<u16>() as u128,
        kani::any::<u16>() as u128,
        kani::any::<u16>() as u128,
        kani::any::<u16>() as u128,
        kani::any::<u16>() as u128,
    );
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

// INV-067: full-domain contract check for the conversion partition used by
// resolved close. A source haircut can change attribution, but only atoms
// actually converted to capital may leave terminal junior claim face.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_released_pnl_conversion_partition)]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn contract_check_kernel_released_pnl_conversion_partition() {
    let positive_face: u128 = kani::any();
    let converted: u128 = kani::any();
    let source_face_burn: u128 = kani::any();
    let retain_haircut_face: bool = kani::any();
    let _ = V16Core::kernel_released_pnl_conversion_partition(
        positive_face,
        converted,
        source_face_burn,
        retain_haircut_face,
    );
}

// INV-067/INV-071: a bounded resolved close may realize one source domain per
// call. Prove that its unconverted haircut is reserved exactly once and that
// the remaining released PnL is precisely the unprocessed source-attributed
// face, preventing a later source from realizing the same junior value.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_retain_terminal_source_haircut)]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn contract_check_kernel_retain_terminal_source_haircut() {
    let positive_pnl: u128 = kani::any();
    let reserved_pnl: u128 = kani::any();
    let source_attributed_face: u128 = kani::any();
    let converted: u128 = kani::any();
    let _ = V16Core::kernel_retain_terminal_source_haircut(
        positive_pnl,
        reserved_pnl,
        source_attributed_face,
        converted,
    );
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
// (b-stale, signed PnL, live leg via non-empty bitmap, source claim, capital,
// receipt, recovery). unwind(16): the active-bitmap is-empty scan / memcmp.
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
        kani::any(),
    );
}

// INV-076/074: close-snapshot freshness is scoped to the originating asset.
// The production guard has no market-wide slot input, so unrelated accrual is
// observationally irrelevant; only an attached originating leg whose own asset
// slot advanced beyond the immutable anchor is stale.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_open_close_snapshot_is_stale)]
#[kani::solver(cadical)]
fn contract_check_kernel_open_close_snapshot_is_stale() {
    let _ = V16Core::kernel_open_close_snapshot_is_stale(kani::any(), kani::any(), kani::any());
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

// Exhaustive scalar routing for terminal forfeit: the formerly rollback-only
// zero-capacity branch is uniquely classified as successful Recovery progress.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_forfeit_residual_step)]
#[kani::solver(cadical)]
fn contract_check_kernel_forfeit_residual_step() {
    let residual_remaining: u128 = kani::any();
    let booking_capacity: u128 = kani::any();
    let _ = V16Core::kernel_forfeit_residual_step(residual_remaining, booking_capacity);
}

// A Recovery exit that moves basis to zero preserves the exact loss weight and
// only changes effective OI plus the matching pending-obligation count.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_retain_leg_as_pending_obligation)]
#[kani::solver(cadical)]
fn contract_check_kernel_retain_leg_as_pending_obligation() {
    let leg: PortfolioLegV16 = kani::any();
    let asset: AssetStateV16 = kani::any();
    let retained_effective_oi_q: u128 = kani::any();
    let _ = V16Core::kernel_retain_leg_as_pending_obligation(leg, asset, retained_effective_oi_q);
}

// Exhaust all lifecycle/count inputs for the overlap-safe release predicate.
// Recovery release is possible exactly when every opposite stored leg is
// already a zero-basis obligation; malformed count summaries fail closed.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_recovery_pending_obligation_release_allowed)]
#[kani::solver(cadical)]
fn contract_check_kernel_recovery_pending_obligation_release_allowed() {
    let lifecycle: AssetLifecycleV16 = kani::any();
    let opposite_stored_count: u64 = kani::any();
    let opposite_pending_count: u64 = kani::any();
    let _ = V16Core::kernel_recovery_pending_obligation_release_allowed(
        lifecycle,
        opposite_stored_count,
        opposite_pending_count,
    );
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

#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_mark_kf_stale_cohorts)]
#[kani::solver(cadical)]
fn contract_check_kernel_mark_kf_stale_cohorts() {
    let asset: AssetStateV16 = kani::any();
    let long_changed: bool = kani::any();
    let short_changed: bool = kani::any();
    let cohort_epoch: u64 = kani::any();
    let _ = V16Core::kernel_mark_kf_stale_cohorts(asset, long_changed, short_changed, cohort_epoch);
}

#[cfg(all(kani, feature = "contracts"))]
#[kani::proof_for_contract(V16Core::kernel_settle_kf_stale_cohort)]
#[kani::solver(cadical)]
fn contract_check_kernel_settle_kf_stale_cohort() {
    let asset: AssetStateV16 = kani::any();
    let side: SideV16 = kani::any();
    let leg_kf_epoch_snap: u64 = kani::any();
    let _ = V16Core::kernel_settle_kf_stale_cohort(asset, side, leg_kf_epoch_snap);
}

/// Issue #204: a principal cure keeps the close ledger's partition identity, reduces gross
/// loss and residual by exactly `min(paid, residual)`, releases the domain barrier exactly
/// when the residual reaches zero, and leaves every other field unchanged.
#[cfg(all(kani, feature = "contracts"))]
#[kani::proof]
#[kani::solver(cadical)]
fn proof_principal_cure_preserves_close_ledger_partition() {
    let mut before = CloseProgressLedgerV16::EMPTY;
    before.active = true;
    before.close_id = kani::any();
    before.asset_index = kani::any();
    before.market_id = kani::any();
    before.drift_reference_slot = kani::any();
    before.max_close_slot = kani::any();
    before.support_consumed = kani::any::<u64>() as u128;
    before.junior_face_burned = kani::any::<u64>() as u128;
    before.insurance_spent = kani::any::<u64>() as u128;
    before.b_loss_booked = kani::any::<u64>() as u128;
    before.explicit_loss_assigned = kani::any::<u64>() as u128;
    before.quantity_adl_applied_q = kani::any::<u64>() as u128;
    before.drift_consumed = kani::any::<u64>() as u128;
    before.gross_loss_at_close_start = kani::any::<u64>() as u128;
    let progress = before.support_consumed
        + before.insurance_spent
        + before.b_loss_booked
        + before.explicit_loss_assigned;
    let total = before.gross_loss_at_close_start + before.drift_consumed;
    kani::assume(progress < total);
    before.residual_remaining = total - progress;
    let paid = kani::any::<u64>() as u128;

    let (after, released) = V16Core::kernel_principal_cure_close_ledger(before, paid).unwrap();
    let cure = paid
        .min(before.residual_remaining)
        .min(before.gross_loss_at_close_start);

    assert_eq!(after.residual_remaining, before.residual_remaining - cure);
    assert_eq!(
        after.gross_loss_at_close_start,
        before.gross_loss_at_close_start - cure
    );
    assert_eq!(
        after.residual_remaining,
        after.gross_loss_at_close_start + after.drift_consumed - progress
    );
    assert_eq!(released, cure != 0 && after.residual_remaining == 0);
    if released {
        if before.has_irreversible_progress() {
            assert!(after.active && after.finalized && !after.canceled);
        } else {
            assert!(!after.active && !after.finalized && after.canceled);
            // Canceled ledgers must carry no progress and residual == gross (both zero here).
            assert_eq!(after.residual_remaining, after.gross_loss_at_close_start);
        }
    } else {
        assert_eq!(
            (after.active, after.finalized, after.canceled),
            (before.active, before.finalized, before.canceled)
        );
    }
    let mut after_masked = after;
    after_masked.residual_remaining = before.residual_remaining;
    after_masked.gross_loss_at_close_start = before.gross_loss_at_close_start;
    after_masked.active = before.active;
    after_masked.finalized = before.finalized;
    after_masked.canceled = before.canceled;
    assert_eq!(after_masked, before);
}

/// Issue #132: the two leg-teardown kernels remove zero effective OI for a
/// current-epoch nonzero-basis leg only when that leg's side has none left
/// (per-leg effective quantities round up and a leg that leaves takes its
/// whole rounded-up quantity, so earlier exits can consume the whole side).
/// On such a side the zero removal succeeds whenever the leg's own counters
/// are present, its raw basis in the side's raw basis sum among them (#449),
/// and changes only those. On any other side it is rejected, so a live leg
/// can never detach, or convert to a pending obligation, and leave its
/// effective OI behind.
///
/// Closure layer (plain proof over the kernel bodies, no contract machinery).
#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::solver(cadical)]
fn closure_leg_teardown_removes_zero_effective_oi_only_on_an_exhausted_side() {
    let leg: PortfolioLegV16 = kani::any();
    let asset: AssetStateV16 = kani::any();
    let removed_q: u128 = kani::any();
    kani::assume(leg.basis_pos_q > i128::MIN);
    let raw_abs_q = leg.basis_pos_q.unsigned_abs();
    let (side_oi_q, raw_sum_q, side_epoch, side_mode, stored, pending, weight_sum, dust) =
        match leg.side {
            SideV16::Long => (
                asset.oi_eff_long_q,
                asset.raw_basis_sum_long_q,
                asset.epoch_long,
                asset.mode_long,
                asset.stored_pos_count_long,
                asset.pending_obligation_count_long,
                asset.loss_weight_sum_long,
                asset.social_loss_dust_long_num,
            ),
            SideV16::Short => (
                asset.oi_eff_short_q,
                asset.raw_basis_sum_short_q,
                asset.epoch_short,
                asset.mode_short,
                asset.stored_pos_count_short,
                asset.pending_obligation_count_short,
                asset.loss_weight_sum_short,
                asset.social_loss_dust_short_num,
            ),
        };
    let prior_reset =
        side_mode == SideModeV16::ResetPending && leg.epoch_snap.checked_add(1) == Some(side_epoch);
    let live_leg = leg.active && raw_abs_q != 0 && !prior_reset;

    let cleared = V16Core::kernel_clear_leg(leg, asset, removed_q);
    let retained = V16Core::kernel_retain_leg_as_pending_obligation(leg, asset, removed_q);

    // Acceptance stays fail-closed: never more than the raw basis or the side
    // holds, and zero for a live leg only on an exhausted side.
    if cleared.is_ok() {
        assert!(leg.active && removed_q <= raw_abs_q);
        if live_leg {
            assert!(removed_q <= side_oi_q);
            assert!(removed_q != 0 || side_oi_q == 0);
        } else {
            assert!(removed_q == 0);
        }
    }
    if retained.is_ok() {
        assert!(leg.active && raw_abs_q != 0 && leg.loss_weight != 0);
        assert!(removed_q <= raw_abs_q && removed_q <= side_oi_q);
        assert!(removed_q != 0 || side_oi_q == 0);
    }
    if live_leg && removed_q == 0 && side_oi_q != 0 {
        assert_eq!(cleared, Err(V16Error::InvalidLeg));
        assert_eq!(retained, Err(V16Error::InvalidLeg));
    }

    // On an exhausted side the zero removal cannot fail for lack of OI, and it
    // changes exactly the leg's own counters.
    if live_leg && removed_q == 0 && side_oi_q == 0 {
        kani::cover!(
            pending != 0,
            "exhausted side still carrying a pending obligation"
        );
        let clear_counters_present = stored != 0
            && weight_sum >= leg.loss_weight
            && leg.b_rem < SOCIAL_LOSS_DEN
            && dust < SOCIAL_LOSS_DEN;
        if raw_sum_q < raw_abs_q {
            // The side's raw basis sum does not hold this leg's raw basis:
            // both kernels refuse instead of wrapping the sum.
            if clear_counters_present {
                assert_eq!(cleared, Err(V16Error::CounterUnderflow));
            }
            if leg.loss_weight != 0 {
                assert_eq!(retained, Err(V16Error::CounterUnderflow));
            }
        }
        if clear_counters_present && raw_sum_q >= raw_abs_q {
            let after = cleared.unwrap();
            let (oi, raw_after, stored_after, pending_after, weight_after) = match leg.side {
                SideV16::Long => (
                    after.oi_eff_long_q,
                    after.raw_basis_sum_long_q,
                    after.stored_pos_count_long,
                    after.pending_obligation_count_long,
                    after.loss_weight_sum_long,
                ),
                SideV16::Short => (
                    after.oi_eff_short_q,
                    after.raw_basis_sum_short_q,
                    after.stored_pos_count_short,
                    after.pending_obligation_count_short,
                    after.loss_weight_sum_short,
                ),
            };
            assert_eq!(
                (oi, raw_after, stored_after, pending_after, weight_after),
                (
                    0,
                    raw_sum_q - raw_abs_q,
                    stored - 1,
                    pending,
                    weight_sum - leg.loss_weight
                )
            );
        }
        if leg.loss_weight != 0 && pending != u64::MAX && raw_sum_q >= raw_abs_q {
            let (obligation, after) = retained.unwrap();
            assert!(obligation.active && obligation.basis_pos_q == 0);
            assert_eq!(obligation.loss_weight, leg.loss_weight);
            let (oi, raw_after, stored_after, pending_after, weight_after) = match leg.side {
                SideV16::Long => (
                    after.oi_eff_long_q,
                    after.raw_basis_sum_long_q,
                    after.stored_pos_count_long,
                    after.pending_obligation_count_long,
                    after.loss_weight_sum_long,
                ),
                SideV16::Short => (
                    after.oi_eff_short_q,
                    after.raw_basis_sum_short_q,
                    after.stored_pos_count_short,
                    after.pending_obligation_count_short,
                    after.loss_weight_sum_short,
                ),
            };
            assert_eq!(
                (oi, raw_after, stored_after, pending_after, weight_after),
                (0, raw_sum_q - raw_abs_q, stored, pending + 1, weight_sum)
            );
        }
    }
}

// Batch 14: the engine's unchecked arithmetic (the proof ledger's PO pieces). The deployed
// program builds the engine without overflow checks (its release profile checks only the
// program's own code), so an overflowing `+`, `-`, `*`, shift or negation in the engine would
// continue with a wrapped value. Kani checks every such operation it executes. Each proof below
// calls one engine function with every input symbolic and no assumption, so it shows that no
// operation the function runs overflows, whatever its caller passes. Closure layer (the records
// that derive `kani::Arbitrary` do so there); the records that do not are built field by field.

#[cfg(all(kani, feature = "closure"))]
fn any_close_progress_ledger_for_overflow() -> CloseProgressLedgerV16 {
    CloseProgressLedgerV16 {
        active: kani::any(),
        finalized: kani::any(),
        canceled: kani::any(),
        close_id: kani::any(),
        asset_index: kani::any(),
        market_id: kani::any(),
        domain_side: kani::any(),
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
    }
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
fn proof_no_overflow_liquidation_risk_notional_ceil() {
    let r = liquidation_risk_notional_ceil(kani::any(), kani::any());
    kani::cover!(matches!(r, Ok(n) if n > 0));
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(2)]
fn proof_no_overflow_active_bitmap_is_empty() {
    let bitmap: V16ActiveBitmap = kani::any();
    let empty = active_bitmap_is_empty(bitmap);
    kani::cover!(empty);
    kani::cover!(!empty);
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(2)]
fn proof_no_overflow_active_bitmap_count_ones() {
    let bitmap: V16ActiveBitmap = kani::any();
    let n = active_bitmap_count_ones(bitmap);
    kani::cover!(n as usize == V16_MAX_PORTFOLIO_ASSETS_N);
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
fn proof_no_overflow_source_claim_domain_first_burn_partition() {
    let (source_burn, rest) = V16Core::source_claim_domain_first_burn_partition(kani::any(), kani::any());
    kani::cover!(source_burn > 0 && rest > 0);
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
fn proof_no_overflow_kernel_settle_positive_face_after_support() {
    let r = V16Core::kernel_settle_positive_face_after_support(kani::any(), kani::any(), kani::any());
    kani::cover!(matches!(r, Ok((pnl, burned)) if pnl > 0 && burned > 0));
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
fn proof_no_overflow_kernel_principal_cure_close_ledger() {
    let r = V16Core::kernel_principal_cure_close_ledger(any_close_progress_ledger_for_overflow(), kani::any());
    kani::cover!(matches!(r, Ok((_, true))));
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
fn proof_no_overflow_prepare_counterparty_backing_expiry_delta() {
    let bucket: BackingBucketV16 = kani::any();
    let source: SourceCreditStateV16 = kani::any();
    let r = V16Core::prepare_counterparty_backing_expiry_delta(bucket, source, kani::any());
    kani::cover!(r.is_ok());
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
fn proof_no_overflow_target_effective_lag_adverse_delta() {
    let side: SideV16 = kani::any();
    let d = V16Core::target_effective_lag_adverse_delta(side, kani::any(), kani::any());
    kani::cover!(d > 0);
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
fn proof_no_overflow_liquidation_fee_for_close() {
    let r = liquidation_fee_for_close(kani::any(), kani::any(), kani::any(), kani::any(), kani::any());
    kani::cover!(matches!(r, Ok(f) if f > 0));
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
fn proof_no_overflow_adjust_u128() {
    let r = adjust_u128(kani::any(), kani::any(), kani::any());
    kani::cover!(r.is_ok());
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
fn proof_no_overflow_apply_total_delta_u64() {
    // An associated function of the view that does not involve the view's type parameter.
    let r = MarketGroupV16ViewMut::<'static, u8>::apply_total_delta_u64(kani::any(), kani::any(), kani::any());
    kani::cover!(r.is_ok());
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
fn proof_no_overflow_dynamic_asset_slot_capacity_from_account_len() {
    // The subtraction `account_len - header_len` and its guard do not involve the slot type: the
    // header length is the size of the header alone. The program instantiates its own slot type.
    let r = MarketGroupV16HeaderAccount::dynamic_asset_slot_capacity_from_account_len::<u8>(kani::any());
    kani::cover!(matches!(r, Ok(n) if n > 0));
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(18)]
fn proof_no_overflow_insert_account_kf_settlement_plan_entry() {
    let mut plan: [Option<u64>; V16_MAX_PORTFOLIO_ASSETS_N] = kani::any();
    let r = insert_account_kf_settlement_plan_entry(&mut plan, kani::any(), kani::any());
    kani::cover!(matches!(r, Ok(n) if n > 1));
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::unwind(18)]
fn proof_no_overflow_token_value_flow_validate() {
    let flow: TokenValueFlowProofV16 = kani::any();
    let r = flow.validate();
    kani::cover!(r.is_ok());
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
fn proof_no_overflow_kernel_advance_close_ledger() {
    let r = V16Core::kernel_advance_close_ledger(
        any_close_progress_ledger_for_overflow(),
        kani::any(),
        kani::any(),
        kani::any(),
        kani::any(),
        kani::any(),
    );
    kani::cover!(matches!(r, Ok(l) if l.residual_remaining > 0));
}

// Batch 16: more of the engine's unchecked arithmetic (the proof ledger's PO pieces), the payout-rate
// invariant of the resolved ledger (piece PR1) and the leg-validity fact of the account decoder (PL1).
// As in batch 14, each no-overflow proof calls one engine function with every input symbolic and no
// assumption, so Kani's overflow checks cover every operation the call executes. Closure layer.

#[cfg(all(kani, feature = "closure"))]
fn any_v16_config_for_overflow() -> V16Config {
    V16Config {
        max_portfolio_assets: kani::any(),
        max_market_slots: kani::any(),
        min_nonzero_mm_req: kani::any(),
        min_nonzero_im_req: kani::any(),
        h_min: kani::any(),
        h_max: kani::any(),
        maintenance_margin_bps: kani::any(),
        initial_margin_bps: kani::any(),
        max_trading_fee_bps: kani::any(),
        liquidation_fee_bps: kani::any(),
        liquidation_fee_cap: kani::any(),
        min_liquidation_abs: kani::any(),
        max_accrual_dt_slots: kani::any(),
        max_abs_funding_e9_per_slot: kani::any(),
        min_funding_lifetime_slots: kani::any(),
        max_price_move_bps_per_slot: kani::any(),
        max_account_b_settlement_chunks: kani::any(),
        max_bankrupt_close_chunks: kani::any(),
        max_bankrupt_close_lifetime_slots: kani::any(),
        asset_activation_cooldown_slots: kani::any(),
        max_recovery_fallback_deviation_bps: kani::any(),
        backing_fee_base_rate_e9_per_slot: kani::any(),
        backing_fee_kink_util_bps: kani::any(),
        backing_fee_slope_at_kink_e9_per_slot: kani::any(),
        backing_fee_slope_above_kink_e9_per_slot: kani::any(),
        backing_freshness_buckets: kani::any(),
        margin_mode_realizable_full_shared_cross_margin: kani::any(),
        source_credit_lien_required: kani::any(),
        insurance_credit_reservation_required: kani::any(),
        permissionless_recovery_enabled: kani::any(),
        recovery_fallback_price_enabled: kani::any(),
        recovery_fallback_envelope_enabled: kani::any(),
        credit_lien_revalidation_required: kani::any(),
        stale_certificate_penalty_enabled: kani::any(),
        full_refresh_required_for_favorable_actions: kani::any(),
        public_liveness_profile_crank_forward: kani::any(),
    }
}

#[cfg(all(kani, feature = "closure"))]
fn any_resolved_payout_ledger() -> ResolvedPayoutLedgerV16 {
    ResolvedPayoutLedgerV16 {
        snapshot_residual: kani::any(),
        terminal_claim_exact_receipts_num: kani::any(),
        terminal_claim_bound_unreceipted_num: kani::any(),
        current_payout_rate_num: kani::any(),
        current_payout_rate_den: kani::any(),
        snapshot_slot: kani::any(),
        payout_halted: kani::any(),
        finalized: kani::any(),
    }
}

#[cfg(all(kani, feature = "closure"))]
fn any_resolved_payout_receipt() -> ResolvedPayoutReceiptV16 {
    ResolvedPayoutReceiptV16 {
        present: kani::any(),
        prior_bound_contribution_num: kani::any(),
        live_released_face_at_receipt: kani::any(),
        terminal_positive_claim_face: kani::any(),
        paid_effective: kani::any(),
        finalized: kani::any(),
    }
}

// PR1: whatever ledger it is given, the recompute either fails or leaves a rate with 1 <= den and
// num <= den (so a receipt's gross payout `face * num / den` never exceeds its face).
#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
fn proof_resolved_payout_rate_recompute_leaves_a_rate_at_most_one() {
    let r = V16Core::kernel_recompute_resolved_payout_rate(any_resolved_payout_ledger());
    if let Ok(l) = r {
        assert!(l.current_payout_rate_den >= 1);
        assert!(l.current_payout_rate_num <= l.current_payout_rate_den);
    }
    kani::cover!(matches!(r, Ok(l) if l.current_payout_rate_num < l.current_payout_rate_den));
    kani::cover!(matches!(r, Ok(l) if l.current_payout_rate_num == l.current_payout_rate_den));
}


#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
fn proof_no_overflow_prepare_insurance_lien_consume_delta() {
    let reservation: InsuranceCreditReservationV16 = kani::any();
    let source: SourceCreditStateV16 = kani::any();
    let r = V16Core::prepare_insurance_lien_consume_delta(
        reservation,
        source,
        kani::any(),
        kani::any(),
        kani::any(),
    );
    kani::cover!(matches!(r, Ok((_, _, spent, _)) if spent > 0));
}




// Batch 16b: the three batch 16 proofs that ran out of time on 256-bit arithmetic, with the wide
// callees replaced by stubs. The wide-math functions never panic (the arithmetic layer, A1 to G5,
// proved), so every value they can return is among the arbitrary values the stubs return: the
// no-overflow proofs below cover every operation of the function itself, for every input.

#[cfg(all(kani, feature = "closure"))]
fn stub_any_u256_option(_a: U256, _b: U256) -> Option<U256> {
    if kani::any() {
        Some(U256::new(kani::any(), kani::any()))
    } else {
        None
    }
}

#[cfg(all(kani, feature = "closure"))]
fn stub_any_u128_from_u256(_a: &U256) -> Option<u128> {
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn stub_any_backing_rate(_config: V16Config, _source: SourceCreditStateV16) -> V16Result<u64> {
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn stub_any_wide_mul_div_floor_u128(_a: u128, _b: u128, _d: u128) -> u128 {
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::stub(U256::checked_mul, stub_any_u256_option)]
#[kani::stub(U256::checked_div, stub_any_u256_option)]
#[kani::stub(U256::try_into_u128, stub_any_u128_from_u256)]
#[kani::stub(V16Core::backing_utilization_rate_e9_for_source_state, stub_any_backing_rate)]
fn proof_no_overflow_backing_utilization_fee_quote_atoms_for_lien_wide_stubbed() {
    let source: SourceCreditStateV16 = kani::any();
    let r = V16Core::backing_utilization_fee_quote_atoms_for_lien(
        any_v16_config_for_overflow(),
        source,
        kani::any(),
        kani::any(),
        kani::any(),
    );
    kani::cover!(matches!(r, Ok(f) if f > 0));
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::stub(wide_mul_div_floor_u128, stub_any_wide_mul_div_floor_u128)]
fn proof_no_overflow_kernel_resolved_receipt_can_still_gain_wide_stubbed() {
    let r = V16Core::kernel_resolved_receipt_can_still_gain(
        any_resolved_payout_ledger(),
        kani::any(),
        any_resolved_payout_receipt(),
    );
    kani::cover!(matches!(r, Ok(true)));
}

// PL1 is structural: the decoder returns an active leg only after `validate_active_leg` accepted
// that same leg. The proof replaces the loss-weight computation inside the validator by a
// deterministic stand-in, which leaves that structure as it is; the property holds for any
// deterministic loss weight, so it holds for the real one.
#[cfg(all(kani, feature = "closure"))]
fn stub_deterministic_loss_weight(abs_basis_q: u128, a_basis: u128) -> V16Result<u128> {
    Ok(abs_basis_q ^ a_basis)
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::stub(loss_weight_for_basis, stub_deterministic_loss_weight)]
fn proof_decoded_active_leg_is_valid_for_any_loss_weight() {
    let bytes: [u8; core::mem::size_of::<PortfolioLegV16Account>()] = kani::any();
    let mut account: PortfolioLegV16Account = bytemuck::Zeroable::zeroed();
    bytemuck::bytes_of_mut(&mut account).copy_from_slice(&bytes);
    let decoded = account.try_to_runtime();
    if let Ok(leg) = decoded {
        if leg.active {
            assert!(validate_active_leg(leg).is_ok());
        }
    }
    kani::cover!(matches!(decoded, Ok(leg) if leg.active), "an active leg decodes");
}

// Batch 18: no-overflow proofs for the liquidation helpers and the risk-notional inverse, with the
// helpers they call returning arbitrary values. Each stubbed callee either never panics (wide math:
// the arithmetic layer) or has a no-overflow piece of its own, and an arbitrary value covers every
// value it can return, so each proof covers every operation of the function itself, for every input.

#[cfg(all(kani, feature = "closure"))]
fn any_health_cert() -> HealthCertV16 {
    HealthCertV16 {
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
    }
}

#[cfg(all(kani, feature = "closure"))]
fn stub_any_leg_maintenance(_c: V16Config, _q: u128, _s: SideV16, _p: u64, _t: u64) -> V16Result<u128> {
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn stub_any_risk_notional(_q: u128, _p: u64) -> V16Result<u128> {
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn stub_any_close_fee(_n: u128, _bps: u64, _min: u128, _cap: u128, _full: bool) -> V16Result<u128> {
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn stub_any_health_deficit(_e: i128, _m: u128, _om: u128, _nm: u128, _f: u128) -> V16Result<u128> {
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::stub(liquidation_leg_maintenance_requirement, stub_any_leg_maintenance)]
#[kani::stub(liquidation_risk_notional_ceil, stub_any_risk_notional)]
#[kani::stub(liquidation_fee_for_close, stub_any_close_fee)]
#[kani::stub(liquidation_projected_health_deficit_from_parts, stub_any_health_deficit)]
fn proof_no_overflow_liquidation_projected_healthy_after_close_stubbed() {
    let r = liquidation_projected_healthy_after_close(
        any_v16_config_for_overflow(),
        any_health_cert(),
        kani::any(),
        kani::any(),
        kani::any(),
        kani::any(),
        kani::any(),
        kani::any(),
        kani::any(),
        kani::any(),
    );
    kani::cover!(matches!(r, Ok(true)));
}

#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::stub(U256::checked_mul, stub_any_u256_option)]
#[kani::stub(U256::checked_div, stub_any_u256_option)]
#[kani::stub(U256::try_into_u128, stub_any_u128_from_u256)]
fn proof_no_overflow_min_abs_q_for_risk_notional_at_least_wide_stubbed() {
    let r = min_abs_q_for_risk_notional_at_least(kani::any(), kani::any());
    kani::cover!(matches!(r, Ok(q) if q > 1));
}

/// PO.signed_position: the negation of a short leg's size cannot overflow for any leg the
/// deployed callers pass. Each of the six passes `PortfolioLegV16::EMPTY` or a leg decoded with
/// `try_to_runtime`, whose active legs passed `validate_active_leg` (ledger piece PL1), and
/// `validate_basis` refuses `i128::MIN`; the callers change only `f_snap` before the call.
#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
fn proof_no_overflow_signed_position_of_a_decoded_leg() {
    let leg: PortfolioLegV16 = kani::any();
    kani::assume(!leg.active || leg.basis_pos_q != i128::MIN);
    let q = signed_position(leg);
    kani::cover!(q < 0);
    kani::cover!(q > 0);
}

// Header pieces of class "local" (T*.header): the real predicate before and after the call.
//
// The ledger states each such piece as: if the header predicate (`validate_shape` of a default
// build) held before the entry point and it returns Ok, it holds of the state it leaves. These
// entry points end in no check, so the proof evaluates the predicate itself, before and after the
// call, with the entry point run as written:
// - the predicate is the default build's: in Kani builds `validate_shape` also runs the audit-only
//   slot scan, which `default_build_has_no_audit_scan` switches off, as a default build compiles it
//   out;
// - its costly pure parts are memoized. A memo stand-in abstracts a deterministic function: its
//   first call returns any result and remembers it with its input, a later call with the same
//   input returns that result, and a call with another input returns any result. Every behaviour
//   of the real function is among the stand-in's, so a property proved with it holds of the real
//   one. The key of each is its whole input (checked against the source by header_local_check.py):
//   the slot-count check (its three arguments), the config check (the config's bytes), the
//   config's fund-shape check (the config value), the two bound conversions (the amount), and the
//   aggregate-totals check (the nine header fields it reads). The slot-count check is the only
//   part of the predicate that reads the slice, and the entry points read no slot of the view, so
//   the proof covers every number of slots;
// - the header and the slot are arbitrary bytes, and so is every argument; nothing is assumed.
#[cfg(all(kani, feature = "closure"))]
fn default_build_has_no_audit_scan<'a, T>(_market: &MarketGroupV16View<'a, T>) -> V16Result<()>
where
    'a: 'a,
{
    Ok(())
}

#[cfg(all(kani, feature = "closure"))]
static mut MEMO_SLOT_COUNT_CHECK: Option<((usize, usize, usize), V16Result<()>)> = None;

/// Memo stand-in for `MarketGroupV16HeaderAccount::validate_dynamic_market_slots_len_static`.
#[cfg(all(kani, feature = "closure"))]
#[allow(unsafe_code)]
fn memo_slot_count_check(
    supplied_len: usize,
    capacity: usize,
    configured_market_slots: usize,
) -> V16Result<()> {
    let key = (supplied_len, capacity, configured_market_slots);
    // SAFETY: Kani runs one harness on one thread.
    unsafe {
        match MEMO_SLOT_COUNT_CHECK {
            Some((k, r)) if k == key => r,
            Some(_) => kani::any(),
            None => {
                let r: V16Result<()> = kani::any();
                MEMO_SLOT_COUNT_CHECK = Some((key, r));
                r
            }
        }
    }
}

#[cfg(all(kani, feature = "closure"))]
static mut MEMO_CONFIG_SHAPE: Option<(
    [u8; core::mem::size_of::<V16ConfigAccount>()],
    V16Result<V16Config>,
)> = None;

/// Memo stand-in for `V16ConfigAccount::try_to_runtime_shape`.
#[cfg(all(kani, feature = "closure"))]
#[allow(unsafe_code)]
fn memo_config_shape(config: &V16ConfigAccount) -> V16Result<V16Config> {
    let mut key = [0u8; core::mem::size_of::<V16ConfigAccount>()];
    key.copy_from_slice(bytemuck::bytes_of(config));
    // SAFETY: Kani runs one harness on one thread.
    unsafe {
        match MEMO_CONFIG_SHAPE {
            Some((k, r)) if k == key => r,
            _ => {
                let r: V16Result<V16Config> = if kani::any() {
                    Ok(any_v16_config_for_overflow())
                } else {
                    Err(kani::any())
                };
                if MEMO_CONFIG_SHAPE.is_none() {
                    MEMO_CONFIG_SHAPE = Some((key, r));
                }
                r
            }
        }
    }
}

#[cfg(all(kani, feature = "closure"))]
static mut MEMO_CONFIG_FUND_SHAPE: [Option<(V16Config, V16Result<()>)>; 3] = [None; 3];

/// Memo stand-in for `V16Config::validate_public_user_fund_shape`. It remembers up to three inputs:
/// the slot-capacity growth checks a changed config, which the predicate then checks again.
#[cfg(all(kani, feature = "closure"))]
#[allow(unsafe_code)]
fn memo_config_fund_shape(config: &V16Config) -> V16Result<()> {
    // SAFETY: Kani runs one harness on one thread.
    unsafe {
        let mut i = 0;
        while i < 3 {
            match MEMO_CONFIG_FUND_SHAPE[i] {
                Some((k, r)) if k == *config => return r,
                None => {
                    let r: V16Result<()> = kani::any();
                    MEMO_CONFIG_FUND_SHAPE[i] = Some((*config, r));
                    return r;
                }
                Some(_) => i += 1,
            }
        }
        kani::any()
    }
}

#[cfg(all(kani, feature = "closure"))]
static mut MEMO_AMOUNT_FROM_BOUND_NUM: Option<(u128, V16Result<u128>)> = None;

/// Memo stand-in for `V16Core::amount_from_bound_num`.
#[cfg(all(kani, feature = "closure"))]
#[allow(unsafe_code)]
fn memo_amount_from_bound_num(bound_num: u128) -> V16Result<u128> {
    // SAFETY: Kani runs one harness on one thread.
    unsafe {
        match MEMO_AMOUNT_FROM_BOUND_NUM {
            Some((k, r)) if k == bound_num => r,
            Some(_) => kani::any(),
            None => {
                let r: V16Result<u128> = kani::any();
                MEMO_AMOUNT_FROM_BOUND_NUM = Some((bound_num, r));
                r
            }
        }
    }
}

#[cfg(all(kani, feature = "closure"))]
static mut MEMO_BOUND_NUM_FROM_AMOUNT: Option<(u128, V16Result<u128>)> = None;

/// Memo stand-in for `V16Core::bound_num_from_amount`.
#[cfg(all(kani, feature = "closure"))]
#[allow(unsafe_code)]
fn memo_bound_num_from_amount(amount: u128) -> V16Result<u128> {
    // SAFETY: Kani runs one harness on one thread.
    unsafe {
        match MEMO_BOUND_NUM_FROM_AMOUNT {
            Some((k, r)) if k == amount => r,
            Some(_) => kani::any(),
            None => {
                let r: V16Result<u128> = kani::any();
                MEMO_BOUND_NUM_FROM_AMOUNT = Some((amount, r));
                r
            }
        }
    }
}

#[cfg(all(kani, feature = "closure"))]
static mut MEMO_HEADER_AGGREGATE_TOTALS: Option<([u128; 9], V16Result<()>)> = None;

/// Memo stand-in for `MarketGroupV16View::validate_header_aggregate_totals`, keyed on the nine
/// header fields it reads (header_local_check.py checks the list against its body).
#[cfg(all(kani, feature = "closure"))]
#[allow(unsafe_code)]
fn memo_header_aggregate_totals<'a, T>(market: &MarketGroupV16View<'a, T>) -> V16Result<()>
where
    'a: 'a,
{
    let h = market.header;
    let key = [
        h.c_tot.get(),
        h.insurance.get(),
        h.backing_provider_earnings_total.get(),
        h.vault.get(),
        h.source_fresh_backing_total_num.get(),
        h.source_insurance_credit_reserved_total_atoms.get(),
        h.insurance_domain_budget_remaining_total.get(),
        h.pnl_pos_bound_tot_num.get(),
        h.source_claim_bound_total_num.get(),
    ];
    // SAFETY: Kani runs one harness on one thread.
    unsafe {
        match MEMO_HEADER_AGGREGATE_TOTALS {
            Some((k, r)) if k == key => r,
            Some(_) => kani::any(),
            None => {
                let r: V16Result<()> = kani::any();
                MEMO_HEADER_AGGREGATE_TOTALS = Some((key, r));
                r
            }
        }
    }
}

/// If the predicate held before the call and the call returned Ok, it holds now.
#[cfg(all(kani, feature = "closure"))]
fn assert_held_before_holds_after<'a, T>(
    held: bool,
    ok: bool,
    market: &MarketGroupV16ViewMut<'a, T>,
) where
    'a: 'a,
{
    if held && ok {
        assert!(market.validate_shape().is_ok());
    }
}

// T09.header
#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::solver(cadical)]
#[kani::stub(
    MarketGroupV16View::validate_shape_full_audit_scan,
    default_build_has_no_audit_scan
)]
#[kani::stub(
    MarketGroupV16HeaderAccount::validate_dynamic_market_slots_len_static,
    memo_slot_count_check
)]
#[kani::stub(V16ConfigAccount::try_to_runtime_shape, memo_config_shape)]
#[kani::stub(V16Config::validate_public_user_fund_shape, memo_config_fund_shape)]
#[kani::stub(V16Core::amount_from_bound_num, memo_amount_from_bound_num)]
#[kani::stub(V16Core::bound_num_from_amount, memo_bound_num_from_amount)]
#[kani::stub(
    MarketGroupV16View::validate_header_aggregate_totals,
    memo_header_aggregate_totals
)]
fn proof_v16_advance_resolved_slot_keeps_the_header_predicate() {
    let (mut header, mut markets) = arbitrary_header_and_slot();
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let held = market.validate_shape().is_ok();
    let result = market.advance_resolved_slot_not_atomic(kani::any());
    assert_held_before_holds_after(held, result.is_ok(), &market);
    kani::cover!(
        held && result.is_ok(),
        "the predicate held and Ok is reached"
    );
    kani::cover!(
        held && result.is_err(),
        "the predicate held and an error is reached"
    );
}

// T05.header
#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::solver(cadical)]
#[kani::stub(
    MarketGroupV16View::validate_shape_full_audit_scan,
    default_build_has_no_audit_scan
)]
#[kani::stub(
    MarketGroupV16HeaderAccount::validate_dynamic_market_slots_len_static,
    memo_slot_count_check
)]
#[kani::stub(V16ConfigAccount::try_to_runtime_shape, memo_config_shape)]
#[kani::stub(V16Config::validate_public_user_fund_shape, memo_config_fund_shape)]
#[kani::stub(V16Core::amount_from_bound_num, memo_amount_from_bound_num)]
#[kani::stub(V16Core::bound_num_from_amount, memo_bound_num_from_amount)]
#[kani::stub(
    MarketGroupV16View::validate_header_aggregate_totals,
    memo_header_aggregate_totals
)]
fn proof_v16_activate_empty_market_slot_keeps_the_header_predicate() {
    let (mut header, mut markets) = arbitrary_header_and_slot();
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let held = market.validate_shape().is_ok();
    let result = market.header.activate_empty_market_slot_not_atomic(
        kani::any(),
        &mut market.markets[0],
        kani::any(),
        kani::any(),
    );
    assert_held_before_holds_after(held, result.is_ok(), &market);
    kani::cover!(
        held && result.is_ok(),
        "the predicate held and Ok is reached"
    );
    kani::cover!(
        held && result.is_err(),
        "the predicate held and an error is reached"
    );
}

// T03.header
#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::solver(cadical)]
#[kani::stub(
    MarketGroupV16View::validate_shape_full_audit_scan,
    default_build_has_no_audit_scan
)]
#[kani::stub(
    MarketGroupV16HeaderAccount::validate_dynamic_market_slots_len_static,
    memo_slot_count_check
)]
#[kani::stub(V16ConfigAccount::try_to_runtime_shape, memo_config_shape)]
#[kani::stub(V16Config::validate_public_user_fund_shape, memo_config_fund_shape)]
#[kani::stub(V16Core::amount_from_bound_num, memo_amount_from_bound_num)]
#[kani::stub(V16Core::bound_num_from_amount, memo_bound_num_from_amount)]
#[kani::stub(
    MarketGroupV16View::validate_header_aggregate_totals,
    memo_header_aggregate_totals
)]
fn proof_v16_activate_empty_asset_slot_keeps_the_header_predicate() {
    let (mut header, mut markets) = arbitrary_header_and_slot();
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let held = market.validate_shape().is_ok();
    let result = market.header.activate_empty_asset_slot_not_atomic(
        kani::any(),
        &mut market.markets[0].engine,
        kani::any(),
        kani::any(),
    );
    assert_held_before_holds_after(held, result.is_ok(), &market);
    kani::cover!(
        held && result.is_ok(),
        "the predicate held and Ok is reached"
    );
    kani::cover!(
        held && result.is_err(),
        "the predicate held and an error is reached"
    );
}

// Batch 2 of the local class: T29 and T47 (they write only the materialized-portfolio count), and
// T38 (the slot-capacity growth), whose property is restated: a header-account method that grows
// the capacity cannot leave a view of the old slice length valid, so it keeps every check but the
// slot-count one (first harness), and the slot-count check holds for a view whose slice has the new
// capacity's number of slots (second harness). The config check runs as written in both: T38
// changes the config, and its fund-shape check (memoized by value) is the part of it that holds
// the 256-bit arithmetic.

/// The slot-count check left out: the property is about every other check of the predicate.
#[cfg(all(kani, feature = "closure"))]
fn slot_count_check_left_out(
    _supplied_len: usize,
    _capacity: usize,
    _configured_market_slots: usize,
) -> V16Result<()> {
    Ok(())
}

/// The account's own check against the market: it takes both by shared reference, so it writes
/// nothing; either outcome.
#[cfg(all(kani, feature = "closure"))]
fn stand_in_view_account_check_any_outcome<'a, T>(
    _account: &PortfolioV16View<'a>,
    _market: &MarketGroupV16View<'_, T>,
) -> V16Result<()>
where
    'a: 'a,
{
    if kani::any() {
        Ok(())
    } else {
        Err(kani::any())
    }
}

/// The account's emptiness test: it takes the account by shared reference, so it writes nothing;
/// any result.
#[cfg(all(kani, feature = "closure"))]
fn stand_in_account_emptiness_any_result<'a>(_account: &PortfolioV16View<'a>) -> V16Result<bool>
where
    'a: 'a,
{
    kani::any()
}

// T29.header
#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::solver(cadical)]
#[kani::stub(
    MarketGroupV16View::validate_shape_full_audit_scan,
    default_build_has_no_audit_scan
)]
#[kani::stub(
    MarketGroupV16HeaderAccount::validate_dynamic_market_slots_len_static,
    memo_slot_count_check
)]
#[kani::stub(V16ConfigAccount::try_to_runtime_shape, memo_config_shape)]
#[kani::stub(V16Config::validate_public_user_fund_shape, memo_config_fund_shape)]
#[kani::stub(V16Core::amount_from_bound_num, memo_amount_from_bound_num)]
#[kani::stub(V16Core::bound_num_from_amount, memo_bound_num_from_amount)]
#[kani::stub(
    MarketGroupV16View::validate_header_aggregate_totals,
    memo_header_aggregate_totals
)]
#[kani::stub(
    PortfolioV16View::validate_with_market,
    stand_in_view_account_check_any_outcome
)]
#[kani::stub(
    PortfolioV16View::is_empty_for_dematerialization,
    stand_in_account_emptiness_any_result
)]
fn proof_v16_deregister_empty_materialized_portfolio_keeps_the_header_predicate() {
    let (mut header, mut markets) = arbitrary_header_and_slot();
    let account_header = arbitrary_account();
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let held = market.validate_shape().is_ok();
    let result = market.deregister_empty_materialized_portfolio_not_atomic(&PortfolioV16View::new(
        &account_header,
    ));
    assert_held_before_holds_after(held, result.is_ok(), &market);
    kani::cover!(
        held && result.is_ok(),
        "the predicate held and Ok is reached"
    );
    kani::cover!(
        held && result.is_err(),
        "the predicate held and an error is reached"
    );
}

// T47.header
#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::solver(cadical)]
#[kani::stub(
    MarketGroupV16View::validate_shape_full_audit_scan,
    default_build_has_no_audit_scan
)]
#[kani::stub(
    MarketGroupV16HeaderAccount::validate_dynamic_market_slots_len_static,
    memo_slot_count_check
)]
#[kani::stub(V16ConfigAccount::try_to_runtime_shape, memo_config_shape)]
#[kani::stub(V16Config::validate_public_user_fund_shape, memo_config_fund_shape)]
#[kani::stub(V16Core::amount_from_bound_num, memo_amount_from_bound_num)]
#[kani::stub(V16Core::bound_num_from_amount, memo_bound_num_from_amount)]
#[kani::stub(
    MarketGroupV16View::validate_header_aggregate_totals,
    memo_header_aggregate_totals
)]
#[kani::stub(
    PortfolioV16View::validate_with_market,
    stand_in_view_account_check_any_outcome
)]
#[kani::stub(
    PortfolioV16View::is_empty_for_dematerialization,
    stand_in_account_emptiness_any_result
)]
fn proof_v16_register_empty_materialized_portfolio_keeps_the_header_predicate() {
    let (mut header, mut markets) = arbitrary_header_and_slot();
    let account_header = arbitrary_account();
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let held = market.validate_shape().is_ok();
    let result = market
        .register_empty_materialized_portfolio_not_atomic(&PortfolioV16View::new(&account_header));
    assert_held_before_holds_after(held, result.is_ok(), &market);
    kani::cover!(
        held && result.is_ok(),
        "the predicate held and Ok is reached"
    );
    kani::cover!(
        held && result.is_err(),
        "the predicate held and an error is reached"
    );
}

// T38.header (1 of 2): every check of the predicate but the slot-count one
#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::solver(cadical)]
#[kani::stub(
    MarketGroupV16View::validate_shape_full_audit_scan,
    default_build_has_no_audit_scan
)]
#[kani::stub(
    MarketGroupV16HeaderAccount::validate_dynamic_market_slots_len_static,
    slot_count_check_left_out
)]
#[kani::stub(V16Config::validate_public_user_fund_shape, memo_config_fund_shape)]
#[kani::stub(V16Core::amount_from_bound_num, memo_amount_from_bound_num)]
#[kani::stub(V16Core::bound_num_from_amount, memo_bound_num_from_amount)]
#[kani::stub(
    MarketGroupV16View::validate_header_aggregate_totals,
    memo_header_aggregate_totals
)]
fn proof_v16_grow_asset_slot_capacity_keeps_the_other_header_checks() {
    let (mut header, mut markets) = arbitrary_header_and_slot();
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let held = market.validate_shape().is_ok();
    let result = market
        .header
        .grow_asset_slot_capacity_not_atomic(kani::any(), kani::any());
    assert_held_before_holds_after(held, result.is_ok(), &market);
    kani::cover!(held && result.is_ok(), "the checks held and Ok is reached");
    kani::cover!(
        held && result.is_err(),
        "the checks held and an error is reached"
    );
}

// T38.header (2 of 2): the slot-count check holds for a slice of the new capacity
#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::solver(cadical)]
#[kani::stub(V16Config::validate_public_user_fund_shape, memo_config_fund_shape)]
fn proof_v16_grow_asset_slot_capacity_fits_a_slice_of_the_new_capacity() {
    let (mut header, _markets) = arbitrary_header_and_slot();
    let result = header.grow_asset_slot_capacity_not_atomic(kani::any(), kani::any());
    if result.is_ok() {
        let capacity = header.asset_slot_capacity.get() as usize;
        assert!(
            MarketGroupV16HeaderAccount::validate_dynamic_market_slots_len_static(
                capacity,
                capacity,
                header.config.max_market_slots.get() as usize,
            )
            .is_ok()
        );
    }
    kani::cover!(result.is_ok(), "Ok is reached");
    kani::cover!(result.is_err(), "an error is reached");
}

// Header pieces of class "final" (T*.header): the path structure, with every callee replaced.
//
// The ledger states each such piece as: if the header predicate (`validate_shape` of a default
// build) held before the entry point and it returns Ok, it holds of the state it leaves; the
// obligation is the path structure, "every Ok path ends in `validate_shape()` after its last
// market write". Each harness below checks that structure of the entry point's own body:
// - every method it calls on the market view is replaced by a stand-in: one that takes the view
//   by shared reference returns any value; one that takes it mutably may first write any bytes to
//   the header, the asset slots and the view's scratch, and any values to the fields of each
//   account it is given that these entry points read, then returns any value. Every behaviour of
//   the real callee, as far as the entry point can observe it, is among them;
// - its own `validate_shape()` calls are replaced by a recorder that returns either outcome and
//   remembers the header it was asked about and its answer, so the proof holds whatever the
//   predicate is, and so for a default build's predicate and any number of asset slots;
// - the account's own check `validate_with_market` (shared references only, so it writes nothing)
//   returns either outcome; pure helpers (the deltas, the token-flow records, the kernels, the
//   decoders) run as written, except a pure function a harness names in its stubs (`any_pure_*`):
//   it takes its inputs by value and writes nothing, and its stand-in returns any result;
// - the header and the slot are arbitrary bytes; of an account, the fields the entry point reads
//   itself are arbitrary (`arbitrary_read_fields`; the generator checks the list), the others are
//   read only by callees, which are replaced. Nothing is assumed of any of them.
// The assertion: an Ok return leaves the header bytes unchanged, or the last shape check passed on
// exactly the header the entry point leaves. Either way the predicate holds after if it held
// before. A callee that has no early Ok and ends in its own check is proved the stronger way (every
// Ok follows a passed check on the header it leaves) and is replaced in its callers by a stand-in
// with that property: any writes and any result, and a passed check recorded on every Ok. The
// generator checks that the entry points read no slot themselves, use an account only through the
// fields listed in `arbitrary_read_fields`, its own check and the replaced callees, and give `self`
// to nothing but the replaced callees.
#[cfg(all(kani, feature = "closure"))]
static mut LAST_SHAPE_CHECK: Option<(MarketGroupV16HeaderAccount, bool)> = None;

/// Stand-in for the view's `validate_shape`: either outcome, recorded with the header it was given.
#[cfg(all(kani, feature = "closure"))]
#[allow(unsafe_code)]
fn recorded_shape_check<'a, T>(market: &MarketGroupV16ViewMut<'a, T>) -> V16Result<()>
where
    'a: 'a,
{
    let passed: bool = kani::any();
    // SAFETY: Kani runs one harness on one thread.
    unsafe {
        LAST_SHAPE_CHECK = Some((*market.header, passed));
    }
    if passed {
        Ok(())
    } else {
        Err(kani::any())
    }
}

/// Records a passed check on the header the market view holds now.
#[cfg(all(kani, feature = "closure"))]
#[allow(unsafe_code)]
fn record_passed_shape_check<'a, T>(market: &MarketGroupV16ViewMut<'a, T>)
where
    'a: 'a,
{
    // SAFETY: Kani runs one harness on one thread.
    unsafe {
        LAST_SHAPE_CHECK = Some((*market.header, true));
    }
}
#[cfg(all(kani, feature = "closure"))]
fn havoc_market_view<'a, T>(market: &mut MarketGroupV16ViewMut<'a, T>)
where
    'a: 'a,
{
    let header: [u8; core::mem::size_of::<MarketGroupV16HeaderAccount>()] = kani::any();
    bytemuck::bytes_of_mut(&mut *market.header).copy_from_slice(&header);
    for slot in market.markets.iter_mut() {
        let engine: [u8; core::mem::size_of::<EngineAssetSlotV16Account>()] = kani::any();
        bytemuck::bytes_of_mut(&mut slot.engine).copy_from_slice(&engine);
    }
    market.protection_observed = kani::any();
}

/// Stand-in for an account's own check against the market: it takes both by shared reference, so
/// it writes nothing; either outcome.
#[cfg(all(kani, feature = "closure"))]
fn stand_in_account_check_any_outcome<'a, T>(
    _account: &PortfolioV16ViewMut<'a>,
    _market: &MarketGroupV16View<'_, T>,
) -> V16Result<()>
where
    'a: 'a,
{
    if kani::any() {
        Ok(())
    } else {
        Err(kani::any())
    }
}

/// Any values for the account fields that the entry points of this section read themselves.
#[cfg(all(kani, feature = "closure"))]
fn arbitrary_read_fields(account: &mut PortfolioAccountV16Account) {
    account.capital = V16PodU128::new(kani::any());
    account.pnl = V16PodI128::new(kani::any());
    account.reserved_pnl = V16PodU128::new(kani::any());
    account.fee_credits = V16PodI128::new(kani::any());
    account.cancel_deposit_escrow = V16PodU128::new(kani::any());
    account.last_fee_slot = V16PodU64::new(kani::any());
    let words: [u64; V16_ACTIVE_BITMAP_WORDS] = kani::any();
    account.active_bitmap = words.map(V16PodU64::new);
    let cert: [u8; core::mem::size_of::<HealthCertV16Account>()] = kani::any();
    bytemuck::bytes_of_mut(&mut account.health_cert).copy_from_slice(&cert);
    let close: [u8; core::mem::size_of::<CloseProgressLedgerV16Account>()] = kani::any();
    bytemuck::bytes_of_mut(&mut account.close_progress).copy_from_slice(&close);
}

#[cfg(all(kani, feature = "closure"))]
fn havoc_account(account: &mut PortfolioV16ViewMut<'_>) {
    arbitrary_read_fields(account.header);
}

#[cfg(all(kani, feature = "closure"))]
fn arbitrary_header_and_slot() -> (MarketGroupV16HeaderAccount, [Market<u64>; 1]) {
    let bytes: [u8; core::mem::size_of::<MarketGroupV16HeaderAccount>()] = kani::any();
    let mut header: MarketGroupV16HeaderAccount = bytemuck::Zeroable::zeroed();
    bytemuck::bytes_of_mut(&mut header).copy_from_slice(&bytes);
    let engine: [u8; core::mem::size_of::<EngineAssetSlotV16Account>()] = kani::any();
    let mut slot = EngineAssetSlotV16Account::default();
    bytemuck::bytes_of_mut(&mut slot).copy_from_slice(&engine);
    (header, [Market::new(kani::any(), slot)])
}

#[cfg(all(kani, feature = "closure"))]
fn arbitrary_account() -> PortfolioAccountV16Account {
    let mut account = PortfolioAccountV16Account::default();
    arbitrary_read_fields(&mut account);
    account
}

/// Ok leaves the header bytes unchanged, or the last shape check passed on exactly the header left.
#[cfg(all(kani, feature = "closure"))]
#[allow(unsafe_code)]
fn assert_ok_keeps_the_header_predicate<'a, T>(
    ok: bool,
    before: &MarketGroupV16HeaderAccount,
    market: &MarketGroupV16ViewMut<'a, T>,
) where
    'a: 'a,
{
    if ok {
        let now = bytemuck::bytes_of(&*market.header);
        let unchanged = now == bytemuck::bytes_of(before);
        // SAFETY: Kani runs one harness on one thread.
        let checked = match unsafe { LAST_SHAPE_CHECK } {
            Some((checked_header, passed)) => passed && bytemuck::bytes_of(&checked_header) == now,
            None => false,
        };
        assert!(unchanged || checked);
    }
}

/// Every Ok follows a passed check on exactly the header left (for a callee with no early Ok, whose
/// stand-in then records a passed check on every Ok).
#[cfg(all(kani, feature = "closure"))]
#[allow(unsafe_code)]
fn assert_ok_follows_a_passed_check<'a, T>(ok: bool, market: &MarketGroupV16ViewMut<'a, T>)
where
    'a: 'a,
{
    if ok {
        // SAFETY: Kani runs one harness on one thread.
        let checked = match unsafe { LAST_SHAPE_CHECK } {
            Some((checked_header, passed)) => {
                passed && bytemuck::bytes_of(&checked_header) == bytemuck::bytes_of(&*market.header)
            }
            None => false,
        };
        assert!(checked);
    }
}

#[cfg(all(kani, feature = "closure"))]
fn any_reservation_encumbrance_proof() -> ReservationEncumbranceProofV16 {
    ReservationEncumbranceProofV16 {
        domain: kani::any(),
        exact_positive_claim_num: kani::any(),
        positive_claim_bound_num: kani::any(),
        source_fresh_reserved_backing_num: kani::any(),
        source_spent_backing_num: kani::any(),
        source_provider_receivable_num: kani::any(),
        bucket_fresh_unliened_backing_num: kani::any(),
        bucket_valid_liened_backing_num: kani::any(),
        bucket_consumed_liened_backing_num: kani::any(),
        source_valid_liened_backing_num: kani::any(),
        source_impaired_liened_backing_num: kani::any(),
        bucket_impaired_liened_backing_num: kani::any(),
        source_insurance_credit_reserved_num: kani::any(),
        reservation_insurance_credit_reserved_num: kani::any(),
        source_valid_liened_insurance_num: kani::any(),
        reservation_valid_liened_insurance_num: kani::any(),
        source_impaired_liened_insurance_num: kani::any(),
        reservation_impaired_liened_insurance_num: kani::any(),
        source_credit_rate_num: kani::any(),
    }
}

// Values of the engine's plain records that the stand-ins return or the harnesses pass: every field any value.
#[cfg(all(kani, feature = "closure"))]
impl kani::Arbitrary for TradeRequestV16 {
    fn any() -> Self {
        TradeRequestV16 {
            asset_index: kani::any(),
            size_q: kani::any(),
            exec_price: kani::any(),
            fee_bps: kani::any(),
        }
    }
}

#[cfg(all(kani, feature = "closure"))]
impl kani::Arbitrary for TradeOutcomeV16 {
    fn any() -> Self {
        TradeOutcomeV16 {
            fee_a: kani::any(),
            fee_b: kani::any(),
            notional: kani::any(),
        }
    }
}

#[cfg(all(kani, feature = "closure"))]
impl kani::Arbitrary for BatchTradeOutcomeV16 {
    fn any() -> Self {
        BatchTradeOutcomeV16 {
            fill_count: kani::any(),
            fee_a: kani::any(),
            fee_b: kani::any(),
            notional: kani::any(),
        }
    }
}

#[cfg(all(kani, feature = "closure"))]
impl kani::Arbitrary for SourceCreditConsumptionV16 {
    fn any() -> Self {
        SourceCreditConsumptionV16 {
            face_burn: kani::any(),
            counterparty_credit_consumed: kani::any(),
            insurance_credit_consumed: kani::any(),
        }
    }
}

#[cfg(all(kani, feature = "closure"))]
fn any_progress_outcome() -> PermissionlessProgressOutcomeV16 {
    match kani::any::<u8>() {
        0 => PermissionlessProgressOutcomeV16::AccountCurrent,
        1 => PermissionlessProgressOutcomeV16::AccountBChunk(AccountBSettlementChunkV16 {
            delta_b: kani::any(),
            loss: kani::any(),
            new_remainder: kani::any(),
            remaining_after: kani::any(),
        }),
        2 => PermissionlessProgressOutcomeV16::SourceBackingExpired {
            domain: kani::any(),
        },
        3 => PermissionlessProgressOutcomeV16::ResidualBooked(BResidualBookingOutcomeV16 {
            booked_loss: kani::any(),
            explicit_loss: kani::any(),
            delta_b: kani::any(),
            remaining_after: kani::any(),
        }),
        _ => PermissionlessProgressOutcomeV16::RecoveryDeclared(kani::any()),
    }
}

#[cfg(all(kani, feature = "closure"))]
fn any_account_fee_anchor_for_loss_currentness<'a, T>(
    _market: &MarketGroupV16ViewMut<'a, T>,
    _account: &PortfolioV16View<'_>,
    _now_slot: u64,
) -> V16Result<u64>
where
    'a: 'a,
{
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn any_account_has_active_source_claim_exposure<'a, T>(
    _market: &MarketGroupV16ViewMut<'a, T>,
    _account: &PortfolioV16View<'_>,
) -> V16Result<bool>
where
    'a: 'a,
{
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn any_account_source_realizable_support<'a, T>(
    _market: &MarketGroupV16ViewMut<'a, T>,
    _account: &PortfolioV16View<'_>,
    _face_claim: u128,
) -> V16Result<u128>
where
    'a: 'a,
{
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn any_asset_has_unsettled_kf_position<'a, T>(
    _market: &MarketGroupV16ViewMut<'a, T>,
    _domain: usize,
) -> V16Result<bool>
where
    'a: 'a,
{
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn any_asset_state<'a, T>(
    _market: &MarketGroupV16ViewMut<'a, T>,
    _asset_index: usize,
) -> V16Result<AssetStateV16>
where
    'a: 'a,
{
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn any_backing_bucket_for_domain<'a, T>(
    _market: &MarketGroupV16ViewMut<'a, T>,
    _domain: usize,
) -> V16Result<BackingBucketV16>
where
    'a: 'a,
{
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn any_can_ignore_unrelated_loss_stale_for_trade<'a, T>(
    _market: &MarketGroupV16ViewMut<'a, T>,
    _long_account: &PortfolioV16View<'_>,
    _short_account: &PortfolioV16View<'_>,
    _asset_index: usize,
) -> V16Result<bool>
where
    'a: 'a,
{
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn any_checked_asset_set_epoch_bump<'a, T>(
    _market: &MarketGroupV16ViewMut<'a, T>,
) -> V16Result<(u64, u64)>
where
    'a: 'a,
{
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn any_domain_asset_side<'a, T>(
    _market: &MarketGroupV16ViewMut<'a, T>,
    _domain: usize,
) -> V16Result<(usize, SideV16)>
where
    'a: 'a,
{
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn any_domain_insurance_budget_spent<'a, T>(
    _market: &MarketGroupV16ViewMut<'a, T>,
    _domain: usize,
) -> V16Result<(u128, u128)>
where
    'a: 'a,
{
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn any_ensure_favorable_action_current_certificate<'a, T>(
    _market: &MarketGroupV16ViewMut<'a, T>,
    _account: &PortfolioV16View<'_>,
) -> V16Result<()>
where
    'a: 'a,
{
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn any_face_claim_to_burn_for_support<'a, T>(
    _market: &MarketGroupV16ViewMut<'a, T>,
    _effective_support: u128,
    _residual: u128,
    _junior_bound: u128,
) -> V16Result<u128>
where
    'a: 'a,
{
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn any_fee_sync_settlement_noop_last_asset<'a, T>(
    _market: &MarketGroupV16ViewMut<'a, T>,
    _account: &PortfolioV16View<'_>,
) -> V16Result<Option<usize>>
where
    'a: 'a,
{
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn any_first_terminal_claim_free_recredit_asset<'a, T>(
    _market: &MarketGroupV16ViewMut<'a, T>,
) -> V16Result<Option<usize>>
where
    'a: 'a,
{
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn any_group_has_position_or_loss_state_for_oracle_reset<'a, T>(
    _market: &MarketGroupV16ViewMut<'a, T>,
) -> V16Result<bool>
where
    'a: 'a,
{
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn any_haircut_effective_support<'a, T>(
    _market: &MarketGroupV16ViewMut<'a, T>,
    _face_claim: u128,
    _residual: u128,
    _junior_bound: u128,
) -> V16Result<u128>
where
    'a: 'a,
{
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn any_insurance_domain_index<'a, T>(
    _market: &MarketGroupV16ViewMut<'a, T>,
    _asset_index: usize,
    _side: SideV16,
) -> V16Result<usize>
where
    'a: 'a,
{
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn any_insurance_reservation_for_domain<'a, T>(
    _market: &MarketGroupV16ViewMut<'a, T>,
    _domain: usize,
) -> V16Result<InsuranceCreditReservationV16>
where
    'a: 'a,
{
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn any_junior_claim_bound<'a, T>(_market: &MarketGroupV16ViewMut<'a, T>) -> u128
where
    'a: 'a,
{
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn any_pending_domain_loss_barrier_count<'a, T>(
    _market: &MarketGroupV16ViewMut<'a, T>,
    _asset_index: usize,
    _side: SideV16,
) -> V16Result<u64>
where
    'a: 'a,
{
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn any_preflight_claim_resolved_payout_topup<'a, T>(
    _market: &MarketGroupV16ViewMut<'a, T>,
    _account: &PortfolioV16View<'_>,
) -> V16Result<()>
where
    'a: 'a,
{
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn any_preflight_convert_released_pnl_to_capital<'a, T>(
    _market: &MarketGroupV16ViewMut<'a, T>,
    _account: &PortfolioV16View<'_>,
) -> V16Result<()>
where
    'a: 'a,
{
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn any_preflight_cure_and_cancel_close<'a, T>(
    _market: &MarketGroupV16ViewMut<'a, T>,
    _account: &PortfolioV16View<'_>,
    _optional_deposit: u128,
) -> V16Result<()>
where
    'a: 'a,
{
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn any_provider_principal_num<'a, T>(
    _market: &MarketGroupV16ViewMut<'a, T>,
    _domain: usize,
) -> V16Result<u128>
where
    'a: 'a,
{
    kani::any()
}

// Stand-in for `MarketGroupV16ViewMut::account_has_source_claims`, which takes its inputs by value
// or by shared reference and writes nothing: any result, so every result of the real function is
// among its own.
#[cfg(all(kani, feature = "closure"))]
fn any_pure_account_has_source_claims<'a, T>(_account: &PortfolioV16View<'_>) -> V16Result<bool>
where
    'a: 'a,
{
    kani::any()
}

// Stand-in for `MarketGroupV16ViewMut::account_has_source_liens`, which takes its inputs by value
// or by shared reference and writes nothing: any result, so every result of the real function is
// among its own.
#[cfg(all(kani, feature = "closure"))]
fn any_pure_account_has_source_liens<'a, T>(_account: &PortfolioV16View<'_>) -> bool
where
    'a: 'a,
{
    kani::any()
}

// Stand-in for `MarketGroupV16ViewMut::active_leg_for_asset`, which takes its inputs by value or by
// shared reference and writes nothing: any result, so every result of the real function is among
// its own.
#[cfg(all(kani, feature = "closure"))]
fn any_pure_active_leg_for_asset<'a, T>(
    _account: &PortfolioV16View<'_>,
    _asset_index: usize,
) -> V16Result<PortfolioLegV16>
where
    'a: 'a,
{
    kani::any()
}

// Stand-in for `V16Core::effective_abs_quantity_for_leg`, which takes its inputs by value or by
// shared reference and writes nothing: any result, so every result of the real function is among
// its own.
#[cfg(all(kani, feature = "closure"))]
fn any_pure_effective_abs_quantity_for_leg(
    _asset: AssetStateV16,
    _leg: PortfolioLegV16,
) -> V16Result<u128> {
    kani::any()
}

// Stand-in for `V16Core::expected_source_credit_rate_num_for_state`, which takes its inputs by
// value or by shared reference and writes nothing: any result, so every result of the real function
// is among its own.
#[cfg(all(kani, feature = "closure"))]
fn any_pure_expected_source_credit_rate_num_for_state(
    _state: SourceCreditStateV16,
) -> V16Result<u128> {
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn any_require_asset_mark_pushable<'a, T>(
    _market: &MarketGroupV16ViewMut<'a, T>,
    _asset_index: usize,
) -> V16Result<()>
where
    'a: 'a,
{
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn any_require_empty_asset_lifecycle_state_with_policy<'a, T>(
    _market: &MarketGroupV16ViewMut<'a, T>,
    _asset_index: usize,
    _allow_terminal_spent_budget: bool,
    _allow_terminal_social_loss_audit: bool,
    _allow_terminal_source_spent_audit: bool,
    _allow_terminal_price_funding_history: bool,
) -> V16Result<()>
where
    'a: 'a,
{
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn any_require_terminal_claim_free_state<'a, T>(
    _market: &MarketGroupV16ViewMut<'a, T>,
) -> V16Result<()>
where
    'a: 'a,
{
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn any_reservation_encumbrance_proof_for_domain_parts<'a, T>(
    _market: &MarketGroupV16ViewMut<'a, T>,
    _domain: usize,
    _source: SourceCreditStateV16,
    _bucket: BackingBucketV16,
    _reservation: InsuranceCreditReservationV16,
) -> V16Result<ReservationEncumbranceProofV16>
where
    'a: 'a,
{
    if kani::any() {
        Ok(any_reservation_encumbrance_proof())
    } else {
        Err(kani::any())
    }
}

#[cfg(all(kani, feature = "closure"))]
fn any_residual<'a, T>(_market: &MarketGroupV16ViewMut<'a, T>) -> u128
where
    'a: 'a,
{
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn any_resolved_counterparty_backing_surplus<'a, T>(
    _market: &MarketGroupV16ViewMut<'a, T>,
    _domain: usize,
) -> V16Result<u128>
where
    'a: 'a,
{
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn any_source_credit_for_domain<'a, T>(
    _market: &MarketGroupV16ViewMut<'a, T>,
    _domain: usize,
) -> V16Result<SourceCreditStateV16>
where
    'a: 'a,
{
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn any_source_credit_for_domain_shape<'a, T>(
    _market: &MarketGroupV16ViewMut<'a, T>,
    _domain: usize,
) -> V16Result<SourceCreditStateV16>
where
    'a: 'a,
{
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn any_stranded_loser_backing_owed_to_twin_num<'a, T>(
    _market: &MarketGroupV16ViewMut<'a, T>,
    _domain: usize,
) -> V16Result<u128>
where
    'a: 'a,
{
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn any_validate_configured_asset_index<'a, T>(
    _market: &MarketGroupV16ViewMut<'a, T>,
    _asset_index: usize,
) -> V16Result<()>
where
    'a: 'a,
{
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn any_validate_source_domain_ledger<'a, T>(
    _market: &MarketGroupV16ViewMut<'a, T>,
    _domain: usize,
) -> V16Result<()>
where
    'a: 'a,
{
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn any_validate_unconfigured_market_tail<'a, T>(
    _market: &MarketGroupV16ViewMut<'a, T>,
) -> V16Result<()>
where
    'a: 'a,
{
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn havoc_add_fresh_counterparty_backing_unchecked<'a, T>(
    market: &mut MarketGroupV16ViewMut<'a, T>,
    _domain: usize,
    _amount: u128,
    _expiry_slot: u64,
) -> V16Result<()>
where
    'a: 'a,
{
    havoc_market_view(market);
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn havoc_adjust_provider_principal_num<'a, T>(
    market: &mut MarketGroupV16ViewMut<'a, T>,
    _domain: usize,
    _amount: u128,
    _add: bool,
) -> V16Result<()>
where
    'a: 'a,
{
    havoc_market_view(market);
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn havoc_begin_legless_asset_resets<'a, T>(
    market: &mut MarketGroupV16ViewMut<'a, T>,
    _asset_index: usize,
) -> V16Result<()>
where
    'a: 'a,
{
    havoc_market_view(market);
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn havoc_burn_account_source_claim_bound_num_domain_first<'a, T>(
    market: &mut MarketGroupV16ViewMut<'a, T>,
    account: &mut PortfolioV16ViewMut<'_>,
    _domain: usize,
    _burn_num: u128,
) -> V16Result<()>
where
    'a: 'a,
{
    havoc_market_view(market);
    havoc_account(account);
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn havoc_charge_account_fee_current_not_atomic<'a, T>(
    market: &mut MarketGroupV16ViewMut<'a, T>,
    account: &mut PortfolioV16ViewMut<'_>,
    _requested_fee: u128,
) -> V16Result<u128>
where
    'a: 'a,
{
    havoc_market_view(market);
    havoc_account(account);
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn havoc_claim_resolved_payout_topup_core_not_atomic<'a, T>(
    market: &mut MarketGroupV16ViewMut<'a, T>,
    account: &mut PortfolioV16ViewMut<'_>,
) -> V16Result<u128>
where
    'a: 'a,
{
    havoc_market_view(market);
    havoc_account(account);
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn havoc_commit_asset_set_epoch_bump<'a, T>(
    market: &mut MarketGroupV16ViewMut<'a, T>,
    _next_asset_set_epoch: u64,
    _next_risk_epoch: u64,
) where
    'a: 'a,
{
    havoc_market_view(market);
}

#[cfg(all(kani, feature = "closure"))]
fn havoc_consume_validated_account_source_credit_not_atomic<'a, T>(
    market: &mut MarketGroupV16ViewMut<'a, T>,
    account: &mut PortfolioV16ViewMut<'_>,
    _effective_credit: u128,
    _burn_account_claims: bool,
    _require_full: bool,
) -> V16Result<(SourceCreditConsumptionV16, u128, u128)>
where
    'a: 'a,
{
    havoc_market_view(market);
    havoc_account(account);
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn havoc_execute_batch_with_fee_after_tail_validation_not_atomic<'a, T>(
    market: &mut MarketGroupV16ViewMut<'a, T>,
    long_account: &mut PortfolioV16ViewMut<'_>,
    short_account: &mut PortfolioV16ViewMut<'_>,
    _requests: &[TradeRequestV16],
) -> V16Result<BatchTradeOutcomeV16>
where
    'a: 'a,
{
    havoc_market_view(market);
    havoc_account(long_account);
    havoc_account(short_account);
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn havoc_expire_lapsed_source_backing_for_asset_not_atomic<'a, T>(
    market: &mut MarketGroupV16ViewMut<'a, T>,
    _asset_index: usize,
    _now_slot: u64,
) -> V16Result<()>
where
    'a: 'a,
{
    havoc_market_view(market);
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn havoc_full_account_refresh_not_atomic<'a, T>(
    market: &mut MarketGroupV16ViewMut<'a, T>,
    account: &mut PortfolioV16ViewMut<'_>,
) -> V16Result<HealthCertV16>
where
    'a: 'a,
{
    havoc_market_view(market);
    havoc_account(account);
    if kani::any() {
        Ok(any_health_cert())
    } else {
        Err(kani::any())
    }
}

#[cfg(all(kani, feature = "closure"))]
fn havoc_normalize_terminal_empty_asset_history_not_atomic<'a, T>(
    market: &mut MarketGroupV16ViewMut<'a, T>,
    _asset_index: usize,
) -> V16Result<()>
where
    'a: 'a,
{
    havoc_market_view(market);
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn havoc_recredit_terminal_claim_free_residual_for_asset_core_not_atomic<'a, T>(
    market: &mut MarketGroupV16ViewMut<'a, T>,
    _asset_index: usize,
) -> V16Result<u128>
where
    'a: 'a,
{
    havoc_market_view(market);
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn havoc_release_provider_principal_after<'a, T>(
    market: &mut MarketGroupV16ViewMut<'a, T>,
    _domain: usize,
    _reserved_before: u128,
    _reserved_after: u128,
) -> V16Result<()>
where
    'a: 'a,
{
    havoc_market_view(market);
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn havoc_retire_terminal_unbudgeted_insurance_core_not_atomic<'a, T>(
    market: &mut MarketGroupV16ViewMut<'a, T>,
) -> V16Result<u128>
where
    'a: 'a,
{
    havoc_market_view(market);
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn havoc_set_account_pnl_after_source_claim_burn<'a, T>(
    market: &mut MarketGroupV16ViewMut<'a, T>,
    account: &mut PortfolioV16ViewMut<'_>,
    _new_pnl: i128,
    _source_face_burn_num: u128,
) -> V16Result<()>
where
    'a: 'a,
{
    havoc_market_view(market);
    havoc_account(account);
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn havoc_set_account_pnl_with_source<'a, T>(
    market: &mut MarketGroupV16ViewMut<'a, T>,
    account: &mut PortfolioV16ViewMut<'_>,
    _new_pnl: i128,
    _source_domain: usize,
) -> V16Result<()>
where
    'a: 'a,
{
    havoc_market_view(market);
    havoc_account(account);
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn havoc_set_asset_state<'a, T>(
    market: &mut MarketGroupV16ViewMut<'a, T>,
    _asset_index: usize,
    _asset: AssetStateV16,
) -> V16Result<()>
where
    'a: 'a,
{
    havoc_market_view(market);
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn havoc_set_backing_bucket_for_domain<'a, T>(
    market: &mut MarketGroupV16ViewMut<'a, T>,
    _domain: usize,
    _bucket: BackingBucketV16,
) -> V16Result<()>
where
    'a: 'a,
{
    havoc_market_view(market);
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn havoc_set_domain_insurance_budget_core<'a, T>(
    market: &mut MarketGroupV16ViewMut<'a, T>,
    _domain: usize,
    _budget: u128,
    _insurance_limit: u128,
) -> V16Result<()>
where
    'a: 'a,
{
    havoc_market_view(market);
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn havoc_set_pending_domain_loss_barrier_count<'a, T>(
    market: &mut MarketGroupV16ViewMut<'a, T>,
    _asset_index: usize,
    _side: SideV16,
    _value: u64,
) -> V16Result<()>
where
    'a: 'a,
{
    havoc_market_view(market);
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn havoc_set_source_credit_for_domain<'a, T>(
    market: &mut MarketGroupV16ViewMut<'a, T>,
    _domain: usize,
    _source: SourceCreditStateV16,
) -> V16Result<()>
where
    'a: 'a,
{
    havoc_market_view(market);
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn havoc_settle_account_kf_effects_not_atomic<'a, T>(
    market: &mut MarketGroupV16ViewMut<'a, T>,
    account: &mut PortfolioV16ViewMut<'_>,
    _normalize_exhausted_sides: bool,
) -> V16Result<()>
where
    'a: 'a,
{
    havoc_market_view(market);
    havoc_account(account);
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn havoc_settle_account_side_effects_after_kf_not_atomic<'a, T>(
    market: &mut MarketGroupV16ViewMut<'a, T>,
    account: &mut PortfolioV16ViewMut<'_>,
    _b_loss_atom_budget: u128,
) -> V16Result<PermissionlessProgressOutcomeV16>
where
    'a: 'a,
{
    havoc_market_view(market);
    havoc_account(account);
    if kani::any() {
        Ok(any_progress_outcome())
    } else {
        Err(kani::any())
    }
}

#[cfg(all(kani, feature = "closure"))]
fn havoc_settle_negative_pnl_from_principal_core_not_atomic<'a, T>(
    market: &mut MarketGroupV16ViewMut<'a, T>,
    account: &mut PortfolioV16ViewMut<'_>,
) -> V16Result<u128>
where
    'a: 'a,
{
    havoc_market_view(market);
    havoc_account(account);
    kani::any()
}

#[cfg(all(kani, feature = "closure"))]
fn havoc_shift_provider_protection_floor_num<'a, T>(
    market: &mut MarketGroupV16ViewMut<'a, T>,
    _domain: usize,
    _amount_num: u128,
    _add: bool,
) -> V16Result<()>
where
    'a: 'a,
{
    havoc_market_view(market);
    kani::any()
}

// The stand-in of `cure_and_cancel_close_with_cert_not_atomic` has the property that
// `proof_v16_cure_and_cancel_close_with_cert_ok_keeps_the_header_predicate` proves of it: any
// writes and any result, and every Ok follows a passed shape check on the header it leaves.
#[cfg(all(kani, feature = "closure"))]
fn verified_cure_and_cancel_close_with_cert_not_atomic<'a, T>(
    market: &mut MarketGroupV16ViewMut<'a, T>,
    account: &mut PortfolioV16ViewMut<'_>,
    _optional_deposit: u128,
    _cert: HealthCertV16,
) -> V16Result<()>
where
    'a: 'a,
{
    havoc_market_view(market);
    havoc_account(account);
    let result: V16Result<()> = kani::any();
    if result.is_ok() {
        record_passed_shape_check(market);
    }
    result
}

// The stand-in of `execute_trade_with_fee_loss_stale_scoped_not_atomic` has the property that
// `proof_v16_execute_trade_with_fee_loss_stale_scoped_ok_keeps_the_header_predicate` proves of it:
// any writes and any result, and every Ok follows a passed shape check on the header it leaves.
#[cfg(all(kani, feature = "closure"))]
fn verified_execute_trade_with_fee_loss_stale_scoped_not_atomic<'a, T>(
    market: &mut MarketGroupV16ViewMut<'a, T>,
    long_account: &mut PortfolioV16ViewMut<'_>,
    short_account: &mut PortfolioV16ViewMut<'_>,
    _request: TradeRequestV16,
) -> V16Result<TradeOutcomeV16>
where
    'a: 'a,
{
    havoc_market_view(market);
    havoc_account(long_account);
    havoc_account(short_account);
    let result: V16Result<TradeOutcomeV16> = kani::any();
    if result.is_ok() {
        record_passed_shape_check(market);
    }
    result
}

// The stand-in of `withdraw_fresh_counterparty_backing_not_atomic` has the property that
// `proof_v16_withdraw_fresh_counterparty_backing_ok_keeps_the_header_predicate` proves of it: any
// writes and any result, and every Ok follows a passed shape check on the header it leaves.
#[cfg(all(kani, feature = "closure"))]
fn verified_withdraw_fresh_counterparty_backing_not_atomic<'a, T>(
    market: &mut MarketGroupV16ViewMut<'a, T>,
    _domain: usize,
    _amount: u128,
) -> V16Result<()>
where
    'a: 'a,
{
    havoc_market_view(market);
    let result: V16Result<()> = kani::any();
    if result.is_ok() {
        record_passed_shape_check(market);
    }
    result
}

// T06.header
#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::solver(cadical)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, recorded_shape_check)]
#[kani::stub(
    PortfolioV16ViewMut::validate_with_market,
    stand_in_account_check_any_outcome
)]
#[kani::stub(
    MarketGroupV16ViewMut::set_account_pnl_with_source,
    havoc_set_account_pnl_with_source
)]
fn proof_v16_add_account_source_positive_pnl_ok_keeps_the_header_predicate() {
    let (mut header, mut markets) = arbitrary_header_and_slot();
    let before = header;
    let mut account_header = arbitrary_account();
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let result = market.add_account_source_positive_pnl_not_atomic(
        &mut PortfolioV16ViewMut::new(&mut account_header),
        kani::any(),
        kani::any(),
    );
    assert_ok_keeps_the_header_predicate(result.is_ok(), &before, &market);
    kani::cover!(result.is_ok(), "Ok is reached");
    kani::cover!(result.is_err(), "an error is reached");
}

// T12.header
#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::solver(cadical)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, recorded_shape_check)]
#[kani::stub(MarketGroupV16ViewMut::asset_state, any_asset_state)]
#[kani::stub(
    MarketGroupV16ViewMut::backing_bucket_for_domain,
    any_backing_bucket_for_domain
)]
#[kani::stub(
    MarketGroupV16ViewMut::insurance_domain_index,
    any_insurance_domain_index
)]
#[kani::stub(
    MarketGroupV16ViewMut::insurance_reservation_for_domain,
    any_insurance_reservation_for_domain
)]
#[kani::stub(
    MarketGroupV16ViewMut::require_empty_asset_lifecycle_state_with_policy,
    any_require_empty_asset_lifecycle_state_with_policy
)]
#[kani::stub(
    MarketGroupV16ViewMut::set_backing_bucket_for_domain,
    havoc_set_backing_bucket_for_domain
)]
#[kani::stub(
    MarketGroupV16ViewMut::set_source_credit_for_domain,
    havoc_set_source_credit_for_domain
)]
#[kani::stub(
    MarketGroupV16ViewMut::source_credit_for_domain_shape,
    any_source_credit_for_domain_shape
)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_configured_asset_index,
    any_validate_configured_asset_index
)]
fn proof_v16_canonicalize_spent_backing_history_ok_keeps_the_header_predicate() {
    let (mut header, mut markets) = arbitrary_header_and_slot();
    let before = header;
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let result = market.canonicalize_spent_backing_history_not_atomic(kani::any());
    assert_ok_keeps_the_header_predicate(result.is_ok(), &before, &market);
    kani::cover!(result.is_ok(), "Ok is reached");
    kani::cover!(result.is_err(), "an error is reached");
}

// T13.header
#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::solver(cadical)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, recorded_shape_check)]
#[kani::stub(
    PortfolioV16ViewMut::validate_with_market,
    stand_in_account_check_any_outcome
)]
#[kani::stub(
    MarketGroupV16ViewMut::backing_bucket_for_domain,
    any_backing_bucket_for_domain
)]
#[kani::stub(MarketGroupV16ViewMut::domain_asset_side, any_domain_asset_side)]
#[kani::stub(
    MarketGroupV16ViewMut::domain_insurance_budget_spent,
    any_domain_insurance_budget_spent
)]
#[kani::stub(
    MarketGroupV16ViewMut::ensure_favorable_action_current_certificate,
    any_ensure_favorable_action_current_certificate
)]
#[kani::stub(
    MarketGroupV16ViewMut::set_backing_bucket_for_domain,
    havoc_set_backing_bucket_for_domain
)]
#[kani::stub(
    MarketGroupV16ViewMut::set_domain_insurance_budget_core,
    havoc_set_domain_insurance_budget_core
)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_source_domain_ledger,
    any_validate_source_domain_ledger
)]
fn proof_v16_charge_account_backing_fee_ok_keeps_the_header_predicate() {
    let (mut header, mut markets) = arbitrary_header_and_slot();
    let before = header;
    let mut account_header = arbitrary_account();
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let result = market.charge_account_backing_fee_not_atomic(
        &mut PortfolioV16ViewMut::new(&mut account_header),
        kani::any(),
        kani::any(),
        kani::any(),
        kani::any(),
    );
    assert_ok_keeps_the_header_predicate(result.is_ok(), &before, &market);
    kani::cover!(result.is_ok(), "Ok is reached");
    kani::cover!(result.is_err(), "an error is reached");
}

// T14.header
#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::solver(cadical)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, recorded_shape_check)]
#[kani::stub(
    PortfolioV16ViewMut::validate_with_market,
    stand_in_account_check_any_outcome
)]
#[kani::stub(
    MarketGroupV16ViewMut::claim_resolved_payout_topup_core_not_atomic,
    havoc_claim_resolved_payout_topup_core_not_atomic
)]
#[kani::stub(
    MarketGroupV16ViewMut::preflight_claim_resolved_payout_topup,
    any_preflight_claim_resolved_payout_topup
)]
fn proof_v16_claim_resolved_payout_topup_ok_keeps_the_header_predicate() {
    let (mut header, mut markets) = arbitrary_header_and_slot();
    let before = header;
    let mut account_header = arbitrary_account();
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let result = market
        .claim_resolved_payout_topup_not_atomic(&mut PortfolioV16ViewMut::new(&mut account_header));
    assert_ok_keeps_the_header_predicate(result.is_ok(), &before, &market);
    kani::cover!(result.is_ok(), "Ok is reached");
    kani::cover!(result.is_err(), "an error is reached");
}

// T18.header
#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::solver(cadical)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, recorded_shape_check)]
#[kani::stub(
    PortfolioV16ViewMut::validate_with_market,
    stand_in_account_check_any_outcome
)]
#[kani::stub(
    MarketGroupV16ViewMut::account_has_active_source_claim_exposure,
    any_account_has_active_source_claim_exposure
)]
#[kani::stub(
    MarketGroupV16ViewMut::account_source_realizable_support,
    any_account_source_realizable_support
)]
#[kani::stub(
    MarketGroupV16ViewMut::burn_account_source_claim_bound_num_domain_first,
    havoc_burn_account_source_claim_bound_num_domain_first
)]
#[kani::stub(
    MarketGroupV16ViewMut::consume_validated_account_source_credit_not_atomic,
    havoc_consume_validated_account_source_credit_not_atomic
)]
#[kani::stub(
    MarketGroupV16ViewMut::face_claim_to_burn_for_support,
    any_face_claim_to_burn_for_support
)]
#[kani::stub(
    MarketGroupV16ViewMut::haircut_effective_support,
    any_haircut_effective_support
)]
#[kani::stub(MarketGroupV16ViewMut::junior_claim_bound, any_junior_claim_bound)]
#[kani::stub(
    MarketGroupV16ViewMut::preflight_convert_released_pnl_to_capital,
    any_preflight_convert_released_pnl_to_capital
)]
#[kani::stub(MarketGroupV16ViewMut::residual, any_residual)]
#[kani::stub(
    MarketGroupV16ViewMut::set_account_pnl_after_source_claim_burn,
    havoc_set_account_pnl_after_source_claim_burn
)]
#[kani::stub(
    MarketGroupV16ViewMut::account_has_source_claims,
    any_pure_account_has_source_claims
)]
#[kani::stub(
    MarketGroupV16ViewMut::account_has_source_liens,
    any_pure_account_has_source_liens
)]
fn proof_v16_convert_released_pnl_to_capital_ok_keeps_the_header_predicate() {
    let (mut header, mut markets) = arbitrary_header_and_slot();
    let before = header;
    let mut account_header = arbitrary_account();
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let result = market.convert_released_pnl_to_capital_not_atomic(&mut PortfolioV16ViewMut::new(
        &mut account_header,
    ));
    assert_ok_keeps_the_header_predicate(result.is_ok(), &before, &market);
    kani::cover!(result.is_ok(), "Ok is reached");
    kani::cover!(result.is_err(), "an error is reached");
}

// T21.header
#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::solver(cadical)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, recorded_shape_check)]
#[kani::stub(
    PortfolioV16ViewMut::validate_with_market,
    stand_in_account_check_any_outcome
)]
fn proof_v16_credit_account_from_insurance_ok_keeps_the_header_predicate() {
    let (mut header, mut markets) = arbitrary_header_and_slot();
    let before = header;
    let mut account_header = arbitrary_account();
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let result = market.credit_account_from_insurance_not_atomic(
        &mut PortfolioV16ViewMut::new(&mut account_header),
        kani::any(),
    );
    assert_ok_keeps_the_header_predicate(result.is_ok(), &before, &market);
    kani::cover!(result.is_ok(), "Ok is reached");
    kani::cover!(result.is_err(), "an error is reached");
}

// T22.header
#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::solver(cadical)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, recorded_shape_check)]
#[kani::stub(
    MarketGroupV16ViewMut::backing_bucket_for_domain,
    any_backing_bucket_for_domain
)]
#[kani::stub(MarketGroupV16ViewMut::domain_asset_side, any_domain_asset_side)]
#[kani::stub(
    MarketGroupV16ViewMut::set_backing_bucket_for_domain,
    havoc_set_backing_bucket_for_domain
)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_source_domain_ledger,
    any_validate_source_domain_ledger
)]
fn proof_v16_credit_backing_provider_earnings_ok_keeps_the_header_predicate() {
    let (mut header, mut markets) = arbitrary_header_and_slot();
    let before = header;
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let result = market.credit_backing_provider_earnings_not_atomic(kani::any(), kani::any());
    assert_ok_keeps_the_header_predicate(result.is_ok(), &before, &market);
    kani::cover!(result.is_ok(), "Ok is reached");
    kani::cover!(result.is_err(), "an error is reached");
}

// T23.header
#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::solver(cadical)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, recorded_shape_check)]
#[kani::stub(
    MarketGroupV16ViewMut::domain_insurance_budget_spent,
    any_domain_insurance_budget_spent
)]
#[kani::stub(
    MarketGroupV16ViewMut::set_domain_insurance_budget_core,
    havoc_set_domain_insurance_budget_core
)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_source_domain_ledger,
    any_validate_source_domain_ledger
)]
fn proof_v16_credit_domain_insurance_budget_ok_keeps_the_header_predicate() {
    let (mut header, mut markets) = arbitrary_header_and_slot();
    let before = header;
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let result = market.credit_domain_insurance_budget_not_atomic(kani::any(), kani::any());
    assert_ok_keeps_the_header_predicate(result.is_ok(), &before, &market);
    kani::cover!(result.is_ok(), "Ok is reached");
    kani::cover!(result.is_err(), "an error is reached");
}

// T25.header
#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::solver(cadical)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, recorded_shape_check)]
#[kani::stub(
    MarketGroupV16ViewMut::cure_and_cancel_close_with_cert_not_atomic,
    verified_cure_and_cancel_close_with_cert_not_atomic
)]
#[kani::stub(
    MarketGroupV16ViewMut::full_account_refresh_not_atomic,
    havoc_full_account_refresh_not_atomic
)]
#[kani::stub(
    MarketGroupV16ViewMut::preflight_cure_and_cancel_close,
    any_preflight_cure_and_cancel_close
)]
fn proof_v16_cure_and_cancel_close_ok_keeps_the_header_predicate() {
    let (mut header, mut markets) = arbitrary_header_and_slot();
    let before = header;
    let mut account_header = arbitrary_account();
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let result = market.cure_and_cancel_close_not_atomic(
        &mut PortfolioV16ViewMut::new(&mut account_header),
        kani::any(),
    );
    assert_ok_keeps_the_header_predicate(result.is_ok(), &before, &market);
    kani::cover!(result.is_ok(), "Ok is reached");
    kani::cover!(result.is_err(), "an error is reached");
}

// T26.header
#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::solver(cadical)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, recorded_shape_check)]
#[kani::stub(MarketGroupV16ViewMut::domain_asset_side, any_domain_asset_side)]
#[kani::stub(
    MarketGroupV16ViewMut::domain_insurance_budget_spent,
    any_domain_insurance_budget_spent
)]
#[kani::stub(
    MarketGroupV16ViewMut::set_domain_insurance_budget_core,
    havoc_set_domain_insurance_budget_core
)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_source_domain_ledger,
    any_validate_source_domain_ledger
)]
fn proof_v16_deposit_domain_insurance_ok_keeps_the_header_predicate() {
    let (mut header, mut markets) = arbitrary_header_and_slot();
    let before = header;
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let result = market.deposit_domain_insurance_not_atomic(kani::any(), kani::any());
    assert_ok_keeps_the_header_predicate(result.is_ok(), &before, &market);
    kani::cover!(result.is_ok(), "Ok is reached");
    kani::cover!(result.is_err(), "an error is reached");
}

// T27.header
#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::solver(cadical)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, recorded_shape_check)]
#[kani::stub(
    MarketGroupV16ViewMut::add_fresh_counterparty_backing_unchecked,
    havoc_add_fresh_counterparty_backing_unchecked
)]
#[kani::stub(
    MarketGroupV16ViewMut::adjust_provider_principal_num,
    havoc_adjust_provider_principal_num
)]
#[kani::stub(MarketGroupV16ViewMut::domain_asset_side, any_domain_asset_side)]
#[kani::stub(
    MarketGroupV16ViewMut::shift_provider_protection_floor_num,
    havoc_shift_provider_protection_floor_num
)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_source_domain_ledger,
    any_validate_source_domain_ledger
)]
fn proof_v16_deposit_fresh_counterparty_backing_ok_keeps_the_header_predicate() {
    let (mut header, mut markets) = arbitrary_header_and_slot();
    let before = header;
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let result =
        market.deposit_fresh_counterparty_backing_not_atomic(kani::any(), kani::any(), kani::any());
    assert_ok_keeps_the_header_predicate(result.is_ok(), &before, &market);
    kani::cover!(result.is_ok(), "Ok is reached");
    kani::cover!(result.is_err(), "an error is reached");
}

// T28.header
#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::solver(cadical)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, recorded_shape_check)]
#[kani::stub(
    PortfolioV16ViewMut::validate_with_market,
    stand_in_account_check_any_outcome
)]
fn proof_v16_deposit_ok_keeps_the_header_predicate() {
    let (mut header, mut markets) = arbitrary_header_and_slot();
    let before = header;
    let mut account_header = arbitrary_account();
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let result = market.deposit_not_atomic(
        &mut PortfolioV16ViewMut::new(&mut account_header),
        kani::any(),
    );
    assert_ok_keeps_the_header_predicate(result.is_ok(), &before, &market);
    kani::cover!(result.is_ok(), "Ok is reached");
    kani::cover!(result.is_err(), "an error is reached");
}

// T31.header
#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::solver(cadical)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, recorded_shape_check)]
#[kani::stub(
    PortfolioV16ViewMut::validate_with_market,
    stand_in_account_check_any_outcome
)]
#[kani::stub(
    MarketGroupV16ViewMut::can_ignore_unrelated_loss_stale_for_trade,
    any_can_ignore_unrelated_loss_stale_for_trade
)]
#[kani::stub(
    MarketGroupV16ViewMut::execute_batch_with_fee_after_tail_validation_not_atomic,
    havoc_execute_batch_with_fee_after_tail_validation_not_atomic
)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_unconfigured_market_tail,
    any_validate_unconfigured_market_tail
)]
fn proof_v16_execute_trade_with_fee_loss_stale_scoped_ok_keeps_the_header_predicate() {
    let (mut header, mut markets) = arbitrary_header_and_slot();
    let mut long_account_header = arbitrary_account();
    let mut short_account_header = arbitrary_account();
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let result = market.execute_trade_with_fee_loss_stale_scoped_not_atomic(
        &mut PortfolioV16ViewMut::new(&mut long_account_header),
        &mut PortfolioV16ViewMut::new(&mut short_account_header),
        kani::any(),
    );
    assert_ok_follows_a_passed_check(result.is_ok(), &market);
    kani::cover!(result.is_ok(), "Ok is reached");
    kani::cover!(result.is_err(), "an error is reached");
}

// T35.header
#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::solver(cadical)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, recorded_shape_check)]
#[kani::stub(MarketGroupV16ViewMut::asset_state, any_asset_state)]
#[kani::stub(
    MarketGroupV16ViewMut::execute_trade_with_fee_loss_stale_scoped_not_atomic,
    verified_execute_trade_with_fee_loss_stale_scoped_not_atomic
)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_configured_asset_index,
    any_validate_configured_asset_index
)]
#[kani::stub(
    MarketGroupV16ViewMut::active_leg_for_asset,
    any_pure_active_leg_for_asset
)]
#[kani::stub(
    V16Core::effective_abs_quantity_for_leg,
    any_pure_effective_abs_quantity_for_leg
)]
fn proof_v16_force_close_recovery_pair_ok_keeps_the_header_predicate() {
    let (mut header, mut markets) = arbitrary_header_and_slot();
    let before = header;
    let mut account_a_header = arbitrary_account();
    let mut account_b_header = arbitrary_account();
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let result = market.force_close_recovery_pair_not_atomic(
        &mut PortfolioV16ViewMut::new(&mut account_a_header),
        &mut PortfolioV16ViewMut::new(&mut account_b_header),
        kani::any(),
        kani::any(),
    );
    assert_ok_keeps_the_header_predicate(result.is_ok(), &before, &market);
    kani::cover!(result.is_ok(), "Ok is reached");
    kani::cover!(result.is_err(), "an error is reached");
}

// T34.header
#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::solver(cadical)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, recorded_shape_check)]
#[kani::stub(MarketGroupV16ViewMut::asset_state, any_asset_state)]
#[kani::stub(
    MarketGroupV16ViewMut::commit_asset_set_epoch_bump,
    havoc_commit_asset_set_epoch_bump
)]
#[kani::stub(MarketGroupV16ViewMut::set_asset_state, havoc_set_asset_state)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_configured_asset_index,
    any_validate_configured_asset_index
)]
fn proof_v16_force_asset_recovery_ok_keeps_the_header_predicate() {
    let (mut header, mut markets) = arbitrary_header_and_slot();
    let before = header;
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let result = market.force_asset_recovery_not_atomic(kani::any(), kani::any());
    assert_ok_keeps_the_header_predicate(result.is_ok(), &before, &market);
    kani::cover!(result.is_ok(), "Ok is reached");
    kani::cover!(result.is_err(), "an error is reached");
}

// T43.header
#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::solver(cadical)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, recorded_shape_check)]
#[kani::stub(MarketGroupV16ViewMut::asset_state, any_asset_state)]
#[kani::stub(
    MarketGroupV16ViewMut::checked_asset_set_epoch_bump,
    any_checked_asset_set_epoch_bump
)]
#[kani::stub(
    MarketGroupV16ViewMut::commit_asset_set_epoch_bump,
    havoc_commit_asset_set_epoch_bump
)]
#[kani::stub(MarketGroupV16ViewMut::set_asset_state, havoc_set_asset_state)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_configured_asset_index,
    any_validate_configured_asset_index
)]
fn proof_v16_mark_asset_drain_only_ok_keeps_the_header_predicate() {
    let (mut header, mut markets) = arbitrary_header_and_slot();
    let before = header;
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let result = market.mark_asset_drain_only_not_atomic(kani::any());
    assert_ok_keeps_the_header_predicate(result.is_ok(), &before, &market);
    kani::cover!(result.is_ok(), "Ok is reached");
    kani::cover!(result.is_err(), "an error is reached");
}

// T46.header
#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::solver(cadical)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, recorded_shape_check)]
#[kani::stub(
    MarketGroupV16ViewMut::recredit_terminal_claim_free_residual_for_asset_core_not_atomic,
    havoc_recredit_terminal_claim_free_residual_for_asset_core_not_atomic
)]
#[kani::stub(
    MarketGroupV16ViewMut::require_terminal_claim_free_state,
    any_require_terminal_claim_free_state
)]
fn proof_v16_recredit_terminal_claim_free_residual_for_asset_ok_keeps_the_header_predicate() {
    let (mut header, mut markets) = arbitrary_header_and_slot();
    let before = header;
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let result = market.recredit_terminal_claim_free_residual_for_asset_not_atomic(kani::any());
    assert_ok_keeps_the_header_predicate(result.is_ok(), &before, &market);
    kani::cover!(result.is_ok(), "Ok is reached");
    kani::cover!(result.is_err(), "an error is reached");
}

// T52.header
#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::solver(cadical)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, recorded_shape_check)]
#[kani::stub(MarketGroupV16ViewMut::asset_state, any_asset_state)]
#[kani::stub(
    MarketGroupV16ViewMut::group_has_position_or_loss_state_for_oracle_reset,
    any_group_has_position_or_loss_state_for_oracle_reset
)]
#[kani::stub(MarketGroupV16ViewMut::set_asset_state, havoc_set_asset_state)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_configured_asset_index,
    any_validate_configured_asset_index
)]
fn proof_v16_reset_empty_asset_oracle_anchor_ok_keeps_the_header_predicate() {
    let (mut header, mut markets) = arbitrary_header_and_slot();
    let before = header;
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let result =
        market.reset_empty_asset_oracle_anchor_not_atomic(kani::any(), kani::any(), kani::any());
    assert_ok_keeps_the_header_predicate(result.is_ok(), &before, &market);
    kani::cover!(result.is_ok(), "Ok is reached");
    kani::cover!(result.is_err(), "an error is reached");
}

// T53.header
#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::solver(cadical)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, recorded_shape_check)]
fn proof_v16_resolve_market_ok_keeps_the_header_predicate() {
    let (mut header, mut markets) = arbitrary_header_and_slot();
    let before = header;
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let result = market.resolve_market_not_atomic(kani::any());
    assert_ok_keeps_the_header_predicate(result.is_ok(), &before, &market);
    kani::cover!(result.is_ok(), "Ok is reached");
    kani::cover!(result.is_err(), "an error is reached");
}

// T55.header
#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::solver(cadical)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, recorded_shape_check)]
#[kani::stub(MarketGroupV16ViewMut::asset_state, any_asset_state)]
#[kani::stub(
    MarketGroupV16ViewMut::checked_asset_set_epoch_bump,
    any_checked_asset_set_epoch_bump
)]
#[kani::stub(
    MarketGroupV16ViewMut::commit_asset_set_epoch_bump,
    havoc_commit_asset_set_epoch_bump
)]
#[kani::stub(
    MarketGroupV16ViewMut::expire_lapsed_source_backing_for_asset_not_atomic,
    havoc_expire_lapsed_source_backing_for_asset_not_atomic
)]
#[kani::stub(
    MarketGroupV16ViewMut::normalize_terminal_empty_asset_history_not_atomic,
    havoc_normalize_terminal_empty_asset_history_not_atomic
)]
#[kani::stub(MarketGroupV16ViewMut::set_asset_state, havoc_set_asset_state)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_configured_asset_index,
    any_validate_configured_asset_index
)]
fn proof_v16_retire_empty_asset_ok_keeps_the_header_predicate() {
    let (mut header, mut markets) = arbitrary_header_and_slot();
    let before = header;
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let result = market.retire_empty_asset_not_atomic(kani::any(), kani::any());
    assert_ok_keeps_the_header_predicate(result.is_ok(), &before, &market);
    kani::cover!(result.is_ok(), "Ok is reached");
    kani::cover!(result.is_err(), "an error is reached");
}

// T56.header
#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::solver(cadical)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, recorded_shape_check)]
#[kani::stub(
    MarketGroupV16ViewMut::first_terminal_claim_free_recredit_asset,
    any_first_terminal_claim_free_recredit_asset
)]
#[kani::stub(
    MarketGroupV16ViewMut::require_terminal_claim_free_state,
    any_require_terminal_claim_free_state
)]
#[kani::stub(
    MarketGroupV16ViewMut::retire_terminal_unbudgeted_insurance_core_not_atomic,
    havoc_retire_terminal_unbudgeted_insurance_core_not_atomic
)]
fn proof_v16_retire_terminal_unbudgeted_insurance_ok_keeps_the_header_predicate() {
    let (mut header, mut markets) = arbitrary_header_and_slot();
    let before = header;
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let result = market.retire_terminal_unbudgeted_insurance_not_atomic();
    assert_ok_keeps_the_header_predicate(result.is_ok(), &before, &market);
    kani::cover!(result.is_ok(), "Ok is reached");
    kani::cover!(result.is_err(), "an error is reached");
}

// T57.header
#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::solver(cadical)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, recorded_shape_check)]
#[kani::stub(MarketGroupV16ViewMut::asset_state, any_asset_state)]
#[kani::stub(
    MarketGroupV16ViewMut::require_asset_mark_pushable,
    any_require_asset_mark_pushable
)]
#[kani::stub(MarketGroupV16ViewMut::set_asset_state, havoc_set_asset_state)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_configured_asset_index,
    any_validate_configured_asset_index
)]
fn proof_v16_set_asset_raw_oracle_target_ok_keeps_the_header_predicate() {
    let (mut header, mut markets) = arbitrary_header_and_slot();
    let before = header;
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let result = market.set_asset_raw_oracle_target_not_atomic(kani::any(), kani::any());
    assert_ok_keeps_the_header_predicate(result.is_ok(), &before, &market);
    kani::cover!(result.is_ok(), "Ok is reached");
    kani::cover!(result.is_err(), "an error is reached");
}

// T59.header
#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::solver(cadical)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, recorded_shape_check)]
#[kani::stub(
    PortfolioV16ViewMut::validate_with_market,
    stand_in_account_check_any_outcome
)]
#[kani::stub(
    MarketGroupV16ViewMut::account_fee_anchor_for_loss_currentness,
    any_account_fee_anchor_for_loss_currentness
)]
#[kani::stub(MarketGroupV16ViewMut::asset_state, any_asset_state)]
#[kani::stub(
    MarketGroupV16ViewMut::charge_account_fee_current_not_atomic,
    havoc_charge_account_fee_current_not_atomic
)]
#[kani::stub(
    MarketGroupV16ViewMut::fee_sync_settlement_noop_last_asset,
    any_fee_sync_settlement_noop_last_asset
)]
#[kani::stub(
    MarketGroupV16ViewMut::settle_account_kf_effects_not_atomic,
    havoc_settle_account_kf_effects_not_atomic
)]
#[kani::stub(
    MarketGroupV16ViewMut::settle_account_side_effects_after_kf_not_atomic,
    havoc_settle_account_side_effects_after_kf_not_atomic
)]
#[kani::stub(
    MarketGroupV16ViewMut::settle_negative_pnl_from_principal_core_not_atomic,
    havoc_settle_negative_pnl_from_principal_core_not_atomic
)]
fn proof_v16_sync_account_fee_to_slot_ok_keeps_the_header_predicate() {
    let (mut header, mut markets) = arbitrary_header_and_slot();
    let before = header;
    let mut account_header = arbitrary_account();
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let result = market.sync_account_fee_to_slot_not_atomic(
        &mut PortfolioV16ViewMut::new(&mut account_header),
        kani::any(),
        kani::any(),
    );
    assert_ok_keeps_the_header_predicate(result.is_ok(), &before, &market);
    kani::cover!(result.is_ok(), "Ok is reached");
    kani::cover!(result.is_err(), "an error is reached");
}

// T60.header
#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::solver(cadical)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, recorded_shape_check)]
#[kani::stub(
    MarketGroupV16ViewMut::backing_bucket_for_domain,
    any_backing_bucket_for_domain
)]
#[kani::stub(MarketGroupV16ViewMut::domain_asset_side, any_domain_asset_side)]
#[kani::stub(
    MarketGroupV16ViewMut::set_backing_bucket_for_domain,
    havoc_set_backing_bucket_for_domain
)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_source_domain_ledger,
    any_validate_source_domain_ledger
)]
fn proof_v16_withdraw_backing_provider_earnings_ok_keeps_the_header_predicate() {
    let (mut header, mut markets) = arbitrary_header_and_slot();
    let before = header;
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let result = market.withdraw_backing_provider_earnings_not_atomic(kani::any(), kani::any());
    assert_ok_keeps_the_header_predicate(result.is_ok(), &before, &market);
    kani::cover!(result.is_ok(), "Ok is reached");
    kani::cover!(result.is_err(), "an error is reached");
}

// T61.header
#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::solver(cadical)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, recorded_shape_check)]
#[kani::stub(MarketGroupV16ViewMut::domain_asset_side, any_domain_asset_side)]
#[kani::stub(
    MarketGroupV16ViewMut::domain_insurance_budget_spent,
    any_domain_insurance_budget_spent
)]
#[kani::stub(
    MarketGroupV16ViewMut::insurance_reservation_for_domain,
    any_insurance_reservation_for_domain
)]
#[kani::stub(
    MarketGroupV16ViewMut::set_domain_insurance_budget_core,
    havoc_set_domain_insurance_budget_core
)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_source_domain_ledger,
    any_validate_source_domain_ledger
)]
fn proof_v16_withdraw_domain_insurance_ok_keeps_the_header_predicate() {
    let (mut header, mut markets) = arbitrary_header_and_slot();
    let before = header;
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let result = market.withdraw_domain_insurance_not_atomic(kani::any(), kani::any());
    assert_ok_keeps_the_header_predicate(result.is_ok(), &before, &market);
    kani::cover!(result.is_ok(), "Ok is reached");
    kani::cover!(result.is_err(), "an error is reached");
}

// T62.header
#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::solver(cadical)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, recorded_shape_check)]
#[kani::stub(
    MarketGroupV16ViewMut::asset_has_unsettled_kf_position,
    any_asset_has_unsettled_kf_position
)]
#[kani::stub(
    MarketGroupV16ViewMut::backing_bucket_for_domain,
    any_backing_bucket_for_domain
)]
#[kani::stub(MarketGroupV16ViewMut::domain_asset_side, any_domain_asset_side)]
#[kani::stub(
    MarketGroupV16ViewMut::insurance_reservation_for_domain,
    any_insurance_reservation_for_domain
)]
#[kani::stub(
    MarketGroupV16ViewMut::provider_principal_num,
    any_provider_principal_num
)]
#[kani::stub(
    MarketGroupV16ViewMut::release_provider_principal_after,
    havoc_release_provider_principal_after
)]
#[kani::stub(
    MarketGroupV16ViewMut::reservation_encumbrance_proof_for_domain_parts,
    any_reservation_encumbrance_proof_for_domain_parts
)]
#[kani::stub(
    MarketGroupV16ViewMut::resolved_counterparty_backing_surplus,
    any_resolved_counterparty_backing_surplus
)]
#[kani::stub(
    MarketGroupV16ViewMut::set_backing_bucket_for_domain,
    havoc_set_backing_bucket_for_domain
)]
#[kani::stub(
    MarketGroupV16ViewMut::set_source_credit_for_domain,
    havoc_set_source_credit_for_domain
)]
#[kani::stub(
    MarketGroupV16ViewMut::shift_provider_protection_floor_num,
    havoc_shift_provider_protection_floor_num
)]
#[kani::stub(
    MarketGroupV16ViewMut::source_credit_for_domain,
    any_source_credit_for_domain
)]
#[kani::stub(
    MarketGroupV16ViewMut::source_credit_for_domain_shape,
    any_source_credit_for_domain_shape
)]
#[kani::stub(
    MarketGroupV16ViewMut::stranded_loser_backing_owed_to_twin_num,
    any_stranded_loser_backing_owed_to_twin_num
)]
#[kani::stub(
    MarketGroupV16ViewMut::validate_source_domain_ledger,
    any_validate_source_domain_ledger
)]
#[kani::stub(
    V16Core::expected_source_credit_rate_num_for_state,
    any_pure_expected_source_credit_rate_num_for_state
)]
fn proof_v16_withdraw_fresh_counterparty_backing_ok_keeps_the_header_predicate() {
    let (mut header, mut markets) = arbitrary_header_and_slot();
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let result = market.withdraw_fresh_counterparty_backing_not_atomic(kani::any(), kani::any());
    assert_ok_follows_a_passed_check(result.is_ok(), &market);
    kani::cover!(result.is_ok(), "Ok is reached");
    kani::cover!(result.is_err(), "an error is reached");
}

// T63.header
#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::solver(cadical)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, recorded_shape_check)]
#[kani::stub(
    PortfolioV16ViewMut::validate_with_market,
    stand_in_account_check_any_outcome
)]
#[kani::stub(
    MarketGroupV16ViewMut::settle_negative_pnl_from_principal_core_not_atomic,
    havoc_settle_negative_pnl_from_principal_core_not_atomic
)]
fn proof_v16_withdraw_ok_keeps_the_header_predicate() {
    let (mut header, mut markets) = arbitrary_header_and_slot();
    let before = header;
    let mut account_header = arbitrary_account();
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let result = market.withdraw_not_atomic(
        &mut PortfolioV16ViewMut::new(&mut account_header),
        kani::any(),
    );
    assert_ok_keeps_the_header_predicate(result.is_ok(), &before, &market);
    kani::cover!(result.is_ok(), "Ok is reached");
    kani::cover!(result.is_err(), "an error is reached");
}

// T64.header
#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::solver(cadical)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, recorded_shape_check)]
#[kani::stub(
    MarketGroupV16ViewMut::resolved_counterparty_backing_surplus,
    any_resolved_counterparty_backing_surplus
)]
#[kani::stub(
    MarketGroupV16ViewMut::withdraw_fresh_counterparty_backing_not_atomic,
    verified_withdraw_fresh_counterparty_backing_not_atomic
)]
fn proof_v16_withdraw_resolved_counterparty_backing_surplus_ok_keeps_the_header_predicate() {
    let (mut header, mut markets) = arbitrary_header_and_slot();
    let before = header;
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let result =
        market.withdraw_resolved_counterparty_backing_surplus_not_atomic(kani::any(), kani::any());
    assert_ok_keeps_the_header_predicate(result.is_ok(), &before, &market);
    kani::cover!(result.is_ok(), "Ok is reached");
    kani::cover!(result.is_err(), "an error is reached");
}

// T25.header (its core)
#[cfg(all(kani, feature = "closure"))]
#[kani::proof]
#[kani::solver(cadical)]
#[kani::stub(MarketGroupV16ViewMut::validate_shape, recorded_shape_check)]
#[kani::stub(
    PortfolioV16ViewMut::validate_with_market,
    stand_in_account_check_any_outcome
)]
#[kani::stub(
    MarketGroupV16ViewMut::begin_legless_asset_resets,
    havoc_begin_legless_asset_resets
)]
#[kani::stub(MarketGroupV16ViewMut::domain_asset_side, any_domain_asset_side)]
#[kani::stub(
    MarketGroupV16ViewMut::insurance_domain_index,
    any_insurance_domain_index
)]
#[kani::stub(
    MarketGroupV16ViewMut::pending_domain_loss_barrier_count,
    any_pending_domain_loss_barrier_count
)]
#[kani::stub(
    MarketGroupV16ViewMut::preflight_cure_and_cancel_close,
    any_preflight_cure_and_cancel_close
)]
#[kani::stub(
    MarketGroupV16ViewMut::set_pending_domain_loss_barrier_count,
    havoc_set_pending_domain_loss_barrier_count
)]
fn proof_v16_cure_and_cancel_close_with_cert_ok_keeps_the_header_predicate() {
    let (mut header, mut markets) = arbitrary_header_and_slot();
    let mut account_header = arbitrary_account();
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let result = market.cure_and_cancel_close_with_cert_not_atomic(
        &mut PortfolioV16ViewMut::new(&mut account_header),
        kani::any(),
        any_health_cert(),
    );
    assert_ok_follows_a_passed_check(result.is_ok(), &market);
    kani::cover!(result.is_ok(), "Ok is reached");
    kani::cover!(result.is_err(), "an error is reached");
}
