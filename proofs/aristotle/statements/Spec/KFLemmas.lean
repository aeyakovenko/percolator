module

public import Spec.Rust.KF
public import Spec.WideMathDiv2

/-!
# Read-only data of the `wide_math` callees (engine revision `bfd4d0c4`)

The functions of `src/v16.rs` verified in this task call functions of `src/wide_math.rs`
(`floor_div_signed_conservative_i128`, `mul_div_floor_u256_with_rem`) that read string literals
and a promoted constant placed by the program at start-up. `kfDivGlobals` names them; it is a
precondition of the properties of `Spec.KF`, as `Spec.WideMath.divGlobals` was of the earlier
results.

The properties of `Spec.KF` and `Spec.KR` also take, unchanged, the assumed documented behaviour
of core-library functions that the earlier `wide_math` results themselves assumed
(`Spec.WideMath.U512DivLib`, `Spec.WideMath.DivRemLib`): those name the library functions by the
same handles, which denote the same functions in this build.
-/

@[expose] public section

namespace Spec.KF
open Iris Iris.BI Rust2Lean.Public
open Spec.WideMath (litAt)

/-- The read-only data of `src/wide_math.rs` that this build's `floor_div_signed_conservative_i128`
and `mul_div_floor_u256_with_rem` read: the promoted constant `Ordering::Less` and the string
literals of their panic and `expect` messages. Every such global is placed by the program at
start-up; this is an environment fact, not a restriction on inputs. (It is the part of the
earlier `Spec.WideMath.divGlobals` that exists in this build.) -/
def kfDivGlobals {PROP : Type} [BI PROP] (S : Vocab PROP) : PROP :=
  iprop(S.global Mir.KF.Global.«const [1 bytes] 165fc9f0fda32787» 1 [255] ∗
    litAt S Mir.KF.Global.«"remainder must fit U256"» "remainder must fit U256" ∗
    litAt S Mir.KF.Global.«"U256 division by zero"» "U256 division by zero" ∗
    litAt S Mir.KF.Global.«"U512 division by zero"» "U512 division by zero" ∗
    litAt S Mir.KF.Global.«"mul_div quotient must fit U256"» "mul_div quotient must fit U256" ∗
    litAt S Mir.KF.Global.«"mul_div_floor_u256_with_rem: zero denominator"»
      "mul_div_floor_u256_with_rem: zero denominator" ∗
    litAt S Mir.KF.Global.«"floor_div_signed_conservative_i1"… [52 bytes] f6ba71dbb1700623»
      "floor_div_signed_conservative_i128: zero denominator" ∗
    litAt S Mir.KF.Global.«"floor_div_signed_conservative_i1"… [55 bytes] 2550b6a279c23806»
      "floor_div_signed_conservative_i128: result out of range")

end Spec.KF
