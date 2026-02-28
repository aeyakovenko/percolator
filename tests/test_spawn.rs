#[cfg(test)]
mod tests {
    use crate::pokelator::spawn::*;
    use crate::pokelator::spawn::triggers::SpawnTrigger;
    use crate::pokelator::creature::rarity::Rarity;

    #[test]
    fn test_spawn_trigger_rates() {
        // Liquidation events should spawn more often than new blocks
        let mut block_spawns = 0;
        let mut liq_spawns = 0;

        for i in 0..10000u64 {
            if SpawnTrigger::NewBlock.should_spawn(i) {
                block_spawns += 1;
            }
            if SpawnTrigger::LiquidationEvent.should_spawn(i) {
                liq_spawns += 1;
            }
        }

        assert!(liq_spawns > block_spawns);
    }

    #[test]
    fn test_liquidation_trigger_biases_rarity() {
        let mut liq_rares = 0;
        let mut block_rares = 0;

        for i in 0..10000u64 {
            let liq_rarity = SpawnTrigger::LiquidationEvent.derive_rarity(i);
            let block_rarity = SpawnTrigger::NewBlock.derive_rarity(i);

            if matches!(liq_rarity, Rarity::Rare | Rarity::Epic | Rarity::Legendary) {
                liq_rares += 1;
            }
            if matches!(block_rarity, Rarity::Rare | Rarity::Epic | Rarity::Legendary) {
                block_rares += 1;
            }
        }

        // Liquidation events should produce more rare+ creatures
        assert!(liq_rares > block_rares);
    }

    #[test]
    fn test_spawn_manager_catch() {
        let mut manager = SpawnManager::new();

        manager.process_event(SpawnTrigger::LiquidationEvent, 100, 500);

        let available = manager.available_spawns(500);
        assert!(!available.is_empty());

        let catcher = [1u8; 32];
        let result = manager.catch(0, catcher);
        assert!(result.is_ok());

        // Should no longer be available
        let available = manager.available_spawns(500);
        assert!(available.is_empty());
    }

    #[test]
    fn test_spawn_expiry() {
        let mut manager = SpawnManager::new();
        manager.process_event(SpawnTrigger::LiquidationEvent, 100, 500);

        // Should be available now
        assert!(!manager.available_spawns(500).is_empty());

        // Should be expired after expiry_slots
        assert!(manager.available_spawns(500 + manager.expiry_slots + 1).is_empty());
    }
}
