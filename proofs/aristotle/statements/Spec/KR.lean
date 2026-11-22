module

public import Spec.KFLemmas
public import Spec.KFModel
public import Spec.KernelC

/-!
# Part 2: the size of one bankruptcy-residual booking step (engine revision `bfd4d0c4`)

Public contracts for the associated function
`V16Core::kernel_bankruptcy_residual_step_capacity(b_now, weight_sum, rem, residual_remaining,
chunk_atoms) -> V16Result<u128>` of `src/v16.rs`, and its use by
`V16Core::apply_bankruptcy_residual_chunk_to_loss_side(&mut asset, opp, c, residual_remaining)`,
as compiled in the program `Mir.KF` (handles in `Spec.Rust.KF`): default features, no
`cfg(kani)`, release profile with `overflow-checks = true` and `panic = "unwind"`.

Exact integers: `D = SOCIAL_LOSS_DEN = 10^21` and `bookDelta c rem weight_sum =
floor((c * D + rem) / weight_sum)` (`Spec.KFModel`). Inputs range over the whole of `u128`.

Every property is a partial-correctness statement (`S.returns`: no execution panics, and the
postcondition holds if the call returns; `ifReturns`: the postcondition holds if the call
returns). Nothing is claimed about termination (the `U256` division called has a loop).

Hypotheses. Every function reached is covered by a proof of its body, including the
`src/wide_math.rs` callees `U256::from_u128`, `try_into_u128`, `partial_cmp`, `checked_add`,
`checked_sub`, `checked_mul` and `checked_div` (with `div_rem_u256` and its helpers; the earlier
results A1, A3, A4, B1-B4, reused for this build), `kernel_social_loss_chunk_cap`, the four
closures, `social_loss_book_split`, core-library integer and `Option` operations, `u128::min`
and the `?` machinery, except the core-library functions whose documented behaviour the earlier
results already assumed: `PartialOrd::gt` and `PartialOrd::ge` on `U256`
(`Spec.WideMath.DivRemLib`; `gt` is also what `U256::from_u128(rem) > max_scaled` calls), which
is a hypothesis of every property below.
-/

@[expose] public section

namespace Spec.KR
open Iris Iris.BI Rust2Lean.Public
open Spec.Kernel (okValue errValue v16ErrorValue ifReturns Side sideValue bookedValue
  BResidualOutcome)
open Spec.KernelC (AssetState assetValue)
open Spec.KF (U128 den bookDelta capacity_ensures ArithmeticOverflow SOCIAL_LOSS_DEN
  MAX_VAULT_TVL U128_MAX)

/-! ## Calls and hypotheses -/

/-- The call `V16Core::kernel_bankruptcy_residual_step_capacity(b_now, weight_sum, rem,
residual_remaining, chunk_atoms)` with outcome predicate `Φ`. -/
def callCapacity {PROP : Type} [BI PROP] (S : Vocab PROP)
    (b_now weight_sum rem residual_remaining chunk_atoms : Nat) (Φ : CallOutcome S.Value → PROP) :
    PROP :=
  S.call Mir.KF.Fn.«percolator::v16::V16Core::kernel_bankruptcy_residual_step_capacity»
    [(S.intVal b_now, S.argTy
        Mir.KF.Fn.«percolator::v16::V16Core::kernel_bankruptcy_residual_step_capacity» 0),
     (S.intVal weight_sum, S.argTy
        Mir.KF.Fn.«percolator::v16::V16Core::kernel_bankruptcy_residual_step_capacity» 1),
     (S.intVal rem, S.argTy
        Mir.KF.Fn.«percolator::v16::V16Core::kernel_bankruptcy_residual_step_capacity» 2),
     (S.intVal residual_remaining, S.argTy
        Mir.KF.Fn.«percolator::v16::V16Core::kernel_bankruptcy_residual_step_capacity» 3),
     (S.intVal chunk_atoms, S.argTy
        Mir.KF.Fn.«percolator::v16::V16Core::kernel_bankruptcy_residual_step_capacity» 4)] Φ

/-- The Rust type `AssetStateV16`, named as the type of the first parameter of
`V16Core::kernel_mark_kf_stale_cohorts` in this build. -/
def assetTy {PROP : Type} [BI PROP] (S : Vocab PROP) : S.Ty :=
  S.argTy Mir.KF.Fn.«percolator::v16::V16Core::kernel_mark_kf_stale_cohorts» 0

