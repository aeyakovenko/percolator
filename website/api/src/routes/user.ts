import { Router } from 'express';
import { PublicKey, Connection } from '@solana/web3.js';
import {
  fetchSlabRaw,
  parseAllAccounts,
  parseEngine,
  parseParams,
  parseConfig,
  formatLamports,
  formatPositionSize,
  calculateMarginUsage,
  AccountKind,
  SlabAccount,
} from '../services/slab-parser';
import {
  loadDevnetConfig,
  getSlabPubkey,
  getRpcUrl,
  getMarketInfo,
} from '../services/devnet-config';

export const userRouter = Router();

function getConnection(): Connection {
  return new Connection(getRpcUrl(), 'confirmed');
}

/**
 * Convert inverted price to real price
 * When invert=true, the slab stores 1e12/price, so we need to invert back
 */
function convertPrice(rawPriceE6: number, invert: boolean): number {
  if (invert && rawPriceE6 > 0) {
    // Raw is 1e12/actual_price, so actual = 1e12/raw
    return 1e12 / rawPriceE6 / 1e6; // Convert to USD
  }
  return rawPriceE6 / 1e6; // Already in correct format
}

/**
 * Get current oracle price
 */
async function getOraclePrice(connection: Connection): Promise<number> {
  try {
    const config = loadDevnetConfig();
    const oraclePubkey = new PublicKey(config.oracle);
    const oracleInfo = await connection.getAccountInfo(oraclePubkey);

    if (!oracleInfo) return 0;

    // Parse Chainlink OCR2 price
    const oracleData = Buffer.from(oracleInfo.data);
    const CL_MIN_LEN = 224;
    const CL_OFF_DECIMALS = 138;
    const CL_OFF_ANSWER = 216;

    if (oracleData.length >= CL_MIN_LEN) {
      const decimals = oracleData.readUInt8(CL_OFF_DECIMALS);
      const lo = oracleData.readBigUInt64LE(CL_OFF_ANSWER);
      const hi = oracleData.readBigInt64LE(CL_OFF_ANSWER + 8);
      const rawAnswer = (hi << 64n) | lo;

      // Convert to USD price
      const scale = 6 - decimals;
      let rawPriceE6: number;
      if (scale >= 0) {
        rawPriceE6 = Number(rawAnswer) * Math.pow(10, scale);
      } else {
        rawPriceE6 = Number(rawAnswer) / Math.pow(10, -scale);
      }

      return rawPriceE6 / 1e6; // Return as USD price
    }
    return 0;
  } catch (error) {
    console.error('Failed to fetch oracle price:', error);
    return 0;
  }
}

/**
 * GET /api/user/balance
 * Get user's balance in the slab (real on-chain data)
 */
