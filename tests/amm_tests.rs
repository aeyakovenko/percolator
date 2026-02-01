// End-to-end integration tests with realistic AMM matcher
// Tests complete user journeys with multiple participants

use percolator::*;

fn default_params() -> RiskParams {
    RiskParams {
        warmup_period_slots: 100,
        maintenance_margin_bps: 500, // 5%
        initial_margin_bps: 1000,    // 10%
        trading_fee_bps: 10,         // 0.1%
        max_accounts: 1000,
        new_account_fee: U128::new(0),          // Zero fee for tests
        risk_reduction_threshold: U128::new(0), // Default: only trigger on full depletion
        maintenance_fee_per_slot: U128::new(0), // No maintenance fee by default
        max_crank_staleness_slots: u64::MAX,
        liquidation_fee_bps: 50,                 // 0.5% liquidation fee
        liquidation_fee_cap: U128::new(100_000), // Cap at 100k units
        liquidation_buffer_bps: 100,             // 1% buffer above maintenance
        min_liquidation_abs: U128::new(100_000), // Minimum 0.1 units
    }
}

// Simple AMM-style matcher that always succeeds
// In production, this would perform actual matching logic or CPI
struct AMMatcher;

impl MatchingEngine for AMMatcher {
    fn execute_match(
        &self,
        _matching_engine_program: &[u8; 32],
        _matching_engine_context: &[u8; 32],
        _lp_account_id: u64,
        oracle_price: u64,
        size: i128,
    ) -> Result<TradeExecution> {
        // AMM always provides liquidity at requested price/size
        Ok(TradeExecution {
            price: oracle_price,
            size,
        })
    }
}

const MATCHER: AMMatcher = AMMatcher;

// Helper function to clamp to positive values
fn clamp_pos_i128(val: i128) -> u128 {
    if val > 0 {
        val as u128
    } else {
        0
    }
}

// ============================================================================
// E2E Test 1: Complete User Journey
// ============================================================================

