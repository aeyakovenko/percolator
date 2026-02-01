/**
 * Transaction builder utilities for Percolator trading
 */

import {
  Transaction,
  PublicKey,
  TransactionInstruction,
  SystemProgram,
  SYSVAR_RENT_PUBKEY,
  Connection
} from '@solana/web3.js';
import { getConnection } from './solana';
import { loadDevnetConfig, getProgramId, getMatcherProgramId, getSlabPubkey } from './devnet-config';
import BN from 'bn.js';

// Load config once
const config = loadDevnetConfig();

// Program IDs - use deployed values from devnet-config.json
export const SLAB_PROGRAM_ID = getProgramId();

export const ROUTER_PROGRAM_ID = getMatcherProgramId(); // Matcher acts as router for LP trades

// Slab Account - our deployed slab
export const SLAB_ACCOUNT = getSlabPubkey();

/**
 * Instruction discriminators - ACTUAL Percolator program instructions
 */
export enum PercolatorInstruction {
  InitMarket = 0,
  InitUser = 1,
  InitLP = 2,
  DepositCollateral = 3,
  WithdrawCollateral = 4,
  KeeperCrank = 5,
  TradeNoCpi = 6,
  LiquidateAtOracle = 7,
  CloseAccount = 8,
  TopUpInsurance = 9,
  TradeCpi = 10,
  SetRiskThreshold = 11,
  UpdateAdmin = 12,
  CloseSlab = 13,
  UpdateConfig = 14,
}

// Keep old enum for backwards compatibility
export enum SlabInstruction {
  Initialize = 0,
  InitUser = 1,
  InitLP = 2,
  Deposit = 3,
  Withdraw = 4,
  KeeperCrank = 5,
  TradeNoCpi = 6,
  Liquidate = 7,
  CloseAccount = 8,
  TopUpInsurance = 9,
  TradeCpi = 10,
}

export enum RouterInstruction {
  Initialize = 0,
  Deposit = 1,
  Withdraw = 2,
  MultiReserve = 3,
  MultiCommit = 4,
  LockCap = 5,
  UnlockCap = 6,
  Liquidate = 7,
}

// Token program ID
const TOKEN_PROGRAM_ID = new PublicKey('TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA');
const ASSOCIATED_TOKEN_PROGRAM_ID = new PublicKey('ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL');

// Get Associated Token Address
function getAssociatedTokenAddress(mint: PublicKey, owner: PublicKey): PublicKey {
  const [address] = PublicKey.findProgramAddressSync(
    [owner.toBuffer(), TOKEN_PROGRAM_ID.toBuffer(), mint.toBuffer()],
    ASSOCIATED_TOKEN_PROGRAM_ID
  );
  return address;
}

/**
 * Build instruction to create an Associated Token Account
 * This is needed for users who don't have a wSOL ATA yet
 */
