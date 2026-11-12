//! A K/F loss met by the account's own face in the domain the loss is owed to (issue #252).
//!
//! An account that was short while the mark fell holds positive face in the asset's long-loser
//! domain L, backed by what the longs paid in. If it then turns long and the mark falls again,
//! its new loss is owed to that same domain L. The claim on L and the debt to L are the same
//! money: they must cancel, and the backing behind the cancelled face must stay in L, where it
//! now backs the gain of whoever is short against the account.
//!
//! Before the fix the pre-support netting skipped face held in the loss domain itself, so the
//! loss took the ordinary support-and-burn path: it consumed L's backing into the junior
//! residual and credited nothing back. With a backing provider in L the provider silently
//! funded the gap and lost it at wind-down; without one, L showed a Live credit rate below 1
//! although every loser had paid in full.
//!
//! Base scenario (one asset at 1.00, no fees, 100% margin, 1e12 of capital each):
//!   1. B buys 1,000 from A: A short 1,000, B long 1,000.
//!   2. The mark falls to `peak`. A gains `1,000 * (1.00 - peak)`, claimed from domain 0
//!      (asset 0, long losers), where B pays the same amount in.
//!   3. A buys 2,000 from C at `peak`: A long 1,000, C short 2,000.
//!   4. The mark falls to `end`. A and B each owe `1,000 * (peak - end)` to domain 0; C is owed
//!      `2,000 * (peak - end)` from it.
mod kf_world;

use kf_world::{fully_cranked, play, random_history, shrink, World, DEPOSIT, PX0};

const A: usize = 0;
const B: usize = 1;
const C: usize = 2;
/// Asset 0, long losers: where A's face sits and where A's later loss is owed.
const L: usize = 0;
const M: u128 = 1_000_000;

const ORDERS: [[usize; 3]; 6] = [
    [A, B, C],
    [A, C, B],
    [B, A, C],
    [B, C, A],
    [C, A, B],
    [C, B, A],
];

/// Steps 1-3 of the base scenario. Everyone is refreshed at the peak before A turns long: the
/// engine admits only risk-reducing trades on an asset while any account on it has an
/// unsettled K/F move, so A cannot turn long before B has paid, and A's face is always backed
/// by what the longs actually paid in.
fn turned_long_at(provider: u128, peak: u64) -> World {
    let mut w = World::new(1, &[DEPOSIT; 3]);
    if provider != 0 {
        w.provider_deposit(L, provider);
    }
    w.trade(B, A, 0, 1_000);
    w.step(0, peak, 0);
    w.refresh_all(&[A, B, C]);
    w.trade(A, C, 0, 2_000);
    assert_eq!(w.claim(A, L), 1_000 * (PX0 - peak) as u128);
    w.assert_settled("after A turned long");
    w
}

/// The base scenario, with everyone refreshed at the end in `end_order`.
fn base(provider: u128, peak: u64, end: u64, end_order: &[usize]) -> World {
    let mut w = turned_long_at(provider, peak);
    w.step(0, end, 0);
    w.refresh_all(end_order);
    w
}

/// Whether A's face in domain 0 was worth par when A's loss arrived. A provider keeps the
/// domain at rate 1 throughout. Without one, the domain holds only what B paid in, so it is
/// at rate 1 unless C's new gain was registered against it before A settled; A's face is
/// then worth less than par for a moment and takes the kept-face rule instead (the loss is
/// paid from capital and the face stays, fully backed again once everyone has settled).
fn face_at_par(provider: u128, order: &[usize]) -> bool {
    let at = |who| order.iter().position(|a| *a == who).unwrap();
    provider != 0 || at(A) < at(C)
}

/// Checks the base scenario's end state from its inputs: A held `face` in domain 0 and lost
/// `loss` to it, B lost `b_total` in all and C is owed `c_gain`.
fn assert_base_outcome(
    w: &mut World,
    provider: u128,
    order: &[usize],
    face: u128,
    loss: u128,
    b_total: u128,
    c_gain: u128,
) {
    let tag = format!(
        "provider {provider}, refresh order {order:?}\n{}",
        w.describe()
    );
    // At par the face cancels against the loss and only the rest of the loss is paid from
    // capital; below par the whole loss is paid from capital and the face is kept.
    let cancelled = if face_at_par(provider, order) {
        face.min(loss)
    } else {
        0
    };
    assert_eq!(
        w.accts[A].capital.get(),
        DEPOSIT - (loss - cancelled),
        "{tag}"
    );
    assert_eq!(w.accts[A].pnl.get(), (face - cancelled) as i128, "{tag}");
    assert_eq!(w.claim(A, L), face - cancelled, "{tag}");
    assert_eq!(w.accts[B].capital.get(), DEPOSIT - b_total, "{tag}");
    assert_eq!(w.accts[B].pnl.get(), 0, "{tag}");
    assert_eq!(w.accts[C].capital.get(), DEPOSIT, "{tag}");
    assert_eq!(w.claim(C, L), c_gain, "{tag}");
    // Domain 0 holds A's remaining claim and C's, backed atom for atom by what the losers
    // paid in; the provider's principal sits untouched on top.
    let claims = face - cancelled + c_gain;
    assert_eq!(w.domain(L), (claims, provider + claims), "{tag}");
    assert_eq!(w.junior_residual(), 0, "{tag}");
    w.assert_settled(&tag);

    // Wind-down: every trader is paid its equity, the provider takes its whole principal
    // back and nothing is retired.
    let fair = w.fair_wind_down();
    assert_eq!(fair.provider_out[L], provider);
    assert_eq!(
        fair.payouts,
        vec![DEPOSIT + face - loss, DEPOSIT - b_total, DEPOSIT + c_gain]
    );
    assert_eq!(w.wind_down(order), fair, "{tag}");
}

