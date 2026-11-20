//! The provider-protection floor of a source domain (percolator-prog#382).
//!
//! A backing provider's protection is the principal that is still its own:
//! `min(principal, fresh backing - exact claims)`. Public transitions lower it (a
//! gain is registered as a claim before its loser has paid), losers refill it,
//! and a later claim spends the refill again. A program that keeps depositor
//! shares on top of one provider position can only look at the market when a
//! depositor acts, so a low that was refilled before the next look is gone from
//! the current state, and the same loss is charged once or twice depending on
//! when somebody looked.
//!
//! The engine therefore keeps, per source domain and per asset generation, the
//! lowest protection the domain has had, moved only by the provider's own
//! deposits and principal withdrawals. Everything here goes through the public
//! `*_not_atomic` API, one mutable view per call.
mod kf_world;

use kf_world::{random_history, Op, Rng, World, BACKING_LIFETIME, DEPOSIT, PX0};
use percolator::{AssetLifecycleV16, MarketGroupV16ViewMut, SideV16, V16Error, BOUND_SCALE};

fn side(domain: usize) -> SideV16 {
    if domain % 2 == 0 {
        SideV16::Long
    } else {
        SideV16::Short
    }
}

/// `(protection now, floor)` of a domain, BOUND_SCALE-scaled, as a reader of the
/// market sees them.
fn protection_and_floor_num(w: &World, domain: usize) -> (u128, u128) {
    w.markets[domain / 2]
        .engine
        .provider_protection_and_floor_num(side(domain))
}

/// The same in whole atoms.
fn protection_and_floor(w: &World, domain: usize) -> (u128, u128) {
    let (protection, floor) = protection_and_floor_num(w, domain);
    assert_eq!(protection % BOUND_SCALE, 0);
    assert_eq!(floor % BOUND_SCALE, 0);
    (protection / BOUND_SCALE, floor / BOUND_SCALE)
}

/// The engine's provider principal of a domain, in atoms.
fn principal(w: &World, domain: usize) -> u128 {
    let e = &w.markets[domain / 2].engine;
    let num = if domain % 2 == 0 {
        e.provider_principal_long_num.get()
    } else {
        e.provider_principal_short_num.get()
    };
    assert_eq!(num % BOUND_SCALE, 0);
    num / BOUND_SCALE
}

/// The stored floor of a domain, in atoms: what the slot holds, before the reader
/// takes the lower of it and the current protection.
fn stored_floor(w: &World, domain: usize) -> u128 {
    let e = &w.markets[domain / 2].engine;
    let num = if domain % 2 == 0 {
        e.provider_protection_floor_long_num.get()
    } else {
        e.provider_protection_floor_short_num.get()
    };
    assert_eq!(num % BOUND_SCALE, 0);
    num / BOUND_SCALE
}

fn deposit(w: &mut World, domain: usize, amount: u128) {
    w.provider_deposit(domain, amount);
}

fn try_deposit(w: &mut World, domain: usize, amount: u128) -> Result<(), V16Error> {
    w.op(|m, _| m.deposit_fresh_counterparty_backing_not_atomic(domain, amount, BACKING_LIFETIME))?;
    w.provider[domain] += amount;
    Ok(())
}

fn withdraw(w: &mut World, domain: usize, amount: u128) -> Result<(), V16Error> {
    w.op(|m, _| m.withdraw_fresh_counterparty_backing_not_atomic(domain, amount))?;
    w.provider[domain] = w.provider[domain].saturating_sub(amount);
    Ok(())
}

/// One asset. Account 0 is long 2 units against accounts 1 and 2 (short 1 each),
/// account 3 is long 1 against account 4. A provider put `principal` atoms into
/// domain 1, which the shorts' losses are paid into and the longs' gains are
/// claimed from. The mark then rises by one atom per unit and nobody is
/// refreshed: account 0 has gained 2, account 3 has gained 1, and accounts 1, 2
/// and 4 owe 1 each.
fn staggered_world(principal: u128) -> World {
    let mut w = World::new(1, &[DEPOSIT; 5]);
    deposit(&mut w, 1, principal);
    w.trade(0, 1, 0, 1);
    w.trade(0, 2, 0, 1);
    w.trade(3, 4, 0, 1);
    w.step(0, PX0 + 1, 0);
    assert_eq!(w.domain(1), (0, principal));
    assert_eq!(protection_and_floor(&w, 1), (principal, principal));
    w
}

