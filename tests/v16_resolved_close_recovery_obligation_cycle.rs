//! Resolved close with Recovery obligations on more than one asset.
//!
//! A leg forfeited on a Recovery asset stays attached with a zero basis and its loss weight
//! (a pending obligation) for as long as a real position is attached on the opposite side of
//! the asset: a bankruptcy of that position would be shared with it. Resolved close used to
//! detach none of an account's legs while one of them was such a waiting obligation. Two
//! accounts that each hold an obligation on one asset and a real position on another,
//! crossed, therefore waited for each other for ever. Neither could close, and because an
//! attached leg blocks every positive payout, nobody in the market who had won anything was
//! paid either.
//!
//! Now the obligation holds the account's other legs only while some account in the market
//! still has a loss to settle or to book. After that it holds only itself: the other legs
//! detach, and the obligation leaves when no real position is attached opposite it, as
//! before.
//!
//! Worlds are built through the public `*_not_atomic` entry points; a failed call is rolled
//! back as the program would roll back its instruction.
#![cfg(feature = "fuzz")]

mod kf_world;

use kf_world::{World, DEPOSIT, PX0};
use percolator::*;

const M: u128 = 1_000_000;
const P: usize = 0;
const Q: usize = 1;
const R: usize = 2;
const S: usize = 3;

/// (asset, basis, loss weight) of every attached leg, in slot order.
fn legs(w: &World, i: usize) -> Vec<(u32, i128, u128)> {
    w.accts[i]
        .legs
        .iter()
        .map(|leg| leg.try_to_runtime().unwrap())
        .filter(|leg| leg.active)
        .map(|leg| (leg.asset_index, leg.basis_pos_q, leg.loss_weight))
        .collect()
}

fn forfeit(w: &mut World, i: usize, asset: usize) -> DeadLegForfeitOutcomeV16 {
    w.op(|m, a| m.forfeit_recovery_leg_not_atomic(&mut a[i], asset, u128::MAX >> 8))
        .unwrap_or_else(|e| panic!("forfeit of account {i} on asset {asset}: {e:?}"))
}

/// Three assets. P is long 10 of asset 0 against Q and short 10 of asset 1 against Q. R is
/// long 100 of asset 2 against S, and asset 2 rises 1%: R has won 1M that S pays. Assets 0
/// and 1 never move and enter Recovery. Nobody is insolvent.
fn two_recovery_assets_and_a_winner_elsewhere() -> World {
    let mut w = World::new(3, &[DEPOSIT; 4]);
    w.trade(P, Q, 0, 10);
    w.trade(Q, P, 1, 10);
    w.trade(R, S, 2, 100);
    w.step(2, PX0 + PX0 / 100, 0);
    let slot = w.slot;
    for asset in 0..2 {
        w.op(|m, _| m.force_asset_recovery_not_atomic(asset, slot))
            .unwrap();
    }
    w
}

fn resolve(w: &mut World) {
    w.slot += 1;
    let slot = w.slot;
    w.op(|m, _| m.resolve_market_not_atomic(slot)).unwrap();
}

fn close(w: &mut World, i: usize) -> ResolvedCloseOutcomeV16 {
    w.op(|m, a| m.close_resolved_account_not_atomic(&mut a[i], 0))
        .unwrap_or_else(|e| panic!("resolved close of account {i}: {e:?}"))
}

fn orders(n: usize) -> Vec<Vec<usize>> {
    fn extend(prefix: &mut Vec<usize>, n: usize, out: &mut Vec<Vec<usize>>) {
        if prefix.len() == n {
            out.push(prefix.clone());
            return;
        }
        for i in 0..n {
            if !prefix.contains(&i) {
                prefix.push(i);
                extend(prefix, n, out);
                prefix.pop();
            }
        }
    }
    let mut out = Vec::new();
    extend(&mut Vec::new(), n, &mut out);
    out
}

/// P forfeits its leg on asset 0 and Q its leg on asset 1. Each forfeited leg is held back
/// by the other account's real position on the other side, and each account still has a
/// real position on the other asset. After resolution everyone closes and is paid exactly,
/// in every close order.
#[test]
fn crossed_recovery_obligations_close_and_everyone_is_paid() {
    for order in orders(4) {
        let mut w = two_recovery_assets_and_a_winner_elsewhere();
        assert!(!forfeit(&mut w, P, 0).detached);
        assert!(!forfeit(&mut w, Q, 1).detached);
        let unit = POS_SCALE as i128;
        assert_eq!(
            legs(&w, P),
            [(0, 0, 10 * POS_SCALE), (1, -10 * unit, 10 * POS_SCALE)]
        );
        assert_eq!(
            legs(&w, Q),
            [(0, -10 * unit, 10 * POS_SCALE), (1, 0, 10 * POS_SCALE)]
        );

        let fair = w.fair_wind_down();
        assert_eq!(
            fair.payouts,
            [DEPOSIT, DEPOSIT, DEPOSIT + M, DEPOSIT - M],
            "the world's own books"
        );
        assert_eq!(w.wind_down(&order), fair, "close order {order:?}");
    }
}

