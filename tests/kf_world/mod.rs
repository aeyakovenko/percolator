//! A small first-principles world for K/F loss settlement tests.
//!
//! Everything goes through the public `*_not_atomic` API: deposits, provider backing, trades
//! at the mark, price and funding accrual, refreshes, conversions, resolution, resolved closes,
//! provider withdrawals and terminal retirement. The world keeps its own books beside the
//! engine: each account's position per asset and the value its positions earned. No trading
//! fee, and (in the tests using it) nobody is ever insolvent, so:
//!
//! * an account's capital + pnl must equal its deposits plus what its positions earned;
//! * once every account has been refreshed at the current marks, every loser has paid its
//!   whole loss, so every source domain must hold exactly its provider's principal on top of
//!   the claims registered against it, and nothing may sit in the junior residual;
//! * after wind-down every trader is paid exactly its equity, every provider takes exactly its
//!   principal back, and nothing is retired.
#![allow(dead_code)]

use percolator::{
    AutoCrankObservationV16, AutoCrankPlanV16, AutoCrankWorkV16, BackingBucketStatusV16,
    BackingBucketV16, EngineAssetSlotV16Account, Market, MarketGroupV16HeaderAccount,
    MarketGroupV16ViewMut, PortfolioAccountV16Account, PortfolioV16ViewMut, ProvenanceHeaderV16,
    ProvenanceHeaderV16Account, ResolvedCloseOutcomeV16, SourceCreditStateV16,
    TerminalSlabOutcomeV16, TradeRequestV16, V16Config, V16Error, BOUND_SCALE, POS_SCALE,
};

/// Mark of every asset at genesis. Large enough that a 1% move and the maximum funding rate
/// are whole atoms per unit of position.
pub const PX0: u64 = 1_000_000;
/// Trader deposit that keeps every position in these tests fully collateralized.
pub const DEPOSIT: u128 = 1_000_000_000_000;
/// Expiry of provider-deposited backing, and the lifetime the engine gives the backing it
/// creates from loser payments; far beyond any test's last slot.
pub const BACKING_LIFETIME: u64 = 10_000;

pub struct World {
    pub header: MarketGroupV16HeaderAccount,
    pub markets: Vec<Market<u64>>,
    pub accts: Vec<PortfolioAccountV16Account>,
    /// Current mark per asset.
    pub px: Vec<u64>,
    pub slot: u64,
    /// The world's own books: position per account per asset, in whole units (long positive).
    pub pos: Vec<Vec<i128>>,
    /// The world's own books: deposits plus everything the account's positions earned.
    pub equity: Vec<i128>,
    /// Provider principal per source domain, in atoms.
    pub provider: Vec<u128>,
}

/// What everyone ends with after the market is wound down.
#[derive(Debug, PartialEq, Eq)]
pub struct WindDown {
    /// What each domain's provider took out, in atoms.
    pub provider_out: Vec<u128>,
    /// Resolved close payout of each account.
    pub payouts: Vec<u128>,
    pub retired: u128,
    pub insurance: u128,
    pub vault_end: u128,
}

fn account_fixture(seed: u8) -> PortfolioAccountV16Account {
    let header = ProvenanceHeaderV16Account::from_runtime(&ProvenanceHeaderV16::new(
        [1; 32], [seed; 32], [3; 32],
    ));
    let mut account = PortfolioAccountV16Account::default();
    account.init_empty_in_place(header).unwrap();
    account
}

/// BOUND_SCALE amount as whole atoms, with any fraction spelled out.
pub fn atoms(num: u128) -> String {
    if num % BOUND_SCALE == 0 {
        format!("{}", num / BOUND_SCALE)
    } else {
        format!("{}+{}/1e12", num / BOUND_SCALE, num % BOUND_SCALE)
    }
}

fn pair<'a, 'b>(
    a: &'a mut [PortfolioV16ViewMut<'b>],
    i: usize,
    j: usize,
) -> (
    &'a mut PortfolioV16ViewMut<'b>,
    &'a mut PortfolioV16ViewMut<'b>,
) {
    assert_ne!(i, j);
    if i < j {
        let (lo, hi) = a.split_at_mut(j);
        (&mut lo[i], &mut hi[0])
    } else {
        let (lo, hi) = a.split_at_mut(i);
        (&mut hi[0], &mut lo[j])
    }
}

