//! Insurance fund for covering residual bad debt
//!
//! This module implements a minimal insurance fund that:
//! - Accrues fees from taker trades
//! - Pays out to cover bad debt after liquidations
//! - Enforces per-event and daily payout caps
//! - Tracks uncovered bad debt for telemetry

/// Insurance fund parameters (configurable by governance)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct InsuranceParams {
    /// Basis points of taker fees to insurance (e.g., 10 = 0.10%)
    pub fee_bps_to_insurance: u16,
    /// Max payout as bps of event notional (e.g., 50 = 0.50%)
    pub max_payout_bps_of_oi: u16,
    /// Max daily payout as bps of vault balance (e.g., 300 = 3%)
    pub max_daily_payout_bps_of_vault: u16,
    /// Cooldown between payouts for same instrument (optional, can be 0)
    pub cooloff_secs: u32,
}

impl Default for InsuranceParams {
    fn default() -> Self {
        Self {
            fee_bps_to_insurance: 10,           // 0.10% of taker fees
            max_payout_bps_of_oi: 50,           // 0.50% of event notional cap
            max_daily_payout_bps_of_vault: 300, // 3% of vault per day
            cooloff_secs: 0,                     // No cooldown for v0
        }
    }
}

/// Insurance fund state (tracking balances and limits)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct InsuranceState {
    /// Cached vault balance (updated on accrual/payout)
    pub vault_balance: u128,
    /// Last payout timestamp
    pub last_payout_ts: u64,
    /// Daily payout accumulator (resets on day rollover)
    pub daily_payout_accum: u128,
    /// Total payouts since inception
    pub total_payouts: u128,
    /// Total fees accrued since inception
    pub total_fees_accrued: u128,
    /// Uncovered bad debt (tracked for telemetry)
    pub uncovered_bad_debt: u128,
    /// Last day (for rollover detection)
    last_day: u64,
    /// Vault balance at start of current day (for daily cap calculation)
    day_start_vault_balance: u128,
}

impl Default for InsuranceState {
    fn default() -> Self {
        Self {
            vault_balance: 0,
            last_payout_ts: 0,
            daily_payout_accum: 0,
            total_payouts: 0,
            total_fees_accrued: 0,
            uncovered_bad_debt: 0,
            last_day: 0,
            day_start_vault_balance: 0,
        }
    }
}

impl InsuranceState {
    /// Check if it's a new day (for daily limit reset)
    fn is_new_day(&self, now: u64) -> bool {
        const SECONDS_PER_DAY: u64 = 86400;
        let current_day = now / SECONDS_PER_DAY;
        current_day > self.last_day
    }

    /// Accrue insurance fees from a trade (using verified math)
    ///
    /// Called during fill processing to siphon a % of notional to insurance fund.
    ///
    /// # Arguments
    /// * `notional` - Trade notional (qty * price, in base units)
    /// * `params` - Insurance parameters
    ///
    /// # Returns
    /// Amount to transfer to insurance vault
    ///
    /// # Safety
    ///
    /// Uses formally verified saturating arithmetic from model_safety::math
    /// to prevent overflow/underflow bugs.
    pub fn accrue_from_fill(&mut self, notional: u128, params: &InsuranceParams) -> u128 {
        use model_safety::math::{mul_u128, div_u128, add_u128};

        // Calculate accrual = (notional * fee_bps) / 10_000
        // Use verified math to prevent overflow
        let numerator = mul_u128(notional, params.fee_bps_to_insurance as u128);
        let accrual = div_u128(numerator, 10_000);

        // Update balances using verified saturating addition
        self.vault_balance = add_u128(self.vault_balance, accrual);
        self.total_fees_accrued = add_u128(self.total_fees_accrued, accrual);

        accrual
    }

