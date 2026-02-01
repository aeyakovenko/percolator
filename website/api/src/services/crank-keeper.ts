import { Connection, PublicKey, Transaction, sendAndConfirmTransaction, Keypair } from '@solana/web3.js';
import {
  buildKeeperCrankInstruction,
  getRecentBlockhash,
} from './transactions';
import { loadDevnetConfig, getSlabPubkey } from './devnet-config';

/**
 * Automated Crank Keeper Service
 * Keeps the RiskEngine crank fresh by running cranks every 60 seconds
 * This prevents the EngineUnauthorized (0xf) error
 */

let keeperInterval: NodeJS.Timeout | null = null;
let isRunning = false;
let lastCrankTime = 0;
let crankStats = {
  totalCranks: 0,
  successfulCranks: 0,
  failedCranks: 0,
  lastError: null as string | null,
};

/**
 * Start the automated crank keeper
 * Runs 16 cranks every 60 seconds to keep the engine fresh
 */
export function startCrankKeeper(
  connection: Connection,
  slabAddress: PublicKey,
  lpKeypair: any
) {
  if (isRunning) {
    console.log('⚠️  Crank keeper already running');
    return;
  }

  console.log('🔄 Starting automated crank keeper (60s interval)...');
  isRunning = true;

  // Run immediately on start
  runCrankCycle(connection, slabAddress, lpKeypair);

  // Then run every 60 seconds
  keeperInterval = setInterval(() => {
    runCrankCycle(connection, slabAddress, lpKeypair);
  }, 60000); // 60 seconds = well within 200 slot limit

  console.log('✅ Crank keeper started');
}

/**
 * Stop the automated crank keeper
 */
export function stopCrankKeeper() {
  if (!isRunning) {
    console.log('⚠️  Crank keeper not running');
    return;
  }

  if (keeperInterval) {
    clearInterval(keeperInterval);
    keeperInterval = null;
  }

  isRunning = false;
  console.log('🛑 Crank keeper stopped');
}

/**
 * Get crank keeper status
 */
export function getCrankKeeperStatus() {
  return {
    isRunning,
    lastCrankTime,
    secondsSinceLastCrank: lastCrankTime ? Math.floor((Date.now() - lastCrankTime) / 1000) : null,
    stats: crankStats,
  };
}

/**
 * Execute a single crank instruction
 */
async function executeCrank(
  connection: Connection,
  slabAddress: PublicKey,
  lpKeypair: Keypair
): Promise<string> {
  // Load config for oracle address
  const config = loadDevnetConfig();
  const oraclePubkey = new PublicKey(config.oracle);

  // Build KeeperCrank instruction in permissionless mode (callerIdx = 0xFFFF)
  const crankIx = buildKeeperCrankInstruction({
    callerAuthority: lpKeypair.publicKey,
    slabAccount: slabAddress,
    oracleAccount: oraclePubkey,
    callerIdx: 0xFFFF, // Permissionless mode
    allowPanic: 0, // Don't allow panic
  });

  // Create transaction
  const transaction = new Transaction().add(crankIx);
  const { blockhash, lastValidBlockHeight } = await getRecentBlockhash(connection);
  transaction.recentBlockhash = blockhash;
  transaction.lastValidBlockHeight = lastValidBlockHeight;
  transaction.feePayer = lpKeypair.publicKey;

  // Sign and send
  const signature = await sendAndConfirmTransaction(
    connection,
    transaction,
    [lpKeypair],
    {
      commitment: 'confirmed',
      skipPreflight: false,
    }
  );

  return signature;
}

/**
 * Run a full crank cycle (16 cranks)
 */
async function runCrankCycle(
  connection: Connection,
  slabAddress: PublicKey,
  lpKeypair: Keypair
) {
  const startTime = Date.now();
  console.log(`🔄 [Keeper] Running crank cycle at ${new Date().toISOString()}`);

  try {
    const results = [];

    // Run 16 cranks for a full sweep
    for (let i = 0; i < 16; i++) {
      try {
        const signature = await executeCrank(connection, slabAddress, lpKeypair);
        results.push({ success: true, signature });
        crankStats.successfulCranks++;
      } catch (error: any) {
        console.error(`❌ [Keeper] Crank ${i + 1} failed:`, error.message);
        results.push({ success: false, error: error.message });
        crankStats.failedCranks++;
        crankStats.lastError = error.message;
      }
    }

    crankStats.totalCranks += 16;
    lastCrankTime = Date.now();

    const successCount = results.filter(r => r.success).length;
    const duration = Date.now() - startTime;

    console.log(`✅ [Keeper] Crank cycle complete: ${successCount}/16 successful (${duration}ms)`);

  } catch (error: any) {
    console.error('❌ [Keeper] Crank cycle failed:', error.message);
    crankStats.lastError = error.message;
  }
}

/**
 * Reset crank keeper stats
 */
export function resetCrankKeeperStats() {
  crankStats = {
    totalCranks: 0,
    successfulCranks: 0,
    failedCranks: 0,
    lastError: null,
  };
  console.log('📊 Crank keeper stats reset');
}
