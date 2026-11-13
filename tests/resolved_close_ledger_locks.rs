//! Permanent resolved-close locks that come from the bankrupt-close ledger.
//!
//! A bankrupt close that cannot finish in one step leaves a pending close
//! ledger on the account and a pending-loss barrier on the domain it will
//! book to (the opposite side of the closed leg). While the barrier is up no
//! leg of that side may detach and no positive PnL in the group is paid. Only
//! the account that owns the ledger can take the barrier down.
//!
//! Cured close: the owner pays the loss itself (a deposit) before the residual
//! is booked. The Live crank then cures the ledger: nothing is booked, the
//! barrier comes down (issue #204). Resolved close skipped that step whenever
//! the principal covered the whole loss, paid the account out and left the
//! barrier up for good.
//!
//! Every call goes through the public `*_not_atomic` API. A failed call is
//! rolled back here, as the wrapper's transaction would roll it back.
use percolator::{
    AssetStateV16, AutoCrankOutcomeV16, AutoCrankWorkV16, CloseProgressLedgerV16,
    EngineAssetSlotV16Account, Market, MarketGroupV16HeaderAccount, MarketGroupV16ViewMut,
    PortfolioAccountV16Account, PortfolioLegV16, PortfolioV16ViewMut, ProvenanceHeaderV16,
    ProvenanceHeaderV16Account, ResolvedCloseOutcomeV16, SideV16, TradeRequestV16, V16Config,
    V16Error, POS_SCALE,
};

const PRICE: u64 = 1_000_000;
const LONG: SideV16 = SideV16::Long;
const SHORT: SideV16 = SideV16::Short;

struct Book {
    header: MarketGroupV16HeaderAccount,
    markets: Vec<Market<u64>>,
    accounts: Vec<PortfolioAccountV16Account>,
    slot: u64,
    price: u64,
    deposited: u128,
}

impl Book {
    /// One asset at `PRICE` (one quote atom per position atom), 10% initial
    /// and maintenance margin, at most 5% mark move per slot, and `accounts`
    /// empty portfolios. `b_chunk_atoms` caps how much residual one call may
    /// book (`None`: the default, which never binds here).
    fn new(accounts: usize, b_chunk_atoms: Option<u128>) -> Self {
        let mut cfg = V16Config::public_user_fund_with_market_slots(1, 1, 0, 10);
        cfg.initial_margin_bps = 1_000;
        cfg.maintenance_margin_bps = 1_000;
        cfg.max_price_move_bps_per_slot = 500;
        if let Some(atoms) = b_chunk_atoms {
            cfg.public_b_chunk_atoms = atoms;
        }
        let mut header = MarketGroupV16HeaderAccount::new_dynamic([1; 32], cfg, 1, 0).unwrap();
        let mut markets = vec![Market::new(0, EngineAssetSlotV16Account::default())];
        header
            .activate_empty_asset_slot_not_atomic(0, &mut markets[0].engine, PRICE, 1)
            .unwrap();
        let accounts = (0..accounts)
            .map(|seed| {
                let provenance = ProvenanceHeaderV16Account::from_runtime(
                    &ProvenanceHeaderV16::new([1; 32], [seed as u8 + 10; 32], [3; 32]),
                );
                let mut account = PortfolioAccountV16Account::default();
                account.init_empty_in_place(provenance).unwrap();
                account
            })
            .collect();
        Book {
            header,
            markets,
            accounts,
            slot: 1,
            price: PRICE,
            deposited: 0,
        }
    }

    /// Runs `f` as one transaction: an `Err` restores the pre-call state.
    fn tx<R>(&mut self, f: impl FnOnce(&mut Self) -> Result<R, V16Error>) -> Result<R, V16Error> {
        let before = (self.header, self.markets.clone(), self.accounts.clone());
        let result = f(self);
        if result.is_err() {
            (self.header, self.markets, self.accounts) = before;
        }
        result
    }

    fn with<R>(
        &mut self,
        account: usize,
        f: impl FnOnce(
            &mut MarketGroupV16ViewMut<'_, u64>,
            &mut PortfolioV16ViewMut<'_>,
        ) -> Result<R, V16Error>,
    ) -> Result<R, V16Error> {
        self.tx(|book| {
            let mut market = MarketGroupV16ViewMut::new(&mut book.header, &mut book.markets);
            let mut account = PortfolioV16ViewMut::new(&mut book.accounts[account]);
            f(&mut market, &mut account)
        })
    }

