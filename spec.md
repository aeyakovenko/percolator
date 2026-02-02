# Risk Engine Spec (Source of Truth)
**Design:** **Protected Principal + Junior Profit Claims with Global Haircut Ratio**  
**Status:** Implementation source-of-truth (normative language: MUST / MUST NOT / SHOULD / MAY)  
**Scope:** Perpetual DEX risk engine for a single quote-token vault (e.g., Solana program-owned vault).  
**Goal:** Achieve the same safety goals as the prior design (oracle manipulation resistance within a warmup window, principal protection, bounded insolvency handling, conservation, and liveness) with **no global ADL scans** and **no “recover stranded” function**.

---

## 0. Security goals (normative)
The engine MUST provide the following properties:

1. **Principal protection:** One account’s insolvency MUST NOT directly reduce any other account’s protected principal.
2. **Oracle manipulation safety (within warmup window `T`):** Profits created by short-lived oracle distortion MUST NOT be withdrawable as principal immediately; they are time-gated by warmup and economically capped by system backing.
3. **Profit-first haircuts:** When the system is undercollateralized, haircuts MUST apply to **junior profit claims** (positive PnL not yet converted to principal) before any protected principal is impacted.
4. **Conservation:** The engine MUST NOT create withdrawable claims exceeding vault tokens, except for a bounded rounding slack (explicitly specified).
5. **Liveness:** The system MUST NOT require “all OI = 0” or manual admin recovery to resume safe withdrawals. In particular, a surviving profitable LP position MUST NOT block accounting progress.

---

## 1. Types, units, and scaling

### 1.1 Amounts
- `u128` amounts are denominated in **quote token atomic units** (the vault token).
- `i128` signed amounts represent realized PnL in the same quote token unit.

### 1.2 Prices and positions
- `price: u64` is **quote per 1 base**, scaled by `1e6`.
- `pos: i128` is in **base units** (consistent across the engine).  
- Notional:
  - `notional = |pos| * price / 1e6` (computed in `u128` with saturation/checked bounds).

### 1.3 Bounds (MUST enforce)
The engine MUST reject or saturate safely when inputs exceed the following conceptual bounds:
- `price > 0` and `price ≤ MAX_ORACLE_PRICE` (implementation-defined; MUST avoid overflow).
- `|pos| ≤ MAX_POSITION_ABS` (implementation-defined; MUST avoid overflow).
- Any multiply/divide MUST avoid wraparound; overflow MUST return an error (or use a documented fail-safe that is conservative for solvency, e.g., treat equity as 0 for margin checks).

---

## 2. State model

### 2.1 Account state
For each account `i`, the engine stores at least:

- `C_i: u128` — **protected principal** (“capital”).
- `PNL_i: i128` — realized PnL claim (can be positive or negative).
- `R_i: u128` — reserved positive PnL (optional; used only if wrapper supports pending PnL withdrawals). MUST satisfy:
  - `0 ≤ R_i ≤ max(PNL_i, 0)`.

Warmup fields (per account):
- `w_start_i: u64` — warmup start slot.
- `w_slope_i: u128` — slope in quote-units per slot.

Position/funding fields (if perp trading supported):
- `pos_i: i128`
- `entry_i: u64` — last settlement reference price (variation margin anchor).
- `f_snap_i: i128` — funding index snapshot.

Fees (optional but recommended):
- `fee_credits_i: i128` — prepaid maintenance credits (may go negative if debt).
- `last_fee_slot_i: u64`

### 2.2 Global engine state
The engine stores at least:

- `V: u128` — vault token balance (program-owned vault).
- `I: u128` — insurance fund balance (a senior claim within `V`).
- `I_floor: u128` — insurance floor threshold (policy parameter; does not affect solvency math directly but may gate risk-increasing ops).
- `current_slot: u64`

Funding (if supported):
- `F_global: i128`
- `last_funding_slot: u64`

**O(1) aggregates (MUST maintain):**
- `C_tot: u128 = Σ C_i` over all used accounts.
- `PNL_pos_tot: u128 = Σ max(PNL_i, 0)` over all used accounts.

Optional aggregates (MAY maintain):
- `OI_tot: u128 = Σ |pos_i|` for policy/liquidation heuristics.

---

## 3. Junior profit solvency via a single global haircut ratio

### 3.1 Residual backing available to junior profits
Define:

- `Residual = max(0, V - C_tot - I)`

`Residual` is the only backing for **junior profit claims** (positive realized PnL that has not been converted into principal).

**Invariant:** The engine MUST maintain `V ≥ C_tot + I` at all times (conservative; if violated, the engine is corrupt and MUST halt/fail).

