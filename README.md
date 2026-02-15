# Percolator

**EDUCATIONAL RESEARCH PROJECT — NOT PRODUCTION READY. NOT AUDITED. Do NOT use with real funds.**

A predictable alternative to ADL.

If you want the `xy = k` of perpetual futures risk engines -- something you can reason about, audit, and run without human intervention -- the cleanest move is simple: stop treating profit like money. Treat it like what it really is in a stressed exchange: a junior claim on a shared balance sheet.

> No user can ever withdraw more value than actually exists on the exchange balance sheet.

## The Core Idea

- **Principal** (capital) is senior.
- **Profits** are junior IOUs.
- A single global ratio `h` determines how much of all profits are actually backed.
- Profits convert into withdrawable capital only through a bounded warmup process.

## Why This Is Different From ADL

Most perp venues use a waterfall: liquidate, insurance absorbs loss, and if insufficient, ADL. ADL preserves solvency by forcibly reducing profitable positions. The withdrawal-window model instead applies a global pro-rata haircut on profit extraction.

## One Vault. Two Claim Classes.

### Senior Claim: Capital

Capital is withdrawable. Withdrawals only return capital, never raw profit.

### Junior Claim: Profit

Profit is an IOU backed by system residual value. It is not immediately withdrawable. It must first mature into capital through time-gated warmup (spec section 5-6).

## The Global Coverage Ratio `h`

```
Residual  = max(0, V - C_tot - I)

              min(Residual, PNL_pos_tot)
    h     =  --------------------------
                    PNL_pos_tot
```

If the system is fully backed, `h = 1`. If stressed, `h < 1`. Every profitable account is backed by the same fraction `h`.

`V` is the vault balance. `C_tot` is the sum of all capital. `I` is the insurance fund. `PNL_pos_tot` is the sum of all positive PnL across all accounts.

## Profit as Equity

```
effective_pnl_i = floor(max(PNL_i, 0) * h)
```

All winners share the same haircut. No rankings. No queue. Just proportional equity math.

## The Withdrawal Window

Only capital can leave the system. Profits must mature into capital through warmup, and the amount converted is bounded by `h`:

```
payout = floor(warmable_amount * h_num / h_den)
```

If the system is stressed, `h` falls and less profit converts. If losses are realized or buffers improve, `h` rises. The mechanism self-heals mathematically.

## Concrete Example

**Fully solvent:** `Residual = 150`, `PNL_pos_tot = 120` => `h = 1` (fully backed)

**Stressed:** `Residual = 50`, `PNL_pos_tot = 200` => `h = 0.25` (each dollar of profit is backed by 25 cents)

## Side-by-Side

| | ADL | Withdrawal-Window |
|---|---|---|
| **Mechanism** | Forcibly closes profitable positions | Haircuts profit extraction |
| **When triggered** | Insurance depleted | Continuously via `h` |
| **User experience** | Position deleted without consent | Withdrawable amount reduced |
| **Recovery** | Manual re-entry | Automatic as `h` recovers |

## The Invariant

```
Withdrawable value <= Backed capital
```

Formally verified with 145 Kani proofs covering conservation, principal protection, isolation, and no-teleport properties.

## Open Source

Fork it, test it, send bug reports. Percolator is open research under Apache-2.0.

```bash
cargo test --features test

cargo install --locked kani-verifier
cargo kani setup
cargo kani
```

## References

- Tarun Chitra, *Autodeleveraging: Impossibilities and Optimization*, arXiv:2512.01112, 2025. https://arxiv.org/abs/2512.01112
