//! A certified liquidation deficit must stay liquidatable when the backing behind the
//! account's existing claims lapses.
//!
//! Liquidation re-values the account against the source domains it holds claims in and
//! fails closed (`Stale`) on a bucket that is still Fresh at or past its expiry slot; only
//! the refresh continuation expires such a bucket. The classifier used to report the
//! account as liquidation work as long as its certificate was current, and liquidation
//! outranks refresh, so the crank returned `Stale` on every call until some unrelated
//! epoch bump made the certificate stale. A market with no price or funding movement
//! could not liquidate the account at all.
//!
//! Such a bucket now turns the certified deficit into refresh work first: one expiry
//! step per lapsed bucket, one recertification, then liquidation. Buckets liquidation
//! does not read (a leg's latent favorable domain while nothing is pending, the leg's own
//! side, other assets) do not defer it.
//!
//! Domain `d` is `(asset d / 2, Long)` when `d` is even and `(asset d / 2, Short)`
//! when odd. A long's gain is owed by `(asset, Short)`.

use percolator::*;

const MARKET_GROUP: [u8; 32] = [1; 32];
const OWNER: [u8; 32] = [3; 32];

fn portfolio(seed: u8) -> PortfolioAccountV16Account {
    let provenance = ProvenanceHeaderV16Account::from_runtime(&ProvenanceHeaderV16::new(
        MARKET_GROUP,
        [seed; 32],
        OWNER,
    ));
    let mut account = PortfolioAccountV16Account::default();
    account.init_empty_in_place(provenance).unwrap();
    account
}

fn two<T>(items: &mut [T], a: usize, b: usize) -> (&mut T, &mut T) {
    assert_ne!(a, b);
    if a < b {
        let (lo, hi) = items.split_at_mut(b);
        (&mut lo[a], &mut hi[0])
    } else {
        let (lo, hi) = items.split_at_mut(a);
        (&mut hi[0], &mut lo[b])
    }
}

/// Engine state plus portfolios. Every engine call goes through `exec`, which
/// restores the prior state when the call fails, as the SVM transaction rollback
/// the `*_not_atomic` API relies on would.
#[derive(Clone)]
struct World {
    header: MarketGroupV16HeaderAccount,
    markets: Vec<Market<u64>>,
    accounts: Vec<PortfolioAccountV16Account>,
}

type Crank = (AutoCrankPlanV16, AutoCrankOutcomeV16);

const REFRESHED: AutoCrankOutcomeV16 =
    AutoCrankOutcomeV16::Progressed(PermissionlessProgressOutcomeV16::AccountCurrent);

fn expired(domain: usize) -> AutoCrankOutcomeV16 {
    AutoCrankOutcomeV16::Progressed(PermissionlessProgressOutcomeV16::SourceBackingExpired {
        domain,
    })
}

fn refresh_plan(asset: usize) -> AutoCrankPlanV16 {
    AutoCrankPlanV16::RefreshAccount {
        asset_index: Some(asset),
    }
}

impl World {
    /// `asset_slots` assets activated at `price` under the stock public configuration
    /// (`h_max = 10`, so engine-created backing lives 10 slots).
    fn new(
        asset_slots: u32,
        price: u64,
        portfolios: usize,
        tweak: impl FnOnce(&mut V16Config),
    ) -> Self {
        let max_assets = asset_slots.min(V16_MAX_PORTFOLIO_ASSETS_N as u32) as u16;
        let mut config =
            V16Config::public_user_fund_with_market_slots(max_assets, asset_slots, 0, 10);
        tweak(&mut config);
        let mut header =
            MarketGroupV16HeaderAccount::new_dynamic(MARKET_GROUP, config, asset_slots, 0).unwrap();
        let mut markets: Vec<Market<u64>> = (0..asset_slots)
            .map(|i| Market::new(u64::from(i), EngineAssetSlotV16Account::default()))
            .collect();
        for (i, market) in markets.iter_mut().enumerate() {
            header
                .activate_empty_asset_slot_not_atomic(
                    i as u32,
                    &mut market.engine,
                    price,
                    i as u64 + 1,
                )
                .unwrap();
        }
        let accounts = (0..portfolios).map(|i| portfolio(10 + i as u8)).collect();
        let mut world = Self {
            header,
            markets,
            accounts,
        };
        world.catch_up();
        world
    }

