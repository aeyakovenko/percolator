/**
 * Decoder helpers for Percolator's raw RiskEngine account layout.
 *
 * Supports:
 * - Aggregate engine state decoding
 * - Full account slab decoding when the raw engine bytes are present
 * - Common wrapper layouts where the engine starts at offset 8
 */

import { PublicKey } from '@solana/web3.js';

const MAX_ACCOUNTS = 4096;
const BITMAP_WORDS = 64;
const ACCOUNT_SIZE = 240;
const ACCOUNT_ARRAY_SIZE = ACCOUNT_SIZE * MAX_ACCOUNTS;

const VAULT_OFFSET = 0;
const INSURANCE_BALANCE_OFFSET = 16;
const INSURANCE_FEE_REVENUE_OFFSET = 32;
const RISK_PARAMS_SIZE = 192;
const CURRENT_SLOT_OFFSET = 240;
const FUNDING_INDEX_OFFSET = 248;
const LAST_FUNDING_SLOT_OFFSET = 264;
const FUNDING_RATE_OFFSET = 272;
const LAST_CRANK_SLOT_OFFSET = 280;
const TOTAL_OI_OFFSET = 296;
const C_TOT_OFFSET = 312;
const PNL_POS_TOT_OFFSET = 328;
const USED_BITMAP_OFFSET = 456;
const NUM_USED_ACCOUNTS_OFFSET = 968;
const ACCOUNT_ARRAY_OFFSET = 9184;
const ENGINE_STATE_MIN_SIZE = NUM_USED_ACCOUNTS_OFFSET + 2;
const FULL_ENGINE_SIZE = ACCOUNT_ARRAY_OFFSET + ACCOUNT_ARRAY_SIZE;

const DEFAULT_ENGINE_OFFSETS = [0, 8];

const ACCOUNT_ID_OFFSET = 0;
const ACCOUNT_CAPITAL_OFFSET = 8;
const ACCOUNT_KIND_OFFSET = 24;
const ACCOUNT_PNL_OFFSET = 32;
const ACCOUNT_RESERVED_PNL_OFFSET = 48;
const ACCOUNT_WARMUP_STARTED_AT_SLOT_OFFSET = 56;
const ACCOUNT_WARMUP_SLOPE_OFFSET = 64;
const ACCOUNT_POSITION_SIZE_OFFSET = 80;
const ACCOUNT_ENTRY_PRICE_OFFSET = 96;
const ACCOUNT_FUNDING_INDEX_OFFSET = 104;
const ACCOUNT_MATCHER_PROGRAM_OFFSET = 120;
const ACCOUNT_MATCHER_CONTEXT_OFFSET = 152;
const ACCOUNT_OWNER_OFFSET = 184;
const ACCOUNT_FEE_CREDITS_OFFSET = 216;
const ACCOUNT_LAST_FEE_SLOT_OFFSET = 232;

function readU16(data: Uint8Array, offset: number): number {
  const view = new DataView(data.buffer, data.byteOffset + offset, 2);
  return view.getUint16(0, true);
}

function readU64(data: Uint8Array, offset: number): number {
  const view = new DataView(data.buffer, data.byteOffset + offset, 8);
  return Number(view.getBigUint64(0, true));
}

function readU64Bigint(data: Uint8Array, offset: number): bigint {
  const view = new DataView(data.buffer, data.byteOffset + offset, 8);
  return view.getBigUint64(0, true);
}

function readI64(data: Uint8Array, offset: number): number {
  const view = new DataView(data.buffer, data.byteOffset + offset, 8);
  return Number(view.getBigInt64(0, true));
}

function readU128(data: Uint8Array, offset: number): bigint {
  const view = new DataView(data.buffer, data.byteOffset + offset, 16);
  const lo = view.getBigUint64(0, true);
  const hi = view.getBigUint64(8, true);
  return lo + (hi << 64n);
}

function readI128(data: Uint8Array, offset: number): bigint {
  const view = new DataView(data.buffer, data.byteOffset + offset, 16);
  const lo = view.getBigUint64(0, true);
  const hi = view.getBigUint64(8, true);
  return BigInt.asIntN(128, lo + (hi << 64n));
}

function readByte(data: Uint8Array, offset: number): number {
  return data[offset] ?? 0;
}

