'use client';

import { useState, useMemo } from 'react';
import { Connection, PublicKey } from '@solana/web3.js';
import { decodeRiskEngine, type DecodedRiskEngine } from '@percolatortool/sdk';

const RPC_DEVNET = 'https://api.devnet.solana.com';
const RPC_MAINNET_PUBLIC = 'https://api.mainnet-beta.solana.com';
const HELIUS_MAINNET = (apiKey: string) => `https://mainnet.helius-rpc.com/?api-key=${apiKey}`;

const KNOWN_PERCOLATOR_PROGRAM = 'GM8zjJ8LTBMv9xEsverh6H6wLyevgMHEJXcEzyY3rY24';

function Spinner() {
  return (
    <span className="btn-spinner" aria-hidden />
  );
}

export type LiveDataOptions = {
  network: 'devnet' | 'mainnet';
  stateAddress?: string;
};

export function LiveDataConnect({
  onLiveData,
  onClear,
}: {
  onLiveData: (data: DecodedRiskEngine, opts: LiveDataOptions) => void;
  onClear: () => void;
}) {
  const [network, setNetwork] = useState<'devnet' | 'mainnet'>('mainnet');
  const [heliusKey, setHeliusKey] = useState('');
  const [programId, setProgramId] = useState(KNOWN_PERCOLATOR_PROGRAM);
  const [stateAddress, setStateAddress] = useState('');
  const [loading, setLoading] = useState(false);
  const [findLoading, setFindLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const rpc = useMemo(() => {
    if (network === 'devnet') return RPC_DEVNET;
    if (network === 'mainnet' && heliusKey.trim()) return HELIUS_MAINNET(heliusKey.trim());
    return RPC_MAINNET_PUBLIC;
  }, [network, heliusKey]);

  async function handleLoad() {
    if (!stateAddress.trim()) {
      setError('Enter state account address or use "Find state & load" with a program ID');
      return;
    }
    setError(null);
    setLoading(true);
    try {
      const connection = new Connection(rpc);
      const pubkey = new PublicKey(stateAddress.trim());
      const account = await connection.getAccountInfo(pubkey);
      if (!account?.data) {
        setError('Account not found or no data');
        return;
      }
      const decoded = decodeRiskEngine(account.data);
      if (!decoded) {
        setError('Invalid state layout (wrong account or format)');
        return;
      }
      onLiveData(decoded, { network, stateAddress: stateAddress.trim() });
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to fetch');
    } finally {
      setLoading(false);
    }
  }

  async function handleFindAndLoad() {
    if (!programId.trim()) {
      setError('Enter Percolator wrapper program ID');
      return;
    }
    setError(null);
    setFindLoading(true);
    try {
      const connection = new Connection(rpc);
      const programPubkey = new PublicKey(programId.trim());
      // Fetch enough of the header to decode aggregates and num_used_accounts.
      const accounts = await connection.getProgramAccounts(programPubkey, {
        dataSlice: { offset: 0, length: 980 },
      });
      for (const { pubkey: key, account } of accounts) {
        if (account.data.length < 344) continue;
        const decoded = decodeRiskEngine(account.data);
        if (decoded) {
          const addr = key.toBase58();
          const fullAccount = await connection.getAccountInfo(key);
          if (!fullAccount?.data) continue;
          const fullDecoded = decodeRiskEngine(fullAccount.data);
          if (!fullDecoded) continue;
          setStateAddress(addr);
          onLiveData(fullDecoded, { network, stateAddress: addr });
          setFindLoading(false);
          return;
        }
      }
      setError('No engine state account found for this program. Try another RPC or program ID.');
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to find state');
    } finally {
      setFindLoading(false);
    }
  }

  return (
    <div className="glass-card connect-box">
      <h3 className="connect-title">Live data (optional)</h3>
      <p className="connect-desc">
        <strong>Just testing?</strong> You don’t need an address — the dashboard below shows sample data.
        To use a live deployment: paste the <strong>Percolator program ID</strong> (e.g. your find) and click <strong>Find state & load</strong>, or paste a state account address and click Load.
      </p>
      <div className="connect-fields">
        <div className="connect-row">
          <label className="connect-label">Network</label>
          <select
            value={network}
            onChange={(e) => setNetwork(e.target.value as 'devnet' | 'mainnet')}
            className="connect-select"
          >
            <option value="devnet">Devnet</option>
            <option value="mainnet">Mainnet</option>
          </select>
        </div>
        {network === 'mainnet' && (
          <div className="connect-row">
            <label className="connect-label">Helius API key (optional — avoids 403 from browser)</label>
            <input
              type="password"
              placeholder="Paste your Helius API key"
              value={heliusKey}
              onChange={(e) => setHeliusKey(e.target.value)}
              className="connect-input"
              autoComplete="off"
            />
          </div>
        )}
        <input
          type="text"
          placeholder="Percolator program ID (e.g. GM8zj...)"
          value={programId}
          onChange={(e) => setProgramId(e.target.value)}
          className="connect-input"
        />
        <input
          type="text"
          placeholder="Or paste state account address (base58)"
          value={stateAddress}
          onChange={(e) => setStateAddress(e.target.value)}
          className="connect-input"
        />
      </div>
      <p className="connect-rpc-hint">Using: {network === 'mainnet' && heliusKey.trim() ? 'Helius mainnet' : rpc}</p>
      <div className="connect-actions">
        <button type="button" onClick={handleFindAndLoad} disabled={findLoading || loading} className="connect-btn connect-btn-primary">
          {findLoading && <Spinner />}
          {findLoading ? ' Finding state…' : 'Find state & load'}
        </button>
        <button type="button" onClick={handleLoad} disabled={loading || findLoading} className="connect-btn connect-btn-primary">
          {loading && <Spinner />}
          {loading ? ' Loading…' : 'Load from address'}
        </button>
        <button type="button" onClick={() => { onClear(); setStateAddress(''); setError(null); }} className="connect-btn connect-btn-ghost">
          Use sample data
        </button>
      </div>
      {error && <p className="connect-error">{error}</p>}
    </div>
  );
}
