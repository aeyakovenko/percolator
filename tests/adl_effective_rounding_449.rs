//! Issue percolator-prog#449: per-leg ADL-effective quantities are ceilings,
//! so after a quantity ADL the sum of a side's leg ceilings can exceed the
//! side's aggregate effective OI. These tests pin what the engine does with
//! that surplus on every exit route.

use percolator::{
    active_bitmap_is_empty, AssetStateV16, EngineAssetSlotV16Account, Market,
    MarketGroupV16HeaderAccount, MarketGroupV16ViewMut, PortfolioAccountV16Account,
    PortfolioV16ViewMut, ProvenanceHeaderV16, ProvenanceHeaderV16Account, RebalanceRequestV16,
    SideV16, TradeRequestV16, V16Config, V16PodU64, ADL_ONE, POS_SCALE,
};

const PRICE: u64 = 1_000_000;

fn market() -> (MarketGroupV16HeaderAccount, Vec<Market<u64>>) {
    let mut cfg = V16Config::public_user_fund_with_market_slots(1, 1, 0, 10);
    cfg.max_price_move_bps_per_slot = 9_000;
    let mut header = MarketGroupV16HeaderAccount::new_dynamic([1; 32], cfg, 1, 0).unwrap();
    let mut markets = vec![Market::new(0, EngineAssetSlotV16Account::default())];
    header
        .activate_empty_asset_slot_not_atomic(0, &mut markets[0].engine, PRICE, 1)
        .unwrap();
    (header, markets)
}

fn account(seed: u8) -> PortfolioAccountV16Account {
    let header = ProvenanceHeaderV16Account::from_runtime(&ProvenanceHeaderV16::new(
        [1; 32], [seed; 32], [3; 32],
    ));
    let mut account = PortfolioAccountV16Account::default();
    account.init_empty_in_place(header).unwrap();
    account
}

fn asset(markets: &[Market<u64>]) -> AssetStateV16 {
    markets[0].engine.asset.try_to_runtime().unwrap()
}

fn raw_and_basis(acct: &PortfolioAccountV16Account) -> Option<(SideV16, u128, u128, u64)> {
    let leg = acct.legs[0].try_to_runtime().unwrap();
    leg.active
        .then(|| (leg.side, leg.basis_pos_q.unsigned_abs(), leg.a_basis, leg.epoch_snap))
}

fn ceil_eff(raw: u128, a_basis: u128, a: u128) -> u128 {
    (raw * a + a_basis - 1) / a_basis
}

/// Per side: (sum of current-epoch leg ceilings, exact sum numerator over ADL_ONE)
fn census(markets: &[Market<u64>], accts: &[PortfolioAccountV16Account]) -> [(u128, u128); 2] {
    let a = asset(markets);
    let mut out = [(0u128, 0u128); 2];
    for acct in accts {
        if let Some((side, raw, a_basis, epoch)) = raw_and_basis(acct) {
            let (cur_a, cur_epoch, i) = match side {
                SideV16::Long => (a.a_long, a.epoch_long, 0),
                SideV16::Short => (a.a_short, a.epoch_short, 1),
            };
            if epoch != cur_epoch {
                continue;
            }
            assert_eq!(a_basis, ADL_ONE, "test universe opens every leg at unit A");
            out[i].0 += ceil_eff(raw, a_basis, cur_a);
            out[i].1 += raw * cur_a; // exact effective * ADL_ONE
        }
    }
    out
}

fn trade(
    header: &mut MarketGroupV16HeaderAccount,
    markets: &mut [Market<u64>],
    buyer: &mut PortfolioAccountV16Account,
    seller: &mut PortfolioAccountV16Account,
    q: u128,
) -> Result<(), percolator::V16Error> {
    let mut market = MarketGroupV16ViewMut::new(header, markets);
    let mut b = PortfolioV16ViewMut::new(buyer);
    let mut s = PortfolioV16ViewMut::new(seller);
    market
        .execute_trade_with_fee_loss_stale_scoped_not_atomic(
            &mut b,
            &mut s,
            TradeRequestV16 {
                asset_index: 0,
                size_q: i128::try_from(q).unwrap(),
                exec_price: PRICE,
                fee_bps: 0,
            },
        )
        .map(|_| ())
}

fn rebalance(
    header: &mut MarketGroupV16HeaderAccount,
    markets: &mut [Market<u64>],
    acct: &mut PortfolioAccountV16Account,
    q: u128,
) -> Result<u128, percolator::V16Error> {
    let mut market = MarketGroupV16ViewMut::new(header, markets);
    let mut a = PortfolioV16ViewMut::new(acct);
    market
        .rebalance_reduce_position_not_atomic(
            &mut a,
            RebalanceRequestV16 {
                asset_index: 0,
                reduce_q: q,
            },
        )
        .map(|o| o.reduced_q)
}

