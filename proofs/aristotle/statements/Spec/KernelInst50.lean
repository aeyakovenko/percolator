module

public import Spec.Rust.Inst50
public import Spec.Kernel5

/-!
# Public contracts for K50–K56, the accounting deltas of `MarketGroupV16ViewMut`, at `T = u64`

K50 `apply_total_delta`, K51 `credit_backing_provider_earnings_delta`,
K52 `set_domain_insurance_spent_delta`, K53 `set_domain_insurance_budget_delta`,
K54 `withdraw_domain_insurance_delta`, K55 `credit_account_from_insurance_delta` and
K56 `retire_terminal_unbudgeted_insurance_delta` are private associated functions of the generic
`impl<'a, T> MarketGroupV16ViewMut<'a, T>` of `src/v16.rs`. They are compiled in the default
build, but as generic code: the crate itself never instantiates `T`, and none of the seven
functions mentions `T`. These properties are about their instances at `T = u64`, as compiled
in the program of the harness crate `harness/k50_k56` (which depends on the unchanged
`percolator` with default features and reaches them through public `MarketGroupV16ViewMut::<u64>`
methods); their function handles are in `Spec.Rust.Inst50`.

For each function, two properties of `aristotle/kernel_contracts_spec.md`:

* `Kxx.contract` (`*_contract` below): for all inputs satisfying the `requires`, if the call
  returns, the returned value satisfies the `ensures` (the predicates of `Spec.KernelModel5`,
  in which parameter names denote the values passed by the caller).
* `Kxx.nopanic` (`*_nopanic` below): for all such inputs, no execution of the call panics (it
  never unwinds; overflow checks are on in the release profile). This says nothing about
  termination.

They are about the release build (`overflow-checks = true`, `panic = "unwind"`), no
`cfg(kani)`. Every function these kernels call (`domain_budget_remaining_parts`,
`apply_total_delta` and core library functions) is covered by a proof of its body, so no
contract of a called function is assumed.
-/

@[expose] public section

namespace Spec.Kernel
open Iris Iris.BI Rust2Lean.Public

/-! ## K50 `apply_total_delta` -/

