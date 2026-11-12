//! A K/F loss met by the account's prior positive face must still pay the domain it is owed
//! to (issue #175).
//!
//! Every K/F loss is owed to one source domain L: the opposite side's gain is claimed from L.
//! An account may pay that loss from capital or by giving up claims it holds elsewhere, but
//! either way L must end up with the backing. The original report (August) showed a loss
//! netted against a same-refresh gain reserving nothing for L, so L's provider funded the
//! winner and was never repaid. Whole-account settlement order (losses before gains) and the
//! Live pre-support netting have since closed that for Live refreshes; the first group of
//! tests pins it.
//!
//! What remained: the pre-support netting is Live-only, so a loss first crystallized in
//! Resolved mode against prior face took the plain support-and-burn path, whose consumed
//! counterparty backing went to the junior residual with nothing booked for L. With a
//! provider in L the winner was then paid from the provider's principal and the consumed
//! backing was retired at terminal: the report's "provider remains unpaid after resolution".
//! Support consumed by a K/F loss is now booked for L by the same rule as the capital-paid
//! part of that loss (Live: L in full; Resolved: the S2 rule).
mod kf_world;

use kf_world::{fully_cranked, play, random_history, Op, Rng, Shape, World, DEPOSIT};

const M: u128 = 1_000_000;

// ---------------------------------------------------------------- as reported: one refresh

/// Two assets. The taker (0) is long 100 of each against the LP (1). Asset 0 falls 1% and
/// asset 1 rises 1%: the taker loses 1M on asset 0 (owed to domain 0) and gains 1M on asset 1
/// (claimed from domain 3); the LP is the mirror image (loses to domain 3, gains from
/// domain 0). A provider backs `provider_domain`. `asset0_first` picks which leg is attached
/// first (the lower portfolio slot), `taker_first` who is refreshed first.
fn mirrored_legs(provider_domain: usize, asset0_first: bool, taker_first: bool) -> World {
    const TAKER: usize = 0;
    const LP: usize = 1;
    let mut w = World::new(2, &[DEPOSIT; 2]);
    w.provider_deposit(provider_domain, 10 * M);
    for asset in if asset0_first { [0, 1] } else { [1, 0] } {
        w.trade(TAKER, LP, asset, 100);
    }
    w.step(0, 990_000, 0);
    w.step(1, 1_010_000, 0);
    w.refresh_all(&if taker_first {
        [TAKER, LP]
    } else {
        [LP, TAKER]
    });
    w
}

/// The report's matrix: both leg attachment orders, both refresh orders, a provider in each of
/// the four domains. Whichever leg comes first, each loss is paid from capital into its own
/// domain before the same-refresh gain is credited, so every domain is exactly backed, both
/// accounts can realize their gain, the provider can take its principal back at any time, and
/// the outcome does not depend on the attachment order.
#[test]
fn same_refresh_gain_and_loss_legs_back_each_loss_domain_in_any_leg_order() {
    for provider_domain in 0..4 {
        let mut outcomes = Vec::new();
        for asset0_first in [true, false] {
            for taker_first in [true, false] {
                let mut w = mirrored_legs(provider_domain, asset0_first, taker_first);
                let tag = format!(
                    "provider in domain {provider_domain}, asset 0 first {asset0_first}, taker first {taker_first}\n{}",
                    w.describe()
                );
                for i in 0..2 {
                    assert_eq!(w.accts[i].capital.get(), DEPOSIT - M, "{tag}");
                    assert_eq!(w.accts[i].pnl.get(), M as i128, "{tag}");
                }
                // The taker's gain is claimed from domain 3, the LP's from domain 0.
                assert_eq!((w.claim(0, 3), w.claim(1, 0)), (M, M), "{tag}");
                for d in 0..4 {
                    let claims = if d == 0 || d == 3 { M } else { 0 };
                    assert_eq!(w.domain(d), (claims, w.provider[d] + claims), "{tag}");
                }
                w.assert_settled(&tag);
                // Live: the provider can withdraw its whole principal, and both claims convert
                // in full once the accounts are flat.
                w.op(|m, _| {
                    m.withdraw_fresh_counterparty_backing_not_atomic(provider_domain, 10 * M)
                })
                .expect("the provider's principal is not needed by any claim");
                w.provider[provider_domain] = 0;
                w.trade(1, 0, 0, 100);
                w.trade(1, 0, 1, 100);
                for i in 0..2 {
                    // A conversion needs the account's own certificate to be current.
                    w.refresh(i);
                    assert_eq!(w.convert(i), Ok(M), "{tag}");
                    assert_eq!(w.accts[i].capital.get(), DEPOSIT, "{tag}");
                    w.op(|m, a| m.withdraw_not_atomic(&mut a[i], DEPOSIT))
                        .expect("full withdrawal");
                }
                assert_eq!(w.header.vault.get(), 0, "{tag}");
                outcomes.push((w.header, w.markets[0].engine, w.markets[1].engine));
            }
        }
        // Same positions, marks, deposits and refresh order: the leg attachment order
        // changes nothing at all.
        for taker_first in 0..2 {
            assert!(
                outcomes[taker_first] == outcomes[2 + taker_first],
                "provider in domain {provider_domain}: attachment order changed the outcome"
            );
        }
    }
}

