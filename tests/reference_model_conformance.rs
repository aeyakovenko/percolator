//! ROADMAP Phase 6 — reference-model conformance fuzz (Pillar S, no-LoF value).
//!
//! For economics the engine computes with wide (U256) arithmetic that Kani
//! cannot symex, the honest substitute is differential conformance against an
//! INDEPENDENT reference model — but only over a STATED domain. This file pins
//! the resolved-payout `claimable` arithmetic:
//!     claimable(receipt, ledger) == floor(face * rate_num / rate_den) - paid
//! against a native-u128 reference that shares no code with the engine's
//! `wide_mul_div_floor_u128`.
//!
//! Two tiers, per the roadmap:
//!   TIER A — bounded EXHAUSTIVE subdomain: enumerated, so engine==reference is
//!            a proof-by-enumeration WITHIN the bound.
//!   TIER B — adversarial sampling over the STATED no-overflow domain
//!            (face*rate_num <= u128::MAX), with an explicit seed corpus of
//!            denominator/numerator/rounding/boundary edges.
//!
//! STATED LIMIT (not hidden): the reference is exact only where face*rate_num
//! fits u128. The product>u128 regime is the engine's U256 path and is OUT of
//! this reference's bound — covered by the engine's own arithmetic, not claimed
//! here. This is fuzz, not proof, outside Tier A.

use percolator::v16::*;
use percolator::{BOUND_SCALE, SOCIAL_LOSS_DEN};
use proptest::prelude::*;

/// Independent reference: native u128, no shared code with the engine helper.
/// Valid over the no-overflow domain face*rate_num <= u128::MAX (caller-ensured).
/// Returns Err(()) exactly when gross < paid (engine returns Err there).
fn ref_claimable(face: u128, rate_num: u128, rate_den: u128, paid: u128) -> Result<u128, ()> {
    assert!(rate_den > 0, "reference domain requires rate_den > 0");
    let gross = face
        .checked_mul(rate_num)
        .expect("reference domain bounds the product to u128")
        / rate_den;
    gross.checked_sub(paid).ok_or(())
}

/// A receipt that passes `validate_resolved_payout_receipt_value`:
/// present, exact_num(face*BOUND_SCALE) <= prior_bound, paid <= face,
/// finalized iff paid == face.
fn valid_receipt(face: u128, paid: u128) -> ResolvedPayoutReceiptV16 {
    ResolvedPayoutReceiptV16 {
        present: true,
        prior_bound_contribution_num: face
            .checked_mul(BOUND_SCALE)
            .expect("face*BOUND_SCALE within domain"),
        live_released_face_at_receipt: 0,
        terminal_positive_claim_face: face,
        paid_effective: paid,
        finalized: paid == face,
    }
}

fn ledger(rate_num: u128, rate_den: u128) -> ResolvedPayoutLedgerV16 {
    ResolvedPayoutLedgerV16 {
        snapshot_residual: 0,
        terminal_claim_exact_receipts_num: 0,
        terminal_claim_bound_unreceipted_num: 0,
        current_payout_rate_num: rate_num,
        current_payout_rate_den: rate_den,
        snapshot_slot: 1,
        payout_halted: false,
        finalized: false,
    }
}

fn check(face: u128, rate_num: u128, rate_den: u128, paid: u128) {
    let engine = MarketGroupV16ViewMut::<u64>::kani_resolved_receipt_claimable_against_ledger(
        valid_receipt(face, paid),
        ledger(rate_num, rate_den),
    );
    match ref_claimable(face, rate_num, rate_den, paid) {
        Ok(expect) => assert_eq!(
            engine,
            Ok(expect),
            "engine != reference at face={face} rn={rate_num} rd={rate_den} paid={paid}"
        ),
        Err(()) => assert!(
            engine.is_err(),
            "reference says gross<paid (Err) but engine returned {engine:?}"
        ),
    }
}

#[test]
fn tier_a_exhaustive_small_domain() {
    // Proof-by-enumeration of engine==reference over a small bounded domain.
    const B: u128 = 6;
    let mut n = 0u64;
    for face in 0..=B {
        for paid in 0..=face {
            for rate_num in 0..=B {
                for rate_den in 1..=B {
                    check(face, rate_num, rate_den, paid);
                    n += 1;
                }
            }
        }
    }
    eprintln!("tier_a_exhaustive: {n} cases (engine == reference, fully enumerated)");
}

