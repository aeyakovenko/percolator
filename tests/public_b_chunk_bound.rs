// The public B chunk, the most collateral atoms one B-settlement step moves, is the
// constant PUBLIC_B_CHUNK_ATOMS (= MAX_VAULT_TVL). Until 2026-11-22 it was a market
// setting: a creator could choose a chunk of 340282366920938463 atoms or more, and
// after a social loss on a side every B-settlement step of an account on that side
// failed with ArithmeticOverflow, so those accounts could neither be cranked nor close
// their positions; or a chunk of one atom, so that every settlement crawled.
use percolator::{
    AutoCrankObservationV16, AutoCrankWorkV16, EngineAssetSlotV16Account, Market,
    MarketGroupV16HeaderAccount, MarketGroupV16ViewMut, PortfolioAccountV16Account,
    PortfolioV16ViewMut, ProvenanceHeaderV16, ProvenanceHeaderV16Account, TradeRequestV16,
    V16Config, V16Error, V16PodU128, MAX_VAULT_TVL, POS_SCALE, PUBLIC_B_CHUNK_ATOMS,
    SOCIAL_LOSS_DEN,
};

fn create_market() -> Result<MarketGroupV16HeaderAccount, V16Error> {
    let cfg = V16Config::public_user_fund_with_market_slots(1, 1, 0, 10);
    MarketGroupV16HeaderAccount::new_dynamic([1u8; 32], cfg, 1, 0)
}

#[test]
fn the_public_b_chunk_is_the_largest_vault_and_never_overflows_a_step() {
    assert_eq!(PUBLIC_B_CHUNK_ATOMS, MAX_VAULT_TVL);
    // the B-settlement step's largest numerator, (chunk + 1) * SOCIAL_LOSS_DEN - 1
    assert!((PUBLIC_B_CHUNK_ATOMS + 1)
        .checked_mul(SOCIAL_LOSS_DEN)
        .is_some());
}

/// Opens a long and a short position, books a social loss of 50 atoms onto the
/// long side, cranks the long account until the engine has nothing left to do for
/// it, and then lets it sell its position back.
fn close_after_social_loss() -> Result<(), V16Error> {
    let market_id = [1u8; 32];
    let owner = [4u8; 32];
    let mut header = create_market()?;
    let mut markets = vec![Market::new(0u64, EngineAssetSlotV16Account::default())];
    header
        .activate_empty_asset_slot_not_atomic(0, &mut markets[0].engine, 100, 1)
        .unwrap();
    let mut long_header = PortfolioAccountV16Account::default();
    long_header
        .init_empty_in_place(ProvenanceHeaderV16Account::from_runtime(
            &ProvenanceHeaderV16::new(market_id, [9; 32], owner),
        ))
        .unwrap();
    let mut short_header = PortfolioAccountV16Account::default();
    short_header
        .init_empty_in_place(ProvenanceHeaderV16Account::from_runtime(
            &ProvenanceHeaderV16::new(market_id, [10; 32], owner),
        ))
        .unwrap();
    let fill = TradeRequestV16 {
        asset_index: 0,
        size_q: POS_SCALE as i128,
        exec_price: 100,
        fee_bps: 0,
    };
    let loss_weight = {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        let mut long = PortfolioV16ViewMut::new(&mut long_header);
        let mut short = PortfolioV16ViewMut::new(&mut short_header);
        market.deposit_not_atomic(&mut long, 10_000).unwrap();
        market.deposit_not_atomic(&mut short, 10_000).unwrap();
        market
            .execute_trade_with_fee_loss_stale_scoped_not_atomic(&mut long, &mut short, fill)
            .unwrap();
        long.header
            .legs
            .iter()
            .find_map(|leg| {
                let leg = leg.try_to_runtime().ok()?;
                (leg.active && leg.asset_index == 0).then_some(leg.loss_weight)
            })
            .unwrap()
    };
    markets[0].engine.asset.b_long_num = V16PodU128::new(50 * SOCIAL_LOSS_DEN / loss_weight);
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut long = PortfolioV16ViewMut::new(&mut long_header);
    let mut short = PortfolioV16ViewMut::new(&mut short_header);
    let now_slot = market.header.current_slot.get();
    // One crank settles at most one chunk of the loss, so the whole 50 atoms at once.
    for _ in 0..64 {
        let out = market.permissionless_auto_crank_not_atomic(
            &mut long,
            AutoCrankWorkV16 {
                now_slot,
                observations: &[AutoCrankObservationV16 {
                    asset_index: 0,
                    effective_price: 100,
                    funding_rate_e9: 0,
                }],
                resolved_close_fee_rate_per_slot: 0,
            },
        )?;
        if format!("{out:?}").contains("NoAction") {
            break;
        }
    }
    market
        .execute_trade_with_fee_loss_stale_scoped_not_atomic(&mut short, &mut long, fill)
        .map(|_| ())
}

#[test]
fn an_account_on_a_side_with_a_social_loss_can_close() {
    assert_eq!(close_after_social_loss(), Ok(()));
}