impl World {
    /// `assets` assets at `PX0`, one account per entry of `deposits`. 100% margin.
    pub fn new(assets: usize, deposits: &[u128]) -> Self {
        Self::with_config(assets, deposits, |_| {})
    }

    pub fn with_config(
        assets: usize,
        deposits: &[u128],
        tweak: impl FnOnce(&mut V16Config),
    ) -> Self {
        let mut cfg = V16Config::public_user_fund_with_market_slots(
            assets as u16,
            assets as u32,
            0,
            BACKING_LIFETIME,
        );
        cfg.max_price_move_bps_per_slot = 500;
        cfg.max_accrual_dt_slots = 1;
        cfg.max_abs_funding_e9_per_slot = 10_000;
        tweak(&mut cfg);
        let mut header =
            MarketGroupV16HeaderAccount::new_dynamic([1; 32], cfg, assets as u32, 0).unwrap();
        let mut markets: Vec<Market<u64>> = (0..assets)
            .map(|i| Market::new(i as u64, EngineAssetSlotV16Account::default()))
            .collect();
        // One activation per slot (activation cooldown), then every asset is brought to the
        // same slot.
        for i in 0..assets {
            header
                .activate_empty_asset_slot_not_atomic(
                    i as u32,
                    &mut markets[i].engine,
                    PX0,
                    (i + 1) as u64,
                )
                .unwrap();
        }
        {
            let mut m = MarketGroupV16ViewMut::new(&mut header, &mut markets);
            for i in 0..assets {
                m.accrue_asset_to_not_atomic(i, assets as u64, PX0, 0, true)
                    .unwrap();
            }
        }
        let mut w = World {
            header,
            markets,
            accts: (0..deposits.len())
                .map(|i| account_fixture(10 + i as u8))
                .collect(),
            px: vec![PX0; assets],
            slot: assets as u64,
            pos: vec![vec![0; assets]; deposits.len()],
            equity: deposits.iter().map(|d| *d as i128).collect(),
            provider: vec![0; assets * 2],
        };
        for (i, d) in deposits.iter().enumerate() {
            // Every account is a materialized portfolio, as in the program: the slab cannot
            // be retired while one of them still holds anything, an unpaid receipt included.
            w.op(|m, a| m.register_empty_materialized_portfolio_not_atomic(&a[i].as_view()))
                .unwrap();
            w.op(|m, a| m.deposit_not_atomic(&mut a[i], *d)).unwrap();
        }
        w
    }

