#![cfg(feature = "fuzz")]

use percolator::{
    AssetLifecycleV16, EngineAssetSlotV16Account, LiquidationRequestV16, Market,
    MarketGroupV16HeaderAccount, MarketGroupV16ViewMut, PermissionlessCrankActionV16,
    PermissionlessCrankRequestV16, PermissionlessRecoveryReasonV16, PortfolioAccountV16Account,
    PortfolioV16View, PortfolioV16ViewMut, ProvenanceHeaderV16, ProvenanceHeaderV16Account,
    SideModeV16, SideV16, TradeRequestV16, V16Config, V16Error, V16PodU64, BOUND_SCALE, POS_SCALE,
};
use proptest::prelude::*;

const FUZZ_DOMAIN_COUNT: usize = 2;
const FUZZ_INITIAL_MARGIN_BPS: u64 = 500;
const FUZZ_MAINTENANCE_MARGIN_BPS: u64 = 250;
const FUZZ_TRADE_LOT_STRIDE: u128 = 50;
const FUZZ_TRADE_LOT_SEED_MOD: u128 = 512;
const FUZZ_SELECTOR_STRIDES: [u8; 8] = [1, 3, 5, 7, 9, 11, 13, 15];
const FUZZ_MIXED_MIN_ACTIONS: usize = 16;
const FUZZ_MIXED_MAX_ACTIONS: usize = 96;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FuzzActionKind {
    Deposit,
    Withdraw,
    SyncAccountFeeNextSlot,
    FullAccountRefresh,
    ExecuteTradeWithFeeLossStaleScoped,
    PermissionlessCrankRefresh,
    PermissionlessCrankSettleB,
    SyncAccountFeeCurrentSlot,
    ConvertReleasedPnlToCapital,
    LiquidateAccount,
    PermissionlessCrankRecover,
    ResolveMarket,
    CloseResolvedAccount,
    DepositDomainInsurance,
    WithdrawDomainInsurance,
    DepositFreshCounterpartyBacking,
    WithdrawFreshCounterpartyBacking,
    ForfeitRecoveryLeg,
    FinalizeSideReset,
    WithdrawBackingProviderEarnings,
    ClaimResolvedPayoutTopup,
}

impl FuzzActionKind {
    const ALL: [Self; 21] = [
        Self::Deposit,
        Self::Withdraw,
        Self::SyncAccountFeeNextSlot,
        Self::FullAccountRefresh,
        Self::ExecuteTradeWithFeeLossStaleScoped,
        Self::PermissionlessCrankRefresh,
        Self::PermissionlessCrankSettleB,
        Self::SyncAccountFeeCurrentSlot,
        Self::ConvertReleasedPnlToCapital,
        Self::LiquidateAccount,
        Self::PermissionlessCrankRecover,
        Self::ResolveMarket,
        Self::CloseResolvedAccount,
        Self::DepositDomainInsurance,
        Self::WithdrawDomainInsurance,
        Self::DepositFreshCounterpartyBacking,
        Self::WithdrawFreshCounterpartyBacking,
        Self::ForfeitRecoveryLeg,
        Self::FinalizeSideReset,
        Self::WithdrawBackingProviderEarnings,
        Self::ClaimResolvedPayoutTopup,
    ];

    const COUNT: usize = Self::ALL.len();
    const SELECTOR_ACTIONS: [Self; 23] = [
        Self::Deposit,
        Self::Withdraw,
        Self::SyncAccountFeeNextSlot,
        Self::FullAccountRefresh,
        Self::ExecuteTradeWithFeeLossStaleScoped,
        Self::PermissionlessCrankRefresh,
        Self::PermissionlessCrankSettleB,
        Self::SyncAccountFeeCurrentSlot,
        Self::ConvertReleasedPnlToCapital,
        Self::LiquidateAccount,
        Self::PermissionlessCrankRecover,
        Self::ResolveMarket,
        Self::CloseResolvedAccount,
        Self::DepositDomainInsurance,
        Self::WithdrawDomainInsurance,
        Self::DepositFreshCounterpartyBacking,
        Self::WithdrawFreshCounterpartyBacking,
        Self::ForfeitRecoveryLeg,
        Self::FinalizeSideReset,
        Self::WithdrawBackingProviderEarnings,
        Self::ClaimResolvedPayoutTopup,
        // Preserve the established 23-slot selector corpus after retiring actions.
        Self::ClaimResolvedPayoutTopup,
        Self::ClaimResolvedPayoutTopup,
    ];
    const SELECTOR_COUNT: u8 = Self::SELECTOR_ACTIONS.len() as u8;

    fn from_selector(selector: u8) -> Self {
        Self::SELECTOR_ACTIONS[selector as usize % Self::SELECTOR_ACTIONS.len()]
    }

    fn index(self) -> usize {
        self as usize
    }

    fn is_domain_target(self) -> bool {
        matches!(
            self,
            Self::DepositDomainInsurance
                | Self::WithdrawDomainInsurance
                | Self::DepositFreshCounterpartyBacking
                | Self::WithdrawFreshCounterpartyBacking
                | Self::WithdrawBackingProviderEarnings
        )
    }

    fn name(self) -> &'static str {
        match self {
            Self::Deposit => "deposit",
            Self::Withdraw => "withdraw",
            Self::SyncAccountFeeNextSlot => "sync_account_fee_next_slot",
            Self::FullAccountRefresh => "full_account_refresh",
            Self::ExecuteTradeWithFeeLossStaleScoped => "execute_trade_with_fee_loss_stale_scoped",
            Self::PermissionlessCrankRefresh => "permissionless_crank_refresh",
            Self::PermissionlessCrankSettleB => "permissionless_crank_settle_b",
            Self::SyncAccountFeeCurrentSlot => "sync_account_fee_current_slot",
            Self::ConvertReleasedPnlToCapital => "convert_released_pnl_to_capital",
            Self::LiquidateAccount => "liquidate_account",
            Self::PermissionlessCrankRecover => "permissionless_crank_recover",
            Self::ResolveMarket => "resolve_market",
            Self::CloseResolvedAccount => "close_resolved_account",
            Self::DepositDomainInsurance => "deposit_domain_insurance",
            Self::WithdrawDomainInsurance => "withdraw_domain_insurance",
            Self::DepositFreshCounterpartyBacking => "deposit_fresh_counterparty_backing",
            Self::WithdrawFreshCounterpartyBacking => "withdraw_fresh_counterparty_backing",
            Self::ForfeitRecoveryLeg => "forfeit_recovery_leg",
            Self::FinalizeSideReset => "finalize_side_reset",
            Self::WithdrawBackingProviderEarnings => "withdraw_backing_provider_earnings",
            Self::ClaimResolvedPayoutTopup => "claim_resolved_payout_topup",
        }
    }
}

fn fuzz_side(seed: u16) -> SideV16 {
    if (seed & 1) == 0 {
        SideV16::Long
    } else {
        SideV16::Short
    }
}

fn ids() -> ([u8; 32], [u8; 32], [u8; 32], [u8; 32]) {
    ([1; 32], [2; 32], [3; 32], [4; 32])
}

fn fuzz_group() -> (MarketGroupV16HeaderAccount, Vec<Market<u64>>) {
    let (market_id, _, _, _) = ids();
    let mut cfg = V16Config::public_user_fund_with_market_slots(1, 1, 0, 10);
    cfg.initial_margin_bps = FUZZ_INITIAL_MARGIN_BPS;
    cfg.maintenance_margin_bps = FUZZ_MAINTENANCE_MARGIN_BPS;
    cfg.max_price_move_bps_per_slot = 100;
    cfg.max_trading_fee_bps = 10;
    cfg.public_b_chunk_atoms = 1;
    let mut header = MarketGroupV16HeaderAccount::new_dynamic(market_id, cfg, 1, 0).unwrap();
    let mut markets = vec![Market::new(0u64, EngineAssetSlotV16Account::default())];
    header
        .activate_empty_asset_slot_not_atomic(0, &mut markets[0].engine, 1, 1)
        .unwrap();
    (header, markets)
}

fn fuzz_account(account_id: [u8; 32]) -> PortfolioAccountV16Account {
    let (market_id, _, _, owner) = ids();
    let header = ProvenanceHeaderV16Account::from_runtime(&ProvenanceHeaderV16::new(
        market_id, account_id, owner,
    ));
    let mut account = PortfolioAccountV16Account::default();
    account.init_empty_in_place(header).unwrap();
    account
}

