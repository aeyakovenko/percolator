/// Rarity tiers for creatures. Determines base stat ranges,
/// catch cost, and maximum leverage in battle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rarity {
    Common,
    Uncommon,
    Rare,
    Epic,
    Legendary,
}

impl Rarity {
    /// Returns (min, max) base stat range for this rarity.
    pub fn base_stat_range(&self) -> (u64, u64) {
        match self {
            Rarity::Common => (30, 50),
            Rarity::Uncommon => (45, 70),
            Rarity::Rare => (60, 90),
            Rarity::Epic => (80, 110),
            Rarity::Legendary => (100, 140),
        }
    }

    /// Returns the catch cost in lamports.
    pub fn catch_cost_lamports(&self) -> u64 {
        match self {
            Rarity::Common => 10_000_000,        // 0.01 SOL
            Rarity::Uncommon => 50_000_000,      // 0.05 SOL
            Rarity::Rare => 200_000_000,         // 0.2 SOL
            Rarity::Epic => 500_000_000,         // 0.5 SOL
            Rarity::Legendary => 1_000_000_000,  // 1.0 SOL
        }
    }

    /// Returns the maximum leverage tier for this rarity at level 1.
    pub fn base_max_leverage(&self) -> u64 {
        match self {
            Rarity::Common => 3,
            Rarity::Uncommon => 5,
            Rarity::Rare => 7,
            Rarity::Epic => 10,
            Rarity::Legendary => 15,
        }
    }

    /// Derives rarity from an on-chain entropy value (0-999).
    /// Distribution: Common 50%, Uncommon 25%, Rare 15%, Epic 8%, Legendary 2%.
    pub fn from_entropy(value: u64) -> Rarity {
        let roll = value % 1000;
        match roll {
            0..=499 => Rarity::Common,
            500..=749 => Rarity::Uncommon,
            750..=899 => Rarity::Rare,
            900..=979 => Rarity::Epic,
            _ => Rarity::Legendary,
        }
    }
}
