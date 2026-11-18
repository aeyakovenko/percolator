//! A backing withdrawal in Live leaves a loser's payment that a reversal stranded.
//!
//! A loser that is refreshed at a peak pays its loss into its side's loss domain. If the
//! move reverses before the winners were refreshed, nobody ever claims that payment there:
//! the winners' gain is gone, and the payer's regain is claimed on the twin domain, which
//! the other side does not fund because it never had a gain to give back. Once every
//! position is settled the payment is loser-paid backing beyond the claims on its domain.
//!
//! The provider exit used to treat that as surplus. With every position settled it kept
//! only `available backing >= registered claims`, so the withdrawer of that domain could
//! take the payment, with or without principal of its own there, and the payer was left
//! with a claim that nothing backs. Nobody is insolvent in these worlds.
//!
//! World: 100% margin, no fees, 1e12 of capital each, the asset at 1.00.
mod kf_world;

use kf_world::{World, DEPOSIT};
use percolator::V16Error;

const X: usize = 0;
const Y: usize = 1;
const M: u128 = 1_000_000;
/// What Y, short 500, pays when it alone is refreshed 4% up.
const PAYMENT: u128 = 20 * M;

/// X is long 500 against Y. The mark rises 4% and only Y is refreshed; the mark returns and
/// everyone is refreshed. `principal` is provider backing in the short-loser domain, where
/// Y's payment sits.
fn payment_stranded_by_a_reversal(principal: u128) -> World {
    let short_losers = World::loss_domain(0, false);
    let long_losers = World::loss_domain(0, true);
    let mut w = World::new(1, &[DEPOSIT; 2]);
    if principal != 0 {
        w.provider_deposit(short_losers, principal);
    }
    w.trade(X, Y, 0, 500);
    w.step(0, 1_040_000, 0);
    w.refresh(Y);
    w.step(0, 1_000_000, 0);
    w.refresh_all(&[X, Y]);
    // Nobody gained or lost. Y holds its regain as a claim on the long-loser domain, which
    // nothing backs, and its own payment sits in the short-loser domain under no claim.
    assert_eq!(w.engine_equity(X), DEPOSIT as i128);
    assert_eq!(w.engine_equity(Y), DEPOSIT as i128);
    assert_eq!(w.accts[Y].pnl.get(), PAYMENT as i128);
    assert_eq!(w.domain(long_losers), (PAYMENT, 0));
    assert_eq!(w.domain(short_losers), (0, principal + PAYMENT));
    w
}

fn withdraw(w: &mut World, domain: usize, amount: u128) -> Result<(), V16Error> {
    w.op(|m, _| m.withdraw_fresh_counterparty_backing_not_atomic(domain, amount))
}

#[test]
fn a_backing_withdrawal_leaves_a_payment_stranded_by_a_reversal() {
    let short_losers = World::loss_domain(0, false);
    for principal in [0, 1_000 * M] {
        let mut w = payment_stranded_by_a_reversal(principal);
        assert_eq!(
            withdraw(&mut w, short_losers, principal + PAYMENT),
            Err(V16Error::LockActive),
            "principal {principal}: the payment is not the provider's"
        );
        assert_eq!(
            withdraw(&mut w, short_losers, principal + 1),
            Err(V16Error::LockActive),
            "principal {principal}: not an atom of it"
        );
        if principal != 0 {
            withdraw(&mut w, short_losers, principal).expect("a provider takes out what it put in");
            assert_eq!(withdraw(&mut w, short_losers, 1), Err(V16Error::LockActive));
        }
        assert_eq!(w.domain(short_losers), (0, PAYMENT));
        w.validate().unwrap();
    }
}

/// With the payment left where it is, the wind-down pays everyone exactly in either close
/// order: Y's claim is met from the junior pool, which the payment joins when its bucket
/// lapses.
#[test]
fn the_payer_is_whole_after_resolution() {
    for principal in [0, 1_000 * M] {
        for order in [[X, Y], [Y, X]] {
            let mut w = payment_stranded_by_a_reversal(principal);
            let fair = w.fair_wind_down();
            assert_eq!(fair.payouts, vec![DEPOSIT, DEPOSIT]);
            assert_eq!(w.wind_down(&order), fair, "principal {principal}, order {order:?}");
        }
    }
}

/// The same wind-down after the withdrawer has taken, in Live, everything the engine lets
/// it take from the domain that holds the payment. It comes away with its principal and
/// nothing else, and the payer is still paid in full. Before, it came away with the payment
/// as well and the payer ended that much short.
#[test]
fn a_withdrawer_that_tries_first_takes_only_its_principal() {
    let short_losers = World::loss_domain(0, false);
    for principal in [0, 1_000 * M] {
        for order in [[X, Y], [Y, X]] {
            let mut w = payment_stranded_by_a_reversal(principal);
            let mut taken = 0u128;
            for amount in [principal + PAYMENT, principal + 1, principal] {
                if amount != 0 && withdraw(&mut w, short_losers, amount).is_ok() {
                    taken += amount;
                }
            }
            assert_eq!(taken, principal, "principal {principal}");
            let wound = w.wind_down(&order);
            assert_eq!(
                wound.payouts,
                vec![DEPOSIT, DEPOSIT],
                "principal {principal}, order {order:?}"
            );
            assert_eq!(wound.provider_out[short_losers], 0);
            assert_eq!((wound.retired, wound.vault_end), (0, 0));
        }
    }
}