/// Required test 1. Protection goes 16 -> 14 through a public source claim, is
/// refilled 14 -> 15 by a loser before anybody looks, and a second claim spends
/// the refill 15 -> 14. The floor is 14 throughout.
#[test]
fn a_refilled_loss_stays_recorded_and_spending_the_refill_records_nothing() {
    let mut w = staggered_world(16);

    // The winner is refreshed first: its 2 atoms are a claim that only the
    // provider's principal stands behind.
    w.refresh(0);
    assert_eq!(w.domain(1), (2, 16));
    assert_eq!(protection_and_floor(&w, 1), (14, 14));

    // One loser pays. The current state shows 15, and no field of the source
    // credit or of the bucket says it was ever lower.
    w.refresh(1);
    assert_eq!(w.domain(1), (2, 17));
    assert_eq!(protection_and_floor(&w, 1), (15, 14));
    assert_eq!(w.source(1).spent_backing_num, 0);
    assert_eq!(w.source(1).provider_receivable_num, 0);
    assert_eq!(w.bucket(1).consumed_liened_backing_num, 0);
    assert_eq!(w.bucket(1).impaired_liened_backing_num, 0);

    // A second winner's claim spends the refill. Back at 14: the same low, not a
    // second loss.
    w.refresh(3);
    assert_eq!(w.domain(1), (3, 17));
    assert_eq!(protection_and_floor(&w, 1), (14, 14));

    // The other two losers pay: the provider is whole again and the floor stays.
    w.refresh(2);
    w.refresh(4);
    assert_eq!(w.domain(1), (3, 19));
    assert_eq!(protection_and_floor(&w, 1), (16, 14));
    assert_eq!(principal(&w, 1), 16);
    w.assert_settled("staggered refreshes");
}

/// Required test 3, for a conversion. Taking a gain out consumes backing and
/// burns the claim it paid, which leaves protection where it was. With protection
/// sitting exactly on the floor, nothing is recorded: the loss was recorded when
/// the claim was registered.
///
/// Account 0 is long 2 against accounts 1 and 2, account 3 is long 2 against
/// account 4. The mark rises by one. Account 0 is refreshed and then closes, so
/// its two counterparties pay; account 3 is refreshed, so its 2 atoms are fronted
/// by the provider again. Then account 0 converts.
#[test]
fn a_conversion_at_the_floor_is_not_a_second_loss() {
    let mut w = World::new(1, &[DEPOSIT; 5]);
    deposit(&mut w, 1, 16);
    w.trade(0, 1, 0, 1);
    w.trade(0, 2, 0, 1);
    w.trade(3, 4, 0, 2);
    w.step(0, PX0 + 1, 0);

    w.refresh(0);
    assert_eq!(protection_and_floor(&w, 1), (14, 14));
    w.trade(1, 0, 0, 1);
    w.trade(2, 0, 0, 1);
    assert_eq!(w.domain(1), (2, 18));
    assert_eq!(protection_and_floor(&w, 1), (16, 14));
    w.refresh(3);
    assert_eq!(w.domain(1), (4, 18));
    assert_eq!(protection_and_floor(&w, 1), (14, 14));

    // Account 0 takes its 2 atoms: 2 of backing are consumed and 2 of claim burned.
    w.refresh(0);
    assert_eq!(stored_floor(&w, 1), 14);
    assert_eq!(w.convert(0), Ok(2));
    assert_eq!(w.domain(1), (2, 16));
    assert_eq!(protection_and_floor(&w, 1), (14, 14));
    assert_eq!(stored_floor(&w, 1), 14);

    // The last loser pays, and account 3 closes and converts from above the floor.
    w.refresh(4);
    assert_eq!(protection_and_floor(&w, 1), (16, 14));
    w.trade(4, 3, 0, 2);
    w.refresh(3);
    assert_eq!(w.convert(3), Ok(2));
    assert_eq!(w.domain(1), (0, 16));
    assert_eq!(protection_and_floor(&w, 1), (16, 14));
    assert_eq!(stored_floor(&w, 1), 14);
    assert_eq!(principal(&w, 1), 16);
    w.assert_settled("conversions");
}

