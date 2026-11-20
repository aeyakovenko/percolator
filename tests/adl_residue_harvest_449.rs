//! Issue percolator-prog#449, adversarial follow-up: can an attacker who owns
//! every account turn the ADL-effective ceiling residue into meaningful value?
//!
//! Notation (per side, current-epoch legs only, every leg opened at a_basis = ADL_ONE):
//!   G_num = sum(raw * A) - OI * ADL_ONE   (exact exposure over OI, scaled by ADL_ONE)
//!   D     = sum(ceil(raw * A / ADL_ONE)) - OI
//! G is the phantom exposure: a +dP move pays the side G * dP / POS_SCALE more than the
//! other side loses.
//!
//! These tests were written against the engine before the fix, where every same-side reducing
//! op raised that side's G by up to one raw unit and G was bounded only by the number of such
//! ops. They keep the same adversaries and now pin the fix:
//! * A side keeps the sum of its legs' raw basis and derives A from it after every operation,
//!   and a reduction gives up at least the raw basis that carries the effective OI it removes.
//!   G is therefore never positive while the side has effective OI, and is short of zero by
//!   less than `raw_basis_sum / ADL_ONE` raw units, whatever the route (bilateral trade,
//!   RebalanceReduce, full clear), size, leg count, A, or number of operations.
//! * D is never negative and never exceeds the number of legs.
//! * The greedy adversary that picks the ADL shaping size and the harvest op to maximise the
//!   next step's growth ends with no phantom exposure at all.
//! * Realisation: a 10% move in either direction gives the adversary nothing, with or without
//!   another winner in the same source domain.
//! * A haircut side stays closed to new risk until its effective OI again equals its raw
//!   basis (A back at ADL_ONE) or it resets.

use percolator::{
    AssetStateV16, EngineAssetSlotV16Account, Market, MarketGroupV16HeaderAccount,
    MarketGroupV16ViewMut, PortfolioAccountV16Account, PortfolioV16ViewMut, ProvenanceHeaderV16,
    ProvenanceHeaderV16Account, RebalanceRequestV16, SideV16, TradeRequestV16, V16Config,
    V16PodU64, ADL_ONE, MIN_A_SIDE, POS_SCALE,
};

const P0: u64 = 1_000_000;
const DEPOSIT: u128 = 400_000_000_000_000;

#[derive(Clone)]
struct World {
    header: MarketGroupV16HeaderAccount,
    markets: Vec<Market<u64>>,
    /// longs[0..nl] then shorts[nl..nl+ns]
    accts: Vec<PortfolioAccountV16Account>,
    nl: usize,
    price: u64,
    slot: u64,
}

#[derive(Clone, Copy, Debug)]
enum Op {
    /// short j buys q from long i: reduces one long leg and one short leg
    Trade { i: usize, j: usize, q: u128 },
    /// unilateral long reduce (ADLs the short side)
    RebalLong { i: usize, q: u128 },
    /// unilateral short reduce (ADLs the long side)
    RebalShort { j: usize, q: u128 },
}

fn account(seed: u8) -> PortfolioAccountV16Account {
    let header = ProvenanceHeaderV16Account::from_runtime(&ProvenanceHeaderV16::new(
        [1; 32], [seed; 32], [3; 32],
    ));
    let mut account = PortfolioAccountV16Account::default();
    account.init_empty_in_place(header).unwrap();
    account
}

impl World {
    fn new(nl: usize, ns: usize) -> Self {
        let mut cfg = V16Config::public_user_fund_with_market_slots(1, 1, 0, 10);
        cfg.max_price_move_bps_per_slot = 9_000;
        let mut header = MarketGroupV16HeaderAccount::new_dynamic([1; 32], cfg, 1, 0).unwrap();
        let mut markets = vec![Market::new(0, EngineAssetSlotV16Account::default())];
        header
            .activate_empty_asset_slot_not_atomic(0, &mut markets[0].engine, P0, 1)
            .unwrap();
        let mut w = World {
            header,
            markets,
            accts: (0..nl + ns).map(|i| account(40 + i as u8)).collect(),
            nl,
            price: P0,
            slot: 1,
        };
        for k in 0..nl + ns {
            w.deposit(k, DEPOSIT);
        }
        w
    }

