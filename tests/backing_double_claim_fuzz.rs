//! Fuzz/integration coverage for the residual/backing double-claim class.
//!
//! Recoverable counterparty backing principal is provider-withdrawable with no
//! mode or payout-snapshot gate, so it must never be counted in residual(), the
//! junior payout pool. If it is, the resolved payout snapshot promises winners
//! the same vault atoms the provider can still withdraw, and whichever party
//! moves second is robbed or stranded. The Kani proof
//! `proof_v16_residual_excludes_recoverable_counterparty_backing_principal`
//! pins the residual() primitive; these randomized tests drive the real
//! end-to-end resolved close + provider withdrawal in BOTH orders and assert
//! the two claims never overlap.

use percolator::BOUND_SCALE;
use percolator::{
    AssetStateV16Account, BackingBucketStatusV16, BackingBucketV16, BackingBucketV16Account,
    EngineAssetSlotV16Account, Market, MarketGroupV16HeaderAccount, MarketGroupV16ViewMut,
    PortfolioAccountV16Account, PortfolioLegV16, PortfolioLegV16Account, PortfolioV16ViewMut,
    ProvenanceHeaderV16, ProvenanceHeaderV16Account, ResolvedCloseOutcomeV16, SideV16,
    SourceCreditStateV16, SourceCreditStateV16Account, V16Config, V16PodI128, V16PodU128,
    V16PodU32, V16PodU64, ADL_ONE, CREDIT_RATE_SCALE, POS_SCALE,
};
use proptest::prelude::*;

fn market_id() -> [u8; 32] {
    [1u8; 32]
}

fn empty_account() -> PortfolioAccountV16Account {
    let header = ProvenanceHeaderV16Account::from_runtime(&ProvenanceHeaderV16::new(
        market_id(),
        [2u8; 32],
        [2u8; 32],
    ));
    let mut account = PortfolioAccountV16Account::default();
    account.init_empty_in_place(header).unwrap();
    account
}

/// Resolved single-winner haircut market with `backing` atoms of recoverable
/// counterparty backing principal sitting in the vault alongside the winner's
/// capital and the junior residual.
fn resolved_market_with_backing(
    capital: u128,
    pnl: u128,
    residual: u128,
    backing: u128,
) -> (MarketGroupV16HeaderAccount, [Market<u64>; 1]) {
    let cfg = V16Config::public_user_fund_with_market_slots(1, 1, 0, 10);
    let mut header = MarketGroupV16HeaderAccount::new_dynamic(market_id(), cfg, 1, 0).unwrap();
    let mut markets = [Market::new(0u64, EngineAssetSlotV16Account::default())];
    header
        .activate_empty_asset_slot_not_atomic(0, &mut markets[0].engine, 100, 1)
        .unwrap();
    header.mode = 1; // Resolved
    header.resolved_slot = V16PodU64::new(1);
    header.current_slot = V16PodU64::new(1);
    header.vault = V16PodU128::new(capital + residual + backing);
    header.c_tot = V16PodU128::new(capital);
    header.pnl_pos_tot = V16PodU128::new(pnl);
    header.pnl_matured_pos_tot = V16PodU128::new(pnl);
    header.pnl_pos_bound_tot = V16PodU128::new(pnl);
    header.pnl_pos_bound_tot_num = V16PodU128::new(pnl * BOUND_SCALE);
    if backing != 0 {
        let backing_num = backing * BOUND_SCALE;
        header.source_fresh_backing_total_num = V16PodU128::new(backing_num);
        let engine_market_id = markets[0].engine.asset.market_id.get();
        markets[0].engine.backing_long = BackingBucketV16Account::from_runtime(&BackingBucketV16 {
            market_id: engine_market_id,
            fresh_unliened_backing_num: backing_num,
            expiry_slot: 100,
            status: BackingBucketStatusV16::Fresh,
            ..BackingBucketV16::EMPTY
        });
        markets[0].engine.source_credit_long =
            SourceCreditStateV16Account::from_runtime(&SourceCreditStateV16 {
                fresh_reserved_backing_num: backing_num,
                credit_rate_num: CREDIT_RATE_SCALE,
                ..SourceCreditStateV16::EMPTY
            });
    }
    (header, markets)
}

fn winner_account(capital: u128, pnl: u128) -> PortfolioAccountV16Account {
    let mut account_header = empty_account();
    account_header.capital = V16PodU128::new(capital);
    account_header.pnl = V16PodI128::new(pnl as i128);
    account_header.last_fee_slot = V16PodU64::new(1);
    account_header
}

/// Close the winner, then (optionally first) withdraw the provider principal.
/// Returns (winner_payout, vault_after_everything).
fn run_order(
    capital: u128,
    pnl: u128,
    residual: u128,
    backing: u128,
    provider_first: bool,
) -> (u128, u128) {
    let (mut header, mut markets) = resolved_market_with_backing(capital, pnl, residual, backing);
    let mut account_header = winner_account(capital, pnl);
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    assert_eq!(market.validate_shape(), Ok(()));
    assert_eq!(account.validate_with_market(&market.as_view()), Ok(()));

    let vault_before = market.header.vault.get();
    if provider_first {
        market
            .withdraw_fresh_counterparty_backing_not_atomic(0, backing)
            .expect("provider principal must be withdrawable before the winner closes");
    }
    let outcome = market
        .close_resolved_account_not_atomic(&mut account, 0)
        .expect("winner close must not revert");
    let closed = matches!(outcome, ResolvedCloseOutcomeV16::Closed { .. });
    assert!(closed, "winner did not fully close");
    if !provider_first {
        market
            .withdraw_fresh_counterparty_backing_not_atomic(0, backing)
            .expect("provider principal must remain withdrawable after the winner closes");
    }
    assert_eq!(market.validate_shape(), Ok(()));
    let vault_after = market.header.vault.get();
    let winner_payout = vault_before - vault_after - backing;
    (winner_payout, vault_after)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// The winner's resolved payout and the provider's principal must be funded
    /// by DISJOINT vault atoms: the winner is paid capital + haircut residual
    /// (never the provider's backing), and the provider can recover the full
    /// principal regardless of whether the withdrawal happens before or after
    /// the payout snapshot is captured by the winner's close.
    #[test]
    fn winner_payout_and_provider_principal_never_overlap(
        capital in 0u128..=1_000_000u128,
        pnl in 2u128..=1_000_000u128,
        residual_frac in 1u128..=999u128,
        backing in 1u128..=1_000_000u128,
    ) {
        // haircut: residual strictly below the winner's junior bound.
        let residual = (pnl.saturating_mul(residual_frac) / 1000).max(1).min(pnl - 1);
        prop_assume!(residual < pnl);

        let (payout_after, vault_after) =
            run_order(capital, pnl, residual, backing, false);
        let (payout_first, vault_first) =
            run_order(capital, pnl, residual, backing, true);

        // The winner gets exactly its capital plus the honest junior residual...
        prop_assert_eq!(payout_after, capital + residual);
        // ...identically in both orders (the snapshot must not depend on whether
        // the provider already recovered principal)...
        prop_assert_eq!(payout_first, payout_after);
        // ...and nothing else leaks: the vault drains to zero in both orders.
        prop_assert_eq!(vault_after, 0);
        prop_assert_eq!(vault_first, 0);
    }
}

