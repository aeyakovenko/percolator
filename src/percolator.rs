//! Formally Verified Risk Engine for Perpetual DEX — v12.1.0
//!
//! Implements the v12.1.0 spec: Native 128-bit Architecture.
//!
//! This module implements a formally verified risk engine that guarantees:
//! 1. Protected principal for flat accounts
//! 2. PNL warmup prevents instant withdrawal of manipulated profits
//! 3. ADL via lazy A/K side indices on the opposing OI side
//! 4. Conservation of funds across all operations (V >= C_tot + I)
//! 5. No hidden protocol MM — bankruptcy socialization through explicit A/K state only
//!
//! # Atomicity Model
//!
//! Public functions suffixed with `_not_atomic` can return `Err` after partial
//! state mutation. **Callers MUST abort the entire transaction on `Err`** —
//! they must not retry, suppress, or continue with mutated state.
//!
//! On Solana SVM, any `Err` return from an instruction aborts the transaction
//! and rolls back all account state automatically. This is the expected
//! deployment model.
//!
//! Public functions WITHOUT the suffix (`deposit`, `top_up_insurance_fund`,
//! `deposit_fee_credits`, `accrue_market_to`) use validate-then-mutate:
//! `Err` means no state was changed.
//!
//! Internal helpers (`enqueue_adl`, `liquidate_at_oracle_internal`, etc.)
//! are not individually atomic — they rely on the calling `_not_atomic`
//! method to propagate `Err` to the transaction boundary.

#![no_std]
#![forbid(unsafe_code)]

#[cfg(kani)]
extern crate kani;

// ============================================================================
// Conditional visibility macro
// ============================================================================

// ============================================================================
// Conditional visibility macro
// ============================================================================

/// Internal methods that proof harnesses and integration tests need direct
/// access to. Private in production builds, `pub` under test/kani.
/// Each invocation emits two mutually-exclusive cfg-gated copies of the same
/// function: one `pub`, one private.
macro_rules! test_visible {
    (
        $(#[$meta:meta])*
        fn $name:ident($($args:tt)*) $(-> $ret:ty)? $body:block
    ) => {
        $(#[$meta])*
        #[cfg(any(feature = "test", kani))]
        pub fn $name($($args)*) $(-> $ret)? $body

        $(#[$meta])*
        #[cfg(not(any(feature = "test", kani)))]
        fn $name($($args)*) $(-> $ret)? $body
    };
}

// ============================================================================
// Constants
// ============================================================================

#[cfg(kani)]
pub const MAX_ACCOUNTS: usize = 4;

#[cfg(all(feature = "test", not(kani)))]
pub const MAX_ACCOUNTS: usize = 64;

#[cfg(all(not(kani), not(feature = "test")))]
pub const MAX_ACCOUNTS: usize = 4096;

pub const BITMAP_WORDS: usize = (MAX_ACCOUNTS + 63) / 64;
pub const MAX_ROUNDING_SLACK: u128 = MAX_ACCOUNTS as u128;
const ACCOUNT_IDX_MASK: usize = MAX_ACCOUNTS - 1;

pub const GC_CLOSE_BUDGET: u32 = 32;
pub const ACCOUNTS_PER_CRANK: u16 = 128;
pub const LIQ_BUDGET_PER_CRANK: u16 = 64;

/// POS_SCALE = 1_000_000 (spec §1.2)
pub const POS_SCALE: u128 = 1_000_000;

/// ADL_ONE = 1_000_000 (spec §1.3)
pub const ADL_ONE: u128 = 1_000_000;

/// MIN_A_SIDE = 1_000 (spec §1.4)
pub const MIN_A_SIDE: u128 = 1_000;

/// MAX_ORACLE_PRICE = 1_000_000_000_000 (spec §1.4)
pub const MAX_ORACLE_PRICE: u64 = 1_000_000_000_000;

/// MAX_FUNDING_DT = 65535 (spec §1.4)
pub const MAX_FUNDING_DT: u64 = u16::MAX as u64;

/// MAX_ABS_FUNDING_BPS_PER_SLOT = 10000 (spec §1.4)
pub const MAX_ABS_FUNDING_BPS_PER_SLOT: i64 = 10_000;

// Normative bounds (spec §1.4)
pub const MAX_VAULT_TVL: u128 = 10_000_000_000_000_000;
pub const MAX_POSITION_ABS_Q: u128 = 100_000_000_000_000;
pub const MAX_ACCOUNT_NOTIONAL: u128 = 100_000_000_000_000_000_000;
pub const MAX_TRADE_SIZE_Q: u128 = MAX_POSITION_ABS_Q; // spec §1.4
pub const MAX_OI_SIDE_Q: u128 = 100_000_000_000_000;
pub const MAX_MATERIALIZED_ACCOUNTS: u64 = 1_000_000;
pub const MAX_ACCOUNT_POSITIVE_PNL: u128 = 100_000_000_000_000_000_000_000_000_000_000;
pub const MAX_PNL_POS_TOT: u128 = 100_000_000_000_000_000_000_000_000_000_000_000_000;
pub const MAX_TRADING_FEE_BPS: u64 = 10_000;
pub const MAX_MARGIN_BPS: u64 = 10_000;
pub const MAX_LIQUIDATION_FEE_BPS: u64 = 10_000;
pub const MAX_PROTOCOL_FEE_ABS: u128 = 1_000_000_000_000_000_000_000_000_000_000_000_000; // 10^36, spec §1.4
pub const MAX_MAINTENANCE_FEE_PER_SLOT: u128 = 10_000_000_000_000_000; // spec §1.4

// ============================================================================
// BPF-Safe 128-bit Types
// ============================================================================
pub mod i128;
pub use i128::{I128, U128};

// ============================================================================
// Wide 256-bit Arithmetic (used for transient intermediates only)
// ============================================================================
pub mod wide_math;
use wide_math::{
    U256, I256,
    mul_div_floor_u128, mul_div_ceil_u128,
    wide_mul_div_floor_u128,
    wide_signed_mul_div_floor_from_k_pair,
    wide_mul_div_ceil_u128_or_over_i128max, OverI128Magnitude,
    saturating_mul_u128_u64,
    fee_debt_u128_checked,
    mul_div_floor_u256_with_rem,
    ceil_div_positive_checked,
    floor_div_signed_conservative_i128,
};

// ============================================================================
// Core Data Structures
// ============================================================================

// AccountKind as plain u8 — eliminates UB risk from invalid enum discriminants
// when casting raw slab bytes to &Account via zero-copy. u8 has no invalid
// representations, so &*(ptr as *const Account) is always sound.
// pub enum AccountKind { User = 0, LP = 1 }  // replaced by constants below

/// Side mode for OI sides (spec §2.4)
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SideMode {
    Normal = 0,
    DrainOnly = 1,
    ResetPending = 2,
}

/// Instruction context for deferred reset scheduling (spec §5.7-5.8)
pub struct InstructionContext {
    pub pending_reset_long: bool,
    pub pending_reset_short: bool,
}

impl InstructionContext {
    pub fn new() -> Self {
        Self {
            pending_reset_long: false,
            pending_reset_short: false,
        }
    }
}

/// Unified account (spec §2.1)
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Account {
    pub account_id: u64,
    pub capital: U128,
    pub kind: u8,  // 0 = User, 1 = LP (was AccountKind enum)

    /// Realized PnL (i128, spec §2.1)
    pub pnl: i128,

    /// Reserved positive PnL (u128, spec §2.1)
    pub reserved_pnl: u128,

    /// Warmup start slot
    pub warmup_started_at_slot: u64,

    /// Linear warmup slope (u128, spec §2.1)
    pub warmup_slope_per_step: u128,

    /// Signed fixed-point base quantity basis (i128, spec §2.1)
    pub position_basis_q: i128,

    /// Side multiplier snapshot at last explicit position attachment (u128)
    pub adl_a_basis: u128,

    /// K coefficient snapshot (i128)
    pub adl_k_snap: i128,

    /// Side epoch snapshot
    pub adl_epoch_snap: u64,

    /// LP matching engine program ID
    pub matcher_program: [u8; 32],
    pub matcher_context: [u8; 32],

    /// Owner pubkey
    pub owner: [u8; 32],

    /// Fee credits
    pub fee_credits: I128,
    pub last_fee_slot: u64,

    /// Cumulative LP trading fees
    pub fees_earned_total: U128,
}

impl Account {
    pub const KIND_USER: u8 = 0;
    pub const KIND_LP: u8 = 1;

    pub fn is_lp(&self) -> bool {
        self.kind == Self::KIND_LP
    }

    pub fn is_user(&self) -> bool {
        self.kind == Self::KIND_USER
    }
}

fn empty_account() -> Account {
    Account {
        account_id: 0,
        capital: U128::ZERO,
        kind: Account::KIND_USER,
        pnl: 0i128,
        reserved_pnl: 0u128,
        warmup_started_at_slot: 0,
        warmup_slope_per_step: 0u128,
        position_basis_q: 0i128,
        adl_a_basis: ADL_ONE,
        adl_k_snap: 0i128,
        adl_epoch_snap: 0,
        matcher_program: [0; 32],
        matcher_context: [0; 32],
        owner: [0; 32],
        fee_credits: I128::ZERO,
        last_fee_slot: 0,
        fees_earned_total: U128::ZERO,
    }
}

/// Insurance fund state
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InsuranceFund {
    pub balance: U128,
}

/// Risk engine parameters
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RiskParams {
    pub warmup_period_slots: u64,
    pub maintenance_margin_bps: u64,
    pub initial_margin_bps: u64,
    pub trading_fee_bps: u64,
    pub max_accounts: u64,
    pub new_account_fee: U128,
    pub maintenance_fee_per_slot: U128,
    pub max_crank_staleness_slots: u64,
    pub liquidation_fee_bps: u64,
    pub liquidation_fee_cap: U128,
    pub min_liquidation_abs: U128,
    pub min_initial_deposit: U128,
    /// Absolute nonzero-position margin floors (spec §9.1)
    pub min_nonzero_mm_req: u128,
    pub min_nonzero_im_req: u128,
    /// Insurance fund floor (spec §1.4: 0 <= I_floor <= MAX_VAULT_TVL)
    pub insurance_floor: U128,
}

/// Main risk engine state (spec §2.2)
#[repr(C)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RiskEngine {
    pub vault: U128,
    pub insurance_fund: InsuranceFund,
    pub params: RiskParams,
    pub current_slot: u64,

    /// Stored funding rate for anti-retroactivity
    pub funding_rate_bps_per_slot_last: i64,

    // Keeper crank tracking
    pub last_crank_slot: u64,
    pub max_crank_staleness_slots: u64,

    // O(1) aggregates (spec §2.2)
    pub c_tot: U128,
    pub pnl_pos_tot: u128,
    pub pnl_matured_pos_tot: u128,

    // Crank cursors
    pub liq_cursor: u16,
    pub gc_cursor: u16,
    pub last_full_sweep_start_slot: u64,
    pub last_full_sweep_completed_slot: u64,
    pub crank_cursor: u16,
    pub sweep_start_idx: u16,

    // Lifetime counters
    pub lifetime_liquidations: u64,

    // ADL side state (spec §2.2)
    pub adl_mult_long: u128,
    pub adl_mult_short: u128,
    pub adl_coeff_long: i128,
    pub adl_coeff_short: i128,
    pub adl_epoch_long: u64,
    pub adl_epoch_short: u64,
    pub adl_epoch_start_k_long: i128,
    pub adl_epoch_start_k_short: i128,
    pub oi_eff_long_q: u128,
    pub oi_eff_short_q: u128,
    pub side_mode_long: SideMode,
    pub side_mode_short: SideMode,
    pub stored_pos_count_long: u64,
    pub stored_pos_count_short: u64,
    pub stale_account_count_long: u64,
    pub stale_account_count_short: u64,

    /// Dynamic phantom dust bounds (spec §4.6, §5.7)
    pub phantom_dust_bound_long_q: u128,
    pub phantom_dust_bound_short_q: u128,

    /// Materialized account count (spec §2.2)
    pub materialized_account_count: u64,

    /// Last oracle price used in accrue_market_to
    pub last_oracle_price: u64,
    /// Last slot used in accrue_market_to
    pub last_market_slot: u64,
    /// Funding price sample (for anti-retroactivity)
    pub funding_price_sample_last: u64,

    // Insurance floor is read from self.params.insurance_floor (no duplicate field)

    // Slab management
    pub used: [u64; BITMAP_WORDS],
    pub num_used_accounts: u16,
    pub next_account_id: u64,
    pub free_head: u16,
    pub next_free: [u16; MAX_ACCOUNTS],
    pub accounts: [Account; MAX_ACCOUNTS],
}

// ============================================================================
// Error Types
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RiskError {
    InsufficientBalance,
    Undercollateralized,
    Unauthorized,
    InvalidMatchingEngine,
    PnlNotWarmedUp,
    Overflow,
    AccountNotFound,
    NotAnLPAccount,
    PositionSizeMismatch,
    AccountKindMismatch,
    SideBlocked,
    CorruptState,
}

pub type Result<T> = core::result::Result<T, RiskError>;

/// Liquidation policy (spec §10.6)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiquidationPolicy {
    FullClose,
    ExactPartial(u128), // q_close_q
}

/// Outcome of a keeper crank operation
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CrankOutcome {
    pub advanced: bool,
    pub slots_forgiven: u64,
    pub caller_settle_ok: bool,
    pub force_realize_needed: bool,
    pub panic_needed: bool,
    pub num_liquidations: u32,
    pub num_liq_errors: u16,
    pub num_gc_closed: u32,
    pub last_cursor: u16,
    pub sweep_complete: bool,
}

// ============================================================================
// Small Helpers
// ============================================================================

#[inline]
fn add_u128(a: u128, b: u128) -> u128 {
    a.checked_add(b).expect("add_u128 overflow")
}

#[inline]
fn sub_u128(a: u128, b: u128) -> u128 {
    a.checked_sub(b).expect("sub_u128 underflow")
}

#[inline]
fn mul_u128(a: u128, b: u128) -> u128 {
    a.checked_mul(b).expect("mul_u128 overflow")
}

/// Determine which side a signed position is on. Positive = long, negative = short.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Long,
    Short,
}

fn side_of_i128(v: i128) -> Option<Side> {
    if v == 0 {
        None
    } else if v > 0 {
        Some(Side::Long)
    } else {
        Some(Side::Short)
    }
}

fn opposite_side(s: Side) -> Side {
    match s {
        Side::Long => Side::Short,
        Side::Short => Side::Long,
    }
}

/// Clamp i128 max(v, 0) as u128
fn i128_clamp_pos(v: i128) -> u128 {
    if v > 0 {
        v as u128
    } else {
        0u128
    }
}

// ============================================================================
// Core Implementation
// ============================================================================

impl RiskEngine {
    /// Validate configuration parameters (spec §1.4, §2.2.1).
    /// Panics on invalid configuration to prevent deployment with unsafe params.
    fn validate_params(params: &RiskParams) {
        // Capacity: max_accounts within compile-time slab (spec §1.4)
        assert!(
            (params.max_accounts as usize) <= MAX_ACCOUNTS && params.max_accounts > 0,
            "max_accounts must be in 1..=MAX_ACCOUNTS"
        );

        // Margin ordering: 0 <= maintenance_bps <= initial_bps <= 10_000 (spec §1.4)
        assert!(
            params.maintenance_margin_bps <= params.initial_margin_bps,
            "maintenance_margin_bps must be <= initial_margin_bps (spec §1.4)"
        );
        assert!(
            params.initial_margin_bps <= 10_000,
            "initial_margin_bps must be <= 10_000"
        );

        // BPS bounds (spec §1.4)
        assert!(
            params.trading_fee_bps <= 10_000,
            "trading_fee_bps must be <= 10_000"
        );
        assert!(
            params.liquidation_fee_bps <= 10_000,
            "liquidation_fee_bps must be <= 10_000"
        );

        // Nonzero margin floor ordering: 0 < mm < im <= min_initial_deposit (spec §1.4)
        assert!(
            params.min_nonzero_mm_req > 0,
            "min_nonzero_mm_req must be > 0"
        );
        assert!(
            params.min_nonzero_mm_req < params.min_nonzero_im_req,
            "min_nonzero_mm_req must be strictly less than min_nonzero_im_req"
        );
        assert!(
            params.min_nonzero_im_req <= params.min_initial_deposit.get(),
            "min_nonzero_im_req must be <= min_initial_deposit (spec §1.4)"
        );

        // MIN_INITIAL_DEPOSIT bounds: 0 < min_initial_deposit <= MAX_VAULT_TVL (spec §1.4)
        assert!(
            params.min_initial_deposit.get() > 0,
            "min_initial_deposit must be > 0 (spec §1.4)"
        );
        assert!(
            params.min_initial_deposit.get() <= MAX_VAULT_TVL,
            "min_initial_deposit must be <= MAX_VAULT_TVL"
        );

        // Liquidation fee ordering: 0 <= min_liquidation_abs <= liquidation_fee_cap (spec §1.4)
        assert!(
            params.min_liquidation_abs.get() <= params.liquidation_fee_cap.get(),
            "min_liquidation_abs must be <= liquidation_fee_cap (spec §1.4)"
        );
        assert!(
            params.liquidation_fee_cap.get() <= MAX_PROTOCOL_FEE_ABS,
            "liquidation_fee_cap must be <= MAX_PROTOCOL_FEE_ABS (spec §1.4)"
        );

        // Maintenance fee bound (spec §8.2)
        assert!(
            params.maintenance_fee_per_slot.get() <= MAX_MAINTENANCE_FEE_PER_SLOT,
            "maintenance_fee_per_slot must be <= MAX_MAINTENANCE_FEE_PER_SLOT (spec §8.2.1)"
        );

        // Insurance floor (spec §1.4: 0 <= I_floor <= MAX_VAULT_TVL)
        assert!(
            params.insurance_floor.get() <= MAX_VAULT_TVL,
            "insurance_floor must be <= MAX_VAULT_TVL (spec §1.4)"
        );
    }

    /// Create a new risk engine for testing. Initializes with
    /// init_oracle_price = 1 (spec §2.7 compliant).
    #[cfg(any(feature = "test", kani))]
    pub fn new(params: RiskParams) -> Self {
        Self::new_with_market(params, 0, 1)
    }

    /// Create a new risk engine with explicit market initialization (spec §2.7).
    /// Requires `0 < init_oracle_price <= MAX_ORACLE_PRICE` per spec §1.2.
    pub fn new_with_market(params: RiskParams, init_slot: u64, init_oracle_price: u64) -> Self {
        Self::validate_params(&params);
        assert!(
            init_oracle_price > 0 && init_oracle_price <= MAX_ORACLE_PRICE,
            "init_oracle_price must be in (0, MAX_ORACLE_PRICE] per spec §2.7"
        );
        let mut engine = Self {
            vault: U128::ZERO,
            insurance_fund: InsuranceFund {
                balance: U128::ZERO,
            },
            params,
            current_slot: init_slot,
            funding_rate_bps_per_slot_last: 0,
            last_crank_slot: 0,
            max_crank_staleness_slots: params.max_crank_staleness_slots,
            c_tot: U128::ZERO,
            pnl_pos_tot: 0u128,
            pnl_matured_pos_tot: 0u128,
            liq_cursor: 0,
            gc_cursor: 0,
            last_full_sweep_start_slot: 0,
            last_full_sweep_completed_slot: 0,
            crank_cursor: 0,
            sweep_start_idx: 0,
            lifetime_liquidations: 0,
            adl_mult_long: ADL_ONE,
            adl_mult_short: ADL_ONE,
            adl_coeff_long: 0i128,
            adl_coeff_short: 0i128,
            adl_epoch_long: 0,
            adl_epoch_short: 0,
            adl_epoch_start_k_long: 0i128,
            adl_epoch_start_k_short: 0i128,
            oi_eff_long_q: 0u128,
            oi_eff_short_q: 0u128,
            side_mode_long: SideMode::Normal,
            side_mode_short: SideMode::Normal,
            stored_pos_count_long: 0,
            stored_pos_count_short: 0,
            stale_account_count_long: 0,
            stale_account_count_short: 0,
            phantom_dust_bound_long_q: 0u128,
            phantom_dust_bound_short_q: 0u128,
            materialized_account_count: 0,
            last_oracle_price: init_oracle_price,
            last_market_slot: init_slot,
            funding_price_sample_last: init_oracle_price,
            used: [0; BITMAP_WORDS],
            num_used_accounts: 0,
            next_account_id: 0,
            free_head: 0,
            next_free: [0; MAX_ACCOUNTS],
            accounts: [empty_account(); MAX_ACCOUNTS],
        };

        for i in 0..MAX_ACCOUNTS - 1 {
            engine.next_free[i] = (i + 1) as u16;
        }
        engine.next_free[MAX_ACCOUNTS - 1] = u16::MAX;

        engine
    }

