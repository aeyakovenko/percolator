//! A bankruptcy booking must leave the winners' settlement state stale.
//!
//! `book_bankruptcy_residual_chunk_internal` raises `B_side_num` but bumps no
//! epoch and sets no `b_stale` flag. The winner's certificate therefore still
//! passes `kernel_cert_is_current`, and a same-side Resize - the only route that
//! rewrites `loss_weight` without passing `clear_leg_at_slot_inner` - is accepted.
//!
//! This test pins the engine state right after the booking and the gate on the
//! resize route, without the portfolio PnL path (see `poc_ca_b_skip_resize_e2e`).
#![allow(dead_code)]

use percolator::{
    AutoCrankObservationV16, AutoCrankPlanV16, AutoCrankWorkV16, EngineAssetSlotV16Account,
    LiquidationRequestV16, Market, MarketGroupV16HeaderAccount, MarketGroupV16ViewMut,
    PortfolioAccountV16Account, PortfolioLegV16, PortfolioV16ViewMut, ProvenanceHeaderV16,
    ProvenanceHeaderV16Account, TradeRequestV16, V16Config, V16Error, POS_SCALE,
};

const Q: i128 = (1_000 * POS_SCALE) as i128;

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
) -> Result<(), V16Error> {
    m.execute_trade_with_fee_loss_stale_scoped_not_atomic(
        a,
        b,
        TradeRequestV16 {
            asset_index: 0,
            size_q,
            exec_price: 300,
            fee_bps: 0,
        },
    )
    .map(|_| ())
}

fn crank(m: &mut MarketGroupV16ViewMut<'_, u64>, x: &mut PortfolioV16ViewMut<'_>) {
    for _ in 0..32 {
        let res = m
            .permissionless_auto_crank_not_atomic(
                x,
                AutoCrankWorkV16 {
                    now_slot: 3,
                    observations: &[AutoCrankObservationV16 {
                        asset_index: 0,
                        effective_price: 300,
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

/// Size that sells one effective unit less than the whole position: a
/// same-side Resize, not a Clear.
fn resize_leaving_one_unit(m: &MarketGroupV16ViewMut<'_, u64>, leg: PortfolioLegV16) -> i128 {
    let asset = m.markets[0].engine.asset.try_to_runtime().unwrap();
    let effective = leg
        .basis_pos_q
        .unsigned_abs()
        .checked_mul(asset.a_long)
        .unwrap()
        .div_ceil(leg.a_basis);
    -((effective - POS_SCALE) as i128)
}

#[test]
fn resize_is_rejected_while_the_booked_b_delta_is_unsettled() {
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

    m.deposit_not_atomic(&mut l1, 1_000_000).unwrap();
    m.deposit_not_atomic(&mut l2, 1_000_000).unwrap();
    m.deposit_not_atomic(&mut s, 100_000).unwrap();
    m.deposit_not_atomic(&mut s2, 10_000_000).unwrap();
    trade(&mut m, &mut l1, &mut s, Q).unwrap();
    trade(&mut m, &mut l2, &mut s2, Q).unwrap();

    m.set_asset_raw_oracle_target_not_atomic(0, 200).unwrap();
    m.accrue_asset_to_not_atomic(0, 2, 200, 0, true).unwrap();
    m.set_asset_raw_oracle_target_not_atomic(0, 300).unwrap();
    m.accrue_asset_to_not_atomic(0, 3, 300, 0, true).unwrap();
    for x in [&mut s2, &mut l2, &mut s, &mut l1] {
        m.full_account_refresh_not_atomic(x).unwrap();
    }

    let risk_epoch_before = m.header.risk_epoch.get();
    let cert_before = l1.header.health_cert.try_to_runtime().unwrap();
    let out = m
        .liquidate_account_not_atomic(&mut s, LiquidationRequestV16 { asset_index: 0 })
        .unwrap();
    assert_eq!(out.residual_booked, 100_000);

    let asset = m.markets[0].engine.asset.try_to_runtime().unwrap();
    let leg_before = leg_of(&l1);
    // The booking raised the long side's B index by residual * SOCIAL_LOSS_DEN /
    // loss_weight_sum = 100,000 * 1e21 / 2e9 = 5e16, and did nothing else.
    assert_eq!(asset.b_long_num, 50_000_000_000_000_000);
    assert_eq!(leg_before.b_snap, 0);
    assert_eq!(leg_before.b_epoch_snap, asset.epoch_long);
    assert_eq!(m.header.risk_epoch.get(), risk_epoch_before);
    assert_eq!(
        l1.header
            .health_cert
            .try_to_runtime()
            .unwrap()
            .cert_risk_epoch,
        cert_before.cert_risk_epoch
    );
    assert!(!leg_before.b_stale);

    // One effective unit less than the whole position: a same-side Resize.
    let sell = resize_leaving_one_unit(&m, leg_before);
    let resized = trade(&mut m, &mut l1, &mut s2, sell);
    match resized {
        Ok(()) => panic!(
            "resize ran with B unsettled: loss_weight {} -> {} (b_snap {})",
            leg_before.loss_weight,
            leg_of(&l1).loss_weight,
            leg_of(&l1).b_snap
        ),
        Err(V16Error::Stale) => {}
        Err(other) => panic!("unexpected resize error: {other:?}"),
    }
    assert_eq!(leg_of(&l1), leg_before);

    // Once the deferred delta is settled the identical resize is allowed.
    crank(&mut m, &mut l1);
    trade(&mut m, &mut l1, &mut s2, sell).unwrap();
    assert_eq!(leg_of(&l1).loss_weight, 2_000_000);
}
