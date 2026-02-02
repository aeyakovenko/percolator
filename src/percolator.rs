//! Formally Verified Risk Engine for Perpetual DEX
//!
//! ⚠️ EDUCATIONAL USE ONLY - NOT PRODUCTION READY ⚠️
//!
//! This is an experimental research project for educational purposes only.
//! DO NOT use with real funds. Not independently audited. Not production ready.
//!
//! This module implements a formally verified risk engine that guarantees:
//! 1. User funds are safe against oracle manipulation attacks (within time window T)
//! 2. PNL warmup prevents instant withdrawal of manipulated profits
//! 3. ADL haircuts apply to unwrapped PNL first, protecting user principal
//! 4. Conservation of funds across all operations
//! 5. User isolation - one user's actions don't affect others
//!
//! All data structures are laid out in a single contiguous memory chunk,
//! suitable for a single Solana account.

#![no_std]
#![forbid(unsafe_code)]

#[cfg(kani)]
extern crate kani;

// ============================================================================
// Constants
// ============================================================================

// MAX_ACCOUNTS is feature-configured, not target-configured.
// This ensures x86 and SBF builds use the same sizes for a given feature set.
#[cfg(kani)]
pub const MAX_ACCOUNTS: usize = 4; // Small for fast formal verification (1 bitmap word, 4 bits)

#[cfg(all(feature = "test", not(kani)))]
pub const MAX_ACCOUNTS: usize = 64; // Small for tests

#[cfg(all(not(kani), not(feature = "test")))]
pub const MAX_ACCOUNTS: usize = 4096; // Production

// Derived constants - all use size_of, no hardcoded values
pub const BITMAP_WORDS: usize = (MAX_ACCOUNTS + 63) / 64;
pub const MAX_ROUNDING_SLACK: u128 = MAX_ACCOUNTS as u128;
/// Mask for wrapping indices (MAX_ACCOUNTS must be power of 2)
const ACCOUNT_IDX_MASK: usize = MAX_ACCOUNTS - 1;

/// Maximum number of dust accounts to close per crank call.
/// Limits compute usage while still making progress on cleanup.
pub const GC_CLOSE_BUDGET: u32 = 32;

/// Number of occupied accounts to process per crank call.
/// When the system has fewer than this many accounts, one crank covers everything.
pub const ACCOUNTS_PER_CRANK: u16 = 256;

/// Hard liquidation budget per crank call (caps total work)
/// Set to 120 to keep worst-case crank CU under ~50% of Solana limit
pub const LIQ_BUDGET_PER_CRANK: u16 = 120;

/// Max number of force-realize closes per crank call.
/// Hard CU bound in force-realize mode. Liquidations are skipped when active.
pub const FORCE_REALIZE_BUDGET_PER_CRANK: u16 = 32;

/// Maximum oracle price (prevents overflow in mark_pnl calculations)
/// 10^15 allows prices up to $1B with 6 decimal places
pub const MAX_ORACLE_PRICE: u64 = 1_000_000_000_000_000;

/// Maximum absolute position size (prevents overflow in mark_pnl calculations)
/// 10^20 allows positions up to 100 billion units
/// Combined with MAX_ORACLE_PRICE, guarantees mark_pnl multiply won't overflow i128
pub const MAX_POSITION_ABS: u128 = 100_000_000_000_000_000_000;

// ============================================================================
// BPF-Safe 128-bit Types (see src/i128.rs)
// ============================================================================
pub mod i128;
pub use i128::{I128, U128};

// ============================================================================
// Core Data Structures
// ============================================================================

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccountKind {
    User = 0,
    LP = 1,
}

/// Unified account - can be user or LP
///
/// LPs are distinguished by having kind = LP and matcher_program/context set.
/// Users have kind = User and matcher arrays zeroed.
///
/// This unification ensures LPs receive the same risk management protections as users:
/// - PNL warmup
/// - ADL (Auto-Deleveraging)
/// - Liquidations
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Account {
    /// Unique account ID (monotonically increasing, never recycled)
    /// Note: Field order matches on-chain slab layout (account_id at offset 0)
    pub account_id: u64,

    // ========================================
    // Capital & PNL (universal)
    // ========================================
    /// Deposited capital (user principal or LP capital)
    /// NEVER reduced by ADL/socialization (Invariant I1)
    pub capital: U128,

    /// Account kind (User or LP)
    /// Note: Field is at offset 24 in on-chain layout, after capital
    pub kind: AccountKind,

    /// Realized PNL from trading (can be positive or negative)
    pub pnl: I128,

    /// PNL reserved for pending withdrawals
    /// Note: u64 to match on-chain slab layout (8 bytes, not 16)
    pub reserved_pnl: u64,

    // ========================================
    // Warmup (embedded, no separate struct)
    // ========================================
    /// Slot when warmup started
    pub warmup_started_at_slot: u64,

    /// Linear vesting rate per slot
    pub warmup_slope_per_step: U128,

    // ========================================
    // Position (universal)
    // ========================================
    /// Current position size (+ long, - short)
    pub position_size: I128,

    /// Last oracle mark price at which this account's position was settled (variation margin).
    /// NOT an average trade entry price.
    pub entry_price: u64,

    // ========================================
    // Funding (universal)
    // ========================================
    /// Funding index snapshot (quote per base, 1e6 scale)
    pub funding_index: I128,

    // ========================================
    // LP-specific (only meaningful for LP kind)
    // ========================================
    /// Matching engine program ID (zero for user accounts)
    pub matcher_program: [u8; 32],

    /// Matching engine context account (zero for user accounts)
    pub matcher_context: [u8; 32],

    // ========================================
    // Owner & Maintenance Fees (wrapper-related)
    // ========================================
    /// Owner pubkey (32 bytes, signature checks done by wrapper)
    pub owner: [u8; 32],

    /// Fee credits in capital units (can go negative if fees owed)
    pub fee_credits: I128,

    /// Last slot when maintenance fees were settled for this account
    pub last_fee_slot: u64,

    /// Padding to match on-chain Account size (248 bytes)
    pub _padding: [u8; 8],
}

impl Account {
    /// Check if this account is an LP
    pub fn is_lp(&self) -> bool {
        matches!(self.kind, AccountKind::LP)
    }

    /// Check if this account is a regular user
    pub fn is_user(&self) -> bool {
        matches!(self.kind, AccountKind::User)
    }
}

/// Helper to create empty account
fn empty_account() -> Account {
    Account {
        account_id: 0,
        capital: U128::ZERO,
        kind: AccountKind::User,
        pnl: I128::ZERO,
        reserved_pnl: 0,
        warmup_started_at_slot: 0,
        warmup_slope_per_step: U128::ZERO,
        position_size: I128::ZERO,
        entry_price: 0,
        funding_index: I128::ZERO,
        matcher_program: [0; 32],
        matcher_context: [0; 32],
        owner: [0; 32],
        fee_credits: I128::ZERO,
        last_fee_slot: 0,
        _padding: [0; 8],
    }
}

/// Insurance fund state
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InsuranceFund {
    /// Insurance fund balance
    pub balance: U128,

    /// Accumulated fees from trades
    pub fee_revenue: U128,
}

/// Outcome from oracle_close_position_core helper
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClosedOutcome {
    /// Absolute position size that was closed
    pub abs_pos: u128,
    /// Mark PnL from closing at oracle price
    pub mark_pnl: i128,
    /// Capital before settlement
    pub cap_before: u128,
    /// Capital after settlement
    pub cap_after: u128,
    /// Whether a position was actually closed
    pub position_was_closed: bool,
}

/// Risk engine parameters
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RiskParams {
    /// Warmup period in slots (time T)
    pub warmup_period_slots: u64,

    /// Maintenance margin ratio in basis points (e.g., 500 = 5%)
    pub maintenance_margin_bps: u64,

    /// Initial margin ratio in basis points
    pub initial_margin_bps: u64,

    /// Trading fee in basis points
    pub trading_fee_bps: u64,

    /// Maximum number of accounts
    pub max_accounts: u64,

    /// Flat account creation fee (absolute amount in capital units)
    pub new_account_fee: U128,

    /// Insurance fund threshold for entering risk-reduction-only mode
    /// If insurance fund balance drops below this, risk-reduction mode activates
    pub risk_reduction_threshold: U128,

    // ========================================
    // Maintenance Fee Parameters
    // ========================================
    /// Maintenance fee per account per slot (in capital units)
    /// Engine is purely slot-native; any per-day conversion is wrapper/UI responsibility
    pub maintenance_fee_per_slot: U128,

    /// Maximum allowed staleness before crank is required (in slots)
    /// Set to u64::MAX to disable crank freshness check
    pub max_crank_staleness_slots: u64,

    /// Liquidation fee in basis points (e.g., 50 = 0.50%)
    /// Paid from liquidated account's capital into insurance fund
    pub liquidation_fee_bps: u64,

    /// Absolute cap on liquidation fee (in capital units)
    /// Prevents whales paying enormous fees
    pub liquidation_fee_cap: U128,

    // ========================================
    // Partial Liquidation Parameters
    // ========================================
    /// Buffer above maintenance margin (in basis points) to target after partial liquidation.
    /// E.g., if maintenance is 500 bps (5%) and buffer is 100 bps (1%), we target 6% margin.
    /// This prevents immediate re-liquidation from small price movements.
    pub liquidation_buffer_bps: u64,

    /// Minimum absolute position size after partial liquidation.
    /// If remaining position would be below this threshold, full liquidation occurs.
    /// Prevents dust positions that are uneconomical to maintain or re-liquidate.
    /// Denominated in base units (same scale as position_size.abs()).
    pub min_liquidation_abs: U128,
}

/// Main risk engine state - fixed slab with bitmap
#[repr(C)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RiskEngine {
    /// Total vault balance (all deposited funds)
    pub vault: U128,

    /// Insurance fund
    pub insurance_fund: InsuranceFund,

    /// Risk parameters
    pub params: RiskParams,

    /// Current slot (for warmup calculations)
    pub current_slot: u64,

    /// Global funding index (quote per 1 base, scaled by 1e6)
    pub funding_index_qpb_e6: I128,

    /// Last slot when funding was accrued
    pub last_funding_slot: u64,

    // ========================================
    // Keeper Crank Tracking
    // ========================================
    /// Last slot when keeper crank was executed
    pub last_crank_slot: u64,

    /// Maximum allowed staleness before crank is required (in slots)
    pub max_crank_staleness_slots: u64,

    // ========================================
    // Open Interest Tracking (O(1))
    // ========================================
    /// Total open interest = sum of abs(position_size) across all accounts
    /// This measures total risk exposure in the system.
    pub total_open_interest: U128,

    // ========================================
    // O(1) Aggregates (spec §2.2, §4)
    // ========================================
    /// Sum of all account capital: C_tot = Σ C_i
    /// Maintained incrementally via set_capital() helper.
    pub c_tot: U128,

    /// Sum of all positive PnL: PNL_pos_tot = Σ max(PNL_i, 0)
    /// Maintained incrementally via set_pnl() helper.
    pub pnl_pos_tot: U128,

    // ========================================
    // Crank Cursors (bounded scan support)
    // ========================================
    /// Cursor for liquidation scan (wraps around MAX_ACCOUNTS)
    pub liq_cursor: u16,

    /// Cursor for garbage collection scan (wraps around MAX_ACCOUNTS)
    pub gc_cursor: u16,

    /// Slot when the current full sweep started (step 0 was executed)
    pub last_full_sweep_start_slot: u64,

    /// Slot when the last full sweep completed
    pub last_full_sweep_completed_slot: u64,

    /// Cursor: index where the next crank will start scanning
    pub crank_cursor: u16,

    /// Index where the current sweep started (for completion detection)
    pub sweep_start_idx: u16,

    // ========================================
    // Lifetime Counters (telemetry)
    // ========================================
    /// Total number of liquidations performed (lifetime)
    pub lifetime_liquidations: u64,

    /// Total number of force-realize closes performed (lifetime)
    pub lifetime_force_realize_closes: u64,

    // ========================================
    // LP Aggregates (O(1) maintained for funding/threshold)
    // ========================================
    /// Net LP position: sum of position_size across all LP accounts
    /// Updated incrementally in execute_trade and close paths
    pub net_lp_pos: I128,

    /// Sum of abs(position_size) across all LP accounts
    /// Updated incrementally in execute_trade and close paths
    pub lp_sum_abs: U128,

    /// Max abs(position_size) across all LP accounts (monotone upper bound)
    /// Only increases; reset via bounded sweep at sweep completion
    pub lp_max_abs: U128,

    /// In-progress max abs for current sweep (reset at sweep start, committed at completion)
    pub lp_max_abs_sweep: U128,

    // ========================================
    // Slab Management
    // ========================================
    /// Occupancy bitmap (4096 bits = 64 u64 words)
    pub used: [u64; BITMAP_WORDS],

    /// Number of used accounts (O(1) counter, fixes H2: fee bypass TOCTOU)
    pub num_used_accounts: u16,

    /// Next account ID to assign (monotonically increasing, never recycled)
    pub next_account_id: u64,

    /// Freelist head (u16::MAX = none)
    pub free_head: u16,

    /// Padding to align accounts with on-chain slab layout.
    /// The original Account struct had `kind` at offset 0 and `account_id` at offset 8.
    /// After reordering Account fields (account_id first), we need this padding
    /// to maintain backward compatibility with existing on-chain data.
    /// On-chain accounts are at engine offset 95256, not 95248.
    pub _padding_accounts: [u8; 8],

    /// Freelist next pointers
    pub next_free: [u16; MAX_ACCOUNTS],

    /// Account slab (4096 accounts)
    pub accounts: [Account; MAX_ACCOUNTS],
}

// ============================================================================
// Error Types
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RiskError {
    /// Insufficient balance for operation
    InsufficientBalance,

    /// Account would become undercollateralized
    Undercollateralized,

    /// Unauthorized operation
    Unauthorized,

    /// Invalid matching engine
    InvalidMatchingEngine,

    /// PNL not yet warmed up
    PnlNotWarmedUp,

    /// Arithmetic overflow
    Overflow,

    /// Account not found
    AccountNotFound,

    /// Account is not an LP account
    NotAnLPAccount,

    /// Position size mismatch
    PositionSizeMismatch,

    /// Account kind mismatch
    AccountKindMismatch,
}

pub type Result<T> = core::result::Result<T, RiskError>;

/// Outcome of a keeper crank operation
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CrankOutcome {
    /// Whether the crank successfully advanced last_crank_slot
    pub advanced: bool,
    /// Slots forgiven for caller's maintenance (50% discount via time forgiveness)
    pub slots_forgiven: u64,
    /// Whether caller's maintenance fee settle succeeded (false if undercollateralized)
    pub caller_settle_ok: bool,
    /// Whether force-realize mode is active (insurance at/below threshold)
    pub force_realize_needed: bool,
    /// Whether panic_settle_all should be called (system in stress)
    pub panic_needed: bool,
    /// Number of accounts liquidated during this crank
    pub num_liquidations: u32,
    /// Number of liquidation errors (triggers risk_reduction_only)
    pub num_liq_errors: u16,
    /// Number of dust accounts garbage collected during this crank
    pub num_gc_closed: u32,
    /// Number of positions force-closed during this crank (when force_realize_needed)
    pub force_realize_closed: u16,
    /// Number of force-realize errors during this crank
    pub force_realize_errors: u16,
    /// Index where this crank stopped (next crank continues from here)
    pub last_cursor: u16,
    /// Whether this crank completed a full sweep of all accounts
    pub sweep_complete: bool,
}