    fn deposit(&mut self, k: usize, amount: u128) {
        let mut m = MarketGroupV16ViewMut::new(&mut self.header, &mut self.markets);
        let mut a = PortfolioV16ViewMut::new(&mut self.accts[k]);
        m.deposit_not_atomic(&mut a, amount).unwrap();
    }

    fn asset(&self) -> AssetStateV16 {
        self.markets[0].engine.asset.try_to_runtime().unwrap()
    }

    fn trade_raw(
        &mut self,
        buyer: usize,
        seller: usize,
        q: u128,
    ) -> Result<(), percolator::V16Error> {
        assert_ne!(buyer, seller);
        let price = self.price;
        let World {
            header,
            markets,
            accts,
            ..
        } = self;
        let (b, s) = if buyer < seller {
            let (x, y) = accts.split_at_mut(seller);
            (&mut x[buyer], &mut y[0])
        } else {
            let (x, y) = accts.split_at_mut(buyer);
            (&mut y[0], &mut x[seller])
        };
        let mut m = MarketGroupV16ViewMut::new(header, markets);
        let mut b = PortfolioV16ViewMut::new(b);
        let mut s = PortfolioV16ViewMut::new(s);
        m.execute_trade_with_fee_loss_stale_scoped_not_atomic(
            &mut b,
            &mut s,
            TradeRequestV16 {
                asset_index: 0,
                size_q: i128::try_from(q).unwrap(),
                exec_price: price,
                fee_bps: 0,
            },
        )
        .map(|_| ())
    }

    fn rebalance(&mut self, k: usize, q: u128) -> Result<u128, percolator::V16Error> {
        let mut m = MarketGroupV16ViewMut::new(&mut self.header, &mut self.markets);
        let mut a = PortfolioV16ViewMut::new(&mut self.accts[k]);
        m.rebalance_reduce_position_not_atomic(
            &mut a,
            RebalanceRequestV16 {
                asset_index: 0,
                reduce_q: q,
            },
        )
        .map(|o| o.reduced_q)
    }

    fn apply(&mut self, op: Op) -> Result<(), percolator::V16Error> {
        let nl = self.nl;
        match op {
            Op::Trade { i, j, q } => self.trade_raw(nl + j, i, q),
            Op::RebalLong { i, q } => self.rebalance(i, q).map(|_| ()),
            Op::RebalShort { j, q } => self.rebalance(nl + j, q).map(|_| ()),
        }
    }

    fn leg(&self, k: usize) -> Option<(SideV16, u128, u128, u64)> {
        let leg = self.accts[k].legs[0].try_to_runtime().unwrap();
        leg.active.then(|| {
            (
                leg.side,
                leg.basis_pos_q.unsigned_abs(),
                leg.a_basis,
                leg.epoch_snap,
            )
        })
    }

    fn eff(&self, k: usize) -> u128 {
        let a = self.asset();
        match self.leg(k) {
            Some((side, raw, ab, ep)) => {
                let (cur, cur_ep) = match side {
                    SideV16::Long => (a.a_long, a.epoch_long),
                    SideV16::Short => (a.a_short, a.epoch_short),
                };
                if ep != cur_ep {
                    0
                } else {
                    (raw * cur + ab - 1) / ab
                }
            }
            None => 0,
        }
    }

    /// Per side [long, short]: (G_num, D, epoch)
    fn census(&self) -> [(i128, i128, u64); 2] {
        let a = self.asset();
        let mut exact = [0u128; 2];
        let mut ceil = [0u128; 2];
        for k in 0..self.accts.len() {
            if let Some((side, raw, ab, ep)) = self.leg(k) {
                let (cur, cur_ep, s) = match side {
                    SideV16::Long => (a.a_long, a.epoch_long, 0),
                    SideV16::Short => (a.a_short, a.epoch_short, 1),
                };
                if ep != cur_ep {
                    continue;
                }
                assert_eq!(ab, ADL_ONE, "no risk increase while A != ADL_ONE");
                exact[s] += raw * cur;
                ceil[s] += (raw * cur + ab - 1) / ab;
            }
        }
        let oi = [a.oi_eff_long_q, a.oi_eff_short_q];
        let ep = [a.epoch_long, a.epoch_short];
        [0, 1].map(|s| {
            (
                exact[s] as i128 - (oi[s] * ADL_ONE) as i128,
                ceil[s] as i128 - oi[s] as i128,
                ep[s],
            )
        })
    }