    /// 10% initial and maintenance margin, at most 5% price move per slot.
    fn leveraged(asset_slots: u32, portfolios: usize) -> Self {
        Self::new(asset_slots, 100, portfolios, |cfg| {
            cfg.initial_margin_bps = 1_000;
            cfg.maintenance_margin_bps = 1_000;
            cfg.max_price_move_bps_per_slot = 500;
        })
    }

    fn exec<R>(
        &mut self,
        f: impl FnOnce(
            &mut MarketGroupV16ViewMut<'_, u64>,
            &mut [PortfolioAccountV16Account],
        ) -> V16Result<R>,
    ) -> V16Result<R> {
        let snapshot = self.clone();
        let out = {
            let mut market = MarketGroupV16ViewMut::new(&mut self.header, &mut self.markets);
            f(&mut market, &mut self.accounts)
        };
        if out.is_err() {
            *self = snapshot;
        }
        out
    }

    fn on<R>(
        &mut self,
        i: usize,
        f: impl FnOnce(
            &mut MarketGroupV16ViewMut<'_, u64>,
            &mut PortfolioV16ViewMut<'_>,
        ) -> V16Result<R>,
    ) -> V16Result<R> {
        self.exec(|market, accounts| f(market, &mut PortfolioV16ViewMut::new(&mut accounts[i])))
    }

    fn slot(&self) -> u64 {
        self.header.current_slot.get()
    }

    fn asset(&self, i: usize) -> AssetStateV16 {
        self.markets[i].engine.asset.try_to_runtime().unwrap()
    }

    fn price(&self, asset: usize) -> u64 {
        self.asset(asset).effective_price
    }

    fn bucket(&self, domain: usize) -> BackingBucketV16 {
        let slot = &self.markets[domain / 2].engine;
        if domain % 2 == 0 {
            slot.backing_long.try_to_runtime().unwrap()
        } else {
            slot.backing_short.try_to_runtime().unwrap()
        }
    }

    fn source_credit(&self, domain: usize) -> SourceCreditStateV16 {
        let slot = &self.markets[domain / 2].engine;
        if domain % 2 == 0 {
            slot.source_credit_long.try_to_runtime().unwrap()
        } else {
            slot.source_credit_short.try_to_runtime().unwrap()
        }
    }

    /// Lapsed: still marked Fresh at or past its expiry slot.
    fn lapsed(&self, domain: usize) -> bool {
        let bucket = self.bucket(domain);
        bucket.status == BackingBucketStatusV16::Fresh && bucket.expiry_slot <= self.slot()
    }

    fn fresh_backing_atoms(&self) -> u128 {
        self.header.source_fresh_backing_total_num.get() / BOUND_SCALE
    }

    fn cert(&self, i: usize) -> HealthCertV16 {
        self.accounts[i].health_cert.try_to_runtime().unwrap()
    }

    fn pnl(&self, i: usize) -> i128 {
        self.accounts[i].pnl.get()
    }

    fn capital(&self, i: usize) -> u128 {
        self.accounts[i].capital.get()
    }

    /// Claim atoms account `i` holds in `domain`, if its roster names the domain.
    fn claim(&self, i: usize, domain: usize) -> Option<u128> {
        self.accounts[i]
            .source_domains
            .iter()
            .find(|s| s.source_claim_bound_num.get() != 0 && s.domain.get() as usize == domain)
            .map(|s| s.source_claim_bound_num.get() / BOUND_SCALE)
    }

    fn deposit(&mut self, i: usize, amount: u128) {
        self.on(i, |m, a| m.deposit_not_atomic(a, amount)).unwrap();
    }