// ============================================================================
// Math Helpers (Saturating Arithmetic for Safety)
// ============================================================================

#[inline]
fn add_u128(a: u128, b: u128) -> u128 {
    a.saturating_add(b)
}

#[inline]
fn sub_u128(a: u128, b: u128) -> u128 {
    a.saturating_sub(b)
}

#[inline]
fn mul_u128(a: u128, b: u128) -> u128 {
    a.saturating_mul(b)
}

#[inline]
fn div_u128(a: u128, b: u128) -> Result<u128> {
    if b == 0 {
        Err(RiskError::Overflow) // Division by zero
    } else {
        Ok(a / b)
    }
}

#[inline]
fn clamp_pos_i128(val: i128) -> u128 {
    if val > 0 {
        val as u128
    } else {
        0
    }
}

#[allow(dead_code)]
#[inline]
fn clamp_neg_i128(val: i128) -> u128 {
    if val < 0 {
        neg_i128_to_u128(val)
    } else {
        0
    }
}

/// Saturating absolute value for i128 (handles i128::MIN without overflow)
#[inline]
fn saturating_abs_i128(val: i128) -> i128 {
    if val == i128::MIN {
        i128::MAX
    } else {
        val.abs()
    }
}

/// Safely convert negative i128 to u128 (handles i128::MIN without overflow)
///
/// For i128::MIN, -i128::MIN would overflow because i128::MAX + 1 cannot be represented.
/// We handle this by returning (i128::MAX as u128) + 1 = 170141183460469231731687303715884105728.
#[inline]
fn neg_i128_to_u128(val: i128) -> u128 {
    debug_assert!(val < 0, "neg_i128_to_u128 called with non-negative value");
    if val == i128::MIN {
        (i128::MAX as u128) + 1
    } else {
        (-val) as u128
    }
}

/// Safely convert u128 to i128 with clamping (handles values > i128::MAX)
///
/// If x > i128::MAX, the cast would wrap to a negative value.
/// We clamp to i128::MAX instead to preserve correctness of margin checks.
#[inline]
fn u128_to_i128_clamped(x: u128) -> i128 {
    if x > i128::MAX as u128 {
        i128::MAX
    } else {
        x as i128
    }
}

// ============================================================================
// Matching Engine Trait
// ============================================================================

/// Result of a successful trade execution from the matching engine
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TradeExecution {
    /// Actual execution price (may differ from oracle/requested price)
    pub price: u64,
    /// Actual executed size (may be partial fill)
    pub size: i128,
}

/// Trait for pluggable matching engines
///
/// Implementers can provide custom order matching logic via CPI.
/// The matching engine is responsible for validating and executing trades
/// according to its own rules (CLOB, AMM, RFQ, etc).
pub trait MatchingEngine {
    /// Execute a trade between LP and user
    ///
    /// # Arguments
    /// * `lp_program` - The LP's matching engine program ID
    /// * `lp_context` - The LP's matching engine context account
    /// * `lp_account_id` - Unique ID of the LP account (never recycled)
    /// * `oracle_price` - Current oracle price for reference
    /// * `size` - Requested position size (positive = long, negative = short)
    ///
    /// # Returns
    /// * `Ok(TradeExecution)` with actual executed price and size
    /// * `Err(RiskError)` if the trade is rejected
    ///
    /// # Safety
    /// The matching engine MUST verify user authorization before approving trades.
    /// The risk engine will check solvency after the trade executes.
    fn execute_match(
        &self,
        lp_program: &[u8; 32],
        lp_context: &[u8; 32],
        lp_account_id: u64,
        oracle_price: u64,
        size: i128,
    ) -> Result<TradeExecution>;
}

/// No-op matching engine (for testing)
/// Returns the requested price and size as-is
pub struct NoOpMatcher;

impl MatchingEngine for NoOpMatcher {
    fn execute_match(
        &self,
        _lp_program: &[u8; 32],
        _lp_context: &[u8; 32],
        _lp_account_id: u64,
        oracle_price: u64,
        size: i128,
    ) -> Result<TradeExecution> {
        // Return requested price/size unchanged (no actual matching logic)
        Ok(TradeExecution {
            price: oracle_price,
            size,
        })
    }
}

// ============================================================================
// Core Implementation
// ============================================================================

impl RiskEngine {
    /// Create a new risk engine (stack-allocates the full struct - avoid in BPF!)
    ///
    /// WARNING: This allocates ~6MB on the stack at MAX_ACCOUNTS=4096.
    /// For Solana BPF programs, use `init_in_place` instead.
    pub fn new(params: RiskParams) -> Self {
        let mut engine = Self {
            vault: U128::ZERO,
            insurance_fund: InsuranceFund {
                balance: U128::ZERO,
                fee_revenue: U128::ZERO,
            },
            params,
            current_slot: 0,
            funding_index_qpb_e6: I128::ZERO,
            last_funding_slot: 0,
            last_crank_slot: 0,
            max_crank_staleness_slots: params.max_crank_staleness_slots,
            total_open_interest: U128::ZERO,
            c_tot: U128::ZERO,
            pnl_pos_tot: U128::ZERO,
            liq_cursor: 0,
            gc_cursor: 0,
            last_full_sweep_start_slot: 0,
            last_full_sweep_completed_slot: 0,
            crank_cursor: 0,
            sweep_start_idx: 0,
            lifetime_liquidations: 0,
            lifetime_force_realize_closes: 0,
            net_lp_pos: I128::ZERO,
            lp_sum_abs: U128::ZERO,
            lp_max_abs: U128::ZERO,
            lp_max_abs_sweep: U128::ZERO,
            used: [0; BITMAP_WORDS],
            num_used_accounts: 0,
            next_account_id: 0,
            free_head: 0,
            _padding_accounts: [0; 8],
            next_free: [0; MAX_ACCOUNTS],
            accounts: [empty_account(); MAX_ACCOUNTS],
        };

        // Initialize freelist: 0 -> 1 -> 2 -> ... -> 4095 -> NONE
        for i in 0..MAX_ACCOUNTS - 1 {
            engine.next_free[i] = (i + 1) as u16;
        }
        engine.next_free[MAX_ACCOUNTS - 1] = u16::MAX; // Sentinel

        engine
    }

    /// Initialize a RiskEngine in place (zero-copy friendly).
    ///
    /// PREREQUISITE: The memory backing `self` MUST be zeroed before calling.
    /// This method only sets non-zero fields to avoid touching the entire ~6MB struct.
    ///
    /// This is the correct way to initialize RiskEngine in Solana BPF programs
    /// where stack space is limited to 4KB.
    pub fn init_in_place(&mut self, params: RiskParams) {
        // Set params (non-zero field)
        self.params = params;
        self.max_crank_staleness_slots = params.max_crank_staleness_slots;

        // Initialize freelist: 0 -> 1 -> 2 -> ... -> MAX_ACCOUNTS-1 -> NONE
        // All other fields are zero which is correct for:
        // - vault, insurance_fund, current_slot, funding_index, etc. = 0
        // - used bitmap = all zeros (no accounts in use)
        // - accounts = all zeros (equivalent to empty_account())
        // - free_head = 0 (first free slot is 0)
        for i in 0..MAX_ACCOUNTS - 1 {
            self.next_free[i] = (i + 1) as u16;
        }
        self.next_free[MAX_ACCOUNTS - 1] = u16::MAX; // Sentinel
    }

    // ========================================
    // Bitmap Helpers
    // ========================================

    pub fn is_used(&self, idx: usize) -> bool {
        if idx >= MAX_ACCOUNTS {
            return false;
        }
        let w = idx >> 6;
        let b = idx & 63;
        ((self.used[w] >> b) & 1) == 1
    }

    fn set_used(&mut self, idx: usize) {
        let w = idx >> 6;
        let b = idx & 63;
        self.used[w] |= 1u64 << b;
    }

    fn clear_used(&mut self, idx: usize) {
        let w = idx >> 6;
        let b = idx & 63;
        self.used[w] &= !(1u64 << b);
    }

    fn for_each_used_mut<F: FnMut(usize, &mut Account)>(&mut self, mut f: F) {
        for (block, word) in self.used.iter().copied().enumerate() {
            let mut w = word;
            while w != 0 {
                let bit = w.trailing_zeros() as usize;
                let idx = block * 64 + bit;
                w &= w - 1; // Clear lowest bit
                if idx >= MAX_ACCOUNTS {
                    continue; // Guard against stray high bits in bitmap
                }
                f(idx, &mut self.accounts[idx]);
            }
        }
    }

    fn for_each_used<F: FnMut(usize, &Account)>(&self, mut f: F) {
        for (block, word) in self.used.iter().copied().enumerate() {
            let mut w = word;
            while w != 0 {
                let bit = w.trailing_zeros() as usize;
                let idx = block * 64 + bit;
                w &= w - 1; // Clear lowest bit
                if idx >= MAX_ACCOUNTS {
                    continue; // Guard against stray high bits in bitmap
                }
                f(idx, &self.accounts[idx]);
            }
        }
    }

    // ========================================
    // O(1) Aggregate Helpers (spec §4)
    // ========================================

    /// Mandatory helper: set account PnL and maintain pnl_pos_tot aggregate (spec §4.2).
    /// All code paths that modify PnL MUST call this.
    #[inline]
    pub fn set_pnl(&mut self, idx: usize, new_pnl: i128) {
        let old = self.accounts[idx].pnl.get();
        let old_pos = if old > 0 { old as u128 } else { 0 };
        let new_pos = if new_pnl > 0 { new_pnl as u128 } else { 0 };
        self.pnl_pos_tot = U128::new(
            self.pnl_pos_tot
                .get()
                .saturating_add(new_pos)
                .saturating_sub(old_pos),
        );
        self.accounts[idx].pnl = I128::new(new_pnl);
    }

    /// Helper: set account capital and maintain c_tot aggregate (spec §4.1).
    #[inline]
    pub fn set_capital(&mut self, idx: usize, new_capital: u128) {
        let old = self.accounts[idx].capital.get();
        if new_capital >= old {
            self.c_tot = U128::new(self.c_tot.get().saturating_add(new_capital - old));
        } else {
            self.c_tot = U128::new(self.c_tot.get().saturating_sub(old - new_capital));
        }
        self.accounts[idx].capital = U128::new(new_capital);
    }

    /// Recompute c_tot and pnl_pos_tot from account data. For test use after direct state mutation.
    pub fn recompute_aggregates(&mut self) {
        let mut c_tot = 0u128;
        let mut pnl_pos_tot = 0u128;
        self.for_each_used(|_idx, account| {
            c_tot = c_tot.saturating_add(account.capital.get());
            let pnl = account.pnl.get();
            if pnl > 0 {
                pnl_pos_tot = pnl_pos_tot.saturating_add(pnl as u128);
            }
        });
        self.c_tot = U128::new(c_tot);
        self.pnl_pos_tot = U128::new(pnl_pos_tot);
    }

    /// Compute haircut ratio (h_num, h_den) per spec §3.2.
    /// h = min(Residual, PNL_pos_tot) / PNL_pos_tot where Residual = max(0, V - C_tot - I).
    /// Returns (1, 1) when PNL_pos_tot == 0.
    #[inline]
    pub fn haircut_ratio(&self) -> (u128, u128) {
        let pnl_pos_tot = self.pnl_pos_tot.get();
        if pnl_pos_tot == 0 {
            return (1, 1);
        }
        let residual = self
            .vault
            .get()
            .saturating_sub(self.c_tot.get())
            .saturating_sub(self.insurance_fund.balance.get());
        let h_num = core::cmp::min(residual, pnl_pos_tot);
        (h_num, pnl_pos_tot)
    }

    /// Compute effective positive PnL after haircut for a given account PnL (spec §3.3).
    /// PNL_eff_pos_i = floor(max(PNL_i, 0) * h_num / h_den)
    #[inline]
    pub fn effective_pos_pnl(&self, pnl: i128) -> u128 {
        if pnl <= 0 {
            return 0;
        }
        let pos_pnl = pnl as u128;
        let (h_num, h_den) = self.haircut_ratio();
        if h_den == 0 {
            return pos_pnl;
        }
        // floor(pos_pnl * h_num / h_den)
        mul_u128(pos_pnl, h_num) / h_den
    }

    /// Compute effective realized equity per spec §3.3.
    /// Eq_real_i = max(0, C_i + min(PNL_i, 0) + PNL_eff_pos_i)
    #[inline]
    pub fn effective_equity(&self, account: &Account) -> u128 {
        let cap_i = u128_to_i128_clamped(account.capital.get());
        let neg_pnl = core::cmp::min(account.pnl.get(), 0);
        let eff_pos = self.effective_pos_pnl(account.pnl.get());
        let eq_i = cap_i
            .saturating_add(neg_pnl)
            .saturating_add(u128_to_i128_clamped(eff_pos));
        if eq_i > 0 {
            eq_i as u128
        } else {
            0
        }
    }

    // ========================================
    // Account Allocation
    // ========================================

    fn alloc_slot(&mut self) -> Result<u16> {
        if self.free_head == u16::MAX {
            return Err(RiskError::Overflow); // Slab full
        }
        let idx = self.free_head;
        self.free_head = self.next_free[idx as usize];
        self.set_used(idx as usize);
        // Increment O(1) counter atomically (fixes H2: TOCTOU fee bypass)
        self.num_used_accounts = self.num_used_accounts.saturating_add(1);
        Ok(idx)
    }

    /// Count used accounts
    fn count_used(&self) -> u64 {
        let mut count = 0u64;
        self.for_each_used(|_, _| {
            count += 1;
        });
        count
    }

    // ========================================
    // Account Management
    // ========================================

    /// Add a new user account
    pub fn add_user(&mut self, fee_payment: u128) -> Result<u16> {
        // Use O(1) counter instead of O(N) count_used() (fixes H2: TOCTOU fee bypass)
        let used_count = self.num_used_accounts as u64;
        if used_count >= self.params.max_accounts {
            return Err(RiskError::Overflow);
        }

        // Flat fee (no scaling)
        let required_fee = self.params.new_account_fee.get();
        if fee_payment < required_fee {
            return Err(RiskError::InsufficientBalance);
        }

        // Bug #4 fix: Compute excess payment to credit to user capital
        let excess = fee_payment.saturating_sub(required_fee);

        // Pay fee to insurance (fee tokens are deposited into vault)
        // Account for FULL fee_payment in vault, not just required_fee
        self.vault = self.vault + fee_payment;
        self.insurance_fund.balance = self.insurance_fund.balance + required_fee;
        self.insurance_fund.fee_revenue = self.insurance_fund.fee_revenue + required_fee;

        // Allocate slot and assign unique ID
        let idx = self.alloc_slot()?;
        let account_id = self.next_account_id;
        self.next_account_id = self.next_account_id.saturating_add(1);

        // Initialize account with excess credited to capital
        self.accounts[idx as usize] = Account {
            kind: AccountKind::User,
            account_id,
            capital: U128::new(excess), // Bug #4 fix: excess goes to user capital
            pnl: I128::ZERO,
            reserved_pnl: 0,
            warmup_started_at_slot: self.current_slot,
            warmup_slope_per_step: U128::ZERO,
            position_size: I128::ZERO,
            entry_price: 0,
            funding_index: self.funding_index_qpb_e6,
            matcher_program: [0; 32],
            matcher_context: [0; 32],
            owner: [0; 32],
            fee_credits: I128::ZERO,
            last_fee_slot: self.current_slot,
            _padding: [0; 8],
        };

        // Maintain c_tot aggregate (account was created with capital = excess)
        if excess > 0 {
            self.c_tot = U128::new(self.c_tot.get().saturating_add(excess));
        }

        Ok(idx)
    }

