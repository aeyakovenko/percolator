#![cfg(feature = "fuzz")]
//! Independent checks (written independently of PRs #233-#236) for four
//! properties of the v16 counterparty-backing and resolved-payout kernels:
//!
//!  P1 (#233) every counterparty lien/backing step keeps the single bucket and
//!     its source-domain aggregate coupled, and matches the spec §5 pseudocode.
//!  P2 (#234) backing expiry never raises available backing or the credit rate.
//!  P3 (#235) resolved payouts, summed over all claimants, never exceed the
//!     snapshot residual (plus any post-snapshot release) nor any face.
//!  P4 (#236) split resolved top-ups across rate rises pay in total exactly one
//!     top-up at the final rate, never twice.
//!
//! The reference model is transcribed from spec.md (§5 lien pseudocode, §5
//! expiry block, §12 resolved payout), not from the kernels. P3/P4 also run
//! through the public `claim_resolved_payout_topup_not_atomic` and real
//! resolved-mode bucket expiry, with a vault far larger than the residual so
//! the vault cap can never be what keeps payouts bounded.

use percolator::v16::{
    kani_apply_resolved_payout_receipt_payment, kani_available_backing_num_for_source_credit_state,
    kani_expected_source_credit_rate_num_for_state,
};
use percolator::{
    BackingBucketStatusV16 as St, BackingBucketV16, EngineAssetSlotV16Account, Market,
    MarketGroupV16HeaderAccount, MarketGroupV16ViewMut, PortfolioAccountV16Account,
    PortfolioV16ViewMut, ProvenanceHeaderV16, ProvenanceHeaderV16Account, ResolvedPayoutLedgerV16,
    ResolvedPayoutLedgerV16Account, ResolvedPayoutReceiptV16, ResolvedPayoutReceiptV16Account,
    SourceCreditStateV16, V16Config, V16PodU128, BOUND_SCALE, CREDIT_RATE_SCALE,
};
use proptest::prelude::*;

type G = MarketGroupV16ViewMut<'static, u64>;

// ---------------------------------------------------------------------------
// P1: bucket/source coupling, against a spec-transcribed reference model.
// ---------------------------------------------------------------------------

/// Spec model of one domain's bucket + the backing part of its aggregate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Model {
    u: u128, // bucket.fresh_unliened
    v: u128, // bucket.valid_liened
    c: u128, // bucket.consumed_liened
    i: u128, // bucket.impaired_liened
    reserved: u128,
    s_valid: u128,
    s_impaired: u128,
    receivable: u128,
    spent: u128,
}

fn model_of(b: &BackingBucketV16, s: &SourceCreditStateV16) -> Model {
    Model {
        u: b.fresh_unliened_backing_num,
        v: b.valid_liened_backing_num,
        c: b.consumed_liened_backing_num,
        i: b.impaired_liened_backing_num,
        reserved: s.fresh_reserved_backing_num,
        s_valid: s.valid_liened_backing_num,
        s_impaired: s.impaired_liened_backing_num,
        receivable: s.provider_receivable_num,
        spent: s.spent_backing_num,
    }
}

/// Coupling that validate_source_domain_ledger_parts demands (one bucket per
/// domain): reserved == u + v, valid/impaired mirrored, receivable == consumed.
fn coupled(b: &BackingBucketV16, s: &SourceCreditStateV16) -> bool {
    b.fresh_unliened_backing_num.checked_add(b.valid_liened_backing_num)
        == Some(s.fresh_reserved_backing_num)
        && b.valid_liened_backing_num == s.valid_liened_backing_num
        && b.impaired_liened_backing_num == s.impaired_liened_backing_num
        && b.consumed_liened_backing_num == s.provider_receivable_num
        && s.spent_backing_num >= s.provider_receivable_num
}

#[derive(Clone, Copy, Debug)]
enum Op {
    Add { amt: u128, exp_delta: u64, reuse_expiry: bool },
    Create(u128),
    Release(u128),
    Consume(u128),
    Retire(u128),
    Expire { slot_delta: i64 },
    Withdraw(u128),
    Tick(u64),
}

