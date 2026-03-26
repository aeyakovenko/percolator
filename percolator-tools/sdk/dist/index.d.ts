/**
 * Percolator SDK — types and instruction builders for wrapper programs.
 * Engine does not move tokens; wrapper must do SPL transfers and call engine.
 *
 * @see https://github.com/aeyakovenko/percolator
 */
export type U128 = {
    lo: bigint;
    hi: bigint;
} | bigint;
export type I128 = {
    lo: bigint;
    hi: bigint;
} | bigint;
export interface RiskEngineState {
    vault: bigint;
    insuranceBalance: bigint;
    cTot: bigint;
    pnlPosTot: bigint;
    totalOpenInterest: bigint;
    currentSlot: number;
    lastCrankSlot: number;
    fundingIndexQpbE6: bigint;
    lastFundingSlot: number;
    fundingRateBpsPerSlotLast: number;
    lifetimeLiquidations: number;
    numUsedAccounts: number;
}
export interface AccountSummary {
    accountIndex: number;
    accountId: number;
    kind: 'user' | 'lp';
    capital: bigint;
    pnl: bigint;
    positionSize: bigint;
    entryPrice: number;
    owner: string;
}
export interface DepositArgs {
    accountIndex: number;
    amount: bigint;
    nowSlot: number;
}
export interface WithdrawArgs {
    accountIndex: number;
    amount: bigint;
    nowSlot: number;
    oraclePrice: number;
}
export interface ExecuteTradeArgs {
    lpIndex: number;
    userIndex: number;
    nowSlot: number;
    oraclePrice: number;
    size: bigint;
}
export interface KeeperCrankArgs {
    callerIndex: number;
    nowSlot: number;
    oraclePrice: number;
    fundingRateBpsPerSlot: number;
    allowPanic: boolean;
}
export declare const INSTRUCTION_NAMES: {
    readonly deposit: 0;
    readonly withdraw: 1;
    readonly executeTrade: 2;
    readonly keeperCrank: 3;
    readonly addUser: 4;
    readonly addLp: 5;
};
/**
 * Build deposit instruction data.
 * Wrapper must: transfer tokens to vault, then call engine.deposit(idx, amount, now_slot).
 */
export declare function buildDepositInstructionData(args: DepositArgs): Buffer;
/**
 * Build withdraw instruction data.
 * Wrapper must: call engine.withdraw(...); if Ok, transfer tokens out of vault.
 */
export declare function buildWithdrawInstructionData(args: WithdrawArgs): Buffer;
/**
 * Build execute_trade instruction data.
 * Wrapper must validate signers and oracle, then call engine.execute_trade(...).
 */
export declare function buildExecuteTradeInstructionData(args: ExecuteTradeArgs): Buffer;
/**
 * Build keeper_crank instruction data.
 * Permissionless; anyone can call. Wrapper forwards to engine.keeper_crank(...).
 */
export declare function buildKeeperCrankInstructionData(args: KeeperCrankArgs): Buffer;
export { decodeAccountSlab, decodeEngineState, decodeRiskEngine, formatBigint, type DecodedEngineAccount, type DecodedEngineState, type DecodedRiskEngine, } from './decodeEngineState';