    /// Add a new LP account
    pub fn add_lp(
        &mut self,
        matching_engine_program: [u8; 32],
        matching_engine_context: [u8; 32],
        fee_payment: u128,
    ) -> Result<u16> {
        // Use O(1) counter instead of O(N) count_used() (fixes H2: TOCTOU fee bypass)
        let used_count = self.num_used_accounts as u64;
        if used_count >= self.params.max_accounts {
            return Err(RiskError::Overflow);
        }

        // Flat fee (no scaling)
        let required_fee = self.params.new_account_fee.get();
        if fee_payment < required_fee {
            return Err(RiskError::InsufficientBalance);
        }

        // Bug #4 fix: Compute excess payment to credit to LP capital
        let excess = fee_payment.saturating_sub(required_fee);

        // Pay fee to insurance (fee tokens are deposited into vault)
        // Account for FULL fee_payment in vault, not just required_fee
        self.vault = self.vault + fee_payment;
        self.insurance_fund.balance = self.insurance_fund.balance + required_fee;
        self.insurance_fund.fee_revenue = self.insurance_fund.fee_revenue + required_fee;

        // Allocate slot and assign unique ID
        let idx = self.alloc_slot()?;
        let account_id = self.next_account_id;
        self.next_account_id = self.next_account_id.saturating_add(1);

        // Initialize account with excess credited to capital
        self.accounts[idx as usize] = Account {
            kind: AccountKind::LP,
            account_id,
            capital: U128::new(excess), // Bug #4 fix: excess goes to LP capital
            pnl: I128::ZERO,
            reserved_pnl: 0,
            warmup_started_at_slot: self.current_slot,
            warmup_slope_per_step: U128::ZERO,
            position_size: I128::ZERO,
            entry_price: 0,
            funding_index: self.funding_index_qpb_e6,
            matcher_program: matching_engine_program,
            matcher_context: matching_engine_context,
            owner: [0; 32],
            fee_credits: I128::ZERO,
            last_fee_slot: self.current_slot,
            _padding: [0; 8],
        };

        // Maintain c_tot aggregate (account was created with capital = excess)
        if excess > 0 {
            self.c_tot = U128::new(self.c_tot.get().saturating_add(excess));
        }

        Ok(idx)
    }

    // ========================================
    // Maintenance Fees
    // ========================================

    /// Settle maintenance fees for an account.
    ///
    /// Returns the fee amount due (for keeper rebate calculation).
    ///
    /// Algorithm:
    /// 1. Compute dt = now_slot - account.last_fee_slot
    /// 2. If dt == 0, return 0 (no-op)
    /// 3. Compute due = fee_per_slot * dt
    /// 4. Deduct from fee_credits; if negative, pay from capital to insurance
    /// 5. If position exists and below maintenance after fee, return Err
    pub fn settle_maintenance_fee(
        &mut self,
        idx: u16,
        now_slot: u64,
        oracle_price: u64,
    ) -> Result<u128> {
        if idx as usize >= MAX_ACCOUNTS || !self.is_used(idx as usize) {
            return Err(RiskError::Unauthorized);
        }

        let account = &mut self.accounts[idx as usize];

        // Calculate elapsed time
        let dt = now_slot.saturating_sub(account.last_fee_slot);
        if dt == 0 {
            return Ok(0);
        }

        // Calculate fee due (engine is purely slot-native)
        let due = self
            .params
            .maintenance_fee_per_slot
            .get()
            .saturating_mul(dt as u128);

        // Update last_fee_slot
        account.last_fee_slot = now_slot;

        // Deduct from fee_credits (coupon: no insurance booking here —
        // insurance was already paid when credits were granted)
        account.fee_credits = account.fee_credits.saturating_sub(due as i128);

        // If fee_credits is negative, pay from capital
        let mut paid_from_capital = 0u128;
        if account.fee_credits.is_negative() {
            let owed = neg_i128_to_u128(account.fee_credits.get());
            let pay = core::cmp::min(owed, account.capital.get());

            account.capital = account.capital.saturating_sub(pay);
            self.insurance_fund.balance = self.insurance_fund.balance + pay;
            self.insurance_fund.fee_revenue = self.insurance_fund.fee_revenue + pay;
            // Maintain c_tot aggregate
            self.c_tot = U128::new(self.c_tot.get().saturating_sub(pay));

            // Credit back what was paid
            account.fee_credits = account.fee_credits.saturating_add(pay as i128);
            paid_from_capital = pay;
        }

        // Check maintenance margin if account has a position (MTM check)
        if !account.position_size.is_zero() {
            // Re-borrow immutably for margin check
            let account_ref = &self.accounts[idx as usize];
            if !self.is_above_maintenance_margin_mtm(account_ref, oracle_price) {
                return Err(RiskError::Undercollateralized);
            }
        }

        Ok(paid_from_capital) // Return actual amount paid into insurance
    }

    /// Best-effort maintenance settle for crank paths.
    /// - Always advances last_fee_slot
    /// - Charges fees into insurance if possible
    /// - NEVER fails due to margin checks
    /// - Still returns Unauthorized if idx invalid
    fn settle_maintenance_fee_best_effort_for_crank(
        &mut self,
        idx: u16,
        now_slot: u64,
    ) -> Result<u128> {
        if idx as usize >= MAX_ACCOUNTS || !self.is_used(idx as usize) {
            return Err(RiskError::Unauthorized);
        }

        let account = &mut self.accounts[idx as usize];

        let dt = now_slot.saturating_sub(account.last_fee_slot);
        if dt == 0 {
            return Ok(0);
        }

        let due = self
            .params
            .maintenance_fee_per_slot
            .get()
            .saturating_mul(dt as u128);

        // Advance slot marker regardless
        account.last_fee_slot = now_slot;

        // Deduct from fee_credits (coupon: no insurance booking here —
        // insurance was already paid when credits were granted)
        account.fee_credits = account.fee_credits.saturating_sub(due as i128);

        // If negative, pay what we can from capital (no margin check)
        let mut paid_from_capital = 0u128;
        if account.fee_credits.is_negative() {
            let owed = neg_i128_to_u128(account.fee_credits.get());
            let pay = core::cmp::min(owed, account.capital.get());

            account.capital = account.capital.saturating_sub(pay);
            self.insurance_fund.balance = self.insurance_fund.balance + pay;
            self.insurance_fund.fee_revenue = self.insurance_fund.fee_revenue + pay;
            // Maintain c_tot aggregate
            self.c_tot = U128::new(self.c_tot.get().saturating_sub(pay));

            account.fee_credits = account.fee_credits.saturating_add(pay as i128);
            paid_from_capital = pay;
        }

        Ok(paid_from_capital) // Return actual amount paid into insurance
    }

    /// Pay down existing fee debt (negative fee_credits) using available capital.
    /// Does not advance last_fee_slot or charge new fees — just sweeps capital
    /// that became available (e.g. after warmup settlement) into insurance.
    fn pay_fee_debt_from_capital(&mut self, idx: u16) {
        let a = &mut self.accounts[idx as usize];
        if a.fee_credits.is_negative() && !a.capital.is_zero() {
            let owed = neg_i128_to_u128(a.fee_credits.get());
            let pay = core::cmp::min(owed, a.capital.get());
            if pay > 0 {
                a.capital = a.capital.saturating_sub(pay);
                self.insurance_fund.balance = self.insurance_fund.balance + pay;
                self.insurance_fund.fee_revenue = self.insurance_fund.fee_revenue + pay;
                // Maintain c_tot aggregate
                self.c_tot = U128::new(self.c_tot.get().saturating_sub(pay));
                a.fee_credits = a.fee_credits.saturating_add(pay as i128);
            }
        }
    }

    /// Touch account for force-realize paths: settles funding, mark, and fees but
    /// uses best-effort fee settle that can't stall on margin checks.
    fn touch_account_for_force_realize(
        &mut self,
        idx: u16,
        now_slot: u64,
        oracle_price: u64,
    ) -> Result<()> {
        // Funding settle is required for correct pnl
        self.touch_account(idx)?;
        // Mark-to-market settlement (variation margin)
        self.settle_mark_to_oracle(idx, oracle_price)?;
        // Best-effort fees; never fails due to maintenance margin
        let _ = self.settle_maintenance_fee_best_effort_for_crank(idx, now_slot)?;
        Ok(())
    }

    /// Touch account for liquidation paths: settles funding, mark, and fees but
    /// uses best-effort fee settle since we're about to liquidate anyway.
    fn touch_account_for_liquidation(
        &mut self,
        idx: u16,
        now_slot: u64,
        oracle_price: u64,
    ) -> Result<()> {
        // Funding settle is required for correct pnl
        self.touch_account(idx)?;
        // Best-effort mark-to-market (saturating — never wedges on extreme PnL)
        self.settle_mark_to_oracle_best_effort(idx, oracle_price)?;
        // Best-effort fees; margin check would just block the liquidation we need to do
        let _ = self.settle_maintenance_fee_best_effort_for_crank(idx, now_slot)?;
        Ok(())
    }

    /// Set owner pubkey for an account
    pub fn set_owner(&mut self, idx: u16, owner: [u8; 32]) -> Result<()> {
        if idx as usize >= MAX_ACCOUNTS || !self.is_used(idx as usize) {
            return Err(RiskError::Unauthorized);
        }
        self.accounts[idx as usize].owner = owner;
        Ok(())
    }

    /// Pre-fund fee credits for an account.
    ///
    /// The wrapper must have already transferred `amount` tokens into the vault.
    /// This pre-pays future maintenance fees: vault increases, insurance receives
    /// the amount as revenue (since credits are a coupon — spending them later
    /// does NOT re-book into insurance), and the account's fee_credits balance
    /// increases by `amount`.
    pub fn deposit_fee_credits(&mut self, idx: u16, amount: u128, now_slot: u64) -> Result<()> {
        if idx as usize >= MAX_ACCOUNTS || !self.is_used(idx as usize) {
            return Err(RiskError::Unauthorized);
        }
        self.current_slot = now_slot;

        // Wrapper transferred tokens into vault
        self.vault = self.vault + amount;

        // Pre-fund: insurance receives the amount now.
        // When credits are later spent during fee settlement, no further
        // insurance booking occurs (coupon semantics).
        self.insurance_fund.balance = self.insurance_fund.balance + amount;
        self.insurance_fund.fee_revenue = self.insurance_fund.fee_revenue + amount;

        // Credit the account
        self.accounts[idx as usize].fee_credits = self.accounts[idx as usize]
            .fee_credits
            .saturating_add(amount as i128);

        Ok(())
    }

    /// Add fee credits without vault/insurance accounting.
    /// Only for tests and Kani proofs — production code must use deposit_fee_credits.
    #[cfg(any(test, feature = "test", kani))]
    pub fn add_fee_credits(&mut self, idx: u16, amount: u128) -> Result<()> {
        if idx as usize >= MAX_ACCOUNTS || !self.is_used(idx as usize) {
            return Err(RiskError::Unauthorized);
        }
        self.accounts[idx as usize].fee_credits = self.accounts[idx as usize]
            .fee_credits
            .saturating_add(amount as i128);
        Ok(())
    }

    /// Set the risk reduction threshold (admin function).
    /// This controls when risk-reduction-only mode is triggered.
    #[inline]
    pub fn set_risk_reduction_threshold(&mut self, new_threshold: u128) {
        self.params.risk_reduction_threshold = U128::new(new_threshold);
    }

    /// Get the current risk reduction threshold.
    #[inline]
    pub fn risk_reduction_threshold(&self) -> u128 {
        self.params.risk_reduction_threshold.get()
    }

    /// Close an account and return its capital to the caller.
    ///
    /// Requirements:
    /// - Account must exist
    /// - Position must be zero (no open positions)
    /// - fee_credits >= 0 (no outstanding fees owed)
    /// - pnl must be 0 after settlement (positive pnl must be warmed up first)
    ///
    /// Returns Err(PnlNotWarmedUp) if pnl > 0 (user must wait for warmup).
    /// Returns Err(Undercollateralized) if pnl < 0 (shouldn't happen after settlement).
    /// Returns the capital amount on success.
    pub fn close_account(&mut self, idx: u16, now_slot: u64, oracle_price: u64) -> Result<u128> {
        // Update current_slot so warmup/bookkeeping progresses consistently
        self.current_slot = now_slot;

        if idx as usize >= MAX_ACCOUNTS || !self.is_used(idx as usize) {
            return Err(RiskError::AccountNotFound);
        }

        // Full settlement: funding + maintenance fees + warmup
        // This converts warmed pnl to capital and realizes negative pnl
        self.touch_account_full(idx, now_slot, oracle_price)?;

        // Position must be zero
        if !self.accounts[idx as usize].position_size.is_zero() {
            return Err(RiskError::Undercollateralized); // Has open position
        }

        // Forgive any remaining fee debt (Finding C: fee debt traps).
        // pay_fee_debt_from_capital (via touch_account_full above) already paid
        // what it could. Any remainder is uncollectable — forgive and proceed.
        if self.accounts[idx as usize].fee_credits.is_negative() {
            self.accounts[idx as usize].fee_credits = I128::ZERO;
        }

        let account = &self.accounts[idx as usize];

        // PnL must be zero to close. This enforces:
        // 1. Users can't bypass warmup by closing with positive unwarmed pnl
        // 2. Conservation is maintained (forfeiting pnl would create unbounded slack)
        // 3. Negative pnl after full settlement implies insolvency
        if account.pnl.is_positive() {
            return Err(RiskError::PnlNotWarmedUp);
        }
        if account.pnl.is_negative() {
            return Err(RiskError::Undercollateralized);
        }

        let capital = account.capital;

        // Deduct from vault
        if capital > self.vault {
            return Err(RiskError::InsufficientBalance);
        }
        self.vault = self.vault - capital;

        // Decrement c_tot before freeing slot (free_slot zeroes account but doesn't update c_tot)
        self.set_capital(idx as usize, 0);

        // Free the slot
        self.free_slot(idx);

        Ok(capital.get())
    }

    /// Free an account slot (internal helper).
    /// Clears the account, bitmap, and returns slot to freelist.
    /// Caller must ensure the account is safe to free (no capital, no positive pnl, etc).
    fn free_slot(&mut self, idx: u16) {
        self.accounts[idx as usize] = empty_account();
        self.clear_used(idx as usize);
        self.next_free[idx as usize] = self.free_head;
        self.free_head = idx;
        self.num_used_accounts = self.num_used_accounts.saturating_sub(1);
    }