/// The reported case: A's loss equals its face in the loss domain. Both cancel, A's capital
/// is untouched, and domain 0 ends holding exactly what B paid in on top of the provider's
/// principal -- the backing for C's gain. After wind-down the provider has its whole
/// principal back and nothing is retired.
#[test]
fn kf_loss_cancels_the_accounts_own_face_in_the_loss_domain() {
    for provider in [1_000 * M, 0] {
        for order in ORDERS {
            // A: +40M as a short to 0.96, -40M as a long to 0.92. B: -80M. C: +80M.
            let mut w = base(provider, 960_000, 920_000, &order);
            assert_base_outcome(&mut w, provider, &order, 40 * M, 40 * M, 80 * M, 80 * M);
        }
    }
    // Spelled out for the reported order (everyone refreshed A, B, C) with a provider.
    let w = base(1_000 * M, 960_000, 920_000, &[A, B, C]);
    assert_eq!(
        (w.accts[A].capital.get(), w.accts[A].pnl.get()),
        (DEPOSIT, 0)
    );
    assert_eq!(w.domain(L), (80 * M, 1_080 * M));
}

/// The reported case through the permissionless auto-crank, the way the program brings
/// accounts current: the same end state as with direct refreshes.
#[test]
fn kf_loss_cancels_the_accounts_own_face_through_the_permissionless_crank() {
    for provider in [1_000 * M, 0] {
        for order in ORDERS {
            let mut w = World::new(1, &[DEPOSIT; 3]);
            if provider != 0 {
                w.provider_deposit(L, provider);
            }
            w.trade(B, A, 0, 1_000);
            w.step(0, 960_000, 0);
            for i in [A, B, C] {
                w.crank(i);
            }
            w.trade(A, C, 0, 2_000);
            w.step(0, 920_000, 0);
            for i in order {
                w.crank(i);
            }
            assert_base_outcome(&mut w, provider, &order, 40 * M, 40 * M, 80 * M, 80 * M);
        }
    }
}

/// A loss larger than the face: the face cancels in full and only the rest is paid from A's
/// capital into the loss domain.
#[test]
fn kf_loss_larger_than_the_own_face_pays_only_the_rest_from_capital() {
    for provider in [1_000 * M, 0] {
        for order in ORDERS {
            // A: +20M of face at 0.98, then -40M as a long to 0.94. B: -60M. C: +80M.
            let mut w = base(provider, 980_000, 940_000, &order);
            assert_base_outcome(&mut w, provider, &order, 20 * M, 40 * M, 60 * M, 80 * M);
        }
    }
    let w = base(1_000 * M, 980_000, 940_000, &[A, B, C]);
    assert_eq!(
        (w.accts[A].capital.get(), w.accts[A].pnl.get()),
        (DEPOSIT - 20 * M, 0)
    );
    assert_eq!(w.domain(L), (80 * M, 1_080 * M));
}

/// A loss smaller than the face: exactly the loss is cancelled and A keeps the rest of its
/// claim, still fully backed.
#[test]
fn kf_loss_smaller_than_the_own_face_keeps_the_rest_of_the_claim() {
    for provider in [1_000 * M, 0] {
        for order in ORDERS {
            // A: +40M of face at 0.96, then -10M as a long to 0.95. B: -50M. C: +20M.
            let mut w = base(provider, 960_000, 950_000, &order);
            assert_base_outcome(&mut w, provider, &order, 40 * M, 10 * M, 50 * M, 20 * M);
        }
    }
    let w = base(1_000 * M, 960_000, 950_000, &[A, B, C]);
    assert_eq!(
        (w.accts[A].capital.get(), w.accts[A].pnl.get()),
        (DEPOSIT, 30 * M as i128)
    );
    assert_eq!(w.domain(L), (50 * M, 1_050 * M));
}

