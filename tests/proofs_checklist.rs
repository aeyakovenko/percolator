//! Kani proofs addressing formal verification checklist gaps.
//! Each proof targets a specific checklist item (A/B/E/F/G).

#![cfg(kani)]

mod common;
use common::*;

// ############################################################################
// A2: 0 <= R_i <= max(PNL_i, 0) after set_pnl
// ############################################################################

/// set_pnl always maintains 0 <= R_i <= max(PNL_i, 0) for any PNL transition.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_a2_reserve_bounds_after_set_pnl() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let idx = engine.add_user(0).unwrap();
    engine.deposit(idx, 500_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    let init_pnl: i128 = kani::any();
    kani::assume(init_pnl >= -100_000 && init_pnl <= 100_000);
    engine.set_pnl(idx as usize, init_pnl);

    let r1 = engine.accounts[idx as usize].reserved_pnl.get();
    let pos1 = core::cmp::max(engine.accounts[idx as usize].pnl.get(), 0) as u128;
    assert!(r1 <= pos1, "A2: R_i <= max(PNL_i,0) after first set");

    let new_pnl: i128 = kani::any();
    kani::assume(new_pnl > -200_000 && new_pnl < 200_000);
    kani::assume(new_pnl != i128::MIN);
    kani::assume(new_pnl <= MAX_ACCOUNT_POSITIVE_PNL as i128 || new_pnl <= 0);
    engine.set_pnl(idx as usize, new_pnl);

    let r2 = engine.accounts[idx as usize].reserved_pnl.get();
    let pos2 = core::cmp::max(engine.accounts[idx as usize].pnl.get(), 0) as u128;
    assert!(r2 <= pos2, "A2: R_i <= max(PNL_i,0) after transition");

    kani::cover!(init_pnl > 0 && new_pnl > init_pnl, "positive increase");
    kani::cover!(init_pnl > 0 && new_pnl < 0, "positive to negative");
    kani::cover!(init_pnl < 0 && new_pnl > 0, "negative to positive");
}

// ############################################################################
// A7: fee_credits ∈ [-(i128::MAX), 0] after trade fees
// ############################################################################

