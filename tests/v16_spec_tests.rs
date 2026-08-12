use percolator::{
    active_bitmap_is_empty, auto_crank_plan_requires_caller_observation, AutoCrankObservationV16,
    AutoCrankOutcomeV16, AutoCrankPlanV16, AutoCrankWorkV16,
};
use percolator::{
    v16_domain_count_for_market_slots, AssetLifecycleV16, AssetStateV16Account,
    BackingBucketStatusV16, BackingBucketV16, BackingBucketV16Account, EngineAssetSlotV16Account,
    HealthCertV16, HealthCertV16Account, LiquidationRequestV16, Market,
    MarketGroupV16HeaderAccount, MarketGroupV16ViewMut, PermissionlessProgressOutcomeV16,
    PermissionlessRecoveryReasonV16, PortfolioAccountV16Account, PortfolioLegV16,
    PortfolioLegV16Account, PortfolioSourceDomainV16Account, PortfolioV16View, PortfolioV16ViewMut,
    ProvenanceHeaderV16, ProvenanceHeaderV16Account, RebalanceRequestV16, ResolvedPayoutLedgerV16,
    ResolvedPayoutLedgerV16Account, ResolvedPayoutReceiptV16, ResolvedPayoutReceiptV16Account,
    SideModeV16, SideV16, SourceCreditStateV16, SourceCreditStateV16Account, TradeRequestV16,
    V16Config, V16Error, V16OptionalRecoveryReasonAccount, V16PodI128, V16PodU128, V16PodU32,
    V16PodU64, V16_EMPTY_ACTIVE_BITMAP,
};
#[cfg(feature = "fuzz")]
use percolator::{PermissionlessCrankActionV16, PermissionlessCrankRequestV16};
use percolator::{ADL_ONE, BOUND_SCALE, CREDIT_RATE_SCALE, POS_SCALE};

const FUNDING_COUNTER_PRICE: u64 = 1_000_000;
const FUNDING_COUNTER_RATE_E9: i128 = 10_000;
const FUNDING_COUNTER_ATOMS_PER_SLOT: u128 = 10;

fn ids() -> ([u8; 32], [u8; 32], [u8; 32]) {
    ([1; 32], [2; 32], [3; 32])
}

fn market_fixture(
    market_slots: u32,
    init_price: u64,
) -> (MarketGroupV16HeaderAccount, Vec<Market<u64>>) {
    let (market_id, _, _) = ids();
    let cfg =
        V16Config::public_user_fund_with_market_slots(market_slots as u16, market_slots, 0, 10);
    let mut header =
        MarketGroupV16HeaderAccount::new_dynamic(market_id, cfg, market_slots, 0).unwrap();
    let mut markets = (0..market_slots)
        .map(|i| Market::new(i as u64, EngineAssetSlotV16Account::default()))
        .collect::<Vec<_>>();
    for i in 0..market_slots as usize {
        header
            .activate_empty_asset_slot_not_atomic(
                i as u32,
                &mut markets[i].engine,
                init_price,
                (i + 1) as u64,
            )
            .unwrap();
    }
    {
        let view = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        view.validate_shape().unwrap();
    }
    (header, markets)
}

fn funding_market_fixture(init_price: u64) -> (MarketGroupV16HeaderAccount, Vec<Market<u64>>) {
    let (market_id, _, _) = ids();
    let mut cfg = V16Config::public_user_fund_with_market_slots(1, 1, 0, 10);
    cfg.max_abs_funding_e9_per_slot = FUNDING_COUNTER_RATE_E9 as u64;
    cfg.max_price_move_bps_per_slot = 9_000;
    let mut header = MarketGroupV16HeaderAccount::new_dynamic(market_id, cfg, 1, 0).unwrap();
    let mut markets = vec![Market::new(0, EngineAssetSlotV16Account::default())];
    header
        .activate_empty_asset_slot_not_atomic(0, &mut markets[0].engine, init_price, 1)
        .unwrap();
    {
        let view = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        view.validate_shape().unwrap();
    }
    (header, markets)
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

fn signed_q(q: u128) -> i128 {
    i128::try_from(q).unwrap()
}

fn funding_counter_tuple(account: &PortfolioAccountV16Account) -> (u128, u128, u128, u128) {
    (
        account.funding_long_paid_atoms_total.get(),
        account.funding_long_received_atoms_total.get(),
        account.funding_short_paid_atoms_total.get(),
        account.funding_short_received_atoms_total.get(),
    )
}

fn open_one_lot_pair(
    market: &mut MarketGroupV16ViewMut<'_, u64>,
    long: &mut PortfolioV16ViewMut<'_>,
    short: &mut PortfolioV16ViewMut<'_>,
) {
    market.deposit_not_atomic(long, 10_000_000).unwrap();
    market.deposit_not_atomic(short, 10_000_000).unwrap();
    market
        .execute_trade_with_fee_loss_stale_scoped_not_atomic(
            long,
            short,
            TradeRequestV16 {
                asset_index: 0,
                size_q: signed_q(POS_SCALE),
                exec_price: FUNDING_COUNTER_PRICE,
                fee_bps: 0,
            },
        )
        .unwrap();
}

#[test]
fn v16_public_fund_validator_accepts_nontrivial_exact_solvency_profile() {
    let mut cfg = V16Config::public_user_fund_with_market_slots(1, 1, 1, 10);
    cfg.maintenance_margin_bps = 10_000;
    cfg.initial_margin_bps = 10_000;
    cfg.max_price_move_bps_per_slot = 100;
    cfg.max_accrual_dt_slots = 1;
    cfg.min_funding_lifetime_slots = 1;
    cfg.max_abs_funding_e9_per_slot = 0;
    cfg.liquidation_fee_bps = 100;
    cfg.min_liquidation_abs = 1;
    cfg.liquidation_fee_cap = 1;
    cfg.min_nonzero_mm_req = 2;
    cfg.min_nonzero_im_req = 3;

    assert_eq!(cfg.validate_public_user_fund(), Ok(()));
}

#[test]
fn v16_view_deposit_and_withdraw_are_the_tested_paths() {
    let (mut header, mut markets) = market_fixture(1, 100);
    let mut account_header = account_fixture(1, 2);
    let mut market_view = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account_view = PortfolioV16ViewMut::new(&mut account_header);

    market_view
        .deposit_not_atomic(&mut account_view, 11)
        .unwrap();
    market_view
        .withdraw_not_atomic(&mut account_view, 4)
        .unwrap();

    assert_eq!(account_view.header.capital.get(), 7);
    assert_eq!(market_view.header.c_tot.get(), 7);
    assert_eq!(market_view.header.vault.get(), 7);
    market_view.validate_shape().unwrap();
    account_view
        .validate_with_market(&market_view.as_view())
        .unwrap();
}

#[test]
fn v16_funding_counter_layout_canary_places_fields_before_fee_state() {
    let width = core::mem::size_of::<V16PodU128>();

    assert_eq!(
        core::mem::offset_of!(PortfolioAccountV16Account, funding_long_paid_atoms_total),
        core::mem::offset_of!(PortfolioAccountV16Account, residual_received_atoms_total) + width
    );
    assert_eq!(
        core::mem::offset_of!(
            PortfolioAccountV16Account,
            funding_long_received_atoms_total
        ),
        core::mem::offset_of!(PortfolioAccountV16Account, funding_long_paid_atoms_total) + width
    );
    assert_eq!(
        core::mem::offset_of!(PortfolioAccountV16Account, funding_short_paid_atoms_total),
        core::mem::offset_of!(
            PortfolioAccountV16Account,
            funding_long_received_atoms_total
        ) + width
    );
    assert_eq!(
        core::mem::offset_of!(
            PortfolioAccountV16Account,
            funding_short_received_atoms_total
        ),
        core::mem::offset_of!(PortfolioAccountV16Account, funding_short_paid_atoms_total) + width
    );
    assert_eq!(
        core::mem::offset_of!(PortfolioAccountV16Account, fee_credits),
        core::mem::offset_of!(
            PortfolioAccountV16Account,
            funding_short_received_atoms_total
        ) + width
    );
}

#[test]
fn v16_funding_counters_record_long_pays_short_once_on_refresh() {
    let (mut header, mut markets) = funding_market_fixture(FUNDING_COUNTER_PRICE);
    let mut long_header = account_fixture(1, 120);
    let mut short_header = account_fixture(1, 121);

    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        let mut long = PortfolioV16ViewMut::new(&mut long_header);
        let mut short = PortfolioV16ViewMut::new(&mut short_header);
        open_one_lot_pair(&mut market, &mut long, &mut short);
        market
            .accrue_asset_to_not_atomic(0, 2, FUNDING_COUNTER_PRICE, FUNDING_COUNTER_RATE_E9, true)
            .unwrap();
    }

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut long = PortfolioV16ViewMut::new(&mut long_header);
    let mut short = PortfolioV16ViewMut::new(&mut short_header);
    market.full_account_refresh_not_atomic(&mut long).unwrap();
    market.full_account_refresh_not_atomic(&mut short).unwrap();

    assert_eq!(
        funding_counter_tuple(long.header),
        (FUNDING_COUNTER_ATOMS_PER_SLOT, 0, 0, 0)
    );
    assert_eq!(
        funding_counter_tuple(short.header),
        (0, 0, 0, FUNDING_COUNTER_ATOMS_PER_SLOT)
    );
    assert_eq!(
        long.header.capital.get(),
        10_000_000 - FUNDING_COUNTER_ATOMS_PER_SLOT
    );
    assert_eq!(
        long.header.funding_long_paid_atoms_total.get(),
        short.header.funding_short_received_atoms_total.get(),
        "payer/receiver funding counters must conserve across both refreshed accounts"
    );

    market.full_account_refresh_not_atomic(&mut long).unwrap();
    market.full_account_refresh_not_atomic(&mut short).unwrap();
    assert_eq!(
        funding_counter_tuple(long.header),
        (FUNDING_COUNTER_ATOMS_PER_SLOT, 0, 0, 0),
        "advancing f_snap must prevent double counting on a later refresh"
    );
    assert_eq!(
        funding_counter_tuple(short.header),
        (0, 0, 0, FUNDING_COUNTER_ATOMS_PER_SLOT)
    );
    market.validate_shape().unwrap();
    long.validate_with_market(&market.as_view()).unwrap();
    short.validate_with_market(&market.as_view()).unwrap();
}

#[test]
fn v16_funding_counters_record_short_pays_long_on_negative_funding() {
    let (mut header, mut markets) = funding_market_fixture(FUNDING_COUNTER_PRICE);
    let mut long_header = account_fixture(1, 122);
    let mut short_header = account_fixture(1, 123);

    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        let mut long = PortfolioV16ViewMut::new(&mut long_header);
        let mut short = PortfolioV16ViewMut::new(&mut short_header);
        open_one_lot_pair(&mut market, &mut long, &mut short);
        market
            .accrue_asset_to_not_atomic(0, 2, FUNDING_COUNTER_PRICE, -FUNDING_COUNTER_RATE_E9, true)
            .unwrap();
    }

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut long = PortfolioV16ViewMut::new(&mut long_header);
    let mut short = PortfolioV16ViewMut::new(&mut short_header);
    market.full_account_refresh_not_atomic(&mut long).unwrap();
    market.full_account_refresh_not_atomic(&mut short).unwrap();

    assert_eq!(
        funding_counter_tuple(long.header),
        (0, FUNDING_COUNTER_ATOMS_PER_SLOT, 0, 0)
    );
    assert_eq!(
        funding_counter_tuple(short.header),
        (0, 0, FUNDING_COUNTER_ATOMS_PER_SLOT, 0)
    );
    assert_eq!(
        long.header.funding_long_received_atoms_total.get(),
        short.header.funding_short_paid_atoms_total.get(),
        "receiver/payer funding counters must conserve when shorts pay longs"
    );
    market.validate_shape().unwrap();
    long.validate_with_market(&market.as_view()).unwrap();
    short.validate_with_market(&market.as_view()).unwrap();
}

#[test]
fn v16_funding_counters_settle_before_same_side_resize() {
    let (mut header, mut markets) = funding_market_fixture(FUNDING_COUNTER_PRICE);
    let mut long_header = account_fixture(1, 124);
    let mut short_header = account_fixture(1, 125);

    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        let mut long = PortfolioV16ViewMut::new(&mut long_header);
        let mut short = PortfolioV16ViewMut::new(&mut short_header);
        open_one_lot_pair(&mut market, &mut long, &mut short);
        market
            .accrue_asset_to_not_atomic(0, 2, FUNDING_COUNTER_PRICE, FUNDING_COUNTER_RATE_E9, true)
            .unwrap();
        market
            .execute_trade_with_fee_loss_stale_scoped_not_atomic(
                &mut long,
                &mut short,
                TradeRequestV16 {
                    asset_index: 0,
                    size_q: signed_q(POS_SCALE),
                    exec_price: FUNDING_COUNTER_PRICE,
                    fee_bps: 0,
                },
            )
            .unwrap();
    }

    let long_leg = long_header.legs[0].try_to_runtime().unwrap();
    let short_leg = short_header.legs[0].try_to_runtime().unwrap();
    assert_eq!(long_leg.basis_pos_q, signed_q(2 * POS_SCALE));
    assert_eq!(short_leg.basis_pos_q, -signed_q(2 * POS_SCALE));
    assert_eq!(
        funding_counter_tuple(&long_header),
        (FUNDING_COUNTER_ATOMS_PER_SLOT, 0, 0, 0)
    );
    assert_eq!(
        funding_counter_tuple(&short_header),
        (0, 0, 0, FUNDING_COUNTER_ATOMS_PER_SLOT)
    );

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut long = PortfolioV16ViewMut::new(&mut long_header);
    let mut short = PortfolioV16ViewMut::new(&mut short_header);
    market.full_account_refresh_not_atomic(&mut long).unwrap();
    market.full_account_refresh_not_atomic(&mut short).unwrap();
    assert_eq!(
        funding_counter_tuple(long.header),
        (FUNDING_COUNTER_ATOMS_PER_SLOT, 0, 0, 0)
    );
    assert_eq!(
        funding_counter_tuple(short.header),
        (0, 0, 0, FUNDING_COUNTER_ATOMS_PER_SLOT)
    );
}

#[test]
fn v16_funding_counters_settle_before_trade_close_clears_leg() {
    let (mut header, mut markets) = funding_market_fixture(FUNDING_COUNTER_PRICE);
    let mut long_header = account_fixture(1, 126);
    let mut short_header = account_fixture(1, 127);

    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        let mut long = PortfolioV16ViewMut::new(&mut long_header);
        let mut short = PortfolioV16ViewMut::new(&mut short_header);
        open_one_lot_pair(&mut market, &mut long, &mut short);
        market
            .accrue_asset_to_not_atomic(0, 2, FUNDING_COUNTER_PRICE, FUNDING_COUNTER_RATE_E9, true)
            .unwrap();
        market
            .execute_trade_with_fee_loss_stale_scoped_not_atomic(
                &mut long,
                &mut short,
                TradeRequestV16 {
                    asset_index: 0,
                    size_q: -signed_q(POS_SCALE),
                    exec_price: FUNDING_COUNTER_PRICE,
                    fee_bps: 0,
                },
            )
            .unwrap();
    }

    assert_eq!(long_header.active_bitmap[0].get(), 0);
    assert_eq!(short_header.active_bitmap[0].get(), 0);
    assert_eq!(
        funding_counter_tuple(&long_header),
        (FUNDING_COUNTER_ATOMS_PER_SLOT, 0, 0, 0)
    );
    assert_eq!(
        funding_counter_tuple(&short_header),
        (0, 0, 0, FUNDING_COUNTER_ATOMS_PER_SLOT)
    );
}

#[test]
fn v16_funding_counters_record_forfeited_dead_leg_settlement() {
    let (mut header, mut markets) = funding_market_fixture(FUNDING_COUNTER_PRICE);
    let mut long_header = account_fixture(1, 128);
    let mut short_header = account_fixture(1, 129);

    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        let mut long = PortfolioV16ViewMut::new(&mut long_header);
        let mut short = PortfolioV16ViewMut::new(&mut short_header);
        open_one_lot_pair(&mut market, &mut long, &mut short);
        market
            .accrue_asset_to_not_atomic(0, 2, FUNDING_COUNTER_PRICE, FUNDING_COUNTER_RATE_E9, true)
            .unwrap();
        market.force_asset_recovery_not_atomic(0, 2).unwrap();
        market
            .forfeit_recovery_leg_not_atomic(&mut long, 0, 1)
            .unwrap();
        market
            .forfeit_recovery_leg_not_atomic(&mut short, 0, 1)
            .unwrap();
    }

    assert_eq!(
        funding_counter_tuple(&long_header),
        (FUNDING_COUNTER_ATOMS_PER_SLOT, 0, 0, 0)
    );
    assert_eq!(
        funding_counter_tuple(&short_header),
        (0, 0, 0, FUNDING_COUNTER_ATOMS_PER_SLOT)
    );
}

#[test]
fn v16_funding_counters_ignore_inactive_accounts_when_market_funding_moves() {
    let (mut header, mut markets) = funding_market_fixture(FUNDING_COUNTER_PRICE);
    let mut long_header = account_fixture(1, 130);
    let mut short_header = account_fixture(1, 131);
    let mut idle_header = account_fixture(1, 132);

    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        let mut long = PortfolioV16ViewMut::new(&mut long_header);
        let mut short = PortfolioV16ViewMut::new(&mut short_header);
        open_one_lot_pair(&mut market, &mut long, &mut short);
        market
            .accrue_asset_to_not_atomic(0, 2, FUNDING_COUNTER_PRICE, FUNDING_COUNTER_RATE_E9, true)
            .unwrap();
    }

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut idle = PortfolioV16ViewMut::new(&mut idle_header);
    market.full_account_refresh_not_atomic(&mut idle).unwrap();

    assert_eq!(funding_counter_tuple(idle.header), (0, 0, 0, 0));
    market.validate_shape().unwrap();
    idle.validate_with_market(&market.as_view()).unwrap();
}

#[test]
fn v16_view_fee_sync_settles_flat_loss_before_fee() {
    let (mut header, mut markets) = market_fixture(1, 100);
    let mut account_header = account_fixture(1, 4);
    header.vault = V16PodU128::new(100);
    header.c_tot = V16PodU128::new(100);
    header.negative_pnl_account_count = V16PodU64::new(1);
    header.current_slot = V16PodU64::new(10);
    header.slot_last = V16PodU64::new(10);
    account_header.capital = V16PodU128::new(100);
    account_header.pnl = V16PodI128::new(-40);

    let mut market_view = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account_view = PortfolioV16ViewMut::new(&mut account_header);
    let charged = market_view
        .sync_account_fee_to_slot_not_atomic(&mut account_view, 10, 10)
        .unwrap();

    assert_eq!(charged, 60);
    assert_eq!(account_view.header.pnl.get(), 0);
    assert_eq!(account_view.header.capital.get(), 0);
    assert_eq!(market_view.header.c_tot.get(), 0);
    assert_eq!(market_view.header.insurance.get(), 60);
    assert_eq!(market_view.header.vault.get(), 100);
    assert_eq!(market_view.header.negative_pnl_account_count.get(), 0);
}

#[test]
fn v16_fee_sync_on_nonflat_account_settles_hidden_k_loss_before_fee() {
    let (mut header, mut markets) = market_fixture(1, 100);
    let mut long_header = account_fixture(1, 14);
    let mut short_header = account_fixture(1, 15);
    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        let mut long = PortfolioV16ViewMut::new(&mut long_header);
        let mut short = PortfolioV16ViewMut::new(&mut short_header);
        market.deposit_not_atomic(&mut long, 100).unwrap();
        market.deposit_not_atomic(&mut short, 1_000).unwrap();
        market
            .execute_trade_with_fee_loss_stale_scoped_not_atomic(
                &mut long,
                &mut short,
                TradeRequestV16 {
                    asset_index: 0,
                    size_q: signed_q(POS_SCALE),
                    exec_price: 100,
                    fee_bps: 0,
                },
            )
            .unwrap();
        market
            .accrue_asset_to_not_atomic(0, 2, 50, 0, true)
            .unwrap();
    }
    assert_eq!(long_header.pnl.get(), 0);
    assert_eq!(long_header.capital.get(), 100);
    assert_eq!(header.insurance.get(), 0);

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut long = PortfolioV16ViewMut::new(&mut long_header);
    let charged = market
        .sync_account_fee_to_slot_not_atomic(&mut long, 2, 100)
        .unwrap();

    assert_eq!(
        charged, 50,
        "lazy K loss must consume principal before recurring fee collection"
    );
    assert_eq!(long.header.capital.get(), 0);
    assert_eq!(long.header.pnl.get(), 0);
    assert_eq!(market.header.insurance.get(), 50);
    market.validate_shape().unwrap();
    long.validate_with_market(&market.as_view()).unwrap();
}

#[test]
fn v16_finalize_side_reset_is_public_value_neutral_and_epoch_bumping() {
    let (mut header, mut markets) = market_fixture(1, 100);
    let vault_before = header.vault.get();
    let c_tot_before = header.c_tot.get();
    let insurance_before = header.insurance.get();
    let risk_epoch_before = header.risk_epoch.get();
    let mut asset = markets[0].engine.asset.try_to_runtime().unwrap();
    asset.mode_long = SideModeV16::ResetPending;
    markets[0].engine.asset = AssetStateV16Account::from_runtime(&asset);

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    market
        .finalize_side_reset_not_atomic(0, SideV16::Long)
        .unwrap();

    let finalized = market.markets[0].engine.asset.try_to_runtime().unwrap();
    assert_eq!(finalized.mode_long, SideModeV16::Normal);
    assert_eq!(market.header.risk_epoch.get(), risk_epoch_before + 1);
    assert_eq!(market.header.vault.get(), vault_before);
    assert_eq!(market.header.c_tot.get(), c_tot_before);
    assert_eq!(market.header.insurance.get(), insurance_before);
    market.validate_shape().unwrap();
}

#[test]
fn v16_finalize_side_reset_rejects_blocked_pending_side() {
    let (mut header, mut markets) = market_fixture(1, 100);
    let risk_epoch_before = header.risk_epoch.get();
    let mut asset = markets[0].engine.asset.try_to_runtime().unwrap();
    asset.mode_short = SideModeV16::ResetPending;
    asset.pending_obligation_count_short = 1;
    markets[0].engine.asset = AssetStateV16Account::from_runtime(&asset);

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    assert_eq!(
        market.finalize_side_reset_not_atomic(0, SideV16::Short),
        Err(V16Error::Stale)
    );

    let blocked = market.markets[0].engine.asset.try_to_runtime().unwrap();
    assert_eq!(blocked.mode_short, SideModeV16::ResetPending);
    assert_eq!(market.header.risk_epoch.get(), risk_epoch_before);
    assert_eq!(market.validate_shape(), Ok(()));
}

#[test]
fn v16_trade_rejects_fresh_risk_when_either_side_is_recovering() {
    let cases = [
        (SideModeV16::ResetPending, SideModeV16::Normal),
        (SideModeV16::Normal, SideModeV16::ResetPending),
        (SideModeV16::DrainOnly, SideModeV16::Normal),
        (SideModeV16::Normal, SideModeV16::DrainOnly),
    ];
    for (mode_long, mode_short) in cases {
        let (mut header, mut markets) = market_fixture(1, 100);
        let mut long_header = account_fixture(1, 16);
        let mut short_header = account_fixture(1, 17);
        {
            let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
            let mut long = PortfolioV16ViewMut::new(&mut long_header);
            let mut short = PortfolioV16ViewMut::new(&mut short_header);
            market.deposit_not_atomic(&mut long, 1_000).unwrap();
            market.deposit_not_atomic(&mut short, 1_000).unwrap();
        }
        let mut asset = markets[0].engine.asset.try_to_runtime().unwrap();
        asset.mode_long = mode_long;
        asset.mode_short = mode_short;
        markets[0].engine.asset = AssetStateV16Account::from_runtime(&asset);
        let value_before = (header.vault, header.c_tot, header.insurance);

        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        let mut long = PortfolioV16ViewMut::new(&mut long_header);
        let mut short = PortfolioV16ViewMut::new(&mut short_header);
        assert_eq!(
            market.execute_trade_with_fee_loss_stale_scoped_not_atomic(
                &mut long,
                &mut short,
                TradeRequestV16 {
                    asset_index: 0,
                    size_q: signed_q(POS_SCALE),
                    exec_price: 100,
                    fee_bps: 0,
                },
            ),
            Err(V16Error::LockActive),
            "fresh risk admitted with {mode_long:?}/{mode_short:?}"
        );

        let asset = market.markets[0].engine.asset.try_to_runtime().unwrap();
        assert_eq!(asset.oi_eff_long_q, 0);
        assert_eq!(asset.oi_eff_short_q, 0);
        assert_eq!(asset.stored_pos_count_long, 0);
        assert_eq!(asset.stored_pos_count_short, 0);
        assert_eq!(
            long.header.active_bitmap,
            V16_EMPTY_ACTIVE_BITMAP.map(V16PodU64::new)
        );
        assert_eq!(
            short.header.active_bitmap,
            V16_EMPTY_ACTIVE_BITMAP.map(V16PodU64::new)
        );
        assert_eq!(
            (
                market.header.vault,
                market.header.c_tot,
                market.header.insurance
            ),
            value_before
        );
    }
}

#[test]
fn v16_trade_keeps_two_sided_risk_reduction_open_during_side_recovery() {
    let (mut header, mut markets) = market_fixture(1, 100);
    let mut long_header = account_fixture(1, 18);
    let mut short_header = account_fixture(1, 19);
    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        let mut long = PortfolioV16ViewMut::new(&mut long_header);
        let mut short = PortfolioV16ViewMut::new(&mut short_header);
        market.deposit_not_atomic(&mut long, 1_000).unwrap();
        market.deposit_not_atomic(&mut short, 1_000).unwrap();
        market
            .execute_trade_with_fee_loss_stale_scoped_not_atomic(
                &mut long,
                &mut short,
                TradeRequestV16 {
                    asset_index: 0,
                    size_q: signed_q(2 * POS_SCALE),
                    exec_price: 100,
                    fee_bps: 0,
                },
            )
            .unwrap();
    }
    let mut asset = markets[0].engine.asset.try_to_runtime().unwrap();
    asset.mode_long = SideModeV16::ResetPending;
    asset.mode_short = SideModeV16::DrainOnly;
    markets[0].engine.asset = AssetStateV16Account::from_runtime(&asset);

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut long = PortfolioV16ViewMut::new(&mut long_header);
    let mut short = PortfolioV16ViewMut::new(&mut short_header);
    market
        .execute_trade_with_fee_loss_stale_scoped_not_atomic(
            &mut long,
            &mut short,
            TradeRequestV16 {
                asset_index: 0,
                size_q: -signed_q(POS_SCALE),
                exec_price: 100,
                fee_bps: 0,
            },
        )
        .expect("recovery must not block a matched reduction of both open sides");

    let asset = market.markets[0].engine.asset.try_to_runtime().unwrap();
    assert_eq!(asset.oi_eff_long_q, POS_SCALE);
    assert_eq!(asset.oi_eff_short_q, POS_SCALE);
    assert_eq!(asset.stored_pos_count_long, 1);
    assert_eq!(asset.stored_pos_count_short, 1);
    assert_eq!(
        long.header.legs[0].try_to_runtime().unwrap().basis_pos_q,
        signed_q(POS_SCALE)
    );
    assert_eq!(
        short.header.legs[0].try_to_runtime().unwrap().basis_pos_q,
        -signed_q(POS_SCALE)
    );
    market.validate_shape().unwrap();
    long.validate_with_market(&market.as_view()).unwrap();
    short.validate_with_market(&market.as_view()).unwrap();
}

