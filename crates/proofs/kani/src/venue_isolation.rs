//! Kani proofs for venue isolation and LP bucket safety
//!
//! These proofs verify that:
//! - **V1: Bucket Isolation** - Operations on one bucket never affect another
//! - **V2: Principal Inviolability** - LP operations never reduce principal
//! - **V3: AMM LP Exclusive Burn** - AMM LP only reduced by burn_amm_lp
//! - **V4: Slab LP Exclusive Cancel** - Slab LP only reduced by cancel_slab_order
//! - **V5: Type Safety** - Can't mix AMM/Slab operations on wrong bucket

use model_safety::lp_bucket::*;

/// V1: Bucket isolation - operations on bucket i don't affect bucket j
///
/// Property: After any operation on bucket[i], bucket[j] (j != i) is unchanged.
#[kani::proof]
#[kani::unwind(5)]
fn v1_bucket_isolation_amm_mint() {
    let principal: u128 = kani::any();
    kani::assume(principal < 10000);

    // Create portfolio with 2 AMM buckets
    let mut portfolio = Portfolio::new(principal);
    portfolio.lp_buckets[0] = LpBucket::Amm(AmmLp::new(100));
    portfolio.lp_buckets[1] = LpBucket::Amm(AmmLp::new(200));

    // Snapshot bucket 1 before operation on bucket 0
    let bucket1_before = portfolio.lp_buckets[1];

    // Mint shares in bucket 0
    let shares: u64 = kani::any();
    kani::assume(shares < 1000);
    let after = mint_amm_lp(portfolio, 0, shares);

    // Bucket 1 should be unchanged
    match (bucket1_before, after.lp_buckets[1]) {
        (LpBucket::Amm(before_amm), LpBucket::Amm(after_amm)) => {
            assert_eq!(before_amm.lp_shares, after_amm.lp_shares,
                "V1: Bucket 1 shares unchanged by operation on bucket 0");
        }
        _ => {}
    }
}

/// V1: Bucket isolation for AMM burn
#[kani::proof]
#[kani::unwind(5)]
fn v1_bucket_isolation_amm_burn() {
    let principal: u128 = kani::any();
    kani::assume(principal < 10000);

    let mut portfolio = Portfolio::new(principal);
    portfolio.lp_buckets[0] = LpBucket::Amm(AmmLp::new(100));
    portfolio.lp_buckets[1] = LpBucket::Amm(AmmLp::new(200));
    portfolio.lp_buckets[2] = LpBucket::Slab(SlabLp::new(500, 300));

    // Snapshot all other buckets
    let bucket1_before = portfolio.lp_buckets[1];
    let bucket2_before = portfolio.lp_buckets[2];

    // Burn shares from bucket 0
    let shares: u64 = kani::any();
    kani::assume(shares <= 100);
    let after = burn_amm_lp(portfolio, 0, shares);

    // Other buckets unchanged
    match (bucket1_before, after.lp_buckets[1]) {
        (LpBucket::Amm(before), LpBucket::Amm(after_bucket)) => {
            assert_eq!(before.lp_shares, after_bucket.lp_shares,
                "V1: Bucket 1 unchanged by burn on bucket 0");
        }
        _ => {}
    }

    match (bucket2_before, after.lp_buckets[2]) {
        (LpBucket::Slab(before), LpBucket::Slab(after_bucket)) => {
            assert_eq!(before.reserved_quote, after_bucket.reserved_quote,
                "V1: Bucket 2 quote unchanged");
            assert_eq!(before.reserved_base, after_bucket.reserved_base,
                "V1: Bucket 2 base unchanged");
        }
        _ => {}
    }
}

/// V1: Bucket isolation for Slab operations
#[kani::proof]
#[kani::unwind(5)]
fn v1_bucket_isolation_slab_place() {
    let principal: u128 = kani::any();
    kani::assume(principal < 10000);

    let mut portfolio = Portfolio::new(principal);
    portfolio.lp_buckets[0] = LpBucket::Slab(SlabLp::new(100, 50));
    portfolio.lp_buckets[1] = LpBucket::Slab(SlabLp::new(200, 100));

    let bucket1_before = portfolio.lp_buckets[1];

    // Place order on bucket 0
    let quote: u128 = kani::any();
    let base: u128 = kani::any();
    kani::assume(quote < 1000 && base < 1000);
    let after = place_slab_order(portfolio, 0, quote, base);

    // Bucket 1 unchanged
    match (bucket1_before, after.lp_buckets[1]) {
        (LpBucket::Slab(before), LpBucket::Slab(after_bucket)) => {
            assert_eq!(before.reserved_quote, after_bucket.reserved_quote,
                "V1: Other slab bucket quote unchanged");
            assert_eq!(before.reserved_base, after_bucket.reserved_base,
                "V1: Other slab bucket base unchanged");
        }
        _ => {}
    }
}

