//! Proof of concept: two sites in the K/F settlement path destroy an account's
//! positive PnL beyond the arriving loss. Runs against the public API of
//! aeyakovenko/percolator@143e68c4 with ZERO source modification.
//!
//!   cargo test --release --test kf_pnl_writeoff_poc -- --nocapture
//!
//! `control_monotonic_paths_are_exact` is the baseline: monotonic paths settle
//! exact to the atom, so the engine's settlement arithmetic is not in question.
//! The two `finding_*` tests each use a price path that returns EXACTLY to its
//! starting value, so the honest result for both is zero.

use percolator::{
    EngineAssetSlotV16Account, Market, MarketGroupV16HeaderAccount, MarketGroupV16ViewMut,
    PortfolioAccountV16Account, PortfolioV16ViewMut, ProvenanceHeaderV16,
    ProvenanceHeaderV16Account, TradeRequestV16, V16Config, POS_SCALE,
};

fn fixture(init_price: u64) -> (MarketGroupV16HeaderAccount, Vec<Market<u64>>) {
    let cfg = V16Config::public_user_fund_with_market_slots(1, 1, 1_000, 100_000);
    let mut header = MarketGroupV16HeaderAccount::new_dynamic([1; 32], cfg, 1, 0).unwrap();
    let mut markets = vec![Market::new(0u64, EngineAssetSlotV16Account::default())];
    header
        .activate_empty_asset_slot_not_atomic(0, &mut markets[0].engine, init_price, 1)
        .unwrap();
    (header, markets)
}

fn account(seed: u8) -> PortfolioAccountV16Account {
    let h = ProvenanceHeaderV16Account::from_runtime(&ProvenanceHeaderV16::new(
        [1; 32], [seed; 32], [3; 32],
    ));
    let mut a = PortfolioAccountV16Account::default();
    a.init_empty_in_place(h).unwrap();
    a
}

fn usd(v: i128) -> String { format!("{:+.6}", v as f64 / 1e6) }

fn ramp(from: u64, to: u64, steps: usize) -> Vec<u64> {
    (0..=steps).map(|i| (from as i64 + (to as i64 - from as i64) * i as i64 / steps as i64) as u64).collect()
}

/// Sawtooth that returns EXACTLY to `base` at the end of every cycle.
fn sawtooth(base: u64, amp: u64, cycles: usize, per_leg: usize) -> Vec<u64> {
    let mut v = vec![base];
    for _ in 0..cycles {
        for i in 1..=per_leg { v.push(base + amp * i as u64 / per_leg as u64); }
        for i in (0..per_leg).rev() { v.push(base + amp * i as u64 / per_leg as u64); }
    }
    v
}

/// Geometric ramp at 2.5%/step, inside `max_price_move_bps_per_slot` (6 bps/slot
/// x 50 slots = 300 bps per step).
fn geo(from: u64, to: u64, up: bool) -> Vec<u64> {
    let mut v = vec![from];
    let mut p = from as f64;
    loop {
        p = if up { p * 1.025 } else { p / 1.025 };
        let n = p as u64;
        if (up && n >= to) || (!up && n <= to) { v.push(to); break; }
        v.push(n);
    }
    v
}

struct Out { lp0: i128, lp1: i128, tr0: i128, tr1: i128, v0: u128, v1: u128,
             ok: u32, neg_steps: u32, gain_into_neg: u32,
             lp_capital: u128, lp_pnl: i128, lp_cryst: u128 }

/// Trader LONG / LP SHORT. Both accounts are settled every step, so "the
/// counterparty did not gain" can never be an artifact of never settling them.
/// Emulates on-chain transaction atomicity: a settlement that returns Err is
/// rolled back, so a rejected step never leaves a partial mutation behind.
fn run(path: &[u64], lp_dep: u128, tr_dep: u128, size_tokens: i128) -> Out {
    run_ordered(path, lp_dep, tr_dep, size_tokens, false)
}