function countBits(word: bigint): number {
  let count = 0;
  let remaining = word;
  while (remaining !== 0n) {
    remaining &= remaining - 1n;
    count += 1;
  }
  return count;
}

function readPubkey(data: Uint8Array, offset: number): string | null {
  const bytes = data.slice(offset, offset + 32);
  const isZero = bytes.every((value) => value === 0);
  if (isZero) return null;
  return new PublicKey(bytes).toBase58();
}

function toKind(kindByte: number): 'user' | 'lp' {
  return kindByte === 1 ? 'lp' : 'user';
}

function canRead(data: Uint8Array, offset: number, size: number): boolean {
  return offset >= 0 && offset + size <= data.length;
}

function computeCandidateScore(
  data: Uint8Array,
  offset: number,
  engine: DecodedEngineState,
): number {
  if (engine.numUsedAccounts !== undefined && engine.numUsedAccounts > MAX_ACCOUNTS) {
    return -1;
  }

  let score = 0;

  if (engine.vault >= engine.insuranceBalance) score += 1;
  if (engine.vault >= engine.cTot) score += 1;
  if (engine.vault >= engine.cTot + engine.insuranceBalance) score += 2;
  if (engine.currentSlot >= engine.lastCrankSlot) score += 1;
  if (engine.currentSlot >= engine.lastFundingSlot) score += 1;

  if (canRead(data, offset + USED_BITMAP_OFFSET, BITMAP_WORDS * 8)) {
    let usedCount = 0;
    for (let i = 0; i < BITMAP_WORDS; i += 1) {
      usedCount += countBits(readU64Bigint(data, offset + USED_BITMAP_OFFSET + i * 8));
    }

    if (engine.numUsedAccounts !== undefined) {
      if (usedCount === engine.numUsedAccounts) {
        score += 4;
      } else if (Math.abs(usedCount - engine.numUsedAccounts) <= 2) {
        score += 1;
      } else {
        score -= 2;
      }
    }
  }

  return score;
}

export interface DecodeEngineOptions {
  offset?: number;
}

export interface DecodeRiskEngineOptions {
  offsets?: number[];
}

export interface DecodedEngineState {
  vault: bigint;
  insuranceBalance: bigint;
  insuranceFeeRevenue: bigint;
  currentSlot: number;
  lastFundingSlot: number;
  fundingRateBpsPerSlot: number;
  lastCrankSlot: number;
  totalOpenInterest: bigint;
  cTot: bigint;
  pnlPosTot: bigint;
  lifetimeLiquidations?: number;
  numUsedAccounts?: number;
}

export interface DecodedEngineAccount {
  accountIndex: number;
  accountId: bigint;
  kind: 'user' | 'lp';
  capital: bigint;
  pnl: bigint;
  reservedPnl: number;
  warmupStartedAtSlot: number;
  warmupSlopePerStep: bigint;
  positionSize: bigint;
  entryPrice: number;
  fundingIndex: bigint;
  matcherProgram: string | null;
  matcherContext: string | null;
  owner: string | null;
  feeCredits: bigint;
  lastFeeSlot: number;
}

export interface DecodedRiskEngine {
  offset: number;
  engine: DecodedEngineState;
  accounts: DecodedEngineAccount[];
  accountsDecoded: boolean;
}

function decodeEngineStateAt(data: Uint8Array, offset = 0): DecodedEngineState | null {
  if (!canRead(data, offset, ENGINE_STATE_MIN_SIZE)) return null;

  return {
    vault: readU128(data, offset + VAULT_OFFSET),
    insuranceBalance: readU128(data, offset + INSURANCE_BALANCE_OFFSET),
    insuranceFeeRevenue: readU128(data, offset + INSURANCE_FEE_REVENUE_OFFSET),
    currentSlot: readU64(data, offset + CURRENT_SLOT_OFFSET),
    lastFundingSlot: readU64(data, offset + LAST_FUNDING_SLOT_OFFSET),
    fundingRateBpsPerSlot: readI64(data, offset + FUNDING_RATE_OFFSET),
    lastCrankSlot: readU64(data, offset + LAST_CRANK_SLOT_OFFSET),
    totalOpenInterest: readU128(data, offset + TOTAL_OI_OFFSET),
    cTot: readU128(data, offset + C_TOT_OFFSET),
    pnlPosTot: readU128(data, offset + PNL_POS_TOT_OFFSET),
    lifetimeLiquidations: readU64(data, offset + 376),
    numUsedAccounts: readU16(data, offset + NUM_USED_ACCOUNTS_OFFSET),
  };
}