/// The report's scale control: at 10% margin each account deposits 10M and holds two 50-unit
/// legs opened at 1.00 (the taker long both, the LP short both). Asset 0 walks down to 0.80
/// and asset 1 up to 1.20, and each account is then refreshed once: its 10M loss consumes its
/// whole deposit and its 10M gain stays as PnL. Both gains are fully backed by the other
/// account's payment, so neither is locked: flat, each converts its 10M and withdraws it, in
/// either leg attachment order and either refresh order.
#[test]
fn same_refresh_legs_that_consume_the_whole_deposit_leave_both_gains_backed() {
    for asset0_first in [true, false] {
        for taker_first in [true, false] {
            let mut w = World::with_config(2, &[10 * M; 2], |cfg| {
                cfg.maintenance_margin_bps = 1_000;
                cfg.initial_margin_bps = 1_000;
            });
            for asset in if asset0_first { [0, 1] } else { [1, 0] } {
                w.trade(0, 1, asset, 50);
            }
            for px in [950_000, 905_000, 860_000, 820_000, 800_000] {
                w.step(0, px, 0);
            }
            for px in [1_050_000, 1_100_000, 1_150_000, 1_200_000] {
                w.step(1, px, 0);
            }
            w.refresh_all(&if taker_first { [0, 1] } else { [1, 0] });
            let tag = format!(
                "asset 0 first {asset0_first}, taker first {taker_first}\n{}",
                w.describe()
            );
            for i in 0..2 {
                assert_eq!(w.accts[i].capital.get(), 0, "{tag}");
                assert_eq!(w.accts[i].pnl.get(), 10 * M as i128, "{tag}");
            }
            // The taker lost on asset 0 (domain 0) and gained on asset 1 (domain 3).
            assert_eq!(w.domain(0), (10 * M, 10 * M), "{tag}");
            assert_eq!(w.domain(3), (10 * M, 10 * M), "{tag}");
            w.assert_settled(&tag);
            w.trade(1, 0, 0, 50);
            w.trade(1, 0, 1, 50);
            for i in 0..2 {
                w.refresh(i);
                assert_eq!(w.convert(i), Ok(10 * M), "{tag}");
                w.op(|m, a| m.withdraw_not_atomic(&mut a[i], 10 * M))
                    .expect("full withdrawal");
            }
            assert_eq!(w.header.vault.get(), 0, "{tag}");
        }
    }
}

// ------------------------------------------------------ Live: the loss arrives a refresh later