/// The floor is taken between calls, not between the writes of one call. A trade
/// that closes a winner against its loser settles both: the gain is registered as
/// a claim, and then the loss that pays it arrives. Between those two writes the
/// domain reads lower by the gain, and after the call it reads what it read
/// before. Nobody could have looked in between, so nothing is recorded.
///
/// This is the smallest history on which an engine that records at every write
/// differs: it leaves the floor 16,000,000 atoms short of the provider's
/// 4,000,000,000 (found by shrinking random histories against such an engine).
#[test]
fn a_gain_and_the_loss_that_pays_it_settled_in_one_call_record_nothing() {
    const PRINCIPAL: u128 = 4_000_000_000;
    for winner_is_short in [true, false] {
        let mut w = World::new(1, &[DEPOSIT; 3]);
        // The losing side's domain: the longs' when the mark falls.
        let domain = World::loss_domain(0, winner_is_short);
        deposit(&mut w, domain, PRINCIPAL);
        let (long, short) = (2, 0);
        w.trade(long, short, 0, 800);
        let px = if winner_is_short {
            PX0 - 20_000
        } else {
            PX0 + 20_000
        };
        w.step(0, px, 0);
        assert_eq!(protection_and_floor(&w, domain), (PRINCIPAL, PRINCIPAL));

        // Both close against each other at the mark: 16,000,000 change hands.
        w.trade(short, long, 0, 800);
        assert_eq!(w.domain(domain), (16_000_000, PRINCIPAL + 16_000_000));
        assert_eq!(
            protection_and_floor(&w, domain),
            (PRINCIPAL, PRINCIPAL),
            "winner short: {winner_is_short}"
        );
        assert_eq!(stored_floor(&w, domain), PRINCIPAL);
        w.assert_settled("closed in one call");

        // The same gain refreshed on its own, before its loser, is a low between
        // calls and is recorded.
        let mut w = World::new(1, &[DEPOSIT; 3]);
        deposit(&mut w, domain, PRINCIPAL);
        w.trade(long, short, 0, 800);
        w.step(0, px, 0);
        w.refresh(if winner_is_short { short } else { long });
        assert_eq!(
            protection_and_floor(&w, domain),
            (PRINCIPAL - 16_000_000, PRINCIPAL - 16_000_000)
        );
        w.refresh(if winner_is_short { long } else { short });
        assert_eq!(
            protection_and_floor(&w, domain),
            (PRINCIPAL, PRINCIPAL - 16_000_000)
        );
    }
}

/// Required test 2, engine half. Whether something writes the domain at the low,
/// after the refill, or not at all until the end, the floor is the same. The
/// write is a provider deposit of one atom and its withdrawal, which leave
/// principal and protection where they were.
#[test]
fn the_floor_does_not_depend_on_when_the_domain_is_next_written() {
    let run = |touch_at: Option<usize>| {
        let mut w = staggered_world(16);
        let mut seen = Vec::new();
        for (step, account) in [0usize, 1, 3, 2, 4].into_iter().enumerate() {
            w.refresh(account);
            if touch_at == Some(step) {
                deposit(&mut w, 1, 1);
                withdraw(&mut w, 1, 1).unwrap();
            }
            seen.push(protection_and_floor(&w, 1));
        }
        (seen, stored_floor(&w, 1), principal(&w, 1))
    };
    let untouched = run(None);
    assert_eq!(
        untouched.0,
        vec![(14, 14), (15, 14), (14, 14), (15, 14), (16, 14)]
    );
    for step in 0..5 {
        assert_eq!(run(Some(step)).0, untouched.0, "touched after step {step}");
        assert_eq!(run(Some(step)).2, 16);
    }
    // Untouched, the stored value is whatever the last write found; the reader
    // still sees 14. Touched after the last refresh, the stored value is 14 too.
    assert_eq!(run(Some(4)).1, 14);
}