/// The same with a funding loss instead of a mark loss: after A turns long at 0.96 the mark
/// stays put and longs pay funding for five slots (6 atoms per unit per slot): 30,000 each
/// for A and B, owed to domain 0, and 60,000 for C.
#[test]
fn funding_loss_cancels_the_accounts_own_face_in_the_loss_domain() {
    for provider in [1_000 * M, 0] {
        for order in ORDERS {
            let mut w = turned_long_at(provider, 960_000);
            for _ in 0..5 {
                w.step(0, 960_000, 6_250);
            }
            w.refresh_all(&order);
            assert_base_outcome(
                &mut w,
                provider,
                &order,
                40 * M,
                30_000,
                40 * M + 30_000,
                60_000,
            );
        }
    }
}

/// Control: below rate 1 nothing is cancelled at par. With no provider and C's gain
/// registered first, domain 0 holds 80M for 120M of claims when A's loss arrives, so A pays
/// the whole 40M from capital into the domain and keeps its 40M of face, which is fully
/// backed again as soon as that payment lands (unchanged behaviour).
#[test]
fn control_own_face_below_rate_one_is_kept_and_the_loss_paid_from_capital() {
    let mut w = turned_long_at(0, 960_000);
    w.step(0, 920_000, 0);
    w.refresh_all(&[B, C]);
    assert_eq!(w.domain(L), (120 * M, 80 * M));
    w.refresh(A);
    assert_eq!(w.accts[A].capital.get(), DEPOSIT - 40 * M);
    assert_eq!(w.accts[A].pnl.get(), 40 * M as i128);
    assert_eq!(w.claim(A, L), 40 * M);
    assert_eq!(w.domain(L), (120 * M, 120 * M));
    w.assert_settled("kept face");
    assert_eq!(w.wind_down(&[A, B, C]), w.fair_wind_down());
}

/// The base scenario with A converting its face to capital (at par) right after turning
/// long, so its loss at the end is paid from capital.
fn converted_first(provider: u128) -> World {
    let mut w = turned_long_at(provider, 960_000);
    assert_eq!(w.convert(A), Ok(40 * M));
    assert_eq!(w.accts[A].capital.get(), DEPOSIT + 40 * M);
    assert_eq!(w.domain(L), (0, provider));
    w.step(0, 920_000, 0);
    w.refresh_all(&[A, B, C]);
    w
}

/// Control: converting the face and then paying the loss from capital always settled exactly.
#[test]
fn control_converting_the_face_before_the_loss_settles_exactly() {
    for provider in [1_000 * M, 0] {
        let mut w = converted_first(provider);
        assert_eq!(w.accts[A].capital.get(), DEPOSIT);
        assert_eq!(w.domain(L), (80 * M, provider + 80 * M));
        w.assert_settled("converted first");
        assert_eq!(w.wind_down(&[A, B, C]), w.fair_wind_down());
    }
}

/// Cancelling the face against the loss ends exactly where converting it first does: the same
/// balances for every account and the same claims and backing in the domain.
#[test]
fn cancelling_the_own_face_ends_where_converting_it_first_does() {
    for provider in [1_000 * M, 0] {
        let converted = converted_first(provider);
        let netted = base(provider, 960_000, 920_000, &[A, B, C]);
        assert_eq!(netted.domain(L), converted.domain(L), "provider {provider}");
        for i in [A, B, C] {
            assert_eq!(
                (netted.accts[i].capital.get(), netted.accts[i].pnl.get()),
                (
                    converted.accts[i].capital.get(),
                    converted.accts[i].pnl.get()
                ),
                "provider {provider}, account {i}"
            );
        }
        assert_eq!(netted.junior_residual(), converted.junior_residual());
    }
}

/// Control: the same loss met by fully backed face in a *different* domain. A is short asset 0
/// (its gain sits in domain 0) and long asset 1, whose loss is owed to domain 2. The face is
/// realized one for one and its backing moves to domain 2 (unchanged behaviour).
#[test]
fn control_face_in_another_domain_moves_its_backing_to_the_loss_domain() {
    for provider in [&[][..], &[(0usize, 1_000 * M)], &[(2, 1_000 * M)]] {
        for order in ORDERS {
            let mut w = World::new(2, &[DEPOSIT; 3]);
            for (d, amount) in provider {
                w.provider_deposit(*d, *amount);
            }
            w.trade(B, A, 0, 1_000);
            w.trade(A, C, 1, 1_000);
            w.step(0, 960_000, 0);
            w.refresh_all(&[A, B, C]);
            assert_eq!(w.claim(A, 0), 40 * M);
            w.step(1, 960_000, 0);
            w.refresh_all(&order);
            let tag = format!("provider {provider:?}, refresh order {order:?}");
            assert_eq!(w.accts[A].capital.get(), DEPOSIT, "{tag}");
            assert_eq!(w.accts[A].pnl.get(), 0, "{tag}");
            assert_eq!(w.claim(C, 2), 40 * M, "{tag}");
            assert_eq!(w.domain(0), (0, w.provider[0]), "{tag}");
            assert_eq!(w.domain(2), (40 * M, w.provider[2] + 40 * M), "{tag}");
            w.assert_settled(&tag);
            assert_eq!(w.wind_down(&order), w.fair_wind_down(), "{tag}");
        }
    }
}