/// Step by step. P and Q are closed first and keep every leg while R and S, on a third
/// asset, have not settled. Once S has paid its loss, P's real position detaches and its
/// obligation stays; the obligation leaves only when Q's real position opposite it has
/// detached.
#[test]
fn an_obligation_holds_the_other_legs_only_while_a_loss_is_unsettled() {
    let mut w = two_recovery_assets_and_a_winner_elsewhere();
    forfeit(&mut w, P, 0);
    forfeit(&mut w, Q, 1);
    resolve(&mut w);
    let unit = POS_SCALE as i128;
    let p_legs = [(0, 0, 10 * POS_SCALE), (1, -10 * unit, 10 * POS_SCALE)];
    let q_legs = [(0, -10 * unit, 10 * POS_SCALE), (1, 0, 10 * POS_SCALE)];

    // R and S have not settled the move of asset 2: S still has a loss to settle.
    for call in 0..3 {
        for i in [P, Q] {
            assert_eq!(
                close(&mut w, i),
                ResolvedCloseOutcomeV16::ProgressOnly,
                "call {call} of account {i}"
            );
        }
        assert_eq!(legs(&w, P), p_legs, "call {call}");
        assert_eq!(legs(&w, Q), q_legs, "call {call}");
    }

    // R settles its gain: S's leg is still unsettled, and nothing of P's leaves.
    assert_eq!(close(&mut w, R), ResolvedCloseOutcomeV16::ProgressOnly);
    assert_eq!(close(&mut w, P), ResolvedCloseOutcomeV16::ProgressOnly);
    assert_eq!(legs(&w, P), p_legs);

    // S settles and pays. No account has a loss left to settle or to book.
    assert_eq!(
        close(&mut w, S),
        ResolvedCloseOutcomeV16::Closed {
            payout: DEPOSIT - M
        }
    );

    // P's real position on asset 1 detaches. Its obligation on asset 0 waits: Q's real
    // position is still attached opposite it.
    assert_eq!(close(&mut w, P), ResolvedCloseOutcomeV16::ProgressOnly);
    assert_eq!(legs(&w, P), [(0, 0, 10 * POS_SCALE)]);
    assert_eq!(close(&mut w, P), ResolvedCloseOutcomeV16::ProgressOnly);
    assert_eq!(legs(&w, P), [(0, 0, 10 * POS_SCALE)]);

    // One leg of Q detaches per call: its obligation on asset 1, which P's position no
    // longer holds, and its real position on asset 0.
    assert_eq!(close(&mut w, Q), ResolvedCloseOutcomeV16::ProgressOnly);
    assert_eq!(legs(&w, Q).len(), 1);
    assert_eq!(legs(&w, P), [(0, 0, 10 * POS_SCALE)]);
    let mut q_paid = None;
    for _ in 0..3 {
        if let ResolvedCloseOutcomeV16::Closed { payout } = close(&mut w, Q) {
            q_paid = Some(payout);
            break;
        }
    }
    assert_eq!(q_paid, Some(DEPOSIT));

    // Nothing real is left opposite P's obligation: it is released and P is paid.
    assert_eq!(
        close(&mut w, P),
        ResolvedCloseOutcomeV16::Closed { payout: DEPOSIT }
    );
    let mut r_paid = None;
    for _ in 0..3 {
        if let ResolvedCloseOutcomeV16::Closed { payout } = close(&mut w, R) {
            r_paid = Some(payout);
            break;
        }
    }
    assert_eq!(r_paid, Some(DEPOSIT + M));
    w.validate().unwrap();
}

/// Control, the same before and after the fix: one obligation and nothing crossed. Only P
/// forfeits. Its obligation waits for Q's real position on asset 0 and Q is free to close.
#[test]
fn one_obligation_waits_and_everyone_is_paid() {
    for order in orders(4) {
        let mut w = two_recovery_assets_and_a_winner_elsewhere();
        assert!(!forfeit(&mut w, P, 0).detached);
        let fair = w.fair_wind_down();
        assert_eq!(w.wind_down(&order), fair, "close order {order:?}");
    }
}