    /// `long` buys `units` of `asset` from `short` at the asset's current price.
    fn trade(&mut self, long: usize, short: usize, asset: usize, units: u128) -> V16Result<()> {
        let price = self.price(asset);
        self.exec(|market, accounts| {
            let (l, s) = two(accounts, long, short);
            market
                .execute_trade_with_fee_loss_stale_scoped_not_atomic(
                    &mut PortfolioV16ViewMut::new(l),
                    &mut PortfolioV16ViewMut::new(s),
                    TradeRequestV16 {
                        asset_index: asset,
                        size_q: (units * POS_SCALE) as i128,
                        exec_price: price,
                        fee_bps: 0,
                    },
                )
                .map(|_| ())
        })
    }

    /// Owner-path full refresh: never expires backing, fails closed instead.
    fn refresh(&mut self, i: usize) -> V16Result<HealthCertV16> {
        self.on(i, |m, a| m.full_account_refresh_not_atomic(a))
    }

    fn liquidate(&mut self, i: usize, asset: usize) -> V16Result<()> {
        self.on(i, |m, a| {
            m.liquidate_account_not_atomic(a, LiquidationRequestV16 { asset_index: asset })
                .map(|_| ())
        })
    }

    fn summary_at(&self, i: usize, now_slot: u64) -> ActionableSummaryV16 {
        let mut world = self.clone();
        let market = MarketGroupV16ViewMut::new(&mut world.header, &mut world.markets);
        market
            .build_actionable_summary_at_slot(&PortfolioV16View::new(&world.accounts[i]), now_slot)
            .unwrap()
    }

    /// The single public permissionless crank at authenticated slot `now_slot`, with one
    /// observation per listed asset at its committed price (the wrapper accrues every
    /// hinted asset before it calls the engine, so the observation never moves the mark).
    fn crank_with(&mut self, i: usize, now_slot: u64, observed: &[usize]) -> V16Result<Crank> {
        let observations: Vec<AutoCrankObservationV16> = observed
            .iter()
            .map(|&asset_index| AutoCrankObservationV16 {
                asset_index,
                effective_price: self.price(asset_index),
                funding_rate_e9: 0,
            })
            .collect();
        self.on(i, |m, a| {
            m.permissionless_auto_crank_not_atomic(
                a,
                AutoCrankWorkV16 {
                    now_slot,
                    observations: &observations,
                    resolved_close_fee_rate_per_slot: 0,
                },
            )
        })
        .map(|result| (result.selected, result.outcome))
    }

    /// Crank at the committed slot with an observation for every asset.
    fn crank(&mut self, i: usize) -> V16Result<Crank> {
        let assets: Vec<usize> = (0..self.markets.len()).collect();
        self.crank_with(i, self.slot(), &assets)
    }

    /// Bring every asset's accrual up to the committed slot at its current price.
    fn catch_up(&mut self) {
        let now = self.slot();
        for asset in 0..self.markets.len() {
            let price = self.price(asset);
            while self.asset(asset).slot_last < now {
                self.exec(|m, _| {
                    m.accrue_asset_to_not_atomic(asset, now, price, 0, true)
                        .map(|_| ())
                })
                .unwrap();
            }
        }
    }

    /// Advance every asset one slot; listed assets move to the given price.
    fn step(&mut self, moves: &[(usize, u64)]) {
        self.catch_up();
        let next = self.slot() + 1;
        let assets = self.markets.len();
        self.exec(|m, _| {
            if !moves.is_empty() {
                m.set_asset_raw_oracle_targets_not_atomic(moves)?;
            }
            for asset in 0..assets {
                let price = moves
                    .iter()
                    .find(|(moved, _)| *moved == asset)
                    .map_or(m.markets[asset].engine.asset.effective_price.get(), |m| m.1);
                m.accrue_asset_to_not_atomic(asset, next, price, 0, true)?;
            }
            Ok(())
        })
        .unwrap();
    }

    /// Advance the clock to `to_slot` at unchanged prices.
    fn tick_to(&mut self, to_slot: u64) {
        while self.slot() < to_slot {
            self.step(&[]);
        }
    }