/// X (0) is long 1,000 of asset 0 against Y (1) and long 1,000 of asset 1 against W (2).
/// Asset 0 rises 4% and X is refreshed (with Y when `y_settled`): X holds 40M of face in
/// domain 1. Asset 1 then falls 4%: X owes 40M to domain 2, where W's gain is claimed.
fn gain_then_loss(provider: &[(usize, u128)], y_settled: bool) -> World {
    let mut w = World::new(2, &[DEPOSIT; 3]);
    for (d, amount) in provider {
        w.provider_deposit(*d, *amount);
    }
    w.trade(0, 1, 0, 1_000);
    w.trade(0, 2, 1, 1_000);
    w.step(0, 1_040_000, 0);
    w.refresh(0);
    if y_settled {
        w.refresh(1);
    }
    assert_eq!(w.claim(0, 1), 40 * M);
    w.step(1, 960_000, 0);
    w
}

const XYW_ORDERS: [[usize; 3]; 6] = [
    [0, 1, 2],
    [0, 2, 1],
    [1, 0, 2],
    [1, 2, 0],
    [2, 0, 1],
    [2, 1, 0],
];
const PROVIDERS: [&[(usize, u128)]; 4] = [
    &[],
    &[(1, 1_000 * M)],
    &[(2, 1_000 * M)],
    &[(1, 1_000 * M), (2, 1_000 * M)],
];

/// Live control (unchanged behaviour): whether X's face is fully backed when the loss arrives
/// (its backing moves to the loss domain) or not yet (X pays from capital and keeps the
/// face), domain 2 receives the whole 40M and everything settles exactly.
#[test]
fn live_cross_asset_loss_against_prior_face_backs_the_loss_domain() {
    for provider in PROVIDERS {
        for y_settled in [true, false] {
            for order in XYW_ORDERS {
                let mut w = gain_then_loss(provider, y_settled);
                w.refresh_all(&order);
                let tag = format!(
                    "provider {provider:?}, Y settled at the peak {y_settled}, refresh order {order:?}"
                );
                assert_eq!(w.engine_equity(0), DEPOSIT as i128, "{tag}");
                assert_eq!(w.claim(2, 2), 40 * M, "{tag}");
                let (claims, backing) = w.domain(2);
                assert_eq!(backing, w.provider[2] + claims, "{tag}\n{}", w.describe());
                w.assert_settled(&tag);
                assert_eq!(w.wind_down(&order), w.fair_wind_down(), "{tag}");
            }
        }
    }
}

/// Live, a loss beyond free capital. At 10% margin X (0) holds 100M of capital, is long 600 of
/// asset 0 against Y (1) and long 400 of asset 1 against W (2). Asset 0 rises 4% and only X is
/// refreshed: its 24M of face in domain 1 is half backed by a 12M provider there. Asset 1 then
/// falls to 0.745 without X being liquidated: X owes 102M to domain 2. Free capital pays 100M
/// and X's face pays the last 2M through support-and-burn (4M of half-backed face for 2M of
/// support, the unchanged rate of that path). The 2M of counterparty backing that support
/// consumed is owed to domain 2 like the rest of the loss: W's 102M gain is fully backed there
/// and nothing is left in the junior residual, so domain 2's provider is not drawn on.
#[test]
fn live_loss_beyond_free_capital_books_its_consumed_support_for_the_loss_domain() {
    for loss_domain_provider in [0, 1_000 * M] {
        let mut w = World::with_config(2, &[100 * M, DEPOSIT, DEPOSIT], |cfg| {
            cfg.maintenance_margin_bps = 1_000;
            cfg.initial_margin_bps = 1_000;
        });
        w.provider_deposit(1, 12 * M);
        if loss_domain_provider != 0 {
            w.provider_deposit(2, loss_domain_provider);
        }
        w.trade(0, 1, 0, 600);
        w.trade(0, 2, 1, 400);
        w.step(0, 1_040_000, 0);
        w.refresh(0);
        assert_eq!(w.claim(0, 1), 24 * M);
        assert_eq!(w.domain(1), (24 * M, 12 * M));
        for px in [950_000, 905_000, 860_000, 820_000, 780_000, 745_000] {
            w.step(1, px, 0);
        }
        w.refresh(0);
        let tag = format!(
            "provider in the loss domain {loss_domain_provider}\n{}",
            w.describe()
        );
        assert_eq!(w.accts[0].capital.get(), 0, "{tag}");
        assert_eq!(w.claim(0, 1), 20 * M, "{tag}");
        // Domain 1 gave up 2M of the provider's 12M for the 4M of face it burned.
        assert_eq!(w.domain(1), (20 * M, 10 * M), "{tag}");
        // Domain 2 received X's whole 102M: 100M of capital and the 2M of support.
        assert_eq!(w.domain(2), (0, loss_domain_provider + 102 * M), "{tag}");
        assert_eq!(w.junior_residual(), 0, "{tag}");
        w.refresh(2);
        assert_eq!(w.claim(2, 2), 102 * M, "{tag}");
        assert_eq!(
            w.domain(2),
            (102 * M, loss_domain_provider + 102 * M),
            "{tag}"
        );
        assert_eq!(
            w.source(2).credit_rate_num,
            percolator::CREDIT_RATE_SCALE,
            "{tag}"
        );
        assert_eq!(w.junior_residual(), 0, "{tag}");
    }
}

