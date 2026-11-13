//! A Live K/F loss that is met by liened claim face.
//!
//! An account that opens a position on credit pledges part of a claim it holds:
//! that face is liened, and the counterparty backing behind it is reserved for
//! the account alone. A later loss lowers the account's positive PnL and burns
//! claim face for it, unliened face first. For unliened face the backing behind
//! it moves to the domain the loss is owed to (#457, #175). For liened face the
//! lien was only un-pledged where it stood: the backing stayed in the face's
//! domain, which no longer had a claim on it, and the gain that the loss pays
//! for was left unbacked.
//!
//! Until the credit rate became lien-aware (#250) this needed the whole claim to
//! be liened: a partial lien cut the domain's stored rate, and with a partially
//! backed claim the whole loss is paid from capital instead. With #250 a lien no
//! longer cuts the rate, so every loss larger than the unliened face reached it.
//!
//! Everything goes through the public `*_not_atomic` API (see `kf_world`).
mod kf_world;

use kf_world::{Rng, World, DEPOSIT, PX0};
use percolator::BOUND_SCALE;

const M: u128 = 1_000_000;

fn half_margin(assets: usize, deposits: &[u128]) -> World {
    World::with_config(assets, deposits, |cfg| {
        cfg.maintenance_margin_bps = 1_000;
        cfg.initial_margin_bps = 5_000;
    })
}

fn liened_face(w: &World, account: usize) -> u128 {
    let liened: u128 = w.accts[account]
        .source_domains
        .iter()
        .map(|s| s.source_claim_liened_num.get())
        .sum();
    assert_eq!(liened % BOUND_SCALE, 0);
    liened / BOUND_SCALE
}

/// X (0) holds 520M at 50% initial margin and is long 1,000 of asset 0 against
/// Y (1). Asset 0 rises 4% and both settle: X holds 40M of face in domain 1,
/// paid in by Y. X then buys `units` of asset 1 from W (2), which liens
/// `units / 2` million of that face, and asset 1 falls 4%.
fn loss_on_a_leg_opened_on_credit(units: u128) -> (World, u128) {
    let mut w = half_margin(2, &[520 * M, DEPOSIT, DEPOSIT]);
    w.trade(0, 1, 0, 1_000);
    w.step(0, 1_040_000, 0);
    w.refresh_all(&[0, 1]);
    assert_eq!(w.domain(1), (40 * M, 40 * M));
    w.trade(0, 2, 1, units);
    assert_eq!(liened_face(&w, 0), units * M / 2);
    w.step(1, 960_000, 0);
    let loss = units * 40_000;
    (w, loss)
}

/// The whole face is liened and the loss is owed to another asset's domain.
/// X's claim pays the loss, so its backing has to follow the loss to domain 2,
/// where W claims it. Before the fix domain 1 kept 3.2M that no claim owned and
/// W's 3.2M gain had no backing at all.
#[test]
fn loss_met_by_liened_face_pays_the_domain_it_is_owed_to() {
    let (mut w, loss) = loss_on_a_leg_opened_on_credit(80);
    assert_eq!(loss, 3_200_000);
    w.refresh_all(&[0, 1, 2]);
    // The account itself is where it always was: capital untouched, face and
    // lien down by the loss.
    assert_eq!(w.accts[0].capital.get(), 520 * M);
    assert_eq!(w.accts[0].pnl.get(), (40 * M - loss) as i128);
    assert_eq!(liened_face(&w, 0), 40 * M - loss);
    assert_eq!(w.domain(2), (loss, loss), "{}", w.describe());
    assert_eq!(w.domain(1), (40 * M - loss, 40 * M - loss), "{}", w.describe());
    w.assert_settled("whole face liened");
}

/// Part of the face is liened. A loss within the unliened face always moved its
/// backing; a loss beyond it burns liened face for the rest. In every case the
/// loss domain ends exactly backed and nothing is left behind in domain 1.
///
/// Before the fix the 78 and 76 unit cases left 2.12M and 1.04M in domain 1 and
/// W that much short. (Before #250 those two paid from capital instead.)
#[test]
fn loss_beyond_the_unliened_face_pays_the_domain_it_is_owed_to() {
    for units in [78u128, 76, 60, 40, 4] {
        let (mut w, loss) = loss_on_a_leg_opened_on_credit(units);
        let unliened = 40 * M - units * M / 2;
        w.refresh_all(&[0, 1, 2]);
        assert_eq!(w.accts[0].capital.get(), 520 * M, "{units} units");
        assert_eq!(
            w.accts[0].pnl.get(),
            (40 * M - loss) as i128,
            "{units} units"
        );
        // Unliened face goes first; the lien shrinks only by what it had to pay.
        assert_eq!(
            liened_face(&w, 0),
            units * M / 2 - loss.saturating_sub(unliened),
            "{units} units"
        );
        assert_eq!(w.domain(2), (loss, loss), "{units} units\n{}", w.describe());
        assert_eq!(
            w.domain(1),
            (40 * M - loss, 40 * M - loss),
            "{units} units\n{}",
            w.describe()
        );
        w.assert_settled("partly liened face");
    }
}