    /// External backing for `domain` from its provider, lapsing at `expiry_slot`.
    fn provide_backing(&mut self, domain: usize, amount: u128, expiry_slot: u64) -> V16Result<()> {
        self.exec(|m, _| {
            m.deposit_fresh_counterparty_backing_not_atomic(domain, amount, expiry_slot)
        })
    }

    /// Every quote atom is in exactly one class and the header aggregates match the slots.
    fn assert_conserved(&self) {
        let mut world = self.clone();
        MarketGroupV16ViewMut::new(&mut world.header, &mut world.markets)
            .validate_shape()
            .unwrap();
        let h = &self.header;
        let senior = h.c_tot.get()
            + h.insurance.get()
            + h.backing_provider_earnings_total.get()
            + self.fresh_backing_atoms();
        assert!(h.vault.get() >= senior, "vault below the senior stack");
        let capital: u128 = self.accounts.iter().map(|a| a.capital.get()).sum();
        assert_eq!(h.c_tot.get(), capital, "c_tot drifted from account capital");
        let domains = self.markets.len() * 2;
        let fresh: u128 = (0..domains)
            .map(|d| {
                let b = self.bucket(d);
                assert_eq!(
                    self.source_credit(d).fresh_reserved_backing_num,
                    if b.status == BackingBucketStatusV16::Fresh {
                        b.fresh_unliened_backing_num + b.valid_liened_backing_num
                    } else {
                        0
                    },
                    "domain {d} fresh reserved backing is not its Fresh bucket"
                );
                self.source_credit(d).fresh_reserved_backing_num
            })
            .sum();
        assert_eq!(h.source_fresh_backing_total_num.get(), fresh);
        let claims: u128 = (0..domains)
            .map(|d| self.source_credit(d).positive_claim_bound_num)
            .sum();
        let account_claims: u128 = self
            .accounts
            .iter()
            .flat_map(|a| a.source_domains.iter())
            .map(|s| s.source_claim_bound_num.get())
            .sum();
        assert_eq!(
            claims, account_claims,
            "domain claims are not account claims"
        );
    }
}

/// The funded stocks that neither an expiry nor a fee-free liquidation may move.
#[derive(Debug, PartialEq, Eq)]
struct Stocks {
    vault: u128,
    c_tot: u128,
    insurance: u128,
    earnings: u128,
}

impl World {
    fn stocks(&self) -> Stocks {
        Stocks {
            vault: self.header.vault.get(),
            c_tot: self.header.c_tot.get(),
            insurance: self.header.insurance.get(),
            earnings: self.header.backing_provider_earnings_total.get(),
        }
    }
}

const NOTHING: Crank = (AutoCrankPlanV16::NoAction, AutoCrankOutcomeV16::NoAction);

fn is_liquidation(crank: Crank) -> bool {
    matches!(crank.0, AutoCrankPlanV16::Liquidate { .. }) && crank.1 == REFRESHED
}

/// W (0) holds two legs under 10% margin: long 10 of asset 0 against L (1) and long 10
/// of asset 1 against M (2). Asset 0 gains 5%, asset 1 loses 5% and W and L settle; then
/// asset 1 loses 4 more and W is refreshed. W's certificate is current and shows a
/// deficit of 36 (equity 160 against 196). W holds a claim of 10 in domain 1, backed in
/// full by the bucket L's loss funded. Returns the world and that bucket's expiry slot.
fn certified_deficit_with_backed_claim() -> (World, u64) {
    let mut w = World::leveraged(2, 4);
    w.deposit(0, 200);
    w.deposit(1, 1_000);
    w.deposit(2, 1_000);
    w.trade(0, 1, 0, 10).unwrap();
    w.trade(0, 2, 1, 10).unwrap();
    w.step(&[(0, 105), (1, 95)]);
    w.crank(1).unwrap();
    w.crank(0).unwrap();
    w.step(&[(1, 91)]);
    assert_eq!(w.crank(0).unwrap(), (refresh_plan(0), REFRESHED));
    let cert = w.cert(0);
    assert_eq!(
        (cert.certified_equity, cert.certified_maintenance_req),
        (160, 196)
    );
    assert_eq!(cert.certified_liq_deficit, 36);
    assert_eq!((w.capital(0), w.pnl(0), w.claim(0, 1)), (150, 10, Some(10)));
    assert_eq!(w.source_credit(1).credit_rate_num, CREDIT_RATE_SCALE);
    let expiry = w.bucket(1).expiry_slot;
    (w, expiry)
}

