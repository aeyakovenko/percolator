//! Same-leg reversal netting must not move provider-fronted backing (S1).
//!
//! When a long's mark gain reverses before its short counterparty is refreshed, the #457/#226
//! same-leg netting cancels the long's face in the twin (short-loser) domain. Only backing the
//! short losers actually paid into that domain may follow the cancelled face to the long-loser
//! domain (it backs those shorts' regain there). Backing a provider fronted for the face is
//! still the provider's: moving it lands it where no claim ever arrives, and it was retired at
//! terminal close (the provider lost face x rate per round trip).
#![allow(dead_code)]

use percolator::{
    AutoCrankObservationV16, AutoCrankPlanV16, AutoCrankWorkV16, BackingBucketStatusV16,
    EngineAssetSlotV16Account, Market, MarketGroupV16HeaderAccount, MarketGroupV16ViewMut,
    PortfolioAccountV16Account, PortfolioV16ViewMut, ProvenanceHeaderV16,
    ProvenanceHeaderV16Account, ResolvedCloseOutcomeV16, TerminalSlabOutcomeV16, TradeRequestV16,
    V16Config, BOUND_SCALE, POS_SCALE,
};

const Q: i128 = (1_000 * POS_SCALE) as i128;
const CAPITAL: u128 = 1_000_000;
const EXPIRY: u64 = 1_000;

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
    a: &mut PortfolioV16ViewMut<'_>,
    b: &mut PortfolioV16ViewMut<'_>,
    q: i128,
    px: u64,
) {
    m.execute_trade_with_fee_loss_stale_scoped_not_atomic(
        a,
        b,
        TradeRequestV16 {
            asset_index: 0,
            size_q: q,
            exec_price: px,
            fee_bps: 0,
        },
    )
    .unwrap();
}

fn move_price(m: &mut MarketGroupV16ViewMut<'_, u64>, slot: u64, px: u64) {
    m.set_asset_raw_oracle_target_not_atomic(0, px).unwrap();
    m.accrue_asset_to_not_atomic(0, slot, px, 0, true).unwrap();
}

fn crank(
    m: &mut MarketGroupV16ViewMut<'_, u64>,
    x: &mut PortfolioV16ViewMut<'_>,
    slot: u64,
    px: u64,
) {
    for _ in 0..32 {
        let res = m
            .permissionless_auto_crank_not_atomic(
                x,
                AutoCrankWorkV16 {
                    now_slot: slot,
                    observations: &[AutoCrankObservationV16 {
                        asset_index: 0,
                        effective_price: px,
                        funding_rate_e9: 0,
                    }],
                    resolved_close_fee_rate_per_slot: 0,
                },
            )
            .unwrap();
        if res.selected == AutoCrankPlanV16::NoAction {
            return;
        }
    }
    panic!("crank reached no fixpoint");
}

fn close_until_done(
    m: &mut MarketGroupV16ViewMut<'_, u64>,
    a: &mut PortfolioV16ViewMut<'_>,
) -> u128 {
    for _ in 0..64 {
        match m.close_resolved_account_not_atomic(a, 0).unwrap() {
            ResolvedCloseOutcomeV16::ProgressOnly => {}
            ResolvedCloseOutcomeV16::Closed { payout } => return payout,
        }
    }
    panic!("account did not close");
}

/// (claims, fresh reserved backing) of a domain, in atoms (floored).
fn domain_state(m: &MarketGroupV16ViewMut<'_, u64>, domain: usize) -> (u128, u128) {
    let e = &m.markets[0].engine;
    let sc = if domain == 0 {
        e.source_credit_long.try_to_runtime().unwrap()
    } else {
        e.source_credit_short.try_to_runtime().unwrap()
    };
    (
        sc.positive_claim_bound_num / BOUND_SCALE,
        sc.fresh_reserved_backing_num / BOUND_SCALE,
    )
}

fn bucket_fresh_atoms(m: &MarketGroupV16ViewMut<'_, u64>, domain: usize) -> u128 {
    let e = &m.markets[0].engine;
    let b = if domain == 0 {
        e.backing_long.try_to_runtime().unwrap()
    } else {
        e.backing_short.try_to_runtime().unwrap()
    };
    if b.status == BackingBucketStatusV16::Fresh {
        b.fresh_unliened_backing_num / BOUND_SCALE
    } else {
        0
    }
}

