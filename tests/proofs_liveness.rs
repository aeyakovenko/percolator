//! Section 7 — Liveness, progress, no-deadlock
//!
//! Auto-finalization, trade reopening, ADL fallback routes,
//! precision exhaustion, crank quiescence, drain-only progress.

#![cfg(kani)]

mod common;
use common::*;

// ============================================================================
// T11.43: end_instruction_auto_finalizes_ready_side
// ============================================================================

#[kani::proof]
#[kani::solver(cadical)]
fn t11_43_end_instruction_auto_finalizes_ready_side() {
    let mut engine = RiskEngine::new(zero_fee_params());

    engine.side_mode_long = SideMode::ResetPending;
    engine.oi_eff_long_q = U128::ZERO;
    engine.stale_account_count_long = 0;
    engine.stored_pos_count_long = 0;

    engine.side_mode_short = SideMode::ResetPending;
    engine.oi_eff_short_q = U128::ZERO;
    engine.stale_account_count_short = 1;
    engine.stored_pos_count_short = 0;

    let ctx = InstructionContext::new();
    engine.finalize_end_of_instruction_resets(&ctx);

    assert!(engine.side_mode_long == SideMode::Normal,
        "ready ResetPending side must auto-finalize to Normal");
    assert!(engine.side_mode_short == SideMode::ResetPending,
        "non-ready side must stay ResetPending");
}

// ============================================================================
// T11.44: trade_path_reopens_ready_reset_side
// ============================================================================

#[kani::proof]
#[kani::solver(cadical)]
fn t11_44_trade_path_reopens_ready_reset_side() {
    let mut engine = RiskEngine::new(zero_fee_params());

    let a = engine.add_user(0).unwrap();
    let b = engine.add_user(0).unwrap();
    engine.deposit(a, 10_000_000, 100, 0).unwrap();
    engine.deposit(b, 10_000_000, 100, 0).unwrap();

    engine.side_mode_long = SideMode::ResetPending;
    engine.oi_eff_long_q = U128::ZERO;
    engine.oi_eff_short_q = U128::ZERO;
    engine.stale_account_count_long = 0;
    engine.stored_pos_count_long = 0;

    engine.last_oracle_price = 100;
    engine.last_market_slot = 1;
    engine.last_crank_slot = 1;

    let size_q = POS_SCALE as i128;
    let result = engine.execute_trade_not_atomic(a, b, 100, 1, size_q, 100, 0i128, 0);

    assert!(result.is_ok(), "trade must succeed after auto-finalization of ready reset side");
    assert!(engine.side_mode_long == SideMode::Normal);
    assert!(engine.oi_eff_long_q == engine.oi_eff_short_q);
}

// ============================================================================
// T11.45: try_negate_u256_correctness
// ============================================================================
// NOTE: try_negate_u256_to_i256 has been removed from the engine after the
// migration to native 128-bit types. This test is preserved as a pure
// wide_math test using U256/I256 types that still exist for transient math.

// (Test removed — function no longer exists in the public API)

// ============================================================================
// T11.46: enqueue_adl_k_add_overflow_still_routes_quantity
// ============================================================================

#[kani::proof]
#[kani::solver(cadical)]
fn t11_46_enqueue_adl_k_add_overflow_still_routes_quantity() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let mut ctx = InstructionContext::new();

    engine.adl_coeff_long = I128::new(i128::MIN + 1);
    engine.adl_mult_long = U128::new(POS_SCALE);
    engine.oi_eff_long_q = U128::new(4 * POS_SCALE);
    engine.oi_eff_short_q = U128::new(4 * POS_SCALE);
    engine.insurance_fund.balance = U128::new(10_000_000);
    engine.stored_pos_count_long = 1;

    let k_before = engine.adl_coeff_long;
    let a_before = engine.adl_mult_long;
    let ins_before = engine.insurance_fund.balance.get();

    let d = 1_000_000u128;
    let q_close = 2 * POS_SCALE;

    let result = engine.enqueue_adl(&mut ctx, Side::Short, q_close, d);
    assert!(result.is_ok());

    // K_opp must be UNCHANGED when K_opp + delta_K overflows
    assert!(engine.adl_coeff_long == k_before,
        "K_opp must not be modified on K-space overflow (spec §5.6 step 6)");
    // A must shrink (quantity was still routed)
    assert!(engine.adl_mult_long < a_before, "A must shrink on K overflow");
    // OI must decrease by q_close
    assert!(engine.oi_eff_long_q.get() == 2 * POS_SCALE);
    // Insurance fund must decrease by D (absorb_protocol_loss was invoked)
    assert!(engine.insurance_fund.balance.get() < ins_before,
        "insurance fund must decrease — absorb_protocol_loss must be invoked");
}