/// The defect: nothing moves after the bucket behind W's claim lapses, so W's certificate
/// stays current. On every such slot the crank used to select liquidation and revert with
/// `Stale`. It now takes three cranks in one slot: expire, recertify, liquidate.
#[test]
fn certified_deficit_is_liquidated_after_claim_backing_lapses() {
    let (mut w, expiry) = certified_deficit_with_backed_claim();
    assert!(w.summary_at(0, w.slot()).liquidatable);
    w.tick_to(expiry);
    for _quiet_slot in 0..4 {
        let mut now = w.clone();
        let slot = now.slot();
        assert!(now.lapsed(1));
        let summary = now.summary_at(0, slot);
        let stocks = now.stocks();

        assert_eq!(now.crank(0).unwrap(), (refresh_plan(0), expired(1)));
        assert!(summary.stale && !summary.liquidatable);
        assert_eq!(now.stocks(), stocks);
        assert_eq!(now.crank(0).unwrap(), (refresh_plan(0), REFRESHED));
        // The claim is no longer backed: equity is capital alone.
        let cert = now.cert(0);
        assert_eq!(
            (cert.certified_equity, cert.certified_liq_deficit),
            (150, 46)
        );
        assert!(now.summary_at(0, slot).liquidatable);
        assert!(is_liquidation(now.crank(0).unwrap()));
        let cert = now.cert(0);
        assert_eq!(
            (cert.certified_equity, cert.certified_maintenance_req),
            (150, 150)
        );
        assert_eq!(cert.certified_liq_deficit, 0);
        assert_eq!(now.slot(), slot);
        assert_eq!(now.stocks(), stocks);
        // The bucket on W's own side of asset 1 lapsed at the same slot; W's cranks do
        // not read it and leave it alone.
        assert!(now.lapsed(2));
        now.assert_conserved();

        w.step(&[]);
    }
}

/// The deferral is bounded: one expiry step per lapsed bucket the refresh reads, roster
/// domains first and then latent ones, one recertification, then liquidation, all in the
/// slot the keeper arrives. W (0) is long three assets against L (1), holds claims in
/// domains 1 and 3, and has no claim in domain 5 (asset 2 only lost).
#[test]
fn liquidation_follows_one_expiry_per_lapsed_bucket_and_one_refresh() {
    let mut w = World::leveraged(3, 2);
    w.deposit(0, 300);
    w.deposit(1, 5_000);
    for asset in 0..3 {
        w.trade(0, 1, asset, 10).unwrap();
    }
    w.step(&[(2, 95)]);
    w.crank(0).unwrap();
    w.step(&[(0, 101), (1, 101)]);
    w.crank(1).unwrap();
    w.crank(0).unwrap();
    assert_eq!(
        [w.claim(0, 1), w.claim(0, 3), w.claim(0, 5)],
        [Some(10), Some(10), None]
    );
    let expiry = w.bucket(1).expiry_slot;
    assert_eq!(w.bucket(3).expiry_slot, expiry);
    // A provider leaves backing in domain 5 that lapses with them.
    w.provide_backing(5, 7, expiry).unwrap();
    assert_eq!(w.crank(0).unwrap(), (refresh_plan(0), REFRESHED));
    let cert = w.cert(0);
    assert_eq!(
        (cert.certified_equity, cert.certified_liq_deficit),
        (270, 27)
    );
    w.tick_to(expiry);
    assert_eq!([w.lapsed(1), w.lapsed(3), w.lapsed(5)], [true; 3]);
    // W's own side of asset 2 (its loss went there) lapsed one slot earlier.
    assert!(w.lapsed(4));
    let slot = w.slot();
    let stocks = w.stocks();

    let mut cranks = Vec::new();
    while cranks.last().map_or(true, |crank| !is_liquidation(*crank)) {
        assert!(cranks.len() < 5, "liquidation not reached: {cranks:?}");
        cranks.push(w.crank(0).unwrap());
    }
    assert_eq!(
        cranks[..4],
        [
            (refresh_plan(0), expired(1)),
            (refresh_plan(0), expired(3)),
            (refresh_plan(0), expired(5)),
            (refresh_plan(0), REFRESHED),
        ]
    );
    assert_eq!(cranks.len(), 5);
    assert_eq!(w.slot(), slot);
    assert_eq!(w.cert(0).certified_liq_deficit, 0);
    assert_eq!(w.stocks(), stocks);
    assert!(w.lapsed(4));
    w.assert_conserved();
}

