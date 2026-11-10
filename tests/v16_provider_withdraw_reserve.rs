use percolator::{
    EngineAssetSlotV16Account, Market, MarketGroupV16HeaderAccount, MarketGroupV16ViewMut,
    PortfolioAccountV16Account, PortfolioV16ViewMut, ProvenanceHeaderV16,
    ProvenanceHeaderV16Account, ResolvedPayoutLedgerV16, ResolvedPayoutLedgerV16Account,
    ResolvedPayoutReceiptV16, ResolvedPayoutReceiptV16Account, V16Config, V16PodU128, V16PodU64,
    BOUND_SCALE,
};

const PROVIDER: u128 = 100_000;
const RESIDUAL: u128 = 10_000;
const CLAIM_FACE: u128 = 50_000;
const JUNIOR_NEED: u128 = 40_000;
const SURPLUS: u128 = 60_000;
const DOMAIN: usize = 1;
const EXPIRY: u64 = 50;

fn empty_account() -> PortfolioAccountV16Account {
    let header = ProvenanceHeaderV16Account::from_runtime(&ProvenanceHeaderV16::new(
        [1u8; 32], [2u8; 32], [2u8; 32],
    ));
    let mut account = PortfolioAccountV16Account::default();
    account.init_empty_in_place(header).unwrap();
    account
}

fn fresh_activated_market() -> (MarketGroupV16HeaderAccount, [Market<u64>; 1]) {
    let cfg = V16Config::public_user_fund_with_market_slots(1, 1, 0, 10);
    let mut header = MarketGroupV16HeaderAccount::new_dynamic([1u8; 32], cfg, 1, 0).unwrap();
    let mut markets = [Market::new(0u64, EngineAssetSlotV16Account::default())];
    header
        .activate_empty_asset_slot_not_atomic(0, &mut markets[0].engine, 100, 1)
        .unwrap();
    (header, markets)
}

/// The pre-exploit state: provider-backed, resolved, payout snapshot captured.
fn resolved_market_with_provider_and_claimant() -> (
    MarketGroupV16HeaderAccount,
    [Market<u64>; 1],
    PortfolioAccountV16Account,
) {
    let (mut header, mut markets) = fresh_activated_market();
    {
        let mut m = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        m.deposit_fresh_counterparty_backing_not_atomic(DOMAIN, PROVIDER, EXPIRY)
            .unwrap();
    }
    // Resolve with every portfolio closed: the pool holds only RESIDUAL
    // (crystallized losses), and the claimants' whole CLAIM_FACE is frozen
    // into receipts. The provider's backing has not expired yet.
    header.mode = 1; // MarketModeV16::Resolved
    header.resolved_slot = V16PodU64::new(1);
    header.current_slot = V16PodU64::new(1);
    header.vault = V16PodU128::new(PROVIDER + RESIDUAL);
    header.payout_snapshot_captured = 1;
    header.resolved_payout_ledger =
        ResolvedPayoutLedgerV16Account::from_runtime(&ResolvedPayoutLedgerV16 {
            snapshot_residual: RESIDUAL,
            terminal_claim_exact_receipts_num: CLAIM_FACE * BOUND_SCALE,
            terminal_claim_bound_unreceipted_num: 0,
            current_payout_rate_num: RESIDUAL * BOUND_SCALE,
            current_payout_rate_den: CLAIM_FACE * BOUND_SCALE,
            snapshot_slot: 1,
            payout_halted: false,
            finalized: false,
        });

    // One claimant holds the whole 50_000 receipt (the maintainers' own fuzz
    // fixture shape: the receipt's contribution equals the ledger's exact
    // receipts).
    let mut claimant = empty_account();
    claimant.resolved_payout_receipt =
        ResolvedPayoutReceiptV16Account::from_runtime(&ResolvedPayoutReceiptV16 {
            present: true,
            prior_bound_contribution_num: CLAIM_FACE * BOUND_SCALE,
            live_released_face_at_receipt: 0,
            terminal_positive_claim_face: CLAIM_FACE,
            paid_effective: 0,
            finalized: false,
        });
    (header, markets, claimant)
}