    fn market<R>(
        &mut self,
        f: impl FnOnce(&mut MarketGroupV16ViewMut<'_, u64>) -> Result<R, V16Error>,
    ) -> Result<R, V16Error> {
        self.tx(|book| {
            f(&mut MarketGroupV16ViewMut::new(
                &mut book.header,
                &mut book.markets,
            ))
        })
    }

    fn deposit(&mut self, account: usize, amount: u128) {
        self.with(account, |m, a| m.deposit_not_atomic(a, amount))
            .unwrap();
        self.deposited += amount;
    }

    /// `long` buys `q` from `short` at the current mark.
    fn trade(&mut self, long: usize, short: usize, q: u128) -> Result<(), V16Error> {
        assert_ne!(long, short);
        let exec_price = self.price;
        self.tx(|book| {
            let mut market = MarketGroupV16ViewMut::new(&mut book.header, &mut book.markets);
            let (low, high) = (long.min(short), long.max(short));
            let (left, right) = book.accounts.split_at_mut(high);
            let (low_account, high_account) = (&mut left[low], &mut right[0]);
            let (long_account, short_account) = if long < short {
                (low_account, high_account)
            } else {
                (high_account, low_account)
            };
            market
                .execute_trade_with_fee_loss_stale_scoped_not_atomic(
                    &mut PortfolioV16ViewMut::new(long_account),
                    &mut PortfolioV16ViewMut::new(short_account),
                    TradeRequestV16 {
                        asset_index: 0,
                        size_q: q as i128,
                        exec_price,
                        fee_bps: 0,
                    },
                )
                .map(|_| ())
        })
    }

    /// Moves the oracle target and the mark by 5% (the per-slot cap) in one
    /// slot.
    fn mark(&mut self, up: bool) {
        let delta = self.price / 20;
        let price = if up {
            self.price + delta
        } else {
            self.price - delta
        };
        let slot = self.slot + 1;
        self.market(|m| {
            m.set_asset_raw_oracle_target_not_atomic(0, price)?;
            m.accrue_asset_to_not_atomic(0, slot, price, 0, true)
                .map(|_| ())
        })
        .unwrap();
        self.slot = slot;
        self.price = price;
    }

    /// One step of the public permissionless crank at the current slot.
    fn crank(&mut self, account: usize) -> Result<AutoCrankOutcomeV16, V16Error> {
        self.with(account, |m, a| {
            let now_slot = m.header.current_slot.get();
            m.permissionless_auto_crank_not_atomic(
                a,
                AutoCrankWorkV16 {
                    now_slot,
                    observations: &[],
                    resolved_close_fee_rate_per_slot: 0,
                },
            )
            .map(|result| result.outcome)
        })
    }

    fn resolve(&mut self) {
        let slot = self.slot;
        self.market(|m| m.resolve_market_not_atomic(slot)).unwrap();
    }

    fn close(&mut self, account: usize) -> Result<ResolvedCloseOutcomeV16, V16Error> {
        self.with(account, |m, a| m.close_resolved_account_not_atomic(a, 0))
    }

    /// Round-robin resolved close over `order` (every account) until each
    /// has paid out and a whole round changes nothing more. `Closed` is not
    /// final: a later call pays a top-up once the other receipts have refined
    /// the payout rate, so an account's payouts are summed. A round that
    /// changes nothing while an account is still open is retried after the
    /// resolved clock moved one slot, so that a wait on a backing horizon is
    /// not mistaken for a lock; after `IDLE_ROUNDS` such rounds the state is
    /// dead and the last result of each open account is returned.
    fn drain(&mut self, order: &[usize]) -> Result<Vec<u128>, String> {
        const IDLE_ROUNDS: usize = 32;
        assert_eq!(order.len(), self.accounts.len());
        let mut payouts = vec![0u128; self.accounts.len()];
        let mut idle = 0;
        for _round in 0..4_096 {
            let before = (self.header, self.markets.clone(), self.accounts.clone());
            let mut open = Vec::new();
            for &account in order {
                match self.close(account) {
                    Ok(ResolvedCloseOutcomeV16::Closed { payout }) => payouts[account] += payout,
                    result => open.push((account, result)),
                }
            }
            if before != (self.header, self.markets.clone(), self.accounts.clone()) {
                idle = 0;
                continue;
            }
            if open.is_empty() {
                return Ok(payouts);
            }
            idle += 1;
            if idle == IDLE_ROUNDS {
                return Err(format!("no progress; open accounts {open:?}"));
            }
            let next = self.header.current_slot.get() + 1;
            self.market(|m| m.advance_resolved_slot_not_atomic(next))
                .unwrap();
        }
        Err("still changing after 4096 rounds".to_string())
    }

    fn asset(&self) -> AssetStateV16 {
        self.markets[0].engine.asset.try_to_runtime().unwrap()
    }

    fn leg(&self, account: usize) -> PortfolioLegV16 {
        self.accounts[account].legs[0].try_to_runtime().unwrap()
    }

    fn capital(&self, account: usize) -> u128 {
        self.accounts[account].capital.get()
    }

    fn pnl(&self, account: usize) -> i128 {
        self.accounts[account].pnl.get()
    }

    fn ledger(&self, account: usize) -> CloseProgressLedgerV16 {
        self.accounts[account]
            .close_progress
            .try_to_runtime()
            .unwrap()
    }

    /// The residual `account`'s close still has to book or have cured.
    fn pending_residual(&self, account: usize) -> u128 {
        let ledger = self.ledger(account);
        if ledger.active && !ledger.finalized && !ledger.canceled {
            ledger.residual_remaining
        } else {
            0
        }
    }

    /// What `account`'s close charged to others: booked to the opposite side,
    /// assigned as explicit loss, or drawn from insurance or support.
    fn charged_to_others(&self, account: usize) -> u128 {
        let ledger = self.ledger(account);
        ledger.b_loss_booked
            + ledger.explicit_loss_assigned
            + ledger.insurance_spent
            + ledger.support_consumed
    }

    /// Pending-loss barriers on the (long, short) domain.
    fn barriers(&self) -> (u64, u64) {
        let engine = &self.markets[0].engine;
        (
            engine.pending_domain_loss_barrier_long.get(),
            engine.pending_domain_loss_barrier_short.get(),
        )
    }

    fn vault(&self) -> u128 {
        self.header.vault.get()
    }

    #[track_caller]
    fn assert_valid(&mut self) {
        let market = MarketGroupV16ViewMut::new(&mut self.header, &mut self.markets);
        market.validate_shape().unwrap();
        for account in self.accounts.iter_mut() {
            PortfolioV16ViewMut::new(account)
                .validate_with_market(&market.as_view())
                .unwrap();
        }
    }

    /// Every account has left: no leg, effective OI, loss weight, barrier,
    /// payout blocker or negative PnL remains, and what was paid out plus
    /// what the vault still holds is what was deposited.
    #[track_caller]
    fn assert_settled(&mut self, payouts: &[u128]) {
        self.assert_valid();
        let asset = self.asset();
        assert_eq!(
            (
                asset.oi_eff_long_q,
                asset.oi_eff_short_q,
                asset.stored_pos_count_long,
                asset.stored_pos_count_short,
                asset.pending_obligation_count_long,
                asset.pending_obligation_count_short,
                asset.loss_weight_sum_long,
                asset.loss_weight_sum_short,
            ),
            (0, 0, 0, 0, 0, 0, 0, 0),
            "(oi long, oi short, stored long, stored short, pending long, pending short, \
             weight long, weight short)"
        );
        assert_eq!(self.barriers(), (0, 0));
        assert_eq!(self.header.resolved_payout_blocker_count.get(), 0);
        assert_eq!(self.header.negative_pnl_account_count.get(), 0);
        for account in 0..self.accounts.len() {
            assert_eq!(
                (
                    self.capital(account),
                    self.pnl(account),
                    self.pending_residual(account)
                ),
                (0, 0, 0),
                "account {account}: (capital, pnl, pending residual)"
            );
        }
        assert_eq!(
            payouts.iter().sum::<u128>() + self.vault(),
            self.deposited,
            "payouts {payouts:?} plus the vault must equal the deposits"
        );
    }
}

fn opposite(side: SideV16) -> SideV16 {
    match side {
        SideV16::Long => SideV16::Short,
        SideV16::Short => SideV16::Long,
    }
}

const LOT: u128 = POS_SCALE;
const OPEN_Q: u128 = 100 * LOT;
const TRADER: usize = 0;
const MAKER: usize = 1;
const TRADER_CAPITAL: u128 = 10_000_000; // exactly 10% of the 100-lot notional
const MAKER_CAPITAL: u128 = 1_000_000_000;

/// The trader's loss after three 5% steps against 100 lots opened at `PRICE`,
/// beyond the capital it posted.
fn unpaid_loss(trader_side: SideV16) -> u128 {
    let mut price = PRICE as u128;
    for _ in 0..3 {
        match trader_side {
            SideV16::Long => price -= price / 20,
            SideV16::Short => price += price / 20,
        }
    }
    100 * price.abs_diff(PRICE as u128) - TRADER_CAPITAL
}

/// The trader opens 100 lots on `trader_side` at 10% margin against the well
/// funded maker and the mark moves three 5% steps against it.
fn bankrupt_trader(trader_side: SideV16, b_chunk_atoms: Option<u128>) -> Book {
    let mut book = Book::new(2, b_chunk_atoms);
    book.deposit(TRADER, TRADER_CAPITAL);
    book.deposit(MAKER, MAKER_CAPITAL);
    match trader_side {
        SideV16::Long => book.trade(TRADER, MAKER, OPEN_Q).unwrap(),
        SideV16::Short => book.trade(MAKER, TRADER, OPEN_Q).unwrap(),
    }
    for _ in 0..3 {
        book.mark(trader_side == SHORT);
    }
    book
}

/// `bankrupt_trader`, after which the trader sells the whole position back to
/// the maker. That trade cannot book the unpaid loss; it leaves it as a
/// pending close on the trader with a barrier on the maker's domain.
fn pending_close_after_terminal_trade(trader_side: SideV16, b_chunk_atoms: Option<u128>) -> Book {
    let mut book = bankrupt_trader(trader_side, b_chunk_atoms);
    match trader_side {
        SideV16::Long => book.trade(MAKER, TRADER, OPEN_Q).unwrap(),
        SideV16::Short => book.trade(TRADER, MAKER, OPEN_Q).unwrap(),
    }
    let unpaid = unpaid_loss(trader_side);
    assert_eq!(
        (book.capital(TRADER), book.pnl(TRADER)),
        (0, -(unpaid as i128))
    );
    assert_eq!(book.pending_residual(TRADER), unpaid);
    assert!(!book.leg(TRADER).active);
    assert_eq!(book.ledger(TRADER).domain_side, opposite(trader_side));
    let barriers = match trader_side {
        SideV16::Long => (0, 1),
        SideV16::Short => (1, 0),
    };
    assert_eq!(book.barriers(), barriers);
    // The maker keeps a zero-basis leg that carries its loss weight until the
    // pending close is done.
    let maker_leg = book.leg(MAKER);
    assert!(maker_leg.active && maker_leg.basis_pos_q == 0 && maker_leg.loss_weight != 0);
    book.assert_valid();
    book
}

/// The maker's gain on the 100 lots, which is the trader's whole loss.
fn maker_gain(trader_side: SideV16) -> u128 {
    TRADER_CAPITAL + unpaid_loss(trader_side)
}

// ---------------------------------------------------------------------------
// Controls: these pass before and after the fix.
// ---------------------------------------------------------------------------

/// Nobody pays the unpaid loss: resolved close books it to the maker's side
/// and takes the barrier down. The maker's gain is cut by the booked loss.
#[test]
fn control_resolved_close_books_an_unpaid_pending_close() {
    for trader_side in [LONG, SHORT] {
        for order in [[TRADER, MAKER], [MAKER, TRADER]] {
            let case = format!("trader {trader_side:?}, close order {order:?}");
            let mut book = pending_close_after_terminal_trade(trader_side, None);
            let unpaid = unpaid_loss(trader_side);
            book.resolve();
            let payouts = book.drain(&order).unwrap_or_else(|e| panic!("{case}: {e}"));
            assert_eq!(
                payouts,
                [0, MAKER_CAPITAL + maker_gain(trader_side) - unpaid],
                "{case}"
            );
            let ledger = book.ledger(TRADER);
            assert!(ledger.active && ledger.finalized, "{case}: {ledger:?}");
            assert_eq!(ledger.b_loss_booked, unpaid, "{case}");
            assert_eq!(book.vault(), 0, "{case}");
            book.assert_settled(&payouts);
        }
    }
}

/// The owner pays the whole unpaid loss and the Live crank cures the close
/// before the market resolves (issue #204): nothing is booked, the barrier
/// comes down, and resolved close then pays the maker its whole gain. This is
/// the outcome the resolved-close cure below has to match.
#[test]
fn control_live_crank_cures_a_repaid_pending_close() {
    for trader_side in [LONG, SHORT] {
        for surplus in [0u128, 5] {
            let case = format!("trader {trader_side:?}, surplus {surplus}");
            let mut book = pending_close_after_terminal_trade(trader_side, None);
            let unpaid = unpaid_loss(trader_side);
            book.deposit(TRADER, unpaid + surplus);
            book.crank(TRADER).unwrap();
            assert_eq!(book.pending_residual(TRADER), 0, "{case}");
            assert_eq!(book.charged_to_others(TRADER), 0, "{case}");
            assert_eq!(book.barriers(), (0, 0), "{case}");
            assert_eq!(
                (book.capital(TRADER), book.pnl(TRADER)),
                (surplus, 0),
                "{case}"
            );
            book.resolve();
            let payouts = book
                .drain(&[MAKER, TRADER])
                .unwrap_or_else(|e| panic!("{case}: {e}"));
            assert_cured_payouts(&mut book, trader_side, surplus, &payouts, &case);
        }
    }
}

/// The owner paid its whole loss, so the maker keeps its whole gain. The
/// payout rate of a positive PnL claim rounds down: at most one atom of it
/// stays in the vault.
#[track_caller]
fn assert_cured_payouts(
    book: &mut Book,
    trader_side: SideV16,
    surplus: u128,
    payouts: &[u128],
    case: &str,
) {
    let maker_whole = MAKER_CAPITAL + maker_gain(trader_side);
    assert_eq!(payouts[TRADER], surplus, "{case}: trader payout");
    assert!(
        payouts[MAKER] == maker_whole || payouts[MAKER] + 1 == maker_whole,
        "{case}: maker payout {} of {maker_whole}",
        payouts[MAKER]
    );
    assert_eq!(book.charged_to_others(TRADER), 0, "{case}");
    assert!(book.vault() <= 1, "{case}: vault {}", book.vault());
    book.assert_settled(payouts);
}

// ---------------------------------------------------------------------------
// A cured pending close in resolved close.
// ---------------------------------------------------------------------------

/// The owner pays the whole unpaid loss, but the market resolves before any
/// crank touched the account. Resolved close must cure the pending close just
/// as the Live crank does: book nothing, take the barrier down, pay the owner
/// what it has left, and then pay the maker its whole gain.
///
/// Before the fix the owner was paid out with the close still pending. The
/// barrier then stayed up for good: the maker's leg could never detach, so
/// neither its capital nor its gain could leave.
#[test]
fn resolved_close_cures_a_pending_close_the_owner_repaid() {
    for trader_side in [LONG, SHORT] {
        for surplus in [0u128, 5] {
            // The wrapper only takes deposits while the market is Live; the
            // engine also takes them after resolution.
            for deposit_after_resolve in [false, true] {
                for order in [[TRADER, MAKER], [MAKER, TRADER]] {
                    let case = format!(
                        "trader {trader_side:?}, surplus {surplus}, deposit after resolve \
                         {deposit_after_resolve}, close order {order:?}"
                    );
                    let mut book = pending_close_after_terminal_trade(trader_side, None);
                    let unpaid = unpaid_loss(trader_side);
                    if !deposit_after_resolve {
                        book.deposit(TRADER, unpaid + surplus);
                    }
                    book.resolve();
                    if deposit_after_resolve {
                        book.deposit(TRADER, unpaid + surplus);
                    }
                    let payouts = book.drain(&order).unwrap_or_else(|e| panic!("{case}: {e}"));
                    // Cured with nothing booked: the ledger is left exactly as
                    // an owner cure-and-cancel leaves it.
                    let ledger = book.ledger(TRADER);
                    assert!(
                        ledger.canceled && !ledger.active && !ledger.finalized,
                        "{case}: {ledger:?}"
                    );
                    assert_eq!(
                        (ledger.gross_loss_at_close_start, ledger.residual_remaining),
                        (0, 0),
                        "{case}"
                    );
                    assert_cured_payouts(&mut book, trader_side, surplus, &payouts, &case);
                }
            }
        }
    }
}

/// The step itself: the call that finds the loss fully paid cures the close
/// before it pays the owner, and moves no value doing so.
#[test]
fn resolved_close_never_pays_out_over_a_pending_close() {
    for trader_side in [LONG, SHORT] {
        let case = format!("trader {trader_side:?}");
        let mut book = pending_close_after_terminal_trade(trader_side, None);
        let unpaid = unpaid_loss(trader_side);
        book.deposit(TRADER, unpaid + 5);
        book.resolve();
        let (vault, c_tot, insurance) = (
            book.vault(),
            book.header.c_tot.get(),
            book.header.insurance.get(),
        );
        let blockers = book.header.resolved_payout_blocker_count.get();
        assert_eq!(
            book.close(TRADER),
            Ok(ResolvedCloseOutcomeV16::Closed { payout: 5 }),
            "{case}"
        );
        assert_eq!(book.pending_residual(TRADER), 0, "{case}");
        assert_eq!(book.charged_to_others(TRADER), 0, "{case}");
        assert_eq!(book.barriers(), (0, 0), "{case}");
        // The barrier was one payout blocker; the maker's leg is the other.
        assert_eq!(
            book.header.resolved_payout_blocker_count.get(),
            blockers - 1,
            "{case}"
        );
        // The loss left the owner's capital into the pool that backs the
        // maker's gain; the payout took the rest. Insurance is untouched.
        assert_eq!(
            (
                book.vault(),
                book.header.c_tot.get(),
                book.header.insurance.get()
            ),
            (vault - 5, c_tot - unpaid - 5, insurance),
            "{case}: (vault, c_tot, insurance)"
        );
        book.assert_valid();
        let mut payouts = book
            .drain(&[TRADER, MAKER])
            .unwrap_or_else(|e| panic!("{case}: {e}"));
        payouts[TRADER] += 5;
        assert_cured_payouts(&mut book, trader_side, 5, &payouts, &case);
    }
}

/// With a small booking chunk the Live crank books part of the unpaid loss
/// before the owner pays the rest. Resolved close then finalizes the close
/// with exactly the booked part charged to the maker's side.
///
/// Before the fix the barrier stayed up for good.
#[test]
fn resolved_close_finalizes_a_partly_booked_close_the_owner_repaid() {
    const CHUNK: u128 = 1_000_000;
    for trader_side in [LONG, SHORT] {
        for order in [[TRADER, MAKER], [MAKER, TRADER]] {
            let case = format!("trader {trader_side:?}, close order {order:?}");
            let mut book = pending_close_after_terminal_trade(trader_side, Some(CHUNK));
            let unpaid = unpaid_loss(trader_side);
            book.crank(TRADER).unwrap();
            assert_eq!(book.pending_residual(TRADER), unpaid - CHUNK, "{case}");
            assert_eq!(book.ledger(TRADER).b_loss_booked, CHUNK, "{case}");
            book.deposit(TRADER, unpaid - CHUNK + 5);
            book.resolve();
            let payouts = book.drain(&order).unwrap_or_else(|e| panic!("{case}: {e}"));
            let ledger = book.ledger(TRADER);
            assert!(
                ledger.active && ledger.finalized && !ledger.canceled,
                "{case}: {ledger:?}"
            );
            assert_eq!(
                (
                    ledger.gross_loss_at_close_start,
                    ledger.b_loss_booked,
                    ledger.residual_remaining
                ),
                (CHUNK, CHUNK, 0),
                "{case}"
            );
            assert_eq!(book.charged_to_others(TRADER), CHUNK, "{case}");
            let maker_whole = MAKER_CAPITAL + maker_gain(trader_side) - CHUNK;
            assert_eq!(payouts[TRADER], 5, "{case}");
            assert!(
                payouts[MAKER] == maker_whole || payouts[MAKER] + 1 == maker_whole,
                "{case}: maker payout {} of {maker_whole}",
                payouts[MAKER]
            );
            book.assert_settled(&payouts);
        }
    }
}

/// The same with the close begun by resolved close itself: a first call books
/// one chunk and leaves the rest pending on the still attached leg, then the
/// owner pays the rest (engine API only: the wrapper takes no deposit after
/// resolution).
///
/// Before the fix the owner's own leg could not detach either, so the owner's
/// remaining capital was locked along with the maker.
#[test]
fn resolved_close_finalizes_its_own_chunked_close_the_owner_repaid() {
    const CHUNK: u128 = 1_000_000;
    for trader_side in [LONG, SHORT] {
        let case = format!("trader {trader_side:?}");
        let mut book = bankrupt_trader(trader_side, Some(CHUNK));
        let unpaid = unpaid_loss(trader_side);
        book.resolve();
        assert_eq!(
            book.close(TRADER),
            Ok(ResolvedCloseOutcomeV16::ProgressOnly),
            "{case}"
        );
        assert_eq!(book.pending_residual(TRADER), unpaid - CHUNK, "{case}");
        assert!(book.leg(TRADER).active, "{case}");
        book.deposit(TRADER, unpaid - CHUNK + 5);
        let payouts = book
            .drain(&[TRADER, MAKER])
            .unwrap_or_else(|e| panic!("{case}: {e}"));
        assert_eq!(book.charged_to_others(TRADER), CHUNK, "{case}");
        let maker_whole = MAKER_CAPITAL + maker_gain(trader_side) - CHUNK;
        assert_eq!(payouts[TRADER], 5, "{case}");
        assert!(
            payouts[MAKER] == maker_whole || payouts[MAKER] + 1 == maker_whole,
            "{case}: maker payout {} of {maker_whole}",
            payouts[MAKER]
        );
        book.assert_settled(&payouts);
    }
}

/// A leaked barrier holds more than the two accounts of the close: no
/// positive PnL in the group is paid while a payout blocker is up. Here a
/// bystander pair traded and went flat before the trader's close began; the
/// winner of that pair has to be paid as well.
#[test]
fn resolved_close_cure_unblocks_every_positive_pnl_account() {
    const WINNER: usize = 2;
    const LOSER: usize = 3;
    for trader_side in [LONG, SHORT] {
        let case = format!("trader {trader_side:?}");
        let mut book = Book::new(4, None);
        book.deposit(TRADER, TRADER_CAPITAL);
        book.deposit(MAKER, MAKER_CAPITAL);
        book.deposit(WINNER, MAKER_CAPITAL);
        book.deposit(LOSER, MAKER_CAPITAL);
        // The winner is on the maker's side, the loser on the trader's.
        match trader_side {
            SideV16::Long => {
                book.trade(TRADER, MAKER, OPEN_Q).unwrap();
                book.trade(LOSER, WINNER, 10 * LOT).unwrap();
            }
            SideV16::Short => {
                book.trade(MAKER, TRADER, OPEN_Q).unwrap();
                book.trade(WINNER, LOSER, 10 * LOT).unwrap();
            }
        }
        for _ in 0..3 {
            book.mark(trader_side == SHORT);
        }
        // The bystanders close against each other, then the trader sells out.
        match trader_side {
            SideV16::Long => {
                book.trade(WINNER, LOSER, 10 * LOT).unwrap();
                book.trade(MAKER, TRADER, OPEN_Q).unwrap();
            }
            SideV16::Short => {
                book.trade(LOSER, WINNER, 10 * LOT).unwrap();
                book.trade(TRADER, MAKER, OPEN_Q).unwrap();
            }
        }
        let unpaid = unpaid_loss(trader_side);
        let bystander_gain = maker_gain(trader_side) / 10;
        assert_eq!(book.pending_residual(TRADER), unpaid, "{case}");
        assert!(
            !book.leg(WINNER).active && !book.leg(LOSER).active,
            "{case}"
        );
        assert_eq!(book.pnl(WINNER), bystander_gain as i128, "{case}");
        book.deposit(TRADER, unpaid);
        book.resolve();
        let payouts = book
            .drain(&[WINNER, LOSER, MAKER, TRADER])
            .unwrap_or_else(|e| panic!("{case}: {e}"));
        assert_eq!(book.charged_to_others(TRADER), 0, "{case}");
        assert_eq!(payouts[TRADER], 0, "{case}");
        assert_eq!(payouts[LOSER], MAKER_CAPITAL - bystander_gain, "{case}");
        for (account, gain) in [(MAKER, maker_gain(trader_side)), (WINNER, bystander_gain)] {
            let whole = MAKER_CAPITAL + gain;
            assert!(
                payouts[account] == whole || payouts[account] + 1 == whole,
                "{case}: account {account} payout {} of {whole}",
                payouts[account]
            );
        }
        book.assert_settled(&payouts);
    }
}

// ---------------------------------------------------------------------------
// A loss that arrives after the account's close was finalized.
// ---------------------------------------------------------------------------

const FIRST: usize = 0;
const FIRST_MAKER: usize = 1;
const SECOND_MAKER: usize = 2;
const SECOND: usize = 3;

/// Two bankruptcies on opposite sides of one asset, still Live.
///
/// FIRST opens 100 lots on `first_side` at 10% margin against FIRST_MAKER,
/// SECOND opens 100 lots on the other side at 10% margin against SECOND_MAKER.
/// Three 5% steps bankrupt FIRST, which sells out to FIRST_MAKER: its unpaid
/// loss is left as a pending close with a barrier on SECOND's side, carried by
/// FIRST_MAKER's obligation leg and SECOND's leg in equal parts. Six 5% steps
/// back then bankrupt SECOND.
fn two_bankruptcies(first_side: SideV16, b_chunk_atoms: Option<u128>) -> Book {
    let mut book = Book::new(4, b_chunk_atoms);
    book.deposit(FIRST, TRADER_CAPITAL);
    book.deposit(FIRST_MAKER, MAKER_CAPITAL);
    book.deposit(SECOND_MAKER, MAKER_CAPITAL);
    book.deposit(SECOND, TRADER_CAPITAL);
    match first_side {
        SideV16::Long => {
            book.trade(FIRST, FIRST_MAKER, OPEN_Q).unwrap();
            book.trade(SECOND_MAKER, SECOND, OPEN_Q).unwrap();
        }
        SideV16::Short => {
            book.trade(FIRST_MAKER, FIRST, OPEN_Q).unwrap();
            book.trade(SECOND, SECOND_MAKER, OPEN_Q).unwrap();
        }
    }
    for _ in 0..3 {
        book.mark(first_side == SHORT);
    }
    match first_side {
        SideV16::Long => book.trade(FIRST_MAKER, FIRST, OPEN_Q).unwrap(),
        SideV16::Short => book.trade(FIRST, FIRST_MAKER, OPEN_Q).unwrap(),
    }
    for _ in 0..6 {
        book.mark(first_side == LONG);
    }
    assert_eq!(book.pending_residual(FIRST), unpaid_loss(first_side));
    assert_eq!(book.ledger(FIRST).domain_side, opposite(first_side));
    assert!(book.leg(SECOND).active && book.leg(SECOND_MAKER).active);
    book.assert_valid();
    book
}

/// SECOND's loss on its own 100 lots beyond the capital it posted: the mark
/// went three 5% steps its way and then six 5% steps against it.
fn second_unpaid_loss(first_side: SideV16) -> u128 {
    let mut price = PRICE as u128;
    for step in 0..9 {
        let down = (step < 3) == (first_side == LONG);
        if down {
            price -= price / 20;
        } else {
            price += price / 20;
        }
    }
    100 * price.abs_diff(PRICE as u128) - TRADER_CAPITAL
}

/// What the two makers are paid when each bankrupt's unpaid loss is booked to
/// the legs of its opposite side. FIRST's loss falls half on FIRST_MAKER and
/// half on SECOND; SECOND cannot pay its half either, so that half and
/// SECOND's own unpaid loss fall on SECOND_MAKER. Each maker's positive PnL
/// payout rounds down by at most one atom.
fn booked_maker_payouts(first_side: SideV16) -> [u128; 2] {
    let half = unpaid_loss(first_side) / 2;
    assert_eq!(2 * half, unpaid_loss(first_side));
    [
        MAKER_CAPITAL + TRADER_CAPITAL + half,
        MAKER_CAPITAL + TRADER_CAPITAL - half,
    ]
}

#[track_caller]
fn assert_two_bankruptcies_settled(book: &mut Book, payouts: &[u128], case: &str) {
    assert_eq!((payouts[FIRST], payouts[SECOND]), (0, 0), "{case}");
    for maker in [FIRST_MAKER, SECOND_MAKER] {
        assert!(
            payouts[maker] >= MAKER_CAPITAL,
            "{case}: maker {maker} was paid {} of its own {MAKER_CAPITAL}",
            payouts[maker]
        );
    }
    assert!(book.vault() <= 2, "{case}: vault {}", book.vault());
    book.assert_settled(payouts);
}

fn permutations(items: &[usize]) -> Vec<Vec<usize>> {
    if items.len() <= 1 {
        return vec![items.to_vec()];
    }
    let mut out = Vec::new();
    for (index, &first) in items.iter().enumerate() {
        let mut rest = items.to_vec();
        rest.remove(index);
        for mut tail in permutations(&rest) {
            tail.insert(0, first);
            out.push(tail);
        }
    }
    out
}

/// Control: FIRST's pending close is finished before SECOND is touched, so
/// SECOND's one close carries both losses. This order always settled.
#[test]
fn control_two_bankruptcies_settle_when_the_pending_close_goes_first() {
    for first_side in [LONG, SHORT] {
        let case = format!("first {first_side:?}");
        let mut book = two_bankruptcies(first_side, None);
        book.resolve();
        let payouts = book
            .drain(&[FIRST, SECOND, FIRST_MAKER, SECOND_MAKER])
            .unwrap_or_else(|e| panic!("{case}: {e}"));
        let [first_maker, second_maker] = booked_maker_payouts(first_side);
        assert_eq!(
            payouts,
            [0, first_maker - 1, second_maker - 1, 0],
            "{case}"
        );
        let ledger = book.ledger(SECOND);
        assert_eq!(
            (ledger.close_id, ledger.b_loss_booked),
            (
                1,
                second_unpaid_loss(first_side) + unpaid_loss(first_side) / 2
            ),
            "{case}"
        );
        assert_two_bankruptcies_settled(&mut book, &payouts, &case);
    }
}

/// SECOND is closed first: its own unpaid loss is booked and its close is
/// finalized, but its leg has to stay for FIRST's barrier. FIRST's close then
/// books half of FIRST's loss onto that leg, which SECOND cannot pay. The next
/// close of SECOND must book that loss as a new close.
///
/// Before the fix a finalized close was never reopened: SECOND stayed negative
/// with its leg attached for good, and neither maker (1e9 of capital each)
/// could ever be paid.
#[test]
fn resolved_close_reopens_a_finalized_close_for_a_later_loss() {
    for first_side in [LONG, SHORT] {
        let case = format!("first {first_side:?}");
        let mut book = two_bankruptcies(first_side, None);
        let half = unpaid_loss(first_side) / 2;
        book.resolve();

        assert_eq!(
            book.close(SECOND),
            Ok(ResolvedCloseOutcomeV16::ProgressOnly),
            "{case}"
        );
        let ledger = book.ledger(SECOND);
        assert!(ledger.active && ledger.finalized, "{case}: {ledger:?}");
        assert_eq!(
            (ledger.close_id, ledger.b_loss_booked, book.pnl(SECOND)),
            (1, second_unpaid_loss(first_side), 0),
            "{case}"
        );
        assert!(book.leg(SECOND).active, "{case}");

        assert_eq!(
            book.close(FIRST),
            Ok(ResolvedCloseOutcomeV16::Closed { payout: 0 }),
            "{case}"
        );
        assert_eq!(book.barriers(), (0, 0), "{case}");

        let (vault, c_tot, insurance) = (
            book.vault(),
            book.header.c_tot.get(),
            book.header.insurance.get(),
        );
        assert_eq!(
            book.close(SECOND),
            Ok(ResolvedCloseOutcomeV16::Closed { payout: 0 }),
            "{case}"
        );
        let ledger = book.ledger(SECOND);
        assert!(ledger.active && ledger.finalized, "{case}: {ledger:?}");
        assert_eq!(
            (
                ledger.close_id,
                ledger.domain_side,
                ledger.gross_loss_at_close_start,
                ledger.b_loss_booked,
                ledger.residual_remaining
            ),
            (2, first_side, half, half, 0),
            "{case}"
        );
        assert_eq!(book.pnl(SECOND), 0, "{case}");
        assert!(!book.leg(SECOND).active, "{case}");
        assert_eq!(book.barriers(), (0, 0), "{case}");
        // Booking moves no value: the loss is taken from SECOND_MAKER's gain
        // when that account settles.
        assert_eq!(
            (
                book.vault(),
                book.header.c_tot.get(),
                book.header.insurance.get()
            ),
            (vault, c_tot, insurance),
            "{case}: (vault, c_tot, insurance)"
        );
        book.assert_valid();

        let payouts = book
            .drain(&[SECOND, FIRST, FIRST_MAKER, SECOND_MAKER])
            .unwrap_or_else(|e| panic!("{case}: {e}"));
        let [first_maker, second_maker] = booked_maker_payouts(first_side);
        assert_eq!(
            payouts,
            [0, first_maker - 1, second_maker - 1, 0],
            "{case}"
        );
        assert_two_bankruptcies_settled(&mut book, &payouts, &case);
    }
}

/// Whatever order the four accounts are closed in, with the residual booked
/// in one step or in chunks, everything settles: both bankrupts leave with
/// nothing, both makers get at least their own capital back, and all but
/// rounding dust leaves the vault.
///
/// Before the fix 12 of the 24 orders ended in the lock above with unchunked
/// booking and all 24 with chunked booking, for either side: with chunks
/// SECOND's close finalizes while FIRST's is still booking onto its leg.
#[test]
fn two_bankruptcies_settle_in_every_close_order() {
    for first_side in [LONG, SHORT] {
        for b_chunk_atoms in [None, Some(1_000_000)] {
            for order in permutations(&[FIRST, FIRST_MAKER, SECOND_MAKER, SECOND]) {
                let case = format!("first {first_side:?}, chunk {b_chunk_atoms:?}, order {order:?}");
                let mut book = two_bankruptcies(first_side, b_chunk_atoms);
                book.resolve();
                let payouts = book.drain(&order).unwrap_or_else(|e| panic!("{case}: {e}"));
                assert_two_bankruptcies_settled(&mut book, &payouts, &case);
            }
        }
    }
}

/// The same later loss with insurance funded for SECOND's bankruptcies: the
/// first close is paid by insurance and finalized, and the later loss has to
/// draw insurance through a new close as well.
///
/// Before the fix the draw was applied to the finalized close and failed with
/// `LockActive` on every call.
#[test]
fn a_later_loss_draws_insurance_through_a_new_close() {
    const INSURANCE: u128 = 10_000_000;
    for first_side in [LONG, SHORT] {
        let case = format!("first {first_side:?}");
        let mut book = two_bankruptcies(first_side, None);
        let half = unpaid_loss(first_side) / 2;
        let second_unpaid = second_unpaid_loss(first_side);
        assert!(second_unpaid + half < INSURANCE);
        // SECOND's unpaid loss is charged to `first_side`; that domain's
        // insurance pays first.
        let domain = match first_side {
            SideV16::Long => 0,
            SideV16::Short => 1,
        };
        book.market(|m| m.deposit_domain_insurance_not_atomic(domain, INSURANCE))
            .unwrap();
        book.deposited += INSURANCE;
        book.resolve();

        assert_eq!(
            book.close(SECOND),
            Ok(ResolvedCloseOutcomeV16::ProgressOnly),
            "{case}"
        );
        let ledger = book.ledger(SECOND);
        assert!(ledger.active && ledger.finalized, "{case}: {ledger:?}");
        assert_eq!(
            (ledger.insurance_spent, ledger.b_loss_booked),
            (second_unpaid, 0),
            "{case}"
        );
        assert_eq!(
            book.close(FIRST),
            Ok(ResolvedCloseOutcomeV16::Closed { payout: 0 }),
            "{case}"
        );
        assert_eq!(
            book.close(SECOND),
            Ok(ResolvedCloseOutcomeV16::Closed { payout: 0 }),
            "{case}"
        );
        let ledger = book.ledger(SECOND);
        assert_eq!(
            (
                ledger.close_id,
                ledger.insurance_spent,
                ledger.b_loss_booked
            ),
            (2, half, 0),
            "{case}"
        );
        assert_eq!(
            book.header.insurance.get(),
            INSURANCE - second_unpaid - half,
            "{case}"
        );

        let payouts = book
            .drain(&[SECOND, FIRST, FIRST_MAKER, SECOND_MAKER])
            .unwrap_or_else(|e| panic!("{case}: {e}"));
        assert_eq!((payouts[FIRST], payouts[SECOND]), (0, 0), "{case}");
        // Insurance paid everything SECOND could not: SECOND_MAKER keeps its
        // whole gain, FIRST_MAKER is cut only by its own half of FIRST's loss.
        let wholes = [
            (FIRST_MAKER, MAKER_CAPITAL + TRADER_CAPITAL + half),
            (
                SECOND_MAKER,
                MAKER_CAPITAL + TRADER_CAPITAL + second_unpaid,
            ),
        ];
        for (maker, whole) in wholes {
            assert!(
                payouts[maker] == whole || payouts[maker] + 1 == whole,
                "{case}: maker {maker} payout {} of {whole}",
                payouts[maker]
            );
        }
        book.assert_settled(&payouts);
    }
}