/// Control for the other direction: liquidation is not deferred by a lapsed bucket it
/// does not read. With the bucket behind W's claim still live, lapsed buckets in the
/// latent domain of W's other leg (3) and on W's own side of asset 0 (0) change nothing:
/// the first crank liquidates.
#[test]
fn liquidation_is_not_deferred_by_buckets_it_does_not_read() {
    let (mut w, expiry) = certified_deficit_with_backed_claim();

    // Nothing lapsed at all.
    let mut untouched = w.clone();
    assert!(is_liquidation(untouched.crank(0).unwrap()));

    let early = w.slot() + 2;
    assert!(early < expiry);
    w.provide_backing(3, 9, early).unwrap();
    w.provide_backing(0, 11, early).unwrap();
    // The top-ups moved the risk epoch; recertify W, which changes nothing else.
    assert_eq!(w.crank(0).unwrap(), (refresh_plan(0), REFRESHED));
    assert_eq!(w.cert(0).certified_liq_deficit, 36);
    w.tick_to(early);
    assert_eq!([w.lapsed(0), w.lapsed(1), w.lapsed(3)], [true, false, true]);
    let summary = w.summary_at(0, w.slot());
    assert!(summary.liquidatable && summary.stale);

    assert!(is_liquidation(w.crank(0).unwrap()));
    assert_eq!(w.cert(0).certified_liq_deficit, 0);
    // The latent bucket is refresh work afterwards; the own-side one is not W's to expire.
    assert_eq!(w.crank(0).unwrap(), (refresh_plan(0), expired(3)));
    assert_eq!(w.crank(0).unwrap(), (refresh_plan(0), REFRESHED));
    assert_eq!(w.crank(0).unwrap(), NOTHING);
    assert_eq!(
        [w.lapsed(0), w.lapsed(1), w.lapsed(3)],
        [true, false, false]
    );
    w.assert_conserved();
}

/// The lapse is judged at the authenticated slot, like every other clock-driven
/// classification. Here the committed clock still lags the bucket's expiry: liquidation
/// used to run against the committed clock and count the lapsed backing (deficit 36);
/// it now waits for the expiry and sizes against the true deficit (46).
#[test]
fn claim_backing_lapse_is_judged_at_the_authenticated_slot() {
    let (mut w, expiry) = certified_deficit_with_backed_claim();
    let committed = w.slot();
    assert!(committed < expiry);
    assert!(is_liquidation(
        w.clone().crank_with(0, expiry - 1, &[0, 1]).unwrap()
    ));

    assert_eq!(
        w.crank_with(0, expiry, &[0, 1]).unwrap(),
        (refresh_plan(0), expired(1))
    );
    assert_eq!(w.slot(), expiry);
    assert_eq!(
        w.crank_with(0, expiry, &[0, 1]).unwrap(),
        (refresh_plan(0), REFRESHED)
    );
    assert_eq!(w.cert(0).certified_liq_deficit, 46);
    assert!(is_liquidation(w.crank_with(0, expiry, &[0, 1]).unwrap()));
    assert_eq!(w.cert(0).certified_liq_deficit, 0);
    w.assert_conserved();
}

