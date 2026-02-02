"use client"

import { useState, useEffect, useRef } from 'react';
import { useWallet } from '@solana/wallet-adapter-react';
import { Transaction, Connection, clusterApiUrl } from '@solana/web3.js';
import { toast } from '@/components/ui/use-toast';
import { Clock, AlertCircle, TrendingUp, TrendingDown, Wallet } from 'lucide-react';

type OrderSide = 'buy' | 'sell';
type OrderType = 'limit' | 'market';

interface OrderFormProps {
  coin: "ethereum" | "bitcoin" | "solana" | "jupiter" | "bonk" | "dogwifhat";
  currentPrice: number;
}

interface UserBalance {
  success: boolean;
  hasAccount: boolean;
  balance: number;
  available: number;
  position?: {
    size: string;
    side: 'long' | 'short' | 'flat';
    entryPrice: string;
    markPrice: string;
  };
}

interface ReservedOrder {
  side: OrderSide;
  price: number;
  size: number;
  expiresAt: number;
  transaction: Transaction;
  holdId?: number;
}

// Helper to convert coin string to v2 format
const coinToV2 = (coin: "ethereum" | "bitcoin" | "solana" | "jupiter" | "bonk" | "dogwifhat"): 'SOL' | 'ETH' | 'BTC' | 'JUP' | 'BONK' | 'WIF' => {
  switch(coin) {
    case 'ethereum': return 'ETH';
    case 'bitcoin': return 'BTC';
    case 'solana': return 'SOL';
    case 'jupiter': return 'JUP';
    case 'bonk': return 'BONK';
    case 'dogwifhat': return 'WIF';
  }
};