fn deposit(
    header: &mut MarketGroupV16HeaderAccount,
    markets: &mut [Market<u64>],
    acct: &mut PortfolioAccountV16Account,
) {
    let mut market = MarketGroupV16ViewMut::new(header, markets);
    let mut a = PortfolioV16ViewMut::new(acct);
    market.deposit_not_atomic(&mut a, 1_000_000_000_000).unwrap();
}

/// Longs L1=500_000, L2=250_000 against one short S=750_000; S unilaterally
/// reduces 500_000 => a_long = floor(1e15/3), OI 250_000/250_000, leg
/// ceilings 166_667 + 83_334 = 250_001 (the #449 state).
fn issue_state() -> (
    MarketGroupV16HeaderAccount,
    Vec<Market<u64>>,
    Vec<PortfolioAccountV16Account>,
) {
    let (mut header, mut markets) = market();
    let mut accts = vec![account(10), account(11), account(12)];
    for a in accts.iter_mut() {
        deposit(&mut header, &mut markets, a);
    }
    let (l, s) = accts.split_at_mut(2);
    trade(&mut header, &mut markets, &mut l[0], &mut s[0], 500_000).unwrap();
    trade(&mut header, &mut markets, &mut l[1], &mut s[0], 250_000).unwrap();
    assert_eq!(
        rebalance(&mut header, &mut markets, &mut s[0], 500_000).unwrap(),
        500_000
    );
    (header, markets, accts)
}

#[test]
fn issue_449_reproduces_ceiling_surplus_without_engine_oi_mismatch() {
    let (_header, markets, accts) = issue_state();
    let a = asset(&markets);
    assert_eq!(a.a_long, ADL_ONE / 3);
    assert_eq!(a.oi_eff_long_q, 250_000);
    assert_eq!(a.oi_eff_short_q, 250_000);
    let c = census(&markets, &accts);
    assert_eq!(c[0].0, 166_667 + 83_334);
    assert_eq!(c[1].0, 250_000);
    // Exact (un-rounded) long exposure stays at or below aggregate OI here.
    assert!(c[0].1 <= a.oi_eff_long_q * ADL_ONE);
}

#[derive(Clone, Copy, Debug)]
enum Exit {
    Rebalance,
    TradeWithShort,
}

/// Close each long in the given order; return per-step errors and final state.
fn exit_all(order: [usize; 2], how: Exit) {
    let (mut header, mut markets, mut accts) = issue_state();
    for &i in order.iter() {
        let eff = {
            let a = asset(&markets);
            let (_, raw, ab, _) = raw_and_basis(&accts[i]).unwrap();
            ceil_eff(raw, ab, a.a_long)
        };
        let (l, s) = accts.split_at_mut(2);
        match how {
            Exit::Rebalance => {
                let got = rebalance(&mut header, &mut markets, &mut l[i], u128::MAX >> 8)
                    .unwrap_or_else(|e| panic!("{order:?} {how:?} leg {i}: {e:?}"));
                let a = asset(&markets);
                assert!(got <= eff);
                assert_eq!(a.oi_eff_long_q, a.oi_eff_short_q);
            }
            Exit::TradeWithShort => {
                // Short S buys back from long i: capped by S's effective size,
                // since S may not flip while A != ADL_ONE.
                let a = asset(&markets);
                let s_eff = raw_and_basis(&s[0])
                    .map(|(_, raw, ab, _)| ceil_eff(raw, ab, a.a_short))
                    .unwrap_or(0);
                let q = eff.min(s_eff).min(a.oi_eff_long_q);
                if q > 0 {
                    trade(&mut header, &mut markets, &mut s[0], &mut l[i], q)
                        .unwrap_or_else(|e| panic!("{order:?} {how:?} leg {i}: {e:?}"));
                }
                // One unit beyond aggregate OI is rejected, never saturated.
                let a = asset(&markets);
                if let Some((_, raw, ab, ep)) = raw_and_basis(&l[i]) {
                    if ep == a.epoch_long && ceil_eff(raw, ab, a.a_long) > a.oi_eff_long_q {
                        assert!(trade(&mut header, &mut markets, &mut s[0], &mut l[i], 1).is_err());
                    }
                }
            }
        }
    }
    // OI must be exhausted on both sides and any residue reset, not stranded.
    let a = asset(&markets);
    assert_eq!((a.oi_eff_long_q, a.oi_eff_short_q), (0, 0), "{order:?} {how:?}");
    for acct in accts.iter_mut() {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        let mut v = PortfolioV16ViewMut::new(acct);
        market.full_account_refresh_not_atomic(&mut v).unwrap();
        assert!(
            active_bitmap_is_empty(v.header.active_bitmap.map(V16PodU64::get)),
            "{order:?} {how:?}: residue leg not cleared by refresh"
        );
        // flat price, zero fees: nobody gains or loses value
        assert_eq!(v.header.pnl.get(), 0, "{order:?} {how:?}");
        assert_eq!(v.header.capital.get(), 1_000_000_000_000, "{order:?} {how:?}");
    }
    let market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    market.validate_shape().unwrap();
}