// ------------------------------------------------- Resolved: the loss is first crystallized late

/// The same world resolved right after asset 1 falls, with only `live` refreshed in Live
/// after that fall. Whoever was not refreshed crystallizes its asset-1 K/F in Resolved mode.
fn resolved_after_loss(provider: &[(usize, u128)], live: &[usize]) -> World {
    let mut w = gain_then_loss(provider, true);
    w.refresh_all(live);
    w
}

/// The reproduction. X's 40M loss on asset 1 is first crystallized in Resolved mode and is met
/// by the 40M of fully backed face X holds in domain 1. Domain 2 must still receive the 40M:
/// with a provider there, W is paid from the domain and the provider takes its whole principal
/// back. (Before the fix the provider recovered 960M of 1,000M and 40M was retired, in every
/// close order, unless X happened to be refreshed in Live first.)
#[test]
fn resolved_cross_asset_loss_against_prior_face_repays_the_loss_domain_provider() {
    for provider in PROVIDERS {
        for live in [&[][..], &[0], &[2], &[0, 2]] {
            for order in XYW_ORDERS {
                let mut w = resolved_after_loss(provider, live);
                let fair = w.fair_wind_down();
                assert_eq!(
                    fair.payouts,
                    vec![DEPOSIT, DEPOSIT - 40 * M, DEPOSIT + 40 * M]
                );
                assert_eq!(
                    w.wind_down(&order),
                    fair,
                    "provider {provider:?}, refreshed in Live {live:?}, close order {order:?}"
                );
            }
        }
    }
}

/// The same loss as funding instead of a mark move: asset 1 stays at 1.00 and longs pay 10
/// atoms per unit per slot for five slots, so X owes 50,000 to domain 2.
#[test]
fn resolved_cross_asset_funding_loss_against_prior_face_repays_the_loss_domain_provider() {
    for provider in PROVIDERS {
        for live in [&[][..], &[0], &[2]] {
            for order in XYW_ORDERS {
                let mut w = World::new(2, &[DEPOSIT; 3]);
                for (d, amount) in provider {
                    w.provider_deposit(*d, *amount);
                }
                w.trade(0, 1, 0, 1_000);
                w.trade(0, 2, 1, 1_000);
                w.step(0, 1_040_000, 0);
                w.refresh_all(&[0, 1]);
                for _ in 0..5 {
                    w.step(1, 1_000_000, 10_000);
                }
                w.refresh_all(live);
                let fair = w.fair_wind_down();
                assert_eq!(
                    fair.payouts,
                    vec![
                        DEPOSIT + 40 * M - 50_000,
                        DEPOSIT - 40 * M,
                        DEPOSIT + 50_000
                    ]
                );
                assert_eq!(
                    w.wind_down(&order),
                    fair,
                    "provider {provider:?}, refreshed in Live {live:?}, close order {order:?}"
                );
            }
        }
    }
}

