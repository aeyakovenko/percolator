//! Same-side resize must not shrink a winner's loss weight after a bankruptcy booking.
//!
//! `book_bankruptcy_residual_chunk_internal` raises `B_side_num` without bumping an
//! epoch or setting `b_stale`, so the winner's certificate stays current and the
//! trade path skips the refresh. A same-side Resize is the one route that rewrites
//! `loss_weight` without going through `clear_leg_at_slot_inner`'s
//! `b_target == b_snap` gate, so a winner can sell most of its position after the
//! booking and have the deferred B delta charged at the shrunk weight. The other
//! winners absorb the remainder and the domain credit rate falls below 1.
//!
//! Expected: each long pays 50,000 of the 100,000 residual and nets +150,000.
//! Before the fix the attacker nets +171,3xx and the honest long +128,6xx.
#![allow(dead_code)]

use percolator::{
    AutoCrankObservationV16, AutoCrankPlanV16, AutoCrankWorkV16, EngineAssetSlotV16Account,
    LiquidationRequestV16, Market, MarketGroupV16HeaderAccount, MarketGroupV16ViewMut,
    PortfolioAccountV16Account, PortfolioLegV16, PortfolioV16ViewMut, ProvenanceHeaderV16,
    ProvenanceHeaderV16Account, TradeRequestV16, V16Config, V16Error, BOUND_SCALE, POS_SCALE,
};

/// Base size of every leg: 1,000 units.
const Q: i128 = (1_000 * POS_SCALE) as i128;
/// Deposits.
const L_CAPITAL: u128 = 1_000_000;
const S_CAPITAL: u128 = 100_000;
const S2_CAPITAL: u128 = 10_000_000;

fn account_fixture(seed: u8) -> PortfolioAccountV16Account {
    let header = ProvenanceHeaderV16Account::from_runtime(&ProvenanceHeaderV16::new(
        [1; 32], [seed; 32], [3; 32],
    ));
    let mut account = PortfolioAccountV16Account::default();
    account.init_empty_in_place(header).unwrap();
    account
}

fn one_asset_market() -> (MarketGroupV16HeaderAccount, Vec<Market<u64>>) {
    let cfg = V16Config::public_user_fund_with_market_slots(1, 1, 0, 10);
    let mut header = MarketGroupV16HeaderAccount::new_dynamic([1; 32], cfg, 1, 0).unwrap();
    let mut markets = vec![Market::new(0, EngineAssetSlotV16Account::default())];
    header
        .activate_empty_asset_slot_not_atomic(0, &mut markets[0].engine, 100, 1)
        .unwrap();
    (header, markets)
}

fn trade(
    m: &mut MarketGroupV16ViewMut<'_, u64>,
    a: &mut PortfolioV16ViewMut<'_>,
    b: &mut PortfolioV16ViewMut<'_>,
    size_q: i128,
    px: u64,
) -> Result<(), V16Error> {
    m.execute_trade_with_fee_loss_stale_scoped_not_atomic(
        a,
        b,
        TradeRequestV16 {
            asset_index: 0,
            size_q,
            exec_price: px,
            fee_bps: 0,
        },
    )
    .map(|_| ())
}

fn crank(
    m: &mut MarketGroupV16ViewMut<'_, u64>,
    x: &mut PortfolioV16ViewMut<'_>,
    slot: u64,
    px: u64,
) {
    for _ in 0..32 {
        let res = m
            .permissionless_auto_crank_not_atomic(
                x,
                AutoCrankWorkV16 {
                    now_slot: slot,
                    observations: &[AutoCrankObservationV16 {
                        asset_index: 0,
                        effective_price: px,
                        funding_rate_e9: 0,
                    }],
                    resolved_close_fee_rate_per_slot: 0,
                },
            )
            .unwrap();
        if res.selected == AutoCrankPlanV16::NoAction {
            return;
        }
    }
    panic!("crank reached no fixpoint");
}

fn leg_of(a: &PortfolioV16ViewMut<'_>) -> PortfolioLegV16 {
    for slot in 0..8 {
        let leg = a.header.legs[slot].try_to_runtime().unwrap();
        if leg.active && leg.asset_index == 0 {
            return leg;
        }
    }
    panic!("no active leg");
}

/// Effective size of a leg (`ceil(raw * current_a / a_basis)`), the unit the
/// trade request is denominated in.
fn effective_units(m: &MarketGroupV16ViewMut<'_, u64>, leg: PortfolioLegV16) -> u128 {
    let asset = m.markets[0].engine.asset.try_to_runtime().unwrap();
    let raw = leg.basis_pos_q.unsigned_abs();
    let num = raw.checked_mul(asset.a_long).unwrap();
    num.div_ceil(leg.a_basis)
}

/// Size that sells one effective unit less than the whole position, i.e. a
/// same-side Resize rather than a Clear.
fn resize_leaving_one_unit(m: &MarketGroupV16ViewMut<'_, u64>, leg: PortfolioLegV16) -> i128 {
    -((effective_units(m, leg) - POS_SCALE) as i128)
}

/// (claims, fresh reserved backing) of a domain, in atoms.
fn domain_state(m: &MarketGroupV16ViewMut<'_, u64>, domain: usize) -> (u128, u128) {
    let e = &m.markets[0].engine;
    let sc = if domain == 0 {
        e.source_credit_long.try_to_runtime().unwrap()
    } else {
        e.source_credit_short.try_to_runtime().unwrap()
    };
    (
        sc.positive_claim_bound_num / BOUND_SCALE,
        sc.fresh_reserved_backing_num / BOUND_SCALE,
    )
}