    /// Runs one engine instruction with transaction atomicity: an `Err` rolls every mutation
    /// back, as the program does.
    pub fn op<R>(
        &mut self,
        f: impl FnOnce(
            &mut MarketGroupV16ViewMut<'_, u64>,
            &mut [PortfolioV16ViewMut<'_>],
        ) -> Result<R, V16Error>,
    ) -> Result<R, V16Error> {
        let snapshot = (self.header, self.markets.clone(), self.accts.clone());
        let result = {
            let mut m = MarketGroupV16ViewMut::new(&mut self.header, &mut self.markets);
            let mut views: Vec<PortfolioV16ViewMut<'_>> = self
                .accts
                .iter_mut()
                .map(PortfolioV16ViewMut::new)
                .collect();
            f(&mut m, &mut views)
        };
        if result.is_err() {
            self.header = snapshot.0;
            self.markets = snapshot.1;
            self.accts = snapshot.2;
        }
        result
    }

    /// Source domain of `(asset, side)`: the domain that side's losses are paid into and the
    /// opposite side's gains are claimed from. Long is 0, short is 1.
    pub fn loss_domain(asset: usize, long: bool) -> usize {
        2 * asset + usize::from(!long)
    }

    pub fn provider_deposit(&mut self, domain: usize, amount: u128) {
        self.op(|m, _| {
            m.deposit_fresh_counterparty_backing_not_atomic(domain, amount, BACKING_LIFETIME)
        })
        .unwrap();
        self.provider[domain] += amount;
    }

    /// `buyer` buys `units` of `asset` from `seller` at the mark.
    pub fn trade(&mut self, buyer: usize, seller: usize, asset: usize, units: u128) {
        self.try_trade(buyer, seller, asset, units)
            .unwrap_or_else(|e| panic!("trade {buyer}<-{seller} asset {asset} x{units}: {e:?}"));
    }

    pub fn try_trade(
        &mut self,
        buyer: usize,
        seller: usize,
        asset: usize,
        units: u128,
    ) -> Result<(), V16Error> {
        let px = self.px[asset];
        self.op(|m, a| {
            let (b, s) = pair(a, buyer, seller);
            m.execute_trade_with_fee_loss_stale_scoped_not_atomic(
                b,
                s,
                TradeRequestV16 {
                    asset_index: asset,
                    size_q: (units * POS_SCALE) as i128,
                    exec_price: px,
                    fee_bps: 0,
                },
            )
        })?;
        self.pos[buyer][asset] += units as i128;
        self.pos[seller][asset] -= units as i128;
        Ok(())
    }

    /// Advances one slot: `asset` moves to `px` and is charged `funding_e9` for that slot
    /// (a long pays `units * funding_e9 * px / 1e9`; negative rates pay the longs). Every
    /// asset is accrued to the new slot.
    pub fn step(&mut self, asset: usize, px: u64, funding_e9: i128) {
        self.slot += 1;
        let slot = self.slot;
        let old = self.px[asset];
        let prices = {
            let mut p = self.px.clone();
            p[asset] = px;
            p
        };
        self.op(|m, _| {
            m.set_asset_raw_oracle_target_not_atomic(asset, px)?;
            for (a, p) in prices.iter().enumerate() {
                let rate = if a == asset { funding_e9 } else { 0 };
                m.accrue_asset_to_not_atomic(a, slot, *p, rate, true)?;
            }
            Ok(())
        })
        .unwrap_or_else(|e| panic!("step asset {asset} {old}->{px}: {e:?}"));
        self.px[asset] = px;
        for i in 0..self.accts.len() {
            let funding_e9_atoms = self.pos[i][asset] * funding_e9 * px as i128;
            assert_eq!(
                funding_e9_atoms % 1_000_000_000,
                0,
                "choose positions, funding rates and marks that pay whole atoms"
            );
            self.equity[i] +=
                self.pos[i][asset] * (px as i128 - old as i128) - funding_e9_atoms / 1_000_000_000;
        }
    }

    pub fn refresh(&mut self, i: usize) {
        self.try_refresh(i)
            .unwrap_or_else(|e| panic!("refresh of account {i}: {e:?}"));
    }

    pub fn try_refresh(&mut self, i: usize) -> Result<(), V16Error> {
        self.op(|m, a| m.full_account_refresh_not_atomic(&mut a[i]).map(|_| ()))
    }

    /// Brings the account current through the permissionless auto-crank (the engine picks
    /// each step) instead of a direct refresh.
    pub fn crank(&mut self, i: usize) {
        let now_slot = self.slot;
        let observations: Vec<AutoCrankObservationV16> = self
            .px
            .iter()
            .enumerate()
            .map(|(asset_index, px)| AutoCrankObservationV16 {
                asset_index,
                effective_price: *px,
                funding_rate_e9: 0,
            })
            .collect();
        for _ in 0..32 {
            let result = self
                .op(|m, a| {
                    m.permissionless_auto_crank_not_atomic(
                        &mut a[i],
                        AutoCrankWorkV16 {
                            now_slot,
                            observations: &observations,
                            resolved_close_fee_rate_per_slot: 0,
                        },
                    )
                })
                .unwrap_or_else(|e| panic!("crank of account {i}: {e:?}"));
            if result.selected == AutoCrankPlanV16::NoAction {
                return;
            }
        }
        panic!("crank of account {i} reached no fixpoint");
    }

    /// Elective Live conversion of released positive PnL into capital.
    pub fn convert(&mut self, i: usize) -> Result<u128, V16Error> {
        self.op(|m, a| m.convert_released_pnl_to_capital_not_atomic(&mut a[i]))
    }

    pub fn source(&self, domain: usize) -> SourceCreditStateV16 {
        let e = &self.markets[domain / 2].engine;
        if domain % 2 == 0 {
            e.source_credit_long.try_to_runtime().unwrap()
        } else {
            e.source_credit_short.try_to_runtime().unwrap()
        }
    }

    pub fn bucket(&self, domain: usize) -> BackingBucketV16 {
        let e = &self.markets[domain / 2].engine;
        if domain % 2 == 0 {
            e.backing_long.try_to_runtime().unwrap()
        } else {
            e.backing_short.try_to_runtime().unwrap()
        }
    }

    /// (claims registered against the domain, fresh counterparty backing it holds), in atoms.
    pub fn domain(&self, domain: usize) -> (u128, u128) {
        let s = self.source(domain);
        assert_eq!(s.positive_claim_bound_num % BOUND_SCALE, 0);
        assert_eq!(s.fresh_reserved_backing_num % BOUND_SCALE, 0);
        (
            s.positive_claim_bound_num / BOUND_SCALE,
            s.fresh_reserved_backing_num / BOUND_SCALE,
        )
    }

    /// The account's claim against `domain`, in atoms.
    pub fn claim(&self, i: usize, domain: usize) -> u128 {
        self.accts[i]
            .source_domains
            .iter()
            .filter(|s| s.is_occupied() && s.domain.get() as usize == domain)
            .map(|s| s.source_claim_bound_num.get() / BOUND_SCALE)
            .sum()
    }

    /// Vault value that is neither capital, insurance, provider earnings nor counterparty
    /// backing: the junior residual pool.
    pub fn junior_residual(&self) -> i128 {
        let h = &self.header;
        h.vault.get() as i128
            - h.c_tot.get() as i128
            - h.insurance.get() as i128
            - h.backing_provider_earnings_total.get() as i128
            - (h.source_fresh_backing_total_num.get() / BOUND_SCALE) as i128
    }

    pub fn engine_equity(&self, i: usize) -> i128 {
        self.accts[i].capital.get() as i128 + self.accts[i].pnl.get()
    }

    pub fn refresh_all(&mut self, order: &[usize]) {
        for &i in order {
            self.refresh(i);
        }
    }

    /// Violations of the settled-state properties (see the module docs). Call only when every
    /// account has just been refreshed at the current marks.
    pub fn settled_violations(&self) -> Vec<String> {
        let mut out = Vec::new();
        for i in 0..self.accts.len() {
            if self.engine_equity(i) != self.equity[i] {
                out.push(format!(
                    "account {i} holds {} but its deposits and positions earned {}",
                    self.engine_equity(i),
                    self.equity[i]
                ));
            }
            if self.accts[i].pnl.get() < 0 {
                out.push(format!("account {i} has negative pnl"));
            }
        }
        for d in 0..self.provider.len() {
            let s = self.source(d);
            let want = s.positive_claim_bound_num + self.provider[d] * BOUND_SCALE;
            if s.fresh_reserved_backing_num != want {
                out.push(format!(
                    "domain {d} holds {} of backing for {} of claims and {} of provider principal",
                    atoms(s.fresh_reserved_backing_num),
                    atoms(s.positive_claim_bound_num),
                    self.provider[d]
                ));
            }
        }
        if self.junior_residual() != 0 {
            out.push(format!("junior residual is {}", self.junior_residual()));
        }
        if let Err(e) = self.validate() {
            out.push(format!("engine validation failed: {e:?}"));
        }
        out
    }

    pub fn assert_settled(&self, tag: &str) {
        let v = self.settled_violations();
        assert!(v.is_empty(), "{tag}: {}\n{}", v.join("; "), self.describe());
    }

    /// The engine's own whole-state and per-account validators.
    pub fn validate(&self) -> Result<(), V16Error> {
        let mut header = self.header;
        let mut markets = self.markets.clone();
        let mut accts = self.accts.clone();
        let m = MarketGroupV16ViewMut::new(&mut header, &mut markets);
        m.validate_shape()?;
        for a in accts.iter_mut() {
            PortfolioV16ViewMut::new(a).validate_with_market(&m.as_view())?;
        }
        Ok(())
    }

    /// Resolves the market as it stands and winds it down the way the program does:
    ///
    /// 1. every account closes (round robin in `close_order` until all are closed);
    /// 2. each domain's provider takes out all the backing the engine lets it take;
    /// 3. time passes beyond every backing lifetime and the buckets left over are expired
    ///    (their principal forfeits to the junior pool);
    /// 4. accounts still holding a resolved receipt claim what it now pays;
    /// 5. emptied portfolios are dematerialized and the slab is retired.
    ///
    /// Value that nobody could take stays in `vault_end` (the slab then never becomes ready
    /// to close and `retired` is 0).
    pub fn wind_down(&mut self, close_order: &[usize]) -> WindDown {
        let n = self.accts.len();
        self.slot += 1;
        let resolved_slot = self.slot;
        self.op(|m, _| m.resolve_market_not_atomic(resolved_slot))
            .unwrap();
        let mut payouts: Vec<Option<u128>> = vec![None; n];
        for _ in 0..64 {
            for &i in close_order {
                if payouts[i].is_some() {
                    continue;
                }
                match self
                    .op(|m, a| m.close_resolved_account_not_atomic(&mut a[i], 0))
                    .unwrap_or_else(|e| panic!("resolved close of account {i}: {e:?}"))
                {
                    ResolvedCloseOutcomeV16::ProgressOnly => {}
                    ResolvedCloseOutcomeV16::Closed { payout } => payouts[i] = Some(payout),
                }
            }
            if payouts.iter().all(Option::is_some) {
                break;
            }
        }
        let mut payouts: Vec<u128> = payouts
            .into_iter()
            .enumerate()
            .map(|(i, p)| p.unwrap_or_else(|| panic!("account {i} did not close")))
            .collect();

        let mut provider_out = vec![0u128; self.provider.len()];
        for d in 0..self.provider.len() {
            let b = self.bucket(d);
            if b.status != BackingBucketStatusV16::Fresh {
                continue;
            }
            // The whole bucket if the engine allows it, else its resolved surplus.
            let free = b.fresh_unliened_backing_num / BOUND_SCALE;
            let surplus = self
                .op(|m, _| m.resolved_counterparty_backing_surplus(d))
                .unwrap_or(0);
            for amount in [free, surplus] {
                if amount != 0
                    && self
                        .op(|m, _| m.withdraw_fresh_counterparty_backing_not_atomic(d, amount))
                        .is_ok()
                {
                    provider_out[d] = amount;
                    break;
                }
            }
        }

        // Past every backing lifetime: what no provider took lapses into the junior pool.
        let end_slot = resolved_slot + 2 * BACKING_LIFETIME;
        self.op(|m, _| m.advance_resolved_slot_not_atomic(end_slot))
            .unwrap();
        for d in 0..self.provider.len() {
            if self.bucket(d).status == BackingBucketStatusV16::Fresh {
                self.op(|m, _| m.expire_source_backing_bucket_not_atomic(d, end_slot))
                    .unwrap_or_else(|e| panic!("expiry of domain {d}: {e:?}"));
            }
        }
        for i in 0..n {
            if self.accts[i].resolved_payout_receipt.present != 0 {
                if let Ok(paid) =
                    self.op(|m, a| m.claim_resolved_payout_topup_not_atomic(&mut a[i]))
                {
                    payouts[i] += paid;
                }
            }
            let _ = self
                .op(|m, a| m.deregister_empty_materialized_portfolio_not_atomic(&a[i].as_view()));
        }

        let mut retired = 0;
        let mut start = 0usize;
        for _ in 0..64 {
            match self.op(|m, _| m.advance_terminal_slab_not_atomic(end_slot, start)) {
                Ok(TerminalSlabOutcomeV16::ReadyToClose { retired: r }) => {
                    retired = r;
                    break;
                }
                Ok(TerminalSlabOutcomeV16::ScanProgress { next_asset_index }) => {
                    start = next_asset_index
                }
                Ok(_) => start = 0,
                // Something is still owed to a portfolio that cannot collect it.
                Err(_) => break,
            }
        }
        WindDown {
            provider_out,
            payouts,
            retired,
            insurance: self.header.insurance.get(),
            vault_end: self.header.vault.get(),
        }
    }

    /// The only fair wind-down: every trader is paid its equity, every provider takes its
    /// principal back, nothing is retired and the vault ends empty.
    pub fn fair_wind_down(&self) -> WindDown {
        WindDown {
            provider_out: self.provider.clone(),
            payouts: self.equity.iter().map(|e| *e as u128).collect(),
            retired: 0,
            insurance: 0,
            vault_end: 0,
        }
    }

    /// Human-readable state, for assertion messages.
    pub fn describe(&self) -> String {
        let h = &self.header;
        let mut out = format!(
            "slot {} marks {:?} vault {} c_tot {} insurance {} junior residual {}\n",
            self.slot,
            self.px,
            h.vault.get(),
            h.c_tot.get(),
            h.insurance.get(),
            self.junior_residual()
        );
        for d in 0..self.provider.len() {
            let s = self.source(d);
            let b = self.bucket(d);
            if s == SourceCreditStateV16::EMPTY && b.status == BackingBucketStatusV16::Empty {
                continue;
            }
            out.push_str(&format!(
                "  domain {d}: claims {} backing {} (liened {}) provider principal {} receivable {} rate {}/1e12 bucket {:?}\n",
                atoms(s.positive_claim_bound_num),
                atoms(s.fresh_reserved_backing_num),
                atoms(s.valid_liened_backing_num),
                self.provider[d],
                atoms(s.provider_receivable_num),
                s.credit_rate_num,
                b.status,
            ));
        }
        for (i, a) in self.accts.iter().enumerate() {
            let claims: Vec<String> = a
                .source_domains
                .iter()
                .filter(|s| s.is_occupied())
                .map(|s| {
                    format!(
                        "domain {}: {} (liened {})",
                        s.domain.get(),
                        atoms(s.source_claim_bound_num.get()),
                        atoms(s.source_claim_liened_num.get())
                    )
                })
                .collect();
            out.push_str(&format!(
                "  account {i}: capital {} pnl {} (books say {}) positions {:?} claims {:?}\n",
                a.capital.get(),
                a.pnl.get(),
                self.equity[i],
                self.pos[i],
                claims
            ));
        }
        out
    }
}

// ---------------------------------------------------------------------------- random worlds

/// One step of a randomly generated market history.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    /// `buyer` buys `units` of `asset` from `seller` at the mark (skipped if the engine
    /// rejects it).
    Trade {
        buyer: usize,
        seller: usize,
        asset: usize,
        units: u128,
    },
    /// `asset` moves by `delta` over one slot and is charged `funding_e9` for it.
    Step {
        asset: usize,
        delta: i64,
        funding_e9: i128,
    },
    Refresh(usize),
    Convert(usize),
    /// Refresh everyone, starting at account `first`, then require the settled-state
    /// properties.
    Settle {
        first: usize,
    },
}

