/**
 * Percolator SDK — types and instruction builders for wrapper programs.
 * Engine does not move tokens; wrapper must do SPL transfers and call engine.
 *
 * @see https://github.com/aeyakovenko/percolator
 */

// -----------------------------------------------------------------------------
// Types (align with engine state where useful for builders)
// -----------------------------------------------------------------------------

export type U128 = { lo: bigint; hi: bigint } | bigint;
export type I128 = { lo: bigint; hi: bigint } | bigint;

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

// -----------------------------------------------------------------------------
// Instruction discriminators (wrapper-specific; replace with your program's)
// -----------------------------------------------------------------------------

export const INSTRUCTION_NAMES = {
  deposit: 0x00,
  withdraw: 0x01,
  executeTrade: 0x02,
  keeperCrank: 0x03,
  addUser: 0x04,
  addLp: 0x05,
} as const;

// -----------------------------------------------------------------------------
// Instruction builders (serialize args for CPI; layout is wrapper-defined)
// -----------------------------------------------------------------------------

function encodeU64(n: number): Buffer {
  const b = Buffer.alloc(8);
  b.writeBigUInt64LE(BigInt(n));
  return b;
}

function encodeU128(n: bigint): Buffer {
  const b = Buffer.alloc(16);
  b.writeBigUInt64LE(n & BigInt('0xffffffffffffffff'), 0);
  b.writeBigUInt64LE(n >> BigInt(64), 8);
  return b;
}

function encodeI128(n: bigint): Buffer {
  const b = Buffer.alloc(16);
  const lo = n & BigInt('0xffffffffffffffff');
  const hi = n >> BigInt(64);
  b.writeBigUInt64LE(lo >= 0 ? lo : lo + BigInt(1) << BigInt(64), 0);
  b.writeBigInt64LE(hi, 8);
  return b;
}

/**
 * Build deposit instruction data.
 * Wrapper must: transfer tokens to vault, then call engine.deposit(idx, amount, now_slot).
 */
export function buildDepositInstructionData(args: DepositArgs): Buffer {
  const parts = [
    Buffer.from([INSTRUCTION_NAMES.deposit]),
    encodeU64(args.accountIndex),
    encodeU128(args.amount),
    encodeU64(args.nowSlot),
  ];
  return Buffer.concat(parts);
}

/**
 * Build withdraw instruction data.
 * Wrapper must: call engine.withdraw(...); if Ok, transfer tokens out of vault.
 */
export function buildWithdrawInstructionData(args: WithdrawArgs): Buffer {
  const parts = [
    Buffer.from([INSTRUCTION_NAMES.withdraw]),
    encodeU64(args.accountIndex),
    encodeU128(args.amount),
    encodeU64(args.nowSlot),
    encodeU64(args.oraclePrice),
  ];
  return Buffer.concat(parts);
}

/**
 * Build execute_trade instruction data.
 * Wrapper must validate signers and oracle, then call engine.execute_trade(...).
 */
export function buildExecuteTradeInstructionData(args: ExecuteTradeArgs): Buffer {
  const size = BigInt(args.size);
  const parts = [
    Buffer.from([INSTRUCTION_NAMES.executeTrade]),
    encodeU64(args.lpIndex),
    encodeU64(args.userIndex),
    encodeU64(args.nowSlot),
    encodeU64(args.oraclePrice),
    encodeI128(size),
  ];
  return Buffer.concat(parts);
}

/**
 * Build keeper_crank instruction data.
 * Permissionless; anyone can call. Wrapper forwards to engine.keeper_crank(...).
 */
export function buildKeeperCrankInstructionData(args: KeeperCrankArgs): Buffer {
  const parts = [
    Buffer.from([INSTRUCTION_NAMES.keeperCrank]),
    encodeU64(args.callerIndex),
    encodeU64(args.nowSlot),
    encodeU64(args.oraclePrice),
    encodeI128(BigInt(args.fundingRateBpsPerSlot)),
    Buffer.from([args.allowPanic ? 1 : 0]),
  ];
  return Buffer.concat(parts);
}

// -----------------------------------------------------------------------------
// Engine state decoder (read vault, OI, funding, etc. from state account)
// -----------------------------------------------------------------------------

export {
  decodeAccountSlab,
  decodeEngineState,
  decodeRiskEngine,
  formatBigint,
  type DecodedEngineAccount,
  type DecodedEngineState,
  type DecodedRiskEngine,
} from './decodeEngineState';