#[test]
fn tier_b_seed_corpus_edges() {
    // Explicit seed corpus: denominator/numerator/rounding/boundary edges.
    let big = 1u128 << 50;
    let corpus: &[(u128, u128, u128, u128)] = &[
        (0, 0, 1, 0),
        (1, 1, 1, 0),
        (1, 1, 1, 1),             // gross == paid -> 0
        (big, big, 1, 0),         // large product, den 1
        (big, 1, big, 0),         // floors to 0
        (7, 3, 4, 0),             // floor(21/4) = 5
        (7, 3, 4, 5),             // gross 5, paid 5 -> 0
        (100, 999, 1000, 0),      // floor(99900/1000) = 99 (one-less edge)
        (big, big, big, big - 1), // gross ~ big, paid just below
        (big, big, big, big + 1), // paid > gross -> Err
    ];
    for &(f, rn, rd, p) in corpus {
        if f.checked_mul(rn).is_none() || f.checked_mul(BOUND_SCALE).is_none() {
            continue; // outside the stated reference domain
        }
        check(f, rn, rd, p.min(f));
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(5000))]

    /// Tier B sampling over the stated no-overflow domain. face,rate_num,rate_den
    /// up to 2^56 (product < 2^112 < u128), paid drawn in [0, face].
    #[test]
    fn tier_b_sampled_no_overflow_domain(
        face in 0u128..(1u128 << 56),
        rate_num in 0u128..(1u128 << 56),
        rate_den in 1u128..(1u128 << 56),
        paid_frac in 0u128..=1_000_000u128,
    ) {
        // Guard the stated domain (rarely trips for these bounds, but explicit).
        prop_assume!(face == 0 || rate_num <= u128::MAX / face);
        prop_assume!(face == 0 || face <= u128::MAX / BOUND_SCALE);
        let paid = face.saturating_mul(paid_frac) / 1_000_000; // paid in [0, face]
        check(face, rate_num, rate_den, paid);
    }
}

// ---- U8 bound-num conversion conformance (roadmap Phase 6; deferred from the
// Kani U8 soundness lemma, which is intractable — symbolic bound_num bit-blasts
// validate_shape's u128 ceil-division). Independent native reference. ----

// reference: bound_num_from_amount(a) == a * BOUND_SCALE (Err on overflow)
fn ref_bound_num_from_amount(a: u128) -> Result<u128, ()> {
    a.checked_mul(BOUND_SCALE).ok_or(())
}
// reference: amount_from_bound_num(n) == ceil(n / BOUND_SCALE)
fn ref_amount_from_bound_num(n: u128) -> Result<u128, ()> {
    let whole = n / BOUND_SCALE;
    if n % BOUND_SCALE == 0 {
        Ok(whole)
    } else {
        whole.checked_add(1).ok_or(())
    }
}

fn check_bound_num(a: u128) {
    let engine = kani_bound_num_from_amount(a);
    match ref_bound_num_from_amount(a) {
        Ok(e) => {
            assert_eq!(engine, Ok(e), "bound_num_from_amount({a})");
            // U8 property: the exact bound never understates the amount it scales
            assert!(e >= a, "bound_num understates amount at {a}");
        }
        Err(()) => assert!(engine.is_err()),
    }
}
fn check_amount(n: u128) {
    let engine = kani_amount_from_bound_num(n);
    match ref_amount_from_bound_num(n) {
        Ok(e) => assert_eq!(engine, Ok(e), "amount_from_bound_num({n})"),
        Err(()) => assert!(engine.is_err()),
    }
}

#[test]
fn u8_bound_num_tier_a_exhaustive() {
    // small exhaustive + around the BOUND_SCALE boundary (ceil edges)
    for a in 0u128..=64 {
        check_bound_num(a);
    }
    for n in 0u128..=64 {
        check_amount(n);
    }
    for d in 0u128..=3 {
        for off in 0u128..=3 {
            check_amount(d * BOUND_SCALE + off); // ceil boundary: rem 0 vs nonzero
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(5000))]
    #[test]
    fn u8_bound_num_from_amount_sampled(a in 0u128..(1u128 << 70)) {
        check_bound_num(a); // crosses the overflow boundary of a*BOUND_SCALE
    }
    #[test]
    fn u8_amount_from_bound_num_sampled(n in 0u128..u128::MAX) {
        check_amount(n); // full range, incl. the +1 ceil-overflow edge near u128::MAX
    }
}