fn fund_two_accounts(
    header: &mut MarketGroupV16HeaderAccount,
    markets: &mut Vec<Market<u64>>,
    account_a: &mut PortfolioAccountV16Account,
    account_b: &mut PortfolioAccountV16Account,
    amount: u128,
) {
    let mut market = MarketGroupV16ViewMut::new(header, markets);
    let mut a = PortfolioV16ViewMut::new(account_a);
    let mut b = PortfolioV16ViewMut::new(account_b);
    market.deposit_not_atomic(&mut a, amount).unwrap();
    market.deposit_not_atomic(&mut b, amount).unwrap();
}

#[test]
fn v16_price_path_and_trades_allow_late_short_to_profit_from_future_funding() {
    const INITIAL_PRICE_CENTS: u64 = 5_000_000; // $50,000.00
    const COLLATERAL_CENTS: u128 = 25_000_000; // $250,000.00 per trader
    const TAKER_FEE_BPS: u64 = 1;
    const FUNDING_RATE_E9: i128 = 10_000; // 10 ppm per slot
    const LATE_FUNDING_SLOTS: u64 = 24;

    let (market_id, _, _, _) = ids();
    let mut cfg = V16Config::public_user_fund_with_market_slots(1, 1, 0, 10);
    cfg.initial_margin_bps = 2_000;
    cfg.maintenance_margin_bps = 1_500;
    cfg.max_trading_fee_bps = 5;
    cfg.max_price_move_bps_per_slot = 100;
    cfg.max_abs_funding_e9_per_slot = FUNDING_RATE_E9 as u64;
    cfg.validate_public_user_fund()
        .expect("realistic market parameters must satisfy the public-fund solvency envelope");

    let mut header = MarketGroupV16HeaderAccount::new_dynamic(market_id, cfg, 1, 0).unwrap();
    let mut markets = vec![Market::new(0u64, EngineAssetSlotV16Account::default())];
    header
        .activate_empty_asset_slot_not_atomic(0, &mut markets[0].engine, INITIAL_PRICE_CENTS, 1)
        .unwrap();

    let mut early_long_header = fuzz_account([10; 32]);
    let mut early_short_header = fuzz_account([11; 32]);
    let mut late_long_header = fuzz_account([12; 32]);
    let mut late_short_header = fuzz_account([13; 32]);

    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        for account_header in [
            &mut early_long_header,
            &mut early_short_header,
            &mut late_long_header,
            &mut late_short_header,
        ] {
            let mut account = PortfolioV16ViewMut::new(account_header);
            market
                .deposit_not_atomic(&mut account, COLLATERAL_CENTS)
                .unwrap();
        }

        let mut early_long = PortfolioV16ViewMut::new(&mut early_long_header);
        let mut early_short = PortfolioV16ViewMut::new(&mut early_short_header);
        market
            .execute_trade_with_fee_loss_stale_scoped_not_atomic(
                &mut early_long,
                &mut early_short,
                TradeRequestV16 {
                    asset_index: 0,
                    size_q: i128::try_from(2 * POS_SCALE).unwrap(),
                    exec_price: INITIAL_PRICE_CENTS,
                    fee_bps: TAKER_FEE_BPS,
                },
            )
            .unwrap();

        // Authenticated observations move a BTC-like market by less than the
        // configured 1% per-slot limit. Positive rates accompany the elevated
        // prices; the brief negative rate makes the path non-monotonic.
        for (slot, price, funding_rate_e9) in [
            (2, 5_040_000, 2_000),
            (3, 5_080_320, 6_000),
            (4, 5_049_838, -1_000),
            (5, 5_080_137, 8_000),
        ] {
            market
                .set_asset_raw_oracle_target_not_atomic(0, price)
                .unwrap();
            market
                .accrue_asset_to_not_atomic(0, slot, price, funding_rate_e9, true)
                .unwrap();
        }

        let elevated_price = market.markets[0].engine.asset.effective_price.get();
        market
            .execute_trade_with_fee_loss_stale_scoped_not_atomic(
                &mut early_long,
                &mut early_short,
                TradeRequestV16 {
                    asset_index: 0,
                    size_q: -i128::try_from(2 * POS_SCALE).unwrap(),
                    exec_price: elevated_price,
                    fee_bps: TAKER_FEE_BPS,
                },
            )
            .unwrap();

        // Realize the closed incumbents' claims before a fresh pair uses the
        // same source-credit domains.
        if early_long.header.pnl.get() > 0 {
            market
                .convert_released_pnl_to_capital_not_atomic(&mut early_long)
                .unwrap();
        }
        if early_short.header.pnl.get() > 0 {
            market
                .convert_released_pnl_to_capital_not_atomic(&mut early_short)
                .unwrap();
        }

        let historical_short_funding = market.markets[0].engine.asset.f_short_num.get();
        let mut late_long = PortfolioV16ViewMut::new(&mut late_long_header);
        let mut late_short = PortfolioV16ViewMut::new(&mut late_short_header);
        market
            .execute_trade_with_fee_loss_stale_scoped_not_atomic(
                &mut late_long,
                &mut late_short,
                TradeRequestV16 {
                    asset_index: 0,
                    size_q: i128::try_from(POS_SCALE).unwrap(),
                    exec_price: elevated_price,
                    fee_bps: TAKER_FEE_BPS,
                },
            )
            .unwrap();

        let late_short_leg = late_short.header.legs[0].try_to_runtime().unwrap();
        assert_eq!(late_short_leg.f_snap, historical_short_funding);
        assert_eq!(
            late_short.header.funding_short_received_atoms_total.get(),
            0
        );

        // The elevated market then charges positive funding for 24 slots. A
        // one-BTC short receives 50 cents per slot at this price and rate.
        for slot in 6..(6 + LATE_FUNDING_SLOTS) {
            market
                .accrue_asset_to_not_atomic(0, slot, elevated_price, FUNDING_RATE_E9, true)
                .unwrap();
        }

        market
            .execute_trade_with_fee_loss_stale_scoped_not_atomic(
                &mut late_long,
                &mut late_short,
                TradeRequestV16 {
                    asset_index: 0,
                    size_q: -i128::try_from(POS_SCALE).unwrap(),
                    exec_price: elevated_price,
                    fee_bps: TAKER_FEE_BPS,
                },
            )
            .unwrap();

        let funding_profit = 50 * LATE_FUNDING_SLOTS as u128;
        let fee_per_fill = (elevated_price as u128).div_ceil(10_000);
        let round_trip_fees = 2 * fee_per_fill;
        assert_eq!(
            late_short.header.funding_short_received_atoms_total.get(),
            funding_profit
        );
        assert_eq!(
            late_long.header.funding_long_paid_atoms_total.get(),
            funding_profit,
            "the late pair's funding transfer must conserve"
        );
        assert_eq!(late_short.header.pnl.get(), funding_profit as i128);
        assert_eq!(
            late_short.header.capital.get(),
            COLLATERAL_CENTS - round_trip_fees
        );
        assert_eq!(
            late_short.header.capital.get() + late_short.header.pnl.get() as u128
                - COLLATERAL_CENTS,
            funding_profit - round_trip_fees,
            "the late short must retain the funding profit after both trade fees"
        );
        assert_eq!(late_short.header.active_bitmap[0].get(), 0);

        market.validate_shape().unwrap();
        late_long.validate_with_market(&market.as_view()).unwrap();
        late_short.validate_with_market(&market.as_view()).unwrap();
    }
}

struct FuzzState {
    header: MarketGroupV16HeaderAccount,
    markets: Vec<Market<u64>>,
    account_a: PortfolioAccountV16Account,
    account_b: PortfolioAccountV16Account,
}

fn clone_fuzz_state(
    header: &MarketGroupV16HeaderAccount,
    markets: &[Market<u64>],
    account_a: &PortfolioAccountV16Account,
    account_b: &PortfolioAccountV16Account,
) -> FuzzState {
    FuzzState {
        header: *header,
        markets: markets.to_vec(),
        account_a: *account_a,
        account_b: *account_b,
    }
}

fn restore_fuzz_state(
    state: FuzzState,
    header: &mut MarketGroupV16HeaderAccount,
    markets: &mut Vec<Market<u64>>,
    account_a: &mut PortfolioAccountV16Account,
    account_b: &mut PortfolioAccountV16Account,
) {
    *header = state.header;
    *markets = state.markets;
    *account_a = state.account_a;
    *account_b = state.account_b;
}

fn fuzz_state_changed(
    state: &FuzzState,
    header: &MarketGroupV16HeaderAccount,
    markets: &[Market<u64>],
    account_a: &PortfolioAccountV16Account,
    account_b: &PortfolioAccountV16Account,
) -> bool {
    (header, markets, account_a, account_b)
        != (
            &state.header,
            state.markets.as_slice(),
            &state.account_a,
            &state.account_b,
        )
}

