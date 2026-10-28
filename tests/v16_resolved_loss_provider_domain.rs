//! S2: a loser's K/F loss first crystallized in Resolved mode must repay the
//! backing domain whose provider funds the opposite-side winners, exactly as
//! the same loss does in Live mode. Before the fix the loss went to the
//! junior residual whenever the domain's current claims were already covered
//! (or not yet attributed), the winners were paid from provider principal,
//! and terminal cleanup retired the loser's money: the provider lost it.
//!
//! Setup shared by most tests: provider backs domain 1 (asset 0, Short) with
//! 100_000; two longs (1_000_000 each) and one short loser (2_000_000) trade
//! 1_000 units at 100, the mark moves to 105, the market resolves. Each long
//! is owed 5_000, the short owes 10_000.
use percolator::{
    BackingBucketStatusV16, EngineAssetSlotV16Account, Market, MarketGroupV16HeaderAccount,
    MarketGroupV16ViewMut, PortfolioAccountV16Account, PortfolioV16ViewMut, ProvenanceHeaderV16,
    ProvenanceHeaderV16Account, ResolvedCloseOutcomeV16, TerminalSlabOutcomeV16, TradeRequestV16,
    V16Config, BOUND_SCALE, CREDIT_RATE_SCALE, POS_SCALE,
};

const Q: u128 = 1_000 * POS_SCALE;
const PROVIDER: u128 = 100_000;
const PROVIDER_DOMAIN: usize = 1;
const WINNER_PAYOUT: u128 = 1_005_000;
const LOSER_PAYOUT: u128 = 1_990_000;

fn account_fixture(seed: u8) -> PortfolioAccountV16Account {
    let header = ProvenanceHeaderV16Account::from_runtime(&ProvenanceHeaderV16::new(
        [1; 32], [seed; 32], [3; 32],
    ));
    let mut account = PortfolioAccountV16Account::default();
    account.init_empty_in_place(header).unwrap();
    account
}

fn one_asset_market() -> (MarketGroupV16HeaderAccount, Vec<Market<u64>>) {
    let mut cfg = V16Config::public_user_fund_with_market_slots(1, 1, 0, 10);
    cfg.max_price_move_bps_per_slot = 500;
    cfg.max_accrual_dt_slots = 1;
    let mut header = MarketGroupV16HeaderAccount::new_dynamic([1; 32], cfg, 1, 0).unwrap();
    let mut markets = vec![Market::new(0, EngineAssetSlotV16Account::default())];
    header
        .activate_empty_asset_slot_not_atomic(0, &mut markets[0].engine, 100, 1)
        .unwrap();
    (header, markets)
}

fn trade(
    m: &mut MarketGroupV16ViewMut<'_, u64>,
    long: &mut PortfolioV16ViewMut<'_>,
    short: &mut PortfolioV16ViewMut<'_>,
    q: u128,
    px: u64,
) {
    m.execute_trade_with_fee_loss_stale_scoped_not_atomic(
        long,
        short,
        TradeRequestV16 {
            asset_index: 0,
            size_q: q as i128,
            exec_price: px,
            fee_bps: 0,
        },
    )
    .unwrap();
}

fn bucket(m: &MarketGroupV16ViewMut<'_, u64>, domain: usize) -> percolator::BackingBucketV16 {
    let e = &m.markets[0].engine;
    if domain == 0 {
        e.backing_long.try_to_runtime().unwrap()
    } else {
        e.backing_short.try_to_runtime().unwrap()
    }
}

fn source(m: &MarketGroupV16ViewMut<'_, u64>, domain: usize) -> percolator::SourceCreditStateV16 {
    let e = &m.markets[0].engine;
    if domain == 0 {
        e.source_credit_long.try_to_runtime().unwrap()
    } else {
        e.source_credit_short.try_to_runtime().unwrap()
    }
}

fn fresh_atoms(m: &MarketGroupV16ViewMut<'_, u64>, domain: usize) -> u128 {
    let b = bucket(m, domain);
    if b.status == BackingBucketStatusV16::Fresh {
        b.fresh_unliened_backing_num / BOUND_SCALE
    } else {
        0
    }
}

