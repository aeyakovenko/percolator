#!/usr/bin/env python3
"""gen_kernel_spec.py <src/v16.rs of the new revision> <engine revision label> > kernel_contracts_spec for that revision
Rebuild specs/kernel_contracts_spec.md for another engine revision: same text, regenerated table (labels kept by
function name through gen_kernel_list.py), and a section naming what changed since the revision the project held."""
import subprocess, sys, os, re
here = os.path.dirname(os.path.abspath(__file__))
src, rev = sys.argv[1], sys.argv[2]
old = open(os.path.join(here, "specs/kernel_contracts_spec.md")).read()
rows = subprocess.run([sys.executable, os.path.join(here, "gen_kernel_list.py"), src,
                       os.path.join(here, "specs/kernel_contracts_list.tsv")], capture_output=True, text=True, check=True).stdout
table = ["| Label | Function | Line of `fn` in `src/v16.rs` | `requires` | `ensures` | `modifies` |", "|---|---|---|---|---|---|"]
n = 0
for line in rows.splitlines():
    f = line.split("\t")
    table.append(f"| {f[0]} | `{f[1]}` | {f[2][3:]} | {f[4].split('=')[1]} | {f[5].split('=')[1]} | {f[6].split('=')[1]} |")
    n += 1
a = old.index("| Label | Function |")
b = old.index("## Priority, if the run cannot finish all of them")
new = old[:a] + "\n".join(table) + "\n\n" + old[b:]
new = re.sub(r"carries \d+ functions annotated", f"carries {n} functions annotated", new)
# What still stands from the revision the recorded proofs were made against (base_rev/v16_add1d027.rs, engine
# master add1d027): computed, not asserted from memory.
import tempfile
with tempfile.NamedTemporaryFile("w", suffix=".tsv", delete=False) as tf:
    tf.write(rows)
cmp_out = subprocess.run([sys.executable, os.path.join(here, "changed_fns.py"), os.path.join(here, "base_rev/v16_add1d027.rs"), src, tf.name],
                         capture_output=True, text=True, check=True).stdout
os.unlink(tf.name)
m = re.search(r"## Functions: (\d+) changed, (\d+) new, (\d+) removed", cmp_out)
n_changed, n_new, n_removed = (int(x) for x in m.groups())
need = re.search(r"kernels that need a new proof: \d+: (.*)", cmp_out).group(1).split()
EXPECTED = ["K02", "K03", "K07", "K26", "K32", "K34", "K35", "K59", "K60"]
if need != EXPECTED:
    sys.exit(f"the comparison names other kernels than the text of this spec does: {need}; rewrite the section first")
new = new.replace("# Specification: the engine's kernel contracts",
                  f"# Specification: the engine's kernel contracts (engine revision {rev})")
new += f"""
## What changed in engine revision {rev}

`src/v16.rs` is uploaded again with this revision. Proofs made against the earlier revision stay
valid for every function whose body, contract and reachable callees did not change and whose types
did not change. Nine functions need work: six with new or changed text, and three whose text is the
same but which take the asset record, which gained two fields.

- K59 `kernel_side_adl_index` and K60 `kernel_raw_basis_after_adl_reduction` are new. Each side of an
  asset now keeps the sum of its legs' raw basis (`raw_basis_sum_long_q`, `raw_basis_sum_short_q`),
  and the side's A index is derived from that sum and the side's effective open interest.
- K07 `kernel_resize_leg_same_side`, K32 `kernel_retain_leg_as_pending_obligation`, K34
  `kernel_clear_leg` and K35 `kernel_attach_leg` maintain the new sum, and their contracts gained
  clauses about it. K35 also requires and ensures `a_basis == ADL_ONE`.
- K02 `kernel_mark_kf_stale_cohorts`, K03 `kernel_settle_kf_stale_cohort` and K26
  `apply_bankruptcy_residual_chunk_to_loss_side` are textually unchanged, but they take or return
  `AssetStateV16`, which now has the fields `raw_basis_sum_long_q` and `raw_basis_sum_short_q`. Their
  properties have to be established again on the new record (their contracts say which fields move;
  the two new fields must be among the ones that do not).

No other listed function needs work. A function-by-function comparison of the two revisions (every
`fn` with its attributes and body, and every `struct` and `enum`) found {n_changed} functions with changed
text, {n_new} new ones and {n_removed} removed, and two changed types (`AssetStateV16` and its account form
`AssetStateV16Account`). Apart from the nine named above, no kernel of this list has changed text, calls
a changed or new function (directly or through its callees, matched by name), or takes, returns or
names a changed type. K50 to K58 are associated functions of the market view without a `self`
parameter: they take scalars only, so the larger asset record does not reach them.

Two consequences of K59 and K60 are worth proving as their own properties, for all inputs with
`raw_basis_sum_q <= 2 * MAX_OI_SIDE_Q` (the engine never holds more raw basis on a side):

- K59.exposure: when `kernel_side_adl_index(oi, raw_sum, a)` returns `Ok(A)` with `raw_sum > 0` and
  `oi > 0`, then `A * raw_sum <= ADL_ONE * oi < (A + 1) * raw_sum`. (This is the contract's own
  clause; it is the statement that the legs' exact exposure never exceeds the side's effective open
  interest and is short of it by less than `raw_sum / ADL_ONE`.)
- K60.progress: when `kernel_raw_basis_after_adl_reduction(raw, e, e2, oi, raw_sum)` returns
  `Ok(next_raw)`, then `1 <= next_raw < raw` and `oi - (e - e2) <= (raw_sum - raw) + next_raw`,
  and if `oi == raw_sum` then `next_raw == e2`.
"""
sys.stdout.write(new)