/// Required test 4. A deposit raises the floor by the deposit and a principal
/// withdrawal lowers it by the withdrawal; the loss on record, principal less
/// floor, does not move with either.
#[test]
fn deposits_and_withdrawals_move_the_floor_with_the_principal() {
    let mut w = staggered_world(16);
    w.refresh(0);
    assert_eq!(protection_and_floor(&w, 1), (14, 14));
    assert_eq!(principal(&w, 1) - protection_and_floor(&w, 1).1, 2);

    deposit(&mut w, 1, 10);
    assert_eq!(principal(&w, 1), 26);
    assert_eq!(protection_and_floor(&w, 1), (24, 24));

    // The losers pay: protection is the whole principal, the floor stays.
    w.refresh(1);
    w.refresh(2);
    assert_eq!(protection_and_floor(&w, 1), (26, 24));

    withdraw(&mut w, 1, 5).unwrap();
    assert_eq!(principal(&w, 1), 21);
    assert_eq!(protection_and_floor(&w, 1), (21, 19));
    assert_eq!(principal(&w, 1) - protection_and_floor(&w, 1).1, 2);

    // A second winner's claim of 1 is inside the refill of 2: no new low.
    w.refresh(3);
    assert_eq!(protection_and_floor(&w, 1), (20, 19));
    w.refresh(4);
    assert_eq!(protection_and_floor(&w, 1), (21, 19));

    // The provider leaves with its whole principal; the claims stay backed by what
    // the losers paid. The floor follows the principal down and stops at zero: 2
    // of what the provider took out were refilled, not protected.
    withdraw(&mut w, 1, 21).unwrap();
    assert_eq!(w.domain(1), (3, 3));
    assert_eq!(principal(&w, 1), 0);
    assert_eq!(protection_and_floor(&w, 1), (0, 0));
    assert_eq!(stored_floor(&w, 1), 0);
    w.assert_settled("provider out");
}

/// Required test 5, first half. The two domains of an asset do not see each
/// other: a loss of the long-loser domain's provider leaves the short-loser
/// domain's floor alone, and the other way round.
#[test]
fn the_two_domains_of_an_asset_are_independent() {
    let mut w = World::new(1, &[DEPOSIT; 2]);
    deposit(&mut w, 0, 7);
    deposit(&mut w, 1, 16);
    w.trade(0, 1, 0, 1);

    // Up 2: the long's gain is claimed from domain 1.
    w.step(0, PX0 + 2, 0);
    w.refresh(0);
    assert_eq!(protection_and_floor(&w, 0), (7, 7));
    assert_eq!(protection_and_floor(&w, 1), (14, 14));
    w.refresh(1);
    assert_eq!(protection_and_floor(&w, 1), (16, 14));

    // Down 5, to 3 below the entry: the short's gain is claimed from domain 0.
    w.step(0, PX0 - 3, 0);
    w.refresh(1);
    let (protection_long, floor_long) = protection_and_floor(&w, 0);
    assert!(
        protection_long < 7 && floor_long == protection_long,
        "the short's gain is fronted by the provider of domain 0: {protection_long} {floor_long}"
    );
    assert_eq!(protection_and_floor(&w, 1).1, 14);
    w.refresh(0);
    assert_eq!(protection_and_floor(&w, 0), (7, floor_long));
    assert_eq!(protection_and_floor(&w, 1), (16, 14));
}