#[test]
fn v16_crossed_trade_cannot_spend_same_call_addition_as_preexisting_oi() {
    const SURVIVOR_Q: u128 = 13 * POS_SCALE;
    const MATCHED_Q: u128 = 10 * POS_SCALE;
    const CROSS_Q: u128 = 11 * POS_SCALE;

    let (mut header, mut markets) = market_fixture(1, 1);
    let mut survivor_header = account_fixture(1, 20);
    let mut liquidated_header = account_fixture(1, 21);
    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        let mut survivor = PortfolioV16ViewMut::new(&mut survivor_header);
        let mut liquidated = PortfolioV16ViewMut::new(&mut liquidated_header);
        market.deposit_not_atomic(&mut survivor, 100).unwrap();
        market.deposit_not_atomic(&mut liquidated, 100).unwrap();
    }

    let mut asset = markets[0].engine.asset.try_to_runtime().unwrap();
    asset.a_long = ADL_ONE * MATCHED_Q / SURVIVOR_Q;
    asset.oi_eff_long_q = MATCHED_Q;
    asset.oi_eff_short_q = MATCHED_Q;
    asset.stored_pos_count_long = 1;
    asset.stored_pos_count_short = 1;
    asset.loss_weight_sum_long = SURVIVOR_Q;
    asset.loss_weight_sum_short = MATCHED_Q;
    markets[0].engine.asset = AssetStateV16Account::from_runtime(&asset);
    header.resolved_payout_blocker_count = V16PodU64::new(2);

    survivor_header.legs[0] = PortfolioLegV16Account::from_runtime(&PortfolioLegV16 {
        active: true,
        asset_index: 0,
        market_id: asset.market_id,
        side: SideV16::Long,
        basis_pos_q: signed_q(SURVIVOR_Q),
        a_basis: ADL_ONE,
        k_snap: asset.k_long,
        f_snap: asset.f_long_num,
        epoch_snap: asset.epoch_long,
        loss_weight: SURVIVOR_Q,
        b_snap: asset.b_long_num,
        b_rem: 0,
        b_epoch_snap: asset.epoch_long,
        b_stale: false,
        stale: false,
    });
    survivor_header.active_bitmap[0] = V16PodU64::new(1);
    survivor_header.health_cert.valid = 0;
    liquidated_header.legs[0] = PortfolioLegV16Account::from_runtime(&PortfolioLegV16 {
        active: true,
        asset_index: 0,
        market_id: asset.market_id,
        side: SideV16::Short,
        basis_pos_q: -signed_q(MATCHED_Q),
        a_basis: ADL_ONE,
        k_snap: asset.k_short,
        f_snap: asset.f_short_num,
        epoch_snap: asset.epoch_short,
        loss_weight: MATCHED_Q,
        b_snap: asset.b_short_num,
        b_rem: 0,
        b_epoch_snap: asset.epoch_short,
        b_stale: false,
        stale: false,
    });
    liquidated_header.active_bitmap[0] = V16PodU64::new(1);
    liquidated_header.health_cert.valid = 0;

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut survivor = PortfolioV16ViewMut::new(&mut survivor_header);
    let mut liquidated = PortfolioV16ViewMut::new(&mut liquidated_header);
    market.validate_shape().unwrap();
    survivor.validate_with_market(&market.as_view()).unwrap();
    liquidated.validate_with_market(&market.as_view()).unwrap();
    let value_before = (
        market.header.vault,
        market.header.c_tot,
        market.header.insurance,
    );
    let asset_before = market.markets[0].engine.asset;

    let result = market.execute_trade_with_fee_loss_stale_scoped_not_atomic(
        &mut liquidated,
        &mut survivor,
        TradeRequestV16 {
            asset_index: 0,
            size_q: signed_q(CROSS_Q),
            exec_price: 1,
            fee_bps: 0,
        },
    );

    assert_eq!(result, Err(V16Error::LockActive));
    assert_eq!(market.markets[0].engine.asset, asset_before);
    assert_eq!(
        (
            market.header.vault,
            market.header.c_tot,
            market.header.insurance
        ),
        value_before
    );
}

#[test]
fn v16_exact_oi_cross_starts_reset_for_adl_basis_residue() {
    const SURVIVOR_Q: u128 = 13 * POS_SCALE;
    const MATCHED_Q: u128 = 10 * POS_SCALE;

    let (mut header, mut markets) = market_fixture(1, 1);
    let mut survivor_header = account_fixture(1, 20);
    let mut liquidated_header = account_fixture(1, 21);
    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        let mut survivor = PortfolioV16ViewMut::new(&mut survivor_header);
        let mut liquidated = PortfolioV16ViewMut::new(&mut liquidated_header);
        market.deposit_not_atomic(&mut survivor, 100).unwrap();
        market.deposit_not_atomic(&mut liquidated, 100).unwrap();
    }

    let mut asset = markets[0].engine.asset.try_to_runtime().unwrap();
    asset.a_long = ADL_ONE * MATCHED_Q / SURVIVOR_Q;
    asset.oi_eff_long_q = MATCHED_Q;
    asset.oi_eff_short_q = MATCHED_Q;
    asset.stored_pos_count_long = 1;
    asset.stored_pos_count_short = 1;
    asset.loss_weight_sum_long = SURVIVOR_Q;
    asset.loss_weight_sum_short = MATCHED_Q;
    markets[0].engine.asset = AssetStateV16Account::from_runtime(&asset);
    header.resolved_payout_blocker_count = V16PodU64::new(2);

    survivor_header.legs[0] = PortfolioLegV16Account::from_runtime(&PortfolioLegV16 {
        active: true,
        asset_index: 0,
        market_id: asset.market_id,
        side: SideV16::Long,
        basis_pos_q: signed_q(SURVIVOR_Q),
        a_basis: ADL_ONE,
        k_snap: asset.k_long,
        f_snap: asset.f_long_num,
        epoch_snap: asset.epoch_long,
        loss_weight: SURVIVOR_Q,
        b_snap: asset.b_long_num,
        b_rem: 0,
        b_epoch_snap: asset.epoch_long,
        b_stale: false,
        stale: false,
    });
    survivor_header.active_bitmap[0] = V16PodU64::new(1);
    survivor_header.health_cert.valid = 0;
    liquidated_header.legs[0] = PortfolioLegV16Account::from_runtime(&PortfolioLegV16 {
        active: true,
        asset_index: 0,
        market_id: asset.market_id,
        side: SideV16::Short,
        basis_pos_q: -signed_q(MATCHED_Q),
        a_basis: ADL_ONE,
        k_snap: asset.k_short,
        f_snap: asset.f_short_num,
        epoch_snap: asset.epoch_short,
        loss_weight: MATCHED_Q,
        b_snap: asset.b_short_num,
        b_rem: 0,
        b_epoch_snap: asset.epoch_short,
        b_stale: false,
        stale: false,
    });
    liquidated_header.active_bitmap[0] = V16PodU64::new(1);
    liquidated_header.health_cert.valid = 0;

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut survivor = PortfolioV16ViewMut::new(&mut survivor_header);
    let mut liquidated = PortfolioV16ViewMut::new(&mut liquidated_header);
    market.validate_shape().unwrap();
    survivor.validate_with_market(&market.as_view()).unwrap();
    liquidated.validate_with_market(&market.as_view()).unwrap();

    market
        .execute_trade_with_fee_loss_stale_scoped_not_atomic(
            &mut liquidated,
            &mut survivor,
            TradeRequestV16 {
                asset_index: 0,
                size_q: signed_q(MATCHED_Q),
                exec_price: 1,
                fee_bps: 0,
            },
        )
        .unwrap();

    let after = market.markets[0].engine.asset.try_to_runtime().unwrap();
    let survivor_leg = survivor.header.legs[0].try_to_runtime().unwrap();
    assert_eq!(after.oi_eff_long_q, 0);
    assert_eq!(after.oi_eff_short_q, 0);
    assert_eq!(survivor_leg.basis_pos_q, signed_q(SURVIVOR_Q - MATCHED_Q));
    assert!(survivor_leg.active);
    assert_eq!(after.mode_long, SideModeV16::ResetPending);
    assert_eq!(after.loss_weight_sum_long, 0);
    market.validate_shape().unwrap();
    survivor.validate_with_market(&market.as_view()).unwrap();
    liquidated.validate_with_market(&market.as_view()).unwrap();
}

#[test]
fn v16_exact_oi_unilateral_reduce_starts_reset_for_adl_basis_residue() {
    const SURVIVOR_Q: u128 = 13 * POS_SCALE;
    const MATCHED_Q: u128 = 10 * POS_SCALE;

    let (mut header, mut markets) = market_fixture(1, 1);
    let mut survivor_header = account_fixture(1, 22);
    let mut counterparty_header = account_fixture(1, 23);
    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        let mut survivor = PortfolioV16ViewMut::new(&mut survivor_header);
        let mut counterparty = PortfolioV16ViewMut::new(&mut counterparty_header);
        market.deposit_not_atomic(&mut survivor, 100).unwrap();
        market.deposit_not_atomic(&mut counterparty, 100).unwrap();
    }

    let mut asset = markets[0].engine.asset.try_to_runtime().unwrap();
    asset.a_long = ADL_ONE * MATCHED_Q / SURVIVOR_Q;
    asset.oi_eff_long_q = MATCHED_Q;
    asset.oi_eff_short_q = MATCHED_Q;
    asset.stored_pos_count_long = 1;
    asset.stored_pos_count_short = 1;
    asset.loss_weight_sum_long = SURVIVOR_Q;
    asset.loss_weight_sum_short = MATCHED_Q;
    markets[0].engine.asset = AssetStateV16Account::from_runtime(&asset);
    header.resolved_payout_blocker_count = V16PodU64::new(2);

    survivor_header.legs[0] = PortfolioLegV16Account::from_runtime(&PortfolioLegV16 {
        active: true,
        asset_index: 0,
        market_id: asset.market_id,
        side: SideV16::Long,
        basis_pos_q: signed_q(SURVIVOR_Q),
        a_basis: ADL_ONE,
        k_snap: asset.k_long,
        f_snap: asset.f_long_num,
        epoch_snap: asset.epoch_long,
        loss_weight: SURVIVOR_Q,
        b_snap: asset.b_long_num,
        b_rem: 0,
        b_epoch_snap: asset.epoch_long,
        b_stale: false,
        stale: false,
    });
    survivor_header.active_bitmap[0] = V16PodU64::new(1);
    survivor_header.health_cert.valid = 0;
    counterparty_header.legs[0] = PortfolioLegV16Account::from_runtime(&PortfolioLegV16 {
        active: true,
        asset_index: 0,
        market_id: asset.market_id,
        side: SideV16::Short,
        basis_pos_q: -signed_q(MATCHED_Q),
        a_basis: ADL_ONE,
        k_snap: asset.k_short,
        f_snap: asset.f_short_num,
        epoch_snap: asset.epoch_short,
        loss_weight: MATCHED_Q,
        b_snap: asset.b_short_num,
        b_rem: 0,
        b_epoch_snap: asset.epoch_short,
        b_stale: false,
        stale: false,
    });
    counterparty_header.active_bitmap[0] = V16PodU64::new(1);
    counterparty_header.health_cert.valid = 0;

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut survivor = PortfolioV16ViewMut::new(&mut survivor_header);
    let counterparty = PortfolioV16ViewMut::new(&mut counterparty_header);
    market.validate_shape().unwrap();
    survivor.validate_with_market(&market.as_view()).unwrap();
    counterparty
        .validate_with_market(&market.as_view())
        .unwrap();

    market
        .permissionless_auto_crank_not_atomic(
            &mut survivor,
            AutoCrankWorkV16 {
                now_slot: 1,
                observations: &[AutoCrankObservationV16 {
                    asset_index: 0,
                    effective_price: 1,
                    funding_rate_e9: 0,
                }],
                resolved_close_fee_rate_per_slot: 0,
            },
        )
        .unwrap();
    market
        .rebalance_reduce_position_not_atomic(
            &mut survivor,
            RebalanceRequestV16 {
                asset_index: 0,
                reduce_q: MATCHED_Q,
            },
        )
        .unwrap();

    let after = market.markets[0].engine.asset.try_to_runtime().unwrap();
    let survivor_leg = survivor.header.legs[0].try_to_runtime().unwrap();
    assert_eq!(after.oi_eff_long_q, 0);
    assert_eq!(after.oi_eff_short_q, 0);
    assert_eq!(survivor_leg.basis_pos_q, signed_q(SURVIVOR_Q - MATCHED_Q));
    assert!(survivor_leg.active);
    assert_eq!(after.mode_long, SideModeV16::ResetPending);
    assert_eq!(after.mode_short, SideModeV16::ResetPending);
    market
        .permissionless_auto_crank_not_atomic(
            &mut survivor,
            AutoCrankWorkV16 {
                now_slot: 1,
                observations: &[],
                resolved_close_fee_rate_per_slot: 0,
            },
        )
        .unwrap();
    assert!(!survivor.header.legs[0].try_to_runtime().unwrap().active);
    market.validate_shape().unwrap();
    survivor.validate_with_market(&market.as_view()).unwrap();
    counterparty
        .validate_with_market(&market.as_view())
        .unwrap();
}

#[test]
fn v16_batch_trade_applies_multiple_fills_after_inline_refresh() {
    let (mut header, mut markets) = market_fixture(2, 100);
    let mut long_header = account_fixture(2, 201);
    let mut short_header = account_fixture(2, 202);
    let requests = [
        TradeRequestV16 {
            asset_index: 0,
            size_q: signed_q(POS_SCALE),
            exec_price: 100,
            fee_bps: 0,
        },
        TradeRequestV16 {
            asset_index: 1,
            size_q: signed_q(2 * POS_SCALE),
            exec_price: 100,
            fee_bps: 0,
        },
    ];

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut long = PortfolioV16ViewMut::new(&mut long_header);
    let mut short = PortfolioV16ViewMut::new(&mut short_header);
    market.deposit_not_atomic(&mut long, 1_000).unwrap();
    market.deposit_not_atomic(&mut short, 1_000).unwrap();

    let outcome = market
        .execute_batch_with_fee_loss_stale_scoped_not_atomic(&mut long, &mut short, &requests)
        .unwrap();

    assert_eq!(outcome.fill_count, 2);
    assert_eq!(outcome.notional, 300);
    assert_eq!(outcome.fee_a, 0);
    assert_eq!(outcome.fee_b, 0);
    assert_ne!(long.header.active_bitmap[0].get(), 0);
    assert_ne!(short.header.active_bitmap[0].get(), 0);
    assert_eq!(
        market.markets[0].engine.asset.oi_eff_long_q.get(),
        POS_SCALE
    );
    assert_eq!(
        market.markets[0].engine.asset.oi_eff_short_q.get(),
        POS_SCALE
    );
    assert_eq!(
        market.markets[1].engine.asset.oi_eff_long_q.get(),
        2 * POS_SCALE
    );
    assert_eq!(
        market.markets[1].engine.asset.oi_eff_short_q.get(),
        2 * POS_SCALE
    );
    market.validate_shape().unwrap();
    long.validate_with_market(&market.as_view()).unwrap();
    short.validate_with_market(&market.as_view()).unwrap();
}

#[test]
fn v16_batch_trade_supports_mixed_signed_spread_legs() {
    let (mut header, mut markets) = market_fixture(2, 100);
    let mut taker_header = account_fixture(2, 221);
    let mut lp_header = account_fixture(2, 222);
    let size_q = signed_q(5 * POS_SCALE);
    let requests = [
        TradeRequestV16 {
            asset_index: 0,
            size_q,
            exec_price: 100,
            fee_bps: 0,
        },
        TradeRequestV16 {
            asset_index: 1,
            size_q: -size_q,
            exec_price: 100,
            fee_bps: 0,
        },
    ];

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut taker = PortfolioV16ViewMut::new(&mut taker_header);
    let mut lp = PortfolioV16ViewMut::new(&mut lp_header);
    market.deposit_not_atomic(&mut taker, 1_000).unwrap();
    market.deposit_not_atomic(&mut lp, 1_000).unwrap();

    let outcome = market
        .execute_batch_with_fee_loss_stale_scoped_not_atomic(&mut taker, &mut lp, &requests)
        .unwrap();

    assert_eq!(outcome.fill_count, 2);
    assert_eq!(outcome.notional, 1_000);
    assert_eq!(
        market.markets[0].engine.asset.oi_eff_long_q.get(),
        5 * POS_SCALE
    );
    assert_eq!(
        market.markets[0].engine.asset.oi_eff_short_q.get(),
        5 * POS_SCALE
    );
    assert_eq!(
        market.markets[1].engine.asset.oi_eff_long_q.get(),
        5 * POS_SCALE
    );
    assert_eq!(
        market.markets[1].engine.asset.oi_eff_short_q.get(),
        5 * POS_SCALE
    );

    let taker_asset0 = taker.header.legs[0].try_to_runtime().unwrap();
    let taker_asset1 = taker.header.legs[1].try_to_runtime().unwrap();
    let lp_asset0 = lp.header.legs[0].try_to_runtime().unwrap();
    let lp_asset1 = lp.header.legs[1].try_to_runtime().unwrap();
    assert_eq!(taker_asset0.side, SideV16::Long);
    assert_eq!(taker_asset1.side, SideV16::Short);
    assert_eq!(lp_asset0.side, SideV16::Short);
    assert_eq!(lp_asset1.side, SideV16::Long);
    assert_eq!(taker_asset0.basis_pos_q, size_q);
    assert_eq!(taker_asset1.basis_pos_q, -size_q);
    assert_eq!(lp_asset0.basis_pos_q, -size_q);
    assert_eq!(lp_asset1.basis_pos_q, size_q);
    market.validate_shape().unwrap();
    taker.validate_with_market(&market.as_view()).unwrap();
    lp.validate_with_market(&market.as_view()).unwrap();
}

#[test]
fn v16_single_trade_matches_batch_of_one_state() {
    let (mut single_header, mut single_markets) = market_fixture(1, 100);
    let mut single_long_header = account_fixture(1, 209);
    let mut single_short_header = account_fixture(1, 210);
    let mut batch_header = single_header;
    let mut batch_markets = single_markets.clone();
    let mut batch_long_header = single_long_header;
    let mut batch_short_header = single_short_header;
    let request = TradeRequestV16 {
        asset_index: 0,
        size_q: signed_q(2 * POS_SCALE),
        exec_price: 100,
        fee_bps: 0,
    };

    let single_outcome = {
        let mut market = MarketGroupV16ViewMut::new(&mut single_header, &mut single_markets);
        let mut long = PortfolioV16ViewMut::new(&mut single_long_header);
        let mut short = PortfolioV16ViewMut::new(&mut single_short_header);
        market.deposit_not_atomic(&mut long, 1_000).unwrap();
        market.deposit_not_atomic(&mut short, 1_000).unwrap();
        market
            .execute_trade_with_fee_loss_stale_scoped_not_atomic(&mut long, &mut short, request)
            .unwrap()
    };
    let batch_outcome = {
        let mut market = MarketGroupV16ViewMut::new(&mut batch_header, &mut batch_markets);
        let mut long = PortfolioV16ViewMut::new(&mut batch_long_header);
        let mut short = PortfolioV16ViewMut::new(&mut batch_short_header);
        market.deposit_not_atomic(&mut long, 1_000).unwrap();
        market.deposit_not_atomic(&mut short, 1_000).unwrap();
        market
            .execute_batch_with_fee_loss_stale_scoped_not_atomic(&mut long, &mut short, &[request])
            .unwrap()
    };

    assert_eq!(batch_outcome.fill_count, 1);
    assert_eq!(single_outcome.fee_a, batch_outcome.fee_a);
    assert_eq!(single_outcome.fee_b, batch_outcome.fee_b);
    assert_eq!(single_outcome.notional, batch_outcome.notional);
    assert_eq!(single_header, batch_header);
    assert_eq!(single_markets, batch_markets);
    assert_eq!(single_long_header, batch_long_header);
    assert_eq!(single_short_header, batch_short_header);
}

#[test]
fn v16_subatom_trade_charges_fee_on_ceil_fee_notional() {
    let (mut header, mut markets) = market_fixture(1, 100);
    header.config.max_trading_fee_bps = V16PodU64::new(1);
    let mut long_header = account_fixture(1, 213);
    let mut short_header = account_fixture(1, 214);
    let sub_atom_size = POS_SCALE / 100 - 1;
    let request = TradeRequestV16 {
        asset_index: 0,
        size_q: signed_q(sub_atom_size),
        exec_price: 100,
        fee_bps: 1,
    };

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut long = PortfolioV16ViewMut::new(&mut long_header);
    let mut short = PortfolioV16ViewMut::new(&mut short_header);
    market.deposit_not_atomic(&mut long, 1_000).unwrap();
    market.deposit_not_atomic(&mut short, 1_000).unwrap();

    let outcome = market
        .execute_trade_with_fee_loss_stale_scoped_not_atomic(&mut long, &mut short, request)
        .unwrap();

    assert_eq!(outcome.notional, 0);
    assert_eq!(outcome.fee_a, 1);
    assert_eq!(outcome.fee_b, 1);
    assert_eq!(market.header.insurance.get(), 2);
    assert_eq!(
        market.markets[0].engine.asset.oi_eff_long_q.get(),
        sub_atom_size
    );
    assert_eq!(
        market.markets[0].engine.asset.oi_eff_short_q.get(),
        sub_atom_size
    );
    market.validate_shape().unwrap();
    long.validate_with_market(&market.as_view()).unwrap();
    short.validate_with_market(&market.as_view()).unwrap();
}

#[test]
fn v16_batch_trade_checks_initial_margin_on_final_portfolio() {
    let (mut header, mut markets) = market_fixture(2, 100);
    let mut taker_header = account_fixture(2, 211);
    let mut lp_header = account_fixture(2, 212);
    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        let mut taker = PortfolioV16ViewMut::new(&mut taker_header);
        let mut lp = PortfolioV16ViewMut::new(&mut lp_header);
        market.deposit_not_atomic(&mut taker, 1_000).unwrap();
        market.deposit_not_atomic(&mut lp, 1_000).unwrap();
        market
            .execute_trade_with_fee_loss_stale_scoped_not_atomic(
                &mut lp,
                &mut taker,
                TradeRequestV16 {
                    asset_index: 0,
                    size_q: signed_q(10 * POS_SCALE),
                    exec_price: 100,
                    fee_bps: 0,
                },
            )
            .unwrap();
    }

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut taker = PortfolioV16ViewMut::new(&mut taker_header);
    let mut lp = PortfolioV16ViewMut::new(&mut lp_header);
    let outcome = market
        .execute_batch_with_fee_loss_stale_scoped_not_atomic(
            &mut taker,
            &mut lp,
            &[
                TradeRequestV16 {
                    asset_index: 1,
                    size_q: signed_q(10 * POS_SCALE),
                    exec_price: 100,
                    fee_bps: 0,
                },
                TradeRequestV16 {
                    asset_index: 0,
                    size_q: signed_q(10 * POS_SCALE),
                    exec_price: 100,
                    fee_bps: 0,
                },
            ],
        )
        .expect("batch must not reject a final-IM-valid basket due to interim IM");

    assert_eq!(outcome.fill_count, 2);
    assert_eq!(outcome.notional, 2_000);
    assert_eq!(
        market.markets[0].engine.asset.oi_eff_long_q.get(),
        0,
        "second fill closes the original asset-0 exposure"
    );
    assert_eq!(
        market.markets[1].engine.asset.oi_eff_long_q.get(),
        10 * POS_SCALE,
        "final portfolio keeps only the replacement asset-1 exposure"
    );
    assert_eq!(
        taker
            .header
            .health_cert
            .try_to_runtime()
            .unwrap()
            .certified_initial_req,
        1_000
    );
    assert_eq!(
        lp.header
            .health_cert
            .try_to_runtime()
            .unwrap()
            .certified_initial_req,
        1_000
    );
    market.validate_shape().unwrap();
    taker.validate_with_market(&market.as_view()).unwrap();
    lp.validate_with_market(&market.as_view()).unwrap();
}

#[test]
fn v16_batch_trade_self_settles_stale_certificates_once_before_fills() {
    let (mut header, mut markets) = market_fixture(1, 100);
    let mut long_header = account_fixture(1, 203);
    let mut short_header = account_fixture(1, 204);
    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        let mut long = PortfolioV16ViewMut::new(&mut long_header);
        let mut short = PortfolioV16ViewMut::new(&mut short_header);
        market.deposit_not_atomic(&mut long, 1_000).unwrap();
        market.deposit_not_atomic(&mut short, 1_000).unwrap();
        market
            .execute_trade_with_fee_loss_stale_scoped_not_atomic(
                &mut long,
                &mut short,
                TradeRequestV16 {
                    asset_index: 0,
                    size_q: signed_q(POS_SCALE),
                    exec_price: 100,
                    fee_bps: 0,
                },
            )
            .unwrap();
        market
            .accrue_asset_to_not_atomic(0, 2, 101, 0, true)
            .unwrap();
        market.markets[0].engine.asset.raw_oracle_target_price = V16PodU64::new(101);
    }
    assert_eq!(long_header.pnl.get(), 0);

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut long = PortfolioV16ViewMut::new(&mut long_header);
    let mut short = PortfolioV16ViewMut::new(&mut short_header);
    let outcome = market
        .execute_batch_with_fee_loss_stale_scoped_not_atomic(
            &mut long,
            &mut short,
            &[TradeRequestV16 {
                asset_index: 0,
                size_q: signed_q(POS_SCALE),
                exec_price: 101,
                fee_bps: 0,
            }],
        )
        .unwrap();

    assert_eq!(outcome.fill_count, 1);
    assert_eq!(outcome.notional, 101);
    assert!(long.header.pnl.get() > 0);
    market.validate_shape().unwrap();
    long.validate_with_market(&market.as_view()).unwrap();
    short.validate_with_market(&market.as_view()).unwrap();
}