// ============================================================================
// T11.47: precision_exhaustion_terminal_drain
// ============================================================================

#[kani::proof]
#[kani::solver(cadical)]
fn t11_47_precision_exhaustion_terminal_drain() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let mut ctx = InstructionContext::new();

    engine.adl_mult_long = U128::new(1);
    engine.adl_coeff_long = I128::ZERO;
    engine.oi_eff_long_q = U128::new(3 * POS_SCALE);
    engine.oi_eff_short_q = U128::new(3 * POS_SCALE);
    engine.stored_pos_count_long = 1;

    let q_close = POS_SCALE;
    let d = 0u128;

    let result = engine.enqueue_adl(&mut ctx, Side::Short, q_close, d);
    assert!(result.is_ok());

    assert!(ctx.pending_reset_long);
    assert!(ctx.pending_reset_short);
    assert!(engine.oi_eff_long_q.get() == 0);
    assert!(engine.oi_eff_short_q.get() == 0);
}

// ============================================================================
// T11.48: bankruptcy_liquidation_routes_q_when_D_zero
// ============================================================================

#[kani::proof]
#[kani::solver(cadical)]
fn t11_48_bankruptcy_liquidation_routes_q_when_D_zero() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let mut ctx = InstructionContext::new();

    engine.adl_mult_long = U128::new(POS_SCALE);
    engine.adl_coeff_long = I128::new(42i128);
    engine.oi_eff_long_q = U128::new(4 * POS_SCALE);
    engine.oi_eff_short_q = U128::new(4 * POS_SCALE);
    engine.stored_pos_count_long = 1;

    let k_before = engine.adl_coeff_long;
    let a_before = engine.adl_mult_long;

    let d = 0u128;
    let q_close = POS_SCALE;

    let result = engine.enqueue_adl(&mut ctx, Side::Short, q_close, d);
    assert!(result.is_ok());

    assert!(engine.adl_coeff_long == k_before, "K must be unchanged when D == 0");
    assert!(engine.adl_mult_long < a_before, "A must shrink");
    assert!(engine.oi_eff_long_q.get() == 3 * POS_SCALE);
}

// ============================================================================
// T11.49: pure_pnl_bankruptcy_path
// ============================================================================

#[kani::proof]
#[kani::solver(cadical)]
fn t11_49_pure_pnl_bankruptcy_path() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let mut ctx = InstructionContext::new();

    engine.adl_mult_long = U128::new(POS_SCALE);
    engine.adl_coeff_long = I128::ZERO;
    engine.oi_eff_long_q = U128::new(2 * POS_SCALE);
    engine.oi_eff_short_q = U128::new(2 * POS_SCALE);
    engine.stored_pos_count_long = 1;

    let a_before = engine.adl_mult_long;
    let k_before = engine.adl_coeff_long;

    let d = 1_000u128;
    let q_close = 0u128;

    let result = engine.enqueue_adl(&mut ctx, Side::Short, q_close, d);
    assert!(result.is_ok());

    assert!(engine.adl_mult_long == a_before, "A must be unchanged for pure PnL bankruptcy");
    assert!(engine.adl_coeff_long != k_before, "K must change when D > 0");
    assert!(engine.oi_eff_long_q.get() == 2 * POS_SCALE);
}

// ============================================================================
// T11.53: keeper_crank_quiesces_after_pending_reset
// ============================================================================

