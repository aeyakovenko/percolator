#!/usr/bin/env python3
"""gen_kernel_spec_next.py <src/v16.rs of the new revision> <new rev> <base rev> > specs/kernel_contracts_spec_<new rev>.md
The kernel contract spec for the revision after <base rev>: the base spec with its table regenerated
(labels kept by function name through the base list) and a section on what changed since <base rev>.
Stops unless the function-by-function comparison (changed_fns.py) says that no kernel of the base list
needs a new proof and that the new kernels are exactly the ones this script describes."""
import subprocess, sys, os, re
here = os.path.dirname(os.path.abspath(__file__))
src, rev, base = sys.argv[1], sys.argv[2], sys.argv[3]
old = open(os.path.join(here, f"specs/kernel_contracts_spec_{base}.md")).read()
base_list = os.path.join(here, f"specs/kernel_contracts_list_{base}.tsv")
rows = subprocess.run([sys.executable, os.path.join(here, "gen_kernel_list.py"), src, base_list],
                      capture_output=True, text=True, check=True).stdout
table = ["| Label | Function | Line of `fn` in `src/v16.rs` | `requires` | `ensures` | `modifies` |", "|---|---|---|---|---|---|"]
labels = {}
for line in rows.splitlines():
    f = line.split("\t")
    table.append(f"| {f[0]} | `{f[1]}` | {f[2][3:]} | {f[4].split('=')[1]} | {f[5].split('=')[1]} | {f[6].split('=')[1]} |")
    labels[f[0]] = f[1]
base_labels = {l.split("\t")[0]: l.split("\t")[1] for l in open(base_list) if "\t" in l}
new_kernels = {k: v for k, v in labels.items() if k not in base_labels}
EXPECTED_NEW = {"K61": "kernel_provider_protection_num", "K62": "kernel_provider_protection_floor_observe",
                "K63": "kernel_provider_protection_floor_shift"}
if new_kernels != EXPECTED_NEW or any(labels.get(k) != v for k, v in base_labels.items()):
    sys.exit(f"the kernel list is not the base list plus the three kernels this script describes: new {new_kernels}")
cmp_out = subprocess.run([sys.executable, os.path.join(here, "changed_fns.py"),
                          os.path.join(here, f"src_{base}/v16.rs"), src, base_list], capture_output=True, text=True, check=True).stdout
m = re.search(r"## Functions: (\d+) changed, (\d+) new, (\d+) removed", cmp_out)
n_changed, n_new, n_removed = (int(x) for x in m.groups())
need = re.search(r"kernels that need a new proof: \d+: (.*)", cmp_out).group(1).split()
if need:
    sys.exit(f"the comparison says kernels of the base list need a new proof: {need}; write the section for them first")
types = re.findall(r"(?m)^- changed: `(\w+)`$", cmp_out[cmp_out.index("## Types"):])
a = old.index("| Label | Function |")
b = old.index("## Priority, if the run cannot finish all of them")
new = old[:a] + "\n".join(table) + "\n\n" + old[b:]
new, k = re.subn(r"carries \d+ functions annotated", f"carries {len(labels)} functions annotated", new)
assert k == 1
new, k = re.subn(r"\(engine revision \w+\)", f"(engine revision {rev})", new, count=1)
assert k == 1
new += f"""
## What changed in engine revision {rev} (since {base})

`src/v16.rs` is uploaded again with this revision. Nothing proved for `{base}` has to be proved
again in this layer: a function-by-function comparison of the two revisions (every `fn` with its
attributes and body, and every `struct` and `enum`) found {n_changed} functions with changed text, {n_new} new
ones and {n_removed} removed, and {len(types)} changed types ({', '.join('`' + t + '`' for t in types)}). No kernel of this list has changed
text, calls a changed or new function (directly or through its callees, matched by name), or
takes, returns or names a changed type. The changed types are the market view and the per-asset
account slot; kernels take scalars and the runtime records, which did not change.

Three kernels are new. They carry the provider-protection floor of a source domain: provider
protection is the provider principal that neither a claim on the domain nor an unrepaid
consumption has taken, and the floor is the lowest protection the domain has had, moved only by
the provider's own deposits and withdrawals. All amounts are `u128` and none of the three can fail.

- K61 `kernel_provider_protection_num(principal, reserved, claims)` returns
  `min(principal, reserved - claims)` with the difference floored at zero. Contract: the result is
  at most `principal`; if `reserved >= claims` it is at most `reserved - claims` and equals one of
  the two; otherwise it is 0.
- K62 `kernel_provider_protection_floor_observe(floor, protection)` returns `min(floor, protection)`.
  Contract: the result is at most both and equals one of them.
- K63 `kernel_provider_protection_floor_shift(floor, amount, add)` returns `floor + amount` or
  `floor - amount`, saturating. Contract: when `add`, the result is at least `floor` and exceeds it
  by exactly `amount` unless it is `u128::MAX`; otherwise it is at most `floor` and below it by
  exactly `amount` unless it is 0.

Four consequences are worth proving as their own properties, for all inputs:

- K61.realization: consuming `x <= reserved` of backing together with burning `x` of claims leaves
  the protection unchanged when `claims >= x`
  (`protection(p, r - x, c - x) == protection(p, r, c)`).
- K61.monotone: protection does not fall when `reserved` rises or `claims` falls, and does not rise
  when a claim is added.
- K62.minimum: observing a sequence of protections in any order leaves the minimum of the starting
  floor and all of them.
- K63.inverse: a deposit of `a` followed by a withdrawal of `a` leaves the floor unchanged unless the
  deposit saturated. With `floor <= principal`, adding `a` to both keeps `principal - floor`
  unchanged unless an addition overflows, and subtracting `a <= principal` from the principal and
  shifting the floor down by `a` keeps it unchanged exactly when `a <= floor` (otherwise the floor
  stops at 0 and the difference becomes `principal - a`).
"""
sys.stdout.write(new)