#[test]
fn v16_batch_trade_rejects_loss_stale_risk_increase_after_inline_settlement() {
    let (mut header, mut markets) = market_fixture(1, 100);
    let mut long_header = account_fixture(1, 207);
    let mut short_header = account_fixture(1, 208);
    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        let mut long = PortfolioV16ViewMut::new(&mut long_header);
        let mut short = PortfolioV16ViewMut::new(&mut short_header);
        market.deposit_not_atomic(&mut long, 1_000).unwrap();
        market.deposit_not_atomic(&mut short, 1_000).unwrap();
        market
            .execute_trade_with_fee_loss_stale_scoped_not_atomic(
                &mut long,
                &mut short,
                TradeRequestV16 {
                    asset_index: 0,
                    size_q: signed_q(POS_SCALE),
                    exec_price: 100,
                    fee_bps: 0,
                },
            )
            .unwrap();
        market
            .accrue_asset_to_not_atomic(0, 3, 101, 0, true)
            .unwrap();
        market.markets[0].engine.asset.raw_oracle_target_price = V16PodU64::new(101);
    }

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut long = PortfolioV16ViewMut::new(&mut long_header);
    let mut short = PortfolioV16ViewMut::new(&mut short_header);
    let res = market.execute_batch_with_fee_loss_stale_scoped_not_atomic(
        &mut long,
        &mut short,
        &[TradeRequestV16 {
            asset_index: 0,
            size_q: signed_q(POS_SCALE),
            exec_price: 101,
            fee_bps: 0,
        }],
    );

    assert_eq!(res, Err(V16Error::LockActive));
}

#[test]
fn v16_public_scoped_trade_preserves_unrelated_loss_stale_summary() {
    let (mut header, mut markets) = market_fixture(2, 100);
    let mut long_header = account_fixture(2, 209);
    let mut short_header = account_fixture(2, 210);
    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        let mut long = PortfolioV16ViewMut::new(&mut long_header);
        let mut short = PortfolioV16ViewMut::new(&mut short_header);
        market.deposit_not_atomic(&mut long, 1_000).unwrap();
        market.deposit_not_atomic(&mut short, 1_000).unwrap();
    }
    header.current_slot = V16PodU64::new(10);
    header.slot_last = V16PodU64::new(9);
    header.loss_stale_active = 1;
    let mut current_asset = markets[0].engine.asset.try_to_runtime().unwrap();
    current_asset.slot_last = 10;
    markets[0].engine.asset = AssetStateV16Account::from_runtime(&current_asset);

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut long = PortfolioV16ViewMut::new(&mut long_header);
    let mut short = PortfolioV16ViewMut::new(&mut short_header);
    let outcome = market
        .execute_trade_with_fee_loss_stale_scoped_not_atomic(
            &mut long,
            &mut short,
            TradeRequestV16 {
                asset_index: 0,
                size_q: signed_q(POS_SCALE),
                exec_price: 100,
                fee_bps: 0,
            },
        )
        .expect("unrelated loss-stale summary must not block a locally current trade");

    assert_eq!(outcome.notional, 100);
    assert_eq!(market.header.loss_stale_active, 1);
    assert_eq!(market.markets[0].engine.asset.slot_last.get(), 10);
    market.validate_shape().unwrap();
    long.validate_with_market(&market.as_view()).unwrap();
    short.validate_with_market(&market.as_view()).unwrap();
}

#[test]
fn v16_batch_trade_is_bounded_by_configured_portfolio_asset_cap() {
    let (mut header, mut markets) = market_fixture(1, 100);
    let mut long_header = account_fixture(1, 205);
    let mut short_header = account_fixture(1, 206);
    let requests = [
        TradeRequestV16 {
            asset_index: 0,
            size_q: signed_q(POS_SCALE),
            exec_price: 100,
            fee_bps: 0,
        },
        TradeRequestV16 {
            asset_index: 0,
            size_q: signed_q(POS_SCALE),
            exec_price: 100,
            fee_bps: 0,
        },
    ];
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut long = PortfolioV16ViewMut::new(&mut long_header);
    let mut short = PortfolioV16ViewMut::new(&mut short_header);
    market.deposit_not_atomic(&mut long, 1_000).unwrap();
    market.deposit_not_atomic(&mut short, 1_000).unwrap();

    let res = market
        .execute_batch_with_fee_loss_stale_scoped_not_atomic(&mut long, &mut short, &requests);

    assert_eq!(res, Err(V16Error::InvalidConfig));
}

#[test]
fn v16_view_dynamic_market_slots_can_be_activated_without_runtime_vec_engine() {
    let (mut header, mut markets) = market_fixture(3, 100);
    let view = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    view.validate_shape().unwrap();

    assert_eq!(
        view.header
            .config
            .try_to_runtime()
            .unwrap()
            .max_market_slots,
        3
    );
    assert_eq!(view.markets.len(), 3);
    assert_eq!(view.markets[2].engine.asset.market_id.get(), 3);
    assert_eq!(view.markets[2].engine.asset.effective_price.get(), 100);
}

#[test]
fn v16_public_raw_oracle_target_update_is_value_neutral_and_lifecycle_gated() {
    let (mut header, mut markets) = market_fixture(1, 100);
    let vault_before = header.vault.get();
    let c_tot_before = header.c_tot.get();
    let insurance_before = header.insurance.get();
    let oracle_epoch_before = header.oracle_epoch.get();

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    market
        .set_asset_raw_oracle_target_not_atomic(0, 111)
        .unwrap();
    let asset = market.markets[0].engine.asset.try_to_runtime().unwrap();

    assert_eq!(asset.raw_oracle_target_price, 111);
    assert_eq!(asset.effective_price, 100);
    assert_eq!(market.header.oracle_epoch.get(), oracle_epoch_before + 1);
    assert_eq!(market.header.vault.get(), vault_before);
    assert_eq!(market.header.c_tot.get(), c_tot_before);
    assert_eq!(market.header.insurance.get(), insurance_before);
    market
        .set_asset_raw_oracle_target_not_atomic(0, 111)
        .unwrap();
    assert_eq!(market.header.oracle_epoch.get(), oracle_epoch_before + 1);
    market.validate_shape().unwrap();
}

#[test]
fn v16_public_empty_asset_oracle_anchor_reset_rejects_any_group_position_state() {
    let (mut header, mut markets) = market_fixture(2, 100);
    let mut other_asset = markets[1].engine.asset.try_to_runtime().unwrap();
    other_asset.oi_eff_long_q = POS_SCALE;
    other_asset.oi_eff_short_q = POS_SCALE;
    other_asset.stored_pos_count_long = 1;
    other_asset.stored_pos_count_short = 1;
    other_asset.loss_weight_sum_long = POS_SCALE;
    other_asset.loss_weight_sum_short = POS_SCALE;
    markets[1].engine.asset = AssetStateV16Account::from_runtime(&other_asset);

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let res = market.reset_empty_asset_oracle_anchor_not_atomic(0, 123, 10);

    assert_eq!(res, Err(V16Error::LockActive));
    assert_eq!(market.markets[0].engine.asset.effective_price.get(), 100);
}

#[test]
fn v16_public_empty_asset_oracle_anchor_reset_is_value_neutral() {
    let (mut header, mut markets) = market_fixture(2, 100);
    let vault_before = header.vault.get();
    let c_tot_before = header.c_tot.get();
    let insurance_before = header.insurance.get();

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    market
        .reset_empty_asset_oracle_anchor_not_atomic(0, 123, 10)
        .unwrap();
    let asset = market.markets[0].engine.asset.try_to_runtime().unwrap();

    assert_eq!(asset.raw_oracle_target_price, 123);
    assert_eq!(asset.effective_price, 123);
    assert_eq!(asset.fund_px_last, 123);
    assert_eq!(asset.slot_last, 10);
    assert_eq!(market.header.current_slot.get(), 10);
    assert_eq!(market.header.slot_last.get(), 10);
    assert_eq!(market.header.vault.get(), vault_before);
    assert_eq!(market.header.c_tot.get(), c_tot_before);
    assert_eq!(market.header.insurance.get(), insurance_before);
    market.validate_shape().unwrap();
}

#[test]
fn v16_public_force_asset_recovery_freezes_mark_and_is_idempotent() {
    let (mut header, mut markets) = market_fixture(2, 100);
    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        market
            .set_asset_raw_oracle_target_not_atomic(1, 150)
            .unwrap();
    }
    let asset_epoch_before = header.asset_set_epoch.get();
    let risk_epoch_before = header.risk_epoch.get();
    let vault_before = header.vault.get();
    let c_tot_before = header.c_tot.get();
    let insurance_before = header.insurance.get();

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    market.force_asset_recovery_not_atomic(1, 2).unwrap();
    let asset = market.markets[1].engine.asset.try_to_runtime().unwrap();

    assert_eq!(asset.lifecycle, AssetLifecycleV16::Recovery);
    assert_eq!(asset.raw_oracle_target_price, asset.effective_price);
    assert_eq!(market.header.asset_set_epoch.get(), asset_epoch_before + 1);
    assert_eq!(market.header.risk_epoch.get(), risk_epoch_before + 1);
    assert_eq!(market.header.vault.get(), vault_before);
    assert_eq!(market.header.c_tot.get(), c_tot_before);
    assert_eq!(market.header.insurance.get(), insurance_before);

    market.force_asset_recovery_not_atomic(1, 2).unwrap();
    assert_eq!(market.header.asset_set_epoch.get(), asset_epoch_before + 1);
    assert_eq!(market.header.risk_epoch.get(), risk_epoch_before + 1);
    market.validate_shape().unwrap();
}

#[test]
fn v16_restart_empty_asset_preserves_domain_budget_for_nonzero_asset() {
    let (mut header, mut markets) = market_fixture(2, 100);
    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        market.deposit_domain_insurance_not_atomic(2, 10).unwrap();
        market.force_asset_recovery_not_atomic(1, 2).unwrap();
    }
    let old_market_id = markets[1].engine.asset.market_id.get();
    let budget_before = markets[1].engine.insurance_domain_budget_long.get();
    let budget_total_before = header.insurance_domain_budget_remaining_total.get();
    let vault_before = header.vault.get();
    let c_tot_before = header.c_tot.get();
    let insurance_before = header.insurance.get();

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    market
        .restart_empty_asset_preserving_insurance_budget_not_atomic(1, 222, 3)
        .unwrap();
    let asset = market.markets[1].engine.asset.try_to_runtime().unwrap();

    assert_eq!(asset.lifecycle, AssetLifecycleV16::Active);
    assert_ne!(asset.market_id, old_market_id);
    assert_eq!(asset.raw_oracle_target_price, 222);
    assert_eq!(
        market.markets[1].engine.insurance_domain_budget_long.get(),
        budget_before
    );
    assert_eq!(
        market.header.insurance_domain_budget_remaining_total.get(),
        budget_total_before
    );
    assert_eq!(market.header.vault.get(), vault_before);
    assert_eq!(market.header.c_tot.get(), c_tot_before);
    assert_eq!(market.header.insurance.get(), insurance_before);
    market.validate_shape().unwrap();
}

#[test]
fn v16_terminal_spent_domain_budget_cleanup_unblocks_empty_asset_restart() {
    let (mut header, mut markets) = market_fixture(2, 100);
    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        market.deposit_domain_insurance_not_atomic(2, 10).unwrap();
        market.force_asset_recovery_not_atomic(1, 2).unwrap();
    }
    header.insurance = V16PodU128::new(0);
    header.insurance_domain_budget_remaining_total = V16PodU128::new(0);
    markets[1].engine.insurance_domain_spent_long = V16PodU128::new(10);
    let old_market_id = markets[1].engine.asset.market_id.get();
    let vault_before = header.vault.get();
    let c_tot_before = header.c_tot.get();
    let insurance_before = header.insurance.get();
    let remaining_before = header.insurance_domain_budget_remaining_total.get();

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    assert_eq!(market.validate_shape(), Ok(()));
    assert_eq!(
        market.restart_empty_asset_preserving_insurance_budget_not_atomic(1, 222, 3),
        Err(V16Error::LockActive)
    );

    market
        .clear_terminal_spent_domain_budgets_for_empty_asset_not_atomic(1)
        .unwrap();
    assert_eq!(market.header.vault.get(), vault_before);
    assert_eq!(market.header.c_tot.get(), c_tot_before);
    assert_eq!(market.header.insurance.get(), insurance_before);
    assert_eq!(
        market.header.insurance_domain_budget_remaining_total.get(),
        remaining_before
    );
    assert_eq!(
        market.markets[1].engine.insurance_domain_budget_long.get(),
        0
    );
    assert_eq!(
        market.markets[1].engine.insurance_domain_spent_long.get(),
        0
    );

    market
        .restart_empty_asset_preserving_insurance_budget_not_atomic(1, 222, 3)
        .unwrap();
    let asset = market.markets[1].engine.asset.try_to_runtime().unwrap();
    assert_eq!(asset.lifecycle, AssetLifecycleV16::Active);
    assert_ne!(asset.market_id, old_market_id);
    assert_eq!(asset.effective_price, 222);
    market.validate_shape().unwrap();
}

#[test]
fn v16_terminal_spent_domain_budget_cleanup_rejects_remaining_budget() {
    let (mut header, mut markets) = market_fixture(2, 100);
    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        market.deposit_domain_insurance_not_atomic(2, 10).unwrap();
        market.force_asset_recovery_not_atomic(1, 2).unwrap();
    }
    header.insurance = V16PodU128::new(6);
    header.insurance_domain_budget_remaining_total = V16PodU128::new(6);
    markets[1].engine.insurance_domain_spent_long = V16PodU128::new(4);
    let vault_before = header.vault;
    let insurance_before = header.insurance;
    let remaining_before = header.insurance_domain_budget_remaining_total;
    let budget_before = markets[1].engine.insurance_domain_budget_long;
    let spent_before = markets[1].engine.insurance_domain_spent_long;

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    assert_eq!(market.validate_shape(), Ok(()));
    assert_eq!(
        market.clear_terminal_spent_domain_budgets_for_empty_asset_not_atomic(1),
        Err(V16Error::LockActive)
    );
    assert_eq!(market.header.vault, vault_before);
    assert_eq!(market.header.insurance, insurance_before);
    assert_eq!(
        market.header.insurance_domain_budget_remaining_total,
        remaining_before
    );
    assert_eq!(
        market.markets[1].engine.insurance_domain_budget_long,
        budget_before
    );
    assert_eq!(
        market.markets[1].engine.insurance_domain_spent_long,
        spent_before
    );
}

#[test]
fn v16_terminal_spent_domain_budget_cleanup_rejects_active_asset() {
    let (mut header, mut markets) = market_fixture(2, 100);
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    assert_eq!(market.validate_shape(), Ok(()));
    assert_eq!(
        market.clear_terminal_spent_domain_budgets_for_empty_asset_not_atomic(1),
        Err(V16Error::LockActive)
    );
}

#[test]
fn v16_canonicalize_retired_empty_asset_slot_clears_inert_domain_state() {
    let (mut header, mut markets) = market_fixture(2, 100);
    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        market.retire_empty_asset_not_atomic(1, 3).unwrap();
    }
    let old_market_id = markets[1].engine.asset.market_id.get();
    let inert_empty_source = SourceCreditStateV16 {
        credit_epoch: 7,
        credit_rate_num: 0,
        ..SourceCreditStateV16::EMPTY
    };
    markets[1].engine.source_credit_long =
        SourceCreditStateV16Account::from_runtime(&inert_empty_source);
    markets[1].engine.source_credit_short =
        SourceCreditStateV16Account::from_runtime(&inert_empty_source);
    let vault_before = header.vault.get();
    let c_tot_before = header.c_tot.get();
    let insurance_before = header.insurance.get();

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    market
        .canonicalize_retired_empty_asset_slot_not_atomic(1)
        .unwrap();
    let asset = market.markets[1].engine.asset.try_to_runtime().unwrap();

    assert_eq!(asset.lifecycle, AssetLifecycleV16::Retired);
    assert_eq!(asset.market_id, old_market_id);
    assert_eq!(
        market.markets[1]
            .engine
            .source_credit_long
            .try_to_runtime()
            .unwrap(),
        SourceCreditStateV16::EMPTY
    );
    assert_eq!(market.header.vault.get(), vault_before);
    assert_eq!(market.header.c_tot.get(), c_tot_before);
    assert_eq!(market.header.insurance.get(), insurance_before);
    market.validate_shape().unwrap();
}

#[test]
fn v16_reused_market_slot_rejects_old_market_id_leg() {
    let (mut header, mut markets) = market_fixture(1, 100);
    let mut account_header = account_fixture(1, 16);
    let old_market_id = markets[0].engine.asset.market_id.get();
    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        market.retire_empty_asset_not_atomic(0, 1).unwrap();
    }
    header
        .activate_empty_asset_slot_not_atomic(0, &mut markets[0].engine, 200, 2)
        .unwrap();
    assert_ne!(markets[0].engine.asset.market_id.get(), old_market_id);

    account_header.legs[0] = PortfolioLegV16Account::from_runtime(&PortfolioLegV16 {
        active: true,
        asset_index: 0,
        market_id: old_market_id,
        side: SideV16::Long,
        basis_pos_q: POS_SCALE as i128,
        a_basis: ADL_ONE,
        k_snap: 0,
        f_snap: 0,
        epoch_snap: 0,
        loss_weight: POS_SCALE,
        b_snap: 0,
        b_rem: 0,
        b_epoch_snap: 0,
        b_stale: false,
        stale: false,
    });
    account_header.active_bitmap[0] = V16PodU64::new(1);

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    assert_eq!(
        market.full_account_refresh_not_atomic(&mut account),
        Err(V16Error::HiddenLeg),
        "stale legs from a retired market slot must not bind to the reactivated market"
    );
    market.validate_shape().unwrap();
}

#[test]
fn v16_retire_and_reactivate_empty_asset_after_source_credit_epoch_bump() {
    let (mut header, mut markets) = market_fixture(1, 100);
    let old_market_id = markets[0].engine.asset.market_id.get();
    let recomputed_empty_source = SourceCreditStateV16 {
        credit_epoch: 2,
        ..SourceCreditStateV16::EMPTY
    };
    markets[0].engine.source_credit_long =
        SourceCreditStateV16Account::from_runtime(&recomputed_empty_source);
    markets[0].engine.source_credit_short =
        SourceCreditStateV16Account::from_runtime(&recomputed_empty_source);

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    market.retire_empty_asset_not_atomic(0, 1).unwrap();
    assert_eq!(
        market.markets[0]
            .engine
            .asset
            .try_to_runtime()
            .unwrap()
            .lifecycle,
        AssetLifecycleV16::Retired
    );

    market
        .header
        .activate_empty_market_slot_not_atomic(0, &mut market.markets[0], 200, 2)
        .unwrap();
    assert_ne!(
        market.markets[0].engine.asset.market_id.get(),
        old_market_id
    );
    assert_eq!(
        market.markets[0]
            .engine
            .source_credit_long
            .try_to_runtime()
            .unwrap(),
        SourceCreditStateV16::EMPTY
    );
    assert_eq!(
        market.markets[0]
            .engine
            .source_credit_short
            .try_to_runtime()
            .unwrap(),
        SourceCreditStateV16::EMPTY
    );
    market.validate_shape().unwrap();
}

#[test]
fn v16_view_rejects_overwithdraw() {
    let (mut header, mut markets) = market_fixture(1, 100);
    let mut account_header = account_fixture(1, 6);
    let mut market_view = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account_view = PortfolioV16ViewMut::new(&mut account_header);
    market_view
        .deposit_not_atomic(&mut account_view, 3)
        .unwrap();

    let err = market_view.withdraw_not_atomic(&mut account_view, 4);

    assert_eq!(err, Err(V16Error::LockActive));
}

#[cfg(feature = "fuzz")]
#[test]
fn v16_insurance_lien_consume_rejects_fractional_bound_amount() {
    let (mut header, mut markets) = market_fixture(1, 100);
    header.vault = V16PodU128::new(10);
    header.insurance = V16PodU128::new(10);

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    market.deposit_domain_insurance_not_atomic(0, 10).unwrap();
    market
        .reserve_insurance_credit_not_atomic(0, BOUND_SCALE)
        .unwrap();
    market
        .create_source_credit_lien_from_insurance_not_atomic(0, BOUND_SCALE)
        .unwrap();

    let before_insurance = market.header.insurance;
    let before_spent = market.markets[0].engine.insurance_domain_spent_long;
    let before_reservation = market.markets[0].engine.insurance_reservation_long;
    let before_source = market.markets[0].engine.source_credit_long;

    let err = market.consume_source_credit_lien_from_insurance_not_atomic(0, 1);

    assert_eq!(err, Err(V16Error::InvalidConfig));
    assert_eq!(market.header.insurance, before_insurance);
    assert_eq!(
        market.markets[0].engine.insurance_domain_spent_long,
        before_spent
    );
    assert_eq!(
        market.markets[0].engine.insurance_reservation_long,
        before_reservation
    );
    assert_eq!(market.markets[0].engine.source_credit_long, before_source);
}

#[test]
fn v16_domain_insurance_deposit_and_withdraw_use_engine_budget_accounting() {
    let (mut header, mut markets) = market_fixture(1, 100);
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);

    market.deposit_domain_insurance_not_atomic(0, 10).unwrap();
    assert_eq!(market.header.vault.get(), 10);
    assert_eq!(market.header.insurance.get(), 10);
    assert_eq!(
        market.header.insurance_domain_budget_remaining_total.get(),
        10
    );
    assert_eq!(
        market.markets[0].engine.insurance_domain_budget_long.get(),
        10
    );

    market.withdraw_domain_insurance_not_atomic(0, 4).unwrap();
    assert_eq!(market.header.vault.get(), 6);
    assert_eq!(market.header.insurance.get(), 6);
    assert_eq!(
        market.header.insurance_domain_budget_remaining_total.get(),
        6
    );
    assert_eq!(
        market.markets[0].engine.insurance_domain_budget_long.get(),
        6
    );
    assert_eq!(market.validate_shape(), Ok(()));
}

#[test]
fn v16_credit_account_from_insurance_uses_unbudgeted_surplus_only() {
    let (mut header, mut markets) = market_fixture(1, 100);
    header.vault = V16PodU128::new(10);
    header.insurance = V16PodU128::new(10);
    let mut account_header = account_fixture(1, 9);
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);

    market
        .credit_account_from_insurance_not_atomic(&mut account, 3)
        .unwrap();
    assert_eq!(market.header.vault.get(), 10);
    assert_eq!(market.header.insurance.get(), 7);
    assert_eq!(market.header.c_tot.get(), 3);
    assert_eq!(account.header.capital.get(), 3);
    assert_eq!(market.validate_shape(), Ok(()));
    assert_eq!(account.validate_with_market(&market.as_view()), Ok(()));

    market
        .credit_domain_insurance_budget_not_atomic(0, 7)
        .unwrap();
    let err = market.credit_account_from_insurance_not_atomic(&mut account, 1);
    assert_eq!(
        err,
        Err(V16Error::LockActive),
        "budgeted domain insurance must not be paid as a cranker reward"
    );
}

#[test]
fn v16_public_domain_insurance_spent_setter_preserves_budget_total() {
    let (mut header, mut markets) = market_fixture(1, 100);
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);

    market.deposit_domain_insurance_not_atomic(0, 10).unwrap();
    market.set_domain_insurance_spent(0, 4).unwrap();
    assert_eq!(
        market.header.insurance_domain_budget_remaining_total.get(),
        6
    );
    assert_eq!(
        market.markets[0].engine.insurance_domain_spent_long.get(),
        4
    );
    market.set_domain_insurance_spent(0, 0).unwrap();
    assert_eq!(
        market.header.insurance_domain_budget_remaining_total.get(),
        10
    );
    assert_eq!(market.validate_shape(), Ok(()));
}

#[test]
fn v16_public_domain_insurance_spent_setter_rejects_unbacked_clear() {
    let (mut header, mut markets) = market_fixture(1, 100);
    header.vault = V16PodU128::new(5);
    header.insurance = V16PodU128::new(5);
    header.insurance_domain_budget_remaining_total = V16PodU128::new(5);
    markets[0].engine.insurance_domain_budget_long = V16PodU128::new(10);
    markets[0].engine.insurance_domain_spent_long = V16PodU128::new(5);
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    assert_eq!(market.validate_shape(), Ok(()));

    let err = market.set_domain_insurance_spent(0, 0);

    assert_eq!(err, Err(V16Error::LockActive));
    assert_eq!(
        market.header.insurance_domain_budget_remaining_total.get(),
        5
    );
    assert_eq!(
        market.markets[0].engine.insurance_domain_spent_long.get(),
        5
    );
}

#[test]
fn v16_backing_provider_earnings_credit_and_withdraw_are_engine_accounted() {
    let (mut header, mut markets) = market_fixture(1, 100);
    header.vault = V16PodU128::new(10);
    let market_id = markets[0].engine.asset.market_id.get();
    markets[0].engine.backing_long = BackingBucketV16Account::from_runtime(&BackingBucketV16 {
        market_id,
        fresh_unliened_backing_num: 1,
        expiry_slot: 10,
        status: BackingBucketStatusV16::Fresh,
        ..BackingBucketV16::EMPTY
    });
    markets[0].engine.source_credit_long =
        SourceCreditStateV16Account::from_runtime(&SourceCreditStateV16 {
            fresh_reserved_backing_num: 1,
            credit_rate_num: CREDIT_RATE_SCALE,
            ..SourceCreditStateV16::EMPTY
        });
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);

    market
        .credit_backing_provider_earnings_not_atomic(0, 4)
        .unwrap();
    assert_eq!(market.header.vault.get(), 10);
    assert_eq!(market.header.backing_provider_earnings_total.get(), 4);
    assert_eq!(
        market.markets[0]
            .engine
            .backing_long
            .utilization_fee_earnings
            .get(),
        4
    );
    market
        .withdraw_backing_provider_earnings_not_atomic(0, 3)
        .unwrap();
    assert_eq!(market.header.vault.get(), 7);
    assert_eq!(market.header.backing_provider_earnings_total.get(), 1);
    assert_eq!(
        market.markets[0]
            .engine
            .backing_long
            .utilization_fee_earnings
            .get(),
        1
    );
    assert_eq!(market.validate_shape(), Ok(()));
}

#[test]
fn v16_backing_provider_earnings_credit_rejects_without_vault_slack() {
    let (mut header, mut markets) = market_fixture(1, 100);
    header.vault = V16PodU128::new(10);
    header.c_tot = V16PodU128::new(10);
    let market_id = markets[0].engine.asset.market_id.get();
    markets[0].engine.backing_long = BackingBucketV16Account::from_runtime(&BackingBucketV16 {
        market_id,
        fresh_unliened_backing_num: 1,
        expiry_slot: 10,
        status: BackingBucketStatusV16::Fresh,
        ..BackingBucketV16::EMPTY
    });
    markets[0].engine.source_credit_long =
        SourceCreditStateV16Account::from_runtime(&SourceCreditStateV16 {
            fresh_reserved_backing_num: 1,
            credit_rate_num: CREDIT_RATE_SCALE,
            ..SourceCreditStateV16::EMPTY
        });
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    assert_eq!(market.validate_shape(), Ok(()));

    let err = market.credit_backing_provider_earnings_not_atomic(0, 1);

    assert_eq!(err, Err(V16Error::LockActive));
    assert_eq!(market.header.backing_provider_earnings_total.get(), 0);
    assert_eq!(
        market.markets[0]
            .engine
            .backing_long
            .utilization_fee_earnings
            .get(),
        0
    );
}

