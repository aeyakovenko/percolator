pub mod resolver;
pub mod matchmaking;
pub mod settlement;

use super::creature::Creature;

/// A pending or active battle between two creatures.
#[derive(Debug, Clone)]
pub struct Battle {
    pub id: u64,
    pub player_a: BattleEntry,
    pub player_b: BattleEntry,
    pub status: BattleStatus,
    pub result: Option<BattleResult>,
    pub created_slot: u64,
    pub resolved_slot: Option<u64>,
}

/// One side of a battle.
#[derive(Debug, Clone)]
pub struct BattleEntry {
    pub owner: [u8; 32],
    pub creature_id: [u8; 32],
    pub leverage: u64,
    pub collateral_lamports: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BattleStatus {
    /// Waiting for an opponent.
    Queued,
    /// Both sides matched, waiting for resolution.
    Matched,
    /// Battle resolved, settlement pending.
    Resolved,
    /// Fully settled through risk engine.
    Settled,
    /// Cancelled before match.
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct BattleResult {
    pub winner: [u8; 32],
    pub loser: [u8; 32],
    pub winner_payout_lamports: u64,
    pub loser_loss_lamports: u64,
    pub insurance_fee_lamports: u64,
    pub rng_seed: u64,
    pub damage_log: Vec<DamageEvent>,
}

/// A single damage event in the battle log.
#[derive(Debug, Clone)]
pub struct DamageEvent {
    pub attacker: [u8; 32],
    pub damage: u64,
    pub move_type: MoveType,
    pub was_critical: bool,
    pub type_multiplier: f64,
}

#[derive(Debug, Clone, Copy)]
pub enum MoveType {
    Physical,
    Special,
}

impl Battle {
    /// Creates a new battle from two matched entries.
    pub fn new(
        id: u64,
        player_a: BattleEntry,
        player_b: BattleEntry,
        slot: u64,
    ) -> Self {
        Battle {
            id,
            player_a,
            player_b,
            status: BattleStatus::Matched,
            result: None,
            created_slot: slot,
            resolved_slot: None,
        }
    }

    /// Returns the total collateral locked in this battle.
    pub fn total_collateral(&self) -> u64 {
        self.player_a.collateral_lamports + self.player_b.collateral_lamports
    }

    /// Returns the total leveraged notional value.
    pub fn total_notional(&self) -> u64 {
        (self.player_a.collateral_lamports * self.player_a.leverage)
            + (self.player_b.collateral_lamports * self.player_b.leverage)
    }
}
