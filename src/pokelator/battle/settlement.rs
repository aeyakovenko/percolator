use super::{Battle, BattleResult, BattleStatus};

/// Settlement errors that can occur during Percolator integration.
#[derive(Debug)]
pub enum SettlementError {
    BattleNotResolved,
    AlreadySettled,
    InsufficientInsurance,
    RiskEngineError(String),
}

/// Processes a resolved battle through Percolator's risk engine.
///
/// Settlement flow:
/// 1. Verify battle is in Resolved state
/// 2. Route loser's collateral loss through risk engine
/// 3. Deduct insurance fee to insurance fund
/// 4. Credit winner's payout through risk engine
/// 5. If insurance insufficient, trigger ADL waterfall
/// 6. Mark battle as Settled
///
/// The Percolator invariant holds throughout:
///   Withdrawals_a <= Deposits_a + LossPaid_not_a + SpendableInsurance_end
pub fn settle_battle(
    battle: &mut Battle,
    result: &BattleResult,
    insurance_balance: &mut u64,
    insurance_floor: u64,
) -> Result<SettlementAction, SettlementError> {
    if battle.status != BattleStatus::Resolved {
        return Err(SettlementError::BattleNotResolved);
    }

    if battle.status == BattleStatus::Settled {
        return Err(SettlementError::AlreadySettled);
    }

    // Credit insurance fee
    *insurance_balance += result.insurance_fee_lamports;

    let spendable_insurance = if *insurance_balance > insurance_floor {
        *insurance_balance - insurance_floor
    } else {
        0
    };

    // Check if the winner's payout is fully backed
    let winner_collateral = if result.winner == battle.player_a.owner {
        battle.player_a.collateral_lamports
    } else {
        battle.player_b.collateral_lamports
    };

    let profit = result.winner_payout_lamports.saturating_sub(winner_collateral);

    let action = if profit <= result.loser_loss_lamports {
        // Normal case: loser's loss covers winner's profit
        SettlementAction::Normal {
            winner: result.winner,
            winner_payout: result.winner_payout_lamports,
            loser: result.loser,
            loser_loss: result.loser_loss_lamports,
            insurance_fee: result.insurance_fee_lamports,
        }
    } else if profit <= result.loser_loss_lamports + spendable_insurance {
        // Insurance covers the gap
        let insurance_used = profit - result.loser_loss_lamports;
        *insurance_balance -= insurance_used;
        SettlementAction::InsuranceBacked {
            winner: result.winner,
            winner_payout: result.winner_payout_lamports,
            loser: result.loser,
            loser_loss: result.loser_loss_lamports,
            insurance_used,
        }
    } else {
        // ADL: socialize the shortfall
        let shortfall = profit - result.loser_loss_lamports - spendable_insurance;
        let haircut_payout = result.winner_payout_lamports - shortfall;
        SettlementAction::ADL {
            winner: result.winner,
            winner_payout: haircut_payout,
            loser: result.loser,
            loser_loss: result.loser_loss_lamports,
            shortfall,
        }
    };

    battle.status = BattleStatus::Settled;
    Ok(action)
}

/// The settlement action to execute after risk engine processing.
#[derive(Debug)]
pub enum SettlementAction {
    /// Loser's loss fully covers winner's profit.
    Normal {
        winner: [u8; 32],
        winner_payout: u64,
        loser: [u8; 32],
        loser_loss: u64,
        insurance_fee: u64,
    },
    /// Insurance fund covers a shortfall.
    InsuranceBacked {
        winner: [u8; 32],
        winner_payout: u64,
        loser: [u8; 32],
        loser_loss: u64,
        insurance_used: u64,
    },
    /// Auto-deleveraging: winner's payout is reduced (haircut).
    ADL {
        winner: [u8; 32],
        winner_payout: u64,
        loser: [u8; 32],
        loser_loss: u64,
        shortfall: u64,
    },
}
