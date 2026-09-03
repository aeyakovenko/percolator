#![cfg(feature = "fuzz")]

use percolator::{
    EngineAssetSlotV16Account, LiquidationRequestV16, Market, MarketGroupV16HeaderAccount,
    MarketGroupV16ViewMut, PermissionlessCrankActionV16, PermissionlessCrankRequestV16,
    PermissionlessRecoveryReasonV16, PortfolioAccountV16Account, PortfolioLegV16, PortfolioV16View,
    PortfolioV16ViewMut, ProvenanceHeaderV16, ProvenanceHeaderV16Account, TradeRequestV16,
    V16Config, V16Error, V16PodU128, MAX_VAULT_TVL,
};
use percolator::{BOUND_SCALE, POS_SCALE, SOCIAL_LOSS_DEN};
use proptest::prelude::*;

fn ids() -> ([u8; 32], [u8; 32], [u8; 32], [u8; 32]) {
    ([1; 32], [2; 32], [3; 32], [4; 32])
}

fn fuzz_group() -> (MarketGroupV16HeaderAccount, Vec<Market<u64>>) {
    let (market_id, _, _, _) = ids();
    let mut cfg = V16Config::public_user_fund_with_market_slots(1, 1, 0, 10);
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

#[test]
fn v16_b_settlement_budget_is_loss_atoms_not_index_delta() {
    const TARGET_B: u128 = 107_486_458_947_473_684_210_526_315;
    const LOSS_WEIGHT: u128 = 19_000_000;

    let (market_id, _, _, _) = ids();
    let cfg = V16Config::public_user_fund_with_market_slots(1, 1, 0, 10);
    assert_eq!(cfg.public_b_chunk_atoms, MAX_VAULT_TVL);
    let mut header = MarketGroupV16HeaderAccount::new_dynamic(market_id, cfg, 1, 0).unwrap();
    let mut markets = vec![Market::new(0u64, EngineAssetSlotV16Account::default())];
    let market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let leg = PortfolioLegV16 {
        active: true,
        loss_weight: LOSS_WEIGHT,
        ..PortfolioLegV16::default()
    };
    let full_loss = LOSS_WEIGHT.checked_mul(TARGET_B).unwrap() / SOCIAL_LOSS_DEN;
    assert!(full_loss > 0 && full_loss < MAX_VAULT_TVL);

    let chunk = market
        .kani_account_b_settlement_chunk_from_leg(leg, TARGET_B, MAX_VAULT_TVL)
        .unwrap();

    assert_eq!(chunk.delta_b, TARGET_B);
    assert_eq!(chunk.loss, full_loss);
    assert_eq!(chunk.remaining_after, 0);
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
    assert!(market.header.vault.get() >= market.header.c_tot.get() + market.header.insurance.get());
    let positive_pnl = [account_a.pnl.get(), account_b.pnl.get()]
        .into_iter()
        .filter(|pnl| *pnl > 0)
        .map(|pnl| pnl as u128)
        .sum::<u128>();
    assert_eq!(market.header.pnl_pos_tot.get(), positive_pnl);
}

fn source_claim_num(account: &PortfolioAccountV16Account, domain: usize) -> u128 {
    account
        .source_domains
        .iter()
        .find(|source| {
            source.source_claim_market_id.get() != 0 && source.domain.get() as usize == domain
        })
        .map(|source| source.source_claim_bound_num.get())
        .unwrap_or(0)
}

struct TwoDomainBSettlement {
    other_claim_num: u128,
    target_claim_num: u128,
    target_lien_num: u128,
    target_valid_backing_num: u128,
    target_valid_insurance_num: u128,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TargetLienBacking {
    None,
    Counterparty,
    Insurance,
}

fn settle_two_domain_b_loss(
    target_claim: u128,
    other_claim: u128,
    loss: u128,
    target_lien: u128,
    target_lien_backing: TargetLienBacking,
) -> TwoDomainBSettlement {
    let (market_id, _, _, owner) = ids();
    let mut cfg = V16Config::public_user_fund_with_market_slots(2, 2, 0, 10);
    cfg.public_b_chunk_atoms = loss.max(1);
    let mut header = MarketGroupV16HeaderAccount::new_dynamic(market_id, cfg, 2, 0).unwrap();
    let mut markets = vec![
        Market::new(0u64, EngineAssetSlotV16Account::default()),
        Market::new(1u64, EngineAssetSlotV16Account::default()),
    ];
    for (asset_index, market) in markets.iter_mut().enumerate() {
        header
            .activate_empty_asset_slot_not_atomic(
                asset_index as u32,
                &mut market.engine,
                100,
                (asset_index + 1) as u64,
            )
            .unwrap();
    }
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

    let loss_weight = {
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        let mut long = PortfolioV16ViewMut::new(&mut long_header);
        let mut short = PortfolioV16ViewMut::new(&mut short_header);
        market.deposit_not_atomic(&mut long, 10_000).unwrap();
        market.deposit_not_atomic(&mut short, 10_000).unwrap();
        match target_lien_backing {
            TargetLienBacking::None => {}
            TargetLienBacking::Counterparty => market
                .deposit_fresh_counterparty_backing_not_atomic(3, 1_000, 100)
                .unwrap(),
            TargetLienBacking::Insurance => {
                market
                    .deposit_domain_insurance_not_atomic(3, 1_000)
                    .unwrap();
                market
                    .reserve_insurance_credit_not_atomic(3, 1_000 * BOUND_SCALE)
                    .unwrap();
            }
        }
        market
            .execute_trade_with_fee_loss_stale_scoped_not_atomic(
                &mut long,
                &mut short,
                TradeRequestV16 {
                    asset_index: 1,
                    size_q: POS_SCALE as i128,
                    exec_price: 100,
                    fee_bps: 0,
                },
            )
            .unwrap();
        market
            .add_account_source_positive_pnl_not_atomic(&mut long, 1, other_claim)
            .unwrap();
        market
            .add_account_source_positive_pnl_not_atomic(&mut long, 3, target_claim)
            .unwrap();
        if target_lien != 0 {
            long.header.health_cert.certified_initial_req =
                V16PodU128::new(long.header.capital.get() + target_lien);
            long.header.health_cert.valid = 1;
            market
                .kani_create_initial_margin_source_lien_if_needed(&mut long)
                .unwrap();
            assert_eq!(
                source_claim_num(&*long.header, 3),
                target_claim * BOUND_SCALE
            );
            let target_source = long
                .header
                .source_domains
                .iter()
                .find(|source| {
                    source.source_claim_market_id.get() != 0 && source.domain.get() as usize == 3
                })
                .expect("target-domain claim");
            assert_eq!(
                target_source.source_claim_liened_num.get(),
                target_lien * BOUND_SCALE
            );
        }
        long.header
            .legs
            .iter()
            .find_map(|leg| {
                let leg = leg.try_to_runtime().ok()?;
                (leg.active && leg.asset_index == 1).then_some(leg.loss_weight)
            })
            .unwrap()
    };
    let delta_b = loss
        .checked_mul(SOCIAL_LOSS_DEN)
        .unwrap()
        .checked_div(loss_weight)
        .unwrap();
    markets[1].engine.asset.b_long_num = V16PodU128::new(delta_b);

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut long = PortfolioV16ViewMut::new(&mut long_header);
    let outcome = market
        .kani_settle_account_b_chunk(&mut long, 1, delta_b)
        .unwrap();
    assert_eq!(outcome.loss, loss);
    let target_source = long_header
        .source_domains
        .iter()
        .find(|source| {
            source.source_claim_market_id.get() != 0 && source.domain.get() as usize == 3
        })
        .copied()
        .unwrap_or_default();
    TwoDomainBSettlement {
        other_claim_num: source_claim_num(&long_header, 1),
        target_claim_num: source_claim_num(&long_header, 3),
        target_lien_num: target_source.source_claim_liened_num.get(),
        target_valid_backing_num: markets[1]
            .engine
            .backing_short
            .try_to_runtime()
            .unwrap()
            .valid_liened_backing_num,
        target_valid_insurance_num: markets[1]
            .engine
            .insurance_reservation_short
            .try_to_runtime()
            .unwrap()
            .valid_liened_insurance_num,
    }
}

#[test]
fn v16_b_loss_burns_the_matching_source_domain_before_sparse_slot_order() {
    let after = settle_two_domain_b_loss(100, 100, 50, 0, TargetLienBacking::None);
    assert_eq!(after.other_claim_num, 100 * BOUND_SCALE);
    assert_eq!(after.target_claim_num, 50 * BOUND_SCALE);
}

#[test]
fn v16_b_loss_falls_back_to_cross_margin_claims_after_its_domain_is_exhausted() {
    let after = settle_two_domain_b_loss(20, 100, 50, 0, TargetLienBacking::None);
    assert_eq!(after.target_claim_num, 0);
    assert_eq!(after.other_claim_num, 70 * BOUND_SCALE);
}

#[test]
fn v16_b_loss_preserves_the_unburned_part_of_a_matching_source_lien() {
    let after = settle_two_domain_b_loss(100, 100, 50, 80, TargetLienBacking::Counterparty);
    assert_eq!(after.other_claim_num, 100 * BOUND_SCALE);
    assert_eq!(after.target_claim_num, 50 * BOUND_SCALE);
    assert_eq!(after.target_lien_num, 50 * BOUND_SCALE);
    assert_eq!(after.target_valid_backing_num, 50 * BOUND_SCALE);
}

#[test]
fn v16_b_loss_preserves_the_unburned_part_of_a_matching_insurance_lien() {
    let after = settle_two_domain_b_loss(100, 100, 50, 80, TargetLienBacking::Insurance);
    assert_eq!(after.other_claim_num, 100 * BOUND_SCALE);
    assert_eq!(after.target_claim_num, 50 * BOUND_SCALE);
    assert_eq!(after.target_lien_num, 50 * BOUND_SCALE);
    assert_eq!(after.target_valid_backing_num, 0);
    assert_eq!(after.target_valid_insurance_num, 50 * BOUND_SCALE);
}

#[test]
fn v16_source_claim_burn_crosses_a_just_emptied_sparse_slot() {
    let (market_id, _, _, owner) = ids();
    let cfg = V16Config::public_user_fund_with_market_slots(2, 2, 0, 10);
    let mut header = MarketGroupV16HeaderAccount::new_dynamic(market_id, cfg, 2, 0).unwrap();
    let mut markets = vec![
        Market::new(0u64, EngineAssetSlotV16Account::default()),
        Market::new(1u64, EngineAssetSlotV16Account::default()),
    ];
    for (asset_index, market) in markets.iter_mut().enumerate() {
        header
            .activate_empty_asset_slot_not_atomic(
                asset_index as u32,
                &mut market.engine,
                100,
                (asset_index + 1) as u64,
            )
            .unwrap();
    }
    let mut account_header = PortfolioAccountV16Account::default();
    account_header
        .init_empty_in_place(ProvenanceHeaderV16Account::from_runtime(
            &ProvenanceHeaderV16::new(market_id, [11; 32], owner),
        ))
        .unwrap();

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    market
        .add_account_source_positive_pnl_not_atomic(&mut account, 1, 100)
        .unwrap();
    market
        .add_account_source_positive_pnl_not_atomic(&mut account, 3, 100)
        .unwrap();

    market.kani_set_account_pnl(&mut account, 0).unwrap();
    assert_eq!(account.header.pnl.get(), 0);
    assert_eq!(source_claim_num(account.header, 1), 0);
    assert_eq!(source_claim_num(account.header, 3), 0);
}

#[allow(clippy::too_many_arguments)]
fn run_with_svm_rollback(
    header: &mut MarketGroupV16HeaderAccount,
    markets: &mut Vec<Market<u64>>,
    account_a: &mut PortfolioAccountV16Account,
    account_b: &mut PortfolioAccountV16Account,
    result: Result<(), V16Error>,
    before: (
        MarketGroupV16HeaderAccount,
        Vec<Market<u64>>,
        PortfolioAccountV16Account,
        PortfolioAccountV16Account,
    ),
) {
    if result.is_err() {
        *header = before.0;
        *markets = before.1;
        *account_a = before.2;
        *account_b = before.3;
    }
    assert_fuzz_invariants(header, markets, account_a, account_b);
}

#[allow(clippy::too_many_arguments)]
fn apply_fuzz_action(
    header: &mut MarketGroupV16HeaderAccount,
    markets: &mut Vec<Market<u64>>,
    account_a: &mut PortfolioAccountV16Account,
    account_b: &mut PortfolioAccountV16Account,
    selector: u8,
    amount_seed: u16,
) {
    let before = (*header, markets.clone(), *account_a, *account_b);
    let target_a = (selector & 0x8) == 0;
    let amount = (amount_seed as u128) % 128;
    let result = match selector % 12 {
        0 => {
            let mut market = MarketGroupV16ViewMut::new(header, markets);
            if target_a {
                let mut account = PortfolioV16ViewMut::new(account_a);
                market.deposit_not_atomic(&mut account, amount)
            } else {
                let mut account = PortfolioV16ViewMut::new(account_b);
                market.deposit_not_atomic(&mut account, amount)
            }
        }
        1 => {
            let mut market = MarketGroupV16ViewMut::new(header, markets);
            if target_a {
                let mut account = PortfolioV16ViewMut::new(account_a);
                market.withdraw_not_atomic(&mut account, amount)
            } else {
                let mut account = PortfolioV16ViewMut::new(account_b);
                market.withdraw_not_atomic(&mut account, amount)
            }
        }
        2 => {
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
        3 => {
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
        4 => {
            let mut market = MarketGroupV16ViewMut::new(header, markets);
            let mut long_account = PortfolioV16ViewMut::new(account_a);
            let mut short_account = PortfolioV16ViewMut::new(account_b);
            market
                .execute_trade_with_fee_loss_stale_scoped_not_atomic(
                    &mut long_account,
                    &mut short_account,
                    TradeRequestV16 {
                        asset_index: 0,
                        size_q: i128::try_from(1 + (amount % 4)).unwrap(),
                        exec_price: 1,
                        fee_bps: (amount_seed as u64) % 11,
                    },
                )
                .map(|_| ())
        }
        5 => {
            let mut market = MarketGroupV16ViewMut::new(header, markets);
            if target_a {
                let mut account = PortfolioV16ViewMut::new(account_a);
                market
                    .kani_permissionless_crank(
                        &mut account,
                        PermissionlessCrankRequestV16 {
                            now_slot: market.header.current_slot.get().saturating_add(1),
                            asset_index: 0,
                            effective_price: 1 + ((amount_seed as u64) & 1),
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
                            effective_price: 1 + ((amount_seed as u64) & 1),
                            funding_rate_e9: 0,
                            action: PermissionlessCrankActionV16::Refresh,
                        },
                    )
                    .map(|_| ())
            }
        }
        6 => {
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
        7 => {
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
        8 => {
            let mut market = MarketGroupV16ViewMut::new(header, markets);
            if target_a {
                let mut account = PortfolioV16ViewMut::new(account_a);
                market
                    .liquidate_account_not_atomic(
                        &mut account,
                        LiquidationRequestV16 { asset_index: 0 },
                    )
                    .map(|_| ())
            } else {
                let mut account = PortfolioV16ViewMut::new(account_b);
                market
                    .liquidate_account_not_atomic(
                        &mut account,
                        LiquidationRequestV16 { asset_index: 0 },
                    )
                    .map(|_| ())
            }
        }
        9 => {
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
        10 => {
            let mut market = MarketGroupV16ViewMut::new(header, markets);
            market
                .resolve_market_not_atomic(market.header.current_slot.get())
                .map(|_| ())
        }
        _ => {
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
    };

    run_with_svm_rollback(header, markets, account_a, account_b, result, before);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn v16_fuzz_public_live_view_actions_preserve_conservation_under_svm_rollback(
        actions in prop::collection::vec((0u8..16, 0u16..512), 1..80)
    ) {
        let (mut header, mut markets) = fuzz_group();
        let (_, a_id, b_id, _) = ids();
        let mut account_a = fuzz_account(a_id);
        let mut account_b = fuzz_account(b_id);
        {
            let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
            let mut a = PortfolioV16ViewMut::new(&mut account_a);
            let mut b = PortfolioV16ViewMut::new(&mut account_b);
            market.deposit_not_atomic(&mut a, 1_000).unwrap();
            market.deposit_not_atomic(&mut b, 1_000).unwrap();
        }
        assert_fuzz_invariants(
            &mut header,
            &mut markets,
            &account_a,
            &account_b,
        );

        for (selector, amount_seed) in actions {
            apply_fuzz_action(
                &mut header,
                &mut markets,
                &mut account_a,
                &mut account_b,
                selector,
                amount_seed,
            );
        }
    }
}
