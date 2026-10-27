//! Regression for percolator#230 (V12/Zellic H-01): a counterparty backing bucket that expires
//! while a trader holds an open-position lien on it must not wedge either side of the asset.
//!
//! Expiry forfeits the liened principal to the junior residual pool and marks the bucket
//! Impaired. The account-local lien is reconciled against that bucket state during the
//! holder's own settlement (spec: liens referencing an impaired bucket are impaired by bucket
//! status and settle through bounded cranks), and a Live loss landing on a still-Impaired bucket
//! is booked to junior support as in Resolved mode.
#![allow(dead_code)]

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
/// restores the prior state when the call fails, as the SVM transaction
/// rollback that the engine relies on would.
#[derive(Clone)]
struct World {
    header: MarketGroupV16HeaderAccount,
    markets: Vec<Market<u64>>,
    accounts: Vec<PortfolioAccountV16Account>,
}

impl World {
    /// `asset_slots` assets activated at `price` under the stock public
    /// configuration `V16Config::public_user_fund_with_market_slots`.
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
        Self {
            header,
            markets,
            accounts,
        }
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

    /// Source domain `d` is (asset d / 2, Long) when d is even, else (asset d / 2, Short).
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

    /// Vault value owed to no senior claimant, i.e. the junior pool.
    fn residual(&self) -> u128 {
        let h = &self.header;
        h.vault.get()
            - h.c_tot.get()
            - h.insurance.get()
            - h.backing_provider_earnings_total.get()
            - h.source_fresh_backing_total_num.get() / BOUND_SCALE
    }

    fn deposit(&mut self, i: usize, amount: u128) {
        self.on(i, |m, a| m.deposit_not_atomic(a, amount)).unwrap();
    }

    fn withdraw(&mut self, i: usize, amount: u128) -> V16Result<()> {
        self.on(i, |m, a| m.withdraw_not_atomic(a, amount))
    }

    /// `long` buys `size_q` base quantity of `asset` from `short` at `price`.
    fn trade_q(
        &mut self,
        long: usize,
        short: usize,
        asset: usize,
        size_q: i128,
        price: u64,
    ) -> V16Result<()> {
        self.exec(|market, accounts| {
            let (l, s) = two(accounts, long, short);
            let request = TradeRequestV16 {
                asset_index: asset,
                size_q,
                exec_price: price,
                fee_bps: 0,
            };
            market
                .execute_trade_with_fee_loss_stale_scoped_not_atomic(
                    &mut PortfolioV16ViewMut::new(l),
                    &mut PortfolioV16ViewMut::new(s),
                    request,
                )
                .map(|_| ())
        })
    }

    fn trade(
        &mut self,
        long: usize,
        short: usize,
        asset: usize,
        units: u128,
        price: u64,
    ) -> V16Result<()> {
        self.trade_q(long, short, asset, (units * POS_SCALE) as i128, price)
    }

    fn refresh(&mut self, i: usize) -> V16Result<HealthCertV16> {
        self.on(i, |m, a| m.full_account_refresh_not_atomic(a))
    }

    fn liquidate(&mut self, i: usize, asset: usize) -> V16Result<()> {
        self.on(i, |m, a| {
            m.liquidate_account_not_atomic(a, LiquidationRequestV16 { asset_index: asset })
                .map(|_| ())
        })
    }

    /// Owner-authorised unilateral reduction.
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

    fn convert(&mut self, i: usize) -> V16Result<u128> {
        self.on(i, |m, a| m.convert_released_pnl_to_capital_not_atomic(a))
    }

    fn close_resolved(&mut self, i: usize) -> V16Result<ResolvedCloseOutcomeV16> {
        self.on(i, |m, a| m.close_resolved_account_not_atomic(a, 0))
    }

    fn claim_topup(&mut self, i: usize) -> V16Result<u128> {
        self.on(i, |m, a| m.claim_resolved_payout_topup_not_atomic(a))
    }