    fn set_price(&mut self, price: u64) {
        self.slot += 1;
        let slot = self.slot;
        let mut m = MarketGroupV16ViewMut::new(&mut self.header, &mut self.markets);
        m.accrue_asset_to_not_atomic(0, slot, price, 0, true)
            .unwrap();
        m.markets[0].engine.asset.raw_oracle_target_price = V16PodU64::new(price);
        self.price = price;
    }

    /// Engine-certified (haircut) equity of account k after a full refresh.
    fn certified_equity(&mut self, k: usize) -> i128 {
        let mut m = MarketGroupV16ViewMut::new(&mut self.header, &mut self.markets);
        let mut v = PortfolioV16ViewMut::new(&mut self.accts[k]);
        m.full_account_refresh_not_atomic(&mut v)
            .unwrap()
            .certified_equity
    }

    fn face_value(&mut self, k: usize) -> i128 {
        let mut m = MarketGroupV16ViewMut::new(&mut self.header, &mut self.markets);
        let mut v = PortfolioV16ViewMut::new(&mut self.accts[k]);
        m.full_account_refresh_not_atomic(&mut v).unwrap();
        v.header.capital.get() as i128 + v.header.pnl.get()
    }
}

/// Per-op accounting and the hard bounds. Returns per-side growth of this op (G_num deltas).
#[derive(Default, Debug, Clone, Copy)]
struct Tally {
    ops: u64,
    side_ops: [u128; 2],
    epoch: [u64; 2],
    max_growth_num: [i128; 2],
    max_two_sided_num: i128,
    max_g_num: [i128; 2],
    max_d: [i128; 2],
}

fn reduced_sides(op: Op) -> [bool; 2] {
    match op {
        Op::Trade { .. } => [true, true],
        Op::RebalLong { .. } => [true, false],
        Op::RebalShort { .. } => [false, true],
    }
}

impl Tally {
    fn new(w: &World) -> Self {
        let c = w.census();
        Tally {
            epoch: [c[0].2, c[1].2],
            ..Default::default()
        }
    }

    fn record(
        &mut self,
        before: [(i128, i128, u64); 2],
        after: [(i128, i128, u64); 2],
        op: Op,
        nlegs: [usize; 2],
    ) {
        self.ops += 1;
        let reduced = reduced_sides(op);
        let mut two = 0;
        for s in 0..2 {
            if after[s].2 != self.epoch[s] {
                // side reset: residue cleared, start a new budget
                self.epoch[s] = after[s].2;
                self.side_ops[s] = 0;
                assert_eq!((after[s].0, after[s].1), (0, 0), "reset clears residue");
                continue;
            }
            let growth = after[s].0 - before[s].0;
            if reduced[s] {
                self.side_ops[s] += 1;
                // HARD BOUND 1: one same-side reducing op adds < 1 raw unit of exact exposure
                assert!(
                    growth < ADL_ONE as i128,
                    "{op:?} side {s} grew {growth} >= 1 raw unit"
                );
                two += growth.max(0);
            } else {
                // passive (ADL'd) side: re-scaling never adds phantom exposure
                assert!(
                    after[s].0 <= before[s].0.max(0),
                    "{op:?} ADL re-scale grew passive side {s}: {} -> {}",
                    before[s].0,
                    after[s].0
                );
            }
            self.max_growth_num[s] = self.max_growth_num[s].max(growth);
            // HARD BOUND 2: G < (same-side reducing ops since the side's last reset) raw units
            assert!(
                after[s].0 < (self.side_ops[s].max(1) * ADL_ONE) as i128,
                "side {s}: G_num {} >= ops {}",
                after[s].0,
                self.side_ops[s]
            );
            // HARD BOUND 3: ceilings exceed exact by < 1 per leg
            assert!(
                (after[s].1 as i128) * (ADL_ONE as i128) - after[s].0
                    < (nlegs[s] as i128 + 1) * ADL_ONE as i128
            );
            self.max_g_num[s] = self.max_g_num[s].max(after[s].0);
            self.max_d[s] = self.max_d[s].max(after[s].1);
        }
        // one op touches at most one leg per side => two-sided growth < 2
        assert!(two < 2 * ADL_ONE as i128);
        self.max_two_sided_num = self.max_two_sided_num.max(two);
    }
}