/-- The call `MarketGroupV16ViewMut::<u64>::apply_total_delta(total, old, new)` with
postcondition `Φ`. -/
def callApplyTotalDelta {PROP : Type} [BI PROP] (S : Vocab PROP) (total old new : Nat)
    (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.Inst50.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, u64>::apply_total_delta»
    [(S.intVal total, S.argTy
        Mir.Inst50.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, u64>::apply_total_delta» 0),
     (S.intVal old, S.argTy
        Mir.Inst50.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, u64>::apply_total_delta» 1),
     (S.intVal new, S.argTy
        Mir.Inst50.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, u64>::apply_total_delta» 2)]
    Φ

/-- K50.contract at `T = u64`: if `apply_total_delta(total, old, new)` returns, its result is
`Ok(t)` satisfying the `ensures` (`apply_total_delta_ensures`), or an `Err(_)`. -/
def apply_total_delta_contract : Property := fun S =>
  -- For any total, old and new values,
  ∀ (total old new : Nat),
    -- PRECONDITIONS
    -- if they are `u128` values (no `requires`),
    U128 total → U128 old → U128 new →
    ⊢ -- then, on normal return of the call,
      callApplyTotalDelta S total old new (ifReturns S fun v =>
        -- POSTCONDITIONS
        -- the result is `Ok(t)` with the `ensures` of K50, or an `Err(_)`.
        U128Result S (apply_total_delta_ensures total old new) True v)

/-- K50.nopanic at `T = u64`: `apply_total_delta(total, old, new)` does not panic. -/
def apply_total_delta_nopanic : Property := fun S =>
  -- For any total, old and new values,
  ∀ (total old new : Nat),
    -- PRECONDITIONS
    -- if they are `u128` values (no `requires`),
    U128 total → U128 old → U128 new →
    ⊢ -- then the call does not unwind (panic).
      callApplyTotalDelta S total old new (S.returns fun _ => iprop(True))

/-! ## K51 `credit_backing_provider_earnings_delta` -/

/-- The call `MarketGroupV16ViewMut::<u64>::credit_backing_provider_earnings_delta(vault, c_tot,
insurance, earnings_total, bucket_earnings, amount)` with postcondition `Φ`. -/
def callCreditEarnings {PROP : Type} [BI PROP] (S : Vocab PROP)
    (vault c_tot insurance earnings_total bucket_earnings amount : Nat)
    (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.Inst50.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, u64>::credit_backing_provider_earnings_delta»
    [(S.intVal vault, S.argTy Mir.Inst50.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, u64>::credit_backing_provider_earnings_delta» 0),
     (S.intVal c_tot, S.argTy Mir.Inst50.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, u64>::credit_backing_provider_earnings_delta» 1),
     (S.intVal insurance, S.argTy Mir.Inst50.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, u64>::credit_backing_provider_earnings_delta» 2),
     (S.intVal earnings_total, S.argTy Mir.Inst50.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, u64>::credit_backing_provider_earnings_delta» 3),
     (S.intVal bucket_earnings, S.argTy Mir.Inst50.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, u64>::credit_backing_provider_earnings_delta» 4),
     (S.intVal amount, S.argTy Mir.Inst50.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, u64>::credit_backing_provider_earnings_delta» 5)]
    Φ

/-- K51.contract at `T = u64`: if `credit_backing_provider_earnings_delta(..)` returns, its
result is `Ok((et, be))` satisfying the `ensures` (`credit_earnings_ensures`), or an
`Err(_)`. -/
def credit_backing_provider_earnings_delta_contract : Property := fun S =>
  -- For any vault, totals, earnings and amount,
  ∀ (vault c_tot insurance earnings_total bucket_earnings amount : Nat),
    -- PRECONDITIONS
    -- if they are `u128` values (no `requires`),
    U128 vault → U128 c_tot → U128 insurance → U128 earnings_total → U128 bucket_earnings →
    U128 amount →
    ⊢ -- then, on normal return of the call,
      callCreditEarnings S vault c_tot insurance earnings_total bucket_earnings amount
        (ifReturns S fun v =>
          -- POSTCONDITIONS
          -- the result is `Ok((et, be))` with the `ensures` of K51, or an `Err(_)`.
          PairResult S
            (credit_earnings_ensures vault c_tot insurance earnings_total bucket_earnings
              amount) True v)

/-- K51.nopanic at `T = u64`: `credit_backing_provider_earnings_delta(..)` does not panic. -/
def credit_backing_provider_earnings_delta_nopanic : Property := fun S =>
  -- For any vault, totals, earnings and amount,
  ∀ (vault c_tot insurance earnings_total bucket_earnings amount : Nat),
    -- PRECONDITIONS
    -- if they are `u128` values (no `requires`),
    U128 vault → U128 c_tot → U128 insurance → U128 earnings_total → U128 bucket_earnings →
    U128 amount →
    ⊢ -- then the call does not unwind (panic).
      callCreditEarnings S vault c_tot insurance earnings_total bucket_earnings amount
        (S.returns fun _ => iprop(True))

/-! ## K52 `set_domain_insurance_spent_delta` -/

/-- The call `MarketGroupV16ViewMut::<u64>::set_domain_insurance_spent_delta(total_remaining,
insurance, budget, old_spent, new_spent)` with postcondition `Φ`. -/
def callSetSpent {PROP : Type} [BI PROP] (S : Vocab PROP)
    (total_remaining insurance budget old_spent new_spent : Nat)
    (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.Inst50.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, u64>::set_domain_insurance_spent_delta»
    [(S.intVal total_remaining, S.argTy Mir.Inst50.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, u64>::set_domain_insurance_spent_delta» 0),
     (S.intVal insurance, S.argTy Mir.Inst50.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, u64>::set_domain_insurance_spent_delta» 1),
     (S.intVal budget, S.argTy Mir.Inst50.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, u64>::set_domain_insurance_spent_delta» 2),
     (S.intVal old_spent, S.argTy Mir.Inst50.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, u64>::set_domain_insurance_spent_delta» 3),
     (S.intVal new_spent, S.argTy Mir.Inst50.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, u64>::set_domain_insurance_spent_delta» 4)]
    Φ

/-- K52.contract at `T = u64`: under the `requires`, if `set_domain_insurance_spent_delta(..)`
returns, its result is `Ok(t)` satisfying the `ensures` (`set_spent_ensures`), or an
`Err(_)`. -/
def set_domain_insurance_spent_delta_contract : Property := fun S =>
  -- For any totals, budget and spent amounts,
  ∀ (total_remaining insurance budget old_spent new_spent : Nat),
    -- PRECONDITIONS
    -- if they are `u128` values,
    U128 total_remaining → U128 insurance → U128 budget → U128 old_spent → U128 new_spent →
    -- and the `requires` holds (`old_spent <= budget && new_spent <= budget`),
    set_spent_requires budget old_spent new_spent →
    ⊢ -- then, on normal return of the call,
      callSetSpent S total_remaining insurance budget old_spent new_spent (ifReturns S fun v =>
        -- POSTCONDITIONS
        -- the result is `Ok(t)` with the `ensures` of K52, or an `Err(_)`.
        U128Result S (set_spent_ensures total_remaining insurance budget old_spent new_spent)
          True v)

/-- K52.nopanic at `T = u64`: under the `requires`, `set_domain_insurance_spent_delta(..)`
does not panic. -/
def set_domain_insurance_spent_delta_nopanic : Property := fun S =>
  -- For any totals, budget and spent amounts,
  ∀ (total_remaining insurance budget old_spent new_spent : Nat),
    -- PRECONDITIONS
    -- if they are `u128` values,
    U128 total_remaining → U128 insurance → U128 budget → U128 old_spent → U128 new_spent →
    -- and the `requires` holds,
    set_spent_requires budget old_spent new_spent →
    ⊢ -- then the call does not unwind (panic).
      callSetSpent S total_remaining insurance budget old_spent new_spent
        (S.returns fun _ => iprop(True))

/-! ## K53 `set_domain_insurance_budget_delta` -/

/-- The call `MarketGroupV16ViewMut::<u64>::set_domain_insurance_budget_delta(total_remaining,
insurance_limit, old_budget, spent, new_budget)` with postcondition `Φ`. -/
def callSetBudget {PROP : Type} [BI PROP] (S : Vocab PROP)
    (total_remaining insurance_limit old_budget spent new_budget : Nat)
    (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.Inst50.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, u64>::set_domain_insurance_budget_delta»
    [(S.intVal total_remaining, S.argTy Mir.Inst50.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, u64>::set_domain_insurance_budget_delta» 0),
     (S.intVal insurance_limit, S.argTy Mir.Inst50.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, u64>::set_domain_insurance_budget_delta» 1),
     (S.intVal old_budget, S.argTy Mir.Inst50.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, u64>::set_domain_insurance_budget_delta» 2),
     (S.intVal spent, S.argTy Mir.Inst50.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, u64>::set_domain_insurance_budget_delta» 3),
     (S.intVal new_budget, S.argTy Mir.Inst50.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, u64>::set_domain_insurance_budget_delta» 4)]
    Φ

/-- K53.contract at `T = u64`: under the `requires`, if `set_domain_insurance_budget_delta(..)`
returns, its result is `Ok(t)` satisfying the `ensures` (`set_budget_ensures`), or an
`Err(_)`. -/
def set_domain_insurance_budget_delta_contract : Property := fun S =>
  -- For any totals, budgets and spent amount,
  ∀ (total_remaining insurance_limit old_budget spent new_budget : Nat),
    -- PRECONDITIONS
    -- if they are `u128` values,
    U128 total_remaining → U128 insurance_limit → U128 old_budget → U128 spent →
    U128 new_budget →
    -- and the `requires` holds (`spent <= old_budget && spent <= new_budget`),
    set_budget_requires old_budget spent new_budget →
    ⊢ -- then, on normal return of the call,
      callSetBudget S total_remaining insurance_limit old_budget spent new_budget
        (ifReturns S fun v =>
          -- POSTCONDITIONS
          -- the result is `Ok(t)` with the `ensures` of K53, or an `Err(_)`.
          U128Result S
            (set_budget_ensures total_remaining insurance_limit old_budget spent new_budget)
            True v)

/-- K53.nopanic at `T = u64`: under the `requires`, `set_domain_insurance_budget_delta(..)`
does not panic. -/
def set_domain_insurance_budget_delta_nopanic : Property := fun S =>
  -- For any totals, budgets and spent amount,
  ∀ (total_remaining insurance_limit old_budget spent new_budget : Nat),
    -- PRECONDITIONS
    -- if they are `u128` values,
    U128 total_remaining → U128 insurance_limit → U128 old_budget → U128 spent →
    U128 new_budget →
    -- and the `requires` holds,
    set_budget_requires old_budget spent new_budget →
    ⊢ -- then the call does not unwind (panic).
      callSetBudget S total_remaining insurance_limit old_budget spent new_budget
        (S.returns fun _ => iprop(True))

/-! ## K54 `withdraw_domain_insurance_delta` -/

/-- The call `MarketGroupV16ViewMut::<u64>::withdraw_domain_insurance_delta(vault, insurance,
source_reserved_atoms, budget, spent, domain_reserved_atoms, amount)` with postcondition
`Φ`. -/
def callWithdrawInsurance {PROP : Type} [BI PROP] (S : Vocab PROP)
    (vault insurance source_reserved_atoms budget spent domain_reserved_atoms amount : Nat)
    (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.Inst50.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, u64>::withdraw_domain_insurance_delta»
    [(S.intVal vault, S.argTy Mir.Inst50.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, u64>::withdraw_domain_insurance_delta» 0),
     (S.intVal insurance, S.argTy Mir.Inst50.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, u64>::withdraw_domain_insurance_delta» 1),
     (S.intVal source_reserved_atoms, S.argTy Mir.Inst50.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, u64>::withdraw_domain_insurance_delta» 2),
     (S.intVal budget, S.argTy Mir.Inst50.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, u64>::withdraw_domain_insurance_delta» 3),
     (S.intVal spent, S.argTy Mir.Inst50.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, u64>::withdraw_domain_insurance_delta» 4),
     (S.intVal domain_reserved_atoms, S.argTy Mir.Inst50.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, u64>::withdraw_domain_insurance_delta» 5),
     (S.intVal amount, S.argTy Mir.Inst50.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, u64>::withdraw_domain_insurance_delta» 6)]
    Φ

/-- K54.contract at `T = u64`: if `withdraw_domain_insurance_delta(..)` returns, its result is
`Ok((v, i, b))` satisfying the `ensures` (`withdraw_insurance_ensures`), or an `Err(_)`. -/
def withdraw_domain_insurance_delta_contract : Property := fun S =>
  -- For any vault, insurance, reservations, budget, spent and amount,
  ∀ (vault insurance source_reserved_atoms budget spent domain_reserved_atoms amount : Nat),
    -- PRECONDITIONS
    -- if they are `u128` values (no `requires`),
    U128 vault → U128 insurance → U128 source_reserved_atoms → U128 budget → U128 spent →
    U128 domain_reserved_atoms → U128 amount →
    ⊢ -- then, on normal return of the call,
      callWithdrawInsurance S vault insurance source_reserved_atoms budget spent
        domain_reserved_atoms amount (ifReturns S fun v =>
          -- POSTCONDITIONS
          -- the result is `Ok((v, i, b))` with the `ensures` of K54, or an `Err(_)`.
          U128TripleResult S
            (withdraw_insurance_ensures vault insurance source_reserved_atoms budget spent
              domain_reserved_atoms amount) v)

/-- K54.nopanic at `T = u64`: `withdraw_domain_insurance_delta(..)` does not panic. -/
def withdraw_domain_insurance_delta_nopanic : Property := fun S =>
  -- For any vault, insurance, reservations, budget, spent and amount,
  ∀ (vault insurance source_reserved_atoms budget spent domain_reserved_atoms amount : Nat),
    -- PRECONDITIONS
    -- if they are `u128` values (no `requires`),
    U128 vault → U128 insurance → U128 source_reserved_atoms → U128 budget → U128 spent →
    U128 domain_reserved_atoms → U128 amount →
    ⊢ -- then the call does not unwind (panic).
      callWithdrawInsurance S vault insurance source_reserved_atoms budget spent
        domain_reserved_atoms amount (S.returns fun _ => iprop(True))

/-! ## K55 `credit_account_from_insurance_delta` -/

/-- The call `MarketGroupV16ViewMut::<u64>::credit_account_from_insurance_delta(insurance,
budget_remaining, c_tot, capital, amount)` with postcondition `Φ`. -/
def callCreditFromInsurance {PROP : Type} [BI PROP] (S : Vocab PROP)
    (insurance budget_remaining c_tot capital amount : Nat)
    (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.Inst50.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, u64>::credit_account_from_insurance_delta»
    [(S.intVal insurance, S.argTy Mir.Inst50.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, u64>::credit_account_from_insurance_delta» 0),
     (S.intVal budget_remaining, S.argTy Mir.Inst50.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, u64>::credit_account_from_insurance_delta» 1),
     (S.intVal c_tot, S.argTy Mir.Inst50.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, u64>::credit_account_from_insurance_delta» 2),
     (S.intVal capital, S.argTy Mir.Inst50.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, u64>::credit_account_from_insurance_delta» 3),
     (S.intVal amount, S.argTy Mir.Inst50.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, u64>::credit_account_from_insurance_delta» 4)]
    Φ

/-- K55.contract at `T = u64`: if `credit_account_from_insurance_delta(..)` returns, its
result is `Ok((i, c, cap))` satisfying the `ensures` (`credit_from_insurance_ensures`), or an
`Err(_)`. -/
def credit_account_from_insurance_delta_contract : Property := fun S =>
  -- For any insurance, budget remaining, total, capital and amount,
  ∀ (insurance budget_remaining c_tot capital amount : Nat),
    -- PRECONDITIONS
    -- if they are `u128` values (no `requires`),
    U128 insurance → U128 budget_remaining → U128 c_tot → U128 capital → U128 amount →
    ⊢ -- then, on normal return of the call,
      callCreditFromInsurance S insurance budget_remaining c_tot capital amount
        (ifReturns S fun v =>
          -- POSTCONDITIONS
          -- the result is `Ok((i, c, cap))` with the `ensures` of K55, or an `Err(_)`.
          U128TripleResult S
            (credit_from_insurance_ensures insurance budget_remaining c_tot capital amount) v)

/-- K55.nopanic at `T = u64`: `credit_account_from_insurance_delta(..)` does not panic. -/
def credit_account_from_insurance_delta_nopanic : Property := fun S =>
  -- For any insurance, budget remaining, total, capital and amount,
  ∀ (insurance budget_remaining c_tot capital amount : Nat),
    -- PRECONDITIONS
    -- if they are `u128` values (no `requires`),
    U128 insurance → U128 budget_remaining → U128 c_tot → U128 capital → U128 amount →
    ⊢ -- then the call does not unwind (panic).
      callCreditFromInsurance S insurance budget_remaining c_tot capital amount
        (S.returns fun _ => iprop(True))

/-! ## K56 `retire_terminal_unbudgeted_insurance_delta` -/

/-- The call `MarketGroupV16ViewMut::<u64>::retire_terminal_unbudgeted_insurance_delta(vault,
insurance, budget_remaining, source_reserved_atoms)` with postcondition `Φ`. -/
def callRetireInsurance {PROP : Type} [BI PROP] (S : Vocab PROP)
    (vault insurance budget_remaining source_reserved_atoms : Nat)
    (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.Inst50.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, u64>::retire_terminal_unbudgeted_insurance_delta»
    [(S.intVal vault, S.argTy Mir.Inst50.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, u64>::retire_terminal_unbudgeted_insurance_delta» 0),
     (S.intVal insurance, S.argTy Mir.Inst50.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, u64>::retire_terminal_unbudgeted_insurance_delta» 1),
     (S.intVal budget_remaining, S.argTy Mir.Inst50.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, u64>::retire_terminal_unbudgeted_insurance_delta» 2),
     (S.intVal source_reserved_atoms, S.argTy Mir.Inst50.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, u64>::retire_terminal_unbudgeted_insurance_delta» 3)]
    Φ

/-- K56.contract at `T = u64`: if `retire_terminal_unbudgeted_insurance_delta(..)` returns, its
result is `Ok((retired, next_vault, next_insurance))` satisfying the `ensures`
(`retire_insurance_ensures`), or an `Err(_)`. -/
def retire_terminal_unbudgeted_insurance_delta_contract : Property := fun S =>
  -- For any vault, insurance, budget remaining and source reservation,
  ∀ (vault insurance budget_remaining source_reserved_atoms : Nat),
    -- PRECONDITIONS
    -- if they are `u128` values (no `requires`),
    U128 vault → U128 insurance → U128 budget_remaining → U128 source_reserved_atoms →
    ⊢ -- then, on normal return of the call,
      callRetireInsurance S vault insurance budget_remaining source_reserved_atoms
        (ifReturns S fun v =>
          -- POSTCONDITIONS
          -- the result is `Ok((retired, next_vault, next_insurance))` with the `ensures` of
          -- K56, or an `Err(_)`.
          U128TripleResult S
            (retire_insurance_ensures vault insurance budget_remaining source_reserved_atoms) v)

/-- K56.nopanic at `T = u64`: `retire_terminal_unbudgeted_insurance_delta(..)` does not
panic. -/
def retire_terminal_unbudgeted_insurance_delta_nopanic : Property := fun S =>
  -- For any vault, insurance, budget remaining and source reservation,
  ∀ (vault insurance budget_remaining source_reserved_atoms : Nat),
    -- PRECONDITIONS
    -- if they are `u128` values (no `requires`),
    U128 vault → U128 insurance → U128 budget_remaining → U128 source_reserved_atoms →
    ⊢ -- then the call does not unwind (panic).
      callRetireInsurance S vault insurance budget_remaining source_reserved_atoms
        (S.returns fun _ => iprop(True))

end Spec.Kernel