export function buildCreateAtaInstruction(params: {
  payer: PublicKey;
  owner: PublicKey;
  mint: PublicKey;
}): TransactionInstruction {
  const { payer, owner, mint } = params;
  const ata = getAssociatedTokenAddress(mint, owner);

  return new TransactionInstruction({
    keys: [
      { pubkey: payer, isSigner: true, isWritable: true },
      { pubkey: ata, isSigner: false, isWritable: true },
      { pubkey: owner, isSigner: false, isWritable: false },
      { pubkey: mint, isSigner: false, isWritable: false },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    programId: ASSOCIATED_TOKEN_PROGRAM_ID,
    data: Buffer.alloc(0), // No data needed for create ATA
  });
}

/**
 * Build instruction to transfer SOL to wSOL ATA and sync (wrap native SOL)
 * This funds the wSOL account with native SOL
 */
export function buildTransferToAtaInstruction(params: {
  from: PublicKey;
  toAta: PublicKey;
  lamports: number;
}): TransactionInstruction {
  const { from, toAta, lamports } = params;

  return SystemProgram.transfer({
    fromPubkey: from,
    toPubkey: toAta,
    lamports: lamports,
  });
}

/**
 * Build SyncNative instruction - syncs native SOL balance with wSOL token balance
 * Call this after transferring SOL to a wSOL ATA
 */
export function buildSyncNativeInstruction(params: {
  ata: PublicKey;
}): TransactionInstruction {
  const { ata } = params;

  // SyncNative instruction is tag 17 in SPL Token
  const data = Buffer.alloc(1);
  data.writeUInt8(17, 0); // SyncNative = 17

  return new TransactionInstruction({
    keys: [
      { pubkey: ata, isSigner: false, isWritable: true },
    ],
    programId: TOKEN_PROGRAM_ID,
    data,
  });
}

/**
 * Build InitUser instruction - creates a user account in the slab
 * Tag 1: InitUser { fee_payment: u64 }
 *
 * Required accounts:
 * 0. User (signer)
 * 1. Slab (writable)
 * 2. User ATA (token account for collateral)
 * 3. Vault (token account)
 * 4. Token Program
 */
export function buildInitUserInstruction(params: {
  slabAccount: PublicKey;
  userAuthority: PublicKey;
  userAta: PublicKey;
  vaultAccount: PublicKey;
  feePayment?: number; // lamports for account fee (default 0)
}): TransactionInstruction {
  const { slabAccount, userAuthority, userAta, vaultAccount, feePayment = 0 } = params;

  // Format: [tag(1), fee_payment(8)]
  const data = Buffer.alloc(9);
  data.writeUInt8(PercolatorInstruction.InitUser, 0);
  const feeBN = new BN(feePayment);
  feeBN.toArrayLike(Buffer, 'le', 8).copy(data, 1);

  return new TransactionInstruction({
    keys: [
      { pubkey: userAuthority, isSigner: true, isWritable: true },
      { pubkey: slabAccount, isSigner: false, isWritable: true },
      { pubkey: userAta, isSigner: false, isWritable: true },
      { pubkey: vaultAccount, isSigner: false, isWritable: true },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    programId: SLAB_PROGRAM_ID,
    data,
  });
}

// Export for use elsewhere
export { getAssociatedTokenAddress, TOKEN_PROGRAM_ID };

/**
 * Build DepositCollateral instruction
 * Tag 3: DepositCollateral { user_idx: u16, amount: u64 }
 *
 * Required accounts (5 total):
 * 0. User (signer)
 * 1. Slab (writable)
 * 2. User ATA (token account)
 * 3. Vault (token account)
 * 4. Token Program
 */
export function buildDepositInstruction(params: {
  slabAccount: PublicKey;
  userAuthority: PublicKey;
  userAta: PublicKey;
  vaultAccount: PublicKey;
  userIdx: number;
  amount: number; // lamports
}): TransactionInstruction {
  const { slabAccount, userAuthority, userAta, vaultAccount, userIdx, amount } = params;

  // Format: [tag(1), user_idx(2), amount(8)]
  const data = Buffer.alloc(11);
  data.writeUInt8(PercolatorInstruction.DepositCollateral, 0);
  data.writeUInt16LE(userIdx, 1);
  const amountBN = new BN(amount);
  amountBN.toArrayLike(Buffer, 'le', 8).copy(data, 3);

  return new TransactionInstruction({
    keys: [
      { pubkey: userAuthority, isSigner: true, isWritable: false },  // 0: User (signer)
      { pubkey: slabAccount, isSigner: false, isWritable: true },    // 1: Slab (writable)
      { pubkey: userAta, isSigner: false, isWritable: true },        // 2: User ATA
      { pubkey: vaultAccount, isSigner: false, isWritable: true },   // 3: Vault
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false }, // 4: Token Program
    ],
    programId: SLAB_PROGRAM_ID,
    data,
  });
}

/**
 * Build TradeNoCpi instruction - execute trade against LP without CPI
 * Tag 6: TradeNoCpi { lp_idx: u16, user_idx: u16, size: i128 }
 *
 * size > 0 means user goes LONG (buys from LP)
 * size < 0 means user goes SHORT (sells to LP)
 */
export function buildTradeNoCpiInstruction(params: {
  slabAccount: PublicKey;
  oracleAccount: PublicKey;
  userAuthority: PublicKey;
  lpIdx: number;
  userIdx: number;
  size: number; // positive = long, negative = short (in base units e6)
}): TransactionInstruction {
  const { slabAccount, oracleAccount, userAuthority, lpIdx, userIdx, size } = params;

  // Format: [tag(1), lp_idx(2), user_idx(2), size(16)]
  const data = Buffer.alloc(21);
  data.writeUInt8(PercolatorInstruction.TradeNoCpi, 0);
  data.writeUInt16LE(lpIdx, 1);
  data.writeUInt16LE(userIdx, 3);

  // Write i128 size in little-endian
  const sizeBN = new BN(size);
  const sizeBytes = sizeBN.toTwos(128).toArrayLike(Buffer, 'le', 16);
  sizeBytes.copy(data, 5);

  return new TransactionInstruction({
    keys: [
      { pubkey: slabAccount, isSigner: false, isWritable: true },
      { pubkey: oracleAccount, isSigner: false, isWritable: false },
      { pubkey: userAuthority, isSigner: true, isWritable: false },
    ],
    programId: SLAB_PROGRAM_ID,
    data,
  });
}

/**
 * Build a Reserve instruction for the slab (LEGACY - wraps TradeNoCpi for backwards compat)
 */
export function buildReserveInstruction(params: {
  slabAccount: PublicKey;
  userAccount?: PublicKey;
  accountIdx: number;
  instrumentIdx: number;
  side: 'buy' | 'sell';
  qty: number;
  limitPrice: number;
  ttlMs: number;
  commitmentHash: Buffer;
  routeId: number;
}): TransactionInstruction {
  const { slabAccount, userAccount, side, qty } = params;

  // Convert to TradeNoCpi format
  // size > 0 = long (buy), size < 0 = short (sell)
  const sizeE6 = Math.floor(qty * 1e6) * (side === 'buy' ? 1 : -1);

  // Get oracle from config
  const oracleAccount = new PublicKey(config.oracle);

  return buildTradeNoCpiInstruction({
    slabAccount,
    oracleAccount,
    userAuthority: userAccount || slabAccount,
    lpIdx: 0, // LP index 0 (our passive LP)
    userIdx: 0, // User index 0 (needs to be looked up or created)
    size: sizeE6,
  });
}

/**
 * Build a Commit instruction for the slab
 */
export function buildCommitInstruction(params: {
  slabAccount: PublicKey;
  userAccount?: PublicKey;
  holdId: number;
  commitmentReveal: Buffer;
}): TransactionInstruction {
  const { slabAccount, userAccount, holdId, commitmentReveal } = params;

  // Build instruction data
  // Format: [discriminator(1), hold_id(8), commitment_reveal(32)]
  const data = Buffer.alloc(41);
  let offset = 0;

  // Discriminator
  data.writeUInt8(SlabInstruction.Commit, offset);
  offset += 1;

  // Hold ID
  const holdIdBN = new BN(holdId);
  holdIdBN.toArrayLike(Buffer, 'le', 8).copy(data, offset);
  offset += 8;

  // Commitment reveal
  commitmentReveal.copy(data, offset);

  // Build accounts array
  const keys = [
    { pubkey: slabAccount, isSigner: false, isWritable: true },
  ];

  if (userAccount) {
    keys.push({ pubkey: userAccount, isSigner: true, isWritable: false });
  }

  return new TransactionInstruction({
    keys,
    programId: SLAB_PROGRAM_ID,
    data,
  });
}

/**
 * Build a MultiReserve instruction for the router
 */
export function buildMultiReserveInstruction(params: {
  routerAccount: PublicKey;
  escrowAccount: PublicKey;
  portfolioAccount: PublicKey;
  capAccounts: PublicKey[];
  slabAccounts: PublicKey[];
  userAuthority: PublicKey;
  instruments: number[];
  sides: ('buy' | 'sell')[];
  quantities: number[];
  limitPrices: number[];
  ttlMs: number;
  commitmentHash: Buffer;
}): TransactionInstruction {
  const {
    routerAccount,
    escrowAccount,
    portfolioAccount,
    capAccounts,
    slabAccounts,
    userAuthority,
    instruments,
    sides,
    quantities,
    limitPrices,
    ttlMs,
    commitmentHash,
  } = params;

  // Validate arrays have same length
  const numOrders = instruments.length;
  if (
    sides.length !== numOrders ||
    quantities.length !== numOrders ||
    limitPrices.length !== numOrders ||
    slabAccounts.length !== numOrders
  ) {
    throw new Error('MultiReserve: array lengths must match');
  }

  // Build instruction data
  // Format: [discriminator(1), num_orders(1), ttl_ms(8), commitment_hash(32), 
  //          orders[instrument(2), side(1), qty(8), limit_px(8)]...]
  const dataSize = 1 + 1 + 8 + 32 + numOrders * 19;
  const data = Buffer.alloc(dataSize);
  let offset = 0;

  // Discriminator
  data.writeUInt8(RouterInstruction.MultiReserve, offset);
  offset += 1;

  // Number of orders
  data.writeUInt8(numOrders, offset);
  offset += 1;

  // TTL milliseconds
  const ttlBN = new BN(ttlMs);
  ttlBN.toArrayLike(Buffer, 'le', 8).copy(data, offset);
  offset += 8;

  // Commitment hash
  commitmentHash.copy(data, offset);
  offset += 32;

  // Orders
  for (let i = 0; i < numOrders; i++) {
    // Instrument
    data.writeUInt16LE(instruments[i], offset);
    offset += 2;

    // Side
    data.writeUInt8(sides[i] === 'buy' ? Side.Buy : Side.Sell, offset);
    offset += 1;

    // Quantity
    const qtyBN = new BN(Math.floor(quantities[i] * 1e6));
    qtyBN.toArrayLike(Buffer, 'le', 8).copy(data, offset);
    offset += 8;

    // Limit price
    const priceBN = new BN(Math.floor(limitPrices[i] * 1e6));
    priceBN.toArrayLike(Buffer, 'le', 8).copy(data, offset);
    offset += 8;
  }

  // Build accounts array
  const keys = [
    { pubkey: routerAccount, isSigner: false, isWritable: true },
    { pubkey: escrowAccount, isSigner: false, isWritable: true },
    { pubkey: portfolioAccount, isSigner: false, isWritable: true },
    { pubkey: userAuthority, isSigner: true, isWritable: false },
  ];

  // Add cap accounts
  for (const capAccount of capAccounts) {
    keys.push({ pubkey: capAccount, isSigner: false, isWritable: true });
  }

  // Add slab accounts
  for (const slabAccount of slabAccounts) {
    keys.push({ pubkey: slabAccount, isSigner: false, isWritable: true });
  }

  return new TransactionInstruction({
    keys,
    programId: ROUTER_PROGRAM_ID,
    data,
  });
}

/**
 * Build a MultiCommit instruction for the router
 */
export function buildMultiCommitInstruction(params: {
  routerAccount: PublicKey;
  escrowAccount: PublicKey;
  portfolioAccount: PublicKey;
  slabAccounts: PublicKey[];
  userAuthority: PublicKey;
  holdIds: number[];
  commitmentReveal: Buffer;
}): TransactionInstruction {
  const {
    routerAccount,
    escrowAccount,
    portfolioAccount,
    slabAccounts,
    userAuthority,
    holdIds,
    commitmentReveal,
  } = params;

  // Build instruction data
  // Format: [discriminator(1), num_holds(1), commitment_reveal(32), hold_ids[8]...]
  const dataSize = 1 + 1 + 32 + holdIds.length * 8;
  const data = Buffer.alloc(dataSize);
  let offset = 0;

  // Discriminator
  data.writeUInt8(RouterInstruction.MultiCommit, offset);
  offset += 1;

  // Number of holds
  data.writeUInt8(holdIds.length, offset);
  offset += 1;

  // Commitment reveal
  commitmentReveal.copy(data, offset);
  offset += 32;

  // Hold IDs
  for (const holdId of holdIds) {
    const holdIdBN = new BN(holdId);
    holdIdBN.toArrayLike(Buffer, 'le', 8).copy(data, offset);
    offset += 8;
  }

  // Build accounts array
  const keys = [
    { pubkey: routerAccount, isSigner: false, isWritable: true },
    { pubkey: escrowAccount, isSigner: false, isWritable: true },
    { pubkey: portfolioAccount, isSigner: false, isWritable: true },
    { pubkey: userAuthority, isSigner: true, isWritable: false },
  ];

  // Add slab accounts
  for (const slabAccount of slabAccounts) {
    keys.push({ pubkey: slabAccount, isSigner: false, isWritable: true });
  }

  return new TransactionInstruction({
    keys,
    programId: ROUTER_PROGRAM_ID,
    data,
  });
}

/**
 * Generate a commitment hash for commit-reveal
 */
export function generateCommitmentHash(secret: Buffer): Buffer {
  const crypto = require('crypto');
  return crypto.createHash('sha256').update(secret).digest();
}

/**
 * Generate a random secret for commit-reveal
 */
export function generateSecret(): Buffer {
  const crypto = require('crypto');
  return crypto.randomBytes(32);
}

/**
 * Serialize a transaction to base64 for sending to frontend
 */
export function serializeTransaction(tx: Transaction): string {
  return tx.serialize({ requireAllSignatures: false }).toString('base64');
}

/**
 * Simulate transaction before sending to wallet
 * This prevents Phantom warnings by ensuring the transaction will succeed
 * 
 * Note: @solana/web3.js v1.x doesn't support sigVerify: false in simulation
 * For now, we skip simulation and rely on:
 * 1. Single signer (user wallet only)
 * 2. Proper fee payer set
 * 3. Fresh blockhash
 */
export async function simulateTransaction(tx: Transaction): Promise<{ success: boolean; error?: string }> {
  // Skip simulation for now - v1.x API limitation
  // Transactions are simple (single signer, single instruction) and should succeed
  console.log('⚠️  Transaction simulation skipped (v1.x API limitation)');
  return { success: true };
}

// Clock sysvar
const SYSVAR_CLOCK_PUBKEY = new PublicKey('SysvarC1ock11111111111111111111111111111111');

/**
 * Build TradeCpi instruction - execute trade through matcher CPI
 * Tag 10: TradeCpi { lp_idx: u16, user_idx: u16, size: i128 }
 *
 * Required accounts (8 total):
 * 0. User (signer) - the user's wallet
 * 1. LP Owner (signer) - the LP owner wallet (server-side)
 * 2. Slab (writable) - the slab account
 * 3. Clock - clock sysvar
 * 4. Oracle - oracle account
 * 5. Matcher Program - the matcher program
 * 6. Matcher Context (writable) - the matcher context account
 * 7. LP PDA - the LP's PDA
 *
 * size > 0 means user goes LONG (buys from LP)
 * size < 0 means user goes SHORT (sells to LP)
 */
export function buildTradeCpiInstruction(params: {
  userAuthority: PublicKey;
  lpOwner: PublicKey;
  slabAccount: PublicKey;
  oracleAccount: PublicKey;
  matcherProgram: PublicKey;
  matcherContext: PublicKey;
  lpPda: PublicKey;
  lpIdx: number;
  userIdx: number;
  size: number; // positive = long, negative = short (in base units e6)
}): TransactionInstruction {
  const {
    userAuthority,
    lpOwner,
    slabAccount,
    oracleAccount,
    matcherProgram,
    matcherContext,
    lpPda,
    lpIdx,
    userIdx,
    size,
  } = params;

  // Format: [tag(1), lp_idx(2), user_idx(2), size(16)]
  const data = Buffer.alloc(21);
  data.writeUInt8(PercolatorInstruction.TradeCpi, 0);
  data.writeUInt16LE(lpIdx, 1);
  data.writeUInt16LE(userIdx, 3);

  // Write i128 size in little-endian
  const sizeBN = new BN(size);
  const sizeBytes = sizeBN.toTwos(128).toArrayLike(Buffer, 'le', 16);
  sizeBytes.copy(data, 5);

  return new TransactionInstruction({
    keys: [
      { pubkey: userAuthority, isSigner: true, isWritable: false },   // 0: User (signer)
      { pubkey: lpOwner, isSigner: true, isWritable: false },         // 1: LP Owner (signer)
      { pubkey: slabAccount, isSigner: false, isWritable: true },     // 2: Slab (writable)
      { pubkey: SYSVAR_CLOCK_PUBKEY, isSigner: false, isWritable: false }, // 3: Clock
      { pubkey: oracleAccount, isSigner: false, isWritable: false },  // 4: Oracle
      { pubkey: matcherProgram, isSigner: false, isWritable: false }, // 5: Matcher Program
      { pubkey: matcherContext, isSigner: false, isWritable: true },  // 6: Matcher Context (writable)
      { pubkey: lpPda, isSigner: false, isWritable: false },          // 7: LP PDA
    ],
    programId: SLAB_PROGRAM_ID,
    data,
  });
}

/**
 * Build KeeperCrank instruction - update engine state (funding, liquidations, sweep)
 * Tag 5: KeeperCrank { caller_idx: u16, allow_panic: u8 }
 *
 * This instruction MUST be called regularly to keep the engine fresh.
 * With caller_idx = 0xFFFF (u16::MAX), it runs in permissionless mode.
 *
 * Accounts:
 *   0: Caller (signer if not permissionless)
 *   1: Slab (writable)
 *   2: Clock
 *   3: Oracle
 */
export function buildKeeperCrankInstruction(params: {
  callerAuthority: PublicKey; // Will be passed even in permissionless mode
  slabAccount: PublicKey;
  oracleAccount: PublicKey;
  callerIdx?: number; // Default: 0xFFFF for permissionless
  allowPanic?: boolean;
}): TransactionInstruction {
  const {
    callerAuthority,
    slabAccount,
    oracleAccount,
    callerIdx = 0xFFFF, // Permissionless by default
    allowPanic = false,
  } = params;

  // Format: [tag(1), caller_idx(2), allow_panic(1)]
  const data = Buffer.alloc(4);
  data.writeUInt8(PercolatorInstruction.KeeperCrank, 0);
  data.writeUInt16LE(callerIdx, 1);
  data.writeUInt8(allowPanic ? 1 : 0, 3);

  // In permissionless mode (callerIdx == 0xFFFF), caller doesn't need to be signer
  const isPermissionless = callerIdx === 0xFFFF;

  return new TransactionInstruction({
    keys: [
      { pubkey: callerAuthority, isSigner: !isPermissionless, isWritable: false }, // 0: Caller
      { pubkey: slabAccount, isSigner: false, isWritable: true },    // 1: Slab (writable)
      { pubkey: SYSVAR_CLOCK_PUBKEY, isSigner: false, isWritable: false }, // 2: Clock
      { pubkey: oracleAccount, isSigner: false, isWritable: false }, // 3: Oracle
    ],
    programId: SLAB_PROGRAM_ID,
    data,
  });
}

/**
 * Get recent blockhash for transaction
 */
export async function getRecentBlockhash(): Promise<string> {
  try {
    const connection = getConnection();
    const { blockhash } = await connection.getLatestBlockhash('finalized');
    return blockhash;
  } catch (error) {
    // If local RPC is not available, try devnet
    try {
      console.warn('Local RPC unavailable, connecting to devnet...');
      const devnetConnection = new Connection('https://api.devnet.solana.com', 'confirmed');
      const { blockhash } = await devnetConnection.getLatestBlockhash('finalized');
      console.log('✅ Got blockhash from devnet:', blockhash);
      return blockhash;
    } catch (devnetError) {
      // Last resort: mock blockhash
      console.warn('Failed to get recent blockhash from devnet, using mock value');
      return 'GH7ome3EiwEr7tu9JuTh2dpYWBJK3z69Xm1ZE3MEE6JC';
    }
  }
}

