# Percolator Tools

SDK, Keeper, and Dashboard for [Percolator](https://github.com/aeyakovenko/percolator) — Toly's risk engine for perpetual DEXs on Solana.

| Package | Description |
|--------|-------------|
| **[sdk](./sdk)** | TypeScript instruction builders plus raw RiskEngine decoders for engine state and account slab data. |
| **[keeper](./keeper)** | Script to call `keeper_crank` on a Percolator wrapper (run locally or as a bot). |
| **[dashboard](./dashboard)** | Analytics UI: live vault, OI, funding, account count, liquidation count, and decoded position rows from the on-chain account slab. |

## Quick start

```bash
# SDK (for building wrappers / frontends)
cd sdk && npm install && npm run build

# Keeper (run crank for a deployment)
cd keeper && npm install && npm start

# Dashboard (dev)
cd dashboard && npm install && npm run dev
```

## Deploy

- **Dashboard:** See [DEPLOY.md](./DEPLOY.md) for Vercel, static export, or Node.
- **SDK:** Optional — publish with `cd sdk && npm run build && npm publish --access public`.

## Audit

Phase 1 security review: [AUDIT_FINDINGS.md](../percolator/AUDIT_FINDINGS.md) in the main percolator repo.