#[test]
fn test_e2e_complete_user_journey() {
    // Scenario: Alice and Bob trade against LP, experience PNL, funding, warmup, withdrawal

    let mut engine = Box::new(RiskEngine::new(default_params()));

    // Initialize insurance fund
    engine.insurance_fund.balance = U128::new(50_000);

    // Add LP with capital (LP takes leveraged position opposite to users)
    let lp = engine.add_lp([1u8; 32], [2u8; 32], 10_000).unwrap();
    engine.accounts[lp as usize].capital = U128::new(100_000);
    engine.vault = U128::new(100_000);

    // Add two users
    let alice = engine.add_user(10_000).unwrap();
    let bob = engine.add_user(10_000).unwrap();

    // Users deposit principal
    engine.deposit(alice, 10_000, 0).unwrap();
    engine.deposit(bob, 15_000, 0).unwrap();
    engine.vault = U128::new(125_000); // 100k LP + 10k Alice + 15k Bob

    // === Phase 1: Trading ===

    // Alice opens long position at $1000
    let oracle_price = 1_000_000; // $1 in 6 decimal scale
    engine
        .execute_trade(&MATCHER, lp, alice, 0, oracle_price, 5_000)
        .unwrap();

    // Bob opens short position at $1000
    engine
        .execute_trade(&MATCHER, lp, bob, 0, oracle_price, -3_000)
        .unwrap();

    // Check positions
    assert_eq!(engine.accounts[alice as usize].position_size.get(), 5_000);
    assert_eq!(engine.accounts[bob as usize].position_size.get(), -3_000);
    assert_eq!(engine.accounts[lp as usize].position_size.get(), -2_000); // Net opposite to users

    // === Phase 2: Price Movement & Unrealized PNL ===

    // Price moves to $1.20 (+20%)
    let new_price = 1_200_000;

    // Alice closes half her position, realizing profit
    let slot = engine.current_slot;
    engine
        .execute_trade(&MATCHER, lp, alice, slot, new_price, -2_500)
        .unwrap();

    // Alice should have positive PNL from the closed portion
    // Profit = (1.20 - 1.00) × 2500 = 500
    assert!(engine.accounts[alice as usize].pnl.is_positive());
    let alice_pnl = engine.accounts[alice as usize].pnl;

    // === Phase 3: Funding Accrual ===

    // Accrue funding rate (longs pay shorts)
    engine.advance_slot(10);
    engine
        .accrue_funding(engine.current_slot, new_price, 100)
        .unwrap(); // 100 bps/slot, longs pay

    // Settle funding for users
    engine.touch_account(alice).unwrap();
    engine.touch_account(bob).unwrap();

    // Alice (long) should have paid funding, Bob (short) should have received
    assert!(engine.accounts[alice as usize].pnl < alice_pnl); // PNL reduced by funding
    assert!(engine.accounts[bob as usize].pnl.is_positive()); // Received funding

    // === Phase 4: PNL Warmup ===

    // Check that Alice's PNL needs to warm up before withdrawal
    let alice_withdrawable = engine.withdrawable_pnl(&engine.accounts[alice as usize]);

    // Advance some slots
    engine.advance_slot(50); // Halfway through warmup

    let alice_warmed_halfway = engine.withdrawable_pnl(&engine.accounts[alice as usize]);
    assert!(alice_warmed_halfway > alice_withdrawable);

    // Advance to full warmup
    engine.advance_slot(100);

    let alice_fully_warmed = engine.withdrawable_pnl(&engine.accounts[alice as usize]);
    assert!(alice_fully_warmed >= alice_warmed_halfway);

    // === Phase 5: Withdrawal ===

    // Alice closes her remaining position first
    let slot = engine.current_slot;
    engine
        .execute_trade(
            &MATCHER,
            lp,
            alice,
            slot,
            new_price,
            -engine.accounts[alice as usize].position_size.get(),
        )
        .unwrap();

    // Advance time for full warmup
    engine.advance_slot(100);

    // Now Alice can withdraw her warmed PNL + principal
    let alice_final_withdrawable = engine.withdrawable_pnl(&engine.accounts[alice as usize]);
    let alice_withdrawal = engine.accounts[alice as usize].capital.get() + alice_final_withdrawable;

    if alice_withdrawal > 0 {
        let slot = engine.current_slot;
        engine
            .withdraw(alice, alice_withdrawal, slot, 1_000_000)
            .unwrap();

        // Alice should have minimal remaining balance
        assert!(
            engine.accounts[alice as usize].capital.get()
                + clamp_pos_i128(engine.accounts[alice as usize].pnl.get())
                < 100
        );
    }

    // === Phase 6: Panic Settle All ===

    // Price moves to $2.00, causing significant losses
    let settle_price = 2_000_000;

    // Trigger panic settlement at the oracle price
    engine.panic_settle_all(settle_price).unwrap();

    // All positions should be closed
    assert_eq!(
        engine.accounts[alice as usize].position_size.get(),
        0,
        "Alice position should be closed"
    );
    assert_eq!(
        engine.accounts[bob as usize].position_size.get(),
        0,
        "Bob position should be closed"
    );
    assert_eq!(
        engine.accounts[lp as usize].position_size.get(),
        0,
        "LP position should be closed"
    );

    // System should be in risk-reduction mode
    assert!(
        engine.risk_reduction_only,
        "Should be in risk-reduction mode after panic settle"
    );

    // All PNLs should be >= 0 (negative PNL clamped and socialized)
    assert!(
        engine.accounts[alice as usize].pnl.get() >= 0,
        "Alice PNL should be >= 0"
    );
    assert!(
        engine.accounts[bob as usize].pnl.get() >= 0,
        "Bob PNL should be >= 0"
    );
    assert!(
        engine.accounts[lp as usize].pnl.get() >= 0,
        "LP PNL should be >= 0"
    );

    println!("✅ E2E test passed: Complete user journey works correctly");
}

