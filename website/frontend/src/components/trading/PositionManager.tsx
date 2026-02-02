"use client"

import { useState, useEffect } from 'react';
import { useWallet } from '@solana/wallet-adapter-react';
import { TrendingUp, TrendingDown, AlertTriangle, RefreshCw } from 'lucide-react';

interface Position {
  symbol: string;
  side: 'long' | 'short';
  size: number;
  entryPrice: number;
  markPrice: number;
  leverage: number;
  liquidationPrice?: number;
  margin: number;
  pnl: number;
  pnlPercent: number;
  accountIndex?: number;
  warmupStartedAt?: string;
}

interface PositionManagerProps {
  prices?: Record<string, number>;
  currentPrice?: number;
}

export function PositionManager({ prices, currentPrice }: PositionManagerProps) {
  const { publicKey } = useWallet();
  const [positions, setPositions] = useState<Position[]>([]);
  const [balance, setBalance] = useState<{
    total: number;
    available: number;
    marginUsage: number;
    hasAccount: boolean;
  }>({ total: 0, available: 0, marginUsage: 0, hasAccount: false });
  const [totalPnl, setTotalPnl] = useState(0);
  const [totalMargin, setTotalMargin] = useState(0);
  const [loading, setLoading] = useState(true);
  const [lastUpdate, setLastUpdate] = useState<Date | null>(null);

  const fetchData = async () => {
    if (!publicKey) {
      setPositions([]);
      setBalance({ total: 0, available: 0, marginUsage: 0, hasAccount: false });
      setLoading(false);
      return;
    }

    try {
      const API_URL = process.env.NEXT_PUBLIC_API_URL || 'http://localhost:5001';

      // Fetch balance and positions in parallel
      const [balanceRes, positionsRes] = await Promise.all([
        fetch(`${API_URL}/api/user/balance?user=${publicKey.toBase58()}`),
        fetch(`${API_URL}/api/user/positions?user=${publicKey.toBase58()}`),
      ]);

      if (balanceRes.ok) {
        const balanceData = await balanceRes.json();
        setBalance({
          total: balanceData.balance || 0,
          available: balanceData.available || 0,
          marginUsage: balanceData.marginUsage || 0,
          hasAccount: balanceData.hasAccount || false,
        });
      }

      if (positionsRes.ok) {
        const positionsData = await positionsRes.json();
        if (positionsData.positions && positionsData.positions.length > 0) {
          const formattedPositions = positionsData.positions.map((pos: any) => {
            // Use currentPrice from props or prices map for real-time P&L
            const symbol = pos.symbol || 'SOL-PERP';
            const priceKey = symbol.replace('-PERP', '');
            const markPrice = currentPrice || prices?.[priceKey] || pos.mark_price || pos.entry_price;
            const entryPrice = pos.entry_price;
            const size = pos.qty;
            const isLong = pos.side === 'long';

            // Calculate P&L with real-time price
            const priceDiff = isLong ? markPrice - entryPrice : entryPrice - markPrice;
            const pnl = priceDiff * size;
            const notional = size * entryPrice;
            const pnlPercent = notional > 0 ? (pnl / notional) * 100 * (pos.leverage || 10) : 0;

            // Calculate liquidation price (approximate)
            const maintenanceMarginBps = 500; // 5%
            const maintenanceMargin = (notional * maintenanceMarginBps) / 10000;
            const liquidationPrice = isLong
              ? entryPrice * (1 - 1 / (pos.leverage || 10) + maintenanceMarginBps / 10000)
              : entryPrice * (1 + 1 / (pos.leverage || 10) - maintenanceMarginBps / 10000);

            return {
              symbol: symbol,
              side: pos.side,
              size: size,
              entryPrice: entryPrice,
              markPrice: markPrice,
              leverage: pos.leverage || 10,
              margin: pos.margin || notional / (pos.leverage || 10),
              pnl: pnl,
              pnlPercent: pnlPercent,
              liquidationPrice: liquidationPrice,
              accountIndex: pos.accountIndex,
              warmupStartedAt: pos.warmupStartedAt,
            };
          });
          setPositions(formattedPositions);
        } else {
          setPositions([]);
        }
      }

      setLastUpdate(new Date());
    } catch (error) {
      console.error('Failed to fetch position data:', error);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchData();
    const interval = setInterval(fetchData, 5000); // Refresh every 5 seconds
    return () => clearInterval(interval);
  }, [publicKey]);

  // Recalculate P&L when price updates
  useEffect(() => {
    if (positions.length > 0 && currentPrice) {
      setPositions(prev => prev.map(pos => {
        const priceDiff = pos.side === 'long'
          ? currentPrice - pos.entryPrice
          : pos.entryPrice - currentPrice;
        const pnl = priceDiff * pos.size;
        const notional = pos.size * pos.entryPrice;
        const pnlPercent = notional > 0 ? (pnl / notional) * 100 * pos.leverage : 0;
        return { ...pos, markPrice: currentPrice, pnl, pnlPercent };
      }));
    }
  }, [currentPrice]);

  // Calculate totals
  useEffect(() => {
    const pnl = positions.reduce((sum, pos) => sum + pos.pnl, 0);
    const margin = positions.reduce((sum, pos) => sum + pos.margin, 0);
    setTotalPnl(pnl);
    setTotalMargin(margin);
  }, [positions]);

  if (!publicKey) {
    return (
      <div className="bg-zinc-900/50 rounded-xl p-4 border border-white/5">
        <h3 className="text-sm font-semibold text-zinc-300 mb-3">Positions</h3>
        <p className="text-xs text-zinc-500 text-center py-4">Connect wallet to view positions</p>
      </div>
    );
  }

  if (loading) {
    return (
      <div className="bg-zinc-900/50 rounded-xl p-4 border border-white/5">
        <h3 className="text-sm font-semibold text-zinc-300 mb-3">Positions</h3>
        <div className="animate-pulse space-y-2">
          <div className="h-12 bg-zinc-800 rounded"></div>
          <div className="h-12 bg-zinc-800 rounded"></div>
        </div>
      </div>
    );
  }

  return (
    <div className="bg-zinc-900/50 rounded-xl p-4 border border-white/5">
      {/* Header */}
      <div className="flex items-center justify-between mb-4">
        <div className="flex items-center gap-2">
          <h3 className="text-sm font-semibold text-zinc-300">Positions</h3>
          <button
            onClick={fetchData}
            className="p-1 hover:bg-white/5 rounded transition-colors"
            title="Refresh"
          >
            <RefreshCw className="w-3 h-3 text-zinc-500 hover:text-zinc-300" />
          </button>
        </div>
        {lastUpdate && (
          <span className="text-[10px] text-zinc-600">
            {lastUpdate.toLocaleTimeString()}
          </span>
        )}
      </div>

      {/* Account Summary */}
      {balance.hasAccount && (
        <div className="grid grid-cols-3 gap-2 mb-4 p-3 bg-zinc-950/50 rounded-lg border border-white/5">
          <div>
            <p className="text-[10px] text-zinc-500 uppercase">Balance</p>
            <p className="text-sm font-mono text-zinc-200">{balance.total.toFixed(4)} SOL</p>
          </div>
          <div>
            <p className="text-[10px] text-zinc-500 uppercase">Available</p>
            <p className="text-sm font-mono text-zinc-300">{balance.available.toFixed(4)} SOL</p>
          </div>
          <div>
            <p className="text-[10px] text-zinc-500 uppercase">Margin</p>
            <p className={`text-sm font-mono ${balance.marginUsage > 80 ? 'text-red-400' : balance.marginUsage > 50 ? 'text-amber-400' : 'text-emerald-400'}`}>
              {balance.marginUsage.toFixed(1)}%
            </p>
          </div>
        </div>
      )}

      {/* P&L Summary (if positions exist) */}
      {positions.length > 0 && (
        <div className="flex items-center justify-between mb-4">
          <div className="text-right">
            <p className="text-[10px] text-zinc-500 uppercase">Total P&L</p>
            <p className={`text-sm font-bold ${totalPnl >= 0 ? 'text-emerald-400' : 'text-red-400'}`}>
              {totalPnl >= 0 ? '+' : '-'}${Math.abs(totalPnl).toFixed(2)}
            </p>
          </div>
          <div className="text-right">
            <p className="text-[10px] text-zinc-500 uppercase">Margin Used</p>
            <p className="text-sm font-mono text-zinc-300">${totalMargin.toFixed(2)}</p>
          </div>
        </div>
      )}

      {/* No Account Message */}
      {!balance.hasAccount && (
        <div className="text-center py-6">
          <p className="text-xs text-zinc-500 mb-2">No account found</p>
          <p className="text-[10px] text-zinc-600">Deposit collateral to start trading</p>
        </div>
      )}

      {/* Positions List */}
      {balance.hasAccount && positions.length === 0 && (
        <p className="text-xs text-zinc-500 text-center py-4">No open positions</p>
      )}

      {positions.length > 0 && (
        <div className="space-y-2">
          {positions.map((position, index) => (
            <div
              key={index}
              className="bg-zinc-950/50 rounded-lg p-3 border border-white/5 hover:border-white/10 transition-colors"
            >
              <div className="flex items-center justify-between mb-2">
                <div className="flex items-center gap-2">
                  <span className="text-sm font-bold text-zinc-200">{position.symbol}</span>
                  <span className={`text-[10px] font-bold px-1.5 py-0.5 rounded ${
                    position.side === 'long'
                      ? 'bg-emerald-500/20 text-emerald-400'
                      : 'bg-red-500/20 text-red-400'
                  }`}>
                    {position.side.toUpperCase()} {position.leverage}x
                  </span>
                </div>
                <div className={`flex items-center gap-1 ${
                  position.pnl >= 0 ? 'text-emerald-400' : 'text-red-400'
                }`}>
                  {position.pnl >= 0 ? (
                    <TrendingUp className="w-3 h-3" />
                  ) : (
                    <TrendingDown className="w-3 h-3" />
                  )}
                  <span className="text-sm font-bold">
                    {position.pnl >= 0 ? '+' : '-'}${Math.abs(position.pnl).toFixed(2)}
                  </span>
                  <span className="text-[10px]">
                    ({position.pnlPercent >= 0 ? '+' : ''}{position.pnlPercent.toFixed(2)}%)
                  </span>
                </div>
              </div>

              <div className="grid grid-cols-4 gap-2 text-[10px]">
                <div>
                  <p className="text-zinc-500">Size</p>
                  <p className="text-zinc-300 font-mono">{position.size.toFixed(3)} SOL</p>
                </div>
                <div>
                  <p className="text-zinc-500">Entry</p>
                  <p className="text-zinc-300 font-mono">${position.entryPrice.toFixed(2)}</p>
                </div>
                <div>
                  <p className="text-zinc-500">Mark</p>
                  <p className="text-zinc-300 font-mono">${position.markPrice.toFixed(2)}</p>
                </div>
                <div>
                  <p className="text-zinc-500 flex items-center gap-0.5">
                    Liq. <AlertTriangle className="w-2 h-2 text-amber-500" />
                  </p>
                  <p className="text-amber-400 font-mono">
                    ${position.liquidationPrice?.toFixed(2) || '-'}
                  </p>
                </div>
              </div>

              {/* Liquidation warning bar */}
              {position.liquidationPrice &&
               Math.abs(position.markPrice - position.liquidationPrice) / position.markPrice < 0.15 && (
                <div className="mt-2 bg-amber-500/10 border border-amber-500/20 rounded px-2 py-1">
                  <p className="text-[10px] text-amber-400 flex items-center gap-1">
                    <AlertTriangle className="w-3 h-3" />
                    Position approaching liquidation price
                  </p>
                </div>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