#[test]
fn v16_public_backing_principal_deposit_and_withdraw_move_vault_and_source_state() {
    let (mut header, mut markets) = market_fixture(1, 100);
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);

    market
        .deposit_fresh_counterparty_backing_not_atomic(0, 5, 10)
        .unwrap();
    assert_eq!(market.header.vault.get(), 5);
    assert_eq!(
        market.markets[0]
            .engine
            .backing_long
            .fresh_unliened_backing_num
            .get(),
        5 * BOUND_SCALE
    );
    assert_eq!(
        market.markets[0]
            .engine
            .source_credit_long
            .fresh_reserved_backing_num
            .get(),
        5 * BOUND_SCALE
    );

    market
        .withdraw_fresh_counterparty_backing_not_atomic(0, 2)
        .unwrap();
    assert_eq!(market.header.vault.get(), 3);
    assert_eq!(
        market.markets[0]
            .engine
            .backing_long
            .fresh_unliened_backing_num
            .get(),
        3 * BOUND_SCALE
    );
    assert_eq!(
        market.markets[0]
            .engine
            .source_credit_long
            .fresh_reserved_backing_num
            .get(),
        3 * BOUND_SCALE
    );
    assert_eq!(market.validate_shape(), Ok(()));
}

#[cfg(feature = "fuzz")]
#[test]
fn v16_public_backing_principal_withdraw_rejects_if_claims_would_be_underbacked() {
    let (mut header, mut markets) = market_fixture(1, 100);
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    market
        .deposit_fresh_counterparty_backing_not_atomic(0, 5, 10)
        .unwrap();
    market.header.pnl_pos_bound_tot_num = V16PodU128::new(5 * BOUND_SCALE);
    market.header.pnl_pos_bound_tot = V16PodU128::new(5);
    market
        .add_source_positive_claim_bound_not_atomic(0, 5 * BOUND_SCALE, 5 * BOUND_SCALE)
        .unwrap();

    let err = market.withdraw_fresh_counterparty_backing_not_atomic(0, 1);

    assert_eq!(err, Err(V16Error::LockActive));
    assert_eq!(market.header.vault.get(), 5);
    assert_eq!(
        market.markets[0]
            .engine
            .source_credit_long
            .credit_rate_num
            .get(),
        CREDIT_RATE_SCALE
    );
    assert_eq!(market.validate_shape(), Ok(()));
}

#[test]
fn v16_public_account_backing_fee_routes_provider_and_insurance_splits_atomically() {
    let (mut header, mut markets) = market_fixture(1, 100);
    let mut account_header = account_fixture(1, 23);
    header.vault = V16PodU128::new(100);
    header.c_tot = V16PodU128::new(100);
    account_header.capital = V16PodU128::new(100);
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    market
        .deposit_fresh_counterparty_backing_not_atomic(0, 1, 10)
        .unwrap();
    account_header.health_cert = HealthCertV16Account::from_runtime(&HealthCertV16 {
        certified_equity: 100,
        certified_initial_req: 50,
        certified_maintenance_req: 40,
        cert_oracle_epoch: market.header.oracle_epoch.get(),
        cert_funding_epoch: market.header.funding_epoch.get(),
        cert_risk_epoch: market.header.risk_epoch.get(),
        cert_asset_set_epoch: market.header.asset_set_epoch.get(),
        active_bitmap_at_cert: V16_EMPTY_ACTIVE_BITMAP,
        valid: true,
        ..HealthCertV16::default()
    });
    let mut account = PortfolioV16ViewMut::new(&mut account_header);

    let charged = market
        .charge_account_backing_fee_not_atomic(&mut account, 0, 6, 1, 4)
        .unwrap();

    assert_eq!(charged, 10);
    assert_eq!(market.header.vault.get(), 101);
    assert_eq!(market.header.c_tot.get(), 90);
    assert_eq!(account.header.capital.get(), 90);
    assert_eq!(market.header.insurance.get(), 4);
    assert_eq!(
        market.header.insurance_domain_budget_remaining_total.get(),
        4
    );
    assert_eq!(
        market.markets[0]
            .engine
            .backing_long
            .utilization_fee_earnings
            .get(),
        6
    );
    assert_eq!(account.header.health_cert.certified_equity.get(), 90);
    assert_eq!(market.validate_shape(), Ok(()));
    assert_eq!(account.validate_with_market(&market.as_view()), Ok(()));
}

#[test]
fn v16_public_account_backing_fee_rejects_if_post_fee_im_would_fail() {
    let (mut header, mut markets) = market_fixture(1, 100);
    let mut account_header = account_fixture(1, 24);
    header.vault = V16PodU128::new(100);
    header.c_tot = V16PodU128::new(100);
    account_header.capital = V16PodU128::new(100);
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    market
        .deposit_fresh_counterparty_backing_not_atomic(0, 1, 10)
        .unwrap();
    account_header.health_cert = HealthCertV16Account::from_runtime(&HealthCertV16 {
        certified_equity: 100,
        certified_initial_req: 95,
        certified_maintenance_req: 80,
        cert_oracle_epoch: market.header.oracle_epoch.get(),
        cert_funding_epoch: market.header.funding_epoch.get(),
        cert_risk_epoch: market.header.risk_epoch.get(),
        cert_asset_set_epoch: market.header.asset_set_epoch.get(),
        active_bitmap_at_cert: V16_EMPTY_ACTIVE_BITMAP,
        valid: true,
        ..HealthCertV16::default()
    });
    let mut account = PortfolioV16ViewMut::new(&mut account_header);

    let err = market.charge_account_backing_fee_not_atomic(&mut account, 0, 6, 1, 4);

    assert_eq!(err, Err(V16Error::LockActive));
    assert_eq!(market.header.c_tot.get(), 100);
    assert_eq!(account.header.capital.get(), 100);
    assert_eq!(market.header.insurance.get(), 0);
    assert_eq!(market.validate_shape(), Ok(()));
}

#[test]
fn v16_public_liquidation_on_unfunded_domain_cannot_drain_shared_insurance() {
    let (mut header, mut markets) = market_fixture(1, 100);
    let mut account_header = account_fixture(1, 10);
    header.vault = V16PodU128::new(50);
    header.insurance = V16PodU128::new(50);
    header.negative_pnl_account_count = V16PodU64::new(1);

    let mut asset = markets[0].engine.asset.try_to_runtime().unwrap();
    asset.oi_eff_long_q = 2 * POS_SCALE;
    asset.oi_eff_short_q = 2 * POS_SCALE;
    asset.loss_weight_sum_long = 2 * POS_SCALE;
    asset.loss_weight_sum_short = 2 * POS_SCALE;
    asset.stored_pos_count_long = 2;
    asset.stored_pos_count_short = 2;
    markets[0].engine.asset = AssetStateV16Account::from_runtime(&asset);
    header.resolved_payout_blocker_count = V16PodU64::new(4);

    account_header.pnl = V16PodI128::new(-5);
    account_header.legs[0] = PortfolioLegV16Account::from_runtime(&PortfolioLegV16 {
        active: true,
        asset_index: 0,
        market_id: asset.market_id,
        side: SideV16::Long,
        basis_pos_q: POS_SCALE as i128,
        a_basis: ADL_ONE,
        k_snap: asset.k_long,
        f_snap: asset.f_long_num,
        epoch_snap: asset.epoch_long,
        loss_weight: POS_SCALE,
        b_snap: asset.b_long_num,
        b_rem: 0,
        b_epoch_snap: asset.epoch_long,
        b_stale: false,
        stale: false,
    });
    account_header.active_bitmap[0] = V16PodU64::new(1);

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    let insurance_before = market.header.insurance.get();
    let vault_before = market.header.vault.get();

    let out = market
        .liquidate_account_not_atomic(&mut account, LiquidationRequestV16 { asset_index: 0 })
        .expect("liquidation should progress by booking residual, not draining other domains");

    assert_eq!(out.insurance_used, 0);
    assert_eq!(market.header.insurance.get(), insurance_before);
    assert_eq!(market.header.vault.get(), vault_before);
    assert_eq!(
        market.markets[0].engine.insurance_domain_spent_short.get(),
        0
    );
    assert!(out.residual_booked > 0);
    market.validate_shape().unwrap();
    account.validate_with_market(&market.as_view()).unwrap();
}

#[test]
fn v16_liquidation_engine_selects_full_close_and_allows_dust_min_fee() {
    const PRICE: u64 = 1_000_000;
    const POSITION_Q: u128 = 100;
    const ACCOUNT_CAPITAL: u128 = 50;
    const MIN_LIQ_FEE: u128 = 10;
    const LIQ_FEE_BPS: u64 = 100;

    let (mut header, mut markets) = market_fixture(1, PRICE);
    header.config.liquidation_fee_bps = V16PodU64::new(LIQ_FEE_BPS);
    header.config.min_liquidation_abs = V16PodU128::new(MIN_LIQ_FEE);
    header.config.liquidation_fee_cap = V16PodU128::new(1_000);
    header.vault = V16PodU128::new(ACCOUNT_CAPITAL * 2);
    header.c_tot = V16PodU128::new(ACCOUNT_CAPITAL * 2);

    let mut asset = markets[0].engine.asset.try_to_runtime().unwrap();
    asset.oi_eff_long_q = POSITION_Q * 2;
    asset.oi_eff_short_q = POSITION_Q * 2;
    asset.loss_weight_sum_long = POSITION_Q * 2;
    asset.loss_weight_sum_short = POSITION_Q * 2;
    asset.stored_pos_count_long = 2;
    asset.stored_pos_count_short = 2;
    markets[0].engine.asset = AssetStateV16Account::from_runtime(&asset);
    header.resolved_payout_blocker_count = V16PodU64::new(4);

    let make_liquidatable = |seed: u8| {
        let mut account = account_fixture(1, seed);
        account.capital = V16PodU128::new(ACCOUNT_CAPITAL);
        account.legs[0] = PortfolioLegV16Account::from_runtime(&PortfolioLegV16 {
            active: true,
            asset_index: 0,
            market_id: asset.market_id,
            side: SideV16::Long,
            basis_pos_q: i128::try_from(POSITION_Q).unwrap(),
            a_basis: ADL_ONE,
            k_snap: asset.k_long,
            f_snap: asset.f_long_num,
            epoch_snap: asset.epoch_long,
            loss_weight: POSITION_Q,
            b_snap: asset.b_long_num,
            b_rem: 0,
            b_epoch_snap: asset.epoch_long,
            b_stale: false,
            stale: false,
        });
        account.active_bitmap[0] = V16PodU64::new(1);
        account
    };

    let mut full_header = make_liquidatable(13);
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut full_account = PortfolioV16ViewMut::new(&mut full_header);
    let full = market
        .liquidate_account_not_atomic(&mut full_account, LiquidationRequestV16 { asset_index: 0 })
        .unwrap();

    assert_eq!(full.closed_q, POSITION_Q);
    assert_eq!(full.fee_charged, MIN_LIQ_FEE);
    assert_eq!(
        full_account.header.capital.get(),
        ACCOUNT_CAPITAL - MIN_LIQ_FEE
    );
    assert_eq!(full_account.header.active_bitmap[0].get(), 0);
    assert_eq!(market.header.insurance.get(), MIN_LIQ_FEE);
    market.validate_shape().unwrap();
    full_account
        .validate_with_market(&market.as_view())
        .unwrap();
}

#[test]
fn v16_liquidation_engine_selects_healthy_partial_before_margin_floor() {
    const PRICE: u64 = POS_SCALE as u64;
    const POSITION_Q: u128 = 10_000;
    const ACCOUNT_CAPITAL: u128 = 980;
    const EXPECTED_CLOSE_Q: u128 = 981;
    const EXPECTED_FEE: u128 = 79;

    let (mut header, mut markets) = market_fixture(1, PRICE);
    header.config.maintenance_margin_bps = V16PodU64::new(1_000);
    header.config.initial_margin_bps = V16PodU64::new(1_000);
    header.config.min_nonzero_mm_req = V16PodU128::new(800);
    header.config.min_nonzero_im_req = V16PodU128::new(801);
    header.config.liquidation_fee_bps = V16PodU64::new(800);
    header.config.min_liquidation_abs = V16PodU128::new(0);
    header.config.liquidation_fee_cap = V16PodU128::new(1_000);
    header.config.max_price_move_bps_per_slot = V16PodU64::new(1);
    header
        .config
        .try_to_runtime_shape()
        .unwrap()
        .validate_public_user_fund()
        .unwrap();
    header.vault = V16PodU128::new(ACCOUNT_CAPITAL * 2);
    header.c_tot = V16PodU128::new(ACCOUNT_CAPITAL * 2);

    let mut asset = markets[0].engine.asset.try_to_runtime().unwrap();
    asset.effective_price = PRICE;
    asset.raw_oracle_target_price = PRICE;
    asset.oi_eff_long_q = POSITION_Q * 2;
    asset.oi_eff_short_q = POSITION_Q * 2;
    asset.loss_weight_sum_long = POSITION_Q * 2;
    asset.loss_weight_sum_short = POSITION_Q * 2;
    asset.stored_pos_count_long = 2;
    asset.stored_pos_count_short = 2;
    markets[0].engine.asset = AssetStateV16Account::from_runtime(&asset);
    header.resolved_payout_blocker_count = V16PodU64::new(4);

    let mut account_header = account_fixture(1, 14);
    account_header.capital = V16PodU128::new(ACCOUNT_CAPITAL);
    account_header.legs[0] = PortfolioLegV16Account::from_runtime(&PortfolioLegV16 {
        active: true,
        asset_index: 0,
        market_id: asset.market_id,
        side: SideV16::Long,
        basis_pos_q: i128::try_from(POSITION_Q).unwrap(),
        a_basis: ADL_ONE,
        k_snap: asset.k_long,
        f_snap: asset.f_long_num,
        epoch_snap: asset.epoch_long,
        loss_weight: POSITION_Q,
        b_snap: asset.b_long_num,
        b_rem: 0,
        b_epoch_snap: asset.epoch_long,
        b_stale: false,
        stale: false,
    });
    account_header.active_bitmap[0] = V16PodU64::new(1);

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    let out = market
        .liquidate_account_not_atomic(&mut account, LiquidationRequestV16 { asset_index: 0 })
        .unwrap();

    assert_eq!(out.closed_q, EXPECTED_CLOSE_Q);
    assert_eq!(out.fee_charged, EXPECTED_FEE);
    assert_eq!(account.header.capital.get(), ACCOUNT_CAPITAL - EXPECTED_FEE);
    assert_eq!(account.header.active_bitmap[0].get(), 1);
    let leg = account.header.legs[0].try_to_runtime().unwrap();
    assert_eq!(
        leg.basis_pos_q,
        i128::try_from(POSITION_Q - EXPECTED_CLOSE_Q).unwrap()
    );
    let cert = account.header.health_cert.try_to_runtime().unwrap();
    assert_eq!(cert.certified_liq_deficit, 0);
    assert_eq!(cert.certified_equity, 901);
    assert_eq!(cert.certified_maintenance_req, 901);
    market.validate_shape().unwrap();
    account.validate_with_market(&market.as_view()).unwrap();
}

#[cfg(feature = "fuzz")] // exercises the internal direct-crank primitive via the shim
#[test]
fn v16_permissionless_liquidation_progresses_when_unrelated_asset_is_loss_stale() {
    let (mut header, mut markets) = market_fixture(2, 100);
    let mut account_header = account_fixture(2, 11);
    header.current_slot = V16PodU64::new(10);
    header.slot_last = V16PodU64::new(9);
    header.loss_stale_active = 1;
    header.vault = V16PodU128::new(50);
    header.insurance = V16PodU128::new(50);
    header.negative_pnl_account_count = V16PodU64::new(1);

    let mut asset0 = markets[0].engine.asset.try_to_runtime().unwrap();
    asset0.slot_last = 10;
    asset0.oi_eff_long_q = 2 * POS_SCALE;
    asset0.oi_eff_short_q = 2 * POS_SCALE;
    asset0.loss_weight_sum_long = 2 * POS_SCALE;
    asset0.loss_weight_sum_short = 2 * POS_SCALE;
    asset0.stored_pos_count_long = 2;
    asset0.stored_pos_count_short = 2;
    markets[0].engine.asset = AssetStateV16Account::from_runtime(&asset0);
    let mut asset1 = markets[1].engine.asset.try_to_runtime().unwrap();
    asset1.slot_last = 9;
    asset1.oi_eff_long_q = POS_SCALE;
    asset1.oi_eff_short_q = POS_SCALE;
    asset1.loss_weight_sum_long = POS_SCALE;
    asset1.loss_weight_sum_short = POS_SCALE;
    asset1.stored_pos_count_long = 1;
    asset1.stored_pos_count_short = 1;
    markets[1].engine.asset = AssetStateV16Account::from_runtime(&asset1);
    header.resolved_payout_blocker_count = V16PodU64::new(6);

    account_header.pnl = V16PodI128::new(-5);
    account_header.legs[0] = PortfolioLegV16Account::from_runtime(&PortfolioLegV16 {
        active: true,
        asset_index: 0,
        market_id: asset0.market_id,
        side: SideV16::Long,
        basis_pos_q: POS_SCALE as i128,
        a_basis: ADL_ONE,
        k_snap: asset0.k_long,
        f_snap: asset0.f_long_num,
        epoch_snap: asset0.epoch_long,
        loss_weight: POS_SCALE,
        b_snap: asset0.b_long_num,
        b_rem: 0,
        b_epoch_snap: asset0.epoch_long,
        b_stale: false,
        stale: false,
    });
    account_header.active_bitmap[0] = V16PodU64::new(1);

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    let outcome = market
        .kani_permissionless_crank(
            &mut account,
            percolator::PermissionlessCrankRequestV16 {
                now_slot: 10,
                asset_index: 0,
                effective_price: 100,
                funding_rate_e9: 0,
                action: percolator::PermissionlessCrankActionV16::Liquidate(
                    LiquidationRequestV16 { asset_index: 0 },
                ),
            },
        )
        .expect(
            "locally current liquidation must progress despite unrelated global loss-staleness",
        );

    assert_eq!(
        outcome,
        percolator::PermissionlessProgressOutcomeV16::AccountCurrent
    );
    assert_eq!(market.header.loss_stale_active, 0);
    assert_eq!(market.header.slot_last.get(), 10);
    let unrelated_asset = market.markets[1].engine.asset.try_to_runtime().unwrap();
    assert_eq!(unrelated_asset.slot_last, 9);
    assert_eq!(account.header.active_bitmap[0].get(), 0);
    market.validate_shape().unwrap();
    account.validate_with_market(&market.as_view()).unwrap();
}

#[cfg(feature = "fuzz")] // exercises the internal direct-crank primitive via the shim
#[test]
fn v16_permissionless_recovery_crank_is_value_neutral_and_idempotent() {
    let (mut header, mut markets) = market_fixture(1, 100);
    let mut account_header = account_fixture(1, 12);
    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        let mut account = PortfolioV16ViewMut::new(&mut account_header);
        market.deposit_not_atomic(&mut account, 7).unwrap();
    }
    header.insurance = V16PodU128::new(3);
    header.vault = V16PodU128::new(10);
    let vault_before = header.vault;
    let c_tot_before = header.c_tot;
    let insurance_before = header.insurance;
    let capital_before = account_header.capital;
    let pnl_before = account_header.pnl;

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    let first = market
        .kani_permissionless_crank(
            &mut account,
            PermissionlessCrankRequestV16 {
                now_slot: 1,
                asset_index: 0,
                effective_price: 100,
                funding_rate_e9: 0,
                action: PermissionlessCrankActionV16::Recover(
                    PermissionlessRecoveryReasonV16::ExplicitLossOrDustAuditOverflow,
                ),
            },
        )
        .unwrap();
    let second = market
        .kani_permissionless_crank(
            &mut account,
            PermissionlessCrankRequestV16 {
                now_slot: 1,
                asset_index: 0,
                effective_price: 100,
                funding_rate_e9: 0,
                action: PermissionlessCrankActionV16::Recover(
                    PermissionlessRecoveryReasonV16::BIndexHeadroomExhausted,
                ),
            },
        )
        .unwrap();
    let refresh_after_recovery = market.kani_permissionless_crank(
        &mut account,
        PermissionlessCrankRequestV16 {
            now_slot: 1,
            asset_index: 0,
            effective_price: 100,
            funding_rate_e9: 0,
            action: PermissionlessCrankActionV16::Refresh,
        },
    );

    assert_eq!(
        first,
        PermissionlessProgressOutcomeV16::RecoveryDeclared(
            PermissionlessRecoveryReasonV16::ExplicitLossOrDustAuditOverflow
        )
    );
    assert_eq!(second, first);
    assert_eq!(refresh_after_recovery, Err(V16Error::LockActive));
    assert_eq!(market.header.vault, vault_before);
    assert_eq!(market.header.c_tot, c_tot_before);
    assert_eq!(market.header.insurance, insurance_before);
    assert_eq!(account.header.capital, capital_before);
    assert_eq!(account.header.pnl, pnl_before);
    market.validate_shape().unwrap();
    account.validate_with_market(&market.as_view()).unwrap();
}

#[test]
fn v16_resolved_payout_topup_finishes_receipt_without_overpaying() {
    let (mut header, mut markets) = market_fixture(1, 100);
    let mut account_header = account_fixture(1, 13);
    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        market.resolve_market_not_atomic(1).unwrap();
    }
    let terminal_claim = 10u128;
    header.vault = V16PodU128::new(4);
    header.payout_snapshot_captured = 1;
    header.resolved_payout_ledger =
        ResolvedPayoutLedgerV16Account::from_runtime(&ResolvedPayoutLedgerV16 {
            snapshot_residual: terminal_claim,
            terminal_claim_exact_receipts_num: terminal_claim * BOUND_SCALE,
            terminal_claim_bound_unreceipted_num: 0,
            current_payout_rate_num: 1,
            current_payout_rate_den: 1,
            snapshot_slot: 1,
            payout_halted: false,
            finalized: false,
        });
    account_header.resolved_payout_receipt =
        ResolvedPayoutReceiptV16Account::from_runtime(&ResolvedPayoutReceiptV16 {
            present: true,
            prior_bound_contribution_num: terminal_claim * BOUND_SCALE,
            live_released_face_at_receipt: 0,
            terminal_positive_claim_face: terminal_claim,
            paid_effective: 2,
            finalized: false,
        });

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    let first = market
        .claim_resolved_payout_topup_not_atomic(&mut account)
        .unwrap();
    let after_first = account
        .header
        .resolved_payout_receipt
        .try_to_runtime()
        .unwrap();
    market.header.vault = V16PodU128::new(4);
    let second = market
        .claim_resolved_payout_topup_not_atomic(&mut account)
        .unwrap();
    let after_second = account
        .header
        .resolved_payout_receipt
        .try_to_runtime()
        .unwrap();
    let third = market
        .claim_resolved_payout_topup_not_atomic(&mut account)
        .unwrap();

    assert_eq!(first, 4);
    assert_eq!(after_first.paid_effective, 6);
    assert!(!after_first.finalized);
    assert_eq!(second, 4);
    assert_eq!(after_second.paid_effective, terminal_claim);
    assert!(after_second.finalized);
    assert_eq!(third, 0);
    assert_eq!(market.header.vault.get(), 0);
    market.validate_shape().unwrap();
    account.validate_with_market(&market.as_view()).unwrap();
}

#[test]
fn v16_risk_increasing_trade_creates_source_credit_lien_for_im() {
    let (mut header, mut markets) = market_fixture(1, 1);
    let mut long_header = account_fixture(1, 8);
    let mut short_header = account_fixture(1, 9);
    let claim = 100u128;
    let claim_num = claim * BOUND_SCALE;
    long_header.pnl = V16PodI128::new(claim as i128);
    long_header.source_domains[0].domain = V16PodU32::new(0);
    long_header.source_domains[0].source_claim_market_id = V16PodU64::new(1);
    long_header.source_domains[0].source_claim_bound_num = V16PodU128::new(claim_num);
    header.pnl_pos_tot = V16PodU128::new(claim);
    header.pnl_pos_bound_tot_num = V16PodU128::new(claim_num);
    header.pnl_pos_bound_tot = V16PodU128::new(claim);
    header.source_claim_bound_total_num = V16PodU128::new(claim_num);
    // The hand-built fresh backing below is recoverable provider principal: it
    // must be funded by vault atoms and tracked in the header aggregate.
    header.vault = V16PodU128::new(claim);
    header.source_fresh_backing_total_num = V16PodU128::new(claim_num);
    markets[0].engine.source_credit_long =
        SourceCreditStateV16Account::from_runtime(&SourceCreditStateV16 {
            positive_claim_bound_num: claim_num,
            exact_positive_claim_num: claim_num,
            fresh_reserved_backing_num: claim_num,
            credit_rate_num: CREDIT_RATE_SCALE,
            ..SourceCreditStateV16::EMPTY
        });
    markets[0].engine.backing_long = BackingBucketV16Account::from_runtime(&BackingBucketV16 {
        market_id: 1,
        fresh_unliened_backing_num: claim_num,
        expiry_slot: 100,
        status: BackingBucketStatusV16::Fresh,
        ..BackingBucketV16::EMPTY
    });
    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        let mut short = PortfolioV16ViewMut::new(&mut short_header);
        market.deposit_not_atomic(&mut short, 1_000).unwrap();
    }

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut long = PortfolioV16ViewMut::new(&mut long_header);
    let mut short = PortfolioV16ViewMut::new(&mut short_header);
    market
        .execute_trade_with_fee_loss_stale_scoped_not_atomic(
            &mut long,
            &mut short,
            TradeRequestV16 {
                asset_index: 0,
                size_q: signed_q(10 * POS_SCALE),
                exec_price: 1,
                fee_bps: 0,
            },
        )
        .expect("risk-increasing trade should atomically lien backed source credit for IM");

    assert_eq!(long.header.capital.get(), 0);
    assert_eq!(
        long.header.source_domains[0].source_claim_liened_num.get(),
        10 * BOUND_SCALE
    );
    assert_eq!(
        long.header.source_domains[0]
            .source_lien_effective_reserved
            .get(),
        10
    );
    assert_eq!(
        long.header.source_domains[0]
            .source_lien_counterparty_backing_num
            .get(),
        10 * BOUND_SCALE
    );
    assert_eq!(
        market.markets[0]
            .engine
            .source_credit_long
            .valid_liened_backing_num
            .get(),
        10 * BOUND_SCALE
    );
    assert_eq!(
        market.markets[0]
            .engine
            .backing_long
            .valid_liened_backing_num
            .get(),
        10 * BOUND_SCALE
    );
    assert_eq!(
        market.markets[0]
            .engine
            .backing_long
            .fresh_unliened_backing_num
            .get(),
        90 * BOUND_SCALE
    );
    assert_eq!(
        market.convert_released_pnl_to_capital_not_atomic(&mut long),
        Err(V16Error::LockActive),
        "source-backed positive PnL must not be realized while the source-claim exposure remains open"
    );
    market.validate_shape().unwrap();
    long.validate_with_market(&market.as_view()).unwrap();
    short.validate_with_market(&market.as_view()).unwrap();
}