/-- The call `V16Core::apply_bankruptcy_residual_chunk_to_loss_side(asset, opp, engine_chunk,
residual_remaining)`, with `asset` the reference `p`, with outcome predicate `Φ`. -/
def callResidualChunk {PROP : Type} [BI PROP] (S : Vocab PROP) (p : S.Ptr) (opp : Side)
    (engine_chunk residual_remaining : Nat) (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.KF.Fn.«percolator::v16::V16Core::apply_bankruptcy_residual_chunk_to_loss_side»
    [(S.ptrVal p, S.argTy
        Mir.KF.Fn.«percolator::v16::V16Core::apply_bankruptcy_residual_chunk_to_loss_side» 0),
     (sideValue S opp, S.argTy
        Mir.KF.Fn.«percolator::v16::V16Core::apply_bankruptcy_residual_chunk_to_loss_side» 1),
     (S.intVal engine_chunk, S.argTy
        Mir.KF.Fn.«percolator::v16::V16Core::apply_bankruptcy_residual_chunk_to_loss_side» 2),
     (S.intVal residual_remaining, S.argTy
        Mir.KF.Fn.«percolator::v16::V16Core::apply_bankruptcy_residual_chunk_to_loss_side» 3)]
    Φ

/-- The fields of side `opp` of an asset state that the booking reads: `b_long_num`,
`loss_weight_sum_long`, `social_loss_remainder_long_num` for `Long`, the `_short` fields for
`Short`. -/
def sideFields (a : AssetState) : Side → Nat × Nat × Nat
  | .Long => (a.b_long_num, a.loss_weight_sum_long, a.social_loss_remainder_long_num)
  | .Short => (a.b_short_num, a.loss_weight_sum_short, a.social_loss_remainder_short_num)

/-! ## KR.contract -/

/-- **KR.contract.** If `kernel_bankruptcy_residual_step_capacity(..)` returns, its result
satisfies the `kani::ensures` clause above the function: `Ok(chunk)` with
`chunk <= residual_remaining`, `chunk <= chunk_atoms` and
`(residual_remaining != 0 && weight_sum != 0) || chunk == 0`, or `Err(e)` with
`e == ArithmeticOverflow`. -/
def kernel_bankruptcy_residual_step_capacity_contract : Property := fun S =>
  -- ASSUMPTION: the documented behaviour of `gt` and `ge` on `U256` (as assumed by the
  -- earlier results).
  Spec.WideMath.DivRemLib S →
  -- For any index, weight sum, remainder, residual and chunk size,
  ∀ (b_now weight_sum rem residual_remaining chunk_atoms : Nat),
    -- PRECONDITIONS
    -- if they are `u128` values (there is no `requires`),
    U128 b_now → U128 weight_sum → U128 rem → U128 residual_remaining → U128 chunk_atoms →
    ⊢ -- then, if the call returns,
      callCapacity S b_now weight_sum rem residual_remaining chunk_atoms (ifReturns S fun v =>
        -- POSTCONDITION
        -- the result satisfies the `ensures` clause.
        (∃ c : Nat, v = okValue S (S.intVal c) ∧
          capacity_ensures b_now weight_sum rem residual_remaining chunk_atoms c) ∨
        (∃ e : Nat, v = errValue S (v16ErrorValue S e) ∧ e = ArithmeticOverflow))

/-! ## KR.nopanic -/

/-- **KR.nopanic.** No execution of `kernel_bankruptcy_residual_step_capacity(..)` panics, and
it never returns `Err`: if it returns, the result is `Ok(c)` for a `u128` `c`. -/
def kernel_bankruptcy_residual_step_capacity_nopanic : Property := fun S =>
  -- ASSUMPTION: the documented behaviour of `gt` and `ge` on `U256` (as assumed by the
  -- earlier results).
  Spec.WideMath.DivRemLib S →
  -- For any index, weight sum, remainder, residual and chunk size,
  ∀ (b_now weight_sum rem residual_remaining chunk_atoms : Nat),
    -- PRECONDITIONS
    -- if they are `u128` values,
    U128 b_now → U128 weight_sum → U128 rem → U128 residual_remaining → U128 chunk_atoms →
    ⊢ -- then the call does not panic and, if it returns,
      callCapacity S b_now weight_sum rem residual_remaining chunk_atoms (S.returns fun v => iprop(
        -- POSTCONDITION
        -- the result is `Ok(c)` (it is never `Err`).
        ⌊∃ c : Nat, U128 c ∧ v = okValue S (S.intVal c)⌋))

/-! ## KR.fits -/

/-- **KR.fits.** If `kernel_bankruptcy_residual_step_capacity(..)` returns `Ok(c)` with `c > 0`,
then `floor((c * D + rem) / weight_sum) <= u128::MAX - b_now`. -/
def kernel_bankruptcy_residual_step_capacity_fits : Property := fun S =>
  -- ASSUMPTION: the documented behaviour of `gt` and `ge` on `U256` (as assumed by the
  -- earlier results).
  Spec.WideMath.DivRemLib S →
  -- For any index, weight sum, remainder, residual and chunk size,
  ∀ (b_now weight_sum rem residual_remaining chunk_atoms : Nat),
    -- PRECONDITIONS
    -- if they are `u128` values,
    U128 b_now → U128 weight_sum → U128 rem → U128 residual_remaining → U128 chunk_atoms →
    ⊢ -- then the call does not panic and, if it returns `Ok(c)` with `c > 0`,
      callCapacity S b_now weight_sum rem residual_remaining chunk_atoms (S.returns fun v => iprop(
        -- POSTCONDITION
        -- booking `c` cannot overflow the index.
        ⌊∀ c : Nat, v = okValue S (S.intVal c) → 0 < c →
          bookDelta c rem weight_sum ≤ U128_MAX - b_now⌋))

/-! ## KR.zero -/

/-- **KR.zero.** If `residual_remaining > 0`, `weight_sum > 0` and `chunk_atoms > 0`, the call
returns `Ok(0)` exactly when `floor((D + rem) / weight_sum) > u128::MAX - b_now`. -/
def kernel_bankruptcy_residual_step_capacity_zero : Property := fun S =>
  -- ASSUMPTION: the documented behaviour of `gt` and `ge` on `U256` (as assumed by the
  -- earlier results).
  Spec.WideMath.DivRemLib S →
  -- For any index, weight sum, remainder, residual and chunk size,
  ∀ (b_now weight_sum rem residual_remaining chunk_atoms : Nat),
    -- PRECONDITIONS
    -- if they are `u128` values,
    U128 b_now → U128 weight_sum → U128 rem → U128 residual_remaining → U128 chunk_atoms →
    -- and the residual, the weight sum and the chunk size are positive,
    0 < residual_remaining → 0 < weight_sum → 0 < chunk_atoms →
    ⊢ -- then the call does not panic and, if it returns,
      callCapacity S b_now weight_sum rem residual_remaining chunk_atoms (S.returns fun v => iprop(
        -- POSTCONDITION
        -- it returns `Ok(0)` exactly when booking one atom would overflow the index.
        ⌊v = okValue S (S.intVal 0) ↔ bookDelta 1 rem weight_sum > U128_MAX - b_now⌋))

/-! ## KR.books -/

/-- **KR.books.** Assume `weight_sum <= D`, `rem < D` and `chunk_atoms <= MAX_VAULT_TVL`. If
`kernel_bankruptcy_residual_step_capacity(b_now, weight_sum, rem, residual_remaining,
chunk_atoms)` returns `Ok(c)` with `c > 0`, then for every asset state `asset` whose fields for
side `opp` are `b_now`, `weight_sum` and `rem`,
`apply_bankruptcy_residual_chunk_to_loss_side(&mut asset, opp, c, residual_remaining)` does not
panic and, if it returns, returns `Ok(Some(o))` with `o.booked_loss == c`, `o.delta_b >= 1` and
`o.remaining_after == residual_remaining - c` (the caller gets `*asset` back). -/
def kernel_bankruptcy_residual_step_capacity_books : Property := fun S =>
  -- ASSUMPTION: the documented behaviour of `gt` and `ge` on `U256` (as assumed by the
  -- earlier results).
  Spec.WideMath.DivRemLib S →
  -- For any index, weight sum, remainder, residual and chunk size,
  ∀ (b_now weight_sum rem residual_remaining chunk_atoms : Nat),
    -- PRECONDITIONS
    -- if they are `u128` values,
    U128 b_now → U128 weight_sum → U128 rem → U128 residual_remaining → U128 chunk_atoms →
    -- the weight sum is at most `D` (asset-shape validation),
    weight_sum ≤ SOCIAL_LOSS_DEN →
    -- the remainder is below `D`,
    rem < SOCIAL_LOSS_DEN →
    -- and the chunk size is at most `MAX_VAULT_TVL` (config admission),
    chunk_atoms ≤ MAX_VAULT_TVL →
    ⊢ -- then the call does not panic and, if it returns,
      callCapacity S b_now weight_sum rem residual_remaining chunk_atoms (S.returns fun v => iprop(
        -- POSTCONDITIONS
        -- for the returned `Ok(c)` with `c > 0`,
        ∀ c : Nat, ⌜v = okValue S (S.intVal c)⌝ -∗ ⌜0 < c⌝ -∗
        -- for any reference `p`, asset state `asset` and side `opp`
        ∀ (p : S.Ptr) (asset : AssetState) (opp : Side),
          -- such that `asset` is a valid `AssetStateV16` whose side-`opp` fields are
          -- `b_now`, `weight_sum` and `rem`,
          ⌜asset.WF ∧ sideFields asset opp = (b_now, weight_sum, rem)⌝ -∗
          -- if we own the `AssetStateV16` at `p`, holding `asset`,
          S.pointsTo p (assetTy S) (assetValue S asset) -∗
          -- the booking call (with `p` passed as `asset`) does not panic and, if it returns,
          callResidualChunk S p opp c residual_remaining (S.returns fun v' => iprop(
            -- there is a new value `asset'` of `*asset` and an outcome `o` such that
            ∃ (asset' : AssetState) (o : BResidualOutcome),
              -- we own the `AssetStateV16` at `p` again, holding `asset'`,
              S.pointsTo p (assetTy S) (assetValue S asset') ∗
              -- the result is `Ok(Some(o))`,
              ⌊v' = okValue S (bookedValue S (some o))⌋ ∗
              -- which books exactly `c`, moves the index by at least one unit,
              ⌊o.booked_loss = c ∧ o.delta_b ≥ 1 ∧
              -- and leaves `residual_remaining - c` to book.
                o.remaining_after = residual_remaining - c⌋))))

end Spec.KR