    /// Settle bad debt after liquidation
    ///
    /// Called at the end of liquidation if user equity < 0.
    /// Pays out from insurance vault to cover the shortfall, subject to caps.
    ///
    /// # Arguments
    /// * `bad_debt` - Absolute value of negative equity (in collateral units)
    /// * `event_notional` - Sum of liquidation fill notionals (for per-event cap)
    /// * `params` - Insurance parameters
    /// * `now` - Current timestamp
    ///
    /// # Returns
    /// `(payout, uncovered)` tuple where:
    /// - `payout` is the amount transferred from insurance vault
    /// - `uncovered` is the remaining bad debt not covered
    pub fn settle_bad_debt(
        &mut self,
        bad_debt: u128,
        event_notional: u128,
        params: &InsuranceParams,
        now: u64,
    ) -> (u128, u128) {
        // Daily cap housekeeping
        const SECONDS_PER_DAY: u64 = 86400;
        let current_day = now / SECONDS_PER_DAY;
        if self.is_new_day(now) {
            self.daily_payout_accum = 0;
            self.last_day = current_day;
            // Snapshot vault balance at start of new day
            self.day_start_vault_balance = self.vault_balance;
        }

        // Initialize day_start_vault_balance on first call
        if self.day_start_vault_balance == 0 && self.vault_balance > 0 {
            self.day_start_vault_balance = self.vault_balance;
        }

        use model_safety::math::{mul_u128, div_u128, sub_u128, add_u128, min_u128};

        // Calculate caps (use day-start vault balance for daily cap)
        let daily_cap_numerator = mul_u128(
            self.day_start_vault_balance,
            params.max_daily_payout_bps_of_vault as u128,
        );
        let daily_cap = div_u128(daily_cap_numerator, 10_000);
        let remaining_daily = sub_u128(daily_cap, self.daily_payout_accum);

        let per_event_cap_numerator = mul_u128(event_notional, params.max_payout_bps_of_oi as u128);
        let per_event_cap = div_u128(per_event_cap_numerator, 10_000);

        // Max allowed payout is minimum of: vault balance, per-event cap, remaining daily cap
        let max_allowed = min_u128(
            min_u128(self.vault_balance, per_event_cap),
            remaining_daily,
        );

        // Actual payout is minimum of bad debt and max allowed
        let payout = min_u128(bad_debt, max_allowed);

        // Update state using verified arithmetic
        if payout > 0 {
            self.vault_balance = sub_u128(self.vault_balance, payout);
            self.daily_payout_accum = add_u128(self.daily_payout_accum, payout);
            self.total_payouts = add_u128(self.total_payouts, payout);
            self.last_payout_ts = now;
        }

        // Calculate uncovered amount
        let uncovered = sub_u128(bad_debt, payout);
        if uncovered > 0 {
            self.uncovered_bad_debt = add_u128(self.uncovered_bad_debt, uncovered);
        }

        (payout, uncovered)
    }

    /// Manual top-up of insurance vault (governance only)
    ///
    /// # Safety
    ///
    /// Uses formally verified arithmetic to prevent overflow.
    pub fn top_up(&mut self, amount: u128) {
        use model_safety::math::add_u128;

        self.vault_balance = add_u128(self.vault_balance, amount);
        self.total_fees_accrued = add_u128(self.total_fees_accrued, amount);
    }

