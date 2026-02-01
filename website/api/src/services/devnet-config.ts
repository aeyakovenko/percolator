import { PublicKey } from '@solana/web3.js';
import fs from 'fs';
import path from 'path';

export interface LpConfig {
  index: number;
  pda: string;
  matcherContext: string;
  collateral: number;
}

export interface DevnetConfig {
  network: string;
  rpcUrl: string;
  programId: string;
  matcherProgramId: string;
  slab: string;
  vault: string;
  vaultPda: string;
  oracle: string;
  oracleType: 'pyth' | 'chainlink';
  inverted: boolean;
  pair: string;
  maxLeverage: number;
  initialMarginBps: number;
  maintenanceMarginBps: number;
  mint: string;
  lp: LpConfig;
  insuranceFund: number;
  admin: string;
  adminAta: string;
}

let cachedConfig: DevnetConfig | null = null;

export function loadDevnetConfig(): DevnetConfig {
  if (cachedConfig) return cachedConfig;

  // Try multiple paths to find devnet-config.json
  const possiblePaths = [
    path.join(process.cwd(), 'devnet-config.json'),
    path.join(process.cwd(), '..', 'devnet-config.json'),
    path.join(process.cwd(), '..', '..', 'devnet-config.json'),
    path.join(__dirname, '..', '..', '..', '..', 'devnet-config.json'),
  ];

  let configPath: string | null = null;
  for (const p of possiblePaths) {
    if (fs.existsSync(p)) {
      configPath = p;
      break;
    }
  }

  if (!configPath) {
    console.warn('⚠️  devnet-config.json not found, using fallback config');
    // Fallback to environment variables or defaults
    cachedConfig = {
      network: process.env.SOLANA_NETWORK || 'devnet',
      rpcUrl: process.env.SOLANA_RPC_URL || 'https://api.devnet.solana.com',
      programId: process.env.PROGRAM_ID || 'FjawyjMgQcuRZHaSvnxyB6N6pKD7waNTxXGSfFBQinzP',
      matcherProgramId: process.env.MATCHER_PROGRAM_ID || 'DcNN5mqdeGq7ZWnyNXT9j3ZkGcamyirGYVNnnJQjUfcR',
      slab: process.env.SLAB_ACCOUNT || '7RkyJSarWThR5sZDJihZfV22rS3mbg538nHtcaTnkrec',
      vault: '55hLrPTEc6XMSkmgQU7gyzCRv2ZoErmWaaDeRKBHcoQB',
      vaultPda: '4wD4Lw9HwhGtcYdsHjU88C924wbcyVwwP9pzKEzpWWbz',
      oracle: '99B2bTijsU6f1GCT73HmdR7HCFFjGMBcPZY6jZ96ynrR',
      oracleType: 'chainlink',
      inverted: true,
      pair: 'SOL-PERP',
      maxLeverage: 10,
      initialMarginBps: 1000,
      maintenanceMarginBps: 500,
      mint: 'So11111111111111111111111111111111111111112',
      lp: {
        index: 0,
        pda: '5Jbcoky4qz9rvdoGHK4s9KcFrLM536syjm56GvFziEDw',
        matcherContext: '4bN3B3Epf1odSh6FmeGGwX7sXqHS7xur2tknYjRjt4pk',
        collateral: 1,
      },
      insuranceFund: 1,
      admin: '6yzQ3Vryi8f9Xhp3ik3a77CbuNXAB1LrdoeCupFVs8aw',
      adminAta: 'CPrGoPF1N5r53cTgPzPQ3thoucUPBupqkzQapY5dPbff',
    };
    return cachedConfig;
  }

  try {
    const configData = fs.readFileSync(configPath, 'utf-8');
    cachedConfig = JSON.parse(configData) as DevnetConfig;
    console.log(`✅ Loaded devnet config from ${configPath}`);
    console.log(`   Program ID: ${cachedConfig.programId}`);
    console.log(`   Slab: ${cachedConfig.slab}`);
    console.log(`   Pair: ${cachedConfig.pair}`);
    return cachedConfig;
  } catch (error) {
    console.error('❌ Failed to load devnet-config.json:', error);
    throw error;
  }
}

// Helper to get PublicKey instances
export function getSlabPubkey(): PublicKey {
  return new PublicKey(loadDevnetConfig().slab);
}

export function getProgramId(): PublicKey {
  return new PublicKey(loadDevnetConfig().programId);
}

export function getMatcherProgramId(): PublicKey {
  return new PublicKey(loadDevnetConfig().matcherProgramId);
}

export function getVaultPubkey(): PublicKey {
  return new PublicKey(loadDevnetConfig().vault);
}

export function getOraclePubkey(): PublicKey {
  return new PublicKey(loadDevnetConfig().oracle);
}

export function getMintPubkey(): PublicKey {
  return new PublicKey(loadDevnetConfig().mint);
}

export function getRpcUrl(): string {
  return loadDevnetConfig().rpcUrl;
}

// Market info helper
export function getMarketInfo() {
  const config = loadDevnetConfig();
  return {
    pair: config.pair,
    maxLeverage: config.maxLeverage,
    initialMarginBps: config.initialMarginBps,
    maintenanceMarginBps: config.maintenanceMarginBps,
    oracleType: config.oracleType,
    inverted: config.inverted,
  };
}