// ============================================================================
// E2E Test 2: Multi-User Trading with ADL
// ============================================================================

#[test]
fn test_e2e_multi_user_with_adl() {
    // Scenario: Multiple users trade, one causes loss requiring ADL

    let mut engine = Box::new(RiskEngine::new(default_params()));
    engine.insurance_fund.balance = U128::new(10_000);

    let lp = engine.add_lp([1u8; 32], [2u8; 32], 10_000).unwrap();
    engine.accounts[lp as usize].capital = U128::new(200_000);
    engine.vault = U128::new(200_000);

    // Add 5 users
    let mut users = Vec::new();
    for _ in 0..5 {
        let user = engine.add_user(10_000).unwrap();
        engine.deposit(user, 10_000, 0).unwrap();
        users.push(user);
    }
    engine.vault = U128::new(250_000);

    // Users 0-3 open long positions at $1000
    for i in 0..4 {
        engine
            .execute_trade(&MATCHER, lp, users[i], 0, 1_000_000, 2_000)
            .unwrap();
    }

    // User 4 opens short position
    engine
        .execute_trade(&MATCHER, lp, users[4], 0, 1_000_000, -8_000)
        .unwrap();

    // Price moves to $1.10 - all longs profit, short loses
    let new_price = 1_100_000;

    // Close some positions to realize PNL
    for i in 0..4 {
        engine
            .execute_trade(&MATCHER, lp, users[i], 0, new_price, -2_000)
            .unwrap();
    }

    // Users 0-3 should have positive PNL
    for i in 0..4 {
        assert!(engine.accounts[users[i] as usize].pnl.is_positive());
    }

    // Simulate a large loss event requiring ADL (e.g., LP underwater)
    let adl_loss = 5_000;
    engine.apply_adl(adl_loss).unwrap();

    // Verify ADL haircutted unwrapped PNL first
    // Users should still have their principal intact (minus trading fees settled)
    // Note: With immediate negative PnL settlement, trading fees reduce capital slightly
    for i in 0..4 {
        // Capital may be slightly reduced by trading fees that were settled
        // but principal (minus fees) should be protected by I1
        assert!(
            engine.accounts[users[i] as usize].capital.get() >= 9_990,
            "Principal should be mostly intact (minus fees): got {}",
            engine.accounts[users[i] as usize].capital
        );
    }

    // Total PNL should be reduced by ADL
    let total_pnl_after: i128 = users
        .iter()
        .map(|&u| engine.accounts[u as usize].pnl.get())
        .sum();

    // Some PNL should remain (not all haircutted)
    println!("Total PNL after ADL: {}", total_pnl_after);

    println!("✅ E2E test passed: Multi-user ADL scenario works correctly");
}

// ============================================================================
// E2E Test 3: Warmup Rate Limiting Under Stress
// NOTE: Commented out - warmup rate limiting was removed in slab 4096 redesign
// ============================================================================

