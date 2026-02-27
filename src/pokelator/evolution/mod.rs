pub mod abilities;

use super::creature::Creature;
use abilities::Ability;

/// XP awarded for different battle outcomes.
const XP_WIN: u64 = 100;
const XP_LOSS: u64 = 25;
const XP_WIN_BONUS_PER_LEVEL_DIFF: u64 = 15;

/// Processes a battle outcome and applies XP, level ups, and ability unlocks.
pub fn process_battle_outcome(
    creature: &mut Creature,
    won: bool,
    opponent_level: u32,
) -> EvolutionResult {
    let mut result = EvolutionResult {
        xp_gained: 0,
        leveled_up: false,
        new_level: creature.level,
        ability_unlocked: None,
    };

    // Calculate XP
    let base_xp = if won { XP_WIN } else { XP_LOSS };
    let level_diff_bonus = if won && opponent_level > creature.level {
        (opponent_level - creature.level) as u64 * XP_WIN_BONUS_PER_LEVEL_DIFF
    } else {
        0
    };

    result.xp_gained = base_xp + level_diff_bonus;

    // Update win/loss record
    if won {
        creature.wins += 1;
    } else {
        creature.losses += 1;
    }

    // Award XP and check for level up
    let leveled = creature.award_xp(result.xp_gained);
    if leveled {
        result.leveled_up = true;
        result.new_level = creature.level;

        // Check for ability unlock at this level
        if let Some(ability) = abilities::check_unlock(creature.level) {
            if !creature.abilities.contains(&(ability as u8)) {
                let max_slots = ability_slots_for_level(creature.level);
                if creature.abilities.len() < max_slots {
                    creature.abilities.push(ability as u8);
                    result.ability_unlocked = Some(ability);
                }
            }
        }
    }

    result
}

/// Returns the number of ability slots available at a given level.
pub fn ability_slots_for_level(level: u32) -> usize {
    match level {
        1..=5 => 0,
        6..=10 => 1,
        11..=20 => 2,
        21..=50 => 3,
        _ => 4,
    }
}

/// Returns the maximum leverage allowed at a given level.
pub fn max_leverage_for_level(level: u32) -> u64 {
    match level {
        1..=5 => 3,
        6..=10 => 5,
        11..=20 => 7,
        21..=50 => 10,
        _ => 15,
    }
}

#[derive(Debug)]
pub struct EvolutionResult {
    pub xp_gained: u64,
    pub leveled_up: bool,
    pub new_level: u32,
    pub ability_unlocked: Option<Ability>,
}
