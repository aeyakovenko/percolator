//! Issue #274: the permissionless crank's B-settlement step must settle the account's pending K/F
//! first, as every other B path does, so the order in which cranks reach a bankrupt loser and its
//! winner cannot change terminal values (spec, K/F settlement and requirement 27).
//!
//! One asset. Account 0 (the loser) is short, account 1 (the winner) is long. The mark rises at
//! the cap until the loser's loss exceeds its deposit; nobody touches an account during the rise.
//! The liquidation books the loser's shortfall into B, so the winner's next crank is planned as a
//! B chunk (B-stale outranks a refresh). Before the fix that chunk landed the winner's B share as a
//! debt ahead of its K/F gain, and the gain then cured the debt from the backing the loser had paid
//! into the winner's source domain. Every case is run in both orders and wound down.
//!
//! Written from the engine source and `tests/kf_world`.

mod kf_world;
use kf_world::*;

const W_DEP: u128 = 10_000_000;
const UNITS: u128 = 9;

fn world(l_dep: u128, provider: u128) -> World {
    let mut w = World::with_config(1, &[l_dep, W_DEP], |cfg| {
        cfg.initial_margin_bps = 1_000;
        cfg.maintenance_margin_bps = 500;
        cfg.max_price_move_bps_per_slot = 400;
        cfg.max_abs_funding_e9_per_slot = 0;
    });
    if provider != 0 {
        // A provider backs the domain the winner's gains are claimed from: the short side's
        // loss domain.
        w.provider_deposit(World::loss_domain(0, false), provider);
    }
    w.trade(1, 0, 0, UNITS);
    let mut px = PX0;
    for _ in 0..4 {
        px = px * 104 / 100;
        w.step(0, px, 0);
    }
    w
}

#[derive(Debug, PartialEq, Eq)]
struct Outcome {
    before_wind_down: Vec<(u128, i128)>,
    junior_residual: i128,
    domain1: (u128, u128),
    wind_down: WindDown,
}

fn run(l_dep: u128, provider: u128, loser_first: bool) -> Outcome {
    let mut w = world(l_dep, provider);
    if loser_first {
        w.crank(0);
        w.crank(1);
    } else {
        w.crank(1);
        w.crank(0);
        w.crank(1);
    }
    // Bring both current again, in a fixed order, before the wind-down.
    w.crank(0);
    w.crank(1);
    let before_wind_down = (0..2)
        .map(|i| (w.accts[i].capital.get(), w.accts[i].pnl.get()))
        .collect();
    let junior_residual = w.junior_residual();
    let domain1 = w.domain(1);
    let wind_down = w.wind_down(&[0, 1]);
    Outcome {
        before_wind_down,
        junior_residual,
        domain1,
        wind_down,
    }
}

fn compare(tag: &str, l_dep: u128, provider: u128) -> (Outcome, Outcome) {
    let winner_first = run(l_dep, provider, false);
    let loser_first = run(l_dep, provider, true);
    println!("{tag}: winner first: {winner_first:?}");
    println!("{tag}: loser first:  {loser_first:?}");
    (winner_first, loser_first)
}

#[test]
fn v16_settle_b_crank_order_cannot_change_terminal_values() {
    let mut differs = Vec::new();
    for (tag, l_dep, provider) in [
        ("solvent control", 2_000_000u128, 0u128),
        ("default, no provider", 1_000_000, 0),
        ("default, provider-backed domain", 1_000_000, 300_000),
        ("small default", 1_400_000, 0),
    ] {
        let (a, b) = compare(tag, l_dep, provider);
        if a != b {
            differs.push(tag);
        }
        // Absolute, not only order-equal: once both accounts are current nothing is unowned,
        // and the wind-down retires nothing (the provider takes its principal back).
        for outcome in [&a, &b] {
            assert_eq!(
                outcome.junior_residual, 0,
                "{tag}: unowned value while Live"
            );
            assert_eq!(
                outcome.wind_down.retired, 0,
                "{tag}: value retired at wind-down"
            );
            assert_eq!(
                outcome.wind_down.vault_end, 0,
                "{tag}: value left in the vault"
            );
            if provider != 0 {
                assert!(
                    outcome.wind_down.provider_out[1] + 1 >= provider,
                    "{tag}: provider short"
                );
            }
        }
    }
    assert!(
        differs.is_empty(),
        "crank order changes the outcome in: {differs:?}"
    );
}

/// Liveness beside the order: the provider's backing on the winner's source domain lapses
/// during the rise, so the winner's K/F settlement meets a lapsed bucket.
fn run_lapsed(loser_first: bool) -> Result<WindDown, String> {
    let mut w = World::with_config(1, &[1_000_000, W_DEP], |cfg| {
        cfg.initial_margin_bps = 1_000;
        cfg.maintenance_margin_bps = 500;
        cfg.max_price_move_bps_per_slot = 400;
        cfg.max_abs_funding_e9_per_slot = 0;
    });
    let domain = World::loss_domain(0, false);
    let expiry = w.slot + 3;
    w.op(|m, _| m.deposit_fresh_counterparty_backing_not_atomic(domain, 300_000, expiry))
        .map_err(|e| format!("provider deposit: {e:?}"))?;
    w.provider[domain] += 300_000;
    w.trade(1, 0, 0, UNITS);
    let mut px = PX0;
    for _ in 0..4 {
        px = px * 104 / 100;
        w.step(0, px, 0);
    }
    let order: &[usize] = if loser_first {
        &[0, 1, 0, 1]
    } else {
        &[1, 0, 1, 0, 1]
    };
    for &i in order {
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| w.crank(i)));
        if r.is_err() {
            return Err(format!(
                "crank of account {i} failed (order loser_first={loser_first})"
            ));
        }
    }
    Ok(w.wind_down(&[0, 1]))
}

