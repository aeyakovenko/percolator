import { Router } from 'express';
import { PublicKey, Connection } from '@solana/web3.js';
import { activeOrders, completedTrades } from './trading';
import {
  fetchAndParseSlab,
  fetchSlabRaw,
  parseHeader,
  parseConfig,
  parseParams,
  parseEngine,
  parseAllAccounts,
  formatLamports,
  formatPositionSize,
  AccountKind,
} from '../services/slab-parser';
import {
  loadDevnetConfig,
  getSlabPubkey,
  getProgramId,
  getRpcUrl,
  getMarketInfo,
} from '../services/devnet-config';

export const slabRouter = Router();

// Get config on startup
let config: ReturnType<typeof loadDevnetConfig>;
try {
  config = loadDevnetConfig();
} catch (e) {
  console.warn('Using fallback slab config');
}

function getConnection(): Connection {
  return new Connection(getRpcUrl(), 'confirmed');
}

/**
 * GET /api/slab-live/state
 * Get full parsed slab state from on-chain data
 */
slabRouter.get('/state', async (req, res) => {
  try {
    console.log('📊 Fetching and parsing slab state from devnet...');
    const connection = getConnection();
    const slabPubkey = getSlabPubkey();

    const state = await fetchAndParseSlab(connection, slabPubkey);
    const marketInfo = getMarketInfo();

    // Format for JSON response (bigints need string conversion)
    const response = {
      success: true,
      network: config?.network || 'devnet',
      slabAccount: slabPubkey.toBase58(),
      programId: getProgramId().toBase58(),
      market: marketInfo,
      header: {
        magic: state.header.magic.toString(16),
        version: state.header.version,
        bump: state.header.bump,
        admin: state.header.admin,
        nonce: state.header.nonce.toString(),
        lastThrUpdateSlot: state.header.lastThrUpdateSlot.toString(),
      },
      config: {
        collateralMint: state.config.collateralMint,
        vaultPubkey: state.config.vaultPubkey,
        indexFeedId: state.config.indexFeedId,
        maxStalenessSlots: state.config.maxStalenessSlots.toString(),
        confFilterBps: state.config.confFilterBps,
        vaultAuthorityBump: state.config.vaultAuthorityBump,
        invert: state.config.invert,
        unitScale: state.config.unitScale,
      },
      params: {
        warmupPeriodSlots: state.params.warmupPeriodSlots.toString(),
        maintenanceMarginBps: state.params.maintenanceMarginBps.toString(),
        initialMarginBps: state.params.initialMarginBps.toString(),
        tradingFeeBps: state.params.tradingFeeBps.toString(),
        maxAccounts: state.params.maxAccounts.toString(),
        newAccountFee: formatLamports(state.params.newAccountFee),
        riskReductionThreshold: formatLamports(state.params.riskReductionThreshold),
        liquidationFeeBps: state.params.liquidationFeeBps.toString(),
      },
      engine: {
        vault: formatLamports(state.engine.vault),
        insuranceFund: {
          balance: formatLamports(state.engine.insuranceFund.balance),
          feeRevenue: formatLamports(state.engine.insuranceFund.feeRevenue),
        },
        currentSlot: state.engine.currentSlot.toString(),
        fundingIndexQpbE6: state.engine.fundingIndexQpbE6.toString(),
        lastFundingSlot: state.engine.lastFundingSlot.toString(),
        totalOpenInterest: formatLamports(state.engine.totalOpenInterest, 6),
        riskReductionOnly: state.engine.riskReductionOnly,
        warmupPaused: state.engine.warmupPaused,
        crankStep: state.engine.crankStep,
        numUsedAccounts: state.engine.numUsedAccounts,
        nextAccountId: state.engine.nextAccountId.toString(),
        lifetimeLiquidations: state.engine.lifetimeLiquidations.toString(),
        lifetimeForceCloses: state.engine.lifetimeForceCloses.toString(),
      },
      accounts: state.accounts.map(({ idx, account }) => ({
        index: idx,
        kind: account.kind === AccountKind.LP ? 'LP' : 'User',
        accountId: account.accountId.toString(),
        owner: account.owner,
        capital: formatLamports(account.capital),
        pnl: formatLamports(account.pnl),
        positionSize: formatPositionSize(account.positionSize),
        entryPrice: account.entryPrice.toString(),
        warmupStartedAtSlot: account.warmupStartedAtSlot.toString(),
        matcherProgram: account.matcherProgram,
        matcherContext: account.matcherContext,
      })),
      timestamp: Date.now(),
    };

    console.log(`✅ Parsed slab state: ${state.accounts.length} accounts, vault=${response.engine.vault}`);
    res.json(response);
  } catch (error: any) {
    console.error('❌ Error parsing slab state:', error);
    res.status(500).json({
      error: error.message,
      slabAccount: config?.slab || 'unknown',
    });
  }
});

