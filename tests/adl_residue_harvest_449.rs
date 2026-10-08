//! Issue percolator-prog#449, adversarial follow-up: can an attacker who owns
//! every account turn the ADL-effective ceiling residue into meaningful value?
//!
//! Notation (per side, current-epoch legs only, every leg opened at a_basis = ADL_ONE):
//!   G_num = sum(raw * A) - OI * ADL_ONE   (exact exposure over OI, scaled by ADL_ONE)
//!   D     = sum(ceil(raw * A / ADL_ONE)) - OI
//! G is the phantom exposure: a +dP move pays the side G * dP / POS_SCALE more than the
//! other side loses.
//!
//! What these tests pin:
//! * Every same-side reducing op raises that side's G by strictly less than one raw unit,
//!   whatever the route (bilateral trade, RebalanceReduce, full clear), size, leg count,
//!   or A. ADL re-scaling of the passive side never raises G. So G stays below
//!   (same-side reducing ops) * 1 raw unit. The bound also holds against a greedy search
//!   that picks the ADL shaping size and the harvest op that maximise the next step's
//!   growth.
//! * No op touches more than one leg per (asset, side): a bilateral trade reduces at most one
//!   long leg and one short leg, so the combined two-sided growth stays below 2 per op.
//! * Realisation: the phantom pays out only through source-domain backing. In a closed universe
//!   the excess long PnL is not realizable equity. When another winner shares the same source
//!   domain, the attacker takes the excess from that winner's haircut. The amount stays at most
//!   G * dP / POS_SCALE atoms, and the attacker carries the matching downside.

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

fn run(w: &mut World, t: &mut Tally, op: Op) -> bool {
    let before = w.census();
    let snapshot = w.clone();
    match w.apply(op) {
        Ok(()) => {
            let after = w.census();
            let ns = w.accts.len() - w.nl;
            t.record(before, after, op, [w.nl, ns]);
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
#[test]
fn harvest_449_strategy_matrix_growth_per_op_below_one_raw_unit() {
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
    assert!(worst.0 < ADL_ONE as i128);
    assert!(best_avg.0 < 1.0);
    assert!(
        drain_only_runs > 0,
        "matrix must reach A < MIN_A_SIDE (DrainOnly)"
    );
}

/// Greedy adversary with two-step lookahead. For each ADL shaping size (short rebalance q),
/// try every harvest (any long leg, any chunk size) and keep the pair that maximises the long
/// side's G. This is the attacker steering A to leave each leg's exact exposure at the bottom
/// of its ceiling bucket before the reduce.
#[test]
fn harvest_449_greedy_adl_shaping_adversary_stays_below_one_unit_per_op() {
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
        assert!(best_harvest_growth < ADL_ONE as i128);
        assert!(g < (harvest_ops * ADL_ONE) as i128);
        // the steered attacker approaches, but never reaches, one raw unit per harvest op
        assert!(
            g_units(g) / harvest_ops as f64 > 0.5,
            "greedy should beat random bucket placement"
        );
    }
}

/// Bounded lifetime. While A != ADL_ONE no leg can grow or open, so OI only shrinks, and each
/// reducing op removes at least one raw unit of OI. G therefore stays below both the op count
/// and the OI that existed at the ADL. When OI reaches zero the side resets and the phantom
/// vanishes.
#[test]
fn harvest_449_no_reopen_while_haircut_and_phantom_dies_at_reset() {
    let mut w = open(2, 2, 1_000);
    let s0 = w.eff(2);
    w.rebalance(2, s0 / 3).unwrap();
    // risk increase is locked while A_long != ADL_ONE: no fresh leg, no growth
    let mut grow = w.clone();
    assert!(
        grow.trade_raw(0, 3, 1).is_err(),
        "long grow must be locked while haircut"
    );
    let mut t = Tally::new(&w);
    let oi0 = w.asset().oi_eff_long_q;
    let mut peak = 0i128;
    loop {
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
        if !progressed || w.asset().oi_eff_long_q == 0 {
            break;
        }
    }
    assert_eq!(w.asset().oi_eff_long_q, 0);
    assert_eq!(w.census()[0].0, 0, "reset erases the phantom");
    eprintln!(
        "lifetime: oi0={oi0} ops={} peak G={:.3}",
        t.ops,
        g_units(peak)
    );
    assert!(peak < (oi0 * ADL_ONE) as i128);
}

/// Realisation. The price moves 10% while the harvested phantom G is live on the long side.
/// The attacker holds both sides, so the face-value change is exactly the phantom
/// G * dP / POS_SCALE. It is directional: a down move turns it into a real loss.
/// Realizable (engine-certified, haircut) equity:
/// (a) Closed universe: no extra backing in the long source domain, so the phantom gain is
///     not realizable.
/// (b) An honest winner earned backed PnL on an earlier episode of the same asset. The
///     attacker's realizable gain is bounded by the face phantom and paid by the honest
///     winner's haircut.
#[test]
fn harvest_449_realization_is_capped_by_phantom_and_directional() {
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
            assert!(g_num > 100 * ADL_ONE as i128);
            let att: Vec<usize> = (0..N + 2).collect();
            let face0: i128 = att.iter().map(|&k| w.face_value(k)).sum();
            let cert0: i128 = att.iter().map(|&k| w.certified_equity(k)).sum();
            let hcert0 = w.certified_equity(hl);
            let p2 = if up { p1 + p1 / 10 } else { p1 - p1 / 10 };
            w.set_price(p2);
            let face1: i128 = att.iter().map(|&k| w.face_value(k)).sum();
            let cert1: i128 = att.iter().map(|&k| w.certified_equity(k)).sum();
            let hcert1 = w.certified_equity(hl);
            let dp = p2 as i128 - p1 as i128;
            let predicted = g_num * dp / (ADL_ONE * POS_SCALE) as i128;
            let (face_gain, cert_gain, honest_delta) =
                (face1 - face0, cert1 - cert0, hcert1 - hcert0);
            eprintln!(
                "realize honest={honest} up={up}: G={:.2} raw ({:.6} base), face gain={face_gain} atoms (pred {predicted}), attacker certified gain={cert_gain}, honest certified delta={honest_delta}, honest pnl={}",
                g_units(g_num),
                g_units(g_num) / POS_SCALE as f64,
                w.accts[hl].pnl.get()
            );
            assert!(
                (face_gain - predicted).abs() <= N as i128 + 2,
                "face {face_gain} vs {predicted}"
            );
            assert!(
                cert_gain <= face_gain.max(0),
                "realizable never exceeds the phantom"
            );
            if !up {
                assert!(face_gain < 0, "down move: phantom is a real loss");
                assert!(cert_gain < 0);
            }
            if up && !honest {
                assert!(
                    cert_gain <= 0,
                    "closed universe: the phantom is unbacked, so nothing is realizable"
                );
            }
            if up && honest {
                // whatever the attacker realizes comes out of the honest winner's haircut
                assert!(honest_delta <= 0);
                assert!(cert_gain.max(0) <= -honest_delta + N as i128 + 2);
            }
        }
    }
}