/// Required test 5, second half. A restarted asset is a new generation: the slot
/// is rebuilt, so nothing of the old floor comes along, not even a stored value
/// that was never brought up to date, and a deposit into the new generation is
/// protected in full.
#[test]
fn an_asset_restart_starts_the_floor_again() {
    let mut w = World::new(1, &[DEPOSIT; 2]);
    let expiry = w.slot + 3;
    for (domain, amount) in [(0usize, 7u128), (1, 16)] {
        w.op(|m, _| m.deposit_fresh_counterparty_backing_not_atomic(domain, amount, expiry))
            .unwrap();
        w.provider[domain] = amount;
    }
    w.trade(0, 1, 0, 1);
    w.step(0, PX0 + 2, 0);
    w.refresh(0);
    assert_eq!(protection_and_floor(&w, 1), (14, 14));
    w.trade(1, 0, 0, 1);
    w.refresh(0);
    assert_eq!(w.convert(0), Ok(2));
    assert_eq!(w.domain(1), (0, 16));
    assert_eq!(protection_and_floor(&w, 1), (16, 14));
    assert_eq!(protection_and_floor(&w, 0), (7, 7));

    // Both providers let their backing lapse: forfeited. Nothing has written
    // either domain since, so the stored floors are the old ones.
    w.step(0, PX0 + 2, 0);
    w.step(0, PX0 + 2, 0);
    let now = w.slot;
    for domain in [0, 1] {
        w.op(|m, _| m.expire_source_backing_bucket_not_atomic(domain, now))
            .unwrap();
        w.provider[domain] = 0;
        assert_eq!(protection_and_floor(&w, domain), (0, 0));
    }
    assert_eq!((stored_floor(&w, 0), stored_floor(&w, 1)), (7, 14));

    let old_market_id = w.markets[0].engine.asset.market_id.get();
    let slot = w.slot + 1;
    w.op(|m, _| m.force_asset_recovery_not_atomic(0, slot))
        .unwrap();
    // Domain 1 has the audit history of the conversion to clear. That changes neither its
    // backing nor its claims, so it brings no floor up to date: both stored floors go into
    // the restart as they are.
    w.op(|m, _| m.canonicalize_spent_backing_history_not_atomic(0))
        .unwrap();
    assert_eq!((stored_floor(&w, 0), stored_floor(&w, 1)), (7, 14));

    w.op(|m, _| m.restart_empty_asset_preserving_insurance_budget_not_atomic(0, PX0, slot))
        .unwrap();
    w.slot = slot;
    w.px[0] = PX0;
    let engine = &w.markets[0].engine;
    assert_ne!(engine.asset.market_id.get(), old_market_id);
    assert_eq!(
        engine.asset.try_to_runtime().unwrap().lifecycle,
        AssetLifecycleV16::Active
    );
    assert_eq!(engine.provider_protection_floor_long_num.get(), 0);
    assert_eq!(engine.provider_protection_floor_short_num.get(), 0);

    deposit(&mut w, 1, 5);
    assert_eq!(principal(&w, 1), 5);
    assert_eq!(protection_and_floor(&w, 1), (5, 5));
    assert_eq!(protection_and_floor(&w, 0), (0, 0));
}

/// Backing that lapses is forfeited to the junior pool. That is a loss of the
/// provider's, not a withdrawal: the floor goes with it.
#[test]
fn forfeited_principal_is_a_loss() {
    let mut w = World::new(1, &[DEPOSIT; 2]);
    let expiry = w.slot + 2;
    w.op(|m, _| m.deposit_fresh_counterparty_backing_not_atomic(1, 16, expiry))
        .unwrap();
    assert_eq!(protection_and_floor(&w, 1), (16, 16));
    w.step(0, PX0, 0);
    w.step(0, PX0, 0);
    let now = w.slot;
    w.op(|m, _| m.expire_source_backing_bucket_not_atomic(1, now))
        .unwrap();
    assert_eq!(principal(&w, 1), 0);
    assert_eq!(w.domain(1), (0, 0));
    assert_eq!(protection_and_floor(&w, 1), (0, 0));
    // The stored value is brought up to date at the next write of the domain.
    assert_eq!(stored_floor(&w, 1), 16);
    let expiry = w.slot + 100;
    w.op(|m, _| m.deposit_fresh_counterparty_backing_not_atomic(1, 5, expiry))
        .unwrap();
    assert_eq!(stored_floor(&w, 1), 5);
    assert_eq!(protection_and_floor(&w, 1), (5, 5));
}

