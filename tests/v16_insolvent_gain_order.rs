//! The issue #274 residual: a gain that reaches an account which is already insolvent is valued
//! against the backing its source domain holds at that moment, and burned in full unless it
//! cures the whole older loss. Cranking the winner before the losers who pay into that domain
//! burns it; the losers' payment then arrives in a domain where no claim is left to own it.
//!
//! Before engine fix C the provider exit treated that payment as the domain's surplus: once
//! every position was settled (Live) or in the resolved surplus, the domain's provider, or with
//! no provider the domain's backing authority, withdrew it. Measured on the parent with the
//! worlds below, winner first against loser first: with no backing the authority took 852,129;
//! with a 50,000 bucket the provider took 902,129; with a 300,000 bucket that lapsed, 1,000,000.
//!
//! Now the provider exit pays out provider principal and nothing beyond it, in every mode. This
//! file checks, for every world and every crank order: no domain pays out more than its provider
//! put in; a provider whose bucket stays live gets its principal back (less at most two atoms
//! that the resolved surplus rule keeps for the junior payout need); and value is conserved.
//! What the losers paid for a burned gain stays in the domain and lapses into the junior pool;
//! with no claim left short it is retired at the terminal close.
//!
//! Open (rule B): the winner's own result still depends on the order (with a 50,000 bucket it is
//! paid 852,131 when its loser is cranked first and nothing otherwise).
//!
//! Written from the engine source and `tests/kf_world`.

mod kf_world;
use kf_world::*;
use percolator::{V16Config, V16Error};

#[derive(Clone, Copy, Debug)]
enum Backing {
    /// No provider on the gain's source domain.
    None,
    /// A fresh bucket of this size for the whole history.
    Live(u128),
    /// A bucket of this size that lapses during the rise.
    Lapsing(u128),
}

fn cfg(c: &mut V16Config) {
    c.initial_margin_bps = 1_000;
    c.maintenance_margin_bps = 500;
    c.max_price_move_bps_per_slot = 400;
    c.max_abs_funding_e9_per_slot = 0;
}

fn back(w: &mut World, domain: usize, backing: Backing) {
    match backing {
        Backing::None => {}
        Backing::Live(amount) => w.provider_deposit(domain, amount),
        Backing::Lapsing(amount) => {
            let expiry = w.slot + 3;
            w.op(|m, _| m.deposit_fresh_counterparty_backing_not_atomic(domain, amount, expiry))
                .unwrap();
            w.provider[domain] += amount;
        }
    }
}

fn rise(w: &mut World, asset: usize, steps: usize) {
    let mut px = PX0;
    for _ in 0..steps {
        px = px * 104 / 100;
        w.step(asset, px, 0);
    }
}

/// Accounts: 0 = L (short asset 0), 1 = W (long asset 0, short asset 1), 2 = X (long asset 1).
/// Asset 1 rises first and W's loss there exceeds its deposit; W is refreshed and so is
/// insolvent before asset 0 rises, which makes L bankrupt and gives W a gain sourced from the
/// domain L's loss is owed to.
fn insolvent_winner(backing: Backing) -> World {
    let mut w = World::with_config(2, &[1_000_000, 2_400_000, 100_000_000], cfg);
    back(&mut w, World::loss_domain(0, false), backing);
    w.trade(1, 0, 0, 9);
    w.trade(2, 1, 1, 15);
    rise(&mut w, 1, 4);
    w.refresh(1);
    rise(&mut w, 0, 4);
    w
}

/// Accounts: 0 = A (long asset 0, short asset 1), 1 = B (short asset 0, long asset 1). Each is
/// the other's only counterparty. Asset 1 rises until A's loss to B exceeds A's deposit; A is
/// refreshed, so it is insolvent; then asset 0 rises until B's loss to A exceeds B's deposit.
/// Each one's gain is paid by the other.
fn mutual(backing: Backing) -> World {
    let mut w = World::with_config(2, &[1_800_000, 1_800_000], cfg);
    back(&mut w, World::loss_domain(0, false), backing);
    back(&mut w, World::loss_domain(1, true), backing);
    w.trade(0, 1, 0, 9);
    w.trade(1, 0, 1, 9);
    rise(&mut w, 1, 6);
    w.refresh(0);
    rise(&mut w, 0, 6);
    w
}

/// Two insolvent winners on opposite sides of asset 0, each paid by a different loser:
/// 0 = W1 (long asset 0 from 1.0, short asset 1), 1 = P1 (short asset 0 from 1.0, deep),
/// 2 = W2 (short asset 0 from the top, short asset 2), 3 = P2 (long asset 0 from the top, deep),
/// 4 = X (long assets 1 and 2, deep). Asset 0 rises, W2 and P2 trade at the top, asset 0 falls
/// back part of the way: W1 and W2 both hold a gain on asset 0 against a loser on the other side.
/// Assets 1 and 2 rise so W1 and W2 are insolvent first (both refreshed).
fn opposite_winners(backing: Backing) -> World {
    let mut w = World::with_config(
        3,
        &[2_400_000, 100_000_000, 2_700_000, 100_000_000, 100_000_000],
        cfg,
    );
    back(&mut w, World::loss_domain(0, false), backing);
    back(&mut w, World::loss_domain(0, true), backing);
    w.trade(0, 1, 0, 9);
    w.trade(4, 0, 1, 15);
    w.trade(4, 2, 2, 15);
    rise(&mut w, 0, 4);
    w.crank(0);
    w.crank(1);
    w.trade(3, 2, 0, 9);
    let top = w.px[0];
    let mut px = top;
    for _ in 0..2 {
        px = px * 98 / 100;
        w.step(0, px, 0);
    }
    rise(&mut w, 1, 4);
    rise(&mut w, 2, 4);
    w.refresh(0);
    w.refresh(2);
    w
}

