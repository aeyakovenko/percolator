use super::super::creature::rarity::Rarity;

/// On-chain events that can trigger creature spawns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpawnTrigger {
    /// Regular block production.
    NewBlock,
    /// Significant volume spike on a token pair.
    VolumeSpike,
    /// Large whale transfer detected.
    WhaleMovement,
    /// Liquidation event on a perp protocol.
    LiquidationEvent,
    /// Validator stake/unstake event.
    ValidatorEvent,
}

impl SpawnTrigger {
    /// Returns the spawn probability for this trigger type (0-1000 scale).
    /// Higher = more likely to spawn.
    pub fn spawn_rate(&self) -> u64 {
        match self {
            SpawnTrigger::NewBlock => 50,           // 5% per block
            SpawnTrigger::VolumeSpike => 200,       // 20%
            SpawnTrigger::WhaleMovement => 350,     // 35%
            SpawnTrigger::LiquidationEvent => 500,  // 50%
            SpawnTrigger::ValidatorEvent => 150,    // 15%
        }
    }

    /// Checks if this event triggers a spawn based on entropy.
    pub fn should_spawn(&self, entropy: u64) -> bool {
        (entropy % 1000) < self.spawn_rate()
    }

    /// Derives creature rarity from the trigger type and entropy.
    /// Rarer triggers bias toward higher rarity creatures.
    pub fn derive_rarity(&self, entropy: u64) -> Rarity {
        let bias = match self {
            SpawnTrigger::NewBlock => 0,
            SpawnTrigger::VolumeSpike => 100,
            SpawnTrigger::WhaleMovement => 200,
            SpawnTrigger::LiquidationEvent => 300,
            SpawnTrigger::ValidatorEvent => 150,
        };

        // Shift the entropy roll toward rarer outcomes based on trigger bias
        let adjusted = (entropy >> 8) % 1000;
        let biased = adjusted.saturating_add(bias).min(999);

        Rarity::from_entropy(biased)
    }
}
