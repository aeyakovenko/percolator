# Percolator tools — roadmap

Tooling and products built on [Percolator](https://github.com/aeyakovenko/percolator) (Toly's risk engine for perp DEXs on Solana). Development focus; details in [DEV_PLAN.md](./DEV_PLAN.md).

---

## Done

| Item | Description |
|------|-------------|
| **SDK** | [@percolatortool/sdk](https://www.npmjs.com/package/@percolatortool/sdk) — TypeScript instruction builders for deposit, withdraw, execute_trade, keeper crank. |
| **Dashboard** | Live engine state from mainnet: vault, OI, funding, liquidations count, accounts. Responsive UI, Solscan links. Positions/liquidations tables still sample (slab decode next). |
| **Keeper** | Script to run the permissionless crank. In this repo. |

---

## Next

| Item | Description |
|------|-------------|
| **SDK: engine state decoder** | Export decodeEngineState (and helpers) in the SDK so one package does build + decode. Dashboard and others consume from SDK. |
| **Dashboard: slab decode** | Parse engine account slab so Top positions and Recent liquidations show live data from chain. |
| **Reference wrapper** | Minimal Solana program: deposit/withdraw/trade using SDK data layout, CPI to Percolator, SPL vault. Base for any perp product. |
| **Meme perps** | First perp product: reference wrapper + oracle + web UI (deposit, open/close, withdraw). Uses our SDK. Meme = branding / first market. |
| **Keeper: docs + config** | README and config (program ID, RPC, keys) so others can run the keeper without editing code. |

---

## Later

| Item | Description |
|------|-------------|
| **CLI** | e.g. decode engine state from CLI (`npx ... state <address>`). |
| **More wrappers / integrations** | Additional products or teams building on Percolator with the SDK. |

---

*Repo: [github.com/cryptoduke01/percolator](https://github.com/cryptoduke01/percolator). Full dev breakdown: [DEV_PLAN.md](./DEV_PLAN.md).*