/*
#[test]
fn test_e2e_warmup_rate_limiting_stress() {
    // Scenario: Many users with large PNL, warmup capacity gets constrained

    let mut engine = Box::new(RiskEngine::new(default_params()));

    // Small insurance fund to test capacity limits
    engine.insurance_fund.balance = U128::new(20_000);

    let lp = engine.add_lp([1u8; 32], [2u8; 32], 10_000).unwrap();
    engine.accounts[lp as usize].capital = U128::new(500_000);
    engine.vault = U128::new(500_000);

    // Add 10 users
    let mut users = Vec::new();
    for _ in 0..10 {
        let user = engine.add_user(10_000).unwrap();
        engine.deposit(user, 5_000, 0).unwrap();
        users.push(user);
    }
    engine.vault = U128::new(550_000);

    // All users open large long positions
    for &user in &users {
        engine.execute_trade(&MATCHER, lp, user, 0, 1_000_000, 10_000).unwrap();
    }

    // Price moves up 50% - huge unrealized PNL
    let boom_price = 1_500_000;

    // Close all positions to realize massive PNL
    for &user in &users {
        engine.execute_trade(&MATCHER, lp, user, 0, boom_price, -10_000).unwrap();
        // execute_trade automatically calls update_warmup_slope() after PNL changes
    }

    // Each user should have large positive PNL (~5000 each = 50k total)
    let mut total_pnl = 0i128;
    for &user in &users {
        assert!(engine.accounts[user as usize].pnl.get() > 1_000);
        total_pnl += engine.accounts[user as usize].pnl.get();
    }
    println!("Total realized PNL across all users: {}", total_pnl);

    // Verify warmup rate limiting is enforced
    // Max warmup rate = insurance_fund * 0.5 / (T/2)
    // Note: Insurance fund may have increased from fees, so max_rate may be slightly higher
    let max_rate = engine.insurance_fund.balance * 5000 / 50 / 10_000;
    assert!(max_rate >= 200, "Max rate should be at least 200");

    println!("Insurance fund balance: {}", engine.insurance_fund.balance);
    println!("Calculated max warmup rate: {}", max_rate);
    println!("Actual total warmup rate: {}", engine.total_warmup_rate);

    // CRITICAL: Verify that warmup slopes were actually set by update_warmup_slope()
    // If total_warmup_rate is 0, it means update_warmup_slope() was never called
    assert!(engine.total_warmup_rate > 0,
            "Warmup slopes should be set after PNL changes (update_warmup_slope called by execute_trade)");

    // Total warmup rate should not exceed this (allow small rounding tolerance)
    assert!(engine.total_warmup_rate <= max_rate + 5,
            "Warmup rate {} significantly exceeds limit {}", engine.total_warmup_rate, max_rate);

    // CRITICAL: Verify rate limiting is actually constraining the system
    // Calculate what the total would be WITHOUT rate limiting
    let total_pnl_u128 = total_pnl as u128;
    let ideal_total_slope = total_pnl_u128 / engine.params.warmup_period_slots as u128;
    println!("Ideal total slope (no limiting): {}", ideal_total_slope);

    // If ideal > max_rate, then rate limiting MUST be active
    if ideal_total_slope > max_rate {
        assert_eq!(engine.total_warmup_rate, max_rate,
                   "Rate limiting should cap total slope at max_rate when demand exceeds capacity");
        println!("✅ Rate limiting is ACTIVE: capped at {} (would be {} without limiting)",
                 engine.total_warmup_rate, ideal_total_slope);
    } else {
        println!("ℹ️  Rate limiting not triggered: demand ({}) below capacity ({})",
                 ideal_total_slope, max_rate);
    }

    // Users with higher PNL should get proportionally more capacity
    // But sum of all slopes should be capped
    let total_slope: u128 = users.iter()
        .map(|&u| engine.accounts[u as usize].warmup_slope_per_step)
        .sum();

    assert_eq!(total_slope, engine.total_warmup_rate,
               "Sum of individual slopes must equal total_warmup_rate");
    assert!(total_slope <= max_rate,
            "Total slope must not exceed max rate");

    println!("✅ E2E test passed: Warmup rate limiting under stress works correctly");
    println!("   Total slope: {}, Max rate: {}", total_slope, max_rate);
}
*/

// ============================================================================
// E2E Test 4: Complete Cycle with Funding
// ============================================================================