    /// The single public permissionless crank, given one oracle observation.
    fn crank(
        &mut self,
        i: usize,
        now_slot: u64,
        asset: usize,
        price: u64,
    ) -> V16Result<AutoCrankResultV16> {
        self.on(i, |m, a| {
            let observations = [AutoCrankObservationV16 {
                asset_index: asset,
                effective_price: price,
                funding_rate_e9: 0,
            }];
            m.permissionless_auto_crank_not_atomic(
                a,
                AutoCrankWorkV16 {
                    now_slot,
                    observations: &observations,
                    resolved_close_fee_rate_per_slot: 0,
                },
            )
        })
    }

    /// Bring `asset` up to the current slot without moving its price.
    fn catch_up(&mut self, asset: usize) {
        let price = self.asset(asset).effective_price;
        while self.asset(asset).slot_last < self.slot() {
            let now = self.slot();
            self.exec(|m, _| {
                m.accrue_asset_to_not_atomic(asset, now, price, 0, true)
                    .map(|_| ())
            })
            .unwrap();
        }
    }

    /// Advance the clock to `to_slot`, one slot at a time, at `asset`'s current price.
    fn tick_to(&mut self, asset: usize, to_slot: u64) {
        self.catch_up(asset);
        let price = self.asset(asset).effective_price;
        while self.slot() < to_slot {
            let next = self.slot() + 1;
            self.exec(|m, _| {
                m.accrue_asset_to_not_atomic(asset, next, price, 0, true)
                    .map(|_| ())
            })
            .unwrap();
        }
    }

    /// Publish a new oracle target for `asset` and accrue one slot to it.
    fn move_price(&mut self, asset: usize, price: u64) {
        self.catch_up(asset);
        let next = self.slot() + 1;
        self.exec(|m, _| {
            m.set_asset_raw_oracle_target_not_atomic(asset, price)?;
            m.accrue_asset_to_not_atomic(asset, next, price, 0, true)
                .map(|_| ())
        })
        .unwrap();
    }

    /// What keepers do continuously: expire every lapsed Fresh backing bucket.
    fn expire_lapsed(&mut self) {
        let now = self.slot();
        for domain in 0..self.markets.len() * 2 {
            let bucket = self.bucket(domain);
            if bucket.status == BackingBucketStatusV16::Fresh && bucket.expiry_slot <= now {
                self.exec(|m, _| m.expire_source_backing_bucket_not_atomic(domain, now))
                    .unwrap();
            }
        }
    }

    fn resolve(&mut self) {
        let now = self.slot();
        self.exec(|m, _| m.resolve_market_not_atomic(now)).unwrap();
    }
}

impl World {
    /// Vault conservation over the senior stack: every quote atom is in exactly one class.
    fn assert_conserved(&self) {
        let h = &self.header;
        let senior = h.c_tot.get()
            + h.insurance.get()
            + h.backing_provider_earnings_total.get()
            + h.source_fresh_backing_total_num.get() / BOUND_SCALE;
        assert!(h.vault.get() >= senior, "vault below senior stack");
        let capital: u128 = self.accounts.iter().map(|a| a.capital.get()).sum();
        assert_eq!(h.c_tot.get(), capital, "c_tot drifted from account capital");
        let fresh: u128 = (0..self.markets.len() * 2)
            .map(|d| {
                let b = self.bucket(d);
                b.fresh_unliened_backing_num + b.valid_liened_backing_num
            })
            .sum();
        assert_eq!(h.source_fresh_backing_total_num.get(), fresh);
        let impaired_bucket: u128 = (0..self.markets.len() * 2)
            .map(|d| self.bucket(d).impaired_liened_backing_num)
            .sum();
        let impaired_source: u128 = (0..self.markets.len() * 2)
            .map(|d| self.source_credit(d).impaired_liened_backing_num)
            .sum();
        assert_eq!(impaired_bucket, impaired_source);
    }

