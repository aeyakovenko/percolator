module

public import Spec.KernelModel

/-!
# Independent mathematical model for `V16Core::kernel_b_settlement_chunk`

A mathematical reading of the `kani::ensures` clause of the associated function
`V16Core::kernel_b_settlement_chunk(b_snap, b_rem, loss_weight, target, chunk_atoms,
endpoint_loss_atom_budget)` of `src/v16.rs` (engine revision `a3efa566`), and of the conditions
under which it refuses a step.

Conventions:

* Rust `u128` values are modelled by `Nat` with the range predicate `U128` (`x < 2^128`).
* `+`, `-`, `*` inside the model are exact arithmetic on `Nat`. In the `ensures` clause they are
  only evaluated where the clause guards them (`target - b_snap` after `target >= b_snap`).
* `x.checked_mul(y).and_then(|v| v.checked_add(z))` is the `Option` computed by
  `checkedMulAdd x y z`: `Some(x * y + z)` when neither the product nor the sum leaves `u128`,
  else `None`. Equality of two such `Option`s is the Rust `==` on `Option<u128>`.
* `SOCIAL_LOSS_DEN` is the constant of `src/lib.rs`,
  `pub const SOCIAL_LOSS_DEN: u128 = 1_000_000_000_000_000_000_000;`, i.e. `10^21`.
* Error discriminants of `V16Error` (declaration order): `ArithmeticOverflow = 1`,
  `RecoveryRequired = 9`.
-/

@[expose] public section

namespace Spec.KernelB
open Spec.Kernel (U128 two128)

/-- `SOCIAL_LOSS_DEN` of `src/lib.rs`: `1_000_000_000_000_000_000_000 = 10^21`. -/
def SOCIAL_LOSS_DEN : Nat := 10 ^ 21

/-- The Rust discriminant of `V16Error::ArithmeticOverflow`. -/
def errArithmeticOverflow : Nat := 1

/-- The Rust discriminant of `V16Error::RecoveryRequired`. -/
def errRecoveryRequired : Nat := 9

/-- An `AccountBSettlementChunkV16` (fields in declaration order). -/
structure BChunk where
  delta_b : Nat
  loss : Nat
  new_remainder : Nat
  remaining_after : Nat

/-- Every field of the chunk is a `u128` value. -/
def BChunk.WF (c : BChunk) : Prop :=
  U128 c.delta_b ∧ U128 c.loss ∧ U128 c.new_remainder ∧ U128 c.remaining_after

/-- `x.checked_mul(y).and_then(|v| v.checked_add(z))` on `u128`. -/
def checkedMulAdd (x y z : Nat) : Option Nat :=
  if x * y < two128 ∧ x * y + z < two128 then some (x * y + z) else none

/-- `limit = if chunk_atoms < endpoint_loss_atom_budget { chunk_atoms } else
{ endpoint_loss_atom_budget }`, i.e. the minimum of the two. -/
def settleLimit (chunk_atoms endpoint_loss_atom_budget : Nat) : Nat :=
  min chunk_atoms endpoint_loss_atom_budget

/-- The `Ok(chunk)` arm of the `ensures` clause of `kernel_b_settlement_chunk`, literally:

```
target >= b_snap
    && chunk.delta_b <= target - b_snap
    && chunk.remaining_after == target - b_snap - chunk.delta_b
    && (chunk.delta_b == 0
        || (chunk.loss <= limit
            && chunk.new_remainder < SOCIAL_LOSS_DEN
            && loss_weight.checked_mul(chunk.delta_b).and_then(|v| v.checked_add(b_rem))
                == chunk.loss.checked_mul(SOCIAL_LOSS_DEN).and_then(|v| v.checked_add(chunk.new_remainder))))
    && (chunk.delta_b != 0 || (target == b_snap && chunk.loss == 0 && chunk.new_remainder == b_rem))
``` -/
def b_settlement_ensures (b_snap b_rem loss_weight target chunk_atoms endpoint_loss_atom_budget : Nat)
    (c : BChunk) : Prop :=
  b_snap ≤ target ∧
  c.delta_b ≤ target - b_snap ∧
  c.remaining_after = target - b_snap - c.delta_b ∧
  (c.delta_b = 0 ∨
    (c.loss ≤ settleLimit chunk_atoms endpoint_loss_atom_budget ∧
     c.new_remainder < SOCIAL_LOSS_DEN ∧
     checkedMulAdd loss_weight c.delta_b b_rem =
       checkedMulAdd c.loss SOCIAL_LOSS_DEN c.new_remainder)) ∧
  (c.delta_b ≠ 0 ∨ (target = b_snap ∧ c.loss = 0 ∧ c.new_remainder = b_rem))