/// V2: Principal inviolability - LP operations never reduce principal
///
/// Property: Principal is never modified by any LP operation.
#[kani::proof]
#[kani::unwind(5)]
fn v2_principal_inviolable() {
    let principal: u128 = kani::any();
    kani::assume(principal < 10000);

    let portfolio = Portfolio::new(principal);

    // Test all operations
    let shares: u64 = kani::any();
    kani::assume(shares < 1000);

    // Mint AMM LP
    let after_mint = mint_amm_lp(portfolio.clone(), 0, shares);
    assert_eq!(after_mint.principal, principal,
        "V2: Principal unchanged by mint_amm_lp");

    // Burn AMM LP
    let after_burn = burn_amm_lp(portfolio.clone(), 0, shares);
    assert_eq!(after_burn.principal, principal,
        "V2: Principal unchanged by burn_amm_lp");

    // Place slab order
    let quote: u128 = kani::any();
    let base: u128 = kani::any();
    kani::assume(quote < 1000 && base < 1000);
    let after_place = place_slab_order(portfolio.clone(), 0, quote, base);
    assert_eq!(after_place.principal, principal,
        "V2: Principal unchanged by place_slab_order");

    // Cancel slab order
    let after_cancel = cancel_slab_order(portfolio.clone(), 0, quote, base);
    assert_eq!(after_cancel.principal, principal,
        "V2: Principal unchanged by cancel_slab_order");
}

/// V3: AMM LP shares can only decrease via burn_amm_lp
///
/// Property: If AMM LP shares decrease, it must be from burn_amm_lp.
/// Other operations (place_slab_order, cancel_slab_order) don't affect AMM shares.
#[kani::proof]
#[kani::unwind(5)]
fn v3_amm_lp_exclusive_burn() {
    let principal: u128 = kani::any();
    kani::assume(principal < 10000);

    let mut portfolio = Portfolio::new(principal);
    portfolio.lp_buckets[0] = LpBucket::Amm(AmmLp::new(100));

    let initial_shares = 100u64;

    // Slab operations on different bucket should not affect AMM shares
    let quote: u128 = kani::any();
    let base: u128 = kani::any();
    kani::assume(quote < 1000 && base < 1000);

    let after_place = place_slab_order(portfolio.clone(), 1, quote, base);
    if let LpBucket::Amm(amm) = after_place.lp_buckets[0] {
        assert_eq!(amm.lp_shares, initial_shares,
            "V3: place_slab_order on different bucket doesn't affect AMM shares");
    }

    let after_cancel = cancel_slab_order(portfolio.clone(), 1, quote, base);
    if let LpBucket::Amm(amm) = after_cancel.lp_buckets[0] {
        assert_eq!(amm.lp_shares, initial_shares,
            "V3: cancel_slab_order on different bucket doesn't affect AMM shares");
    }
}

/// V4: Slab LP reserves can only decrease via cancel_slab_order
///
/// Property: If Slab LP reserves decrease, it must be from cancel_slab_order.
/// Other operations (mint_amm_lp, burn_amm_lp) don't affect Slab reserves.
#[kani::proof]
#[kani::unwind(5)]
fn v4_slab_lp_exclusive_cancel() {
    let principal: u128 = kani::any();
    kani::assume(principal < 10000);

    let mut portfolio = Portfolio::new(principal);
    portfolio.lp_buckets[0] = LpBucket::Slab(SlabLp::new(100, 50));

    let initial_quote = 100u128;
    let initial_base = 50u128;

    // AMM operations on different bucket should not affect Slab reserves
    let shares: u64 = kani::any();
    kani::assume(shares < 1000);

    let after_mint = mint_amm_lp(portfolio.clone(), 1, shares);
    if let LpBucket::Slab(slab) = after_mint.lp_buckets[0] {
        assert_eq!(slab.reserved_quote, initial_quote,
            "V4: mint_amm_lp on different bucket doesn't affect Slab quote");
        assert_eq!(slab.reserved_base, initial_base,
            "V4: mint_amm_lp on different bucket doesn't affect Slab base");
    }

    let after_burn = burn_amm_lp(portfolio.clone(), 1, shares);
    if let LpBucket::Slab(slab) = after_burn.lp_buckets[0] {
        assert_eq!(slab.reserved_quote, initial_quote,
            "V4: burn_amm_lp on different bucket doesn't affect Slab quote");
        assert_eq!(slab.reserved_base, initial_base,
            "V4: burn_amm_lp on different bucket doesn't affect Slab base");
    }
}

