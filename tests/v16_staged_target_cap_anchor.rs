//! INV-045 (program charter): the path accrual moves the mark toward its target by at most
//! `anchor * cap` per slot, where the anchor (`fund_px_last`) is the mark when the current target
//! took effect. A target can take effect two ways: the path accrual is given it, or it is staged
//! first (`set_asset_raw_oracle_targets_not_atomic`, which the program's trade route uses) and the
//! path accrual is then given the same target. Both must start the same anchor. Before the fix a
//! staged target kept the anchor of the earlier target: from a mark of 10,000 with an anchor of
//! 1,000,000 one slot moved the mark 10,000 (100%) instead of 100 (1%).
use percolator::{
    canonical_accrual_price_step_v16, v16_domain_count_for_market_slots, AccrualStepV16,
    AssetStateV16, EngineAssetSlotV16Account, Market, MarketGroupV16HeaderAccount,
    MarketGroupV16ViewMut, PortfolioAccountV16Account, PortfolioV16ViewMut, ProvenanceHeaderV16,
    ProvenanceHeaderV16Account, TradeRequestV16, V16Config, POS_SCALE,
};

const P0: u64 = 1_000_000;
const CAP_BPS: u64 = 100;
const DT: u64 = 10;

fn ids() -> ([u8; 32], [u8; 32], [u8; 32]) {
    ([1; 32], [2; 32], [3; 32])
}

fn account_fixture(market_slots: u32, account_seed: u8) -> PortfolioAccountV16Account {
    let (market_id, _, owner) = ids();
    let header = ProvenanceHeaderV16Account::from_runtime(&ProvenanceHeaderV16::new(
        market_id,
        [account_seed; 32],
        owner,
    ));
    let _ = v16_domain_count_for_market_slots(market_slots).unwrap();
    let mut account = PortfolioAccountV16Account::default();
    account.init_empty_in_place(header).unwrap();
    account
}

fn asset(markets: &[Market<u64>]) -> AssetStateV16 {
    markets[0].engine.asset.try_to_runtime().unwrap()
}

/// `n` canonical one-slot steps from `price` toward `target` with the given anchor and first
/// remainder, as the program builds them.
fn path(mut price: u64, target: u64, anchor: u64, mut rem: u16, n: u64) -> Vec<AccrualStepV16> {
    let mut steps = Vec::new();
    for _ in 0..n {
        let before = rem;
        let (p, r) =
            canonical_accrual_price_step_v16(price, target, anchor, CAP_BPS, true, rem).unwrap();
        price = p;
        rem = r;
        steps.push(AccrualStepV16 {
            effective_price: p,
            funding_rate_e9: 0,
            price_move_remainder_before_bps_num: before,
            price_move_remainder_after_bps_num: r,
        });
    }
    steps
}

/// An exposed asset at P0 whose mark an oracle target of 1 has driven down for 99 slots: the mark
/// is 10,000 and the anchor is still P0 (the target was never reached).
fn exposed_asset_far_from_its_anchor() -> (MarketGroupV16HeaderAccount, Vec<Market<u64>>) {
    let (market_id, _, _) = ids();
    let mut cfg = V16Config::public_user_fund_with_market_slots(1, 1, 0, 10);
    cfg.max_accrual_dt_slots = DT;
    cfg.min_funding_lifetime_slots = 10;
    cfg.max_abs_funding_e9_per_slot = 10_000;
    cfg.max_price_move_bps_per_slot = CAP_BPS;
    let mut header = MarketGroupV16HeaderAccount::new_dynamic(market_id, cfg, 1, 0).unwrap();
    let mut markets = vec![Market::new(0, EngineAssetSlotV16Account::default())];
    header
        .activate_empty_asset_slot_not_atomic(0, &mut markets[0].engine, P0, 1)
        .unwrap();
    let mut long_header = account_fixture(1, 225);
    let mut short_header = account_fixture(1, 226);
    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        let mut long = PortfolioV16ViewMut::new(&mut long_header);
        let mut short = PortfolioV16ViewMut::new(&mut short_header);
        market.deposit_not_atomic(&mut long, 100_000_000_000).unwrap();
        market.deposit_not_atomic(&mut short, 100_000_000_000).unwrap();
        market
            .execute_trade_with_fee_loss_stale_scoped_not_atomic(
                &mut long,
                &mut short,
                TradeRequestV16 {
                    asset_index: 0,
                    size_q: (4 * POS_SCALE) as i128,
                    exec_price: P0,
                    fee_bps: 0,
                },
            )
            .unwrap();
    }
    let a = asset(&markets);
    assert!(a.oi_eff_long_q != 0 && a.oi_eff_short_q != 0, "the asset must be exposed");
    let mut slots_left = 99u64;
    while slots_left > 0 {
        let n = slots_left.min(DT);
        let a = asset(&markets);
        // 1_000_000 * 100 is a multiple of 10_000, so every remainder of this trajectory is 0
        let steps = path(a.effective_price, 1, P0, 0, n);
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        market
            .accrue_asset_path_to_not_atomic(0, a.slot_last + n, 1, &steps, true)
            .unwrap();
        slots_left -= n;
    }
    let a = asset(&markets);
    assert_eq!(
        (a.effective_price, a.fund_px_last, a.raw_oracle_target_price),
        (10_000, P0, 1)
    );
    (header, markets)
}

