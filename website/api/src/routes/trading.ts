import { Router } from 'express';
import { PublicKey, Transaction, TransactionInstruction, SystemProgram, LAMPORTS_PER_SOL, Keypair } from '@solana/web3.js';
import * as fs from 'fs';
import * as path from 'path';
import {
  buildInitUserInstruction,
  buildDepositInstruction,
  buildWithdrawInstruction,
  buildTradeNoCpiInstruction,
  buildTradeCpiInstruction,
  buildCreateAtaInstruction,
  buildTransferToAtaInstruction,
  buildSyncNativeInstruction,
  buildKeeperCrankInstruction,
  serializeTransaction,
  simulateTransaction,
  getRecentBlockhash,
  getAssociatedTokenAddress,
  SLAB_PROGRAM_ID,
  SLAB_ACCOUNT,
} from '../services/transactions';
import { logTransaction } from './monitor';
import {
  fetchSlabRaw,
  parseAllAccounts,
  parseConfig,
  parseEngine,
  findAccountByOwner,
} from '../services/slab-parser';
import { getConnection } from '../services/solana';
import { loadDevnetConfig, getSlabPubkey } from '../services/devnet-config';
import { getCrankKeeperStatus, stopCrankKeeper, startCrankKeeper, resetCrankKeeperStats } from '../services/crank-keeper';

// Load LP owner keypair for server-side signing
let lpOwnerKeypair: Keypair | null = null;
try {
  // Use the same method as devnet-config to find the project root
  const projectRoot = path.resolve(__dirname, '../../../../');
  const keypairPath = path.join(projectRoot, 'devnet-deployer.json');

  if (fs.existsSync(keypairPath)) {
    const keypairContent = fs.readFileSync(keypairPath, 'utf8').trim();

    // Check if it's JSON array format or base58 format
    if (keypairContent.startsWith('[')) {
      // JSON array format: [1,2,3,...]
      const keypairData = JSON.parse(keypairContent);
      lpOwnerKeypair = Keypair.fromSecretKey(Uint8Array.from(keypairData));
    } else {
      // Base58 format (Solana CLI default)
      const bs58 = require('bs58');
      const secretKey = bs58.decode(keypairContent);
      lpOwnerKeypair = Keypair.fromSecretKey(secretKey);
    }
    console.log(`✅ Loaded LP owner keypair: ${lpOwnerKeypair.publicKey.toBase58()}`);
  } else {
    console.warn(`⚠️ LP owner keypair not found at ${keypairPath}`);
    console.warn('   TradeCpi will not work without LP keypair');
  }
} catch (error) {
  console.error('❌ Failed to load LP owner keypair:', error);
}

export const tradingRouter = Router();

// Load config
const config = loadDevnetConfig();

// Store active reservations (in production, use Redis or database)
const activeReservations = new Map<string, {
  holdId: number;
  secret: Buffer;
  vwapPrice: number;
  worstPrice: number;
  maxCharge: number;
  filledQty: number;
  expiryMs: number;
}>();

// Store completed trades for orderbook display
export const completedTrades: Array<{
  timestamp: number;
  user: string;
  side: 'buy' | 'sell';
  price: number;
  quantity: number;
  signature?: string;
}> = [];

// Store active orders (reservations that haven't been committed yet)
export const activeOrders: Array<{
  holdId: number;
  user: string;
  side: 'buy' | 'sell';
  price: number;
  quantity: number;
  timestamp: number;
  expiryMs: number;
}> = [];

/**
 * POST /api/trade/order
 * Simplified order placement (hides reserve-commit complexity)
 * User just wants to buy or sell - we handle the two-phase execution
 */
tradingRouter.post('/order', async (req, res) => {
  try {
    const {
      user,
      side, // 'buy' or 'sell'
      price,
      quantity,
      orderType = 'limit',
      instrument = 0,
    } = req.body;

    if (!user || !quantity) {
      return res.status(400).json({ error: 'Missing required fields: user, quantity' });
    }

    if (orderType === 'limit' && !price) {
      return res.status(400).json({ error: 'Price required for limit orders' });
    }

    // For now, just log the order (mock mode)
    // In production, this would call reserve-commit automatically
    const holdId = Math.floor(Math.random() * 1000000);
    
    console.log(`📝 Order placed: ${side} ${quantity} @ ${price || 'market'}`);
    
    // Log to monitor
    const { logTransaction } = await import('./monitor');
    logTransaction({
      id: Date.now(),
      type: 'trade',
      timestamp: Date.now(),
      user: user.substring(0, 6) + '...' + user.substring(user.length - 4),
      action: `${side === 'buy' ? 'Bought' : 'Sold'} ${quantity} @ ${price || 'market'}`,
      amount: price ? parseFloat(quantity) * parseFloat(price) : 0,
      data: {
        side,
        qty: quantity,
        price,
        instrument,
      },
      signature: `Order${holdId}`,
    });

    res.json({
      success: true,
      orderId: holdId,
      side,
      quantity,
      price,
      status: 'placed',
      message: `${side === 'buy' ? 'Buy' : 'Sell'} order placed successfully!`,
      note: 'Order is now in the book (mock mode - will be real after deployment)'
    });

  } catch (error: any) {
    console.error('Order error:', error);
    res.status(500).json({ error: error.message });
  }
});

/**
 * POST /api/trade/reserve
 * Execute a trade using TradeNoCpi instruction
 * If user doesn't have an account, creates one first with InitUser
 * Returns an unsigned transaction for the user to sign
 */