/// Ordinary withdrawals must preserve the resolved junior payout reserve.
#[test]
fn ordinary_withdraw_preserves_the_resolved_junior_need() {
    let (mut header, mut markets, mut claimant_h) = resolved_market_with_provider_and_claimant();
    let mut m = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut claimant = PortfolioV16ViewMut::new(&mut claimant_h);
    assert_eq!(m.validate_shape(), Ok(()));
    assert_eq!(claimant.validate_with_market(&m.as_view()), Ok(()));
    assert_eq!(m.resolved_counterparty_backing_surplus(DOMAIN), Ok(SURPLUS));
    let header_before = *m.header;
    let engine_before = m.markets[0].engine;
    for amount in [PROVIDER, SURPLUS + 1] {
        assert_eq!(
            m.withdraw_fresh_counterparty_backing_not_atomic(DOMAIN, amount),
            Err(percolator::V16Error::LockActive)
        );
        assert_eq!(
            bytemuck::bytes_of(m.header),
            bytemuck::bytes_of(&header_before)
        );
        assert_eq!(
            bytemuck::bytes_of(&m.markets[0].engine),
            bytemuck::bytes_of(&engine_before)
        );
    }
    m.withdraw_fresh_counterparty_backing_not_atomic(DOMAIN, SURPLUS)
        .expect("ordinary surplus withdrawal must succeed");
    assert_eq!(m.header.vault.get(), RESIDUAL + JUNIOR_NEED);
    assert_eq!(m.validate_shape(), Ok(()));
    m.expire_source_backing_bucket_not_atomic(DOMAIN, EXPIRY + 1)
        .unwrap();
    assert_eq!(
        m.claim_resolved_payout_topup_not_atomic(&mut claimant),
        Ok(CLAIM_FACE)
    );
    assert_eq!(m.header.vault.get(), 0);
}

/// CONTROL: the same state through the sanctioned path. The provider takes only
/// the surplus; at expiry the remaining need forfeits into the pool and the
/// claimant collects the whole face.
#[test]
fn sanctioned_path_pays_the_claimant_in_full() {
    let (mut header, mut markets, mut claimant_h) = resolved_market_with_provider_and_claimant();
    let mut m = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut claimant = PortfolioV16ViewMut::new(&mut claimant_h);

    m.withdraw_resolved_counterparty_backing_surplus_not_atomic(DOMAIN, SURPLUS)
        .expect("surplus withdrawal must succeed");
    assert_eq!(m.header.vault.get(), PROVIDER + RESIDUAL - SURPLUS);

    // The 40_000 need forfeits into the payout pool at expiry...
    m.expire_source_backing_bucket_not_atomic(DOMAIN, EXPIRY + 1)
        .expect("bucket expiry must succeed");

    // ...the rate is full, and the claimant collects the whole 50_000 face.
    let paid = m
        .claim_resolved_payout_topup_not_atomic(&mut claimant)
        .expect("claim must succeed");
    assert_eq!(paid, CLAIM_FACE);
    assert_eq!(m.header.vault.get(), 0);
}

#[test]
fn ordinary_withdraw_preserves_live_behavior() {
    let (mut header, mut markets) = fresh_activated_market();
    let mut m = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    m.deposit_fresh_counterparty_backing_not_atomic(DOMAIN, PROVIDER, EXPIRY)
        .unwrap();
    m.withdraw_fresh_counterparty_backing_not_atomic(DOMAIN, PROVIDER)
        .unwrap();
    assert_eq!(m.header.vault.get(), 0);
    assert_eq!(m.validate_shape(), Ok(()));
}

#[test]
fn ordinary_withdraw_rejects_unready_and_halted_resolved_pools() {
    let (mut header, mut markets, _) = resolved_market_with_provider_and_claimant();
    header.payout_snapshot_captured = 0;
    header.negative_pnl_account_count = V16PodU64::new(1);
    let mut m = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    assert_eq!(
        m.withdraw_fresh_counterparty_backing_not_atomic(DOMAIN, 1),
        Err(percolator::V16Error::LockActive)
    );
    m.header.payout_snapshot_captured = 1;
    m.header.negative_pnl_account_count = V16PodU64::new(0);
    let mut ledger = m.header.resolved_payout_ledger.try_to_runtime().unwrap();
    ledger.payout_halted = true;
    m.header.resolved_payout_ledger = ResolvedPayoutLedgerV16Account::from_runtime(&ledger);
    assert_eq!(
        m.withdraw_fresh_counterparty_backing_not_atomic(DOMAIN, 1),
        Err(percolator::V16Error::RecoveryRequired)
    );
    m.header.current_slot = V16PodU64::new(EXPIRY);
    assert_eq!(
        m.withdraw_fresh_counterparty_backing_not_atomic(DOMAIN, 1),
        Err(percolator::V16Error::Stale)
    );
}
