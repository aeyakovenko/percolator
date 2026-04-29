# Percolator

**EDUCATIONAL RESEARCH PROJECT — NOT PRODUCTION READY. NOT AUDITED. Do NOT use with real funds.**

A predictable perpetual-futures risk engine built around backed exits, lazy
overhang clearing, and bounded cranks.

If you want the `xy = k` of perpetual futures risk engines -- something you can reason about, audit, and run without human intervention -- the cleanest move is simple: stop treating profit like money. Treat it like what it really is in a stressed exchange: a junior claim on a shared balance sheet.

> No user can ever withdraw more value than actually exists on the exchange balance sheet.

## Three Invariants

A stressed perp exchange has three jobs:

1. **Backed exits:** when the vault is stressed, nobody can extract more value than the balance sheet can pay.
2. **Fair overhang clearing:** when positions go bankrupt, the residual is absorbed pro rata instead of by a discretionary ADL queue.
3. **Bounded cranks:** when the oracle moves, the live book is repriced only inside the configured one-step risk budget.

Percolator composes three mechanisms:

- **H** (the haircut ratio) makes positive PnL a junior claim on residual value.
- **A/K/F** (lazy side indices) settles mark moves, funding, and ADL overhang without selecting individual losers.
- **The price/funding envelope** bounds every exposed accrual step before K/F/price/slot state can mutate.

---

## H: Backed Exits

Capital is senior. Profit is junior. A single global ratio determines how much
released positive PnL is actually backed.

```
Residual  = max(0, V - C_tot - I)

              min(Residual, PNL_matured_pos_tot)
    h     =  ----------------------------------
                    PNL_matured_pos_tot
```

If fully backed, `h = 1`. If stressed, `h < 1`. Every profitable account sees
the same fraction of its *released* positive PnL:

```
ReleasedPos_i   = max(PNL_i, 0) - R_i
effective_pnl_i = floor(ReleasedPos_i * h)
```

Fresh profit sits in a per-account reserve `R_i` and converts to released
(matured) profit through admission and warmup. Only admitted matured profit
enters the haircut denominator (`PNL_matured_pos_tot`) and per-account effective
PnL.

This is the core anti-oracle-manipulation defense. An attacker who spikes a
price sees live gain locked in reserve, excluded from both the ratio and their
withdrawable amount, until the instruction policy admits it. Public wrappers
using untrusted live oracle or execution-price PnL must use nonzero admission
warmup; stress-threshold gating is not a substitute.

No rankings, no queue priority, no first-come advantage. The floor rounding is conservative — the sum of all effective PnL never exceeds what exists in the vault.

When the system is stressed, `h` falls and less profit converts. When losses
settle or buffers recover, `h` rises. Self-healing.

Flat accounts are always protected — `h` only gates profit extraction, never touches deposited capital.

---

## A/K/F: Fair Overhang Clearing

When a leveraged account goes bankrupt, two things need to happen: remove the position quantity from open interest, and distribute any uncovered deficit across the opposing side.

Traditional ADL queues pick specific counterparties and force-close them.
Percolator replaces the queue with lazy side indices:

- **A** scales everyone's effective position equally.
- **K** accumulates mark and ADL overhang effects.
- **F** accumulates funding effects.

```
effective_pos(i) = floor(basis_i * A / a_basis_i)
pnl_delta(i)     =
    floor(|basis_i| * ((K - k_snap_i) * FUNDING_DEN + (F - f_snap_i))
          / (a_basis_i * POS_SCALE * FUNDING_DEN))
```

When a liquidation reduces OI, `A` decreases -- every account on that side
shrinks by the same ratio. When a deficit is socialized, `K` shifts -- every
account absorbs the same per-unit loss. Funding moves through `F` the same way:
accounts settle against their snapshots when touched.

No account is singled out. Settlement is O(1) per account and order-independent.

### Markets Return to Healthy

A/K/F guarantees forward progress through a deterministic cycle:

**DrainOnly** — when `A` drops below a precision threshold, no new OI can be added. Positions can only close.