/// Control: W (0) also holds a lien on the bucket behind its claim. It opened its second
/// leg on credit with 45 of its 50 claim liened. That leg's loss of 20 nets against the
/// claim (30 left, all of it liened) and leaves a certified deficit of 13. Liquidation
/// reconciles such a lien itself, expiring the bucket on the way, so this account was
/// never stuck. It is still liquidated to the same end state, with the lien retired and
/// the bucket expired; the expiry now runs through the refresh first, which this test
/// deliberately does not pin.
#[test]
fn account_with_a_lien_on_lapsed_claim_backing_is_still_liquidated() {
    let mut w = World::leveraged(2, 4);
    w.deposit(0, 100);
    w.deposit(1, 1_000);
    w.deposit(2, 1_000);
    w.trade(0, 1, 0, 10).unwrap();
    w.step(&[(0, 105)]);
    w.crank(1).unwrap();
    w.crank(0).unwrap();
    w.trade(0, 2, 1, 4).unwrap();
    let lien = w.accounts[0].source_domains[0];
    assert_eq!(
        lien.source_lien_counterparty_backing_num.get(),
        45 * BOUND_SCALE
    );
    w.step(&[(1, 95)]);
    assert_eq!(w.crank(0).unwrap(), (refresh_plan(0), REFRESHED));
    assert_eq!((w.claim(0, 1), w.accounts[0].capital.get()), (Some(30), 100));
    let lien = w.accounts[0].source_domains[0];
    assert_eq!(
        lien.source_lien_counterparty_backing_num.get(),
        30 * BOUND_SCALE
    );
    assert_eq!(w.cert(0).certified_liq_deficit, 13);
    w.tick_to(w.bucket(1).expiry_slot);
    assert!(w.lapsed(1));
    let slot = w.slot();
    let stocks = w.stocks();

    let mut liquidated = false;
    for _ in 0..4 {
        let crank = w.crank(0).unwrap();
        liquidated |= is_liquidation(crank);
        if crank == NOTHING {
            break;
        }
    }
    assert!(liquidated);
    assert_eq!(w.crank(0).unwrap(), NOTHING);
    assert_eq!(w.slot(), slot);
    let cert = w.cert(0);
    assert_eq!(
        (cert.certified_equity, cert.certified_maintenance_req),
        (100, 100)
    );
    assert_eq!(w.bucket(1).status, BackingBucketStatusV16::Expired);
    let lien = w.accounts[0].source_domains[0];
    assert_eq!(lien.source_lien_counterparty_backing_num.get(), 0);
    assert_eq!(w.stocks(), stocks);
    w.assert_conserved();
}

/// Control: only the crank's selection changed. Called directly, liquidation and the
/// owner refresh still fail closed on the lapsed bucket and leave the state untouched.
#[test]
fn direct_liquidation_still_fails_closed_on_lapsed_claim_backing() {
    let (mut w, expiry) = certified_deficit_with_backed_claim();
    w.tick_to(expiry);
    let state = |w: &World| {
        (
            w.header,
            w.markets[0].engine,
            w.markets[1].engine,
            w.accounts.clone(),
        )
    };
    let before = state(&w);

    assert_eq!(w.liquidate(0, 0), Err(V16Error::Stale));
    assert_eq!(w.liquidate(0, 1), Err(V16Error::Stale));
    assert_eq!(w.refresh(0), Err(V16Error::Stale));
    assert!(w.lapsed(1));
    assert!(state(&w) == before);
}

/// xorshift64*: deterministic sequences without a test dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

impl World {
    /// What the gate relies on when it lets a lapsed latent bucket stand: an account
    /// classified as liquidation work has a current certificate, so no leg is behind its
    /// side's K/F cohort and liquidation has no gain to credit to a latent domain.
    fn assert_nothing_pending(&self, i: usize, context: &str) {
        for leg in self.accounts[i].legs.iter() {
            let leg = leg.try_to_runtime().unwrap();
            if !leg.active {
                continue;
            }
            let asset = self.asset(leg.asset_index as usize);
            let cohort = match leg.side {
                SideV16::Long => asset.kf_epoch_long,
                SideV16::Short => asset.kf_epoch_short,
            };
            assert_eq!(
                leg.kf_epoch_snap, cohort,
                "{context}: account {i} is liquidatable with K/F pending on asset {}",
                leg.asset_index
            );
        }
    }