/// A loss larger and a loss smaller than the face, first crystallized in Resolved mode: the
/// face pays what it covers (its backing booked for domain 2), capital pays the rest, and the
/// provider of either domain is whole.
#[test]
fn resolved_cross_asset_loss_larger_or_smaller_than_the_face() {
    for (asset1_units, x_end) in [(2_000u128, DEPOSIT - 40 * M), (500, DEPOSIT + 20 * M)] {
        for provider in PROVIDERS {
            for order in XYW_ORDERS {
                let mut w = World::new(2, &[DEPOSIT; 3]);
                for (d, amount) in provider {
                    w.provider_deposit(*d, *amount);
                }
                w.trade(0, 1, 0, 1_000);
                w.trade(0, 2, 1, asset1_units);
                w.step(0, 1_040_000, 0);
                w.refresh_all(&[0, 1]);
                w.step(1, 960_000, 0);
                let fair = w.fair_wind_down();
                assert_eq!(fair.payouts[0], x_end);
                assert_eq!(
                    w.wind_down(&order),
                    fair,
                    "{asset1_units} units, provider {provider:?}, close order {order:?}"
                );
            }
        }
    }
}

/// Issue #252's scenario with the last move first crystallized in Resolved mode: A's loss is
/// met by its own face in the loss domain. The backing its face consumes is booked straight
/// back for that domain, so the provider is whole here too.
#[test]
fn resolved_loss_against_own_face_in_the_loss_domain_repays_the_provider() {
    for live in [&[][..], &[0], &[1], &[2], &[1, 2]] {
        for order in XYW_ORDERS {
            let mut w = World::new(1, &[DEPOSIT; 3]);
            w.provider_deposit(0, 1_000 * M);
            w.trade(1, 0, 0, 1_000);
            w.step(0, 960_000, 0);
            w.refresh_all(&[0, 1, 2]);
            w.trade(0, 2, 0, 2_000);
            w.step(0, 920_000, 0);
            w.refresh_all(live);
            let fair = w.fair_wind_down();
            assert_eq!(
                fair.payouts,
                vec![DEPOSIT, DEPOSIT - 80 * M, DEPOSIT + 80 * M]
            );
            assert_eq!(
                w.wind_down(&order),
                fair,
                "refreshed in Live {live:?}, close order {order:?}"
            );
        }
    }
}

/// Same leg, first crystallized in Resolved mode. X (0) is long 1,000 against Y (1); the mark
/// rises 4% and both settle (Y pays 40M into domain 1, X claims it), then falls back to 1.01
/// and the market resolves. X's 30M reversal loss is owed to domain 0, where Y's 30M regain is
/// claimed: the loser-paid backing behind X's face is booked there, so a provider in either
/// domain is whole.
#[test]
fn resolved_same_leg_reversal_against_loser_paid_twin_face_backs_the_regain() {
    let providers: [&[(usize, u128)]; 4] = [
        &[],
        &[(0, 1_000 * M)],
        &[(1, 1_000 * M)],
        &[(0, 1_000 * M), (1, 1_000 * M)],
    ];
    for provider in providers {
        for live in [&[][..], &[0], &[1]] {
            for order in [[0usize, 1], [1, 0]] {
                let mut w = World::new(1, &[DEPOSIT; 2]);
                for (d, amount) in provider {
                    w.provider_deposit(*d, *amount);
                }
                w.trade(0, 1, 0, 1_000);
                w.step(0, 1_040_000, 0);
                w.refresh_all(&[0, 1]);
                w.step(0, 1_010_000, 0);
                w.refresh_all(live);
                let fair = w.fair_wind_down();
                assert_eq!(fair.payouts, vec![DEPOSIT + 10 * M, DEPOSIT - 10 * M]);
                assert_eq!(
                    w.wind_down(&order),
                    fair,
                    "provider {provider:?}, refreshed in Live {live:?}, close order {order:?}"
                );
            }
        }
    }
}

