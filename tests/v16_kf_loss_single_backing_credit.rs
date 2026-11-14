//! One K/F loss can pay the domain it is owed to twice before support-and-burn: the at-par
//! netting moves the backing behind fully backed face there, and free capital pays the rest
//! while the account keeps partially backed face. The engine books both payments in one
//! ledger update. These tests pin what that update must leave behind: exactly what the two
//! payments leave when they are booked one after another, the domain's credit epoch included
//! (one step per payment).
//!
//! Scenario (three assets at 1.00, no fees, 100% margin, 1e12 of capital each):
//!   1. X is long 1,000 of asset 0 and of asset 1 against Y, and short 2,000 of asset 2
//!      against W.
//!   2. Asset 0 rises 4% and X and Y settle: X holds 40M of face in domain 1, which Y paid in.
//!   3. Asset 1 rises 4% and only X settles: X holds 40M of face in domain 3 that Y has not
//!      paid yet (credit rate 0).
//!   4. Asset 2 rises 4%: X owes 80M to domain 5.
mod kf_world;

use kf_world::{World, DEPOSIT, PX0};
use percolator::BackingBucketStatusV16;

const X: usize = 0;
const Y: usize = 1;
const W: usize = 2;
const M: u128 = 1_000_000;
const UP: u64 = PX0 + PX0 / 25;

/// Asset 0, short losers: X's fully backed face.
const PAID: usize = 1;
/// Asset 1, short losers: X's face that its loser has not paid yet.
const UNPAID: usize = 3;
/// Asset 2, short losers: where X's loss is owed.
const OWED: usize = 5;

fn positions() -> World {
    let mut w = World::new(3, &[DEPOSIT; 3]);
    w.trade(X, Y, 0, 1_000);
    w.trade(X, Y, 1, 1_000);
    w.trade(W, X, 2, 2_000);
    w
}

/// Steps 2 and 3 as asked for: `paid` gives X the fully backed face, `unpaid` the face whose
/// loser has not settled.
fn before_the_loss(paid: bool, unpaid: bool) -> World {
    let mut w = positions();
    if paid {
        w.step(0, UP, 0);
        w.refresh_all(&[X, Y]);
        assert_eq!(w.domain(PAID), (40 * M, 40 * M));
    }
    if unpaid {
        w.step(1, UP, 0);
        w.refresh(X);
        assert_eq!(w.domain(UNPAID), (40 * M, 0));
    }
    w.step(2, UP, 0);
    w
}

#[test]
fn moved_backing_and_free_capital_reach_the_loss_domain_together() {
    let mut w = before_the_loss(true, true);
    let epoch_before = w.source(OWED).credit_epoch;
    let capital_before = w.accts[X].capital.get();
    w.refresh(X);

    // The fully backed face paid 40M of the loss with the backing behind it; capital paid
    // the other 40M and the unpaid face is kept.
    assert_eq!(w.domain(PAID), (0, 0));
    assert_eq!(w.domain(UNPAID), (40 * M, 0));
    assert_eq!(w.domain(OWED), (0, 80 * M));
    assert_eq!(w.claim(X, PAID), 0);
    assert_eq!(w.claim(X, UNPAID), 40 * M);
    assert_eq!(capital_before - w.accts[X].capital.get(), 40 * M);
    assert_eq!(w.accts[X].pnl.get(), 40 * M as i128);
    assert_eq!(w.bucket(OWED).status, BackingBucketStatusV16::Fresh);
    // Two payments, two epoch steps, although the ledger was written once.
    assert_eq!(w.source(OWED).credit_epoch, epoch_before + 2);
    w.validate().unwrap();

    w.refresh_all(&[Y, W]);
    assert_eq!(w.domain(UNPAID), (40 * M, 40 * M));
    assert_eq!(w.domain(OWED), (80 * M, 80 * M));
    w.assert_settled("everyone settled");
}

#[test]
fn moved_backing_alone_is_one_payment() {
    let mut w = before_the_loss(true, false);
    let epoch_before = w.source(OWED).credit_epoch;
    let capital_before = w.accts[X].capital.get();
    w.refresh(X);

    // Nothing the account holds is partially backed, so the rest of the loss takes the
    // ordinary path: it is reserved from capital in a booking of its own.
    assert_eq!(w.domain(PAID), (0, 0));
    assert_eq!(w.domain(OWED), (0, 80 * M));
    assert_eq!(capital_before - w.accts[X].capital.get(), 40 * M);
    assert_eq!(w.accts[X].pnl.get(), 0);
    assert_eq!(w.source(OWED).credit_epoch, epoch_before + 2);
    w.validate().unwrap();

    w.refresh_all(&[Y, W]);
    w.assert_settled("everyone settled");
}

#[test]
fn free_capital_alone_is_one_payment() {
    let mut w = before_the_loss(false, true);
    let epoch_before = w.source(OWED).credit_epoch;
    let capital_before = w.accts[X].capital.get();
    w.refresh(X);

    assert_eq!(w.domain(UNPAID), (40 * M, 0));
    assert_eq!(w.domain(OWED), (0, 80 * M));
    assert_eq!(capital_before - w.accts[X].capital.get(), 80 * M);
    assert_eq!(w.accts[X].pnl.get(), 40 * M as i128);
    assert_eq!(w.source(OWED).credit_epoch, epoch_before + 1);
    w.validate().unwrap();

    w.refresh_all(&[Y, W]);
    w.assert_settled("everyone settled");
}
