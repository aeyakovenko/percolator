//! Bounded rollback atomicity proofs for `withdraw_not_atomic`.
//!
//! These harnesses target the concrete bug class where `withdraw_not_atomic`
//! can settle local account effects before a later balance check rejects.
//! The postcondition checks the withdraw mutation cone on the `Err` path.

#![cfg(kani)]

mod common;
use common::*;

fn assert_withdraw_relevant_state_restored(engine: &RiskEngine, before: &RiskEngine, idx: usize) {
    assert_eq!(engine.vault.get(), before.vault.get());
    assert_eq!(
        engine.insurance_fund.balance.get(),
        before.insurance_fund.balance.get()
    );
    assert_eq!(engine.current_slot, before.current_slot);
    assert_eq!(engine.c_tot.get(), before.c_tot.get());
    assert_eq!(engine.pnl_pos_tot, before.pnl_pos_tot);
    assert_eq!(engine.pnl_matured_pos_tot, before.pnl_matured_pos_tot);
    assert_eq!(engine.oi_eff_long_q, before.oi_eff_long_q);
    assert_eq!(engine.oi_eff_short_q, before.oi_eff_short_q);
    assert_eq!(engine.stored_pos_count_long, before.stored_pos_count_long);
    assert_eq!(engine.stored_pos_count_short, before.stored_pos_count_short);
    assert_eq!(
        engine.stale_account_count_long,
        before.stale_account_count_long
    );
    assert_eq!(
        engine.stale_account_count_short,
        before.stale_account_count_short
    );
    assert_eq!(
        engine.materialized_account_count,
        before.materialized_account_count
    );
    assert_eq!(engine.neg_pnl_account_count, before.neg_pnl_account_count);
    assert_eq!(engine.rr_cursor_position, before.rr_cursor_position);
    assert_eq!(engine.sweep_generation, before.sweep_generation);
    assert_eq!(
        engine.stress_consumed_bps_e9_since_envelope,
        before.stress_consumed_bps_e9_since_envelope
    );
    assert_eq!(
        engine.stress_envelope_remaining_indices,
        before.stress_envelope_remaining_indices
    );
    assert_eq!(
        engine.stress_envelope_start_slot,
        before.stress_envelope_start_slot
    );
    assert_eq!(
        engine.stress_envelope_start_generation,
        before.stress_envelope_start_generation
    );
    assert_eq!(
        engine.last_sweep_generation_advance_slot,
        before.last_sweep_generation_advance_slot
    );
    assert_eq!(
        engine.bankruptcy_hmax_lock_active,
        before.bankruptcy_hmax_lock_active
    );
    assert_eq!(engine.last_oracle_price, before.last_oracle_price);
    assert_eq!(engine.fund_px_last, before.fund_px_last);
    assert_eq!(engine.last_market_slot, before.last_market_slot);
    assert_eq!(engine.f_long_num, before.f_long_num);
    assert_eq!(engine.f_short_num, before.f_short_num);
    assert_eq!(engine.num_used_accounts, before.num_used_accounts);
    assert_eq!(engine.free_head, before.free_head);

    let word = idx >> 6;
    assert_eq!(engine.used[word], before.used[word]);
    assert_eq!(engine.next_free[idx], before.next_free[idx]);
    assert_eq!(engine.prev_free[idx], before.prev_free[idx]);
    let account = &engine.accounts[idx];
    let before_account = &before.accounts[idx];
    assert_eq!(account.capital.get(), before_account.capital.get());
    assert_eq!(account.kind, before_account.kind);
    assert_eq!(account.pnl, before_account.pnl);
    assert_eq!(account.reserved_pnl, before_account.reserved_pnl);
    assert_eq!(account.position_basis_q, before_account.position_basis_q);
    assert_eq!(account.adl_a_basis, before_account.adl_a_basis);
    assert_eq!(account.adl_k_snap, before_account.adl_k_snap);
    assert_eq!(account.f_snap, before_account.f_snap);
    assert_eq!(account.adl_epoch_snap, before_account.adl_epoch_snap);
    assert_eq!(account.fee_credits.get(), before_account.fee_credits.get());
    assert_eq!(account.last_fee_slot, before_account.last_fee_slot);
    assert_eq!(account.sched_present, before_account.sched_present);
    assert_eq!(account.sched_remaining_q, before_account.sched_remaining_q);
    assert_eq!(account.sched_anchor_q, before_account.sched_anchor_q);
    assert_eq!(account.sched_start_slot, before_account.sched_start_slot);
    assert_eq!(account.sched_horizon, before_account.sched_horizon);
    assert_eq!(account.sched_release_q, before_account.sched_release_q);
    assert_eq!(account.pending_present, before_account.pending_present);
    assert_eq!(
        account.pending_remaining_q,
        before_account.pending_remaining_q
    );
    assert_eq!(account.pending_horizon, before_account.pending_horizon);
    assert_eq!(
        account.pending_created_slot,
        before_account.pending_created_slot
    );
}

#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn proof_withdraw_fee_sweep_regression_restores_mutation_cone() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let idx = add_user_test(&mut engine, 0).unwrap();

    let capital = 1_000u16;
    let fee_debt = 100u16;
    let amount = 950u16;

    engine
        .deposit_not_atomic(idx, capital as u128, DEFAULT_SLOT)
        .unwrap();
    engine.accounts[idx as usize].fee_credits = I128::new(-(fee_debt as i128));

    let before = engine.clone();
    let result = engine.withdraw_not_atomic(
        idx,
        amount as u128,
        DEFAULT_ORACLE,
        DEFAULT_SLOT,
        0,
        0,
        100,
        None,
    );

    assert_eq!(result, Err(RiskError::InsufficientBalance));
    assert_withdraw_relevant_state_restored(&engine, &before, idx as usize);
}

#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn proof_withdraw_flat_loss_regression_restores_mutation_cone() {
    let mut engine = RiskEngine::new(zero_fee_params());
    let idx = add_user_test(&mut engine, 0).unwrap();

    let capital = 1_000u16;
    let loss = 100u16;
    let amount = 950u16;

    engine
        .deposit_not_atomic(idx, capital as u128, DEFAULT_SLOT)
        .unwrap();
    engine.set_pnl(idx as usize, -(loss as i128)).unwrap();

    let before = engine.clone();
    let result = engine.withdraw_not_atomic(
        idx,
        amount as u128,
        DEFAULT_ORACLE,
        DEFAULT_SLOT,
        0,
        0,
        100,
        None,
    );

    assert_eq!(result, Err(RiskError::InsufficientBalance));
    assert_withdraw_relevant_state_restored(&engine, &before, idx as usize);
}