/// The bounds of the fix, checked after every successful operation.
fn assert_side_totals_are_exact(w: &World) {
    let a = w.asset();
    let c = w.census();
    let mut legs = [0i128; 2];
    let mut raw = [0u128; 2];
    for k in 0..w.accts.len() {
        if let Some((side, r, _, ep)) = w.leg(k) {
            let (cur_ep, s) = match side {
                SideV16::Long => (a.epoch_long, 0),
                SideV16::Short => (a.epoch_short, 1),
            };
            if ep == cur_ep && r != 0 {
                legs[s] += 1;
                raw[s] += r;
            }
        }
    }
    let totals = [
        (a.oi_eff_long_q, a.raw_basis_sum_long_q, a.a_long),
        (a.oi_eff_short_q, a.raw_basis_sum_short_q, a.a_short),
    ];
    for s in 0..2 {
        let (oi, raw_sum, a_side) = totals[s];
        let (g_num, d, _) = c[s];
        assert_eq!(raw_sum, raw[s], "side {s}: raw basis sum");
        if raw_sum == 0 {
            assert_eq!(
                (g_num, d),
                (0, -(oi as i128)),
                "side {s}: no current-epoch basis"
            );
            continue;
        }
        assert!(oi <= raw_sum, "side {s}: OI {oi} above raw basis {raw_sum}");
        assert_eq!(
            a_side,
            (ADL_ONE * oi / raw_sum).max(1),
            "side {s}: A is derived from OI {oi} and raw basis {raw_sum}"
        );
        if oi == 0 {
            // exhausted: the smallest index, under a tenth of a raw unit of exposure
            assert_eq!(g_num, raw_sum as i128, "side {s}: exhausted");
            assert_eq!(d, legs[s], "side {s}: exhausted");
            continue;
        }
        // exact exposure: never above the OI, short of it by less than raw_sum / ADL_ONE
        assert!(
            g_num <= 0 && g_num > -(raw_sum as i128),
            "side {s}: G_num {g_num} outside (-{raw_sum}, 0]"
        );
        // ceilings: cover the OI, exceed it by less than one per leg
        assert!(
            d >= 0 && d < legs[s],
            "side {s}: D {d} with {} legs",
            legs[s]
        );
    }
}

/// No certificate that still counts as current may be healthier than the account's state.
///
/// A haircut only shrinks effective quantities, so certificates issued before it stay
/// conservative. A reduction on a haircut side can raise the side's index and with it another
/// account's effective quantity; the engine then advances the risk epoch, and the certificates
/// issued before are no longer current. Checked for every account after every operation: a
/// certificate whose epochs and bitmap still match is compared with a full refresh of a copy.
fn assert_current_certificates_are_conservative(w: &World) {
    for k in 0..w.accts.len() {
        let cached = w.accts[k].health_cert.try_to_runtime().unwrap();
        let current = cached.valid
            && cached.cert_oracle_epoch == w.header.oracle_epoch.get()
            && cached.cert_funding_epoch == w.header.funding_epoch.get()
            && cached.cert_risk_epoch == w.header.risk_epoch.get()
            && cached.cert_asset_set_epoch == w.header.asset_set_epoch.get()
            && cached.active_bitmap_at_cert == w.accts[k].active_bitmap.map(|word| word.get());
        if !current {
            continue;
        }
        let mut copy = w.clone();
        let mut m = MarketGroupV16ViewMut::new(&mut copy.header, &mut copy.markets);
        let mut v = PortfolioV16ViewMut::new(&mut copy.accts[k]);
        let Ok(full) = m.full_account_refresh_not_atomic(&mut v) else {
            continue;
        };
        assert!(
            cached.certified_equity <= full.certified_equity
                && cached.certified_initial_req >= full.certified_initial_req
                && cached.certified_maintenance_req >= full.certified_maintenance_req
                && cached.certified_worst_case_loss >= full.certified_worst_case_loss
                && cached.certified_liq_deficit >= full.certified_liq_deficit,
            "account {k}: a current certificate is healthier than a full refresh: \
             cached {cached:?}, full {full:?}"
        );
    }
}