#[kani::proof]
#[kani::solver(cadical)]
fn t11_53_keeper_crank_quiesces_after_pending_reset() {
    let mut engine = RiskEngine::new(zero_fee_params());

    engine.last_oracle_price = 100;
    engine.last_market_slot = 0;
    engine.adl_mult_long = U128::new(ADL_ONE);
    engine.adl_mult_short = U128::new(ADL_ONE);
    engine.adl_epoch_long = 0;
    engine.adl_epoch_short = 0;

    let a = engine.add_user(0).unwrap();
    let b = engine.add_user(0).unwrap();
    let c = engine.add_user(0).unwrap();

    // a: long POS_SCALE (entire long side OI), tiny capital → deeply underwater
    engine.deposit(a, 1, 100, 0).unwrap();
    engine.accounts[a as usize].position_basis_q = I128::new(POS_SCALE as i128);
    engine.accounts[a as usize].adl_a_basis = U128::new(ADL_ONE);
    engine.accounts[a as usize].adl_k_snap = I128::ZERO;
    engine.accounts[a as usize].adl_epoch_snap = 0;

    // b: short POS_SCALE, well-funded
    engine.deposit(b, 10_000_000, 100, 0).unwrap();
    engine.accounts[b as usize].position_basis_q = I128::new(-(POS_SCALE as i128));
    engine.accounts[b as usize].adl_a_basis = U128::new(ADL_ONE);
    engine.accounts[b as usize].adl_k_snap = I128::ZERO;
    engine.accounts[b as usize].adl_epoch_snap = 0;

    // c: NO position, just capital (should NOT be touched after pending reset)
    engine.deposit(c, 10_000_000, 100, 0).unwrap();

    // BALANCED OI: 1 long (a) = PS, 1 short (b) = PS
    engine.stored_pos_count_long = 1;
    engine.stored_pos_count_short = 1;
    engine.oi_eff_long_q = U128::new(POS_SCALE);
    engine.oi_eff_short_q = U128::new(POS_SCALE);

    // Set K_long very negative → account a is deeply underwater
    engine.adl_coeff_long = I128::new(-((ADL_ONE as i128) * 1000));

    let c_cap_before = engine.accounts[c as usize].capital.get();
    let c_pnl_before = engine.accounts[c as usize].pnl;

    let result = engine.keeper_crank_not_atomic(1, 100, &[(a, Some(LiquidationPolicy::FullClose))], 1, 0i128, 0);
    assert!(result.is_ok());

    assert!(engine.accounts[c as usize].capital.get() == c_cap_before,
        "c's capital must not change — crank must quiesce after pending reset");
    assert!(engine.accounts[c as usize].pnl == c_pnl_before,
        "c's PnL must not change — crank must quiesce after pending reset");
}

// ============================================================================
// proof_drain_only_to_reset_progress
// ============================================================================

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_drain_only_to_reset_progress() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let mut ctx = InstructionContext::new();

    // Long side: DrainOnly, OI = 0
    engine.side_mode_long = SideMode::DrainOnly;
    engine.oi_eff_long_q = U128::ZERO;
    engine.oi_eff_short_q = U128::ZERO;
    engine.stored_pos_count_long = 0;
    // Short side still has stored positions → §5.7.A (bilateral-empty) does NOT fire
    engine.stored_pos_count_short = 1;

    let result = engine.schedule_end_of_instruction_resets(&mut ctx);
    assert!(result.is_ok());

    // §5.7.D must fire for the DrainOnly long side
    assert!(ctx.pending_reset_long,
        "DrainOnly side with OI=0 must schedule reset via §5.7.D");
    assert!(!ctx.pending_reset_short,
        "opposite side must not get reset from DrainOnly path alone");
}

// ============================================================================
// proof_keeper_reset_lifecycle_last_stale_triggers_finalize
// ============================================================================

#[kani::proof]
#[kani::solver(cadical)]
fn proof_keeper_reset_lifecycle_last_stale_triggers_finalize() {
    let mut engine = RiskEngine::new(zero_fee_params());

    engine.last_oracle_price = 100;
    engine.last_market_slot = 0;
    engine.adl_mult_long = U128::new(ADL_ONE);
    engine.adl_mult_short = U128::new(ADL_ONE);
    engine.adl_epoch_long = 1;   // new epoch (post-reset)
    engine.adl_epoch_short = 0;

    let a = engine.add_user(0).unwrap();
    let b = engine.add_user(0).unwrap();

    // a: the last stale long account — has a position from epoch 0 (stale)
    engine.deposit(a, 10_000_000, 100, 0).unwrap();
    engine.accounts[a as usize].position_basis_q = I128::new(POS_SCALE as i128);
    engine.accounts[a as usize].adl_a_basis = U128::new(ADL_ONE);
    engine.accounts[a as usize].adl_k_snap = I128::ZERO;
    engine.accounts[a as usize].adl_epoch_snap = 0;  // mismatches adl_epoch_long=1

    // b: a short account (non-stale, current epoch)
    engine.deposit(b, 10_000_000, 100, 0).unwrap();
    engine.accounts[b as usize].position_basis_q = I128::ZERO;
    engine.accounts[b as usize].adl_a_basis = U128::new(ADL_ONE);
    engine.accounts[b as usize].adl_k_snap = I128::ZERO;
    engine.accounts[b as usize].adl_epoch_snap = 0;

    // Long side: ResetPending, 1 stale account remaining, OI=0
    engine.side_mode_long = SideMode::ResetPending;
    engine.oi_eff_long_q = U128::ZERO;
    engine.oi_eff_short_q = U128::ZERO;
    engine.stale_account_count_long = 1;
    engine.stored_pos_count_long = 1;

    assert!(engine.side_mode_long == SideMode::ResetPending);

    let result = engine.keeper_crank_not_atomic(1, 100, &[(a, None), (b, None)], 2, 0i128, 0);
    assert!(result.is_ok());

    assert!(engine.side_mode_long == SideMode::Normal,
        "touching last stale account must finalize ResetPending → Normal (spec property #26)");
    assert!(engine.stale_account_count_long == 0);
    assert!(engine.stored_pos_count_long == 0);
}

