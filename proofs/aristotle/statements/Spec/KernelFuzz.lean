module

public import Spec.Rust.KernelFuzz
public import Spec.Kernel4

/-!
# Public contracts for K06, K42 and K47 (`fuzz`-feature build of `src/v16.rs`)

`V16Core::prepare_source_positive_claim_bound_delta` (K06),
`V16Core::prepare_counterparty_lien_impair_delta` (K42) and
`V16Core::prepare_insurance_lien_impair_delta` (K47) carry
`#[cfg(any(kani, feature = "fuzz"))]`, so they are not compiled in the deployed build (default
features). The only configuration without `cfg(kani)` that compiles them is the crate with its
`fuzz` feature; these properties are about that build (release profile, `overflow-checks = true`,
`panic = "unwind"`), in which the three function bodies are the source text shown in
`src/v16.rs`, unchanged. The `fuzz` feature is enabled through a separate dependent crate
(`harness/k06_k42_k47`) that adds no code.

For each function, `Kxx.contract` and `Kxx.nopanic` are as in `Spec.Kernel4`. Every function
they call is covered by a proof of its body; no contract of a called function is assumed.
-/

@[expose] public section

namespace Spec.Kernel
open Iris Iris.BI Rust2Lean.Public

/-! ## K06 `prepare_source_positive_claim_bound_delta` -/