/// The same loss with the refreshes in every order: whoever settles first, the
/// loss domain ends exactly backed.
#[test]
fn loss_met_by_liened_face_is_refresh_order_independent() {
    for order in [[0usize, 1, 2], [0, 2, 1], [2, 0, 1], [2, 1, 0], [1, 2, 0]] {
        let (mut w, loss) = loss_on_a_leg_opened_on_credit(78);
        w.refresh_all(&order);
        assert_eq!(w.domain(2), (loss, loss), "order {order:?}\n{}", w.describe());
        w.assert_settled("refresh order");
    }
}

/// Liened face on the same leg. X is long 1,000 against Y; the mark rises 4%
/// and both settle (Y pays 40M into domain 1); X buys 50 more, which liens 26M
/// of its 40M of face. The mark returns to 1.00: X loses 42M and Y regains 42M,
/// claimed from domain 0.
///
/// All of Y's 40M has to follow X's cancelled face to domain 0, and X pays the
/// last 2M from capital. Before the fix the 26M behind the liened face stayed
/// in domain 1 with no claim on it and Y's regain was 26M short.
#[test]
fn reversal_against_liened_twin_face_returns_the_loser_paid_backing() {
    let mut w = half_margin(1, &[520 * M, DEPOSIT]);
    w.trade(0, 1, 0, 1_000);
    w.step(0, 1_040_000, 0);
    w.refresh_all(&[0, 1]);
    w.trade(0, 1, 0, 50);
    assert_eq!(liened_face(&w, 0), 26 * M);
    w.step(0, PX0, 0);
    w.refresh_all(&[0, 1]);
    assert_eq!(w.accts[0].capital.get(), 518 * M);
    assert_eq!(w.accts[0].pnl.get(), 0);
    assert_eq!(liened_face(&w, 0), 0);
    assert_eq!(w.domain(0), (42 * M, 42 * M), "{}", w.describe());
    assert_eq!(w.domain(1), (0, 0), "{}", w.describe());
    w.assert_settled("liened twin face");
}

/// A provider also backs the twin domain. A provider backs domain 1 with
/// 1,000M; X is long 1,000 against Y, the mark rises 4%, X is refreshed
/// (the provider fronts its 40M of face) and then Y (its 40M refills the
/// provider). X buys 50 more, liening 26M, and the mark returns to 1.00. Only
/// what the twin's loser paid in follows X's cancelled face to domain 0: the
/// provider's 1,000M stays where it is.
#[test]
fn liened_twin_face_never_takes_the_provider_principal_with_it() {
    let mut w = half_margin(1, &[520 * M, DEPOSIT]);
    w.provider_deposit(1, 1_000 * M);
    w.trade(0, 1, 0, 1_000);
    w.step(0, 1_040_000, 0);
    w.refresh_all(&[0, 1]);
    assert_eq!(w.domain(1), (40 * M, 1_040 * M));
    w.trade(0, 1, 0, 50);
    assert_eq!(liened_face(&w, 0), 26 * M);
    w.step(0, PX0, 0);
    w.refresh_all(&[0, 1]);
    assert_eq!(w.accts[0].capital.get(), 518 * M);
    assert_eq!(w.accts[0].pnl.get(), 0);
    assert_eq!(w.domain(1), (0, 1_000 * M), "{}", w.describe());
    assert_eq!(w.domain(0), (42 * M, 42 * M), "{}", w.describe());
    w.assert_settled("liened twin face beside a provider");
}

