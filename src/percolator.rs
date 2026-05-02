//! Formally Verified Risk Engine for Perpetual DEX — v12.19.53
//!
//! Implements the v12.19.53 spec.
//!
//! This module implements a formally verified risk engine that guarantees:
//! 1. Protected principal for flat accounts
//! 2. PNL warmup prevents instant withdrawal of manipulated profits
//! 3. ADL via lazy A/K side indices on the opposing OI side
//! 4. Conservation of funds across all operations (V >= C_tot + I)
//! 5. Bankruptcy socialization primarily through explicit A/K state. In the rare
//!    case of K-space i128 overflow during ADL, the remaining deficit falls to
//!    implicit global haircut (h) rather than panicking — preserving liquidation
//!    liveness at the cost of reducing the opposing side's junior PnL claims.
//!
//! # Atomicity Model
//!
//! Public functions suffixed with `_not_atomic` run directly inside an
//! engine-local rollback guard, or purely delegate to a guarded `_not_atomic`
//! callee: if they return `Err`, engine state is restored to its pre-call
//! snapshot before the error is returned. Most guards snapshot only the
//! non-account engine state plus the explicitly touched account slots; dynamic
//! keeper scans snapshot the bounded candidate and round-robin touch sets. The
//! suffix remains a deployment reminder that wrappers still own outer
//! transaction atomicity for token transfers, account persistence, and
//! multi-account side effects.
//!
//! On Solana SVM, any `Err` return from an instruction aborts the transaction
//! and rolls back all account state automatically. This is the expected
//! deployment model.
//!
//! Public functions WITHOUT the suffix (`top_up_insurance_fund`,
//! `deposit_fee_credits`, `accrue_market_to`) use validate-then-mutate:
//! `Err` means no state was changed.
//!
//! Internal helpers (`enqueue_adl`, `liquidate_at_oracle_internal`, etc.)
//! are not individually atomic — they rely on the calling `_not_atomic`
//! method to propagate `Err` to the transaction boundary.

#![no_std]
#![forbid(unsafe_code)]
#![allow(unused_doc_comments)]
#![allow(
    clippy::bool_comparison,
    clippy::collapsible_if,
    clippy::doc_lazy_continuation,
    clippy::empty_line_after_doc_comments,
    clippy::if_same_then_else,
    clippy::manual_div_ceil,
    clippy::manual_is_multiple_of,
    clippy::manual_saturating_arithmetic,
    clippy::manual_unwrap_or,
    clippy::needless_range_loop,
    clippy::new_without_default,
    clippy::question_mark,
    clippy::should_implement_trait,
    clippy::too_many_arguments,
    clippy::unnecessary_cast,
    clippy::unnecessary_map_or
)]

#[cfg(kani)]
extern crate kani;

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
        #[cfg(any(feature = "test", feature = "stress", kani))]
        pub fn $name($($args)*) $(-> $ret)? $body

        $(#[$meta])*
        #[cfg(not(any(feature = "test", feature = "stress", kani)))]
        fn $name($($args)*) $(-> $ret)? $body
    };
}

macro_rules! rollback_on_error {
    ($self:ident, [$($idx:expr),* $(,)?], $body:block) => {{
        let __before = $self.rollback_snapshot_accounts([$($idx as usize),*]);
        #[allow(clippy::redundant_closure_call)]
        let __result = (|| $body)();
        if __result.is_err() {
            $self.restore_rollback_snapshot(__before);
        }
        __result
    }};
}

// ============================================================================
// Constants
// ============================================================================

#[cfg(kani)]
pub const MAX_ACCOUNTS: usize = 4;

#[cfg(all(feature = "test", not(kani)))]
pub const MAX_ACCOUNTS: usize = 64;

// Deployment-scale capacity tiers. Priority cascade: kani > test > small >
// medium > default. Each tier must remain a power of two (static assert below).
#[cfg(all(feature = "small", not(feature = "test"), not(kani)))]
pub const MAX_ACCOUNTS: usize = 256;

#[cfg(all(
    feature = "medium",
    not(feature = "small"),
    not(feature = "test"),
    not(kani)
))]
pub const MAX_ACCOUNTS: usize = 1024;

#[cfg(all(
    not(kani),
    not(feature = "test"),
    not(feature = "small"),
    not(feature = "medium")
))]
pub const MAX_ACCOUNTS: usize = 4096;

pub const BITMAP_WORDS: usize = (MAX_ACCOUNTS + 63) / 64;
pub const MAX_ROUNDING_SLACK: u128 = MAX_ACCOUNTS as u128;
const _: () = assert!(MAX_ACCOUNTS.is_power_of_two());

// Liquidation Phase 1 budget is passed directly to keeper_crank_*.

/// POS_SCALE = 1_000_000 (spec §1.2)
pub const POS_SCALE: u128 = 1_000_000;

/// ADL_ONE = 1e15 (spec §1.3)
pub const ADL_ONE: u128 = 1_000_000_000_000_000;

/// MIN_A_SIDE = 1e14 (spec §1.4)
pub const MIN_A_SIDE: u128 = 100_000_000_000_000;

/// MAX_ORACLE_PRICE = 1_000_000_000_000 (spec §1.4)
pub const MAX_ORACLE_PRICE: u64 = 1_000_000_000_000;

/// FUNDING_DEN = 1_000_000_000 (spec §5.4)
pub const FUNDING_DEN: u128 = 1_000_000_000;

/// STRESS_CONSUMPTION_SCALE = 1e9 (spec §1.4 / §5.3)
pub const STRESS_CONSUMPTION_SCALE: u128 = 1_000_000_000;

/// MAX_ABS_FUNDING_E9_PER_SLOT = 10_000 (spec §1.4, parts-per-billion).
///
/// Engine-wide ceiling on the wrapper-supplied funding rate. Deliberately
/// set far below the 1e9 parts-per-billion maximum so cumulative F_side_num
/// cannot saturate `i128` within a production market horizon. With
/// ADL_ONE=1e15, MAX_ORACLE_PRICE=1e12, and the init-time envelope
/// `ADL_ONE * MAX_ORACLE_PRICE * max_abs_funding_e9_per_slot *
/// min_funding_lifetime_slots <= i128::MAX`, a rate ceiling of 1e4 allows
/// a worst-case cumulative-F lifetime of up to ~1.7e7 slots (400ms slots
/// → ~7.89e7 slots/year, so ~0.22 years ≈ 2.6 months at sustained
/// max-rate funding in one direction). Realistic operating rates are
/// orders of magnitude smaller; observed horizons at typical rates are
/// measured in decades to centuries.
pub const MAX_ABS_FUNDING_E9_PER_SLOT: i128 = 10_000;

// Normative bounds (spec §1.4)
pub const MAX_VAULT_TVL: u128 = 10_000_000_000_000_000;
pub const MAX_POSITION_ABS_Q: u128 = 100_000_000_000_000;
pub const MAX_ACCOUNT_NOTIONAL: u128 = 100_000_000_000_000_000_000;
pub const MAX_TRADE_SIZE_Q: u128 = MAX_POSITION_ABS_Q; // spec §1.4
pub const MAX_OI_SIDE_Q: u128 = 100_000_000_000_000;
pub const MAX_ACCOUNT_POSITIVE_PNL: u128 = 100_000_000_000_000_000_000_000_000_000_000;
pub const MAX_PNL_POS_TOT: u128 = 100_000_000_000_000_000_000_000_000_000_000_000_000;
pub const MAX_TRADING_FEE_BPS: u64 = 10_000;
pub const MAX_MARGIN_BPS: u64 = 10_000;
pub const MAX_LIQUIDATION_FEE_BPS: u64 = 10_000;
pub const MAX_PROTOCOL_FEE_ABS: u128 = 1_000_000_000_000_000_000_000_000_000_000_000_000; // 10^36, spec §1.4

pub const MAX_WARMUP_SLOTS: u64 = u64::MAX;
pub const MAX_RESOLVE_PRICE_DEVIATION_BPS: u64 = 10_000;
const NO_SLOT: u64 = u64::MAX;

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
    ceil_div_positive_checked, fee_debt_u128_checked, mul_div_ceil_u128, mul_div_ceil_u256,
    mul_div_floor_u128, mul_div_floor_u256_with_rem, wide_mul_div_ceil_u128_or_over_i128max,
    wide_mul_div_floor_u128, OverI128Magnitude, I256, U256,
};

// ============================================================================
// Core Data Structures
// ============================================================================

// AccountKind as plain u8 — eliminates UB risk from invalid enum discriminants
// when casting raw slab bytes to &Account via zero-copy. u8 has no invalid
// representations, so &*(ptr as *const Account) is always sound.
// pub enum AccountKind { User = 0, LP = 1 }  // replaced by constants below

/// Market mode (spec §2.2).
///
/// **Repr contract:** `#[repr(u8)]` with discriminants fixed at
/// `Live=0, Resolved=1`. The byte layout in `RiskEngine::market_mode` is
/// equivalent to a `u8` at those values. Any other byte value is UB when
/// reached through a safe `&RiskEngine`/`&mut RiskEngine` reference —
/// forming such a reference over memory with invalid discriminant bytes
/// is the caller's obligation to avoid (init_in_place's safety
/// contract). Zero-initialized memory is always valid (maps to `Live`).
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarketMode {
    Live = 0,
    Resolved = 1,
}

/// Resolve-branch selector for `resolve_market_not_atomic` (spec §9.8).
///
/// The ordinary vs degenerate branch is selected explicitly. Equality of
/// economic values such as `live_oracle_price == P_last` or
/// `funding_rate_e9_per_slot == 0` does not imply degenerate mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResolveMode {
    /// Self-synchronizing live-sync branch. Accrues market to `now_slot` using
    /// the supplied `live_oracle_price` and `funding_rate_e9_per_slot`, then
    /// enforces the deviation-band check against `resolved_price`.
    Ordinary = 0,
    /// Privileged recovery branch. Skips additional live accrual after
    /// `slot_last` and skips the deviation-band check. MUST be entered only
    /// when the wrapper explicitly selects it AND supplies `live_oracle_price
    /// == P_last` AND `funding_rate_e9_per_slot == 0`.
    Degenerate = 1,
}

/// Reserve mode for set_pnl (spec §4.8)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReserveMode {
    /// Admission-pair: engine decides h_eff from (h_min, h_max) at reserve creation time
    UseAdmissionPair(u64, u64),
    /// Immediate release, only valid in Resolved mode (fails on Live)
    ImmediateReleaseResolvedOnly,
    /// Positive increase is forbidden (returns Err)
    NoPositiveIncreaseAllowed,
}

/// Side mode for OI sides (spec §2.4).
///
/// **Repr contract:** same as `MarketMode` — `#[repr(u8)]` with fixed
/// discriminants `Normal=0, DrainOnly=1, ResetPending=2`. Zero-initialized
/// memory maps to `Normal`. See `MarketMode` docstring for the
/// safe-reference discriminant-validity contract.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SideMode {
    Normal = 0,
    DrainOnly = 1,
    ResetPending = 2,
}

/// Max accounts that can be touched in a single instruction.
pub const MAX_TOUCHED_PER_INSTRUCTION: usize = 256;
const KEEPER_ROLLBACK_ACCOUNT_SLOTS: usize = MAX_TOUCHED_PER_INSTRUCTION * 2;

/// h_max sticky set is stored as a bitmap indexed by storage slot.
/// Size = BITMAP_WORDS (same sizing as the allocator's `used` bitmap).
/// Storage cost: (MAX_ACCOUNTS / 8) bytes. At MAX_ACCOUNTS=4096, 512 bytes.
/// Lookup/insert are O(1).

/// Instruction context for deferred reset scheduling and shared touched-account tracking.
pub struct InstructionContext {
    pub pending_reset_long: bool,
    pub pending_reset_short: bool,
    /// Instruction-local bankruptcy h-max candidate. This is set before
    /// commit when the instruction discovers a live post-principal loss tail.
    pub bankruptcy_hmax_candidate_active: bool,
    /// True once this instruction has used ordinary positive-PnL lanes. If a
    /// later bankruptcy candidate appears, the instruction must fail rather
    /// than commit stale h-min/release/conversion decisions.
    pub positive_pnl_usability_mutated: bool,
    /// True once this instruction starts or restarts the stress/h-max envelope.
    /// Same-instruction Phase 2 inspections must not reduce the new envelope.
    pub stress_envelope_restarted: bool,
    /// Phase 2-local conservative guard. This blocks positive-PnL usability
    /// before replaying a cursor window that may discover a latent bankruptcy,
    /// but it is not itself a real stress/h-max event.
    pub speculative_hmax_guard_active: bool,
    /// Shared admission pair for this instruction
    pub admit_h_min_shared: u64,
    pub admit_h_max_shared: u64,
    /// Optional scaled consumption-threshold gate (spec §4.7, v12.19.53).
    /// `None` disables step 2 of `admit_fresh_reserve_h_lock`.
    /// Public entrypoints accept whole-bps thresholds; the context stores
    /// `threshold * STRESS_CONSUMPTION_SCALE`.
    /// `Some(0)` is invalid at input validation time — callers must
    /// pass `None` to disable, never `Some(0)`.
    pub admit_h_max_consumption_threshold_bps_opt_shared: Option<u128>,
    /// Deduplicated touched accounts, maintained in ascending-index order
    /// by sorted-insert in `add_touched`. No separate sort pass required
    /// in finalize_touched_accounts_post_live.
    pub touched_accounts: [u16; MAX_TOUCHED_PER_INSTRUCTION],
    pub touched_count: u16,
    /// Live local-touch finalization is single-use. Once set, no later touch
    /// can be added to this context and finalization cannot run again.
    pub finalized: bool,
    /// Per-instruction sticky set: accounts that required admit_h_max.
    /// Bitmap indexed by storage slot for O(1) membership test/insert.
    pub h_max_sticky_bitmap: [u64; BITMAP_WORDS],
}

impl InstructionContext {
    pub fn new() -> Self {
        Self {
            pending_reset_long: false,
            pending_reset_short: false,
            bankruptcy_hmax_candidate_active: false,
            positive_pnl_usability_mutated: false,
            stress_envelope_restarted: false,
            speculative_hmax_guard_active: false,
            admit_h_min_shared: 0,
            admit_h_max_shared: 0,
            admit_h_max_consumption_threshold_bps_opt_shared: None,
            touched_accounts: [0; MAX_TOUCHED_PER_INSTRUCTION],
            touched_count: 0,
            finalized: false,
            h_max_sticky_bitmap: [0; BITMAP_WORDS],
        }
    }

    pub fn new_with_admission(admit_h_min: u64, admit_h_max: u64) -> Self {
        Self {
            pending_reset_long: false,
            pending_reset_short: false,
            bankruptcy_hmax_candidate_active: false,
            positive_pnl_usability_mutated: false,
            stress_envelope_restarted: false,
            speculative_hmax_guard_active: false,
            admit_h_min_shared: admit_h_min,
            admit_h_max_shared: admit_h_max,
            admit_h_max_consumption_threshold_bps_opt_shared: None,
            touched_accounts: [0; MAX_TOUCHED_PER_INSTRUCTION],
            touched_count: 0,
            finalized: false,
            h_max_sticky_bitmap: [0; BITMAP_WORDS],
        }
    }

    /// Construct with admission pair and consumption-threshold gate.
    pub fn new_with_admission_and_threshold(
        admit_h_min: u64,
        admit_h_max: u64,
        threshold_opt: Option<u128>,
    ) -> Self {
        Self {
            pending_reset_long: false,
            pending_reset_short: false,
            bankruptcy_hmax_candidate_active: false,
            positive_pnl_usability_mutated: false,
            stress_envelope_restarted: false,
            speculative_hmax_guard_active: false,
            admit_h_min_shared: admit_h_min,
            admit_h_max_shared: admit_h_max,
            admit_h_max_consumption_threshold_bps_opt_shared: threshold_opt
                .map(|t| t.checked_mul(STRESS_CONSUMPTION_SCALE).unwrap_or(u128::MAX)),
            touched_accounts: [0; MAX_TOUCHED_PER_INSTRUCTION],
            touched_count: 0,
            finalized: false,
            h_max_sticky_bitmap: [0; BITMAP_WORDS],
        }
    }

    /// Check if account is in sticky set. O(1) bitmap test.
    pub fn is_h_max_sticky(&self, idx: u16) -> bool {
        let i = idx as usize;
        if i >= MAX_ACCOUNTS {
            return false;
        }
        let word = i / 64;
        let bit = i % 64;
        (self.h_max_sticky_bitmap[word] >> bit) & 1 == 1
    }

    /// Insert account into sticky set. O(1) bitmap set.
    pub fn mark_h_max_sticky(&mut self, idx: u16) -> bool {
        let i = idx as usize;
        if i >= MAX_ACCOUNTS {
            return false;
        }
        let word = i / 64;
        let bit = i % 64;
        self.h_max_sticky_bitmap[word] |= 1u64 << bit;
        true
    }

    pub fn mark_positive_pnl_usability(&mut self) {
        self.positive_pnl_usability_mutated = true;
    }

    /// Add account to touched set, maintaining ascending-index order.
    /// O(log n) search + O(n) shift-on-insert.
    /// Returns true on success (including dedup hit), false if this context
    /// has already finalized or touched-set capacity is exceeded. Callers
    /// MUST propagate false as a conservative failure.
    pub fn add_touched(&mut self, idx: u16) -> bool {
        if self.finalized {
            return false;
        }
        let count = self.touched_count as usize;
        // Binary search: find insertion point. If idx already present,
        // dedup with no mutation.
        let mut lo = 0usize;
        let mut hi = count;
        while lo < hi {
            let mid = (lo + hi) / 2;
            let v = self.touched_accounts[mid];
            if v == idx {
                return true;
            } // already present
            if v < idx {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        // lo is the insertion point.
        if count >= MAX_TOUCHED_PER_INSTRUCTION {
            return false;
        }
        // Shift [lo, count) right by one, then insert at lo.
        let mut j = count;
        while j > lo {
            self.touched_accounts[j] = self.touched_accounts[j - 1];
            j -= 1;
        }
        self.touched_accounts[lo] = idx;
        self.touched_count += 1;
        true
    }
}

/// Unified account (spec §2.1)
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Account {
    pub capital: U128,
    /// Wrapper-owned account-kind annotation (spec §2.1.1, non-normative).
    /// The engine stores and canonicalizes `kind` but MUST NOT read it for
    /// any spec-normative decision (margin, liquidation, fees, accrual,
    /// resolution). `is_lp()` / `is_user()` are wrapper conveniences only.
    pub kind: u8, // 0 = User, 1 = LP

    /// Realized PnL (i128, spec §2.1)
    pub pnl: i128,

    /// Reserved positive PnL (u128, spec §2.1)
    pub reserved_pnl: u128,

    /// Signed fixed-point base quantity basis (i128, spec §2.1)
    pub position_basis_q: i128,

    /// Side multiplier snapshot at last explicit position attachment (u128)
    pub adl_a_basis: u128,

    /// K coefficient snapshot (i128)
    pub adl_k_snap: i128,

    /// Per-account funding snapshot at last attachment.
    pub f_snap: i128,

    /// Side epoch snapshot
    pub adl_epoch_snap: u64,

    /// Wrapper-owned matching-engine bindings (spec §2.1.1, non-normative).
    /// Opaque payload stored by the engine but never read for any
    /// spec-normative decision. Typical use: CPI routing by the wrapper's
    /// LP/matching-engine integration.
    pub matcher_program: [u8; 32],
    pub matcher_context: [u8; 32],

    /// Wrapper-owned owner pubkey (spec §2.1.1, non-normative).
    /// Authorization is a wrapper responsibility; the engine never reads
    /// `owner` for any spec-normative decision. `set_owner` is a defensive
    /// helper that preserves the "zero iff unclaimed" convention — it
    /// refuses to overwrite a nonzero owner and refuses to write zero.
    pub owner: [u8; 32],

    /// Fee credits
    pub fee_credits: I128,

    /// Per-account recurring-fee checkpoint (spec §2.1, §4.6.1).
    /// Anchors the slot at which this account's wrapper-owned recurring
    /// maintenance fee was last realized. On materialization, set to the
    /// materialization slot; on free_slot, reset to 0. Invariant:
    ///   market Live     → last_fee_slot_i <= current_slot
    ///   market Resolved → last_fee_slot_i <= resolved_slot
    pub last_fee_slot: u64,

    // ---- Two-bucket warmup reserve (spec §4.3) ----
    /// Scheduled reserve bucket, which matures linearly.
    pub sched_present: u8,
    pub sched_remaining_q: u128,
    pub sched_anchor_q: u128,
    pub sched_start_slot: u64,
    pub sched_horizon: u64,
    pub sched_release_q: u128,
    /// Pending reserve bucket (newest, does not mature while pending)
    pub pending_present: u8,
    pub pending_remaining_q: u128,
    pub pending_horizon: u64,
    pub pending_created_slot: u64,
}

impl Account {
    pub const KIND_USER: u8 = 0;
    pub const KIND_LP: u8 = 1;
    pub const EMPTY: Account = Account {
        capital: U128::ZERO,
        kind: Account::KIND_USER,
        pnl: 0,
        reserved_pnl: 0,
        position_basis_q: 0,
        adl_a_basis: ADL_ONE,
        adl_k_snap: 0,
        f_snap: 0,
        adl_epoch_snap: 0,
        matcher_program: [0; 32],
        matcher_context: [0; 32],
        owner: [0; 32],
        fee_credits: I128::ZERO,
        last_fee_slot: 0,
        sched_present: 0,
        sched_remaining_q: 0,
        sched_anchor_q: 0,
        sched_start_slot: 0,
        sched_horizon: 0,
        sched_release_q: 0,
        pending_present: 0,
        pending_remaining_q: 0,
        pending_horizon: 0,
        pending_created_slot: 0,
    };

    pub fn is_lp(&self) -> bool {
        self.kind == Self::KIND_LP
    }

    pub fn is_user(&self) -> bool {
        self.kind == Self::KIND_USER
    }
}

#[cfg(any(feature = "test", kani))]
fn empty_account() -> Account {
    Account::EMPTY
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
    pub maintenance_margin_bps: u64,
    pub initial_margin_bps: u64,
    pub trading_fee_bps: u64,
    pub max_accounts: u64,
    pub liquidation_fee_bps: u64,
    pub liquidation_fee_cap: U128,
    pub min_liquidation_abs: U128,
    // NOTE: `min_initial_deposit` was removed from the engine. The wrapper
    // is expected to enforce both a minimum deposit (anti-spam) and
    // recurring account fees (capital erosion over time) to keep the
    // materialized-account set bounded. The engine only enforces
    // `amount > 0` on materialization; any higher floor is wrapper policy.
    /// Absolute nonzero-position margin floors (spec §9.1)
    pub min_nonzero_mm_req: u128,
    pub min_nonzero_im_req: u128,
    /// Warmup horizon bounds (spec §6.1)
    pub h_min: u64,
    pub h_max: u64,
    /// Resolved settlement price deviation bound (spec §10.7)
    pub resolve_price_deviation_bps: u64,
    /// Max dt allowed in a single accrue_market_to call (spec §5.5 clause 6).
    /// Init-time invariant: ADL_ONE * MAX_ORACLE_PRICE *
    /// max_abs_funding_e9_per_slot * max_accrual_dt_slots <= i128::MAX
    /// ensures F_side_num cannot overflow in a single envelope-respecting call.
    pub max_accrual_dt_slots: u64,
    /// Max |funding_rate_e9_per_slot| allowed (spec §1.4).
    pub max_abs_funding_e9_per_slot: u64,
    /// Deployment-chosen cumulative funding lifetime floor (spec §1.4).
    ///
    /// Persisted `F_long_num` / `F_short_num` accumulate across calls and
    /// are stored as `i128`. A sequence of envelope-valid accruals can
    /// still drive them to the `i128` boundary over time. This parameter
    /// encodes the minimum number of slots the deployment guarantees F
    /// will stay within bounds at sustained `max_abs_funding_e9_per_slot`
    /// on both sides.
    ///
    /// Init-time invariant:
    ///   ADL_ONE * MAX_ORACLE_PRICE * max_abs_funding_e9_per_slot
    ///     * min_funding_lifetime_slots <= i128::MAX
    /// and `min_funding_lifetime_slots >= max_accrual_dt_slots`
    /// (cumulative bound must be at least as strong as per-call).
    ///
    /// Production deployments SHOULD pick a lifetime comfortably beyond
    /// any planned market horizon. With the tightened global rate ceiling
    /// MAX_ABS_FUNDING_E9_PER_SLOT = 10_000, ADL_ONE = 1e15, and
    /// MAX_ORACLE_PRICE = 1e12, the cumulative envelope
    ///   rate * lifetime <= i128::MAX / (ADL_ONE * MAX_ORACLE_PRICE) ≈ 1.7e11
    /// gives (at 400ms slots ≈ 7.89e7 slots/year):
    ///   rate <= 10_000 (global max) ⇒ lifetime ~1.7e7 slots  ≈ 2.6 months
    ///   rate <=  1_000              ⇒ lifetime ~1.7e8 slots  ≈ 2.15 years
    ///   rate <=    100              ⇒ lifetime ~1.7e9 slots  ≈ 21.5 years
    ///   rate <=     10              ⇒ lifetime ~1.7e10 slots ≈ 215  years
    /// These are sustained-worst-case lifetimes. At realistic operating
    /// rates (orders of magnitude below the ceiling), observed horizons
    /// are much longer. Tests MAY set this equal to `max_accrual_dt_slots`.
    ///
    /// Saturation at `max_abs_funding_e9_per_slot` is the worst case;
    /// realistic operating rates are orders of magnitude smaller, so the
    /// observed F-saturation horizon is typically far longer than this
    /// parameter guarantees.
    pub min_funding_lifetime_slots: u64,
    /// Per-market active-positions cap per side (spec §1.4).
    /// Invariant: max_active_positions_per_side <= max_accounts <= MAX_ACCOUNTS.
    pub max_active_positions_per_side: u64,
    /// Per-slot price-move cap in bps (spec §1.4, v12.19.53).
    ///
    /// Bounds the magnitude of `|oracle_price - P_last| / P_last` per
    /// accrual envelope: `accrue_market_to` rejects any call on a
    /// price-moving live-exposed market where
    /// `abs_delta_price * 10_000 > max_price_move_bps_per_slot * dt * P_last`.
    ///
    /// Init-time solvency-envelope invariant (spec §1.4): the exact bounded
    /// verifier must prove, for every account RiskNotional in
    /// `[1, MAX_ACCOUNT_NOTIONAL]`, that the configured maintenance
    /// requirement covers worst-case price movement, funding, and capped
    /// liquidation fee.
    pub max_price_move_bps_per_slot: u64,
}

/// Main risk engine state (spec §2.2)
#[repr(C)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RiskEngine {
    pub vault: U128,
    pub insurance_fund: InsuranceFund,
    pub params: RiskParams,
    pub current_slot: u64,

    /// Market mode (spec §2.2)
    pub market_mode: MarketMode,
    /// Resolved market state
    pub resolved_price: u64,
    pub resolved_slot: u64,
    /// Resolved terminal payout snapshot — locked after all positions zeroed.
    /// h_num/h_den frozen once, used for all terminal closes (order-invariant).
    pub resolved_payout_h_num: u128,
    pub resolved_payout_h_den: u128,
    pub resolved_payout_ready: u8, // 0 = not ready, 1 = snapshot locked
    /// Resolved terminal K deltas (spec §9.7 step 8).
    /// Stored separately from live K_side to avoid K headroom exhaustion during resolution.
    pub resolved_k_long_terminal_delta: i128,
    pub resolved_k_short_terminal_delta: i128,
    /// Live oracle price used for the live-sync leg of resolve_market
    pub resolved_live_price: u64,

    // O(1) aggregates (spec §2.2)
    pub c_tot: U128,
    pub pnl_pos_tot: u128,
    pub pnl_matured_pos_tot: u128,

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

    /// Certified phantom dust can clear residual OI; potential dust cannot.
    pub phantom_dust_certified_long_q: u128,
    pub phantom_dust_certified_short_q: u128,
    pub phantom_dust_potential_long_q: u128,
    pub phantom_dust_potential_short_q: u128,

    /// Materialized account count (spec §2.2)
    pub materialized_account_count: u64,

    /// Count of accounts with PNL < 0 (spec §4.7).
    pub neg_pnl_account_count: u64,

    /// Round-robin sweep cursor (spec §2.2, v12.19.53).
    /// Persistent cursor walked by `keeper_crank` Phase 2. Bounded by
    /// `0 <= rr_cursor_position < params.max_accounts`. Greedy Phase 2
    /// skips unused slots and touches up to the supplied account budget before
    /// stopping or wrapping at the deployment's physical slab size.
    pub rr_cursor_position: u64,
    /// Sweep generation counter (spec §2.2, v12.19.53).
    /// Incremented at most once per slot after a full wraparound of
    /// `rr_cursor_position`.
    /// Read-only from the wrapper perspective; can only advance by running
    /// `keeper_crank` through a complete cursor wrap.
    pub sweep_generation: u64,
    /// Cumulative price/funding stress since the current post-stress envelope
    /// started. Scaled bps-e9, saturating.
    pub stress_consumed_bps_e9_since_envelope: u128,
    /// Authenticated index advances still required before stress can clear.
    pub stress_envelope_remaining_indices: u64,
    /// Slot where the current stress envelope started. `NO_SLOT` means none.
    pub stress_envelope_start_slot: u64,
    /// Sweep generation at stress start. `NO_SLOT` means none.
    pub stress_envelope_start_generation: u64,
    /// Last slot where `sweep_generation` advanced. `NO_SLOT` means never.
    pub last_sweep_generation_advance_slot: u64,
    /// Persistent bankruptcy h-max source. Uses the same reconciliation
    /// envelope fields as threshold stress, but can be active with zero
    /// consumption when the lock came from a post-principal loss tail.
    pub bankruptcy_hmax_lock_active: bool,

    /// Last oracle price used in accrue_market_to (P_last, spec §5.5)
    pub last_oracle_price: u64,
    /// Last funding-sample price (fund_px_last, spec §5.5 step 11)
    pub fund_px_last: u64,
    /// Last slot used in accrue_market_to
    pub last_market_slot: u64,
    /// Cumulative funding numerator for long side.
    pub f_long_num: i128,
    /// Cumulative funding numerator for short side.
    pub f_short_num: i128,
    /// F snapshot at epoch start for long side.
    pub f_epoch_start_long_num: i128,
    /// F snapshot at epoch start for short side.
    pub f_epoch_start_short_num: i128,

    // Slab management
    pub used: [u64; BITMAP_WORDS],
    pub num_used_accounts: u16,
    pub free_head: u16,
    /// Forward pointer in the doubly-linked free list. Only meaningful when
    /// the slot is free. u16::MAX terminates the list.
    pub next_free: [u16; MAX_ACCOUNTS],
    /// Backward pointer: mirror of next_free. Enables O(1) removal at any
    /// position for arbitrary-slot materialization.
    pub prev_free: [u16; MAX_ACCOUNTS],
    pub accounts: [Account; MAX_ACCOUNTS],
}

struct RiskEngineNonAccountSnapshot {
    vault: U128,
    insurance_fund: InsuranceFund,
    params: RiskParams,
    current_slot: u64,
    market_mode: MarketMode,
    resolved_price: u64,
    resolved_slot: u64,
    resolved_payout_h_num: u128,
    resolved_payout_h_den: u128,
    resolved_payout_ready: u8,
    resolved_k_long_terminal_delta: i128,
    resolved_k_short_terminal_delta: i128,
    resolved_live_price: u64,
    c_tot: U128,
    pnl_pos_tot: u128,
    pnl_matured_pos_tot: u128,
    adl_mult_long: u128,
    adl_mult_short: u128,
    adl_coeff_long: i128,
    adl_coeff_short: i128,
    adl_epoch_long: u64,
    adl_epoch_short: u64,
    adl_epoch_start_k_long: i128,
    adl_epoch_start_k_short: i128,
    oi_eff_long_q: u128,
    oi_eff_short_q: u128,
    side_mode_long: SideMode,
    side_mode_short: SideMode,
    stored_pos_count_long: u64,
    stored_pos_count_short: u64,
    stale_account_count_long: u64,
    stale_account_count_short: u64,
    phantom_dust_certified_long_q: u128,
    phantom_dust_certified_short_q: u128,
    phantom_dust_potential_long_q: u128,
    phantom_dust_potential_short_q: u128,
    materialized_account_count: u64,
    neg_pnl_account_count: u64,
    rr_cursor_position: u64,
    sweep_generation: u64,
    stress_consumed_bps_e9_since_envelope: u128,
    stress_envelope_remaining_indices: u64,
    stress_envelope_start_slot: u64,
    stress_envelope_start_generation: u64,
    last_sweep_generation_advance_slot: u64,
    bankruptcy_hmax_lock_active: bool,
    last_oracle_price: u64,
    fund_px_last: u64,
    last_market_slot: u64,
    f_long_num: i128,
    f_short_num: i128,
    f_epoch_start_long_num: i128,
    f_epoch_start_short_num: i128,
    used: [u64; BITMAP_WORDS],
    num_used_accounts: u16,
    free_head: u16,
    next_free: [u16; MAX_ACCOUNTS],
    prev_free: [u16; MAX_ACCOUNTS],
}

struct RiskEngineRollbackSnapshot<const N: usize> {
    non_account: RiskEngineNonAccountSnapshot,
    account_count: usize,
    account_indices: [usize; N],
    accounts: [Account; N],
}

// ============================================================================
// Error Types
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RiskError {
    InsufficientBalance,
    Undercollateralized,
    Unauthorized,
    PnlNotWarmedUp,
    Overflow,
    AccountNotFound,
    SideBlocked,
    CorruptState,
}

pub type Result<T> = core::result::Result<T, RiskError>;

/// Result of force_close_resolved_not_atomic (spec §10.8).
/// Eliminates the Ok(0) ambiguity between "deferred" and "closed with zero payout."
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResolvedCloseResult {
    /// Phase 1 reconciled but terminal payout not yet ready.
    /// Account is still open. Re-call after all accounts reconciled.
    ProgressOnly,
    /// Account closed and freed. Payout is the returned capital.
    Closed(u128),
}

impl ResolvedCloseResult {
    pub fn closed(self) -> Option<u128> {
        match self {
            Self::Closed(cap) => Some(cap),
            Self::ProgressOnly => None,
        }
    }

    #[cfg(any(feature = "test", kani))]
    pub fn expect_closed(self, msg: &str) -> u128 {
        match self {
            Self::Closed(cap) => cap,
            Self::ProgressOnly => panic!("{}", msg),
        }
    }

    /// True if the account was deferred (still open).
    pub fn is_progress_only(self) -> bool {
        matches!(self, Self::ProgressOnly)
    }
}

/// Liquidation policy (spec §10.6)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiquidationPolicy {
    FullClose,
    ExactPartial(u128), // q_close_q
}

/// Outcome of a keeper crank operation
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CrankOutcome {
    pub num_liquidations: u32,
}

/// Pure Phase 2 cursor-scan outcome. The keeper path computes this before
/// mutating cursor/generation state, then performs the materialized touches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Phase2ScanOutcome {
    pub next_cursor: u64,
    pub inspected: u64,
    pub touched: u64,
    pub stress_counted_inspected: u64,
    pub wrapped: bool,
}

/// Stable keeper-crank request object. New crank policy fields should be added
/// here rather than appended to positional call sites.
pub struct KeeperCrankRequest<'a> {
    pub now_slot: u64,
    pub oracle_price: u64,
    pub ordered_candidates: &'a [(u16, Option<LiquidationPolicy>)],
    pub max_revalidations: u16,
    pub max_candidate_inspections: u16,
    pub funding_rate_e9: i128,
    pub admit_h_min: u64,
    pub admit_h_max: u64,
    pub admit_h_max_consumption_threshold_bps_opt: Option<u128>,
    pub rr_touch_limit: u64,
    pub rr_scan_limit: u64,
}

impl<'a> KeeperCrankRequest<'a> {
    pub fn full_scan(
        now_slot: u64,
        oracle_price: u64,
        ordered_candidates: &'a [(u16, Option<LiquidationPolicy>)],
        max_revalidations: u16,
        funding_rate_e9: i128,
        admit_h_min: u64,
        admit_h_max: u64,
        admit_h_max_consumption_threshold_bps_opt: Option<u128>,
        rr_touch_limit: u64,
    ) -> Self {
        Self {
            now_slot,
            oracle_price,
            ordered_candidates,
            max_revalidations,
            max_candidate_inspections: MAX_TOUCHED_PER_INSTRUCTION as u16,
            funding_rate_e9,
            admit_h_min,
            admit_h_max,
            admit_h_max_consumption_threshold_bps_opt,
            rr_touch_limit,
            rr_scan_limit: u64::MAX,
        }
    }
}

// ============================================================================
// Small Helpers
// ============================================================================

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
    fn snapshot_non_account(&self) -> RiskEngineNonAccountSnapshot {
        RiskEngineNonAccountSnapshot {
            vault: self.vault,
            insurance_fund: self.insurance_fund,
            params: self.params,
            current_slot: self.current_slot,
            market_mode: self.market_mode,
            resolved_price: self.resolved_price,
            resolved_slot: self.resolved_slot,
            resolved_payout_h_num: self.resolved_payout_h_num,
            resolved_payout_h_den: self.resolved_payout_h_den,
            resolved_payout_ready: self.resolved_payout_ready,
            resolved_k_long_terminal_delta: self.resolved_k_long_terminal_delta,
            resolved_k_short_terminal_delta: self.resolved_k_short_terminal_delta,
            resolved_live_price: self.resolved_live_price,
            c_tot: self.c_tot,
            pnl_pos_tot: self.pnl_pos_tot,
            pnl_matured_pos_tot: self.pnl_matured_pos_tot,
            adl_mult_long: self.adl_mult_long,
            adl_mult_short: self.adl_mult_short,
            adl_coeff_long: self.adl_coeff_long,
            adl_coeff_short: self.adl_coeff_short,
            adl_epoch_long: self.adl_epoch_long,
            adl_epoch_short: self.adl_epoch_short,
            adl_epoch_start_k_long: self.adl_epoch_start_k_long,
            adl_epoch_start_k_short: self.adl_epoch_start_k_short,
            oi_eff_long_q: self.oi_eff_long_q,
            oi_eff_short_q: self.oi_eff_short_q,
            side_mode_long: self.side_mode_long,
            side_mode_short: self.side_mode_short,
            stored_pos_count_long: self.stored_pos_count_long,
            stored_pos_count_short: self.stored_pos_count_short,
            stale_account_count_long: self.stale_account_count_long,
            stale_account_count_short: self.stale_account_count_short,
            phantom_dust_certified_long_q: self.phantom_dust_certified_long_q,
            phantom_dust_certified_short_q: self.phantom_dust_certified_short_q,
            phantom_dust_potential_long_q: self.phantom_dust_potential_long_q,
            phantom_dust_potential_short_q: self.phantom_dust_potential_short_q,
            materialized_account_count: self.materialized_account_count,
            neg_pnl_account_count: self.neg_pnl_account_count,
            rr_cursor_position: self.rr_cursor_position,
            sweep_generation: self.sweep_generation,
            stress_consumed_bps_e9_since_envelope: self.stress_consumed_bps_e9_since_envelope,
            stress_envelope_remaining_indices: self.stress_envelope_remaining_indices,
            stress_envelope_start_slot: self.stress_envelope_start_slot,
            stress_envelope_start_generation: self.stress_envelope_start_generation,
            last_sweep_generation_advance_slot: self.last_sweep_generation_advance_slot,
            bankruptcy_hmax_lock_active: self.bankruptcy_hmax_lock_active,
            last_oracle_price: self.last_oracle_price,
            fund_px_last: self.fund_px_last,
            last_market_slot: self.last_market_slot,
            f_long_num: self.f_long_num,
            f_short_num: self.f_short_num,
            f_epoch_start_long_num: self.f_epoch_start_long_num,
            f_epoch_start_short_num: self.f_epoch_start_short_num,
            used: self.used,
            num_used_accounts: self.num_used_accounts,
            free_head: self.free_head,
            next_free: self.next_free,
            prev_free: self.prev_free,
        }
    }

    fn restore_non_account(&mut self, snapshot: RiskEngineNonAccountSnapshot) {
        self.vault = snapshot.vault;
        self.insurance_fund = snapshot.insurance_fund;
        self.params = snapshot.params;
        self.current_slot = snapshot.current_slot;
        self.market_mode = snapshot.market_mode;
        self.resolved_price = snapshot.resolved_price;
        self.resolved_slot = snapshot.resolved_slot;
        self.resolved_payout_h_num = snapshot.resolved_payout_h_num;
        self.resolved_payout_h_den = snapshot.resolved_payout_h_den;
        self.resolved_payout_ready = snapshot.resolved_payout_ready;
        self.resolved_k_long_terminal_delta = snapshot.resolved_k_long_terminal_delta;
        self.resolved_k_short_terminal_delta = snapshot.resolved_k_short_terminal_delta;
        self.resolved_live_price = snapshot.resolved_live_price;
        self.c_tot = snapshot.c_tot;
        self.pnl_pos_tot = snapshot.pnl_pos_tot;
        self.pnl_matured_pos_tot = snapshot.pnl_matured_pos_tot;
        self.adl_mult_long = snapshot.adl_mult_long;
        self.adl_mult_short = snapshot.adl_mult_short;
        self.adl_coeff_long = snapshot.adl_coeff_long;
        self.adl_coeff_short = snapshot.adl_coeff_short;
        self.adl_epoch_long = snapshot.adl_epoch_long;
        self.adl_epoch_short = snapshot.adl_epoch_short;
        self.adl_epoch_start_k_long = snapshot.adl_epoch_start_k_long;
        self.adl_epoch_start_k_short = snapshot.adl_epoch_start_k_short;
        self.oi_eff_long_q = snapshot.oi_eff_long_q;
        self.oi_eff_short_q = snapshot.oi_eff_short_q;
        self.side_mode_long = snapshot.side_mode_long;
        self.side_mode_short = snapshot.side_mode_short;
        self.stored_pos_count_long = snapshot.stored_pos_count_long;
        self.stored_pos_count_short = snapshot.stored_pos_count_short;
        self.stale_account_count_long = snapshot.stale_account_count_long;
        self.stale_account_count_short = snapshot.stale_account_count_short;
        self.phantom_dust_certified_long_q = snapshot.phantom_dust_certified_long_q;
        self.phantom_dust_certified_short_q = snapshot.phantom_dust_certified_short_q;
        self.phantom_dust_potential_long_q = snapshot.phantom_dust_potential_long_q;
        self.phantom_dust_potential_short_q = snapshot.phantom_dust_potential_short_q;
        self.materialized_account_count = snapshot.materialized_account_count;
        self.neg_pnl_account_count = snapshot.neg_pnl_account_count;
        self.rr_cursor_position = snapshot.rr_cursor_position;
        self.sweep_generation = snapshot.sweep_generation;
        self.stress_consumed_bps_e9_since_envelope = snapshot.stress_consumed_bps_e9_since_envelope;
        self.stress_envelope_remaining_indices = snapshot.stress_envelope_remaining_indices;
        self.stress_envelope_start_slot = snapshot.stress_envelope_start_slot;
        self.stress_envelope_start_generation = snapshot.stress_envelope_start_generation;
        self.last_sweep_generation_advance_slot = snapshot.last_sweep_generation_advance_slot;
        self.bankruptcy_hmax_lock_active = snapshot.bankruptcy_hmax_lock_active;
        self.last_oracle_price = snapshot.last_oracle_price;
        self.fund_px_last = snapshot.fund_px_last;
        self.last_market_slot = snapshot.last_market_slot;
        self.f_long_num = snapshot.f_long_num;
        self.f_short_num = snapshot.f_short_num;
        self.f_epoch_start_long_num = snapshot.f_epoch_start_long_num;
        self.f_epoch_start_short_num = snapshot.f_epoch_start_short_num;
        self.used = snapshot.used;
        self.num_used_accounts = snapshot.num_used_accounts;
        self.free_head = snapshot.free_head;
        self.next_free = snapshot.next_free;
        self.prev_free = snapshot.prev_free;
    }

    fn rollback_snapshot_accounts<const N: usize>(
        &self,
        indices: [usize; N],
    ) -> RiskEngineRollbackSnapshot<N> {
        let mut account_indices = [usize::MAX; N];
        let mut accounts = [Account::EMPTY; N];
        let mut account_count = 0usize;
        for idx in indices {
            self.record_rollback_account(
                idx,
                &mut account_count,
                &mut account_indices,
                &mut accounts,
            );
        }
        RiskEngineRollbackSnapshot {
            non_account: self.snapshot_non_account(),
            account_count,
            account_indices,
            accounts,
        }
    }

    fn rollback_snapshot_keeper(
        &self,
        req: &KeeperCrankRequest<'_>,
    ) -> RiskEngineRollbackSnapshot<KEEPER_ROLLBACK_ACCOUNT_SLOTS> {
        let mut account_indices = [usize::MAX; KEEPER_ROLLBACK_ACCOUNT_SLOTS];
        let mut accounts = [Account::EMPTY; KEEPER_ROLLBACK_ACCOUNT_SLOTS];
        let mut account_count = 0usize;

        let max_candidate_inspections = core::cmp::min(
            MAX_TOUCHED_PER_INSTRUCTION as u16,
            req.max_candidate_inspections,
        );
        let mut inspected = 0u16;
        for &(candidate_idx, _) in req.ordered_candidates {
            if inspected >= max_candidate_inspections {
                break;
            }
            inspected = inspected.saturating_add(1);
            self.record_rollback_account(
                candidate_idx as usize,
                &mut account_count,
                &mut account_indices,
                &mut accounts,
            );
        }

        if req.rr_touch_limit != 0 && req.rr_scan_limit != 0 {
            let wrap_bound = self.params.max_accounts;
            if wrap_bound != 0 && self.rr_cursor_position < wrap_bound {
                let scan_cap = core::cmp::min(req.rr_scan_limit, wrap_bound);
                let touch_limit =
                    core::cmp::min(req.rr_touch_limit, MAX_TOUCHED_PER_INSTRUCTION as u64);
                let mut i = self.rr_cursor_position;
                let mut replayed_inspected = 0u64;
                let mut replayed_touched = 0u64;
                while replayed_inspected < scan_cap && replayed_touched < touch_limit {
                    if i >= wrap_bound {
                        break;
                    }
                    let idx = i as usize;
                    if self.is_used(idx) {
                        self.record_rollback_account(
                            idx,
                            &mut account_count,
                            &mut account_indices,
                            &mut accounts,
                        );
                        replayed_touched = replayed_touched.saturating_add(1);
                    }
                    i = i.saturating_add(1);
                    replayed_inspected = replayed_inspected.saturating_add(1);
                    if i == wrap_bound {
                        break;
                    }
                }
            }
        }

        RiskEngineRollbackSnapshot {
            non_account: self.snapshot_non_account(),
            account_count,
            account_indices,
            accounts,
        }
    }

    fn record_rollback_account<const N: usize>(
        &self,
        idx: usize,
        account_count: &mut usize,
        account_indices: &mut [usize; N],
        accounts: &mut [Account; N],
    ) {
        if idx >= MAX_ACCOUNTS || *account_count >= N {
            return;
        }
        let mut i = 0usize;
        while i < *account_count {
            if account_indices[i] == idx {
                return;
            }
            i += 1;
        }
        account_indices[*account_count] = idx;
        accounts[*account_count] = self.accounts[idx];
        *account_count += 1;
    }

    fn restore_rollback_snapshot<const N: usize>(
        &mut self,
        snapshot: RiskEngineRollbackSnapshot<N>,
    ) {
        self.restore_non_account(snapshot.non_account);
        let mut i = 0usize;
        while i < snapshot.account_count {
            self.accounts[snapshot.account_indices[i]] = snapshot.accounts[i];
            i += 1;
        }
    }

    #[cfg(all(
        not(kani),
        any(feature = "test", feature = "audit-scan", debug_assertions)
    ))]
    fn is_zero_bytes_32(bytes: &[u8; 32]) -> bool {
        (bytes[0]
            | bytes[1]
            | bytes[2]
            | bytes[3]
            | bytes[4]
            | bytes[5]
            | bytes[6]
            | bytes[7]
            | bytes[8]
            | bytes[9]
            | bytes[10]
            | bytes[11]
            | bytes[12]
            | bytes[13]
            | bytes[14]
            | bytes[15]
            | bytes[16]
            | bytes[17]
            | bytes[18]
            | bytes[19]
            | bytes[20]
            | bytes[21]
            | bytes[22]
            | bytes[23]
            | bytes[24]
            | bytes[25]
            | bytes[26]
            | bytes[27]
            | bytes[28]
            | bytes[29]
            | bytes[30]
            | bytes[31])
            == 0
    }

    #[cfg(not(kani))]
    fn ceil_div_u256_to_u128(n: U256, d: U256) -> Option<u128> {
        ceil_div_positive_checked(n, d).try_into_u128()
    }

    #[cfg(not(kani))]
    fn ceil_mul_div_u128(a: u128, b: u128, d: u128) -> Option<u128> {
        if d == 0 {
            return None;
        }
        a.checked_mul(b)?
            .checked_add(d.checked_sub(1)?)?
            .checked_div(d)
    }

    #[cfg(not(kani))]
    fn solvency_envelope_total_for_notional(
        params: &RiskParams,
        n: u128,
        loss_budget_num: u128,
        loss_budget_den: u128,
        price_budget_bps: u128,
    ) -> Option<u128> {
        let loss = match Self::ceil_mul_div_u128(n, loss_budget_num, loss_budget_den) {
            Some(v) => v,
            None => return None,
        };

        let worst_liq_multiplier = match 10_000u128.checked_add(price_budget_bps) {
            Some(v) => v,
            None => return None,
        };
        let worst_liq_notional = match Self::ceil_mul_div_u128(n, worst_liq_multiplier, 10_000u128)
        {
            Some(v) => v,
            None => return None,
        };

        let liq_fee_raw = match Self::ceil_mul_div_u128(
            worst_liq_notional,
            params.liquidation_fee_bps as u128,
            10_000u128,
        ) {
            Some(v) => v,
            None => return None,
        };
        let liq_fee = core::cmp::min(
            core::cmp::max(liq_fee_raw, params.min_liquidation_abs.get()),
            params.liquidation_fee_cap.get(),
        );

        loss.checked_add(liq_fee)
    }

    #[cfg(not(kani))]
    fn maintenance_requirement_for_notional(params: &RiskParams, n: u128) -> Option<u128> {
        let mm_prop = match n
            .checked_mul(params.maintenance_margin_bps as u128)
            .and_then(|v| v.checked_div(10_000u128))
        {
            Some(v) => v,
            None => return None,
        };
        Some(core::cmp::max(mm_prop, params.min_nonzero_mm_req))
    }

    #[cfg(not(kani))]
    fn solvency_envelope_holds_for_notional(
        params: &RiskParams,
        n: u128,
        loss_budget_num: u128,
        loss_budget_den: u128,
        price_budget_bps: u128,
    ) -> bool {
        let total = match Self::solvency_envelope_total_for_notional(
            params,
            n,
            loss_budget_num,
            loss_budget_den,
            price_budget_bps,
        ) {
            Some(v) => v,
            None => return false,
        };
        let mm_req = match Self::maintenance_requirement_for_notional(params, n) {
            Some(v) => v,
            None => return false,
        };
        total <= mm_req
    }

    #[cfg(not(kani))]
    fn solvency_envelope_interval_certifies(
        params: &RiskParams,
        lo: u128,
        hi: u128,
        loss_budget_num: u128,
        loss_budget_den: u128,
        price_budget_bps: u128,
    ) -> Option<bool> {
        let total_hi = Self::solvency_envelope_total_for_notional(
            params,
            hi,
            loss_budget_num,
            loss_budget_den,
            price_budget_bps,
        )?;
        let mm_lo = Self::maintenance_requirement_for_notional(params, lo)?;
        Some(total_hi <= mm_lo)
    }

    #[cfg(not(kani))]
    fn validate_solvency_envelope_range(
        params: &RiskParams,
        lo: u128,
        hi: u128,
        loss_budget_num: u128,
        loss_budget_den: u128,
        price_budget_bps: u128,
    ) -> Result<()> {
        if lo > hi {
            return Ok(());
        }

        const MAX_SOLVENCY_INTERVALS: usize = 96;
        const MAX_SOLVENCY_STEPS: usize = 4096;
        const EXACT_CHUNK: u128 = 64;

        let mut stack = [(0u128, 0u128); MAX_SOLVENCY_INTERVALS];
        let mut len = 1usize;
        let mut steps = 0usize;
        stack[0] = (lo, hi);

        while len != 0 {
            steps = steps.checked_add(1).ok_or(RiskError::Overflow)?;
            if steps > MAX_SOLVENCY_STEPS {
                return Err(RiskError::Overflow);
            }

            len -= 1;
            let (range_lo, range_hi) = stack[len];

            if Self::solvency_envelope_interval_certifies(
                params,
                range_lo,
                range_hi,
                loss_budget_num,
                loss_budget_den,
                price_budget_bps,
            ) == Some(true)
            {
                continue;
            }

            if range_hi == range_lo || range_hi - range_lo <= EXACT_CHUNK {
                let mut n = range_lo;
                loop {
                    if !Self::solvency_envelope_holds_for_notional(
                        params,
                        n,
                        loss_budget_num,
                        loss_budget_den,
                        price_budget_bps,
                    ) {
                        return Err(RiskError::Overflow);
                    }
                    if n == range_hi {
                        break;
                    }
                    n = n.checked_add(1).ok_or(RiskError::Overflow)?;
                }
                continue;
            }

            let mid = range_lo + (range_hi - range_lo) / 2;
            if len + 2 > MAX_SOLVENCY_INTERVALS {
                return Err(RiskError::Overflow);
            }
            stack[len] = (mid.checked_add(1).ok_or(RiskError::Overflow)?, range_hi);
            stack[len + 1] = (range_lo, mid);
            len += 2;
        }

        Ok(())
    }

    #[cfg(not(kani))]
    pub fn exact_solvency_envelope_ok(params: &RiskParams) -> bool {
        Self::validate_exact_solvency_envelope(params).is_ok()
    }

    #[cfg(kani)]
    pub fn exact_solvency_envelope_ok(_params: &RiskParams) -> bool {
        true
    }

    #[cfg(not(kani))]
    fn validate_exact_solvency_envelope(params: &RiskParams) -> Result<()> {
        let move_cap = U256::from_u128(params.max_price_move_bps_per_slot as u128);
        let dt = U256::from_u128(params.max_accrual_dt_slots as u128);
        let rate = U256::from_u128(params.max_abs_funding_e9_per_slot as u128);
        let ten_thousand = U256::from_u128(10_000u128);
        let funding_den = U256::from_u128(FUNDING_DEN);

        let price_budget_bps = move_cap
            .checked_mul(dt)
            .and_then(|v| v.try_into_u128())
            .ok_or(RiskError::Overflow)?;
        let funding_budget_num = rate
            .checked_mul(dt)
            .and_then(|v| v.checked_mul(ten_thousand))
            .ok_or(RiskError::Overflow)?;
        let loss_budget_num = U256::from_u128(price_budget_bps)
            .checked_mul(funding_den)
            .and_then(|v| v.checked_add(funding_budget_num))
            .ok_or(RiskError::Overflow)?;
        let loss_budget_den = ten_thousand
            .checked_mul(funding_den)
            .ok_or(RiskError::Overflow)?;

        let funding_budget_bps_ceil = Self::ceil_div_u256_to_u128(funding_budget_num, funding_den)
            .ok_or(RiskError::Overflow)?;
        let loss_budget_bps_ceil = price_budget_bps
            .checked_add(funding_budget_bps_ceil)
            .ok_or(RiskError::Overflow)?;
        let worst_liq_budget_bps_ceil = Self::ceil_div_u256_to_u128(
            U256::from_u128(10_000u128.saturating_add(price_budget_bps))
                .checked_mul(U256::from_u128(params.liquidation_fee_bps as u128))
                .ok_or(RiskError::Overflow)?,
            ten_thousand,
        )
        .ok_or(RiskError::Overflow)?;
        let linear_budget_bps = loss_budget_bps_ceil
            .checked_add(worst_liq_budget_bps_ceil)
            .ok_or(RiskError::Overflow)?;

        let exact_full_margin_loss_only = params.maintenance_margin_bps == 10_000
            && loss_budget_bps_ceil == 10_000
            && worst_liq_budget_bps_ceil == 0
            && params.min_liquidation_abs.get() == 0;
        if exact_full_margin_loss_only {
            return Ok(());
        }

        let loss_budget_num = loss_budget_num.try_into_u128().ok_or(RiskError::Overflow)?;
        let loss_budget_den = loss_budget_den.try_into_u128().ok_or(RiskError::Overflow)?;

        // Normative domain: account RiskNotional is capped at MAX_ACCOUNT_NOTIONAL.
        let domain_max = MAX_ACCOUNT_NOTIONAL;
        if params.maintenance_margin_bps == 0 {
            // With no proportional term, the absolute floor is the whole
            // maintenance requirement; the monotone worst case is domain max.
            if Self::solvency_envelope_holds_for_notional(
                params,
                domain_max,
                loss_budget_num,
                loss_budget_den,
                price_budget_bps,
            ) {
                return Ok(());
            }
            return Err(RiskError::Overflow);
        }

        // Floor-region proof. While proportional maintenance is below the
        // configured minimum, loss+fee is monotone in risk notional, so the
        // largest floor-covered notional inside the normative domain is the
        // only point that must be checked.
        let floor_region_max = U256::from_u128(
            params
                .min_nonzero_mm_req
                .checked_add(1)
                .ok_or(RiskError::Overflow)?,
        )
        .checked_mul(ten_thousand)
        .and_then(|v| v.checked_sub(U256::ONE))
        .and_then(|v| v.checked_div(U256::from_u128(params.maintenance_margin_bps as u128)))
        .and_then(|v| v.try_into_u128())
        .ok_or(RiskError::Overflow)?;
        let floor_region_end = core::cmp::min(floor_region_max, domain_max);
        if floor_region_end != 0
            && !Self::solvency_envelope_holds_for_notional(
                params,
                floor_region_end,
                loss_budget_num,
                loss_budget_den,
                price_budget_bps,
            )
        {
            return Err(RiskError::Overflow);
        }
        if floor_region_max >= domain_max {
            return Ok(());
        }

        let exact_start = floor_region_end.checked_add(1).ok_or(RiskError::Overflow)?;

        if linear_budget_bps < params.maintenance_margin_bps as u128 {
            // Fast conservative proof: treating liquidation fees as uncapped
            // is stronger than the spec, and gives a small exact tail for
            // ordinary parameter sets.
            let slope_gap = (params.maintenance_margin_bps as u128) - linear_budget_bps;
            let rounding_slack = 3u128;
            let tail_for_linear = ceil_div_positive_checked(
                U256::from_u128(rounding_slack * 10_000),
                U256::from_u128(slope_gap),
            )
            .try_into_u128()
            .ok_or(RiskError::Overflow)?;

            let loss_gap = (params.maintenance_margin_bps as u128)
                .checked_sub(loss_budget_bps_ceil)
                .ok_or(RiskError::Overflow)?;
            let floor_fee_slack = params
                .min_liquidation_abs
                .get()
                .checked_add(2)
                .ok_or(RiskError::Overflow)?;
            let tail_for_fee_floor = ceil_div_positive_checked(
                U256::from_u128(floor_fee_slack)
                    .checked_mul(ten_thousand)
                    .ok_or(RiskError::Overflow)?,
                U256::from_u128(loss_gap),
            )
            .try_into_u128()
            .ok_or(RiskError::Overflow)?;

            let exact_tail = core::cmp::max(tail_for_linear, tail_for_fee_floor);
            if exact_tail <= exact_start {
                return Ok(());
            }

            let exact_end = core::cmp::min(exact_tail.saturating_sub(1), domain_max);
            return Self::validate_solvency_envelope_range(
                params,
                exact_start,
                exact_end,
                loss_budget_num,
                loss_budget_den,
                price_budget_bps,
            );
        }

        if loss_budget_bps_ceil >= params.maintenance_margin_bps as u128 {
            return Self::validate_solvency_envelope_range(
                params,
                exact_start,
                domain_max,
                loss_budget_num,
                loss_budget_den,
                price_budget_bps,
            );
        }

        // Capped-fee proof: when uncapped liquidation fee slope would exceed
        // maintenance, the exact validator covers the finite prefix and the
        // tail proof uses liquidation_fee_cap as a bounded additive term.
        let slope_gap = (params.maintenance_margin_bps as u128) - loss_budget_bps_ceil;
        let rounding_slack = 3u128;
        let capped_fee_slack = params
            .liquidation_fee_cap
            .get()
            .checked_add(rounding_slack)
            .ok_or(RiskError::Overflow)?;
        let exact_tail = ceil_div_positive_checked(
            U256::from_u128(capped_fee_slack)
                .checked_mul(ten_thousand)
                .ok_or(RiskError::Overflow)?,
            U256::from_u128(slope_gap),
        )
        .try_into_u128()
        .ok_or(RiskError::Overflow)?;

        if exact_tail <= exact_start {
            return Ok(());
        }

        let exact_end = core::cmp::min(exact_tail.saturating_sub(1), domain_max);
        Self::validate_solvency_envelope_range(
            params,
            exact_start,
            exact_end,
            loss_budget_num,
            loss_budget_den,
            price_budget_bps,
        )
    }

    #[cfg(kani)]
    fn validate_exact_solvency_envelope(_params: &RiskParams) -> Result<()> {
        Ok(())
    }

    fn validate_params_fast_shape(params: &RiskParams) -> Result<()> {
        if params.max_accounts == 0 || (params.max_accounts as usize) > MAX_ACCOUNTS {
            return Err(RiskError::Overflow);
        }
        if params.max_active_positions_per_side == 0
            || params.max_active_positions_per_side > params.max_accounts
        {
            return Err(RiskError::Overflow);
        }
        if params.maintenance_margin_bps > params.initial_margin_bps
            || params.initial_margin_bps > MAX_MARGIN_BPS
            || params.trading_fee_bps > MAX_MARGIN_BPS
            || params.liquidation_fee_bps > MAX_MARGIN_BPS
        {
            return Err(RiskError::Overflow);
        }
        if params.min_nonzero_mm_req == 0 || params.min_nonzero_mm_req >= params.min_nonzero_im_req
        {
            return Err(RiskError::Overflow);
        }
        if params.min_liquidation_abs.get() > params.liquidation_fee_cap.get()
            || params.liquidation_fee_cap.get() > MAX_PROTOCOL_FEE_ABS
        {
            return Err(RiskError::Overflow);
        }
        if params.h_min > params.h_max || params.h_max == 0 {
            return Err(RiskError::Overflow);
        }
        if params.resolve_price_deviation_bps > MAX_RESOLVE_PRICE_DEVIATION_BPS {
            return Err(RiskError::Overflow);
        }
        if params.max_accrual_dt_slots == 0
            || (params.max_abs_funding_e9_per_slot as i128) > MAX_ABS_FUNDING_E9_PER_SLOT
            || params.min_funding_lifetime_slots < params.max_accrual_dt_slots
            || params.max_price_move_bps_per_slot == 0
            || params.max_price_move_bps_per_slot > MAX_MARGIN_BPS
        {
            return Err(RiskError::Overflow);
        }

        let adl = U256::from_u128(ADL_ONE);
        let px = U256::from_u128(MAX_ORACLE_PRICE as u128);
        let rate = U256::from_u128(params.max_abs_funding_e9_per_slot as u128);
        let dt = U256::from_u128(params.max_accrual_dt_slots as u128);
        let i128_max = U256::from_u128(i128::MAX as u128);
        let per_call_ok = adl
            .checked_mul(px)
            .and_then(|v| v.checked_mul(rate))
            .and_then(|v| v.checked_mul(dt))
            .map(|v| v <= i128_max)
            .unwrap_or(false);
        if !per_call_ok {
            return Err(RiskError::Overflow);
        }

        let life = U256::from_u128(params.min_funding_lifetime_slots as u128);
        let lifetime_ok = adl
            .checked_mul(px)
            .and_then(|v| v.checked_mul(rate))
            .and_then(|v| v.checked_mul(life))
            .map(|v| v <= i128_max)
            .unwrap_or(false);
        if !lifetime_ok {
            return Err(RiskError::Overflow);
        }

        Ok(())
    }

    pub fn try_validate_params(params: &RiskParams) -> Result<()> {
        Self::validate_params_fast_shape(params)?;
        Self::validate_exact_solvency_envelope(params)?;
        Ok(())
    }

    fn validate_params(params: &RiskParams) {
        Self::try_validate_params(params).expect("invalid RiskParams")
    }

    /// Create a new risk engine for testing. Initializes with
    /// init_oracle_price = 1 (spec §2.7 compliant).
    #[cfg(any(feature = "test", kani))]
    pub fn new(params: RiskParams) -> Self {
        Self::new_with_market(params, 0, 1)
    }

    /// Create a new risk engine with explicit market initialization (spec §2.7).
    /// Requires `0 < init_oracle_price <= MAX_ORACLE_PRICE` per spec §1.2.
    ///
    /// Test/kani only. Returns Self by value, which on SBF would require
    /// materializing ~MAX_ACCOUNTS * sizeof(Account) bytes on the stack
    /// (>>4KB limit). Production callers MUST use `init_in_place` on
    /// pre-allocated zero-initialized memory (SystemProgram.createAccount).
    #[cfg(any(feature = "test", kani))]
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
            market_mode: MarketMode::Live,
            resolved_price: 0,
            resolved_slot: 0,
            resolved_payout_h_num: 0,
            resolved_payout_h_den: 0,
            resolved_payout_ready: 0,
            resolved_k_long_terminal_delta: 0,
            resolved_k_short_terminal_delta: 0,
            resolved_live_price: 0,
            c_tot: U128::ZERO,
            pnl_pos_tot: 0u128,
            pnl_matured_pos_tot: 0u128,
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
            phantom_dust_certified_long_q: 0u128,
            phantom_dust_certified_short_q: 0u128,
            phantom_dust_potential_long_q: 0u128,
            phantom_dust_potential_short_q: 0u128,
            materialized_account_count: 0,
            neg_pnl_account_count: 0,
            rr_cursor_position: 0,
            sweep_generation: 0,
            stress_consumed_bps_e9_since_envelope: 0,
            stress_envelope_remaining_indices: 0,
            stress_envelope_start_slot: NO_SLOT,
            stress_envelope_start_generation: NO_SLOT,
            last_sweep_generation_advance_slot: NO_SLOT,
            bankruptcy_hmax_lock_active: false,
            last_oracle_price: init_oracle_price,
            fund_px_last: init_oracle_price,
            last_market_slot: init_slot,
            f_long_num: 0,
            f_short_num: 0,
            f_epoch_start_long_num: 0,
            f_epoch_start_short_num: 0,
            used: [0; BITMAP_WORDS],
            num_used_accounts: 0,
            free_head: 0,
            next_free: [0; MAX_ACCOUNTS],
            prev_free: [0; MAX_ACCOUNTS],
            accounts: [empty_account(); MAX_ACCOUNTS],
        };

        // Build the doubly-linked free list 0 → 1 → ... → N-1 → NIL.
        engine.prev_free[0] = u16::MAX; // head has no prev
        for i in 0..MAX_ACCOUNTS - 1 {
            engine.next_free[i] = (i + 1) as u16;
            engine.prev_free[i + 1] = i as u16;
        }
        engine.next_free[MAX_ACCOUNTS - 1] = u16::MAX;

        engine
    }

    /// Initialize in place (for Solana BPF zero-copy, spec §2.7).
    ///
    /// **Safety contract:** the underlying memory for `&mut RiskEngine` MUST
    /// be either zero-initialized (as SystemProgram.createAccount on Solana
    /// guarantees) or come from a valid RiskEngine. The engine
    /// contains `repr(u8)` enum fields (`MarketMode`, `SideMode`) whose
    /// valid discriminants are 0..=1 and 0..=2 respectively. Zero-initialized
    /// memory is a valid discriminant for `MarketMode::Live` (0) and
    /// `SideMode::Normal` (0) by construction.
    ///
    /// Callers that need to initialize arbitrary non-zero bytes must perform
    /// pointer-level enum initialization via `MaybeUninit` or `ptr::write`
    /// BEFORE forming the `&mut RiskEngine` reference — constructing the
    /// reference over invalid enum discriminants is UB. This engine does
    /// not ship a raw-pointer init shim; production boot paths use
    /// zero-initialized SystemProgram accounts.
    pub fn init_in_place(
        &mut self,
        params: RiskParams,
        init_slot: u64,
        init_oracle_price: u64,
    ) -> Result<()> {
        Self::try_validate_params(&params)?;
        if init_oracle_price == 0 || init_oracle_price > MAX_ORACLE_PRICE {
            return Err(RiskError::Overflow);
        }
        self.vault = U128::ZERO;
        self.insurance_fund = InsuranceFund {
            balance: U128::ZERO,
        };
        self.params = params;
        self.current_slot = init_slot;
        self.market_mode = MarketMode::Live;
        self.resolved_price = 0;
        self.resolved_slot = 0;
        self.resolved_payout_h_num = 0;
        self.resolved_payout_h_den = 0;
        self.resolved_payout_ready = 0;
        self.resolved_k_long_terminal_delta = 0;
        self.resolved_k_short_terminal_delta = 0;
        self.resolved_live_price = 0;
        self.c_tot = U128::ZERO;
        self.pnl_pos_tot = 0;
        self.pnl_matured_pos_tot = 0;
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
        self.phantom_dust_certified_long_q = 0;
        self.phantom_dust_certified_short_q = 0;
        self.phantom_dust_potential_long_q = 0;
        self.phantom_dust_potential_short_q = 0;
        self.materialized_account_count = 0;
        self.neg_pnl_account_count = 0;
        self.rr_cursor_position = 0;
        self.sweep_generation = 0;
        self.stress_consumed_bps_e9_since_envelope = 0;
        self.stress_envelope_remaining_indices = 0;
        self.stress_envelope_start_slot = NO_SLOT;
        self.stress_envelope_start_generation = NO_SLOT;
        self.last_sweep_generation_advance_slot = NO_SLOT;
        self.bankruptcy_hmax_lock_active = false;
        self.last_oracle_price = init_oracle_price;
        self.fund_px_last = init_oracle_price;
        self.last_market_slot = init_slot;
        self.f_long_num = 0;
        self.f_short_num = 0;
        self.f_epoch_start_long_num = 0;
        self.f_epoch_start_short_num = 0;
        self.used = [0; BITMAP_WORDS];
        self.num_used_accounts = 0;
        self.free_head = 0;
        // Fully canonicalize every account in-place without constructing a
        // large temporary Account on the stack.
        for i in 0..MAX_ACCOUNTS {
            let a = &mut self.accounts[i];
            a.kind = Account::KIND_USER;
            a.capital = U128::ZERO;
            a.pnl = 0;
            a.reserved_pnl = 0;
            a.position_basis_q = 0;
            a.adl_a_basis = ADL_ONE;
            a.adl_k_snap = 0;
            a.f_snap = 0;
            a.adl_epoch_snap = 0;
            a.matcher_program = [0; 32];
            a.matcher_context = [0; 32];
            a.owner = [0; 32];
            a.fee_credits = I128::ZERO;
            a.last_fee_slot = 0;
            a.sched_present = 0;
            a.sched_remaining_q = 0;
            a.sched_anchor_q = 0;
            a.sched_start_slot = 0;
            a.sched_horizon = 0;
            a.sched_release_q = 0;
            a.pending_present = 0;
            a.pending_remaining_q = 0;
            a.pending_horizon = 0;
            a.pending_created_slot = 0;
        }
        self.prev_free[0] = u16::MAX;
        for i in 0..MAX_ACCOUNTS - 1 {
            self.next_free[i] = (i + 1) as u16;
            self.prev_free[i + 1] = i as u16;
        }
        self.next_free[MAX_ACCOUNTS - 1] = u16::MAX;
        Ok(())
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

    #[cfg(any(feature = "test", feature = "stress", kani))]
    pub fn for_each_used<F: FnMut(usize, &Account)>(&self, mut f: F) {
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

    test_visible! {
    fn free_slot(&mut self, idx: u16) -> Result<()> {
        let i = idx as usize;
        if i >= MAX_ACCOUNTS || idx as u64 >= self.params.max_accounts {
            return Err(RiskError::AccountNotFound);
        }
        if !self.is_used(i) { return Err(RiskError::CorruptState); }
        if self.accounts[i].pnl != 0 { return Err(RiskError::CorruptState); }
        if self.accounts[i].reserved_pnl != 0 { return Err(RiskError::CorruptState); }
        if self.accounts[i].position_basis_q != 0 { return Err(RiskError::CorruptState); }
        if self.accounts[i].sched_present != 0 || self.accounts[i].pending_present != 0 {
            return Err(RiskError::CorruptState);
        }
        if !self.accounts[i].capital.is_zero() {
            return Err(RiskError::CorruptState);
        }
        self.validate_fee_credits_shape(i)?;
        // The current free-list head must be a genuine free head before
        // this slot is prepended.
        if self.free_head != u16::MAX {
            let h = self.free_head as usize;
            if h >= MAX_ACCOUNTS {
                return Err(RiskError::CorruptState);
            }
            if self.is_used(h) {
                return Err(RiskError::CorruptState);
            }
            if self.prev_free[h] != u16::MAX {
                return Err(RiskError::CorruptState);
            }
        }
        let a = &mut self.accounts[i];
        a.capital = U128::ZERO;
        a.kind = Account::KIND_USER;
        a.pnl = 0;
        a.reserved_pnl = 0;
        a.position_basis_q = 0;
        a.adl_a_basis = ADL_ONE;
        a.adl_k_snap = 0;
        a.f_snap = 0;
        a.adl_epoch_snap = 0;
        a.matcher_program = [0; 32];
        a.matcher_context = [0; 32];
        a.owner = [0; 32];
        a.fee_credits = I128::ZERO;
        a.last_fee_slot = 0;
        a.sched_present = 0;
        a.sched_remaining_q = 0;
        a.sched_anchor_q = 0;
        a.sched_start_slot = 0;
        a.sched_horizon = 0;
        a.sched_release_q = 0;
        a.pending_present = 0;
        a.pending_remaining_q = 0;
        a.pending_horizon = 0;
        a.pending_created_slot = 0;
        self.clear_used(i);
        // Push to head of doubly-linked free list.
        self.next_free[i] = self.free_head;
        self.prev_free[i] = u16::MAX;
        if self.free_head != u16::MAX {
            self.prev_free[self.free_head as usize] = idx;
        }
        self.free_head = idx;
        self.num_used_accounts = self.num_used_accounts.checked_sub(1)
            .ok_or(RiskError::CorruptState)?;
        self.materialized_account_count = self.materialized_account_count.checked_sub(1)
            .ok_or(RiskError::CorruptState)?;
        Ok(())
    }
    }

    /// materialize_account(i, slot_anchor) — spec §2.5.
    /// Materializes a missing account at a specific slot index.
    /// The slot must not be currently in use.
    test_visible! {
    fn materialize_at(&mut self, idx: u16, slot_anchor: u64) -> Result<()> {
        if idx as usize >= MAX_ACCOUNTS {
            return Err(RiskError::AccountNotFound);
        }
        // Spec §1.4: active market indices are [0, cfg_max_accounts). A
        // wrapper/scanner that enumerates only that range MUST NOT miss a
        // materialized account. The count bound below is not sufficient
        // on its own: with headroom in num_used_accounts, picking any
        // idx in [cfg_max_accounts, MAX_ACCOUNTS) would silently create
        // a live account outside the configured market range.
        if (idx as u64) >= self.params.max_accounts {
            return Err(RiskError::AccountNotFound);
        }

        let used_count = self.num_used_accounts as u64;
        if used_count >= self.params.max_accounts {
            return Err(RiskError::Overflow);
        }

        // Enforce materialized_account_count bound (spec §10.0).
        // Bound is params.max_accounts (the deployment's configured slab
        // capacity) — same value used for the free-list check above.
        self.materialized_account_count = self.materialized_account_count
            .checked_add(1).ok_or(RiskError::Overflow)?;
        if self.materialized_account_count > self.params.max_accounts {
            self.materialized_account_count -= 1;
            return Err(RiskError::Overflow);
        }

        // O(1) unlink from doubly-linked free list. If idx is not actually
        // free (no prev/next pointers in a consistent free-list state AND
        // bitmap says used), the pre-check above via !is_used in callers
        // should have already prevented this path. We require idx to be
        // marked unused (i.e., currently in the free list).
        if self.is_used(idx as usize) {
            self.materialized_account_count -= 1;
            return Err(RiskError::CorruptState);
        }
        let i = idx as usize;
        let next = self.next_free[i];
        let prev = self.prev_free[i];
        // Freelist-link consistency. Three layers of defense:
        //   (a) bounds check — prev/next must be either u16::MAX (list
        //       terminator) or a valid slot index < MAX_ACCOUNTS. Without
        //       this, a corrupted pointer would panic at the array index
        //       below, violating the deterministic-conservative-failure
        //       rule. Must come first since (b) and (c) index the arrays.
        //   (b) local back-pointer agreement — prev/next's reciprocal
        //       pointer must point to idx;
        //   (c) neighbor-used check — a truly-free neighbor is marked
        //       unused in the bitmap. If a corrupt neighbor pointer
        //       lands on an allocated slot, reject.
        if prev != u16::MAX && (prev as usize) >= MAX_ACCOUNTS {
            self.materialized_account_count -= 1;
            return Err(RiskError::CorruptState);
        }
        if next != u16::MAX && (next as usize) >= MAX_ACCOUNTS {
            self.materialized_account_count -= 1;
            return Err(RiskError::CorruptState);
        }
        if prev == u16::MAX {
            if self.free_head != idx {
                self.materialized_account_count -= 1;
                return Err(RiskError::CorruptState);
            }
        } else {
            if self.next_free[prev as usize] != idx {
                self.materialized_account_count -= 1;
                return Err(RiskError::CorruptState);
            }
            if self.is_used(prev as usize) {
                self.materialized_account_count -= 1;
                return Err(RiskError::CorruptState);
            }
        }
        if next != u16::MAX {
            if self.prev_free[next as usize] != idx {
                self.materialized_account_count -= 1;
                return Err(RiskError::CorruptState);
            }
            if self.is_used(next as usize) {
                self.materialized_account_count -= 1;
                return Err(RiskError::CorruptState);
            }
        }
        // Links verified — perform the unlink.
        if prev == u16::MAX {
            self.free_head = next;
        } else {
            self.next_free[prev as usize] = next;
        }
        if next != u16::MAX {
            self.prev_free[next as usize] = prev;
        }
        // Clear idx's freelist pointers now that it's allocated. Prevents
        // stale values from later masquerading as valid free-list state
        // if this slot is corrupted while in use.
        self.next_free[i] = u16::MAX;
        self.prev_free[i] = u16::MAX;

        self.set_used(idx as usize);
        self.num_used_accounts = self.num_used_accounts.checked_add(1)
            .expect("num_used_accounts overflow — slot leak corruption");

        // Initialize per spec §2.5 — field-by-field to avoid constructing
        // a ~4KB temporary Account on the stack (SBF stack limit is 4KB).
        {
            let a = &mut self.accounts[idx as usize];
            a.kind = Account::KIND_USER;
            a.capital = U128::ZERO;
            a.pnl = 0i128;
            a.reserved_pnl = 0u128;
            a.position_basis_q = 0i128;
            a.adl_a_basis = ADL_ONE;
            a.adl_k_snap = 0i128;
            a.f_snap = 0i128;
            a.adl_epoch_snap = 0;
            a.matcher_program = [0; 32];
            a.matcher_context = [0; 32];
            a.owner = [0; 32];
            a.fee_credits = I128::ZERO;
            // Anchor recurring-fee checkpoint at the materialization slot so
            // accounts are not charged for earlier time.
            a.last_fee_slot = slot_anchor;
            a.sched_present = 0;
            a.sched_remaining_q = 0;
            a.sched_anchor_q = 0;
            a.sched_start_slot = 0;
            a.sched_horizon = 0;
            a.sched_release_q = 0;
            a.pending_present = 0;
            a.pending_remaining_q = 0;
            a.pending_horizon = 0;
            a.pending_created_slot = 0;
        }

        Ok(())
    }
    }

    // ========================================================================
    // O(1) Aggregate Helpers (spec §4)
    // ========================================================================

    /// admit_fresh_reserve_h_lock (spec §4.7): decide effective horizon for fresh reserve.
    /// Returns admit_h_min if instant release preserves h=1, admit_h_max otherwise.
    /// Sticky: once an account gets h_max in this instruction, all later increments also get h_max.
    ///
    /// Internal helper. Not part of the public engine surface — callers should
    /// go through set_pnl_with_reserve with ReserveMode::UseAdmissionPair.
    test_visible! {
    fn admit_fresh_reserve_h_lock(
        &self, idx: usize, fresh_positive_pnl: u128,
        ctx: &mut InstructionContext, admit_h_min: u64, admit_h_max: u64,
    ) -> Result<u64> {
        // Step 1: sticky check (spec §4.7 step 1).
        if ctx.is_h_max_sticky(idx as u16) { return Ok(admit_h_max); }

        let admitted_h_eff = if self.stress_gate_active(ctx) {
            admit_h_max
        } else {
            self.admission_residual_lane(fresh_positive_pnl, admit_h_min, admit_h_max)?
        };

        // Step 4: mark sticky if admit_h_max. mark_h_max_sticky returns false
        // on capacity exhaustion; propagate as failure rather than silently
        // skipping the sticky.
        if admitted_h_eff == admit_h_max {
            if !ctx.mark_h_max_sticky(idx as u16) {
                return Err(RiskError::Overflow);
            }
        }
        Ok(admitted_h_eff)
    }
    }

    /// Post-impact residual-scarcity admission lane (spec §4.7 step 3).
    /// Factored out so the consumption-threshold gate (step 2) can either
    /// bypass it (returning admit_h_max unconditionally) or delegate to it.
    fn admission_residual_lane(
        &self,
        _fresh_positive_pnl: u128,
        admit_h_min: u64,
        admit_h_max: u64,
    ) -> Result<u64> {
        let senior = self
            .c_tot
            .get()
            .checked_add(self.insurance_fund.balance.get())
            .ok_or(RiskError::Overflow)?;
        let residual = self
            .vault
            .get()
            .checked_sub(senior)
            .ok_or(RiskError::CorruptState)?;
        Ok(if residual > 0 {
            admit_h_min
        } else {
            admit_h_max
        })
    }

    /// admit_outstanding_reserve_on_touch (spec §4.9): accelerate existing reserve if h=1 holds.
    ///
    /// Internal helper. Not part of the public engine surface — called by
    /// touch_account_live_local as part of the live-touch pipeline.
    test_visible! {
    fn admit_outstanding_reserve_on_touch(
        &mut self,
        idx: usize,
        ctx: &mut InstructionContext,
    ) -> Result<()> {
        if self.market_mode != MarketMode::Live { return Ok(()); }

        // Validate reserve integrity before any arithmetic or mutation.
        self.validate_reserve_shape(idx)?;

        // Compute all deltas before mutation and use checked arithmetic.
        let a = &self.accounts[idx];
        let sched_r = if a.sched_present != 0 { a.sched_remaining_q } else { 0 };
        let pend_r = if a.pending_present != 0 { a.pending_remaining_q } else { 0 };
        let reserve_total = sched_r.checked_add(pend_r).ok_or(RiskError::CorruptState)?;
        if reserve_total == 0 { return Ok(()); }
        if ctx.admit_h_min_shared != 0 {
            return Ok(());
        }
        if self.stress_gate_active(ctx) {
            return Ok(());
        }

        let senior = self.c_tot.get()
            .checked_add(self.insurance_fund.balance.get())
            .ok_or(RiskError::Overflow)?;
        let residual = self.vault.get()
            .checked_sub(senior)
            .ok_or(RiskError::CorruptState)?;
        let new_matured = self.pnl_matured_pos_tot
            .checked_add(reserve_total)
            .ok_or(RiskError::Overflow)?;

        if residual == 0 {
            // Does not admit — no mutation.
            return Ok(());
        }

        // Pre-validate the global invariant BEFORE any mutation.
        if new_matured > self.pnl_pos_tot {
            return Err(RiskError::CorruptState);
        }

        // Phase 2: all checks passed — commit.
        ctx.mark_positive_pnl_usability();
        self.pnl_matured_pos_tot = new_matured;
        let a = &mut self.accounts[idx];
        a.sched_present = 0;
        a.sched_remaining_q = 0;
        a.sched_anchor_q = 0;
        a.sched_start_slot = 0;
        a.sched_horizon = 0;
        a.sched_release_q = 0;
        a.pending_present = 0;
        a.pending_remaining_q = 0;
        a.pending_horizon = 0;
        a.pending_created_slot = 0;
        a.reserved_pnl = 0;
        Ok(())
    }
    }

    /// set_pnl: thin wrapper routing through set_pnl_with_reserve(ImmediateRelease).
    /// All PnL mutations go through one canonical path. ImmediateRelease routes
    /// positive increases directly to matured (no reserve queue), and decreases
    /// go through apply_reserve_loss_newest_first.
    test_visible! {
    fn set_pnl(&mut self, idx: usize, new_pnl: i128) -> Result<()> {
        self.set_pnl_with_reserve(idx, new_pnl, ReserveMode::ImmediateReleaseResolvedOnly, None)
    }
    }

    /// set_pnl with reserve_mode (spec §4.5).
    /// Canonical PNL mutation that routes positive increases through the cohort queue.
    test_visible! {
    fn set_pnl_with_reserve(&mut self, idx: usize, new_pnl: i128, reserve_mode: ReserveMode, ctx: Option<&mut InstructionContext>) -> Result<()> {
        if new_pnl == i128::MIN { return Err(RiskError::Overflow); }

        let old = self.accounts[idx].pnl;
        let old_pos = i128_clamp_pos(old);
        // Entry invariant: R_i <= max(PNL_i, 0) (spec §2.1). Reject before any mutation.
        if self.accounts[idx].reserved_pnl > old_pos {
            return Err(RiskError::CorruptState);
        }
        // Validate reserve shape without retaining the computed "released"
        // Bind the caller amount to currently released positive PnL.
        if self.market_mode == MarketMode::Live {
            old_pos.checked_sub(self.accounts[idx].reserved_pnl).ok_or(RiskError::CorruptState)?;
        } else if self.accounts[idx].reserved_pnl != 0 {
            return Err(RiskError::CorruptState);
        }
        let new_pos = i128_clamp_pos(new_pnl);

        // Pre-validate reserve mode BEFORE any mutation
        if new_pos > old_pos {
            match reserve_mode {
                ReserveMode::NoPositiveIncreaseAllowed => {
                    return Err(RiskError::Overflow);
                }
                ReserveMode::ImmediateReleaseResolvedOnly => {
                    if self.market_mode == MarketMode::Live {
                        return Err(RiskError::Unauthorized);
                    }
                }
                ReserveMode::UseAdmissionPair(_, _) => {
                    if self.market_mode != MarketMode::Live {
                        return Err(RiskError::Unauthorized);
                    }
                }
            }
        }

        if self.market_mode == MarketMode::Live && new_pos > MAX_ACCOUNT_POSITIVE_PNL {
            return Err(RiskError::Overflow);
        }

        // Pre-validate aggregate cap before mutation
        if new_pos > old_pos {
            let delta = new_pos - old_pos;
            let new_tot = self.pnl_pos_tot.checked_add(delta).ok_or(RiskError::Overflow)?;
            if self.market_mode == MarketMode::Live && new_tot > MAX_PNL_POS_TOT {
                return Err(RiskError::Overflow);
            }
        }

        if new_pos > old_pos {
            let delta = new_pos - old_pos;
            self.pnl_pos_tot = self.pnl_pos_tot.checked_add(delta).ok_or(RiskError::Overflow)?;
        } else if old_pos > new_pos {
            let delta = old_pos - new_pos;
            self.pnl_pos_tot = self.pnl_pos_tot.checked_sub(delta).ok_or(RiskError::Overflow)?;
        }

        if new_pos > old_pos {
            let reserve_add = new_pos - old_pos;
            if old < 0 && new_pnl >= 0 {
                self.neg_pnl_account_count = self.neg_pnl_account_count.checked_sub(1)
                    .ok_or(RiskError::CorruptState)?;
            } else if old >= 0 && new_pnl < 0 {
                self.neg_pnl_account_count = self.neg_pnl_account_count.checked_add(1)
                    .ok_or(RiskError::CorruptState)?;
            }
            self.accounts[idx].pnl = new_pnl;

            match reserve_mode {
                ReserveMode::NoPositiveIncreaseAllowed => {
                    return Err(RiskError::Overflow); // unreachable: pre-validated
                }
                ReserveMode::ImmediateReleaseResolvedOnly => {
                    // Only valid in Resolved mode (pre-validated above)
                    self.pnl_matured_pos_tot = self.pnl_matured_pos_tot.checked_add(reserve_add)
                        .ok_or(RiskError::Overflow)?;
                    // Spec §4.8 step 18: invariant pair.
                    if self.pnl_matured_pos_tot > self.pnl_pos_tot { return Err(RiskError::CorruptState); }
                    let pos_pnl_final: u128 = if new_pnl > 0 { new_pnl as u128 } else { 0 };
                    if self.accounts[idx].reserved_pnl > pos_pnl_final { return Err(RiskError::CorruptState); }
                    return Ok(());
                }
                ReserveMode::UseAdmissionPair(admit_h_min, admit_h_max) => {
                    // Admission-pair: engine decides effective horizon (spec §4.7)
                    let ctx = ctx.ok_or(RiskError::CorruptState)?;
                    let stress_active = self.stress_gate_active(ctx);
                    let admitted_h_eff = self.admit_fresh_reserve_h_lock(
                        idx, reserve_add, ctx, admit_h_min, admit_h_max)?;
                    if admitted_h_eff != admit_h_max {
                        ctx.mark_positive_pnl_usability();
                    }
                    if admitted_h_eff == 0 {
                        self.pnl_matured_pos_tot = self.pnl_matured_pos_tot.checked_add(reserve_add)
                            .ok_or(RiskError::Overflow)?;
                    } else {
                        self.append_or_route_new_reserve_with_stress(
                            idx,
                            reserve_add,
                            self.current_slot,
                            admitted_h_eff,
                            stress_active,
                        )?;
                    }
                    // Spec §4.8 step 18: invariant pair.
                    if self.pnl_matured_pos_tot > self.pnl_pos_tot { return Err(RiskError::CorruptState); }
                    let pos_pnl_final: u128 = if new_pnl > 0 { new_pnl as u128 } else { 0 };
                    if self.accounts[idx].reserved_pnl > pos_pnl_final { return Err(RiskError::CorruptState); }
                    return Ok(());
                }
            }
        } else {
            // Case B: no positive increase
            let pos_loss = old_pos - new_pos;
            if self.market_mode == MarketMode::Live {
                let reserve_loss = core::cmp::min(pos_loss, self.accounts[idx].reserved_pnl);
                if reserve_loss > 0 {
                    self.apply_reserve_loss_newest_first(idx, reserve_loss)?;
                }
                let matured_loss = pos_loss - reserve_loss;
                if matured_loss > 0 {
                    self.pnl_matured_pos_tot = self.pnl_matured_pos_tot.checked_sub(matured_loss)
                        .ok_or(RiskError::CorruptState)?;
                }
            } else {
                // Resolved: R_i must be 0
                if self.accounts[idx].reserved_pnl != 0 { return Err(RiskError::CorruptState); }
                if pos_loss > 0 {
                    self.pnl_matured_pos_tot = self.pnl_matured_pos_tot.checked_sub(pos_loss)
                        .ok_or(RiskError::CorruptState)?;
                }
            }
            // Track neg_pnl_account_count sign transitions (spec §4.7)
            if old < 0 && new_pnl >= 0 {
                self.neg_pnl_account_count = self.neg_pnl_account_count.checked_sub(1)
                    .ok_or(RiskError::CorruptState)?;
            } else if old >= 0 && new_pnl < 0 {
                self.neg_pnl_account_count = self.neg_pnl_account_count.checked_add(1)
                    .ok_or(RiskError::CorruptState)?;
            }
            self.accounts[idx].pnl = new_pnl;

            // Step 20: if new_pos == 0 and Live, require empty queue
            if new_pos == 0 && self.market_mode == MarketMode::Live {
                if self.accounts[idx].reserved_pnl != 0 { return Err(RiskError::CorruptState); }
                if self.accounts[idx].sched_present != 0 { return Err(RiskError::CorruptState); }
                if self.accounts[idx].pending_present != 0 { return Err(RiskError::CorruptState); }
            }

            // Spec §4.8 step 18: invariant pair.
            if self.pnl_matured_pos_tot > self.pnl_pos_tot { return Err(RiskError::CorruptState); }
            let pos_pnl_final: u128 = if new_pnl > 0 { new_pnl as u128 } else { 0 };
            if self.accounts[idx].reserved_pnl > pos_pnl_final { return Err(RiskError::CorruptState); }
            return Ok(());
        }
    }
    }

    /// consume_released_pnl (spec §4.4.1): remove only matured released positive PnL,
    /// leaving R_i unchanged.
    test_visible! {
    fn consume_released_pnl(&mut self, idx: usize, x: u128) -> Result<()> {
        if x == 0 { return Err(RiskError::CorruptState); }

        let old_pos = i128_clamp_pos(self.accounts[idx].pnl);
        let old_r = self.accounts[idx].reserved_pnl;
        let old_rel = old_pos.checked_sub(old_r).ok_or(RiskError::CorruptState)?;
        if x > old_rel { return Err(RiskError::CorruptState); }

        let new_pos = old_pos.checked_sub(x).ok_or(RiskError::CorruptState)?;
        // Validation-only subtraction; result unused (new_rel would equal
        // old_rel - x >= 0 given the `x > old_rel` guard above).
        let _ = old_rel.checked_sub(x).ok_or(RiskError::CorruptState)?;
        if new_pos < old_r { return Err(RiskError::CorruptState); }

        // Update pnl_pos_tot
        self.pnl_pos_tot = self.pnl_pos_tot.checked_sub(x)
            .ok_or(RiskError::CorruptState)?;

        // Update pnl_matured_pos_tot
        self.pnl_matured_pos_tot = self.pnl_matured_pos_tot.checked_sub(x)
            .ok_or(RiskError::CorruptState)?;
        if self.pnl_matured_pos_tot > self.pnl_pos_tot { return Err(RiskError::CorruptState); }

        // PNL_i = checked_sub_i128(PNL_i, checked_cast_i128(x))
        let x_i128: i128 = x.try_into().map_err(|_| RiskError::Overflow)?;
        let new_pnl = self.accounts[idx].pnl.checked_sub(x_i128)
            .ok_or(RiskError::Overflow)?;
        if new_pnl == i128::MIN { return Err(RiskError::Overflow); }
        self.accounts[idx].pnl = new_pnl;
        // R_i remains unchanged
        Ok(())
    }
    }

    /// set_capital (spec §4.2): checked signed-delta update of C_tot
    test_visible! {
    fn set_capital(&mut self, idx: usize, new_capital: u128) -> Result<()> {
        let old = self.accounts[idx].capital.get();
        if new_capital >= old {
            let delta = new_capital - old;
            self.c_tot = U128::new(self.c_tot.get().checked_add(delta)
                .ok_or(RiskError::Overflow)?);
        } else {
            let delta = old - new_capital;
            self.c_tot = U128::new(self.c_tot.get().checked_sub(delta)
                .ok_or(RiskError::CorruptState)?);
        }
        self.accounts[idx].capital = U128::new(new_capital);
        Ok(())
    }
    }

    /// set_position_basis_q (spec §4.4 + property 37): update stored pos
    /// counts based on sign changes. Enforces `cfg_max_active_positions_per_side`
    /// on any INCREMENTING transition (0 → nonzero, sign flip) by default.
    ///
    /// `allow_transient_spike = true` skips the per-attach increment check
    /// and is used only by `execute_trade_not_atomic`'s 2-attach bilateral
    /// swap where attach order can transiently push count to cap+1 before
    /// the second attach brings it back. Trade-level pre-flight proves
    /// the FINAL count does not breach cap; the end-of-instruction
    /// `assert_public_postconditions` re-verifies.
    test_visible! {
    fn set_position_basis_q(&mut self, idx: usize, new_basis: i128) -> Result<()> {
        self.set_position_basis_q_inner(idx, new_basis, /*allow_transient_spike=*/false)
    }
    }

    test_visible! {
    fn set_position_basis_q_allow_spike(&mut self, idx: usize, new_basis: i128) -> Result<()> {
        self.set_position_basis_q_inner(idx, new_basis, /*allow_transient_spike=*/true)
    }
    }

    test_visible! {
    fn clear_position_basis_q(&mut self, idx: usize) -> Result<()> {
        self.set_position_basis_q(idx, 0i128)?;
        self.accounts[idx].adl_a_basis = ADL_ONE;
        self.accounts[idx].adl_k_snap = 0i128;
        self.accounts[idx].f_snap = 0i128;
        self.accounts[idx].adl_epoch_snap = 0;
        Ok(())
    }
    }

    fn set_position_basis_q_inner(
        &mut self,
        idx: usize,
        new_basis: i128,
        allow_transient_spike: bool,
    ) -> Result<()> {
        if idx >= MAX_ACCOUNTS {
            return Err(RiskError::AccountNotFound);
        }
        let old = self.accounts[idx].position_basis_q;
        let old_side = side_of_i128(old);
        let new_side = side_of_i128(new_basis);
        let mut next_long = self.stored_pos_count_long;
        let mut next_short = self.stored_pos_count_short;

        if let Some(s) = old_side {
            match s {
                Side::Long => {
                    next_long = next_long.checked_sub(1).ok_or(RiskError::CorruptState)?;
                }
                Side::Short => {
                    next_short = next_short.checked_sub(1).ok_or(RiskError::CorruptState)?;
                }
            }
        }

        if let Some(s) = new_side {
            match s {
                Side::Long => {
                    next_long = next_long.checked_add(1).ok_or(RiskError::CorruptState)?;
                }
                Side::Short => {
                    next_short = next_short.checked_add(1).ok_or(RiskError::CorruptState)?;
                }
            }
        }
        let cap = self.params.max_active_positions_per_side;
        if !allow_transient_spike && (next_long > cap || next_short > cap) {
            return Err(RiskError::Overflow);
        }

        self.stored_pos_count_long = next_long;
        self.stored_pos_count_short = next_short;
        self.accounts[idx].position_basis_q = new_basis;
        Ok(())
    }

    /// attach_effective_position (spec §4.5)
    test_visible! {
    fn attach_effective_position(&mut self, idx: usize, new_eff_pos_q: i128) -> Result<()> {
        self.attach_effective_position_inner(idx, new_eff_pos_q, /*allow_spike=*/false)
    }
    }

    /// Variant used by `execute_trade_not_atomic`'s 2-attach bilateral swap.
    /// The trade's pre-flight cap check (execute_trade_not_atomic line ~4327)
    /// proves the FINAL per-side count does not breach cap; within the
    /// two-call attach sequence, either arg order can transiently push count
    /// to cap+1. This variant skips the per-attach cap check while still
    /// decrementing counts correctly and enforcing all other invariants.
    fn attach_effective_position_allow_spike(
        &mut self,
        idx: usize,
        new_eff_pos_q: i128,
    ) -> Result<()> {
        self.attach_effective_position_inner(idx, new_eff_pos_q, /*allow_spike=*/ true)
    }

    fn attach_effective_position_inner(
        &mut self,
        idx: usize,
        new_eff_pos_q: i128,
        allow_spike: bool,
    ) -> Result<()> {
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
                        let product =
                            U256::from_u128(abs_basis).checked_mul(U256::from_u128(a_side));
                        if let Some(p) = product {
                            let rem = p.checked_rem(U256::from_u128(a_basis));
                            if let Some(r) = rem {
                                if !r.is_zero() {
                                    self.inc_phantom_dust_potential(old_side)?;
                                }
                            }
                        }
                    }
                }
            }
        }

        if new_eff_pos_q == 0 {
            // Decrement-only path — no cap check needed.
            self.clear_position_basis_q(idx)?;
        } else {
            // Spec §4.6: abs(new_eff_pos_q) <= MAX_POSITION_ABS_Q
            if new_eff_pos_q.unsigned_abs() > MAX_POSITION_ABS_Q {
                return Err(RiskError::Overflow);
            }
            let side = side_of_i128(new_eff_pos_q).ok_or(RiskError::CorruptState)?;
            self.validate_persistent_global_signed_shape()?;
            if allow_spike {
                self.set_position_basis_q_allow_spike(idx, new_eff_pos_q)?;
            } else {
                self.set_position_basis_q(idx, new_eff_pos_q)?;
            }

            match side {
                Side::Long => {
                    self.accounts[idx].adl_a_basis = self.adl_mult_long;
                    self.accounts[idx].adl_k_snap = self.adl_coeff_long;
                    self.accounts[idx].f_snap = self.f_long_num;
                    self.accounts[idx].adl_epoch_snap = self.adl_epoch_long;
                }
                Side::Short => {
                    self.accounts[idx].adl_a_basis = self.adl_mult_short;
                    self.accounts[idx].adl_k_snap = self.adl_coeff_short;
                    self.accounts[idx].f_snap = self.f_short_num;
                    self.accounts[idx].adl_epoch_snap = self.adl_epoch_short;
                }
            }
        }
        Ok(())
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

    fn get_f_side(&self, s: Side) -> i128 {
        match s {
            Side::Long => self.f_long_num,
            Side::Short => self.f_short_num,
        }
    }

    fn get_f_epoch_start(&self, s: Side) -> i128 {
        match s {
            Side::Long => self.f_epoch_start_long_num,
            Side::Short => self.f_epoch_start_short_num,
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

    fn set_k_side(&mut self, s: Side, v: i128) -> Result<()> {
        if v == i128::MIN {
            return Err(RiskError::Overflow);
        }
        match s {
            Side::Long => self.adl_coeff_long = v,
            Side::Short => self.adl_coeff_short = v,
        }
        Ok(())
    }

    /// Compute per-account F-delta PnL.
    /// result = floor(abs_basis * (f_now - f_snap) / (den * FUNDING_DEN))
    /// Uses I256/U256 wide arithmetic to avoid i128 overflow.
    /// Mirrors the pattern of wide_signed_mul_div_floor_from_k_pair.
    /// Combined K/F settlement helper (spec §1.6).
    /// floor(abs_basis * ((k_now - k_then) * FUNDING_DEN + (f_now - f_then)) / (den * FUNDING_DEN))
    /// Uses exact 256-bit intermediates. Single floor on the combined numerator.
    fn compute_kf_pnl_delta(
        abs_basis: u128,
        k_snap: i128,
        k_now: i128,
        f_snap: i128,
        f_now: i128,
        den: u128,
    ) -> Result<i128> {
        if abs_basis == 0 {
            return Ok(0);
        }
        // K_diff in I256 — can reach 2*i128::MAX for opposing-sign K snapshots.
        let k_diff = I256::from_i128(k_now)
            .checked_sub(I256::from_i128(k_snap))
            .ok_or(RiskError::Overflow)?;
        // K_diff * FUNDING_DEN in exact I256 via abs/sign decomposition.
        // No narrowing through i128 or u128 — stays in U256/I256 throughout.
        let k_scaled = if k_diff.is_zero() {
            I256::ZERO
        } else {
            let neg = k_diff.is_negative();
            if k_diff == I256::MIN {
                return Err(RiskError::Overflow);
            }
            let abs_k = k_diff.abs_u256();
            let prod_u256 = abs_k
                .checked_mul(U256::from_u128(FUNDING_DEN))
                .ok_or(RiskError::Overflow)?;
            let pos = I256::from_u256_or_overflow(prod_u256).ok_or(RiskError::Overflow)?;
            if neg {
                I256::ZERO.checked_sub(pos).ok_or(RiskError::Overflow)?
            } else {
                pos
            }
        };
        // F_diff
        let f_diff = I256::from_i128(f_now)
            .checked_sub(I256::from_i128(f_snap))
            .ok_or(RiskError::Overflow)?;
        // Combined numerator = K_diff * FUNDING_DEN + F_diff
        let combined = k_scaled.checked_add(f_diff).ok_or(RiskError::Overflow)?;
        if combined.is_zero() {
            return Ok(0);
        }
        // abs_basis * |combined| / (den * FUNDING_DEN), floor toward -inf
        let negative = combined.is_negative();
        if combined == I256::MIN {
            return Err(RiskError::Overflow);
        }
        let abs_combined = combined.abs_u256();
        let abs_basis_u256 = U256::from_u128(abs_basis);
        let den_wide = U256::from_u128(den)
            .checked_mul(U256::from_u128(FUNDING_DEN))
            .ok_or(RiskError::Overflow)?;
        let p = abs_basis_u256
            .checked_mul(abs_combined)
            .ok_or(RiskError::Overflow)?;
        let (q, rem) = wide_math::div_rem_u256(p, den_wide);
        if negative {
            let mag = if !rem.is_zero() {
                q.checked_add(U256::ONE).ok_or(RiskError::Overflow)?
            } else {
                q
            };
            let mag_u128 = mag.try_into_u128().ok_or(RiskError::Overflow)?;
            if mag_u128 > i128::MAX as u128 {
                return Err(RiskError::Overflow);
            }
            Ok(-(mag_u128 as i128))
        } else {
            let q_u128 = q.try_into_u128().ok_or(RiskError::Overflow)?;
            if q_u128 > i128::MAX as u128 {
                return Err(RiskError::Overflow);
            }
            Ok(q_u128 as i128)
        }
    }

    /// Wide variant of compute_kf_pnl_delta that accepts I256 for k_now/f_now.
    /// Used by resolved reconciliation where K_epoch_start + terminal_delta may exceed i128.
    fn compute_kf_pnl_delta_wide(
        abs_basis: u128,
        k_snap: i128,
        k_now_wide: I256,
        f_snap: i128,
        f_now_wide: I256,
        den: u128,
    ) -> Result<i128> {
        if abs_basis == 0 {
            return Ok(0);
        }
        let k_diff = k_now_wide
            .checked_sub(I256::from_i128(k_snap))
            .ok_or(RiskError::Overflow)?;
        let k_scaled = if k_diff.is_zero() {
            I256::ZERO
        } else {
            let neg = k_diff.is_negative();
            if k_diff == I256::MIN {
                return Err(RiskError::Overflow);
            }
            let abs_k = k_diff.abs_u256();
            let prod_u256 = abs_k
                .checked_mul(U256::from_u128(FUNDING_DEN))
                .ok_or(RiskError::Overflow)?;
            let pos = I256::from_u256_or_overflow(prod_u256).ok_or(RiskError::Overflow)?;
            if neg {
                I256::ZERO.checked_sub(pos).ok_or(RiskError::Overflow)?
            } else {
                pos
            }
        };
        let f_diff = f_now_wide
            .checked_sub(I256::from_i128(f_snap))
            .ok_or(RiskError::Overflow)?;
        let combined = k_scaled.checked_add(f_diff).ok_or(RiskError::Overflow)?;
        if combined.is_zero() {
            return Ok(0);
        }
        let negative = combined.is_negative();
        if combined == I256::MIN {
            return Err(RiskError::Overflow);
        }
        let abs_combined = combined.abs_u256();
        let abs_basis_u256 = U256::from_u128(abs_basis);
        let den_wide = U256::from_u128(den)
            .checked_mul(U256::from_u128(FUNDING_DEN))
            .ok_or(RiskError::Overflow)?;
        let p = abs_basis_u256
            .checked_mul(abs_combined)
            .ok_or(RiskError::Overflow)?;
        let (q, rem) = wide_math::div_rem_u256(p, den_wide);
        if negative {
            let mag = if !rem.is_zero() {
                q.checked_add(U256::ONE).ok_or(RiskError::Overflow)?
            } else {
                q
            };
            let mag_u128 = mag.try_into_u128().ok_or(RiskError::Overflow)?;
            if mag_u128 > i128::MAX as u128 {
                return Err(RiskError::Overflow);
            }
            Ok(-(mag_u128 as i128))
        } else {
            let q_u128 = q.try_into_u128().ok_or(RiskError::Overflow)?;
            if q_u128 > i128::MAX as u128 {
                return Err(RiskError::Overflow);
            }
            Ok(q_u128 as i128)
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

    /// Spec §4.6: account-local floor slack adds potential dust only.
    fn inc_phantom_dust_potential(&mut self, s: Side) -> Result<()> {
        match s {
            Side::Long => {
                self.phantom_dust_potential_long_q = self
                    .phantom_dust_potential_long_q
                    .checked_add(1u128)
                    .ok_or(RiskError::Overflow)?;
            }
            Side::Short => {
                self.phantom_dust_potential_short_q = self
                    .phantom_dust_potential_short_q
                    .checked_add(1u128)
                    .ok_or(RiskError::Overflow)?;
            }
        }
        Ok(())
    }

    fn get_phantom_dust_certified(&self, s: Side) -> u128 {
        match s {
            Side::Long => self.phantom_dust_certified_long_q,
            Side::Short => self.phantom_dust_certified_short_q,
        }
    }

    fn set_phantom_dust_certified(&mut self, s: Side, v: u128) {
        match s {
            Side::Long => self.phantom_dust_certified_long_q = v,
            Side::Short => self.phantom_dust_certified_short_q = v,
        }
    }

    fn get_phantom_dust_potential(&self, s: Side) -> u128 {
        match s {
            Side::Long => self.phantom_dust_potential_long_q,
            Side::Short => self.phantom_dust_potential_short_q,
        }
    }

    fn set_phantom_dust_potential(&mut self, s: Side, v: u128) {
        match s {
            Side::Long => self.phantom_dust_potential_long_q = v,
            Side::Short => self.phantom_dust_potential_short_q = v,
        }
    }

    fn exact_adl_k_loss_upper_bound(
        &self,
        side: Side,
        delta_k_abs: u128,
        budget: u128,
    ) -> Result<u128> {
        if delta_k_abs == 0 {
            return Ok(0);
        }
        if self.params.max_accounts > MAX_ACCOUNTS as u64 {
            return Err(RiskError::CorruptState);
        }

        let epoch = self.get_epoch_side(side);
        let mut total = 0u128;
        let max_accounts = self.params.max_accounts as usize;
        for idx in 0..max_accounts {
            if !self.is_used(idx) {
                continue;
            }
            self.validate_touched_account_shape(idx)?;
            let account = &self.accounts[idx];
            if side_of_i128(account.position_basis_q) != Some(side) {
                continue;
            }
            if account.adl_epoch_snap != epoch {
                continue;
            }
            let den = account
                .adl_a_basis
                .checked_mul(POS_SCALE)
                .ok_or(RiskError::Overflow)?;
            let loss = mul_div_ceil_u256(
                U256::from_u128(account.position_basis_q.unsigned_abs()),
                U256::from_u128(delta_k_abs),
                U256::from_u128(den),
            )
            .try_into_u128()
            .ok_or(RiskError::Overflow)?;
            total = total.checked_add(loss).ok_or(RiskError::Overflow)?;
            if total > budget {
                return Ok(total);
            }
        }
        Ok(total)
    }

    fn threshold_stress_gate_active(&self, ctx: &InstructionContext) -> bool {
        match ctx.admit_h_max_consumption_threshold_bps_opt_shared {
            Some(threshold) => self.stress_consumed_bps_e9_since_envelope >= threshold,
            None => false,
        }
    }

    fn loss_stale_positive_pnl_lock_active(&self) -> bool {
        self.market_mode == MarketMode::Live
            && self.last_market_slot < self.current_slot
            && (self.oi_eff_long_q != 0 || self.oi_eff_short_q != 0)
    }

    fn real_stress_gate_active(&self, ctx: &InstructionContext) -> bool {
        self.threshold_stress_gate_active(ctx)
            || self.bankruptcy_hmax_lock_active
            || ctx.bankruptcy_hmax_candidate_active
            || self.loss_stale_positive_pnl_lock_active()
    }

    fn stress_gate_active(&self, ctx: &InstructionContext) -> bool {
        self.real_stress_gate_active(ctx) || ctx.speculative_hmax_guard_active
    }

    fn ensure_loss_current_for_position_change(&self) -> Result<()> {
        if self.loss_stale_positive_pnl_lock_active() {
            return Err(RiskError::Undercollateralized);
        }
        Ok(())
    }

    fn trigger_bankruptcy_hmax_lock(&mut self, ctx: &mut InstructionContext) -> Result<()> {
        if ctx.positive_pnl_usability_mutated && !self.real_stress_gate_active(ctx) {
            return Err(RiskError::Undercollateralized);
        }
        ctx.bankruptcy_hmax_candidate_active = true;
        ctx.stress_envelope_restarted = true;
        self.bankruptcy_hmax_lock_active = true;
        self.stress_envelope_remaining_indices = self.params.max_accounts;
        self.stress_envelope_start_slot = self.current_slot;
        self.stress_envelope_start_generation = self.sweep_generation;
        Ok(())
    }

    fn keeper_accrual_is_equity_active(
        &self,
        now_slot: u64,
        oracle_price: u64,
        funding_rate_e9: i128,
    ) -> bool {
        let total_dt = now_slot.saturating_sub(self.last_market_slot);
        let has_any_oi = self.oi_eff_long_q != 0 || self.oi_eff_short_q != 0;
        let price_move_active =
            self.last_oracle_price > 0 && oracle_price != self.last_oracle_price && has_any_oi;
        let funding_active = total_dt > 0
            && funding_rate_e9 != 0
            && self.oi_eff_long_q != 0
            && self.oi_eff_short_q != 0
            && self.fund_px_last > 0;
        price_move_active || funding_active
    }

    fn keeper_has_possible_protective_progress(
        &self,
        _now_slot: u64,
        ordered_candidates: &[(u16, Option<LiquidationPolicy>)],
        max_revalidations: u16,
        max_candidate_inspections: u16,
        rr_touch_limit: u64,
        rr_scan_limit: u64,
    ) -> Result<bool> {
        let max_candidate_inspections = core::cmp::min(
            MAX_TOUCHED_PER_INSTRUCTION as u16,
            max_candidate_inspections,
        );
        let mut inspected: u16 = 0;
        if max_revalidations > 0 && max_candidate_inspections > 0 {
            for &(candidate_idx, _) in ordered_candidates {
                if inspected >= max_candidate_inspections {
                    break;
                }
                inspected = inspected.checked_add(1).ok_or(RiskError::Overflow)?;
                let cidx = candidate_idx as usize;
                if cidx >= MAX_ACCOUNTS || !self.is_used(cidx) {
                    continue;
                }
                if candidate_idx as u64 >= self.params.max_accounts {
                    continue;
                }
                return Ok(true);
            }
        }

        if rr_touch_limit == 0 || rr_scan_limit == 0 {
            return Ok(false);
        }
        let wrap_bound = self.params.max_accounts;
        if wrap_bound == 0 || self.rr_cursor_position >= wrap_bound {
            return Err(RiskError::CorruptState);
        }
        Ok(core::cmp::min(rr_scan_limit, wrap_bound) > 0)
    }

    fn account_has_existing_bankruptcy_tail(&self, idx: usize) -> Result<bool> {
        self.validate_touched_account_shape(idx)?;
        let pnl = self.accounts[idx].pnl;
        if pnl >= 0 {
            return Ok(false);
        }
        if pnl == i128::MIN {
            return Err(RiskError::CorruptState);
        }
        Ok(self.accounts[idx].capital.get() < pnl.unsigned_abs())
    }

    fn pretrigger_bankruptcy_hmax_for_candidates(
        &mut self,
        ctx: &mut InstructionContext,
        ordered_candidates: &[(u16, Option<LiquidationPolicy>)],
        max_revalidations: u16,
        max_candidate_inspections: u16,
    ) -> Result<()> {
        if self.bankruptcy_hmax_lock_active || max_revalidations == 0 {
            return Ok(());
        }
        let max_candidate_inspections = core::cmp::min(
            MAX_TOUCHED_PER_INSTRUCTION as u16,
            max_candidate_inspections,
        );
        let mut inspected = 0u16;
        let mut attempts = 0u16;
        for &(candidate_idx, _) in ordered_candidates {
            if attempts >= max_revalidations || inspected >= max_candidate_inspections {
                break;
            }
            inspected = inspected.checked_add(1).ok_or(RiskError::Overflow)?;
            let idx = candidate_idx as usize;
            if idx >= MAX_ACCOUNTS || !self.is_used(idx) {
                continue;
            }
            if candidate_idx as u64 >= self.params.max_accounts {
                continue;
            }
            attempts = attempts.checked_add(1).ok_or(RiskError::Overflow)?;
            if self.account_has_existing_bankruptcy_tail(idx)? {
                self.trigger_bankruptcy_hmax_lock(ctx)?;
                break;
            }
        }
        Ok(())
    }

    test_visible! {
    fn pretrigger_bankruptcy_hmax_for_phase2(
        &mut self,
        ctx: &mut InstructionContext,
        inspected: u64,
    ) -> Result<()> {
        if self.bankruptcy_hmax_lock_active || inspected == 0 {
            return Ok(());
        }
        let wrap_bound = self.params.max_accounts;
        if wrap_bound == 0 || self.rr_cursor_position >= wrap_bound {
            return Err(RiskError::CorruptState);
        }
        let mut i = self.rr_cursor_position;
        let mut replayed = 0u64;
        while replayed < inspected {
            if i >= wrap_bound {
                return Err(RiskError::CorruptState);
            }
            let idx = i as usize;
            if self.is_used(idx) && self.account_has_existing_bankruptcy_tail(idx)? {
                self.trigger_bankruptcy_hmax_lock(ctx)?;
                break;
            }
            i = i.checked_add(1).ok_or(RiskError::Overflow)?;
            replayed = replayed.checked_add(1).ok_or(RiskError::Overflow)?;
            if i == wrap_bound {
                break;
            }
        }
        Ok(())
    }
    }

    // ========================================================================
    // effective_pos_q (spec §5.2)
    // ========================================================================

    fn effective_pos_q_checked(&self, idx: usize, require_used: bool) -> Result<i128> {
        if idx >= MAX_ACCOUNTS {
            return Err(RiskError::AccountNotFound);
        }
        if require_used && idx as u64 >= self.params.max_accounts {
            return Err(RiskError::AccountNotFound);
        }
        if require_used && !self.is_used(idx) {
            return Err(RiskError::AccountNotFound);
        }
        let basis = self.accounts[idx].position_basis_q;
        if basis == 0 {
            return Ok(0i128);
        }

        let side = side_of_i128(basis).ok_or(RiskError::CorruptState)?;
        let epoch_snap = self.accounts[idx].adl_epoch_snap;
        let epoch_side = self.get_epoch_side(side);

        if epoch_snap != epoch_side {
            if self.get_side_mode(side) != SideMode::ResetPending
                || epoch_snap.checked_add(1) != Some(epoch_side)
            {
                return Err(RiskError::CorruptState);
            }
            return Ok(0i128);
        }

        let a_side = self.get_a_side(side);
        let a_basis = self.accounts[idx].adl_a_basis;

        if a_basis == 0 {
            return Err(RiskError::CorruptState);
        }

        let abs_basis = basis.unsigned_abs();
        let effective_abs = mul_div_floor_u128(abs_basis, a_side, a_basis);

        if effective_abs > i128::MAX as u128 {
            return Err(RiskError::CorruptState);
        }

        if basis < 0 {
            if effective_abs == 0 {
                Ok(0i128)
            } else {
                Ok(-(effective_abs as i128))
            }
        } else {
            Ok(effective_abs as i128)
        }
    }

    pub fn try_effective_pos_q(&self, idx: usize) -> Result<i128> {
        self.effective_pos_q_checked(idx, true)
    }

    test_visible! {
    fn effective_pos_q(&self, idx: usize) -> i128 {
        self.effective_pos_q_checked(idx, false)
            .expect("canonical effective_pos_q state")
    }
    }

    /// settle_side_effects_live (spec §5.3): routes PnL delta
    /// through set_pnl_with_reserve with UseHLock for cohort queue.
    test_visible! {
    fn settle_side_effects_live(&mut self, idx: usize, ctx: &mut InstructionContext) -> Result<()> {
        let basis = self.accounts[idx].position_basis_q;
        if basis == 0 { return Ok(()); }

        let side = side_of_i128(basis).unwrap();
        let epoch_snap = self.accounts[idx].adl_epoch_snap;
        let epoch_side = self.get_epoch_side(side);
        let a_basis = self.accounts[idx].adl_a_basis;
        if a_basis == 0 { return Err(RiskError::CorruptState); }
        let abs_basis = basis.unsigned_abs();

        if epoch_snap == epoch_side {
            // Same epoch
            let a_side = self.get_a_side(side);
            let k_side = self.get_k_side(side);
            let k_snap = self.accounts[idx].adl_k_snap;
            let q_eff_new = mul_div_floor_u128(abs_basis, a_side, a_basis);
            let den = a_basis.checked_mul(POS_SCALE).ok_or(RiskError::Overflow)?;
            // Combined K/F settlement: single floor (spec §1.6).
            let f_side = self.get_f_side(side);
            let f_snap = self.accounts[idx].f_snap;
            let pnl_delta = Self::compute_kf_pnl_delta(abs_basis, k_snap, k_side, f_snap, f_side, den)?;

            let new_pnl = self.accounts[idx].pnl.checked_add(pnl_delta)
                .ok_or(RiskError::Overflow)?;
            if new_pnl == i128::MIN { return Err(RiskError::Overflow); }

            self.set_pnl_with_reserve(idx, new_pnl, ReserveMode::UseAdmissionPair(ctx.admit_h_min_shared, ctx.admit_h_max_shared), Some(ctx))?;

            if q_eff_new == 0 {
                self.inc_phantom_dust_potential(side)?;
                self.clear_position_basis_q(idx)?;
            } else {
                self.accounts[idx].adl_k_snap = k_side;
                self.accounts[idx].f_snap = f_side;
                self.accounts[idx].adl_epoch_snap = epoch_side;
            }
        } else {
            // Epoch mismatch — validate then mutate
            let side_mode = self.get_side_mode(side);
            if side_mode != SideMode::ResetPending { return Err(RiskError::CorruptState); }
            if epoch_snap.checked_add(1) != Some(epoch_side) { return Err(RiskError::CorruptState); }

            let k_epoch_start = self.get_k_epoch_start(side);
            let k_snap = self.accounts[idx].adl_k_snap;
            let den = a_basis.checked_mul(POS_SCALE).ok_or(RiskError::Overflow)?;
            // Combined K/F settlement for epoch mismatch (spec §1.6).
            let f_end = self.get_f_epoch_start(side);
            let f_snap = self.accounts[idx].f_snap;
            let pnl_delta = Self::compute_kf_pnl_delta(abs_basis, k_snap, k_epoch_start, f_snap, f_end, den)?;

            let new_pnl = self.accounts[idx].pnl.checked_add(pnl_delta)
                .ok_or(RiskError::Overflow)?;
            if new_pnl == i128::MIN { return Err(RiskError::Overflow); }

            let old_stale = self.get_stale_count(side);
            let new_stale = old_stale.checked_sub(1).ok_or(RiskError::CorruptState)?;

            // Mutate
            self.set_pnl_with_reserve(idx, new_pnl, ReserveMode::UseAdmissionPair(ctx.admit_h_min_shared, ctx.admit_h_max_shared), Some(ctx))?;
            self.clear_position_basis_q(idx)?;
            self.set_stale_count(side, new_stale);
        }

        Ok(())
    }

    }

    // ========================================================================
    // Live accrual envelope
    // ========================================================================

    /// Guard no-accrual live public paths that advance `current_slot` without
    /// advancing `last_market_slot`.
    ///
    /// Non-market-advancing public endpoints (top_up_insurance_fund,
    /// reclaim, charge_account_fee, settle_flat_negative_pnl, deposit_fee
    /// _credits, sync_account_fee_to_slot on Live) also set
    /// `current_slot = now_slot` for monotonicity but do NOT advance
    /// `last_market_slot`. Without this check a permissionless caller
    /// could pick any `now_slot > last_market_slot + max_dt`, committing
    /// the advance and permanently bricking live accrual — every
    /// subsequent `accrue_market_to(n, ..)` with `n >= current_slot`
    /// would fail because `n - last_market_slot > max_dt`, and
    /// monotonicity forbids smaller `n`.
    ///
    /// Zero-OI markets may fast-forward no-accrual paths because no live
    /// position can lose equity. Exposed markets use checked subtraction
    /// against `last_market_slot` to avoid `slot_last + max_dt` overflow.
    fn check_live_accrual_envelope(&self, now_slot: u64) -> Result<()> {
        if self.market_mode != MarketMode::Live {
            return Err(RiskError::Unauthorized);
        }
        if now_slot < self.current_slot {
            return Err(RiskError::Overflow);
        }
        if self.last_market_slot > self.current_slot {
            return Err(RiskError::CorruptState);
        }
        if self.oi_eff_long_q == 0 && self.oi_eff_short_q == 0 {
            return Ok(());
        }
        let dt = now_slot
            .checked_sub(self.last_market_slot)
            .ok_or(RiskError::Overflow)?;
        if dt > self.params.max_accrual_dt_slots {
            return Err(RiskError::Overflow);
        }
        Ok(())
    }

    // ========================================================================
    // accrue_market_to (spec §5.4)
    // ========================================================================

    pub fn accrue_market_to(
        &mut self,
        now_slot: u64,
        oracle_price: u64,
        funding_rate_e9: i128,
    ) -> Result<()> {
        // Pre-state invariant check: any corruption (including zero
        // last_oracle_price, out-of-range cursors, ready-flag inconsistency)
        // surfaces BEFORE any mutation. Same validate-then-mutate contract
        // as top_up_insurance_fund and deposit_fee_credits.
        self.assert_public_postconditions()?;
        self.accrue_market_segment_to_internal(
            now_slot,
            now_slot,
            now_slot,
            oracle_price,
            funding_rate_e9,
        )
    }

    fn accrue_market_segment_to_internal(
        &mut self,
        accrual_slot: u64,
        current_slot_after: u64,
        stress_start_slot_after: u64,
        oracle_price: u64,
        funding_rate_e9: i128,
    ) -> Result<()> {
        if self.market_mode != MarketMode::Live {
            return Err(RiskError::Unauthorized);
        }
        if oracle_price == 0 || oracle_price > MAX_ORACLE_PRICE {
            return Err(RiskError::Overflow);
        }

        // Validate funding rate bound (spec §1.4).
        if funding_rate_e9.unsigned_abs() > self.params.max_abs_funding_e9_per_slot as u128 {
            return Err(RiskError::Overflow);
        }

        // Time monotonicity (spec §5.4 preconditions)
        if current_slot_after < self.current_slot {
            return Err(RiskError::Overflow);
        }
        if accrual_slot < self.last_market_slot {
            return Err(RiskError::Overflow);
        }
        if accrual_slot > current_slot_after || stress_start_slot_after > current_slot_after {
            return Err(RiskError::Overflow);
        }

        // Step 4: snapshot OI at start (fixed for all sub-steps per spec §5.4)
        let long_live = self.oi_eff_long_q != 0;
        let short_live = self.oi_eff_short_q != 0;

        let total_dt = accrual_slot
            .checked_sub(self.last_market_slot)
            .ok_or(RiskError::Overflow)?;
        if total_dt == 0 && self.last_oracle_price == oracle_price {
            // Step 5: no change — set current_slot and return (spec §5.4)
            self.current_slot = current_slot_after;
            return Ok(());
        }

        // Spec §5.5 step 6-8 (v12.19.53): enforce per-call dt envelope whenever
        // funding OR price movement would actually drain equity.
        //
        // - funding_active: funding_rate != 0 AND both sides have OI AND fund_px_last > 0
        // - price_move_active: P_last > 0 AND oracle_price != P_last AND OI nonzero on some side
        //
        // If either is true, dt <= cfg_max_accrual_dt_slots MUST hold. This
        // is load-bearing for goal 52: bounded dt + bounded per-slot price
        // move + init-time solvency envelope together prevent the A1-class
        // self-neutral insurance siphon.
        //
        // Zero-OI idle markets and zero-funding-no-price-move cases remain
        // fast-forwardable; that's required for idle heartbeat cranks.
        let funding_active =
            funding_rate_e9 != 0 && long_live && short_live && self.fund_px_last > 0;
        let price_move_active = self.last_oracle_price > 0
            && oracle_price != self.last_oracle_price
            && (long_live || short_live);
        if (funding_active || price_move_active) && total_dt > self.params.max_accrual_dt_slots {
            return Err(RiskError::Overflow);
        }

        // Spec §5.5 step 9 (v12.19.53): per-accrual price-move cap.
        //
        //   require abs(oracle_price - P_last) * 10_000
        //           <= cfg_max_price_move_bps_per_slot * dt * P_last
        //
        // The check fires whenever price_move_active is true, INCLUDING
        // dt == 0. With dt == 0 and any nonzero price move, RHS = 0 and
        // LHS > 0 → rejects correctly. This closes the same-slot bypass
        // that would otherwise let live OI be marked through an arbitrary
        // price jump with zero elapsed time, weakening goal 52.
        //
        // Check fires BEFORE any K/F/P_last/slot_last/consumption mutation.
        let mut price_consumed_bps_e9: u128 = 0;
        if price_move_active {
            let abs_dp = (oracle_price as i128 - self.last_oracle_price as i128).unsigned_abs();
            // LHS = abs_dp * 10_000. abs_dp <= 2 * MAX_ORACLE_PRICE (2e12),
            // so LHS <= 2e16 — fits u128 trivially.
            let lhs = abs_dp.checked_mul(10_000u128).ok_or(RiskError::Overflow)?;
            // RHS = cap * dt * P_last. cap <= MAX_MARGIN_BPS (1e4, validated
            // in validate_params), dt <= u64::MAX (1.8e19), P_last <= 1e12,
            // product can exceed u128 (1.8e35), so compute in U256.
            let rhs = U256::from_u128(self.params.max_price_move_bps_per_slot as u128)
                .checked_mul(U256::from_u128(total_dt as u128))
                .and_then(|v| v.checked_mul(U256::from_u128(self.last_oracle_price as u128)))
                .ok_or(RiskError::Overflow)?;
            let lhs_wide = U256::from_u128(lhs);
            if lhs_wide > rhs {
                return Err(RiskError::Overflow);
            }

            // Spec §5.3: consumption is floor scaled-bps, not whole bps.
            // Sub-scaled-bps jitter floors to 0, and finite thresholds compare
            // in this same scaled domain.
            let consumed_wide = U256::from_u128(lhs)
                .checked_mul(U256::from_u128(STRESS_CONSUMPTION_SCALE))
                .ok_or(RiskError::Overflow)?
                .checked_div(U256::from_u128(self.last_oracle_price as u128))
                .ok_or(RiskError::Overflow)?;
            price_consumed_bps_e9 = consumed_wide.try_into_u128().unwrap_or(u128::MAX);
        }
        let mut funding_consumed_bps_e9: u128 = 0;
        if funding_active && total_dt > 0 {
            let funding_wide = U256::from_u128(funding_rate_e9.unsigned_abs())
                .checked_mul(U256::from_u128(total_dt as u128))
                .and_then(|v| v.checked_mul(U256::from_u128(10_000u128)))
                .unwrap_or(U256::MAX);
            funding_consumed_bps_e9 = funding_wide.try_into_u128().unwrap_or(u128::MAX);
        }
        let consumed_this_step = price_consumed_bps_e9.saturating_add(funding_consumed_bps_e9);

        // Use scratch K values for the entire mark + funding computation.
        // Only commit to engine state after ALL computations succeed.
        // This prevents partial K advancement on mid-function errors.
        let mut k_long = self.adl_coeff_long;
        let mut k_short = self.adl_coeff_short;

        // Step 5: Mark-to-market (once, spec §1.5 item 21)
        let current_price = self.last_oracle_price;
        let delta_p = (oracle_price as i128)
            .checked_sub(current_price as i128)
            .ok_or(RiskError::Overflow)?;
        if delta_p != 0 {
            // Compute mark deltas in I256, only fail when final K doesn't fit i128.
            // This avoids false overflow when delta magnitude > i128::MAX but
            // current K has opposite sign so the sum still fits.
            let delta_p_wide = I256::from_i128(delta_p);
            if long_live {
                let a_long_wide = I256::from_u128(self.adl_mult_long);
                let dk_wide = a_long_wide
                    .checked_mul_i256(delta_p_wide)
                    .ok_or(RiskError::Overflow)?;
                let k_long_wide = I256::from_i128(k_long)
                    .checked_add(dk_wide)
                    .ok_or(RiskError::Overflow)?;
                k_long = Self::try_into_non_min_i128(k_long_wide)?;
            }
            if short_live {
                let a_short_wide = I256::from_u128(self.adl_mult_short);
                let dk_wide = a_short_wide
                    .checked_mul_i256(delta_p_wide)
                    .ok_or(RiskError::Overflow)?;
                let k_short_wide = I256::from_i128(k_short)
                    .checked_sub(dk_wide)
                    .ok_or(RiskError::Overflow)?;
                k_short = Self::try_into_non_min_i128(k_short_wide)?;
            }
        }

        // Step 8: Funding transfer: one exact total delta (spec §5.5).
        // fund_num_total = fund_px_0 * funding_rate_e9_per_slot * dt
        // computed in exact wide signed domain. No substep loop.
        let mut f_long = self.f_long_num;
        let mut f_short = self.f_short_num;
        if funding_rate_e9 != 0 && total_dt > 0 && long_live && short_live {
            let fund_px_0 = self.fund_px_last;

            if fund_px_0 > 0 {
                // Exact computation in I256: fund_num_total = fund_px_0 * rate * dt
                // Only fail when final persisted F doesn't fit i128.
                let px_wide = I256::from_u128(fund_px_0 as u128);
                let rate_wide = I256::from_i128(funding_rate_e9);
                let dt_wide = I256::from_u128(total_dt as u128);
                let fund_num_total_wide = px_wide
                    .checked_mul_i256(rate_wide)
                    .ok_or(RiskError::Overflow)?
                    .checked_mul_i256(dt_wide)
                    .ok_or(RiskError::Overflow)?;

                // F_long -= A_long * fund_num_total
                let a_long_wide = I256::from_u128(self.adl_mult_long);
                let df_long_wide = a_long_wide
                    .checked_mul_i256(fund_num_total_wide)
                    .ok_or(RiskError::Overflow)?;
                let f_long_wide = I256::from_i128(f_long)
                    .checked_sub(df_long_wide)
                    .ok_or(RiskError::Overflow)?;
                f_long = Self::try_into_non_min_i128(f_long_wide)?;

                // F_short += A_short * fund_num_total
                let a_short_wide = I256::from_u128(self.adl_mult_short);
                let df_short_wide = a_short_wide
                    .checked_mul_i256(fund_num_total_wide)
                    .ok_or(RiskError::Overflow)?;
                let f_short_wide = I256::from_i128(f_short)
                    .checked_add(df_short_wide)
                    .ok_or(RiskError::Overflow)?;
                f_short = Self::try_into_non_min_i128(f_short_wide)?;
            }
        }

        self.validate_live_kf_future_headroom(k_long, k_short, f_long, f_short)?;

        let new_stress_consumed = self
            .stress_consumed_bps_e9_since_envelope
            .saturating_add(consumed_this_step);
        let mut stress_remaining = self.stress_envelope_remaining_indices;
        let mut stress_start_slot = self.stress_envelope_start_slot;
        let mut stress_start_generation = self.stress_envelope_start_generation;
        if consumed_this_step > 0 {
            stress_remaining = self.params.max_accounts;
            stress_start_slot = stress_start_slot_after;
            stress_start_generation = self.sweep_generation;
        }

        // ALL computations succeeded — commit all state atomically.
        self.adl_coeff_long = k_long;
        self.adl_coeff_short = k_short;
        self.f_long_num = f_long;
        self.f_short_num = f_short;
        self.current_slot = current_slot_after;
        self.last_market_slot = accrual_slot;
        self.last_oracle_price = oracle_price;
        self.fund_px_last = oracle_price;
        self.stress_consumed_bps_e9_since_envelope = new_stress_consumed;
        self.stress_envelope_remaining_indices = stress_remaining;
        self.stress_envelope_start_slot = stress_start_slot;
        self.stress_envelope_start_generation = stress_start_generation;

        // Post-state sanity check — should be a no-op if pre-state was valid
        // and the math is correct.
        self.assert_public_postconditions()?;
        Ok(())
    }

    /// Validate h_lock before any state mutation.
    #[cfg_attr(any(feature = "test", feature = "stress", kani), doc(hidden))]
    pub fn validate_admission_pair(
        admit_h_min: u64,
        admit_h_max: u64,
        params: &RiskParams,
    ) -> Result<()> {
        // spec §1.4: for live instructions that may create fresh reserve,
        // admit_h_max > 0 and admit_h_max >= cfg_h_min.
        // admit_h_max == 0 would bypass admission entirely (0 returned regardless
        // of state), breaking the h=1 invariant. Reject.
        if admit_h_max == 0 {
            return Err(RiskError::Overflow);
        }
        if admit_h_max < params.h_min {
            return Err(RiskError::Overflow);
        }
        // 0 <= admit_h_min <= admit_h_max <= cfg_h_max
        if admit_h_min > admit_h_max {
            return Err(RiskError::Overflow);
        }
        if admit_h_max > params.h_max {
            return Err(RiskError::Overflow);
        }
        // if admit_h_min > 0, then admit_h_min >= cfg_h_min
        if admit_h_min > 0 && admit_h_min < params.h_min {
            return Err(RiskError::Overflow);
        }
        Ok(())
    }

    /// Validate the optional consumption-threshold (spec §4.7, §9.0 step 1,
    /// v12.19.53). `None` disables the gate; `Some(threshold)` requires
    /// `threshold > 0`. `Some(0)` is invalid and must be rejected
    /// conservatively before any state mutation.
    pub fn validate_threshold_opt(threshold_opt: Option<u128>) -> Result<()> {
        match threshold_opt {
            None => Ok(()),
            Some(t) if t > 0 && t <= u128::MAX / STRESS_CONSUMPTION_SCALE => Ok(()),
            Some(_) => Err(RiskError::Overflow),
        }
    }

    // ========================================================================
    // absorb_protocol_loss (spec §4.7)
    // ========================================================================

    /// use_insurance_buffer (spec §4.11): deduct loss from insurance down to floor,
    /// return the remaining uninsured loss. Losses consume the full
    /// insurance balance; haircut activates when balance reaches zero.
    fn use_insurance_buffer(&mut self, loss: u128) -> u128 {
        if loss == 0 {
            return 0;
        }
        let ins_bal = self.insurance_fund.balance.get();
        let pay = core::cmp::min(loss, ins_bal);
        if pay > 0 {
            self.insurance_fund.balance = U128::new(ins_bal - pay);
        }
        loss - pay
    }

    /// record_uninsured_protocol_loss (spec §4.17): bookkeeping no-op.
    ///
    /// After insurance is drained, any remaining uninsured loss is already
    /// implicitly represented by the junior haircut mechanism: the forgiven
    /// negative PnL leaves the matched positive PnL (matured_pos_tot) as an
    /// unchanged claim against Residual = V - C_tot - I. When
    /// matured_pos_tot > Residual, payouts scale by h = Residual/matured.
    ///
    /// MUST NOT drain V here — doing so would shrink Residual below its
    /// natural post-forgiveness value and double-penalize junior holders
    /// (first via h < 1, again via V reduction).
    ///
    /// Intuition: Alice +100, Bob -100, V = 50, insurance = 0. Forgiving Bob
    /// leaves matured = 100, residual = 50 → h = 0.5, Alice gets 50. If we
    /// also drained V by 50, residual would drop to 0 → Alice gets 0.
    #[allow(unused_variables)]
    fn record_uninsured_protocol_loss(&mut self, loss: u128) {
        // Intentional no-op. See doc comment.
    }

    /// absorb_protocol_loss (spec §4.17): use_insurance_buffer then
    /// record_uninsured_protocol_loss for any remainder.
    test_visible! {
    fn absorb_protocol_loss(&mut self, loss: u128) {
        if loss == 0 {
            return;
        }
        let rem = self.use_insurance_buffer(loss);
        self.record_uninsured_protocol_loss(rem);
    }
    }

    // ========================================================================
    // sync_account_fee_to_slot (spec §4.6.1)
    // ========================================================================

    fn ensure_fee_draw_does_not_precede_loss(&self, idx: usize) -> Result<()> {
        if self.accounts[idx].pnl < 0 {
            return Err(RiskError::Undercollateralized);
        }
        if self.market_mode == MarketMode::Live && self.account_has_unsettled_live_effects(idx)? {
            return Err(RiskError::Undercollateralized);
        }
        Ok(())
    }

    fn account_has_unsettled_live_effects(&self, idx: usize) -> Result<bool> {
        let account = &self.accounts[idx];
        if account.position_basis_q == 0 {
            return Ok(false);
        }
        let side = side_of_i128(account.position_basis_q).ok_or(RiskError::CorruptState)?;
        if account.adl_epoch_snap != self.get_epoch_side(side) {
            return Ok(true);
        }
        Ok(account.adl_a_basis != self.get_a_side(side)
            || account.adl_k_snap != self.get_k_side(side)
            || account.f_snap != self.get_f_side(side))
    }

    /// Internal helper that realizes wrapper-owned recurring maintenance fees
    /// for account `idx` over `[last_fee_slot, fee_slot_anchor]` at the given
    /// per-slot rate, then advances `last_fee_slot`.
    ///
    /// Preconditions:
    /// - `idx` is materialized
    /// - `fee_slot_anchor >= last_fee_slot` (monotonicity)
    /// - on Live:     `fee_slot_anchor <= current_slot`
    /// - on Resolved: `fee_slot_anchor <= resolved_slot`
    ///
    /// Behavior:
    /// - `fee_abs_raw = fee_rate_per_slot * dt` in wide U256 to prevent overflow.
    /// - Cap at `MAX_PROTOCOL_FEE_ABS` (spec §4.6.1 step 4 — liveness cap).
    /// - Route the capped amount through `charge_fee_to_insurance` so the
    ///   collectible portion moves C → I and any shortfall becomes local
    ///   fee debt; uncollectible tail is dropped.
    /// - Advance `last_fee_slot` to `fee_slot_anchor`.
    ///
    /// Kept test-visible so tests and Kani proofs can exercise the explicit
    /// anchor path. The public entrypoint (`sync_account_fee_to_slot_not_atomic`)
    /// does NOT accept a caller-supplied anchor; it derives the anchor from
    /// market mode (current_slot on Live, resolved_slot on Resolved).
    test_visible! {
    fn sync_account_fee_to_slot(
        &mut self,
        idx: usize,
        fee_slot_anchor: u64,
        fee_rate_per_slot: u128,
    ) -> Result<()> {
        self.validate_touched_account_shape(idx)?;
        let last = self.accounts[idx].last_fee_slot;
        if fee_slot_anchor < last { return Err(RiskError::Overflow); }
        // Mode-specific upper bound on the anchor.
        match self.market_mode {
            MarketMode::Live => {
                if fee_slot_anchor > self.current_slot {
                    return Err(RiskError::Overflow);
                }
            }
            MarketMode::Resolved => {
                if fee_slot_anchor > self.resolved_slot {
                    return Err(RiskError::Overflow);
                }
            }
        }
        let dt = fee_slot_anchor - last;
        if dt == 0 {
            // No-op at same anchor; still idempotent-advance (already at anchor).
            return Ok(());
        }
        if fee_rate_per_slot == 0 {
            self.accounts[idx].last_fee_slot = fee_slot_anchor;
            return Ok(());
        }
        // Exact wide multiply; cap at MAX_PROTOCOL_FEE_ABS for liveness.
        let raw = U256::from_u128(fee_rate_per_slot)
            .checked_mul(U256::from_u128(dt as u128))
            .ok_or(RiskError::Overflow)?;
        let cap = U256::from_u128(MAX_PROTOCOL_FEE_ABS);
        let fee_abs_u256 = if raw > cap { cap } else { raw };
        let fee_abs: u128 = fee_abs_u256.try_into_u128().ok_or(RiskError::Overflow)?;
        if fee_abs > 0 {
            self.ensure_fee_draw_does_not_precede_loss(idx)?;
            self.charge_fee_to_insurance(idx, fee_abs)?;
        }
        self.accounts[idx].last_fee_slot = fee_slot_anchor;
        Ok(())
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
        if d > 0 {
            self.trigger_bankruptcy_hmax_lock(ctx)?;
        }
        let d_rem = if d > 0 { self.use_insurance_buffer(d) } else { 0u128 };

        // Step 3: read opposing OI
        let oi = self.get_oi_eff(opp);

        // Step 4 (§5.6 step 4): if OI == 0
        if oi == 0 {
            if d_rem > 0 {
                self.record_uninsured_protocol_loss(d_rem);
            }
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
            if d_rem > 0 {
                self.record_uninsured_protocol_loss(d_rem);
            }
            self.set_oi_eff(opp, oi_post);
            if oi_post == 0 {
                self.set_phantom_dust_certified(opp, 0);
                self.set_phantom_dust_potential(opp, 0);
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
        let old_certified = core::cmp::min(self.get_phantom_dust_certified(opp), oi);
        let old_potential = core::cmp::min(self.get_phantom_dust_potential(opp), oi);
        let loss_bearing_oi = oi
            .checked_sub(old_certified)
            .ok_or(RiskError::CorruptState)?;
        let uncertified_potential = old_potential.saturating_sub(old_certified);

        // Step 7: phantom-adjusted K-loss allocation. Phantom OI is not a
        // valid loss-bearing denominator, so its pro-rata loss share is
        // recorded as uninsured unless an exact scan proves tighter state.
        if d_rem != 0 {
            let d_social = if loss_bearing_oi == 0 {
                self.record_uninsured_protocol_loss(d_rem);
                0
            } else {
                let d_phantom = if old_certified == 0 {
                    0
                } else {
                    mul_div_ceil_u256(
                        U256::from_u128(d_rem),
                        U256::from_u128(old_certified),
                        U256::from_u128(oi),
                    )
                    .try_into_u128()
                    .unwrap_or(u128::MAX)
                };
                let d_phantom = core::cmp::min(d_phantom, d_rem);
                if d_phantom > 0 {
                    self.record_uninsured_protocol_loss(d_phantom);
                }
                d_rem - d_phantom
            };
            if d_social != 0 {
                if uncertified_potential != 0 {
                    self.record_uninsured_protocol_loss(d_social);
                } else {
                    let a_ps = a_old.checked_mul(POS_SCALE).ok_or(RiskError::Overflow)?;
                    match wide_mul_div_ceil_u128_or_over_i128max(d_social, a_ps, loss_bearing_oi) {
                        Ok(delta_k_abs) => {
                            match self.exact_adl_k_loss_upper_bound(opp, delta_k_abs, d_social) {
                                Ok(loss_upper) if loss_upper <= d_social => {
                                    let delta_k = -(delta_k_abs as i128);
                                    let k_opp = self.get_k_side(opp);
                                    match k_opp.checked_add(delta_k) {
                                        Some(new_k)
                                            if Self::validate_k_future_headroom(a_old, new_k)
                                                .is_ok() =>
                                        {
                                            self.set_k_side(opp, new_k)?;
                                        }
                                        _ => {
                                            self.record_uninsured_protocol_loss(d_social);
                                        }
                                    }
                                }
                                Ok(_) | Err(RiskError::Overflow) => {
                                    self.record_uninsured_protocol_loss(d_social);
                                }
                                Err(e) => return Err(e),
                            }
                        }
                        Err(OverI128Magnitude) => {
                            self.record_uninsured_protocol_loss(d_social);
                        }
                    }
                }
            }
        }

        // Step 8 (§5.6 step 8): if OI_post == 0, both flags are set.
        if oi_post == 0 {
            self.set_oi_eff(opp, 0u128);
            self.set_phantom_dust_certified(opp, 0);
            self.set_phantom_dust_potential(opp, 0);
            set_pending_reset(ctx, opp);
            set_pending_reset(ctx, liq_side);
            return Ok(());
        }

        // Steps 8-9: compute A_candidate and A_trunc_rem using U256 intermediates
        let a_old_u256 = U256::from_u128(a_old);
        let oi_post_u256 = U256::from_u128(oi_post);
        let oi_u256 = U256::from_u128(oi);
        let (a_candidate_u256, _a_trunc_rem) = mul_div_floor_u256_with_rem(
            a_old_u256,
            oi_post_u256,
            oi_u256,
        );

        // Step 10: A_candidate > 0
        if !a_candidate_u256.is_zero() {
            let a_new = a_candidate_u256.try_into_u128().ok_or(RiskError::Overflow)?;
            self.set_a_side(opp, a_new);
            self.set_oi_eff(opp, oi_post);
            let represented_source_lower = oi
                .checked_sub(old_potential)
                .ok_or(RiskError::CorruptState)?;
            let represented_after = U256::from_u128(represented_source_lower)
                .checked_mul(U256::from_u128(a_new))
                .ok_or(RiskError::Overflow)?
                .checked_div(U256::from_u128(a_old))
                .ok_or(RiskError::Overflow)?
                .try_into_u128()
                .ok_or(RiskError::Overflow)?;
            let represented_after = core::cmp::min(oi_post, represented_after);
            let aggregate_gap = oi_post
                .checked_sub(represented_after)
                .ok_or(RiskError::CorruptState)?;
            let account_floor_bound = self.get_stored_pos_count(opp) as u128;
            let post_potential = core::cmp::min(
                oi_post,
                aggregate_gap
                    .checked_add(account_floor_bound)
                    .ok_or(RiskError::Overflow)?,
            );
            let post_certified = core::cmp::min(oi_post, old_certified.saturating_sub(q_close_q));
            self.set_phantom_dust_certified(opp, post_certified);
            self.set_phantom_dust_potential(opp, post_potential);
            if a_new < MIN_A_SIDE {
                self.set_side_mode(opp, SideMode::DrainOnly);
            }
            return Ok(());
        }

        // Step 11: precision exhaustion terminal drain
        self.set_oi_eff(opp, 0u128);
        self.set_oi_eff(liq_side, 0u128);
        self.set_phantom_dust_certified(opp, 0);
        self.set_phantom_dust_potential(opp, 0);
        self.set_phantom_dust_certified(liq_side, 0);
        self.set_phantom_dust_potential(liq_side, 0);
        set_pending_reset(ctx, opp);
        set_pending_reset(ctx, liq_side);

        Ok(())
    }
    }

    // ========================================================================
    // begin_full_drain_reset / finalize_side_reset (spec §2.5, §2.7)
    // ========================================================================

    test_visible! {
    fn begin_full_drain_reset(&mut self, side: Side) -> Result<()> {
        // Require OI_eff_side == 0
        if self.get_oi_eff(side) != 0 { return Err(RiskError::CorruptState); }
        if self.get_side_mode(side) == SideMode::ResetPending {
            return Err(RiskError::CorruptState);
        }
        self.validate_persistent_global_signed_shape()?;
        let next_epoch = self
            .get_epoch_side(side)
            .checked_add(1)
            .ok_or(RiskError::Overflow)?;

        // K_epoch_start_side = K_side
        let k = self.get_k_side(side);
        match side {
            Side::Long => self.adl_epoch_start_k_long = k,
            Side::Short => self.adl_epoch_start_k_short = k,
        }

        // F_epoch_start_side = F_side.
        match side {
            Side::Long => self.f_epoch_start_long_num = self.f_long_num,
            Side::Short => self.f_epoch_start_short_num = self.f_short_num,
        }

        // Reset live K_side and F_side to 0 for the new epoch (spec §2.10).
        //
        // Without this, a side that was ADL-shrunk far (small A_side) and
        // pushed K_side close to the i128 edge would carry that near-
        // boundary K into the new epoch, where A_side is restored to
        // ADL_ONE. The first mark-to-market after the side reopens would
        // then overflow K because
        //     |K_old_epoch| + ADL_ONE * delta_p
        // exceeds i128, even though the enqueue_adl headroom check (which
        // reserves only A_old * MAX_ORACLE_PRICE) accepted the K write.
        //
        // The zeroing is economically correct: stale accounts settle
        // against K_epoch_start_side / F_epoch_start_side_num (just
        // snapshotted above), not against the live indices. New-epoch
        // accounts snapshot the live K_side/F_side_num at attach time;
        // starting from 0 gives them a clean headroom baseline without
        // changing settlement semantics.
        match side {
            Side::Long => {
                self.adl_coeff_long = 0;
                self.f_long_num = 0;
            }
            Side::Short => {
                self.adl_coeff_short = 0;
                self.f_short_num = 0;
            }
        }

        // Increment epoch
        match side {
            Side::Long => self.adl_epoch_long = next_epoch,
            Side::Short => self.adl_epoch_short = next_epoch,
        }

        // A_side = ADL_ONE
        self.set_a_side(side, ADL_ONE);

        // stale_account_count_side = stored_pos_count_side
        let spc = self.get_stored_pos_count(side);
        self.set_stale_count(side, spc);

        // Reset starts a new side epoch; stale accounts settle against the
        // snapshotted epoch and no old dust proof carries forward.
        match side {
            Side::Long => {
                self.phantom_dust_certified_long_q = 0u128;
                self.phantom_dust_potential_long_q = 0u128;
            }
            Side::Short => {
                self.phantom_dust_certified_short_q = 0u128;
                self.phantom_dust_potential_short_q = 0u128;
            }
        }

        // mode = ResetPending
        self.set_side_mode(side, SideMode::ResetPending);
        Ok(())
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
        // §5.5.A: if both sides are aggregate-flat, residual symmetric OI is
        // certified orphan OI because no current-epoch account can represent it.
        if self.stored_pos_count_long == 0 && self.stored_pos_count_short == 0 {
            let has_residual = self.oi_eff_long_q != 0
                || self.oi_eff_short_q != 0
                || self.phantom_dust_certified_long_q != 0
                || self.phantom_dust_certified_short_q != 0
                || self.phantom_dust_potential_long_q != 0
                || self.phantom_dust_potential_short_q != 0;
            if has_residual {
                if self.oi_eff_long_q != self.oi_eff_short_q {
                    return Err(RiskError::CorruptState);
                }
                self.oi_eff_long_q = 0u128;
                self.oi_eff_short_q = 0u128;
                self.phantom_dust_certified_long_q = 0;
                self.phantom_dust_certified_short_q = 0;
                self.phantom_dust_potential_long_q = 0;
                self.phantom_dust_potential_short_q = 0;
                ctx.pending_reset_long = true;
                ctx.pending_reset_short = true;
            }
        }
        // §5.5.B: one empty side. Certified dust can clear the non-empty
        // side; otherwise the non-empty side enters explicit orphan-exposure
        // drain reset so future mark/funding cannot run against an orphan.
        else if self.stored_pos_count_long == 0 && self.stored_pos_count_short > 0 {
            let has_residual = self.oi_eff_long_q != 0
                || self.oi_eff_short_q != 0
                || self.phantom_dust_certified_long_q != 0
                || self.phantom_dust_certified_short_q != 0
                || self.phantom_dust_potential_long_q != 0
                || self.phantom_dust_potential_short_q != 0;
            if has_residual {
                if self.oi_eff_long_q != self.oi_eff_short_q {
                    return Err(RiskError::CorruptState);
                }
                self.oi_eff_long_q = 0u128;
                self.oi_eff_short_q = 0u128;
                self.phantom_dust_certified_long_q = 0;
                self.phantom_dust_certified_short_q = 0;
                self.phantom_dust_potential_long_q = 0;
                self.phantom_dust_potential_short_q = 0;
                ctx.pending_reset_long = true;
                ctx.pending_reset_short = true;
            }
        }
        // §5.5.C: symmetric case with short empty and long non-empty.
        else if self.stored_pos_count_short == 0 && self.stored_pos_count_long > 0 {
            let has_residual = self.oi_eff_long_q != 0
                || self.oi_eff_short_q != 0
                || self.phantom_dust_certified_long_q != 0
                || self.phantom_dust_certified_short_q != 0
                || self.phantom_dust_potential_long_q != 0
                || self.phantom_dust_potential_short_q != 0;
            if has_residual {
                if self.oi_eff_long_q != self.oi_eff_short_q {
                    return Err(RiskError::CorruptState);
                }
                self.oi_eff_long_q = 0u128;
                self.oi_eff_short_q = 0u128;
                self.phantom_dust_certified_long_q = 0;
                self.phantom_dust_certified_short_q = 0;
                self.phantom_dust_potential_long_q = 0;
                self.phantom_dust_potential_short_q = 0;
                ctx.pending_reset_long = true;
                ctx.pending_reset_short = true;
            }
        }

        // §5.7.D: DrainOnly sides with zero OI
        if self.side_mode_long == SideMode::DrainOnly && self.oi_eff_long_q == 0 {
            ctx.pending_reset_long = true;
        }
        if self.side_mode_short == SideMode::DrainOnly && self.oi_eff_short_q == 0 {
            ctx.pending_reset_short = true;
        }
        if self.stored_pos_count_long == 0 && self.oi_eff_long_q == 0 {
            self.phantom_dust_certified_long_q = 0;
            self.phantom_dust_potential_long_q = 0;
        }
        if self.stored_pos_count_short == 0 && self.oi_eff_short_q == 0 {
            self.phantom_dust_certified_short_q = 0;
            self.phantom_dust_potential_short_q = 0;
        }
        if self.stored_pos_count_long > 0
            && self.oi_eff_long_q == 0
            && self.side_mode_long != SideMode::ResetPending
        {
            ctx.pending_reset_long = true;
        }
        if self.stored_pos_count_short > 0
            && self.oi_eff_short_q == 0
            && self.side_mode_short != SideMode::ResetPending
        {
            ctx.pending_reset_short = true;
        }

        Ok(())
    }
    }

    test_visible! {
    fn finalize_end_of_instruction_resets(&mut self, ctx: &InstructionContext) -> Result<()> {
        if ctx.pending_reset_long && self.side_mode_long != SideMode::ResetPending {
            self.begin_full_drain_reset(Side::Long)?;
        }
        if ctx.pending_reset_short && self.side_mode_short != SideMode::ResetPending {
            self.begin_full_drain_reset(Side::Short)?;
        }
        self.maybe_finalize_ready_reset_sides();
        Ok(())
    }
    }

    /// Preflight finalize: if a side is ResetPending with OI=0, stale=0, pos_count=0,
    /// transition it back to Normal so fresh OI can be added.
    /// Called before OI-increase gating and at end-of-instruction.
    test_visible! {
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
    }

    // ========================================================================
    // Haircut and Equity (spec §3)
    // ========================================================================

    /// Compute haircut ratio (h_num, h_den) as u128 pair (spec §3.3)
    /// Uses pnl_matured_pos_tot as denominator.
    pub fn haircut_ratio(&self) -> (u128, u128) {
        if self.pnl_matured_pos_tot == 0 {
            return (1u128, 1u128);
        }
        let senior_sum = self
            .c_tot
            .get()
            .checked_add(self.insurance_fund.balance.get());
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
        let h_num = if residual < self.pnl_matured_pos_tot {
            residual
        } else {
            self.pnl_matured_pos_tot
        };
        (h_num, self.pnl_matured_pos_tot)
    }

    fn released_pos_checked(&self, idx: usize, require_used: bool) -> Result<u128> {
        if idx >= MAX_ACCOUNTS {
            return Err(RiskError::AccountNotFound);
        }
        if require_used && idx as u64 >= self.params.max_accounts {
            return Err(RiskError::AccountNotFound);
        }
        if require_used && !self.is_used(idx) {
            return Err(RiskError::AccountNotFound);
        }
        let pnl = self.accounts[idx].pnl;
        let pos_pnl = i128_clamp_pos(pnl);
        if self.market_mode == MarketMode::Resolved {
            return Ok(pos_pnl);
        }
        if self.accounts[idx].reserved_pnl > pos_pnl {
            return Err(RiskError::CorruptState);
        }
        Ok(pos_pnl - self.accounts[idx].reserved_pnl)
    }

    pub fn try_released_pos(&self, idx: usize) -> Result<u128> {
        self.released_pos_checked(idx, true)
    }

    fn effective_matured_pnl_checked(&self, idx: usize, require_used: bool) -> Result<u128> {
        let released = self.released_pos_checked(idx, require_used)?;
        if released == 0 {
            return Ok(0u128);
        }
        let (h_num, h_den) = self.haircut_ratio();
        if h_den == 0 {
            return Ok(released);
        }
        Ok(wide_mul_div_floor_u128(released, h_num, h_den))
    }

    pub fn try_effective_matured_pnl(&self, idx: usize) -> Result<u128> {
        self.effective_matured_pnl_checked(idx, true)
    }

    /// PNL_eff_matured_i (spec §3.3): haircutted matured released positive PnL
    test_visible! {
    fn effective_matured_pnl(&self, idx: usize) -> u128 {
        self.effective_matured_pnl_checked(idx, false)
            .expect("canonical matured PnL state")
    }
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
                // Overflow in either direction: fail conservative (spec §3.4).
                // i128::MIN + 1 fails every > 0 and > MM_req gate.
                i128::MIN + 1
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
        if raw < 0 {
            0i128
        } else {
            raw
        }
    }

    /// Eq_init_raw_i (spec §3.4): C_i + min(PNL_i, 0) + PNL_eff_matured_i - FeeDebt_i
    /// For initial margin and withdrawal checks. Uses haircutted matured PnL only.
    /// Returns i128. Negative overflow projected to i128::MIN + 1 per §3.4.
    pub fn account_equity_init_raw(&self, account: &Account, idx: usize) -> i128 {
        let cap = I256::from_u128(account.capital.get());
        let neg_pnl = I256::from_i128(if account.pnl < 0 { account.pnl } else { 0i128 });
        let eff_matured = match self.effective_matured_pnl_checked(idx, false) {
            Ok(v) => I256::from_u128(v),
            Err(_) => return i128::MIN + 1,
        };
        let fee_debt = I256::from_u128(fee_debt_u128_checked(account.fee_credits.get()));

        let sum = cap
            .checked_add(neg_pnl)
            .expect("I256 add overflow")
            .checked_add(eff_matured)
            .expect("I256 add overflow")
            .checked_sub(fee_debt)
            .expect("I256 sub overflow");

        match sum.try_into_i128() {
            Some(v) => v,
            None => {
                // Overflow in either direction: fail conservative.
                i128::MIN + 1
            }
        }
    }

    /// Eq_init_net_i (spec §3.4): max(0, Eq_init_raw_i). For IM checks (trades).
    pub fn account_equity_init_net(&self, account: &Account, idx: usize) -> i128 {
        let raw = self.account_equity_init_raw(account, idx);
        if raw < 0 {
            0i128
        } else {
            raw
        }
    }

    /// Eq_withdraw_raw_i (spec §3.5): C + min(PNL, 0) + PNL_eff_matured - FeeDebt.
    /// Uses exact I256 arithmetic. Includes haircutted matured released PnL.
    pub fn account_equity_withdraw_raw(&self, account: &Account, idx: usize) -> i128 {
        let cap = I256::from_u128(account.capital.get());
        let neg_pnl = I256::from_i128(if account.pnl < 0 { account.pnl } else { 0i128 });
        let eff_matured = match self.effective_matured_pnl_checked(idx, false) {
            Ok(v) => I256::from_u128(v),
            Err(_) => return i128::MIN + 1,
        };
        let fee_debt = I256::from_u128(fee_debt_u128_checked(account.fee_credits.get()));
        let sum = cap
            .checked_add(neg_pnl)
            .expect("I256 add")
            .checked_add(eff_matured)
            .expect("I256 add")
            .checked_sub(fee_debt)
            .expect("I256 sub");
        match sum.try_into_i128() {
            Some(v) => v,
            None => i128::MIN + 1, // fail conservative on any overflow
        }
    }

    pub fn account_equity_withdraw_no_pos_raw(&self, account: &Account) -> i128 {
        let cap = I256::from_u128(account.capital.get());
        let neg_pnl = I256::from_i128(if account.pnl < 0 { account.pnl } else { 0i128 });
        let fee_debt = I256::from_u128(fee_debt_u128_checked(account.fee_credits.get()));
        let sum = cap
            .checked_add(neg_pnl)
            .expect("I256 add")
            .checked_sub(fee_debt)
            .expect("I256 sub");
        sum.try_into_i128().unwrap_or(i128::MIN + 1)
    }

    /// max_safe_flat_conversion_released (spec §4.12).
    /// Returns largest x_safe <= x_cap such that converting x_safe released profit
    /// on a live flat account cannot make Eq_maint_raw_i negative post-conversion.
    /// Uses 256-bit exact intermediates per spec §1.6 item 29.
    pub fn max_safe_flat_conversion_released(
        &self,
        idx: usize,
        x_cap: u128,
        h_num: u128,
        h_den: u128,
    ) -> u128 {
        if x_cap == 0 {
            return 0;
        }
        if idx >= MAX_ACCOUNTS || idx as u64 >= self.params.max_accounts || !self.is_used(idx) {
            return 0;
        }
        let e_before = self.account_equity_maint_raw(&self.accounts[idx]);
        if e_before <= 0 {
            return 0;
        }
        if h_den == 0 || h_num > h_den {
            return 0;
        }
        if h_num == h_den {
            return x_cap;
        }
        let haircut_loss_num = h_den - h_num;
        // min(x_cap, floor(E_before * h_den / haircut_loss_num))
        let safe = wide_mul_div_floor_u128(e_before as u128, h_den, haircut_loss_num);
        core::cmp::min(x_cap, safe)
    }

    fn risk_notional_from_eff_q(eff: i128, oracle_price: u64) -> u128 {
        if eff == 0 {
            return 0;
        }
        mul_div_ceil_u128(eff.unsigned_abs(), oracle_price as u128, POS_SCALE)
    }

    fn notional_checked(&self, idx: usize, oracle_price: u64, require_used: bool) -> Result<u128> {
        if oracle_price == 0 || oracle_price > MAX_ORACLE_PRICE {
            return Err(RiskError::Overflow);
        }
        let eff = self.effective_pos_q_checked(idx, require_used)?;
        Ok(Self::risk_notional_from_eff_q(eff, oracle_price))
    }

    pub fn try_notional(&self, idx: usize, oracle_price: u64) -> Result<u128> {
        self.notional_checked(idx, oracle_price, true)
    }

    /// notional (spec §7): ceil(|effective_pos_q| * oracle_price / POS_SCALE)
    test_visible! {
    fn notional(&self, idx: usize, oracle_price: u64) -> u128 {
        self.notional_checked(idx, oracle_price, false)
            .expect("canonical risk notional state")
    }
    }

    /// is_above_maintenance_margin (spec §9.1): Eq_net_i > MM_req_i
    /// Per spec §9.1: if eff == 0 then MM_req = 0; else MM_req = max(proportional, MIN_NONZERO_MM_REQ)
    pub fn is_above_maintenance_margin(
        &self,
        account: &Account,
        idx: usize,
        oracle_price: u64,
    ) -> bool {
        let eq_net = self.account_equity_net(account, oracle_price);
        let Ok(eff) = self.effective_pos_q_checked(idx, false) else {
            return false;
        };
        if eff == 0 {
            return eq_net > 0;
        }
        let Ok(not) = self.notional_checked(idx, oracle_price, false) else {
            return false;
        };
        let proportional =
            mul_div_floor_u128(not, self.params.maintenance_margin_bps as u128, 10_000);
        let mm_req = core::cmp::max(proportional, self.params.min_nonzero_mm_req);
        let mm_req_i128 = if mm_req > i128::MAX as u128 {
            i128::MAX
        } else {
            mm_req as i128
        };
        eq_net > mm_req_i128
    }

    /// is_above_initial_margin (spec §9.1): exact Eq_init_raw_i >= IM_req_i
    /// Per spec §9.1: if eff == 0 then IM_req = 0; else IM_req = max(proportional, MIN_NONZERO_IM_REQ)
    /// Per spec §3.4: MUST use exact raw equity, not clamped Eq_init_net_i,
    /// so negative raw equity is distinguishable from zero.
    pub fn is_above_initial_margin(
        &self,
        account: &Account,
        idx: usize,
        oracle_price: u64,
    ) -> bool {
        let eq_init_raw = self.account_equity_init_raw(account, idx);
        let Ok(eff) = self.effective_pos_q_checked(idx, false) else {
            return false;
        };
        if eff == 0 {
            return eq_init_raw >= 0;
        }
        let Ok(not) = self.notional_checked(idx, oracle_price, false) else {
            return false;
        };
        let proportional = mul_div_floor_u128(not, self.params.initial_margin_bps as u128, 10_000);
        let im_req = core::cmp::max(proportional, self.params.min_nonzero_im_req);
        let im_req_i128 = if im_req > i128::MAX as u128 {
            i128::MAX
        } else {
            im_req as i128
        };
        eq_init_raw >= im_req_i128
    }

    /// Eq_trade_open_raw_i (spec §3.5): counterfactual trade approval
    /// metric with the candidate trade's own positive slippage removed.
    /// `candidate_trade_pnl` is the signed execution-slippage PnL for this account
    /// from the candidate trade under evaluation.
    pub fn account_equity_trade_open_raw(
        &self,
        account: &Account,
        _idx: usize,
        candidate_trade_pnl: i128,
    ) -> i128 {
        let trade_gain = if candidate_trade_pnl > 0 {
            candidate_trade_pnl as u128
        } else {
            0u128
        };

        // Trade lane uses FULL positive PnL via g (spec §3.5), not just released.
        // This allows unreleased reserved PnL to support the same account's
        // risk-increasing trades through the global haircut.
        // Only the candidate trade's own positive gain is neutralized.
        let pos_pnl = i128_clamp_pos(account.pnl);
        let pos_pnl_trade_open = pos_pnl.saturating_sub(trade_gain);

        // PNL_trade_open_i for loss component
        let pnl_trade_open = account
            .pnl
            .checked_sub(trade_gain as i128)
            .unwrap_or(i128::MIN + 1);

        // Counterfactual global positive aggregate (using pnl_pos_tot, not matured)
        // If aggregates are corrupt, return most restrictive equity (blocks trades)
        let pnl_pos_tot_trade_open = match self.pnl_pos_tot.checked_sub(pos_pnl) {
            Some(v) => match v.checked_add(pos_pnl_trade_open) {
                Some(v2) => v2,
                None => return i128::MIN + 1, // corrupt: blocks all trades
            },
            None => return i128::MIN + 1, // corrupt: blocks all trades
        };

        // Counterfactual trade haircut g
        let pnl_eff_trade_open = if pnl_pos_tot_trade_open == 0 {
            pos_pnl_trade_open
        } else {
            let senior_sum = self
                .c_tot
                .get()
                .checked_add(self.insurance_fund.balance.get())
                .unwrap_or(u128::MAX);
            let residual = if self.vault.get() >= senior_sum {
                self.vault.get() - senior_sum
            } else {
                0u128
            };
            let g_num = core::cmp::min(residual, pnl_pos_tot_trade_open);
            mul_div_floor_u128(pos_pnl_trade_open, g_num, pnl_pos_tot_trade_open)
        };

        // Eq_trade_open = C_i + min(PNL_trade_open, 0) + g*PosPNL_trade_open - FeeDebt
        let cap = I256::from_u128(account.capital.get());
        let neg_pnl = I256::from_i128(if pnl_trade_open < 0 {
            pnl_trade_open
        } else {
            0i128
        });
        let eff = I256::from_u128(pnl_eff_trade_open);
        let fee_debt = I256::from_u128(fee_debt_u128_checked(account.fee_credits.get()));

        let result = cap
            .checked_add(neg_pnl)
            .expect("I256 add")
            .checked_add(eff)
            .expect("I256 add")
            .checked_sub(fee_debt)
            .expect("I256 sub");

        match result.try_into_i128() {
            Some(v) => v,
            None => i128::MIN + 1, // fail conservative on any overflow
        }
    }

    pub fn account_equity_trade_open_no_pos_raw(
        &self,
        account: &Account,
        candidate_trade_pnl: i128,
    ) -> i128 {
        let trade_gain = if candidate_trade_pnl > 0 {
            candidate_trade_pnl as u128
        } else {
            0u128
        };
        let pnl_trade_open = account
            .pnl
            .checked_sub(trade_gain as i128)
            .unwrap_or(i128::MIN + 1);
        let cap = I256::from_u128(account.capital.get());
        let neg_pnl = I256::from_i128(if pnl_trade_open < 0 {
            pnl_trade_open
        } else {
            0i128
        });
        let fee_debt = I256::from_u128(fee_debt_u128_checked(account.fee_credits.get()));
        let result = cap
            .checked_add(neg_pnl)
            .expect("I256 add")
            .checked_sub(fee_debt)
            .expect("I256 sub");
        result.try_into_i128().unwrap_or(i128::MIN + 1)
    }

    /// is_above_initial_margin_trade_open (spec §9.1 + §3.5):
    /// Uses Eq_trade_open_raw_i for risk-increasing trade approval.
    pub fn is_above_initial_margin_trade_open(
        &self,
        account: &Account,
        idx: usize,
        oracle_price: u64,
        candidate_trade_pnl: i128,
    ) -> bool {
        let eq = self.account_equity_trade_open_raw(account, idx, candidate_trade_pnl);
        let Ok(eff) = self.effective_pos_q_checked(idx, false) else {
            return false;
        };
        if eff == 0 {
            return eq >= 0;
        }
        let Ok(not) = self.notional_checked(idx, oracle_price, false) else {
            return false;
        };
        let proportional = mul_div_floor_u128(not, self.params.initial_margin_bps as u128, 10_000);
        let im_req = core::cmp::max(proportional, self.params.min_nonzero_im_req);
        let im_req_i128 = if im_req > i128::MAX as u128 {
            i128::MAX
        } else {
            im_req as i128
        };
        eq >= im_req_i128
    }

    pub fn is_above_initial_margin_trade_open_no_pos(
        &self,
        account: &Account,
        idx: usize,
        oracle_price: u64,
        candidate_trade_pnl: i128,
    ) -> bool {
        let eq = self.account_equity_trade_open_no_pos_raw(account, candidate_trade_pnl);
        let Ok(eff) = self.effective_pos_q_checked(idx, false) else {
            return false;
        };
        if eff == 0 {
            return eq >= 0;
        }
        let Ok(not) = self.notional_checked(idx, oracle_price, false) else {
            return false;
        };
        let proportional = mul_div_floor_u128(not, self.params.initial_margin_bps as u128, 10_000);
        let im_req = core::cmp::max(proportional, self.params.min_nonzero_im_req);
        let im_req_i128 = if im_req > i128::MAX as u128 {
            i128::MAX
        } else {
            im_req as i128
        };
        eq >= im_req_i128
    }

    // ========================================================================
    // Conservation check (spec §3.1)
    // ========================================================================

    pub fn check_conservation(&self) -> bool {
        let senior = self
            .c_tot
            .get()
            .checked_add(self.insurance_fund.balance.get());
        match senior {
            Some(s) => self.vault.get() >= s,
            None => false,
        }
    }

    /// sweep_empty_market_surplus_to_insurance (spec §3.2, v12.19.53).
    ///
    /// When the last account closes, signed-floor rounding on PnL can
    /// leave `vault > c_tot + insurance_fund.balance` with no junior
    /// claim (positive PnL floored to 0 while the matched negative PnL
    /// floored toward -∞ to -1). Without a sweep, that rounding residual
    /// stays in `vault` forever: `c_tot == 0`, `pnl_pos_tot == 0`,
    /// `insurance_fund.balance == 0` — but `vault > 0`. The wrapper's
    /// slab-close check requires `vault == 0`, so the market cannot be
    /// retired.
    ///
    /// Called after `free_slot` from every terminal-close path. The
    /// sweep fires ONLY when the engine is fully empty:
    ///   - num_used_accounts == 0
    ///   - c_tot == 0, pnl_pos_tot == 0, pnl_matured_pos_tot == 0
    ///   - oi_eff_long_q == 0, oi_eff_short_q == 0
    /// In all other states it is a no-op and safe to call unconditionally.
    /// The sweep moves `vault - insurance` into `insurance`, after which
    /// `vault == insurance` and the wrapper's insurance-withdraw path
    /// can drain both together.
    fn sweep_empty_market_surplus_to_insurance(&mut self) -> Result<()> {
        if self.num_used_accounts != 0 {
            return Ok(());
        }
        if !self.c_tot.is_zero()
            || self.pnl_pos_tot != 0
            || self.pnl_matured_pos_tot != 0
            || self.oi_eff_long_q != 0
            || self.oi_eff_short_q != 0
        {
            return Ok(());
        }
        if self.stored_pos_count_long != 0
            || self.stored_pos_count_short != 0
            || self.stale_account_count_long != 0
            || self.stale_account_count_short != 0
            || self.neg_pnl_account_count != 0
        {
            return Err(RiskError::CorruptState);
        }
        let v = self.vault.get();
        let i = self.insurance_fund.balance.get();
        if v < i {
            return Err(RiskError::CorruptState);
        }
        let surplus = v - i;
        if surplus != 0 {
            // v = i + surplus, so the new balance fits whatever vault fit.
            self.insurance_fund.balance =
                U128::new(i.checked_add(surplus).ok_or(RiskError::Overflow)?);
        }
        Ok(())
    }

    fn assert_public_postconditions(&self) -> Result<()> {
        self.assert_public_postconditions_fast()?;
        #[cfg(all(
            not(kani),
            any(feature = "test", feature = "audit-scan", debug_assertions)
        ))]
        self.validate_public_account_postconditions()?;
        Ok(())
    }

    fn assert_public_postconditions_fast(&self) -> Result<()> {
        Self::validate_params_fast_shape(&self.params).map_err(|_| RiskError::CorruptState)?;
        self.validate_persistent_global_signed_shape()?;
        let vault = self.vault.get();
        let capital = self.c_tot.get();
        let insurance = self.insurance_fund.balance.get();
        if vault > MAX_VAULT_TVL || capital > vault || insurance > vault {
            return Err(RiskError::CorruptState);
        }
        if !self.check_conservation() {
            return Err(RiskError::CorruptState);
        }
        if self.oi_eff_long_q != self.oi_eff_short_q {
            return Err(RiskError::CorruptState);
        }
        if self.oi_eff_long_q > MAX_OI_SIDE_Q || self.oi_eff_short_q > MAX_OI_SIDE_Q {
            return Err(RiskError::CorruptState);
        }
        if self.adl_mult_long > ADL_ONE || self.adl_mult_short > ADL_ONE {
            return Err(RiskError::CorruptState);
        }
        if self.oi_eff_long_q != 0 && self.adl_mult_long == 0 {
            return Err(RiskError::CorruptState);
        }
        if self.oi_eff_short_q != 0 && self.adl_mult_short == 0 {
            return Err(RiskError::CorruptState);
        }
        if self.market_mode == MarketMode::Live {
            self.validate_live_kf_future_headroom(
                self.adl_coeff_long,
                self.adl_coeff_short,
                self.f_long_num,
                self.f_short_num,
            )
            .map_err(|_| RiskError::CorruptState)?;
        }
        // Spec §1.4 / §4.4: at the public surface, cfg_max_active_positions_per_side
        // MUST NOT be exceeded. Intra-instruction transient spikes (e.g. bilateral
        // trade attaching one holder before detaching another) are permitted
        // because no intermediate state is observable; the cap is a per-instruction
        // end-state invariant. This catches any call site that increments a side
        // without pre-validating the cap.
        let cap = self.params.max_active_positions_per_side;
        if self.stored_pos_count_long > cap || self.stored_pos_count_short > cap {
            return Err(RiskError::CorruptState);
        }
        if self.pnl_matured_pos_tot > self.pnl_pos_tot {
            return Err(RiskError::CorruptState);
        }
        if self.market_mode == MarketMode::Live && self.pnl_pos_tot > MAX_PNL_POS_TOT {
            return Err(RiskError::CorruptState);
        }
        if self.materialized_account_count > self.params.max_accounts {
            return Err(RiskError::CorruptState);
        }
        // num_used_accounts and materialized_account_count track the same
        // count under different names — they must agree.
        if self.materialized_account_count != self.num_used_accounts as u64 {
            return Err(RiskError::CorruptState);
        }
        if self.neg_pnl_account_count > self.materialized_account_count {
            return Err(RiskError::CorruptState);
        }
        if self.rr_cursor_position >= self.params.max_accounts {
            return Err(RiskError::CorruptState);
        }
        if self.last_sweep_generation_advance_slot != NO_SLOT
            && self.last_sweep_generation_advance_slot > self.current_slot
        {
            return Err(RiskError::CorruptState);
        }
        let reconciliation_envelope_active =
            self.stress_consumed_bps_e9_since_envelope > 0 || self.bankruptcy_hmax_lock_active;
        if !reconciliation_envelope_active {
            if self.stress_envelope_remaining_indices != 0
                || self.stress_envelope_start_slot != NO_SLOT
                || self.stress_envelope_start_generation != NO_SLOT
            {
                return Err(RiskError::CorruptState);
            }
        } else {
            if self.stress_envelope_start_slot == NO_SLOT
                || self.stress_envelope_start_generation == NO_SLOT
                || self.stress_envelope_remaining_indices > self.params.max_accounts
                || self.stress_envelope_start_slot > self.current_slot
            {
                return Err(RiskError::CorruptState);
            }
        }
        // Oracle-price sentinels are always valid (spec §1.5).
        if self.last_oracle_price == 0 || self.last_oracle_price > MAX_ORACLE_PRICE {
            return Err(RiskError::CorruptState);
        }
        if self.fund_px_last == 0 || self.fund_px_last > MAX_ORACLE_PRICE {
            return Err(RiskError::CorruptState);
        }
        // Monotonic slot invariant (spec §1.5): current_slot >= last_market_slot.
        if self.current_slot < self.last_market_slot {
            return Err(RiskError::CorruptState);
        }
        // resolved_payout_ready is a 0/1 latch.
        if self.resolved_payout_ready > 1 {
            return Err(RiskError::CorruptState);
        }
        // Before ready, both h_num and h_den MUST be zero (spec §2.2 invariant).
        if self.resolved_payout_ready == 0
            && (self.resolved_payout_h_num != 0 || self.resolved_payout_h_den != 0)
        {
            return Err(RiskError::CorruptState);
        }
        // Spec §6.8: when resolved payout snapshot is ready, h_num <= h_den.
        // Before ready, both should be zero (checked by the ready = 0 branch
        // in capture_resolved_payout_snapshot_if_needed callers).
        if self.resolved_payout_ready != 0
            && self.resolved_payout_h_num > self.resolved_payout_h_den
        {
            return Err(RiskError::CorruptState);
        }
        // Mode-specific state shape (spec §2.2).
        match self.market_mode {
            MarketMode::Live => {
                // All resolved_* fields MUST be zero on Live markets.
                if self.resolved_price != 0
                    || self.resolved_live_price != 0
                    || self.resolved_slot != 0
                    || self.resolved_k_long_terminal_delta != 0
                    || self.resolved_k_short_terminal_delta != 0
                    || self.resolved_payout_ready != 0
                {
                    return Err(RiskError::CorruptState);
                }
                if (self.side_mode_long == SideMode::ResetPending && self.oi_eff_long_q != 0)
                    || (self.side_mode_short == SideMode::ResetPending && self.oi_eff_short_q != 0)
                {
                    return Err(RiskError::CorruptState);
                }
                if self.side_mode_long != SideMode::ResetPending
                    && self.stored_pos_count_long > 0
                    && self.oi_eff_long_q == 0
                {
                    return Err(RiskError::CorruptState);
                }
                if self.side_mode_short != SideMode::ResetPending
                    && self.stored_pos_count_short > 0
                    && self.oi_eff_short_q == 0
                {
                    return Err(RiskError::CorruptState);
                }
                if self.stored_pos_count_long == 0
                    && self.oi_eff_long_q == 0
                    && (self.phantom_dust_certified_long_q != 0
                        || self.phantom_dust_potential_long_q != 0)
                {
                    return Err(RiskError::CorruptState);
                }
                if self.stored_pos_count_short == 0
                    && self.oi_eff_short_q == 0
                    && (self.phantom_dust_certified_short_q != 0
                        || self.phantom_dust_potential_short_q != 0)
                {
                    return Err(RiskError::CorruptState);
                }
            }
            MarketMode::Resolved => {
                if self.bankruptcy_hmax_lock_active {
                    return Err(RiskError::CorruptState);
                }
                // resolved_price and resolved_live_price MUST be strictly positive.
                if self.resolved_price == 0 || self.resolved_live_price == 0 {
                    return Err(RiskError::CorruptState);
                }
                // Spec §9.9 step 3: current_slot frozen at resolved_slot.
                if self.current_slot != self.resolved_slot {
                    return Err(RiskError::CorruptState);
                }
            }
        }
        Ok(())
    }

    fn advance_sweep_generation(&mut self, now_slot: u64) -> Result<()> {
        self.sweep_generation = self
            .sweep_generation
            .checked_add(1)
            .ok_or(RiskError::Overflow)?;
        self.last_sweep_generation_advance_slot = now_slot;
        Ok(())
    }

    fn clear_stress_envelope(&mut self) {
        self.stress_consumed_bps_e9_since_envelope = 0;
        self.stress_envelope_remaining_indices = 0;
        self.stress_envelope_start_slot = NO_SLOT;
        self.stress_envelope_start_generation = NO_SLOT;
        self.bankruptcy_hmax_lock_active = false;
    }

    fn apply_stress_envelope_progress(
        &mut self,
        now_slot: u64,
        counted_indices: u64,
    ) -> Result<()> {
        let envelope_active =
            self.stress_consumed_bps_e9_since_envelope > 0 || self.bankruptcy_hmax_lock_active;
        if !envelope_active || counted_indices == 0 {
            return Ok(());
        }
        let dec = core::cmp::min(self.stress_envelope_remaining_indices, counted_indices);
        self.stress_envelope_remaining_indices = self
            .stress_envelope_remaining_indices
            .checked_sub(dec)
            .ok_or(RiskError::CorruptState)?;
        let generation_after_stress = self.stress_envelope_start_generation != NO_SLOT
            && self.sweep_generation > self.stress_envelope_start_generation
            && self.last_sweep_generation_advance_slot != NO_SLOT
            && self.last_sweep_generation_advance_slot > self.stress_envelope_start_slot;
        if self.stress_envelope_remaining_indices == 0
            && self.stress_envelope_start_slot != now_slot
            && generation_after_stress
        {
            self.clear_stress_envelope();
        }
        Ok(())
    }

    #[cfg(all(
        not(kani),
        any(feature = "test", feature = "audit-scan", debug_assertions)
    ))]
    fn validate_public_account_postconditions(&self) -> Result<()> {
        let mut used_count = 0u64;
        let mut neg_count = 0u64;
        let mut stored_long = 0u64;
        let mut stored_short = 0u64;
        let mut stale_long = 0u64;
        let mut stale_short = 0u64;

        for idx in 0..MAX_ACCOUNTS {
            let used = self.is_used(idx);
            let account = &self.accounts[idx];
            if !used {
                if account.kind != Account::KIND_USER
                    || !account.capital.is_zero()
                    || account.pnl != 0
                    || account.reserved_pnl != 0
                    || account.position_basis_q != 0
                    || account.adl_a_basis != ADL_ONE
                    || account.adl_k_snap != 0
                    || account.f_snap != 0
                    || account.adl_epoch_snap != 0
                    || account.fee_credits.get() != 0
                    || account.last_fee_slot != 0
                    || account.sched_present != 0
                    || account.sched_remaining_q != 0
                    || account.sched_anchor_q != 0
                    || account.sched_start_slot != 0
                    || account.sched_horizon != 0
                    || account.sched_release_q != 0
                    || account.pending_present != 0
                    || account.pending_remaining_q != 0
                    || account.pending_horizon != 0
                    || account.pending_created_slot != 0
                    || !Self::is_zero_bytes_32(&account.matcher_program)
                    || !Self::is_zero_bytes_32(&account.matcher_context)
                    || !Self::is_zero_bytes_32(&account.owner)
                {
                    return Err(RiskError::CorruptState);
                }
                continue;
            }

            if idx as u64 >= self.params.max_accounts {
                return Err(RiskError::CorruptState);
            }
            used_count = used_count.checked_add(1).ok_or(RiskError::CorruptState)?;
            if account.kind != Account::KIND_USER && account.kind != Account::KIND_LP {
                return Err(RiskError::CorruptState);
            }
            if account.sched_present > 1 || account.pending_present > 1 {
                return Err(RiskError::CorruptState);
            }
            Self::validate_non_min_i128(account.pnl)?;
            Self::validate_non_min_i128(account.adl_k_snap)?;
            Self::validate_non_min_i128(account.f_snap)?;
            match self.market_mode {
                MarketMode::Live => {
                    if account.last_fee_slot > self.current_slot {
                        return Err(RiskError::CorruptState);
                    }
                }
                MarketMode::Resolved => {
                    if account.last_fee_slot > self.resolved_slot {
                        return Err(RiskError::CorruptState);
                    }
                }
            }
            if account.pnl < 0 {
                neg_count = neg_count.checked_add(1).ok_or(RiskError::CorruptState)?;
            }
            self.validate_fee_credits_shape(idx)?;
            self.validate_reserve_shape(idx)?;

            if account.position_basis_q != 0 {
                if account.position_basis_q.unsigned_abs() > MAX_POSITION_ABS_Q {
                    return Err(RiskError::CorruptState);
                }
                if account.adl_a_basis == 0 {
                    return Err(RiskError::CorruptState);
                }
                match side_of_i128(account.position_basis_q) {
                    Some(Side::Long) => {
                        stored_long = stored_long.checked_add(1).ok_or(RiskError::CorruptState)?;
                        if account.adl_epoch_snap != self.adl_epoch_long {
                            if self.side_mode_long != SideMode::ResetPending
                                || account.adl_epoch_snap.checked_add(1)
                                    != Some(self.adl_epoch_long)
                            {
                                return Err(RiskError::CorruptState);
                            }
                            stale_long =
                                stale_long.checked_add(1).ok_or(RiskError::CorruptState)?;
                        } else if self.notional_checked(idx, self.last_oracle_price, false)?
                            > MAX_ACCOUNT_NOTIONAL
                        {
                            return Err(RiskError::CorruptState);
                        }
                    }
                    Some(Side::Short) => {
                        stored_short =
                            stored_short.checked_add(1).ok_or(RiskError::CorruptState)?;
                        if account.adl_epoch_snap != self.adl_epoch_short {
                            if self.side_mode_short != SideMode::ResetPending
                                || account.adl_epoch_snap.checked_add(1)
                                    != Some(self.adl_epoch_short)
                            {
                                return Err(RiskError::CorruptState);
                            }
                            stale_short =
                                stale_short.checked_add(1).ok_or(RiskError::CorruptState)?;
                        } else if self.notional_checked(idx, self.last_oracle_price, false)?
                            > MAX_ACCOUNT_NOTIONAL
                        {
                            return Err(RiskError::CorruptState);
                        }
                    }
                    None => return Err(RiskError::CorruptState),
                }
            }
        }

        if used_count != self.num_used_accounts as u64
            || used_count != self.materialized_account_count
            || neg_count != self.neg_pnl_account_count
            || stored_long != self.stored_pos_count_long
            || stored_short != self.stored_pos_count_short
            || stale_long != self.stale_account_count_long
            || stale_short != self.stale_account_count_short
        {
            return Err(RiskError::CorruptState);
        }
        Ok(())
    }

    // ========================================================================
    // Warmup Helpers (spec §6)
    // ========================================================================

    /// released_pos (spec §2.1): Live subtracts reserve; Resolved does not.
    test_visible! {
    fn released_pos(&self, idx: usize) -> u128 {
        self.released_pos_checked(idx, false)
            .expect("canonical released PnL state")
    }
    }

    // ========================================================================
    // Two-bucket warmup reserve helpers (spec §4.3)
    // ========================================================================

    fn append_or_route_new_reserve_with_stress(
        &mut self,
        idx: usize,
        reserve_add: u128,
        now_slot: u64,
        h_lock: u64,
        stress_active: bool,
    ) -> Result<()> {
        // Validate existing reserve shape before mutating on top of it.
        // Malformed bucket state must fail rather than be merged through.
        self.validate_reserve_shape(idx)?;

        let a = &mut self.accounts[idx];

        if stress_active {
            if a.pending_present == 0 {
                a.pending_present = 1;
                a.pending_remaining_q = reserve_add;
                a.pending_horizon = h_lock;
                a.pending_created_slot = now_slot;
            } else {
                a.pending_remaining_q = a
                    .pending_remaining_q
                    .checked_add(reserve_add)
                    .ok_or(RiskError::Overflow)?;
                a.pending_horizon = core::cmp::max(a.pending_horizon, h_lock);
            }
            a.reserved_pnl = a
                .reserved_pnl
                .checked_add(reserve_add)
                .ok_or(RiskError::Overflow)?;
            return Ok(());
        }

        // Step 1: if sched absent and pending present → promote pending to scheduled
        if a.sched_present == 0 && a.pending_present != 0 {
            a.sched_present = 1;
            a.sched_remaining_q = a.pending_remaining_q;
            a.sched_anchor_q = a.pending_remaining_q;
            a.sched_start_slot = now_slot;
            a.sched_horizon = a.pending_horizon;
            a.sched_release_q = 0;
            a.pending_present = 0;
            a.pending_remaining_q = 0;
            a.pending_horizon = 0;
            a.pending_created_slot = 0;
        }

        if a.sched_present == 0 {
            // Step 2: sched absent → create scheduled bucket
            a.sched_present = 1;
            a.sched_remaining_q = reserve_add;
            a.sched_anchor_q = reserve_add;
            a.sched_start_slot = now_slot;
            a.sched_horizon = h_lock;
            a.sched_release_q = 0;
        } else if a.sched_present != 0
            && a.pending_present == 0
            && a.sched_start_slot == now_slot
            && a.sched_horizon == h_lock
            && a.sched_release_q == 0
        {
            // Step 3: merge into scheduled (same slot, same horizon, not yet released)
            a.sched_remaining_q = a
                .sched_remaining_q
                .checked_add(reserve_add)
                .ok_or(RiskError::Overflow)?;
            a.sched_anchor_q = a
                .sched_anchor_q
                .checked_add(reserve_add)
                .ok_or(RiskError::Overflow)?;
        } else if a.pending_present == 0 {
            // Step 4: create pending bucket
            a.pending_present = 1;
            a.pending_remaining_q = reserve_add;
            a.pending_horizon = h_lock;
            a.pending_created_slot = now_slot;
        } else {
            // Step 5: merge into pending (horizon = max)
            a.pending_remaining_q = a
                .pending_remaining_q
                .checked_add(reserve_add)
                .ok_or(RiskError::Overflow)?;
            a.pending_horizon = core::cmp::max(a.pending_horizon, h_lock);
        }

        // Step 6: R_i += reserve_add
        a.reserved_pnl = a
            .reserved_pnl
            .checked_add(reserve_add)
            .ok_or(RiskError::Overflow)?;
        Ok(())
    }

    /// append_or_route_new_reserve (spec §4.3). Test/proof-facing helper for
    /// the normal non-stress path; live public flows use the stress-aware
    /// internal variant through admission.
    test_visible! {
    fn append_or_route_new_reserve(
        &mut self,
        idx: usize,
        reserve_add: u128,
        now_slot: u64,
        h_lock: u64,
    ) -> Result<()> {
        self.append_or_route_new_reserve_with_stress(idx, reserve_add, now_slot, h_lock, false)
    }

    }

    /// apply_reserve_loss_newest_first (spec §4.4) — consume from pending first, then scheduled.
    test_visible! {
    fn apply_reserve_loss_newest_first(&mut self, idx: usize, reserve_loss: u128) -> Result<()> {
        // Validate reserve integrity first — a malformed bucket (e.g., sums
        // not matching reserved_pnl, horizons out of bounds, reserved_pnl
        // exceeding positive PnL) must fail rather than be partially
        // consumed and transformed into a different malformed state.
        self.validate_reserve_shape(idx)?;

        // Phase 1: compute per-bucket takes without mutating. Validates
        // feasibility (reserve_loss <= total available, reserve_loss <=
        // reserved_pnl).
        let a = &self.accounts[idx];
        let pend_avail = if a.pending_present != 0 { a.pending_remaining_q } else { 0 };
        let sched_avail = if a.sched_present != 0 { a.sched_remaining_q } else { 0 };
        let total_avail = pend_avail
            .checked_add(sched_avail)
            .ok_or(RiskError::CorruptState)?;
        if reserve_loss > total_avail { return Err(RiskError::CorruptState); }
        // Pre-validate R_i decrement.
        let new_reserved_pnl = a.reserved_pnl
            .checked_sub(reserve_loss)
            .ok_or(RiskError::CorruptState)?;

        // Newest-first order: pending → scheduled.
        let take_pend = core::cmp::min(reserve_loss, pend_avail);
        // Safe: take_pend <= reserve_loss.
        let take_sched = reserve_loss - take_pend;
        // Safe: take_sched = reserve_loss - take_pend <= total_avail - pend_avail = sched_avail.

        // Phase 2: commit.
        let a = &mut self.accounts[idx];
        if take_pend > 0 {
            a.pending_remaining_q -= take_pend;
            if a.pending_remaining_q == 0 {
                a.pending_present = 0;
                a.pending_horizon = 0;
                a.pending_created_slot = 0;
            }
        }
        if take_sched > 0 {
            a.sched_remaining_q -= take_sched;
            if a.sched_remaining_q == 0 {
                a.sched_present = 0;
                a.sched_anchor_q = 0;
                a.sched_start_slot = 0;
                a.sched_horizon = 0;
                a.sched_release_q = 0;
            }
        }
        a.reserved_pnl = new_reserved_pnl;
        Ok(())
    }

    }

    /// prepare_account_for_resolved_touch (spec §4.4.3)
    test_visible! {
    fn prepare_account_for_resolved_touch(&mut self, idx: usize) {
        let a = &mut self.accounts[idx];
        // Always clear bucket metadata even if reserved_pnl == 0.
        a.sched_present = 0;
        a.sched_remaining_q = 0;
        a.sched_anchor_q = 0;
        a.sched_start_slot = 0;
        a.sched_horizon = 0;
        a.sched_release_q = 0;
        a.pending_present = 0;
        a.pending_remaining_q = 0;
        a.pending_horizon = 0;
        a.pending_created_slot = 0;
        a.reserved_pnl = 0;
        // Do NOT mutate PNL_matured_pos_tot (already set globally at resolve time)
    }
    }

    /// Validate reserve-bucket shape consistency.
    /// Absent bucket => all fields zero. Present scheduled => horizon > 0,
    /// release <= anchor, remaining <= anchor - release.
    /// Total: sched_remaining + pending_remaining == reserved_pnl.
    fn validate_fee_credits_shape(&self, idx: usize) -> Result<()> {
        if idx >= MAX_ACCOUNTS {
            return Err(RiskError::AccountNotFound);
        }
        let fc = self.accounts[idx].fee_credits.get();
        if fc > 0 || fc == i128::MIN {
            return Err(RiskError::CorruptState);
        }
        Ok(())
    }

    fn validate_non_min_i128(v: i128) -> Result<()> {
        if v == i128::MIN {
            return Err(RiskError::CorruptState);
        }
        Ok(())
    }

    fn try_into_non_min_i128(x: I256) -> Result<i128> {
        let v = x.try_into_i128().ok_or(RiskError::Overflow)?;
        if v == i128::MIN {
            return Err(RiskError::Overflow);
        }
        Ok(v)
    }

    fn validate_persistent_global_signed_shape(&self) -> Result<()> {
        Self::validate_non_min_i128(self.adl_coeff_long)?;
        Self::validate_non_min_i128(self.adl_coeff_short)?;
        Self::validate_non_min_i128(self.adl_epoch_start_k_long)?;
        Self::validate_non_min_i128(self.adl_epoch_start_k_short)?;
        Self::validate_non_min_i128(self.f_long_num)?;
        Self::validate_non_min_i128(self.f_short_num)?;
        Self::validate_non_min_i128(self.f_epoch_start_long_num)?;
        Self::validate_non_min_i128(self.f_epoch_start_short_num)?;
        Self::validate_non_min_i128(self.resolved_k_long_terminal_delta)?;
        Self::validate_non_min_i128(self.resolved_k_short_terminal_delta)?;
        Ok(())
    }

    fn validate_k_future_headroom(a: u128, k: i128) -> Result<()> {
        if a == 0 || a > ADL_ONE {
            return Err(RiskError::CorruptState);
        }
        let mark = U256::from_u128(a)
            .checked_mul(U256::from_u128(MAX_ORACLE_PRICE as u128))
            .ok_or(RiskError::Overflow)?;
        let used = U256::from_u128(k.unsigned_abs())
            .checked_add(mark)
            .ok_or(RiskError::Overflow)?;
        if used > U256::from_u128(i128::MAX as u128) {
            return Err(RiskError::Overflow);
        }
        Ok(())
    }

    fn validate_f_future_headroom(&self, a: u128, f: i128) -> Result<()> {
        if a == 0 || a > ADL_ONE {
            return Err(RiskError::CorruptState);
        }
        let headroom = U256::from_u128(a)
            .checked_mul(U256::from_u128(MAX_ORACLE_PRICE as u128))
            .and_then(|v| {
                v.checked_mul(U256::from_u128(
                    self.params.max_abs_funding_e9_per_slot as u128,
                ))
            })
            .and_then(|v| v.checked_mul(U256::from_u128(self.params.max_accrual_dt_slots as u128)))
            .ok_or(RiskError::Overflow)?;
        let used = U256::from_u128(f.unsigned_abs())
            .checked_add(headroom)
            .ok_or(RiskError::Overflow)?;
        if used > U256::from_u128(i128::MAX as u128) {
            return Err(RiskError::Overflow);
        }
        Ok(())
    }

    fn validate_live_kf_future_headroom(
        &self,
        k_long: i128,
        k_short: i128,
        f_long: i128,
        f_short: i128,
    ) -> Result<()> {
        Self::validate_k_future_headroom(self.adl_mult_long, k_long)?;
        Self::validate_k_future_headroom(self.adl_mult_short, k_short)?;
        self.validate_f_future_headroom(self.adl_mult_long, f_long)?;
        self.validate_f_future_headroom(self.adl_mult_short, f_short)?;
        Ok(())
    }

    fn validate_used_account_slot(&self, idx: usize) -> Result<()> {
        if idx >= MAX_ACCOUNTS || idx as u64 >= self.params.max_accounts || !self.is_used(idx) {
            return Err(RiskError::AccountNotFound);
        }
        Ok(())
    }

    fn validate_touched_account_shape_at_fee_slot(
        &self,
        idx: usize,
        fee_slot_anchor: u64,
    ) -> Result<()> {
        self.validate_used_account_slot(idx)?;
        let account = &self.accounts[idx];
        if account.kind != Account::KIND_USER && account.kind != Account::KIND_LP {
            return Err(RiskError::CorruptState);
        }
        if account.sched_present > 1 || account.pending_present > 1 {
            return Err(RiskError::CorruptState);
        }
        if account.last_fee_slot > fee_slot_anchor {
            return Err(RiskError::CorruptState);
        }
        Self::validate_non_min_i128(account.pnl)?;
        Self::validate_non_min_i128(account.adl_k_snap)?;
        Self::validate_non_min_i128(account.f_snap)?;
        self.validate_fee_credits_shape(idx)?;
        self.validate_reserve_shape(idx)?;

        if account.position_basis_q != 0 {
            if account.position_basis_q.unsigned_abs() > MAX_POSITION_ABS_Q {
                return Err(RiskError::CorruptState);
            }
            if account.adl_a_basis == 0 {
                return Err(RiskError::CorruptState);
            }
            let side = side_of_i128(account.position_basis_q).ok_or(RiskError::CorruptState)?;
            let epoch_snap = account.adl_epoch_snap;
            let epoch_side = self.get_epoch_side(side);
            if epoch_snap != epoch_side {
                if self.get_side_mode(side) != SideMode::ResetPending
                    || epoch_snap.checked_add(1) != Some(epoch_side)
                {
                    return Err(RiskError::CorruptState);
                }
            }
        }
        Ok(())
    }

    fn validate_touched_account_shape(&self, idx: usize) -> Result<()> {
        let fee_slot_anchor = match self.market_mode {
            MarketMode::Live => self.current_slot,
            MarketMode::Resolved => self.resolved_slot,
        };
        self.validate_touched_account_shape_at_fee_slot(idx, fee_slot_anchor)
    }

    fn validate_reserve_shape(&self, idx: usize) -> Result<()> {
        let a = &self.accounts[idx];
        if a.sched_present == 0 {
            if a.sched_remaining_q != 0
                || a.sched_anchor_q != 0
                || a.sched_start_slot != 0
                || a.sched_horizon != 0
                || a.sched_release_q != 0
            {
                return Err(RiskError::CorruptState);
            }
        } else {
            // Spec §4.4/§1.4: sched_horizon in [cfg_h_min, cfg_h_max] when present.
            if a.sched_horizon == 0 {
                return Err(RiskError::CorruptState);
            }
            if a.sched_horizon < self.params.h_min {
                return Err(RiskError::CorruptState);
            }
            if a.sched_horizon > self.params.h_max {
                return Err(RiskError::CorruptState);
            }
            if a.sched_release_q > a.sched_anchor_q {
                return Err(RiskError::CorruptState);
            }
            let used = a
                .sched_remaining_q
                .checked_add(a.sched_release_q)
                .ok_or(RiskError::CorruptState)?;
            if used > a.sched_anchor_q {
                return Err(RiskError::CorruptState);
            }
        }
        if a.sched_present != 0 && a.sched_remaining_q == 0 {
            return Err(RiskError::CorruptState);
        }
        if a.pending_present == 0 {
            if a.pending_remaining_q != 0 || a.pending_horizon != 0 || a.pending_created_slot != 0 {
                return Err(RiskError::CorruptState);
            }
        } else {
            // Spec §4.4/§1.4: pending_horizon in [cfg_h_min, cfg_h_max]
            if a.pending_horizon == 0 {
                return Err(RiskError::CorruptState);
            }
            if a.pending_horizon < self.params.h_min {
                return Err(RiskError::CorruptState);
            }
            if a.pending_horizon > self.params.h_max {
                return Err(RiskError::CorruptState);
            }
            if a.pending_remaining_q == 0 {
                return Err(RiskError::CorruptState);
            }
        }
        let sched_r = if a.sched_present != 0 {
            a.sched_remaining_q
        } else {
            0
        };
        let pend_r = if a.pending_present != 0 {
            a.pending_remaining_q
        } else {
            0
        };
        let total = sched_r.checked_add(pend_r).ok_or(RiskError::CorruptState)?;
        if total != a.reserved_pnl {
            return Err(RiskError::CorruptState);
        }

        // Spec §2.1: R_i <= max(PNL_i, 0). Without this, a corrupt account
        // with reserved_pnl > max(pnl, 0) would pass shape validation and
        // subsequent helpers (apply_reserve_loss, admit_outstanding) would
        // mutate on top of an invalid state.
        let pos_pnl: u128 = if a.pnl > 0 { a.pnl as u128 } else { 0 };
        if a.reserved_pnl > pos_pnl {
            return Err(RiskError::CorruptState);
        }

        Ok(())
    }

    /// advance_profit_warmup (spec §4.8, two-bucket)
    /// Releases reserve from the scheduled bucket per linear maturity.
    fn advance_profit_warmup_with_context(
        &mut self,
        idx: usize,
        ctx: &mut InstructionContext,
    ) -> Result<()> {
        // Validate reserve integrity before the pending-to-scheduled promotion.
        self.validate_reserve_shape(idx)?;
        if self.stress_gate_active(ctx) {
            return Ok(());
        }

        let r = self.accounts[idx].reserved_pnl;
        if r == 0 {
            return Ok(());
        }

        // Step 2: if sched absent and pending present → promote
        if self.accounts[idx].sched_present == 0 && self.accounts[idx].pending_present != 0 {
            ctx.mark_positive_pnl_usability();
            let a = &mut self.accounts[idx];
            a.sched_present = 1;
            a.sched_remaining_q = a.pending_remaining_q;
            a.sched_anchor_q = a.pending_remaining_q;
            a.sched_start_slot = self.current_slot;
            a.sched_horizon = a.pending_horizon;
            a.sched_release_q = 0;
            a.pending_present = 0;
            a.pending_remaining_q = 0;
            a.pending_horizon = 0;
            a.pending_created_slot = 0;
        }

        // If sched absent but R > 0 with no pending either -> corrupt
        if self.accounts[idx].sched_present == 0 {
            return Err(RiskError::CorruptState);
        }

        // Step 4: elapsed = current_slot - sched_start_slot
        if self.current_slot < self.accounts[idx].sched_start_slot {
            return Err(RiskError::CorruptState);
        }
        let elapsed = (self.current_slot - self.accounts[idx].sched_start_slot) as u128;

        // Step 5: sched_total = min(anchor, floor(anchor * elapsed / horizon))
        let a = &mut self.accounts[idx];
        if a.sched_horizon == 0 {
            return Err(RiskError::CorruptState);
        }
        let sched_total = if elapsed >= a.sched_horizon as u128 {
            a.sched_anchor_q
        } else {
            mul_div_floor_u128(a.sched_anchor_q, elapsed, a.sched_horizon as u128)
        };

        // Step 6: require sched_total >= sched_release_q
        if sched_total < a.sched_release_q {
            return Err(RiskError::CorruptState);
        }

        // Step 7: sched_increment
        let sched_increment = sched_total - a.sched_release_q;

        // Step 8: release = min(remaining, increment)
        let release = core::cmp::min(a.sched_remaining_q, sched_increment);

        // Step 9: if release > 0
        if release > 0 {
            ctx.mark_positive_pnl_usability();
            a.sched_remaining_q = a
                .sched_remaining_q
                .checked_sub(release)
                .ok_or(RiskError::CorruptState)?;
            a.reserved_pnl = a
                .reserved_pnl
                .checked_sub(release)
                .ok_or(RiskError::CorruptState)?;
            self.pnl_matured_pos_tot = self
                .pnl_matured_pos_tot
                .checked_add(release)
                .ok_or(RiskError::Overflow)?;
        }

        // Step 10: sched_release_q = sched_total
        self.accounts[idx].sched_release_q = sched_total;

        // Step 11: if scheduled empty → clear, promote pending if present
        if self.accounts[idx].sched_remaining_q == 0 {
            self.accounts[idx].sched_present = 0;
            self.accounts[idx].sched_anchor_q = 0;
            self.accounts[idx].sched_start_slot = 0;
            self.accounts[idx].sched_horizon = 0;
            self.accounts[idx].sched_release_q = 0;

            // Promote pending if present
            if self.accounts[idx].pending_present != 0 {
                let a = &mut self.accounts[idx];
                a.sched_present = 1;
                a.sched_remaining_q = a.pending_remaining_q;
                a.sched_anchor_q = a.pending_remaining_q;
                a.sched_start_slot = self.current_slot;
                a.sched_horizon = a.pending_horizon;
                a.sched_release_q = 0;
                a.pending_present = 0;
                a.pending_remaining_q = 0;
                a.pending_horizon = 0;
                a.pending_created_slot = 0;
            }
        }

        // Step 12: if R_i == 0 → require both absent
        if self.accounts[idx].reserved_pnl == 0 {
            if self.accounts[idx].sched_present != 0 || self.accounts[idx].pending_present != 0 {
                return Err(RiskError::CorruptState);
            }
        }

        if self.pnl_matured_pos_tot > self.pnl_pos_tot {
            return Err(RiskError::CorruptState);
        }
        Ok(())
    }

    test_visible! {
    fn advance_profit_warmup(&mut self, idx: usize) -> Result<()> {
        let mut ctx = InstructionContext::new();
        self.advance_profit_warmup_with_context(idx, &mut ctx)
    }
    }

    // ========================================================================
    // Loss settlement and profit conversion (spec §7)
    // ========================================================================

    /// settle_losses (spec §7.1): settle negative PnL from principal
    fn trigger_bankruptcy_hmax_lock_without_context(&mut self) {
        self.bankruptcy_hmax_lock_active = true;
        self.stress_envelope_remaining_indices = self.params.max_accounts;
        self.stress_envelope_start_slot = self.current_slot;
        self.stress_envelope_start_generation = self.sweep_generation;
    }

    fn settle_losses_with_context(
        &mut self,
        idx: usize,
        ctx: Option<&mut InstructionContext>,
    ) -> Result<()> {
        let pnl = self.accounts[idx].pnl;
        if pnl >= 0 {
            return Ok(());
        }
        if pnl == i128::MIN {
            return Err(RiskError::CorruptState);
        }
        let need = pnl.unsigned_abs();
        let cap = self.accounts[idx].capital.get();
        let pay = core::cmp::min(need, cap);
        if pay > 0 {
            self.set_capital(idx, cap - pay)?;
            let pay_i128 = pay as i128; // pay <= need = |pnl| <= i128::MAX, safe
            let new_pnl = pnl.checked_add(pay_i128).ok_or(RiskError::CorruptState)?;
            if new_pnl == i128::MIN {
                return Err(RiskError::CorruptState);
            }
            self.set_pnl(idx, new_pnl)?;
        }
        if self.market_mode == MarketMode::Live
            && self.accounts[idx].pnl < 0
            && self.accounts[idx].capital.get() == 0
        {
            if let Some(ctx) = ctx {
                self.trigger_bankruptcy_hmax_lock(ctx)?;
            } else {
                self.trigger_bankruptcy_hmax_lock_without_context();
            }
        }
        Ok(())
    }

    fn settle_losses(&mut self, idx: usize) -> Result<()> {
        self.settle_losses_with_context(idx, None)
    }

    /// resolve_flat_negative (spec §7.3): for flat accounts with negative PnL
    fn resolve_flat_negative_with_context(
        &mut self,
        idx: usize,
        ctx: Option<&mut InstructionContext>,
    ) -> Result<()> {
        let eff = self.effective_pos_q_checked(idx, false)?;
        if eff != 0 {
            return Ok(()); // Not flat
        }
        let pnl = self.accounts[idx].pnl;
        if pnl < 0 {
            if pnl == i128::MIN {
                return Err(RiskError::CorruptState);
            }
            let loss = pnl.unsigned_abs();
            if self.market_mode == MarketMode::Live {
                if let Some(ctx) = ctx {
                    self.trigger_bankruptcy_hmax_lock(ctx)?;
                } else {
                    self.trigger_bankruptcy_hmax_lock_without_context();
                }
            }
            self.absorb_protocol_loss(loss);
            self.set_pnl_with_reserve(idx, 0i128, ReserveMode::NoPositiveIncreaseAllowed, None)?;
        }
        Ok(())
    }

    fn resolve_flat_negative(&mut self, idx: usize) -> Result<()> {
        self.resolve_flat_negative_with_context(idx, None)
    }

    /// fee_debt_sweep (spec §7.5): after any capital increase, sweep fee debt
    test_visible! {
    fn fee_debt_sweep(&mut self, idx: usize) -> Result<()> {
        self.validate_touched_account_shape(idx)?;
        let fc = self.accounts[idx].fee_credits.get();
        let debt = fee_debt_u128_checked(fc);
        if debt == 0 {
            return Ok(());
        }
        let cap = self.accounts[idx].capital.get();
        let pay = core::cmp::min(debt, cap);
        if pay > 0 {
            self.set_capital(idx, cap - pay)?;
            // pay <= debt = |fee_credits|, so fee_credits + pay <= 0: no overflow
            let pay_i128 = core::cmp::min(pay, i128::MAX as u128) as i128;
            self.accounts[idx].fee_credits = I128::new(self.accounts[idx].fee_credits.get()
                .checked_add(pay_i128).ok_or(RiskError::CorruptState)?);
            self.insurance_fund.balance = U128::new(
                self.insurance_fund.balance.get().checked_add(pay)
                    .ok_or(RiskError::Overflow)?);
        }
        // Per spec §7.5: unpaid fee debt remains as local fee_credits until
        // physical capital becomes available or manual profit conversion occurs.
        // MUST NOT consume junior PnL claims to mint senior insurance capital.
        Ok(())
    }
    }

    // ========================================================================
    // touch_account_live_local (spec §7.7)
    // ========================================================================

    /// Live local touch: advance warmup, settle side effects, settle losses.
    /// Does NOT auto-convert, does NOT fee-sweep. Those happen in finalize.
    test_visible! {
    fn touch_account_live_local(&mut self, idx: usize, ctx: &mut InstructionContext) -> Result<()> {
        if self.market_mode != MarketMode::Live { return Err(RiskError::Unauthorized); }
        self.validate_touched_account_shape(idx)?;
        if !ctx.add_touched(idx as u16) {
            return Err(RiskError::Overflow); // finalized context or touched-set capacity exceeded
        }

        // Step 4: accelerate outstanding reserve if h=1 admits (spec §4.9)
        self.admit_outstanding_reserve_on_touch(idx, ctx)?;

        // Step 5: advance cohort-based warmup
        self.advance_profit_warmup_with_context(idx, ctx)?;

        // Step 5: settle side effects with H_lock for reserve routing
        self.settle_side_effects_live(idx, ctx)?;

        // Step 6: settle losses from principal
        self.settle_losses_with_context(idx, Some(ctx))?;

        // Step 7: resolve flat negative
        if self.effective_pos_q_checked(idx, false)? == 0 && self.accounts[idx].pnl < 0 {
            self.resolve_flat_negative_with_context(idx, Some(ctx))?;
        }

        // Steps 8-9: MUST NOT auto-convert, MUST NOT fee-sweep
        Ok(())
    }

    }

    /// finalize_touched_accounts_post_live (spec §7.8).
    /// Whole-only conversion + fee sweep with shared snapshot.
    test_visible! {
    fn finalize_touched_account_post_live_with_snapshot(
        &mut self,
        idx: usize,
        is_whole: bool,
        stress_active: bool,
        ctx: &mut InstructionContext,
    ) -> Result<()> {
        // Whole-only flat auto-conversion
        if !stress_active
            && is_whole
            && self.accounts[idx].position_basis_q == 0
            && self.accounts[idx].pnl > 0
        {
            let released = self.released_pos_checked(idx, false)?;
            if released > 0 {
                ctx.mark_positive_pnl_usability();
                self.consume_released_pnl(idx, released)?;
                let new_cap = self.accounts[idx].capital.get()
                    .checked_add(released).ok_or(RiskError::Overflow)?;
                self.set_capital(idx, new_cap)?;
            }
        }

        // Fee-debt sweep
        self.fee_debt_sweep(idx)?;
        Ok(())
    }
    }

    test_visible! {
    fn finalize_touched_accounts_post_live(&mut self, ctx: &mut InstructionContext) -> Result<()> {
        if ctx.finalized {
            return Err(RiskError::CorruptState);
        }
        let stress_active = self.stress_gate_active(ctx);
        // Step 1: compute shared snapshot
        let senior_sum = self.c_tot.get().checked_add(
            self.insurance_fund.balance.get()).unwrap_or(u128::MAX);
        let residual = if self.vault.get() >= senior_sum {
            self.vault.get() - senior_sum
        } else { 0u128 };
        let h_snapshot_den = self.pnl_matured_pos_tot;
        let h_snapshot_num = if h_snapshot_den == 0 { 0 } else {
            core::cmp::min(residual, h_snapshot_den)
        };
        let is_whole = h_snapshot_den > 0 && h_snapshot_num == h_snapshot_den;
        ctx.finalized = true;

        // Step 2: iterate touched accounts in ascending order.
        // `add_touched` preserves order, so no sort pass is required.
        let count = ctx.touched_count as usize;
        for ti in 0..count {
            let idx = ctx.touched_accounts[ti] as usize;
            self.finalize_touched_account_post_live_with_snapshot(idx, is_whole, stress_active, ctx)?;
        }
        Ok(())
    }

    }

    // ========================================================================
    // Account Management
    // ========================================================================

    pub fn set_owner(&mut self, idx: u16, owner: [u8; 32]) -> Result<()> {
        if self.validate_used_account_slot(idx as usize).is_err() {
            return Err(RiskError::Unauthorized);
        }
        // Preserve the "owner is claimed iff nonzero" convention.
        // Rejecting zero here means set_owner cannot silently un-claim an
        // account and callers cannot land the slot in an ambiguous state.
        if owner == [0u8; 32] {
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
    // deposit (spec §9.2)
    // ========================================================================

    /// Spec §9.2: `deposit(i, amount, now_slot)`. Pure capital-transfer path;
    /// does not call `accrue_market_to` and therefore takes no oracle input.
    pub fn deposit_not_atomic(&mut self, idx: u16, amount: u128, now_slot: u64) -> Result<()> {
        rollback_on_error!(self, [idx], {
            if self.market_mode != MarketMode::Live {
                return Err(RiskError::Unauthorized);
            }
            // Time monotonicity (spec §10.3 step 1)
            if now_slot < self.current_slot {
                return Err(RiskError::Overflow);
            }
            if now_slot < self.last_market_slot {
                return Err(RiskError::Overflow);
            }
            // deposit_not_atomic advances current_slot without calling
            // accrue_market_to; enforce the live accrual envelope so a
            // zero-amount (or any) deposit cannot brick subsequent accrual.
            self.check_live_accrual_envelope(now_slot)?;

            // Pre-validate vault capacity before any mutations (prevents ghost account)
            let v_candidate = self
                .vault
                .get()
                .checked_add(amount)
                .ok_or(RiskError::Overflow)?;
            if v_candidate > MAX_VAULT_TVL {
                return Err(RiskError::Overflow);
            }

            // Step 2: spec §10.2 — deposit is the canonical materialization path.
            // The engine only requires amount > 0 for a fresh materialization
            // (no zero-capital ghost accounts). Any higher minimum-deposit
            // threshold is wrapper policy: the wrapper is expected to enforce
            // a deployment-chosen floor for anti-spam, paired with recurring
            // maintenance fees (§7.3) to keep the materialized-account set
            // bounded.
            let capital_amount = amount;
            if !self.is_used(idx as usize) {
                if amount == 0 {
                    return Err(RiskError::InsufficientBalance);
                }
                self.materialize_at(idx, now_slot)?;
            }
            let i = idx as usize;
            self.validate_touched_account_shape_at_fee_slot(i, now_slot)?;

            // Pre-validate: settle_losses can only fail on i128::MIN PNL (corruption).
            // Check before any mutation to maintain validate-then-mutate contract.
            if self.accounts[i].pnl == i128::MIN {
                return Err(RiskError::CorruptState);
            }

            // Step 3: current_slot = now_slot
            self.current_slot = now_slot;
            self.vault = U128::new(v_candidate);

            // Step 6: set_capital(i, C_i + capital_amount)
            let new_cap = self.accounts[i]
                .capital
                .get()
                .checked_add(capital_amount)
                .ok_or(RiskError::Overflow)?;
            self.set_capital(i, new_cap)?;

            // Step 7: settle_losses_from_principal
            self.settle_losses(i)?;

            // Step 8: deposit MUST NOT invoke resolve_flat_negative (spec §7.3).
            // A pure deposit path that does not call accrue_market_to MUST NOT
            // invoke this path — surviving flat negative PNL waits for a later
            // accrued touch.

            // Step 9: if flat and PNL >= 0, sweep fee debt (spec §7.5)
            // Per spec §10.3: deposit into account with basis != 0 MUST defer.
            // Per spec §7.5: only a surviving negative PNL_i blocks the sweep.
            if self.accounts[i].position_basis_q == 0 && self.accounts[i].pnl >= 0 {
                self.fee_debt_sweep(i)?;
            }

            self.assert_public_postconditions()?;
            Ok(())
        })
    }

    // ========================================================================
    // withdraw_not_atomic (spec §10.3)
    // ========================================================================

    test_visible! {
    fn commit_withdrawal(&mut self, idx: usize, amount: u128) -> Result<()> {
        if self.accounts[idx].capital.get() < amount {
            return Err(RiskError::InsufficientBalance);
        }
        self.set_capital(idx, self.accounts[idx].capital.get() - amount)?;
        self.vault = U128::new(self.vault.get().checked_sub(amount).ok_or(RiskError::CorruptState)?);
        Ok(())
    }
    }

    pub fn withdraw_not_atomic(
        &mut self,
        idx: u16,
        amount: u128,
        oracle_price: u64,
        now_slot: u64,
        funding_rate_e9: i128,
        admit_h_min: u64,
        admit_h_max: u64,
        admit_h_max_consumption_threshold_bps_opt: Option<u128>,
    ) -> Result<()> {
        rollback_on_error!(self, [idx], {
            Self::validate_admission_pair(admit_h_min, admit_h_max, &self.params)?;
            Self::validate_threshold_opt(admit_h_max_consumption_threshold_bps_opt)?;

            if oracle_price == 0 || oracle_price > MAX_ORACLE_PRICE {
                return Err(RiskError::Overflow);
            }

            self.validate_touched_account_shape(idx as usize)?;

            if self.market_mode != MarketMode::Live {
                return Err(RiskError::Unauthorized);
            }

            let mut ctx = InstructionContext::new_with_admission_and_threshold(
                admit_h_min,
                admit_h_max,
                admit_h_max_consumption_threshold_bps_opt,
            );

            if self.loss_stale_positive_pnl_lock_active()
                && self.accounts[idx as usize].position_basis_q != 0
            {
                return Err(RiskError::Undercollateralized);
            }

            // Step 2: accrue market
            self.accrue_market_to(now_slot, oracle_price, funding_rate_e9)?;
            self.current_slot = now_slot;
            if self.loss_stale_positive_pnl_lock_active()
                && self.accounts[idx as usize].position_basis_q != 0
            {
                return Err(RiskError::Undercollateralized);
            }

            // Step 3: live local touch
            self.touch_account_live_local(idx as usize, &mut ctx)?;
            let stress_active = self.stress_gate_active(&ctx);

            // Finalize touched (whole-only conversion + fee sweep)
            self.finalize_touched_accounts_post_live(&mut ctx)?;

            // Step 4: require amount <= C_i
            if self.accounts[idx as usize].capital.get() < amount {
                return Err(RiskError::InsufficientBalance);
            }

            // Step 5: the engine allows any partial withdraw that leaves
            // `capital - amount >= 0`. A post-withdraw "dust floor" (capital
            // must be 0 or >= some threshold) is wrapper policy — enforce it
            // at the wrapper layer if your deployment doesn't want tiny
            // residuals lingering in account slots.

            // Step 6: if position exists, require post-withdrawal margin using
            // `Eq_withdraw_raw` (spec §3.5) — capital + min(PNL, 0) + haircutted
            // matured PnL - fee debt. Haircutting by the current `h` ratio
            // prevents approval against matured claims that would be diluted
            // by other accounts' conversions.
            let eff = self.effective_pos_q_checked(idx as usize, false)?;
            if eff != 0 {
                // Post-withdrawal equity: current withdraw equity minus withdrawal amount
                let eq_withdraw = if stress_active {
                    self.account_equity_withdraw_no_pos_raw(&self.accounts[idx as usize])
                } else {
                    self.account_equity_withdraw_raw(&self.accounts[idx as usize], idx as usize)
                };
                let notional = self.notional_checked(idx as usize, oracle_price, false)?;
                // eff != 0 here, so always enforce min_nonzero_im_req. The
                // risk notional itself is ceil-rounded, but proportional IM can
                // still floor to 0 for microscopic positions.
                let im_req = core::cmp::max(
                    mul_div_floor_u128(notional, self.params.initial_margin_bps as u128, 10_000),
                    self.params.min_nonzero_im_req,
                );
                // Spec §8.1: Eq_withdraw_raw_i >= IM_req_i. Compare in wide
                // signed domain — a bare `im_req as i128` can wrap when
                // im_req > i128::MAX (min_nonzero_im_req is u128; spec §1.4
                // does not clip it to i128 range), approving an otherwise
                // undercollateralized withdrawal. Use I256 to avoid the wrap.
                let eq_post_wide = I256::from_i128(eq_withdraw)
                    .checked_sub(I256::from_u128(amount))
                    .ok_or(RiskError::Overflow)?;
                let im_req_wide = I256::from_u128(im_req);
                if eq_post_wide < im_req_wide {
                    return Err(RiskError::Undercollateralized);
                }
            }

            // Step 7: commit withdrawal
            self.commit_withdrawal(idx as usize, amount)?;

            // Steps 8-9: end-of-instruction resets
            self.schedule_end_of_instruction_resets(&mut ctx)?;
            self.finalize_end_of_instruction_resets(&ctx)?;

            self.assert_public_postconditions()?;
            Ok(())
        })
    }

    // ========================================================================
    // settle_account_not_atomic (spec §10.7)
    // ========================================================================

    /// Top-level settle wrapper per spec §10.7.
    pub fn settle_account_not_atomic(
        &mut self,
        idx: u16,
        oracle_price: u64,
        now_slot: u64,
        funding_rate_e9: i128,
        admit_h_min: u64,
        admit_h_max: u64,
        admit_h_max_consumption_threshold_bps_opt: Option<u128>,
    ) -> Result<()> {
        rollback_on_error!(self, [idx], {
            Self::validate_admission_pair(admit_h_min, admit_h_max, &self.params)?;
            Self::validate_threshold_opt(admit_h_max_consumption_threshold_bps_opt)?;

            if oracle_price == 0 || oracle_price > MAX_ORACLE_PRICE {
                return Err(RiskError::Overflow);
            }
            self.validate_touched_account_shape(idx as usize)?;

            if self.market_mode != MarketMode::Live {
                return Err(RiskError::Unauthorized);
            }

            let mut ctx = InstructionContext::new_with_admission_and_threshold(
                admit_h_min,
                admit_h_max,
                admit_h_max_consumption_threshold_bps_opt,
            );

            // Step 2: accrue market
            self.accrue_market_to(now_slot, oracle_price, funding_rate_e9)?;
            self.current_slot = now_slot;

            // Step 3: live local touch (no auto-convert, no fee-sweep)
            self.touch_account_live_local(idx as usize, &mut ctx)?;

            // Step 4: finalize (shared snapshot, whole-only conversion, fee-sweep)
            self.finalize_touched_accounts_post_live(&mut ctx)?;

            // Steps 5-6: end-of-instruction resets
            self.schedule_end_of_instruction_resets(&mut ctx)?;
            self.finalize_end_of_instruction_resets(&ctx)?;

            self.assert_public_postconditions()?;
            Ok(())
        })
    }

    // ========================================================================
    // execute_trade_not_atomic (spec §10.4)
    // ========================================================================

    test_visible! {
    fn validate_execute_trade_entry(
        &self,
        a: u16,
        b: u16,
        oracle_price: u64,
        now_slot: u64,
        size_q: i128,
        exec_price: u64,
        admit_h_min: u64,
        admit_h_max: u64,
        admit_h_max_consumption_threshold_bps_opt: Option<u128>,
    ) -> Result<()> {
        Self::validate_admission_pair(admit_h_min, admit_h_max, &self.params)?;
        Self::validate_threshold_opt(admit_h_max_consumption_threshold_bps_opt)?;

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

        // execute_trade_not_atomic accrues market state directly.
        self.validate_used_account_slot(a as usize)?;
        self.validate_used_account_slot(b as usize)?;
        if a == b {
            return Err(RiskError::Overflow);
        }

        if self.market_mode != MarketMode::Live {
            return Err(RiskError::Unauthorized);
        }

        if now_slot < self.current_slot {
            return Err(RiskError::Overflow);
        }
        if now_slot < self.last_market_slot {
            return Err(RiskError::Overflow);
        }
        Ok(())
    }
    }

    pub fn execute_trade_not_atomic(
        &mut self,
        a: u16,
        b: u16,
        oracle_price: u64,
        now_slot: u64,
        size_q: i128,
        exec_price: u64,
        funding_rate_e9: i128,
        admit_h_min: u64,
        admit_h_max: u64,
        admit_h_max_consumption_threshold_bps_opt: Option<u128>,
    ) -> Result<()> {
        rollback_on_error!(self, [a, b], {
            self.validate_execute_trade_entry(
                a,
                b,
                oracle_price,
                now_slot,
                size_q,
                exec_price,
                admit_h_min,
                admit_h_max,
                admit_h_max_consumption_threshold_bps_opt,
            )?;
            self.validate_touched_account_shape(a as usize)?;
            self.validate_touched_account_shape(b as usize)?;
            let mut ctx = InstructionContext::new_with_admission_and_threshold(
                admit_h_min,
                admit_h_max,
                admit_h_max_consumption_threshold_bps_opt,
            );

            self.ensure_loss_current_for_position_change()?;

            // Step 10: accrue market once
            self.accrue_market_to(now_slot, oracle_price, funding_rate_e9)?;
            self.current_slot = now_slot;
            self.ensure_loss_current_for_position_change()?;

            // Steps 11-12 (spec §9.4 v12.19.53): live local touch both counterparties
            // in deterministic ascending storage-index order. One touch may change
            // PNL_matured_pos_tot and therefore the second account's admission
            // outcome; cross-client order differences are forbidden.
            // Property #108: touch(min(a,b)) first, then touch(max(a,b)).
            let (first, second) = if a <= b { (a, b) } else { (b, a) };
            self.touch_account_live_local(first as usize, &mut ctx)?;
            self.touch_account_live_local(second as usize, &mut ctx)?;

            // Step 12a: flush dust-only empty sides before computing
            // bilateral_oi_after. A touch that hits the "q_eff_new == 0" dust
            // branch zeros the account basis and decrements stored_pos_count
            // but leaves oi_eff_side pointing at the pre-cleanup dust value; cleanup
            // relies on the end-of-instruction bilateral-empty-dust branch.
            // If the trade attaches fresh OI before that cleanup runs,
            // stored_pos_count becomes nonzero again, the cleanup branch no
            // longer fires, and the stale dust permanently inflates OI. A
            // dedicated reset_ctx keeps the trade's pending_reset flags from
            // re-resetting the freshly opened positions at end of instruction.
            {
                let mut reset_ctx = InstructionContext::new();
                self.schedule_end_of_instruction_resets(&mut reset_ctx)?;
                self.finalize_end_of_instruction_resets(&reset_ctx)?;
            }
            // After the flush, any real remaining stale/drain state is still
            // reflected in side_mode_*; the existing ResetPending/DrainOnly
            // OI-increase gate below will reject if a trade would grow a
            // side that is still mid-reset.

            // Step 13: capture old effective positions
            let old_eff_a = self.effective_pos_q_checked(a as usize, false)?;
            let old_eff_b = self.effective_pos_q_checked(b as usize, false)?;

            // Steps 14-16: capture pre-trade MM requirements and raw maintenance buffers
            // Spec §9.1: if effective_pos_q(i) == 0, MM_req_i = 0
            let mm_req_pre_a = if old_eff_a == 0 {
                0u128
            } else {
                let not = self.notional_checked(a as usize, oracle_price, false)?;
                core::cmp::max(
                    mul_div_floor_u128(not, self.params.maintenance_margin_bps as u128, 10_000),
                    self.params.min_nonzero_mm_req,
                )
            };
            let mm_req_pre_b = if old_eff_b == 0 {
                0u128
            } else {
                let not = self.notional_checked(b as usize, oracle_price, false)?;
                core::cmp::max(
                    mul_div_floor_u128(not, self.params.maintenance_margin_bps as u128, 10_000),
                    self.params.min_nonzero_mm_req,
                )
            };
            let maint_raw_wide_pre_a =
                self.account_equity_maint_raw_wide(&self.accounts[a as usize]);
            let maint_raw_wide_pre_b =
                self.account_equity_maint_raw_wide(&self.accounts[b as usize]);
            let buffer_pre_a = maint_raw_wide_pre_a
                .checked_sub(I256::from_u128(mm_req_pre_a))
                .expect("I256 sub");
            let buffer_pre_b = maint_raw_wide_pre_b
                .checked_sub(I256::from_u128(mm_req_pre_b))
                .expect("I256 sub");

            // Step 6: compute new effective positions
            let new_eff_a = old_eff_a.checked_add(size_q).ok_or(RiskError::Overflow)?;
            let neg_size_q = size_q.checked_neg().ok_or(RiskError::Overflow)?;
            let new_eff_b = old_eff_b
                .checked_add(neg_size_q)
                .ok_or(RiskError::Overflow)?;

            // Validate position bounds
            if new_eff_a != 0 && new_eff_a.unsigned_abs() > MAX_POSITION_ABS_Q {
                return Err(RiskError::Overflow);
            }
            if new_eff_b != 0 && new_eff_b.unsigned_abs() > MAX_POSITION_ABS_Q {
                return Err(RiskError::Overflow);
            }

            // Validate notional bounds
            {
                let notional_a = Self::risk_notional_from_eff_q(new_eff_a, oracle_price);
                if notional_a > MAX_ACCOUNT_NOTIONAL {
                    return Err(RiskError::Overflow);
                }
                let notional_b = Self::risk_notional_from_eff_q(new_eff_b, oracle_price);
                if notional_b > MAX_ACCOUNT_NOTIONAL {
                    return Err(RiskError::Overflow);
                }
            }

            // Preflight: finalize any ResetPending sides that are fully ready,
            // so OI-increase gating doesn't block trades on reopenable sides.
            self.maybe_finalize_ready_reset_sides();

            // Step 5: compute bilateral OI once (spec §5.2.2) and use for both
            // mode gating and later writeback. Avoids redundant checked arithmetic.
            let (oi_long_after, oi_short_after) =
                self.bilateral_oi_after(&old_eff_a, &new_eff_a, &old_eff_b, &new_eff_b)?;

            // Validate OI bounds
            if oi_long_after > MAX_OI_SIDE_Q || oi_short_after > MAX_OI_SIDE_Q {
                return Err(RiskError::Overflow);
            }

            // Reject if trade would increase OI on a blocked side.
            self.enforce_side_mode_oi_gate(
                old_eff_a,
                new_eff_a,
                old_eff_b,
                new_eff_b,
                oi_long_after,
                oi_short_after,
            )?;

            // Spec §1.4: per-side active-position cap. Pre-validate the NET
            // delta across BOTH legs so a valid position swap at the cap
            // (taker replacing maker on the same side) is not false-rejected
            // by a transient per-account spike during the attach pair.
            {
                let long_before = (side_of_i128(old_eff_a) == Some(Side::Long)) as i64
                    + (side_of_i128(old_eff_b) == Some(Side::Long)) as i64;
                let long_after = (side_of_i128(new_eff_a) == Some(Side::Long)) as i64
                    + (side_of_i128(new_eff_b) == Some(Side::Long)) as i64;
                let short_before = (side_of_i128(old_eff_a) == Some(Side::Short)) as i64
                    + (side_of_i128(old_eff_b) == Some(Side::Short)) as i64;
                let short_after = (side_of_i128(new_eff_a) == Some(Side::Short)) as i64
                    + (side_of_i128(new_eff_b) == Some(Side::Short)) as i64;
                let final_long = (self.stored_pos_count_long as i64) + long_after - long_before;
                let final_short = (self.stored_pos_count_short as i64) + short_after - short_before;
                if final_long < 0 || final_short < 0 {
                    return Err(RiskError::CorruptState);
                }
                let cap = self.params.max_active_positions_per_side as i64;
                if final_long > cap || final_short > cap {
                    return Err(RiskError::Overflow);
                }
            }

            // Step 21: trade PnL alignment (spec §10.5)
            let price_diff = (oracle_price as i128) - (exec_price as i128);
            let trade_pnl_a = compute_trade_pnl(size_q, price_diff)?;
            let trade_pnl_b = trade_pnl_a.checked_neg().ok_or(RiskError::Overflow)?;

            let pnl_a = self.accounts[a as usize]
                .pnl
                .checked_add(trade_pnl_a)
                .ok_or(RiskError::Overflow)?;
            if pnl_a == i128::MIN {
                return Err(RiskError::Overflow);
            }
            self.set_pnl_with_reserve(
                a as usize,
                pnl_a,
                ReserveMode::UseAdmissionPair(ctx.admit_h_min_shared, ctx.admit_h_max_shared),
                Some(&mut ctx),
            )?;

            let pnl_b = self.accounts[b as usize]
                .pnl
                .checked_add(trade_pnl_b)
                .ok_or(RiskError::Overflow)?;
            if pnl_b == i128::MIN {
                return Err(RiskError::Overflow);
            }
            self.set_pnl_with_reserve(
                b as usize,
                pnl_b,
                ReserveMode::UseAdmissionPair(ctx.admit_h_min_shared, ctx.admit_h_max_shared),
                Some(&mut ctx),
            )?;

            // Step 8: attach effective positions
            // Use allow_spike variant: bilateral trade may transiently push one
            // side's count to cap+1 between the two attaches. execute_trade's
            // pre-flight proves the final per-side count fits cap, and
            // assert_public_postconditions re-verifies at instruction end.
            self.attach_effective_position_allow_spike(a as usize, new_eff_a)?;
            self.attach_effective_position_allow_spike(b as usize, new_eff_b)?;

            // Step 9: write pre-computed OI (same values from step 5, spec §5.2.2)
            self.oi_eff_long_q = oi_long_after;
            self.oi_eff_short_q = oi_short_after;

            // Step 10: settle post-trade losses from principal for both accounts (spec §10.4 step 18)
            // Loss seniority: losses MUST be settled before explicit fees (spec §0 item 14)
            self.settle_losses_with_context(a as usize, Some(&mut ctx))?;
            self.settle_losses_with_context(b as usize, Some(&mut ctx))?;

            // Step 11: charge trading fees (spec §10.4 step 19, §8.1)
            let trade_notional =
                mul_div_floor_u128(size_q.unsigned_abs(), exec_price as u128, POS_SCALE);
            let fee = if trade_notional > 0 && self.params.trading_fee_bps > 0 {
                mul_div_ceil_u128(trade_notional, self.params.trading_fee_bps as u128, 10_000)
            } else {
                0
            };

            // Charge fee from both accounts (spec §10.5 step 28). Only the
            // equity-impact value (capital_paid + collectible_debt) feeds the
            // post-trade margin enforcement below; cash-to-insurance and
            // dropped portions are side effects of charge_fee_to_insurance.
            let mut fee_impact_a = 0u128;
            let mut fee_impact_b = 0u128;
            if fee > 0 {
                if fee > MAX_PROTOCOL_FEE_ABS {
                    return Err(RiskError::Overflow);
                }
                let (_cash_a, impact_a, _dropped_a) =
                    self.charge_fee_to_insurance(a as usize, fee)?;
                let (_cash_b, impact_b, _dropped_b) =
                    self.charge_fee_to_insurance(b as usize, fee)?;
                fee_impact_a = impact_a;
                fee_impact_b = impact_b;
            }

            // Step 29: post-trade margin enforcement (spec §10.5)
            // The spec says "(Eq_maint_raw_i + fee)" using the nominal fee.
            // We use fee_impact (capital_paid + collectible_debt) instead because:
            // - charge_fee_to_insurance can drop excess beyond collectible headroom
            // - Eq_maint_raw only decreased by impact, not the full nominal fee
            // - Adding back impact correctly reverses the actual state change
            // - Using nominal fee would over-compensate and admit invalid trades
            self.enforce_post_trade_margin(
                a as usize,
                b as usize,
                oracle_price,
                &old_eff_a,
                &new_eff_a,
                &old_eff_b,
                &new_eff_b,
                buffer_pre_a,
                buffer_pre_b,
                fee_impact_a,
                fee_impact_b,
                trade_pnl_a,
                trade_pnl_b,
                self.stress_gate_active(&ctx),
            )?;

            // Finalize touched accounts (shared snapshot conversion + fee sweep)
            self.finalize_touched_accounts_post_live(&mut ctx)?;

            // Steps 16-17: end-of-instruction resets
            self.schedule_end_of_instruction_resets(&mut ctx)?;
            self.finalize_end_of_instruction_resets(&ctx)?;

            self.assert_public_postconditions()?;
            Ok(())
        })
    }

    test_visible! {
    /// Charge fee per spec §8.1 — route shortfall through fee_credits instead of PNL.
    /// Returns (capital_paid_to_insurance, total_equity_impact).
    /// capital_paid is realized revenue; total includes collectible debt.
    /// Any excess beyond collectible headroom is silently dropped.
    /// Returns (fee_paid_to_insurance, fee_equity_impact, fee_dropped) per spec §4.14.
    fn charge_fee_to_insurance(&mut self, idx: usize, fee: u128) -> Result<(u128, u128, u128)> {
        self.validate_touched_account_shape(idx)?;
        let current_fc = self.accounts[idx].fee_credits.get();
        if fee > MAX_PROTOCOL_FEE_ABS {
            return Err(RiskError::Overflow);
        }
        let cap = self.accounts[idx].capital.get();
        let fee_paid = core::cmp::min(fee, cap);
        if fee_paid > 0 {
            self.set_capital(idx, cap - fee_paid)?;
            self.insurance_fund.balance = U128::new(
                self.insurance_fund.balance.get().checked_add(fee_paid)
                    .ok_or(RiskError::Overflow)?);
        }
        let fee_shortfall = fee - fee_paid;
        if fee_shortfall > 0 {
            // Route collectible shortfall through fee_credits (debit).
            // Cap at collectible headroom to avoid reverting (spec §8.2.2):
            // fee_credits must stay in [-(i128::MAX), 0]; any excess is dropped.
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
            let equity_impact = fee_paid + collectible;
            let dropped = fee - equity_impact;
            Ok((fee_paid, equity_impact, dropped))
        } else {
            Ok((fee_paid, fee_paid, 0))
        }
    }
    }

    /// OI component helpers for exact bilateral decomposition (spec §5.2.2)
    fn oi_long_component(pos: i128) -> u128 {
        if pos > 0 {
            pos as u128
        } else {
            0u128
        }
    }

    fn oi_short_component(pos: i128) -> u128 {
        if pos < 0 {
            pos.unsigned_abs()
        } else {
            0u128
        }
    }

    /// Compute exact bilateral candidate side-OI after-values (spec §5.2.2).
    /// Returns (OI_long_after, OI_short_after).
    test_visible! {
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
    }

    test_visible! {
    fn enforce_side_mode_oi_gate(
        &self,
        old_eff_a: i128,
        new_eff_a: i128,
        old_eff_b: i128,
        new_eff_b: i128,
        oi_long_after: u128,
        oi_short_after: u128,
    ) -> Result<()> {
        if (self.side_mode_long == SideMode::DrainOnly || self.side_mode_long == SideMode::ResetPending)
            && oi_long_after > self.oi_eff_long_q
        {
            return Err(RiskError::SideBlocked);
        }
        if (self.side_mode_short == SideMode::DrainOnly || self.side_mode_short == SideMode::ResetPending)
            && oi_short_after > self.oi_eff_short_q
        {
            return Err(RiskError::SideBlocked);
        }
        self.enforce_side_mode_account_gate(old_eff_a, new_eff_a)?;
        self.enforce_side_mode_account_gate(old_eff_b, new_eff_b)?;
        Ok(())
    }
    }

    fn enforce_side_mode_account_gate(&self, old_eff: i128, new_eff: i128) -> Result<()> {
        if new_eff == 0 {
            return Ok(());
        }
        let side = side_of_i128(new_eff).ok_or(RiskError::CorruptState)?;
        match self.get_side_mode(side) {
            SideMode::Normal => Ok(()),
            SideMode::ResetPending => Err(RiskError::SideBlocked),
            SideMode::DrainOnly => {
                let old_same_side = side_of_i128(old_eff) == Some(side);
                if old_same_side && new_eff.unsigned_abs() <= old_eff.unsigned_abs() {
                    Ok(())
                } else {
                    Err(RiskError::SideBlocked)
                }
            }
        }
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
        trade_pnl_a: i128,
        trade_pnl_b: i128,
        stress_active: bool,
    ) -> Result<()> {
        self.enforce_one_side_margin(
            a,
            oracle_price,
            old_eff_a,
            new_eff_a,
            buffer_pre_a,
            fee_a,
            trade_pnl_a,
            stress_active,
        )?;
        self.enforce_one_side_margin(
            b,
            oracle_price,
            old_eff_b,
            new_eff_b,
            buffer_pre_b,
            fee_b,
            trade_pnl_b,
            stress_active,
        )?;
        Ok(())
    }

    test_visible! {
    fn enforce_one_side_margin(
        &self,
        idx: usize,
        oracle_price: u64,
        old_eff: &i128,
        new_eff: &i128,
        buffer_pre: I256,
        fee: u128,
        candidate_trade_pnl: i128,
        stress_active: bool,
    ) -> Result<()> {
        if *new_eff == 0 {
            // Flat result: fee-neutral negative shortfall must not worsen.
            // min(Eq_maint_raw_post + fee_equity_impact, 0) >= min(Eq_maint_raw_pre, 0)
            // Uses the actual applied fee impact (fee parameter), not nominal requested fee.
            // buffer_pre = Eq_maint_raw_pre - MM_req_pre; add MM_req_pre back.
            // Use old_eff (pre-trade) to compute MM_req_pre — NOT current state (post-trade).
            let mm_req_pre_wide = if *old_eff == 0 { I256::ZERO } else {
                let not_pre = Self::risk_notional_from_eff_q(*old_eff, oracle_price);
                I256::from_u128(core::cmp::max(
                    mul_div_floor_u128(not_pre, self.params.maintenance_margin_bps as u128, 10_000),
                    self.params.min_nonzero_mm_req))
            };
            let eq_maint_raw_pre = buffer_pre.checked_add(mm_req_pre_wide).expect("I256 add");
            let shortfall_pre = if eq_maint_raw_pre.is_negative() { eq_maint_raw_pre } else { I256::ZERO };

            let eq_maint_raw_post = self.account_equity_maint_raw_wide(&self.accounts[idx]);
            let fee_wide = I256::from_u128(fee);
            let maint_raw_fee_neutral = eq_maint_raw_post.checked_add(fee_wide).expect("I256 add");
            let shortfall_post = if maint_raw_fee_neutral.is_negative() { maint_raw_fee_neutral } else { I256::ZERO };

            // shortfall_post >= shortfall_pre (both <= 0; "worsening" means more negative)
            if shortfall_post.checked_sub(shortfall_pre).map_or(true, |d| d.is_negative()) {
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
            // Require Eq_trade_open_raw_i >= IM_req (spec §3.5 + §9.1)
            // Uses counterfactual equity with candidate trade's positive slippage removed
            let ok = if stress_active {
                self.is_above_initial_margin_trade_open_no_pos(
                    &self.accounts[idx],
                    idx,
                    oracle_price,
                    candidate_trade_pnl,
                )
            } else {
                self.is_above_initial_margin_trade_open(
                    &self.accounts[idx],
                    idx,
                    oracle_price,
                    candidate_trade_pnl,
                )
            };
            if !ok {
                return Err(RiskError::Undercollateralized);
            }
        } else if self.is_above_maintenance_margin(&self.accounts[idx], idx, oracle_price) {
            // Maintenance healthy: allow
        } else if strictly_reducing {
            // Strict risk-reducing exemption.
            // Both conditions must hold in exact widened I256:
            // 1. Fee-neutral buffer improves: (Eq_maint_raw_post + fee) - MM_req_post > buffer_pre
            // 2. Fee-neutral shortfall does not worsen: min(Eq_maint_raw_post + fee, 0) >= min(Eq_maint_raw_pre, 0)
            let maint_raw_wide_post = self.account_equity_maint_raw_wide(&self.accounts[idx]);
            let fee_wide = I256::from_u128(fee);

            // Fee-neutral post equity and buffer
            let maint_raw_fee_neutral = maint_raw_wide_post.checked_add(fee_wide).expect("I256 add");
            let mm_req_post = {
                let not = self.notional_checked(idx, oracle_price, false)?;
                core::cmp::max(
                    mul_div_floor_u128(not, self.params.maintenance_margin_bps as u128, 10_000),
                    self.params.min_nonzero_mm_req
                )
            };
            let buffer_post_fee_neutral = maint_raw_fee_neutral.checked_sub(I256::from_u128(mm_req_post)).expect("I256 sub");

            // Recover pre-trade raw equity from buffer_pre + MM_req_pre
            let mm_req_pre = {
                let not_pre = if *old_eff == 0 { 0u128 } else {
                    Self::risk_notional_from_eff_q(*old_eff, oracle_price)
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
        funding_rate_e9: i128,
        admit_h_min: u64,
        admit_h_max: u64,
        admit_h_max_consumption_threshold_bps_opt: Option<u128>,
    ) -> Result<bool> {
        rollback_on_error!(self, [idx], {
            Self::validate_admission_pair(admit_h_min, admit_h_max, &self.params)?;
            Self::validate_threshold_opt(admit_h_max_consumption_threshold_bps_opt)?;

            // Spec §9.6 step 2: require account materialized (public entry point).
            self.validate_touched_account_shape(idx as usize)?;

            if self.market_mode != MarketMode::Live {
                return Err(RiskError::Unauthorized);
            }

            let mut ctx = InstructionContext::new_with_admission_and_threshold(
                admit_h_min,
                admit_h_max,
                admit_h_max_consumption_threshold_bps_opt,
            );

            self.ensure_loss_current_for_position_change()?;

            // Step 2: accrue market
            self.accrue_market_to(now_slot, oracle_price, funding_rate_e9)?;
            self.current_slot = now_slot;
            self.ensure_loss_current_for_position_change()?;

            // Step 3: live local touch
            self.touch_account_live_local(idx as usize, &mut ctx)?;

            // Step 4: liquidate (before finalize, so post-liquidation state gets finalized)
            let result =
                self.liquidate_at_oracle_internal(idx, now_slot, oracle_price, policy, &mut ctx)?;

            // Step 5: finalize AFTER liquidation — post-liquidation flat accounts
            // get whole-only conversion and fee sweep
            self.finalize_touched_accounts_post_live(&mut ctx)?;

            // End-of-instruction resets
            self.schedule_end_of_instruction_resets(&mut ctx)?;
            self.finalize_end_of_instruction_resets(&ctx)?;

            self.assert_public_postconditions()?;
            Ok(result)
        })
    }

    /// Internal liquidation routine: takes caller's shared InstructionContext.
    /// Precondition (spec §9.4): caller has already called touch_account_live_local(i).
    /// Does NOT call schedule/finalize resets — caller is responsible.
    fn liquidate_at_oracle_internal(
        &mut self,
        idx: u16,
        _now_slot: u64,
        oracle_price: u64,
        policy: LiquidationPolicy,
        ctx: &mut InstructionContext,
    ) -> Result<bool> {
        if idx as usize >= MAX_ACCOUNTS
            || idx as u64 >= self.params.max_accounts
            || !self.is_used(idx as usize)
        {
            return Ok(false);
        }

        if oracle_price == 0 || oracle_price > MAX_ORACLE_PRICE {
            return Err(RiskError::Overflow);
        }

        // Check position exists
        let old_eff = self.effective_pos_q_checked(idx as usize, false)?;
        if old_eff == 0 {
            return Ok(false);
        }

        // Step 4: check liquidation eligibility (spec §9.3)
        if self.is_above_maintenance_margin(
            &self.accounts[idx as usize],
            idx as usize,
            oracle_price,
        ) {
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
                let new_eff_abs_q = abs_old_eff
                    .checked_sub(q_close_q)
                    .ok_or(RiskError::Overflow)?;
                // Step 5: require new_eff_abs_q > 0 (property 68)
                if new_eff_abs_q == 0 {
                    return Err(RiskError::Overflow);
                }
                // Step 6: new_eff_pos_q_i = sign(old) * new_eff_abs_q
                let sign = if old_eff > 0 { 1i128 } else { -1i128 };
                let new_eff = sign
                    .checked_mul(new_eff_abs_q as i128)
                    .ok_or(RiskError::Overflow)?;

                // Step 7-8: close q_close_q at oracle, attach new position
                self.attach_effective_position(idx as usize, new_eff)?;

                // Step 9: settle realized losses from principal
                self.settle_losses_with_context(idx as usize, Some(ctx))?;

                // Step 10-11: charge liquidation fee on quantity closed
                let liq_fee = {
                    let notional_val =
                        mul_div_floor_u128(q_close_q, oracle_price as u128, POS_SCALE);
                    let liq_fee_raw = mul_div_ceil_u128(
                        notional_val,
                        self.params.liquidation_fee_bps as u128,
                        10_000,
                    );
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
                self.enforce_partial_liq_post_health(idx as usize, oracle_price)?;

                Ok(true)
            }
            LiquidationPolicy::FullClose => {
                // Spec §9.5: full-close liquidation (existing behavior)
                let q_close_q = abs_old_eff;

                // Close entire position at oracle
                self.attach_effective_position(idx as usize, 0i128)?;

                // Settle losses from principal
                self.settle_losses_with_context(idx as usize, Some(ctx))?;

                // Charge liquidation fee (spec §8.3)
                let liq_fee = if q_close_q == 0 {
                    0u128
                } else {
                    let notional_val =
                        mul_div_floor_u128(q_close_q, oracle_price as u128, POS_SCALE);
                    let liq_fee_raw = mul_div_ceil_u128(
                        notional_val,
                        self.params.liquidation_fee_bps as u128,
                        10_000,
                    );
                    core::cmp::min(
                        core::cmp::max(liq_fee_raw, self.params.min_liquidation_abs.get()),
                        self.params.liquidation_fee_cap.get(),
                    )
                };
                self.charge_fee_to_insurance(idx as usize, liq_fee)?;

                // Determine deficit D
                let eff_post = self.effective_pos_q_checked(idx as usize, false)?;
                let d: u128 = if eff_post == 0 && self.accounts[idx as usize].pnl < 0 {
                    if self.accounts[idx as usize].pnl == i128::MIN {
                        return Err(RiskError::CorruptState);
                    }
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
                    // Spec §8.5 step 8: NoPositiveIncreaseAllowed for defense-in-depth
                    self.set_pnl_with_reserve(
                        idx as usize,
                        0i128,
                        ReserveMode::NoPositiveIncreaseAllowed,
                        None,
                    )?;
                }

                Ok(true)
            }
        }
    }

    test_visible! {
    fn enforce_partial_liq_post_health(&self, idx: usize, oracle_price: u64) -> Result<()> {
        if !self.is_above_maintenance_margin(&self.accounts[idx], idx, oracle_price) {
            return Err(RiskError::Undercollateralized);
        }
        Ok(())
    }
    }

    // ========================================================================
    // keeper_crank_not_atomic (spec §10.6)
    // ========================================================================

    /// keeper_crank_not_atomic (spec §9.7): minimal on-chain permissionless
    /// shortlist processor. Candidate discovery is performed off-chain;
    /// `ordered_candidates[]` is untrusted. Each candidate is
    /// `(account_idx, optional liquidation policy hint)`.
    ///
    /// Two-phase: Phase 1 runs keeper-priority liquidation; Phase 2 always
    /// runs a mandatory structural sweep starting from `rr_cursor_position`.
    /// It performs a full authenticated scan window and touches up to
    /// `rr_touch_limit` materialized accounts. Use
    /// `keeper_crank_with_request_not_atomic` for explicit scan limits.
    test_visible! {
    fn phase2_scan_outcome(
        &self,
        wrap_bound: u64,
        rr_touch_limit: u64,
        rr_scan_limit: u64,
        stress_active: bool,
        wrap_allowed: bool,
        same_slot_as_stress_start: bool,
    ) -> Result<Phase2ScanOutcome> {
        let mut i = self.rr_cursor_position;
        let scan_cap = core::cmp::min(rr_scan_limit, wrap_bound);
        let mut touched = 0u64;
        let mut inspected = 0u64;
        let mut stress_counted_inspected = 0u64;
        let mut wrapped = false;

        if rr_touch_limit == 0 || rr_scan_limit == 0 {
            if wrap_bound == 0 || i >= wrap_bound {
                return Err(RiskError::CorruptState);
            }
            return Ok(Phase2ScanOutcome {
                next_cursor: i,
                inspected,
                touched,
                stress_counted_inspected,
                wrapped,
            });
        }

        while inspected < scan_cap && touched < rr_touch_limit {
            if wrap_bound == 0 || i >= wrap_bound {
                return Err(RiskError::CorruptState);
            }
            if i == wrap_bound - 1 && !wrap_allowed {
                break;
            }
            if self.is_used(i as usize) {
                touched = touched.checked_add(1).ok_or(RiskError::Overflow)?;
            }
            i = i.checked_add(1).ok_or(RiskError::Overflow)?;
            inspected = inspected.checked_add(1).ok_or(RiskError::Overflow)?;
            if stress_active && wrap_allowed && !same_slot_as_stress_start {
                stress_counted_inspected = stress_counted_inspected
                    .checked_add(1)
                    .ok_or(RiskError::Overflow)?;
            }
            if i == wrap_bound {
                i = 0;
                wrapped = true;
                break;
            }
        }

        Ok(Phase2ScanOutcome {
            next_cursor: i,
            inspected,
            touched,
            stress_counted_inspected,
            wrapped,
        })
    }
    }

    pub fn keeper_crank_not_atomic(
        &mut self,
        now_slot: u64,
        oracle_price: u64,
        ordered_candidates: &[(u16, Option<LiquidationPolicy>)],
        max_revalidations: u16,
        funding_rate_e9: i128,
        admit_h_min: u64,
        admit_h_max: u64,
        admit_h_max_consumption_threshold_bps_opt: Option<u128>,
        rr_touch_limit: u64,
    ) -> Result<CrankOutcome> {
        self.keeper_crank_with_request_not_atomic(KeeperCrankRequest::full_scan(
            now_slot,
            oracle_price,
            ordered_candidates,
            max_revalidations,
            funding_rate_e9,
            admit_h_min,
            admit_h_max,
            admit_h_max_consumption_threshold_bps_opt,
            rr_touch_limit,
        ))
    }

    pub fn keeper_crank_with_request_not_atomic(
        &mut self,
        req: KeeperCrankRequest<'_>,
    ) -> Result<CrankOutcome> {
        let __before = self.rollback_snapshot_keeper(&req);
        #[allow(clippy::redundant_closure_call)]
        let __result = (|| {
            let KeeperCrankRequest {
                now_slot,
                oracle_price,
                ordered_candidates,
                max_revalidations,
                max_candidate_inspections,
                funding_rate_e9,
                admit_h_min,
                admit_h_max,
                admit_h_max_consumption_threshold_bps_opt,
                rr_touch_limit,
                rr_scan_limit,
            } = req;
            // Pre-state invariant check catches corrupt inputs like
            // rr_cursor_position out of range or ready-snapshot inconsistency
            // before mutation.
            self.assert_public_postconditions()?;

            // Step 1 (spec §9.0): validate inputs pre-mutation.
            Self::validate_admission_pair(admit_h_min, admit_h_max, &self.params)?;
            Self::validate_threshold_opt(admit_h_max_consumption_threshold_bps_opt)?;

            if oracle_price == 0 || oracle_price > MAX_ORACLE_PRICE {
                return Err(RiskError::Overflow);
            }

            if self.market_mode != MarketMode::Live {
                return Err(RiskError::Unauthorized);
            }

            // Combined Phase 1 + Phase 2 touched-account budget must fit the
            // runtime ctx capacity. In Phase 2, rr_touch_limit is a touched-account
            // budget; unused slots are authenticated by the engine bitmap and do
            // not consume touched-account capacity.
            let combined_touch_budget = (max_revalidations as u64).saturating_add(rr_touch_limit);
            if combined_touch_budget > MAX_TOUCHED_PER_INSTRUCTION as u64 {
                return Err(RiskError::Overflow);
            }

            // Step 2: initialize instruction context with threshold gate wired in.
            let mut ctx = InstructionContext::new_with_admission_and_threshold(
                admit_h_min,
                admit_h_max,
                admit_h_max_consumption_threshold_bps_opt,
            );

            // Steps 3-4: validate time monotonicity.
            if now_slot < self.current_slot {
                return Err(RiskError::Overflow);
            }
            if now_slot < self.last_market_slot {
                return Err(RiskError::Overflow);
            }
            let equity_active_accrual =
                self.keeper_accrual_is_equity_active(now_slot, oracle_price, funding_rate_e9);
            let remaining_dt = now_slot
                .checked_sub(self.last_market_slot)
                .ok_or(RiskError::Overflow)?;
            let accrual_slot =
                if equity_active_accrual && remaining_dt > self.params.max_accrual_dt_slots {
                    self.last_market_slot
                        .checked_add(self.params.max_accrual_dt_slots)
                        .ok_or(RiskError::Overflow)?
                } else {
                    now_slot
                };
            if equity_active_accrual
                && !self.keeper_has_possible_protective_progress(
                    now_slot,
                    ordered_candidates,
                    max_revalidations,
                    max_candidate_inspections,
                    rr_touch_limit,
                    rr_scan_limit,
                )?
            {
                return Err(RiskError::Undercollateralized);
            }

            // Step 5: accrue once. If the authenticated slot is too stale for a
            // single equity-active accrual, advance exactly one bounded segment and
            // keep public time owned by `now_slot`.
            self.accrue_market_segment_to_internal(
                accrual_slot,
                now_slot,
                now_slot,
                oracle_price,
                funding_rate_e9,
            )?;
            let loss_stale_after_accrual = self.loss_stale_positive_pnl_lock_active();

            // Phase 1 (spec §9.7 step 6): spot liquidation from keeper shortlist.
            let mut attempts: u16 = 0;
            let max_candidate_inspections = core::cmp::min(
                MAX_TOUCHED_PER_INSTRUCTION as u16,
                max_candidate_inspections,
            );
            self.pretrigger_bankruptcy_hmax_for_candidates(
                &mut ctx,
                ordered_candidates,
                max_revalidations,
                max_candidate_inspections,
            )?;
            let mut inspected: u16 = 0;
            let mut num_liquidations: u32 = 0;
            let mut protective_progress = false;

            for &(candidate_idx, ref hint) in ordered_candidates {
                if attempts >= max_revalidations || inspected >= max_candidate_inspections {
                    break;
                }
                if ctx.pending_reset_long || ctx.pending_reset_short {
                    break;
                }
                inspected = inspected.checked_add(1).ok_or(RiskError::Overflow)?;
                if (candidate_idx as usize) >= MAX_ACCOUNTS || !self.is_used(candidate_idx as usize)
                {
                    continue;
                }
                if candidate_idx as u64 >= self.params.max_accounts {
                    continue;
                }

                attempts += 1;
                let cidx = candidate_idx as usize;

                self.touch_account_live_local(cidx, &mut ctx)?;
                protective_progress = true;

                if !loss_stale_after_accrual && !ctx.pending_reset_long && !ctx.pending_reset_short
                {
                    let eff = self.effective_pos_q_checked(cidx, false)?;
                    if eff != 0 {
                        if !self.is_above_maintenance_margin(
                            &self.accounts[cidx],
                            cidx,
                            oracle_price,
                        ) {
                            if let Some(policy) =
                                self.validate_keeper_hint(candidate_idx, eff, hint, oracle_price)?
                            {
                                match self.liquidate_at_oracle_internal(
                                    candidate_idx,
                                    now_slot,
                                    oracle_price,
                                    policy,
                                    &mut ctx,
                                ) {
                                    Ok(true) => {
                                        num_liquidations += 1;
                                    }
                                    Ok(false) => {}
                                    Err(e) => return Err(e),
                                }
                            }
                        }
                    }
                }
            }

            // Phase 2 (spec §9.7 step 7): mandatory round-robin structural sweep.
            // Runs unconditionally — including when Phase 1 exited early on a
            // pending reset. Phase 2 does NOT execute liquidations, does NOT
            // count against max_revalidations, and does NOT break on pending
            // reset. Its job is to deterministically walk index space, skip unused
            // slots, and touch up to rr_touch_limit materialized accounts so
            // warmup/reserve state advances uniformly across the deployment.
            //
            // Cursor wrap bound: params.max_accounts (runtime slab capacity).
            // Generation turnover is proportional to the real deployment size;
            // the spec's theoretical 1e6 bound was collapsed onto this runtime
            // value so compact shards do not spend most of a generation
            // walking non-existent index space.
            let wrap_bound = self.params.max_accounts;
            let reconciliation_envelope_active =
                self.stress_consumed_bps_e9_since_envelope > 0 || self.bankruptcy_hmax_lock_active;
            let same_slot_as_stress_start =
                reconciliation_envelope_active && self.stress_envelope_start_slot == now_slot;
            let wrap_allowed = self.last_sweep_generation_advance_slot == NO_SLOT
                || now_slot > self.last_sweep_generation_advance_slot;

            let phase2 = self.phase2_scan_outcome(
                wrap_bound,
                rr_touch_limit,
                rr_scan_limit,
                reconciliation_envelope_active,
                wrap_allowed,
                same_slot_as_stress_start,
            )?;
            self.pretrigger_bankruptcy_hmax_for_phase2(&mut ctx, phase2.inspected)?;

            let phase2_guard_prev = ctx.speculative_hmax_guard_active;
            let phase2_guard_enabled = phase2.touched > 0
                && !phase2_guard_prev
                && !ctx.bankruptcy_hmax_candidate_active
                && !self.bankruptcy_hmax_lock_active;
            if phase2_guard_enabled {
                ctx.speculative_hmax_guard_active = true;
            }

            let mut touch_i = self.rr_cursor_position;
            let mut replayed_inspected = 0u64;
            let mut replayed_touched = 0u64;
            while replayed_inspected < phase2.inspected {
                if wrap_bound == 0 || touch_i >= wrap_bound {
                    return Err(RiskError::CorruptState);
                }
                let iu = touch_i as usize;
                if self.is_used(iu) {
                    self.touch_account_live_local(iu, &mut ctx)?;
                    replayed_touched =
                        replayed_touched.checked_add(1).ok_or(RiskError::Overflow)?;
                }
                touch_i = touch_i.checked_add(1).ok_or(RiskError::Overflow)?;
                replayed_inspected = replayed_inspected
                    .checked_add(1)
                    .ok_or(RiskError::Overflow)?;
                if touch_i == wrap_bound {
                    touch_i = 0;
                    break;
                }
            }
            if phase2_guard_enabled
                && !ctx.stress_envelope_restarted
                && !self.bankruptcy_hmax_lock_active
            {
                ctx.speculative_hmax_guard_active = phase2_guard_prev;
            }
            if touch_i != phase2.next_cursor || replayed_touched != phase2.touched {
                return Err(RiskError::CorruptState);
            }
            if phase2.inspected > 0 {
                protective_progress = true;
            }

            if phase2.wrapped && wrap_allowed {
                self.advance_sweep_generation(now_slot)?;
            }
            self.rr_cursor_position = phase2.next_cursor;
            let stress_counted_inspected = if ctx.stress_envelope_restarted {
                0
            } else {
                phase2.stress_counted_inspected
            };
            self.apply_stress_envelope_progress(now_slot, stress_counted_inspected)?;
            if equity_active_accrual && !protective_progress {
                return Err(RiskError::Undercollateralized);
            }

            // Finalize: compute fresh snapshot from post-mutation state, apply
            // whole-only conversion + fee sweep to all tracked accounts.
            self.finalize_touched_accounts_post_live(&mut ctx)?;

            // End-of-instruction resets (spec §9.7 steps 9-10).
            self.schedule_end_of_instruction_resets(&mut ctx)?;
            self.finalize_end_of_instruction_resets(&ctx)?;

            self.assert_public_postconditions()?;
            Ok(CrankOutcome { num_liquidations })
        })();
        if __result.is_err() {
            self.restore_rollback_snapshot(__before);
        }
        __result
    }

    /// Validate a keeper-supplied liquidation-policy hint (spec §11.1 rule 3).
    /// Returns None if no liquidation action should be taken (absent hint per
    /// spec §11.2), or Some(policy) if the hint is valid. ExactPartial hints
    /// are validated via a stateless pre-flight check; invalid partials
    /// return None (no liquidation action) per spec §11.1 rule 3.
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
    ) -> Result<Option<LiquidationPolicy>> {
        let i = idx as usize;
        self.validate_touched_account_shape(i)?;
        match hint {
            // Spec §11.2: absent hint means no liquidation action for this candidate.
            None => Ok(None),
            Some(LiquidationPolicy::FullClose) => Ok(Some(LiquidationPolicy::FullClose)),
            Some(LiquidationPolicy::ExactPartial(q_close_q)) => {
                let abs_eff = eff.unsigned_abs();
                // Bounds check: 0 < q_close_q < abs(eff)
                // Spec §11.1 rule 3: invalid hint → no liquidation action (None)
                if *q_close_q == 0 || *q_close_q >= abs_eff {
                    return Ok(None);
                }

                // Stateless pre-flight: predict post-partial maintenance health.
                let account = &self.accounts[i];

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
                    None => return Ok(None),
                };

                // 3. Predict post-partial MM_req
                let rem_eff = abs_eff - *q_close_q;
                let rem_notional = mul_div_ceil_u128(rem_eff, oracle_price as u128, POS_SCALE);
                let proportional_mm = mul_div_floor_u128(rem_notional, self.params.maintenance_margin_bps as u128, 10_000);
                let predicted_mm_req = if rem_eff == 0 {
                    0u128
                } else {
                    core::cmp::max(proportional_mm, self.params.min_nonzero_mm_req)
                };

                // 4. Health check: predicted_eq > predicted_mm_req
                // Spec §11.1 rule 3: failed pre-flight → no liquidation action (None)
                if predicted_eq <= I256::from_u128(predicted_mm_req) {
                    return Ok(None);
                }

                Ok(Some(LiquidationPolicy::ExactPartial(*q_close_q)))
            }
        }
    }
    }

    // ========================================================================
    // convert_released_pnl_not_atomic (spec §10.4.1)
    // ========================================================================

    test_visible! {
    fn convert_released_pnl_core(
        &mut self,
        idx: usize,
        x_req: u128,
        oracle_price: u64,
    ) -> Result<()> {
        let released = self.released_pos_checked(idx, false)?;
        if x_req == 0 || x_req > released {
            return Err(RiskError::Overflow);
        }

        // Step 6: compute y using pre-conversion haircut (spec §7.4).
        let (h_num, h_den) = self.haircut_ratio();
        if h_den == 0 { return Err(RiskError::CorruptState); }

        // Step 9 (spec §9.3.1): flat-account safety cap (spec §4.12)
        if self.accounts[idx].position_basis_q == 0 {
            let max_safe = self.max_safe_flat_conversion_released(idx, x_req, h_num, h_den);
            if x_req > max_safe {
                return Err(RiskError::Undercollateralized);
            }
        }

        let y: u128 = if h_num == h_den {
            x_req
        } else {
            wide_mul_div_floor_u128(x_req, h_num, h_den)
        };

        // Step 7: consume_released_pnl(i, x_req)
        self.consume_released_pnl(idx, x_req)?;

        // Step 8: set_capital(i, C_i + y)
        let new_cap = self.accounts[idx].capital.get()
            .checked_add(y).ok_or(RiskError::Overflow)?;
        self.set_capital(idx, new_cap)?;

        // Step 9: sweep fee debt
        self.fee_debt_sweep(idx)?;

        // Step 10: post-conversion health check
        let eff = self.effective_pos_q_checked(idx, false)?;
        if eff != 0 {
            // Open position: require maintenance margin
            if !self.is_above_maintenance_margin(&self.accounts[idx], idx, oracle_price) {
                return Err(RiskError::Undercollateralized);
            }
        } else {
            // Flat account: require non-negative raw maintenance equity.
            let eq = self.account_equity_maint_raw(&self.accounts[idx]);
            if eq < 0 {
                return Err(RiskError::Undercollateralized);
            }
        }

        Ok(())
    }
    }

    /// Explicit voluntary conversion of matured released positive PnL for open-position accounts.
    pub fn convert_released_pnl_not_atomic(
        &mut self,
        idx: u16,
        x_req: u128,
        oracle_price: u64,
        now_slot: u64,
        funding_rate_e9: i128,
        admit_h_min: u64,
        admit_h_max: u64,
        admit_h_max_consumption_threshold_bps_opt: Option<u128>,
    ) -> Result<()> {
        rollback_on_error!(self, [idx], {
            Self::validate_admission_pair(admit_h_min, admit_h_max, &self.params)?;
            Self::validate_threshold_opt(admit_h_max_consumption_threshold_bps_opt)?;

            if oracle_price == 0 || oracle_price > MAX_ORACLE_PRICE {
                return Err(RiskError::Overflow);
            }
            self.validate_touched_account_shape(idx as usize)?;

            if self.market_mode != MarketMode::Live {
                return Err(RiskError::Unauthorized);
            }

            let mut ctx = InstructionContext::new_with_admission_and_threshold(
                admit_h_min,
                admit_h_max,
                admit_h_max_consumption_threshold_bps_opt,
            );

            if self.loss_stale_positive_pnl_lock_active() {
                return Err(RiskError::Undercollateralized);
            }

            // Step 2: accrue market
            self.accrue_market_to(now_slot, oracle_price, funding_rate_e9)?;
            self.current_slot = now_slot;

            // Step 3: live local touch (no auto-convert, no finalize yet)
            self.touch_account_live_local(idx as usize, &mut ctx)?;
            if self.stress_gate_active(&ctx) {
                return Err(RiskError::Undercollateralized);
            }

            // Steps 4-10 happen before finalize so auto-convert cannot consume
            // the user's released PnL before they can request it.
            self.convert_released_pnl_core(idx as usize, x_req, oracle_price)?;

            // Step 11: finalize after explicit conversion.
            self.finalize_touched_accounts_post_live(&mut ctx)?;

            // Steps 12-13: end-of-instruction resets
            self.schedule_end_of_instruction_resets(&mut ctx)?;
            self.finalize_end_of_instruction_resets(&ctx)?;

            self.assert_public_postconditions()?;
            Ok(())
        })
    }

    // ========================================================================
    // close_account_not_atomic
    // ========================================================================

    pub fn close_account_not_atomic(
        &mut self,
        idx: u16,
        now_slot: u64,
        oracle_price: u64,
        funding_rate_e9: i128,
        admit_h_min: u64,
        admit_h_max: u64,
        admit_h_max_consumption_threshold_bps_opt: Option<u128>,
    ) -> Result<u128> {
        rollback_on_error!(self, [idx], {
            Self::validate_admission_pair(admit_h_min, admit_h_max, &self.params)?;
            Self::validate_threshold_opt(admit_h_max_consumption_threshold_bps_opt)?;

            if self.market_mode != MarketMode::Live {
                return Err(RiskError::Unauthorized);
            }

            self.validate_touched_account_shape(idx as usize)?;

            let mut ctx = InstructionContext::new_with_admission_and_threshold(
                admit_h_min,
                admit_h_max,
                admit_h_max_consumption_threshold_bps_opt,
            );

            if self.loss_stale_positive_pnl_lock_active()
                && self.accounts[idx as usize].position_basis_q != 0
            {
                return Err(RiskError::Undercollateralized);
            }

            // Accrue market + live local touch + finalize
            self.accrue_market_to(now_slot, oracle_price, funding_rate_e9)?;
            self.current_slot = now_slot;
            if self.loss_stale_positive_pnl_lock_active()
                && self.accounts[idx as usize].position_basis_q != 0
            {
                return Err(RiskError::Undercollateralized);
            }
            self.touch_account_live_local(idx as usize, &mut ctx)?;
            self.finalize_touched_accounts_post_live(&mut ctx)?;

            // Position must be zero
            let eff = self.effective_pos_q_checked(idx as usize, false)?;
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

            // Spec §9.5 step 11: require FeeDebt_i == 0 (fee_credits >= 0).
            // Voluntary close must not forgive fee debt (unlike reclaim).
            if self.accounts[idx as usize].fee_credits.get() < 0 {
                return Err(RiskError::Undercollateralized);
            }

            // Spec §9.5 step 10: require R_i == 0 and both reserve buckets absent.
            if self.accounts[idx as usize].reserved_pnl != 0
                || self.accounts[idx as usize].sched_present != 0
                || self.accounts[idx as usize].pending_present != 0
            {
                return Err(RiskError::Undercollateralized);
            }

            let capital = self.accounts[idx as usize].capital;

            if capital > self.vault {
                return Err(RiskError::InsufficientBalance);
            }
            self.vault = self.vault - capital;
            self.set_capital(idx as usize, 0)?;

            // End-of-instruction resets before freeing
            self.schedule_end_of_instruction_resets(&mut ctx)?;
            self.finalize_end_of_instruction_resets(&ctx)?;

            self.free_slot(idx)?;
            self.sweep_empty_market_surplus_to_insurance()?;

            self.assert_public_postconditions()?;
            Ok(capital.get())
        })
    }

    // ========================================================================
    // force_close_resolved_not_atomic (resolved/frozen market path)
    // ========================================================================

    // ========================================================================
    // resolve_market (spec §10.7)
    // ========================================================================

    /// Transition market from Live to Resolved at a price-bounded settlement price.
    /// First accrues live state, then stores terminal K deltas separately.
    pub fn resolve_market_not_atomic(
        &mut self,
        resolve_mode: ResolveMode,
        resolved_price: u64,
        live_oracle_price: u64,
        now_slot: u64,
        funding_rate_e9: i128,
    ) -> Result<()> {
        rollback_on_error!(self, [], {
            if self.market_mode != MarketMode::Live {
                return Err(RiskError::Unauthorized);
            }
            if now_slot < self.current_slot {
                return Err(RiskError::Overflow);
            }
            // Degenerate branch also skips accrue_market_to's last_market_slot
            // monotonicity check; enforce it here so the degenerate branch cannot
            // decrease last_market_slot under corrupt state.
            if now_slot < self.last_market_slot {
                return Err(RiskError::Overflow);
            }
            if resolved_price == 0 || resolved_price > MAX_ORACLE_PRICE {
                return Err(RiskError::Overflow);
            }
            if live_oracle_price == 0 || live_oracle_price > MAX_ORACLE_PRICE {
                return Err(RiskError::Overflow);
            }

            // Explicit branch selection per spec §9.8.
            // Value-detected branch selection is forbidden: a flat live oracle
            // must NOT automatically enter the degenerate branch.
            let used_degenerate = match resolve_mode {
                ResolveMode::Degenerate => {
                    // Degenerate branch requires these trusted equalities.
                    if live_oracle_price != self.last_oracle_price {
                        return Err(RiskError::Overflow);
                    }
                    if funding_rate_e9 != 0 {
                        return Err(RiskError::Overflow);
                    }
                    self.current_slot = now_slot;
                    self.last_market_slot = now_slot;
                    true
                }
                ResolveMode::Ordinary => {
                    // Ordinary branch: accrue to now_slot using live inputs.
                    // Even when `live == P_last && rate == 0`, the ordinary
                    // branch stays ordinary (spec test 85).
                    self.accrue_market_to(now_slot, live_oracle_price, funding_rate_e9)?;
                    false
                }
            };

            // Band check runs on the ordinary branch only. The degenerate branch
            // relies entirely on trusted wrapper inputs (spec §9.8 step 9).
            if !used_degenerate {
                let p_last = self.last_oracle_price;
                let p_last_i = p_last as i128;
                let p_res = resolved_price as i128;
                let dev_bps = self.params.resolve_price_deviation_bps as i128;
                let diff_abs = (p_res - p_last_i).unsigned_abs();
                let lhs = (diff_abs as u128)
                    .checked_mul(10_000)
                    .ok_or(RiskError::Overflow)?;
                let rhs = (dev_bps as u128)
                    .checked_mul(p_last as u128)
                    .ok_or(RiskError::Overflow)?;
                if lhs > rhs {
                    return Err(RiskError::Overflow);
                }
            }

            let pre_mode_long = self.side_mode_long;
            let pre_mode_short = self.side_mode_short;
            let pre_a_long = self.adl_mult_long;
            let pre_a_short = self.adl_mult_short;
            let pre_oi_long = self.oi_eff_long_q;
            let pre_oi_short = self.oi_eff_short_q;
            let pre_stored_long = self.stored_pos_count_long;
            let pre_stored_short = self.stored_pos_count_short;

            // Step 8: compute resolved terminal mark deltas in exact signed arithmetic.
            // These deltas carry the settlement shift WITHOUT adding to persistent K_side,
            // so resolution can succeed even near K headroom (spec §9.7 step 8).
            let price_diff = resolved_price as i128 - live_oracle_price as i128;
            let resolved_k_long_td = if pre_mode_long == SideMode::ResetPending || pre_oi_long == 0
            {
                0i128
            } else {
                checked_u128_mul_i128(pre_a_long, price_diff)?
            };
            let resolved_k_short_td =
                if pre_mode_short == SideMode::ResetPending || pre_oi_short == 0 {
                    0i128
                } else {
                    // Short side: negative of price_diff
                    let neg_price_diff = price_diff.checked_neg().ok_or(RiskError::Overflow)?;
                    checked_u128_mul_i128(pre_a_short, neg_price_diff)?
                };

            // Steps 8-13: set resolved state
            self.current_slot = now_slot;
            self.market_mode = MarketMode::Resolved;
            self.resolved_price = resolved_price;
            self.resolved_live_price = live_oracle_price;
            self.resolved_slot = now_slot;
            self.resolved_k_long_terminal_delta = resolved_k_long_td;
            self.resolved_k_short_terminal_delta = resolved_k_short_td;
            self.clear_stress_envelope();

            // Step 13: clear resolved payout snapshot state
            self.resolved_payout_h_num = 0;
            self.resolved_payout_h_den = 0;
            self.resolved_payout_ready = 0;

            // Step 14: all positive PnL is now matured
            self.pnl_matured_pos_tot = self.pnl_pos_tot;

            // Steps 15-16: zero OI
            self.oi_eff_long_q = 0;
            self.oi_eff_short_q = 0;
            if pre_stored_long == 0 {
                self.phantom_dust_certified_long_q = 0;
                self.phantom_dust_potential_long_q = 0;
            }
            if pre_stored_short == 0 {
                self.phantom_dust_certified_short_q = 0;
                self.phantom_dust_potential_short_q = 0;
            }

            // Steps 17-20: drain/finalize sides
            if pre_mode_long != SideMode::ResetPending && pre_stored_long > 0 {
                self.begin_full_drain_reset(Side::Long)?;
            }
            if pre_mode_short != SideMode::ResetPending && pre_stored_short > 0 {
                self.begin_full_drain_reset(Side::Short)?;
            }
            if self.side_mode_long == SideMode::ResetPending
                && self.stale_account_count_long == 0
                && self.stored_pos_count_long == 0
            {
                self.finalize_side_reset(Side::Long)?;
            }
            if self.side_mode_short == SideMode::ResetPending
                && self.stale_account_count_short == 0
                && self.stored_pos_count_short == 0
            {
                self.finalize_side_reset(Side::Short)?;
            }

            // Step 21: resolve additionally requires both sides == 0 (stronger
            // than bilateral balance).
            if self.oi_eff_long_q != 0 || self.oi_eff_short_q != 0 {
                return Err(RiskError::CorruptState);
            }

            self.assert_public_postconditions()?;
            Ok(())
        })
    }

    /// Combined convenience: reconcile + terminal close if ready.
    /// For pnl <= 0 accounts or terminal-ready markets, completes in one call
    /// and returns `ResolvedCloseResult::Closed(capital)`.
    /// For positive-PnL on non-terminal markets, reconciliation persists and
    /// `ResolvedCloseResult::ProgressOnly` is returned (account stays open —
    /// re-call after terminal readiness is reached).
    pub fn force_close_resolved_not_atomic(&mut self, idx: u16) -> Result<ResolvedCloseResult> {
        self.force_close_resolved_with_fee_not_atomic(idx, 0)
    }

    /// Force-close an account on a resolved market. Uses `self.resolved_slot`
    /// as the time anchor (no slot argument).
    ///
    /// Settles K-pair PnL, zeros position, settles losses, absorbs from
    /// insurance, converts profit (bypassing warmup), sweeps fee debt,
    /// forgives remainder, returns capital, frees slot.
    ///
    /// Skips accrue_market_to (market is frozen). Handles both same-epoch
    /// and epoch-mismatch accounts.
    pub fn force_close_resolved_with_fee_not_atomic(
        &mut self,
        idx: u16,
        fee_rate_per_slot: u128,
    ) -> Result<ResolvedCloseResult> {
        rollback_on_error!(self, [idx], {
            // Phase 1: always reconcile (persists on success)
            self.reconcile_resolved_not_atomic(idx)?;

            let i = idx as usize;

            // Finalize any sides that are fully ready for reopening
            self.maybe_finalize_ready_reset_sides();
            self.sync_account_fee_to_slot(i, self.resolved_slot, fee_rate_per_slot)?;
            self.assert_public_postconditions()?;

            // pnl <= 0: can close immediately (loser/zero — no payout gate)
            // pnl > 0: needs terminal readiness for payout
            if self.accounts[i].pnl > 0 && !self.is_terminal_ready() {
                // Reconciled but not yet payable. Progress persisted.
                return Ok(ResolvedCloseResult::ProgressOnly);
            }

            // Phase 2: terminal close
            let capital = self.close_resolved_terminal_not_atomic(idx)?;
            Ok(ResolvedCloseResult::Closed(capital))
        })
    }

    /// Phase 1: Reconcile a resolved account. Materializes K-pair PnL,
    /// zeroes position, settles losses, absorbs insurance. Always persists
    /// on success. Idempotent on already-reconciled accounts.
    pub fn reconcile_resolved_not_atomic(&mut self, idx: u16) -> Result<()> {
        rollback_on_error!(self, [idx], {
            if self.market_mode != MarketMode::Resolved {
                return Err(RiskError::Unauthorized);
            }
            self.validate_touched_account_shape(idx as usize)?;
            // Recurring maintenance-fee ordering is a WRAPPER responsibility.
            // The engine provides `sync_account_fee_to_slot_not_atomic` as a
            // primitive for wrappers that enable fees, but does not enforce
            // "fees synced before resolved close" — the engine has no way to
            // distinguish "wrapper has rate=0, no sync needed" from "wrapper
            // has rate>0 and forgot". A deployment with recurring fees MUST
            // call sync on every account before `reconcile_resolved_not_atomic`
            // / `force_close_resolved_not_atomic` /
            // `close_resolved_terminal_not_atomic`.
            //
            // Spec §9.9 step 3: require current_slot == resolved_slot.
            // resolve_market sets current_slot = resolved_slot; subsequent
            // resolved-mode instructions MUST preserve that invariant rather
            // than silently repair it. If they diverge, that's a symptom of
            // post-resolution slot mutation from some other path, and it
            // should surface as an error rather than be masked.
            if self.current_slot != self.resolved_slot {
                return Err(RiskError::CorruptState);
            }
            let i = idx as usize;

            // Always clear reserve metadata (even flat accounts may have ghost bucket flags)
            self.prepare_account_for_resolved_touch(i);

            if self.accounts[i].position_basis_q != 0 {
                let basis = self.accounts[i].position_basis_q;
                let abs_basis = basis.unsigned_abs();
                let a_basis = self.accounts[i].adl_a_basis;
                if a_basis == 0 {
                    return Err(RiskError::CorruptState);
                }
                let k_snap = self.accounts[i].adl_k_snap;
                let f_snap_acct = self.accounts[i].f_snap;
                let side = side_of_i128(basis).unwrap();
                let epoch_snap = self.accounts[i].adl_epoch_snap;
                let epoch_side = self.get_epoch_side(side);

                // Resolved reconciliation uses K_epoch_start + resolved_k_terminal_delta
                // as the target K (spec §5.4 steps 6-7). F uses F_epoch_start.
                // All accounts are stale after resolution (epoch mismatch).
                let resolved_k_td = match side {
                    Side::Long => self.resolved_k_long_terminal_delta,
                    Side::Short => self.resolved_k_short_terminal_delta,
                };
                let den = a_basis.checked_mul(POS_SCALE).ok_or(RiskError::Overflow)?;
                let pnl_delta = if epoch_snap == epoch_side {
                    // Same-epoch with nonzero basis in resolved mode is corrupt state.
                    // After resolution, all nonzero-basis accounts must be stale.
                    return Err(RiskError::CorruptState);
                } else {
                    // Stale (normal resolved path): require one-epoch lag
                    if epoch_snap.checked_add(1) != Some(epoch_side) {
                        return Err(RiskError::CorruptState);
                    }
                    if self.get_stale_count(side) == 0 {
                        return Err(RiskError::CorruptState);
                    }
                    // K_epoch_start + terminal delta in wide I256.
                    // The terminal K sum may exceed i128; the wide helper handles this exactly.
                    let k_terminal_wide = I256::from_i128(self.get_k_epoch_start(side))
                        .checked_add(I256::from_i128(resolved_k_td))
                        .ok_or(RiskError::Overflow)?;
                    let f_end_wide = I256::from_i128(self.get_f_epoch_start(side));
                    Self::compute_kf_pnl_delta_wide(
                        abs_basis,
                        k_snap,
                        k_terminal_wide,
                        f_snap_acct,
                        f_end_wide,
                        den,
                    )?
                };
                let new_pnl = self.accounts[i]
                    .pnl
                    .checked_add(pnl_delta)
                    .ok_or(RiskError::Overflow)?;
                if new_pnl == i128::MIN {
                    return Err(RiskError::Overflow);
                }

                // MUTATE (prepare already called above, epoch validated above)
                if pnl_delta != 0 {
                    self.set_pnl(i, new_pnl)?;
                    self.pnl_matured_pos_tot = self.pnl_pos_tot;
                }
                if epoch_snap != epoch_side {
                    let old_stale = self.get_stale_count(side);
                    self.set_stale_count(
                        side,
                        old_stale.checked_sub(1).ok_or(RiskError::CorruptState)?,
                    );
                }
                self.clear_position_basis_q(i)?;
            }

            self.settle_losses(i)?;
            self.resolve_flat_negative(i)?;
            self.maybe_finalize_ready_reset_sides();

            self.assert_public_postconditions()?;
            Ok(())
        })
    }

    /// Check if resolved market is terminal-ready for payouts.
    /// Uses O(1) neg_pnl_account_count instead of an account scan.
    ///
    /// Defense-in-depth: the payout-snapshot-ready flag is not trusted in
    /// isolation. Even when `resolved_payout_ready != 0`, all three
    /// counters are re-checked. This makes readiness fail-conservative:
    /// a corrupt ready flag alone cannot unlock terminal payout if the
    /// stored / stale / negative-PnL counters still say otherwise.
    pub fn is_terminal_ready(&self) -> bool {
        if self.market_mode != MarketMode::Resolved {
            return false;
        }
        // All positions zeroed
        if self.stored_pos_count_long != 0 || self.stored_pos_count_short != 0 {
            return false;
        }
        // All stale accounts reconciled
        if self.stale_account_count_long != 0 || self.stale_account_count_short != 0 {
            return false;
        }
        // No negative PnL accounts remaining (spec §4.7).
        if self.neg_pnl_account_count != 0 {
            return false;
        }
        if self.pnl_matured_pos_tot != self.pnl_pos_tot {
            return false;
        }
        // All counters agree: market is ready. The payout_ready flag is a
        // one-way latch: once set, the snapshot h_num/h_den is locked for
        // all remaining positive payouts. We accept either latch-set or
        // counters-agree as "ready" — both imply a consistent view.
        true
    }

    /// Phase 2: Terminal close. Requires terminal readiness.
    pub fn close_resolved_terminal_not_atomic(&mut self, idx: u16) -> Result<u128> {
        self.close_resolved_terminal_with_fee_not_atomic(idx, 0)
    }

    pub fn close_resolved_terminal_with_fee_not_atomic(
        &mut self,
        idx: u16,
        fee_rate_per_slot: u128,
    ) -> Result<u128> {
        rollback_on_error!(self, [idx], {
            if self.market_mode != MarketMode::Resolved {
                return Err(RiskError::Unauthorized);
            }
            self.validate_touched_account_shape(idx as usize)?;
            // Spec §9.9 step 3: resolved-market instructions MUST run at the
            // frozen anchor slot. reconcile_resolved_not_atomic enforces this;
            // terminal close does too — a post-resolution drift of current_slot
            // is corruption, not recoverable state.
            if self.current_slot != self.resolved_slot {
                return Err(RiskError::CorruptState);
            }
            let i = idx as usize;
            // Reject unreconciled accounts: position must be zeroed, PnL >= 0
            if self.accounts[i].position_basis_q != 0 {
                return Err(RiskError::Undercollateralized);
            }
            if self.accounts[i].pnl < 0 {
                // Negative PnL means losses not yet absorbed — must reconcile first
                return Err(RiskError::Undercollateralized);
            }
            if self.accounts[i].pnl > 0 && !self.is_terminal_ready() {
                return Err(RiskError::Unauthorized);
            }
            self.sync_account_fee_to_slot(i, self.resolved_slot, fee_rate_per_slot)?;

            // Canonicalize reserve metadata before free_slot.
            self.prepare_account_for_resolved_touch(i);
            if self.accounts[i].pnl > 0 {
                if self.resolved_payout_ready == 0 {
                    self.pnl_matured_pos_tot = self.pnl_pos_tot;
                    let senior = self
                        .c_tot
                        .get()
                        .checked_add(self.insurance_fund.balance.get())
                        .unwrap_or(u128::MAX);
                    let residual = if self.vault.get() >= senior {
                        self.vault.get() - senior
                    } else {
                        0u128
                    };
                    let h_den = self.pnl_matured_pos_tot;
                    let h_num = if h_den == 0 {
                        0
                    } else {
                        core::cmp::min(residual, h_den)
                    };
                    self.resolved_payout_h_num = h_num;
                    self.resolved_payout_h_den = h_den;
                    self.resolved_payout_ready = 1;
                }
                // prepare_account_for_resolved_touch already cleared reserve to 0;
                // assert the invariant explicitly as defense-in-depth before using
                // resolved released-PnL view.
                if self.accounts[i].reserved_pnl != 0 {
                    return Err(RiskError::CorruptState);
                }
                let released = self.released_pos_checked(i, false)?;
                if released > 0 {
                    // Spec forbids h_den==0 with positive released PnL when snapshot is ready.
                    if self.resolved_payout_h_den == 0 {
                        return Err(RiskError::CorruptState);
                    }
                    let y = wide_mul_div_floor_u128(
                        released,
                        self.resolved_payout_h_num,
                        self.resolved_payout_h_den,
                    );
                    // Canonical resolved-close path (spec): set_pnl_with_reserve to
                    // zero the account's PnL with NoPositiveIncreaseAllowed, then
                    // credit the haircutted payout y to capital. Unlike
                    // consume_released_pnl (which is a Live-mode matured-drain
                    // helper), this uses the same canonical PnL mutation primitive
                    // as the rest of the engine.
                    self.set_pnl_with_reserve(
                        i,
                        0i128,
                        ReserveMode::NoPositiveIncreaseAllowed,
                        None,
                    )?;
                    let new_cap = self.accounts[i]
                        .capital
                        .get()
                        .checked_add(y)
                        .ok_or(RiskError::Overflow)?;
                    self.set_capital(i, new_cap)?;
                }
            }
            self.fee_debt_sweep(i)?;
            self.validate_fee_credits_shape(i)?;
            let fc = self.accounts[i].fee_credits.get();
            if fc < 0 {
                self.accounts[i].fee_credits = I128::ZERO;
            }
            let capital = self.accounts[i].capital;
            if capital > self.vault {
                return Err(RiskError::InsufficientBalance);
            }
            self.vault = self.vault - capital;
            self.set_capital(i, 0)?;
            self.free_slot(idx)?;
            self.sweep_empty_market_surplus_to_insurance()?;

            self.assert_public_postconditions()?;
            Ok(capital.get())
        })
    }

    // ========================================================================
    // Permissionless account reclamation (spec §10.7 + §2.6)
    // ========================================================================

    /// reclaim_empty_account_not_atomic(i, now_slot) — permissionless O(1) empty/dust-account recycling.
    /// Spec §10.7: MUST NOT call accrue_market_to, MUST NOT mutate side state,
    /// MUST NOT materialize any account.
    pub fn reclaim_empty_account_not_atomic(&mut self, idx: u16, now_slot: u64) -> Result<()> {
        rollback_on_error!(self, [idx], {
            if self.market_mode != MarketMode::Live {
                return Err(RiskError::Unauthorized);
            }
            if now_slot < self.current_slot {
                return Err(RiskError::Overflow);
            }
            self.validate_touched_account_shape_at_fee_slot(idx as usize, now_slot)?;
            // Reject time jumps that would brick subsequent accrue_market_to.
            self.check_live_accrual_envelope(now_slot)?;

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
            // Require bucket metadata empty (not just reserved_pnl == 0)
            if account.sched_present != 0 || account.pending_present != 0 {
                return Err(RiskError::Undercollateralized);
            }
            self.validate_fee_credits_shape(idx as usize)?;

            // Step 4: anchor current_slot
            self.current_slot = now_slot;

            // No engine-native maintenance fee (spec §8).

            // Step 5: final reclaim-eligibility check.
            // The engine only reclaims accounts whose capital has been fully
            // drained. Wrappers that want to recycle slots with tiny residual
            // capital MUST drain that residual first (e.g., via
            // `charge_account_fee_not_atomic` to push the remainder into the
            // insurance fund). This keeps the engine's reclaim predicate a
            // single bit (`capital == 0`) and pushes any "dust threshold"
            // policy into the wrapper.
            if !self.accounts[idx as usize].capital.is_zero() {
                return Err(RiskError::Undercollateralized);
            }

            // Forgive uncollectible fee debt (spec §2.6).
            self.validate_fee_credits_shape(idx as usize)?;
            let fc = self.accounts[idx as usize].fee_credits.get();
            if fc < 0 {
                self.accounts[idx as usize].fee_credits = I128::ZERO;
            }

            // Free the slot
            self.free_slot(idx)?;
            self.sweep_empty_market_surplus_to_insurance()?;

            self.assert_public_postconditions()?;
            Ok(())
        })
    }

    // ========================================================================
    // Insurance fund operations
    // ========================================================================

    /// Top up the insurance fund by `amount`.
    ///
    /// Validate-then-mutate: pre-state invariants are checked BEFORE any
    /// commit. A corrupt pre-state returns Err with no mutation.
    pub fn top_up_insurance_fund(&mut self, amount: u128, now_slot: u64) -> Result<()> {
        if self.market_mode != MarketMode::Live {
            return Err(RiskError::Unauthorized);
        }
        // Pre-state invariant check: any corruption surfaces BEFORE mutation.
        self.assert_public_postconditions()?;
        // Spec §10.3.2: time monotonicity
        if now_slot < self.current_slot {
            return Err(RiskError::Overflow);
        }
        // Reject time jumps that would brick subsequent accrue_market_to.
        self.check_live_accrual_envelope(now_slot)?;
        // Validate-then-mutate: all checks before any state change
        let new_vault = self
            .vault
            .get()
            .checked_add(amount)
            .ok_or(RiskError::Overflow)?;
        if new_vault > MAX_VAULT_TVL {
            return Err(RiskError::Overflow);
        }
        let new_ins = self
            .insurance_fund
            .balance
            .get()
            .checked_add(amount)
            .ok_or(RiskError::Overflow)?;
        // All checks passed — commit
        self.current_slot = now_slot;
        self.vault = U128::new(new_vault);
        self.insurance_fund.balance = U128::new(new_ins);
        // Post-state sanity check (belt-and-suspenders; should be no-op
        // if pre-check passed and the math is correct).
        self.assert_public_postconditions()?;
        Ok(())
    }

    /// Move insurance balance into an existing account's capital without
    /// changing vault size. Intended for wrapper-level incentives that are paid
    /// out of already-collected insurance funds.
    pub fn credit_account_from_insurance_not_atomic(
        &mut self,
        idx: u16,
        amount: u128,
        now_slot: u64,
    ) -> Result<()> {
        rollback_on_error!(self, [idx], {
            if self.market_mode != MarketMode::Live {
                return Err(RiskError::Unauthorized);
            }
            if now_slot < self.current_slot {
                return Err(RiskError::Overflow);
            }
            self.validate_touched_account_shape_at_fee_slot(idx as usize, now_slot)?;
            self.assert_public_postconditions()?;
            self.check_live_accrual_envelope(now_slot)?;
            let ins = self.insurance_fund.balance.get();
            if amount > ins {
                return Err(RiskError::InsufficientBalance);
            }
            let new_cap = self.accounts[idx as usize]
                .capital
                .get()
                .checked_add(amount)
                .ok_or(RiskError::Overflow)?;

            self.current_slot = now_slot;
            self.insurance_fund.balance = U128::new(ins - amount);
            self.set_capital(idx as usize, new_cap)?;

            if self.accounts[idx as usize].position_basis_q == 0
                && self.accounts[idx as usize].pnl >= 0
            {
                self.fee_debt_sweep(idx as usize)?;
            }

            self.assert_public_postconditions()?;
            Ok(())
        })
    }

    /// Withdraw insurance from a live market. The wrapper owns authorization
    /// and rate limits; the engine owns canonical accounting.
    pub fn withdraw_live_insurance_not_atomic(
        &mut self,
        amount: u128,
        now_slot: u64,
    ) -> Result<()> {
        rollback_on_error!(self, [], {
            if self.market_mode != MarketMode::Live {
                return Err(RiskError::Unauthorized);
            }
            self.assert_public_postconditions()?;
            if now_slot < self.current_slot {
                return Err(RiskError::Overflow);
            }
            self.check_live_accrual_envelope(now_slot)?;
            let ins = self.insurance_fund.balance.get();
            if amount > ins {
                return Err(RiskError::InsufficientBalance);
            }
            let vault_next = self
                .vault
                .get()
                .checked_sub(amount)
                .ok_or(RiskError::CorruptState)?;

            self.current_slot = now_slot;
            self.insurance_fund.balance = U128::new(ins - amount);
            self.vault = U128::new(vault_next);
            self.assert_public_postconditions()?;
            Ok(())
        })
    }

    /// Withdraw all terminal insurance from a resolved, empty market. This also
    /// folds any empty-market rounding surplus into insurance before draining.
    pub fn withdraw_resolved_insurance_not_atomic(&mut self) -> Result<u128> {
        rollback_on_error!(self, [], {
            if self.market_mode != MarketMode::Resolved {
                return Err(RiskError::Unauthorized);
            }
            self.assert_public_postconditions()?;
            if self.num_used_accounts != 0 {
                return Err(RiskError::Unauthorized);
            }
            self.sweep_empty_market_surplus_to_insurance()?;
            let payout = self.insurance_fund.balance.get();
            if payout == 0 {
                return Ok(0);
            }
            let vault_next = self
                .vault
                .get()
                .checked_sub(payout)
                .ok_or(RiskError::CorruptState)?;
            self.insurance_fund.balance = U128::ZERO;
            self.vault = U128::new(vault_next);
            self.assert_public_postconditions()?;
            Ok(payout)
        })
    }

    // ========================================================================
    // Account fees (wrapper-owned)
    // ========================================================================

    /// charge_account_fee_not_atomic: public pure one-shot fee instruction.
    ///
    /// USE FOR: ad-hoc wrapper-owned charges (e.g., manual adjustments,
    /// one-time penalties). The engine does NOT track which interval this
    /// represents.
    ///
    /// DO NOT USE FOR recurring time-based fees. The canonical recurring
    /// path is `sync_account_fee_to_slot_not_atomic` which reads and
    /// advances `last_fee_slot` atomically. Mixing these two APIs for the
    /// same economic interval will double-charge — this method leaves
    /// `last_fee_slot` unchanged, so a subsequent sync call will re-charge
    /// the same dt.
    ///
    /// Only mutates: C_i, fee_credits_i, I, C_tot, current_slot.
    /// Never calls accrue_market_to or touches PNL, reserves, A/K, OI,
    /// side modes, stale counters, or dust bounds.
    ///
    /// Fee beyond collectible headroom is dropped (not socialized).
    pub fn charge_account_fee_not_atomic(
        &mut self,
        idx: u16,
        fee_abs: u128,
        now_slot: u64,
    ) -> Result<()> {
        rollback_on_error!(self, [idx], {
            if self.market_mode != MarketMode::Live {
                return Err(RiskError::Unauthorized);
            }
            if now_slot < self.current_slot {
                return Err(RiskError::Overflow);
            }
            self.validate_touched_account_shape_at_fee_slot(idx as usize, now_slot)?;
            // Reject time jumps that would brick subsequent accrue_market_to.
            self.check_live_accrual_envelope(now_slot)?;
            if fee_abs > MAX_PROTOCOL_FEE_ABS {
                return Err(RiskError::Overflow);
            }
            if fee_abs > 0 {
                self.ensure_fee_draw_does_not_precede_loss(idx as usize)?;
            }

            self.current_slot = now_slot;

            if fee_abs > 0 {
                self.charge_fee_to_insurance(idx as usize, fee_abs)?;
            }

            self.assert_public_postconditions()?;
            Ok(())
        })
    }

    // ========================================================================
    // Fee credits
    // ========================================================================
    // settle_flat_negative_pnl (spec §10.8)
    // ========================================================================

    /// Lightweight permissionless instruction to resolve flat accounts with
    /// negative PnL. Does NOT call accrue_market_to. Only absorbs the
    /// negative PnL through insurance and zeroes it.
    ///
    /// Preconditions: account is flat (position_basis_q == 0) and pnl < 0.
    pub fn settle_flat_negative_pnl_not_atomic(&mut self, idx: u16, now_slot: u64) -> Result<()> {
        rollback_on_error!(self, [idx], {
            if self.market_mode != MarketMode::Live {
                return Err(RiskError::Unauthorized);
            }
            if now_slot < self.current_slot {
                return Err(RiskError::Overflow);
            }
            self.validate_touched_account_shape_at_fee_slot(idx as usize, now_slot)?;
            // Reject time jumps that would brick subsequent accrue_market_to.
            self.check_live_accrual_envelope(now_slot)?;
            let i = idx as usize;
            // Flat only, reserve state empty
            if self.accounts[i].position_basis_q != 0 {
                return Err(RiskError::Undercollateralized);
            }
            if self.accounts[i].reserved_pnl != 0
                || self.accounts[i].sched_present != 0
                || self.accounts[i].pending_present != 0
            {
                return Err(RiskError::Undercollateralized);
            }
            // Spec §9.2.4 step 4: set current_slot = now_slot BEFORE the
            // pnl >= 0 early-return. A successful no-op still advances the
            // engine's time anchor; without this, a caller can see inconsistent
            // `current_slot` behavior between no-op and full-path invocations.
            self.current_slot = now_slot;

            // Noop if PnL >= 0 (spec §9.2.4 step 6-7).
            if self.accounts[i].pnl >= 0 {
                self.assert_public_postconditions()?;
                return Ok(());
            }

            // Settle losses from principal first, then absorb remaining via insurance
            self.settle_losses(i)?;
            self.resolve_flat_negative(i)?;

            self.assert_public_postconditions()?;
            Ok(())
        })
    }

    // ========================================================================
    // sync_account_fee_to_slot_not_atomic (spec §4.6.1)
    // ========================================================================

    /// Public entrypoint for wrapper-owned recurring-fee realization.
    ///
    /// Wrappers that enable recurring maintenance fees MUST call this before
    /// any health-sensitive engine operation on the same Solana transaction
    /// (spec §9.0 step 5). Solana transaction atomicity guarantees the sync
    /// and the subsequent operation commit together or roll back together.
    ///
    /// The public entrypoint does NOT accept an arbitrary `fee_slot_anchor`.
    /// The engine picks the anchor deterministically:
    ///
    /// - On Live nonflat accounts: `fee_slot_anchor <= last_market_slot`
    ///   whenever `now_slot` is ahead of loss accrual, so recurring fees
    ///   cannot become senior to unapplied mark/funding losses.
    /// - On Live flat/current accounts: `fee_slot_anchor = now_slot`.
    /// - On Resolved: `fee_slot_anchor = resolved_slot`.
    ///
    /// Charges exactly once over `[last_fee_slot, fee_slot_anchor]`. A
    /// second call with `now_slot == current_slot` is a no-op. Newly
    /// materialized accounts start at their materialization slot and are
    /// never back-charged.
    ///
    /// The internal `sync_account_fee_to_slot` helper (which accepts an
    /// explicit anchor) remains available for tests and Kani proofs but
    /// is not part of the public engine surface.
    pub fn sync_account_fee_to_slot_not_atomic(
        &mut self,
        idx: u16,
        now_slot: u64,
        fee_rate_per_slot: u128,
    ) -> Result<()> {
        rollback_on_error!(self, [idx], {
            if now_slot < self.current_slot {
                return Err(RiskError::Overflow);
            }
            let live_loss_safe_anchor = if self.market_mode == MarketMode::Live
                && now_slot > self.last_market_slot
                && (idx as usize) < MAX_ACCOUNTS
                && self.accounts[idx as usize].position_basis_q != 0
            {
                self.last_market_slot
            } else {
                now_slot
            };
            let shape_anchor = match self.market_mode {
                MarketMode::Live => live_loss_safe_anchor,
                MarketMode::Resolved => self.resolved_slot,
            };
            self.validate_touched_account_shape_at_fee_slot(idx as usize, shape_anchor)?;
            if fee_rate_per_slot > 0 && shape_anchor > self.accounts[idx as usize].last_fee_slot {
                self.ensure_fee_draw_does_not_precede_loss(idx as usize)?;
            }
            // Reject time jumps that would brick subsequent accrue_market_to.
            // Only meaningful on Live; on Resolved the envelope is moot because
            // accrue_market_to is no longer reachable, but the check is safe
            // (last_market_slot is frozen to resolved_slot).
            if self.market_mode == MarketMode::Live && now_slot > self.current_slot {
                self.check_live_accrual_envelope(now_slot)?;
            }
            let anchor = match self.market_mode {
                MarketMode::Live => {
                    self.current_slot = now_slot;
                    live_loss_safe_anchor
                }
                MarketMode::Resolved => {
                    // Resolved fee sync is anchored at resolution.
                    self.resolved_slot
                }
            };
            self.sync_account_fee_to_slot(idx as usize, anchor, fee_rate_per_slot)?;
            self.assert_public_postconditions()?;
            Ok(())
        })
    }

    // ========================================================================
    // Public getters for wrapper use
    // ========================================================================

    /// Whether the market is in Resolved mode.
    pub fn is_resolved(&self) -> bool {
        self.market_mode == MarketMode::Resolved
    }

    /// Resolved market context (price, slot). Only meaningful when is_resolved().
    pub fn resolved_context(&self) -> (u64, u64) {
        (self.resolved_price, self.resolved_slot)
    }

    // ========================================================================
    // Fee credits
    // ========================================================================

    /// Spec §9.2.1: `pay = min(amount, FeeDebt_i)`. The engine applies at
    /// most `FeeDebt_i` to the account's fee_credits and returns the booked
    /// amount so the caller can handle any unused input.
    ///
    /// Validate-then-mutate: pre-state invariants are checked BEFORE any
    /// commit. Returns `pay`, the exact amount booked against fee_credits.
    pub fn deposit_fee_credits(&mut self, idx: u16, amount: u128, now_slot: u64) -> Result<u128> {
        if self.market_mode != MarketMode::Live {
            return Err(RiskError::Unauthorized);
        }
        if now_slot < self.current_slot {
            return Err(RiskError::Overflow);
        }
        self.validate_touched_account_shape_at_fee_slot(idx as usize, now_slot)?;
        let fc = self.accounts[idx as usize].fee_credits.get();
        // Pre-state invariant check: any corruption surfaces BEFORE mutation.
        self.assert_public_postconditions()?;
        // Reject time jumps that would brick subsequent accrue_market_to.
        self.check_live_accrual_envelope(now_slot)?;

        // Spec §9.2.1 step 5: pay = min(amount, FeeDebt_i).
        let debt = fee_debt_u128_checked(fc);
        let pay = core::cmp::min(amount, debt);
        if pay == 0 {
            // Spec step 6: if pay == 0, return with current_slot anchored.
            self.current_slot = now_slot;
            // Post-state check even on no-op: spec §9.2.1 step 12 still
            // requires V >= C_tot + I.
            self.assert_public_postconditions()?;
            return Ok(0);
        }
        if pay > i128::MAX as u128 {
            return Err(RiskError::Overflow);
        }
        let new_vault = self
            .vault
            .get()
            .checked_add(pay)
            .ok_or(RiskError::Overflow)?;
        if new_vault > MAX_VAULT_TVL {
            return Err(RiskError::Overflow);
        }
        let new_ins = self
            .insurance_fund
            .balance
            .get()
            .checked_add(pay)
            .ok_or(RiskError::Overflow)?;
        let new_credits = self.accounts[idx as usize]
            .fee_credits
            .checked_add(pay as i128)
            .ok_or(RiskError::Overflow)?;
        // All checks passed — commit state.
        self.current_slot = now_slot;
        self.vault = U128::new(new_vault);
        self.insurance_fund.balance = U128::new(new_ins);
        self.accounts[idx as usize].fee_credits = new_credits;
        self.assert_public_postconditions()?;
        Ok(pay)
    }

    // ========================================================================
    // Recompute aggregates (test helper)
    // ========================================================================

    // ========================================================================
    // Utilities
    // ========================================================================

    test_visible! {
    fn advance_slot(&mut self, slots: u64) {
        self.current_slot = self.current_slot.saturating_add(slots);
    }
    }

    #[cfg(any(feature = "test", feature = "stress", kani))]
    pub fn count_used(&self) -> u64 {
        let mut count = 0u64;
        self.for_each_used(|_, _| {
            count += 1;
        });
        count
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
    let product = U256::from_u128(a)
        .checked_mul(U256::from_u128(abs_b))
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
            Some(v) if v <= i128::MAX as u128 => Ok(-(v as i128)),
            _ => Err(RiskError::Overflow),
        }
    } else {
        match q.try_into_u128() {
            Some(v) if v <= i128::MAX as u128 => Ok(v as i128),
            _ => Err(RiskError::Overflow),
        }
    }
}