/// Runs the scenario and returns (L1 net, L2 net, domain-1 claims/backing after B).
fn run() -> (i128, i128, (u128, u128)) {
    let (mut header, mut markets) = one_asset_market();
    let mut l1h = account_fixture(80);
    let mut l2h = account_fixture(81);
    let mut sh = account_fixture(82);
    let mut s2h = account_fixture(83);
    let mut m = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut l1 = PortfolioV16ViewMut::new(&mut l1h);
    let mut l2 = PortfolioV16ViewMut::new(&mut l2h);
    let mut s = PortfolioV16ViewMut::new(&mut sh);
    let mut s2 = PortfolioV16ViewMut::new(&mut s2h);

    m.deposit_not_atomic(&mut l1, L_CAPITAL).unwrap();
    m.deposit_not_atomic(&mut l2, L_CAPITAL).unwrap();
    m.deposit_not_atomic(&mut s, S_CAPITAL).unwrap();
    m.deposit_not_atomic(&mut s2, S2_CAPITAL).unwrap();

    // L1 long 1,000 vs S short 1,000; L2 long 1,000 vs S2 short 1,000.
    trade(&mut m, &mut l1, &mut s, Q, 100).unwrap();
    trade(&mut m, &mut l2, &mut s2, Q, 100).unwrap();

    // Price 100 -> 200 -> 300.
    m.set_asset_raw_oracle_target_not_atomic(0, 200).unwrap();
    m.accrue_asset_to_not_atomic(0, 2, 200, 0, true).unwrap();
    m.set_asset_raw_oracle_target_not_atomic(0, 300).unwrap();
    m.accrue_asset_to_not_atomic(0, 3, 300, 0, true).unwrap();

    // Every account refreshes at 300. S is now 100,000 short of its loss.
    for x in [&mut s2, &mut l2, &mut s, &mut l1] {
        m.full_account_refresh_not_atomic(x).unwrap();
    }
    assert_eq!(s.header.pnl.get(), -100_000);
    assert_eq!(s.header.capital.get(), 0);

    let cert_before = l1.header.health_cert.try_to_runtime().unwrap();
    let risk_epoch_before = m.header.risk_epoch.get();

    // Liquidate S: the 100,000 residual is booked into B_long (50,000 per full-weight long).
    let out = m
        .liquidate_account_not_atomic(&mut s, LiquidationRequestV16 { asset_index: 0 })
        .unwrap();
    assert_eq!(out.residual_booked, 100_000);

    let asset = m.markets[0].engine.asset.try_to_runtime().unwrap();
    let leg = leg_of(&l1);
    // The booking raised B but left the winner's settlement state marked current.
    assert_eq!(asset.b_long_num, 50_000_000_000_000_000);
    assert_eq!(leg.b_snap, 0);
    assert_eq!(leg.loss_weight, 1_000_000_000);
    assert!(!leg.b_stale);
    assert_eq!(m.header.risk_epoch.get(), risk_epoch_before);
    assert_eq!(
        l1.header
            .health_cert
            .try_to_runtime()
            .unwrap()
            .cert_risk_epoch,
        cert_before.cert_risk_epoch
    );

    // L1 sells all but one effective unit. The position's effective quantity is
    // `raw * a_long / a_basis`; liquidation of S halved A, so 1,000 units of raw
    // basis are 500 effective units. Leave one unit: 499,000,000 effective.
    let sell = resize_leaving_one_unit(&m, leg_of(&l1));
    let resized = trade(&mut m, &mut l1, &mut s2, sell, 300);
    match resized {
        // Before the fix the resize lands against an unsettled B: the weight is
        // rewritten from 1e9 to 2e6 while b_snap is still 0.
        Ok(()) => assert!(leg_of(&l1).loss_weight < leg.loss_weight),
        // With the gate the resize is refused; the wrapper settles B first.
        Err(V16Error::Stale) => {
            crank(&mut m, &mut l1, 3, 300);
            trade(&mut m, &mut l1, &mut s2, sell, 300).unwrap();
        }
        Err(other) => panic!("unexpected resize error: {other:?}"),
    }

    // Both winners settle their B share, then flatten.
    crank(&mut m, &mut l1, 3, 300);
    crank(&mut m, &mut l2, 3, 300);
    assert_eq!(leg_of(&l1).b_snap, 50_000_000_000_000_000);
    assert_eq!(leg_of(&l2).b_snap, 50_000_000_000_000_000);

    let domain_after_b = domain_state(&m, 1);
    trade(&mut m, &mut l1, &mut s2, -(POS_SCALE as i128), 300).unwrap();
    let l2_flat = -(effective_units(&m, leg_of(&l2)) as i128);
    trade(&mut m, &mut l2, &mut s2, l2_flat, 300).unwrap();
    assert!(l1.header.active_bitmap.map(|x| x.get()) == [0]);
    assert!(l2.header.active_bitmap.map(|x| x.get()) == [0]);

    // Realize the gains and withdraw everything.
    let mut paid = [0u128; 2];
    for (i, x) in [&mut l1, &mut l2].into_iter().enumerate() {
        m.full_account_refresh_not_atomic(x).unwrap();
        m.convert_released_pnl_to_capital_not_atomic(x).unwrap();
        let capital = x.header.capital.get();
        m.withdraw_not_atomic(x, capital).unwrap();
        paid[i] = capital;
    }

    let l1_paid = paid[0];
    let l2_paid = paid[1];
    (
        l1_paid as i128 - L_CAPITAL as i128,
        l2_paid as i128 - L_CAPITAL as i128,
        domain_after_b,
    )
}

#[test]
fn same_side_resize_cannot_shrink_the_booked_loss_share() {
    let (l1_net, l2_net, (claims, backing)) = run();
    assert_eq!(
        (l1_net, l2_net),
        (150_000, 150_000),
        "each winner owes 50,000 of the 100,000 residual; domain 1 after B: \
         {claims} claims against {backing} backing"
    );
}
