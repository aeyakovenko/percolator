import { Connection, PublicKey } from '@solana/web3.js';

// =============================================================================
// Constants from Rust (ported from cli/src/solana/slab.ts)
// =============================================================================
const MAGIC: bigint = 0x504552434f4c4154n; // "PERCOLAT"
const HEADER_LEN = 72;
const CONFIG_OFFSET = HEADER_LEN;
const CONFIG_LEN = 256;
const RESERVED_OFF = 48;

// Engine offsets
const ENGINE_OFF = 328;
const ENGINE_VAULT_OFF = 0;
const ENGINE_INSURANCE_OFF = 16;
const ENGINE_PARAMS_OFF = 48;
const ENGINE_CURRENT_SLOT_OFF = 192;
const ENGINE_FUNDING_INDEX_OFF = 200;
const ENGINE_LAST_FUNDING_SLOT_OFF = 216;
const ENGINE_LOSS_ACCUM_OFF = 224;
const ENGINE_RISK_REDUCTION_ONLY_OFF = 240;
const ENGINE_RISK_REDUCTION_WITHDRAWN_OFF = 248;
const ENGINE_WARMUP_PAUSED_OFF = 264;
const ENGINE_WARMUP_PAUSE_SLOT_OFF = 272;
const ENGINE_LAST_CRANK_SLOT_OFF = 280;
const ENGINE_MAX_CRANK_STALENESS_OFF = 288;
const ENGINE_TOTAL_OI_OFF = 296;
const ENGINE_WARMED_POS_OFF = 312;
const ENGINE_WARMED_NEG_OFF = 328;
const ENGINE_WARMUP_INSURANCE_OFF = 344;
// After warmup_insurance_reserved, there are large ADL scratch arrays and pending fields.
// Empirically verified offsets (2026-01 devnet):
// - liq_cursor: u16 @ 86412
// - gc_cursor: u16 @ 86414
// - last_full_sweep_start_slot: u64 @ 86416
// - last_full_sweep_completed_slot: u64 @ 86424
// - crank_step: u8 @ 86432
const ENGINE_LIQ_CURSOR_OFF = 86412;
const ENGINE_GC_CURSOR_OFF = 86414;
const ENGINE_LAST_SWEEP_START_OFF = 86416;
const ENGINE_LAST_SWEEP_COMPLETE_OFF = 86424;
const ENGINE_CRANK_STEP_OFF = 86432;
const ENGINE_LIFETIME_LIQUIDATIONS_OFF = 86440;
const ENGINE_LIFETIME_FORCE_CLOSES_OFF = 86448;
const ENGINE_BITMAP_OFF = 86520;
const ENGINE_NUM_USED_OFF = 87032;
const ENGINE_NEXT_ACCOUNT_ID_OFF = 87040;
const ENGINE_ACCOUNTS_OFF = 95256;

// RiskParams offsets
const PARAMS_WARMUP_PERIOD_OFF = 0;
const PARAMS_MAINTENANCE_MARGIN_OFF = 8;
const PARAMS_INITIAL_MARGIN_OFF = 16;
const PARAMS_TRADING_FEE_OFF = 24;
const PARAMS_MAX_ACCOUNTS_OFF = 32;
const PARAMS_NEW_ACCOUNT_FEE_OFF = 40;
const PARAMS_RISK_THRESHOLD_OFF = 56;
const PARAMS_MAINTENANCE_FEE_OFF = 72;
const PARAMS_MAX_CRANK_STALENESS_OFF = 88;
const PARAMS_LIQUIDATION_FEE_BPS_OFF = 96;
const PARAMS_LIQUIDATION_FEE_CAP_OFF = 104;
const PARAMS_LIQUIDATION_BUFFER_OFF = 120;
const PARAMS_MIN_LIQUIDATION_OFF = 128;

// Account layout offsets
const ACCT_ACCOUNT_ID_OFF = 0;
const ACCT_CAPITAL_OFF = 8;
const ACCT_KIND_OFF = 24;
const ACCT_PNL_OFF = 32;
const ACCT_RESERVED_PNL_OFF = 48;
const ACCT_WARMUP_STARTED_OFF = 56;
const ACCT_WARMUP_SLOPE_OFF = 64;
const ACCT_POSITION_SIZE_OFF = 80;
const ACCT_ENTRY_PRICE_OFF = 96;
const ACCT_FUNDING_INDEX_OFF = 104;
const ACCT_MATCHER_PROGRAM_OFF = 120;
const ACCT_MATCHER_CONTEXT_OFF = 152;
const ACCT_OWNER_OFF = 184;
const ACCT_FEE_CREDITS_OFF = 216;
const ACCT_LAST_FEE_SLOT_OFF = 232;

