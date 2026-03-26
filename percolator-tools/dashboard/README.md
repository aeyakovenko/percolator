# Percolator Dashboard

Analytics UI for Percolator-based perpetual DEXs: vault, insurance, open interest, funding, live decoded positions, and liquidation counts.

## Run (mock data)

```bash
npm install
npm run dev
```

Open [http://localhost:3000](http://localhost:3000). The page starts in **sample data mode** so you can ship and demo before a live wrapper exists.

## Live data

When you have a deployed wrapper or raw engine account:

1. Enter a **Program ID** and click **Find state & load**, or paste a **state account address** directly.
2. The dashboard decodes aggregate RiskEngine state plus the live `accounts[]` slab using the SDK.
3. Top positions will render real live rows from chain when the full slab is present.

## What is live today

- Vault
- Insurance
- Open interest
- Funding rate
- Current slot / last crank slot
- Lifetime liquidation count
- Account count
- Live decoded position rows from the account slab

## Current limitation

The dashboard shows the **live liquidation count**, but not a live liquidation history table yet. Percolator exposes the aggregate count in state; recent liquidation rows would require wrapper event logs, an indexer, or a separate history account.