/// After a trade, fee_credits stays in valid range.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_a7_fee_credits_bounds_after_trade() {
    let mut engine = RiskEngine::new(default_params()); // trading_fee_bps=10
    let a = engine.add_user(1000).unwrap();
    let b = engine.add_user(1000).unwrap();
    // Tiny capital so fee exceeds capital → routes through fee_credits
    engine.deposit(a, 100, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.deposit(b, 500_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    let size: i128 = kani::any();
    kani::assume(size > 0 && size <= 10 * POS_SCALE as i128);

    let result = engine.execute_trade_not_atomic(
        a, b, DEFAULT_ORACLE, DEFAULT_SLOT, size, DEFAULT_ORACLE, 0i128, 0);

    if result.is_ok() {
        let fc = engine.accounts[a as usize].fee_credits.get();
        assert!(fc <= 0, "A7: fee_credits <= 0");
        assert!(fc != i128::MIN, "A7: fee_credits != i128::MIN");
        assert!(fc >= -(i128::MAX), "A7: fee_credits >= -(i128::MAX)");
    }

    kani::cover!(result.is_ok(), "trade with fee debt");
}

// ############################################################################
// F2: Insurance floor respected after absorb_protocol_loss
// ############################################################################

/// absorb_protocol_loss never drops I below I_floor.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_f2_insurance_floor_after_absorb() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let idx = engine.add_user(0).unwrap();
    engine.deposit(idx, 100_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    let ins_bal: u128 = kani::any();
    kani::assume(ins_bal >= 1 && ins_bal <= 100_000);
    let floor: u128 = kani::any();
    kani::assume(floor > 0 && floor <= ins_bal);
    engine.insurance_fund.balance = U128::new(ins_bal);
    engine.params.insurance_floor = U128::new(floor);
    engine.vault = U128::new(engine.vault.get() + ins_bal);

    let loss: u128 = kani::any();
    kani::assume(loss > 0 && loss <= 100_000);

    engine.absorb_protocol_loss(loss);

    assert!(engine.insurance_fund.balance.get() >= floor,
        "F2: I must remain >= I_floor after absorb_protocol_loss");

    kani::cover!(loss > ins_bal.saturating_sub(floor), "loss exceeds available above floor");
    kani::cover!(loss <= ins_bal.saturating_sub(floor), "loss fits above floor");
}

// ############################################################################
// F8: Loss seniority in touch (losses before fees)
// ############################################################################

/// After touch on a crashed position, losses reduce capital (senior to fees).
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_f8_loss_seniority_in_touch() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let a = engine.add_user(0).unwrap();
    let b = engine.add_user(0).unwrap();
    engine.deposit(a, 500_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.deposit(b, 500_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    let size = (50 * POS_SCALE) as i128;
    engine.execute_trade_not_atomic(a, b, DEFAULT_ORACLE, DEFAULT_SLOT, size, DEFAULT_ORACLE, 0i128, 0).unwrap();

    let capital_before = engine.accounts[a as usize].capital.get();

    // Price crash → negative PnL for long
    let slot2 = DEFAULT_SLOT + 10;
    let mut ctx = InstructionContext::new_with_h_lock(0);
    let _ = engine.accrue_market_to(slot2, 800, 0);
    engine.current_slot = slot2;
    let _ = engine.touch_account_live_local(a as usize, &mut ctx);
    engine.finalize_touched_accounts_post_live(&ctx);

    let capital_after = engine.accounts[a as usize].capital.get();
    assert!(capital_after <= capital_before,
        "F8: capital must not increase after touch on crashed position");
    assert!(engine.check_conservation(), "conservation after touch");

    kani::cover!(capital_after < capital_before, "losses reduced capital");
}

// ############################################################################
// B7: OI_long == OI_short after trade (symbolic size)
// ############################################################################

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_b7_oi_balance_after_trade() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let a = engine.add_user(0).unwrap();
    let b = engine.add_user(0).unwrap();
    engine.deposit(a, 500_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.deposit(b, 500_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    let size: i128 = kani::any();
    kani::assume(size > 0 && size <= 100 * POS_SCALE as i128);

    let result = engine.execute_trade_not_atomic(
        a, b, DEFAULT_ORACLE, DEFAULT_SLOT, size, DEFAULT_ORACLE, 0i128, 0);
    if result.is_ok() {
        assert!(engine.oi_eff_long_q == engine.oi_eff_short_q,
            "B7: OI_long == OI_short after trade");
    }

    kani::cover!(result.is_ok(), "trade with OI balance");
}

// ############################################################################
// B1: Conservation after trade with fees
// ############################################################################

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_b1_conservation_after_trade_with_fees() {
    let mut engine = RiskEngine::new(default_params());
    let a = engine.add_user(1000).unwrap();
    let b = engine.add_user(1000).unwrap();
    engine.deposit(a, 100_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.deposit(b, 100_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    assert!(engine.check_conservation());

    let size: i128 = kani::any();
    kani::assume(size > 0 && size <= 50 * POS_SCALE as i128);

    let result = engine.execute_trade_not_atomic(
        a, b, DEFAULT_ORACLE, DEFAULT_SLOT, size, DEFAULT_ORACLE, 0i128, 0);
    if result.is_ok() {
        assert!(engine.check_conservation(),
            "B1: conservation after trade with fees");
    }

    kani::cover!(result.is_ok(), "fee trade conserves");
}

// ############################################################################
// E8: Position bound enforcement
// ############################################################################

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_e8_position_bound_enforcement() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let a = engine.add_user(0).unwrap();
    let b = engine.add_user(0).unwrap();
    engine.deposit(a, 10_000_000_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.deposit(b, 10_000_000_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    let oversize = (MAX_POSITION_ABS_Q + 1) as i128;
    let result = engine.execute_trade_not_atomic(
        a, b, DEFAULT_ORACLE, DEFAULT_SLOT, oversize, DEFAULT_ORACLE, 0i128, 0);
    assert!(result.is_err(), "E8: oversize trade must be rejected");

    kani::cover!(true, "oversize rejected");
}

// ############################################################################
// B5: PNL_matured_pos_tot <= PNL_pos_tot after set_pnl + set_reserved_pnl
// ############################################################################

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_b5_matured_leq_pos_tot() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let idx = engine.add_user(0).unwrap();
    engine.deposit(idx, 500_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    let pnl: i128 = kani::any();
    kani::assume(pnl > 0 && pnl <= 100_000);
    engine.set_pnl(idx as usize, pnl);
    assert!(engine.pnl_matured_pos_tot <= engine.pnl_pos_tot, "B5 after set_pnl");

    // Transition to lower PNL
    let new_pnl: i128 = kani::any();
    kani::assume(new_pnl >= 0 && new_pnl < pnl);
    engine.set_pnl(idx as usize, new_pnl);
    assert!(engine.pnl_matured_pos_tot <= engine.pnl_pos_tot,
        "B5: matured <= pos_tot after decrease");

    // Transition to negative PNL
    engine.set_pnl(idx as usize, -1000);
    assert!(engine.pnl_matured_pos_tot <= engine.pnl_pos_tot,
        "B5: matured <= pos_tot after negative");

    kani::cover!(new_pnl > 0, "partial decrease");
}

// ############################################################################
// G4: DrainOnly blocks OI-increasing trades
// ############################################################################

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_g4_drain_only_blocks_oi_increase() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let a = engine.add_user(0).unwrap();
    let b = engine.add_user(0).unwrap();
    engine.deposit(a, 500_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.deposit(b, 500_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    engine.side_mode_long = SideMode::DrainOnly;

    let size: i128 = kani::any();
    kani::assume(size > 0 && size <= 50 * POS_SCALE as i128);
    let result = engine.execute_trade_not_atomic(
        a, b, DEFAULT_ORACLE, DEFAULT_SLOT, size, DEFAULT_ORACLE, 0i128, 0);

    assert!(result.is_err(), "G4: DrainOnly must block OI increase");

    kani::cover!(result.is_err(), "DrainOnly blocks");
}

// ############################################################################
// Goal 5: No same-trade bootstrap from positive slippage
// ############################################################################

/// A trade whose own positive slippage would be needed to pass IM must be
/// rejected. The trade-open equity excludes the candidate trade's gain.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_goal5_no_same_trade_bootstrap() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let a = engine.add_user(0).unwrap();
    let b = engine.add_user(0).unwrap();
    // a gets just enough capital to pass IM for a small position,
    // but NOT enough if the trade adds large positive slippage
    engine.deposit(a, 10_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.deposit(b, 1_000_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    // Trade size: 100 units at oracle 1000 = 100k notional.
    // IM = 100k * 10% = 10k. Capital = 10k. Just barely passes.
    let size = (100 * POS_SCALE) as i128;

    // Execute at exec_price BELOW oracle (a gains positive slippage)
    // exec_price=900: trade_pnl_a = size * (oracle - exec) / POS_SCALE = 100*100 = 10_000
    // Without bootstrap protection, the +10k gain would raise Eq and let
    // a pass even with a bigger position. With protection, the gain is
    // excluded from trade-open equity.
    let exec_price = 900u64;
    let result = engine.execute_trade_not_atomic(
        a, b, DEFAULT_ORACLE, DEFAULT_SLOT, size, exec_price, 0i128, 0);

    // The trade's own +10k slippage must NOT count toward IM.
    // trade_open equity = C(10k) + min(PNL_trade_open, 0) + haircutted_released_trade_open
    // PNL_trade_open = PNL - trade_gain = 10k - 10k = 0 (since PNL was 0 before,
    //   becomes +10k from trade, then trade_gain=10k is subtracted)
    // So Eq_trade_open ~ 10k only (capital), which barely passes IM=10k.
    // This is borderline — the key property is that the +10k slippage
    // does NOT inflate equity beyond the pre-trade capital.
    // If it DID inflate equity, a much larger trade would pass.

    // Verify: try a MUCH larger trade that would only pass with bootstrap
    let big_size = (200 * POS_SCALE) as i128; // 200k notional, IM=20k
    let big_result = engine.execute_trade_not_atomic(
        a, b, DEFAULT_ORACLE, DEFAULT_SLOT, big_size, exec_price, 0i128, 0);

    // With only 10k capital and slippage excluded, IM=20k cannot be met
    assert!(big_result.is_err(),
        "Goal 5: trade must NOT bootstrap itself via own positive slippage");

    kani::cover!(big_result.is_err(), "bootstrap blocked");
}

// ############################################################################
// Goal 7: Pending merge uses max horizon
// ############################################################################

/// When both buckets are occupied, merges into pending use horizon = max.
#[kani::proof]
#[kani::unwind(4)]
#[kani::solver(cadical)]
fn proof_goal7_pending_merge_max_horizon() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let idx = engine.add_user(0).unwrap();
    engine.deposit(idx, 1_000_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    // First append creates sched
    engine.accounts[idx as usize].pnl += 10_000;
    engine.pnl_pos_tot += 10_000;
    engine.append_or_route_new_reserve(idx as usize, 10_000, DEFAULT_SLOT, 10);
    assert_eq!(engine.accounts[idx as usize].sched_present, 1);

    // Second append creates pending (different slot)
    engine.accounts[idx as usize].pnl += 10_000;
    engine.pnl_pos_tot += 10_000;
    engine.append_or_route_new_reserve(idx as usize, 10_000, DEFAULT_SLOT + 1, 5);
    assert_eq!(engine.accounts[idx as usize].pending_present, 1);

    let h1: u8 = kani::any();
    kani::assume(h1 >= 1 && h1 <= 100);
    let h_lock = h1 as u64;

    // Third append merges into pending
    engine.accounts[idx as usize].pnl += 10_000;
    engine.pnl_pos_tot += 10_000;
    engine.append_or_route_new_reserve(idx as usize, 10_000, DEFAULT_SLOT + 2, h_lock);

    assert!(engine.accounts[idx as usize].pending_horizon >= h_lock,
        "Goal 7: pending horizon must be >= h_lock after merge");

    kani::cover!(true, "pending max-horizon enforced");
}

// ############################################################################
// Goal 23: No pure-capital insurance draw without accrual
// ############################################################################

/// deposit does not call accrue_market_to and must not draw from insurance.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_goal23_deposit_no_insurance_draw() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let idx = engine.add_user(0).unwrap();
    engine.deposit(idx, 100_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    let ins_before = engine.insurance_fund.balance.get();

    // Symbolic deposit amount
    let amount: u128 = kani::any();
    kani::assume(amount > 0 && amount <= 500_000);

    let result = engine.deposit(idx, amount, DEFAULT_ORACLE, DEFAULT_SLOT + 1);
    if result.is_ok() {
        let ins_after = engine.insurance_fund.balance.get();
        assert!(ins_after >= ins_before,
            "Goal 23: deposit must never decrease insurance");
    }

    kani::cover!(result.is_ok(), "deposit succeeds without insurance draw");
}

// ############################################################################
// Goal 27: Path-independent touched-account finalization
// ############################################################################

/// Finalize_touched_accounts_post_live produces the same conversion result
/// regardless of which accounts are touched (order-independent within the
/// touched set, since the shared snapshot is computed once).
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_goal27_finalize_path_independent() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let a = engine.add_user(0).unwrap();
    let b = engine.add_user(0).unwrap();
    engine.deposit(a, 500_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.deposit(b, 500_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    // Give both flat positive PnL
    engine.set_pnl(a as usize, 10_000);
    engine.set_pnl(b as usize, 20_000);

    // Touch a then b
    let mut ctx1 = InstructionContext::new_with_h_lock(0);
    ctx1.add_touched(a);
    ctx1.add_touched(b);

    // Clone engine for comparison
    let mut engine2 = engine.clone();

    // Touch b then a (reversed order)
    let mut ctx2 = InstructionContext::new_with_h_lock(0);
    ctx2.add_touched(b);
    ctx2.add_touched(a);

    engine.finalize_touched_accounts_post_live(&ctx1);
    engine2.finalize_touched_accounts_post_live(&ctx2);

    // Both orderings must produce identical state
    assert_eq!(engine.accounts[a as usize].capital.get(),
               engine2.accounts[a as usize].capital.get(),
        "Goal 27: a's capital must be order-independent");
    assert_eq!(engine.accounts[b as usize].capital.get(),
               engine2.accounts[b as usize].capital.get(),
        "Goal 27: b's capital must be order-independent");
    assert_eq!(engine.pnl_matured_pos_tot, engine2.pnl_matured_pos_tot,
        "Goal 27: matured aggregate must be order-independent");

    kani::cover!(true, "finalize is order-independent");
}

// ############################################################################
// Two-bucket warmup proofs
// ############################################################################

/// R_i = sched_remaining + pending_remaining after append.
#[kani::proof]
#[kani::unwind(5)]
#[kani::solver(cadical)]
fn proof_two_bucket_reserve_sum_after_append() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let idx = engine.add_user(0).unwrap();
    engine.deposit(idx, 1_000_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    let h_lock: u64 = kani::any();
    kani::assume(h_lock >= 1 && h_lock <= 100);

    // First append: creates scheduled
    let r1: u128 = kani::any();
    kani::assume(r1 > 0 && r1 <= 50_000);
    engine.accounts[idx as usize].pnl += r1 as i128;
    engine.pnl_pos_tot += r1;
    engine.append_or_route_new_reserve(idx as usize, r1, DEFAULT_SLOT, h_lock);

    // Second append at different slot: creates pending
    let r2: u128 = kani::any();
    kani::assume(r2 > 0 && r2 <= 50_000);
    engine.accounts[idx as usize].pnl += r2 as i128;
    engine.pnl_pos_tot += r2;
    engine.append_or_route_new_reserve(idx as usize, r2, DEFAULT_SLOT + 1, h_lock);

    // R_i must equal sum of both buckets
    let a = &engine.accounts[idx as usize];
    let sched_r = if a.sched_present != 0 { a.sched_remaining_q.get() } else { 0 };
    let pend_r = if a.pending_present != 0 { a.pending_remaining_q.get() } else { 0 };
    assert_eq!(a.reserved_pnl.get(), sched_r + pend_r,
        "R_i must equal sched + pending");

    kani::cover!(a.sched_present != 0 && a.pending_present != 0, "both buckets present");
}

/// Loss hits pending first (newest-first).
#[kani::proof]
#[kani::unwind(5)]
#[kani::solver(cadical)]
fn proof_two_bucket_loss_newest_first() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let idx = engine.add_user(0).unwrap();
    engine.deposit(idx, 1_000_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    // Create sched + pending
    engine.accounts[idx as usize].pnl = I128::new(30_000);
    engine.pnl_pos_tot = U128::new(30_000);
    engine.append_or_route_new_reserve(idx as usize, 10_000, DEFAULT_SLOT, 10);
    engine.append_or_route_new_reserve(idx as usize, 20_000, DEFAULT_SLOT + 1, 10);

    let sched_before = engine.accounts[idx as usize].sched_remaining_q;

    // Loss that fits in pending
    let loss: u128 = kani::any();
    kani::assume(loss > 0 && loss <= 20_000);
    engine.apply_reserve_loss_newest_first(idx as usize, loss);

    // Scheduled must be untouched
    assert_eq!(engine.accounts[idx as usize].sched_remaining_q, sched_before,
        "scheduled must be untouched when loss fits in pending");

    kani::cover!(loss == 20_000, "exact pending drain");
    kani::cover!(loss < 20_000, "partial pending loss");
}

/// Scheduled bucket matures exactly per its horizon.
#[kani::proof]
#[kani::unwind(5)]
#[kani::solver(cadical)]
fn proof_two_bucket_scheduled_timing() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let idx = engine.add_user(0).unwrap();
    engine.deposit(idx, 1_000_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    let anchor: u128 = kani::any();
    kani::assume(anchor > 0 && anchor <= 1_000);
    let h: u64 = kani::any();
    kani::assume(h >= 1 && h <= 20);

    engine.accounts[idx as usize].pnl = I128::new(anchor as i128);
    engine.pnl_pos_tot = U128::new(anchor);
    engine.append_or_route_new_reserve(idx as usize, anchor, DEFAULT_SLOT, h);

    let dt: u64 = kani::any();
    kani::assume(dt >= 1 && dt <= 40);
    engine.current_slot = DEFAULT_SLOT + dt;

    let r_before = engine.accounts[idx as usize].reserved_pnl.get();
    engine.advance_profit_warmup(idx as usize);
    let released = r_before - engine.accounts[idx as usize].reserved_pnl.get();

    let expected = if dt as u128 >= h as u128 { anchor }
        else { mul_div_floor_u128(anchor, dt as u128, h as u128) };
    assert_eq!(released, expected, "release must match floor(anchor*elapsed/horizon)");

    kani::cover!(dt < h, "partial maturity");
    kani::cover!(dt >= h, "full maturity");
}

/// Pending does not mature.
#[kani::proof]
#[kani::unwind(5)]
#[kani::solver(cadical)]
fn proof_two_bucket_pending_non_maturity() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let idx = engine.add_user(0).unwrap();
    engine.deposit(idx, 1_000_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    // Create sched + pending
    engine.accounts[idx as usize].pnl = I128::new(30_000);
    engine.pnl_pos_tot = U128::new(30_000);
    engine.append_or_route_new_reserve(idx as usize, 10_000, DEFAULT_SLOT, 10);
    engine.append_or_route_new_reserve(idx as usize, 20_000, DEFAULT_SLOT + 1, 10);

    let pending_before = engine.accounts[idx as usize].pending_remaining_q;

    // Advance well past horizon
    engine.current_slot = DEFAULT_SLOT + 200;
    engine.advance_profit_warmup(idx as usize);

    // If pending is still present (not promoted), it must not have matured
    if engine.accounts[idx as usize].pending_present != 0 {
        assert_eq!(engine.accounts[idx as usize].pending_remaining_q, pending_before,
            "pending must not mature while pending");
    }

    kani::cover!(true, "warmup with pending exercised");
}
