#![cfg(kani)]

use percolator::v16::{
    active_bitmap_count_ones, active_bitmap_get, active_bitmap_is_empty,
    backing_domain_fee_split_for_lien_delta_num, kani_active_bitmap_set as active_bitmap_set,
    kani_add_open_interest_for_new_position, kani_apply_backing_provider_earnings_withdraw,
    kani_apply_backing_utilization_fee_charge, kani_apply_resolved_payout_receipt_payment,
    kani_available_backing_num_for_source_credit_state,
    kani_backing_utilization_fee_quote_atoms_for_lien,
    kani_backing_utilization_rate_e9_for_source_state,
    kani_expected_source_credit_rate_num_for_state, kani_health_cert_after_capital_debit,
    kani_health_requirements_from_base_and_target_lag,
    kani_liquidation_close_would_leave_uncovered_loss_with_open_risk,
    kani_liquidation_engine_close_request_q, kani_liquidation_fee_from_raw_fee,
    kani_liquidation_partial_search_hi, kani_liquidation_projected_health_deficit_from_parts,
    kani_liquidation_projected_healthy_after_close, kani_loss_stale_trade_scope_allowed,
    kani_pending_domain_loss_barrier_blocks_position_change, kani_position_delta_increases_risk,
    kani_prepare_asset_recovery_transition, kani_source_credit_state_realizable_support_for_face,
    kani_target_effective_lag_adverse_delta, kani_trade_preflight_risk_gate,
    kani_validate_positive_pnl_source_attribution, AssetLifecycleV16, AssetStateV16,
    AssetStateV16Account, BackingBucketStatusV16, BackingBucketV16, BackingBucketV16Account,
    BatchTradeOutcomeV16, CloseProgressLedgerV16, CloseProgressLedgerV16Account,
    EngineAssetSlotV16Account, HLockLaneV16, HealthCertV16, HealthCertV16Account,
    InsuranceCreditReservationV16, InsuranceCreditReservationV16Account, Market,
    MarketGroupV16HeaderAccount, MarketGroupV16View, MarketGroupV16ViewMut,
    PermissionlessCrankActionV16, PermissionlessCrankRequestV16, PermissionlessProgressOutcomeV16,
    PermissionlessRecoveryReasonV16, PortfolioAccountV16Account, PortfolioLegV16,
    PortfolioLegV16Account, PortfolioSourceDomainV16Account, PortfolioV16View, PortfolioV16ViewMut,
    ProvenanceHeaderV16, ProvenanceHeaderV16Account, ResolvedCloseOutcomeV16,
    ResolvedPayoutLedgerV16, ResolvedPayoutLedgerV16Account, ResolvedPayoutReceiptV16,
    ResolvedPayoutReceiptV16Account, SideModeV16, SideV16, SourceCreditStateV16,
    SourceCreditStateV16Account, StockReconciliationProofV16, TokenValueClassV16,
    TokenValueFlowProofV16, TradeRequestV16, V16Config, V16ConfigAccount, V16Error,
    V16OptionalRecoveryReasonAccount, V16PodI128, V16PodU128, V16PodU32, V16PodU64,
    BACKING_FEE_RATE_DEN_E9, MAX_BACKING_FEE_RATE_E9_PER_SLOT, MAX_BACKING_FEE_UTIL_BPS,
    PORTFOLIO_SOURCE_DOMAIN_CAP, V16_EMPTY_ACTIVE_BITMAP, V16_MAX_PORTFOLIO_ASSETS_N,
};
use percolator::v16::{
    kani_eq_engine_asset_slot_v16_account, kani_eq_market_group_v16_header_account,
    kani_eq_portfolio_account_v16_account,
};
use percolator::{
    ADL_ONE, BOUND_SCALE, CREDIT_RATE_SCALE, MAX_ACCOUNT_NOTIONAL, MAX_MARGIN_BPS,
    MAX_ORACLE_PRICE, MAX_POSITION_ABS_Q, MAX_TRADE_SIZE_Q, MAX_VAULT_TVL, POS_SCALE,
    SOCIAL_LOSS_DEN, V16_ACTIVE_BITMAP_WORDS,
};

fn ids() -> ([u8; 32], [u8; 32], [u8; 32]) {
    ([1; 32], [2; 32], [3; 32])
}

fn empty_account_fixture(market_id: [u8; 32], account_tag: u8) -> PortfolioAccountV16Account {
    let mut account_id = [0u8; 32];
    account_id[0] = account_tag;
    let mut owner = [0u8; 32];
    owner[0] = account_tag;
    let account_header =
        PortfolioAccountV16Account::try_empty(ProvenanceHeaderV16Account::from_runtime(
            &ProvenanceHeaderV16::new(market_id, account_id, owner),
        ))
        .unwrap();
    account_header
}

fn one_market_view_fixture() -> (
    MarketGroupV16HeaderAccount,
    [Market<u64>; 1],
    PortfolioAccountV16Account,
) {
    let (market_id, _, _) = ids();
    let cfg = V16Config::public_user_fund_with_market_slots(1, 1, 0, 10);
    let mut header = MarketGroupV16HeaderAccount::new_dynamic(market_id, cfg, 1, 0).unwrap();
    let mut markets = [Market::new(0u64, EngineAssetSlotV16Account::default())];
    {
        let mut view = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        view.activate_empty_market_not_atomic(0, 100, 1).unwrap();
    }
    let account_header = empty_account_fixture(market_id, 2);
    (header, markets, account_header)
}

fn two_market_view_fixture() -> (
    MarketGroupV16HeaderAccount,
    [Market<u64>; 2],
    PortfolioAccountV16Account,
) {
    let (market_id, _, _) = ids();
    let cfg = V16Config::public_user_fund_with_market_slots(2, 2, 0, 10);
    let mut header = MarketGroupV16HeaderAccount::new_dynamic(market_id, cfg, 2, 0).unwrap();
    let mut markets = [
        Market::new(0u64, EngineAssetSlotV16Account::default()),
        Market::new(0u64, EngineAssetSlotV16Account::default()),
    ];
    {
        let mut view = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        view.activate_empty_market_not_atomic(0, 100, 1).unwrap();
        view.activate_empty_market_not_atomic(1, 100, 2).unwrap();
    }
    let account_header = empty_account_fixture(market_id, 2);
    (header, markets, account_header)
}

fn one_market_only_fixture() -> (MarketGroupV16HeaderAccount, [Market<u64>; 1]) {
    let (market_id, _, _) = ids();
    let cfg = V16Config::public_user_fund_with_market_slots(1, 1, 0, 10);
    let mut header = MarketGroupV16HeaderAccount::new_dynamic(market_id, cfg, 1, 0).unwrap();
    let mut markets = [Market::new(0u64, EngineAssetSlotV16Account::default())];
    {
        let mut view = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        view.activate_empty_market_not_atomic(0, 100, 1).unwrap();
    }
    (header, markets)
}

fn one_market_persisted_slot_fixture() -> (MarketGroupV16HeaderAccount, [Market<u64>; 1]) {
    let (market_id, _, _) = ids();
    let cfg = V16Config::public_user_fund_with_market_slots(1, 1, 0, 10);
    let mut header = MarketGroupV16HeaderAccount::new_dynamic(market_id, cfg, 1, 0).unwrap();
    header.current_slot = V16PodU64::new(1);
    header.slot_last = V16PodU64::new(1);
    header.next_market_id = V16PodU64::new(2);
    header.asset_activation_count = V16PodU64::new(1);
    header.last_asset_activation_slot = V16PodU64::new(1);
    let mut markets = [Market::new(0u64, EngineAssetSlotV16Account::default())];
    let mut asset = AssetStateV16::default();
    asset.market_id = 1;
    asset.raw_oracle_target_price = 100;
    asset.effective_price = 100;
    asset.fund_px_last = 100;
    asset.slot_last = 1;
    markets[0].engine.asset = AssetStateV16Account::from_runtime(&asset);
    (header, markets)
}

fn one_market_direct_view_fixture() -> (
    MarketGroupV16HeaderAccount,
    [Market<u64>; 1],
    PortfolioAccountV16Account,
) {
    let (market_group_id, _, _) = ids();
    let cfg = V16Config::public_user_fund_with_market_slots(1, 1, 0, 10);
    let mut header = MarketGroupV16HeaderAccount::default();
    header.market_group_id = market_group_id;
    header.config = V16ConfigAccount::from_runtime(&cfg);
    header.asset_slot_capacity = V16PodU32::new(1);
    header.asset_activation_count = V16PodU64::new(1);
    header.next_market_id = V16PodU64::new(2);
    header.slot_last = V16PodU64::new(1);
    header.current_slot = V16PodU64::new(1);
    let mut asset = AssetStateV16::default();
    asset.market_id = 1;
    asset.lifecycle = AssetLifecycleV16::Active;
    asset.raw_oracle_target_price = 100;
    asset.effective_price = 100;
    asset.fund_px_last = 100;
    asset.slot_last = 1;
    let mut markets = [Market::new(
        0u64,
        EngineAssetSlotV16Account::empty_for_market(1),
    )];
    markets[0].engine.asset = AssetStateV16Account::from_runtime(&asset);
    (header, markets, PortfolioAccountV16Account::default())
}

fn empty_recovery_slot_for_market(
    market_id: u64,
    price: u64,
    slot_last: u64,
    budget_long: u128,
    budget_short: u128,
) -> EngineAssetSlotV16Account {
    let mut slot = EngineAssetSlotV16Account::empty_for_market(market_id);
    let mut asset = AssetStateV16::default();
    asset.market_id = market_id;
    asset.lifecycle = AssetLifecycleV16::Recovery;
    asset.raw_oracle_target_price = price;
    asset.effective_price = price;
    asset.fund_px_last = price;
    asset.slot_last = slot_last;
    slot.asset = AssetStateV16Account::from_runtime(&asset);
    slot.insurance_domain_budget_long = V16PodU128::new(budget_long);
    slot.insurance_domain_budget_short = V16PodU128::new(budget_short);
    slot
}

#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn proof_v16_active_bitmap_set_get_count_is_exact_and_bounds_checked() {
    let slot_raw: u8 = kani::any();
    let pre_set: bool = kani::any();
    let cap = V16_MAX_PORTFOLIO_ASSETS_N;
    kani::assume((slot_raw as usize) <= cap + 2);
    let slot = slot_raw as usize;
    let mut bitmap = V16_EMPTY_ACTIVE_BITMAP;

    if pre_set && slot < cap {
        active_bitmap_set(&mut bitmap, slot).unwrap();
    }

    let before_word0 = bitmap[0];
    let before_count = active_bitmap_count_ones(bitmap);
    let before_get = active_bitmap_get(bitmap, slot);
    let result = active_bitmap_set(&mut bitmap, slot);
    let after_count = active_bitmap_count_ones(bitmap);

    kani::cover!(
        slot < cap && !before_get,
        "in-bounds unset active leg can be recorded"
    );
    kani::cover!(
        slot < cap && before_get,
        "in-bounds already-active leg set is idempotent"
    );
    kani::cover!(slot >= cap, "out-of-bounds active leg set fails closed");

    // The current v16 account shape has one bitmap word; if that changes, this
    // proof should be widened rather than silently checking only word zero.
    assert_eq!(V16_ACTIVE_BITMAP_WORDS, 1);
    assert!(active_bitmap_is_empty(V16_EMPTY_ACTIVE_BITMAP));

    if slot < cap {
        assert!(result.is_ok());
        assert!(active_bitmap_get(bitmap, slot));
        assert!(!active_bitmap_is_empty(bitmap));
        assert_eq!(bitmap[0], before_word0 | (1u64 << slot));
        if before_get {
            assert_eq!(after_count, before_count);
        } else {
            assert_eq!(after_count, before_count + 1);
        }
    } else {
        assert!(matches!(result, Err(V16Error::InvalidConfig)));
        assert_eq!(bitmap[0], before_word0);
        assert!(!active_bitmap_get(bitmap, slot));
        assert_eq!(after_count, before_count);
    }
}

#[kani::proof]
#[kani::unwind(24)]
#[kani::solver(cadical)]
fn proof_v16_public_finalize_side_reset_success_is_value_neutral() {
    let finalize_long: bool = kani::any();
    let c_tot: u128 = kani::any();
    let insurance: u128 = kani::any();
    let surplus: u128 = kani::any();
    kani::assume(c_tot <= MAX_VAULT_TVL);
    kani::assume(insurance <= MAX_VAULT_TVL - c_tot);
    kani::assume(surplus <= MAX_VAULT_TVL - c_tot - insurance);
    let (mut header, mut markets) = one_market_persisted_slot_fixture();
    header.vault = V16PodU128::new(c_tot + insurance + surplus);
    header.c_tot = V16PodU128::new(c_tot);
    header.insurance = V16PodU128::new(insurance);
    let risk_epoch_before = header.risk_epoch.get();
    let vault_before = header.vault.get();
    let c_tot_before = header.c_tot.get();
    let insurance_before = header.insurance.get();
    let mut asset = markets[0].engine.asset.try_to_runtime().unwrap();
    if finalize_long {
        asset.mode_long = SideModeV16::ResetPending;
    } else {
        asset.mode_short = SideModeV16::ResetPending;
    }
    markets[0].engine.asset = AssetStateV16Account::from_runtime(&asset);

    let side = if finalize_long {
        SideV16::Long
    } else {
        SideV16::Short
    };
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let residual_before = market.kani_residual();
    let result = market.finalize_side_reset_not_atomic(0, side);
    let after = market.markets[0].engine.asset.try_to_runtime().unwrap();

    kani::cover!(
        finalize_long && c_tot > 255 && insurance > 255 && surplus > 255,
        "long ResetPending side finalizes through the public API over wide symbolic value state"
    );
    kani::cover!(
        !finalize_long && c_tot > 255 && insurance > 255 && surplus > 255,
        "short ResetPending side finalizes through the public API over wide symbolic value state"
    );
    assert_eq!(market.header.vault.get(), vault_before);
    assert_eq!(market.header.c_tot.get(), c_tot_before);
    assert_eq!(market.header.insurance.get(), insurance_before);
    assert_eq!(result, Ok(()));
    assert_eq!(market.header.risk_epoch.get(), risk_epoch_before + 1);
    if finalize_long {
        assert_eq!(after.mode_long, SideModeV16::Normal);
    } else {
        assert_eq!(after.mode_short, SideModeV16::Normal);
    }
    // Junior-pool isolation: this transition must not move the junior
    // residual pool.
    assert_eq!(market.kani_residual(), residual_before);
}

#[kani::proof]
#[kani::unwind(16)]
#[kani::solver(cadical)]
fn proof_v16_public_finalize_side_reset_rejects_each_blocker_without_mutation() {
    let finalize_long: bool = kani::any();
    let blocker_kind: u8 = kani::any();
    let c_tot: u128 = kani::any();
    let insurance: u128 = kani::any();
    let surplus: u128 = kani::any();
    kani::assume(blocker_kind <= 3);
    kani::assume(c_tot <= MAX_VAULT_TVL);
    kani::assume(insurance <= MAX_VAULT_TVL - c_tot);
    kani::assume(surplus <= MAX_VAULT_TVL - c_tot - insurance);

    let (mut header, mut markets) = one_market_persisted_slot_fixture();
    header.vault = V16PodU128::new(c_tot + insurance + surplus);
    header.c_tot = V16PodU128::new(c_tot);
    header.insurance = V16PodU128::new(insurance);
    let risk_epoch_before = header.risk_epoch.get();
    let vault_before = header.vault.get();
    let c_tot_before = header.c_tot.get();
    let insurance_before = header.insurance.get();
    let mut asset = markets[0].engine.asset.try_to_runtime().unwrap();
    if finalize_long {
        asset.mode_long = SideModeV16::ResetPending;
        match blocker_kind {
            0 => asset.stored_pos_count_long = 1,
            1 => asset.stale_account_count_long = 1,
            2 => asset.pending_obligation_count_long = 1,
            _ => markets[0].engine.pending_domain_loss_barrier_long = V16PodU64::new(1),
        }
    } else {
        asset.mode_short = SideModeV16::ResetPending;
        match blocker_kind {
            0 => asset.stored_pos_count_short = 1,
            1 => asset.stale_account_count_short = 1,
            2 => asset.pending_obligation_count_short = 1,
            _ => markets[0].engine.pending_domain_loss_barrier_short = V16PodU64::new(1),
        }
    }
    markets[0].engine.asset = AssetStateV16Account::from_runtime(&asset);

    let side = if finalize_long {
        SideV16::Long
    } else {
        SideV16::Short
    };
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let result = market.finalize_side_reset_not_atomic(0, side);
    let after = market.markets[0].engine.asset.try_to_runtime().unwrap();

    kani::cover!(
        blocker_kind == 0 && finalize_long && c_tot > 255 && insurance > 255 && surplus > 255,
        "stored-position blocker prevents public reset finalization over wide symbolic value state"
    );
    kani::cover!(
        blocker_kind == 3 && !finalize_long && c_tot > 255 && insurance > 255 && surplus > 255,
        "pending-barrier blocker prevents public reset finalization over wide symbolic value state"
    );
    assert_eq!(result, Err(V16Error::Stale));
    assert_eq!(market.header.vault.get(), vault_before);
    assert_eq!(market.header.c_tot.get(), c_tot_before);
    assert_eq!(market.header.insurance.get(), insurance_before);
    assert_eq!(market.header.risk_epoch.get(), risk_epoch_before);
    if finalize_long {
        assert_eq!(after.mode_long, SideModeV16::ResetPending);
    } else {
        assert_eq!(after.mode_short, SideModeV16::ResetPending);
    }
}

#[kani::proof]
#[kani::unwind(32)]
#[kani::solver(cadical)]
// A resolved receipt may only migrate its own claim bound from the unreceipted
// pool to the exact pool. Any understated pool fails closed before value moves.
fn proof_v16_resolved_receipt_bound_migration_is_exact_or_fails_closed() {
    let exact_raw: u8 = kani::any();
    let pnl_raw: u8 = kani::any();
    let unreceipted_raw: u8 = kani::any();
    let vault_raw: u8 = kani::any();
    kani::assume(exact_raw <= 8);
    kani::assume((1..=8).contains(&pnl_raw));
    kani::assume(unreceipted_raw <= 16);
    kani::assume(vault_raw <= 32);

    let exact = exact_raw as u128;
    let pnl = pnl_raw as u128;
    let unreceipted = unreceipted_raw as u128;
    let vault = vault_raw as u128;
    let exact_num = exact * BOUND_SCALE;
    let pnl_num = pnl * BOUND_SCALE;
    let unreceipted_num = unreceipted * BOUND_SCALE;
    let ledger_total_num = exact_num + unreceipted_num;
    let true_total = exact + pnl + unreceipted.saturating_sub(pnl);
    let true_total_num = true_total * BOUND_SCALE;
    let rate_num = if ledger_total_num == 0 {
        1
    } else {
        (vault * BOUND_SCALE).min(ledger_total_num)
    };
    let rate_den = if ledger_total_num == 0 {
        1
    } else {
        ledger_total_num
    };

    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    header.mode = 1; // Resolved
    header.current_slot = V16PodU64::new(2);
    header.resolved_slot = V16PodU64::new(2);
    header.vault = V16PodU128::new(vault);
    header.pnl_pos_tot = V16PodU128::new(true_total);
    header.pnl_pos_bound_tot = V16PodU128::new(true_total);
    header.pnl_pos_bound_tot_num = V16PodU128::new(true_total_num);
    header.payout_snapshot = V16PodU128::new(vault);
    header.payout_snapshot_pnl_pos_tot = V16PodU128::new(true_total);
    header.payout_snapshot_captured = 1;
    header.resolved_payout_ledger =
        ResolvedPayoutLedgerV16Account::from_runtime(&ResolvedPayoutLedgerV16 {
            snapshot_residual: vault,
            terminal_claim_exact_receipts_num: exact_num,
            terminal_claim_bound_unreceipted_num: unreceipted_num,
            current_payout_rate_num: rate_num,
            current_payout_rate_den: rate_den,
            snapshot_slot: 2,
            payout_halted: false,
            finalized: false,
        });
    account_header.pnl = V16PodI128::new(pnl as i128);
    account_header.last_fee_slot = V16PodU64::new(2);
    let vault_before = header.vault.get();
    let c_tot_before = header.c_tot.get();
    let insurance_before = header.insurance.get();
    let account_before = account_header;

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    assert_eq!(market.validate_shape(), Ok(()));
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    let result = market.kani_create_resolved_payout_receipt_if_needed(&mut account);
    let ledger = market
        .header
        .resolved_payout_ledger
        .try_to_runtime()
        .unwrap();
    let receipt = account
        .header
        .resolved_payout_receipt
        .try_to_runtime()
        .unwrap();

    kani::cover!(
        unreceipted >= pnl && exact > 0 && unreceipted > pnl && vault < true_total,
        "receipt migration covers prior receipts, remaining claims, and a haircut"
    );
    kani::cover!(
        unreceipted < pnl && unreceipted > 0 && exact > 0,
        "receipt migration rejects a nonzero but insufficient remaining bound"
    );
    kani::cover!(
        unreceipted == 0 && exact == 0 && vault > 0,
        "receipt migration rejects an empty ledger against a positive claim"
    );

    assert_eq!(market.header.vault.get(), vault_before);
    assert_eq!(market.header.c_tot.get(), c_tot_before);
    assert_eq!(market.header.insurance.get(), insurance_before);
    assert_eq!(market.header.pnl_pos_tot.get(), true_total);
    assert_eq!(market.header.pnl_pos_bound_tot.get(), true_total);
    assert_eq!(market.header.pnl_pos_bound_tot_num.get(), true_total_num);
    assert_eq!(account.header.pnl, account_before.pnl);
    assert_eq!(account.header.capital, account_before.capital);
    assert_eq!(account.header.reserved_pnl, account_before.reserved_pnl);
    assert_eq!(account.header.fee_credits, account_before.fee_credits);
    assert_eq!(account.header.last_fee_slot, account_before.last_fee_slot);
    assert_eq!(
        account.header.active_bitmap.map(V16PodU64::get),
        account_before.active_bitmap.map(V16PodU64::get)
    );

    if unreceipted >= pnl {
        assert_eq!(result, Ok(()));
        assert_eq!(
            ledger.terminal_claim_exact_receipts_num,
            exact_num + pnl_num
        );
        assert_eq!(
            ledger.terminal_claim_bound_unreceipted_num,
            unreceipted_num - pnl_num
        );
        assert_eq!(
            ledger.terminal_claim_exact_receipts_num + ledger.terminal_claim_bound_unreceipted_num,
            ledger_total_num
        );
        assert_eq!(ledger.current_payout_rate_num, rate_num);
        assert_eq!(ledger.current_payout_rate_den, rate_den);
        assert!(!ledger.payout_halted);
        assert!(receipt.present);
        assert_eq!(receipt.terminal_positive_claim_face, pnl);
        assert_eq!(receipt.prior_bound_contribution_num, pnl_num);
    } else {
        assert_eq!(result, Err(V16Error::RecoveryRequired));
        assert_eq!(ledger.terminal_claim_exact_receipts_num, exact_num);
        assert_eq!(ledger.terminal_claim_bound_unreceipted_num, unreceipted_num);
        assert_eq!(ledger.current_payout_rate_num, rate_num);
        assert_eq!(ledger.current_payout_rate_den, rate_den);
        assert!(ledger.payout_halted);
        assert!(!receipt.present);
    }
    assert_eq!(market.validate_shape(), Ok(()));
}

#[kani::proof]
#[kani::unwind(64)]
#[kani::solver(cadical)]
fn proof_v16_in_place_account_init_clears_hidden_risk_state_and_validates() {
    let account_tag: u8 = kani::any();
    let owner_tag: u8 = kani::any();
    let dirty_capital_raw: u8 = kani::any();
    let dirty_source_raw: u8 = kani::any();
    kani::assume(dirty_capital_raw != 0);
    kani::assume(dirty_source_raw != 0);

    let (market_id, _, _) = ids();
    let (mut market_header, mut markets) = one_market_only_fixture();
    let mut account_id = [0u8; 32];
    account_id[0] = account_tag;
    let mut owner = [0u8; 32];
    owner[0] = owner_tag;
    let provenance = ProvenanceHeaderV16Account::from_runtime(&ProvenanceHeaderV16::new(
        market_id, account_id, owner,
    ));
    let mut account = PortfolioAccountV16Account::default();

    account.capital = V16PodU128::new(dirty_capital_raw as u128);
    account.pnl = V16PodI128::new(dirty_capital_raw as i128);
    account.active_bitmap = [V16PodU64::new(u64::MAX); V16_ACTIVE_BITMAP_WORDS];
    account.legs[0] = PortfolioLegV16Account::from_runtime(&PortfolioLegV16 {
        active: true,
        asset_index: 0,
        market_id: 1,
        side: SideV16::Long,
        basis_pos_q: POS_SCALE as i128,
        a_basis: ADL_ONE,
        loss_weight: POS_SCALE,
        ..PortfolioLegV16::EMPTY
    });
    account.source_domains[0].domain = V16PodU32::new(dirty_source_raw as u32);
    account.source_domains[0].source_claim_market_id = V16PodU64::new(1);
    account.source_domains[0].source_claim_bound_num = V16PodU128::new(BOUND_SCALE);
    account.stale_state = 2;
    account.b_stale_state = 2;
    account.rebalance_lock = 1;
    account.liquidation_lock = 1;

    account.init_empty_in_place(provenance).unwrap();

    let market = MarketGroupV16ViewMut::new(&mut market_header, &mut markets);
    let account_view = PortfolioV16View::new(&account);
    kani::cover!(
        account_tag != 0 && owner_tag != 0 && dirty_capital_raw > 1 && dirty_source_raw > 1,
        "in-place account init covers dirty pre-state with nontrivial provenance"
    );
    assert_eq!(account.provenance_header, provenance);
    assert_eq!(account.owner, owner);
    assert_eq!(account.capital.get(), 0);
    assert_eq!(account.pnl.get(), 0);
    assert_eq!(account.reserved_pnl.get(), 0);
    assert_eq!(account.fee_credits.get(), 0);
    assert_eq!(account.active_bitmap[0].get(), 0);
    assert!(account.legs[0].try_to_runtime().unwrap().is_empty());
    assert!(account.source_domains[0].kani_is_sparse_tail_default());
    assert_eq!(account.stale_state, 0);
    assert_eq!(account.b_stale_state, 0);
    assert_eq!(account.rebalance_lock, 0);
    assert_eq!(account.liquidation_lock, 0);
    assert_eq!(account_view.validate_with_market(&market.as_view()), Ok(()));
}

#[kani::proof]
#[kani::unwind(64)]
#[kani::solver(cadical)]
fn proof_v16_public_materialized_portfolio_register_is_value_neutral() {
    let count_raw: u8 = kani::any();
    let c_tot_raw: u8 = kani::any();
    let insurance_raw: u8 = kani::any();
    let surplus_raw: u8 = kani::any();
    kani::assume(count_raw < u8::MAX);

    let (mut header, mut markets, account_header) = one_market_view_fixture();
    header.materialized_portfolio_count = V16PodU64::new(count_raw as u64);
    header.c_tot = V16PodU128::new(c_tot_raw as u128);
    header.insurance = V16PodU128::new(insurance_raw as u128);
    header.vault = V16PodU128::new(c_tot_raw as u128 + insurance_raw as u128 + surplus_raw as u128);

    let vault_before = header.vault.get();
    let c_tot_before = header.c_tot.get();
    let insurance_before = header.insurance.get();
    let risk_epoch_before = header.risk_epoch.get();

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let account = PortfolioV16View::new(&account_header);
    let result = market.register_empty_materialized_portfolio_not_atomic(&account);

    kani::cover!(
        count_raw > 2 && c_tot_raw > 2 && insurance_raw > 2 && surplus_raw > 2,
        "materialized portfolio register covers nontrivial senior value state"
    );
    assert_eq!(result, Ok(()));
    assert_eq!(
        market.header.materialized_portfolio_count.get(),
        count_raw as u64 + 1
    );
    assert_eq!(market.header.vault.get(), vault_before);
    assert_eq!(market.header.c_tot.get(), c_tot_before);
    assert_eq!(market.header.insurance.get(), insurance_before);
    assert_eq!(market.header.risk_epoch.get(), risk_epoch_before);
    assert_eq!(
        account.validate_with_market(&market.as_view()),
        Ok(()),
        "registering an empty portfolio must not mutate account safety shape"
    );
}

#[kani::proof]
#[kani::unwind(64)]
#[kani::solver(cadical)]
fn proof_v16_public_materialized_portfolio_deregister_is_value_neutral() {
    let count_raw: u8 = kani::any();
    let c_tot_raw: u8 = kani::any();
    let insurance_raw: u8 = kani::any();
    let surplus_raw: u8 = kani::any();
    kani::assume(count_raw > 0);

    let (mut header, mut markets, account_header) = one_market_view_fixture();
    header.materialized_portfolio_count = V16PodU64::new(count_raw as u64);
    header.c_tot = V16PodU128::new(c_tot_raw as u128);
    header.insurance = V16PodU128::new(insurance_raw as u128);
    header.vault = V16PodU128::new(c_tot_raw as u128 + insurance_raw as u128 + surplus_raw as u128);

    let vault_before = header.vault.get();
    let c_tot_before = header.c_tot.get();
    let insurance_before = header.insurance.get();
    let risk_epoch_before = header.risk_epoch.get();

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let account = PortfolioV16View::new(&account_header);
    let result = market.deregister_empty_materialized_portfolio_not_atomic(&account);

    kani::cover!(
        count_raw > 2 && c_tot_raw > 2 && insurance_raw > 2 && surplus_raw > 2,
        "materialized portfolio deregister covers nontrivial senior value state"
    );
    assert_eq!(result, Ok(()));
    assert_eq!(
        market.header.materialized_portfolio_count.get(),
        count_raw as u64 - 1
    );
    assert_eq!(market.header.vault.get(), vault_before);
    assert_eq!(market.header.c_tot.get(), c_tot_before);
    assert_eq!(market.header.insurance.get(), insurance_before);
    assert_eq!(market.header.risk_epoch.get(), risk_epoch_before);
    assert_eq!(
        account.validate_with_market(&market.as_view()),
        Ok(()),
        "deregistering an empty portfolio must not mutate account safety shape"
    );
}

#[kani::proof]
#[kani::unwind(64)]
#[kani::solver(cadical)]
fn proof_v16_public_materialized_portfolio_register_rejects_value_state_before_mutation() {
    let count_raw: u8 = kani::any();
    let capital_raw: u8 = kani::any();
    let surplus_raw: u8 = kani::any();
    kani::assume(count_raw < u8::MAX);
    kani::assume(capital_raw > 0);

    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    header.materialized_portfolio_count = V16PodU64::new(count_raw as u64);
    header.c_tot = V16PodU128::new(capital_raw as u128);
    header.vault = V16PodU128::new(capital_raw as u128 + surplus_raw as u128);
    account_header.capital = V16PodU128::new(capital_raw as u128);

    let count_before = header.materialized_portfolio_count.get();
    let vault_before = header.vault.get();
    let c_tot_before = header.c_tot.get();
    let insurance_before = header.insurance.get();

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let account = PortfolioV16View::new(&account_header);
    let result = market.register_empty_materialized_portfolio_not_atomic(&account);

    kani::cover!(
        count_raw > 2 && capital_raw > 2 && surplus_raw > 2,
        "materialized portfolio register rejection covers nontrivial account capital"
    );
    assert_eq!(result, Err(V16Error::LockActive));
    assert_eq!(
        market.header.materialized_portfolio_count.get(),
        count_before
    );
    assert_eq!(market.header.vault.get(), vault_before);
    assert_eq!(market.header.c_tot.get(), c_tot_before);
    assert_eq!(market.header.insurance.get(), insurance_before);
}

#[kani::proof]
#[kani::unwind(64)]
#[kani::solver(cadical)]
fn proof_v16_public_materialized_portfolio_deregister_rejects_value_state_before_mutation() {
    let count_raw: u8 = kani::any();
    let capital_raw: u8 = kani::any();
    let surplus_raw: u8 = kani::any();
    kani::assume(count_raw > 0);
    kani::assume(capital_raw > 0);

    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    header.materialized_portfolio_count = V16PodU64::new(count_raw as u64);
    header.c_tot = V16PodU128::new(capital_raw as u128);
    header.vault = V16PodU128::new(capital_raw as u128 + surplus_raw as u128);
    account_header.capital = V16PodU128::new(capital_raw as u128);

    let count_before = header.materialized_portfolio_count.get();
    let vault_before = header.vault.get();
    let c_tot_before = header.c_tot.get();
    let insurance_before = header.insurance.get();

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let account = PortfolioV16View::new(&account_header);
    let result = market.deregister_empty_materialized_portfolio_not_atomic(&account);

    kani::cover!(
        count_raw > 2 && capital_raw > 2 && surplus_raw > 2,
        "materialized portfolio deregister rejection covers nontrivial account capital"
    );
    assert_eq!(result, Err(V16Error::LockActive));
    assert_eq!(
        market.header.materialized_portfolio_count.get(),
        count_before
    );
    assert_eq!(market.header.vault.get(), vault_before);
    assert_eq!(market.header.c_tot.get(), c_tot_before);
    assert_eq!(market.header.insurance.get(), insurance_before);
}

#[kani::proof]
#[kani::unwind(64)]
#[kani::solver(cadical)]
fn proof_v16_public_raw_oracle_target_update_is_value_neutral() {
    let target: u16 = kani::any();
    let c_tot: u128 = kani::any();
    let insurance: u128 = kani::any();
    let surplus: u128 = kani::any();
    kani::assume((1..=10_000).contains(&target));
    kani::assume(c_tot <= MAX_VAULT_TVL);
    kani::assume(insurance <= MAX_VAULT_TVL - c_tot);
    kani::assume(surplus <= MAX_VAULT_TVL - c_tot - insurance);
    let (mut header, mut markets, _) = one_market_view_fixture();
    header.vault = V16PodU128::new(c_tot + insurance + surplus);
    header.c_tot = V16PodU128::new(c_tot);
    header.insurance = V16PodU128::new(insurance);
    let vault_before = header.vault.get();
    let c_tot_before = header.c_tot.get();
    let insurance_before = header.insurance.get();
    let effective_before = markets[0].engine.asset.effective_price.get();

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let residual_before = market.kani_residual();
    let res = market.set_asset_raw_oracle_target_not_atomic(0, target as u64);

    kani::cover!(
        target > 100 && c_tot > 255 && insurance > 255 && surplus > 255,
        "raw target update covers nontrivial target/effective lag over wide symbolic value state"
    );
    assert_eq!(res, Ok(()));
    assert_eq!(
        market.markets[0].engine.asset.raw_oracle_target_price.get(),
        target as u64
    );
    assert_eq!(
        market.markets[0].engine.asset.effective_price.get(),
        effective_before
    );
    assert_eq!(market.header.vault.get(), vault_before);
    assert_eq!(market.header.c_tot.get(), c_tot_before);
    assert_eq!(market.header.insurance.get(), insurance_before);
    // Junior-pool isolation: this transition must not move the junior
    // residual pool (haircut/payout funding).
    assert_eq!(market.kani_residual(), residual_before);
}

#[kani::proof]
#[kani::solver(cadical)]
fn proof_v16_asset_recovery_transition_freezes_price_and_bumps_once() {
    let raw_price: u16 = kani::any();
    let use_drain_only: bool = kani::any();
    let asset_set_epoch: u8 = kani::any();
    let risk_epoch: u8 = kani::any();
    kani::assume((1..=10_000).contains(&raw_price));
    let mut asset = AssetStateV16 {
        lifecycle: if use_drain_only {
            AssetLifecycleV16::DrainOnly
        } else {
            AssetLifecycleV16::Active
        },
        raw_oracle_target_price: 1,
        effective_price: raw_price as u64,
        fund_px_last: raw_price as u64,
        ..AssetStateV16::default()
    };
    let (next, next_asset_set_epoch, next_risk_epoch) =
        kani_prepare_asset_recovery_transition(asset, asset_set_epoch as u64, risk_epoch as u64)
            .unwrap();
    asset.lifecycle = AssetLifecycleV16::Recovery;
    asset.raw_oracle_target_price = asset.effective_price;

    kani::cover!(
        use_drain_only && raw_price > 100,
        "asset recovery transition covers drain-only asset with nontrivial price"
    );
    assert_eq!(next, asset);
    assert_eq!(next_asset_set_epoch, asset_set_epoch as u64 + 1);
    assert_eq!(next_risk_epoch, risk_epoch as u64 + 1);
}

#[kani::proof]
#[kani::solver(cadical)]
fn proof_v16_asset_recovery_transition_is_idempotent_after_recovery() {
    let raw_price: u16 = kani::any();
    let asset_set_epoch: u8 = kani::any();
    let risk_epoch: u8 = kani::any();
    kani::assume((1..=10_000).contains(&raw_price));
    let asset = AssetStateV16 {
        lifecycle: AssetLifecycleV16::Recovery,
        raw_oracle_target_price: raw_price as u64,
        effective_price: raw_price as u64,
        fund_px_last: raw_price as u64,
        ..AssetStateV16::default()
    };
    let (next, next_asset_set_epoch, next_risk_epoch) =
        kani_prepare_asset_recovery_transition(asset, asset_set_epoch as u64, risk_epoch as u64)
            .unwrap();

    kani::cover!(
        raw_price > 100,
        "asset recovery transition covers nontrivial idempotent recovery price"
    );
    assert_eq!(next, asset);
    assert_eq!(next_asset_set_epoch, asset_set_epoch as u64);
    assert_eq!(next_risk_epoch, risk_epoch as u64);
}

#[kani::proof]
#[kani::solver(cadical)]
fn proof_v16_loss_stale_trade_scope_allows_only_unrelated_current_assets() {
    let market_loss_stale_active: bool = kani::any();
    let trade_asset_loss_stale: bool = kani::any();
    let long_account_loss_stale_exposed: bool = kani::any();
    let short_account_loss_stale_exposed: bool = kani::any();
    let allowed = kani_loss_stale_trade_scope_allowed(
        market_loss_stale_active,
        trade_asset_loss_stale,
        long_account_loss_stale_exposed,
        short_account_loss_stale_exposed,
    );

    kani::cover!(
        allowed,
        "loss-stale scoped trade allows the unrelated-current branch"
    );
    kani::cover!(
        market_loss_stale_active
            && (trade_asset_loss_stale
                || long_account_loss_stale_exposed
                || short_account_loss_stale_exposed)
            && !allowed,
        "loss-stale scoped trade denies stale asset or stale account exposure"
    );
    assert_eq!(
        allowed,
        market_loss_stale_active
            && !trade_asset_loss_stale
            && !long_account_loss_stale_exposed
            && !short_account_loss_stale_exposed
    );
}

#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
fn proof_v16_sparse_source_domain_insert_roundtrips_occupied_domain() {
    let domain_raw: u8 = kani::any();
    let claim_raw: u8 = kani::any();
    kani::assume(domain_raw < 64);
    kani::assume(claim_raw > 0);
    let domain = domain_raw as usize;
    let claim_num = claim_raw as u128 * BOUND_SCALE;
    let (_, _, mut account_header) = one_market_view_fixture();

    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    let slot = account.kani_source_domain_slot_or_insert(domain).unwrap();
    account.header.source_domains[slot].source_claim_market_id = V16PodU64::new(1);
    account.header.source_domains[slot].source_claim_bound_num = V16PodU128::new(claim_num);

    let view = account.as_view();
    let found = view.kani_source_domain_slot(domain).unwrap();
    let source = view.kani_source_domain(domain).unwrap();

    kani::cover!(
        domain > 1 && claim_raw > 1,
        "sparse source-domain lookup covers nontrivial domain and claim"
    );
    assert_eq!(found, Some(slot));
    assert_eq!(source.domain.get(), domain_raw as u32);
    assert_eq!(source.source_claim_bound_num.get(), claim_num);
}

#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
fn proof_v16_source_domain_insert_reuses_same_domain_market_id_tag() {
    let claim_raw: u8 = kani::any();
    kani::assume(claim_raw > 0);
    let claim_num = claim_raw as u128 * BOUND_SCALE;
    let (_, _, mut account_header) = one_market_view_fixture();

    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    let first = account.kani_source_domain_slot_or_insert(1).unwrap();
    account.header.source_domains[first].source_claim_market_id = V16PodU64::new(1);
    let second = account.kani_source_domain_slot_or_insert(1).unwrap();
    account.header.source_domains[second].source_claim_bound_num = V16PodU128::new(claim_num);

    kani::cover!(
        claim_raw > 1,
        "source-domain insert reuses a same-domain market-id tag before the claim becomes occupied"
    );
    assert_eq!(first, second);
    assert_eq!(account.header.source_domains[first].domain.get(), 1);
    assert_eq!(
        account.header.source_domains[first]
            .source_claim_market_id
            .get(),
        1
    );
    assert_eq!(
        account.header.source_domains[first]
            .source_claim_bound_num
            .get(),
        claim_num
    );
    assert_eq!(
        account.as_view().kani_source_domain_slot(1),
        Ok(Some(first))
    );
}

#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
fn proof_v16_sparse_source_domain_cap_full_rejects_new_domain() {
    let domain_offset: u8 = kani::any();
    let (_, _, mut account_header) = one_market_view_fixture();
    let mut i = 0usize;
    while i < PORTFOLIO_SOURCE_DOMAIN_CAP {
        account_header.source_domains[i].domain = V16PodU32::new(i as u32);
        account_header.source_domains[i].source_claim_market_id = V16PodU64::new(1);
        account_header.source_domains[i].source_claim_bound_num = V16PodU128::new(BOUND_SCALE);
        i += 1;
    }
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    let rejected = account
        .kani_source_domain_slot_or_insert(PORTFOLIO_SOURCE_DOMAIN_CAP + domain_offset as usize);

    kani::cover!(
        domain_offset > 0,
        "sparse source-domain cap-full rejection covers symbolic new domain"
    );
    assert_eq!(rejected, Err(V16Error::LockActive));
}

#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
fn proof_v16_sparse_source_domain_validation_rejects_duplicate_occupied_domain() {
    let claim_raw: u8 = kani::any();
    kani::assume((1..=8).contains(&claim_raw));
    let claim_num = claim_raw as u128 * BOUND_SCALE;
    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    account_header.pnl = V16PodI128::new(claim_raw as i128 * 2);
    header.pnl_pos_tot = V16PodU128::new(claim_raw as u128 * 2);
    header.pnl_pos_bound_tot_num = V16PodU128::new(claim_num * 2);
    header.pnl_pos_bound_tot = V16PodU128::new(claim_raw as u128 * 2);
    markets[0].engine.source_credit_long =
        SourceCreditStateV16Account::from_runtime(&SourceCreditStateV16 {
            positive_claim_bound_num: claim_num * 2,
            exact_positive_claim_num: claim_num * 2,
            fresh_reserved_backing_num: claim_num * 2,
            credit_rate_num: CREDIT_RATE_SCALE,
            ..SourceCreditStateV16::EMPTY
        });
    account_header.source_domains[0].domain = V16PodU32::new(0);
    account_header.source_domains[0].source_claim_market_id = V16PodU64::new(1);
    account_header.source_domains[0].source_claim_bound_num = V16PodU128::new(claim_num);
    let mut single_header = account_header;
    single_header.pnl = V16PodI128::new(claim_raw as i128);
    account_header.source_domains[1].domain = V16PodU32::new(0);
    account_header.source_domains[1].source_claim_market_id = V16PodU64::new(1);
    account_header.source_domains[1].source_claim_bound_num = V16PodU128::new(claim_num);

    let market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let single = PortfolioV16ViewMut::new(&mut single_header);
    let account = PortfolioV16ViewMut::new(&mut account_header);
    let accepted = single
        .as_view()
        .kani_validate_source_credit_shape_with_market(&market.as_view());
    let rejected = account
        .as_view()
        .kani_validate_source_credit_shape_with_market(&market.as_view());

    kani::cover!(
        claim_raw > 1,
        "duplicate sparse source-domain validation rejects nontrivial duplicate claims"
    );
    assert_eq!(accepted, Ok(()));
    assert!(rejected.is_err());
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_sparse_source_domain_validation_rejects_unoccupied_tagged_slot() {
    let domain_raw: u8 = kani::any();
    kani::assume((1..=63).contains(&domain_raw));
    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    account_header.source_domains[1].domain = V16PodU32::new(domain_raw as u32);
    account_header.source_domains[1].source_claim_market_id = V16PodU64::new(1);

    let market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let account = PortfolioV16View::new(&account_header);
    let rejected = account.kani_validate_source_credit_shape_with_market(&market.as_view());

    kani::cover!(
        domain_raw > 1,
        "sparse source-domain validation rejects nontrivial unoccupied tagged slot"
    );
    assert_eq!(rejected, Err(V16Error::HiddenLeg));
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_sparse_source_domain_validation_accepts_domain_indexed_claim() {
    let claim_raw: u8 = kani::any();
    kani::assume((1..=16).contains(&claim_raw));
    let claim_num = claim_raw as u128 * BOUND_SCALE;
    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    account_header.pnl = V16PodI128::new(claim_raw as i128);
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

    let market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let account = PortfolioV16View::new(&account_header);

    kani::cover!(
        claim_raw > 1,
        "source-domain validation accepts nontrivial domain-indexed persisted claim"
    );
    assert_eq!(account.kani_source_domain_slot(1), Ok(Some(1)));
    assert_eq!(
        account.kani_validate_source_credit_shape_with_market(&market.as_view()),
        Ok(())
    );
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_mutable_view_compacts_persisted_source_domain_tail() {
    let claim_raw: u8 = kani::any();
    kani::assume(claim_raw > 0);
    let claim_num = claim_raw as u128 * BOUND_SCALE;
    let (_, _, mut account_header) = one_market_view_fixture();
    account_header.pnl = V16PodI128::new(claim_raw as i128);
    account_header.source_domains[0].domain = V16PodU32::new(0);
    account_header.source_domains[0].source_claim_market_id = V16PodU64::new(1);
    account_header.source_domains[1].domain = V16PodU32::new(0);
    account_header.source_domains[1].source_claim_market_id = V16PodU64::new(1);
    account_header.source_domains[1].source_claim_bound_num = V16PodU128::new(claim_num);

    let account = PortfolioV16ViewMut::new(&mut account_header);

    kani::cover!(
        claim_raw > 1,
        "mutable view construction compacts a nontrivial persisted source-domain tail"
    );
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
    let view = account.as_view();
    assert_eq!(view.kani_source_domain_slot(0), Ok(Some(0)));
    let source = view.kani_source_domain(0).unwrap();
    assert_eq!(source.source_claim_market_id.get(), 1);
    assert_eq!(source.source_claim_bound_num.get(), claim_num);
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_view_deposit_preserves_c_tot_vault_capital_sum() {
    let start_capital_raw: u8 = kani::any();
    let other_capital_raw: u8 = kani::any();
    let insurance_raw: u8 = kani::any();
    let surplus_raw: u8 = kani::any();
    let amount_raw: u8 = kani::any();
    kani::assume(start_capital_raw <= 16);
    kani::assume(other_capital_raw <= 16);
    kani::assume(insurance_raw <= 16);
    kani::assume(surplus_raw <= 16);
    kani::assume(amount_raw <= 16);
    let start_capital = start_capital_raw as u128;
    let other_capital = other_capital_raw as u128;
    let insurance = insurance_raw as u128;
    let surplus = surplus_raw as u128;
    let amount = amount_raw as u128;
    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    let c_tot_before = start_capital + other_capital;
    let vault_before = c_tot_before + insurance + surplus;
    header.c_tot = V16PodU128::new(c_tot_before);
    header.insurance = V16PodU128::new(insurance);
    header.vault = V16PodU128::new(vault_before);
    account_header.capital = V16PodU128::new(start_capital);
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    let residual_before = market.kani_residual();

    market.deposit_not_atomic(&mut account, amount).unwrap();

    kani::cover!(
        amount > 0 && start_capital > 0 && other_capital > 0 && insurance > 0 && surplus > 0,
        "view deposit covers nonzero deposit into nonempty senior state"
    );
    kani::cover!(
        amount == 0 && start_capital > 0 && other_capital > 0,
        "view deposit covers zero-amount no-op over nonempty senior state"
    );
    assert_eq!(account.header.capital.get(), start_capital + amount);
    assert_eq!(market.header.c_tot.get(), c_tot_before + amount);
    assert_eq!(market.header.vault.get(), vault_before + amount);
    assert_eq!(market.header.insurance.get(), insurance);
    assert_eq!(
        market.header.vault.get() - market.header.c_tot.get() - market.header.insurance.get(),
        surplus
    );
    // Junior-pool isolation: deposits move vault and c_tot in lockstep, so a
    // user can never inflate the junior residual pool (haircut/payout funding)
    // by depositing.
    assert_eq!(market.kani_residual(), residual_before);
    assert_eq!(market.validate_shape(), Ok(()));
    assert_eq!(account.validate_with_market(&market.as_view()), Ok(()));
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_public_market_activation_starts_domains_unfunded_and_value_neutral() {
    let c_tot_raw: u8 = kani::any();
    let insurance_raw: u8 = kani::any();
    let price_raw: u16 = kani::any();
    let slot_raw: u8 = kani::any();
    kani::assume((1..=10_000).contains(&price_raw));
    kani::assume(slot_raw > 0);
    let c_tot = c_tot_raw as u128;
    let insurance = insurance_raw as u128;
    let initial_price = price_raw as u64;
    let activation_slot = slot_raw as u64;
    let (market_id, _, _) = ids();
    let cfg = V16Config::public_user_fund_with_market_slots(1, 1, 0, 10);
    let mut header = MarketGroupV16HeaderAccount::new_dynamic(market_id, cfg, 1, 0).unwrap();
    header.vault = V16PodU128::new(c_tot + insurance);
    header.c_tot = V16PodU128::new(c_tot);
    header.insurance = V16PodU128::new(insurance);
    let mut markets = [Market::new(0u64, EngineAssetSlotV16Account::default())];
    let vault_before = header.vault;
    let c_tot_before = header.c_tot;
    let insurance_before = header.insurance;

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    market
        .activate_empty_market_not_atomic(0, initial_price, activation_slot)
        .unwrap();
    let slot = &market.markets[0].engine;
    let asset = slot.asset.try_to_runtime().unwrap();

    kani::cover!(
        c_tot > 0 && insurance > 0 && initial_price > 100 && activation_slot > 1,
        "public market activation covers nonzero symbolic senior-balance case"
    );
    assert_eq!(asset.lifecycle, AssetLifecycleV16::Active);
    assert_eq!(asset.market_id, 1);
    assert_eq!(asset.effective_price, initial_price);
    assert_eq!(market.header.vault, vault_before);
    assert_eq!(market.header.c_tot, c_tot_before);
    assert_eq!(market.header.insurance, insurance_before);
    assert_eq!(slot.insurance_domain_budget_long.get(), 0);
    assert_eq!(slot.insurance_domain_budget_short.get(), 0);
    assert_eq!(slot.insurance_domain_spent_long.get(), 0);
    assert_eq!(slot.insurance_domain_spent_short.get(), 0);
    assert_eq!(
        slot.source_credit_long.try_to_runtime().unwrap(),
        SourceCreditStateV16::EMPTY
    );
    assert_eq!(
        slot.source_credit_short.try_to_runtime().unwrap(),
        SourceCreditStateV16::EMPTY
    );
    assert_eq!(
        slot.insurance_reservation_long.try_to_runtime().unwrap(),
        InsuranceCreditReservationV16::EMPTY
    );
    assert_eq!(
        slot.insurance_reservation_short.try_to_runtime().unwrap(),
        InsuranceCreditReservationV16::EMPTY
    );
    assert_eq!(market.validate_shape(), Ok(()));
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_public_market_capacity_growth_is_monotone_and_value_neutral() {
    let growth_raw: u8 = kani::any();
    let c_tot: u128 = kani::any();
    let insurance: u128 = kani::any();
    let surplus: u128 = kani::any();
    kani::assume(c_tot <= MAX_VAULT_TVL);
    kani::assume(insurance <= MAX_VAULT_TVL - c_tot);
    kani::assume(surplus <= MAX_VAULT_TVL - c_tot - insurance);
    let new_capacity = 1 + growth_raw as u32;
    let (market_id, _, _) = ids();
    let cfg = V16Config::public_user_fund_with_market_slots(1, 1, 0, 10);
    let mut header = MarketGroupV16HeaderAccount::new_dynamic(market_id, cfg, 1, 0).unwrap();
    header.vault = V16PodU128::new(c_tot + insurance + surplus);
    header.c_tot = V16PodU128::new(c_tot);
    header.insurance = V16PodU128::new(insurance);
    let vault_before = header.vault;
    let c_tot_before = header.c_tot;
    let insurance_before = header.insurance;
    let asset_set_epoch_before = header.asset_set_epoch.get();
    let risk_epoch_before = header.risk_epoch.get();

    header
        .grow_asset_slot_capacity_not_atomic(new_capacity, new_capacity)
        .unwrap();
    let config = header.config.try_to_runtime_shape().unwrap();

    kani::cover!(
        new_capacity > 1 && c_tot > 255 && insurance > 255 && surplus > 255,
        "public market capacity growth covers actual growth over wide symbolic value state"
    );
    assert_eq!(header.asset_slot_capacity.get(), new_capacity);
    assert_eq!(config.max_market_slots, new_capacity);
    assert_eq!(header.vault, vault_before);
    assert_eq!(header.c_tot, c_tot_before);
    assert_eq!(header.insurance, insurance_before);
    assert_eq!(header.asset_set_epoch.get(), asset_set_epoch_before + 1);
    assert_eq!(header.risk_epoch.get(), risk_epoch_before + 1);
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_public_market_capacity_growth_rejects_invalid_requests_without_mutation() {
    let blocker: u8 = kani::any();
    let c_tot: u128 = kani::any();
    let insurance: u128 = kani::any();
    let surplus: u128 = kani::any();
    kani::assume(blocker <= 5);
    kani::assume(c_tot <= MAX_VAULT_TVL);
    kani::assume(insurance <= MAX_VAULT_TVL - c_tot);
    kani::assume(surplus <= MAX_VAULT_TVL - c_tot - insurance);

    let (market_id, _, _) = ids();
    let cfg = V16Config::public_user_fund_with_market_slots(2, 2, 0, 10);
    let mut header = MarketGroupV16HeaderAccount::new_dynamic(market_id, cfg, 2, 0).unwrap();
    header.vault = V16PodU128::new(c_tot + insurance + surplus);
    header.c_tot = V16PodU128::new(c_tot);
    header.insurance = V16PodU128::new(insurance);

    let (new_capacity, new_max_market_slots, expected_err) = match blocker {
        0 => {
            header.mode = 1;
            (3, 3, V16Error::InvalidConfig)
        }
        1 => (1, 1, V16Error::InvalidConfig),
        2 => (3, 1, V16Error::InvalidConfig),
        3 => (3, 4, V16Error::InvalidConfig),
        4 => {
            header.asset_set_epoch = V16PodU64::new(u64::MAX);
            (3, 3, V16Error::CounterOverflow)
        }
        _ => {
            header.risk_epoch = V16PodU64::new(u64::MAX);
            (3, 3, V16Error::CounterOverflow)
        }
    };

    let vault_before = header.vault;
    let c_tot_before = header.c_tot;
    let insurance_before = header.insurance;
    let config_before = header.config;
    let capacity_before = header.asset_slot_capacity;
    let mode_before = header.mode;
    let asset_set_epoch_before = header.asset_set_epoch;
    let risk_epoch_before = header.risk_epoch;

    let result = header.grow_asset_slot_capacity_not_atomic(new_capacity, new_max_market_slots);

    kani::cover!(
        blocker == 0 && c_tot > 255 && insurance > 255 && surplus > 255,
        "capacity growth rejects non-live market without value mutation"
    );
    kani::cover!(
        blocker == 1,
        "capacity growth rejects shrinking allocated capacity"
    );
    kani::cover!(
        blocker == 2,
        "capacity growth rejects shrinking configured market slots"
    );
    kani::cover!(
        blocker == 3,
        "capacity growth rejects configured market slots beyond allocated capacity"
    );
    kani::cover!(
        blocker == 4,
        "capacity growth rejects asset-set epoch overflow before mutation"
    );
    kani::cover!(
        blocker == 5,
        "capacity growth rejects risk epoch overflow before mutation"
    );
    assert_eq!(result, Err(expected_err));
    assert_eq!(header.vault, vault_before);
    assert_eq!(header.c_tot, c_tot_before);
    assert_eq!(header.insurance, insurance_before);
    assert_eq!(header.config, config_before);
    assert_eq!(header.asset_slot_capacity, capacity_before);
    assert_eq!(header.mode, mode_before);
    assert_eq!(header.asset_set_epoch, asset_set_epoch_before);
    assert_eq!(header.risk_epoch, risk_epoch_before);
}

#[kani::proof]
#[kani::unwind(32)]
#[kani::solver(cadical)]
fn proof_v16_retired_slot_reactivation_accepts_only_empty_source_credit_amounts() {
    let old_market_id_raw: u8 = kani::any();
    let credit_epoch_raw: u8 = kani::any();
    let zero_credit_rate: bool = kani::any();
    let claim_units_raw: u8 = kani::any();
    let c_tot_raw: u8 = kani::any();
    let insurance_raw: u8 = kani::any();
    let surplus_raw: u8 = kani::any();
    let price_raw: u16 = kani::any();
    kani::assume(old_market_id_raw != 0);
    kani::assume(credit_epoch_raw != 0);
    kani::assume(claim_units_raw <= 8);
    kani::assume(c_tot_raw <= 8);
    kani::assume(insurance_raw <= 8);
    kani::assume(surplus_raw <= 8);
    kani::assume((1..=10_000).contains(&price_raw));
    let nonempty_claim = claim_units_raw != 0;
    let c_tot = c_tot_raw as u128;
    let insurance = insurance_raw as u128;
    let surplus = surplus_raw as u128;

    let (market_id, _, _) = ids();
    let cfg = V16Config::public_user_fund_with_market_slots(1, 1, 0, 10);
    let mut header = MarketGroupV16HeaderAccount::new_dynamic(market_id, cfg, 1, 0).unwrap();
    header.vault = V16PodU128::new(c_tot + insurance + surplus);
    header.c_tot = V16PodU128::new(c_tot);
    header.insurance = V16PodU128::new(insurance);
    let old_market_id = old_market_id_raw as u64;
    let new_market_id = old_market_id + 1;
    header.next_market_id = V16PodU64::new(new_market_id);
    header.asset_activation_count = V16PodU64::new(1);
    header.last_asset_activation_slot = V16PodU64::new(0);
    let vault_before = header.vault;
    let c_tot_before = header.c_tot;
    let insurance_before = header.insurance;

    let mut retired_asset = AssetStateV16 {
        lifecycle: AssetLifecycleV16::Retired,
        market_id: old_market_id,
        retired_slot: 1,
        ..AssetStateV16::default()
    };
    retired_asset.a_long = ADL_ONE;
    retired_asset.a_short = ADL_ONE;
    let old_backing = BackingBucketV16::empty_for_market(old_market_id);
    let mut source = SourceCreditStateV16 {
        credit_epoch: credit_epoch_raw as u64,
        credit_rate_num: if zero_credit_rate {
            0
        } else {
            CREDIT_RATE_SCALE
        },
        ..SourceCreditStateV16::EMPTY
    };
    source.positive_claim_bound_num = (claim_units_raw as u128) * BOUND_SCALE;
    let mut slot = EngineAssetSlotV16Account {
        asset: AssetStateV16Account::from_runtime(&retired_asset),
        source_credit_long: SourceCreditStateV16Account::from_runtime(&source),
        source_credit_short: SourceCreditStateV16Account::from_runtime(&source),
        backing_long: BackingBucketV16Account::from_runtime(&old_backing),
        backing_short: BackingBucketV16Account::from_runtime(&old_backing),
        ..EngineAssetSlotV16Account::default()
    };
    let slot_before = slot;

    let result = header.activate_empty_market_slot_not_atomic(0, &mut slot, price_raw as u64, 2);

    kani::cover!(
        !nonempty_claim
            && zero_credit_rate
            && credit_epoch_raw > 1
            && price_raw > 100
            && c_tot > 0
            && insurance > 0
            && surplus > 0,
        "retired-slot activation accepts empty source credit with old epoch, zero rate, and symbolic value state"
    );
    kani::cover!(
        nonempty_claim && claim_units_raw > 4 && credit_epoch_raw > 1,
        "retired-slot activation rejects symbolic nonempty source credit before reuse"
    );
    assert_eq!(header.vault, vault_before);
    assert_eq!(header.c_tot, c_tot_before);
    assert_eq!(header.insurance, insurance_before);
    assert_eq!(result.is_ok(), !nonempty_claim);
    if nonempty_claim {
        assert_eq!(slot, slot_before);
        assert_eq!(header.next_market_id.get(), new_market_id);
    } else {
        let asset = slot.asset.try_to_runtime().unwrap();
        assert_eq!(asset.lifecycle, AssetLifecycleV16::Active);
        assert_eq!(asset.market_id, new_market_id);
        assert_eq!(asset.raw_oracle_target_price, price_raw as u64);
        assert_eq!(asset.effective_price, price_raw as u64);
        assert_eq!(
            slot.source_credit_long.try_to_runtime().unwrap(),
            SourceCreditStateV16::EMPTY
        );
        assert_eq!(
            slot.source_credit_short.try_to_runtime().unwrap(),
            SourceCreditStateV16::EMPTY
        );
        assert_eq!(
            slot.backing_long.try_to_runtime().unwrap().market_id,
            new_market_id
        );
        assert_eq!(
            slot.backing_short.try_to_runtime().unwrap().market_id,
            new_market_id
        );
        assert_eq!(header.next_market_id.get(), new_market_id + 1);
        assert_eq!(header.asset_set_epoch.get(), 1);
        assert_eq!(header.risk_epoch.get(), 1);
    }
}

#[kani::proof]
#[kani::unwind(16)]
#[kani::solver(cadical)]
fn proof_v16_restart_empty_asset_core_preserves_budgets_and_assigns_fresh_market() {
    let budget_long_raw: u8 = kani::any();
    let budget_short_raw: u8 = kani::any();
    let next_market_id_before_raw: u8 = kani::any();
    let activation_count_before: u64 = kani::any();
    let asset_set_epoch_before: u64 = kani::any();
    let risk_epoch_before: u64 = kani::any();
    let price_raw: u16 = kani::any();
    let now_slot_raw: u8 = kani::any();
    kani::assume((1..=10_000).contains(&price_raw));
    kani::assume(next_market_id_before_raw > 0);
    kani::assume(now_slot_raw > 0);
    let budget_long = budget_long_raw as u128;
    let budget_short = budget_short_raw as u128;
    let next_market_id_before = next_market_id_before_raw as u64;
    let old_slot = empty_recovery_slot_for_market(1, 100, 10, budget_long, budget_short);
    let restarted =
        MarketGroupV16ViewMut::<u64>::kani_restarted_asset_slot_preserving_insurance_budget(
            &old_slot,
            next_market_id_before,
            price_raw as u64,
            now_slot_raw as u64,
        );
    let counters = MarketGroupV16ViewMut::<u64>::kani_asset_restart_next_counters(
        next_market_id_before,
        activation_count_before,
        asset_set_epoch_before,
        risk_epoch_before,
    );
    let expected_counter_ok = activation_count_before != u64::MAX
        && asset_set_epoch_before != u64::MAX
        && risk_epoch_before != u64::MAX
        && next_market_id_before != u64::MAX;
    let asset = restarted.asset.try_to_runtime().unwrap();

    kani::cover!(
        budget_long > 0 && budget_short > 0 && expected_counter_ok,
        "restart core covers nonzero preserved domain budgets"
    );
    kani::cover!(
        !expected_counter_ok,
        "restart core covers counter overflow rejection"
    );
    assert_eq!(asset.lifecycle, AssetLifecycleV16::Active);
    assert_eq!(asset.market_id, next_market_id_before);
    assert_eq!(asset.raw_oracle_target_price, price_raw as u64);
    assert_eq!(asset.effective_price, price_raw as u64);
    assert_eq!(asset.fund_px_last, price_raw as u64);
    assert_eq!(asset.slot_last, now_slot_raw as u64);
    assert_eq!(
        restarted.insurance_domain_budget_long.get(),
        old_slot.insurance_domain_budget_long.get()
    );
    assert_eq!(
        restarted.insurance_domain_budget_short.get(),
        old_slot.insurance_domain_budget_short.get()
    );
    assert_eq!(restarted.insurance_domain_spent_long.get(), 0);
    assert_eq!(restarted.insurance_domain_spent_short.get(), 0);
    let source = restarted.source_credit_long.try_to_runtime().unwrap();
    assert_eq!(source.positive_claim_bound_num, 0);
    assert_eq!(source.exact_positive_claim_num, 0);
    assert_eq!(source.fresh_reserved_backing_num, 0);
    assert_eq!(source.spent_backing_num, 0);
    assert_eq!(source.provider_receivable_num, 0);
    assert_eq!(source.valid_liened_backing_num, 0);
    assert_eq!(source.impaired_liened_backing_num, 0);
    assert_eq!(source.insurance_credit_reserved_num, 0);
    assert_eq!(source.valid_liened_insurance_num, 0);
    assert_eq!(source.impaired_liened_insurance_num, 0);
    assert_eq!(source.credit_rate_num, CREDIT_RATE_SCALE);
    assert_eq!(
        restarted.backing_long.try_to_runtime().unwrap().market_id,
        next_market_id_before
    );
    assert_eq!(counters.is_ok(), expected_counter_ok);
    if let Ok((next_market_id, activation_count, asset_set_epoch, risk_epoch)) = counters {
        assert_eq!(next_market_id, next_market_id_before + 1);
        assert_eq!(activation_count, activation_count_before + 1);
        assert_eq!(asset_set_epoch, asset_set_epoch_before + 1);
        assert_eq!(risk_epoch, risk_epoch_before + 1);
    }
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_public_restart_empty_asset_zero_preserves_budgets_and_senior_value() {
    let restart_retired: bool = kani::any();
    let long_budget_raw: u8 = kani::any();
    let short_budget_raw: u8 = kani::any();
    let slack_raw: u8 = kani::any();
    let price_raw: u16 = kani::any();
    kani::assume((1..=10_000).contains(&price_raw));

    let old_market_id = 1u64;
    let next_market_id_before = 2u64;
    let current_slot = 5u64;
    let now_slot = 6u64;
    let old_activation_count = 1u64;
    let old_asset_epoch = 3u64;
    let old_risk_epoch = 4u64;
    let long_budget = long_budget_raw as u128;
    let short_budget = short_budget_raw as u128;
    let budget_total = long_budget + short_budget;
    let insurance = budget_total + slack_raw as u128;

    let (market_group_id, _, _) = ids();
    let cfg = V16Config::public_user_fund_with_market_slots(1, 1, 0, 10);
    let mut header = MarketGroupV16HeaderAccount::new_dynamic(market_group_id, cfg, 1, 0).unwrap();
    header.current_slot = V16PodU64::new(current_slot);
    header.slot_last = V16PodU64::new(current_slot);
    header.next_market_id = V16PodU64::new(next_market_id_before);
    header.asset_activation_count = V16PodU64::new(old_activation_count);
    header.last_asset_activation_slot = V16PodU64::new(current_slot);
    header.asset_set_epoch = V16PodU64::new(old_asset_epoch);
    header.risk_epoch = V16PodU64::new(old_risk_epoch);
    header.vault = V16PodU128::new(insurance);
    header.insurance = V16PodU128::new(insurance);
    header.insurance_domain_budget_remaining_total = V16PodU128::new(budget_total);

    let mut markets = [Market::new(
        0u64,
        EngineAssetSlotV16Account::empty_for_market(old_market_id),
    )];
    let mut old_asset = AssetStateV16::default();
    old_asset.market_id = old_market_id;
    old_asset.lifecycle = if restart_retired {
        AssetLifecycleV16::Retired
    } else {
        AssetLifecycleV16::Recovery
    };
    old_asset.raw_oracle_target_price = 100;
    old_asset.effective_price = 100;
    old_asset.fund_px_last = 100;
    old_asset.slot_last = current_slot;
    old_asset.retired_slot = if restart_retired { current_slot } else { 0 };
    markets[0].engine.asset = AssetStateV16Account::from_runtime(&old_asset);
    markets[0].engine.insurance_domain_budget_long = V16PodU128::new(long_budget);
    markets[0].engine.insurance_domain_budget_short = V16PodU128::new(short_budget);

    let vault_before = header.vault.get();
    let insurance_before = header.insurance.get();
    let c_tot_before = header.c_tot.get();
    let budget_total_before = header.insurance_domain_budget_remaining_total.get();

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    market
        .restart_empty_asset_preserving_insurance_budget_not_atomic(0, price_raw as u64, now_slot)
        .unwrap();

    let restarted = market.markets[0].engine;
    let restarted_asset = restarted.asset.try_to_runtime().unwrap();

    kani::cover!(
        long_budget > 0 && short_budget > 0 && slack_raw > 0,
        "public asset-zero restart covers preserved nonzero budgets with senior slack"
    );
    kani::cover!(
        restart_retired,
        "public asset-zero restart covers retired source lifecycle"
    );
    kani::cover!(
        !restart_retired,
        "public asset-zero restart covers recovery source lifecycle"
    );

    assert_eq!(restarted_asset.lifecycle, AssetLifecycleV16::Active);
    assert_eq!(restarted_asset.market_id, next_market_id_before);
    assert_eq!(restarted_asset.raw_oracle_target_price, price_raw as u64);
    assert_eq!(restarted_asset.effective_price, price_raw as u64);
    assert_eq!(restarted_asset.fund_px_last, price_raw as u64);
    assert_eq!(restarted_asset.slot_last, now_slot);
    assert_eq!(restarted.insurance_domain_budget_long.get(), long_budget);
    assert_eq!(restarted.insurance_domain_budget_short.get(), short_budget);
    assert_eq!(restarted.insurance_domain_spent_long.get(), 0);
    assert_eq!(restarted.insurance_domain_spent_short.get(), 0);
    assert_eq!(
        restarted.source_credit_long.try_to_runtime().unwrap(),
        SourceCreditStateV16::EMPTY
    );
    assert_eq!(
        restarted.source_credit_short.try_to_runtime().unwrap(),
        SourceCreditStateV16::EMPTY
    );
    assert_eq!(
        restarted.backing_long.try_to_runtime().unwrap().market_id,
        next_market_id_before
    );
    assert_eq!(
        restarted.backing_short.try_to_runtime().unwrap().market_id,
        next_market_id_before
    );
    assert_eq!(market.header.vault.get(), vault_before);
    assert_eq!(market.header.insurance.get(), insurance_before);
    assert_eq!(market.header.c_tot.get(), c_tot_before);
    assert_eq!(
        market.header.insurance_domain_budget_remaining_total.get(),
        budget_total_before
    );
    assert_eq!(market.header.current_slot.get(), now_slot);
    assert_eq!(
        market.header.next_market_id.get(),
        next_market_id_before + 1
    );
    assert_eq!(
        market.header.asset_activation_count.get(),
        old_activation_count + 1
    );
    assert_eq!(market.header.asset_set_epoch.get(), old_asset_epoch + 1);
    assert_eq!(market.header.risk_epoch.get(), old_risk_epoch + 1);
    assert_eq!(market.header.last_asset_activation_slot.get(), now_slot);
}

fn assert_v16_public_restart_two_slot_selected_only<const SELECTED_INDEX: usize>() {
    let restart_retired: bool = kani::any();
    let selected_long_budget_raw: u8 = kani::any();
    let selected_short_budget_raw: u8 = kani::any();
    let other_long_budget_raw: u8 = kani::any();
    let other_short_budget_raw: u8 = kani::any();
    let slack_raw: u8 = kani::any();
    let price_raw: u16 = kani::any();
    assert!(SELECTED_INDEX < 2);
    kani::assume((1..=10_000).contains(&price_raw));
    let selected_index = SELECTED_INDEX;
    let other_index = 1usize - selected_index;

    let current_slot = 5u64;
    let now_slot = 6u64;
    let next_market_id_before = 3u64;
    let old_activation_count = 2u64;
    let old_asset_epoch = 4u64;
    let old_risk_epoch = 7u64;
    let selected_long_budget = selected_long_budget_raw as u128;
    let selected_short_budget = selected_short_budget_raw as u128;
    let other_long_budget = other_long_budget_raw as u128;
    let other_short_budget = other_short_budget_raw as u128;
    let budget_total =
        selected_long_budget + selected_short_budget + other_long_budget + other_short_budget;
    let senior_slack = slack_raw as u128;
    let insurance = budget_total + senior_slack;

    let (market_group_id, _, _) = ids();
    let cfg = V16Config::public_user_fund_with_market_slots(1, 2, 0, 10);
    let mut header = MarketGroupV16HeaderAccount::new_dynamic(market_group_id, cfg, 2, 0).unwrap();
    header.current_slot = V16PodU64::new(current_slot);
    header.slot_last = V16PodU64::new(current_slot);
    header.next_market_id = V16PodU64::new(next_market_id_before);
    header.asset_activation_count = V16PodU64::new(old_activation_count);
    header.last_asset_activation_slot = V16PodU64::new(current_slot);
    header.asset_set_epoch = V16PodU64::new(old_asset_epoch);
    header.risk_epoch = V16PodU64::new(old_risk_epoch);
    header.vault = V16PodU128::new(insurance);
    header.insurance = V16PodU128::new(insurance);
    header.insurance_domain_budget_remaining_total = V16PodU128::new(budget_total);

    let mut markets = [
        Market::new(10u64, EngineAssetSlotV16Account::empty_for_market(1)),
        Market::new(20u64, EngineAssetSlotV16Account::empty_for_market(2)),
    ];
    let mut selected_asset = AssetStateV16::default();
    selected_asset.market_id = (selected_index + 1) as u64;
    selected_asset.lifecycle = if restart_retired {
        AssetLifecycleV16::Retired
    } else {
        AssetLifecycleV16::Recovery
    };
    selected_asset.raw_oracle_target_price = 100;
    selected_asset.effective_price = 100;
    selected_asset.fund_px_last = 100;
    selected_asset.slot_last = current_slot;
    selected_asset.retired_slot = if restart_retired { current_slot } else { 0 };
    markets[selected_index].engine.asset = AssetStateV16Account::from_runtime(&selected_asset);
    markets[selected_index].engine.insurance_domain_budget_long =
        V16PodU128::new(selected_long_budget);
    markets[selected_index].engine.insurance_domain_budget_short =
        V16PodU128::new(selected_short_budget);

    let mut other_asset = AssetStateV16::default();
    other_asset.market_id = (other_index + 1) as u64;
    other_asset.lifecycle = AssetLifecycleV16::Active;
    other_asset.raw_oracle_target_price = 111;
    other_asset.effective_price = 111;
    other_asset.fund_px_last = 111;
    other_asset.slot_last = current_slot;
    markets[other_index].engine.asset = AssetStateV16Account::from_runtime(&other_asset);
    markets[other_index].engine.insurance_domain_budget_long = V16PodU128::new(other_long_budget);
    markets[other_index].engine.insurance_domain_budget_short = V16PodU128::new(other_short_budget);

    let vault_before = header.vault.get();
    let insurance_before = header.insurance.get();
    let c_tot_before = header.c_tot.get();
    let budget_total_before = header.insurance_domain_budget_remaining_total.get();
    let selected_wrapper_before = markets[selected_index].wrapper;
    let other_wrapper_before = markets[other_index].wrapper;
    let other_asset_before = markets[other_index].engine.asset.try_to_runtime().unwrap();
    let other_budget_long_before = markets[other_index]
        .engine
        .insurance_domain_budget_long
        .get();
    let other_budget_short_before = markets[other_index]
        .engine
        .insurance_domain_budget_short
        .get();
    let selected_budget_long_before = markets[selected_index]
        .engine
        .insurance_domain_budget_long
        .get();
    let selected_budget_short_before = markets[selected_index]
        .engine
        .insurance_domain_budget_short
        .get();

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let residual_before = market.kani_residual();
    market
        .restart_empty_asset_preserving_insurance_budget_not_atomic(
            selected_index,
            price_raw as u64,
            now_slot,
        )
        .unwrap();

    kani::cover!(
        selected_long_budget > 0 && selected_short_budget > 0,
        "two-slot restart covers selected asset with nonzero preserved budgets"
    );
    kani::cover!(
        other_long_budget > 0 && other_short_budget > 0 && senior_slack > 0,
        "two-slot restart covers unrelated funded asset and senior slack"
    );

    let restarted = market.markets[selected_index].engine;
    let restarted_asset = restarted.asset.try_to_runtime().unwrap();
    assert_eq!(restarted_asset.lifecycle, AssetLifecycleV16::Active);
    assert_eq!(restarted_asset.market_id, next_market_id_before);
    assert_eq!(restarted_asset.raw_oracle_target_price, price_raw as u64);
    assert_eq!(restarted_asset.effective_price, price_raw as u64);
    assert_eq!(restarted_asset.fund_px_last, price_raw as u64);
    assert_eq!(restarted_asset.slot_last, now_slot);
    assert_eq!(
        restarted.insurance_domain_budget_long.get(),
        selected_budget_long_before
    );
    assert_eq!(
        restarted.insurance_domain_budget_short.get(),
        selected_budget_short_before
    );
    assert_eq!(restarted.insurance_domain_spent_long.get(), 0);
    assert_eq!(restarted.insurance_domain_spent_short.get(), 0);
    assert_eq!(
        restarted.source_credit_long.try_to_runtime().unwrap(),
        SourceCreditStateV16::EMPTY
    );
    assert_eq!(
        restarted.source_credit_short.try_to_runtime().unwrap(),
        SourceCreditStateV16::EMPTY
    );
    assert_eq!(
        restarted.backing_long.try_to_runtime().unwrap().market_id,
        next_market_id_before
    );
    assert_eq!(
        restarted.backing_short.try_to_runtime().unwrap().market_id,
        next_market_id_before
    );

    assert_eq!(
        market.markets[selected_index].wrapper,
        selected_wrapper_before
    );
    assert_eq!(market.markets[other_index].wrapper, other_wrapper_before);
    let other_asset_after = market.markets[other_index]
        .engine
        .asset
        .try_to_runtime()
        .unwrap();
    assert_eq!(other_asset_after.market_id, other_asset_before.market_id);
    assert_eq!(other_asset_after.lifecycle, other_asset_before.lifecycle);
    assert_eq!(
        other_asset_after.raw_oracle_target_price,
        other_asset_before.raw_oracle_target_price
    );
    assert_eq!(
        other_asset_after.effective_price,
        other_asset_before.effective_price
    );
    assert_eq!(
        other_asset_after.fund_px_last,
        other_asset_before.fund_px_last
    );
    assert_eq!(other_asset_after.slot_last, other_asset_before.slot_last);
    assert_eq!(
        market.markets[other_index]
            .engine
            .insurance_domain_budget_long
            .get(),
        other_budget_long_before
    );
    assert_eq!(
        market.markets[other_index]
            .engine
            .insurance_domain_budget_short
            .get(),
        other_budget_short_before
    );
    assert_eq!(market.header.vault.get(), vault_before);
    assert_eq!(market.header.insurance.get(), insurance_before);
    assert_eq!(market.header.c_tot.get(), c_tot_before);
    assert_eq!(
        market.header.insurance_domain_budget_remaining_total.get(),
        budget_total_before
    );
    assert_eq!(
        market.header.next_market_id.get(),
        next_market_id_before + 1
    );
    assert_eq!(
        market.header.asset_activation_count.get(),
        old_activation_count + 1
    );
    assert_eq!(market.header.asset_set_epoch.get(), old_asset_epoch + 1);
    assert_eq!(market.header.risk_epoch.get(), old_risk_epoch + 1);
    assert_eq!(market.header.current_slot.get(), now_slot);
    assert_eq!(market.header.last_asset_activation_slot.get(), now_slot);
    // Junior-pool isolation: this transition must not move the junior
    // residual pool.
    assert_eq!(market.kani_residual(), residual_before);
}

#[kani::proof]
#[kani::unwind(64)]
#[kani::solver(cadical)]
fn proof_v16_public_restart_asset_zero_preserves_only_selected_slot_in_two_slot_view() {
    assert_v16_public_restart_two_slot_selected_only::<0>();
}

#[kani::proof]
#[kani::unwind(64)]
#[kani::solver(cadical)]
fn proof_v16_public_restart_nonzero_asset_preserves_only_selected_slot_in_two_slot_view() {
    assert_v16_public_restart_two_slot_selected_only::<1>();
}

#[kani::proof]
#[kani::unwind(32)]
#[kani::solver(cadical)]
fn proof_v16_public_restart_rejects_spent_domain_before_mutation() {
    let restart_retired: bool = kani::any();
    let spent_raw: u8 = kani::any();
    let remaining_raw: u8 = kani::any();
    let slack_raw: u8 = kani::any();
    let price_raw: u16 = kani::any();
    kani::assume(spent_raw > 0);
    kani::assume((1..=10_000).contains(&price_raw));

    let old_market_id = 1u64;
    let next_market_id_before = 2u64;
    let current_slot = 5u64;
    let now_slot = 6u64;
    let old_activation_count = 1u64;
    let old_asset_epoch = 3u64;
    let old_risk_epoch = 4u64;
    let spent = spent_raw as u128;
    let remaining = remaining_raw as u128;
    let budget = spent + remaining;
    let insurance = remaining + slack_raw as u128;

    let (market_group_id, _, _) = ids();
    let cfg = V16Config::public_user_fund_with_market_slots(1, 1, 0, 10);
    let mut header = MarketGroupV16HeaderAccount::new_dynamic(market_group_id, cfg, 1, 0).unwrap();
    header.current_slot = V16PodU64::new(current_slot);
    header.slot_last = V16PodU64::new(current_slot);
    header.next_market_id = V16PodU64::new(next_market_id_before);
    header.asset_activation_count = V16PodU64::new(old_activation_count);
    header.last_asset_activation_slot = V16PodU64::new(current_slot);
    header.asset_set_epoch = V16PodU64::new(old_asset_epoch);
    header.risk_epoch = V16PodU64::new(old_risk_epoch);
    header.vault = V16PodU128::new(insurance);
    header.insurance = V16PodU128::new(insurance);
    header.insurance_domain_budget_remaining_total = V16PodU128::new(remaining);

    let mut markets = [Market::new(
        0u64,
        EngineAssetSlotV16Account::empty_for_market(old_market_id),
    )];
    let mut old_asset = AssetStateV16::default();
    old_asset.market_id = old_market_id;
    old_asset.lifecycle = if restart_retired {
        AssetLifecycleV16::Retired
    } else {
        AssetLifecycleV16::Recovery
    };
    old_asset.raw_oracle_target_price = 100;
    old_asset.effective_price = 100;
    old_asset.fund_px_last = 100;
    old_asset.slot_last = current_slot;
    old_asset.retired_slot = if restart_retired { current_slot } else { 0 };
    markets[0].engine.asset = AssetStateV16Account::from_runtime(&old_asset);
    markets[0].engine.insurance_domain_budget_long = V16PodU128::new(budget);
    markets[0].engine.insurance_domain_spent_long = V16PodU128::new(spent);

    let vault_before = header.vault.get();
    let insurance_before = header.insurance.get();
    let c_tot_before = header.c_tot.get();
    let next_market_id_before_header = header.next_market_id.get();
    let current_slot_before = header.current_slot.get();
    let activation_count_before = header.asset_activation_count.get();
    let asset_epoch_before = header.asset_set_epoch.get();
    let risk_epoch_before = header.risk_epoch.get();
    let budget_total_before = header.insurance_domain_budget_remaining_total.get();
    let asset_before = markets[0].engine.asset.try_to_runtime().unwrap();
    let budget_long_before = markets[0].engine.insurance_domain_budget_long.get();
    let spent_long_before = markets[0].engine.insurance_domain_spent_long.get();
    let budget_short_before = markets[0].engine.insurance_domain_budget_short.get();
    let spent_short_before = markets[0].engine.insurance_domain_spent_short.get();
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let result = market.restart_empty_asset_preserving_insurance_budget_not_atomic(
        0,
        price_raw as u64,
        now_slot,
    );

    kani::cover!(
        remaining > 0 && slack_raw > 0,
        "restart rejection covers spent domain with remaining budget and senior slack"
    );
    kani::cover!(
        restart_retired,
        "restart rejection covers retired source lifecycle"
    );
    kani::cover!(
        !restart_retired,
        "restart rejection covers recovery source lifecycle"
    );

    assert!(matches!(result, Err(V16Error::LockActive)));
    assert_eq!(market.header.vault.get(), vault_before);
    assert_eq!(market.header.insurance.get(), insurance_before);
    assert_eq!(market.header.c_tot.get(), c_tot_before);
    assert_eq!(
        market.header.next_market_id.get(),
        next_market_id_before_header
    );
    assert_eq!(market.header.current_slot.get(), current_slot_before);
    assert_eq!(
        market.header.asset_activation_count.get(),
        activation_count_before
    );
    assert_eq!(market.header.asset_set_epoch.get(), asset_epoch_before);
    assert_eq!(market.header.risk_epoch.get(), risk_epoch_before);
    assert_eq!(
        market.header.insurance_domain_budget_remaining_total.get(),
        budget_total_before
    );
    let asset_after = market.markets[0].engine.asset.try_to_runtime().unwrap();
    assert_eq!(asset_after.market_id, asset_before.market_id);
    assert_eq!(asset_after.lifecycle, asset_before.lifecycle);
    assert_eq!(
        asset_after.raw_oracle_target_price,
        asset_before.raw_oracle_target_price
    );
    assert_eq!(asset_after.effective_price, asset_before.effective_price);
    assert_eq!(asset_after.fund_px_last, asset_before.fund_px_last);
    assert_eq!(asset_after.slot_last, asset_before.slot_last);
    assert_eq!(asset_after.retired_slot, asset_before.retired_slot);
    assert_eq!(
        market.markets[0].engine.insurance_domain_budget_long.get(),
        budget_long_before
    );
    assert_eq!(
        market.markets[0].engine.insurance_domain_spent_long.get(),
        spent_long_before
    );
    assert_eq!(
        market.markets[0].engine.insurance_domain_budget_short.get(),
        budget_short_before
    );
    assert_eq!(
        market.markets[0].engine.insurance_domain_spent_short.get(),
        spent_short_before
    );
    assert_eq!(
        market.markets[0]
            .engine
            .source_credit_long
            .positive_claim_bound_num
            .get(),
        0
    );
    assert_eq!(
        market.markets[0]
            .engine
            .source_credit_short
            .positive_claim_bound_num
            .get(),
        0
    );
    assert_eq!(
        market.markets[0].engine.backing_long.market_id.get(),
        old_market_id
    );
    assert_eq!(
        market.markets[0].engine.backing_short.market_id.get(),
        old_market_id
    );
}

#[kani::proof]
#[kani::unwind(16)]
#[kani::solver(cadical)]
fn proof_v16_canonical_retired_asset_slot_preserves_identity_and_clears_local_ledgers() {
    let market_id_raw: u8 = kani::any();
    let price_raw: u16 = kani::any();
    let retired_slot_raw: u8 = kani::any();
    let slot_last_raw: u8 = kani::any();
    kani::assume((1..=10_000).contains(&price_raw));
    kani::assume(market_id_raw > 0);
    kani::assume(retired_slot_raw > 0);
    let mut old_asset = AssetStateV16::default();
    old_asset.market_id = market_id_raw as u64;
    old_asset.lifecycle = AssetLifecycleV16::Retired;
    old_asset.retired_slot = retired_slot_raw as u64;
    old_asset.raw_oracle_target_price = price_raw as u64;
    old_asset.effective_price = price_raw as u64;
    old_asset.fund_px_last = price_raw as u64;
    old_asset.slot_last = slot_last_raw as u64;

    let canonical = MarketGroupV16ViewMut::<u64>::kani_canonical_retired_asset_slot(old_asset);
    let asset = canonical.asset.try_to_runtime().unwrap();

    kani::cover!(
        market_id_raw > 1 && retired_slot_raw > 1 && price_raw > 100,
        "canonical retired slot covers nontrivial retired identity"
    );
    assert_eq!(asset.lifecycle, AssetLifecycleV16::Retired);
    assert_eq!(asset.market_id, old_asset.market_id);
    assert_eq!(asset.retired_slot, old_asset.retired_slot);
    assert_eq!(
        asset.raw_oracle_target_price,
        old_asset.raw_oracle_target_price
    );
    assert_eq!(asset.effective_price, old_asset.effective_price);
    assert_eq!(canonical.insurance_domain_budget_long.get(), 0);
    assert_eq!(canonical.insurance_domain_budget_short.get(), 0);
    assert_eq!(canonical.insurance_domain_spent_long.get(), 0);
    assert_eq!(canonical.insurance_domain_spent_short.get(), 0);
    assert_eq!(
        canonical.source_credit_long.try_to_runtime().unwrap(),
        SourceCreditStateV16::EMPTY
    );
    assert_eq!(
        canonical.backing_long.try_to_runtime().unwrap().market_id,
        old_asset.market_id
    );
    assert_eq!(
        canonical
            .insurance_reservation_long
            .try_to_runtime()
            .unwrap(),
        InsuranceCreditReservationV16::EMPTY
    );
}

#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn proof_v16_dynamic_market_slot_slice_len_matches_runtime_capacity() {
    let supplied_raw: u8 = kani::any();
    let capacity_raw: u8 = kani::any();
    let configured_raw: u8 = kani::any();
    let supplied = supplied_raw as usize;
    let capacity = capacity_raw as usize;
    let configured = configured_raw as usize;
    let result = MarketGroupV16HeaderAccount::kani_validate_dynamic_market_slots_len(
        supplied, capacity, configured,
    );
    let expected_ok = supplied == capacity && capacity >= configured;

    kani::cover!(
        expected_ok && capacity > configured,
        "dynamic market slot length proof covers realloc capacity above configured markets"
    );
    kani::cover!(
        supplied < capacity,
        "dynamic market slot length proof covers undersupplied wrapper slice"
    );
    kani::cover!(
        supplied > capacity,
        "dynamic market slot length proof covers oversupplied wrapper slice"
    );
    kani::cover!(
        supplied == capacity && capacity < configured,
        "dynamic market slot length proof covers capacity below configured markets"
    );
    assert_eq!(result.is_ok(), expected_ok);
    if expected_ok {
        assert_eq!(result, Ok(()));
    } else {
        assert!(supplied != capacity || capacity < configured);
        assert_eq!(result, Err(V16Error::InvalidConfig));
    }
}

#[kani::proof]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn proof_v16_dynamic_market_account_len_roundtrips_capacity_and_offsets() {
    let capacity_raw: u16 = kani::any();
    let index_raw: u16 = kani::any();
    let capacity = capacity_raw as usize;
    let index = index_raw as usize;
    let header_len = core::mem::size_of::<MarketGroupV16HeaderAccount>();
    let stride = MarketGroupV16HeaderAccount::kani_dynamic_asset_slot_stride::<u8>();
    kani::assume(stride > 1);

    let len_result = MarketGroupV16HeaderAccount::dynamic_market_group_account_len::<u8>(capacity);
    let expected_len = capacity
        .checked_mul(stride)
        .and_then(|trailing| header_len.checked_add(trailing));

    kani::cover!(
        expected_len.is_some() && capacity > 1,
        "dynamic account length proof covers nontrivial realloc capacity"
    );
    kani::cover!(
        capacity == 0,
        "dynamic account length proof covers zero capacity"
    );
    assert_eq!(len_result.is_ok(), expected_len.is_some());

    let len = expected_len.unwrap();
    assert_eq!(len_result, Ok(len));
    assert_eq!(
        MarketGroupV16HeaderAccount::dynamic_asset_slot_capacity_from_account_len::<u8>(len),
        Ok(capacity)
    );
    assert_eq!(
        MarketGroupV16HeaderAccount::validate_dynamic_market_group_account_len::<u8>(len, capacity),
        Ok(())
    );

    let unaligned_len = len.checked_add(1).unwrap();
    assert_eq!(
        MarketGroupV16HeaderAccount::dynamic_asset_slot_capacity_from_account_len::<u8>(
            unaligned_len
        ),
        Err(V16Error::InvalidConfig)
    );
    assert_eq!(
        MarketGroupV16HeaderAccount::validate_dynamic_market_group_account_len::<u8>(
            unaligned_len,
            capacity
        ),
        Err(V16Error::InvalidConfig)
    );

    if capacity > 0 && index < capacity {
        let offset = MarketGroupV16HeaderAccount::dynamic_asset_slot_offset::<u8>(index).unwrap();
        assert_eq!(offset, header_len + index * stride);
        assert!(offset >= header_len);
        assert!(offset.checked_add(stride).unwrap() <= len);
        kani::cover!(
            index + 1 == capacity,
            "dynamic offset proof covers final occupied slot boundary"
        );
    }

    assert_eq!(
        MarketGroupV16HeaderAccount::dynamic_asset_slot_capacity_from_account_len::<u8>(
            header_len - 1
        ),
        Err(V16Error::InvalidConfig)
    );
}

#[kani::proof]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn proof_v16_dynamic_market_account_len_fails_closed_on_arithmetic_overflow() {
    let capacity: usize = kani::any();
    let index: usize = kani::any();
    let header_len = core::mem::size_of::<MarketGroupV16HeaderAccount>();
    let stride = MarketGroupV16HeaderAccount::kani_dynamic_asset_slot_stride::<u8>();
    kani::assume(stride > 1);
    let max_capacity_without_len_overflow = (usize::MAX - header_len) / stride;
    kani::assume(capacity > max_capacity_without_len_overflow);
    kani::assume(index > max_capacity_without_len_overflow);

    kani::cover!(
        capacity > max_capacity_without_len_overflow + 1,
        "dynamic account length overflow proof covers deep overflow region"
    );
    assert_eq!(
        MarketGroupV16HeaderAccount::dynamic_market_group_account_len::<u8>(capacity),
        Err(V16Error::ArithmeticOverflow)
    );
    assert_eq!(
        MarketGroupV16HeaderAccount::dynamic_asset_slot_offset::<u8>(index),
        Err(V16Error::ArithmeticOverflow)
    );
}

#[kani::proof]
#[kani::unwind(16)]
#[kani::solver(cadical)]
fn proof_v16_dynamic_market_extension_slots_must_be_zero_fill() {
    let extension_index_raw: u8 = kani::any();
    let invalid_index_raw: u8 = kani::any();
    kani::assume(extension_index_raw > 0);
    let extension_index = extension_index_raw as usize;
    let invalid_index = 256usize + invalid_index_raw as usize;
    let (market_id, _, _) = ids();
    let cfg = V16Config::public_user_fund_with_market_slots(1, 1, 0, 10);
    let header = MarketGroupV16HeaderAccount::new_dynamic(market_id, cfg, 256, 0).unwrap();
    let zero_fill = EngineAssetSlotV16Account::default();
    let mut canonical_disabled_extension = EngineAssetSlotV16Account::default();
    canonical_disabled_extension.insurance_domain_budget_long = V16PodU128::new(MAX_VAULT_TVL);
    canonical_disabled_extension.insurance_domain_budget_short = V16PodU128::new(MAX_VAULT_TVL);
    let mut dirty_extension = EngineAssetSlotV16Account::default();
    dirty_extension.insurance_domain_spent_long = V16PodU128::new(1);

    let zero_extension =
        header.kani_validate_dynamic_market_slot_shape_at(extension_index, &zero_fill);
    let canonical_disabled_result = header
        .kani_validate_dynamic_market_slot_shape_at(extension_index, &canonical_disabled_extension);
    let dirty_extension_result =
        header.kani_validate_dynamic_market_slot_shape_at(extension_index, &dirty_extension);
    let configured_dirty_result =
        header.kani_validate_dynamic_market_slot_shape_at(0, &dirty_extension);
    let out_of_capacity_result =
        header.kani_validate_dynamic_market_slot_shape_at(invalid_index, &zero_fill);

    kani::cover!(
        extension_index > 1,
        "dynamic extension slot proof covers later realloc slot"
    );
    kani::cover!(
        invalid_index > 256,
        "dynamic extension slot proof covers out-of-capacity slot index"
    );
    assert_eq!(zero_extension, Ok(()));
    assert_eq!(canonical_disabled_result, Ok(()));
    assert_eq!(dirty_extension_result, Err(V16Error::InvalidConfig));
    assert_eq!(configured_dirty_result, Ok(()));
    assert_eq!(out_of_capacity_result, Err(V16Error::InvalidConfig));
}

#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn proof_v16_insurance_domain_mapping_is_in_bounds_unique_and_roundtrips() {
    let asset_raw: u8 = kani::any();
    let side_is_short: bool = kani::any();
    kani::assume(asset_raw < 4);
    let asset_index = asset_raw as usize;
    let side = if side_is_short {
        SideV16::Short
    } else {
        SideV16::Long
    };
    let (market_id, _, _) = ids();
    let cfg = V16Config::public_user_fund_with_market_slots(1, 4, 0, 10);
    let mut header = MarketGroupV16HeaderAccount::new_dynamic(market_id, cfg, 4, 0).unwrap();
    let mut markets = [
        Market::new(0u64, EngineAssetSlotV16Account::default()),
        Market::new(1u64, EngineAssetSlotV16Account::default()),
        Market::new(2u64, EngineAssetSlotV16Account::default()),
        Market::new(3u64, EngineAssetSlotV16Account::default()),
    ];
    let market = MarketGroupV16ViewMut::new(&mut header, &mut markets);

    let domain = market
        .kani_insurance_domain_index(asset_index, side)
        .unwrap();
    let long_domain = market
        .kani_insurance_domain_index(asset_index, SideV16::Long)
        .unwrap();
    let short_domain = market
        .kani_insurance_domain_index(asset_index, SideV16::Short)
        .unwrap();
    let roundtrip = market.kani_domain_asset_side(domain).unwrap();
    let invalid_asset = market.kani_insurance_domain_index(4, side);
    let invalid_domain = market.kani_domain_asset_side(8);

    kani::cover!(
        asset_index == 0 && !side_is_short,
        "domain mapping covers asset-zero long domain"
    );
    kani::cover!(
        asset_index > 1 && side_is_short,
        "domain mapping covers nonzero short domain"
    );
    kani::cover!(
        invalid_asset == Err(V16Error::InvalidLeg) && invalid_domain == Err(V16Error::InvalidLeg),
        "domain mapping covers invalid asset and invalid domain rejection"
    );
    assert!(domain < 8);
    assert_eq!(domain, asset_index * 2 + usize::from(side_is_short));
    assert_eq!(roundtrip, (asset_index, side));
    assert_eq!(long_domain, asset_index * 2);
    assert_eq!(short_domain, asset_index * 2 + 1);
    assert_ne!(long_domain, short_domain);
    assert_eq!(invalid_asset, Err(V16Error::InvalidLeg));
    assert_eq!(invalid_domain, Err(V16Error::InvalidLeg));
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_view_overwithdraw_rejects() {
    let start_capital_raw: u8 = kani::any();
    let other_capital_raw: u8 = kani::any();
    let insurance_raw: u8 = kani::any();
    let surplus_raw: u8 = kani::any();
    let extra_raw: u8 = kani::any();
    kani::assume(start_capital_raw <= 16);
    kani::assume(other_capital_raw <= 16);
    kani::assume(insurance_raw <= 16);
    kani::assume(surplus_raw <= 16);
    kani::assume((1..=16).contains(&extra_raw));
    let start_capital = start_capital_raw as u128;
    let other_capital = other_capital_raw as u128;
    let insurance = insurance_raw as u128;
    let surplus = surplus_raw as u128;
    let withdraw = start_capital + extra_raw as u128;
    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    let c_tot_before = start_capital + other_capital;
    let vault_before = c_tot_before + insurance + surplus;
    header.c_tot = V16PodU128::new(c_tot_before);
    header.insurance = V16PodU128::new(insurance);
    header.vault = V16PodU128::new(vault_before);
    account_header.capital = V16PodU128::new(start_capital);
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);

    let result = market.withdraw_not_atomic(&mut account, withdraw);

    kani::cover!(
        start_capital > 0 && other_capital > 0 && insurance > 0 && surplus > 0 && extra_raw > 1,
        "overwithdraw rejection covers nonempty senior state without value mutation"
    );
    kani::cover!(
        start_capital == 0 && withdraw > 0,
        "overwithdraw rejection covers zero-capital account"
    );
    assert_eq!(result, Err(V16Error::LockActive));
    assert_eq!(account.header.capital.get(), start_capital);
    assert_eq!(market.header.c_tot.get(), c_tot_before);
    assert_eq!(market.header.vault.get(), vault_before);
    assert_eq!(market.header.insurance.get(), insurance);
    assert_eq!(
        market.header.vault.get() - market.header.c_tot.get() - market.header.insurance.get(),
        surplus
    );
    assert_eq!(market.validate_shape(), Ok(()));
    assert_eq!(account.validate_with_market(&market.as_view()), Ok(()));
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_view_withdraw_reduces_vault_ctot_and_capital_equally() {
    let start_capital_raw: u8 = kani::any();
    let other_capital_raw: u8 = kani::any();
    let insurance_raw: u8 = kani::any();
    let surplus_raw: u8 = kani::any();
    let amount_raw: u8 = kani::any();
    kani::assume(start_capital_raw <= 16);
    kani::assume(other_capital_raw <= 16);
    kani::assume(insurance_raw <= 16);
    kani::assume(surplus_raw <= 16);
    kani::assume(amount_raw <= start_capital_raw);
    let start_capital = start_capital_raw as u128;
    let other_capital = other_capital_raw as u128;
    let insurance = insurance_raw as u128;
    let surplus = surplus_raw as u128;
    let amount = amount_raw as u128;
    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    let c_tot_before = start_capital + other_capital;
    let vault_before = c_tot_before + insurance + surplus;
    header.c_tot = V16PodU128::new(c_tot_before);
    header.insurance = V16PodU128::new(insurance);
    header.vault = V16PodU128::new(vault_before);
    account_header.capital = V16PodU128::new(start_capital);
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    let residual_before = market.kani_residual();

    market.withdraw_not_atomic(&mut account, amount).unwrap();

    kani::cover!(
        amount > 0 && amount < start_capital && other_capital > 0 && insurance > 0 && surplus > 0,
        "successful withdraw covers partial exit from nonempty senior state"
    );
    kani::cover!(
        amount > 0 && amount == start_capital && other_capital > 0,
        "successful withdraw covers full account-capital exit with unrelated capital"
    );
    kani::cover!(
        amount == 0 && start_capital > 0 && other_capital > 0,
        "successful withdraw covers zero-amount no-op over nonempty senior state"
    );
    assert_eq!(market.header.vault.get(), vault_before - amount);
    assert_eq!(market.header.c_tot.get(), c_tot_before - amount);
    assert_eq!(account.header.capital.get(), start_capital - amount);
    assert_eq!(market.header.insurance.get(), insurance);
    assert_eq!(
        market.header.vault.get() - market.header.c_tot.get() - market.header.insurance.get(),
        surplus
    );
    // Junior-pool isolation: withdrawals exit only the user's own senior
    // capital; the junior residual pool (haircut/payout funding) cannot be
    // drained through withdraw.
    assert_eq!(market.kani_residual(), residual_before);
    assert_eq!(market.validate_shape(), Ok(()));
    assert_eq!(account.validate_with_market(&market.as_view()), Ok(()));
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_nonflat_withdraw_rejects_before_value_exit() {
    let start_capital_raw: u8 = kani::any();
    let other_capital_raw: u8 = kani::any();
    let insurance_raw: u8 = kani::any();
    let surplus_raw: u8 = kani::any();
    let amount_raw: u8 = kani::any();
    kani::assume(start_capital_raw <= 16);
    kani::assume(other_capital_raw <= 16);
    kani::assume(insurance_raw <= 16);
    kani::assume(surplus_raw <= 16);
    kani::assume(amount_raw > 0);
    let start_capital = start_capital_raw as u128;
    let other_capital = other_capital_raw as u128;
    let insurance = insurance_raw as u128;
    let surplus = surplus_raw as u128;
    let amount = amount_raw as u128;
    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    let c_tot_before = start_capital + other_capital;
    let vault_before = c_tot_before + insurance + surplus;
    header.vault = V16PodU128::new(vault_before);
    header.c_tot = V16PodU128::new(c_tot_before);
    header.insurance = V16PodU128::new(insurance);
    account_header.capital = V16PodU128::new(start_capital);
    let asset = markets[0].engine.asset.try_to_runtime().unwrap();
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
    let result = market.withdraw_not_atomic(&mut account, amount);

    kani::cover!(
        start_capital > 0 && other_capital > 0 && insurance > 0 && surplus > 0 && amount > 5,
        "nonflat withdraw rejection covers nonempty senior state without value mutation"
    );
    kani::cover!(
        start_capital == 0 && amount > 0,
        "nonflat withdraw rejection covers zero-capital active account"
    );
    assert_eq!(result, Err(V16Error::Stale));
    assert_eq!(market.header.vault.get(), vault_before);
    assert_eq!(market.header.c_tot.get(), c_tot_before);
    assert_eq!(market.header.insurance.get(), insurance);
    assert_eq!(account.header.capital.get(), start_capital);
    assert_eq!(
        market.header.vault.get() - market.header.c_tot.get() - market.header.insurance.get(),
        surplus
    );
    assert_eq!(market.validate_shape(), Ok(()));
    assert_eq!(account.validate_with_market(&market.as_view()), Ok(()));
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_withdraw_settles_flat_negative_pnl_before_value_exit() {
    let start_capital_raw: u8 = kani::any();
    let other_capital_raw: u8 = kani::any();
    let insurance_raw: u8 = kani::any();
    let surplus_raw: u8 = kani::any();
    let loss_raw: u8 = kani::any();
    let amount_raw: u8 = kani::any();
    kani::assume(start_capital_raw <= 16);
    kani::assume(other_capital_raw <= 16);
    kani::assume(insurance_raw <= 16);
    kani::assume(surplus_raw <= 16);
    kani::assume(loss_raw > 0);
    kani::assume(amount_raw > 0);
    let start_capital = start_capital_raw as u128;
    let other_capital = other_capital_raw as u128;
    let insurance = insurance_raw as u128;
    let surplus = surplus_raw as u128;
    let loss = loss_raw as u128;
    let amount = amount_raw as u128;
    kani::assume(loss <= start_capital);
    kani::assume(amount <= start_capital - loss);

    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    let c_tot_before = start_capital + other_capital;
    let vault_before = c_tot_before + insurance + surplus;
    header.vault = V16PodU128::new(vault_before);
    header.c_tot = V16PodU128::new(c_tot_before);
    header.insurance = V16PodU128::new(insurance);
    header.negative_pnl_account_count = V16PodU64::new(1);
    account_header.capital = V16PodU128::new(start_capital);
    account_header.pnl = V16PodI128::new(-(loss as i128));

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    market.withdraw_not_atomic(&mut account, amount).unwrap();

    kani::cover!(
        start_capital > loss + amount && other_capital > 0 && insurance > 0 && surplus > 0,
        "withdraw loss-seniority covers target account plus independent aggregate state"
    );
    kani::cover!(
        start_capital == loss + amount,
        "withdraw loss-seniority covers exact zero-capital target after loss and exit"
    );
    assert_eq!(account.header.pnl.get(), 0);
    assert_eq!(market.header.negative_pnl_account_count.get(), 0);
    assert_eq!(account.header.capital.get(), start_capital - loss - amount);
    assert_eq!(market.header.c_tot.get(), c_tot_before - loss - amount);
    assert_eq!(market.header.vault.get(), vault_before - amount);
    assert_eq!(market.header.insurance.get(), insurance);
    assert_eq!(
        market.header.vault.get() - market.header.c_tot.get() - market.header.insurance.get(),
        surplus + loss
    );
    assert_eq!(market.validate_shape(), Ok(()));
    assert_eq!(account.validate_with_market(&market.as_view()), Ok(()));
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_recovery_mode_blocks_withdraw() {
    let capital: u128 = kani::any();
    let other_capital: u128 = kani::any();
    let amount: u128 = kani::any();
    let insurance: u128 = kani::any();
    let surplus: u128 = kani::any();
    kani::assume(capital > 0);
    kani::assume(amount > 0);
    let c_tot = capital.checked_add(other_capital);
    kani::assume(c_tot.is_some());
    let senior = c_tot.unwrap().checked_add(insurance);
    kani::assume(senior.is_some());
    let vault = senior.unwrap().checked_add(surplus);
    kani::assume(vault.is_some());
    kani::assume(vault.unwrap() <= MAX_VAULT_TVL);
    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    header.mode = 2;
    header.recovery_reason = V16OptionalRecoveryReasonAccount::from_runtime(Some(
        PermissionlessRecoveryReasonV16::ExplicitLossOrDustAuditOverflow,
    ));
    header.vault = V16PodU128::new(vault.unwrap());
    header.c_tot = V16PodU128::new(c_tot.unwrap());
    header.insurance = V16PodU128::new(insurance);
    account_header.capital = V16PodU128::new(capital);
    let vault_before = header.vault;
    let c_tot_before = header.c_tot;
    let insurance_before = header.insurance;
    let capital_before = account_header.capital;

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    let result = market.withdraw_not_atomic(&mut account, amount);

    kani::cover!(
        amount > MAX_VAULT_TVL && capital > 0,
        "recovery mode blocks ordinary withdraw request beyond configured TVL-scale values"
    );
    kani::cover!(
        other_capital > 0 && insurance > 0 && surplus > 0,
        "recovery mode blocks ordinary withdraw with independent aggregate state"
    );
    assert_eq!(result, Err(V16Error::LockActive));
    assert_eq!(market.header.vault, vault_before);
    assert_eq!(market.header.c_tot, c_tot_before);
    assert_eq!(market.header.insurance, insurance_before);
    assert_eq!(account.header.capital, capital_before);
    assert_eq!(market.validate_shape(), Ok(()));
    assert_eq!(account.validate_with_market(&market.as_view()), Ok(()));
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_recovery_mode_blocks_fee_sync_and_pnl_conversion_before_mutation() {
    let capital: u128 = kani::any();
    let other_capital: u128 = kani::any();
    let pnl_raw: u8 = kani::any();
    let reserved_raw: u8 = kani::any();
    let fee_rate_raw: u8 = kani::any();
    let now_slot_raw: u8 = kani::any();
    let insurance: u128 = kani::any();
    let surplus: u128 = kani::any();
    let c_tot = capital.checked_add(other_capital);
    kani::assume(c_tot.is_some());
    let senior = c_tot.unwrap().checked_add(insurance);
    kani::assume(senior.is_some());
    let vault = senior.unwrap().checked_add(surplus);
    kani::assume(vault.is_some());
    kani::assume(vault.unwrap() <= MAX_VAULT_TVL);
    kani::assume(reserved_raw <= pnl_raw);
    kani::assume(now_slot_raw > 0);
    let pnl = pnl_raw as i128;
    let reserved = reserved_raw as u128;
    let now_slot = now_slot_raw as u64;
    let fee_rate = fee_rate_raw as u128;
    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    header.mode = 2;
    header.recovery_reason = V16OptionalRecoveryReasonAccount::from_runtime(Some(
        PermissionlessRecoveryReasonV16::ExplicitLossOrDustAuditOverflow,
    ));
    header.vault = V16PodU128::new(vault.unwrap());
    header.c_tot = V16PodU128::new(c_tot.unwrap());
    header.insurance = V16PodU128::new(insurance);
    account_header.capital = V16PodU128::new(capital);
    account_header.pnl = V16PodI128::new(pnl);
    account_header.reserved_pnl = V16PodU128::new(reserved);
    account_header.last_fee_slot = V16PodU64::new(0);
    let vault_before = header.vault;
    let c_tot_before = header.c_tot;
    let insurance_before = header.insurance;
    let capital_before = account_header.capital;
    let pnl_before = account_header.pnl;
    let reserved_before = account_header.reserved_pnl;
    let last_fee_slot_before = account_header.last_fee_slot;

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    let fee_result = market.sync_account_fee_to_slot_not_atomic(&mut account, now_slot, fee_rate);
    let convert_result = market.convert_released_pnl_to_capital_not_atomic(&mut account);

    kani::cover!(
        now_slot > 1 && fee_rate > 0 && pnl > reserved as i128 && other_capital > 0,
        "recovery mode blocks fee sync and released positive PnL conversion before mutation"
    );
    kani::cover!(
        pnl > 0 && reserved == pnl as u128 && insurance > 0 && surplus > 0,
        "recovery mode blocks fee sync and fully reserved positive PnL with senior state"
    );
    assert_eq!(fee_result, Err(V16Error::LockActive));
    assert_eq!(convert_result, Err(V16Error::LockActive));
    assert_eq!(market.header.vault, vault_before);
    assert_eq!(market.header.c_tot, c_tot_before);
    assert_eq!(market.header.insurance, insurance_before);
    assert_eq!(account.header.capital, capital_before);
    assert_eq!(account.header.pnl, pnl_before);
    assert_eq!(account.header.reserved_pnl, reserved_before);
    assert_eq!(account.header.last_fee_slot, last_fee_slot_before);
    assert_eq!(market.validate_shape(), Ok(()));
    assert_eq!(account.validate_with_market(&market.as_view()), Ok(()));
}

#[kani::proof]
#[kani::unwind(32)]
#[kani::solver(cadical)]
fn proof_v16_public_resolve_market_is_value_neutral_and_clears_loss_stale() {
    let current_slot_raw: u8 = kani::any();
    let stale_lag_raw: u8 = kani::any();
    let resolved_delta_raw: u8 = kani::any();
    let c_tot: u128 = kani::any();
    let insurance: u128 = kani::any();
    let surplus: u128 = kani::any();
    kani::assume(current_slot_raw > 0);
    kani::assume(stale_lag_raw < current_slot_raw);
    kani::assume(resolved_delta_raw <= 32);
    kani::assume(c_tot <= MAX_VAULT_TVL);
    kani::assume(insurance <= MAX_VAULT_TVL - c_tot);
    kani::assume(surplus <= MAX_VAULT_TVL - c_tot - insurance);
    let current_slot = current_slot_raw as u64;
    let slot_last = current_slot - stale_lag_raw as u64;
    let resolved_slot = current_slot + resolved_delta_raw as u64;
    let (mut header, mut markets, _) = one_market_view_fixture();
    header.vault = V16PodU128::new(c_tot + insurance + surplus);
    header.c_tot = V16PodU128::new(c_tot);
    header.insurance = V16PodU128::new(insurance);
    header.loss_stale_active = if slot_last < current_slot { 1 } else { 0 };
    header.current_slot = V16PodU64::new(current_slot);
    header.slot_last = V16PodU64::new(slot_last);
    let vault_before = header.vault;
    let c_tot_before = header.c_tot;
    let insurance_before = header.insurance;
    let slot_last_before = header.slot_last;
    let asset_before = markets[0].engine.asset;
    let long_budget_before = markets[0].engine.insurance_domain_budget_long;
    let short_budget_before = markets[0].engine.insurance_domain_budget_short;
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let residual_before = market.kani_residual();

    market.resolve_market_not_atomic(resolved_slot).unwrap();

    kani::cover!(
        stale_lag_raw == 0 && resolved_delta_raw == 0 && c_tot > 255,
        "resolved market transition covers already-current same-slot resolution"
    );
    kani::cover!(
        stale_lag_raw > 0
            && resolved_delta_raw > 0
            && c_tot > 255
            && insurance > 255
            && surplus > 255,
        "resolved market transition covers future authenticated slot over wide symbolic value state"
    );
    assert_eq!(market.header.mode, 1);
    assert_eq!(market.header.resolved_slot.get(), resolved_slot);
    assert_eq!(market.header.current_slot.get(), resolved_slot);
    assert_eq!(market.header.slot_last, slot_last_before);
    assert_eq!(market.header.loss_stale_active, 0);
    assert_eq!(market.header.vault, vault_before);
    assert_eq!(market.header.c_tot, c_tot_before);
    assert_eq!(market.header.insurance, insurance_before);
    assert_eq!(market.markets[0].engine.asset, asset_before);
    assert_eq!(
        market.markets[0].engine.insurance_domain_budget_long,
        long_budget_before
    );
    assert_eq!(
        market.markets[0].engine.insurance_domain_budget_short,
        short_budget_before
    );
    assert_eq!(market.validate_shape(), Ok(()));
    // Junior-pool isolation: this transition must not move the junior
    // residual pool (haircut/payout funding).
    assert_eq!(market.kani_residual(), residual_before);
}

#[kani::proof]
#[kani::unwind(80)]
#[kani::solver(cadical)]
fn proof_v16_open_source_claim_exposure_blocks_convert() {
    let claim_raw: u8 = kani::any();
    kani::assume(claim_raw > 0);
    let claim = claim_raw as u128;
    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    let market_id = markets[0].engine.asset.market_id.get();
    let face_num = claim * BOUND_SCALE;
    let mut bitmap = account_header.active_bitmap.map(V16PodU64::get);
    active_bitmap_set(&mut bitmap, 0).unwrap();
    let leg = PortfolioLegV16 {
        active: true,
        asset_index: 0,
        market_id,
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
    };
    account_header.legs[0] = PortfolioLegV16Account::from_runtime(&leg);
    account_header.active_bitmap = bitmap.map(V16PodU64::new);
    account_header.pnl = V16PodI128::new(claim as i128);
    account_header.health_cert = HealthCertV16Account::from_runtime(&HealthCertV16 {
        certified_equity: 100,
        certified_initial_req: 1,
        certified_maintenance_req: 1,
        certified_liq_deficit: 0,
        certified_worst_case_loss: 1,
        cert_oracle_epoch: header.oracle_epoch.get(),
        cert_funding_epoch: header.funding_epoch.get(),
        cert_risk_epoch: header.risk_epoch.get(),
        cert_asset_set_epoch: header.asset_set_epoch.get(),
        active_bitmap_at_cert: bitmap,
        valid: true,
    });
    markets[0].engine.source_credit_short =
        SourceCreditStateV16Account::from_runtime(&SourceCreditStateV16 {
            positive_claim_bound_num: face_num,
            exact_positive_claim_num: face_num,
            credit_rate_num: 0,
            ..SourceCreditStateV16::EMPTY
        });
    account_header.source_domains[0].domain = V16PodU32::new(1);
    account_header.source_domains[0].source_claim_market_id = V16PodU64::new(market_id);
    account_header.source_domains[0].source_claim_bound_num = V16PodU128::new(face_num);
    let market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let account = PortfolioV16ViewMut::new(&mut account_header);

    let blocked = market
        .kani_convert_source_claim_exposure_guard(&account.as_view())
        .unwrap();

    kani::cover!(
        blocked && claim > 10,
        "active source-claim exposure reaches convert guard for wide symbolic claim"
    );
    assert!(blocked);
}

// Public-path favorable-action freshness: conversion preflight accepts exactly
// the current/unlocked branch and rejects stale certificates or h-lock lanes
// before value mutation. This links the cert-currentness/h-lock kernels to the
// production entrypoint preflight instead of proving only the private leaf
// predicates.
#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_public_convert_preflight_requires_current_unlocked_account() {
    let blocker: u8 = kani::any();
    kani::assume(blocker <= 5);

    let claim = 3u128;
    let (mut header, mut markets, mut account_header) = one_market_view_fixture();

    header.vault = V16PodU128::new(claim);
    header.c_tot = V16PodU128::new(claim);
    if blocker == 4 {
        header.threshold_stress_active = 1;
    }

    account_header.capital = V16PodU128::new(claim);
    account_header.pnl = V16PodI128::new(claim as i128);
    if blocker == 3 {
        account_header.stale_state = 1;
    }
    account_header.health_cert = HealthCertV16Account::from_runtime(&HealthCertV16 {
        certified_equity: claim as i128,
        certified_initial_req: 0,
        certified_maintenance_req: 0,
        certified_liq_deficit: 0,
        certified_worst_case_loss: 0,
        cert_oracle_epoch: header.oracle_epoch.get() + if blocker == 1 { 1 } else { 0 },
        cert_funding_epoch: header.funding_epoch.get(),
        cert_risk_epoch: header.risk_epoch.get(),
        cert_asset_set_epoch: header.asset_set_epoch.get(),
        active_bitmap_at_cert: if blocker == 2 {
            [1u64; V16_ACTIVE_BITMAP_WORDS]
        } else {
            V16_EMPTY_ACTIVE_BITMAP
        },
        valid: blocker != 0,
    });

    let vault_before = header.vault;
    let c_tot_before = header.c_tot;
    let account_capital_before = account_header.capital;
    let account_pnl_before = account_header.pnl;
    let market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let account = PortfolioV16View::new(&account_header);

    let result = market.kani_preflight_convert_released_pnl_to_capital(&account);

    kani::cover!(
        blocker <= 2 && claim > 1 && result == Err(V16Error::Stale),
        "stale health certificate blocks favorable conversion before mutation"
    );
    kani::cover!(
        (blocker == 3 || blocker == 4) && claim > 1 && result == Err(V16Error::LockActive),
        "h-lock lane blocks favorable conversion before mutation"
    );
    kani::cover!(
        blocker == 5 && claim > 1 && result == Ok(()),
        "current unlocked account passes favorable conversion preflight"
    );

    let expected = if blocker <= 2 {
        Err(V16Error::Stale)
    } else if blocker <= 4 {
        Err(V16Error::LockActive)
    } else {
        Ok(())
    };
    assert_eq!(result, expected);
    assert_eq!(market.header.vault, vault_before);
    assert_eq!(market.header.c_tot, c_tot_before);
    assert_eq!(account.header.capital, account_capital_before);
    assert_eq!(account.header.pnl, account_pnl_before);
}

#[kani::proof]
#[kani::unwind(24)]
#[kani::solver(cadical)]
fn proof_v16_bankruptcy_hlock_selects_hmax_before_source_backed_value_exit() {
    let claim_raw: u8 = kani::any();
    kani::assume(claim_raw > 0);
    let claim = claim_raw as u128;
    let claim_num = claim * BOUND_SCALE;
    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    account_header.pnl = V16PodI128::new(claim as i128);
    account_header.health_cert = HealthCertV16Account::from_runtime(&HealthCertV16 {
        certified_equity: claim as i128,
        certified_initial_req: 0,
        certified_maintenance_req: 0,
        certified_liq_deficit: 0,
        certified_worst_case_loss: 0,
        cert_oracle_epoch: header.oracle_epoch.get(),
        cert_funding_epoch: header.funding_epoch.get(),
        cert_risk_epoch: header.risk_epoch.get(),
        cert_asset_set_epoch: header.asset_set_epoch.get(),
        active_bitmap_at_cert: V16_EMPTY_ACTIVE_BITMAP,
        valid: true,
    });
    header.pnl_pos_tot = V16PodU128::new(claim);
    header.pnl_pos_bound_tot_num = V16PodU128::new(claim_num);
    header.pnl_pos_bound_tot = V16PodU128::new(claim);
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
    account_header.source_domains[0].domain = V16PodU32::new(0);
    account_header.source_domains[0].source_claim_market_id = V16PodU64::new(1);
    account_header.source_domains[0].source_claim_bound_num = V16PodU128::new(claim_num);
    // CLEAN-ROOM FIX (was vacuous): master welded bankruptcy_hlock_active = 1 and
    // passed instruction_candidate = false, so h_lock_lane returns HMax on the
    // bankruptcy bit BEFORE reading any of the source-backed fixture — the entire
    // source-credit/backing/cert state above was inert and assert_eq!(lane, HMax)
    // could not fail for ANY implementation. Fix: make the bankruptcy bit AND the
    // instruction-candidate flag symbolic and assert the EXACT discriminant
    // lane == HMax  <=>  (bit OR candidate). The HMin arm is the real content: it
    // proves the source-backed positive-PnL fixture triggers NONE of the
    // account-side hmax conditions (stale, b-stale, loss-stale live leg, pending
    // close residual, domain loss barrier), so the bankruptcy bit / candidate is
    // the operative discriminant before any source-backed value exit. (Fixture
    // mode is Live and threshold_stress_active is 0, so those market-side
    // disjuncts are inactive.)
    let hlock_active: bool = kani::any();
    let instruction_candidate: bool = kani::any();
    header.bankruptcy_hlock_active = if hlock_active { 1 } else { 0 };
    let vault_before = header.vault;
    let c_tot_before = header.c_tot;
    let capital_before = account_header.capital;
    let pnl_before = account_header.pnl;

    let market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let account = PortfolioV16ViewMut::new(&mut account_header);
    let lane = market
        .kani_h_lock_lane(Some(&account.as_view()), instruction_candidate)
        .unwrap();

    kani::cover!(
        claim > 1 && hlock_active && !instruction_candidate,
        "bankruptcy h-lock bit alone selects hmax for nontrivial source-backed PnL"
    );
    kani::cover!(
        claim > 1 && !hlock_active && instruction_candidate,
        "instruction-local bankruptcy candidate alone selects hmax"
    );
    kani::cover!(
        claim > 1 && !hlock_active && !instruction_candidate,
        "source-backed positive PnL alone does NOT force hmax (HMin arm)"
    );
    let expected = if hlock_active || instruction_candidate {
        HLockLaneV16::HMax
    } else {
        HLockLaneV16::HMin
    };
    assert_eq!(lane, expected);
    // h_lock_lane is a read-only query: no state moves.
    assert_eq!(market.header.vault, vault_before);
    assert_eq!(market.header.c_tot, c_tot_before);
    assert_eq!(account.header.capital, capital_before);
    assert_eq!(account.header.pnl, pnl_before);
}

#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn proof_v16_global_hlock_lane_selects_hmax_only_for_global_stress_or_candidate() {
    let threshold_stress_active: bool = kani::any();
    let bankruptcy_hlock_active: bool = kani::any();
    let recovery_mode: bool = kani::any();
    let instruction_bankruptcy_candidate: bool = kani::any();
    let (mut header, mut markets) = one_market_only_fixture();
    header.threshold_stress_active = threshold_stress_active as u8;
    header.bankruptcy_hlock_active = bankruptcy_hlock_active as u8;
    header.mode = if recovery_mode { 2 } else { 0 };
    let market = MarketGroupV16ViewMut::new(&mut header, &mut markets);

    let lane = market
        .kani_h_lock_lane(None, instruction_bankruptcy_candidate)
        .unwrap();
    let expected = if threshold_stress_active
        || bankruptcy_hlock_active
        || recovery_mode
        || instruction_bankruptcy_candidate
    {
        HLockLaneV16::HMax
    } else {
        HLockLaneV16::HMin
    };

    kani::cover!(
        lane == HLockLaneV16::HMin,
        "global h-lock lane covers healthy h_min branch"
    );
    kani::cover!(
        lane == HLockLaneV16::HMax && threshold_stress_active,
        "global h-lock lane covers threshold stress h_max branch"
    );
    kani::cover!(
        lane == HLockLaneV16::HMax && bankruptcy_hlock_active,
        "global h-lock lane covers bankruptcy h_max branch"
    );
    kani::cover!(
        lane == HLockLaneV16::HMax && recovery_mode,
        "global h-lock lane covers recovery h_max branch"
    );
    kani::cover!(
        lane == HLockLaneV16::HMax && instruction_bankruptcy_candidate,
        "global h-lock lane covers same-instruction candidate h_max branch"
    );
    assert_eq!(lane, expected);
}

#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn proof_v16_view_trade_position_delta_preserves_oi_symmetry() {
    let size_q: u128 = kani::any();
    let loss_weight: u128 = kani::any();
    let first_account_long: bool = kani::any();
    kani::assume(size_q > 0);
    kani::assume(size_q <= MAX_TRADE_SIZE_Q);
    let signed_size_q = if first_account_long {
        size_q as i128
    } else {
        -(size_q as i128)
    };
    let (abs_q, first_delta, second_delta) =
        MarketGroupV16ViewMut::<u64>::kani_trade_signed_size_deltas(signed_size_q).unwrap();
    let mut asset = AssetStateV16::default();
    let before = asset;

    let first_side = if first_delta > 0 {
        SideV16::Long
    } else {
        SideV16::Short
    };
    let second_side = if second_delta > 0 {
        SideV16::Long
    } else {
        SideV16::Short
    };
    kani_add_open_interest_for_new_position(&mut asset, first_side, abs_q, loss_weight).unwrap();
    kani_add_open_interest_for_new_position(&mut asset, second_side, abs_q, loss_weight).unwrap();

    kani::cover!(
        first_account_long && size_q > POS_SCALE && loss_weight > POS_SCALE,
        "trade open-interest accounting covers first-account long with nontrivial size and weight"
    );
    kani::cover!(
        !first_account_long && size_q > POS_SCALE && loss_weight > POS_SCALE,
        "trade open-interest accounting covers first-account short with nontrivial size and weight"
    );
    assert_eq!(abs_q, size_q);
    assert_eq!(first_delta.checked_add(second_delta), Some(0));
    assert_eq!(asset.oi_eff_long_q, size_q);
    assert_eq!(asset.oi_eff_short_q, size_q);
    assert_eq!(asset.loss_weight_sum_long, loss_weight);
    assert_eq!(asset.loss_weight_sum_short, loss_weight);
    assert_eq!(asset.stored_pos_count_long, 1);
    assert_eq!(asset.stored_pos_count_short, 1);
    assert_eq!(asset.market_id, before.market_id);
    assert_eq!(asset.effective_price, before.effective_price);
    assert_eq!(asset.k_long, before.k_long);
    assert_eq!(asset.k_short, before.k_short);
    assert_eq!(asset.f_long_num, before.f_long_num);
    assert_eq!(asset.f_short_num, before.f_short_num);
    assert_eq!(asset.b_long_num, before.b_long_num);
    assert_eq!(asset.b_short_num, before.b_short_num);
}

#[kani::proof]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn proof_v16_signed_trade_request_maps_to_opposite_account_deltas() {
    let size_q: i128 = kani::any();
    let abs_size_q = size_q.unsigned_abs();
    let expected_ok =
        size_q != 0 && abs_size_q <= MAX_TRADE_SIZE_Q && size_q.checked_neg().is_some();

    let result = MarketGroupV16ViewMut::<u64>::kani_trade_signed_size_deltas(size_q);

    kani::cover!(
        expected_ok && size_q > 0,
        "signed trade request covers first-account-long leg"
    );
    kani::cover!(
        expected_ok && size_q < 0,
        "signed trade request covers first-account-short leg"
    );
    kani::cover!(
        size_q == 0,
        "signed trade request covers zero-size rejection"
    );
    kani::cover!(
        abs_size_q > MAX_TRADE_SIZE_Q,
        "signed trade request covers max-size rejection"
    );
    assert_eq!(result.is_ok(), expected_ok);
    if let Ok((abs_q, first_delta, second_delta)) = result {
        assert_eq!(abs_q, abs_size_q);
        assert_eq!(first_delta, size_q);
        assert_eq!(second_delta, size_q.checked_neg().unwrap());
        assert_eq!(first_delta.checked_add(second_delta), Some(0));
        assert!(abs_q > 0);
        assert!(abs_q <= MAX_TRADE_SIZE_Q);
    }
}

#[kani::proof]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn proof_v16_adjust_u128_applies_exact_delta_or_fails_closed() {
    let current: u128 = kani::any();
    let old: u128 = kani::any();
    let new: u128 = kani::any();
    let result = percolator::v16::kani_adjust_u128(current, old, new);

    kani::cover!(
        new > old && current <= u128::MAX - (new - old),
        "u128 adjustment covers increasing aggregate without overflow"
    );
    kani::cover!(
        new > old && current > u128::MAX - (new - old),
        "u128 adjustment covers fail-closed aggregate overflow"
    );
    kani::cover!(
        new < old && current >= old - new,
        "u128 adjustment covers decreasing aggregate without underflow"
    );
    kani::cover!(
        new < old && current < old - new,
        "u128 adjustment covers fail-closed aggregate underflow"
    );
    kani::cover!(new == old, "u128 adjustment covers identity update");
    if new >= old {
        let delta = new - old;
        if let Some(expected) = current.checked_add(delta) {
            assert_eq!(result, Ok(expected));
            assert_eq!(expected - current, delta);
        } else {
            assert_eq!(result, Err(V16Error::ArithmeticOverflow));
        }
    } else {
        let delta = old - new;
        if let Some(expected) = current.checked_sub(delta) {
            assert_eq!(result, Ok(expected));
            assert_eq!(current - expected, delta);
        } else {
            assert_eq!(result, Err(V16Error::CounterUnderflow));
        }
    }
}

#[kani::proof]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn proof_v16_position_delta_risk_classifier_matches_abs_exposure_change() {
    let current: i128 = kani::any();
    let delta_q: i128 = kani::any();
    kani::assume(current != i128::MIN);
    kani::assume(current.unsigned_abs() <= MAX_POSITION_ABS_Q);
    kani::assume(delta_q != i128::MIN);
    kani::assume(delta_q.unsigned_abs() <= MAX_TRADE_SIZE_Q);

    let next = current.checked_add(delta_q);
    let expected_ok = match next {
        Some(next) => next == 0 || (next != i128::MIN && next.unsigned_abs() <= MAX_POSITION_ABS_Q),
        None => false,
    };
    let result = kani_position_delta_increases_risk(current, delta_q);

    kani::cover!(
        expected_ok && result == Ok(true) && current != 0 && delta_q.signum() == current.signum(),
        "risk classifier covers same-side exposure increase"
    );
    kani::cover!(
        expected_ok && result == Ok(false) && current != 0 && delta_q.signum() != current.signum(),
        "risk classifier covers risk reduction or side flip"
    );
    kani::cover!(
        !expected_ok,
        "risk classifier covers invalid next-position rejection"
    );
    assert_eq!(result.is_ok(), expected_ok);
    if let Ok(increases) = result {
        let next = next.unwrap();
        assert_eq!(increases, next.unsigned_abs() > current.unsigned_abs());
        if current == 0 {
            assert_eq!(increases, next != 0);
        }
        if increases {
            assert!(next != 0);
        }
    }
}

#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn proof_v16_trade_preflight_risk_gate_blocks_only_unsafe_risk_increase() {
    let long_current: i128 = kani::any();
    let short_current: i128 = kani::any();
    let trade_size: i128 = kani::any();
    let asset_loss_stale: bool = kani::any();
    let target_effective_lag: bool = kani::any();
    let pending_barrier: bool = kani::any();
    kani::assume(long_current != i128::MIN);
    kani::assume(short_current != i128::MIN);
    kani::assume(long_current.unsigned_abs() <= MAX_POSITION_ABS_Q);
    kani::assume(short_current.unsigned_abs() <= MAX_POSITION_ABS_Q);
    kani::assume(trade_size != 0 && trade_size != i128::MIN);
    kani::assume(trade_size.unsigned_abs() <= MAX_TRADE_SIZE_Q);
    let short_delta_opt = trade_size.checked_neg();
    kani::assume(short_delta_opt.is_some());
    let short_delta = short_delta_opt.unwrap();
    let long_next_opt = long_current.checked_add(trade_size);
    let short_next_opt = short_current.checked_add(short_delta);
    kani::assume(long_next_opt.is_some());
    kani::assume(short_next_opt.is_some());
    let long_next = long_next_opt.unwrap();
    let short_next = short_next_opt.unwrap();
    let long_risk_result = kani_position_delta_increases_risk(long_current, trade_size);
    let short_risk_result = kani_position_delta_increases_risk(short_current, short_delta);
    kani::assume(long_risk_result.is_ok());
    kani::assume(short_risk_result.is_ok());
    let long_risk = long_risk_result.unwrap();
    let short_risk = short_risk_result.unwrap();
    let risk_increasing = long_risk || short_risk;
    let result = kani_trade_preflight_risk_gate(
        risk_increasing,
        asset_loss_stale,
        target_effective_lag,
        pending_barrier,
    );
    let expected_blocked =
        pending_barrier || (risk_increasing && (asset_loss_stale || target_effective_lag));

    kani::cover!(
        expected_blocked && asset_loss_stale && !target_effective_lag && !pending_barrier,
        "trade preflight risk gate blocks risk increase on loss-stale asset"
    );
    kani::cover!(
        expected_blocked && target_effective_lag && !asset_loss_stale && !pending_barrier,
        "trade preflight risk gate blocks risk increase on target/effective lag"
    );
    kani::cover!(
        !risk_increasing && (asset_loss_stale || target_effective_lag) && !pending_barrier,
        "trade preflight risk gate allows pure spread reduction under stale-or-lag state"
    );
    kani::cover!(
        risk_increasing && !asset_loss_stale && !target_effective_lag && !pending_barrier,
        "trade preflight risk gate allows healthy risk increase"
    );
    kani::cover!(
        pending_barrier && !risk_increasing,
        "trade preflight risk gate blocks pending-domain barrier even for non-increasing deltas"
    );
    kani::cover!(
        trade_size.unsigned_abs() > 1024 && long_current.unsigned_abs() > 1024,
        "trade preflight risk gate proof covers wide symbolic position and trade magnitudes"
    );
    assert_eq!(long_next, long_current.checked_add(trade_size).unwrap());
    assert_eq!(short_next, short_current.checked_add(short_delta).unwrap());
    assert_eq!(result.is_ok(), !expected_blocked);
    if expected_blocked {
        assert_eq!(result, Err(V16Error::LockActive));
    } else {
        assert_eq!(result, Ok(()));
    }
}

#[kani::proof]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn proof_v16_batch_outcome_accumulator_is_exact_and_overflow_checked() {
    let fill_count: u32 = kani::any();
    let fee_a: u128 = kani::any();
    let fee_b: u128 = kani::any();
    let notional: u128 = kani::any();
    let add_fee_a: u128 = kani::any();
    let add_fee_b: u128 = kani::any();
    let add_notional: u128 = kani::any();
    let risk_before: bool = kani::any();
    let long_claim_before: bool = kani::any();
    let short_claim_before: bool = kani::any();
    let applied_risk: bool = kani::any();
    let applied_long_claim: bool = kani::any();
    let applied_short_claim: bool = kani::any();
    let mut outcome = BatchTradeOutcomeV16 {
        fill_count,
        fee_a,
        fee_b,
        notional,
    };
    let mut risk = risk_before;
    let mut long_claim = long_claim_before;
    let mut short_claim = short_claim_before;

    let expected_fill = fill_count.checked_add(1);
    let expected_fee_a = fee_a.checked_add(add_fee_a);
    let expected_fee_b = fee_b.checked_add(add_fee_b);
    let expected_notional = notional.checked_add(add_notional);
    let expected_ok = expected_fill.is_some()
        && expected_fee_a.is_some()
        && expected_fee_b.is_some()
        && expected_notional.is_some();

    let result = MarketGroupV16ViewMut::<u64>::kani_accumulate_batch_trade_apply(
        &mut outcome,
        &mut risk,
        &mut long_claim,
        &mut short_claim,
        add_fee_a,
        add_fee_b,
        add_notional,
        applied_risk,
        applied_long_claim,
        applied_short_claim,
    );

    kani::cover!(
        expected_ok
            && add_fee_a != 0
            && add_fee_b != 0
            && add_notional != 0
            && applied_risk
            && applied_long_claim
            && applied_short_claim,
        "batch accumulator covers a nontrivial successful fill aggregation"
    );
    kani::cover!(!expected_ok, "batch accumulator covers overflow rejection");
    assert_eq!(result.is_ok(), expected_ok);
    if result.is_ok() {
        assert_eq!(outcome.fill_count, expected_fill.unwrap());
        assert_eq!(outcome.fee_a, expected_fee_a.unwrap());
        assert_eq!(outcome.fee_b, expected_fee_b.unwrap());
        assert_eq!(outcome.notional, expected_notional.unwrap());
        assert_eq!(risk, risk_before || applied_risk);
        assert_eq!(long_claim, long_claim_before || applied_long_claim);
        assert_eq!(short_claim, short_claim_before || applied_short_claim);
    }
}

#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn proof_v16_wrapper_shape_distinct_asset_batch_projection_preserves_oi_and_outcome() {
    let first_account_long0: bool = kani::any();
    let first_account_long1: bool = kani::any();
    let q0_units: u8 = kani::any();
    let q1_units: u8 = kani::any();
    let loss_weight0_raw: u8 = kani::any();
    let loss_weight1_raw: u8 = kani::any();
    let fee_a0: u16 = kani::any();
    let fee_b0: u16 = kani::any();
    let fee_a1: u16 = kani::any();
    let fee_b1: u16 = kani::any();
    let notional0: u16 = kani::any();
    let notional1: u16 = kani::any();
    let risk0: bool = kani::any();
    let risk1: bool = kani::any();
    let long_claim0: bool = kani::any();
    let long_claim1: bool = kani::any();
    let short_claim0: bool = kani::any();
    let short_claim1: bool = kani::any();

    kani::assume((1..=8).contains(&q0_units));
    kani::assume((1..=8).contains(&q1_units));
    kani::assume(loss_weight0_raw > 0);
    kani::assume(loss_weight1_raw > 0);
    kani::assume(notional0 > 0);
    kani::assume(notional1 > 0);
    let q0 = q0_units as u128 * POS_SCALE;
    let q1 = q1_units as u128 * POS_SCALE;
    let signed_q0 = if first_account_long0 {
        q0 as i128
    } else {
        -(q0 as i128)
    };
    let signed_q1 = if first_account_long1 {
        q1 as i128
    } else {
        -(q1 as i128)
    };

    let (abs_q0, delta_a0, delta_b0) =
        MarketGroupV16ViewMut::<u64>::kani_trade_signed_size_deltas(signed_q0).unwrap();
    let (abs_q1, delta_a1, delta_b1) =
        MarketGroupV16ViewMut::<u64>::kani_trade_signed_size_deltas(signed_q1).unwrap();
    let mut asset0 = AssetStateV16::default();
    let mut asset1 = AssetStateV16::default();
    let side_a0 = if delta_a0 > 0 {
        SideV16::Long
    } else {
        SideV16::Short
    };
    let side_b0 = if delta_b0 > 0 {
        SideV16::Long
    } else {
        SideV16::Short
    };
    let side_a1 = if delta_a1 > 0 {
        SideV16::Long
    } else {
        SideV16::Short
    };
    let side_b1 = if delta_b1 > 0 {
        SideV16::Long
    } else {
        SideV16::Short
    };
    let loss_weight0 = loss_weight0_raw as u128;
    let loss_weight1 = loss_weight1_raw as u128;
    kani_add_open_interest_for_new_position(&mut asset0, side_a0, abs_q0, loss_weight0).unwrap();
    kani_add_open_interest_for_new_position(&mut asset0, side_b0, abs_q0, loss_weight0).unwrap();
    kani_add_open_interest_for_new_position(&mut asset1, side_a1, abs_q1, loss_weight1).unwrap();
    kani_add_open_interest_for_new_position(&mut asset1, side_b1, abs_q1, loss_weight1).unwrap();

    let mut outcome = BatchTradeOutcomeV16 {
        fill_count: 0,
        fee_a: 0,
        fee_b: 0,
        notional: 0,
    };
    let mut risk_increasing = false;
    let mut long_has_source_claims = false;
    let mut short_has_source_claims = false;
    MarketGroupV16ViewMut::<u64>::kani_accumulate_batch_trade_apply(
        &mut outcome,
        &mut risk_increasing,
        &mut long_has_source_claims,
        &mut short_has_source_claims,
        fee_a0 as u128,
        fee_b0 as u128,
        notional0 as u128,
        risk0,
        long_claim0,
        short_claim0,
    )
    .unwrap();
    MarketGroupV16ViewMut::<u64>::kani_accumulate_batch_trade_apply(
        &mut outcome,
        &mut risk_increasing,
        &mut long_has_source_claims,
        &mut short_has_source_claims,
        fee_a1 as u128,
        fee_b1 as u128,
        notional1 as u128,
        risk1,
        long_claim1,
        short_claim1,
    )
    .unwrap();

    kani::cover!(
        first_account_long0 && !first_account_long1 && fee_a0 > 0 && fee_b1 > 0,
        "wrapper-shape batch projection covers mixed long/short spread with nonzero fees"
    );
    kani::cover!(
        !first_account_long0 && first_account_long1 && risk0 && !risk1,
        "wrapper-shape batch projection covers inverse spread and asymmetric risk flag"
    );
    kani::cover!(
        long_claim0 && !long_claim1 && !short_claim0 && short_claim1,
        "wrapper-shape batch projection covers independent source-claim flag aggregation"
    );
    assert_ne!(side_a0, side_b0);
    assert_ne!(side_a1, side_b1);
    assert_eq!(delta_a0.checked_add(delta_b0), Some(0));
    assert_eq!(delta_a1.checked_add(delta_b1), Some(0));
    assert_eq!(asset0.oi_eff_long_q, q0);
    assert_eq!(asset0.oi_eff_short_q, q0);
    assert_eq!(asset1.oi_eff_long_q, q1);
    assert_eq!(asset1.oi_eff_short_q, q1);
    assert_eq!(asset0.loss_weight_sum_long, loss_weight0);
    assert_eq!(asset0.loss_weight_sum_short, loss_weight0);
    assert_eq!(asset1.loss_weight_sum_long, loss_weight1);
    assert_eq!(asset1.loss_weight_sum_short, loss_weight1);
    assert_eq!(outcome.fill_count, 2);
    assert_eq!(outcome.fee_a, fee_a0 as u128 + fee_a1 as u128);
    assert_eq!(outcome.fee_b, fee_b0 as u128 + fee_b1 as u128);
    assert_eq!(outcome.notional, notional0 as u128 + notional1 as u128);
    assert_eq!(risk_increasing, risk0 || risk1);
    assert_eq!(long_has_source_claims, long_claim0 || long_claim1);
    assert_eq!(short_has_source_claims, short_claim0 || short_claim1);
}

#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn proof_v16_final_batch_margin_gate_accepts_only_final_certified_im() {
    let equity: i128 = kani::any();
    let req: u128 = kani::any();
    let cert_valid: bool = kani::any();
    kani::assume(equity != i128::MIN);
    let mut account_header = PortfolioAccountV16Account::default();
    account_header.health_cert = HealthCertV16Account::from_runtime(&HealthCertV16 {
        certified_equity: equity,
        certified_initial_req: req,
        certified_maintenance_req: req,
        certified_liq_deficit: 0,
        certified_worst_case_loss: req,
        cert_oracle_epoch: 0,
        cert_funding_epoch: 0,
        cert_risk_epoch: 0,
        cert_asset_set_epoch: 0,
        active_bitmap_at_cert: V16_EMPTY_ACTIVE_BITMAP,
        valid: cert_valid,
    });

    let account = PortfolioV16View::new(&account_header);
    let result = MarketGroupV16ViewMut::<u64>::kani_ensure_initial_margin(&account);
    let expected_ok = cert_valid && equity >= 0 && (equity as u128) >= req;

    kani::cover!(
        expected_ok && (equity as u128) > req,
        "final batch margin gate covers accepting overcollateralized certificates"
    );
    kani::cover!(
        cert_valid && equity < 0,
        "final batch margin gate covers rejecting negative final equity"
    );
    kani::cover!(
        cert_valid && equity >= 0 && (equity as u128) < req,
        "final batch margin gate covers rejecting undercollateralized certificates"
    );
    kani::cover!(
        !cert_valid,
        "final batch margin gate covers rejecting stale final certificates"
    );
    assert_eq!(result.is_ok(), expected_ok);
}

#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn proof_v16_locked_trade_margin_gate_cannot_use_positive_pnl_credit() {
    let capital_raw: u16 = kani::any();
    let pnl_raw: i16 = kani::any();
    let fee_debt_raw: u16 = kani::any();
    let req_raw: u16 = kani::any();
    kani::assume(capital_raw <= 2_000);
    kani::assume((-2_000..=2_000).contains(&pnl_raw));
    kani::assume(fee_debt_raw <= 2_000);
    kani::assume(req_raw <= 4_000);

    let capital = capital_raw as u128;
    let pnl = pnl_raw as i128;
    let fee_debt = fee_debt_raw as i128;
    let req = req_raw as u128;
    let certified_equity = (capital_raw as i128) + pnl - fee_debt;
    let no_positive_equity = (capital_raw as i128) + pnl.min(0) - fee_debt;

    let mut account_header = PortfolioAccountV16Account::default();
    account_header.capital = V16PodU128::new(capital);
    account_header.pnl = V16PodI128::new(pnl);
    account_header.fee_credits = V16PodI128::new(-fee_debt);
    account_header.health_cert = HealthCertV16Account::from_runtime(&HealthCertV16 {
        certified_equity,
        certified_initial_req: req,
        certified_maintenance_req: req,
        certified_liq_deficit: 0,
        certified_worst_case_loss: req,
        cert_oracle_epoch: 0,
        cert_funding_epoch: 0,
        cert_risk_epoch: 0,
        cert_asset_set_epoch: 0,
        active_bitmap_at_cert: V16_EMPTY_ACTIVE_BITMAP,
        valid: true,
    });

    let account = PortfolioV16View::new(&account_header);
    let certified_result = MarketGroupV16ViewMut::<u64>::kani_ensure_initial_margin(&account);
    let locked_result =
        MarketGroupV16ViewMut::<u64>::kani_ensure_no_positive_credit_initial_margin(&account);
    let expected_certified_ok = certified_equity >= 0 && (certified_equity as u128) >= req;
    let expected_locked_ok = no_positive_equity >= 0 && (no_positive_equity as u128) >= req;

    kani::cover!(
        pnl > 0 && expected_certified_ok && !expected_locked_ok,
        "locked trade final check rejects margin that only passes with positive PnL credit"
    );
    kani::cover!(
        pnl <= 0 && expected_certified_ok && expected_locked_ok && fee_debt_raw > 0,
        "locked trade final check accepts fee-adjusted principal-only margin"
    );
    kani::cover!(
        fee_debt > capital_raw as i128 && !expected_locked_ok,
        "locked trade final check rejects fee debt exceeding principal"
    );
    assert_eq!(certified_result.is_ok(), expected_certified_ok);
    assert_eq!(locked_result.is_ok(), expected_locked_ok);
    if expected_certified_ok && !expected_locked_ok {
        assert_eq!(locked_result, Err(V16Error::LockActive));
    }
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_live_market_shape_rejects_long_short_oi_mismatch() {
    let long_units_raw: u8 = kani::any();
    let short_units_raw: u8 = kani::any();
    kani::assume(long_units_raw > 0);
    kani::assume(short_units_raw > 0);
    kani::assume(long_units_raw != short_units_raw);
    let (mut header, mut markets, _) = one_market_view_fixture();
    let mut asset = markets[0].engine.asset.try_to_runtime().unwrap();
    asset.oi_eff_long_q = long_units_raw as u128 * POS_SCALE;
    asset.oi_eff_short_q = short_units_raw as u128 * POS_SCALE;
    asset.loss_weight_sum_long = long_units_raw as u128 * POS_SCALE;
    asset.loss_weight_sum_short = short_units_raw as u128 * POS_SCALE;
    asset.stored_pos_count_long = 1;
    asset.stored_pos_count_short = 1;
    markets[0].engine.asset = AssetStateV16Account::from_runtime(&asset);

    let market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let result = market.validate_shape();

    kani::cover!(
        long_units_raw > 5 && long_units_raw > short_units_raw,
        "OI mismatch proof covers wide long-heavy invalid state"
    );
    kani::cover!(
        short_units_raw > 5 && short_units_raw > long_units_raw,
        "OI mismatch proof covers wide short-heavy invalid state"
    );
    assert_eq!(result, Err(V16Error::InvalidConfig));
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_pending_domain_loss_barrier_detects_touching_position_changes() {
    let long_position_raw: u8 = kani::any();
    let short_position_raw: u8 = kani::any();
    kani::assume(long_position_raw > 0);
    kani::assume(short_position_raw > 0);
    let long_position = long_position_raw as i128 * POS_SCALE as i128;
    let short_position = -(short_position_raw as i128 * POS_SCALE as i128);
    let (mut header, mut markets, _) = one_market_view_fixture();
    markets[0].engine.pending_domain_loss_barrier_long = V16PodU64::new(1);
    let market = MarketGroupV16ViewMut::new(&mut header, &mut markets);

    let closes_long = market
        .kani_position_change_touches_pending_domain_loss_barrier(0, long_position, 0)
        .unwrap();
    let opens_long = market
        .kani_position_change_touches_pending_domain_loss_barrier(0, 0, long_position)
        .unwrap();
    let unrelated_short = market
        .kani_position_change_touches_pending_domain_loss_barrier(0, short_position, 0)
        .unwrap();

    kani::cover!(
        long_position_raw > 5,
        "pending-domain barrier proof covers wide long position"
    );
    kani::cover!(
        short_position_raw > 5,
        "pending-domain barrier proof covers wide unrelated short position"
    );
    assert!(closes_long);
    assert!(opens_long);
    assert!(!unrelated_short);
}

#[kani::proof]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn proof_v16_pending_domain_loss_barrier_allows_only_same_side_reductions() {
    let touches_barrier: bool = kani::any();
    let current: i128 = kani::any();
    let next: i128 = kani::any();
    kani::assume(current != i128::MIN);
    kani::assume(next != i128::MIN);
    kani::assume(current.unsigned_abs() <= MAX_POSITION_ABS_Q);
    kani::assume(next.unsigned_abs() <= MAX_POSITION_ABS_Q);

    let blocked =
        kani_pending_domain_loss_barrier_blocks_position_change(touches_barrier, current, next);
    let same_side_reduction_or_flat = current != 0
        && (next == 0 || current.signum() == next.signum())
        && next.unsigned_abs() < current.unsigned_abs();

    kani::cover!(
        touches_barrier && current == 0 && next != 0 && blocked,
        "pending-domain barrier blocks opening new touched risk"
    );
    kani::cover!(
        touches_barrier
            && current != 0
            && current.signum() == next.signum()
            && next.unsigned_abs() > current.unsigned_abs()
            && blocked,
        "pending-domain barrier blocks same-side risk increase"
    );
    kani::cover!(
        touches_barrier
            && current != 0
            && next != 0
            && current.signum() != next.signum()
            && blocked,
        "pending-domain barrier blocks side flips"
    );
    kani::cover!(
        touches_barrier && current != 0 && next == 0 && !blocked,
        "pending-domain barrier permits flat obligation"
    );
    kani::cover!(
        touches_barrier && same_side_reduction_or_flat && next != 0 && !blocked,
        "pending-domain barrier permits same-side risk reduction"
    );
    kani::cover!(
        !touches_barrier && current != next && !blocked,
        "pending-domain barrier ignores unrelated position changes"
    );
    assert_eq!(blocked, touches_barrier && !same_side_reduction_or_flat);
    if touches_barrier && same_side_reduction_or_flat {
        assert!(!blocked);
    }
    if touches_barrier && current == 0 && next != 0 {
        assert!(blocked);
    }
    if touches_barrier && current != 0 && next != 0 && current.signum() != next.signum() {
        assert!(blocked);
    }
}

#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn proof_v16_liquidation_projection_identifies_minimum_no_fee_close() {
    let abs_q_raw: u16 = kani::any();
    let equity_raw: u16 = kani::any();

    kani::assume(abs_q_raw >= 2);
    kani::assume(equity_raw >= 1 && equity_raw < abs_q_raw);
    let abs_q = abs_q_raw as u128;
    let equity = equity_raw as u128;

    let selected_new_maintenance = equity;
    let one_less_new_maintenance = equity + 1;
    let selected_deficit = kani_liquidation_projected_health_deficit_from_parts(
        equity as i128,
        abs_q,
        abs_q,
        selected_new_maintenance,
        0,
    )
    .unwrap();
    let one_less_deficit = kani_liquidation_projected_health_deficit_from_parts(
        equity as i128,
        abs_q,
        abs_q,
        one_less_new_maintenance,
        0,
    )
    .unwrap();

    kani::cover!(
        abs_q > equity + 8,
        "minimum no-fee liquidation projection covers nontrivial partial close"
    );
    assert_eq!(selected_deficit, 0);
    assert_eq!(one_less_deficit, 1);
}

#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn proof_v16_liquidation_projection_includes_fee_equity_debit() {
    let abs_q_raw: u16 = kani::any();
    let equity_raw: u16 = kani::any();
    let fee_raw: u16 = kani::any();

    kani::assume(abs_q_raw >= 3);
    kani::assume(equity_raw >= 2 && equity_raw < abs_q_raw);
    kani::assume(fee_raw > 0 && fee_raw < equity_raw);
    let abs_q = abs_q_raw as u128;
    let equity = equity_raw as u128;
    let fee = fee_raw as u128;
    let selected_new_maintenance = equity - fee;
    let one_less_new_maintenance = selected_new_maintenance + 1;

    let selected_deficit = kani_liquidation_projected_health_deficit_from_parts(
        equity as i128,
        abs_q,
        abs_q,
        selected_new_maintenance,
        fee,
    )
    .unwrap();
    let one_less_deficit = kani_liquidation_projected_health_deficit_from_parts(
        equity as i128,
        abs_q,
        abs_q,
        one_less_new_maintenance,
        fee,
    )
    .unwrap();

    kani::cover!(
        abs_q > equity + 8 && fee > 1,
        "minimum liquidation projection covers nontrivial fee debit"
    );
    assert_eq!(selected_deficit, 0);
    assert_eq!(one_less_deficit, 1);
}

#[kani::proof]
#[kani::unwind(10)]
#[kani::solver(cadical)]
fn proof_v16_liquidation_selector_is_healthy_locally_minimal_or_full_close() {
    let equity_raw: u8 = kani::any();
    let position_q = 1_000u128;
    let maintenance = (position_q / 10).max(80);
    let equity = equity_raw as u128;
    kani::assume(equity > 0 && equity < maintenance);

    let mut config = V16Config::public_user_fund(1, 0, 1);
    config.maintenance_margin_bps = 1_000;
    config.initial_margin_bps = 1_000;
    config.min_nonzero_mm_req = 80;
    config.min_nonzero_im_req = 81;
    config.liquidation_fee_bps = 800;
    config.liquidation_fee_cap = 1_000;
    config.max_price_move_bps_per_slot = 1;
    let cert = HealthCertV16 {
        certified_equity: equity as i128,
        certified_maintenance_req: maintenance,
        certified_liq_deficit: maintenance - equity,
        valid: true,
        ..HealthCertV16::default()
    };
    let leg = PortfolioLegV16 {
        active: true,
        side: SideV16::Long,
        basis_pos_q: position_q as i128,
        ..PortfolioLegV16::EMPTY
    };
    let price = POS_SCALE as u64;

    let selected = kani_liquidation_engine_close_request_q(
        config,
        cert,
        equity,
        0,
        leg,
        price,
        price,
        config.liquidation_fee_bps,
    )
    .unwrap();
    let partial_hi = kani_liquidation_partial_search_hi(config, position_q, price).unwrap();

    kani::cover!(
        selected < position_q && selected > 8,
        "selector covers a nontrivial health-restoring partial liquidation"
    );
    kani::cover!(
        selected == position_q && partial_hi > 0,
        "selector covers full close when no pre-floor partial restores health"
    );
    assert!((1..=position_q).contains(&selected));
    if selected < position_q {
        assert!(selected <= partial_hi);
        assert!(kani_liquidation_projected_healthy_after_close(
            config,
            cert,
            equity,
            0,
            leg,
            price,
            price,
            config.liquidation_fee_bps,
            selected,
        )
        .unwrap());
        if selected > 1 {
            assert!(!kani_liquidation_projected_healthy_after_close(
                config,
                cert,
                equity,
                0,
                leg,
                price,
                price,
                config.liquidation_fee_bps,
                selected - 1,
            )
            .unwrap());
        }
    } else {
        assert!(
            partial_hi == 0
                || !kani_liquidation_projected_healthy_after_close(
                    config,
                    cert,
                    equity,
                    0,
                    leg,
                    price,
                    price,
                    config.liquidation_fee_bps,
                    partial_hi,
                )
                .unwrap()
        );
    }
}

#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn proof_v16_liquidation_cannot_leave_uncovered_loss_with_other_open_risk() {
    let loss_abs: u128 = kani::any();
    let capital: u128 = kani::any();
    let leg_abs_q: u128 = kani::any();
    let close_q: u128 = kani::any();
    kani::assume((1..=MAX_VAULT_TVL).contains(&loss_abs));
    kani::assume(capital <= MAX_VAULT_TVL);
    kani::assume((1..=MAX_TRADE_SIZE_Q).contains(&leg_abs_q));
    kani::assume((1..=leg_abs_q).contains(&close_q));
    let loss = -(loss_abs as i128);
    let mut two_leg_bitmap = V16_EMPTY_ACTIVE_BITMAP;
    active_bitmap_set(&mut two_leg_bitmap, 0).unwrap();
    active_bitmap_set(&mut two_leg_bitmap, 1).unwrap();
    let mut single_leg_bitmap = V16_EMPTY_ACTIVE_BITMAP;
    active_bitmap_set(&mut single_leg_bitmap, 0).unwrap();

    let close_with_other_risk = kani_liquidation_close_would_leave_uncovered_loss_with_open_risk(
        loss,
        capital,
        two_leg_bitmap,
        0,
        close_q,
        leg_abs_q,
    )
    .unwrap();
    let full_close_without_other_risk =
        kani_liquidation_close_would_leave_uncovered_loss_with_open_risk(
            loss,
            capital,
            single_leg_bitmap,
            0,
            leg_abs_q,
            leg_abs_q,
        )
        .unwrap();
    let partial_close_without_other_risk =
        kani_liquidation_close_would_leave_uncovered_loss_with_open_risk(
            loss,
            capital,
            single_leg_bitmap,
            0,
            close_q,
            leg_abs_q,
        )
        .unwrap();
    let covered_loss_with_other_risk =
        kani_liquidation_close_would_leave_uncovered_loss_with_open_risk(
            loss,
            loss_abs,
            two_leg_bitmap,
            0,
            close_q,
            leg_abs_q,
        )
        .unwrap();
    let uncovered = loss_abs > capital;

    kani::cover!(
        uncovered && close_q == leg_abs_q && close_with_other_risk,
        "liquidation guard detects symbolic uncovered loss with remaining open risk"
    );
    kani::cover!(
        uncovered && close_q < leg_abs_q && partial_close_without_other_risk,
        "liquidation guard detects partial close preserving the only active leg"
    );
    assert_eq!(close_with_other_risk, uncovered);
    assert!(!full_close_without_other_risk);
    assert_eq!(
        partial_close_without_other_risk,
        uncovered && close_q < leg_abs_q
    );
    assert!(!covered_loss_with_other_risk);
}

#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn proof_v16_liquidation_fee_rejects_subminimum_partial_chunks() {
    let raw_fee_raw: u16 = kani::any();
    let min_liquidation_abs_raw: u16 = kani::any();
    let cap_extra: u16 = kani::any();

    kani::assume(min_liquidation_abs_raw > 0);
    let raw_fee = raw_fee_raw as u128;
    let min_liquidation_abs = min_liquidation_abs_raw as u128;
    let liquidation_fee_cap = min_liquidation_abs + cap_extra as u128;
    kani::assume(raw_fee < min_liquidation_abs);

    let result =
        kani_liquidation_fee_from_raw_fee(raw_fee, min_liquidation_abs, liquidation_fee_cap, false);

    kani::cover!(
        raw_fee > 0 && cap_extra > 0,
        "subminimum partial liquidation proof covers positive proportional fee and nontrivial cap"
    );
    assert_eq!(result, Err(V16Error::NonProgress));
}

#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn proof_v16_liquidation_fee_allows_subminimum_full_close() {
    let raw_fee_raw: u16 = kani::any();
    let min_liquidation_abs_raw: u16 = kani::any();
    let cap_extra: u16 = kani::any();

    kani::assume(min_liquidation_abs_raw > 0);
    let raw_fee = raw_fee_raw as u128;
    let min_liquidation_abs = min_liquidation_abs_raw as u128;
    let liquidation_fee_cap = min_liquidation_abs + cap_extra as u128;
    kani::assume(raw_fee < min_liquidation_abs);

    let fee =
        kani_liquidation_fee_from_raw_fee(raw_fee, min_liquidation_abs, liquidation_fee_cap, true)
            .unwrap();

    kani::cover!(
        raw_fee > 0 && cap_extra > 0,
        "subminimum full liquidation proof covers dust close with nontrivial fee cap"
    );
    assert_eq!(fee, min_liquidation_abs);
}

#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn proof_v16_liquidation_partial_fee_acceptance_implies_no_min_floor_extraction() {
    let raw_fee_raw: u16 = kani::any();
    let min_liquidation_abs_raw: u16 = kani::any();
    let cap_extra: u16 = kani::any();

    let raw_fee = raw_fee_raw as u128;
    let min_liquidation_abs = min_liquidation_abs_raw as u128;
    let liquidation_fee_cap = min_liquidation_abs + cap_extra as u128;
    let result =
        kani_liquidation_fee_from_raw_fee(raw_fee, min_liquidation_abs, liquidation_fee_cap, false);

    kani::cover!(
        min_liquidation_abs > 0 && raw_fee >= min_liquidation_abs && cap_extra > 0,
        "accepted partial liquidation proof covers nonzero minimum fee with proportional fee above floor"
    );
    if let Ok(fee) = result {
        assert!(min_liquidation_abs == 0 || raw_fee >= min_liquidation_abs);
        assert_eq!(fee, raw_fee.min(liquidation_fee_cap));
    }
}

#[kani::proof]
#[kani::unwind(32)]
#[kani::solver(cadical)]
fn proof_v16_trade_fee_helper_moves_capital_to_insurance_only() {
    let capital: u128 = kani::any();
    let insurance: u128 = kani::any();
    let requested_fee: u128 = kani::any();
    kani::assume(insurance <= u128::MAX - capital);
    let expected = capital.min(requested_fee);
    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    header.vault = V16PodU128::new(insurance + capital);
    header.c_tot = V16PodU128::new(capital);
    header.insurance = V16PodU128::new(insurance);
    account_header.capital = V16PodU128::new(capital);
    account_header.pnl = V16PodI128::new(0);
    let vault_before = header.vault.get();
    let pnl_before = account_header.pnl.get();
    let health_valid_before = account_header.health_cert.valid;
    let pnl_pos_tot_before = header.pnl_pos_tot.get();
    let pnl_pos_bound_tot_num_before = header.pnl_pos_bound_tot_num.get();
    let source_claim_bound_total_num_before = header.source_claim_bound_total_num.get();
    let negative_pnl_count_before = header.negative_pnl_account_count.get();
    let senior_before = header
        .c_tot
        .get()
        .checked_add(header.insurance.get())
        .unwrap();

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let residual_before = market.kani_residual();
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    let charged = market
        .kani_charge_account_fee_current_not_atomic(&mut account, requested_fee)
        .unwrap();

    kani::cover!(
        capital > 0 && requested_fee > capital,
        "trade fee helper covers capped fee collection"
    );
    kani::cover!(
        capital > 0 && requested_fee <= capital && requested_fee > 0,
        "trade fee helper covers full requested fee collection"
    );
    kani::cover!(
        requested_fee == 0 || capital == 0,
        "trade fee helper covers zero-charge no-op"
    );
    kani::cover!(
        expected > 0 && account.header.health_cert.valid == 0,
        "trade fee helper invalidates health certificate only when capital moves"
    );
    assert_eq!(charged, expected);
    assert_eq!(market.header.vault.get(), vault_before);
    assert_eq!(
        market
            .header
            .c_tot
            .get()
            .checked_add(market.header.insurance.get()),
        Some(senior_before)
    );
    assert_eq!(account.header.capital.get(), capital - expected);
    assert_eq!(market.header.c_tot.get(), capital - expected);
    assert_eq!(market.header.insurance.get(), insurance + expected);
    assert_eq!(account.header.pnl.get(), pnl_before);
    assert_eq!(market.header.pnl_pos_tot.get(), pnl_pos_tot_before);
    assert_eq!(
        market.header.pnl_pos_bound_tot_num.get(),
        pnl_pos_bound_tot_num_before
    );
    assert_eq!(
        market.header.source_claim_bound_total_num.get(),
        source_claim_bound_total_num_before
    );
    assert_eq!(
        market.header.negative_pnl_account_count.get(),
        negative_pnl_count_before
    );
    if expected == 0 {
        assert_eq!(account.header.health_cert.valid, health_valid_before);
    } else {
        assert_eq!(account.header.health_cert.valid, 0);
    }
    // Junior-pool isolation: this transition must not move the junior
    // residual pool (haircut/payout funding).
    assert_eq!(market.kani_residual(), residual_before);
}

#[kani::proof]
#[kani::unwind(32)]
#[kani::solver(cadical)]
fn proof_v16_trade_fee_helper_does_not_charge_negative_pnl_account() {
    let capital: u128 = kani::any();
    let insurance: u128 = kani::any();
    let loss_raw: u8 = kani::any();
    let requested_fee: u128 = kani::any();
    kani::assume(insurance <= u128::MAX - capital);
    kani::assume(loss_raw > 0);
    let loss = loss_raw as i128;
    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    header.vault = V16PodU128::new(insurance + capital);
    header.c_tot = V16PodU128::new(capital);
    header.insurance = V16PodU128::new(insurance);
    account_header.capital = V16PodU128::new(capital);
    account_header.pnl = V16PodI128::new(-loss);
    let vault_before = header.vault;
    let c_tot_before = header.c_tot;
    let insurance_before = header.insurance;
    let pnl_pos_tot_before = header.pnl_pos_tot;
    let pnl_pos_bound_tot_before = header.pnl_pos_bound_tot;
    let pnl_pos_bound_tot_num_before = header.pnl_pos_bound_tot_num;
    let source_claim_bound_total_num_before = header.source_claim_bound_total_num;
    let negative_pnl_count_before = header.negative_pnl_account_count;
    let capital_before = account_header.capital;
    let health_cert_before = account_header.health_cert;
    let fee_credits_before = account_header.fee_credits;
    let source_domains_before = account_header.source_domains;

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    let charged = market
        .kani_charge_account_fee_current_not_atomic(&mut account, requested_fee)
        .unwrap();

    kani::cover!(
        requested_fee > 0,
        "negative-PnL account reaches no-fee guard with requested fee"
    );
    kani::cover!(
        requested_fee == 0,
        "negative-PnL account reaches zero-fee no-op guard"
    );
    assert_eq!(charged, 0);
    assert_eq!(market.header.vault, vault_before);
    assert_eq!(market.header.c_tot, c_tot_before);
    assert_eq!(market.header.insurance, insurance_before);
    assert_eq!(market.header.pnl_pos_tot, pnl_pos_tot_before);
    assert_eq!(market.header.pnl_pos_bound_tot, pnl_pos_bound_tot_before);
    assert_eq!(
        market.header.pnl_pos_bound_tot_num,
        pnl_pos_bound_tot_num_before
    );
    assert_eq!(
        market.header.source_claim_bound_total_num,
        source_claim_bound_total_num_before
    );
    assert_eq!(
        market.header.negative_pnl_account_count,
        negative_pnl_count_before
    );
    assert_eq!(account.header.capital, capital_before);
    assert_eq!(account.header.pnl.get(), -loss);
    assert_eq!(account.header.health_cert, health_cert_before);
    assert_eq!(account.header.fee_credits, fee_credits_before);
    assert_eq!(account.header.source_domains, source_domains_before);
}

#[kani::proof]
#[kani::unwind(64)]
#[kani::solver(cadical)]
fn proof_v16_fee_core_moves_current_capital_to_insurance_only() {
    let fee_raw: u8 = kani::any();
    kani::assume((1..=7).contains(&fee_raw));
    let requested_fee = fee_raw as u128;
    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    header.vault = V16PodU128::new(7);
    header.c_tot = V16PodU128::new(7);
    account_header.capital = V16PodU128::new(7);
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let residual_before = market.kani_residual();
    let mut account = PortfolioV16ViewMut::new(&mut account_header);

    let charged = market
        .kani_charge_account_fee_current_not_atomic(&mut account, requested_fee)
        .unwrap();

    kani::cover!(requested_fee > 1, "fee core covers nontrivial amount");
    assert_eq!(charged, requested_fee);
    assert_eq!(account.header.capital.get(), 7 - requested_fee);
    assert_eq!(market.header.c_tot.get(), 7 - requested_fee);
    assert_eq!(market.header.insurance.get(), requested_fee);
    assert_eq!(market.header.vault.get(), 7);
    // Junior-pool isolation: this transition must not move the junior
    // residual pool (haircut/payout funding).
    assert_eq!(market.kani_residual(), residual_before);
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_negative_pnl_settlement_consumes_principal_before_residual() {
    let capital: u128 = kani::any();
    let loss: u128 = kani::any();
    kani::assume(capital <= MAX_VAULT_TVL);
    kani::assume((1..=MAX_VAULT_TVL).contains(&loss));
    let paid_expected = capital.min(loss);
    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    header.vault = V16PodU128::new(capital);
    header.c_tot = V16PodU128::new(capital);
    header.negative_pnl_account_count = V16PodU64::new(1);
    account_header.capital = V16PodU128::new(capital);
    account_header.pnl = V16PodI128::new(-(loss as i128));
    let vault_before = header.vault.get();

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    let paid = market
        .kani_settle_negative_pnl_from_principal_core_not_atomic(&mut account)
        .unwrap();

    kani::cover!(
        capital > 0 && capital < loss,
        "principal settlement covers residual bankruptcy branch"
    );
    kani::cover!(
        capital >= loss,
        "principal settlement covers fully paid realized loss"
    );
    assert_eq!(paid, paid_expected);
    assert_eq!(market.header.vault.get(), vault_before);
    assert_eq!(market.header.c_tot.get(), capital - paid_expected);
    assert_eq!(account.header.capital.get(), capital - paid_expected);
    assert_eq!(
        account.header.pnl.get(),
        -(loss as i128) + paid_expected as i128
    );
    if paid_expected < loss {
        assert_eq!(market.header.bankruptcy_hlock_active, 1);
        assert_eq!(market.header.negative_pnl_account_count.get(), 1);
    } else {
        assert_eq!(market.header.negative_pnl_account_count.get(), 0);
    }
}

#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn proof_v16_backing_domain_fee_split_for_lien_delta_is_exact_and_conservative() {
    let atoms_raw: u8 = kani::any();
    let fee_raw: u16 = kani::any();
    let share_raw: u16 = kani::any();
    let unaligned: bool = kani::any();
    let invalid_fee: bool = kani::any();
    let invalid_share: bool = kani::any();
    kani::assume(atoms_raw <= 64);
    kani::assume(fee_raw <= MAX_MARGIN_BPS as u16);
    kani::assume(share_raw <= MAX_MARGIN_BPS as u16);
    let atoms = atoms_raw as u128;
    let lien_delta_num = atoms * BOUND_SCALE + u128::from(unaligned);
    let fee_bps = if invalid_fee {
        MAX_MARGIN_BPS as u16 + 1
    } else {
        fee_raw
    };
    let share_bps = if invalid_share {
        MAX_MARGIN_BPS as u16 + 1
    } else {
        share_raw
    };

    let result = backing_domain_fee_split_for_lien_delta_num(lien_delta_num, fee_bps, share_bps);
    let expected_ok = !unaligned && !invalid_fee && !invalid_share;

    kani::cover!(
        expected_ok
            && atoms > 1
            && fee_bps > 0
            && share_bps > 0
            && share_bps < MAX_MARGIN_BPS as u16,
        "backing domain fee split covers mixed provider/insurance routing"
    );
    kani::cover!(
        expected_ok && atoms > 0 && fee_bps == MAX_MARGIN_BPS as u16,
        "backing domain fee split covers full-fee cap"
    );
    kani::cover!(
        expected_ok && atoms > 0 && fee_bps > 0 && share_bps == 0,
        "backing domain fee split covers provider-only routing"
    );
    kani::cover!(
        expected_ok && atoms > 0 && fee_bps > 0 && share_bps == MAX_MARGIN_BPS as u16,
        "backing domain fee split covers insurance-only routing"
    );
    kani::cover!(
        unaligned,
        "backing domain fee split rejects unaligned lien delta"
    );
    kani::cover!(
        invalid_fee || invalid_share,
        "backing domain fee split rejects invalid bps"
    );
    assert_eq!(result.is_ok(), expected_ok);
    if let Ok(split) = result {
        let expected_fee = if atoms == 0 || fee_bps == 0 {
            0
        } else {
            let num = atoms * fee_bps as u128;
            num / MAX_MARGIN_BPS as u128 + u128::from(num % MAX_MARGIN_BPS as u128 != 0)
        };
        let expected_insurance = expected_fee * share_bps as u128 / MAX_MARGIN_BPS as u128;
        assert_eq!(split.lien_delta_atoms, atoms);
        assert_eq!(split.total_fee, expected_fee);
        assert_eq!(split.insurance_fee, expected_insurance);
        assert_eq!(split.provider_fee + split.insurance_fee, split.total_fee);
        assert!(split.total_fee <= atoms);
    } else {
        assert!(unaligned || invalid_fee || invalid_share);
    }
}

#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn proof_v16_backing_utilization_fee_never_charges_negative_pnl_account() {
    let capital: u128 = kani::any();
    let fee: u128 = kani::any();
    let earnings: u128 = kani::any();
    let loss_raw: u8 = kani::any();
    kani::assume(loss_raw > 0);
    let loss = loss_raw as i128;
    let group_c_tot = capital;

    let (charged, next_capital, next_c_tot, next_earnings) =
        kani_apply_backing_utilization_fee_charge(capital, group_c_tot, earnings, -loss, fee)
            .unwrap();

    kani::cover!(
        fee > 0 && capital > 0,
        "negative-PnL backing utilization fee reaches no-charge guard"
    );
    assert_eq!(charged, 0);
    assert_eq!(next_capital, capital);
    assert_eq!(next_c_tot, group_c_tot);
    assert_eq!(next_earnings, earnings);
}

#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn proof_v16_backing_utilization_fee_is_capped_by_capital_and_conserves_ctot_to_earnings() {
    let capital: u128 = kani::any();
    let fee: u128 = kani::any();
    let earnings: u128 = kani::any();
    let group_c_tot = capital;
    let expected = capital.min(fee);
    let expected_earnings = earnings.checked_add(expected);

    let result = kani_apply_backing_utilization_fee_charge(capital, group_c_tot, earnings, 0, fee);

    kani::cover!(
        fee > capital && capital > 0,
        "backing utilization fee covers capital-capped collection"
    );
    kani::cover!(
        fee <= capital && fee > 0,
        "backing utilization fee covers full requested collection"
    );
    kani::cover!(
        expected > 0 && expected_earnings.is_none(),
        "backing utilization fee rejects bucket earnings overflow"
    );
    assert_eq!(result.is_ok(), expected_earnings.is_some());
    if let Ok((charged, next_capital, next_c_tot, next_earnings)) = result {
        assert_eq!(charged, expected);
        assert_eq!(next_capital, capital - expected);
        assert_eq!(next_c_tot, group_c_tot - expected);
        assert_eq!(next_earnings, expected_earnings.unwrap());
        assert_eq!(
            next_c_tot.checked_add(next_earnings),
            group_c_tot.checked_add(earnings)
        );
    } else {
        assert_eq!(result, Err(V16Error::CounterOverflow));
    }
}

fn symbolic_backing_fee_config(
    kink_raw: u16,
    base_raw: u8,
    slope_at_raw: u8,
    slope_above_raw: u8,
) -> V16Config {
    let mut config = V16Config::public_user_fund_with_market_slots(1, 1, 0, 10);
    config.backing_fee_kink_util_bps =
        kink_raw.max(1).min((MAX_BACKING_FEE_UTIL_BPS - 1) as u16) as u64;
    config.backing_fee_base_rate_e9_per_slot = base_raw as u64;
    config.backing_fee_slope_at_kink_e9_per_slot = slope_at_raw as u64;
    config.backing_fee_slope_above_kink_e9_per_slot = slope_above_raw as u64;
    config
}

#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn proof_v16_backing_utilization_rate_zero_and_invalid_source_branches_are_exact() {
    let fresh_raw: u8 = kani::any();
    let valid_raw: u8 = kani::any();
    let kink_raw: u16 = kani::any();
    let base_raw: u8 = kani::any();
    let slope_at_raw: u8 = kani::any();
    let slope_above_raw: u8 = kani::any();
    kani::assume(fresh_raw <= 32);
    kani::assume(valid_raw <= 48);
    kani::assume(kink_raw <= MAX_BACKING_FEE_UTIL_BPS as u16);
    let fresh = fresh_raw as u128;
    let valid = valid_raw as u128;
    kani::assume(valid == 0 || fresh == 0 || valid > fresh);
    let config = symbolic_backing_fee_config(kink_raw, base_raw, slope_at_raw, slope_above_raw);
    let source = SourceCreditStateV16 {
        fresh_reserved_backing_num: fresh * BOUND_SCALE,
        valid_liened_backing_num: valid * BOUND_SCALE,
        ..SourceCreditStateV16::EMPTY
    };

    let result = kani_backing_utilization_rate_e9_for_source_state(config, source);
    let expected = if valid == 0 {
        Ok(0)
    } else if fresh == 0 || valid > fresh {
        Err(V16Error::InvalidConfig)
    } else {
        unreachable!()
    };

    kani::cover!(
        valid == 0 && fresh > 0,
        "backing utilization rate covers zero-lien no-charge branch"
    );
    kani::cover!(
        valid > 0 && fresh == 0,
        "backing utilization rate rejects liened source with zero backing"
    );
    kani::cover!(
        valid > fresh && fresh > 0,
        "backing utilization rate rejects over-liened source state"
    );
    assert_eq!(result, expected);
}

#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn proof_v16_backing_utilization_rate_below_kink_matches_exact_schedule() {
    let fresh_raw: u8 = kani::any();
    let valid_raw: u8 = kani::any();
    let base_raw: u8 = kani::any();
    let slope_at_raw: u8 = kani::any();
    let slope_above_raw: u8 = kani::any();
    kani::assume((1..=32).contains(&fresh_raw));
    kani::assume((1..=32).contains(&valid_raw));
    kani::assume(valid_raw <= fresh_raw);
    let fresh = fresh_raw as u128;
    let valid = valid_raw as u128;
    let config = symbolic_backing_fee_config(8_000, base_raw, slope_at_raw, slope_above_raw);
    let kink = config.backing_fee_kink_util_bps;
    let util_bps = (valid * MAX_BACKING_FEE_UTIL_BPS as u128 / fresh) as u64;
    kani::assume(util_bps <= kink);
    let source = SourceCreditStateV16 {
        fresh_reserved_backing_num: fresh * BOUND_SCALE,
        valid_liened_backing_num: valid * BOUND_SCALE,
        ..SourceCreditStateV16::EMPTY
    };

    let rate = kani_backing_utilization_rate_e9_for_source_state(config, source).unwrap();
    let expected = config.backing_fee_base_rate_e9_per_slot
        + (config.backing_fee_slope_at_kink_e9_per_slot * util_bps / kink);

    kani::cover!(
        util_bps < kink && valid < fresh,
        "backing utilization below-kink proof covers strict below-kink utilization"
    );
    kani::cover!(
        util_bps == kink,
        "backing utilization below-kink proof covers exact kink boundary"
    );
    assert_eq!(rate, expected);
    assert!(rate >= config.backing_fee_base_rate_e9_per_slot);
    assert!(
        rate <= config.backing_fee_base_rate_e9_per_slot
            + config.backing_fee_slope_at_kink_e9_per_slot
    );
    assert!(rate <= MAX_BACKING_FEE_RATE_E9_PER_SLOT);
}

#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn proof_v16_backing_utilization_rate_above_kink_matches_exact_schedule() {
    let fresh_raw: u8 = kani::any();
    let valid_raw: u8 = kani::any();
    let base_raw: u8 = kani::any();
    let slope_at_raw: u8 = kani::any();
    let slope_above_raw: u8 = kani::any();
    kani::assume((1..=32).contains(&fresh_raw));
    kani::assume((1..=32).contains(&valid_raw));
    kani::assume(valid_raw <= fresh_raw);
    let fresh = fresh_raw as u128;
    let valid = valid_raw as u128;
    let config = symbolic_backing_fee_config(8_000, base_raw, slope_at_raw, slope_above_raw);
    let kink = config.backing_fee_kink_util_bps;
    let util_bps = (valid * MAX_BACKING_FEE_UTIL_BPS as u128 / fresh) as u64;
    kani::assume(util_bps > kink);
    let source = SourceCreditStateV16 {
        fresh_reserved_backing_num: fresh * BOUND_SCALE,
        valid_liened_backing_num: valid * BOUND_SCALE,
        ..SourceCreditStateV16::EMPTY
    };

    let rate = kani_backing_utilization_rate_e9_for_source_state(config, source).unwrap();
    let expected = config.backing_fee_base_rate_e9_per_slot
        + config.backing_fee_slope_at_kink_e9_per_slot
        + (config.backing_fee_slope_above_kink_e9_per_slot * (util_bps - kink)
            / (MAX_BACKING_FEE_UTIL_BPS - kink));

    kani::cover!(
        util_bps > kink && valid < fresh,
        "backing utilization above-kink proof covers partial above-kink utilization"
    );
    kani::cover!(
        util_bps == MAX_BACKING_FEE_UTIL_BPS,
        "backing utilization above-kink proof covers full utilization cap"
    );
    assert_eq!(rate, expected);
    assert!(
        rate >= config.backing_fee_base_rate_e9_per_slot
            + config.backing_fee_slope_at_kink_e9_per_slot
    );
    assert!(
        rate <= config.backing_fee_base_rate_e9_per_slot
            + config.backing_fee_slope_at_kink_e9_per_slot
            + config.backing_fee_slope_above_kink_e9_per_slot
    );
    assert!(rate <= MAX_BACKING_FEE_RATE_E9_PER_SLOT);
}

#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn proof_v16_backing_utilization_fee_quote_atoms_is_exact_floor_and_time_bounded() {
    let fresh_raw: u8 = kani::any();
    let valid_raw: u8 = kani::any();
    let lien_raw: u8 = kani::any();
    let dt_raw: u8 = kani::any();
    let kink_raw: u16 = kani::any();
    let base_raw: u8 = kani::any();
    let slope_at_raw: u8 = kani::any();
    let slope_above_raw: u8 = kani::any();
    kani::assume(fresh_raw <= 64);
    kani::assume(valid_raw <= 80);
    kani::assume(lien_raw <= 64);
    kani::assume(dt_raw <= 16);
    kani::assume(kink_raw <= MAX_BACKING_FEE_UTIL_BPS as u16);
    let fresh = fresh_raw as u128;
    let valid = valid_raw as u128;
    let lien_atoms = lien_raw as u128;
    let dt = dt_raw as u64;
    let config = symbolic_backing_fee_config(kink_raw, base_raw, slope_at_raw, slope_above_raw);
    let source = SourceCreditStateV16 {
        fresh_reserved_backing_num: fresh * BOUND_SCALE,
        valid_liened_backing_num: valid * BOUND_SCALE,
        ..SourceCreditStateV16::EMPTY
    };
    let lien_backing_num = lien_atoms * BOUND_SCALE;
    let from_slot = 10u64;
    let to_slot = from_slot + dt;

    let rate = kani_backing_utilization_rate_e9_for_source_state(config, source);
    let result = kani_backing_utilization_fee_quote_atoms_for_lien(
        config,
        source,
        lien_backing_num,
        from_slot,
        to_slot,
    );
    let expected_ok = lien_atoms == 0 || dt == 0 || rate.is_ok();

    kani::cover!(
        result == Ok(0) && lien_atoms == 0 && dt > 0,
        "backing utilization fee covers zero-lien no-op"
    );
    kani::cover!(
        result == Ok(0) && lien_atoms > 0 && dt == 0,
        "backing utilization fee covers zero-time no-op"
    );
    kani::cover!(
        expected_ok && lien_atoms > 0 && dt > 1 && rate.unwrap_or(0) > 0,
        "backing utilization fee covers nontrivial positive fee path"
    );
    kani::cover!(
        !expected_ok && valid > fresh && valid > 0 && lien_atoms > 0 && dt > 0,
        "backing utilization fee rejects invalid source state"
    );
    assert_eq!(result.is_ok(), expected_ok);
    if let Ok(fee) = result {
        if lien_atoms == 0 || dt == 0 || rate.unwrap() == 0 {
            assert_eq!(fee, 0);
        } else {
            let expected =
                lien_atoms * rate.unwrap() as u128 * dt as u128 / BACKING_FEE_RATE_DEN_E9;
            assert_eq!(fee, expected);
            assert!(fee <= lien_atoms * dt as u128);
        }
    } else {
        assert!(rate.is_err() && lien_atoms > 0 && dt > 0);
    }
}

#[kani::proof]
#[kani::unwind(32)]
#[kani::solver(cadical)]
fn proof_v16_backing_utilization_collection_first_touch_initializes_cursor_without_value_move() {
    let capital_raw: u8 = kani::any();
    let lien_raw: u8 = kani::any();
    let current_slot_raw: u8 = kani::any();
    kani::assume(lien_raw > 0);
    kani::assume(current_slot_raw > 0);
    let capital = capital_raw as u128;
    let lien_num = lien_raw as u128 * BOUND_SCALE;
    let current_slot = current_slot_raw as u64;
    let (mut header, mut markets, mut account_header) = one_market_direct_view_fixture();
    let market_id = markets[0].engine.asset.market_id.get();
    header.current_slot = V16PodU64::new(current_slot);
    header.slot_last = V16PodU64::new(current_slot);
    header.vault = V16PodU128::new(capital);
    header.c_tot = V16PodU128::new(capital);
    account_header.capital = V16PodU128::new(capital);
    account_header.health_cert.valid = 1;
    account_header.source_domains[0] = PortfolioSourceDomainV16Account {
        domain: V16PodU32::new(0),
        source_claim_market_id: V16PodU64::new(market_id),
        source_lien_counterparty_backing_num: V16PodU128::new(lien_num),
        source_lien_fee_last_slot: V16PodU64::new(0),
        ..PortfolioSourceDomainV16Account::default()
    };

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let vault_before = market.header.vault.get();
    let c_tot_before = market.header.c_tot.get();
    let insurance_before = market.header.insurance.get();
    let earnings_before = market.header.backing_provider_earnings_total.get();
    let mut account = PortfolioV16ViewMut {
        header: &mut account_header,
    };
    let capital_before = account.header.capital.get();
    let charged = market
        .kani_collect_account_backing_utilization_fee_for_domain_not_atomic(&mut account, 0)
        .unwrap();

    kani::cover!(
        capital > 0 && lien_raw > 1 && current_slot > 1,
        "first backing-utilization collection covers nontrivial lien cursor initialization"
    );
    assert_eq!(charged, 0);
    assert_eq!(
        account.header.source_domains[0]
            .source_lien_fee_last_slot
            .get(),
        current_slot
    );
    assert_eq!(account.header.capital.get(), capital_before);
    assert_eq!(market.header.c_tot.get(), c_tot_before);
    assert_eq!(market.header.vault.get(), vault_before);
    assert_eq!(market.header.insurance.get(), insurance_before);
    assert_eq!(
        market.header.backing_provider_earnings_total.get(),
        earnings_before
    );
    assert_eq!(
        account.header.source_domains[0]
            .source_lien_capital_at_risk_fee_revenue
            .get(),
        0
    );
    assert_eq!(account.header.health_cert.valid, 1);
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_backing_utilization_collection_full_charge_conserves_senior_value() {
    let slack_raw: u8 = kani::any();
    let earnings_raw: u8 = kani::any();
    let revenue_raw: u8 = kani::any();
    kani::assume(slack_raw <= 4);
    kani::assume(earnings_raw <= 4);
    kani::assume(revenue_raw <= 4);
    let lien_atoms = 1u128;
    let lien_num = lien_atoms * BOUND_SCALE;
    let dt = 1u64;
    let earnings_before = earnings_raw as u128;
    let revenue_before = revenue_raw as u128;
    let last_slot = 3u64;
    let current_slot = last_slot + dt;
    let requested_fee = lien_atoms * dt as u128;
    let capital = requested_fee + slack_raw as u128;
    let expected_charged = requested_fee;
    let (mut header, mut markets, mut account_header) = one_market_direct_view_fixture();
    let market_id = markets[0].engine.asset.market_id.get();
    header.config.backing_fee_base_rate_e9_per_slot =
        V16PodU64::new(MAX_BACKING_FEE_RATE_E9_PER_SLOT);
    header.config.backing_fee_slope_at_kink_e9_per_slot = V16PodU64::new(0);
    header.config.backing_fee_slope_above_kink_e9_per_slot = V16PodU64::new(0);
    header.current_slot = V16PodU64::new(current_slot);
    header.slot_last = V16PodU64::new(current_slot);
    header.vault = V16PodU128::new(capital + earnings_before + lien_atoms);
    header.c_tot = V16PodU128::new(capital);
    header.backing_provider_earnings_total = V16PodU128::new(earnings_before);
    header.source_fresh_backing_total_num = V16PodU128::new(lien_num);
    account_header.capital = V16PodU128::new(capital);
    account_header.pnl = V16PodI128::new(0);
    account_header.health_cert.valid = 1;
    account_header.source_domains[0] = PortfolioSourceDomainV16Account {
        domain: V16PodU32::new(0),
        source_claim_market_id: V16PodU64::new(market_id),
        source_lien_counterparty_backing_num: V16PodU128::new(lien_num),
        source_lien_fee_last_slot: V16PodU64::new(last_slot),
        source_lien_capital_at_risk_fee_revenue: V16PodU128::new(revenue_before),
        ..PortfolioSourceDomainV16Account::default()
    };
    markets[0].engine.source_credit_long =
        SourceCreditStateV16Account::from_runtime(&SourceCreditStateV16 {
            fresh_reserved_backing_num: lien_num,
            valid_liened_backing_num: lien_num,
            credit_rate_num: CREDIT_RATE_SCALE,
            ..SourceCreditStateV16::EMPTY
        });
    markets[0].engine.backing_long = BackingBucketV16Account::from_runtime(&BackingBucketV16 {
        market_id,
        valid_liened_backing_num: lien_num,
        utilization_fee_earnings: earnings_before,
        expiry_slot: current_slot + 1,
        status: BackingBucketStatusV16::Fresh,
        ..BackingBucketV16::EMPTY
    });

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let vault_before = market.header.vault.get();
    let insurance_before = market.header.insurance.get();
    let mut account = PortfolioV16ViewMut {
        header: &mut account_header,
    };
    let charged = market
        .kani_collect_account_backing_utilization_fee_for_domain_not_atomic(&mut account, 0)
        .unwrap();
    let bucket_after = market.kani_backing_bucket_for_domain(0).unwrap();

    kani::cover!(
        slack_raw > 0,
        "backing-utilization collection covers nontrivial full positive-capital fee charge"
    );
    assert_eq!(charged, expected_charged);
    assert_eq!(
        account.header.source_domains[0]
            .source_lien_fee_last_slot
            .get(),
        current_slot
    );
    assert_eq!(account.header.capital.get(), capital - expected_charged);
    assert_eq!(market.header.c_tot.get(), capital - expected_charged);
    assert_eq!(
        bucket_after.utilization_fee_earnings,
        earnings_before + expected_charged
    );
    assert_eq!(
        market.header.backing_provider_earnings_total.get(),
        earnings_before + expected_charged
    );
    assert_eq!(
        account.header.source_domains[0]
            .source_lien_capital_at_risk_fee_revenue
            .get(),
        revenue_before + expected_charged
    );
    assert_eq!(market.header.vault.get(), vault_before);
    assert_eq!(market.header.insurance.get(), insurance_before);
    assert_eq!(
        market.header.vault.get(),
        market.header.c_tot.get()
            + market.header.insurance.get()
            + market.header.backing_provider_earnings_total.get()
            + market.header.source_fresh_backing_total_num.get() / BOUND_SCALE
    );
    assert_eq!(
        account.header.health_cert.valid,
        if expected_charged > 0 { 0 } else { 1 }
    );
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_backing_utilization_collection_cap_charge_conserves_senior_value() {
    let earnings_raw: u8 = kani::any();
    let revenue_raw: u8 = kani::any();
    kani::assume(earnings_raw <= 4);
    kani::assume(revenue_raw <= 4);
    let lien_atoms = 2u128;
    let lien_num = lien_atoms * BOUND_SCALE;
    let dt = 1u64;
    let expected_charged = 1u128;
    let capital = expected_charged;
    let earnings_before = earnings_raw as u128;
    let revenue_before = revenue_raw as u128;
    let last_slot = 3u64;
    let current_slot = last_slot + dt;
    let (mut header, mut markets, mut account_header) = one_market_direct_view_fixture();
    let market_id = markets[0].engine.asset.market_id.get();
    header.config.backing_fee_base_rate_e9_per_slot =
        V16PodU64::new(MAX_BACKING_FEE_RATE_E9_PER_SLOT);
    header.config.backing_fee_slope_at_kink_e9_per_slot = V16PodU64::new(0);
    header.config.backing_fee_slope_above_kink_e9_per_slot = V16PodU64::new(0);
    header.current_slot = V16PodU64::new(current_slot);
    header.slot_last = V16PodU64::new(current_slot);
    header.vault = V16PodU128::new(capital + earnings_before + lien_atoms);
    header.c_tot = V16PodU128::new(capital);
    header.backing_provider_earnings_total = V16PodU128::new(earnings_before);
    header.source_fresh_backing_total_num = V16PodU128::new(lien_num);
    account_header.capital = V16PodU128::new(capital);
    account_header.pnl = V16PodI128::new(0);
    account_header.health_cert.valid = 1;
    account_header.source_domains[0] = PortfolioSourceDomainV16Account {
        domain: V16PodU32::new(0),
        source_claim_market_id: V16PodU64::new(market_id),
        source_lien_counterparty_backing_num: V16PodU128::new(lien_num),
        source_lien_fee_last_slot: V16PodU64::new(last_slot),
        source_lien_capital_at_risk_fee_revenue: V16PodU128::new(revenue_before),
        ..PortfolioSourceDomainV16Account::default()
    };
    markets[0].engine.source_credit_long =
        SourceCreditStateV16Account::from_runtime(&SourceCreditStateV16 {
            fresh_reserved_backing_num: lien_num,
            valid_liened_backing_num: lien_num,
            credit_rate_num: CREDIT_RATE_SCALE,
            ..SourceCreditStateV16::EMPTY
        });
    markets[0].engine.backing_long = BackingBucketV16Account::from_runtime(&BackingBucketV16 {
        market_id,
        valid_liened_backing_num: lien_num,
        utilization_fee_earnings: earnings_before,
        expiry_slot: current_slot + 1,
        status: BackingBucketStatusV16::Fresh,
        ..BackingBucketV16::EMPTY
    });

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let vault_before = market.header.vault.get();
    let insurance_before = market.header.insurance.get();
    let mut account = PortfolioV16ViewMut {
        header: &mut account_header,
    };
    let charged = market
        .kani_collect_account_backing_utilization_fee_for_domain_not_atomic(&mut account, 0)
        .unwrap();
    let bucket_after = market.kani_backing_bucket_for_domain(0).unwrap();

    kani::cover!(
        earnings_raw > 0 || revenue_raw > 0,
        "backing-utilization collection covers nontrivial capital-capped fee charge"
    );
    assert_eq!(charged, expected_charged);
    assert_eq!(
        account.header.source_domains[0]
            .source_lien_fee_last_slot
            .get(),
        current_slot
    );
    assert_eq!(account.header.capital.get(), 0);
    assert_eq!(market.header.c_tot.get(), 0);
    assert_eq!(
        bucket_after.utilization_fee_earnings,
        earnings_before + expected_charged
    );
    assert_eq!(
        market.header.backing_provider_earnings_total.get(),
        earnings_before + expected_charged
    );
    assert_eq!(
        account.header.source_domains[0]
            .source_lien_capital_at_risk_fee_revenue
            .get(),
        revenue_before + expected_charged
    );
    assert_eq!(market.header.vault.get(), vault_before);
    assert_eq!(market.header.insurance.get(), insurance_before);
    assert_eq!(
        market.header.vault.get(),
        market.header.c_tot.get()
            + market.header.insurance.get()
            + market.header.backing_provider_earnings_total.get()
            + market.header.source_fresh_backing_total_num.get() / BOUND_SCALE
    );
    assert_eq!(account.header.health_cert.valid, 0);
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_backing_utilization_collection_negative_pnl_never_draws_capital() {
    let capital_raw: u8 = kani::any();
    let earnings_raw: u8 = kani::any();
    let revenue_raw: u8 = kani::any();
    kani::assume(capital_raw <= 16);
    kani::assume(earnings_raw <= 16);
    kani::assume(revenue_raw <= 16);
    let capital = capital_raw as u128;
    let earnings_before = earnings_raw as u128;
    let revenue_before = revenue_raw as u128;
    let lien_num = BOUND_SCALE;
    let last_slot = 3u64;
    let current_slot = last_slot + 1;
    let (mut header, mut markets, mut account_header) = one_market_direct_view_fixture();
    let market_id = markets[0].engine.asset.market_id.get();
    header.config.backing_fee_base_rate_e9_per_slot =
        V16PodU64::new(MAX_BACKING_FEE_RATE_E9_PER_SLOT);
    header.config.backing_fee_slope_at_kink_e9_per_slot = V16PodU64::new(0);
    header.config.backing_fee_slope_above_kink_e9_per_slot = V16PodU64::new(0);
    header.current_slot = V16PodU64::new(current_slot);
    header.slot_last = V16PodU64::new(current_slot);
    header.vault = V16PodU128::new(capital + earnings_before + 1);
    header.c_tot = V16PodU128::new(capital);
    header.backing_provider_earnings_total = V16PodU128::new(earnings_before);
    header.source_fresh_backing_total_num = V16PodU128::new(lien_num);
    account_header.capital = V16PodU128::new(capital);
    account_header.pnl = V16PodI128::new(-1);
    account_header.health_cert.valid = 1;
    account_header.source_domains[0] = PortfolioSourceDomainV16Account {
        domain: V16PodU32::new(0),
        source_claim_market_id: V16PodU64::new(market_id),
        source_lien_counterparty_backing_num: V16PodU128::new(lien_num),
        source_lien_fee_last_slot: V16PodU64::new(last_slot),
        source_lien_capital_at_risk_fee_revenue: V16PodU128::new(revenue_before),
        ..PortfolioSourceDomainV16Account::default()
    };
    markets[0].engine.source_credit_long =
        SourceCreditStateV16Account::from_runtime(&SourceCreditStateV16 {
            fresh_reserved_backing_num: lien_num,
            valid_liened_backing_num: lien_num,
            credit_rate_num: CREDIT_RATE_SCALE,
            ..SourceCreditStateV16::EMPTY
        });
    markets[0].engine.backing_long = BackingBucketV16Account::from_runtime(&BackingBucketV16 {
        market_id,
        valid_liened_backing_num: lien_num,
        utilization_fee_earnings: earnings_before,
        expiry_slot: current_slot + 1,
        status: BackingBucketStatusV16::Fresh,
        ..BackingBucketV16::EMPTY
    });

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let vault_before = market.header.vault.get();
    let insurance_before = market.header.insurance.get();
    let mut account = PortfolioV16ViewMut {
        header: &mut account_header,
    };
    let charged = market
        .kani_collect_account_backing_utilization_fee_for_domain_not_atomic(&mut account, 0)
        .unwrap();
    let bucket_after = market.kani_backing_bucket_for_domain(0).unwrap();

    kani::cover!(
        capital > 0,
        "negative-PnL backing-utilization collection covers loss-bearing capital guard"
    );
    assert_eq!(charged, 0);
    assert_eq!(
        account.header.source_domains[0]
            .source_lien_fee_last_slot
            .get(),
        current_slot
    );
    assert_eq!(account.header.capital.get(), capital);
    assert_eq!(market.header.c_tot.get(), capital);
    assert_eq!(bucket_after.utilization_fee_earnings, earnings_before);
    assert_eq!(
        market.header.backing_provider_earnings_total.get(),
        earnings_before
    );
    assert_eq!(
        account.header.source_domains[0]
            .source_lien_capital_at_risk_fee_revenue
            .get(),
        revenue_before
    );
    assert_eq!(market.header.vault.get(), vault_before);
    assert_eq!(market.header.insurance.get(), insurance_before);
    assert_eq!(account.header.health_cert.valid, 1);
}

#[kani::proof]
#[kani::unwind(64)]
#[kani::solver(cadical)]
fn proof_v16_public_account_backing_fee_split_preserves_senior_stock() {
    let provider_fee_raw: u8 = kani::any();
    let insurance_fee_raw: u8 = kani::any();
    let margin_slack_raw: u8 = kani::any();
    kani::assume(provider_fee_raw <= 4);
    kani::assume(insurance_fee_raw <= 4);
    kani::assume(provider_fee_raw > 0 || insurance_fee_raw > 0);
    kani::assume((1..=8).contains(&margin_slack_raw));
    let provider_fee = provider_fee_raw as u128;
    let insurance_fee = insurance_fee_raw as u128;
    let total_fee = provider_fee + insurance_fee;
    let capital = total_fee + margin_slack_raw as u128;
    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    let market_id = markets[0].engine.asset.market_id.get();
    header.vault = V16PodU128::new(capital + 1);
    header.c_tot = V16PodU128::new(capital);
    header.source_fresh_backing_total_num = V16PodU128::new(BOUND_SCALE);
    account_header.capital = V16PodU128::new(capital);
    markets[0].engine.backing_long = BackingBucketV16Account::from_runtime(&BackingBucketV16 {
        market_id,
        fresh_unliened_backing_num: BOUND_SCALE,
        expiry_slot: 10,
        status: BackingBucketStatusV16::Fresh,
        ..BackingBucketV16::EMPTY
    });
    markets[0].engine.source_credit_long =
        SourceCreditStateV16Account::from_runtime(&SourceCreditStateV16 {
            fresh_reserved_backing_num: BOUND_SCALE,
            credit_rate_num: CREDIT_RATE_SCALE,
            ..SourceCreditStateV16::EMPTY
        });
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    account_header.health_cert = HealthCertV16Account::from_runtime(&HealthCertV16 {
        certified_equity: capital as i128,
        certified_initial_req: margin_slack_raw as u128,
        certified_maintenance_req: margin_slack_raw as u128,
        cert_oracle_epoch: market.header.oracle_epoch.get(),
        cert_funding_epoch: market.header.funding_epoch.get(),
        cert_risk_epoch: market.header.risk_epoch.get(),
        cert_asset_set_epoch: market.header.asset_set_epoch.get(),
        active_bitmap_at_cert: V16_EMPTY_ACTIVE_BITMAP,
        valid: true,
        ..HealthCertV16::default()
    });
    let vault_before = market.header.vault.get();
    let c_tot_before = market.header.c_tot.get();
    let insurance_before = market.header.insurance.get();
    let earnings_before = market.header.backing_provider_earnings_total.get();
    let mut account = PortfolioV16ViewMut::new(&mut account_header);

    let charged = market
        .charge_account_backing_fee_not_atomic(&mut account, 0, provider_fee, 1, insurance_fee)
        .unwrap();
    let bucket = market.markets[0]
        .engine
        .backing_long
        .try_to_runtime()
        .unwrap();

    kani::cover!(
        provider_fee > 0 && insurance_fee > 0,
        "public account backing fee covers provider and insurance split"
    );
    kani::cover!(
        provider_fee == 0 && insurance_fee > 0,
        "public account backing fee covers insurance-only split"
    );
    kani::cover!(
        provider_fee > 0 && insurance_fee == 0,
        "public account backing fee covers provider-only split"
    );
    assert_eq!(charged, total_fee);
    assert_eq!(market.header.vault.get(), vault_before);
    assert_eq!(market.header.c_tot.get(), c_tot_before - total_fee);
    assert_eq!(account.header.capital.get(), capital - total_fee);
    assert_eq!(
        market.header.insurance.get(),
        insurance_before + insurance_fee
    );
    assert_eq!(
        market.header.backing_provider_earnings_total.get(),
        earnings_before + provider_fee
    );
    assert_eq!(bucket.utilization_fee_earnings, provider_fee);
    assert_eq!(
        market.header.insurance_domain_budget_remaining_total.get(),
        insurance_fee
    );
    assert_eq!(
        market.header.c_tot.get()
            + market.header.insurance.get()
            + market.header.backing_provider_earnings_total.get(),
        c_tot_before + insurance_before + earnings_before
    );
    assert_eq!(
        account.header.health_cert.certified_equity.get(),
        margin_slack_raw as i128
    );
}

#[kani::proof]
#[kani::unwind(64)]
#[kani::solver(cadical)]
fn proof_v16_public_backing_fee_charges_only_selected_domain() {
    let provider_fee_raw: u8 = kani::any();
    let insurance_fee_raw: u8 = kani::any();
    let margin_slack_raw: u8 = kani::any();
    let unrelated_budget_raw: u8 = kani::any();
    let unrelated_backing_raw: u8 = kani::any();
    kani::assume(provider_fee_raw <= 4);
    kani::assume(insurance_fee_raw <= 4);
    kani::assume(provider_fee_raw > 0 || insurance_fee_raw > 0);
    kani::assume((1..=8).contains(&margin_slack_raw));
    kani::assume((1..=8).contains(&unrelated_budget_raw));
    kani::assume((1..=4).contains(&unrelated_backing_raw));
    let provider_fee = provider_fee_raw as u128;
    let insurance_fee = insurance_fee_raw as u128;
    let total_fee = provider_fee + insurance_fee;
    let capital = total_fee + margin_slack_raw as u128;
    let selected_backing_num = BOUND_SCALE;
    let unrelated_backing_num = unrelated_backing_raw as u128 * BOUND_SCALE;
    let unrelated_budget = unrelated_budget_raw as u128;
    let (market_group_id, _, _) = ids();
    let cfg = V16Config::public_user_fund_with_market_slots(1, 2, 0, 10);
    let mut header = MarketGroupV16HeaderAccount::default();
    header.market_group_id = market_group_id;
    header.config = V16ConfigAccount::from_runtime(&cfg);
    header.asset_slot_capacity = V16PodU32::new(2);
    header.asset_activation_count = V16PodU64::new(2);
    header.next_market_id = V16PodU64::new(3);
    header.slot_last = V16PodU64::new(1);
    header.current_slot = V16PodU64::new(1);
    header.vault = V16PodU128::new(capital + unrelated_budget + 1 + unrelated_backing_raw as u128);
    header.c_tot = V16PodU128::new(capital);
    header.insurance = V16PodU128::new(unrelated_budget);
    header.insurance_domain_budget_remaining_total = V16PodU128::new(unrelated_budget);
    header.source_fresh_backing_total_num = V16PodU128::new(
        selected_backing_num
            .checked_add(unrelated_backing_num)
            .unwrap(),
    );
    let mut markets = [
        Market::new(0u64, EngineAssetSlotV16Account::empty_for_market(1)),
        Market::new(0u64, EngineAssetSlotV16Account::empty_for_market(2)),
    ];
    let mut asset0 = AssetStateV16::default();
    asset0.market_id = 1;
    asset0.lifecycle = AssetLifecycleV16::Active;
    asset0.raw_oracle_target_price = 100;
    asset0.effective_price = 100;
    asset0.fund_px_last = 100;
    asset0.slot_last = 1;
    markets[0].engine.asset = AssetStateV16Account::from_runtime(&asset0);
    markets[0].engine.backing_long = BackingBucketV16Account::from_runtime(&BackingBucketV16 {
        market_id: 1,
        fresh_unliened_backing_num: selected_backing_num,
        expiry_slot: 10,
        status: BackingBucketStatusV16::Fresh,
        ..BackingBucketV16::EMPTY
    });
    markets[0].engine.source_credit_long =
        SourceCreditStateV16Account::from_runtime(&SourceCreditStateV16 {
            fresh_reserved_backing_num: selected_backing_num,
            credit_rate_num: CREDIT_RATE_SCALE,
            ..SourceCreditStateV16::EMPTY
        });
    let mut asset1 = AssetStateV16::default();
    asset1.market_id = 2;
    asset1.lifecycle = AssetLifecycleV16::Active;
    asset1.raw_oracle_target_price = 100;
    asset1.effective_price = 100;
    asset1.fund_px_last = 100;
    asset1.slot_last = 1;
    markets[1].engine.asset = AssetStateV16Account::from_runtime(&asset1);
    markets[1].engine.insurance_domain_budget_long = V16PodU128::new(unrelated_budget);
    markets[1].engine.backing_long = BackingBucketV16Account::from_runtime(&BackingBucketV16 {
        market_id: 2,
        fresh_unliened_backing_num: unrelated_backing_num,
        expiry_slot: 10,
        status: BackingBucketStatusV16::Fresh,
        ..BackingBucketV16::EMPTY
    });
    markets[1].engine.source_credit_long =
        SourceCreditStateV16Account::from_runtime(&SourceCreditStateV16 {
            fresh_reserved_backing_num: unrelated_backing_num,
            credit_rate_num: CREDIT_RATE_SCALE,
            ..SourceCreditStateV16::EMPTY
        });
    let mut account_header = empty_account_fixture(market_group_id, 4);
    account_header.capital = V16PodU128::new(capital);
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    account_header.health_cert = HealthCertV16Account::from_runtime(&HealthCertV16 {
        certified_equity: capital as i128,
        certified_initial_req: margin_slack_raw as u128,
        certified_maintenance_req: margin_slack_raw as u128,
        cert_oracle_epoch: market.header.oracle_epoch.get(),
        cert_funding_epoch: market.header.funding_epoch.get(),
        cert_risk_epoch: market.header.risk_epoch.get(),
        cert_asset_set_epoch: market.header.asset_set_epoch.get(),
        active_bitmap_at_cert: V16_EMPTY_ACTIVE_BITMAP,
        valid: true,
        ..HealthCertV16::default()
    });
    let unrelated_budget_before = market.markets[1].engine.insurance_domain_budget_long;
    let unrelated_spent_before = market.markets[1].engine.insurance_domain_spent_long;
    let unrelated_bucket_before = market.markets[1]
        .engine
        .backing_long
        .try_to_runtime()
        .unwrap();
    let unrelated_source_before = market.markets[1]
        .engine
        .source_credit_long
        .try_to_runtime()
        .unwrap();
    let vault_before = market.header.vault.get();
    let c_tot_before = market.header.c_tot.get();
    let insurance_before = market.header.insurance.get();
    let earnings_before = market.header.backing_provider_earnings_total.get();
    let budget_total_before = market.header.insurance_domain_budget_remaining_total.get();
    let mut account = PortfolioV16ViewMut::new(&mut account_header);

    let charged = market
        .charge_account_backing_fee_not_atomic(&mut account, 0, provider_fee, 0, insurance_fee)
        .unwrap();
    let selected_bucket = market.markets[0]
        .engine
        .backing_long
        .try_to_runtime()
        .unwrap();
    let unrelated_bucket_after = market.markets[1]
        .engine
        .backing_long
        .try_to_runtime()
        .unwrap();
    let unrelated_source_after = market.markets[1]
        .engine
        .source_credit_long
        .try_to_runtime()
        .unwrap();

    kani::cover!(
        provider_fee > 0 && insurance_fee > 0 && unrelated_budget > 0,
        "selected backing fee covers mixed routing with unrelated funded domain"
    );
    assert_eq!(charged, total_fee);
    assert_eq!(market.header.vault.get(), vault_before);
    assert_eq!(market.header.c_tot.get(), c_tot_before - total_fee);
    assert_eq!(
        market.header.insurance.get(),
        insurance_before + insurance_fee
    );
    assert_eq!(
        market.header.backing_provider_earnings_total.get(),
        earnings_before + provider_fee
    );
    assert_eq!(
        market.header.insurance_domain_budget_remaining_total.get(),
        budget_total_before + insurance_fee
    );
    assert_eq!(selected_bucket.utilization_fee_earnings, provider_fee);
    assert_eq!(
        market.markets[0].engine.insurance_domain_budget_long.get(),
        insurance_fee
    );
    assert_eq!(
        market.markets[1].engine.insurance_domain_budget_long,
        unrelated_budget_before
    );
    assert_eq!(
        market.markets[1].engine.insurance_domain_spent_long,
        unrelated_spent_before
    );
    assert_eq!(unrelated_bucket_after, unrelated_bucket_before);
    assert_eq!(unrelated_source_after, unrelated_source_before);
    assert_eq!(
        market.header.c_tot.get()
            + market.header.insurance.get()
            + market.header.backing_provider_earnings_total.get(),
        c_tot_before + insurance_before + earnings_before
    );
    assert_eq!(market.validate_shape(), Ok(()));
}

#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn proof_v16_backing_provider_earnings_withdraw_cannot_exceed_earnings() {
    let vault: u128 = kani::any();
    let earnings: u128 = kani::any();
    let amount: u128 = kani::any();
    let result = kani_apply_backing_provider_earnings_withdraw(vault, earnings, amount);
    let expected_ok = amount <= earnings && amount <= vault;

    kani::cover!(
        amount == earnings && amount > 0 && expected_ok,
        "provider earnings withdraw covers full earned payout"
    );
    kani::cover!(amount == 0, "provider earnings withdraw covers zero no-op");
    kani::cover!(
        amount <= earnings && amount > vault,
        "provider earnings withdraw rejects vault underflow even if bucket has earnings"
    );
    assert_eq!(result.is_ok(), expected_ok);
    if let Ok((next_vault, next_earnings)) = result {
        kani::cover!(
            amount > 0 && amount < earnings,
            "provider earnings withdraw covers partial earned payout"
        );
        assert_eq!(next_vault, vault - amount);
        assert_eq!(next_earnings, earnings - amount);
        assert_eq!(vault - next_vault, earnings - next_earnings);
        assert_eq!(
            next_vault.checked_sub(next_earnings),
            vault.checked_sub(earnings)
        );
    } else {
        kani::cover!(
            amount > earnings,
            "provider earnings withdraw rejects over-withdraw"
        );
        assert_eq!(result, Err(V16Error::CounterUnderflow));
    }
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_public_backing_provider_earnings_withdraw_debits_only_earned_vault() {
    let earnings_raw: u8 = kani::any();
    let withdraw_raw: u8 = kani::any();
    kani::assume((1..=10).contains(&earnings_raw));
    kani::assume((1..=10).contains(&withdraw_raw));
    kani::assume(withdraw_raw <= earnings_raw);
    let earnings = earnings_raw as u128;
    let withdraw = withdraw_raw as u128;
    let (mut header, mut markets, _) = one_market_view_fixture();
    let market_id = markets[0].engine.asset.market_id.get();
    header.vault = V16PodU128::new(earnings);
    markets[0].engine.backing_long = BackingBucketV16Account::from_runtime(&BackingBucketV16 {
        market_id,
        utilization_fee_earnings: earnings,
        status: BackingBucketStatusV16::Expired,
        ..BackingBucketV16::EMPTY
    });
    let c_tot_before = header.c_tot;
    let insurance_before = header.insurance;
    let vault_before = header.vault.get();
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    market.refresh_header_aggregate_totals_for_test().unwrap();
    let total_before = market.header.backing_provider_earnings_total.get();

    market
        .withdraw_backing_provider_earnings_not_atomic(0, withdraw)
        .unwrap();
    let bucket = market.markets[0]
        .engine
        .backing_long
        .try_to_runtime()
        .unwrap();

    kani::cover!(
        withdraw > 1 && bucket.utilization_fee_earnings > 0,
        "public backing earnings withdraw covers partial earned payout"
    );
    kani::cover!(
        withdraw == earnings,
        "public backing earnings withdraw covers full earned payout"
    );
    assert_eq!(market.header.vault.get(), earnings - withdraw);
    assert_eq!(bucket.utilization_fee_earnings, earnings - withdraw);
    assert_eq!(
        market.header.backing_provider_earnings_total.get(),
        earnings - withdraw
    );
    assert_eq!(vault_before - market.header.vault.get(), withdraw);
    assert_eq!(
        total_before - market.header.backing_provider_earnings_total.get(),
        withdraw
    );
    assert_eq!(
        market.header.vault.get()
            - market.header.c_tot.get()
            - market.header.insurance.get()
            - market.header.backing_provider_earnings_total.get(),
        0
    );
    assert_eq!(market.header.c_tot, c_tot_before);
    assert_eq!(market.header.insurance, insurance_before);
    assert_eq!(market.validate_shape(), Ok(()));
}

#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn proof_v16_backing_provider_earnings_credit_requires_vault_slack() {
    let vault: u128 = kani::any();
    let c_tot: u128 = kani::any();
    let insurance: u128 = kani::any();
    let earnings: u128 = kani::any();
    let bucket_earnings: u128 = kani::any();
    let amount: u128 = kani::any();
    let senior_before = c_tot
        .checked_add(insurance)
        .and_then(|v| v.checked_add(earnings));
    kani::assume(senior_before.is_some());
    kani::assume(senior_before.unwrap() <= vault);
    kani::assume(bucket_earnings <= earnings);

    let result = MarketGroupV16ViewMut::<u64>::kani_credit_backing_provider_earnings_delta(
        vault,
        c_tot,
        insurance,
        earnings,
        bucket_earnings,
        amount,
    );
    let next_earnings_expected = earnings.checked_add(amount);
    let next_bucket_expected = bucket_earnings.checked_add(amount);
    let senior_after = senior_before.unwrap().checked_add(amount);
    let expected_ok = next_earnings_expected.is_some()
        && next_bucket_expected.is_some()
        && senior_after.map(|v| v <= vault).unwrap_or(false);

    kani::cover!(
        amount > 0 && expected_ok && senior_before.unwrap() < vault,
        "backing-provider earnings credit covers fee credit from vault slack"
    );
    kani::cover!(
        amount > 0 && !expected_ok && senior_before.unwrap() == vault,
        "backing-provider earnings credit rejects without vault slack"
    );
    assert_eq!(result.is_ok(), expected_ok);
    if let Ok((next_earnings, next_bucket_earnings)) = result {
        assert_eq!(next_earnings, next_earnings_expected.unwrap());
        assert_eq!(next_bucket_earnings, next_bucket_expected.unwrap());
        assert!(
            c_tot
                .checked_add(insurance)
                .and_then(|v| v.checked_add(next_earnings))
                .unwrap()
                <= vault
        );
    } else if next_earnings_expected.is_none() {
        assert_eq!(result, Err(V16Error::CounterOverflow));
    } else if senior_after.is_none() {
        assert_eq!(result, Err(V16Error::ArithmeticOverflow));
    } else if senior_after.unwrap() > vault {
        assert_eq!(result, Err(V16Error::LockActive));
    } else {
        assert_eq!(result, Err(V16Error::CounterOverflow));
    }
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_public_backing_provider_earnings_credit_uses_only_vault_slack() {
    let c_tot_raw: u8 = kani::any();
    let insurance_raw: u8 = kani::any();
    let existing_raw: u8 = kani::any();
    let amount_raw: u8 = kani::any();
    let surplus_raw: u8 = kani::any();
    kani::assume(c_tot_raw <= 8);
    kani::assume(insurance_raw <= 8);
    kani::assume(existing_raw <= 8);
    kani::assume((1..=8).contains(&amount_raw));
    kani::assume(surplus_raw <= 8);
    let c_tot = c_tot_raw as u128;
    let insurance = insurance_raw as u128;
    let existing = existing_raw as u128;
    let amount = amount_raw as u128;
    let surplus = surplus_raw as u128;
    let (mut header, mut markets) = one_market_only_fixture();
    let market_id = markets[0].engine.asset.market_id.get();
    // +1 atom funds the hand-built fresh backing below (provider principal is
    // vault-funded and senior-side, never part of the creditable slack).
    header.vault = V16PodU128::new(c_tot + insurance + existing + amount + surplus + 1);
    header.c_tot = V16PodU128::new(c_tot);
    header.insurance = V16PodU128::new(insurance);
    header.backing_provider_earnings_total = V16PodU128::new(existing);
    header.source_fresh_backing_total_num = V16PodU128::new(BOUND_SCALE);
    markets[0].engine.backing_long = BackingBucketV16Account::from_runtime(&BackingBucketV16 {
        market_id,
        fresh_unliened_backing_num: BOUND_SCALE,
        utilization_fee_earnings: existing,
        expiry_slot: 10,
        status: BackingBucketStatusV16::Fresh,
        ..BackingBucketV16::EMPTY
    });
    markets[0].engine.source_credit_long =
        SourceCreditStateV16Account::from_runtime(&SourceCreditStateV16 {
            fresh_reserved_backing_num: BOUND_SCALE,
            credit_rate_num: CREDIT_RATE_SCALE,
            ..SourceCreditStateV16::EMPTY
        });
    let vault_before = header.vault.get();
    let c_tot_before = header.c_tot.get();
    let insurance_before = header.insurance.get();
    let total_before = header.backing_provider_earnings_total.get();
    let bucket_before = markets[0]
        .engine
        .backing_long
        .utilization_fee_earnings
        .get();
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let residual_before = market.kani_residual();

    market
        .credit_backing_provider_earnings_not_atomic(0, amount)
        .unwrap();
    let bucket = market.markets[0]
        .engine
        .backing_long
        .try_to_runtime()
        .unwrap();

    kani::cover!(
        existing > 0 && amount > 1,
        "public backing earnings credit covers additive provider earnings"
    );
    kani::cover!(
        surplus == 0,
        "public backing earnings credit covers exact vault-slack boundary"
    );
    assert_eq!(market.header.vault.get(), vault_before);
    assert_eq!(market.header.c_tot.get(), c_tot_before);
    assert_eq!(market.header.insurance.get(), insurance_before);
    assert_eq!(
        market.header.backing_provider_earnings_total.get(),
        total_before + amount
    );
    assert_eq!(bucket.utilization_fee_earnings, bucket_before + amount);
    assert_eq!(bucket.status, BackingBucketStatusV16::Fresh);
    assert!(
        market.header.c_tot.get()
            + market.header.insurance.get()
            + market.header.backing_provider_earnings_total.get()
            <= market.header.vault.get()
    );
    assert_eq!(
        market.header.vault.get()
            - market.header.c_tot.get()
            - market.header.insurance.get()
            - market.header.backing_provider_earnings_total.get()
            - market.header.source_fresh_backing_total_num.get() / BOUND_SCALE,
        surplus
    );
    assert_eq!(market.validate_shape(), Ok(()));
    // Exact pool drawdown (spec lockstep: junior_residual_pool -= X ;
    // backing_provider_earnings += X): the earnings credit draws the junior
    // pool by exactly the credited amount and nothing else.
    assert_eq!(market.kani_residual(), residual_before - amount);
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_public_counterparty_backing_deposit_moves_vault_and_scaled_source_state() {
    let existing_raw: u8 = kani::any();
    let amount_raw: u8 = kani::any();
    kani::assume(existing_raw <= 8);
    kani::assume((1..=8).contains(&amount_raw));
    let existing = existing_raw as u128;
    let amount = amount_raw as u128;
    let existing_num = existing * BOUND_SCALE;
    let amount_num = amount * BOUND_SCALE;
    let (mut header, mut markets) = one_market_only_fixture();
    let market_id = markets[0].engine.asset.market_id.get();
    header.vault = V16PodU128::new(existing);
    if existing != 0 {
        markets[0].engine.backing_long = BackingBucketV16Account::from_runtime(&BackingBucketV16 {
            market_id,
            fresh_unliened_backing_num: existing_num,
            expiry_slot: 10,
            status: BackingBucketStatusV16::Fresh,
            ..BackingBucketV16::EMPTY
        });
        markets[0].engine.source_credit_long =
            SourceCreditStateV16Account::from_runtime(&SourceCreditStateV16 {
                fresh_reserved_backing_num: existing_num,
                credit_rate_num: CREDIT_RATE_SCALE,
                ..SourceCreditStateV16::EMPTY
            });
        header.source_fresh_backing_total_num = V16PodU128::new(existing_num);
    }
    let vault_before = header.vault.get();
    let c_tot_before = header.c_tot.get();
    let insurance_before = header.insurance.get();
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);

    market
        .deposit_fresh_counterparty_backing_not_atomic(0, amount, 10)
        .unwrap();
    let bucket = market.markets[0]
        .engine
        .backing_long
        .try_to_runtime()
        .unwrap();
    let source = market.markets[0]
        .engine
        .source_credit_long
        .try_to_runtime()
        .unwrap();

    kani::cover!(
        existing == 0 && amount > 1,
        "public counterparty backing deposit covers first backing deposit"
    );
    kani::cover!(
        existing > 0 && amount > 1,
        "public counterparty backing deposit covers additive fresh-bucket deposit"
    );
    assert_eq!(market.header.vault.get(), vault_before + amount);
    assert_eq!(market.header.c_tot.get(), c_tot_before);
    assert_eq!(market.header.insurance.get(), insurance_before);
    assert_eq!(bucket.status, BackingBucketStatusV16::Fresh);
    assert_eq!(bucket.expiry_slot, 10);
    assert_eq!(bucket.fresh_unliened_backing_num, existing_num + amount_num);
    assert_eq!(bucket.valid_liened_backing_num, 0);
    assert_eq!(bucket.consumed_liened_backing_num, 0);
    assert_eq!(source.fresh_reserved_backing_num, existing_num + amount_num);
    assert_eq!(source.valid_liened_backing_num, 0);
    assert_eq!(source.spent_backing_num, 0);
    assert_eq!(source.provider_receivable_num, 0);
    assert_eq!(source.credit_rate_num, CREDIT_RATE_SCALE);
    assert_eq!(
        bucket.fresh_unliened_backing_num,
        source.fresh_reserved_backing_num
    );
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_public_counterparty_backing_deposit_refills_expired_receivable_bucket() {
    let amount_raw: u8 = kani::any();
    let receivable_raw: u8 = kani::any();
    kani::assume((1..=8).contains(&amount_raw));
    kani::assume((1..=8).contains(&receivable_raw));
    let amount = amount_raw as u128;
    let receivable = receivable_raw as u128;
    let amount_num = amount * BOUND_SCALE;
    let receivable_num = receivable * BOUND_SCALE;
    let refill_num = core::cmp::min(amount_num, receivable_num);
    let (mut header, mut markets) = one_market_only_fixture();
    let market_id = markets[0].engine.asset.market_id.get();
    markets[0].engine.backing_long = BackingBucketV16Account::from_runtime(&BackingBucketV16 {
        market_id,
        consumed_liened_backing_num: receivable_num,
        expiry_slot: 4,
        status: BackingBucketStatusV16::Expired,
        ..BackingBucketV16::EMPTY
    });
    markets[0].engine.source_credit_long =
        SourceCreditStateV16Account::from_runtime(&SourceCreditStateV16 {
            spent_backing_num: receivable_num,
            provider_receivable_num: receivable_num,
            credit_rate_num: CREDIT_RATE_SCALE,
            ..SourceCreditStateV16::EMPTY
        });
    let vault_before = header.vault.get();
    let c_tot_before = header.c_tot.get();
    let insurance_before = header.insurance.get();
    let risk_epoch_before = header.risk_epoch.get();
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);

    market
        .deposit_fresh_counterparty_backing_not_atomic(0, amount, 10)
        .unwrap();
    let bucket = market.markets[0]
        .engine
        .backing_long
        .try_to_runtime()
        .unwrap();
    let source = market.markets[0]
        .engine
        .source_credit_long
        .try_to_runtime()
        .unwrap();

    kani::cover!(
        amount_raw < receivable_raw,
        "public counterparty backing deposit covers partial expired receivable refill"
    );
    kani::cover!(
        amount_raw >= receivable_raw,
        "public counterparty backing deposit covers complete expired receivable refill"
    );
    assert_eq!(market.header.vault.get(), vault_before + amount);
    assert_eq!(market.header.c_tot.get(), c_tot_before);
    assert_eq!(market.header.insurance.get(), insurance_before);
    assert_eq!(market.header.risk_epoch.get(), risk_epoch_before + 1);
    assert_eq!(bucket.status, BackingBucketStatusV16::Fresh);
    assert_eq!(bucket.expiry_slot, 10);
    assert_eq!(bucket.fresh_unliened_backing_num, amount_num);
    assert_eq!(
        bucket.consumed_liened_backing_num,
        receivable_num - refill_num
    );
    assert_eq!(bucket.valid_liened_backing_num, 0);
    assert_eq!(bucket.impaired_liened_backing_num, 0);
    assert_eq!(source.fresh_reserved_backing_num, amount_num);
    assert_eq!(source.provider_receivable_num, receivable_num - refill_num);
    assert_eq!(source.spent_backing_num, receivable_num);
    assert_eq!(source.valid_liened_backing_num, 0);
    assert_eq!(source.impaired_liened_backing_num, 0);
    assert_eq!(source.credit_rate_num, CREDIT_RATE_SCALE);
    assert_eq!(source.credit_epoch, 1);
    assert_eq!(
        bucket.consumed_liened_backing_num,
        source.provider_receivable_num
    );
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_public_counterparty_backing_expiry_is_value_neutral_and_impairs_liened_backing() {
    let fresh_raw: u8 = kani::any();
    let liened_raw: u8 = kani::any();
    kani::assume(fresh_raw <= 8);
    kani::assume(liened_raw <= 8);
    kani::assume(fresh_raw != 0 || liened_raw != 0);
    let fresh_atoms = fresh_raw as u128;
    let liened_atoms = liened_raw as u128;
    let fresh_num = fresh_atoms * BOUND_SCALE;
    let liened_num = liened_atoms * BOUND_SCALE;
    let (mut header, mut markets) = one_market_only_fixture();
    let market_id = markets[0].engine.asset.market_id.get();
    header.vault = V16PodU128::new(fresh_atoms + liened_atoms);
    header.source_fresh_backing_total_num = V16PodU128::new(fresh_num + liened_num);
    markets[0].engine.backing_long = BackingBucketV16Account::from_runtime(&BackingBucketV16 {
        market_id,
        fresh_unliened_backing_num: fresh_num,
        valid_liened_backing_num: liened_num,
        expiry_slot: 5,
        status: BackingBucketStatusV16::Fresh,
        ..BackingBucketV16::EMPTY
    });
    markets[0].engine.source_credit_long =
        SourceCreditStateV16Account::from_runtime(&SourceCreditStateV16 {
            fresh_reserved_backing_num: fresh_num + liened_num,
            valid_liened_backing_num: liened_num,
            credit_rate_num: CREDIT_RATE_SCALE,
            ..SourceCreditStateV16::EMPTY
        });
    let vault_before = header.vault.get();
    let c_tot_before = header.c_tot.get();
    let insurance_before = header.insurance.get();
    let risk_epoch_before = header.risk_epoch.get();
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let residual_before = market.kani_residual();

    market
        .expire_source_backing_bucket_not_atomic(0, 10)
        .unwrap();
    let bucket = market.markets[0]
        .engine
        .backing_long
        .try_to_runtime()
        .unwrap();
    let source = market.markets[0]
        .engine
        .source_credit_long
        .try_to_runtime()
        .unwrap();

    kani::cover!(
        fresh_raw > 0 && liened_raw == 0,
        "public backing expiry covers fresh-only expired bucket"
    );
    kani::cover!(
        liened_raw > 0,
        "public backing expiry covers liened backing impairment"
    );
    assert_eq!(market.header.vault.get(), vault_before);
    assert_eq!(market.header.c_tot.get(), c_tot_before);
    assert_eq!(market.header.insurance.get(), insurance_before);
    assert_eq!(market.header.risk_epoch.get(), risk_epoch_before + 1);
    assert_eq!(bucket.fresh_unliened_backing_num, 0);
    assert_eq!(bucket.valid_liened_backing_num, 0);
    assert_eq!(bucket.impaired_liened_backing_num, liened_num);
    assert_eq!(bucket.consumed_liened_backing_num, 0);
    assert_eq!(source.fresh_reserved_backing_num, 0);
    assert_eq!(source.valid_liened_backing_num, 0);
    assert_eq!(source.impaired_liened_backing_num, liened_num);
    assert_eq!(source.provider_receivable_num, 0);
    assert_eq!(source.spent_backing_num, 0);
    assert_eq!(source.credit_rate_num, CREDIT_RATE_SCALE);
    assert_eq!(source.credit_epoch, 1);
    if liened_num == 0 {
        assert_eq!(bucket.status, BackingBucketStatusV16::Expired);
    } else {
        assert_eq!(bucket.status, BackingBucketStatusV16::Impaired);
    }
    assert_eq!(
        bucket.impaired_liened_backing_num,
        source.impaired_liened_backing_num
    );
    // Stock destination of the expired atoms (review finding): expiry forfeits
    // the WHOLE bucket (unliened and liened alike) out of
    // counterparty_backing_principal into the JUNIOR RESIDUAL pool, vault flat.
    // No class disappears; the junior pool rises by EXACTLY the forfeit — this
    // exact equality is also the LoF-horn falsifier (no atoms split off into
    // movable surplus or anywhere else).
    assert_eq!(
        market.kani_residual(),
        residual_before + fresh_atoms + liened_atoms
    );
    // DoS-horn falsifier (review): the mandatory stock reconciliation MUST hold
    // on the post-expiry impaired state, or the next insurance/close/recovery
    // checkpoint would revert (frozen vault). expire runs validate_shape
    // internally via refresh; assert it explicitly so the property is pinned.
    assert_eq!(market.validate_shape(), Ok(()));
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_public_counterparty_backing_withdraw_debits_vault_and_scaled_source_state() {
    let backing_raw: u8 = kani::any();
    let withdraw_raw: u8 = kani::any();
    kani::assume((1..=8).contains(&backing_raw));
    kani::assume((1..=8).contains(&withdraw_raw));
    kani::assume(withdraw_raw <= backing_raw);
    let backing = backing_raw as u128;
    let withdraw = withdraw_raw as u128;
    let backing_num = backing * BOUND_SCALE;
    let withdraw_num = withdraw * BOUND_SCALE;
    let remaining_num = backing_num - withdraw_num;
    let (mut header, mut markets) = one_market_only_fixture();
    let market_id = markets[0].engine.asset.market_id.get();
    header.vault = V16PodU128::new(backing);
    header.source_fresh_backing_total_num = V16PodU128::new(backing_num);
    markets[0].engine.backing_long = BackingBucketV16Account::from_runtime(&BackingBucketV16 {
        market_id,
        fresh_unliened_backing_num: backing_num,
        expiry_slot: 10,
        status: BackingBucketStatusV16::Fresh,
        ..BackingBucketV16::EMPTY
    });
    markets[0].engine.source_credit_long =
        SourceCreditStateV16Account::from_runtime(&SourceCreditStateV16 {
            fresh_reserved_backing_num: backing_num,
            credit_rate_num: CREDIT_RATE_SCALE,
            ..SourceCreditStateV16::EMPTY
        });
    let vault_before = header.vault.get();
    let c_tot_before = header.c_tot.get();
    let insurance_before = header.insurance.get();
    let risk_epoch_before = header.risk_epoch.get();
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);

    market
        .withdraw_fresh_counterparty_backing_not_atomic(0, withdraw)
        .unwrap();
    let bucket = market.markets[0]
        .engine
        .backing_long
        .try_to_runtime()
        .unwrap();
    let source = market.markets[0]
        .engine
        .source_credit_long
        .try_to_runtime()
        .unwrap();

    kani::cover!(
        withdraw_raw < backing_raw,
        "public counterparty backing withdraw covers partial principal withdrawal"
    );
    kani::cover!(
        withdraw_raw == backing_raw,
        "public counterparty backing withdraw covers full principal withdrawal"
    );
    assert_eq!(market.header.vault.get(), vault_before - withdraw);
    assert_eq!(market.header.c_tot.get(), c_tot_before);
    assert_eq!(market.header.insurance.get(), insurance_before);
    assert_eq!(market.header.risk_epoch.get(), risk_epoch_before + 1);
    assert_eq!(bucket.fresh_unliened_backing_num, remaining_num);
    assert_eq!(bucket.valid_liened_backing_num, 0);
    assert_eq!(bucket.consumed_liened_backing_num, 0);
    assert_eq!(bucket.impaired_liened_backing_num, 0);
    assert_eq!(source.fresh_reserved_backing_num, remaining_num);
    assert_eq!(source.valid_liened_backing_num, 0);
    assert_eq!(source.spent_backing_num, 0);
    assert_eq!(source.provider_receivable_num, 0);
    assert_eq!(source.credit_rate_num, CREDIT_RATE_SCALE);
    assert_eq!(source.credit_epoch, 1);
    if remaining_num == 0 {
        assert_eq!(bucket.status, BackingBucketStatusV16::Empty);
        assert_eq!(bucket.expiry_slot, 0);
    } else {
        assert_eq!(bucket.status, BackingBucketStatusV16::Fresh);
        assert_eq!(bucket.expiry_slot, 10);
    }
    assert_eq!(
        bucket.fresh_unliened_backing_num,
        source.fresh_reserved_backing_num
    );
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_public_counterparty_backing_withdraw_rejects_underbacking_source_claims() {
    let backing_raw: u8 = kani::any();
    let withdraw_raw: u8 = kani::any();
    kani::assume((1..=8).contains(&backing_raw));
    kani::assume((1..=8).contains(&withdraw_raw));
    kani::assume(withdraw_raw <= backing_raw);
    let backing = backing_raw as u128;
    let withdraw = withdraw_raw as u128;
    let backing_num = backing * BOUND_SCALE;
    let (mut header, mut markets) = one_market_only_fixture();
    let market_id = markets[0].engine.asset.market_id.get();
    header.vault = V16PodU128::new(backing);
    header.pnl_pos_bound_tot = V16PodU128::new(backing);
    header.pnl_pos_bound_tot_num = V16PodU128::new(backing_num);
    header.source_fresh_backing_total_num = V16PodU128::new(backing_num);
    markets[0].engine.backing_long = BackingBucketV16Account::from_runtime(&BackingBucketV16 {
        market_id,
        fresh_unliened_backing_num: backing_num,
        expiry_slot: 10,
        status: BackingBucketStatusV16::Fresh,
        ..BackingBucketV16::EMPTY
    });
    markets[0].engine.source_credit_long =
        SourceCreditStateV16Account::from_runtime(&SourceCreditStateV16 {
            positive_claim_bound_num: backing_num,
            exact_positive_claim_num: backing_num,
            fresh_reserved_backing_num: backing_num,
            credit_rate_num: CREDIT_RATE_SCALE,
            ..SourceCreditStateV16::EMPTY
        });
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);

    let result = market.withdraw_fresh_counterparty_backing_not_atomic(0, withdraw);

    kani::cover!(
        withdraw_raw == 1,
        "public counterparty backing withdraw rejects minimal underbacking"
    );
    kani::cover!(
        withdraw_raw > 1,
        "public counterparty backing withdraw rejects nontrivial underbacking"
    );
    assert_eq!(result, Err(V16Error::LockActive));
}

#[kani::proof]
#[kani::unwind(16)]
#[kani::solver(cadical)]
fn proof_v16_counterparty_backing_withdraw_delta_debits_only_unliened_backing() {
    let backing_raw: u16 = kani::any();
    let withdraw_raw: u16 = kani::any();
    kani::assume(backing_raw > 0);
    kani::assume(withdraw_raw > 0);
    kani::assume(backing_raw <= 511);
    kani::assume(withdraw_raw <= 511);
    kani::assume(withdraw_raw <= backing_raw);
    let backing = backing_raw as u128;
    let withdraw = withdraw_raw as u128;
    let backing_num = backing * BOUND_SCALE;
    let withdraw_num = withdraw * BOUND_SCALE;
    let bucket = BackingBucketV16 {
        market_id: 1,
        fresh_unliened_backing_num: backing_num,
        valid_liened_backing_num: BOUND_SCALE,
        expiry_slot: 10,
        status: BackingBucketStatusV16::Fresh,
        ..BackingBucketV16::EMPTY
    };
    let source = SourceCreditStateV16 {
        fresh_reserved_backing_num: backing_num + BOUND_SCALE,
        valid_liened_backing_num: BOUND_SCALE,
        credit_rate_num: CREDIT_RATE_SCALE,
        ..SourceCreditStateV16::EMPTY
    };

    let (bucket_after, source_after) =
        MarketGroupV16ViewMut::<u64>::kani_prepare_counterparty_backing_withdraw_delta(
            bucket,
            source,
            withdraw_num,
        )
        .unwrap();

    let remaining = backing - withdraw;
    kani::cover!(
        withdraw > 0 && remaining > 0,
        "fresh backing withdraw delta covers partial principal withdrawal"
    );
    kani::cover!(
        withdraw == backing,
        "fresh backing withdraw delta covers full unliened principal withdrawal"
    );
    assert_eq!(
        bucket_after.fresh_unliened_backing_num,
        remaining * BOUND_SCALE
    );
    assert_eq!(bucket_after.valid_liened_backing_num, BOUND_SCALE);
    assert_eq!(
        source_after.fresh_reserved_backing_num,
        remaining * BOUND_SCALE + BOUND_SCALE
    );
    assert_eq!(source_after.valid_liened_backing_num, BOUND_SCALE);
    assert_eq!(source_after.provider_receivable_num, 0);
    assert_eq!(source_after.spent_backing_num, 0);
    if remaining == 0 {
        assert_eq!(bucket_after.status, BackingBucketStatusV16::Fresh);
    }
}

#[kani::proof]
#[kani::unwind(16)]
#[kani::solver(cadical)]
fn proof_v16_counterparty_backing_withdraw_delta_status_transitions_are_exact() {
    let amount_raw: u8 = kani::any();
    let fresh_raw: u8 = kani::any();
    let valid_raw: u8 = kani::any();
    let consumed_raw: u8 = kani::any();
    let impaired_raw: u8 = kani::any();
    let status_raw: u8 = kani::any();
    let source_short: bool = kani::any();
    kani::assume(amount_raw <= 8);
    kani::assume(fresh_raw <= 8);
    kani::assume(valid_raw <= 8);
    kani::assume(consumed_raw <= 8);
    kani::assume(impaired_raw <= 8);
    kani::assume(status_raw <= 3);

    let amount = amount_raw as u128 * BOUND_SCALE;
    let fresh = fresh_raw as u128 * BOUND_SCALE;
    let valid = valid_raw as u128 * BOUND_SCALE;
    let consumed = consumed_raw as u128 * BOUND_SCALE;
    let impaired = impaired_raw as u128 * BOUND_SCALE;
    let status = match status_raw {
        0 => BackingBucketStatusV16::Fresh,
        1 => BackingBucketStatusV16::Expired,
        2 => BackingBucketStatusV16::Impaired,
        _ => BackingBucketStatusV16::Empty,
    };
    let source_fresh_reserved = if source_short && amount > 0 {
        amount - 1
    } else {
        fresh + valid
    };
    let bucket = BackingBucketV16 {
        market_id: 1,
        fresh_unliened_backing_num: fresh,
        valid_liened_backing_num: valid,
        consumed_liened_backing_num: consumed,
        impaired_liened_backing_num: impaired,
        expiry_slot: 10,
        status,
        ..BackingBucketV16::EMPTY
    };
    let source = SourceCreditStateV16 {
        fresh_reserved_backing_num: source_fresh_reserved,
        valid_liened_backing_num: valid,
        spent_backing_num: consumed,
        provider_receivable_num: consumed,
        impaired_liened_backing_num: impaired,
        credit_rate_num: CREDIT_RATE_SCALE,
        ..SourceCreditStateV16::EMPTY
    };

    let result = MarketGroupV16ViewMut::<u64>::kani_prepare_counterparty_backing_withdraw_delta(
        bucket, source, amount,
    );
    let expected_ok = amount == 0
        || (status == BackingBucketStatusV16::Fresh
            && fresh >= amount
            && source_fresh_reserved >= amount);

    kani::cover!(
        amount == 0 && status != BackingBucketStatusV16::Fresh,
        "counterparty backing withdraw zero amount is an idempotent no-op before status checks"
    );
    kani::cover!(
        amount > 0 && status != BackingBucketStatusV16::Fresh,
        "counterparty backing withdraw rejects non-Fresh buckets"
    );
    kani::cover!(
        amount > 0 && status == BackingBucketStatusV16::Fresh && fresh < amount,
        "counterparty backing withdraw rejects insufficient bucket backing"
    );
    kani::cover!(
        amount > 0
            && status == BackingBucketStatusV16::Fresh
            && fresh >= amount
            && source_fresh_reserved < amount,
        "counterparty backing withdraw rejects insufficient source backing"
    );
    kani::cover!(
        expected_ok && amount > 0 && fresh > amount,
        "counterparty backing withdraw partial success remains Fresh"
    );
    kani::cover!(
        expected_ok && amount > 0 && fresh == amount && valid > 0,
        "counterparty backing withdraw full unliened success with valid liens remains Fresh"
    );
    kani::cover!(
        expected_ok && amount > 0 && fresh == amount && valid == 0 && impaired > 0,
        "counterparty backing withdraw full unliened success with impaired liens becomes Impaired"
    );
    kani::cover!(
        expected_ok && amount > 0 && fresh == amount && valid == 0 && impaired == 0 && consumed > 0,
        "counterparty backing withdraw full unliened success with consumed receivable becomes Expired"
    );
    kani::cover!(
        expected_ok
            && amount > 0
            && fresh == amount
            && valid == 0
            && impaired == 0
            && consumed == 0,
        "counterparty backing withdraw full unliened success with no obligations becomes Empty"
    );

    if expected_ok {
        let (next_bucket, next_source) = result.unwrap();
        assert_eq!(next_bucket.fresh_unliened_backing_num, fresh - amount);
        assert_eq!(
            next_source.fresh_reserved_backing_num,
            source_fresh_reserved - amount
        );
        assert_eq!(next_bucket.valid_liened_backing_num, valid);
        assert_eq!(next_source.valid_liened_backing_num, valid);
        assert_eq!(next_bucket.consumed_liened_backing_num, consumed);
        assert_eq!(next_source.provider_receivable_num, consumed);
        assert_eq!(next_bucket.impaired_liened_backing_num, impaired);
        assert_eq!(next_source.impaired_liened_backing_num, impaired);
        if amount == 0 || fresh > amount || valid > 0 {
            assert_eq!(next_bucket.status, status);
        } else if impaired > 0 {
            assert_eq!(next_bucket.status, BackingBucketStatusV16::Impaired);
        } else if consumed > 0 {
            assert_eq!(next_bucket.status, BackingBucketStatusV16::Expired);
        } else {
            assert_eq!(next_bucket.status, BackingBucketStatusV16::Empty);
            assert_eq!(next_bucket.expiry_slot, 0);
        }
    } else {
        assert_eq!(result, Err(V16Error::LockActive));
    }
}

#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
fn proof_v16_counterparty_backing_withdraw_cannot_underback_claims() {
    let backing_raw: u16 = kani::any();
    let withdraw_raw: u16 = kani::any();
    kani::assume(backing_raw > 1);
    kani::assume(withdraw_raw > 0);
    kani::assume(backing_raw <= 1024);
    kani::assume(withdraw_raw <= 1024);
    kani::assume(withdraw_raw < backing_raw);
    let backing_num = backing_raw as u128 * BOUND_SCALE;
    let withdraw_num = withdraw_raw as u128 * BOUND_SCALE;
    let source = SourceCreditStateV16 {
        positive_claim_bound_num: backing_num,
        exact_positive_claim_num: backing_num,
        fresh_reserved_backing_num: backing_num,
        credit_rate_num: CREDIT_RATE_SCALE,
        ..SourceCreditStateV16::EMPTY
    };
    let bucket = BackingBucketV16 {
        market_id: 1,
        fresh_unliened_backing_num: backing_num,
        expiry_slot: 10,
        status: BackingBucketStatusV16::Fresh,
        ..BackingBucketV16::EMPTY
    };

    let (bucket_after, source_after) =
        MarketGroupV16ViewMut::<u64>::kani_prepare_counterparty_backing_withdraw_delta(
            bucket,
            source,
            withdraw_num,
        )
        .unwrap();
    let post_rate = kani_expected_source_credit_rate_num_for_state(source_after).unwrap();

    kani::cover!(
        backing_raw > 255 && withdraw_raw > 1,
        "counterparty backing withdraw proof covers widened nontrivial underbacking"
    );
    assert_eq!(
        bucket_after.fresh_unliened_backing_num,
        backing_num - withdraw_num
    );
    assert_eq!(bucket_after.valid_liened_backing_num, 0);
    assert_eq!(source_after.positive_claim_bound_num, backing_num);
    assert_eq!(source_after.exact_positive_claim_num, backing_num);
    assert_eq!(
        source_after.fresh_reserved_backing_num,
        backing_num - withdraw_num
    );
    assert_eq!(source_after.valid_liened_backing_num, 0);
    assert_eq!(source_after.provider_receivable_num, 0);
    assert_eq!(source_after.spent_backing_num, 0);
    assert!(source_after.fresh_reserved_backing_num < source_after.positive_claim_bound_num);
    assert!(post_rate < CREDIT_RATE_SCALE);
}

#[kani::proof]
#[kani::unwind(16)]
#[kani::solver(cadical)]
fn proof_v16_health_cert_capital_debit_preserves_im_or_rejects() {
    let equity: i128 = kani::any();
    let im: u128 = kani::any();
    let maintenance: u128 = kani::any();
    let fee: u128 = kani::any();
    kani::assume(equity != i128::MIN);
    let cert = HealthCertV16 {
        certified_equity: equity,
        certified_initial_req: im,
        certified_maintenance_req: maintenance,
        valid: true,
        ..HealthCertV16::default()
    };
    let result = kani_health_cert_after_capital_debit(cert, fee);
    let next_equity_expected = i128::try_from(fee)
        .ok()
        .and_then(|fee_i128| equity.checked_sub(fee_i128));
    let expected_ok = next_equity_expected
        .map(|next_equity| next_equity >= 0 && (next_equity as u128) >= im)
        .unwrap_or(false);

    assert_eq!(result.is_ok(), expected_ok);
    if expected_ok {
        let next = result.unwrap();
        kani::cover!(
            fee > 0 && im > 0 && maintenance > im,
            "health cert fee debit covers positive fee with IM still satisfied"
        );
        kani::cover!(
            next_equity_expected.unwrap() as u128 == im,
            "health cert fee debit covers exact post-fee IM boundary"
        );
        assert_eq!(next.certified_equity, next_equity_expected.unwrap());
        assert_eq!(next.certified_initial_req, im);
        assert_eq!(next.certified_maintenance_req, maintenance);
        assert_eq!(next.valid, cert.valid);
        assert!((next.certified_equity as u128) >= next.certified_initial_req);
        assert_eq!(
            next.certified_liq_deficit,
            maintenance.saturating_sub(next.certified_equity as u128)
        );
    } else {
        kani::cover!(
            fee > 0 && next_equity_expected.is_some(),
            "health cert fee debit rejects insufficient post-fee IM"
        );
        kani::cover!(
            fee > i128::MAX as u128 || next_equity_expected.is_none(),
            "health cert fee debit covers arithmetic rejection"
        );
        if fee > i128::MAX as u128 {
            assert_eq!(result, Err(V16Error::ArithmeticOverflow));
        } else if next_equity_expected.is_none() {
            assert_eq!(result, Err(V16Error::ArithmeticOverflow));
        } else {
            assert_eq!(result, Err(V16Error::LockActive));
        }
    }
}

#[kani::proof]
#[kani::unwind(64)]
#[kani::solver(cadical)]
fn proof_v16_reused_asset_slot_rejects_stale_market_id_leg() {
    // CLEAN-ROOM FIX (was vacuous w.r.t. its named property): on master the leg
    // carried loss_weight = POS_SCALE while basis = units*POS_SCALE and
    // a_basis = ADL_ONE, so loss_weight_for_basis(abs, ADL_ONE) == units*POS_SCALE
    // and validate_active_leg returns InvalidLeg for EVERY units >= 2 — BEFORE
    // the leg.market_id != asset.market_id (HiddenLeg) check at v16.rs:3160. The
    // covers required units_raw > 2, so the stale-market-id property was never
    // exercised in the covered region, and the bare assert!(is_err()) passed on
    // the WRONG error (InvalidLeg, not HiddenLeg). Fix: exact per-basis weight so
    // validate_active_leg passes, the error pinned to HiddenLeg, plus an
    // Ok-baseline control showing the same multi-unit leg validates when bound to
    // the live asset's market_id (so the rejection is attributable to market_id).
    let stale_market_id_raw: u8 = kani::any();
    let units_raw: u8 = kani::any();
    let is_short: bool = kani::any();
    let market_id_is_current: bool = kani::any();
    kani::assume((1..=4).contains(&units_raw));
    let units = units_raw as i128;
    let basis = units * POS_SCALE as i128;
    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    let current_market_id = markets[0].engine.asset.market_id.get();
    // the "stale" id must genuinely differ from the live asset's id
    kani::assume(stale_market_id_raw as u64 != current_market_id);
    let leg = PortfolioLegV16 {
        active: true,
        asset_index: 0,
        market_id: if market_id_is_current {
            current_market_id
        } else {
            stale_market_id_raw as u64
        },
        side: if is_short {
            SideV16::Short
        } else {
            SideV16::Long
        },
        basis_pos_q: if is_short { -basis } else { basis },
        a_basis: ADL_ONE,
        k_snap: 0,
        f_snap: 0,
        epoch_snap: 0,
        // exact ceil(abs * SOCIAL_WEIGHT_SCALE / a_basis); with a_basis == ADL_ONE
        // == SOCIAL_WEIGHT_SCALE this is abs itself, so validate_active_leg passes
        // for every units in 1..=4 and the market_id clause becomes the operative
        // discriminant.
        loss_weight: basis.unsigned_abs(),
        b_snap: 0,
        b_rem: 0,
        b_epoch_snap: 0,
        b_stale: false,
        stale: false,
    };
    account_header.legs[0] = percolator::v16::PortfolioLegV16Account::from_runtime(&leg);
    let mut bitmap = account_header.active_bitmap.map(V16PodU64::get);
    active_bitmap_set(&mut bitmap, 0).unwrap();
    account_header.active_bitmap = bitmap.map(V16PodU64::new);

    let market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let account = PortfolioV16ViewMut::new(&mut account_header);
    let result = account.as_view().validate_with_market(&market.as_view());

    kani::cover!(
        !market_id_is_current && units_raw > 2 && is_short,
        "multi-unit stale market_id short leg reaches the market-id check"
    );
    kani::cover!(
        !market_id_is_current && units_raw > 2 && !is_short,
        "multi-unit stale market_id long leg reaches the market-id check"
    );
    kani::cover!(
        market_id_is_current && units_raw > 2,
        "current market_id baseline control validates at multi-unit basis"
    );
    if market_id_is_current {
        // Ok-baseline control: identical multi-unit leg bound to the live asset
        // validates, so the rejection below is attributable to market_id alone.
        assert_eq!(result, Ok(()));
    } else {
        // the named property: a stale market_id leg is rejected with HiddenLeg
        // (NOT InvalidLeg) after asset slot reuse.
        assert_eq!(result, Err(V16Error::HiddenLeg));
    }
}

#[kani::proof]
#[kani::unwind(64)]
#[kani::solver(cadical)]
fn proof_v16_duplicate_asset_legs_reject_before_double_counting_support() {
    let units_raw: u8 = kani::any();
    kani::assume((1..=8).contains(&units_raw));
    let units = units_raw as i128;
    let basis = units as i128 * POS_SCALE as i128;
    let (_, _, mut account_header) = one_market_view_fixture();
    let long_leg = PortfolioLegV16 {
        active: true,
        asset_index: 0,
        market_id: 1,
        side: SideV16::Long,
        basis_pos_q: basis,
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
    };
    let short_leg = PortfolioLegV16 {
        side: SideV16::Short,
        basis_pos_q: -basis,
        ..long_leg
    };
    account_header.legs[0] = percolator::v16::PortfolioLegV16Account::from_runtime(&long_leg);
    account_header.legs[1] = percolator::v16::PortfolioLegV16Account::from_runtime(&short_leg);
    let mut bitmap = account_header.active_bitmap.map(V16PodU64::get);
    active_bitmap_set(&mut bitmap, 0).unwrap();
    active_bitmap_set(&mut bitmap, 1).unwrap();
    account_header.active_bitmap = bitmap.map(V16PodU64::new);

    let account = PortfolioV16ViewMut::new(&mut account_header);
    let result = account.as_view().kani_active_leg_slot_for_asset(0);

    kani::cover!(
        units_raw > 4 && result.is_err(),
        "duplicate active asset legs are rejected before double-counting wide symbolic size"
    );
    assert!(result.is_err());
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_mark_asset_drain_only_is_value_neutral_and_epoch_scoped() {
    let c_tot_raw: u8 = kani::any();
    let insurance_raw: u8 = kani::any();
    kani::assume(c_tot_raw <= 8);
    kani::assume(insurance_raw <= 8);
    let c_tot = c_tot_raw as u128;
    let insurance = insurance_raw as u128;
    let (mut header, mut markets, _) = one_market_view_fixture();
    header.vault = V16PodU128::new(c_tot + insurance);
    header.c_tot = V16PodU128::new(c_tot);
    header.insurance = V16PodU128::new(insurance);
    let vault_before = header.vault;
    let c_tot_before = header.c_tot;
    let insurance_before = header.insurance;
    let asset_set_epoch_before = header.asset_set_epoch.get();
    let risk_epoch_before = header.risk_epoch.get();

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let residual_before = market.kani_residual();
    market.mark_asset_drain_only_not_atomic(0).unwrap();
    let asset = market.markets[0].engine.asset.try_to_runtime().unwrap();

    kani::cover!(
        c_tot > 0 && insurance > 0 && asset.lifecycle == AssetLifecycleV16::DrainOnly,
        "active asset can enter drain-only without moving symbolic senior balances"
    );
    assert_eq!(asset.lifecycle, AssetLifecycleV16::DrainOnly);
    assert_eq!(market.header.vault, vault_before);
    assert_eq!(market.header.c_tot, c_tot_before);
    assert_eq!(market.header.insurance, insurance_before);
    assert_eq!(
        market.header.asset_set_epoch.get(),
        asset_set_epoch_before + 1
    );
    assert_eq!(market.header.risk_epoch.get(), risk_epoch_before + 1);
    assert_eq!(market.validate_shape(), Ok(()));
    // Junior-pool isolation: lifecycle marking moves no value and cannot
    // touch the junior residual pool.
    assert_eq!(market.kani_residual(), residual_before);
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_retire_nonempty_asset_rejects() {
    let units_raw: u8 = kani::any();
    let retire_slot_raw: u16 = kani::any();
    kani::assume(units_raw > 0);
    kani::assume(retire_slot_raw > 0);
    let (mut header, mut markets, _) = one_market_view_fixture();
    let mut asset = markets[0].engine.asset.try_to_runtime().unwrap();
    asset.oi_eff_long_q = units_raw as u128 * POS_SCALE;
    asset.stored_pos_count_long = 1;
    asset.loss_weight_sum_long = units_raw as u128 * POS_SCALE;
    markets[0].engine.asset = AssetStateV16Account::from_runtime(&asset);

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let result = market.retire_empty_asset_not_atomic(0, retire_slot_raw as u64);

    kani::cover!(
        units_raw > 5 && retire_slot_raw > 10 && result == Err(V16Error::LockActive),
        "nonempty asset retirement reaches fail-closed guard for wide OI and slot"
    );
    assert_eq!(result, Err(V16Error::LockActive));
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_retire_empty_asset_is_value_neutral_and_epoch_scoped() {
    let with_senior_balances: bool = kani::any();
    let retire_slot_raw: u8 = kani::any();
    kani::assume((1..=10).contains(&retire_slot_raw));
    let c_tot = if with_senior_balances { 7 } else { 0 };
    let insurance = if with_senior_balances { 3 } else { 0 };
    let retire_slot = retire_slot_raw as u64;
    let (mut header, mut markets, _) = one_market_view_fixture();
    header.vault = V16PodU128::new(c_tot + insurance);
    header.c_tot = V16PodU128::new(c_tot);
    header.insurance = V16PodU128::new(insurance);
    let vault_before = header.vault;
    let c_tot_before = header.c_tot;
    let insurance_before = header.insurance;
    let asset_set_epoch_before = header.asset_set_epoch.get();
    let risk_epoch_before = header.risk_epoch.get();

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    market
        .retire_empty_asset_not_atomic(0, retire_slot)
        .unwrap();
    let asset = market.markets[0].engine.asset.try_to_runtime().unwrap();

    kani::cover!(
        retire_slot > 1 && with_senior_balances && asset.lifecycle == AssetLifecycleV16::Retired,
        "empty asset can retire without moving nonzero senior balances"
    );
    assert_eq!(asset.lifecycle, AssetLifecycleV16::Retired);
    assert_eq!(asset.retired_slot, retire_slot);
    assert_eq!(market.header.current_slot.get(), retire_slot);
    assert_eq!(market.header.vault, vault_before);
    assert_eq!(market.header.c_tot, c_tot_before);
    assert_eq!(market.header.insurance, insurance_before);
    assert_eq!(
        market.header.asset_set_epoch.get(),
        asset_set_epoch_before + 1
    );
    assert_eq!(market.header.risk_epoch.get(), risk_epoch_before + 1);
    assert_eq!(market.validate_shape(), Ok(()));
}

#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn proof_v16_positive_pnl_requires_full_source_claim_attribution() {
    let pnl_raw: u16 = kani::any();
    let missing_raw: u16 = kani::any();
    let extra_raw: u16 = kani::any();
    kani::assume(pnl_raw > 0);
    kani::assume(missing_raw > 0);
    kani::assume(pnl_raw <= 4096);
    let pnl = pnl_raw as i128;
    let required = pnl_raw as u128 * BOUND_SCALE;
    let missing = (missing_raw as u128).min(required);
    let insufficient = required - missing;
    let over_attributed = required + extra_raw as u128;

    let ok = kani_validate_positive_pnl_source_attribution(pnl, required);
    let over_ok = kani_validate_positive_pnl_source_attribution(pnl, over_attributed);
    let err = kani_validate_positive_pnl_source_attribution(pnl, insufficient);
    let non_positive = kani_validate_positive_pnl_source_attribution(-pnl, 0);

    kani::cover!(
        insufficient < required,
        "positive PnL source attribution rejects under-attributed claim bounds"
    );
    kani::cover!(
        pnl_raw > 255 && missing_raw == 1,
        "positive PnL source attribution covers widened one-unit source deficit"
    );
    kani::cover!(
        extra_raw > 0,
        "positive PnL source attribution accepts over-attributed claim bounds"
    );
    assert_eq!(ok, Ok(()));
    assert_eq!(over_ok, Ok(()));
    assert_eq!(err, Err(V16Error::InvalidLeg));
    assert_eq!(non_positive, Ok(()));
}

#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn proof_v16_source_credit_rate_never_exceeds_available_backing_ratio() {
    let claim_atoms = kani::any::<u16>() as u128;
    let backing_atoms = kani::any::<u16>() as u128;
    kani::assume(claim_atoms > 0);
    kani::assume(claim_atoms <= 511);
    kani::assume(backing_atoms <= 511);
    let claim_num = claim_atoms * BOUND_SCALE;
    let backing_num = backing_atoms * BOUND_SCALE;
    let state = SourceCreditStateV16 {
        positive_claim_bound_num: claim_num,
        exact_positive_claim_num: claim_num,
        fresh_reserved_backing_num: backing_num,
        ..SourceCreditStateV16::EMPTY
    };

    let rate = kani_expected_source_credit_rate_num_for_state(state).unwrap();
    let support = kani_source_credit_state_realizable_support_for_face(state, claim_atoms).unwrap();

    kani::cover!(
        backing_num < claim_num,
        "source credit rate proof covers haircut branch"
    );
    kani::cover!(
        backing_num >= claim_num,
        "source credit rate proof covers full-credit branch"
    );
    assert!(rate <= CREDIT_RATE_SCALE);
    assert!(support <= backing_atoms);
    assert!(support <= claim_atoms);
    if backing_num >= claim_num {
        assert_eq!(rate, CREDIT_RATE_SCALE);
        assert_eq!(support, claim_atoms);
    } else {
        assert!(rate < CREDIT_RATE_SCALE);
    }
}

#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn proof_v16_available_source_support_excludes_liened_and_encumbered_amounts() {
    let fresh_raw: u8 = kani::any();
    let liened_raw: u8 = kani::any();
    let insurance_raw: u8 = kani::any();
    let valid_insurance_raw: u8 = kani::any();
    let impaired_insurance_raw: u8 = kani::any();
    let force_invalid_backing: bool = kani::any();
    let force_invalid_insurance: bool = kani::any();
    kani::assume(fresh_raw <= 7);
    kani::assume(liened_raw <= 7);
    kani::assume(insurance_raw <= 7);
    kani::assume(valid_insurance_raw <= 7);
    kani::assume(impaired_insurance_raw <= 7);
    let fresh_atoms = fresh_raw as u128;
    let insurance_atoms = insurance_raw as u128;
    let liened_atoms = if force_invalid_backing {
        fresh_atoms + 1
    } else {
        (liened_raw.min(fresh_raw)) as u128
    };
    let valid_insurance_atoms = if force_invalid_insurance {
        insurance_atoms + 1
    } else {
        (valid_insurance_raw.min(insurance_raw)) as u128
    };
    let impaired_limit = insurance_atoms.saturating_sub(valid_insurance_atoms);
    let impaired_insurance_atoms = if force_invalid_insurance {
        impaired_insurance_raw as u128
    } else {
        (impaired_insurance_raw as u128).min(impaired_limit)
    };
    let expected = if liened_atoms > fresh_atoms
        || valid_insurance_atoms + impaired_insurance_atoms > insurance_atoms
    {
        None
    } else {
        Some(
            (fresh_atoms - liened_atoms)
                + (insurance_atoms - valid_insurance_atoms - impaired_insurance_atoms),
        )
    };
    let state = SourceCreditStateV16 {
        fresh_reserved_backing_num: fresh_atoms * BOUND_SCALE,
        valid_liened_backing_num: liened_atoms * BOUND_SCALE,
        insurance_credit_reserved_num: insurance_atoms * BOUND_SCALE,
        valid_liened_insurance_num: valid_insurance_atoms * BOUND_SCALE,
        impaired_liened_insurance_num: impaired_insurance_atoms * BOUND_SCALE,
        ..SourceCreditStateV16::EMPTY
    };

    let result = kani_available_backing_num_for_source_credit_state(state);

    kani::cover!(
        fresh_atoms > 0 && liened_atoms == fresh_atoms && insurance_atoms == 0,
        "available source support excludes fully liened counterparty backing"
    );
    kani::cover!(
        insurance_atoms > 0
            && valid_insurance_atoms + impaired_insurance_atoms == insurance_atoms
            && fresh_atoms == 0,
        "available source support excludes fully encumbered insurance support"
    );
    kani::cover!(
        expected.is_some()
            && expected.unwrap() > 0
            && liened_atoms > 0
            && valid_insurance_atoms > 0,
        "available source support combines unliened backing and unencumbered insurance"
    );
    kani::cover!(
        force_invalid_backing || force_invalid_insurance,
        "available source support rejects impossible encumbrance state"
    );

    match expected {
        Some(expected_atoms) => assert_eq!(result, Ok(expected_atoms * BOUND_SCALE)),
        None => assert_eq!(result, Err(V16Error::InvalidConfig)),
    }
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_positive_kf_delta_creates_source_claim_bound() {
    let delta_raw: u8 = kani::any();
    kani::assume((1..=10).contains(&delta_raw));
    let delta = delta_raw as i128;
    let delta_num = delta_raw as u128 * BOUND_SCALE;
    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    account_header.pnl = V16PodI128::new(0);

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    let (support_consumed, junior_face_burned) = market
        .kani_apply_signed_kf_delta_to_pnl(&mut account, delta, Some(1))
        .unwrap();

    kani::cover!(
        delta > 1,
        "positive K/F settlement creates nontrivial source-attributed claim"
    );
    assert_eq!(support_consumed, 0);
    assert_eq!(junior_face_burned, 0);
    assert_eq!(account.header.pnl.get(), delta);
    assert_eq!(
        account.header.source_domains[0]
            .source_claim_bound_num
            .get(),
        delta_num
    );
    assert_eq!(account.header.source_domains[0].domain.get(), 1);
    assert_eq!(
        account.header.source_domains[0]
            .source_claim_market_id
            .get(),
        1
    );
    assert_eq!(
        market.markets[0]
            .engine
            .source_credit_short
            .positive_claim_bound_num
            .get(),
        delta_num
    );
    assert_eq!(
        market.markets[0]
            .engine
            .source_credit_short
            .exact_positive_claim_num
            .get(),
        delta_num
    );
    assert_eq!(market.header.pnl_pos_tot.get(), delta as u128);
    assert_eq!(market.header.pnl_pos_bound_tot_num.get(), delta_num);
    assert_eq!(market.header.source_claim_bound_total_num.get(), delta_num);
}

#[kani::proof]
#[kani::unwind(24)]
#[kani::solver(cadical)]
fn proof_v16_unliened_source_support_is_capped_by_realizable_backing() {
    let claim_raw: u8 = kani::any();
    let backing_raw: u8 = kani::any();
    kani::assume((1..=5).contains(&claim_raw));
    kani::assume(backing_raw <= claim_raw);

    let claim = claim_raw as u128;
    let backing = backing_raw as u128;
    let claim_num = claim * BOUND_SCALE;
    let backing_num = backing * BOUND_SCALE;
    let mut source_credit = SourceCreditStateV16 {
        positive_claim_bound_num: claim_num,
        exact_positive_claim_num: claim_num,
        fresh_reserved_backing_num: backing_num,
        ..SourceCreditStateV16::EMPTY
    };
    source_credit.credit_rate_num =
        kani_expected_source_credit_rate_num_for_state(source_credit).unwrap();
    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    account_header.pnl = V16PodI128::new(claim as i128);
    account_header.reserved_pnl = V16PodU128::new(claim);
    account_header.source_domains[0].domain = V16PodU32::new(0);
    account_header.source_domains[0].source_claim_market_id = V16PodU64::new(1);
    account_header.source_domains[0].source_claim_bound_num = V16PodU128::new(claim_num);
    header.pnl_pos_tot = V16PodU128::new(claim);
    header.pnl_matured_pos_tot = V16PodU128::new(claim);
    header.pnl_pos_bound_tot_num = V16PodU128::new(claim_num);
    header.pnl_pos_bound_tot = V16PodU128::new(claim);
    markets[0].engine.source_credit_long =
        SourceCreditStateV16Account::from_runtime(&source_credit);
    markets[0].engine.backing_long = if backing_num == 0 {
        BackingBucketV16Account::from_runtime(&BackingBucketV16::empty_for_market(1))
    } else {
        BackingBucketV16Account::from_runtime(&BackingBucketV16 {
            market_id: 1,
            fresh_unliened_backing_num: backing_num,
            expiry_slot: 100,
            status: BackingBucketStatusV16::Fresh,
            ..BackingBucketV16::EMPTY
        })
    };

    let market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let account = PortfolioV16ViewMut::new(&mut account_header);
    let support = market
        .kani_account_unliened_source_realizable_support(&account.as_view(), claim)
        .unwrap();

    kani::cover!(
        backing != 0 && backing < claim,
        "unliened source support proof covers partial backing haircut"
    );
    kani::cover!(
        backing == 0,
        "unliened source support proof covers zero source support"
    );
    kani::cover!(
        backing == claim,
        "unliened source support proof covers fully backed claim"
    );
    assert!(support <= backing);
    assert!(support <= claim);
    if backing == claim {
        assert_eq!(support, claim);
    }
}

// Cross-account solvency: two independent winners holding positive-PnL claims
// attributed to the SAME source-credit domain cannot jointly realize more value
// than the single shared backing pool they both draw from. The existing
// single-account `unliened_source_support_is_capped_by_realizable_backing` proves
// support <= backing for ONE account; nothing proves the apportionment is
// conservative ACROSS accounts. This is the static heart of the issue-#104
// (asymmetric K-snap) class: an undercapitalized loser leaves backing < total
// claim, and the credit-rate haircut must dilute BOTH winners so their summed
// realizable support never exceeds the actual backing.
#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn proof_v16_cross_account_source_support_sum_capped_by_shared_backing() {
    let a_raw: u8 = kani::any();
    let b_raw: u8 = kani::any();
    let backing_raw: u8 = kani::any();
    kani::assume((1..=63).contains(&a_raw));
    kani::assume((1..=63).contains(&b_raw));
    let a = a_raw as u128;
    let b = b_raw as u128;
    let total = a + b;
    // Undercapitalized (haircut) OR exactly-backed regime: backing <= total claim.
    kani::assume(backing_raw as u128 <= total);
    let backing = backing_raw as u128;

    let total_num = total * BOUND_SCALE;
    let backing_num = backing * BOUND_SCALE;

    // Shared domain: total claim bound = a + b, single backing pool = `backing`.
    let mut source_credit = SourceCreditStateV16 {
        positive_claim_bound_num: total_num,
        exact_positive_claim_num: total_num,
        fresh_reserved_backing_num: backing_num,
        ..SourceCreditStateV16::EMPTY
    };
    source_credit.credit_rate_num =
        kani_expected_source_credit_rate_num_for_state(source_credit).unwrap();

    // The sparse table and settlement wiring are covered by separate proofs. This
    // harness targets the shared-source arithmetic used by every account support
    // query: two independently evaluated face claims cannot jointly realize more
    // than the single source-credit backing pool.
    let support_a = kani_source_credit_state_realizable_support_for_face(source_credit, a).unwrap();
    let support_b = kani_source_credit_state_realizable_support_for_face(source_credit, b).unwrap();

    kani::cover!(
        backing < total,
        "cross-account support covers undercapitalized haircut regime"
    );
    kani::cover!(
        backing == total,
        "cross-account support covers fully backed regime"
    );

    // Global conservation: the two winners' independently-computed realizable
    // support cannot jointly exceed the shared backing pool.
    assert!(support_a + support_b <= backing);
    assert!(support_a <= a);
    assert!(support_b <= b);
}

// Global junior-bound aggregation invariant: the group-level junior claim bound
// (`pnl_pos_bound_tot_num`) is the denominator for the non-source haircut
// (`haircut_effective_support`) and the resolved-payout snapshot, so it must
// never UNDERSTATE the aggregate per-domain source claims it haircuts against —
// otherwise the denominator is too small and support is over-computed. The
// mutation paths (credit/burn) keep `global >= sum(per-domain)` in lockstep, but
// `validate_shape` never checks it: a state with a fully-backed domain claim but
// a zero global bound is internally inconsistent yet currently accepted. This
// proof pins that invariant — it FAILS until validate_shape enforces the sum.
#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn proof_v16_validate_shape_rejects_global_junior_bound_below_domain_claims() {
    let claim_raw: u8 = kani::any();
    kani::assume((1..=5).contains(&claim_raw));
    let claim = claim_raw as u128;
    let claim_num = claim * BOUND_SCALE;

    // Inline market (no account fixture -> no 16-leg loop), so unwind(8) suffices.
    let (market_id, _, _) = ids();
    let cfg = V16Config::public_user_fund_with_market_slots(1, 1, 0, 10);
    let mut header = MarketGroupV16HeaderAccount::new_dynamic(market_id, cfg, 1, 0).unwrap();
    let mut markets = [Market::new(0u64, EngineAssetSlotV16Account::default())];
    {
        let mut view = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        view.activate_empty_market_not_atomic(0, 100, 1).unwrap();
    }

    // A pristine, fully-backed long domain holding `claim` of source claims:
    // available backing == claim_num so credit_rate is full (CREDIT_RATE_SCALE).
    let source_credit = SourceCreditStateV16 {
        positive_claim_bound_num: claim_num,
        exact_positive_claim_num: claim_num,
        fresh_reserved_backing_num: claim_num,
        credit_rate_num: CREDIT_RATE_SCALE,
        ..SourceCreditStateV16::EMPTY
    };
    markets[0].engine.source_credit_long =
        SourceCreditStateV16Account::from_runtime(&source_credit);
    markets[0].engine.backing_long = BackingBucketV16Account::from_runtime(&BackingBucketV16 {
        market_id: 1,
        fresh_unliened_backing_num: claim_num,
        expiry_slot: 100,
        status: BackingBucketStatusV16::Fresh,
        ..BackingBucketV16::EMPTY
    });
    header.source_claim_bound_total_num = V16PodU128::new(claim_num);
    header.source_fresh_backing_total_num = V16PodU128::new(claim_num);
    header.vault = V16PodU128::new(claim);
    // Group-level junior bound left at 0 -> global UNDERSTATES the domain's claims.
    // Every other facet of the state is valid (the backing is vault-funded and
    // aggregated); the only inconsistency is the missing aggregation relation.

    let market = MarketGroupV16ViewMut::new(&mut header, &mut markets);

    kani::cover!(
        claim > 0,
        "global-vs-domain aggregation covers nontrivial claim"
    );
    // The group bound (0) understates the per-domain source claims (claim_num > 0).
    // A sound validator must reject this; today it does not.
    assert_eq!(market.validate_shape(), Err(V16Error::InvalidConfig));
}

// Loser-side backing reservation is value-neutral: when a counterparty's realized
// loss is backed, exactly `backing` atoms move out of the loser's capital AND out
// of c_tot (in lockstep) and are absorbed into the loser's pnl, while the group
// vault is unchanged and `backing` never exceeds the loser's free capital. This is
// the collateralization step behind every source-credited winner claim, and it had
// NO proof coverage. `backing = min(new_loss, capital - negative_before)` exercises
// both the loss-capped and capital-capped branches.
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
fn proof_v16_capital_backed_loss_reservation_is_value_neutral_and_capital_capped() {
    let capital_raw: u8 = kani::any();
    let loss_raw: u8 = kani::any();
    kani::assume((1..=4).contains(&capital_raw));
    kani::assume((1..=8).contains(&loss_raw));
    let capital = capital_raw as u128;
    let loss = loss_raw as u128;

    // Inline market (no account fixture -> no 16-leg loop), valid activated domain 0.
    let (market_id, _, _) = ids();
    let cfg = V16Config::public_user_fund_with_market_slots(1, 1, 0, 10);
    let mut header = MarketGroupV16HeaderAccount::new_dynamic(market_id, cfg, 1, 0).unwrap();
    let mut markets = [Market::new(0u64, EngineAssetSlotV16Account::default())];
    {
        let mut view = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        view.activate_empty_market_not_atomic(0, 100, 1).unwrap();
    }
    // Single undercapitalized loser holding `loss` of realized loss as negative pnl.
    header.vault = V16PodU128::new(capital);
    header.c_tot = V16PodU128::new(capital);
    header.negative_pnl_account_count = V16PodU64::new(1);

    let mut acct_header = PortfolioAccountV16Account::default();
    acct_header.capital = V16PodU128::new(capital);
    acct_header.pnl = V16PodI128::new(-(loss as i128));

    let vault_before = header.vault.get();
    let c_tot_before = header.c_tot.get();

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let residual_before = market.kani_residual();
    let mut account = PortfolioV16ViewMut::new(&mut acct_header);

    // negative_before = 0 (nothing pre-encumbered); new loss = `loss`.
    market
        .kani_reserve_new_capital_backed_loss_for_source_domain_not_atomic(&mut account, 0, 0, loss)
        .unwrap();

    let expected_backing = loss.min(capital);

    kani::cover!(
        loss < capital,
        "capital-backed loss covers loss-capped branch"
    );
    kani::cover!(
        loss > capital,
        "capital-backed loss covers capital-capped branch"
    );

    // Backing never exceeds the loser's free capital nor the new loss.
    assert!(expected_backing <= capital);
    assert!(expected_backing <= loss);
    // Capital and c_tot each fall by exactly `backing` (lockstep), pnl rises by it,
    // and the vault does not move (value is reshaped, not created or destroyed).
    assert_eq!(account.header.capital.get(), capital - expected_backing);
    assert_eq!(market.header.c_tot.get(), c_tot_before - expected_backing);
    assert_eq!(market.header.vault.get(), vault_before);
    assert_eq!(
        account.header.pnl.get(),
        -(loss as i128) + expected_backing as i128
    );
    // Junior-pool isolation: the reservation moves capital into backing in
    // lockstep (c_tot -X, counterparty_backing_principal +X), so the junior
    // residual pool is untouched by loss crystallization.
    assert_eq!(market.kani_residual(), residual_before);
}

// residual() is the JUNIOR (positive-PnL) payout pool and feeds both the resolved
// payout snapshot and the live haircut. `backing_provider_earnings` (utilization
// fees owed to LPs) is SENIOR — validate_shape's senior stack includes it — so it
// must NOT sit in the junior pool. residual() currently subtracts only c_tot +
// insurance, over-stating the junior pool by exactly the earnings; on a haircut
// resolved-close that over-payment drives the final validate_shape past the vault
// and reverts forever (fund-stuck). residual() must equal
// vault - c_tot - insurance - backing_provider_earnings. This FAILS until residual
// also subtracts the senior earnings.
#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_residual_excludes_senior_backing_provider_earnings() {
    let earnings_raw: u8 = kani::any();
    let surplus_raw: u8 = kani::any();
    kani::assume(earnings_raw > 0);
    let earnings = earnings_raw as u128;
    let surplus = surplus_raw as u128;

    let (mut header, mut markets, _) = one_market_view_fixture();
    let market_id = markets[0].engine.asset.market_id.get();
    // vault covers c_tot(0) + insurance(0) + earnings(senior) + surplus(junior).
    header.vault = V16PodU128::new(earnings + surplus);
    markets[0].engine.backing_long = BackingBucketV16Account::from_runtime(&BackingBucketV16 {
        market_id,
        utilization_fee_earnings: earnings,
        status: BackingBucketStatusV16::Expired,
        ..BackingBucketV16::EMPTY
    });
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    market.refresh_header_aggregate_totals_for_test().unwrap();

    kani::cover!(
        earnings > 0 && surplus > 0,
        "residual exclusion covers nontrivial senior earnings and junior surplus"
    );
    // Start state is shape-valid: earnings is senior and within vault.
    assert_eq!(market.validate_shape(), Ok(()));
    // The junior payout pool must exclude the senior earnings.
    assert_eq!(market.kani_residual(), surplus);
}

// Terminal wind-down must be able to release a counterparty lien without the
// Live-only freshness gate. This proof targets the exact production delta used
// by Resolved-mode source-claim burn; the full close/set_account_pnl path is
// covered by integration tests, while this harness keeps Kani focused on the
// liveness-critical arithmetic/ledger transition.
#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn proof_v16_resolved_winddown_releases_liened_source_claim() {
    let units_raw: u8 = kani::any();
    kani::assume(units_raw > 0);
    let amount = (units_raw as u128) * BOUND_SCALE;

    let backing_bucket = BackingBucketV16 {
        market_id: 1,
        valid_liened_backing_num: amount,
        expiry_slot: 100,
        status: BackingBucketStatusV16::Fresh,
        ..BackingBucketV16::EMPTY
    };
    let source_credit = SourceCreditStateV16 {
        fresh_reserved_backing_num: amount,
        valid_liened_backing_num: amount,
        positive_claim_bound_num: amount,
        exact_positive_claim_num: amount,
        credit_rate_num: CREDIT_RATE_SCALE,
        ..SourceCreditStateV16::EMPTY
    };

    let (bucket_after, source_after) =
        MarketGroupV16ViewMut::<u64>::kani_prepare_counterparty_lien_terminal_release_delta(
            backing_bucket,
            source_credit,
            amount,
        )
        .unwrap();
    kani::cover!(
        units_raw > 4,
        "terminal wind-down releases wide counterparty lien"
    );
    assert_eq!(bucket_after.valid_liened_backing_num, 0);
    assert_eq!(bucket_after.fresh_unliened_backing_num, amount);
    assert_eq!(bucket_after.consumed_liened_backing_num, 0);
    assert_eq!(source_after.valid_liened_backing_num, 0);
    assert_eq!(source_after.fresh_reserved_backing_num, amount);
    assert_eq!(source_after.spent_backing_num, 0);
    assert_eq!(source_after.provider_receivable_num, 0);
}

// The terminal counterparty-lien release must also ignore bucket status/expiry:
// returning liened backing is a wind-down operation, not a fresh lending action.
// If this regresses to the Live release helper, a resolved market can deadlock
// after a bucket expires.
#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn proof_v16_resolved_winddown_releases_expired_liened_source_claim() {
    let units_raw: u8 = kani::any();
    let status_is_expired: bool = kani::any();
    kani::assume(units_raw > 0);
    let amount = (units_raw as u128) * BOUND_SCALE;

    let backing_bucket = BackingBucketV16 {
        market_id: 1,
        valid_liened_backing_num: amount,
        expiry_slot: 1,
        status: if status_is_expired {
            BackingBucketStatusV16::Expired
        } else {
            BackingBucketStatusV16::Fresh
        },
        ..BackingBucketV16::EMPTY
    };
    let source_credit = SourceCreditStateV16 {
        fresh_reserved_backing_num: amount,
        valid_liened_backing_num: amount,
        positive_claim_bound_num: amount,
        exact_positive_claim_num: amount,
        credit_rate_num: CREDIT_RATE_SCALE,
        ..SourceCreditStateV16::EMPTY
    };

    let (bucket_after, source_after) =
        MarketGroupV16ViewMut::<u64>::kani_prepare_counterparty_lien_terminal_release_delta(
            backing_bucket,
            source_credit,
            amount,
        )
        .unwrap();
    kani::cover!(
        status_is_expired && units_raw > 4,
        "terminal wind-down releases wide expired-status counterparty lien"
    );
    assert_eq!(bucket_after.valid_liened_backing_num, 0);
    assert_eq!(bucket_after.fresh_unliened_backing_num, amount);
    assert_eq!(bucket_after.consumed_liened_backing_num, 0);
    assert_eq!(source_after.valid_liened_backing_num, 0);
    assert_eq!(source_after.fresh_reserved_backing_num, amount);
    assert_eq!(source_after.spent_backing_num, 0);
    assert_eq!(source_after.provider_receivable_num, 0);
}

// Terminal wind-down must also clear insurance-backed liens that were impaired
// before resolution. The Live release helper intentionally only releases valid
// liens; terminal cleanup needs to remove the impaired counter and the reserved
// insurance backing, otherwise the source domain/asset slot can never become
// empty again.
#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn proof_v16_resolved_winddown_releases_impaired_insurance_lien() {
    let units_raw: u8 = kani::any();
    let impaired_case: bool = kani::any();
    kani::assume(units_raw > 0);
    let amount = (units_raw as u128) * BOUND_SCALE;
    let valid = if impaired_case { 0 } else { amount };
    let impaired = amount - valid;
    let reservation = InsuranceCreditReservationV16 {
        insurance_credit_reserved_num: amount,
        valid_liened_insurance_num: valid,
        impaired_liened_insurance_num: impaired,
        ..InsuranceCreditReservationV16::EMPTY
    };
    let source = SourceCreditStateV16 {
        insurance_credit_reserved_num: amount,
        valid_liened_insurance_num: valid,
        impaired_liened_insurance_num: impaired,
        credit_rate_num: CREDIT_RATE_SCALE,
        ..SourceCreditStateV16::EMPTY
    };

    let (reservation_after, source_after) =
        MarketGroupV16ViewMut::<u64>::kani_prepare_insurance_lien_terminal_release_delta(
            reservation,
            source,
            amount,
        )
        .unwrap();

    kani::cover!(
        impaired_case && units_raw > 4,
        "terminal wind-down releases wide impaired insurance lien"
    );
    kani::cover!(
        !impaired_case && units_raw > 4,
        "terminal wind-down still releases wide valid insurance lien"
    );
    assert_eq!(reservation_after.insurance_credit_reserved_num, 0);
    assert_eq!(reservation_after.valid_liened_insurance_num, 0);
    assert_eq!(reservation_after.impaired_liened_insurance_num, 0);
    assert_eq!(reservation_after.consumed_insurance_num, 0);
    assert_eq!(source_after.insurance_credit_reserved_num, 0);
    assert_eq!(source_after.valid_liened_insurance_num, 0);
    assert_eq!(source_after.impaired_liened_insurance_num, 0);
    assert_eq!(source_after, SourceCreditStateV16::EMPTY);
}

#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn proof_v16_final_impaired_source_claim_burn_clears_account_occupancy_counters() {
    let units_raw: u8 = kani::any();
    let burn_units_raw: u8 = kani::any();
    let effective_raw: u8 = kani::any();
    let impaired_fee_raw: u8 = kani::any();
    kani::assume((1..=8).contains(&units_raw));
    kani::assume((1..=units_raw).contains(&burn_units_raw));
    kani::assume((1..=8).contains(&effective_raw));
    kani::assume(impaired_fee_raw > 0);
    let claim_num = units_raw as u128 * BOUND_SCALE;
    let burn_num = burn_units_raw as u128 * BOUND_SCALE;
    let effective = effective_raw as u128;
    let impaired_fee = impaired_fee_raw as u128;
    let expected_next_claim = claim_num - burn_num;
    let expected_effective_burn = if expected_next_claim == 0 {
        effective
    } else {
        (burn_units_raw as u128).min(effective)
    };
    let expected_fee_after = if expected_next_claim == 0 {
        0
    } else {
        impaired_fee
    };
    let mut account_header = PortfolioAccountV16Account::default();
    account_header.source_domains[0] = PortfolioSourceDomainV16Account {
        domain: V16PodU32::new(0),
        source_claim_market_id: V16PodU64::new(1),
        source_claim_bound_num: V16PodU128::new(claim_num),
        source_claim_impaired_num: V16PodU128::new(claim_num),
        source_lien_impaired_effective_reserved: V16PodU128::new(effective),
        source_lien_impaired_capital_at_risk_fee_revenue: V16PodU128::new(impaired_fee),
        ..PortfolioSourceDomainV16Account::default()
    };
    let mut account = PortfolioV16ViewMut::new(&mut account_header);

    let (burned, effective_burned) =
        MarketGroupV16ViewMut::<u64>::kani_burn_impaired_account_source_claim_fields(
            &mut account,
            0,
            burn_num,
        )
        .unwrap();
    let source = account.header.source_domains[0];

    kani::cover!(
        burn_units_raw < units_raw && effective_raw > 4 && impaired_fee_raw > 1,
        "impaired source-claim burn covers partial burn retaining fee counter"
    );
    kani::cover!(
        burn_units_raw == units_raw && effective_raw > 4 && impaired_fee_raw > 1,
        "final impaired source-claim burn covers nontrivial fee-counter cleanup"
    );
    assert_eq!(burned, burn_num);
    assert_eq!(effective_burned, expected_effective_burn);
    assert_eq!(source.source_claim_bound_num.get(), expected_next_claim);
    assert_eq!(source.source_claim_impaired_num.get(), expected_next_claim);
    assert_eq!(
        source.source_lien_impaired_effective_reserved.get(),
        effective - expected_effective_burn
    );
    assert_eq!(
        source
            .source_lien_impaired_capital_at_risk_fee_revenue
            .get(),
        expected_fee_after
    );
    if expected_next_claim == 0 {
        assert!(
            !source.is_occupied(),
            "final impaired source-claim burn must not leave reward counters blocking dematerialization"
        );
    } else {
        assert!(source.is_occupied());
    }
}

#[kani::proof]
#[kani::unwind(16)]
#[kani::solver(cadical)]
fn proof_v16_insurance_lien_terminal_release_delta_handles_mixed_and_rejects_invalid() {
    let amount_units_raw: u8 = kani::any();
    let reservation_reserved_raw: u8 = kani::any();
    let reservation_valid_raw: u8 = kani::any();
    let reservation_impaired_raw: u8 = kani::any();
    let source_reserved_raw: u8 = kani::any();
    let source_valid_raw: u8 = kani::any();
    let source_impaired_raw: u8 = kani::any();
    let force_unaligned: bool = kani::any();
    kani::assume(amount_units_raw <= 8);
    kani::assume(reservation_reserved_raw <= 8);
    kani::assume(reservation_valid_raw <= 8);
    kani::assume(reservation_impaired_raw <= 8);
    kani::assume(source_reserved_raw <= 8);
    kani::assume(source_valid_raw <= 8);
    kani::assume(source_impaired_raw <= 8);

    let aligned_amount = amount_units_raw as u128 * BOUND_SCALE;
    let amount = if force_unaligned {
        aligned_amount + 1
    } else {
        aligned_amount
    };
    let reservation_reserved = reservation_reserved_raw as u128 * BOUND_SCALE;
    let reservation_valid = reservation_valid_raw as u128 * BOUND_SCALE;
    let reservation_impaired = reservation_impaired_raw as u128 * BOUND_SCALE;
    let source_reserved = source_reserved_raw as u128 * BOUND_SCALE;
    let source_valid = source_valid_raw as u128 * BOUND_SCALE;
    let source_impaired = source_impaired_raw as u128 * BOUND_SCALE;
    let reservation = InsuranceCreditReservationV16 {
        insurance_credit_reserved_num: reservation_reserved,
        valid_liened_insurance_num: reservation_valid,
        impaired_liened_insurance_num: reservation_impaired,
        ..InsuranceCreditReservationV16::EMPTY
    };
    let source = SourceCreditStateV16 {
        insurance_credit_reserved_num: source_reserved,
        valid_liened_insurance_num: source_valid,
        impaired_liened_insurance_num: source_impaired,
        credit_rate_num: CREDIT_RATE_SCALE,
        ..SourceCreditStateV16::EMPTY
    };

    let result = MarketGroupV16ViewMut::<u64>::kani_prepare_insurance_lien_terminal_release_delta(
        reservation,
        source,
        amount,
    );
    let valid_release = amount.min(reservation_valid);
    let impaired_release = amount - valid_release;
    let expected_ok = amount == 0
        || (!force_unaligned
            && reservation_reserved >= amount
            && source_reserved >= amount
            && source_valid >= valid_release
            && reservation_impaired >= impaired_release
            && source_impaired >= impaired_release);

    kani::cover!(amount == 0, "terminal insurance release covers zero no-op");
    kani::cover!(
        amount > 0 && force_unaligned,
        "terminal insurance release rejects unaligned bound amount"
    );
    kani::cover!(
        !force_unaligned && amount > 0 && reservation_reserved < amount,
        "terminal insurance release rejects insufficient reservation total"
    );
    kani::cover!(
        !force_unaligned
            && amount > 0
            && reservation_reserved >= amount
            && source_reserved >= amount
            && source_valid < valid_release,
        "terminal insurance release rejects insufficient source valid lien"
    );
    kani::cover!(
        !force_unaligned
            && amount > 0
            && valid_release < amount
            && reservation_impaired < impaired_release,
        "terminal insurance release rejects insufficient impaired reservation"
    );
    kani::cover!(
        expected_ok && amount > 0 && valid_release == amount && reservation_reserved > amount,
        "terminal insurance release covers valid-only partial release"
    );
    kani::cover!(
        expected_ok && amount > 0 && valid_release > 0 && valid_release < amount,
        "terminal insurance release covers mixed valid and impaired release"
    );
    kani::cover!(
        expected_ok && amount > 0 && valid_release == 0,
        "terminal insurance release covers impaired-only release"
    );

    assert_eq!(result.is_ok(), expected_ok);
    if expected_ok {
        let (next_reservation, next_source) = result.unwrap();
        assert_eq!(
            next_reservation.insurance_credit_reserved_num,
            reservation_reserved - amount
        );
        assert_eq!(
            next_reservation.valid_liened_insurance_num,
            reservation_valid - valid_release
        );
        assert_eq!(
            next_reservation.impaired_liened_insurance_num,
            reservation_impaired - impaired_release
        );
        assert_eq!(
            next_source.insurance_credit_reserved_num,
            source_reserved - amount
        );
        assert_eq!(
            next_source.valid_liened_insurance_num,
            source_valid - valid_release
        );
        assert_eq!(
            next_source.impaired_liened_insurance_num,
            source_impaired - impaired_release
        );
    } else if force_unaligned && amount > 0 {
        assert_eq!(result, Err(V16Error::InvalidConfig));
    } else {
        assert_eq!(result, Err(V16Error::CounterUnderflow));
    }
}

// General guard for the Finding-B class ("junior payout pool must exclude ALL
// senior funds"): residual() must be exactly the junior surplus that makes the
// full stock reconciliation balance — vault = senior_capital + insurance +
// backing_provider_earnings + residual. Constructing StockReconciliationProofV16
// with residual() as the unallocated (junior) surplus and validating it FAILS if
// residual omits any senior bucket (accounted != token_vault). This generalizes
// the earnings-specific proof to every senior bucket at once.
#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_residual_reconciles_with_senior_stock() {
    let c_tot_raw: u8 = kani::any();
    let insurance_raw: u8 = kani::any();
    let earnings_raw: u8 = kani::any();
    let backing_raw: u8 = kani::any();
    let surplus_raw: u8 = kani::any();
    let c_tot = c_tot_raw as u128;
    let insurance = insurance_raw as u128;
    let earnings = earnings_raw as u128;
    let backing = backing_raw as u128;
    let surplus = surplus_raw as u128;
    let vault = c_tot + insurance + earnings + backing + surplus;

    let (mut header, mut markets, _) = one_market_view_fixture();
    let market_id = markets[0].engine.asset.market_id.get();
    header.vault = V16PodU128::new(vault);
    header.c_tot = V16PodU128::new(c_tot);
    header.insurance = V16PodU128::new(insurance);
    if earnings > 0 || backing > 0 {
        markets[0].engine.backing_long = BackingBucketV16Account::from_runtime(&BackingBucketV16 {
            market_id,
            utilization_fee_earnings: earnings,
            fresh_unliened_backing_num: backing * BOUND_SCALE,
            expiry_slot: if backing > 0 { 10 } else { 0 },
            status: if backing > 0 {
                BackingBucketStatusV16::Fresh
            } else {
                BackingBucketStatusV16::Expired
            },
            ..BackingBucketV16::EMPTY
        });
        markets[0].engine.source_credit_long =
            SourceCreditStateV16Account::from_runtime(&SourceCreditStateV16 {
                fresh_reserved_backing_num: backing * BOUND_SCALE,
                credit_rate_num: CREDIT_RATE_SCALE,
                ..SourceCreditStateV16::EMPTY
            });
    }
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    market.refresh_header_aggregate_totals_for_test().unwrap();

    kani::cover!(
        c_tot > 4 && insurance > 4 && earnings > 4 && backing > 4 && surplus > 4,
        "residual reconciliation covers wide senior buckets with junior surplus"
    );
    // Valid, reachable shape (senior stack within vault).
    assert_eq!(market.validate_shape(), Ok(()));

    let residual = market.kani_residual();
    // residual is the true junior surplus...
    assert_eq!(residual, surplus);
    // ...and it reconciles the full senior/junior stock against the vault: omitting
    // ANY senior bucket from residual (LP earnings, recoverable counterparty
    // backing principal, ...) would break this balance.
    let recon = StockReconciliationProofV16 {
        token_vault: vault,
        senior_capital_total: c_tot,
        insurance_capital: insurance,
        backing_provider_earnings: earnings,
        counterparty_backing_principal: backing,
        settlement_rounding_residue_total: 0,
        unallocated_protocol_surplus: residual,
    };
    assert_eq!(recon.validate(), Ok(()));
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_live_positive_kf_delta_without_source_rejects() {
    let delta_raw: u8 = kani::any();
    let loss_raw: u8 = kani::any();
    let start_negative: bool = kani::any();
    kani::assume(delta_raw > 0);
    kani::assume(loss_raw > 0);
    let delta = delta_raw as i128;
    let loss = loss_raw as i128;
    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    if start_negative {
        account_header.pnl = V16PodI128::new(-loss);
        header.negative_pnl_account_count = V16PodU64::new(1);
    } else {
        account_header.pnl = V16PodI128::new(0);
    }
    let vault_before = header.vault;
    let c_tot_before = header.c_tot;
    let pnl_pos_tot_before = header.pnl_pos_tot;
    let pnl_pos_bound_tot_num_before = header.pnl_pos_bound_tot_num;
    let source_credit_before = markets[0].engine.source_credit_long;
    let source_domain_before = account_header.source_domains[0];

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    let result = market.kani_apply_signed_kf_delta_to_pnl(&mut account, delta, None);

    kani::cover!(
        !start_negative && delta > 10,
        "live positive K/F delta without source rejects before minting claim"
    );
    kani::cover!(
        start_negative && delta > loss,
        "live positive K/F delta without source burns excess face against existing loss"
    );
    if start_negative {
        assert_eq!(result, Ok((0, delta as u128)));
        assert_eq!(account.header.pnl.get(), -loss);
        assert_eq!(market.header.negative_pnl_account_count.get(), 1);
        assert_eq!(market.header.pnl_pos_tot.get(), 0);
        assert_eq!(market.header.pnl_pos_bound_tot_num.get(), 0);
        assert_eq!(market.header.source_claim_bound_total_num.get(), 0);
        assert_eq!(
            market.markets[0]
                .engine
                .source_credit_long
                .positive_claim_bound_num,
            source_credit_before.positive_claim_bound_num
        );
        assert_eq!(
            account.header.source_domains[0].source_claim_bound_num,
            source_domain_before.source_claim_bound_num
        );
        assert_eq!(
            account.header.source_domains[0].source_claim_market_id,
            source_domain_before.source_claim_market_id
        );
    } else {
        assert_eq!(result, Err(V16Error::InvalidLeg));
        assert_eq!(market.header.vault, vault_before);
        assert_eq!(market.header.c_tot, c_tot_before);
        assert_eq!(market.header.pnl_pos_tot, pnl_pos_tot_before);
        assert_eq!(
            market.header.pnl_pos_bound_tot_num,
            pnl_pos_bound_tot_num_before
        );
        assert_eq!(account.header.pnl.get(), 0);
        assert_eq!(
            market.markets[0]
                .engine
                .source_credit_long
                .positive_claim_bound_num,
            source_credit_before.positive_claim_bound_num
        );
        assert_eq!(
            account.header.source_domains[0].source_claim_bound_num,
            source_domain_before.source_claim_bound_num
        );
        assert_eq!(
            account.header.source_domains[0].source_claim_market_id,
            source_domain_before.source_claim_market_id
        );
    }
}

#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn proof_v16_resolved_receipt_payment_cannot_exceed_terminal_claim() {
    let terminal_raw: u16 = kani::any();
    let paid_raw: u16 = kani::any();
    let bound_slack_raw: u16 = kani::any();
    kani::assume(terminal_raw > 0);
    kani::assume(terminal_raw <= 4096);
    kani::assume(paid_raw <= terminal_raw);
    let terminal = terminal_raw as u128;
    let paid = paid_raw as u128;
    let bound_slack_num = bound_slack_raw as u128 * BOUND_SCALE;
    let prior_bound = terminal * BOUND_SCALE + bound_slack_num;
    let receipt = ResolvedPayoutReceiptV16 {
        present: true,
        prior_bound_contribution_num: prior_bound,
        live_released_face_at_receipt: terminal,
        terminal_positive_claim_face: terminal,
        paid_effective: paid,
        finalized: paid == terminal,
    };
    let remaining = terminal - paid;
    let ok_payment = kani_apply_resolved_payout_receipt_payment(receipt, remaining).unwrap();
    let overpay = kani_apply_resolved_payout_receipt_payment(receipt, remaining + 1);

    kani::cover!(
        paid < terminal && remaining > 0,
        "resolved receipt proof covers non-final receipt topup"
    );
    kani::cover!(
        paid == terminal && remaining == 0,
        "resolved receipt proof covers finalized idempotent zero topup"
    );
    kani::cover!(
        terminal_raw > 255 && bound_slack_raw > 0,
        "resolved receipt proof covers widened over-bound terminal receipt"
    );
    assert_eq!(ok_payment.paid_effective, terminal);
    assert!(ok_payment.finalized);
    assert_eq!(ok_payment.prior_bound_contribution_num, prior_bound);
    assert!(ok_payment.prior_bound_contribution_num >= terminal * BOUND_SCALE);
    assert_eq!(
        ok_payment.live_released_face_at_receipt,
        receipt.live_released_face_at_receipt
    );
    assert_eq!(ok_payment.terminal_positive_claim_face, terminal);
    assert_eq!(overpay, Err(V16Error::InvalidLeg));
}

#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn proof_v16_resolved_receipt_claimable_is_rate_monotone_and_overpaid_fails_closed() {
    const RATE_DEN: u128 = 17;
    let terminal_raw: u8 = kani::any();
    let paid_raw: u8 = kani::any();
    let low_raw: u8 = kani::any();
    let high_raw: u8 = kani::any();
    kani::assume((1..=64).contains(&terminal_raw));
    kani::assume(low_raw <= high_raw);
    kani::assume(high_raw as u128 <= RATE_DEN);
    kani::assume(paid_raw <= terminal_raw);

    let terminal = terminal_raw as u128;
    let paid = paid_raw as u128;
    let low = low_raw as u128;
    let high = high_raw as u128;
    let gross_low = terminal * low / RATE_DEN;
    let gross_high = terminal * high / RATE_DEN;
    let receipt = ResolvedPayoutReceiptV16 {
        present: true,
        prior_bound_contribution_num: terminal * BOUND_SCALE,
        live_released_face_at_receipt: terminal,
        terminal_positive_claim_face: terminal,
        paid_effective: paid,
        finalized: paid == terminal,
    };
    let low_ledger = ResolvedPayoutLedgerV16 {
        snapshot_residual: 0,
        terminal_claim_exact_receipts_num: terminal * BOUND_SCALE,
        terminal_claim_bound_unreceipted_num: 0,
        current_payout_rate_num: low,
        current_payout_rate_den: RATE_DEN,
        snapshot_slot: 1,
        payout_halted: false,
        finalized: false,
    };
    let high_ledger = ResolvedPayoutLedgerV16 {
        current_payout_rate_num: high,
        ..low_ledger
    };

    let claim_low = MarketGroupV16ViewMut::<u64>::kani_resolved_receipt_claimable_against_ledger(
        receipt, low_ledger,
    );
    let claim_high = MarketGroupV16ViewMut::<u64>::kani_resolved_receipt_claimable_against_ledger(
        receipt,
        high_ledger,
    );

    kani::cover!(
        low < high && paid <= gross_low && gross_high > gross_low,
        "resolved receipt rate monotonicity covers a strictly improving payout rate"
    );
    kani::cover!(
        paid > gross_low,
        "resolved receipt claimability rejects a receipt overpaid at the lower payout rate"
    );
    kani::cover!(
        low == high && paid <= gross_low,
        "resolved receipt rate monotonicity covers equal-rate idempotence"
    );
    assert!(gross_high >= gross_low);
    if paid <= gross_low {
        let low_value = claim_low.unwrap();
        let high_value = claim_high.unwrap();
        assert_eq!(low_value, gross_low - paid);
        assert_eq!(high_value, gross_high - paid);
        assert!(high_value >= low_value);
    } else {
        assert_eq!(claim_low, Err(V16Error::InvalidLeg));
    }
}

#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
fn proof_v16_public_resolved_payout_topup_pays_min_claimable_and_vault() {
    let claimable_raw: u8 = kani::any();
    let vault_raw: u8 = kani::any();
    kani::assume((1..=64).contains(&claimable_raw));
    kani::assume(vault_raw <= 64);
    let claimable = claimable_raw as u128;
    let vault = vault_raw as u128;
    let paid_before = 2u128;
    let terminal = paid_before + claimable;
    let payout = claimable.min(vault);
    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    header.mode = 1;
    header.vault = V16PodU128::new(vault);
    header.payout_snapshot_captured = 1;
    header.resolved_payout_ledger =
        ResolvedPayoutLedgerV16Account::from_runtime(&ResolvedPayoutLedgerV16 {
            snapshot_residual: terminal,
            terminal_claim_exact_receipts_num: terminal * BOUND_SCALE,
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
            prior_bound_contribution_num: terminal * BOUND_SCALE,
            live_released_face_at_receipt: 0,
            terminal_positive_claim_face: terminal,
            paid_effective: paid_before,
            finalized: false,
        });
    let ledger_before = header.resolved_payout_ledger;
    let c_tot_before = header.c_tot;
    let insurance_before = header.insurance;
    let account_capital_before = account_header.capital;
    let account_pnl_before = account_header.pnl;
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let residual_before = market.kani_residual();
    let mut account = PortfolioV16ViewMut::new(&mut account_header);

    let paid = market
        .kani_claim_resolved_payout_topup_core_not_atomic(&mut account)
        .unwrap();
    let receipt = account
        .header
        .resolved_payout_receipt
        .try_to_runtime()
        .unwrap();

    kani::cover!(payout > 0, "resolved payout topup pays a nonzero amount");
    kani::cover!(
        payout < claimable,
        "resolved payout topup is capped by vault"
    );
    kani::cover!(
        payout == claimable,
        "resolved payout topup can fully pay claimable amount"
    );
    assert_eq!(paid, payout);
    assert_eq!(market.header.vault.get(), vault - payout);
    assert_eq!(market.header.c_tot, c_tot_before);
    assert_eq!(market.header.insurance, insurance_before);
    assert_eq!(market.header.resolved_payout_ledger, ledger_before);
    assert_eq!(account.header.capital, account_capital_before);
    assert_eq!(account.header.pnl, account_pnl_before);
    assert_eq!(receipt.paid_effective, paid_before + payout);
    assert_eq!(receipt.terminal_positive_claim_face, terminal);
    assert_eq!(receipt.prior_bound_contribution_num, terminal * BOUND_SCALE);
    assert_eq!(receipt.finalized, payout == claimable);
    // Exact pool drawdown: a resolved payout draws the junior pool by
    // exactly the amount paid (vault falls, senior stack untouched).
    assert_eq!(market.kani_residual(), residual_before - payout);
}

#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
fn proof_v16_resolved_external_payout_requires_exact_capital_plus_claim_sources() {
    let capital_raw: u8 = kani::any();
    let resolved_raw: u8 = kani::any();
    let surplus_raw: u8 = kani::any();
    let mode_raw: u8 = kani::any();
    kani::assume(mode_raw <= 2);
    let capital_paid = capital_raw as u128;
    let resolved_payout_paid = resolved_raw as u128;
    let source_sum = capital_paid + resolved_payout_paid;
    let requested_external_out = match mode_raw {
        0 => source_sum,
        1 => source_sum + 1,
        _ => {
            kani::assume(source_sum > 0);
            source_sum - 1
        }
    };
    let surplus = surplus_raw as u128;
    let vault_before = requested_external_out + surplus;
    let vault_after = surplus;

    let result = TokenValueFlowProofV16::capital_and_resolved_payout_to_external_out(
        capital_paid,
        resolved_payout_paid,
        requested_external_out,
        vault_before,
        vault_after,
    );

    kani::cover!(
        mode_raw == 0 && capital_paid > 0 && resolved_payout_paid > 0,
        "resolved external payout supports mixed capital and resolved claim sources"
    );
    kani::cover!(
        mode_raw == 1 && source_sum > 0,
        "resolved external payout rejects overpay beyond available sources"
    );
    kani::cover!(
        mode_raw == 2 && source_sum > 1,
        "resolved external payout rejects under-matched source accounting"
    );

    if mode_raw == 0 {
        let proof = result.unwrap();
        assert_eq!(proof.external_quote_in, 0);
        assert_eq!(proof.external_quote_out, source_sum);
        assert_eq!(proof.vault_before, vault_before);
        assert_eq!(proof.vault_after, vault_after);
        assert_eq!(
            proof.debits[TokenValueClassV16::AccountCapital as usize],
            capital_paid
        );
        assert_eq!(
            proof.debits[TokenValueClassV16::ResolvedPayoutPaid as usize],
            resolved_payout_paid
        );
        assert_eq!(
            proof.credits[TokenValueClassV16::ExternalQuote as usize],
            source_sum
        );
        assert_eq!(proof.validate(), Ok(()));
    } else {
        assert_eq!(result, Err(V16Error::InvalidConfig));
    }
}

#[kani::proof]
#[kani::unwind(16)]
#[kani::solver(cadical)]
fn proof_v16_two_resolved_receipts_are_order_independent_when_snapshot_funded() {
    let a_raw: u8 = kani::any();
    let b_raw: u8 = kani::any();
    let residual_raw: u8 = kani::any();
    kani::assume((1..=32).contains(&a_raw));
    kani::assume((1..=32).contains(&b_raw));
    kani::assume((1..=64).contains(&residual_raw));
    let a_claim = a_raw as u128;
    let b_claim = b_raw as u128;
    let total_claim = a_claim + b_claim;
    let snapshot_residual = residual_raw as u128;
    let total_bound_num = total_claim * BOUND_SCALE;
    let rate_num = (snapshot_residual * BOUND_SCALE).min(total_bound_num);
    let ledger = ResolvedPayoutLedgerV16 {
        snapshot_residual,
        terminal_claim_exact_receipts_num: total_bound_num,
        terminal_claim_bound_unreceipted_num: 0,
        current_payout_rate_num: rate_num,
        current_payout_rate_den: total_bound_num,
        snapshot_slot: 1,
        payout_halted: false,
        finalized: false,
    };
    let a_receipt = ResolvedPayoutReceiptV16 {
        present: true,
        prior_bound_contribution_num: a_claim * BOUND_SCALE,
        live_released_face_at_receipt: 0,
        terminal_positive_claim_face: a_claim,
        paid_effective: 0,
        finalized: false,
    };
    let b_receipt = ResolvedPayoutReceiptV16 {
        present: true,
        prior_bound_contribution_num: b_claim * BOUND_SCALE,
        live_released_face_at_receipt: 0,
        terminal_positive_claim_face: b_claim,
        paid_effective: 0,
        finalized: false,
    };

    // CLEAN-ROOM FIX (was a referential-transparency tautology): on master both
    // "orders" called the PURE resolved_receipt_claimable_against_ledger(receipt,
    // ledger) on identical by-value args with NOTHING mutated between them, so
    // paid_a_first == paid_a_second held for ANY implementation — the order
    // changed no intermediate state, so order-independence was never exercised.
    // Fix: thread a symbolic SCARCE draining vault so paying one receipt reduces
    // what the other can draw; prove (a) total extraction is order-independent
    // (= min(ca+cb, vault)) even under scarcity, and (b) when fully funded each
    // receipt is paid its full claimable regardless of order. Uses the real
    // production claimable + apply_resolved_payout_receipt_payment.
    let vault_raw: u8 = kani::any();
    let vault: u128 = vault_raw as u128;

    let ca = MarketGroupV16ViewMut::<u64>::kani_resolved_receipt_claimable_against_ledger(
        a_receipt, ledger,
    )
    .unwrap();
    let cb = MarketGroupV16ViewMut::<u64>::kani_resolved_receipt_claimable_against_ledger(
        b_receipt, ledger,
    )
    .unwrap();

    // Order A-first: A draws against the full vault, B against what remains.
    let pay_a1 = ca.min(vault);
    let a_after1 = kani_apply_resolved_payout_receipt_payment(a_receipt, pay_a1).unwrap();
    let rem_after_a = vault - pay_a1;
    let pay_b1 = cb.min(rem_after_a);
    let b_after1 = kani_apply_resolved_payout_receipt_payment(b_receipt, pay_b1).unwrap();
    let total_a_first = pay_a1 + pay_b1;

    // Order B-first: B draws against the full vault, A against what remains.
    let pay_b2 = cb.min(vault);
    let b_after2 = kani_apply_resolved_payout_receipt_payment(b_receipt, pay_b2).unwrap();
    let rem_after_b = vault - pay_b2;
    let pay_a2 = ca.min(rem_after_b);
    let a_after2 = kani_apply_resolved_payout_receipt_payment(a_receipt, pay_a2).unwrap();
    let total_b_first = pay_a2 + pay_b2;

    kani::cover!(
        vault < ca + cb && ca > 0 && cb > 0,
        "scarce vault: at least one receipt is haircut, so order changes per-receipt payout"
    );
    kani::cover!(
        vault >= ca + cb && ca > 0 && cb > 0,
        "funded vault: both receipts payable in full"
    );
    kani::cover!(a_claim != b_claim, "asymmetric claim sizes");

    // (a) TOTAL extraction is order-independent under scarcity AND never exceeds
    //     the pool — the draining composition equals min(ca+cb, vault) either way.
    assert_eq!(total_a_first, total_b_first);
    assert!(total_a_first <= vault);
    // (b) when the vault funds both, each receipt is paid its full claimable
    //     regardless of order (per-receipt order-independence under sufficiency).
    if vault >= ca + cb {
        assert_eq!(pay_a1, ca);
        assert_eq!(pay_b1, cb);
        assert_eq!(pay_a2, ca);
        assert_eq!(pay_b2, cb);
        assert_eq!(a_after1.paid_effective, a_after2.paid_effective);
        assert_eq!(b_after1.paid_effective, b_after2.paid_effective);
    }
}

#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
fn proof_v16_public_resolved_close_flat_account_pays_only_capital_and_vault() {
    let capital_raw: u8 = kani::any();
    kani::assume((1..=5).contains(&capital_raw));
    let capital = capital_raw as u128;
    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    header.mode = 1;
    header.current_slot = V16PodU64::new(2);
    header.resolved_slot = V16PodU64::new(2);
    header.vault = V16PodU128::new(capital);
    header.c_tot = V16PodU128::new(capital);
    account_header.capital = V16PodU128::new(capital);
    account_header.pnl = V16PodI128::new(0);
    account_header.last_fee_slot = V16PodU64::new(2);
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);

    let outcome = market
        .close_resolved_account_not_atomic(&mut account, 0)
        .unwrap();

    kani::cover!(capital > 1, "resolved flat close pays nontrivial capital");
    assert_eq!(outcome, ResolvedCloseOutcomeV16::Closed { payout: capital });
    assert_eq!(market.header.vault.get(), 0);
    assert_eq!(market.header.c_tot.get(), 0);
    assert_eq!(account.header.capital.get(), 0);
    assert_eq!(account.header.pnl.get(), 0);
    assert_eq!(account.header.reserved_pnl.get(), 0);
    assert_eq!(market.validate_shape(), Ok(()));
    assert_eq!(account.validate_with_market(&market.as_view()), Ok(()));
}

#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
fn proof_v16_resolved_two_active_legs_are_unattributed_for_bankruptcy() {
    let (mut header, mut markets, mut account_header) = two_market_view_fixture();
    header.mode = 1; // Resolved
    header.current_slot = V16PodU64::new(2);
    header.resolved_slot = V16PodU64::new(2);
    header.slot_last = V16PodU64::new(2);

    let mut bitmap = account_header.active_bitmap.map(V16PodU64::get);
    active_bitmap_set(&mut bitmap, 0).unwrap();
    active_bitmap_set(&mut bitmap, 1).unwrap();
    account_header.active_bitmap = bitmap.map(V16PodU64::new);

    let mut asset0 = markets[0].engine.asset.try_to_runtime().unwrap();
    asset0.oi_eff_long_q = POS_SCALE;
    asset0.stored_pos_count_long = 1;
    asset0.loss_weight_sum_long = POS_SCALE;
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
        b_stale: false,
        stale: false,
    });

    let mut asset1 = markets[1].engine.asset.try_to_runtime().unwrap();
    asset1.oi_eff_short_q = POS_SCALE;
    asset1.stored_pos_count_short = 1;
    asset1.loss_weight_sum_short = POS_SCALE;
    markets[1].engine.asset = AssetStateV16Account::from_runtime(&asset1);
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

    let market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let account = PortfolioV16ViewMut::new(&mut account_header);
    let attribution = market
        .kani_resolved_bankruptcy_attribution(&account.as_view())
        .unwrap();

    kani::cover!(true, "resolved bankruptcy attribution sees two active legs");
    assert_eq!(attribution, None);
}

#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
fn proof_v16_resolved_unattributed_bad_debt_clears_without_recovery() {
    let loss_raw: u8 = kani::any();
    kani::assume((1..=8).contains(&loss_raw));
    let loss = loss_raw as i128;
    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    header.mode = 1; // Resolved
    header.current_slot = V16PodU64::new(2);
    header.resolved_slot = V16PodU64::new(2);
    header.slot_last = V16PodU64::new(2);
    header.negative_pnl_account_count = V16PodU64::new(1);
    account_header.pnl = V16PodI128::new(-loss);
    account_header.last_fee_slot = V16PodU64::new(2);

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    let result = market.kani_settle_resolved_bankruptcy_negative_pnl(&mut account);

    kani::cover!(
        loss > 3,
        "resolved close covers nontrivial unattributed terminal bad debt"
    );
    assert_ne!(result, Err(V16Error::RecoveryRequired));
    assert_eq!(result, Ok(()));
    assert_eq!(account.header.pnl.get(), 0);
    assert!(active_bitmap_is_empty(
        account.header.active_bitmap.map(V16PodU64::get)
    ));
    assert_eq!(market.header.negative_pnl_account_count.get(), 0);
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_expired_close_progress_declares_recovery_without_value_mutation() {
    let gross_raw: u8 = kani::any();
    let c_tot_raw: u8 = kani::any();
    let insurance_raw: u8 = kani::any();
    let max_slot_raw: u8 = kani::any();
    let overrun_raw: u8 = kani::any();
    kani::assume(gross_raw > 0);
    kani::assume(c_tot_raw <= 64);
    kani::assume(insurance_raw <= 64);
    kani::assume(max_slot_raw > 0);
    kani::assume(overrun_raw > 0);
    let gross = gross_raw as u128;
    let c_tot = c_tot_raw as u128;
    let insurance = insurance_raw as u128;
    let max_slot = max_slot_raw as u64;
    let current_slot = max_slot + overrun_raw as u64;
    let (mut header, mut markets, _) = one_market_view_fixture();
    header.current_slot = V16PodU64::new(current_slot);
    header.vault = V16PodU128::new(c_tot + insurance);
    header.c_tot = V16PodU128::new(c_tot);
    header.insurance = V16PodU128::new(insurance);
    let vault_before = header.vault;
    let c_tot_before = header.c_tot;
    let insurance_before = header.insurance;
    let ledger = CloseProgressLedgerV16 {
        active: true,
        finalized: false,
        canceled: false,
        close_id: 1,
        asset_index: 0,
        market_id: 1,
        domain_side: SideV16::Long,
        gross_loss_at_close_start: gross,
        drift_reference_slot: 0,
        max_close_slot: max_slot,
        residual_remaining: gross,
        ..CloseProgressLedgerV16::EMPTY
    };

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let result = market.kani_ensure_close_progress_not_expired(ledger);

    kani::cover!(
        overrun_raw > 1 && gross > 1 && result == Err(V16Error::RecoveryRequired),
        "expired live close progress declares recovery for symbolic close lifetime overrun"
    );
    assert_eq!(result, Err(V16Error::RecoveryRequired));
    assert_eq!(market.header.mode, 2);
    assert_eq!(
        market.header.recovery_reason.try_to_runtime().unwrap(),
        Some(PermissionlessRecoveryReasonV16::ActiveBankruptCloseCannotProgress)
    );
    assert_eq!(market.header.vault, vault_before);
    assert_eq!(market.header.c_tot, c_tot_before);
    assert_eq!(market.header.insurance, insurance_before);
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_close_progress_ledger_residual_equation_is_enforced() {
    let gross_raw: u8 = kani::any();
    let drift_raw: u8 = kani::any();
    let support_raw: u8 = kani::any();
    let insurance_raw: u8 = kani::any();
    let b_loss_raw: u8 = kani::any();
    let explicit_raw: u8 = kani::any();

    let gross = gross_raw as u128;
    let drift = drift_raw as u128;
    let support = support_raw as u128;
    let insurance = insurance_raw as u128;
    let b_loss = b_loss_raw as u128;
    let explicit = explicit_raw as u128;
    let total_loss = gross + drift;
    let progress = support + insurance + b_loss + explicit;
    kani::assume(total_loss > 0);
    kani::assume(progress <= total_loss);
    let residual = total_loss - progress;
    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    let base = CloseProgressLedgerV16 {
        active: true,
        finalized: residual == 0,
        canceled: false,
        close_id: 1,
        asset_index: 0,
        market_id: 1,
        domain_side: SideV16::Long,
        gross_loss_at_close_start: gross,
        drift_reference_slot: 0,
        max_close_slot: 10,
        support_consumed: support,
        junior_face_burned: support,
        insurance_spent: insurance,
        b_loss_booked: b_loss,
        explicit_loss_assigned: explicit,
        drift_consumed: drift,
        residual_remaining: residual,
        ..CloseProgressLedgerV16::EMPTY
    };
    let market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    account_header.close_progress = CloseProgressLedgerV16Account::from_runtime(&base);
    let account = PortfolioV16ViewMut::new(&mut account_header);

    let ok = account.validate_with_market(&market.as_view());

    let mut bad_header = account_header;
    let bad = CloseProgressLedgerV16 {
        residual_remaining: residual + 1,
        ..base
    };
    bad_header.close_progress = CloseProgressLedgerV16Account::from_runtime(&bad);
    let bad_account = PortfolioV16ViewMut::new(&mut bad_header);
    let rejected = bad_account.validate_with_market(&market.as_view());

    let understated_rejected = if residual > 0 {
        let mut understated_header = account_header;
        let understated = CloseProgressLedgerV16 {
            residual_remaining: residual - 1,
            ..base
        };
        understated_header.close_progress =
            CloseProgressLedgerV16Account::from_runtime(&understated);
        let understated_account = PortfolioV16ViewMut::new(&mut understated_header);
        understated_account.validate_with_market(&market.as_view())
    } else {
        Err(V16Error::InvalidLeg)
    };

    kani::cover!(
        residual == 0,
        "close progress proof covers finalized residual"
    );
    kani::cover!(
        residual != 0,
        "close progress proof covers pending residual"
    );
    kani::cover!(
        progress != 0,
        "close progress proof covers nonzero close cure progress"
    );
    kani::cover!(
        residual > 1,
        "close progress proof covers understated residual rejection"
    );
    assert_eq!(ok, Ok(()));
    assert_eq!(rejected, Err(V16Error::InvalidLeg));
    assert_eq!(understated_rejected, Err(V16Error::InvalidLeg));
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_permissionless_recovery_crank_is_accounting_neutral() {
    let c_tot_raw: u16 = kani::any();
    let insurance_raw: u16 = kani::any();
    let surplus_raw: u16 = kani::any();
    let current_slot_raw: u8 = kani::any();
    let now_slot_raw: u8 = kani::any();
    let reason_sel: u8 = kani::any();
    kani::assume(c_tot_raw <= 1024);
    kani::assume(insurance_raw <= 1024);
    kani::assume(surplus_raw <= 1024);
    kani::assume(current_slot_raw > 0);
    kani::assume(now_slot_raw > 0);
    kani::assume(reason_sel <= 2);
    let c_tot = c_tot_raw as u128;
    let insurance = insurance_raw as u128;
    let surplus = surplus_raw as u128;
    let current_slot = current_slot_raw as u64;
    let now_slot = now_slot_raw as u64;
    let reason = match reason_sel {
        0 => PermissionlessRecoveryReasonV16::ExplicitLossOrDustAuditOverflow,
        1 => PermissionlessRecoveryReasonV16::BlockedSegmentHeadroomOrRepresentability,
        _ => PermissionlessRecoveryReasonV16::OracleOrTargetUnavailableByAuthenticatedPolicy,
    };
    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    header.current_slot = V16PodU64::new(current_slot);
    header.slot_last = V16PodU64::new(current_slot);
    header.vault = V16PodU128::new(c_tot + insurance + surplus);
    header.c_tot = V16PodU128::new(c_tot);
    header.insurance = V16PodU128::new(insurance);
    account_header.capital = V16PodU128::new(c_tot);
    let vault_before = header.vault;
    let c_tot_before = header.c_tot;
    let insurance_before = header.insurance;
    let current_slot_before = header.current_slot;
    let slot_last_before = header.slot_last;
    let asset_before = markets[0].engine.asset.try_to_runtime().unwrap();
    let capital_before = account_header.capital;
    let pnl_before = account_header.pnl;
    let reserved_before = account_header.reserved_pnl;
    let fee_credits_before = account_header.fee_credits;

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let residual_before = market.kani_residual();
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    let outcome = market
        .kani_permissionless_crank(
            &mut account,
            PermissionlessCrankRequestV16 {
                now_slot,
                asset_index: 0,
                effective_price: 100,
                funding_rate_e9: 0,
                action: PermissionlessCrankActionV16::Recover(reason),
            },
        )
        .unwrap();

    kani::cover!(
        outcome == PermissionlessProgressOutcomeV16::RecoveryDeclared(reason)
            && now_slot > 1
            && c_tot > 0
            && insurance > 0
            && surplus > 0,
        "permissionless recovery crank reaches recovery declaration over symbolic senior balances and vault slack"
    );
    kani::cover!(
        reason == PermissionlessRecoveryReasonV16::BlockedSegmentHeadroomOrRepresentability,
        "permissionless recovery crank covers blocked-segment recovery reason"
    );
    kani::cover!(
        reason == PermissionlessRecoveryReasonV16::OracleOrTargetUnavailableByAuthenticatedPolicy,
        "permissionless recovery crank covers oracle-unavailable recovery reason"
    );
    assert_eq!(
        outcome,
        PermissionlessProgressOutcomeV16::RecoveryDeclared(reason)
    );
    assert_eq!(market.header.mode, 2);
    assert_eq!(
        market.header.recovery_reason.try_to_runtime().unwrap(),
        Some(reason)
    );
    assert_eq!(market.header.current_slot, current_slot_before);
    assert_eq!(market.header.slot_last, slot_last_before);
    assert_eq!(market.header.vault, vault_before);
    assert_eq!(market.header.c_tot, c_tot_before);
    assert_eq!(market.header.insurance, insurance_before);
    let asset_after = market.markets[0].engine.asset.try_to_runtime().unwrap();
    assert_eq!(asset_after.market_id, asset_before.market_id);
    assert_eq!(asset_after.lifecycle, asset_before.lifecycle);
    assert_eq!(asset_after.effective_price, asset_before.effective_price);
    assert_eq!(
        asset_after.raw_oracle_target_price,
        asset_before.raw_oracle_target_price
    );
    assert_eq!(asset_after.slot_last, asset_before.slot_last);
    assert_eq!(account.header.capital, capital_before);
    assert_eq!(account.header.pnl, pnl_before);
    assert_eq!(account.header.reserved_pnl, reserved_before);
    assert_eq!(account.header.fee_credits, fee_credits_before);
    // Junior-pool isolation: this transition must not move the junior
    // residual pool (haircut/payout funding).
    assert_eq!(market.kani_residual(), residual_before);
}

#[kani::proof]
#[kani::unwind(80)]
#[kani::solver(cadical)]
fn proof_v16_public_permissionless_empty_market_crank_advances_clock_without_value_movement() {
    let c_tot_raw: u16 = kani::any();
    let insurance_raw: u16 = kani::any();
    let surplus_raw: u16 = kani::any();
    let now_slot_raw: u8 = kani::any();
    let price_raw: u8 = kani::any();
    kani::assume(c_tot_raw <= 1024);
    kani::assume(insurance_raw <= 1024);
    kani::assume(surplus_raw <= 1024);
    kani::assume((1..=4).contains(&now_slot_raw));
    kani::assume((80..=120).contains(&price_raw));
    let c_tot = c_tot_raw as u128;
    let insurance = insurance_raw as u128;
    let surplus = surplus_raw as u128;
    let now_slot = now_slot_raw as u64;
    let effective_price = price_raw as u64;
    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    header.vault = V16PodU128::new(c_tot + insurance + surplus);
    header.c_tot = V16PodU128::new(c_tot);
    header.insurance = V16PodU128::new(insurance);
    let vault_before = header.vault;
    let c_tot_before = header.c_tot;
    let insurance_before = header.insurance;
    let asset_before = markets[0].engine.asset.try_to_runtime().unwrap();
    let oracle_epoch_before = header.oracle_epoch;
    let funding_epoch_before = header.funding_epoch;
    let capital_before = account_header.capital;
    let pnl_before = account_header.pnl;
    let reserved_before = account_header.reserved_pnl;
    let fee_credits_before = account_header.fee_credits;
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let residual_before = market.kani_residual();
    let mut account = PortfolioV16ViewMut::new(&mut account_header);

    let outcome = market
        .kani_permissionless_crank(
            &mut account,
            PermissionlessCrankRequestV16 {
                now_slot,
                asset_index: 0,
                effective_price,
                funding_rate_e9: 0,
                action: PermissionlessCrankActionV16::Refresh,
            },
        )
        .unwrap();
    let asset = market.markets[0].engine.asset.try_to_runtime().unwrap();
    let expected_asset_slot = if now_slot > asset_before.slot_last + 1 {
        asset_before.slot_last + 1
    } else {
        now_slot
    };

    kani::cover!(
        outcome == PermissionlessProgressOutcomeV16::AccountCurrent
            && asset.effective_price == effective_price
            && now_slot == expected_asset_slot
            && effective_price > 100
            && c_tot > 0
            && insurance > 0
            && surplus > 0,
        "permissionless empty-market crank catches up current segment over symbolic senior stock"
    );
    kani::cover!(
        now_slot == 1 && effective_price == 100,
        "permissionless empty-market crank covers same-slot no-op clock and price"
    );
    kani::cover!(
        effective_price < 100 && now_slot > expected_asset_slot,
        "permissionless empty-market crank covers stale bounded negative-price catchup"
    );
    assert_eq!(outcome, PermissionlessProgressOutcomeV16::AccountCurrent);
    assert_eq!(market.header.current_slot.get(), now_slot);
    assert_eq!(market.header.slot_last.get(), expected_asset_slot);
    assert_eq!(
        market.header.loss_stale_active,
        if expected_asset_slot < now_slot { 1 } else { 0 }
    );
    assert_eq!(asset.slot_last, expected_asset_slot);
    assert_eq!(asset.effective_price, effective_price);
    assert_eq!(asset.fund_px_last, effective_price);
    assert_eq!(
        asset.k_long,
        asset_before.k_long
            + (effective_price as i128 - asset_before.effective_price as i128) * ADL_ONE as i128
    );
    assert_eq!(
        asset.k_short,
        asset_before.k_short
            - (effective_price as i128 - asset_before.effective_price as i128) * ADL_ONE as i128
    );
    assert_eq!(market.header.oracle_epoch, oracle_epoch_before);
    assert_eq!(market.header.funding_epoch, funding_epoch_before);
    assert_eq!(market.header.vault, vault_before);
    assert_eq!(market.header.c_tot, c_tot_before);
    assert_eq!(market.header.insurance, insurance_before);
    assert_eq!(account.header.capital, capital_before);
    assert_eq!(account.header.pnl, pnl_before);
    assert_eq!(account.header.reserved_pnl, reserved_before);
    assert_eq!(account.header.fee_credits, fee_credits_before);
    // Junior-pool isolation: this transition must not move the junior
    // residual pool (haircut/payout funding).
    assert_eq!(market.kani_residual(), residual_before);
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_equity_active_accrual_requires_protective_progress_before_mutation() {
    let price_delta_raw: u8 = kani::any();
    kani::assume(price_delta_raw > 0);
    let price_delta = price_delta_raw as u64;
    let (mut header, mut markets, _) = one_market_view_fixture();
    let mut asset = markets[0].engine.asset.try_to_runtime().unwrap();
    asset.oi_eff_long_q = POS_SCALE;
    asset.stored_pos_count_long = 1;
    markets[0].engine.asset = AssetStateV16Account::from_runtime(&asset);
    let header_before = header;
    let market_before = markets[0];

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let result = market.accrue_asset_to_not_atomic(0, 2, 100 + price_delta, 0, false);

    kani::cover!(
        result == Err(V16Error::NonProgress) && price_delta > 1,
        "equity-active accrual proof covers nontrivial no-progress rejection"
    );
    kani::cover!(
        result.is_err() && result != Err(V16Error::NonProgress),
        "equity-active accrual proof covers pre-progress validation rejection"
    );
    assert!(result.is_err());
    assert_eq!(market.header.current_slot, header_before.current_slot);
    assert_eq!(market.header.slot_last, header_before.slot_last);
    assert_eq!(market.header.oracle_epoch, header_before.oracle_epoch);
    assert_eq!(market.markets[0].engine.asset, market_before.engine.asset);
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_equity_active_accrual_with_progress_commits_one_bounded_segment() {
    let now_slot_raw: u8 = kani::any();
    let price_delta_raw: u8 = kani::any();
    kani::assume((2..=4).contains(&now_slot_raw));
    kani::assume((1..=5).contains(&price_delta_raw));
    let now_slot = now_slot_raw as u64;
    let price = 100 + price_delta_raw as u64;
    let (mut header, mut markets, _) = one_market_view_fixture();
    let mut asset = markets[0].engine.asset.try_to_runtime().unwrap();
    let expected_asset_slot = asset.slot_last + 1;
    asset.oi_eff_long_q = POS_SCALE;
    asset.oi_eff_short_q = POS_SCALE;
    asset.stored_pos_count_long = 1;
    asset.stored_pos_count_short = 1;
    asset.loss_weight_sum_long = POS_SCALE;
    asset.loss_weight_sum_short = POS_SCALE;
    markets[0].engine.asset = AssetStateV16Account::from_runtime(&asset);
    let vault_before = header.vault;
    let c_tot_before = header.c_tot;
    let insurance_before = header.insurance;
    let oracle_epoch_before = header.oracle_epoch.get();

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let residual_before = market.kani_residual();
    let outcome = market
        .accrue_asset_to_not_atomic(0, now_slot, price, 0, true)
        .unwrap();
    let asset_after = market.markets[0].engine.asset.try_to_runtime().unwrap();

    kani::cover!(
        now_slot > expected_asset_slot,
        "equity-active accrual proof covers stale multi-slot catchup"
    );
    kani::cover!(
        price_delta_raw > 1,
        "equity-active accrual proof covers nontrivial price movement"
    );
    assert_eq!(outcome.dt, 1);
    assert!(outcome.price_move_active);
    assert!(!outcome.funding_active);
    assert!(outcome.equity_active);
    assert_eq!(outcome.loss_stale_after, expected_asset_slot < now_slot);
    assert_eq!(asset_after.slot_last, expected_asset_slot);
    assert_eq!(asset_after.effective_price, price);
    assert_eq!(market.header.current_slot.get(), now_slot);
    assert_eq!(market.header.slot_last.get(), expected_asset_slot);
    assert_eq!(
        market.header.loss_stale_active,
        if expected_asset_slot < now_slot { 1 } else { 0 }
    );
    assert_eq!(market.header.oracle_epoch.get(), oracle_epoch_before + 1);
    assert_eq!(market.header.vault, vault_before);
    assert_eq!(market.header.c_tot, c_tot_before);
    assert_eq!(market.header.insurance, insurance_before);
    // Junior-pool isolation: this transition must not move the junior
    // residual pool.
    assert_eq!(market.kani_residual(), residual_before);
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_price_move_cap_rejects_before_accrual_mutation() {
    let price_raw: u16 = kani::any();
    kani::assume(price_raw > 200);
    let (mut header, mut markets, _) = one_market_view_fixture();
    let mut asset = markets[0].engine.asset.try_to_runtime().unwrap();
    asset.oi_eff_long_q = POS_SCALE;
    asset.oi_eff_short_q = POS_SCALE;
    asset.stored_pos_count_long = 1;
    asset.stored_pos_count_short = 1;
    asset.loss_weight_sum_long = POS_SCALE;
    asset.loss_weight_sum_short = POS_SCALE;
    markets[0].engine.asset = AssetStateV16Account::from_runtime(&asset);
    let header_before = header;
    let market_before = markets[0];

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let result = market.accrue_asset_to_not_atomic(0, 2, price_raw as u64, 0, true);

    kani::cover!(
        price_raw > 201,
        "price-move cap proof covers nontrivial out-of-envelope price"
    );
    assert_eq!(result, Err(V16Error::RecoveryRequired));
    assert_eq!(market.header.current_slot, header_before.current_slot);
    assert_eq!(market.header.slot_last, header_before.slot_last);
    assert_eq!(market.header.oracle_epoch, header_before.oracle_epoch);
    assert_eq!(market.header.vault, header_before.vault);
    assert_eq!(market.header.c_tot, header_before.c_tot);
    assert_eq!(market.header.insurance, header_before.insurance);
    assert_eq!(market.markets[0].engine.asset, market_before.engine.asset);
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_funding_rate_cap_rejects_before_accrual_mutation() {
    let funding_raw: u8 = kani::any();
    kani::assume(funding_raw > 0);
    let (mut header, mut markets, _) = one_market_view_fixture();
    let mut asset = markets[0].engine.asset.try_to_runtime().unwrap();
    asset.oi_eff_long_q = POS_SCALE;
    asset.oi_eff_short_q = POS_SCALE;
    asset.stored_pos_count_long = 1;
    asset.stored_pos_count_short = 1;
    asset.loss_weight_sum_long = POS_SCALE;
    asset.loss_weight_sum_short = POS_SCALE;
    markets[0].engine.asset = AssetStateV16Account::from_runtime(&asset);
    let header_before = header;
    let market_before = markets[0];

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let result = market.accrue_asset_to_not_atomic(0, 2, 100, funding_raw as i128, true);

    kani::cover!(
        funding_raw > 1,
        "funding-rate cap proof covers nontrivial rejected funding"
    );
    assert_eq!(result, Err(V16Error::InvalidConfig));
    assert_eq!(market.header.current_slot, header_before.current_slot);
    assert_eq!(market.header.slot_last, header_before.slot_last);
    assert_eq!(market.header.funding_epoch, header_before.funding_epoch);
    assert_eq!(market.header.vault, header_before.vault);
    assert_eq!(market.header.c_tot, header_before.c_tot);
    assert_eq!(market.header.insurance, header_before.insurance);
    assert_eq!(market.markets[0].engine.asset, market_before.engine.asset);
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_resolved_residual_booking_without_loss_bearing_side_is_explicit_only() {
    let residual_raw: u64 = kani::any();
    kani::assume(residual_raw > 0);
    let residual = residual_raw as u128;
    let (mut header, mut markets, _) = one_market_view_fixture();
    header.mode = 1;
    let asset_before = markets[0].engine.asset;

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let outcome = market
        .kani_book_bankruptcy_residual_chunk_internal(0, SideV16::Long, residual)
        .unwrap();

    kani::cover!(
        residual > u8::MAX as u128,
        "resolved residual booking proof covers wide explicit residual"
    );
    assert_eq!(outcome.booked_loss, 0);
    assert_eq!(outcome.explicit_loss, residual);
    assert_eq!(outcome.delta_b, 0);
    assert_eq!(outcome.remaining_after, 0);
    assert_eq!(market.header.bankruptcy_hlock_active, 1);
    assert_eq!(market.markets[0].engine.asset, asset_before);
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_live_residual_booking_to_loss_bearing_side_is_bounded_and_exact() {
    let residual_raw: u8 = kani::any();
    let booked_raw: u8 = kani::any();
    let rem_raw: u8 = kani::any();
    kani::assume(residual_raw > 0);
    kani::assume(booked_raw > 0);
    kani::assume(booked_raw <= residual_raw);
    let residual = residual_raw as u128;
    let booked = booked_raw as u128;
    let rem = rem_raw as u128;

    let (_, markets, _) = one_market_view_fixture();
    let mut asset = markets[0].engine.asset.try_to_runtime().unwrap();
    asset.oi_eff_long_q = POS_SCALE;
    asset.oi_eff_short_q = POS_SCALE;
    asset.stored_pos_count_long = 1;
    asset.stored_pos_count_short = 1;
    asset.loss_weight_sum_long = SOCIAL_LOSS_DEN;
    asset.loss_weight_sum_short = SOCIAL_LOSS_DEN;
    asset.social_loss_remainder_short_num = rem;
    let b_short_before = asset.b_short_num;

    let outcome = MarketGroupV16ViewMut::<u64>::kani_apply_bankruptcy_residual_chunk_to_loss_side(
        &mut asset,
        SideV16::Short,
        booked,
        residual,
    )
    .unwrap()
    .unwrap();
    let numerator = booked * SOCIAL_LOSS_DEN + rem;
    let expected_delta_b = numerator / SOCIAL_LOSS_DEN;
    let expected_rem = numerator % SOCIAL_LOSS_DEN;

    kani::cover!(
        residual > booked && booked > 10,
        "live residual booking proof covers wide partial booking"
    );
    kani::cover!(
        rem != 0,
        "live residual booking proof covers carried social-loss remainder"
    );
    assert!(outcome.booked_loss > 0);
    assert!(outcome.booked_loss <= residual);
    assert_eq!(outcome.booked_loss, booked);
    assert_eq!(outcome.explicit_loss, 0);
    assert_eq!(outcome.delta_b, expected_delta_b);
    assert_eq!(outcome.remaining_after, residual - booked);
    assert_eq!(asset.b_short_num, b_short_before + expected_delta_b);
    assert_eq!(asset.social_loss_remainder_short_num, expected_rem);
    assert_eq!(asset.b_long_num, 0);
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_bankruptcy_residual_capacity_is_nonzero_and_bounded_with_headroom() {
    let residual_raw: u8 = kani::any();
    let chunk_raw: u8 = kani::any();
    let rem_raw: u8 = kani::any();
    kani::assume(residual_raw > 0);
    kani::assume(chunk_raw > 0);
    let residual = residual_raw as u128;
    let chunk = chunk_raw as u128;
    let expected = residual.min(chunk);

    let (mut header, mut markets, _) = one_market_view_fixture();
    header.config.public_b_chunk_atoms = V16PodU128::new(chunk);
    let mut asset = markets[0].engine.asset.try_to_runtime().unwrap();
    asset.oi_eff_long_q = POS_SCALE;
    asset.oi_eff_short_q = POS_SCALE;
    asset.stored_pos_count_long = 1;
    asset.stored_pos_count_short = 1;
    asset.loss_weight_sum_long = SOCIAL_LOSS_DEN;
    asset.loss_weight_sum_short = SOCIAL_LOSS_DEN;
    asset.social_loss_remainder_short_num = rem_raw as u128;
    markets[0].engine.asset = AssetStateV16Account::from_runtime(&asset);

    let market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let capacity = market
        .kani_bankruptcy_residual_single_step_capacity(0, SideV16::Long, residual)
        .unwrap();

    kani::cover!(
        residual > chunk,
        "bankruptcy residual capacity proof covers public chunk cap"
    );
    kani::cover!(
        residual <= chunk,
        "bankruptcy residual capacity proof covers full residual fit"
    );
    assert_eq!(capacity, expected);
    assert!(capacity > 0);
    assert!(capacity <= residual);
    assert!(capacity <= chunk);
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_liquidation_preflight_accepts_only_fully_durable_residual() {
    let residual_raw: u8 = kani::any();
    kani::assume((1..=8).contains(&residual_raw));
    let residual = residual_raw as u128;
    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    header.config.public_b_chunk_atoms = V16PodU128::new(residual);
    header.vault = V16PodU128::new(0);
    header.insurance = V16PodU128::new(0);
    account_header.pnl = V16PodI128::new(-(residual as i128));
    let mut asset = markets[0].engine.asset.try_to_runtime().unwrap();
    asset.oi_eff_long_q = POS_SCALE;
    asset.oi_eff_short_q = POS_SCALE;
    asset.stored_pos_count_long = 1;
    asset.stored_pos_count_short = 1;
    asset.loss_weight_sum_long = SOCIAL_LOSS_DEN;
    asset.loss_weight_sum_short = SOCIAL_LOSS_DEN;
    markets[0].engine.asset = AssetStateV16Account::from_runtime(&asset);
    let header_before = header;
    let market_before = markets[0];

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    market.refresh_header_aggregate_totals_for_test().unwrap();
    let account = PortfolioV16ViewMut::new(&mut account_header);
    let result =
        market.kani_preflight_liquidation_residual_durability(0, SideV16::Long, &account.as_view());

    kani::cover!(
        residual > 1,
        "liquidation residual preflight proof covers nontrivial residual"
    );
    assert_eq!(result, Ok(()));
    assert_eq!(market.header.mode, header_before.mode);
    assert_eq!(market.header.recovery_reason, header_before.recovery_reason);
    assert_eq!(market.header.vault, header_before.vault);
    assert_eq!(market.header.c_tot, header_before.c_tot);
    assert_eq!(market.header.insurance, header_before.insurance);
    assert_eq!(market.markets[0], market_before);
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_liquidation_preflight_routes_insufficient_residual_capacity_to_recovery() {
    let residual_raw: u8 = kani::any();
    kani::assume((2..=8).contains(&residual_raw));
    let residual = residual_raw as u128;
    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    header.config.public_b_chunk_atoms = V16PodU128::new(residual - 1);
    header.vault = V16PodU128::new(0);
    header.insurance = V16PodU128::new(0);
    account_header.pnl = V16PodI128::new(-(residual as i128));
    let mut asset = markets[0].engine.asset.try_to_runtime().unwrap();
    asset.oi_eff_long_q = POS_SCALE;
    asset.oi_eff_short_q = POS_SCALE;
    asset.stored_pos_count_long = 1;
    asset.stored_pos_count_short = 1;
    asset.loss_weight_sum_long = SOCIAL_LOSS_DEN;
    asset.loss_weight_sum_short = SOCIAL_LOSS_DEN;
    markets[0].engine.asset = AssetStateV16Account::from_runtime(&asset);
    let vault_before = header.vault;
    let c_tot_before = header.c_tot;
    let insurance_before = header.insurance;

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    market.refresh_header_aggregate_totals_for_test().unwrap();
    let account = PortfolioV16ViewMut::new(&mut account_header);
    let result =
        market.kani_preflight_liquidation_residual_durability(0, SideV16::Long, &account.as_view());

    kani::cover!(
        residual > 2,
        "liquidation residual preflight proof covers nontrivial recovery residual"
    );
    assert_eq!(result, Err(V16Error::RecoveryRequired));
    assert_eq!(market.header.mode, 2);
    assert_eq!(
        market.header.recovery_reason.try_to_runtime().unwrap(),
        Some(PermissionlessRecoveryReasonV16::ActiveBankruptCloseCannotProgress)
    );
    assert_eq!(market.header.vault, vault_before);
    assert_eq!(market.header.c_tot, c_tot_before);
    assert_eq!(market.header.insurance, insurance_before);
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_view_fee_sync_settles_negative_pnl_before_fee() {
    let capital_raw: u8 = kani::any();
    let loss_raw: u8 = kani::any();
    let fee_rate_raw: u8 = kani::any();
    kani::assume((1..=100).contains(&capital_raw));
    kani::assume((1..=100).contains(&loss_raw));
    kani::assume((1..=100).contains(&fee_rate_raw));
    kani::assume(loss_raw < capital_raw);
    let capital = capital_raw as u128;
    let loss = loss_raw as u128;
    let fee_rate = fee_rate_raw as u128;
    let expected_fee = (capital - loss).min(fee_rate * 10);
    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    header.vault = V16PodU128::new(capital);
    header.c_tot = V16PodU128::new(capital);
    header.negative_pnl_account_count = V16PodU64::new(1);
    header.current_slot = V16PodU64::new(10);
    header.slot_last = V16PodU64::new(10);
    account_header.capital = V16PodU128::new(capital);
    account_header.pnl = V16PodI128::new(-(loss as i128));
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);

    let charged = market
        .sync_account_fee_to_slot_not_atomic(&mut account, 10, fee_rate)
        .unwrap();

    kani::cover!(
        loss > 1 && fee_rate > capital - loss && account.header.pnl.get() == 0,
        "view fee sync settles realized loss before capping fee to remaining capital"
    );
    assert_eq!(charged, expected_fee);
    assert_eq!(account.header.pnl.get(), 0);
    assert_eq!(account.header.capital.get(), capital - loss - expected_fee);
    assert_eq!(market.header.c_tot.get(), capital - loss - expected_fee);
    assert_eq!(market.header.insurance.get(), expected_fee);
    assert_eq!(market.header.vault.get(), capital);
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_loss_senior_fee_ordering_consumes_kf_loss_before_fee() {
    let capital_raw: u8 = kani::any();
    let hidden_loss_raw: u8 = kani::any();
    let requested_fee_raw: u8 = kani::any();
    kani::assume(hidden_loss_raw > 0);

    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    let capital = capital_raw as u128;
    let hidden_loss = hidden_loss_raw as u128;
    let requested_fee = requested_fee_raw as u128;
    header.vault = V16PodU128::new(capital);
    header.c_tot = V16PodU128::new(capital);
    account_header.capital = V16PodU128::new(capital);
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);

    market
        .kani_apply_signed_kf_delta_to_pnl(&mut account, -(hidden_loss as i128), None)
        .unwrap();
    let paid = market
        .kani_settle_negative_pnl_from_principal_core_not_atomic(&mut account)
        .unwrap();
    let charged = market
        .kani_charge_account_fee_current_not_atomic(&mut account, requested_fee)
        .unwrap();

    let expected_paid = capital.min(hidden_loss);
    let expected_pnl = if hidden_loss > capital {
        -((hidden_loss - capital) as i128)
    } else {
        0
    };
    let expected_fee = if expected_pnl < 0 {
        0
    } else {
        requested_fee.min(capital - expected_paid)
    };
    kani::cover!(
        capital > 10 && hidden_loss < capital && requested_fee > capital - hidden_loss,
        "loss-senior fee ordering covers wide fee capped after K/F loss"
    );
    kani::cover!(
        capital > 10 && hidden_loss > capital && requested_fee > 10,
        "loss-senior fee ordering covers wide no-fee bankrupt K/F loss"
    );
    assert_eq!(paid, expected_paid);
    assert_eq!(charged, expected_fee);
    assert_eq!(
        account.header.capital.get(),
        capital - expected_paid - expected_fee
    );
    assert_eq!(account.header.pnl.get(), expected_pnl);
    assert_eq!(
        market.header.c_tot.get(),
        capital - expected_paid - expected_fee
    );
    assert_eq!(market.header.insurance.get(), expected_fee);
    assert_eq!(market.header.vault.get(), capital);
    assert_eq!(
        market.header.c_tot.get() + market.header.insurance.get(),
        capital - expected_paid
    );
    if hidden_loss > capital {
        assert_eq!(expected_fee, 0);
        assert_eq!(market.header.bankruptcy_hlock_active, 1);
        assert_eq!(market.header.negative_pnl_account_count.get(), 1);
    } else {
        assert_eq!(account.header.pnl.get(), 0);
        assert_eq!(market.header.negative_pnl_account_count.get(), 0);
    }
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_view_domain_budget_caps_bankruptcy_insurance_spend() {
    let budget_raw: u8 = kani::any();
    let other_budget_raw: u8 = kani::any();
    let insurance_raw: u8 = kani::any();
    let loss_raw: u8 = kani::any();
    kani::assume(budget_raw <= 32);
    kani::assume(other_budget_raw <= 32);
    kani::assume(insurance_raw <= 32);
    kani::assume((1..=32).contains(&loss_raw));
    kani::assume((budget_raw as u16) + (other_budget_raw as u16) <= insurance_raw as u16);
    let budget = budget_raw as u128;
    let other_budget = other_budget_raw as u128;
    let insurance = insurance_raw as u128;
    let loss = loss_raw as u128;
    let expected_used = budget.min(loss);
    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    header.vault = V16PodU128::new(insurance);
    header.insurance = V16PodU128::new(insurance);
    header.negative_pnl_account_count = V16PodU64::new(1);
    markets[0].engine.insurance_domain_budget_short = V16PodU128::new(budget);
    markets[0].engine.insurance_domain_budget_long = V16PodU128::new(other_budget);
    account_header.pnl = V16PodI128::new(-(loss as i128));
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    market.refresh_header_aggregate_totals_for_test().unwrap();
    let remaining_before = market.header.insurance_domain_budget_remaining_total.get();
    let short_budget_before = market.markets[0].engine.insurance_domain_budget_short.get();
    let long_budget_before = market.markets[0].engine.insurance_domain_budget_long.get();
    let long_spent_before = market.markets[0].engine.insurance_domain_spent_long.get();
    let mut account = PortfolioV16ViewMut::new(&mut account_header);

    let used = market
        .kani_consume_domain_insurance_for_negative_pnl(0, SideV16::Long, &mut account)
        .unwrap();

    kani::cover!(budget == 0 && used == 0, "zero domain budget spend branch");
    kani::cover!(
        budget > 0 && budget < loss && used == budget,
        "domain budget spend proof covers budget-capped branch"
    );
    kani::cover!(
        loss < budget && used == loss,
        "domain budget spend proof covers loss-capped branch"
    );
    kani::cover!(
        other_budget > 0 && expected_used > 0,
        "domain budget spend proof covers unrelated funded domain isolation"
    );
    assert_eq!(used, expected_used);
    assert_eq!(market.header.insurance.get(), insurance - expected_used);
    assert_eq!(market.header.vault.get(), insurance);
    assert_eq!(market.header.c_tot.get(), 0);
    assert_eq!(
        market.markets[0].engine.insurance_domain_spent_short.get(),
        expected_used
    );
    assert_eq!(
        market.header.insurance_domain_budget_remaining_total.get(),
        remaining_before - expected_used
    );
    assert_eq!(
        market.markets[0].engine.insurance_domain_budget_short.get(),
        short_budget_before
    );
    assert_eq!(
        market.markets[0].engine.insurance_domain_budget_long.get(),
        long_budget_before
    );
    assert_eq!(
        market.markets[0].engine.insurance_domain_spent_long.get(),
        long_spent_before
    );
    assert_eq!(
        account.header.pnl.get(),
        -(loss as i128) + expected_used as i128
    );
    assert_eq!(market.header.bankruptcy_hlock_active, 1);
    assert_eq!(
        market.header.negative_pnl_account_count.get(),
        if expected_used == loss { 0 } else { 1 }
    );
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_reserved_domain_insurance_cannot_be_double_spent_by_bankruptcy() {
    let budget_raw: u8 = kani::any();
    let insurance_raw: u8 = kani::any();
    let loss_raw: u8 = kani::any();
    let reserved_raw: u8 = kani::any();
    kani::assume(budget_raw <= 32);
    kani::assume(insurance_raw <= 32);
    kani::assume((1..=32).contains(&loss_raw));
    kani::assume(reserved_raw <= 32);
    kani::assume(reserved_raw <= budget_raw);
    kani::assume(budget_raw <= insurance_raw);
    let budget = budget_raw as u128;
    let insurance = insurance_raw as u128;
    let loss = loss_raw as u128;
    let reserved = reserved_raw as u128;
    let unreserved_budget = budget - reserved;
    let expected_used = unreserved_budget.min(loss);
    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    header.vault = V16PodU128::new(insurance);
    header.insurance = V16PodU128::new(insurance);
    header.negative_pnl_account_count = V16PodU64::new(1);
    markets[0].engine.insurance_domain_budget_short = V16PodU128::new(budget);
    markets[0].engine.insurance_reservation_short =
        InsuranceCreditReservationV16Account::from_runtime(&InsuranceCreditReservationV16 {
            insurance_credit_reserved_num: reserved * BOUND_SCALE,
            ..InsuranceCreditReservationV16::EMPTY
        });
    markets[0].engine.source_credit_short =
        SourceCreditStateV16Account::from_runtime(&SourceCreditStateV16 {
            insurance_credit_reserved_num: reserved * BOUND_SCALE,
            credit_rate_num: CREDIT_RATE_SCALE,
            ..SourceCreditStateV16::EMPTY
        });
    account_header.pnl = V16PodI128::new(-(loss as i128));
    let reservation_before = markets[0].engine.insurance_reservation_short;
    let source_before = markets[0].engine.source_credit_short;

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    market.refresh_header_aggregate_totals_for_test().unwrap();
    let budget_total_before = market.header.insurance_domain_budget_remaining_total.get();
    let reserved_total_before = market
        .header
        .source_insurance_credit_reserved_total_atoms
        .get();
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    let used = market
        .kani_consume_domain_insurance_for_negative_pnl(0, SideV16::Long, &mut account)
        .unwrap();

    kani::cover!(
        reserved > 0,
        "reserved insurance proof covers nonzero encumbrance"
    );
    kani::cover!(
        reserved == budget && used == 0,
        "reserved insurance proof covers fully encumbered domain budget"
    );
    kani::cover!(
        reserved < budget && unreserved_budget < loss && used == unreserved_budget,
        "reserved insurance proof covers unreserved-budget-capped branch"
    );
    kani::cover!(
        loss < unreserved_budget && used == loss,
        "reserved insurance proof covers loss-capped branch"
    );
    assert_eq!(used, expected_used);
    assert_eq!(market.header.insurance.get(), insurance - expected_used);
    assert_eq!(market.header.vault.get(), insurance);
    assert_eq!(market.header.c_tot.get(), 0);
    assert_eq!(
        market.markets[0].engine.insurance_domain_spent_short.get(),
        expected_used
    );
    assert_eq!(
        market.header.insurance_domain_budget_remaining_total.get(),
        budget_total_before - expected_used
    );
    assert_eq!(
        market
            .header
            .source_insurance_credit_reserved_total_atoms
            .get(),
        reserved_total_before
    );
    assert_eq!(
        market.markets[0].engine.insurance_reservation_short,
        reservation_before
    );
    assert_eq!(market.markets[0].engine.source_credit_short, source_before);
    assert_eq!(
        account.header.pnl.get(),
        -(loss as i128) + expected_used as i128
    );
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_new_unfunded_domain_cannot_consume_shared_insurance() {
    let target_short_domain: bool = kani::any();
    let funded_budget_raw: u16 = kani::any();
    let funded_spent_raw: u16 = kani::any();
    let shared_slack_raw: u16 = kani::any();
    let residual_loss_raw: u8 = kani::any();
    kani::assume(funded_budget_raw <= 1024);
    kani::assume(funded_spent_raw <= funded_budget_raw);
    kani::assume(shared_slack_raw <= 1024);
    kani::assume(residual_loss_raw > 0);
    let funded_budget = funded_budget_raw as u128;
    let funded_spent = funded_spent_raw as u128;
    let funded_remaining = funded_budget - funded_spent;
    let shared_insurance = funded_remaining + shared_slack_raw as u128;
    let residual_loss = residual_loss_raw as u128;
    let bankrupt_side = if target_short_domain {
        SideV16::Long
    } else {
        SideV16::Short
    };

    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    header.vault = V16PodU128::new(shared_insurance);
    header.insurance = V16PodU128::new(shared_insurance);
    header.insurance_domain_budget_remaining_total = V16PodU128::new(funded_remaining);
    header.negative_pnl_account_count = V16PodU64::new(1);
    account_header.pnl = V16PodI128::new(-(residual_loss as i128));
    if target_short_domain {
        markets[0].engine.insurance_domain_budget_long = V16PodU128::new(funded_budget);
        markets[0].engine.insurance_domain_spent_long = V16PodU128::new(funded_spent);
        assert_eq!(markets[0].engine.insurance_domain_budget_short.get(), 0);
    } else {
        markets[0].engine.insurance_domain_budget_short = V16PodU128::new(funded_budget);
        markets[0].engine.insurance_domain_spent_short = V16PodU128::new(funded_spent);
        assert_eq!(markets[0].engine.insurance_domain_budget_long.get(), 0);
    }
    let budget_long_before = markets[0].engine.insurance_domain_budget_long;
    let spent_long_before = markets[0].engine.insurance_domain_spent_long;
    let budget_short_before = markets[0].engine.insurance_domain_budget_short;
    let spent_short_before = markets[0].engine.insurance_domain_spent_short;
    let remaining_total_before = header.insurance_domain_budget_remaining_total;

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    let used = market
        .kani_consume_domain_insurance_for_negative_pnl(0, bankrupt_side, &mut account)
        .unwrap();

    kani::cover!(
        target_short_domain && funded_remaining > 0 && shared_insurance >= residual_loss,
        "unfunded short domain cannot spend funded long budget despite sufficient shared insurance"
    );
    kani::cover!(
        !target_short_domain && funded_remaining > 0 && shared_insurance > residual_loss,
        "unfunded long domain cannot spend funded short budget despite excess shared insurance"
    );
    assert_eq!(used, 0);
    assert_eq!(market.header.insurance.get(), shared_insurance);
    assert_eq!(market.header.vault.get(), shared_insurance);
    assert_eq!(
        market.header.insurance_domain_budget_remaining_total,
        remaining_total_before
    );
    assert_eq!(
        market.markets[0].engine.insurance_domain_budget_long,
        budget_long_before
    );
    assert_eq!(
        market.markets[0].engine.insurance_domain_spent_long,
        spent_long_before
    );
    assert_eq!(
        market.markets[0].engine.insurance_domain_budget_short,
        budget_short_before
    );
    assert_eq!(
        market.markets[0].engine.insurance_domain_spent_short.get(),
        spent_short_before.get()
    );
    assert_eq!(account.header.pnl.get(), -(residual_loss as i128));
}

#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn proof_v16_credit_account_from_insurance_uses_only_unbudgeted_surplus() {
    let insurance: u128 = kani::any();
    let budgeted: u128 = kani::any();
    let c_tot: u128 = kani::any();
    let capital: u128 = kani::any();
    let amount: u128 = kani::any();
    kani::assume(budgeted <= insurance);
    kani::assume(c_tot <= u128::MAX - insurance);

    let result = MarketGroupV16ViewMut::<u64>::kani_credit_account_from_insurance_delta(
        insurance, budgeted, c_tot, capital, amount,
    );
    let next_c_tot_expected = c_tot.checked_add(amount);
    let next_capital_expected = capital.checked_add(amount);
    let expected_ok = amount <= insurance
        && budgeted <= insurance - amount
        && next_c_tot_expected.is_some()
        && next_capital_expected.is_some();

    kani::cover!(
        amount > 0 && expected_ok && budgeted < insurance,
        "credit-account-from-insurance delta covers nonzero unbudgeted surplus reward"
    );
    kani::cover!(
        amount > 0 && expected_ok && amount == insurance - budgeted,
        "credit-account-from-insurance delta covers exact unbudgeted surplus consumption"
    );
    kani::cover!(
        amount > 0 && !expected_ok && budgeted == insurance,
        "credit-account-from-insurance delta covers rejecting fully budgeted insurance"
    );
    assert_eq!(result.is_ok(), expected_ok);
    if let Ok((next_insurance, next_c_tot, next_capital)) = result {
        assert_eq!(next_insurance, insurance - amount);
        assert_eq!(next_c_tot, next_c_tot_expected.unwrap());
        assert_eq!(next_capital, next_capital_expected.unwrap());
        assert!(budgeted <= next_insurance);
        assert_eq!(next_insurance - budgeted, insurance - budgeted - amount);
        assert_eq!(
            next_insurance.checked_add(next_c_tot).unwrap(),
            insurance.checked_add(c_tot).unwrap(),
            "insurance-to-account credit preserves senior stock"
        );
    } else if amount > insurance {
        assert_eq!(result, Err(V16Error::CounterUnderflow));
    } else if budgeted > insurance - amount {
        assert_eq!(result, Err(V16Error::LockActive));
    } else {
        assert_eq!(result, Err(V16Error::ArithmeticOverflow));
    }
}

fn run_funding_target_sign_case(positive_funding: bool, units: i128) -> (i128, i128, i128) {
    let (mut header, mut markets, _) = one_market_view_fixture();
    if positive_funding {
        markets[0].engine.asset.f_long_num = V16PodI128::new(-(ADL_ONE as i128) * units);
        markets[0].engine.asset.f_short_num = V16PodI128::new((ADL_ONE as i128) * units);
    } else {
        markets[0].engine.asset.f_long_num = V16PodI128::new((ADL_ONE as i128) * units);
        markets[0].engine.asset.f_short_num = V16PodI128::new(-(ADL_ONE as i128) * units);
    }
    let leg = PortfolioLegV16 {
        active: true,
        asset_index: 0,
        market_id: 1,
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
    };
    let market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    market.kani_leg_kf_delta_for_settlement(leg).unwrap()
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_view_positive_funding_charges_long_side() {
    let units_raw: u16 = kani::any();
    kani::assume(units_raw > 0);
    kani::assume(units_raw <= 511);
    let units = units_raw as i128;
    let (k_now, f_now, net) = run_funding_target_sign_case(true, units);
    kani::cover!(
        units > 1 && k_now == 0 && f_now == -(ADL_ONE as i128) * units && net == -units,
        "positive funding charges long with symbolic funding magnitude"
    );
    assert_eq!(k_now, 0);
    assert_eq!(f_now, -(ADL_ONE as i128) * units);
    assert_eq!(net, -units);
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_view_negative_funding_pays_long_side() {
    let units_raw: u16 = kani::any();
    kani::assume(units_raw > 0);
    kani::assume(units_raw <= 511);
    let units = units_raw as i128;
    let (k_now, f_now, net) = run_funding_target_sign_case(false, units);
    kani::cover!(
        units > 1 && k_now == 0 && f_now == (ADL_ONE as i128) * units && net == units,
        "negative funding pays long with symbolic funding magnitude"
    );
    assert_eq!(k_now, 0);
    assert_eq!(f_now, (ADL_ONE as i128) * units);
    assert_eq!(net, units);
}

#[kani::proof]
#[kani::unwind(64)]
#[kani::solver(cadical)]
fn proof_v16_view_initial_margin_source_lien_creation_is_backed() {
    let effective_raw: u16 = kani::any();
    kani::assume(effective_raw > 0);
    let effective = effective_raw as u128;
    let backing_num = effective * BOUND_SCALE;
    let face_num = backing_num;
    // Symbolic slot (not welded to 0, which is the field default): otherwise the
    // final source_lien_fee_last_slot assertion is vacuous — 0 == default(0) holds
    // whether or not the engine runs the fee-clock initialization.
    let current_slot: u64 = kani::any();
    kani::assume(current_slot < 100); // stay under the fixture backing-bucket expiry (100)

    let source_credit = SourceCreditStateV16 {
        positive_claim_bound_num: face_num,
        exact_positive_claim_num: face_num,
        fresh_reserved_backing_num: backing_num,
        credit_rate_num: CREDIT_RATE_SCALE,
        ..SourceCreditStateV16::EMPTY
    };
    let backing_bucket = BackingBucketV16 {
        market_id: 1,
        fresh_unliened_backing_num: backing_num,
        expiry_slot: 100,
        status: BackingBucketStatusV16::Fresh,
        ..BackingBucketV16::EMPTY
    };
    let (backing_after, source_credit_after) =
        MarketGroupV16ViewMut::<u64>::kani_prepare_counterparty_lien_create_delta(
            backing_bucket,
            source_credit,
            current_slot,
            backing_num,
        )
        .unwrap();
    let mut source_domain = PortfolioSourceDomainV16Account::default();
    source_domain.source_claim_market_id = V16PodU64::new(1);
    source_domain.source_claim_bound_num = V16PodU128::new(face_num);
    MarketGroupV16ViewMut::<u64>::kani_apply_counterparty_source_credit_lien_delta(
        &mut source_domain,
        face_num,
        backing_num,
        effective,
        current_slot,
    )
    .unwrap();

    kani::cover!(effective > 0, "source-credit IM lien branch is reachable");
    kani::cover!(
        current_slot > 0,
        "fee-clock assertion is exercised on a nonzero slot (non-vacuous)"
    );
    assert_eq!(backing_after.fresh_unliened_backing_num, 0);
    assert_eq!(backing_after.valid_liened_backing_num, backing_num);
    assert_eq!(source_credit_after.valid_liened_backing_num, backing_num);
    assert_eq!(
        source_credit_after.fresh_reserved_backing_num,
        backing_after.valid_liened_backing_num
    );
    assert_eq!(source_domain.source_claim_liened_num.get(), face_num);
    assert_eq!(
        source_domain.source_lien_effective_reserved.get(),
        effective
    );
    assert_eq!(
        source_domain.source_claim_counterparty_liened_num.get(),
        face_num
    );
    assert_eq!(
        source_domain.source_lien_counterparty_backing_num.get(),
        backing_num
    );
    assert_eq!(source_domain.source_lien_fee_last_slot.get(), current_slot);
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_public_counterparty_lien_create_moves_fresh_to_valid_without_value_movement() {
    let amount_raw: u8 = kani::any();
    kani::assume((1..=5).contains(&amount_raw));
    let atoms = amount_raw as u128;
    let amount = atoms * BOUND_SCALE;
    let (mut header, mut markets) = one_market_only_fixture();
    let market_id = markets[0].engine.asset.market_id.get();
    header.vault = V16PodU128::new(atoms);
    header.source_fresh_backing_total_num = V16PodU128::new(amount);
    markets[0].engine.backing_long = BackingBucketV16Account::from_runtime(&BackingBucketV16 {
        market_id,
        fresh_unliened_backing_num: amount,
        expiry_slot: 10,
        status: BackingBucketStatusV16::Fresh,
        ..BackingBucketV16::EMPTY
    });
    markets[0].engine.source_credit_long =
        SourceCreditStateV16Account::from_runtime(&SourceCreditStateV16 {
            fresh_reserved_backing_num: amount,
            credit_rate_num: CREDIT_RATE_SCALE,
            ..SourceCreditStateV16::EMPTY
        });
    let vault_before = header.vault.get();
    let c_tot_before = header.c_tot.get();
    let insurance_before = header.insurance.get();
    let risk_epoch_before = header.risk_epoch.get();
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);

    market
        .create_source_credit_lien_from_counterparty_not_atomic(0, amount)
        .unwrap();
    let bucket = market.markets[0]
        .engine
        .backing_long
        .try_to_runtime()
        .unwrap();
    let source = market.markets[0]
        .engine
        .source_credit_long
        .try_to_runtime()
        .unwrap();

    kani::cover!(
        amount_raw > 1,
        "public counterparty lien create is nontrivial"
    );
    assert_eq!(market.header.vault.get(), vault_before);
    assert_eq!(market.header.c_tot.get(), c_tot_before);
    assert_eq!(market.header.insurance.get(), insurance_before);
    assert_eq!(market.header.risk_epoch.get(), risk_epoch_before + 1);
    assert_eq!(bucket.status, BackingBucketStatusV16::Fresh);
    assert_eq!(bucket.fresh_unliened_backing_num, 0);
    assert_eq!(bucket.valid_liened_backing_num, amount);
    assert_eq!(bucket.impaired_liened_backing_num, 0);
    assert_eq!(bucket.consumed_liened_backing_num, 0);
    assert_eq!(source.fresh_reserved_backing_num, amount);
    assert_eq!(source.valid_liened_backing_num, amount);
    assert_eq!(source.impaired_liened_backing_num, 0);
    assert_eq!(source.spent_backing_num, 0);
    assert_eq!(source.provider_receivable_num, 0);
    assert_eq!(source.credit_rate_num, CREDIT_RATE_SCALE);
    assert_eq!(source.credit_epoch, 1);
}

#[kani::proof]
#[kani::unwind(16)]
#[kani::solver(cadical)]
fn proof_v16_counterparty_lien_create_delta_is_expiry_gated_and_exact() {
    let amount_raw: u8 = kani::any();
    let fresh_raw: u8 = kani::any();
    let valid_raw: u8 = kani::any();
    let status_raw: u8 = kani::any();
    let expired: bool = kani::any();
    kani::assume(amount_raw <= 8);
    kani::assume(fresh_raw <= 8);
    kani::assume(valid_raw <= 8);
    kani::assume(status_raw <= 3);

    let current_slot = 10u64;
    let expiry_slot = if expired { current_slot } else { 20 };
    let amount = amount_raw as u128 * BOUND_SCALE;
    let fresh = fresh_raw as u128 * BOUND_SCALE;
    let valid = valid_raw as u128 * BOUND_SCALE;
    let status = match status_raw {
        0 => BackingBucketStatusV16::Fresh,
        1 => BackingBucketStatusV16::Expired,
        2 => BackingBucketStatusV16::Impaired,
        _ => BackingBucketStatusV16::Empty,
    };
    let bucket = BackingBucketV16 {
        market_id: 1,
        fresh_unliened_backing_num: fresh,
        valid_liened_backing_num: valid,
        expiry_slot,
        status,
        ..BackingBucketV16::EMPTY
    };
    let source = SourceCreditStateV16 {
        valid_liened_backing_num: valid,
        credit_rate_num: CREDIT_RATE_SCALE,
        ..SourceCreditStateV16::EMPTY
    };

    let result = MarketGroupV16ViewMut::<u64>::kani_prepare_counterparty_lien_create_delta(
        bucket,
        source,
        current_slot,
        amount,
    );
    let expected_ok =
        amount == 0 || (status == BackingBucketStatusV16::Fresh && !expired && fresh >= amount);

    kani::cover!(
        amount == 0 && status != BackingBucketStatusV16::Fresh,
        "counterparty lien create zero amount is an idempotent no-op before status checks"
    );
    kani::cover!(
        amount > 0 && status != BackingBucketStatusV16::Fresh,
        "counterparty lien create rejects non-Fresh buckets"
    );
    kani::cover!(
        amount > 0 && status == BackingBucketStatusV16::Fresh && expired,
        "counterparty lien create rejects expired Fresh buckets"
    );
    kani::cover!(
        amount > 0 && status == BackingBucketStatusV16::Fresh && !expired && fresh < amount,
        "counterparty lien create rejects insufficient fresh backing"
    );
    kani::cover!(
        expected_ok && amount > 0 && fresh > amount && valid > 0,
        "counterparty lien create partially moves fresh backing into an existing lien"
    );
    kani::cover!(
        expected_ok && amount > 0 && fresh == amount,
        "counterparty lien create can lien all currently fresh backing"
    );

    if expected_ok {
        let (next_bucket, next_source) = result.unwrap();
        assert_eq!(next_bucket.status, status);
        assert_eq!(next_bucket.expiry_slot, expiry_slot);
        assert_eq!(next_bucket.fresh_unliened_backing_num, fresh - amount);
        assert_eq!(next_bucket.valid_liened_backing_num, valid + amount);
        assert_eq!(next_source.valid_liened_backing_num, valid + amount);
    } else {
        assert_eq!(result, Err(V16Error::LockActive));
    }
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_public_counterparty_lien_release_restores_unliened_backing_without_value_movement() {
    let amount_raw: u8 = kani::any();
    kani::assume((1..=5).contains(&amount_raw));
    let amount = amount_raw as u128 * BOUND_SCALE;
    let (mut header, mut markets, _) = one_market_view_fixture();
    let market_id = markets[0].engine.asset.market_id.get();
    header.vault = V16PodU128::new(amount_raw as u128 * 2);
    header.source_fresh_backing_total_num = V16PodU128::new(amount * 2);
    markets[0].engine.backing_long = BackingBucketV16Account::from_runtime(&BackingBucketV16 {
        market_id,
        fresh_unliened_backing_num: amount,
        valid_liened_backing_num: amount,
        expiry_slot: 10,
        status: BackingBucketStatusV16::Fresh,
        ..BackingBucketV16::EMPTY
    });
    markets[0].engine.source_credit_long =
        SourceCreditStateV16Account::from_runtime(&SourceCreditStateV16 {
            fresh_reserved_backing_num: amount * 2,
            valid_liened_backing_num: amount,
            credit_rate_num: CREDIT_RATE_SCALE,
            ..SourceCreditStateV16::EMPTY
        });
    let vault_before = header.vault;
    let c_tot_before = header.c_tot;
    let insurance_before = header.insurance;
    let risk_epoch_before = header.risk_epoch.get();
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);

    market
        .release_source_credit_lien_from_counterparty_not_atomic(0, amount)
        .unwrap();
    let after_release_bucket = market.markets[0]
        .engine
        .backing_long
        .try_to_runtime()
        .unwrap();
    let after_release_source = market.markets[0]
        .engine
        .source_credit_long
        .try_to_runtime()
        .unwrap();

    kani::cover!(
        amount_raw > 1,
        "public counterparty lien release is nontrivial"
    );
    assert_eq!(after_release_bucket.status, BackingBucketStatusV16::Fresh);
    assert_eq!(after_release_bucket.fresh_unliened_backing_num, amount * 2);
    assert_eq!(after_release_bucket.valid_liened_backing_num, 0);
    assert_eq!(after_release_source.fresh_reserved_backing_num, amount * 2);
    assert_eq!(after_release_source.valid_liened_backing_num, 0);
    assert_eq!(market.header.vault, vault_before);
    assert_eq!(market.header.c_tot, c_tot_before);
    assert_eq!(market.header.insurance, insurance_before);
    assert!(market.header.risk_epoch.get() > risk_epoch_before);
    assert_eq!(market.validate_shape(), Ok(()));
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_public_counterparty_lien_consume_creates_receivable_without_value_movement() {
    let amount_raw: u8 = kani::any();
    kani::assume((1..=5).contains(&amount_raw));
    let amount = amount_raw as u128 * BOUND_SCALE;
    let (mut header, mut markets, _) = one_market_view_fixture();
    let market_id = markets[0].engine.asset.market_id.get();
    header.vault = V16PodU128::new(amount_raw as u128);
    header.source_fresh_backing_total_num = V16PodU128::new(amount);
    markets[0].engine.backing_long = BackingBucketV16Account::from_runtime(&BackingBucketV16 {
        market_id,
        valid_liened_backing_num: amount,
        expiry_slot: 10,
        status: BackingBucketStatusV16::Fresh,
        ..BackingBucketV16::EMPTY
    });
    markets[0].engine.source_credit_long =
        SourceCreditStateV16Account::from_runtime(&SourceCreditStateV16 {
            fresh_reserved_backing_num: amount,
            valid_liened_backing_num: amount,
            credit_rate_num: CREDIT_RATE_SCALE,
            ..SourceCreditStateV16::EMPTY
        });
    let vault_before = header.vault;
    let c_tot_before = header.c_tot;
    let insurance_before = header.insurance;
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);

    market
        .consume_source_credit_lien_from_counterparty_not_atomic(0, amount)
        .unwrap();
    let bucket = market.markets[0]
        .engine
        .backing_long
        .try_to_runtime()
        .unwrap();
    let source = market.markets[0]
        .engine
        .source_credit_long
        .try_to_runtime()
        .unwrap();

    kani::cover!(
        amount_raw > 1,
        "public counterparty lien consume is nontrivial"
    );
    assert_eq!(bucket.status, BackingBucketStatusV16::Expired);
    assert_eq!(bucket.fresh_unliened_backing_num, 0);
    assert_eq!(bucket.valid_liened_backing_num, 0);
    assert_eq!(bucket.consumed_liened_backing_num, amount);
    assert_eq!(source.fresh_reserved_backing_num, 0);
    assert_eq!(source.valid_liened_backing_num, 0);
    assert_eq!(source.spent_backing_num, amount);
    assert_eq!(source.provider_receivable_num, amount);
    assert_eq!(market.header.vault, vault_before);
    assert_eq!(market.header.c_tot, c_tot_before);
    assert_eq!(market.header.insurance, insurance_before);
    assert_eq!(market.validate_shape(), Ok(()));
}

#[kani::proof]
#[kani::unwind(16)]
#[kani::solver(cadical)]
fn proof_v16_counterparty_lien_consume_delta_is_receivable_exact_and_fail_closed() {
    let amount_raw: u8 = kani::any();
    let bucket_fresh_raw: u8 = kani::any();
    let bucket_valid_raw: u8 = kani::any();
    let bucket_consumed_raw: u8 = kani::any();
    let bucket_impaired_raw: u8 = kani::any();
    let source_fresh_raw: u8 = kani::any();
    let source_valid_raw: u8 = kani::any();
    let source_spent_raw: u8 = kani::any();
    let source_receivable_raw: u8 = kani::any();
    let status_raw: u8 = kani::any();
    kani::assume(amount_raw <= 8);
    kani::assume(bucket_fresh_raw <= 8);
    kani::assume(bucket_valid_raw <= 8);
    kani::assume(bucket_consumed_raw <= 8);
    kani::assume(bucket_impaired_raw <= 8);
    kani::assume(source_fresh_raw <= 8);
    kani::assume(source_valid_raw <= 8);
    kani::assume(source_spent_raw <= 8);
    kani::assume(source_receivable_raw <= 8);
    kani::assume(status_raw <= 3);

    let amount = amount_raw as u128 * BOUND_SCALE;
    let bucket_fresh = bucket_fresh_raw as u128 * BOUND_SCALE;
    let bucket_valid = bucket_valid_raw as u128 * BOUND_SCALE;
    let bucket_consumed = bucket_consumed_raw as u128 * BOUND_SCALE;
    let bucket_impaired = bucket_impaired_raw as u128 * BOUND_SCALE;
    let source_fresh = source_fresh_raw as u128 * BOUND_SCALE;
    let source_valid = source_valid_raw as u128 * BOUND_SCALE;
    let source_spent = source_spent_raw as u128 * BOUND_SCALE;
    let source_receivable = source_receivable_raw as u128 * BOUND_SCALE;
    let status = match status_raw {
        0 => BackingBucketStatusV16::Fresh,
        1 => BackingBucketStatusV16::Expired,
        2 => BackingBucketStatusV16::Impaired,
        _ => BackingBucketStatusV16::Empty,
    };
    let bucket = BackingBucketV16 {
        market_id: 1,
        fresh_unliened_backing_num: bucket_fresh,
        valid_liened_backing_num: bucket_valid,
        consumed_liened_backing_num: bucket_consumed,
        impaired_liened_backing_num: bucket_impaired,
        status,
        ..BackingBucketV16::EMPTY
    };
    let source = SourceCreditStateV16 {
        fresh_reserved_backing_num: source_fresh,
        valid_liened_backing_num: source_valid,
        spent_backing_num: source_spent,
        provider_receivable_num: source_receivable,
        credit_rate_num: CREDIT_RATE_SCALE,
        ..SourceCreditStateV16::EMPTY
    };

    let result = MarketGroupV16ViewMut::<u64>::kani_prepare_counterparty_lien_consume_delta(
        bucket, source, amount,
    );
    let expected_ok =
        amount == 0 || (bucket_valid >= amount && source_valid >= amount && source_fresh >= amount);

    kani::cover!(
        amount == 0 && bucket_valid == 0 && source_fresh == 0,
        "counterparty lien consume covers zero no-op before support checks"
    );
    kani::cover!(
        amount > 0 && bucket_valid < amount,
        "counterparty lien consume rejects insufficient bucket valid backing"
    );
    kani::cover!(
        amount > 0 && bucket_valid >= amount && source_valid < amount,
        "counterparty lien consume rejects insufficient source valid backing"
    );
    kani::cover!(
        amount > 0 && bucket_valid >= amount && source_valid >= amount && source_fresh < amount,
        "counterparty lien consume rejects insufficient source fresh reservation"
    );
    kani::cover!(
        expected_ok && amount > 0 && bucket_valid > amount && source_fresh > amount,
        "counterparty lien consume covers partial consumption with remaining lien"
    );
    kani::cover!(
        expected_ok
            && amount > 0
            && bucket_fresh == 0
            && bucket_valid == amount
            && bucket_impaired == 0,
        "counterparty lien consume covers terminal consumption status transition"
    );

    assert_eq!(result.is_ok(), expected_ok);
    if expected_ok {
        let (next_bucket, next_source) = result.unwrap();
        let c_tot: u128 = kani::any::<u8>() as u128;
        let insurance: u128 = kani::any::<u8>() as u128;
        let backing_provider_earnings: u128 = kani::any::<u8>() as u128;
        let junior_residual_before: u128 = kani::any::<u8>() as u128;
        let senior_backing_before = source_fresh / BOUND_SCALE;
        let senior_backing_after = next_source.fresh_reserved_backing_num / BOUND_SCALE;
        let vault = c_tot
            + insurance
            + backing_provider_earnings
            + senior_backing_before
            + junior_residual_before;
        let residual_before = vault
            .saturating_sub(c_tot + insurance + backing_provider_earnings + senior_backing_before);
        let residual_after = vault
            .saturating_sub(c_tot + insurance + backing_provider_earnings + senior_backing_after);
        let expected_status =
            if amount != 0 && bucket_fresh == 0 && bucket_valid == amount && bucket_impaired == 0 {
                BackingBucketStatusV16::Expired
            } else {
                status
            };
        assert_eq!(next_bucket.fresh_unliened_backing_num, bucket_fresh);
        assert_eq!(next_bucket.valid_liened_backing_num, bucket_valid - amount);
        assert_eq!(
            next_bucket.consumed_liened_backing_num,
            bucket_consumed + amount
        );
        assert_eq!(next_bucket.impaired_liened_backing_num, bucket_impaired);
        assert_eq!(next_bucket.status, expected_status);
        assert_eq!(
            next_source.fresh_reserved_backing_num,
            source_fresh - amount
        );
        assert_eq!(next_source.valid_liened_backing_num, source_valid - amount);
        assert_eq!(next_source.spent_backing_num, source_spent + amount);
        assert_eq!(
            next_source.provider_receivable_num,
            source_receivable + amount
        );
        assert_eq!(
            senior_backing_after + amount / BOUND_SCALE,
            senior_backing_before
        );
        assert_eq!(residual_after, residual_before + amount / BOUND_SCALE);
        kani::cover!(
            amount > 0 && junior_residual_before > 0 && senior_backing_after > 0,
            "counterparty lien consume reclassifies part of senior backing into junior residual"
        );
    } else {
        assert_eq!(result, Err(V16Error::CounterUnderflow));
    }
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_public_counterparty_lien_impair_moves_valid_to_impaired_without_value_movement() {
    let amount_raw: u8 = kani::any();
    kani::assume((1..=5).contains(&amount_raw));
    let atoms = amount_raw as u128;
    let amount = atoms * BOUND_SCALE;
    let (mut header, mut markets) = one_market_only_fixture();
    let market_id = markets[0].engine.asset.market_id.get();
    header.vault = V16PodU128::new(atoms);
    header.source_fresh_backing_total_num = V16PodU128::new(amount);
    markets[0].engine.backing_long = BackingBucketV16Account::from_runtime(&BackingBucketV16 {
        market_id,
        valid_liened_backing_num: amount,
        expiry_slot: 10,
        status: BackingBucketStatusV16::Fresh,
        ..BackingBucketV16::EMPTY
    });
    markets[0].engine.source_credit_long =
        SourceCreditStateV16Account::from_runtime(&SourceCreditStateV16 {
            fresh_reserved_backing_num: amount,
            valid_liened_backing_num: amount,
            credit_rate_num: CREDIT_RATE_SCALE,
            ..SourceCreditStateV16::EMPTY
        });
    let vault_before = header.vault.get();
    let c_tot_before = header.c_tot.get();
    let insurance_before = header.insurance.get();
    let risk_epoch_before = header.risk_epoch.get();
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let residual_before = market.kani_residual();

    market
        .impair_source_credit_lien_from_counterparty_not_atomic(0, amount)
        .unwrap();
    let bucket = market.markets[0]
        .engine
        .backing_long
        .try_to_runtime()
        .unwrap();
    let source = market.markets[0]
        .engine
        .source_credit_long
        .try_to_runtime()
        .unwrap();

    kani::cover!(
        amount_raw > 1,
        "public counterparty lien impair is nontrivial"
    );
    assert_eq!(market.header.vault.get(), vault_before);
    assert_eq!(market.header.c_tot.get(), c_tot_before);
    assert_eq!(market.header.insurance.get(), insurance_before);
    assert_eq!(market.header.risk_epoch.get(), risk_epoch_before + 1);
    assert_eq!(bucket.status, BackingBucketStatusV16::Impaired);
    assert_eq!(bucket.fresh_unliened_backing_num, 0);
    assert_eq!(bucket.valid_liened_backing_num, 0);
    assert_eq!(bucket.impaired_liened_backing_num, amount);
    assert_eq!(bucket.consumed_liened_backing_num, 0);
    assert_eq!(source.fresh_reserved_backing_num, 0);
    assert_eq!(source.valid_liened_backing_num, 0);
    assert_eq!(source.impaired_liened_backing_num, amount);
    assert_eq!(source.spent_backing_num, 0);
    assert_eq!(source.provider_receivable_num, 0);
    assert_eq!(source.credit_rate_num, CREDIT_RATE_SCALE);
    assert_eq!(source.credit_epoch, 1);
    // Stock destination of the impaired atoms (review finding): impairment is
    // NOT an orphaning — the principal leaves counterparty_backing_principal
    // (fresh_reserved drops) and lands, atom for atom, in the JUNIOR RESIDUAL
    // pool with the vault flat. No class disappears; the junior pool rises.
    assert_eq!(market.kani_residual(), residual_before + atoms);
}

#[kani::proof]
#[kani::unwind(24)]
#[kani::solver(cadical)]
fn proof_v16_insurance_lien_consume_spends_only_its_domain_budget() {
    // CLEAN-ROOM FIX: master drove kani_apply_insurance_lien_consume_domain_delta,
    // a cfg(kani)/fuzz-only MODEL wrapper that OMITTED the
    // reservation_encumbrance_proof_for_domain_parts(...).validate()? and the exit
    // validate_shape() the engine path consume_source_credit_lien_from_insurance_
    // not_atomic runs (v16.rs) -- so it certified a path the engine never executes,
    // and its single saturated domain never tested the named ISOLATION property.
    // Fix: drive the REAL engine path and prove DOMAIN ISOLATION -- domain 0 (Long)
    // is the consume target; the sibling domain 1 (Short) on the same asset carries
    // its own budget/reservation that MUST survive untouched. The model wrapper is
    // deleted; the single-domain mutation is independently covered by
    // proof_v16_public_insurance_lien_consume_debits_only_domain_insurance.
    let long_raw: u8 = kani::any();
    let short_raw: u8 = kani::any();
    kani::assume((1..=8).contains(&long_raw));
    kani::assume((1..=8).contains(&short_raw));
    let atoms_long = long_raw as u128;
    let atoms_short = short_raw as u128;
    let amount_long = atoms_long * BOUND_SCALE;
    let amount_short = atoms_short * BOUND_SCALE;
    let total_atoms = atoms_long + atoms_short;

    let (mut header, mut markets) = one_market_only_fixture();
    // de-saturated pool: the insurance pool funds BOTH domains' reservations, so
    // consuming the Long lien is not an all-or-nothing drain of the whole pool.
    header.vault = V16PodU128::new(total_atoms);
    header.insurance = V16PodU128::new(total_atoms);
    header.source_insurance_credit_reserved_total_atoms = V16PodU128::new(total_atoms);
    header.insurance_domain_budget_remaining_total = V16PodU128::new(total_atoms);
    // domain 0 = Long: the consume target.
    markets[0].engine.insurance_domain_budget_long = V16PodU128::new(atoms_long);
    markets[0].engine.insurance_reservation_long =
        InsuranceCreditReservationV16Account::from_runtime(&InsuranceCreditReservationV16 {
            insurance_credit_reserved_num: amount_long,
            valid_liened_insurance_num: amount_long,
            ..InsuranceCreditReservationV16::EMPTY
        });
    markets[0].engine.source_credit_long =
        SourceCreditStateV16Account::from_runtime(&SourceCreditStateV16 {
            insurance_credit_reserved_num: amount_long,
            valid_liened_insurance_num: amount_long,
            credit_rate_num: CREDIT_RATE_SCALE,
            ..SourceCreditStateV16::EMPTY
        });
    // domain 1 = Short: the sibling whose budget/reservation must SURVIVE.
    markets[0].engine.insurance_domain_budget_short = V16PodU128::new(atoms_short);
    markets[0].engine.insurance_reservation_short =
        InsuranceCreditReservationV16Account::from_runtime(&InsuranceCreditReservationV16 {
            insurance_credit_reserved_num: amount_short,
            valid_liened_insurance_num: amount_short,
            ..InsuranceCreditReservationV16::EMPTY
        });
    markets[0].engine.source_credit_short =
        SourceCreditStateV16Account::from_runtime(&SourceCreditStateV16 {
            insurance_credit_reserved_num: amount_short,
            valid_liened_insurance_num: amount_short,
            credit_rate_num: CREDIT_RATE_SCALE,
            ..SourceCreditStateV16::EMPTY
        });

    let vault_before = header.vault.get();
    let c_tot_before = header.c_tot.get();
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);

    // sibling (Short / domain 1) snapshot BEFORE the consume
    let short_budget_before = market.markets[0].engine.insurance_domain_budget_short.get();
    let short_spent_before = market.markets[0].engine.insurance_domain_spent_short.get();
    let short_res_before = market.markets[0]
        .engine
        .insurance_reservation_short
        .try_to_runtime()
        .unwrap();
    let short_src_before = market.markets[0]
        .engine
        .source_credit_short
        .try_to_runtime()
        .unwrap();

    // REAL engine path (runs the encumbrance-proof validate + exit validate_shape).
    market
        .consume_source_credit_lien_from_insurance_not_atomic(0, amount_long)
        .unwrap();

    let reservation = market.markets[0]
        .engine
        .insurance_reservation_long
        .try_to_runtime()
        .unwrap();
    let source = market.markets[0]
        .engine
        .source_credit_long
        .try_to_runtime()
        .unwrap();
    let short_res_after = market.markets[0]
        .engine
        .insurance_reservation_short
        .try_to_runtime()
        .unwrap();
    let short_src_after = market.markets[0]
        .engine
        .source_credit_short
        .try_to_runtime()
        .unwrap();

    kani::cover!(
        atoms_long > 1 && atoms_short > 1,
        "nontrivial Long consume with a live sibling Short domain"
    );

    // domain 0 (Long) lien is consumed in full ...
    assert_eq!(reservation.consumed_insurance_num, amount_long);
    assert_eq!(reservation.valid_liened_insurance_num, 0);
    assert_eq!(reservation.insurance_credit_reserved_num, 0);
    assert_eq!(source.valid_liened_insurance_num, 0);
    assert_eq!(source.insurance_credit_reserved_num, 0);
    assert_eq!(
        market.markets[0].engine.insurance_domain_spent_long.get(),
        atoms_long
    );
    assert_eq!(
        market.markets[0].engine.insurance_domain_budget_long.get(),
        atoms_long
    );

    // ... and the sibling Short domain (domain 1) is UNTOUCHED -- the named
    // isolation property: consuming one domain spends ONLY its own budget.
    assert_eq!(
        market.markets[0].engine.insurance_domain_budget_short.get(),
        short_budget_before
    );
    assert_eq!(
        market.markets[0].engine.insurance_domain_spent_short.get(),
        short_spent_before
    );
    assert_eq!(short_res_after, short_res_before);
    assert_eq!(
        short_src_after.valid_liened_insurance_num,
        short_src_before.valid_liened_insurance_num
    );
    assert_eq!(
        short_src_after.insurance_credit_reserved_num,
        short_src_before.insurance_credit_reserved_num
    );

    // the insurance pool falls by EXACTLY the Long spend; the Short reservation
    // (atoms_short) still stands, so the pool is not zeroed.
    assert_eq!(market.header.insurance.get(), atoms_short);
    assert_eq!(
        market.header.insurance_domain_budget_remaining_total.get(),
        atoms_short
    );
    assert_eq!(market.header.vault.get(), vault_before);
    assert_eq!(market.header.c_tot.get(), c_tot_before);
}

#[kani::proof]
#[kani::unwind(16)]
#[kani::solver(cadical)]
fn proof_v16_insurance_lien_consume_delta_is_aligned_and_atom_exact() {
    let amount_units_raw: u8 = kani::any();
    let reservation_reserved_raw: u8 = kani::any();
    let reservation_valid_raw: u8 = kani::any();
    let source_reserved_raw: u8 = kani::any();
    let source_valid_raw: u8 = kani::any();
    let domain_spent_raw: u8 = kani::any();
    let insurance_raw: u8 = kani::any();
    let force_unaligned: bool = kani::any();
    kani::assume(amount_units_raw <= 8);
    kani::assume(reservation_reserved_raw <= 8);
    kani::assume(reservation_valid_raw <= 8);
    kani::assume(source_reserved_raw <= 8);
    kani::assume(source_valid_raw <= 8);
    kani::assume(domain_spent_raw <= 8);
    kani::assume(insurance_raw <= 8);

    let aligned_amount = amount_units_raw as u128 * BOUND_SCALE;
    let amount = if force_unaligned {
        aligned_amount + 1
    } else {
        aligned_amount
    };
    let reservation_reserved = reservation_reserved_raw as u128 * BOUND_SCALE;
    let reservation_valid = reservation_valid_raw as u128 * BOUND_SCALE;
    let source_reserved = source_reserved_raw as u128 * BOUND_SCALE;
    let source_valid = source_valid_raw as u128 * BOUND_SCALE;
    let domain_spent = domain_spent_raw as u128;
    let insurance = insurance_raw as u128;
    let spend_atoms = amount_units_raw as u128;
    let reservation = InsuranceCreditReservationV16 {
        insurance_credit_reserved_num: reservation_reserved,
        valid_liened_insurance_num: reservation_valid,
        ..InsuranceCreditReservationV16::EMPTY
    };
    let source = SourceCreditStateV16 {
        insurance_credit_reserved_num: source_reserved,
        valid_liened_insurance_num: source_valid,
        credit_rate_num: CREDIT_RATE_SCALE,
        ..SourceCreditStateV16::EMPTY
    };

    let result = MarketGroupV16ViewMut::<u64>::kani_prepare_insurance_lien_consume_delta(
        reservation,
        source,
        domain_spent,
        insurance,
        amount,
    );
    let expected_ok = amount == 0
        || (!force_unaligned
            && reservation_valid >= amount
            && reservation_reserved >= amount
            && source_valid >= amount
            && source_reserved >= amount
            && insurance >= spend_atoms);

    kani::cover!(amount == 0, "insurance consume delta covers zero no-op");
    kani::cover!(
        amount > 0 && force_unaligned,
        "insurance consume delta rejects non-atom-aligned bound amount"
    );
    kani::cover!(
        !force_unaligned && amount > 0 && reservation_valid < amount,
        "insurance consume delta rejects insufficient reservation valid lien"
    );
    kani::cover!(
        !force_unaligned && amount > 0 && source_reserved < amount,
        "insurance consume delta rejects insufficient source reservation"
    );
    kani::cover!(
        !force_unaligned
            && amount > 0
            && reservation_valid >= amount
            && reservation_reserved >= amount
            && source_valid >= amount
            && source_reserved >= amount
            && insurance < spend_atoms,
        "insurance consume delta rejects insufficient senior insurance atoms"
    );
    kani::cover!(
        expected_ok && amount > 0 && reservation_reserved > amount && source_valid > amount,
        "insurance consume delta covers partial aligned consume"
    );

    assert_eq!(result.is_ok(), expected_ok);
    if expected_ok {
        let (next_reservation, next_source, next_domain_spent, next_insurance) = result.unwrap();
        assert_eq!(
            next_reservation.valid_liened_insurance_num,
            reservation_valid - amount
        );
        assert_eq!(
            next_reservation.insurance_credit_reserved_num,
            reservation_reserved - amount
        );
        assert_eq!(next_reservation.consumed_insurance_num, amount);
        assert_eq!(
            next_source.valid_liened_insurance_num,
            source_valid - amount
        );
        assert_eq!(
            next_source.insurance_credit_reserved_num,
            source_reserved - amount
        );
        assert_eq!(next_domain_spent, domain_spent + spend_atoms);
        assert_eq!(next_insurance, insurance - spend_atoms);
    } else if force_unaligned && amount > 0 {
        assert_eq!(result, Err(V16Error::InvalidConfig));
    } else {
        assert_eq!(result, Err(V16Error::CounterUnderflow));
    }
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_public_insurance_lien_consume_debits_only_domain_insurance() {
    let atom_raw: u8 = kani::any();
    kani::assume((1..=8).contains(&atom_raw));
    let atoms = atom_raw as u128;
    let amount = atoms * BOUND_SCALE;
    let (mut header, mut markets) = one_market_only_fixture();
    header.vault = V16PodU128::new(atoms);
    header.insurance = V16PodU128::new(atoms);
    header.source_insurance_credit_reserved_total_atoms = V16PodU128::new(atoms);
    header.insurance_domain_budget_remaining_total = V16PodU128::new(atoms);
    markets[0].engine.insurance_domain_budget_long = V16PodU128::new(atoms);
    markets[0].engine.insurance_reservation_long =
        InsuranceCreditReservationV16Account::from_runtime(&InsuranceCreditReservationV16 {
            insurance_credit_reserved_num: amount,
            valid_liened_insurance_num: amount,
            ..InsuranceCreditReservationV16::EMPTY
        });
    markets[0].engine.source_credit_long =
        SourceCreditStateV16Account::from_runtime(&SourceCreditStateV16 {
            insurance_credit_reserved_num: amount,
            valid_liened_insurance_num: amount,
            credit_rate_num: CREDIT_RATE_SCALE,
            ..SourceCreditStateV16::EMPTY
        });
    let vault_before = header.vault.get();
    let c_tot_before = header.c_tot.get();
    let risk_epoch_before = header.risk_epoch.get();
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);

    market
        .consume_source_credit_lien_from_insurance_not_atomic(0, amount)
        .unwrap();
    let reservation = market.markets[0]
        .engine
        .insurance_reservation_long
        .try_to_runtime()
        .unwrap();
    let source = market.markets[0]
        .engine
        .source_credit_long
        .try_to_runtime()
        .unwrap();

    kani::cover!(
        atom_raw > 1,
        "public insurance lien consume spends a nontrivial domain amount"
    );
    assert_eq!(market.header.vault.get(), vault_before);
    assert_eq!(market.header.c_tot.get(), c_tot_before);
    assert_eq!(market.header.insurance.get(), 0);
    assert_eq!(market.header.risk_epoch.get(), risk_epoch_before + 1);
    assert_eq!(
        market
            .header
            .source_insurance_credit_reserved_total_atoms
            .get(),
        0
    );
    assert_eq!(
        market.header.insurance_domain_budget_remaining_total.get(),
        0
    );
    assert_eq!(
        market.markets[0].engine.insurance_domain_budget_long.get(),
        atoms
    );
    assert_eq!(
        market.markets[0].engine.insurance_domain_spent_long.get(),
        atoms
    );
    assert_eq!(reservation.insurance_credit_reserved_num, 0);
    assert_eq!(reservation.valid_liened_insurance_num, 0);
    assert_eq!(reservation.impaired_liened_insurance_num, 0);
    assert_eq!(reservation.consumed_insurance_num, amount);
    assert_eq!(source.insurance_credit_reserved_num, 0);
    assert_eq!(source.valid_liened_insurance_num, 0);
    assert_eq!(source.impaired_liened_insurance_num, 0);
    assert_eq!(source.credit_rate_num, CREDIT_RATE_SCALE);
    assert_eq!(source.credit_epoch, 1);
}

#[kani::proof]
#[kani::unwind(32)]
#[kani::solver(cadical)]
fn proof_v16_public_insurance_reserve_rejects_unfunded_domain() {
    let amount_raw: u8 = kani::any();
    let c_tot_raw: u8 = kani::any();
    let insurance_raw: u8 = kani::any();
    let surplus_raw: u8 = kani::any();
    kani::assume(amount_raw > 0);
    kani::assume(c_tot_raw <= 8);
    kani::assume(insurance_raw <= 8);
    kani::assume(surplus_raw <= 8);
    let amount = amount_raw as u128 * BOUND_SCALE;
    let c_tot = c_tot_raw as u128;
    let insurance = insurance_raw as u128;
    let surplus = surplus_raw as u128;
    let (mut header, mut markets, _) = one_market_view_fixture();
    header.vault = V16PodU128::new(c_tot + insurance + surplus);
    header.c_tot = V16PodU128::new(c_tot);
    header.insurance = V16PodU128::new(insurance);
    let vault_before = header.vault;
    let c_tot_before = header.c_tot;
    let insurance_before = header.insurance;
    let budget_total_before = header.insurance_domain_budget_remaining_total;
    let long_budget_before = markets[0].engine.insurance_domain_budget_long;
    let short_budget_before = markets[0].engine.insurance_domain_budget_short;
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);

    let result = market.reserve_insurance_credit_not_atomic(0, amount);

    kani::cover!(
        result == Err(V16Error::LockActive) && insurance > 0 && surplus > 0,
        "unfunded domain insurance reservation reaches isolation guard despite global insurance"
    );
    assert_eq!(result, Err(V16Error::LockActive));
    assert_eq!(market.header.vault, vault_before);
    assert_eq!(market.header.c_tot, c_tot_before);
    assert_eq!(market.header.insurance, insurance_before);
    assert_eq!(
        market.header.insurance_domain_budget_remaining_total,
        budget_total_before
    );
    assert_eq!(
        market.markets[0].engine.insurance_domain_budget_long,
        long_budget_before
    );
    assert_eq!(
        market.markets[0].engine.insurance_domain_budget_short,
        short_budget_before
    );
}

#[kani::proof]
#[kani::unwind(24)]
#[kani::solver(cadical)]
fn proof_v16_domain_insurance_deposit_updates_o1_remaining_total() {
    let budget_raw: u8 = kani::any();
    kani::assume(budget_raw > 0);
    let budget = budget_raw as u128;
    let (mut header, mut markets, _) = one_market_view_fixture();
    let vault_before = header.vault.get();
    let c_tot_before = header.c_tot.get();
    let insurance_before = header.insurance.get();
    let remaining_before = header.insurance_domain_budget_remaining_total.get();
    let long_budget_before = markets[0].engine.insurance_domain_budget_long.get();
    let short_budget_before = markets[0].engine.insurance_domain_budget_short.get();
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let residual_before = market.kani_residual();

    market
        .deposit_domain_insurance_not_atomic(0, budget)
        .unwrap();

    kani::cover!(
        budget > 1,
        "domain insurance deposit covers nontrivial budget"
    );
    assert_eq!(market.header.vault.get(), vault_before + budget);
    assert_eq!(market.header.insurance.get(), insurance_before + budget);
    assert_eq!(market.header.c_tot.get(), c_tot_before);
    assert_eq!(
        market.header.insurance_domain_budget_remaining_total.get(),
        remaining_before + budget
    );
    assert_eq!(
        market.markets[0].engine.insurance_domain_budget_long.get(),
        long_budget_before + budget
    );
    assert_eq!(
        market.markets[0].engine.insurance_domain_budget_short.get(),
        short_budget_before
    );
    assert_eq!(
        market.header.vault.get() - vault_before,
        market.header.insurance.get() - insurance_before
    );
    assert_eq!(
        market.header.insurance_domain_budget_remaining_total.get() - remaining_before,
        market.markets[0].engine.insurance_domain_budget_long.get() - long_budget_before
    );
    // Junior-pool isolation: insurance deposits move vault and insurance in
    // lockstep, so the junior residual pool cannot be inflated through them.
    assert_eq!(market.kani_residual(), residual_before);
    assert_eq!(market.validate_shape(), Ok(()));
}

#[kani::proof]
#[kani::unwind(24)]
#[kani::solver(cadical)]
fn proof_v16_public_credit_domain_insurance_budget_is_value_neutral_and_backed() {
    let amount_raw: u8 = kani::any();
    let existing_raw: u16 = kani::any();
    kani::assume(amount_raw > 0);
    let amount = amount_raw as u128;
    let existing_budget = existing_raw as u128;
    let (mut header, mut markets) = one_market_only_fixture();
    header.vault = V16PodU128::new(existing_budget + amount);
    header.insurance = V16PodU128::new(existing_budget + amount);
    header.insurance_domain_budget_remaining_total = V16PodU128::new(existing_budget);
    markets[0].engine.insurance_domain_budget_long = V16PodU128::new(existing_budget);
    let vault_before = header.vault.get();
    let c_tot_before = header.c_tot.get();
    let insurance_before = header.insurance.get();
    let remaining_before = header.insurance_domain_budget_remaining_total.get();
    let long_budget_before = markets[0].engine.insurance_domain_budget_long.get();
    let short_budget_before = markets[0].engine.insurance_domain_budget_short.get();
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let residual_before = market.kani_residual();

    market
        .credit_domain_insurance_budget_not_atomic(0, amount)
        .unwrap();

    kani::cover!(
        amount > 1,
        "public domain budget credit covers nontrivial already-collected fee"
    );
    kani::cover!(
        existing_budget > 255 && amount > 1,
        "public domain budget credit covers wide additive existing-budget branch"
    );
    assert_eq!(market.header.vault.get(), vault_before);
    assert_eq!(market.header.c_tot.get(), c_tot_before);
    assert_eq!(market.header.insurance.get(), insurance_before);
    assert_eq!(
        market.header.insurance_domain_budget_remaining_total.get(),
        remaining_before + amount
    );
    assert_eq!(
        market.markets[0].engine.insurance_domain_budget_long.get(),
        long_budget_before + amount
    );
    assert_eq!(
        market.markets[0].engine.insurance_domain_budget_short.get(),
        short_budget_before
    );
    assert_eq!(
        market.header.insurance.get(),
        market.header.insurance_domain_budget_remaining_total.get()
    );
    assert_eq!(
        market.header.insurance_domain_budget_remaining_total.get() - remaining_before,
        market.markets[0].engine.insurance_domain_budget_long.get() - long_budget_before
    );
    assert_eq!(market.validate_shape(), Ok(()));
    // Junior-pool isolation: budget crediting relabels insurance capacity
    // only; the junior residual pool is untouched.
    assert_eq!(market.kani_residual(), residual_before);
}

#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn proof_v16_domain_insurance_withdraw_delta_is_budget_scoped_and_value_conserving() {
    let selected_budget: u128 = kani::any();
    let spent: u128 = kani::any();
    let domain_reserved: u128 = kani::any();
    let source_reserved: u128 = kani::any();
    let global_available: u128 = kani::any();
    let vault: u128 = kani::any();
    let withdraw: u128 = kani::any();
    kani::assume(selected_budget > 0);
    kani::assume(spent <= selected_budget);
    kani::assume(domain_reserved <= selected_budget - spent);
    kani::assume(source_reserved <= u128::MAX - global_available);
    let selected_available = selected_budget - spent - domain_reserved;
    let insurance = source_reserved + global_available;
    let result = MarketGroupV16ViewMut::<u64>::kani_withdraw_domain_insurance_delta(
        vault,
        insurance,
        source_reserved,
        selected_budget,
        spent,
        domain_reserved,
        withdraw,
    );
    let expected_ok = withdraw <= selected_available.min(global_available).min(vault);

    kani::cover!(
        withdraw > 1 && expected_ok && selected_available <= global_available,
        "domain insurance withdraw delta covers nontrivial domain-scoped success"
    );
    kani::cover!(
        withdraw <= selected_available && withdraw > global_available,
        "domain insurance withdraw delta rejects globally reserved insurance"
    );
    kani::cover!(
        withdraw <= global_available && withdraw > selected_available,
        "domain insurance withdraw delta rejects selected-domain overdraw"
    );
    kani::cover!(
        withdraw <= selected_available && withdraw <= global_available && withdraw > vault,
        "domain insurance withdraw delta rejects vault-liquidity overdraw"
    );
    assert_eq!(result.is_ok(), expected_ok);
    if let Ok((next_vault, next_insurance, next_budget)) = result {
        assert_eq!(next_vault, vault - withdraw);
        assert_eq!(next_insurance, insurance - withdraw);
        assert_eq!(next_budget, selected_budget - withdraw);
        assert_eq!(vault - next_vault, insurance - next_insurance);
        assert_eq!(vault - next_vault, selected_budget - next_budget);
        assert_eq!(
            next_insurance - source_reserved,
            global_available - withdraw
        );
        assert_eq!(
            next_budget - spent - domain_reserved,
            selected_available - withdraw
        );
        if selected_available <= global_available {
            assert!(next_budget - spent - domain_reserved <= next_insurance - source_reserved);
        }
    } else {
        assert!(withdraw > selected_available || withdraw > global_available || withdraw > vault);
        assert_eq!(result, Err(V16Error::LockActive));
    }
}

#[kani::proof]
#[kani::unwind(24)]
#[kani::solver(cadical)]
fn proof_v16_public_domain_insurance_withdraw_capacity_matches_budget_reserved_and_vault_min() {
    let budget_raw: u8 = kani::any();
    let spent_raw: u8 = kani::any();
    let domain_reserved_raw: u8 = kani::any();
    let other_reserved_raw: u8 = kani::any();
    let global_available_raw: u8 = kani::any();
    let vault_raw: u8 = kani::any();
    kani::assume(budget_raw > 0);
    kani::assume(spent_raw <= budget_raw);
    kani::assume(domain_reserved_raw <= budget_raw - spent_raw);
    let budget = budget_raw as u128;
    let spent = spent_raw as u128;
    let domain_reserved = domain_reserved_raw as u128;
    let other_reserved = other_reserved_raw as u128;
    let global_available = global_available_raw as u128;
    let vault = vault_raw as u128;
    let insurance = domain_reserved + other_reserved + global_available;
    let (mut header, mut markets) = one_market_only_fixture();
    header.vault = V16PodU128::new(vault);
    header.insurance = V16PodU128::new(insurance);
    header.source_insurance_credit_reserved_total_atoms =
        V16PodU128::new(domain_reserved + other_reserved);
    header.insurance_domain_budget_remaining_total = V16PodU128::new(budget - spent);
    markets[0].engine.insurance_domain_budget_long = V16PodU128::new(budget);
    markets[0].engine.insurance_domain_spent_long = V16PodU128::new(spent);
    markets[0].engine.insurance_reservation_long =
        InsuranceCreditReservationV16Account::from_runtime(&InsuranceCreditReservationV16 {
            insurance_credit_reserved_num: domain_reserved * BOUND_SCALE,
            ..InsuranceCreditReservationV16::EMPTY
        });
    let market = MarketGroupV16ViewMut::new(&mut header, &mut markets);

    let capacity = market.domain_insurance_withdraw_capacity(0).unwrap();
    let remaining = market.domain_insurance_budget_remaining(0).unwrap();
    let selected_available = budget - spent - domain_reserved;
    let expected_capacity = selected_available.min(global_available).min(vault);

    kani::cover!(
        expected_capacity > 0
            && selected_available <= global_available
            && selected_available <= vault,
        "domain withdraw capacity covers selected-domain budget as binding constraint"
    );
    kani::cover!(
        expected_capacity > 0 && global_available < selected_available && global_available <= vault,
        "domain withdraw capacity covers globally reserved insurance as binding constraint"
    );
    kani::cover!(
        expected_capacity > 0 && vault < selected_available && vault < global_available,
        "domain withdraw capacity covers vault liquidity as binding constraint"
    );
    kani::cover!(
        expected_capacity == 0 && (selected_available == 0 || global_available == 0 || vault == 0),
        "domain withdraw capacity covers zero-capacity boundary"
    );
    assert_eq!(remaining, budget - spent);
    assert_eq!(capacity, expected_capacity);
    assert!(capacity <= remaining);
    assert!(capacity <= global_available);
    assert!(capacity <= vault);
}

#[kani::proof]
#[kani::unwind(24)]
#[kani::solver(cadical)]
fn proof_v16_public_domain_insurance_withdraw_is_budget_scoped_and_value_conserving() {
    let budget_raw: u16 = kani::any();
    let withdraw_raw: u8 = kani::any();
    let surplus_raw: u16 = kani::any();
    kani::assume(budget_raw > 0);
    kani::assume(withdraw_raw > 0 && (withdraw_raw as u16) <= budget_raw);
    let budget = budget_raw as u128;
    let withdraw = withdraw_raw as u128;
    let surplus = surplus_raw as u128;
    let (mut header, mut markets) = one_market_only_fixture();
    header.vault = V16PodU128::new(budget + surplus);
    header.insurance = V16PodU128::new(budget + surplus);
    header.insurance_domain_budget_remaining_total = V16PodU128::new(budget);
    markets[0].engine.insurance_domain_budget_long = V16PodU128::new(budget);
    let vault_before = header.vault.get();
    let c_tot_before = header.c_tot.get();
    let insurance_before = header.insurance.get();
    let remaining_before = header.insurance_domain_budget_remaining_total.get();
    let long_budget_before = markets[0].engine.insurance_domain_budget_long.get();
    let short_budget_before = markets[0].engine.insurance_domain_budget_short.get();
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let residual_before = market.kani_residual();

    market
        .withdraw_domain_insurance_not_atomic(0, withdraw)
        .unwrap();

    kani::cover!(
        withdraw < budget,
        "public domain insurance withdraw covers partial selected-domain withdrawal"
    );
    kani::cover!(
        withdraw == budget,
        "public domain insurance withdraw covers full selected-domain withdrawal"
    );
    kani::cover!(
        surplus > 255,
        "public domain insurance withdraw preserves wide unrelated unbudgeted surplus"
    );
    assert_eq!(market.header.vault.get(), vault_before - withdraw);
    assert_eq!(market.header.insurance.get(), insurance_before - withdraw);
    assert_eq!(market.header.c_tot.get(), c_tot_before);
    assert_eq!(
        market.header.insurance_domain_budget_remaining_total.get(),
        remaining_before - withdraw
    );
    assert_eq!(
        market.markets[0].engine.insurance_domain_budget_long.get(),
        long_budget_before - withdraw
    );
    assert_eq!(
        market.markets[0].engine.insurance_domain_budget_short.get(),
        short_budget_before
    );
    assert_eq!(
        market.header.insurance.get() - market.header.insurance_domain_budget_remaining_total.get(),
        surplus
    );
    assert_eq!(
        long_budget_before - market.markets[0].engine.insurance_domain_budget_long.get(),
        vault_before - market.header.vault.get()
    );
    // Junior-pool isolation: insurance withdrawals exit only insurance stock
    // (vault and insurance fall in lockstep); the junior residual pool cannot
    // be drained through them.
    assert_eq!(market.kani_residual(), residual_before);
    assert_eq!(market.validate_shape(), Ok(()));
}

#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn proof_v16_domain_insurance_spent_delta_cannot_create_unbacked_budget() {
    let budget: u128 = kani::any();
    let old_spent: u128 = kani::any();
    let new_spent: u128 = kani::any();
    let other_remaining: u128 = kani::any();
    let extra_insurance: u128 = kani::any();
    kani::assume(budget > 0);
    kani::assume(old_spent <= budget);
    kani::assume(new_spent <= budget);
    let old_remaining = budget - old_spent;
    let new_remaining = budget - new_spent;
    kani::assume(other_remaining <= u128::MAX - old_remaining);
    kani::assume(other_remaining <= u128::MAX - new_remaining);
    let total_remaining = old_remaining + other_remaining;
    kani::assume(extra_insurance <= u128::MAX - total_remaining);
    let insurance = total_remaining + extra_insurance;
    let result = MarketGroupV16ViewMut::<u64>::kani_set_domain_insurance_spent_delta(
        total_remaining,
        insurance,
        budget,
        old_spent,
        new_spent,
    );
    let expected_total = other_remaining + new_remaining;
    let expected_ok = expected_total <= insurance;

    kani::cover!(
        expected_ok && new_spent < old_spent && old_spent > 1,
        "domain spent delta covers backed spent clearing"
    );
    kani::cover!(
        !expected_ok && new_spent < old_spent,
        "domain spent delta rejects unbacked spent clearing"
    );
    assert_eq!(result.is_ok(), expected_ok);
    if let Ok(next_total) = result {
        assert_eq!(next_total, expected_total);
        assert!(next_total <= insurance);
    }
}

#[kani::proof]
#[kani::unwind(24)]
#[kani::solver(cadical)]
fn proof_v16_public_domain_insurance_spent_setter_preserves_budget_total_and_value() {
    let budget_raw: u16 = kani::any();
    let old_spent_raw: u16 = kani::any();
    let new_spent_raw: u16 = kani::any();
    let surplus_raw: u16 = kani::any();
    kani::assume(budget_raw > 0);
    kani::assume(old_spent_raw <= budget_raw);
    kani::assume(new_spent_raw <= budget_raw);
    let budget = budget_raw as u128;
    let old_spent = old_spent_raw as u128;
    let new_spent = new_spent_raw as u128;
    let surplus = surplus_raw as u128;
    let (mut header, mut markets) = one_market_only_fixture();
    header.vault = V16PodU128::new(budget + surplus);
    header.insurance = V16PodU128::new(budget + surplus);
    header.insurance_domain_budget_remaining_total = V16PodU128::new(budget - old_spent);
    markets[0].engine.insurance_domain_budget_long = V16PodU128::new(budget);
    markets[0].engine.insurance_domain_spent_long = V16PodU128::new(old_spent);
    let vault_before = header.vault.get();
    let c_tot_before = header.c_tot.get();
    let insurance_before = header.insurance.get();
    let budget_before = markets[0].engine.insurance_domain_budget_long.get();
    let short_budget_before = markets[0].engine.insurance_domain_budget_short.get();
    let short_spent_before = markets[0].engine.insurance_domain_spent_short.get();
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);

    market.set_domain_insurance_spent(0, new_spent).unwrap();

    kani::cover!(
        new_spent > old_spent,
        "public domain spent setter covers increasing spent"
    );
    kani::cover!(
        new_spent < old_spent && surplus > 255,
        "public domain spent setter covers clearing spent with wide backed surplus"
    );
    kani::cover!(
        new_spent == old_spent,
        "public domain spent setter covers no-op spent rewrite"
    );
    assert_eq!(market.header.vault.get(), vault_before);
    assert_eq!(market.header.insurance.get(), insurance_before);
    assert_eq!(market.header.c_tot.get(), c_tot_before);
    assert_eq!(
        market.markets[0].engine.insurance_domain_budget_long.get(),
        budget_before
    );
    assert_eq!(
        market.markets[0].engine.insurance_domain_budget_short.get(),
        short_budget_before
    );
    assert_eq!(
        market.markets[0].engine.insurance_domain_spent_long.get(),
        new_spent
    );
    assert_eq!(
        market.markets[0].engine.insurance_domain_spent_short.get(),
        short_spent_before
    );
    assert_eq!(
        market.header.insurance_domain_budget_remaining_total.get(),
        budget - new_spent
    );
    assert_eq!(
        market.header.insurance_domain_budget_remaining_total.get() + new_spent,
        market.markets[0].engine.insurance_domain_budget_long.get()
    );
    assert!(
        market.header.insurance_domain_budget_remaining_total.get()
            <= market.header.insurance.get()
    );
    assert_eq!(
        market.header.insurance.get() - market.header.insurance_domain_budget_remaining_total.get(),
        surplus + new_spent
    );
    assert_eq!(market.validate_shape(), Ok(()));
}

#[kani::proof]
#[kani::unwind(24)]
#[kani::solver(cadical)]
fn proof_v16_public_domain_insurance_spent_setter_rejects_invalid_without_mutation() {
    let budget_raw: u8 = kani::any();
    let old_spent_raw: u8 = kani::any();
    let surplus_raw: u8 = kani::any();
    let invalid_domain: bool = kani::any();
    kani::assume(budget_raw > 0);
    kani::assume(old_spent_raw <= budget_raw);
    let budget = budget_raw as u128;
    let old_spent = old_spent_raw as u128;
    let surplus = surplus_raw as u128;
    let spent = budget + 1;
    let domain = if invalid_domain { 2 } else { 0 };

    let (mut header, mut markets) = one_market_only_fixture();
    header.vault = V16PodU128::new(budget + surplus);
    header.insurance = V16PodU128::new(budget + surplus);
    header.insurance_domain_budget_remaining_total = V16PodU128::new(budget - old_spent);
    markets[0].engine.insurance_domain_budget_long = V16PodU128::new(budget);
    markets[0].engine.insurance_domain_spent_long = V16PodU128::new(old_spent);
    let vault_before = header.vault.get();
    let c_tot_before = header.c_tot.get();
    let insurance_before = header.insurance.get();
    let remaining_before = header.insurance_domain_budget_remaining_total.get();
    let long_budget_before = markets[0].engine.insurance_domain_budget_long.get();
    let long_spent_before = markets[0].engine.insurance_domain_spent_long.get();
    let short_budget_before = markets[0].engine.insurance_domain_budget_short.get();
    let short_spent_before = markets[0].engine.insurance_domain_spent_short.get();
    let risk_epoch_before = header.risk_epoch.get();
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);

    let result = market.set_domain_insurance_spent(domain, spent);

    kani::cover!(
        invalid_domain,
        "spent setter rejects invalid domain before budget mutation"
    );
    kani::cover!(
        !invalid_domain && old_spent < budget,
        "spent setter rejects spent above budget before budget mutation"
    );
    assert_eq!(
        result,
        if invalid_domain {
            Err(V16Error::InvalidLeg)
        } else {
            Err(V16Error::InvalidConfig)
        }
    );
    assert_eq!(market.header.vault.get(), vault_before);
    assert_eq!(market.header.insurance.get(), insurance_before);
    assert_eq!(market.header.c_tot.get(), c_tot_before);
    assert_eq!(
        market.header.insurance_domain_budget_remaining_total.get(),
        remaining_before
    );
    assert_eq!(
        market.markets[0].engine.insurance_domain_budget_long.get(),
        long_budget_before
    );
    assert_eq!(
        market.markets[0].engine.insurance_domain_spent_long.get(),
        long_spent_before
    );
    assert_eq!(
        market.markets[0].engine.insurance_domain_budget_short.get(),
        short_budget_before
    );
    assert_eq!(
        market.markets[0].engine.insurance_domain_spent_short.get(),
        short_spent_before
    );
    assert_eq!(market.header.risk_epoch.get(), risk_epoch_before);
}

#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn proof_v16_domain_insurance_budget_delta_cannot_overallocate_pooled_insurance() {
    let old_budget: u128 = kani::any();
    let spent: u128 = kani::any();
    let add: u128 = kani::any();
    let other_remaining: u128 = kani::any();
    let extra_insurance: u128 = kani::any();
    kani::assume(old_budget > 0);
    kani::assume(spent <= old_budget);
    kani::assume(add <= u128::MAX - old_budget);
    let new_budget = old_budget + add;
    let old_remaining = old_budget - spent;
    let new_remaining = new_budget - spent;
    kani::assume(other_remaining <= u128::MAX - old_remaining);
    kani::assume(other_remaining <= u128::MAX - new_remaining);
    let total_remaining = old_remaining + other_remaining;
    kani::assume(extra_insurance <= u128::MAX - total_remaining);
    let insurance = total_remaining + extra_insurance;
    let result = MarketGroupV16ViewMut::<u64>::kani_set_domain_insurance_budget_delta(
        total_remaining,
        insurance,
        old_budget,
        spent,
        new_budget,
    );
    let expected_total = other_remaining + new_remaining;
    let expected_ok = expected_total <= insurance;

    kani::cover!(
        expected_ok && add > 0 && extra_insurance > 0,
        "domain budget delta covers backed budget credit"
    );
    kani::cover!(
        !expected_ok && add > 0,
        "domain budget delta rejects over-allocation of pooled insurance"
    );
    assert_eq!(result.is_ok(), expected_ok);
    if let Ok(next_total) = result {
        assert_eq!(next_total, expected_total);
        assert!(next_total <= insurance);
    }
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_public_insurance_reserve_encumbers_budget_without_value_movement() {
    let atoms_raw: u8 = kani::any();
    kani::assume((1..=8).contains(&atoms_raw));
    let atoms = atoms_raw as u128;
    let amount = atoms * BOUND_SCALE;
    let (mut header, mut markets) = one_market_only_fixture();
    header.vault = V16PodU128::new(atoms);
    header.insurance = V16PodU128::new(atoms);
    header.insurance_domain_budget_remaining_total = V16PodU128::new(atoms);
    markets[0].engine.insurance_domain_budget_long = V16PodU128::new(atoms);
    let vault_before = header.vault;
    let c_tot_before = header.c_tot;
    let insurance_before = header.insurance;
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);

    let result = market.reserve_insurance_credit_not_atomic(0, amount);
    assert_eq!(result, Ok(()));
    let reservation = market.markets[0]
        .engine
        .insurance_reservation_long
        .try_to_runtime()
        .unwrap();
    let source = market.markets[0]
        .engine
        .source_credit_long
        .try_to_runtime()
        .unwrap();

    kani::cover!(
        atoms_raw > 1,
        "funded domain insurance reservation covers nontrivial symbolic amount"
    );
    assert_eq!(reservation.insurance_credit_reserved_num, amount);
    assert_eq!(reservation.valid_liened_insurance_num, 0);
    assert_eq!(source.insurance_credit_reserved_num, amount);
    assert_eq!(source.valid_liened_insurance_num, 0);
    assert_eq!(
        market
            .header
            .source_insurance_credit_reserved_total_atoms
            .get(),
        atoms
    );
    assert_eq!(
        market.header.insurance_domain_budget_remaining_total.get(),
        atoms
    );
    assert_eq!(market.header.vault, vault_before);
    assert_eq!(market.header.c_tot, c_tot_before);
    assert_eq!(market.header.insurance, insurance_before);
}

#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn proof_v16_public_insurance_lien_create_moves_reserved_credit_to_valid_lien() {
    // `atoms` is now symbolic (was a hard-coded 3 — the proof asserted facts about a
    // single concrete lien size). The market is built inline rather than via
    // `one_market_view_fixture`, whose discarded account ran a 16-element legs
    // zero-fill loop; that loop plus unwind(96) blew the formula past the 600s budget.
    let atoms_raw: u8 = kani::any();
    kani::assume((1..=5).contains(&atoms_raw));
    let atoms = atoms_raw as u128;
    let amount = atoms * BOUND_SCALE;
    let (market_id, _, _) = ids();
    let cfg = V16Config::public_user_fund_with_market_slots(1, 1, 0, 10);
    let mut header = MarketGroupV16HeaderAccount::new_dynamic(market_id, cfg, 1, 0).unwrap();
    let mut markets = [Market::new(0u64, EngineAssetSlotV16Account::default())];
    {
        let mut view = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        view.activate_empty_market_not_atomic(0, 100, 1).unwrap();
    }
    header.vault = V16PodU128::new(atoms);
    header.insurance = V16PodU128::new(atoms);
    markets[0].engine.insurance_domain_budget_long = V16PodU128::new(atoms);
    markets[0].engine.insurance_reservation_long =
        InsuranceCreditReservationV16Account::from_runtime(&InsuranceCreditReservationV16 {
            insurance_credit_reserved_num: amount,
            ..InsuranceCreditReservationV16::EMPTY
        });
    markets[0].engine.source_credit_long =
        SourceCreditStateV16Account::from_runtime(&SourceCreditStateV16 {
            insurance_credit_reserved_num: amount,
            credit_rate_num: CREDIT_RATE_SCALE,
            ..SourceCreditStateV16::EMPTY
        });
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    market.refresh_header_aggregate_totals_for_test().unwrap();

    market
        .create_source_credit_lien_from_insurance_not_atomic(0, amount)
        .unwrap();
    let reservation = market.markets[0]
        .engine
        .insurance_reservation_long
        .try_to_runtime()
        .unwrap();
    let source = market.markets[0]
        .engine
        .source_credit_long
        .try_to_runtime()
        .unwrap();

    kani::cover!(
        reservation.valid_liened_insurance_num == amount,
        "public insurance lien create covers nontrivial lien"
    );
    assert_eq!(reservation.insurance_credit_reserved_num, amount);
    assert_eq!(reservation.valid_liened_insurance_num, amount);
    assert_eq!(source.insurance_credit_reserved_num, amount);
    assert_eq!(source.valid_liened_insurance_num, amount);
    assert_eq!(
        market
            .header
            .source_insurance_credit_reserved_total_atoms
            .get(),
        atoms
    );
    assert_eq!(
        market.header.insurance_domain_budget_remaining_total.get(),
        atoms
    );
    assert_eq!(market.header.insurance.get(), atoms);
    assert_eq!(market.header.vault.get(), atoms);
    assert_eq!(
        market.markets[0].engine.insurance_domain_spent_long.get(),
        0
    );
    assert_eq!(market.validate_shape(), Ok(()));
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_public_insurance_lien_release_restores_reserved_credit_without_value_movement() {
    let atoms_raw: u8 = kani::any();
    kani::assume((1..=8).contains(&atoms_raw));
    let atoms = atoms_raw as u128;
    let amount = atoms * BOUND_SCALE;
    let (mut header, mut markets) = one_market_only_fixture();
    header.vault = V16PodU128::new(atoms);
    header.insurance = V16PodU128::new(atoms);
    header.source_insurance_credit_reserved_total_atoms = V16PodU128::new(atoms);
    header.insurance_domain_budget_remaining_total = V16PodU128::new(atoms);
    markets[0].engine.insurance_domain_budget_long = V16PodU128::new(atoms);
    markets[0].engine.insurance_reservation_long =
        InsuranceCreditReservationV16Account::from_runtime(&InsuranceCreditReservationV16 {
            insurance_credit_reserved_num: amount,
            valid_liened_insurance_num: amount,
            ..InsuranceCreditReservationV16::EMPTY
        });
    markets[0].engine.source_credit_long =
        SourceCreditStateV16Account::from_runtime(&SourceCreditStateV16 {
            insurance_credit_reserved_num: amount,
            valid_liened_insurance_num: amount,
            credit_rate_num: CREDIT_RATE_SCALE,
            ..SourceCreditStateV16::EMPTY
        });
    let vault_before = header.vault.get();
    let c_tot_before = header.c_tot.get();
    let insurance_before = header.insurance.get();
    let risk_epoch_before = header.risk_epoch.get();
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);

    market
        .release_source_credit_lien_from_insurance_not_atomic(0, amount)
        .unwrap();
    let reservation = market.markets[0]
        .engine
        .insurance_reservation_long
        .try_to_runtime()
        .unwrap();
    let source = market.markets[0]
        .engine
        .source_credit_long
        .try_to_runtime()
        .unwrap();

    kani::cover!(
        atoms_raw > 1,
        "public insurance lien release covers nontrivial reserved-credit restoration"
    );
    assert_eq!(market.header.vault.get(), vault_before);
    assert_eq!(market.header.c_tot.get(), c_tot_before);
    assert_eq!(market.header.insurance.get(), insurance_before);
    assert_eq!(market.header.risk_epoch.get(), risk_epoch_before + 1);
    assert_eq!(
        market
            .header
            .source_insurance_credit_reserved_total_atoms
            .get(),
        atoms
    );
    assert_eq!(
        market.header.insurance_domain_budget_remaining_total.get(),
        atoms
    );
    assert_eq!(
        market.markets[0].engine.insurance_domain_budget_long.get(),
        atoms
    );
    assert_eq!(
        market.markets[0].engine.insurance_domain_spent_long.get(),
        0
    );
    assert_eq!(reservation.insurance_credit_reserved_num, amount);
    assert_eq!(reservation.valid_liened_insurance_num, 0);
    assert_eq!(reservation.impaired_liened_insurance_num, 0);
    assert_eq!(source.insurance_credit_reserved_num, amount);
    assert_eq!(source.valid_liened_insurance_num, 0);
    assert_eq!(source.impaired_liened_insurance_num, 0);
    assert_eq!(source.credit_rate_num, CREDIT_RATE_SCALE);
    assert_eq!(source.credit_epoch, 1);
}

#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_public_insurance_lien_impair_moves_valid_to_impaired_without_value_movement() {
    let atoms_raw: u8 = kani::any();
    kani::assume((1..=8).contains(&atoms_raw));
    let atoms = atoms_raw as u128;
    let amount = atoms * BOUND_SCALE;
    let (mut header, mut markets) = one_market_only_fixture();
    header.vault = V16PodU128::new(atoms);
    header.insurance = V16PodU128::new(atoms);
    header.source_insurance_credit_reserved_total_atoms = V16PodU128::new(atoms);
    header.insurance_domain_budget_remaining_total = V16PodU128::new(atoms);
    markets[0].engine.insurance_domain_budget_long = V16PodU128::new(atoms);
    markets[0].engine.insurance_reservation_long =
        InsuranceCreditReservationV16Account::from_runtime(&InsuranceCreditReservationV16 {
            insurance_credit_reserved_num: amount,
            valid_liened_insurance_num: amount,
            ..InsuranceCreditReservationV16::EMPTY
        });
    markets[0].engine.source_credit_long =
        SourceCreditStateV16Account::from_runtime(&SourceCreditStateV16 {
            insurance_credit_reserved_num: amount,
            valid_liened_insurance_num: amount,
            credit_rate_num: CREDIT_RATE_SCALE,
            ..SourceCreditStateV16::EMPTY
        });
    let vault_before = header.vault.get();
    let c_tot_before = header.c_tot.get();
    let insurance_before = header.insurance.get();
    let risk_epoch_before = header.risk_epoch.get();
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);

    market
        .impair_source_credit_lien_from_insurance_not_atomic(0, amount)
        .unwrap();
    let reservation = market.markets[0]
        .engine
        .insurance_reservation_long
        .try_to_runtime()
        .unwrap();
    let source = market.markets[0]
        .engine
        .source_credit_long
        .try_to_runtime()
        .unwrap();

    kani::cover!(
        atoms_raw > 1,
        "public insurance lien impair covers nontrivial impairment"
    );
    assert_eq!(market.header.vault.get(), vault_before);
    assert_eq!(market.header.c_tot.get(), c_tot_before);
    assert_eq!(market.header.insurance.get(), insurance_before);
    assert_eq!(market.header.risk_epoch.get(), risk_epoch_before + 1);
    assert_eq!(
        market
            .header
            .source_insurance_credit_reserved_total_atoms
            .get(),
        atoms
    );
    assert_eq!(
        market.header.insurance_domain_budget_remaining_total.get(),
        atoms
    );
    assert_eq!(
        market.markets[0].engine.insurance_domain_budget_long.get(),
        atoms
    );
    assert_eq!(
        market.markets[0].engine.insurance_domain_spent_long.get(),
        0
    );
    assert_eq!(reservation.insurance_credit_reserved_num, amount);
    assert_eq!(reservation.valid_liened_insurance_num, 0);
    assert_eq!(reservation.impaired_liened_insurance_num, amount);
    assert_eq!(reservation.consumed_insurance_num, 0);
    assert_eq!(source.insurance_credit_reserved_num, amount);
    assert_eq!(source.valid_liened_insurance_num, 0);
    assert_eq!(source.impaired_liened_insurance_num, amount);
    assert_eq!(source.credit_rate_num, CREDIT_RATE_SCALE);
    assert_eq!(source.credit_epoch, 1);
}

#[kani::proof]
#[kani::unwind(16)]
#[kani::solver(cadical)]
fn proof_v16_insurance_lien_split_consume_spends_exact_reserved_atoms() {
    let first_raw: u8 = kani::any();
    let second_raw: u8 = kani::any();
    kani::assume(first_raw > 0);
    kani::assume(second_raw > 0);
    let first_atoms = first_raw as u128;
    let second_atoms = second_raw as u128;
    let first_num = first_atoms * BOUND_SCALE;
    let second_num = second_atoms * BOUND_SCALE;
    let total_num = first_num + second_num;
    let total_atoms = first_atoms + second_atoms;
    let reservation = InsuranceCreditReservationV16 {
        insurance_credit_reserved_num: total_num,
        valid_liened_insurance_num: total_num,
        ..InsuranceCreditReservationV16::EMPTY
    };
    let source = SourceCreditStateV16 {
        insurance_credit_reserved_num: total_num,
        valid_liened_insurance_num: total_num,
        credit_rate_num: CREDIT_RATE_SCALE,
        ..SourceCreditStateV16::EMPTY
    };

    let (reservation, source, spent, insurance) =
        MarketGroupV16ViewMut::<u64>::kani_prepare_insurance_lien_consume_delta(
            reservation,
            source,
            0,
            total_atoms,
            first_num,
        )
        .unwrap();
    assert_eq!(spent, first_atoms);
    assert_eq!(insurance, second_atoms);
    assert_eq!(reservation.insurance_credit_reserved_num, second_num);
    assert_eq!(reservation.valid_liened_insurance_num, second_num);
    assert_eq!(reservation.consumed_insurance_num, first_num);
    assert_eq!(source.insurance_credit_reserved_num, second_num);
    assert_eq!(source.valid_liened_insurance_num, second_num);
    assert_eq!(source.credit_rate_num, CREDIT_RATE_SCALE);

    let (reservation, source, spent, insurance) =
        MarketGroupV16ViewMut::<u64>::kani_prepare_insurance_lien_consume_delta(
            reservation,
            source,
            spent,
            insurance,
            second_num,
        )
        .unwrap();

    kani::cover!(
        first_atoms > 1 && second_atoms > 1,
        "split aligned insurance-lien consumption is nontrivial"
    );
    assert_eq!(spent, total_atoms);
    assert_eq!(insurance, 0);
    assert_eq!(reservation.insurance_credit_reserved_num, 0);
    assert_eq!(reservation.valid_liened_insurance_num, 0);
    assert_eq!(reservation.consumed_insurance_num, total_num);
    assert_eq!(reservation.impaired_liened_insurance_num, 0);
    assert_eq!(source.insurance_credit_reserved_num, 0);
    assert_eq!(source.valid_liened_insurance_num, 0);
    assert_eq!(source.impaired_liened_insurance_num, 0);
    assert_eq!(source.credit_rate_num, CREDIT_RATE_SCALE);
}

#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn proof_v16_insurance_lien_fractional_consume_rejects() {
    let atoms_raw: u16 = kani::any();
    let fractional_raw: u16 = kani::any();
    kani::assume(atoms_raw > 0);
    kani::assume(atoms_raw <= 4096);
    kani::assume(fractional_raw > 0);
    let available_num = (atoms_raw as u128 + 1) * BOUND_SCALE;
    let fractional_num = (atoms_raw as u128 * BOUND_SCALE) + fractional_raw as u128;
    let reservation = InsuranceCreditReservationV16 {
        insurance_credit_reserved_num: available_num,
        valid_liened_insurance_num: available_num,
        ..InsuranceCreditReservationV16::EMPTY
    };
    let source = SourceCreditStateV16 {
        insurance_credit_reserved_num: available_num,
        valid_liened_insurance_num: available_num,
        credit_rate_num: CREDIT_RATE_SCALE,
        ..SourceCreditStateV16::EMPTY
    };

    let result = MarketGroupV16ViewMut::<u64>::kani_prepare_insurance_lien_consume_delta(
        reservation,
        source,
        0,
        atoms_raw as u128 + 1,
        fractional_num,
    );

    kani::cover!(
        atoms_raw > 255 && fractional_raw > 1,
        "fractional insurance-lien consume covers widened non-atom-aligned support"
    );
    assert_eq!(result, Err(V16Error::InvalidConfig));
}

#[kani::proof]
#[kani::unwind(16)]
#[kani::solver(cadical)]
fn proof_v16_expired_counterparty_backing_bucket_accepts_receivable_refill() {
    let amount_raw: u8 = kani::any();
    let receivable_raw: u8 = kani::any();
    kani::assume(amount_raw > 0);
    kani::assume(receivable_raw > 0);
    let amount = amount_raw as u128;
    let receivable = receivable_raw as u128;
    let bucket = BackingBucketV16 {
        market_id: 1,
        consumed_liened_backing_num: receivable,
        expiry_slot: 4,
        status: BackingBucketStatusV16::Expired,
        ..BackingBucketV16::EMPTY
    };
    let source = SourceCreditStateV16 {
        spent_backing_num: receivable,
        provider_receivable_num: receivable,
        credit_rate_num: CREDIT_RATE_SCALE,
        ..SourceCreditStateV16::EMPTY
    };

    let (next_bucket, next_source) =
        MarketGroupV16ViewMut::<u64>::kani_prepare_counterparty_backing_add_delta(
            bucket, source, amount, 10, 20,
        )
        .unwrap();
    let refill = amount.min(receivable);

    kani::cover!(amount < receivable, "partial expired-bucket refill");
    kani::cover!(amount >= receivable, "complete expired-bucket refill");
    assert_eq!(next_bucket.status, BackingBucketStatusV16::Fresh);
    assert_eq!(next_bucket.expiry_slot, 20);
    assert_eq!(next_bucket.consumed_liened_backing_num, receivable - refill);
    assert_eq!(next_source.provider_receivable_num, receivable - refill);
    assert_eq!(next_bucket.fresh_unliened_backing_num, amount);
    assert_eq!(next_bucket.valid_liened_backing_num, 0);
    assert_eq!(next_bucket.impaired_liened_backing_num, 0);
    assert_eq!(next_source.fresh_reserved_backing_num, amount);
    assert_eq!(next_source.spent_backing_num, receivable);
    assert_eq!(next_source.valid_liened_backing_num, 0);
    assert_eq!(next_source.impaired_liened_backing_num, 0);
    assert_eq!(next_source.credit_rate_num, CREDIT_RATE_SCALE);
    assert_eq!(
        next_source.provider_receivable_num + refill,
        next_source.spent_backing_num
    );
}

#[kani::proof]
#[kani::unwind(16)]
#[kani::solver(cadical)]
fn proof_v16_counterparty_backing_add_delta_refills_or_rejects_by_bucket_state() {
    let amount_raw: u8 = kani::any();
    let receivable_raw: u8 = kani::any();
    let fresh_raw: u8 = kani::any();
    let status_raw: u8 = kani::any();
    let same_expiry: bool = kani::any();
    let stale_expiry: bool = kani::any();
    kani::assume(amount_raw <= 8);
    kani::assume(receivable_raw <= 8);
    kani::assume(fresh_raw <= 8);
    kani::assume(status_raw <= 3);

    let current_slot = 10u64;
    let requested_expiry = if stale_expiry { current_slot } else { 20 };
    let status = match status_raw {
        0 => BackingBucketStatusV16::Empty,
        1 => BackingBucketStatusV16::Expired,
        2 => BackingBucketStatusV16::Fresh,
        _ => BackingBucketStatusV16::Impaired,
    };
    let bucket_expiry = if status == BackingBucketStatusV16::Fresh && same_expiry {
        requested_expiry
    } else {
        17
    };
    let amount = amount_raw as u128;
    let receivable = receivable_raw as u128;
    let fresh_before = if status == BackingBucketStatusV16::Fresh {
        fresh_raw as u128
    } else {
        0
    };
    let bucket = BackingBucketV16 {
        market_id: 1,
        fresh_unliened_backing_num: fresh_before,
        consumed_liened_backing_num: receivable,
        expiry_slot: bucket_expiry,
        status,
        ..BackingBucketV16::EMPTY
    };
    let source = SourceCreditStateV16 {
        fresh_reserved_backing_num: fresh_before,
        spent_backing_num: receivable,
        provider_receivable_num: receivable,
        credit_rate_num: CREDIT_RATE_SCALE,
        ..SourceCreditStateV16::EMPTY
    };

    let result = MarketGroupV16ViewMut::<u64>::kani_prepare_counterparty_backing_add_delta(
        bucket,
        source,
        amount,
        current_slot,
        requested_expiry,
    );
    let valid_bucket = matches!(
        status,
        BackingBucketStatusV16::Empty | BackingBucketStatusV16::Expired
    ) || (status == BackingBucketStatusV16::Fresh && same_expiry);
    let expected_ok = amount > 0 && !stale_expiry && valid_bucket;

    kani::cover!(
        expected_ok && status == BackingBucketStatusV16::Empty && amount > receivable,
        "counterparty backing add covers fresh empty bucket and complete receivable refill"
    );
    kani::cover!(
        expected_ok && status == BackingBucketStatusV16::Expired && amount < receivable,
        "counterparty backing add covers expired bucket partial receivable refill"
    );
    kani::cover!(
        expected_ok && status == BackingBucketStatusV16::Fresh && fresh_before > 0,
        "counterparty backing add covers additive fresh bucket with matching expiry"
    );
    kani::cover!(
        amount == 0 || stale_expiry,
        "counterparty backing add rejects zero amount or non-future expiry"
    );
    kani::cover!(
        amount > 0 && !stale_expiry && status == BackingBucketStatusV16::Fresh && !same_expiry,
        "counterparty backing add rejects fresh bucket with mismatched expiry"
    );

    if expected_ok {
        let (next_bucket, next_source) = result.unwrap();
        let refill = amount.min(receivable);
        assert_eq!(next_bucket.status, BackingBucketStatusV16::Fresh);
        assert_eq!(next_bucket.expiry_slot, requested_expiry);
        assert_eq!(
            next_bucket.fresh_unliened_backing_num,
            fresh_before + amount
        );
        assert_eq!(
            next_source.fresh_reserved_backing_num,
            fresh_before + amount
        );
        assert_eq!(next_bucket.consumed_liened_backing_num, receivable - refill);
        assert_eq!(next_source.provider_receivable_num, receivable - refill);
        assert_eq!(next_source.spent_backing_num, receivable);
        assert_eq!(
            next_source.provider_receivable_num + refill,
            source.provider_receivable_num
        );
    } else if amount == 0 || stale_expiry {
        assert_eq!(result, Err(V16Error::InvalidConfig));
    } else {
        assert_eq!(result, Err(V16Error::LockActive));
    }
}

#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn proof_v16_source_credit_lien_face_and_backing_use_scaled_units() {
    let effective_raw: u8 = kani::any();
    let divisor_raw: u8 = kani::any();
    kani::assume((1..=32).contains(&effective_raw));
    kani::assume((1..=32).contains(&divisor_raw));
    let effective = effective_raw as u128;
    let divisor = divisor_raw as u128;
    let rate = CREDIT_RATE_SCALE / divisor;

    let (required_face_num, required_backing_num) =
        MarketGroupV16ViewMut::<u64>::kani_source_credit_lien_amounts_for_effective(
            effective, rate,
        )
        .unwrap();
    let realized_scaled = required_face_num.checked_mul(rate).unwrap() / CREDIT_RATE_SCALE;

    kani::cover!(
        divisor == 1 && effective > 1,
        "full-rate source lien sizing branch"
    );
    kani::cover!(
        divisor > 1 && required_face_num > required_backing_num,
        "partial-rate source lien sizing branch"
    );
    kani::cover!(
        effective_raw > 8 && divisor_raw > 8,
        "source lien sizing proof covers widened effective credit and haircut rate"
    );
    assert_eq!(required_backing_num, effective * BOUND_SCALE);
    if rate == CREDIT_RATE_SCALE {
        assert_eq!(required_face_num, required_backing_num);
    }
    assert!(required_face_num >= required_backing_num);
    assert!(realized_scaled >= required_backing_num);
    if required_face_num > 0 {
        let previous_realized_scaled = required_face_num
            .checked_sub(1)
            .unwrap()
            .checked_mul(rate)
            .unwrap()
            / CREDIT_RATE_SCALE;
        assert!(
            previous_realized_scaled < required_backing_num,
            "source-credit lien face must be the minimal scaled face that realizes the required backing"
        );
    }
}

#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn proof_v16_residual_reward_credit_is_capped_by_principal_and_crystallized_loss() {
    let crystallized_raw: u8 = kani::any();
    let spent_raw: u8 = kani::any();
    let principal_raw: u8 = kani::any();
    let received_raw: u8 = kani::any();
    let crystallized = crystallized_raw as u128;
    let spent = spent_raw as u128;
    let principal = principal_raw as u128;
    let received_before = received_raw as u128;
    kani::assume(spent <= crystallized);

    let mut trader_header = PortfolioAccountV16Account::default();
    trader_header.residual_crystallized_loss_atoms_total = V16PodU128::new(crystallized);
    trader_header.residual_spent_principal_atoms_total = V16PodU128::new(spent);
    let mut lp_header = PortfolioAccountV16Account::default();
    lp_header.residual_received_atoms_total = V16PodU128::new(received_before);
    let mut trader = PortfolioV16ViewMut {
        header: &mut trader_header,
    };
    let mut lp = PortfolioV16ViewMut {
        header: &mut lp_header,
    };

    let credit = MarketGroupV16ViewMut::<u64>::kani_transfer_account_residual_reward_credit(
        &mut trader,
        &mut lp,
        principal,
    )
    .unwrap();
    let available = crystallized - spent;

    kani::cover!(
        principal > available && available > 0,
        "residual reward proof covers crystallized-loss cap"
    );
    kani::cover!(
        principal <= available && principal > 0,
        "residual reward proof covers principal cap"
    );
    assert_eq!(credit, principal.min(available));
    assert!(credit <= principal);
    assert!(credit <= available);
    assert_eq!(
        trader.header.residual_spent_principal_atoms_total.get(),
        spent + credit
    );
    assert!(
        trader.header.residual_spent_principal_atoms_total.get()
            <= trader.header.residual_crystallized_loss_atoms_total.get()
    );
    assert_eq!(
        lp.header.residual_received_atoms_total.get(),
        received_before + credit
    );
}

#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn proof_v16_underbacked_source_credit_cannot_satisfy_im_lien_requirements() {
    let claim_raw: u16 = kani::any();
    let available_raw: u16 = kani::any();
    let required_raw: u16 = kani::any();
    kani::assume((1..=256).contains(&claim_raw));
    kani::assume(available_raw < claim_raw);
    kani::assume(required_raw > available_raw);
    kani::assume(required_raw <= claim_raw);

    let claim_num = claim_raw as u128 * BOUND_SCALE;
    let available_num = available_raw as u128 * BOUND_SCALE;
    let required_credit = required_raw as u128;
    let source = SourceCreditStateV16 {
        positive_claim_bound_num: claim_num,
        exact_positive_claim_num: claim_num,
        fresh_reserved_backing_num: available_num,
        credit_rate_num: 0,
        ..SourceCreditStateV16::EMPTY
    };
    let mut source = source;
    source.credit_rate_num = kani_expected_source_credit_rate_num_for_state(source).unwrap();
    let sized = MarketGroupV16ViewMut::<u64>::kani_source_credit_lien_amounts_for_effective(
        required_credit,
        source.credit_rate_num,
    );

    kani::cover!(
        available_raw == 0,
        "underbacked source-credit proof covers zero-backed domain"
    );
    kani::cover!(
        available_raw > 8 && required_raw > available_raw,
        "underbacked source-credit proof covers wide partially backed domain"
    );
    kani::cover!(
        claim_raw > 64,
        "underbacked source-credit proof covers widened claim domain"
    );
    assert!(source.credit_rate_num < CREDIT_RATE_SCALE);
    if let Ok((required_face_num, required_backing_num)) = sized {
        assert_eq!(required_backing_num, required_credit * BOUND_SCALE);
        assert!(required_face_num > source.positive_claim_bound_num);
        assert!(required_backing_num > available_num);
    } else {
        assert_eq!(source.credit_rate_num, 0);
    }
}

#[kani::proof]
#[kani::unwind(16)]
#[kani::solver(cadical)]
fn proof_v16_counterparty_credit_consumption_reports_atoms_not_scaled_backing() {
    let effective_raw: u8 = kani::any();
    let divisor_raw: u8 = kani::any();
    kani::assume(effective_raw > 0);
    kani::assume((1..=16).contains(&divisor_raw));
    let effective = effective_raw as u128;
    let divisor = divisor_raw as u128;
    let rate = CREDIT_RATE_SCALE / divisor;
    let (required_face_num, backing_num) =
        MarketGroupV16ViewMut::<u64>::kani_source_credit_lien_amounts_for_effective(
            effective, rate,
        )
        .unwrap();
    let source_credit = SourceCreditStateV16 {
        positive_claim_bound_num: required_face_num,
        exact_positive_claim_num: required_face_num,
        fresh_reserved_backing_num: backing_num,
        credit_rate_num: rate,
        ..SourceCreditStateV16::EMPTY
    };
    let backing_bucket = BackingBucketV16 {
        market_id: 1,
        fresh_unliened_backing_num: backing_num,
        expiry_slot: 100,
        status: BackingBucketStatusV16::Fresh,
        ..BackingBucketV16::EMPTY
    };
    let (backing_after_create, source_after_create) =
        MarketGroupV16ViewMut::<u64>::kani_prepare_counterparty_lien_create_delta(
            backing_bucket,
            source_credit,
            0,
            backing_num,
        )
        .unwrap();
    assert_eq!(backing_after_create.fresh_unliened_backing_num, 0);
    assert_eq!(backing_after_create.valid_liened_backing_num, backing_num);
    assert_eq!(backing_after_create.consumed_liened_backing_num, 0);
    assert_eq!(
        source_after_create.positive_claim_bound_num,
        required_face_num
    );
    assert_eq!(
        source_after_create.exact_positive_claim_num,
        required_face_num
    );
    assert_eq!(source_after_create.fresh_reserved_backing_num, backing_num);
    assert_eq!(source_after_create.valid_liened_backing_num, backing_num);
    assert_eq!(source_after_create.spent_backing_num, 0);
    assert_eq!(source_after_create.provider_receivable_num, 0);
    assert_eq!(source_after_create.credit_rate_num, rate);

    let (backing_after_consume, source_after_consume) =
        MarketGroupV16ViewMut::<u64>::kani_prepare_counterparty_lien_consume_delta(
            backing_after_create,
            source_after_create,
            backing_num,
        )
        .unwrap();
    let cure_atoms =
        MarketGroupV16ViewMut::<u64>::kani_counterparty_cure_atoms_from_scaled_backing(backing_num)
            .unwrap();

    kani::cover!(
        effective > 1,
        "counterparty source-credit consume uses nontrivial atom value"
    );
    kani::cover!(
        divisor > 1 && required_face_num > backing_num,
        "counterparty source-credit consume covers partial-rate source lien"
    );
    assert!(required_face_num >= backing_num);
    assert_eq!(backing_num, effective * BOUND_SCALE);
    assert_eq!(cure_atoms, effective);
    assert_ne!(cure_atoms, backing_num);
    assert_eq!(backing_after_consume.fresh_unliened_backing_num, 0);
    assert_eq!(backing_after_consume.valid_liened_backing_num, 0);
    assert_eq!(
        backing_after_consume.consumed_liened_backing_num,
        backing_num
    );
    assert_eq!(
        source_after_consume.positive_claim_bound_num,
        required_face_num
    );
    assert_eq!(
        source_after_consume.exact_positive_claim_num,
        required_face_num
    );
    assert_eq!(source_after_consume.fresh_reserved_backing_num, 0);
    assert_eq!(source_after_consume.valid_liened_backing_num, 0);
    assert_eq!(source_after_consume.spent_backing_num, backing_num);
    assert_eq!(source_after_consume.provider_receivable_num, backing_num);
    assert_eq!(source_after_consume.credit_rate_num, rate);
}

#[kani::proof]
#[kani::unwind(24)]
#[kani::solver(cadical)]
fn proof_v16_counterparty_source_credit_support_does_not_debit_vault_or_insurance() {
    let amount_raw: u8 = kani::any();
    kani::assume(amount_raw > 0);
    let amount = amount_raw as u128;
    let vault_before: u128 = kani::any();
    kani::assume(vault_before <= 1_000_000);

    let proof = TokenValueFlowProofV16::support_to_account_capital(
        amount,
        amount,
        0,
        0,
        vault_before,
        vault_before,
    )
    .unwrap();

    kani::cover!(
        amount > 1,
        "counterparty-backed source credit support mints account capital without insurance spend"
    );
    assert_eq!(proof.vault_after, vault_before);
    assert_eq!(proof.external_quote_in, 0);
    assert_eq!(proof.external_quote_out, 0);
    assert_eq!(
        proof.debits[TokenValueClassV16::AccountCapital as usize],
        amount
    );
    assert_eq!(
        proof.credits[TokenValueClassV16::CloseCounterpartyCreditConsumed as usize],
        amount
    );
    assert_eq!(
        proof.credits[TokenValueClassV16::CloseInsuranceSpent as usize],
        0
    );
    assert_eq!(
        proof.debits[TokenValueClassV16::InsuranceCapital as usize],
        0
    );
    assert_eq!(proof.validate(), Ok(()));
}

#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
fn proof_v16_support_to_account_capital_requires_exact_mixed_source_sum() {
    let counterparty_raw: u8 = kani::any();
    let insurance_raw: u8 = kani::any();
    let surplus_raw: u8 = kani::any();
    let overcredit: bool = kani::any();
    let vault_raw: u16 = kani::any();
    let counterparty = counterparty_raw as u128;
    let insurance = insurance_raw as u128;
    let surplus = surplus_raw as u128;
    let source_sum = counterparty + insurance + surplus;
    let requested_credit = if overcredit {
        source_sum + 1
    } else {
        source_sum
    };
    let vault = vault_raw as u128;

    let result = TokenValueFlowProofV16::support_to_account_capital(
        requested_credit,
        counterparty,
        insurance,
        surplus,
        vault,
        vault,
    );

    kani::cover!(
        !overcredit && counterparty > 0 && insurance > 0 && surplus > 0,
        "mixed support sources exactly credit account capital"
    );
    kani::cover!(
        !overcredit && counterparty > 0 && insurance == 0 && surplus == 0,
        "counterparty-only support remains valid"
    );
    kani::cover!(
        overcredit && counterparty > 0 && insurance > 0,
        "over-crediting beyond mixed support sources rejects"
    );

    if overcredit {
        assert_eq!(result, Err(V16Error::InvalidConfig));
    } else {
        let proof = result.unwrap();
        assert_eq!(proof.vault_before, vault);
        assert_eq!(proof.vault_after, vault);
        assert_eq!(proof.external_quote_in, 0);
        assert_eq!(proof.external_quote_out, 0);
        assert_eq!(
            proof.debits[TokenValueClassV16::AccountCapital as usize],
            source_sum
        );
        assert_eq!(
            proof.credits[TokenValueClassV16::CloseCounterpartyCreditConsumed as usize],
            counterparty
        );
        assert_eq!(
            proof.credits[TokenValueClassV16::CloseInsuranceSpent as usize],
            insurance
        );
        assert_eq!(
            proof.credits[TokenValueClassV16::UnallocatedProtocolSurplus as usize],
            surplus
        );
        assert_eq!(proof.validate(), Ok(()));
    }
}

#[kani::proof]
#[kani::unwind(24)]
#[kani::solver(cadical)]
fn proof_v16_counterparty_source_credit_support_is_prebacked_by_realized_capital() {
    let amount_raw: u8 = kani::any();
    kani::assume(amount_raw > 0);
    let amount = amount_raw as u128;
    let c_tot_before: u128 = kani::any();
    kani::assume(amount <= c_tot_before && c_tot_before <= 1_000_000);
    let vault = c_tot_before;

    let reserve_proof =
        TokenValueFlowProofV16::account_capital_to_realized_loss(amount, vault, vault).unwrap();
    let c_tot_after_reserve = c_tot_before - amount;

    let support_proof =
        TokenValueFlowProofV16::support_to_account_capital(amount, amount, 0, 0, vault, vault)
            .unwrap();
    let c_tot_after_support = c_tot_after_reserve + amount;

    kani::cover!(
        amount > 1 && c_tot_before > amount,
        "counterparty support is backed by a prior nontrivial capital reservation"
    );
    assert_eq!(
        reserve_proof.debits[TokenValueClassV16::AccountCapital as usize],
        amount
    );
    assert_eq!(
        reserve_proof.credits[TokenValueClassV16::ExplicitBackedLoss as usize],
        amount
    );
    assert_eq!(
        support_proof.credits[TokenValueClassV16::CloseCounterpartyCreditConsumed as usize],
        amount
    );
    assert_eq!(
        support_proof.debits[TokenValueClassV16::AccountCapital as usize],
        amount
    );
    assert_eq!(reserve_proof.validate(), Ok(()));
    assert_eq!(support_proof.validate(), Ok(()));
    assert_eq!(c_tot_after_support, c_tot_before);
    assert_eq!(reserve_proof.vault_after, vault);
    assert_eq!(support_proof.vault_after, vault);
}

#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn proof_v16_target_effective_lag_adverse_delta_is_side_specific() {
    let is_long: bool = kani::any();
    let effective_price: u64 = kani::any();
    let raw_target_price: u64 = kani::any();
    kani::assume((1..=MAX_ORACLE_PRICE).contains(&effective_price));
    kani::assume((1..=MAX_ORACLE_PRICE).contains(&raw_target_price));
    let side = if is_long {
        SideV16::Long
    } else {
        SideV16::Short
    };
    let adverse_delta =
        kani_target_effective_lag_adverse_delta(side, effective_price, raw_target_price);
    let expected_delta = match side {
        SideV16::Long if raw_target_price < effective_price => effective_price - raw_target_price,
        SideV16::Short if raw_target_price > effective_price => raw_target_price - effective_price,
        _ => 0,
    };

    kani::cover!(
        is_long && raw_target_price < effective_price && adverse_delta > 1024,
        "target/effective lag delta covers adverse long mark"
    );
    kani::cover!(
        !is_long && raw_target_price > effective_price && adverse_delta > 1024,
        "target/effective lag delta covers adverse short mark"
    );
    kani::cover!(
        ((is_long && raw_target_price >= effective_price)
            || (!is_long && raw_target_price <= effective_price))
            && adverse_delta == 0,
        "target/effective lag delta covers favorable/no-penalty branch"
    );
    assert_eq!(adverse_delta, expected_delta);
    if adverse_delta != 0 {
        assert!(
            (side == SideV16::Long && raw_target_price < effective_price)
                || (side == SideV16::Short && raw_target_price > effective_price)
        );
    }
}

#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn proof_v16_target_effective_lag_penalty_enters_all_health_lanes() {
    let base_initial: u128 = kani::any();
    let base_maintenance: u128 = kani::any();
    let risk_notional: u128 = kani::any();
    let penalty: u128 = kani::any();
    kani::assume(penalty > 0);
    kani::assume(base_initial <= u128::MAX - penalty);
    kani::assume(base_maintenance <= u128::MAX - penalty);
    kani::assume(risk_notional <= u128::MAX - penalty);

    let (initial_req, maintenance_req, worst_case_loss) =
        kani_health_requirements_from_base_and_target_lag(
            base_initial,
            base_maintenance,
            risk_notional,
            penalty,
        )
        .unwrap();

    kani::cover!(
        base_initial > 0 && base_maintenance > 0,
        "target/effective lag health proof covers nonzero base margin lanes"
    );
    kani::cover!(
        base_initial > base_maintenance && base_maintenance > 0,
        "target/effective lag health proof covers distinct base IM/MM lanes"
    );
    kani::cover!(
        risk_notional > 1024 && penalty > 1024,
        "target/effective lag health proof covers wide symbolic notional and penalty"
    );
    assert_eq!(initial_req, base_initial + penalty);
    assert_eq!(maintenance_req, base_maintenance + penalty);
    assert_eq!(worst_case_loss, risk_notional + penalty);
    assert!(initial_req >= penalty);
    assert!(maintenance_req >= penalty);
    assert!(worst_case_loss >= penalty);
}

#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn proof_v16_nontrivial_public_profile_satisfies_symbolic_mm_envelope() {
    let x_raw: u32 = kani::any();

    kani::assume(x_raw > 0);

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

    let x = x_raw as u128;

    kani::cover!(
        x > 64,
        "nontrivial accepted config covers interior notionals beyond endpoint checks"
    );
    assert!(x <= MAX_ACCOUNT_NOTIONAL);
    assert_eq!(cfg.kani_solvency_envelope_holds_for_notional(x), Ok(true));
}

#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn proof_v16_symbolic_conservative_fee_profile_satisfies_mm_envelope_on_small_notionals() {
    let price_move_bps: u16 = kani::any();
    let liq_fee_bps: u16 = kani::any();
    let min_liq_abs_raw: u8 = kani::any();
    let liq_fee_cap_raw: u8 = kani::any();
    let x_raw: u32 = kani::any();

    kani::assume((1..=250).contains(&price_move_bps));
    kani::assume(liq_fee_bps <= 250);
    kani::assume(min_liq_abs_raw <= 3);
    kani::assume(liq_fee_cap_raw <= 3);
    kani::assume(min_liq_abs_raw <= liq_fee_cap_raw);
    kani::assume(x_raw > 0);

    let mut cfg = V16Config::public_user_fund_with_market_slots(1, 1, 1, 10);
    cfg.maintenance_margin_bps = 10_000;
    cfg.initial_margin_bps = 10_000;
    cfg.max_price_move_bps_per_slot = price_move_bps as u64;
    cfg.max_accrual_dt_slots = 1;
    cfg.min_funding_lifetime_slots = 1;
    cfg.max_abs_funding_e9_per_slot = 0;
    cfg.liquidation_fee_bps = liq_fee_bps as u64;
    cfg.min_liquidation_abs = min_liq_abs_raw as u128;
    cfg.liquidation_fee_cap = liq_fee_cap_raw as u128;
    cfg.min_nonzero_mm_req = liq_fee_cap_raw as u128 + 1;
    cfg.min_nonzero_im_req = cfg.min_nonzero_mm_req + 1;

    let x = x_raw as u128;

    kani::cover!(
        liq_fee_bps > 0 && min_liq_abs_raw > 0,
        "conservative profile includes nonzero proportional and absolute liquidation fee"
    );
    kani::cover!(
        x > 64,
        "conservative symbolic fee profile covers interior small-notional envelope"
    );
    assert_eq!(cfg.kani_solvency_envelope_holds_for_notional(x), Ok(true));
}

#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn proof_v16_symbolic_funding_profile_satisfies_mm_envelope_on_small_notionals() {
    let funding_e9_raw: u16 = kani::any();
    let x_raw: u32 = kani::any();

    kani::assume(funding_e9_raw <= 50);
    kani::assume(x_raw > 0);

    let mut cfg = V16Config::public_user_fund_with_market_slots(1, 1, 1, 10);
    cfg.maintenance_margin_bps = 10_000;
    cfg.initial_margin_bps = 10_000;
    cfg.max_price_move_bps_per_slot = 100;
    cfg.max_accrual_dt_slots = 1;
    cfg.min_funding_lifetime_slots = 1;
    cfg.max_abs_funding_e9_per_slot = funding_e9_raw as u64;
    cfg.liquidation_fee_bps = 100;
    cfg.min_liquidation_abs = 1;
    cfg.liquidation_fee_cap = 1;
    cfg.min_nonzero_mm_req = 2;
    cfg.min_nonzero_im_req = 3;

    let x = x_raw as u128;

    kani::cover!(
        funding_e9_raw > 0 && x > 64,
        "symbolic funding profile covers nonzero funding and interior notional"
    );
    assert_eq!(cfg.kani_solvency_envelope_holds_for_notional(x), Ok(true));
}

// Clean-room inductive senior-solvency proof (independent of any external PR).
//
// validate_shape enforces the senior leg `c_tot + insurance (+ earnings) <= vault`
// and per-account `capital <= c_tot` via an O(N) loop scan, which makes
// assume(validate_shape) intractable over full-domain symbolic state. This decomposes
// the senior-solvency invariant into a loop-free predicate, assumes it over FULLY
// SYMBOLIC u128/i128 economic scalars (no <=1000 bounds), applies the bare negative-PnL
// principal-settlement transition, and proves INV(s) => INV(f(s)) plus the exact
// value-conservation delta laws. Covers fire on partial and full settlement
// (non-vacuous). No markets/legs are touched by this transition, so unwind(8) holds.
fn inv_senior_accounting(vault: u128, c_tot: u128, insurance: u128) -> bool {
    c_tot
        .checked_add(insurance)
        .map(|s| s <= vault)
        .unwrap_or(false)
}

#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
fn proof_v16_inductive_settle_negative_pnl_preserves_senior_solvency() {
    let vault: u128 = kani::any();
    let c_tot: u128 = kani::any();
    let insurance: u128 = kani::any();
    let capital: u128 = kani::any();
    let pnl: i128 = kani::any();

    // assume(canonical_inv(s)) -- decomposed, loop-free, full-domain symbolic.
    kani::assume(inv_senior_accounting(vault, c_tot, insurance));
    kani::assume(capital <= c_tot); // per-account capital cannot exceed the aggregate
    kani::assume(pnl > i128::MIN); // engine validate_non_min_i128 precondition
    kani::assume(pnl < 0); // the negative-PnL principal-settlement case

    let (market_id, _, _) = ids();
    let cfg = V16Config::public_user_fund_with_market_slots(1, 1, 0, 10);
    let mut header = MarketGroupV16HeaderAccount::new_dynamic(market_id, cfg, 1, 0).unwrap();
    let mut markets = [Market::new(0u64, EngineAssetSlotV16Account::default())];
    header.vault = V16PodU128::new(vault);
    header.c_tot = V16PodU128::new(c_tot);
    header.insurance = V16PodU128::new(insurance);
    header.negative_pnl_account_count = V16PodU64::new(1); // the one negative account

    let mut acct_header = PortfolioAccountV16Account::default();
    acct_header.capital = V16PodU128::new(capital);
    acct_header.pnl = V16PodI128::new(pnl);

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut acct_header);

    let loss = pnl.unsigned_abs();
    kani::cover!(
        capital < loss,
        "partial principal settlement: capital < loss"
    );
    kani::cover!(
        capital >= loss,
        "full principal settlement: capital covers loss"
    );

    let result = market.kani_settle_negative_pnl_from_principal_core_not_atomic(&mut account);
    assert!(result.is_ok());
    let paid = result.unwrap();

    let vault_after = market.header.vault.get();
    let c_tot_after = market.header.c_tot.get();
    let insurance_after = market.header.insurance.get();

    // INV(f(s)): senior solvency and the per-account leg are preserved by the transition.
    assert!(inv_senior_accounting(
        vault_after,
        c_tot_after,
        insurance_after
    ));
    assert!(account.header.capital.get() <= c_tot_after);

    // Value-conservation delta laws: the transition moves exactly `paid` from the
    // account's capital and the c_tot aggregate (lockstep), leaves vault and insurance
    // untouched, and `paid` is capped at min(capital, loss).
    assert_eq!(paid, capital.min(loss));
    let expected_pnl = pnl + i128::try_from(paid).unwrap();
    assert_eq!(account.header.pnl.get(), expected_pnl);
    assert_eq!(vault_after, vault);
    assert_eq!(insurance_after, insurance);
    assert_eq!(c_tot_after, c_tot - paid);
    assert_eq!(account.header.capital.get(), capital - paid);
    if paid < loss {
        assert!(expected_pnl < 0);
        assert_eq!(market.header.bankruptcy_hlock_active, 1);
        assert_eq!(market.header.negative_pnl_account_count.get(), 1);
    } else {
        assert_eq!(expected_pnl, 0);
        assert_eq!(market.header.bankruptcy_hlock_active, 0);
        assert_eq!(market.header.negative_pnl_account_count.get(), 0);
    }
}

#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
fn proof_v16_inductive_fee_core_preserves_senior_solvency_and_never_debits_insurance() {
    let vault: u128 = kani::any();
    let c_tot: u128 = kani::any();
    let insurance: u128 = kani::any();
    let capital: u128 = kani::any();
    let requested_fee: u128 = kani::any();
    let pnl: i128 = kani::any();

    // Decomposed canonical senior-accounting invariant over full symbolic scalars.
    kani::assume(inv_senior_accounting(vault, c_tot, insurance));
    kani::assume(capital <= c_tot);

    let (market_id, _, _) = ids();
    let cfg = V16Config::public_user_fund_with_market_slots(1, 1, 0, 10);
    let mut header = MarketGroupV16HeaderAccount::new_dynamic(market_id, cfg, 1, 0).unwrap();
    let mut markets = [Market::new(0u64, EngineAssetSlotV16Account::default())];
    header.vault = V16PodU128::new(vault);
    header.c_tot = V16PodU128::new(c_tot);
    header.insurance = V16PodU128::new(insurance);

    let mut acct_header = PortfolioAccountV16Account::default();
    acct_header.capital = V16PodU128::new(capital);
    acct_header.pnl = V16PodI128::new(pnl);

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut acct_header);

    kani::cover!(
        pnl >= 0 && requested_fee > 0 && requested_fee < capital,
        "fee core covers partial fee charge"
    );
    kani::cover!(
        pnl >= 0 && requested_fee > capital && capital > 0,
        "fee core covers capital-capped fee charge"
    );
    kani::cover!(
        pnl < 0 && requested_fee > 0 && capital > 0,
        "fee core covers negative-PnL no-charge branch"
    );

    let result = market.kani_charge_account_fee_current_not_atomic(&mut account, requested_fee);
    assert!(result.is_ok());
    let charged = result.unwrap();
    let expected = if requested_fee == 0 || pnl < 0 {
        0
    } else {
        requested_fee.min(capital)
    };

    let vault_after = market.header.vault.get();
    let c_tot_after = market.header.c_tot.get();
    let insurance_after = market.header.insurance.get();
    assert_eq!(charged, expected);
    assert_eq!(vault_after, vault);
    assert_eq!(c_tot_after, c_tot - charged);
    assert_eq!(insurance_after, insurance + charged);
    assert_eq!(account.header.capital.get(), capital - charged);
    assert!(insurance_after >= insurance);
    assert!(inv_senior_accounting(
        vault_after,
        c_tot_after,
        insurance_after
    ));
    assert!(account.header.capital.get() <= c_tot_after);
}

// Finding E: cure_and_cancel_close_not_atomic leaves close_progress in the `canceled`
// (inert) state, never EMPTY; withdraw_not_atomic rejected any non-EMPTY close_progress,
// so a flat, solvent user who cured a forced close could never withdraw their capital
// again (Deposit doesn't gate, so deposits also became frozen -> capital sink). A canceled
// ledger is validated to carry no irreversible progress (residual == gross_loss), i.e. no
// obligation, so it must not block withdrawal. RED until withdraw treats a canceled ledger
// like EMPTY.
#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_withdraw_allowed_after_canceled_close() {
    let amount_raw: u8 = kani::any();
    kani::assume((1..=4).contains(&amount_raw));
    let amount = amount_raw as u128;

    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    market.deposit_not_atomic(&mut account, 5).unwrap(); // flat, solvent, capital 5

    // Post-cure inert canceled close ledger (valid per
    // validate_close_progress_ledger_with_market: canceled & !active & !finalized,
    // close_id != 0, no irreversible progress, residual == gross_loss == 0).
    account.header.close_progress =
        CloseProgressLedgerV16Account::from_runtime(&CloseProgressLedgerV16 {
            canceled: true,
            close_id: 1,
            asset_index: 0,
            market_id: 1,
            domain_side: SideV16::Long,
            ..CloseProgressLedgerV16::EMPTY
        });

    // The cured state is valid and reachable.
    assert_eq!(account.validate_with_market(&market.as_view()), Ok(()));

    let capital_before = account.header.capital.get();
    // A flat, solvent, cured user must be able to withdraw their own capital.
    let result = market.withdraw_not_atomic(&mut account, amount);

    kani::cover!(amount > 1, "withdraw-after-cure covers nontrivial amount");
    assert_eq!(result, Ok(()));
    assert_eq!(account.header.capital.get(), capital_before - amount);
}

fn finalized_zero_residual_close_for_proof(
    market_id: u64,
    prior_id: u64,
    gross: u128,
    progress_kind: u8,
) -> CloseProgressLedgerV16 {
    CloseProgressLedgerV16 {
        active: true,
        finalized: true,
        close_id: prior_id,
        asset_index: 0,
        market_id,
        domain_side: SideV16::Long,
        gross_loss_at_close_start: gross,
        support_consumed: if progress_kind == 0 { gross } else { 0 },
        junior_face_burned: if progress_kind == 0 { gross } else { 0 },
        insurance_spent: if progress_kind == 1 { gross } else { 0 },
        b_loss_booked: if progress_kind == 2 { gross } else { 0 },
        explicit_loss_assigned: if progress_kind == 3 { gross } else { 0 },
        residual_remaining: 0,
        ..CloseProgressLedgerV16::EMPTY
    }
}

// A finalized zero-residual close is terminal, even though the persisted ledger
// stays `active` to preserve close identity/history. It must therefore be inert
// for empty-account lifecycle accounting. This fails on the old implementation:
// `is_empty_for_dematerialization` rejected the valid active+finalized ledger.
#[kani::proof]
#[kani::unwind(56)]
#[kani::solver(cadical)]
fn proof_v16_finalized_zero_residual_close_is_inert_for_dematerialization() {
    let deregister: bool = kani::any();
    let progress_kind: u8 = kani::any();
    let gross_raw: u8 = kani::any();
    let prior_id_raw: u8 = kani::any();
    kani::assume(progress_kind <= 3);
    kani::assume((1..=4).contains(&gross_raw));
    kani::assume((1..=4).contains(&prior_id_raw));

    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    if deregister {
        header.materialized_portfolio_count = V16PodU64::new(1);
    }
    let terminal = finalized_zero_residual_close_for_proof(
        markets[0].engine.asset.market_id.get(),
        prior_id_raw as u64,
        gross_raw as u128,
        progress_kind,
    );
    account_header.close_progress = CloseProgressLedgerV16Account::from_runtime(&terminal);
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let account = PortfolioV16ViewMut::new(&mut account_header);

    assert_eq!(account.validate_with_market(&market.as_view()), Ok(()));
    if deregister {
        market
            .deregister_empty_materialized_portfolio_not_atomic(&account.as_view())
            .unwrap();
        assert_eq!(market.header.materialized_portfolio_count.get(), 0);
    } else {
        market
            .register_empty_materialized_portfolio_not_atomic(&account.as_view())
            .unwrap();
        assert_eq!(market.header.materialized_portfolio_count.get(), 1);
    }

    kani::cover!(deregister, "terminal close can deregister empty account");
    kani::cover!(!deregister, "terminal close can register empty account");
    kani::cover!(
        progress_kind == 0,
        "terminal close includes support progress"
    );
    kani::cover!(
        progress_kind == 3,
        "terminal close includes explicit-loss progress"
    );
    assert_eq!(market.validate_shape(), Ok(()));
}

// A terminal close has no pending residual obligation and must not freeze a
// flat, solvent user's ordinary principal withdrawal.
#[kani::proof]
#[kani::unwind(56)]
#[kani::solver(cadical)]
fn proof_v16_finalized_zero_residual_close_is_inert_for_flat_withdraw() {
    let progress_kind: u8 = kani::any();
    let gross_raw: u8 = kani::any();
    kani::assume(progress_kind <= 3);
    kani::assume((1..=4).contains(&gross_raw));
    let capital = 4u128;
    let withdraw = 1u128;

    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    header.vault = V16PodU128::new(capital);
    header.c_tot = V16PodU128::new(capital);
    account_header.capital = V16PodU128::new(capital);
    let terminal = finalized_zero_residual_close_for_proof(
        markets[0].engine.asset.market_id.get(),
        1,
        gross_raw as u128,
        progress_kind,
    );
    account_header.close_progress = CloseProgressLedgerV16Account::from_runtime(&terminal);
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);

    market.withdraw_not_atomic(&mut account, withdraw).unwrap();

    kani::cover!(
        progress_kind == 1,
        "terminal close includes insurance progress"
    );
    assert_eq!(account.header.capital.get(), capital - withdraw);
    assert_eq!(market.header.c_tot.get(), capital - withdraw);
    assert_eq!(market.header.vault.get(), capital - withdraw);
}

// A terminal close carries the close_id watermark but is not an active pending
// close for ownership. A later close may begin and must strictly advance the id.
#[kani::proof]
#[kani::unwind(56)]
#[kani::solver(cadical)]
fn proof_v16_finalized_zero_residual_close_is_inert_for_next_begin() {
    let progress_kind: u8 = kani::any();
    let gross_raw: u8 = kani::any();
    let prior_id_raw: u8 = kani::any();
    let next_gross_raw: u8 = kani::any();
    kani::assume(progress_kind <= 3);
    kani::assume((1..=4).contains(&gross_raw));
    kani::assume((1..=4).contains(&prior_id_raw));
    kani::assume((1..=4).contains(&next_gross_raw));
    let prior_id = prior_id_raw as u64;
    let next_gross = next_gross_raw as u128;

    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    account_header.last_fee_slot = V16PodU64::new(1);
    let terminal = finalized_zero_residual_close_for_proof(
        markets[0].engine.asset.market_id.get(),
        prior_id,
        gross_raw as u128,
        progress_kind,
    );
    account_header.close_progress = CloseProgressLedgerV16Account::from_runtime(&terminal);
    let current_slot = header.current_slot.get();
    let max_life = header.config.max_bankrupt_close_lifetime_slots.get();
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);

    assert_eq!(account.validate_with_market(&market.as_view()), Ok(()));
    market
        .kani_begin_close_progress_ledger(&mut account, 0, SideV16::Long, next_gross)
        .unwrap();

    let next = account.header.close_progress.try_to_runtime().unwrap();
    kani::cover!(
        progress_kind == 2,
        "terminal close includes booked-loss progress"
    );
    kani::cover!(prior_id > 1, "terminal close carries nontrivial prior id");
    assert!(next.active && !next.finalized && !next.canceled);
    assert_eq!(next.close_id, prior_id + 1);
    assert_eq!(next.gross_loss_at_close_start, next_gross);
    assert_eq!(next.residual_remaining, next_gross);
    assert_eq!(next.drift_reference_slot, current_slot);
    assert_eq!(next.max_close_slot, current_slot + max_life);
    assert_eq!(
        market.markets[0]
            .engine
            .pending_domain_loss_barrier_long
            .get(),
        1
    );
}

fn resolved_payout_receipt_for_proof(face: u128, paid: u128) -> ResolvedPayoutReceiptV16 {
    ResolvedPayoutReceiptV16 {
        present: true,
        prior_bound_contribution_num: face * BOUND_SCALE,
        live_released_face_at_receipt: 0,
        terminal_positive_claim_face: face,
        paid_effective: paid,
        finalized: paid == face,
    }
}

// A fully paid resolved-payout receipt is terminal account history. Like a
// finalized close ledger, it must not strand empty-account lifecycle APIs just
// because the persisted receipt has not yet been zeroed.
#[kani::proof]
#[kani::unwind(56)]
#[kani::solver(cadical)]
fn proof_v16_finalized_resolved_receipt_is_inert_for_dematerialization() {
    let deregister: bool = kani::any();
    let face_raw: u8 = kani::any();
    kani::assume((1..=8).contains(&face_raw));
    let face = face_raw as u128;

    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    if deregister {
        header.materialized_portfolio_count = V16PodU64::new(1);
    }
    account_header.resolved_payout_receipt = ResolvedPayoutReceiptV16Account::from_runtime(
        &resolved_payout_receipt_for_proof(face, face),
    );
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let account = PortfolioV16ViewMut::new(&mut account_header);

    assert_eq!(account.validate_with_market(&market.as_view()), Ok(()));
    if deregister {
        market
            .deregister_empty_materialized_portfolio_not_atomic(&account.as_view())
            .unwrap();
        assert_eq!(market.header.materialized_portfolio_count.get(), 0);
    } else {
        market
            .register_empty_materialized_portfolio_not_atomic(&account.as_view())
            .unwrap();
        assert_eq!(market.header.materialized_portfolio_count.get(), 1);
    }

    kani::cover!(deregister, "finalized receipt can deregister empty account");
    kani::cover!(!deregister, "finalized receipt can register empty account");
    kani::cover!(
        face > 2,
        "finalized receipt covers nontrivial terminal face"
    );
    assert_eq!(market.validate_shape(), Ok(()));
}

// The inertness rule is exact: a present receipt with unpaid effective face is
// still an outstanding resolved-payout claim and must block empty-account
// dematerialization without mutating global materialized-account accounting.
#[kani::proof]
#[kani::unwind(56)]
#[kani::solver(cadical)]
fn proof_v16_unfinalized_resolved_receipt_blocks_dematerialization_before_mutation() {
    let deregister: bool = kani::any();
    let face_raw: u8 = kani::any();
    let paid_raw: u8 = kani::any();
    kani::assume((2..=8).contains(&face_raw));
    kani::assume(paid_raw < face_raw);
    let face = face_raw as u128;
    let paid = paid_raw as u128;

    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    if deregister {
        header.materialized_portfolio_count = V16PodU64::new(1);
    }
    account_header.resolved_payout_receipt = ResolvedPayoutReceiptV16Account::from_runtime(
        &resolved_payout_receipt_for_proof(face, paid),
    );
    let count_before = header.materialized_portfolio_count.get();
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let account = PortfolioV16ViewMut::new(&mut account_header);

    assert_eq!(account.validate_with_market(&market.as_view()), Ok(()));
    let result = if deregister {
        market.deregister_empty_materialized_portfolio_not_atomic(&account.as_view())
    } else {
        market.register_empty_materialized_portfolio_not_atomic(&account.as_view())
    };

    kani::cover!(
        deregister && paid > 0,
        "unfinalized partially paid receipt blocks deregister"
    );
    kani::cover!(
        !deregister && paid > 0,
        "unfinalized partially paid receipt blocks register"
    );
    assert_eq!(result, Err(V16Error::LockActive));
    assert_eq!(
        market.header.materialized_portfolio_count.get(),
        count_before
    );
    assert_eq!(market.validate_shape(), Ok(()));
}

// The payout topup core must bridge the two states: a receipt that is fully
// paid at the terminal haircut rate is not finalized by full-face equality, but
// it is economically terminal. The production core must clear it, after which
// ordinary empty-account dematerialization is available.
#[kani::proof]
#[kani::unwind(56)]
#[kani::solver(cadical)]
fn proof_v16_terminal_resolved_receipt_core_restores_dematerialization() {
    let face_raw: u8 = kani::any();
    let residual_raw: u8 = kani::any();
    kani::assume((2..=6).contains(&face_raw));
    kani::assume((1..=5).contains(&residual_raw));
    kani::assume(residual_raw < face_raw);
    let face = face_raw as u128;
    let residual = residual_raw as u128;
    let total_bound_num = face * BOUND_SCALE;

    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    header.mode = 1; // Resolved
    header.vault = V16PodU128::new(residual);
    header.payout_snapshot_captured = 1;
    header.resolved_payout_ledger =
        ResolvedPayoutLedgerV16Account::from_runtime(&ResolvedPayoutLedgerV16 {
            snapshot_residual: residual,
            terminal_claim_exact_receipts_num: total_bound_num,
            terminal_claim_bound_unreceipted_num: 0,
            current_payout_rate_num: residual * BOUND_SCALE,
            current_payout_rate_den: total_bound_num,
            snapshot_slot: 1,
            payout_halted: false,
            finalized: false,
        });
    account_header.resolved_payout_receipt =
        ResolvedPayoutReceiptV16Account::from_runtime(&ResolvedPayoutReceiptV16 {
            present: true,
            prior_bound_contribution_num: total_bound_num,
            live_released_face_at_receipt: 0,
            terminal_positive_claim_face: face,
            paid_effective: residual,
            finalized: false,
        });

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    let residual_before = market.kani_residual();
    assert_eq!(
        kani_apply_resolved_payout_receipt_payment(
            account
                .header
                .resolved_payout_receipt
                .try_to_runtime()
                .unwrap(),
            0,
        )
        .unwrap(),
        account
            .header
            .resolved_payout_receipt
            .try_to_runtime()
            .unwrap()
    );
    assert_eq!(
        market.register_empty_materialized_portfolio_not_atomic(&account.as_view()),
        Err(V16Error::LockActive)
    );

    let paid_out = market
        .kani_claim_resolved_payout_topup_core_not_atomic(&mut account)
        .unwrap();
    let receipt = account
        .header
        .resolved_payout_receipt
        .try_to_runtime()
        .unwrap();

    market
        .register_empty_materialized_portfolio_not_atomic(&account.as_view())
        .unwrap();

    kani::cover!(
        face > 3 && residual > 1,
        "terminal public topup clears a symbolic haircut receipt"
    );
    assert_eq!(paid_out, 0);
    assert!(!receipt.present);
    assert_eq!(market.header.materialized_portfolio_count.get(), 1);
    assert_eq!(market.kani_residual(), residual_before);
}

// Finding D: an insolvent resolved market's winner receipt can never finalize.
// Resolved close records terminal_positive_claim_face = FULL positive PnL, and the only
// finalize site (plus the receipt validator) require paid_effective == that full face.
// Under a haircut (snapshot_residual < total bound => payout rate < 1), the receipt is
// paid at most floor(face * rate) < face, so paid_effective never reaches face: the
// receipt stays present && !finalized forever, the portfolio can never dematerialize,
// and the market (insurance + earnings + residual vault + rent) is stranded permanently.
// Fix: once a receipt is fully paid at the TERMINAL rate (no unreceipted bound remains,
// so the rate can no longer rise), it is fully diluted -- the shortfall is unrecoverable
// bad debt, not an obligation -- so it is cleared, letting the portfolio close. RED until
// claim_resolved_payout_topup_not_atomic clears a fully-diluted-at-terminal receipt.
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
fn proof_v16_insolvent_resolved_receipt_clears_at_terminal_rate() {
    let face_raw: u8 = kani::any();
    let residual_raw: u8 = kani::any();
    kani::assume((2..=6).contains(&face_raw));
    kani::assume((1..=5).contains(&residual_raw));
    kani::assume(residual_raw < face_raw);
    let face = face_raw as u128;
    let residual = residual_raw as u128; // payout rate < 1 (insolvent haircut)
    let total_bound_num = face * BOUND_SCALE;

    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    header.mode = 1; // Resolved
    header.vault = V16PodU128::new(residual);
    header.payout_snapshot_captured = 1;
    header.resolved_payout_ledger =
        ResolvedPayoutLedgerV16Account::from_runtime(&ResolvedPayoutLedgerV16 {
            snapshot_residual: residual,
            terminal_claim_exact_receipts_num: total_bound_num,
            terminal_claim_bound_unreceipted_num: 0, // TERMINAL: rate can no longer rise
            current_payout_rate_num: residual * BOUND_SCALE,
            current_payout_rate_den: total_bound_num,
            snapshot_slot: 1,
            payout_halted: false,
            finalized: false,
        });
    // Receipt already paid its full terminal-rate entitlement: gross =
    // floor(face * (residual/face)) = residual, so claimable == 0, but paid_effective
    // (residual) can never equal the full face (face) under the haircut.
    account_header.resolved_payout_receipt =
        ResolvedPayoutReceiptV16Account::from_runtime(&ResolvedPayoutReceiptV16 {
            present: true,
            prior_bound_contribution_num: total_bound_num,
            live_released_face_at_receipt: 0,
            terminal_positive_claim_face: face,
            paid_effective: residual,
            finalized: false,
        });
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let residual_before = market.kani_residual();
    let mut account = PortfolioV16ViewMut::new(&mut account_header);

    let paid_out = market
        .kani_claim_resolved_payout_topup_core_not_atomic(&mut account)
        .unwrap();
    let receipt = account
        .header
        .resolved_payout_receipt
        .try_to_runtime()
        .unwrap();

    kani::cover!(
        face > 3 && residual > 1,
        "insolvent terminal-rate receipt-clear path reached for symbolic haircut"
    );
    assert_eq!(paid_out, 0); // nothing more is claimable at the terminal rate
                             // The fully-diluted receipt must be cleared so the portfolio can dematerialize,
                             // not left present-but-unfinalized (which strands the market forever).
    assert!(!receipt.present);
    assert_eq!(market.validate_shape(), Ok(()));
    assert_eq!(account.validate_with_market(&market.as_view()), Ok(()));
    // The terminal clear pays nothing (claimable is zero), so the junior
    // pool is untouched by receipt dematerialization.
    assert_eq!(market.kani_residual(), residual_before);
}

// Recoverable counterparty backing principal is a senior-side claim: the
// provider can withdraw it whenever the domain is fully backed, with no mode
// or payout-snapshot gate (withdraw_fresh_counterparty_backing_not_atomic).
// So it must NOT count in residual(), the junior positive-PnL payout pool —
// otherwise the resolved payout snapshot promises winners the same vault
// atoms the provider can still withdraw, and whichever party moves second is
// robbed or stranded (Finding-B class: residual() omitting a senior claim).
#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_residual_excludes_recoverable_counterparty_backing_principal() {
    let deposit_raw: u8 = kani::any();
    let surplus_raw: u8 = kani::any();
    kani::assume((1..=8).contains(&deposit_raw));
    kani::assume(surplus_raw <= 8);
    let deposit = deposit_raw as u128;
    let surplus = surplus_raw as u128;
    let (mut header, mut markets) = one_market_only_fixture();
    header.vault = V16PodU128::new(7 + surplus);
    header.c_tot = V16PodU128::new(7);
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    market.validate_shape().unwrap();
    let residual_before = market.kani_residual();
    assert_eq!(residual_before, surplus);

    // Engine-built deposit of recoverable provider principal.
    market
        .deposit_fresh_counterparty_backing_not_atomic(0, deposit, 10)
        .unwrap();

    kani::cover!(surplus_raw == 0, "covers zero junior surplus");
    kani::cover!(surplus_raw > 0, "covers positive junior surplus");
    // The deposit is withdrawable principal, not junior surplus: the junior
    // payout pool must be unchanged by it...
    assert_eq!(market.kani_residual(), residual_before);
    // ...and stay unchanged after the provider recovers the principal.
    market
        .withdraw_fresh_counterparty_backing_not_atomic(0, deposit)
        .unwrap();
    assert_eq!(market.kani_residual(), residual_before);
}

// Terminal realization of a source-backed claim (Issue-2 class) — KANI
// TRACTABILITY NOTE (flagged): every harness that enters the realize/convert
// path times out at the 900s solver budget, even fully concrete (the in-path
// account validation forces unwind 40 and the per-domain U256 credit math
// blows up the formula; a single concrete witness passed once at 658s and
// timed out on three re-runs — permanently on the budget line). The realize
// primitive therefore has NO direct Kani harness. Its coverage decomposes
// into already-passing proofs — consume-delta exactness
// (proof_v16_public_counterparty_lien_consume_creates_receivable_without_
// value_movement, proof_v16_counterparty_lien_consume_delta_is_receivable_
// exact_and_fail_closed, proof_v16_counterparty_credit_consumption_reports_
// atoms_not_scaled_backing), support caps (proof_v16_unliened_source_support_
// is_capped_by_realizable_backing, proof_v16_cross_account_source_support_
// sum_capped_by_shared_backing), the close-ledger partition equality
// (proof_v16_close_progress_ledger_residual_equation_is_enforced), and the
// expired-backing liveness witness (proof_v16_expired_backing_yields_zero_
// realizable_support_after_expiry) — while the end-to-end realize semantics
// (exact payout values, no-double-pay, idempotence, ordering, expiry
// liveness) are asserted at runtime by tests/backing_double_claim_fuzz.rs
// (5 randomized properties, 300 cases each) and the spec tests.

// Expiry-liveness primitive (wrapper finding 2026-06-10): the resolved-close
// realize step must not strand a source-backed winner whose backing has lapsed
// (bucket still Fresh but expiry_slot <= current_slot — nothing processes
// expiry in production). Querying realizable support against a past-expiry
// bucket returns Stale. The primitive that avoids the deadlock: expiring the
// lapsed bucket forfeits its principal (fresh_reserved -> 0), drops the domain
// credit rate to zero, and makes realizable support exactly zero — so the
// realize step falls through to the junior receipt path instead of reverting.
// The full close_resolved path is Kani-intractable; this pins the primitive.
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
fn proof_v16_expired_backing_yields_zero_realizable_support_after_expiry() {
    // CONCRETE WITNESS (flagged): any symbolic input here blows the solver
    // budget (the realizable-support query's per-domain U256 credit math on
    // top of the expire + audit-scan path). The symbolic surface is covered
    // end-to-end by backing_double_claim_fuzz::terminal_close_with_expired_
    // backing_does_not_strand; this witness pins the primitive exactly.
    // CLEAN-ROOM FIX (was a dead cover): backing/claim were welded to 2/3, so
    // cover!(backing == claim) was UNSATISFIABLE ("1 of 2 cover properties
    // satisfied") and the fully-backed expiry case was never exercised. A
    // symbolic selector drives BOTH concrete witnesses (under-backed and
    // fully-backed) so both covers become satisfiable (2/2), without making the
    // realizable-support math symbolic (which blows the solver budget). The
    // post-expiry assertions are forfeiture-state facts that hold for either
    // ratio (the lapsed backing is zeroed regardless of how much it was).
    let fully_backed: bool = kani::any();
    let backing: u128 = if fully_backed { 3 } else { 2 };
    let claim = 3u128;
    let backing_num = backing * BOUND_SCALE;
    let claim_num = claim * BOUND_SCALE;
    let expiry_slot = 5u64;
    let current_slot = 20u64; // strictly past expiry

    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    let market_id = markets[0].engine.asset.market_id.get();
    header.current_slot = V16PodU64::new(current_slot);
    header.slot_last = V16PodU64::new(current_slot);
    header.vault = V16PodU128::new(backing);
    header.pnl_pos_tot = V16PodU128::new(claim);
    header.pnl_matured_pos_tot = V16PodU128::new(claim);
    header.pnl_pos_bound_tot = V16PodU128::new(claim);
    header.pnl_pos_bound_tot_num = V16PodU128::new(claim_num);
    header.source_claim_bound_total_num = V16PodU128::new(claim_num);
    header.source_fresh_backing_total_num = V16PodU128::new(backing_num);
    // A lapsed bucket: still Fresh, but expiry_slot is in the past.
    markets[0].engine.backing_long = BackingBucketV16Account::from_runtime(&BackingBucketV16 {
        market_id,
        fresh_unliened_backing_num: backing_num,
        expiry_slot,
        status: BackingBucketStatusV16::Fresh,
        ..BackingBucketV16::EMPTY
    });
    markets[0].engine.source_credit_long =
        SourceCreditStateV16Account::from_runtime(&SourceCreditStateV16 {
            positive_claim_bound_num: claim_num,
            exact_positive_claim_num: claim_num,
            fresh_reserved_backing_num: backing_num,
            credit_rate_num: (backing_num * CREDIT_RATE_SCALE / claim_num).min(CREDIT_RATE_SCALE),
            ..SourceCreditStateV16::EMPTY
        });
    account_header.pnl = V16PodI128::new(claim as i128);
    account_header.source_domains[0].domain = V16PodU32::new(0);
    account_header.source_domains[0].source_claim_market_id = V16PodU64::new(market_id);
    account_header.source_domains[0].source_claim_bound_num = V16PodU128::new(claim_num);

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    // ASSERT (was assume — an all-or-nothing switch that made the whole proof
    // vacuously true whenever the fixture failed validation): the constructed
    // fixture is a well-formed engine state for both backing ratios.
    assert_eq!(market.validate_shape(), Ok(()));

    // Before expiry, the freshness validator rejects the lapsed bucket — this
    // is the Stale that would strand the close if the realize step queried it.
    assert_eq!(
        market.kani_validate_source_domain_ledger_current(0),
        Err(V16Error::Stale)
    );

    // Expire the lapsed bucket (the realize step's deadlock-avoidance move).
    market
        .expire_source_backing_bucket_not_atomic(0, current_slot)
        .unwrap();

    let source = market.markets[0]
        .engine
        .source_credit_long
        .try_to_runtime()
        .unwrap();
    let bucket = market.markets[0]
        .engine
        .backing_long
        .try_to_runtime()
        .unwrap();

    kani::cover!(backing < claim, "expiry covers under-backed claim");
    kani::cover!(backing == claim, "expiry covers fully-backed claim");
    // The principal is forfeited (bucket emptied, status Expired) ...
    assert_eq!(bucket.status, BackingBucketStatusV16::Expired);
    assert_eq!(bucket.fresh_unliened_backing_num, 0);
    assert_eq!(source.fresh_reserved_backing_num, 0);
    // ... the credit rate collapses to zero (no backing underwrites the claim) ...
    assert_eq!(source.credit_rate_num, 0);
    // ... the bucket is now current (no Stale) so the close can proceed ...
    assert_eq!(market.kani_validate_source_domain_ledger_current(0), Ok(()));
    // ... and realizable support is exactly zero -> realize falls through to the
    // junior receipt path (forfeited principal is now junior residual).
    assert_eq!(
        market
            .kani_account_unliened_source_realizable_support(
                &PortfolioV16View::new(&account_header),
                claim
            )
            .unwrap(),
        0
    );
}

// First-class engine API for granting source-attributed positive PnL (the
// wrapper previously shadow-implemented this op host-side). The value-neutral
// + aggregate-lockstep property is covered concretely by the unit test
// v16_grant_source_positive_pnl_attributes_claims_and_aggregates_in_lockstep:
// the symbolic Kani witness exceeds the 900s budget even fully concrete (the
// grant runs validate_with_market + a claim-bound-denominated rate recompute +
// validate_shape + validate_with_market — three heavy sweeps). The mode gate
// IS proven below (it errors before the heavy path).
// Mode gate: granting source PnL outside Live is rejected before mutation.
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
fn proof_v16_grant_source_positive_pnl_rejects_non_live() {
    let resolved: bool = kani::any();
    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    header.mode = if resolved { 1 } else { 2 }; // Resolved | Recovery
    header.resolved_slot = V16PodU64::new(1);
    if !resolved {
        header.recovery_reason = V16OptionalRecoveryReasonAccount::from_runtime(Some(
            PermissionlessRecoveryReasonV16::BelowProgressFloor,
        ));
    }
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);

    let result = market.add_account_source_positive_pnl_not_atomic(&mut account, 0, 1);

    kani::cover!(resolved, "non-live grant rejection covers Resolved");
    kani::cover!(!resolved, "non-live grant rejection covers Recovery");
    assert_eq!(result, Err(V16Error::LockActive));
    assert_eq!(account.header.pnl.get(), 0);
    assert_eq!(market.header.pnl_pos_tot.get(), 0);
    assert_eq!(market.header.source_claim_bound_total_num.get(), 0);
}

// Conservation core for credit_account_from_insurance: the public entrypoint
// wraps this delta with full account/market validation, which is covered by
// the API itself and is too expensive for a focused 300s proof. This harness
// pins the LoF-relevant invariant directly: spending only UNBUDGETED insurance
// moves insurance -> account capital/c_tot with vault flat, the senior stack
// c_tot + insurance unchanged, and the junior residual pool untouched.
#[kani::proof]
#[kani::unwind(18)]
#[kani::solver(cadical)]
fn proof_v16_credit_account_from_insurance_is_value_neutral_and_pool_isolated() {
    let amount_raw: u8 = kani::any();
    let budget_raw: u8 = kani::any();
    let unbudgeted_raw: u8 = kani::any();
    let c_tot_raw: u8 = kani::any();
    let capital_raw: u8 = kani::any();
    let surplus_raw: u8 = kani::any();
    kani::assume((1..=8).contains(&amount_raw));
    kani::assume(budget_raw <= 4);
    kani::assume(unbudgeted_raw <= 4);
    kani::assume(c_tot_raw <= 8);
    kani::assume(capital_raw <= 8);
    kani::assume(surplus_raw <= 4);
    let amount = amount_raw as u128;
    let budget_remaining = budget_raw as u128;
    let unbudgeted_surplus = unbudgeted_raw as u128;
    let c_tot = c_tot_raw as u128;
    let capital = capital_raw as u128;
    let surplus = surplus_raw as u128;
    let insurance = budget_remaining + unbudgeted_surplus + amount;
    let vault = c_tot + insurance + surplus;
    let senior_before = c_tot + insurance;
    let residual_before = vault - senior_before;

    let (next_insurance, next_c_tot, next_capital) =
        MarketGroupV16ViewMut::<u64>::kani_credit_account_from_insurance_delta(
            insurance,
            budget_remaining,
            c_tot,
            capital,
            amount,
        )
        .unwrap();

    let mut flow = TokenValueFlowProofV16::empty(vault, vault);
    flow.debit(TokenValueClassV16::InsuranceCapital, amount)
        .unwrap();
    flow.credit(TokenValueClassV16::AccountCapital, amount)
        .unwrap();
    assert_eq!(flow.validate(), Ok(()));

    kani::cover!(
        budget_remaining > 0,
        "insurance credit preserves budgeted insurance"
    );
    kani::cover!(
        surplus > 0,
        "insurance credit covers nonzero junior surplus present"
    );
    kani::cover!(amount > 1, "insurance credit covers nontrivial amount");
    assert_eq!(next_insurance, insurance - amount);
    assert_eq!(next_c_tot, c_tot + amount);
    assert_eq!(next_capital, capital + amount);
    assert!(next_insurance >= budget_remaining);
    assert_eq!(next_c_tot + next_insurance, senior_before);
    assert_eq!(vault - (next_c_tot + next_insurance), residual_before);
}

// reset_empty_asset_oracle_anchor re-anchors an EMPTY Active asset's oracle
// price triple. It is gated on the whole group having NO position or loss
// state (pnl_pos_tot, stale/negative counters, hlock/stress/loss flags,
// recovery, per-asset OI/loss all zero), so re-anchoring can reprice nothing.
// The transition must move NO quote value (vault/c_tot/insurance/residual all
// flat) and touch ONLY the asset price triple + the slot clocks: an oracle
// re-anchor on an idle market can neither mint nor move funds and cannot
// affect any account's PnL (there is none).
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
fn proof_v16_reset_empty_asset_oracle_anchor_is_value_neutral_and_idle_gated() {
    let new_price_raw: u16 = kani::any();
    let c_tot_raw: u8 = kani::any();
    let insurance_raw: u8 = kani::any();
    let surplus_raw: u8 = kani::any();
    kani::assume((1..=10_000).contains(&new_price_raw));
    let new_price = new_price_raw as u64;
    let c_tot = c_tot_raw as u128;
    let insurance = insurance_raw as u128;
    let surplus = surplus_raw as u128;
    let now_slot = 7u64;

    let (mut header, mut markets, _) = one_market_direct_view_fixture();
    header.c_tot = V16PodU128::new(c_tot);
    header.insurance = V16PodU128::new(insurance);
    header.vault = V16PodU128::new(c_tot + insurance + surplus);
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    kani::assume(market.validate_shape() == Ok(()));
    let vault_before = market.header.vault.get();
    let c_tot_before = market.header.c_tot.get();
    let insurance_before = market.header.insurance.get();
    let residual_before = market.kani_residual();

    market
        .reset_empty_asset_oracle_anchor_not_atomic(0, new_price, now_slot)
        .unwrap();
    let asset = market.markets[0].engine.asset.try_to_runtime().unwrap();

    kani::cover!(
        surplus > 0,
        "oracle re-anchor covers nonzero junior surplus present"
    );
    kani::cover!(
        new_price_raw > 1,
        "oracle re-anchor covers nontrivial price"
    );
    // The price triple is re-anchored to the authenticated price; clocks advance.
    assert_eq!(asset.raw_oracle_target_price, new_price);
    assert_eq!(asset.effective_price, new_price);
    assert_eq!(asset.fund_px_last, new_price);
    assert_eq!(asset.slot_last, now_slot);
    assert_eq!(market.header.current_slot.get(), now_slot);
    // No quote value moves; junior pool untouched.
    assert_eq!(market.header.vault.get(), vault_before);
    assert_eq!(market.header.c_tot.get(), c_tot_before);
    assert_eq!(market.header.insurance.get(), insurance_before);
    assert_eq!(market.kani_residual(), residual_before);
    assert_eq!(market.validate_shape(), Ok(()));
}

// The idle gate is load-bearing: re-anchoring is rejected the moment ANY
// junior PnL exists in the group (the canonical "positions exist" signal),
// before any mutation — so an oracle re-anchor can never reprice live PnL.
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
fn proof_v16_reset_empty_asset_oracle_anchor_rejects_when_pnl_present() {
    let pnl_raw: u8 = kani::any();
    kani::assume(pnl_raw > 0);
    let pnl = pnl_raw as u128;

    let (mut header, mut markets, _) = one_market_direct_view_fixture();
    // A live positive-PnL claim makes the group non-idle.
    header.vault = V16PodU128::new(pnl);
    header.pnl_pos_tot = V16PodU128::new(pnl);
    header.pnl_matured_pos_tot = V16PodU128::new(pnl);
    header.pnl_pos_bound_tot = V16PodU128::new(pnl);
    header.pnl_pos_bound_tot_num = V16PodU128::new(pnl * BOUND_SCALE);
    let raw_before = markets[0]
        .engine
        .asset
        .try_to_runtime()
        .unwrap()
        .raw_oracle_target_price;
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);

    let result = market.reset_empty_asset_oracle_anchor_not_atomic(0, 123, 7);

    kani::cover!(
        pnl_raw > 1,
        "oracle re-anchor rejection covers nontrivial live pnl"
    );
    assert_eq!(result, Err(V16Error::LockActive));
    // Rejected before mutation: price unchanged.
    assert_eq!(
        market.markets[0]
            .engine
            .asset
            .try_to_runtime()
            .unwrap()
            .raw_oracle_target_price,
        raw_before
    );
}

// End-to-end proof for the public provider-earnings withdrawal (only the
// arithmetic delta was proven; this pins the full transition). Withdrawing
// earned utilization fees exits vault and the earnings class in lockstep:
// senior capital, insurance, backing principal, and the junior residual pool
// are all untouched — a provider can extract exactly what was earned and
// nothing else.
#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_public_backing_provider_earnings_withdraw_is_earnings_scoped_and_pool_neutral() {
    let earnings_raw: u8 = kani::any();
    let withdraw_raw: u8 = kani::any();
    let surplus_raw: u8 = kani::any();
    kani::assume((1..=8).contains(&earnings_raw));
    kani::assume((1..=8).contains(&withdraw_raw));
    kani::assume(withdraw_raw <= earnings_raw);
    kani::assume(surplus_raw <= 8);
    let earnings = earnings_raw as u128;
    let withdraw = withdraw_raw as u128;
    let surplus = surplus_raw as u128;

    let (mut header, mut markets) = one_market_only_fixture();
    let market_id = markets[0].engine.asset.market_id.get();
    header.vault = V16PodU128::new(earnings + surplus);
    header.backing_provider_earnings_total = V16PodU128::new(earnings);
    markets[0].engine.backing_long = BackingBucketV16Account::from_runtime(&BackingBucketV16 {
        market_id,
        utilization_fee_earnings: earnings,
        status: BackingBucketStatusV16::Expired,
        ..BackingBucketV16::EMPTY
    });
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    kani::assume(market.validate_shape() == Ok(()));
    let residual_before = market.kani_residual();

    market
        .withdraw_backing_provider_earnings_not_atomic(0, withdraw)
        .unwrap();
    let bucket = market.markets[0]
        .engine
        .backing_long
        .try_to_runtime()
        .unwrap();

    kani::cover!(
        withdraw < earnings,
        "earnings withdraw covers partial extraction"
    );
    kani::cover!(
        withdraw == earnings,
        "earnings withdraw covers full extraction"
    );
    kani::cover!(
        surplus > 0,
        "earnings withdraw covers junior surplus present"
    );
    // vault and earnings fall in lockstep; nothing else moves.
    assert_eq!(market.header.vault.get(), earnings + surplus - withdraw);
    assert_eq!(bucket.utilization_fee_earnings, earnings - withdraw);
    assert_eq!(
        market.header.backing_provider_earnings_total.get(),
        earnings - withdraw
    );
    assert_eq!(market.header.c_tot.get(), 0);
    assert_eq!(market.header.insurance.get(), 0);
    assert_eq!(market.header.source_fresh_backing_total_num.get(), 0);
    // Junior-pool isolation: the provider extracts earned fees only, never
    // the haircut/payout pool.
    assert_eq!(market.kani_residual(), residual_before);
    assert_eq!(market.validate_shape(), Ok(()));
}

// The public resolved-payout topup wrapper gates on Resolved mode + a captured
// snapshot BEFORE any mutation: claiming against a Live market or before
// snapshot capture must reject with LockActive and change nothing.
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
fn proof_v16_claim_resolved_payout_topup_preflight_rejects_unready() {
    let live_mode: bool = kani::any();
    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    if !live_mode {
        // Resolved but snapshot NOT captured.
        header.mode = 1;
        header.resolved_slot = V16PodU64::new(1);
    }
    let vault_before = header.vault.get();
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);

    let result = market.claim_resolved_payout_topup_not_atomic(&mut account);

    kani::cover!(live_mode, "topup preflight covers Live-mode rejection");
    kani::cover!(!live_mode, "topup preflight covers pre-snapshot rejection");
    assert_eq!(result, Err(V16Error::LockActive));
    assert_eq!(market.header.vault.get(), vault_before);
    assert_eq!(account.header.capital.get(), 0);
}

// cure_and_cancel_close preflight: curing a forced close is allowed ONLY for
// an ACTIVE close ledger with no irreversible progress. An account with no
// close in progress (the overwhelmingly common state) must reject with
// LockActive before the heavy refresh path runs — and before any mutation —
// so the cure entrypoint cannot be used as a free refresh/settle or touch
// value on accounts that have nothing to cure. (The cure body itself routes
// through full_account_refresh, which is Kani-intractable; its value
// semantics are covered by the wrapper's runtime suite.)
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
fn proof_v16_cure_and_cancel_close_rejects_without_active_close() {
    let deposit_raw: u8 = kani::any();
    let deposit = deposit_raw as u128;
    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    let vault_before = header.vault.get();
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);

    let result = market.cure_and_cancel_close_not_atomic(&mut account, deposit);

    kani::cover!(
        deposit > 0,
        "cure preflight covers nonzero optional deposit"
    );
    kani::cover!(deposit == 0, "cure preflight covers zero deposit");
    assert_eq!(result, Err(V16Error::LockActive));
    // Rejected before mutation: no value or account state moved.
    assert_eq!(market.header.vault.get(), vault_before);
    assert_eq!(account.header.capital.get(), 0);
    assert_eq!(account.header.cancel_deposit_escrow.get(), 0);
}

// ============ INTRACTABLE-TIER GRIND: realize-body verdict ============
// CONCLUSIVELY INTRACTABLE (full elimination, 2026-06-10): the terminal-
// realization full-backing witness exceeds the 900s budget under EVERY
// reduction, separately and combined:
//   (a) fully concrete inputs                              -> TIMEOUT 900s
//   (b) + in-path account validation stubbed via
//       #[kani::stub] (mechanism works; generic impl
//       methods need an early-bound `'a: 'a` replacement)  -> TIMEOUT 900s
//   (c) + SAT-solver swap                                  -> kissat 4.x
//       rejects CBMC's CNF dialect (tooling dead-end)
//   (d) BOUND_SCALE/CREDIT_RATE_SCALE shrunk to 8 under
//       cfg(kani) (arithmetic-width hypothesis)            -> >600s
//   (e) (b) and (d) combined                               -> >500s
//   (f) V16_MAX_PORTFOLIO_ASSETS_N shrunk 16 -> 4 under
//       cfg(kani) (reduced-leg struct profile)            -> TIMEOUT 900s
//       (and the trade-fill seam ALSO timed out under it)
// Conclusion: the bulk is the lien create/consume + claim-burn +
// encumbrance-proof machinery itself — the very subject of the proof — not
// the validators, not the 1e12 arithmetic width, not the solver, not the
// leg-array cardinality. A kani::any() inductive-state formulation does not
// escape this either.
//   (g) kani function contracts (-Z function-contracts, proof_for_contract +
//       stub_verified): the MECHANISM works end-to-end — a leaf contract on
//       the lien-sizing fn checked in 11 VCCs with sub-second solves — but the
//       route fails twice over: (i) the flag slows kani-compiler ~5x crate-
//       wide (13-24 min compile per invocation), incompatible with per-proof
//       isolated runs; (ii) stub_verified HAVOCS the contracted returns
//       (ensures-constrained symbolic values), re-introducing symbolic flows
//       through the rate-division killers downstream — the composed realize
//       witness still exceeded 37 min of solver time with both leaves
//       contracted and the validator stubbed. Composition trades away the
//       concreteness that makes witnesses tractable. Route closed.
// Remaining untested: all reductions combined at once (legs=4 + scales=8 +
// stubbed validators) — research-grade. The realize/cure/convert bodies stay covered by the
// component proofs (consume-delta exactness, support caps, partition
// equality) and the runtime suites (backing_double_claim_fuzz, spec tests).

// ============ INTRACTABLE-TIER GRIND: trade-seam verdict ============
// The pure-execute trade step (apply_trade_after_refresh — the batch
// refactor's settle-free fill: preflight, fee, position deltas, OI) ALSO
// exceeds the 900s budget fully concrete on two flat accounts (zero fee, no
// recertify). Combined with the realize-body elimination above, the verdict
// is uniform: every transition that mutates an account pair or source-claim
// state through the real 16-slot engine structs is beyond the budget; the
// tractable frontier is single-account ops without claim machinery plus all
// gate/rejection paths. A kani wrapper for the seam
// (kani_apply_trade_after_refresh_not_atomic) is kept in src/v16.rs for
// future attempts (e.g. if the account struct ever gains a cfg(kani)
// reduced-leg profile). Trade-fill semantics remain covered by the spec
// tests (v16_risk_increasing_trade_*, batch tests) and component proofs
// (trade_preflight_risk_gate, checked_trade_fee, position-delta and OI
// proofs).

// ============ P2: TWO-OP INTERACTION WITNESSES ============
// Sequence invariants that single-op proofs cannot see. Each op is
// individually proven cheap; the witness asserts the INTERACTION property.

// P2-1: deposit then withdraw of the same amount is an exact round trip —
// no dust, no residue, every header and account quantity restored, the
// junior pool untouched at both steps.
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
fn proof_v16_seq_deposit_withdraw_round_trip_is_exact() {
    let start_capital_raw: u8 = kani::any();
    let insurance_raw: u8 = kani::any();
    let surplus_raw: u8 = kani::any();
    let amount_raw: u8 = kani::any();
    kani::assume(start_capital_raw <= 8);
    kani::assume(insurance_raw <= 8);
    kani::assume(surplus_raw <= 8);
    kani::assume(amount_raw >= 1 && amount_raw <= 8);
    let start_capital = start_capital_raw as u128;
    let insurance = insurance_raw as u128;
    let surplus = surplus_raw as u128;
    let amount = amount_raw as u128;
    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    let vault_before = start_capital + insurance + surplus;
    header.c_tot = V16PodU128::new(start_capital);
    header.insurance = V16PodU128::new(insurance);
    header.vault = V16PodU128::new(vault_before);
    account_header.capital = V16PodU128::new(start_capital);
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    let residual_before = market.kani_residual();

    market.deposit_not_atomic(&mut account, amount).unwrap();
    let residual_mid = market.kani_residual();
    market.withdraw_not_atomic(&mut account, amount).unwrap();

    kani::cover!(
        amount > 0 && surplus > 0,
        "round trip covers junior surplus present"
    );
    assert_eq!(market.header.vault.get(), vault_before);
    assert_eq!(market.header.c_tot.get(), start_capital);
    assert_eq!(market.header.insurance.get(), insurance);
    assert_eq!(account.header.capital.get(), start_capital);
    assert_eq!(residual_mid, residual_before);
    assert_eq!(market.kani_residual(), residual_before);
    assert_eq!(market.validate_shape(), Ok(()));
}

// P2-2: two successive fee charges compound exactly — insurance gains the
// sum, capital loses the sum, vault flat throughout, pool untouched.
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
fn proof_v16_seq_two_fees_compound_exactly() {
    let capital_raw: u8 = kani::any();
    let f1_raw: u8 = kani::any();
    let f2_raw: u8 = kani::any();
    kani::assume(capital_raw <= 8);
    kani::assume(f1_raw <= 4 && f2_raw <= 4);
    let capital = capital_raw as u128;
    let f1 = f1_raw as u128;
    let f2 = f2_raw as u128;
    kani::assume(f1 + f2 <= capital);
    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    header.c_tot = V16PodU128::new(capital);
    header.vault = V16PodU128::new(capital);
    account_header.capital = V16PodU128::new(capital);
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    let residual_before = market.kani_residual();

    let c1 = market
        .kani_charge_account_fee_current_not_atomic(&mut account, f1)
        .unwrap();
    let c2 = market
        .kani_charge_account_fee_current_not_atomic(&mut account, f2)
        .unwrap();

    kani::cover!(f1 > 0 && f2 > 0, "two-fee covers both nonzero");
    assert_eq!(c1, f1);
    assert_eq!(c2, f2);
    assert_eq!(market.header.insurance.get(), f1 + f2);
    assert_eq!(account.header.capital.get(), capital - f1 - f2);
    assert_eq!(market.header.c_tot.get(), capital - f1 - f2);
    assert_eq!(market.header.vault.get(), capital);
    assert_eq!(market.kani_residual(), residual_before);
    assert_eq!(market.validate_shape(), Ok(()));
}

// P2-3: two successive cranks — clocks are monotone, the second crank moves
// no value either, and the pool stays untouched across the pair.
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
fn proof_v16_seq_double_crank_is_monotone_and_value_flat() {
    let s1_raw: u8 = kani::any();
    let s2_raw: u8 = kani::any();
    kani::assume((1..=3).contains(&s1_raw));
    kani::assume(s2_raw >= s1_raw && s2_raw <= 5);
    let s1 = s1_raw as u64;
    let s2 = s2_raw as u64;
    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    let c_tot = 3u128;
    header.vault = V16PodU128::new(c_tot + 2);
    header.c_tot = V16PodU128::new(c_tot);
    header.insurance = V16PodU128::new(1);
    account_header.capital = V16PodU128::new(c_tot);
    let vault_before = header.vault;
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let residual_before = market.kani_residual();
    let mut account = PortfolioV16ViewMut::new(&mut account_header);

    let effective_price = market.markets[0]
        .engine
        .asset
        .try_to_runtime()
        .unwrap()
        .effective_price;
    for now_slot in [s1, s2] {
        market
            .kani_permissionless_crank(
                &mut account,
                PermissionlessCrankRequestV16 {
                    now_slot,
                    asset_index: 0,
                    effective_price,
                    funding_rate_e9: 0,
                    action: PermissionlessCrankActionV16::Refresh,
                },
            )
            .unwrap();
    }
    let asset = market.markets[0].engine.asset.try_to_runtime().unwrap();

    kani::cover!(s2 > s1, "double crank covers forward time");
    kani::cover!(s2 == s1, "double crank covers same-slot replay");
    // Clock monotone and bounded by request; no value moved by either crank.
    assert!(asset.slot_last <= s2);
    assert_eq!(market.header.vault, vault_before);
    assert_eq!(market.header.c_tot.get(), c_tot);
    assert_eq!(market.header.insurance.get(), 1);
    assert_eq!(market.kani_residual(), residual_before);
    assert_eq!(market.validate_shape(), Ok(()));
}

// P2-4: budget credit then domain-insurance withdrawal — the caps COMPOSE:
// withdrawal is bounded by the just-set budget, debits vault/insurance/
// budget in lockstep, and the global remaining total tracks exactly.
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
fn proof_v16_seq_budget_credit_then_withdraw_caps_compose() {
    let budget_raw: u8 = kani::any();
    let withdraw_raw: u8 = kani::any();
    kani::assume(budget_raw >= 1 && budget_raw <= 6);
    kani::assume(withdraw_raw >= 1 && withdraw_raw <= 6);
    let budget = budget_raw as u128;
    let withdraw = withdraw_raw as u128;
    kani::assume(withdraw <= budget);
    let (mut header, mut markets) = one_market_only_fixture();
    header.vault = V16PodU128::new(budget);
    header.insurance = V16PodU128::new(budget);
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let residual_before = market.kani_residual();

    market
        .credit_domain_insurance_budget_not_atomic(0, budget)
        .unwrap();
    market
        .withdraw_domain_insurance_not_atomic(0, withdraw)
        .unwrap();

    kani::cover!(withdraw < budget, "cap composition covers partial");
    kani::cover!(withdraw == budget, "cap composition covers full drain");
    assert_eq!(market.header.vault.get(), budget - withdraw);
    assert_eq!(market.header.insurance.get(), budget - withdraw);
    assert_eq!(
        market.markets[0].engine.insurance_domain_budget_long.get(),
        budget - withdraw
    );
    assert_eq!(
        market.header.insurance_domain_budget_remaining_total.get(),
        budget - withdraw
    );
    assert_eq!(market.kani_residual(), residual_before);
    assert_eq!(market.validate_shape(), Ok(()));
}

// ============ SPEC #21: CLOSE-OWNERSHIP EXCLUSION (the engine's mechanism) ============
// The spec text describes priority-based preemption (ClosePriority tuple); the
// ENGINE implements a simpler, stronger discipline: exclusive per-domain
// barriers (a second close beginning in an occupied domain rejects with
// LockActive before mutation), one active close per account, monotone
// close_id identity, and bounded lifetime via max_close_slot. Hold-and-wait
// is impossible (each close holds exactly one domain barrier) and livelock is
// impossible (exclusion, not comparison). These proofs pin that mechanism.

// A second close beginning in an OCCUPIED domain rejects before any mutation:
// the barrier is exclusive, so cross-close contention cannot interleave.
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
fn proof_v16_close_begin_rejects_occupied_domain_before_mutation() {
    let gross_raw: u8 = kani::any();
    kani::assume(gross_raw >= 1 && gross_raw <= 8);
    let gross = gross_raw as u128;
    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    markets[0].engine.pending_domain_loss_barrier_long = V16PodU64::new(1);
    account_header.last_fee_slot = V16PodU64::new(1);
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);

    let result = market.kani_begin_close_progress_ledger(&mut account, 0, SideV16::Long, gross);

    kani::cover!(true, "occupied-domain begin rejection reached");
    assert_eq!(result, Err(V16Error::LockActive));
    // No mutation: ledger still empty, barrier unchanged.
    let ledger = account.header.close_progress.try_to_runtime().unwrap();
    assert!(!ledger.active);
    assert_eq!(
        market.markets[0]
            .engine
            .pending_domain_loss_barrier_long
            .get(),
        1
    );
}

// A successful begin takes the barrier 0 -> 1 and stamps the IMMUTABLE close
// identity exactly: close_id monotone (old+1, min 1), residual == gross,
// drift reference == current slot, lifetime == current + configured max.
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
fn proof_v16_close_begin_takes_barrier_and_stamps_immutable_identity() {
    let gross_raw: u8 = kani::any();
    let prior_id_raw: u8 = kani::any();
    kani::assume(gross_raw >= 1 && gross_raw <= 8);
    let gross = gross_raw as u128;
    let prior_id = (prior_id_raw as u64).max(1);
    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    account_header.last_fee_slot = V16PodU64::new(1);
    // prior INERT (canceled) ledger carries the close_id watermark; the inert
    // shape requires canceled + zero progress (validate_close_progress rules).
    account_header.close_progress =
        CloseProgressLedgerV16Account::from_runtime(&CloseProgressLedgerV16 {
            canceled: true,
            close_id: prior_id,
            asset_index: 0,
            market_id: markets[0].engine.asset.market_id.get(),
            domain_side: SideV16::Long,
            ..CloseProgressLedgerV16::EMPTY
        });
    let current_slot = header.current_slot.get();
    let max_life = header.config.max_bankrupt_close_lifetime_slots.get();
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);

    market
        .kani_begin_close_progress_ledger(&mut account, 0, SideV16::Long, gross)
        .unwrap();

    let ledger = account.header.close_progress.try_to_runtime().unwrap();
    kani::cover!(prior_id > 0, "begin covers nonzero prior close_id");
    assert!(ledger.active && !ledger.finalized && !ledger.canceled);
    // close identity is strictly monotone: a new close NEVER reuses an id.
    assert_eq!(ledger.close_id, prior_id + 1);
    assert!(ledger.close_id > prior_id);
    assert_eq!(ledger.gross_loss_at_close_start, gross);
    assert_eq!(ledger.residual_remaining, gross);
    assert_eq!(ledger.drift_reference_slot, current_slot);
    assert_eq!(ledger.max_close_slot, current_slot + max_life);
    // the domain barrier is now held exclusively
    assert_eq!(
        market.markets[0]
            .engine
            .pending_domain_loss_barrier_long
            .get(),
        1
    );
}

// An account with an ACTIVE close cannot begin another (one close per
// account): rejects with LockActive before mutation.
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
fn proof_v16_close_begin_rejects_account_with_active_close() {
    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    account_header.last_fee_slot = V16PodU64::new(1);
    let mut active = CloseProgressLedgerV16::EMPTY;
    active.active = true;
    active.close_id = 1;
    active.gross_loss_at_close_start = 3;
    active.residual_remaining = 3;
    active.market_id = markets[0].engine.asset.market_id.get();
    active.max_close_slot = 100;
    account_header.close_progress = CloseProgressLedgerV16Account::from_runtime(&active);
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);

    let result = market.kani_begin_close_progress_ledger(&mut account, 0, SideV16::Long, 5);

    kani::cover!(true, "active-close begin rejection reached");
    assert_eq!(result, Err(V16Error::LockActive));
    let ledger = account.header.close_progress.try_to_runtime().unwrap();
    assert_eq!(ledger.close_id, 1);
    assert_eq!(ledger.gross_loss_at_close_start, 3);
}

// ============ SPEC #19/#24: CLOSE-LIFECYCLE EXIT REJECTIONS ============

// #19 (pending obligations survive exit): an account with an ACTIVE close
// ledger cannot withdraw — the pending residual obligation pins the
// account's value until cured, finalized, or safely canceled.
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
fn proof_v16_withdraw_rejects_while_close_active() {
    let amount_raw: u8 = kani::any();
    kani::assume((1..=4).contains(&amount_raw));
    let amount = amount_raw as u128;
    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    header.vault = V16PodU128::new(5);
    header.c_tot = V16PodU128::new(5);
    account_header.capital = V16PodU128::new(5);
    account_header.last_fee_slot = V16PodU64::new(1);
    let mut active = CloseProgressLedgerV16::EMPTY;
    active.active = true;
    active.close_id = 1;
    active.asset_index = 0;
    active.domain_side = SideV16::Long;
    active.market_id = markets[0].engine.asset.market_id.get();
    active.gross_loss_at_close_start = 3;
    active.residual_remaining = 3;
    active.drift_reference_slot = 1;
    active.max_close_slot = 100;
    account_header.close_progress = CloseProgressLedgerV16Account::from_runtime(&active);
    markets[0].engine.pending_domain_loss_barrier_long = V16PodU64::new(1);
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);

    let result = market.withdraw_not_atomic(&mut account, amount);

    kani::cover!(true, "active-close withdraw rejection reached");
    assert!(result.is_err());
    // No value moved.
    assert_eq!(market.header.vault.get(), 5);
    assert_eq!(market.header.c_tot.get(), 5);
    assert_eq!(account.header.capital.get(), 5);
}

// #24 (residual durability before clear): a close ledger with residual still
// outstanding cannot pass the canceled/inert validation shape — cancel
// requires no irreversible progress AND a fully intact (un-booked) residual,
// so exposure can never be cleared with residual silently dropped.
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
fn proof_v16_close_cancel_shape_rejects_dropped_residual() {
    let booked_raw: u8 = kani::any();
    kani::assume((1..=4).contains(&booked_raw));
    let booked = booked_raw as u128;
    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    account_header.last_fee_slot = V16PodU64::new(1);
    // A "canceled" ledger that pretends to drop a partially-booked residual:
    // irreversible progress (b_loss_booked > 0) with canceled status must be
    // rejected by the ledger validator.
    let mut bad = CloseProgressLedgerV16::EMPTY;
    bad.canceled = true;
    bad.close_id = 1;
    bad.asset_index = 0;
    bad.market_id = markets[0].engine.asset.market_id.get();
    bad.gross_loss_at_close_start = 4;
    bad.b_loss_booked = booked;
    bad.residual_remaining = 4 - booked;
    account_header.close_progress = CloseProgressLedgerV16Account::from_runtime(&bad);
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let account = PortfolioV16ViewMut::new(&mut account_header);

    let result = account.validate_with_market(&market.as_view());

    kani::cover!(true, "dropped-residual cancel shape reached");
    assert!(result.is_err());
    let _ = &mut market;
}

// ============ E2E EXACT-FRAME LATTICE ============
// The end-to-end strengthening: per public op, the ENTIRE post-state equals
// the pre-state EXCEPT the op's declared deltas — proven by whole-struct
// equality (kani_eq_* index-loop helpers; never derived PartialEq/memcmp).
// This upgrades "the declared deltas move correctly" (the conservation
// lattice) to "and NOTHING else moves anywhere in the state" — the frame
// half of the no-steal theorem.

// deposit: exactly {vault, c_tot} on the header, {capital} on the account;
// every other header field, the whole market slot, and every other account
// field (all 16 legs, all 4 source domains, bitmap, certs, ledgers) frozen.
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
fn proof_v16_frame_deposit_touches_only_declared_state() {
    let cap_raw: u8 = kani::any();
    let amt_raw: u8 = kani::any();
    kani::assume(cap_raw <= 8);
    kani::assume(amt_raw >= 1 && amt_raw <= 8);
    let cap = cap_raw as u128;
    let amt = amt_raw as u128;
    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    header.vault = V16PodU128::new(cap);
    header.c_tot = V16PodU128::new(cap);
    account_header.capital = V16PodU128::new(cap);
    let h0 = header;
    let s0 = markets[0].engine;
    let a0 = account_header;
    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        let mut account = PortfolioV16ViewMut::new(&mut account_header);
        market.deposit_not_atomic(&mut account, amt).unwrap();
    }
    kani::cover!(true, "deposit frame witness reached");
    let mut eh = h0;
    eh.vault = V16PodU128::new(cap + amt);
    eh.c_tot = V16PodU128::new(cap + amt);
    assert!(kani_eq_market_group_v16_header_account(&eh, &header));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &s0,
        &markets[0].engine
    ));
    let mut ea = a0;
    ea.capital = V16PodU128::new(cap + amt);
    assert!(kani_eq_portfolio_account_v16_account(&ea, &account_header));
}

// withdraw: the exact inverse frame.
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
fn proof_v16_frame_withdraw_touches_only_declared_state() {
    let cap_raw: u8 = kani::any();
    let amt_raw: u8 = kani::any();
    kani::assume(cap_raw >= 1 && cap_raw <= 8);
    kani::assume(amt_raw >= 1 && amt_raw <= cap_raw);
    let cap = cap_raw as u128;
    let amt = amt_raw as u128;
    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    header.vault = V16PodU128::new(cap);
    header.c_tot = V16PodU128::new(cap);
    account_header.capital = V16PodU128::new(cap);
    account_header.last_fee_slot = V16PodU64::new(1);
    let h0 = header;
    let s0 = markets[0].engine;
    let a0 = account_header;
    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        let mut account = PortfolioV16ViewMut::new(&mut account_header);
        market.withdraw_not_atomic(&mut account, amt).unwrap();
    }
    kani::cover!(amt < cap, "withdraw frame covers partial");
    let mut eh = h0;
    eh.vault = V16PodU128::new(cap - amt);
    eh.c_tot = V16PodU128::new(cap - amt);
    assert!(kani_eq_market_group_v16_header_account(&eh, &header));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &s0,
        &markets[0].engine
    ));
    let mut ea = a0;
    ea.capital = V16PodU128::new(cap - amt);
    assert!(kani_eq_portfolio_account_v16_account(&ea, &account_header));
}

// REJECTION frame (the atomicity half): a withdraw that fails the capital
// gate leaves EVERY byte of meaningful state untouched — Err => unchanged,
// over the whole structs, not just headline fields.
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
fn proof_v16_frame_overwithdraw_err_leaves_state_unchanged() {
    let cap_raw: u8 = kani::any();
    let amt_raw: u8 = kani::any();
    kani::assume(cap_raw <= 8);
    kani::assume(amt_raw > cap_raw && amt_raw <= 16);
    let cap = cap_raw as u128;
    let amt = amt_raw as u128;
    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    header.vault = V16PodU128::new(cap);
    header.c_tot = V16PodU128::new(cap);
    account_header.capital = V16PodU128::new(cap);
    account_header.last_fee_slot = V16PodU64::new(1);
    let h0 = header;
    let s0 = markets[0].engine;
    let a0 = account_header;
    let result = {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        let mut account = PortfolioV16ViewMut::new(&mut account_header);
        market.withdraw_not_atomic(&mut account, amt)
    };
    kani::cover!(true, "overwithdraw frame witness reached");
    assert!(result.is_err());
    assert!(kani_eq_market_group_v16_header_account(&h0, &header));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &s0,
        &markets[0].engine
    ));
    assert!(kani_eq_portfolio_account_v16_account(&a0, &account_header));
}

// domain-insurance deposit frame: exactly {vault, insurance,
// insurance_domain_budget_remaining_total} on the header and
// {insurance_domain_budget_long} on the slot.
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
fn proof_v16_frame_domain_insurance_deposit_touches_only_declared_state() {
    let amt_raw: u8 = kani::any();
    kani::assume(amt_raw >= 1 && amt_raw <= 8);
    let amt = amt_raw as u128;
    let (mut header, mut markets) = one_market_only_fixture();
    let h0 = header;
    let s0 = markets[0].engine;
    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        market.deposit_domain_insurance_not_atomic(0, amt).unwrap();
    }
    kani::cover!(true, "domain-insurance deposit frame reached");
    let mut eh = h0;
    eh.vault = V16PodU128::new(h0.vault.get() + amt);
    eh.insurance = V16PodU128::new(h0.insurance.get() + amt);
    eh.insurance_domain_budget_remaining_total =
        V16PodU128::new(h0.insurance_domain_budget_remaining_total.get() + amt);
    assert!(kani_eq_market_group_v16_header_account(&eh, &header));
    let mut es = s0;
    es.insurance_domain_budget_long = V16PodU128::new(s0.insurance_domain_budget_long.get() + amt);
    assert!(kani_eq_engine_asset_slot_v16_account(
        &es,
        &markets[0].engine
    ));
}

// domain-insurance withdraw frame: the exact inverse set, plus nothing else.
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
fn proof_v16_frame_domain_insurance_withdraw_touches_only_declared_state() {
    let amt_raw: u8 = kani::any();
    let fund_raw: u8 = kani::any();
    kani::assume(amt_raw >= 1 && amt_raw <= 8);
    kani::assume(fund_raw >= amt_raw && fund_raw <= 8);
    let amt = amt_raw as u128;
    let fund = fund_raw as u128;
    let (mut header, mut markets) = one_market_only_fixture();
    header.vault = V16PodU128::new(fund);
    header.insurance = V16PodU128::new(fund);
    header.insurance_domain_budget_remaining_total = V16PodU128::new(fund);
    markets[0].engine.insurance_domain_budget_long = V16PodU128::new(fund);
    let h0 = header;
    let s0 = markets[0].engine;
    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        market.withdraw_domain_insurance_not_atomic(0, amt).unwrap();
    }
    kani::cover!(amt < fund, "domain-insurance withdraw frame covers partial");
    let mut eh = h0;
    eh.vault = V16PodU128::new(fund - amt);
    eh.insurance = V16PodU128::new(fund - amt);
    eh.insurance_domain_budget_remaining_total = V16PodU128::new(fund - amt);
    assert!(kani_eq_market_group_v16_header_account(&eh, &header));
    let mut es = s0;
    es.insurance_domain_budget_long = V16PodU128::new(fund - amt);
    assert!(kani_eq_engine_asset_slot_v16_account(
        &es,
        &markets[0].engine
    ));
}

// provider-earnings withdraw frame: exactly {vault,
// backing_provider_earnings_total} on the header and the bucket's
// utilization_fee_earnings on the slot.
#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_frame_earnings_withdraw_touches_only_declared_state() {
    let amt_raw: u8 = kani::any();
    let earn_raw: u8 = kani::any();
    kani::assume(amt_raw >= 1 && amt_raw <= 8);
    kani::assume(earn_raw >= amt_raw && earn_raw <= 8);
    let amt = amt_raw as u128;
    let earn = earn_raw as u128;
    let (mut header, mut markets) = one_market_only_fixture();
    let market_id = markets[0].engine.asset.market_id.get();
    header.vault = V16PodU128::new(earn);
    header.backing_provider_earnings_total = V16PodU128::new(earn);
    markets[0].engine.backing_long = BackingBucketV16Account::from_runtime(&BackingBucketV16 {
        market_id,
        utilization_fee_earnings: earn,
        status: BackingBucketStatusV16::Expired,
        ..BackingBucketV16::EMPTY
    });
    let h0 = header;
    let s0 = markets[0].engine;
    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        market
            .withdraw_backing_provider_earnings_not_atomic(0, amt)
            .unwrap();
    }
    kani::cover!(amt < earn, "earnings withdraw frame covers partial");
    let mut eh = h0;
    eh.vault = V16PodU128::new(earn - amt);
    eh.backing_provider_earnings_total = V16PodU128::new(earn - amt);
    assert!(kani_eq_market_group_v16_header_account(&eh, &header));
    let mut es = s0;
    es.backing_long = BackingBucketV16Account::from_runtime(&BackingBucketV16 {
        market_id,
        utilization_fee_earnings: earn - amt,
        status: BackingBucketStatusV16::Expired,
        ..BackingBucketV16::EMPTY
    });
    assert!(kani_eq_engine_asset_slot_v16_account(
        &es,
        &markets[0].engine
    ));
}

// resolve_market frame: exactly {mode, resolved_slot, current_slot,
// loss_stale_active} on the header; the slot and all value fields frozen.
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
fn proof_v16_frame_resolve_market_touches_only_declared_state() {
    let delta_raw: u8 = kani::any();
    kani::assume(delta_raw >= 1 && delta_raw <= 8);
    let (mut header, mut markets, _) = one_market_view_fixture();
    let resolved_slot = header.current_slot.get() + delta_raw as u64;
    let h0 = header;
    let s0 = markets[0].engine;
    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        market.resolve_market_not_atomic(resolved_slot).unwrap();
    }
    kani::cover!(true, "resolve frame reached");
    let mut eh = h0;
    eh.mode = 1;
    eh.resolved_slot = V16PodU64::new(resolved_slot);
    eh.current_slot = V16PodU64::new(resolved_slot);
    eh.loss_stale_active = 0;
    assert!(kani_eq_market_group_v16_header_account(&eh, &header));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &s0,
        &markets[0].engine
    ));
}

// mark_asset_drain_only frame: exactly {asset_set_epoch, risk_epoch} on the
// header and {asset.lifecycle} on the slot.
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
fn proof_v16_frame_mark_drain_only_touches_only_declared_state() {
    let (mut header, mut markets, _) = one_market_view_fixture();
    let h0 = header;
    let s0 = markets[0].engine;
    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        market.mark_asset_drain_only_not_atomic(0).unwrap();
    }
    kani::cover!(true, "drain-only frame reached");
    let mut eh = h0;
    eh.asset_set_epoch = V16PodU64::new(h0.asset_set_epoch.get() + 1);
    eh.risk_epoch = V16PodU64::new(h0.risk_epoch.get() + 1);
    assert!(kani_eq_market_group_v16_header_account(&eh, &header));
    let mut es = s0;
    let mut asset = s0.asset.try_to_runtime().unwrap();
    asset.lifecycle = AssetLifecycleV16::DrainOnly;
    es.asset = AssetStateV16Account::from_runtime(&asset);
    assert!(kani_eq_engine_asset_slot_v16_account(
        &es,
        &markets[0].engine
    ));
}

// budget-credit frame: exactly {insurance_domain_budget_remaining_total} on
// the header and {insurance_domain_budget_long} on the slot — pure capacity
// relabel, every value field frozen.
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
fn proof_v16_frame_budget_credit_touches_only_declared_state() {
    let amt_raw: u8 = kani::any();
    kani::assume(amt_raw >= 1 && amt_raw <= 8);
    let amt = amt_raw as u128;
    let (mut header, mut markets) = one_market_only_fixture();
    header.vault = V16PodU128::new(amt);
    header.insurance = V16PodU128::new(amt);
    let h0 = header;
    let s0 = markets[0].engine;
    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        market
            .credit_domain_insurance_budget_not_atomic(0, amt)
            .unwrap();
    }
    kani::cover!(true, "budget-credit frame reached");
    let mut eh = h0;
    eh.insurance_domain_budget_remaining_total =
        V16PodU128::new(h0.insurance_domain_budget_remaining_total.get() + amt);
    assert!(kani_eq_market_group_v16_header_account(&eh, &header));
    let mut es = s0;
    es.insurance_domain_budget_long = V16PodU128::new(s0.insurance_domain_budget_long.get() + amt);
    assert!(kani_eq_engine_asset_slot_v16_account(
        &es,
        &markets[0].engine
    ));
}

// backing-deposit frame: exactly {vault, source_fresh_backing_total_num} on
// the header, {backing_long bucket, source_credit_long fresh_reserved} on
// the slot — nothing else anywhere.
#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_frame_backing_deposit_touches_only_declared_state() {
    let amt_raw: u8 = kani::any();
    kani::assume(amt_raw >= 1 && amt_raw <= 4);
    let amt = amt_raw as u128;
    let amt_num = amt * BOUND_SCALE;
    let (mut header, mut markets) = one_market_only_fixture();
    let market_id = markets[0].engine.asset.market_id.get();
    let h0 = header;
    let s0 = markets[0].engine;
    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        market
            .deposit_fresh_counterparty_backing_not_atomic(0, amt, 10)
            .unwrap();
    }
    kani::cover!(true, "backing deposit frame reached");
    let mut eh = h0;
    eh.vault = V16PodU128::new(h0.vault.get() + amt);
    eh.source_fresh_backing_total_num =
        V16PodU128::new(h0.source_fresh_backing_total_num.get() + amt_num);
    // the deposit refreshes the domain: risk epoch and credit epoch advance.
    eh.risk_epoch = V16PodU64::new(h0.risk_epoch.get() + 1);
    assert!(kani_eq_market_group_v16_header_account(&eh, &header));
    let mut es = s0;
    es.backing_long = BackingBucketV16Account::from_runtime(&BackingBucketV16 {
        market_id,
        fresh_unliened_backing_num: amt_num,
        expiry_slot: 10,
        status: BackingBucketStatusV16::Fresh,
        ..BackingBucketV16::EMPTY
    });
    let mut src = s0.source_credit_long.try_to_runtime().unwrap();
    src.fresh_reserved_backing_num += amt_num;
    src.credit_epoch += 1;
    src.credit_rate_num = CREDIT_RATE_SCALE;
    es.source_credit_long = SourceCreditStateV16Account::from_runtime(&src);
    assert!(kani_eq_engine_asset_slot_v16_account(
        &es,
        &markets[0].engine
    ));
}

// backing-withdraw frame: the exact inverse set.
#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_frame_backing_withdraw_touches_only_declared_state() {
    let amt_raw: u8 = kani::any();
    let fund_raw: u8 = kani::any();
    kani::assume(amt_raw >= 1 && amt_raw <= 4);
    kani::assume(fund_raw >= amt_raw && fund_raw <= 4);
    let amt = amt_raw as u128;
    let fund = fund_raw as u128;
    let (mut header, mut markets) = one_market_only_fixture();
    let market_id = markets[0].engine.asset.market_id.get();
    header.vault = V16PodU128::new(fund);
    header.source_fresh_backing_total_num = V16PodU128::new(fund * BOUND_SCALE);
    markets[0].engine.backing_long = BackingBucketV16Account::from_runtime(&BackingBucketV16 {
        market_id,
        fresh_unliened_backing_num: fund * BOUND_SCALE,
        expiry_slot: 10,
        status: BackingBucketStatusV16::Fresh,
        ..BackingBucketV16::EMPTY
    });
    markets[0].engine.source_credit_long =
        SourceCreditStateV16Account::from_runtime(&SourceCreditStateV16 {
            fresh_reserved_backing_num: fund * BOUND_SCALE,
            credit_rate_num: CREDIT_RATE_SCALE,
            ..SourceCreditStateV16::EMPTY
        });
    let h0 = header;
    let s0 = markets[0].engine;
    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        market
            .withdraw_fresh_counterparty_backing_not_atomic(0, amt)
            .unwrap();
    }
    kani::cover!(amt < fund, "backing withdraw frame covers partial");
    let rem = fund - amt;
    let mut eh = h0;
    eh.vault = V16PodU128::new(rem);
    eh.source_fresh_backing_total_num = V16PodU128::new(rem * BOUND_SCALE);
    eh.risk_epoch = V16PodU64::new(h0.risk_epoch.get() + 1);
    assert!(kani_eq_market_group_v16_header_account(&eh, &header));
    let mut es = s0;
    es.backing_long = BackingBucketV16Account::from_runtime(&BackingBucketV16 {
        market_id,
        fresh_unliened_backing_num: rem * BOUND_SCALE,
        expiry_slot: if rem == 0 { 0 } else { 10 },
        status: if rem == 0 {
            BackingBucketStatusV16::Empty
        } else {
            BackingBucketStatusV16::Fresh
        },
        ..BackingBucketV16::EMPTY
    });
    es.source_credit_long = SourceCreditStateV16Account::from_runtime(&SourceCreditStateV16 {
        fresh_reserved_backing_num: rem * BOUND_SCALE,
        credit_rate_num: CREDIT_RATE_SCALE,
        credit_epoch: 1,
        ..SourceCreditStateV16::EMPTY
    });
    assert!(kani_eq_engine_asset_slot_v16_account(
        &es,
        &markets[0].engine
    ));
}

// fee-charge frame: exactly {insurance, c_tot} on the header and {capital}
// on the account; vault and everything else frozen.
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
fn proof_v16_frame_fee_charge_touches_only_declared_state() {
    let cap_raw: u8 = kani::any();
    let fee_raw: u8 = kani::any();
    kani::assume(cap_raw <= 8);
    kani::assume(fee_raw <= cap_raw);
    let cap = cap_raw as u128;
    let fee = fee_raw as u128;
    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    header.vault = V16PodU128::new(cap);
    header.c_tot = V16PodU128::new(cap);
    account_header.capital = V16PodU128::new(cap);
    let h0 = header;
    let s0 = markets[0].engine;
    let a0 = account_header;
    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        let mut account = PortfolioV16ViewMut::new(&mut account_header);
        let charged = market
            .kani_charge_account_fee_current_not_atomic(&mut account, fee)
            .unwrap();
        assert_eq!(charged, fee);
    }
    kani::cover!(fee > 0, "fee frame covers nonzero fee");
    let mut eh = h0;
    eh.insurance = V16PodU128::new(fee);
    eh.c_tot = V16PodU128::new(cap - fee);
    assert!(kani_eq_market_group_v16_header_account(&eh, &header));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &s0,
        &markets[0].engine
    ));
    let mut ea = a0;
    ea.capital = V16PodU128::new(cap - fee);
    assert!(kani_eq_portfolio_account_v16_account(&ea, &account_header));
}

// oracle-target-update frame: exactly {asset.raw_oracle_target_price} — one
// field in the whole state.
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
fn proof_v16_frame_oracle_target_update_touches_only_declared_state() {
    let target_raw: u8 = kani::any();
    kani::assume(target_raw >= 1);
    let target = target_raw as u64;
    let (mut header, mut markets, _) = one_market_view_fixture();
    let h0 = header;
    let s0 = markets[0].engine;
    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        market
            .set_asset_raw_oracle_target_not_atomic(0, target)
            .unwrap();
    }
    kani::cover!(true, "oracle target frame reached");
    assert!(kani_eq_market_group_v16_header_account(&h0, &header));
    let mut es = s0;
    let mut asset = s0.asset.try_to_runtime().unwrap();
    asset.raw_oracle_target_price = target;
    es.asset = AssetStateV16Account::from_runtime(&asset);
    assert!(kani_eq_engine_asset_slot_v16_account(
        &es,
        &markets[0].engine
    ));
}

// insurance->account credit frame: exactly {insurance, c_tot} on the header
// and {capital} on the account; vault frozen (internal relabel + credit).
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
fn proof_v16_frame_insurance_account_credit_touches_only_declared_state() {
    let amt_raw: u8 = kani::any();
    let ins_raw: u8 = kani::any();
    kani::assume(amt_raw >= 1 && amt_raw <= 8);
    kani::assume(ins_raw >= amt_raw && ins_raw <= 8);
    let amt = amt_raw as u128;
    let ins = ins_raw as u128;
    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    header.vault = V16PodU128::new(ins);
    header.insurance = V16PodU128::new(ins);
    account_header.last_fee_slot = V16PodU64::new(1);
    let h0 = header;
    let s0 = markets[0].engine;
    let a0 = account_header;
    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        let mut account = PortfolioV16ViewMut::new(&mut account_header);
        market
            .credit_account_from_insurance_not_atomic(&mut account, amt)
            .unwrap();
    }
    kani::cover!(amt < ins, "insurance credit frame covers partial");
    let mut eh = h0;
    eh.insurance = V16PodU128::new(ins - amt);
    eh.c_tot = V16PodU128::new(amt);
    assert!(kani_eq_market_group_v16_header_account(&eh, &header));
    assert!(kani_eq_engine_asset_slot_v16_account(
        &s0,
        &markets[0].engine
    ));
    let mut ea = a0;
    ea.capital = V16PodU128::new(amt);
    assert!(kani_eq_portfolio_account_v16_account(&ea, &account_header));
}

// side-reset frame: exactly {header.risk_epoch, asset.mode_long} — the
// finalization flips one side mode and bumps one epoch, nothing else.
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
fn proof_v16_frame_side_reset_touches_only_declared_state() {
    let (mut header, mut markets, _) = one_market_view_fixture();
    {
        let mut asset = markets[0].engine.asset.try_to_runtime().unwrap();
        asset.mode_long = SideModeV16::ResetPending;
        markets[0].engine.asset = AssetStateV16Account::from_runtime(&asset);
    }
    let h0 = header;
    let s0 = markets[0].engine;
    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        market
            .finalize_side_reset_not_atomic(0, SideV16::Long)
            .unwrap();
    }
    kani::cover!(true, "side reset frame reached");
    let mut eh = h0;
    eh.risk_epoch = V16PodU64::new(h0.risk_epoch.get() + 1);
    assert!(kani_eq_market_group_v16_header_account(&eh, &header));
    let mut es = s0;
    let mut asset = s0.asset.try_to_runtime().unwrap();
    asset.mode_long = SideModeV16::Normal;
    es.asset = AssetStateV16Account::from_runtime(&asset);
    assert!(kani_eq_engine_asset_slot_v16_account(
        &es,
        &markets[0].engine
    ));
}

// same-slot crank frame: a refresh may touch ONLY the five clock/observation
// fields (current_slot, slot_last, loss_stale_active, oracle_epoch,
// funding_epoch — exact semantics pinned by the dedicated crank proofs);
// every other header field, the slot, and the account are frozen.
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
fn proof_v16_frame_crank_touches_only_clock_and_cert_state() {
    let (mut header, mut markets, mut account_header) = one_market_view_fixture();
    account_header.last_fee_slot = V16PodU64::new(1);
    let effective_price = markets[0]
        .engine
        .asset
        .try_to_runtime()
        .unwrap()
        .effective_price;
    let current = header.current_slot.get();
    let h0 = header;
    let s0 = markets[0].engine;
    let a0 = account_header;
    {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        let mut account = PortfolioV16ViewMut::new(&mut account_header);
        market
            .kani_permissionless_crank(
                &mut account,
                PermissionlessCrankRequestV16 {
                    now_slot: current,
                    asset_index: 0,
                    effective_price,
                    funding_rate_e9: 0,
                    action: PermissionlessCrankActionV16::Refresh,
                },
            )
            .unwrap();
    }
    kani::cover!(true, "same-slot crank frame reached");
    let mut eh = h0;
    eh.current_slot = header.current_slot;
    eh.slot_last = header.slot_last;
    eh.loss_stale_active = header.loss_stale_active;
    eh.oracle_epoch = header.oracle_epoch;
    eh.funding_epoch = header.funding_epoch;
    assert!(kani_eq_market_group_v16_header_account(&eh, &header));
    // slot clock fields may advance; freeze everything else on the slot too
    let mut es = s0;
    let mut a_after = markets[0].engine.asset.try_to_runtime().unwrap();
    let a_before = s0.asset.try_to_runtime().unwrap();
    // copy the volatile asset clock/observation fields from post-state
    let mut a_expect = a_before;
    a_expect.slot_last = a_after.slot_last;
    a_expect.effective_price = a_after.effective_price;
    a_expect.fund_px_last = a_after.fund_px_last;
    a_expect.raw_oracle_target_price = a_after.raw_oracle_target_price;
    es.asset = AssetStateV16Account::from_runtime(&a_expect);
    let _ = &mut a_after;
    assert!(kani_eq_engine_asset_slot_v16_account(
        &es,
        &markets[0].engine
    ));
    // the refresh recertifies the hint account: health_cert and the staleness
    // flags are the only account-side volatiles; value, legs, domains,
    // escrow, ledgers all frozen.
    let mut ea = a0;
    ea.health_cert = account_header.health_cert;
    ea.stale_state = account_header.stale_state;
    ea.b_stale_state = account_header.b_stale_state;
    ea.last_fee_slot = account_header.last_fee_slot;
    assert!(kani_eq_portfolio_account_v16_account(&ea, &account_header));
}

// ROADMAP Phase 1 (Pillar F soundness lemma U3): validate_shape's Ok-exit
// IMPLIES the senior stack is covered by the vault — c_tot + insurance +
// backing_provider_earnings_total <= vault. An importable safety-floor lemma
// (a wrapper proof can consume it): "any state the engine commits has its
// protected-principal senior layers fully vault-backed" (spec req 6). The
// cover keeps it non-vacuous (the Ok branch is reachable for symbolic senior
// fields), so the cover-vacuity gate guards against an unsatisfiable assume.
#[kani::proof]
#[kani::unwind(16)]
#[kani::solver(cadical)]
fn proof_v16_validator_sound_senior_stack_within_vault() {
    let vault: u128 = kani::any();
    let c_tot: u128 = kani::any();
    let insurance: u128 = kani::any();
    let (mut header, mut markets) = one_market_only_fixture();
    header.vault = V16PodU128::new(vault);
    header.c_tot = V16PodU128::new(c_tot);
    header.insurance = V16PodU128::new(insurance);
    let market = MarketGroupV16ViewMut::new(&mut header, &mut markets);

    // Premise: the engine considers this a committable state.
    kani::assume(market.validate_shape() == Ok(()));
    kani::cover!(
        c_tot > 0 && insurance > 0 && vault > c_tot,
        "senior-stack soundness lemma is reachable with nontrivial junior + insurance"
    );

    // Conclusion (the importable floor lemma): senior layers are vault-covered.
    let senior = market
        .header
        .c_tot
        .get()
        .checked_add(market.header.insurance.get())
        .and_then(|v| v.checked_add(market.header.backing_provider_earnings_total.get()))
        .expect("senior stack sum cannot overflow a validated state");
    assert!(senior <= market.header.vault.get());
}

// ROADMAP Phase 1 (Pillar F soundness lemmas, batched): validate_shape's Ok-exit
// implies the header-scalar invariants that DON'T depend on account aggregates —
// junior layers within vault (U2) and a monotone clock (U9). The PnL-aggregate
// invariants (U6/U7) are deferred to an account-bearing fixture: on this
// no-account fixture the audit-scan recomputes pnl_pos_tot to 0, so a "pnl_pos>0"
// witness is UNSATISFIABLE (a dead cover) — caught by the cover-vacuity
// discipline, so this lemma stays within the non-vacuous set. Importable floor
// lemmas; the cover keeps it non-vacuous.
#[kani::proof]
#[kani::unwind(16)]
#[kani::solver(cadical)]
fn proof_v16_validator_sound_scalar_invariants() {
    let vault: u128 = kani::any();
    let c_tot: u128 = kani::any();
    let insurance: u128 = kani::any();
    let current_slot: u64 = kani::any();
    let slot_last: u64 = kani::any();
    let (mut header, mut markets) = one_market_only_fixture();
    header.vault = V16PodU128::new(vault);
    header.c_tot = V16PodU128::new(c_tot);
    header.insurance = V16PodU128::new(insurance);
    header.current_slot = V16PodU64::new(current_slot);
    header.slot_last = V16PodU64::new(slot_last);
    let market = MarketGroupV16ViewMut::new(&mut header, &mut markets);

    kani::assume(market.validate_shape() == Ok(()));
    kani::cover!(
        c_tot > 0 && insurance > 0 && vault > c_tot && slot_last < current_slot,
        "scalar-invariant soundness lemma reachable with nontrivial junior + clock gap"
    );

    let h = &market.header;
    assert!(h.c_tot.get() <= h.vault.get()); // U2
    assert!(h.insurance.get() <= h.vault.get()); // U2
    assert!(h.slot_last.get() <= h.current_slot.get()); // U9
}

// ROADMAP Phase 1 (Pillar F soundness, U6/U7 — the PnL aggregates deferred from
// the scalar batch): validate_shape's Ok-exit implies matured <= positive PnL
// (U6) and the positive-PnL bound >= positive PnL (U7). PnL fields fully
// symbolic so the lemma tests the VALIDATOR (not the fixture). The cover proves
// pnl_pos>0 is genuinely reachable under validate_shape==Ok (non-vacuous).
#[kani::proof]
#[kani::unwind(16)]
#[kani::solver(cadical)]
fn proof_v16_validator_sound_pnl_aggregates() {
    let pnl_pos: u128 = kani::any();
    let pnl_matured: u128 = kani::any();
    let pnl_bound: u128 = kani::any();
    let pnl_bound_num: u128 = kani::any();
    let (mut header, mut markets) = one_market_only_fixture();
    header.pnl_pos_tot = V16PodU128::new(pnl_pos);
    header.pnl_matured_pos_tot = V16PodU128::new(pnl_matured);
    header.pnl_pos_bound_tot = V16PodU128::new(pnl_bound);
    header.pnl_pos_bound_tot_num = V16PodU128::new(pnl_bound_num);
    let market = MarketGroupV16ViewMut::new(&mut header, &mut markets);

    kani::assume(market.validate_shape() == Ok(()));
    kani::cover!(
        pnl_pos > 0 && pnl_matured > 0 && pnl_matured < pnl_pos,
        "pnl-aggregate soundness lemma reachable with nontrivial positive + matured PnL"
    );

    let h = &market.header;
    assert!(h.pnl_matured_pos_tot.get() <= h.pnl_pos_tot.get()); // U6
    assert!(h.pnl_pos_bound_tot.get() >= h.pnl_pos_tot.get()); // U7
}

// ROADMAP Phase 1 (Pillar F soundness, U1/U10): validate_shape's Ok-exit implies
// vault within the TVL cap (U1) and a nonzero market-id counter (U10). Only the
// non-division-coupled fields are made symbolic.
//
// U8 (pnl_pos_bound_tot_num >= pnl_pos*BOUND_SCALE) is INTRACTABLE as a Kani
// soundness lemma: making pnl_pos_bound_tot_num symbolic forces validate_shape's
// internal amount_from_bound_num (u128 division by BOUND_SCALE) to bit-blast the
// division circuit (timed out at 900s — the same wide-arithmetic wall the
// complex bodies hit). U8 is deferred to reference-model fuzz (Phase 6):
// bound_num/amount conformance over a stated domain, NOT a Kani lemma.
#[kani::proof]
#[kani::unwind(16)]
#[kani::solver(cadical)]
fn proof_v16_validator_sound_bound_and_config() {
    let vault: u128 = kani::any();
    let next_mid: u64 = kani::any();
    let (mut header, mut markets) = one_market_only_fixture();
    header.vault = V16PodU128::new(vault);
    header.next_market_id = V16PodU64::new(next_mid);
    let market = MarketGroupV16ViewMut::new(&mut header, &mut markets);

    kani::assume(market.validate_shape() == Ok(()));
    kani::cover!(
        vault > 0 && next_mid > 1,
        "config soundness lemma reachable with nontrivial vault + market-id"
    );

    let h = &market.header;
    assert!(h.vault.get() <= MAX_VAULT_TVL); // U1
    assert!(h.next_market_id.get() != 0); // U10
}

// ROADMAP Phase 1 (Pillar F soundness, U19): validate_with_market's Ok-exit
// implies the account-reserve invariants — reserved PnL never exceeds positive
// PnL, and residual principal spent never exceeds residual crystallized loss.
// Account-header scalars symbolic so the lemma tests the validator. (These
// clauses run before the leg loop; an empty account keeps it tractable.)
#[kani::proof]
#[kani::unwind(64)]
#[kani::solver(cadical)]
fn proof_v16_validator_sound_account_reserves() {
    let pnl: i128 = kani::any();
    let reserved_pnl: u128 = kani::any();
    let residual_spent: u128 = kani::any();
    let residual_crystallized: u128 = kani::any();
    kani::assume(pnl > i128::MIN);
    let (header, markets, mut account_header) = one_market_view_fixture();
    account_header.pnl = V16PodI128::new(pnl);
    account_header.reserved_pnl = V16PodU128::new(reserved_pnl);
    account_header.residual_spent_principal_atoms_total = V16PodU128::new(residual_spent);
    account_header.residual_crystallized_loss_atoms_total = V16PodU128::new(residual_crystallized);
    let mut header = header;
    let mut markets = markets;
    let market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let account = PortfolioV16ViewMut::new(&mut account_header);

    kani::assume(account.as_view().validate_with_market(&market.as_view()) == Ok(()));
    kani::cover!(
        pnl > 0 && reserved_pnl > 0 && residual_crystallized > 0 && residual_spent > 0,
        "account-reserve soundness lemma reachable with nontrivial reserves"
    );

    let h = &account.header;
    let pos_pnl = if h.pnl.get() > 0 {
        h.pnl.get() as u128
    } else {
        0
    };
    assert!(h.reserved_pnl.get() <= pos_pnl); // U19a
    assert!(
        h.residual_spent_principal_atoms_total.get()
            <= h.residual_crystallized_loss_atoms_total.get()
    ); // U19b
}

// REALIZABILITY (no-DoS auto-crank dispatch seam): only the no-active-asset
// RefreshAccount fallback hard-requires a caller-supplied observation; active-
// asset refresh and every other plan are dispatchable from committed state.
// Pinning this truth table means a future arm that gates committed-state progress
// on an observation contradicts a machine-checked theorem. Exhaustive over the
// six AutoCrankPlanV16 variants; the spec matrix ties the predicate to the real
// dispatch for each reachable class.
#[kani::proof]
fn proof_v16_auto_crank_refresh_is_unique_observation_requiring_plan() {
    use percolator::v16::auto_crank_plan_requires_caller_observation as needs_obs;
    use percolator::v16::AutoCrankPlanV16;
    let i: usize = kani::any();
    // Active-asset refresh can use the committed asset state; only the fallback
    // with no selected active asset requires a caller observation.
    assert!(!needs_obs(&AutoCrankPlanV16::RefreshAccount {
        asset_index: Some(i)
    }));
    assert!(needs_obs(&AutoCrankPlanV16::RefreshAccount {
        asset_index: None
    }));
    // No other plan does — committed on-chain state suffices.
    assert!(!needs_obs(&AutoCrankPlanV16::SettleBChunk {
        asset_index: i
    }));
    assert!(!needs_obs(&AutoCrankPlanV16::Liquidate { asset_index: i }));
    assert!(!needs_obs(&AutoCrankPlanV16::NoAction));
    assert!(!needs_obs(&AutoCrankPlanV16::CloseResolved));
    // The predicate matches DeclareRecovery { .. } regardless of reason, so a
    // concrete variant exercises the arm (the reason enum isn't Arbitrary here).
    assert!(!needs_obs(&AutoCrankPlanV16::DeclareRecovery {
        reason: PermissionlessRecoveryReasonV16::ActiveBankruptCloseCannotProgress,
    }));
}

// LoF — no free open interest: a risk-increasing fill with nonzero size, nonzero
// price, and nonzero fee_bps must charge a STRICTLY POSITIVE fee per side. The
// pre-fix code derived the fee from trade_notional_floor, which rounds sub-atom
// notional (size_q * exec_price < POS_SCALE) to 0, so a nonzero fill paid zero fee
// while still TAKING open interest. Charging on CEIL notional guarantees >= 1 atom
// per side. This is exactly the LOWER BOUND the conservation/application fee proofs
// are blind to: a zero fee conserves value and over-charges nothing, so every
// existing fee proof passes on the bug. Mutation-checked: swapping the shim to
// trade_notional_floor makes this FAIL on the sub-atom counterexample.
#[kani::proof]
#[kani::unwind(40)]
#[kani::solver(cadical)]
fn proof_v16_nonzero_trade_charges_positive_fee_per_side() {
    let size_q: u128 = kani::any();
    let exec_price: u64 = kani::any();
    let fee_bps: u64 = kani::any();
    kani::assume(size_q > 0 && size_q <= percolator::MAX_TRADE_SIZE_Q);
    kani::assume(exec_price > 0 && exec_price <= percolator::MAX_ORACLE_PRICE);
    kani::assume(fee_bps > 0 && fee_bps <= percolator::MAX_TRADING_FEE_BPS);
    // Exercise the bug's regime: sub-atom notional that floored to zero.
    kani::cover!(
        (size_q.saturating_mul(exec_price as u128)) < percolator::POS_SCALE,
        "sub-atom notional (the floored-to-zero regime) is reachable"
    );
    let fee = percolator::v16::kani_trade_fee_atoms_per_side(size_q, exec_price, fee_bps).unwrap();
    assert!(
        fee >= 1,
        "nonzero fill at nonzero price with nonzero fee must charge >= 1 atom per side (no free OI)"
    );
}

// LoF (no free lien) — backing-utilization fee carry-forward. The lien-rent quote
// floors to 0 for a small lien over a short interval. The buggy collect advanced
// the fee cursor (source_lien_fee_last_slot) to current_slot UNCONDITIONALLY,
// before the charged==0 return, so the floored-away fractional accrual was
// discarded every call — a frequently-refreshed small lien paid zero rent forever
// (slow-drip LoF to providers/insurance). Same blind spot as the sub-atom trade
// fee: the existing quote proof BLESSES the floor-to-zero and asserts no lower
// bound. The fix carries the accrual forward by NOT advancing the cursor when the
// fee is zero. This proof drives the real collect path with a deliberately
// floor-to-zero rent (base_rate=1, lien=1 atom, dt=1) and asserts the cursor is
// preserved with no value moved. It FAILS on the pre-fix unconditional advance.
#[kani::proof]
#[kani::unwind(48)]
#[kani::solver(cadical)]
fn proof_v16_backing_utilization_zero_fee_carries_accrual_forward() {
    let capital_raw: u8 = kani::any();
    let earnings_raw: u8 = kani::any();
    kani::assume(capital_raw > 0); // account CAN pay; the only reason fee is 0 is the floor
    let lien_atoms = 1u128;
    let lien_num = lien_atoms * BOUND_SCALE;
    let last_slot = 3u64;
    let current_slot = last_slot + 1; // dt = 1
    let capital = capital_raw as u128;
    let earnings_before = earnings_raw as u128;
    let (mut header, mut markets, mut account_header) = one_market_direct_view_fixture();
    let market_id = markets[0].engine.asset.market_id.get();
    // base_rate=1, slopes=0  =>  rate = 1  =>  fee = floor(lien_num * 1 * 1 / (DEN_E9*BOUND_SCALE)) = 0
    header.config.backing_fee_base_rate_e9_per_slot = V16PodU64::new(1);
    header.config.backing_fee_slope_at_kink_e9_per_slot = V16PodU64::new(0);
    header.config.backing_fee_slope_above_kink_e9_per_slot = V16PodU64::new(0);
    header.current_slot = V16PodU64::new(current_slot);
    header.slot_last = V16PodU64::new(current_slot);
    header.vault = V16PodU128::new(capital + earnings_before + lien_atoms);
    header.c_tot = V16PodU128::new(capital);
    header.backing_provider_earnings_total = V16PodU128::new(earnings_before);
    header.source_fresh_backing_total_num = V16PodU128::new(lien_num);
    account_header.capital = V16PodU128::new(capital);
    account_header.pnl = V16PodI128::new(0);
    account_header.health_cert.valid = 1;
    account_header.source_domains[0] = PortfolioSourceDomainV16Account {
        domain: V16PodU32::new(0),
        source_claim_market_id: V16PodU64::new(market_id),
        source_lien_counterparty_backing_num: V16PodU128::new(lien_num),
        source_lien_fee_last_slot: V16PodU64::new(last_slot),
        ..PortfolioSourceDomainV16Account::default()
    };
    markets[0].engine.source_credit_long =
        SourceCreditStateV16Account::from_runtime(&SourceCreditStateV16 {
            fresh_reserved_backing_num: lien_num,
            valid_liened_backing_num: lien_num,
            credit_rate_num: CREDIT_RATE_SCALE,
            ..SourceCreditStateV16::EMPTY
        });
    markets[0].engine.backing_long = BackingBucketV16Account::from_runtime(&BackingBucketV16 {
        market_id,
        valid_liened_backing_num: lien_num,
        utilization_fee_earnings: earnings_before,
        expiry_slot: current_slot + 1,
        status: BackingBucketStatusV16::Fresh,
        ..BackingBucketV16::EMPTY
    });

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let vault_before = market.header.vault.get();
    let c_tot_before = market.header.c_tot.get();
    let insurance_before = market.header.insurance.get();
    let mut account = PortfolioV16ViewMut {
        header: &mut account_header,
    };
    let charged = market
        .kani_collect_account_backing_utilization_fee_for_domain_not_atomic(&mut account, 0)
        .unwrap();

    // The rent floored to zero this interval...
    assert_eq!(charged, 0, "sub-atom rent must floor to zero this interval");
    // ...so the cursor MUST NOT advance (carry the accrual forward; no free lien).
    assert_eq!(
        account.header.source_domains[0]
            .source_lien_fee_last_slot
            .get(),
        last_slot,
        "zero-fee collection must preserve the fee cursor (carry accrual forward)"
    );
    // No value moved.
    assert_eq!(account.header.capital.get(), capital);
    assert_eq!(market.header.c_tot.get(), c_tot_before);
    assert_eq!(market.header.vault.get(), vault_before);
    assert_eq!(market.header.insurance.get(), insurance_before);
}
