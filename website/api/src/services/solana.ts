import { Connection, PublicKey, Keypair } from '@solana/web3.js';
import { AnchorProvider, Program, Wallet } from '@coral-xyz/anchor';
import fs from 'fs';
import { startCrankKeeper } from './crank-keeper';

let connection: Connection;
let provider: AnchorProvider;
let slabProgram: Program | null = null;
let wallet: Wallet;
let lpKeypair: Keypair | null = null;

export async function initializeSolana() {
  // Use devnet by default
  const rpcUrl = process.env.SOLANA_RPC_URL || 'https://api.devnet.solana.com';
  console.log('✅ Solana RPC URL:', rpcUrl);
  connection = new Connection(rpcUrl, 'confirmed');

  // Load wallet (for signing transactions if needed)
  const walletPath = process.env.WALLET_PATH || `${process.env.HOME}/.config/solana/id.json`;
  let keypair: Keypair;
  
  try {
    const walletData = JSON.parse(fs.readFileSync(walletPath, 'utf-8'));
    keypair = Keypair.fromSecretKey(new Uint8Array(walletData));
    console.log('Loaded wallet from', walletPath);
  } catch (error) {
    console.warn('No wallet found, using dummy keypair (read-only mode)');
    keypair = Keypair.generate();
  }

  wallet = new Wallet(keypair);
  provider = new AnchorProvider(connection, wallet, {
    commitment: 'confirmed',
    preflightCommitment: 'confirmed',
  });

  // Test connection
  try {
    const version = await connection.getVersion();
    console.log('Connected to Solana cluster version:', version['solana-core']);
  } catch (error) {
    console.warn('Solana RPC not available, API will work with mock data only');
  }

  // TODO: Load slab program IDL and initialize Program
  // const slabProgramId = new PublicKey(process.env.SLAB_PROGRAM_ID!);
  // slabProgram = new Program(IDL, slabProgramId, provider);

  // Load LP keypair for crank operations
  try {
    const path = require('path');

    // Get LP keypair path - support both absolute and relative paths
    let lpKeypairPath: string;
    if (process.env.LP_KEYPAIR_PATH) {
      // Check if it's an absolute path
      if (path.isAbsolute(process.env.LP_KEYPAIR_PATH)) {
        lpKeypairPath = process.env.LP_KEYPAIR_PATH;
      } else {
        // Relative path from project root
        const projectRoot = path.resolve(__dirname, '../../../../');
        lpKeypairPath = path.join(projectRoot, process.env.LP_KEYPAIR_PATH);
      }
    } else {
      // Default: devnet-deployer.json in project root
      const projectRoot = path.resolve(__dirname, '../../../../');
      lpKeypairPath = path.join(projectRoot, 'devnet-deployer.json');
    }

    console.log('🔍 Looking for LP keypair at:', lpKeypairPath);

    if (!fs.existsSync(lpKeypairPath)) {
      throw new Error(`LP keypair not found at ${lpKeypairPath}`);
    }

    const lpKeypairData = JSON.parse(fs.readFileSync(lpKeypairPath, 'utf-8'));
    lpKeypair = Keypair.fromSecretKey(new Uint8Array(lpKeypairData));
    console.log('✅ Loaded LP keypair:', lpKeypair.publicKey.toBase58());

    // Start automated crank keeper if slab address is configured
    const slabAddress = process.env.NEXT_PUBLIC_SLAB_ACCOUNT;
    if (slabAddress) {
      console.log('🔄 Starting automated crank keeper...');
      startCrankKeeper(connection, new PublicKey(slabAddress), lpKeypair);
    } else {
      console.warn('⚠️  NEXT_PUBLIC_SLAB_ACCOUNT not set, crank keeper disabled');
    }
  } catch (error: any) {
    console.warn('⚠️  LP keypair not found, crank keeper disabled:', error.message);
  }
}

export function getConnection(): Connection {
  if (!connection) throw new Error('Solana connection not initialized');
  return connection;
}

export function getProvider(): AnchorProvider {
  if (!provider) throw new Error('Anchor provider not initialized');
  return provider;
}

export function getSlabProgram(): Program {
  if (!slabProgram) throw new Error('Slab program not initialized');
  return slabProgram;
}

export function getWallet(): Wallet {
  if (!wallet) throw new Error('Wallet not initialized');
  return wallet;
}

export function getLpKeypair(): Keypair {
  if (!lpKeypair) throw new Error('LP keypair not initialized');
  return lpKeypair;
}

// Helper to fetch slab state
export async function fetchSlabState(slabAddress: PublicKey): Promise<any> {
  const accountInfo = await connection.getAccountInfo(slabAddress);
  if (!accountInfo) throw new Error('Slab account not found');
  
  // TODO: Parse SlabState from account data
  // For now, return raw data
  return {
    data: accountInfo.data,
    owner: accountInfo.owner.toBase58(),
    lamports: accountInfo.lamports,
  };
}