export function OrderForm({ coin, currentPrice }: OrderFormProps) {
  const { publicKey, signTransaction } = useWallet();
  const [orderSide, setOrderSide] = useState<OrderSide>('buy');
  const [orderType, setOrderType] = useState<OrderType>('limit');
  const [price, setPrice] = useState(currentPrice > 0 ? currentPrice.toFixed(2) : '');
  const [size, setSize] = useState('');
  const [loading, setLoading] = useState(false);
  const [depositLoading, setDepositLoading] = useState(false);
  const [withdrawLoading, setWithdrawLoading] = useState(false);
  const [depositAmount, setDepositAmount] = useState('0.5');
  const [withdrawAmount, setWithdrawAmount] = useState('');
  const [showDeposit, setShowDeposit] = useState(false);
  const [showWithdraw, setShowWithdraw] = useState(false);
  const [reservedOrder, setReservedOrder] = useState<ReservedOrder | null>(null);
  const [timeRemaining, setTimeRemaining] = useState(0);
  const timerRef = useRef<NodeJS.Timeout | null>(null);
  const connection = new Connection(clusterApiUrl('devnet'), 'confirmed');
  const [userBalance, setUserBalance] = useState<UserBalance | null>(null);
  const [balanceLoading, setBalanceLoading] = useState(false);

  // Handle deposit
  const handleDeposit = async () => {
    if (!publicKey || !signTransaction) {
      toast({
        type: 'warning',
        title: 'Wallet Required',
        message: 'Please connect your wallet'
      });
      return;
    }

    const amount = parseFloat(depositAmount);
    if (amount <= 0) {
      toast({
        type: 'error',
        title: 'Invalid Amount',
        message: 'Please enter a valid deposit amount'
      });
      return;
    }

    setDepositLoading(true);
    try {
      const API_URL = process.env.NEXT_PUBLIC_API_URL || 'http://localhost:5001';

      const response = await fetch(`${API_URL}/api/trade/deposit`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          user: publicKey.toBase58(),
          amount: amount,
        })
      });

      const result = await response.json();

      if (!response.ok || !result.success) {
        throw new Error(result.error || result.message || 'Failed to create deposit transaction');
      }

      toast({
        type: 'info',
        title: 'Depositing',
        message: `Depositing ${amount} SOL to your trading account...`
      });

      // Sign and send the deposit transaction
      const depositTx = Transaction.from(Buffer.from(result.transaction, 'base64'));
      const signedTx = await signTransaction(depositTx);
      const signature = await connection.sendRawTransaction(signedTx.serialize());

      toast({
        type: 'info',
        title: 'Confirming',
        message: 'Waiting for confirmation...'
      });

      await connection.confirmTransaction(signature, 'confirmed');

      toast({
        type: 'success',
        title: 'Deposit Complete!',
        message: `Successfully deposited ${amount} SOL. You can now trade larger positions!`
      });

      // Refresh balance
      await fetchUserBalance();

      setShowDeposit(false);
      setDepositAmount('0.5');

    } catch (error: any) {
      console.error('Deposit failed:', error);
      toast({
        type: 'error',
        title: 'Deposit Failed',
        message: error.message || 'Failed to deposit'
      });
    } finally {
      setDepositLoading(false);
    }
  };

  // Handle withdraw
  const handleWithdraw = async () => {
    if (!publicKey || !signTransaction) {
      toast({
        type: 'warning',
        title: 'Wallet Required',
        message: 'Please connect your wallet'
      });
      return;
    }

    const amount = parseFloat(withdrawAmount);
    if (amount <= 0) {
      toast({
        type: 'error',
        title: 'Invalid Amount',
        message: 'Please enter a valid withdraw amount'
      });
      return;
    }

    // Check if user has open position
    if (userBalance?.position && parseFloat(userBalance.position.size) !== 0) {
      toast({
        type: 'error',
        title: 'Close Position First',
        message: 'You must close your position before withdrawing collateral'
      });
      return;
    }

    setWithdrawLoading(true);
    try {
      const API_URL = process.env.NEXT_PUBLIC_API_URL || 'http://localhost:5001';

      const response = await fetch(`${API_URL}/api/trade/withdraw`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          user: publicKey.toBase58(),
          amount: amount,
        })
      });

      const result = await response.json();

      if (!response.ok || !result.success) {
        throw new Error(result.error || result.message || 'Failed to create withdraw transaction');
      }

      // Sign and send transaction
      const transaction = Transaction.from(Buffer.from(result.transaction, 'base64'));
      const signedTx = await signTransaction(transaction);
      const signature = await connection.sendRawTransaction(signedTx.serialize());

      toast({
        type: 'info',
        title: 'Processing',
        message: 'Withdraw transaction sent, waiting for confirmation...'
      });

      await connection.confirmTransaction(signature, 'confirmed');

      toast({
        type: 'success',
        title: 'Withdraw Complete!',
        message: `Successfully withdrew ${amount} SOL to your wallet!`
      });

      // Refresh balance
      await fetchUserBalance();

      setShowWithdraw(false);
      setWithdrawAmount('');

    } catch (error: any) {
      console.error('Withdraw failed:', error);
      toast({
        type: 'error',
        title: 'Withdraw Failed',
        message: error.message || 'Failed to withdraw'
      });
    } finally {
      setWithdrawLoading(false);
    }
  };

  // Fetch user balance and position
  const fetchUserBalance = async () => {
    if (!publicKey) return;

    setBalanceLoading(true);
    try {
      const API_URL = process.env.NEXT_PUBLIC_API_URL || 'http://localhost:5001';
      const response = await fetch(`${API_URL}/api/user/balance?user=${publicKey.toBase58()}`);
      const data = await response.json();

      if (data.success) {
        setUserBalance(data);
      }
    } catch (error) {
      console.error('Failed to fetch balance:', error);
    } finally {
      setBalanceLoading(false);
    }
  };

  // Fetch balance on mount and when wallet changes
  useEffect(() => {
    if (publicKey) {
      fetchUserBalance();
      // Refresh every 10 seconds
      const interval = setInterval(fetchUserBalance, 10000);
      return () => clearInterval(interval);
    } else {
      setUserBalance(null);
    }
  }, [publicKey]);

  // Update price when currentPrice changes
  useEffect(() => {
    if (currentPrice > 0 && orderType === 'market') {
      setPrice(currentPrice.toFixed(2));
    }
  }, [currentPrice, orderType]);

  // Countdown timer effect
  useEffect(() => {
    if (reservedOrder) {
      timerRef.current = setInterval(() => {
        const remaining = Math.max(0, reservedOrder.expiresAt - Date.now());
        setTimeRemaining(remaining);
        
        if (remaining === 0) {
          // Auto-cancel expired reservation
          setReservedOrder(null);
          toast({
            type: 'warning',
            title: 'Reservation Expired',
            message: 'Reservation expired! Please reserve again.'
          });
          if (timerRef.current) clearInterval(timerRef.current);
        }
      }, 100);
      
      return () => {
        if (timerRef.current) clearInterval(timerRef.current);
      };
    }
  }, [reservedOrder]);

  // Step 1: Reserve the order (lock in price)
  const handleReserve = async (e: React.FormEvent) => {
    e.preventDefault();
    
    if (!publicKey || !signTransaction) {
      toast({
        type: 'warning',
        title: 'Wallet Required',
        message: 'Please connect your wallet'
      });
      return;
    }

    setLoading(true);
    try {
      const orderPrice = orderType === 'limit' ? parseFloat(price) : currentPrice;
      const orderSize = parseFloat(size);

      if (orderSize <= 0 || orderPrice <= 0) {
        throw new Error('Invalid price or size');
      }

      const coinV2 = coinToV2(coin);
      const API_URL = process.env.NEXT_PUBLIC_API_URL || 'http://localhost:5001';

      // Call API to reserve order
      const response = await fetch(`${API_URL}/api/trade/reserve`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          user: publicKey.toBase58(),
          slice: coinV2,
          side: orderSide,
          orderType: orderType,
          price: orderPrice,
          quantity: orderSize,
        })
      });

      const result = await response.json();

      if (!response.ok || !result.success) {
        throw new Error(result.error || 'Failed to reserve order');
      }

      // Handle init_user step (user doesn't have an account yet)
      if (result.step === 'init_user') {
        toast({
          type: 'info',
          title: 'Creating Account',
          message: 'Creating your trading account on-chain...'
        });

        // Sign and send the InitUser transaction
        const initTx = Transaction.from(Buffer.from(result.transaction, 'base64'));
        const signedInitTx = await signTransaction(initTx);
        const initSignature = await connection.sendRawTransaction(signedInitTx.serialize());

        toast({
          type: 'info',
          title: 'Confirming',
          message: 'Waiting for account creation to confirm...'
        });

        await connection.confirmTransaction(initSignature, 'confirmed');

        toast({
          type: 'success',
          title: 'Account Created',
          message: 'Your account is ready! Placing your trade now...'
        });

        // Now call reserve again to get the actual trade transaction
        const tradeResponse = await fetch(`${API_URL}/api/trade/reserve`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            user: publicKey.toBase58(),
            slice: coinV2,
            side: orderSide,
            orderType: orderType,
            price: orderPrice,
            quantity: orderSize,
          })
        });

        const tradeResult = await tradeResponse.json();

        if (!tradeResponse.ok || !tradeResult.success) {
          throw new Error(tradeResult.error || 'Failed to create trade after account init');
        }

        // Now we should have the trade transaction
        const transaction = Transaction.from(Buffer.from(tradeResult.transaction, 'base64'));
        const expiryMs = tradeResult.expiryMs || (Date.now() + 30000);

        const reservation: ReservedOrder = {
          side: orderSide,
          price: orderPrice,
          size: orderSize,
          expiresAt: expiryMs,
          transaction,
          holdId: tradeResult.holdId,
        };

        setReservedOrder(reservation);
        toast({
          type: 'success',
          title: 'Trade Ready',
          message: 'Trade is ready! Click Commit to execute.'
        });
        return;
      }

      // Handle trade step (normal case - user has an account)
      const transaction = Transaction.from(Buffer.from(result.transaction, 'base64'));
      const expiryMs = result.expiryMs || (Date.now() + 30000);

      const reservation: ReservedOrder = {
        side: orderSide,
        price: orderPrice,
        size: orderSize,
        expiresAt: expiryMs,
        transaction,
        holdId: result.holdId,
      };

      setReservedOrder(reservation);
      toast({
        type: 'success',
        title: 'Trade Ready',
        message: `Price locked at $${orderPrice.toFixed(2)}. Click Commit to execute on-chain.`
      });
      
    } catch (error: any) {
      console.error('Failed to reserve order:', error);
      toast({
        type: 'error',
        title: 'Reserve Failed',
        message: `Failed to reserve: ${error.message || error.toString()}`
      });
    } finally {
      setLoading(false);
    }
  };

  // Step 2: Execute the trade (TradeNoCpi - single transaction)
  const handleCommit = async () => {
    if (!reservedOrder || !signTransaction || !publicKey) return;

    setLoading(true);
    try {
      const API_URL = process.env.NEXT_PUBLIC_API_URL || 'http://localhost:5001';

      // Sign and send the trade transaction
      console.log('🔐 Signing trade transaction...');
      console.log('   Transaction has', reservedOrder.transaction.signatures.length, 'signatures before user signs');

      const signedTx = await signTransaction(reservedOrder.transaction);

      console.log('   Transaction has', signedTx.signatures.length, 'signatures after user signs');
      console.log('   Signatures:', signedTx.signatures.map(s => s.signature ? 'Present' : 'Missing'));

      // Check if transaction is properly signed
      const allSigned = signedTx.signatures.every(s => s.signature !== null);
      if (!allSigned) {
        throw new Error('Transaction not fully signed - missing signatures');
      }

      console.log('📡 Sending transaction to Solana devnet...');
      const signature = await connection.sendRawTransaction(signedTx.serialize(), {
        skipPreflight: false, // Run simulation first
        preflightCommitment: 'confirmed',
      });

      console.log('✅ Transaction sent! Signature:', signature);
      console.log('   View on explorer: https://explorer.solana.com/tx/' + signature + '?cluster=devnet');

      toast({
        type: 'info',
        title: 'Transaction Sent',
        message: 'Executing on Solana devnet... Position will update in ~2 seconds.'
      });

      await connection.confirmTransaction(signature, 'confirmed');
      console.log('✅ Trade transaction confirmed:', signature);

      // Notify server that trade was executed
      await fetch(`${API_URL}/api/trade/commit`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          user: publicKey.toBase58(),
          holdId: reservedOrder.holdId,
          signature: signature,
        })
      });

      // Record the fill on the server
      await fetch(`${API_URL}/api/trade/record-fill`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          user: publicKey.toBase58(),
          side: reservedOrder.side,
          price: reservedOrder.price,
          quantity: reservedOrder.size,
          signature: signature,
        })
      });

      const positionChange = reservedOrder.side === 'buy' ? '+' : '-';
      toast({
        type: 'success',
        title: '✅ Trade Executed On-Chain!',
        message: `Position ${positionChange}${reservedOrder.size} ${coinToV2(coin)} @ $${reservedOrder.price.toFixed(2)}. Your collateral stays in vault.`
      });

      // Refresh balance to show new position
      await fetchUserBalance();

      // Reset form
      setReservedOrder(null);
      setSize('');
      setPrice(currentPrice > 0 ? currentPrice.toFixed(2) : '');
      if (timerRef.current) clearInterval(timerRef.current);

    } catch (error: any) {
      console.error('Failed to execute trade:', error);
      toast({
        type: 'error',
        title: 'Trade Failed',
        message: `Failed to execute: ${error.message || error.toString()}`
      });
    } finally {
      setLoading(false);
    }
  };

  // Cancel reservation
  const handleCancel = () => {
    setReservedOrder(null);
    if (timerRef.current) clearInterval(timerRef.current);
    toast({
      type: 'info',
      title: 'Reservation Cancelled',
      message: 'Reservation cancelled'
    });
  };

  const totalValue = orderType === 'limit' 
    ? (parseFloat(price) * parseFloat(size || '0')).toFixed(2)
    : (currentPrice * parseFloat(size || '0')).toFixed(2);

  const coinV2 = coinToV2(coin);

  return (
    <div className="bg-zinc-900/50 rounded-xl p-3 border border-white/5">
      {/* Account Status Banner */}
      {publicKey && userBalance && (
        <div className="mb-3 p-3 bg-zinc-950 rounded-lg border border-white/5">
          <div className="flex items-center justify-between mb-2">
            <div className="flex items-center gap-2">
              <Wallet className="w-4 h-4 text-blue-400" />
              <span className="text-xs font-bold text-zinc-400">Your Account</span>
            </div>
            {balanceLoading && (
              <span className="text-[10px] text-zinc-600">Updating...</span>
            )}
          </div>

          {/* Collateral Balance */}
          <div className="space-y-2">
            <div className="flex justify-between items-center">
              <span className="text-[10px] text-zinc-500">Collateral in Vault</span>
              <span className="text-sm font-mono font-bold text-emerald-400">
                {userBalance.balance.toFixed(4)} SOL
              </span>
            </div>

            {/* Current Position */}
            {userBalance.position && parseFloat(userBalance.position.size) !== 0 && (
              <div className="pt-2 border-t border-white/5">
                <div className="flex justify-between items-center">
                  <span className="text-[10px] text-zinc-500">Open Position</span>
                  <div className="flex items-center gap-2">
                    {userBalance.position.side === 'long' ? (
                      <TrendingUp className="w-3 h-3 text-emerald-400" />
                    ) : (
                      <TrendingDown className="w-3 h-3 text-red-400" />
                    )}
                    <span className={`text-sm font-mono font-bold ${
                      userBalance.position.side === 'long' ? 'text-emerald-400' : 'text-red-400'
                    }`}>
                      {userBalance.position.side === 'long' ? 'LONG' : 'SHORT'} {Math.abs(parseFloat(userBalance.position.size)).toFixed(4)} SOL
                    </span>
                  </div>
                </div>
                <div className="grid grid-cols-2 gap-2 mt-2">
                  <div>
                    <span className="text-[10px] text-zinc-600">Entry</span>
                    <p className="text-xs font-mono text-zinc-400">${userBalance.position.entryPrice}</p>
                  </div>
                  <div>
                    <span className="text-[10px] text-zinc-600">Mark</span>
                    <p className="text-xs font-mono text-zinc-300">${userBalance.position.markPrice}</p>
                  </div>
                </div>
                {/* P&L Display */}
                {(() => {
                  const entry = parseFloat(userBalance.position.entryPrice);
                  const mark = parseFloat(userBalance.position.markPrice);
                  const size = parseFloat(userBalance.position.size);
                  const isLong = userBalance.position.side === 'long';
                  const pnl = isLong ? (mark - entry) * size : (entry - mark) * size;
                  const pnlPct = entry > 0 ? (pnl / (entry * size)) * 100 : 0;
                  return (
                    <div className="flex justify-between items-center mt-2 pt-2 border-t border-white/5">
                      <span className="text-[10px] text-zinc-500">Unrealized P&L</span>
                      <span className={`text-xs font-mono font-bold ${pnl >= 0 ? 'text-emerald-400' : 'text-red-400'}`}>
                        {pnl >= 0 ? '+' : ''}{pnl.toFixed(2)} USD ({pnlPct >= 0 ? '+' : ''}{pnlPct.toFixed(1)}%)
                      </span>
                    </div>
                  );
                })()}
              </div>
            )}

            {/* No Position Message */}
            {(!userBalance.position || parseFloat(userBalance.position.size) === 0) && (
              <div className="pt-2 border-t border-white/5">
                <span className="text-[10px] text-zinc-600">No open position</span>
              </div>
            )}
          </div>

          {/* Info about how it works */}
          <div className="mt-2 pt-2 border-t border-white/5">
            <p className="text-[9px] text-zinc-600 leading-relaxed">
              💡 Your collateral stays in the Percolator vault. Trading changes your position, not your wallet balance.
            </p>
          </div>
        </div>
      )}

      {/* Deposit Collateral Section */}
      <div className="mb-3">
        <button
          type="button"
          onClick={() => setShowDeposit(!showDeposit)}
          className="w-full flex items-center justify-between px-3 py-2 bg-blue-500/10 hover:bg-blue-500/20 border border-blue-500/20 rounded-lg text-xs text-blue-400 transition-colors"
        >
          <span className="font-medium">Deposit Collateral</span>
          <span className="text-[10px]">{showDeposit ? '▲' : '▼'}</span>
        </button>

        {showDeposit && (
          <div className="mt-2 p-3 bg-zinc-950 rounded-lg border border-white/5 space-y-3">
            <div className="p-2 bg-blue-500/5 rounded border border-blue-500/10">
              <p className="text-[10px] text-blue-400 leading-relaxed">
                💡 <strong>How it works:</strong> Your SOL goes into the Percolator vault as collateral. You can then open positions (long/short) using this collateral. The money stays in the vault until you withdraw.
              </p>
            </div>
            <p className="text-[10px] text-zinc-500">
              Deposit SOL as collateral to trade larger positions. At 10x max leverage, 0.5 SOL lets you trade ~5 SOL notional.
            </p>
            <div className="relative">
              <label className="absolute left-3 top-1/2 -translate-y-1/2 text-xs text-zinc-500 pointer-events-none">Amount</label>
              <input
                type="number"
                step="0.1"
                min="0.01"
                value={depositAmount}
                onChange={(e) => setDepositAmount(e.target.value)}
                placeholder="0.5"
                className="w-full pl-16 pr-12 py-2 bg-zinc-900 border border-white/5 rounded-lg text-sm text-right text-zinc-100 placeholder-zinc-700 focus:outline-none focus:border-blue-500/50 focus:ring-1 focus:ring-blue-500/20 transition-all"
              />
              <span className="absolute right-3 top-1/2 -translate-y-1/2 text-xs text-zinc-600 pointer-events-none">SOL</span>
            </div>
            <div className="flex gap-1">
              {[0.1, 0.5, 1, 2].map((amt) => (
                <button
                  key={amt}
                  type="button"
                  onClick={() => setDepositAmount(amt.toString())}
                  className={`flex-1 py-1 text-[10px] font-medium rounded transition-colors ${
                    depositAmount === amt.toString()
                      ? 'bg-blue-500/20 text-blue-400'
                      : 'text-zinc-500 hover:text-zinc-300 hover:bg-white/5'
                  }`}
                >
                  {amt} SOL
                </button>
              ))}
            </div>
            <button
              type="button"
              onClick={handleDeposit}
              disabled={depositLoading || !publicKey}
              className="w-full py-2 bg-blue-500 hover:bg-blue-600 text-white font-bold text-xs rounded-lg transition-all disabled:opacity-50 disabled:cursor-not-allowed"
            >
              {depositLoading ? 'Depositing...' : !publicKey ? 'Connect Wallet' : `Deposit ${depositAmount} SOL`}
            </button>
          </div>
        )}
      </div>

      {/* Withdraw Collateral Section */}
      {userBalance && userBalance.hasAccount && userBalance.balance > 0 && (
        <div className="mb-3">
          <button
            type="button"
            onClick={() => setShowWithdraw(!showWithdraw)}
            className="w-full flex items-center justify-between px-3 py-2 bg-amber-500/10 hover:bg-amber-500/20 border border-amber-500/20 rounded-lg text-xs text-amber-400 transition-colors"
          >
            <span className="font-medium">Withdraw Collateral</span>
            <span className="text-[10px]">{showWithdraw ? '▲' : '▼'}</span>
          </button>

          {showWithdraw && (
            <div className="mt-2 p-3 bg-zinc-950 rounded-lg border border-white/5 space-y-3">
              {userBalance.position && parseFloat(userBalance.position.size) !== 0 ? (
                <div className="p-2 bg-red-500/10 rounded border border-red-500/20">
                  <p className="text-[10px] text-red-400 leading-relaxed">
                    ⚠️ <strong>Close position first:</strong> You have an open position. Close it before withdrawing collateral.
                  </p>
                </div>
              ) : (
                <>
                  <p className="text-[10px] text-zinc-500">
                    Withdraw your collateral back to your wallet. Available: {userBalance.balance.toFixed(4)} SOL
                  </p>
                  <div className="relative">
                    <label className="absolute left-3 top-1/2 -translate-y-1/2 text-xs text-zinc-500 pointer-events-none">Amount</label>
                    <input
                      type="number"
                      step="0.01"
                      min="0.01"
                      max={userBalance.balance}
                      value={withdrawAmount}
                      onChange={(e) => setWithdrawAmount(e.target.value)}
                      placeholder={userBalance.balance.toFixed(4)}
                      className="w-full pl-16 pr-12 py-2 bg-zinc-900 border border-white/5 rounded-lg text-sm text-right text-zinc-100 placeholder-zinc-700 focus:outline-none focus:border-amber-500/50 focus:ring-1 focus:ring-amber-500/20 transition-all"
                    />
                    <span className="absolute right-3 top-1/2 -translate-y-1/2 text-xs text-zinc-600 pointer-events-none">SOL</span>
                  </div>
                  <div className="flex gap-1">
                    <button
                      type="button"
                      onClick={() => setWithdrawAmount((userBalance.balance * 0.25).toFixed(4))}
                      className="flex-1 py-1 text-[10px] font-medium rounded transition-colors text-zinc-500 hover:text-zinc-300 hover:bg-white/5"
                    >
                      25%
                    </button>
                    <button
                      type="button"
                      onClick={() => setWithdrawAmount((userBalance.balance * 0.5).toFixed(4))}
                      className="flex-1 py-1 text-[10px] font-medium rounded transition-colors text-zinc-500 hover:text-zinc-300 hover:bg-white/5"
                    >
                      50%
                    </button>
                    <button
                      type="button"
                      onClick={() => setWithdrawAmount((userBalance.balance * 0.75).toFixed(4))}
                      className="flex-1 py-1 text-[10px] font-medium rounded transition-colors text-zinc-500 hover:text-zinc-300 hover:bg-white/5"
                    >
                      75%
                    </button>
                    <button
                      type="button"
                      onClick={() => setWithdrawAmount(userBalance.balance.toFixed(4))}
                      className="flex-1 py-1 text-[10px] font-medium rounded transition-colors bg-amber-500/20 text-amber-400"
                    >
                      Max
                    </button>
                  </div>
                  <button
                    type="button"
                    onClick={handleWithdraw}
                    disabled={withdrawLoading || !publicKey || !withdrawAmount || parseFloat(withdrawAmount) <= 0}
                    className="w-full py-2 bg-amber-500 hover:bg-amber-600 text-white font-bold text-xs rounded-lg transition-all disabled:opacity-50 disabled:cursor-not-allowed"
                  >
                    {withdrawLoading ? 'Withdrawing...' : `Withdraw ${withdrawAmount || '0'} SOL`}
                  </button>
                </>
              )}
            </div>
          )}
        </div>
      )}

      {!reservedOrder ? (
        <form onSubmit={handleReserve} className="space-y-3">
          {/* Order Side */}
          <div className="flex gap-1 p-1 bg-zinc-950 rounded-lg border border-white/5">
            <button
              type="button"
              onClick={() => setOrderSide('buy')}
              className={`flex-1 py-1.5 text-xs font-medium rounded-md transition-all ${
                orderSide === 'buy'
                  ? 'bg-emerald-500/20 text-emerald-400 shadow-sm border border-emerald-500/20'
                  : 'text-zinc-500 hover:text-zinc-300 hover:bg-white/5'
              }`}
            >
              Buy
            </button>
            <button
              type="button"
              onClick={() => setOrderSide('sell')}
              className={`flex-1 py-1.5 text-xs font-medium rounded-md transition-all ${
                orderSide === 'sell'
                  ? 'bg-red-500/20 text-red-400 shadow-sm border border-red-500/20'
                  : 'text-zinc-500 hover:text-zinc-300 hover:bg-white/5'
              }`}
            >
              Sell
            </button>
          </div>

          {/* Order Type */}
          <div className="flex gap-2">
            <button
              type="button"
              onClick={() => setOrderType('limit')}
              className={`flex-1 text-[10px] font-medium uppercase tracking-wider border-b-2 pb-1 transition-colors ${
                orderType === 'limit'
                  ? 'border-blue-500 text-blue-400'
                  : 'border-transparent text-zinc-500 hover:text-zinc-300'
              }`}
            >
              Limit
            </button>
            <button
              type="button"
              onClick={() => setOrderType('market')}
              className={`flex-1 text-[10px] font-medium uppercase tracking-wider border-b-2 pb-1 transition-colors ${
                orderType === 'market'
                  ? 'border-blue-500 text-blue-400'
                  : 'border-transparent text-zinc-500 hover:text-zinc-300'
              }`}
            >
              Market
            </button>
          </div>

          <div className="space-y-2">
            {/* Price Input (for limit orders) */}
            {orderType === 'limit' && (
              <div className="relative">
                <label className="absolute left-3 top-1/2 -translate-y-1/2 text-xs text-zinc-500 pointer-events-none">Price</label>
                <input
                  type="number"
                  step="0.01"
                  value={price}
                  onChange={(e) => setPrice(e.target.value)}
                  placeholder={currentPrice > 0 ? currentPrice.toFixed(2) : '0.00'}
                  className="w-full pl-12 pr-12 py-2 bg-zinc-950 border border-white/5 rounded-lg text-sm text-right text-zinc-100 placeholder-zinc-700 focus:outline-none focus:border-blue-500/50 focus:ring-1 focus:ring-blue-500/20 transition-all"
                  required
                />
                <span className="absolute right-3 top-1/2 -translate-y-1/2 text-xs text-zinc-600 pointer-events-none">USDC</span>
              </div>
            )}

            {/* Size Input */}
            <div className="relative">
              <label className="absolute left-3 top-1/2 -translate-y-1/2 text-xs text-zinc-500 pointer-events-none">Size</label>
              <input
                type="number"
                step="0.001"
                min="0"
                value={size}
                onChange={(e) => setSize(e.target.value)}
                placeholder="0.000"
                className="w-full pl-12 pr-12 py-2 bg-zinc-950 border border-white/5 rounded-lg text-sm text-right text-zinc-100 placeholder-zinc-700 focus:outline-none focus:border-blue-500/50 focus:ring-1 focus:ring-blue-500/20 transition-all"
                required
              />
              <span className="absolute right-3 top-1/2 -translate-y-1/2 text-xs text-zinc-600 pointer-events-none">{coinV2}</span>
            </div>

            {/* Quick Size Presets */}
            <div className="flex gap-1">
              {[25, 50, 75, 100].map((percent) => (
                <button
                  key={percent}
                  type="button"
                  onClick={() => {
                    // Calculate size based on available balance (mock for now)
                    const mockBalance = 1000; // USDC
                    const effectivePrice = orderType === 'limit' ? parseFloat(price) || currentPrice : currentPrice;
                    if (effectivePrice > 0) {
                      const maxSize = mockBalance / effectivePrice;
                      setSize((maxSize * percent / 100).toFixed(4));
                    }
                  }}
                  className="flex-1 py-1 text-[10px] font-medium text-zinc-500 hover:text-zinc-300 hover:bg-white/5 rounded transition-colors"
                >
                  {percent}%
                </button>
              ))}
            </div>
          </div>

          {/* Total Value & Fees */}
          <div className="space-y-1 px-1">
            <div className="flex justify-between items-center">
              <span className="text-xs text-zinc-500">Total Value</span>
              <span className="text-xs font-mono text-zinc-300">{totalValue} USDC</span>
            </div>
            <div className="flex justify-between items-center">
              <span className="text-[10px] text-zinc-600">Est. Fee (0.1%)</span>
              <span className="text-[10px] font-mono text-zinc-500">{(parseFloat(totalValue) * 0.001).toFixed(4)} USDC</span>
            </div>
          </div>

          {/* Reserve Button */}
          <button
            type="submit"
            disabled={loading || !publicKey || !size}
            className={`w-full py-2.5 text-xs font-bold uppercase tracking-wide rounded-lg transition-all disabled:opacity-50 disabled:cursor-not-allowed ${
              orderSide === 'buy'
                ? 'bg-emerald-500 hover:bg-emerald-600 text-white shadow-lg shadow-emerald-500/20'
                : 'bg-red-500 hover:bg-red-600 text-white shadow-lg shadow-red-500/20'
            }`}
          >
            {loading ? 'Processing...' : !publicKey ? 'Connect Wallet' : `Reserve ${orderSide === 'buy' ? 'Buy' : 'Sell'}`}
          </button>
        </form>
      ) : (
        <div className="space-y-3">
          {/* Reservation Info Banner */}
          <div className="bg-amber-500/10 border border-amber-500/20 rounded-lg p-3">
            <div className="flex items-start gap-3">
              <AlertCircle className="w-4 h-4 text-amber-500 shrink-0 mt-0.5" />
              <div>
                <p className="text-xs font-bold text-amber-500 mb-0.5">Order Reserved</p>
                <p className="text-[10px] text-amber-400/80">Price locked. Commit within 30 seconds.</p>
              </div>
            </div>
          </div>

          {/* Countdown Timer */}
          <div className="bg-zinc-950 rounded-lg p-3 border border-blue-500/20 relative overflow-hidden">
            <div className="absolute top-0 left-0 h-0.5 bg-blue-500/50 transition-all duration-1000" style={{ width: `${(timeRemaining / 30000) * 100}%` }} />
            
            <div className="flex items-center justify-between mb-3">
              <span className="text-xs text-zinc-400">Time Remaining</span>
              <span className={`text-lg font-mono font-bold ${
                timeRemaining < 10000 ? 'text-red-400' : 'text-blue-400'
              }`}>
                {(timeRemaining / 1000).toFixed(1)}s
              </span>
            </div>
            
            {/* Order Details */}
            <div className="space-y-1.5">
              <div className="flex justify-between text-xs">
                <span className="text-zinc-500">Side</span>
                <span className={`font-bold ${reservedOrder.side === 'buy' ? 'text-emerald-400' : 'text-red-400'}`}>
                  {reservedOrder.side.toUpperCase()}
                </span>
              </div>
              <div className="flex justify-between text-xs">
                <span className="text-zinc-500">Price</span>
                <span className="text-zinc-200 font-mono">${reservedOrder.price.toFixed(2)}</span>
              </div>
              <div className="flex justify-between text-xs">
                <span className="text-zinc-500">Size</span>
                <span className="text-zinc-200 font-mono">{reservedOrder.size} {coinV2}</span>
              </div>
              <div className="flex justify-between border-t border-white/5 pt-2 text-xs">
                <span className="text-zinc-500">Total</span>
                <span className="text-zinc-100 font-mono font-bold">${(reservedOrder.price * reservedOrder.size).toFixed(2)} USDC</span>
              </div>
            </div>
          </div>

          {/* Commit & Cancel Buttons */}
          <div className="grid grid-cols-2 gap-2">
            <button
              onClick={handleCommit}
              disabled={loading}
              className="py-2 bg-emerald-500 hover:bg-emerald-600 text-white font-bold text-xs rounded-lg transition-all disabled:opacity-50 shadow-lg shadow-emerald-500/20"
            >
              {loading ? 'Committing...' : 'Commit'}
            </button>
            <button
              onClick={handleCancel}
              disabled={loading}
              className="py-2 bg-zinc-800 hover:bg-zinc-700 text-zinc-400 font-medium text-xs rounded-lg transition-colors disabled:opacity-50"
            >
              Cancel
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
