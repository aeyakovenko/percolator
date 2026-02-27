#[cfg(test)]
mod tests {
    use crate::pokelator::battle::*;
    use crate::pokelator::battle::settlement::*;
    use crate::pokelator::arena::insurance::InsuranceFund;

    fn mock_battle_result(winner: [u8; 32], loser: [u8; 32]) -> (Battle, BattleResult) {
        let entry_a = BattleEntry {
            owner: winner,
            creature_id: [1u8; 32],
            leverage: 5,
            collateral_lamports: 1_000_000_000,
        };

        let entry_b = BattleEntry {
            owner: loser,
            creature_id: [2u8; 32],
            leverage: 5,
            collateral_lamports: 1_000_000_000,
        };

        let mut battle = Battle::new(1, entry_a, entry_b, 100);
        battle.status = BattleStatus::Resolved;

        let result = BattleResult {
            winner,
            loser,
            winner_payout_lamports: 1_990_000_000, // 2 SOL - 0.5% fee
            loser_loss_lamports: 1_000_000_000,
            insurance_fee_lamports: 10_000_000,
            rng_seed: 42,
            damage_log: Vec::new(),
        };

        (battle, result)
    }

    #[test]
    fn test_normal_settlement() {
        let winner = [1u8; 32];
        let loser = [2u8; 32];
        let (mut battle, result) = mock_battle_result(winner, loser);

        let mut insurance_balance = 100_000_000u64;
        let insurance_floor = 50_000_000u64;

        let action = settle_battle(
            &mut battle,
            &result,
            &mut insurance_balance,
            insurance_floor,
        ).unwrap();

        assert!(matches!(action, SettlementAction::Normal { .. }));
        assert_eq!(battle.status, BattleStatus::Settled);

        // Insurance should have increased by the fee
        assert_eq!(insurance_balance, 100_000_000 + 10_000_000);
    }

    #[test]
    fn test_cannot_settle_unresolved_battle() {
        let entry_a = BattleEntry {
            owner: [1u8; 32],
            creature_id: [1u8; 32],
            leverage: 2,
            collateral_lamports: 500_000_000,
        };

        let entry_b = BattleEntry {
            owner: [2u8; 32],
            creature_id: [2u8; 32],
            leverage: 2,
            collateral_lamports: 500_000_000,
        };

        let mut battle = Battle::new(1, entry_a, entry_b, 100);
        // Status is Matched, not Resolved

        let result = BattleResult {
            winner: [1u8; 32],
            loser: [2u8; 32],
            winner_payout_lamports: 990_000_000,
            loser_loss_lamports: 500_000_000,
            insurance_fee_lamports: 5_000_000,
            rng_seed: 42,
            damage_log: Vec::new(),
        };

        let mut insurance_balance = 100_000_000u64;
        let action = settle_battle(&mut battle, &result, &mut insurance_balance, 50_000_000);

        assert!(action.is_err());
    }

    #[test]
    fn test_insurance_fund_health() {
        let mut fund = InsuranceFund::new(100_000_000);

        assert_eq!(fund.health_ratio(), 0.0);

        fund.deposit_fee(50_000_000);
        assert!(fund.health_ratio() < 1.0); // Below floor

        fund.deposit_fee(100_000_000);
        assert!(fund.health_ratio() > 1.0); // Above floor

        assert_eq!(fund.spendable(), 50_000_000);
    }
}
