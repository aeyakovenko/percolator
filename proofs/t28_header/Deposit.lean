import DepositGraph

namespace Percolator.T28.Mir

theorem depositGraph_checked : safe depositGraph 45 0 ⟨true, false⟩ = true := by
  decide

theorem deposit_preserves_header {State : Type} (Valid : State → Prop)
    {before after : State} (initial : Valid before)
    (trace : Returns Valid depositGraph 0 before false after true) : Valid after := by
  exact safe_sound Valid trace ⟨fun _ => initial, rfl⟩ 45 depositGraph_checked

private def replaceEffects (pc : Nat) (effects : List Effect) : Graph := fun atPc =>
  (depositGraph atPc).map fun b => if atPc = pc then { b with effects } else b

theorem unchecked_success_rejected :
    safe (replaceEffects 42 [.setOk]) 45 0 ⟨true, false⟩ = false := by decide

theorem early_mutation_rejected :
    safe (replaceEffects 7 [.write, .setOk]) 45 0 ⟨true, false⟩ = false := by decide

end Percolator.T28.Mir

#print axioms Percolator.T28.Mir.depositGraph_checked
#print axioms Percolator.T28.Mir.deposit_preserves_header

#print axioms Percolator.T28.Mir.unchecked_success_rejected
#print axioms Percolator.T28.Mir.early_mutation_rejected
