/// Abilities that creatures can unlock through leveling.
/// Each ability modifies battle mechanics when active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Ability {
    /// +10% ATK for the first 3 turns.
    FirstStrike = 0,
    /// +15% DEF when HP drops below 30%.
    LastStand = 1,
    /// +20% type advantage multiplier.
    TypeMastery = 2,
    /// 5% chance to completely dodge an attack.
    Evasion = 3,
    /// Heal 5% HP per turn.
    Regeneration = 4,
    /// +25% crit chance.
    PrecisionStrike = 5,
    /// Reflect 10% of damage taken back to attacker.
    ThornArmor = 6,
    /// +10% to all stats for 2 turns after taking a crit.
    Revenge = 7,
    /// Reduce opponent's speed by 15%.
    Intimidate = 8,
    /// Double XP gain from this battle (no combat effect).
    QuickLearner = 9,
}

/// Checks if a new ability should be unlocked at the given level.
/// Returns None if no ability unlocks at this exact level.
pub fn check_unlock(level: u32) -> Option<Ability> {
    match level {
        6 => Some(Ability::FirstStrike),
        8 => Some(Ability::Evasion),
        10 => Some(Ability::QuickLearner),
        12 => Some(Ability::LastStand),
        15 => Some(Ability::TypeMastery),
        18 => Some(Ability::Regeneration),
        22 => Some(Ability::PrecisionStrike),
        28 => Some(Ability::ThornArmor),
        35 => Some(Ability::Revenge),
        45 => Some(Ability::Intimidate),
        _ => None,
    }
}

/// Returns the stat modifier for an ability.
pub fn ability_modifier(ability: &Ability) -> AbilityEffect {
    match ability {
        Ability::FirstStrike => AbilityEffect {
            stat: StatTarget::Atk,
            multiplier: 1.10,
            duration_turns: Some(3),
            condition: EffectCondition::Always,
        },
        Ability::LastStand => AbilityEffect {
            stat: StatTarget::Def,
            multiplier: 1.15,
            duration_turns: None,
            condition: EffectCondition::HpBelow(30),
        },
        Ability::TypeMastery => AbilityEffect {
            stat: StatTarget::TypeBonus,
            multiplier: 1.20,
            duration_turns: None,
            condition: EffectCondition::TypeAdvantage,
        },
        Ability::Evasion => AbilityEffect {
            stat: StatTarget::Dodge,
            multiplier: 0.05,
            duration_turns: None,
            condition: EffectCondition::Always,
        },
        Ability::Regeneration => AbilityEffect {
            stat: StatTarget::Hp,
            multiplier: 0.05,
            duration_turns: None,
            condition: EffectCondition::PerTurn,
        },
        Ability::PrecisionStrike => AbilityEffect {
            stat: StatTarget::CritChance,
            multiplier: 1.25,
            duration_turns: None,
            condition: EffectCondition::Always,
        },
        Ability::ThornArmor => AbilityEffect {
            stat: StatTarget::Reflect,
            multiplier: 0.10,
            duration_turns: None,
            condition: EffectCondition::OnHit,
        },
        Ability::Revenge => AbilityEffect {
            stat: StatTarget::AllStats,
            multiplier: 1.10,
            duration_turns: Some(2),
            condition: EffectCondition::OnCritReceived,
        },
        Ability::Intimidate => AbilityEffect {
            stat: StatTarget::OpponentSpeed,
            multiplier: 0.85,
            duration_turns: None,
            condition: EffectCondition::Always,
        },
        Ability::QuickLearner => AbilityEffect {
            stat: StatTarget::XpGain,
            multiplier: 2.0,
            duration_turns: None,
            condition: EffectCondition::Always,
        },
    }
}

#[derive(Debug, Clone)]
pub struct AbilityEffect {
    pub stat: StatTarget,
    pub multiplier: f64,
    pub duration_turns: Option<u32>,
    pub condition: EffectCondition,
}

#[derive(Debug, Clone)]
pub enum StatTarget {
    Hp,
    Atk,
    Def,
    SpAtk,
    SpDef,
    Speed,
    AllStats,
    CritChance,
    Dodge,
    Reflect,
    TypeBonus,
    OpponentSpeed,
    XpGain,
}

#[derive(Debug, Clone)]
pub enum EffectCondition {
    Always,
    HpBelow(u64),
    TypeAdvantage,
    PerTurn,
    OnHit,
    OnCritReceived,
}