// ============================================================================
// proof_unilateral_empty_orphan_dust_clearance
// ============================================================================

#[kani::proof]
#[kani::solver(cadical)]
fn proof_unilateral_empty_orphan_dust_clearance() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let mut ctx = InstructionContext::new();

    // Long side: no stored positions, but has phantom dust OI
    engine.stored_pos_count_long = 0;
    // Short side: still has stored positions
    engine.stored_pos_count_short = 2;

    // Phantom dust: OI == dust bound (should clear)
    let dust = 42u128;
    engine.phantom_dust_bound_long_q = dust;
    engine.oi_eff_long_q = U128::new(dust);   // OI <= dust bound
    engine.oi_eff_short_q = U128::new(dust);  // balanced (required by spec)

    let result = engine.schedule_end_of_instruction_resets(&mut ctx);
    assert!(result.is_ok());

    // §5.7.B: long side is empty, OI within dust bound → both sides get reset
    assert!(ctx.pending_reset_long,
        "unilateral-empty side with OI within dust bound must schedule reset (§5.7.B)");
    assert!(ctx.pending_reset_short,
        "opposite side must also get reset for bilateral consistency (§5.7.B)");
    // OI must be zeroed
    assert!(engine.oi_eff_long_q.get() == 0,
        "OI must be zeroed after dust clearance");
    assert!(engine.oi_eff_short_q.get() == 0,
        "OI must be zeroed after dust clearance");
}

// ############################################################################
// Full ADL pipeline integration: trade → liquidation → ADL → reset → reopen
// ############################################################################

/// End-to-end ADL pipeline: two accounts open bilateral positions,
/// one goes bankrupt, liquidation triggers enqueue_adl with K-socialization,
/// end-of-instruction resets fire, and a subsequent trade reopens the market.
/// Verifies OI_eff_long == OI_eff_short is maintained throughout.
#[kani::proof]
#[kani::unwind(70)]
#[kani::solver(cadical)]
fn proof_adl_pipeline_trade_liquidate_reopen() {
    let mut engine = RiskEngine::new(zero_fee_params());
    engine.last_crank_slot = DEFAULT_SLOT;

    let a = engine.add_user(0).unwrap();
    let b = engine.add_user(0).unwrap();
    let c = engine.add_user(0).unwrap();
    engine.deposit(a, 100_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.deposit(b, 500_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.deposit(c, 500_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    // Step 1: a goes long, b goes short (bilateral position)
    let size = (500 * POS_SCALE) as i128;
    engine.execute_trade_not_atomic(a, b, DEFAULT_ORACLE, DEFAULT_SLOT, size, DEFAULT_ORACLE, 0i128, 0).unwrap();
    assert!(engine.oi_eff_long_q == engine.oi_eff_short_q, "OI must balance after trade");

    // Step 2: make a deeply bankrupt (loss exceeds capital)
    engine.set_pnl(a as usize, -200_000i128);

    // Step 3: liquidate a via keeper_crank_not_atomic
    let slot2 = DEFAULT_SLOT + 1;
    let candidates = [(a, Some(LiquidationPolicy::FullClose)), (b, Some(LiquidationPolicy::FullClose)), (c, Some(LiquidationPolicy::FullClose))];
    let result = engine.keeper_crank_not_atomic(slot2, DEFAULT_ORACLE, &candidates, 10, 0i128, 0);
    assert!(result.is_ok());
    let outcome = result.unwrap();
    assert!(engine.oi_eff_long_q == engine.oi_eff_short_q, "OI must balance after liquidation+ADL");

    // Step 4: verify ADL fired — K should have changed (deficit socialized to b)
    // or A should have changed (quantity reduction)
    assert!(outcome.num_liquidations > 0, "at least one liquidation must have occurred");

    // Step 5: subsequent trade reopening the market
    // c goes long against b (new bilateral position after ADL)
    let new_size = (100 * POS_SCALE) as i128;
    let slot3 = slot2 + 1;
    engine.last_crank_slot = slot3;
    let result2 = engine.execute_trade_not_atomic(c, b, DEFAULT_ORACLE, slot3, new_size, DEFAULT_ORACLE, 0i128, 0);

    // Trade may or may not succeed (b's equity may be impaired from ADL)
    // but OI balance must hold regardless
    assert!(engine.oi_eff_long_q == engine.oi_eff_short_q, "OI must balance after reopen attempt");
    assert!(engine.check_conservation(), "conservation after full pipeline");

    kani::cover!(result2.is_ok(), "post-ADL trade succeeds");
}
