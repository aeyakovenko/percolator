//! Issue #251: a resolved winner whose source claim converts to zero whole
//! atoms must neither freeze the group nor wait for nothing.
//!
//! A resolved winner realizes its source claim against the domain's backing at
//! the current credit rate, in whole atoms. Before the fix a claim that
//! converted to zero on a Fresh, partially backed domain was an error for as
//! long as any co-claimant remained, and the SVM rollback also undid whatever
//! the same instruction had already done. Two things followed:
//!
//! - When the account held the group's last stored leg, the rollback restored
//!   the leg, so the group never became payout-ready: no winner could realize
//!   and everything stayed frozen until the bucket lapsed at `resolved_slot +
//!   backing horizon`. By then every Fresh bucket of the group had lapsed too,
//!   so claims that were fully backed were paid at the common junior rate.
//! - Wherever every remaining claim of a domain converted to zero, they all
//!   waited on each other until the bucket lapsed. One atom of backing in a
//!   domain with two claimants is enough for that.
//!
//! After the fix a zero-conversion claim waits only for backing that a
//! co-claimant's lien still pledges, and a wait never rolls back progress.
//!
//! Single-asset world (`dust_world`): longs L1, L2 lose 5 per unit, shorts W
//! (2_000 units) and X (dust) win. L1 closes before any winner attributed a
//! claim, so its 5_000 goes to the junior residual; L2 then funds the winners'
//! domain with half of what they are owed. X's 1-atom claim converts to zero
//! at that rate.
//!
//! Two-asset world (`seniority_world`): asset 0 has an under-collateralized
//! long Z against shorts A and DUST (domain 0 backed at ~0.78, no junior
//! residual); asset 1 has an honest pair, short V against long LV, fully
//! backed. Honest payouts: V is paid in full from its own domain.
//!
//! Two-asset worlds with liens (`pledged_backing_live`, the lien-holder test):
//! a co-claimant's lien pledges the backing, so a large claim converts to zero
//! until the lien holder has been closed.
use percolator::{
    active_bitmap_is_empty, AutoCrankObservationV16, AutoCrankOutcomeV16, AutoCrankPlanV16,
    AutoCrankResultV16, AutoCrankWorkV16, BackingBucketStatusV16, BackingBucketV16,
    EngineAssetSlotV16Account, Market, MarketGroupV16HeaderAccount, MarketGroupV16ViewMut,
    PortfolioAccountV16Account, PortfolioV16ViewMut, ProvenanceHeaderV16,
    ProvenanceHeaderV16Account, ResolvedCloseOutcomeV16, SourceCreditStateV16, TradeRequestV16,
    V16Config, V16Error, V16PodU64, V16Result, BOUND_SCALE, CREDIT_RATE_SCALE, POS_SCALE,
};

const UNIT: u128 = POS_SCALE;
/// `public_user_fund_with_market_slots(.., h_max = 10)`: resolved backing lapses
/// at `resolved_slot + 10`.
const HORIZON: u64 = 10;

/// Engine state plus portfolios. Every engine call goes through `exec`, which
/// restores the prior state when the call fails: the engine entry points are
/// `_not_atomic` and rely on the SVM transaction rollback for atomicity.
#[derive(Clone)]
struct World {
    header: MarketGroupV16HeaderAccount,
    markets: Vec<Market<u64>>,
    accounts: Vec<PortfolioAccountV16Account>,
    paid: Vec<u128>,
}