/// Liened face, first crystallized in Resolved mode. At 50% initial margin X (0) holds 520M,
/// is long 1,000 of asset 0 against Y (1) and, after a 4% rise that both settle, buys 80 of
/// asset 1 from W (2): the added initial margin liens X's whole 40M of face. Asset 1 falls 4%
/// and the market resolves. A resolved close releases the lien before it settles the loss,
/// so the 3.2M the face pays is consumed and booked for domain 2 like any other support.
/// (The same loss settled in Live still strands that backing: see the open-defect
/// reproducers.)
#[test]
fn resolved_loss_against_liened_face_repays_the_loss_domain_provider() {
    for provider in PROVIDERS {
        for live in [&[][..], &[2], &[1, 2]] {
            for order in XYW_ORDERS {
                let mut w = World::with_config(2, &[520 * M, DEPOSIT, DEPOSIT], |cfg| {
                    cfg.maintenance_margin_bps = 1_000;
                    cfg.initial_margin_bps = 5_000;
                });
                for (d, amount) in provider {
                    w.provider_deposit(*d, *amount);
                }
                w.trade(0, 1, 0, 1_000);
                w.step(0, 1_040_000, 0);
                w.refresh_all(&[0, 1]);
                w.trade(0, 2, 1, 80);
                w.step(1, 960_000, 0);
                w.refresh_all(live);
                let fair = w.fair_wind_down();
                assert_eq!(
                    fair.payouts,
                    vec![556_800_000, DEPOSIT - 40 * M, DEPOSIT + 3_200_000]
                );
                assert_eq!(
                    w.wind_down(&order),
                    fair,
                    "provider {provider:?}, refreshed in Live {live:?}, close order {order:?}"
                );
            }
        }
    }
}

// --------------------------------------------------------------------- random funded markets

/// Random markets in which every domain has a provider. The history is fully cranked; then
/// each asset makes one last move that only a random subset of accounts settles in Live, the
/// market resolves and the accounts close in a random order. Wherever each loss is first
/// crystallized, every trader is paid its equity, every provider takes its whole principal
/// back and nothing is retired.
#[test]
fn funded_markets_wind_down_fairly_wherever_the_last_losses_crystallize() {
    for seed in 1..=200u64 {
        let (mut shape, ops) = random_history(seed, 40, seed % 2 == 0);
        shape.providers = (0..shape.assets * 2).map(|d| (d, 5_000 * M)).collect();
        let mut ops = fully_cranked(ops);
        let mut r = Rng::new(seed ^ 0x5eed);
        for asset in 0..shape.assets {
            ops.push(Op::Step {
                asset,
                delta: 10_000 * (1 + r.below(3) as i64) * if r.below(2) == 0 { 1 } else { -1 },
                funding_e9: 0,
            });
        }
        for i in 0..shape.accounts {
            if r.below(3) == 0 {
                ops.push(Op::Refresh(i));
            }
        }
        let mut order: Vec<usize> = (0..shape.accounts).collect();
        for i in (1..order.len()).rev() {
            order.swap(i, r.below(i as u64 + 1) as usize);
        }
        let outcome = wind_down_outcome(&shape, &ops, &order);
        assert!(
            outcome.is_ok(),
            "seed {seed}: {}\nworld {shape:?}\nclose order {order:?}\nhistory {ops:#?}",
            outcome.unwrap_err()
        );
    }
}

fn wind_down_outcome(shape: &Shape, ops: &[Op], close_order: &[usize]) -> Result<(), String> {
    let mut w = play(shape, ops)?;
    let fair = w.fair_wind_down();
    let got = w.wind_down(close_order);
    if got == fair {
        Ok(())
    } else {
        Err(format!(
            "wind-down was\n  {got:?}\nbut the fair one is\n  {fair:?}"
        ))
    }
}