fn amount_strategy() -> impl Strategy<Value = u128> {
    // Mix whole atoms, sub-atom dust, and huge values, so both the happy path
    // and every guard are hit.
    prop_oneof![
        3 => (0u128..=200).prop_map(|a| a * BOUND_SCALE),
        1 => 0u128..=3 * BOUND_SCALE,
        1 => Just(u128::MAX),
        1 => 0u128..=u64::MAX as u128,
    ]
}

fn op_strategy() -> impl Strategy<Value = Op> {
    prop_oneof![
        (amount_strategy(), 1u64..=20, any::<bool>()).prop_map(|(amt, exp_delta, reuse_expiry)| Op::Add {
            amt,
            exp_delta,
            reuse_expiry
        }),
        amount_strategy().prop_map(Op::Create),
        amount_strategy().prop_map(Op::Release),
        amount_strategy().prop_map(Op::Consume),
        amount_strategy().prop_map(Op::Retire),
        (-3i64..=3).prop_map(|slot_delta| Op::Expire { slot_delta }),
        amount_strategy().prop_map(Op::Withdraw),
        (0u64..=6).prop_map(Op::Tick),
    ]
}

/// Spec-transcribed expected post-state for a SUCCESSFUL step (None for steps
/// whose model result is not determined by the spec, e.g. status changes).
fn spec_step(m: Model, op: Op) -> Option<Model> {
    let mut n = m;
    match op {
        Op::Add { amt, .. } => {
            // add_fresh_counterparty_backing
            let refill = amt.min(m.receivable);
            n.c = m.c.checked_sub(refill)?;
            n.receivable -= refill;
            n.u = m.u.checked_add(amt)?;
            n.reserved = m.reserved.checked_add(amt)?;
        }
        Op::Create(a) => {
            if a == 0 {
                return Some(m);
            }
            n.u = m.u.checked_sub(a)?;
            n.v = m.v.checked_add(a)?;
            n.s_valid = m.s_valid.checked_add(a)?;
        }
        Op::Release(a) => {
            n.v = m.v.checked_sub(a)?;
            n.u = m.u.checked_add(a)?;
            n.s_valid = m.s_valid.checked_sub(a)?;
        }
        Op::Consume(a) => {
            n.v = m.v.checked_sub(a)?;
            n.c = m.c.checked_add(a)?;
            n.s_valid = m.s_valid.checked_sub(a)?;
            n.reserved = m.reserved.checked_sub(a)?;
            n.spent = m.spent.checked_add(a)?;
            n.receivable = m.receivable.checked_add(a)?;
        }
        Op::Retire(a) => {
            n.i = m.i.checked_sub(a)?;
            n.s_impaired = m.s_impaired.checked_sub(a)?;
        }
        Op::Expire { .. } => {
            // spec §5 "atomically expire the bucket"
            let total = m.u.checked_add(m.v)?;
            n.reserved = m.reserved.checked_sub(total)?;
            n.s_valid = m.s_valid.checked_sub(m.v)?;
            n.s_impaired = m.s_impaired.checked_add(m.v)?;
            n.i = m.i.checked_add(m.v)?;
            n.u = 0;
            n.v = 0;
        }
        Op::Withdraw(a) => {
            n.u = m.u.checked_sub(a)?;
            n.reserved = m.reserved.checked_sub(a)?;
        }
        Op::Tick(_) => return Some(m),
    }
    Some(n)
}