    /// Garbage collect dust accounts.
    ///
    /// A "dust account" is a slot that can never pay out anything:
    /// - position_size == 0
    /// - capital == 0
    /// - reserved_pnl == 0
    /// - pnl <= 0
    ///
    /// Any remaining negative PnL is socialized via ADL waterfall before freeing.
    /// No token transfers occur - this is purely internal bookkeeping cleanup.
    ///
    /// Called at end of keeper_crank after liquidation/settlement has already run.
    ///
    /// Returns the number of accounts closed.
    pub fn garbage_collect_dust(&mut self) -> u32 {
        // Collect dust candidates: accounts with zero position, capital, reserved, and non-positive pnl
        let mut to_free: [u16; GC_CLOSE_BUDGET as usize] = [0; GC_CLOSE_BUDGET as usize];
        let mut num_to_free = 0usize;

        // Scan up to ACCOUNTS_PER_CRANK slots, capped to MAX_ACCOUNTS
        let max_scan = (ACCOUNTS_PER_CRANK as usize).min(MAX_ACCOUNTS);
        let start = self.gc_cursor as usize;

        for offset in 0..max_scan {
            // Budget check
            if num_to_free >= GC_CLOSE_BUDGET as usize {
                break;
            }

            let idx = (start + offset) & ACCOUNT_IDX_MASK;

            // Check if slot is used via bitmap
            let block = idx >> 6;
            let bit = idx & 63;
            if (self.used[block] & (1u64 << bit)) == 0 {
                continue;
            }

            // NEVER garbage collect LP accounts - they are essential for market operation
            if self.accounts[idx].is_lp() {
                continue;
            }

            // Best-effort fee settle so accounts with tiny capital get drained in THIS sweep.
            let _ = self.settle_maintenance_fee_best_effort_for_crank(idx as u16, self.current_slot);

            // Dust predicate: must have zero position, capital, reserved, and non-positive pnl
            {
                let account = &self.accounts[idx];
                if !account.position_size.is_zero() {
                    continue;
                }
                if !account.capital.is_zero() {
                    continue;
                }
                if account.reserved_pnl != 0 {
                    continue;
                }
                if account.pnl.is_positive() {
                    continue;
                }
            }

            // If flat, funding is irrelevant — snap to global so dust can be collected.
            // Position size is already confirmed zero above, so no unsettled funding value.
            if self.accounts[idx].funding_index != self.funding_index_qpb_e6 {
                self.accounts[idx].funding_index = self.funding_index_qpb_e6;
            }

            // Write off negative pnl (spec §6.1: unpayable loss just reduces Residual)
            if self.accounts[idx].pnl.is_negative() {
                self.set_pnl(idx, 0);
            }

            // Queue for freeing
            to_free[num_to_free] = idx as u16;
            num_to_free += 1;
        }

        // Update cursor for next call
        self.gc_cursor = ((start + max_scan) & ACCOUNT_IDX_MASK) as u16;

        // Free all collected dust accounts
        for i in 0..num_to_free {
            self.free_slot(to_free[i]);
        }

        num_to_free as u32
    }

    // ========================================
    // Keeper Crank
    // ========================================

    /// Check if a fresh crank is required before state-changing operations.
    /// Returns Err if the crank is stale (too old).
    pub fn require_fresh_crank(&self, now_slot: u64) -> Result<()> {
        if now_slot.saturating_sub(self.last_crank_slot) > self.max_crank_staleness_slots {
            return Err(RiskError::Unauthorized); // NeedsCrank
        }
        Ok(())
    }

    /// Check if a full sweep started recently.
    /// For risk-increasing ops, we require a sweep to have STARTED recently.
    /// The priority-liquidation phase runs every crank, so once a sweep starts,
    /// the worst accounts are immediately addressed.
    pub fn require_recent_full_sweep(&self, now_slot: u64) -> Result<()> {
        if now_slot.saturating_sub(self.last_full_sweep_start_slot) > self.max_crank_staleness_slots
        {
            return Err(RiskError::Unauthorized); // SweepStale
        }
        Ok(())
    }


    /// Check if force-realize mode is active (insurance at or below threshold).
    /// When active, keeper_crank will run windowed force-realize steps.
    #[inline]
    fn force_realize_active(&self) -> bool {
        self.insurance_fund.balance <= self.params.risk_reduction_threshold
    }

    /// Keeper crank entrypoint - advances global state and performs maintenance.
    ///
    /// Returns CrankOutcome with flags indicating what happened.
    ///
    /// Behavior:
    /// 1. Accrue funding
    /// 2. Advance last_crank_slot if now_slot > last_crank_slot
    /// 3. Settle maintenance fees for caller (50% discount)
    /// 4. Process up to ACCOUNTS_PER_CRANK occupied accounts:
    ///    - Liquidation (if not in force-realize mode)
    ///    - Force-realize (if insurance at/below threshold)
    ///    - Socialization (haircut profits to cover losses)
    ///    - LP max tracking
    /// 5. Detect and finalize full sweep completion
    ///
    /// This is the single permissionless "do-the-right-thing" entrypoint.
    /// - Always attempts caller's maintenance settle with 50% discount (best-effort)
    /// - Only advances last_crank_slot when now_slot > last_crank_slot
    /// - Returns last_cursor: the index where this crank stopped
    /// - Returns sweep_complete: true if this crank completed a full sweep
    ///
    /// When the system has fewer than ACCOUNTS_PER_CRANK accounts, one crank
    /// covers all accounts and completes a full sweep.
    pub fn keeper_crank(
        &mut self,
        caller_idx: u16,
        now_slot: u64,
        oracle_price: u64,
        funding_rate_bps_per_slot: i64,
        allow_panic: bool,
    ) -> Result<CrankOutcome> {
        // Validate oracle price bounds (prevents overflow in mark_pnl calculations)
        if oracle_price == 0 || oracle_price > MAX_ORACLE_PRICE {
            return Err(RiskError::Overflow);
        }

        // Update current_slot so warmup/bookkeeping progresses consistently
        self.current_slot = now_slot;

        // Detect if this is the start of a new sweep
        let starting_new_sweep = self.crank_cursor == self.sweep_start_idx;
        if starting_new_sweep {
            self.last_full_sweep_start_slot = now_slot;
            // Reset in-progress lp_max_abs for fresh sweep
            self.lp_max_abs_sweep = U128::ZERO;
        }

        // Accrue funding first (always) - propagate errors, don't continue with corrupt state
        self.accrue_funding(now_slot, oracle_price, funding_rate_bps_per_slot)?;

        // Check if we're advancing the global crank slot
        let advanced = now_slot > self.last_crank_slot;
        if advanced {
            self.last_crank_slot = now_slot;
        }

        // Always attempt caller's maintenance settle (best-effort, no timestamp games)
        let (slots_forgiven, caller_settle_ok) = if (caller_idx as usize) < MAX_ACCOUNTS
            && self.is_used(caller_idx as usize)
        {
            let last_fee = self.accounts[caller_idx as usize].last_fee_slot;
            let dt = now_slot.saturating_sub(last_fee);
            let forgive = dt / 2;

            if forgive > 0 && dt > 0 {
                self.accounts[caller_idx as usize].last_fee_slot = last_fee.saturating_add(forgive);
            }
            let settle_result =
                self.settle_maintenance_fee_best_effort_for_crank(caller_idx, now_slot);
            (forgive, settle_result.is_ok())
        } else {
            (0, true)
        };

        // Detect conditions for informational flags (before processing)
        let force_realize_active = self.force_realize_active();

        // Process up to ACCOUNTS_PER_CRANK occupied accounts
        let mut num_liquidations: u32 = 0;
        let mut num_liq_errors: u16 = 0;
        let mut force_realize_closed: u16 = 0;
        let mut force_realize_errors: u16 = 0;
        let mut sweep_complete = false;
        let mut accounts_processed: u16 = 0;
        let mut liq_budget = LIQ_BUDGET_PER_CRANK;
        let mut force_realize_budget = FORCE_REALIZE_BUDGET_PER_CRANK;

        let start_cursor = self.crank_cursor;

        // Iterate through index space looking for occupied accounts
        let mut idx = self.crank_cursor as usize;
        let mut slots_scanned: usize = 0;

        while accounts_processed < ACCOUNTS_PER_CRANK && slots_scanned < MAX_ACCOUNTS {
            slots_scanned += 1;

            // Check if slot is used
            let block = idx >> 6;
            let bit = idx & 63;
            let is_occupied = (self.used[block] & (1u64 << bit)) != 0;

            if is_occupied {
                accounts_processed += 1;

                // Always settle maintenance fees for every visited account.
                // This drains idle accounts over time so they eventually become dust.
                let _ = self.settle_maintenance_fee_best_effort_for_crank(idx as u16, now_slot);

                // === Liquidation (if not in force-realize mode) ===
                if !force_realize_active && liq_budget > 0 {
                    if !self.accounts[idx].position_size.is_zero() {
                        match self.liquidate_at_oracle(idx as u16, now_slot, oracle_price) {
                            Ok(true) => {
                                num_liquidations += 1;
                                liq_budget = liq_budget.saturating_sub(1);
                            }
                            Ok(false) => {}
                            Err(_) => {
                                num_liq_errors += 1;
                            }
                        }
                    }

                    // Force-close negative equity or dust positions
                    if !self.accounts[idx].position_size.is_zero() {
                        let equity =
                            self.account_equity_mtm_at_oracle(&self.accounts[idx], oracle_price);
                        let abs_pos = self.accounts[idx].position_size.unsigned_abs();
                        let is_dust = abs_pos < self.params.min_liquidation_abs.get();

                        if equity == 0 || is_dust {
                            // Force close: settle mark, close position, write off loss
                            let _ = self.touch_account_for_liquidation(idx as u16, now_slot, oracle_price);
                            let _ = self.oracle_close_position_core(idx as u16, oracle_price);
                            self.lifetime_force_realize_closes =
                                self.lifetime_force_realize_closes.saturating_add(1);
                        }
                    }
                }

                // === Force-realize (when insurance at/below threshold) ===
                if force_realize_active && force_realize_budget > 0 {
                    if !self.accounts[idx].position_size.is_zero() {
                        if self
                            .touch_account_for_force_realize(idx as u16, now_slot, oracle_price)
                            .is_ok()
                        {
                            if self.oracle_close_position_core(idx as u16, oracle_price).is_ok() {
                                force_realize_closed += 1;
                                force_realize_budget = force_realize_budget.saturating_sub(1);
                                self.lifetime_force_realize_closes =
                                    self.lifetime_force_realize_closes.saturating_add(1);
                            } else {
                                force_realize_errors += 1;
                            }
                        } else {
                            force_realize_errors += 1;
                        }
                    }
                }

                // === LP max tracking ===
                if self.accounts[idx].is_lp() {
                    let abs_pos = self.accounts[idx].position_size.unsigned_abs();
                    self.lp_max_abs_sweep = self.lp_max_abs_sweep.max(U128::new(abs_pos));
                }
            }

            // Advance to next index (with wrap)
            idx = (idx + 1) & ACCOUNT_IDX_MASK;

            // Check for sweep completion: we've wrapped around to sweep_start_idx
            // (and we've actually processed some slots, not just starting)
            if idx == self.sweep_start_idx as usize && slots_scanned > 0 {
                sweep_complete = true;
                break;
            }
        }

        // Update cursor for next crank
        self.crank_cursor = idx as u16;

        // If sweep complete, finalize
        if sweep_complete {
            self.last_full_sweep_completed_slot = now_slot;
            self.lp_max_abs = self.lp_max_abs_sweep;
            self.sweep_start_idx = self.crank_cursor;
        }

        // Garbage collect dust accounts
        let num_gc_closed = self.garbage_collect_dust();

        // Detect conditions for informational flags
        let force_realize_needed = self.force_realize_active();
        let panic_needed = false; // No longer needed with haircut ratio

        Ok(CrankOutcome {
            advanced,
            slots_forgiven,
            caller_settle_ok,
            force_realize_needed,
            panic_needed,
            num_liquidations,
            num_liq_errors,
            num_gc_closed,
            force_realize_closed,
            force_realize_errors,
            last_cursor: self.crank_cursor,
            sweep_complete,
        })
    }

    // ========================================
    // Liquidation
    // ========================================

    /// Compute mark PnL for a position at oracle price (pure helper, no side effects).
    /// Returns the PnL from closing the position at oracle price.
    /// - Longs: profit when oracle > entry
    /// - Shorts: profit when entry > oracle
    pub fn mark_pnl_for_position(pos: i128, entry: u64, oracle: u64) -> Result<i128> {
        if pos == 0 {
            return Ok(0);
        }

        let abs_pos = saturating_abs_i128(pos) as u128;

        let diff: i128 = if pos > 0 {
            // Long: profit when oracle > entry
            (oracle as i128).saturating_sub(entry as i128)
        } else {
            // Short: profit when entry > oracle
            (entry as i128).saturating_sub(oracle as i128)
        };

        // mark_pnl = diff * abs_pos / 1_000_000
        diff.checked_mul(abs_pos as i128)
            .ok_or(RiskError::Overflow)?
            .checked_div(1_000_000)
            .ok_or(RiskError::Overflow)
    }

    /// Compute how much position to close for liquidation (closed-form, single-pass).
    ///
    /// Returns (close_abs, is_full_close) where:
    /// - close_abs = absolute position size to close
    /// - is_full_close = true if this is a full position close (including dust kill-switch)
    ///
    /// ## Algorithm:
    /// 1. Compute target_bps = maintenance_margin_bps + liquidation_buffer_bps
    /// 2. Compute max safe remaining position: abs_pos_safe_max = floor(E_mtm * 10_000 * 1_000_000 / (P * target_bps))
    /// 3. close_abs = abs_pos - abs_pos_safe_max
    /// 4. If remaining position < min_liquidation_abs, do full close (dust kill-switch)
    ///
    /// Uses MTM equity (capital + realized_pnl + mark_pnl) for correct risk calculation.
    /// This is deterministic, requires no iteration, and guarantees single-pass liquidation.
    pub fn compute_liquidation_close_amount(
        &self,
        account: &Account,
        oracle_price: u64,
    ) -> (u128, bool) {
        let abs_pos = saturating_abs_i128(account.position_size.get()) as u128;
        if abs_pos == 0 {
            return (0, false);
        }

        // MTM equity at oracle price (fail-safe: overflow returns 0 = full liquidation)
        let equity = self.account_equity_mtm_at_oracle(account, oracle_price);

        // Target margin = maintenance + buffer (in basis points)
        let target_bps = self
            .params
            .maintenance_margin_bps
            .saturating_add(self.params.liquidation_buffer_bps);

        // Maximum safe remaining position (floor-safe calculation)
        // abs_pos_safe_max = floor(equity * 10_000 * 1_000_000 / (oracle_price * target_bps))
        // Rearranged to avoid intermediate overflow:
        // abs_pos_safe_max = floor(equity * 10_000_000_000 / (oracle_price * target_bps))
        let numerator = mul_u128(equity, 10_000_000_000);
        let denominator = mul_u128(oracle_price as u128, target_bps as u128);

        let mut abs_pos_safe_max = if denominator == 0 {
            0 // Edge case: full liquidation if no denominator
        } else {
            numerator / denominator
        };

        // Clamp to current position (can't have safe max > actual position)
        abs_pos_safe_max = core::cmp::min(abs_pos_safe_max, abs_pos);

        // Conservative rounding guard: subtract 1 unit to ensure we close slightly more
        // than mathematically required. This guarantees post-liquidation account is
        // strictly on the safe side of the inequality despite integer truncation.
        if abs_pos_safe_max > 0 {
            abs_pos_safe_max -= 1;
        }

        // Required close amount
        let close_abs = abs_pos.saturating_sub(abs_pos_safe_max);

        // Dust kill-switch: if remaining position would be below min, do full close
        let remaining = abs_pos.saturating_sub(close_abs);
        if remaining < self.params.min_liquidation_abs.get() {
            return (abs_pos, true); // Full close
        }

        (close_abs, close_abs == abs_pos)
    }