const BITMAP_WORDS = 64;
const MAX_ACCOUNTS = 4096;
const ACCOUNT_SIZE = 248;

// =============================================================================
// Interfaces
// =============================================================================

export interface SlabHeader {
  magic: bigint;
  version: number;
  bump: number;
  admin: string;
  nonce: bigint;
  lastThrUpdateSlot: bigint;
}

export interface MarketConfig {
  collateralMint: string;
  vaultPubkey: string;
  indexFeedId: string;
  maxStalenessSlots: bigint;
  confFilterBps: number;
  vaultAuthorityBump: number;
  invert: boolean;
  unitScale: number;
}

export interface InsuranceFund {
  balance: bigint;
  feeRevenue: bigint;
}

export interface RiskParams {
  warmupPeriodSlots: bigint;
  maintenanceMarginBps: bigint;
  initialMarginBps: bigint;
  tradingFeeBps: bigint;
  maxAccounts: bigint;
  newAccountFee: bigint;
  riskReductionThreshold: bigint;
  maintenanceFeePerSlot: bigint;
  maxCrankStalenessSlots: bigint;
  liquidationFeeBps: bigint;
  liquidationFeeCap: bigint;
  liquidationBufferBps: bigint;
  minLiquidationAbs: bigint;
}

export interface EngineState {
  vault: bigint;
  insuranceFund: InsuranceFund;
  currentSlot: bigint;
  fundingIndexQpbE6: bigint;
  lastFundingSlot: bigint;
  lossAccum: bigint;
  riskReductionOnly: boolean;
  riskReductionModeWithdrawn: bigint;
  warmupPaused: boolean;
  warmupPauseSlot: bigint;
  lastCrankSlot: bigint;
  maxCrankStalenessSlots: bigint;
  totalOpenInterest: bigint;
  warmedPosTotal: bigint;
  warmedNegTotal: bigint;
  warmupInsuranceReserved: bigint;
  lastSweepStartSlot: bigint;
  lastSweepCompleteSlot: bigint;
  crankStep: number;
  lifetimeLiquidations: bigint;
  lifetimeForceCloses: bigint;
  numUsedAccounts: number;
  nextAccountId: bigint;
}

export enum AccountKind {
  User = 0,
  LP = 1,
}

export interface SlabAccount {
  kind: AccountKind;
  accountId: bigint;
  capital: bigint;
  pnl: bigint;
  reservedPnl: bigint;
  warmupStartedAtSlot: bigint;
  warmupSlopePerStep: bigint;
  positionSize: bigint;
  entryPrice: bigint;
  fundingIndex: bigint;
  matcherProgram: string;
  matcherContext: string;
  owner: string;
  feeCredits: bigint;
  lastFeeSlot: bigint;
}

export interface ParsedSlabState {
  header: SlabHeader;
  config: MarketConfig;
  params: RiskParams;
  engine: EngineState;
  accounts: { idx: number; account: SlabAccount }[];
}

// =============================================================================
// Helper functions
// =============================================================================

function readI128LE(buf: Buffer, offset: number): bigint {
  const lo = buf.readBigUInt64LE(offset);
  const hi = buf.readBigInt64LE(offset + 8);
  return (hi << 64n) | lo;
}

function readU128LE(buf: Buffer, offset: number): bigint {
  const lo = buf.readBigUInt64LE(offset);
  const hi = buf.readBigUInt64LE(offset + 8);
  return (hi << 64n) | lo;
}

// =============================================================================
// Parsing Functions
// =============================================================================

export function parseHeader(data: Buffer): SlabHeader {
  if (data.length < HEADER_LEN) {
    throw new Error(`Slab data too short for header: ${data.length} < ${HEADER_LEN}`);
  }

  const magic = data.readBigUInt64LE(0);
  if (magic !== MAGIC) {
    throw new Error(`Invalid slab magic: expected ${MAGIC.toString(16)}, got ${magic.toString(16)}`);
  }

  const version = data.readUInt32LE(8);
  const bump = data.readUInt8(12);
  const admin = new PublicKey(data.subarray(16, 48)).toBase58();
  const nonce = data.readBigUInt64LE(RESERVED_OFF);
  const lastThrUpdateSlot = data.readBigUInt64LE(RESERVED_OFF + 8);

  return { magic, version, bump, admin, nonce, lastThrUpdateSlot };
}