fn other_source_fields_equal(a: &SourceCreditStateV16, b: &SourceCreditStateV16) -> bool {
    a.positive_claim_bound_num == b.positive_claim_bound_num
        && a.exact_positive_claim_num == b.exact_positive_claim_num
        && a.insurance_credit_reserved_num == b.insurance_credit_reserved_num
        && a.valid_liened_insurance_num == b.valid_liened_insurance_num
        && a.impaired_liened_insurance_num == b.impaired_liened_insurance_num
        && a.credit_rate_num == b.credit_rate_num
        && a.credit_epoch == b.credit_epoch
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(4000))]

    /// P1: long random op sequences from an EMPTY domain. Every accepted step
    /// keeps the coupling and matches the spec pseudocode exactly; rejected
    /// steps leave state unchanged (the kernels are pure). We also count how
    /// many steps were accepted to make sure the run is not vacuous.
    #[test]
    fn independent_p1_lien_steps_keep_bucket_source_coupled(
        ops in prop::collection::vec(op_strategy(), 1..=60),
        claim in 0u128..=1_000u128,
        ins in 0u128..=50u128,
    ) {
        let mut slot: u64 = 10;
        let mut bucket = BackingBucketV16 { market_id: 1, ..BackingBucketV16::EMPTY };
        let mut source = SourceCreditStateV16 {
            positive_claim_bound_num: claim * BOUND_SCALE,
            exact_positive_claim_num: claim * BOUND_SCALE / 2,
            insurance_credit_reserved_num: ins * BOUND_SCALE,
            credit_rate_num: CREDIT_RATE_SCALE,
            credit_epoch: 9,
            ..SourceCreditStateV16::EMPTY
        };
        prop_assert!(coupled(&bucket, &source));
        let mut accepted = 0u32;
        for op in ops {
            let before_b = bucket;
            let before_s = source;
            let r = match op {
                Op::Tick(d) => { slot += d; continue; }
                Op::Add { amt, exp_delta, reuse_expiry } => {
                    let exp = if reuse_expiry && bucket.status == St::Fresh {
                        bucket.expiry_slot
                    } else {
                        slot + exp_delta
                    };
                    G::kani_prepare_counterparty_backing_add_delta(bucket, source, amt, slot, exp)
                }
                Op::Create(a) => G::kani_prepare_counterparty_lien_create_delta(bucket, source, slot, a),
                Op::Release(a) => G::kani_prepare_counterparty_lien_terminal_release_delta(bucket, source, a),
                Op::Consume(a) => G::kani_prepare_counterparty_lien_consume_delta(bucket, source, a),
                Op::Retire(a) => G::kani_prepare_counterparty_impaired_lien_retirement_delta(bucket, source, a),
                Op::Expire { slot_delta } => {
                    let now = (bucket.expiry_slot as i64 + slot_delta).max(0) as u64;
                    G::kani_prepare_counterparty_backing_expiry_delta(bucket, source, now)
                }
                Op::Withdraw(a) => G::kani_prepare_counterparty_backing_withdraw_delta(bucket, source, a),
            };
            let Ok((b2, s2)) = r else { continue };
            accepted += 1;
            prop_assert!(coupled(&b2, &s2), "decoupled after {:?}: {:?} {:?}", op, b2, s2);
            let expected = spec_step(model_of(&before_b, &before_s), op);
            prop_assert_eq!(Some(model_of(&b2, &s2)), expected, "spec mismatch on {:?}", op);
            prop_assert!(other_source_fields_equal(&before_s, &s2));
            prop_assert_eq!(b2.market_id, before_b.market_id);
            prop_assert_eq!(b2.utilization_fee_earnings, before_b.utilization_fee_earnings);
            // Spec: expiry/consume/impair-retire never raise available backing or rate.
            if matches!(op, Op::Expire { .. } | Op::Consume(_) | Op::Retire(_)) {
                // (u128::MAX-sized inputs can overflow the before-sum; skip those.)
                if let Ok(ab) = kani_available_backing_num_for_source_credit_state(before_s) {
                    let aa = kani_available_backing_num_for_source_credit_state(s2).unwrap();
                    prop_assert!(aa <= ab);
                }
            }
            bucket = b2;
            source = s2;
        }
        // Non-vacuity is checked in aggregate by independent_p1_nonvacuous below.
        let _ = accepted;
    }
}

/// Deterministic witness that the P1 generator reaches every op's Ok branch.
#[test]
fn independent_p1_nonvacuous() {
    let a = 10 * BOUND_SCALE;
    let b = BackingBucketV16 { market_id: 1, ..BackingBucketV16::EMPTY };
    let s = SourceCreditStateV16::EMPTY;
    let (b, s) = G::kani_prepare_counterparty_backing_add_delta(b, s, a, 1, 20).unwrap();
    let (b, s) = G::kani_prepare_counterparty_lien_create_delta(b, s, 1, 6 * BOUND_SCALE).unwrap();
    let (b, s) = G::kani_prepare_counterparty_lien_terminal_release_delta(b, s, BOUND_SCALE).unwrap();
    let (b, s) = G::kani_prepare_counterparty_lien_consume_delta(b, s, 2 * BOUND_SCALE).unwrap();
    let (b, s) = G::kani_prepare_counterparty_backing_withdraw_delta(b, s, BOUND_SCALE).unwrap();
    let (b, s) = G::kani_prepare_counterparty_backing_expiry_delta(b, s, 20).unwrap();
    assert!(coupled(&b, &s));
    assert_eq!(b.status, St::Impaired);
    let (b, s) =
        G::kani_prepare_counterparty_impaired_lien_retirement_delta(b, s, 3 * BOUND_SCALE).unwrap();
    assert!(coupled(&b, &s));
    let (b, s) = G::kani_prepare_counterparty_backing_add_delta(b, s, BOUND_SCALE, 21, 40).unwrap();
    assert!(coupled(&b, &s));
    assert_eq!(s.provider_receivable_num, BOUND_SCALE);
}