    /// Core helper for closing a SLICE of a position at oracle price (partial liquidation).
    ///
    /// Similar to oracle_close_position_core but:
    /// - Only closes `close_abs` units of position (not the entire position)
    /// - Computes proportional mark_pnl for the closed slice
    /// - Entry price remains unchanged (correct for same-direction partial reduction)
    ///
    /// ## PnL Routing (same invariant as full close):
    /// - mark_pnl > 0 (profit) → backed by haircut ratio h (no ADL needed)
    /// - mark_pnl <= 0 (loss) → realized via settle_warmup_to_capital (capital path)
    /// - Residual negative PnL (capital exhausted) → written off via set_pnl(i, 0) (spec §6.1)
    ///
    /// ASSUMES: Caller has already called touch_account_full() on this account.
    fn oracle_close_position_slice_core(
        &mut self,
        idx: u16,
        oracle_price: u64,
        close_abs: u128,
    ) -> Result<ClosedOutcome> {
        let pos = self.accounts[idx as usize].position_size.get();
        let current_abs_pos = saturating_abs_i128(pos) as u128;

        if close_abs == 0 || current_abs_pos == 0 {
            return Ok(ClosedOutcome {
                abs_pos: 0,
                mark_pnl: 0,
                cap_before: self.accounts[idx as usize].capital.get(),
                cap_after: self.accounts[idx as usize].capital.get(),
                position_was_closed: false,
            });
        }

        if close_abs >= current_abs_pos {
            return self.oracle_close_position_core(idx, oracle_price);
        }

        let entry = self.accounts[idx as usize].entry_price;
        let cap_before = self.accounts[idx as usize].capital.get();

        let diff: i128 = if pos > 0 {
            (oracle_price as i128).saturating_sub(entry as i128)
        } else {
            (entry as i128).saturating_sub(oracle_price as i128)
        };

        let mark_pnl = match diff
            .checked_mul(close_abs as i128)
            .and_then(|v| v.checked_div(1_000_000))
        {
            Some(pnl) => pnl,
            None => -u128_to_i128_clamped(cap_before),
        };

        // Apply mark PnL via set_pnl (maintains pnl_pos_tot aggregate)
        let new_pnl = self.accounts[idx as usize].pnl.get().saturating_add(mark_pnl);
        self.set_pnl(idx as usize, new_pnl);

        // Update position
        let new_abs_pos = current_abs_pos.saturating_sub(close_abs);
        self.accounts[idx as usize].position_size = if pos > 0 {
            I128::new(new_abs_pos as i128)
        } else {
            I128::new(-(new_abs_pos as i128))
        };

        // Update OI
        self.total_open_interest = self.total_open_interest - close_abs;

        // Update LP aggregates if LP
        if self.accounts[idx as usize].is_lp() {
            let new_pos = self.accounts[idx as usize].position_size.get();
            self.net_lp_pos = self.net_lp_pos - pos + new_pos;
            self.lp_sum_abs = self.lp_sum_abs - close_abs;
        }

        // Settle warmup (loss settlement + profit conversion per spec §6)
        self.settle_warmup_to_capital(idx)?;

        // Write off residual negative PnL (capital exhausted) per spec §6.1
        if self.accounts[idx as usize].pnl.is_negative() {
            self.set_pnl(idx as usize, 0);
        }

        let cap_after = self.accounts[idx as usize].capital.get();

        Ok(ClosedOutcome {
            abs_pos: close_abs,
            mark_pnl,
            cap_before,
            cap_after,
            position_was_closed: true,
        })
    }

    /// Core helper for oracle-price full position close (spec §6).
    ///
    /// Applies mark PnL, closes position, settles warmup, writes off unpayable loss.
    /// No ADL needed — undercollateralization is reflected via haircut ratio h.
    ///
    /// ASSUMES: Caller has already called touch_account_full() on this account.
    fn oracle_close_position_core(&mut self, idx: u16, oracle_price: u64) -> Result<ClosedOutcome> {
        if self.accounts[idx as usize].position_size.is_zero() {
            return Ok(ClosedOutcome {
                abs_pos: 0,
                mark_pnl: 0,
                cap_before: self.accounts[idx as usize].capital.get(),
                cap_after: self.accounts[idx as usize].capital.get(),
                position_was_closed: false,
            });
        }

        let pos = self.accounts[idx as usize].position_size.get();
        let abs_pos = saturating_abs_i128(pos) as u128;
        let entry = self.accounts[idx as usize].entry_price;
        let cap_before = self.accounts[idx as usize].capital.get();

        let mark_pnl = match Self::mark_pnl_for_position(pos, entry, oracle_price) {
            Ok(pnl) => pnl,
            Err(_) => -u128_to_i128_clamped(cap_before),
        };

        // Apply mark PnL via set_pnl (maintains pnl_pos_tot aggregate)
        let new_pnl = self.accounts[idx as usize].pnl.get().saturating_add(mark_pnl);
        self.set_pnl(idx as usize, new_pnl);

        // Close position
        self.accounts[idx as usize].position_size = I128::ZERO;
        self.accounts[idx as usize].entry_price = oracle_price;

        // Update OI
        self.total_open_interest = self.total_open_interest - abs_pos;

        // Update LP aggregates if LP
        if self.accounts[idx as usize].is_lp() {
            self.net_lp_pos = self.net_lp_pos - pos;
            self.lp_sum_abs = self.lp_sum_abs - abs_pos;
        }

        // Settle warmup (loss settlement + profit conversion per spec §6)
        self.settle_warmup_to_capital(idx)?;

        // Write off residual negative PnL (capital exhausted) per spec §6.1
        if self.accounts[idx as usize].pnl.is_negative() {
            self.set_pnl(idx as usize, 0);
        }

        let cap_after = self.accounts[idx as usize].capital.get();

        Ok(ClosedOutcome {
            abs_pos,
            mark_pnl,
            cap_before,
            cap_after,
            position_was_closed: true,
        })
    }

    /// Liquidate a single account at oracle price if below maintenance margin.
    ///
    /// Returns Ok(true) if liquidation occurred, Ok(false) if not needed/possible.
    /// Per spec: close position, settle losses, write off unpayable PnL, charge fee.
    /// No ADL — haircut ratio h reflects any undercollateralization.
    pub fn liquidate_at_oracle(
        &mut self,
        idx: u16,
        now_slot: u64,
        oracle_price: u64,
    ) -> Result<bool> {
        self.current_slot = now_slot;

        if (idx as usize) >= MAX_ACCOUNTS || !self.is_used(idx as usize) {
            return Ok(false);
        }

        if oracle_price == 0 || oracle_price > MAX_ORACLE_PRICE {
            return Err(RiskError::Overflow);
        }

        if self.accounts[idx as usize].position_size.is_zero() {
            return Ok(false);
        }

        // Settle funding + mark-to-market + best-effort fees
        self.touch_account_for_liquidation(idx, now_slot, oracle_price)?;

        let account = &self.accounts[idx as usize];
        if self.is_above_maintenance_margin_mtm(account, oracle_price) {
            return Ok(false);
        }

        let (close_abs, is_full_close) =
            self.compute_liquidation_close_amount(account, oracle_price);

        if close_abs == 0 {
            return Ok(false);
        }

        // Close position (no ADL — losses written off in close helper)
        let mut outcome = if is_full_close {
            self.oracle_close_position_core(idx, oracle_price)?
        } else {
            match self.oracle_close_position_slice_core(idx, oracle_price, close_abs) {
                Ok(r) => r,
                Err(RiskError::Overflow) => {
                    self.oracle_close_position_core(idx, oracle_price)?
                }
                Err(e) => return Err(e),
            }
        };

        if !outcome.position_was_closed {
            return Ok(false);
        }

        // Safety check: if position remains and still below target, full close
        if !self.accounts[idx as usize].position_size.is_zero() {
            let target_bps = self
                .params
                .maintenance_margin_bps
                .saturating_add(self.params.liquidation_buffer_bps);
            if !self.is_above_margin_bps_mtm(&self.accounts[idx as usize], oracle_price, target_bps)
            {
                let fallback = self.oracle_close_position_core(idx, oracle_price)?;
                if fallback.position_was_closed {
                    outcome.abs_pos = outcome.abs_pos.saturating_add(fallback.abs_pos);
                }
            }
        }

        // Charge liquidation fee (from remaining capital → insurance)
        let notional = mul_u128(outcome.abs_pos, oracle_price as u128) / 1_000_000;
        let fee_raw = mul_u128(notional, self.params.liquidation_fee_bps as u128) / 10_000;
        let fee = core::cmp::min(fee_raw, self.params.liquidation_fee_cap.get());
        let account_capital = self.accounts[idx as usize].capital.get();
        let pay = core::cmp::min(fee, account_capital);

        self.set_capital(idx as usize, account_capital.saturating_sub(pay));
        self.insurance_fund.balance = self.insurance_fund.balance.saturating_add_u128(U128::new(pay));
        self.insurance_fund.fee_revenue = self.insurance_fund.fee_revenue.saturating_add_u128(U128::new(pay));

        self.lifetime_liquidations = self.lifetime_liquidations.saturating_add(1);

        Ok(true)
    }

    // ========================================
    // Warmup
    // ========================================

    /// Calculate withdrawable PNL for an account after warmup
    pub fn withdrawable_pnl(&self, account: &Account) -> u128 {
        // Only positive PNL can be withdrawn
        let positive_pnl = clamp_pos_i128(account.pnl.get());

        // Available = positive PNL - reserved
        let available_pnl = sub_u128(positive_pnl, account.reserved_pnl as u128);

        let effective_slot = self.current_slot;

        // Calculate elapsed slots
        let elapsed_slots = effective_slot.saturating_sub(account.warmup_started_at_slot);

        // Calculate warmed up cap: slope * elapsed_slots
        let warmed_up_cap = mul_u128(account.warmup_slope_per_step.get(), elapsed_slots as u128);

        // Return minimum of available and warmed up
        core::cmp::min(available_pnl, warmed_up_cap)
    }

    /// Update warmup slope for an account
    /// NOTE: No warmup rate cap (removed for simplicity)
    pub fn update_warmup_slope(&mut self, idx: u16) -> Result<()> {
        if !self.is_used(idx as usize) {
            return Err(RiskError::AccountNotFound);
        }

        let account = &mut self.accounts[idx as usize];

        // Calculate available gross PnL: AvailGross_i = max(PNL_i, 0) - R_i (spec §5)
        let positive_pnl = clamp_pos_i128(account.pnl.get());
        let avail_gross = sub_u128(positive_pnl, account.reserved_pnl as u128);

        // Calculate slope: avail_gross / warmup_period
        // Ensure slope >= 1 when avail_gross > 0 to prevent "zero forever" bug
        let slope = if self.params.warmup_period_slots > 0 {
            let base = avail_gross / (self.params.warmup_period_slots as u128);
            if avail_gross > 0 {
                core::cmp::max(1, base)
            } else {
                0
            }
        } else {
            avail_gross // Instant warmup if period is 0
        };

        // Verify slope >= 1 when available PnL exists
        #[cfg(any(test, kani))]
        debug_assert!(
            slope >= 1 || avail_gross == 0,
            "Warmup slope bug: slope {} with avail_gross {}",
            slope,
            avail_gross
        );

        // Update slope
        account.warmup_slope_per_step = U128::new(slope);

        account.warmup_started_at_slot = self.current_slot;

        Ok(())
    }

    // ========================================
    // Funding
    // ========================================

    /// Accrue funding globally in O(1)
    pub fn accrue_funding(
        &mut self,
        now_slot: u64,
        oracle_price: u64,
        funding_rate_bps_per_slot: i64,
    ) -> Result<()> {
        let dt = now_slot.saturating_sub(self.last_funding_slot);
        if dt == 0 {
            return Ok(());
        }

        // Input validation to prevent overflow
        if oracle_price == 0 || oracle_price > MAX_ORACLE_PRICE {
            return Err(RiskError::Overflow);
        }

        // Cap funding rate at 10000 bps (100%) per slot as sanity bound
        // Real-world funding rates should be much smaller (typically < 1 bps/slot)
        if funding_rate_bps_per_slot.abs() > 10_000 {
            return Err(RiskError::Overflow);
        }

        if dt > 31_536_000 {
            return Err(RiskError::Overflow);
        }

        // Use checked math to prevent silent overflow
        let price = oracle_price as i128;
        let rate = funding_rate_bps_per_slot as i128;
        let dt_i = dt as i128;

        // ΔF = price × rate × dt / 10,000
        let delta = price
            .checked_mul(rate)
            .ok_or(RiskError::Overflow)?
            .checked_mul(dt_i)
            .ok_or(RiskError::Overflow)?
            .checked_div(10_000)
            .ok_or(RiskError::Overflow)?;

        self.funding_index_qpb_e6 = self
            .funding_index_qpb_e6
            .checked_add(delta)
            .ok_or(RiskError::Overflow)?;

        self.last_funding_slot = now_slot;
        Ok(())
    }

    /// Settle funding for an account (lazy update)
    fn settle_account_funding(account: &mut Account, global_funding_index: I128) -> Result<()> {
        let delta_f = global_funding_index
            .get()
            .checked_sub(account.funding_index.get())
            .ok_or(RiskError::Overflow)?;

        if delta_f != 0 && !account.position_size.is_zero() {
            // payment = position × ΔF / 1e6
            // Round UP for positive payments (account pays), truncate for negative (account receives)
            // This ensures vault always has at least what's owed (one-sided conservation slack).
            let raw = account
                .position_size
                .get()
                .checked_mul(delta_f)
                .ok_or(RiskError::Overflow)?;

            let payment = if raw > 0 {
                // Account is paying: round UP to ensure vault gets at least theoretical amount
                raw.checked_add(999_999)
                    .ok_or(RiskError::Overflow)?
                    .checked_div(1_000_000)
                    .ok_or(RiskError::Overflow)?
            } else {
                // Account is receiving: truncate towards zero to give at most theoretical amount
                raw.checked_div(1_000_000).ok_or(RiskError::Overflow)?
            };

            // Longs pay when funding positive: pnl -= payment
            account.pnl = I128::new(
                account
                    .pnl
                    .get()
                    .checked_sub(payment)
                    .ok_or(RiskError::Overflow)?,
            );
        }

        account.funding_index = global_funding_index;
        Ok(())
    }

    /// Touch an account (settle funding before operations)
    pub fn touch_account(&mut self, idx: u16) -> Result<()> {
        if !self.is_used(idx as usize) {
            return Err(RiskError::AccountNotFound);
        }

        let global_fi = self.funding_index_qpb_e6;
        let old_pnl = self.accounts[idx as usize].pnl.get();
        Self::settle_account_funding(&mut self.accounts[idx as usize], global_fi)?;
        let new_pnl = self.accounts[idx as usize].pnl.get();
        // Update pnl_pos_tot for funding settlement
        let old_pos = if old_pnl > 0 { old_pnl as u128 } else { 0 };
        let new_pos = if new_pnl > 0 { new_pnl as u128 } else { 0 };
        self.pnl_pos_tot = U128::new(
            self.pnl_pos_tot
                .get()
                .saturating_add(new_pos)
                .saturating_sub(old_pos),
        );
        Ok(())
    }

