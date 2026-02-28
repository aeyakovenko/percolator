pub mod registry;
pub mod rarity;

use rarity::Rarity;

/// Creature types for battle matchup resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CreatureType {
    Fire,
    Water,
    Electric,
    Ground,
    Ice,
}

impl CreatureType {
    /// Returns the type this creature is strong against.
    pub fn strong_against(&self) -> CreatureType {
        match self {
            CreatureType::Fire => CreatureType::Ice,
            CreatureType::Water => CreatureType::Fire,
            CreatureType::Electric => CreatureType::Water,
            CreatureType::Ground => CreatureType::Electric,
            CreatureType::Ice => CreatureType::Ground,
        }
    }

    /// Returns the type this creature is weak against.
    pub fn weak_against(&self) -> CreatureType {
        match self {
            CreatureType::Fire => CreatureType::Water,
            CreatureType::Water => CreatureType::Electric,
            CreatureType::Electric => CreatureType::Ground,
            CreatureType::Ground => CreatureType::Ice,
            CreatureType::Ice => CreatureType::Fire,
        }
    }

    /// Returns the damage multiplier when attacking a target type.
    pub fn matchup_multiplier(&self, target: &CreatureType) -> f64 {
        if target == &self.strong_against() {
            1.5
        } else if target == &self.weak_against() {
            0.65
        } else {
            1.0
        }
    }
}

/// Base stats for a creature, determined at mint.
#[derive(Debug, Clone)]
pub struct Stats {
    pub hp: u64,
    pub atk: u64,
    pub def: u64,
    pub sp_atk: u64,
    pub sp_def: u64,
    pub speed: u64,
}

impl Stats {
    /// Creates base stats scaled by rarity tier.
    pub fn from_rarity(rarity: &Rarity, seed: u64) -> Self {
        let base = rarity.base_stat_range();
        let variance = |s: u64| -> u64 {
            let v = (s ^ seed.wrapping_mul(0x517cc1b727220a95)) % 20;
            base.0 + (v * (base.1 - base.0)) / 20
        };

        Stats {
            hp: variance(1),
            atk: variance(2),
            def: variance(3),
            sp_atk: variance(4),
            sp_def: variance(5),
            speed: variance(6),
        }
    }

    /// Returns the total stat sum, used for power rating.
    pub fn total(&self) -> u64 {
        self.hp + self.atk + self.def + self.sp_atk + self.sp_def + self.speed
    }

    /// Applies level scaling to stats.
    pub fn scaled(&self, level: u32) -> Stats {
        let multiplier = match level {
            1..=5 => 1.0,
            6..=10 => 1.15,
            11..=20 => 1.30,
            21..=50 => 1.50,
            _ => 1.75,
        };

        Stats {
            hp: (self.hp as f64 * multiplier) as u64,
            atk: (self.atk as f64 * multiplier) as u64,
            def: (self.def as f64 * multiplier) as u64,
            sp_atk: (self.sp_atk as f64 * multiplier) as u64,
            sp_def: (self.sp_def as f64 * multiplier) as u64,
            speed: (self.speed as f64 * multiplier) as u64,
        }
    }
}

/// A creature instance on chain.
#[derive(Debug, Clone)]
pub struct Creature {
    pub id: [u8; 32],
    pub owner: [u8; 32],
    pub creature_type: CreatureType,
    pub rarity: Rarity,
    pub base_stats: Stats,
    pub level: u32,
    pub xp: u64,
    pub wins: u32,
    pub losses: u32,
    pub mint_slot: u64,
    pub abilities: Vec<u8>,
}

impl Creature {
    /// Returns the creature's current effective stats with level scaling.
    pub fn effective_stats(&self) -> Stats {
        self.base_stats.scaled(self.level)
    }

    /// Returns the maximum leverage this creature can use in battle.
    pub fn max_leverage(&self) -> u64 {
        match self.level {
            1..=5 => 3,
            6..=10 => 5,
            11..=20 => 7,
            21..=50 => 10,
            _ => 15,
        }
    }

    /// Returns the power rating used for matchmaking.
    pub fn power_rating(&self) -> u64 {
        self.effective_stats().total() + (self.wins as u64 * 10)
    }

    /// Awards XP for a battle outcome. Returns true if the creature leveled up.
    pub fn award_xp(&mut self, xp: u64) -> bool {
        self.xp += xp;
        let xp_to_level = (self.level as u64) * 100;
        if self.xp >= xp_to_level {
            self.xp -= xp_to_level;
            self.level += 1;
            true
        } else {
            false
        }
    }
}