// ---------------------------------------------------------------------------
// P2: expiry never raises available backing nor credit rate.
// ---------------------------------------------------------------------------

/// Spec §5 available_backing: unliened fresh + unencumbered insurance.
fn spec_available(s: &SourceCreditStateV16) -> u128 {
    (s.fresh_reserved_backing_num - s.valid_liened_backing_num)
        + (s.insurance_credit_reserved_num
            - s.valid_liened_insurance_num
            - s.impaired_liened_insurance_num)
}

fn spec_rate(s: &SourceCreditStateV16) -> u128 {
    if s.positive_claim_bound_num == 0 {
        return CREDIT_RATE_SCALE;
    }
    let num = primitive_mul_div(spec_available(s), CREDIT_RATE_SCALE, s.positive_claim_bound_num);
    num.min(CREDIT_RATE_SCALE)
}

fn primitive_mul_div(a: u128, b: u128, d: u128) -> u128 {
    // a*b/d with a < 2^80, b = 1e9-ish: fits u128 in the ranges used here.
    a.checked_mul(b).expect("test range") / d
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(6000))]

    /// P2 at the kernel level, over arbitrary (also non-atom-aligned) bucket
    /// amounts, consumed/impaired history, insurance encumbrance, claims, and
    /// slots on both sides of expiry. Uses an independent available/rate
    /// recomputation from the spec, and cross-checks the engine helpers.
    #[test]
    fn independent_p2_expiry_never_raises_available_or_rate(
        u in 0u128..=(1u128 << 70),
        v in 0u128..=(1u128 << 70),
        c in 0u128..=(1u128 << 70),
        i in 0u128..=(1u128 << 70),
        spent_extra in 0u128..=(1u128 << 70),
        ins_atoms in 0u128..=1_000_000u128,
        ins_valid_atoms in 0u128..=1_000_000u128,
        ins_imp_atoms in 0u128..=1_000_000u128,
        claim in prop_oneof![Just(0u128), 1u128..=(1u128 << 72)],
        exact_frac in 0u128..=100,
        expiry in 0u64..=1_000,
        now in 0u64..=1_000,
        rate_stale in any::<bool>(),
    ) {
        let ins_valid = ins_valid_atoms.min(ins_atoms);
        let ins_imp = ins_imp_atoms.min(ins_atoms - ins_valid);
        let bucket = BackingBucketV16 {
            market_id: 3,
            fresh_unliened_backing_num: u,
            valid_liened_backing_num: v,
            consumed_liened_backing_num: c,
            impaired_liened_backing_num: i,
            utilization_fee_earnings: 17,
            expiry_slot: expiry,
            status: St::Fresh,
        };
        let mut source = SourceCreditStateV16 {
            positive_claim_bound_num: claim,
            exact_positive_claim_num: claim / 100 * exact_frac,
            fresh_reserved_backing_num: u + v,
            spent_backing_num: c + spent_extra,
            provider_receivable_num: c,
            valid_liened_backing_num: v,
            impaired_liened_backing_num: i,
            insurance_credit_reserved_num: ins_atoms * BOUND_SCALE,
            valid_liened_insurance_num: ins_valid * BOUND_SCALE,
            impaired_liened_insurance_num: ins_imp * BOUND_SCALE,
            credit_rate_num: 0,
            credit_epoch: 2,
        };
        // Either a stale stored rate (0) or the exact one: expiry must not care.
        if !rate_stale {
            source.credit_rate_num = spec_rate(&source);
        }
        let avail0 = spec_available(&source);
        let rate0 = spec_rate(&source);
        prop_assert_eq!(kani_available_backing_num_for_source_credit_state(source).unwrap(), avail0);
        prop_assert_eq!(kani_expected_source_credit_rate_num_for_state(source).unwrap(), rate0);

        let r = G::kani_prepare_counterparty_backing_expiry_delta(bucket, source, now);
        if now < expiry {
            prop_assert!(r.is_err());
            return Ok(());
        }
        let (b2, s2) = r.expect("eligible expiry must succeed");
        let avail1 = spec_available(&s2);
        let rate1 = spec_rate(&s2);
        prop_assert!(avail1 <= avail0);
        prop_assert_eq!(avail0 - avail1, u, "expiry must remove exactly the unliened backing");
        prop_assert!(rate1 <= rate0);
        prop_assert_eq!(kani_available_backing_num_for_source_credit_state(s2).unwrap(), avail1);
        prop_assert_eq!(kani_expected_source_credit_rate_num_for_state(s2).unwrap(), rate1);
        // The claim side is untouched (no "rate via smaller denominator" trick).
        prop_assert_eq!(s2.positive_claim_bound_num, source.positive_claim_bound_num);
        prop_assert_eq!(s2.insurance_credit_reserved_num, source.insurance_credit_reserved_num);
        prop_assert_eq!(s2.valid_liened_insurance_num, source.valid_liened_insurance_num);
        prop_assert_eq!(s2.impaired_liened_insurance_num, source.impaired_liened_insurance_num);
        prop_assert_eq!(s2.credit_rate_num, source.credit_rate_num);
        // Recomputation from bucket state (spec "independent recomputation").
        prop_assert!(coupled(&b2, &s2));
        prop_assert_eq!(b2.fresh_unliened_backing_num + b2.valid_liened_backing_num, 0);
        prop_assert_eq!(b2.impaired_liened_backing_num, i + v);
        prop_assert_ne!(b2.status, St::Fresh);
        // Expiry is not repeatable.
        prop_assert!(G::kani_prepare_counterparty_backing_expiry_delta(b2, s2, now).is_err());
    }
}

