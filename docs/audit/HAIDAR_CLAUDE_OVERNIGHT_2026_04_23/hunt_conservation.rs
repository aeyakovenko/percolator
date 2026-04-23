//! Conservation fuzzing at production scale.
//!
//! Kani explicitly documents that `execute_trade` is NOT end-to-end verified.
//! Neither are liquidate_at_oracle, close_account, force_close_resolved.
//! This test hammers random operation sequences (including the unverified ones)
//! and asserts `V >= C + I` + aggregate consistency after every operation.

#![cfg(feature = "fuzz")]

mod common;

use percolator::*;
use proptest::prelude::*;

const ORACLE_PROD: u64 = 11_494;

fn assert_invariants(engine: &RiskEngine, context: &str) {
    let v = engine.vault.get();
    let c = engine.c_tot.get();
    let i = engine.insurance_fund.balance.get();
    assert!(
        v >= c + i,
        "CONSERVATION BREAK at {}: vault={} c_tot={} insurance={} deficit={}",
        context, v, c, i, (c + i).saturating_sub(v)
    );
    assert!(
        engine.pnl_matured_pos_tot <= engine.pnl_pos_tot,
        "MATURED>POS_TOT at {}: matured={} pos_tot={}",
        context, engine.pnl_matured_pos_tot, engine.pnl_pos_tot
    );

    let mut sum_pos_pnl: u128 = 0;
    let mut sum_capital: u128 = 0;
    let mut cnt_long = 0i64;
    let mut cnt_short = 0i64;
    for idx in 0..engine.accounts.len() {
        let w = idx >> 6;
        let b = idx & 63;
        if w >= engine.used.len() {
            continue;
        }
        let used = ((engine.used[w] >> b) & 1) == 1;
        if !used {
            continue;
        }
        let pnl = engine.accounts[idx].pnl;
        if pnl > 0 {
            sum_pos_pnl = sum_pos_pnl.saturating_add(pnl as u128);
        }
        sum_capital = sum_capital.saturating_add(engine.accounts[idx].capital.get());
        let pos_pnl: u128 = if pnl > 0 { pnl as u128 } else { 0 };
        let r = engine.accounts[idx].reserved_pnl;
        assert!(
            r <= pos_pnl,
            "RESERVE>POS_PNL at {} idx={}: reserved={} pos_pnl={}",
            context, idx, r, pos_pnl
        );
        let pb = engine.accounts[idx].position_basis_q;
        if pb > 0 { cnt_long += 1; }
        else if pb < 0 { cnt_short += 1; }
    }
    assert_eq!(
        sum_pos_pnl, engine.pnl_pos_tot,
        "PNL_POS_TOT DESYNC at {}: sum={} aggregate={}",
        context, sum_pos_pnl, engine.pnl_pos_tot
    );
    assert_eq!(
        sum_capital, engine.c_tot.get(),
        "C_TOT DESYNC at {}: sum={} aggregate={}",
        context, sum_capital, engine.c_tot.get()
    );
    assert_eq!(
        cnt_long, engine.stored_pos_count_long as i64,
        "stored_pos_count_long DESYNC at {}: sum={} aggregate={}",
        context, cnt_long, engine.stored_pos_count_long
    );
    assert_eq!(
        cnt_short, engine.stored_pos_count_short as i64,
        "stored_pos_count_short DESYNC at {}: sum={} aggregate={}",
        context, cnt_short, engine.stored_pos_count_short
    );
}

fn make_engine() -> RiskEngine {
    RiskEngine::new(common::default_params())
}

#[derive(Debug, Clone)]
enum Op {
    AddUser,
    Deposit { idx: u16, amount: u64 },
    Trade { a: u16, b: u16, size: i64, exec_price: u64 },
    Withdraw { idx: u16, amount: u64 },
    TopUpInsurance { amount: u64 },
    AdvanceSlot { dt: u16 },
    SettleAccount { idx: u16 },
    CloseAccount { idx: u16 },
    Liquidate { idx: u16 },
}