userRouter.get('/balance', async (req, res) => {
  try {
    const { user } = req.query;

    if (!user) {
      return res.status(400).json({ error: 'user address required' });
    }

    const userPubkey = new PublicKey(user as string);
    const connection = getConnection();
    const slabPubkey = getSlabPubkey();

    const data = await fetchSlabRaw(connection, slabPubkey);
    const accounts = parseAllAccounts(data);
    const params = parseParams(data);
    const slabConfig = parseConfig(data);
    const engine = parseEngine(data);

    const found = accounts.find(({ account }) => account.owner === userPubkey.toBase58());

    if (!found) {
      // User has no account yet
      return res.json({
        success: true,
        hasAccount: false,
        user: userPubkey.toBase58(),
        slab: slabPubkey.toBase58(),
        balance: 0,
        available: 0,
        reserved: 0,
        pnl_unrealized: 0,
        pnl_realized: 0,
        message: 'No account found - deposit collateral to create one',
      });
    }

    const { account } = found;

    // Calculate available balance (capital - margin used)
    const capitalNum = Number(account.capital) / 1e9; // Convert from lamports
    const pnlNum = Number(account.pnl) / 1e9;
    const marginUsage = calculateMarginUsage(
      account.positionSize,
      account.entryPrice,
      account.capital,
      params.initialMarginBps
    );
    const reservedAmount = (capitalNum * marginUsage) / 100;
    const available = capitalNum - reservedAmount;

    // Convert entry price for display
    const rawEntryPriceE6 = Number(account.entryPrice);
    const entryPriceUSD = convertPrice(rawEntryPriceE6, slabConfig.invert);

    // Get real-time mark price
    const markPriceUSD = await getOraclePrice(connection);

    res.json({
      success: true,
      hasAccount: true,
      user: userPubkey.toBase58(),
      slab: slabPubkey.toBase58(),
      accountIndex: found.idx,
      accountId: account.accountId.toString(),
      kind: account.kind === AccountKind.LP ? 'LP' : 'User',
      balance: capitalNum,
      balanceRaw: account.capital.toString(),
      available: Math.max(0, available),
      reserved: reservedAmount,
      pnl_unrealized: pnlNum,
      pnl_realized: 0, // Would need historical data
      marginUsage: marginUsage,
      warmupStartedAt: account.warmupStartedAtSlot.toString(),
      position: {
        size: formatPositionSize(account.positionSize),
        sizeRaw: account.positionSize.toString(),
        entryPrice: entryPriceUSD.toFixed(2),
        markPrice: markPriceUSD > 0 ? markPriceUSD.toFixed(2) : entryPriceUSD.toFixed(2),
        side: account.positionSize > 0n ? 'long' : account.positionSize < 0n ? 'short' : 'flat',
      },
    });
  } catch (error: any) {
    console.error('❌ Error fetching user balance:', error);
    res.status(500).json({ error: error.message });
  }
});

/**
 * GET /api/user/positions
 * Get user's open positions (real on-chain data)
 */
userRouter.get('/positions', async (req, res) => {
  try {
    const { user } = req.query;

    if (!user) {
      return res.status(400).json({ error: 'user address required' });
    }

    const userPubkey = new PublicKey(user as string);
    const connection = getConnection();
    const slabPubkey = getSlabPubkey();
    const marketInfo = getMarketInfo();

    const data = await fetchSlabRaw(connection, slabPubkey);
    const accounts = parseAllAccounts(data);
    const slabConfig = parseConfig(data);

    const found = accounts.find(({ account }) => account.owner === userPubkey.toBase58());

    if (!found || found.account.positionSize === 0n) {
      return res.json({
        success: true,
        positions: [],
        message: found ? 'No open positions' : 'No account found',
      });
    }

    const { account } = found;

    // Single position per account in Percolator
    const positionSize = account.positionSize;
    const absSize = positionSize < 0n ? -positionSize : positionSize;
    const sizeNum = Number(absSize) / 1e6; // 6 decimals for perp size
    const isLong = positionSize > 0n;

    // Convert entry price - handle inversion
    const rawEntryPriceE6 = Number(account.entryPrice);
    const entryPriceUSD = convertPrice(rawEntryPriceE6, slabConfig.invert);

    // Get real-time oracle price for mark
    const markPriceUSD = await getOraclePrice(connection) || entryPriceUSD;

    // Calculate notional and P&L in USD
    const notionalUSD = sizeNum * entryPriceUSD;
    const pnlUSD = isLong
      ? (markPriceUSD - entryPriceUSD) * sizeNum
      : (entryPriceUSD - markPriceUSD) * sizeNum;
    const pnlPct = notionalUSD > 0 ? (pnlUSD / notionalUSD) * 100 : 0;

    res.json({
      success: true,
      positions: [
        {
          symbol: marketInfo.pair,
          qty: sizeNum,
          qtyRaw: positionSize.toString(),
          side: isLong ? 'long' : 'short',
          entry_price: entryPriceUSD,
          mark_price: markPriceUSD,
          notional: notionalUSD,
          pnl: pnlUSD,
          pnl_pct: pnlPct,
          leverage: marketInfo.maxLeverage,
          margin: notionalUSD / marketInfo.maxLeverage,
          accountIndex: found.idx,
          warmupStartedAt: account.warmupStartedAtSlot.toString(),
        },
      ],
    });
  } catch (error: any) {
    console.error('❌ Error fetching user positions:', error);
    res.status(500).json({ error: error.message });
  }
});

