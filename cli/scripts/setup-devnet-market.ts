/**
 * Setup a persistent devnet inverted market for testing
 *
 * DISCLAIMER: This code is for EDUCATIONAL PURPOSES ONLY.
 * The percolator program has NOT been audited. Do NOT use in production
 * or with real funds. Use at your own risk.
 *
 * This script creates:
 * - An inverted SOL/USD market using Chainlink's live oracle
 * - A funded insurance fund (100 tokens)
 * - A 50bps passive matcher LP (10 tokens collateral)
 *
 * Usage:
 *   npx tsx scripts/setup-devnet-market.ts
 *
 * The market info is saved to devnet-market.json for reference.
 */

import {
  Connection,
  Keypair,
  PublicKey,
  Transaction,
  sendAndConfirmTransaction,
  ComputeBudgetProgram,
  SystemProgram,
  SYSVAR_CLOCK_PUBKEY,
  SYSVAR_RENT_PUBKEY,
  LAMPORTS_PER_SOL,
} from "@solana/web3.js";
import {
  getOrCreateAssociatedTokenAccount,
  TOKEN_PROGRAM_ID,
  NATIVE_MINT,
} from "@solana/spl-token";
import * as fs from "fs";
import {
  encodeInitMarket,
  encodeInitLP,
  encodeDepositCollateral,
  encodeTopUpInsurance,
  encodeKeeperCrank,
} from "../src/abi/instructions.js";
import {
  ACCOUNTS_INIT_MARKET,
  ACCOUNTS_INIT_LP,
  ACCOUNTS_DEPOSIT_COLLATERAL,
  ACCOUNTS_TOPUP_INSURANCE,
  ACCOUNTS_KEEPER_CRANK,
  buildAccountMetas,
} from "../src/abi/accounts.js";
import { deriveVaultAuthority, deriveLpPda } from "../src/solana/pda.js";
import { parseHeader, parseConfig, parseEngine, parseUsedIndices } from "../src/solana/slab.js";
import { buildIx } from "../src/runtime/tx.js";

// ============================================================================
// CONSTANTS
// ============================================================================

// Chainlink SOL/USD on devnet (actively updated!)
const CHAINLINK_SOL_USD = new PublicKey("99B2bTijsU6f1GCT73HmdR7HCFFjGMBcPZY6jZ96ynrR");

// Program IDs - Our deployed programs
const PROGRAM_ID = new PublicKey("FjawyjMgQcuRZHaSvnxyB6N6pKD7waNTxXGSfFBQinzP");
const MATCHER_PROGRAM_ID = new PublicKey("DcNN5mqdeGq7ZWnyNXT9j3ZkGcamyirGYVNnnJQjUfcR");
const MATCHER_CTX_SIZE = 320;

// Market parameters
// SLAB_SIZE = HEADER_LEN + CONFIG_LEN (aligned) + ENGINE_LEN
// Updated to match percolator-prog SLAB_LEN constant
const SLAB_SIZE = 1111384;

// Funding amounts (in lamports with 9 decimals for wrapped SOL)
const INSURANCE_FUND_AMOUNT = 1_000_000_000n;  // 1 SOL
const LP_COLLATERAL_AMOUNT = 1_000_000_000n;   // 1 SOL

// ============================================================================
// HELPERS
// ============================================================================

async function getChainlinkPrice(connection: Connection): Promise<{ price: number; timestamp: number }> {
  const info = await connection.getAccountInfo(CHAINLINK_SOL_USD);
  if (!info) throw new Error("Chainlink oracle not found");

  const data = info.data;
  const decimals = data.readUInt8(138);
  const timestamp = Number(data.readBigUInt64LE(208));
  const answer = data.readBigInt64LE(216);

  const price = Number(answer) / Math.pow(10, decimals);
  return { price, timestamp };
}

// ============================================================================
// MAIN
// ============================================================================