#[test]
fn v16_resolved_impaired_source_accepts_prospective_terminal_loss() {
    const Q: u128 = 1_000 * POS_SCALE;
    const INCREASE_Q: u128 = POS_SCALE;
    let (market_id, _, _) = ids();
    let mut cfg = V16Config::public_user_fund_with_market_slots(1, 1, 0, 10);
    cfg.maintenance_margin_bps = 1_000;
    cfg.initial_margin_bps = 5_000;
    cfg.max_price_move_bps_per_slot = 500;
    cfg.max_accrual_dt_slots = 1;
    let mut header = MarketGroupV16HeaderAccount::new_dynamic(market_id, cfg, 1, 0).unwrap();
    let mut markets = vec![Market::new(0, EngineAssetSlotV16Account::default())];
    header
        .activate_empty_asset_slot_not_atomic(0, &mut markets[0].engine, 100, 1)
        .unwrap();

    let mut liened_winner_header = account_fixture(1, 40);
    let mut liened_peer_header = account_fixture(1, 41);
    let mut expiry_trigger_header = account_fixture(1, 42);
    let mut prospective_loser_header = account_fixture(1, 43);
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut liened_winner = PortfolioV16ViewMut::new(&mut liened_winner_header);
    let mut liened_peer = PortfolioV16ViewMut::new(&mut liened_peer_header);
    let mut expiry_trigger = PortfolioV16ViewMut::new(&mut expiry_trigger_header);
    let mut prospective_loser = PortfolioV16ViewMut::new(&mut prospective_loser_header);

    market
        .deposit_fresh_counterparty_backing_not_atomic(1, 100_000, 3)
        .unwrap();
    market
        .deposit_not_atomic(&mut liened_winner, 52_501)
        .unwrap();
    market
        .deposit_not_atomic(&mut liened_peer, 1_000_000)
        .unwrap();
    market
        .deposit_not_atomic(&mut expiry_trigger, 1_000_000)
        .unwrap();
    market
        .deposit_not_atomic(&mut prospective_loser, 1_000_000)
        .unwrap();
    for (long, short) in [
        (&mut liened_winner, &mut liened_peer),
        (&mut expiry_trigger, &mut prospective_loser),
    ] {
        market
            .execute_trade_with_fee_loss_stale_scoped_not_atomic(
                long,
                short,
                TradeRequestV16 {
                    asset_index: 0,
                    size_q: signed_q(Q),
                    exec_price: 100,
                    fee_bps: 0,
                },
            )
            .unwrap();
    }

    market
        .set_asset_raw_oracle_target_not_atomic(0, 105)
        .unwrap();
    market
        .accrue_asset_to_not_atomic(0, 2, 105, 0, true)
        .unwrap();
    for account in [&mut liened_peer, &mut liened_winner, &mut expiry_trigger] {
        market.full_account_refresh_not_atomic(account).unwrap();
    }
    assert_eq!(prospective_loser.header.pnl.get(), 0);
    market
        .execute_trade_with_fee_loss_stale_scoped_not_atomic(
            &mut liened_winner,
            &mut liened_peer,
            TradeRequestV16 {
                asset_index: 0,
                size_q: signed_q(INCREASE_Q),
                exec_price: 105,
                fee_bps: 0,
            },
        )
        .unwrap();

    market.resolve_market_not_atomic(3).unwrap();
    assert_eq!(
        market
            .close_resolved_account_not_atomic(&mut expiry_trigger, 0)
            .unwrap(),
        percolator::ResolvedCloseOutcomeV16::ProgressOnly,
    );
    assert_eq!(
        market.markets[0]
            .engine
            .backing_short
            .try_to_runtime()
            .unwrap()
            .status,
        BackingBucketStatusV16::Impaired,
    );

    market
        .close_resolved_account_not_atomic(&mut prospective_loser, 0)
        .expect("an impaired source bucket must accept a prospective terminal loss");
    assert_eq!(prospective_loser.header.pnl.get(), 0);
    market.validate_shape().unwrap();
    prospective_loser
        .validate_with_market(&market.as_view())
        .unwrap();
}

#[test]
fn v16_resolved_foreign_expiry_impairs_account_lien_before_release() {
    const Q: u128 = 1_000 * POS_SCALE;
    const INCREASE_Q: u128 = POS_SCALE;
    let (market_id, _, _) = ids();
    let mut cfg = V16Config::public_user_fund_with_market_slots(1, 1, 0, 10);
    cfg.maintenance_margin_bps = 1_000;
    cfg.initial_margin_bps = 5_000;
    cfg.max_price_move_bps_per_slot = 500;
    cfg.max_accrual_dt_slots = 1;
    let mut header = MarketGroupV16HeaderAccount::new_dynamic(market_id, cfg, 1, 0).unwrap();
    let mut markets = vec![Market::new(0, EngineAssetSlotV16Account::default())];
    header
        .activate_empty_asset_slot_not_atomic(0, &mut markets[0].engine, 100, 1)
        .unwrap();

    let mut target_header = account_fixture(1, 44);
    let mut target_peer_header = account_fixture(1, 45);
    let mut expiry_trigger_header = account_fixture(1, 46);
    let mut trigger_peer_header = account_fixture(1, 47);
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut target = PortfolioV16ViewMut::new(&mut target_header);
    let mut target_peer = PortfolioV16ViewMut::new(&mut target_peer_header);
    let mut expiry_trigger = PortfolioV16ViewMut::new(&mut expiry_trigger_header);
    let mut trigger_peer = PortfolioV16ViewMut::new(&mut trigger_peer_header);

    market
        .deposit_fresh_counterparty_backing_not_atomic(1, 100_000, 3)
        .unwrap();
    market.deposit_not_atomic(&mut target, 52_501).unwrap();
    market
        .deposit_not_atomic(&mut target_peer, 1_000_000)
        .unwrap();
    market
        .deposit_not_atomic(&mut expiry_trigger, 1_000_000)
        .unwrap();
    market
        .deposit_not_atomic(&mut trigger_peer, 1_000_000)
        .unwrap();
    for (long, short) in [
        (&mut target, &mut target_peer),
        (&mut expiry_trigger, &mut trigger_peer),
    ] {
        market
            .execute_trade_with_fee_loss_stale_scoped_not_atomic(
                long,
                short,
                TradeRequestV16 {
                    asset_index: 0,
                    size_q: signed_q(Q),
                    exec_price: 100,
                    fee_bps: 0,
                },
            )
            .unwrap();
    }

    market
        .set_asset_raw_oracle_target_not_atomic(0, 105)
        .unwrap();
    market
        .accrue_asset_to_not_atomic(0, 2, 105, 0, true)
        .unwrap();
    for account in [
        &mut target_peer,
        &mut trigger_peer,
        &mut target,
        &mut expiry_trigger,
    ] {
        market.full_account_refresh_not_atomic(account).unwrap();
    }
    market
        .execute_trade_with_fee_loss_stale_scoped_not_atomic(
            &mut target,
            &mut target_peer,
            TradeRequestV16 {
                asset_index: 0,
                size_q: signed_q(INCREASE_Q),
                exec_price: 105,
                fee_bps: 0,
            },
        )
        .unwrap();
    let lien_before = target.header.source_domains[0];
    assert!(lien_before.source_claim_counterparty_liened_num.get() > 0);
    assert!(lien_before.source_lien_counterparty_backing_num.get() > 0);
    assert_eq!(lien_before.source_claim_impaired_num.get(), 0);
    assert!(
        expiry_trigger.header.source_domains[0]
            .source_claim_bound_num
            .get()
            > 0
    );
    assert_eq!(
        expiry_trigger.header.source_domains[0]
            .source_claim_liened_num
            .get(),
        0
    );

    market.resolve_market_not_atomic(3).unwrap();
    assert_eq!(
        market
            .close_resolved_account_not_atomic(&mut expiry_trigger, 0)
            .unwrap(),
        percolator::ResolvedCloseOutcomeV16::ProgressOnly,
    );
    let bucket_before = market.markets[0]
        .engine
        .backing_short
        .try_to_runtime()
        .unwrap();
    assert_eq!(bucket_before.status, BackingBucketStatusV16::Impaired);

    let mut relabeled = false;
    for _ in 0..4 {
        assert_eq!(
            market
                .close_resolved_account_not_atomic(&mut target, 0)
                .expect("foreign bucket expiry must leave bounded lien-impairment continuations",),
            percolator::ResolvedCloseOutcomeV16::ProgressOnly,
        );
        if target.header.source_domains[0]
            .source_claim_counterparty_liened_num
            .get()
            == 0
        {
            relabeled = true;
            break;
        }
    }
    assert!(relabeled, "the account-local lien never became impaired");
    let impaired = target.header.source_domains[0];
    assert_eq!(impaired.source_claim_liened_num.get(), 0);
    assert_eq!(impaired.source_claim_counterparty_liened_num.get(), 0);
    assert_eq!(impaired.source_lien_counterparty_backing_num.get(), 0);
    assert_eq!(
        impaired.source_claim_impaired_num.get(),
        lien_before.source_claim_counterparty_liened_num.get()
    );
    assert_eq!(
        impaired.source_lien_impaired_effective_reserved.get(),
        lien_before.source_lien_effective_reserved.get()
    );
    assert_eq!(
        market.markets[0]
            .engine
            .backing_short
            .try_to_runtime()
            .unwrap(),
        bucket_before,
        "account-local relabel must not mutate the already-impaired aggregate bucket"
    );

    let mut all_closed = false;
    let mut last_outcomes = Vec::new();
    for _ in 0..16 {
        last_outcomes.clear();
        for account in [
            &mut target_peer,
            &mut trigger_peer,
            &mut expiry_trigger,
            &mut target,
        ] {
            last_outcomes.push(
                market
                    .close_resolved_account_not_atomic(account, 0)
                    .expect("foreign-expired source claims must retain a terminal continuation"),
            );
        }
        all_closed = [&target_peer, &trigger_peer, &expiry_trigger, &target]
            .iter()
            .all(|account| {
                account.header.capital.get() == 0
                    && account.header.pnl.get() == 0
                    && active_bitmap_is_empty(account.header.active_bitmap.map(V16PodU64::get))
            });
        if all_closed {
            break;
        }
    }
    assert!(
        all_closed,
        "foreign-expired source claims did not terminate: last={last_outcomes:?}"
    );
    let bucket_after = market.markets[0]
        .engine
        .backing_short
        .try_to_runtime()
        .unwrap();
    let source_after = market.markets[0]
        .engine
        .source_credit_short
        .try_to_runtime()
        .unwrap();
    assert_eq!(bucket_after.status, BackingBucketStatusV16::Expired);
    assert_eq!(bucket_after.impaired_liened_backing_num, 0);
    assert_eq!(source_after.impaired_liened_backing_num, 0);
    market.validate_shape().unwrap();
    target.validate_with_market(&market.as_view()).unwrap();
}

#[test]
fn v16_live_mark_reversal_unwinds_source_lien_before_claim_burn() {
    const OPEN_Q: u128 = 1_000 * POS_SCALE;
    const INCREASE_Q: u128 = 50 * POS_SCALE;
    let (mut header, mut markets) = market_fixture(1, 100);
    header.config.maintenance_margin_bps = V16PodU64::new(1_000);
    header.config.initial_margin_bps = V16PodU64::new(5_000);
    header.config.max_price_move_bps_per_slot = V16PodU64::new(500);
    header.config.max_accrual_dt_slots = V16PodU64::new(1);
    header.config.min_funding_lifetime_slots = V16PodU64::new(1);
    let mut long_header = account_fixture(1, 10);
    let mut short_header = account_fixture(1, 11);

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut long = PortfolioV16ViewMut::new(&mut long_header);
    let mut short = PortfolioV16ViewMut::new(&mut short_header);
    market
        .deposit_fresh_counterparty_backing_not_atomic(1, 100_000, 100)
        .unwrap();
    market.deposit_not_atomic(&mut long, 52_501).unwrap();
    market.deposit_not_atomic(&mut short, 1_000_000).unwrap();
    market
        .execute_trade_with_fee_loss_stale_scoped_not_atomic(
            &mut long,
            &mut short,
            TradeRequestV16 {
                asset_index: 0,
                size_q: signed_q(OPEN_Q),
                exec_price: 100,
                fee_bps: 0,
            },
        )
        .unwrap();

    market
        .set_asset_raw_oracle_target_not_atomic(0, 105)
        .unwrap();
    market
        .accrue_asset_to_not_atomic(0, 2, 105, 0, true)
        .unwrap();
    market.full_account_refresh_not_atomic(&mut short).unwrap();
    market.full_account_refresh_not_atomic(&mut long).unwrap();
    assert_eq!(long.header.pnl.get(), 5_000);
    market
        .execute_trade_with_fee_loss_stale_scoped_not_atomic(
            &mut long,
            &mut short,
            TradeRequestV16 {
                asset_index: 0,
                size_q: signed_q(INCREASE_Q),
                exec_price: 105,
                fee_bps: 0,
            },
        )
        .unwrap();
    let lien_before = long.header.source_domains[0];
    assert_eq!(long.header.pnl.get(), 5_000);
    assert!(lien_before.source_claim_liened_num.get() > 0);
    assert!(lien_before.source_lien_counterparty_backing_num.get() > 0);
    let capital_before_reversal = long.header.capital.get();
    let lien_effective = lien_before.source_lien_effective_reserved.get();
    let backing_before_reversal = market.markets[0]
        .engine
        .backing_short
        .try_to_runtime()
        .unwrap();

    market
        .set_asset_raw_oracle_target_not_atomic(0, 100)
        .unwrap();
    market
        .accrue_asset_to_not_atomic(0, 3, 100, 0, true)
        .unwrap();
    market.full_account_refresh_not_atomic(&mut short).unwrap();
    let cert = market
        .full_account_refresh_not_atomic(&mut long)
        .expect("a mark reversal must settle even when the prior positive claim backed IM");

    let backing_after_reversal = market.markets[0]
        .engine
        .backing_short
        .try_to_runtime()
        .unwrap();
    let unliened_support_consumed = 5_000 - lien_effective;
    let principal_loss = 5_250 - unliened_support_consumed;
    assert_eq!(long.header.pnl.get(), 0);
    assert_eq!(
        long.header.capital.get(),
        capital_before_reversal - principal_loss
    );
    assert_eq!(long.header.source_domains[0], Default::default());
    assert_eq!(
        backing_after_reversal.fresh_unliened_backing_num,
        backing_before_reversal
            .fresh_unliened_backing_num
            .checked_sub(unliened_support_consumed * BOUND_SCALE)
            .unwrap()
            .checked_add(lien_before.source_lien_counterparty_backing_num.get())
            .unwrap(),
        "the still-liened backing is unpledged rather than consumed"
    );
    assert_eq!(backing_after_reversal.valid_liened_backing_num, 0);
    assert_eq!(
        backing_after_reversal.consumed_liened_backing_num,
        backing_before_reversal.consumed_liened_backing_num
            + unliened_support_consumed * BOUND_SCALE,
        "only realizable unliened support offsets the reversal loss"
    );
    assert!(cert.valid);
    assert!(
        cert.certified_equity >= 0 && (cert.certified_equity as u128) < cert.certified_initial_req,
        "the regression requires a funded account below initial margin"
    );
    market.validate_shape().unwrap();
    long.validate_with_market(&market.as_view()).unwrap();
    short.validate_with_market(&market.as_view()).unwrap();

    market
        .execute_trade_with_fee_loss_stale_scoped_not_atomic(
            &mut long,
            &mut short,
            TradeRequestV16 {
                asset_index: 0,
                size_q: -signed_q(POS_SCALE),
                exec_price: 100,
                fee_bps: 0,
            },
        )
        .expect("risk-reducing trade remains available after source-lien unwind");
}

#[test]
fn v16_under_margin_owner_can_transfer_risk_to_margin_healthy_counterparty() {
    const OPEN_Q: u128 = 100 * POS_SCALE;
    let (mut header, mut markets) = market_fixture(1, 100);
    header.config.maintenance_margin_bps = V16PodU64::new(1_000);
    header.config.initial_margin_bps = V16PodU64::new(5_000);
    header.config.max_price_move_bps_per_slot = V16PodU64::new(1_000);
    header.config.max_accrual_dt_slots = V16PodU64::new(1);
    let mut owner_header = account_fixture(1, 12);
    let mut original_short_header = account_fixture(1, 13);
    let mut new_holder_header = account_fixture(1, 14);

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut owner = PortfolioV16ViewMut::new(&mut owner_header);
    let mut original_short = PortfolioV16ViewMut::new(&mut original_short_header);
    let mut new_holder = PortfolioV16ViewMut::new(&mut new_holder_header);
    market.deposit_not_atomic(&mut owner, 5_001).unwrap();
    market
        .deposit_not_atomic(&mut original_short, 100_000)
        .unwrap();
    market.deposit_not_atomic(&mut new_holder, 10_000).unwrap();
    market
        .execute_trade_with_fee_loss_stale_scoped_not_atomic(
            &mut owner,
            &mut original_short,
            TradeRequestV16 {
                asset_index: 0,
                size_q: signed_q(OPEN_Q),
                exec_price: 100,
                fee_bps: 0,
            },
        )
        .unwrap();

    market
        .set_asset_raw_oracle_target_not_atomic(0, 90)
        .unwrap();
    market
        .accrue_asset_to_not_atomic(0, 2, 90, 0, true)
        .unwrap();
    market
        .full_account_refresh_not_atomic(&mut original_short)
        .unwrap();
    let owner_cert = market.full_account_refresh_not_atomic(&mut owner).unwrap();
    market
        .full_account_refresh_not_atomic(&mut new_holder)
        .unwrap();
    assert!(
        owner_cert.certified_equity >= 0
            && (owner_cert.certified_equity as u128) < owner_cert.certified_initial_req,
        "owner must be below IM before the transfer"
    );

    market
        .execute_trade_with_fee_loss_stale_scoped_not_atomic(
            &mut owner,
            &mut new_holder,
            TradeRequestV16 {
                asset_index: 0,
                size_q: -signed_q(POS_SCALE),
                exec_price: 90,
                fee_bps: 0,
            },
        )
        .expect("strict reducer may exit while the new risk holder passes IM");

    assert_eq!(
        owner.header.legs[0].try_to_runtime().unwrap().basis_pos_q,
        signed_q(99 * POS_SCALE)
    );
    assert_eq!(
        new_holder.header.legs[0]
            .try_to_runtime()
            .unwrap()
            .basis_pos_q,
        signed_q(POS_SCALE)
    );
    let new_holder_cert = new_holder.header.health_cert.try_to_runtime().unwrap();
    assert!(
        new_holder_cert.valid
            && new_holder_cert.certified_equity >= 0
            && (new_holder_cert.certified_equity as u128) >= new_holder_cert.certified_initial_req,
        "new risk holder remains fully margined"
    );
    market.validate_shape().unwrap();
    owner.validate_with_market(&market.as_view()).unwrap();
    original_short
        .validate_with_market(&market.as_view())
        .unwrap();
    new_holder.validate_with_market(&market.as_view()).unwrap();
}

#[test]
fn v16_residual_reward_credit_uses_real_principal_not_notional() {
    let (mut header, mut markets) = market_fixture(1, 1_000);
    header.config.initial_margin_bps = V16PodU64::new(500);
    header.config.maintenance_margin_bps = V16PodU64::new(500);
    header.config.min_nonzero_im_req = V16PodU128::new(2);
    header.config.min_nonzero_mm_req = V16PodU128::new(1);
    let mut taker_header = account_fixture(1, 23);
    let mut lp_header = account_fixture(1, 24);
    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        let mut taker = PortfolioV16ViewMut::new(&mut taker_header);
        let mut lp = PortfolioV16ViewMut::new(&mut lp_header);
        market.deposit_not_atomic(&mut taker, 10_000).unwrap();
        market.deposit_not_atomic(&mut lp, 10_000).unwrap();
    }

    taker_header.residual_crystallized_loss_atoms_total = V16PodU128::new(10_000);
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut taker = PortfolioV16ViewMut::new(&mut taker_header);
    let mut lp = PortfolioV16ViewMut::new(&mut lp_header);
    market
        .execute_trade_with_fee_loss_stale_scoped_not_atomic(
            &mut taker,
            &mut lp,
            TradeRequestV16 {
                asset_index: 0,
                size_q: signed_q(POS_SCALE),
                exec_price: 1_000,
                fee_bps: 0,
            },
        )
        .unwrap();

    assert_eq!(
        taker.header.residual_spent_principal_atoms_total.get(),
        50,
        "1 lot at price 1000 with 500 bps IM spends only 50 atoms of residual budget"
    );
    assert_eq!(lp.header.residual_received_atoms_total.get(), 50);
    assert_ne!(
        lp.header.residual_received_atoms_total.get(),
        1_000,
        "counter must not credit leveraged notional"
    );
    taker.validate_with_market(&market.as_view()).unwrap();
    lp.validate_with_market(&market.as_view()).unwrap();
    market.validate_shape().unwrap();
}

#[test]
fn v16_residual_reward_credit_is_capped_by_available_crystallized_loss() {
    let (mut header, mut markets) = market_fixture(1, 1_000);
    header.config.initial_margin_bps = V16PodU64::new(500);
    header.config.maintenance_margin_bps = V16PodU64::new(500);
    let mut taker_header = account_fixture(1, 25);
    let mut lp_header = account_fixture(1, 26);
    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        let mut taker = PortfolioV16ViewMut::new(&mut taker_header);
        let mut lp = PortfolioV16ViewMut::new(&mut lp_header);
        market.deposit_not_atomic(&mut taker, 10_000).unwrap();
        market.deposit_not_atomic(&mut lp, 10_000).unwrap();
    }

    taker_header.residual_crystallized_loss_atoms_total = V16PodU128::new(30);
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut taker = PortfolioV16ViewMut::new(&mut taker_header);
    let mut lp = PortfolioV16ViewMut::new(&mut lp_header);
    market
        .execute_trade_with_fee_loss_stale_scoped_not_atomic(
            &mut taker,
            &mut lp,
            TradeRequestV16 {
                asset_index: 0,
                size_q: signed_q(POS_SCALE),
                exec_price: 1_000,
                fee_bps: 0,
            },
        )
        .unwrap();

    assert_eq!(taker.header.residual_spent_principal_atoms_total.get(), 30);
    assert_eq!(lp.header.residual_received_atoms_total.get(), 30);
    taker.validate_with_market(&market.as_view()).unwrap();
    lp.validate_with_market(&market.as_view()).unwrap();
}

#[test]
fn v16_principal_loss_crystallizes_residual_budget_monotonically() {
    let (mut header, mut markets) = market_fixture(1, 100);
    let mut account_header = account_fixture(1, 27);
    header.vault = V16PodU128::new(100);
    header.c_tot = V16PodU128::new(100);
    header.negative_pnl_account_count = V16PodU64::new(1);
    account_header.capital = V16PodU128::new(100);
    account_header.pnl = V16PodI128::new(-40);
    account_header.residual_crystallized_loss_atoms_total = V16PodU128::new(7);

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    market
        .sync_account_fee_to_slot_not_atomic(&mut account, 1, 0)
        .unwrap();

    assert_eq!(account.header.capital.get(), 60);
    assert_eq!(account.header.pnl.get(), 0);
    assert_eq!(
        account.header.residual_crystallized_loss_atoms_total.get(),
        47,
        "historical crystallized-loss budget only increases by real capital consumed"
    );
    account.validate_with_market(&market.as_view()).unwrap();
    market.validate_shape().unwrap();
}

#[test]
fn v16_source_backed_conversion_clears_sparse_source_domain_slot() {
    let (mut header, mut markets) = market_fixture(1, 1);
    let mut account_header = account_fixture(1, 18);
    let claim = 20u128;
    let claim_num = claim * BOUND_SCALE;
    // Keep an unrelated live residual and historical opposite-domain insurance
    // spend present. Only the claim-free terminal sweep may recredit an overlap.
    header.vault = V16PodU128::new(claim + 10);
    header.insurance = V16PodU128::new(5);
    header.insurance_domain_budget_remaining_total = V16PodU128::new(5);
    header.pnl_pos_tot = V16PodU128::new(claim);
    header.pnl_pos_bound_tot_num = V16PodU128::new(claim_num);
    header.pnl_pos_bound_tot = V16PodU128::new(claim);
    header.source_claim_bound_total_num = V16PodU128::new(claim_num);
    header.source_fresh_backing_total_num = V16PodU128::new(claim_num);
    account_header.pnl = V16PodI128::new(claim as i128);
    account_header.source_domains[0].domain = V16PodU32::new(0);
    account_header.source_domains[0].source_claim_market_id = V16PodU64::new(1);
    account_header.source_domains[0].source_claim_bound_num = V16PodU128::new(claim_num);
    markets[0].engine.source_credit_long =
        SourceCreditStateV16Account::from_runtime(&SourceCreditStateV16 {
            positive_claim_bound_num: claim_num,
            exact_positive_claim_num: claim_num,
            fresh_reserved_backing_num: claim_num,
            credit_rate_num: CREDIT_RATE_SCALE,
            ..SourceCreditStateV16::EMPTY
        });
    markets[0].engine.backing_long = BackingBucketV16Account::from_runtime(&BackingBucketV16 {
        market_id: 1,
        fresh_unliened_backing_num: claim_num,
        expiry_slot: 100,
        status: BackingBucketStatusV16::Fresh,
        ..BackingBucketV16::EMPTY
    });
    markets[0].engine.insurance_domain_budget_short = V16PodU128::new(10);
    markets[0].engine.insurance_domain_spent_short = V16PodU128::new(5);

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    market
        .full_account_refresh_not_atomic(&mut account)
        .unwrap();
    let converted = market
        .convert_released_pnl_to_capital_not_atomic(&mut account)
        .expect("flat source-backed PnL should be convertible when backing is available");

    assert_eq!(converted, claim);
    assert_eq!(account.header.pnl.get(), 0);
    assert_eq!(account.header.capital.get(), claim);
    assert_eq!(
        market.header.insurance.get(),
        5,
        "live conversion must not recredit historical insurance spend"
    );
    assert_eq!(
        market.markets[0].engine.insurance_domain_spent_short.get(),
        5
    );
    assert_eq!(
        account.header.source_domains[0],
        PortfolioSourceDomainV16Account::default()
    );
    account.validate_with_market(&market.as_view()).unwrap();
    market.validate_shape().unwrap();
}