fn assert_domains_backed(m: &MarketGroupV16ViewMut<'_, u64>, tag: &str) {
    for d in 0..2 {
        let (claims, backing) = domain_state(m, d);
        assert!(
            backing >= claims,
            "{tag}: domain {d} holds {backing} of backing for {claims} of claims"
        );
    }
    let h = &m.header;
    assert!(
        h.vault.get() >= h.c_tot.get() + h.insurance.get(),
        "{tag}: vault below c_tot + insurance"
    );
}

#[derive(Debug, PartialEq, Eq)]
struct Outcome {
    /// What the dom-1 provider withdrew.
    provider: u128,
    /// Unclaimed fresh backing left in the long-loser domain after everyone is flat.
    orphan_dom0: u128,
    /// Resolved close payouts: the long, then each short.
    payouts: Vec<u128>,
    retired: u128,
    insurance: u128,
    vault_end: u128,
}

/// The long (A) is long `shorts.len()` x 1,000 against shorts B_i (1,000 each). A provider
/// backs the short-loser domain (asset 0 Short = domain 1). Price 100 -> 105 (A refreshed, and
/// each B_i with `peak_refresh[i]`) -> `end`; everyone is refreshed, flattened at `end` and the
/// market is wound down.
fn round_trip(provider_dep: u128, peak_refresh: &[bool], end: u64) -> Outcome {
    let n = peak_refresh.len();
    let (mut header, mut markets) = one_asset_market();
    let mut ah = account_fixture(80);
    let mut bh: Vec<_> = (0..n).map(|i| account_fixture(81 + i as u8)).collect();
    let mut m = MarketGroupV16ViewMut::new(&mut header, &mut markets);
    let mut a = PortfolioV16ViewMut::new(&mut ah);
    let mut bs: Vec<_> = bh.iter_mut().map(PortfolioV16ViewMut::new).collect();
    if provider_dep > 0 {
        m.deposit_fresh_counterparty_backing_not_atomic(1, provider_dep, EXPIRY)
            .unwrap();
    }
    m.deposit_not_atomic(&mut a, CAPITAL).unwrap();
    for b in bs.iter_mut() {
        m.deposit_not_atomic(b, CAPITAL).unwrap();
    }
    let total_in = provider_dep + CAPITAL * (n as u128 + 1);
    for b in bs.iter_mut() {
        trade(&mut m, &mut a, b, Q, 100);
    }
    move_price(&mut m, 2, 105);
    crank(&mut m, &mut a, 2, 105);
    for (b, &r) in bs.iter_mut().zip(peak_refresh) {
        if r {
            crank(&mut m, b, 2, 105);
        }
    }
    move_price(&mut m, 3, end);
    crank(&mut m, &mut a, 3, end);
    for b in bs.iter_mut() {
        crank(&mut m, b, 3, end);
    }
    assert_domains_backed(&m, "everyone refreshed");
    for b in bs.iter_mut() {
        trade(&mut m, b, &mut a, Q, end);
    }
    crank(&mut m, &mut a, 3, end);
    for b in bs.iter_mut() {
        crank(&mut m, b, 3, end);
    }
    assert_domains_backed(&m, "flat");
    // The provider withdraws everything not backing a live claim, then the rest after the
    // claimants are paid.
    let mut provider = 0;
    let free = bucket_fresh_atoms(&m, 1).saturating_sub(domain_state(&m, 1).0);
    if free > 0 {
        m.withdraw_fresh_counterparty_backing_not_atomic(1, free)
            .unwrap();
        provider += free;
    }
    let orphan_dom0 = bucket_fresh_atoms(&m, 0).saturating_sub(domain_state(&m, 0).0);
    m.resolve_market_not_atomic(4).unwrap();
    let mut payouts = vec![close_until_done(&mut m, &mut a)];
    for b in bs.iter_mut() {
        payouts.push(close_until_done(&mut m, b));
    }
    let surplus = m.resolved_counterparty_backing_surplus(1).unwrap_or(0);
    if surplus > 0 {
        m.withdraw_resolved_counterparty_backing_surplus_not_atomic(1, surplus)
            .unwrap();
        provider += surplus;
    }
    let rest = bucket_fresh_atoms(&m, 1);
    if rest > 0 {
        m.withdraw_fresh_counterparty_backing_not_atomic(1, rest)
            .unwrap();
        provider += rest;
    }
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
    let out = Outcome {
        provider,
        orphan_dom0,
        payouts,
        retired,
        insurance: m.header.insurance.get(),
        vault_end: m.header.vault.get(),
    };
    // Token conservation: every atom deposited is paid out, retired, or still in the vault.
    assert_eq!(
        out.provider + out.payouts.iter().sum::<u128>() + out.retired + out.insurance,
        total_in,
        "{out:?}"
    );
    out
}

