#[cfg(test)]
mod tests {
    use crate::pokelator::creature::*;
    use crate::pokelator::creature::rarity::Rarity;
    use crate::pokelator::creature::registry::CreatureRegistry;

    #[test]
    fn test_rarity_distribution() {
        let mut common = 0;
        let mut uncommon = 0;
        let mut rare = 0;
        let mut epic = 0;
        let mut legendary = 0;

        for i in 0..10000u64 {
            match Rarity::from_entropy(i) {
                Rarity::Common => common += 1,
                Rarity::Uncommon => uncommon += 1,
                Rarity::Rare => rare += 1,
                Rarity::Epic => epic += 1,
                Rarity::Legendary => legendary += 1,
            }
        }

        // Should roughly match: 50%, 25%, 15%, 8%, 2%
        assert!(common == 5000);
        assert!(uncommon == 2500);
        assert!(rare == 1500);
        assert!(epic == 800);
        assert!(legendary == 200);
    }

    #[test]
    fn test_stats_scale_with_rarity() {
        let common_stats = Stats::from_rarity(&Rarity::Common, 42);
        let legendary_stats = Stats::from_rarity(&Rarity::Legendary, 42);

        assert!(legendary_stats.total() > common_stats.total());
    }

    #[test]
    fn test_level_scaling() {
        let stats = Stats::from_rarity(&Rarity::Common, 42);

        let level_1 = stats.scaled(1);
        let level_10 = stats.scaled(10);
        let level_50 = stats.scaled(50);

        assert!(level_10.total() > level_1.total());
        assert!(level_50.total() > level_10.total());
    }

    #[test]
    fn test_type_matchups() {
        assert_eq!(CreatureType::Fire.strong_against(), CreatureType::Ice);
        assert_eq!(CreatureType::Fire.weak_against(), CreatureType::Water);

        assert!(CreatureType::Fire.matchup_multiplier(&CreatureType::Ice) > 1.0);
        assert!(CreatureType::Fire.matchup_multiplier(&CreatureType::Water) < 1.0);
        assert_eq!(CreatureType::Fire.matchup_multiplier(&CreatureType::Ground), 1.0);
    }

    #[test]
    fn test_creature_mint() {
        let mut registry = CreatureRegistry::new();
        let owner = [1u8; 32];

        let creature = registry.mint(owner, 12345, 100).unwrap();
        assert_eq!(creature.level, 1);
        assert_eq!(creature.wins, 0);
        assert_eq!(creature.owner, owner);
        assert_eq!(registry.total_minted, 1);
    }

    #[test]
    fn test_max_leverage_by_level() {
        let stats = Stats::from_rarity(&Rarity::Common, 1);

        let mut creature = Creature {
            id: [0u8; 32],
            owner: [0u8; 32],
            creature_type: CreatureType::Fire,
            rarity: Rarity::Common,
            base_stats: stats,
            level: 1,
            xp: 0,
            wins: 0,
            losses: 0,
            mint_slot: 0,
            abilities: Vec::new(),
        };

        assert_eq!(creature.max_leverage(), 3);
        creature.level = 10;
        assert_eq!(creature.max_leverage(), 5);
        creature.level = 20;
        assert_eq!(creature.max_leverage(), 7);
        creature.level = 50;
        assert_eq!(creature.max_leverage(), 10);
        creature.level = 51;
        assert_eq!(creature.max_leverage(), 15);
    }

    #[test]
    fn test_xp_and_levelup() {
        let stats = Stats::from_rarity(&Rarity::Common, 1);

        let mut creature = Creature {
            id: [0u8; 32],
            owner: [0u8; 32],
            creature_type: CreatureType::Water,
            rarity: Rarity::Common,
            base_stats: stats,
            level: 1,
            xp: 0,
            wins: 0,
            losses: 0,
            mint_slot: 0,
            abilities: Vec::new(),
        };

        // Level 1 needs 100 XP to level up
        assert!(!creature.award_xp(50));
        assert_eq!(creature.level, 1);

        assert!(creature.award_xp(50));
        assert_eq!(creature.level, 2);
    }
}