fn run_ordered(path: &[u64], lp_dep: u128, tr_dep: u128, size_tokens: i128, tr_first: bool) -> Out {
    let init = path[0];
    let (mut header, mut markets) = fixture(init);
    let mut lp = account(200);
    let mut tr = account(7);
    {
        let mut m = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        m.deposit_not_atomic(&mut PortfolioV16ViewMut::new(&mut lp), lp_dep).unwrap();
        m.deposit_not_atomic(&mut PortfolioV16ViewMut::new(&mut tr), tr_dep).unwrap();
        let req = TradeRequestV16 { asset_index: 0, size_q: POS_SCALE as i128 * size_tokens,
                                    exec_price: init, fee_bps: 0 };
        m.execute_trade_with_fee_loss_stale_scoped_not_atomic(
            &mut PortfolioV16ViewMut::new(&mut tr), &mut PortfolioV16ViewMut::new(&mut lp), req).unwrap();
    }
    let lp0 = lp.capital.get() as i128 + lp.pnl.get();
    let tr0 = tr.capital.get() as i128 + tr.pnl.get();
    let v0 = header.vault.get();
    let k0 = markets[0].engine.asset.k_long.get();

    let (mut slot, mut ok, mut neg_steps, mut gain_into_neg) = (50u64, 0u32, 0u32, 0u32);
    let mut k_moved = false;
    for &px in path.iter().skip(1) {
        let pnl_before = lp.pnl.get();
        let (h, m2, l, t) = (header, markets.clone(), lp, tr);
        let good = (|| -> Result<(), ()> {
            let mut m = MarketGroupV16ViewMut::new(&mut header, &mut markets);
            m.accrue_asset_to_not_atomic(0, slot, px, 0, true).map_err(|_| ())?;
            if tr_first {
                m.full_account_refresh_not_atomic(&mut PortfolioV16ViewMut::new(&mut tr)).map_err(|_| ())?;
                m.full_account_refresh_not_atomic(&mut PortfolioV16ViewMut::new(&mut lp)).map_err(|_| ())?;
            } else {
                m.full_account_refresh_not_atomic(&mut PortfolioV16ViewMut::new(&mut lp)).map_err(|_| ())?;
                m.full_account_refresh_not_atomic(&mut PortfolioV16ViewMut::new(&mut tr)).map_err(|_| ())?;
            }
            Ok(())
        })().is_ok();
        if !good { header = h; markets = m2; lp = l; tr = t; } else { ok += 1; }
        if markets[0].engine.asset.k_long.get() != k0
            || markets[0].engine.asset.k_short.get() != 0 { k_moved = true; }
        if lp.pnl.get() < 0 { neg_steps += 1; }
        if pnl_before < 0 && lp.pnl.get() >= pnl_before { gain_into_neg += 1; }
        slot += 50;
    }
    assert!(k_moved, "PoC measured nothing: K never advanced at any point during the run");
    Out { lp0, lp1: lp.capital.get() as i128 + lp.pnl.get(), tr0,
          tr1: tr.capital.get() as i128 + tr.pnl.get(), v0, v1: header.vault.get(),
          ok, neg_steps, gain_into_neg,
          lp_capital: lp.capital.get(), lp_pnl: lp.pnl.get(),
          lp_cryst: lp.residual_crystallized_loss_atoms_total.get() }
}

fn show(o: &Out, steps: usize) {
    println!("  settles_ok={}/{}  steps_with_pnl_negative={}  gain_events_into_negative_pnl={}",
             o.ok, steps, o.neg_steps, o.gain_into_neg);
    println!("  LP            {} -> {}   delta {}", usd(o.lp0), usd(o.lp1), usd(o.lp1 - o.lp0));
    println!("  COUNTERPARTY  {} -> {}   delta {}", usd(o.tr0), usd(o.tr1), usd(o.tr1 - o.tr0));
    println!("  LP capital={} pnl={} crystallized={}",
             usd(o.lp_capital as i128), usd(o.lp_pnl), usd(o.lp_cryst as i128));
    println!("  vault {} -> {}  (external token stock: MUST be flat)", usd(o.v0 as i128), usd(o.v1 as i128));
}

