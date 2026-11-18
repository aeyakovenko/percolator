//! Resolved close: a claim realized while a co-claimant's lien is outstanding.
//!
//! A lien takes its backing out of the available backing of its domain. Where the
//! domain's backing does not cover its claims, the unliened face then shares what is
//! left. Resolved close releases every lien before its holder realizes, so that state
//! lasts only until the lien holders have been closed. A claim realized in between was
//! paid from the unpledged remainder alone and the rest of its share went to the lien
//! holders: the order of two permissionless closes decided who was paid. Such a claim
//! now waits for the release, as a claim that converts to zero already did.
//!
//! Every world is built through the public `*_not_atomic` entry points (deposits,
//! trades, accruals, resolve, resolved closes), and a failed call is rolled back as the
//! program would roll back its instruction.
#![cfg(feature = "fuzz")]

use percolator::{
    EngineAssetSlotV16Account, Market, MarketGroupV16HeaderAccount, MarketGroupV16ViewMut,
    PortfolioAccountV16Account, PortfolioV16ViewMut, ProvenanceHeaderV16,
    ProvenanceHeaderV16Account, ResolvedCloseOutcomeV16, SourceCreditStateV16, TradeRequestV16,
    V16Config, V16Error, V16Result, BOUND_SCALE, POS_SCALE,
};

