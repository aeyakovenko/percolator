#![cfg(kani)]
//! Independent Kani harnesses (independent of PRs #233-#236).

use percolator::v16::kani_available_backing_num_for_source_credit_state;
use percolator::{
    BackingBucketStatusV16, BackingBucketV16, MarketGroupV16ViewMut, SourceCreditStateV16,
};

type G = MarketGroupV16ViewMut<'static, u64>;

/// P2 at full u128 width (no atom alignment, no small bounds): for any
/// coupled Fresh bucket, an accepted expiry lowers available backing by
/// exactly the unliened amount, leaves the claim side untouched, and keeps
/// the bucket/source coupling.
#[kani::proof]
#[kani::unwind(2)]
fn independent_kani_expiry_full_width_never_raises_available() {
    let u: u128 = kani::any();
    let v: u128 = kani::any();
    let i: u128 = kani::any();
    let c: u128 = kani::any();
    let reserved = u.checked_add(v);
    kani::assume(reserved.is_some());
    let bucket = BackingBucketV16 {
        market_id: kani::any(),
        fresh_unliened_backing_num: u,
        valid_liened_backing_num: v,
        consumed_liened_backing_num: c,
        impaired_liened_backing_num: i,
        utilization_fee_earnings: kani::any(),
        expiry_slot: kani::any(),
        status: BackingBucketStatusV16::Fresh,
    };
    let source = SourceCreditStateV16 {
        positive_claim_bound_num: kani::any(),
        exact_positive_claim_num: kani::any(),
        fresh_reserved_backing_num: reserved.unwrap(),
        spent_backing_num: kani::any(),
        provider_receivable_num: c,
        valid_liened_backing_num: v,
        impaired_liened_backing_num: i,
        insurance_credit_reserved_num: kani::any(),
        valid_liened_insurance_num: kani::any(),
        impaired_liened_insurance_num: kani::any(),
        credit_rate_num: kani::any(),
        credit_epoch: kani::any(),
    };
    let before = kani_available_backing_num_for_source_credit_state(source);
    kani::assume(before.is_ok());
    let now: u64 = kani::any();
    let Ok((b2, s2)) = G::kani_prepare_counterparty_backing_expiry_delta(bucket, source, now) else {
        return;
    };
    kani::cover!(u > 0 && v > 0, "mixed expiry reached");
    let after = kani_available_backing_num_for_source_credit_state(s2).unwrap();
    assert!(after <= before.unwrap());
    assert!(before.unwrap() - after == u);
    assert!(s2.positive_claim_bound_num == source.positive_claim_bound_num);
    assert!(s2.insurance_credit_reserved_num == source.insurance_credit_reserved_num);
    assert!(s2.valid_liened_insurance_num == source.valid_liened_insurance_num);
    assert!(s2.impaired_liened_insurance_num == source.impaired_liened_insurance_num);
    assert!(s2.fresh_reserved_backing_num == b2.fresh_unliened_backing_num + b2.valid_liened_backing_num);
    assert!(s2.valid_liened_backing_num == b2.valid_liened_backing_num);
    assert!(s2.impaired_liened_backing_num == b2.impaired_liened_backing_num);
    assert!(s2.provider_receivable_num == b2.consumed_liened_backing_num);
}
