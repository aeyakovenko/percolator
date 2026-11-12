//! Issue #132: after quantity ADL an opposite-side portfolio must still be
//! able to leave, both by Recovery forfeit and by resolved close.
//!
//! A unilateral close (a liquidation or an owner rebalance) removes effective
//! OI from the opposite side by lowering that side's A index instead of
//! rewriting every leg. A leg's effective quantity is
//! `ceil(raw * A_side / a_basis)` while `A_side` itself is floored, so the
//! legs of one side can add up to more than the side's effective OI. The last
//! legs to leave then own less than their rounded-up quantity, possibly
//! nothing.
//!
//! The reported path (resolved close subtracting a leg's raw basis from the
//! reduced side OI) no longer reproduces: resolved close consumes
//! `min(leg effective, side effective OI)`, and a side whose effective OI is
//! exhausted resets so that its residue detaches. The `control_*` tests pin
//! that for liquidations and rebalances on either side.
//!
//! What still failed: a side cannot reset while it carries a pending
//! obligation (a Recovery forfeit that kept its loss weight). On such a side
//! - a Recovery forfeit charged the full rounded-up quantity and failed with
//!   `CounterUnderflow` as soon as the side held less;
//! - resolved close could not detach a current-epoch leg once the side's
//!   effective OI was zero (`InvalidLeg`). With that state on both sides the
//!   obligations waited for those legs forever, locking four solvent accounts.
//!
//! Every call goes through the public `*_not_atomic` API. A failed call is
//! rolled back here, as the wrapper's transaction would roll it back.
use percolator::{
    AssetLifecycleV16, AssetStateV16, AutoCrankOutcomeV16, AutoCrankWorkV16,
    EngineAssetSlotV16Account, LiquidationOutcomeV16, LiquidationRequestV16, Market,
    MarketGroupV16HeaderAccount, MarketGroupV16ViewMut, PortfolioAccountV16Account,
    PortfolioLegV16, PortfolioV16ViewMut, ProvenanceHeaderV16, ProvenanceHeaderV16Account,
    RebalanceRequestV16, ResolvedCloseOutcomeV16, SideModeV16, SideV16, TradeRequestV16, V16Config,
    V16Error, ADL_ONE, MIN_A_SIDE, POS_SCALE,
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
    /// One asset at `PRICE` (one quote atom per position atom) and `accounts`
    /// empty portfolios. `margin_bps` is both initial and maintenance margin.
    fn new(accounts: usize, margin_bps: u64) -> Self {
        let mut cfg = V16Config::public_user_fund_with_market_slots(1, 1, 0, 10);
        cfg.initial_margin_bps = margin_bps;
        cfg.maintenance_margin_bps = margin_bps;
        cfg.max_price_move_bps_per_slot = 500;
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

    fn with_pair<R>(
        &mut self,
        first: usize,
        second: usize,
        f: impl FnOnce(
            &mut MarketGroupV16ViewMut<'_, u64>,
            &mut PortfolioV16ViewMut<'_>,
            &mut PortfolioV16ViewMut<'_>,
        ) -> Result<R, V16Error>,
    ) -> Result<R, V16Error> {
        assert_ne!(first, second);
        self.tx(|book| {
            let mut market = MarketGroupV16ViewMut::new(&mut book.header, &mut book.markets);
            let (low, high) = (first.min(second), first.max(second));
            let (left, right) = book.accounts.split_at_mut(high);
            let (low_account, high_account) = (&mut left[low], &mut right[0]);
            let (a, b) = if first < second {
                (low_account, high_account)
            } else {
                (high_account, low_account)
            };
            f(
                &mut market,
                &mut PortfolioV16ViewMut::new(a),
                &mut PortfolioV16ViewMut::new(b),
            )
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
        let exec_price = self.price;
        self.with_pair(long, short, |m, a, b| {
            m.execute_trade_with_fee_loss_stale_scoped_not_atomic(
                a,
                b,
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

    /// Moves the oracle target and the mark by `bps` (at most the 5% cap) in
    /// one slot.
    fn mark(&mut self, up: bool, bps: u64) {
        let delta = self.price * bps / 10_000;
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

    fn liquidate(&mut self, account: usize) -> Result<LiquidationOutcomeV16, V16Error> {
        self.with(account, |m, a| {
            m.liquidate_account_not_atomic(a, LiquidationRequestV16 { asset_index: 0 })
        })
    }

    fn rebalance(&mut self, account: usize, reduce_q: u128) -> Result<u128, V16Error> {
        self.with(account, |m, a| {
            m.rebalance_reduce_position_not_atomic(
                a,
                RebalanceRequestV16 {
                    asset_index: 0,
                    reduce_q,
                },
            )
            .map(|outcome| outcome.reduced_q)
        })
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

    fn refresh(&mut self, account: usize) -> Result<(), V16Error> {
        self.with(account, |m, a| {
            m.full_account_refresh_not_atomic(a).map(|_| ())
        })
    }

    fn force_recovery(&mut self) {
        let slot = self.slot;
        self.market(|m| m.force_asset_recovery_not_atomic(0, slot))
            .unwrap();
        assert_eq!(self.asset().lifecycle, AssetLifecycleV16::Recovery);
    }

    /// Returns whether the leg was detached (`false`: kept as a zero-basis
    /// pending obligation).
    fn forfeit(&mut self, account: usize) -> Result<bool, V16Error> {
        self.with(account, |m, a| {
            m.forfeit_recovery_leg_not_atomic(a, 0, u128::MAX)
                .map(|outcome| outcome.detached)
        })
    }

    fn recovery_pair_close(&mut self, a: usize, b: usize, q: u128) -> Result<u128, V16Error> {
        self.with_pair(a, b, |m, a, b| {
            m.force_close_recovery_pair_not_atomic(a, b, 0, q)
        })
    }

    fn withdraw(&mut self, account: usize, amount: u128) -> Result<(), V16Error> {
        self.with(account, |m, a| m.withdraw_not_atomic(a, amount))
    }

    fn resolve(&mut self) {
        let slot = self.slot;
        self.market(|m| m.resolve_market_not_atomic(slot)).unwrap();
    }

    fn close(&mut self, account: usize) -> Result<ResolvedCloseOutcomeV16, V16Error> {
        self.with(account, |m, a| m.close_resolved_account_not_atomic(a, 0))
    }

    /// Calls resolved close on `account` alone until it pays out. Fails on
    /// the first error, or if `max_calls` calls only report progress.
    fn close_alone(&mut self, account: usize, max_calls: usize) -> Result<u128, String> {
        for call in 0..max_calls {
            match self.close(account) {
                Ok(ResolvedCloseOutcomeV16::Closed { payout }) => return Ok(payout),
                Ok(ResolvedCloseOutcomeV16::ProgressOnly) => {}
                Err(error) => return Err(format!("call {call}: {error:?}")),
            }
        }
        Err(format!("still open after {max_calls} calls"))
    }

    /// Round-robin resolved close in `order` until every account has paid
    /// out. A round that changes nothing is a dead state and fails with the
    /// last result seen per open account.
    fn drain(&mut self, order: &[usize]) -> Result<Vec<u128>, String> {
        let mut payouts: Vec<Option<u128>> = vec![None; self.accounts.len()];
        for _round in 0..64 {
            let before = (self.header, self.markets.clone(), self.accounts.clone());
            let mut last = Vec::new();
            for &account in order {
                if payouts[account].is_some() {
                    continue;
                }
                let result = self.close(account);
                if let Ok(ResolvedCloseOutcomeV16::Closed { payout }) = result {
                    payouts[account] = Some(payout);
                }
                last.push((account, result));
            }
            if order.iter().all(|&account| payouts[account].is_some()) {
                return Ok(payouts.into_iter().map(|p| p.unwrap_or(0)).collect());
            }
            if before == (self.header, self.markets.clone(), self.accounts.clone()) {
                return Err(format!("no progress; last results {last:?}"));
            }
        }
        Err("still open after 64 rounds".to_string())
    }

    fn asset(&self) -> AssetStateV16 {
        self.markets[0].engine.asset.try_to_runtime().unwrap()
    }

    fn leg(&self, account: usize) -> PortfolioLegV16 {
        self.accounts[account].legs[0].try_to_runtime().unwrap()
    }

    fn has_leg(&self, account: usize) -> bool {
        self.leg(account).active
    }

    fn raw(&self, account: usize) -> u128 {
        self.leg(account).basis_pos_q.unsigned_abs()
    }

    /// The leg's ADL-effective quantity, `ceil(raw * A_side / a_basis)`.
    fn effective(&self, account: usize) -> u128 {
        let leg = self.leg(account);
        if !leg.active {
            return 0;
        }
        let asset = self.asset();
        let a_side = match leg.side {
            SideV16::Long => asset.a_long,
            SideV16::Short => asset.a_short,
        };
        (leg.basis_pos_q.unsigned_abs() * a_side).div_ceil(leg.a_basis)
    }

    fn side_oi(&self, side: SideV16) -> u128 {
        let asset = self.asset();
        match side {
            SideV16::Long => asset.oi_eff_long_q,
            SideV16::Short => asset.oi_eff_short_q,
        }
    }

    fn side_a(&self, side: SideV16) -> u128 {
        let asset = self.asset();
        match side {
            SideV16::Long => asset.a_long,
            SideV16::Short => asset.a_short,
        }
    }

    /// (stored legs, pending obligations) on `side`.
    fn side_counts(&self, side: SideV16) -> (u64, u64) {
        let asset = self.asset();
        match side {
            SideV16::Long => (
                asset.stored_pos_count_long,
                asset.pending_obligation_count_long,
            ),
            SideV16::Short => (
                asset.stored_pos_count_short,
                asset.pending_obligation_count_short,
            ),
        }
    }

    fn capital(&self, account: usize) -> u128 {
        self.accounts[account].capital.get()
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

    /// No leg, effective OI or loss weight is left on the asset.
    #[track_caller]
    fn assert_asset_empty(&self) {
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
    }
}

fn permutations(items: &[usize]) -> Vec<Vec<usize>> {
    if items.len() <= 1 {
        return vec![items.to_vec()];
    }
    let mut out = Vec::new();
    for i in 0..items.len() {
        let mut rest = items.to_vec();
        let head = rest.remove(i);
        for mut tail in permutations(&rest) {
            tail.insert(0, head);
            out.push(tail);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Controls: the reported path. These pass before and after the #132 fix.
// ---------------------------------------------------------------------------

const LOT: u128 = POS_SCALE;
const OPEN_Q: u128 = 100 * LOT;
const TRADER_CAPITAL: u128 = 10_000_000; // exactly 10% of the 100-lot notional
const MAKER_CAPITAL: u128 = 1_000_000_000;

/// Account 0 opens `OPEN_Q` on `trader_side` at 10% margin against the well
/// funded account 1, then the mark moves `adverse_steps` x 5% against it.
fn leveraged_pair(trader_side: SideV16, adverse_steps: usize) -> Book {
    let mut book = Book::new(2, 1_000);
    book.deposit(0, TRADER_CAPITAL);
    book.deposit(1, MAKER_CAPITAL);
    match trader_side {
        SideV16::Long => book.trade(0, 1, OPEN_Q).unwrap(),
        SideV16::Short => book.trade(1, 0, OPEN_Q).unwrap(),
    }
    for _ in 0..adverse_steps {
        book.mark(trader_side == SHORT, 500);
    }
    book
}

fn opposite(side: SideV16) -> SideV16 {
    match side {
        SideV16::Long => SideV16::Short,
        SideV16::Short => SideV16::Long,
    }
}

#[test]
fn control_resolved_close_after_partial_liquidation() {
    for trader_side in [LONG, SHORT] {
        for refresh_maker_before_resolve in [false, true] {
            for order in [[0usize, 1], [1, 0]] {
                let case = format!(
                    "liquidated {trader_side:?}, refresh={refresh_maker_before_resolve}, \
                     close order {order:?}"
                );
                let mut book = leveraged_pair(trader_side, 1);
                let maker_side = opposite(trader_side);
                let liquidation = book.liquidate(0).unwrap();
                // Solvent and partial: nothing is socialized, the maker's side is
                // scaled by A instead of having its leg rewritten.
                assert!(
                    liquidation.closed_q > 0 && liquidation.closed_q < OPEN_Q,
                    "{case}: closed_q={}",
                    liquidation.closed_q
                );
                assert_eq!(
                    (liquidation.residual_booked, liquidation.explicit_loss),
                    (0, 0),
                    "{case}"
                );
                let remaining = OPEN_Q - liquidation.closed_q;
                assert_eq!(book.raw(1), OPEN_Q, "{case}: maker raw basis is untouched");
                assert_eq!(book.side_oi(maker_side), remaining, "{case}");
                assert!(book.side_a(maker_side) < ADL_ONE, "{case}");
                assert!(
                    book.raw(1) > book.side_oi(maker_side),
                    "{case}: the reported precondition, raw basis above side effective OI"
                );
                if refresh_maker_before_resolve {
                    book.refresh(1).unwrap();
                }
                book.resolve();

                let payouts = book
                    .drain(&order)
                    .unwrap_or_else(|stuck| panic!("{case}: {stuck}"));
                assert_eq!(
                    payouts.iter().sum::<u128>() + book.header.vault.get(),
                    book.deposited,
                    "{case}: vault conservation"
                );
                // The trader lost 5% of its notional to the maker, to the atom.
                assert_eq!(
                    payouts,
                    vec![TRADER_CAPITAL - 5_000_000, MAKER_CAPITAL + 5_000_000],
                    "{case}"
                );
                book.assert_asset_empty();
                book.assert_valid();
            }
        }
    }
}

#[test]
fn control_resolved_close_after_bankrupt_full_liquidation() {
    for trader_side in [LONG, SHORT] {
        for order in [[0usize, 1], [1, 0]] {
            let case = format!("liquidated {trader_side:?}, close order {order:?}");
            // Three 5% steps against a 10% margin: the trader is bankrupt.
            let mut book = leveraged_pair(trader_side, 3);
            let maker_side = opposite(trader_side);
            let liquidation = book.liquidate(0).unwrap();
            assert_eq!(liquidation.closed_q, OPEN_Q, "{case}: full close");
            assert!(liquidation.residual_booked > 0, "{case}: {liquidation:?}");
            // The whole maker side was deleveraged: no effective OI is left while
            // the maker still stores its full raw basis behind a side reset.
            assert_eq!(book.side_oi(maker_side), 0, "{case}");
            assert_eq!(book.raw(1), OPEN_Q, "{case}");
            let asset = book.asset();
            let maker_mode = match maker_side {
                SideV16::Long => asset.mode_long,
                SideV16::Short => asset.mode_short,
            };
            assert_eq!(maker_mode, SideModeV16::ResetPending, "{case}");
            book.resolve();

            let payouts = book
                .drain(&order)
                .unwrap_or_else(|stuck| panic!("{case}: {stuck}"));
            assert_eq!(
                payouts[0], 0,
                "{case}: the bankrupt trader has nothing left"
            );
            assert_eq!(
                payouts.iter().sum::<u128>() + book.header.vault.get(),
                book.deposited,
                "{case}: vault conservation"
            );
            // The maker is paid its capital plus exactly what the trader could
            // pay; the uncovered remainder of its gain was socialized.
            assert_eq!(payouts[1], MAKER_CAPITAL + TRADER_CAPITAL, "{case}");
            book.assert_asset_empty();
            book.assert_valid();
        }
    }
}

#[test]
fn control_resolved_close_after_owner_rebalance() {
    // Reducing 4 lots to 3 keeps the side Normal; reducing 100 lots to 1 takes
    // A below MIN_A_SIDE and the opposite side to DrainOnly.
    for (open_lots, reduce_lots, expect_drain_only) in [(4u128, 1u128, false), (100, 99, true)] {
        for reducer_side in [LONG, SHORT] {
            for order in [[0usize, 1], [1, 0]] {
                let case = format!(
                    "{open_lots} lots -{reduce_lots}, reducer {reducer_side:?}, order {order:?}"
                );
                let mut book = Book::new(2, 10_000);
                book.deposit(0, 1_000_000_000);
                book.deposit(1, 1_000_000_000);
                match reducer_side {
                    SideV16::Long => book.trade(0, 1, open_lots * LOT).unwrap(),
                    SideV16::Short => book.trade(1, 0, open_lots * LOT).unwrap(),
                }
                assert_eq!(
                    book.rebalance(0, reduce_lots * LOT),
                    Ok(reduce_lots * LOT),
                    "{case}"
                );
                let holder_side = opposite(reducer_side);
                let left = (open_lots - reduce_lots) * LOT;
                assert_eq!(book.raw(1), open_lots * LOT, "{case}");
                assert_eq!(book.side_oi(holder_side), left, "{case}");
                assert_eq!(book.effective(1), left, "{case}");
                assert_eq!(
                    book.side_a(holder_side) < MIN_A_SIDE,
                    expect_drain_only,
                    "{case}"
                );
                let asset = book.asset();
                let holder_mode = match holder_side {
                    SideV16::Long => asset.mode_long,
                    SideV16::Short => asset.mode_short,
                };
                assert_eq!(
                    holder_mode == SideModeV16::DrainOnly,
                    expect_drain_only,
                    "{case}"
                );
                book.resolve();

                let payouts = book
                    .drain(&order)
                    .unwrap_or_else(|stuck| panic!("{case}: {stuck}"));
                assert_eq!(payouts, vec![1_000_000_000, 1_000_000_000], "{case}");
                assert_eq!(book.header.vault.get(), 0, "{case}");
                book.assert_asset_empty();
                book.assert_valid();
            }
        }
    }
}

/// A long holds 20 lots against a solvent short and 1 lot against a small
/// short that goes bankrupt and is liquidated, which scales the long side to
/// 20/21 while the long keeps its 21-lot raw basis. The solvent pair then
/// trades the remaining 20 effective lots back.
fn winner_after_social_loss_and_bilateral_close() -> Book {
    const WINNER: usize = 0;
    const SOLVENT_SHORT: usize = 1;
    const SMALL_SHORT: usize = 2;
    let mut book = Book::new(3, 1_000);
    book.deposit(WINNER, 1_000_000_000);
    book.deposit(SOLVENT_SHORT, 1_000_000_000);
    book.deposit(SMALL_SHORT, LOT / 10); // exactly 10% of one lot
    book.trade(WINNER, SOLVENT_SHORT, 20 * LOT).unwrap();
    book.trade(WINNER, SMALL_SHORT, LOT).unwrap();
    for _ in 0..3 {
        book.mark(true, 500);
    }
    let liquidation = book.liquidate(SMALL_SHORT).unwrap();
    assert_eq!(liquidation.closed_q, LOT);
    assert!(liquidation.residual_booked > 0, "{liquidation:?}");
    assert_eq!(
        (book.side_oi(LONG), book.side_oi(SHORT)),
        (20 * LOT, 20 * LOT)
    );
    assert_eq!(
        book.raw(WINNER),
        21 * LOT,
        "the winner's raw basis is untouched"
    );
    assert_eq!(book.effective(WINNER), 20 * LOT);

    // The winner settles its share of the socialized loss, then closes all of
    // its effective quantity against the solvent short.
    assert!(matches!(
        book.crank(WINNER),
        Ok(AutoCrankOutcomeV16::Progressed(_))
    ));
    book.trade(SOLVENT_SHORT, WINNER, 20 * LOT).unwrap();
    book
}

#[test]
fn control_winner_leaves_no_raw_residue_after_social_loss_and_bilateral_close() {
    // The bilateral close of the whole effective quantity detaches the whole
    // leg: no raw basis survives the matched effective OI.
    let mut book = winner_after_social_loss_and_bilateral_close();
    assert_eq!(
        [0usize, 1, 2].map(|account| book.has_leg(account)),
        [false; 3]
    );
    book.assert_asset_empty();
    book.assert_valid();

    for order in permutations(&[0, 1, 2]) {
        let mut book = winner_after_social_loss_and_bilateral_close();
        book.resolve();
        let payouts = book
            .drain(&order)
            .unwrap_or_else(|stuck| panic!("close order {order:?}: {stuck}"));
        // Three 5% steps move each lot by 157_625. The solvent short pays
        // 20 lots of it. The small short owes 157_625 and has 100_000, so
        // 57_625 of the winner's 21-lot gain is socialized back onto it, and
        // one atom of rounding stays in the vault.
        assert_eq!(
            payouts,
            vec![
                1_000_000_000 + 21 * 157_625 - 57_625 - 1,
                1_000_000_000 - 20 * 157_625,
                0
            ],
            "close order {order:?}"
        );
        assert_eq!(book.header.vault.get(), 1, "close order {order:?}");
        assert_eq!(
            payouts.iter().sum::<u128>() + book.header.vault.get(),
            book.deposited,
            "close order {order:?}: vault conservation"
        );
        book.assert_asset_empty();
        book.assert_valid();
    }
}

// ---------------------------------------------------------------------------
// Rounded-up legs on both sides.
// ---------------------------------------------------------------------------

const L1: usize = 0;
const L2: usize = 1;
const S1: usize = 2;
const S2: usize = 3;
const CAPITAL: u128 = 1_000_000;

/// Longs L1 and L2 hold 3 atoms each against shorts S1 and S2. L1 and then S1
/// reduce by one atom on their own, so each side's A index is floored below
/// the ceiling of its legs:
///
/// | side  | effective OI | leg effective quantities |
/// |-------|--------------|--------------------------|
/// | long  | 4            | L1 = 2, L2 = 3           |
/// | short | 4            | S1 = 2, S2 = 3           |
fn rounded_up_legs_on_both_sides() -> Book {
    let mut book = Book::new(4, 10_000);
    for account in [L1, L2, S1, S2] {
        book.deposit(account, CAPITAL);
    }
    book.trade(L1, S1, 3).unwrap();
    book.trade(L2, S2, 3).unwrap();
    assert_eq!(book.rebalance(L1, 1), Ok(1));
    assert_eq!(book.rebalance(S1, 1), Ok(1));
    assert_eq!(
        (book.side_oi(LONG), book.side_oi(SHORT)),
        (4, 4),
        "side effective OI"
    );
    assert_eq!(
        [L1, L2, S1, S2].map(|account| book.effective(account)),
        [2, 3, 2, 3],
        "leg effective quantities"
    );
    assert_eq!(
        (book.side_a(LONG), book.side_a(SHORT)),
        (ADL_ONE * 4 / 5, ADL_ONE * 5 / 6),
        "A indices"
    );
    book
}

#[track_caller]
fn assert_everyone_closes_whole(book: &mut Book, order: &[usize], case: &str) {
    let payouts = book
        .drain(order)
        .unwrap_or_else(|stuck| panic!("{case}: close order {order:?}: {stuck}"));
    assert_eq!(payouts, vec![CAPITAL; 4], "{case}: close order {order:?}");
    assert_eq!(book.header.vault.get(), 0, "{case}: close order {order:?}");
    book.assert_asset_empty();
    let asset = book.asset();
    assert_eq!(
        (asset.mode_long, asset.mode_short),
        (SideModeV16::ResetPending, SideModeV16::ResetPending),
        "{case}: close order {order:?}"
    );
    book.assert_valid();
}

#[test]
fn control_resolved_close_with_rounded_up_legs_in_every_order() {
    for order in permutations(&[L1, L2, S1, S2]) {
        let mut book = rounded_up_legs_on_both_sides();
        book.resolve();
        assert_everyone_closes_whole(&mut book, &order, "no pending obligation");
    }
}

/// Fails before the fix: the second forfeit on each side returned
/// `CounterUnderflow`, because the first one had already taken its rounded-up
/// share and the side held less than the next leg's ceiling.
#[test]
fn issue132_recovery_forfeit_removes_only_the_remaining_side_effective_oi() {
    let mut book = rounded_up_legs_on_both_sides();
    book.force_recovery();

    // Shorts still hold real positions, so both longs keep their loss weight
    // as zero-basis obligations.
    assert_eq!(book.forfeit(L2), Ok(false), "forfeit L2");
    assert_eq!(
        book.side_oi(LONG),
        1,
        "L2 removed its 3 of the long side's 4"
    );
    assert_eq!(
        book.forfeit(L1),
        Ok(false),
        "forfeit L1: effective 2 against a side that holds 1"
    );
    assert_eq!(book.side_oi(LONG), 0);
    assert_eq!(book.side_counts(LONG), (2, 2), "long (stored, pending)");
    assert_eq!((book.raw(L1), book.raw(L2)), (0, 0));

    // Every long is now an obligation, so the shorts detach outright.
    assert_eq!(book.forfeit(S2), Ok(true), "forfeit S2");
    assert_eq!(
        book.side_oi(SHORT),
        1,
        "S2 removed its 3 of the short side's 4"
    );
    assert_eq!(
        book.forfeit(S1),
        Ok(true),
        "forfeit S1: effective 2 against a side that holds 1"
    );
    assert_eq!(book.side_oi(SHORT), 0);
    assert_eq!(book.side_counts(SHORT), (0, 0), "short (stored, pending)");

    // No real short is left, so the long obligations are released.
    assert_eq!(book.forfeit(L1), Ok(true), "release L1");
    assert_eq!(book.forfeit(L2), Ok(true), "release L2");
    book.assert_asset_empty();
    book.assert_valid();

    // Flat again: everyone can take their capital out of the Live market.
    for account in [L1, L2, S1, S2] {
        assert!(!book.has_leg(account));
        assert_eq!(book.capital(account), CAPITAL);
        assert_eq!(
            book.withdraw(account, CAPITAL),
            Ok(()),
            "withdraw {account}"
        );
    }
    assert_eq!(book.header.vault.get(), 0);
    book.assert_valid();
}

/// Fails before the fix: after the matched Recovery close consumed the last
/// effective OI, both survivors kept one raw atom on a side that could not
/// reset, and every later forfeit returned `CounterUnderflow`.
#[test]
fn issue132_recovery_forfeit_detaches_a_leg_whose_side_has_no_effective_oi_left() {
    let mut book = rounded_up_legs_on_both_sides();
    book.force_recovery();
    assert_eq!(book.forfeit(L2), Ok(false), "forfeit L2");
    assert_eq!(book.forfeit(S2), Ok(false), "forfeit S2");
    assert_eq!((book.side_oi(LONG), book.side_oi(SHORT)), (1, 1));

    // The public pair close lands only the matched effective atom.
    assert_eq!(book.recovery_pair_close(L1, S1, 10), Ok(1));
    assert_eq!((book.side_oi(LONG), book.side_oi(SHORT)), (0, 0));
    assert_eq!(
        (
            book.raw(L1),
            book.effective(L1),
            book.raw(S1),
            book.effective(S1)
        ),
        (1, 1, 1, 1),
        "(L1 raw, L1 effective, S1 raw, S1 effective)"
    );
    let asset = book.asset();
    assert_eq!(
        (asset.mode_long, asset.mode_short),
        (SideModeV16::Normal, SideModeV16::Normal),
        "neither side can reset while it carries an obligation"
    );

    // S1 is still a real position, so L1 keeps its weight as an obligation
    // that removes no effective OI; then nothing real is left opposite S1.
    assert_eq!(book.forfeit(L1), Ok(false), "forfeit L1");
    assert_eq!(book.side_counts(LONG), (2, 2), "long (stored, pending)");
    assert_eq!(book.forfeit(S1), Ok(true), "forfeit S1");
    assert_eq!(book.forfeit(S2), Ok(true), "release S2");
    assert_eq!(book.forfeit(L1), Ok(true), "release L1");
    assert_eq!(book.forfeit(L2), Ok(true), "release L2");
    book.assert_asset_empty();
    for account in [L1, L2, S1, S2] {
        assert_eq!(
            book.withdraw(account, CAPITAL),
            Ok(()),
            "withdraw {account}"
        );
    }
    assert_eq!(book.header.vault.get(), 0);
    book.assert_valid();
}

/// One forfeit per side, then the market resolves.
fn resolved_with_an_obligation_on_each_side() -> Book {
    let mut book = rounded_up_legs_on_both_sides();
    book.force_recovery();
    assert_eq!(book.forfeit(L2), Ok(false), "forfeit L2");
    assert_eq!(book.forfeit(S2), Ok(false), "forfeit S2");
    assert_eq!((book.side_oi(LONG), book.side_oi(SHORT)), (1, 1));
    assert_eq!((book.effective(L1), book.effective(S1)), (2, 2));
    assert_eq!(book.side_counts(LONG), (2, 1), "long (stored, pending)");
    assert_eq!(book.side_counts(SHORT), (2, 1), "short (stored, pending)");
    book.resolve();
    book
}

/// Fails before the fix: L1 and S1 each consumed their side's last effective
/// atom and then returned `InvalidLeg` on every call, while the two
/// obligations waited for them. All four accounts stayed locked.
#[test]
fn issue132_resolved_close_detaches_exhausted_legs_behind_pending_obligations() {
    for order in permutations(&[L1, L2, S1, S2]) {
        let mut book = resolved_with_an_obligation_on_each_side();
        assert_everyone_closes_whole(&mut book, &order, "obligation on each side");
    }
}

/// Fails before the fix: with the obligation only on its own side L1 did get
/// out eventually, but only after other accounts closed first; its own calls
/// returned `InvalidLeg`. A solvent settled leg must not depend on that.
#[test]
fn issue132_resolved_close_of_an_exhausted_leg_does_not_wait_for_other_accounts() {
    let mut book = rounded_up_legs_on_both_sides();
    book.force_recovery();
    assert_eq!(book.forfeit(L2), Ok(false), "forfeit L2");
    assert_eq!(book.side_oi(LONG), 1);
    book.resolve();

    // First call takes the last effective atom, second detaches the residue.
    assert_eq!(book.close_alone(L1, 3), Ok(CAPITAL), "L1 on its own");
    assert_eq!(
        book.side_counts(LONG),
        (1, 1),
        "only L2's obligation is left"
    );

    // L2's obligation is released once no real short is left.
    let payouts = book.drain(&[L2, S1, S2]).unwrap();
    assert_eq!(
        [L2, S1, S2].map(|account| payouts[account]),
        [CAPITAL; 3],
        "(L2, S1, S2) payouts"
    );
    assert_eq!(book.header.vault.get(), 0);
    book.assert_asset_empty();
    book.assert_valid();
}

// ---------------------------------------------------------------------------
// Deterministic sweep over the same family.
// ---------------------------------------------------------------------------

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // splitmix64
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound
    }
}

/// Random small books: opens of 1..=6 atoms between 4..=6 fully funded
/// accounts, unilateral reduces (the ADL source) and small mark moves; then,
/// in three of four cases, asset Recovery with forfeits and matched closes;
/// then resolution and a shuffled round-robin close. Everyone must get out,
/// and the vault must account for every atom.
fn sweep_case(seed: u64) -> Result<(), String> {
    let mut rng = Rng(seed);
    let accounts = 4 + rng.below(3) as usize;
    let mut book = Book::new(accounts, 10_000);
    for account in 0..accounts {
        book.deposit(account, CAPITAL);
    }
    let pick_pair = |rng: &mut Rng| {
        let who = rng.below(accounts as u64) as usize;
        let other = (who + 1 + rng.below(accounts as u64 - 1) as usize) % accounts;
        (who, other)
    };
    // Failed calls are rolled back; the sweep only needs the mix.
    for _ in 0..12 + rng.below(20) {
        let (who, other) = pick_pair(&mut rng);
        match rng.below(6) {
            0 | 1 => {
                let _ = book.trade(who, other, 1 + rng.below(6) as u128);
            }
            2 | 3 => {
                let effective = book.effective(who);
                if effective != 0 {
                    let _ = book.rebalance(who, 1 + rng.next() as u128 % effective);
                }
            }
            4 => book.mark(rng.below(2) == 0, rng.below(200)),
            _ => {
                let _ = book.refresh(who);
            }
        }
    }
    if rng.below(4) != 0 {
        book.force_recovery();
        for _ in 0..rng.below(2 * accounts as u64 + 1) {
            let (who, other) = pick_pair(&mut rng);
            if rng.below(3) == 0 {
                let _ = book.recovery_pair_close(who, other, 1 + rng.below(6) as u128);
            } else if book.forfeit(who) == Err(V16Error::CounterUnderflow) {
                // A forfeit may be refused (nothing to forfeit, an obligation
                // not yet released), but never for lack of side effective OI.
                return Err(format!(
                    "forfeit of account {who} underflowed: effective {} against side OI {}",
                    book.effective(who),
                    book.side_oi(book.leg(who).side)
                ));
            }
        }
    }
    book.resolve();
    let mut order: Vec<usize> = (0..accounts).collect();
    for i in (1..accounts).rev() {
        order.swap(i, rng.below(i as u64 + 1) as usize);
    }
    let payouts = book.drain(&order)?;
    let paid: u128 = payouts.iter().sum();
    if paid + book.header.vault.get() != book.deposited {
        return Err(format!(
            "vault conservation: paid {paid} + vault {} != deposited {}",
            book.header.vault.get(),
            book.deposited
        ));
    }
    let asset = book.asset();
    if asset.stored_pos_count_long != 0
        || asset.stored_pos_count_short != 0
        || asset.oi_eff_long_q != 0
        || asset.oi_eff_short_q != 0
    {
        return Err(format!("legs or effective OI left behind: {asset:?}"));
    }
    Ok(())
}

/// Fails before the fix on every seed whose forfeit meets a side holding less
/// than the leg's rounded-up quantity, and on the seeds that resolve with an
/// exhausted side behind a pending obligation (the first is reported).
#[test]
fn issue132_sweep_every_account_leaves_a_resolved_market_after_adl() {
    let failures: Vec<(u64, String)> = (0..600u64)
        .filter_map(|seed| sweep_case(seed).err().map(|error| (seed, error)))
        .collect();
    assert!(
        failures.is_empty(),
        "{} of 600 seeds stuck; first: seed {} -> {}",
        failures.len(),
        failures[0].0,
        failures[0].1
    );
}