export function parseConfig(data: Buffer): MarketConfig {
  const minLen = CONFIG_OFFSET + CONFIG_LEN;
  if (data.length < minLen) {
    throw new Error(`Slab data too short for config: ${data.length} < ${minLen}`);
  }

  let off = CONFIG_OFFSET;

  const collateralMint = new PublicKey(data.subarray(off, off + 32)).toBase58();
  off += 32;

  const vaultPubkey = new PublicKey(data.subarray(off, off + 32)).toBase58();
  off += 32;

  const indexFeedId = new PublicKey(data.subarray(off, off + 32)).toBase58();
  off += 32;

  const maxStalenessSlots = data.readBigUInt64LE(off);
  off += 8;

  const confFilterBps = data.readUInt16LE(off);
  off += 2;

  const vaultAuthorityBump = data.readUInt8(off);
  off += 1;

  const invert = data.readUInt8(off) !== 0;
  off += 1;

  const unitScale = data.readUInt32LE(off);

  return {
    collateralMint,
    vaultPubkey,
    indexFeedId,
    maxStalenessSlots,
    confFilterBps,
    vaultAuthorityBump,
    invert,
    unitScale,
  };
}

export function parseParams(data: Buffer): RiskParams {
  const base = ENGINE_OFF + ENGINE_PARAMS_OFF;
  if (data.length < base + 160) {
    throw new Error('Slab data too short for RiskParams');
  }

  return {
    warmupPeriodSlots: data.readBigUInt64LE(base + PARAMS_WARMUP_PERIOD_OFF),
    maintenanceMarginBps: data.readBigUInt64LE(base + PARAMS_MAINTENANCE_MARGIN_OFF),
    initialMarginBps: data.readBigUInt64LE(base + PARAMS_INITIAL_MARGIN_OFF),
    tradingFeeBps: data.readBigUInt64LE(base + PARAMS_TRADING_FEE_OFF),
    maxAccounts: data.readBigUInt64LE(base + PARAMS_MAX_ACCOUNTS_OFF),
    newAccountFee: readU128LE(data, base + PARAMS_NEW_ACCOUNT_FEE_OFF),
    riskReductionThreshold: readU128LE(data, base + PARAMS_RISK_THRESHOLD_OFF),
    maintenanceFeePerSlot: readU128LE(data, base + PARAMS_MAINTENANCE_FEE_OFF),
    maxCrankStalenessSlots: data.readBigUInt64LE(base + PARAMS_MAX_CRANK_STALENESS_OFF),
    liquidationFeeBps: data.readBigUInt64LE(base + PARAMS_LIQUIDATION_FEE_BPS_OFF),
    liquidationFeeCap: readU128LE(data, base + PARAMS_LIQUIDATION_FEE_CAP_OFF),
    liquidationBufferBps: data.readBigUInt64LE(base + PARAMS_LIQUIDATION_BUFFER_OFF),
    minLiquidationAbs: readU128LE(data, base + PARAMS_MIN_LIQUIDATION_OFF),
  };
}