    /// Initialize in place (for Solana BPF zero-copy, spec §2.7).
    /// Fully canonicalizes all state — safe even on non-zeroed memory.
    pub fn init_in_place(&mut self, params: RiskParams, init_slot: u64, init_oracle_price: u64) {
        Self::validate_params(&params);
        assert!(
            init_oracle_price > 0 && init_oracle_price <= MAX_ORACLE_PRICE,
            "init_oracle_price must be in (0, MAX_ORACLE_PRICE] per spec §2.7"
        );
        self.vault = U128::ZERO;
        self.insurance_fund = InsuranceFund { balance: U128::ZERO };
        self.params = params;
        self.current_slot = init_slot;
        self.funding_rate_bps_per_slot_last = 0;
        self.last_crank_slot = 0;
        self.max_crank_staleness_slots = params.max_crank_staleness_slots;
        self.c_tot = U128::ZERO;
        self.pnl_pos_tot = 0;
        self.pnl_matured_pos_tot = 0;
        self.liq_cursor = 0;
        self.gc_cursor = 0;
        self.last_full_sweep_start_slot = 0;
        self.last_full_sweep_completed_slot = 0;
        self.crank_cursor = 0;
        self.sweep_start_idx = 0;
        self.lifetime_liquidations = 0;
        self.adl_mult_long = ADL_ONE;
        self.adl_mult_short = ADL_ONE;
        self.adl_coeff_long = 0;
        self.adl_coeff_short = 0;
        self.adl_epoch_long = 0;
        self.adl_epoch_short = 0;
        self.adl_epoch_start_k_long = 0;
        self.adl_epoch_start_k_short = 0;
        self.oi_eff_long_q = 0;
        self.oi_eff_short_q = 0;
        self.side_mode_long = SideMode::Normal;
        self.side_mode_short = SideMode::Normal;
        self.stored_pos_count_long = 0;
        self.stored_pos_count_short = 0;
        self.stale_account_count_long = 0;
        self.stale_account_count_short = 0;
        self.phantom_dust_bound_long_q = 0;
        self.phantom_dust_bound_short_q = 0;
        self.materialized_account_count = 0;
        self.last_oracle_price = init_oracle_price;
        self.last_market_slot = init_slot;
        self.funding_price_sample_last = init_oracle_price;
        // insurance_floor is now read directly from self.params.insurance_floor
        self.used = [0; BITMAP_WORDS];
        self.num_used_accounts = 0;
        self.next_account_id = 0;
        self.free_head = 0;
        self.accounts = [empty_account(); MAX_ACCOUNTS];
        for i in 0..MAX_ACCOUNTS - 1 {
            self.next_free[i] = (i + 1) as u16;
        }
        self.next_free[MAX_ACCOUNTS - 1] = u16::MAX;
    }

    // ========================================================================
    // Bitmap Helpers
    // ========================================================================

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