**ResetPending** — when OI reaches zero, the engine snapshots `K`, increments the epoch, and resets `A` back to 1. Remaining accounts settle their residual PnL exactly once when next touched.

**Normal** — once all stale accounts have settled and OI is confirmed zero, the side reopens for trading with full precision.

No admin intervention. No governance vote. The state machine always makes progress.

---

## Price/Funding Envelope

The third invariant is a system bound: an exposed market cannot be cranked
through an arbitrary oracle or funding jump in one step.

For any crank that advances the engine price while open interest exists, the
allowed price move is capped by elapsed slots:

```
abs(P_new - P_last) * 10_000
    <= max_price_move_bps_per_slot * dt * P_last
```

Equivalently, the normalized move is bounded by
`max_price_move_bps_per_slot * dt / 10_000`.

At a high level, the maximum price movement between exposed cranks is bounded by
the system's risk budget. If the market is configured around `L` times leverage,
the safe one-step move is roughly on the order of `1 / L`, with room reserved
for funding, liquidation fees, integer rounding, and fee floors/caps.

This turns "crank often enough" into a hard solvency boundary rather than an
operator preference. A stale or fast-moving oracle target must be fed into the
engine as a capped staircase of effective prices. Same-slot exposed cranks use
the previous price; they cannot mark live OI through a zero-time jump.

Active price or funding accrual also has a maximum elapsed-slot window; beyond
that, ordinary live catch-up fails closed and the wrapper must use recovery or
resolution.

Initialization proves a per-risk-notional envelope for the worst allowed
price/funding step plus liquidation fees. At runtime, before any K/F/price/slot
mutation, the engine checks that the next effective step stays inside that
envelope. If it does not fit, the crank fails closed instead of moving the
market into an unbudgeted state.

---

## How They Compose

| | H | A/K/F | Price/funding envelope |
|---|---|---|---|
| **Solves** | Backed exits | Bankrupt overhang clearing | Bounded live repricing |
| **Math** | Pro-rata profit scaling | Pro-rata position, mark, funding, and deficit scaling | Exact per-risk-notional loss budget |
| **Triggered by** | Withdrawal, conversion, settlement | Mark, funding, liquidation, reset | Live accrual/crank |
| **Failure mode** | Less profit is released | Side drains and resets | Crank fails closed or wrapper stair-steps |

Together:
- No user can withdraw more than exists.
- No user is singled out for forced closure.
- Flat accounts keep their deposits.
- Risk-increasing trades cannot count their own favorable execution slippage as margin.
- Markets recover through deterministic side resets.
- Exposed cranks are bounded to the configured price/funding budget.
- Raw oracle targets are wrapper-owned; the engine only sees capped effective prices.

A/K/F fairness is exact for open-position economics. H fairness is exact for the
currently stored realized claim set, not for the economically "true" claim set
you would get after globally touching every account.

The engine is not the whole public protocol by itself. A compliant wrapper must
enforce authorization, source and clamp oracle/funding inputs, use nonzero live
PnL admission for untrusted public flows, sync recurring fees when enabled, and
reject extraction-sensitive actions while raw oracle target and effective engine
price diverge.

### Wrapper Security Boundary

Do not expose core primitives directly as permissionless public instructions:

- `set_owner`, live/resolved insurance withdrawals, and insurance-to-account
  credits require wrapper authentication and policy limits.
- `ResolveMode::Degenerate` is a privileged recovery branch. Public resolution
  flows should use ordinary resolution unless an authenticated recovery path
  explicitly authorizes the degenerate branch.
- Account-free `accrue_market_to` must not be a public heartbeat on exposed
  markets when price or funding would move account equity. Public catch-up
  should use an account-touching keeper/liquidation path or a wrapper proof that
  the accrual is not equity-active.

---

## Open Source

Fork it, test it, send bug reports. Percolator is open research under Apache-2.0.

```bash
cargo install --locked kani-verifier
cargo kani setup
cargo kani
```

## References

- Tarun Chitra, *Autodeleveraging: Impossibilities and Optimization*, arXiv:2512.01112, 2025. https://arxiv.org/abs/2512.01112