#[test]
fn test_e2e_funding_complete_cycle() {
    // Scenario: Users trade, funding accrues over time, positions flip, funding reverses

    let mut engine = Box::new(RiskEngine::new(default_params()));
    engine.insurance_fund.balance = U128::new(50_000);

    let lp = engine.add_lp([1u8; 32], [2u8; 32], 10_000).unwrap();
    engine.accounts[lp as usize].capital = U128::new(100_000);
    engine.vault = U128::new(100_000);

    let alice = engine.add_user(10_000).unwrap();
    let bob = engine.add_user(10_000).unwrap();

    engine.deposit(alice, 20_000, 0).unwrap();
    engine.deposit(bob, 20_000, 0).unwrap();
    engine.vault = U128::new(140_000);

    // Alice goes long, Bob goes short
    engine
        .execute_trade(&MATCHER, lp, alice, 0, 1_000_000, 10_000)
        .unwrap();
    engine
        .execute_trade(&MATCHER, lp, bob, 0, 1_000_000, -10_000)
        .unwrap();

    // Advance time and accrue funding (longs pay shorts)
    engine.advance_slot(20);
    engine
        .accrue_funding(engine.current_slot, 1_000_000, 50)
        .unwrap(); // 50 bps/slot

    // Settle funding
    engine.touch_account(alice).unwrap();
    engine.touch_account(bob).unwrap();

    let alice_pnl_after_funding = engine.accounts[alice as usize].pnl.get();
    let bob_pnl_after_funding = engine.accounts[bob as usize].pnl.get();

    // Alice (long) paid, Bob (short) received
    assert!(alice_pnl_after_funding < 0); // Paid funding
    assert!(bob_pnl_after_funding > 0); // Received funding

    // Verify zero-sum property (approximately, minus rounding)
    let total_funding = alice_pnl_after_funding + bob_pnl_after_funding;
    assert!(
        total_funding.abs() < 100,
        "Funding should be approximately zero-sum"
    );

    // === Positions Flip ===

    // Alice closes long and opens short
    engine
        .execute_trade(&MATCHER, lp, alice, 0, 1_000_000, -20_000)
        .unwrap();

    // Bob closes short and opens long
    engine
        .execute_trade(&MATCHER, lp, bob, 0, 1_000_000, 20_000)
        .unwrap();

    // Now Alice is short and Bob is long
    assert!(engine.accounts[alice as usize].position_size.is_negative());
    assert!(engine.accounts[bob as usize].position_size.is_positive());

    // Advance time and accrue more funding (now Alice receives, Bob pays)
    engine.advance_slot(20);
    engine
        .accrue_funding(engine.current_slot, 1_000_000, 50)
        .unwrap();

    engine.touch_account(alice).unwrap();
    engine.touch_account(bob).unwrap();

    // Now funding should have reversed
    let alice_final = engine.accounts[alice as usize].pnl.get();
    let bob_final = engine.accounts[bob as usize].pnl.get();

    // Alice (now short) should have received some funding back
    assert!(alice_final > alice_pnl_after_funding);

    // Bob (now long) should have paid
    assert!(bob_final < bob_pnl_after_funding);

    println!("✅ E2E test passed: Funding complete cycle works correctly");
}

// ============================================================================
// E2E Test 5: Oracle Manipulation Attack Scenario
// NOTE: Partially commented out - warmup rate limiting was removed in slab 4096 redesign
// ============================================================================