#[test]
fn v16_sparse_source_domains_reject_unoccupied_tagged_slot() {
    let (mut header, mut markets) = market_fixture(1, 1);
    let mut account_header = account_fixture(1, 19);
    account_header.source_domains[1].domain = V16PodU32::new(1);
    account_header.source_domains[1].source_claim_market_id = V16PodU64::new(1);

    let market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let account = PortfolioV16View::new(&account_header);
    assert_eq!(
        account.validate_with_market(&market.as_view()),
        Err(V16Error::HiddenLeg),
        "unoccupied tagged source-domain slots must not survive validation"
    );
}

#[test]
fn v16_mutable_view_compacts_persisted_domain_indexed_source_claim_before_deposit() {
    let (mut header, mut markets) = market_fixture(1, 1);
    let mut account_header = account_fixture(1, 20);
    let claim = 7u128;
    let claim_num = claim * BOUND_SCALE;
    header.vault = V16PodU128::new(claim);
    header.c_tot = V16PodU128::new(0);
    header.pnl_pos_tot = V16PodU128::new(claim);
    header.pnl_pos_bound_tot_num = V16PodU128::new(claim_num);
    header.pnl_pos_bound_tot = V16PodU128::new(claim);
    header.source_claim_bound_total_num = V16PodU128::new(claim_num);
    header.source_fresh_backing_total_num = V16PodU128::new(claim_num);
    account_header.pnl = V16PodI128::new(claim as i128);
    account_header.source_domains[1].domain = V16PodU32::new(1);
    account_header.source_domains[1].source_claim_market_id = V16PodU64::new(1);
    account_header.source_domains[1].source_claim_bound_num = V16PodU128::new(claim_num);
    markets[0].engine.source_credit_short =
        SourceCreditStateV16Account::from_runtime(&SourceCreditStateV16 {
            positive_claim_bound_num: claim_num,
            exact_positive_claim_num: claim_num,
            fresh_reserved_backing_num: claim_num,
            credit_rate_num: CREDIT_RATE_SCALE,
            ..SourceCreditStateV16::EMPTY
        });
    markets[0].engine.backing_short = BackingBucketV16Account::from_runtime(&BackingBucketV16 {
        market_id: 1,
        fresh_unliened_backing_num: claim_num,
        expiry_slot: 100,
        status: BackingBucketStatusV16::Fresh,
        ..BackingBucketV16::EMPTY
    });

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    PortfolioV16View::new(&account_header)
        .validate_with_market(&market.as_view())
        .expect("read-only validation must accept coherent domain-indexed parked PnL");
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    market
        .deposit_not_atomic(&mut account, 3)
        .expect("later deposit must accept a persisted parked source claim");

    assert_eq!(account.header.capital.get(), 3);
    assert_eq!(account.header.source_domains[0].domain.get(), 1);
    assert_eq!(
        account.header.source_domains[0]
            .source_claim_bound_num
            .get(),
        claim_num
    );
    assert_eq!(
        account.header.source_domains[1],
        PortfolioSourceDomainV16Account::default()
    );
    account.validate_with_market(&market.as_view()).unwrap();
    market.validate_shape().unwrap();
}

#[test]
fn v16_trade_created_parked_source_claim_survives_later_deposit() {
    let (mut header, mut markets) = market_fixture(1, 100);
    let mut long_header = account_fixture(1, 21);
    let mut short_header = account_fixture(1, 22);

    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        let mut long = PortfolioV16ViewMut::new(&mut long_header);
        let mut short = PortfolioV16ViewMut::new(&mut short_header);
        market.deposit_not_atomic(&mut long, 1_000).unwrap();
        market.deposit_not_atomic(&mut short, 1_000).unwrap();
        market
            .execute_trade_with_fee_loss_stale_scoped_not_atomic(
                &mut long,
                &mut short,
                TradeRequestV16 {
                    asset_index: 0,
                    size_q: signed_q(POS_SCALE),
                    exec_price: 100,
                    fee_bps: 0,
                },
            )
            .unwrap();
        market
            .accrue_asset_to_not_atomic(0, 2, 101, 0, true)
            .unwrap();
        market.full_account_refresh_not_atomic(&mut long).unwrap();
    }

    assert!(long_header.pnl.get() > 0);
    assert!(
        long_header
            .source_domains
            .iter()
            .any(|source| source.domain.get() == 1
                && source.source_claim_market_id.get() == 1
                && source.source_claim_bound_num.get() != 0),
        "winner refresh must persist the source-domain claim created by K/F settlement"
    );

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    PortfolioV16View::new(&long_header)
        .validate_with_market(&market.as_view())
        .expect("read-only validation must accept the trade-created parked claim");
    let mut long = PortfolioV16ViewMut::new(&mut long_header);
    market
        .deposit_not_atomic(&mut long, 3)
        .expect("later deposit must accept the persisted trade-created parked claim");

    assert_eq!(long.header.capital.get(), 1_003);
    long.validate_with_market(&market.as_view()).unwrap();
    market.validate_shape().unwrap();
}

#[test]
fn v16_grant_source_positive_pnl_attributes_claims_and_aggregates_in_lockstep() {
    let (mut header, mut markets) = market_fixture(1, 1);
    let mut account_header = account_fixture(1, 31);
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);

    market
        .add_account_source_positive_pnl_not_atomic(&mut account, 0, 25)
        .expect("granting source-attributed positive pnl must succeed in Live");

    assert_eq!(account.header.pnl.get(), 25);
    assert_eq!(
        account.header.source_domains[0]
            .source_claim_bound_num
            .get(),
        25 * BOUND_SCALE
    );
    assert_eq!(market.header.pnl_pos_tot.get(), 25);
    assert_eq!(market.header.pnl_pos_bound_tot_num.get(), 25 * BOUND_SCALE);
    assert_eq!(
        market.header.source_claim_bound_total_num.get(),
        25 * BOUND_SCALE
    );
    // The grant is notional attribution: no quote value moves.
    assert_eq!(market.header.vault.get(), 0);
    assert_eq!(market.header.c_tot.get(), 0);
    assert_eq!(market.validate_shape(), Ok(()));
    assert_eq!(account.validate_with_market(&market.as_view()), Ok(()));

    // Granting in a non-Live market is rejected before any mutation.
    market.header.mode = 1; // Resolved
    market.header.resolved_slot = V16PodU64::new(1);
    let err = market.add_account_source_positive_pnl_not_atomic(&mut account, 0, 1);
    assert_eq!(err, Err(V16Error::LockActive));
    assert_eq!(account.header.pnl.get(), 25);
}

// ROADMAP 3C step 4 — self-classifying crank. The keeper no longer chooses the
// action: build_actionable_summary classifies the account and
// select_progress_witness picks the continuation the auto-crank dispatches.
#[test]
fn v16_auto_crank_classifies_fresh_account_stale_then_refreshes_to_clean() {
    let (mut header, mut markets) = market_fixture(1, 100);
    let mut account_header = account_fixture(1, 21);
    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        let mut account = PortfolioV16ViewMut::new(&mut account_header);
        market.deposit_not_atomic(&mut account, 1_000).unwrap();
    }

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);

    // A fresh (uncertified) account in a Live market is classified stale ONLY.
    let summary = market.build_actionable_summary(&account.as_view()).unwrap();
    assert!(summary.stale, "fresh uncertified account must be stale");
    assert!(
        !summary.b_stale
            && !summary.pending_close
            && !summary.expired_close
            && !summary.liquidatable
            && !summary.recovery_eligible
            && !summary.resolved_winner,
        "no other actionable class on a fresh empty account"
    );

    let obs = [AutoCrankObservationV16 {
        asset_index: 0,
        effective_price: 100,
        funding_rate_e9: 0,
    }];
    let work = AutoCrankWorkV16 {
        now_slot: 5,
        observations: &obs,
        resolved_close_fee_rate_per_slot: 0,
    };

    // The engine selects RefreshAccount (engine-chosen asset) and dispatches it;
    // the account becomes current (real liveness progress, no caller-chosen action).
    let r = market
        .permissionless_auto_crank_not_atomic(&mut account, work)
        .unwrap();
    assert!(matches!(
        r.selected,
        AutoCrankPlanV16::RefreshAccount { .. }
    ));
    assert_eq!(
        r.outcome,
        AutoCrankOutcomeV16::Progressed(PermissionlessProgressOutcomeV16::AccountCurrent)
    );

    // Now certified & clean -> not actionable -> NoAction (terminates).
    let summary2 = market.build_actionable_summary(&account.as_view()).unwrap();
    assert!(
        !summary2.is_actionable(),
        "a refreshed, clean account is not actionable"
    );
    let r2 = market
        .permissionless_auto_crank_not_atomic(&mut account, work)
        .unwrap();
    assert_eq!(r2.selected, AutoCrankPlanV16::NoAction);
    assert_eq!(r2.outcome, AutoCrankOutcomeV16::NoAction);

    market.validate_shape().unwrap();
    account.validate_with_market(&market.as_view()).unwrap();
}

#[test]
fn v16_auto_crank_expires_one_lapsed_live_source_domain_per_step() {
    let (mut header, mut markets) = market_fixture(2, 100);
    let mut account_header = account_fixture(2, 22);
    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        let mut account = PortfolioV16ViewMut::new(&mut account_header);
        market.deposit_not_atomic(&mut account, 100).unwrap();
        market
            .deposit_fresh_counterparty_backing_not_atomic(1, 40, 5)
            .unwrap();
        market
            .deposit_fresh_counterparty_backing_not_atomic(3, 40, 5)
            .unwrap();
        market
            .add_account_source_positive_pnl_not_atomic(&mut account, 1, 40)
            .unwrap();
        market
            .add_account_source_positive_pnl_not_atomic(&mut account, 3, 40)
            .unwrap();
        market
            .accrue_asset_to_not_atomic(0, 10, 100, 0, true)
            .unwrap();
        market
            .accrue_asset_to_not_atomic(1, 10, 100, 0, true)
            .unwrap();
    }

    let before = markets[0].engine.backing_short.try_to_runtime().unwrap();
    assert_eq!(before.status, BackingBucketStatusV16::Fresh);
    assert_eq!(before.expiry_slot, 5);
    assert_eq!(
        markets[1]
            .engine
            .backing_short
            .try_to_runtime()
            .unwrap()
            .status,
        BackingBucketStatusV16::Fresh
    );
    assert!(header.current_slot.get() > before.expiry_slot);
    let vault_before = header.vault.get();
    let c_tot_before = header.c_tot.get();
    let insurance_before = header.insurance.get();
    let earnings_before = header.backing_provider_earnings_total.get();
    let source_backing_before = header.source_fresh_backing_total_num.get();
    let risk_epoch_before = header.risk_epoch.get();

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    let observations = [AutoCrankObservationV16 {
        asset_index: 0,
        effective_price: 100,
        funding_rate_e9: 0,
    }];
    let expiry = market
        .permissionless_auto_crank_not_atomic(
            &mut account,
            AutoCrankWorkV16 {
                now_slot: 10,
                observations: &observations,
                resolved_close_fee_rate_per_slot: 0,
            },
        )
        .expect("Live auto-crank must expire lapsed backing instead of returning Stale");

    assert!(matches!(
        expiry.selected,
        AutoCrankPlanV16::RefreshAccount { .. }
    ));
    assert_eq!(
        expiry.outcome,
        AutoCrankOutcomeV16::Progressed(PermissionlessProgressOutcomeV16::SourceBackingExpired {
            domain: 1
        })
    );
    let after = market.markets[0]
        .engine
        .backing_short
        .try_to_runtime()
        .unwrap();
    assert_eq!(after.status, BackingBucketStatusV16::Expired);
    assert_eq!(after.fresh_unliened_backing_num, 0);
    assert_eq!(
        market.markets[1]
            .engine
            .backing_short
            .try_to_runtime()
            .unwrap()
            .status,
        BackingBucketStatusV16::Fresh,
        "one auto-crank expires exactly one source domain"
    );
    assert_eq!(market.header.vault.get(), vault_before);
    assert_eq!(market.header.c_tot.get(), c_tot_before);
    assert_eq!(market.header.insurance.get(), insurance_before);
    assert_eq!(
        market.header.backing_provider_earnings_total.get(),
        earnings_before
    );
    assert_eq!(
        market.header.source_fresh_backing_total_num.get(),
        source_backing_before - 40 * BOUND_SCALE
    );
    assert_eq!(market.header.risk_epoch.get(), risk_epoch_before + 1);
    assert_eq!(account.header.capital.get(), 100);
    assert_eq!(account.header.pnl.get(), 80);
    assert!(!account.header.health_cert.try_to_runtime().unwrap().valid);

    let second_expiry = market
        .permissionless_auto_crank_not_atomic(
            &mut account,
            AutoCrankWorkV16 {
                now_slot: 10,
                observations: &observations,
                resolved_close_fee_rate_per_slot: 0,
            },
        )
        .expect("the next bounded auto-crank must expire the next domain");
    assert_eq!(
        second_expiry.outcome,
        AutoCrankOutcomeV16::Progressed(PermissionlessProgressOutcomeV16::SourceBackingExpired {
            domain: 3
        })
    );
    assert_eq!(
        market.markets[1]
            .engine
            .backing_short
            .try_to_runtime()
            .unwrap()
            .status,
        BackingBucketStatusV16::Expired
    );
    assert_eq!(market.header.vault.get(), vault_before);
    assert_eq!(market.header.c_tot.get(), c_tot_before);
    assert_eq!(market.header.insurance.get(), insurance_before);
    assert_eq!(
        market.header.backing_provider_earnings_total.get(),
        earnings_before
    );
    assert_eq!(market.header.source_fresh_backing_total_num.get(), 0);
    assert_eq!(market.header.risk_epoch.get(), risk_epoch_before + 2);

    let refresh = market
        .permissionless_auto_crank_not_atomic(
            &mut account,
            AutoCrankWorkV16 {
                now_slot: 10,
                observations: &observations,
                resolved_close_fee_rate_per_slot: 0,
            },
        )
        .expect("the final bounded auto-crank must finish account refresh");
    assert_eq!(
        refresh.outcome,
        AutoCrankOutcomeV16::Progressed(PermissionlessProgressOutcomeV16::AccountCurrent)
    );
    assert!(account.header.health_cert.try_to_runtime().unwrap().valid);
    market.validate_shape().unwrap();
    account.validate_with_market(&market.as_view()).unwrap();
}

#[test]
fn v16_auto_crank_skips_recovery_first_leg_for_live_refresh() {
    let (mut header, mut markets) = market_fixture(2, 100);
    let mut account_header = account_fixture(2, 22);
    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        let mut account = PortfolioV16ViewMut::new(&mut account_header);
        market.deposit_not_atomic(&mut account, 1_000).unwrap();
    }

    header.current_slot = V16PodU64::new(10);
    header.slot_last = V16PodU64::new(10);
    let mut asset0 = markets[0].engine.asset.try_to_runtime().unwrap();
    asset0.lifecycle = AssetLifecycleV16::Recovery;
    asset0.slot_last = 10;
    asset0.oi_eff_long_q = POS_SCALE;
    asset0.oi_eff_short_q = POS_SCALE;
    asset0.loss_weight_sum_long = POS_SCALE;
    asset0.loss_weight_sum_short = POS_SCALE;
    asset0.stored_pos_count_long = 1;
    asset0.stored_pos_count_short = 1;
    markets[0].engine.asset = AssetStateV16Account::from_runtime(&asset0);

    let mut asset1 = markets[1].engine.asset.try_to_runtime().unwrap();
    asset1.slot_last = 10;
    asset1.oi_eff_long_q = POS_SCALE;
    asset1.oi_eff_short_q = POS_SCALE;
    asset1.loss_weight_sum_long = POS_SCALE;
    asset1.loss_weight_sum_short = POS_SCALE;
    asset1.stored_pos_count_long = 1;
    asset1.stored_pos_count_short = 1;
    markets[1].engine.asset = AssetStateV16Account::from_runtime(&asset1);
    header.resolved_payout_blocker_count = V16PodU64::new(4);

    account_header.legs[0] = PortfolioLegV16Account::from_runtime(&PortfolioLegV16 {
        active: true,
        asset_index: 0,
        market_id: asset0.market_id,
        side: SideV16::Long,
        basis_pos_q: POS_SCALE as i128,
        a_basis: ADL_ONE,
        k_snap: asset0.k_long,
        f_snap: asset0.f_long_num,
        epoch_snap: asset0.epoch_long,
        loss_weight: POS_SCALE,
        b_snap: asset0.b_long_num,
        b_rem: 0,
        b_epoch_snap: asset0.epoch_long,
        b_stale: false,
        stale: false,
    });
    account_header.legs[1] = PortfolioLegV16Account::from_runtime(&PortfolioLegV16 {
        active: true,
        asset_index: 1,
        market_id: asset1.market_id,
        side: SideV16::Short,
        basis_pos_q: -(POS_SCALE as i128),
        a_basis: ADL_ONE,
        k_snap: asset1.k_short,
        f_snap: asset1.f_short_num,
        epoch_snap: asset1.epoch_short,
        loss_weight: POS_SCALE,
        b_snap: asset1.b_short_num,
        b_rem: 0,
        b_epoch_snap: asset1.epoch_short,
        b_stale: false,
        stale: false,
    });
    account_header.active_bitmap[0] = V16PodU64::new(3);

    let obs = [AutoCrankObservationV16 {
        asset_index: 1,
        effective_price: 100,
        funding_rate_e9: 0,
    }];
    let work = AutoCrankWorkV16 {
        now_slot: 10,
        observations: &obs,
        resolved_close_fee_rate_per_slot: 0,
    };
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    assert!(
        market
            .build_actionable_summary(&account.as_view())
            .unwrap()
            .stale
    );

    let result = market
        .permissionless_auto_crank_not_atomic(&mut account, work)
        .expect("a Recovery first leg must not block the live asset refresh");
    assert_eq!(
        result.selected,
        AutoCrankPlanV16::RefreshAccount {
            asset_index: Some(1),
        },
    );
    assert_eq!(
        result.outcome,
        AutoCrankOutcomeV16::Progressed(PermissionlessProgressOutcomeV16::AccountCurrent),
    );
    market.validate_shape().unwrap();
    account.validate_with_market(&market.as_view()).unwrap();
}

#[test]
fn v16_auto_crank_skips_prior_reset_obligation_for_live_liquidation() {
    let (mut header, mut markets) = market_fixture(2, 100);
    let mut account_header = account_fixture(2, 23);
    header.current_slot = V16PodU64::new(10);
    header.slot_last = V16PodU64::new(10);

    let mut asset0 = markets[0].engine.asset.try_to_runtime().unwrap();
    asset0.slot_last = 10;
    asset0.epoch_long = 1;
    asset0.mode_long = SideModeV16::ResetPending;
    asset0.oi_eff_long_q = 0;
    asset0.oi_eff_short_q = 0;
    asset0.loss_weight_sum_long = 0;
    asset0.loss_weight_sum_short = 0;
    asset0.stored_pos_count_long = 1;
    markets[0].engine.asset = AssetStateV16Account::from_runtime(&asset0);

    let mut asset1 = markets[1].engine.asset.try_to_runtime().unwrap();
    asset1.slot_last = 10;
    asset1.oi_eff_long_q = 2 * POS_SCALE;
    asset1.oi_eff_short_q = 2 * POS_SCALE;
    asset1.loss_weight_sum_long = 2 * POS_SCALE;
    asset1.loss_weight_sum_short = 2 * POS_SCALE;
    asset1.stored_pos_count_long = 2;
    asset1.stored_pos_count_short = 2;
    markets[1].engine.asset = AssetStateV16Account::from_runtime(&asset1);
    header.resolved_payout_blocker_count = V16PodU64::new(5);

    account_header.legs[0] = PortfolioLegV16Account::from_runtime(&PortfolioLegV16 {
        active: true,
        asset_index: 0,
        market_id: asset0.market_id,
        side: SideV16::Long,
        basis_pos_q: POS_SCALE as i128,
        a_basis: ADL_ONE,
        k_snap: asset0.k_epoch_start_long,
        f_snap: asset0.f_epoch_start_long_num,
        epoch_snap: 0,
        loss_weight: POS_SCALE,
        b_snap: asset0.b_epoch_start_long_num,
        b_rem: 0,
        b_epoch_snap: 0,
        b_stale: false,
        stale: false,
    });
    account_header.legs[1] = PortfolioLegV16Account::from_runtime(&PortfolioLegV16 {
        active: true,
        asset_index: 1,
        market_id: asset1.market_id,
        side: SideV16::Short,
        basis_pos_q: -(POS_SCALE as i128),
        a_basis: ADL_ONE,
        k_snap: asset1.k_short,
        f_snap: asset1.f_short_num,
        epoch_snap: asset1.epoch_short,
        loss_weight: POS_SCALE,
        b_snap: asset1.b_short_num,
        b_rem: 0,
        b_epoch_snap: asset1.epoch_short,
        b_stale: false,
        stale: false,
    });
    account_header.active_bitmap[0] = V16PodU64::new(3);
    account_header.health_cert = HealthCertV16Account::from_runtime(&HealthCertV16 {
        certified_equity: 0,
        certified_initial_req: 2,
        certified_maintenance_req: 2,
        certified_liq_deficit: 2,
        certified_worst_case_loss: 200,
        cert_oracle_epoch: header.oracle_epoch.get(),
        cert_funding_epoch: header.funding_epoch.get(),
        cert_risk_epoch: header.risk_epoch.get(),
        cert_asset_set_epoch: header.asset_set_epoch.get(),
        active_bitmap_at_cert: [3],
        valid: true,
    });

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    let refresh = market
        .permissionless_auto_crank_not_atomic(
            &mut account,
            AutoCrankWorkV16 {
                now_slot: 10,
                observations: &[],
                resolved_close_fee_rate_per_slot: 0,
            },
        )
        .expect("the prior-reset first leg must be detached permissionlessly");

    assert_eq!(
        refresh.selected,
        AutoCrankPlanV16::RefreshAccount {
            asset_index: Some(0)
        }
    );
    assert!(!account.header.legs[0].try_to_runtime().unwrap().active);
    assert!(account.header.legs[1].try_to_runtime().unwrap().active);

    let liquidation = market
        .permissionless_auto_crank_not_atomic(
            &mut account,
            AutoCrankWorkV16 {
                now_slot: 10,
                observations: &[],
                resolved_close_fee_rate_per_slot: 0,
            },
        )
        .expect("the next step must liquidate the remaining live asset");
    assert_eq!(
        liquidation.selected,
        AutoCrankPlanV16::Liquidate { asset_index: 1 }
    );
    assert!(!account.header.legs[1].try_to_runtime().unwrap().active);
    market.validate_shape().unwrap();
    account.validate_with_market(&market.as_view()).unwrap();
}

#[test]
fn v16_adl_reduced_basis_caps_exit_to_effective_oi_then_detaches_residue() {
    let (mut header, mut markets) = market_fixture(1, 100);
    let mut account_header = account_fixture(1, 24);

    // A partial ADL can leave a winner's stored basis larger than the side's
    // remaining effective OI. This is the exact state reached by the public
    // wrapper regression: basis=2 lots, matched effective OI=1 lot.
    let mut asset = markets[0].engine.asset.try_to_runtime().unwrap();
    asset.oi_eff_long_q = POS_SCALE;
    asset.oi_eff_short_q = POS_SCALE;
    asset.a_long = ADL_ONE / 2;
    asset.loss_weight_sum_long = 2 * POS_SCALE;
    asset.loss_weight_sum_short = POS_SCALE;
    asset.stored_pos_count_long = 1;
    asset.stored_pos_count_short = 1;
    markets[0].engine.asset = AssetStateV16Account::from_runtime(&asset);
    header.resolved_payout_blocker_count = V16PodU64::new(2);

    account_header.legs[0] = PortfolioLegV16Account::from_runtime(&PortfolioLegV16 {
        active: true,
        asset_index: 0,
        market_id: asset.market_id,
        side: SideV16::Long,
        basis_pos_q: (2 * POS_SCALE) as i128,
        a_basis: ADL_ONE,
        k_snap: asset.k_long,
        f_snap: asset.f_long_num,
        epoch_snap: asset.epoch_long,
        loss_weight: 2 * POS_SCALE,
        b_snap: asset.b_long_num,
        b_rem: 0,
        b_epoch_snap: asset.epoch_long,
        b_stale: false,
        stale: false,
    });
    account_header.active_bitmap[0] = V16PodU64::new(1);

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    market.deposit_not_atomic(&mut account, 1_000).unwrap();

    let reduced = market
        .rebalance_reduce_position_not_atomic(
            &mut account,
            RebalanceRequestV16 {
                asset_index: 0,
                reduce_q: 2 * POS_SCALE,
            },
        )
        .expect("max-work exit must clamp to matched effective OI");
    assert_eq!(reduced.reduced_q, POS_SCALE);
    let residue = account.header.legs[0].try_to_runtime().unwrap();
    assert_eq!(residue.basis_pos_q, POS_SCALE as i128);
    let reset = market.markets[0].engine.asset.try_to_runtime().unwrap();
    assert_eq!(reset.oi_eff_long_q, 0);
    assert_eq!(reset.oi_eff_short_q, 0);
    assert_eq!(reset.mode_long, SideModeV16::ResetPending);
    assert_eq!(reset.mode_short, SideModeV16::ResetPending);

    let cleanup = market
        .permissionless_auto_crank_not_atomic(
            &mut account,
            AutoCrankWorkV16 {
                now_slot: 1,
                observations: &[],
                resolved_close_fee_rate_per_slot: 0,
            },
        )
        .expect("terminal ADL residue must be permissionlessly detachable");
    assert_eq!(
        cleanup.selected,
        AutoCrankPlanV16::RefreshAccount {
            asset_index: Some(0)
        }
    );
    assert!(!account.header.legs[0].try_to_runtime().unwrap().active);
    assert_eq!(account.header.active_bitmap[0].get(), 0);
    market.validate_shape().unwrap();
    account.validate_with_market(&market.as_view()).unwrap();
}