    /// From this state, repeated permissionless cranks on account `i` at the committed
    /// slot never fail and reach "nothing left to do" in bounded steps. Whenever the
    /// classifier reports liquidation work, the crank performs a liquidation.
    fn assert_account_crankable(&self, i: usize, context: &str) {
        let mut w = self.clone();
        // One expiry per roster domain and per leg, a refresh, and at most one
        // liquidation pass per leg with a recertification in between.
        let bound = PORTFOLIO_SOURCE_DOMAIN_CAP + 4 * V16_MAX_PORTFOLIO_ASSETS_N + 8;
        for _ in 0..bound {
            let liquidatable = w.summary_at(i, w.slot()).liquidatable;
            if liquidatable {
                w.assert_nothing_pending(i, context);
            }
            match w.crank(i) {
                Ok(NOTHING) => return,
                Ok(crank) => assert!(
                    !liquidatable || is_liquidation(crank),
                    "{context}: account {i} classified liquidatable but cranked {crank:?}"
                ),
                Err(error) => panic!("{context}: crank on account {i} failed with {error:?}"),
            }
            w.assert_conserved();
        }
        panic!("{context}: account {i} did not settle in {bound} cranks");
    }
}

/// Random Live histories with thin capital, so accounts do become liquidatable: price
/// moves in both directions, idle stretches longer than the backing horizon, provider
/// top-ups with short expiries, trades, cranks in arbitrary account order and owner
/// refreshes. After every step every account must be crankable to completion. Before
/// the fix a history that left a certified deficit standing while the backing behind a
/// claim lapsed failed here with `Stale` (first at seed 6, step 120).
#[test]
fn random_thin_capital_histories_leave_every_account_crankable() {
    const ACCOUNTS: usize = 5;
    for seed in 1..=24u64 {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let mut w = World::leveraged(2, ACCOUNTS);
        for i in 0..ACCOUNTS {
            w.deposit(i, 150 + u128::from(rng.below(400)));
        }
        let _ = w.trade(0, 1, 0, 3);
        let _ = w.trade(2, 3, 1, 3);
        let _ = w.trade(2, 0, 0, 1);
        let _ = w.trade(1, 3, 1, 2);
        let _ = w.trade(4, 1, 0, 2);
        for step in 0..200 {
            let context = format!("seed {seed} step {step}");
            match rng.below(10) {
                0..=2 => {
                    let asset = rng.below(2) as usize;
                    let price = w.price(asset);
                    let delta = rng.below(price / 20 + 1);
                    let next = if rng.below(2) == 0 {
                        (price + delta).min(170)
                    } else {
                        (price - delta).max(50)
                    };
                    w.step(&[(asset, next)]);
                }
                3 => {
                    let to = w.slot() + 1 + rng.below(13);
                    w.tick_to(to);
                }
                4..=5 => {
                    let i = rng.below(ACCOUNTS as u64) as usize;
                    if let Err(error) = w.crank(i) {
                        panic!("{context}: crank on account {i} failed with {error:?}");
                    }
                }
                6..=7 => {
                    let domain = rng.below(4) as usize;
                    let amount = 1 + u128::from(rng.below(40));
                    let expiry = w.slot() + 1 + rng.below(12);
                    let _ = w.provide_backing(domain, amount, expiry);
                }
                8 => {
                    let long = rng.below(ACCOUNTS as u64) as usize;
                    let short = (long + 1 + rng.below(ACCOUNTS as u64 - 1) as usize) % ACCOUNTS;
                    let _ = w.trade(long, short, rng.below(2) as usize, 1);
                }
                _ => {
                    let _ = w.refresh(rng.below(ACCOUNTS as u64) as usize);
                }
            }
            assert_eq!(w.header.mode, 0, "{context}: market left Live");
            w.assert_conserved();
            for i in 0..ACCOUNTS {
                w.assert_account_crankable(i, &context);
            }
        }
    }
}