/// Nothing about views is stored. Opening a view and reading changes no byte. And
/// two histories that differ only in how the same settlements are split over
/// views leave the same bytes, unless a provider's protection was lower between
/// two of the views: then the floor differs by exactly that low, and nothing else.
///
/// The two histories: a winner and its loser are refreshed one after the other,
/// and then close against each other; or they close against each other at once,
/// which settles both in one view.
#[test]
fn how_calls_are_split_over_views_leaves_no_trace_but_a_recorded_low() {
    let mut w = staggered_world(16);
    let before = (w.header, w.markets.clone());
    {
        let m = MarketGroupV16ViewMut::new(&mut w.header, &mut w.markets);
        m.validate_shape().unwrap();
        assert_eq!(m.provider_protection_floor_num(1), Ok(16 * BOUND_SCALE));
        assert_eq!(m.provider_protection_num(1), Ok(16 * BOUND_SCALE));
        assert!(m.provider_protection_floor_num(2).is_err());
    }
    assert!(before.0 == w.header && before.1 == w.markets);

    // Account 0 is long against account 1 and the mark falls: the short's gain is claimed
    // from domain 0, which the long's loss is paid into. Account 1 buys the position back,
    // and a trade settles its buyer first, so the gain is registered before the loss
    // arrives.
    let run = |principal: u128, refresh_first: bool| {
        let mut w = World::new(1, &[DEPOSIT; 2]);
        if principal != 0 {
            deposit(&mut w, 0, principal);
        }
        w.trade(0, 1, 0, 800);
        w.step(0, PX0 - 20_000, 0);
        if refresh_first {
            w.refresh(1);
            w.refresh(0);
        }
        w.trade(1, 0, 0, 800);
        w.assert_settled("closed");
        w
    };
    // No provider: the same bytes.
    let (split, at_once) = (run(0, true), run(0, false));
    assert!(split.header == at_once.header && split.markets == at_once.markets);
    assert!(split.accts == at_once.accts);

    // A provider: the winner refreshed on its own was fronted 16,000,000 between two
    // views, and that low is the only difference.
    const PRINCIPAL: u128 = 4_000_000_000;
    let (split, mut at_once) = (run(PRINCIPAL, true), run(PRINCIPAL, false));
    assert_eq!(stored_floor(&split, 0), PRINCIPAL - 16_000_000);
    assert_eq!(stored_floor(&at_once, 0), PRINCIPAL);
    at_once.markets[0].engine.provider_protection_floor_long_num =
        split.markets[0].engine.provider_protection_floor_long_num;
    assert!(split.header == at_once.header && split.markets == at_once.markets);
    assert!(split.accts == at_once.accts);
}

/// A view remembers the domains of at most `PROVIDER_PROTECTION_OBSERVED_ASSET_CAP`
/// assets: every asset the rosters of two portfolios can name, which is every asset
/// a transition that settles portfolios can change. Past that it forgets, and a
/// forgotten domain can be observed in the middle of a step: here a withdrawal,
/// whose backing has left and whose principal has not yet. That can only lower a
/// floor, never raise one or miss a low. No transition that settles portfolios
/// reaches it (the terminal slab scan passes more assets and only expires buckets,
/// which leaves nothing to observe mid-step); this view is built for it.
#[test]
fn past_the_view_cap_a_floor_can_only_be_lower() {
    const ASSETS: usize = percolator::PROVIDER_PROTECTION_OBSERVED_ASSET_CAP + 3;
    let mut w = World::with_config(ASSETS, &[DEPOSIT; 2], |cfg| {
        cfg.max_portfolio_assets = percolator::V16_MAX_PORTFOLIO_ASSETS_N as u16;
    });
    for asset in 0..ASSETS {
        deposit(&mut w, 2 * asset + 1, 16);
    }
    // One view takes one atom out of every domain.
    w.op(|m, _| {
        for asset in 0..ASSETS {
            m.withdraw_fresh_counterparty_backing_not_atomic(2 * asset + 1, 1)?;
        }
        Ok(())
    })
    .unwrap();
    for asset in 0..ASSETS {
        let domain = 2 * asset + 1;
        assert_eq!(principal(&w, domain), 15);
        let (protection, floor) = protection_and_floor(&w, domain);
        assert_eq!(protection, 15);
        if asset < percolator::PROVIDER_PROTECTION_OBSERVED_ASSET_CAP {
            assert_eq!(floor, 15, "asset {asset}: a withdrawal is not a loss");
        } else {
            assert_eq!(
                floor, 14,
                "asset {asset}: forgotten, and observed mid-withdrawal"
            );
        }
    }
}

// ---------------------------------------------------------------- random histories