    /// Settle mark-to-market PnL to the current oracle price (variation margin).
    ///
    /// This realizes all unrealized PnL at the given oracle price and resets
    /// entry_price = oracle_price. After calling this, mark_pnl_for_position
    /// will return 0 for this account at this oracle price.
    ///
    /// This makes positions fungible: any LP can close any user's position
    /// because PnL is settled to a common reference price.
    pub fn settle_mark_to_oracle(&mut self, idx: u16, oracle_price: u64) -> Result<()> {
        if idx as usize >= MAX_ACCOUNTS || !self.is_used(idx as usize) {
            return Err(RiskError::AccountNotFound);
        }

        if self.accounts[idx as usize].position_size.is_zero() {
            // No position: just set entry to oracle for determinism
            self.accounts[idx as usize].entry_price = oracle_price;
            return Ok(());
        }

        // Compute mark PnL at current oracle
        let mark = Self::mark_pnl_for_position(
            self.accounts[idx as usize].position_size.get(),
            self.accounts[idx as usize].entry_price,
            oracle_price,
        )?;

        // Realize the mark PnL via set_pnl (maintains pnl_pos_tot)
        let new_pnl = self.accounts[idx as usize]
            .pnl
            .get()
            .checked_add(mark)
            .ok_or(RiskError::Overflow)?;
        self.set_pnl(idx as usize, new_pnl);

        // Reset entry to oracle (mark PnL is now 0 at this price)
        self.accounts[idx as usize].entry_price = oracle_price;

        Ok(())
    }

    /// Best-effort mark-to-oracle settlement that uses saturating_add instead of
    /// checked_add, so it never fails on overflow.  This prevents the liquidation
    /// path from wedging on extreme mark PnL values.
    fn settle_mark_to_oracle_best_effort(&mut self, idx: u16, oracle_price: u64) -> Result<()> {
        if idx as usize >= MAX_ACCOUNTS || !self.is_used(idx as usize) {
            return Err(RiskError::AccountNotFound);
        }

        if self.accounts[idx as usize].position_size.is_zero() {
            self.accounts[idx as usize].entry_price = oracle_price;
            return Ok(());
        }

        // Compute mark PnL at current oracle
        let mark = Self::mark_pnl_for_position(
            self.accounts[idx as usize].position_size.get(),
            self.accounts[idx as usize].entry_price,
            oracle_price,
        )?;

        // Realize the mark PnL via set_pnl (saturating — never fails on overflow)
        let new_pnl = self.accounts[idx as usize].pnl.get().saturating_add(mark);
        self.set_pnl(idx as usize, new_pnl);

        // Reset entry to oracle (mark PnL is now 0 at this price)
        self.accounts[idx as usize].entry_price = oracle_price;

        Ok(())
    }

    /// Full account touch: funding + mark settlement + maintenance fees + warmup.
    /// This is the standard "lazy settlement" path called on every user operation.
    /// Triggers liquidation check if fees push account below maintenance margin.
    pub fn touch_account_full(&mut self, idx: u16, now_slot: u64, oracle_price: u64) -> Result<()> {
        // 1. Settle funding
        self.touch_account(idx)?;

        // 2. Settle mark-to-market (variation margin)
        self.settle_mark_to_oracle(idx, oracle_price)?;

        // 3. Settle maintenance fees (may trigger undercollateralized error)
        self.settle_maintenance_fee(idx, now_slot, oracle_price)?;

        // 4. Settle warmup (convert warmed PnL to capital, realize losses)
        self.settle_warmup_to_capital(idx)?;

        // 5. Sweep any fee debt from newly-available capital (warmup may
        //    have created capital that should pay outstanding fee debt)
        self.pay_fee_debt_from_capital(idx);

        // 6. Re-check maintenance margin after fee debt sweep
        if !self.accounts[idx as usize].position_size.is_zero() {
            if !self.is_above_maintenance_margin_mtm(
                &self.accounts[idx as usize],
                oracle_price,
            ) {
                return Err(RiskError::Undercollateralized);
            }
        }

        Ok(())
    }

    /// Minimal touch for crank liquidations: funding + maintenance only.
    /// Skips warmup settlement for performance - losses are handled inline
    /// by the deferred close helpers, positive warmup left for user ops.
    fn touch_account_for_crank(
        &mut self,
        idx: u16,
        now_slot: u64,
        oracle_price: u64,
    ) -> Result<()> {
        // 1. Settle funding
        self.touch_account(idx)?;

        // 2. Settle maintenance fees (may trigger undercollateralized error)
        self.settle_maintenance_fee(idx, now_slot, oracle_price)?;

        // NOTE: No warmup settlement - handled inline for losses in close helpers
        Ok(())
    }

    // ========================================
    // Deposits and Withdrawals
    // ========================================

    /// Deposit funds to account.
    ///
    /// Settles any accrued maintenance fees from the deposit first,
    /// with the remainder added to capital. This ensures fee conservation
    /// (fees are never forgiven) and prevents stuck accounts.
    pub fn deposit(&mut self, idx: u16, amount: u128, now_slot: u64) -> Result<()> {
        // Update current_slot so warmup/bookkeeping progresses consistently
        self.current_slot = now_slot;

        if !self.is_used(idx as usize) {
            return Err(RiskError::AccountNotFound);
        }

        let account = &mut self.accounts[idx as usize];
        let mut deposit_remaining = amount;

        // Calculate and settle accrued fees
        let dt = now_slot.saturating_sub(account.last_fee_slot);
        if dt > 0 {
            let due = self
                .params
                .maintenance_fee_per_slot
                .get()
                .saturating_mul(dt as u128);
            account.last_fee_slot = now_slot;

            // Deduct from fee_credits (coupon: no insurance booking here —
            // insurance was already paid when credits were granted)
            account.fee_credits = account.fee_credits.saturating_sub(due as i128);
        }

        // Pay any owed fees from deposit first
        if account.fee_credits.is_negative() {
            let owed = neg_i128_to_u128(account.fee_credits.get());
            let pay = core::cmp::min(owed, deposit_remaining);

            deposit_remaining -= pay;
            self.insurance_fund.balance = self.insurance_fund.balance + pay;
            self.insurance_fund.fee_revenue = self.insurance_fund.fee_revenue + pay;

            // Credit back what was paid
            account.fee_credits = account.fee_credits.saturating_add(pay as i128);
        }

        // Vault gets full deposit (tokens received)
        self.vault = U128::new(add_u128(self.vault.get(), amount));

        // Capital gets remainder after fees (via set_capital to maintain c_tot)
        let new_cap = add_u128(self.accounts[idx as usize].capital.get(), deposit_remaining);
        self.set_capital(idx as usize, new_cap);

        // Settle warmup after deposit (allows losses to be paid promptly if underwater)
        self.settle_warmup_to_capital(idx)?;

        // If any older fee debt remains, use capital to pay it now.
        self.pay_fee_debt_from_capital(idx);

        Ok(())
    }

    /// Withdraw capital from an account.
    /// Relies on Solana transaction atomicity: if this returns Err, the entire TX aborts.
    pub fn withdraw(
        &mut self,
        idx: u16,
        amount: u128,
        now_slot: u64,
        oracle_price: u64,
    ) -> Result<()> {
        // Update current_slot so warmup/bookkeeping progresses consistently
        self.current_slot = now_slot;

        // Validate oracle price bounds (prevents overflow in mark_pnl calculations)
        if oracle_price == 0 || oracle_price > MAX_ORACLE_PRICE {
            return Err(RiskError::Overflow);
        }

        // Require fresh crank (time-based) before state-changing operations
        self.require_fresh_crank(now_slot)?;

        // Require recent full sweep started
        self.require_recent_full_sweep(now_slot)?;

        // Validate account exists
        if !self.is_used(idx as usize) {
            return Err(RiskError::AccountNotFound);
        }

        // Full settlement: funding + maintenance fees + warmup
        self.touch_account_full(idx, now_slot, oracle_price)?;

        // Read account state (scope the borrow)
        let (old_capital, pnl, position_size, entry_price) = {
            let account = &self.accounts[idx as usize];
            (
                account.capital,
                account.pnl,
                account.position_size,
                account.entry_price,
            )
        };

        // Check we have enough capital
        if old_capital.get() < amount {
            return Err(RiskError::InsufficientBalance);
        }

        // Calculate MTM equity after withdrawal with haircut (spec §3.3)
        // equity_mtm = max(0, new_capital + min(pnl, 0) + effective_pos_pnl(pnl) + mark_pnl)
        // Fail-safe: if mark_pnl overflows (corrupted entry_price/position_size), treat as 0 equity
        let new_capital = sub_u128(old_capital.get(), amount);
        let new_equity_mtm =
            match Self::mark_pnl_for_position(position_size.get(), entry_price, oracle_price) {
                Ok(mark_pnl) => {
                    let cap_i = u128_to_i128_clamped(new_capital);
                    let neg_pnl = core::cmp::min(pnl.get(), 0);
                    let eff_pos = self.effective_pos_pnl(pnl.get());
                    let new_eq_i = cap_i
                        .saturating_add(neg_pnl)
                        .saturating_add(u128_to_i128_clamped(eff_pos))
                        .saturating_add(mark_pnl);
                    if new_eq_i > 0 {
                        new_eq_i as u128
                    } else {
                        0
                    }
                }
                Err(_) => 0, // Overflow => worst-case equity => will fail margin check below
            };

        // If account has position, must maintain initial margin at ORACLE price (MTM check)
        // This prevents withdrawing to a state that's immediately liquidatable
        if !position_size.is_zero() {
            let position_notional = mul_u128(
                saturating_abs_i128(position_size.get()) as u128,
                oracle_price as u128,
            ) / 1_000_000;

            let initial_margin_required =
                mul_u128(position_notional, self.params.initial_margin_bps as u128) / 10_000;

            if new_equity_mtm < initial_margin_required {
                return Err(RiskError::Undercollateralized);
            }
        }

        // Commit the withdrawal (via set_capital to maintain c_tot)
        self.set_capital(idx as usize, new_capital);
        self.vault = U128::new(sub_u128(self.vault.get(), amount));

        // Post-withdrawal MTM maintenance margin check at oracle price
        // This is a safety belt to ensure we never leave an account in liquidatable state
        if !self.accounts[idx as usize].position_size.is_zero() {
            if !self.is_above_maintenance_margin_mtm(&self.accounts[idx as usize], oracle_price) {
                // Revert the withdrawal (via set_capital to maintain c_tot)
                self.set_capital(idx as usize, old_capital.get());
                self.vault = U128::new(add_u128(self.vault.get(), amount));
                return Err(RiskError::Undercollateralized);
            }
        }

        // Regression assert: after settle + withdraw, negative PnL should have been settled
        #[cfg(any(test, kani))]
        debug_assert!(
            !self.accounts[idx as usize].pnl.is_negative()
                || self.accounts[idx as usize].capital.is_zero(),
            "Withdraw: negative PnL must settle immediately"
        );

        Ok(())
    }

    // ========================================
    // Trading
    // ========================================

    /// Calculate account's collateral (capital + positive PNL)
    /// NOTE: This is the OLD collateral definition. For margin checks, use account_equity instead.
    pub fn account_collateral(&self, account: &Account) -> u128 {
        add_u128(account.capital.get(), clamp_pos_i128(account.pnl.get()))
    }

    /// Realized-only equity: max(0, capital + realized_pnl).
    ///
    /// DEPRECATED for margin checks: Use account_equity_mtm_at_oracle instead.
    /// This helper is retained for reporting, PnL display, and test assertions that
    /// specifically need realized-only equity.
    #[inline]
    pub fn account_equity(&self, account: &Account) -> u128 {
        let cap_i = u128_to_i128_clamped(account.capital.get());
        let eq_i = cap_i.saturating_add(account.pnl.get());
        if eq_i > 0 {
            eq_i as u128
        } else {
            0
        }
    }

    /// Mark-to-market equity at oracle price with haircut (the ONLY correct equity for margin checks).
    /// equity_mtm = max(0, C_i + min(PNL_i, 0) + PNL_eff_pos_i + mark_pnl)
    /// where PNL_eff_pos_i = floor(max(PNL_i, 0) * h_num / h_den) per spec §3.3.
    ///
    /// FAIL-SAFE: On overflow, returns 0 (worst-case equity) to ensure liquidation
    /// can still trigger. This prevents overflow from blocking liquidation.
    pub fn account_equity_mtm_at_oracle(&self, account: &Account, oracle_price: u64) -> u128 {
        let mark = match Self::mark_pnl_for_position(
            account.position_size.get(),
            account.entry_price,
            oracle_price,
        ) {
            Ok(m) => m,
            Err(_) => return 0, // Overflow => worst-case equity
        };
        let cap_i = u128_to_i128_clamped(account.capital.get());
        let neg_pnl = core::cmp::min(account.pnl.get(), 0);
        let eff_pos = self.effective_pos_pnl(account.pnl.get());
        let eq_i = cap_i
            .saturating_add(neg_pnl)
            .saturating_add(u128_to_i128_clamped(eff_pos))
            .saturating_add(mark);
        if eq_i > 0 {
            eq_i as u128
        } else {
            0
        }
    }

    /// MTM margin check: is equity_mtm > required margin?
    /// This is the ONLY correct margin predicate for all risk checks.
    ///
    /// FAIL-SAFE: Returns false on any error (treat as below margin / liquidatable).
    pub fn is_above_margin_bps_mtm(&self, account: &Account, oracle_price: u64, bps: u64) -> bool {
        let equity = self.account_equity_mtm_at_oracle(account, oracle_price);

        // Position value at oracle price
        let position_value = mul_u128(
            saturating_abs_i128(account.position_size.get()) as u128,
            oracle_price as u128,
        ) / 1_000_000;

        // Margin requirement at given bps
        let margin_required = mul_u128(position_value, bps as u128) / 10_000;

        equity > margin_required
    }

    /// MTM maintenance margin check (fail-safe: returns false on overflow)
    #[inline]
    pub fn is_above_maintenance_margin_mtm(&self, account: &Account, oracle_price: u64) -> bool {
        self.is_above_margin_bps_mtm(account, oracle_price, self.params.maintenance_margin_bps)
    }

    /// Check if account is above maintenance margin (DEPRECATED: uses realized-only equity)
    /// Use is_above_maintenance_margin_mtm for all margin checks.
    pub fn is_above_maintenance_margin(&self, account: &Account, oracle_price: u64) -> bool {
        self.is_above_margin_bps(account, oracle_price, self.params.maintenance_margin_bps)
    }

    /// Cheap priority score for ranking liquidation candidates.
    /// Score = max(maint_required - equity, 0).
    /// Higher score = more urgent to liquidate.
    ///
    /// This is a ranking heuristic only - NOT authoritative.
    /// Real liquidation still calls touch_account_full() and checks margin properly.
    /// A "wrong" top-K pick is harmless: it just won't liquidate.
    #[inline]
    fn liq_priority_score(&self, a: &Account, oracle_price: u64) -> u128 {
        if a.position_size.is_zero() {
            return 0;
        }

        // MTM equity (fail-safe: overflow returns 0, making account appear liquidatable)
        let equity = self.account_equity_mtm_at_oracle(a, oracle_price);

        let pos_value = mul_u128(
            saturating_abs_i128(a.position_size.get()) as u128,
            oracle_price as u128,
        ) / 1_000_000;

        let maint = mul_u128(pos_value, self.params.maintenance_margin_bps as u128) / 10_000;

        if equity >= maint {
            0
        } else {
            maint - equity
        }
    }

