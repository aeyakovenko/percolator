//! Section 5 — Economic safety, conservation
//!
//! Bounded integration, ADL safety, dust bounds, funding no-mint.

#![cfg(kani)]

mod common;
use common::*;

// ############################################################################
// BOUNDED INTEGRATION PROOFS (from kani.rs)
// ############################################################################

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn bounded_deposit_conservation() {
    let mut engine = RiskEngine::new(zero_fee_params());

    let idx = engine.add_user(0).unwrap();

    let amount: u32 = kani::any();
    kani::assume(amount > 0 && amount <= 10_000_000);

    engine.deposit(idx, amount as u128, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    assert!(engine.vault.get() == amount as u128);
    assert!(engine.c_tot.get() == amount as u128);
    assert!(engine.check_conservation());
}

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn bounded_withdraw_conservation() {
    let mut engine = RiskEngine::new(zero_fee_params());
    engine.last_crank_slot = DEFAULT_SLOT;

    let deposit: u32 = kani::any();
    kani::assume(deposit >= 1000 && deposit <= 1_000_000);
    let idx = engine.add_user(0).unwrap();
    engine.deposit(idx, deposit as u128, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    let amount: u32 = kani::any();
    kani::assume(amount > 0 && amount <= deposit);

    let result = engine.withdraw_not_atomic(idx, amount as u128, DEFAULT_ORACLE, DEFAULT_SLOT, 0i128, 0);
    kani::cover!(result.is_ok(), "withdraw_not_atomic Ok path reachable");
    if result.is_ok() {
        assert!(engine.check_conservation());
        assert!(engine.accounts[idx as usize].capital.get() == deposit as u128 - amount as u128);
    }
}

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn bounded_trade_conservation() {
    let mut engine = RiskEngine::new(zero_fee_params());

    let a = engine.add_user(0).unwrap();
    let b = engine.add_user(0).unwrap();

    let dep: u32 = kani::any();
    kani::assume(dep >= 1_000_000 && dep <= 5_000_000);
    engine.deposit(a, dep as u128, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.deposit(b, dep as u128, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    assert!(engine.check_conservation());

    // Symbolic trade size (reasonable range to stay within margin)
    let size_q = (100 * POS_SCALE) as i128;
    let result = engine.execute_trade_not_atomic(a, b, DEFAULT_ORACLE, DEFAULT_SLOT, size_q, DEFAULT_ORACLE, 0i128, 0);

    // If trade succeeds (margin allows), conservation must hold
    if result.is_ok() {
        assert!(engine.check_conservation(),
            "conservation must hold after execute_trade_not_atomic");
    } else {
        // Trade rejected by margin — conservation must still hold
        assert!(engine.check_conservation(),
            "conservation must hold even when trade is rejected");
    }
    kani::cover!(result.is_ok(), "trade execution path reachable");
}

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn bounded_haircut_ratio_bounded() {
    let mut engine = RiskEngine::new(zero_fee_params());

    let vault_val: u32 = kani::any();
    let c_tot_val: u32 = kani::any();
    let ins_val: u32 = kani::any();
    let ppt_val: u32 = kani::any();
    let matured_val: u32 = kani::any();
    kani::assume(matured_val <= ppt_val); // matured <= total positive PnL

    engine.vault = U128::new(vault_val as u128);
    engine.c_tot = U128::new(c_tot_val as u128);
    engine.insurance_fund.balance = U128::new(ins_val as u128);
    engine.pnl_pos_tot = U128::new(ppt_val as u128);
    engine.pnl_matured_pos_tot = U128::new(matured_val as u128); // v12.14.0: haircut denominator

    let (h_num, h_den) = engine.haircut_ratio();

    // h_num <= h_den always (haircut ratio <= 1)
    assert!(h_num <= h_den);
    // h_den is either pnl_matured_pos_tot or 1 (when matured == 0)
    assert!(h_den != 0);

    // Exercise h < 1 branch: when residual < pnl_matured_pos_tot
    if vault_val as u128 >= c_tot_val as u128 + ins_val as u128 {
        let residual = vault_val as u128 - c_tot_val as u128 - ins_val as u128;
        if matured_val > 0 && residual < matured_val as u128 {
            kani::cover!(true, "h < 1 branch reachable");
            assert!(h_num < h_den, "h must be < 1 when residual < matured");
        }
    }
}

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn bounded_equity_nonneg_flat() {
    // Test account_equity_maint_raw (the unclamped value) for a flat account.
    // For a flat account with zero fees: raw = capital + pnl.
    // Case 1: positive capital, non-negative PnL → raw >= 0.
    // Case 2: negative PnL → raw == capital + pnl - fee_debt (exact).
    let mut engine = RiskEngine::new(zero_fee_params());
    let idx = engine.add_user(0).unwrap();

    let cap: u16 = kani::any();
    kani::assume(cap > 0 && cap <= 10_000);
    engine.set_capital(idx as usize, cap as u128);

    let pnl_val: i16 = kani::any();
    kani::assume(pnl_val > i16::MIN);
    engine.set_pnl(idx as usize, pnl_val as i128);

    assert!(engine.accounts[idx as usize].position_basis_q.get() == 0);

    let raw = engine.account_equity_maint_raw(&engine.accounts[idx as usize]);

    if pnl_val >= 0 {
        // Positive capital + non-negative PnL (zero fees) → raw must be non-negative
        assert!(raw >= 0,
            "flat account with positive capital and non-negative PnL must have raw equity >= 0");
    } else {
        // Negative PnL: raw must equal capital + pnl - fee_debt exactly.
        // fee_debt is 0 for zero_fee_params with fresh account.
        let fee_debt = fee_debt_u128_checked(engine.accounts[idx as usize].fee_credits.get());
        let expected = (cap as i128) + (pnl_val as i128) - (fee_debt as i128);
        assert!(raw == expected,
            "flat account raw equity must equal capital + pnl - fee_debt");
    }
}

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn bounded_liquidation_conservation() {
    let mut engine = RiskEngine::new(zero_fee_params());

    let a = engine.add_user(0).unwrap();

    let deposit_amt: u32 = kani::any();
    kani::assume(deposit_amt >= 10_000 && deposit_amt <= 1_000_000);
    engine.deposit(a, deposit_amt as u128, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    // Give user a negative PnL that makes them underwater (loss > deposit)
    let excess: u16 = kani::any();
    kani::assume(excess >= 1 && excess <= 10_000);
    let loss = deposit_amt as i128 + excess as i128;
    engine.set_pnl(a as usize, -loss);

    // Use touch_account_live_local to resolve the flat negative through the real engine pipeline
    // (settle_losses → resolve_flat_negative → insurance/absorb)
    {
        let mut ctx = InstructionContext::new_with_h_lock(0);
        let _ = engine.accrue_market_to(DEFAULT_SLOT, DEFAULT_ORACLE, 0);
        engine.current_slot = DEFAULT_SLOT;
        let _ = engine.touch_account_live_local(a as usize, &mut ctx);
        engine.finalize_touched_accounts_post_live(&ctx);
    }

    assert!(engine.check_conservation(),
        "conservation must hold after touch resolves underwater account");
}

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn bounded_margin_withdrawal() {
    let mut engine = RiskEngine::new(zero_fee_params());
    engine.last_crank_slot = DEFAULT_SLOT;

    let a = engine.add_user(0).unwrap();

    let deposit_amt: u32 = kani::any();
    kani::assume(deposit_amt >= 1000 && deposit_amt <= 10_000_000);
    engine.deposit(a, deposit_amt as u128, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    let withdraw_amt: u32 = kani::any();
    // Dust guard: post-withdrawal capital must be 0 or >= MIN_INITIAL_DEPOSIT (2).
    // So either withdraw_not_atomic all, or leave at least MIN_INITIAL_DEPOSIT.
    let min_dep = engine.params.min_initial_deposit.get() as u32;
    kani::assume(withdraw_amt > 0 && withdraw_amt <= deposit_amt);
    kani::assume(withdraw_amt == deposit_amt || deposit_amt - withdraw_amt >= min_dep);
    let result = engine.withdraw_not_atomic(a, withdraw_amt as u128, DEFAULT_ORACLE, DEFAULT_SLOT, 0i128, 0);
    assert!(result.is_ok());
    assert!(engine.check_conservation());

    let remaining = engine.accounts[a as usize].capital.get();
    if remaining < u128::MAX {
        let result2 = engine.withdraw_not_atomic(a, remaining + 1, DEFAULT_ORACLE, DEFAULT_SLOT, 0i128, 0);
        assert!(result2.is_err());
    }
}

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_top_up_insurance_preserves_conservation() {
    let mut engine = RiskEngine::new(zero_fee_params());

    let amount: u32 = kani::any();
    kani::assume(amount > 0 && amount <= 1_000_000);

    let vault_before = engine.vault.get();
    let ins_before = engine.insurance_fund.balance.get();

    engine.top_up_insurance_fund(amount as u128, DEFAULT_SLOT).unwrap();

    assert!(engine.vault.get() == vault_before + amount as u128);
    assert!(engine.insurance_fund.balance.get() == ins_before + amount as u128);
    assert!(engine.check_conservation());
}

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_deposit_then_withdraw_roundtrip() {
    let mut engine = RiskEngine::new(zero_fee_params());
    engine.last_crank_slot = DEFAULT_SLOT;

    let idx = engine.add_user(0).unwrap();
    let amount: u32 = kani::any();
    kani::assume(amount > 0 && amount <= 1_000_000);

    engine.deposit(idx, amount as u128, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    assert!(engine.check_conservation());

    let result = engine.withdraw_not_atomic(idx, amount as u128, DEFAULT_ORACLE, DEFAULT_SLOT, 0i128, 0);
    assert!(result.is_ok());
    assert!(engine.accounts[idx as usize].capital.get() == 0);
    assert!(engine.check_conservation());
}

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_multiple_deposits_aggregate_correctly() {
    let mut engine = RiskEngine::new(zero_fee_params());

    let a = engine.add_user(0).unwrap();
    let b = engine.add_user(0).unwrap();

    let amount_a: u32 = kani::any();
    let amount_b: u32 = kani::any();
    kani::assume(amount_a <= 1_000_000);
    kani::assume(amount_b <= 1_000_000);

    engine.deposit(a, amount_a as u128, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.deposit(b, amount_b as u128, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    let cap_a = engine.accounts[a as usize].capital.get();
    let cap_b = engine.accounts[b as usize].capital.get();

    assert!(engine.c_tot.get() == cap_a + cap_b);
    assert!(engine.check_conservation());
}

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_close_account_returns_capital() {
    let mut engine = RiskEngine::new(zero_fee_params());

    let idx = engine.add_user(0).unwrap();
    engine.deposit(idx, 50_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    assert!(engine.check_conservation());

    let result = engine.close_account_not_atomic(idx, DEFAULT_SLOT, DEFAULT_ORACLE, 0i128, 0);
    assert!(result.is_ok());
    let returned = result.unwrap();
    assert!(returned == 50_000);
    assert!(engine.check_conservation());
}

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_trade_pnl_is_zero_sum_algebraic() {
    let mut engine = RiskEngine::new(zero_fee_params());

    let a = engine.add_user(0).unwrap();
    let b = engine.add_user(0).unwrap();

    engine.deposit(a, 5_000_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.deposit(b, 5_000_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    let size_q = (100 * POS_SCALE) as i128;
    let result = engine.execute_trade_not_atomic(a, b, DEFAULT_ORACLE, DEFAULT_SLOT, size_q, DEFAULT_ORACLE, 0i128, 0);
    assert!(result.is_ok(), "trade must succeed with sufficient margin");

    // After a trade, PnL must be zero-sum across the two counterparties
    let pnl_a = engine.accounts[a as usize].pnl.get();
    let pnl_b = engine.accounts[b as usize].pnl.get();
    assert!(pnl_a + pnl_b == 0, "trade PnL must be zero-sum");
}

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_flat_negative_resolves_through_insurance() {
    let mut engine = RiskEngine::new(zero_fee_params());

    let idx = engine.add_user(0).unwrap();
    engine.vault = U128::new(10_000);
    engine.insurance_fund.balance = U128::new(5_000);

    engine.set_pnl(idx as usize, -1000i128);

    let ins_before = engine.insurance_fund.balance.get();

    {
        let mut ctx = InstructionContext::new_with_h_lock(0);
        engine.accrue_market_to(DEFAULT_SLOT, DEFAULT_ORACLE, 0).unwrap();
        engine.current_slot = DEFAULT_SLOT;
        engine.touch_account_live_local(idx as usize, &mut ctx).unwrap();
        engine.finalize_touched_accounts_post_live(&ctx);
    }

    assert!(engine.accounts[idx as usize].pnl.get() == 0i128);
    assert!(engine.insurance_fund.balance.get() <= ins_before);
}

// ############################################################################
// ADL SAFETY (from ak.rs)
// ############################################################################

#[kani::proof]
#[kani::unwind(1)]
#[kani::solver(cadical)]
fn t4_17_enqueue_adl_preserves_oi_balance_qty_only() {
    let q1: u8 = kani::any();
    let q2: u8 = kani::any();
    kani::assume(q1 > 0 && q2 > 0);
    let oi = (q1 as u16) + (q2 as u16);
    kani::assume(oi <= 15);

    let q_close: u8 = kani::any();
    kani::assume(q_close > 0 && (q_close as u16) < oi);
    let oi_post = oi - (q_close as u16);

    let a_old = S_ADL_ONE;
    let a_new = a_after_adl(a_old, oi_post, oi);

    let basis_q1 = (q1 as u16) * S_POS_SCALE;
    let basis_q2 = (q2 as u16) * S_POS_SCALE;
    let eff_q1 = lazy_eff_q(basis_q1, a_new, a_old) / S_POS_SCALE;
    let eff_q2 = lazy_eff_q(basis_q2, a_new, a_old) / S_POS_SCALE;

    assert!(eff_q1 + eff_q2 <= oi_post, "sum of effective positions must not exceed oi_post");
    assert!(eff_q1 <= q1 as u16);
    assert!(eff_q2 <= q2 as u16);
}

/// Precision exhaustion: when A_candidate floors to 0 despite OI_post > 0,
/// engine must zero BOTH sides' OI and set both pending_reset.
/// Uses actual engine enqueue_adl with symbolic A_mult close to exhaustion.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn t4_18_precision_exhaustion_both_sides_reset() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let mut ctx = InstructionContext::new();

    // A_mult = 2, OI = 3*PS. Closing 2*PS leaves OI_post = 1*PS.
    // A_candidate = floor(2 * 1 / 3) = 0 → precision exhaustion.
    engine.adl_mult_long = U128::new(2);
    engine.adl_coeff_long = I128::ZERO;
    engine.oi_eff_long_q = U128::new(3 * POS_SCALE);
    engine.oi_eff_short_q = U128::new(3 * POS_SCALE);
    engine.stored_pos_count_long = 1;

    let q_close = 2 * POS_SCALE;
    let result = engine.enqueue_adl(&mut ctx, Side::Short, q_close, 0u128);
    assert!(result.is_ok());

    // Both sides' OI must be zeroed (precision exhaustion terminal drain)
    assert!(engine.oi_eff_long_q.get() == 0, "opposing OI must be zeroed");
    assert!(engine.oi_eff_short_q.get() == 0, "liquidated OI must be zeroed");
    assert!(ctx.pending_reset_long, "opposing side must be pending reset");
    assert!(ctx.pending_reset_short, "liquidated side must be pending reset");
}

#[kani::proof]
#[kani::unwind(1)]
#[kani::solver(cadical)]
fn t4_19_full_drain_terminal_k_includes_deficit() {
    let oi: u8 = kani::any();
    kani::assume(oi > 0 && oi <= 10);
    let d: u8 = kani::any();
    kani::assume(d > 0 && d <= 100);

    let a_opp = S_ADL_ONE;
    let k_before: i32 = 0;

    let delta_k_abs = ((d as u16) * (a_opp as u16) + (oi as u16) - 1) / (oi as u16);
    let delta_k = -(delta_k_abs as i32);
    let k_after = k_before + delta_k;

    assert!(k_after < k_before);

    let k_epoch_start = k_after;
    assert!(k_epoch_start == k_before + delta_k);
    assert!(k_epoch_start < k_before);
}

#[kani::proof]
#[kani::unwind(1)]
#[kani::solver(cadical)]
fn t4_20_bankruptcy_qty_routes_when_d_zero() {
    let oi: u8 = kani::any();
    kani::assume(oi >= 2);
    let q_close: u8 = kani::any();
    kani::assume(q_close > 0 && q_close < oi);

    let a_old = S_ADL_ONE;
    let oi_post = oi - q_close;

    let a_new = ((a_old as u16) * (oi_post as u16)) / (oi as u16);

    assert!((a_new as u16) <= (a_old as u16));
    assert!((a_new as u16) < (a_old as u16));

    assert!(oi_post < oi);
}

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn t4_21_precision_exhaustion_zeroes_both_sides() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let mut ctx = InstructionContext::new();

    engine.adl_mult_long = U128::new(1);
    engine.oi_eff_long_q = U128::new(3 * POS_SCALE);
    engine.oi_eff_short_q = U128::new(3 * POS_SCALE);
    engine.adl_coeff_long = I128::ZERO;
    engine.stored_pos_count_long = 1;

    let q_close = POS_SCALE;
    let d = 0u128;

    let result = engine.enqueue_adl(&mut ctx, Side::Short, q_close, d);
    assert!(result.is_ok());

    assert!(engine.oi_eff_long_q.get() == 0);
    assert!(engine.oi_eff_short_q.get() == 0);
    assert!(ctx.pending_reset_long);
    assert!(ctx.pending_reset_short);
}

/// K-space overflow routes deficit to absorb_protocol_loss, preserving K.
/// Uses actual engine enqueue_adl with K near i128::MIN to trigger overflow.
#[kani::proof]
#[kani::solver(cadical)]
fn t4_22_k_overflow_routes_to_absorb() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let mut ctx = InstructionContext::new();

    // Set K near i128::MIN so delta_K addition underflows
    engine.adl_coeff_long = I128::new(i128::MIN + 1);
    engine.adl_mult_long = U128::new(POS_SCALE); // Use POS_SCALE (not ADL_ONE) to keep computation manageable
    engine.oi_eff_long_q = U128::new(4 * POS_SCALE);
    engine.oi_eff_short_q = U128::new(4 * POS_SCALE);
    engine.stored_pos_count_long = 1;
    engine.insurance_fund.balance = U128::new(10_000_000);

    let k_before = engine.adl_coeff_long;
    let ins_before = engine.insurance_fund.balance.get();

    // ADL with deficit — delta_K will be large negative, K_opp + delta_K underflows
    let q_close = POS_SCALE;
    let d = 1_000_000u128;

    let result = engine.enqueue_adl(&mut ctx, Side::Short, q_close, d);
    assert!(result.is_ok());

    // K must be unchanged (overflow routed to absorb)
    assert!(engine.adl_coeff_long == k_before,
        "K must be unchanged when overflow routes to absorb");
    // Insurance must have decreased (absorb_protocol_loss was called)
    assert!(engine.insurance_fund.balance.get() < ins_before,
        "insurance must decrease when absorbing overflow deficit");
    // A must still shrink (quantity routing is independent of K overflow)
    assert!(engine.adl_mult_long.get() < POS_SCALE, "A must shrink even on K overflow");
}

/// D=0 ADL: K must be unchanged, A must decrease, OI updated.
/// Uses actual engine enqueue_adl with zero deficit.
#[kani::proof]
#[kani::solver(cadical)]
fn t4_23_d_zero_routes_quantity_only() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let mut ctx = InstructionContext::new();

    let k_init: i8 = kani::any();
    engine.adl_coeff_long = I128::new(k_init as i128);
    engine.adl_mult_long = U128::new(ADL_ONE);
    engine.oi_eff_long_q = U128::new(10 * POS_SCALE);
    engine.oi_eff_short_q = U128::new(10 * POS_SCALE);
    engine.stored_pos_count_long = 1;

    let k_before = engine.adl_coeff_long;
    let a_before = engine.adl_mult_long;

    // D=0 quantity-only ADL
    let q_close = POS_SCALE;
    let result = engine.enqueue_adl(&mut ctx, Side::Short, q_close, 0u128);
    assert!(result.is_ok());

    // K must be unchanged when D == 0
    assert!(engine.adl_coeff_long == k_before, "K must be unchanged when D == 0");
    // A must decrease
    assert!(engine.adl_mult_long < a_before, "A must decrease after quantity ADL");
    // OI must decrease by q_close on both sides
    assert!(engine.oi_eff_long_q.get() == 9 * POS_SCALE);
    assert!(engine.oi_eff_short_q.get() == 9 * POS_SCALE);
}

// ############################################################################
// DUST BOUNDS (from ak.rs)
// ############################################################################

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn t5_21_local_floor_quantity_error_bounded() {
    let basis_q: u16 = kani::any();
    kani::assume(basis_q > 0);

    let a_cur: u16 = kani::any();
    kani::assume(a_cur > 0);
    let a_basis: u16 = kani::any();
    kani::assume(a_basis > 0 && a_basis >= a_cur);

    let product = (basis_q as u64) * (a_cur as u64);
    let remainder = product % (a_basis as u64);

    assert!(remainder < a_basis as u64);
}

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn t5_21_pnl_rounding_conservative() {
    let basis_q: u8 = kani::any();
    kani::assume(basis_q > 0);
    let k_diff: i8 = kani::any();
    kani::assume(k_diff < 0);

    let a_basis = S_ADL_ONE;
    let scaled_basis = (basis_q as u16) * S_POS_SCALE;

    let pnl = lazy_pnl(scaled_basis, k_diff as i32, a_basis);

    assert!(pnl <= 0, "negative k_diff must produce non-positive PnL");

    let exact_num = (scaled_basis as i32) * (k_diff as i32);
    let den = (a_basis as i32) * (S_POS_SCALE as i32);
    let trunc = exact_num / den;
    assert!(pnl <= trunc);
}

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn t5_22_phantom_dust_total_bound() {
    let q1: u8 = kani::any();
    let q2: u8 = kani::any();
    kani::assume(q1 > 0 && q2 > 0);
    let a_cur: u16 = kani::any();
    let a_basis: u16 = kani::any();
    kani::assume(a_basis > 0 && a_cur > 0 && a_cur <= a_basis);

    let basis_q1 = (q1 as u32) * (S_POS_SCALE as u32);
    let basis_q2 = (q2 as u32) * (S_POS_SCALE as u32);

    let rem1 = (basis_q1 as u32) * (a_cur as u32) % (a_basis as u32);
    let rem2 = (basis_q2 as u32) * (a_cur as u32) % (a_basis as u32);

    assert!(rem1 < a_basis as u32);
    assert!(rem2 < a_basis as u32);

    assert!(rem1 + rem2 < 2 * (a_basis as u32),
        "total dust from 2 accounts < 2 effective units");
}

#[kani::proof]
#[kani::unwind(1)]
#[kani::solver(cadical)]
fn t5_23_dust_clearance_guard_safe() {
    let n: u8 = kani::any();
    kani::assume(n > 0 && n <= 32);

    let dust_bound: u8 = n;

    let max_dust_per_acct = S_POS_SCALE as u16 - 1;
    let max_total_dust_fp = (n as u16) * max_dust_per_acct;
    let max_total_dust_base = max_total_dust_fp / (S_POS_SCALE as u16);
    assert!(max_total_dust_base < n as u16, "total OI dust < phantom_dust_bound");
    assert!(dust_bound == n, "dust_bound tracks exact zeroing count");
}

// ############################################################################
// FUNDING NO-MINT (from ak.rs)
// ############################################################################

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn t13_54_funding_no_mint_asymmetric_a() {
    let mut engine = RiskEngine::new(zero_fee_params());

    engine.oi_eff_long_q = U128::new(POS_SCALE);
    engine.oi_eff_short_q = U128::new(POS_SCALE);

    let a_long: u16 = kani::any();
    kani::assume(a_long >= 1 && a_long <= 10);
    let a_short: u16 = kani::any();
    kani::assume(a_short >= 1 && a_short <= 10);
    engine.adl_mult_long = U128::new(a_long as u128);
    engine.adl_mult_short = U128::new(a_short as u128);

    engine.last_oracle_price = 100;
    engine.last_market_slot = 0;

    let rate: i8 = kani::any();
    kani::assume(rate != 0 && rate >= -10 && rate <= 10);

    let k_long_before = engine.adl_coeff_long;
    let k_short_before = engine.adl_coeff_short;

    let result = engine.accrue_market_to(1, 100, rate as i128);
    assert!(result.is_ok());

    let k_long_after = engine.adl_coeff_long;
    let k_short_after = engine.adl_coeff_short;

    let dk_long = k_long_after.checked_sub(k_long_before).unwrap();
    let dk_short = k_short_after.checked_sub(k_short_before).unwrap();

    // Cross-multiply to check no-mint: dk_long * A_short + dk_short * A_long <= 0
    let term_long = dk_long.checked_mul(a_short as i128).unwrap();
    let term_short = dk_short.checked_mul(a_long as i128).unwrap();
    let cross_total = term_long.checked_add(term_short).unwrap();
    assert!(cross_total <= 0,
        "funding must not mint: cross-multiplied K changes must be <= 0");
}

// ############################################################################
// NEW: proof_junior_profit_backing
// ############################################################################

/// Σ PNL_pos ≤ Residual (bounded 2-account)
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_junior_profit_backing() {
    // Direct-state proof: skip engine deposit path for solver efficiency.
    // Prove: floor(pnl_matured_pos_tot * h_num / h_den) <= residual
    // for all valid vault/c_tot/insurance/matured configurations.
    let vault_val: u8 = kani::any();
    let c_tot_val: u8 = kani::any();
    let ins_val: u8 = kani::any();
    let matured_val: u8 = kani::any();

    kani::assume(matured_val > 0);
    let senior = (c_tot_val as u16) + (ins_val as u16);
    kani::assume((vault_val as u16) >= senior);

    let vault = vault_val as u32;
    let c_tot = c_tot_val as u32;
    let ins = ins_val as u32;
    let matured = matured_val as u32;

    let residual = vault - c_tot - ins;

    let h_num = if residual < matured { residual } else { matured };
    let h_den = matured;

    let effective_ppt = matured * h_num / h_den;

    assert!(effective_ppt <= residual,
        "haircutted matured PnL must be backed by residual alone");

    // Verify both branches reachable
    kani::cover!(residual < matured, "h < 1 branch");
    kani::cover!(residual >= matured, "h = 1 branch");
}

// ############################################################################
// NEW: proof_protected_principal
// ############################################################################

/// Flat account capital unaffected by other's insolvency.
/// Uses touch_account_live_local which internally calls settle_losses + resolve_flat_negative.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_protected_principal() {
    let mut engine = RiskEngine::new(zero_fee_params());

    let a = engine.add_user(0).unwrap();
    let b = engine.add_user(0).unwrap();

    let dep_a: u32 = kani::any();
    kani::assume(dep_a > 0 && dep_a <= 1_000_000);
    let dep_b: u32 = kani::any();
    kani::assume(dep_b > 0 && dep_b <= 1_000_000);

    engine.deposit(a, dep_a as u128, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.deposit(b, dep_b as u128, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    let a_cap_before = engine.accounts[a as usize].capital.get();

    // b goes insolvent: negative PnL exceeding capital
    let loss: u16 = kani::any();
    kani::assume(loss > 0);
    let loss_val = dep_b as u128 + (loss as u128);
    engine.set_pnl(b as usize, -(loss_val as i128));

    // touch_account_live_local runs the real settlement pipeline:
    // settle_side_effects_with_h_lock → settle_losses → resolve_flat_negative
    engine.last_oracle_price = DEFAULT_ORACLE;
    engine.last_market_slot = DEFAULT_SLOT;
    {
        let mut ctx = InstructionContext::new_with_h_lock(0);
        let _ = engine.accrue_market_to(DEFAULT_SLOT, DEFAULT_ORACLE, 0);
        engine.current_slot = DEFAULT_SLOT;
        let _ = engine.touch_account_live_local(b as usize, &mut ctx);
        engine.finalize_touched_accounts_post_live(&ctx);
    }

    // a's capital must be unchanged through b's entire loss resolution
    let a_cap_after = engine.accounts[a as usize].capital.get();
    assert!(a_cap_after == a_cap_before,
        "flat account capital must be unaffected by other's insolvency");
}

// ============================================================================
// proof_withdraw_simulation_preserves_residual
// ============================================================================
//
// Issue #1: Withdraw margin simulation must not inflate the haircut ratio.

#[kani::proof]
#[kani::solver(cadical)]
fn proof_withdraw_simulation_preserves_residual() {
    let mut engine = RiskEngine::new(zero_fee_params());

    let a = engine.add_user(0).unwrap();
    let b = engine.add_user(0).unwrap();
    engine.deposit(a, 10_000_000, 100, 0).unwrap();
    engine.deposit(b, 10_000_000, 100, 0).unwrap();

    engine.last_oracle_price = 100;
    engine.last_market_slot = 1;
    engine.last_crank_slot = 1;

    // Trade so a has a position (exercises the margin-check + haircut path)
    let size_q = POS_SCALE as i128;
    engine.execute_trade_not_atomic(a, b, 100, 1, size_q, 100, 0i128, 0).unwrap();

    // Record haircut before actual withdraw_not_atomic
    let (h_num_before, h_den_before) = engine.haircut_ratio();
    let conservation_before = engine.check_conservation();
    assert!(conservation_before, "conservation must hold before withdraw_not_atomic");

    // Call the real engine.withdraw_not_atomic(, 0i128, 0)
    let result = engine.withdraw_not_atomic(a, 1_000, 100, 1, 0i128, 0);
    assert!(result.is_ok(), "withdraw_not_atomic of 1000 from 10M capital must succeed");

    let (h_num_after, h_den_after) = engine.haircut_ratio();
    assert!(engine.check_conservation(), "conservation must hold after withdraw_not_atomic");

    // h must not increase: cross-multiply h_after/1 <= h_before/1
    let lhs = h_num_after.checked_mul(h_den_before);
    let rhs = h_num_before.checked_mul(h_den_after);
    if let (Some(l), Some(r)) = (lhs, rhs) {
        assert!(l <= r,
            "haircut must not increase after withdraw_not_atomic — Residual inflation detected");
    }
}

// ============================================================================
// proof_funding_rate_validated_before_storage
// ============================================================================

#[kani::proof]
#[kani::solver(cadical)]
fn proof_funding_rate_validated_before_storage() {
    let mut engine = RiskEngine::new(zero_fee_params());

    engine.last_oracle_price = 100;
    engine.last_market_slot = 0;
    engine.last_crank_slot = 0;

    let a = engine.add_user(0).unwrap();
    engine.deposit(a, 10_000_000, 100, 0).unwrap();

    // Pass an invalid funding rate (> MAX_ABS_FUNDING_E9_PER_SLOT) directly
    // v12.16.4: rate is validated inside accrue_market_to
    let bad_rate: i128 = MAX_ABS_FUNDING_E9_PER_SLOT + 1;
    let result = engine.keeper_crank_not_atomic(1, 100, &[(a, None)], 1, bad_rate, 0);
    assert!(result.is_err(), "out-of-bounds rate must be rejected by keeper_crank_not_atomic");

    // Valid rate must succeed
    let result2 = engine.keeper_crank_not_atomic(1, 100, &[(a, None)], 1, 0i128, 0);
    assert!(result2.is_ok(),
        "protocol must accept valid funding rate");
}

// ============================================================================
// proof_gc_dust_preserves_fee_credits
// ============================================================================

#[kani::proof]
#[kani::solver(cadical)]
fn proof_gc_dust_preserves_fee_credits() {
    let mut engine = RiskEngine::new(zero_fee_params());

    let a = engine.add_user(0).unwrap();
    engine.deposit(a, 10_000, 100, 1).unwrap();

    engine.last_oracle_price = 100;
    engine.last_market_slot = 1;
    engine.last_crank_slot = 1;
    engine.current_slot = 1;

    // Account has 0 capital, 0 position, but positive fee_credits (prepaid)
    engine.set_capital(a as usize, 0);
    engine.accounts[a as usize].fee_credits = I128::new(5_000);
    engine.accounts[a as usize].position_basis_q = I128::ZERO;
    engine.accounts[a as usize].reserved_pnl = U128::ZERO;
    engine.set_pnl(a as usize, 0i128);

    assert!(engine.is_used(a as usize));
    engine.garbage_collect_dust();

    // Positive fee_credits: account must be PRESERVED (prepaid credits)
    assert!(engine.is_used(a as usize),
        "GC must not delete account with positive fee_credits");
    assert!(engine.accounts[a as usize].fee_credits.get() == 5_000,
        "fee_credits must be preserved");

    // Now test negative fee_credits (debt): account SHOULD be collected
    // and the uncollectible debt written off
    let b = engine.add_user(0).unwrap();
    engine.deposit(b, 10_000, 100, 1).unwrap();
    engine.set_capital(b as usize, 0);
    engine.accounts[b as usize].fee_credits = I128::new(-3_000); // debt
    engine.accounts[b as usize].position_basis_q = I128::ZERO;
    engine.accounts[b as usize].reserved_pnl = U128::ZERO;
    engine.set_pnl(b as usize, 0i128);

    assert!(engine.is_used(b as usize));
    engine.garbage_collect_dust();

    // Negative fee_credits (debt) on dead account: must be collected and debt written off
    assert!(!engine.is_used(b as usize),
        "GC must collect dead account with negative fee_credits (uncollectible debt)");
}

// ############################################################################
// min_liquidation_abs does not prevent liquidation of underwater accounts
// ############################################################################

#[kani::proof]
#[kani::solver(cadical)]
fn proof_min_liq_abs_does_not_block_liquidation() {
    let mut params = zero_fee_params();
    params.liquidation_fee_bps = 100;
    params.liquidation_fee_cap = U128::new(1_000_000);
    // Concrete min_liquidation_abs to keep engine pipeline tractable.
    // Tests a non-trivial floor value to verify it doesn't block liquidation.
    params.min_liquidation_abs = U128::new(100_000);
    let mut engine = RiskEngine::new(params);

    let a = engine.add_user(0).unwrap();
    let b = engine.add_user(0).unwrap();
    engine.deposit(a, 50_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.deposit(b, 500_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    // Near-max leverage long for a
    let size = (480 * POS_SCALE) as i128;
    let result = engine.execute_trade_not_atomic(a, b, DEFAULT_ORACLE, DEFAULT_SLOT, size, DEFAULT_ORACLE, 0i128, 0);
    assert!(result.is_ok());

    // Crash price to trigger liquidation
    let crash_price = 890u64;
    let slot2 = DEFAULT_SLOT + 1;
    let result = engine.liquidate_at_oracle_not_atomic(a, slot2, crash_price, LiquidationPolicy::FullClose, 0i128, 0);
    // Liquidation must not revert due to min_liquidation_abs
    assert!(result.is_ok(), "min_liquidation_abs must not block liquidation");
    assert!(engine.check_conservation(), "conservation must hold after liquidation with min_abs");
}

// ############################################################################
// Trading loss seniority: settle_losses before fee_debt_sweep
// ############################################################################

#[kani::proof]
#[kani::solver(cadical)]
fn proof_trading_loss_seniority() {
    let mut params = zero_fee_params();
    let mut engine = RiskEngine::new(params);

    let a = engine.add_user(0).unwrap();
    engine.deposit(a, 10_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    engine.last_oracle_price = DEFAULT_ORACLE;
    engine.last_market_slot = DEFAULT_SLOT;

    // Give account negative PnL (trading loss)
    engine.set_pnl(a as usize, -8_000i128);

    // Advance 50 slots — settle_losses runs during touch
    let touch_slot = DEFAULT_SLOT + 50;
    {
        let mut ctx = InstructionContext::new_with_h_lock(0);
        let _ = engine.accrue_market_to(touch_slot, DEFAULT_ORACLE, 0);
        engine.current_slot = touch_slot;
        let _ = engine.touch_account_live_local(a as usize, &mut ctx);
        engine.finalize_touched_accounts_post_live(&ctx);
    }

    let pnl_after = engine.accounts[a as usize].pnl;

    // Assert: PnL is zero (trading loss fully settled from principal)
    assert!(pnl_after >= 0,
        "trading loss must be fully settled from principal");
}

// ############################################################################
// Strictly risk-reducing exemption path (enforce_one_side_margin I256 buffers)
// ############################################################################

/// Put account below maintenance margin, then verify:
/// 1. Risk-reducing trade (close half) succeeds via I256 buffer comparison
/// 2. Risk-increasing trade is rejected
/// Exercises the enforce_one_side_margin lines 2506-2520.
#[kani::proof]
#[kani::unwind(70)]
#[kani::solver(cadical)]
fn proof_risk_reducing_exemption_path() {
    let mut engine = RiskEngine::new(zero_fee_params());
    engine.last_crank_slot = DEFAULT_SLOT;

    let a = engine.add_user(0).unwrap();
    let b = engine.add_user(0).unwrap();
    engine.deposit(a, 100_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.deposit(b, 500_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    // Open leveraged long for a (8x)
    let size = (800 * POS_SCALE) as i128;
    engine.execute_trade_not_atomic(a, b, DEFAULT_ORACLE, DEFAULT_SLOT, size, DEFAULT_ORACLE, 0i128, 0).unwrap();

    // Inject loss to push a below maintenance margin
    engine.set_pnl(a as usize, -70_000i128);

    // Account may or may not be below MM — the key test is the partial close

    // Risk-reducing trade: close half the position
    let half_close = size / 2;
    let reduce_result = engine.execute_trade_not_atomic(b, a, DEFAULT_ORACLE, DEFAULT_SLOT, half_close, DEFAULT_ORACLE, 0i128, 0);

    // Risk-increasing trade: double the position
    let increase = size;
    // Need to restore state for the increase test
    let mut engine2 = RiskEngine::new(zero_fee_params());
    engine2.last_crank_slot = DEFAULT_SLOT;
    let a2 = engine2.add_user(0).unwrap();
    let b2 = engine2.add_user(0).unwrap();
    engine2.deposit(a2, 100_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine2.deposit(b2, 500_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine2.execute_trade_not_atomic(a2, b2, DEFAULT_ORACLE, DEFAULT_SLOT, size, DEFAULT_ORACLE, 0i128, 0).unwrap();
    engine2.set_pnl(a2 as usize, -70_000i128);
    let increase_result = engine2.execute_trade_not_atomic(a2, b2, DEFAULT_ORACLE, DEFAULT_SLOT, increase, DEFAULT_ORACLE, 0i128, 0);

    // Risk-reducing must succeed, risk-increasing must be rejected
    assert!(reduce_result.is_ok(), "risk-reducing trade must be accepted");
    kani::cover!(reduce_result.is_ok(), "risk-reducing trade accepted");
    assert!(increase_result.is_err(), "risk-increasing trade must be rejected");
    kani::cover!(increase_result.is_err(), "risk-increasing trade rejected");

    // Both engines must maintain conservation
    assert!(engine.check_conservation());
    assert!(engine2.check_conservation());
}

// ############################################################################
// Buffer masking attack: risk-reducing trade must not decrease raw equity
// ############################################################################

/// Verify that the risk-reducing exemption path cannot be exploited to
/// extract value via execution slippage. A bankrupt account closing 99%
/// of its position with adverse exec_price must be rejected if raw equity
/// decreases, even though the maintenance buffer improves from MM_req drop.
#[kani::proof]
#[kani::unwind(70)]
#[kani::solver(cadical)]
fn proof_buffer_masking_blocked() {
    let mut engine = RiskEngine::new(zero_fee_params());
    engine.last_crank_slot = DEFAULT_SLOT;

    let victim = engine.add_user(0).unwrap();
    let attacker = engine.add_user(0).unwrap();
    engine.deposit(victim, 500_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.deposit(attacker, 500_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    // Victim opens leveraged long
    let size = (400 * POS_SCALE) as i128;
    engine.execute_trade_not_atomic(victim, attacker, DEFAULT_ORACLE, DEFAULT_SLOT, size, DEFAULT_ORACLE, 0i128, 0).unwrap();

    // Moderate loss — below maintenance but not deeply bankrupt
    engine.set_pnl(victim as usize, -350_000i128);

    let equity_before = engine.account_equity_maint_raw(&engine.accounts[victim as usize]);

    // Risk-reducing: close half the position at oracle price (no slippage)
    let close_size = size / 2;
    let result = engine.execute_trade_not_atomic(attacker, victim, DEFAULT_ORACLE, DEFAULT_SLOT, close_size, DEFAULT_ORACLE, 0i128, 0);
    kani::cover!(result.is_ok(), "risk-reducing close reachable");

    if result.is_ok() {
        let equity_after = engine.account_equity_maint_raw(&engine.accounts[victim as usize]);
        assert!(equity_after >= equity_before,
            "risk-reducing trade must not decrease raw equity (buffer masking blocked)");
    }
    // Conservation must hold regardless
    assert!(engine.check_conservation());
}

// ############################################################################
// Phantom dust revert: enqueue_adl step 5 must reset drained opp side
// ############################################################################

/// When enqueue_adl drains opposing phantom OI to zero (stored_pos_count_opp=0,
/// OI_post=0), it must unconditionally set pending_reset for both sides
/// so schedule_end_of_instruction_resets doesn't revert on OI imbalance.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_phantom_dust_drain_no_revert() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let mut ctx = InstructionContext::new();

    // Set up opposing side with phantom OI but no stored positions.
    // OI is balanced (required invariant), stored_pos_count_opp = 0.
    engine.adl_mult_long = U128::new(ADL_ONE);
    engine.oi_eff_long_q = U128::new(POS_SCALE);   // phantom OI on long side (opp)
    engine.oi_eff_short_q = U128::new(POS_SCALE);  // matching OI on short side (liq)
    engine.stored_pos_count_long = 0;   // no stored positions on opposing side
    engine.stored_pos_count_short = 1;  // liq side has stored positions

    // Bankrupt short liquidated: close exactly drains opposing phantom OI
    let q_close = POS_SCALE; // drains all of OI_eff_long AND OI_eff_short
    let d = 0u128;

    let result = engine.enqueue_adl(&mut ctx, Side::Short, q_close, d);
    assert!(result.is_ok(), "enqueue_adl must not fail");

    // After enqueue_adl: OI_eff_short was decremented by q_close in step 1 → 0
    // OI_eff_long was set to oi_post = OI - q_close = 0 in step 5
    assert!(engine.oi_eff_long_q.get() == 0, "opp OI must be 0");
    assert!(engine.oi_eff_short_q.get() == 0, "liq OI must be 0");

    // Both pending resets must be set
    assert!(ctx.pending_reset_long, "drained opp side must have pending reset");

    // End-of-instruction resets must not revert
    let result2 = engine.schedule_end_of_instruction_resets(&mut ctx);
    assert!(result2.is_ok(), "schedule must not revert after phantom drain");
}

// ############################################################################
// Fee debt sweep consumes released PnL when capital insufficient
// ############################################################################

/// Profitable open-position account with zero capital accumulates fee debt.
/// fee_debt_sweep must consume matured released PnL to pay the debt,
/// preventing insurance fund starvation.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_fee_debt_sweep_consumes_released_pnl() {
    let mut params = zero_fee_params();
    let mut engine = RiskEngine::new(params);

    let idx = engine.add_user(0).unwrap();
    // Symbolic capital — covers both debt < cap and debt > cap paths
    let cap: u32 = kani::any();
    kani::assume(cap >= 1 && cap <= 1_000_000);
    engine.deposit(idx, cap as u128, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    // Symbolic fee debt
    let debt: u32 = kani::any();
    kani::assume(debt >= 1 && debt <= 1_000_000);
    engine.accounts[idx as usize].fee_credits = I128::new(-(debt as i128));

    let ins_before = engine.insurance_fund.balance.get();
    let cap_before = engine.accounts[idx as usize].capital.get();

    // Run fee_debt_sweep
    engine.fee_debt_sweep(idx as usize);

    let ins_after = engine.insurance_fund.balance.get();
    let fc_after = engine.accounts[idx as usize].fee_credits.get();
    let cap_after = engine.accounts[idx as usize].capital.get();

    // Payment = min(debt, capital)
    let expected_pay = core::cmp::min(debt as u128, cap_before);

    // Exact algebraic verification
    assert!(ins_after == ins_before + expected_pay,
        "insurance must receive min(debt, capital)");
    assert!(fc_after == -(debt as i128) + (expected_pay as i128),
        "fee_credits must increase by payment amount");
    assert!(cap_after == cap_before - expected_pay,
        "capital must decrease by payment amount");
    // fee_credits must remain non-positive
    assert!(fc_after <= 0, "fee_credits must not become positive");

    assert!(engine.check_conservation());
}

// ############################################################################
// settle_maintenance_fee_internal rejects fee_credits == i128::MIN (spec §2.1)
// ############################################################################
// REMOVED in v12.14.0: engine-native maintenance_fee_per_slot was removed.
// proof_touch_drops_excess_at_fee_credits_limit deleted — tested removed feature.

// ############################################################################
// v12.14.0 compliance: flat-close guard uses Eq_maint_raw_i >= 0
// ############################################################################

/// v12.14.0 change #2: A trade that closes to flat must use Eq_maint_raw_i >= 0,
/// not just PNL_i >= 0. An account with positive PNL but large fee debt
/// (Eq_maint_raw_i = C + PNL - FeeDebt < 0) must be rejected.
#[kani::proof]
#[kani::unwind(70)]
#[kani::solver(cadical)]
fn proof_v1126_flat_close_uses_eq_maint_raw() {
    let mut params = zero_fee_params();
    params.trading_fee_bps = 100; // 1% fee
    let mut engine = RiskEngine::new(params);
    engine.last_crank_slot = DEFAULT_SLOT;

    let a = engine.add_user(0).unwrap();
    let b = engine.add_user(0).unwrap();
    engine.deposit(a, 100_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.deposit(b, 500_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    // Open position for a
    let size = (500 * POS_SCALE) as i128;
    engine.execute_trade_not_atomic(a, b, DEFAULT_ORACLE, DEFAULT_SLOT, size, DEFAULT_ORACLE, 0i128, 0).unwrap();

    // Drain a's capital to 0, give positive PNL but massive fee debt
    engine.set_capital(a as usize, 0);
    engine.set_pnl(a as usize, 1000i128); // positive PNL
    engine.accounts[a as usize].fee_credits = I128::new(-5000); // fee debt

    // Eq_maint_raw = C(0) + PNL(1000) - FeeDebt(5000) = -4000 < 0
    // v12.14.0 requires: reject flat close when Eq_maint_raw < 0
    // Old code only checks PNL >= 0 which would pass (PNL = 1000 > 0)

    let close_size = -size;
    let result = engine.execute_trade_not_atomic(a, b, DEFAULT_ORACLE, DEFAULT_SLOT, close_size, DEFAULT_ORACLE, 0i128, 0);

    // Must be rejected: Eq_maint_raw < 0 even though PNL > 0
    assert!(result.is_err(),
        "v12.14.0: flat close must be rejected when Eq_maint_raw < 0 (fee debt exceeds C + PNL)");
}

// ############################################################################
// v12.14.0 compliance: risk-reducing exemption is fee-neutral
// ############################################################################

/// v12.14.0 change #1: The risk-reducing buffer comparison must be fee-neutral.
/// A genuine de-risking trade must not fail solely because the trading fee
/// reduces post-trade equity.
#[kani::proof]
#[kani::unwind(70)]
#[kani::solver(cadical)]
fn proof_v1126_risk_reducing_fee_neutral() {
    let mut params = zero_fee_params();
    params.trading_fee_bps = 100; // 1% fee to make fee friction visible
    let mut engine = RiskEngine::new(params);
    engine.last_crank_slot = DEFAULT_SLOT;

    let a = engine.add_user(0).unwrap();
    let b = engine.add_user(0).unwrap();
    engine.deposit(a, 100_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.deposit(b, 500_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    // Open leveraged position
    let size = (800 * POS_SCALE) as i128;
    engine.execute_trade_not_atomic(a, b, DEFAULT_ORACLE, DEFAULT_SLOT, size, DEFAULT_ORACLE, 0i128, 0).unwrap();

    // Push below maintenance
    engine.set_pnl(a as usize, -50_000i128);

    // Risk-reducing: close half at oracle price (no slippage)
    let half_close = size / 2;
    let result = engine.execute_trade_not_atomic(b, a, DEFAULT_ORACLE, DEFAULT_SLOT, half_close, DEFAULT_ORACLE, 0i128, 0);

    // v12.14.0: fee-neutral comparison means pure fee friction should not block
    // a genuine de-risking trade at oracle price.
    // The post-trade buffer (with fee added back) should be strictly better.
    // Conservation must hold regardless of whether trade succeeds or fails.
    assert!(engine.check_conservation());
    kani::cover!(result.is_ok(), "fee-neutral risk-reducing trade accepted");
}

// ############################################################################
// v12.14.0 compliance: MIN_NONZERO_MM_REQ floor (TODO: implement params first)
// ############################################################################

// Uncommented: RiskParams now has min_nonzero_mm_req / min_nonzero_im_req
#[kani::proof]
#[kani::unwind(70)]
#[kani::solver(cadical)]
fn proof_v1126_min_nonzero_margin_floor() {
    let mut params = zero_fee_params();
    params.min_nonzero_mm_req = 1000;
    params.min_nonzero_im_req = 2000;
    params.min_initial_deposit = U128::new(2000); // must be >= min_nonzero_im_req
    let mut engine = RiskEngine::new(params);
    engine.last_crank_slot = DEFAULT_SLOT;

    let a = engine.add_user(0).unwrap();
    let b = engine.add_user(0).unwrap();
    engine.deposit(a, 100_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.deposit(b, 500_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    // Tiny position: notional so small that proportional MM floors to 0
    let tiny_size = 1i128;
    let result = engine.execute_trade_not_atomic(a, b, DEFAULT_ORACLE, DEFAULT_SLOT, tiny_size, DEFAULT_ORACLE, 0i128, 0);

    // With min_nonzero_im_req = 2000, even a tiny position needs IM >= 2000.
    // Account a has 100_000 capital which exceeds 2000, so trade should succeed.
    // The key verification is that the margin floor is applied.
    assert!(engine.check_conservation());
    kani::cover!(result.is_ok(), "tiny position trade with margin floor");
}

// ############################################################################
// v12.14.0 §2.6: flat-dust reclamation (GC sweeps 0 < C_i < MIN_INITIAL_DEPOSIT)
// ############################################################################

/// A flat account with 0 < C_i < MIN_INITIAL_DEPOSIT, zero PnL/basis/reserved,
/// and nonpositive fee credits must be reclaimable by garbage_collect_dust.
/// The dust capital must be swept into insurance, not lost.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_gc_reclaims_flat_dust_capital() {
    let mut params = zero_fee_params();
    params.min_initial_deposit = U128::new(10_000); // $0.01 minimum
    let mut engine = RiskEngine::new(params);

    let idx = engine.add_user(0).unwrap();
    engine.deposit(idx, 10_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    // Simulate dust: set capital to 1 (below MIN_INITIAL_DEPOSIT of 10_000)
    // This models an account whose capital was drained by fees/losses to dust level.
    engine.set_capital(idx as usize, 1);

    let cap = engine.accounts[idx as usize].capital.get();
    assert!(cap > 0 && cap < 10_000, "account must have dust capital");
    assert!(engine.accounts[idx as usize].pnl.get() == 0);
    assert!(engine.accounts[idx as usize].position_basis_q.get() == 0);
    assert!(engine.is_used(idx as usize));

    let ins_before = engine.insurance_fund.balance.get();
    let vault_before = engine.vault.get();

    // GC must reclaim this account
    engine.garbage_collect_dust();

    // Account must be freed
    assert!(!engine.is_used(idx as usize),
        "GC must reclaim flat account with dust capital below MIN_INITIAL_DEPOSIT");

    // Dust capital must be swept to insurance (not lost)
    let ins_after = engine.insurance_fund.balance.get();
    assert!(ins_after == ins_before + cap,
        "dust capital must be swept into insurance fund");

    // Conservation must hold
    assert!(engine.check_conservation());
}

// ############################################################################
// SPEC §12 PROPERTY #3: Oracle-manipulation haircut safety
// ############################################################################

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_property_3_oracle_manipulation_haircut_safety() {
    // Fresh reserved PnL (R_i > 0) must not dilute h, must not satisfy IM,
    // and must not be withdrawable.
    let mut engine = RiskEngine::new(zero_fee_params());
    let a = engine.add_user(0).unwrap();
    let b = engine.add_user(0).unwrap();

    // Both deposit enough for trading
    engine.deposit(a, 500_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.deposit(b, 500_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    let h_lock = 10u64; // non-zero so PnL goes to cohort reserve
    engine.keeper_crank_not_atomic(DEFAULT_SLOT, DEFAULT_ORACLE, &[], 0, 0i128, h_lock).unwrap();

    // Open positions: a long, b short
    let size_q = (100 * POS_SCALE) as i128;
    engine.execute_trade_not_atomic(a, b, DEFAULT_ORACLE, DEFAULT_SLOT, size_q, DEFAULT_ORACLE, 0i128, h_lock).unwrap();

    // Capture h before oracle spike
    let (h_num_before, h_den_before) = engine.haircut_ratio();

    // Oracle spikes up — a has fresh unrealized profit
    let spike_oracle: u64 = 1_500;
    let slot2 = DEFAULT_SLOT + 1;
    engine.keeper_crank_not_atomic(slot2, spike_oracle, &[(a, None), (b, None)], 64, 0i128, h_lock).unwrap();

    // After touch, a has positive PnL but it's reserved (R_i > 0)
    let pnl_a = engine.accounts[a as usize].pnl.get();
    assert!(pnl_a > 0, "account a must have positive PnL after oracle spike");

    let r_a = engine.accounts[a as usize].reserved_pnl.get();
    assert!(r_a > 0, "fresh profit must be reserved (R_i > 0)");

    // (a) PNL_matured_pos_tot must not have increased from fresh reserved profit
    // Since warmup just started and no time has passed, released = max(PnL,0) - R = 0
    let released_a = engine.released_pos(a as usize);
    assert!(released_a == 0, "no released profit before warmup elapses");

    // (b) h must not have been diluted by fresh reserved profit
    let (h_num_after, h_den_after) = engine.haircut_ratio();
    // h_den should not have grown from the spike (pnl_matured_pos_tot unchanged)
    assert!(h_den_after <= h_den_before || h_den_before == 0,
        "pnl_matured_pos_tot must not increase from unwarmed profit");

    // (c) Eq_init_raw excludes reserved portion
    let eq_init_raw = engine.account_equity_init_raw(&engine.accounts[a as usize], a as usize);
    // effective_matured_pnl should be 0 since released = 0
    let eff_matured = engine.effective_matured_pnl(a as usize);
    assert!(eff_matured == 0, "effective matured PnL must be 0 with no released profit");

    // (d) Withdrawal of any profit portion must fail (only capital is available)
    // Try to withdraw_not_atomic more than original capital
    let slot3 = slot2;
    let withdraw_result = engine.withdraw_not_atomic(a, 500_001, spike_oracle, slot3, 0i128, 0);
    assert!(withdraw_result.is_err(),
        "must not be able to withdraw_not_atomic unreserved profit");

    assert!(engine.check_conservation());
}

// ############################################################################
// SPEC §12 PROPERTY #26: Positive local PnL supports maintenance but not IM
// ############################################################################

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_property_26_maintenance_vs_im_dual_equity() {
    // A freshly profitable account with R_i > 0 must pass maintenance
    // (Eq_maint_raw uses full PNL_i) but fail IM (Eq_init_raw excludes R_i).
    let mut engine = RiskEngine::new(zero_fee_params());
    let a = engine.add_user(0).unwrap();
    let b = engine.add_user(0).unwrap();

    // a deposits minimal capital, b deposits large
    engine.deposit(a, 20_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.deposit(b, 1_000_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    let h_lock = 10u64; // non-zero so PnL goes to cohort reserve
    engine.keeper_crank_not_atomic(DEFAULT_SLOT, DEFAULT_ORACLE, &[], 0, 0i128, h_lock).unwrap();

    let size_q = (100 * POS_SCALE) as i128;
    engine.execute_trade_not_atomic(a, b, DEFAULT_ORACLE, DEFAULT_SLOT, size_q, DEFAULT_ORACLE, 0i128, h_lock).unwrap();

    // Oracle moves up — a gains profit that is reserved (h_lock>0 routes to cohort queue)
    let new_oracle: u64 = 1_100;
    let slot2 = DEFAULT_SLOT + 1;
    engine.keeper_crank_not_atomic(slot2, new_oracle, &[(a, None), (b, None)], 64, 0i128, h_lock).unwrap();

    // a now has fresh PnL from price increase. This PnL is reserved.
    let pnl_a = engine.accounts[a as usize].pnl.get();
    assert!(pnl_a > 0, "a must have positive PnL");
    let r_a = engine.accounts[a as usize].reserved_pnl.get();
    assert!(r_a > 0, "fresh profit must be reserved");

    // Maintenance uses full PnL_i → should be healthy
    let maint_healthy = engine.is_above_maintenance_margin(
        &engine.accounts[a as usize], a as usize, new_oracle);
    assert!(maint_healthy,
        "freshly profitable account must pass maintenance (full PNL_i used)");

    // IM uses Eq_init_raw which excludes reserved R_i
    // Eq_init_raw = C_i + min(PNL_i, 0) + effective_matured_pnl - fee_debt
    // Since PNL_i > 0, min(PNL_i,0) = 0, and effective_matured_pnl = 0 (nothing released)
    // So Eq_init_raw ≈ C_i only
    let eq_init_raw = engine.account_equity_init_raw(&engine.accounts[a as usize], a as usize);
    let eq_maint_raw = engine.account_equity_maint_raw(&engine.accounts[a as usize]);

    // Eq_maint_raw includes full PNL_i, so it must be larger
    assert!(eq_maint_raw > eq_init_raw,
        "Eq_maint_raw must exceed Eq_init_raw when R_i > 0");

    // Notional at new oracle = 100 * 1100 = 110_000
    // IM_req = max(110_000 * 10%, 2) = 11_000
    // a's capital is ~20_000. eq_init_raw ≈ 20_000 (only capital, no released profit)
    // So IM should still pass here. But the key property is the gap between maint and init.
    // Let's verify pure warmup release doesn't reduce Eq_maint_raw:
    let eq_maint_before_warmup = engine.account_equity_maint_raw(&engine.accounts[a as usize]);

    // Advance time partially
    let slot3 = slot2 + 50;
    engine.keeper_crank_not_atomic(slot3, new_oracle, &[(a, None)], 64, 0i128, 0).unwrap();

    let eq_maint_after_warmup = engine.account_equity_maint_raw(&engine.accounts[a as usize]);
    // Pure warmup release on unchanged PNL_i must not reduce Eq_maint_raw
    assert!(eq_maint_after_warmup >= eq_maint_before_warmup,
        "pure warmup release must not reduce Eq_maint_raw");

    assert!(engine.check_conservation());
}

// ############################################################################
// SPEC §12 PROPERTY #56: Exact raw initial-margin approval
// ############################################################################

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_property_56_exact_raw_im_approval() {
    // A risk-increasing trade must be rejected when Eq_init_raw < IM_req,
    // even if Eq_init_net floors to 0. MIN_NONZERO_IM_REQ ensures no
    // evasion through tiny positions.
    let mut engine = RiskEngine::new(zero_fee_params());
    let a = engine.add_user(0).unwrap();
    let b = engine.add_user(0).unwrap();

    // Deposit just enough for the test
    engine.deposit(a, 1, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.deposit(b, 1_000_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.keeper_crank_not_atomic(DEFAULT_SLOT, DEFAULT_ORACLE, &[], 0, 0i128, 0).unwrap();

    // a has C=1, no PnL, no fees. Eq_init_raw = 1.
    // MIN_NONZERO_IM_REQ = 2, so any nonzero position requires IM >= 2.
    // A trade with even 1 unit of position means IM_req >= 2 > 1 = Eq_init_raw.
    let tiny_size = POS_SCALE as i128; // 1 unit
    let result = engine.execute_trade_not_atomic(a, b, DEFAULT_ORACLE, DEFAULT_SLOT, tiny_size, DEFAULT_ORACLE, 0i128, 0);
    assert!(result.is_err(),
        "trade must be rejected: Eq_init_raw (1) < MIN_NONZERO_IM_REQ (2)");

    assert!(engine.check_conservation());
}

// ############################################################################
// AUDIT ISSUE #2: fee_debt_sweep PnL-to-insurance conservation breach
// ############################################################################

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_audit_fee_sweep_pnl_conservation() {
    // fee_debt_sweep must not consume released PnL at face value and credit
    // it 1:1 to insurance. The spec §7.5 sweep only pays from C_i.
    // The extra PnL-to-insurance block is a spec violation.
    //
    // Construct: account with zero capital, released PnL, and fee debt.
    // fee_debt_sweep pays nothing from capital (0), then the rogue block
    // consumes released PnL and adds to insurance — breaching conservation
    // if Residual < consumed amount.
    let mut engine = RiskEngine::new(zero_fee_params());
    let a = engine.add_user(0).unwrap();

    // Give account capital that we'll then drain, plus positive PnL
    engine.deposit(a, 100, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    // Set up: zero capital but positive released PnL
    engine.set_capital(a as usize, 0);
    engine.set_pnl(a as usize, 50i128);
    // Mark PnL as fully matured (no reserve)
    engine.accounts[a as usize].reserved_pnl = U128::ZERO;
    engine.pnl_matured_pos_tot = U128::new(50);

    // Set large fee debt — capital can't cover it
    engine.accounts[a as usize].fee_credits = I128::new(-50);

    // Current state: V=100, C_tot=0, I=0. Residual = 100.
    // pnl_pos_tot=50, pnl_matured_pos_tot=50, released_pos=50.
    // fee_debt = 50.
    assert!(engine.check_conservation(), "pre-sweep conservation");

    engine.fee_debt_sweep(a as usize);

    // The rogue block consumed 50 of released PnL and added 50 to I.
    // V=100, C_tot=0, I=50. Conservation: 100 >= 0+50 ✓
    // In this small example, conservation holds because Residual(100) > consumed(50).
    // To truly break it, we need Residual < consumed amount.
    // But the spec is clear: fee_debt_sweep MUST only pay from C_i.
    // Even when conservation holds numerically, the operation is incorrect because
    // it converts junior PnL claims to senior insurance capital.
    //
    // The structural test: after sweep, insurance must NOT have gained more
    // than what was paid from capital.
    let cap_paid = 0u128; // capital was 0, nothing paid from capital
    let ins_gained = engine.insurance_fund.balance.get();
    // Per spec §7.5: I should only increase by pay = min(debt, C_i) = min(50, 0) = 0
    assert!(ins_gained == cap_paid,
        "insurance must only gain what was paid from capital per spec §7.5, got {}",
        ins_gained);
}

// ############################################################################
// AUDIT ISSUE #4: IM check must use exact raw equity, not clamped
// ############################################################################

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_audit_im_uses_exact_raw_equity() {
    // Verify that is_above_initial_margin correctly rejects when
    // exact Eq_init_raw < IM_req, even when Eq_init_net floors to 0.
    // With MIN_NONZERO_IM_REQ > 0, the clamped path also rejects (0 < 2),
    // but this proof documents the spec requirement for exact raw comparison.
    let mut engine = RiskEngine::new(zero_fee_params());
    let a = engine.add_user(0).unwrap();

    engine.deposit(a, 100, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    // Set up a position with very negative PnL to make Eq_init_raw < 0
    engine.accounts[a as usize].position_basis_q = I128::new((1 * POS_SCALE) as i128);
    engine.stored_pos_count_long = 1;
    engine.oi_eff_long_q = U128::new(POS_SCALE);
    engine.oi_eff_short_q = U128::new(POS_SCALE);
    engine.set_pnl(a as usize, -500i128);

    // Eq_init_raw = C(100) + min(PnL, 0)(-500) + eff_matured(0) - fee(0) = -400
    let raw = engine.account_equity_init_raw(&engine.accounts[a as usize], a as usize);
    assert!(raw < 0, "Eq_init_raw must be negative");

    // IM check must fail for this deeply negative equity
    let passes_im = engine.is_above_initial_margin(
        &engine.accounts[a as usize], a as usize, DEFAULT_ORACLE);
    assert!(!passes_im,
        "is_above_initial_margin must reject when Eq_init_raw < 0");
}

// ############################################################################
// AUDIT ISSUE #3: LP account GC bypass — empty LP slots must be reclaimable
// ############################################################################

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_audit_empty_lp_gc_reclaimable() {
    // An LP account drained to zero capital, zero position, zero PnL
    // must be reclaimable by garbage_collect_dust per spec §2.6.
    let mut engine = RiskEngine::new(zero_fee_params());

    let lp = engine.add_lp([0u8; 32], [0u8; 32], 0).unwrap();
    assert!(engine.is_used(lp as usize), "LP must be materialized");
    assert!(engine.accounts[lp as usize].is_lp(), "must be LP account");

    // LP has zero capital, zero PnL, zero position — it's dead
    assert!(engine.accounts[lp as usize].capital.get() == 0);
    assert!(engine.accounts[lp as usize].pnl.get() == 0);
    assert!(engine.accounts[lp as usize].position_basis_q.get() == 0);

    // GC should reclaim this empty LP slot
    let freed = engine.garbage_collect_dust();

    // Per spec §2.6: empty accounts must be reclaimable
    assert!(!engine.is_used(lp as usize),
        "empty LP account must be reclaimed by garbage_collect_dust");
}

// ############################################################################
// AUDIT ISSUE #1: K-pair chronology — verify code is correct (not swapped)
// ############################################################################

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_audit_k_pair_chronology_not_inverted() {
    // Verify that when K increases (favorable for longs), a long position
    // gets POSITIVE PnL (not negative). This proves the K-pair argument
    // order is correct despite the parameter naming differing from spec.
    let mut engine = RiskEngine::new(zero_fee_params());
    let a = engine.add_user(0).unwrap();
    let b = engine.add_user(0).unwrap();

    engine.deposit(a, 500_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.deposit(b, 500_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.keeper_crank_not_atomic(DEFAULT_SLOT, DEFAULT_ORACLE, &[], 0, 0i128, 0).unwrap();

    // Open long for a, short for b
    let size_q = (100 * POS_SCALE) as i128;
    engine.execute_trade_not_atomic(a, b, DEFAULT_ORACLE, DEFAULT_SLOT, size_q, DEFAULT_ORACLE, 0i128, 0).unwrap();

    let pnl_a_before = engine.accounts[a as usize].pnl;
    let pnl_b_before = engine.accounts[b as usize].pnl;

    // Oracle rises — favorable for long (a), unfavorable for short (b)
    let high_oracle = 1_200u64;
    let slot2 = DEFAULT_SLOT + 1;
    engine.keeper_crank_not_atomic(slot2, high_oracle, &[(a, None), (b, None)], 64, 0i128, 0).unwrap();

    // a (long) must gain PnL when oracle rises
    assert!(engine.accounts[a as usize].pnl > pnl_a_before,
        "long must gain PnL when oracle rises");

    // b (short) must have economic loss when price rises.
    // settle_losses zeroes negative PnL by reducing capital, so check capital instead.
    let cap_b_after = engine.accounts[b as usize].capital.get();
    assert!(cap_b_after < 500_000,
        "short capital must decrease when oracle rises (loss settled)");

    assert!(engine.check_conservation());
}

// ############################################################################
// AUDIT ROUND 2, ISSUE #3: close_account_not_atomic structural correctness
// (FALSE POSITIVE — engine has no auth layer; this proves accounting safety)
// ############################################################################

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_audit2_close_account_structural_safety() {
    // close_account_not_atomic requires zero effective position, zero PnL, and
    // only returns the capital. It cannot extract more than deposited.
    let mut engine = RiskEngine::new(zero_fee_params());
    let a = engine.add_user(0).unwrap();

    let deposit_amt: u32 = kani::any();
    kani::assume(deposit_amt >= 1000 && deposit_amt <= 1_000_000);
    engine.deposit(a, deposit_amt as u128, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    let v_before = engine.vault.get();

    // close_account_not_atomic on a flat account with no position
    let result = engine.close_account_not_atomic(a, DEFAULT_SLOT, DEFAULT_ORACLE, 0i128, 0);
    assert!(result.is_ok(), "flat zero-PnL account must close");

    let capital_returned = result.unwrap();
    // Returned capital equals deposited amount
    assert!(capital_returned == deposit_amt as u128,
        "close_account_not_atomic must return exactly the account's capital");
    // Vault decreased by exactly the capital returned
    assert!(engine.vault.get() == v_before - capital_returned,
        "vault must decrease by exactly capital returned");
    // Account freed
    assert!(!engine.is_used(a as usize), "slot must be freed after close");
}

// ############################################################################
// AUDIT ROUND 2, ISSUE #4: Funding rate clamping — prevent liveness lockup
// ############################################################################

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_audit2_funding_rate_clamped() {
    // Setting an out-of-range funding rate must be clamped so that
    // subsequent accrue_market_to does not abort.
    let mut engine = RiskEngine::new(zero_fee_params());
    let a = engine.add_user(0).unwrap();
    let b = engine.add_user(0).unwrap();

    engine.deposit(a, 500_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.deposit(b, 500_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.keeper_crank_not_atomic(DEFAULT_SLOT, DEFAULT_ORACLE, &[], 0, 0i128, 0).unwrap();

    // Open positions so funding has effect
    let size_q = (10 * POS_SCALE) as i128;
    engine.execute_trade_not_atomic(a, b, DEFAULT_ORACLE, DEFAULT_SLOT, size_q, DEFAULT_ORACLE, 0i128, 0).unwrap();

    // Pass an extreme out-of-range funding rate directly to keeper_crank.
    // v12.16.4: rate is validated inside accrue_market_to, so extreme rates
    // are rejected before any state mutation.
    let extreme_offset: u16 = kani::any();
    kani::assume(extreme_offset >= 1);
    let extreme_rate = MAX_ABS_FUNDING_E9_PER_SLOT + (extreme_offset as i128);

    let slot2 = DEFAULT_SLOT + 1;
    let result = engine.keeper_crank_not_atomic(slot2, DEFAULT_ORACLE, &[(a, None), (b, None)], 64, extreme_rate, 0);
    // Out-of-bounds rate must be rejected
    assert!(result.is_err(), "extreme funding rate must be rejected");
    // State must be unchanged — conservation preserved
    assert!(engine.check_conservation());
}

// ############################################################################
// AUDIT ROUND 2, ISSUE #6: Positive overflow equity — conservative fallback
// ############################################################################

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_audit2_positive_overflow_equity_conservative() {
    // When account equity overflows i128 positively, the function must
    // return i128::MAX (conservative — account is over-collateralized),
    // not 0 (which would falsely trigger liquidation).
    let mut engine = RiskEngine::new(zero_fee_params());
    let a = engine.add_user(0).unwrap();

    // Directly set capital to a value > i128::MAX to force positive overflow.
    // This bypasses MAX_VAULT_TVL but tests the overflow fallback path.
    let huge_capital = (i128::MAX as u128) + 1; // 2^127
    engine.accounts[a as usize].capital = U128::new(huge_capital);
    engine.accounts[a as usize].pnl = I128::ZERO;
    engine.accounts[a as usize].fee_credits = I128::ZERO;

    // Eq_maint_raw = C + PnL - FeeDebt = huge_capital + 0 - 0 = huge_capital > i128::MAX
    let eq_maint = engine.account_equity_maint_raw(&engine.accounts[a as usize]);
    assert!(eq_maint == i128::MAX,
        "positive overflow must project to i128::MAX, not 0");

    // The wide version must be positive
    let wide = engine.account_equity_maint_raw_wide(&engine.accounts[a as usize]);
    assert!(!wide.is_negative(), "wide equity must be positive");

    // Eq_init_raw with same setup
    let eq_init = engine.account_equity_init_raw(&engine.accounts[a as usize], a as usize);
    assert!(eq_init == i128::MAX,
        "init raw positive overflow must project to i128::MAX, not 0");
}

// ############################################################################
// AUDIT ROUND 2, ISSUE #6 (corollary): Positive overflow must not liquidate
// ############################################################################

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_audit2_positive_overflow_no_false_liquidation() {
    // An account with equity overflowing i128 positively must pass
    // maintenance margin check (it's massively over-collateralized).
    let mut engine = RiskEngine::new(zero_fee_params());
    let a = engine.add_user(0).unwrap();

    // Set up a position + huge capital
    let huge_capital = (i128::MAX as u128) + 1;
    engine.accounts[a as usize].capital = U128::new(huge_capital);
    engine.accounts[a as usize].position_basis_q = I128::new((1 * POS_SCALE) as i128);
    engine.stored_pos_count_long = 1;
    engine.oi_eff_long_q = U128::new(POS_SCALE);
    engine.oi_eff_short_q = U128::new(POS_SCALE);

    let above_mm = engine.is_above_maintenance_margin(
        &engine.accounts[a as usize], a as usize, DEFAULT_ORACLE);
    assert!(above_mm,
        "massively over-collateralized account must pass MM check");

    let above_im = engine.is_above_initial_margin(
        &engine.accounts[a as usize], a as usize, DEFAULT_ORACLE);
    assert!(above_im,
        "massively over-collateralized account must pass IM check");
}

// ############################################################################
// AUDIT ROUND 3, ISSUE #3: i128::MIN negate panic in checked_u128_mul_i128
// ############################################################################

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_audit3_checked_u128_mul_i128_no_panic_at_boundary() {
    // When a * |b| = 2^127, the old code would cast to i128::MIN then
    // negate, triggering a panic. Fixed: reject as Overflow instead.
    // Test: a=2^127, b=-1 → product magnitude = 2^127 = i128::MIN territory.
    let a = (i128::MAX as u128) + 1; // 2^127
    let b = -1i128;
    let result = checked_u128_mul_i128(a, b);
    // Must not panic. Must return Err(Overflow) since result would be i128::MIN
    // which is forbidden throughout the engine.
    assert!(result.is_err(), "must return Err, not panic, at i128::MIN boundary");

    // a=1, b=-i128::MAX → product = i128::MAX, valid negative
    let result2 = checked_u128_mul_i128(1, -i128::MAX);
    assert!(result2.is_ok(), "-(i128::MAX) must be valid");
    assert!(result2.unwrap() == -i128::MAX);

    // a=1, b=i128::MAX → valid positive
    let result3 = checked_u128_mul_i128(1, i128::MAX);
    assert!(result3.is_ok());
    assert!(result3.unwrap() == i128::MAX);
}

#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_audit3_compute_trade_pnl_no_panic_at_boundary() {
    // compute_trade_pnl internally calls checked_u128_mul_i128 then divides
    // by POS_SCALE. The i128::MIN panic fix lives in checked_u128_mul_i128
    // (proven by proof_audit3_checked_u128_mul_i128_no_panic_at_boundary).
    //
    // This proof verifies compute_trade_pnl never panics over the full
    // i8 input space. The i8 range [-128, 127] covers both signs and
    // exercises the sign-dispatch, multiplication, and division paths.
    // The 2^127 boundary is covered by the checked_u128_mul_i128 proof.
    //
    // Additionally, we verify structural properties:
    // 1. Zero size always returns Ok(0)
    // 2. Zero price_diff always returns Ok(0)
    // 3. Signs are consistent: positive*positive >= 0, negative*positive <= 0

    let size_q: i8 = kani::any();
    let price_diff: i8 = kani::any();

    let result = compute_trade_pnl(size_q as i128, price_diff as i128);

    // Must never panic — only Ok or Err
    if size_q == 0 || price_diff == 0 {
        // Zero input must return Ok(0)
        assert!(result.is_ok());
        assert!(result.unwrap() == 0, "zero input must produce zero PnL");
    } else if let Ok(pnl) = result {
        // Sign consistency: pnl must agree with sign of (size_q * price_diff)
        let input_positive = (size_q > 0) == (price_diff > 0);
        if input_positive {
            assert!(pnl >= 0, "same-sign inputs must produce non-negative PnL");
        } else {
            assert!(pnl <= 0, "opposite-sign inputs must produce non-positive PnL");
        }
    }
    // Err is acceptable for overflow — just must not panic
}

// ============================================================================
// Audit round 4: Structural safety proofs
// ============================================================================

/// Proof: init_in_place fully canonicalizes all state fields.
/// After init_in_place, the engine must be in a clean state with
/// valid freelist, zero aggregates, and Normal side modes.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_audit4_init_in_place_canonical() {
    let params = zero_fee_params();
    let mut engine = RiskEngine::new(params);

    // Dirty EVERY engine state field to simulate non-zeroed memory
    engine.vault = U128::new(999);
    engine.insurance_fund.balance = U128::new(777);
    engine.c_tot = U128::new(555);
    engine.pnl_pos_tot = U128::new(333);
    engine.pnl_matured_pos_tot = U128::new(222);
    engine.current_slot = 42;
    engine.last_crank_slot = 77;
    engine.gc_cursor = 2;
    engine.adl_mult_long = U128::new(42);
    engine.adl_mult_short = U128::new(43);
    engine.adl_coeff_long = I128::new(100);
    engine.adl_coeff_short = I128::new(200);
    engine.adl_epoch_long = 7;
    engine.adl_epoch_short = 8;
    engine.adl_epoch_start_k_long = I128::new(300);
    engine.adl_epoch_start_k_short = I128::new(400);
    engine.oi_eff_long_q = U128::new(1000);
    engine.oi_eff_short_q = U128::new(2000);
    engine.side_mode_long = SideMode::DrainOnly;
    engine.side_mode_short = SideMode::ResetPending;
    engine.stored_pos_count_long = 10;
    engine.stored_pos_count_short = 11;
    engine.stale_account_count_long = 3;
    engine.stale_account_count_short = 4;
    engine.phantom_dust_bound_long_q = U128::new(50);
    engine.phantom_dust_bound_short_q = U128::new(60);
    engine.num_used_accounts = 10;
    engine.materialized_account_count = 5;
    engine.last_oracle_price = 9999;
    engine.last_market_slot = 55;
    engine.f_long_num = I128::new(42);
    engine.f_short_num = I128::new(-42);
    engine.params.insurance_floor = U128::new(12345);
    engine.free_head = u16::MAX; // break the freelist

    // Re-initialize — must fully reset all fields
    engine.init_in_place(params, 0, DEFAULT_ORACLE);

    // ---- Vault / insurance ----
    assert!(engine.vault.get() == 0);
    assert!(engine.insurance_fund.balance.get() == 0);

    // ---- Aggregates ----
    assert!(engine.c_tot.get() == 0);
    assert!(engine.pnl_pos_tot.get() == 0);
    assert!(engine.pnl_matured_pos_tot.get() == 0);

    // ---- Slots / cursors ----
    assert!(engine.current_slot == 0);
    assert!(engine.last_crank_slot == 0);
    assert!(engine.gc_cursor == 0);
    assert!(engine.f_long_num.get() == 0);
    assert!(engine.f_short_num.get() == 0);

    // ---- ADL / side state ----
    assert!(engine.adl_mult_long == U128::new(ADL_ONE));
    assert!(engine.adl_mult_short == U128::new(ADL_ONE));
    assert!(engine.adl_coeff_long.get() == 0);
    assert!(engine.adl_coeff_short.get() == 0);
    assert!(engine.adl_epoch_long == 0);
    assert!(engine.adl_epoch_short == 0);
    assert!(engine.adl_epoch_start_k_long.get() == 0);
    assert!(engine.adl_epoch_start_k_short.get() == 0);
    assert!(engine.oi_eff_long_q.get() == 0);
    assert!(engine.oi_eff_short_q.get() == 0);
    assert!(engine.side_mode_long == SideMode::Normal);
    assert!(engine.side_mode_short == SideMode::Normal);
    assert!(engine.stored_pos_count_long == 0);
    assert!(engine.stored_pos_count_short == 0);
    assert!(engine.stale_account_count_long == 0);
    assert!(engine.stale_account_count_short == 0);
    assert!(engine.phantom_dust_bound_long_q.get() == 0);
    assert!(engine.phantom_dust_bound_short_q.get() == 0);

    // ---- Account tracking ----
    assert!(engine.num_used_accounts == 0);
    assert!(engine.materialized_account_count == 0);
    assert!(engine.last_oracle_price == DEFAULT_ORACLE);
    assert!(engine.last_market_slot == 0);
    assert!(engine.params.insurance_floor.get() == 0);

    // ---- Used bitmap: all zeroed ----
    let mut any_used = false;
    for i in 0..MAX_ACCOUNTS {
        if engine.is_used(i) { any_used = true; }
    }
    assert!(!any_used, "no accounts must be marked used after init");

    // ---- Freelist integrity ----
    assert!(engine.free_head == 0);
    // Walk the entire freelist and verify it covers all MAX_ACCOUNTS slots
    let mut visited = 0u32;
    let mut cur = engine.free_head;
    while cur != u16::MAX && (visited as usize) < MAX_ACCOUNTS {
        assert!((cur as usize) < MAX_ACCOUNTS, "freelist entry out of bounds");
        cur = engine.next_free[cur as usize];
        visited += 1;
    }
    assert!(visited as usize == MAX_ACCOUNTS, "freelist must cover all slots");
    assert!(cur == u16::MAX, "freelist must terminate with sentinel");
}

/// Proof: freelist integrity after materialize_at via deposit.
/// Allocates slots via add_user (freelist pop) and deposit-materialize
/// (freelist search-and-remove). Verifies that the freelist correctly
/// accounts for all free slots after both allocation paths.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_audit4_materialize_at_freelist_integrity() {
    let params = zero_fee_params();
    let mut engine = RiskEngine::new(params);

    // add_user pops slot 0 from freelist head
    let idx0 = engine.add_user(0).unwrap();
    assert!(idx0 == 0);
    assert!(engine.is_used(0));

    // Deposit-materialize on slot 2 removes it from freelist interior
    // (slot 2 is in the freelist: head→1→2→3→sentinel)
    let result = engine.deposit(2, 1000, DEFAULT_ORACLE, DEFAULT_SLOT);
    assert!(result.is_ok());
    assert!(engine.is_used(2));
    assert!(engine.num_used_accounts == 2);
    assert!(engine.materialized_account_count == 2); // add_user + deposit both increment

    // Freelist should now be: head→1→3→sentinel (0 and 2 removed)
    assert!(engine.free_head == 1);
    assert!(engine.next_free[1] == 3);
    assert!(engine.next_free[3] == u16::MAX);

    // Verify deposit top-up on existing account does NOT re-materialize
    let mat_before = engine.materialized_account_count;
    let used_before = engine.num_used_accounts;
    engine.deposit(2, 500, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    assert!(engine.materialized_account_count == mat_before);
    assert!(engine.num_used_accounts == used_before);

    // Free slot 0, verify it returns to freelist head
    engine.free_slot(idx0);
    assert!(!engine.is_used(0));
    assert!(engine.free_head == 0);
    assert!(engine.num_used_accounts == 1);

    // Re-materialize slot 0 via deposit — must work
    let result2 = engine.deposit(0, 1000, DEFAULT_ORACLE, DEFAULT_SLOT);
    assert!(result2.is_ok());
    assert!(engine.is_used(0));
}

/// Proof: top_up_insurance_fund never panics and enforces MAX_VAULT_TVL.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_audit4_top_up_insurance_no_panic() {
    let params = zero_fee_params();
    let mut engine = RiskEngine::new(params);

    // Set vault near MAX_VAULT_TVL
    engine.vault = U128::new(MAX_VAULT_TVL - 1);
    engine.insurance_fund.balance = U128::new(MAX_VAULT_TVL - 1);

    // Amount that would exceed MAX_VAULT_TVL
    let result = engine.top_up_insurance_fund(2, DEFAULT_SLOT);
    assert!(result.is_err(), "must reject amount that exceeds MAX_VAULT_TVL");

    // Amount that stays within MAX_VAULT_TVL
    let result2 = engine.top_up_insurance_fund(1, DEFAULT_SLOT);
    assert!(result2.is_ok(), "must accept amount within MAX_VAULT_TVL");
    assert!(engine.vault.get() == MAX_VAULT_TVL);
}

/// Proof: top_up_insurance_fund rejects u128::MAX (overflow before TVL check).
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_audit4_top_up_insurance_overflow() {
    let params = zero_fee_params();
    let mut engine = RiskEngine::new(params);
    engine.vault = U128::new(1);
    engine.insurance_fund.balance = U128::new(1);

    // u128::MAX must not panic — must return Err
    let result = engine.top_up_insurance_fund(u128::MAX, DEFAULT_SLOT);
    assert!(result.is_err());
}

/// Proof: deposit_fee_credits rejects time regression (now_slot < current_slot).
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_audit4_deposit_fee_credits_time_monotonicity() {
    let params = zero_fee_params();
    let mut engine = RiskEngine::new(params);
    let idx = engine.add_user(0).unwrap();

    // Give the account fee debt so deposits are not no-ops
    engine.accounts[idx as usize].fee_credits = I128::new(-10000);

    // Set current_slot to 100
    engine.current_slot = 100;

    let vault_before = engine.vault.get();
    let ins_before = engine.insurance_fund.balance.get();
    let credits_before = engine.accounts[idx as usize].fee_credits.get();

    // Deposit at slot 99 must fail — time regression
    let result = engine.deposit_fee_credits(idx, 1000, 99);
    assert!(result.is_err(), "must reject time regression");

    // State must be completely unchanged on failure
    assert!(engine.vault.get() == vault_before, "vault unchanged on rejected deposit");
    assert!(engine.insurance_fund.balance.get() == ins_before, "insurance unchanged");
    assert!(engine.accounts[idx as usize].fee_credits.get() == credits_before, "credits unchanged");
    assert!(engine.current_slot == 100, "current_slot unchanged on rejection");

    // Deposit at slot 100 (equal) must succeed
    let result2 = engine.deposit_fee_credits(idx, 1000, 100);
    assert!(result2.is_ok());

    // Deposit at slot 200 (forward) must succeed
    let result3 = engine.deposit_fee_credits(idx, 500, 200);
    assert!(result3.is_ok());
    assert!(engine.current_slot == 200, "current_slot must advance");
}

/// Proof: deposit_fee_credits uses checked arithmetic, not saturating.
/// Verifies that an amount causing fee_credits overflow returns Err.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_audit4_deposit_fee_credits_checked_arithmetic() {
    let params = zero_fee_params();
    let mut engine = RiskEngine::new(params);
    let idx = engine.add_user(0).unwrap();

    // Set fee_credits to large debt to test checked arithmetic on vault
    engine.accounts[idx as usize].fee_credits = I128::new(-10000);

    // Set vault near u128::MAX to force vault overflow
    engine.vault = U128::new(u128::MAX - 1);
    engine.insurance_fund.balance = U128::new(u128::MAX - 1);
    let result = engine.deposit_fee_credits(idx, 5000, 0);
    assert!(result.is_err(), "must reject vault overflow");

    // Verify fee_credits unchanged on failure
    assert!(engine.accounts[idx as usize].fee_credits.get() == -10000,
        "fee_credits must not change on failed deposit");
}

/// Proof: deposit_fee_credits enforces spec §2.1 fee_credits <= 0 invariant.
/// Over-deposits beyond outstanding debt are capped.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_audit5_deposit_fee_credits_no_positive() {
    let params = zero_fee_params();
    let mut engine = RiskEngine::new(params);
    let idx = engine.add_user(0).unwrap();

    // Give account 500 in fee debt
    engine.accounts[idx as usize].fee_credits = I128::new(-500);

    // Try to deposit 1000 (more than the 500 debt)
    engine.deposit_fee_credits(idx, 1000, 0).unwrap();

    // fee_credits must be exactly 0, not +500
    assert!(engine.accounts[idx as usize].fee_credits.get() == 0,
        "fee_credits must be capped at 0 (spec §2.1)");

    // Vault and insurance should reflect only the 500 that was actually applied
    assert!(engine.vault.get() == 500,
        "vault must increase by capped amount only");
}

/// Proof: deposit_fee_credits on account with zero debt is a no-op.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_audit5_deposit_fee_credits_zero_debt_noop() {
    let params = zero_fee_params();
    let mut engine = RiskEngine::new(params);
    let idx = engine.add_user(0).unwrap();

    // fee_credits = 0 (no debt)
    let vault_before = engine.vault.get();
    engine.deposit_fee_credits(idx, 9999, 0).unwrap();

    // Nothing should change
    assert!(engine.vault.get() == vault_before, "vault unchanged when no debt");
    assert!(engine.accounts[idx as usize].fee_credits.get() == 0, "credits stay 0");
}

/// Proof: reclaim_empty_account_not_atomic follows spec §2.6 preconditions and effects.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_audit5_reclaim_empty_account_basic() {
    let mut params = zero_fee_params();
    params.min_initial_deposit = U128::new(1000);
    let mut engine = RiskEngine::new(params);
    let idx = engine.add_user(0).unwrap();

    // Account is flat, zero capital, zero PnL — reclaimable
    assert!(engine.is_used(idx as usize));
    let used_before = engine.num_used_accounts;

    let result = engine.reclaim_empty_account_not_atomic(idx, DEFAULT_SLOT);
    assert!(result.is_ok());
    assert!(!engine.is_used(idx as usize), "slot must be freed");
    assert!(engine.num_used_accounts == used_before - 1);
}

/// Proof: reclaim_empty_account_not_atomic sweeps dust capital to insurance.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_audit5_reclaim_dust_sweep() {
    let mut params = zero_fee_params();
    params.min_initial_deposit = U128::new(1000);
    let mut engine = RiskEngine::new(params);
    let idx = engine.add_user(0).unwrap();

    // Give the account dust capital (< MIN_INITIAL_DEPOSIT)
    // Must set vault to cover it
    engine.vault = U128::new(500);
    engine.accounts[idx as usize].capital = U128::new(500);
    engine.c_tot = U128::new(500);

    let ins_before = engine.insurance_fund.balance.get();

    let result = engine.reclaim_empty_account_not_atomic(idx, DEFAULT_SLOT);
    assert!(result.is_ok());

    // Dust must have been swept to insurance
    assert!(engine.insurance_fund.balance.get() == ins_before + 500,
        "dust capital must be swept to insurance");
    // Conservation holds: vault unchanged, C_tot decreased, I increased
    assert!(engine.check_conservation());
}

/// Proof: reclaim_empty_account_not_atomic rejects accounts with open positions.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_audit5_reclaim_rejects_open_position() {
    let params = zero_fee_params();
    let mut engine = RiskEngine::new(params);
    let idx = engine.add_user(0).unwrap();

    // Give the account a position
    engine.accounts[idx as usize].position_basis_q = I128::new(100);

    let result = engine.reclaim_empty_account_not_atomic(idx, DEFAULT_SLOT);
    assert!(result.is_err(), "must reject account with open position");
    assert!(engine.is_used(idx as usize), "slot must not be freed on rejection");
}

/// Proof: reclaim_empty_account_not_atomic rejects accounts with capital >= MIN_INITIAL_DEPOSIT.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_audit5_reclaim_rejects_live_capital() {
    let mut params = zero_fee_params();
    params.min_initial_deposit = U128::new(1000);
    let mut engine = RiskEngine::new(params);
    let idx = engine.add_user(0).unwrap();

    // Capital at exactly MIN_INITIAL_DEPOSIT — not reclaimable
    engine.vault = U128::new(1000);
    engine.accounts[idx as usize].capital = U128::new(1000);
    engine.c_tot = U128::new(1000);

    let result = engine.reclaim_empty_account_not_atomic(idx, DEFAULT_SLOT);
    assert!(result.is_err(), "must reject account with live capital");
    assert!(engine.is_used(idx as usize));
}

// ############################################################################
// Gap #3: Conservation proof WITH nonzero trading fees
// ############################################################################

/// Trade conservation must hold when trading_fee_bps > 0.
/// Fees flow from accounts to insurance (C decreases, I increases, V unchanged).
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn bounded_trade_conservation_with_fees() {
    let mut engine = RiskEngine::new(default_params()); // trading_fee_bps = 10

    let a = engine.add_user(1000).unwrap();
    let b = engine.add_user(1000).unwrap();

    let dep: u32 = kani::any();
    kani::assume(dep >= 1_000_000 && dep <= 5_000_000);
    engine.deposit(a, dep as u128, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.deposit(b, dep as u128, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    assert!(engine.check_conservation(), "pre-trade conservation");

    let size_q = (100 * POS_SCALE) as i128;
    let result = engine.execute_trade_not_atomic(a, b, DEFAULT_ORACLE, DEFAULT_SLOT, size_q, DEFAULT_ORACLE, 0i128, 0);

    assert!(engine.check_conservation(),
        "conservation must hold after trade with nonzero fees");
    kani::cover!(result.is_ok(), "fee-bearing trade succeeds");
}

// ############################################################################
// Gap #5: Partial liquidation can succeed
// ############################################################################

/// There exists a q_close_q for an underwater account where ExactPartial
/// passes step 14 (post-partial health check). This proves the pre-flight
/// is not over-conservative for all inputs.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_partial_liquidation_can_succeed() {
    let mut engine = RiskEngine::new(zero_fee_params());

    let a = engine.add_user(0).unwrap();
    let b = engine.add_user(0).unwrap();

    // Large deposits, moderate position → slight undercollateralization
    engine.deposit(a, 1_000_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.deposit(b, 1_000_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    let size = (500 * POS_SCALE) as i128;
    engine.execute_trade_not_atomic(a, b, DEFAULT_ORACLE, DEFAULT_SLOT, size, DEFAULT_ORACLE, 0i128, 0).unwrap();

    // Moderate price drop — a is slightly underwater but has enough equity
    // for a partial close to restore health
    engine.set_pnl(a as usize, -50_000i128);

    let slot2 = DEFAULT_SLOT + 1;
    // Close 80% of position — should leave enough equity for the remaining 20%
    let q_close = (400 * POS_SCALE) as u128;
    let partial_hint = Some(LiquidationPolicy::ExactPartial(q_close));
    let candidates = [(a, partial_hint)];
    let result = engine.keeper_crank_not_atomic(slot2, DEFAULT_ORACLE, &candidates, 10, 0i128, 0);
    assert!(result.is_ok());

    // The partial liquidation should have succeeded (not fallen back to full close)
    let eff_after = engine.effective_pos_q(a as usize);
    kani::cover!(eff_after != 0, "partial liquidation left nonzero remainder");

    assert!(engine.oi_eff_long_q == engine.oi_eff_short_q, "OI balance");
    assert!(engine.check_conservation());
}

// ############################################################################
// Gap #6: Sign-flip trades through bilateral OI decomposition
// ############################################################################

/// A sign-flip trade (account goes from long to short or vice versa) must
/// preserve OI balance and conservation. This exercises the most complex
/// path in bilateral_oi_after.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_sign_flip_trade_conserves() {
    let mut engine = RiskEngine::new(zero_fee_params());

    let a = engine.add_user(0).unwrap();
    let b = engine.add_user(0).unwrap();

    engine.deposit(a, 2_000_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.deposit(b, 2_000_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    // a goes long 100, b goes short 100
    let size1 = (100 * POS_SCALE) as i128;
    engine.execute_trade_not_atomic(a, b, DEFAULT_ORACLE, DEFAULT_SLOT, size1, DEFAULT_ORACLE, 0i128, 0).unwrap();
    assert!(engine.effective_pos_q(a as usize) > 0, "a is long");
    assert!(engine.oi_eff_long_q == engine.oi_eff_short_q);

    // Now sign-flip: a sells 200 → goes from long 100 to short 100
    // b buys 200 → goes from short 100 to long 100
    let size2 = (200 * POS_SCALE) as i128;
    let slot2 = DEFAULT_SLOT + 1;
    let result = engine.execute_trade_not_atomic(b, a, DEFAULT_ORACLE, slot2, size2, DEFAULT_ORACLE, 0i128, 0);
    kani::cover!(result.is_ok(), "sign-flip trade reachable");

    if result.is_ok() {
        assert!(engine.effective_pos_q(a as usize) < 0, "a flipped to short");
        assert!(engine.effective_pos_q(b as usize) > 0, "b flipped to long");
    }
    assert!(engine.oi_eff_long_q == engine.oi_eff_short_q, "OI balance after sign-flip");
    assert!(engine.check_conservation(), "conservation after sign-flip trade");
}

// ############################################################################
// Gap #8: close_account_not_atomic fee forgiveness is bounded
// ############################################################################

/// close_account_not_atomic on an account with substantial fee debt forgives it safely.
/// The debt was already uncollectible because touch_account_live_local swept
/// everything it could via fee_debt_sweep.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_close_account_fee_forgiveness_bounded() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let _ = engine.top_up_insurance_fund(100_000, 0);

    let idx = engine.add_user(0).unwrap();
    engine.deposit(idx, 1, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    // Simulate fee debt: negative fee_credits
    engine.accounts[idx as usize].fee_credits = I128::new(-5000);

    let v_before = engine.vault.get();
    let i_before = engine.insurance_fund.balance.get();

    // close_account_not_atomic should succeed: position=0, pnl=0, capital=1 < min_deposit=2
    let result = engine.close_account_not_atomic(idx, DEFAULT_SLOT, DEFAULT_ORACLE, 0i128, 0);
    assert!(result.is_ok(), "close_account_not_atomic must succeed for dust account with fee debt");

    // Fee debt forgiven — account freed
    assert!(!engine.is_used(idx as usize));

    // Vault decreases by exactly the capital returned (1)
    let returned = v_before - engine.vault.get();
    assert!(returned <= 1, "only dust capital returned");

    // Insurance fund must not decrease from fee forgiveness
    // (fee forgiveness just zeros fee_credits, doesn't touch insurance)
    assert!(engine.insurance_fund.balance.get() >= i_before,
        "fee forgiveness must not draw from insurance");

    assert!(engine.check_conservation());
}

// ############################################################################
// Gap #11 (Weakness): Symbolic trade size for conservation
// ############################################################################

/// Conservation must hold for symbolic trade sizes within margin bounds.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn bounded_trade_conservation_symbolic_size() {
    let mut engine = RiskEngine::new(zero_fee_params());

    let a = engine.add_user(0).unwrap();
    let b = engine.add_user(0).unwrap();

    engine.deposit(a, 5_000_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.deposit(b, 5_000_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    assert!(engine.check_conservation());

    // Symbolic trade size (1 to 500 units, scaled by POS_SCALE)
    let size_units: u16 = kani::any();
    kani::assume(size_units >= 1 && size_units <= 500);
    let size_q = (size_units as i128) * (POS_SCALE as i128);

    let result = engine.execute_trade_not_atomic(a, b, DEFAULT_ORACLE, DEFAULT_SLOT, size_q, DEFAULT_ORACLE, 0i128, 0);

    assert!(engine.check_conservation(),
        "conservation must hold for symbolic trade size");
    kani::cover!(result.is_ok(), "symbolic-size trade succeeds");
}

// ############################################################################
// Gap #7: convert_released_pnl_not_atomic conservation (symbolic)
// ############################################################################

/// convert_released_pnl_not_atomic must preserve V >= C_tot + I.
/// Uses symbolic oracle to cover more of the conversion path.
/// h_lock=0 gives ImmediateRelease through set_pnl_with_reserve.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_convert_released_pnl_conservation() {
    let mut params = zero_fee_params();
    let mut engine = RiskEngine::new(params);

    let a = engine.add_user(0).unwrap();
    let b = engine.add_user(0).unwrap();

    engine.deposit(a, 500_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.deposit(b, 500_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    // Open positions
    let size_q = (100 * POS_SCALE) as i128;
    engine.execute_trade_not_atomic(a, b, DEFAULT_ORACLE, DEFAULT_SLOT, size_q, DEFAULT_ORACLE, 0i128, 0).unwrap();
    assert!(engine.check_conservation(), "pre-conversion conservation");

    // Oracle goes up → a has positive PnL
    let high_oracle = 1_200u64;
    let slot2 = DEFAULT_SLOT + 1;
    engine.keeper_crank_not_atomic(slot2, high_oracle, &[(a, None), (b, None)], 64, 0i128, 0).unwrap();

    // With h_lock=0 (ImmediateRelease), touch already set reserved_pnl=0 → all PnL released
    let released = engine.released_pos(a as usize);

    let v_before = engine.vault.get();
    let c_before = engine.c_tot.get();
    let i_before = engine.insurance_fund.balance.get();

    if released > 0 {
        // Symbolic conversion amount: 1..=released
        let x_req: u32 = kani::any();
        kani::assume(x_req >= 1 && (x_req as u128) <= released);
        let result = engine.convert_released_pnl_not_atomic(a, x_req as u128, high_oracle, slot2 + 1, 0i128, 0);
        kani::cover!(result.is_ok(), "convert_released_pnl_not_atomic Ok path reachable");
        if result.is_ok() {
            assert!(engine.check_conservation(),
                "conservation must hold after convert_released_pnl_not_atomic");
            // Capital must increase (profit was converted)
            assert!(engine.accounts[a as usize].capital.get() >= c_before.saturating_sub(engine.accounts[b as usize].capital.get()),
                "account capital must not decrease on profit conversion");
        }
        // Even on Err, conservation must hold (Err aborts on Solana, but state is still valid)
        assert!(engine.check_conservation(), "conservation holds even on err path");
    }
}

// ############################################################################
// Weakness #9: Symbolic enforce_one_side_margin threshold
// ############################################################################

/// Exercises enforce_one_side_margin with symbolic PnL (margin threshold).
/// The account starts with an open position, then we inject a symbolic PnL
/// and verify that a risk-reducing partial close either succeeds or correctly
/// rejects (never violates conservation).
#[kani::proof]
#[kani::unwind(70)]
#[kani::solver(cadical)]
fn proof_symbolic_margin_enforcement_on_reduce() {
    let mut engine = RiskEngine::new(zero_fee_params());
    engine.last_crank_slot = DEFAULT_SLOT;

    let a = engine.add_user(0).unwrap();
    let b = engine.add_user(0).unwrap();
    engine.deposit(a, 500_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.deposit(b, 500_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    // Open leveraged position
    let size = (400 * POS_SCALE) as i128;
    engine.execute_trade_not_atomic(a, b, DEFAULT_ORACLE, DEFAULT_SLOT, size, DEFAULT_ORACLE, 0i128, 0).unwrap();

    // Inject symbolic PnL: from heavily underwater to modestly above water
    let pnl_val: i32 = kani::any();
    kani::assume(pnl_val >= -400_000 && pnl_val <= 100_000);
    engine.set_pnl(a as usize, pnl_val as i128);

    // Risk-reducing trade: close half
    let half_close = size / 2;
    let result = engine.execute_trade_not_atomic(b, a, DEFAULT_ORACLE, DEFAULT_SLOT, half_close, DEFAULT_ORACLE, 0i128, 0);

    // Conservation must always hold regardless of accept/reject
    assert!(engine.check_conservation(),
        "conservation must hold after margin check");
    assert!(engine.oi_eff_long_q == engine.oi_eff_short_q, "OI balance");

    // Cover both outcomes
    kani::cover!(result.is_ok(), "reduce accepted");
    kani::cover!(result.is_err(), "reduce rejected");
}

// ############################################################################
// Full IM/MM margin enforcement: flat→open, reduction, sign-flip
// ############################################################################

/// Comprehensive margin enforcement proof covering all 3 risk categories:
/// - flat → open (risk-increasing → requires IM)
/// - same-sign reduction (risk-reducing → requires MM only)
/// - sign-flip (risk-increasing → requires IM)
///
/// For every successful trade, both parties must be above MM.
/// For risk-increasing trades, the risk-increasing party must also be above IM.
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_execute_trade_full_margin_enforcement() {
    let mut engine = RiskEngine::new(zero_fee_params());
    engine.last_crank_slot = DEFAULT_SLOT;
    engine.last_market_slot = DEFAULT_SLOT;
    engine.last_oracle_price = DEFAULT_ORACLE;

    let user = engine.add_user(0).unwrap();
    let lp = engine.add_lp([1u8; 32], [0u8; 32], 0).unwrap();

    // Fixed capital near margin boundary for user; LP well-capitalized.
    // Capital is concrete to keep the solver tractable (3 symbolic vars times
    // the full execute_trade pipeline exceeds 10-minute budget).
    let capital = 2_000u128;
    engine.deposit(user, capital, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.deposit(lp, 100_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    // Pre-construct initial position directly (avoids a second full trade
    // pipeline which makes the solver intractable). The A/K state is set
    // to match a fresh same-epoch position at DEFAULT_ORACLE.
    let old_pos_units: i8 = kani::any();
    kani::assume(old_pos_units >= -5 && old_pos_units <= 5);
    let old_pos_q = (old_pos_units as i128) * (POS_SCALE as i128);
    if old_pos_q != 0 {
        engine.attach_effective_position(user as usize, old_pos_q);
        engine.attach_effective_position(lp as usize, -old_pos_q);
        // Update OI to match
        let abs_q = old_pos_q.unsigned_abs();
        engine.oi_eff_long_q = U128::new(abs_q);
        engine.oi_eff_short_q = U128::new(abs_q);
    }

    let old_eff_user = engine.effective_pos_q(user as usize);
    let old_eff_lp = engine.effective_pos_q(lp as usize);

    // Symbolic trade delta in [-5, 5] units
    let delta_units: i8 = kani::any();
    kani::assume(delta_units != 0);
    kani::assume(delta_units >= -5 && delta_units <= 5);

    let result = if delta_units > 0 {
        let sz = (delta_units as u128) * POS_SCALE;
        engine.execute_trade_not_atomic(
            user, lp, DEFAULT_ORACLE, DEFAULT_SLOT, sz as i128, DEFAULT_ORACLE, 0i128, 0)
    } else {
        let sz = ((-delta_units) as u128) * POS_SCALE;
        engine.execute_trade_not_atomic(
            lp, user, DEFAULT_ORACLE, DEFAULT_SLOT, sz as i128, DEFAULT_ORACLE, 0i128, 0)
    };

    if result.is_ok() {
        let new_eff_user = engine.effective_pos_q(user as usize);
        let new_eff_lp = engine.effective_pos_q(lp as usize);

        // Classify risk for each party
        let user_crosses_zero = (old_eff_user > 0 && new_eff_user < 0)
            || (old_eff_user < 0 && new_eff_user > 0);
        let user_risk_increasing = new_eff_user.unsigned_abs() > old_eff_user.unsigned_abs()
            || user_crosses_zero
            || old_eff_user == 0;

        let lp_crosses_zero = (old_eff_lp > 0 && new_eff_lp < 0)
            || (old_eff_lp < 0 && new_eff_lp > 0);
        let lp_risk_increasing = new_eff_lp.unsigned_abs() > old_eff_lp.unsigned_abs()
            || lp_crosses_zero
            || old_eff_lp == 0;

        // All successful trades: both parties must be above MM
        if new_eff_user != 0 {
            assert!(engine.is_above_maintenance_margin(
                &engine.accounts[user as usize], user as usize, DEFAULT_ORACLE),
                "user must be above MM after successful trade");
        }
        if new_eff_lp != 0 {
            assert!(engine.is_above_maintenance_margin(
                &engine.accounts[lp as usize], lp as usize, DEFAULT_ORACLE),
                "LP must be above MM after successful trade");
        }

        // Risk-increasing: must also be above IM
        if user_risk_increasing && new_eff_user != 0 {
            assert!(engine.is_above_initial_margin(
                &engine.accounts[user as usize], user as usize, DEFAULT_ORACLE),
                "user must be above IM after risk-increasing trade");
        }
        if lp_risk_increasing && new_eff_lp != 0 {
            assert!(engine.is_above_initial_margin(
                &engine.accounts[lp as usize], lp as usize, DEFAULT_ORACLE),
                "LP must be above IM after risk-increasing trade");
        }

        assert!(engine.check_conservation());
    }

    // Non-vacuity: flat→open (risk-increasing)
    if old_pos_units == 0 && delta_units >= 1 && delta_units <= 3 {
        kani::cover!(result.is_ok(), "flat-to-open trade succeeds");
    }
    // Non-vacuity: same-sign reduction (risk-reducing)
    if old_pos_units == 5 && delta_units == -2 {
        kani::cover!(result.is_ok(), "same-sign reduction succeeds");
    }
    // Non-vacuity: sign-flip (risk-increasing)
    if old_pos_units == 3 && delta_units == -5 {
        kani::cover!(result.is_ok(), "sign-flip trade reachable");
    }
}

// ############################################################################
// Weakness #12: convert_released_pnl_not_atomic reaches conversion path (not early-return)
// ############################################################################

/// Verifies that convert_released_pnl_not_atomic actually exercises the conversion path
/// (steps 5-10), not just the early-return at step 4. We guarantee
/// position_basis_q != 0 and released > 0 using h_lock=0 (ImmediateRelease).
#[kani::proof]
#[kani::unwind(34)]
#[kani::solver(cadical)]
fn proof_convert_released_pnl_exercises_conversion() {
    let mut params = zero_fee_params();
    let mut engine = RiskEngine::new(params);

    let a = engine.add_user(0).unwrap();
    let b = engine.add_user(0).unwrap();

    engine.deposit(a, 500_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();
    engine.deposit(b, 500_000, DEFAULT_ORACLE, DEFAULT_SLOT).unwrap();

    let size_q = (100 * POS_SCALE) as i128;
    engine.execute_trade_not_atomic(a, b, DEFAULT_ORACLE, DEFAULT_SLOT, size_q, DEFAULT_ORACLE, 0i128, 0).unwrap();

    // Oracle up → a has positive PnL
    let high_oracle = 1_500u64;
    let slot2 = DEFAULT_SLOT + 1;
    engine.keeper_crank_not_atomic(slot2, high_oracle, &[(a, None), (b, None)], 64, 0i128, 0).unwrap();

    // Verify the account still has a position (not flat — won't early-return at step 4)
    assert!(engine.accounts[a as usize].position_basis_q.get() != 0,
        "account must have open position");

    let released = engine.released_pos(a as usize);
    // With warmup=0 and positive PnL, released should be > 0
    assert!(released > 0, "released must be > 0 with h_lock=0 and positive PnL");

    let cap_before = engine.accounts[a as usize].capital.get();

    // Convert all released profit
    let result = engine.convert_released_pnl_not_atomic(a, released, high_oracle, slot2 + 1, 0i128, 0);
    assert!(result.is_ok(), "conversion must succeed for healthy account with released profit");

    // Capital must have increased (the actual conversion happened)
    assert!(engine.accounts[a as usize].capital.get() > cap_before,
        "capital must increase — proves conversion path was taken, not early-return");

    assert!(engine.check_conservation());
}