fn any_op() -> impl Strategy<Value = Op> {
    prop_oneof![
        3 => Just(Op::AddUser),
        5 => (0u16..32u16, 1u64..=100_000_000u64).prop_map(|(idx, amount)| Op::Deposit { idx, amount }),
        8 => (0u16..32u16, 0u16..32u16, -1_000_000_000i64..=1_000_000_000i64,
              (ORACLE_PROD.saturating_sub(100))..=(ORACLE_PROD + 100))
            .prop_map(|(a, b, size, exec_price)| Op::Trade { a, b, size, exec_price }),
        3 => (0u16..32u16, 1u64..=100_000_000u64).prop_map(|(idx, amount)| Op::Withdraw { idx, amount }),
        2 => (1u64..=10_000_000u64).prop_map(|amount| Op::TopUpInsurance { amount }),
        3 => (1u16..=500u16).prop_map(|dt| Op::AdvanceSlot { dt }),
        2 => (0u16..32u16).prop_map(|idx| Op::SettleAccount { idx }),
        1 => (0u16..32u16).prop_map(|idx| Op::CloseAccount { idx }),
        2 => (0u16..32u16).prop_map(|idx| Op::Liquidate { idx }),
    ]
}

fn apply_op(engine: &mut RiskEngine, op: &Op, slot: &mut u64, oracle: u64) {
    let h_min = engine.params.h_min;
    let h_max = engine.params.h_max;
    match op {
        Op::AddUser => { let _ = common::add_user_test(engine, 0); }
        Op::Deposit { idx, amount } => {
            let _ = engine.deposit_not_atomic(*idx, *amount as u128, oracle, *slot);
        }
        Op::Trade { a, b, size, exec_price } => {
            if a != b && *size > 0 && *exec_price > 0 {
                let _ = engine.execute_trade_not_atomic(
                    *a, *b, oracle, *slot, *size as i128, *exec_price, 0, h_min, h_max,
                );
            }
        }
        Op::Withdraw { idx, amount } => {
            let _ = engine.withdraw_not_atomic(
                *idx, *amount as u128, oracle, *slot, 0, h_min, h_max);
        }
        Op::TopUpInsurance { amount } => {
            let _ = engine.top_up_insurance_fund(*amount as u128, *slot);
        }
        Op::AdvanceSlot { dt } => {
            *slot = slot.saturating_add(*dt as u64);
        }
        Op::SettleAccount { idx } => {
            let _ = engine.settle_account_not_atomic(*idx, oracle, *slot, 0, h_min, h_max);
        }
        Op::CloseAccount { idx } => {
            let _ = engine.close_account_not_atomic(*idx, *slot, oracle, 0, h_min, h_max);
        }
        Op::Liquidate { idx } => {
            let _ = engine.liquidate_at_oracle_not_atomic(
                *idx, oracle, *slot, LiquidationPolicy::FullClose, 0, h_min, h_max);
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50_000))]

    #[test]
    fn fuzz_conservation_random(ops in prop::collection::vec(any_op(), 1..=300)) {
        let mut engine = make_engine();
        let mut slot: u64 = 100;
        assert_invariants(&engine, "initial");
        for (step, op) in ops.iter().enumerate() {
            apply_op(&mut engine, op, &mut slot, ORACLE_PROD);
            let ctx = format!("step {} op {:?}", step, op);
            assert_invariants(&engine, &ctx);
        }
    }
}

// ============================================================================
// Targeted adversarial scenarios
// ============================================================================