fn run(w: &mut World, t: &mut Tally, op: Op) -> bool {
    let before = w.census();
    let snapshot = w.clone();
    match w.apply(op) {
        Ok(()) => {
            let after = w.census();
            let ns = w.accts.len() - w.nl;
            t.record(before, after, op, [w.nl, ns]);
            assert_side_totals_are_exact(w);
            assert_current_certificates_are_conservative(w);
            true
        }
        Err(_) => {
            *w = snapshot;
            false
        }
    }
}

/// Open `nl` longs against `ns` shorts. Long i holds base + i * 7_777. Shorts split it evenly.
fn open(nl: usize, ns: usize, base: u128) -> World {
    let mut w = World::new(nl, ns);
    open_book(&mut w, ns, base);
    w
}

fn open_book(w: &mut World, ns: usize, base: u128) {
    let nl = w.nl;
    for i in 0..nl {
        let q = base + 7_777 * (i as u128 + 1);
        for j in 0..ns {
            let part = if j + 1 == ns {
                q - (q / ns as u128) * (ns as u128 - 1)
            } else {
                q / ns as u128
            };
            w.trade_raw(i, nl + j, part).unwrap();
        }
    }
}

fn g_units(num: i128) -> f64 {
    num as f64 / ADL_ONE as f64
}

