//! Resolved close: a position keeps its loss weight until the losses of the other side of
//! its asset are booked.
//!
//! A bankrupt account's unpaid loss is shared by the positions on the other side of its
//! asset, by loss weight, at the moment it is booked. Resolved close used to detach a
//! settled leg at once. A winner that closed before the bankrupt account was touched
//! therefore left with its whole gain, and its share of the shortfall fell on the legs
//! that were still attached, their capital included: whoever cranked first was made whole
//! by whoever cranked later.
//!
//! Now a real position stays attached while a stored leg on the opposite side of its
//! asset has not settled its final K/F, or while some account in the market owes a loss
//! that is not booked or carries a B charge it has not settled. Settling never waits, so
//! one crank of every account ends the wait. The public crank reports `NonProgress` for
//! a close that has nothing to do but wait.
//!
//! Worlds are built through the public `*_not_atomic` entry points; a failed call is
//! rolled back as the program would roll back its instruction.
#![cfg(feature = "fuzz")]

mod kf_world;

use kf_world::{World, PX0};
use percolator::*;

const T: usize = 0;
const W: usize = 1;
const X: usize = 2;

const T_CAPITAL: u128 = 101_000_000;
const W_CAPITAL: u128 = 1_000_000_000;
const X_CAPITAL: u128 = 50_000_000;
/// T is long 1,000 units against W and one unit against X.
const W_UNITS: u128 = 1_000;
const X_UNITS: u128 = 1;
/// The mark falls 13% in three steps.
const MARKS: [u64; 3] = [950_000, 902_500, 870_000];
const FALL: u128 = PX0 as u128 - 870_000;

fn ten_percent_margin(cfg: &mut V16Config) {
    cfg.initial_margin_bps = 1_000;
    cfg.maintenance_margin_bps = 1_000;
}

/// T holds 1,001 units long on a tenth of margin. The mark falls 13% and nobody is
/// cranked, so T reaches resolution owing 130.13M with 101M of capital.
fn bankrupt_long() -> World {
    let mut w = World::with_config(1, &[T_CAPITAL, W_CAPITAL, X_CAPITAL], ten_percent_margin);
    w.trade(T, W, 0, W_UNITS);
    w.trade(T, X, 0, X_UNITS);
    for mark in MARKS {
        w.step(0, mark, 0);
    }
    w
}

/// What T cannot pay.
fn shortfall() -> u128 {
    (W_UNITS + X_UNITS) * FALL - T_CAPITAL
}

/// A winner's share of the shortfall by loss weight, rounded down.
fn share(units: u128) -> u128 {
    shortfall() * units / (W_UNITS + X_UNITS)
}

fn resolve(w: &mut World) {
    w.slot += 1;
    let slot = w.slot;
    w.op(|m, _| m.resolve_market_not_atomic(slot)).unwrap();
}

fn close(w: &mut World, i: usize) -> V16Result<ResolvedCloseOutcomeV16> {
    w.op(|m, a| m.close_resolved_account_not_atomic(&mut a[i], 0))
}

fn crank(w: &mut World, i: usize) -> V16Result<AutoCrankResultV16> {
    let now_slot = w.slot;
    w.op(|m, a| {
        m.permissionless_auto_crank_not_atomic(
            &mut a[i],
            AutoCrankWorkV16 {
                now_slot,
                observations: &[],
                resolved_close_fee_rate_per_slot: 0,
            },
        )
    })
}

fn attached(w: &World, i: usize) -> bool {
    w.accts[i]
        .legs
        .iter()
        .any(|leg| leg.try_to_runtime().unwrap().active)
}

fn orders(n: usize) -> Vec<Vec<usize>> {
    fn rec(cur: &mut Vec<usize>, n: usize, out: &mut Vec<Vec<usize>>) {
        if cur.len() == n {
            out.push(cur.clone());
            return;
        }
        for i in 0..n {
            if !cur.contains(&i) {
                cur.push(i);
                rec(cur, n, out);
                cur.pop();
            }
        }
    }
    let mut out = Vec::new();
    rec(&mut Vec::new(), n, &mut out);
    out
}

/// Whoever closes first, the two winners share T's shortfall by weight. Before the change
/// the first of them to close kept its whole gain: with W first, X paid 29.13M of its 50M
/// of capital for a position worth 1M.
#[test]
fn a_bankrupt_shortfall_is_shared_by_weight_in_every_close_order() {
    let total = T_CAPITAL + W_CAPITAL + X_CAPITAL;
    let mut seen: Option<Vec<u128>> = None;
    for order in orders(3) {
        let mut w = bankrupt_long();
        let out = w.wind_down(&order);
        let case = format!("close order {order:?}: {out:?}");
        assert_eq!(out.payouts[T], 0, "{case}");
        // Each winner keeps its capital and its gain less its share, to the atom the
        // shares round by.
        for (who, capital, units) in [(W, W_CAPITAL, W_UNITS), (X, X_CAPITAL, X_UNITS)] {
            let whole = capital + units * FALL - share(units);
            assert!(
                out.payouts[who] == whole || out.payouts[who] + 1 == whole,
                "{case}: account {who} is paid {} of {whole}",
                out.payouts[who]
            );
        }
        assert!(out.payouts[X] > X_CAPITAL, "{case}: X keeps its capital");
        assert_eq!(
            out.payouts.iter().sum::<u128>() + out.vault_end + out.retired,
            total,
            "{case}: conservation"
        );
        assert!(out.vault_end + out.retired <= 2, "{case}: only rounding dust is left");
        // The same in every order, to the atom the shares round by.
        match &seen {
            None => seen = Some(out.payouts.clone()),
            Some(first) => {
                for (paid, reference) in out.payouts.iter().zip(first) {
                    assert!(paid.abs_diff(*reference) <= 1, "{case}: first order paid {first:?}");
                }
            }
        }
    }
}