/**
 * GET /api/slab-live/orderbook
 * Fetch orderbook from slab account - includes LP quotes
 */
slabRouter.get('/orderbook', async (req, res) => {
  try {
    console.log('📖 Fetching orderbook from slab...');
    const connection = getConnection();
    const slabPubkey = getSlabPubkey();

    const state = await fetchAndParseSlab(connection, slabPubkey);
    const marketInfo = getMarketInfo();

    // Find LP account to get passive quotes
    const lpAccount = state.accounts.find(({ account }) => account.kind === AccountKind.LP);

    // Build orderbook from LP passive quotes (±50bps spread from oracle)
    // For now, we'll show the LP as providing liquidity
    const lpCapital = lpAccount ? formatLamports(lpAccount.account.capital) : '0';

    // Also include active orders from in-memory (for user orders during warmup)
    const now = Date.now();
    const validOrders = activeOrders.filter(o => o.expiryMs > now);

    const bids = validOrders
      .filter(o => o.side === 'buy')
      .map(o => ({ price: o.price, quantity: o.quantity, user: o.user.substring(0, 6) + '...' }))
      .sort((a, b) => b.price - a.price);

    const asks = validOrders
      .filter(o => o.side === 'sell')
      .map(o => ({ price: o.price, quantity: o.quantity, user: o.user.substring(0, 6) + '...' }))
      .sort((a, b) => a.price - b.price);

    const bestBid = bids.length > 0 ? bids[0].price : 0;
    const bestAsk = asks.length > 0 ? asks[0].price : 0;
    const midPrice = (bestBid && bestAsk) ? (bestBid + bestAsk) / 2 : (bestBid || bestAsk || 0);
    const spread = (bestBid && bestAsk) ? ((bestAsk - bestBid) / midPrice) * 100 : 0;

    res.json({
      success: true,
      slabAccount: slabPubkey.toBase58(),
      programId: getProgramId().toBase58(),
      market: marketInfo,
      lp: {
        available: lpAccount !== undefined,
        capital: lpCapital,
        spreadBps: 50, // LP provides ±50bps quotes
        accountIndex: lpAccount?.idx,
      },
      orderbook: {
        bids,
        asks,
        midPrice,
        spread,
        lastUpdate: Date.now(),
      },
      recentTrades: completedTrades.slice(-10).reverse(),
      engine: {
        vault: formatLamports(state.engine.vault),
        insuranceBalance: formatLamports(state.engine.insuranceFund.balance),
        totalOI: formatLamports(state.engine.totalOpenInterest, 6),
        numAccounts: state.engine.numUsedAccounts,
      },
      message: lpAccount
        ? `LP active with ${lpCapital} SOL capital, ±50bps spread`
        : 'No LP - market orders may fail',
    });
  } catch (error: any) {
    console.error('❌ Error fetching orderbook:', error);
    res.status(500).json({
      error: error.message,
      slabAccount: config?.slab || 'unknown',
    });
  }
});

/**
 * GET /api/slab-live/accounts
 * Get all accounts in the slab
 */
slabRouter.get('/accounts', async (req, res) => {
  try {
    const connection = getConnection();
    const slabPubkey = getSlabPubkey();

    const data = await fetchSlabRaw(connection, slabPubkey);
    const accounts = parseAllAccounts(data);

    res.json({
      success: true,
      slabAccount: slabPubkey.toBase58(),
      count: accounts.length,
      accounts: accounts.map(({ idx, account }) => ({
        index: idx,
        kind: account.kind === AccountKind.LP ? 'LP' : 'User',
        accountId: account.accountId.toString(),
        owner: account.owner,
        capital: formatLamports(account.capital),
        capitalRaw: account.capital.toString(),
        pnl: formatLamports(account.pnl),
        positionSize: formatPositionSize(account.positionSize),
        positionSizeRaw: account.positionSize.toString(),
        entryPrice: account.entryPrice.toString(),
        warmupStartedAtSlot: account.warmupStartedAtSlot.toString(),
        matcherProgram: account.matcherProgram,
        matcherContext: account.matcherContext,
        feeCredits: account.feeCredits.toString(),
        lastFeeSlot: account.lastFeeSlot.toString(),
      })),
      timestamp: Date.now(),
    });
  } catch (error: any) {
    console.error('❌ Error fetching accounts:', error);
    res.status(500).json({ error: error.message });
  }
});