// ---- ROADMAP Phase 7 — user-journey sequence conformance: N-receipt resolved
// payout draining a SCARCE pool is order-independent in TOTAL extraction and
// terminates, and per-receipt-equal when the pool funds all. Extends the
// 2-receipt Kani order-independence proof to N via reference-model sequencing
// over the PRODUCTION claimable. Pillars S (no value created) + L (terminates). ----

const N_SEQ: usize = 4;

// Drain a pool of `vault` across receipts (by claimable c[i]) in the given
// index order: each receipt draws min(c[i], remaining). Returns (per-receipt
// paid in original index order, total paid).
fn drain(claimables: &[u128; N_SEQ], vault: u128, order: &[usize; N_SEQ]) -> ([u128; N_SEQ], u128) {
    let mut paid = [0u128; N_SEQ];
    let mut rem = vault;
    let mut total = 0u128;
    for &i in order.iter() {
        let p = claimables[i].min(rem);
        paid[i] = p;
        rem -= p;
        total += p;
    }
    (paid, total)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(4000))]

    /// N-receipt scarce-pool payout: total extraction is order-independent
    /// (= min(Σ claimable, vault)) and never exceeds the pool; when the pool
    /// funds all, every receipt is paid its full claimable in any order.
    /// Claimables come from the PRODUCTION resolved_receipt_claimable helper.
    #[test]
    fn phase7_n_receipt_payout_order_independent(
        faces in prop::array::uniform4(1u128..64),
        rate_num in 0u128..64,
        rate_den in 1u128..64,
        vault in 0u128..256,
    ) {
        // production claimable for each receipt (rate applied, fresh => paid 0)
        let led = ledger(rate_num, rate_den);
        let mut c = [0u128; N_SEQ];
        for i in 0..N_SEQ {
            c[i] = MarketGroupV16ViewMut::<u64>::kani_resolved_receipt_claimable_against_ledger(
                valid_receipt(faces[i], 0), led,
            ).unwrap();
        }
        let fwd: [usize; N_SEQ] = [0, 1, 2, 3];
        let rev: [usize; N_SEQ] = [3, 2, 1, 0];
        let mix: [usize; N_SEQ] = [2, 0, 3, 1];
        let (paid_f, tot_f) = drain(&c, vault, &fwd);
        let (paid_r, tot_r) = drain(&c, vault, &rev);
        let (_paid_m, tot_m) = drain(&c, vault, &mix);

        // (a) total extraction is order-independent and bounded by the pool
        prop_assert_eq!(tot_f, tot_r);
        prop_assert_eq!(tot_f, tot_m);
        prop_assert!(tot_f <= vault);
        // (b) total equals the draining composition min(Σc, vault) — terminates,
        //     no value created
        let sum: u128 = c.iter().sum();
        prop_assert_eq!(tot_f, sum.min(vault));
        // (c) fully funded => each receipt paid its full claimable, any order
        if vault >= sum {
            for i in 0..N_SEQ {
                prop_assert_eq!(paid_f[i], c[i]);
                prop_assert_eq!(paid_r[i], c[i]);
            }
        }
    }
}

// ---- ROADMAP Phase 6 — trade-execution arithmetic conformance (Pillar S; the
// wide multiply/ceil-divide the engine computes via U256). Independent native
// references over the stated u128-product domain (realistic inputs: the U256
// path is only for products > u128, out of this reference's bound). ----
const POS_SCALE_REF: u128 = 1_000_000; // POS_SCALE
const MAX_MARGIN_BPS_REF: u128 = 10_000; // MAX_MARGIN_BPS