function decodeEngineAccountAt(
  data: Uint8Array,
  offset: number,
  accountIndex: number,
): DecodedEngineAccount {
  return {
    accountIndex,
    accountId: readU64Bigint(data, offset + ACCOUNT_ID_OFFSET),
    kind: toKind(readByte(data, offset + ACCOUNT_KIND_OFFSET)),
    capital: readU128(data, offset + ACCOUNT_CAPITAL_OFFSET),
    pnl: readI128(data, offset + ACCOUNT_PNL_OFFSET),
    reservedPnl: readU64(data, offset + ACCOUNT_RESERVED_PNL_OFFSET),
    warmupStartedAtSlot: readU64(data, offset + ACCOUNT_WARMUP_STARTED_AT_SLOT_OFFSET),
    warmupSlopePerStep: readU128(data, offset + ACCOUNT_WARMUP_SLOPE_OFFSET),
    positionSize: readI128(data, offset + ACCOUNT_POSITION_SIZE_OFFSET),
    entryPrice: readU64(data, offset + ACCOUNT_ENTRY_PRICE_OFFSET),
    fundingIndex: readI128(data, offset + ACCOUNT_FUNDING_INDEX_OFFSET),
    matcherProgram: readPubkey(data, offset + ACCOUNT_MATCHER_PROGRAM_OFFSET),
    matcherContext: readPubkey(data, offset + ACCOUNT_MATCHER_CONTEXT_OFFSET),
    owner: readPubkey(data, offset + ACCOUNT_OWNER_OFFSET),
    feeCredits: readI128(data, offset + ACCOUNT_FEE_CREDITS_OFFSET),
    lastFeeSlot: readU64(data, offset + ACCOUNT_LAST_FEE_SLOT_OFFSET),
  };
}

export function decodeEngineState(
  data: Uint8Array,
  options: DecodeEngineOptions = {},
): DecodedEngineState | null {
  return decodeEngineStateAt(data, options.offset ?? 0);
}

export function decodeAccountSlab(
  data: Uint8Array,
  options: DecodeEngineOptions = {},
): DecodedEngineAccount[] | null {
  const offset = options.offset ?? 0;
  if (!canRead(data, offset, FULL_ENGINE_SIZE)) return null;

  const accounts: DecodedEngineAccount[] = [];
  const accountBase = offset + ACCOUNT_ARRAY_OFFSET;

  for (let block = 0; block < BITMAP_WORDS; block += 1) {
    let word = readU64Bigint(data, offset + USED_BITMAP_OFFSET + block * 8);
    while (word !== 0n) {
      const lowestBit = word & -word;
      const bitIndex = lowestBit.toString(2).length - 1;
      const accountIndex = block * 64 + bitIndex;
      const accountOffset = accountBase + accountIndex * ACCOUNT_SIZE;
      accounts.push(decodeEngineAccountAt(data, accountOffset, accountIndex));
      word &= word - 1n;
    }
  }

  return accounts;
}

export function decodeRiskEngine(
  data: Uint8Array,
  options: DecodeRiskEngineOptions = {},
): DecodedRiskEngine | null {
  const offsets = options.offsets ?? DEFAULT_ENGINE_OFFSETS;

  let best: DecodedRiskEngine | null = null;
  let bestScore = -1;

  for (const offset of offsets) {
    const engine = decodeEngineStateAt(data, offset);
    if (!engine) continue;

    const score = computeCandidateScore(data, offset, engine);
    if (score < bestScore) continue;

    const accounts = decodeAccountSlab(data, { offset }) ?? [];
    best = {
      offset,
      engine,
      accounts,
      accountsDecoded: accounts.length > 0 || canRead(data, offset, FULL_ENGINE_SIZE),
    };
    bestScore = score;
  }

  return best;
}

export function formatBigint(n: bigint): string {
  if (n >= 1_000_000_000_000_000n) return (Number(n) / 1e9).toFixed(0) + 'B';
  if (n >= 1_000_000_000_000n) return (Number(n) / 1e6).toFixed(2) + 'M';
  if (n >= 1_000_000_000n) return (Number(n) / 1e3).toFixed(2) + 'K';
  return n.toString();
}
