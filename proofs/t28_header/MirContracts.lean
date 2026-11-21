namespace Percolator.T28.Mir

inductive Effect where
  | frame | write | setOk | setErr | checkHeader
  deriving Repr

inductive Exit where
  | jump (targets : List Nat)
  | ret
  | dead

structure Block where
  effects : List Effect
  exit : Exit

structure Facts where
  knownValid : Bool
  resultOk : Bool

abbrev Graph := Nat → Option Block

def outcomes : Effect → Facts → List Facts
  | .frame, f => [f]
  | .write, f => [⟨false, f.resultOk⟩]
  | .setOk, f => [⟨f.knownValid, true⟩]
  | .setErr, _ => [⟨false, false⟩]
  | .checkHeader, _ => [⟨true, true⟩, ⟨false, false⟩]

def blockOutcomes (effects : List Effect) (f : Facts) : List Facts :=
  match effects with
  | [] => [f]
  | e :: rest => (outcomes e f).flatMap (blockOutcomes rest)

def safe (cfg : Graph) : Nat → Nat → Facts → Bool
  | 0, _, _ => false
  | fuel + 1, pc, f =>
    match cfg pc with
    | none => false
    | some b => (blockOutcomes b.effects f).all fun after =>
      match b.exit with
      | .ret => !after.resultOk || after.knownValid
      | .dead => true
      | .jump targets => targets.all fun target => safe cfg fuel target after

section
variable {State : Type} (Valid : State → Prop)

def Agrees (f : Facts) (state : State) (ok : Bool) : Prop :=
  (f.knownValid = true → Valid state) ∧ f.resultOk = ok

/-- The three crucial callee contracts are preservation across frame operations,
    rejection by FromResidual, and a valid resulting header on validator success.
    Unmodeled writes can change the header arbitrarily. -/
def Contract : Effect → State → Bool → State → Bool → Prop
  | .frame, s, ok, t, result => (Valid s → Valid t) ∧ result = ok
  | .write, _, ok, _, result => result = ok
  | .setOk, s, _, t, result => (Valid s → Valid t) ∧ result = true
  | .setErr, _, _, _, result => result = false
  | .checkHeader, _, _, t, result => result = true → Valid t

inductive Effects : List Effect → State → Bool → State → Bool → Prop
  | nil (s ok) : Effects [] s ok s ok
  | cons {e es s ok t status u result} :
      Contract Valid e s ok t status → Effects es t status u result →
      Effects (e :: es) s ok u result

theorem step_sound {e f s ok t result} (incoming : Agrees Valid f s ok)
    (step : Contract Valid e s ok t result) :
    ∃ after ∈ outcomes e f, Agrees Valid after t result := by
  cases e with
  | frame =>
    exact ⟨f, by simp [outcomes], fun h => step.1 (incoming.1 h),
      incoming.2.trans step.2.symm⟩
  | write =>
    exact ⟨⟨false, f.resultOk⟩, by simp [outcomes], by simp, incoming.2.trans step.symm⟩
  | setOk =>
    exact ⟨⟨f.knownValid, true⟩, by simp [outcomes], fun h => step.1 (incoming.1 h),
      step.2.symm⟩
  | setErr =>
    exact ⟨⟨false, false⟩, by simp [outcomes], by simp, step.symm⟩
  | checkHeader =>
    cases result with
    | false => exact ⟨⟨false, false⟩, by simp [outcomes], by simp, rfl⟩
    | true => exact ⟨⟨true, true⟩, by simp [outcomes], fun _ => step rfl, rfl⟩

theorem effects_sound {es f s ok t result} (incoming : Agrees Valid f s ok)
    (steps : Effects Valid es s ok t result) :
    ∃ after ∈ blockOutcomes es f, Agrees Valid after t result := by
  induction steps generalizing f with
  | nil => exact ⟨f, by simp [blockOutcomes], incoming⟩
  | cons step _ ih =>
    obtain ⟨mid, included, agrees⟩ := step_sound Valid incoming step
    obtain ⟨after, follows, agreesAfter⟩ := ih agrees
    exact ⟨after, by simp [blockOutcomes, List.mem_flatMap]; exact ⟨mid, included, follows⟩,
      agreesAfter⟩

/-- A return trace includes any normal successor of a MIR switch.
    Dead blocks and unwinding supply no successful return. -/
inductive Returns (cfg : Graph) : Nat → State → Bool → State → Bool → Prop
  | here {pc b s status t result} :
      cfg pc = some b → Effects Valid b.effects s status t result → b.exit = .ret →
      Returns cfg pc s status t result
  | later {pc b s status t mid u result targets target} :
      cfg pc = some b → Effects Valid b.effects s status t mid → b.exit = .jump targets →
      target ∈ targets → Returns cfg target t mid u result → Returns cfg pc s status u result

theorem safe_sound {cfg pc s status t f}
    (trace : Returns Valid cfg pc s status t true)
    (incoming : Agrees Valid f s status) (fuel : Nat)
    (checked : safe cfg fuel pc f = true) : Valid t := by
  induction fuel generalizing pc s status f with
  | zero => simp [safe] at checked
  | succ fuel ih =>
    cases trace with
    | here found effects exit =>
      obtain ⟨after, seen, agrees⟩ := effects_sound Valid incoming effects
      simp only [safe, found, exit, List.all_eq_true] at checked
      have known := checked after seen
      exact agrees.1 (by simpa [agrees.2] using known)
    | later found effects exit included rest =>
      obtain ⟨after, seen, agrees⟩ := effects_sound Valid incoming effects
      simp only [safe, found, exit, List.all_eq_true] at checked
      exact ih rest agrees (checked after seen _ included)

end
end Percolator.T28.Mir