#[test]
fn issue_449_every_exit_order_and_route_drains_exactly() {
    for order in [[0, 1], [1, 0]] {
        exit_all(order, Exit::Rebalance);
        exit_all(order, Exit::TradeWithShort);
    }
}

/// Repeated unilateral ADL rounds interleaved with 1-unit bilateral long
/// reductions. The ceiling surplus D = sum(ceil) - OI and the exact surplus
/// G = sum(exact) - OI are NOT bounded by the leg count: each same-side
/// resize can move up to one raw-grid step (A/a_basis < 1 effective unit) of
/// exact exposure into G, because the resize inverse keeps the largest raw
/// basis whose ceiling equals the target. They are bounded by the number of
/// same-side reducing operations in the epoch.
#[test]
fn issue_449_characterize_resize_rounding_surplus_compounds_hardening_gap() {
    const N: usize = 4;
    let (mut header, mut markets) = market();
    // accts[0..N] longs, accts[N] = ADL'ing short, accts[N+1] = short reducer
    let mut accts: Vec<_> = (0..N + 2).map(|i| account(20 + i as u8)).collect();
    for a in accts.iter_mut() {
        deposit(&mut header, &mut markets, a);
    }
    {
        let (l, s) = accts.split_at_mut(N);
        for (i, leg) in l.iter_mut().enumerate() {
            let q = 1_000 * POS_SCALE + 7_777 * (i as u128 + 1);
            trade(&mut header, &mut markets, leg, &mut s[0], q / 2).unwrap();
            trade(&mut header, &mut markets, leg, &mut s[1], q - q / 2).unwrap();
        }
    }
    // One large ADL first so A/a_basis is well below 1 (coarse raw grid).
    {
        let (_, s) = accts.split_at_mut(N);
        let s0_eff = raw_and_basis(&s[0]).unwrap().1;
        rebalance(&mut header, &mut markets, &mut s[0], s0_eff * 2 / 3).unwrap();
    }
    let mut max_d = 0u128;
    let mut max_g_num = 0i128;
    let rounds = 2_000u128;
    let mut long_reducing_ops = 0u128;
    for _ in 0..rounds {
        let (l, s) = accts.split_at_mut(N);
        rebalance(&mut header, &mut markets, &mut s[0], 1).unwrap();
        for leg in l.iter_mut() {
            trade(&mut header, &mut markets, &mut s[1], leg, 1).unwrap();
            long_reducing_ops += 1;
        }
        let a = asset(&markets);
        assert_eq!(a.oi_eff_long_q, a.oi_eff_short_q);
        let c = census(&markets, &accts);
        let d = c[0].0 - a.oi_eff_long_q;
        let g = c[0].1 as i128 - (a.oi_eff_long_q * ADL_ONE) as i128;
        max_d = max_d.max(d);
        max_g_num = max_g_num.max(g);
        // short side is never haircut here, so its legs are exact
        assert_eq!(c[1].0, a.oi_eff_short_q);
        // Op-count bound: |G| <= same-side reducing ops (+1 for the initial
        // large ADL's scaling floor), D < G + N.
        assert!(g.unsigned_abs() <= (long_reducing_ops + 1) * ADL_ONE);
        assert!(d < long_reducing_ops + 1 + N as u128);
    }
    let a = asset(&markets);
    let c = census(&markets, &accts);
    let g_num = c[0].1 as i128 - (a.oi_eff_long_q * ADL_ONE) as i128;
    eprintln!(
        "rounds={rounds} n={N} a_long={} oi={} final D={} max D={} max G={:.6} final G={:.6}",
        a.a_long,
        a.oi_eff_long_q,
        c[0].0 - a.oi_eff_long_q,
        max_d,
        max_g_num as f64 / ADL_ONE as f64,
        g_num as f64 / ADL_ONE as f64,
    );
    // Documented finding: the surplus compounds past any per-leg tolerance.
    assert!(max_d > rounds / 2, "expected linear growth, max D={max_d}");

    // Value consequence: the matched book is no longer zero-sum. Move the
    // price up 10%; aggregate PnL across the closed universe becomes positive
    // by ~G * dP / POS_SCALE (unbacked, so subject to haircut).
    let new_price = PRICE + PRICE / 10;
    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        market
            .accrue_asset_to_not_atomic(0, 2, new_price, 0, true)
            .unwrap();
        market.markets[0].engine.asset.raw_oracle_target_price = V16PodU64::new(new_price);
    }
    let mut sum_pnl = 0i128;
    for acct in accts.iter_mut() {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        let mut v = PortfolioV16ViewMut::new(acct);
        market.full_account_refresh_not_atomic(&mut v).unwrap();
        sum_pnl += v.header.pnl.get() + v.header.capital.get() as i128 - 1_000_000_000_000;
    }
    let expected = g_num * i128::from(new_price - PRICE) / (ADL_ONE * POS_SCALE) as i128;
    eprintln!("sum (capital+pnl-deposit) after +10% = {sum_pnl} (G*dP/POS_SCALE = {expected})");
    assert!(sum_pnl > 0, "zero-sum broken by the harvested surplus");
}