### 3.2 Haircut ratio `h`
Let:
- If `PNL_pos_tot == 0`: define `h = 1`.
- Else define the rational haircut ratio:
  - `h_num = min(Residual, PNL_pos_tot)`
  - `h_den = PNL_pos_tot`
  - `h = h_num / h_den` (in `[0, 1]`)

### 3.3 Effective positive PnL and effective equity
For account `i`:
- `PNL_pos_i = max(PNL_i, 0)`
- `PNL_eff_pos_i`:
  - If `PNL_pos_tot == 0`: `PNL_eff_pos_i = PNL_pos_i`
  - Else: `PNL_eff_pos_i = floor(PNL_pos_i * h_num / h_den)`

Define effective realized equity (without MTM):
- `Eq_real_i = max(0, (C_i as i128) + min(PNL_i, 0) + (PNL_eff_pos_i as i128))`

If MTM is needed at oracle price `P`:
- `mark_i = mark_pnl(pos_i, entry_i, P)` (signed i128)
- `Eq_mtm_i = max(0, Eq_real_i as i128 + mark_i)` (clamp to 0)

**All margin checks MUST use `Eq_mtm_i`.**  
(If the engine always performs variation-margin settlement to oracle before checks, then `mark_i = 0` and `Eq_mtm_i == Eq_real_i` at that oracle.)

### 3.4 Rounding and conservation
Because each `PNL_eff_pos_i` is floored independently:
- `Σ PNL_eff_pos_i ≤ h_num ≤ Residual`

Therefore junior profits cannot be over-withdrawable.

**Rounding slack bound:**  
Let `K = count(accounts with PNL_i > 0)`. Then:
- `Residual - Σ PNL_eff_pos_i < K`  
Implementation MAY set a global constant `MAX_ROUNDING_SLACK ≥ MAX_ACCOUNTS` and assert `Residual - Σ PNL_eff_pos_i ≤ MAX_ROUNDING_SLACK`.

---

## 4. Aggregate maintenance (MUST use helpers)

### 4.1 Helper: set principal
When changing `C_i` from `old_C` to `new_C`, the engine MUST do:
- `C_tot += (new_C - old_C)` (signed delta in u128-safe manner)

### 4.2 Helper: set_pnl (mandatory)
When changing `PNL_i` from `old` to `new`, the engine MUST:
- `PNL_pos_tot += max(new,0) - max(old,0)` (u128-safe)
- `PNL_i = new`

All code paths that modify PnL (trades, funding, mark settlement, fees, liquidation) MUST call `set_pnl`.

---

## 5. Warmup (time-gated conversion of junior profits to protected principal)

### 5.1 Parameter
- `T = warmup_period_slots` (u64).  
If `T == 0`, warmup is instantaneous.

### 5.2 Available gross profit subject to warmup
For account `i`:
- `AvailGross_i = max(PNL_i, 0) - R_i`  (if `R_i` is supported; else `R_i := 0`)

### 5.3 Warmable gross amount at slot `s`
Let `elapsed = s - w_start_i` (saturating).
Let `cap = w_slope_i * elapsed`.
Then:
- `WarmableGross_i = min(AvailGross_i, cap)`

### 5.4 Warmup slope update rule (MUST be deterministic)
After any change that increases `AvailGross_i` (e.g., new profits), and after any conversion:
- If `AvailGross_i == 0`: `w_slope_i = 0`
- Else if `T > 0`: `w_slope_i = max(1, floor(AvailGross_i / T))`
- Else (`T == 0`): `w_slope_i = AvailGross_i`
- Set `w_start_i = current_slot` (unless warmup is explicitly paused by policy; pausing is optional and not required for correctness of this spec).

---

## 6. Loss settlement and profit conversion (the only way value changes class)

### 6.1 Loss settlement (negative PnL pays from principal immediately)
If `PNL_i < 0`, then on settlement:
1. `need = -PNL_i` (u128)
2. `pay = min(need, C_i)`
3. Apply:
   - `C_i -= pay` (update `C_tot`)
   - `PNL_i += pay` (via `set_pnl`)
4. If after paying `PNL_i` is still negative, the remainder is **unpayable** and MUST be written off:
   - `set_pnl(i, 0)`
   - This write-off is represented globally by `Residual < PNL_pos_tot` (i.e., junior profits elsewhere become haircutted by `h`).

**Principal protection:** This process MUST NOT charge any other account’s `C_j`.

### 6.2 Profit conversion (warmup converts junior claim into protected principal)
Conversion can be invoked during any “touch/settle” and MUST be invoked during withdrawals.

Let `x = WarmableGross_i` computed at `s = current_slot`. If `x == 0`, do nothing.