/// Step by step, with the winner first: its gain is settled at once, its leg stays until
/// T's loss is booked, and then it pays its share before it leaves.
#[test]
fn a_winner_keeps_its_leg_until_the_loser_opposite_has_settled() {
    let mut w = bankrupt_long();
    resolve(&mut w);

    assert_eq!(close(&mut w, W), Ok(ResolvedCloseOutcomeV16::ProgressOnly));
    assert_eq!(w.accts[W].pnl.get(), (W_UNITS * FALL) as i128);
    assert!(attached(&w, W), "W's leg waits for T");
    // Nothing else for W to do: a direct close changes nothing.
    let before = (w.header, w.markets.clone(), w.accts.clone());
    assert_eq!(close(&mut w, W), Ok(ResolvedCloseOutcomeV16::ProgressOnly));
    assert!(before == (w.header, w.markets.clone(), w.accts.clone()));

    // T pays all it has; what it cannot pay is booked onto the short side, where both
    // winners are still attached. T's own leg waits for X, which has not settled.
    assert_eq!(close(&mut w, T), Ok(ResolvedCloseOutcomeV16::ProgressOnly));
    assert_eq!(w.accts[T].capital.get(), 0);
    assert_eq!(w.accts[T].pnl.get(), 0);
    assert!(attached(&w, T), "T's leg waits for X");

    // W settles its share from its gain, then its leg leaves.
    let mut calls = 0;
    while attached(&w, W) {
        assert_eq!(close(&mut w, W), Ok(ResolvedCloseOutcomeV16::ProgressOnly));
        calls += 1;
        assert!(calls <= 4, "W's leg leaves once T is booked");
    }
    let gain = W_UNITS * FALL - share(W_UNITS);
    let pnl = w.accts[W].pnl.get() as u128;
    assert!(pnl == gain || pnl + 1 == gain, "W's gain after its share: {pnl} of {gain}");
    assert_eq!(w.accts[W].capital.get(), W_CAPITAL);

    // X settles its gain and its share in one call and leaves; then T's leg can go.
    assert_eq!(close(&mut w, X), Ok(ResolvedCloseOutcomeV16::ProgressOnly));
    let mut calls = 0;
    while attached(&w, X) {
        assert_eq!(close(&mut w, X), Ok(ResolvedCloseOutcomeV16::ProgressOnly));
        calls += 1;
        assert!(calls <= 4, "X's leg leaves");
    }
    assert_eq!(close(&mut w, T), Ok(ResolvedCloseOutcomeV16::Closed { payout: 0 }));
    assert!(!attached(&w, T));
}

/// The public crank does not accept a close that can only wait.
#[test]
fn the_public_crank_reports_no_progress_for_a_close_that_only_waits() {
    let mut w = bankrupt_long();
    resolve(&mut w);

    let first = crank(&mut w, W).expect("W's gain is settled");
    assert_eq!(first.selected, AutoCrankPlanV16::CloseResolved);
    assert!(attached(&w, W));
    // Nothing to do for W: the crank selects no action and changes nothing (the program
    // returns its non-progress error for it).
    let before = (w.header, w.markets.clone(), w.accts.clone());
    let idle = crank(&mut w, W).expect("an idle crank is not an engine error");
    assert_eq!(idle.selected, AutoCrankPlanV16::NoAction);
    assert!(before == (w.header, w.markets.clone(), w.accts.clone()));

    // T is cranked: its loss is booked. W's crank has work again, its share to settle.
    let booked = crank(&mut w, T).expect("T settles");
    assert_eq!(booked.selected, AutoCrankPlanV16::CloseResolved);
    assert_eq!(w.accts[T].capital.get(), 0);
    let next = crank(&mut w, W).expect("W settles its share");
    assert_eq!(next.selected, AutoCrankPlanV16::CloseResolved);
    assert!((w.accts[W].pnl.get() as u128) < W_UNITS * FALL);
}

/// Nobody is insolvent: a loser that closes first settles and pays at once, keeps its
/// leg until the winner has settled, and both are paid exactly in either order.
#[test]
fn a_solvent_pair_is_paid_exactly_in_either_order() {
    const LONG: usize = 0;
    const SHORT: usize = 1;
    const CAPITAL: u128 = 1_000_000_000;
    let world = || {
        let mut w = World::new(1, &[CAPITAL, CAPITAL]);
        w.trade(LONG, SHORT, 0, 100);
        w.step(0, 990_000, 0);
        w
    };
    for order in [[LONG, SHORT], [SHORT, LONG]] {
        let mut w = world();
        let fair = w.fair_wind_down();
        assert_eq!(w.wind_down(&order), fair, "close order {order:?}");
    }

    // The loser first, call by call.
    let mut w = world();
    resolve(&mut w);
    assert_eq!(close(&mut w, LONG), Ok(ResolvedCloseOutcomeV16::ProgressOnly));
    assert_eq!(w.accts[LONG].capital.get(), CAPITAL - 100 * 10_000);
    assert!(attached(&w, LONG), "the loser's leg waits for the winner to settle");
    assert_eq!(close(&mut w, SHORT), Ok(ResolvedCloseOutcomeV16::ProgressOnly));
    assert!(!attached(&w, SHORT), "the winner settles and its leg leaves");
    assert_eq!(
        close(&mut w, LONG),
        Ok(ResolvedCloseOutcomeV16::Closed { payout: CAPITAL - 100 * 10_000 })
    );
}