/// "Self-trade triangle": attacker controls 2 accounts; can they extract value?
proptest! {
    #![proptest_config(ProptestConfig::with_cases(20_000))]

    #[test]
    fn fuzz_self_trade_triangle(
        deposit_a in 10_000_000u64..=1_000_000_000u64,
        deposit_b in 10_000_000u64..=1_000_000_000u64,
        n_trades in 1u16..=50u16,
        trade_size in 100_000i64..=10_000_000i64,
        band_offset in -100i64..=100i64, // +/- 1% of oracle
        slot_advance in 1u16..=1000u16,
    ) {
        let mut engine = make_engine();
        let slot0: u64 = 100;
        let a = common::add_user_test(&mut engine, 0).unwrap();
        let b = common::add_user_test(&mut engine, 0).unwrap();

        let _ = engine.deposit_not_atomic(a, deposit_a as u128, ORACLE_PROD, slot0);
        let _ = engine.deposit_not_atomic(b, deposit_b as u128, ORACLE_PROD, slot0);

        let total_deposited = deposit_a as u128 + deposit_b as u128;
        assert_invariants(&engine, "after deposits");

        let exec_price = if band_offset >= 0 {
            ORACLE_PROD.saturating_add(band_offset as u64)
        } else {
            ORACLE_PROD.saturating_sub((-band_offset) as u64)
        };
        if exec_price == 0 { return Ok(()); }

        let h_min = engine.params.h_min;
        let h_max = engine.params.h_max;

        let mut slot = slot0;
        for i in 0..n_trades {
            slot = slot.saturating_add(slot_advance as u64);
            let size = if i % 2 == 0 { trade_size } else { -trade_size };
            let (ta, tb) = if size > 0 { (a, b) } else { (b, a) };
            let _ = engine.execute_trade_not_atomic(
                ta, tb, ORACLE_PROD, slot,
                size.unsigned_abs() as i128,
                exec_price,
                0, h_min, h_max,
            );
            assert_invariants(&engine, &format!("after trade {}", i));
        }

        // Settle and withdraw max possible
        slot = slot.saturating_add(h_max * 2);
        let _ = engine.settle_account_not_atomic(a, ORACLE_PROD, slot, 0, h_min, h_max);
        let _ = engine.settle_account_not_atomic(b, ORACLE_PROD, slot, 0, h_min, h_max);

        let cap_a = engine.accounts[a as usize].capital.get();
        let cap_b = engine.accounts[b as usize].capital.get();
        let total_withdrawable = cap_a + cap_b;

        // Conservation check: attacker cannot have more capital than deposited
        // (aside from matured PnL which is capped by insurance surplus).
        // Strictly: if they could withdraw more than deposited + their matured pnl,
        // that would be the attack. The "+matured PnL" is zero for self-trade since
        // matured PnL is zero-sum between their two accounts.
        //
        // Weaker invariant: total across both accounts <= total deposited,
        // minus total fees paid (which went to insurance).
        assert!(
            total_withdrawable <= total_deposited,
            "SELF-TRADE EXTRACTION: withdrawable={} > deposited={}",
            total_withdrawable, total_deposited
        );
        assert_invariants(&engine, "final");
    }
}