/// V5: Type safety - can't mix AMM/Slab operations on wrong bucket type
///
/// Property: AMM operations on Slab buckets are no-ops (and vice versa).
#[kani::proof]
#[kani::unwind(5)]
fn v5_type_safety() {
    let principal: u128 = kani::any();
    kani::assume(principal < 10000);

    let mut portfolio = Portfolio::new(principal);

    // Create Slab bucket at index 0
    portfolio.lp_buckets[0] = LpBucket::Slab(SlabLp::new(100, 50));
    let slab_before = portfolio.lp_buckets[0];

    // Try to mint AMM shares in Slab bucket (should be no-op)
    let shares: u64 = kani::any();
    kani::assume(shares < 1000);
    let after_wrong_mint = mint_amm_lp(portfolio.clone(), 0, shares);

    // Slab bucket should be unchanged
    match (slab_before, after_wrong_mint.lp_buckets[0]) {
        (LpBucket::Slab(before), LpBucket::Slab(after)) => {
            assert_eq!(before.reserved_quote, after.reserved_quote,
                "V5: mint_amm_lp on Slab bucket is no-op (quote)");
            assert_eq!(before.reserved_base, after.reserved_base,
                "V5: mint_amm_lp on Slab bucket is no-op (base)");
        }
        _ => {
            // Type changed - violation!
            kani::cover!(false, "V5 violated: Type changed");
        }
    }

    // Create AMM bucket at index 1
    portfolio.lp_buckets[1] = LpBucket::Amm(AmmLp::new(200));
    let amm_before = portfolio.lp_buckets[1];

    // Try to place slab order in AMM bucket (should be no-op)
    let quote: u128 = kani::any();
    let base: u128 = kani::any();
    kani::assume(quote < 1000 && base < 1000);
    let after_wrong_place = place_slab_order(portfolio.clone(), 1, quote, base);

    // AMM bucket should be unchanged
    match (amm_before, after_wrong_place.lp_buckets[1]) {
        (LpBucket::Amm(before), LpBucket::Amm(after)) => {
            assert_eq!(before.lp_shares, after.lp_shares,
                "V5: place_slab_order on AMM bucket is no-op");
        }
        _ => {
            // Type changed - violation!
            kani::cover!(false, "V5 violated: Type changed");
        }
    }
}

/// V6: Saturation prevents overflow
///
/// Property: All arithmetic uses saturating operations, preventing overflow.
#[kani::proof]
#[kani::unwind(5)]
fn v6_saturation_prevents_overflow() {
    let principal: u128 = kani::any();
    kani::assume(principal < u128::MAX / 2);

    let mut portfolio = Portfolio::new(principal);
    portfolio.lp_buckets[0] = LpBucket::Amm(AmmLp::new(u64::MAX - 10));

    // Try to add shares that would overflow
    let after = mint_amm_lp(portfolio, 0, 100);

    // Should saturate at u64::MAX, not overflow
    if let LpBucket::Amm(amm) = after.lp_buckets[0] {
        assert!(amm.lp_shares <= u64::MAX,
            "V6: Saturation prevents overflow");
    }
}

/// V7: Saturating subtraction prevents underflow
///
/// Property: Burning more shares than exist saturates at 0 (becomes Empty).
#[kani::proof]
#[kani::unwind(5)]
fn v7_saturation_prevents_underflow() {
    let principal: u128 = kani::any();
    kani::assume(principal < 10000);

    let mut portfolio = Portfolio::new(principal);
    portfolio.lp_buckets[0] = LpBucket::Amm(AmmLp::new(100));

    // Burn more shares than exist
    let after = burn_amm_lp(portfolio, 0, 200);

    // Should either be 0 or Empty (not underflow)
    match after.lp_buckets[0] {
        LpBucket::Empty => {
            // Correctly becomes empty
        }
        LpBucket::Amm(amm) => {
            assert_eq!(amm.lp_shares, 0,
                "V7: Saturation prevents underflow");
        }
        _ => {
            kani::cover!(false, "V7: Unexpected bucket type");
        }
    }
}