/// The floor a reader must see after each call, from what it saw before the call:
/// the old floor, plus what the provider deposited, less what it withdrew, and no
/// higher than the protection the call left.
fn expect_floors(w: &World, floors: &mut [u128], flow: Option<(usize, u128, bool)>, at: &str) {
    for (domain, floor) in floors.iter_mut().enumerate() {
        let (protection, seen) = protection_and_floor_num(w, domain);
        let mut expected = *floor;
        if let Some((d, amount, add)) = flow {
            if d == domain {
                let amount_num = amount * BOUND_SCALE;
                expected = if add {
                    expected + amount_num
                } else {
                    expected.saturating_sub(amount_num)
                };
            }
        }
        expected = expected.min(protection);
        assert_eq!(
            seen, expected,
            "{at}: domain {domain} floor {seen}, expected {expected} (was {floor}, protection {protection})\n{}",
            w.describe()
        );
        *floor = expected;
    }
}

/// Random markets: trades, price moves, single refreshes in any order,
/// conversions, and provider deposits and withdrawals in between. After every
/// call the floor of every domain is exactly the running minimum of the
/// protection at the boundaries between calls, shifted by the provider's own
/// flows. A floor that took a half-done step, or that missed a low because
/// nobody wrote the domain at the time, fails here.
#[test]
fn the_floor_is_the_running_minimum_of_protection_between_calls() {
    let mut lowered = 0u32;
    let mut refilled_below_principal = 0u32;
    for seed in 0..300u64 {
        let (shape, ops) = random_history(seed, 60, seed % 2 == 0);
        let mut r = Rng::new(seed ^ 0x382);
        let mut w = World::new(shape.assets, &vec![DEPOSIT; shape.accounts]);
        let mut floors = vec![0u128; shape.assets * 2];
        for (d, amount) in &shape.providers {
            deposit(&mut w, *d, *amount);
            expect_floors(
                &w,
                &mut floors,
                Some((*d, *amount, true)),
                "genesis deposit",
            );
        }
        for (at, op) in ops.iter().enumerate() {
            let tag = format!("seed {seed} op {at} {op:?}");
            match *op {
                Op::Trade {
                    buyer,
                    seller,
                    asset,
                    units,
                } => {
                    let _ = w.try_trade(buyer, seller, asset, units);
                    expect_floors(&w, &mut floors, None, &tag);
                }
                Op::Step {
                    asset,
                    delta,
                    funding_e9,
                } => {
                    let px = w.px[asset] as i64 + delta;
                    if (700_000..=1_400_000).contains(&px)
                        && delta.unsigned_abs() * 10_000 <= 480 * w.px[asset]
                    {
                        w.step(asset, px as u64, funding_e9);
                        expect_floors(&w, &mut floors, None, &tag);
                    }
                }
                Op::Refresh(i) => {
                    w.refresh(i);
                    expect_floors(&w, &mut floors, None, &tag);
                }
                Op::Convert(i) => {
                    let _ = w.convert(i);
                    expect_floors(&w, &mut floors, None, &tag);
                }
                Op::Settle { first } => {
                    for k in 0..shape.accounts {
                        w.refresh((first + k) % shape.accounts);
                        expect_floors(&w, &mut floors, None, &tag);
                    }
                }
            }
            // Every few calls a provider acts on a random domain.
            if r.below(4) == 0 {
                let domain = r.below(shape.assets as u64 * 2) as usize;
                let amount = 1_000_000 * (1 + r.below(900) as u128);
                if r.below(2) == 0 {
                    let flow = try_deposit(&mut w, domain, amount)
                        .ok()
                        .map(|()| (domain, amount, true));
                    expect_floors(&w, &mut floors, flow, &tag);
                } else if withdraw(&mut w, domain, amount).is_ok() {
                    expect_floors(&w, &mut floors, Some((domain, amount, false)), &tag);
                } else {
                    expect_floors(&w, &mut floors, None, &tag);
                }
            }
            for domain in 0..floors.len() {
                let (protection, floor) = protection_and_floor_num(&w, domain);
                let principal_num = principal(&w, domain) * BOUND_SCALE;
                assert!(floor <= protection && protection <= principal_num, "{tag}");
                if floor < principal_num {
                    lowered += 1;
                    if protection > floor {
                        refilled_below_principal += 1;
                    }
                }
            }
        }
    }
    // The histories must actually reach the states the floor exists for.
    assert!(lowered > 1_000, "floors below principal: {lowered}");
    assert!(
        refilled_below_principal > 300,
        "refilled above a recorded low: {refilled_below_principal}"
    );
}