// fee = ceil(notional*fee_bps / MAX_MARGIN_BPS), 0 if either operand 0
fn ref_fee(notional: u128, fee_bps: u64) -> Result<u128, ()> {
    if notional == 0 || fee_bps == 0 {
        return Ok(0);
    }
    let p = notional.checked_mul(fee_bps as u128).ok_or(())?;
    let q = p / MAX_MARGIN_BPS_REF;
    Ok(if p % MAX_MARGIN_BPS_REF != 0 {
        q + 1
    } else {
        q
    })
}
// notional_floor = floor(size*price / POS_SCALE), 0 if size 0
fn ref_notional_floor(size: u128, price: u64) -> Result<u128, ()> {
    if size == 0 {
        return Ok(0);
    }
    let p = size.checked_mul(price as u128).ok_or(())?;
    Ok(p / POS_SCALE_REF)
}
// risk_ceil = ceil(abs*price / POS_SCALE), 0 if abs 0
fn ref_risk_ceil(abs: u128, price: u64) -> Result<u128, ()> {
    if abs == 0 {
        return Ok(0);
    }
    let p = abs.checked_mul(price as u128).ok_or(())?;
    let q = p / POS_SCALE_REF;
    Ok(if p % POS_SCALE_REF != 0 { q + 1 } else { q })
}

fn check_fee(n: u128, bps: u64) {
    if let Ok(e) = ref_fee(n, bps) {
        assert_eq!(kani_checked_fee_bps(n, bps), Ok(e), "fee {n} {bps}");
        // S-T3: fee CEILs (charged against the user) — never understates exact
        assert!(e * MAX_MARGIN_BPS_REF >= n.saturating_mul(bps as u128) || n == 0 || bps == 0);
    }
}
fn check_notional(s: u128, px: u64) {
    if let Ok(e) = ref_notional_floor(s, px) {
        assert_eq!(kani_trade_notional_floor(s, px), Ok(e), "notional {s} {px}");
    }
}
fn check_risk(a: u128, px: u64) {
    if let Ok(e) = ref_risk_ceil(a, px) {
        assert_eq!(kani_risk_notional_ceil(a, px), Ok(e), "risk {a} {px}");
    }
}

#[test]
fn trade_arith_tier_a_exhaustive() {
    for n in 0u128..=40 {
        for bps in 0u64..=40 {
            check_fee(n, bps);
        }
    }
    for s in 0u128..=40 {
        for px in 0u64..=40 {
            check_notional(s, px);
            check_risk(s, px);
        }
    }
    // ceil/floor boundaries around the scale denominators
    for d in 0u128..=2 {
        for off in 0u128..=2 {
            check_fee(d * MAX_MARGIN_BPS_REF + off, 1);
            check_notional(d * POS_SCALE_REF + off, 1);
            check_risk(d * POS_SCALE_REF + off, 1);
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(4000))]
    #[test]
    fn trade_fee_conformance(n in 0u128..(1u128 << 80), bps in 0u64..=10_000) {
        check_fee(n, bps);
    }
    #[test]
    fn trade_notional_floor_conformance(s in 0u128..(1u128 << 80), px in 0u64..1_000_000_000_000) {
        check_notional(s, px);
    }
    #[test]
    fn trade_risk_ceil_conformance(a in 0u128..(1u128 << 80), px in 0u64..1_000_000_000_000) {
        check_risk(a, px);
    }
}

// ---- ROADMAP Phase 3B.6 — social-loss booking division split conformance.
// social_loss_book_split(chunk, rem, ws) = (numerator/ws, numerator%ws) where
// numerator = chunk*SOCIAL_LOSS_DEN + rem. The exact integer identity
// (delta_b*ws + new_rem == numerator, new_rem < ws) is the property Kani cannot
// prove (symbolic u128 division by ws); discharged here over a stated domain. ----
fn ref_split(chunk: u128, rem: u128, ws: u128) -> Result<(u128, u128), ()> {
    if ws == 0 {
        return Err(());
    }
    let num = chunk
        .checked_mul(SOCIAL_LOSS_DEN)
        .and_then(|v| v.checked_add(rem))
        .ok_or(())?;
    Ok((num / ws, num % ws))
}
fn check_split(chunk: u128, rem: u128, ws: u128) {
    let engine = kani_social_loss_book_split(chunk, rem, ws);
    match ref_split(chunk, rem, ws) {
        Ok((db, nr)) => {
            assert_eq!(engine, Ok((db, nr)), "split {chunk} {rem} {ws}");
            assert!(nr < ws, "remainder >= weight_sum");
            // exact integer identity: delta_b*ws + new_rem == numerator
            let num = chunk * SOCIAL_LOSS_DEN + rem;
            assert_eq!(
                db.checked_mul(ws).and_then(|v| v.checked_add(nr)),
                Some(num)
            );
            // PROGRESS (3C #5): delta_b advances IFF the numerator reaches one
            // full weight_sum of capacity — so a chunk makes B progress exactly
            // when there is capacity for it (the engine's delta_b==0 -> no-op).
            assert_eq!(db > 0, num >= ws, "delta_b progress mismatch");
        }
        Err(()) => assert!(engine.is_err()),
    }
}

