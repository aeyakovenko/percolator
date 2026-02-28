pub mod insurance;

use super::battle::{Battle, BattleStatus};

/// Global arena state tracking all active and historical battles.
pub struct Arena {
    pub active_battles: Vec<Battle>,
    pub completed_battles: Vec<Battle>,
    pub total_battles: u64,
    pub total_volume_lamports: u64,
    pub total_insurance_collected: u64,
}

impl Arena {
    pub fn new() -> Self {
        Arena {
            active_battles: Vec::new(),
            completed_battles: Vec::new(),
            total_battles: 0,
            total_volume_lamports: 0,
            total_insurance_collected: 0,
        }
    }

    /// Registers a new battle in the arena.
    pub fn register_battle(&mut self, battle: Battle) {
        self.total_battles += 1;
        self.total_volume_lamports += battle.total_collateral();
        self.active_battles.push(battle);
    }

    /// Moves a settled battle from active to completed.
    pub fn finalize_battle(&mut self, battle_id: u64) -> Option<&Battle> {
        if let Some(pos) = self.active_battles.iter().position(|b| b.id == battle_id) {
            let battle = self.active_battles.remove(pos);
            if battle.status == BattleStatus::Settled {
                self.completed_battles.push(battle);
                return self.completed_battles.last();
            }
        }
        None
    }

    /// Returns an active battle by ID.
    pub fn get_active(&self, battle_id: u64) -> Option<&Battle> {
        self.active_battles.iter().find(|b| b.id == battle_id)
    }

    /// Returns battle history for a given player.
    pub fn player_history(&self, owner: &[u8; 32]) -> Vec<&Battle> {
        self.completed_battles
            .iter()
            .filter(|b| {
                b.player_a.owner == *owner || b.player_b.owner == *owner
            })
            .collect()
    }

    /// Returns the player's win/loss record.
    pub fn player_record(&self, owner: &[u8; 32]) -> (u32, u32) {
        let mut wins = 0;
        let mut losses = 0;

        for battle in &self.completed_battles {
            if let Some(ref result) = battle.result {
                if result.winner == *owner {
                    wins += 1;
                } else if result.loser == *owner {
                    losses += 1;
                }
            }
        }

        (wins, losses)
    }

    /// Returns aggregate arena statistics.
    pub fn stats(&self) -> ArenaStats {
        ArenaStats {
            total_battles: self.total_battles,
            active_battles: self.active_battles.len() as u64,
            total_volume_lamports: self.total_volume_lamports,
            total_insurance_collected: self.total_insurance_collected,
        }
    }
}

#[derive(Debug)]
pub struct ArenaStats {
    pub total_battles: u64,
    pub active_battles: u64,
    pub total_volume_lamports: u64,
    pub total_insurance_collected: u64,
}