export function parseEngine(data: Buffer): EngineState {
  const base = ENGINE_OFF;
  if (data.length < base + ENGINE_ACCOUNTS_OFF) {
    throw new Error('Slab data too short for RiskEngine');
  }

  return {
    vault: readU128LE(data, base + ENGINE_VAULT_OFF),
    insuranceFund: {
      balance: readU128LE(data, base + ENGINE_INSURANCE_OFF),
      feeRevenue: readU128LE(data, base + ENGINE_INSURANCE_OFF + 16),
    },
    currentSlot: data.readBigUInt64LE(base + ENGINE_CURRENT_SLOT_OFF),
    fundingIndexQpbE6: readI128LE(data, base + ENGINE_FUNDING_INDEX_OFF),
    lastFundingSlot: data.readBigUInt64LE(base + ENGINE_LAST_FUNDING_SLOT_OFF),
    lossAccum: readU128LE(data, base + ENGINE_LOSS_ACCUM_OFF),
    riskReductionOnly: data.readUInt8(base + ENGINE_RISK_REDUCTION_ONLY_OFF) !== 0,
    riskReductionModeWithdrawn: readU128LE(data, base + ENGINE_RISK_REDUCTION_WITHDRAWN_OFF),
    warmupPaused: data.readUInt8(base + ENGINE_WARMUP_PAUSED_OFF) !== 0,
    warmupPauseSlot: data.readBigUInt64LE(base + ENGINE_WARMUP_PAUSE_SLOT_OFF),
    lastCrankSlot: data.readBigUInt64LE(base + ENGINE_LAST_CRANK_SLOT_OFF),
    maxCrankStalenessSlots: data.readBigUInt64LE(base + ENGINE_MAX_CRANK_STALENESS_OFF),
    totalOpenInterest: readU128LE(data, base + ENGINE_TOTAL_OI_OFF),
    warmedPosTotal: readU128LE(data, base + ENGINE_WARMED_POS_OFF),
    warmedNegTotal: readU128LE(data, base + ENGINE_WARMED_NEG_OFF),
    warmupInsuranceReserved: readU128LE(data, base + ENGINE_WARMUP_INSURANCE_OFF),
    lastSweepStartSlot: data.readBigUInt64LE(base + ENGINE_LAST_SWEEP_START_OFF),
    lastSweepCompleteSlot: data.readBigUInt64LE(base + ENGINE_LAST_SWEEP_COMPLETE_OFF),
    crankStep: data.readUInt8(base + ENGINE_CRANK_STEP_OFF),
    lifetimeLiquidations: data.readBigUInt64LE(base + ENGINE_LIFETIME_LIQUIDATIONS_OFF),
    lifetimeForceCloses: data.readBigUInt64LE(base + ENGINE_LIFETIME_FORCE_CLOSES_OFF),
    numUsedAccounts: data.readUInt16LE(base + ENGINE_NUM_USED_OFF),
    nextAccountId: data.readBigUInt64LE(base + ENGINE_NEXT_ACCOUNT_ID_OFF),
  };
}

export function parseUsedIndices(data: Buffer): number[] {
  const base = ENGINE_OFF + ENGINE_BITMAP_OFF;
  if (data.length < base + BITMAP_WORDS * 8) {
    throw new Error('Slab data too short for bitmap');
  }

  const used: number[] = [];
  for (let word = 0; word < BITMAP_WORDS; word++) {
    const bits = data.readBigUInt64LE(base + word * 8);
    if (bits === 0n) continue;
    for (let bit = 0; bit < 64; bit++) {
      if ((bits >> BigInt(bit)) & 1n) {
        used.push(word * 64 + bit);
      }
    }
  }
  return used;
}

export function maxAccountIndex(dataLen: number): number {
  const accountsEnd = dataLen - ENGINE_OFF - ENGINE_ACCOUNTS_OFF;
  if (accountsEnd <= 0) return 0;
  return Math.floor(accountsEnd / ACCOUNT_SIZE);
}

export function parseAccount(data: Buffer, idx: number): SlabAccount {
  const maxIdx = maxAccountIndex(data.length);
  if (idx < 0 || idx >= maxIdx) {
    throw new Error(`Account index out of range: ${idx} (max: ${maxIdx - 1})`);
  }

  const base = ENGINE_OFF + ENGINE_ACCOUNTS_OFF + idx * ACCOUNT_SIZE;
  if (data.length < base + ACCOUNT_SIZE) {
    throw new Error('Slab data too short for account');
  }

  // Detect LP accounts by checking if matcher_program is non-zero
  const matcherProgramBytes = data.subarray(base + ACCT_MATCHER_PROGRAM_OFF, base + ACCT_MATCHER_PROGRAM_OFF + 32);
  const isLp = !matcherProgramBytes.every((b: number) => b === 0);
  const kind = isLp ? AccountKind.LP : AccountKind.User;

  return {
    kind,
    accountId: data.readBigUInt64LE(base + ACCT_ACCOUNT_ID_OFF),
    capital: readU128LE(data, base + ACCT_CAPITAL_OFF),
    pnl: readI128LE(data, base + ACCT_PNL_OFF),
    reservedPnl: data.readBigUInt64LE(base + ACCT_RESERVED_PNL_OFF),
    warmupStartedAtSlot: data.readBigUInt64LE(base + ACCT_WARMUP_STARTED_OFF),
    warmupSlopePerStep: data.readBigUInt64LE(base + ACCT_WARMUP_SLOPE_OFF),
    positionSize: readI128LE(data, base + ACCT_POSITION_SIZE_OFF),
    entryPrice: data.readBigUInt64LE(base + ACCT_ENTRY_PRICE_OFF),
    fundingIndex: readI128LE(data, base + ACCT_FUNDING_INDEX_OFF),
    matcherProgram: new PublicKey(data.subarray(base + ACCT_MATCHER_PROGRAM_OFF, base + ACCT_MATCHER_PROGRAM_OFF + 32)).toBase58(),
    matcherContext: new PublicKey(data.subarray(base + ACCT_MATCHER_CONTEXT_OFF, base + ACCT_MATCHER_CONTEXT_OFF + 32)).toBase58(),
    owner: new PublicKey(data.subarray(base + ACCT_OWNER_OFF, base + ACCT_OWNER_OFF + 32)).toBase58(),
    feeCredits: readI128LE(data, base + ACCT_FEE_CREDITS_OFF),
    lastFeeSlot: data.readBigUInt64LE(base + ACCT_LAST_FEE_SLOT_OFF),
  };
}