/// Strategy matrix: leg counts, position sizes, ADL depth (A), and the reducing route.
/// `run` checks the exact bounds after every op; the totals here show that nothing accumulates.
#[test]
fn harvest_449_strategy_matrix_never_accumulates_exposure() {
    #[derive(Clone, Copy, Debug)]
    enum Strat {
        /// shape A_long with a 1-unit short rebalance, then 1-unit bilateral reduces on every long
        BilateralAfterAdl,
        /// alternate unilateral reduces: long i (ADLs shorts), short j (ADLs longs)
        PingPongRebalance,
        /// reduce each long by a large odd chunk via trade; reshape with a prime-sized ADL
        ChunkedReduce,
    }
    let mut worst = (0i128, String::new());
    let mut drain_only_runs = 0;
    let mut best_avg = (0f64, String::new());
    for &(nl, ns) in &[(1usize, 2usize), (4, 2), (16, 2), (4, 4), (4, 1)] {
        for &base in &[1_000u128, 1_000 * POS_SCALE, 2_000_000 * POS_SCALE] {
            // ADL depth: fraction (num/den) of short[0]'s size removed unilaterally
            for &(fnum, fden) in &[(1u128, 1_000u128), (2, 3), (88, 100), (95, 100)] {
                for strat in [
                    Strat::BilateralAfterAdl,
                    Strat::PingPongRebalance,
                    Strat::ChunkedReduce,
                ] {
                    let mut w = open(nl, ns, base);
                    let s0 = w.eff(nl);
                    w.rebalance(nl, (s0 * fnum / fden).max(1)).unwrap();
                    let a_after_adl = w.asset().a_long;
                    let mut t = Tally::new(&w);
                    let rounds = if nl >= 16 { 60 } else { 250 };
                    for r in 0..rounds {
                        match strat {
                            Strat::BilateralAfterAdl => {
                                run(&mut w, &mut t, Op::RebalShort { j: 0, q: 1 });
                                for i in 0..nl {
                                    run(&mut w, &mut t, Op::Trade { i, j: ns - 1, q: 1 });
                                }
                            }
                            Strat::PingPongRebalance => {
                                for i in 0..nl {
                                    run(&mut w, &mut t, Op::RebalLong { i, q: 1 });
                                    run(
                                        &mut w,
                                        &mut t,
                                        Op::RebalShort {
                                            j: (i + r) % ns,
                                            q: 1,
                                        },
                                    );
                                }
                            }
                            Strat::ChunkedReduce => {
                                run(
                                    &mut w,
                                    &mut t,
                                    Op::RebalShort {
                                        j: 0,
                                        q: [2, 3, 5, 7, 11, 13][r % 6],
                                    },
                                );
                                for i in 0..nl {
                                    let q = (w.eff(i) / 997).max(1) | 1;
                                    run(&mut w, &mut t, Op::Trade { i, j: ns - 1, q });
                                }
                            }
                        }
                    }
                    drain_only_runs += usize::from(w.asset().a_long < MIN_A_SIDE);
                    let label = format!(
                        "{strat:?} nl={nl} ns={ns} base={base} adl={fnum}/{fden} a_long0={a_after_adl}"
                    );
                    let avg = g_units(t.max_g_num[0].max(t.max_g_num[1]))
                        / (t.side_ops[0].max(t.side_ops[1]).max(1) as f64);
                    if t.max_growth_num[0].max(t.max_growth_num[1]) > worst.0 {
                        worst = (t.max_growth_num[0].max(t.max_growth_num[1]), label.clone());
                    }
                    if avg > best_avg.0 && t.side_ops[0].max(t.side_ops[1]) > 50 {
                        best_avg = (avg, label.clone());
                    }
                    eprintln!(
                        "{label}: ops={} side_ops={:?} max_dG/op=[{:.4},{:.4}] maxG=[{:.2},{:.2}] maxD={:?} 2-sided/op={:.4} a_long={} drain_only={}",
                        t.ops,
                        t.side_ops,
                        g_units(t.max_growth_num[0]),
                        g_units(t.max_growth_num[1]),
                        g_units(t.max_g_num[0]),
                        g_units(t.max_g_num[1]),
                        t.max_d,
                        g_units(t.max_two_sided_num),
                        w.asset().a_long,
                        w.asset().a_long < MIN_A_SIDE,
                    );
                }
            }
        }
    }
    eprintln!(
        "WORST single-op growth = {:.6} raw units ({})",
        g_units(worst.0),
        worst.1
    );
    eprintln!(
        "BEST sustained growth = {:.4} raw units/same-side op ({})",
        best_avg.0, best_avg.1
    );
    // G moves only inside (-raw_basis_sum / ADL_ONE, 0], so a single op cannot raise it by a
    // tenth of a raw unit, and it never accumulates.
    assert!(worst.0 < (ADL_ONE / 10) as i128);
    assert_eq!(best_avg.0, 0.0, "no sustained growth");
    assert!(
        drain_only_runs > 0,
        "matrix must reach A < MIN_A_SIDE (DrainOnly)"
    );
}