async function main() {
  console.log("\n" + "=".repeat(70));
  console.log("PERCOLATOR DEVNET MARKET SETUP");
  console.log("=".repeat(70));
  console.log("\n*** DISCLAIMER: FOR EDUCATIONAL PURPOSES ONLY ***");
  console.log("*** This code has NOT been audited. Do NOT use with real funds. ***\n");
  console.log("This script creates a persistent inverted SOL/USD market on devnet.");
  console.log("Market uses Chainlink's live SOL/USD oracle for price feeds.\n");

  // Setup connection and wallet
  const walletPath = process.env.WALLET_PATH || "../devnet-deployer-array.json";
  const payer = Keypair.fromSecretKey(
    new Uint8Array(JSON.parse(fs.readFileSync(walletPath, "utf-8")))
  );
  const connection = new Connection("https://api.devnet.solana.com", "confirmed");

  console.log(`Wallet: ${payer.publicKey.toBase58()}`);
  const balance = await connection.getBalance(payer.publicKey);
  console.log(`Balance: ${(balance / LAMPORTS_PER_SOL).toFixed(4)} SOL\n`);

  if (balance < 10 * LAMPORTS_PER_SOL) {
    console.log("WARNING: Low balance. Consider running: solana airdrop 5");
  }

  // Check Chainlink oracle
  console.log("Step 1: Verifying Chainlink oracle...");
  const { price, timestamp } = await getChainlinkPrice(connection);
  const age = (Date.now() / 1000) - timestamp;
  console.log(`  Oracle: ${CHAINLINK_SOL_USD.toBase58()}`);
  console.log(`  Current price: $${price.toFixed(2)}`);
  console.log(`  Age: ${age.toFixed(0)} seconds`);

  if (age > 3600) {
    console.log("  WARNING: Oracle is stale (> 1 hour old)");
  } else {
    console.log("  Oracle is FRESH");
  }

  // Use wrapped SOL as collateral
  console.log("\nStep 2: Using wrapped SOL as collateral...");
  const mint = NATIVE_MINT;
  console.log(`  Mint: ${mint.toBase58()} (Wrapped SOL)`);

  // Create slab account
  console.log("\nStep 3: Creating slab account...");
  const slab = Keypair.generate();
  console.log(`  Slab: ${slab.publicKey.toBase58()}`);

  const rentExempt = await connection.getMinimumBalanceForRentExemption(SLAB_SIZE);
  console.log(`  Rent: ${(rentExempt / LAMPORTS_PER_SOL).toFixed(4)} SOL`);

  const createSlabTx = new Transaction();
  createSlabTx.add(ComputeBudgetProgram.setComputeUnitLimit({ units: 100000 }));
  createSlabTx.add(SystemProgram.createAccount({
    fromPubkey: payer.publicKey,
    newAccountPubkey: slab.publicKey,
    lamports: rentExempt,
    space: SLAB_SIZE,
    programId: PROGRAM_ID,
  }));
  await sendAndConfirmTransaction(connection, createSlabTx, [payer, slab], { commitment: "confirmed" });

  // Derive vault PDA
  const [vaultPda, vaultBump] = deriveVaultAuthority(PROGRAM_ID, slab.publicKey);
  console.log(`  Vault PDA: ${vaultPda.toBase58()}`);

  // Create vault ATA
  const vaultAccount = await getOrCreateAssociatedTokenAccount(
    connection, payer, mint, vaultPda, true
  );
  const vault = vaultAccount.address;
  console.log(`  Vault ATA: ${vault.toBase58()}`);

  // Initialize market (INVERTED)
  console.log("\nStep 4: Initializing INVERTED market...");
  const feedId = Buffer.from(CHAINLINK_SOL_USD.toBytes()).toString("hex");

  const initMarketData = encodeInitMarket({
    admin: payer.publicKey,
    collateralMint: mint,
    indexFeedId: feedId,
    maxStalenessSecs: "3600",        // 1 hour staleness
    confFilterBps: 500,              // 5% confidence filter
    invert: 1,                       // INVERTED market
    unitScale: 0,
    warmupPeriodSlots: "10",
    maintenanceMarginBps: "500",     // 5% maintenance margin
    initialMarginBps: "1000",        // 10% initial margin
    tradingFeeBps: "10",             // 0.1% trading fee
    maxAccounts: "1024",             // Allow many accounts
    newAccountFee: "1000000",        // 0.001 SOL to create account
    riskReductionThreshold: "0",
    maintenanceFeePerSlot: "0",
    maxCrankStalenessSlots: "200",
    liquidationFeeBps: "100",        // 1% liquidation fee
    liquidationFeeCap: "1000000000", // 1000 token cap
    liquidationBufferBps: "50",      // 0.5% buffer
    minLiquidationAbs: "100000",     // 0.1 token minimum
  });

  const initMarketKeys = buildAccountMetas(ACCOUNTS_INIT_MARKET, [
    payer.publicKey,
    slab.publicKey,
    mint,
    vault,
    TOKEN_PROGRAM_ID,
    SYSVAR_CLOCK_PUBKEY,
    SYSVAR_RENT_PUBKEY,
    vaultPda,
    SystemProgram.programId,
  ]);

  const initTx = new Transaction();
  initTx.add(ComputeBudgetProgram.setComputeUnitLimit({ units: 200000 }));
  initTx.add(buildIx({ programId: PROGRAM_ID, keys: initMarketKeys, data: initMarketData }));
  await sendAndConfirmTransaction(connection, initTx, [payer], { commitment: "confirmed" });
  console.log("  Market initialized (inverted=true)");

  // Run initial keeper crank
  console.log("\nStep 5: Running initial keeper crank...");
  const crankData = encodeKeeperCrank({ callerIdx: 65535, allowPanic: false });
  const crankKeys = buildAccountMetas(ACCOUNTS_KEEPER_CRANK, [
    payer.publicKey,
    slab.publicKey,
    SYSVAR_CLOCK_PUBKEY,
    CHAINLINK_SOL_USD,
  ]);

  const crankTx = new Transaction();
  crankTx.add(ComputeBudgetProgram.setComputeUnitLimit({ units: 400000 }));
  crankTx.add(buildIx({ programId: PROGRAM_ID, keys: crankKeys, data: crankData }));
  await sendAndConfirmTransaction(connection, crankTx, [payer], { commitment: "confirmed", skipPreflight: true });
  console.log("  Keeper crank executed");

  // Create admin wrapped SOL account and fund it
  console.log("\nStep 6: Creating admin wrapped SOL account...");
  const adminAta = await getOrCreateAssociatedTokenAccount(
    connection, payer, mint, payer.publicKey
  );
  // Fund admin ATA with wrapped SOL for LP collateral + insurance + fees
  const wrapAmount = 5 * LAMPORTS_PER_SOL;  // 5 SOL
  const wrapTx = new Transaction();
  wrapTx.add(ComputeBudgetProgram.setComputeUnitLimit({ units: 50000 }));
  wrapTx.add(SystemProgram.transfer({
    fromPubkey: payer.publicKey,
    toPubkey: adminAta.address,
    lamports: wrapAmount,
  }));
  wrapTx.add({
    programId: TOKEN_PROGRAM_ID,
    keys: [{ pubkey: adminAta.address, isSigner: false, isWritable: true }],
    data: Buffer.from([17]),  // SyncNative instruction
  });
  await sendAndConfirmTransaction(connection, wrapTx, [payer], { commitment: "confirmed" });
  console.log(`  Wrapped ${wrapAmount / LAMPORTS_PER_SOL} SOL to admin ATA`);

  // Create LP with 50bps matcher
  console.log("\nStep 7: Creating LP with 50bps passive matcher...");

  // Get current used indices
  const slabInfo = await connection.getAccountInfo(slab.publicKey);
  const usedIndices = slabInfo ? parseUsedIndices(slabInfo.data) : [];
  const lpIndex = usedIndices.length;

  // Create matcher context account
  const matcherCtxKp = Keypair.generate();
  const matcherRent = await connection.getMinimumBalanceForRentExemption(MATCHER_CTX_SIZE);

  const createMatcherTx = new Transaction();
  createMatcherTx.add(ComputeBudgetProgram.setComputeUnitLimit({ units: 50000 }));
  createMatcherTx.add(SystemProgram.createAccount({
    fromPubkey: payer.publicKey,
    newAccountPubkey: matcherCtxKp.publicKey,
    lamports: matcherRent,
    space: MATCHER_CTX_SIZE,
    programId: MATCHER_PROGRAM_ID,
  }));
  await sendAndConfirmTransaction(connection, createMatcherTx, [payer, matcherCtxKp], { commitment: "confirmed" });
  console.log(`  Matcher context: ${matcherCtxKp.publicKey.toBase58()}`);

  // Derive LP PDA
  const [lpPda] = deriveLpPda(PROGRAM_ID, slab.publicKey, lpIndex);
  console.log(`  LP PDA: ${lpPda.toBase58()}`);

  // Initialize matcher context
  const initMatcherTx = new Transaction();
  initMatcherTx.add(ComputeBudgetProgram.setComputeUnitLimit({ units: 50000 }));
  initMatcherTx.add({
    programId: MATCHER_PROGRAM_ID,
    keys: [
      { pubkey: lpPda, isSigner: false, isWritable: false },
      { pubkey: matcherCtxKp.publicKey, isSigner: false, isWritable: true },
    ],
    data: Buffer.from([1]),  // Init instruction
  });
  await sendAndConfirmTransaction(connection, initMatcherTx, [payer], { commitment: "confirmed" });
  console.log("  Matcher context initialized");

  // Initialize LP account
  const initLpData = encodeInitLP({
    matcherProgram: MATCHER_PROGRAM_ID,
    matcherContext: matcherCtxKp.publicKey,
    feePayment: "2000000",  // 0.002 SOL
  });
  const initLpKeys = buildAccountMetas(ACCOUNTS_INIT_LP, [
    payer.publicKey,
    slab.publicKey,
    adminAta.address,
    vault,
    TOKEN_PROGRAM_ID,
  ]);

  const initLpTx = new Transaction();
  initLpTx.add(ComputeBudgetProgram.setComputeUnitLimit({ units: 50000 }));
  initLpTx.add(buildIx({ programId: PROGRAM_ID, keys: initLpKeys, data: initLpData }));
  await sendAndConfirmTransaction(connection, initLpTx, [payer], { commitment: "confirmed" });
  console.log(`  LP initialized at index ${lpIndex}`);

  // Deposit collateral to LP
  console.log("\nStep 8: Depositing collateral to LP...");
  const depositData = encodeDepositCollateral({ userIdx: lpIndex, amount: LP_COLLATERAL_AMOUNT.toString() });
  const depositKeys = buildAccountMetas(ACCOUNTS_DEPOSIT_COLLATERAL, [
    payer.publicKey,
    slab.publicKey,
    adminAta.address,
    vault,
    TOKEN_PROGRAM_ID,
  ]);

  const depositTx = new Transaction();
  depositTx.add(ComputeBudgetProgram.setComputeUnitLimit({ units: 50000 }));
  depositTx.add(buildIx({ programId: PROGRAM_ID, keys: depositKeys, data: depositData }));
  await sendAndConfirmTransaction(connection, depositTx, [payer], { commitment: "confirmed" });
  console.log(`  Deposited ${Number(LP_COLLATERAL_AMOUNT) / 1e9} SOL to LP`);

  // Top up insurance fund
  console.log("\nStep 9: Topping up insurance fund...");
  const topupData = encodeTopUpInsurance({ amount: INSURANCE_FUND_AMOUNT.toString() });
  const topupKeys = buildAccountMetas(ACCOUNTS_TOPUP_INSURANCE, [
    payer.publicKey,
    slab.publicKey,
    adminAta.address,
    vault,
    TOKEN_PROGRAM_ID,
  ]);

  const topupTx = new Transaction();
  topupTx.add(ComputeBudgetProgram.setComputeUnitLimit({ units: 50000 }));
  topupTx.add(buildIx({ programId: PROGRAM_ID, keys: topupKeys, data: topupData }));
  await sendAndConfirmTransaction(connection, topupTx, [payer], { commitment: "confirmed" });
  console.log(`  Insurance fund topped up with ${Number(INSURANCE_FUND_AMOUNT) / 1e9} SOL`);

  // Verify final state
  console.log("\nStep 10: Verifying market state...");
  const finalSlabInfo = await connection.getAccountInfo(slab.publicKey);
  if (finalSlabInfo) {
    const header = parseHeader(finalSlabInfo.data);
    const config = parseConfig(finalSlabInfo.data);
    const engine = parseEngine(finalSlabInfo.data);

    console.log(`  Version: ${header.version}`);
    console.log(`  Admin: ${header.admin.toBase58()}`);
    console.log(`  Inverted: ${config.invert === 1 ? "Yes" : "No"}`);
    console.log(`  Insurance fund: ${Number(engine.insuranceFund.balance) / 1e9} SOL`);
    console.log(`  Risk reduction mode: ${engine.riskReductionOnly ? "Yes" : "No"}`);
  }

  // Save market info
  const marketInfo = {
    network: "devnet",
    createdAt: new Date().toISOString(),
    programId: PROGRAM_ID.toBase58(),
    matcherProgramId: MATCHER_PROGRAM_ID.toBase58(),
    slab: slab.publicKey.toBase58(),
    mint: mint.toBase58(),
    vault: vault.toBase58(),
    vaultPda: vaultPda.toBase58(),
    oracle: CHAINLINK_SOL_USD.toBase58(),
    oracleType: "chainlink",
    inverted: true,
    lp: {
      index: lpIndex,
      pda: lpPda.toBase58(),
      matcherContext: matcherCtxKp.publicKey.toBase58(),
      collateral: Number(LP_COLLATERAL_AMOUNT) / 1e9,
    },
    insuranceFund: Number(INSURANCE_FUND_AMOUNT) / 1e9,
    admin: payer.publicKey.toBase58(),
    adminAta: adminAta.address.toBase58(),
  };

  fs.writeFileSync("devnet-market.json", JSON.stringify(marketInfo, null, 2));
  console.log("\nMarket info saved to devnet-market.json");

  // Print summary
  console.log("\n" + "=".repeat(70));
  console.log("MARKET SETUP COMPLETE!");
  console.log("=".repeat(70));
  console.log(`
Market Details:
  Slab:           ${slab.publicKey.toBase58()}
  Mint:           ${mint.toBase58()}
  Vault:          ${vault.toBase58()}
  Oracle:         ${CHAINLINK_SOL_USD.toBase58()} (Chainlink SOL/USD)
  Type:           INVERTED (price = 1/SOL in USD terms)

LP (50bps Passive Matcher):
  Index:          ${lpIndex}
  PDA:            ${lpPda.toBase58()}
  Matcher Ctx:    ${matcherCtxKp.publicKey.toBase58()}
  Collateral:     ${Number(LP_COLLATERAL_AMOUNT) / 1e9} SOL

Insurance Fund:   ${Number(INSURANCE_FUND_AMOUNT) / 1e9} SOL

Admin:            ${payer.publicKey.toBase58()}
Admin ATA:        ${adminAta.address.toBase58()}
`);

  console.log("To trade against this market, see the README for examples.");
  console.log("=".repeat(70) + "\n");
}

main().catch(console.error);
