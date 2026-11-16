//! A Resolved-mode close settles losses that can burn through the account's source claims
//! a bounded number of legs per call.
//!
//! Before, the first close call of an account settled every leg it held. With losses over
//! source claims that work grows with the number of losing legs times the number of claims
//! their losses burn through, and at the largest portfolio shape the program allows it does
//! not fit in one transaction. An account that cannot settle keeps its legs counted as
//! unsettled, and the engine pays no positive PnL anywhere in the market while one is.
//!
//! These tests pin the bound, who it applies to, and what it must not change: the legs are
//! settled in the same order by the same transition, and everyone is paid what they are owed.
//!
//! World: 100% margin, no fees, 1e12 of capital each, every asset at 1.00.
mod kf_world;

use kf_world::{World, DEPOSIT, PX0};
use percolator::{ResolvedCloseOutcomeV16, SideV16, RESOLVED_KF_SETTLEMENT_LEGS_PER_CALL};

const X: usize = 0;
const Y: usize = 1;
const M: u128 = 1_000_000;
const UNITS: u128 = 1_000;
/// A 4% move: 40M on 1,000 units.
const UP: u64 = PX0 + PX0 / 25;
const DOWN: u64 = PX0 - PX0 / 25;
const MOVE: u128 = 40 * M;

/// Y first earns `MOVE` on each of the first `claims` assets and closes those positions: it
/// is flat there and holds a fully backed claim per asset, paid in by X. It then goes long
/// each of the next `losses` assets, which fall 4%. Nobody is refreshed after the fall, so
/// when the market resolves all of those legs are unsettled, on both accounts.
fn losses_over_claims(claims: usize, losses: usize) -> World {
    let mut w = World::new(claims + losses, &[DEPOSIT; 2]);
    for asset in 0..claims {
        w.trade(Y, X, asset, UNITS);
        w.step(asset, UP, 0);
        w.refresh_all(&[X, Y]);
        w.trade(X, Y, asset, UNITS);
        assert_eq!(w.domain(World::loss_domain(asset, false)), (MOVE, MOVE));
    }
    assert_eq!(w.accts[Y].pnl.get(), (claims as u128 * MOVE) as i128);
    for asset in claims..claims + losses {
        w.trade(Y, X, asset, UNITS);
    }
    for asset in claims..claims + losses {
        w.step(asset, DOWN, 0);
    }
    w
}

fn resolve(w: &mut World) {
    w.slot += 1;
    let slot = w.slot;
    w.op(|m, _| m.resolve_market_not_atomic(slot)).unwrap();
}

/// Active legs of account `i` that have settled their asset's final K/F.
fn settled_legs(w: &World, i: usize) -> usize {
    let mut settled = 0;
    for leg in w.accts[i].legs.iter() {
        let leg = leg.try_to_runtime().unwrap();
        if !leg.active {
            continue;
        }
        let asset = w.markets[leg.asset_index as usize]
            .engine
            .asset
            .try_to_runtime()
            .unwrap();
        let (k, epoch) = match leg.side {
            SideV16::Long => (asset.k_long, asset.kf_epoch_long),
            SideV16::Short => (asset.k_short, asset.kf_epoch_short),
        };
        if leg.k_snap == k && leg.kf_epoch_snap == epoch {
            settled += 1;
        }
    }
    settled
}

fn active_legs(w: &World, i: usize) -> usize {
    w.accts[i]
        .legs
        .iter()
        .filter(|leg| leg.try_to_runtime().unwrap().active)
        .count()
}

/// Legs of account `i` that are settled or already detached, of the `legs` it had.
fn done_legs(w: &World, i: usize, legs: usize) -> usize {
    settled_legs(w, i) + (legs - active_legs(w, i))
}

fn close_call(w: &mut World, i: usize) -> ResolvedCloseOutcomeV16 {
    w.op(|m, a| m.close_resolved_account_not_atomic(&mut a[i], 0))
        .unwrap_or_else(|e| panic!("resolved close of account {i}: {e:?}"))
}