/// Greedy adversary with two-step lookahead. For each ADL shaping size (short rebalance q),
/// try every harvest (any long leg, any chunk size) and keep the pair that maximises the long
/// side's G. Before the fix this steered A to leave each leg's exact exposure at the bottom of
/// its ceiling bucket before the reduce, and harvested more than half a raw unit per op. It
/// now ends where it started: with no exposure above the long OI.
#[test]
fn harvest_449_greedy_adl_shaping_adversary_harvests_nothing() {
    for &(nl, base) in &[(4usize, 1_000 * POS_SCALE), (8, 3 * POS_SCALE + 1)] {
        let ns = 2;
        let mut w = open(nl, ns, base);
        let s0 = w.eff(nl);
        w.rebalance(nl, s0 / 2).unwrap();
        let mut t = Tally::new(&w);
        let shapes = [1u128, 2, 3, 4, 5, 7, 11, 13, 17, 19, 23, 101, 997];
        let harvests = [1u128, 2, 3];
        let mut harvest_ops = 0u128;
        let mut best_harvest_growth = 0i128;
        let steps = 120;
        for _ in 0..steps {
            let g0 = w.census()[0].0;
            let mut best: Option<(i128, Op, Op)> = None;
            for &sq in &shapes {
                let mut w1 = w.clone();
                if w1.apply(Op::RebalShort { j: 0, q: sq }).is_err() {
                    continue;
                }
                for i in 0..nl {
                    for &hq in &harvests {
                        let mut w2 = w1.clone();
                        if w2.apply(Op::Trade { i, j: 1, q: hq }).is_err() {
                            continue;
                        }
                        let g = w2.census()[0].0 - g0;
                        if best.map_or(true, |(bg, _, _)| g > bg) {
                            best = Some((
                                g,
                                Op::RebalShort { j: 0, q: sq },
                                Op::Trade { i, j: 1, q: hq },
                            ));
                        }
                    }
                }
            }
            let Some((_, shape, harvest)) = best else {
                break;
            };
            assert!(run(&mut w, &mut t, shape));
            let before = w.census()[0].0;
            assert!(run(&mut w, &mut t, harvest));
            best_harvest_growth = best_harvest_growth.max(w.census()[0].0 - before);
            harvest_ops += 1;
        }
        let g = w.census()[0].0;
        eprintln!(
            "greedy nl={nl} base={base}: harvest_ops={harvest_ops} total_ops={} G_long={:.4} ({:.4}/harvest op, {:.4}/op) best single harvest={:.6} a_long={}",
            t.ops,
            g_units(g),
            g_units(g) / harvest_ops as f64,
            g_units(g) / t.ops as f64,
            g_units(best_harvest_growth),
            w.asset().a_long
        );
        assert!(harvest_ops > 0);
        // the best single step moves G by less than a tenth of a raw unit, inside
        // (-raw_basis_sum / ADL_ONE, 0], and the steered attacker ends with nothing
        assert!(best_harvest_growth < (ADL_ONE / 10) as i128);
        assert!(g <= 0, "greedy adversary kept G_num = {g}");
    }
}

/// Lifetime of a haircut side. While A != ADL_ONE no leg can grow or open, so the side's
/// effective OI and raw basis only shrink. Each reduction is rounded against the reducing leg,
/// so the raw basis falls at least as fast as the effective OI carried on it, and the side
/// ends in one of two ways: its effective OI reaches zero and it resets, or its effective OI
/// catches up with its raw basis and it is exactly at unit A again, every leg's effective
/// quantity equal to its raw basis. Only then can risk be added. At no point does the side
/// carry exposure above its OI.
#[test]
fn harvest_449_no_reopen_while_haircut_and_no_phantom_at_any_point() {
    let mut w = open(2, 2, 1_000);
    let s0 = w.eff(2);
    w.rebalance(2, s0 / 3).unwrap();
    assert_ne!(w.asset().a_long, ADL_ONE);
    let mut t = Tally::new(&w);
    let oi0 = w.asset().oi_eff_long_q;
    let mut peak = i128::MIN;
    loop {
        // risk increase is locked while A_long != ADL_ONE: no fresh leg, no growth
        let mut grow = w.clone();
        assert!(
            grow.trade_raw(0, 3, 1).is_err(),
            "long grow must be locked while haircut"
        );
        let progressed = (0..2).any(|i| run(&mut w, &mut t, Op::Trade { i, j: 1, q: 1 }))
            || run(&mut w, &mut t, Op::RebalShort { j: 0, q: 1 })
            || (0..2).any(|i| {
                run(
                    &mut w,
                    &mut t,
                    Op::RebalLong {
                        i,
                        q: u128::MAX >> 8,
                    },
                )
            });
        peak = peak.max(w.census()[0].0);
        let a = w.asset();
        if !progressed || a.oi_eff_long_q == 0 || a.a_long == ADL_ONE {
            break;
        }
    }
    let a = w.asset();
    let c = w.census();
    eprintln!(
        "lifetime: oi0={oi0} ops={} peak G={:.6} end oi={} raw={} a_long={}",
        t.ops,
        g_units(peak),
        a.oi_eff_long_q,
        a.raw_basis_sum_long_q,
        a.a_long
    );
    assert!(peak <= 0, "exposure above the long OI: G_num {peak}");
    assert!(t.ops < 2 * oi0 as u64, "bounded by the OI at the ADL");
    if a.oi_eff_long_q == 0 {
        assert_eq!(c[0].0, 0, "the side reset");
    } else {
        // back at unit A: nothing is rounded any more
        assert_eq!(a.a_long, ADL_ONE);
        assert_eq!(a.oi_eff_long_q, a.raw_basis_sum_long_q);
        assert_eq!((c[0].0, c[0].1), (0, 0), "(G_num, D) at unit A");
        // and the asset takes new risk again without a reset
        assert!(a.a_short == ADL_ONE && a.epoch_long == 0);
        let mut grow = w.clone();
        assert!(
            grow.trade_raw(0, 3, 1).is_ok(),
            "long grow is open again at unit A"
        );
    }
}

