#[cfg(test)]
mod tests {
    use crate::pokelator::battle::*;
    use crate::pokelator::battle::resolver;
    use crate::pokelator::creature::*;
    use crate::pokelator::creature::rarity::Rarity;

    fn mock_creature(creature_type: CreatureType, rarity: Rarity, level: u32) -> Creature {
        Creature {
            id: [1u8; 32],
            owner: [0u8; 32],
            creature_type,
            rarity,
            base_stats: Stats::from_rarity(&rarity, 12345),
            level,
            xp: 0,
            wins: 0,
            losses: 0,
            mint_slot: 100,
            abilities: Vec::new(),
        }
    }

    #[test]
    fn test_battle_resolves_with_winner() {
        let creature_a = mock_creature(CreatureType::Fire, Rarity::Rare, 10);
        let mut creature_b = mock_creature(CreatureType::Ice, Rarity::Common, 5);
        creature_b.id = [2u8; 32];

        let entry_a = BattleEntry {
            owner: [1u8; 32],
            creature_id: creature_a.id,
            leverage: 3,
            collateral_lamports: 1_000_000_000,
        };

        let entry_b = BattleEntry {
            owner: [2u8; 32],
            creature_id: creature_b.id,
            leverage: 3,
            collateral_lamports: 1_000_000_000,
        };

        let mut battle = Battle::new(1, entry_a, entry_b, 500);

        let result = resolver::resolve(
            &mut battle,
            &creature_a,
            &creature_b,
            0xDEADBEEF,
            501,
        );

        assert!(battle.status == BattleStatus::Resolved);
        assert!(result.winner_payout_lamports > 0);
        assert!(result.insurance_fee_lamports > 0);
        assert!(!result.damage_log.is_empty());
    }

    #[test]
    fn test_type_advantage_matters() {
        // Fire vs Ice: Fire should win more often
        let mut fire_wins = 0;
        let mut ice_wins = 0;

        for seed in 0..100u64 {
            let fire = mock_creature(CreatureType::Fire, Rarity::Common, 5);
            let mut ice = mock_creature(CreatureType::Ice, Rarity::Common, 5);
            ice.id = [2u8; 32];

            let entry_a = BattleEntry {
                owner: [1u8; 32],
                creature_id: fire.id,
                leverage: 2,
                collateral_lamports: 500_000_000,
            };

            let entry_b = BattleEntry {
                owner: [2u8; 32],
                creature_id: ice.id,
                leverage: 2,
                collateral_lamports: 500_000_000,
            };

            let mut battle = Battle::new(seed + 1, entry_a, entry_b, 100);
            let result = resolver::resolve(&mut battle, &fire, &ice, seed * 7919, 101);

            if result.winner == [1u8; 32] {
                fire_wins += 1;
            } else {
                ice_wins += 1;
            }
        }

        // Fire should win significantly more than Ice due to type advantage
        assert!(fire_wins > ice_wins, "Fire won {} vs Ice {}", fire_wins, ice_wins);
    }

    #[test]
    fn test_insurance_fee_is_collected() {
        let creature_a = mock_creature(CreatureType::Water, Rarity::Common, 3);
        let mut creature_b = mock_creature(CreatureType::Ground, Rarity::Common, 3);
        creature_b.id = [2u8; 32];

        let collateral = 2_000_000_000u64; // 2 SOL each

        let entry_a = BattleEntry {
            owner: [1u8; 32],
            creature_id: creature_a.id,
            leverage: 2,
            collateral_lamports: collateral,
        };

        let entry_b = BattleEntry {
            owner: [2u8; 32],
            creature_id: creature_b.id,
            leverage: 2,
            collateral_lamports: collateral,
        };

        let mut battle = Battle::new(1, entry_a, entry_b, 200);
        let result = resolver::resolve(&mut battle, &creature_a, &creature_b, 42, 201);

        let expected_fee = (collateral * 2 * 50) / 10_000; // 0.5% of total pot
        assert_eq!(result.insurance_fee_lamports, expected_fee);
        assert_eq!(result.winner_payout_lamports, collateral * 2 - expected_fee);
    }
}