#[test]
fn v16_settle_b_crank_with_lapsed_provider_backing_is_order_independent() {
    let winner_first = run_lapsed(false);
    let loser_first = run_lapsed(true);
    println!("lapsed: winner first: {winner_first:?}");
    println!("lapsed: loser first:  {loser_first:?}");
    assert!(winner_first.is_ok() && loser_first.is_ok());
    assert_eq!(winner_first, loser_first);
}

/// The loser's paid-in backing (engine-created, lifetime `h_max`) lapses after the liquidation and
/// before the winner is cranked.
fn run_lapsed_after_liquidation(loser_first: bool) -> Result<WindDown, String> {
    let mut w = World::with_config(1, &[1_000_000, W_DEP], |cfg| {
        cfg.initial_margin_bps = 1_000;
        cfg.maintenance_margin_bps = 500;
        cfg.max_price_move_bps_per_slot = 400;
        cfg.max_abs_funding_e9_per_slot = 0;
        cfg.h_max = 6;
    });
    w.trade(1, 0, 0, UNITS);
    let mut px = PX0;
    for _ in 0..4 {
        px = px * 104 / 100;
        w.step(0, px, 0);
    }
    let crank = |w: &mut World, i: usize| -> Result<(), String> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| w.crank(i))).map_err(|_| {
            format!(
                "crank of account {i} failed at slot {} (loser_first={loser_first})",
                w.slot
            )
        })
    };
    if loser_first {
        crank(&mut w, 0)?;
    } else {
        crank(&mut w, 1)?;
        crank(&mut w, 0)?;
    }
    for _ in 0..10 {
        w.step(0, px, 0);
    }
    crank(&mut w, 1)?;
    crank(&mut w, 0)?;
    crank(&mut w, 1)?;
    Ok(w.wind_down(&[0, 1]))
}

#[test]
fn v16_settle_b_crank_after_the_loser_backing_lapses_keeps_the_winner_whole() {
    let winner_first = run_lapsed_after_liquidation(false);
    let loser_first = run_lapsed_after_liquidation(true);
    println!("lapsed2: winner first: {winner_first:?}");
    println!("lapsed2: loser first:  {loser_first:?}");
    assert!(winner_first.is_ok() && loser_first.is_ok());
    assert_eq!(winner_first, loser_first);
    // The winner's gain (9 x 169,858 = 1,528,722) less its B share of the loser's shortfall
    // (528,722) is exactly the 1,000,000 the loser paid in: 11,000,000 in either order. Before
    // the fix the loser-first order paid the winner 9,471,279 and retired 1,528,721.
    let wind_down = winner_first.unwrap();
    assert_eq!(wind_down.payouts[1], 11_000_000);
    assert_eq!(wind_down.retired, 0);
}

/// Two winners against one bankrupt loser whose domain a provider backed; four crank orders.
fn two_winner_world() -> World {
    // 0 = the loser (short 9), 1 and 2 = the winners (long 4 and 5).
    let mut w = World::with_config(1, &[1_000_000, W_DEP, W_DEP], |cfg| {
        cfg.initial_margin_bps = 1_000;
        cfg.maintenance_margin_bps = 500;
        cfg.max_price_move_bps_per_slot = 400;
        cfg.max_abs_funding_e9_per_slot = 0;
    });
    w.provider_deposit(World::loss_domain(0, false), 300_000);
    w.trade(1, 0, 0, 4);
    w.trade(2, 0, 0, 5);
    let mut px = PX0;
    for _ in 0..4 {
        px = px * 104 / 100;
        w.step(0, px, 0);
    }
    w
}

#[test]
fn v16_settle_b_crank_order_is_irrelevant_with_two_winners() {
    let orders: [&[usize]; 4] = [&[0, 1, 2], &[0, 2, 1], &[1, 2, 0, 1, 2], &[1, 0, 2, 1]];
    let mut outcomes = Vec::new();
    for order in orders {
        let mut w = two_winner_world();
        for &i in order {
            w.crank(i);
        }
        for i in 0..3 {
            w.crank(i);
        }
        assert_eq!(
            w.junior_residual(),
            0,
            "order {order:?}: unowned value while Live"
        );
        let wind_down = w.wind_down(&[0, 1, 2]);
        println!("two winners, order {order:?}: {wind_down:?}");
        assert_eq!(wind_down.retired, 0, "order {order:?}");
        assert!(
            wind_down.provider_out[1] + 1 >= 300_000,
            "order {order:?}: provider short"
        );
        outcomes.push(wind_down);
    }
    // Before the fix the loser-first orders returned 1 of the provider's 300,000 atoms, and the
    // order W1, loser, W2 returned 6,265.
    assert!(
        outcomes.windows(2).all(|pair| pair[0] == pair[1]),
        "{outcomes:?}"
    );
}