/// Realisation. The price moves 10% after the same 400 rounds that harvested more than a
/// hundred raw units of phantom long exposure before the fix. The adversary holds both sides
/// of the book. There is now no exposure above the long OI, so the face value of its accounts
/// moves by rounding only: each account's settlement rounds against it by less than one atom,
/// in either direction, with or without an honest winner in the same source domain.
#[test]
fn harvest_449_no_phantom_to_realize_in_either_direction() {
    const N: usize = 4;
    for honest in [false, true] {
        for up in [true, false] {
            let mut w = World::new(N, 2);
            w.accts.push(account(90));
            w.accts.push(account(91));
            let (hl, hs) = (w.accts.len() - 2, w.accts.len() - 1);
            w.deposit(hl, DEPOSIT);
            w.deposit(hs, DEPOSIT);
            let p1 = P0 + P0 / 10;
            if honest {
                // honest long wins 10% on 10_000 base, then both go flat
                let qh = 10_000 * POS_SCALE;
                w.trade_raw(hl, hs, qh).unwrap();
                w.set_price(p1);
                w.trade_raw(hs, hl, qh).unwrap();
            } else {
                w.set_price(p1);
            }
            open_book(&mut w, 2, 1_000 * POS_SCALE);
            let s0 = w.eff(N);
            w.rebalance(N, s0 * 2 / 3).unwrap();
            let mut t = Tally::new(&w);
            for _ in 0..400 {
                run(&mut w, &mut t, Op::RebalShort { j: 0, q: 1 });
                for i in 0..N {
                    run(&mut w, &mut t, Op::Trade { i, j: 1, q: 1 });
                }
            }
            let g_num = w.census()[0].0;
            assert!(g_num <= 0, "exposure above the long OI: G_num {g_num}");
            let att: Vec<usize> = (0..N + 2).collect();
            let face0: i128 = att.iter().map(|&k| w.face_value(k)).sum();
            let cert0: i128 = att.iter().map(|&k| w.certified_equity(k)).sum();
            let hcert0 = w.certified_equity(hl);
            let p2 = if up { p1 + p1 / 10 } else { p1 - p1 / 10 };
            w.set_price(p2);
            let face1: i128 = att.iter().map(|&k| w.face_value(k)).sum();
            let cert1: i128 = att.iter().map(|&k| w.certified_equity(k)).sum();
            let hcert1 = w.certified_equity(hl);
            let (face_gain, cert_gain, honest_delta) =
                (face1 - face0, cert1 - cert0, hcert1 - hcert0);
            eprintln!(
                "realize honest={honest} up={up}: G={:.6} raw, face gain={face_gain} atoms, adversary certified gain={cert_gain}, honest certified delta={honest_delta}, honest pnl={}",
                g_units(g_num),
                w.accts[hl].pnl.get()
            );
            assert!(
                face_gain <= 0 && face_gain >= -(N as i128 + 2),
                "face value moved by {face_gain} atoms"
            );
            assert!(cert_gain <= 0, "nothing is realizable: {cert_gain}");
            if honest {
                // the honest winner is flat during the move and pays for nothing
                assert!(honest_delta >= 0, "honest winner lost {honest_delta}");
            }
        }
    }
}
