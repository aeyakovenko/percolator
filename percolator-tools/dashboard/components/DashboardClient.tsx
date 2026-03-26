'use client';

import { useState } from 'react';
import { LiveDataConnect, type LiveDataOptions } from './LiveDataConnect';
import { AddressLink } from './AddressLink';
import { mockPositions, mockLiquidations, mockEngineState } from '@/lib/mockData';
import {
  formatBigint,
  type DecodedEngineAccount,
  type DecodedRiskEngine,
} from '@percolatortool/sdk';

function absBigint(value: bigint): bigint {
  return value < 0n ? -value : value;
}

function formatInteger(value: bigint): string {
  const sign = value < 0n ? '-' : '';
  const digits = absBigint(value).toString();
  return sign + digits.replace(/\B(?=(\d{3})+(?!\d))/g, ',');
}

function formatPriceE6(value: number): string {
  const whole = Math.floor(value / 1_000_000);
  const fractional = (value % 1_000_000).toString().padStart(6, '0').replace(/0+$/, '');
  return fractional ? `${whole}.${fractional}` : `${whole}`;
}

function buildLivePositionRows(accounts: DecodedEngineAccount[]): DecodedEngineAccount[] {
  return [...accounts]
    .sort((a, b) => {
      const aPos = absBigint(a.positionSize);
      const bPos = absBigint(b.positionSize);
      if (aPos === bPos) {
        if (a.capital === b.capital) return Number(a.accountId - b.accountId);
        return a.capital > b.capital ? -1 : 1;
      }
      return aPos > bPos ? -1 : 1;
    })
    .slice(0, 20);
}

