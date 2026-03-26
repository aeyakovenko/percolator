# Percolator tools — development plan

Development work only: what’s done, what to add, and how to get to “meme perps” and more tooling. No X or business here.

---

## 1. Current state (shipped)

| Component | What it does |
|-----------|--------------|
| **SDK** | TypeScript instruction builders: deposit, withdraw, execute_trade, keeper_crank. Used by wrapper programs or frontends to build the `data` passed to Percolator (or a wrapper that forwards to Percolator). |
| **Dashboard** | Next.js app. Fetches engine state from mainnet/devnet, decodes aggregates (vault, OI, funding, liquidations count, accounts). Does **not** yet decode the account slab (so positions/liquidations tables are sample data). |
| **Keeper** | Node script that builds keeper_crank instruction data (inline, no SDK dependency) and can submit it. Needs program ID, RPC, accounts — config/docs can be improved. |

---

## 2. How we get to “meme perps”

Percolator is the **risk engine** (margin, funding, liquidations). A **wrapper** program does SPL transfers and calls into the engine. “Meme perps” = a perp product (meme-style or simple) built on top of Percolator using our SDK.

**Stack:**

1. **Wrapper program** (Solana program, e.g. Rust/Anchor)  
   - Owns state (which user has which account index, etc.).  
   - Receives instructions (deposit, withdraw, trade) whose `data` is built by our SDK.  
   - Does: SPL transfer (for deposit/withdraw) + CPI to Percolator with that data.  
   - Needs: oracle price for withdraw/trade/crank (or gets it from client).

2. **Oracle**  
   - Price feed (Pyth, Switchboard, etc.). Wrapper or client fetches price; trades and liquidations use it.

3. **Web app**  
   - Connect wallet, deposit, open/close position, withdraw.  
   - Uses **our SDK** to build instruction data; sends tx to the wrapper (and wrapper CPIs to Percolator).

**Phases (achievable steps):**

| Phase | Work | Outcome |
|-------|------|--------|
| **2a** | **Reference wrapper** — Minimal Solana program that: accepts deposit/withdraw/trade instructions (data format matches our SDK), does SPL move + CPI to Percolator. No UI yet. | Any frontend (including a “meme perp” UI) can talk to it. |
| **2b** | **Oracle integration** — Wrapper or client reads price from a feed; pass into withdraw/trade/crank. | Real prices for PnL and liquidations. |
| **2c** | **Meme perp UI** — Simple web app: connect wallet, deposit, open/close position (oracle price), withdraw. Uses SDK to build data, calls reference wrapper. “Meme” = branding or first market (e.g. meme coin as base). | First perp product shipped on our stack. |
| **2d** | **Testnet / mainnet** — Deploy wrapper + UI; optional: open to audit of wrapper + UI code. | Live meme perps. |

So: **reference wrapper first**, then oracle, then UI, then deploy. Meme perps = that UI + branding/market.

---

## 3. Other tech we can add

| Priority | Item | Description |
|----------|------|-------------|
| **High** | **Dashboard: slab decode** | Decode the engine’s account slab so “Top positions” and “Recent liquidations” show **live** rows from chain instead of sample data. We already decode aggregates; add parsing of the `accounts[]` (and any liquidation history if exposed). |
| **High** | **SDK: engine state decoder** | ✅ Done — `decodeEngineState`, `formatBigint`, and `DecodedEngineState` are exported from the SDK. Dashboard now depends on `@percolatortool/sdk` for decode. |
| **Medium** | **Reference wrapper** | As above — minimal Solana program that uses our instruction layout and CPIs to Percolator. Base for meme perp and for other builders. |
| **Medium** | **Keeper: docs + config** | README: how to set program ID, RPC, state address, keys. Optional: `.env.example`, or a small config file so people can run the keeper without editing code. |
| **Lower** | **CLI** | e.g. `npx @percolatortool/cli state <state-address>` — fetches account, decodes with SDK, prints vault/OI/funding/etc. Quick way to inspect engine state. |
| **Lower** | **SDK: more helpers** | Optional: parse account slab in SDK (once we have the layout), so dashboard and CLI can both use it. |

---

## 4. Suggested order (how we achieve it)

1. **SDK: export engine state decoder** — ✅ Done. Decoder lives in SDK; dashboard imports from `@percolatortool/sdk`.
2. **Dashboard: slab decode** — Use SDK (or local) decoder to parse `accounts[]` and fill “Top positions” (and liquidations if we have layout). Stops showing sample data when live state is loaded.
3. **Reference wrapper** — Implement minimal wrapper program (Rust or Anchor): deposit, withdraw, execute_trade using our SDK’s data layout; CPI to Percolator; SPL for vault moves. No UI yet.
4. **Oracle + wrapper** — Add oracle price to wrapper (or client); wire into withdraw/trade/crank.
5. **Meme perp UI** — Web app: wallet, deposit, open/close, withdraw; SDK builds data; wrapper is the program. Ship as “meme perps” (branding / first market).
6. **Keeper docs** — Config and README so others can run the keeper.
7. **CLI (optional)** — Small package or script: decode state from CLI.

That order gets you: better dashboard (live tables), one package for decode+build, then the first perp product (meme perps) on top of a reference wrapper, plus clearer keeper usage.

---

## 5. Where things live in the repo

- **SDK** — `percolator-tools/sdk/` (add decoder here when we do step 1).
- **Dashboard** — `percolator-tools/dashboard/` (use SDK decoder; add slab decode in step 2).
- **Keeper** — `percolator-tools/keeper/` (docs + config in step 6).
- **Reference wrapper / meme perp** — New: e.g. `percolator-tools/wrapper/` (Rust program) and `percolator-tools/meme-perp-ui/` (Next.js or Vite) when we start phases 2a–2c.

---

*Next step:* Start with **§4 step 1** (decoder in SDK) and **step 2** (slab decode in dashboard), then add **§4 step 3** (reference wrapper) when ready for meme perps.
