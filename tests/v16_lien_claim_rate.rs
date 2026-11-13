//! Regression for percolator#250: a source-credit lien must not cut the credit rate that the
//! other claimants of its source domain are credited at.
//!
//! A lien pledges part of a domain's fresh backing to one claimant and locks at least as much
//! of that claimant's face (`required_face >= required_backing`). The locked face draws no
//! soft credit (spec: "Locked face claim MUST be excluded from soft maintenance credit"), so
//! the backing that is still unliened is owed only to the face that is still unliened:
//!
//! ```text
//! rate = min(1, (fresh - liened + free insurance credit) / (claims - liened))
//! ```
//!
//! With the liened face left in the denominator, a claim that is covered by its own lien is
//! counted against the remaining backing a second time: in a domain whose backing equals its
//! claims, a lien on a fraction `f` of the backing credits every other claimant at `1 - f`,
//! makes healthy accounts liquidatable, and burns the full face of anyone who converts.
//!
//! Every scenario is driven through the public `*_not_atomic` API: deposits, trades, oracle
//! accruals, a lien created by a real margin-consuming trade, conversions and closes. The
//! twin "control" world differs only in that the lien holder funds its extra margin with
//! capital instead of pledging its gain.
#![allow(dead_code)]

use percolator::*;

const MARKET_GROUP: [u8; 32] = [1; 32];
const OWNER: [u8; 32] = [3; 32];

const W1: usize = 0;
const W2: usize = 1;
const L: usize = 2;
const M: usize = 3;
const N: usize = 4;

/// Source domain of long profits on asset 0: (asset 0, Short).
const GAIN_DOMAIN: usize = 1;
/// Domain a long's loss on asset 0 is booked into: (asset 0, Long).
const LOSS_DOMAIN: usize = 0;

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

/// Engine state plus portfolios. Every engine call goes through `exec`, which restores the
/// prior state when the call fails, as the SVM transaction rollback the engine relies on
/// would.
#[derive(Clone)]
struct World {
    header: MarketGroupV16HeaderAccount,
    markets: Vec<Market<u64>>,
    accounts: Vec<PortfolioAccountV16Account>,
}