#[test]
fn v16_auto_crank_migrates_legacy_normal_adl_residue_into_reset_cleanup() {
    let (mut header, mut markets) = market_fixture(1, 100);
    let mut account_header = account_fixture(1, 25);
    let mut asset = markets[0].engine.asset.try_to_runtime().unwrap();
    asset.oi_eff_long_q = 0;
    asset.oi_eff_short_q = 0;
    asset.a_long = ADL_ONE / 2;
    asset.loss_weight_sum_long = POS_SCALE;
    asset.stored_pos_count_long = 1;
    markets[0].engine.asset = AssetStateV16Account::from_runtime(&asset);
    header.resolved_payout_blocker_count = V16PodU64::new(1);
    account_header.legs[0] = PortfolioLegV16Account::from_runtime(&PortfolioLegV16 {
        active: true,
        asset_index: 0,
        market_id: asset.market_id,
        side: SideV16::Long,
        basis_pos_q: POS_SCALE as i128,
        a_basis: ADL_ONE,
        k_snap: asset.k_long,
        f_snap: asset.f_long_num,
        epoch_snap: asset.epoch_long,
        loss_weight: POS_SCALE,
        b_snap: asset.b_long_num,
        b_rem: 0,
        b_epoch_snap: asset.epoch_long,
        b_stale: false,
        stale: false,
    });
    account_header.active_bitmap[0] = V16PodU64::new(1);

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    market.deposit_not_atomic(&mut account, 1_000).unwrap();
    let result = market
        .permissionless_auto_crank_not_atomic(
            &mut account,
            AutoCrankWorkV16 {
                now_slot: 1,
                observations: &[],
                resolved_close_fee_rate_per_slot: 0,
            },
        )
        .expect("a legacy zero-effective-OI residue must remain crankable after upgrade");
    assert_eq!(
        result.selected,
        AutoCrankPlanV16::RefreshAccount {
            asset_index: Some(0)
        }
    );
    assert_eq!(account.header.active_bitmap[0].get(), 0);
    assert!(!account.header.legs[0].try_to_runtime().unwrap().active);
    let reset = market.markets[0].engine.asset.try_to_runtime().unwrap();
    assert_eq!(reset.mode_long, SideModeV16::ResetPending);
    assert_eq!(reset.stored_pos_count_long, 0);
    market.validate_shape().unwrap();
    account.validate_with_market(&market.as_view()).unwrap();
}

#[test]
fn v16_resolved_close_migrates_legacy_normal_adl_residue_before_detach() {
    let (mut header, mut markets) = market_fixture(1, 100);
    let mut account_header = account_fixture(1, 26);
    let mut asset = markets[0].engine.asset.try_to_runtime().unwrap();
    asset.oi_eff_long_q = 0;
    asset.oi_eff_short_q = 0;
    asset.a_long = ADL_ONE / 2;
    asset.loss_weight_sum_long = POS_SCALE;
    asset.stored_pos_count_long = 1;
    markets[0].engine.asset = AssetStateV16Account::from_runtime(&asset);
    header.resolved_payout_blocker_count = V16PodU64::new(1);
    account_header.legs[0] = PortfolioLegV16Account::from_runtime(&PortfolioLegV16 {
        active: true,
        asset_index: 0,
        market_id: asset.market_id,
        side: SideV16::Long,
        basis_pos_q: POS_SCALE as i128,
        a_basis: ADL_ONE,
        k_snap: asset.k_long,
        f_snap: asset.f_long_num,
        epoch_snap: asset.epoch_long,
        loss_weight: POS_SCALE,
        b_snap: asset.b_long_num,
        b_rem: 0,
        b_epoch_snap: asset.epoch_long,
        b_stale: false,
        stale: false,
    });
    account_header.active_bitmap[0] = V16PodU64::new(1);

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    market.deposit_not_atomic(&mut account, 1_000).unwrap();
    market.resolve_market_not_atomic(1).unwrap();
    let outcome = market
        .close_resolved_account_not_atomic(&mut account, 0)
        .expect("resolution must not strand an upgraded zero-effective-OI residue");
    assert_eq!(
        outcome,
        percolator::ResolvedCloseOutcomeV16::Closed { payout: 1_000 }
    );
    assert_eq!(account.header.active_bitmap[0].get(), 0);
    assert_eq!(account.header.capital.get(), 0);
    assert_eq!(market.header.vault.get(), 0);
    let reset = market.markets[0].engine.asset.try_to_runtime().unwrap();
    assert_eq!(reset.mode_long, SideModeV16::ResetPending);
    assert_eq!(reset.stored_pos_count_long, 0);
    market.validate_shape().unwrap();
    account.validate_with_market(&market.as_view()).unwrap();
}

#[test]
fn v16_recovery_forfeit_migrates_legacy_normal_adl_residue_before_detach() {
    let (mut header, mut markets) = market_fixture(1, 100);
    let mut account_header = account_fixture(1, 27);
    let mut asset = markets[0].engine.asset.try_to_runtime().unwrap();
    asset.oi_eff_long_q = 0;
    asset.oi_eff_short_q = 0;
    asset.a_long = ADL_ONE / 2;
    asset.loss_weight_sum_long = POS_SCALE;
    asset.stored_pos_count_long = 1;
    markets[0].engine.asset = AssetStateV16Account::from_runtime(&asset);
    header.resolved_payout_blocker_count = V16PodU64::new(1);
    account_header.legs[0] = PortfolioLegV16Account::from_runtime(&PortfolioLegV16 {
        active: true,
        asset_index: 0,
        market_id: asset.market_id,
        side: SideV16::Long,
        basis_pos_q: POS_SCALE as i128,
        a_basis: ADL_ONE,
        k_snap: asset.k_long,
        f_snap: asset.f_long_num,
        epoch_snap: asset.epoch_long,
        loss_weight: POS_SCALE,
        b_snap: asset.b_long_num,
        b_rem: 0,
        b_epoch_snap: asset.epoch_long,
        b_stale: false,
        stale: false,
    });
    account_header.active_bitmap[0] = V16PodU64::new(1);

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    market.deposit_not_atomic(&mut account, 1_000).unwrap();
    market.force_asset_recovery_not_atomic(0, 1).unwrap();
    let vault_before = market.header.vault.get();
    let c_tot_before = market.header.c_tot.get();
    let insurance_before = market.header.insurance.get();
    let capital_before = account.header.capital.get();
    let pnl_before = account.header.pnl.get();
    let outcome = market
        .forfeit_recovery_leg_not_atomic(&mut account, 0, POS_SCALE)
        .expect("Recovery forfeit must detach a zero-effective-OI ADL residue");
    assert!(outcome.detached);
    assert_eq!(account.header.active_bitmap[0].get(), 0);
    let reset = market.markets[0].engine.asset.try_to_runtime().unwrap();
    assert_eq!(reset.mode_long, SideModeV16::ResetPending);
    assert_eq!(reset.stored_pos_count_long, 0);
    assert_eq!(market.header.vault.get(), vault_before);
    assert_eq!(market.header.c_tot.get(), c_tot_before);
    assert_eq!(market.header.insurance.get(), insurance_before);
    assert_eq!(account.header.capital.get(), capital_before);
    assert_eq!(account.header.pnl.get(), pnl_before);
    market.validate_shape().unwrap();
    account.validate_with_market(&market.as_view()).unwrap();
}

// ROADMAP 3C step 4 / NB2 finite-multi-step liveness via the self-classifying
// crank: an uncertified, underwater account must be driven to a de-risked fixed
// point by repeated auto-cranks — the classifier ESCALATES (stale -> refresh,
// then liquidatable -> liquidate) and TERMINATES (NoAction), with no caller-
// chosen action. Same liquidatable shape the direct-liquidation test uses.
#[test]
fn v16_auto_crank_drives_stale_underwater_account_to_derisked_fixed_point() {
    let (mut header, mut markets) = market_fixture(2, 100);
    let mut account_header = account_fixture(2, 13);
    header.current_slot = V16PodU64::new(10);
    header.slot_last = V16PodU64::new(9);
    header.loss_stale_active = 1;
    header.vault = V16PodU128::new(50);
    header.insurance = V16PodU128::new(50);
    header.negative_pnl_account_count = V16PodU64::new(1);

    let mut asset0 = markets[0].engine.asset.try_to_runtime().unwrap();
    asset0.slot_last = 10;
    asset0.oi_eff_long_q = 2 * POS_SCALE;
    asset0.oi_eff_short_q = 2 * POS_SCALE;
    asset0.loss_weight_sum_long = 2 * POS_SCALE;
    asset0.loss_weight_sum_short = 2 * POS_SCALE;
    asset0.stored_pos_count_long = 2;
    asset0.stored_pos_count_short = 2;
    markets[0].engine.asset = AssetStateV16Account::from_runtime(&asset0);
    let mut asset1 = markets[1].engine.asset.try_to_runtime().unwrap();
    asset1.slot_last = 9;
    asset1.oi_eff_long_q = POS_SCALE;
    asset1.oi_eff_short_q = POS_SCALE;
    asset1.loss_weight_sum_long = POS_SCALE;
    asset1.loss_weight_sum_short = POS_SCALE;
    asset1.stored_pos_count_long = 1;
    asset1.stored_pos_count_short = 1;
    markets[1].engine.asset = AssetStateV16Account::from_runtime(&asset1);
    header.resolved_payout_blocker_count = V16PodU64::new(6);

    account_header.pnl = V16PodI128::new(-5);
    account_header.legs[0] = PortfolioLegV16Account::from_runtime(&PortfolioLegV16 {
        active: true,
        asset_index: 0,
        market_id: asset0.market_id,
        side: SideV16::Long,
        basis_pos_q: POS_SCALE as i128,
        a_basis: ADL_ONE,
        k_snap: asset0.k_long,
        f_snap: asset0.f_long_num,
        epoch_snap: asset0.epoch_long,
        loss_weight: POS_SCALE,
        b_snap: asset0.b_long_num,
        b_rem: 0,
        b_epoch_snap: asset0.epoch_long,
        b_stale: false,
        stale: false,
    });
    account_header.active_bitmap[0] = V16PodU64::new(1);

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);

    let obs = [AutoCrankObservationV16 {
        asset_index: 0,
        effective_price: 100,
        funding_rate_e9: 0,
    }];
    let work = AutoCrankWorkV16 {
        now_slot: 10,
        observations: &obs,
        resolved_close_fee_rate_per_slot: 0,
    };

    // Drive the engine auto-crank to a fixed point. It MUST converge (no-DoS)
    // within a bounded number of steps, self-selecting the asset each step.
    let mut plans = Vec::new();
    let mut saw_refresh = false;
    let mut saw_liquidate = false;
    let mut steps = 0;
    loop {
        let summary = market.build_actionable_summary(&account.as_view()).unwrap();
        let r = match market.permissionless_auto_crank_not_atomic(&mut account, work) {
            Ok(r) => r,
            Err(e) => panic!(
                "step {steps} dispatch err {e:?}; summary={summary:?}; plans={plans:?}; bitmap={}",
                account.header.active_bitmap[0].get()
            ),
        };
        match r.selected {
            AutoCrankPlanV16::NoAction => break,
            AutoCrankPlanV16::RefreshAccount { .. } => saw_refresh = true,
            AutoCrankPlanV16::Liquidate { .. } => saw_liquidate = true,
            _ => {}
        }
        plans.push(r.selected);
        steps += 1;
        assert!(
            steps < 12,
            "engine auto-crank must converge (no-DoS); selected so far: {:?}",
            plans
        );
    }

    // The engine escalated: it refreshed the stale account, then liquidated
    // the underwater position — and reached a non-actionable fixed point.
    assert!(
        saw_refresh,
        "must refresh the uncertified account: {:?}",
        plans
    );
    assert!(
        saw_liquidate,
        "must liquidate the underwater position: {:?}",
        plans
    );
    assert_eq!(
        account.header.active_bitmap[0].get(),
        0,
        "position must be liquidated at the fixed point"
    );

    market.validate_shape().unwrap();
    account.validate_with_market(&market.as_view()).unwrap();
}

#[test]
fn v16_trade_final_leg_residual_routes_through_close_without_forcing_market_recovery() {
    const SIZE_Q: u128 = 10 * POS_SCALE;
    let (mut header, mut markets) = market_fixture(1, 100);
    header.config.maintenance_margin_bps = V16PodU64::new(1_000);
    header.config.initial_margin_bps = V16PodU64::new(1_000);
    header.config.max_price_move_bps_per_slot = V16PodU64::new(500);
    header.config.max_accrual_dt_slots = V16PodU64::new(1);
    header.config.min_funding_lifetime_slots = V16PodU64::new(1);
    let mut long_header = account_fixture(1, 61);
    let mut short_header = account_fixture(1, 62);

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut long = PortfolioV16ViewMut::new(&mut long_header);
    let mut short = PortfolioV16ViewMut::new(&mut short_header);
    market.deposit_not_atomic(&mut long, 1_000).unwrap();
    market.deposit_not_atomic(&mut short, 250).unwrap();
    market
        .execute_trade_with_fee_loss_stale_scoped_not_atomic(
            &mut long,
            &mut short,
            TradeRequestV16 {
                asset_index: 0,
                size_q: signed_q(SIZE_Q),
                exec_price: 100,
                fee_bps: 0,
            },
        )
        .unwrap();
    for (offset, price) in (105u64..=150).step_by(5).enumerate() {
        let slot = 2 + offset as u64;
        market
            .set_asset_raw_oracle_target_not_atomic(0, price)
            .unwrap();
        market
            .accrue_asset_to_not_atomic(0, slot, price, 0, true)
            .unwrap();
    }

    market
        .execute_trade_with_fee_loss_stale_scoped_not_atomic(
            &mut long,
            &mut short,
            TradeRequestV16 {
                asset_index: 0,
                size_q: -signed_q(SIZE_Q),
                exec_price: 150,
                fee_bps: 0,
            },
        )
        .expect("risk-reducing final trade must remain available");

    let pending = short.header.close_progress.try_to_runtime().unwrap();
    assert!(active_bitmap_is_empty(
        short.header.active_bitmap.map(V16PodU64::get)
    ));
    assert_eq!(short.header.capital.get(), 0);
    assert_eq!(short.header.pnl.get(), -250);
    assert!(pending.active && !pending.finalized && pending.residual_remaining != 0);
    assert_eq!(pending.asset_index, 0);
    assert_eq!(pending.domain_side, SideV16::Long);
    assert_eq!(pending.residual_remaining, 250);
    assert!(
        market
            .build_actionable_summary(&short.as_view())
            .unwrap()
            .pending_close
    );

    let work = AutoCrankWorkV16 {
        now_slot: 11,
        observations: &[],
        resolved_close_fee_rate_per_slot: 0,
    };
    let booked = market
        .permissionless_auto_crank_not_atomic(&mut short, work)
        .expect("pending close must have a committed-state continuation");
    assert_eq!(booked.selected, AutoCrankPlanV16::AdvanceClose);
    assert!(matches!(
        booked.outcome,
        AutoCrankOutcomeV16::Progressed(PermissionlessProgressOutcomeV16::ResidualBooked(_))
    ));
    assert_eq!(short.header.pnl.get(), 0);
    assert_eq!(market.header.negative_pnl_account_count.get(), 0);
    let finalized = short.header.close_progress.try_to_runtime().unwrap();
    assert!(finalized.active && finalized.finalized && finalized.residual_remaining == 0);

    let observations = [AutoCrankObservationV16 {
        asset_index: 0,
        effective_price: 150,
        funding_rate_e9: 0,
    }];
    let normalized = market
        .permissionless_auto_crank_not_atomic(
            &mut short,
            AutoCrankWorkV16 {
                now_slot: work.now_slot,
                observations: &observations,
                resolved_close_fee_rate_per_slot: 0,
            },
        )
        .expect("the completed close account must normalize its stale certificate");
    assert_eq!(
        normalized.selected,
        AutoCrankPlanV16::RefreshAccount { asset_index: None }
    );

    let no_forced_recovery = market
        .permissionless_auto_crank_not_atomic(&mut short, work)
        .expect("a completed account close is not market-wide recovery authority");
    assert_eq!(
        no_forced_recovery.selected,
        AutoCrankPlanV16::NoAction,
        "completed residual work must not terminate unrelated market activity"
    );
    assert_eq!(market.header.mode, 0);

    market
        .resolve_market_not_atomic(work.now_slot)
        .expect("an explicit market-level transition can start terminal settlement");
    assert_eq!(market.header.mode, 1);

    let loser_close = market
        .close_resolved_account_not_atomic(&mut short, 0)
        .expect("the flat bankrupt account must close in Resolved mode");
    assert!(matches!(
        loser_close,
        percolator::ResolvedCloseOutcomeV16::Closed { payout: 0 }
    ));
    let mut winner_closed = false;
    let mut last_winner_close = None;
    for _ in 0..8 {
        let close = market
            .close_resolved_account_not_atomic(&mut long, 0)
            .expect("the loss-side obligation must make bounded resolved progress");
        last_winner_close = Some(close);
        if matches!(close, percolator::ResolvedCloseOutcomeV16::Closed { .. }) {
            winner_closed = true;
            break;
        }
    }
    assert!(
        winner_closed,
        "the winner must reach terminal payout finitely: last={last_winner_close:?}, \
         blockers={}, b_stale={}, stale={}, pnl={}, capital={}, bitmap={:?}, asset={:?}",
        market.header.resolved_payout_blocker_count.get(),
        long.header.b_stale_state,
        long.header.stale_state,
        long.header.pnl.get(),
        long.header.capital.get(),
        long.header.active_bitmap,
        market.markets[0].engine.asset.try_to_runtime().unwrap(),
    );
    assert!(active_bitmap_is_empty(
        long.header.active_bitmap.map(V16PodU64::get)
    ));
    assert_eq!(long.header.capital.get(), 0);
    assert_eq!(long.header.pnl.get(), 0);
    market.validate_shape().unwrap();
    long.validate_with_market(&market.as_view()).unwrap();
    short.validate_with_market(&market.as_view()).unwrap();
}

#[test]
fn v16_batch_trade_starts_terminal_residual_only_at_final_fill() {
    const HALF_Q: u128 = 5 * POS_SCALE;
    let (mut header, mut markets) = market_fixture(2, 100);
    header.config.maintenance_margin_bps = V16PodU64::new(1_000);
    header.config.initial_margin_bps = V16PodU64::new(1_000);
    header.config.max_price_move_bps_per_slot = V16PodU64::new(500);
    header.config.max_accrual_dt_slots = V16PodU64::new(1);
    header.config.min_funding_lifetime_slots = V16PodU64::new(1);
    let mut long_header = account_fixture(2, 63);
    let mut short_header = account_fixture(2, 64);

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut long = PortfolioV16ViewMut::new(&mut long_header);
    let mut short = PortfolioV16ViewMut::new(&mut short_header);
    market.deposit_not_atomic(&mut long, 1_000).unwrap();
    market.deposit_not_atomic(&mut short, 250).unwrap();
    market
        .execute_trade_with_fee_loss_stale_scoped_not_atomic(
            &mut long,
            &mut short,
            TradeRequestV16 {
                asset_index: 0,
                size_q: signed_q(2 * HALF_Q),
                exec_price: 100,
                fee_bps: 0,
            },
        )
        .unwrap();
    for (offset, price) in (105u64..=150).step_by(5).enumerate() {
        let slot = 2 + offset as u64;
        market
            .set_asset_raw_oracle_target_not_atomic(0, price)
            .unwrap();
        market
            .accrue_asset_to_not_atomic(0, slot, price, 0, true)
            .unwrap();
    }

    let outcome = market
        .execute_batch_with_fee_loss_stale_scoped_not_atomic(
            &mut long,
            &mut short,
            &[
                TradeRequestV16 {
                    asset_index: 0,
                    size_q: -signed_q(HALF_Q),
                    exec_price: 150,
                    fee_bps: 0,
                },
                TradeRequestV16 {
                    asset_index: 0,
                    size_q: -signed_q(HALF_Q),
                    exec_price: 150,
                    fee_bps: 0,
                },
            ],
        )
        .expect("an intermediate partial close must not lock the final fill");

    assert_eq!(outcome.fill_count, 2);
    let pending = short.header.close_progress.try_to_runtime().unwrap();
    assert_eq!(pending.close_id, 1);
    assert_eq!(pending.gross_loss_at_close_start, 250);
    assert_eq!(pending.residual_remaining, 250);
    assert!(active_bitmap_is_empty(
        short.header.active_bitmap.map(V16PodU64::get)
    ));
    market.validate_shape().unwrap();
    long.validate_with_market(&market.as_view()).unwrap();
    short.validate_with_market(&market.as_view()).unwrap();
}

#[test]
fn v16_trade_does_not_charge_prior_multi_asset_deficit_or_force_market_recovery() {
    const SIZE_Q: u128 = 10 * POS_SCALE;
    let (mut header, mut markets) = market_fixture(2, 100);
    header.config.maintenance_margin_bps = V16PodU64::new(1_000);
    header.config.initial_margin_bps = V16PodU64::new(1_000);
    header.config.max_price_move_bps_per_slot = V16PodU64::new(500);
    header.config.max_accrual_dt_slots = V16PodU64::new(1);
    header.config.min_funding_lifetime_slots = V16PodU64::new(1);
    let mut long_header = account_fixture(2, 65);
    let mut short_header = account_fixture(2, 66);

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut long = PortfolioV16ViewMut::new(&mut long_header);
    let mut short = PortfolioV16ViewMut::new(&mut short_header);
    market.deposit_not_atomic(&mut long, 2_000).unwrap();
    market.deposit_not_atomic(&mut short, 250).unwrap();
    for asset_index in 0..2 {
        market
            .execute_trade_with_fee_loss_stale_scoped_not_atomic(
                &mut long,
                &mut short,
                TradeRequestV16 {
                    asset_index,
                    size_q: signed_q(SIZE_Q),
                    exec_price: 100,
                    fee_bps: 0,
                },
            )
            .unwrap();
    }
    for (offset, price) in (105u64..=150).step_by(5).enumerate() {
        let slot = 2 + offset as u64;
        market
            .set_asset_raw_oracle_target_not_atomic(0, price)
            .unwrap();
        market
            .accrue_asset_to_not_atomic(0, slot, price, 0, true)
            .unwrap();
    }

    market
        .execute_trade_with_fee_loss_stale_scoped_not_atomic(
            &mut long,
            &mut short,
            TradeRequestV16 {
                asset_index: 0,
                size_q: -signed_q(SIZE_Q),
                exec_price: 150,
                fee_bps: 0,
            },
        )
        .expect("the first risk-reducing close must remain available");
    assert_eq!(short.header.pnl.get(), -250);
    assert_eq!(
        short
            .header
            .close_progress
            .try_to_runtime()
            .unwrap()
            .residual_remaining,
        0
    );

    market
        .execute_trade_with_fee_loss_stale_scoped_not_atomic(
            &mut long,
            &mut short,
            TradeRequestV16 {
                asset_index: 1,
                size_q: -signed_q(SIZE_Q),
                exec_price: 100,
                fee_bps: 0,
            },
        )
        .expect("the final risk-reducing close must remain available");
    assert!(active_bitmap_is_empty(
        short.header.active_bitmap.map(V16PodU64::get)
    ));
    let ledger = short.header.close_progress.try_to_runtime().unwrap();
    assert_eq!(ledger.residual_remaining, 0);
    assert!(
        !market
            .build_actionable_summary(&short.as_view())
            .unwrap()
            .recovery_eligible,
        "an unattributed account must not gain authority to recover the whole market"
    );

    assert_eq!(market.header.mode, 0);
    market
        .resolve_market_not_atomic(market.header.current_slot.get())
        .expect("explicit market resolution handles unattributed terminal debt");
    let loser_close = market
        .close_resolved_account_not_atomic(&mut short, 0)
        .expect("resolved settlement clears unattributed negative PnL without domain guessing");
    assert!(matches!(
        loser_close,
        percolator::ResolvedCloseOutcomeV16::Closed { payout: 0 }
    ));
    assert_eq!(short.header.pnl.get(), 0);
    market.validate_shape().unwrap();
    long.validate_with_market(&market.as_view()).unwrap();
    short.validate_with_market(&market.as_view()).unwrap();
}

#[test]
fn v16_auto_crank_liquidates_current_account_without_observation() {
    let (mut header, mut markets) = market_fixture(1, 100);
    let mut account_header = account_fixture(1, 14);
    header.current_slot = V16PodU64::new(10);
    header.slot_last = V16PodU64::new(10);
    header.vault = V16PodU128::new(50);
    header.insurance = V16PodU128::new(50);
    header.negative_pnl_account_count = V16PodU64::new(1);

    let mut asset = markets[0].engine.asset.try_to_runtime().unwrap();
    asset.slot_last = 10;
    asset.oi_eff_long_q = 2 * POS_SCALE;
    asset.oi_eff_short_q = 2 * POS_SCALE;
    asset.loss_weight_sum_long = 2 * POS_SCALE;
    asset.loss_weight_sum_short = 2 * POS_SCALE;
    asset.stored_pos_count_long = 2;
    asset.stored_pos_count_short = 2;
    markets[0].engine.asset = AssetStateV16Account::from_runtime(&asset);
    header.resolved_payout_blocker_count = V16PodU64::new(4);

    account_header.pnl = V16PodI128::new(-5);
    account_header.legs[0] = PortfolioLegV16Account::from_runtime(&PortfolioLegV16 {
        active: true,
        asset_index: 0,
        market_id: asset.market_id,
        side: SideV16::Long,
        basis_pos_q: POS_SCALE as i128,
        a_basis: ADL_ONE,
        k_snap: asset.k_long,
        f_snap: asset.f_long_num,
        epoch_snap: asset.epoch_long,
        loss_weight: POS_SCALE,
        b_snap: asset.b_long_num,
        b_rem: 0,
        b_epoch_snap: asset.epoch_long,
        b_stale: false,
        stale: false,
    });
    account_header.active_bitmap[0] = V16PodU64::new(1);

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    market
        .full_account_refresh_not_atomic(&mut account)
        .expect("setup must produce a current liquidation cert");
    let summary = market.build_actionable_summary(&account.as_view()).unwrap();
    assert!(
        summary.liquidatable && !summary.stale && !summary.b_stale,
        "setup must be current and liquidatable: {summary:?}"
    );

    let work = AutoCrankWorkV16 {
        now_slot: 10,
        observations: &[],
        resolved_close_fee_rate_per_slot: 0,
    };
    let result = market
        .permissionless_auto_crank_not_atomic(&mut account, work)
        .expect("current liquidation must not require a fresh observation");

    assert_eq!(
        result.selected,
        AutoCrankPlanV16::Liquidate { asset_index: 0 }
    );
    assert!(matches!(
        result.outcome,
        AutoCrankOutcomeV16::Progressed(PermissionlessProgressOutcomeV16::AccountCurrent)
    ));
    assert_eq!(
        account.header.active_bitmap[0].get(),
        0,
        "liquidation must close the selected position"
    );
    market.validate_shape().unwrap();
    account.validate_with_market(&market.as_view()).unwrap();
}