Compute conversion payout `y` using the **pre-conversion** haircut ratio:
- Compute `(h_num, h_den)` from current global state **before** modifying `PNL_i` or `C_i`.
- If `PNL_pos_tot == 0`: `y = x`
- Else: `y = floor(x * h_num / h_den)`

Apply conversion:
- Reduce junior profit claim by `x`:
  - `set_pnl(i, PNL_i - x)`
- Increase protected principal by `y`:
  - `C_i += y` and update `C_tot`

Advance warmup time base:
- `w_start_i = current_slot`

Then update warmup slope per Section 5.4.

**Important property:** If `y = floor(x*h)`, conversions are order-independent up to rounding: they do not require global scans and do not change `h` except by bounded rounding.

---

## 7. Funding and variation margin (if perpetual trading supported)

### 7.1 Funding index
The engine MAY implement a global funding index `F_global` and per-account snapshot `f_snap_i`.

On funding accrual from `last_funding_slot` to `current_slot`, the engine updates `F_global` deterministically using a policy rate.

### 7.2 Funding settlement per account
On account touch, the engine MUST settle funding into realized PnL:
- `ΔF = F_global - f_snap_i`
- `funding_payment = pos_i * ΔF / 1e6` (rounding policy MUST be specified; recommended: round in a conservative direction that does not overpay from the vault)
- `set_pnl(i, PNL_i - funding_payment)` (sign per convention)
- `f_snap_i = F_global`

### 7.3 Mark-to-oracle (variation margin)
To make positions fungible and keep PnL realized, the engine SHOULD implement mark settlement:
- `mark = mark_pnl(pos_i, entry_i, oracle_price)`
- `set_pnl(i, PNL_i + mark)`
- `entry_i = oracle_price`

Then margin checks can use `mark = 0` at that oracle.

---

## 8. Fees (recommended)

### 8.1 Trading fees (senior, paid to insurance)
Trading fees MUST NOT be socialized via the haircut ratio. They are explicit transfers to insurance.

When charging a fee `fee`:
- Deduct from payer protected principal (or fee credits, if implemented):
  - `C_payer -= fee` (update `C_tot`)
- Credit insurance:
  - `I += fee`

### 8.2 Maintenance fees (optional)
Maintenance fees may be charged per slot, paid to insurance. If `fee_credits_i` exist, they SHOULD be spent first.

If an account cannot pay maintenance due to zero principal, it accrues fee debt; this does not create a system-wide claim and does not affect `h` directly.

---

## 9. Margin checks and liquidation

### 9.1 Margin requirements
At oracle price `P`:
- `Notional_i = |pos_i| * P / 1e6`
- `MM_req = Notional_i * maintenance_bps / 10_000`
- `IM_req = Notional_i * initial_bps / 10_000`

Account is healthy if:
- Maintenance: `Eq_mtm_i > MM_req`
- Initial (for risk-increasing ops): `Eq_mtm_i ≥ IM_req`

### 9.2 Liquidation eligibility
An account is liquidatable when:
- `pos_i != 0` AND after a full settle-to-oracle (funding + mark + fees + loss settle),  
  `Eq_mtm_i ≤ MM_req`.

### 9.3 Liquidation execution (oracle-close)
Liquidation MAY be full or partial. Any liquidation MUST:
1. Close some position at oracle (or via matching engine), realizing mark into `PNL_i` via `set_pnl`.
2. Immediately run:
   - loss settlement (Section 6.1)
   - profit conversion (Section 6.2) (optional, but recommended to keep state tidy)
3. Charge liquidation fee from protected principal to insurance (Section 8).

**No global scans are permitted or required.**  
The system remains live regardless of `OI_tot`.

---

## 10. External operations: preconditions and effects

### 10.1 `touch_account_full(i, oracle_price, now_slot)`
This is a canonical settle routine used by all user ops.

MUST perform, in this exact order:
1. Set `current_slot = now_slot`.
2. Settle funding into `PNL_i` (Section 7.2).
3. Settle mark-to-oracle into `PNL_i` and set `entry_i = oracle_price` (Section 7.3).
4. Charge fees/maintenance due (Section 8).
5. Settle losses immediately (Section 6.1).
6. Convert warmable profits to principal (Section 6.2).

### 10.2 `deposit(i, amount)`
Preconditions:
- Caller transfers `amount` tokens into vault outside the engine; engine observes/assumes it.

Effects:
- `V += amount`
- `C_i += amount` (update `C_tot`)

Then SHOULD call `touch_account_full` (to settle any old losses/fees).

### 10.3 `withdraw(i, amount, oracle_price, now_slot)`
Preconditions (recommended freshness gating):
- A “recent crank / sweep started” freshness policy MAY be required (implementation parameter).  
Regardless of policy, `touch_account_full` MUST be called.

