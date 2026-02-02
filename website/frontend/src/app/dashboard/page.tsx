"use client"

import { useState, useRef, useEffect } from 'react';
import TradingChart from '@/components/charts/TradingChart';
import { OrderForm } from '@/components/trading/OrderForm';
import { OrderBook } from '@/components/trading/OrderBook';
import { PastTrades } from '@/components/trading/PastTrades';
import { PositionManager } from '@/components/trading/PositionManager';
import { TestnetBanner } from '@/components/TestnetBanner';
import { StatusFooter } from '@/components/StatusFooter';
import { AMMInterface } from '@/components/trading/AMMInterface';
import { WalletMultiButton } from '@solana/wallet-adapter-react-ui';
import { useWallet } from '@solana/wallet-adapter-react';
import { Home, Terminal, Wallet, Zap } from 'lucide-react';
import Link from 'next/link';
import { type Coin } from '@/lib/hyperliquid-client';
import { cn } from '@/lib/utils';
import { GridPattern } from '@/components/ui/grid-pattern';

export default function DashboardPage() {
  const { publicKey } = useWallet();
  const [selectedCoin, setSelectedCoin] = useState<Coin>('SOL');
  const [currentPrice, setCurrentPrice] = useState<number>(0);
  const [rightPanelWidth, setRightPanelWidth] = useState(420);
  const [isResizing, setIsResizing] = useState(false);
  const [isMounted, setIsMounted] = useState(false);
  const [tradingMode, setTradingMode] = useState<'orderbook' | 'amm'>('orderbook');
  const containerRef = useRef<HTMLDivElement>(null);

  // Handle client-side mounting
  useEffect(() => {
    setIsMounted(true);
  }, []);

  // Handle resize
  useEffect(() => {
    const handleMouseMove = (e: MouseEvent) => {
      if (!isResizing || !containerRef.current) return;
      
      const containerRect = containerRef.current.getBoundingClientRect();
      const newWidth = containerRect.right - e.clientX;
      
      const clampedWidth = Math.max(280, Math.min(700, newWidth));
      setRightPanelWidth(clampedWidth);
    };

    const handleMouseUp = () => {
      setIsResizing(false);
    };

    if (isResizing) {
      document.addEventListener('mousemove', handleMouseMove);
      document.addEventListener('mouseup', handleMouseUp);
      document.body.style.cursor = 'col-resize';
      document.body.style.userSelect = 'none';
    }
  
    return () => {
      document.removeEventListener('mousemove', handleMouseMove);
      document.removeEventListener('mouseup', handleMouseUp);
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
    };
  }, [isResizing]);

  if (!isMounted) {
    return (
      <div className="min-h-screen bg-zinc-950 text-white flex items-center justify-center">
        <div className="flex items-center gap-2">
          <div className="w-4 h-4 border-2 border-blue-500 border-t-transparent rounded-full animate-spin" />
          <span className="text-zinc-400 text-sm font-medium">Initializing Dashboard...</span>
        </div>
      </div>
    );
  }

  const getCoinString = (coin: Coin): "ethereum" | "bitcoin" | "solana" | "jupiter" | "bonk" | "dogwifhat" => {
    switch(coin) {
      case 'ETH': return 'ethereum';
      case 'BTC': return 'bitcoin';
      case 'SOL': return 'solana';
      case 'JUP': return 'jupiter';
      case 'BONK': return 'bonk';
      case 'WIF': return 'dogwifhat';
    }
  };

  const getSymbol = (coin: Coin): string => {
    switch(coin) {
      case 'ETH': return 'ETH-USDC';
      case 'BTC': return 'BTC-USDC';
      case 'SOL': return 'SOL-USDC';
      case 'JUP': return 'JUP-USDC';
      case 'BONK': return 'BONK-USDC';
      case 'WIF': return 'WIF-USDC';
    }
  };

  return (
    <div className="h-screen bg-zinc-950 text-zinc-100 flex flex-col overflow-hidden relative">
      {/* Background Patterns */}
      <div className="absolute inset-0 z-0 overflow-hidden pointer-events-none">
        <GridPattern 
            width={40} 
            height={40} 
            x={-1} 
            y={-1} 
            className={cn("[mask-image:linear-gradient(to_bottom_right,white,transparent,transparent)] opacity-20")} 
        />
        <div className="absolute inset-0 bg-gradient-to-b from-blue-500/5 via-transparent to-purple-500/5" />
      </div>

      {/* Testnet Warning Banner */}
      <div className="relative z-50">
        <TestnetBanner />
      </div>

      {/* Header */}
      <header className="border-b border-white/5 bg-zinc-950/60 backdrop-blur-xl shrink-0 z-50 relative">
        <div className="px-4 py-3">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-6">
              <Link href="/" className="group flex items-center gap-3">
                <div className="w-8 h-8 rounded-xl bg-gradient-to-br from-blue-500 to-violet-600 flex items-center justify-center shadow-lg shadow-blue-500/20">
                  <Zap className="w-5 h-5 text-white fill-white" />
                </div>
                <div className="flex flex-col">
                  <h1 className="text-lg font-bold tracking-tight leading-none">
                    <span className="bg-gradient-to-r from-blue-400 to-violet-400 bg-clip-text text-transparent">Percolator</span>
                  </h1>
                  <span className="text-[10px] font-medium text-zinc-500 tracking-widest uppercase">Pro Terminal</span>
                </div>
              </Link>

              <div className="h-6 w-[1px] bg-white/10 hidden sm:block" />
              
              <div className="hidden sm:flex items-center gap-1">
                 <Link href="/" className="px-3 py-1.5 text-sm font-medium text-zinc-400 hover:text-white hover:bg-white/5 rounded-lg transition-colors">
                    Home
                 </Link>
                 <Link href="/portfolio" className="px-3 py-1.5 text-sm font-medium text-zinc-400 hover:text-white hover:bg-white/5 rounded-lg transition-colors">
                    Portfolio
                 </Link>
                 <Link href="/cli" className="px-3 py-1.5 text-sm font-medium text-zinc-400 hover:text-white hover:bg-white/5 rounded-lg transition-colors">
                    CLI
                 </Link>
              </div>
            </div>

            <div className="flex items-center gap-4">
               <div className="hidden md:flex items-center gap-2 px-3 py-1.5 bg-blue-500/10 border border-blue-500/20 rounded-full">
                  <div className="w-2 h-2 rounded-full bg-blue-400 animate-pulse" />
                  <span className="text-xs font-medium text-blue-300">Devnet Live</span>
               </div>
              
              <WalletMultiButton className="!bg-zinc-900 !border !border-white/10 !rounded-xl !h-10 !px-4 !text-sm !font-medium hover:!bg-zinc-800 !transition-all shadow-lg shadow-black/20" />
            </div>
          </div>
        </div>
      </header>

      {/* Main Content */}
      <div 
        ref={containerRef} 
        className="flex-1 flex flex-col lg:flex-row overflow-hidden relative z-10"
        style={{ 
          height: 'calc(100vh - 130px)',
          minHeight: 0
        }}
      >
        {/* Chart Area */}
        <div 
          className="w-full lg:flex-1 flex flex-col shrink-0 p-1"
          style={isMounted && typeof window !== 'undefined' && window.innerWidth >= 1024 
            ? { 
                width: `calc(100% - ${rightPanelWidth}px)`, 
                height: '100%',
                minHeight: 0
              } 
            : { 
                height: '50%',
                minHeight: '350px',
                maxHeight: '50%',
                marginBottom: '0'
              }
          }
        >
          <div className="w-full h-full rounded-2xl border border-white/5 overflow-hidden bg-zinc-900/40 backdrop-blur-sm shadow-2xl ring-1 ring-white/5">
            <TradingChart 
              coin={selectedCoin} 
              onPriceUpdate={setCurrentPrice}
              onCoinChange={setSelectedCoin}
            />
          </div>
        </div>

        {/* Resizer Handle - Desktop Only */}
        <div
          onMouseDown={() => setIsResizing(true)}
          className="hidden lg:flex w-2 items-center justify-center cursor-col-resize group z-20 -ml-1 hover:w-3 transition-all duration-200"
        >
          <div className="w-[1px] h-12 bg-white/10 group-hover:bg-blue-500/50 group-hover:h-24 transition-all rounded-full" />
        </div>

        {/* Trading Panel */}
        <div 
          className="w-full lg:w-auto p-1 overflow-y-auto shrink-0"
          style={isMounted && typeof window !== 'undefined' && window.innerWidth >= 1024 
            ? { 
                width: `${rightPanelWidth}px`,
                height: '100%',
                maxHeight: '100%'
              } 
            : { 
                height: '50%',
                minHeight: '350px',
                maxHeight: '50%',
              }
          }
        >
          <div className="w-full h-full rounded-2xl border border-white/5 bg-zinc-900/40 backdrop-blur-sm shadow-2xl ring-1 ring-white/5 flex flex-col overflow-hidden">
            {/* Trading Panel Header */}
            <div className="flex items-center justify-between px-4 py-3 border-b border-white/5 bg-white/[0.02]">
              <div>
                <div className="flex items-baseline gap-2">
                   <h2 className="text-sm font-semibold text-zinc-100">{selectedCoin}/USDC</h2>
                   <span className={cn("text-xs font-mono", currentPrice > 0 ? "text-emerald-400" : "text-zinc-500")}>
                      ${currentPrice.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 })}
                   </span>
                </div>
              </div>
              
              {/* Trading Mode Toggle */}
              <div className="flex items-center bg-black/20 p-1 rounded-lg border border-white/5">
                <button
                  onClick={() => setTradingMode('orderbook')}
                  className={cn(
                    "px-3 py-1 rounded-md text-xs font-medium transition-all",
                    tradingMode === 'orderbook'
                      ? "bg-zinc-800 text-white shadow-sm ring-1 ring-white/10"
                      : "text-zinc-500 hover:text-zinc-300"
                  )}
                >
                  Order Book
                </button>
                <button
                  onClick={() => setTradingMode('amm')}
                  className={cn(
                    "px-3 py-1 rounded-md text-xs font-medium transition-all",
                    tradingMode === 'amm'
                      ? "bg-zinc-800 text-white shadow-sm ring-1 ring-white/10"
                      : "text-zinc-500 hover:text-zinc-300"
                  )}
                >
                  AMM
                </button>
              </div>
            </div>

            {/* Trading Interface */}
            <div className="flex-1 flex flex-col min-h-0 overflow-hidden relative">
               <div className="absolute inset-0 overflow-y-auto p-3 space-y-3 custom-scrollbar">
                  {tradingMode === 'orderbook' ? (
                    <>
                      <div className="p-1">
                        <OrderForm
                          coin={getCoinString(selectedCoin)}
                          currentPrice={currentPrice}
                        />
                      </div>
                      <div className="p-1">
                        <PositionManager currentPrice={currentPrice} />
                      </div>
                      <div className="rounded-xl border border-white/5 bg-black/20 overflow-hidden">
                        <OrderBook symbol={getSymbol(selectedCoin)} walletAddress={publicKey?.toBase58()} />
                      </div>
                    </>
                  ) : (
                    <div className="h-full">
                      <AMMInterface 
                        selectedCoin={getCoinString(selectedCoin)} 
                        mode="swap"
                        showToast={() => {}}
                        chartCurrentPrice={currentPrice}
                      />
                    </div>
                  )}
              </div>
            </div>
          </div>
        </div>
      </div>
      
      {/* Status Footer */}
      <div className="hidden lg:block shrink-0 border-t border-white/5 bg-zinc-950/80 backdrop-blur z-20">
        <StatusFooter />
      </div>
    </div>
  );
}