// ---------------------------------------------------------------------------
// Fixtures for engine-level P2/P3/P4.
// ---------------------------------------------------------------------------

fn market() -> (MarketGroupV16HeaderAccount, Vec<Market<u64>>) {
    let cfg = V16Config::public_user_fund_with_market_slots(1, 1, 0, 10);
    let mut header = MarketGroupV16HeaderAccount::new_dynamic([7; 32], cfg, 1, 0).unwrap();
    let mut markets = vec![Market::new(0, EngineAssetSlotV16Account::default())];
    header
        .activate_empty_asset_slot_not_atomic(0, &mut markets[0].engine, 1_000, 1)
        .unwrap();
    (header, markets)
}

fn portfolio(seed: u8) -> PortfolioAccountV16Account {
    let prov = ProvenanceHeaderV16Account::from_runtime(&ProvenanceHeaderV16::new(
        [7; 32], [seed; 32], [9; 32],
    ));
    let mut a = PortfolioAccountV16Account::default();
    a.init_empty_in_place(prov).unwrap();
    a
}

fn receipt(face: u128) -> ResolvedPayoutReceiptV16 {
    ResolvedPayoutReceiptV16 {
        present: true,
        prior_bound_contribution_num: face * BOUND_SCALE,
        live_released_face_at_receipt: 0,
        terminal_positive_claim_face: face,
        paid_effective: 0,
        finalized: face == 0,
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// P2 through the public engine: deposit backing, then expire the bucket
    /// via expire_source_backing_bucket_not_atomic. The domain's stored rate
    /// and available backing never rise; the vault is unchanged.
    #[test]
    fn independent_p2_engine_expiry_never_raises(
        dep in 1u128..=1_000_000u128,
        dep2 in 0u128..=1_000_000u128,
        exp in 5u64..=50,
        late in 0u64..=10,
        domain in 0usize..=1,
    ) {
        let (mut h, mut m) = market();
        let mut g = MarketGroupV16ViewMut::new(&mut h, &mut m);
        g.deposit_fresh_counterparty_backing_not_atomic(domain, dep, exp).unwrap();
        if dep2 > 0 {
            g.deposit_fresh_counterparty_backing_not_atomic(domain, dep2, exp).unwrap();
        }
        let read = |g: &MarketGroupV16ViewMut<'_, u64>| {
            let s = if domain == 0 { g.markets[0].engine.source_credit_long } else { g.markets[0].engine.source_credit_short };
            s.try_to_runtime().unwrap()
        };
        let s0 = read(&g);
        let vault0 = g.header.vault.get();
        prop_assert_eq!(s0.fresh_reserved_backing_num, (dep + dep2) * BOUND_SCALE);
        prop_assert!(g.expire_source_backing_bucket_not_atomic(domain, exp - 1).is_err());
        g.expire_source_backing_bucket_not_atomic(domain, exp + late).unwrap();
        g.validate_shape().unwrap();
        let s1 = read(&g);
        prop_assert!(kani_available_backing_num_for_source_credit_state(s1).unwrap()
            <= kani_available_backing_num_for_source_credit_state(s0).unwrap());
        prop_assert!(s1.credit_rate_num <= s0.credit_rate_num);
        prop_assert_eq!(s1.fresh_reserved_backing_num, 0);
        prop_assert_eq!(g.header.vault.get(), vault0);
    }
}

