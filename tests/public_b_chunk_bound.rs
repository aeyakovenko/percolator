// The public B chunk is bounded by MAX_VAULT_TVL when a market is created. Without
// the bound, a market created with a chunk of 340282366920938463 atoms or more was
// accepted, and after a social loss on a side every B-settlement step of an account
// on that side failed with ArithmeticOverflow, so those accounts could neither be
// cranked nor close their positions.
use percolator::{
    AutoCrankObservationV16, AutoCrankWorkV16, EngineAssetSlotV16Account, Market,
    MarketGroupV16HeaderAccount, MarketGroupV16ViewMut, PortfolioAccountV16Account,
    PortfolioV16ViewMut, ProvenanceHeaderV16, ProvenanceHeaderV16Account, TradeRequestV16,
    V16Config, V16Error, V16PodU128, MAX_VAULT_TVL, POS_SCALE, SOCIAL_LOSS_DEN,
};

fn config_with_chunk(chunk: u128) -> V16Config {
    let mut cfg = V16Config::public_user_fund_with_market_slots(1, 1, 0, 10);
    cfg.public_b_chunk_atoms = chunk;
    cfg
}

fn create_market(chunk: u128) -> Result<MarketGroupV16HeaderAccount, V16Error> {
    MarketGroupV16HeaderAccount::new_dynamic([1u8; 32], config_with_chunk(chunk), 1, 0)
}

#[test]
fn a_chunk_above_the_largest_vault_is_refused() {
    for chunk in [
        MAX_VAULT_TVL + 1,
        340_282_366_920_938_462,
        340_282_366_920_938_463,
        u128::MAX,
    ] {
        assert_eq!(
            config_with_chunk(chunk).validate_public_user_fund(),
            Err(V16Error::InvalidConfig)
        );
        assert_eq!(create_market(chunk).err(), Some(V16Error::InvalidConfig));
    }
    for chunk in [1, MAX_VAULT_TVL] {
        assert_eq!(config_with_chunk(chunk).validate_public_user_fund(), Ok(()));
        assert!(create_market(chunk).is_ok());
    }
}

/// Opens a long and a short position, books a social loss of 50 atoms onto the
/// long side, cranks the long account until the engine has nothing left to do for
/// it, and then lets it sell its position back.
fn close_after_social_loss(chunk: u128) -> Result<(), V16Error> {
    let market_id = [1u8; 32];
    let owner = [4u8; 32];
    let mut header = create_market(chunk)?;
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
    // One crank settles at most one chunk of the loss: 50 for a chunk of one atom.
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
fn an_account_on_a_side_with_a_social_loss_can_close_at_the_largest_admitted_chunk() {
    assert_eq!(close_after_social_loss(MAX_VAULT_TVL), Ok(()));
    assert_eq!(close_after_social_loss(1), Ok(()));
}

#[test]
fn the_chunks_that_used_to_freeze_positions_are_refused_at_creation() {
    assert_eq!(
        close_after_social_loss(340_282_366_920_938_463),
        Err(V16Error::InvalidConfig)
    );
    assert_eq!(
        close_after_social_loss(u128::MAX),
        Err(V16Error::InvalidConfig)
    );
}