    /// Check if account is above a given margin threshold (DEPRECATED: uses realized-only equity).
    ///
    /// Use is_above_margin_bps_mtm for all margin checks. This helper is retained for
    /// tests that specifically need realized-only margin comparison.
    pub fn is_above_margin_bps(&self, account: &Account, oracle_price: u64, bps: u64) -> bool {
        let equity = self.account_equity(account);

        // Calculate position value at current price
        let position_value = mul_u128(
            saturating_abs_i128(account.position_size.get()) as u128,
            oracle_price as u128,
        ) / 1_000_000;

        // Margin requirement at given bps
        let margin_required = mul_u128(position_value, bps as u128) / 10_000;

        equity > margin_required
    }

    /// Risk-reduction-only mode is entered when the system is in deficit. Warmups are frozen so pending PNL cannot become principal. Withdrawals of principal (capital) are allowed (subject to margin). Risk-increasing actions are blocked; only risk-reducing/neutral operations are allowed.
    /// Execute a trade between LP and user.
    /// Relies on Solana transaction atomicity: if this returns Err, the entire TX aborts.
    pub fn execute_trade<M: MatchingEngine>(
        &mut self,
        matcher: &M,
        lp_idx: u16,
        user_idx: u16,
        now_slot: u64,
        oracle_price: u64,
        size: i128,
    ) -> Result<()> {
        // Update current_slot so warmup/bookkeeping progresses consistently
        self.current_slot = now_slot;

        // Require fresh crank (time-based) before state-changing operations
        self.require_fresh_crank(now_slot)?;

        // Validate indices
        if !self.is_used(lp_idx as usize) || !self.is_used(user_idx as usize) {
            return Err(RiskError::AccountNotFound);
        }

        // Validate oracle price bounds (prevents overflow in mark_pnl calculations)
        if oracle_price == 0 || oracle_price > MAX_ORACLE_PRICE {
            return Err(RiskError::Overflow);
        }

        // Validate requested size bounds
        if size == 0 || size == i128::MIN {
            return Err(RiskError::Overflow);
        }
        if saturating_abs_i128(size) as u128 > MAX_POSITION_ABS {
            return Err(RiskError::Overflow);
        }

        // Validate account kinds (using is_lp/is_user methods for SBF workaround)
        if !self.accounts[lp_idx as usize].is_lp() {
            return Err(RiskError::AccountKindMismatch);
        }
        if !self.accounts[user_idx as usize].is_user() {
            return Err(RiskError::AccountKindMismatch);
        }

        // Check if trade increases risk (absolute exposure for either party)
        let old_user_pos = self.accounts[user_idx as usize].position_size.get();
        let old_lp_pos = self.accounts[lp_idx as usize].position_size.get();
        let new_user_pos = old_user_pos.saturating_add(size);
        let new_lp_pos = old_lp_pos.saturating_sub(size);

        let user_inc = saturating_abs_i128(new_user_pos) > saturating_abs_i128(old_user_pos);
        let lp_inc = saturating_abs_i128(new_lp_pos) > saturating_abs_i128(old_lp_pos);

        if user_inc || lp_inc {
            // Risk-increasing: require recent full sweep
            self.require_recent_full_sweep(now_slot)?;
        }

        // Call matching engine
        let lp = &self.accounts[lp_idx as usize];
        let execution = matcher.execute_match(
            &lp.matcher_program,
            &lp.matcher_context,
            lp.account_id,
            oracle_price,
            size,
        )?;

        let exec_price = execution.price;
        let exec_size = execution.size;

        // Validate matcher output (trust boundary enforcement)
        // Price bounds
        if exec_price == 0 || exec_price > MAX_ORACLE_PRICE {
            return Err(RiskError::InvalidMatchingEngine);
        }

        // Size bounds
        if exec_size == 0 {
            // No fill: treat as no-op trade (no side effects, deterministic)
            return Ok(());
        }
        if exec_size == i128::MIN {
            return Err(RiskError::InvalidMatchingEngine);
        }
        if saturating_abs_i128(exec_size) as u128 > MAX_POSITION_ABS {
            return Err(RiskError::InvalidMatchingEngine);
        }

        // Must be same direction as requested
        if (exec_size > 0) != (size > 0) {
            return Err(RiskError::InvalidMatchingEngine);
        }

        // Must be partial fill at most (abs(exec) <= abs(request))
        if saturating_abs_i128(exec_size) > saturating_abs_i128(size) {
            return Err(RiskError::InvalidMatchingEngine);
        }

        // Settle funding, mark-to-market, and maintenance fees for both accounts
        // Mark settlement MUST happen before position changes (variation margin)
        // Note: warmup is settled at the END after trade PnL is generated
        self.touch_account(user_idx)?;
        self.touch_account(lp_idx)?;
        self.settle_mark_to_oracle(user_idx, oracle_price)?;
        self.settle_mark_to_oracle(lp_idx, oracle_price)?;
        self.settle_maintenance_fee(user_idx, now_slot, oracle_price)?;
        self.settle_maintenance_fee(lp_idx, now_slot, oracle_price)?;

        // Calculate fee
        let notional =
            mul_u128(saturating_abs_i128(exec_size) as u128, exec_price as u128) / 1_000_000;
        let fee = mul_u128(notional, self.params.trading_fee_bps as u128) / 10_000;

        // Access both accounts
        let (user, lp) = if user_idx < lp_idx {
            let (left, right) = self.accounts.split_at_mut(lp_idx as usize);
            (&mut left[user_idx as usize], &mut right[0])
        } else {
            let (left, right) = self.accounts.split_at_mut(user_idx as usize);
            (&mut right[0], &mut left[lp_idx as usize])
        };

        // Calculate new positions (checked math - overflow returns Err)
        let new_user_position = user
            .position_size
            .get()
            .checked_add(exec_size)
            .ok_or(RiskError::Overflow)?;
        let new_lp_position = lp
            .position_size
            .get()
            .checked_sub(exec_size)
            .ok_or(RiskError::Overflow)?;

        // Validate final position bounds (prevents overflow in mark_pnl calculations)
        if saturating_abs_i128(new_user_position) as u128 > MAX_POSITION_ABS
            || saturating_abs_i128(new_lp_position) as u128 > MAX_POSITION_ABS
        {
            return Err(RiskError::Overflow);
        }

        // Trade PnL = (oracle - exec_price) * exec_size (zero-sum between parties)
        // User gains if buying below oracle (exec_size > 0, oracle > exec_price)
        // LP gets opposite sign
        // Note: entry_price is already oracle_price after settle_mark_to_oracle
        let price_diff = (oracle_price as i128)
            .checked_sub(exec_price as i128)
            .ok_or(RiskError::Overflow)?;

        let trade_pnl = price_diff
            .checked_mul(exec_size)
            .ok_or(RiskError::Overflow)?
            .checked_div(1_000_000)
            .ok_or(RiskError::Overflow)?;

        // Compute final PNL values (checked math - overflow returns Err)
        let new_user_pnl = user
            .pnl
            .get()
            .checked_add(trade_pnl)
            .ok_or(RiskError::Overflow)?;
        let new_lp_pnl = lp
            .pnl
            .get()
            .checked_sub(trade_pnl)
            .ok_or(RiskError::Overflow)?;

        // Deduct trading fee from user capital, not PnL (spec §8.1)
        let new_user_capital = user
            .capital
            .get()
            .checked_sub(fee)
            .ok_or(RiskError::InsufficientBalance)?;

        // Compute projected pnl_pos_tot AFTER trade PnL for fresh haircut in margin checks.
        // Can't call self.haircut_ratio() due to split_at_mut borrow on accounts;
        // inline the delta computation and haircut formula.
        let old_user_pnl_pos = if user.pnl.get() > 0 { user.pnl.get() as u128 } else { 0 };
        let new_user_pnl_pos = if new_user_pnl > 0 { new_user_pnl as u128 } else { 0 };
        let old_lp_pnl_pos = if lp.pnl.get() > 0 { lp.pnl.get() as u128 } else { 0 };
        let new_lp_pnl_pos = if new_lp_pnl > 0 { new_lp_pnl as u128 } else { 0 };

        // Recompute haircut using projected post-trade pnl_pos_tot (spec §3.3).
        // Fee moves C→I so Residual = V - C_tot - I is unchanged; only pnl_pos_tot changes.
        let projected_pnl_pos_tot = self.pnl_pos_tot
            .get()
            .saturating_add(new_user_pnl_pos)
            .saturating_sub(old_user_pnl_pos)
            .saturating_add(new_lp_pnl_pos)
            .saturating_sub(old_lp_pnl_pos);

        let (h_num, h_den) = if projected_pnl_pos_tot == 0 {
            (1u128, 1u128)
        } else {
            let residual = self.vault.get()
                .saturating_sub(self.c_tot.get())
                .saturating_sub(self.insurance_fund.balance.get());
            (core::cmp::min(residual, projected_pnl_pos_tot), projected_pnl_pos_tot)
        };

        // Inline helper: compute effective positive PnL with post-trade haircut
        let eff_pos_pnl_inline = |pnl: i128| -> u128 {
            if pnl <= 0 {
                return 0;
            }
            let pos_pnl = pnl as u128;
            if h_den == 0 {
                return pos_pnl;
            }
            mul_u128(pos_pnl, h_num) / h_den
        };

        // Check user maintenance margin with haircut (spec §3.3)
        // After settle_mark_to_oracle, entry_price = oracle_price, so mark_pnl = 0
        // Equity = max(0, new_capital + min(pnl, 0) + eff_pos_pnl)
        if new_user_position != 0 {
            let user_cap_i = u128_to_i128_clamped(new_user_capital);
            let neg_pnl = core::cmp::min(new_user_pnl, 0);
            let eff_pos = eff_pos_pnl_inline(new_user_pnl);
            let user_eq_i = user_cap_i
                .saturating_add(neg_pnl)
                .saturating_add(u128_to_i128_clamped(eff_pos));
            let user_equity = if user_eq_i > 0 { user_eq_i as u128 } else { 0 };
            let position_value = mul_u128(
                saturating_abs_i128(new_user_position) as u128,
                oracle_price as u128,
            ) / 1_000_000;
            let margin_required =
                mul_u128(position_value, self.params.maintenance_margin_bps as u128) / 10_000;
            if user_equity <= margin_required {
                return Err(RiskError::Undercollateralized);
            }
        }

        // Check LP maintenance margin with haircut (spec §3.3)
        // After settle_mark_to_oracle, entry_price = oracle_price, so mark_pnl = 0
        if new_lp_position != 0 {
            let lp_cap_i = u128_to_i128_clamped(lp.capital.get());
            let neg_pnl = core::cmp::min(new_lp_pnl, 0);
            let eff_pos = eff_pos_pnl_inline(new_lp_pnl);
            let lp_eq_i = lp_cap_i
                .saturating_add(neg_pnl)
                .saturating_add(u128_to_i128_clamped(eff_pos));
            let lp_equity = if lp_eq_i > 0 { lp_eq_i as u128 } else { 0 };
            let position_value = mul_u128(
                saturating_abs_i128(new_lp_position) as u128,
                oracle_price as u128,
            ) / 1_000_000;
            let margin_required =
                mul_u128(position_value, self.params.maintenance_margin_bps as u128) / 10_000;
            if lp_equity <= margin_required {
                return Err(RiskError::Undercollateralized);
            }
        }

        // Commit all state changes
        self.insurance_fund.fee_revenue =
            U128::new(add_u128(self.insurance_fund.fee_revenue.get(), fee));
        self.insurance_fund.balance = U128::new(add_u128(self.insurance_fund.balance.get(), fee));

        // Credit fee to user's fee_credits (active traders earn credits that offset maintenance)
        user.fee_credits = user.fee_credits.saturating_add(fee as i128);

        // (old/new pnl_pos values computed above for projected haircut; reused here)
        user.pnl = I128::new(new_user_pnl);
        user.position_size = I128::new(new_user_position);
        user.entry_price = oracle_price;
        // Commit fee deduction from user capital (spec §8.1)
        user.capital = U128::new(new_user_capital);

        lp.pnl = I128::new(new_lp_pnl);
        lp.position_size = I128::new(new_lp_position);
        lp.entry_price = oracle_price;

        // Maintain c_tot: user capital decreased by fee
        self.c_tot = U128::new(self.c_tot.get().saturating_sub(fee));

        // Maintain pnl_pos_tot aggregate
        self.pnl_pos_tot = U128::new(
            self.pnl_pos_tot
                .get()
                .saturating_add(new_user_pnl_pos)
                .saturating_sub(old_user_pnl_pos)
                .saturating_add(new_lp_pnl_pos)
                .saturating_sub(old_lp_pnl_pos),
        );

        // Update total open interest tracking (O(1))
        // OI = sum of abs(position_size) across all accounts
        let old_oi =
            saturating_abs_i128(old_user_pos) as u128 + saturating_abs_i128(old_lp_pos) as u128;
        let new_oi = saturating_abs_i128(new_user_position) as u128
            + saturating_abs_i128(new_lp_position) as u128;
        if new_oi > old_oi {
            self.total_open_interest = self.total_open_interest.saturating_add(new_oi - old_oi);
        } else {
            self.total_open_interest = self.total_open_interest.saturating_sub(old_oi - new_oi);
        }

        // Update LP aggregates for funding/threshold (O(1))
        let old_lp_abs = saturating_abs_i128(old_lp_pos) as u128;
        let new_lp_abs = saturating_abs_i128(new_lp_position) as u128;
        // net_lp_pos: delta = new - old
        self.net_lp_pos = self
            .net_lp_pos
            .saturating_sub(old_lp_pos)
            .saturating_add(new_lp_position);
        // lp_sum_abs: delta of abs values
        if new_lp_abs > old_lp_abs {
            self.lp_sum_abs = self.lp_sum_abs.saturating_add(new_lp_abs - old_lp_abs);
        } else {
            self.lp_sum_abs = self.lp_sum_abs.saturating_sub(old_lp_abs - new_lp_abs);
        }
        // lp_max_abs: monotone increase only (conservative upper bound)
        self.lp_max_abs = U128::new(self.lp_max_abs.get().max(new_lp_abs));

        // Two-pass settlement: losses first, then profits.
        // This ensures the loser's capital reduction increases Residual before
        // the winner's profit conversion reads the haircut ratio. Without this,
        // the winner's matured PnL can be haircutted to 0 because Residual
        // hasn't been increased by the loser's loss settlement yet (Finding G).
        self.settle_loss_only(user_idx)?;
        self.settle_loss_only(lp_idx)?;
        // Now Residual reflects realized losses; profit conversion uses correct h.
        self.settle_warmup_to_capital(user_idx)?;
        self.settle_warmup_to_capital(lp_idx)?;

        // Now recompute warmup slopes after PnL changes (resets started_at_slot)
        self.update_warmup_slope(user_idx)?;
        self.update_warmup_slope(lp_idx)?;

        Ok(())
    }
    /// Settle loss only (§6.1): negative PnL pays from capital immediately.
    /// If PnL still negative after capital exhausted, write off via set_pnl(i, 0).
    /// Used in two-pass settlement to ensure all losses are realized (increasing
    /// Residual) before any profit conversions use the haircut ratio.
    pub fn settle_loss_only(&mut self, idx: u16) -> Result<()> {
        if !self.is_used(idx as usize) {
            return Err(RiskError::AccountNotFound);
        }

        let pnl = self.accounts[idx as usize].pnl.get();
        if pnl < 0 {
            let need = neg_i128_to_u128(pnl);
            let capital = self.accounts[idx as usize].capital.get();
            let pay = core::cmp::min(need, capital);

            if pay > 0 {
                self.set_capital(idx as usize, capital - pay);
                self.set_pnl(idx as usize, pnl.saturating_add(pay as i128));
            }

            // Write off any remaining negative PnL (spec §6.1 step 4)
            if self.accounts[idx as usize].pnl.is_negative() {
                self.set_pnl(idx as usize, 0);
            }
        }

        Ok(())
    }