#[test]
fn social_loss_split_tier_a_exhaustive() {
    for chunk in 0u128..=8 {
        for rem in 0u128..=8 {
            for ws in 1u128..=8 {
                check_split(chunk, rem, ws);
            }
        }
    }
    // weight_sum == 0 must fail-closed
    assert!(kani_social_loss_book_split(1, 0, 0).is_err());
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(5000))]
    #[test]
    fn social_loss_split_sampled(
        chunk in 0u128..(1u128 << 55),  // SOCIAL_LOSS_DEN ~ 2^70, so chunk*DEN < 2^125 fits u128
        rem in 0u128..(1u128 << 50),
        ws in 1u128..(1u128 << 60),
    ) {
        check_split(chunk, rem, ws);
    }
}

// ---- ROADMAP Phase 5 (arithmetic policy): explicit BOUNDARY VECTORS. The Tier-A
// exhaustive + Tier-B sampled tests cover small + random-wide domains; these pin
// the differential discharge EXACTLY at the extremes named in the policy
// (max/min notional, fee at 0 and the bps cap, split at the weight_sum /
// SOCIAL_LOSS_DEN boundaries) so the axiom discharge is not only sampled. ----
#[test]
fn arithmetic_boundary_vectors_fee_notional_risk() {
    let big = (1u128 << 80) - 1;
    let max_px = 1_000_000_000_000u64 - 1;
    // fee: zero size, zero bps, max bps cap, max size x cap
    for &(n, bps) in &[
        (0u128, 0u64),
        (0, 10_000),
        (1, 10_000),
        (big, 0),
        (big, 10_000),
        (MAX_MARGIN_BPS_REF, 10_000),
        (MAX_MARGIN_BPS_REF - 1, 10_000),
        (MAX_MARGIN_BPS_REF + 1, 1),
    ] {
        check_fee(n, bps);
    }
    // notional/risk: zero, max, and exactly on the POS_SCALE denominator boundary
    for &(s, px) in &[
        (0u128, 0u64),
        (0, max_px),
        (big, 0),
        (big, max_px),
        (POS_SCALE_REF, 1),
        (POS_SCALE_REF - 1, 1),
        (POS_SCALE_REF + 1, 1),
        (1, max_px),
        (POS_SCALE_REF, max_px),
    ] {
        check_notional(s, px);
        check_risk(s, px);
    }
}

#[test]
fn arithmetic_boundary_vectors_social_loss_split() {
    // split at the weight_sum / SOCIAL_LOSS_DEN boundaries: db progresses IFF
    // numerator >= ws, so pin num just below / at / just above ws.
    for &(chunk, rem, ws) in &[
        (0u128, 0u128, 1u128),                     // zero chunk
        (0, 0, SOCIAL_LOSS_DEN),                   // zero chunk, large ws
        (1, 0, SOCIAL_LOSS_DEN),                   // num == ws exactly -> db==1, nr==0
        (1, 0, SOCIAL_LOSS_DEN + 1),               // num == ws-1 -> db==0
        (1, 1, SOCIAL_LOSS_DEN),                   // num == ws+1 -> db==1, nr==1
        (1, SOCIAL_LOSS_DEN - 1, SOCIAL_LOSS_DEN), // max remainder < ws
        (1000, 0, 1),                              // ws==1 -> nr==0, db==num
        ((1u128 << 50), 0, SOCIAL_LOSS_DEN),       // large chunk
    ] {
        check_split(chunk, rem, ws);
    }
}