impl World {
    fn new(assets: u32, accounts: usize, tweak: impl FnOnce(&mut V16Config)) -> Self {
        let mut cfg =
            V16Config::public_user_fund_with_market_slots(assets as u16, assets, 0, HORIZON);
        cfg.max_price_move_bps_per_slot = 500;
        cfg.max_accrual_dt_slots = 1;
        tweak(&mut cfg);
        let mut header = MarketGroupV16HeaderAccount::new_dynamic([1; 32], cfg, assets, 0).unwrap();
        let mut markets: Vec<Market<u64>> = (0..assets)
            .map(|_| Market::new(0, EngineAssetSlotV16Account::default()))
            .collect();
        for (i, market) in markets.iter_mut().enumerate() {
            header
                .activate_empty_asset_slot_not_atomic(
                    i as u32,
                    &mut market.engine,
                    100,
                    i as u64 + 1,
                )
                .unwrap();
        }
        let accounts = (0..accounts)
            .map(|i| {
                let provenance = ProvenanceHeaderV16Account::from_runtime(
                    &ProvenanceHeaderV16::new([1; 32], [40 + i as u8; 32], [3; 32]),
                );
                let mut account = PortfolioAccountV16Account::default();
                account.init_empty_in_place(provenance).unwrap();
                account
            })
            .collect::<Vec<_>>();
        let paid = vec![0; accounts.len()];
        Self {
            header,
            markets,
            accounts,
            paid,
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

    fn deposit(&mut self, i: usize, amount: u128) {
        self.on(i, |m, a| m.deposit_not_atomic(a, amount)).unwrap();
    }

    /// `long` buys `size_q` of `asset` from `short` at 100.
    fn trade(&mut self, asset: usize, long: usize, short: usize, size_q: u128) {
        self.trade_at(asset, long, short, size_q, 100).unwrap();
    }

    fn trade_at(
        &mut self,
        asset: usize,
        long: usize,
        short: usize,
        size_q: u128,
        price: u64,
    ) -> V16Result<()> {
        self.exec(|market, accounts| {
            assert_ne!(long, short);
            let (lo, hi) = accounts.split_at_mut(long.max(short));
            let (l, s) = if long < short {
                (&mut lo[long], &mut hi[0])
            } else {
                (&mut hi[0], &mut lo[short])
            };
            market
                .execute_trade_with_fee_loss_stale_scoped_not_atomic(
                    &mut PortfolioV16ViewMut::new(l),
                    &mut PortfolioV16ViewMut::new(s),
                    TradeRequestV16 {
                        asset_index: asset,
                        size_q: size_q as i128,
                        exec_price: price,
                        fee_bps: 0,
                    },
                )
                .map(|_| ())
        })
    }

    /// Accrue every asset one slot forward to the given marks.
    fn mark(&mut self, prices: &[u64]) {
        let slot = self.header.current_slot.get() + 1;
        self.exec(|m, _| {
            for (asset, &price) in prices.iter().enumerate() {
                m.set_asset_raw_oracle_target_not_atomic(asset, price)?;
                m.accrue_asset_to_not_atomic(asset, slot, price, 0, true)?;
            }
            Ok(())
        })
        .unwrap();
    }

    fn resolve(&mut self) -> u64 {
        let slot = self.header.current_slot.get() + 1;
        self.exec(|m, _| m.resolve_market_not_atomic(slot)).unwrap();
        slot
    }

    fn advance(&mut self, slot: u64) {
        self.exec(|m, _| m.advance_resolved_slot_not_atomic(slot))
            .unwrap();
    }

    /// One resolved-close instruction; any payout is credited to `paid[i]`.
    fn close(&mut self, i: usize) -> V16Result<ResolvedCloseOutcomeV16> {
        let out = self.on(i, |m, a| m.close_resolved_account_not_atomic(a, 0));
        if let Ok(ResolvedCloseOutcomeV16::Closed { payout }) = out {
            self.paid[i] += payout;
        }
        out
    }

    /// The same step through the single public permissionless crank.
    fn crank(&mut self, i: usize) -> V16Result<AutoCrankResultV16> {
        let now_slot = self.header.current_slot.get();
        let out = self.on(i, |m, a| {
            m.permissionless_auto_crank_not_atomic(
                a,
                AutoCrankWorkV16 {
                    now_slot,
                    observations: &[AutoCrankObservationV16 {
                        asset_index: 0,
                        effective_price: m.markets[0].engine.asset.effective_price.get(),
                        funding_rate_e9: 0,
                    }],
                    resolved_close_fee_rate_per_slot: 0,
                },
            )
        });
        if let Ok(AutoCrankResultV16 {
            outcome: AutoCrankOutcomeV16::ResolvedClose(ResolvedCloseOutcomeV16::Closed { payout }),
            ..
        }) = out
        {
            self.paid[i] += payout;
        }
        out
    }

    fn topup(&mut self, i: usize) {
        if let Ok(payout) = self.on(i, |m, a| m.claim_resolved_payout_topup_not_atomic(a)) {
            self.paid[i] += payout;
        }
    }

    /// Keeper sweep: every account in `order`, repeatedly, without moving the
    /// clock. A waiting close (`LockActive`) is skipped like a keeper would;
    /// any other error fails the test.
    fn sweep(&mut self, order: &[usize]) {
        for _ in 0..12 {
            for &i in order {
                match self.close(i) {
                    Ok(_) | Err(V16Error::LockActive) => {}
                    Err(e) => panic!("close {i} failed: {e:?}"),
                }
                self.assert_conserved();
            }
        }
        for &i in order {
            self.topup(i);
        }
    }

    /// What keepers do continuously: expire every lapsed Fresh backing bucket.
    fn expire_lapsed(&mut self) {
        let now = self.header.current_slot.get();
        for domain in 0..self.markets.len() * 2 {
            let bucket = self.bucket(domain);
            if bucket.status == BackingBucketStatusV16::Fresh
                && bucket
                    .expiry_slot
                    .min(self.header.resolved_slot.get() + HORIZON)
                    <= now
            {
                self.exec(|m, _| m.expire_source_backing_bucket_not_atomic(domain, now))
                    .unwrap();
            }
        }
    }

    fn settled(&self, i: usize) -> bool {
        let a = &self.accounts[i];
        a.capital.get() == 0
            && a.pnl.get() == 0
            && active_bitmap_is_empty(a.active_bitmap.map(V16PodU64::get))
    }

    fn has_leg(&self, i: usize) -> bool {
        !active_bitmap_is_empty(self.accounts[i].active_bitmap.map(V16PodU64::get))
    }

    fn bucket(&self, domain: usize) -> BackingBucketV16 {
        let slot = &self.markets[domain / 2].engine;
        if domain % 2 == 0 {
            slot.backing_long.try_to_runtime().unwrap()
        } else {
            slot.backing_short.try_to_runtime().unwrap()
        }
    }

    fn source(&self, domain: usize) -> SourceCreditStateV16 {
        let slot = &self.markets[domain / 2].engine;
        if domain % 2 == 0 {
            slot.source_credit_long.try_to_runtime().unwrap()
        } else {
            slot.source_credit_short.try_to_runtime().unwrap()
        }
    }

    /// (claims, fresh backing) of a domain in whole atoms.
    fn domain(&self, domain: usize) -> (u128, u128) {
        let s = self.source(domain);
        (
            s.positive_claim_bound_num / BOUND_SCALE,
            s.fresh_reserved_backing_num / BOUND_SCALE,
        )
    }

    fn blockers(&self) -> u64 {
        self.header.resolved_payout_blocker_count.get()
    }

    /// The vault covers every senior class, capital totals agree, and the
    /// engine's own shape and account validations hold.
    fn assert_conserved(&self) {
        let h = &self.header;
        let senior = h.c_tot.get()
            + h.insurance.get()
            + h.backing_provider_earnings_total.get()
            + h.source_fresh_backing_total_num.get() / BOUND_SCALE;
        assert!(h.vault.get() >= senior, "vault below the senior stack");
        let capital: u128 = self.accounts.iter().map(|a| a.capital.get()).sum();
        assert_eq!(h.c_tot.get(), capital);
        let mut copy = self.clone();
        let market = MarketGroupV16ViewMut::new(&mut copy.header, &mut copy.markets);
        market.validate_shape().unwrap();
        for account in copy.accounts.iter_mut() {
            PortfolioV16ViewMut::new(account)
                .validate_with_market(&market.as_view())
                .unwrap();
        }
    }
}

const L1: usize = 0;
const L2: usize = 1;
const W: usize = 2;
const X: usize = 3;

/// Resolved single-asset world, nothing closed yet. `dust_q` is X's short size:
/// 0.2 units win 1 atom (converts to zero at rate ~0.5), 0.4 units win 2 atoms
/// (convert to one).
fn dust_world(dust_q: u128) -> World {
    let mut w = World::new(1, 4, |_| {});
    for i in 0..4 {
        w.deposit(i, 1_000_000);
    }
    w.trade(0, L1, W, 1_000 * UNIT);
    w.trade(0, L2, W, 1_000 * UNIT);
    w.trade(0, L2, X, dust_q);
    w.mark(&[95]);
    w.resolve();
    w
}

/// From `dust_world`: L1 closes before any claim is attributed (its 5_000 goes
/// to the junior residual), W attributes and detaches, L2 funds the winners'
/// domain. Only X's leg is still stored.
fn leave_dust_leg_last(w: &mut World) {
    assert_eq!(
        w.close(L1),
        Ok(ResolvedCloseOutcomeV16::Closed { payout: 995_000 })
    );
    assert_eq!(w.close(W), Ok(ResolvedCloseOutcomeV16::ProgressOnly));
    assert!(matches!(
        w.close(L2),
        Ok(ResolvedCloseOutcomeV16::Closed { .. })
    ));
    assert_eq!(w.bucket(0).status, BackingBucketStatusV16::Fresh);
    assert_eq!(w.domain(0).0, 10_000, "only W's claim is attributed so far");
    assert!(w.source(0).credit_rate_num < CREDIT_RATE_SCALE * 51 / 100);
    assert!(w.has_leg(X) && !w.has_leg(W));
    assert_eq!(w.blockers(), 2, "X's stored + stale-cohort leg");
}

/// The defect. X holds the last stored leg and a claim that converts to zero
/// whole atoms while W is still a co-claimant of the Fresh domain. No lien
/// pledges the domain, so the claim is final: X's close detaches the leg,
/// retires the claim to junior face and pays X's capital in one call, and W is
/// paid in the same slot.
#[test]
fn dust_claim_on_the_last_stored_leg_does_not_freeze_resolved_payouts() {
    let mut w = dust_world(UNIT / 5);
    let resolved_slot = w.header.resolved_slot.get();
    leave_dust_leg_last(&mut w);

    assert_eq!(
        w.close(X),
        Ok(ResolvedCloseOutcomeV16::Closed { payout: 1_000_000 }),
        "X's zero-conversion claim must not keep its leg stored"
    );
    assert!(!w.has_leg(X));
    assert_eq!(w.blockers(), 0, "the group is payout-ready");
    assert_eq!(
        w.domain(0),
        (10_000, 5_001),
        "retired without touching the backing"
    );
    // X's atom is junior face now, paid once the residual covers it.
    let receipt = w.accounts[X]
        .resolved_payout_receipt
        .try_to_runtime()
        .unwrap();
    assert!(receipt.present);
    assert_eq!(
        (receipt.terminal_positive_claim_face, receipt.paid_effective),
        (1, 0)
    );
    w.assert_conserved();

    // W realizes 5_001 of backing and its other 4_999 from the junior residual.
    assert_eq!(
        w.close(W),
        Ok(ResolvedCloseOutcomeV16::Closed { payout: 1_010_000 })
    );
    w.topup(X);
    assert_eq!(w.paid, vec![995_000, 994_999, 1_010_000, 1_000_001]);
    assert_eq!(w.header.vault.get(), 0);
    assert_eq!(
        w.header.current_slot.get(),
        resolved_slot,
        "no waiting needed"
    );
    w.assert_conserved();
}

/// Same order through the single public crank: every winner settles in the
/// resolved slot.
#[test]
fn dust_claim_on_the_last_stored_leg_settles_through_the_public_crank() {
    let mut w = dust_world(UNIT / 5);
    leave_dust_leg_last(&mut w);

    let step = w.crank(X).expect("X's crank must commit its leg detach");
    assert_eq!(step.selected, AutoCrankPlanV16::CloseResolved);
    assert_eq!(
        step.outcome,
        AutoCrankOutcomeV16::ResolvedClose(ResolvedCloseOutcomeV16::Closed { payout: 1_000_000 })
    );
    assert_eq!(w.blockers(), 0);
    // W settles; X's second crank collects its junior atom.
    for i in [W, X] {
        match w.crank(i) {
            Ok(_) => {}
            Err(e) => panic!("crank {i} failed: {e:?}"),
        }
    }
    assert_eq!(w.paid, vec![995_000, 994_999, 1_010_000, 1_000_001]);
    assert!(w.settled(W) && w.settled(X));
    assert_eq!(w.header.vault.get(), 0);
    w.assert_conserved();
}

/// Control (passes before and after): the same world when X is not the last
/// stored leg. Its detach commits while L2 still blocks payouts, W realizes
/// first, and the payouts are identical.
#[test]
fn control_dust_claim_detached_before_the_last_leg() {
    let mut w = dust_world(UNIT / 5);
    assert_eq!(
        w.close(L1),
        Ok(ResolvedCloseOutcomeV16::Closed { payout: 995_000 })
    );
    assert_eq!(w.close(W), Ok(ResolvedCloseOutcomeV16::ProgressOnly));
    assert_eq!(w.close(X), Ok(ResolvedCloseOutcomeV16::ProgressOnly));
    assert!(!w.has_leg(X));
    assert!(matches!(
        w.close(L2),
        Ok(ResolvedCloseOutcomeV16::Closed { .. })
    ));
    assert_eq!(w.blockers(), 0);
    assert_eq!(w.domain(0), (10_001, 5_001));
    w.sweep(&[W, X]);
    assert_eq!(w.paid, vec![995_000, 994_999, 1_010_000, 1_000_001]);
    assert_eq!(w.header.vault.get(), 0);
}

/// Control (passes before and after): X holds the last stored leg but its
/// 2-atom claim converts to one whole atom, so it realizes in the same call.
#[test]
fn control_convertible_claim_on_the_last_stored_leg() {
    let mut w = dust_world(2 * UNIT / 5);
    leave_dust_leg_last(&mut w);
    assert!(matches!(
        w.close(X),
        Ok(ResolvedCloseOutcomeV16::Closed { .. })
    ));
    assert_eq!(w.blockers(), 0);
    w.sweep(&[W, X]);
    assert_eq!(w.paid.iter().sum::<u128>(), 4_000_000);
    assert_eq!(w.paid[W] + w.paid[X], 2_010_002);
    assert_eq!(w.header.vault.get(), 0);
}

const Z: usize = 0;
const A: usize = 1;
const DUST: usize = 2;
const V: usize = 3;
const LV: usize = 4;

/// Resolved two-asset world, nothing closed yet.
///
/// Asset 0 falls 100 -> 87: Z (10_200 of capital) is long 1_000.1 units and
/// owes 13_002, of which it can pay 10_200; A (short 1_000) is owed 13_000 and
/// DUST (short 0.1) is owed 1. Z also holds a 0.1-unit long on asset 1, so its
/// shortfall has no single bankruptcy domain and is written off: domain 0 ends
/// backed at 10_200 / 13_001 and there is no junior residual.
///
/// Asset 1 falls 100 -> 95: V (short 1_000.1) is owed 5_000 by LV (long 1_000).
fn seniority_world() -> World {
    let mut w = seniority_world_live();
    w.resolve();
    w
}

/// `seniority_world` just before resolution (asset 0 at 87, asset 1 at 95,
/// nobody refreshed since the trades).
fn seniority_world_live() -> World {
    let mut w = two_asset_world(&[10_200, 1_000_000, 1_000_000, 1_000_000, 1_000_000]);
    w.trade(0, Z, A, 1_000 * UNIT);
    w.trade(0, Z, DUST, UNIT / 10);
    w.trade(1, Z, V, UNIT / 10);
    w.trade(1, LV, V, 1_000 * UNIT);
    for asset0 in [95, 91, 87] {
        w.mark(&[asset0, 95]);
    }
    w
}

/// Two assets at 100 with 10% margins and one funded account per deposit.
fn two_asset_world(deposits: &[u128]) -> World {
    let mut w = World::new(2, deposits.len(), |cfg| {
        cfg.initial_margin_bps = 1_000;
        cfg.maintenance_margin_bps = 1_000;
    });
    // Asset 1 was activated at slot 2; bring asset 0 level with it.
    w.exec(|m, _| m.accrue_asset_to_not_atomic(0, 2, 100, 0, true).map(|_| ()))
        .unwrap();
    for (i, &amount) in deposits.iter().enumerate() {
        w.deposit(i, amount);
    }
    w
}

/// What every account is paid when the winners attribute first and everyone is
/// then swept in the resolved slot.
const SENIORITY_HONEST: [u128; 5] = [0, 1_010_199, 1_000_001, 1_005_000, 995_000];

#[test]
fn control_seniority_world_honest_order() {
    let mut w = seniority_world();
    w.sweep(&[V, A, DUST, LV, Z]);
    assert_eq!(w.paid, SENIORITY_HONEST);
    assert_eq!(w.header.vault.get(), 0);
}

/// Funds, not only time: with DUST holding the last stored leg nobody could
/// realize before the backing horizon, and a lapsed bucket forfeits its
/// backing to the common junior pool. V's fully backed 5_000 was then shared
/// with A's unbacked face. In the adversarial close order V must be paid in
/// full in the resolved slot.
///
/// The order still decides one rounding atom of domain 0. DUST's share of its
/// backing is 0.78 of an atom. Cranked after A (`SENIORITY_HONEST`) it converts
/// the atom A's floor left behind; retired before A, as here, it converts
/// nothing and that atom stays in the bucket as surplus. A converts 10_199
/// either way.
#[test]
fn dust_leg_last_does_not_cost_backed_winners_their_seniority() {
    let mut w = seniority_world();
    let resolved_slot = w.header.resolved_slot.get();
    // Winners attribute and detach, losers book, DUST's leg stays stored.
    for i in [V, A, LV, Z, V, A, LV, Z] {
        w.close(i).unwrap();
    }
    assert_eq!(w.blockers(), 2);
    assert!(w.has_leg(DUST));
    assert_eq!(w.domain(0), (13_000, 10_200));
    assert_eq!(w.domain(2), (5_000, 5_000));
    assert_eq!(w.source(2).credit_rate_num, CREDIT_RATE_SCALE);

    assert_eq!(
        w.close(DUST),
        Ok(ResolvedCloseOutcomeV16::Closed { payout: 1_000_000 }),
        "DUST's zero-conversion claim must not hold the group's last stored leg"
    );
    assert_eq!(w.blockers(), 0);
    // V realizes its fully backed claim at once, whatever DUST and A do.
    assert_eq!(
        w.close(V),
        Ok(ResolvedCloseOutcomeV16::Closed { payout: 1_005_000 })
    );
    w.sweep(&[DUST, A, DUST, V, LV, Z]);
    assert_eq!(w.paid, vec![0, 1_010_199, 1_000_000, 1_005_000, 995_000]);
    assert_eq!(w.domain(0), (0, 1));
    assert_eq!(w.header.vault.get(), 1);
    assert_eq!(w.header.current_slot.get(), resolved_slot);
}

/// Characterization (passes before and after): what the freeze used to cost.
/// If no winner can realize before `resolved_slot + HORIZON`, both buckets
/// lapse into the junior pool and V is paid 779 less, A 778 more.
#[test]
fn control_lapsed_backing_pays_backed_winners_the_junior_rate() {
    let mut w = seniority_world();
    let resolved_slot = w.header.resolved_slot.get();
    for i in [V, A, LV, Z, V, A, LV, Z] {
        w.close(i).unwrap();
    }
    assert_eq!(
        w.bucket(0).expiry_slot.min(resolved_slot + HORIZON),
        resolved_slot + HORIZON
    );
    w.advance(resolved_slot + HORIZON);
    w.sweep(&[DUST, V, A, LV, Z]);
    assert_eq!(w.paid, vec![0, 1_010_977, 1_000_000, 1_004_221, 995_000]);
    assert_eq!(w.header.vault.get(), 2, "floor dust of the common rate");
}

/// A dust claim in one domain must not hold the account's claim in another.
/// Here the fully backed winner itself carries the 0.1-unit dust short on
/// asset 0 (accounts: Z, A, the winner, LV), and its second leg is the group's
/// last stored one. Its dust claim on domain 0 retires in one step, and its
/// 5_000 on domain 2 realizes in the next.
#[test]
fn dust_claim_in_one_domain_does_not_hold_the_accounts_other_claims() {
    const VV: usize = 2;
    const LL: usize = 3;
    let mut w = two_asset_world(&[10_200, 1_000_000, 1_000_000, 1_000_000]);
    w.trade(0, Z, A, 1_000 * UNIT);
    w.trade(0, Z, VV, UNIT / 10);
    w.trade(1, Z, VV, UNIT / 10);
    w.trade(1, LL, VV, 1_000 * UNIT);
    for asset0 in [95, 91, 87] {
        w.mark(&[asset0, 95]);
    }
    let resolved_slot = w.resolve();
    // The winner attributes both claims and detaches its asset-0 leg; then
    // everyone else closes. Its asset-1 leg is the last stored one.
    for i in [VV, A, LL, Z, A, LL, Z] {
        w.close(i).unwrap();
    }
    assert_eq!(w.blockers(), 1);
    assert_eq!(w.domain(0), (13_001, 10_200));
    assert_eq!(w.domain(2), (5_000, 5_000));

    // Detach, then retire the dust claim on domain 0 (0.78 of an atom).
    assert_eq!(w.close(VV), Ok(ResolvedCloseOutcomeV16::ProgressOnly));
    assert_eq!(w.blockers(), 0);
    assert_eq!(w.domain(0), (13_000, 10_200));
    assert_eq!(w.domain(2), (5_000, 5_000));
    // Realize domain 2 in full.
    assert_eq!(
        w.close(VV),
        Ok(ResolvedCloseOutcomeV16::Closed { payout: 1_005_000 })
    );
    w.sweep(&[A, VV, LL, Z]);
    assert_eq!(w.paid, vec![0, 1_010_199, 1_005_000, 995_000]);
    assert_eq!(w.header.current_slot.get(), resolved_slot);
}

const X2: usize = 4;

/// Two zero-conversion co-claimants, one of them on the last stored leg.
/// Neither waits for the other or for W, in either order: a dust claim that
/// is cranked before W retires to junior face, one that is cranked after W
/// converts the atom W's floor left behind. Every account is paid in full in
/// the resolved slot.
#[test]
fn two_dust_claims_do_not_wait_for_each_other() {
    for x_before_w in [false, true] {
        let mut w = World::new(1, 5, |_| {});
        for i in 0..5 {
            w.deposit(i, 1_000_000);
        }
        // L1 long 1_200.4 (W 1_200, X 0.2, X2 0.2), L2 long 800 against W.
        w.trade(0, L1, W, 1_200 * UNIT);
        w.trade(0, L1, X, UNIT / 5);
        w.trade(0, L1, X2, UNIT / 5);
        w.trade(0, L2, W, 800 * UNIT);
        w.mark(&[95]);
        let resolved_slot = w.resolve();

        assert_eq!(
            w.close(L1),
            Ok(ResolvedCloseOutcomeV16::Closed { payout: 993_998 })
        );
        assert_eq!(w.close(W), Ok(ResolvedCloseOutcomeV16::ProgressOnly));
        assert_eq!(w.close(X), Ok(ResolvedCloseOutcomeV16::ProgressOnly));
        assert_eq!(
            w.close(L2),
            Ok(ResolvedCloseOutcomeV16::Closed { payout: 996_000 })
        );
        // X2 owns the last stored leg; domain 0 holds 4_000 for 10_002 of claims.
        assert_eq!(w.blockers(), 2);
        assert_eq!(
            w.close(X2),
            Ok(ResolvedCloseOutcomeV16::Closed { payout: 1_000_000 })
        );
        assert_eq!(w.blockers(), 0);
        assert_eq!(w.domain(0), (10_001, 4_000));

        if x_before_w {
            assert_eq!(
                w.close(X),
                Ok(ResolvedCloseOutcomeV16::Closed { payout: 1_000_000 })
            );
            assert_eq!(w.domain(0), (10_000, 4_000));
            assert!(matches!(
                w.close(W),
                Ok(ResolvedCloseOutcomeV16::Closed { .. })
            ));
            assert_eq!(w.domain(0), (0, 0));
        } else {
            assert!(matches!(
                w.close(W),
                Ok(ResolvedCloseOutcomeV16::Closed { .. })
            ));
            assert_eq!(w.domain(0), (1, 1));
            assert_eq!(
                w.close(X),
                Ok(ResolvedCloseOutcomeV16::Closed { payout: 1_000_001 })
            );
        }
        w.sweep(&[X, X2, W]);
        assert_eq!(
            w.paid,
            vec![993_998, 996_000, 1_010_000, 1_000_001, 1_000_001]
        );
        assert_eq!(w.header.vault.get(), 0);
        assert_eq!(w.header.current_slot.get(), resolved_slot);
    }
}

/// One atom of backing must not make a whole domain wait. The loser settles
/// before either winner attributed its claim, so the winners' domain has an
/// Empty bucket and their claims are junior (#220). A third party then
/// deposits a single atom of backing: the bucket is Fresh, each 5_000 claim
/// converts to zero, and each used to wait on the other until the bucket
/// lapsed, with the group's last stored leg rolled back on top of it.
#[test]
fn a_single_atom_of_backing_does_not_make_every_claimant_wait() {
    const L: usize = 0;
    const W1: usize = 1;
    const W2: usize = 2;
    let mut w = World::new(1, 3, |_| {});
    for i in 0..3 {
        w.deposit(i, 1_000_000);
    }
    w.trade(0, L, W1, 1_000 * UNIT);
    w.trade(0, L, W2, 1_000 * UNIT);
    w.mark(&[95]);
    let resolved_slot = w.resolve();
    assert_eq!(
        w.close(L),
        Ok(ResolvedCloseOutcomeV16::Closed { payout: 990_000 })
    );
    assert_eq!(w.close(W1), Ok(ResolvedCloseOutcomeV16::ProgressOnly));
    assert_eq!(w.bucket(0).status, BackingBucketStatusV16::Empty);

    w.exec(|m, _| m.deposit_fresh_counterparty_backing_not_atomic(0, 1, u64::MAX))
        .unwrap();
    assert_eq!(w.bucket(0).status, BackingBucketStatusV16::Fresh);
    assert_eq!(
        w.bucket(0).expiry_slot.min(resolved_slot + HORIZON),
        resolved_slot + HORIZON
    );

    // W2 holds the last stored leg. Its share of the atom is half an atom.
    assert_eq!(
        w.close(W2),
        Ok(ResolvedCloseOutcomeV16::Closed { payout: 1_005_000 })
    );
    assert_eq!(w.blockers(), 0);
    // W1 is the last claimant and converts the atom.
    assert_eq!(
        w.close(W1),
        Ok(ResolvedCloseOutcomeV16::Closed { payout: 1_005_000 })
    );
    assert_eq!(w.paid, vec![990_000, 1_005_000, 1_005_000]);
    assert_eq!(
        w.header.vault.get(),
        1,
        "the depositor's atom, now junior surplus"
    );
    assert_eq!(w.header.current_slot.get(), resolved_slot);
    w.assert_conserved();
}

const Y: usize = 1;
const PX: usize = 2;
const PH: usize = 3;
const S: usize = 4;
const PD: usize = 5;

/// Two-asset world in which a zero-conversion claim is not dust. On asset 1,
/// Y wins 5_000 from H, pledges all of it (and with it all of domain 2's
/// backing) to carry a larger short, and goes flat with the lien in place. Z
/// then goes long 1_500 of asset 0 against S and 1_000 of asset 1 against X.
/// Asset 0 falls to 79, asset 1 to 87: Z owes 24_000 on asset 0 and has
/// 23_750, so nothing is left for the 8_000 it owes X.
///
/// Domain 2 at resolution: 5_000 of backing, all pledged to Y; claims Y 5_000
/// and X 8_000. X's claim converts to zero at the pledged rate. Demoting it
/// then would leave all 5_000 to Y; waiting until Y has released and realized
/// pays both pro rata, Y 1_923 and X 3_077.
fn pledged_backing_live() -> World {
    pledged_backing_live_with(false)
}

/// `with_dust` adds PD, short 0.125 of asset 1 against Z from 95 and owed 1.
fn pledged_backing_live_with(with_dust: bool) -> World {
    // Z's initial margin at 95: 14_250 for asset 0 plus 9_500 (9_502 with PD).
    let mut deposits = vec![23_750, 10_200, 1_000_000, 1_000_000, 1_000_000];
    if with_dust {
        deposits[Z] = 23_752;
        deposits.push(1_000_000);
    }
    let mut w = two_asset_world(&deposits);
    w.trade(1, PH, Y, 1_000 * UNIT);
    w.mark(&[95, 95]);
    for i in [PH, Y] {
        w.on(i, |m, a| m.full_account_refresh_not_atomic(a).map(|_| ()))
            .unwrap();
    }
    assert_eq!(w.domain(2), (5_000, 5_000));
    // 1_600 units at 95 need 15_200 of initial margin: Y's 10_200 of capital
    // plus all 5_000 of its claim.
    w.trade_at(1, PH, Y, 600 * UNIT, 95).unwrap();
    w.trade_at(1, Y, PH, 1_600 * UNIT, 95).unwrap();
    assert!(!w.has_leg(Y) && !w.has_leg(PH));
    assert_eq!(w.source(2).valid_liened_backing_num, 5_000 * BOUND_SCALE);
    assert_eq!(w.source(2).credit_rate_num, 0);
    w.trade_at(0, Z, S, 1_500 * UNIT, 95).unwrap();
    w.trade_at(1, Z, PX, 1_000 * UNIT, 95).unwrap();
    if with_dust {
        w.trade_at(1, Z, PD, UNIT / 8, 95).unwrap();
    }
    for marks in [[91, 91], [87, 87], [83, 87], [79, 87]] {
        w.mark(&marks);
    }
    w
}

/// From payout readiness on: X waits however often it is cranked first, Y
/// releases its lien and realizes its share, then X realizes the rest. Returns
/// what every account was paid.
fn settle_pledged_backing(mut w: World) -> Vec<u128> {
    assert_eq!(w.blockers(), 0);
    assert!(!w.has_leg(PX));
    assert_eq!(w.domain(2), (13_000, 5_000));
    assert_eq!(w.source(2).credit_rate_num, 0, "all of it is pledged to Y");
    for _ in 0..3 {
        assert_eq!(w.close(PX), Err(V16Error::LockActive));
    }
    assert_eq!(w.accounts[PX].pnl.get(), 8_000);
    assert_eq!(w.domain(2), (13_000, 5_000));
    assert_eq!(
        w.close(Y),
        Ok(ResolvedCloseOutcomeV16::Closed {
            payout: 10_200 + 1_923
        })
    );
    assert_eq!(
        w.close(PX),
        Ok(ResolvedCloseOutcomeV16::Closed {
            payout: 1_000_000 + 3_077
        })
    );
    w.sweep(&[PX, Y, S, Z, PH]);
    w.paid
}

/// Payouts of `pledged_backing_live`: Z, Y, X, H, S.
const PLEDGED_PAID: [u128; 5] = [0, 12_123, 1_003_077, 995_000, 1_023_749];

/// Control (passes before and after): X detaches while other legs are stored,
/// then waits for Y.
#[test]
fn control_waiting_claim_keeps_its_share_of_pledged_backing() {
    let mut w = pledged_backing_live();
    w.resolve();
    for i in [PX, S, Z, PH, S, Z, PH] {
        w.close(i).unwrap();
    }
    assert_eq!(settle_pledged_backing(w), PLEDGED_PAID);
}

/// The same wait when X holds the group's last stored leg: its settlement and
/// detach commit, and the outcome is identical. X's fee anchor is synced
/// beforehand, as it is for any account on its second leg, so the detach is
/// the only step this call has to commit.
#[test]
fn waiting_claim_on_the_last_stored_leg_keeps_its_share_of_pledged_backing() {
    let mut w = pledged_backing_live();
    let resolved_slot = w.resolve();
    for i in [S, Z, PH, S, Z, PH] {
        w.close(i).unwrap();
    }
    w.on(PX, |m, a| {
        m.sync_account_fee_to_slot_not_atomic(a, resolved_slot, 0)
            .map(|_| ())
    })
    .unwrap();
    assert_eq!(w.blockers(), 2);
    assert!(w.has_leg(PX));
    assert_eq!(w.close(PX), Ok(ResolvedCloseOutcomeV16::ProgressOnly));
    assert!(!w.has_leg(PX));
    assert_eq!(settle_pledged_backing(w), PLEDGED_PAID);
}

/// A lien only makes a claim wait if releasing it could pay that claim. PD is
/// owed 1 atom in the same domain: even with Y's lien released its share is
/// floor(1 * 5_000 / 13_001) = 0, so it retires at once, on the group's last
/// stored leg, while X (share 3_076) keeps waiting for Y.
#[test]
fn dust_claim_does_not_wait_for_a_lien_holder() {
    let mut w = pledged_backing_live_with(true);
    w.resolve();
    for i in [PX, S, Z, PH, PX, S, Z, PH] {
        w.close(i).unwrap();
    }
    assert_eq!(w.blockers(), 2);
    assert!(w.has_leg(PD));
    assert_eq!(w.source(2).valid_liened_backing_num, 5_000 * BOUND_SCALE);
    assert_eq!(
        w.close(PD),
        Ok(ResolvedCloseOutcomeV16::Closed { payout: 1_000_000 })
    );
    assert_eq!(w.source(2).valid_liened_backing_num, 5_000 * BOUND_SCALE);
    // Z has 2 atoms more capital here, all of which S converts. PD's atom is
    // junior face with no residual behind it.
    assert_eq!(
        settle_pledged_backing(w),
        vec![0, 12_123, 1_003_077, 995_000, 1_023_751, 1_000_000]
    );
}

/// The boundary of the new outcome, with no leg involved. X is already flat at
/// resolution (it bought its short back from H at the mark). Its first
/// resolved close still has one pre-payout step, the fee-anchor sync, and
/// reports it; after that a call changes nothing and fails closed, through the
/// direct entry point and through the crank, until Y has realized.
#[test]
fn flat_waiting_claim_reports_progress_only_for_a_committed_step() {
    let mut w = pledged_backing_live();
    // H can only open the other side once every asset-1 account is current.
    for i in [PX, PH, Z] {
        w.on(i, |m, a| m.full_account_refresh_not_atomic(a).map(|_| ()))
            .unwrap();
    }
    w.trade_at(1, PX, PH, 1_000 * UNIT, 87).unwrap();
    assert!(!w.has_leg(PX));
    assert_eq!(w.accounts[PX].pnl.get(), 8_000);
    let resolved_slot = w.resolve();
    for i in [S, Z, PH, S, Z, PH] {
        w.close(i).unwrap();
    }
    assert_eq!(w.blockers(), 0);
    assert!(w.accounts[PX].last_fee_slot.get() < resolved_slot);

    assert_eq!(w.close(PX), Ok(ResolvedCloseOutcomeV16::ProgressOnly));
    assert_eq!(w.accounts[PX].last_fee_slot.get(), resolved_slot);
    let parked = w.clone();
    assert_eq!(w.close(PX), Err(V16Error::LockActive));
    assert_eq!(w.crank(PX).map(|_| ()), Err(V16Error::LockActive));
    assert_eq!(w.header, parked.header);
    assert_eq!(w.accounts[PX], parked.accounts[PX]);
    assert_eq!(settle_pledged_backing(w), PLEDGED_PAID);
}

/// Two flat lien holders pledge most of a domain against each other. Y1 and
/// Y2 each win 4 from H; Y1 pledges all 4, Y2 the 2 its claim is still worth,
/// and X adds 20 of claims that Z cannot pay. Domain 2 at resolution: 8 of
/// backing (6 pledged), 28 of claims.
///
/// Releasing only its own lien leaves Y1 at floor(4 * 6 / 28) = 0, so it has
/// to wait for Y2's release, where it converts 1. Its own release must still
/// commit: rolled back with the wait, the 4 atoms stay pledged, and whoever is
/// cranked next realizes against 2 (or 4) atoms instead of 8.
#[test]
fn waiting_lien_holder_commits_its_lien_release() {
    const Y1: usize = 1;
    const Y2: usize = 2;
    const MX: usize = 3;
    const MH: usize = 4;
    const MS: usize = 5;
    let mut w = two_asset_world(&[62, 8, 8, 1_000_000, 1_000_000, 1_000_000]);
    w.trade(1, MH, Y1, 8 * UNIT / 10);
    w.trade(1, MH, Y2, 8 * UNIT / 10);
    w.mark(&[95, 95]);
    for i in [MH, Y1, Y2] {
        w.on(i, |m, a| m.full_account_refresh_not_atomic(a).map(|_| ()))
            .unwrap();
    }
    assert_eq!(w.domain(2), (8, 8));
    // Initial margin at 95: 1.2 units need 12 (capital 8 + 4 of PnL), 1.0
    // units need 10 (capital 8 + 2 of PnL at the halved rate).
    w.trade_at(1, MH, Y1, 4 * UNIT / 10, 95).unwrap();
    w.trade_at(1, MH, Y2, 2 * UNIT / 10, 95).unwrap();
    w.trade_at(1, Y1, MH, 12 * UNIT / 10, 95).unwrap();
    w.trade_at(1, Y2, MH, 10 * UNIT / 10, 95).unwrap();
    let pledged = |w: &World, i: usize| {
        w.accounts[i].source_domains[0]
            .source_lien_counterparty_backing_num
            .get()
            / BOUND_SCALE
    };
    assert_eq!((pledged(&w, Y1), pledged(&w, Y2)), (4, 2));
    w.trade_at(0, Z, MS, 4 * UNIT, 95).unwrap();
    w.trade_at(1, Z, MX, 25 * UNIT / 10, 95).unwrap();
    for marks in [[91, 91], [87, 87], [83, 87], [79, 87]] {
        w.mark(&marks);
    }
    let resolved_slot = w.resolve();
    // Leave the lien release as the lien holders' only pre-payout step.
    for i in [Y1, Y2] {
        w.on(i, |m, a| {
            m.sync_account_fee_to_slot_not_atomic(a, resolved_slot, 0)
                .map(|_| ())
        })
        .unwrap();
    }
    for i in [MX, MS, Z, MH, MX, MS, Z, MH] {
        w.close(i).unwrap();
    }
    assert_eq!(w.blockers(), 0);
    assert_eq!(w.domain(2), (28, 8));
    assert_eq!(w.source(2).valid_liened_backing_num, 6 * BOUND_SCALE);

    assert_eq!(w.close(Y1), Ok(ResolvedCloseOutcomeV16::ProgressOnly));
    assert_eq!(pledged(&w, Y1), 0);
    assert_eq!(w.source(2).valid_liened_backing_num, 2 * BOUND_SCALE);
    assert_eq!(w.domain(2), (28, 8), "Y1's claim stays attributed");
    assert_eq!(w.close(Y1), Err(V16Error::LockActive));

    // Y2 releases and converts floor(4 * 8 / 28) = 1, Y1 floor(4 * 7 / 24) = 1,
    // X floor(20 * 6 / 20) = 6: all 8 atoms of backing are paid out.
    assert_eq!(
        w.close(Y2),
        Ok(ResolvedCloseOutcomeV16::Closed { payout: 9 })
    );
    assert_eq!(
        w.close(Y1),
        Ok(ResolvedCloseOutcomeV16::Closed { payout: 9 })
    );
    assert_eq!(
        w.close(MX),
        Ok(ResolvedCloseOutcomeV16::Closed { payout: 1_000_006 })
    );
    assert_eq!(w.domain(2), (0, 0));
    w.sweep(&[MX, Y1, Y2, MS, Z, MH]);
    assert_eq!(w.paid, vec![0, 9, 9, 1_000_006, 999_992, 1_000_062]);
}

mod every_close_order {
    use super::*;
    use proptest::prelude::*;

    /// Position sizes in base units: dust (0.2, 0.4), small and large.
    const SIZES: [u128; 6] = [
        UNIT / 5,
        2 * UNIT / 5,
        UNIT,
        10 * UNIT,
        200 * UNIT,
        1_000 * UNIT,
    ];

    #[derive(Clone, Debug)]
    struct Plan {
        /// One trade per short: (long account, short account, size index).
        trades: Vec<(usize, usize, usize)>,
        accounts: usize,
        up: bool,
        order: Vec<usize>,
        /// (number of close calls before it, atoms) of a third-party backing
        /// deposit into the winners' domain.
        deposit: Option<(usize, u128)>,
    }

    fn plan() -> impl Strategy<Value = Plan> {
        (1usize..=3, 1usize..=3).prop_flat_map(|(longs, shorts)| {
            let accounts = longs + shorts;
            (
                proptest::collection::vec((0..longs, 0..SIZES.len()), shorts),
                any::<bool>(),
                Just((0..accounts).collect::<Vec<_>>()).prop_shuffle(),
                proptest::option::of((0usize..12, prop_oneof![Just(1u128), Just(2), Just(7)])),
            )
                .prop_map(move |(pairs, up, order, deposit)| Plan {
                    trades: pairs
                        .into_iter()
                        .enumerate()
                        .map(|(s, (l, size))| (l, longs + s, size))
                        .collect(),
                    accounts,
                    up,
                    order,
                    deposit,
                })
        })
    }

    /// Runs `plan` and returns the world after every account was swept in the
    /// resolved slot, each account's exact K/F entitlement, the resolved slot
    /// and the atoms the third party managed to deposit.
    fn run(plan: &Plan) -> Result<(World, Vec<u128>, u64, u128), TestCaseError> {
        let mut w = World::new(1, plan.accounts, |_| {});
        for i in 0..plan.accounts {
            w.deposit(i, 1_000_000);
        }
        let mut position = vec![0i128; plan.accounts];
        for &(long, short, size) in &plan.trades {
            w.trade(0, long, short, SIZES[size]);
            position[long] += SIZES[size] as i128;
            position[short] -= SIZES[size] as i128;
        }
        w.mark(&[if plan.up { 105 } else { 95 }]);
        let resolved_slot = w.resolve();
        // Winners are paid floor(5 * q), losers pay ceil(5 * q).
        let expected: Vec<u128> = position
            .iter()
            .map(|&q| {
                let wins = (q > 0) == plan.up;
                let value = 5 * q.unsigned_abs();
                if wins {
                    1_000_000 + value / UNIT
                } else {
                    1_000_000 - value.div_ceil(UNIT)
                }
            })
            .collect();
        // Winners' claims are owed by the losing side's domain.
        let winners_domain = if plan.up { 1 } else { 0 };
        let mut calls = 0usize;
        let mut deposited = 0u128;
        for _ in 0..8 {
            for &i in &plan.order {
                if let Some((at, atoms)) = plan.deposit {
                    if at == calls
                        && w.exec(|m, _| {
                            m.deposit_fresh_counterparty_backing_not_atomic(
                                winners_domain,
                                atoms,
                                u64::MAX,
                            )
                        })
                        .is_ok()
                    {
                        deposited = atoms;
                    }
                }
                calls += 1;
                let closed = w.close(i);
                prop_assert!(
                    closed.is_ok(),
                    "close of account {} failed in the resolved slot: {:?}",
                    i,
                    closed
                );
                w.assert_conserved();
            }
        }
        Ok((w, expected, resolved_slot, deposited))
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        /// `seniority_world` under every close order. No close fails, every
        /// account is settled in the resolved slot, nothing is minted, and V
        /// is paid in full from its own domain whenever its claim was
        /// attributed before LV booked. (When LV books first its payment is
        /// junior residual from the start; that order dependence is separate
        /// from this issue.)
        #[test]
        fn backed_winner_keeps_its_seniority(
            order in Just(vec![Z, A, DUST, V, LV]).prop_shuffle(),
        ) {
            let mut w = seniority_world();
            let resolved_slot = w.header.resolved_slot.get();
            for _ in 0..8 {
                for &i in &order {
                    let closed = w.close(i);
                    prop_assert!(closed.is_ok(), "close of account {} failed: {:?}", i, closed);
                    w.assert_conserved();
                }
            }
            for &i in &order {
                w.topup(i);
            }
            for i in 0..5 {
                prop_assert!(w.settled(i), "account {} not settled in the resolved slot", i);
            }
            prop_assert_eq!(w.header.current_slot.get(), resolved_slot);
            let position = |i: usize| order.iter().position(|&a| a == i).unwrap();
            if position(V) < position(LV) {
                prop_assert_eq!(w.paid[V], 1_005_000);
            }
            prop_assert!(w.paid[V] <= 1_005_000);
            prop_assert!(w.paid[A] <= 1_013_000 && w.paid[DUST] <= 1_000_001);
            prop_assert_eq!((w.paid[Z], w.paid[LV]), (0, 995_000));
            let total: u128 = w.paid.iter().sum();
            prop_assert_eq!(total + w.header.vault.get(), 4_010_200);
        }

        /// Solvent single-asset worlds with dust and ordinary positions, any
        /// close order, optionally a few atoms of third-party backing dropped
        /// into the winners' domain mid-way. No close ever fails, every
        /// account is settled without moving the clock, and once the backing
        /// horizon has passed everybody has received exactly its K/F result.
        #[test]
        fn settles_in_the_resolved_slot_and_pays_exactly(plan in plan()) {
            let (mut w, expected, resolved_slot, deposited) = run(&plan)?;
            for i in 0..plan.accounts {
                prop_assert!(w.settled(i), "account {} not settled in the resolved slot", i);
                prop_assert!(w.paid[i] <= expected[i]);
                // In the slot an account can be short of its entitlement only by
                // junior face that waits for leftover backing to lapse.
                prop_assert!(w.paid[i] >= expected[i].min(1_000_000));
            }
            prop_assert_eq!(w.header.current_slot.get(), resolved_slot);

            w.advance(resolved_slot + HORIZON);
            w.expire_lapsed();
            w.sweep(&plan.order);
            prop_assert_eq!(&w.paid, &expected);
            let total: u128 = w.paid.iter().sum();
            prop_assert_eq!(
                w.header.vault.get(),
                plan.accounts as u128 * 1_000_000 + deposited - total
            );
            w.assert_conserved();
        }
    }
}