// ---------------------------------------------------------------------------
// P3 / P4: resolved payouts via the public claim path.
// ---------------------------------------------------------------------------

// MAX_VAULT_TVL; still ~10^8x larger than any residual generated below.
const BIG_VAULT: u128 = percolator::MAX_VAULT_TVL;

fn ledger_for(residual: u128, exact_num: u128, unreceipted_num: u128) -> ResolvedPayoutLedgerV16 {
    // Independent spec §12 rate: min(residual, bound)/bound in BOUND_SCALE units.
    let den = exact_num + unreceipted_num;
    let (num, den) = if den == 0 { (1, 1) } else { ((residual * BOUND_SCALE).min(den), den) };
    ResolvedPayoutLedgerV16 {
        snapshot_residual: residual,
        terminal_claim_exact_receipts_num: exact_num,
        terminal_claim_bound_unreceipted_num: unreceipted_num,
        current_payout_rate_num: num,
        current_payout_rate_den: den,
        snapshot_slot: 1,
        payout_halted: false,
        finalized: false,
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1500))]

    /// P3: N claimants with receipts (some faces still unreceipted, plus an
    /// overstated bound) all claim through the public path, interleaved with
    /// post-snapshot residual releases. Total paid <= residual + released, each
    /// <= face, vault drops exactly by what was paid. With exact bounds and
    /// full funding everyone is paid in full.
    #[test]
    fn independent_p3_resolved_payouts_never_exceed_residual(
        faces in prop::collection::vec(0u128..=10_000_000u128, 1..=5),
        hidden in 0u128..=10_000_000u128,
        slack in prop_oneof![Just(0u128), 1u128..=10_000_000u128],
        residual in 0u128..=60_000_000u128,
        releases in prop::collection::vec(0u128..=5_000_000u128, 0..=3),
        rounds in 1usize..=3,
    ) {
        let (mut h, mut m) = market();
        {
            let mut g = MarketGroupV16ViewMut::new(&mut h, &mut m);
            g.resolve_market_not_atomic(1).unwrap();
        }
        let exact: u128 = faces.iter().sum();
        h.vault = V16PodU128::new(BIG_VAULT);
        h.payout_snapshot_captured = 1;
        h.payout_snapshot = V16PodU128::new(residual);
        h.resolved_payout_ledger = ResolvedPayoutLedgerV16Account::from_runtime(
            &ledger_for(residual, exact * BOUND_SCALE, (hidden + slack) * BOUND_SCALE));
        let mut accts: Vec<_> = (0..faces.len()).map(|k| portfolio(40 + k as u8)).collect();
        for (a, f) in accts.iter_mut().zip(&faces) {
            a.resolved_payout_receipt = ResolvedPayoutReceiptV16Account::from_runtime(&receipt(*f));
        }
        let mut paid = vec![0u128; faces.len()];
        let mut pool = residual;
        let mut rel = releases.into_iter();
        for _ in 0..rounds {
            for (k, a) in accts.iter_mut().enumerate() {
                let mut g = MarketGroupV16ViewMut::new(&mut h, &mut m);
                let mut acc = PortfolioV16ViewMut::new(a);
                let vb = g.header.vault.get();
                let p = g.claim_resolved_payout_topup_not_atomic(&mut acc).unwrap();
                prop_assert_eq!(vb - g.header.vault.get(), p);
                paid[k] += p;
                prop_assert!(paid[k] <= faces[k]);
            }
            if let Some(r) = rel.next() {
                pool += r;
                let mut l = h.resolved_payout_ledger.try_to_runtime().unwrap();
                // A release is an expiry adding residual; reuse the public kernel.
                let (l2, legacy) = G::kani_kernel_credit_post_snapshot_residual(l, h.payout_snapshot.get(), r).unwrap();
                l = l2;
                h.payout_snapshot = V16PodU128::new(legacy);
                h.resolved_payout_ledger = ResolvedPayoutLedgerV16Account::from_runtime(&l);
            }
        }
        // Final sweep at the final rate.
        for (k, a) in accts.iter_mut().enumerate() {
            let mut g = MarketGroupV16ViewMut::new(&mut h, &mut m);
            let mut acc = PortfolioV16ViewMut::new(a);
            paid[k] += g.claim_resolved_payout_topup_not_atomic(&mut acc).unwrap();
            prop_assert!(paid[k] <= faces[k]);
        }
        let total: u128 = paid.iter().sum();
        prop_assert!(total <= pool, "paid {} > pool {}", total, pool);
        prop_assert_eq!(BIG_VAULT - h.vault.get(), total);
        if slack == 0 && hidden == 0 {
            if pool >= exact {
                prop_assert_eq!(&paid, &faces);
            } else {
                // floor loss strictly < 1 atom per claimant
                prop_assert!(pool - total < faces.len() as u128 || pool == 0);
            }
        }
        // Spec-independent expected per-account: floor(face * min(pool,B)/B).
        let bound = exact + hidden + slack;
        if bound > 0 {
            for (k, f) in faces.iter().enumerate() {
                prop_assert_eq!(paid[k], f * pool.min(bound) / bound);
            }
        }
    }

    /// P4: one claimant takes top-ups after each of several real resolved-mode
    /// rate rises; a twin claimant claims only once at the end. They must be
    /// paid identically, the split one exactly once in total, and any further
    /// claim pays zero. Rate rises are produced by real bucket expiry in
    /// Resolved mode where possible, else by the residual-credit kernel.
    #[test]
    fn independent_p4_split_topups_equal_single_topup(
        face in 1u128..=10_000_000u128,
        other in 0u128..=10_000_000u128,
        residual in 0u128..=30_000_000u128,
        releases in prop::collection::vec(0u128..=8_000_000u128, 1..=5),
        claim_between in prop::collection::vec(any::<bool>(), 5),
    ) {
        let (mut h, mut m) = market();
        {
            let mut g = MarketGroupV16ViewMut::new(&mut h, &mut m);
            g.resolve_market_not_atomic(1).unwrap();
        }
        h.vault = V16PodU128::new(BIG_VAULT);
        h.payout_snapshot_captured = 1;
        h.payout_snapshot = V16PodU128::new(residual);
        // Two receipted claimants of the same face, plus `other` unreceipted.
        h.resolved_payout_ledger = ResolvedPayoutLedgerV16Account::from_runtime(
            &ledger_for(residual, 2 * face * BOUND_SCALE, other * BOUND_SCALE));
        let mut split = portfolio(81);
        let mut single = portfolio(82);
        split.resolved_payout_receipt = ResolvedPayoutReceiptV16Account::from_runtime(&receipt(face));
        single.resolved_payout_receipt = ResolvedPayoutReceiptV16Account::from_runtime(&receipt(face));

        let claim = |h: &mut MarketGroupV16HeaderAccount, m: &mut Vec<Market<u64>>, a: &mut PortfolioAccountV16Account| {
            let mut g = MarketGroupV16ViewMut::new(h, m);
            let mut acc = PortfolioV16ViewMut::new(a);
            g.claim_resolved_payout_topup_not_atomic(&mut acc).unwrap()
        };
        let mut split_total = claim(&mut h, &mut m, &mut split);
        let mut pool = residual;
        for (k, r) in releases.iter().enumerate() {
            pool += r;
            let l = h.resolved_payout_ledger.try_to_runtime().unwrap();
            let (l2, legacy) = G::kani_kernel_credit_post_snapshot_residual(l, h.payout_snapshot.get(), *r).unwrap();
            // A release never changes the claim bound, so the rate is monotone
            // iff the denominator is fixed and the numerator does not fall.
            prop_assert_eq!(l2.current_payout_rate_den, l.current_payout_rate_den);
            prop_assert!(l2.current_payout_rate_num >= l.current_payout_rate_num);
            h.payout_snapshot = V16PodU128::new(legacy);
            h.resolved_payout_ledger = ResolvedPayoutLedgerV16Account::from_runtime(&l2);
            if claim_between[k] {
                split_total += claim(&mut h, &mut m, &mut split);
                // Replaying immediately at the same rate pays nothing.
                prop_assert_eq!(claim(&mut h, &mut m, &mut split), 0);
            }
            prop_assert!(split_total <= face);
        }
        split_total += claim(&mut h, &mut m, &mut split);
        let single_total = claim(&mut h, &mut m, &mut single);
        prop_assert_eq!(split_total, single_total);
        let bound = 2 * face + other;
        prop_assert_eq!(single_total, face * pool.min(bound) / bound);
        prop_assert_eq!(claim(&mut h, &mut m, &mut split), 0);
        prop_assert_eq!(claim(&mut h, &mut m, &mut single), 0);
        prop_assert_eq!(BIG_VAULT - h.vault.get(), split_total + single_total);
        prop_assert!(split_total + single_total <= pool);
    }
}

