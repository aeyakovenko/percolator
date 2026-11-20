//! Spent-backing history can be cleared while the asset's other domain holds a
//! bucket that lapsed unused.
//!
//! After a matched round trip a domain keeps `consumed == receivable <= spent`
//! with every live amount at zero. `canonicalize_spent_backing_history_not_atomic`
//! clears that history, and retirement and restart are refused until it has. Its
//! last step requires the whole asset to be empty, and for a backing bucket that
//! means the empty bucket itself: status Empty, no expiry slot. A bucket that
//! lapsed with nothing in it has every amount at zero too, but status Expired
//! and its old expiry slot. Retirement rewrites such a bucket, and retirement is
//! what the history blocks. So an asset with one lapsed, unused bucket and one
//! matched history could not be cleared, retired or restarted. The way out was
//! for the backing authority to deposit one atom into the lapsed domain and
//! withdraw it, which leaves the empty bucket behind.
//!
//! The transition now rewrites a lapsed, empty bucket itself, as retirement does.
mod kf_world;

use kf_world::{World, DEPOSIT, PX0};
use percolator::{AssetLifecycleV16, BackingBucketStatusV16, SourceCreditStateV16, V16Error};

/// One asset in Recovery with no position and no claim. Domain 0's provider let
/// 7 atoms lapse unused. Domain 1's provider fronted a 2-atom gain that was
/// converted, and let the rest lapse: its history is `consumed == receivable ==
/// spent == 2`.
fn lapsed_twin_and_matched_history() -> (World, u64) {
    let mut w = World::new(1, &[DEPOSIT; 2]);
    let expiry = w.slot + 3;
    for (domain, amount) in [(0usize, 7u128), (1, 16)] {
        w.op(|m, _| m.deposit_fresh_counterparty_backing_not_atomic(domain, amount, expiry))
            .unwrap();
    }
    w.trade(0, 1, 0, 1);
    w.step(0, PX0 + 2, 0);
    w.refresh(0);
    w.trade(1, 0, 0, 1);
    w.refresh(0);
    assert_eq!(w.convert(0), Ok(2));
    w.step(0, PX0 + 2, 0);
    w.step(0, PX0 + 2, 0);
    let now = w.slot;
    for domain in [0, 1] {
        w.op(|m, _| m.expire_source_backing_bucket_not_atomic(domain, now))
            .unwrap();
    }
    let slot = w.slot + 1;
    w.op(|m, _| m.force_asset_recovery_not_atomic(0, slot))
        .unwrap();

    let lapsed = w.bucket(0);
    assert_eq!(lapsed.status, BackingBucketStatusV16::Expired);
    assert_eq!(
        (
            lapsed.fresh_unliened_backing_num,
            lapsed.valid_liened_backing_num,
            lapsed.consumed_liened_backing_num,
            lapsed.impaired_liened_backing_num,
            lapsed.utilization_fee_earnings,
        ),
        (0, 0, 0, 0, 0)
    );
    assert_ne!(lapsed.expiry_slot, 0);
    assert_eq!(w.source(0), {
        let mut empty = SourceCreditStateV16::EMPTY;
        empty.credit_epoch = w.source(0).credit_epoch;
        empty
    });
    let history = w.source(1);
    assert_eq!(
        (
            history.spent_backing_num,
            history.provider_receivable_num,
            w.bucket(1).consumed_liened_backing_num,
            history.fresh_reserved_backing_num,
            history.positive_claim_bound_num,
        ),
        (
            2 * percolator::BOUND_SCALE,
            2 * percolator::BOUND_SCALE,
            2 * percolator::BOUND_SCALE,
            0,
            0
        )
    );
    (w, slot)
}

#[test]
fn history_is_cleared_while_the_other_domain_holds_a_lapsed_unused_bucket() {
    let (mut w, slot) = lapsed_twin_and_matched_history();
    let vault = w.header.vault.get();

    // Until the history is gone the asset can be neither retired nor restarted.
    assert_eq!(
        w.op(|m, _| m.retire_empty_asset_not_atomic(0, slot)),
        Err(V16Error::LockActive)
    );
    assert_eq!(
        w.op(|m, _| m.restart_empty_asset_preserving_insurance_budget_not_atomic(0, PX0, slot)),
        Err(V16Error::LockActive)
    );

    assert_eq!(
        w.op(|m, _| m.canonicalize_spent_backing_history_not_atomic(0)),
        Ok(())
    );
    for domain in [0, 1] {
        assert_eq!(w.bucket(domain).status, BackingBucketStatusV16::Empty);
        assert_eq!(w.bucket(domain).expiry_slot, 0);
        assert_eq!(w.source(domain).spent_backing_num, 0);
        assert_eq!(w.source(domain).provider_receivable_num, 0);
    }
    assert_eq!(w.header.vault.get(), vault, "the transition moves no value");

    // Nothing is left to clear, and the asset restarts.
    assert_eq!(
        w.op(|m, _| m.canonicalize_spent_backing_history_not_atomic(0)),
        Err(V16Error::NonProgress)
    );
    w.op(|m, _| m.restart_empty_asset_preserving_insurance_budget_not_atomic(0, PX0, slot))
        .unwrap();
    assert_eq!(
        w.markets[0]
            .engine
            .asset
            .try_to_runtime()
            .unwrap()
            .lifecycle,
        AssetLifecycleV16::Active
    );
}

/// Control: a lapsed bucket that still records consumed principal is history, not
/// an empty bucket, and is left alone unless it is itself matched history. Here
/// domain 0 has nothing to clear and domain 1 is cleared as before; with no
/// lapsed bucket at all the transition behaves as it always did.
#[test]
fn control_without_a_lapsed_bucket_the_transition_is_unchanged() {
    let mut w = World::new(1, &[DEPOSIT; 2]);
    let expiry = w.slot + 3;
    w.op(|m, _| m.deposit_fresh_counterparty_backing_not_atomic(1, 16, expiry))
        .unwrap();
    w.trade(0, 1, 0, 1);
    w.step(0, PX0 + 2, 0);
    w.refresh(0);
    w.trade(1, 0, 0, 1);
    w.refresh(0);
    assert_eq!(w.convert(0), Ok(2));
    w.step(0, PX0 + 2, 0);
    w.step(0, PX0 + 2, 0);
    let now = w.slot;
    w.op(|m, _| m.expire_source_backing_bucket_not_atomic(1, now))
        .unwrap();
    let slot = w.slot + 1;
    w.op(|m, _| m.force_asset_recovery_not_atomic(0, slot))
        .unwrap();
    assert_eq!(w.bucket(0).status, BackingBucketStatusV16::Empty);
    assert_eq!(
        w.op(|m, _| m.canonicalize_spent_backing_history_not_atomic(0)),
        Ok(())
    );
    assert_eq!(w.bucket(1).status, BackingBucketStatusV16::Empty);
    w.op(|m, _| m.restart_empty_asset_preserving_insurance_budget_not_atomic(0, PX0, slot))
        .unwrap();
}