export function DashboardClient() {
  const [live, setLive] = useState<DecodedRiskEngine | null>(null);
  const [liveMeta, setLiveMeta] = useState<LiveDataOptions | null>(null);

  const handleLiveData = (data: DecodedRiskEngine, opts: LiveDataOptions) => {
    setLive(data);
    setLiveMeta(opts);
  };
  const handleClear = () => {
    setLive(null);
    setLiveMeta(null);
  };

  const liveEngine = live?.engine ?? null;
  const vault = liveEngine ? formatBigint(liveEngine.vault) : mockEngineState.vault.replace(/_/g, ',');
  const insurance = liveEngine ? formatBigint(liveEngine.insuranceBalance) : mockEngineState.insuranceBalance.replace(/_/g, ',');
  const oi = liveEngine ? formatBigint(liveEngine.totalOpenInterest) : mockEngineState.totalOpenInterest.replace(/_/g, ',');
  const cTot = liveEngine ? formatBigint(liveEngine.cTot) : mockEngineState.cTot.replace(/_/g, ',');
  const pnlPosTot = liveEngine ? formatBigint(liveEngine.pnlPosTot) : mockEngineState.pnlPosTot.replace(/_/g, ',');
  const fundingRate = liveEngine ? liveEngine.fundingRateBpsPerSlot : mockEngineState.fundingRateBpsPerSlot;
  const currentSlot = liveEngine ? liveEngine.currentSlot : mockEngineState.currentSlot;
  const lastCrankSlot = liveEngine ? liveEngine.lastCrankSlot : mockEngineState.lastCrankSlot;
  const liquidations = liveEngine?.lifetimeLiquidations ?? mockEngineState.lifetimeLiquidations;
  const numAccounts = liveEngine?.numUsedAccounts ?? mockEngineState.numUsedAccounts;

  const livePositionsToShow = live ? buildLivePositionRows(live.accounts) : [];
  const liquidationsToShow = live ? [] : mockLiquidations;

  return (
    <div className="container">
      {live ? (
        <div className="live-header-row">
          <div className="demo-pill live-pill">
            <span className="dot live-dot" />
            Live data from chain
          </div>
          {liveMeta?.stateAddress && (
            <span className="live-state-link">
              State: <AddressLink address={liveMeta.stateAddress} cluster={liveMeta.network} className="link-underline" />
            </span>
          )}
        </div>
      ) : (
        <div className="demo-pill">
          <span className="dot" />
          Demo mode — sample data. Connect a state account below for live data.
        </div>
      )}

      <LiveDataConnect onLiveData={handleLiveData} onClear={handleClear} />

      <header className="header">
        <h1>Percolator Dashboard</h1>
        <p className="subtitle">
          Vault, open interest, funding, positions & liquidations for any Percolator-based perp DEX.
        </p>
      </header>

      <p className="units-note">
        PnL, Capital, Vault, and related values are in <strong>quote token units</strong> (e.g. USDC: 6 decimals → 1 USDC = 1,000,000 units). Not dollars unless the market&apos;s quote token is USD.
      </p>
      <section className="cards">
        <div className="card">
          <h3>Vault</h3>
          <div className="value">{vault}</div>
        </div>
        <div className="card">
          <h3>Insurance</h3>
          <div className="value">{insurance}</div>
        </div>
        <div className="card">
          <h3>Open interest</h3>
          <div className="value">{oi}</div>
        </div>
        <div className="card">
          <h3>c_tot</h3>
          <div className="value">{cTot}</div>
        </div>
        <div className="card">
          <h3>PnL+ (pos tot, quote)</h3>
          <div className="value">{pnlPosTot}</div>
        </div>
        <div className="card">
          <h3>Funding (bps/slot)</h3>
          <div className="value">{fundingRate}</div>
        </div>
        <div className="card">
          <h3>Slot / Last crank</h3>
          <div className="value">{currentSlot.toLocaleString()} / {lastCrankSlot.toLocaleString()}</div>
        </div>
        <div className="card">
          <h3>Liquidations</h3>
          <div className="value">{liquidations}</div>
        </div>
        <div className="card">
          <h3>Accounts</h3>
          <div className="value">{numAccounts}</div>
        </div>
      </section>

      {live && (
        <div className="live-data-note">
          <p>
            <strong>Live slab decode:</strong> the positions table below is now decoded directly from the engine&apos;s on-chain <code>accounts[]</code> slab. {live.accountsDecoded ? `Showing ${livePositionsToShow.length} top accounts by position size.` : 'This account layout did not expose the full slab, so only engine aggregates could be decoded.'}
          </p>
          <p>
            <strong>Liquidation history is still aggregate-only.</strong> Percolator exposes the lifetime liquidation count in state, but not a built-in on-chain recent-history table. To show live liquidation rows, we&apos;d need wrapper event logs, indexer data, or a separate history account.
          </p>
        </div>
      )}

      <h2 className="section-title">Top positions</h2>
      <div className="table-wrap">
        <table>
          <thead>
            <tr>
              <th>Account</th>
              <th>Kind</th>
              <th>Owner</th>
              <th>Token</th>
              <th>Capital (quote)</th>
              <th>PnL (quote)</th>
              <th>Position (base)</th>
              <th>Entry</th>
            </tr>
          </thead>
          <tbody>
            {(live ? livePositionsToShow.length === 0 : mockPositions.length === 0) ? (
              <tr>
                <td colSpan={8} className="empty-table-msg">
                  {live
                    ? 'No live accounts decoded. The engine may be empty, or this wrapper layout may not store the full raw slab.'
                    : 'No data. Connect a state account above for live engine state.'}
                </td>
              </tr>
            ) : (
              live
                ? livePositionsToShow.map((row) => (
                    <tr key={`${row.accountIndex}-${row.accountId.toString()}`}>
                      <td>{row.accountId.toString()}</td>
                      <td><span className={`badge ${row.kind}`}>{row.kind}</span></td>
                      <td>
                        {row.owner ? (
                          <AddressLink address={row.owner} cluster={liveMeta?.network ?? 'mainnet'} className="link-underline" />
                        ) : (
                          'Unassigned'
                        )}
                      </td>
                      <td>-</td>
                      <td>{formatInteger(row.capital)}</td>
                      <td className={row.pnl < 0n ? 'neg' : 'pos'}>{formatInteger(row.pnl)}</td>
                      <td>{formatInteger(row.positionSize)}</td>
                      <td>{formatPriceE6(row.entryPrice)}</td>
                    </tr>
                  ))
                : mockPositions.map((row) => (
                    <tr key={row.accountId}>
                      <td>{row.accountId}</td>
                      <td><span className={`badge ${row.kind}`}>{row.kind}</span></td>
                      <td>
                        {row.ownerAddress ? (
                          <AddressLink address={row.ownerAddress} cluster={liveMeta?.network ?? 'mainnet'} display={row.owner} className="link-underline" />
                        ) : (
                          row.owner
                        )}
                      </td>
                      <td>{row.token}</td>
                      <td>{row.capital.replace(/_/g, ',')}</td>
                      <td className={row.pnl.startsWith('-') ? 'neg' : 'pos'}>{row.pnl.replace(/_/g, ',')}</td>
                      <td>{row.positionSize.replace(/_/g, ',')}</td>
                      <td>{row.entryPrice}</td>
                    </tr>
                  ))
            )}
          </tbody>
        </table>
      </div>
      <p className="table-note">
        {live
          ? 'Positions: decoded from the live Percolator account slab.'
          : 'Positions: demo sample data. Connect a state account above for live engine state.'}
      </p>

      <h2 className="section-title">Recent liquidations</h2>
      <div className="table-wrap">
        <table>
          <thead>
            <tr>
              <th>Slot</th>
              <th>Account</th>
              <th>Side</th>
              <th>Token</th>
              <th>Size (base)</th>
              <th>PnL (quote)</th>
            </tr>
          </thead>
          <tbody>
            {liquidationsToShow.length === 0 ? (
              <tr>
                <td colSpan={6} className="empty-table-msg">
                  {live
                    ? 'No live liquidation history. Only the liquidations count above is from chain; history decode can be added for live rows.'
                    : 'No data. Connect a state account above for live counts.'}
                </td>
              </tr>
            ) : (
              liquidationsToShow.map((liq, i) => (
                <tr key={i}>
                  <td>{liq.slot.toLocaleString()}</td>
                  <td>{liq.accountId}</td>
                  <td>{liq.side}</td>
                  <td>{liq.token}</td>
                  <td>{liq.size.replace(/_/g, ',')}</td>
                  <td className={liq.pnl.startsWith('-') ? 'neg' : 'pos'}>{liq.pnl.replace(/_/g, ',')}</td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>
      {!live && <p className="table-note">Liquidations: demo sample data. Connect a state account above for live counts.</p>}

      <footer className="footer">
        <p>
          Built for <a href="https://github.com/aeyakovenko/percolator" target="_blank" rel="noopener noreferrer">Percolator</a>.
          {live ? ' Showing live engine state from chain.' : ' Enter a state account above to load live vault, OI & aggregates.'}
        </p>
      </footer>
    </div>
  );
}