Procedure:
1. `touch_account_full(i, oracle_price, now_slot)`
2. Ensure `amount ≤ C_i`
3. Ensure post-withdraw margin at oracle:
   - compute `Eq_mtm_i` after reducing `C_i` by `amount`
   - require it meets initial margin if `pos_i != 0`

Effects:
- `C_i -= amount` (update `C_tot`)
- `V -= amount` (wrapper transfers tokens out)

### 10.4 `execute_trade(a, b, oracle_price, now_slot, size, exec_price)`
Preconditions:
- For any risk-increasing trade, freshness gating SHOULD be enforced.
- Bounds: `oracle_price`, `exec_price`, and `size` MUST satisfy Section 1.3.

Procedure:
1. `touch_account_full(a, oracle_price, now_slot)`
2. `touch_account_full(b, oracle_price, now_slot)`
3. Apply trade position deltas (ensuring bounds).
4. Compute trade PnL (zero-sum before fees) and apply using `set_pnl`.
5. Charge explicit trading fees to insurance (Section 8.1).
6. Update warmup slopes for any account whose positive PnL increased (Section 5.4).
7. Enforce post-trade maintenance margin using `Eq_mtm` at oracle.

### 10.5 `keeper_crank(...)` (optional)
A crank is optional in this spec; it MAY:
- accrue funding
- touch a bounded window of accounts to keep funding/mark/fees current
- liquidate unhealthy accounts

**Correctness MUST NOT depend on “OI==0” recovery or admin intervention.**  
The haircut ratio `h` ensures continuous solvency of junior profits with no global scanning.

---

## 11. Why this design eliminates “LP profitable position blocks recovery”
Because the system never relies on a recovery function gated by `OI_tot == 0`.  
Instead:
- undercollateralization is represented immediately as `Residual < PNL_pos_tot` which yields `h < 1`, and
- all profit conversion uses `h` so it cannot mint unbacked principal,
- regardless of open positions, as long as accounts are settled to oracle for operations that extract value.

Therefore, a surviving profitable LP position cannot “block” anything; it is just an open position whose PnL is junior and haircutted if unbacked.

---

## 12. Required test properties (minimum)
An implementation MUST include tests that cover:

1. **Conservation:** `V ≥ C_tot + I` always, and `Σ PNL_eff_pos_i ≤ max(0, V - C_tot - I)`.
2. **Oracle manipulation:** create inflated positive PnL, ensure immediate withdrawal cannot extract it before warmup maturity.
3. **Insolvency haircut:** force a loss beyond a loser’s principal and show winners’ conversions are haircutted but winners’ original principal is unaffected.
4. **Liveness with OI>0:** reproduce “LP orphaned profitable position” scenario; show conversions/withdrawals remain possible without admin top-up, bounded by `h`.
5. **Rounding bound:** worst-case distribution across many positive accounts respects slack bound.

---

## 13. Reference pseudocode (non-normative; for clarity)

### 13.1 Compute haircut ratio
```text
Residual = max(0, V - C_tot - I)
if PNL_pos_tot == 0:
  (h_num, h_den) = (1, 1)
else:
  h_num = min(Residual, PNL_pos_tot)
  h_den = PNL_pos_tot
```

### 13.2 Effective positive PnL
```text
if PNL_i <= 0: PNL_eff_pos_i = 0
else if PNL_pos_tot == 0: PNL_eff_pos_i = PNL_i
else: PNL_eff_pos_i = floor(PNL_i * h_num / h_den)
```

### 13.3 Loss settle then convert
```text
# settle losses
if PNL_i < 0:
  pay = min(C_i, -PNL_i)
  C_i -= pay; C_tot -= pay
  PNL_i += pay; set_pnl(i, PNL_i)
  if PNL_i < 0: set_pnl(i, 0)

# convert warmable profit
x = WarmableGross_i
if x > 0:
  (h_num, h_den) = haircut_ratio_pre_conversion()
  y = (PNL_pos_tot == 0) ? x : floor(x * h_num / h_den)
  set_pnl(i, PNL_i - x)
  C_i += y; C_tot += y
  w_start_i = current_slot
  update_warmup_slope(i)
```

---

## 14. Compatibility notes
- The spec is compatible with **LP accounts** and **user accounts**; both share the same protected principal and junior profit mechanics.
- The spec is compatible with a Solana “single slab account” implementation; the only required global aggregates are `C_tot` and `PNL_pos_tot` (both O(1) maintained).
- The spec deliberately removes global ADL distribution, pending buckets, and stranded recovery.

---

**End of spec.**