impl World {
    /// One asset activated at `price` under the stock public configuration
    /// `V16Config::public_user_fund_with_market_slots` (100% initial and maintenance margin).
    fn new(price: u64, portfolios: usize) -> Self {
        let config = V16Config::public_user_fund_with_market_slots(1, 1, 0, 10);
        let mut header =
            MarketGroupV16HeaderAccount::new_dynamic(MARKET_GROUP, config, 1, 0).unwrap();
        let mut markets = vec![Market::new(0u64, EngineAssetSlotV16Account::default())];
        header
            .activate_empty_asset_slot_not_atomic(0, &mut markets[0].engine, price, 1)
            .unwrap();
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

    fn asset(&self) -> AssetStateV16 {
        self.markets[0].engine.asset.try_to_runtime().unwrap()
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

    /// Decoding validates the stored rate against the engine's own formula.
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

    fn capital(&self, i: usize) -> u128 {
        self.accounts[i].capital.get()
    }

    fn pnl(&self, i: usize) -> i128 {
        self.accounts[i].pnl.get()
    }

    fn deposit(&mut self, i: usize, amount: u128) {
        self.on(i, |m, a| m.deposit_not_atomic(a, amount)).unwrap();
    }

    /// `long` buys `units` of asset 0 from `short` at `price`.
    fn trade(&mut self, long: usize, short: usize, units: u128, price: u64) -> V16Result<()> {
        self.trade_q(long, short, units * POS_SCALE, price)
    }

    /// `long` buys `size_q` base quantity (POS_SCALE per unit) from `short` at `price`.
    fn trade_q(&mut self, long: usize, short: usize, size_q: u128, price: u64) -> V16Result<()> {
        self.exec(|market, accounts| {
            let (l, s) = two(accounts, long, short);
            let request = TradeRequestV16 {
                asset_index: 0,
                size_q: size_q as i128,
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

    fn refresh(&mut self, i: usize) -> V16Result<HealthCertV16> {
        self.on(i, |m, a| m.full_account_refresh_not_atomic(a))
    }

    fn liquidate(&mut self, i: usize) -> V16Result<()> {
        self.on(i, |m, a| {
            m.liquidate_account_not_atomic(a, LiquidationRequestV16 { asset_index: 0 })
                .map(|_| ())
        })
    }

    fn convert(&mut self, i: usize) -> V16Result<u128> {
        self.on(i, |m, a| m.convert_released_pnl_to_capital_not_atomic(a))
    }

    fn close_resolved(&mut self, i: usize) -> V16Result<ResolvedCloseOutcomeV16> {
        self.on(i, |m, a| m.close_resolved_account_not_atomic(a, 0))
    }

    /// Close a resolved account, calling the bounded continuation until it reports `Closed`.
    fn close_resolved_fully(&mut self, i: usize) -> u128 {
        for _ in 0..8 {
            if let ResolvedCloseOutcomeV16::Closed { payout } = self.close_resolved(i).unwrap() {
                return payout;
            }
        }
        panic!("account {i} did not close");
    }

    /// The single public permissionless crank, given one oracle observation.
    fn crank(&mut self, i: usize) -> V16Result<AutoCrankResultV16> {
        let now_slot = self.slot();
        let price = self.asset().effective_price;
        self.on(i, |m, a| {
            let observations = [AutoCrankObservationV16 {
                asset_index: 0,
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

    /// Bring asset 0 up to the current slot without moving its price.
    fn catch_up(&mut self) {
        let price = self.asset().effective_price;
        while self.asset().slot_last < self.slot() {
            let now = self.slot();
            self.exec(|m, _| {
                m.accrue_asset_to_not_atomic(0, now, price, 0, true)
                    .map(|_| ())
            })
            .unwrap();
        }
    }

    /// Advance the clock to `to_slot`, one slot at a time, at the current price.
    fn tick_to(&mut self, to_slot: u64) {
        self.catch_up();
        let price = self.asset().effective_price;
        while self.slot() < to_slot {
            let next = self.slot() + 1;
            self.exec(|m, _| {
                m.accrue_asset_to_not_atomic(0, next, price, 0, true)
                    .map(|_| ())
            })
            .unwrap();
        }
    }

    /// Publish a new oracle target for asset 0 and accrue one slot to it.
    fn move_price(&mut self, price: u64) {
        self.catch_up();
        let next = self.slot() + 1;
        self.exec(|m, _| {
            m.set_asset_raw_oracle_target_not_atomic(0, price)?;
            m.accrue_asset_to_not_atomic(0, next, price, 0, true)
                .map(|_| ())
        })
        .unwrap();
    }

    fn resolve(&mut self) {
        let now = self.slot();
        self.exec(|m, _| m.resolve_market_not_atomic(now)).unwrap();
    }
}

/// Independent readings and checks, computed from account and domain state only.
impl World {
    /// (claims, fresh backing, liened backing) of a domain in whole atoms.
    fn domain_atoms(&self, domain: usize) -> (u128, u128, u128) {
        let s = self.source_credit(domain);
        for num in [
            s.positive_claim_bound_num,
            s.fresh_reserved_backing_num,
            s.valid_liened_backing_num,
        ] {
            assert_eq!(num % BOUND_SCALE, 0, "scenario keeps whole atoms");
        }
        (
            s.positive_claim_bound_num / BOUND_SCALE,
            s.fresh_reserved_backing_num / BOUND_SCALE,
            s.valid_liened_backing_num / BOUND_SCALE,
        )
    }

    fn rate(&self, domain: usize) -> u128 {
        self.source_credit(domain).credit_rate_num
    }

    /// Backing a domain can still credit to unliened face, BOUND_SCALE units.
    fn available_num(&self, domain: usize) -> u128 {
        let s = self.source_credit(domain);
        (s.fresh_reserved_backing_num - s.valid_liened_backing_num)
            + (s.insurance_credit_reserved_num
                - s.valid_liened_insurance_num
                - s.impaired_liened_insurance_num)
    }

    fn source_slot(&self, i: usize, domain: usize) -> Option<PortfolioSourceDomainV16Account> {
        self.accounts[i]
            .source_domains
            .iter()
            .copied()
            .find(|s| s.is_occupied() && s.domain.get() as usize == domain)
    }

    /// (locked face, effective reserve in atoms, pledged counterparty backing) of `i`'s lien.
    fn lien(&self, i: usize, domain: usize) -> (u128, u128, u128) {
        self.source_slot(i, domain).map_or((0, 0, 0), |s| {
            (
                s.source_claim_liened_num.get(),
                s.source_lien_effective_reserved.get(),
                s.source_lien_counterparty_backing_num.get(),
            )
        })
    }

    /// Whole atoms account `i` can be credited from `domain` right now: its valid lien
    /// reserve, plus its unliened face at the domain's stored rate, never more than the
    /// domain's unliened backing.
    fn creditable(&self, i: usize, domain: usize) -> u128 {
        let Some(s) = self.source_slot(i, domain) else {
            return 0;
        };
        let bucket = self.bucket(domain);
        let bucket_live =
            bucket.status == BackingBucketStatusV16::Fresh && bucket.expiry_slot > self.slot();
        let lien = if bucket_live {
            s.source_lien_effective_reserved.get()
        } else {
            s.source_lien_effective_reserved.get()
                - s.source_lien_counterparty_backing_num.get() / BOUND_SCALE
        };
        let unliened = s.source_claim_bound_num.get()
            - s.source_claim_liened_num.get()
            - s.source_claim_impaired_num.get();
        let soft = (unliened * self.rate(domain) / CREDIT_RATE_SCALE / BOUND_SCALE)
            .min(self.available_num(domain) / BOUND_SCALE);
        lien + soft
    }

    /// Property (a): everything the domain's claimants can be credited, liens included, is
    /// covered by the domain's fresh backing plus its unimpaired insurance credit.
    fn assert_domain_backs_all_credit(&self, domain: usize) {
        let s = self.source_credit(domain);
        let backing = (s.fresh_reserved_backing_num + s.insurance_credit_reserved_num
            - s.impaired_liened_insurance_num)
            / BOUND_SCALE;
        let credited: u128 = (0..self.accounts.len())
            .map(|i| self.creditable(i, domain))
            .sum();
        assert!(
            credited <= backing,
            "domain {domain} credits {credited} against {backing} of backing"
        );
    }

    /// Vault conservation over the senior stack, and both domains never over-credited.
    fn assert_sound(&self) {
        let h = &self.header;
        let senior = h.c_tot.get()
            + h.insurance.get()
            + h.backing_provider_earnings_total.get()
            + h.source_fresh_backing_total_num.get() / BOUND_SCALE;
        assert!(h.vault.get() >= senior, "vault below senior stack");
        let capital: u128 = self.accounts.iter().map(|a| a.capital.get()).sum();
        assert_eq!(h.c_tot.get(), capital, "c_tot drifted from account capital");
        for domain in [LOSS_DOMAIN, GAIN_DOMAIN] {
            let b = self.bucket(domain);
            let s = self.source_credit(domain);
            assert_eq!(
                s.fresh_reserved_backing_num,
                b.fresh_unliened_backing_num + b.valid_liened_backing_num
            );
            // The domain's liened backing is exactly what its claimants pledged, and no
            // claimant pledges more backing than the face it locked.
            let mut pledged = 0u128;
            for i in 0..self.accounts.len() {
                let (face, _, backing) = self.lien(i, domain);
                assert!(backing <= face, "lien pledges more backing than face");
                pledged += backing;
            }
            if b.status == BackingBucketStatusV16::Fresh {
                assert_eq!(s.valid_liened_backing_num, pledged);
            }
            self.assert_domain_backs_all_credit(domain);
        }
    }
}

/// The rate the header rule gives for whole-atom domain totals (no insurance credit).
fn expected_rate(claims: u128, fresh: u128, liened: u128) -> u128 {
    assert!(liened <= fresh && liened <= claims);
    if claims == 0 {
        return CREDIT_RATE_SCALE;
    }
    if fresh == liened {
        return 0;
    }
    if claims == liened {
        return CREDIT_RATE_SCALE;
    }
    ((fresh - liened) * CREDIT_RATE_SCALE / (claims - liened)).min(CREDIT_RATE_SCALE)
}

/// W1 and W2 are each long 10 against L from 100. After +10%, L's realised loss of 200 has
/// left its capital and is the fresh backing of `GAIN_DOMAIN`; each winner's gain of 100 is a
/// claim on it. The domain is exactly fully backed: 200 of backing for 200 of claims.
fn two_winners(w1_capital: u128) -> World {
    let mut w = World::new(100, 5);
    w.deposit(W1, w1_capital);
    w.deposit(W2, 1_000);
    w.deposit(L, 4_000);
    w.trade(W1, L, 10, 100).unwrap();
    w.trade(W2, L, 10, 100).unwrap();
    w.move_price(110);
    w.refresh(L).unwrap();
    w.refresh(W1).unwrap();
    w.refresh(W2).unwrap();
    assert_eq!(w.capital(L), 4_000 - 200);
    assert_eq!((w.pnl(W1), w.pnl(W2)), (100, 100));
    assert_eq!(w.domain_atoms(GAIN_DOMAIN), (200, 200, 0));
    assert_eq!(w.rate(GAIN_DOMAIN), CREDIT_RATE_SCALE);
    w.assert_sound();
    w
}

/// W2 adds one unit at 110 against M, so its initial margin becomes 11 * 110 = 1_210.
///
/// With `pledge`, W2 tops its capital up to 1_135 only, and the engine covers the remaining
/// 75 with a lien on 75 of W2's gain: 75 of the domain's backing becomes liened and 75 of
/// W2's face is locked. Without it (the control world) W2 tops up to the full 1_210 and no
/// lien is created. Positions and prices are identical in both worlds.
fn w2_adds_one_unit(w: &mut World, pledge: bool) {
    w.deposit(W2, if pledge { 135 } else { 210 });
    w.deposit(M, 110);
    w.trade(W2, M, 1, 110).unwrap();
    let expected_lien = if pledge {
        (75 * BOUND_SCALE, 75, 75 * BOUND_SCALE)
    } else {
        (0, 0, 0)
    };
    assert_eq!(w.lien(W2, GAIN_DOMAIN), expected_lien);
    assert_eq!(
        w.domain_atoms(GAIN_DOMAIN),
        (200, 200, if pledge { 75 } else { 0 })
    );
}

fn lien_and_control_worlds(w1_capital: u128) -> (World, World) {
    let mut pledged = two_winners(w1_capital);
    w2_adds_one_unit(&mut pledged, true);
    let mut control = two_winners(w1_capital);
    w2_adds_one_unit(&mut control, false);
    (pledged, control)
}

#[test]
fn lien_on_a_fully_backed_domain_leaves_the_other_claimant_at_full_rate() {
    // W1 holds 1_030 of capital and 100 of gain against a 1_100 maintenance requirement.
    let (mut w, mut control) = lien_and_control_worlds(1_030);

    // 125 of unliened backing is owed to 125 of unliened face (W1's 100, W2's other 25).
    assert_eq!(expected_rate(200, 200, 75), CREDIT_RATE_SCALE);
    assert_eq!(w.rate(GAIN_DOMAIN), expected_rate(200, 200, 75));
    assert_eq!(w.creditable(W1, GAIN_DOMAIN), 100);
    assert_eq!(w.creditable(W2, GAIN_DOMAIN), 75 + 25);
    w.assert_sound();

    // (b) W1's margin value does not depend on W2's choice to pledge its own gain.
    let cert = w.refresh(W1).unwrap();
    let control_cert = control.refresh(W1).unwrap();
    assert_eq!(control_cert.certified_equity, 1_030 + 100);
    assert_eq!(cert.certified_equity, control_cert.certified_equity);
    assert_eq!(cert.certified_maintenance_req, 1_100);
    assert_eq!(cert.certified_liq_deficit, 0);
    assert_eq!(control.liquidate(W1), Err(V16Error::NonProgress));
    assert_eq!(
        w.liquidate(W1),
        Err(V16Error::NonProgress),
        "another account's lien must not make W1 liquidatable"
    );

    // (c) W2's own margin value is unchanged by pledging: 1_135 of capital, 75 of lien
    // reserve and 25 of unliened gain, exactly its capital plus its fully backed gain.
    let cert = w.refresh(W2).unwrap();
    assert_eq!(cert.certified_equity, 1_135 + 100);
    assert_eq!(cert.certified_initial_req, 1_210);
    w.assert_sound();
}

#[test]
fn conversion_beside_a_live_lien_pays_the_full_backed_claim() {
    let (mut w, mut control) = lien_and_control_worlds(1_030);
    for world in [&mut w, &mut control] {
        world.deposit(N, 5_000);
        // W1 sells its 10 units to N at the mark: no loss, W1 is flat with 100 of gain.
        world.trade(N, W1, 10, 110).unwrap();
        assert_eq!(world.pnl(W1), 100);
    }

    // Control: the fully backed claim converts one for one.
    assert_eq!(control.convert(W1), Ok(100));
    assert_eq!((control.capital(W1), control.pnl(W1)), (1_130, 0));

    // (b) Beside W2's live lien W1 converts the same 100, not 100 * (1 - 75/200).
    assert_eq!(w.convert(W1), Ok(100));
    assert_eq!((w.capital(W1), w.pnl(W1)), (1_130, 0));
    // W1's 100 of face and 100 of backing left together; W2's 25 of unliened face is still
    // owed the 25 of unliened backing.
    assert_eq!(w.domain_atoms(GAIN_DOMAIN), (100, 100, 75));
    assert_eq!(w.rate(GAIN_DOMAIN), expected_rate(100, 100, 75));
    assert_eq!(w.rate(GAIN_DOMAIN), CREDIT_RATE_SCALE);
    w.assert_sound();

    // W2 goes flat. The crank un-pledges its lien; with no new backing, (d) the release
    // must not move the rate.
    w.trade(N, W2, 11, 110).unwrap();
    let rate_before_release = w.rate(GAIN_DOMAIN);
    assert_eq!(
        w.crank(W2).unwrap().selected,
        AutoCrankPlanV16::ReleaseSourceLiens
    );
    assert_eq!(w.lien(W2, GAIN_DOMAIN), (0, 0, 0));
    assert_eq!(w.domain_atoms(GAIN_DOMAIN), (100, 100, 0));
    assert_eq!(w.rate(GAIN_DOMAIN), rate_before_release);
    assert_eq!(w.convert(W2), Ok(100));
    assert_eq!((w.capital(W2), w.pnl(W2)), (1_135 + 100, 0));

    // (a) The 200 that L paid in reached the two winners, 100 each, and nothing is left
    // behind without a claim.
    assert_eq!(w.domain_atoms(GAIN_DOMAIN), (0, 0, 0));
    assert_eq!(
        w.source_credit(GAIN_DOMAIN).spent_backing_num,
        200 * BOUND_SCALE
    );
    assert_eq!(w.residual(), 0);
    w.assert_sound();
}

#[test]
fn a_loss_on_the_lien_holder_does_not_cut_the_other_claimant() {
    let (mut w, mut control) = lien_and_control_worlds(1_030);
    let mut w1_equity = [0i128; 2];
    for (world, equity) in [&mut w, &mut control].into_iter().zip(&mut w1_equity) {
        // Price -5: every long gives back 5 per unit. W1 settles first: it loses 50 of its
        // 100 gain and keeps 50, still fully backed.
        world.move_price(105);
        *equity = world.refresh(W1).unwrap().certified_equity;
        assert_eq!(world.pnl(W1), 50);
        assert_eq!(world.rate(GAIN_DOMAIN), CREDIT_RATE_SCALE);
        // W2 loses 55, which in the pledged world burns its 25 of unliened face and 30 of
        // its liened face.
        for i in [W2, L, M] {
            world.refresh(i).unwrap();
        }
        assert_eq!(world.pnl(W2), 45);
        assert_eq!(world.rate(GAIN_DOMAIN), CREDIT_RATE_SCALE);
        assert_eq!(world.refresh(W1).unwrap().certified_equity, *equity);
        world.assert_sound();
    }

    // (b) W1 is worth its capital plus its remaining 50 of gain in both worlds.
    assert_eq!(w1_equity, [1_030 + 50, 1_030 + 50]);

    // The backing behind the unliened face that the reversal cancelled (W1's 50 and W2's
    // 25) followed it to the shorts' domain at par, not at a lien-diluted rate.
    let (_, moved, _) = w.domain_atoms(LOSS_DOMAIN);
    assert!(
        moved >= 50 + 25,
        "only {moved} of backing followed 75 of cancelled unliened face"
    );
    // The lien shrank with the liened face that the loss burned and still covers the rest.
    let (face, effective, backing) = w.lien(W2, GAIN_DOMAIN);
    assert_eq!(face, backing);
    assert_eq!(effective * BOUND_SCALE, backing);
    assert!(effective <= 45);
}

#[test]
fn expiry_with_a_live_lien_never_raises_the_rate_and_matches_the_lien_free_world() {
    let (mut w, mut control) = lien_and_control_worlds(1_030);
    for world in [&mut w, &mut control] {
        let rate_before = world.rate(GAIN_DOMAIN);
        let available_before = world.available_num(GAIN_DOMAIN);
        let residual_before = world.residual();

        // The bucket's lifetime was fixed when L's loss first funded it. When it lapses the
        // crank expires it: unliened backing is forfeited and liened backing is impaired.
        let expiry = world.bucket(GAIN_DOMAIN).expiry_slot;
        world.tick_to(expiry);
        assert_eq!(
            world.crank(W1).unwrap().outcome,
            AutoCrankOutcomeV16::Progressed(
                PermissionlessProgressOutcomeV16::SourceBackingExpired {
                    domain: GAIN_DOMAIN as _
                }
            )
        );

        // (d) Expiry and lien impairment never raise available backing or the rate.
        assert!(world.available_num(GAIN_DOMAIN) <= available_before);
        assert!(world.rate(GAIN_DOMAIN) <= rate_before);
        assert_eq!(world.rate(GAIN_DOMAIN), 0);
        assert_eq!(
            world.source_credit(GAIN_DOMAIN).fresh_reserved_backing_num,
            0
        );
        // All 200 of L's payment is forfeited to the junior pool, liened or not.
        assert_eq!(world.residual(), residual_before + 200);
        world.assert_sound();
    }
    assert_eq!(
        w.source_credit(GAIN_DOMAIN).impaired_liened_backing_num,
        75 * BOUND_SCALE
    );

    // W1 is left with its capital in both worlds; W2's forfeited lien no longer counts.
    let cert = w.refresh(W1).unwrap();
    let control_cert = control.refresh(W1).unwrap();
    assert_eq!(control_cert.certified_equity, 1_030);
    assert_eq!(cert.certified_equity, control_cert.certified_equity);
    assert_eq!(w.refresh(W2).unwrap().certified_equity, 1_135);
    assert_eq!(w.lien(W2, GAIN_DOMAIN), (0, 0, 0));
    w.assert_sound();
}

#[test]
fn resolved_close_pays_every_claimant_in_full_in_either_order() {
    for first in [W1, W2] {
        let (mut w, _) = lien_and_control_worlds(1_030);
        w.resolve();
        for i in [L, M] {
            w.close_resolved_fully(i);
        }
        let second = if first == W1 { W2 } else { W1 };
        let mut payout = [0u128; 2];
        // Bounded continuations: the first winner may have to wait for the other's leg.
        for _ in 0..4 {
            for i in [first, second] {
                if let ResolvedCloseOutcomeV16::Closed { payout: paid } =
                    w.close_resolved(i).unwrap()
                {
                    payout[i] += paid;
                }
            }
        }
        assert_eq!(payout[W1], 1_030 + 100);
        assert_eq!(payout[W2], 1_135 + 100);
        assert_eq!(w.header.vault.get(), 0);
    }
}

#[test]
fn resolved_close_beside_a_flat_holders_lien_pays_the_full_backed_claim_at_once() {
    let (mut w, mut control) = lien_and_control_worlds(1_030);
    for world in [&mut w, &mut control] {
        world.deposit(N, 5_000);
        // Both winners sell out at the mark. In the pledged world W2 is flat but its lien
        // has not been normalized when the market resolves.
        world.trade(N, W1, 10, 110).unwrap();
        world.trade(N, W2, 11, 110).unwrap();
        world.resolve();
        for i in [L, M, N] {
            world.close_resolved_fully(i);
        }
    }
    assert_eq!(
        w.lien(W2, GAIN_DOMAIN),
        (75 * BOUND_SCALE, 75, 75 * BOUND_SCALE)
    );

    // W1 closes first, while W2's lien is still pledged.
    assert_eq!(
        control.close_resolved(W1),
        Ok(ResolvedCloseOutcomeV16::Closed { payout: 1_130 })
    );
    assert_eq!(
        w.close_resolved(W1),
        Ok(ResolvedCloseOutcomeV16::Closed { payout: 1_130 }),
        "W1's fully backed claim must not wait for W2's lien or for backing expiry"
    );
    assert_eq!(w.close_resolved_fully(W2), 1_135 + 100);
    assert_eq!(w.header.vault.get(), 0);
    assert_eq!(w.domain_atoms(GAIN_DOMAIN), (0, 0, 0));
}

#[test]
fn provider_withdrawal_watermark_is_unchanged_beside_a_lien() {
    let (mut w, _) = lien_and_control_worlds(1_030);
    // A provider adds 100 on top of the 200 that L paid in: 300 of backing, 200 of claims,
    // 75 of the backing pledged by W2's lien.
    let expiry = w.bucket(GAIN_DOMAIN).expiry_slot;
    w.exec(|m, _| m.deposit_fresh_counterparty_backing_not_atomic(GAIN_DOMAIN, 100, expiry))
        .unwrap();
    assert_eq!(w.domain_atoms(GAIN_DOMAIN), (200, 300, 75));
    assert_eq!(w.rate(GAIN_DOMAIN), CREDIT_RATE_SCALE);
    let withdraw = |w: &mut World, amount: u128| {
        w.exec(|m, _| m.withdraw_fresh_counterparty_backing_not_atomic(GAIN_DOMAIN, amount))
    };

    // The exit gate is the one master has: the unliened backing left behind (225 - x) must
    // cover the whole claim bound (200), lien-covered face included. The lien-aware rate
    // would still be full after a withdrawal of the whole 100 surplus; the gate does not
    // follow the rate.
    assert_eq!(withdraw(&mut w, 26), Err(V16Error::LockActive));
    assert_eq!(withdraw(&mut w, 25), Ok(()));
    assert_eq!(w.domain_atoms(GAIN_DOMAIN), (200, 275, 75));
    assert_eq!(w.rate(GAIN_DOMAIN), CREDIT_RATE_SCALE);
    assert_eq!(withdraw(&mut w, 1), Err(V16Error::LockActive));
    w.assert_sound();

    // Once the lien is released its backing is unliened again and the rest of the surplus
    // can leave; the domain ends exactly fully backed.
    w.deposit(N, 5_000);
    w.trade(N, W2, 11, 110).unwrap();
    w.crank(W2).unwrap();
    assert_eq!(w.domain_atoms(GAIN_DOMAIN), (200, 275, 0));
    assert_eq!(withdraw(&mut w, 76), Err(V16Error::LockActive));
    assert_eq!(withdraw(&mut w, 75), Ok(()));
    assert_eq!(w.domain_atoms(GAIN_DOMAIN), (200, 200, 0));
    assert_eq!(w.rate(GAIN_DOMAIN), CREDIT_RATE_SCALE);
    assert_eq!(withdraw(&mut w, 1), Err(V16Error::LockActive));
    w.assert_sound();
}

/// A domain that is genuinely under-backed when the lien is created: the first bucket lapsed
/// with 200 of claims outstanding, then a further +10% move made L pay in 220 against 220 of
/// new gains. 220 of backing now stands behind 420 of claims.
fn under_backed_world() -> World {
    let mut w = two_winners(1_400);
    let expiry = w.bucket(GAIN_DOMAIN).expiry_slot;
    w.tick_to(expiry);
    w.crank(W1).unwrap();
    assert_eq!(w.domain_atoms(GAIN_DOMAIN), (200, 0, 0));
    w.move_price(121);
    for i in [L, W1, W2] {
        w.refresh(i).unwrap();
    }
    assert_eq!((w.pnl(W1), w.pnl(W2)), (210, 210));
    assert_eq!(w.domain_atoms(GAIN_DOMAIN), (420, 220, 0));
    assert_eq!(w.rate(GAIN_DOMAIN), expected_rate(420, 220, 0));
    w.assert_sound();
    w
}

#[test]
fn lien_on_an_under_backed_domain_stays_conservative_and_is_full_once_backed() {
    let mut w = under_backed_world();
    let lien_free_rate = expected_rate(420, 220, 0);
    assert_eq!(lien_free_rate, 220 * CREDIT_RATE_SCALE / 420);
    // 210 of gain is worth floor(210 * rate) while the domain is under-backed.
    let haircut_gain = (210 * lien_free_rate / CREDIT_RATE_SCALE) as i128;
    assert_eq!(haircut_gain, 109);
    assert_eq!(
        w.refresh(W1).unwrap().certified_equity,
        1_400 + haircut_gain
    );

    // W2 adds one unit at 121 (initial margin 11 * 121 = 1_331) holding 1_271 of capital:
    // 60 is covered by a lien, which at the haircut rate locks ceil(60 / rate) of face.
    w.deposit(W2, 271);
    let w2_before = w.refresh(W2).unwrap().certified_equity;
    assert_eq!(w2_before, 1_271 + haircut_gain);
    w.deposit(M, 121);
    w.trade(W2, M, 1, 121).unwrap();
    let (face, effective, backing) = w.lien(W2, GAIN_DOMAIN);
    assert_eq!((effective, backing), (60, 60 * BOUND_SCALE));
    assert_eq!(
        face,
        (60 * BOUND_SCALE * CREDIT_RATE_SCALE).div_ceil(lien_free_rate)
    );
    assert!(face > backing);

    // 160 of unliened backing is spread over the 420 - 60 of claims the lien does not
    // cover one for one. The haircut part of W2's locked face (face - backing) stays in the
    // denominator, so the rate is conservative: never above the lien-free rate, and (a)
    // never crediting more than the backing.
    assert_eq!(w.domain_atoms(GAIN_DOMAIN), (420, 220, 60));
    assert_eq!(w.rate(GAIN_DOMAIN), expected_rate(420, 220, 60));
    assert_eq!(w.rate(GAIN_DOMAIN), 160 * CREDIT_RATE_SCALE / 360);
    w.assert_sound();
    // Known limit of the rule. The lien locked `face` of W2's claim but only `backing` of
    // it leaves the denominator (the domain keeps no per-claimant face), so while this lien
    // is outstanding W1's rate is below the lien-free rate. It is still above the 160 / 420
    // that counting the whole locked face against the unliened backing would give.
    assert!(w.rate(GAIN_DOMAIN) < lien_free_rate);
    assert!(w.rate(GAIN_DOMAIN) > 160 * CREDIT_RATE_SCALE / 420);
    assert_eq!(w.refresh(W1).unwrap().certified_equity, 1_400 + 93);
    // (c) W2 met its initial margin and gained no margin by pledging.
    let cert = w.refresh(W2).unwrap();
    assert_eq!(cert.certified_initial_req, 1_331);
    assert!(cert.certified_equity >= 1_331);
    assert!(cert.certified_equity <= w2_before);

    // A provider supplies the missing 200. The domain is now fully backed (420 for 420),
    // so every unliened claimant is at the full rate although W2's lien is outstanding.
    let expiry = w.bucket(GAIN_DOMAIN).expiry_slot;
    w.exec(|m, _| m.deposit_fresh_counterparty_backing_not_atomic(GAIN_DOMAIN, 200, expiry))
        .unwrap();
    assert_eq!(w.domain_atoms(GAIN_DOMAIN), (420, 420, 60));
    assert_eq!(expected_rate(420, 420, 60), CREDIT_RATE_SCALE);
    assert_eq!(w.rate(GAIN_DOMAIN), CREDIT_RATE_SCALE);
    assert_eq!(w.refresh(W1).unwrap().certified_equity, 1_400 + 210);
    w.assert_sound();
}

/// Random trading among four accounts on one asset: deposits, fills in both directions,
/// price moves, refreshes, conversions, cranks (which also expire lapsed backing and release
/// flat holders' liens) and liquidations. Calls the engine rejects are rolled back.
mod random_trading {
    use super::*;
    use proptest::prelude::*;

    /// Accounts 0..4 trade at full margin; 4 and 5 are deep makers that can take any side.
    const ACCOUNTS: usize = 6;

    fn full_margin_world() -> World {
        let mut w = World::new(100, ACCOUNTS);
        for i in 0..4 {
            w.deposit(i, 1_000);
        }
        for maker in 4..ACCOUNTS {
            w.deposit(maker, 100_000);
        }
        // Two longs against two shorts, 10 units each on 1_000 of capital: every later move
        // creates winners that can only add size by pledging their gain.
        w.trade(0, 1, 10, 100).unwrap();
        w.trade(2, 3, 10, 100).unwrap();
        w
    }

    #[derive(Clone, Copy, Debug)]
    enum Op {
        Deposit(usize, u128),
        /// Long, short, and the size in tenths of a unit.
        Trade(usize, usize, u128),
        /// Price change in percent; with the flag, every account is then refreshed, as
        /// keepers would, so the move's losses are booked as backing.
        Move(i64, bool),
        Refresh(usize),
        Convert(usize),
        Crank(usize),
        Liquidate(usize),
        /// A trader grows its position by this many tenths of a unit against a maker,
        /// holding only enough capital that this percentage of its soft credit is pledged.
        Pledge(usize, u128, u128),
    }

    fn op_strategy() -> impl Strategy<Value = Op> {
        let who = 0usize..ACCOUNTS;
        prop_oneof![
            3 => (who.clone(), 1u128..=120).prop_map(|(i, a)| Op::Deposit(i, a)),
            6 => (who.clone(), who.clone(), 1u128..=20).prop_map(|(l, s, t)| Op::Trade(l, s, t)),
            4 => (-10i64..=10, any::<bool>()).prop_map(|(p, settle)| Op::Move(p, settle)),
            5 => who.clone().prop_map(Op::Refresh),
            2 => who.clone().prop_map(Op::Convert),
            3 => who.clone().prop_map(Op::Crank),
            1 => who.prop_map(Op::Liquidate),
            4 => (0usize..4, 1u128..=20, 1u128..=100).prop_map(|(i, t, p)| Op::Pledge(i, t, p)),
        ]
    }

    impl World {
        fn apply(&mut self, op: Op) {
            // Rejections are ordinary (insufficient margin, stale certificate, lock, ...).
            match op {
                Op::Deposit(i, amount) => self.deposit(i, amount),
                Op::Trade(long, short, tenths) => {
                    if long != short {
                        let price = self.asset().effective_price;
                        let _ = self.trade_q(long, short, tenths * POS_SCALE / 10, price);
                    }
                }
                Op::Move(percent, settle) => {
                    let price = self.asset().effective_price as i64;
                    self.move_price((price + price * percent / 100).clamp(40, 250) as u64);
                    if settle {
                        for i in 0..ACCOUNTS {
                            let _ = self.refresh(i);
                        }
                    }
                }
                Op::Refresh(i) => {
                    let _ = self.refresh(i);
                }
                Op::Convert(i) => {
                    let _ = self.convert(i);
                }
                Op::Crank(i) => {
                    let _ = self.crank(i);
                }
                Op::Liquidate(i) => {
                    let _ = self.liquidate(i);
                }
                Op::Pledge(i, tenths, percent) => self.pledge(i, tenths, percent),
            }
        }

        fn pledge(&mut self, i: usize, tenths: u128, percent: u128) {
            if self.refresh(i).is_err() {
                return;
            }
            let position = self.accounts[i].legs[0].basis_pos_q.get();
            let domain = if position > 0 {
                GAIN_DOMAIN
            } else {
                LOSS_DOMAIN
            };
            let (_, lien, _) = self.lien(i, domain);
            let soft = match self.creditable(i, domain).checked_sub(lien) {
                Some(soft) if position != 0 && soft != 0 => soft,
                _ => return,
            };
            // Under the stock 100% margin the requirement is the position's notional.
            let price = self.asset().effective_price;
            let add_q = tenths * POS_SCALE / 10;
            let requirement =
                ((position.unsigned_abs() + add_q) * price as u128).div_ceil(POS_SCALE);
            let capital_wanted = requirement.saturating_sub(lien + (soft * percent / 100).max(1));
            if let Some(top_up) = capital_wanted.checked_sub(self.capital(i)) {
                if top_up != 0 {
                    self.deposit(i, top_up);
                }
            }
            let maker = 4;
            let (long, short) = if position > 0 { (i, maker) } else { (maker, i) };
            let _ = self.trade_q(long, short, add_q, price);
        }

        /// Returns whether some claimant currently pledges a lien on `domain`.
        fn check_domain(&self, domain: usize) -> Result<bool, TestCaseError> {
            let bucket = self.bucket(domain);
            let s = self.source_credit(domain);
            prop_assert_eq!(
                s.fresh_reserved_backing_num,
                bucket.fresh_unliened_backing_num + bucket.valid_liened_backing_num
            );
            if bucket.status == BackingBucketStatusV16::Fresh && bucket.expiry_slot <= self.slot() {
                // Lapsed and not yet expired: the engine refuses to read this domain's rate.
                return Ok(false);
            }
            let mut pledged = 0u128;
            let mut fractional = 0u128;
            for i in 0..self.accounts.len() {
                let (face, _, backing) = self.lien(i, domain);
                // A lien keeps whole atoms of backing up to the next atom above its face.
                prop_assert!(backing <= face.div_ceil(BOUND_SCALE) * BOUND_SCALE);
                fractional += u128::from(backing > face);
                pledged += backing;
            }
            let live = bucket.status == BackingBucketStatusV16::Fresh;
            if live {
                prop_assert_eq!(s.valid_liened_backing_num, pledged);
            }
            // (a) Liens plus soft credit on all unliened face fit in the backing.
            let backing = s.fresh_reserved_backing_num / BOUND_SCALE;
            let credited: u128 = (0..self.accounts.len())
                .map(|i| self.creditable(i, domain))
                .sum();
            prop_assert!(
                credited <= backing + fractional,
                "domain {} credits {} against {} of backing",
                domain,
                credited,
                backing
            );
            // A lien never cuts the rate of a domain whose backing covers its claims.
            if s.fresh_reserved_backing_num >= s.positive_claim_bound_num
                && self.available_num(domain) > 0
            {
                prop_assert_eq!(s.credit_rate_num, CREDIT_RATE_SCALE);
            }
            Ok(live && pledged != 0)
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        #[test]
        fn random_trading_never_over_credits_and_liens_never_cut_a_backed_domain(
            ops in prop::collection::vec(op_strategy(), 1..=60),
        ) {
            let mut w = full_margin_world();
            for op in ops {
                w.apply(op);
                let h = &w.header;
                let senior = h.c_tot.get()
                    + h.insurance.get()
                    + h.backing_provider_earnings_total.get()
                    + h.source_fresh_backing_total_num.get() / BOUND_SCALE;
                prop_assert!(h.vault.get() >= senior);
                for domain in [LOSS_DOMAIN, GAIN_DOMAIN] {
                    w.check_domain(domain)?;
                }
            }
        }
    }

    /// The generator does reach live liens: a fixed sequence in its alphabet pledges one.
    #[test]
    fn random_trading_alphabet_reaches_a_live_lien() {
        let mut w = full_margin_world();
        // +10%: account 0 gains 100 on 10 units. With 50 more capital it buys half a unit
        // from a maker: 10.5 * 110 = 1_155 of margin against 1_050 of capital, so 105 would
        // be needed and only 100 is there; a tenth of a unit less needs 94.
        for op in [
            Op::Move(10, false),
            Op::Refresh(1),
            Op::Refresh(3),
            Op::Refresh(0),
            Op::Refresh(2),
            Op::Deposit(0, 50),
            Op::Trade(0, 4, 5),
            Op::Trade(0, 4, 4),
        ] {
            w.apply(op);
        }
        assert!(w.check_domain(GAIN_DOMAIN).unwrap());
        assert_eq!(w.lien(0, GAIN_DOMAIN).1, 94);
    }
}

/// The same rule for a lien pledged from the domain's insurance-credit reservation.
///
/// The counterparty bucket has lapsed, so the winners' 200 of claims are backed only by 200
/// of reserved insurance credit (`reserve_insurance_credit_not_atomic` is test-API only).
/// W2's margin lien then pledges 75 of that reservation; the other 125 of credit is owed to
/// the 125 of face that is not locked.
#[cfg(feature = "fuzz")]
#[test]
fn insurance_backed_lien_leaves_the_other_claimant_at_full_rate() {
    let mut w = two_winners(1_030);
    let expiry = w.bucket(GAIN_DOMAIN).expiry_slot;
    w.tick_to(expiry);
    w.crank(W1).unwrap();
    assert_eq!(w.domain_atoms(GAIN_DOMAIN), (200, 0, 0));
    assert_eq!(w.rate(GAIN_DOMAIN), 0);
    w.exec(|m, _| {
        m.deposit_domain_insurance_not_atomic(GAIN_DOMAIN, 200)?;
        m.reserve_insurance_credit_not_atomic(GAIN_DOMAIN, 200 * BOUND_SCALE)
    })
    .unwrap();
    assert_eq!(w.rate(GAIN_DOMAIN), CREDIT_RATE_SCALE);
    assert_eq!(w.refresh(W1).unwrap().certified_equity, 1_030 + 100);

    w.deposit(W2, 135);
    w.deposit(M, 110);
    w.trade(W2, M, 1, 110).unwrap();
    let lien = w.source_slot(W2, GAIN_DOMAIN).unwrap();
    assert_eq!(lien.source_lien_effective_reserved.get(), 75);
    assert_eq!(
        lien.source_lien_insurance_backing_num.get(),
        75 * BOUND_SCALE
    );
    assert_eq!(lien.source_lien_counterparty_backing_num.get(), 0);
    let s = w.source_credit(GAIN_DOMAIN);
    assert_eq!(s.valid_liened_insurance_num, 75 * BOUND_SCALE);
    assert_eq!(s.insurance_credit_reserved_num, 200 * BOUND_SCALE);

    // 125 of free insurance credit over 200 - 75 of claims the lien does not cover.
    assert_eq!(s.credit_rate_num, CREDIT_RATE_SCALE);
    assert_eq!(w.refresh(W1).unwrap().certified_equity, 1_030 + 100);
    assert_eq!(w.liquidate(W1), Err(V16Error::NonProgress));
    assert_eq!(w.refresh(W2).unwrap().certified_equity, 1_135 + 100);
    w.assert_domain_backs_all_credit(GAIN_DOMAIN);
}

/// Kernel-level properties of the rate rule over random lien lifecycles of one domain.
///
/// The domain ledger is driven through the production delta kernels and the production rate
/// function. A small model keeps each claimant's face, locked face and pledged backing, so
/// that what the claimants can be credited is recomputed independently of the engine.
#[cfg(feature = "fuzz")]
mod rate_kernel {
    use percolator::v16::{
        kani_expected_source_credit_rate_num_for_state as rate_of,
        kani_prepare_source_positive_claim_burn_delta as burn_claim_bound,
    };
    use percolator::{
        BackingBucketStatusV16, BackingBucketV16, MarketGroupV16ViewMut, SourceCreditStateV16,
        BOUND_SCALE, CREDIT_RATE_SCALE,
    };
    use proptest::prelude::*;

    type G = MarketGroupV16ViewMut<'static, u64>;
    const EXPIRY: u64 = 100;
    const CLAIMANTS: usize = 3;

    #[derive(Clone, Copy, Debug, Default)]
    struct Claimant {
        /// Claim face, BOUND_SCALE units.
        claim: u128,
        /// Part of `claim` locked by the claimant's lien.
        locked: u128,
        /// Backing pledged to the lien (whole atoms, BOUND_SCALE units).
        pledged: u128,
    }

    impl Claimant {
        fn unliened(&self) -> u128 {
            self.claim - self.locked
        }
    }

    #[derive(Clone, Copy, Debug)]
    enum Op {
        /// A loser (or provider) pays whole atoms of backing in.
        Backing(u128),
        /// A claimant's gain is registered as claim face.
        Gain(usize, u128),
        /// A claimant pledges backing for this many atoms of effective credit.
        Lien(usize, u128),
        /// A claimant's lien is un-pledged.
        Release(usize),
        /// A claimant realizes up to this many atoms of unliened face at the rate.
        Convert(usize, u128),
        /// A claimant's face is burned by a loss, unliened first, then liened.
        Loss(usize, u128),
        /// The provider withdraws unliened backing, subject to the full-rate gate.
        Withdraw(u128),
        /// The bucket lapses.
        Expire,
    }

    #[derive(Clone, Copy, Debug)]
    struct Domain {
        bucket: BackingBucketV16,
        source: SourceCreditStateV16,
        who: [Claimant; CLAIMANTS],
        expired: bool,
    }

    /// The rule of the file header, written out independently of the engine.
    fn reference_rate(s: &SourceCreditStateV16) -> u128 {
        if s.positive_claim_bound_num == 0 {
            return CREDIT_RATE_SCALE;
        }
        let available = s.fresh_reserved_backing_num - s.valid_liened_backing_num;
        if available == 0 {
            return 0;
        }
        let unliened_claim = s
            .positive_claim_bound_num
            .saturating_sub(s.valid_liened_backing_num);
        if available >= unliened_claim {
            return CREDIT_RATE_SCALE;
        }
        available * CREDIT_RATE_SCALE / unliened_claim
    }

    fn unliened_claim(s: &SourceCreditStateV16) -> u128 {
        s.positive_claim_bound_num
            .saturating_sub(s.valid_liened_backing_num)
    }

    impl Domain {
        fn new() -> Self {
            Self {
                bucket: BackingBucketV16 {
                    market_id: 1,
                    ..BackingBucketV16::EMPTY
                },
                source: SourceCreditStateV16::EMPTY,
                who: [Claimant::default(); CLAIMANTS],
                expired: false,
            }
        }

        fn available(&self) -> u128 {
            self.source.fresh_reserved_backing_num - self.source.valid_liened_backing_num
        }

        /// Applies `op` if the engine would accept it. `None` leaves the domain unchanged.
        fn step(&mut self, op: Op) -> Option<()> {
            if self.expired {
                return None;
            }
            let mut next = *self;
            let rate = self.source.credit_rate_num;
            match op {
                Op::Backing(atoms) => {
                    (next.bucket, next.source) = G::kani_prepare_counterparty_backing_add_delta(
                        self.bucket,
                        self.source,
                        atoms * BOUND_SCALE,
                        0,
                        EXPIRY,
                    )
                    .ok()?;
                }
                Op::Gain(k, atoms) => {
                    if atoms == 0 {
                        return None;
                    }
                    next.who[k].claim += atoms * BOUND_SCALE;
                    next.source.positive_claim_bound_num += atoms * BOUND_SCALE;
                    next.source.exact_positive_claim_num += atoms * BOUND_SCALE;
                }
                Op::Lien(k, atoms) => {
                    let by_claim = self.who[k].unliened() * rate / CREDIT_RATE_SCALE / BOUND_SCALE;
                    let effective = atoms.min(by_claim).min(self.available() / BOUND_SCALE);
                    if effective == 0 {
                        return None;
                    }
                    let (face, backing) =
                        G::kani_source_credit_lien_amounts_for_effective(effective, rate).ok()?;
                    assert!(face <= self.who[k].unliened() && face >= backing);
                    (next.bucket, next.source) = G::kani_prepare_counterparty_lien_create_delta(
                        self.bucket,
                        self.source,
                        0,
                        backing,
                    )
                    .ok()?;
                    next.who[k].locked += face;
                    next.who[k].pledged += backing;
                }
                Op::Release(k) => {
                    if self.who[k].pledged == 0 {
                        return None;
                    }
                    (next.bucket, next.source) =
                        G::kani_prepare_counterparty_lien_terminal_release_delta(
                            self.bucket,
                            self.source,
                            self.who[k].pledged,
                        )
                        .ok()?;
                    next.who[k].locked = 0;
                    next.who[k].pledged = 0;
                }
                Op::Convert(k, atoms) => {
                    let by_claim = self.who[k].unliened() * rate / CREDIT_RATE_SCALE / BOUND_SCALE;
                    let converted = atoms.min(by_claim).min(self.available() / BOUND_SCALE);
                    if converted == 0 {
                        return None;
                    }
                    let (face, backing) =
                        G::kani_source_credit_lien_amounts_for_effective(converted, rate).ok()?;
                    assert!(face <= self.who[k].unliened());
                    let (bucket, source) = G::kani_prepare_counterparty_lien_create_delta(
                        self.bucket,
                        self.source,
                        0,
                        backing,
                    )
                    .ok()?;
                    let (bucket, source) =
                        G::kani_prepare_counterparty_lien_consume_delta(bucket, source, backing)
                            .ok()?;
                    next.bucket = bucket;
                    next.source = burn_claim_bound(source, face).ok()?;
                    next.who[k].claim -= face;
                }
                Op::Loss(k, atoms) => {
                    let burn = (atoms * BOUND_SCALE).min(self.who[k].claim);
                    if burn == 0 {
                        return None;
                    }
                    let liened_burn = burn - burn.min(self.who[k].unliened());
                    if liened_burn != 0 {
                        let release = G::kani_source_lien_backing_release_for_face_burn(
                            self.who[k].locked,
                            self.who[k].pledged,
                            liened_burn,
                        )
                        .ok()?;
                        (next.bucket, next.source) =
                            G::kani_prepare_counterparty_lien_terminal_release_delta(
                                self.bucket,
                                self.source,
                                release,
                            )
                            .ok()?;
                        next.who[k].locked -= liened_burn;
                        next.who[k].pledged -= release;
                    }
                    next.who[k].claim -= burn;
                    next.source = burn_claim_bound(next.source, burn).ok()?;
                }
                Op::Withdraw(atoms) => {
                    if atoms == 0 {
                        return None;
                    }
                    (next.bucket, next.source) =
                        G::kani_prepare_counterparty_backing_withdraw_delta(
                            self.bucket,
                            self.source,
                            atoms * BOUND_SCALE,
                        )
                        .ok()?;
                    // The engine's gate: the domain must still be at the full rate.
                    if rate_of(next.source).ok()? != CREDIT_RATE_SCALE {
                        return None;
                    }
                }
                Op::Expire => {
                    (next.bucket, next.source) = G::kani_prepare_counterparty_backing_expiry_delta(
                        self.bucket,
                        self.source,
                        EXPIRY,
                    )
                    .ok()?;
                    // Liens on the lapsed bucket are impaired: their face stays locked and
                    // their backing is forfeited.
                    for c in &mut next.who {
                        c.pledged = 0;
                    }
                    next.expired = true;
                }
            }
            next.source.credit_rate_num = rate_of(next.source).ok()?;
            *self = next;
            Some(())
        }

        fn check(&self, before: &Domain, op: Op) -> Result<(), TestCaseError> {
            let s = &self.source;
            let rate = s.credit_rate_num;
            prop_assert_eq!(rate, reference_rate(s));
            prop_assert!(rate <= CREDIT_RATE_SCALE);
            let total = s.fresh_reserved_backing_num;
            let liened = s.valid_liened_backing_num;
            let available = total - liened;
            let claims = s.positive_claim_bound_num;
            prop_assert_eq!(claims, self.who.iter().map(|c| c.claim).sum::<u128>());
            prop_assert_eq!(liened, self.who.iter().map(|c| c.pledged).sum::<u128>());
            prop_assert_eq!(
                self.bucket.fresh_unliened_backing_num + self.bucket.valid_liened_backing_num,
                total
            );

            // (a) Lien reserves plus soft credit on every claimant's unliened face fit in the
            // backing. A lien whose remaining locked face is a fraction of an atom below its
            // pledged whole atoms (the burn kernel keeps ceil(face) atoms) can overstate the
            // total by less than one atom.
            let fractional = self.who.iter().filter(|c| c.pledged > c.locked).count() as u128;
            let credited: u128 = self
                .who
                .iter()
                .map(|c| {
                    c.pledged / BOUND_SCALE
                        + (c.unliened() * rate / CREDIT_RATE_SCALE / BOUND_SCALE)
                            .min(available / BOUND_SCALE)
                })
                .sum();
            prop_assert!(
                credited <= total / BOUND_SCALE + fractional,
                "credited {} against backing {} after {:?}",
                credited,
                total / BOUND_SCALE,
                op
            );
            // The ledger-level form of the same bound holds without exception.
            prop_assert!(liened + rate * unliened_claim(s) / CREDIT_RATE_SCALE <= total);

            // A lien cannot cut the rate of a domain whose backing covers its claims.
            if total >= claims && available > 0 {
                prop_assert_eq!(rate, CREDIT_RATE_SCALE);
            }
            // No lien state lifts the rate above the lien-free rate.
            if claims > 0 {
                prop_assert!(rate <= (total * CREDIT_RATE_SCALE / claims).min(CREDIT_RATE_SCALE));
            }

            // (d) Step-wise monotonicity. A state without unliened claim bound has no
            // claimant the rate applies to, so it is not compared.
            let old = before.source.credit_rate_num;
            let comparable = unliened_claim(s) > 0 && unliened_claim(&before.source) > 0;
            match op {
                Op::Lien(..) | Op::Gain(..) => prop_assert!(rate <= old),
                Op::Expire => {
                    prop_assert!(rate <= old);
                    prop_assert!(available <= before.available());
                }
                Op::Backing(_) => prop_assert!(rate >= old),
                Op::Release(_) | Op::Convert(..) | Op::Loss(..) => {
                    prop_assert!(rate >= old || !comparable)
                }
                Op::Withdraw(_) => {
                    prop_assert_eq!(rate, CREDIT_RATE_SCALE);
                    // The gate never lets the provider leave a claim unbacked.
                    prop_assert!(total >= claims);
                }
            }
            // In a domain whose backing covers its claims, pledging and un-pledging move
            // nothing for the other claimants.
            if let Op::Lien(..) | Op::Release(_) = op {
                if total >= claims && available > 0 && before.available() > 0 {
                    prop_assert_eq!(rate, old);
                }
            }
            Ok(())
        }
    }

    fn op_strategy() -> impl Strategy<Value = Op> {
        let who = 0usize..CLAIMANTS;
        prop_oneof![
            3 => (0u128..=40).prop_map(Op::Backing),
            3 => (who.clone(), 0u128..=40).prop_map(|(k, a)| Op::Gain(k, a)),
            4 => (who.clone(), 1u128..=40).prop_map(|(k, a)| Op::Lien(k, a)),
            2 => who.clone().prop_map(Op::Release),
            3 => (who.clone(), 1u128..=40).prop_map(|(k, a)| Op::Convert(k, a)),
            3 => (who, 1u128..=40).prop_map(|(k, a)| Op::Loss(k, a)),
            2 => (1u128..=40).prop_map(Op::Withdraw),
            1 => Just(Op::Expire),
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(3000))]

        #[test]
        fn lien_lifecycle_never_over_credits_and_never_cuts_a_backed_domain(
            ops in prop::collection::vec(op_strategy(), 1..=40),
        ) {
            let mut domain = Domain::new();
            for op in ops {
                let before = domain;
                if domain.step(op).is_some() {
                    domain.check(&before, op)?;
                }
            }
        }
    }

    /// Deterministic witness that every step is reachable, including a lien created below
    /// the full rate, its partial burn down to a fractional locked face, and the gate.
    #[test]
    fn scripted_lifecycle_reaches_every_step() {
        let mut d = Domain::new();
        let apply = |d: &mut Domain, op: Op| {
            let before = *d;
            d.step(op).unwrap_or_else(|| panic!("{op:?} rejected"));
            d.check(&before, op).unwrap();
        };
        // 30 of backing for 40 of claims: rate 3/4.
        apply(&mut d, Op::Gain(0, 20));
        apply(&mut d, Op::Gain(1, 20));
        apply(&mut d, Op::Backing(30));
        assert_eq!(d.source.credit_rate_num, CREDIT_RATE_SCALE * 3 / 4);
        // Claimant 1 pledges 7 atoms, locking ceil(7 / 0.75) = 9 1/3 atoms of face.
        apply(&mut d, Op::Lien(1, 7));
        assert_eq!(d.who[1].pledged, 7 * BOUND_SCALE);
        assert_eq!(d.who[1].locked, (7 * BOUND_SCALE * 4).div_ceil(3));
        // 23 of unliened backing over 40 - 7 of claims not covered one for one.
        assert_eq!(d.source.credit_rate_num, 23 * CREDIT_RATE_SCALE / 33);
        apply(&mut d, Op::Convert(0, 5));
        // Converting below the full rate burns a fractional face, so claimant 1's claim is
        // no longer whole atoms. A loss then burns its unliened face and part of its locked
        // face, leaving a fractional locked face just below its 7 pledged atoms.
        apply(&mut d, Op::Convert(1, 2));
        assert_ne!(d.who[1].claim % BOUND_SCALE, 0);
        apply(&mut d, Op::Loss(1, 11));
        assert_eq!(d.who[1].unliened(), 0);
        assert_eq!(d.who[1].pledged, 7 * BOUND_SCALE);
        assert!(d.who[1].locked < 7 * BOUND_SCALE && d.who[1].locked > 6 * BOUND_SCALE);
        apply(&mut d, Op::Release(1));
        // Backing now exceeds the remaining claims: the provider can take the surplus only.
        apply(&mut d, Op::Backing(40));
        let surplus =
            (d.source.fresh_reserved_backing_num - d.source.positive_claim_bound_num) / BOUND_SCALE;
        assert!(surplus > 0);
        assert!(d.clone().step(Op::Withdraw(surplus + 1)).is_none());
        apply(&mut d, Op::Lien(0, 3));
        apply(&mut d, Op::Withdraw(surplus));
        assert_eq!(d.source.credit_rate_num, CREDIT_RATE_SCALE);
        apply(&mut d, Op::Expire);
        assert_ne!(d.bucket.status, BackingBucketStatusV16::Fresh);
        assert_eq!(d.source.credit_rate_num, 0);
    }

    /// The one place the bound is not exact. The lien-burn kernel keeps whole atoms of
    /// backing up to the next atom above a lien's remaining locked face, so a partially
    /// burned lien can pledge a fraction of an atom more than it locks. The domain cannot
    /// see that fraction, so soft credit summed over the claimants can overstate the
    /// backing by less than one atom per such lien. No claimant can draw the overstated
    /// atom: each credit is capped by the unliened backing when it is realized.
    #[test]
    fn fractional_liens_overstate_soft_credit_by_less_than_one_atom_each_and_never_overdraw() {
        let mut d = Domain::new();
        let apply = |d: &mut Domain, op: Op| {
            let before = *d;
            d.step(op).unwrap_or_else(|| panic!("{op:?} rejected"));
            d.check(&before, op).unwrap();
        };
        // Two claimants pledge below the full rate, convert a little (which leaves each a
        // fractional claim), and then lose all their unliened face and part of the locked.
        for op in [
            Op::Gain(0, 20),
            Op::Gain(1, 20),
            Op::Backing(30),
            Op::Lien(0, 7),
            Op::Lien(1, 7),
            Op::Convert(0, 1),
            Op::Convert(1, 1),
            Op::Loss(0, 12),
            Op::Loss(1, 12),
        ] {
            apply(&mut d, op);
        }
        for holder in [d.who[0], d.who[1]] {
            assert_eq!(holder.unliened(), 0);
            assert_eq!(holder.pledged, 7 * BOUND_SCALE);
            assert!(holder.locked < 7 * BOUND_SCALE && holder.locked > 6 * BOUND_SCALE);
        }
        // Two whole-atom unliened claims of 5, and a provider withdrawal that the gate
        // admits: total backing 23 still covers the 10 + two locked faces of claims.
        apply(&mut d, Op::Gain(2, 5));
        apply(&mut d, Op::Gain(0, 5));
        apply(&mut d, Op::Withdraw(5));
        let s = d.source;
        assert_eq!(s.fresh_reserved_backing_num, 23 * BOUND_SCALE);
        assert!(s.fresh_reserved_backing_num >= s.positive_claim_bound_num);
        assert_eq!(d.available(), 9 * BOUND_SCALE);
        assert_eq!(s.credit_rate_num, CREDIT_RATE_SCALE);

        // 14 pledged + 5 + 5 of soft credit against 23 of backing: one atom over, with two
        // fractional liens.
        let credited: u128 = d
            .who
            .iter()
            .map(|c| c.pledged / BOUND_SCALE + (c.unliened() / BOUND_SCALE).min(9))
            .sum();
        assert_eq!(credited, 23 + 1);

        // Realization pays out exactly the 9 of unliened backing: 5, then 4.
        let unliened_backing = d.available();
        apply(&mut d, Op::Convert(2, 5));
        assert_eq!(d.who[2].claim, 0);
        apply(&mut d, Op::Convert(0, 5));
        assert_eq!(d.who[0].unliened(), BOUND_SCALE);
        assert_eq!(d.available(), 0);
        assert_eq!(
            d.source.spent_backing_num,
            unliened_backing + 2 * BOUND_SCALE
        );
        // The atom is not lost: it is the backing the liens over-pledge, and it returns
        // when they are released.
        apply(&mut d, Op::Release(0));
        apply(&mut d, Op::Release(1));
        assert_eq!(d.source.credit_rate_num, CREDIT_RATE_SCALE);
        assert!(d.available() >= d.source.positive_claim_bound_num);
    }
}
