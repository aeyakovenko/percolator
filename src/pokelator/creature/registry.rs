use super::{Creature, CreatureType, Stats};
use super::rarity::Rarity;

/// On-chain registry tracking all minted creatures and their current state.
pub struct CreatureRegistry {
    pub creatures: Vec<Creature>,
    pub total_minted: u64,
    pub total_burned: u64,
}

impl CreatureRegistry {
    pub fn new() -> Self {
        CreatureRegistry {
            creatures: Vec::new(),
            total_minted: 0,
            total_burned: 0,
        }
    }

    /// Mints a new creature from spawn event data.
    /// The creature's type, rarity, and stats are derived from on-chain entropy.
    pub fn mint(
        &mut self,
        owner: [u8; 32],
        entropy: u64,
        slot: u64,
    ) -> Result<&Creature, RegistryError> {
        let rarity = Rarity::from_entropy(entropy);

        let type_roll = (entropy >> 16) % 5;
        let creature_type = match type_roll {
            0 => CreatureType::Fire,
            1 => CreatureType::Water,
            2 => CreatureType::Electric,
            3 => CreatureType::Ground,
            _ => CreatureType::Ice,
        };

        let id = Self::derive_id(entropy, slot, self.total_minted);
        let base_stats = Stats::from_rarity(&rarity, entropy);

        let creature = Creature {
            id,
            owner,
            creature_type,
            rarity,
            base_stats,
            level: 1,
            xp: 0,
            wins: 0,
            losses: 0,
            mint_slot: slot,
            abilities: Vec::new(),
        };

        self.creatures.push(creature);
        self.total_minted += 1;

        Ok(self.creatures.last().unwrap())
    }

    /// Looks up a creature by ID.
    pub fn get(&self, id: &[u8; 32]) -> Option<&Creature> {
        self.creatures.iter().find(|c| &c.id == id)
    }

    /// Returns a mutable reference to a creature by ID.
    pub fn get_mut(&mut self, id: &[u8; 32]) -> Option<&mut Creature> {
        self.creatures.iter_mut().find(|c| &c.id == id)
    }

    /// Returns all creatures owned by a given pubkey.
    pub fn owned_by(&self, owner: &[u8; 32]) -> Vec<&Creature> {
        self.creatures.iter().filter(|c| &c.owner == owner).collect()
    }

    /// Derives a unique creature ID from spawn parameters.
    fn derive_id(entropy: u64, slot: u64, index: u64) -> [u8; 32] {
        let mut id = [0u8; 32];
        let combined = entropy
            .wrapping_mul(slot)
            .wrapping_add(index)
            .wrapping_mul(0x517cc1b727220a95);

        for i in 0..4 {
            let bytes = (combined.wrapping_mul((i + 1) as u64)).to_le_bytes();
            id[i * 8..(i + 1) * 8].copy_from_slice(&bytes);
        }
        id
    }
}

#[derive(Debug)]
pub enum RegistryError {
    CreatureNotFound,
    NotOwner,
    AlreadyExists,
}