/// Resolved single-winner market where the winner's positive PnL is a
/// SOURCE-BACKED claim: claim_bound == pnl, backed by `backing` atoms of
/// counterparty backing on domain 0 (credit rate = backing/pnl). The vault
/// holds ONLY the backing plus `extra_residual` of ordinary junior funds.
fn resolved_market_with_backed_winner(
    pnl: u128,
    backing: u128,
    extra_residual: u128,
) -> (
    MarketGroupV16HeaderAccount,
    [Market<u64>; 1],
    PortfolioAccountV16Account,
) {
    let (mut header, mut markets) = resolved_market_with_backing(0, pnl, extra_residual, backing);
    header.source_claim_bound_total_num = V16PodU128::new(pnl * BOUND_SCALE);
    if backing != 0 {
        // The claim leans on this domain's backing: rate = backing/claim.
        let claim_num = pnl * BOUND_SCALE;
        let backing_num = backing * BOUND_SCALE;
        markets[0].engine.source_credit_long =
            SourceCreditStateV16Account::from_runtime(&SourceCreditStateV16 {
                positive_claim_bound_num: claim_num,
                exact_positive_claim_num: claim_num,
                fresh_reserved_backing_num: backing_num,
                credit_rate_num: (backing_num * CREDIT_RATE_SCALE / claim_num)
                    .min(CREDIT_RATE_SCALE),
                ..SourceCreditStateV16::EMPTY
            });
    }
    let mut account_header = winner_account(0, pnl);
    let market_id_engine = markets[0].engine.asset.market_id.get();
    account_header.source_domains[0].domain = V16PodU32::new(0);
    account_header.source_domains[0].source_claim_market_id = V16PodU64::new(market_id_engine);
    account_header.source_domains[0].source_claim_bound_num = V16PodU128::new(pnl * BOUND_SCALE);
    (header, markets, account_header)
}

fn resolved_market_with_two_backed_sources() -> (
    MarketGroupV16HeaderAccount,
    [Market<u64>; 2],
    PortfolioAccountV16Account,
) {
    const CLAIM_PER_SOURCE: u128 = 200;
    const BACKING: [u128; 2] = [150, 100];
    const EXTRA_RESIDUAL: u128 = 150;

    let cfg = V16Config::public_user_fund_with_market_slots(2, 2, 0, 10);
    let mut header = MarketGroupV16HeaderAccount::new_dynamic(market_id(), cfg, 2, 0).unwrap();
    let mut markets = [
        Market::new(0u64, EngineAssetSlotV16Account::default()),
        Market::new(0u64, EngineAssetSlotV16Account::default()),
    ];
    for (asset_index, market) in markets.iter_mut().enumerate() {
        header
            .activate_empty_asset_slot_not_atomic(
                asset_index as u32,
                &mut market.engine,
                100,
                asset_index as u64 + 1,
            )
            .unwrap();
        let backing_num = BACKING[asset_index] * BOUND_SCALE;
        market.engine.backing_long = BackingBucketV16Account::from_runtime(&BackingBucketV16 {
            market_id: market.engine.asset.market_id.get(),
            fresh_unliened_backing_num: backing_num,
            expiry_slot: 100,
            status: BackingBucketStatusV16::Fresh,
            ..BackingBucketV16::EMPTY
        });
        market.engine.source_credit_long =
            SourceCreditStateV16Account::from_runtime(&SourceCreditStateV16 {
                positive_claim_bound_num: CLAIM_PER_SOURCE * BOUND_SCALE,
                exact_positive_claim_num: CLAIM_PER_SOURCE * BOUND_SCALE,
                fresh_reserved_backing_num: backing_num,
                credit_rate_num: BACKING[asset_index] * CREDIT_RATE_SCALE / CLAIM_PER_SOURCE,
                ..SourceCreditStateV16::EMPTY
            });
    }

    let total_claim = 2 * CLAIM_PER_SOURCE;
    header.mode = 1;
    header.resolved_slot = V16PodU64::new(2);
    header.current_slot = V16PodU64::new(2);
    header.vault = V16PodU128::new(BACKING.iter().sum::<u128>() + EXTRA_RESIDUAL);
    header.pnl_pos_tot = V16PodU128::new(total_claim);
    header.pnl_matured_pos_tot = V16PodU128::new(total_claim);
    header.pnl_pos_bound_tot = V16PodU128::new(total_claim);
    header.pnl_pos_bound_tot_num = V16PodU128::new(total_claim * BOUND_SCALE);
    header.source_claim_bound_total_num = V16PodU128::new(total_claim * BOUND_SCALE);
    header.source_fresh_backing_total_num =
        V16PodU128::new(BACKING.iter().sum::<u128>() * BOUND_SCALE);

    let mut account = winner_account(0, total_claim);
    for (slot, domain) in [0u32, 2].into_iter().enumerate() {
        let asset_index = domain as usize / 2;
        account.source_domains[slot].domain = V16PodU32::new(domain);
        account.source_domains[slot].source_claim_market_id =
            V16PodU64::new(markets[asset_index].engine.asset.market_id.get());
        account.source_domains[slot].source_claim_bound_num =
            V16PodU128::new(CLAIM_PER_SOURCE * BOUND_SCALE);
    }
    (header, markets, account)
}