    /// Settle warmup: loss settlement + profit conversion per spec §6
    ///
    /// §6.1 Loss settlement: negative PnL pays from capital immediately.
    ///   If PnL still negative after capital exhausted, write off via set_pnl(i, 0).
    ///
    /// §6.2 Profit conversion: warmable gross profit converts to capital at haircut ratio h.
    ///   y = floor(x * h_num / h_den), where (h_num, h_den) is computed pre-conversion.
    pub fn settle_warmup_to_capital(&mut self, idx: u16) -> Result<()> {
        if !self.is_used(idx as usize) {
            return Err(RiskError::AccountNotFound);
        }

        // §6.1 Loss settlement (negative PnL → reduce capital immediately)
        let pnl = self.accounts[idx as usize].pnl.get();
        if pnl < 0 {
            let need = neg_i128_to_u128(pnl);
            let capital = self.accounts[idx as usize].capital.get();
            let pay = core::cmp::min(need, capital);

            if pay > 0 {
                self.set_capital(idx as usize, capital - pay);
                self.set_pnl(idx as usize, pnl.saturating_add(pay as i128));
            }

            // Write off any remaining negative PnL (spec §6.1 step 4)
            if self.accounts[idx as usize].pnl.is_negative() {
                self.set_pnl(idx as usize, 0);
            }
        }

        // §6.2 Profit conversion (warmup converts junior profit → protected principal)
        let pnl = self.accounts[idx as usize].pnl.get();
        if pnl > 0 {
            let positive_pnl = pnl as u128;
            let reserved = self.accounts[idx as usize].reserved_pnl as u128;
            let avail_gross = positive_pnl.saturating_sub(reserved);

            // Compute warmable cap from slope and elapsed time (spec §5.3)
            let started_at = self.accounts[idx as usize].warmup_started_at_slot;
            let elapsed = self.current_slot.saturating_sub(started_at);
            let slope = self.accounts[idx as usize].warmup_slope_per_step.get();
            let cap = mul_u128(slope, elapsed as u128);

            let x = core::cmp::min(avail_gross, cap);

            if x > 0 {
                // Compute haircut ratio BEFORE modifying PnL/capital (spec §6.2)
                let (h_num, h_den) = self.haircut_ratio();
                let y = if h_den == 0 {
                    x
                } else {
                    mul_u128(x, h_num) / h_den
                };

                // Reduce junior profit claim by x
                self.set_pnl(idx as usize, pnl - (x as i128));
                // Increase protected principal by y
                let new_cap = add_u128(self.accounts[idx as usize].capital.get(), y);
                self.set_capital(idx as usize, new_cap);
            }

            // Advance warmup time base and update slope (spec §5.4)
            self.accounts[idx as usize].warmup_started_at_slot = self.current_slot;

            // Recompute warmup slope per spec §5.4
            let new_pnl = self.accounts[idx as usize].pnl.get();
            let new_avail = if new_pnl > 0 {
                (new_pnl as u128).saturating_sub(self.accounts[idx as usize].reserved_pnl as u128)
            } else {
                0
            };
            let slope = if new_avail == 0 {
                0
            } else if self.params.warmup_period_slots > 0 {
                core::cmp::max(1, new_avail / (self.params.warmup_period_slots as u128))
            } else {
                new_avail
            };
            self.accounts[idx as usize].warmup_slope_per_step = U128::new(slope);
        }

        Ok(())
    }

    // Panic Settlement (Atomic Global Settle)
    // ========================================

    /// Atomic global settlement at oracle price
    ///
    /// Emergency instruction that:
    /// 1. Settles all open positions at the given oracle price
    /// 2. Settles losses from capital, writes off unpayable losses via set_pnl(i, 0)
    /// 3. Settles warmup for all accounts
    ///
    /// Under the haircut ratio design, written-off losses reduce Residual,
    /// which reduces h, automatically haircutting all positive PnL claims.
    /// No ADL scan is needed.
    pub fn panic_settle_all(&mut self, oracle_price: u64) -> Result<()> {
        if oracle_price == 0 || oracle_price > MAX_ORACLE_PRICE {
            return Err(RiskError::Overflow);
        }

        // Reset LP aggregates - all positions will be closed
        self.net_lp_pos = I128::ZERO;
        self.lp_sum_abs = U128::ZERO;
        self.lp_max_abs = U128::ZERO;

        // Phase 1: settle funding, apply mark PnL, close positions, settle losses
        let global_funding_index = self.funding_index_qpb_e6;
        for block in 0..BITMAP_WORDS {
            let mut w = self.used[block];
            while w != 0 {
                let bit = w.trailing_zeros() as usize;
                let idx = block * 64 + bit;
                w &= w - 1;

                if idx >= MAX_ACCOUNTS {
                    continue;
                }

                // Settle funding (maintain pnl_pos_tot)
                let old_pnl = self.accounts[idx].pnl.get();
                Self::settle_account_funding(&mut self.accounts[idx], global_funding_index)?;
                let new_pnl = self.accounts[idx].pnl.get();
                let old_pos = if old_pnl > 0 { old_pnl as u128 } else { 0 };
                let new_pos = if new_pnl > 0 { new_pnl as u128 } else { 0 };
                self.pnl_pos_tot = U128::new(
                    self.pnl_pos_tot.get().saturating_add(new_pos).saturating_sub(old_pos),
                );

                // Skip accounts with no position
                if self.accounts[idx].position_size.is_zero() {
                    continue;
                }

                // Compute mark PnL at oracle price
                let pos = self.accounts[idx].position_size.get();
                let abs_pos = saturating_abs_i128(pos) as u128;
                let mark_pnl =
                    match Self::mark_pnl_for_position(pos, self.accounts[idx].entry_price, oracle_price) {
                        Ok(pnl) => pnl,
                        Err(_) => -u128_to_i128_clamped(self.accounts[idx].capital.get()),
                    };

                // Apply mark PnL via set_pnl
                let new_pnl = self.accounts[idx].pnl.get().saturating_add(mark_pnl);
                self.set_pnl(idx, new_pnl);

                // Close position
                self.accounts[idx].position_size = I128::ZERO;
                self.accounts[idx].entry_price = oracle_price;
                self.total_open_interest = self.total_open_interest.saturating_sub(abs_pos);

                // Settle loss: pay from capital, write off remainder
                let pnl = self.accounts[idx].pnl.get();
                if pnl < 0 {
                    let need = neg_i128_to_u128(pnl);
                    let capital = self.accounts[idx].capital.get();
                    let pay = core::cmp::min(need, capital);
                    if pay > 0 {
                        self.set_capital(idx, capital - pay);
                        self.set_pnl(idx, self.accounts[idx].pnl.get().saturating_add(pay as i128));
                    }
                    // Write off any remaining negative PnL
                    if self.accounts[idx].pnl.is_negative() {
                        self.set_pnl(idx, 0);
                    }
                }
            }
        }

        // Phase 2: Settle warmup for all accounts
        for block in 0..BITMAP_WORDS {
            let mut w = self.used[block];
            while w != 0 {
                let bit = w.trailing_zeros() as usize;
                let idx = block * 64 + bit;
                w &= w - 1;

                if idx >= MAX_ACCOUNTS {
                    continue;
                }

                self.settle_warmup_to_capital(idx as u16)?;
            }
        }

        Ok(())
    }

    /// Force realize losses to unstick the exchange at insurance floor
    ///
    /// When insurance is at/below the threshold, this closes all positions at oracle price,
    /// settles losses from capital, and writes off any unpaid losses via set_pnl(i, 0).
    /// Under the haircut ratio design, write-offs reduce Residual which reduces h,
    /// automatically haircutting all positive PnL claims.
    pub fn force_realize_losses(&mut self, oracle_price: u64) -> Result<()> {
        if oracle_price == 0 || oracle_price > MAX_ORACLE_PRICE {
            return Err(RiskError::Overflow);
        }

        // Gate: only allowed when insurance is at or below floor
        if self.insurance_fund.balance > self.params.risk_reduction_threshold {
            return Err(RiskError::Unauthorized);
        }

        // Reset LP aggregates - all positions will be closed
        self.net_lp_pos = I128::ZERO;
        self.lp_sum_abs = U128::ZERO;
        self.lp_max_abs = U128::ZERO;

        // Phase 1: settle funding, close positions, settle losses
        let global_funding_index = self.funding_index_qpb_e6;
        for block in 0..BITMAP_WORDS {
            let mut w = self.used[block];
            while w != 0 {
                let bit = w.trailing_zeros() as usize;
                let idx = block * 64 + bit;
                w &= w - 1;

                if idx >= MAX_ACCOUNTS {
                    continue;
                }

                // Settle funding
                let old_pnl = self.accounts[idx].pnl.get();
                Self::settle_account_funding(&mut self.accounts[idx], global_funding_index)?;
                let new_pnl = self.accounts[idx].pnl.get();
                let old_pos = if old_pnl > 0 { old_pnl as u128 } else { 0 };
                let new_pos = if new_pnl > 0 { new_pnl as u128 } else { 0 };
                self.pnl_pos_tot = U128::new(
                    self.pnl_pos_tot.get().saturating_add(new_pos).saturating_sub(old_pos),
                );

                // Skip accounts with no position
                if self.accounts[idx].position_size.is_zero() {
                    continue;
                }

                // Close position at oracle: compute mark PnL
                let pos = self.accounts[idx].position_size.get();
                let abs_pos = saturating_abs_i128(pos) as u128;
                let mark_pnl =
                    match Self::mark_pnl_for_position(pos, self.accounts[idx].entry_price, oracle_price) {
                        Ok(pnl) => pnl,
                        Err(_) => -u128_to_i128_clamped(self.accounts[idx].capital.get()),
                    };

                // Apply mark PnL via set_pnl
                let new_pnl = self.accounts[idx].pnl.get().saturating_add(mark_pnl);
                self.set_pnl(idx, new_pnl);

                // Close position
                self.accounts[idx].position_size = I128::ZERO;
                self.accounts[idx].entry_price = oracle_price;
                self.total_open_interest = self.total_open_interest.saturating_sub(abs_pos);

                // Settle loss: pay from capital, write off remainder
                let pnl = self.accounts[idx].pnl.get();
                if pnl < 0 {
                    let need = neg_i128_to_u128(pnl);
                    let capital = self.accounts[idx].capital.get();
                    let pay = core::cmp::min(need, capital);
                    if pay > 0 {
                        self.set_capital(idx, capital - pay);
                        self.set_pnl(idx, self.accounts[idx].pnl.get().saturating_add(pay as i128));
                    }
                    // Write off any remaining negative PnL
                    if self.accounts[idx].pnl.is_negative() {
                        self.set_pnl(idx, 0);
                    }
                }
            }
        }

        Ok(())
    }

    /// Top up insurance fund
    ///
    /// Adds tokens to both vault and insurance fund.
    /// Returns true if the top-up brings insurance above the risk reduction threshold.
    pub fn top_up_insurance_fund(&mut self, amount: u128) -> Result<bool> {
        // Add to vault
        self.vault = U128::new(add_u128(self.vault.get(), amount));

        // Add to insurance fund
        self.insurance_fund.balance =
            U128::new(add_u128(self.insurance_fund.balance.get(), amount));

        // Return whether we're now above the force-realize threshold
        let above_threshold =
            self.insurance_fund.balance > self.params.risk_reduction_threshold;
        Ok(above_threshold)
    }


    /// Placeholder: stranded recovery is no longer needed.
    /// The haircut ratio automatically handles undercollateralization.
    pub fn recover_stranded_to_insurance(&mut self) -> Result<u128> {
        Ok(0)
    }


    // ========================================
    // Utilities
    // ========================================

    /// Check conservation invariant (spec §3.1)
    ///
    /// Primary invariant: V >= C_tot + I
    ///
    /// Extended check: vault >= sum(capital) + sum(positive_pnl_clamped) + insurance
    /// with bounded rounding slack from funding/mark settlement.
    ///
    /// We also verify the full accounting identity including settled/unsettled PnL:
    /// vault >= sum(capital) + sum(settled_pnl + mark_pnl) + insurance
    /// The difference (slack) must be bounded by MAX_ROUNDING_SLACK.
    pub fn check_conservation(&self, oracle_price: u64) -> bool {
        let mut total_capital = 0u128;
        let mut net_pnl: i128 = 0;
        let mut net_mark: i128 = 0;
        let mut mark_ok = true;
        let global_index = self.funding_index_qpb_e6;

        self.for_each_used(|_idx, account| {
            total_capital = add_u128(total_capital, account.capital.get());

            // Compute "would-be settled" PNL for this account
            let mut settled_pnl = account.pnl.get();
            if !account.position_size.is_zero() {
                let delta_f = global_index
                    .get()
                    .saturating_sub(account.funding_index.get());
                if delta_f != 0 {
                    let raw = account.position_size.get().saturating_mul(delta_f);
                    let payment = if raw > 0 {
                        raw.saturating_add(999_999).saturating_div(1_000_000)
                    } else {
                        raw.saturating_div(1_000_000)
                    };
                    settled_pnl = settled_pnl.saturating_sub(payment);
                }

                match Self::mark_pnl_for_position(
                    account.position_size.get(),
                    account.entry_price,
                    oracle_price,
                ) {
                    Ok(mark) => {
                        net_mark = net_mark.saturating_add(mark);
                    }
                    Err(_) => {
                        mark_ok = false;
                    }
                }
            }
            net_pnl = net_pnl.saturating_add(settled_pnl);
        });

        if !mark_ok {
            return false;
        }

        // Conservation: vault >= C_tot + I (primary invariant)
        let primary = self.vault.get()
            >= total_capital.saturating_add(self.insurance_fund.balance.get());
        if !primary {
            return false;
        }

        // Extended: vault >= sum(capital) + sum(settled_pnl + mark_pnl) + insurance
        let total_pnl = net_pnl.saturating_add(net_mark);
        let base = add_u128(total_capital, self.insurance_fund.balance.get());

        let expected = if total_pnl >= 0 {
            add_u128(base, total_pnl as u128)
        } else {
            base.saturating_sub(neg_i128_to_u128(total_pnl))
        };

        let actual = self.vault.get();

        if actual < expected {
            return false;
        }
        let slack = actual - expected;
        slack <= MAX_ROUNDING_SLACK
    }

    /// Advance to next slot (for testing warmup)
    pub fn advance_slot(&mut self, slots: u64) {
        self.current_slot = self.current_slot.saturating_add(slots);
    }
}