fn fuzz_domain(seed: u16) -> usize {
    (seed as usize) % FUZZ_DOMAIN_COUNT
}

fn fuzz_target_a(seed: u16) -> bool {
    (seed & 1) == 0
}

fn fuzz_trade_size_q(amount_seed: u16) -> i128 {
    let lots = 1 + ((amount_seed as u128) % FUZZ_TRADE_LOT_SEED_MOD) * FUZZ_TRADE_LOT_STRIDE;
    i128::try_from(lots * POS_SCALE).unwrap()
}

fn fuzz_liquidation_target(
    account_a: &PortfolioAccountV16Account,
    account_b: &PortfolioAccountV16Account,
    prefer_a: bool,
) -> bool {
    let preferred = if prefer_a { account_a } else { account_b };
    let fallback = if prefer_a { account_b } else { account_a };
    if fuzz_account_has_active_asset_leg(preferred, 0)
        || !fuzz_account_has_active_asset_leg(fallback, 0)
    {
        prefer_a
    } else {
        !prefer_a
    }
}

fn balanced_fuzz_selector(offset: u8, stride_seed: u8, step_index: usize) -> u8 {
    let stride = FUZZ_SELECTOR_STRIDES[(stride_seed as usize) % FUZZ_SELECTOR_STRIDES.len()];
    offset.wrapping_add((step_index as u8).wrapping_mul(stride)) % FuzzActionKind::SELECTOR_COUNT
}

fn mixed_lifecycle_selector(offset: u8, stride_seed: u8, step_index: usize) -> u8 {
    let selector = balanced_fuzz_selector(offset, stride_seed, step_index);
    if matches!(
        FuzzActionKind::from_selector(selector),
        FuzzActionKind::PermissionlessCrankRecover | FuzzActionKind::ResolveMarket
    ) {
        return FuzzActionKind::FullAccountRefresh as u8;
    }
    selector
}

fn assert_fuzz_invariants(
    header: &mut MarketGroupV16HeaderAccount,
    markets: &mut [Market<u64>],
    account_a: &PortfolioAccountV16Account,
    account_b: &PortfolioAccountV16Account,
) {
    let market = MarketGroupV16ViewMut::new(header, markets);
    assert_eq!(market.validate_shape(), Ok(()));
    assert_eq!(
        PortfolioV16View::new(account_a).validate_with_market(&market.as_view()),
        Ok(())
    );
    assert_eq!(
        PortfolioV16View::new(account_b).validate_with_market(&market.as_view()),
        Ok(())
    );
    assert_eq!(
        market.header.c_tot.get(),
        account_a.capital.get() + account_b.capital.get()
    );
    assert!(
        market.header.vault.get()
            >= market.header.c_tot.get()
                + market.header.insurance.get()
                + market.header.backing_provider_earnings_total.get()
                + market.header.source_fresh_backing_total_num.get() / BOUND_SCALE
    );
    assert!(
        market.header.insurance_domain_budget_remaining_total.get()
            <= market.header.insurance.get()
    );
    let positive_pnl = [account_a.pnl.get(), account_b.pnl.get()]
        .into_iter()
        .filter(|pnl| *pnl > 0)
        .map(|pnl| pnl as u128)
        .sum::<u128>();
    assert_eq!(market.header.pnl_pos_tot.get(), positive_pnl);

    let slot = &market.markets[0].engine;

    let backing_long = slot.backing_long.try_to_runtime().unwrap();
    let backing_short = slot.backing_short.try_to_runtime().unwrap();
    assert_eq!(
        market.header.backing_provider_earnings_total.get(),
        backing_long.utilization_fee_earnings + backing_short.utilization_fee_earnings,
    );

    let source_long = slot.source_credit_long.try_to_runtime().unwrap();
    let source_short = slot.source_credit_short.try_to_runtime().unwrap();
    assert_eq!(
        market.header.source_fresh_backing_total_num.get(),
        source_long.fresh_reserved_backing_num + source_short.fresh_reserved_backing_num,
    );

    assert_eq!(
        market.header.insurance_domain_budget_remaining_total.get(),
        slot.insurance_domain_budget_long.get() - slot.insurance_domain_spent_long.get()
            + slot.insurance_domain_budget_short.get()
            - slot.insurance_domain_spent_short.get(),
    );
}

fn assert_fuzz_success_invariants(
    header: &mut MarketGroupV16HeaderAccount,
    markets: &mut [Market<u64>],
    account_a: &PortfolioAccountV16Account,
    account_b: &PortfolioAccountV16Account,
) {
    assert_fuzz_invariants(header, markets, account_a, account_b);
    let market = MarketGroupV16ViewMut::new(header, markets);
    let asset = market.markets[0].engine.asset.try_to_runtime().unwrap();
    assert_eq!(asset.oi_eff_long_q, asset.oi_eff_short_q);
}

#[derive(Clone, Copy, Debug)]
struct FuzzActionContext {
    step_index: usize,
    action: &'static str,
    target: &'static str,
    amount_seed: u16,
    amount: u128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FuzzStateSnapshot {
    slot: u64,
    vault: u128,
    c_tot: u128,
    insurance: u128,
    pnl_pos_tot: u128,
    asset0_oi_eff_long_q: Option<u128>,
    account_a_capital: u128,
    account_a_pnl: i128,
    account_b_capital: u128,
    account_b_pnl: i128,
}

impl core::fmt::Display for FuzzStateSnapshot {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "slot={} vault={} c_tot={} insurance={} pnl_pos_tot={} asset0_oi_eff_long_q={:?} account_a_capital={} account_a_pnl={} account_b_capital={} account_b_pnl={}",
            self.slot,
            self.vault,
            self.c_tot,
            self.insurance,
            self.pnl_pos_tot,
            self.asset0_oi_eff_long_q,
            self.account_a_capital,
            self.account_a_pnl,
            self.account_b_capital,
            self.account_b_pnl,
        )
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct FuzzActionOutcomeStats {
    successes: usize,
    rollbacks: usize,
}

#[derive(Clone, Debug)]
struct FuzzActionStats {
    actions: [FuzzActionOutcomeStats; FuzzActionKind::COUNT],
    successful_actions: usize,
    rollback_actions: usize,
}

impl Default for FuzzActionStats {
    fn default() -> Self {
        Self {
            actions: [FuzzActionOutcomeStats::default(); FuzzActionKind::COUNT],
            successful_actions: 0,
            rollback_actions: 0,
        }
    }
}

impl FuzzActionStats {
    fn record(&mut self, selector: u8, succeeded: bool) {
        let action = &mut self.actions[FuzzActionKind::from_selector(selector).index()];
        if succeeded {
            action.successes += 1;
            self.successful_actions += 1;
        } else {
            action.rollbacks += 1;
            self.rollback_actions += 1;
        }
    }

    fn total_actions(&self) -> usize {
        self.successful_actions + self.rollback_actions
    }

