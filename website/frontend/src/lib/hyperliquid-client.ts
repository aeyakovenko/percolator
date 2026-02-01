/**
 * Hyperliquid API Client for Chart Data
 */

const API_URL = process.env.NEXT_PUBLIC_API_URL || 'http://localhost:5001';
const WS_URL = process.env.NEXT_PUBLIC_WS_URL || 'ws://localhost:5001/ws';

export interface CandlestickData {
  time: number;
  open: number;
  high: number;
  low: number;
  close: number;
  volume: number;
}

export type Coin = 'SOL' | 'ETH' | 'BTC' | 'JUP' | 'BONK' | 'WIF';
export type Timeframe = '1m' | '15m' | '1h' | '12h';

class HyperliquidClient {
  private ws: WebSocket | null = null;
  private subscriptions: Map<string, (candle: CandlestickData) => void> = new Map();
  private reconnectAttempts = 0;
  private maxReconnectAttempts = 10;

  /**
   * Fetch historical candlestick data
   */
  async getCandles(
    coin: Coin,
    interval: Timeframe,
    startTime?: number,
    endTime?: number
  ): Promise<CandlestickData[]> {
    try {
      // Map coin to symbol (SOL -> SOLUSD, ETH -> ETHUSD, BTC -> BTCUSD)
      const symbolMap: Record<Coin, string> = {
        'SOL': 'SOLUSD',
        'ETH': 'ETHUSD',
        'BTC': 'BTCUSD',
        'JUP': 'JUPUSD',
        'BONK': 'BONKUSD',
        'WIF': 'WIFUSD',
      };
      const symbol = symbolMap[coin];

      // Map timeframe to dashboard format
      const timeframeMap: Record<Timeframe, string> = {
        '1m': '1m',
        '15m': '15m',
        '1h': '1h',
        '12h': '12h',
      };
      const timeframe = timeframeMap[interval];

      const params = new URLSearchParams({
        timeframe,
        limit: '500',
      });

      if (startTime) params.append('from', startTime.toString());
      if (endTime) params.append('to', endTime.toString());

      const response = await fetch(`${API_URL}/api/dashboard/${symbol}/candles?${params}`);

      if (!response.ok) {
        const errorText = await response.text();
        console.error(`Chart API error: ${response.status}`);
        throw new Error(`API error: ${response.status}`);
      }

      const data = await response.json();

      // Transform data from dashboard format to our format
      // Dashboard returns: [{ time, open, high, low, close, volume }]
      if (Array.isArray(data)) {
        return data.map((c: any) => ({
          time: c.time || c.timestamp || Date.now(),
          open: c.open,
          high: c.high,
          low: c.low,
          close: c.close,
          volume: c.volume || 0,
        }));
      }

      return [];
    } catch (error) {
      console.error(`Failed to fetch candles for ${coin} ${interval}:`, error);
      // Return empty array instead of throwing to prevent crashes
      return [];
    }
  }

  /**
   * Get latest price
   */
  async getPrice(coin: Coin): Promise<number> {
    try {
      // Map coin to symbol
      const symbolMap: Record<Coin, string> = {
        'SOL': 'solana',
        'ETH': 'ethereum',
        'BTC': 'bitcoin',
        'JUP': 'jupiter',
        'BONK': 'bonk',
        'WIF': 'dogwifhat',
      };
      const symbol = symbolMap[coin];

      const response = await fetch(`${API_URL}/api/dashboard/${symbol}`);
      
      if (!response.ok) {
        throw new Error(`API error: ${response.status}`);
      }

      const data = await response.json();
      return data.current_price || 0;
    } catch (error) {
      console.error(`Failed to fetch price for ${coin}:`, error);
      return 0;
    }
  }

  /**
   * Connect to WebSocket for real-time updates
   * NOTE: Disabled for now - WebSocket endpoint not fully implemented
   */
  connect(): Promise<void> {
    return new Promise((resolve, reject) => {
      // WebSocket disabled - charts work fine with polling
      resolve();
      return;

      /* Commented out until WebSocket server is properly configured
      try {
        if (this.ws?.readyState === WebSocket.OPEN) {
          resolve();
          return;
        }

        this.ws = new WebSocket(WS_URL);

        this.ws.onopen = () => {
          this.reconnectAttempts = 0;
          resolve();
        };

        this.ws.onmessage = (event) => {
          try {
            const message = JSON.parse(event.data);

            if (message.type === 'candle' && message.data) {
              const candle: CandlestickData = message.data;
              const key = `${message.coin}-${message.interval}`;

              const handler = this.subscriptions.get(key);
              if (handler) {
                handler(candle);
              }
            }
          } catch (error) {
            console.error('Failed to parse WebSocket message:', error);
          }
        };

        this.ws.onerror = (error) => {
          console.error('WebSocket error:', error);
          reject(error);
        };

        this.ws.onclose = () => {
          this.ws = null;

          // Attempt to reconnect
          if (this.reconnectAttempts < this.maxReconnectAttempts) {
            this.reconnectAttempts++;
            const delay = Math.min(1000 * Math.pow(2, this.reconnectAttempts), 30000);
            setTimeout(() => {
              this.connect().catch(console.error);
            }, delay);
          }
        };
      } catch (error) {
        console.error('Failed to create WebSocket:', error);
        reject(error);
      }
      */
    });
  }

  /**
   * Subscribe to real-time candle updates
   */
  subscribe(
    coin: Coin,
    interval: Timeframe,
    handler: (candle: CandlestickData) => void
  ): () => void {
    const key = `${coin}-${interval}`;
    this.subscriptions.set(key, handler);

    // Connect if not already connected
    if (!this.isConnected()) {
      this.connect().then(() => {
        this.sendSubscription(coin, interval);
      }).catch(console.error);
    } else {
      this.sendSubscription(coin, interval);
    }

    // Return unsubscribe function
    return () => {
      this.subscriptions.delete(key);
      if (this.ws?.readyState === WebSocket.OPEN) {
        this.ws.send(JSON.stringify({
          type: 'unsubscribe',
          coin,
          interval,
        }));
      }
    };
  }

  /**
   * Send subscription message
   */
  private sendSubscription(coin: Coin, interval: Timeframe) {
    if (this.ws?.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify({
        type: 'subscribe',
        coin,
        interval,
      }));
    }
  }

  /**
   * Check if WebSocket is connected
   */
  isConnected(): boolean {
    return this.ws?.readyState === WebSocket.OPEN;
  }

  /**
   * Disconnect WebSocket
   */
  disconnect() {
    if (this.ws) {
      this.ws.close();
      this.ws = null;
      this.subscriptions.clear();
    }
  }
}

export const hyperliquidClient = new HyperliquidClient();

