//! While a position has not settled the latest price move, a backing provider
//! takes out what it put in, not what the domain's losers paid.
//!
//! The withdrawal gate keeps `available backing >= registered claims`. A gain is
//! registered as a claim only when its account is refreshed, so between the
//! loser's refresh (its payment joins the domain's backing) and the winner's,
//! the payment is covered by no claim and the gate let the withdrawer take it.
//! The engine tracks provider principal per domain; in that window the
//! withdrawal is capped at it. Once every position on the asset is settled,
//! every gain is a registered claim and the watermark alone governs again.
mod kf_world;

use kf_world::{World, DEPOSIT};
use percolator::{V16Error, CREDIT_RATE_SCALE};

const M: u128 = 1_000_000;

/// X (0) is long 1,000 against Y (1). The mark rises 4% and only Y is
/// refreshed: its 40M sits in domain 1 and X's gain is not a claim yet.
fn loser_paid_winner_not_refreshed(principal: u128) -> World {
    let mut w = World::new(1, &[DEPOSIT; 2]);
    if principal != 0 {
        w.provider_deposit(1, principal);
    }
    w.trade(0, 1, 0, 1_000);
    w.step(0, 1_040_000, 0);
    w.refresh(1);
    assert_eq!(w.domain(1), (0, principal + 40 * M));
    w
}

fn withdraw(w: &mut World, domain: usize, amount: u128) -> Result<(), V16Error> {
    w.op(|m, _| m.withdraw_fresh_counterparty_backing_not_atomic(domain, amount))
}

/// Before the cap the provider took 1,040M for its 1,000M and X's 40M gain was
/// left with no backing at all (credit rate 0).
#[test]
fn provider_cannot_withdraw_the_loser_payment_before_the_winner_is_refreshed() {
    let mut w = loser_paid_winner_not_refreshed(1_000 * M);
    assert_eq!(withdraw(&mut w, 1, 1_040 * M), Err(V16Error::LockActive));
    assert_eq!(
        withdraw(&mut w, 1, 1_000 * M + 1),
        Err(V16Error::LockActive)
    );
    assert_eq!(withdraw(&mut w, 1, 1_000 * M), Ok(()));
    assert_eq!(w.domain(1), (0, 40 * M));
    assert_eq!(withdraw(&mut w, 1, 1), Err(V16Error::LockActive));
    w.refresh(0);
    assert_eq!(w.domain(1), (40 * M, 40 * M));
    assert_eq!(w.source(1).credit_rate_num, CREDIT_RATE_SCALE);
    w.provider[1] = 0;
    w.assert_settled("provider out, winner backed by the loser");
}

/// With no provider principal in the domain nothing leaves while the winner is
/// unrefreshed. Once it is refreshed its claim holds the payment by the
/// watermark, as before.
#[test]
fn without_provider_principal_nothing_leaves_before_the_winner_is_refreshed() {
    let mut w = loser_paid_winner_not_refreshed(0);
    assert_eq!(withdraw(&mut w, 1, 40 * M), Err(V16Error::LockActive));
    assert_eq!(withdraw(&mut w, 1, 1), Err(V16Error::LockActive));
    w.refresh(0);
    assert_eq!(w.domain(1), (40 * M, 40 * M));
    assert_eq!(w.source(1).credit_rate_num, CREDIT_RATE_SCALE);
    assert_eq!(withdraw(&mut w, 1, 1), Err(V16Error::LockActive));
    w.assert_settled("no provider");
}

/// Control: the claim watermark still binds, and principal that fronted a gain
/// is the provider's again once the loser has paid. The winner is refreshed
/// first, so the provider's 1,000M fronts its 40M claim and only 960M may
/// leave; after the loser's payment the provider can take its whole 1,000M and
/// the claim stays backed by the loser.
#[test]
fn control_provider_recovers_its_whole_principal_once_the_loser_has_paid() {
    let mut w = World::new(1, &[DEPOSIT; 2]);
    w.provider_deposit(1, 1_000 * M);
    w.trade(0, 1, 0, 1_000);
    w.step(0, 1_040_000, 0);
    w.refresh(0);
    assert_eq!(w.domain(1), (40 * M, 1_000 * M));
    assert_eq!(withdraw(&mut w, 1, 960 * M + 1), Err(V16Error::LockActive));
    w.refresh(1);
    assert_eq!(w.domain(1), (40 * M, 1_040 * M));
    assert_eq!(
        withdraw(&mut w, 1, 1_000 * M + 1),
        Err(V16Error::LockActive)
    );
    assert_eq!(withdraw(&mut w, 1, 1_000 * M), Ok(()));
    assert_eq!(w.domain(1), (40 * M, 40 * M));
    assert_eq!(w.source(1).credit_rate_num, CREDIT_RATE_SCALE);
    w.provider[1] = 0;
    w.assert_settled("provider out after the loser paid");
}

/// Control: with every position settled the rule does not bind. Both accounts
/// are refreshed after the move, so the gain is a registered claim and the
/// watermark alone decides, exactly as on master.
#[test]
fn control_settled_positions_leave_the_watermark_alone() {
    let mut w = World::new(1, &[DEPOSIT; 2]);
    w.provider_deposit(1, 1_000 * M);
    w.trade(0, 1, 0, 1_000);
    w.step(0, 1_040_000, 0);
    w.refresh(1);
    w.refresh(0);
    // 40M of claim on 1,040M of backing: the watermark allows 1,000M to leave.
    assert_eq!(w.domain(1), (40 * M, 1_040 * M));
    assert_eq!(
        withdraw(&mut w, 1, 1_000 * M + 1),
        Err(V16Error::LockActive)
    );
    assert_eq!(withdraw(&mut w, 1, 600 * M), Ok(()));
    assert_eq!(withdraw(&mut w, 1, 400 * M), Ok(()));
    assert_eq!(w.domain(1), (40 * M, 40 * M));
    w.provider[1] = 0;
    w.assert_settled("settled positions");
}
