module

public import Spec.Kernel3
public import Spec.KernelModel4

/-!
# Public contracts for the source-credit, lien and backing delta kernels of `src/v16.rs`

For each of K04 `available_backing_num_for_source_credit_state`,
K05 `prepare_source_positive_claim_burn_delta`, K39 `prepare_counterparty_lien_release_delta`,
K40 `prepare_counterparty_lien_terminal_release_delta`,
K41 `prepare_counterparty_lien_consume_delta`,
K43 `prepare_counterparty_impaired_lien_retirement_delta`,
K44 `prepare_insurance_lien_create_delta`, K45 `prepare_insurance_lien_release_delta`,
K46 `prepare_insurance_lien_terminal_release_delta` and
K48 `health_requirements_from_base_and_target_lag` (all associated functions of `V16Core`),
two properties of `aristotle/kernel_contracts_spec.md`:

* `Kxx.contract` (`*_contract` below): for all inputs satisfying the `requires`, if the call
  returns, the returned value satisfies the `ensures` (the predicates of `Spec.KernelModel4`, in
  which parameter names denote the values passed by the caller).
* `Kxx.nopanic` (`*_nopanic` below): for all inputs satisfying the `requires`, no execution of
  the call panics (it never unwinds; overflow checks are on in the release profile). This says
  nothing about termination.

They are about the deployed build: default features, no `cfg(kani)`, release profile with
`overflow-checks = true` and `panic = "unwind"`. The functions these kernels call
(`validate_bound_num_atom_aligned`, the derived `PartialEq::ne` of `BackingBucketStatusV16` and
the core library functions) are covered by proofs of their bodies, so no contract of a called
function is assumed.
-/

@[expose] public section

namespace Spec.Kernel
open Iris Iris.BI Rust2Lean.Public

/-! ## Values and results -/

/-- The Rust value of an `InsuranceCreditReservationV16`, fields in declaration order. -/
def reservationValue {PROP : Type} [BI PROP] (S : Vocab PROP) (r : Reservation) : S.Value :=
  S.tupleVal [S.intVal r.insurance_credit_reserved_num, S.intVal r.valid_liened_insurance_num,
    S.intVal r.impaired_liened_insurance_num, S.intVal r.consumed_insurance_num,
    S.intVal r.source_credit_epoch]

/-- A `V16Result<SourceCreditStateV16>` value that is either `Ok(s)` for an in-range `s` with
`ens s`, or some `Err(e)`. -/
def SourceResult {PROP : Type} [BI PROP] (S : Vocab PROP) (ens : Source → Prop) (v : S.Value) :
    Prop :=
  (∃ s, s.WF ∧ v = okValue S (sourceValue S s) ∧ ens s) ∨
  (∃ d : Nat, d < 12 ∧ v = errValue S (v16ErrorValue S d))

/-- A `V16Result<(InsuranceCreditReservationV16, SourceCreditStateV16)>` value that is either
`Ok((r, s))` for in-range `r`, `s` with `ens r s`, or some `Err(e)`. -/
def ReservationSourceResult {PROP : Type} [BI PROP] (S : Vocab PROP)
    (ens : Reservation → Source → Prop) (v : S.Value) : Prop :=
  (∃ r s, r.WF ∧ s.WF ∧
    v = okValue S (S.tupleVal [reservationValue S r, sourceValue S s]) ∧ ens r s) ∨
  (∃ d : Nat, d < 12 ∧ v = errValue S (v16ErrorValue S d))

/-- A `V16Result<(u128, u128, u128)>` value that is either `Ok((a, b, c))` for `u128`s with
`ens a b c`, or some `Err(e)`. -/
def U128TripleResult {PROP : Type} [BI PROP] (S : Vocab PROP) (ens : Nat → Nat → Nat → Prop)
    (v : S.Value) : Prop :=
  (∃ a b c, U128 a ∧ U128 b ∧ U128 c ∧
    v = okValue S (S.tupleVal [S.intVal a, S.intVal b, S.intVal c]) ∧ ens a b c) ∨
  (∃ d : Nat, d < 12 ∧ v = errValue S (v16ErrorValue S d))