/-- The call `V16Core::prepare_source_positive_claim_bound_delta(source, claim_bound_num,
exact_claim_num)` (in the `fuzz`-feature build) with postcondition `Φ`. -/
def callClaimBound {PROP : Type} [BI PROP] (S : Vocab PROP) (source : Source)
    (claim_bound_num exact_claim_num : Nat) (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.KernelFuzz.Fn.«percolator::v16::V16Core::prepare_source_positive_claim_bound_delta»
    [(sourceValue S source, S.argTy
        Mir.KernelFuzz.Fn.«percolator::v16::V16Core::prepare_source_positive_claim_bound_delta» 0),
     (S.intVal claim_bound_num, S.argTy
        Mir.KernelFuzz.Fn.«percolator::v16::V16Core::prepare_source_positive_claim_bound_delta» 1),
     (S.intVal exact_claim_num, S.argTy
        Mir.KernelFuzz.Fn.«percolator::v16::V16Core::prepare_source_positive_claim_bound_delta» 2)]
    Φ

/-- K06.contract: under its `requires`, if `prepare_source_positive_claim_bound_delta(source,
claim_bound_num, exact_claim_num)` returns, its result is `Ok(s)` satisfying the `ensures`
(`claim_bound_ensures`), or an `Err(_)`. -/
def prepare_source_positive_claim_bound_delta_contract : Property := fun S =>
  -- For any source state and claim amounts,
  ∀ (source : Source) (claim_bound_num exact_claim_num : Nat),
    -- PRECONDITIONS
    -- if every value is in range of its Rust type,
    source.WF → U128 claim_bound_num → U128 exact_claim_num →
    -- and the `requires` holds (`exact_claim_num <= claim_bound_num`),
    claim_bound_requires claim_bound_num exact_claim_num →
    ⊢ -- then, on normal return of the call,
      callClaimBound S source claim_bound_num exact_claim_num (ifReturns S fun v =>
        -- POSTCONDITIONS
        -- the result is `Ok(s)` with the `ensures` of K06, or an `Err(_)`.
        SourceResult S (claim_bound_ensures source claim_bound_num exact_claim_num) v)

/-- K06.nopanic: under its `requires`, `prepare_source_positive_claim_bound_delta(..)` does not
panic. -/
def prepare_source_positive_claim_bound_delta_nopanic : Property := fun S =>
  -- For any source state and claim amounts,
  ∀ (source : Source) (claim_bound_num exact_claim_num : Nat),
    -- PRECONDITIONS
    -- if every value is in range of its Rust type,
    source.WF → U128 claim_bound_num → U128 exact_claim_num →
    -- and the `requires` holds (`exact_claim_num <= claim_bound_num`),
    claim_bound_requires claim_bound_num exact_claim_num →
    ⊢ -- then the call does not unwind (panic).
      callClaimBound S source claim_bound_num exact_claim_num (S.returns fun _ => iprop(True))

/-! ## K42 `prepare_counterparty_lien_impair_delta` -/

/-- The call `V16Core::prepare_counterparty_lien_impair_delta(bucket, source, amount)` (in the
`fuzz`-feature build) with postcondition `Φ`. -/
def callLienImpair {PROP : Type} [BI PROP] (S : Vocab PROP) (bucket : Bucket) (source : Source)
    (amount : Nat) (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.KernelFuzz.Fn.«percolator::v16::V16Core::prepare_counterparty_lien_impair_delta»
    [(bucketValue S bucket, S.argTy
        Mir.KernelFuzz.Fn.«percolator::v16::V16Core::prepare_counterparty_lien_impair_delta» 0),
     (sourceValue S source, S.argTy
        Mir.KernelFuzz.Fn.«percolator::v16::V16Core::prepare_counterparty_lien_impair_delta» 1),
     (S.intVal amount, S.argTy
        Mir.KernelFuzz.Fn.«percolator::v16::V16Core::prepare_counterparty_lien_impair_delta» 2)] Φ

/-- K42.contract: if `prepare_counterparty_lien_impair_delta(bucket, source, amount)` returns,
its result is `Ok((b, s))` satisfying the `ensures` (`lien_impair_ensures`), or an
`Err(_)`. -/
def prepare_counterparty_lien_impair_delta_contract : Property := fun S =>
  -- For any bucket, source state and amount,
  ∀ (bucket : Bucket) (source : Source) (amount : Nat),
    -- PRECONDITIONS
    -- if every value is in range of its Rust type (no `requires`),
    bucket.WF → source.WF → U128 amount →
    ⊢ -- then, on normal return of the call,
      callLienImpair S bucket source amount (ifReturns S fun v =>
        -- POSTCONDITIONS
        -- the result is `Ok((b, s))` with the `ensures` of K42, or an `Err(_)`.
        BucketSourceResult S (lien_impair_ensures bucket source amount) v)

/-- K42.nopanic: `prepare_counterparty_lien_impair_delta(..)` does not panic. -/
def prepare_counterparty_lien_impair_delta_nopanic : Property := fun S =>
  -- For any bucket, source state and amount,
  ∀ (bucket : Bucket) (source : Source) (amount : Nat),
    -- PRECONDITIONS
    -- if every value is in range of its Rust type (no `requires`),
    bucket.WF → source.WF → U128 amount →
    ⊢ -- then the call does not unwind (panic).
      callLienImpair S bucket source amount (S.returns fun _ => iprop(True))

/-! ## K47 `prepare_insurance_lien_impair_delta` -/

/-- The call `V16Core::prepare_insurance_lien_impair_delta(reservation, source, amount)` (in
the `fuzz`-feature build) with postcondition `Φ`. -/
def callInsuranceLienImpair {PROP : Type} [BI PROP] (S : Vocab PROP) (reservation : Reservation)
    (source : Source) (amount : Nat) (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.KernelFuzz.Fn.«percolator::v16::V16Core::prepare_insurance_lien_impair_delta»
    [(reservationValue S reservation, S.argTy
        Mir.KernelFuzz.Fn.«percolator::v16::V16Core::prepare_insurance_lien_impair_delta» 0),
     (sourceValue S source, S.argTy
        Mir.KernelFuzz.Fn.«percolator::v16::V16Core::prepare_insurance_lien_impair_delta» 1),
     (S.intVal amount, S.argTy
        Mir.KernelFuzz.Fn.«percolator::v16::V16Core::prepare_insurance_lien_impair_delta» 2)] Φ

/-- K47.contract: if `prepare_insurance_lien_impair_delta(reservation, source, amount)`
returns, its result is `Ok((r, s))` satisfying the `ensures`
(`insurance_lien_impair_ensures`), or an `Err(_)`. -/
def prepare_insurance_lien_impair_delta_contract : Property := fun S =>
  -- For any reservation, source state and amount,
  ∀ (reservation : Reservation) (source : Source) (amount : Nat),
    -- PRECONDITIONS
    -- if every value is in range of its Rust type (no `requires`),
    reservation.WF → source.WF → U128 amount →
    ⊢ -- then, on normal return of the call,
      callInsuranceLienImpair S reservation source amount (ifReturns S fun v =>
        -- POSTCONDITIONS
        -- the result is `Ok((r, s))` with the `ensures` of K47, or an `Err(_)`.
        ReservationSourceResult S (insurance_lien_impair_ensures reservation source amount) v)

/-- K47.nopanic: `prepare_insurance_lien_impair_delta(..)` does not panic. -/
def prepare_insurance_lien_impair_delta_nopanic : Property := fun S =>
  -- For any reservation, source state and amount,
  ∀ (reservation : Reservation) (source : Source) (amount : Nat),
    -- PRECONDITIONS
    -- if every value is in range of its Rust type (no `requires`),
    reservation.WF → source.WF → U128 amount →
    ⊢ -- then the call does not unwind (panic).
      callInsuranceLienImpair S reservation source amount (S.returns fun _ => iprop(True))

end Spec.Kernel

end
