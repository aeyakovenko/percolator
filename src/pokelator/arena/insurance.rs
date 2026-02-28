/// Insurance fund managing battle settlement reserves.
/// Mirrors Percolator's insurance fund semantics.
///
/// The fund collects fees from every settled battle and backs
/// edge cases where the loser's collateral doesn't fully cover
/// the winner's leveraged payout. If the fund is depleted,
/// ADL (auto-deleveraging) kicks in to socialize losses.
#[derive(Debug)]
pub struct InsuranceFund {
    /// Current balance in lamports.
    pub balance: u64,
    /// Minimum balance that cannot be spent (floor).
    pub floor: u64,
    /// Total fees collected historically.
    pub total_collected: u64,
    /// Total payouts from insurance.
    pub total_paid: u64,
    /// Number of times ADL was triggered.
    pub adl_events: u64,
}

impl InsuranceFund {
    pub fn new(floor: u64) -> Self {
        InsuranceFund {
            balance: 0,
            floor,
            total_collected: 0,
            total_paid: 0,
            adl_events: 0,
        }
    }

    /// Returns the spendable balance above the floor.
    pub fn spendable(&self) -> u64 {
        if self.balance > self.floor {
            self.balance - self.floor
        } else {
            0
        }
    }

    /// Deposits a fee into the insurance fund.
    pub fn deposit_fee(&mut self, amount: u64) {
        self.balance += amount;
        self.total_collected += amount;
    }

    /// Attempts to cover a shortfall from insurance.
    /// Returns the amount actually covered. If insufficient,
    /// returns what's available and the caller must trigger ADL.
    pub fn cover_shortfall(&mut self, amount: u64) -> CoverResult {
        let available = self.spendable();

        if available >= amount {
            self.balance -= amount;
            self.total_paid += amount;
            CoverResult::FullyCovered { amount }
        } else if available > 0 {
            self.balance -= available;
            self.total_paid += available;
            let remaining = amount - available;
            self.adl_events += 1;
            CoverResult::PartiallyCovered {
                covered: available,
                shortfall: remaining,
            }
        } else {
            self.adl_events += 1;
            CoverResult::ADLRequired { shortfall: amount }
        }
    }

    /// Returns the fund health as a ratio (0.0 to 1.0+).
    /// Values below 1.0 indicate the fund is below its floor.
    pub fn health_ratio(&self) -> f64 {
        if self.floor == 0 {
            return f64::MAX;
        }
        self.balance as f64 / self.floor as f64
    }
}

#[derive(Debug)]
pub enum CoverResult {
    /// Insurance fully covered the shortfall.
    FullyCovered { amount: u64 },
    /// Insurance partially covered. Remaining must be socialized via ADL.
    PartiallyCovered { covered: u64, shortfall: u64 },
    /// No insurance available. Full ADL required.
    ADLRequired { shortfall: u64 },
}
