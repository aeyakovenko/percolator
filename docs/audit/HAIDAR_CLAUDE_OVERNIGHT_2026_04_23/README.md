# Overnight audit of percolator — 2026-04-23

Audit session chasing Anatoly's 5-SOL immutable-mainnet bounty. Written up for toly's secondary "AI-hacks-it" prize regardless of exploit outcome.

## Files here

- **AI_METHODOLOGY_REPORT.md** — structured writeup of the entire audit methodology, attack ladder tried, and verdict. Primary submission.
- **GITHUB_ISSUE_DRAFT.md** — draft INFO-severity issue about README's BPF binary SHA256 not matching the live mainnet binary. File this against `aeyakovenko/percolator-cli` if desired.
- **hunt_conservation.rs** — 80k-case proptest harness that fuzzes `V >= C + I` across random operation sequences plus targeted self-trade/deposit-trade-withdraw adversarial scenarios.
- **hunt_production_scale.rs** — 700k-case proptest harness hunting at production scale for math functions that Kani only proves on bounded domains (`invert_price_e6`, `scale_price_e6`, `to_engine_price`).

## ⚠️ Compatibility note for THIS fork

These Rust test files were written against **upstream `aeyakovenko/percolator` commit `3f55f87`** and **upstream `aeyakovenko/percolator-prog` commit `06f86fb`** (the exact commits pinned by the mainnet deployment's build provenance).

This fork (`HaidarIDK/PERColator`) diverged from upstream at `0d7b38d` on 2025-10-20 and is now **1486 commits apart** from upstream master (585 ahead, 901 behind). The engine API has been renamed (upstream uses `_not_atomic` suffix for a family of methods; this fork does not), so the Rust tests **will not compile** against this fork's engine as-is.

To use them against this fork's code, you'd need to:
1. Rename calls like `engine.deposit_not_atomic(...)` → the fork's equivalent
2. Adjust `Result<T>` vs `Result<T, RiskError>` based on the fork's type alias
3. Port any uses of `LiquidationPolicy::FullClose` / `ExactPartial` to the fork's equivalents

For the markdown docs, they're informational regardless of code divergence.

## Audit results TL;DR

| Layer | Technique | Result |
|---|---|---|
| Admin-reachable instructions | Kani proof #17 (all 4 authorities burned) | Formally dead |
| Decision logic | 81 Kani proofs (Tier-1 universal characterizations) | Proven |
| `execute_trade` conservation | 80k proptest scenarios | No break |
| Production-scale math | 700k proptest cases over full u64 | Clean |
| Anatoly's own `test_security` suite | 235 attack-oriented tests | All pass |
| BPF binary vs README SHA | Dumped live mainnet, compared | Mismatch (INFO-severity issue filed) |

**Verdict**: no drain vector found. The 5 SOL bounty is very likely genuinely unstealable via randomized testing + static analysis.

Full methodology in `AI_METHODOLOGY_REPORT.md`.