    /// Withdraw surplus (governance only, requires uncovered_bad_debt == 0)
    ///
    /// # Safety
    ///
    /// Uses formally verified arithmetic to prevent underflow.
    pub fn withdraw_surplus(&mut self, amount: u128) -> Result<(), ()> {
        use model_safety::math::sub_u128;

        if self.uncovered_bad_debt > 0 {
            return Err(()); // Cannot withdraw while there's uncovered debt
        }
        if self.vault_balance < amount {
            return Err(()); // Insufficient balance
        }
        self.vault_balance = sub_u128(self.vault_balance, amount);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_params() {
        let params = InsuranceParams::default();
        assert_eq!(params.fee_bps_to_insurance, 10);
        assert_eq!(params.max_payout_bps_of_oi, 50);
        assert_eq!(params.max_daily_payout_bps_of_vault, 300);
    }

    #[test]
    fn test_accrue_from_fill() {
        let mut state = InsuranceState::default();
        let params = InsuranceParams::default();

        // Accrue from 1M notional trade (0.10% = 1000)
        let accrual = state.accrue_from_fill(1_000_000, &params);
        assert_eq!(accrual, 1000); // 1M * 10 / 10000 = 1000
        assert_eq!(state.vault_balance, 1000);
        assert_eq!(state.total_fees_accrued, 1000);
    }

    #[test]
    fn test_settle_bad_debt_full_coverage() {
        let mut state = InsuranceState::default();
        let mut params = InsuranceParams::default();
        params.max_daily_payout_bps_of_vault = 10000; // 100% - no daily cap for this test

        // Fund the vault
        state.vault_balance = 10_000;

        // Settle 5000 bad debt (fully covered)
        let (payout, uncovered) = state.settle_bad_debt(5_000, 1_000_000, &params, 1000);

        assert_eq!(payout, 5_000); // Full coverage
        assert_eq!(uncovered, 0);
        assert_eq!(state.vault_balance, 5_000); // 10000 - 5000
        assert_eq!(state.total_payouts, 5_000);
        assert_eq!(state.uncovered_bad_debt, 0);
    }

    #[test]
    fn test_settle_bad_debt_partial_coverage_vault_limit() {
        let mut state = InsuranceState::default();
        let mut params = InsuranceParams::default();
        params.max_daily_payout_bps_of_vault = 10000; // 100% - no daily cap for this test

        // Fund the vault with less than bad debt
        state.vault_balance = 3_000;

        // Settle 5000 bad debt (partially covered by vault limit)
        let (payout, uncovered) = state.settle_bad_debt(5_000, 1_000_000, &params, 1000);

        assert_eq!(payout, 3_000); // Limited by vault balance
        assert_eq!(uncovered, 2_000); // 5000 - 3000
        assert_eq!(state.vault_balance, 0);
        assert_eq!(state.total_payouts, 3_000);
        assert_eq!(state.uncovered_bad_debt, 2_000);
    }

    #[test]
    fn test_settle_bad_debt_per_event_cap() {
        let mut state = InsuranceState::default();
        let mut params = InsuranceParams::default();
        params.max_daily_payout_bps_of_vault = 10000; // 100% - no daily cap for this test

        // Fund the vault with plenty
        state.vault_balance = 100_000;

        // Settle 10000 bad debt with 1M event notional
        // Per-event cap = 1M * 50 / 10000 = 5000
        let (payout, uncovered) = state.settle_bad_debt(10_000, 1_000_000, &params, 1000);

        assert_eq!(payout, 5_000); // Limited by per-event cap
        assert_eq!(uncovered, 5_000); // 10000 - 5000
        assert_eq!(state.vault_balance, 95_000);
        assert_eq!(state.uncovered_bad_debt, 5_000);
    }

    #[test]
    fn test_settle_bad_debt_daily_cap() {
        let mut state = InsuranceState::default();
        let params = InsuranceParams::default();

        // Fund the vault
        state.vault_balance = 100_000;

        // Daily cap = 100000 * 300 / 10000 = 3000
        // First payout: 2000
        let (payout1, _) = state.settle_bad_debt(2_000, 1_000_000, &params, 1000);
        assert_eq!(payout1, 2_000);
        assert_eq!(state.daily_payout_accum, 2_000);

        // Second payout in same day: 2000 requested, but only 1000 remaining in daily cap
        let (payout2, uncovered2) = state.settle_bad_debt(2_000, 1_000_000, &params, 1000);
        assert_eq!(payout2, 1_000); // Limited by remaining daily cap
        assert_eq!(uncovered2, 1_000);
        assert_eq!(state.daily_payout_accum, 3_000);
    }

    #[test]
    fn test_daily_cap_reset() {
        let mut state = InsuranceState::default();
        let params = InsuranceParams::default();

        state.vault_balance = 100_000;

        // First day payout
        state.settle_bad_debt(2_000, 1_000_000, &params, 1000);
        assert_eq!(state.daily_payout_accum, 2_000);

        // Next day (86400 seconds later)
        let (payout, _) = state.settle_bad_debt(2_000, 1_000_000, &params, 87400);
        assert_eq!(payout, 2_000);
        assert_eq!(state.daily_payout_accum, 2_000); // Reset
    }

    #[test]
    fn test_top_up() {
        let mut state = InsuranceState::default();
        state.top_up(50_000);
        assert_eq!(state.vault_balance, 50_000);
        assert_eq!(state.total_fees_accrued, 50_000);
    }

    #[test]
    fn test_withdraw_surplus_success() {
        let mut state = InsuranceState::default();
        state.vault_balance = 100_000;
        assert!(state.withdraw_surplus(30_000).is_ok());
        assert_eq!(state.vault_balance, 70_000);
    }

    #[test]
    fn test_withdraw_surplus_with_uncovered_debt() {
        let mut state = InsuranceState::default();
        state.vault_balance = 100_000;
        state.uncovered_bad_debt = 1000;
        assert!(state.withdraw_surplus(30_000).is_err());
    }

    #[test]
    fn test_withdraw_surplus_insufficient_balance() {
        let mut state = InsuranceState::default();
        state.vault_balance = 10_000;
        assert!(state.withdraw_surplus(30_000).is_err());
    }
}