    fn lien(&self, i: usize, domain_slot: usize) -> (u128, u128, u128) {
        let s = &self.accounts[i].source_domains[domain_slot];
        (
            s.source_claim_liened_num.get(),
            s.source_lien_effective_reserved.get(),
            s.source_lien_counterparty_backing_num.get(),
        )
    }
}
/// W (0) goes long against L (1). After a 10% move, W liens the backing created
/// by L's loss to add one unit against M (2), and the public crank expires the
/// bucket. Portfolios 3 and 4 are new traders.
fn expired_bucket_with_open_lien_holder() -> World {
    let mut w = World::new(1, 100, 5, |_| {});
    w.deposit(0, 1_000);
    w.deposit(1, 1_000);
    w.trade(0, 1, 0, 10, 100).unwrap();

    // Price +10%. L's realised loss of 100 leaves its capital and becomes
    // counterparty backing on source domain 1 (asset 0, Short). W's gain is a
    // claim on the same domain.
    w.move_price(0, 110);
    w.refresh(1).unwrap();
    w.refresh(0).unwrap();
    assert_eq!(w.accounts[1].capital.get(), 900);
    assert_eq!(w.bucket(1).fresh_unliened_backing_num, 100 * BOUND_SCALE);

    // W uses its gain as initial margin for one more unit, which liens all 100.
    w.deposit(0, 110);
    w.deposit(2, 110);
    w.trade(0, 2, 0, 1, 110).unwrap();
    assert_eq!(w.bucket(1).valid_liened_backing_num, 100 * BOUND_SCALE);

    // The bucket's lifetime was fixed when it was first funded. When it lapses
    // the crank expires it and writes the liened backing off as impaired.
    let expiry = w.bucket(1).expiry_slot;
    w.tick_to(0, expiry);
    let crank = w.crank(0, expiry, 0, 110).unwrap();
    assert_eq!(
        crank.outcome,
        AutoCrankOutcomeV16::Progressed(PermissionlessProgressOutcomeV16::SourceBackingExpired {
            domain: 1
        })
    );
    assert_eq!(w.bucket(1).status, BackingBucketStatusV16::Impaired);
    assert_eq!(w.bucket(1).valid_liened_backing_num, 0);
    assert_eq!(w.source_credit(1).fresh_reserved_backing_num, 0);

    // W's account-local lien is untouched.
    assert_eq!(
        w.accounts[0].source_domains[0]
            .source_lien_effective_reserved
            .get(),
        100
    );
    w
}

#[test]
fn losing_side_settles_and_liquidates_while_lien_holder_is_open() {
    let mut w = expired_bucket_with_open_lien_holder();
    w.assert_conserved();
    let vault = w.header.vault.get();
    let fresh_before = w.header.source_fresh_backing_total_num.get();
    let residual_before = w.residual();

    // Price +10% again. Both shorts' losses land on domain 1, whose bucket is still Impaired
    // because W has not been touched. They settle, and the realized loss leaves their capital
    // as junior support (exactly as Resolved mode books it), not as fresh backing.
    w.move_price(0, 121);
    let now = w.slot();
    w.refresh(1).unwrap();
    assert_eq!(w.accounts[1].capital.get(), 900 - 110);
    w.crank(2, now, 0, 121).unwrap();
    w.refresh(2).unwrap();
    assert_eq!(w.accounts[2].capital.get(), 110 - 11);
    assert_eq!(w.bucket(1).status, BackingBucketStatusV16::Impaired);
    assert_eq!(w.header.source_fresh_backing_total_num.get(), fresh_before);
    assert_eq!(w.residual(), residual_before + 110 + 11);
    assert_eq!(w.header.vault.get(), vault);
    w.assert_conserved();

    // The long side settles; W's own settlement reconciles its lien against the Impaired
    // bucket, which retires the bucket's last impaired lien and returns it to Expired.
    w.refresh(0).unwrap();
    assert_eq!(w.lien(0, 0), (0, 0, 0));
    assert_eq!(w.bucket(1).status, BackingBucketStatusV16::Expired);
    assert_eq!(w.bucket(1).impaired_liened_backing_num, 0);
    assert_eq!(w.source_credit(1).impaired_liened_backing_num, 0);
    w.assert_conserved();

    // New traders can open risk on the asset again.
    w.deposit(3, 1_000);
    w.deposit(4, 1_000);
    w.trade(3, 4, 0, 1, 121).unwrap();

    // The price keeps rising; L's loss exceeds its capital and L is liquidated.
    w.move_price(0, 210);
    assert!(10 * (210 - 121) > w.accounts[1].capital.get());
    let cert = w.refresh(1).unwrap();
    assert!(cert.certified_liq_deficit > 0);
    w.liquidate(1, 0).unwrap();
    w.assert_conserved();
    assert_eq!(w.header.vault.get(), vault + 2_000);
}