tradingRouter.post('/reserve', async (req, res) => {
  try {
    const {
      user,
      price,
      quantity,
      side = 'buy',
    } = req.body;

    if (!user || !price || !quantity) {
      return res.status(400).json({ error: 'Missing required fields: user, price, quantity' });
    }

    // Parse user public key
    let userPubkey: PublicKey;
    try {
      userPubkey = new PublicKey(user);
    } catch (error) {
      return res.status(400).json({ error: 'Invalid user public key' });
    }

    const slabPubkey = getSlabPubkey();
    const oracleAccount = new PublicKey(config.oracle);
    const connection = getConnection();

    // Check if user has an account in the slab
    let userIdx: number = -1;
    let needsInitUser = true;

    try {
      const slabData = await fetchSlabRaw(connection, slabPubkey);
      const accounts = parseAllAccounts(slabData);
      const found = findAccountByOwner(accounts, userPubkey.toBase58());

      if (found) {
        userIdx = found.idx;
        needsInitUser = false;
        console.log(`✅ Found user account at index ${userIdx}`);
      } else {
        console.log(`ℹ️ User has no account, will create with InitUser`);
      }
    } catch (error) {
      console.warn('⚠️ Could not fetch slab state, assuming new user:', error);
    }

    // Build transaction
    const transaction = new Transaction();

    // If user needs account, add InitUser instruction first
    if (needsInitUser) {
      console.log('📝 Adding InitUser instruction');

      // Get the mint and vault from config
      const mint = new PublicKey(config.mint);
      const vaultAccount = new PublicKey(config.vault);

      // Get user's ATA for the collateral mint (wSOL)
      const userAta = getAssociatedTokenAddress(mint, userPubkey);
      console.log(`   User ATA: ${userAta.toBase58()}`);
      console.log(`   Vault: ${vaultAccount.toBase58()}`);

      // Check if user's ATA exists - if not, we need to create it first
      let ataExists = false;
      try {
        const ataInfo = await connection.getAccountInfo(userAta);
        ataExists = ataInfo !== null && ataInfo.data.length > 0;
        console.log(`   ATA exists: ${ataExists}`);
      } catch (err) {
        console.log('   Could not check ATA, assuming it does not exist');
      }

      // InitUser requires a minimum deposit (in lamports) to create the account
      // This is the collateral that will be deposited into the vault
      // Minimum of 0.01 SOL (10_000_000 lamports) to start trading
      const minimumDeposit = 10_000_000; // 0.01 SOL in lamports

      // If ATA doesn't exist, add instruction to create it
      if (!ataExists) {
        console.log('📝 Adding CreateATA instruction (wSOL account needed)');
        const createAtaIx = buildCreateAtaInstruction({
          payer: userPubkey,
          owner: userPubkey,
          mint: mint,
        });
        transaction.add(createAtaIx);
      }

      // For wSOL: Transfer native SOL to the ATA, then sync to wrap it
      // This wraps native SOL into wSOL tokens that InitUser can transfer
      console.log(`📝 Adding wrap SOL instructions (${minimumDeposit / 1e9} SOL)`);

      // 1. Transfer SOL to ATA
      const transferIx = buildTransferToAtaInstruction({
        from: userPubkey,
        toAta: userAta,
        lamports: minimumDeposit,
      });
      transaction.add(transferIx);

      // 2. Sync native - converts the SOL balance to wSOL token balance
      const syncIx = buildSyncNativeInstruction({
        ata: userAta,
      });
      transaction.add(syncIx);

      // 3. Now InitUser can transfer the wSOL from ATA to vault
      const initUserIx = buildInitUserInstruction({
        slabAccount: slabPubkey,
        userAuthority: userPubkey,
        userAta: userAta,
        vaultAccount: vaultAccount,
        feePayment: minimumDeposit, // Deposit 0.01 SOL as initial collateral
      });
      transaction.add(initUserIx);

      // After InitUser, the user will get the next available index
      // For now, we can't know the exact index until the tx is confirmed
      // So we return an "init only" transaction first
      const blockhash = await getRecentBlockhash();
      transaction.recentBlockhash = blockhash;
      transaction.feePayer = userPubkey;

      const serializedTx = serializeTransaction(transaction);

      return res.json({
        success: true,
        transaction: serializedTx,
        needsSigning: true,
        step: 'init_user',
        message: ataExists
          ? 'Sign this transaction to create your trading account. After confirmation, place your trade again.'
          : 'Sign this transaction to create your wSOL token account and trading account. After confirmation, place your trade again.',
      });
    }

    // User has an account, build TradeCpi instruction with server-side LP signing
    // size > 0 = long (buy), size < 0 = short (sell)
    const sizeE6 = Math.floor(quantity * 1e6) * (side === 'buy' ? 1 : -1);

    // Check if we have LP keypair for TradeCpi
    if (!lpOwnerKeypair) {
      return res.status(500).json({
        error: 'Server not configured for trading',
        message: 'LP owner keypair not loaded. Please ensure devnet-deployer.json exists.',
      });
    }

    // Get LP and matcher info from config
    const matcherProgram = new PublicKey(config.matcherProgramId);
    const matcherContext = new PublicKey(config.lp.matcherContext);
    const lpPda = new PublicKey(config.lp.pda);
    const lpIdx = config.lp.index;

    console.log(`📝 Building TradeCpi: userIdx=${userIdx}, lpIdx=${lpIdx}, size=${sizeE6} (${side})`);
    console.log(`   LP Owner: ${lpOwnerKeypair.publicKey.toBase58()}`);
    console.log(`   Matcher Program: ${matcherProgram.toBase58()}`);
    console.log(`   Matcher Context: ${matcherContext.toBase58()}`);
    console.log(`   LP PDA: ${lpPda.toBase58()}`);

    const tradeIx = buildTradeCpiInstruction({
      userAuthority: userPubkey,
      lpOwner: lpOwnerKeypair.publicKey,
      slabAccount: slabPubkey,
      oracleAccount: oracleAccount,
      matcherProgram: matcherProgram,
      matcherContext: matcherContext,
      lpPda: lpPda,
      lpIdx: lpIdx,
      userIdx: userIdx,
      size: sizeE6,
    });
    transaction.add(tradeIx);

    // Get fresh blockhash
    const blockhash = await getRecentBlockhash();
    transaction.recentBlockhash = blockhash;
    transaction.feePayer = userPubkey;

    // Server partially signs with LP keypair
    // User will complete signing with their wallet
    transaction.partialSign(lpOwnerKeypair);
    console.log(`✅ Server signed transaction with LP owner keypair`);

    // Log transaction details for debugging
    console.log(`📦 Transaction details:`);
    console.log(`   Instructions: ${transaction.instructions.length}`);
    console.log(`   Signatures required: ${transaction.signatures.length}`);
    console.log(`   Fee payer: ${transaction.feePayer?.toBase58()}`);

    // Serialize transaction for frontend
    const serializedTx = serializeTransaction(transaction);

    // Generate hold ID for tracking
    const holdId = Math.floor(Math.random() * 1000000);

    // Store reservation details for commit step
    const vwapPrice = price;
    const expiryMs = Date.now() + 60000;

    const reservationKey = `${user}-${holdId}`;
    activeReservations.set(reservationKey, {
      holdId,
      secret: Buffer.alloc(32), // Not used for TradeNoCpi
      vwapPrice,
      worstPrice: price,
      maxCharge: quantity * price,
      filledQty: quantity,
      expiryMs,
    });

    // Add to active orders
    activeOrders.push({
      holdId,
      user,
      side: side as 'buy' | 'sell',
      price,
      quantity,
      timestamp: Date.now(),
      expiryMs,
    });

    // Log to monitor
    logTransaction({
      id: Date.now(),
      type: 'trade',
      timestamp: Date.now(),
      user: user.substring(0, 6) + '...' + user.substring(user.length - 4),
      data: {
        side,
        qty: quantity,
        price,
        userIdx,
        sizeE6,
      },
      signature: `Trade${holdId}`,
    });

    res.json({
      success: true,
      transaction: serializedTx,
      needsSigning: true,
      holdId,
      vwapPrice,
      expiryMs,
      userIdx,
      step: 'trade',
      message: 'Sign this transaction to execute your trade',
    });
  } catch (error: any) {
    console.error('Trade error:', error);
    res.status(500).json({ error: error.message });
  }
});