/// Deposit, trade, withdraw loop — simulate a user extracting PnL over time.
proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]

    #[test]
    fn fuzz_deposit_trade_withdraw_loop(
        initial_ins in 100_000_000u64..=10_000_000_000u64,
        deposit_u in 10_000_000u64..=1_000_000_000u64,
        deposit_l in 100_000_000u64..=10_000_000_000u64,
        trade_size in 1_000_000i64..=100_000_000i64,
        n_cycles in 1u16..=20u16,
        slots_between in 1u16..=200u16,
        band_offset in -100i64..=100i64,
    ) {
        let mut engine = make_engine();
        let mut slot: u64 = 100;
        let _ = engine.top_up_insurance_fund(initial_ins as u128, slot);
        let u = common::add_user_test(&mut engine, 0).unwrap();
        let lp = common::add_user_test(&mut engine, 0).unwrap();
        let _ = engine.deposit_not_atomic(u, deposit_u as u128, ORACLE_PROD, slot);
        let _ = engine.deposit_not_atomic(lp, deposit_l as u128, ORACLE_PROD, slot);

        let starting_attacker_total = deposit_u as u128 + deposit_l as u128;
        let initial_v = engine.vault.get();
        let initial_i = engine.insurance_fund.balance.get();

        assert_invariants(&engine, "init");

        let exec_price = if band_offset >= 0 {
            ORACLE_PROD.saturating_add(band_offset as u64)
        } else {
            ORACLE_PROD.saturating_sub((-band_offset) as u64)
        };

        let h_min = engine.params.h_min;
        let h_max = engine.params.h_max;

        for cycle in 0..n_cycles {
            slot = slot.saturating_add(slots_between as u64);
            let _ = engine.execute_trade_not_atomic(
                u, lp, ORACLE_PROD, slot,
                trade_size as i128, exec_price, 0, h_min, h_max,
            );
            assert_invariants(&engine, &format!("cycle {} after trade", cycle));

            // Advance to mature warmup
            slot = slot.saturating_add(h_max * 2);
            let _ = engine.settle_account_not_atomic(u, ORACLE_PROD, slot, 0, h_min, h_max);
            let _ = engine.settle_account_not_atomic(lp, ORACLE_PROD, slot, 0, h_min, h_max);
            assert_invariants(&engine, &format!("cycle {} after settle", cycle));

            // Try to withdraw from u
            let cap_u = engine.accounts[u as usize].capital.get();
            if cap_u > 0 {
                let try_withdraw = cap_u / 2;
                if try_withdraw > 0 {
                    let _ = engine.withdraw_not_atomic(
                        u, try_withdraw, ORACLE_PROD, slot, 0, h_min, h_max);
                    assert_invariants(&engine, &format!("cycle {} after withdraw", cycle));
                }
            }
        }

        // After all cycles, close out positions and settle everything
        slot = slot.saturating_add(h_max * 10);
        for _ in 0..5 {
            let _ = engine.settle_account_not_atomic(u, ORACLE_PROD, slot, 0, h_min, h_max);
            let _ = engine.settle_account_not_atomic(lp, ORACLE_PROD, slot, 0, h_min, h_max);
            slot = slot.saturating_add(h_max);
        }

        let final_cap_u = engine.accounts[u as usize].capital.get();
        let final_cap_lp = engine.accounts[lp as usize].capital.get();
        let final_ins = engine.insurance_fund.balance.get();
        let final_v = engine.vault.get();

        let attacker_total = final_cap_u + final_cap_lp;
        // The vault hasn't been drained yet (no withdraws happened final),
        // but attacker's max extractable = final_cap_u + final_cap_lp.
        //
        // For a self-trade: attacker_total must be <= starting_attacker_total - fees_paid.
        // Fees went to insurance: final_ins >= initial_i. If attacker_total > starting - (final_ins - initial_i),
        // that's value creation from nowhere.
        let ins_growth = final_ins.saturating_sub(initial_i);
        let max_allowed_attacker = starting_attacker_total.saturating_sub(ins_growth);

        // Small margin for rounding: allow attacker to have up to starting_deposit
        // (i.e. not prove they lost, but they must not have GAINED beyond rounding)
        assert!(
            attacker_total <= starting_attacker_total + 2, // +2 for rounding slack
            "ATTACKER EXTRACTED VALUE: attacker_total={} > starting={}+slack",
            attacker_total, starting_attacker_total
        );

        // The stricter check: did they beat the fee cost?
        // (attacker_total > max_allowed_attacker means they extracted from insurance/elsewhere)
        if attacker_total > max_allowed_attacker + 10 { // tolerance for rounding across many ops
            panic!(
                "!!! CONSERVATION BREACH — POSSIBLE 5-SOL EXPLOIT !!!\n\
                 starting_attacker={}, ins_growth={}, max_allowed={}, actual_attacker_total={}\n\
                 final_v={} final_ins={} ",
                starting_attacker_total, ins_growth, max_allowed_attacker,
                attacker_total, final_v, final_ins
            );
        }
    }
}