/-- The bytes of the promoted constant `&BackingBucketStatusV16::Impaired` that
`prepare_counterparty_impaired_lien_retirement_delta` compares `bucket.status` with: the single
discriminant byte `3`. -/
def impairedConstant {PROP : Type} [BI PROP] (S : Vocab PROP) : PROP :=
  S.global Mir.Kernel.Global.«const [1 bytes] 165f0df0fda1e813» 1 [3]

/-! ## K04 `available_backing_num_for_source_credit_state` -/

/-- The call `V16Core::available_backing_num_for_source_credit_state(state)` with
postcondition `Φ`. -/
def callAvailableBacking {PROP : Type} [BI PROP] (S : Vocab PROP) (state : Source)
    (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.Kernel.Fn.«percolator::v16::V16Core::available_backing_num_for_source_credit_state»
    [(sourceValue S state, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::available_backing_num_for_source_credit_state» 0)]
    Φ

/-- K04.contract: if `available_backing_num_for_source_credit_state(state)` returns, its result
is `Ok(a)` satisfying the `ensures` (`available_backing_ensures`), or an `Err(_)`. -/
def available_backing_num_for_source_credit_state_contract : Property := fun S =>
  -- For any source state,
  ∀ (state : Source),
    -- PRECONDITIONS
    -- if every field is in range of its Rust type (no `requires`),
    state.WF →
    ⊢ -- then, on normal return of the call,
      callAvailableBacking S state (ifReturns S fun v =>
        -- POSTCONDITIONS
        -- the result is `Ok(a)` with the `ensures` of K04, or an `Err(_)`
        -- (`U128Result` of `Spec.Kernel2`, with no condition on the error).
        U128Result S (available_backing_ensures state) True v)

/-- K04.nopanic: `available_backing_num_for_source_credit_state(state)` does not panic. -/
def available_backing_num_for_source_credit_state_nopanic : Property := fun S =>
  -- For any source state,
  ∀ (state : Source),
    -- PRECONDITIONS
    -- if every field is in range of its Rust type (no `requires`),
    state.WF →
    ⊢ -- then the call does not unwind (panic).
      callAvailableBacking S state (S.returns fun _ => iprop(True))

/-! ## K05 `prepare_source_positive_claim_burn_delta` -/

/-- The call `V16Core::prepare_source_positive_claim_burn_delta(source, face_burn_num)` with
postcondition `Φ`. -/
def callClaimBurn {PROP : Type} [BI PROP] (S : Vocab PROP) (source : Source)
    (face_burn_num : Nat) (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.Kernel.Fn.«percolator::v16::V16Core::prepare_source_positive_claim_burn_delta»
    [(sourceValue S source, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::prepare_source_positive_claim_burn_delta» 0),
     (S.intVal face_burn_num, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::prepare_source_positive_claim_burn_delta» 1)] Φ

/-- K05.contract: under its `requires`, if
`prepare_source_positive_claim_burn_delta(source, face_burn_num)` returns, its result is
`Ok(s)` satisfying the `ensures` (`claim_burn_ensures`), or an `Err(_)`. -/
def prepare_source_positive_claim_burn_delta_contract : Property := fun S =>
  -- For any source state and burn amount,
  ∀ (source : Source) (face_burn_num : Nat),
    -- PRECONDITIONS
    -- if every value is in range of its Rust type,
    source.WF → U128 face_burn_num →
    -- and the `requires` holds,
    claim_burn_requires source face_burn_num →
    ⊢ -- then, on normal return of the call,
      callClaimBurn S source face_burn_num (ifReturns S fun v =>
        -- POSTCONDITIONS
        -- the result is `Ok(s)` with the `ensures` of K05, or an `Err(_)`.
        SourceResult S (claim_burn_ensures source face_burn_num) v)

/-- K05.nopanic: under its `requires`,
`prepare_source_positive_claim_burn_delta(source, face_burn_num)` does not panic. -/
def prepare_source_positive_claim_burn_delta_nopanic : Property := fun S =>
  -- For any source state and burn amount,
  ∀ (source : Source) (face_burn_num : Nat),
    -- PRECONDITIONS
    -- if every value is in range of its Rust type,
    source.WF → U128 face_burn_num →
    -- and the `requires` holds,
    claim_burn_requires source face_burn_num →
    ⊢ -- then the call does not unwind (panic).
      callClaimBurn S source face_burn_num (S.returns fun _ => iprop(True))

/-! ## K39 `prepare_counterparty_lien_release_delta` -/

/-- The call `V16Core::prepare_counterparty_lien_release_delta(bucket, source, current_slot,
amount)` with postcondition `Φ`. -/
def callLienRelease {PROP : Type} [BI PROP] (S : Vocab PROP) (bucket : Bucket) (source : Source)
    (current_slot amount : Nat) (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.Kernel.Fn.«percolator::v16::V16Core::prepare_counterparty_lien_release_delta»
    [(bucketValue S bucket, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::prepare_counterparty_lien_release_delta» 0),
     (sourceValue S source, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::prepare_counterparty_lien_release_delta» 1),
     (S.intVal current_slot, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::prepare_counterparty_lien_release_delta» 2),
     (S.intVal amount, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::prepare_counterparty_lien_release_delta» 3)] Φ

/-- K39.contract: if `prepare_counterparty_lien_release_delta(bucket, source, current_slot,
amount)` returns, its result is `Ok((b, s))` satisfying the `ensures`
(`lien_release_ensures`), or an `Err(_)`. -/
def prepare_counterparty_lien_release_delta_contract : Property := fun S =>
  -- For any bucket, source state, slot and amount,
  ∀ (bucket : Bucket) (source : Source) (current_slot amount : Nat),
    -- PRECONDITIONS
    -- if every value is in range of its Rust type (no `requires`),
    bucket.WF → source.WF → U64 current_slot → U128 amount →
    -- and the promoted constant `&BackingBucketStatusV16::Fresh` is in read-only memory,
    freshConstant S ⊢
      -- then, on normal return of the call,
      callLienRelease S bucket source current_slot amount (ifReturns S fun v =>
        -- POSTCONDITIONS
        -- the result is `Ok((b, s))` with the `ensures` of K39, or an `Err(_)`.
        BucketSourceResult S (lien_release_ensures bucket source amount) v)

/-- K39.nopanic: `prepare_counterparty_lien_release_delta(..)` does not panic. -/
def prepare_counterparty_lien_release_delta_nopanic : Property := fun S =>
  -- For any bucket, source state, slot and amount,
  ∀ (bucket : Bucket) (source : Source) (current_slot amount : Nat),
    -- PRECONDITIONS
    -- if every value is in range of its Rust type (no `requires`),
    bucket.WF → source.WF → U64 current_slot → U128 amount →
    -- and the promoted constant `&BackingBucketStatusV16::Fresh` is in read-only memory,
    freshConstant S ⊢
      -- then the call does not unwind (panic).
      callLienRelease S bucket source current_slot amount (S.returns fun _ => iprop(True))

/-! ## K40 `prepare_counterparty_lien_terminal_release_delta` -/

/-- The call `V16Core::prepare_counterparty_lien_terminal_release_delta(bucket, source, amount)`
with postcondition `Φ`. -/
def callLienTerminalRelease {PROP : Type} [BI PROP] (S : Vocab PROP) (bucket : Bucket)
    (source : Source) (amount : Nat) (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call
    Mir.Kernel.Fn.«percolator::v16::V16Core::prepare_counterparty_lien_terminal_release_delta»
    [(bucketValue S bucket, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::prepare_counterparty_lien_terminal_release_delta»
          0),
     (sourceValue S source, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::prepare_counterparty_lien_terminal_release_delta»
          1),
     (S.intVal amount, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::prepare_counterparty_lien_terminal_release_delta»
          2)] Φ

/-- K40.contract: if `prepare_counterparty_lien_terminal_release_delta(bucket, source, amount)`
returns, its result is `Ok((b, s))` satisfying the `ensures` (`lien_release_ensures`), or an
`Err(_)`. -/
def prepare_counterparty_lien_terminal_release_delta_contract : Property := fun S =>
  -- For any bucket, source state and amount,
  ∀ (bucket : Bucket) (source : Source) (amount : Nat),
    -- PRECONDITIONS
    -- if every value is in range of its Rust type (no `requires`),
    bucket.WF → source.WF → U128 amount →
    ⊢ -- then, on normal return of the call,
      callLienTerminalRelease S bucket source amount (ifReturns S fun v =>
        -- POSTCONDITIONS
        -- the result is `Ok((b, s))` with the `ensures` of K40, or an `Err(_)`.
        BucketSourceResult S (lien_release_ensures bucket source amount) v)

/-- K40.nopanic: `prepare_counterparty_lien_terminal_release_delta(..)` does not panic. -/
def prepare_counterparty_lien_terminal_release_delta_nopanic : Property := fun S =>
  -- For any bucket, source state and amount,
  ∀ (bucket : Bucket) (source : Source) (amount : Nat),
    -- PRECONDITIONS
    -- if every value is in range of its Rust type (no `requires`),
    bucket.WF → source.WF → U128 amount →
    ⊢ -- then the call does not unwind (panic).
      callLienTerminalRelease S bucket source amount (S.returns fun _ => iprop(True))

/-! ## K41 `prepare_counterparty_lien_consume_delta` -/

/-- The call `V16Core::prepare_counterparty_lien_consume_delta(bucket, source, amount)` with
postcondition `Φ`. -/
def callLienConsume {PROP : Type} [BI PROP] (S : Vocab PROP) (bucket : Bucket) (source : Source)
    (amount : Nat) (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.Kernel.Fn.«percolator::v16::V16Core::prepare_counterparty_lien_consume_delta»
    [(bucketValue S bucket, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::prepare_counterparty_lien_consume_delta» 0),
     (sourceValue S source, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::prepare_counterparty_lien_consume_delta» 1),
     (S.intVal amount, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::prepare_counterparty_lien_consume_delta» 2)] Φ

/-- K41.contract: if `prepare_counterparty_lien_consume_delta(bucket, source, amount)` returns,
its result is `Ok((b, s))` satisfying the `ensures` (`lien_consume_ensures`), or an
`Err(_)`. -/
def prepare_counterparty_lien_consume_delta_contract : Property := fun S =>
  -- For any bucket, source state and amount,
  ∀ (bucket : Bucket) (source : Source) (amount : Nat),
    -- PRECONDITIONS
    -- if every value is in range of its Rust type (no `requires`),
    bucket.WF → source.WF → U128 amount →
    ⊢ -- then, on normal return of the call,
      callLienConsume S bucket source amount (ifReturns S fun v =>
        -- POSTCONDITIONS
        -- the result is `Ok((b, s))` with the `ensures` of K41, or an `Err(_)`.
        BucketSourceResult S (lien_consume_ensures bucket source amount) v)

/-- K41.nopanic: `prepare_counterparty_lien_consume_delta(..)` does not panic. -/
def prepare_counterparty_lien_consume_delta_nopanic : Property := fun S =>
  -- For any bucket, source state and amount,
  ∀ (bucket : Bucket) (source : Source) (amount : Nat),
    -- PRECONDITIONS
    -- if every value is in range of its Rust type (no `requires`),
    bucket.WF → source.WF → U128 amount →
    ⊢ -- then the call does not unwind (panic).
      callLienConsume S bucket source amount (S.returns fun _ => iprop(True))

/-! ## K43 `prepare_counterparty_impaired_lien_retirement_delta` -/

/-- The call `V16Core::prepare_counterparty_impaired_lien_retirement_delta(bucket, source,
amount)` with postcondition `Φ`. -/
def callImpairedRetirement {PROP : Type} [BI PROP] (S : Vocab PROP) (bucket : Bucket)
    (source : Source) (amount : Nat) (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call
    Mir.Kernel.Fn.«percolator::v16::V16Core::prepare_counterparty_impaired_lien_retirement_delta»
    [(bucketValue S bucket, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::prepare_counterparty_impaired_lien_retirement_delta»
          0),
     (sourceValue S source, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::prepare_counterparty_impaired_lien_retirement_delta»
          1),
     (S.intVal amount, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::prepare_counterparty_impaired_lien_retirement_delta»
          2)] Φ

/-- K43.contract: if `prepare_counterparty_impaired_lien_retirement_delta(bucket, source,
amount)` returns, its result is `Ok((b, s))` satisfying the `ensures`
(`impaired_retirement_ensures`), or an `Err(_)`. -/
def prepare_counterparty_impaired_lien_retirement_delta_contract : Property := fun S =>
  -- For any bucket, source state and amount,
  ∀ (bucket : Bucket) (source : Source) (amount : Nat),
    -- PRECONDITIONS
    -- if every value is in range of its Rust type (no `requires`),
    bucket.WF → source.WF → U128 amount →
    -- and the promoted constant `&BackingBucketStatusV16::Impaired` is in read-only memory,
    impairedConstant S ⊢
      -- then, on normal return of the call,
      callImpairedRetirement S bucket source amount (ifReturns S fun v =>
        -- POSTCONDITIONS
        -- the result is `Ok((b, s))` with the `ensures` of K43, or an `Err(_)`.
        BucketSourceResult S (impaired_retirement_ensures bucket source amount) v)

/-- K43.nopanic: `prepare_counterparty_impaired_lien_retirement_delta(..)` does not panic. -/
def prepare_counterparty_impaired_lien_retirement_delta_nopanic : Property := fun S =>
  -- For any bucket, source state and amount,
  ∀ (bucket : Bucket) (source : Source) (amount : Nat),
    -- PRECONDITIONS
    -- if every value is in range of its Rust type (no `requires`),
    bucket.WF → source.WF → U128 amount →
    -- and the promoted constant `&BackingBucketStatusV16::Impaired` is in read-only memory,
    impairedConstant S ⊢
      -- then the call does not unwind (panic).
      callImpairedRetirement S bucket source amount (S.returns fun _ => iprop(True))

/-! ## K44 `prepare_insurance_lien_create_delta` -/

/-- The call `V16Core::prepare_insurance_lien_create_delta(reservation, source, amount)` with
postcondition `Φ`. -/
def callInsuranceLienCreate {PROP : Type} [BI PROP] (S : Vocab PROP) (reservation : Reservation)
    (source : Source) (amount : Nat) (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.Kernel.Fn.«percolator::v16::V16Core::prepare_insurance_lien_create_delta»
    [(reservationValue S reservation, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::prepare_insurance_lien_create_delta» 0),
     (sourceValue S source, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::prepare_insurance_lien_create_delta» 1),
     (S.intVal amount, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::prepare_insurance_lien_create_delta» 2)] Φ

/-- K44.contract: if `prepare_insurance_lien_create_delta(reservation, source, amount)`
returns, its result is `Ok((r, s))` satisfying the `ensures`
(`insurance_lien_create_ensures`), or an `Err(_)`. -/
def prepare_insurance_lien_create_delta_contract : Property := fun S =>
  -- For any reservation, source state and amount,
  ∀ (reservation : Reservation) (source : Source) (amount : Nat),
    -- PRECONDITIONS
    -- if every value is in range of its Rust type (no `requires`),
    reservation.WF → source.WF → U128 amount →
    ⊢ -- then, on normal return of the call,
      callInsuranceLienCreate S reservation source amount (ifReturns S fun v =>
        -- POSTCONDITIONS
        -- the result is `Ok((r, s))` with the `ensures` of K44, or an `Err(_)`.
        ReservationSourceResult S (insurance_lien_create_ensures reservation source amount) v)

/-- K44.nopanic: `prepare_insurance_lien_create_delta(..)` does not panic. -/
def prepare_insurance_lien_create_delta_nopanic : Property := fun S =>
  -- For any reservation, source state and amount,
  ∀ (reservation : Reservation) (source : Source) (amount : Nat),
    -- PRECONDITIONS
    -- if every value is in range of its Rust type (no `requires`),
    reservation.WF → source.WF → U128 amount →
    ⊢ -- then the call does not unwind (panic).
      callInsuranceLienCreate S reservation source amount (S.returns fun _ => iprop(True))

/-! ## K45 `prepare_insurance_lien_release_delta` -/

/-- The call `V16Core::prepare_insurance_lien_release_delta(reservation, source, amount)` with
postcondition `Φ`. -/
def callInsuranceLienRelease {PROP : Type} [BI PROP] (S : Vocab PROP) (reservation : Reservation)
    (source : Source) (amount : Nat) (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.Kernel.Fn.«percolator::v16::V16Core::prepare_insurance_lien_release_delta»
    [(reservationValue S reservation, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::prepare_insurance_lien_release_delta» 0),
     (sourceValue S source, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::prepare_insurance_lien_release_delta» 1),
     (S.intVal amount, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::prepare_insurance_lien_release_delta» 2)] Φ

/-- K45.contract: if `prepare_insurance_lien_release_delta(reservation, source, amount)`
returns, its result is `Ok((r, s))` satisfying the `ensures`
(`insurance_lien_release_ensures`), or an `Err(_)`. -/
def prepare_insurance_lien_release_delta_contract : Property := fun S =>
  -- For any reservation, source state and amount,
  ∀ (reservation : Reservation) (source : Source) (amount : Nat),
    -- PRECONDITIONS
    -- if every value is in range of its Rust type (no `requires`),
    reservation.WF → source.WF → U128 amount →
    ⊢ -- then, on normal return of the call,
      callInsuranceLienRelease S reservation source amount (ifReturns S fun v =>
        -- POSTCONDITIONS
        -- the result is `Ok((r, s))` with the `ensures` of K45, or an `Err(_)`.
        ReservationSourceResult S (insurance_lien_release_ensures reservation source amount) v)

/-- K45.nopanic: `prepare_insurance_lien_release_delta(..)` does not panic. -/
def prepare_insurance_lien_release_delta_nopanic : Property := fun S =>
  -- For any reservation, source state and amount,
  ∀ (reservation : Reservation) (source : Source) (amount : Nat),
    -- PRECONDITIONS
    -- if every value is in range of its Rust type (no `requires`),
    reservation.WF → source.WF → U128 amount →
    ⊢ -- then the call does not unwind (panic).
      callInsuranceLienRelease S reservation source amount (S.returns fun _ => iprop(True))

/-! ## K46 `prepare_insurance_lien_terminal_release_delta` -/

/-- The call `V16Core::prepare_insurance_lien_terminal_release_delta(reservation, source,
amount)` with postcondition `Φ`. -/
def callInsuranceLienTerminalRelease {PROP : Type} [BI PROP] (S : Vocab PROP)
    (reservation : Reservation) (source : Source) (amount : Nat)
    (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.Kernel.Fn.«percolator::v16::V16Core::prepare_insurance_lien_terminal_release_delta»
    [(reservationValue S reservation, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::prepare_insurance_lien_terminal_release_delta» 0),
     (sourceValue S source, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::prepare_insurance_lien_terminal_release_delta» 1),
     (S.intVal amount, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::prepare_insurance_lien_terminal_release_delta» 2)]
    Φ

/-- K46.contract: if `prepare_insurance_lien_terminal_release_delta(reservation, source,
amount)` returns, its result is `Ok((r, s))` satisfying the `ensures`
(`insurance_lien_terminal_release_ensures`), or an `Err(_)`. -/
def prepare_insurance_lien_terminal_release_delta_contract : Property := fun S =>
  -- For any reservation, source state and amount,
  ∀ (reservation : Reservation) (source : Source) (amount : Nat),
    -- PRECONDITIONS
    -- if every value is in range of its Rust type (no `requires`),
    reservation.WF → source.WF → U128 amount →
    ⊢ -- then, on normal return of the call,
      callInsuranceLienTerminalRelease S reservation source amount (ifReturns S fun v =>
        -- POSTCONDITIONS
        -- the result is `Ok((r, s))` with the `ensures` of K46, or an `Err(_)`.
        ReservationSourceResult S
          (insurance_lien_terminal_release_ensures reservation source amount) v)

/-- K46.nopanic: `prepare_insurance_lien_terminal_release_delta(..)` does not panic. -/
def prepare_insurance_lien_terminal_release_delta_nopanic : Property := fun S =>
  -- For any reservation, source state and amount,
  ∀ (reservation : Reservation) (source : Source) (amount : Nat),
    -- PRECONDITIONS
    -- if every value is in range of its Rust type (no `requires`),
    reservation.WF → source.WF → U128 amount →
    ⊢ -- then the call does not unwind (panic).
      callInsuranceLienTerminalRelease S reservation source amount
        (S.returns fun _ => iprop(True))

/-! ## K48 `health_requirements_from_base_and_target_lag` -/

/-- The call `V16Core::health_requirements_from_base_and_target_lag(base_initial,
base_maintenance, risk_notional, target_lag_penalty)` with postcondition `Φ`. -/
def callHealthRequirements {PROP : Type} [BI PROP] (S : Vocab PROP)
    (base_initial base_maintenance risk_notional target_lag_penalty : Nat)
    (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.Kernel.Fn.«percolator::v16::V16Core::health_requirements_from_base_and_target_lag»
    [(S.intVal base_initial, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::health_requirements_from_base_and_target_lag» 0),
     (S.intVal base_maintenance, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::health_requirements_from_base_and_target_lag» 1),
     (S.intVal risk_notional, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::health_requirements_from_base_and_target_lag» 2),
     (S.intVal target_lag_penalty, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::health_requirements_from_base_and_target_lag» 3)]
    Φ

/-- K48.contract: if `health_requirements_from_base_and_target_lag(base_initial,
base_maintenance, risk_notional, target_lag_penalty)` returns, its result is
`Ok((im, mm, rn))` satisfying the `ensures` (`health_requirements_ensures`), or an
`Err(_)`. -/
def health_requirements_from_base_and_target_lag_contract : Property := fun S =>
  -- For any four amounts,
  ∀ (base_initial base_maintenance risk_notional target_lag_penalty : Nat),
    -- PRECONDITIONS
    -- if each is in range of `u128` (no `requires`),
    U128 base_initial → U128 base_maintenance → U128 risk_notional → U128 target_lag_penalty →
    ⊢ -- then, on normal return of the call,
      callHealthRequirements S base_initial base_maintenance risk_notional target_lag_penalty
        (ifReturns S fun v =>
          -- POSTCONDITIONS
          -- the result is `Ok((im, mm, rn))` with the `ensures` of K48, or an `Err(_)`.
          U128TripleResult S (health_requirements_ensures base_initial base_maintenance
            risk_notional target_lag_penalty) v)

/-- K48.nopanic: `health_requirements_from_base_and_target_lag(..)` does not panic. -/
def health_requirements_from_base_and_target_lag_nopanic : Property := fun S =>
  -- For any four amounts,
  ∀ (base_initial base_maintenance risk_notional target_lag_penalty : Nat),
    -- PRECONDITIONS
    -- if each is in range of `u128` (no `requires`),
    U128 base_initial → U128 base_maintenance → U128 risk_notional → U128 target_lag_penalty →
    ⊢ -- then the call does not unwind (panic).
      callHealthRequirements S base_initial base_maintenance risk_notional target_lag_penalty
        (S.returns fun _ => iprop(True))

end Spec.Kernel

end