/**
 * POST /api/trade/commit
 * Confirm a trade was executed (TradeNoCpi is single-step, no commit needed)
 * This endpoint now just records the trade as completed
 */
tradingRouter.post('/commit', async (req, res) => {
  try {
    const { user, holdId, signature } = req.body;

    if (!user || !holdId) {
      return res.status(400).json({ error: 'Missing required fields: user, holdId' });
    }

    // Get reservation details
    const reservationKey = `${user}-${holdId}`;
    const reservation = activeReservations.get(reservationKey);

    if (!reservation) {
      // For TradeNoCpi, the trade already executed in the reserve step
      // Just acknowledge success
      return res.json({
        success: true,
        message: 'Trade already executed',
        holdId,
      });
    }

    // Log the completed trade
    logTransaction({
      id: Date.now(),
      type: 'trade',
      timestamp: Date.now(),
      user: user.substring(0, 6) + '...' + user.substring(user.length - 4),
      action: `Traded ${reservation.filledQty} @ ${reservation.vwapPrice}`,
      amount: reservation.filledQty * reservation.vwapPrice,
      signature: signature || `Trade${reservation.holdId}`,
    });

    // Find and remove from active orders
    const orderIndex = activeOrders.findIndex(o => o.holdId === reservation.holdId);
    let orderData = null;
    if (orderIndex > -1) {
      orderData = activeOrders[orderIndex];
      activeOrders.splice(orderIndex, 1);
    }

    // Add to completed trades
    if (orderData) {
      completedTrades.push({
        timestamp: Date.now(),
        user,
        side: orderData.side,
        price: orderData.price,
        quantity: orderData.quantity,
        signature,
      });
    }

    // Clean up reservation
    activeReservations.delete(reservationKey);

    res.json({
      success: true,
      holdId: reservation.holdId,
      vwapPrice: reservation.vwapPrice,
      filledQty: reservation.filledQty,
      totalCost: reservation.filledQty * reservation.vwapPrice,
      message: 'Trade executed successfully!',
      orderData,
    });
  } catch (error: any) {
    console.error('Commit error:', error);
    res.status(500).json({ error: error.message });
  }
});

/**
 * POST /api/trade/multi-reserve
 * Multi-asset trading not yet supported with TradeNoCpi
 */
tradingRouter.post('/multi-reserve', async (req, res) => {
  res.status(501).json({
    error: 'Multi-asset trading not yet implemented',
    message: 'Use /reserve for single asset trades',
  });
});

/**
 * POST /api/trade/cancel
 * Cancel a reservation before it expires
 */
tradingRouter.post('/cancel', async (req, res) => {
  try {
    const { user, holdId } = req.body;

    if (!user || !holdId) {
      return res.status(400).json({ error: 'Missing required fields' });
    }

    const reservationKey = `${user}-${holdId}`;
    const existed = activeReservations.delete(reservationKey);

    res.json({
      success: existed,
      message: existed ? 'Reservation cancelled' : 'Reservation not found',
    });
  } catch (error: any) {
    res.status(500).json({ error: error.message });
  }
});

/**
 * GET /api/trade/reservations/:user
 * Get active reservations for a user
 */
tradingRouter.get('/reservations/:user', async (req, res) => {
  try {
    const { user } = req.params;
    const userReservations: any[] = [];

    activeReservations.forEach((reservation, key) => {
      if (key.startsWith(user)) {
        userReservations.push({
          holdId: reservation.holdId,
          vwapPrice: reservation.vwapPrice,
          worstPrice: reservation.worstPrice,
          maxCharge: reservation.maxCharge,
          filledQty: reservation.filledQty,
          expiryMs: reservation.expiryMs,
          timeRemaining: Math.max(0, reservation.expiryMs - Date.now()),
        });
      }
    });

    res.json({
      success: true,
      reservations: userReservations,
    });
  } catch (error: any) {
    res.status(500).json({ error: error.message });
  }
});

/**
 * POST /api/trade/record-fill
 * Record a successful trade (called by frontend after commit confirmation)
 */
tradingRouter.post('/record-fill', async (req, res) => {
  try {
    const { user, side, price, quantity, signature } = req.body;

    if (!user || !side || !price || !quantity) {
      return res.status(400).json({ error: 'Missing required fields' });
    }

    // Add to completed trades
    completedTrades.push({
      timestamp: Date.now(),
      user,
      side,
      price,
      quantity,
      signature,
    });

    // Keep only last 100 trades
    if (completedTrades.length > 100) {
      completedTrades.shift();
    }

    res.json({ success: true, message: 'Trade recorded' });
  } catch (error: any) {
    res.status(500).json({ error: error.message });
  }
});

/**
 * POST /api/trade/deposit
 * Deposit additional collateral to an existing trading account
 * User must already have an account (use /reserve to create one first)
 */