const UNIT: u128 = POS_SCALE;

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
    /// `assets` assets at 100, one account per deposit. Resolved backing lapses
    /// at `resolved_slot + 10` (`h_max`).
    fn new(assets: u32, deposits: &[u128], tweak: impl FnOnce(&mut V16Config)) -> Self {
        let mut cfg = V16Config::public_user_fund_with_market_slots(assets as u16, assets, 0, 10);
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
        let accounts = (0..deposits.len())
            .map(|i| {
                let provenance = ProvenanceHeaderV16Account::from_runtime(
                    &ProvenanceHeaderV16::new([1; 32], [40 + i as u8; 32], [3; 32]),
                );
                let mut account = PortfolioAccountV16Account::default();
                account.init_empty_in_place(provenance).unwrap();
                account
            })
            .collect::<Vec<_>>();
        let mut world = Self {
            header,
            markets,
            accounts,
            paid: vec![0; deposits.len()],
        };
        if assets > 1 {
            // Later assets were activated at later slots; bring asset 0 level.
            world
                .exec(|m, _| {
                    m.accrue_asset_to_not_atomic(0, assets as u64, 100, 0, true)
                        .map(|_| ())
                })
                .unwrap();
        }
        for (i, &amount) in deposits.iter().enumerate() {
            world.on(i, |m, a| m.deposit_not_atomic(a, amount)).unwrap();
        }
        world
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

    /// `long` buys `size_q` of `asset` from `short` at `price`.
    fn trade(&mut self, asset: usize, long: usize, short: usize, size_q: u128, price: u64) {
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
        .unwrap();
    }

    fn refresh(&mut self, i: usize) {
        self.on(i, |m, a| m.full_account_refresh_not_atomic(a).map(|_| ()))
            .unwrap();
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

    fn resolve(&mut self) {
        let slot = self.header.current_slot.get() + 1;
        self.exec(|m, _| m.resolve_market_not_atomic(slot)).unwrap();
    }

    /// One resolved-close instruction; any payout is credited to `paid[i]`. A
    /// waiting close (`LockActive`) is skipped like a keeper would; any other
    /// error fails the test.
    fn close(&mut self, i: usize) {
        match self.on(i, |m, a| m.close_resolved_account_not_atomic(a, 0)) {
            Ok(ResolvedCloseOutcomeV16::Closed { payout }) => self.paid[i] += payout,
            Ok(ResolvedCloseOutcomeV16::ProgressOnly) | Err(V16Error::LockActive) => {}
            Err(e) => panic!("close {i} failed: {e:?}"),
        }
        self.assert_conserved();
    }

    /// The engine's gate for realizing positive PnL in Resolved mode.
    fn payout_ready(&self) -> bool {
        let h = &self.header;
        h.resolved_payout_blocker_count.get() == 0
            && h.negative_pnl_account_count.get() == 0
            && h.b_stale_account_count.get() == 0
            && h.stale_certificate_count.get() == 0
    }

    /// Crank `order` round-robin until the group is payout-ready, and no
    /// further.
    fn settle_positions(&mut self, order: &[usize]) {
        for _ in 0..12 {
            for &i in order {
                if self.payout_ready() {
                    return;
                }
                self.close(i);
            }
        }
        assert!(self.payout_ready(), "positions did not settle");
    }

    /// Keeper sweep in the resolved slot: every account in `order`, twelve
    /// times, then every receipt top-up.
    fn sweep(mut self, order: &[usize]) -> Self {
        for _ in 0..12 {
            for &i in order {
                self.close(i);
            }
        }
        for &i in order {
            if let Ok(payout) = self.on(i, |m, a| m.claim_resolved_payout_topup_not_atomic(a)) {
                self.paid[i] += payout;
            }
        }
        self
    }

    fn source(&self, domain: usize) -> SourceCreditStateV16 {
        let slot = &self.markets[domain / 2].engine;
        if domain % 2 == 0 {
            slot.source_credit_long.try_to_runtime().unwrap()
        } else {
            slot.source_credit_short.try_to_runtime().unwrap()
        }
    }

    /// (claims, fresh backing) of a domain in whole atoms. Domain `2 * asset` is
    /// funded by that asset's losing longs, `2 * asset + 1` by its losing shorts.
    fn domain(&self, domain: usize) -> (u128, u128) {
        let s = self.source(domain);
        (
            s.positive_claim_bound_num / BOUND_SCALE,
            s.fresh_reserved_backing_num / BOUND_SCALE,
        )
    }

    /// The vault covers every senior class and the engine's own shape and
    /// account validations hold.
    fn assert_conserved(&self) {
        let h = &self.header;
        let senior = h.c_tot.get()
            + h.insurance.get()
            + h.backing_provider_earnings_total.get()
            + h.source_fresh_backing_total_num.get() / BOUND_SCALE;
        assert!(h.vault.get() >= senior, "vault below the senior stack");
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

fn margin_10_percent(cfg: &mut V16Config) {
    cfg.initial_margin_bps = 1_000;
    cfg.maintenance_margin_bps = 1_000;
}

const Z: usize = 0;
const Y: usize = 1;
const X: usize = 2;
const H: usize = 3;
const S: usize = 4;

/// Two assets, 10% margins. On asset 1, Y (10_200 of capital) shorts 1_000 against H at
/// 100. At 95 Y is owed 5_000 and H has paid 5_000 into domain 2. Y pledges `lien` of them
/// to carry `units` (initial margin `units * 9.5`), then goes flat; the lien stays. Z
/// (24_000 of capital) goes long 1_500 of asset 0 against S and 1_000 of asset 1 against
/// X, both at 95. Asset 0 falls to 79 and asset 1 to 87: Z owes 24_000 on asset 0, which
/// takes all it has, and nothing is left for the 8_000 it owes X. Once every position is
/// settled, domain 2 holds 5_000 of backing, `lien` of it pledged to Y, for claims of
/// 5_000 (Y) and 8_000 (X).
fn under_backed_domain_with_a_lien(units: u128, lien: u128) -> World {
    let mut w = World::new(
        2,
        &[24_000, 10_200, 1_000_000, 1_000_000, 1_000_000],
        margin_10_percent,
    );
    w.trade(1, H, Y, 1_000 * UNIT, 100);
    w.mark(&[95, 95]);
    w.refresh(H);
    w.refresh(Y);
    assert_eq!(w.domain(2), (5_000, 5_000));
    if lien != 0 {
        assert_eq!(units * 95 / 10, 10_200 + lien);
        w.trade(1, H, Y, (units - 1_000) * UNIT, 95);
        w.trade(1, Y, H, units * UNIT, 95);
    } else {
        w.trade(1, Y, H, 1_000 * UNIT, 95);
    }
    assert_eq!(w.source(2).valid_liened_backing_num, lien * BOUND_SCALE);
    w.trade(0, Z, S, 1_500 * UNIT, 95);
    w.trade(1, Z, X, 1_000 * UNIT, 95);
    for marks in [[91, 91], [87, 87], [83, 87], [79, 87]] {
        w.mark(&marks);
    }
    w.resolve();
    // Keepers settle the accounts with positions; flat Y is not touched.
    w.settle_positions(&[X, S, Z]);
    assert_eq!(w.domain(2), (13_000, 5_000));
    assert_eq!(w.source(2).valid_liened_backing_num, lien * BOUND_SCALE);
    assert_eq!((w.paid[X], w.paid[Y]), (0, 0));
    w
}

/// 5_000 over claims of 5_000 (Y) and 8_000 (X) is 1_923.08 and 3_076.92.
fn pro_rata(w: &World) -> bool {
    w.paid[Y].abs_diff(10_200 + 1_923) <= 1 && w.paid[X].abs_diff(1_000_000 + 3_077) <= 1
}

/// Whoever is closed first, and however much of the backing is pledged, Y and X share the
/// domain's 5_000 pro rata.
#[test]
fn claim_is_paid_its_share_whether_or_not_the_lien_holder_closed_first() {
    for (units, lien) in [(1_600, 5_000), (1_578, 4_791), (1_200, 1_200)] {
        for order in [[Y, X, S, Z, H], [X, Y, S, Z, H], [X, S, Z, H, Y]] {
            let w = under_backed_domain_with_a_lien(units, lien).sweep(&order);
            assert!(
                pro_rata(&w),
                "lien {lien}, close order {order:?}: Y and X must be paid 12_123 and \
                 1_003_077; paid as [Z, Y, X, H, S]: {:?}",
                w.paid
            );
            assert_eq!(
                w.header.vault.get(),
                0,
                "lien {lien}, close order {order:?}"
            );
        }
    }
}

/// The wait is for the release and nothing else. While Y has not been closed, X's close
/// changes nothing and fails as a waiting close does; Y's next close releases its lien, and
/// X then realizes in its next call.
#[test]
fn a_claim_waits_exactly_until_the_lien_is_released() {
    let mut w = under_backed_domain_with_a_lien(1_578, 4_791);
    for _ in 0..3 {
        let before = w.clone();
        assert_eq!(
            w.on(X, |m, a| m.close_resolved_account_not_atomic(a, 0)),
            Err(V16Error::LockActive)
        );
        assert_eq!(w.paid, before.paid);
        assert_eq!(w.domain(2), (13_000, 5_000));
    }
    // Y's close releases the lien and realizes Y's own share of the 5_000 in the same call.
    w.close(Y);
    assert_eq!(w.source(2).valid_liened_backing_num, 0);
    assert!(w.paid[Y].abs_diff(10_200 + 1_923) <= 1, "{:?}", w.paid);
    assert_eq!(w.paid[X], 0);
    w.close(X);
    assert!(pro_rata(&w), "{:?}", w.paid);
}

/// A lien holder that is not closed cannot hold the claim for ever: once the bucket has
/// lapsed no release can change what the claim gets, and X realizes without Y. (Past the
/// horizon every bucket's backing is pooled, so what each claimant is paid from then on is
/// the junior pool's rate, not this domain's.)
#[test]
fn a_waiting_claim_is_released_when_the_bucket_lapses() {
    let mut w = under_backed_domain_with_a_lien(1_578, 4_791);
    assert_eq!(
        w.on(X, |m, a| m.close_resolved_account_not_atomic(a, 0)),
        Err(V16Error::LockActive)
    );
    // Past the backing horizon (ten slots in this fixture); a keeper expires the bucket.
    let end = w.header.current_slot.get() + 40;
    w.exec(|m, _| m.advance_resolved_slot_not_atomic(end))
        .unwrap();
    w.exec(|m, _| m.expire_source_backing_bucket_not_atomic(2, end))
        .unwrap();
    for _ in 0..4 {
        w.close(X);
    }
    assert!(
        w.paid[X] > 1_000_000,
        "X is paid its capital and a first share without Y: {:?}",
        w.paid
    );
    assert_eq!(w.paid[Y], 0);
    let w = w.sweep(&[X, S, Z, H, Y]);
    assert!(w.paid[Y] > 10_200, "{:?}", w.paid);
    // Everything deposited has been paid out, to the rounding atoms of the pooled rate.
    let paid: u128 = w.paid.iter().sum();
    assert_eq!(paid + w.header.vault.get(), 24_000 + 10_200 + 3_000_000);
    assert!(w.header.vault.get() <= 2, "{:?}", w.paid);
}

/// Control, the same before and after: with no lien in the domain nothing waits.
#[test]
fn without_a_lien_nothing_waits() {
    for order in [[Y, X, S, Z, H], [X, Y, S, Z, H]] {
        let mut w = under_backed_domain_with_a_lien(1_000, 0);
        // The first claimant in the order realizes in its first calls.
        let first = order[0];
        for _ in 0..2 {
            w.close(first);
        }
        assert_ne!(w.paid[first], 0, "close order {order:?}");
        let w = w.sweep(&order);
        assert!(pro_rata(&w), "close order {order:?}: {:?}", w.paid);
    }
}
