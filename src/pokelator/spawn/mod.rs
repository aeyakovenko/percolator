pub mod triggers;

use super::creature::rarity::Rarity;
use triggers::SpawnTrigger;

/// A pending creature spawn event.
#[derive(Debug, Clone)]
pub struct SpawnEvent {
    pub trigger: SpawnTrigger,
    pub rarity: Rarity,
    pub entropy: u64,
    pub slot: u64,
    pub available: bool,
    pub caught_by: Option<[u8; 32]>,
}

/// Manages the spawn queue and processes network events into catchable creatures.
pub struct SpawnManager {
    pub active_spawns: Vec<SpawnEvent>,
    pub total_spawned: u64,
    /// Maximum concurrent uncaught spawns.
    pub max_active: usize,
    /// Slots before an uncaught spawn expires.
    pub expiry_slots: u64,
}

impl SpawnManager {
    pub fn new() -> Self {
        SpawnManager {
            active_spawns: Vec::new(),
            total_spawned: 0,
            max_active: 50,
            expiry_slots: 600, // ~4 minutes
        }
    }

    /// Processes an on-chain event and potentially creates a spawn.
    pub fn process_event(
        &mut self,
        trigger: SpawnTrigger,
        entropy: u64,
        current_slot: u64,
    ) -> Option<&SpawnEvent> {
        // Clean expired spawns
        self.active_spawns.retain(|s| {
            current_slot - s.slot < self.expiry_slots || s.caught_by.is_some()
        });

        // Check capacity
        let active_uncaught = self.active_spawns.iter().filter(|s| s.available).count();
        if active_uncaught >= self.max_active {
            return None;
        }

        // Check if trigger meets spawn threshold
        if !trigger.should_spawn(entropy) {
            return None;
        }

        let rarity = trigger.derive_rarity(entropy);

        let event = SpawnEvent {
            trigger,
            rarity,
            entropy,
            slot: current_slot,
            available: true,
            caught_by: None,
        };

        self.active_spawns.push(event);
        self.total_spawned += 1;

        self.active_spawns.last()
    }

    /// Marks a spawn as caught by a player. Returns the spawn data for minting.
    pub fn catch(
        &mut self,
        index: usize,
        catcher: [u8; 32],
    ) -> Result<&SpawnEvent, SpawnError> {
        let spawn = self.active_spawns.get_mut(index).ok_or(SpawnError::NotFound)?;

        if !spawn.available {
            return Err(SpawnError::AlreadyCaught);
        }

        spawn.available = false;
        spawn.caught_by = Some(catcher);

        Ok(spawn)
    }

    /// Returns all currently available (uncaught, unexpired) spawns.
    pub fn available_spawns(&self, current_slot: u64) -> Vec<&SpawnEvent> {
        self.active_spawns
            .iter()
            .filter(|s| s.available && current_slot - s.slot < self.expiry_slots)
            .collect()
    }
}

#[derive(Debug)]
pub enum SpawnError {
    NotFound,
    AlreadyCaught,
    Expired,
}