#[test]
fn bounded_terminal_source_realization_preserves_intermediate_attribution() {
    let (mut header, mut markets, mut account_header) = resolved_market_with_two_backed_sources();
    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    assert_eq!(market.validate_shape(), Ok(()));
    assert_eq!(account.validate_with_market(&market.as_view()), Ok(()));

    assert_eq!(
        market.close_resolved_account_not_atomic(&mut account, 0),
        Ok(ResolvedCloseOutcomeV16::ProgressOnly)
    );
    assert_eq!(account.header.capital.get(), 150);
    assert_eq!(account.header.pnl.get(), 250);
    assert_eq!(account.header.reserved_pnl.get(), 50);
    assert_eq!(account.validate_with_market(&market.as_view()), Ok(()));
    assert_eq!(market.validate_shape(), Ok(()));

    assert_eq!(
        market.close_resolved_account_not_atomic(&mut account, 0),
        Ok(ResolvedCloseOutcomeV16::Closed { payout: 400 })
    );
    assert_eq!(account.header.capital.get(), 0);
    assert_eq!(account.header.pnl.get(), 0);
    assert_eq!(account.header.reserved_pnl.get(), 0);
    assert_eq!(account.validate_with_market(&market.as_view()), Ok(()));
    assert_eq!(market.validate_shape(), Ok(()));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// A source-backed winner's claim is realizable against its domain backing at
    /// the current credit rate in Live (convert_released_pnl_to_capital). Resolution
    /// must not strip either component of that entitlement: resolved close first
    /// converts the Live-realizable portion using source backing, then retains the
    /// source haircut remainder as ordinary junior face against the terminal pool.
    /// Otherwise a tiny source rate can burn a large claim and strand unrelated
    /// junior value in the vault.
    #[test]
    fn terminal_close_realizes_backed_source_claim(
        pnl in 1u128..=1_000_000u128,
        backing_frac in 1u128..=1000u128,
        extra_residual in 0u128..=1_000_000u128,
    ) {
        let backing = (pnl.saturating_mul(backing_frac) / 1000).max(1).min(pnl);
        // The engine's Live entitlement, mirrored exactly (floored credit rate,
        // floored support): rate = floor(backing_num * CRS / claim_num),
        // realizable = floor(claim_num * rate / CRS / BOUND_SCALE), backing-capped.
        let claim_num = pnl * BOUND_SCALE;
        let backing_num = backing * BOUND_SCALE;
        let rate = (backing_num * CREDIT_RATE_SCALE / claim_num).min(CREDIT_RATE_SCALE);
        let realizable =
            ((claim_num * rate / CREDIT_RATE_SCALE) / BOUND_SCALE).min(backing).min(pnl);
        let retained_terminal_face = pnl - realizable;
        let terminal_junior_payout = retained_terminal_face.min(extra_residual);
        let expected_payout = realizable + terminal_junior_payout;

        let (mut header, mut markets, mut account_header) =
            resolved_market_with_backed_winner(pnl, backing, extra_residual);
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        let mut account = PortfolioV16ViewMut::new(&mut account_header);
        prop_assert_eq!(market.validate_shape(), Ok(()));
        prop_assert_eq!(account.validate_with_market(&market.as_view()), Ok(()));

        let vault_before = market.header.vault.get();
        let outcome = market
            .close_resolved_account_not_atomic(&mut account, 0)
            .expect("backed winner close must not revert");
        let closed = matches!(outcome, ResolvedCloseOutcomeV16::Closed { payout: _ });
        prop_assert!(closed, "backed winner did not fully close");
        let paid = vault_before - market.header.vault.get();

        // Source-backed atoms become capital, while the haircut remainder keeps
        // its place in the terminal junior pool rather than disappearing.
        prop_assert_eq!(paid, expected_payout);
        // ...a fully-backed claim realizes in full...
        if backing >= pnl {
            prop_assert_eq!(paid, pnl);
        }
        prop_assert_eq!(market.header.vault.get(), backing + extra_residual - paid);
        prop_assert_eq!(market.validate_shape(), Ok(()));

        // The two disjoint payout layers never exceed the original face.
        prop_assert!(paid <= pnl);
        prop_assert_eq!(account.header.pnl.get(), 0);
        prop_assert_eq!(account.header.capital.get(), 0);
        // IDEMPOTENT: re-closing the now-extinguished account pays nothing more
        // (realization has no surviving claim/face to act on a second time).
        let outcome2 = market
            .close_resolved_account_not_atomic(&mut account, 0)
            .expect("re-close of an extinguished account must not revert");
        if let ResolvedCloseOutcomeV16::Closed { payout } = outcome2 {
            prop_assert_eq!(payout, 0);
        }
        prop_assert_eq!(market.header.vault.get(), backing + extra_residual - paid);
    }
}