/**
 * GET /api/slab-live/account/:owner
 * Get account by owner pubkey
 */
slabRouter.get('/account/:owner', async (req, res) => {
  try {
    const { owner } = req.params;
    const ownerPubkey = new PublicKey(owner);

    const connection = getConnection();
    const slabPubkey = getSlabPubkey();

    const data = await fetchSlabRaw(connection, slabPubkey);
    const accounts = parseAllAccounts(data);

    const found = accounts.find(({ account }) => account.owner === ownerPubkey.toBase58());

    if (!found) {
      return res.status(404).json({
        success: false,
        error: 'Account not found for owner',
        owner: ownerPubkey.toBase58(),
      });
    }

    const { idx, account } = found;

    res.json({
      success: true,
      slabAccount: slabPubkey.toBase58(),
      index: idx,
      kind: account.kind === AccountKind.LP ? 'LP' : 'User',
      accountId: account.accountId.toString(),
      owner: account.owner,
      capital: formatLamports(account.capital),
      capitalRaw: account.capital.toString(),
      pnl: formatLamports(account.pnl),
      positionSize: formatPositionSize(account.positionSize),
      positionSizeRaw: account.positionSize.toString(),
      entryPrice: account.entryPrice.toString(),
      warmupStartedAtSlot: account.warmupStartedAtSlot.toString(),
      matcherProgram: account.matcherProgram,
      matcherContext: account.matcherContext,
      feeCredits: account.feeCredits.toString(),
      lastFeeSlot: account.lastFeeSlot.toString(),
      timestamp: Date.now(),
    });
  } catch (error: any) {
    console.error('❌ Error fetching account:', error);
    res.status(500).json({ error: error.message });
  }
});

/**
 * GET /api/slab-live/transactions
 * Get recent transactions for the slab account
 */
slabRouter.get('/transactions', async (req, res) => {
  try {
    const connection = getConnection();
    const slabPubkey = getSlabPubkey();

    const limit = parseInt(req.query.limit as string) || 10;

    const signatures = await connection.getSignaturesForAddress(slabPubkey, { limit });

    const transactionsWithSigners = await Promise.all(
      signatures.slice(0, 10).map(async (sig) => {
        try {
          const txDetails = await connection.getTransaction(sig.signature, {
            commitment: 'confirmed',
            maxSupportedTransactionVersion: 0,
          });

          let signer = '';
          if (txDetails) {
            const accountKeys = txDetails.transaction.message.getAccountKeys();
            if (accountKeys && accountKeys.length > 0) {
              signer = accountKeys.get(0)?.toBase58() || '';
            }
          }

          return {
            signature: sig.signature,
            slot: sig.slot,
            blockTime: sig.blockTime,
            err: sig.err,
            memo: sig.memo,
            signer,
            solscanLink: `https://solscan.io/tx/${sig.signature}?cluster=devnet`,
          };
        } catch {
          return {
            signature: sig.signature,
            slot: sig.slot,
            blockTime: sig.blockTime,
            err: sig.err,
            memo: sig.memo,
            signer: '',
            solscanLink: `https://solscan.io/tx/${sig.signature}?cluster=devnet`,
          };
        }
      })
    );

    res.json({
      success: true,
      slabAccount: slabPubkey.toBase58(),
      transactions: transactionsWithSigners,
    });
  } catch (error: any) {
    console.error('❌ Error fetching transactions:', error);
    res.status(500).json({ error: error.message });
  }
});

/**
 * GET /api/slab-live/config
 * Get market configuration
 */
slabRouter.get('/config', async (req, res) => {
  try {
    const marketInfo = getMarketInfo();
    const devnetConfig = loadDevnetConfig();

    res.json({
      success: true,
      network: devnetConfig.network,
      programId: devnetConfig.programId,
      matcherProgramId: devnetConfig.matcherProgramId,
      slab: devnetConfig.slab,
      vault: devnetConfig.vault,
      oracle: devnetConfig.oracle,
      oracleType: devnetConfig.oracleType,
      mint: devnetConfig.mint,
      market: marketInfo,
      lp: devnetConfig.lp,
      admin: devnetConfig.admin,
    });
  } catch (error: any) {
    console.error('❌ Error fetching config:', error);
    res.status(500).json({ error: error.message });
  }
});