#[test]
fn v16_staged_target_starts_the_same_cap_anchor_as_a_target_given_to_the_path() {
    let (header, markets) = exposed_asset_far_from_its_anchor();
    let before = asset(&markets);
    let (mark, slot) = (before.effective_price, before.slot_last);

    // staged: the trade route stages the new target, the crank then accrues with the same target
    let (mut h_s, mut m_s) = (header, markets.clone());
    {
        let mut market = MarketGroupV16ViewMut::new(&mut h_s, &mut m_s);
        market.set_asset_raw_oracle_targets_not_atomic(&[(0, P0)]).unwrap();
    }
    assert_eq!(asset(&m_s).fund_px_last, mark, "a changed target restarts the anchor at the mark");
    {
        let steps = path(mark, P0, asset(&m_s).fund_px_last, 0, 1);
        let mut market = MarketGroupV16ViewMut::new(&mut h_s, &mut m_s);
        market
            .accrue_asset_path_to_not_atomic(0, slot + 1, P0, &steps, true)
            .unwrap();
    }
    // given: the path accrual sees the target change itself and anchors at the mark
    let (mut h_g, mut m_g) = (header, markets.clone());
    {
        let steps = path(mark, P0, mark, 0, 1);
        let mut market = MarketGroupV16ViewMut::new(&mut h_g, &mut m_g);
        market
            .accrue_asset_path_to_not_atomic(0, slot + 1, P0, &steps, true)
            .unwrap();
    }
    assert_eq!(asset(&m_s), asset(&m_g), "staged and given targets leave the same asset");
    assert_eq!(asset(&m_s).effective_price, mark + mark * CAP_BPS / 10_000);
}

#[test]
fn v16_staged_round_trip_restarts_the_anchor_and_an_unchanged_target_keeps_it() {
    let (header, markets) = exposed_asset_far_from_its_anchor();
    let mark = asset(&markets).effective_price;

    // re-staging the target in force changes nothing: the trajectory toward it stays linear
    let (mut h_u, mut m_u) = (header, markets.clone());
    {
        let mut market = MarketGroupV16ViewMut::new(&mut h_u, &mut m_u);
        market.set_asset_raw_oracle_targets_not_atomic(&[(0, 1)]).unwrap();
    }
    assert_eq!(asset(&m_u), asset(&markets), "an unchanged target keeps the anchor");

    // a round trip 1 -> 500_000 -> 1: each change restarts the anchor at the mark
    let (mut h_r, mut m_r) = (header, markets.clone());
    {
        let mut market = MarketGroupV16ViewMut::new(&mut h_r, &mut m_r);
        market.set_asset_raw_oracle_targets_not_atomic(&[(0, 500_000)]).unwrap();
    }
    assert_eq!(asset(&m_r).fund_px_last, mark);
    {
        let mut market = MarketGroupV16ViewMut::new(&mut h_r, &mut m_r);
        market.set_asset_raw_oracle_targets_not_atomic(&[(0, 1)]).unwrap();
    }
    let a = asset(&m_r);
    assert_eq!((a.fund_px_last, a.raw_oracle_target_price), (mark, 1));
    // the next slot toward 1 moves the mark by the cap of the new anchor, not of the old one
    {
        let steps = path(mark, 1, a.fund_px_last, 0, 1);
        let mut market = MarketGroupV16ViewMut::new(&mut h_r, &mut m_r);
        market
            .accrue_asset_path_to_not_atomic(0, a.slot_last + 1, 1, &steps, true)
            .unwrap();
    }
    assert_eq!(asset(&m_r).effective_price, mark - mark * CAP_BPS / 10_000);
}