#[derive(Clone, Debug)]
pub struct Shape {
    pub assets: usize,
    pub accounts: usize,
    /// (domain, atoms) of provider backing deposited at genesis.
    pub providers: Vec<(usize, u128)>,
}

/// xorshift64*: a fixed, dependency-free generator so every run sees the same worlds.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    pub fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// A random world shape and history: 1-3 assets, 3-5 fully collateralized accounts, provider
/// backing in a random third of the domains, then `len` trades, 1-4% price moves (with funding
/// on a third of them when `funding`), single-account refreshes, conversions and settlements.
pub fn random_history(seed: u64, len: usize, funding: bool) -> (Shape, Vec<Op>) {
    let mut r = Rng::new(seed);
    let assets = 1 + r.below(3) as usize;
    let accounts = 3 + r.below(3) as usize;
    let mut providers = Vec::new();
    for d in 0..assets * 2 {
        if r.below(3) == 0 {
            providers.push((d, 1_000_000_000 * (1 + r.below(5) as u128)));
        }
    }
    let mut ops = Vec::new();
    for _ in 0..len {
        let op = match r.below(10) {
            0..=2 => {
                let buyer = r.below(accounts as u64) as usize;
                let mut seller = r.below(accounts as u64) as usize;
                if seller == buyer {
                    seller = (seller + 1) % accounts;
                }
                Op::Trade {
                    buyer,
                    seller,
                    asset: r.below(assets as u64) as usize,
                    units: 100 * (1 + r.below(10) as u128),
                }
            }
            3..=5 => Op::Step {
                asset: r.below(assets as u64) as usize,
                delta: 10_000 * (1 + r.below(4) as i64) * if r.below(2) == 0 { 1 } else { -1 },
                funding_e9: if funding && r.below(3) == 0 {
                    1_000 * (1 + r.below(10) as i128) * if r.below(2) == 0 { 1 } else { -1 }
                } else {
                    0
                },
            },
            6..=7 => Op::Refresh(r.below(accounts as u64) as usize),
            8 => Op::Convert(r.below(accounts as u64) as usize),
            _ => Op::Settle {
                first: r.below(accounts as u64) as usize,
            },
        };
        ops.push(op);
    }
    (
        Shape {
            assets,
            accounts,
            providers,
        },
        ops,
    )
}