/// S1 at twin rate 1: the short is never refreshed at the peak, so the long's face was fronted
/// entirely by the provider. The provider must get everything back; nothing is retired.
#[test]
fn twin_netting_does_not_move_provider_fronted_backing_at_rate_one() {
    let out = round_trip(100_000, &[false], 100);
    assert_eq!(out.provider, 100_000, "{out:?}");
    assert_eq!(out.payouts, vec![CAPITAL, CAPITAL], "{out:?}");
    assert_eq!((out.orphan_dom0, out.retired), (0, 0), "{out:?}");
}

/// S1 at twin rate 0.6 (provider 3,000 behind a 5,000 face).
#[test]
fn twin_netting_does_not_move_provider_fronted_backing_at_a_partial_rate() {
    let out = round_trip(3_000, &[false], 100);
    assert_eq!(out.provider, 3_000, "{out:?}");
    assert_eq!(out.payouts, vec![CAPITAL, CAPITAL], "{out:?}");
    assert_eq!((out.orphan_dom0, out.retired), (0, 0), "{out:?}");
}

/// Control: the short paid its peak loss into the twin domain, so that backing follows the
/// cancelled face and pays the short's regain; the provider is untouched.
#[test]
fn twin_netting_moves_loser_paid_backing_for_the_regain() {
    for provider in [100_000, 3_000, 0] {
        let out = round_trip(provider, &[true], 100);
        assert_eq!(out.provider, provider, "{out:?}");
        assert_eq!(out.payouts, vec![CAPITAL, CAPITAL], "{out:?}");
        assert_eq!((out.orphan_dom0, out.retired), (0, 0), "{out:?}");
    }
}

/// Mixed: the twin domain holds both provider principal and one short's paid peak loss. Only
/// that short's payment moves (it backs exactly its regain); the provider's stays.
#[test]
fn twin_netting_with_provider_and_loser_backing_moves_only_the_loser_part() {
    for provider in [100_000, 7_000, 3_000] {
        for peak in [[true, false], [false, true]] {
            let out = round_trip(provider, &peak, 100);
            assert_eq!(out.provider, provider, "peak {peak:?}: {out:?}");
            assert_eq!(out.payouts, vec![CAPITAL; 3], "peak {peak:?}: {out:?}");
            assert_eq!(
                (out.orphan_dom0, out.retired),
                (0, 0),
                "peak {peak:?}: {out:?}"
            );
        }
    }
}

/// A partial reversal (105 -> 102) with one of two shorts paid at the peak: the long keeps a
/// 4,000 gain, the shorts lose 2,000 each, the provider ends whole and nothing is retired.
#[test]
fn twin_netting_partial_reversal_keeps_provider_whole() {
    for provider in [100_000, 3_000] {
        for peak in [[true, false], [false, true], [true, true], [false, false]] {
            let out = round_trip(provider, &peak, 102);
            assert_eq!(out.provider, provider, "peak {peak:?}: {out:?}");
            assert_eq!(
                out.payouts,
                vec![CAPITAL + 4_000, CAPITAL - 2_000, CAPITAL - 2_000],
                "peak {peak:?}: {out:?}"
            );
            assert_eq!(out.retired, 0, "peak {peak:?}: {out:?}");
        }
    }
}