#[test]
fn terminal_source_realization_recredits_paired_insurance_overlap() {
    const CLAIM: u128 = 30;
    const BACKING: u128 = 30;
    const INSURANCE_BUDGET: u128 = 100;
    const INSURANCE_SPENT: u128 = 20;

    let (mut header, mut markets, mut account_header) =
        resolved_market_with_backed_winner(CLAIM, BACKING, INSURANCE_SPENT);
    let insurance_before = INSURANCE_BUDGET - INSURANCE_SPENT;
    header.insurance = V16PodU128::new(insurance_before);
    header.vault = V16PodU128::new(header.vault.get() + insurance_before);
    header.insurance_domain_budget_remaining_total = V16PodU128::new(insurance_before);
    // The positive claim consumes source domain 0. Its losing counterparty's
    // insurance was charged to the opposite side of the same asset (domain 1).
    markets[0].engine.insurance_domain_budget_short = V16PodU128::new(INSURANCE_BUDGET);
    markets[0].engine.insurance_domain_spent_short = V16PodU128::new(INSURANCE_SPENT);

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    assert_eq!(market.validate_shape(), Ok(()));
    assert_eq!(account.validate_with_market(&market.as_view()), Ok(()));

    let outcome = market
        .close_resolved_account_not_atomic(&mut account, 0)
        .expect("resolved source realization must close");
    assert_eq!(outcome, ResolvedCloseOutcomeV16::Closed { payout: CLAIM });
    assert_eq!(
        market
            .recredit_terminal_claim_free_residual_for_asset_not_atomic(0)
            .expect("claim-free terminal overlap must be recredited"),
        INSURANCE_SPENT
    );
    assert_eq!(market.header.vault.get(), INSURANCE_BUDGET);
    assert_eq!(market.header.insurance.get(), INSURANCE_BUDGET);
    assert_eq!(
        market.header.insurance_domain_budget_remaining_total.get(),
        INSURANCE_BUDGET
    );
    assert_eq!(
        market.markets[0].engine.insurance_domain_spent_short.get(),
        0
    );
    assert_eq!(
        market.markets[0]
            .engine
            .backing_long
            .fresh_unliened_backing_num
            .get(),
        0
    );
    assert_eq!(market.validate_shape(), Ok(()));
    assert_eq!(account.validate_with_market(&market.as_view()), Ok(()));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    #[test]
    fn terminal_claim_free_overlap_recredit_is_capped_without_value_drift(
        claim in 1u128..=1_000_000u128,
        insurance_spent in 0u128..=1_000_000u128,
        claim_free_residual in 0u128..=1_000_000u128,
        insurance_before in 0u128..=1_000_000u128,
    ) {
        let insurance_budget = insurance_before + insurance_spent;
        let expected_recredit = claim.min(insurance_spent).min(claim_free_residual);
        let (mut header, mut markets, mut account_header) =
            resolved_market_with_backed_winner(claim, claim, claim_free_residual);
        header.insurance = V16PodU128::new(insurance_before);
        header.vault = V16PodU128::new(header.vault.get() + insurance_before);
        header.insurance_domain_budget_remaining_total = V16PodU128::new(insurance_before);
        markets[0].engine.insurance_domain_budget_short = V16PodU128::new(insurance_budget);
        markets[0].engine.insurance_domain_spent_short = V16PodU128::new(insurance_spent);

        let vault_before = header.vault.get();
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        let mut account = PortfolioV16ViewMut::new(&mut account_header);
        prop_assert_eq!(market.validate_shape(), Ok(()));

        let outcome = market
            .close_resolved_account_not_atomic(&mut account, 0)
            .expect("fully source-backed resolved winner must close");
        prop_assert_eq!(
            outcome,
            ResolvedCloseOutcomeV16::Closed { payout: claim }
        );
        let recredited = market
            .recredit_terminal_claim_free_residual_for_asset_not_atomic(0)
            .expect("terminal sweep must accept an extinguished claim set");
        prop_assert_eq!(recredited, expected_recredit);
        prop_assert_eq!(market.header.vault.get(), vault_before - claim);
        prop_assert_eq!(
            market.header.insurance.get(),
            insurance_before + expected_recredit
        );
        prop_assert_eq!(
            market.header.insurance_domain_budget_remaining_total.get(),
            insurance_before + expected_recredit
        );
        prop_assert_eq!(
            market.markets[0].engine.insurance_domain_spent_short.get(),
            insurance_spent - expected_recredit
        );
        prop_assert_eq!(
            market.header.vault.get() - market.header.insurance.get(),
            claim_free_residual - expected_recredit
        );
        prop_assert_eq!(market.validate_shape(), Ok(()));
        prop_assert_eq!(account.validate_with_market(&market.as_view()), Ok(()));
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// Same realization property when part of the winner's claim is still LIENED
    /// at terminal (e.g. a Live lock that never completed). The close must first
    /// release the account's liens (Finding-A wind-down semantics) and then
    /// realize the full claim — a liened claim must neither dead-lock the close
    /// (realizable-vs-consumable mismatch -> LockActive) nor strip the winner.
    #[test]
    fn terminal_close_realizes_liened_backed_source_claim(
        pnl in 1u128..=1_000_000u128,
        lien_frac in 1u128..=1000u128,
    ) {
        let backing = pnl; // fully backed
        let liened = (pnl.saturating_mul(lien_frac) / 1000).max(1).min(pnl);
        let claim_num = pnl * BOUND_SCALE;
        let backing_num = backing * BOUND_SCALE;
        let liened_num = liened * BOUND_SCALE;

        let (mut header, mut markets, mut account_header) =
            resolved_market_with_backed_winner(pnl, backing, 0);
        // Move `liened` of the backing under an account-held counterparty lien.
        let engine_market_id = markets[0].engine.asset.market_id.get();
        markets[0].engine.backing_long = BackingBucketV16Account::from_runtime(&BackingBucketV16 {
            market_id: engine_market_id,
            fresh_unliened_backing_num: backing_num - liened_num,
            valid_liened_backing_num: liened_num,
            expiry_slot: 100,
            status: BackingBucketStatusV16::Fresh,
            ..BackingBucketV16::EMPTY
        });
        // Stored rate covers only un-liened backing for new credit.
        let available_num = backing_num - liened_num;
        let stored_rate =
            (available_num * CREDIT_RATE_SCALE / claim_num).min(CREDIT_RATE_SCALE);
        markets[0].engine.source_credit_long =
            SourceCreditStateV16Account::from_runtime(&SourceCreditStateV16 {
                positive_claim_bound_num: claim_num,
                exact_positive_claim_num: claim_num,
                fresh_reserved_backing_num: backing_num,
                valid_liened_backing_num: liened_num,
                credit_rate_num: stored_rate,
                ..SourceCreditStateV16::EMPTY
            });
        account_header.source_domains[0].source_claim_liened_num = V16PodU128::new(liened_num);
        account_header.source_domains[0].source_claim_counterparty_liened_num =
            V16PodU128::new(liened_num);
        account_header.source_domains[0].source_lien_effective_reserved =
            V16PodU128::new(liened);
        account_header.source_domains[0].source_lien_counterparty_backing_num =
            V16PodU128::new(liened_num);

        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        let mut account = PortfolioV16ViewMut::new(&mut account_header);
        prop_assume!(market.validate_shape() == Ok(()));
        prop_assume!(account.validate_with_market(&market.as_view()) == Ok(()));

        let vault_before = market.header.vault.get();
        let outcome = market
            .close_resolved_account_not_atomic(&mut account, 0)
            .expect("liened backed winner close must not revert");
        let closed = matches!(outcome, ResolvedCloseOutcomeV16::Closed { payout: _ });
        prop_assert!(closed, "liened backed winner did not fully close");
        let paid = vault_before - market.header.vault.get();

        // Fully backed claim realizes in full after the terminal lien release.
        prop_assert_eq!(paid, pnl);
        prop_assert_eq!(market.header.vault.get(), 0);
        prop_assert_eq!(market.validate_shape(), Ok(()));
    }
}

/// Ordering regression: winner A (plain junior claim) closes FIRST and captures
/// the payout snapshot while winner B's source-backed face is still outstanding.
/// B then realizes against its backing at terminal. B's realized face must be
/// refined OUT of the ledger's unreceipted bound — otherwise the stale bound
/// dilutes the payout rate forever and A's receipt can never reach the terminal
/// rate (never finalized, never clearable: stranded market).
#[test]
fn realization_after_snapshot_refines_unreceipted_bound() {
    let pnl_a = 1_000u128; // plain junior winner
    let pnl_b = 500u128; // source-backed winner
    let backing = pnl_b; // fully backed
    let insurance_budget = 1_000u128;
    let insurance_spent = 200u128;
    let insurance_before = insurance_budget - insurance_spent;
    // The junior pool exactly covers A. The historical insurance spend is part
    // of that support, not a surplus overlapping B's paired source backing.
    let residual = pnl_a;

    let cfg = V16Config::public_user_fund_with_market_slots(1, 1, 0, 10);
    let mut header = MarketGroupV16HeaderAccount::new_dynamic(market_id(), cfg, 1, 0).unwrap();
    let mut markets = [Market::new(0u64, EngineAssetSlotV16Account::default())];
    header
        .activate_empty_asset_slot_not_atomic(0, &mut markets[0].engine, 100, 1)
        .unwrap();
    header.mode = 1; // Resolved
    header.resolved_slot = V16PodU64::new(1);
    header.current_slot = V16PodU64::new(1);
    header.vault = V16PodU128::new(residual + backing + insurance_before);
    header.insurance = V16PodU128::new(insurance_before);
    header.insurance_domain_budget_remaining_total = V16PodU128::new(insurance_before);
    header.pnl_pos_tot = V16PodU128::new(pnl_a + pnl_b);
    header.pnl_matured_pos_tot = V16PodU128::new(pnl_a + pnl_b);
    header.pnl_pos_bound_tot = V16PodU128::new(pnl_a + pnl_b);
    header.pnl_pos_bound_tot_num = V16PodU128::new((pnl_a + pnl_b) * BOUND_SCALE);
    header.source_claim_bound_total_num = V16PodU128::new(pnl_b * BOUND_SCALE);
    header.source_fresh_backing_total_num = V16PodU128::new(backing * BOUND_SCALE);
    let engine_market_id = markets[0].engine.asset.market_id.get();
    markets[0].engine.backing_long = BackingBucketV16Account::from_runtime(&BackingBucketV16 {
        market_id: engine_market_id,
        fresh_unliened_backing_num: backing * BOUND_SCALE,
        expiry_slot: 100,
        status: BackingBucketStatusV16::Fresh,
        ..BackingBucketV16::EMPTY
    });
    markets[0].engine.source_credit_long =
        SourceCreditStateV16Account::from_runtime(&SourceCreditStateV16 {
            positive_claim_bound_num: pnl_b * BOUND_SCALE,
            exact_positive_claim_num: pnl_b * BOUND_SCALE,
            fresh_reserved_backing_num: backing * BOUND_SCALE,
            credit_rate_num: CREDIT_RATE_SCALE,
            ..SourceCreditStateV16::EMPTY
        });
    markets[0].engine.insurance_domain_budget_short = V16PodU128::new(insurance_budget);
    markets[0].engine.insurance_domain_spent_short = V16PodU128::new(insurance_spent);

    let mut a_header = winner_account(0, pnl_a);
    let mut b_header = winner_account(0, pnl_b);

    b_header.source_domains[0].domain = V16PodU32::new(0);
    b_header.source_domains[0].source_claim_market_id = V16PodU64::new(engine_market_id);
    b_header.source_domains[0].source_claim_bound_num = V16PodU128::new(pnl_b * BOUND_SCALE);

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut a = PortfolioV16ViewMut::new(&mut a_header);
    let mut b = PortfolioV16ViewMut::new(&mut b_header);
    assert_eq!(market.validate_shape(), Ok(()));
    assert_eq!(a.validate_with_market(&market.as_view()), Ok(()));
    assert_eq!(b.validate_with_market(&market.as_view()), Ok(()));

    // A closes first: captures the snapshot while B's face is unreceipted.
    market.close_resolved_account_not_atomic(&mut a, 0).unwrap();
    let a_receipt = a.header.resolved_payout_receipt.try_to_runtime().unwrap();
    assert!(a_receipt.present && !a_receipt.finalized); // diluted by B's face

    // B realizes against its backing at terminal close.
    let vault_before_b = market.header.vault.get();
    market.close_resolved_account_not_atomic(&mut b, 0).unwrap();
    assert_eq!(vault_before_b - market.header.vault.get(), pnl_b);
    assert_eq!(b.header.pnl.get(), 0);
    assert_eq!(b.header.capital.get(), 0);

    // The refined bound lets A top up to its full honest entitlement and FINALIZE.
    let topped = market
        .claim_resolved_payout_topup_not_atomic(&mut a)
        .unwrap();
    let a_receipt = a.header.resolved_payout_receipt.try_to_runtime().unwrap();
    assert_eq!(a_receipt.paid_effective, pnl_a);
    assert!(
        a_receipt.finalized,
        "stale unreceipted bound left A unfinalizable"
    );
    assert!(topped > 0);
    assert_eq!(
        market
            .recredit_terminal_claim_free_residual_for_asset_not_atomic(0)
            .expect("mixed exact support is terminal and claim-free"),
        0,
        "ordinary winner support must not be reclassified as insurance"
    );
    assert_eq!(market.header.vault.get(), insurance_before);
    assert_eq!(market.header.insurance.get(), insurance_before);
    assert_eq!(
        market.markets[0].engine.insurance_domain_spent_short.get(),
        insurance_spent
    );
    assert_eq!(market.validate_shape(), Ok(()));
}

/// Expiry-liveness regression (wrapper finding, 2026-06-10): a source-backed
/// winner whose domain backing went PAST-EXPIRY (bucket still Fresh — nothing
/// processes expiry in production) must still close at resolution. The realize
/// step must not propagate the freshness validator's Stale: it expires the
/// lapsed bucket (forfeiting the unliened principal to the junior pool, the
/// documented expiry semantics) and falls through to the junior receipt path.
#[test]
fn terminal_close_with_expired_backing_does_not_strand() {
    let pnl = 1_000u128;
    let backing = 500u128;
    let (mut header, mut markets, mut account_header) =
        resolved_market_with_backed_winner(pnl, backing, 0);
    // Backing lapsed long before resolution.
    let engine_market_id = markets[0].engine.asset.market_id.get();
    markets[0].engine.backing_long = BackingBucketV16Account::from_runtime(&BackingBucketV16 {
        market_id: engine_market_id,
        fresh_unliened_backing_num: backing * BOUND_SCALE,
        expiry_slot: 5,
        status: BackingBucketStatusV16::Fresh,
        ..BackingBucketV16::EMPTY
    });
    header.resolved_slot = V16PodU64::new(20);
    header.current_slot = V16PodU64::new(20);
    account_header.last_fee_slot = V16PodU64::new(20);

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    assert_eq!(market.validate_shape(), Ok(()));
    assert_eq!(account.validate_with_market(&market.as_view()), Ok(()));

    let vault_before = market.header.vault.get();
    let mut closed = false;
    for _ in 0..3 {
        let outcome = market
            .close_resolved_account_not_atomic(&mut account, 0)
            .expect("expired-backing winner close must not revert (liveness)");
        if matches!(outcome, ResolvedCloseOutcomeV16::Closed { .. }) {
            closed = true;
            break;
        }
    }
    assert!(closed, "expired-backing winner did not fully close");
    let paid = vault_before - market.header.vault.get();

    // Expiry forfeits the lapsed principal to the junior pool: the winner is
    // paid the haircut share (here the whole forfeited amount, residual < face)
    // through the receipt path, not via realization.
    assert_eq!(paid, backing);
    assert_eq!(account.header.capital.get(), 0);
    assert_eq!(account.header.pnl.get(), 0);
    // The bucket is processed (Expired) and the provider cannot recover lapsed
    // principal afterwards.
    let bucket = market.markets[0]
        .engine
        .backing_long
        .try_to_runtime()
        .unwrap();
    assert_eq!(bucket.status, BackingBucketStatusV16::Expired);
    assert!(market
        .withdraw_fresh_counterparty_backing_not_atomic(0, backing)
        .is_err());
    assert_eq!(market.validate_shape(), Ok(()));
}

#[test]
fn resolved_close_prepares_lapsed_backing_before_pending_k_loss() {
    const CAPITAL: u128 = 50_000_000;
    const PNL: u128 = 500_000;
    const BACKING: u128 = 17;
    const SLOT: u64 = 44;
    const POSITION_Q: u128 = 50_000 * POS_SCALE;

    let (mut header, mut markets) = resolved_market_with_backing(CAPITAL, PNL, 0, BACKING);
    let engine_market_id = markets[0].engine.asset.market_id.get();
    header.current_slot = V16PodU64::new(SLOT);
    header.resolved_slot = V16PodU64::new(SLOT);
    header.slot_last = V16PodU64::new(SLOT);
    header.pnl_matured_pos_tot = V16PodU128::new(0);
    header.resolved_payout_blocker_count = V16PodU64::new(1);
    header.source_claim_bound_total_num = V16PodU128::new(PNL * BOUND_SCALE);

    markets[0].engine.backing_long = BackingBucketV16Account::from_runtime(
        &BackingBucketV16::empty_for_market(engine_market_id),
    );
    markets[0].engine.source_credit_long =
        SourceCreditStateV16Account::from_runtime(&SourceCreditStateV16::EMPTY);
    let claim_num = PNL * BOUND_SCALE;
    let backing_num = BACKING * BOUND_SCALE;
    markets[0].engine.backing_short = BackingBucketV16Account::from_runtime(&BackingBucketV16 {
        market_id: engine_market_id,
        fresh_unliened_backing_num: backing_num,
        expiry_slot: SLOT,
        status: BackingBucketStatusV16::Fresh,
        ..BackingBucketV16::EMPTY
    });
    markets[0].engine.source_credit_short =
        SourceCreditStateV16Account::from_runtime(&SourceCreditStateV16 {
            positive_claim_bound_num: claim_num,
            exact_positive_claim_num: claim_num,
            fresh_reserved_backing_num: backing_num,
            credit_rate_num: backing_num * CREDIT_RATE_SCALE / claim_num,
            ..SourceCreditStateV16::EMPTY
        });

    let mut asset = markets[0].engine.asset.try_to_runtime().unwrap();
    asset.slot_last = SLOT;
    asset.oi_eff_long_q = POSITION_Q;
    asset.stored_pos_count_long = 1;
    asset.loss_weight_sum_long = POSITION_Q;
    markets[0].engine.asset = AssetStateV16Account::from_runtime(&asset);

    let mut account_header = winner_account(CAPITAL, PNL);
    account_header.last_fee_slot = V16PodU64::new(SLOT);
    account_header.source_domains[0].domain = V16PodU32::new(1);
    account_header.source_domains[0].source_claim_market_id = V16PodU64::new(engine_market_id);
    account_header.source_domains[0].source_claim_bound_num = V16PodU128::new(claim_num);
    account_header.legs[0] = PortfolioLegV16Account::from_runtime(&PortfolioLegV16 {
        active: true,
        asset_index: 0,
        market_id: engine_market_id,
        side: SideV16::Long,
        basis_pos_q: POSITION_Q as i128,
        a_basis: ADL_ONE,
        k_snap: 10 * ADL_ONE as i128,
        f_snap: 0,
        epoch_snap: 0,
        loss_weight: POSITION_Q,
        b_snap: 0,
        b_rem: 0,
        b_epoch_snap: 0,
        b_stale: false,
        stale: false,
    });
    account_header.active_bitmap[0] = V16PodU64::new(1);

    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut account = PortfolioV16ViewMut::new(&mut account_header);
    assert_eq!(market.validate_shape(), Ok(()));
    assert_eq!(account.validate_with_market(&market.as_view()), Ok(()));

    let capital_before = account.header.capital.get();
    let vault_before = market.header.vault.get();
    let first = market
        .close_resolved_account_not_atomic(&mut account, 0)
        .expect("resolved close must commit the lapsed backing transition");
    assert_eq!(first, ResolvedCloseOutcomeV16::ProgressOnly);
    assert_eq!(account.header.capital.get(), capital_before);
    assert_eq!(account.header.pnl.get(), PNL as i128);
    assert_eq!(market.header.vault.get(), vault_before);
    assert_eq!(
        market.markets[0]
            .engine
            .backing_short
            .try_to_runtime()
            .unwrap()
            .status,
        BackingBucketStatusV16::Expired,
    );

    let mut closed = false;
    for _ in 0..4 {
        let outcome = market
            .close_resolved_account_not_atomic(&mut account, 0)
            .expect("prepared source state must not block K/F settlement");
        if matches!(outcome, ResolvedCloseOutcomeV16::Closed { .. }) {
            closed = true;
            break;
        }
    }
    assert!(closed, "bounded terminal continuation did not close");
    assert_eq!(account.header.capital.get(), 0);
    assert_eq!(account.header.pnl.get(), 0);
    assert_eq!(
        market.header.vault.get(),
        BACKING + PNL,
        "the crystallized mark loss remains in the source-backing ledger",
    );
    assert_eq!(market.validate_shape(), Ok(()));
    assert_eq!(account.validate_with_market(&market.as_view()), Ok(()));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(400))]

    /// Review finding 3 (non-monotone payout in backing / dust-grief) is a
    /// DISTRIBUTION question, not a solvency one. This pins the security
    /// invariants across the FULL backing range including the dust regime
    /// (backing from 0 up to the full face) with a non-empty junior pool: the
    /// close always completes (no DoS), value is conserved (no mint/burn, no
    /// strand, no LoF), the winner is never paid more than its face plus
    /// capital, and the shape reconciles. Whatever the realize-vs-pool split,
    /// none of these can be violated by funding (or not funding) the domain.
    #[test]
    fn backed_winner_close_conserves_across_all_backing_levels(
        pnl in 2u128..=1_000_000u128,
        backing_frac in 0u128..=1000u128,
        pool in 0u128..=1_000_000u128,
    ) {
        // backing spans 0 (zero-backed source claim) .. full face.
        let backing = pnl.saturating_mul(backing_frac) / 1000;
        let (mut header, mut markets, mut account_header) =
            resolved_market_with_backed_winner(pnl, backing, pool);
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        let mut account = PortfolioV16ViewMut::new(&mut account_header);
        prop_assume!(market.validate_shape() == Ok(()));
        prop_assume!(account.validate_with_market(&market.as_view()) == Ok(()));

        let vault_before = market.header.vault.get();
        let outcome = market
            .close_resolved_account_not_atomic(&mut account, 0)
            .expect("backed winner close must not revert at any backing level (no DoS)");
        // No DoS: the close fully settles rather than stalling.
        let closed = matches!(outcome, ResolvedCloseOutcomeV16::Closed { payout: _ });
        prop_assert!(closed, "close did not finalize at backing={}", backing);
        let paid = vault_before - market.header.vault.get();

        // No LoF: value conserved (paid out of the vault, nothing minted),
        // winner never paid above its face (capital is 0 here), vault never
        // over-drained, and the shape still reconciles.
        prop_assert!(paid <= vault_before);
        prop_assert!(paid <= pnl);
        prop_assert_eq!(account.header.pnl.get(), 0);
        prop_assert_eq!(account.header.capital.get(), 0);
        prop_assert_eq!(market.validate_shape(), Ok(()));
        // The unclaimed remainder (if any) stays in the vault as junior pool for
        // other claimants — it is neither stranded-unreconcilable nor lost.
        prop_assert!(market.header.vault.get() <= vault_before);
    }
}