tradingRouter.post('/deposit', async (req, res) => {
  try {
    const { user, amount } = req.body;

    if (!user || !amount) {
      return res.status(400).json({ error: 'Missing required fields: user, amount (in SOL)' });
    }

    const userPubkey = new PublicKey(user);
    const slabPubkey = getSlabPubkey();
    const connection = getConnection();

    // Find user's account index
    let userIdx: number = -1;
    try {
      const slabData = await fetchSlabRaw(connection, slabPubkey);
      const accounts = parseAllAccounts(slabData);
      const found = findAccountByOwner(accounts, userPubkey.toBase58());

      if (found) {
        userIdx = found.idx;
        console.log(`✅ Found user account at index ${userIdx} for deposit`);
      }
    } catch (error) {
      console.warn('⚠️ Could not fetch slab state:', error);
    }

    if (userIdx === -1) {
      return res.status(400).json({
        error: 'No trading account found',
        message: 'You need to create a trading account first. Place a small trade to create one.',
      });
    }

    // Build deposit transaction
    const transaction = new Transaction();
    const mint = new PublicKey(config.mint);
    const vaultAccount = new PublicKey(config.vault);
    const userAta = getAssociatedTokenAddress(mint, userPubkey);

    // Convert SOL to lamports
    const lamports = Math.floor(amount * LAMPORTS_PER_SOL);

    // Transfer SOL to ATA
    const transferIx = buildTransferToAtaInstruction({
      from: userPubkey,
      toAta: userAta,
      lamports: lamports,
    });
    transaction.add(transferIx);

    // Sync native - wrap SOL to wSOL
    const syncIx = buildSyncNativeInstruction({ ata: userAta });
    transaction.add(syncIx);

    // Deposit to percolator
    const depositIx = buildDepositInstruction({
      slabAccount: slabPubkey,
      userAuthority: userPubkey,
      userAta: userAta,
      vaultAccount: vaultAccount,
      userIdx: userIdx,
      amount: lamports,
    });
    transaction.add(depositIx);

    // Set blockhash and fee payer
    const blockhash = await getRecentBlockhash();
    transaction.recentBlockhash = blockhash;
    transaction.feePayer = userPubkey;

    const serializedTx = serializeTransaction(transaction);

    console.log(`📝 Built deposit tx: ${amount} SOL for user ${userIdx}`);

    res.json({
      success: true,
      transaction: serializedTx,
      needsSigning: true,
      amount: amount,
      lamports: lamports,
      userIdx: userIdx,
      message: `Sign to deposit ${amount} SOL to your trading account`,
    });
  } catch (error: any) {
    console.error('Deposit error:', error);
    res.status(500).json({ error: error.message });
  }
});

/**
 * POST /api/trade/withdraw
 * Withdraw collateral from trading account back to wallet
 * User must have an account and sufficient available balance
 */
tradingRouter.post('/withdraw', async (req, res) => {
  try {
    const { user, amount } = req.body;

    if (!user || !amount) {
      return res.status(400).json({ error: 'Missing required fields: user, amount (in SOL)' });
    }

    const userPubkey = new PublicKey(user);
    const slabPubkey = getSlabPubkey();
    const connection = getConnection();

    // Find user's account index
    let userIdx: number = -1;
    let userCapital: number = 0;
    let userPosition: bigint = 0n;
    try {
      const slabData = await fetchSlabRaw(connection, slabPubkey);
      const accounts = parseAllAccounts(slabData);
      const found = findAccountByOwner(accounts, userPubkey.toBase58());

      if (found) {
        userIdx = found.idx;
        userCapital = Number(found.account.capital) / 1e9;
        userPosition = found.account.positionSize;
        console.log(`✅ Found user account at index ${userIdx} for withdraw`);
        console.log(`   Capital: ${userCapital} SOL, Position: ${userPosition}`);
      }
    } catch (error) {
      console.warn('⚠️ Could not fetch slab state:', error);
    }

    if (userIdx === -1) {
      return res.status(400).json({
        error: 'No trading account found',
        message: 'You need to create a trading account first.',
      });
    }

    // Check if user has open position
    if (userPosition !== 0n) {
      return res.status(400).json({
        error: 'Cannot withdraw with open position',
        message: 'Close your position first before withdrawing collateral.',
      });
    }

    // Check if user has sufficient balance
    if (amount > userCapital) {
      return res.status(400).json({
        error: 'Insufficient balance',
        message: `You only have ${userCapital.toFixed(4)} SOL available to withdraw.`,
        available: userCapital,
      });
    }

    // Build withdraw transaction
    const transaction = new Transaction();
    const mint = new PublicKey(config.mint);
    const vaultAccount = new PublicKey(config.vault);
    const userAta = getAssociatedTokenAddress(mint, userPubkey);

    // Derive vault authority PDA
    const [vaultAuthority] = PublicKey.findProgramAddressSync(
      [slabPubkey.toBuffer()],
      new PublicKey(config.programId)
    );

    // Convert SOL to lamports
    const lamports = Math.floor(amount * LAMPORTS_PER_SOL);

    // Build withdraw instruction
    const withdrawIx = buildWithdrawInstruction({
      slabAccount: slabPubkey,
      userAuthority: userPubkey,
      userAta: userAta,
      vaultAccount: vaultAccount,
      vaultAuthority: vaultAuthority,
      userIdx: userIdx,
      amount: lamports,
    });
    transaction.add(withdrawIx);

    // Add close wSOL account instruction to get native SOL back
    // CloseAccount instruction - closes the ATA and sends lamports to owner
    const closeAtaIx = new TransactionInstruction({
      keys: [
        { pubkey: userAta, isSigner: false, isWritable: true },
        { pubkey: userPubkey, isSigner: false, isWritable: true },
        { pubkey: userPubkey, isSigner: true, isWritable: false },
      ],
      programId: new PublicKey('TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA'),
      data: Buffer.from([9]), // CloseAccount = 9
    });
    transaction.add(closeAtaIx);

    // Set blockhash and fee payer
    const blockhash = await getRecentBlockhash();
    transaction.recentBlockhash = blockhash;
    transaction.feePayer = userPubkey;

    const serializedTx = serializeTransaction(transaction);

    console.log(`📝 Built withdraw tx: ${amount} SOL for user ${userIdx}`);

    res.json({
      success: true,
      transaction: serializedTx,
      needsSigning: true,
      amount: amount,
      lamports: lamports,
      userIdx: userIdx,
      message: `Sign to withdraw ${amount} SOL from your trading account`,
    });
  } catch (error: any) {
    console.error('Withdraw error:', error);
    res.status(500).json({ error: error.message });
  }
});

/**
 * POST /api/trade/test-transfer
 * Test endpoint - builds a simple SOL transfer transaction
 * This will ALWAYS succeed (no program deployment needed)
 */
tradingRouter.post('/test-transfer', async (req, res) => {
  try {
    const { user } = req.body;

    if (!user) {
      return res.status(400).json({ error: 'Missing user wallet address' });
    }

    const userPubkey = new PublicKey(user);

    // Create a simple transfer instruction (0.001 SOL to yourself)
    const transferInstruction = SystemProgram.transfer({
      fromPubkey: userPubkey,
      toPubkey: userPubkey, // Send to yourself
      lamports: 0.001 * LAMPORTS_PER_SOL, // 0.001 SOL
    });

    // Build transaction
    const transaction = new Transaction().add(transferInstruction);
    transaction.feePayer = userPubkey;
    transaction.recentBlockhash = await getRecentBlockhash();

    // Serialize and return
    const serializedTx = serializeTransaction(transaction);

    res.json({
      success: true,
      needsSigning: true,
      transaction: serializedTx,
      message: 'Simple test transaction - transfers 0.001 SOL to yourself',
      testMode: true,
    });
  } catch (error: any) {
    console.error('Error in /api/trade/test-transfer:', error);
    res.status(500).json({ error: error.message });
  }
});

