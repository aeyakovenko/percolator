//! A loss first booked in Resolved mode repays the provider that fronted the gain it pays
//! for, whoever closed first.
//!
//! A provider that backs a domain pays a winner's realized gain from its principal and is
//! owed the loser's payment in return (the domain's provider receivable). In Resolved mode a
//! domain with no claim and no creditor leg left is terminal: a loss booked into it funds
//! the junior pool instead of recreating backing that nobody owns. That rule also swallowed
//! the payment the provider was still owed. When the winner closed before the loser, the
//! loser's whole payment went to the junior pool, where nothing claimed it, and was retired
//! at terminal: the provider recovered its principal less what it had fronted.
//!
//! Scenario (two assets at 1.00, no fees, 100% margin, 1e12 of capital each): a provider
//! backs domain 1 (asset 0, short losers) with 1,000M. X is long 1,000 of asset 0 against Y
//! and long 1,000 of asset 1 against W.
//!   1. Asset 0 rises 4% and only X settles: the provider fronts X's 40M of face.
//!   2. Asset 1 falls 4% and X and W settle in Live: X pays its 40M loss with that face, so
//!      40M of the provider's principal moves to domain 2, where W claims it. The provider
//!      is now owed Y's 40M.
//!   3. The market resolves before Y has settled.
mod kf_world;

use kf_world::{World, DEPOSIT};
use percolator::BOUND_SCALE;

const X: usize = 0;
const Y: usize = 1;
const W: usize = 2;
const M: u128 = 1_000_000;
/// Asset 0, short losers: the provider's domain.
const PROVIDER_DOMAIN: usize = 1;
/// Asset 1, long losers: where X's loss is owed and W's gain is claimed.
const W_DOMAIN: usize = 2;
const PRINCIPAL: u128 = 1_000 * M;

const ORDERS: [[usize; 3]; 6] = [
    [X, Y, W],
    [X, W, Y],
    [Y, X, W],
    [Y, W, X],
    [W, X, Y],
    [W, Y, X],
];

fn provider_is_owed_the_loser_payment() -> World {
    let mut w = World::new(2, &[DEPOSIT; 3]);
    w.provider_deposit(PROVIDER_DOMAIN, PRINCIPAL);
    w.trade(X, Y, 0, 1_000);
    w.trade(X, W, 1, 1_000);
    w.step(0, 1_040_000, 0);
    w.refresh(X);
    w.step(1, 960_000, 0);
    w.refresh_all(&[X, W]);
    assert_eq!(w.domain(PROVIDER_DOMAIN), (0, PRINCIPAL - 40 * M));
    assert_eq!(
        w.source(PROVIDER_DOMAIN).provider_receivable_num,
        40 * M * BOUND_SCALE
    );
    assert_eq!(w.domain(W_DOMAIN), (40 * M, 40 * M));
    w
}

#[test]
fn late_resolved_loss_refills_the_provider_in_every_close_order() {
    for order in ORDERS {
        let mut w = provider_is_owed_the_loser_payment();
        let fair = w.fair_wind_down();
        assert_eq!(
            fair.payouts,
            vec![DEPOSIT, DEPOSIT - 40 * M, DEPOSIT + 40 * M]
        );
        assert_eq!(fair.provider_out[PROVIDER_DOMAIN], PRINCIPAL);
        assert_eq!(fair.retired, 0);
        assert_eq!(w.wind_down(&order), fair, "close order {order:?}");
    }
}

/// The same with the loss split between two losers: each payment refills what is still
/// owed, and together they repay the provider exactly.
#[test]
fn two_late_resolved_losses_refill_one_receivable_in_every_close_order() {
    const Y2: usize = 3;
    for order in [
        [X, Y, Y2, W],
        [X, W, Y2, Y],
        [W, X, Y, Y2],
        [Y, X, W, Y2],
        [Y2, Y, X, W],
        [X, Y2, W, Y],
    ] {
        let mut w = World::new(2, &[DEPOSIT; 4]);
        w.provider_deposit(PROVIDER_DOMAIN, PRINCIPAL);
        w.trade(X, Y, 0, 500);
        w.trade(X, Y2, 0, 500);
        w.trade(X, W, 1, 1_000);
        w.step(0, 1_040_000, 0);
        w.refresh(X);
        w.step(1, 960_000, 0);
        w.refresh_all(&[X, W]);
        assert_eq!(
            w.source(PROVIDER_DOMAIN).provider_receivable_num,
            40 * M * BOUND_SCALE
        );
        let fair = w.fair_wind_down();
        assert_eq!(
            fair.payouts,
            vec![
                DEPOSIT,
                DEPOSIT - 20 * M,
                DEPOSIT + 40 * M,
                DEPOSIT - 20 * M
            ]
        );
        assert_eq!(fair.provider_out[PROVIDER_DOMAIN], PRINCIPAL);
        assert_eq!(w.wind_down(&order), fair, "close order {order:?}");
    }
}

/// Control: without a provider nothing was fronted, and the same history pays everyone
/// exactly in every close order, as it did before.
#[test]
fn control_without_a_provider_every_close_order_is_exact() {
    for order in ORDERS {
        let mut w = World::new(2, &[DEPOSIT; 3]);
        w.trade(X, Y, 0, 1_000);
        w.trade(X, W, 1, 1_000);
        w.step(0, 1_040_000, 0);
        w.refresh(X);
        w.step(1, 960_000, 0);
        w.refresh_all(&[X, W]);
        assert_eq!(w.source(PROVIDER_DOMAIN).provider_receivable_num, 0);
        let fair = w.fair_wind_down();
        assert_eq!(
            fair.payouts,
            vec![DEPOSIT, DEPOSIT - 40 * M, DEPOSIT + 40 * M]
        );
        assert_eq!(w.wind_down(&order), fair, "close order {order:?}");
    }
}