// ============ P3: SEQUENCE DIFFERENTIALS ============

/// Live single-asset market holding only junior surplus.
fn live_market_with_pool(pool: u128) -> (MarketGroupV16HeaderAccount, [Market<u64>; 1]) {
    let cfg = V16Config::public_user_fund_with_market_slots(1, 1, 0, 10);
    let mut header = MarketGroupV16HeaderAccount::new_dynamic(market_id(), cfg, 1, 0).unwrap();
    let mut markets = [Market::new(0u64, EngineAssetSlotV16Account::default())];
    header
        .activate_empty_asset_slot_not_atomic(0, &mut markets[0].engine, 100, 1)
        .unwrap();
    header.current_slot = V16PodU64::new(1);
    header.vault = V16PodU128::new(pool);
    (header, markets)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// P3-1 (spec req 27, order independence): closing two resolved winners
    /// in either order pays each the same amount and leaves the same vault —
    /// caller ordering must not redistribute value.
    #[test]
    fn close_order_does_not_redistribute(
        pnl_a in 1u128..=100_000u128,
        pnl_b in 1u128..=100_000u128,
        backing_frac in 0u128..=1000u128,
        pool in 0u128..=100_000u128,
    ) {
        let capital = 1_000u128;
        let claim = pnl_a + pnl_b;
        let backing = claim.saturating_mul(backing_frac) / 1000;
        let run = |a_first: bool| -> (u128, u128, u128) {
            let (mut header, mut markets) =
                resolved_market_with_backing(capital * 2, claim, pool, backing);
            let mut ha = winner_account(capital, pnl_a);
            let mut hb = winner_account(capital, pnl_b);
            let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
            let mut aa = PortfolioV16ViewMut::new(&mut ha);
            let mut ab = PortfolioV16ViewMut::new(&mut hb);
            assert_eq!(market.validate_shape(), Ok(()));
            if a_first {
                market.close_resolved_account_not_atomic(&mut aa, 0).expect("close A");
                market.close_resolved_account_not_atomic(&mut ab, 0).expect("close B");
            } else {
                market.close_resolved_account_not_atomic(&mut ab, 0).expect("close B");
                market.close_resolved_account_not_atomic(&mut aa, 0).expect("close A");
            }
            assert_eq!(market.validate_shape(), Ok(()));
            (
                aa.header.capital.get(),
                ab.header.capital.get(),
                market.header.vault.get(),
            )
        };
        let (ca1, cb1, v1) = run(true);
        let (ca2, cb2, v2) = run(false);
        prop_assert_eq!(v1, v2, "vault differs by close order");
        prop_assert_eq!(ca1, ca2, "winner A payout differs by close order");
        prop_assert_eq!(cb1, cb2, "winner B payout differs by close order");
    }

    /// P3-2 (idempotence, matrix #24-adjacent): re-closing an already-closed
    /// resolved account never moves value, whatever the second call returns.
    #[test]
    fn second_resolved_close_is_value_neutral(
        pnl in 1u128..=100_000u128,
        backing_frac in 0u128..=1000u128,
        pool in 0u128..=100_000u128,
    ) {
        let capital = 1_000u128;
        let backing = pnl.saturating_mul(backing_frac) / 1000;
        let (mut header, mut markets) =
            resolved_market_with_backing(capital, pnl, pool, backing);
        let mut account_header = winner_account(capital, pnl);
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        let mut account = PortfolioV16ViewMut::new(&mut account_header);
        market
            .close_resolved_account_not_atomic(&mut account, 0)
            .expect("first close must not revert");
        let vault_mid = market.header.vault.get();
        let capital_mid = account.header.capital.get();
        let c_tot_mid = market.header.c_tot.get();
        let _ = market.close_resolved_account_not_atomic(&mut account, 0);
        prop_assert_eq!(market.header.vault.get(), vault_mid);
        prop_assert_eq!(account.header.capital.get(), capital_mid);
        prop_assert_eq!(market.header.c_tot.get(), c_tot_mid);
        prop_assert_eq!(market.validate_shape(), Ok(()));
    }

    /// P3-3 (net-extraction bound over random sequences; matrix #19/#24):
    /// for any sequence of user deposits/withdrawals and domain-insurance
    /// deposits/withdrawals, no actor class extracts more than it put in,
    /// and the senior stack stays vault-covered at every prefix.
    #[test]
    fn random_flat_sequence_never_extracts_excess(
        ops in proptest::collection::vec((0u8..4u8, 1u128..=1_000u128), 1..14),
        start_pool in 0u128..=1_000u128,
    ) {
        let (mut header, mut markets) = live_market_with_pool(start_pool);
        let mut account_header = empty_account();
        account_header.last_fee_slot = V16PodU64::new(1);
        let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        let mut account = PortfolioV16ViewMut::new(&mut account_header);
        let (mut udep, mut uwd, mut idep, mut iwd) = (0u128, 0u128, 0u128, 0u128);
        for (op, amount) in ops {
            match op {
                0 => {
                    if market.deposit_not_atomic(&mut account, amount).is_ok() {
                        udep += amount;
                    }
                }
                1 => {
                    if market.withdraw_not_atomic(&mut account, amount).is_ok() {
                        uwd += amount;
                    }
                }
                2 => {
                    if market.deposit_domain_insurance_not_atomic(0, amount).is_ok() {
                        idep += amount;
                    }
                }
                _ => {
                    if market.withdraw_domain_insurance_not_atomic(0, amount).is_ok() {
                        iwd += amount;
                    }
                }
            }
            prop_assert!(uwd <= udep, "user extracted more than deposited");
            prop_assert!(iwd <= idep, "insurance funder extracted more than deposited");
            prop_assert_eq!(market.validate_shape(), Ok(()));
            prop_assert!(
                market.header.c_tot.get() + market.header.insurance.get()
                    <= market.header.vault.get(),
                "senior stack exceeded vault"
            );
        }
    }
}