/**
 * GET /api/trade/debug-accounts
 * Debug endpoint - shows all accounts in the slab with their balances
 */
tradingRouter.get('/debug-accounts', async (req, res) => {
  try {
    const slabPubkey = getSlabPubkey();
    const connection = getConnection();

    const slabData = await fetchSlabRaw(connection, slabPubkey);
    const accounts = parseAllAccounts(slabData);
    const slabConfig = parseConfig(slabData);

    const formatted = accounts.map(({ idx, account }) => ({
      idx,
      owner: account.owner,
      kind: account.kind === 0 ? 'User' : 'LP',
      capital: Number(account.capital) / 1e9, // Convert lamports to SOL
      capitalRaw: account.capital.toString(),
      positionSize: Number(account.positionSize) / 1e6, // e6 precision
      positionSizeRaw: account.positionSize.toString(),
      entryPrice: Number(account.entryPrice) / 1e6,
      pnl: Number(account.pnl) / 1e9,
      accountId: account.accountId.toString(),
      warmupStartedAtSlot: account.warmupStartedAtSlot.toString(),
      warmupSlopePerStep: account.warmupSlopePerStep.toString(),
      fundingIndex: account.fundingIndex.toString(),
    }));

    console.log(`📊 Debug accounts:`, JSON.stringify(formatted, null, 2));

    res.json({
      success: true,
      numAccounts: accounts.length,
      accounts: formatted,
      slabConfig: {
        invert: slabConfig.invert,
        unitScale: slabConfig.unitScale,
        confFilterBps: slabConfig.confFilterBps,
        indexFeedId: slabConfig.indexFeedId,
      },
    });
  } catch (error: any) {
    console.error('Debug accounts error:', error);
    res.status(500).json({ error: error.message });
  }
});

/**
 * GET /api/trade/debug-margin
 * Debug endpoint - simulates margin calculation for a given trade
 */
tradingRouter.get('/debug-margin', async (req, res) => {
  try {
    const { userIdx = '1', size = '1' } = req.query;
    const sizeE6 = Math.floor(parseFloat(size as string) * 1e6);

    const slabPubkey = getSlabPubkey();
    const oraclePubkey = new PublicKey(config.oracle);
    const connection = getConnection();

    // Fetch slab state
    const slabData = await fetchSlabRaw(connection, slabPubkey);
    const accounts = parseAllAccounts(slabData);
    const slabConfig = parseConfig(slabData);

    // Fetch oracle price from Chainlink
    const oracleInfo = await connection.getAccountInfo(oraclePubkey);
    if (!oracleInfo) {
      return res.status(500).json({ error: 'Oracle account not found' });
    }

    // Parse Chainlink OCR2 price
    // Chainlink OCR2 layout from percolator.rs:
    //   CL_OFF_DECIMALS: 138 (u8)
    //   CL_OFF_TIMESTAMP: 208 (u64)
    //   CL_OFF_ANSWER: 216 (i128)
    const oracleData = Buffer.from(oracleInfo.data);
    let rawAnswer: bigint;
    let decimals: number;
    let timestamp: bigint;

    const CL_MIN_LEN = 224;
    const CL_OFF_DECIMALS = 138;
    const CL_OFF_TIMESTAMP = 208;
    const CL_OFF_ANSWER = 216;

    if (oracleData.length >= CL_MIN_LEN) {
      decimals = oracleData.readUInt8(CL_OFF_DECIMALS);
      timestamp = oracleData.readBigUInt64LE(CL_OFF_TIMESTAMP);
      // Read i128 as two 64-bit values
      const lo = oracleData.readBigUInt64LE(CL_OFF_ANSWER);
      const hi = oracleData.readBigInt64LE(CL_OFF_ANSWER + 8);
      rawAnswer = (hi << 64n) | lo;
    } else {
      rawAnswer = BigInt(0);
      decimals = 8;
      timestamp = BigInt(0);
    }

    // Convert to e6 format: price_e6 = answer * 10^(6-decimals)
    const scale = 6 - decimals;
    let rawPriceE6: number;
    if (scale >= 0) {
      rawPriceE6 = Number(rawAnswer) * Math.pow(10, scale);
    } else {
      rawPriceE6 = Number(rawAnswer) / Math.pow(10, -scale);
    }

    // Apply inversion if configured: 1e12 / raw
    let marketPriceE6 = rawPriceE6;
    if (slabConfig.invert && rawPriceE6 > 0) {
      marketPriceE6 = 1e12 / rawPriceE6;
    }

    // Get user and LP account
    const userAccount = accounts.find(a => a.idx === parseInt(userIdx as string));
    const lpAccount = accounts.find(a => a.account.kind === 1); // LP account

    if (!userAccount) {
      return res.status(400).json({ error: `User account ${userIdx} not found` });
    }

    // Simulate margin calculation (matching Rust logic from percolator.rs:3406-3414)
    const userCapital = Number(userAccount.account.capital);
    const userPnl = Number(userAccount.account.pnl);
    const userEquity = userCapital + userPnl;
    const newPositionSize = sizeE6;

    // position_value = |position| * oracle_price / 1e6
    const positionValue = Math.abs(newPositionSize) * marketPriceE6 / 1e6;

    // margin_required = position_value * maintenance_margin_bps / 10000
    const maintenanceMarginBps = Number(config.maintenanceMarginBps || 500);
    const marginRequired = positionValue * maintenanceMarginBps / 10000;

    // Same for LP
    const lpCapital = lpAccount ? Number(lpAccount.account.capital) : 0;
    const lpPnl = lpAccount ? Number(lpAccount.account.pnl) : 0;
    const lpEquity = lpCapital + lpPnl;
    const lpPositionValue = Math.abs(-sizeE6) * marketPriceE6 / 1e6; // LP takes opposite side
    const lpMarginRequired = lpPositionValue * maintenanceMarginBps / 10000;

    const result = {
      success: true,
      oracle: {
        address: config.oracle,
        rawAnswer: rawAnswer.toString(),
        decimals,
        rawPriceE6: rawPriceE6.toFixed(0),
        rawPriceUSD: (rawPriceE6 / 1e6).toFixed(2),
        invert: slabConfig.invert,
        marketPriceE6: marketPriceE6.toFixed(0),
        marketPriceUSD: (marketPriceE6 / 1e6).toFixed(6),
      },
      trade: {
        sizeInput: size,
        sizeE6,
        side: 'buy',
      },
      user: {
        idx: parseInt(userIdx as string),
        capitalLamports: userCapital,
        capitalSOL: userCapital / 1e9,
        pnlLamports: userPnl,
        equityLamports: userEquity,
        equitySOL: userEquity / 1e9,
        newPositionE6: newPositionSize,
        positionValueLamports: positionValue,
        marginRequiredLamports: marginRequired,
        marginRequiredSOL: marginRequired / 1e9,
        willPass: userEquity > marginRequired,
      },
      lp: lpAccount ? {
        idx: lpAccount.idx,
        capitalLamports: lpCapital,
        capitalSOL: lpCapital / 1e9,
        pnlLamports: lpPnl,
        equityLamports: lpEquity,
        equitySOL: lpEquity / 1e9,
        newPositionE6: -sizeE6,
        positionValueLamports: lpPositionValue,
        marginRequiredLamports: lpMarginRequired,
        marginRequiredSOL: lpMarginRequired / 1e9,
        willPass: lpEquity > lpMarginRequired,
      } : null,
      config: {
        maintenanceMarginBps,
        initialMarginBps: config.initialMarginBps,
        inverted: slabConfig.invert,
      },
    };

    console.log('🧮 Debug margin calculation:', JSON.stringify(result, null, 2));
    res.json(result);
  } catch (error: any) {
    console.error('Debug margin error:', error);
    res.status(500).json({ error: error.message });
  }
});

