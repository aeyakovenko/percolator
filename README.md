# Pokélator

Leveraged PvP creature battles on Solana. Powered by [@aeyakovenko](https://github.com/aeyakovenko)'s [Percolator](https://github.com/aeyakovenko/percolator) risk engine.

Catch creatures on-chain. Battle them for SOL with leverage. Lose and get liquidated.

---

## How It Works

Pokélator wraps Percolator's formally verified risk engine with a creature battle layer. The same invariant that prevents insolvency on a perp DEX enforces solvency on creature fights: no participant can ever extract more value than the system actually contains.

```
                    ┌─────────────────────────────────────────┐
                    │              POKÉLATOR                   │
                    │                                         │
  ┌──────────┐     │  ┌─────────┐  ┌──────────┐  ┌────────┐ │
  │  Solana   │────▶│  │ Spawner │  │  Arena   │  │ Evolve │ │
  │  Events   │     │  └────┬────┘  └────┬─────┘  └───┬────┘ │
  └──────────┘     │       │            │             │      │
                    │       ▼            ▼             ▼      │
                    │  ┌─────────────────────────────────────┐│
                    │  │          Creature Registry          ││
                    │  └──────────────┬──────────────────────┘│
                    │                 │                        │
                    │                 ▼                        │
                    │  ┌─────────────────────────────────────┐│
                    │  │    PERCOLATOR RISK ENGINE           ││
                    │  │  ┌───────────────────────────────┐  ││
                    │  │  │ Collateral → Settlement → ADL │  ││
                    │  │  │ Insurance Fund │ Liquidation   │  ││
                    │  │  └───────────────────────────────┘  ││
                    │  └─────────────────────────────────────┘│
                    └─────────────────────────────────────────┘
```

## Catching

Creatures spawn from Solana network activity. Block production, volume spikes, liquidation events. Each creature's stats, type, and rarity are derived from the on-chain data that triggered the spawn.

```
Spawn Trigger          Rarity        Max Leverage
─────────────          ──────        ────────────
New block              Common        3x
Volume spike           Uncommon      5x
Whale movement         Rare          7x
Liquidation event      Epic          10x
Validator event        Legendary     15x
```

Catching costs SOL. The cost scales with rarity.

## Battling

Both players pick a creature and set leverage within their creature's tier cap. Both deposit collateral into Percolator's risk engine.

```
Player A                          Player B
┌──────────────────┐              ┌──────────────────┐
│ Creature: Pyrox  │              │ Creature: Glacir │
│ Type: Fire       │              │ Type: Ice        │
│ Level: 12        │              │ Level: 9         │
│ Leverage: 5x     │              │ Leverage: 3x     │
│ Collateral: 2 SOL│              │ Collateral: 4 SOL│
└────────┬─────────┘              └────────┬─────────┘
         │                                 │
         ▼                                 ▼
┌──────────────────────────────────────────────────┐
│              PERCOLATOR RISK ENGINE              │
│                                                  │
│  Total Pot: 6 SOL                                │
│  Battle Resolution: Stats + Type + On-Chain RNG  │
│                                                  │
│  Winner → Collateral + Leveraged Profit          │
│  Loser  → Liquidated through risk engine         │
│  Edge cases → Insurance fund / ADL waterfall     │
└──────────────────────────────────────────────────┘
```

Battle resolution uses creature stats, type matchups, and an on-chain RNG seed derived from recent blockhashes. No off-chain oracle, no server.

## Type Matchups

```
Fire     → strong vs Ice, weak vs Water
Water    → strong vs Fire, weak vs Electric
Electric → strong vs Water, weak vs Ground
Ground   → strong vs Electric, weak vs Ice
Ice      → strong vs Ground, weak vs Fire
```

## Evolution

Creatures level up through wins. Higher level unlocks better stats, new abilities, and higher leverage tiers.

```
Level 1-5     → Base stats, max 3x leverage
Level 6-10    → +15% stats, max 5x leverage, unlocks ability slot
Level 11-20   → +30% stats, max 7x leverage, second ability slot
Level 21-50   → +50% stats, max 10x leverage, type combo moves
Level 50+     → +75% stats, max 15x leverage, legendary abilities
```

## Risk Engine

All collateral flows through Percolator. The formal invariant holds:

```
Withdrawals_a ≤ Deposits_a + LossPaid_¬a + SpendableInsurance_end
```

No sequence of battles, settlements, or withdrawals can result in someone extracting more value than the system contains. This is not a promise. It is a mathematical proof verified with Kani.

**Settlement flow:**

```
Battle Resolves
      │
      ├── Winner has profit ──▶ Settle via risk engine
      │                         (ADL if insurance insufficient)
      │
      └── Loser has loss ──────▶ Collateral liquidated
                                 Fee → Insurance fund
```

## Creature Stats

Every creature has six base stats determined at mint:

```
HP        → Total hitpoints in battle
ATK       → Physical damage output
DEF       → Physical damage reduction
SP.ATK    → Special move damage output
SP.DEF    → Special move damage reduction
SPEED     → Turn priority + dodge chance
```

Stats scale with rarity and level. A Legendary level 50 creature significantly outclasses a Common level 50.

## Project Structure

```
Pokelator/
├── src/
│   ├── lib.rs                    # Percolator risk engine (original)
│   ├── ...                       # Percolator source files (original)
│   └── pokelator/
│       ├── mod.rs                # Pokelator module root
│       ├── creature/
│       │   ├── mod.rs            # Creature struct, stats, types
│       │   ├── registry.rs       # On-chain creature registry
│       │   └── rarity.rs         # Rarity tiers and stat scaling
│       ├── battle/
│       │   ├── mod.rs            # Battle orchestration
│       │   ├── resolver.rs       # Stat + type + RNG resolution
│       │   ├── matchmaking.rs    # Queue and matching logic
│       │   └── settlement.rs     # Percolator integration for payouts
│       ├── spawn/
│       │   ├── mod.rs            # Spawn trigger system
│       │   └── triggers.rs       # Event-to-creature mapping
│       ├── evolution/
│       │   ├── mod.rs            # Level, XP, stat growth
│       │   └── abilities.rs      # Ability unlock tree
│       └── arena/
│           ├── mod.rs            # Arena state management
│           └── insurance.rs      # Insurance fund + ADL integration
├── tests/
│   ├── ...                       # Percolator tests (original)
│   ├── test_battle.rs            # Battle resolution tests
│   ├── test_creature.rs          # Creature mint and stat tests
│   ├── test_spawn.rs             # Spawn trigger tests
│   └── test_settlement.rs        # Settlement invariant tests
├── Cargo.toml
├── Cargo.lock
├── audit.md                      # Percolator audit (original)
└── README.md
```

## Build

```bash
cargo build
```

## Test

```bash
cargo test
```

## Formal Verification

Percolator's invariants are machine-checked with Kani:

```bash
cargo install --locked kani-verifier
cargo kani setup
cargo kani
```

## Links

- **X:** [@Pokelator_](https://x.com/Pokelator_)
- **Site:** [pokelator.fun](https://pokelator.fun)
- **Token:** $POKÉLATOR on [pump.fun](https://pump.fun)
- **Percolator:** [github.com/aeyakovenko/percolator](https://github.com/aeyakovenko/percolator)
- **Dev:** [github.com/MontanaLuca32](https://github.com/MontanaLuca32)

## License

Apache-2.0 (inherited from Percolator)