#[test]
fn losses_over_claims_settle_a_bounded_number_of_legs_per_call() {
    const CLAIMS: usize = 3;
    const LOSSES: usize = 7;
    let n = RESOLVED_KF_SETTLEMENT_LEGS_PER_CALL;
    let mut w = losses_over_claims(CLAIMS, LOSSES);
    resolve(&mut w);
    let capital = w.accts[Y].capital.get();
    let claims = CLAIMS as u128 * MOVE;
    assert_eq!(settled_legs(&w, Y), 0);

    // Each call settles the bound's worth of losses, in domain order. A loss is met from
    // the claims first and from capital once they are gone.
    let mut lost = 0u128;
    let mut calls = 0;
    while w.accts[Y].pnl.get() > 0 {
        assert_eq!(close_call(&mut w, Y), ResolvedCloseOutcomeV16::ProgressOnly);
        calls += 1;
        lost += n as u128 * MOVE;
        assert_eq!(settled_legs(&w, Y), calls * n, "call {calls}");
        assert_eq!(active_legs(&w, Y), LOSSES, "a chunk detaches nothing");
        assert_eq!(
            w.accts[Y].pnl.get(),
            claims.saturating_sub(lost) as i128,
            "call {calls}: claims left"
        );
        assert_eq!(
            capital - w.accts[Y].capital.get(),
            lost.saturating_sub(claims),
            "call {calls}: capital paid"
        );
        w.validate().unwrap();
    }
    assert_eq!(calls, claims.div_ceil(n as u128 * MOVE) as usize);
    assert!(
        calls * n < LOSSES,
        "losses are left for the ordinary settlement"
    );

    // With no claim left to burn, the rest settles in one call as it always did.
    close_call(&mut w, Y);
    assert_eq!(done_legs(&w, Y, LOSSES), LOSSES);
    assert_eq!(
        capital - w.accts[Y].capital.get(),
        LOSSES as u128 * MOVE - claims
    );
    w.validate().unwrap();
}

#[test]
fn every_close_order_pays_everyone_exactly() {
    for (claims, losses) in [(3, 7), (7, 7), (2, 12)] {
        for order in [[X, Y], [Y, X]] {
            let mut w = losses_over_claims(claims, losses);
            let fair = w.fair_wind_down();
            let net = (losses as i128 - claims as i128) * MOVE as i128;
            assert_eq!(
                fair.payouts,
                vec![
                    (DEPOSIT as i128 + net) as u128,
                    (DEPOSIT as i128 - net) as u128
                ]
            );
            assert_eq!(
                w.wind_down(&order),
                fair,
                "{claims} claims, {losses} losses, close order {order:?}"
            );
        }
    }
}

/// No claim to burn: fourteen losing legs settle in the first call, as before.
#[test]
fn losses_without_claims_settle_in_one_call() {
    const LEGS: usize = 14;
    let mut w = losses_over_claims(0, LEGS);
    resolve(&mut w);
    close_call(&mut w, Y);
    assert_eq!(done_legs(&w, Y, LEGS), LEGS);
    close_call(&mut w, X);
    assert_eq!(done_legs(&w, X, LEGS), LEGS);
}

/// No loss to settle: gains over claims the account already holds settle in the first call,
/// as before. X is the winner of every unsettled leg and holds a claim from an earlier one.
#[test]
fn gains_over_claims_settle_in_one_call() {
    const LEGS: usize = 6;
    let mut w = World::new(LEGS + 1, &[DEPOSIT; 2]);
    w.trade(X, Y, LEGS, UNITS);
    w.step(LEGS, UP, 0);
    w.refresh_all(&[Y, X]);
    w.trade(Y, X, LEGS, UNITS);
    assert_eq!(w.accts[X].pnl.get(), MOVE as i128);
    for asset in 0..LEGS {
        w.trade(X, Y, asset, UNITS);
    }
    for asset in 0..LEGS {
        w.step(asset, UP, 0);
    }
    resolve(&mut w);
    close_call(&mut w, X);
    assert_eq!(done_legs(&w, X, LEGS), LEGS);
}

/// At most the bound of unsettled legs: one call, as before.
#[test]
fn an_account_within_the_bound_settles_in_one_call() {
    let legs = RESOLVED_KF_SETTLEMENT_LEGS_PER_CALL;
    let mut w = losses_over_claims(3, legs);
    resolve(&mut w);
    close_call(&mut w, Y);
    assert_eq!(done_legs(&w, Y, legs), legs);
}
