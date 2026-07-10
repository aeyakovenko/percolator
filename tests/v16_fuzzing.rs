#![cfg(feature = "fuzz")]

use percolator::{
    EngineAssetSlotV16Account, LiquidationRequestV16, Market, MarketGroupV16HeaderAccount,
    MarketGroupV16ViewMut, PermissionlessCrankActionV16, PermissionlessCrankRequestV16,
    PermissionlessRecoveryReasonV16, PortfolioAccountV16Account, PortfolioV16View,
    PortfolioV16ViewMut, ProvenanceHeaderV16, ProvenanceHeaderV16Account, TradeRequestV16,
    V16Config, V16Error,
};
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