    fn emit_diagnostics(&self) {
        eprintln!(
            "v16_fuzz_case_summary successful_actions={} rollback_actions={} total_actions={}",
            self.successful_actions,
            self.rollback_actions,
            self.total_actions(),
        );
        for (action_kind, action) in FuzzActionKind::ALL.iter().zip(&self.actions) {
            let total = action.successes + action.rollbacks;
            if total == 0 {
                continue;
            }
            eprintln!(
                "v16_fuzz_action_summary action={} successes={} rollbacks={} total={}",
                action_kind.name(),
                action.successes,
                action.rollbacks,
                total,
            );
        }
    }
}

#[derive(Clone, Debug, Default)]
struct FuzzWindDownStats {
    resolve_attempted: bool,
    resolve_succeeded: bool,
    close_attempts: usize,
    close_progress: usize,
    topup_attempts: usize,
    topup_progress: usize,
    reset_attempts: usize,
    reset_progress: usize,
    closed_accounts: usize,
}

impl FuzzWindDownStats {
    fn emit_diagnostics(&self) {
        eprintln!(
            "v16_fuzz_wind_down_summary resolve_attempted={} resolve_succeeded={} close_attempts={} close_progress={} topup_attempts={} topup_progress={} reset_attempts={} reset_progress={} closed_accounts={}",
            self.resolve_attempted,
            self.resolve_succeeded,
            self.close_attempts,
            self.close_progress,
            self.topup_attempts,
            self.topup_progress,
            self.reset_attempts,
            self.reset_progress,
            self.closed_accounts,
        );
    }
}

fn fuzz_state_snapshot(
    header: &MarketGroupV16HeaderAccount,
    markets: &[Market<u64>],
    account_a: &PortfolioAccountV16Account,
    account_b: &PortfolioAccountV16Account,
) -> FuzzStateSnapshot {
    FuzzStateSnapshot {
        slot: header.current_slot.get(),
        vault: header.vault.get(),
        c_tot: header.c_tot.get(),
        insurance: header.insurance.get(),
        pnl_pos_tot: header.pnl_pos_tot.get(),
        asset0_oi_eff_long_q: markets
            .first()
            .and_then(|market| market.engine.asset.try_to_runtime().ok())
            .map(|asset| asset.oi_eff_long_q),
        account_a_capital: account_a.capital.get(),
        account_a_pnl: account_a.pnl.get(),
        account_b_capital: account_b.capital.get(),
        account_b_pnl: account_b.pnl.get(),
    }
}

#[allow(clippy::too_many_arguments)]
fn run_with_svm_rollback(
    header: &mut MarketGroupV16HeaderAccount,
    markets: &mut Vec<Market<u64>>,
    account_a: &mut PortfolioAccountV16Account,
    account_b: &mut PortfolioAccountV16Account,
    result: Result<(), V16Error>,
    context: FuzzActionContext,
    before: FuzzState,
) -> bool {
    if let Err(error) = result {
        if std::env::var_os("V16_FUZZ_DIAGNOSTICS").is_some() {
            let before_state = fuzz_state_snapshot(
                &before.header,
                &before.markets,
                &before.account_a,
                &before.account_b,
            );
            let failed_state = fuzz_state_snapshot(header, markets, account_a, account_b);
            eprintln!(
                "step={} v16_fuzz_error action={} target={} amount_seed={} amount={} error={:?} before=({}) failed=({})",
                context.step_index,
                context.action,
                context.target,
                context.amount_seed,
                context.amount,
                error,
                before_state,
                failed_state,
            );
        }
        restore_fuzz_state(before, header, markets, account_a, account_b);
        assert_fuzz_invariants(header, markets, account_a, account_b);
        return false;
    }
    if std::env::var_os("V16_FUZZ_DIAGNOSTICS").is_some() {
        let before_state = fuzz_state_snapshot(
            &before.header,
            &before.markets,
            &before.account_a,
            &before.account_b,
        );
        let after_state = fuzz_state_snapshot(header, markets, account_a, account_b);
        eprintln!(
            "step={} v16_fuzz_success action={} target={} amount_seed={} amount={} before=({}) after=({})",
            context.step_index,
            context.action,
            context.target,
            context.amount_seed,
            context.amount,
            before_state,
            after_state,
        );
    }
    assert_fuzz_success_invariants(header, markets, account_a, account_b);
    true
}

fn advance_fuzz_slot(header: &mut MarketGroupV16HeaderAccount, previous_slot: u64) {
    let next_slot = previous_slot.saturating_add(1);
    if header.current_slot.get() < next_slot {
        header.current_slot = V16PodU64::new(next_slot);
    }
}

fn fuzz_account_has_active_asset_leg(
    account: &PortfolioAccountV16Account,
    asset_index: usize,
) -> bool {
    let bitmap = account.active_bitmap.map(V16PodU64::get);
    account.legs.iter().enumerate().any(|(slot, leg)| {
        percolator::active_bitmap_get(bitmap, slot)
            && leg
                .try_to_runtime()
                .map(|leg| leg.active && leg.asset_index as usize == asset_index)
                .unwrap_or(false)
    })
}

fn fuzz_trade_asset_refresh_request(
    market: &MarketGroupV16ViewMut<'_, u64>,
    asset_index: usize,
) -> Result<Option<(u64, u64)>, V16Error> {
    let asset = market.markets[asset_index].engine.asset.try_to_runtime()?;
    let loss_stale = matches!(
        asset.lifecycle,
        AssetLifecycleV16::Active | AssetLifecycleV16::DrainOnly
    ) && (asset.oi_eff_long_q != 0
        || asset.oi_eff_short_q != 0
        || asset.stored_pos_count_long != 0
        || asset.stored_pos_count_short != 0
        || asset.stale_account_count_long != 0
        || asset.stale_account_count_short != 0
        || asset.pending_obligation_count_long != 0
        || asset.pending_obligation_count_short != 0
        || asset.loss_weight_sum_long != 0
        || asset.loss_weight_sum_short != 0)
        && asset.slot_last < market.header.current_slot.get();
    let target_lag = asset.raw_oracle_target_price != asset.effective_price;
    if !loss_stale && !target_lag {
        return Ok(None);
    }
    let now_slot = if asset.slot_last < market.header.current_slot.get() {
        market.header.current_slot.get()
    } else {
        market.header.current_slot.get().saturating_add(1)
    };
    let effective_price = if target_lag {
        asset.raw_oracle_target_price
    } else {
        asset.effective_price
    };
    Ok(Some((now_slot, effective_price)))
}

fn refresh_fuzz_trade_asset_before_trade(
    market: &mut MarketGroupV16ViewMut<'_, u64>,
    account: &mut PortfolioV16ViewMut<'_>,
    asset_index: usize,
) -> Result<(), V16Error> {
    for _ in 0..=FUZZ_MIXED_MAX_ACTIONS {
        let Some((now_slot, effective_price)) =
            fuzz_trade_asset_refresh_request(market, asset_index)?
        else {
            return Ok(());
        };
        market
            .kani_permissionless_crank(
                account,
                PermissionlessCrankRequestV16 {
                    now_slot,
                    asset_index,
                    effective_price,
                    funding_rate_e9: 0,
                    action: PermissionlessCrankActionV16::Refresh,
                },
            )
            .map(|_| ())?;
    }
    Err(V16Error::NonProgress)
}

fn fuzz_account_is_closed(account: &PortfolioAccountV16Account) -> bool {
    percolator::active_bitmap_is_empty(account.active_bitmap.map(V16PodU64::get))
        && account.capital.get() == 0
        && account.pnl.get() == 0
        && account.reserved_pnl.get() == 0
        && account.fee_credits.get() == 0
        && account
            .resolved_payout_receipt
            .try_to_runtime()
            .map(|receipt| !receipt.present)
            .unwrap_or(false)
}

fn assert_fuzz_accounts_closed_after_wind_down(
    stats: &FuzzWindDownStats,
    header: &MarketGroupV16HeaderAccount,
    markets: &[Market<u64>],
    account_a: &PortfolioAccountV16Account,
    account_b: &PortfolioAccountV16Account,
) {
    let account_a_closed = fuzz_account_is_closed(account_a);
    let account_b_closed = fuzz_account_is_closed(account_b);
    assert!(
        account_a_closed && account_b_closed,
        "V16 FUZZ FINAL ACCOUNT CLOSE FAILURE: account_a_closed={account_a_closed} account_b_closed={account_b_closed} stats={stats:?} state=({}) asset={:?} account_a_active_bitmap={:?} account_a_capital={} account_a_pnl={} account_a_reserved_pnl={} account_a_fee_credits={} account_a_receipt={:?} account_b_active_bitmap={:?} account_b_capital={} account_b_pnl={} account_b_reserved_pnl={} account_b_fee_credits={} account_b_receipt={:?}",
        fuzz_state_snapshot(header, markets, account_a, account_b),
        markets
            .first()
            .map(|market| market.engine.asset.try_to_runtime()),
        account_a.active_bitmap.map(V16PodU64::get),
        account_a.capital.get(),
        account_a.pnl.get(),
        account_a.reserved_pnl.get(),
        account_a.fee_credits.get(),
        account_a.resolved_payout_receipt.try_to_runtime(),
        account_b.active_bitmap.map(V16PodU64::get),
        account_b.capital.get(),
        account_b.pnl.get(),
        account_b.reserved_pnl.get(),
        account_b.fee_credits.get(),
        account_b.resolved_payout_receipt.try_to_runtime(),
    );
}

fn try_fuzz_close_resolved_account(
    header: &mut MarketGroupV16HeaderAccount,
    markets: &mut Vec<Market<u64>>,
    account_a: &mut PortfolioAccountV16Account,
    account_b: &mut PortfolioAccountV16Account,
    target_a: bool,
    stats: &mut FuzzWindDownStats,
) -> bool {
    stats.close_attempts += 1;
    let before = clone_fuzz_state(header, markets, account_a, account_b);
    let result = {
        let mut market = MarketGroupV16ViewMut::new(header, markets);
        if target_a {
            let mut account = PortfolioV16ViewMut::new(account_a);
            market
                .close_resolved_account_not_atomic(&mut account, 0)
                .map(|_| ())
        } else {
            let mut account = PortfolioV16ViewMut::new(account_b);
            market
                .close_resolved_account_not_atomic(&mut account, 0)
                .map(|_| ())
        }
    };
    if result.is_err() {
        restore_fuzz_state(before, header, markets, account_a, account_b);
        assert_fuzz_invariants(header, markets, account_a, account_b);
        return false;
    }
    let progressed = fuzz_state_changed(&before, header, markets, account_a, account_b);
    if progressed {
        stats.close_progress += 1;
    }
    assert_fuzz_invariants(header, markets, account_a, account_b);
    progressed
}

fn try_fuzz_claim_resolved_payout_topup(
    header: &mut MarketGroupV16HeaderAccount,
    markets: &mut Vec<Market<u64>>,
    account_a: &mut PortfolioAccountV16Account,
    account_b: &mut PortfolioAccountV16Account,
    target_a: bool,
    stats: &mut FuzzWindDownStats,
) -> bool {
    stats.topup_attempts += 1;
    let before = clone_fuzz_state(header, markets, account_a, account_b);
    let result = {
        let mut market = MarketGroupV16ViewMut::new(header, markets);
        if target_a {
            let mut account = PortfolioV16ViewMut::new(account_a);
            market
                .claim_resolved_payout_topup_not_atomic(&mut account)
                .map(|_| ())
        } else {
            let mut account = PortfolioV16ViewMut::new(account_b);
            market
                .claim_resolved_payout_topup_not_atomic(&mut account)
                .map(|_| ())
        }
    };
    if result.is_err() {
        restore_fuzz_state(before, header, markets, account_a, account_b);
        assert_fuzz_invariants(header, markets, account_a, account_b);
        return false;
    }
    let progressed = fuzz_state_changed(&before, header, markets, account_a, account_b);
    if progressed {
        stats.topup_progress += 1;
    }
    assert_fuzz_invariants(header, markets, account_a, account_b);
    progressed
}

fn try_fuzz_finalize_side_reset(
    header: &mut MarketGroupV16HeaderAccount,
    markets: &mut Vec<Market<u64>>,
    account_a: &mut PortfolioAccountV16Account,
    account_b: &mut PortfolioAccountV16Account,
    side: SideV16,
    stats: &mut FuzzWindDownStats,
) -> bool {
    stats.reset_attempts += 1;
    let before = clone_fuzz_state(header, markets, account_a, account_b);
    let result = {
        let mut market = MarketGroupV16ViewMut::new(header, markets);
        market.finalize_side_reset_not_atomic(0, side)
    };
    if result.is_err() {
        restore_fuzz_state(before, header, markets, account_a, account_b);
        assert_fuzz_invariants(header, markets, account_a, account_b);
        return false;
    }
    let progressed = fuzz_state_changed(&before, header, markets, account_a, account_b);
    if progressed {
        stats.reset_progress += 1;
    }
    assert_fuzz_invariants(header, markets, account_a, account_b);
    progressed
}

fn settle_pay_and_close_fuzz_portfolios(
    header: &mut MarketGroupV16HeaderAccount,
    markets: &mut Vec<Market<u64>>,
    account_a: &mut PortfolioAccountV16Account,
    account_b: &mut PortfolioAccountV16Account,
) -> FuzzWindDownStats {
    let mut stats = FuzzWindDownStats::default();
    stats.resolve_attempted = true;
    let before_resolve = clone_fuzz_state(header, markets, account_a, account_b);
    let resolve_result = {
        let mut market = MarketGroupV16ViewMut::new(header, markets);
        market.resolve_market_not_atomic(market.header.current_slot.get())
    };
    if let Err(err) = resolve_result {
        restore_fuzz_state(before_resolve, header, markets, account_a, account_b);
        assert_fuzz_invariants(header, markets, account_a, account_b);
        panic!(
            "V16 FUZZ FINAL MARKET RESOLVE FAILURE: resolve_market_not_atomic returned {err:?}; stats={stats:?} state=({}) asset={:?}",
            fuzz_state_snapshot(header, markets, account_a, account_b),
            markets
                .first()
                .map(|market| market.engine.asset.try_to_runtime()),
        );
    }
    stats.resolve_succeeded = true;
    assert_fuzz_invariants(header, markets, account_a, account_b);

    let max_rounds = 32;
    for _ in 0..max_rounds {
        let mut progressed = false;
        progressed |= try_fuzz_close_resolved_account(
            header, markets, account_a, account_b, true, &mut stats,
        );
        progressed |= try_fuzz_close_resolved_account(
            header, markets, account_a, account_b, false, &mut stats,
        );
        progressed |= try_fuzz_claim_resolved_payout_topup(
            header, markets, account_a, account_b, true, &mut stats,
        );
        progressed |= try_fuzz_claim_resolved_payout_topup(
            header, markets, account_a, account_b, false, &mut stats,
        );
        progressed |= try_fuzz_finalize_side_reset(
            header,
            markets,
            account_a,
            account_b,
            SideV16::Long,
            &mut stats,
        );
        progressed |= try_fuzz_finalize_side_reset(
            header,
            markets,
            account_a,
            account_b,
            SideV16::Short,
            &mut stats,
        );
        if !progressed {
            break;
        }
    }

    stats.closed_accounts = usize::from(fuzz_account_is_closed(account_a))
        + usize::from(fuzz_account_is_closed(account_b));
    stats
}

fn fuzz_account_repay_amount(account: &PortfolioAccountV16Account) -> u128 {
    let negative_pnl = if account.pnl.get() < 0 {
        account.pnl.get().unsigned_abs()
    } else {
        0
    };
    let fee_debt = if account.fee_credits.get() < 0 {
        account.fee_credits.get().unsigned_abs()
    } else {
        0
    };
    negative_pnl
        .saturating_add(fee_debt)
        .saturating_sub(account.capital.get())
}

fn repay_fuzz_account_debts(
    header: &mut MarketGroupV16HeaderAccount,
    markets: &mut Vec<Market<u64>>,
    account: &mut PortfolioAccountV16Account,
) -> bool {
    let amount = fuzz_account_repay_amount(account);
    if amount == 0 {
        return false;
    }
    {
        let mut market = MarketGroupV16ViewMut::new(header, markets);
        let mut account_view = PortfolioV16ViewMut::new(account);
        market
            .deposit_not_atomic(&mut account_view, amount)
            .unwrap();
    }
    assert_eq!(
        fuzz_account_repay_amount(account),
        0,
        "repay_fuzz_account_debts failed to clear account debt"
    );
    true
}

fn try_fuzz_settle_repaid_account_debt(
    header: &mut MarketGroupV16HeaderAccount,
    markets: &mut Vec<Market<u64>>,
    account_a: &mut PortfolioAccountV16Account,
    account_b: &mut PortfolioAccountV16Account,
    target_a: bool,
) -> bool {
    let before = clone_fuzz_state(header, markets, account_a, account_b);
    let result = {
        let mut market = MarketGroupV16ViewMut::new(header, markets);
        let now_slot = market.header.current_slot.get();
        if target_a {
            let mut account = PortfolioV16ViewMut::new(account_a);
            market
                .sync_account_fee_to_slot_not_atomic(&mut account, now_slot, 0)
                .map(|_| ())
        } else {
            let mut account = PortfolioV16ViewMut::new(account_b);
            market
                .sync_account_fee_to_slot_not_atomic(&mut account, now_slot, 0)
                .map(|_| ())
        }
    };
    if result.is_err() {
        restore_fuzz_state(before, header, markets, account_a, account_b);
        assert_fuzz_invariants(header, markets, account_a, account_b);
        return false;
    }
    let progressed = fuzz_state_changed(&before, header, markets, account_a, account_b);
    assert_fuzz_invariants(header, markets, account_a, account_b);
    progressed
}

fn try_fuzz_forfeit_recovery_position(
    header: &mut MarketGroupV16HeaderAccount,
    markets: &mut Vec<Market<u64>>,
    account_a: &mut PortfolioAccountV16Account,
    account_b: &mut PortfolioAccountV16Account,
    target_a: bool,
) -> bool {
    let has_active_leg = if target_a {
        fuzz_account_has_active_asset_leg(account_a, 0)
    } else {
        fuzz_account_has_active_asset_leg(account_b, 0)
    };
    if !has_active_leg {
        return false;
    }
    let before = clone_fuzz_state(header, markets, account_a, account_b);
    let result = {
        let mut market = MarketGroupV16ViewMut::new(header, markets);
        let b_delta_budget = u128::MAX / 2;
        if target_a {
            let mut account = PortfolioV16ViewMut::new(account_a);
            market
                .forfeit_recovery_leg_not_atomic(&mut account, 0, b_delta_budget)
                .map(|_| ())
        } else {
            let mut account = PortfolioV16ViewMut::new(account_b);
            market
                .forfeit_recovery_leg_not_atomic(&mut account, 0, b_delta_budget)
                .map(|_| ())
        }
    };
    if let Err(error) = result {
        if std::env::var_os("V16_FUZZ_DIAGNOSTICS").is_some() {
            eprintln!(
                "v16_fuzz_wind_down_forfeit_error target={} error={:?} state=({}) asset={:?}",
                if target_a { "account_a" } else { "account_b" },
                error,
                fuzz_state_snapshot(header, markets, account_a, account_b),
                markets
                    .first()
                    .map(|market| market.engine.asset.try_to_runtime())
            );
        }
        restore_fuzz_state(before, header, markets, account_a, account_b);
        assert_fuzz_invariants(header, markets, account_a, account_b);
        return false;
    }
    let progressed = fuzz_state_changed(&before, header, markets, account_a, account_b);
    assert_fuzz_invariants(header, markets, account_a, account_b);
    progressed
}

fn fuzz_repay_close_accounts_complete(
    account_a: &PortfolioAccountV16Account,
    account_b: &PortfolioAccountV16Account,
) -> bool {
    fuzz_account_repay_amount(account_a) == 0
        && fuzz_account_repay_amount(account_b) == 0
        && !fuzz_account_has_active_asset_leg(account_a, 0)
        && !fuzz_account_has_active_asset_leg(account_b, 0)
}

fn fuzz_repay_close_complete(
    markets: &[Market<u64>],
    account_a: &PortfolioAccountV16Account,
    account_b: &PortfolioAccountV16Account,
) -> bool {
    fuzz_repay_close_accounts_complete(account_a, account_b)
        && fuzz_asset_has_no_position_or_loss_state(markets, 0)
}

fn fuzz_market_is_live(header: &MarketGroupV16HeaderAccount) -> bool {
    header.mode == 0
}

fn assert_fuzz_repay_close_complete(
    reason: &str,
    round: usize,
    header: &MarketGroupV16HeaderAccount,
    markets: &[Market<u64>],
    account_a: &PortfolioAccountV16Account,
    account_b: &PortfolioAccountV16Account,
) {
    let account_a_debt = fuzz_account_repay_amount(account_a);
    let account_b_debt = fuzz_account_repay_amount(account_b);
    let account_a_active = fuzz_account_has_active_asset_leg(account_a, 0);
    let account_b_active = fuzz_account_has_active_asset_leg(account_b, 0);
    let asset_clear = fuzz_asset_has_no_position_or_loss_state(markets, 0);
    assert!(
        account_a_debt == 0
            && account_b_debt == 0
            && !account_a_active
            && !account_b_active
            && asset_clear,
        "{reason}: round={round} account_a_debt={account_a_debt} account_b_debt={account_b_debt} account_a_active_asset_leg={account_a_active} account_b_active_asset_leg={account_b_active} asset_clear={asset_clear} state=({}) asset={:?}",
        fuzz_state_snapshot(header, markets, account_a, account_b),
        markets
            .first()
            .map(|market| market.engine.asset.try_to_runtime())
    );
}

fn repay_debts_and_close_fuzz_positions(
    header: &mut MarketGroupV16HeaderAccount,
    markets: &mut Vec<Market<u64>>,
    account_a: &mut PortfolioAccountV16Account,
    account_b: &mut PortfolioAccountV16Account,
) {
    if !fuzz_market_is_live(header) {
        return;
    }

    let mut stats = FuzzWindDownStats::default();
    let max_rounds = 32;
    for round in 0..max_rounds {
        let mut progressed = false;
        progressed |= repay_fuzz_account_debts(header, markets, account_a);
        progressed |= repay_fuzz_account_debts(header, markets, account_b);
        assert_fuzz_invariants(header, markets, account_a, account_b);

        progressed |=
            try_fuzz_settle_repaid_account_debt(header, markets, account_a, account_b, true);
        progressed |=
            try_fuzz_settle_repaid_account_debt(header, markets, account_a, account_b, false);
        progressed |=
            try_fuzz_forfeit_recovery_position(header, markets, account_a, account_b, true);
        progressed |=
            try_fuzz_forfeit_recovery_position(header, markets, account_a, account_b, false);
        progressed |= try_fuzz_close_resolved_account(
            header, markets, account_a, account_b, true, &mut stats,
        );
        progressed |= try_fuzz_close_resolved_account(
            header, markets, account_a, account_b, false, &mut stats,
        );
        progressed |= try_fuzz_claim_resolved_payout_topup(
            header, markets, account_a, account_b, true, &mut stats,
        );
        progressed |= try_fuzz_claim_resolved_payout_topup(
            header, markets, account_a, account_b, false, &mut stats,
        );
        progressed |= try_fuzz_finalize_side_reset(
            header,
            markets,
            account_a,
            account_b,
            SideV16::Long,
            &mut stats,
        );
        progressed |= try_fuzz_finalize_side_reset(
            header,
            markets,
            account_a,
            account_b,
            SideV16::Short,
            &mut stats,
        );
        if fuzz_repay_close_complete(markets, account_a, account_b) {
            assert_fuzz_invariants(header, markets, account_a, account_b);
            return;
        }
        if fuzz_repay_close_accounts_complete(account_a, account_b) {
            assert_fuzz_repay_close_complete(
                "repay_debts_and_close_fuzz_positions cleared account debt/positions but asset state remains",
                round,
                header,
                markets,
                account_a,
                account_b,
            );
        }
        if !progressed {
            assert_fuzz_invariants(header, markets, account_a, account_b);
            return;
        }
    }

    assert_fuzz_repay_close_complete(
        "repay_debts_and_close_fuzz_positions exhausted round budget before completion",
        max_rounds,
        header,
        markets,
        account_a,
        account_b,
    );
    assert_fuzz_invariants(header, markets, account_a, account_b);
}

fn fuzz_asset_has_no_position_or_loss_state(markets: &[Market<u64>], asset_index: usize) -> bool {
    let Ok(asset) = markets[asset_index].engine.asset.try_to_runtime() else {
        return false;
    };
    asset.mode_long == SideModeV16::Normal
        && asset.mode_short == SideModeV16::Normal
        && asset.f_long_num == 0
        && asset.f_short_num == 0
        && asset.k_epoch_start_long == 0
        && asset.k_epoch_start_short == 0
        && asset.f_epoch_start_long_num == 0
        && asset.f_epoch_start_short_num == 0
        && asset.b_long_num == 0
        && asset.b_short_num == 0
        && asset.b_epoch_start_long_num == 0
        && asset.b_epoch_start_short_num == 0
        && asset.oi_eff_long_q == 0
        && asset.oi_eff_short_q == 0
        && asset.stored_pos_count_long == 0
        && asset.stored_pos_count_short == 0
        && asset.stale_account_count_long == 0
        && asset.stale_account_count_short == 0
        && asset.pending_obligation_count_long == 0
        && asset.pending_obligation_count_short == 0
        && asset.loss_weight_sum_long == 0
        && asset.loss_weight_sum_short == 0
        && asset.social_loss_remainder_long_num == 0
        && asset.social_loss_remainder_short_num == 0
        && asset.social_loss_dust_long_num == 0
        && asset.social_loss_dust_short_num == 0
        && asset.explicit_unallocated_loss_long == 0
        && asset.explicit_unallocated_loss_short == 0
}

fn assert_admin_can_close_fuzz_market(
    header: &mut MarketGroupV16HeaderAccount,
    markets: &mut Vec<Market<u64>>,
) {
    let mut market = MarketGroupV16ViewMut::new(header, markets);
    for domain in 0..FUZZ_DOMAIN_COUNT {
        let backing = if domain == 0 {
            market.markets[0]
                .engine
                .backing_long
                .fresh_unliened_backing_num
                .get()
                / BOUND_SCALE
        } else {
            market.markets[0]
                .engine
                .backing_short
                .fresh_unliened_backing_num
                .get()
                / BOUND_SCALE
        };
        if backing != 0 {
            market
                .withdraw_fresh_counterparty_backing_not_atomic(domain, backing)
                .unwrap();
        }

        let earnings = if domain == 0 {
            market.markets[0]
                .engine
                .backing_long
                .utilization_fee_earnings
                .get()
        } else {
            market.markets[0]
                .engine
                .backing_short
                .utilization_fee_earnings
                .get()
        };
        if earnings != 0 {
            market
                .withdraw_backing_provider_earnings_not_atomic(domain, earnings)
                .unwrap();
        }

        let budget = if domain == 0 {
            market.markets[0].engine.insurance_domain_budget_long.get()
        } else {
            market.markets[0].engine.insurance_domain_budget_short.get()
        };
        if budget != 0 {
            market
                .withdraw_domain_insurance_not_atomic(domain, budget)
                .unwrap();
        }
    }

    let close_slot = market.header.current_slot.get();
    let retire_result = market.retire_empty_asset_not_atomic(0, close_slot);
    assert_eq!(
        retire_result,
        Ok(()),
        "V16 FUZZ FINAL MARKET CLOSE FAILURE: retire_empty_asset_not_atomic returned {retire_result:?}; asset={:?} slot={:?}",
        market.markets[0].engine.asset.try_to_runtime().unwrap(),
        market.markets[0].engine,
    );
    let asset = market.markets[0].engine.asset.try_to_runtime().unwrap();
    assert_eq!(asset.lifecycle, AssetLifecycleV16::Retired);
    assert_eq!(asset.retired_slot, close_slot);
    market
        .canonicalize_retired_empty_asset_slot_not_atomic(0)
        .unwrap();
    let asset = market.markets[0].engine.asset.try_to_runtime().unwrap();
    assert_eq!(asset.lifecycle, AssetLifecycleV16::Retired);
    assert_eq!(asset.retired_slot, close_slot);
    assert_eq!(asset.k_long, 0);
    assert_eq!(asset.k_short, 0);
    market.validate_shape().unwrap();
}

#[allow(clippy::too_many_arguments)]
fn apply_fuzz_action(
    header: &mut MarketGroupV16HeaderAccount,
    markets: &mut Vec<Market<u64>>,
    account_a: &mut PortfolioAccountV16Account,
    account_b: &mut PortfolioAccountV16Account,
    step_index: usize,
    selector: u8,
    amount_seed: u16,
    stats: &mut FuzzActionStats,
) -> bool {
    let before = clone_fuzz_state(header, markets, account_a, account_b);
    let action_slot = header.current_slot.get();
    let action_kind = FuzzActionKind::from_selector(selector);
    let target_a = fuzz_target_a(amount_seed);
    let domain = fuzz_domain(amount_seed);
    let amount = (amount_seed as u128) % 128;
    let context = FuzzActionContext {
        step_index,
        action: action_kind.name(),
        target: if action_kind.is_domain_target() {
            if domain == 0 {
                "domain_0"
            } else {
                "domain_1"
            }
        } else if action_kind == FuzzActionKind::FinalizeSideReset {
            match fuzz_side(amount_seed) {
                SideV16::Long => "long",
                SideV16::Short => "short",
            }
        } else if target_a {
            "account_a"
        } else {
            "account_b"
        },
        amount_seed,
        amount,
    };
    let result = match action_kind {
        FuzzActionKind::Deposit => {
            let mut market = MarketGroupV16ViewMut::new(header, markets);
            if target_a {
                let mut account = PortfolioV16ViewMut::new(account_a);
                market.deposit_not_atomic(&mut account, amount)
            } else {
                let mut account = PortfolioV16ViewMut::new(account_b);
                market.deposit_not_atomic(&mut account, amount)
            }
        }
        FuzzActionKind::Withdraw => {
            let mut market = MarketGroupV16ViewMut::new(header, markets);
            if target_a {
                let mut account = PortfolioV16ViewMut::new(account_a);
                market.withdraw_not_atomic(&mut account, amount)
            } else {
                let mut account = PortfolioV16ViewMut::new(account_b);
                market.withdraw_not_atomic(&mut account, amount)
            }
        }
        FuzzActionKind::SyncAccountFeeNextSlot => {
            let fee_slot = header.current_slot.get().saturating_add(1);
            let mut market = MarketGroupV16ViewMut::new(header, markets);
            if target_a {
                let mut account = PortfolioV16ViewMut::new(account_a);
                market
                    .sync_account_fee_to_slot_not_atomic(&mut account, fee_slot, amount)
                    .map(|_| ())
            } else {
                let mut account = PortfolioV16ViewMut::new(account_b);
                market
                    .sync_account_fee_to_slot_not_atomic(&mut account, fee_slot, amount)
                    .map(|_| ())
            }
        }
        FuzzActionKind::FullAccountRefresh => {
            let mut market = MarketGroupV16ViewMut::new(header, markets);
            if target_a {
                let mut account = PortfolioV16ViewMut::new(account_a);
                market
                    .full_account_refresh_not_atomic(&mut account)
                    .map(|_| ())
            } else {
                let mut account = PortfolioV16ViewMut::new(account_b);
                market
                    .full_account_refresh_not_atomic(&mut account)
                    .map(|_| ())
            }
        }
        FuzzActionKind::ExecuteTradeWithFeeLossStaleScoped => {
            let mut market = MarketGroupV16ViewMut::new(header, markets);
            let refresh_with_a = fuzz_account_has_active_asset_leg(account_a, 0)
                || !fuzz_account_has_active_asset_leg(account_b, 0);
            let refresh_result = if refresh_with_a {
                let mut account = PortfolioV16ViewMut::new(account_a);
                refresh_fuzz_trade_asset_before_trade(&mut market, &mut account, 0)
            } else {
                let mut account = PortfolioV16ViewMut::new(account_b);
                refresh_fuzz_trade_asset_before_trade(&mut market, &mut account, 0)
            };
            refresh_result.and_then(|_| {
                let request = TradeRequestV16 {
                    asset_index: 0,
                    size_q: fuzz_trade_size_q(amount_seed),
                    exec_price: 1,
                    fee_bps: (amount_seed as u64) % 11,
                };
                if target_a {
                    let mut long_account = PortfolioV16ViewMut::new(account_a);
                    let mut short_account = PortfolioV16ViewMut::new(account_b);
                    market.execute_trade_with_fee_loss_stale_scoped_not_atomic(
                        &mut long_account,
                        &mut short_account,
                        request,
                    )
                } else {
                    let mut long_account = PortfolioV16ViewMut::new(account_b);
                    let mut short_account = PortfolioV16ViewMut::new(account_a);
                    market.execute_trade_with_fee_loss_stale_scoped_not_atomic(
                        &mut long_account,
                        &mut short_account,
                        request,
                    )
                }
                .map(|_| ())
            })
        }
        FuzzActionKind::PermissionlessCrankRefresh => {
            let mut market = MarketGroupV16ViewMut::new(header, markets);
            if target_a {
                let mut account = PortfolioV16ViewMut::new(account_a);
                market
                    .kani_permissionless_crank(
                        &mut account,
                        PermissionlessCrankRequestV16 {
                            now_slot: market.header.current_slot.get().saturating_add(1),
                            asset_index: 0,
                            effective_price: 1 + (amount_seed as u64),
                            funding_rate_e9: 0,
                            action: PermissionlessCrankActionV16::Refresh,
                        },
                    )
                    .map(|_| ())
            } else {
                let mut account = PortfolioV16ViewMut::new(account_b);
                market
                    .kani_permissionless_crank(
                        &mut account,
                        PermissionlessCrankRequestV16 {
                            now_slot: market.header.current_slot.get().saturating_add(1),
                            asset_index: 0,
                            effective_price: 1 + (amount_seed as u64),
                            funding_rate_e9: 0,
                            action: PermissionlessCrankActionV16::Refresh,
                        },
                    )
                    .map(|_| ())
            }
        }
        FuzzActionKind::PermissionlessCrankSettleB => {
            let mut market = MarketGroupV16ViewMut::new(header, markets);
            if target_a {
                let mut account = PortfolioV16ViewMut::new(account_a);
                market
                    .kani_permissionless_crank(
                        &mut account,
                        PermissionlessCrankRequestV16 {
                            now_slot: market.header.current_slot.get(),
                            asset_index: 0,
                            effective_price: 1,
                            funding_rate_e9: 0,
                            action: PermissionlessCrankActionV16::SettleB { asset_index: 0 },
                        },
                    )
                    .map(|_| ())
            } else {
                let mut account = PortfolioV16ViewMut::new(account_b);
                market
                    .kani_permissionless_crank(
                        &mut account,
                        PermissionlessCrankRequestV16 {
                            now_slot: market.header.current_slot.get(),
                            asset_index: 0,
                            effective_price: 1,
                            funding_rate_e9: 0,
                            action: PermissionlessCrankActionV16::SettleB { asset_index: 0 },
                        },
                    )
                    .map(|_| ())
            }
        }
        FuzzActionKind::SyncAccountFeeCurrentSlot => {
            let mut market = MarketGroupV16ViewMut::new(header, markets);
            if target_a {
                let mut account = PortfolioV16ViewMut::new(account_a);
                market
                    .sync_account_fee_to_slot_not_atomic(
                        &mut account,
                        market.header.current_slot.get(),
                        amount,
                    )
                    .map(|_| ())
            } else {
                let mut account = PortfolioV16ViewMut::new(account_b);
                market
                    .sync_account_fee_to_slot_not_atomic(
                        &mut account,
                        market.header.current_slot.get(),
                        amount,
                    )
                    .map(|_| ())
            }
        }
        FuzzActionKind::ConvertReleasedPnlToCapital => {
            let mut market = MarketGroupV16ViewMut::new(header, markets);
            if target_a {
                let mut account = PortfolioV16ViewMut::new(account_a);
                market.convert_released_pnl_to_capital_not_atomic(&mut account)
            } else {
                let mut account = PortfolioV16ViewMut::new(account_b);
                market.convert_released_pnl_to_capital_not_atomic(&mut account)
            }
            .map(|_| ())
        }
        FuzzActionKind::LiquidateAccount => {
            let liquidate_a = fuzz_liquidation_target(account_a, account_b, target_a);
            if liquidate_a {
                let mut market = MarketGroupV16ViewMut::new(header, markets);
                let mut account = PortfolioV16ViewMut::new(account_a);
                market
                    .liquidate_account_not_atomic(
                        &mut account,
                        LiquidationRequestV16 { asset_index: 0 },
                    )
                    .map(|_| ())
            } else {
                let mut market = MarketGroupV16ViewMut::new(header, markets);
                let mut account = PortfolioV16ViewMut::new(account_b);
                market
                    .liquidate_account_not_atomic(
                        &mut account,
                        LiquidationRequestV16 { asset_index: 0 },
                    )
                    .map(|_| ())
            }
        }
        FuzzActionKind::PermissionlessCrankRecover => {
            let mut market = MarketGroupV16ViewMut::new(header, markets);
            if target_a {
                let mut account = PortfolioV16ViewMut::new(account_a);
                market
                    .kani_permissionless_crank(
                        &mut account,
                        PermissionlessCrankRequestV16 {
                            now_slot: market.header.current_slot.get(),
                            asset_index: 0,
                            effective_price: 1,
                            funding_rate_e9: 0,
                            action: PermissionlessCrankActionV16::Recover(
                                PermissionlessRecoveryReasonV16::ExplicitLossOrDustAuditOverflow,
                            ),
                        },
                    )
                    .map(|_| ())
            } else {
                let mut account = PortfolioV16ViewMut::new(account_b);
                market
                    .kani_permissionless_crank(
                        &mut account,
                        PermissionlessCrankRequestV16 {
                            now_slot: market.header.current_slot.get(),
                            asset_index: 0,
                            effective_price: 1,
                            funding_rate_e9: 0,
                            action: PermissionlessCrankActionV16::Recover(
                                PermissionlessRecoveryReasonV16::ExplicitLossOrDustAuditOverflow,
                            ),
                        },
                    )
                    .map(|_| ())
            }
        }
        FuzzActionKind::ResolveMarket => {
            let mut market = MarketGroupV16ViewMut::new(header, markets);
            market
                .resolve_market_not_atomic(market.header.current_slot.get())
                .map(|_| ())
        }
        FuzzActionKind::CloseResolvedAccount => {
            let mut market = MarketGroupV16ViewMut::new(header, markets);
            if target_a {
                let mut account = PortfolioV16ViewMut::new(account_a);
                market
                    .close_resolved_account_not_atomic(&mut account, 0)
                    .map(|_| ())
            } else {
                let mut account = PortfolioV16ViewMut::new(account_b);
                market
                    .close_resolved_account_not_atomic(&mut account, 0)
                    .map(|_| ())
            }
        }
        FuzzActionKind::DepositDomainInsurance => {
            let mut market = MarketGroupV16ViewMut::new(header, markets);
            market.deposit_domain_insurance_not_atomic(domain, amount)
        }
        FuzzActionKind::WithdrawDomainInsurance => {
            let mut market = MarketGroupV16ViewMut::new(header, markets);
            market.withdraw_domain_insurance_not_atomic(domain, amount)
        }
        FuzzActionKind::DepositFreshCounterpartyBacking => {
            let mut market = MarketGroupV16ViewMut::new(header, markets);
            market.deposit_fresh_counterparty_backing_not_atomic(
                domain,
                1 + amount,
                market.header.current_slot.get().saturating_add(10),
            )
        }
        FuzzActionKind::WithdrawFreshCounterpartyBacking => {
            let mut market = MarketGroupV16ViewMut::new(header, markets);
            market.withdraw_fresh_counterparty_backing_not_atomic(domain, 1 + amount)
        }
        FuzzActionKind::ForfeitRecoveryLeg => {
            let mut market = MarketGroupV16ViewMut::new(header, markets);
            let b_delta_budget = 1 + amount;
            if target_a {
                let mut account = PortfolioV16ViewMut::new(account_a);
                market
                    .forfeit_recovery_leg_not_atomic(&mut account, 0, b_delta_budget)
                    .map(|_| ())
            } else {
                let mut account = PortfolioV16ViewMut::new(account_b);
                market
                    .forfeit_recovery_leg_not_atomic(&mut account, 0, b_delta_budget)
                    .map(|_| ())
            }
        }
        FuzzActionKind::FinalizeSideReset => {
            let mut market = MarketGroupV16ViewMut::new(header, markets);
            market.finalize_side_reset_not_atomic(0, fuzz_side(amount_seed))
        }
        FuzzActionKind::WithdrawBackingProviderEarnings => {
            let mut market = MarketGroupV16ViewMut::new(header, markets);
            let available = if domain == 0 {
                market.markets[0]
                    .engine
                    .backing_long
                    .utilization_fee_earnings
                    .get()
            } else {
                market.markets[0]
                    .engine
                    .backing_short
                    .utilization_fee_earnings
                    .get()
            };
            market.withdraw_backing_provider_earnings_not_atomic(domain, amount.min(available))
        }
        FuzzActionKind::ClaimResolvedPayoutTopup => {
            let mut market = MarketGroupV16ViewMut::new(header, markets);
            if target_a {
                let mut account = PortfolioV16ViewMut::new(account_a);
                market
                    .claim_resolved_payout_topup_not_atomic(&mut account)
                    .map(|_| ())
            } else {
                let mut account = PortfolioV16ViewMut::new(account_b);
                market
                    .claim_resolved_payout_topup_not_atomic(&mut account)
                    .map(|_| ())
            }
        }
    };

    let succeeded = run_with_svm_rollback(
        header, markets, account_a, account_b, result, context, before,
    );
    advance_fuzz_slot(header, action_slot);
    assert_fuzz_invariants(header, markets, account_a, account_b);
    stats.record(selector, succeeded);
    succeeded
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2000))]

    #[test]
    fn v16_fuzz_mixed_lifecycle_actions_preserve_conservation_under_svm_rollback(
        selector_offset in 0u8..FuzzActionKind::SELECTOR_COUNT,
        selector_stride_seed in 0u8..(FUZZ_SELECTOR_STRIDES.len() as u8),
        amount_seeds in prop::collection::vec(
            0u16..512,
            FUZZ_MIXED_MIN_ACTIONS..FUZZ_MIXED_MAX_ACTIONS,
        )
    ) {
        let (mut header, mut markets) = fuzz_group();
        let (_, a_id, b_id, _) = ids();
        let mut account_a = fuzz_account(a_id);
        let mut account_b = fuzz_account(b_id);
        fund_two_accounts(&mut header, &mut markets, &mut account_a, &mut account_b, 1_000);
        assert_fuzz_invariants(
            &mut header,
            &mut markets,
            &account_a,
            &account_b,
        );

        let mut action_stats = FuzzActionStats::default();
        for (step_index, amount_seed) in amount_seeds.into_iter().enumerate() {
            let selector = mixed_lifecycle_selector(
                selector_offset,
                selector_stride_seed,
                step_index,
            );
            apply_fuzz_action(
                &mut header,
                &mut markets,
                &mut account_a,
                &mut account_b,
                step_index,
                selector,
                amount_seed,
                &mut action_stats,
            );
        }
        repay_debts_and_close_fuzz_positions(
            &mut header,
            &mut markets,
            &mut account_a,
            &mut account_b,
        );
        let wind_down_stats = settle_pay_and_close_fuzz_portfolios(
            &mut header,
            &mut markets,
            &mut account_a,
            &mut account_b,
        );
        if std::env::var_os("V16_FUZZ_DIAGNOSTICS").is_some() {
            action_stats.emit_diagnostics();
            wind_down_stats.emit_diagnostics();
        }
        assert_fuzz_accounts_closed_after_wind_down(
            &wind_down_stats,
            &header,
            &markets,
            &account_a,
            &account_b,
        );
        assert_admin_can_close_fuzz_market(&mut header, &mut markets);
    }
}