#[test]
fn lien_holder_reports_true_equity_and_settles_after_loss() {
    let mut w = expired_bucket_with_open_lien_holder();
    let vault = w.header.vault.get();
    let residual_before = w.residual();

    // W's certificate no longer counts the forfeited 100: equity is its 1,110 of capital
    // against a 1,210 maintenance requirement, so W is liquidatable.
    let cert = w.refresh(0).unwrap();
    assert_eq!(w.accounts[0].capital.get(), 1_110);
    assert_eq!(cert.certified_maintenance_req, 1_210);
    assert_eq!(cert.certified_equity, 1_110);
    assert_eq!(cert.certified_liq_deficit, 100);
    // The reconciliation moved no value: the forfeit was already in the junior pool.
    assert_eq!(w.lien(0, 0), (0, 0, 0));
    assert_eq!(w.bucket(1).status, BackingBucketStatusV16::Expired);
    assert_eq!(w.residual(), residual_before);
    assert_eq!(w.header.vault.get(), vault);
    w.assert_conserved();
    let mut liquidated = w.clone();
    liquidated.liquidate(0, 0).unwrap();
    liquidated.assert_conserved();

    // Price -10%. W's loss burns its (now impaired, unbacked) claim face first, then capital.
    // The capital-funded part becomes fresh backing on domain 0 for the shorts' gains,
    // exactly as an un-expired lien's burn would leave it.
    let fresh0_before = w.bucket(0).fresh_unliened_backing_num;
    w.move_price(0, 100);
    let now = w.slot();
    let cert = w.refresh(0).unwrap();
    assert_eq!(w.accounts[0].capital.get(), 1_100);
    assert_eq!(w.accounts[0].pnl.get(), 0);
    assert_eq!(cert.certified_equity, 1_100);
    assert_eq!(
        w.bucket(0).fresh_unliened_backing_num,
        fresh0_before + 10 * BOUND_SCALE
    );
    assert_eq!(w.residual(), residual_before);
    w.assert_conserved();
    w.crank(0, now, 0, 100).unwrap();
    w.refresh(1).unwrap();
    w.refresh(2).unwrap();
    assert!(w.source_credit(0).credit_rate_num > 0);

    // Every other settlement path for W progresses.
    let mut reduced = w.clone();
    reduced.reduce(0, 0, 11).unwrap();
    reduced.assert_conserved();
    w.deposit(0, 10_000);
    w.refresh(0).unwrap();

    // New risk opens on the asset.
    w.deposit(3, 1_000);
    w.deposit(4, 1_000);
    w.trade(3, 4, 0, 1, 100).unwrap();
    w.assert_conserved();
}

#[test]
fn lien_holder_recovery_paths_settle_after_loss() {
    let mut w = expired_bucket_with_open_lien_holder();
    w.move_price(0, 100);
    let now = w.slot();
    w.exec(|m, _| m.force_asset_recovery_not_atomic(0, now))
        .unwrap();

    let mut forfeited = w.clone();
    forfeited
        .on(0, |m, a| {
            m.forfeit_recovery_leg_not_atomic(a, 0, MAX_VAULT_TVL)
                .map(|_| ())
        })
        .unwrap();
    forfeited.assert_conserved();

    w.exec(|m, accounts| {
        let (long, short) = two(accounts, 0, 1);
        m.force_close_recovery_pair_not_atomic(
            &mut PortfolioV16ViewMut::new(long),
            &mut PortfolioV16ViewMut::new(short),
            0,
            u128::MAX,
        )
        .map(|_| ())
    })
    .unwrap();
    w.assert_conserved();
}