/**
 * GET /api/trade/debug-crank
 * Debug endpoint - shows crank status and whether trades can execute
 */
tradingRouter.get('/debug-crank', async (req, res) => {
  try {
    const slabPubkey = getSlabPubkey();
    const connection = getConnection();

    // Fetch slab state
    const slabData = await fetchSlabRaw(connection, slabPubkey);
    const engine = parseEngine(slabData);

    // Get current slot from Solana
    const currentSlot = await connection.getSlot();

    // Calculate crank staleness
    const lastCrankSlot = Number(engine.lastCrankSlot);
    const maxCrankStalenessSlots = Number(engine.maxCrankStalenessSlots);
    const crankAge = currentSlot - lastCrankSlot;
    const crankIsFresh = crankAge <= maxCrankStalenessSlots;

    // Check sweep status
    const lastSweepStartSlot = Number(engine.lastSweepStartSlot);
    const lastSweepCompleteSlot = Number(engine.lastSweepCompleteSlot);
    const sweepAge = currentSlot - lastSweepStartSlot;
    const sweepIsFresh = sweepAge <= maxCrankStalenessSlots;

    const result = {
      success: true,
      currentSlot,
      crank: {
        lastCrankSlot,
        maxCrankStalenessSlots,
        crankAge,
        crankIsFresh,
        message: crankIsFresh ? 'Crank is fresh - trades allowed' : `Crank is stale (${crankAge} slots old) - needs cranking before trades`,
      },
      sweep: {
        lastSweepStartSlot,
        lastSweepCompleteSlot,
        sweepAge,
        sweepIsFresh,
        message: sweepIsFresh ? 'Sweep is fresh - risk-increasing trades allowed' : `Sweep is stale (${sweepAge} slots old) - needs cranking`,
      },
      canTrade: crankIsFresh && sweepIsFresh,
      recommendation: (!crankIsFresh || !sweepIsFresh)
        ? 'Call the Crank instruction to update the engine state before trading'
        : 'System is ready for trading',
    };

    console.log('⏰ Debug crank status:', JSON.stringify(result, null, 2));
    res.json(result);
  } catch (error: any) {
    console.error('Debug crank error:', error);
    res.status(500).json({ error: error.message });
  }
});

/**
 * GET /api/trade/debug-bytes
 * Debug endpoint - shows raw bytes at key offsets to verify parsing
 */
tradingRouter.get('/debug-bytes', async (req, res) => {
  try {
    const slabPubkey = getSlabPubkey();
    const connection = getConnection();

    // Fetch slab state
    const slabData = await fetchSlabRaw(connection, slabPubkey);

    // Key offsets from slab-parser.ts (empirically verified against devnet 2026-01)
    const ENGINE_OFF = 328;
    const ENGINE_LAST_CRANK_SLOT_OFF = 280;
    const ENGINE_MAX_CRANK_STALENESS_OFF = 288;
    // Sweep fields are after large ADL arrays (empirically verified at engine offset 86412+)
    const ENGINE_LAST_SWEEP_START_OFF = 86416;
    const ENGINE_LAST_SWEEP_COMPLETE_OFF = 86424;
    const ENGINE_CRANK_STEP_OFF = 86432;

    // Read raw bytes at each offset
    const base = ENGINE_OFF;
    const lastCrankSlotBytes = slabData.subarray(base + ENGINE_LAST_CRANK_SLOT_OFF, base + ENGINE_LAST_CRANK_SLOT_OFF + 8);
    const maxCrankStalenessBytes = slabData.subarray(base + ENGINE_MAX_CRANK_STALENESS_OFF, base + ENGINE_MAX_CRANK_STALENESS_OFF + 8);
    const lastSweepStartBytes = slabData.subarray(base + ENGINE_LAST_SWEEP_START_OFF, base + ENGINE_LAST_SWEEP_START_OFF + 8);
    const lastSweepCompleteBytes = slabData.subarray(base + ENGINE_LAST_SWEEP_COMPLETE_OFF, base + ENGINE_LAST_SWEEP_COMPLETE_OFF + 8);
    const crankStepByte = slabData.readUInt8(base + ENGINE_CRANK_STEP_OFF);

    // Parse as u64 little-endian
    const lastCrankSlot = slabData.readBigUInt64LE(base + ENGINE_LAST_CRANK_SLOT_OFF);
    const maxCrankStaleness = slabData.readBigUInt64LE(base + ENGINE_MAX_CRANK_STALENESS_OFF);
    const lastSweepStart = slabData.readBigUInt64LE(base + ENGINE_LAST_SWEEP_START_OFF);
    const lastSweepComplete = slabData.readBigUInt64LE(base + ENGINE_LAST_SWEEP_COMPLETE_OFF);

    // Also try parsing the raw hex at offset 360 to see what's there
    const rawBytesAround360 = slabData.subarray(base + 350, base + 390).toString('hex');

    // Also check the sweep area around 82320
    const rawBytesAroundSweep = slabData.subarray(base + 82310, base + 82350).toString('hex');

    res.json({
      success: true,
      offsets: {
        ENGINE_OFF: base,
        lastCrankSlotOffset: base + ENGINE_LAST_CRANK_SLOT_OFF,
        maxCrankStalenessOffset: base + ENGINE_MAX_CRANK_STALENESS_OFF,
        lastSweepStartOffset: base + ENGINE_LAST_SWEEP_START_OFF,
        lastSweepCompleteOffset: base + ENGINE_LAST_SWEEP_COMPLETE_OFF,
        crankStepOffset: base + ENGINE_CRANK_STEP_OFF,
      },
      rawBytes: {
        lastCrankSlot: lastCrankSlotBytes.toString('hex'),
        maxCrankStaleness: maxCrankStalenessBytes.toString('hex'),
        lastSweepStart: lastSweepStartBytes.toString('hex'),
        lastSweepComplete: lastSweepCompleteBytes.toString('hex'),
        crankStep: crankStepByte,
      },
      parsed: {
        lastCrankSlot: lastCrankSlot.toString(),
        maxCrankStaleness: maxCrankStaleness.toString(),
        lastSweepStart: lastSweepStart.toString(),
        lastSweepComplete: lastSweepComplete.toString(),
        crankStep: crankStepByte,
      },
      rawHexAround360: rawBytesAround360,
      rawHexAroundSweep: rawBytesAroundSweep,
    });
  } catch (error: any) {
    console.error('Debug bytes error:', error);
    res.status(500).json({ error: error.message });
  }
});

