//! Regression for percolator#243: in Live mode a backing bucket that has reached its
//! expiry slot but is still marked Fresh must be expirable through the permissionless
//! crank of any account whose refresh reads it, including an account that does not
//! hold a claim in that source domain yet.
//!
//! A favorable K/F delta on an active leg is credited to the leg's source domain
//! `(asset, opposite side)` during refresh, and the refresh then reads that domain's
//! credit rate. Before the fix the crank only expired buckets of domains already in
//! the account's source roster, so a winner about to receive its first claim in a
//! lapsed domain failed the ledger-currency check with `Stale` on every crank. It could
//! not be refreshed, liquidated, reduced or traded, and its unsettled K/F cohort kept
//! the asset closed to new risk, until a loser booked a new loss into the domain.
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

    /// Vault value owed to no senior claimant, i.e. the junior pool.
    fn residual(&self) -> u128 {
        let h = &self.header;
        h.vault.get()
            - h.c_tot.get()
            - h.insurance.get()
            - h.backing_provider_earnings_total.get()
            - self.fresh_backing_atoms()
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

    fn position_q(&self, i: usize, asset: usize) -> i128 {
        self.accounts[i]
            .legs
            .iter()
            .filter_map(|leg| leg.try_to_runtime().ok())
            .find(|leg| leg.active && leg.asset_index as usize == asset)
            .map_or(0, |leg| match leg.side {
                SideV16::Long => leg.basis_pos_q,
                SideV16::Short => -leg.basis_pos_q.abs(),
            })
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

    fn reduce(&mut self, i: usize, asset: usize, units: u128) -> V16Result<()> {
        self.on(i, |m, a| {
            m.rebalance_reduce_position_not_atomic(
                a,
                RebalanceRequestV16 {
                    asset_index: asset,
                    reduce_q: units * POS_SCALE,
                },
            )
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

/// The funded stocks a pure expiry may not move. The lapsed principal only changes
/// class: it leaves fresh backing for the junior pool (`residual`).
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

/// W (0) is long 2 against L (1) at 100. The mark moves to 110 and only L is cranked:
/// its 20 loss leaves its capital and becomes Fresh backing on domain 1, expiring one
/// freshness horizon later. W is not touched until then, so its 20 gain is still an
/// unsettled K/F delta and domain 1 is not in its roster.
fn winner_unsettled_past_loser_backing_expiry() -> World {
    let mut w = World::new(1, 100, 4, |_| {});
    w.deposit(0, 1_000);
    w.deposit(1, 1_000);
    w.trade(0, 1, 0, 2).unwrap();
    w.step(&[(0, 110)]);
    assert_eq!(w.crank(1).unwrap(), (refresh_plan(0), REFRESHED));
    assert_eq!(w.capital(1), 980);
    let bucket = w.bucket(1);
    assert_eq!(bucket.status, BackingBucketStatusV16::Fresh);
    assert_eq!(bucket.fresh_unliened_backing_num, 20 * BOUND_SCALE);
    assert_eq!(bucket.expiry_slot, w.slot() + 10);
    w.tick_to(bucket.expiry_slot);
    assert!(w.lapsed(1));
    assert_eq!(w.pnl(0), 0);
    assert_eq!(w.claim(0, 1), None);
    w
}

#[test]
fn winner_first_claim_on_lapsed_backing_is_crankable() {
    let mut w = winner_unsettled_past_loser_backing_expiry();
    let before = w.stocks();
    let residual_before = w.residual();

    // One crank commits exactly the canonical expiry and returns before valuation.
    assert_eq!(w.crank(0).unwrap(), (refresh_plan(0), expired(1)));
    let bucket = w.bucket(1);
    assert_eq!(bucket.status, BackingBucketStatusV16::Expired);
    assert_eq!(bucket.fresh_unliened_backing_num, 0);
    assert_eq!(w.source_credit(1).fresh_reserved_backing_num, 0);
    // Pure expiry: the lapsed principal moves to the junior pool, nothing else moves.
    assert_eq!(w.stocks(), before);
    assert_eq!(w.residual(), residual_before + 20);
    assert_eq!((w.capital(0), w.pnl(0), w.claim(0, 1)), (1_000, 0, None));
    w.assert_conserved();

    // The next crank settles the gain against the now canonical bucket: the claim is
    // recorded, and with no live backing it adds nothing to equity.
    assert_eq!(w.crank(0).unwrap(), (refresh_plan(0), REFRESHED));
    assert_eq!(
        (w.capital(0), w.pnl(0), w.claim(0, 1)),
        (1_000, 20, Some(20))
    );
    assert_eq!(w.source_credit(1).credit_rate_num, 0);
    let cert = w.cert(0);
    assert!(cert.valid);
    assert_eq!(cert.certified_equity, 1_000);
    assert_eq!(cert.certified_liq_deficit, 0);
    assert_eq!(w.stocks(), before);
    w.assert_conserved();
    assert_eq!(
        w.crank(0).unwrap(),
        (AutoCrankPlanV16::NoAction, AutoCrankOutcomeV16::NoAction)
    );

    // The asset is open again: the owner paths work and unrelated accounts can trade.
    w.refresh(0).unwrap();
    w.deposit(2, 1_000);
    w.deposit(3, 1_000);
    w.trade(2, 3, 0, 1).unwrap();
    w.trade(1, 0, 0, 2).unwrap();
    assert_eq!((w.position_q(0, 0), w.position_q(1, 0)), (0, 0));
    w.assert_conserved();
}

/// Control: the scan never expires a bucket before its expiry slot. One slot earlier the
/// same crank settles the gain against live backing; the lapse is then handled through
/// the roster, as before the fix.
#[test]
fn unexpired_latent_backing_is_left_alone() {
    let mut w = winner_unsettled_past_loser_backing_expiry_minus(1);
    let before = w.stocks();
    assert!(!w.lapsed(1));
    assert_eq!(w.crank(0).unwrap(), (refresh_plan(0), REFRESHED));
    assert_eq!(w.bucket(1).status, BackingBucketStatusV16::Fresh);
    assert_eq!(w.bucket(1).fresh_unliened_backing_num, 20 * BOUND_SCALE);
    assert_eq!((w.pnl(0), w.claim(0, 1)), (20, Some(20)));
    assert_eq!(w.cert(0).certified_equity, 1_020);
    assert_eq!(w.stocks(), before);

    w.step(&[]);
    assert!(w.lapsed(1));
    assert_eq!(w.crank(0).unwrap(), (refresh_plan(0), expired(1)));
    assert_eq!(w.crank(0).unwrap(), (refresh_plan(0), REFRESHED));
    assert_eq!(w.cert(0).certified_equity, 1_000);
    assert_eq!(w.stocks(), before);
    w.assert_conserved();
}

fn winner_unsettled_past_loser_backing_expiry_minus(slots: u64) -> World {
    let mut w = World::new(1, 100, 4, |_| {});
    w.deposit(0, 1_000);
    w.deposit(1, 1_000);
    w.trade(0, 1, 0, 2).unwrap();
    w.step(&[(0, 110)]);
    w.crank(1).unwrap();
    w.tick_to(w.bucket(1).expiry_slot - slots);
    w
}

/// The dangerous part: W (0) holds two legs under 10% margin. Its long on asset 0 has an
/// unsettled +50 against a lapsed bucket; its long on asset 1 then loses 50, leaving
/// equity 150 against a 200 maintenance requirement. Before the fix every crank on W
/// reverted with `Stale`, so it could not be liquidated while asset 1 kept falling.
#[test]
fn lapsed_latent_backing_does_not_block_liquidation() {
    let mut w = World::leveraged(2, 4);
    w.deposit(0, 200);
    w.deposit(1, 1_000);
    w.deposit(2, 1_000);
    w.trade(0, 1, 0, 10).unwrap();
    w.trade(0, 2, 1, 10).unwrap();
    w.step(&[(0, 105)]);
    assert_eq!(w.crank(1).unwrap(), (refresh_plan(0), REFRESHED));
    assert_eq!(w.bucket(1).fresh_unliened_backing_num, 50 * BOUND_SCALE);
    w.tick_to(w.bucket(1).expiry_slot);
    w.step(&[(1, 95)]);
    assert!(w.lapsed(1));
    let slot = w.slot();
    let vault = w.header.vault.get();

    // Three cranks in the slot the deficit appears: expire, certify, liquidate.
    assert_eq!(w.crank(0).unwrap(), (refresh_plan(0), expired(1)));
    assert_eq!(w.crank(0).unwrap(), (refresh_plan(0), REFRESHED));
    let cert = w.cert(0);
    assert_eq!(
        (cert.certified_equity, cert.certified_maintenance_req),
        (150, 200)
    );
    assert_eq!(cert.certified_liq_deficit, 50);
    assert!(w.summary_at(0, slot).liquidatable);
    let (plan, outcome) = w.crank(0).unwrap();
    // Liquidation takes the riskiest leg (10 x 105 against 10 x 95).
    assert_eq!(plan, AutoCrankPlanV16::Liquidate { asset_index: 0 });
    assert_eq!(outcome, REFRESHED);
    assert_eq!(w.slot(), slot);
    // The leg is cut back to where equity covers maintenance again, while W is still
    // solvent: its loss came out of its own capital and nobody else's.
    let cert = w.cert(0);
    assert_eq!(
        (cert.certified_equity, cert.certified_maintenance_req),
        (150, 150)
    );
    assert_eq!(cert.certified_liq_deficit, 0);
    assert!(w.position_q(0, 0) > 0 && w.position_q(0, 0) < (10 * POS_SCALE) as i128);
    assert_eq!(w.position_q(0, 1), (10 * POS_SCALE) as i128);
    assert_eq!((w.capital(0), w.pnl(0)), (150, 50));
    assert_eq!(w.header.vault.get(), vault);
    assert_eq!(w.header.negative_pnl_account_count.get(), 0);
    w.assert_conserved();
}

/// Same wedge with provider-funded backing and no loser involved: a provider tops up
/// domain 1 with an expiry of its choosing and leaves it there, the mark holds until the
/// bucket has lapsed and only then moves in W's favor. Whichever side is cranked first,
/// both accounts settle and the final state is the same.
#[test]
fn provider_funded_lapsed_bucket_settles_in_either_crank_order() {
    let mut base = World::new(1, 100, 2, |_| {});
    base.deposit(0, 1_000);
    base.deposit(1, 1_000);
    base.trade(0, 1, 0, 2).unwrap();
    let expiry = base.slot() + 5;
    base.provide_backing(1, 300, expiry).unwrap();
    base.tick_to(expiry);
    base.step(&[(0, 110)]);
    assert!(base.lapsed(1));
    let vault = base.header.vault.get();

    // Winner first: its crank expires the bucket, then settles; the loser's loss then
    // funds a new bucket and the winner's claim becomes fully backed.
    let mut winner_first = base.clone();
    assert_eq!(
        winner_first.crank(0).unwrap(),
        (refresh_plan(0), expired(1))
    );
    assert_eq!(winner_first.crank(0).unwrap(), (refresh_plan(0), REFRESHED));
    assert_eq!(winner_first.cert(0).certified_equity, 1_000);
    assert_eq!(winner_first.crank(1).unwrap(), (refresh_plan(0), REFRESHED));
    assert_eq!(winner_first.crank(0).unwrap(), (refresh_plan(0), REFRESHED));

    // Loser first: its new loss normalizes the lapsed bucket inline (as before the fix).
    let mut loser_first = base.clone();
    assert_eq!(loser_first.crank(1).unwrap(), (refresh_plan(0), REFRESHED));
    assert_eq!(loser_first.crank(0).unwrap(), (refresh_plan(0), REFRESHED));

    for w in [&winner_first, &loser_first] {
        assert_eq!(w.bucket(1).status, BackingBucketStatusV16::Fresh);
        assert_eq!(w.bucket(1).fresh_unliened_backing_num, 20 * BOUND_SCALE);
        assert_eq!(w.bucket(1).expiry_slot, w.slot() + 10);
        assert_eq!((w.capital(1), w.pnl(1)), (980, 0));
        assert_eq!(
            (w.capital(0), w.pnl(0), w.claim(0, 1)),
            (1_000, 20, Some(20))
        );
        assert_eq!(w.cert(0).certified_equity, 1_020);
        // The provider's lapsed 300 was forfeited to the junior pool on expiry.
        assert_eq!(w.residual(), 300);
        assert_eq!(w.header.vault.get(), vault);
        w.assert_conserved();
    }
}

/// Mirror image: a short winner's gain is owed by `(asset, Long)`, the even domain.
#[test]
fn short_winner_first_claim_on_lapsed_backing_is_crankable() {
    let mut w = World::new(1, 100, 2, |_| {});
    w.deposit(0, 1_000);
    w.deposit(1, 1_000);
    w.trade(1, 0, 0, 2).unwrap();
    w.step(&[(0, 90)]);
    assert_eq!(w.crank(1).unwrap(), (refresh_plan(0), REFRESHED));
    assert_eq!(w.bucket(0).fresh_unliened_backing_num, 20 * BOUND_SCALE);
    w.tick_to(w.bucket(0).expiry_slot);
    let before = w.stocks();

    assert_eq!(w.crank(0).unwrap(), (refresh_plan(0), expired(0)));
    assert_eq!(w.bucket(0).status, BackingBucketStatusV16::Expired);
    assert_eq!(w.crank(0).unwrap(), (refresh_plan(0), REFRESHED));
    assert_eq!((w.pnl(0), w.claim(0, 0)), (20, Some(20)));
    assert_eq!(w.stocks(), before);
    w.assert_conserved();
}

/// The loss side cannot be relied on to clear the wedge. A (0) is long asset 0 and short
/// asset 1 against B (1); the buckets of both long legs' favorable domains (1 and 3) have
/// lapsed unreferenced, and both marks rise. Each account's loss would normalize the
/// bucket the other is stuck on, but each refresh also carries its own stuck gain and
/// reverted as a whole, so before the fix neither account could ever be cranked and both
/// assets stayed closed to new risk.
#[test]
fn accounts_that_would_unwedge_each_other_both_settle() {
    let mut w = World::new(2, 100, 4, |_| {});
    w.deposit(0, 1_000);
    w.deposit(1, 1_000);
    w.trade(0, 1, 0, 2).unwrap();
    w.trade(1, 0, 1, 2).unwrap();
    let expiry = w.slot() + 3;
    w.provide_backing(1, 5, expiry).unwrap();
    w.provide_backing(3, 5, expiry).unwrap();
    w.tick_to(expiry);
    w.step(&[(0, 110), (1, 110)]);
    assert_eq!(
        [w.lapsed(0), w.lapsed(1), w.lapsed(2), w.lapsed(3)],
        [false, true, false, true]
    );
    let vault = w.header.vault.get();

    assert_eq!(w.crank(0).unwrap(), (refresh_plan(0), expired(1)));
    assert_eq!(w.crank(1).unwrap(), (refresh_plan(0), expired(3)));
    assert_eq!(w.crank(0).unwrap(), (refresh_plan(0), REFRESHED));
    assert_eq!(w.crank(1).unwrap(), (refresh_plan(0), REFRESHED));
    // Each lost 20 on one leg (paid into the other's domain) and gained 20 on the other.
    assert_eq!((w.capital(0), w.pnl(0), w.claim(0, 1)), (980, 20, Some(20)));
    assert_eq!((w.capital(1), w.pnl(1), w.claim(1, 3)), (980, 20, Some(20)));
    assert_eq!(w.bucket(1).fresh_unliened_backing_num, 20 * BOUND_SCALE);
    assert_eq!(w.bucket(3).fresh_unliened_backing_num, 20 * BOUND_SCALE);
    assert_eq!(w.residual(), 10);
    assert_eq!(w.header.vault.get(), vault);
    w.assert_conserved();

    w.deposit(2, 1_000);
    w.deposit(3, 1_000);
    w.trade(2, 3, 0, 1).unwrap();
    w.trade(2, 3, 1, 1).unwrap();
}

/// A DrainOnly asset still accrues and refreshes through the ordinary route.
#[test]
fn drain_only_leg_first_claim_on_lapsed_backing_is_crankable() {
    let mut w = winner_unsettled_past_loser_backing_expiry();
    w.exec(|m, _| m.mark_asset_drain_only_not_atomic(0))
        .unwrap();
    assert_eq!(w.asset(0).lifecycle, AssetLifecycleV16::DrainOnly);
    let before = w.stocks();

    assert_eq!(w.crank(0).unwrap(), (refresh_plan(0), expired(1)));
    assert_eq!(w.crank(0).unwrap(), (refresh_plan(0), REFRESHED));
    assert_eq!((w.pnl(0), w.claim(0, 1)), (20, Some(20)));
    assert_eq!(w.stocks(), before);
    w.assert_conserved();
}

/// The same wedge on an insolvent account. W (0) has lost more than its capital on
/// asset 1 (capital 0, PnL -10) and is refreshed; then its asset 0 leg turns a gain
/// against a lapsed bucket. With negative PnL the gain goes through the
/// prospective-support read of that domain, which rejected the lapsed bucket: before
/// the fix a bankrupt account could not be liquidated.
#[test]
fn insolvent_account_gain_on_lapsed_latent_backing_is_crankable() {
    let mut w = World::leveraged(2, 4);
    w.deposit(0, 200);
    w.deposit(1, 1_000);
    w.deposit(2, 1_000);
    w.trade(0, 1, 0, 10).unwrap();
    w.trade(0, 2, 1, 10).unwrap();
    let expiry = w.slot() + 6;
    w.provide_backing(1, 40, expiry).unwrap();
    for price in [95, 91, 87, 83, 79] {
        w.step(&[(1, price)]);
    }
    assert_eq!(w.crank(0).unwrap(), (refresh_plan(0), REFRESHED));
    assert_eq!((w.capital(0), w.pnl(0)), (0, -10));
    w.step(&[(0, 105)]);
    assert_eq!(w.slot(), expiry);
    assert!(w.lapsed(1));
    let vault = w.header.vault.get();

    assert_eq!(w.crank(0).unwrap(), (refresh_plan(0), expired(1)));
    // The unbacked gain cures nothing; the account is certified with its deficit.
    assert_eq!(w.crank(0).unwrap(), (refresh_plan(0), REFRESHED));
    assert_eq!((w.capital(0), w.pnl(0)), (0, -10));
    assert_eq!(w.cert(0).certified_liq_deficit, 10);
    // Liquidation then closes both legs.
    for _ in 0..2 {
        let (plan, outcome) = w.crank(0).unwrap();
        assert!(matches!(plan, AutoCrankPlanV16::Liquidate { .. }));
        assert_eq!(outcome, REFRESHED);
    }
    assert!(active_bitmap_is_empty(
        w.accounts[0].active_bitmap.map(V16PodU64::get)
    ));
    assert_eq!(w.header.vault.get(), vault);
    w.assert_conserved();
}

/// Clock-driven expiry does not wait for a certificate to go stale. W's certificate is
/// current and nothing is pending, yet once the authenticated slot reaches the expiry of
/// the bucket in its leg's favorable domain the account classifies as refresh work, and a
/// crank without any oracle observation performs the expiry.
#[test]
fn lapsed_latent_backing_is_actionable_with_a_current_certificate() {
    let mut w = World::new(1, 100, 2, |_| {});
    w.deposit(0, 1_000);
    w.deposit(1, 1_000);
    w.trade(0, 1, 0, 2).unwrap();
    let expiry = w.slot() + 5;
    w.provide_backing(1, 300, expiry).unwrap();
    assert_eq!(w.crank(0).unwrap(), (refresh_plan(0), REFRESHED));
    let committed = w.slot();
    assert!(!w.summary_at(0, committed).is_actionable());
    assert!(!w.summary_at(0, expiry - 1).is_actionable());
    assert!(w.summary_at(0, expiry).stale);

    let before = w.stocks();
    assert_eq!(
        w.crank_with(0, expiry - 1, &[]).unwrap(),
        (AutoCrankPlanV16::NoAction, AutoCrankOutcomeV16::NoAction)
    );
    assert_eq!(w.slot(), committed);
    assert_eq!(
        w.crank_with(0, expiry, &[]).unwrap(),
        (refresh_plan(0), expired(1))
    );
    assert_eq!(w.slot(), expiry);
    assert_eq!(w.bucket(1).status, BackingBucketStatusV16::Expired);
    assert_eq!(w.stocks(), before);
    assert_eq!(
        w.crank_with(0, expiry, &[]).unwrap(),
        (refresh_plan(0), REFRESHED)
    );
    assert!(!w.summary_at(0, expiry).is_actionable());
    w.assert_conserved();
}

/// Bounded work: one canonical expiry per crank. Roster domains go first (unchanged),
/// then latent domains in leg-slot order, and only then does the refresh settle.
#[test]
fn one_expiry_per_crank_roster_first_then_legs_in_slot_order() {
    let mut w = World::new(3, 100, 2, |_| {});
    w.deposit(0, 10_000);
    w.deposit(1, 10_000);
    for asset in 0..3 {
        w.trade(0, 1, asset, 2).unwrap();
    }
    // Asset 2 moves and both sides settle: W holds a claim in domain 5.
    w.step(&[(2, 110)]);
    w.crank(1).unwrap();
    w.crank(0).unwrap();
    assert_eq!(w.claim(0, 5), Some(20));
    let expiry = w.bucket(5).expiry_slot;
    // Domains 1 and 3 get provider backing that lapses with it; W has no claim there.
    w.provide_backing(1, 7, expiry).unwrap();
    w.provide_backing(3, 9, expiry).unwrap();
    w.tick_to(expiry);
    w.step(&[(0, 110), (1, 110)]);
    assert_eq!([w.lapsed(1), w.lapsed(3), w.lapsed(5)], [true; 3]);
    let before = w.stocks();

    for (domain, still_lapsed) in [(5, [true, true]), (1, [false, true]), (3, [false, false])] {
        assert_eq!(w.crank(0).unwrap(), (refresh_plan(0), expired(domain)));
        assert_eq!([w.lapsed(1), w.lapsed(3)], still_lapsed);
        assert!(!w.lapsed(5));
        assert_eq!((w.claim(0, 1), w.claim(0, 3)), (None, None));
        assert_eq!(w.stocks(), before);
    }
    assert_eq!(w.crank(0).unwrap(), (refresh_plan(0), REFRESHED));
    assert_eq!([w.claim(0, 1), w.claim(0, 3), w.claim(0, 5)], [Some(20); 3]);
    assert_eq!(w.stocks(), before);
    assert_eq!(w.residual(), 20 + 7 + 9);
    w.assert_conserved();
}

/// A leg on a Recovery-lifecycle asset refreshes from committed state. The committed
/// clock here is already past the bucket's expiry (asset 1 kept accruing), so the same
/// first-claim settlement was rejected as on a live asset.
#[test]
fn recovery_leg_first_claim_on_lapsed_backing_is_crankable() {
    let mut w = World::new(2, 100, 2, |_| {});
    w.deposit(0, 1_000);
    w.deposit(1, 1_000);
    w.trade(0, 1, 0, 2).unwrap();
    w.step(&[(0, 110)]);
    w.crank(1).unwrap();
    let now = w.slot();
    w.exec(|m, _| m.force_asset_recovery_not_atomic(0, now))
        .unwrap();
    assert_eq!(w.asset(0).lifecycle, AssetLifecycleV16::Recovery);
    let expiry = w.bucket(1).expiry_slot;
    while w.slot() < expiry {
        let next = w.slot() + 1;
        w.exec(|m, _| {
            m.accrue_asset_to_not_atomic(1, next, 100, 0, true)
                .map(|_| ())
        })
        .unwrap();
    }
    assert!(w.lapsed(1));
    let before = w.stocks();

    assert_eq!(
        w.crank_with(0, expiry, &[]).unwrap(),
        (refresh_plan(0), expired(1))
    );
    assert_eq!(
        w.crank_with(0, expiry, &[]).unwrap(),
        (refresh_plan(0), REFRESHED)
    );
    assert_eq!((w.pnl(0), w.claim(0, 1)), (20, Some(20)));
    assert_eq!(w.cert(0).certified_equity, 1_000);
    assert_eq!(w.stocks(), before);
    w.assert_conserved();
}

/// The committed-state refresh classifies and expires against the authenticated slot.
/// Here the committed clock still lags the bucket's expiry: the latent bucket is expired
/// first, so the gain is never credited against backing that has lapsed in real time.
#[test]
fn recovery_leg_expires_latent_backing_at_the_authenticated_slot() {
    let mut w = World::new(1, 100, 2, |_| {});
    w.deposit(0, 1_000);
    w.deposit(1, 1_000);
    w.trade(0, 1, 0, 2).unwrap();
    w.step(&[(0, 110)]);
    w.crank(1).unwrap();
    let committed = w.slot();
    w.exec(|m, _| m.force_asset_recovery_not_atomic(0, committed))
        .unwrap();
    let expiry = w.bucket(1).expiry_slot;
    assert!(committed < expiry);

    // One slot short of expiry the gain settles against live backing ...
    let mut early = w.clone();
    assert_eq!(
        early.crank_with(0, expiry - 1, &[]).unwrap(),
        (refresh_plan(0), REFRESHED)
    );
    assert_eq!(early.bucket(1).status, BackingBucketStatusV16::Fresh);
    assert_eq!(early.cert(0).certified_equity, 1_020);

    // ... at expiry it does not.
    assert_eq!(
        w.crank_with(0, expiry, &[]).unwrap(),
        (refresh_plan(0), expired(1))
    );
    assert_eq!(w.slot(), committed);
    assert_eq!(w.pnl(0), 0);
    assert_eq!(
        w.crank_with(0, expiry, &[]).unwrap(),
        (refresh_plan(0), REFRESHED)
    );
    assert_eq!((w.pnl(0), w.cert(0).certified_equity), (20, 1_000));
    w.assert_conserved();
}

/// Control: the fix adds discovery to the permissionless crank only. Owner and direct
/// paths still refuse to read a lapsed bucket and leave it for the crank.
#[test]
fn owner_paths_still_fail_closed_on_lapsed_latent_backing() {
    let mut w = winner_unsettled_past_loser_backing_expiry();
    w.deposit(2, 1_000);
    w.deposit(3, 1_000);
    let before = (w.header, w.markets[0].engine, w.accounts.clone());

    assert_eq!(w.refresh(0), Err(V16Error::Stale));
    assert_eq!(w.liquidate(0, 0), Err(V16Error::Stale));
    assert_eq!(w.reduce(0, 0, 1), Err(V16Error::Stale));
    assert_eq!(w.trade(1, 0, 0, 2), Err(V16Error::Stale));
    // The winner's unsettled K/F cohort keeps the asset closed to new risk.
    assert_eq!(w.trade(2, 3, 0, 1), Err(V16Error::LockActive));
    // The provider can neither roll the lapsed bucket forward nor withdraw from it.
    assert!(w.provide_backing(1, 10, w.slot() + 50).is_err());
    assert_eq!(
        w.exec(|m, _| m.withdraw_fresh_counterparty_backing_not_atomic(1, 1)),
        Err(V16Error::Stale)
    );
    assert!(w.lapsed(1));
    assert!((w.header, w.markets[0].engine, w.accounts.clone()) == before);
}

/// Control: only the favorable domain of a leg is latent. A lapsed bucket in the leg's
/// own-side domain is where its losses go; the loss path already normalizes it inline in
/// the same crank, and that is unchanged.
#[test]
fn own_side_lapsed_bucket_is_normalized_by_the_loss_itself() {
    let mut w = World::new(1, 100, 2, |_| {});
    w.deposit(0, 1_000);
    w.deposit(1, 1_000);
    w.trade(0, 1, 0, 2).unwrap();
    let expiry = w.slot() + 5;
    // Domain 0 is (asset 0, Long): the long's own side.
    w.provide_backing(0, 300, expiry).unwrap();
    w.tick_to(expiry);
    w.step(&[(0, 90)]);
    assert!(w.lapsed(0));
    let vault = w.header.vault.get();

    assert_eq!(w.crank(0).unwrap(), (refresh_plan(0), REFRESHED));
    assert_eq!((w.capital(0), w.pnl(0)), (980, 0));
    let bucket = w.bucket(0);
    assert_eq!(bucket.status, BackingBucketStatusV16::Fresh);
    assert_eq!(bucket.fresh_unliened_backing_num, 20 * BOUND_SCALE);
    assert_eq!(bucket.expiry_slot, w.slot() + 10);
    assert_eq!(w.residual(), 300);
    assert_eq!(w.header.vault.get(), vault);
    w.assert_conserved();
}

/// Control: discovery is limited to the domains this account's refresh reads. W (0) is
/// long asset 0 only. Lapsed buckets on its own side (domain 0) and on an asset it has no
/// leg in (domain 3) do not make it actionable and are not expired through it.
#[test]
fn lapsed_buckets_outside_the_accounts_read_set_are_left_alone() {
    let mut w = World::new(2, 100, 2, |_| {});
    w.deposit(0, 1_000);
    w.deposit(1, 1_000);
    w.trade(0, 1, 0, 2).unwrap();
    let expiry = w.slot() + 3;
    w.provide_backing(0, 11, expiry).unwrap();
    w.provide_backing(3, 13, expiry).unwrap();
    assert_eq!(w.crank(0).unwrap(), (refresh_plan(0), REFRESHED));
    w.tick_to(expiry + 2);
    assert_eq!([w.lapsed(0), w.lapsed(3)], [true, true]);
    assert!(!w.summary_at(0, w.slot()).is_actionable());
    assert_eq!(
        w.crank(0).unwrap(),
        (AutoCrankPlanV16::NoAction, AutoCrankOutcomeV16::NoAction)
    );

    // A favorable move settles in one crank against domain 1; the two lapsed buckets
    // are still where they were.
    w.step(&[(0, 110)]);
    assert_eq!(w.crank(0).unwrap(), (refresh_plan(0), REFRESHED));
    assert_eq!((w.pnl(0), w.claim(0, 1)), (20, Some(20)));
    assert_eq!([w.lapsed(0), w.lapsed(3)], [true, true]);
    assert_eq!(w.bucket(0).fresh_unliened_backing_num, 11 * BOUND_SCALE);
    assert_eq!(w.bucket(3).fresh_unliened_backing_num, 13 * BOUND_SCALE);
    w.assert_conserved();
}

/// The expiry reached through a latent domain is the same canonical transition as the
/// roster one: another account's lien on the bucket is impaired, not un-pledged. W0 (0)
/// holds a 100 lien on domain 1. W3 (3) opens its long afterwards and has no claim in the
/// domain; when the bucket lapses it is W3 that gets cranked first.
#[test]
fn latent_expiry_impairs_another_accounts_lien() {
    let mut w = World::new(1, 100, 4, |_| {});
    w.deposit(0, 1_000);
    w.deposit(1, 2_000);
    w.trade(0, 1, 0, 10).unwrap();
    w.step(&[(0, 110)]);
    w.crank(1).unwrap();
    w.crank(0).unwrap();
    // W0 uses its 100 gain as initial margin for one more unit, which liens all of it.
    w.deposit(0, 110);
    w.deposit(2, 110);
    w.trade(0, 2, 0, 1).unwrap();
    assert_eq!(w.bucket(1).valid_liened_backing_num, 100 * BOUND_SCALE);
    assert_eq!(w.bucket(1).fresh_unliened_backing_num, 0);
    w.deposit(3, 1_000);
    w.trade(3, 1, 0, 5).unwrap();
    w.tick_to(w.bucket(1).expiry_slot - 1);
    w.step(&[(0, 120)]);
    assert!(w.lapsed(1));
    assert_eq!((w.pnl(3), w.claim(3, 1)), (0, None));
    let before = w.stocks();
    let residual_before = w.residual();

    assert_eq!(w.crank(3).unwrap(), (refresh_plan(0), expired(1)));
    let bucket = w.bucket(1);
    assert_eq!(bucket.status, BackingBucketStatusV16::Impaired);
    assert_eq!(
        (
            bucket.fresh_unliened_backing_num,
            bucket.valid_liened_backing_num,
            bucket.impaired_liened_backing_num
        ),
        (0, 0, 100 * BOUND_SCALE)
    );
    assert_eq!(
        w.source_credit(1).impaired_liened_backing_num,
        100 * BOUND_SCALE
    );
    assert_eq!(w.stocks(), before);
    assert_eq!(w.residual(), residual_before + 100);

    assert_eq!(w.crank(3).unwrap(), (refresh_plan(0), REFRESHED));
    assert_eq!((w.pnl(3), w.claim(3, 1)), (50, Some(50)));
    assert_eq!(w.cert(3).certified_equity, 1_000);

    // The lien holder's own crank reconciles its lien against the Impaired bucket, which
    // retires the bucket's last impaired lien, and reports its true equity.
    assert_eq!(w.crank(0).unwrap(), (refresh_plan(0), REFRESHED));
    assert_eq!(w.bucket(1).status, BackingBucketStatusV16::Expired);
    assert_eq!(w.source_credit(1).impaired_liened_backing_num, 0);
    assert_eq!((w.capital(0), w.pnl(0)), (1_110, 210));
    assert_eq!(w.cert(0).certified_equity, 1_110);
    assert_eq!(w.stocks(), before);
    w.assert_conserved();
}

/// Control: liquidation keeps its priority. W's certificate is current and shows a
/// deficit when the bucket in its other leg's favorable domain lapses during a quiet
/// stretch. Nothing is pending on that leg, so the liquidation neither reads the bucket
/// nor waits for its expiry.
#[test]
fn liquidation_with_a_current_certificate_does_not_wait_for_latent_expiry() {
    let mut w = World::leveraged(2, 4);
    w.deposit(0, 200);
    w.deposit(1, 1_000);
    w.deposit(2, 1_000);
    w.trade(0, 1, 0, 10).unwrap();
    w.trade(0, 2, 1, 10).unwrap();
    let expiry = w.slot() + 6;
    w.provide_backing(1, 40, expiry).unwrap();
    w.step(&[(1, 95)]);
    assert_eq!(w.crank(0).unwrap(), (refresh_plan(0), REFRESHED));
    assert_eq!(w.cert(0).certified_liq_deficit, 45);
    w.tick_to(expiry);
    assert!(w.lapsed(1));
    assert!(w.summary_at(0, expiry).liquidatable);

    let (plan, outcome) = w.crank(0).unwrap();
    assert!(matches!(plan, AutoCrankPlanV16::Liquidate { .. }));
    assert_eq!(outcome, REFRESHED);
    assert_eq!(w.cert(0).certified_liq_deficit, 0);
    w.assert_conserved();
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
    /// Liveness: from this state, repeated permissionless cranks on account `i` at the
    /// committed slot never fail and reach "nothing left to do" in bounded steps. Each
    /// expiry step is checked to be a pure expiry of a bucket that had really lapsed.
    fn assert_account_crankable(&self, i: usize, context: &str) {
        let mut w = self.clone();
        // At most one expiry per roster domain and per leg, then B chunks, refresh and
        // (never here) liquidation.
        let bound = PORTFOLIO_SOURCE_DOMAIN_CAP + 2 * V16_MAX_PORTFOLIO_ASSETS_N + 8;
        for _ in 0..bound {
            let lapsed_before: Vec<bool> = (0..w.markets.len() * 2).map(|d| w.lapsed(d)).collect();
            let stocks = w.stocks();
            let rates: Vec<u128> = (0..w.markets.len() * 2)
                .map(|d| w.source_credit(d).credit_rate_num)
                .collect();
            let account = w.accounts[i];
            match w.crank(i) {
                Ok((AutoCrankPlanV16::NoAction, _)) => {
                    assert!(
                        w.cert(i).valid,
                        "{context}: idle account {i} is not certified"
                    );
                    return;
                }
                Ok((
                    _,
                    AutoCrankOutcomeV16::Progressed(
                        PermissionlessProgressOutcomeV16::SourceBackingExpired { domain },
                    ),
                )) => {
                    assert!(
                        lapsed_before[domain],
                        "{context}: domain {domain} expired early"
                    );
                    assert_ne!(w.bucket(domain).status, BackingBucketStatusV16::Fresh);
                    assert_eq!(w.stocks(), stocks, "{context}: expiry moved value");
                    assert!(
                        w.source_credit(domain).credit_rate_num <= rates[domain],
                        "{context}: expiry raised the credit rate of domain {domain}"
                    );
                    // The step is the expiry alone: the account is not settled by it.
                    assert_eq!(
                        (w.capital(i), w.pnl(i), w.accounts[i].legs),
                        (account.capital.get(), account.pnl.get(), account.legs),
                        "{context}: expiry step touched account {i}"
                    );
                }
                Ok(_) => {}
                Err(error) => panic!("{context}: crank on account {i} failed with {error:?}"),
            }
            w.assert_conserved();
        }
        panic!("{context}: account {i} did not settle in {bound} cranks");
    }
}

/// Random Live histories over two assets and four well-capitalized accounts: price
/// moves in both directions, idle stretches longer than the backing horizon, provider
/// top-ups with short expiries on every domain, trades, cranks in arbitrary account
/// order and owner refreshes. After every step every account must be crankable to
/// completion and value must be conserved. Before the fix the first history that let a
/// bucket lapse ahead of a winner's first claim failed here with `Stale`.
#[test]
fn random_live_histories_leave_every_account_crankable() {
    const ACCOUNTS: usize = 4;
    for seed in 1..=40u64 {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let mut w = World::leveraged(2, ACCOUNTS);
        for i in 0..ACCOUNTS {
            w.deposit(i, 1_000_000);
        }
        w.trade(0, 1, 0, 3).unwrap();
        w.trade(2, 3, 1, 3).unwrap();
        w.trade(2, 0, 0, 1).unwrap();
        w.trade(1, 3, 1, 2).unwrap();
        for step in 0..160 {
            let context = format!("seed {seed} step {step}");
            match rng.below(10) {
                0..=2 => {
                    let asset = rng.below(2) as usize;
                    let price = w.price(asset);
                    let delta = rng.below(price / 20 + 1);
                    let next = if rng.below(2) == 0 {
                        (price + delta).min(160)
                    } else {
                        (price - delta).max(60)
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
            w.assert_conserved();
            for i in 0..ACCOUNTS {
                w.assert_account_crankable(i, &context);
            }
        }
    }
}