/// Cranks the accounts in `order`, then each once more, winds the market down and checks the
/// provider exit and conservation. With `principal_back`, a provider whose bucket stays live
/// must also get its principal back, less at most the two atoms the resolved surplus rule may
/// keep for the junior payout need: none of its backing ever fronts a gain whose loser defaults.
fn run(
    mut w: World,
    order: &[usize],
    n: usize,
    backing: Backing,
    principal_back: bool,
    tag: &str,
) -> WindDown {
    for &i in order {
        w.crank(i);
    }
    for i in 0..n {
        w.crank(i);
    }
    let account_deposits: u128 = deposited_by_accounts(&w);
    let principal = w.provider.clone();
    let all: Vec<usize> = (0..n).collect();
    let out = w.wind_down(&all);
    if std::env::var("GAIN_ORDER_VERBOSE").is_ok() {
        println!("  {tag} {order:?}: principal {principal:?} -> {out:?}");
    }
    for (d, (taken, put)) in out.provider_out.iter().zip(principal.iter()).enumerate() {
        assert!(
            taken <= put,
            "{tag} {order:?}: domain {d} paid out {taken}, more than the {put} its provider put in"
        );
        if principal_back && matches!(backing, Backing::Live(_)) && *put != 0 {
            assert!(
                put - taken <= 2,
                "{tag} {order:?}: domain {d}'s provider got back {taken} of its {put}"
            );
        }
    }
    let paid: u128 = out.payouts.iter().sum::<u128>() + out.provider_out.iter().sum::<u128>();
    assert_eq!(
        paid + out.retired + out.vault_end,
        account_deposits + principal.iter().sum::<u128>(),
        "{tag} {order:?}: value is conserved"
    );
    out
}

/// What the accounts deposited: the world's books start each account's equity at its deposit,
/// and every position's earnings net to zero across accounts.
fn deposited_by_accounts(w: &World) -> u128 {
    let total: i128 = w.equity.iter().sum();
    assert!(total >= 0);
    total as u128
}

const W_ORDERS: [&[usize]; 4] = [&[0, 1], &[1, 0, 1], &[1, 2, 0, 1], &[2, 1, 0]];
const M_ORDERS: [&[usize]; 3] = [&[0, 1], &[1, 0], &[1, 0, 1]];
const O_ORDERS: [&[usize]; 4] = [&[1, 3, 0, 2], &[0, 2, 1, 3], &[0, 1, 2, 3], &[2, 3, 0, 1]];

#[test]
fn v16_burned_gain_backing_stays_out_of_the_provider_exit_one_insolvent_winner() {
    for backing in [
        Backing::None,
        Backing::Live(50_000),
        Backing::Live(300_000),
        Backing::Lapsing(300_000),
    ] {
        for order in W_ORDERS {
            run(
                insolvent_winner(backing),
                order,
                3,
                backing,
                true,
                &format!("insolvent winner {backing:?}"),
            );
        }
    }
}

/// Here each account's gain fronts the other's default, so a provider's own principal is at
/// risk, and the order decides how much of it comes back: with 300,000 in each domain, the
/// short-loss domain of asset 0 returns all of it when A is cranked first and 1 atom when B is
/// (the other 299,999 is retired; the same on the parent, open with rule B). Only the upper
/// bound is checked: no domain pays out more than its provider put in.
#[test]
fn v16_burned_gain_backing_stays_out_of_the_provider_exit_two_accounts_owing_each_other() {
    for backing in [
        Backing::None,
        Backing::Live(300_000),
        Backing::Lapsing(300_000),
    ] {
        for order in M_ORDERS {
            run(
                mutual(backing),
                order,
                2,
                backing,
                false,
                &format!("mutual {backing:?}"),
            );
        }
    }
}

#[test]
fn v16_burned_gain_backing_stays_out_of_the_provider_exit_opposite_winners() {
    for backing in [Backing::None, Backing::Live(300_000)] {
        for order in O_ORDERS {
            run(
                opposite_winners(backing),
                order,
                5,
                backing,
                true,
                &format!("opposite winners {backing:?}"),
            );
        }
    }
}

/// Live, every position settled after the winner was cranked first: the loser's payment sits in
/// the domain under no claim. The provider exit refuses everything past provider principal,
/// before it touches a byte, and pays the principal out.
#[test]
fn v16_live_provider_exit_after_a_burned_gain_is_capped_at_principal() {
    for (backing, principal) in [(Backing::None, 0u128), (Backing::Live(50_000), 50_000)] {
        let mut w = insolvent_winner(backing);
        for i in [1, 0, 1, 0, 1, 2] {
            w.crank(i);
        }
        let d = World::loss_domain(0, false);
        let (claims, held) = w.domain(d);
        assert_eq!(
            claims, 0,
            "{backing:?}: the burned gain left no claim on the domain"
        );
        assert!(
            held > principal + 800_000,
            "{backing:?}: the loser's payment sits in the domain: {held}"
        );
        let before = (w.header, w.markets.clone());
        for amount in [held, principal + 1] {
            assert_eq!(
                w.op(|m, _| m.withdraw_fresh_counterparty_backing_not_atomic(d, amount)),
                Err(V16Error::LockActive),
                "{backing:?}: {amount} is past provider principal"
            );
            assert!(w.header == before.0 && w.markets == before.1);
        }
        if principal != 0 {
            w.op(|m, _| m.withdraw_fresh_counterparty_backing_not_atomic(d, principal))
                .expect("a provider takes out what it put in");
            assert_eq!(w.domain(d), (0, held - principal));
        }
    }
}