/**
 * GET /api/user/orders
 * Get user's open orders (currently returns empty - orders execute instantly with LP)
 */
userRouter.get('/orders', async (req, res) => {
  try {
    const { user } = req.query;

    if (!user) {
      return res.status(400).json({ error: 'user address required' });
    }

    // In Percolator with passive LP, orders execute instantly
    // There's no order book in the traditional sense
    res.json({
      success: true,
      orders: [],
      message: 'Percolator uses instant execution with passive LP - no pending orders',
    });
  } catch (error: any) {
    console.error('❌ Error fetching user orders:', error);
    res.status(500).json({ error: error.message });
  }
});

/**
 * GET /api/user/trades
 * Get user's trade history (from on-chain transactions)
 */
userRouter.get('/trades', async (req, res) => {
  try {
    const { user, limit = '50' } = req.query;

    if (!user) {
      return res.status(400).json({ error: 'user address required' });
    }

    const userPubkey = new PublicKey(user as string);
    const connection = getConnection();
    const slabPubkey = getSlabPubkey();

    const maxTrades = Math.min(parseInt(limit as string), 100);

    // Fetch recent signatures for the slab that involve this user
    const signatures = await connection.getSignaturesForAddress(slabPubkey, { limit: maxTrades * 2 });

    // Filter to find transactions involving this user
    const userTrades = [];

    for (const sig of signatures.slice(0, maxTrades)) {
      try {
        const tx = await connection.getTransaction(sig.signature, {
          commitment: 'confirmed',
          maxSupportedTransactionVersion: 0,
        });

        if (!tx) continue;

        const accountKeys = tx.transaction.message.getAccountKeys();
        const involvedAddresses = [];
        for (let i = 0; i < accountKeys.length; i++) {
          involvedAddresses.push(accountKeys.get(i)?.toBase58());
        }

        // Check if user is involved in this transaction
        if (involvedAddresses.includes(userPubkey.toBase58())) {
          userTrades.push({
            signature: sig.signature,
            slot: sig.slot,
            blockTime: sig.blockTime,
            err: sig.err,
            solscanLink: `https://solscan.io/tx/${sig.signature}?cluster=devnet`,
          });
        }
      } catch {
        // Skip failed fetches
      }
    }

    res.json({
      success: true,
      user: userPubkey.toBase58(),
      trades: userTrades,
      count: userTrades.length,
    });
  } catch (error: any) {
    console.error('❌ Error fetching user trades:', error);
    res.status(500).json({ error: error.message });
  }
});

/**
 * GET /api/user/:walletAddress/portfolio
 * Get user's portfolio summary (real on-chain data)
 */