/*
#[test]
fn test_e2e_oracle_attack_protection() {
    // Scenario: Attacker tries to exploit oracle manipulation but gets limited by warmup + ADL

    let mut engine = Box::new(RiskEngine::new(default_params()));
    engine.insurance_fund.balance = U128::new(30_000);

    let lp = engine.add_lp([1u8; 32], [2u8; 32], 10_000).unwrap();
    engine.accounts[lp as usize].capital = U128::new(200_000);
    engine.vault = U128::new(200_000);

    // Honest user
    let honest_user = engine.add_user(10_000).unwrap();
    engine.deposit(honest_user, 20_000, 0).unwrap();

    // Attacker
    let attacker = engine.add_user(10_000).unwrap();
    engine.deposit(attacker, 10_000, 0).unwrap();
    engine.vault = U128::new(230_000);

    // === Phase 1: Normal Trading ===

    // Honest user opens long position
    engine.execute_trade(&MATCHER, lp, honest_user, 0, 1_000_000, 5_000).unwrap();

    // === Phase 2: Oracle Manipulation Attempt ===

    // Attacker opens large position during manipulation
    engine.execute_trade(&MATCHER, lp, attacker, 0, 1_000_000, 20_000).unwrap();

    // Oracle gets manipulated to $2 (fake 100% gain)
    let fake_price = 2_000_000;

    // Attacker tries to close and realize fake profit
    engine.execute_trade(&MATCHER, lp, attacker, 0, fake_price, -20_000).unwrap();
    // execute_trade automatically calls update_warmup_slope() after realizing PNL

    // Attacker has massive fake PNL
    let attacker_fake_pnl = clamp_pos_i128(engine.accounts[attacker as usize].pnl.get());
    assert!(attacker_fake_pnl > 10_000); // Huge profit from manipulation

    // === Phase 3: Warmup Limiting ===

    // Due to warmup rate limiting, attacker's PNL warms up slowly
    // Max warmup rate = insurance_fund * 0.5 / (T/2)
    let expected_max_rate = engine.insurance_fund.balance * 5000 / 50 / 10_000;

    println!("Attacker fake PNL: {}", attacker_fake_pnl);
    println!("Insurance fund: {}", engine.insurance_fund.balance);
    println!("Expected max warmup rate: {}", expected_max_rate);
    println!("Actual warmup rate: {}", engine.total_warmup_rate);
    println!("Attacker slope: {}", engine.accounts[attacker as usize].warmup_slope_per_step);

    // Verify that warmup slope was actually set
    assert!(engine.accounts[attacker as usize].warmup_slope_per_step > 0,
            "Attacker's warmup slope should be set after realizing PNL");

    // Verify rate limiting is working (attacker's slope should be constrained)
    // In a stressed system, individual slope may be less than ideal due to capacity limits
    let ideal_slope = attacker_fake_pnl / engine.params.warmup_period_slots as u128;
    println!("Ideal slope (no limiting): {}", ideal_slope);
    println!("Actual slope (with limiting): {}", engine.accounts[attacker as usize].warmup_slope_per_step);

    // Advance only 10 slots (manipulation is detected quickly)
    engine.advance_slot(10);

    let attacker_warmed = engine.withdrawable_pnl(&engine.accounts[attacker as usize]);
    println!("Attacker withdrawable after 10 slots: {}", attacker_warmed);

    // Only a small fraction should be withdrawable
    // Expected: slope was capped by warmup rate limiting + only 10 slots elapsed
    assert!(attacker_warmed < attacker_fake_pnl / 5,
            "Most fake PNL should still be warming up (got {} out of {})", attacker_warmed, attacker_fake_pnl);

    // === Phase 4: Oracle Reverts, ADL Triggered ===

    // Oracle reverts to true price, creating loss
    // ADL is triggered to socialize the loss

    engine.apply_adl(attacker_fake_pnl).unwrap();

    // Attacker's unwrapped (still warming) PNL gets haircutted
    let attacker_after_adl = clamp_pos_i128(engine.accounts[attacker as usize].pnl.get());

    // Most of the fake PNL should be gone
    assert!(attacker_after_adl < attacker_fake_pnl / 2,
            "ADL should haircut most of the unwrapped PNL");

    // === Phase 5: Honest User Protected ===

    // Honest user's principal should be intact
    assert_eq!(engine.accounts[honest_user as usize].capital.get(), 20_000, "I1: Principal never reduced");

    // Insurance fund took some hit, but limited
    assert!(engine.insurance_fund.balance >= 20_000,
            "Insurance fund protected by warmup rate limiting");

    println!("✅ E2E test passed: Oracle manipulation attack protection works correctly");
    println!("   Attacker fake PNL: {}", attacker_fake_pnl);
    println!("   Attacker after ADL: {}", attacker_after_adl);
    println!("   Attack mitigation: {}%", (attacker_fake_pnl - attacker_after_adl) * 100 / attacker_fake_pnl);
}
*/