    #[allow(dead_code)]
    fn for_each_used_mut<F: FnMut(usize, &mut Account)>(&mut self, mut f: F) {
        for (block, word) in self.used.iter().copied().enumerate() {
            let mut w = word;
            while w != 0 {
                let bit = w.trailing_zeros() as usize;
                let idx = block * 64 + bit;
                w &= w - 1;
                if idx >= MAX_ACCOUNTS {
                    continue;
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
                w &= w - 1;
                if idx >= MAX_ACCOUNTS {
                    continue;
                }
                f(idx, &self.accounts[idx]);
            }
        }
    }

    // ========================================================================
    // Freelist
    // ========================================================================

    fn alloc_slot(&mut self) -> Result<u16> {
        if self.free_head == u16::MAX {
            return Err(RiskError::Overflow);
        }
        let idx = self.free_head;
        self.free_head = self.next_free[idx as usize];
        self.set_used(idx as usize);
        self.num_used_accounts = self.num_used_accounts.checked_add(1)
            .expect("num_used_accounts overflow — slot leak corruption");
        Ok(idx)
    }

    test_visible! {
    fn free_slot(&mut self, idx: u16) {
        self.accounts[idx as usize] = empty_account();
        self.clear_used(idx as usize);
        self.next_free[idx as usize] = self.free_head;
        self.free_head = idx;
        self.num_used_accounts = self.num_used_accounts.checked_sub(1)
            .expect("free_slot: num_used_accounts underflow — double-free corruption");
        // Decrement materialized_account_count (spec §2.1.2)
        self.materialized_account_count = self.materialized_account_count.checked_sub(1)
            .expect("free_slot: materialized_account_count underflow — double-free corruption");
    }
    }

    /// materialize_account(i, slot_anchor) — spec §2.5.
    /// Materializes a missing account at a specific slot index.
    /// The slot must not be currently in use.
    fn materialize_at(&mut self, idx: u16, slot_anchor: u64) -> Result<()> {
        if idx as usize >= MAX_ACCOUNTS {
            return Err(RiskError::AccountNotFound);
        }

        let used_count = self.num_used_accounts as u64;
        if used_count >= self.params.max_accounts {
            return Err(RiskError::Overflow);
        }

        // Enforce materialized_account_count bound (spec §10.0)
        self.materialized_account_count = self.materialized_account_count
            .checked_add(1).ok_or(RiskError::Overflow)?;
        if self.materialized_account_count > MAX_MATERIALIZED_ACCOUNTS {
            self.materialized_account_count -= 1;
            return Err(RiskError::Overflow);
        }

        // Remove idx from free list. Must succeed — if idx is not in the
        // freelist, the state is corrupt and we must not proceed.
        let mut found = false;
        if self.free_head == idx {
            self.free_head = self.next_free[idx as usize];
            found = true;
        } else {
            let mut prev = self.free_head;
            let mut steps = 0usize;
            while prev != u16::MAX && steps < MAX_ACCOUNTS {
                if self.next_free[prev as usize] == idx {
                    self.next_free[prev as usize] = self.next_free[idx as usize];
                    found = true;
                    break;
                }
                prev = self.next_free[prev as usize];
                steps += 1;
            }
        }
        if !found {
            // Roll back materialized_account_count
            self.materialized_account_count -= 1;
            return Err(RiskError::CorruptState);
        }

        self.set_used(idx as usize);
        self.num_used_accounts = self.num_used_accounts.checked_add(1)
            .expect("num_used_accounts overflow — slot leak corruption");

        let account_id = self.next_account_id;
        self.next_account_id = self.next_account_id.saturating_add(1);

        // Initialize per spec §2.5
        self.accounts[idx as usize] = Account {
            kind: Account::KIND_USER,
            account_id,
            capital: U128::ZERO,
            pnl: 0i128,
            reserved_pnl: 0u128,
            warmup_started_at_slot: slot_anchor,
            warmup_slope_per_step: 0u128,
            position_basis_q: 0i128,
            adl_a_basis: ADL_ONE,
            adl_k_snap: 0i128,
            adl_epoch_snap: 0,
            matcher_program: [0; 32],
            matcher_context: [0; 32],
            owner: [0; 32],
            fee_credits: I128::ZERO,
            last_fee_slot: slot_anchor,
            fees_earned_total: U128::ZERO,
        };

        Ok(())
    }

    // ========================================================================
    // O(1) Aggregate Helpers (spec §4)
    // ========================================================================

    /// set_pnl (spec §4.4): Update PNL and maintain pnl_pos_tot + pnl_matured_pos_tot
    /// with proper reserve handling. Forbids i128::MIN.
    test_visible! {
    fn set_pnl(&mut self, idx: usize, new_pnl: i128) {
        // Step 1: forbid i128::MIN
        assert!(new_pnl != i128::MIN, "set_pnl: i128::MIN forbidden");

        let old = self.accounts[idx].pnl;
        let old_pos = i128_clamp_pos(old);
        let old_r = self.accounts[idx].reserved_pnl;
        let old_rel = old_pos - old_r;
        let new_pos = i128_clamp_pos(new_pnl);

        // Step 6: per-account positive-PnL bound
        assert!(new_pos <= MAX_ACCOUNT_POSITIVE_PNL, "set_pnl: exceeds MAX_ACCOUNT_POSITIVE_PNL");

        // Steps 7-8: compute new_R
        let new_r = if new_pos > old_pos {
            // Step 7: positive increase → add to reserve
            let reserve_add = new_pos - old_pos;
            let nr = old_r.checked_add(reserve_add)
                .expect("set_pnl: new_R overflow");
            assert!(nr <= new_pos, "set_pnl: new_R > new_pos");
            nr
        } else {
            // Step 8: decrease or same → saturating_sub loss from reserve
            let pos_loss = old_pos - new_pos;
            let nr = old_r.saturating_sub(pos_loss);
            assert!(nr <= new_pos, "set_pnl: new_R > new_pos");
            nr
        };

        let new_rel = new_pos - new_r;

        // Steps 10-11: update pnl_pos_tot
        if new_pos > old_pos {
            let delta = new_pos - old_pos;
            self.pnl_pos_tot = self.pnl_pos_tot.checked_add(delta)
                .expect("set_pnl: pnl_pos_tot overflow");
        } else if old_pos > new_pos {
            let delta = old_pos - new_pos;
            self.pnl_pos_tot = self.pnl_pos_tot.checked_sub(delta)
                .expect("set_pnl: pnl_pos_tot underflow");
        }
        assert!(self.pnl_pos_tot <= MAX_PNL_POS_TOT, "set_pnl: exceeds MAX_PNL_POS_TOT");

        // Steps 12-13: update pnl_matured_pos_tot
        if new_rel > old_rel {
            let delta = new_rel - old_rel;
            self.pnl_matured_pos_tot = self.pnl_matured_pos_tot.checked_add(delta)
                .expect("set_pnl: pnl_matured_pos_tot overflow");
        } else if old_rel > new_rel {
            let delta = old_rel - new_rel;
            self.pnl_matured_pos_tot = self.pnl_matured_pos_tot.checked_sub(delta)
                .expect("set_pnl: pnl_matured_pos_tot underflow");
        }
        assert!(self.pnl_matured_pos_tot <= self.pnl_pos_tot,
            "set_pnl: pnl_matured_pos_tot > pnl_pos_tot");

        // Steps 14-15: write PNL_i and R_i
        self.accounts[idx].pnl = new_pnl;
        self.accounts[idx].reserved_pnl = new_r;
    }
    }

    /// set_reserved_pnl (spec §4.3): update R_i and maintain pnl_matured_pos_tot.
    test_visible! {
    fn set_reserved_pnl(&mut self, idx: usize, new_r: u128) {
        let pos = i128_clamp_pos(self.accounts[idx].pnl);
        assert!(new_r <= pos, "set_reserved_pnl: new_R > max(PNL_i, 0)");

        let old_r = self.accounts[idx].reserved_pnl;
        let old_rel = pos - old_r;
        let new_rel = pos - new_r;

        // Update pnl_matured_pos_tot by exact delta
        if new_rel > old_rel {
            let delta = new_rel - old_rel;
            self.pnl_matured_pos_tot = self.pnl_matured_pos_tot.checked_add(delta)
                .expect("set_reserved_pnl: pnl_matured_pos_tot overflow");
        } else if old_rel > new_rel {
            let delta = old_rel - new_rel;
            self.pnl_matured_pos_tot = self.pnl_matured_pos_tot.checked_sub(delta)
                .expect("set_reserved_pnl: pnl_matured_pos_tot underflow");
        }
        assert!(self.pnl_matured_pos_tot <= self.pnl_pos_tot,
            "set_reserved_pnl: pnl_matured_pos_tot > pnl_pos_tot");

        self.accounts[idx].reserved_pnl = new_r;
    }
    }

    /// consume_released_pnl (spec §4.4.1): remove only matured released positive PnL,
    /// leaving R_i unchanged.
    test_visible! {
    fn consume_released_pnl(&mut self, idx: usize, x: u128) {
        assert!(x > 0, "consume_released_pnl: x must be > 0");

        let old_pos = i128_clamp_pos(self.accounts[idx].pnl);
        let old_r = self.accounts[idx].reserved_pnl;
        let old_rel = old_pos - old_r;
        assert!(x <= old_rel, "consume_released_pnl: x > ReleasedPos_i");

        let new_pos = old_pos - x;
        let new_rel = old_rel - x;
        assert!(new_pos >= old_r, "consume_released_pnl: new_pos < old_R");

        // Update pnl_pos_tot
        self.pnl_pos_tot = self.pnl_pos_tot.checked_sub(x)
            .expect("consume_released_pnl: pnl_pos_tot underflow");

        // Update pnl_matured_pos_tot
        self.pnl_matured_pos_tot = self.pnl_matured_pos_tot.checked_sub(x)
            .expect("consume_released_pnl: pnl_matured_pos_tot underflow");
        assert!(self.pnl_matured_pos_tot <= self.pnl_pos_tot,
            "consume_released_pnl: pnl_matured_pos_tot > pnl_pos_tot");

        // PNL_i = checked_sub_i128(PNL_i, checked_cast_i128(x))
        let x_i128: i128 = x.try_into().expect("consume_released_pnl: x > i128::MAX");
        let new_pnl = self.accounts[idx].pnl.checked_sub(x_i128)
            .expect("consume_released_pnl: PNL underflow");
        assert!(new_pnl != i128::MIN, "consume_released_pnl: PNL == i128::MIN");
        self.accounts[idx].pnl = new_pnl;
        // R_i remains unchanged
    }
    }

    /// set_capital (spec §4.2): checked signed-delta update of C_tot
    test_visible! {
    fn set_capital(&mut self, idx: usize, new_capital: u128) {
        let old = self.accounts[idx].capital.get();
        if new_capital >= old {
            let delta = new_capital - old;
            self.c_tot = U128::new(self.c_tot.get().checked_add(delta)
                .expect("set_capital: c_tot overflow"));
        } else {
            let delta = old - new_capital;
            self.c_tot = U128::new(self.c_tot.get().checked_sub(delta)
                .expect("set_capital: c_tot underflow"));
        }
        self.accounts[idx].capital = U128::new(new_capital);
    }
    }

    /// set_position_basis_q (spec §4.4): update stored pos counts based on sign changes
    test_visible! {
    fn set_position_basis_q(&mut self, idx: usize, new_basis: i128) {
        let old = self.accounts[idx].position_basis_q;
        let old_side = side_of_i128(old);
        let new_side = side_of_i128(new_basis);

        // Decrement old side count
        if let Some(s) = old_side {
            match s {
                Side::Long => {
                    self.stored_pos_count_long = self.stored_pos_count_long
                        .checked_sub(1).expect("set_position_basis_q: long count underflow");
                }
                Side::Short => {
                    self.stored_pos_count_short = self.stored_pos_count_short
                        .checked_sub(1).expect("set_position_basis_q: short count underflow");
                }
            }
        }

        // Increment new side count
        if let Some(s) = new_side {
            match s {
                Side::Long => {
                    self.stored_pos_count_long = self.stored_pos_count_long
                        .checked_add(1).expect("set_position_basis_q: long count overflow");
                }
                Side::Short => {
                    self.stored_pos_count_short = self.stored_pos_count_short
                        .checked_add(1).expect("set_position_basis_q: short count overflow");
                }
            }
        }

        self.accounts[idx].position_basis_q = new_basis;
    }
    }

    /// attach_effective_position (spec §4.5)
    test_visible! {
    fn attach_effective_position(&mut self, idx: usize, new_eff_pos_q: i128) {
        // Before replacing a nonzero same-epoch basis, account for the fractional
        // remainder that will be orphaned (dynamic dust accounting).
        let old_basis = self.accounts[idx].position_basis_q;
        if old_basis != 0 {
            if let Some(old_side) = side_of_i128(old_basis) {
                let epoch_snap = self.accounts[idx].adl_epoch_snap;
                let epoch_side = self.get_epoch_side(old_side);
                if epoch_snap == epoch_side {
                    let a_basis = self.accounts[idx].adl_a_basis;
                    if a_basis != 0 {
                        let a_side = self.get_a_side(old_side);
                        let abs_basis = old_basis.unsigned_abs();
                        // Use U256 for the intermediate product to avoid u128 overflow
                        let product = U256::from_u128(abs_basis)
                            .checked_mul(U256::from_u128(a_side));
                        if let Some(p) = product {
                            let rem = p.checked_rem(U256::from_u128(a_basis));
                            if let Some(r) = rem {
                                if !r.is_zero() {
                                    self.inc_phantom_dust_bound(old_side);
                                }
                            }
                        }
                    }
                }
            }
        }

        if new_eff_pos_q == 0 {
            self.set_position_basis_q(idx, 0i128);
            // Reset to canonical zero-position defaults (spec §2.4)
            self.accounts[idx].adl_a_basis = ADL_ONE;
            self.accounts[idx].adl_k_snap = 0i128;
            self.accounts[idx].adl_epoch_snap = 0;
        } else {
            // Spec §4.6: abs(new_eff_pos_q) <= MAX_POSITION_ABS_Q
            assert!(
                new_eff_pos_q.unsigned_abs() <= MAX_POSITION_ABS_Q,
                "attach: abs(new_eff_pos_q) exceeds MAX_POSITION_ABS_Q"
            );
            let side = side_of_i128(new_eff_pos_q).expect("attach: nonzero must have side");
            self.set_position_basis_q(idx, new_eff_pos_q);

            match side {
                Side::Long => {
                    self.accounts[idx].adl_a_basis = self.adl_mult_long;
                    self.accounts[idx].adl_k_snap = self.adl_coeff_long;
                    self.accounts[idx].adl_epoch_snap = self.adl_epoch_long;
                }
                Side::Short => {
                    self.accounts[idx].adl_a_basis = self.adl_mult_short;
                    self.accounts[idx].adl_k_snap = self.adl_coeff_short;
                    self.accounts[idx].adl_epoch_snap = self.adl_epoch_short;
                }
            }
        }
    }
    }

    // ========================================================================
    // Side state accessors
    // ========================================================================

    fn get_a_side(&self, s: Side) -> u128 {
        match s {
            Side::Long => self.adl_mult_long,
            Side::Short => self.adl_mult_short,
        }
    }

    fn get_k_side(&self, s: Side) -> i128 {
        match s {
            Side::Long => self.adl_coeff_long,
            Side::Short => self.adl_coeff_short,
        }
    }

    fn get_epoch_side(&self, s: Side) -> u64 {
        match s {
            Side::Long => self.adl_epoch_long,
            Side::Short => self.adl_epoch_short,
        }
    }

    fn get_k_epoch_start(&self, s: Side) -> i128 {
        match s {
            Side::Long => self.adl_epoch_start_k_long,
            Side::Short => self.adl_epoch_start_k_short,
        }
    }

    fn get_side_mode(&self, s: Side) -> SideMode {
        match s {
            Side::Long => self.side_mode_long,
            Side::Short => self.side_mode_short,
        }
    }

    fn get_oi_eff(&self, s: Side) -> u128 {
        match s {
            Side::Long => self.oi_eff_long_q,
            Side::Short => self.oi_eff_short_q,
        }
    }

    fn set_oi_eff(&mut self, s: Side, v: u128) {
        match s {
            Side::Long => self.oi_eff_long_q = v,
            Side::Short => self.oi_eff_short_q = v,
        }
    }

    fn set_side_mode(&mut self, s: Side, m: SideMode) {
        match s {
            Side::Long => self.side_mode_long = m,
            Side::Short => self.side_mode_short = m,
        }
    }

    fn set_a_side(&mut self, s: Side, v: u128) {
        match s {
            Side::Long => self.adl_mult_long = v,
            Side::Short => self.adl_mult_short = v,
        }
    }

    fn set_k_side(&mut self, s: Side, v: i128) {
        match s {
            Side::Long => self.adl_coeff_long = v,
            Side::Short => self.adl_coeff_short = v,
        }
    }

    fn get_stale_count(&self, s: Side) -> u64 {
        match s {
            Side::Long => self.stale_account_count_long,
            Side::Short => self.stale_account_count_short,
        }
    }

    fn set_stale_count(&mut self, s: Side, v: u64) {
        match s {
            Side::Long => self.stale_account_count_long = v,
            Side::Short => self.stale_account_count_short = v,
        }
    }

    fn get_stored_pos_count(&self, s: Side) -> u64 {
        match s {
            Side::Long => self.stored_pos_count_long,
            Side::Short => self.stored_pos_count_short,
        }
    }

    /// Spec §4.6: increment phantom dust bound by 1 q-unit (checked).
    fn inc_phantom_dust_bound(&mut self, s: Side) {
        match s {
            Side::Long => {
                self.phantom_dust_bound_long_q = self.phantom_dust_bound_long_q
                    .checked_add(1u128)
                    .expect("phantom_dust_bound_long_q overflow");
            }
            Side::Short => {
                self.phantom_dust_bound_short_q = self.phantom_dust_bound_short_q
                    .checked_add(1u128)
                    .expect("phantom_dust_bound_short_q overflow");
            }
        }
    }

    /// Spec §4.6.1: increment phantom dust bound by amount_q (checked).
    fn inc_phantom_dust_bound_by(&mut self, s: Side, amount_q: u128) {
        match s {
            Side::Long => {
                self.phantom_dust_bound_long_q = self.phantom_dust_bound_long_q
                    .checked_add(amount_q)
                    .expect("phantom_dust_bound_long_q overflow");
            }
            Side::Short => {
                self.phantom_dust_bound_short_q = self.phantom_dust_bound_short_q
                    .checked_add(amount_q)
                    .expect("phantom_dust_bound_short_q overflow");
            }
        }
    }

    // ========================================================================
    // effective_pos_q (spec §5.2)
    // ========================================================================

    /// Compute effective position quantity for account idx.
    pub fn effective_pos_q(&self, idx: usize) -> i128 {
        let basis = self.accounts[idx].position_basis_q;
        if basis == 0 {
            return 0i128;
        }

        let side = side_of_i128(basis).unwrap();
        let epoch_snap = self.accounts[idx].adl_epoch_snap;
        let epoch_side = self.get_epoch_side(side);

        if epoch_snap != epoch_side {
            // Epoch mismatch → effective position is 0 for current-market risk
            return 0i128;
        }

        let a_side = self.get_a_side(side);
        let a_basis = self.accounts[idx].adl_a_basis;

        if a_basis == 0 {
            return 0i128;
        }

        let abs_basis = basis.unsigned_abs();
        // floor(|basis| * A_s / a_basis)
        let effective_abs = mul_div_floor_u128(abs_basis, a_side, a_basis);

        if basis < 0 {
            if effective_abs == 0 {
                0i128
            } else {
                assert!(effective_abs <= i128::MAX as u128, "effective_pos_q: overflow");
                -(effective_abs as i128)
            }
        } else {
            assert!(effective_abs <= i128::MAX as u128, "effective_pos_q: overflow");
            effective_abs as i128
        }
    }

    // ========================================================================
    // settle_side_effects (spec §5.3)
    // ========================================================================

    test_visible! {
    fn settle_side_effects(&mut self, idx: usize) -> Result<()> {
        let basis = self.accounts[idx].position_basis_q;
        if basis == 0 {
            return Ok(());
        }

        let side = side_of_i128(basis).unwrap();
        let epoch_snap = self.accounts[idx].adl_epoch_snap;
        let epoch_side = self.get_epoch_side(side);
        let a_basis = self.accounts[idx].adl_a_basis;

        if a_basis == 0 {
            return Err(RiskError::CorruptState);
        }

        let abs_basis = basis.unsigned_abs();

        if epoch_snap == epoch_side {
            // Same epoch (spec §5.3 step 4)
            let a_side = self.get_a_side(side);
            let k_side = self.get_k_side(side);
            let k_snap = self.accounts[idx].adl_k_snap;

            // q_eff_new = floor(|basis| * A_s / a_basis)
            let q_eff_new = mul_div_floor_u128(abs_basis, a_side, a_basis);

            // Record old_R before set_pnl (spec §5.3)
            let old_r = self.accounts[idx].reserved_pnl;

            // pnl_delta (spec §5.3 step 4: k_then=k_snap, k_now=K_s)
            let den = a_basis.checked_mul(POS_SCALE).ok_or(RiskError::Overflow)?;
            let pnl_delta = wide_signed_mul_div_floor_from_k_pair(abs_basis, k_snap, k_side, den);

            let old_pnl = self.accounts[idx].pnl;
            let new_pnl = old_pnl.checked_add(pnl_delta).ok_or(RiskError::Overflow)?;
            if new_pnl == i128::MIN {
                return Err(RiskError::Overflow);
            }
            self.set_pnl(idx, new_pnl);

            // Caller obligation: if R_i increased, restart warmup (spec §4.4 / §5.3)
            if self.accounts[idx].reserved_pnl > old_r {
                self.restart_warmup_after_reserve_increase(idx);
            }

            if q_eff_new == 0 {
                // Position effectively zeroed (spec §5.3 step 4)
                // Reset to canonical zero-position defaults (spec §2.4)
                self.inc_phantom_dust_bound(side);
                self.set_position_basis_q(idx, 0i128);
                self.accounts[idx].adl_a_basis = ADL_ONE;
                self.accounts[idx].adl_k_snap = 0i128;
                self.accounts[idx].adl_epoch_snap = 0;
            } else {
                // Update k_snap only; do NOT change basis or a_basis (non-compounding)
                self.accounts[idx].adl_k_snap = k_side;
                self.accounts[idx].adl_epoch_snap = epoch_side;
            }
        } else {
            // Epoch mismatch (spec §5.3 step 5)
            // Validate-then-mutate: all fallible checks before any mutation.
            let side_mode = self.get_side_mode(side);
            if side_mode != SideMode::ResetPending {
                return Err(RiskError::CorruptState);
            }
            if epoch_snap.checked_add(1) != Some(epoch_side) {
                return Err(RiskError::CorruptState);
            }

            let k_epoch_start = self.get_k_epoch_start(side);
            let k_snap = self.accounts[idx].adl_k_snap;

            // Phase 1: COMPUTE + VALIDATE (no mutations)
            let den = a_basis.checked_mul(POS_SCALE).ok_or(RiskError::Overflow)?;
            let pnl_delta = wide_signed_mul_div_floor_from_k_pair(abs_basis, k_snap, k_epoch_start, den);

            let new_pnl = self.accounts[idx].pnl.checked_add(pnl_delta).ok_or(RiskError::Overflow)?;
            if new_pnl == i128::MIN {
                return Err(RiskError::Overflow);
            }

            let old_stale = self.get_stale_count(side);
            let new_stale = old_stale.checked_sub(1).ok_or(RiskError::CorruptState)?;

            // Phase 2: MUTATE (all validated)
            let old_r = self.accounts[idx].reserved_pnl;
            self.set_pnl(idx, new_pnl);

            if self.accounts[idx].reserved_pnl > old_r {
                self.restart_warmup_after_reserve_increase(idx);
            }

            self.set_position_basis_q(idx, 0i128);
            self.set_stale_count(side, new_stale);

            // Reset to canonical zero-position defaults (spec §2.4)
            self.accounts[idx].adl_a_basis = ADL_ONE;
            self.accounts[idx].adl_k_snap = 0i128;
            self.accounts[idx].adl_epoch_snap = 0;
        }

        Ok(())
    }
    }

    // ========================================================================
    // accrue_market_to (spec §5.4)
    // ========================================================================

    pub fn accrue_market_to(&mut self, now_slot: u64, oracle_price: u64) -> Result<()> {
        if oracle_price == 0 || oracle_price > MAX_ORACLE_PRICE {
            return Err(RiskError::Overflow);
        }

        // Time monotonicity (spec §5.4 preconditions)
        if now_slot < self.current_slot {
            return Err(RiskError::Overflow);
        }
        if now_slot < self.last_market_slot {
            return Err(RiskError::Overflow);
        }

        // Step 4: snapshot OI at start (fixed for all sub-steps per spec §5.4)
        let long_live = self.oi_eff_long_q != 0;
        let short_live = self.oi_eff_short_q != 0;

        let total_dt = now_slot.saturating_sub(self.last_market_slot);
        if total_dt == 0 && self.last_oracle_price == oracle_price {
            // Step 5: no change — set current_slot and return (spec §5.4)
            self.current_slot = now_slot;
            return Ok(());
        }

        // Use scratch K values for the entire mark + funding computation.
        // Only commit to engine state after ALL computations succeed.
        // This prevents partial K advancement on mid-function errors.
        let mut k_long = self.adl_coeff_long;
        let mut k_short = self.adl_coeff_short;

        // Step 5: Mark-to-market (once, spec §1.5 item 21)
        let current_price = self.last_oracle_price;
        let delta_p = (oracle_price as i128).checked_sub(current_price as i128)
            .ok_or(RiskError::Overflow)?;
        if delta_p != 0 {
            if long_live {
                let dk = checked_u128_mul_i128(self.adl_mult_long, delta_p)?;
                k_long = k_long.checked_add(dk).ok_or(RiskError::Overflow)?;
            }
            if short_live {
                let dk = checked_u128_mul_i128(self.adl_mult_short, delta_p)?;
                k_short = k_short.checked_sub(dk).ok_or(RiskError::Overflow)?;
            }
        }

        // Step 6: Funding transfer via sub-stepping (spec v12.1.0 §5.4)
        let r_last = self.funding_rate_bps_per_slot_last;
        if r_last != 0 && total_dt > 0 && long_live && short_live {
            let fund_px_0 = self.funding_price_sample_last;

            if fund_px_0 > 0 {
                let mut dt_remaining = total_dt;

                while dt_remaining > 0 {
                    let dt_sub = core::cmp::min(dt_remaining, MAX_FUNDING_DT);
                    dt_remaining -= dt_sub;

                    let fund_num: i128 = (fund_px_0 as i128)
                        .checked_mul(r_last as i128)
                        .ok_or(RiskError::Overflow)?
                        .checked_mul(dt_sub as i128)
                        .ok_or(RiskError::Overflow)?;

                    let fund_term = floor_div_signed_conservative_i128(fund_num, 10_000u128);

                    if fund_term != 0 {
                        let dk_long = checked_u128_mul_i128(self.adl_mult_long, fund_term)?;
                        k_long = k_long.checked_sub(dk_long).ok_or(RiskError::Overflow)?;
                        let dk_short = checked_u128_mul_i128(self.adl_mult_short, fund_term)?;
                        k_short = k_short.checked_add(dk_short).ok_or(RiskError::Overflow)?;
                    }
                }
            }
        }

        // ALL computations succeeded — commit K values and synchronize state
        self.adl_coeff_long = k_long;
        self.adl_coeff_short = k_short;
        self.current_slot = now_slot;
        self.last_market_slot = now_slot;
        self.last_oracle_price = oracle_price;
        self.funding_price_sample_last = oracle_price;

        Ok(())
    }

    /// Pre-validate funding rate bound (called at top of each instruction,
    /// before any mutations, so bad rates never cause partial-mutation errors).
    fn validate_funding_rate(rate: i64) -> Result<()> {
        if rate.unsigned_abs() > MAX_ABS_FUNDING_BPS_PER_SLOT as u64 {
            return Err(RiskError::Overflow);
        }
        Ok(())
    }

    /// recompute_r_last_from_final_state (spec v12.1.0 §4.12).
    /// Stores the pre-validated funding rate for the next interval.
    test_visible! {
    fn recompute_r_last_from_final_state(&mut self, externally_computed_rate: i64) -> Result<()> {
        // Rate already validated at instruction entry; store unconditionally.
        // Belt-and-suspenders: re-check here too.
        if externally_computed_rate.unsigned_abs() > MAX_ABS_FUNDING_BPS_PER_SLOT as u64 {
            return Err(RiskError::Overflow);
        }
        self.funding_rate_bps_per_slot_last = externally_computed_rate;
        Ok(())
    }
    }

    /// Public entry-point for the end-of-instruction lifecycle
    /// (spec §10.0 steps 4-7 / §10.8 steps 9-12).
    ///
    /// Runs schedule_end_of_instruction_resets, finalize, and
    /// recompute_r_last_from_final_state in the canonical order.
    /// Callers that bypass `keeper_crank_not_atomic` (e.g. the resolved-market
    /// settlement crank) must invoke this before returning.
    pub fn run_end_of_instruction_lifecycle(&mut self, ctx: &mut InstructionContext, funding_rate: i64) -> Result<()> {
                Self::validate_funding_rate(funding_rate)?;

        self.schedule_end_of_instruction_resets(ctx)?;
        self.finalize_end_of_instruction_resets(ctx);
        self.recompute_r_last_from_final_state(funding_rate)?;
        Ok(())
    }

    // ========================================================================
    // absorb_protocol_loss (spec §4.7)
    // ========================================================================

    /// use_insurance_buffer (spec §4.11): deduct loss from insurance down to floor,
    /// return the remaining uninsured loss.
    fn use_insurance_buffer(&mut self, loss: u128) -> u128 {
        if loss == 0 {
            return 0;
        }
        let ins_bal = self.insurance_fund.balance.get();
        let available = ins_bal.saturating_sub(self.params.insurance_floor.get());
        let pay = core::cmp::min(loss, available);
        if pay > 0 {
            self.insurance_fund.balance = U128::new(ins_bal - pay);
        }
        loss - pay
    }

    /// absorb_protocol_loss (spec §4.11): use_insurance_buffer then record
    /// any remaining uninsured loss as implicit haircut.
    test_visible! {
    fn absorb_protocol_loss(&mut self, loss: u128) {
        if loss == 0 {
            return;
        }
        let _rem = self.use_insurance_buffer(loss);
        // Remaining loss is implicit haircut through h
    }
    }

    // ========================================================================
    // enqueue_adl (spec §5.6)
    // ========================================================================

    test_visible! {
    fn enqueue_adl(&mut self, ctx: &mut InstructionContext, liq_side: Side, q_close_q: u128, d: u128) -> Result<()> {
        let opp = opposite_side(liq_side);

        // Step 1: decrease liquidated side OI (checked — underflow is corrupt state)
        if q_close_q != 0 {
            let old_oi = self.get_oi_eff(liq_side);
            let new_oi = old_oi.checked_sub(q_close_q).ok_or(RiskError::CorruptState)?;
            self.set_oi_eff(liq_side, new_oi);
        }

        // Step 2 (§5.6 step 2): insurance-first deficit coverage
        let d_rem = if d > 0 { self.use_insurance_buffer(d) } else { 0u128 };

        // Step 3: read opposing OI
        let oi = self.get_oi_eff(opp);

        // Step 4 (§5.6 step 4): if OI == 0
        if oi == 0 {
            // D_rem > 0 → record_uninsured_protocol_loss (implicit through h, no-op)
            if self.get_oi_eff(liq_side) == 0 {
                set_pending_reset(ctx, liq_side);
                set_pending_reset(ctx, opp);
            }
            return Ok(());
        }

        // Step 5 (§5.6 step 5): if OI > 0 and stored_pos_count_opp == 0,
        // route deficit through record_uninsured and do NOT modify K_opp.
        if self.get_stored_pos_count(opp) == 0 {
            if q_close_q > oi {
                return Err(RiskError::CorruptState);
            }
            let oi_post = oi.checked_sub(q_close_q).ok_or(RiskError::Overflow)?;
            // D_rem > 0 → record_uninsured_protocol_loss (implicit through h, no-op)
            self.set_oi_eff(opp, oi_post);
            if oi_post == 0 {
                // Unconditionally reset the drained opp side (fixes phantom dust revert).
                set_pending_reset(ctx, opp);
                // Also reset liq_side only if it too has zero OI
                if self.get_oi_eff(liq_side) == 0 {
                    set_pending_reset(ctx, liq_side);
                }
            }
            return Ok(());
        }

        // Step 6 (§5.6 step 6): require q_close_q <= OI
        if q_close_q > oi {
            return Err(RiskError::CorruptState);
        }

        let a_old = self.get_a_side(opp);
        let oi_post = oi.checked_sub(q_close_q).ok_or(RiskError::Overflow)?;

        // Step 7 (§5.6 step 7): handle D_rem > 0 (quote deficit after insurance)
        // Fused delta_K_abs = ceil(D_rem * A_old * POS_SCALE / OI)
        // Per §1.5 Rule 14: if the quotient doesn't fit in i128, route to
        // record_uninsured_protocol_loss instead of panicking.
        if d_rem != 0 {
            let a_ps = a_old.checked_mul(POS_SCALE).ok_or(RiskError::Overflow)?;
            match wide_mul_div_ceil_u128_or_over_i128max(d_rem, a_ps, oi) {
                Ok(delta_k_abs) => {
                    let delta_k = -(delta_k_abs as i128);
                    let k_opp = self.get_k_side(opp);
                    match k_opp.checked_add(delta_k) {
                        Some(new_k) => {
                            self.set_k_side(opp, new_k);
                        }
                        None => {
                            // K-space overflow: record_uninsured (no-op)
                        }
                    }
                }
                Err(OverI128Magnitude) => {
                    // Quotient overflow: record_uninsured (no-op)
                }
            }
        }

        // Step 8 (§5.6 step 8): if OI_post == 0
        if oi_post == 0 {
            self.set_oi_eff(opp, 0u128);
            set_pending_reset(ctx, opp);
            if self.get_oi_eff(liq_side) == 0 {
                set_pending_reset(ctx, liq_side);
            }
            return Ok(());
        }

        // Steps 8-9: compute A_candidate and A_trunc_rem using U256 intermediates
        let a_old_u256 = U256::from_u128(a_old);
        let oi_post_u256 = U256::from_u128(oi_post);
        let oi_u256 = U256::from_u128(oi);
        let (a_candidate_u256, a_trunc_rem) = mul_div_floor_u256_with_rem(
            a_old_u256,
            oi_post_u256,
            oi_u256,
        );

        // Step 10: A_candidate > 0
        if !a_candidate_u256.is_zero() {
            let a_new = a_candidate_u256.try_into_u128().expect("A_candidate exceeds u128");
            self.set_a_side(opp, a_new);
            self.set_oi_eff(opp, oi_post);
            // Only account for global A-truncation dust when actual truncation occurs
            if !a_trunc_rem.is_zero() {
                let n_opp = self.get_stored_pos_count(opp) as u128;
                let n_opp_u256 = U256::from_u128(n_opp);
                // global_a_dust_bound = N_opp + ceil((OI + N_opp) / A_old)
                let oi_plus_n = oi_u256.checked_add(n_opp_u256).unwrap_or(U256::MAX);
                let ceil_term = ceil_div_positive_checked(oi_plus_n, a_old_u256);
                let global_a_dust_bound = n_opp_u256.checked_add(ceil_term)
                    .unwrap_or(U256::MAX);
                let bound_u128 = global_a_dust_bound.try_into_u128().unwrap_or(u128::MAX);
                self.inc_phantom_dust_bound_by(opp, bound_u128);
            }
            if a_new < MIN_A_SIDE {
                self.set_side_mode(opp, SideMode::DrainOnly);
            }
            return Ok(());
        }

        // Step 11: precision exhaustion terminal drain
        self.set_oi_eff(opp, 0u128);
        self.set_oi_eff(liq_side, 0u128);
        set_pending_reset(ctx, opp);
        set_pending_reset(ctx, liq_side);

        Ok(())
    }
    }

    // ========================================================================
    // begin_full_drain_reset / finalize_side_reset (spec §2.5, §2.7)
    // ========================================================================

    test_visible! {
    fn begin_full_drain_reset(&mut self, side: Side) {
        // Require OI_eff_side == 0
        assert!(self.get_oi_eff(side) == 0, "begin_full_drain_reset: OI not zero");

        // K_epoch_start_side = K_side
        let k = self.get_k_side(side);
        match side {
            Side::Long => self.adl_epoch_start_k_long = k,
            Side::Short => self.adl_epoch_start_k_short = k,
        }

        // Increment epoch
        match side {
            Side::Long => self.adl_epoch_long = self.adl_epoch_long.checked_add(1)
                .expect("epoch overflow"),
            Side::Short => self.adl_epoch_short = self.adl_epoch_short.checked_add(1)
                .expect("epoch overflow"),
        }

        // A_side = ADL_ONE
        self.set_a_side(side, ADL_ONE);

        // stale_account_count_side = stored_pos_count_side
        let spc = self.get_stored_pos_count(side);
        self.set_stale_count(side, spc);

        // phantom_dust_bound_side_q = 0 (spec §2.5 step 6)
        match side {
            Side::Long => self.phantom_dust_bound_long_q = 0u128,
            Side::Short => self.phantom_dust_bound_short_q = 0u128,
        }

        // mode = ResetPending
        self.set_side_mode(side, SideMode::ResetPending);
    }
    }

    test_visible! {
    fn finalize_side_reset(&mut self, side: Side) -> Result<()> {
        if self.get_side_mode(side) != SideMode::ResetPending {
            return Err(RiskError::CorruptState);
        }
        if self.get_oi_eff(side) != 0 {
            return Err(RiskError::CorruptState);
        }
        if self.get_stale_count(side) != 0 {
            return Err(RiskError::CorruptState);
        }
        if self.get_stored_pos_count(side) != 0 {
            return Err(RiskError::CorruptState);
        }
        self.set_side_mode(side, SideMode::Normal);
        Ok(())
    }
    }

    // ========================================================================
    // schedule_end_of_instruction_resets / finalize (spec §5.7-5.8)
    // ========================================================================

    test_visible! {
    fn schedule_end_of_instruction_resets(&mut self, ctx: &mut InstructionContext) -> Result<()> {
        // §5.7.A: Bilateral-empty dust clearance
        if self.stored_pos_count_long == 0 && self.stored_pos_count_short == 0 {
            let clear_bound_q = self.phantom_dust_bound_long_q
                .checked_add(self.phantom_dust_bound_short_q)
                .ok_or(RiskError::CorruptState)?;
            let has_residual = self.oi_eff_long_q != 0
                || self.oi_eff_short_q != 0
                || self.phantom_dust_bound_long_q != 0
                || self.phantom_dust_bound_short_q != 0;
            if has_residual {
                if self.oi_eff_long_q != self.oi_eff_short_q {
                    return Err(RiskError::CorruptState);
                }
                if self.oi_eff_long_q <= clear_bound_q && self.oi_eff_short_q <= clear_bound_q {
                    self.oi_eff_long_q = 0u128;
                    self.oi_eff_short_q = 0u128;
                    ctx.pending_reset_long = true;
                    ctx.pending_reset_short = true;
                } else {
                    return Err(RiskError::CorruptState);
                }
            }
        }
        // §5.7.B: Unilateral-empty long (long empty, short has positions)
        else if self.stored_pos_count_long == 0 && self.stored_pos_count_short > 0 {
            let has_residual = self.oi_eff_long_q != 0
                || self.oi_eff_short_q != 0
                || self.phantom_dust_bound_long_q != 0;
            if has_residual {
                if self.oi_eff_long_q != self.oi_eff_short_q {
                    return Err(RiskError::CorruptState);
                }
                if self.oi_eff_long_q <= self.phantom_dust_bound_long_q {
                    self.oi_eff_long_q = 0u128;
                    self.oi_eff_short_q = 0u128;
                    ctx.pending_reset_long = true;
                    ctx.pending_reset_short = true;
                } else {
                    return Err(RiskError::CorruptState);
                }
            }
        }
        // §5.7.C: Unilateral-empty short (short empty, long has positions)
        else if self.stored_pos_count_short == 0 && self.stored_pos_count_long > 0 {
            let has_residual = self.oi_eff_long_q != 0
                || self.oi_eff_short_q != 0
                || self.phantom_dust_bound_short_q != 0;
            if has_residual {
                if self.oi_eff_long_q != self.oi_eff_short_q {
                    return Err(RiskError::CorruptState);
                }
                if self.oi_eff_short_q <= self.phantom_dust_bound_short_q {
                    self.oi_eff_long_q = 0u128;
                    self.oi_eff_short_q = 0u128;
                    ctx.pending_reset_long = true;
                    ctx.pending_reset_short = true;
                } else {
                    return Err(RiskError::CorruptState);
                }
            }
        }

        // §5.7.D: DrainOnly sides with zero OI
        if self.side_mode_long == SideMode::DrainOnly && self.oi_eff_long_q == 0 {
            ctx.pending_reset_long = true;
        }
        if self.side_mode_short == SideMode::DrainOnly && self.oi_eff_short_q == 0 {
            ctx.pending_reset_short = true;
        }

        Ok(())
    }
    }

    test_visible! {
    fn finalize_end_of_instruction_resets(&mut self, ctx: &InstructionContext) {
        if ctx.pending_reset_long && self.side_mode_long != SideMode::ResetPending {
            self.begin_full_drain_reset(Side::Long);
        }
        if ctx.pending_reset_short && self.side_mode_short != SideMode::ResetPending {
            self.begin_full_drain_reset(Side::Short);
        }
        // Auto-finalize sides that are fully ready for reopening
        self.maybe_finalize_ready_reset_sides();
    }
    }

    /// Preflight finalize: if a side is ResetPending with OI=0, stale=0, pos_count=0,
    /// transition it back to Normal so fresh OI can be added.
    /// Called before OI-increase gating and at end-of-instruction.
    fn maybe_finalize_ready_reset_sides(&mut self) {
        if self.side_mode_long == SideMode::ResetPending
            && self.get_oi_eff(Side::Long) == 0
            && self.get_stale_count(Side::Long) == 0
            && self.get_stored_pos_count(Side::Long) == 0
        {
            self.set_side_mode(Side::Long, SideMode::Normal);
        }
        if self.side_mode_short == SideMode::ResetPending
            && self.get_oi_eff(Side::Short) == 0
            && self.get_stale_count(Side::Short) == 0
            && self.get_stored_pos_count(Side::Short) == 0
        {
            self.set_side_mode(Side::Short, SideMode::Normal);
        }
    }

    // ========================================================================
    // Haircut and Equity (spec §3)
    // ========================================================================

    /// Compute haircut ratio (h_num, h_den) as u128 pair (spec §3.3)
    /// Uses pnl_matured_pos_tot as denominator per v12.1.0.
    pub fn haircut_ratio(&self) -> (u128, u128) {
        if self.pnl_matured_pos_tot == 0 {
            return (1u128, 1u128);
        }
        let senior_sum = self.c_tot.get().checked_add(self.insurance_fund.balance.get());
        let residual: u128 = match senior_sum {
            Some(ss) => {
                if self.vault.get() >= ss {
                    self.vault.get() - ss
                } else {
                    0u128
                }
            }
            None => 0u128, // overflow in senior_sum → deficit
        };
        let h_num = if residual < self.pnl_matured_pos_tot { residual } else { self.pnl_matured_pos_tot };
        (h_num, self.pnl_matured_pos_tot)
    }

    /// PNL_eff_matured_i (spec §3.3): haircutted matured released positive PnL
    pub fn effective_matured_pnl(&self, idx: usize) -> u128 {
        let released = self.released_pos(idx);
        if released == 0 {
            return 0u128;
        }
        let (h_num, h_den) = self.haircut_ratio();
        if h_den == 0 {
            return released;
        }
        wide_mul_div_floor_u128(released, h_num, h_den)
    }

    /// Eq_maint_raw_i (spec §3.4): C_i + PNL_i - FeeDebt_i in exact widened signed domain.
    /// For maintenance margin and one-sided health checks. Uses full local PNL_i.
    /// Returns i128. Negative overflow is projected to i128::MIN + 1 per §3.4
    /// (safe for one-sided checks against nonneg thresholds). For strict
    /// before/after buffer comparisons, use account_equity_maint_raw_wide.
    pub fn account_equity_maint_raw(&self, account: &Account) -> i128 {
        let wide = self.account_equity_maint_raw_wide(account);
        match wide.try_into_i128() {
            Some(v) => v,
            None => {
                // Positive overflow: unreachable under configured bounds (spec §3.4),
                // but MUST fail conservatively — account is over-collateralized,
                // so project to i128::MAX to prevent false liquidation.
                // Negative overflow: project to i128::MIN + 1 per spec §3.4.
                if wide.is_negative() { i128::MIN + 1 } else { i128::MAX }
            }
        }
    }

    /// Eq_maint_raw_i in exact I256 (spec §3.4 "transient widened signed type").
    /// MUST be used for strict before/after raw maintenance-buffer comparisons
    /// (§10.5 step 29). No saturation or clamping.
    pub fn account_equity_maint_raw_wide(&self, account: &Account) -> I256 {
        let cap = I256::from_u128(account.capital.get());
        let pnl = I256::from_i128(account.pnl);
        let fee_debt = I256::from_u128(fee_debt_u128_checked(account.fee_credits.get()));

        // C + PNL - FeeDebt in exact I256 — cannot overflow 256 bits
        let sum = cap.checked_add(pnl).expect("I256 add overflow");
        sum.checked_sub(fee_debt).expect("I256 sub overflow")
    }

    /// Eq_net_i (spec §3.4): max(0, Eq_maint_raw_i). For maintenance margin checks.
    pub fn account_equity_net(&self, account: &Account, _oracle_price: u64) -> i128 {
        let raw = self.account_equity_maint_raw(account);
        if raw < 0 { 0i128 } else { raw }
    }

    /// Eq_init_raw_i (spec §3.4): C_i + min(PNL_i, 0) + PNL_eff_matured_i - FeeDebt_i
    /// For initial margin and withdrawal checks. Uses haircutted matured PnL only.
    /// Returns i128. Negative overflow projected to i128::MIN + 1 per §3.4.
    pub fn account_equity_init_raw(&self, account: &Account, idx: usize) -> i128 {
        let cap = I256::from_u128(account.capital.get());
        let neg_pnl = I256::from_i128(if account.pnl < 0 { account.pnl } else { 0i128 });
        let eff_matured = I256::from_u128(self.effective_matured_pnl(idx));
        let fee_debt = I256::from_u128(fee_debt_u128_checked(account.fee_credits.get()));

        let sum = cap.checked_add(neg_pnl).expect("I256 add overflow")
            .checked_add(eff_matured).expect("I256 add overflow")
            .checked_sub(fee_debt).expect("I256 sub overflow");

        match sum.try_into_i128() {
            Some(v) => v,
            None => {
                // Positive overflow: unreachable under configured bounds (spec §3.4),
                // but MUST fail conservatively — project to i128::MAX.
                // Negative overflow: project to i128::MIN + 1 per spec §3.4.
                if sum.is_negative() { i128::MIN + 1 } else { i128::MAX }
            }
        }
    }

    /// Eq_init_net_i (spec §3.4): max(0, Eq_init_raw_i). For IM/withdrawal checks.
    pub fn account_equity_init_net(&self, account: &Account, idx: usize) -> i128 {
        let raw = self.account_equity_init_raw(account, idx);
        if raw < 0 { 0i128 } else { raw }
    }

    /// notional (spec §9.1): floor(|effective_pos_q| * oracle_price / POS_SCALE)
    pub fn notional(&self, idx: usize, oracle_price: u64) -> u128 {
        let eff = self.effective_pos_q(idx);
        if eff == 0 {
            return 0;
        }
        let abs_eff = eff.unsigned_abs();
        mul_div_floor_u128(abs_eff, oracle_price as u128, POS_SCALE)
    }

    /// is_above_maintenance_margin (spec §9.1): Eq_net_i > MM_req_i
    /// Per spec §9.1: if eff == 0 then MM_req = 0; else MM_req = max(proportional, MIN_NONZERO_MM_REQ)
    pub fn is_above_maintenance_margin(&self, account: &Account, idx: usize, oracle_price: u64) -> bool {
        let eq_net = self.account_equity_net(account, oracle_price);
        let eff = self.effective_pos_q(idx);
        if eff == 0 {
            return eq_net > 0;
        }
        let not = self.notional(idx, oracle_price);
        let proportional = mul_div_floor_u128(not, self.params.maintenance_margin_bps as u128, 10_000);
        let mm_req = core::cmp::max(proportional, self.params.min_nonzero_mm_req);
        let mm_req_i128 = if mm_req > i128::MAX as u128 { i128::MAX } else { mm_req as i128 };
        eq_net > mm_req_i128
    }

    /// is_above_initial_margin (spec §9.1): exact Eq_init_raw_i >= IM_req_i
    /// Per spec §9.1: if eff == 0 then IM_req = 0; else IM_req = max(proportional, MIN_NONZERO_IM_REQ)
    /// Per spec §3.4: MUST use exact raw equity, not clamped Eq_init_net_i,
    /// so negative raw equity is distinguishable from zero.
    pub fn is_above_initial_margin(&self, account: &Account, idx: usize, oracle_price: u64) -> bool {
        let eq_init_raw = self.account_equity_init_raw(account, idx);
        let eff = self.effective_pos_q(idx);
        if eff == 0 {
            return eq_init_raw >= 0;
        }
        let not = self.notional(idx, oracle_price);
        let proportional = mul_div_floor_u128(not, self.params.initial_margin_bps as u128, 10_000);
        let im_req = core::cmp::max(proportional, self.params.min_nonzero_im_req);
        let im_req_i128 = if im_req > i128::MAX as u128 { i128::MAX } else { im_req as i128 };
        eq_init_raw >= im_req_i128
    }

    // ========================================================================
    // Conservation check (spec §3.1)
    // ========================================================================

    pub fn check_conservation(&self) -> bool {
        let senior = self.c_tot.get().checked_add(self.insurance_fund.balance.get());
        match senior {
            Some(s) => self.vault.get() >= s,
            None => false,
        }
    }

    // ========================================================================
    // Warmup Helpers (spec §6)
    // ========================================================================

    /// released_pos (spec §2.1): ReleasedPos_i = max(PNL_i, 0) - R_i
    pub fn released_pos(&self, idx: usize) -> u128 {
        let pnl = self.accounts[idx].pnl;
        let pos_pnl = i128_clamp_pos(pnl);
        pos_pnl.saturating_sub(self.accounts[idx].reserved_pnl)
    }

    /// restart_warmup_after_reserve_increase (spec §4.9)
    /// Caller obligation: MUST be called after set_pnl increases R_i.
    test_visible! {
    fn restart_warmup_after_reserve_increase(&mut self, idx: usize) {
        let t = self.params.warmup_period_slots;
        if t == 0 {
            // Instantaneous warmup: release all reserve immediately
            self.set_reserved_pnl(idx, 0);
            self.accounts[idx].warmup_slope_per_step = 0;
            self.accounts[idx].warmup_started_at_slot = self.current_slot;
            return;
        }
        let r = self.accounts[idx].reserved_pnl;
        if r == 0 {
            self.accounts[idx].warmup_slope_per_step = 0;
            self.accounts[idx].warmup_started_at_slot = self.current_slot;
            return;
        }
        // slope = max(1, floor(R_i / T))
        let base = r / (t as u128);
        let slope = if base == 0 { 1u128 } else { base };
        self.accounts[idx].warmup_slope_per_step = slope;
        self.accounts[idx].warmup_started_at_slot = self.current_slot;
    }
    }

    /// advance_profit_warmup (spec §4.9)
    test_visible! {
    fn advance_profit_warmup(&mut self, idx: usize) {
        let r = self.accounts[idx].reserved_pnl;
        if r == 0 {
            self.accounts[idx].warmup_slope_per_step = 0;
            self.accounts[idx].warmup_started_at_slot = self.current_slot;
            return;
        }
        let t = self.params.warmup_period_slots;
        if t == 0 {
            self.set_reserved_pnl(idx, 0);
            self.accounts[idx].warmup_slope_per_step = 0;
            self.accounts[idx].warmup_started_at_slot = self.current_slot;
            return;
        }
        let elapsed = self.current_slot.saturating_sub(self.accounts[idx].warmup_started_at_slot);
        let cap = saturating_mul_u128_u64(self.accounts[idx].warmup_slope_per_step, elapsed);
        let release = core::cmp::min(r, cap);
        if release > 0 {
            self.set_reserved_pnl(idx, r - release);
        }
        if self.accounts[idx].reserved_pnl == 0 {
            self.accounts[idx].warmup_slope_per_step = 0;
        }
        self.accounts[idx].warmup_started_at_slot = self.current_slot;
    }
    }

    // ========================================================================
    // Loss settlement and profit conversion (spec §7)
    // ========================================================================

    /// settle_losses (spec §7.1): settle negative PnL from principal
    fn settle_losses(&mut self, idx: usize) {
        let pnl = self.accounts[idx].pnl;
        if pnl >= 0 {
            return;
        }
        assert!(pnl != i128::MIN, "settle_losses: i128::MIN");
        let need = pnl.unsigned_abs();
        let cap = self.accounts[idx].capital.get();
        let pay = core::cmp::min(need, cap);
        if pay > 0 {
            self.set_capital(idx, cap - pay);
            let pay_i128 = pay as i128; // pay <= need = |pnl| <= i128::MAX, safe
            let new_pnl = pnl.checked_add(pay_i128)
                .expect("settle_losses: unreachable overflow (pay <= |pnl|)");
            assert!(new_pnl != i128::MIN, "settle_losses: new_pnl == i128::MIN is unreachable");
            self.set_pnl(idx, new_pnl);
        }
    }

    /// resolve_flat_negative (spec §7.3): for flat accounts with negative PnL
    fn resolve_flat_negative(&mut self, idx: usize) {
        let eff = self.effective_pos_q(idx);
        if eff != 0 {
            return; // Not flat — must resolve through liquidation
        }
        let pnl = self.accounts[idx].pnl;
        if pnl < 0 {
            assert!(pnl != i128::MIN, "resolve_flat_negative: i128::MIN");
            let loss = pnl.unsigned_abs();
            self.absorb_protocol_loss(loss);
            self.set_pnl(idx, 0i128);
        }
    }

    /// Profit conversion (spec §7.4): converts matured released profit into
    /// protected principal using consume_released_pnl. Flat-only in automatic touch.
    fn do_profit_conversion(&mut self, idx: usize) {
        let x = self.released_pos(idx);
        if x == 0 {
            return;
        }

        // Compute y using pre-conversion haircut (spec §7.4).
        // Because x > 0 implies pnl_matured_pos_tot > 0, h_den is strictly positive
        // (spec test property 69).
        let (h_num, h_den) = self.haircut_ratio();
        assert!(h_den > 0, "do_profit_conversion: h_den must be > 0 when x > 0");
        let y: u128 = wide_mul_div_floor_u128(x, h_num, h_den);

        // consume_released_pnl(i, x) — leaves R_i unchanged
        self.consume_released_pnl(idx, x);

        // set_capital(i, C_i + y)
        let new_cap = add_u128(self.accounts[idx].capital.get(), y);
        self.set_capital(idx, new_cap);

        // Handle warmup schedule per spec §7.4 step 3-4
        if self.accounts[idx].reserved_pnl == 0 {
            self.accounts[idx].warmup_slope_per_step = 0;
            self.accounts[idx].warmup_started_at_slot = self.current_slot;
        }
        // else leave the existing warmup schedule unchanged
    }

    /// fee_debt_sweep (spec §7.5): after any capital increase, sweep fee debt
    test_visible! {
    fn fee_debt_sweep(&mut self, idx: usize) {
        let fc = self.accounts[idx].fee_credits.get();
        let debt = fee_debt_u128_checked(fc);
        if debt == 0 {
            return;
        }
        let cap = self.accounts[idx].capital.get();
        let pay = core::cmp::min(debt, cap);
        if pay > 0 {
            self.set_capital(idx, cap - pay);
            // pay <= debt = |fee_credits|, so fee_credits + pay <= 0: no overflow
            let pay_i128 = core::cmp::min(pay, i128::MAX as u128) as i128;
            self.accounts[idx].fee_credits = I128::new(self.accounts[idx].fee_credits.get()
                .checked_add(pay_i128).expect("fee_debt_sweep: pay <= debt guarantees no overflow"));
            self.insurance_fund.balance = U128::new(
                self.insurance_fund.balance.get().checked_add(pay)
                    .expect("fee_debt_sweep: insurance overflow (I <= V <= MAX_VAULT_TVL)"));
        }
        // Per spec §7.5: unpaid fee debt remains as local fee_credits until
        // physical capital becomes available or manual profit conversion occurs.
        // MUST NOT consume junior PnL claims to mint senior insurance capital.
    }
    }

    // ========================================================================
    // touch_account_full_not_atomic (spec §10.1)
    // ========================================================================

    pub fn touch_account_full_not_atomic(&mut self, idx: usize, oracle_price: u64, now_slot: u64) -> Result<()> {
        // Bounds and existence check (hardened public API surface)
        if idx >= MAX_ACCOUNTS || !self.is_used(idx) {
            return Err(RiskError::AccountNotFound);
        }
        // Preconditions (spec §10.1 steps 1-4)
        if now_slot < self.current_slot {
            return Err(RiskError::Overflow);
        }
        if now_slot < self.last_market_slot {
            return Err(RiskError::Overflow);
        }
        if oracle_price == 0 || oracle_price > MAX_ORACLE_PRICE {
            return Err(RiskError::Overflow);
        }

        // Step 5: current_slot = now_slot
        self.current_slot = now_slot;

        // Step 6: accrue_market_to
        self.accrue_market_to(now_slot, oracle_price)?;

        // Step 7: advance_profit_warmup (spec §4.9)
        self.advance_profit_warmup(idx);

        // Step 8: settle_side_effects (handles restart_warmup_after_reserve_increase internally)
        self.settle_side_effects(idx)?;

        // Step 9: settle losses from principal
        self.settle_losses(idx);

        // Step 10: resolve flat negative (eff == 0 and PNL < 0)
        if self.effective_pos_q(idx) == 0 && self.accounts[idx].pnl < 0 {
            self.resolve_flat_negative(idx);
        }

        // Step 11: maintenance fees (spec §8.2)
        self.settle_maintenance_fee_internal(idx, now_slot)?;

        // Step 12: if flat, convert matured released profits (spec §7.4)
        if self.accounts[idx].position_basis_q == 0 {
            self.do_profit_conversion(idx);
        }

        // Step 13: fee debt sweep
        self.fee_debt_sweep(idx);

        Ok(())
    }

    /// realize_recurring_maintenance_fee (spec §8.2.2).
    fn settle_maintenance_fee_internal(&mut self, idx: usize, now_slot: u64) -> Result<()> {
        let fee_per_slot = self.params.maintenance_fee_per_slot.get();
        if fee_per_slot == 0 {
            self.accounts[idx].last_fee_slot = now_slot;
            return Ok(());
        }

        let last = self.accounts[idx].last_fee_slot;
        let dt_fee = now_slot.saturating_sub(last);
        if dt_fee == 0 {
            self.accounts[idx].last_fee_slot = now_slot;
            return Ok(());
        }

        // Step 4: fee_due = checked_mul(maintenance_fee_per_slot, dt_fee)
        let fee_due = (dt_fee as u128)
            .checked_mul(fee_per_slot)
            .ok_or(RiskError::Overflow)?;

        // Step 5: require fee_due <= MAX_PROTOCOL_FEE_ABS
        if fee_due > MAX_PROTOCOL_FEE_ABS {
            return Err(RiskError::Overflow);
        }

        // Step 6: charge via charge_fee_to_insurance (spec §8.2.2 ordering)
        if fee_due > 0 {
            self.charge_fee_to_insurance(idx, fee_due)?;
        }

        // Step 7: stamp last_fee_slot after charge (matches spec ordering)
        self.accounts[idx].last_fee_slot = now_slot;

        Ok(())
    }

    // ========================================================================
    // Account Management
    // ========================================================================

    test_visible! {
    fn add_user(&mut self, fee_payment: u128) -> Result<u16> {
        let used_count = self.num_used_accounts as u64;
        if used_count >= self.params.max_accounts {
            return Err(RiskError::Overflow);
        }

        let required_fee = self.params.new_account_fee.get();
        if fee_payment < required_fee {
            return Err(RiskError::InsufficientBalance);
        }

        // MAX_VAULT_TVL bound
        let v_candidate = self.vault.get().checked_add(fee_payment)
            .ok_or(RiskError::Overflow)?;
        if v_candidate > MAX_VAULT_TVL {
            return Err(RiskError::Overflow);
        }

        // All fallible checks before state mutations
        // Enforce materialized_account_count bound (spec §10.0)
        self.materialized_account_count = self.materialized_account_count
            .checked_add(1).ok_or(RiskError::Overflow)?;
        if self.materialized_account_count > MAX_MATERIALIZED_ACCOUNTS {
            self.materialized_account_count -= 1;
            return Err(RiskError::Overflow);
        }

        let idx = match self.alloc_slot() {
            Ok(i) => i,
            Err(e) => {
                self.materialized_account_count -= 1;
                return Err(e);
            }
        };

        // Commit vault/insurance only after all checks pass
        let excess = fee_payment.saturating_sub(required_fee);
        self.vault = U128::new(v_candidate);
        self.insurance_fund.balance = self.insurance_fund.balance + required_fee;

        let account_id = self.next_account_id;
        self.next_account_id = self.next_account_id.saturating_add(1);

        self.accounts[idx as usize] = Account {
            kind: Account::KIND_USER,
            account_id,
            capital: U128::new(excess),
            pnl: 0i128,
            reserved_pnl: 0u128,
            warmup_started_at_slot: self.current_slot,
            warmup_slope_per_step: 0u128,
            position_basis_q: 0i128,
            adl_a_basis: ADL_ONE,
            adl_k_snap: 0i128,
            adl_epoch_snap: 0,
            matcher_program: [0; 32],
            matcher_context: [0; 32],
            owner: [0; 32],
            fee_credits: I128::ZERO,
            last_fee_slot: self.current_slot,
            fees_earned_total: U128::ZERO,
        };

        if excess > 0 {
            self.c_tot = U128::new(self.c_tot.get().checked_add(excess)
                .ok_or(RiskError::Overflow)?);
        }

        Ok(idx)
    }
    }

    test_visible! {
    fn add_lp(
        &mut self,
        matching_engine_program: [u8; 32],
        matching_engine_context: [u8; 32],
        fee_payment: u128,
    ) -> Result<u16> {
        let used_count = self.num_used_accounts as u64;
        if used_count >= self.params.max_accounts {
            return Err(RiskError::Overflow);
        }

        let required_fee = self.params.new_account_fee.get();
        if fee_payment < required_fee {
            return Err(RiskError::InsufficientBalance);
        }

        // MAX_VAULT_TVL bound
        let v_candidate = self.vault.get().checked_add(fee_payment)
            .ok_or(RiskError::Overflow)?;
        if v_candidate > MAX_VAULT_TVL {
            return Err(RiskError::Overflow);
        }

        // Enforce materialized_account_count bound (spec §10.0)
        self.materialized_account_count = self.materialized_account_count
            .checked_add(1).ok_or(RiskError::Overflow)?;
        if self.materialized_account_count > MAX_MATERIALIZED_ACCOUNTS {
            self.materialized_account_count -= 1;
            return Err(RiskError::Overflow);
        }

        let idx = match self.alloc_slot() {
            Ok(i) => i,
            Err(e) => {
                self.materialized_account_count -= 1;
                return Err(e);
            }
        };

        // Commit vault/insurance only after all checks pass
        let excess = fee_payment.saturating_sub(required_fee);
        self.vault = U128::new(v_candidate);
        self.insurance_fund.balance = self.insurance_fund.balance + required_fee;

        let account_id = self.next_account_id;
        self.next_account_id = self.next_account_id.saturating_add(1);

        self.accounts[idx as usize] = Account {
            kind: Account::KIND_LP,
            account_id,
            capital: U128::new(excess),
            pnl: 0i128,
            reserved_pnl: 0u128,
            warmup_started_at_slot: self.current_slot,
            warmup_slope_per_step: 0u128,
            position_basis_q: 0i128,
            adl_a_basis: ADL_ONE,
            adl_k_snap: 0i128,
            adl_epoch_snap: 0,
            matcher_program: matching_engine_program,
            matcher_context: matching_engine_context,
            owner: [0; 32],
            fee_credits: I128::ZERO,
            last_fee_slot: self.current_slot,
            fees_earned_total: U128::ZERO,
        };

        if excess > 0 {
            self.c_tot = U128::new(self.c_tot.get().checked_add(excess)
                .ok_or(RiskError::Overflow)?);
        }

        Ok(idx)
    }
    }

    pub fn set_owner(&mut self, idx: u16, owner: [u8; 32]) -> Result<()> {
        if idx as usize >= MAX_ACCOUNTS || !self.is_used(idx as usize) {
            return Err(RiskError::Unauthorized);
        }
        // Defense-in-depth: reject if owner is already claimed (non-zero).
        // Authorization is the wrapper layer's job, but the engine should
        // not silently overwrite an existing owner.
        if self.accounts[idx as usize].owner != [0u8; 32] {
            return Err(RiskError::Unauthorized);
        }
        self.accounts[idx as usize].owner = owner;
        Ok(())
    }

    // ========================================================================
    // deposit (spec §10.2)
    // ========================================================================

    pub fn deposit(&mut self, idx: u16, amount: u128, _oracle_price: u64, now_slot: u64) -> Result<()> {
        // Time monotonicity (spec §10.3 step 1)
        if now_slot < self.current_slot {
            return Err(RiskError::Overflow);
        }
        if now_slot < self.last_market_slot {
            return Err(RiskError::Overflow);
        }

        // Pre-validate vault capacity before any mutations (prevents ghost account)
        let v_candidate = self.vault.get().checked_add(amount).ok_or(RiskError::Overflow)?;
        if v_candidate > MAX_VAULT_TVL {
            return Err(RiskError::Overflow);
        }

        // Step 2: if account missing, require amount >= MIN_INITIAL_DEPOSIT and materialize
        // Per spec §10.3 step 2 and §2.3: deposit is the canonical materialization path.
        if !self.is_used(idx as usize) {
            let min_dep = self.params.min_initial_deposit.get();
            if amount < min_dep {
                return Err(RiskError::InsufficientBalance);
            }
            self.materialize_at(idx, now_slot)?;
        }

        // Step 3: current_slot = now_slot
        self.current_slot = now_slot;
        self.vault = U128::new(v_candidate);

        // Step 6: set_capital(i, C_i + amount)
        let new_cap = add_u128(self.accounts[idx as usize].capital.get(), amount);
        self.set_capital(idx as usize, new_cap);

        // Step 7: settle_losses_from_principal
        self.settle_losses(idx as usize);

        // Step 8: deposit MUST NOT invoke resolve_flat_negative (spec §7.3).
        // A pure deposit path that does not call accrue_market_to MUST NOT
        // invoke this path — surviving flat negative PNL waits for a later
        // accrued touch.

        // Step 9: if flat and PNL >= 0, sweep fee debt (spec §7.5)
        // Per spec §10.3: deposit into account with basis != 0 MUST defer.
        // Per spec §7.5: only a surviving negative PNL_i blocks the sweep.
        if self.accounts[idx as usize].position_basis_q == 0
            && self.accounts[idx as usize].pnl >= 0
        {
            self.fee_debt_sweep(idx as usize);
        }

        Ok(())
    }

    // ========================================================================
    // withdraw_not_atomic (spec §10.3)
    // ========================================================================

    pub fn withdraw_not_atomic(
        &mut self,
        idx: u16,
        amount: u128,
        oracle_price: u64,
        now_slot: u64,
        funding_rate: i64,
    ) -> Result<()> {
                Self::validate_funding_rate(funding_rate)?;

        if oracle_price == 0 || oracle_price > MAX_ORACLE_PRICE {
            return Err(RiskError::Overflow);
        }

        // No require_fresh_crank: spec §10.4 does not gate withdraw_not_atomic on keeper
        // liveness. touch_account_full_not_atomic calls accrue_market_to with the caller's
        // oracle and slot, satisfying spec §0 goal 6 (liveness without external action).

        if !self.is_used(idx as usize) {
            return Err(RiskError::AccountNotFound);
        }

        let mut ctx = InstructionContext::new();

        // Step 3: touch_account_full_not_atomic
        self.touch_account_full_not_atomic(idx as usize, oracle_price, now_slot)?;

        // Step 4: require amount <= C_i
        if self.accounts[idx as usize].capital.get() < amount {
            return Err(RiskError::InsufficientBalance);
        }

        // Step 5: universal dust guard — post-withdraw_not_atomic capital must be 0 or >= MIN_INITIAL_DEPOSIT
        let post_cap = self.accounts[idx as usize].capital.get() - amount;
        if post_cap != 0 && post_cap < self.params.min_initial_deposit.get() {
            return Err(RiskError::InsufficientBalance);
        }

        // Step 6: if position exists, require post-withdraw_not_atomic initial margin
        let eff = self.effective_pos_q(idx as usize);
        if eff != 0 {
            // Simulate withdrawal: adjust BOTH capital AND vault to keep Residual consistent
            let old_cap = self.accounts[idx as usize].capital.get();
            let old_vault = self.vault;
            self.set_capital(idx as usize, post_cap);
            self.vault = U128::new(sub_u128(self.vault.get(), amount));
            let passes_im = self.is_above_initial_margin(&self.accounts[idx as usize], idx as usize, oracle_price);
            // Revert both
            self.set_capital(idx as usize, old_cap);
            self.vault = old_vault;
            if !passes_im {
                return Err(RiskError::Undercollateralized);
            }
        }

        // Step 7: commit withdrawal
        self.set_capital(idx as usize, self.accounts[idx as usize].capital.get() - amount);
        self.vault = U128::new(sub_u128(self.vault.get(), amount));

        // Steps 8-9: end-of-instruction resets
        self.schedule_end_of_instruction_resets(&mut ctx)?;
        self.finalize_end_of_instruction_resets(&ctx);
        self.recompute_r_last_from_final_state(funding_rate)?;

        Ok(())
    }

    // ========================================================================
    // settle_account_not_atomic (spec §10.7)
    // ========================================================================

    /// Top-level settle wrapper per spec §10.7.
    /// If settlement is exposed as a standalone instruction, this wrapper MUST be used.
    pub fn settle_account_not_atomic(
        &mut self,
        idx: u16,
        oracle_price: u64,
        now_slot: u64,
        funding_rate: i64,
    ) -> Result<()> {
                Self::validate_funding_rate(funding_rate)?;

        if oracle_price == 0 || oracle_price > MAX_ORACLE_PRICE {
            return Err(RiskError::Overflow);
        }
        if !self.is_used(idx as usize) {
            return Err(RiskError::AccountNotFound);
        }

        let mut ctx = InstructionContext::new();

        // Step 3: touch_account_full_not_atomic
        self.touch_account_full_not_atomic(idx as usize, oracle_price, now_slot)?;

        // Steps 4-5: end-of-instruction resets
        self.schedule_end_of_instruction_resets(&mut ctx)?;
        self.finalize_end_of_instruction_resets(&ctx);
        self.recompute_r_last_from_final_state(funding_rate)?;

        // Step 7: assert OI balance
        assert!(self.oi_eff_long_q == self.oi_eff_short_q, "OI_eff_long != OI_eff_short after settle");

        Ok(())
    }

    // ========================================================================
    // execute_trade_not_atomic (spec §10.4)
    // ========================================================================

    pub fn execute_trade_not_atomic(
        &mut self,
        a: u16,
        b: u16,
        oracle_price: u64,
        now_slot: u64,
        size_q: i128,
        exec_price: u64,
        funding_rate: i64,
    ) -> Result<()> {
                Self::validate_funding_rate(funding_rate)?;

        if oracle_price == 0 || oracle_price > MAX_ORACLE_PRICE {
            return Err(RiskError::Overflow);
        }
        if exec_price == 0 || exec_price > MAX_ORACLE_PRICE {
            return Err(RiskError::Overflow);
        }
        // Spec §10.5 step 7: require 0 < size_q <= MAX_TRADE_SIZE_Q
        if size_q <= 0 {
            return Err(RiskError::Overflow);
        }
        if size_q as u128 > MAX_TRADE_SIZE_Q {
            return Err(RiskError::Overflow);
        }

        // trade_notional check (spec §10.4 step 6)
        let trade_notional_check = mul_div_floor_u128(size_q as u128, exec_price as u128, POS_SCALE);
        if trade_notional_check > MAX_ACCOUNT_NOTIONAL {
            return Err(RiskError::Overflow);
        }

        // No require_fresh_crank: spec §10.5 does not gate execute_trade_not_atomic on
        // keeper liveness. touch_account_full_not_atomic calls accrue_market_to with the
        // caller's oracle and slot, satisfying spec §0 goal 6.

        if !self.is_used(a as usize) || !self.is_used(b as usize) {
            return Err(RiskError::AccountNotFound);
        }
        if a == b {
            return Err(RiskError::Overflow);
        }

        let mut ctx = InstructionContext::new();

        // Steps 11-12: touch both
        self.touch_account_full_not_atomic(a as usize, oracle_price, now_slot)?;
        self.touch_account_full_not_atomic(b as usize, oracle_price, now_slot)?;

        // Step 13: capture old effective positions
        let old_eff_a = self.effective_pos_q(a as usize);
        let old_eff_b = self.effective_pos_q(b as usize);

        // Steps 14-16: capture pre-trade MM requirements and raw maintenance buffers
        // Spec §9.1: if effective_pos_q(i) == 0, MM_req_i = 0
        let mm_req_pre_a = if old_eff_a == 0 { 0u128 } else {
            let not = self.notional(a as usize, oracle_price);
            core::cmp::max(
                    mul_div_floor_u128(not, self.params.maintenance_margin_bps as u128, 10_000),
                    self.params.min_nonzero_mm_req
                )
        };
        let mm_req_pre_b = if old_eff_b == 0 { 0u128 } else {
            let not = self.notional(b as usize, oracle_price);
            core::cmp::max(
                    mul_div_floor_u128(not, self.params.maintenance_margin_bps as u128, 10_000),
                    self.params.min_nonzero_mm_req
                )
        };
        let maint_raw_wide_pre_a = self.account_equity_maint_raw_wide(&self.accounts[a as usize]);
        let maint_raw_wide_pre_b = self.account_equity_maint_raw_wide(&self.accounts[b as usize]);
        let buffer_pre_a = maint_raw_wide_pre_a.checked_sub(I256::from_u128(mm_req_pre_a)).expect("I256 sub");
        let buffer_pre_b = maint_raw_wide_pre_b.checked_sub(I256::from_u128(mm_req_pre_b)).expect("I256 sub");

        // Step 6: compute new effective positions
        let new_eff_a = old_eff_a.checked_add(size_q).ok_or(RiskError::Overflow)?;
        let neg_size_q = size_q.checked_neg().ok_or(RiskError::Overflow)?;
        let new_eff_b = old_eff_b.checked_add(neg_size_q).ok_or(RiskError::Overflow)?;

        // Validate position bounds
        if new_eff_a != 0 && new_eff_a.unsigned_abs() > MAX_POSITION_ABS_Q {
            return Err(RiskError::Overflow);
        }
        if new_eff_b != 0 && new_eff_b.unsigned_abs() > MAX_POSITION_ABS_Q {
            return Err(RiskError::Overflow);
        }

        // Validate notional bounds
        {
            let notional_a = mul_div_floor_u128(new_eff_a.unsigned_abs(), oracle_price as u128, POS_SCALE);
            if notional_a > MAX_ACCOUNT_NOTIONAL {
                return Err(RiskError::Overflow);
            }
            let notional_b = mul_div_floor_u128(new_eff_b.unsigned_abs(), oracle_price as u128, POS_SCALE);
            if notional_b > MAX_ACCOUNT_NOTIONAL {
                return Err(RiskError::Overflow);
            }
        }

        // Preflight: finalize any ResetPending sides that are fully ready,
        // so OI-increase gating doesn't block trades on reopenable sides.
        self.maybe_finalize_ready_reset_sides();

        // Step 5: compute bilateral OI once (spec §5.2.2) and use for both
        // mode gating and later writeback. Avoids redundant checked arithmetic.
        let (oi_long_after, oi_short_after) = self.bilateral_oi_after(
            &old_eff_a, &new_eff_a, &old_eff_b, &new_eff_b)?;

        // Validate OI bounds
        if oi_long_after > MAX_OI_SIDE_Q || oi_short_after > MAX_OI_SIDE_Q {
            return Err(RiskError::Overflow);
        }

        // Reject if trade would increase OI on a blocked side
        if (self.side_mode_long == SideMode::DrainOnly || self.side_mode_long == SideMode::ResetPending)
            && oi_long_after > self.oi_eff_long_q {
            return Err(RiskError::SideBlocked);
        }
        if (self.side_mode_short == SideMode::DrainOnly || self.side_mode_short == SideMode::ResetPending)
            && oi_short_after > self.oi_eff_short_q {
            return Err(RiskError::SideBlocked);
        }

        // Step 21: trade PnL alignment (spec §10.5)
        let price_diff = (oracle_price as i128) - (exec_price as i128);
        let trade_pnl_a = compute_trade_pnl(size_q, price_diff)?;
        let trade_pnl_b = trade_pnl_a.checked_neg().ok_or(RiskError::Overflow)?;

        let old_r_a = self.accounts[a as usize].reserved_pnl;
        let old_r_b = self.accounts[b as usize].reserved_pnl;

        let pnl_a = self.accounts[a as usize].pnl.checked_add(trade_pnl_a).ok_or(RiskError::Overflow)?;
        if pnl_a == i128::MIN { return Err(RiskError::Overflow); }
        self.set_pnl(a as usize, pnl_a);

        let pnl_b = self.accounts[b as usize].pnl.checked_add(trade_pnl_b).ok_or(RiskError::Overflow)?;
        if pnl_b == i128::MIN { return Err(RiskError::Overflow); }
        self.set_pnl(b as usize, pnl_b);

        // Caller obligation: restart warmup if R increased
        if self.accounts[a as usize].reserved_pnl > old_r_a {
            self.restart_warmup_after_reserve_increase(a as usize);
        }
        if self.accounts[b as usize].reserved_pnl > old_r_b {
            self.restart_warmup_after_reserve_increase(b as usize);
        }

        // Step 8: attach effective positions
        self.attach_effective_position(a as usize, new_eff_a);
        self.attach_effective_position(b as usize, new_eff_b);

        // Step 9: write pre-computed OI (same values from step 5, spec §5.2.2)
        self.oi_eff_long_q = oi_long_after;
        self.oi_eff_short_q = oi_short_after;

        // Step 10: settle post-trade losses from principal for both accounts (spec §10.4 step 18)
        // Loss seniority: losses MUST be settled before explicit fees (spec §0 item 14)
        self.settle_losses(a as usize);
        self.settle_losses(b as usize);

        // Step 11: charge trading fees (spec §10.4 step 19, §8.1)
        let trade_notional = mul_div_floor_u128(size_q.unsigned_abs(), exec_price as u128, POS_SCALE);
        let fee = if trade_notional > 0 && self.params.trading_fee_bps > 0 {
            mul_div_ceil_u128(trade_notional, self.params.trading_fee_bps as u128, 10_000)
        } else {
            0
        };

        // Charge fee from both accounts (spec §10.5 step 28)
        // (cash_to_insurance, total_equity_impact) for each side
        let mut fee_cash_a = 0u128;
        let mut fee_cash_b = 0u128;
        let mut fee_impact_a = 0u128;
        let mut fee_impact_b = 0u128;
        if fee > 0 {
            if fee > MAX_PROTOCOL_FEE_ABS {
                return Err(RiskError::Overflow);
            }
            let (cash_a, impact_a) = self.charge_fee_to_insurance(a as usize, fee)?;
            let (cash_b, impact_b) = self.charge_fee_to_insurance(b as usize, fee)?;
            fee_cash_a = cash_a;
            fee_cash_b = cash_b;
            fee_impact_a = impact_a;
            fee_impact_b = impact_b;
        }

        // Track LP fees: use total equity impact (capital paid + collectible debt).
        // This is the nominal fee obligation from the counterparty's trade.
        // Debt may be collected later via fee_debt_sweep or forgiven on dust
        // reclamation — that's an insurance concern, not LP attribution.
        if self.accounts[a as usize].is_lp() {
            self.accounts[a as usize].fees_earned_total = U128::new(
                add_u128(self.accounts[a as usize].fees_earned_total.get(), fee_impact_b)
            );
        }
        if self.accounts[b as usize].is_lp() {
            self.accounts[b as usize].fees_earned_total = U128::new(
                add_u128(self.accounts[b as usize].fees_earned_total.get(), fee_impact_a)
            );
        }

        // Steps 25-26: flat-close PNL guard (spec §10.5)
        if new_eff_a == 0 && self.accounts[a as usize].pnl < 0 {
            return Err(RiskError::Undercollateralized);
        }
        if new_eff_b == 0 && self.accounts[b as usize].pnl < 0 {
            return Err(RiskError::Undercollateralized);
        }

        // Step 29: post-trade margin enforcement (spec §10.5)
        // The spec says "(Eq_maint_raw_i + fee)" using the nominal fee.
        // We use fee_impact (capital_paid + collectible_debt) instead because:
        // - charge_fee_to_insurance can drop excess beyond collectible headroom
        // - Eq_maint_raw only decreased by impact, not the full nominal fee
        // - Adding back impact correctly reverses the actual state change
        // - Using nominal fee would over-compensate and admit invalid trades
        self.enforce_post_trade_margin(
            a as usize, b as usize, oracle_price,
            &old_eff_a, &new_eff_a, &old_eff_b, &new_eff_b,
            buffer_pre_a, buffer_pre_b, fee_impact_a, fee_impact_b,
        )?;

        // Steps 16-17: end-of-instruction resets
        self.schedule_end_of_instruction_resets(&mut ctx)?;
        self.finalize_end_of_instruction_resets(&ctx);

        // Step 32: recompute r_last if funding-rate inputs changed (spec §10.5)
        self.recompute_r_last_from_final_state(funding_rate)?;

        // Step 18: assert OI balance (spec §10.4)
        assert!(self.oi_eff_long_q == self.oi_eff_short_q, "OI_eff_long != OI_eff_short after trade");

        Ok(())
    }

    /// Charge fee per spec §8.1 — route shortfall through fee_credits instead of PNL.
    /// Returns (capital_paid_to_insurance, total_equity_impact).
    /// capital_paid is realized revenue; total includes collectible debt.
    /// Any excess beyond collectible headroom is silently dropped.
    fn charge_fee_to_insurance(&mut self, idx: usize, fee: u128) -> Result<(u128, u128)> {
        if fee > MAX_PROTOCOL_FEE_ABS {
            return Err(RiskError::Overflow);
        }
        let cap = self.accounts[idx].capital.get();
        let fee_paid = core::cmp::min(fee, cap);
        if fee_paid > 0 {
            self.set_capital(idx, cap - fee_paid);
            self.insurance_fund.balance = self.insurance_fund.balance + fee_paid;
        }
        let fee_shortfall = fee - fee_paid;
        if fee_shortfall > 0 {
            // Route collectible shortfall through fee_credits (debit).
            // Cap at collectible headroom to avoid reverting (spec §8.2.2):
            // fee_credits must stay in [-(i128::MAX), 0]; any excess is dropped.
            let current_fc = self.accounts[idx].fee_credits.get();
            // Headroom = current_fc - (-(i128::MAX)) = current_fc + i128::MAX
            let headroom = match current_fc.checked_add(i128::MAX) {
                Some(h) if h > 0 => h as u128,
                _ => 0u128, // at or beyond limit — no room
            };
            let collectible = core::cmp::min(fee_shortfall, headroom);
            if collectible > 0 {
                // Safe: collectible <= headroom <= i128::MAX, and
                // current_fc - collectible >= -(i128::MAX)
                let new_fc = current_fc - (collectible as i128);
                self.accounts[idx].fee_credits = I128::new(new_fc);
            }
            // Any excess beyond collectible headroom is silently dropped
            Ok((fee_paid, fee_paid + collectible))
        } else {
            Ok((fee_paid, fee_paid))
        }
    }

    /// OI component helpers for exact bilateral decomposition (spec §5.2.2)
    fn oi_long_component(pos: i128) -> u128 {
        if pos > 0 { pos as u128 } else { 0u128 }
    }

    fn oi_short_component(pos: i128) -> u128 {
        if pos < 0 { pos.unsigned_abs() } else { 0u128 }
    }

    /// Compute exact bilateral candidate side-OI after-values (spec §5.2.2).
    /// Returns (OI_long_after, OI_short_after).
    fn bilateral_oi_after(
        &self,
        old_a: &i128, new_a: &i128,
        old_b: &i128, new_b: &i128,
    ) -> Result<(u128, u128)> {
        let oi_long_after = self.oi_eff_long_q
            .checked_sub(Self::oi_long_component(*old_a)).ok_or(RiskError::CorruptState)?
            .checked_sub(Self::oi_long_component(*old_b)).ok_or(RiskError::CorruptState)?
            .checked_add(Self::oi_long_component(*new_a)).ok_or(RiskError::Overflow)?
            .checked_add(Self::oi_long_component(*new_b)).ok_or(RiskError::Overflow)?;

        let oi_short_after = self.oi_eff_short_q
            .checked_sub(Self::oi_short_component(*old_a)).ok_or(RiskError::CorruptState)?
            .checked_sub(Self::oi_short_component(*old_b)).ok_or(RiskError::CorruptState)?
            .checked_add(Self::oi_short_component(*new_a)).ok_or(RiskError::Overflow)?
            .checked_add(Self::oi_short_component(*new_b)).ok_or(RiskError::Overflow)?;

        Ok((oi_long_after, oi_short_after))
    }

    /// Check side-mode gating using exact bilateral OI decomposition (spec §5.2.2 + §9.6).
    /// A trade would increase net side OI iff OI_side_after > OI_eff_side.
    fn check_side_mode_for_trade(
        &self,
        old_a: &i128, new_a: &i128,
        old_b: &i128, new_b: &i128,
    ) -> Result<()> {
        let (oi_long_after, oi_short_after) = self.bilateral_oi_after(old_a, new_a, old_b, new_b)?;

        for &side in &[Side::Long, Side::Short] {
            let mode = self.get_side_mode(side);
            if mode != SideMode::DrainOnly && mode != SideMode::ResetPending {
                continue;
            }
            let (oi_after, oi_before) = match side {
                Side::Long => (oi_long_after, self.oi_eff_long_q),
                Side::Short => (oi_short_after, self.oi_eff_short_q),
            };
            if oi_after > oi_before {
                return Err(RiskError::SideBlocked);
            }
        }
        Ok(())
    }

    /// Enforce post-trade margin per spec §10.5 step 29.
    /// Uses strict risk-reducing buffer comparison with exact I256 Eq_maint_raw.
    fn enforce_post_trade_margin(
        &self,
        a: usize,
        b: usize,
        oracle_price: u64,
        old_eff_a: &i128,
        new_eff_a: &i128,
        old_eff_b: &i128,
        new_eff_b: &i128,
        buffer_pre_a: I256,
        buffer_pre_b: I256,
        fee_a: u128,
        fee_b: u128,
    ) -> Result<()> {
        self.enforce_one_side_margin(a, oracle_price, old_eff_a, new_eff_a, buffer_pre_a, fee_a)?;
        self.enforce_one_side_margin(b, oracle_price, old_eff_b, new_eff_b, buffer_pre_b, fee_b)?;
        Ok(())
    }

    fn enforce_one_side_margin(
        &self,
        idx: usize,
        oracle_price: u64,
        old_eff: &i128,
        new_eff: &i128,
        buffer_pre: I256,
        fee: u128,
    ) -> Result<()> {
        if *new_eff == 0 {
            // v12.1.0 §10.5 step 29: flat-close guard uses exact Eq_maint_raw_i >= 0
            // (not just PNL >= 0). Prevents flat exits with negative net wealth from fee debt.
            let maint_raw = self.account_equity_maint_raw_wide(&self.accounts[idx]);
            if maint_raw.is_negative() {
                return Err(RiskError::Undercollateralized);
            }
            return Ok(());
        }

        let abs_old: u128 = if *old_eff == 0 { 0u128 } else { old_eff.unsigned_abs() };
        let abs_new = new_eff.unsigned_abs();

        // Determine if risk-increasing (spec §9.2)
        let risk_increasing = abs_new > abs_old
            || (*old_eff > 0 && *new_eff < 0)
            || (*old_eff < 0 && *new_eff > 0)
            || *old_eff == 0;

        // Determine if strictly risk-reducing (spec §9.2)
        let strictly_reducing = *old_eff != 0
            && *new_eff != 0
            && ((*old_eff > 0 && *new_eff > 0) || (*old_eff < 0 && *new_eff < 0))
            && abs_new < abs_old;

        if risk_increasing {
            // Require initial-margin healthy using Eq_init_net_i
            if !self.is_above_initial_margin(&self.accounts[idx], idx, oracle_price) {
                return Err(RiskError::Undercollateralized);
            }
        } else if self.is_above_maintenance_margin(&self.accounts[idx], idx, oracle_price) {
            // Maintenance healthy: allow
        } else if strictly_reducing {
            // v12.1.0 §10.5 step 29: strict risk-reducing exemption (fee-neutral).
            // Both conditions must hold in exact widened I256:
            // 1. Fee-neutral buffer improves: (Eq_maint_raw_post + fee) - MM_req_post > buffer_pre
            // 2. Fee-neutral shortfall does not worsen: min(Eq_maint_raw_post + fee, 0) >= min(Eq_maint_raw_pre, 0)
            let maint_raw_wide_post = self.account_equity_maint_raw_wide(&self.accounts[idx]);
            let fee_wide = I256::from_u128(fee);

            // Fee-neutral post equity and buffer
            let maint_raw_fee_neutral = maint_raw_wide_post.checked_add(fee_wide).expect("I256 add");
            let mm_req_post = {
                let not = self.notional(idx, oracle_price);
                core::cmp::max(
                    mul_div_floor_u128(not, self.params.maintenance_margin_bps as u128, 10_000),
                    self.params.min_nonzero_mm_req
                )
            };
            let buffer_post_fee_neutral = maint_raw_fee_neutral.checked_sub(I256::from_u128(mm_req_post)).expect("I256 sub");

            // Recover pre-trade raw equity from buffer_pre + MM_req_pre
            let mm_req_pre = {
                let not_pre = if *old_eff == 0 { 0u128 } else {
                    mul_div_floor_u128(old_eff.unsigned_abs(), oracle_price as u128, POS_SCALE)
                };
                core::cmp::max(
                    mul_div_floor_u128(not_pre, self.params.maintenance_margin_bps as u128, 10_000),
                    self.params.min_nonzero_mm_req
                )
            };
            let maint_raw_pre = buffer_pre.checked_add(I256::from_u128(mm_req_pre)).expect("I256 add");

            // Condition 1: fee-neutral buffer strictly improves
            let cond1 = buffer_post_fee_neutral > buffer_pre;

            // Condition 2: fee-neutral shortfall below zero does not worsen
            // min(post + fee, 0) >= min(pre, 0)
            let zero = I256::from_i128(0);
            let shortfall_post = if maint_raw_fee_neutral < zero { maint_raw_fee_neutral } else { zero };
            let shortfall_pre = if maint_raw_pre < zero { maint_raw_pre } else { zero };
            let cond2 = shortfall_post >= shortfall_pre;

            if cond1 && cond2 {
                // Both conditions met: allow
            } else {
                return Err(RiskError::Undercollateralized);
            }
        } else {
            return Err(RiskError::Undercollateralized);
        }
        Ok(())
    }

    /// Update OI using exact bilateral decomposition (spec §5.2.2).
    /// The same values computed for gating MUST be written back — no alternate decomposition.
    fn update_oi_from_positions(
        &mut self,
        old_a: &i128, new_a: &i128,
        old_b: &i128, new_b: &i128,
    ) -> Result<()> {
        let (oi_long_after, oi_short_after) = self.bilateral_oi_after(old_a, new_a, old_b, new_b)?;

        // Check bounds
        if oi_long_after > MAX_OI_SIDE_Q {
            return Err(RiskError::Overflow);
        }
        if oi_short_after > MAX_OI_SIDE_Q {
            return Err(RiskError::Overflow);
        }

        self.oi_eff_long_q = oi_long_after;
        self.oi_eff_short_q = oi_short_after;

        Ok(())
    }

    // ========================================================================
    // liquidate_at_oracle_not_atomic (spec §10.5 + §10.0)
    // ========================================================================

    /// Top-level liquidation: creates its own InstructionContext and finalizes resets.
    /// Accepts LiquidationPolicy per spec §10.6.
    pub fn liquidate_at_oracle_not_atomic(
        &mut self,
        idx: u16,
        now_slot: u64,
        oracle_price: u64,
        policy: LiquidationPolicy,
        funding_rate: i64,
    ) -> Result<bool> {
                Self::validate_funding_rate(funding_rate)?;

        // Bounds and existence check BEFORE touch_account_full_not_atomic to prevent
        // market-state mutation (accrue_market_to) on missing accounts.
        if idx as usize >= MAX_ACCOUNTS || !self.is_used(idx as usize) {
            return Ok(false);
        }

        let mut ctx = InstructionContext::new();

        // Per spec §10.6 step 3: touch_account_full_not_atomic before the liquidation routine.
        self.touch_account_full_not_atomic(idx as usize, oracle_price, now_slot)?;

        let result = self.liquidate_at_oracle_internal(idx, now_slot, oracle_price, policy, &mut ctx)?;

        // End-of-instruction resets must run unconditionally because
        // touch_account_full_not_atomic mutates state even when liquidation doesn't proceed.
        self.schedule_end_of_instruction_resets(&mut ctx)?;
        self.finalize_end_of_instruction_resets(&ctx);
        self.recompute_r_last_from_final_state(funding_rate)?;

        // Assert OI balance unconditionally (spec §10.6 step 11)
        assert!(self.oi_eff_long_q == self.oi_eff_short_q, "OI_eff_long != OI_eff_short after liquidation");
        Ok(result)
    }

    /// Internal liquidation routine: takes caller's shared InstructionContext.
    /// Precondition (spec §9.4): caller has already called touch_account_full_not_atomic(i).
    /// Does NOT call schedule/finalize resets — caller is responsible.
    fn liquidate_at_oracle_internal(
        &mut self,
        idx: u16,
        _now_slot: u64,
        oracle_price: u64,
        policy: LiquidationPolicy,
        ctx: &mut InstructionContext,
    ) -> Result<bool> {
        if idx as usize >= MAX_ACCOUNTS || !self.is_used(idx as usize) {
            return Ok(false);
        }

        if oracle_price == 0 || oracle_price > MAX_ORACLE_PRICE {
            return Err(RiskError::Overflow);
        }

        // Check position exists
        let old_eff = self.effective_pos_q(idx as usize);
        if old_eff == 0 {
            return Ok(false);
        }

        // Step 4: check liquidation eligibility (spec §9.3)
        if self.is_above_maintenance_margin(&self.accounts[idx as usize], idx as usize, oracle_price) {
            return Ok(false);
        }

        let liq_side = side_of_i128(old_eff).unwrap();
        let abs_old_eff = old_eff.unsigned_abs();

        match policy {
            LiquidationPolicy::ExactPartial(q_close_q) => {
                // Spec §9.4: partial liquidation
                // Step 1-2: require 0 < q_close_q < abs(old_eff_pos_q_i)
                if q_close_q == 0 || q_close_q >= abs_old_eff {
                    return Err(RiskError::Overflow);
                }
                // Step 4: new_eff_abs_q = abs(old) - q_close_q
                let new_eff_abs_q = abs_old_eff.checked_sub(q_close_q)
                    .ok_or(RiskError::Overflow)?;
                // Step 5: require new_eff_abs_q > 0 (property 68)
                if new_eff_abs_q == 0 {
                    return Err(RiskError::Overflow);
                }
                // Step 6: new_eff_pos_q_i = sign(old) * new_eff_abs_q
                let sign = if old_eff > 0 { 1i128 } else { -1i128 };
                let new_eff = sign.checked_mul(new_eff_abs_q as i128)
                    .ok_or(RiskError::Overflow)?;

                // Step 7-8: close q_close_q at oracle, attach new position
                self.attach_effective_position(idx as usize, new_eff);

                // Step 9: settle realized losses from principal
                self.settle_losses(idx as usize);

                // Step 10-11: charge liquidation fee on quantity closed
                let liq_fee = {
                    let notional_val = mul_div_floor_u128(q_close_q, oracle_price as u128, POS_SCALE);
                    let liq_fee_raw = mul_div_ceil_u128(notional_val, self.params.liquidation_fee_bps as u128, 10_000);
                    core::cmp::min(
                        core::cmp::max(liq_fee_raw, self.params.min_liquidation_abs.get()),
                        self.params.liquidation_fee_cap.get(),
                    )
                };
                self.charge_fee_to_insurance(idx as usize, liq_fee)?;

                // Step 12: enqueue ADL with d=0 (partial, no bankruptcy)
                self.enqueue_adl(ctx, liq_side, q_close_q, 0)?;

                // Step 13: check if pending reset was scheduled
                // (If so, skip further live-OI-dependent work, but step 14 still runs)

                // Step 14: MANDATORY post-partial local maintenance health check
                // This MUST run even when step 13 has scheduled a pending reset (spec §9.4).
                if !self.is_above_maintenance_margin(&self.accounts[idx as usize], idx as usize, oracle_price) {
                    return Err(RiskError::Undercollateralized);
                }

                self.lifetime_liquidations = self.lifetime_liquidations.saturating_add(1);
                Ok(true)
            }
            LiquidationPolicy::FullClose => {
                // Spec §9.5: full-close liquidation (existing behavior)
                let q_close_q = abs_old_eff;

                // Close entire position at oracle
                self.attach_effective_position(idx as usize, 0i128);

                // Settle losses from principal
                self.settle_losses(idx as usize);

                // Charge liquidation fee (spec §8.3)
                let liq_fee = if q_close_q == 0 {
                    0u128
                } else {
                    let notional_val = mul_div_floor_u128(q_close_q, oracle_price as u128, POS_SCALE);
                    let liq_fee_raw = mul_div_ceil_u128(notional_val, self.params.liquidation_fee_bps as u128, 10_000);
                    core::cmp::min(
                        core::cmp::max(liq_fee_raw, self.params.min_liquidation_abs.get()),
                        self.params.liquidation_fee_cap.get(),
                    )
                };
                self.charge_fee_to_insurance(idx as usize, liq_fee)?;

                // Determine deficit D
                let eff_post = self.effective_pos_q(idx as usize);
                let d: u128 = if eff_post == 0 && self.accounts[idx as usize].pnl < 0 {
                    assert!(self.accounts[idx as usize].pnl != i128::MIN, "liquidate: i128::MIN pnl");
                    self.accounts[idx as usize].pnl.unsigned_abs()
                } else {
                    0u128
                };

                // Enqueue ADL
                if q_close_q != 0 || d != 0 {
                    self.enqueue_adl(ctx, liq_side, q_close_q, d)?;
                }

                // If D > 0, set_pnl(i, 0)
                if d != 0 {
                    self.set_pnl(idx as usize, 0i128);
                }

                self.lifetime_liquidations = self.lifetime_liquidations.saturating_add(1);
                Ok(true)
            }
        }
    }

    // ========================================================================
    // keeper_crank_not_atomic (spec §10.6)
    // ========================================================================

    /// keeper_crank_not_atomic (spec §10.8): Minimal on-chain permissionless shortlist processor.
    /// Candidate discovery is performed off-chain. ordered_candidates[] is untrusted.
    /// Each candidate is (account_idx, optional liquidation policy hint).
    pub fn keeper_crank_not_atomic(
        &mut self,
        now_slot: u64,
        oracle_price: u64,
        ordered_candidates: &[(u16, Option<LiquidationPolicy>)],
        max_revalidations: u16,
        funding_rate: i64,
    ) -> Result<CrankOutcome> {
                Self::validate_funding_rate(funding_rate)?;

        if oracle_price == 0 || oracle_price > MAX_ORACLE_PRICE {
            return Err(RiskError::Overflow);
        }

        // Step 1: initialize instruction context
        let mut ctx = InstructionContext::new();

        // Steps 2-4: validate inputs
        if now_slot < self.current_slot {
            return Err(RiskError::Overflow);
        }
        if now_slot < self.last_market_slot {
            return Err(RiskError::Overflow);
        }

        // Step 5: accrue_market_to exactly once
        self.accrue_market_to(now_slot, oracle_price)?;

        // Step 6: current_slot = now_slot
        self.current_slot = now_slot;

        let advanced = now_slot > self.last_crank_slot;
        if advanced {
            self.last_crank_slot = now_slot;
        }

        // Step 7-8: process candidates in keeper-supplied order
        let mut attempts: u16 = 0;
        let mut num_liquidations: u32 = 0;

        for &(candidate_idx, ref hint) in ordered_candidates {
            // Budget check
            if attempts >= max_revalidations {
                break;
            }
            // Stop on pending reset
            if ctx.pending_reset_long || ctx.pending_reset_short {
                break;
            }
            // Skip missing accounts (doesn't count against budget)
            if (candidate_idx as usize) >= MAX_ACCOUNTS || !self.is_used(candidate_idx as usize) {
                continue;
            }

            // Count as an attempt
            attempts += 1;
            let cidx = candidate_idx as usize;

            // Per-candidate local exact-touch (spec §11.2): same as touch_account_full_not_atomic
            // steps 7-13 on already-accrued state. MUST NOT call accrue_market_to again.

            // Step 7: advance_profit_warmup
            self.advance_profit_warmup(cidx);

            // Step 8: settle_side_effects (handles restart_warmup internally)
            self.settle_side_effects(cidx)?;

            // Step 9: settle losses
            self.settle_losses(cidx);

            // Step 10: resolve flat negative
            if self.effective_pos_q(cidx) == 0 && self.accounts[cidx].pnl < 0 {
                self.resolve_flat_negative(cidx);
            }

            // Step 11: maintenance fees (spec §8.2)
            self.settle_maintenance_fee_internal(cidx, now_slot)?;

            // Step 12: if flat, profit conversion
            if self.accounts[cidx].position_basis_q == 0 {
                self.do_profit_conversion(cidx);
            }

            // Step 13: fee debt sweep
            self.fee_debt_sweep(cidx);

            // Check if liquidatable after exact current-state touch.
            // Apply hint if present and current-state-valid (spec §11.1 rule 3).
            if !ctx.pending_reset_long && !ctx.pending_reset_short {
                let eff = self.effective_pos_q(cidx);
                if eff != 0 {
                    if !self.is_above_maintenance_margin(&self.accounts[cidx], cidx, oracle_price) {
                        // Validate hint via stateless pre-flight (spec §11.1 rule 3).
                        // None hint → no action per spec §11.2.
                        // Invalid ExactPartial → None (no action) per spec §11.1 rule 3.
                        if let Some(policy) = self.validate_keeper_hint(candidate_idx, eff, hint, oracle_price) {
                            match self.liquidate_at_oracle_internal(candidate_idx, now_slot, oracle_price, policy, &mut ctx) {
                                Ok(true) => { num_liquidations += 1; }
                                Ok(false) => {}
                                Err(e) => return Err(e),
                            }
                        }
                    }
                }
            }
        }

        // Steps 9-10: end-of-instruction resets
        self.schedule_end_of_instruction_resets(&mut ctx)?;
        self.finalize_end_of_instruction_resets(&ctx);

        // Step 11: recompute r_last exactly once from final post-reset state
        self.recompute_r_last_from_final_state(funding_rate)?;

        // Step 12: assert OI balance
        assert!(self.oi_eff_long_q == self.oi_eff_short_q,
            "OI_eff_long != OI_eff_short after keeper_crank_not_atomic");

        Ok(CrankOutcome {
            advanced,
            slots_forgiven: 0,
            caller_settle_ok: true,
            force_realize_needed: false,
            panic_needed: false,
            num_liquidations,
            num_liq_errors: 0,
            num_gc_closed: 0,
            last_cursor: 0,
            sweep_complete: false,
        })
    }

    /// Validate a keeper-supplied liquidation-policy hint (spec §11.1 rule 3).
    /// Returns None if no liquidation action should be taken (absent hint per
    /// spec §11.2), or Some(policy) if the hint is valid. ExactPartial hints
    /// are validated via a stateless pre-flight check; invalid partials fall
    /// back to FullClose to preserve crank liveness.
    ///
    /// Pre-flight correctness: settle_losses preserves C + PNL (spec §7.1),
    /// and the synthetic close at oracle generates zero additional PnL delta,
    /// so Eq_maint_raw after partial = Eq_maint_raw_before - liq_fee.
    test_visible! {
    fn validate_keeper_hint(
        &self,
        idx: u16,
        eff: i128,
        hint: &Option<LiquidationPolicy>,
        oracle_price: u64,
    ) -> Option<LiquidationPolicy> {
        match hint {
            // Spec §11.2: absent hint means no liquidation action for this candidate.
            None => None,
            Some(LiquidationPolicy::FullClose) => Some(LiquidationPolicy::FullClose),
            Some(LiquidationPolicy::ExactPartial(q_close_q)) => {
                let abs_eff = eff.unsigned_abs();
                // Bounds check: 0 < q_close_q < abs(eff)
                // Spec §11.1 rule 3: invalid hint → no liquidation action (None)
                if *q_close_q == 0 || *q_close_q >= abs_eff {
                    return None;
                }

                // Stateless pre-flight: predict post-partial maintenance health.
                let account = &self.accounts[idx as usize];

                // 1. Predict liquidation fee
                let notional_closed = mul_div_floor_u128(*q_close_q, oracle_price as u128, POS_SCALE);
                let liq_fee_raw = mul_div_ceil_u128(notional_closed, self.params.liquidation_fee_bps as u128, 10_000);
                let liq_fee = core::cmp::min(
                    core::cmp::max(liq_fee_raw, self.params.min_liquidation_abs.get()),
                    self.params.liquidation_fee_cap.get(),
                );

                // 2. Predict post-partial Eq_maint_raw (settle_losses preserves C + PNL sum).
                // Model the same capped fee application as charge_fee_to_insurance:
                // only capital + collectible fee-debt headroom is actually applied.
                let cap = account.capital.get();
                let fee_from_capital = core::cmp::min(liq_fee, cap);
                let fee_shortfall = liq_fee - fee_from_capital;
                let current_fc = account.fee_credits.get();
                let fc_headroom = match current_fc.checked_add(i128::MAX) {
                    Some(h) if h > 0 => h as u128,
                    _ => 0u128,
                };
                let fee_from_debt = core::cmp::min(fee_shortfall, fc_headroom);
                let fee_applied = fee_from_capital + fee_from_debt;

                let eq_raw_wide = self.account_equity_maint_raw_wide(account);
                let predicted_eq = match eq_raw_wide.checked_sub(I256::from_u128(fee_applied)) {
                    Some(v) => v,
                    None => return None,
                };

                // 3. Predict post-partial MM_req
                let rem_eff = abs_eff - *q_close_q;
                let rem_notional = mul_div_floor_u128(rem_eff, oracle_price as u128, POS_SCALE);
                let proportional_mm = mul_div_floor_u128(rem_notional, self.params.maintenance_margin_bps as u128, 10_000);
                let predicted_mm_req = if rem_eff == 0 {
                    0u128
                } else {
                    core::cmp::max(proportional_mm, self.params.min_nonzero_mm_req)
                };

                // 4. Health check: predicted_eq > predicted_mm_req
                // Spec §11.1 rule 3: failed pre-flight → no liquidation action (None)
                if predicted_eq <= I256::from_u128(predicted_mm_req) {
                    return None;
                }

                Some(LiquidationPolicy::ExactPartial(*q_close_q))
            }
        }
    }
    }

    // ========================================================================
    // convert_released_pnl_not_atomic (spec §10.4.1)
    // ========================================================================

    /// Explicit voluntary conversion of matured released positive PnL for open-position accounts.
    pub fn convert_released_pnl_not_atomic(
        &mut self,
        idx: u16,
        x_req: u128,
        oracle_price: u64,
        now_slot: u64,
        funding_rate: i64,
    ) -> Result<()> {
                Self::validate_funding_rate(funding_rate)?;

        if oracle_price == 0 || oracle_price > MAX_ORACLE_PRICE {
            return Err(RiskError::Overflow);
        }
        if !self.is_used(idx as usize) {
            return Err(RiskError::AccountNotFound);
        }

        let mut ctx = InstructionContext::new();

        // Step 3: touch_account_full_not_atomic
        self.touch_account_full_not_atomic(idx as usize, oracle_price, now_slot)?;

        // Step 4: if flat, auto-conversion already happened in touch
        if self.accounts[idx as usize].position_basis_q == 0 {
            self.schedule_end_of_instruction_resets(&mut ctx)?;
            self.finalize_end_of_instruction_resets(&ctx);
            self.recompute_r_last_from_final_state(funding_rate)?;
            return Ok(());
        }

        // Step 5: require 0 < x_req <= ReleasedPos_i
        let released = self.released_pos(idx as usize);
        if x_req == 0 || x_req > released {
            return Err(RiskError::Overflow);
        }

        // Step 6: compute y using pre-conversion haircut (spec §7.4).
        // Because x_req > 0 implies pnl_matured_pos_tot > 0, h_den is strictly positive.
        let (h_num, h_den) = self.haircut_ratio();
        assert!(h_den > 0, "convert_released_pnl_not_atomic: h_den must be > 0 when x_req > 0");
        let y: u128 = wide_mul_div_floor_u128(x_req, h_num, h_den);

        // Step 7: consume_released_pnl(i, x_req)
        self.consume_released_pnl(idx as usize, x_req);

        // Step 8: set_capital(i, C_i + y)
        let new_cap = add_u128(self.accounts[idx as usize].capital.get(), y);
        self.set_capital(idx as usize, new_cap);

        // Step 9: sweep fee debt
        self.fee_debt_sweep(idx as usize);

        // Step 10: require maintenance healthy if still has position
        let eff = self.effective_pos_q(idx as usize);
        if eff != 0 {
            if !self.is_above_maintenance_margin(&self.accounts[idx as usize], idx as usize, oracle_price) {
                return Err(RiskError::Undercollateralized);
            }
        }

        // Steps 11-12: end-of-instruction resets
        self.schedule_end_of_instruction_resets(&mut ctx)?;
        self.finalize_end_of_instruction_resets(&ctx);
        self.recompute_r_last_from_final_state(funding_rate)?;

        Ok(())
    }

    // ========================================================================
    // close_account_not_atomic
    // ========================================================================

    pub fn close_account_not_atomic(&mut self, idx: u16, now_slot: u64, oracle_price: u64, funding_rate: i64) -> Result<u128> {
                Self::validate_funding_rate(funding_rate)?;

        if idx as usize >= MAX_ACCOUNTS || !self.is_used(idx as usize) {
            return Err(RiskError::AccountNotFound);
        }

        let mut ctx = InstructionContext::new();

        self.touch_account_full_not_atomic(idx as usize, oracle_price, now_slot)?;

        // Position must be zero
        let eff = self.effective_pos_q(idx as usize);
        if eff != 0 {
            return Err(RiskError::Undercollateralized);
        }

        // PnL must be zero (check BEFORE fee forgiveness to avoid
        // mutating fee_credits on a path that returns Err)
        if self.accounts[idx as usize].pnl > 0 {
            return Err(RiskError::PnlNotWarmedUp);
        }
        if self.accounts[idx as usize].pnl < 0 {
            return Err(RiskError::Undercollateralized);
        }

        // Forgive fee debt (safe: position is zero, PnL is zero)
        if self.accounts[idx as usize].fee_credits.get() < 0 {
            self.accounts[idx as usize].fee_credits = I128::ZERO;
        }

        let capital = self.accounts[idx as usize].capital;

        if capital > self.vault {
            return Err(RiskError::InsufficientBalance);
        }
        self.vault = self.vault - capital;
        self.set_capital(idx as usize, 0);

        // End-of-instruction resets before freeing
        self.schedule_end_of_instruction_resets(&mut ctx)?;
        self.finalize_end_of_instruction_resets(&ctx);
        self.recompute_r_last_from_final_state(funding_rate)?;

        self.free_slot(idx);

        Ok(capital.get())
    }

    // ========================================================================
    // force_close_resolved_not_atomic (resolved/frozen market path)
    // ========================================================================

    /// Force-close an account on a resolved market.
    ///
    /// `resolved_slot` is the market resolution boundary slot, used to anchor
    /// `current_slot` and realize maintenance fees through that slot.
    ///
    /// Settles K-pair PnL, zeros position, settles losses, absorbs from
    /// insurance, converts profit (bypassing warmup), sweeps fee debt,
    /// forgives remainder, returns capital, frees slot.
    ///
    /// Skips accrue_market_to (market is frozen). Handles both same-epoch
    /// and epoch-mismatch accounts.
    pub fn force_close_resolved_not_atomic(&mut self, idx: u16, resolved_slot: u64, funding_rate: i64) -> Result<u128> {
        Self::validate_funding_rate(funding_rate)?;

        if idx as usize >= MAX_ACCOUNTS || !self.is_used(idx as usize) {
            return Err(RiskError::AccountNotFound);
        }
        if resolved_slot < self.current_slot {
            return Err(RiskError::Overflow);
        }
        self.current_slot = resolved_slot;

        let mut ctx = InstructionContext::new();

        let i = idx as usize;

        // Step 1: Settle K-pair PnL and zero position.
        // Uses validate-then-mutate: compute pnl_delta and validate all checked
        // ops BEFORE any mutation, preventing partial-mutation-on-error.
        // Does NOT call settle_side_effects (which interleaves mutations with
        // fallible checked_sub on stale_count).
        if self.accounts[i].position_basis_q != 0 {
            let basis = self.accounts[i].position_basis_q;
            let abs_basis = basis.unsigned_abs();
            let a_basis = self.accounts[i].adl_a_basis;
            let k_snap = self.accounts[i].adl_k_snap;
            let side = side_of_i128(basis).unwrap();
            let epoch_snap = self.accounts[i].adl_epoch_snap;
            let epoch_side = self.get_epoch_side(side);

            // Reject corrupt ADL state (a_basis must be > 0 for any position)
            if a_basis == 0 {
                return Err(RiskError::CorruptState);
            }

            // Phase 1: COMPUTE (no mutations)
            let k_end = if epoch_snap == epoch_side {
                self.get_k_side(side)
            } else {
                self.get_k_epoch_start(side)
            };
            let den = a_basis.checked_mul(POS_SCALE).ok_or(RiskError::Overflow)?;
            let pnl_delta = wide_signed_mul_div_floor_from_k_pair(abs_basis, k_snap, k_end, den);

            // Phase 1b: VALIDATE (check all fallible ops before mutating)
            let new_pnl = self.accounts[i].pnl.checked_add(pnl_delta)
                .ok_or(RiskError::Overflow)?;
            if new_pnl == i128::MIN {
                return Err(RiskError::Overflow);
            }
            // Compute OI decrement before any mutation.
            // In resolved-market force-close, OI may already be partially or
            // fully decremented by prior force-closes of the opposing side.
            // Use saturating_sub for both sides to handle this gracefully.
            let eff = self.effective_pos_q(i);
            let eff_abs = eff.unsigned_abs();

            if epoch_snap != epoch_side {
                // Validate epoch adjacency (same check as settle_side_effects
                // minus the ResetPending mode check, which is relaxed for
                // resolved markets where the side may be in any mode)
                if epoch_snap.checked_add(1) != Some(epoch_side) {
                    return Err(RiskError::CorruptState);
                }
                let old_stale = self.get_stale_count(side);
                if old_stale == 0 {
                    return Err(RiskError::CorruptState);
                }
            }

            // Phase 2: MUTATE (all validated, safe to commit)
            if pnl_delta != 0 {
                let old_r = self.accounts[i].reserved_pnl;
                self.set_pnl(i, new_pnl);
                if self.accounts[i].reserved_pnl > old_r {
                    self.restart_warmup_after_reserve_increase(i);
                }
            }

            // Decrement stale count (pre-validated above)
            if epoch_snap != epoch_side {
                let old_stale = self.get_stale_count(side);
                self.set_stale_count(side, old_stale - 1);
            }

            // Decrement OI bilaterally — saturating for both sides because
            // prior force-closes of the opposing side may have already zeroed OI.
            if eff_abs > 0 {
                self.oi_eff_long_q = self.oi_eff_long_q.saturating_sub(eff_abs);
                self.oi_eff_short_q = self.oi_eff_short_q.saturating_sub(eff_abs);
            }

            // Account for same-epoch phantom dust before zeroing (same logic
            // as attach_effective_position detach path, spec §4.5/§4.6)
            if epoch_snap == epoch_side && a_basis != 0 {
                let a_side_val = self.get_a_side(side);
                let product = U256::from_u128(abs_basis)
                    .checked_mul(U256::from_u128(a_side_val));
                if let Some(p) = product {
                    let rem = p.checked_rem(U256::from_u128(a_basis));
                    if let Some(r) = rem {
                        if !r.is_zero() {
                            self.inc_phantom_dust_bound(side);
                        }
                    }
                }
            }

            // Zero position
            self.set_position_basis_q(i, 0);
            self.accounts[i].adl_a_basis = ADL_ONE;
            self.accounts[i].adl_k_snap = 0;
            self.accounts[i].adl_epoch_snap = 0;
        }

        // Step 2: Settle losses from principal (senior to fees)
        self.settle_losses(i);

        // Step 3: Absorb any remaining flat negative PnL
        self.resolve_flat_negative(i);

        // Step 3b: Realize recurring maintenance fees (spec §8.2).
        // After losses and flat-negative absorption, matching touch_account_full_not_atomic
        // ordering where fees are junior to trading losses.
        self.settle_maintenance_fee_internal(i, self.current_slot)?;

        // Step 4: Convert positive PnL to capital (bypass warmup for resolved market).
        // Uses the same release-then-haircut order as do_profit_conversion and
        // convert_released_pnl_not_atomic. Sequential closers see progressively larger
        // pnl_matured_pos_tot denominators, which is the same behavior as normal
        // sequential profit conversion — this is inherent to the haircut model,
        // not a force_close-specific issue.
        if self.accounts[i].pnl > 0 {
            // Release all reserves unconditionally (bypass warmup)
            self.set_reserved_pnl(i, 0);
            // Convert using post-release haircut
            let released = self.released_pos(i);
            if released > 0 {
                let (h_num, h_den) = self.haircut_ratio();
                let y = if h_den == 0 { released } else {
                    wide_mul_div_floor_u128(released, h_num, h_den)
                };
                self.consume_released_pnl(i, released);
                let new_cap = add_u128(self.accounts[i].capital.get(), y);
                self.set_capital(i, new_cap);
            }
        }

        // Step 5: Sweep fee debt from capital
        self.fee_debt_sweep(i);

        // Step 6: Forgive any remaining fee debt
        if self.accounts[i].fee_credits.get() < 0 {
            self.accounts[i].fee_credits = I128::ZERO;
        }

        // Step 7: Return capital and free slot
        let capital = self.accounts[i].capital;
        if capital > self.vault {
            return Err(RiskError::InsufficientBalance);
        }
        self.vault = self.vault - capital;
        self.set_capital(i, 0);

        // End-of-instruction resets before freeing (spec §5.7-5.8, §10.0)
        self.schedule_end_of_instruction_resets(&mut ctx)?;
        self.finalize_end_of_instruction_resets(&ctx);
        self.recompute_r_last_from_final_state(funding_rate)?;

        self.free_slot(idx);

        Ok(capital.get())
    }

    // ========================================================================
    // Permissionless account reclamation (spec §10.7 + §2.6)
    // ========================================================================

    /// reclaim_empty_account_not_atomic(i, now_slot) — permissionless O(1) empty/dust-account recycling.
    /// Spec §10.7: MUST NOT call accrue_market_to, MUST NOT mutate side state,
    /// MUST NOT materialize any account. Realizes recurring maintenance fees
    /// on the already-flat state before checking final reclaim eligibility.
    pub fn reclaim_empty_account_not_atomic(&mut self, idx: u16, now_slot: u64) -> Result<()> {
        if idx as usize >= MAX_ACCOUNTS || !self.is_used(idx as usize) {
            return Err(RiskError::AccountNotFound);
        }
        if now_slot < self.current_slot {
            return Err(RiskError::Overflow);
        }

        // Step 3: Pre-realization flat-clean preconditions (spec §10.7 / §2.6)
        let account = &self.accounts[idx as usize];
        if account.position_basis_q != 0 {
            return Err(RiskError::Undercollateralized);
        }
        if account.pnl != 0 {
            return Err(RiskError::Undercollateralized);
        }
        if account.reserved_pnl != 0 {
            return Err(RiskError::Undercollateralized);
        }
        if account.fee_credits.get() > 0 {
            return Err(RiskError::Undercollateralized);
        }

        // Step 4: anchor current_slot
        self.current_slot = now_slot;

        // Step 5: realize recurring maintenance fees (spec §8.2.3 item 3)
        self.settle_maintenance_fee_internal(idx as usize, now_slot)?;

        // Step 6: final reclaim-eligibility check (spec §2.6)
        // C_i must be 0 or dust (< MIN_INITIAL_DEPOSIT)
        if self.accounts[idx as usize].capital.get() >= self.params.min_initial_deposit.get()
            && !self.accounts[idx as usize].capital.is_zero()
        {
            return Err(RiskError::Undercollateralized);
        }

        // Step 7: reclamation effects (spec §2.6)
        let dust_cap = self.accounts[idx as usize].capital.get();
        if dust_cap > 0 {
            self.set_capital(idx as usize, 0);
            self.insurance_fund.balance = self.insurance_fund.balance + dust_cap;
        }

        // Forgive uncollectible fee debt (spec §2.6)
        if self.accounts[idx as usize].fee_credits.get() < 0 {
            self.accounts[idx as usize].fee_credits = I128::new(0);
        }

        // Free the slot
        self.free_slot(idx);

        Ok(())
    }

    // ========================================================================
    // Garbage collection
    // ========================================================================

    test_visible! {
    fn garbage_collect_dust(&mut self) -> u32 {
        let mut to_free: [u16; GC_CLOSE_BUDGET as usize] = [0; GC_CLOSE_BUDGET as usize];
        let mut num_to_free = 0usize;

        let max_scan = (ACCOUNTS_PER_CRANK as usize).min(MAX_ACCOUNTS);
        let start = self.gc_cursor as usize;

        let mut scanned: usize = 0;
        for offset in 0..max_scan {
            if num_to_free >= GC_CLOSE_BUDGET as usize {
                break;
            }
            scanned = offset + 1;

            let idx = (start + offset) & ACCOUNT_IDX_MASK;
            let block = idx >> 6;
            let bit = idx & 63;
            if (self.used[block] & (1u64 << bit)) == 0 {
                continue;
            }

            // Dust predicate: check flat-clean preconditions BEFORE fee realization
            // (matching reclaim_empty_account_not_atomic pattern — spec §8.2.3).
            let account = &self.accounts[idx];
            if account.position_basis_q != 0 {
                continue;
            }
            if account.pnl != 0 {
                continue;
            }
            if account.reserved_pnl != 0 {
                continue;
            }
            if account.fee_credits.get() > 0 {
                continue;
            }

            // Realize recurring maintenance fees on already-flat state (spec §8.2.3).
            // Only called after flat-clean preconditions are verified.
            if self.settle_maintenance_fee_internal(idx, self.current_slot).is_err() {
                continue;
            }

            // Re-check capital after fee realization (fee may have reduced it)
            if self.accounts[idx].capital.get() >= self.params.min_initial_deposit.get()
                && !self.accounts[idx].capital.is_zero() {
                continue;
            }

            // Sweep dust capital into insurance (spec §2.6)
            let dust_cap = self.accounts[idx].capital.get();
            if dust_cap > 0 {
                self.set_capital(idx, 0);
                self.insurance_fund.balance = self.insurance_fund.balance + dust_cap;
            }

            // Forgive uncollectible fee debt (spec §2.6)
            if self.accounts[idx].fee_credits.get() < 0 {
                self.accounts[idx].fee_credits = I128::new(0);
            }

            to_free[num_to_free] = idx as u16;
            num_to_free += 1;
        }

        // Advance cursor by actual number of offsets scanned, not max_scan.
        // Prevents skipping unscanned accounts on early break.
        self.gc_cursor = ((start + scanned) & ACCOUNT_IDX_MASK) as u16;

        for i in 0..num_to_free {
            self.free_slot(to_free[i]);
        }

        num_to_free as u32
    }
    }

    // ========================================================================
    // Crank freshness
    // ========================================================================

    fn require_fresh_crank(&self, now_slot: u64) -> Result<()> {
        if now_slot.saturating_sub(self.last_crank_slot) > self.max_crank_staleness_slots {
            return Err(RiskError::Unauthorized);
        }
        Ok(())
    }

    fn require_recent_full_sweep(&self, now_slot: u64) -> Result<()> {
        if now_slot.saturating_sub(self.last_full_sweep_start_slot) > self.max_crank_staleness_slots {
            return Err(RiskError::Unauthorized);
        }
        Ok(())
    }

    // ========================================================================
    // Insurance fund operations
    // ========================================================================

    pub fn top_up_insurance_fund(&mut self, amount: u128, now_slot: u64) -> Result<bool> {
        // Spec §10.3.2: time monotonicity
        if now_slot < self.current_slot {
            return Err(RiskError::Overflow);
        }
        // Validate-then-mutate: all checks before any state change
        let new_vault = self.vault.get().checked_add(amount)
            .ok_or(RiskError::Overflow)?;
        if new_vault > MAX_VAULT_TVL {
            return Err(RiskError::Overflow);
        }
        let new_ins = self.insurance_fund.balance.get().checked_add(amount)
            .ok_or(RiskError::Overflow)?;
        // All checks passed — commit
        self.current_slot = now_slot;
        self.vault = U128::new(new_vault);
        self.insurance_fund.balance = U128::new(new_ins);
        Ok(self.insurance_fund.balance.get() > self.params.insurance_floor.get())
    }

    // set_insurance_floor removed — configuration immutability (spec §2.2.1).
    // Insurance floor is fixed at initialization and cannot be changed at runtime.

    // ========================================================================
    // Fee credits
    // ========================================================================

    pub fn deposit_fee_credits(&mut self, idx: u16, amount: u128, now_slot: u64) -> Result<()> {
        if idx as usize >= MAX_ACCOUNTS || !self.is_used(idx as usize) {
            return Err(RiskError::Unauthorized);
        }
        if now_slot < self.current_slot {
            return Err(RiskError::Unauthorized);
        }
        // Cap at outstanding debt to enforce spec §2.1 invariant: fee_credits <= 0
        let debt = fee_debt_u128_checked(self.accounts[idx as usize].fee_credits.get());
        let capped = amount.min(debt);
        if capped == 0 {
            self.current_slot = now_slot;
            return Ok(()); // no debt to pay off
        }
        if capped > i128::MAX as u128 {
            return Err(RiskError::Overflow);
        }
        let new_vault = self.vault.get().checked_add(capped)
            .ok_or(RiskError::Overflow)?;
        if new_vault > MAX_VAULT_TVL {
            return Err(RiskError::Overflow);
        }
        let new_ins = self.insurance_fund.balance.get().checked_add(capped)
            .ok_or(RiskError::Overflow)?;
        let new_credits = self.accounts[idx as usize].fee_credits
            .checked_add(capped as i128)
            .ok_or(RiskError::Overflow)?;
        // All checks passed — commit state
        self.current_slot = now_slot;
        self.vault = U128::new(new_vault);
        self.insurance_fund.balance = U128::new(new_ins);
        self.accounts[idx as usize].fee_credits = new_credits;
        Ok(())
    }

    #[cfg(any(test, feature = "test", kani))]
    test_visible! {
    fn add_fee_credits(&mut self, idx: u16, amount: u128) -> Result<()> {
        if idx as usize >= MAX_ACCOUNTS || !self.is_used(idx as usize) {
            return Err(RiskError::Unauthorized);
        }
        self.accounts[idx as usize].fee_credits = self.accounts[idx as usize]
            .fee_credits.saturating_add(amount as i128);
        Ok(())
    }
    }

    // ========================================================================
    // Recompute aggregates (test helper)
    // ========================================================================

    test_visible! {
    fn recompute_aggregates(&mut self) {
        let mut c_tot = 0u128;
        let mut pnl_pos_tot = 0u128;
        let mut pnl_matured_pos_tot = 0u128;
        self.for_each_used(|_idx, account| {
            c_tot = c_tot.saturating_add(account.capital.get());
            let pos_pnl = i128_clamp_pos(account.pnl);
            pnl_pos_tot = pnl_pos_tot.saturating_add(pos_pnl);
            let released = pos_pnl.saturating_sub(account.reserved_pnl);
            pnl_matured_pos_tot = pnl_matured_pos_tot.saturating_add(released);
        });
        self.c_tot = U128::new(c_tot);
        self.pnl_pos_tot = pnl_pos_tot;
        self.pnl_matured_pos_tot = pnl_matured_pos_tot;
    }
    }

    // ========================================================================
    // Utilities
    // ========================================================================

    test_visible! {
    fn advance_slot(&mut self, slots: u64) {
        self.current_slot = self.current_slot.saturating_add(slots);
    }
    }

    /// Count used accounts
    test_visible! {
    fn count_used(&self) -> u64 {
        let mut count = 0u64;
        self.for_each_used(|_, _| {
            count += 1;
        });
        count
    }
    }
}

// ============================================================================
// Free-standing helpers
// ============================================================================

/// Set pending reset on a side in the instruction context
fn set_pending_reset(ctx: &mut InstructionContext, side: Side) {
    match side {
        Side::Long => ctx.pending_reset_long = true,
        Side::Short => ctx.pending_reset_short = true,
    }
}

/// Multiply a u128 by an i128 returning i128 (checked).
/// Computes u128 * i128 → i128. Used for A_side * delta_p in accrue_market_to.
pub fn checked_u128_mul_i128(a: u128, b: i128) -> Result<i128> {
    if a == 0 || b == 0 {
        return Ok(0i128);
    }
    let negative = b < 0;
    let abs_b = if b == i128::MIN {
        return Err(RiskError::Overflow);
    } else {
        b.unsigned_abs()
    };
    // a * abs_b may overflow u128, use wide arithmetic
    let product = U256::from_u128(a).checked_mul(U256::from_u128(abs_b))
        .ok_or(RiskError::Overflow)?;
    // Bound to i128::MAX magnitude for both signs. Excludes i128::MIN (which is
    // forbidden throughout the engine) and avoids -(i128::MIN) negate panic.
    match product.try_into_u128() {
        Some(v) if v <= i128::MAX as u128 => {
            if negative {
                Ok(-(v as i128))
            } else {
                Ok(v as i128)
            }
        }
        _ => Err(RiskError::Overflow),
    }
}

/// Compute trade PnL: floor_div_signed_conservative(size_q * price_diff, POS_SCALE)
/// Uses native i128 arithmetic (spec §1.5.1 shows trade slippage fits in i128).
pub fn compute_trade_pnl(size_q: i128, price_diff: i128) -> Result<i128> {
    if size_q == 0 || price_diff == 0 {
        return Ok(0i128);
    }

    // Determine sign of result
    let neg_size = size_q < 0;
    let neg_price = price_diff < 0;
    let result_negative = neg_size != neg_price;

    let abs_size = size_q.unsigned_abs();
    let abs_price = price_diff.unsigned_abs();

    // Use wide_signed_mul_div_floor_from_k_pair style computation
    // abs_size * abs_price / POS_SCALE with signed floor rounding
    let abs_size_u256 = U256::from_u128(abs_size);
    let abs_price_u256 = U256::from_u128(abs_price);
    let ps_u256 = U256::from_u128(POS_SCALE);

    // div_rem using mul_div_floor_u256_with_rem (internally computes wide product)
    let (q, r) = mul_div_floor_u256_with_rem(abs_size_u256, abs_price_u256, ps_u256);

    if result_negative {
        // mag = q + 1 if r != 0, else q (floor toward -inf)
        let mag = if !r.is_zero() {
            q.checked_add(U256::ONE).ok_or(RiskError::Overflow)?
        } else {
            q
        };
        // Bound to i128::MAX magnitude to avoid -(i128::MIN) negate panic.
        // i128::MIN is forbidden throughout the engine.
        match mag.try_into_u128() {
            Some(v) if v <= i128::MAX as u128 => {
                Ok(-(v as i128))
            }
            _ => Err(RiskError::Overflow),
        }
    } else {
        match q.try_into_u128() {
            Some(v) if v <= i128::MAX as u128 => Ok(v as i128),
            _ => Err(RiskError::Overflow),
        }
    }
}