/-- The exact reading of the last equation of the clause ("no overflow on either side"): when
`delta_b != 0`, `loss_weight * delta_b + b_rem` fits in a `u128` and equals
`loss * SOCIAL_LOSS_DEN + new_remainder` (exact arithmetic). With the clause's equation this
also says that both `checked` chains are `Some`. -/
def b_settlement_exact (b_rem loss_weight : Nat) (c : BChunk) : Prop :=
  c.delta_b ≠ 0 →
    loss_weight * c.delta_b + b_rem < two128 ∧
    loss_weight * c.delta_b + b_rem = c.loss * SOCIAL_LOSS_DEN + c.new_remainder

/-- The `Err(e)` arm of the `ensures` clause: `e == RecoveryRequired || e == ArithmeticOverflow`. -/
def b_settlement_err (e : Nat) : Prop :=
  e = errRecoveryRequired ∨ e = errArithmeticOverflow

/-- KB.refusals, `RecoveryRequired`: the call refuses with `RecoveryRequired` exactly when
`target < b_snap`, or `target > b_snap` and one of

* `loss_weight == 0`,
* `endpoint_loss_atom_budget == 0`,
* with `limit = min(chunk_atoms, endpoint_loss_atom_budget)`, the bound
  `(limit + 1) * SOCIAL_LOSS_DEN` fits in a `u128` (so it is not an overflow refusal) and
  `b_rem + loss_weight >= (limit + 1) * SOCIAL_LOSS_DEN` (exact arithmetic): the carried
  remainder already exceeds the limit, or not even one whole unit of move fits within it. -/
def b_settlement_recovery (b_snap b_rem loss_weight target chunk_atoms endpoint_loss_atom_budget : Nat) :
    Prop :=
  target < b_snap ∨
  (b_snap < target ∧
    (loss_weight = 0 ∨ endpoint_loss_atom_budget = 0 ∨
      ((settleLimit chunk_atoms endpoint_loss_atom_budget + 1) * SOCIAL_LOSS_DEN < two128 ∧
       (settleLimit chunk_atoms endpoint_loss_atom_budget + 1) * SOCIAL_LOSS_DEN ≤
         b_rem + loss_weight)))

/-- KB.refusals, `ArithmeticOverflow`: the call refuses with `ArithmeticOverflow` exactly when
`target > b_snap`, `loss_weight != 0`, `endpoint_loss_atom_budget != 0` and, with
`limit = min(chunk_atoms, endpoint_loss_atom_budget)`, `(limit + 1) * SOCIAL_LOSS_DEN` does not
fit in a `u128` (exact arithmetic: it is at least `2^128`). With `SOCIAL_LOSS_DEN = 10^21` this is
`limit >= 340282366920938463` (`= floor((2^128 - 1) / 10^21)`). -/
def b_settlement_overflow (b_snap loss_weight target chunk_atoms endpoint_loss_atom_budget : Nat) :
    Prop :=
  b_snap < target ∧ loss_weight ≠ 0 ∧ endpoint_loss_atom_budget ≠ 0 ∧
    two128 ≤ (settleLimit chunk_atoms endpoint_loss_atom_budget + 1) * SOCIAL_LOSS_DEN

end Spec.KernelB