/// Global and per-domain conservation: the vault covers every senior claim
/// (capital, insurance, provider earnings and recoverable backing) and no
/// domain's claim bound exceeds its backing while the provider is funded.
fn assert_conserved(m: &MarketGroupV16ViewMut<'_, u64>, provider_funded: bool) {
    m.validate_shape().unwrap();
    let h = &m.header;
    let senior = h.c_tot.get()
        + h.insurance.get()
        + h.backing_provider_earnings_total.get()
        + h.source_fresh_backing_total_num.get() / BOUND_SCALE;
    assert!(
        h.vault.get() >= senior,
        "vault {} < senior claims {senior}",
        h.vault.get()
    );
    if provider_funded {
        for d in 0..2 {
            let s = source(m, d);
            let available = s.fresh_reserved_backing_num - s.valid_liened_backing_num
                + s.insurance_credit_reserved_num;
            assert!(
                available >= s.positive_claim_bound_num,
                "domain {d}: claims {} exceed backing {available}",
                s.positive_claim_bound_num
            );
            assert_eq!(s.credit_rate_num, CREDIT_RATE_SCALE);
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Outcome {
    provider: u128,
    winner_a: u128,
    winner_b: u128,
    loser: u128,
    insurance: u128,
    retired: u128,
    vault_end: u128,
}

/// live_refresh: 0 none, 1 loser refreshed in Live, 2 winners refreshed in
/// Live, 3 winners then loser. `loser_first` picks the resolved close order.
fn run(loser_first: bool, surplus_midway: bool, live_refresh: u8) -> Outcome {
    const EXPIRY: u64 = 50;
    let (mut header, mut markets) = one_asset_market();
    let mut wa_h = account_fixture(60);
    let mut wb_h = account_fixture(61);
    let mut l_h = account_fixture(62);
    let mut m = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut wa = PortfolioV16ViewMut::new(&mut wa_h);
    let mut wb = PortfolioV16ViewMut::new(&mut wb_h);
    let mut l = PortfolioV16ViewMut::new(&mut l_h);
    m.deposit_fresh_counterparty_backing_not_atomic(PROVIDER_DOMAIN, PROVIDER, EXPIRY)
        .unwrap();
    m.deposit_not_atomic(&mut wa, 1_000_000).unwrap();
    m.deposit_not_atomic(&mut wb, 1_000_000).unwrap();
    m.deposit_not_atomic(&mut l, 2_000_000).unwrap();
    trade(&mut m, &mut wa, &mut l, Q, 100);
    trade(&mut m, &mut wb, &mut l, Q, 100);
    m.set_asset_raw_oracle_target_not_atomic(0, 105).unwrap();
    m.accrue_asset_to_not_atomic(0, 2, 105, 0, true).unwrap();
    if live_refresh == 2 || live_refresh == 3 {
        m.full_account_refresh_not_atomic(&mut wa).unwrap();
        m.full_account_refresh_not_atomic(&mut wb).unwrap();
    }
    if live_refresh == 1 || live_refresh == 3 {
        m.full_account_refresh_not_atomic(&mut l).unwrap();
    }
    assert_conserved(&m, true);
    m.resolve_market_not_atomic(4).unwrap();

    let mut pay: [Option<u128>; 3] = [None, None, None];
    let mut provider = 0u128;
    let mut midway_done = false;
    for _ in 0..64 {
        let order: [usize; 3] = if loser_first { [2, 0, 1] } else { [0, 1, 2] };
        for &i in &order {
            if pay[i].is_some() {
                continue;
            }
            let acct: &mut PortfolioV16ViewMut<'_> = match i {
                0 => &mut wa,
                1 => &mut wb,
                _ => &mut l,
            };
            let snapshot_before = m.header.payout_snapshot_captured;
            match m.close_resolved_account_not_atomic(acct, 0) {
                Ok(ResolvedCloseOutcomeV16::ProgressOnly) => {}
                Ok(ResolvedCloseOutcomeV16::Closed { payout }) => pay[i] = Some(payout),
                Err(e) => panic!("{i} close failed: {e:?}"),
            }
            if i == 2 {
                // The loser's resolved loss is always booked before the
                // payout snapshot: winners cannot realize, and so cannot
                // capture it, while any stored position or negative PnL
                // remains.
                assert_eq!(snapshot_before, 0, "loss booked after payout capture");
            }
            assert_conserved(&m, true);
        }
        if surplus_midway && !midway_done {
            if let Ok(s) = m.resolved_counterparty_backing_surplus(PROVIDER_DOMAIN) {
                if s > 0 {
                    m.withdraw_resolved_counterparty_backing_surplus_not_atomic(PROVIDER_DOMAIN, s)
                        .unwrap();
                    provider += s;
                    midway_done = true;
                    assert_conserved(&m, true);
                }
            }
        }
        if pay.iter().all(Option::is_some) {
            break;
        }
    }
    let rest = fresh_atoms(&m, PROVIDER_DOMAIN);
    if rest > 0 {
        m.withdraw_fresh_counterparty_backing_not_atomic(PROVIDER_DOMAIN, rest)
            .unwrap();
        provider += rest;
    }
    assert_conserved(&m, false);
    let mut retired = 0;
    let mut start = 0usize;
    for _ in 0..16 {
        match m
            .advance_terminal_slab_not_atomic(EXPIRY + 1, start)
            .unwrap()
        {
            TerminalSlabOutcomeV16::ReadyToClose { retired: r } => {
                retired = r;
                break;
            }
            TerminalSlabOutcomeV16::ScanProgress { next_asset_index } => start = next_asset_index,
            _ => start = 0,
        }
    }
    Outcome {
        provider,
        winner_a: pay[0].unwrap(),
        winner_b: pay[1].unwrap(),
        loser: pay[2].unwrap(),
        insurance: m.header.insurance.get(),
        retired,
        vault_end: m.header.vault.get(),
    }
}

fn expected() -> Outcome {
    Outcome {
        provider: PROVIDER,
        winner_a: WINNER_PAYOUT,
        winner_b: WINNER_PAYOUT,
        loser: LOSER_PAYOUT,
        insurance: 0,
        retired: 0,
        vault_end: 0,
    }
}

#[test]
fn s2_resolved_loser_loss_repays_provider_in_every_live_refresh_variant() {
    // Loser settled in Live (1, 3) was already correct; loser's loss first
    // crystallized in Resolved (0, 2) used to leave the provider at 90_000
    // and retire 10_000.
    for live_refresh in 0..4u8 {
        assert_eq!(
            run(true, false, live_refresh),
            expected(),
            "loser-first, live_refresh={live_refresh}"
        );
    }
}

#[test]
fn s2_resolved_close_order_does_not_change_provider_recovery() {
    for loser_first in [false, true] {
        for live_refresh in 0..4u8 {
            assert_eq!(
                run(loser_first, false, live_refresh),
                expected(),
                "loser_first={loser_first}, live_refresh={live_refresh}"
            );
        }
    }
}

#[test]
fn s2_resolved_surplus_withdrawal_midway_keeps_provider_whole() {
    for loser_first in [false, true] {
        assert_eq!(
            run(loser_first, true, 0),
            expected(),
            "loser_first={loser_first}"
        );
    }
}

/// Exactly what the provider's domain receives when the loser's loss books in
/// Resolved mode: the full 10_000 joins its backing (no residual is created),
/// and the winners then draw exactly that from it.
#[test]
fn s2_resolved_loss_credits_provider_domain_exactly() {
    for winners_first in [false, true] {
        let (mut header, mut markets) = one_asset_market();
        let mut wa_h = account_fixture(70);
        let mut wb_h = account_fixture(71);
        let mut l_h = account_fixture(72);
        let mut m = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        let mut wa = PortfolioV16ViewMut::new(&mut wa_h);
        let mut wb = PortfolioV16ViewMut::new(&mut wb_h);
        let mut l = PortfolioV16ViewMut::new(&mut l_h);
        m.deposit_fresh_counterparty_backing_not_atomic(PROVIDER_DOMAIN, PROVIDER, 50)
            .unwrap();
        m.deposit_not_atomic(&mut wa, 1_000_000).unwrap();
        m.deposit_not_atomic(&mut wb, 1_000_000).unwrap();
        m.deposit_not_atomic(&mut l, 2_000_000).unwrap();
        trade(&mut m, &mut wa, &mut l, Q, 100);
        trade(&mut m, &mut wb, &mut l, Q, 100);
        m.set_asset_raw_oracle_target_not_atomic(0, 105).unwrap();
        m.accrue_asset_to_not_atomic(0, 2, 105, 0, true).unwrap();
        m.resolve_market_not_atomic(4).unwrap();
        if winners_first {
            // Winners attribute their claims to the provider domain but cannot
            // realize (nor capture the payout snapshot) before the loser settles.
            for w in [&mut wa, &mut wb] {
                assert_eq!(
                    m.close_resolved_account_not_atomic(w, 0).unwrap(),
                    ResolvedCloseOutcomeV16::ProgressOnly
                );
            }
            assert_eq!(m.header.payout_snapshot_captured, 0);
            assert_eq!(
                source(&m, PROVIDER_DOMAIN).positive_claim_bound_num,
                10_000 * BOUND_SCALE
            );
        }
        let vault = m.header.vault.get();
        let residual_before = vault
            - m.header.c_tot.get()
            - m.header.insurance.get()
            - m.header.source_fresh_backing_total_num.get() / BOUND_SCALE;
        let mut loser_payout = None;
        for _ in 0..16 {
            if let ResolvedCloseOutcomeV16::Closed { payout } =
                m.close_resolved_account_not_atomic(&mut l, 0).unwrap()
            {
                loser_payout = Some(payout);
                break;
            }
        }
        assert_eq!(loser_payout, Some(LOSER_PAYOUT));
        assert_eq!(fresh_atoms(&m, PROVIDER_DOMAIN), PROVIDER + 10_000);
        let residual_after = m.header.vault.get()
            - m.header.c_tot.get()
            - m.header.insurance.get()
            - m.header.source_fresh_backing_total_num.get() / BOUND_SCALE;
        assert_eq!(
            residual_after, residual_before,
            "no junior residual created"
        );
        assert_conserved(&m, true);
    }
}

/// Unchanged: a provider bucket past the resolved backing horizon cannot take
/// fresh backing, so the loss stays junior residual and no backing is created.
#[test]
fn s2_past_horizon_bucket_keeps_loss_in_junior_residual() {
    let (mut header, mut markets) = one_asset_market();
    let mut w_h = account_fixture(80);
    let mut l_h = account_fixture(81);
    let mut m = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut w = PortfolioV16ViewMut::new(&mut w_h);
    let mut l = PortfolioV16ViewMut::new(&mut l_h);
    m.deposit_fresh_counterparty_backing_not_atomic(PROVIDER_DOMAIN, PROVIDER, u64::MAX)
        .unwrap();
    m.deposit_not_atomic(&mut w, 1_000_000).unwrap();
    m.deposit_not_atomic(&mut l, 1_000_000).unwrap();
    trade(&mut m, &mut w, &mut l, Q, 100);
    m.set_asset_raw_oracle_target_not_atomic(0, 105).unwrap();
    m.accrue_asset_to_not_atomic(0, 2, 105, 0, true).unwrap();
    m.resolve_market_not_atomic(4).unwrap();
    // Far past any backing horizon (bounded by resolved_slot + H).
    m.advance_resolved_slot_not_atomic(4 + 1_000).unwrap();
    let mut loser_payout = None;
    for _ in 0..16 {
        if let ResolvedCloseOutcomeV16::Closed { payout } =
            m.close_resolved_account_not_atomic(&mut l, 0).unwrap()
        {
            loser_payout = Some(payout);
            break;
        }
    }
    assert_eq!(loser_payout, Some(995_000));
    assert_ne!(
        bucket(&m, PROVIDER_DOMAIN).status,
        BackingBucketStatusV16::Fresh
    );
    assert_eq!(m.header.source_fresh_backing_total_num.get(), 0);
    assert_conserved(&m, false);
    let mut winner_payout = None;
    for _ in 0..16 {
        if let ResolvedCloseOutcomeV16::Closed { payout } =
            m.close_resolved_account_not_atomic(&mut w, 0).unwrap()
        {
            winner_payout = Some(payout);
            break;
        }
    }
    assert_eq!(winner_payout, Some(1_005_000));
    assert_conserved(&m, false);
}

/// Unchanged: an Impaired provider bucket cannot take fresh backing; a loss
/// booked into its domain stays junior support (no backing created).
#[test]
fn s2_impaired_bucket_keeps_loss_in_junior_residual() {
    const INCREASE_Q: u128 = POS_SCALE;
    let mut cfg = V16Config::public_user_fund_with_market_slots(1, 1, 0, 10);
    cfg.maintenance_margin_bps = 1_000;
    cfg.initial_margin_bps = 5_000;
    cfg.max_price_move_bps_per_slot = 500;
    cfg.max_accrual_dt_slots = 1;
    let mut header = MarketGroupV16HeaderAccount::new_dynamic([1; 32], cfg, 1, 0).unwrap();
    let mut markets = vec![Market::new(0, EngineAssetSlotV16Account::default())];
    header
        .activate_empty_asset_slot_not_atomic(0, &mut markets[0].engine, 100, 1)
        .unwrap();
    let mut winner_h = account_fixture(40);
    let mut peer_h = account_fixture(41);
    let mut trigger_h = account_fixture(42);
    let mut loser_h = account_fixture(43);
    let mut m = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut winner = PortfolioV16ViewMut::new(&mut winner_h);
    let mut peer = PortfolioV16ViewMut::new(&mut peer_h);
    let mut trigger = PortfolioV16ViewMut::new(&mut trigger_h);
    let mut loser = PortfolioV16ViewMut::new(&mut loser_h);
    m.deposit_fresh_counterparty_backing_not_atomic(PROVIDER_DOMAIN, PROVIDER, 3)
        .unwrap();
    m.deposit_not_atomic(&mut winner, 52_501).unwrap();
    m.deposit_not_atomic(&mut peer, 1_000_000).unwrap();
    m.deposit_not_atomic(&mut trigger, 1_000_000).unwrap();
    m.deposit_not_atomic(&mut loser, 1_000_000).unwrap();
    trade(&mut m, &mut winner, &mut peer, Q, 100);
    trade(&mut m, &mut trigger, &mut loser, Q, 100);
    m.set_asset_raw_oracle_target_not_atomic(0, 105).unwrap();
    m.accrue_asset_to_not_atomic(0, 2, 105, 0, true).unwrap();
    for a in [&mut peer, &mut winner, &mut trigger, &mut loser] {
        m.full_account_refresh_not_atomic(a).unwrap();
    }
    // Liens the winner's claim against provider backing, so expiry impairs it.
    trade(&mut m, &mut winner, &mut peer, INCREASE_Q, 105);
    m.set_asset_raw_oracle_target_not_atomic(0, 110).unwrap();
    m.accrue_asset_to_not_atomic(0, 3, 110, 0, true).unwrap();
    m.resolve_market_not_atomic(4).unwrap();
    assert_eq!(
        m.close_resolved_account_not_atomic(&mut trigger, 0)
            .unwrap(),
        ResolvedCloseOutcomeV16::ProgressOnly
    );
    assert_eq!(
        bucket(&m, PROVIDER_DOMAIN).status,
        BackingBucketStatusV16::Impaired
    );
    let fresh_total_before = m.header.source_fresh_backing_total_num.get();
    let bucket_before = bucket(&m, PROVIDER_DOMAIN);
    m.close_resolved_account_not_atomic(&mut loser, 0).unwrap();
    assert_eq!(loser.header.pnl.get(), 0);
    assert_eq!(bucket(&m, PROVIDER_DOMAIN), bucket_before);
    assert_eq!(
        m.header.source_fresh_backing_total_num.get(),
        fresh_total_before
    );
    assert_conserved(&m, false);
}

/// The provider fronted a realized gain in Live (provider receivable), then
/// its bucket lapsed at resolution. A loss first crystallized in Resolved
/// refills exactly that receivable into the domain (as the same loss would in
/// Live) and nothing more: the domain has no claims, so the support gap is 0.
#[test]
fn s2_expired_bucket_loss_refills_provider_receivable_only() {
    const EXPIRY: u64 = 3;
    let (mut header, mut markets) = one_asset_market();
    let mut w_h = account_fixture(90);
    let mut n_h = account_fixture(91);
    let mut l_h = account_fixture(92);
    let mut m = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut w = PortfolioV16ViewMut::new(&mut w_h);
    let mut n = PortfolioV16ViewMut::new(&mut n_h);
    let mut l = PortfolioV16ViewMut::new(&mut l_h);
    m.deposit_fresh_counterparty_backing_not_atomic(PROVIDER_DOMAIN, PROVIDER, EXPIRY)
        .unwrap();
    m.deposit_not_atomic(&mut w, 1_000_000).unwrap();
    m.deposit_not_atomic(&mut n, 1_000_000).unwrap();
    m.deposit_not_atomic(&mut l, 1_000_000).unwrap();
    trade(&mut m, &mut w, &mut l, Q, 100);
    m.set_asset_raw_oracle_target_not_atomic(0, 105).unwrap();
    m.accrue_asset_to_not_atomic(0, 2, 105, 0, true).unwrap();
    m.full_account_refresh_not_atomic(&mut w).unwrap();
    m.full_account_refresh_not_atomic(&mut l).unwrap();
    // W hands its long to N at the mark and realizes its 5_000 gain from the
    // domain's backing: the provider has now advanced 5_000 (receivable).
    trade(&mut m, &mut n, &mut w, Q, 105);
    m.full_account_refresh_not_atomic(&mut w).unwrap();
    assert_eq!(
        m.convert_released_pnl_to_capital_not_atomic(&mut w)
            .unwrap(),
        5_000
    );
    let receivable = source(&m, PROVIDER_DOMAIN).provider_receivable_num;
    assert_eq!(receivable, 5_000 * BOUND_SCALE);
    // A second move is first crystallized in Resolved mode, after the
    // provider's bucket lapsed.
    m.set_asset_raw_oracle_target_not_atomic(0, 110).unwrap();
    m.accrue_asset_to_not_atomic(0, 3, 110, 0, true).unwrap();
    m.resolve_market_not_atomic(EXPIRY + 1).unwrap();
    let mut loser_payout = None;
    for _ in 0..16 {
        if let ResolvedCloseOutcomeV16::Closed { payout } =
            m.close_resolved_account_not_atomic(&mut l, 0).unwrap()
        {
            loser_payout = Some(payout);
            break;
        }
    }
    assert_eq!(loser_payout, Some(990_000));
    let s = source(&m, PROVIDER_DOMAIN);
    assert_eq!(s.positive_claim_bound_num, 0);
    assert_eq!(s.provider_receivable_num, 0, "receivable refilled");
    assert_eq!(
        fresh_atoms(&m, PROVIDER_DOMAIN),
        5_000,
        "exactly the receivable is credited, nothing beyond it"
    );
    assert_conserved(&m, false);
    let mut payouts = Vec::new();
    for acct in [&mut n, &mut w] {
        let mut paid = None;
        for _ in 0..16 {
            if let ResolvedCloseOutcomeV16::Closed { payout } =
                m.close_resolved_account_not_atomic(acct, 0).unwrap()
            {
                paid = Some(payout);
                break;
            }
        }
        payouts.push(paid.expect("close"));
        assert_conserved(&m, false);
    }
    assert_eq!(payouts, vec![1_005_000, 1_005_000]);
}