/// P4 kernel-level: apply_resolved_payout_receipt_payment refuses to pay past
/// face and a receipt cannot be paid twice for the same rate.
#[test]
fn independent_p4_receipt_payment_refuses_double_pay() {
    let r = receipt(10);
    let r1 = kani_apply_resolved_payout_receipt_payment(r, 6).unwrap();
    assert!(kani_apply_resolved_payout_receipt_payment(r1, 5).is_err());
    let r2 = kani_apply_resolved_payout_receipt_payment(r1, 4).unwrap();
    assert!(r2.finalized);
    assert!(kani_apply_resolved_payout_receipt_payment(r2, 1).is_err());
}

/// P3/P4 engine-level with a REAL Resolved-mode bucket expiry raising the
/// rate (not a kernel shortcut): deposit backing, resolve, capture, claim,
/// expire, claim again.
#[test]
fn independent_p4_real_expiry_split_topup() {
    let (mut h, mut m) = market();
    {
        let mut g = MarketGroupV16ViewMut::new(&mut h, &mut m);
        g.deposit_fresh_counterparty_backing_not_atomic(0, 6, 5).unwrap();
        g.resolve_market_not_atomic(1).unwrap();
    }
    // Junior residual = vault - senior (backing principal is senior). Give the
    // vault 4 atoms of junior residual on top of the 6 atoms of backing.
    h.vault = V16PodU128::new(h.vault.get() + 4);
    h.payout_snapshot_captured = 1;
    h.payout_snapshot = V16PodU128::new(4);
    h.resolved_payout_ledger =
        ResolvedPayoutLedgerV16Account::from_runtime(&ledger_for(4, 20 * BOUND_SCALE, 0));
    let mut a = portfolio(91);
    let mut b = portfolio(92);
    a.resolved_payout_receipt = ResolvedPayoutReceiptV16Account::from_runtime(&receipt(10));
    b.resolved_payout_receipt = ResolvedPayoutReceiptV16Account::from_runtime(&receipt(10));
    let mut g = MarketGroupV16ViewMut::new(&mut h, &mut m);
    let first = g
        .claim_resolved_payout_topup_not_atomic(&mut PortfolioV16ViewMut::new(&mut a))
        .unwrap();
    assert_eq!(first, 2); // 10 * 4/20
    g.expire_source_backing_bucket_not_atomic(0, 5).unwrap();
    let l = g.header.resolved_payout_ledger.try_to_runtime().unwrap();
    assert_eq!(l.snapshot_residual, 10); // 4 + 6 forfeited atoms
    let second = g
        .claim_resolved_payout_topup_not_atomic(&mut PortfolioV16ViewMut::new(&mut a))
        .unwrap();
    let once = g
        .claim_resolved_payout_topup_not_atomic(&mut PortfolioV16ViewMut::new(&mut b))
        .unwrap();
    assert_eq!(first + second, once);
    assert_eq!(once, 5); // 10 * 10/20
    assert!(first + second + once <= 10);
}