/**
 * POST /api/trade/admin/crank
 * Admin endpoint - execute KeeperCrank to update engine state
 * Uses server-side LP keypair to sign (permissionless mode)
 *
 * This MUST be called before any trades can be executed if the crank is stale.
 * The engine uses 16 steps for a full sweep, so this may need to be called
 * multiple times to complete a sweep. Pass count=16 to run a full sweep.
 */
tradingRouter.post('/admin/crank', async (req, res) => {
  try {
    if (!lpOwnerKeypair) {
      return res.status(500).json({ error: 'LP owner keypair not loaded' });
    }

    const { count = 1 } = req.body;
    const numCranks = Math.min(Math.max(1, parseInt(count) || 1), 16);

    const slabPubkey = getSlabPubkey();
    const oraclePubkey = new PublicKey(config.oracle);
    const connection = getConnection();

    const signatures: string[] = [];
    let lastError: string | null = null;

    console.log(`⏰ Executing ${numCranks} crank(s)...`);

    for (let i = 0; i < numCranks; i++) {
      try {
        // Build KeeperCrank instruction in permissionless mode (callerIdx = 0xFFFF)
        const crankIx = buildKeeperCrankInstruction({
          callerAuthority: lpOwnerKeypair.publicKey,
          slabAccount: slabPubkey,
          oracleAccount: oraclePubkey,
          callerIdx: 0xFFFF, // Permissionless
          allowPanic: false,
        });

        const transaction = new Transaction();
        transaction.add(crankIx);

        // Set blockhash and fee payer
        const blockhash = await getRecentBlockhash();
        transaction.recentBlockhash = blockhash;
        transaction.feePayer = lpOwnerKeypair.publicKey;

        // Sign with LP keypair (pays for gas)
        transaction.sign(lpOwnerKeypair);

        const signature = await connection.sendRawTransaction(transaction.serialize());
        await connection.confirmTransaction(signature, 'confirmed');
        signatures.push(signature);
        console.log(`✅ Crank ${i + 1}/${numCranks} confirmed: ${signature}`);
      } catch (err: any) {
        lastError = err.message;
        console.error(`❌ Crank ${i + 1}/${numCranks} failed:`, err.message);
        break;
      }
    }

    // Re-fetch crank status to confirm
    const slabData = await fetchSlabRaw(connection, slabPubkey);
    const engine = parseEngine(slabData);
    const currentSlot = await connection.getSlot();
    const newCrankAge = currentSlot - Number(engine.lastCrankSlot);
    const sweepAge = currentSlot - Number(engine.lastSweepStartSlot);

    res.json({
      success: signatures.length > 0,
      cranksExecuted: signatures.length,
      cranksRequested: numCranks,
      signatures,
      lastError,
      message: signatures.length === numCranks
        ? `All ${numCranks} cranks executed successfully`
        : `${signatures.length}/${numCranks} cranks executed`,
      newStatus: {
        lastCrankSlot: Number(engine.lastCrankSlot),
        lastSweepStartSlot: Number(engine.lastSweepStartSlot),
        lastSweepCompleteSlot: Number(engine.lastSweepCompleteSlot),
        crankStep: engine.crankStep,
        currentSlot,
        crankAge: newCrankAge,
        sweepAge,
        crankIsFresh: newCrankAge <= Number(engine.maxCrankStalenessSlots),
        sweepIsFresh: sweepAge <= Number(engine.maxCrankStalenessSlots),
        canTrade: newCrankAge <= Number(engine.maxCrankStalenessSlots)
          && sweepAge <= Number(engine.maxCrankStalenessSlots),
      },
    });
  } catch (error: any) {
    console.error('Crank error:', error);
    res.status(500).json({ error: error.message });
  }
});

/**
 * POST /api/trade/admin/deposit-lp
 * Admin endpoint - deposit SOL to the LP account
 * Uses server-side LP keypair to sign
 */
tradingRouter.post('/admin/deposit-lp', async (req, res) => {
  try {
    const { amount } = req.body;

    if (!amount || amount <= 0) {
      return res.status(400).json({ error: 'Missing or invalid amount (in SOL)' });
    }

    if (!lpOwnerKeypair) {
      return res.status(500).json({ error: 'LP owner keypair not loaded' });
    }

    const lpOwnerPubkey = lpOwnerKeypair.publicKey;
    const slabPubkey = getSlabPubkey();
    const connection = getConnection();

    // LP account is at index 0
    const lpIdx = config.lp.index;

    // Build deposit transaction
    const transaction = new Transaction();
    const mint = new PublicKey(config.mint);
    const vaultAccount = new PublicKey(config.vault);
    const lpAta = getAssociatedTokenAddress(mint, lpOwnerPubkey);

    // Convert SOL to lamports
    const lamports = Math.floor(amount * LAMPORTS_PER_SOL);

    // Transfer SOL to ATA
    const transferIx = buildTransferToAtaInstruction({
      from: lpOwnerPubkey,
      toAta: lpAta,
      lamports: lamports,
    });
    transaction.add(transferIx);

    // Sync native - wrap SOL to wSOL
    const syncIx = buildSyncNativeInstruction({ ata: lpAta });
    transaction.add(syncIx);

    // Deposit to percolator LP account
    const depositIx = buildDepositInstruction({
      slabAccount: slabPubkey,
      userAuthority: lpOwnerPubkey,
      userAta: lpAta,
      vaultAccount: vaultAccount,
      userIdx: lpIdx,
      amount: lamports,
    });
    transaction.add(depositIx);

    // Set blockhash and fee payer
    const blockhash = await getRecentBlockhash();
    transaction.recentBlockhash = blockhash;
    transaction.feePayer = lpOwnerPubkey;

    // Server signs the transaction (LP owner)
    transaction.sign(lpOwnerKeypair);

    // Send and confirm
    console.log(`📝 Depositing ${amount} SOL to LP account (idx ${lpIdx})...`);
    const signature = await connection.sendRawTransaction(transaction.serialize());

    console.log(`⏳ Waiting for confirmation: ${signature}`);
    await connection.confirmTransaction(signature, 'confirmed');

    console.log(`✅ LP deposit confirmed: ${signature}`);

    res.json({
      success: true,
      signature: signature,
      amount: amount,
      lamports: lamports,
      lpIdx: lpIdx,
      message: `Deposited ${amount} SOL to LP account`,
    });
  } catch (error: any) {
    console.error('LP deposit error:', error);
    res.status(500).json({ error: error.message });
  }
});