#[test]
fn v16_auto_crank_commits_recovery_for_uncovered_cross_margin_liquidation() {
    let (mut header, mut markets) = market_fixture(2, 100);
    let mut account_header = account_fixture(2, 15);
    header.current_slot = V16PodU64::new(10);
    header.slot_last = V16PodU64::new(10);
    header.vault = V16PodU128::new(50);
    header.insurance = V16PodU128::new(50);
    header.negative_pnl_account_count = V16PodU64::new(1);

    for (asset_index, market_slot) in markets.iter_mut().enumerate() {
        let mut asset = market_slot.engine.asset.try_to_runtime().unwrap();
        asset.slot_last = 10;
        asset.oi_eff_long_q = 2 * POS_SCALE;
        asset.oi_eff_short_q = 2 * POS_SCALE;
        asset.loss_weight_sum_long = 2 * POS_SCALE;
        asset.loss_weight_sum_short = 2 * POS_SCALE;
        asset.stored_pos_count_long = 2;
        asset.stored_pos_count_short = 2;
        market_slot.engine.asset = AssetStateV16Account::from_runtime(&asset);
        account_header.legs[asset_index] = PortfolioLegV16Account::from_runtime(&PortfolioLegV16 {
            active: true,
            asset_index: asset_index as u32,
            market_id: asset.market_id,
            side: SideV16::Long,
            basis_pos_q: POS_SCALE as i128,
            a_basis: ADL_ONE,
            k_snap: asset.k_long,
            f_snap: asset.f_long_num,
            epoch_snap: asset.epoch_long,
            loss_weight: POS_SCALE,
            b_snap: asset.b_long_num,
            b_rem: 0,
            b_epoch_snap: asset.epoch_long,
            b_stale: false,
            stale: false,
        });
    }
    header.resolved_payout_blocker_count = V16PodU64::new(8);
    account_header.active_bitmap[0] = V16PodU64::new(0b11);
    account_header.pnl = V16PodI128::new(-5);

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    market
        .full_account_refresh_not_atomic(&mut account)
        .expect("setup must produce a current cross-margin liquidation cert");
    let summary = market.build_actionable_summary(&account.as_view()).unwrap();
    assert!(summary.liquidatable && !summary.recovery_eligible);

    let bitmap_before = account.header.active_bitmap;
    let pnl_before = account.header.pnl;
    let capital_before = account.header.capital;
    let vault_before = market.header.vault;
    let c_tot_before = market.header.c_tot;
    let insurance_before = market.header.insurance;
    let result = market
        .permissionless_auto_crank_not_atomic(
            &mut account,
            AutoCrankWorkV16 {
                now_slot: 10,
                observations: &[],
                resolved_close_fee_rate_per_slot: 0,
            },
        )
        .expect("recovery-required liquidation must be successful crank progress");

    assert_eq!(
        result.selected,
        AutoCrankPlanV16::Liquidate { asset_index: 0 }
    );
    assert_eq!(
        result.outcome,
        AutoCrankOutcomeV16::Progressed(PermissionlessProgressOutcomeV16::RecoveryDeclared(
            PermissionlessRecoveryReasonV16::ActiveBankruptCloseCannotProgress,
        ))
    );
    assert_eq!(market.header.mode, 2, "market must commit Recovery mode");
    assert_eq!(account.header.active_bitmap, bitmap_before);
    assert_eq!(account.header.pnl, pnl_before);
    assert_eq!(account.header.capital, capital_before);
    assert_eq!(market.header.vault, vault_before);
    assert_eq!(market.header.c_tot, c_tot_before);
    assert_eq!(market.header.insurance, insurance_before);
    market.validate_shape().unwrap();
    account.validate_with_market(&market.as_view()).unwrap();

    let recovery_reason_before = market.header.recovery_reason;
    let finalized = market
        .permissionless_auto_crank_not_atomic(
            &mut account,
            AutoCrankWorkV16 {
                now_slot: 10,
                observations: &[],
                resolved_close_fee_rate_per_slot: 0,
            },
        )
        .expect("the next public crank must finalize Recovery into Resolved");
    assert_eq!(finalized.selected, AutoCrankPlanV16::FinalizeRecovery);
    assert_eq!(finalized.outcome, AutoCrankOutcomeV16::RecoveryResolved);
    assert_eq!(
        market.header.mode, 1,
        "terminal close must become reachable"
    );
    assert_eq!(market.header.recovery_reason, recovery_reason_before);
    assert_eq!(account.header.active_bitmap, bitmap_before);
    assert_eq!(account.header.pnl, pnl_before);
    assert_eq!(account.header.capital, capital_before);
    assert_eq!(market.header.vault, vault_before);
    assert_eq!(market.header.c_tot, c_tot_before);
    assert_eq!(market.header.insurance, insurance_before);
    market.validate_shape().unwrap();
    account.validate_with_market(&market.as_view()).unwrap();
}

// ROADMAP 3C step 4 — terminal no-DoS route via the self-classifying crank: a
// Live account whose outstanding bankruptcy close ledger has EXPIRED (current
// slot past max_close_slot) is classified expired_close, and the auto-crank
// dispatches the terminal recovery declaration (ActiveBankruptCloseCannotProgress)
// — the close-cannot-progress recovery, no caller-chosen action, no value move.
#[test]
fn v16_auto_crank_declares_recovery_for_expired_live_close() {
    use percolator::{CloseProgressLedgerV16, CloseProgressLedgerV16Account};

    let (mut header, mut markets) = market_fixture(1, 100);
    let mut account_header = account_fixture(1, 41);
    header.current_slot = V16PodU64::new(10);

    // An active, outstanding (residual>0), EXPIRED close ledger on asset 0.
    let market_id = markets[0].engine.asset.try_to_runtime().unwrap().market_id;
    account_header.close_progress =
        CloseProgressLedgerV16Account::from_runtime(&CloseProgressLedgerV16 {
            active: true,
            finalized: false,
            canceled: false,
            close_id: 1,
            asset_index: 0,
            market_id,
            domain_side: SideV16::Short,
            gross_loss_at_close_start: 10,
            drift_reference_slot: 1,
            max_close_slot: 2, // < current_slot 10 => expired
            support_consumed: 0,
            junior_face_burned: 0,
            insurance_spent: 0,
            b_loss_booked: 0,
            explicit_loss_assigned: 0,
            quantity_adl_applied_q: 0,
            drift_consumed: 0,
            residual_remaining: 10,
        });

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);

    let summary = market.build_actionable_summary(&account.as_view()).unwrap();
    assert!(
        summary.expired_close,
        "outstanding expired close ledger must classify expired_close: {summary:?}"
    );
    assert!(!summary.recovery_eligible && !summary.resolved_winner);

    // DeclareRecovery needs no observation (empty work).
    let work = AutoCrankWorkV16 {
        now_slot: 10,
        observations: &[],
        resolved_close_fee_rate_per_slot: 0,
    };
    let vault_before = market.header.vault;
    let r = market
        .permissionless_auto_crank_not_atomic(&mut account, work)
        .unwrap();
    assert_eq!(
        r.selected,
        AutoCrankPlanV16::DeclareRecovery {
            reason: PermissionlessRecoveryReasonV16::ActiveBankruptCloseCannotProgress
        }
    );
    assert_eq!(
        r.outcome,
        AutoCrankOutcomeV16::Progressed(PermissionlessProgressOutcomeV16::RecoveryDeclared(
            PermissionlessRecoveryReasonV16::ActiveBankruptCloseCannotProgress
        ))
    );
    // recovery declaration moves no value.
    assert_eq!(market.header.vault, vault_before);
    market.validate_shape().unwrap();
}

// ROADMAP 3C step 4 — resolved_winner classification (selector routes it to
// CloseResolved). KEY regression guard: resolved_winner must NOT gate on
// payout_snapshot_captured — close_resolved captures the snapshot LAZILY (it is
// the sole capturer), so gating on it would deadlock the first winner (never
// classified -> never captured). Here the snapshot is NOT pre-captured yet a
// payout-ready winner is still classified resolved_winner. (The dispatch's
// terminal payout realization itself is covered by the resolved-close proofs and
// the v16_resolved_payout_topup_* tests, whose fixtures fully set up the payout
// ledger; building that consistent fixture by hand here is out of scope.)
#[test]
fn v16_auto_crank_classifies_payout_ready_resolved_winner_without_snapshot() {
    let (mut header, mut markets) = market_fixture(1, 100);
    let mut account_header = account_fixture(1, 42);
    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        market.resolve_market_not_atomic(1).unwrap();
    }
    header.vault = V16PodU128::new(50);
    // Positive PnL, all blocking counts clear (resolved_positive_payout_ready);
    // payout_snapshot_captured stays 0 — the property under test.
    account_header.pnl = V16PodI128::new(5);

    let market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let account = PortfolioV16ViewMut::new(&mut account_header);
    assert_eq!(
        market.header.payout_snapshot_captured, 0,
        "snapshot intentionally NOT captured"
    );

    let summary = market.build_actionable_summary(&account.as_view()).unwrap();
    assert!(
        summary.resolved_winner,
        "a payout-ready resolved winner must be resolved_winner even before the \
         snapshot is captured (no snapshot gate -> no first-winner deadlock): {summary:?}"
    );
    assert!(!summary.recovery_eligible && !summary.stale && !summary.liquidatable);
}

// ROADMAP 3C step 4 — b-stale dispatch arm: an account with a b-stale active leg
// is classified b_stale (priority over the stale-cert refresh), and the auto-crank
// dispatches the B-chunk settle without requiring oracle observations.
#[test]
fn v16_auto_crank_settles_b_stale_leg() {
    let (mut header, mut markets) = market_fixture(1, 100);
    let mut account_header = account_fixture(1, 51);
    header.current_slot = V16PodU64::new(10);
    header.slot_last = V16PodU64::new(10);
    let mut asset0 = markets[0].engine.asset.try_to_runtime().unwrap();
    asset0.slot_last = 10;
    asset0.oi_eff_long_q = POS_SCALE;
    asset0.oi_eff_short_q = POS_SCALE;
    asset0.loss_weight_sum_long = POS_SCALE;
    asset0.loss_weight_sum_short = POS_SCALE;
    asset0.stored_pos_count_long = 1;
    asset0.stored_pos_count_short = 1;
    markets[0].engine.asset = AssetStateV16Account::from_runtime(&asset0);

    // Active leg flagged b-stale, with b_snap already at the current target so the
    // settle resolves to a clean delta_b=0 clear (progress: clears the b-stale flag).
    account_header.legs[0] = PortfolioLegV16Account::from_runtime(&PortfolioLegV16 {
        active: true,
        asset_index: 0,
        market_id: asset0.market_id,
        side: SideV16::Long,
        basis_pos_q: POS_SCALE as i128,
        a_basis: ADL_ONE,
        k_snap: asset0.k_long,
        f_snap: asset0.f_long_num,
        epoch_snap: asset0.epoch_long,
        loss_weight: POS_SCALE,
        b_snap: asset0.b_long_num,
        b_rem: 0,
        b_epoch_snap: asset0.epoch_long,
        b_stale: true,
        stale: false,
    });
    account_header.active_bitmap[0] = V16PodU64::new(1);

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);

    let summary = market.build_actionable_summary(&account.as_view()).unwrap();
    assert!(
        summary.b_stale,
        "b-stale leg must classify b_stale: {summary:?}"
    );

    let work = AutoCrankWorkV16 {
        now_slot: 10,
        observations: &[],
        resolved_close_fee_rate_per_slot: 0,
    };
    let r = market
        .permissionless_auto_crank_not_atomic(&mut account, work)
        .unwrap();
    // b_stale has priority over the stale-cert refresh, so SettleBChunk is selected
    // with the engine-chosen asset (the b-stale leg's asset) and dispatched to the
    // real B-chunk settle entrypoint (AccountBChunk outcome). The rank-decreasing
    // B-advance for a genuinely drifted leg (delta_b>0) is proven at the A2 kernel.
    assert_eq!(
        r.selected,
        AutoCrankPlanV16::SettleBChunk { asset_index: 0 }
    );
    assert!(matches!(
        r.outcome,
        AutoCrankOutcomeV16::Progressed(PermissionlessProgressOutcomeV16::AccountBChunk(_))
    ));
    market.validate_shape().unwrap();
}

// ENGINE.MD order-insensitivity: when the engine-selected step needs an
// observation the caller did not supply (e.g. a stale keeper tx whose task
// changed), the auto-crank returns a clean NonProgress error WITHOUT mutating
// state — so arbitrary landing order is safe.
#[test]
fn v16_auto_crank_missing_observation_is_clean_nonprogress_no_mutation() {
    let (mut header, mut markets) = market_fixture(1, 100);
    let mut account_header = account_fixture(1, 71);
    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        let mut account = PortfolioV16ViewMut::new(&mut account_header);
        market.deposit_not_atomic(&mut account, 1_000).unwrap();
    }
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);

    // fresh account -> selector wants RefreshAccount, which needs an observation;
    // supply NONE -> clean NonProgress, no mutation.
    let summary = market.build_actionable_summary(&account.as_view()).unwrap();
    assert!(summary.stale);
    let cert_before = account.header.health_cert;
    let work = AutoCrankWorkV16 {
        now_slot: 5,
        observations: &[],
        resolved_close_fee_rate_per_slot: 0,
    };
    let r = market.permissionless_auto_crank_not_atomic(&mut account, work);
    assert_eq!(r, Err(percolator::V16Error::NonProgress));
    // no mutation (SVM would roll back anyway, but the engine did not commit).
    assert_eq!(account.header.health_cert, cert_before);
    market.validate_shape().unwrap();
}

// REALIZABILITY MATRIX — closes the no-DoS dispatch seam that hid the b-stale /
// committed-state-liquidation stall. The faithful invariant is OBSERVATION-
// INDEPENDENCE: for a plan that `auto_crank_plan_requires_caller_observation`
// reports as NOT requiring one, the single public crank must return the SAME
// outcome whether or not an observation is supplied (the observation is redundant
// — the plan is realizable from committed state). For the one form that DOES
// require one (RefreshAccount with no active asset), the empty-observation call
// cleanly stalls (NonProgress) while supplying the observation progresses. This
// both guards liveness AND ties the pure predicate to the REAL dispatch per class,
// so the predicate cannot drift from behaviour. Note: a committed-state plan may
// still return a genuine economic terminal (e.g. RecoveryRequired) — that is fine,
// because it returns the SAME terminal with or without the observation; the bug
// was an outcome that DIFFERED on the observation.
fn assert_observation_independent(
    label: &str,
    build: impl Fn() -> (
        MarketGroupV16HeaderAccount,
        Vec<Market<u64>>,
        PortfolioAccountV16Account,
    ),
    obs_asset_index: usize,
    now_slot: u64,
    expected_plan: AutoCrankPlanV16,
    expected_requires_obs: bool,
) {
    // The predicate's claim must equal this class's documented observation need.
    assert_eq!(
        auto_crank_plan_requires_caller_observation(&expected_plan),
        expected_requires_obs,
        "{label}: predicate disagrees with documented observation-requirement"
    );

    let run = |observations: &[AutoCrankObservationV16]| {
        let (mut header, mut markets, mut account_header) = build();
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        let mut account = PortfolioV16ViewMut::new(&mut account_header);
        market.permissionless_auto_crank_not_atomic(
            &mut account,
            AutoCrankWorkV16 {
                now_slot,
                observations,
                resolved_close_fee_rate_per_slot: 0,
            },
        )
    };

    let r_empty = run(&[]);

    // The observation a keeper would otherwise supply: the asset's *committed*
    // price (the value the cert was already certified against) and zero funding.
    let committed_price = {
        let (_h, m, _a) = build();
        m[obs_asset_index]
            .engine
            .asset
            .try_to_runtime()
            .unwrap()
            .effective_price
    };
    let obs = [AutoCrankObservationV16 {
        asset_index: obs_asset_index,
        effective_price: committed_price,
        funding_rate_e9: 0,
    }];
    let r_obs = run(&obs);

    if expected_requires_obs {
        assert_eq!(
            r_empty,
            Err(percolator::V16Error::NonProgress),
            "{label}: a plan requiring an observation must cleanly stall without one"
        );
        assert!(
            r_obs.is_ok(),
            "{label}: must progress once the observation is supplied, got {r_obs:?}"
        );
    } else {
        // The whole bug class: outcome differing on a redundant observation.
        assert_eq!(
            r_empty, r_obs,
            "{label}: observation must not change the outcome (plan is realizable \
             from committed state)"
        );
        if let Ok(res) = r_empty {
            assert_eq!(
                res.selected, expected_plan,
                "{label}: unexpected selected plan"
            );
        }
    }
}

#[test]
fn v16_auto_crank_progress_realizable_without_observation_for_every_class() {
    // --- A1 stale with no active asset: fallback refresh has no committed asset
    // to use, so it still needs a caller observation.
    assert_observation_independent(
        "stale_empty_account",
        || {
            let (mut header, mut markets) = market_fixture(1, 100);
            let mut account_header = account_fixture(1, 200);
            let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
            let mut account = PortfolioV16ViewMut::new(&mut account_header);
            market.deposit_not_atomic(&mut account, 1_000).unwrap();
            drop(market);
            drop(account);
            (header, markets, account_header)
        },
        0,
        5,
        AutoCrankPlanV16::RefreshAccount { asset_index: None },
        true,
    );

    // --- A1 stale with an active asset: refresh is realizable from committed
    // state. This is the no-DoS case for stale multi-asset accounts whose first
    // active asset does not have a fresh oracle observation available.
    assert_observation_independent(
        "stale_active_asset",
        || {
            let (mut header, mut markets) = market_fixture(1, 100);
            let mut account_header = account_fixture(1, 209);
            let mut counterparty_header = account_fixture(1, 210);
            let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
            let mut account = PortfolioV16ViewMut::new(&mut account_header);
            let mut counterparty = PortfolioV16ViewMut::new(&mut counterparty_header);
            market.deposit_not_atomic(&mut account, 1_000).unwrap();
            market.deposit_not_atomic(&mut counterparty, 1_000).unwrap();
            open_one_lot_pair(&mut market, &mut account, &mut counterparty);
            account.header.health_cert.valid = 0;
            drop(market);
            drop(account);
            drop(counterparty);
            (header, markets, account_header)
        },
        0,
        5,
        AutoCrankPlanV16::RefreshAccount {
            asset_index: Some(0),
        },
        false,
    );

    // --- A2 b_stale: SettleBChunk ignores price -> observation REDUNDANT.
    assert_observation_independent(
        "b_stale",
        || {
            let (mut header, mut markets) = market_fixture(1, 100);
            let mut account_header = account_fixture(1, 201);
            header.current_slot = V16PodU64::new(10);
            header.slot_last = V16PodU64::new(10);
            let mut asset0 = markets[0].engine.asset.try_to_runtime().unwrap();
            asset0.slot_last = 10;
            asset0.oi_eff_long_q = POS_SCALE;
            asset0.oi_eff_short_q = POS_SCALE;
            asset0.loss_weight_sum_long = POS_SCALE;
            asset0.loss_weight_sum_short = POS_SCALE;
            asset0.stored_pos_count_long = 1;
            asset0.stored_pos_count_short = 1;
            markets[0].engine.asset = AssetStateV16Account::from_runtime(&asset0);
            account_header.legs[0] = PortfolioLegV16Account::from_runtime(&PortfolioLegV16 {
                active: true,
                asset_index: 0,
                market_id: asset0.market_id,
                side: SideV16::Long,
                basis_pos_q: POS_SCALE as i128,
                a_basis: ADL_ONE,
                k_snap: asset0.k_long,
                f_snap: asset0.f_long_num,
                epoch_snap: asset0.epoch_long,
                loss_weight: POS_SCALE,
                b_snap: asset0.b_long_num,
                b_rem: 0,
                b_epoch_snap: asset0.epoch_long,
                b_stale: true,
                stale: false,
            });
            account_header.active_bitmap[0] = V16PodU64::new(1);
            (header, markets, account_header)
        },
        0,
        10,
        AutoCrankPlanV16::SettleBChunk { asset_index: 0 },
        false,
    );

    // --- A3 pending_close: the immutable close ledger carries all dispatch
    // inputs, so AdvanceClose must not depend on a caller observation.
    assert_observation_independent(
        "pending_close",
        || {
            const SIZE_Q: u128 = 10 * POS_SCALE;
            let (mut header, mut markets) = market_fixture(1, 100);
            header.config.maintenance_margin_bps = V16PodU64::new(1_000);
            header.config.initial_margin_bps = V16PodU64::new(1_000);
            header.config.max_price_move_bps_per_slot = V16PodU64::new(500);
            header.config.max_accrual_dt_slots = V16PodU64::new(1);
            header.config.min_funding_lifetime_slots = V16PodU64::new(1);
            header.config.public_b_chunk_atoms = V16PodU128::new(100);
            let mut long_header = account_fixture(1, 211);
            let mut short_header = account_fixture(1, 212);
            {
                let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
                let mut long = PortfolioV16ViewMut::new(&mut long_header);
                let mut short = PortfolioV16ViewMut::new(&mut short_header);
                market.deposit_not_atomic(&mut long, 1_000).unwrap();
                market.deposit_not_atomic(&mut short, 250).unwrap();
                market
                    .execute_trade_with_fee_loss_stale_scoped_not_atomic(
                        &mut long,
                        &mut short,
                        TradeRequestV16 {
                            asset_index: 0,
                            size_q: signed_q(SIZE_Q),
                            exec_price: 100,
                            fee_bps: 0,
                        },
                    )
                    .unwrap();
                for (offset, price) in (105u64..=150).step_by(5).enumerate() {
                    let slot = 2 + offset as u64;
                    market
                        .set_asset_raw_oracle_target_not_atomic(0, price)
                        .unwrap();
                    market
                        .accrue_asset_to_not_atomic(0, slot, price, 0, true)
                        .unwrap();
                }
                market
                    .execute_trade_with_fee_loss_stale_scoped_not_atomic(
                        &mut long,
                        &mut short,
                        TradeRequestV16 {
                            asset_index: 0,
                            size_q: -signed_q(SIZE_Q),
                            exec_price: 150,
                            fee_bps: 0,
                        },
                    )
                    .unwrap();
            }
            (header, markets, short_header)
        },
        0,
        11,
        AutoCrankPlanV16::AdvanceClose,
        false,
    );

    // --- A5 liquidatable: Liquidate reads the current cert -> observation REDUNDANT.
    assert_observation_independent(
        "liquidatable",
        || {
            let (mut header, mut markets) = market_fixture(1, 100);
            let mut account_header = account_fixture(1, 202);
            header.current_slot = V16PodU64::new(10);
            header.slot_last = V16PodU64::new(10);
            header.vault = V16PodU128::new(50);
            header.insurance = V16PodU128::new(50);
            header.negative_pnl_account_count = V16PodU64::new(1);
            let mut asset = markets[0].engine.asset.try_to_runtime().unwrap();
            asset.slot_last = 10;
            asset.oi_eff_long_q = 2 * POS_SCALE;
            asset.oi_eff_short_q = 2 * POS_SCALE;
            asset.loss_weight_sum_long = 2 * POS_SCALE;
            asset.loss_weight_sum_short = 2 * POS_SCALE;
            asset.stored_pos_count_long = 2;
            asset.stored_pos_count_short = 2;
            markets[0].engine.asset = AssetStateV16Account::from_runtime(&asset);
            header.resolved_payout_blocker_count = V16PodU64::new(4);
            account_header.pnl = V16PodI128::new(-5);
            account_header.legs[0] = PortfolioLegV16Account::from_runtime(&PortfolioLegV16 {
                active: true,
                asset_index: 0,
                market_id: asset.market_id,
                side: SideV16::Long,
                basis_pos_q: POS_SCALE as i128,
                a_basis: ADL_ONE,
                k_snap: asset.k_long,
                f_snap: asset.f_long_num,
                epoch_snap: asset.epoch_long,
                loss_weight: POS_SCALE,
                b_snap: asset.b_long_num,
                b_rem: 0,
                b_epoch_snap: asset.epoch_long,
                b_stale: false,
                stale: false,
            });
            account_header.active_bitmap[0] = V16PodU64::new(1);
            {
                let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
                let mut account = PortfolioV16ViewMut::new(&mut account_header);
                market
                    .full_account_refresh_not_atomic(&mut account)
                    .expect("setup must produce a current liquidation cert");
            }
            (header, markets, account_header)
        },
        0,
        10,
        AutoCrankPlanV16::Liquidate { asset_index: 0 },
        false,
    );

    // --- A4 expired_close: DeclareRecovery needs no price -> observation REDUNDANT.
    assert_observation_independent(
        "expired_close",
        || {
            use percolator::{CloseProgressLedgerV16, CloseProgressLedgerV16Account};
            let (mut header, markets) = market_fixture(1, 100);
            let mut account_header = account_fixture(1, 203);
            header.current_slot = V16PodU64::new(10);
            let market_id = markets[0].engine.asset.try_to_runtime().unwrap().market_id;
            account_header.close_progress =
                CloseProgressLedgerV16Account::from_runtime(&CloseProgressLedgerV16 {
                    active: true,
                    finalized: false,
                    canceled: false,
                    close_id: 1,
                    asset_index: 0,
                    market_id,
                    domain_side: SideV16::Short,
                    gross_loss_at_close_start: 10,
                    drift_reference_slot: 1,
                    max_close_slot: 2,
                    support_consumed: 0,
                    junior_face_burned: 0,
                    insurance_spent: 0,
                    b_loss_booked: 0,
                    explicit_loss_assigned: 0,
                    quantity_adl_applied_q: 0,
                    drift_consumed: 0,
                    residual_remaining: 10,
                });
            (header, markets, account_header)
        },
        0,
        10,
        AutoCrankPlanV16::DeclareRecovery {
            reason: PermissionlessRecoveryReasonV16::ActiveBankruptCloseCannotProgress,
        },
        false,
    );

    // --- A6 terminal Recovery: the next step is a value-neutral transition to
    // Resolved and needs no oracle observation.
    assert_observation_independent(
        "finalize_recovery",
        || {
            let (mut header, markets) = market_fixture(1, 100);
            let account_header = account_fixture(1, 205);
            header.mode = 2;
            header.recovery_reason = V16OptionalRecoveryReasonAccount::from_runtime(Some(
                PermissionlessRecoveryReasonV16::ActiveBankruptCloseCannotProgress,
            ));
            (header, markets, account_header)
        },
        0,
        10,
        AutoCrankPlanV16::FinalizeRecovery,
        false,
    );

    // --- A7 resolved_winner: CloseResolved needs no price -> observation REDUNDANT.
    // (Previously only its CLASSIFICATION was tested; the empty-observation DISPATCH
    // — including a legitimate RecoveryRequired terminal — was uncovered.)
    assert_observation_independent(
        "resolved_winner",
        || {
            let (mut header, mut markets) = market_fixture(1, 100);
            let mut account_header = account_fixture(1, 204);
            {
                let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
                market.resolve_market_not_atomic(1).unwrap();
            }
            header.vault = V16PodU128::new(50);
            account_header.pnl = V16PodI128::new(5);
            (header, markets, account_header)
        },
        0,
        10,
        AutoCrankPlanV16::CloseResolved,
        false,
    );
}
