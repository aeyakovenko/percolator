I re-checked all ten properties and their statements are unchanged. All ten proofs still check, and I've marked each one proved again in the Properties table.

- **Spec.KF.scaled_adl_delta_fast_delta** (KF.delta): proved.
- **Spec.KF.f_snap_after_settlement_nopanic** (KF.nopanic): proved.
- **Spec.KF.f_snap_after_settlement_carry_of_pos** (KF.carry): proved, with the precondition `a_basis > 0` that was added before. The original wording fails when `a_basis = 0`; that counterexample is still recorded.
- **Spec.KF.f_snap_after_settlement_ok_exact** (KF.ok): proved. As before, its `Ok` claim also needs `a_basis > 0`; the original wording fails when `a_basis = 0`.
- **Spec.KF.f_snap_after_settlement_cadence** (KF.cadence): proved.
- **Spec.KR.kernel_bankruptcy_residual_step_capacity_contract** (KR.contract): proved.
- **Spec.KR.kernel_bankruptcy_residual_step_capacity_nopanic** (KR.nopanic): proved.
- **Spec.KR.kernel_bankruptcy_residual_step_capacity_fits** (KR.fits): proved.
- **Spec.KR.kernel_bankruptcy_residual_step_capacity_zero** (KR.zero): proved.
- **Spec.KR.kernel_bankruptcy_residual_step_capacity_books** (KR.books): proved.

**How they were checked:** each proof was rebuilt against the code for `src/v16.rs`. It concludes exactly the published statement, contains no unfinished steps, and uses only Lean's standard axioms. The first rebuild attempt stopped partway because the machine ran out of memory, not because of a proof error. The rebuild then completed without errors.

**What the results still assume:** the same library guarantees as before. These are:
- `PartialOrd::gt`/`ge` on `U256` (for KF.nopanic, KF.carry, KF.ok and all KR rows).
- `Option::<U256>::expect` and `Range<usize>::next`/`next_back` (for the KF rows that depend on the division helper).
- the standard Rust environment, in which every object is smaller than \(2^{56}\) bytes.

I did no other work.