/// Control: a lapsed bucket's stored rate is not trusted. With a three-slot backing lifetime
/// and no provider, the backing behind A's face lapses before A's loss is settled. Nothing is
/// cancelled at the stale rate of 1: the refresh fails closed until the bucket is expired (its
/// principal forfeits to the junior residual), after which the face is unbacked, so it is kept
/// and the loss is paid from capital (unchanged behaviour).
#[test]
fn control_lapsed_loss_domain_backing_is_not_netted_at_its_stale_rate() {
    let mut w = World::with_config(1, &[DEPOSIT; 3], |cfg| cfg.h_max = 3);
    w.trade(B, A, 0, 1_000);
    w.step(0, 960_000, 0);
    w.refresh_all(&[A, B, C]);
    let expiry = w.bucket(L).expiry_slot;
    assert_eq!(expiry, w.slot + 3);
    w.trade(A, C, 0, 2_000);
    for px in [950_000, 935_000, 920_000] {
        w.step(0, px, 0);
    }
    assert_eq!(w.slot, expiry);
    assert_eq!(w.source(L).credit_rate_num, percolator::CREDIT_RATE_SCALE);
    assert_eq!(w.try_refresh(A), Err(percolator::V16Error::Stale));

    let now = w.slot;
    w.op(|m, _| m.expire_source_backing_bucket_not_atomic(L, now))
        .unwrap();
    assert_eq!(w.domain(L), (40 * M, 0));
    assert_eq!(w.junior_residual(), 40 * M as i128);
    w.refresh(A);
    // A long 1,000 from 0.96 to 0.92: 40M, all from capital; the 40M of face is kept.
    assert_eq!(w.accts[A].capital.get(), DEPOSIT - 40 * M);
    assert_eq!(w.claim(A, L), 40 * M);
    assert_eq!(w.domain(L), (40 * M, 40 * M));
}

/// Control: liened face in the loss domain. At 50% initial margin and 500M of capital, A turns
/// long 1,125 at 0.96: the 540M of initial margin liens its whole 40M of face. The 45M loss at
/// 0.92 unwinds the lien and burns the face at par, leaving the un-pledged backing in domain 0,
/// and the last 5M comes from capital (unchanged behaviour).
#[test]
fn control_liened_own_face_is_unwound_in_place() {
    for order in ORDERS {
        let mut w = World::with_config(1, &[500 * M, DEPOSIT, DEPOSIT], |cfg| {
            cfg.maintenance_margin_bps = 1_000;
            cfg.initial_margin_bps = 5_000;
        });
        w.trade(B, A, 0, 1_000);
        w.step(0, 960_000, 0);
        w.refresh_all(&[A, B, C]);
        w.trade(A, C, 0, 2_125);
        let liened: u128 = w.accts[A]
            .source_domains
            .iter()
            .map(|s| s.source_claim_liened_num.get())
            .sum();
        assert_eq!(
            liened,
            40 * M * percolator::BOUND_SCALE,
            "the whole face is liened"
        );
        w.step(0, 920_000, 0);
        w.refresh_all(&order);
        let tag = format!("refresh order {order:?}");
        assert_eq!(w.accts[A].capital.get(), 495 * M, "{tag}");
        assert_eq!(w.accts[A].pnl.get(), 0, "{tag}");
        // C is owed 2,125 * 0.04 = 85M: B's 80M and A's 5M.
        assert_eq!(w.domain(L), (85 * M, 85 * M), "{tag}\n{}", w.describe());
        w.assert_settled(&tag);
    }
}

/// In a fully cranked market -- every account refreshed after every price step -- every loser
/// has always paid in full, so after each step every domain must hold exactly its provider's
/// principal on top of the claims against it, with nothing in the junior residual. Random
/// histories over 1-3 assets with trades, position flips, price and funding moves and
/// conversions.
#[test]
fn fully_cranked_markets_keep_every_domain_exactly_backed() {
    for seed in 1..=300u64 {
        let (shape, ops) = random_history(seed, 60, seed % 2 == 0);
        let ops = fully_cranked(ops);
        if let Err(violation) = play(&shape, &ops) {
            let small = shrink(&ops, |ops| play(&shape, ops).is_err());
            panic!(
                "seed {seed}: {violation}\nworld {shape:?}\nsmallest failing history: {small:#?}\n{}",
                play(&shape, &small).err().unwrap()
            );
        }
    }
}