export function parseAllAccounts(data: Buffer): { idx: number; account: SlabAccount }[] {
  const indices = parseUsedIndices(data);
  const maxIdx = maxAccountIndex(data.length);
  const validIndices = indices.filter(idx => idx < maxIdx);
  return validIndices.map(idx => ({
    idx,
    account: parseAccount(data, idx),
  }));
}

// =============================================================================
// Main fetch and parse function
// =============================================================================

export async function fetchAndParseSlab(
  connection: Connection,
  slabPubkey: PublicKey
): Promise<ParsedSlabState> {
  const accountInfo = await connection.getAccountInfo(slabPubkey);
  if (!accountInfo) {
    throw new Error(`Slab account not found: ${slabPubkey.toBase58()}`);
  }

  const data = Buffer.from(accountInfo.data);

  return {
    header: parseHeader(data),
    config: parseConfig(data),
    params: parseParams(data),
    engine: parseEngine(data),
    accounts: parseAllAccounts(data),
  };
}

export async function fetchSlabRaw(
  connection: Connection,
  slabPubkey: PublicKey
): Promise<Buffer> {
  const accountInfo = await connection.getAccountInfo(slabPubkey);
  if (!accountInfo) {
    throw new Error(`Slab account not found: ${slabPubkey.toBase58()}`);
  }
  return Buffer.from(accountInfo.data);
}

// =============================================================================
// Utility functions for formatting
// =============================================================================

// Convert lamports (or micro units) to human-readable format
export function formatLamports(lamports: bigint, decimals: number = 9): string {
  const divisor = BigInt(10 ** decimals);
  const whole = lamports / divisor;
  const fraction = lamports % divisor;
  const fractionStr = fraction.toString().padStart(decimals, '0').replace(/0+$/, '');
  return fractionStr ? `${whole}.${fractionStr}` : whole.toString();
}

// Convert position size to human-readable format (signed, 6 decimals for perp)
export function formatPositionSize(size: bigint): string {
  const isNegative = size < 0n;
  const absSize = isNegative ? -size : size;
  const formatted = formatLamports(absSize, 6);
  return isNegative ? `-${formatted}` : formatted;
}

// Calculate margin usage percentage
export function calculateMarginUsage(
  positionSize: bigint,
  entryPrice: bigint,
  capital: bigint,
  initialMarginBps: bigint
): number {
  if (capital === 0n) return 0;

  const absPosition = positionSize < 0n ? -positionSize : positionSize;
  // Position notional = |size| * entry_price / 1e6 (price in micro-units)
  const notional = (absPosition * entryPrice) / 1000000n;
  // Required margin = notional * initialMarginBps / 10000
  const requiredMargin = (notional * initialMarginBps) / 10000n;

  // Margin usage = requiredMargin / capital * 100
  return Number((requiredMargin * 10000n) / capital) / 100;
}

// Find account by owner pubkey
export function findAccountByOwner(
  accounts: { idx: number; account: SlabAccount }[],
  ownerPubkey: string
): { idx: number; account: SlabAccount } | undefined {
  return accounts.find(({ account }) => account.owner === ownerPubkey);
}
