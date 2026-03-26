# @percolatortool/sdk

TypeScript SDK for building on top of **Percolator** — [Anatoly Yakovenko (@aeyakovenko)](https://x.com/aeyakovenko)'s risk engine for perpetual DEXs on Solana. This package provides instruction builders plus raw account decoders so you can build wrapper programs, frontends, dashboards, and bots without reimplementing wire formats or account layout parsing.

## What is Percolator?

[Percolator](https://github.com/aeyakovenko/percolator) is an on-chain risk engine for perpetual futures: it handles margin, funding, liquidations, and PnL in a single program. Wrappers (separate programs or frontends) call into it to open/close positions, run keeper cranks, and manage vault/insurance. This SDK was built as part of the **Percolator tooling and audit** so that integrators can quickly build wrappers and bots without hand-rolling instruction layouts.

## What this SDK is for

- **Wrapper builders** — Build a Solana program that uses Percolator as the risk layer; use the SDK to encode `deposit`, `withdraw`, `execute_trade`, and `keeper_crank` instruction data.
- **Frontends & bots** — Compose transactions that invoke your wrapper (and thus Percolator) with correctly formatted instruction data.
- **Dashboards & indexers** — Decode aggregate RiskEngine state and the live `accounts[]` slab directly from chain.
- **Keepers** — The percolator-tools keeper (in this repo) can use this SDK for full types; it also ships with an inline crank builder so it works standalone.

## Install

```bash
npm install @percolatortool/sdk
# or
pnpm add @percolatortool/sdk
```

From the repo (monorepo or local path):

```bash
npm install ./path/to/percolator-tools/sdk
```

## Use

```ts
import {
  buildDepositInstructionData,
  buildWithdrawInstructionData,
  buildExecuteTradeInstructionData,
  buildKeeperCrankInstructionData,
  type DepositArgs,
  type KeeperCrankArgs,
} from '@percolatortool/sdk';

// Deposit
const depositData = buildDepositInstructionData({
  accountIndex: 0,
  amount: BigInt(1_000_000),
  nowSlot: 12345,
});

// Keeper crank (permissionless)
const crankData = buildKeeperCrankInstructionData({
  callerIndex: 0,
  nowSlot: 12345,
  oraclePrice: 1_000_000,
  fundingRateBpsPerSlot: 1,
  allowPanic: false,
});
```

Then pass `depositData` / `crankData` as the `data` for a `TransactionInstruction` to your **wrapper** program ID (your wrapper then forwards to Percolator as defined by its IDL).

### Engine state decoder

You can also decode an engine state account (vault, OI, funding, liquidations count, etc.):

```ts
import { decodeEngineState, formatBigint, type DecodedEngineState } from '@percolatortool/sdk';

const decoded = decodeEngineState(accountData); // Uint8Array from getAccountInfo
if (decoded) {
  console.log('Vault:', formatBigint(decoded.vault));
  console.log('Open interest:', formatBigint(decoded.totalOpenInterest));
  console.log('Liquidations:', decoded.lifetimeLiquidations);
}
```

### Full RiskEngine decode

For dashboards or bots, you can decode both the aggregate engine state and the live account slab:

```ts
import { decodeRiskEngine, decodeAccountSlab } from '@percolatortool/sdk';

const risk = decodeRiskEngine(accountData);
if (risk) {
  console.log('Decoded from offset:', risk.offset);
  console.log('Accounts decoded:', risk.accounts.length);
}

const accounts = decodeAccountSlab(accountData, { offset: 0 }) ?? [];
for (const account of accounts) {
  console.log(account.accountId.toString(), account.kind, account.positionSize.toString());
}
```

`decodeRiskEngine()` tries common layouts where the raw engine starts at byte `0` or byte `8`, which is useful for wrapper accounts with an 8-byte prefix or discriminator.

## Important note

Instruction **layout** (discriminators, field order) is defined by your **wrapper** program. This SDK uses a minimal layout; if your wrapper uses Anchor or a different layout, adapt the builders or use this as reference.

## Links

- **Percolator (risk engine):** [github.com/aeyakovenko/percolator](https://github.com/aeyakovenko/percolator)
- **This SDK + keeper + dashboard:** [github.com/cryptoduke01/percolator](https://github.com/cryptoduke01/percolator) (fork with percolator-tools).

## Credits

SDK and Percolator tooling (dashboard, keeper) by [duke.sol](https://x.com/cryptoduke01) as part of the Percolator audit and ecosystem tooling. Percolator itself by [Anatoly Yakovenko (@aeyakovenko)](https://x.com/aeyakovenko).
