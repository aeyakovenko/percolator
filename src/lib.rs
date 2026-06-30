//! Percolator risk engine — v16.
//!
//! v16 keeps the account-local engine surface and adds source-domain realizable
//! credit accounting so positive PnL cannot be used beyond proven source-domain
//! backing.

#![no_std]
#![deny(unsafe_code)]

extern crate alloc;

#[cfg(kani)]
extern crate kani;

pub const POS_SCALE: u128 = 1_000_000;
pub const ADL_ONE: u128 = 1_000_000_000_000_000;
pub const MIN_A_SIDE: u128 = 100_000_000_000_000;
pub const MAX_ORACLE_PRICE: u64 = 1_000_000_000_000;
pub const FUNDING_DEN: u128 = 1_000_000_000;
pub const STRESS_CONSUMPTION_SCALE: u128 = 1_000_000_000;
pub const SOCIAL_WEIGHT_SCALE: u128 = ADL_ONE;
pub const SOCIAL_LOSS_DEN: u128 = 1_000_000_000_000_000_000_000;
pub const SUPPORT_WEIGHT_SCALE: u128 = 1_000_000;
pub const FULL_SUPPORT_WEIGHT: u128 = SUPPORT_WEIGHT_SCALE;
pub const BOUND_SCALE: u128 = 1_000_000_000_000;
pub const CREDIT_RATE_SCALE: u128 = 1_000_000_000_000;
pub const MAX_VAULT_TVL: u128 = 10_000_000_000_000_000;
pub const MAX_POSITION_ABS_Q: u128 = 100_000_000_000_000;
pub const MAX_ACCOUNT_NOTIONAL: u128 = 100_000_000_000_000_000_000;
pub const MAX_TRADE_SIZE_Q: u128 = MAX_POSITION_ABS_Q;
pub const MAX_OI_SIDE_Q: u128 = 100_000_000_000_000;
pub const MAX_TRADING_FEE_BPS: u64 = 10_000;
pub const MAX_MARGIN_BPS: u64 = 10_000;
pub const MAX_LIQUIDATION_FEE_BPS: u64 = 10_000;
pub const MAX_PROTOCOL_FEE_ABS: u128 = 1_000_000_000_000_000_000_000_000_000_000_000_000;
pub const MAX_WARMUP_SLOTS: u64 = u64::MAX;
pub const MAX_RESOLVE_PRICE_DEVIATION_BPS: u64 = 10_000;
pub const MAX_RECOVERY_FALLBACK_DEVIATION_BPS: u64 = MAX_RESOLVE_PRICE_DEVIATION_BPS;

#[cfg(any(kani, feature = "fuzz"))]
pub mod v16;
#[cfg(not(any(kani, feature = "fuzz")))]
mod v16;
#[cfg(kani)]
pub mod wide_math;
#[cfg(not(kani))]
mod wide_math;

#[cfg(kani)]
pub use v16::*;
#[cfg(not(kani))]
pub use v16::{
    active_bitmap_count_ones, active_bitmap_empty, active_bitmap_get, active_bitmap_is_empty,
    auto_crank_plan_requires_caller_observation, backing_domain_fee_split_for_lien_delta_num,
    v16_domain_count_for_market_slots, v16_domain_pair_for_asset_index, AccrueAssetOutcomeV16,
    ActionableSummaryV16, AssetLifecycleV16, AssetStateV16, AssetStateV16Account,
    AutoCrankObservationV16, AutoCrankOutcomeV16, AutoCrankPlanV16, AutoCrankResultV16,
    AutoCrankWorkV16, BackingBucketStatusV16, BackingBucketV16, BackingBucketV16Account,
    BackingDomainFeeSplitV16, BatchTradeOutcomeV16, CloseProgressLedgerV16,
    CloseProgressLedgerV16Account, DeadLegForfeitOutcomeV16, EngineAssetSlotV16Account,
    HealthCertV16, HealthCertV16Account, InsuranceCreditReservationV16,
    InsuranceCreditReservationV16Account, LiquidationOutcomeV16, LiquidationRequestV16, Market,
    MarketGroupV16HeaderAccount, MarketGroupV16View, MarketGroupV16ViewMut, MarketModeV16,
    MarketSlotV16View, MarketSlotV16ViewMut, PermissionlessCrankActionV16,
    PermissionlessCrankRequestV16, PermissionlessProgressOutcomeV16,
    PermissionlessRecoveryReasonV16, PortfolioAccountV16Account, PortfolioLegV16,
    PortfolioLegV16Account, PortfolioSourceDomainV16Account, PortfolioV16View, PortfolioV16ViewMut,
    ProvenanceHeaderV16, ProvenanceHeaderV16Account, RebalanceOutcomeV16, RebalanceRequestV16,
    ResolvedCloseOutcomeV16, ResolvedPayoutLedgerV16, ResolvedPayoutLedgerV16Account,
    ResolvedPayoutReceiptV16, ResolvedPayoutReceiptV16Account, SideModeV16, SideV16,
    SourceCreditStateV16, SourceCreditStateV16Account, TradeRequestV16, V16ActiveBitmap, V16Config,
    V16ConfigAccount, V16Error, V16OptionalRecoveryReasonAccount, V16PodI128, V16PodU128,
    V16PodU32, V16PodU64, V16Result, PORTFOLIO_SOURCE_DOMAIN_CAP, V16_EMPTY_ACTIVE_BITMAP,
    V16_MAX_PORTFOLIO_ASSETS_N,
};