userRouter.get('/:walletAddress/portfolio', async (req, res) => {
  try {
    const { walletAddress } = req.params;

    if (!walletAddress) {
      return res.status(400).json({ error: 'wallet address required' });
    }

    const userPubkey = new PublicKey(walletAddress);
    const connection = getConnection();
    const slabPubkey = getSlabPubkey();
    const marketInfo = getMarketInfo();

    const data = await fetchSlabRaw(connection, slabPubkey);
    const accounts = parseAllAccounts(data);
    const params = parseParams(data);
    const slabConfig = parseConfig(data);

    const found = accounts.find(({ account }) => account.owner === userPubkey.toBase58());

    if (!found) {
      return res.json({
        success: true,
        hasAccount: false,
        user: walletAddress,
        equity: 0,
        freeCollateral: 0,
        totalPositionValue: 0,
        unrealizedPnl: 0,
        marginUsage: 0,
        positions: [],
        message: 'No account found - deposit collateral to start trading',
      });
    }

    const { account } = found;

    // Calculate portfolio metrics
    const capitalNum = Number(account.capital) / 1e9;
    const pnlNum = Number(account.pnl) / 1e9;
    const equity = capitalNum + pnlNum;

    const marginUsage = calculateMarginUsage(
      account.positionSize,
      account.entryPrice,
      account.capital,
      params.initialMarginBps
    );

    const reservedMargin = (capitalNum * marginUsage) / 100;
    const freeCollateral = Math.max(0, equity - reservedMargin);

    // Get real-time oracle price
    const markPriceUSD = await getOraclePrice(connection);

    // Build positions array
    const positions = [];
    if (account.positionSize !== 0n) {
      const positionSize = account.positionSize;
      const absSize = positionSize < 0n ? -positionSize : positionSize;
      const sizeNum = Number(absSize) / 1e6;
      const isLong = positionSize > 0n;

      // Convert entry price - handle inversion
      const rawEntryPriceE6 = Number(account.entryPrice);
      const entryPriceUSD = convertPrice(rawEntryPriceE6, slabConfig.invert);
      const notionalUSD = sizeNum * entryPriceUSD;

      // Calculate P&L with real-time price
      const pnlUSD = markPriceUSD > 0
        ? (isLong ? (markPriceUSD - entryPriceUSD) : (entryPriceUSD - markPriceUSD)) * sizeNum
        : 0;

      positions.push({
        symbol: marketInfo.pair,
        side: isLong ? 'long' : 'short',
        size: sizeNum,
        entryPrice: entryPriceUSD,
        markPrice: markPriceUSD || entryPriceUSD,
        notional: notionalUSD,
        pnl: pnlUSD,
        leverage: marketInfo.maxLeverage,
      });
    }

    res.json({
      success: true,
      hasAccount: true,
      user: walletAddress,
      accountIndex: found.idx,
      accountId: account.accountId.toString(),
      kind: account.kind === AccountKind.LP ? 'LP' : 'User',
      equity: equity,
      freeCollateral: freeCollateral,
      totalPositionValue: positions.reduce((sum, p) => sum + p.notional, 0),
      unrealizedPnl: pnlNum,
      marginUsage: marginUsage,
      positions: positions,
      warmupStartedAt: account.warmupStartedAtSlot.toString(),
      market: marketInfo,
    });
  } catch (error: any) {
    console.error('❌ Error fetching portfolio:', error);
    res.status(500).json({ error: error.message });
  }
});

/**
 * GET /api/user/all
 * Get all user accounts (for admin/debug)
 */
userRouter.get('/all', async (req, res) => {
  try {
    const connection = getConnection();
    const slabPubkey = getSlabPubkey();

    const data = await fetchSlabRaw(connection, slabPubkey);
    const accounts = parseAllAccounts(data);

    const users = accounts
      .filter(({ account }) => account.kind === AccountKind.User)
      .map(({ idx, account }) => ({
        index: idx,
        accountId: account.accountId.toString(),
        owner: account.owner,
        capital: formatLamports(account.capital),
        pnl: formatLamports(account.pnl),
        positionSize: formatPositionSize(account.positionSize),
        entryPrice: account.entryPrice.toString(),
      }));

    const lps = accounts
      .filter(({ account }) => account.kind === AccountKind.LP)
      .map(({ idx, account }) => ({
        index: idx,
        accountId: account.accountId.toString(),
        owner: account.owner,
        capital: formatLamports(account.capital),
        matcherProgram: account.matcherProgram,
        matcherContext: account.matcherContext,
      }));

    res.json({
      success: true,
      slab: slabPubkey.toBase58(),
      totalAccounts: accounts.length,
      users: users,
      lps: lps,
      timestamp: Date.now(),
    });
  } catch (error: any) {
    console.error('❌ Error fetching all users:', error);
    res.status(500).json({ error: error.message });
  }
});