/// The history of a fully cranked market: every account is refreshed after every price step
/// (the order rotates), and nobody is refreshed in between.
pub fn fully_cranked(ops: Vec<Op>) -> Vec<Op> {
    ops.into_iter()
        .filter(|op| !matches!(op, Op::Refresh(_) | Op::Settle { .. }))
        .flat_map(|op| match op {
            Op::Step { asset, delta, .. } => vec![
                op,
                Op::Settle {
                    first: asset + delta.unsigned_abs() as usize / 10_000,
                },
            ],
            other => vec![other],
        })
        .collect()
}

/// Builds the world of `shape` and plays `ops`. `Err` describes the first settled-state
/// violation.
pub fn play(shape: &Shape, ops: &[Op]) -> Result<World, String> {
    let mut w = World::new(shape.assets, &vec![DEPOSIT; shape.accounts]);
    for (d, amount) in &shape.providers {
        w.provider_deposit(*d, *amount);
    }
    for (at, op) in ops.iter().enumerate() {
        match *op {
            Op::Trade {
                buyer,
                seller,
                asset,
                units,
            } => {
                let _ = w.try_trade(buyer, seller, asset, units);
            }
            Op::Step {
                asset,
                delta,
                funding_e9,
            } => {
                let px = w.px[asset] as i64 + delta;
                // Stay inside the 500 bps per-slot price-move cap and a sane price range.
                if (700_000..=1_400_000).contains(&px)
                    && delta.unsigned_abs() * 10_000 <= 480 * w.px[asset]
                {
                    w.step(asset, px as u64, funding_e9);
                }
            }
            Op::Refresh(i) => w
                .try_refresh(i)
                .map_err(|e| format!("op {at}: refresh of account {i} failed: {e:?}"))?,
            Op::Convert(i) => {
                let _ = w.convert(i);
            }
            Op::Settle { first } => {
                for k in 0..shape.accounts {
                    let i = (first + k) % shape.accounts;
                    w.try_refresh(i)
                        .map_err(|e| format!("op {at}: refresh of account {i} failed: {e:?}"))?;
                }
                let v = w.settled_violations();
                if !v.is_empty() {
                    return Err(format!(
                        "op {at} {op:?}: {}\n{}",
                        v.join("; "),
                        w.describe()
                    ));
                }
            }
        }
    }
    Ok(w)
}

/// Greedy delta debugging: drops ops while `fails` still holds, so a failing history is
/// reported at its smallest.
pub fn shrink(ops: &[Op], fails: impl Fn(&[Op]) -> bool) -> Vec<Op> {
    let mut cur = ops.to_vec();
    let mut chunk = (cur.len() / 2).max(1);
    loop {
        let mut i = 0;
        while i < cur.len() {
            let mut cand = cur.clone();
            let end = (i + chunk).min(cand.len());
            cand.drain(i..end);
            if fails(&cand) {
                cur = cand;
            } else {
                i += chunk;
            }
        }
        if chunk == 1 {
            return cur;
        }
        chunk /= 2;
    }
}
