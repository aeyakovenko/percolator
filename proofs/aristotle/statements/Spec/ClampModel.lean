module

public import Spec.KernelModel

/-!
# Independent mathematical reading of the program's price clamp

A mathematical reading of `oracle_v16::clamp_toward_engine_dt(p_last, target, cap_bps, dt_slots)
-> u64` of `wrapper_ebcded58/oracle_v16_clamp.rs` (a verbatim copy of module `oracle_v16` of the
program's `src/v16_program.rs`):

```text
if p_last == 0 || target == 0 { return target; }
if cap_bps == 0 || dt_slots == 0 { return p_last; }
let max_delta = (p_last as u128).saturating_mul(cap_bps as u128).saturating_mul(dt_slots as u128)
    / 10_000;
let max_delta = core::cmp::min(max_delta, u64::MAX as u128) as u64;
if target > p_last { core::cmp::min(target, p_last.saturating_add(max_delta)) }
else { core::cmp::max(target, p_last.saturating_sub(max_delta)) }
```

Rust `u64`/`u128` values are `Nat`s; `satMul128 a b = min(a * b, u128::MAX)` is
`saturating_mul` on `u128`, `min(p + d, u64::MAX)` is `saturating_add` on `u64`, and Lean's
truncated `p - d` (`0` when `d > p`) is `saturating_sub` on `u64`. `/` is division of naturals
(rounding down), as `u128` division.
-/

@[expose] public section

namespace Spec.Clamp
open Spec.Kernel (U64)

/-- `u64::MAX`. -/
def U64_MAX : Nat := 2 ^ 64 - 1

/-- `u128::MAX`. -/
def U128_MAX : Nat := 2 ^ 128 - 1

/-- `a.saturating_mul(b)` on `u128`. -/
def satMul128 (a b : Nat) : Nat := min (a * b) U128_MAX

/-- `|a - b|` -/
def absDiff (a b : Nat) : Nat := if a ≤ b then b - a else a - b

/-- The value `clamp_toward_engine_dt(p_last, target, cap_bps, dt_slots)` computes, transcribed
from the source above. -/
def clampModel (p_last target cap_bps dt_slots : Nat) : Nat :=
  if p_last = 0 ∨ target = 0 then target
  else if cap_bps = 0 ∨ dt_slots = 0 then p_last
  else
    let max_delta := min (satMul128 (satMul128 p_last cap_bps) dt_slots / 10000) U64_MAX
    if target > p_last then min target (min (p_last + max_delta) U64_MAX)
    else max target (p_last - max_delta)

end Spec.Clamp
