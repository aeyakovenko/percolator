use super::{Battle, BattleResult, BattleStatus, DamageEvent, MoveType};
use crate::pokelator::creature::Creature;

/// Maximum number of turns before a battle force-resolves.
const MAX_TURNS: u32 = 20;

/// Critical hit chance as percentage (0-100).
const CRIT_CHANCE: u64 = 12;

/// Critical hit damage multiplier.
const CRIT_MULTIPLIER: f64 = 1.5;

/// Insurance fee rate on total pot (basis points).
const INSURANCE_FEE_BPS: u64 = 50; // 0.5%

/// Resolves a battle between two creatures using stats, type matchups,
/// and on-chain RNG. Returns the battle result for settlement.
pub fn resolve(
    battle: &mut Battle,
    creature_a: &Creature,
    creature_b: &Creature,
    blockhash_seed: u64,
    slot: u64,
) -> BattleResult {
    let stats_a = creature_a.effective_stats();
    let stats_b = creature_b.effective_stats();

    let mut hp_a = stats_a.hp as i64;
    let mut hp_b = stats_b.hp as i64;

    let mut rng = blockhash_seed;
    let mut damage_log: Vec<DamageEvent> = Vec::new();

    for turn in 0..MAX_TURNS {
        rng = xorshift64(rng);

        // Determine turn order by speed + RNG variance
        let speed_a = stats_a.speed + (rng % 10);
        rng = xorshift64(rng);
        let speed_b = stats_b.speed + (rng % 10);

        let (first, second) = if speed_a >= speed_b {
            (true, false)
        } else {
            (false, true)
        };

        // First attacker
        rng = xorshift64(rng);
        let move_type_first = if rng % 2 == 0 {
            MoveType::Physical
        } else {
            MoveType::Special
        };

        if first {
            let dmg = calculate_damage(
                &stats_a,
                &stats_b,
                creature_a,
                creature_b,
                move_type_first,
                &mut rng,
            );
            hp_b -= dmg.0 as i64;
            damage_log.push(dmg.1);

            if hp_b <= 0 {
                break;
            }
        }

        // Second attacker
        rng = xorshift64(rng);
        let move_type_second = if rng % 2 == 0 {
            MoveType::Physical
        } else {
            MoveType::Special
        };

        if !first || hp_b > 0 {
            let dmg = calculate_damage(
                &stats_b,
                &stats_a,
                creature_b,
                creature_a,
                move_type_second,
                &mut rng,
            );
            hp_a -= dmg.0 as i64;
            damage_log.push(dmg.1);

            if hp_a <= 0 {
                break;
            }
        }
    }

    // Determine winner. If both alive after MAX_TURNS, higher remaining HP wins.
    let a_wins = hp_a > hp_b;

    let (winner, loser) = if a_wins {
        (battle.player_a.owner, battle.player_b.owner)
    } else {
        (battle.player_b.owner, battle.player_a.owner)
    };

    let total_pot = battle.total_collateral();
    let insurance_fee = (total_pot * INSURANCE_FEE_BPS) / 10_000;
    let winner_payout = total_pot - insurance_fee;

    let loser_collateral = if a_wins {
        battle.player_b.collateral_lamports
    } else {
        battle.player_a.collateral_lamports
    };

    battle.status = BattleStatus::Resolved;
    battle.resolved_slot = Some(slot);

    let result = BattleResult {
        winner,
        loser,
        winner_payout_lamports: winner_payout,
        loser_loss_lamports: loser_collateral,
        insurance_fee_lamports: insurance_fee,
        rng_seed: blockhash_seed,
        damage_log,
    };

    battle.result = Some(result.clone());
    result
}

/// Calculates damage for a single attack.
fn calculate_damage(
    attacker_stats: &super::super::creature::Stats,
    defender_stats: &super::super::creature::Stats,
    attacker: &Creature,
    defender: &Creature,
    move_type: MoveType,
    rng: &mut u64,
) -> (u64, DamageEvent) {
    let (atk, def) = match move_type {
        MoveType::Physical => (attacker_stats.atk, defender_stats.def),
        MoveType::Special => (attacker_stats.sp_atk, defender_stats.sp_def),
    };

    let type_multiplier = attacker.creature_type.matchup_multiplier(&defender.creature_type);

    *rng = xorshift64(*rng);
    let is_crit = (*rng % 100) < CRIT_CHANCE;
    let crit_mult = if is_crit { CRIT_MULTIPLIER } else { 1.0 };

    // Damage formula: ((2 * atk / def) + 2) * type_mult * crit_mult * variance
    *rng = xorshift64(*rng);
    let variance = 85 + (*rng % 16); // 85-100% variance
    let raw = ((2 * atk / def.max(1)) + 2) as f64;
    let damage = (raw * type_multiplier * crit_mult * (variance as f64 / 100.0)) as u64;
    let damage = damage.max(1);

    let event = DamageEvent {
        attacker: attacker.id,
        damage,
        move_type,
        was_critical: is_crit,
        type_multiplier,
    };

    (damage, event)
}

/// Fast on-chain RNG using xorshift64.
fn xorshift64(mut state: u64) -> u64 {
    state ^= state << 13;
    state ^= state >> 7;
    state ^= state << 17;
    state
}