/// Liened face pays a loss exactly as unliened face does. Two worlds with a
/// provider in domain 1 and the same 3.2M loss owed to domain 2: in one X
/// bought 40 units of asset 1 (20M of its face liened) and the asset fell 8%,
/// so unliened face pays; in the other X bought 80 units (all 40M liened) and
/// the asset fell 4%, so liened face pays. Both domains end in the same state:
/// claims, backing, consumed history and provider receivable.
#[test]
fn liened_face_pays_a_loss_exactly_as_unliened_face_does() {
    let world = |units: u128, falls: u64| {
        let mut w = half_margin(2, &[520 * M, DEPOSIT, DEPOSIT]);
        w.provider_deposit(1, 1_000 * M);
        w.trade(0, 1, 0, 1_000);
        w.step(0, 1_040_000, 0);
        w.refresh_all(&[0, 1]);
        w.trade(0, 2, 1, units);
        for fall in 1..=falls {
            w.step(1, PX0 - 40_000 * fall, 0);
        }
        w.refresh_all(&[0, 1, 2]);
        w.assert_settled("liened and unliened face");
        w
    };
    let unliened = world(40, 2);
    let liened = world(80, 1);
    assert_eq!(liened_face(&unliened, 0), 20 * M);
    assert_eq!(liened_face(&liened, 0), 40 * M - 3_200_000);
    for domain in [1, 2] {
        let (a, b) = (unliened.source(domain), liened.source(domain));
        assert_eq!(
            (
                a.positive_claim_bound_num,
                a.fresh_reserved_backing_num,
                a.spent_backing_num,
                a.provider_receivable_num,
            ),
            (
                b.positive_claim_bound_num,
                b.fresh_reserved_backing_num,
                b.spent_backing_num,
                b.provider_receivable_num,
            ),
            "domain {domain}"
        );
    }
    assert_eq!(liened.domain(2), (3_200_000, 3_200_000));
    assert_eq!(liened.domain(1), (36_800_000, 1_036_800_000));
}

/// Random thin-capital histories at 50% initial margin, refreshed in full after
/// every move: accounts open legs on credit and take losses on them, so liened
/// face is burned all the time. After every full refresh each domain must hold
/// exactly its claims and nothing may sit in the junior residual.
///
/// Before the fix 760 of these 2,000 histories broke that (1,564 before the
/// lien-aware rate, #250, and the loss-domain bookings of #252 and #175).
#[test]
fn random_histories_with_liens_keep_every_domain_exactly_backed() {
    const HISTORIES: u64 = 2_000;
    const STEPS: usize = 60;
    const CAPITAL: u128 = 300 * M;
    let mut burned_liened_face = 0u64;
    let mut failures = Vec::new();
    for seed in 0..HISTORIES {
        let mut rng = Rng::new(0x51ED_FACE ^ (seed.wrapping_mul(0x9E37_79B9_7F4A_7C15)));
        let assets = 2usize;
        let accounts = 4usize;
        let mut w = half_margin(assets, &[CAPITAL; 4]);
        let mut violation = None;
        'history: for step in 0..STEPS {
            if rng.below(3) != 0 {
                let buyer = rng.below(accounts as u64) as usize;
                let seller = (buyer + 1 + rng.below(accounts as u64 - 1) as usize) % accounts;
                let asset = rng.below(assets as u64) as usize;
                let units = 10 * (1 + rng.below(40)) as u128;
                // A trade the margin rules refuse is rolled back and skipped.
                let _ = w.try_trade(buyer, seller, asset, units);
            } else {
                let asset = rng.below(assets as u64) as usize;
                let delta = 10_000 * (1 + rng.below(4));
                let up = rng.below(2) == 0;
                let px = if up {
                    w.px[asset] + delta
                } else {
                    w.px[asset] - delta
                };
                if !(800_000..=1_200_000).contains(&px) {
                    continue;
                }
                w.step(asset, px, 0);
                let start = rng.below(accounts as u64) as usize;
                for k in 0..accounts {
                    let i = (start + k) % accounts;
                    let (liened, pnl) = (liened_face(&w, i), w.accts[i].pnl.get());
                    w.refresh(i);
                    if liened_face(&w, i) < liened && w.accts[i].pnl.get() < pnl {
                        burned_liened_face += 1;
                    }
                }
                let v = w.settled_violations();
                if !v.is_empty() {
                    violation = Some(format!("seed {seed} step {step}: {}", v.join("; ")));
                    break 'history;
                }
            }
        }
        if let Some(v) = violation {
            failures.push(v);
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {HISTORIES} histories left a domain mis-backed, first: {}",
        failures.len(),
        failures[0]
    );
    // The sweep is only worth its name if it reaches the path.
    assert!(
        burned_liened_face > 1_000,
        "only {burned_liened_face} refreshes burned liened face"
    );
}