/**
 * POST /api/trade/test-execute
 * Test endpoint - server executes a full trade (TradeCpi) for debugging
 * This will actually move funds on-chain
 */
tradingRouter.post('/test-execute', async (req, res) => {
  try {
    const { user, size, side = 'buy' } = req.body;

    if (!user || !size) {
      return res.status(400).json({ error: 'Missing required fields: user, size' });
    }

    if (!lpOwnerKeypair) {
      return res.status(500).json({ error: 'LP keypair not loaded' });
    }

    const userPubkey = new PublicKey(user);
    const slabPubkey = getSlabPubkey();
    const oracleAccount = new PublicKey(config.oracle);
    const connection = getConnection();

    // Find user index
    const slabData = await fetchSlabRaw(connection, slabPubkey);
    const accounts = parseAllAccounts(slabData);
    const found = findAccountByOwner(accounts, userPubkey.toBase58());

    if (!found) {
      return res.status(400).json({ error: 'User account not found. Create one first.' });
    }

    const userIdx = found.idx;
    const sizeE6 = Math.floor(parseFloat(size) * 1e6) * (side === 'buy' ? 1 : -1);

    // Get LP and matcher info
    const matcherProgram = new PublicKey(config.matcherProgramId);
    const matcherContext = new PublicKey(config.lp.matcherContext);
    const lpPda = new PublicKey(config.lp.pda);
    const lpIdx = config.lp.index;

    console.log(`🧪 [TEST] Executing trade for user ${userIdx}`);
    console.log(`   Size: ${sizeE6} (${side})`);
    console.log(`   LP: ${lpOwnerKeypair.publicKey.toBase58()}`);

    // Build TradeCpi instruction
    const tradeIx = buildTradeCpiInstruction({
      userAuthority: userPubkey,
      lpOwner: lpOwnerKeypair.publicKey,
      slabAccount: slabPubkey,
      oracleAccount: oracleAccount,
      matcherProgram: matcherProgram,
      matcherContext: matcherContext,
      lpPda: lpPda,
      lpIdx: lpIdx,
      userIdx: userIdx,
      size: sizeE6,
    });

    const transaction = new Transaction().add(tradeIx);
    const blockhash = await getRecentBlockhash();
    transaction.recentBlockhash = blockhash;
    transaction.feePayer = userPubkey;

    // BOTH server and user need to sign for TradeCpi
    // For this test, we'll just sign with LP keypair and return the tx for user to sign
    transaction.partialSign(lpOwnerKeypair);

    console.log(`✅ [TEST] Transaction built and LP signed`);
    console.log(`   Now send this to frontend for user signature`);

    const serializedTx = serializeTransaction(transaction);

    res.json({
      success: true,
      transaction: serializedTx,
      needsSigning: true,
      message: 'Transaction ready for user signature. Sign and send to see if it executes on-chain.',
      userIdx,
      sizeE6,
      side,
    });

  } catch (error: any) {
    console.error('Test execute error:', error);
    res.status(500).json({ error: error.message });
  }
});

/**
 * GET /api/trade/price
 * Get current market price for a coin
 * For now returns a simple fallback price when charts aren't available
 */
tradingRouter.get('/price', async (req, res) => {
  try {
    const { coin = 'SOL' } = req.query;

    // Simple fallback prices - in production these would come from an oracle
    const prices: Record<string, number> = {
      'SOL': 188.50,
      'ETH': 3200.00,
      'BTC': 65000.00,
      'JUP': 0.85,
      'BONK': 0.000015,
      'WIF': 2.10,
    };

    const price = prices[coin as string] || 0;

    res.json({
      success: true,
      coin,
      price,
      timestamp: Date.now(),
      source: 'fallback', // In production: 'oracle' or 'chart'
    });
  } catch (error: any) {
    console.error('Price fetch error:', error);
    res.status(500).json({ error: error.message });
  }
});

/**
 * GET /api/trade/keeper/status
 * Get automated crank keeper status
 */
tradingRouter.get('/keeper/status', async (req, res) => {
  try {
    const status = getCrankKeeperStatus();
    res.json({
      success: true,
      keeper: status,
    });
  } catch (error: any) {
    console.error('Keeper status error:', error);
    res.status(500).json({ error: error.message });
  }
});

/**
 * POST /api/trade/keeper/stop
 * Stop the automated crank keeper
 */
tradingRouter.post('/keeper/stop', async (req, res) => {
  try {
    stopCrankKeeper();
    res.json({
      success: true,
      message: 'Crank keeper stopped',
    });
  } catch (error: any) {
    console.error('Keeper stop error:', error);
    res.status(500).json({ error: error.message });
  }
});

/**
 * POST /api/trade/keeper/start
 * Start the automated crank keeper
 */
tradingRouter.post('/keeper/start', async (req, res) => {
  try {
    const connection = getConnection();
    const slabAddress = getSlabPubkey();

    if (!lpOwnerKeypair) {
      return res.status(500).json({ error: 'LP keypair not loaded' });
    }

    startCrankKeeper(connection, slabAddress, lpOwnerKeypair);

    res.json({
      success: true,
      message: 'Crank keeper started',
    });
  } catch (error: any) {
    console.error('Keeper start error:', error);
    res.status(500).json({ error: error.message });
  }
});

/**
 * POST /api/trade/keeper/reset-stats
 * Reset crank keeper statistics
 */
tradingRouter.post('/keeper/reset-stats', async (req, res) => {
  try {
    resetCrankKeeperStats();
    res.json({
      success: true,
      message: 'Crank keeper stats reset',
    });
  } catch (error: any) {
    console.error('Keeper reset error:', error);
    res.status(500).json({ error: error.message });
  }
});