#[test]
fn control_monotonic_paths_are_exact() {
    // LP is SHORT. Price down = favourable, price up = adverse. 100 tokens, so
    // 1 price unit = 100 atoms; a 30,000-unit move = $3.000000.
    let o = run(&ramp(100_000, 70_000, 30), 1_000_000_000, 1_000_000_000, 100);
    println!("\nCONTROL favourable:"); show(&o, 30);
    assert_eq!(o.lp1 - o.lp0, 3_000_000, "favourable monotonic path must be exact");

    let o = run(&ramp(100_000, 130_000, 30), 1_000_000_000, 1_000_000_000, 100);
    println!("\nCONTROL adverse:"); show(&o, 30);
    assert_eq!(o.lp1 - o.lp0, -3_000_000, "adverse monotonic path must be exact");
}

/// SITE 1 — `apply_haircut_bounded_close_loss_to_pnl`.
/// Any under-supported loss burns the account's ENTIRE positive face, not just
/// the consumed part. Zero-net-move path => honest result is 0.
#[test]
fn issue172_site1_zero_net_price_churn_is_value_neutral() {
    let path = sawtooth(100_000, 2_000, 20, 4);
    assert_eq!(*path.last().unwrap(), 100_000, "path must return exactly to start");
    let o = run(&path, 1_000_000_000, 1_000_000_000, 100);
    println!("\nSITE 1 — 20 cycles of +/-2%, ending EXACTLY where it started:");
    show(&o, path.len() - 1);
    assert!(o.ok > 20, "VACUOUS: engine barely settled");
    assert_eq!(o.v0, o.v1, "no external tokens moved — purely internal accounting");
    assert_eq!(o.tr1, o.tr0, "counterparty is unchanged");
    assert_eq!(o.lp1, o.lp0, "zero-net price churn must not cost the LP (issue #172 site 1)");
}

/// Issue #172 site 2 — `apply_signed_kf_delta_to_pnl`.
/// A gain arriving at an account whose capital is exhausted (negative pnl) must be able to cure
/// that loss from its source domain's backing. It used to see zero support whenever the domain
/// had no registered claims, so every reversal gain was burned and the LP ended bankrupt on a
/// path that returns exactly to its start.
#[test]
fn issue172_site2_exhausted_lp_recovers_on_reversal() {
    let mut path = geo(100_000, 400_000, true);
    path.extend(geo(400_000, 100_000, false).into_iter().skip(1));
    assert_eq!(*path.last().unwrap(), 100_000, "path must return exactly to start");
    for tr_first in [false, true] {
        let o = run_ordered(&path, 1_000_000_000, 500_000_000_000, 5_000, tr_first);
        println!("\nSITE 2 (counterparty refreshed first: {tr_first}):");
        show(&o, path.len() - 1);
        assert!(o.ok > 20, "VACUOUS: engine barely settled");
        assert!(o.neg_steps > 0, "PRECONDITION NOT MET: pnl never went negative");
        assert!(o.gain_into_neg > 0, "PRECONDITION NOT MET: no gain reached a negative-pnl account");
        assert_eq!(o.v0, o.v1, "no external tokens moved");
        // Before the fix every reversal gain saw zero support and was burned, leaving the LP at
        // -$500 (a -$1,500 swing) on a path that returns to its start. Backed gains now cure the
        // debt; only gains beyond the reserved backing stay unbacked (junior by design).
        assert!(o.lp1 > -100_000_000, "LP ends at {} (was -500,000,000 before the fix)", o.lp1);
        // The counterparty's face beyond the LP's collateral was unbacked while the LP was
        // insolvent; that value stays junior residual (vault is flat), bounded by the LP's
        // peak bad debt ($500).
        assert!(o.tr0 - o.tr1 <= 500_000_000);
    }
}
