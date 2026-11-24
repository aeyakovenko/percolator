#!/usr/bin/env python3
"""Build the proof ledger: every charter invariant split into provable pieces, with a status each.

  make_ledger.py            reads the live Aristotle property list (needs ~/.aristotle)
  make_ledger.py --offline  uses the last saved list (properties.json)

Inputs (all in this directory unless noted):
  coverage_matrix.md                 invariant -> stage and the kernels that carry it
  specs/kernel_contracts_list.tsv    the engine's contract-annotated kernels (K01..K58)
  specs/wide_math_spec.md            the arithmetic properties (A1..G5)
  engine source (ENGINE_SRC)         public entry points
  wrapper source (WRAPPER_SRC)       instruction variants
Outputs: proof_ledger.md, proof_ledger.tsv, properties.json
"""
import json, os, re, subprocess, sys, datetime

HERE = os.path.dirname(os.path.abspath(__file__))
# Frozen copies of the landed revisions (LANDED_REV, WRAPPER_REV). The worktrees move on with work in progress.
# In the engine repository this script and the data it reads and writes live in proofs/,
# and the engine source is the checkout itself (which must be at LANDED_REV); the program source comes
# from a checkout of percolator-prog at WRAPPER_REV, named by LEDGER_WRAPPER_SRC.
_here = os.path.dirname(os.path.abspath(__file__))
_frozen = os.path.join(_here, "src_263c34d4/v16.rs")
ENGINE_SRC = os.environ.get("LEDGER_ENGINE_SRC") or (_frozen if os.path.exists(_frozen) else os.path.join(_here, "..", "src", "v16.rs"))
WRAPPER_SRC = os.environ.get("LEDGER_WRAPPER_SRC") or os.path.join(_here, "wrapper_0cb5794e/v16_program.rs")
if not os.path.exists(WRAPPER_SRC):
    sys.exit("make_ledger.py: set LEDGER_WRAPPER_SRC to src/v16_program.rs of a percolator-prog checkout at the pinned revision")
# The Aristotle project id is account data and is not kept in this file: set ARISTOTLE_PROJECT, or put
# it in a file `.aristotle_project` next to this script (not published).
_pf = os.path.join(os.path.dirname(os.path.abspath(__file__)), ".aristotle_project")
PROJECT = os.environ.get("ARISTOTLE_PROJECT") or (open(_pf).read().strip() if os.path.exists(_pf) else "")
PROVED_AT = "engine add1d027"  # the revision the proofs before the #449 engine change were made against
NEW_REV = "c030aa40"            # the #449 engine change, master from 2026-11-20 06:58 to the landing of LANDED_REV
# The engine revision this ledger describes: the provider-protection floor (percolator#271). ENGINE_SRC
# is this revision. Nothing proved for NEW_REV in layers A and K has to be proved again for it:
# specs/changed_since_c030aa40_at_d1fa39f6.md (changed_fns.py) finds no listed kernel with changed text,
# a changed or new callee, or a changed type, and src/wide_math.rs is identical. The entry-point
# pieces proved for NEW_REV do have to be established again (REESTABLISH): their text is unchanged,
# but they are methods over, or build, a type that gained fields.
# Rolled 2026-11-21 to cb78f1f0, the first two batches of the refactor that moves the logic of the
# engine's methods into pure kernels (specs/changed_since_c030aa40_at_cb78f1f0.md and
# specs/changed_since_d1fa39f6_at_cb78f1f0.md: no listed kernel changed; only private methods changed,
# and ten kernels K64 to K73 are new, each verified by Kani for every input).
PREV_LANDED_REV = "d1fa39f6"   # the revision K61-K63 are new in
CB_REV = "cb78f1f0"            # refactor batches 1-2: K64-K73
# Rolled 2026-11-21 to 80f8f0b4: refactor batches 3-9 (K74-K93, of which fourteen are verified by Kani for every
# input and six only for bounded inputs, with their full-width proofs by Aristotle: K91 task k1, K93 task k2, K88
# task k3, K76-K78 task k4) and config admission refusing a public B chunk above MAX_VAULT_TVL.
# specs/changed_since_cb78f1f0_at_80f8f0b4.md: no listed kernel changed; its flag on K35 is a false positive (the
# callee add_open_interest_for_new_position gained only a cfg(kani) contract attribute; the deployed text is equal).
B39_REV = "80f8f0b4"            # refactor batches 3-9: K74-K93
# Rolled 2026-11-22 to 08e8f7f0: refactor batch 12. The public B chunk is the constant PUBLIC_B_CHUNK_ATOMS =
# MAX_VAULT_TVL instead of a market setting (the methods that read the setting read the constant, and the config
# types lost the field), and K78's contract states its funding floor with div_euclid, the clause the prover proved
# in task k4. specs/changed_since_80f8f0b4_at_08e8f7f0.md: no listed kernel changed (K78's contract attribute did,
# which the comparison does not see; its body did not); transition_kinds.08e8f7f0.py: five public entry points
# changed only in the chunk argument, classes unchanged. All three Kani runs were made in full on the batch.
# Rolled 2026-11-22 to 4e77481b: refactor batch 13. Both accrual entry points end their read-only prologue with
# a call to the rest of the accrual, moved verbatim into a private #[inline(always)] function, so that Kani proves
# the price check (INV-045 C0) for every price. specs/changed_since_08e8f7f0_at_4e77481b.md: two entry points
# changed, two private functions new, no listed kernel changed or reaches them; transition_kinds.4e77481b.py:
# the two entry points re-read, classes unchanged. Kani: the harnesses that reach the change were run again in
# all three layers with the three new proofs; every other verdict carries from batch 12's full runs.
# Rolled 2026-11-22 to 71076137: batch 14 (06cc3b91, fifteen no-overflow Kani proofs) and batch 15. A target staged by
# set_asset_raw_oracle_targets_not_atomic that changes now restarts the path accrual's price-cap anchor at the effective
# price (INV-045). specs/changed_since_4e77481b_at_71076137.md: that setter is the one function changed; no listed
# kernel changed or reaches it; transition_kinds.71076137.py: the setter re-read, class final. Kani: the 4 harnesses
# that reach the setter (the new proof among them) were run again in all three layers; every other verdict carries.
# Rolled 2026-11-24 to 06b7e5be: the INV-077 fee-sync fix. A fee sync whose side-effect settlement has nothing to settle
# skips it, writes the loss-stale summary itself and keeps a current certificate with the charge taken from its equity
# (new kernel K94, contract-checked) when that leaves no liquidation deficit; otherwise it voids it as the settlement
# would. specs/changed_since_71076137_at_06b7e5be.md: the fee sync is the one function changed, three are new, no listed
# kernel changed or reaches them; transition_kinds.06b7e5be.py: the fee sync re-read, class final; two new loop counters,
# both rule R1. Kani: the 5 harnesses that reach the change (the contract check among them) were run again in every
# layer that builds them; every other verdict carries.
# Rolled 2026-11-24 to 263c34d4: engine issue #274. The permissionless crank's B chunk (`SettleBChunk`) settles the
# account's pending K/F first, as the refresh, the side-effect settlement and the Recovery forfeit do; before, a
# winner's B share could land ahead of its gain and the gain cured it from loser-paid backing nobody was owed, so crank
# order decided what providers and winners kept. specs/changed_since_06b7e5be_at_263c34d4.md: one function changed
# (permissionless_crank_not_atomic, pub(crate), no entry point of its own), no type, no kernel; the same arithmetic.
# Kani: the 5 harnesses that reach the change were run again in every layer that builds them; every other verdict carries.
LANDED_REV = "263c34d4"
REESTABLISH = {"T09.header", "T38.header"}
# Kernels whose proof has to be made again for NEW_REV: specs/changed_since_add1d027_at_c030aa40.md
# (changed_fns.py: own text, callees by name, changed types). Every other kernel, its callees and its
# types are textually identical in both revisions, and src/wide_math.rs is identical, so those proofs
# hold for NEW_REV as they stand.
REPROVE = {"K02", "K03", "K07", "K26", "K32", "K34", "K35", "K59", "K60"}
# UTC time the task for NEW_REV was submitted (written by the submission step); a row of a REPROVE
# kernel counts for NEW_REV only when Aristotle touched it after that.
_since = os.path.join(HERE, "b6_submitted_at.txt")
REPROVE_SINCE = open(_since).read().strip() if os.path.exists(_since) else None

def read(path):
    return open(os.path.join(HERE, path) if not path.startswith("/") else path).read()

# ---------------------------------------------------------------- Aristotle statuses
def aristotle_properties(offline):
    """Every declaration of the project: name, status, and whether its final check ran."""
    cache = os.path.join(HERE, "properties.json")
    if not offline:
        env = dict(os.environ)
        env["ARISTOTLE_API_KEY"] = os.environ.get("ARISTOTLE_API_KEY") or open(os.path.expanduser("~/.aristotle")).read().strip()
        import shutil
        out = subprocess.run([shutil.which("aristotle") or os.path.expanduser("~/.local/bin/aristotle"), "properties", PROJECT,
                              "--status", "ALL", "--json"], capture_output=True, text=True, env=env).stdout
        try:
            decls = json.loads(out)["declarations"]
        except Exception:
            decls = []
        if decls:
            json.dump(decls, open(cache, "w"), indent=1)
    rows = []
    for d in json.load(open(cache)):
        status = d.get("status_name") or ""
        if status == "PROVED" and d.get("unchecked"):
            status = "PROVED (unchecked)"
        rows.append({"name": d["name"], "status": status, "description": d.get("description") or "",
                     "last_updated": d.get("last_updated") or ""})
    return rows

# ---------------------------------------------------------------- which code is in the deployed build
SRC_LINES = read(ENGINE_SRC).split("\n")
def cfg_gate(fn_name):
    """The `#[cfg(..)]` directly above `fn_name` (above its attributes and doc comment), if any.
    Functions under `cfg(any(kani, feature = "fuzz"))` are not compiled into the deployed program."""
    for i, line in enumerate(SRC_LINES):
        if re.match(r"\s*(pub(\([a-z]+\))? )?(const )?fn " + re.escape(fn_name) + r"\b", line):
            j = i - 1
            while j >= 0:
                t = SRC_LINES[j].strip()
                if re.match(r"(pub(\([a-z]+\))? )?(const )?fn |\}$|impl ", t) or t == "":
                    break
                m = re.match(r"#\[cfg\((.*)\)\]$", t)
                if m:
                    return m.group(1)
                j -= 1
            return None
    return None
NOT_DEPLOYED = "not in the deployed build"

# ---------------------------------------------------------------- pieces
pieces = {}   # id -> dict(layer, target, statement, method, status, evidence)
def piece(pid, layer, target, statement, method, status="open", evidence=""):
    pieces[pid] = dict(id=pid, layer=layer, target=target, statement=statement, method=method,
                       status=status, evidence=evidence)

# A: arithmetic
spec = read("specs/wide_math_spec.md")
for m in re.finditer(r"(?m)^- ([A-G]\d+b?)\. (.*(?:\n  .*)*)", spec):
    text = " ".join(m.group(2).split())
    piece(m.group(1), "A arithmetic", "wide_math.rs", text[:150] + ("..." if len(text) > 150 else ""), "Aristotle")

# K: kernels
kernels = {}
for line in read("specs/kernel_contracts_list.tsv").splitlines():
    f = line.split("\t")
    if len(f) >= 2:
        kernels[f[0]] = f[1]
kernels["K59"] = "kernel_side_adl_index"                    # new in the #449 engine change
kernels["K60"] = "kernel_raw_basis_after_adl_reduction"     # new in the #449 engine change
kernels["K61"] = "kernel_provider_protection_num"            # new in LANDED_REV (provider-protection floor)
kernels["K62"] = "kernel_provider_protection_floor_observe"  # new in LANDED_REV
kernels["K63"] = "kernel_provider_protection_floor_shift"    # new in d1fa39f6
# new in cb78f1f0 (refactor batches 1-2, K64-K73) and in 80f8f0b4 (batches 3-9, K74-K93)
REFACTOR_KERNELS = {}
# Kani checks these only for bounded inputs (specs/kernel_contracts_spec_80f8f0b4.md); the prover proves them at full width
BOUNDED_KERNELS = {"K76", "K77", "K78", "K88", "K91", "K93"}
for _line in read("specs/kernel_contracts_list_263c34d4.tsv").splitlines():
    _f = _line.split("\t")
    if len(_f) >= 2 and int(_f[0][1:]) >= 64:
        REFACTOR_KERNELS[_f[0]] = _f[1]
        kernels[_f[0]] = _f[1]
changed_by_449 = {"K07", "K32", "K34", "K35"}
for k, fn in kernels.items():
    note = ""
    if k in changed_by_449:
        note = " (contract extended by the #449 engine change: prove against the new revision)"
    if k in ("K59", "K60"):
        note = " (new in the #449 engine change; Kani covers an 8-bit grid only)"
    if k in ("K61", "K62", "K63"):
        note = f" (new in engine {PREV_LANDED_REV}; its Kani contract check verifies for all inputs)"
    if k in REFACTOR_KERNELS:
        _rev = CB_REV if int(k[1:]) <= 73 else (B39_REV if int(k[1:]) <= 93 else "06b7e5be")
        if k in BOUNDED_KERNELS:
            note = f" (new in engine {_rev}; its Kani contract check covers bounded inputs only, the full-width proof is the prover's)"
        else:
            note = f" (new in engine {_rev}; its Kani contract check verifies for all inputs)"
    gate = cfg_gate(fn)
    piece(f"{k}.contract", "K kernel", fn, f"`{fn}` satisfies its requires/ensures contract for every input{note}", "Aristotle")
    piece(f"{k}.nopanic", "K kernel", fn, f"no execution of `{fn}` panics under its requires", "Aristotle")
    if gate and "kani" in gate:
        for suffix in ("contract", "nopanic"):
            pieces[f"{k}.{suffix}"]["status"] = NOT_DEPLOYED
            pieces[f"{k}.{suffix}"]["evidence"] = f"compiled only under cfg({gate}); it serves a test-only entry point"
# Two named consequences of the new kernels (last section of specs/kernel_contracts_spec_<rev>.md).
piece("K59.exposure", "K kernel", "kernel_side_adl_index",
      "when `kernel_side_adl_index(oi, raw_sum, a)` returns `Ok(A)` with `raw_sum > 0` and `oi > 0`: "
      "`A * raw_sum <= ADL_ONE * oi < (A + 1) * raw_sum`, so the legs' exact exposure never exceeds the side's "
      "effective OI and is short of it by less than `raw_sum / ADL_ONE` (the #449 bound)", "Aristotle")
piece("K60.progress", "K kernel", "kernel_raw_basis_after_adl_reduction",
      "when `kernel_raw_basis_after_adl_reduction(raw, e, e2, oi, raw_sum)` returns `Ok(next_raw)`: "
      "`1 <= next_raw < raw`, `oi - (e - e2) <= (raw_sum - raw) + next_raw`, and `next_raw == e2` when `oi == raw_sum`",
      "Aristotle")
# Four named consequences of the provider-protection kernels (last section of specs/kernel_contracts_spec_d1fa39f6.md).
piece("K61.realization", "K kernel", "kernel_provider_protection_num",
      "consuming `x` of backing together with burning `x` of claims leaves the protection unchanged: "
      "`protection(p, r - x, c - x) == protection(p, r, c)` for `x <= r` and `x <= c`", "Aristotle")
piece("K61.monotone", "K kernel", "kernel_provider_protection_num",
      "protection does not fall when the reserved backing rises or the claims fall, and does not rise when a claim is added",
      "Aristotle")
piece("K62.minimum", "K kernel", "kernel_provider_protection_floor_observe",
      "observing a sequence of protections in any order leaves the minimum of the starting floor and all of them", "Aristotle")
piece("K63.inverse", "K kernel", "kernel_provider_protection_floor_shift",
      "a deposit of `a` followed by a withdrawal of `a` leaves the floor unchanged unless the deposit saturated; with "
      "`floor <= principal`, shifting both down by `a <= principal` keeps `principal - floor` unchanged exactly when `a <= floor`",
      "Aristotle")
EXTRA_K = {("kernel_side_adl_index", "exposure"): "K59.exposure",
           ("kernel_raw_basis_after_adl_reduction", "progress"): "K60.progress",
           ("kernel_provider_protection_num", "realization"): "K61.realization",
           ("kernel_provider_protection_num", "monotone"): "K61.monotone",
           ("kernel_provider_protection_floor_observe", "minimum"): "K62.minimum",
           ("kernel_provider_protection_floor_shift", "inverse"): "K63.inverse"}

# T: transitions (one per public engine entry point)
sys.path.insert(0, HERE)
src = read(ENGINE_SRC)
def entry_bodies(src):
    """name -> body text of each public `*_not_atomic` entry point (brace matched)."""
    out = {}
    for m in re.finditer(r"(?m)^    pub fn ([a-z_0-9]+_not_atomic)", src):
        i = src.index("{", src.index(")", m.end()))
        depth, j = 0, i
        while True:
            c = src[j]
            if c == "{":
                depth += 1
            elif c == "}":
                depth -= 1
                if depth == 0:
                    break
            j += 1
        out.setdefault(m.group(1), src[i:j + 1])
    return out
bodies = entry_bodies(src)
entries = sorted(bodies)
from transition_kinds import classify
T_ROWS = {r["name"]: r for r in classify(src)}
assert sorted(T_ROWS) == entries, "transition_kinds.py and this file see different entry points"
t_kind = {}
for i, e in enumerate(entries, 1):
    assert T_ROWS[e]["label"] == f"T{i:02d}"
    cls, note = T_ROWS[e]["cls"], T_ROWS[e]["note"]
    how = {
        "final": "every Ok path ends in `validate_shape()` after its last market write, so the predicate holds by a runtime "
                 "check; the obligation is that path structure (Aristotle, small)",
        "delegates": "a call to an entry point of class final; inherits its obligation (Aristotle, small)",
        "local": "no shape check; follows from guards inside the function for every state (Aristotle, small real proof)",
        "frame": "no shape check; the function writes no header field a check reads (Aristotle, from its write set)",
        "mixed": "final on most Ok paths; the paths named in the statement end without a check (Aristotle: structural part "
                 "small, the named paths as their class says)",
        "real": "no shape check in a default build; needs kernel contracts and invariants relating the header to slots and "
                "accounts (Aristotle, large; split by writer in specs/header_write_sets_<rev>.md); today: Kani closure and "
                "composition harnesses and audit-scan fuzz",
        "dispatch": "the conjunction of the obligations of the functions its branches call, four of them internal and of "
                    "class real (Aristotle, after those)",
        "not deployed": "not in the deployed build",
    }[cls]
    t_kind[e] = cls
    piece(f"T{i:02d}.header", "T transition", e,
          f"[{cls}] if the header predicate (`validate_shape` of a default build) held before `{e}` and it returns Ok, it holds "
          f"of the state it leaves" + (f". Found when read: {note}" if note else ""),
          how)
    piece(f"T{i:02d}.scan", "T transition", e,
          f"if the per-asset scan predicate (`validate_shape_full_audit_scan`) held before `{e}` and it returns Ok, it holds after",
          "Aristotle from the kernel contracts (frame for the slots it does not write, kernel deltas for the ones it does); "
          "today: audit-scan fuzz, Kani closure and composition harnesses")
    piece(f"T{i:02d}.account", "T transition", e,
          f"every portfolio account `{e}` is given passes `validate_with_market` against the market it leaves, if it did before and the call returns Ok",
          "Aristotle (the entry points that end in `account.validate_with_market(..)` are structural, like the header half); "
          "today: audit-scan fuzz and the account audits of the program")
    piece(f"T{i:02d}.mirror", "T transition", e,
          f"`{e}` keeps every market total in step with the accounts it is given: for each mirrored total (capital, positive PnL and "
          "its bound, source claims per domain, stored positions, raw basis and loss weight per side, stale and negative-PnL counts) "
          "the total minus the given accounts' contribution is unchanged (specs: global invariant, part M)",
          "Aristotle from the kernel contracts; today: the program's reference-model census and the stateful fuzz")
    gate = cfg_gate(e)
    if gate and "kani" in gate:
        for suffix in ("header", "scan", "account", "mirror"):
            pieces[f"T{i:02d}.{suffix}"]["status"] = NOT_DEPLOYED
            pieces[f"T{i:02d}.{suffix}"]["evidence"] = f"compiled only under cfg({gate}): a test and proof API, absent from the program"

# P: panic sites, the calls from src/v16.rs into src/wide_math.rs (gen_callsite_spec.py)
P_SPEC = f"specs/wide_math_callsites_spec_{LANDED_REV}.md"
p_none = []
if os.path.exists(os.path.join(HERE, P_SPEC)):
    for m in re.finditer(r"(?m)^\| (P\d\d) \| (\d+) \| `(\w+)` \| `(\w+)` \((\w+)\) \| (\w+) \| ([^|]*) \|", read(P_SPEC)):
        pid, line, fn, callee, label, cls, why = m.groups()
        piece(pid, "P panic sites", fn,
              f"the call to `{callee}` at line {line} cannot meet its panic condition ({label}); class {cls}: {why.strip()}",
              "Aristotle, from the exact panic conditions of the arithmetic layer" if cls != "none" else f"follows from {label}")
        if cls == "none":
            p_none.append((pid, label))
    for pid, target, text in (
        ("PL1", "PortfolioLegV16Account::try_to_runtime", "an active leg decoded from account data passed `validate_active_leg`"),
        ("PL2", "leg_kf_delta_components_for_settlement_from_asset", "every caller in a default build passes an active leg it decoded with `try_to_runtime`"),
        # Added 2026-11-23 with batch 19: PO.signed_position is verified under the assumption that an active leg's
        # basis_pos_q is not i128::MIN; that every caller passes such a leg is this piece (a reading, like PL2).
        ("PL3", "signed_position", "every deployed caller passes `PortfolioLegV16::EMPTY` or a leg decoded with `try_to_runtime` (six call sites), so an active leg's `basis_pos_q` is not `i128::MIN` (PL1 and `validate_basis`)"),
        ("PR1", "kernel_recompute_resolved_payout_rate", "the recomputed payout rate has `1 <= den` and `num <= den`"),
        ("PR2", "writers of header.resolved_payout_ledger", "every writer of the payout ledger leaves the rate unchanged or ends with a recompute"),
        ("PR3", "payout_snapshot_captured", "the snapshot flag is set only by the function that initializes the ledger and recomputes the rate"),
        ("PR4", "resolved payout receipts", "a present receipt is written only after the ledger was initialized; callers pass the header's ledger"),
    ):
        piece(pid, "P panic sites", target, text, "Aristotle (PL: leg validity; PR: the ledger-rate invariant)")

# P.surface: what src/v16.rs can reach in wide_math.rs at all (compiler census, census_wide_surface.sh)
_census = os.path.join(HERE, "specs", f"wide_surface_census_{LANDED_REV}.txt")
_census_ok = os.path.exists(_census) and open(_census).read().startswith(f"OK {LANDED_REV}")
piece("P.surface", "P panic sites", "src/v16.rs",
      "`src/v16.rs` reaches `wide_math.rs` only through five free functions and these `U256` items: `ONE`, `from_u128`, "
      "`from_u64`, `is_zero`, `try_into_u128`, `checked_add/sub/mul/div/rem`, equality and ordering; each of those items "
      "never panics (A1 to A4, B1 to B4)",
      "compiler census (the file builds against a stand-in module exposing only these items) plus the arithmetic layer",
      status="proved" if _census_ok else "open",
      evidence=(f"specs/wide_surface_census_{LANDED_REV}.txt (cargo check --lib, default features) with A1-A4, B1-B4; the comparison "
                "operators are the core library's provided methods over the proved `partial_cmp`") if _census_ok else "")

# PO: the engine's unchecked arithmetic (wrap_census.py). The program's release profile checks the program's own
# arithmetic and builds the engine without overflow checks; every proof was made with them on, so an "if it returns"
# statement holds on-chain only where the operations it runs cannot overflow. One piece per engine function whose
# unchecked operations no proved no-panic piece of its own covers.
# Every engine roll must regenerate the census for its revision (wrap_census.py; the clippy sites are per revision).
_wrap = f"specs/engine_wrapping_arithmetic_{LANDED_REV}.tsv"
assert os.path.exists(os.path.join(HERE, _wrap)), f"missing {_wrap}: run wrap_census.py for {LANDED_REV}"
for _row in read(_wrap).splitlines()[1:]:
    _fn, _line, _n, _lines, _ops, _cov = _row.split("\t")
    if _cov != "-":
        continue
    piece(f"PO.{_fn}", "P panic sites", _fn,
          f"no unchecked `+`, `-`, `*`, shift or negation in `{_fn}` overflows on an input its deployed callers can "
          f"pass ({_n} at line{'s' if _n != '1' else ''} {_lines.replace(',', ', ')}): the deployed build does not "
          "check the engine's arithmetic, so an overflow there would continue with a wrapped value",
          "Kani or Aristotle (a no-panic proof, overflow checks on), or checked arithmetic that returns an error")

# PO pieces closed by Kani (batch 14, engine 06cc3b91, whose src/v16.rs is 4e77481b's): each harness calls the function with
# every input symbolic and no assumption, closure layer, so Kani's overflow checks cover every operation the call runs.
PO_KANI_REV = "06cc3b91"
PO_KANI = {
    "PO.liquidation_risk_notional_ceil": "proof_no_overflow_liquidation_risk_notional_ceil",
    "PO.active_bitmap_is_empty": "proof_no_overflow_active_bitmap_is_empty",
    "PO.active_bitmap_count_ones": "proof_no_overflow_active_bitmap_count_ones",
    "PO.V16Core::source_claim_domain_first_burn_partition": "proof_no_overflow_source_claim_domain_first_burn_partition",
    "PO.V16Core::kernel_settle_positive_face_after_support": "proof_no_overflow_kernel_settle_positive_face_after_support",
    "PO.V16Core::kernel_principal_cure_close_ledger": "proof_no_overflow_kernel_principal_cure_close_ledger",
    "PO.V16Core::kernel_advance_close_ledger": "proof_no_overflow_kernel_advance_close_ledger",
    "PO.V16Core::prepare_counterparty_backing_expiry_delta": "proof_no_overflow_prepare_counterparty_backing_expiry_delta",
    "PO.V16Core::target_effective_lag_adverse_delta": "proof_no_overflow_target_effective_lag_adverse_delta",
    "PO.liquidation_fee_for_close": "proof_no_overflow_liquidation_fee_for_close",
    "PO.adjust_u128": "proof_no_overflow_adjust_u128",
    "PO.MarketGroupV16ViewMut::apply_total_delta_u64": "proof_no_overflow_apply_total_delta_u64",
    "PO.MarketGroupV16HeaderAccount::dynamic_asset_slot_capacity_from_account_len": "proof_no_overflow_dynamic_asset_slot_capacity_from_account_len",
    "PO.insert_account_kf_settlement_plan_entry": "proof_no_overflow_insert_account_kf_settlement_plan_entry",
    "PO.TokenValueFlowProofV16::validate": "proof_no_overflow_token_value_flow_validate",
}
# two of them are associated functions of a generic type and run with the type parameter `u8`; the operations they
# check do not involve it
PO_KANI_AT_U8 = {"PO.MarketGroupV16ViewMut::apply_total_delta_u64",
                 "PO.MarketGroupV16HeaderAccount::dynamic_asset_slot_capacity_from_account_len"}
for _pid, _h in PO_KANI.items():
    assert _pid in pieces, f"{_pid}: no such PO piece (the census changed?)"
    pieces[_pid]["status"] = "verified by Kani"
    _at = (" (type parameter `u8`; the checked operations do not involve it)" if _pid in PO_KANI_AT_U8 else "")
    pieces[_pid]["evidence"] = (f"{_h} (Kani, closure layer, every input symbolic, no assumption){_at}, "
                                f"engine {PO_KANI_REV}; the function is unchanged in {LANDED_REV} "
                                f"(specs/changed_since_4e77481b_at_{LANDED_REV}.md)")

# Batch 16 (engine 3e615422, whose src/v16.rs is 71076137's): Kani proofs in the closure layer, every input symbolic, no
# assumption. The PO harnesses call one function each, so Kani's overflow checks cover every operation it runs.
B16_KANI_REV = "3e615422"
B16_KANI = {
    "PO.V16Core::prepare_insurance_lien_consume_delta": "proof_no_overflow_prepare_insurance_lien_consume_delta",
    "PO.V16Core::kernel_resolved_receipt_can_still_gain": "proof_no_overflow_kernel_resolved_receipt_can_still_gain_wide_stubbed",
    "PO.V16Core::backing_utilization_fee_quote_atoms_for_lien": "proof_no_overflow_backing_utilization_fee_quote_atoms_for_lien_wide_stubbed",
    "PR1": "proof_resolved_payout_rate_recompute_leaves_a_rate_at_most_one",
    "PL1": "proof_decoded_active_leg_is_valid_for_any_loss_weight",
}
B16_WHAT = {
    "PR1": "for every ledger, the recompute fails or leaves 1 <= den and num <= den",
    "PL1": "for every byte pattern of a stored leg, a leg decoded as active passes validate_active_leg",
}
B16_STUBS = {
    "proof_no_overflow_prepare_insurance_lien_consume_delta": [],
    "proof_no_overflow_kernel_resolved_receipt_can_still_gain_wide_stubbed": ['wide_mul_div_floor_u128'],
    "proof_no_overflow_backing_utilization_fee_quote_atoms_for_lien_wide_stubbed": ['U256::checked_mul', 'U256::checked_div', 'U256::try_into_u128', 'V16Core::backing_utilization_rate_e9_for_source_state'],
    "proof_resolved_payout_rate_recompute_leaves_a_rate_at_most_one": [],
    "proof_decoded_active_leg_is_valid_for_any_loss_weight": ['loss_weight_for_basis'],
}
B16_STUB_WHY = {
    "U256::checked_mul": "arbitrary result (wide math never panics: A1-G5)",
    "U256::checked_div": "arbitrary result (wide math never panics: A1-G5)",
    "U256::try_into_u128": "arbitrary result (wide math never panics: A1-G5)",
    "wide_mul_div_floor_u128": "arbitrary result (wide math never panics: A1-G5)",
    "V16Core::backing_utilization_rate_e9_for_source_state": "arbitrary result (its own operations are its own piece)",
    "loss_weight_for_basis": "a deterministic stand-in (PL1 is structural: it holds for any deterministic loss weight)",
}
for _pid, _h in B16_KANI.items():
    assert _pid in pieces, f"{_pid}: no such piece (the census or the ledger changed?)"
    pieces[_pid]["status"] = "verified by Kani"
    _st = B16_STUBS.get(_h) or []
    pieces[_pid]["evidence"] = (f"{_h} (Kani, closure layer, every input symbolic, no assumption"
                                + (f"; {B16_WHAT[_pid]}" if _pid in B16_WHAT else "")
                                + ("; stubbed: " + "; ".join(f"{x}: {B16_STUB_WHY[x]}" for x in _st) if _st else "")
                                + f"), engine {B16_KANI_REV}")

# PO pieces closed by rule R1 (po_rules.py, re-checked here on the landed source at every regen): every unchecked
# operation of the function is the statement `i += 1;` inside `while i < BOUND {` (or `while i < BOUND && .. {`), with
# no other write to `i` in the loop, no `&mut i` and no closure naming `i` in the function, and at most one increment
# per iteration (the last statement of the body, or followed by `continue;`). Both sides of `<` have `i`'s type, so
# the increment runs with i < BOUND <= MAX and cannot overflow.
PO_RULE = "R1"
sys.path.insert(0, HERE)
import po_rules as _po_rules
_po_rules.selftest()   # the rule's must-accept and must-reject cases, before it classifies anything
_po_sites = {}
for _r in _po_rules.classify(ENGINE_SRC, os.path.join(HERE, _wrap)):
    _po_sites.setdefault(_r["function"], []).append(_r)
for _fn, _rs in sorted(_po_sites.items()):
    _pid = f"PO.{_fn}"
    if _pid not in pieces or pieces[_pid]["status"] != "open" or not all(_r["rule"] == "R1" for _r in _rs):
        continue
    pieces[_pid]["status"] = "proved by rule"
    pieces[_pid]["evidence"] = ("rule R1, loop counters bounded by their own loop guard (po_rules.py on engine "
                                f"{LANDED_REV}): " + "; ".join(f"line {_r['line']} ({_r['reason']})" for _r in _rs))

# Batch 18 (engine 19f76585, whose src/v16.rs is 71076137's): Kani proofs in the closure layer, every input symbolic, no
# assumption. The PO harnesses call one function each, so Kani's overflow checks cover every operation it runs.
B18_KANI_REV = "19f76585"
B18_KANI = {
    "PO.liquidation_projected_healthy_after_close": "proof_no_overflow_liquidation_projected_healthy_after_close_stubbed",
    "PO.min_abs_q_for_risk_notional_at_least": "proof_no_overflow_min_abs_q_for_risk_notional_at_least_wide_stubbed",
}
B18_WHAT = {
}
B18_STUBS = {
    "proof_no_overflow_liquidation_projected_healthy_after_close_stubbed": ['liquidation_leg_maintenance_requirement', 'liquidation_risk_notional_ceil', 'liquidation_fee_for_close', 'liquidation_projected_health_deficit_from_parts'],
    "proof_no_overflow_min_abs_q_for_risk_notional_at_least_wide_stubbed": ['U256::checked_mul', 'U256::checked_div', 'U256::try_into_u128'],
}
B18_STUB_WHY = {
    "U256::checked_mul": "arbitrary result (wide math never panics: A1-G5)",
    "U256::checked_div": "arbitrary result (wide math never panics: A1-G5)",
    "U256::try_into_u128": "arbitrary result (wide math never panics: A1-G5)",
    "wide_mul_div_floor_u128": "arbitrary result (wide math never panics: A1-G5)",
    "V16Core::backing_utilization_rate_e9_for_source_state": "arbitrary result (its own operations are its own piece)",
    "loss_weight_for_basis": "a deterministic stand-in (PL1 is structural: it holds for any deterministic loss weight)",
    "liquidation_leg_maintenance_requirement": "arbitrary result (its own operations are its own piece)",
    "liquidation_risk_notional_ceil": "arbitrary result (its own no-overflow piece is proved, batch 14)",
    "liquidation_fee_for_close": "arbitrary result (its own no-overflow piece is proved, batch 14)",
    "liquidation_projected_health_deficit_from_parts": "arbitrary result (its own operations are its own piece)",
}
for _pid, _h in B18_KANI.items():
    assert _pid in pieces, f"{_pid}: no such piece (the census or the ledger changed?)"
    pieces[_pid]["status"] = "verified by Kani"
    _st = B18_STUBS.get(_h) or []
    pieces[_pid]["evidence"] = (f"{_h} (Kani, closure layer, every input symbolic, no assumption"
                                + (f"; {B18_WHAT[_pid]}" if _pid in B18_WHAT else "")
                                + ("; stubbed: " + "; ".join(f"{x}: {B18_STUB_WHY[x]}" for x in _st) if _st else "")
                                + f"), engine {B18_KANI_REV}")

# Batch 19 (engine 96e9b6d2, whose src/v16.rs is 71076137's): Kani proofs in the closure layer, every input symbolic,
# no assumption except where B19_ASSUMES names one a deployed caller guarantees. The PO harnesses call one function
# each, so Kani's overflow checks cover every operation it runs.
B19_KANI_REV = "96e9b6d2"
B19_KANI = {
    "PO.signed_position": "proof_no_overflow_signed_position_of_a_decoded_leg",
}
B19_WHAT = {
}
B19_STUBS = {
    "proof_no_overflow_signed_position_of_a_decoded_leg": [],
}
B19_STUB_WHY = {
    "U256::checked_mul": "arbitrary result (wide math never panics: A1-G5)",
    "U256::checked_div": "arbitrary result (wide math never panics: A1-G5)",
    "U256::try_into_u128": "arbitrary result (wide math never panics: A1-G5)",
    "wide_mul_div_floor_u128": "arbitrary result (wide math never panics: A1-G5)",
    "V16Core::backing_utilization_rate_e9_for_source_state": "arbitrary result (its own operations are its own piece)",
    "loss_weight_for_basis": "a deterministic stand-in (PL1 is structural: it holds for any deterministic loss weight)",
    "liquidation_leg_maintenance_requirement": "arbitrary result (its own operations are its own piece)",
    "liquidation_risk_notional_ceil": "arbitrary result (its own no-overflow piece is proved, batch 14)",
    "liquidation_fee_for_close": "arbitrary result (its own no-overflow piece is proved, batch 14)",
    "liquidation_projected_health_deficit_from_parts": "arbitrary result (its own operations are its own piece)",
}
B19_ASSUMES = {
    "proof_no_overflow_signed_position_of_a_decoded_leg": "one assumption, that an active leg's basis_pos_q is not i128::MIN; that every deployed caller guarantees it is piece PL3 (open: six call sites read, not proved); without the assumption the proof fails",
}
for _pid, _h in B19_KANI.items():
    assert _pid in pieces, f"{_pid}: no such piece (the census or the ledger changed?)"
    pieces[_pid]["status"] = "verified by Kani"
    _st = B19_STUBS.get(_h) or []
    pieces[_pid]["evidence"] = (f"{_h} (Kani, closure layer, every input symbolic, "
                                + (B19_ASSUMES[_h] if _h in B19_ASSUMES else "no assumption")
                                + (f"; {B19_WHAT[_pid]}" if _pid in B19_WHAT else "")
                                + ("; stubbed: " + "; ".join(f"{x}: {B19_STUB_WHY[x]}" for x in _st) if _st else "")
                                + f"), engine {B19_KANI_REV}")

# TR: the real class of layer T, split into statements local to the header and one account
for pid, text in (
    ("TR.a", "H2, H3: every flag write is 0, 1 or `encode_bool(..)`; `recovery_reason` is written only by `from_runtime(Some(reason))`"),
    ("TR.b", "H5, H6, H14, H15: `c_tot + insurance + backing_provider_earnings_total + source_fresh_backing_total_num / BOUND_SCALE` "
             "rises by at most the change of `vault` (for the real entry points: it does not rise)"),
    ("TR.c", "H16: `insurance - insurance_domain_budget_remaining_total` does not fall, and the reserved insurance credit total stays "
             "within the remaining budget total"),
    ("TR.d", "H7, H10, H11: the positive-PnL totals are written only by `set_account_pnl_inner`, which keeps the bound relations by construction"),
    ("TR.e", "H17: with `U(account)` the account's positive-PnL bound minus its source claims, `U(account) >= 0` is preserved and "
             "`(pnl_pos_bound_tot_num - source_claim_bound_total_num) - U(account)` does not fall"),
    ("TR.f", "H4, H12: the payout ledger is written only through `from_runtime`, and only after the snapshot flag is set"),
):
    piece(pid, "T transition", "writers reached by T32, T37, T42, T45 and the crank's internal functions",
          text + " (specs/transition_shape_spec_<rev>.md, section on the real class)",
          "Aristotle from the kernel contracts; the header pieces of the real entry points are the conjunction of TR.a to TR.f")

# G: the global invariant every reachable state has to satisfy, and the induction that gives it.
# Three of its five parts are functions of the code and need no statement of their own: H = `validate_shape` of a default
# build, S = `validate_shape_full_audit_scan`, V = `validate_with_market` for every portfolio account of the market.
for pid, target, text, how in (
    ("G.M", "market and all portfolio accounts", "part M: each market total that mirrors account state equals the sum over all portfolio "
     "accounts of the market (the engine cannot check this at run time: it never sees all accounts). To be written as a formal statement, "
     "one line per mirrored total", "formal statement, then reviewed"),
    ("G.C", "vault token accounts and market header", "part C: custody. The program's vault token accounts hold exactly what the header's "
     "`vault` says, in the configured mints, under the program's authority. To be written as a formal statement", "formal statement, then reviewed"),
    ("G.other", "portfolio accounts an instruction does not name",
     "frame: an instruction leaves every account it does not name valid against the market it writes (a market write such as an asset "
     "retirement or a new market id must not invalidate a leg of an account that is not present)", "Aristotle, per kind of market write"),
    ("G.init", "InitMarket", "the initial state satisfies H, S, V, M and C", "Aristotle"),
    ("G.reach", "every instruction", "induction: H, S, V, M and C hold in every state reachable from InitMarket by successful instructions; "
     "the conjunction of the instruction pieces of layer W", "Aristotle, composition of layer W"),
):
    piece(pid, "G global invariant", target, text, how)

# Y: what the proofs assume and what is established by evidence instead of proof
for pid, target, text in (
    ("Y.runtime", "Solana runtime", "assumed: transaction atomicity (a failed instruction changes nothing), account ownership and signer "
     "checks, rent and reallocation rules, the Clock sysvar (the elapsed slots of the accruals and the price clamps come "
     "from it; seven handlers would fall back to the instruction's slot argument if the read failed, which it does not "
     "on-chain), the compute meter"),
    ("Y.spl", "SPL Token program", "assumed: a transfer moves exactly the amount between the named accounts or fails; mint and owner fields "
     "are as the token program wrote them"),
    ("Y.oracle", "oracle accounts", "assumed: the layouts the program parses (Pyth, Switchboard, Chainlink) are the publishers' layouts. "
     "Nothing is assumed about the values: what an oracle reports and what any authority signs are adversarial inputs. The charter "
     "already bounds what they can do to a user (INV-005 with its role matrix, INV-014, INV-045, INV-046, INV-064, INV-018, the "
     "isolation invariants and the exit rule), and those invariants are proved with the oracle and the authorities quantified over"),
    ("Y.tool", "Aristotle", "trusted: its model of Rust and of the core library, with the library behaviours each run summary lists as "
     "assumed, and the bound on object sizes (2^56 bytes)"),
    ("Y.compute", "compute budget (INV-077)", "evidence, not proof: every instruction fits the compute limit at the maximum shape "
     "(measured by the program's max-shape tests)"),
    ("Y.terminates", "loops", "evidence, not proof: every loop is bounded by a compile-time constant or by an input length that the "
     "compute meter bounds (Aristotle proves no termination)"),
    ("Y.statements", "the formal statements", "an independent reader has checked that each formal statement says what the charter "
     "invariant means; a proof of the wrong statement proves nothing"),
):
    piece(pid, "Y assumptions and evidence", target, text, "written down and signed off; not a proof")

# S: the settlement and bookkeeping methods between the kernels and the entry points. The owner's
# priority (2026-11-20): Aristotle goes where Kani has gaps, business logic before easily tested math.
# The gap is measured by the call graph (kani_reach.py, specs/kani_gap_map_<rev>.md): 62 deployed
# functions and 14 of the 50 deployed entry points are reached by no harness: trade execution,
# recovery and forfeit, three crank steps and the terminal routes. Those come first (the trade pilot
# below: TF, TA, TB with the contracts TC1 to TC9; prompts/stage_s2_trade_pilot.txt). The two
# methods S1 and S2 were a first pilot, submitted on a count by NAME that overstated the gap;
# existing harnesses reach both (8 and 19 harnesses), so the task was cancelled and the pieces wait
# behind the certain gaps. Each method is proved from its own body and from contracts of the
# methods it calls; a property is closed only when the contracts it names are.
TRADE_EVIDENCE = ("Aristotle. The trade pilot (task s2, engine d1fa39f6) stated it and stopped by itself with this piece open: the "
                  "walk of the compiled body was too expensive (the byte-level model of the 9,419-byte account, the number of `?` exits, "
                  "and shared borrows that must be given back on every exit). Kani proves the gates of the trade entry point, not its "
                  "body, so today the evidence is the engine's tests and fuzz and the program's stateful suites")
for pid, target, text in (
    ("TF.margin", "finish_trade_checks_not_atomic",
     "no trade that needed initial margin from an account ends with that account below it: its certificate is valid and "
     "`certified_equity >= certified_initial_req` at the return (rests on TC9)"),
    ("TF.locked", "finish_trade_checks_not_atomic",
     "in the locked lane the account also meets its initial requirement from capital and negative PnL alone, without positive credit"),
    ("TF.frame", "finish_trade_checks_not_atomic",
     "writes nothing unless the trade increased risk outside the locked lane; then only the fields a lien creation writes (rests on TC9)"),
    ("TF.order", "finish_trade_checks_not_atomic",
     "the long account's stored certificate can overstate its equity when its margin is checked, because the short account's margin "
     "lien was created after it was written (answered from the source by the pilot and by a second reading, not proved); the "
     "margin rule does not rest on that number: see TF.reserved"),
    ("TF.reserved", "create_initial_margin_source_lien_if_needed",
     "when it returns Ok, the account's initial requirement is covered by capital less its negative PnL and fee debt plus the "
     "credit its own valid liens reserve (`incremental_initial_margin_source_credit_needed == 0`), and no other account's lien "
     "can take reserved backing; so a risk-increasing trade never leaves an account with source claims below its initial "
     "requirement, whatever its certificate says"),
    ("TA.fee", "apply_trade_after_refresh_not_atomic",
     "each account pays `min(fee, capital)`, or nothing when its PnL is negative; the fee and the notional are the stated formulas (rests on TC1)"),
    ("TA.conservation", "apply_trade_after_refresh_not_atomic",
     "applying a fill moves no token value and no PnL: `vault` and both accounts' PnL are unchanged, and each fee moves from that "
     "account's capital (and `c_tot`) to `insurance` (rests on TC1 to TC7)"),
    ("TA.positions", "apply_trade_after_refresh_not_atomic",
     "each account's position on the traded asset becomes the planned one and its other legs are unchanged, for a lookup whose "
     "current position is the stored one (the pilot found the first wording false without that: a lookup with equal current and "
     "next positions changes nothing and is not compared with the stored leg; the engine builds lookups from the account) "
     "(rests on TC2, TC5)"),
    ("TA.gate", "apply_trade_after_refresh_not_atomic", "a side the fill grew does not end above the side cap (rests on TC8)"),
    ("TA.frame", "apply_trade_after_refresh_not_atomic",
     "writes four header fields, eight account fields and only the traded asset's slot (rests on TC1 to TC7)"),
    ("TA.nopanic", "apply_trade_after_refresh_not_atomic", "no execution of the body panics outside its named callees"),
    ("TB.flag", "execute_batch_with_fee_loss_stale_scoped_not_atomic",
     "the scoped suspension of `loss_stale_active` never leaks: on every return after the inner call, Ok or Err, the flag has its entry value"),
    ("TB.checked", "execute_batch_with_fee_loss_stale_scoped_not_atomic",
     "every Ok return follows a passing `validate_shape` and `validate_with_market` of both accounts on the final state"),
    ("TC1", "charge_account_fee_current_not_atomic",
     "contract: charges `min(requested, capital)` or nothing when PnL is negative; capital and `c_tot` fall and `insurance` rises by that amount; `vault` is not written"),
    ("TC2", "validate_trade_position_preflight", "contract: writes nothing; returns the lookups it was given and the risk flags computed from them"),
    ("TC3", "require_asset_risk_change_allowed", "contract: writes nothing; a risk increase needs both side indices at `ADL_ONE`"),
    ("TC4", "begin_terminal_trade_residual_if_needed", "contract: writes only the account's close ledger, one header counter and one field of the traded asset's slot, and nothing unless a final fill closes a losing single-leg account"),
    ("TC5", "apply_current_position_delta_with_lookup",
     "contract: the leg becomes the planned position, other legs are unchanged, none of capital, PnL, `c_tot`, `insurance`, `vault` is written, and nothing at all is written for a zero delta"),
    ("TC6", "transfer_trade_residual_reward_credit", "contract: writes only the two accounts' residual reward counters"),
    ("TC7", "recertify_account_after_trade_delta", "contract: writes only the account's certificate"),
    ("TC8", "kernel_trade_side_oi_cap_gate", "contract: the exact return condition of the side cap gate"),
    ("TC9", "create_initial_margin_source_lien_if_needed",
     "contract: on Ok the certificate is valid; writes only capital, certificate and source domains of that account and six header fields"),
):
    piece(pid, "S settlement glue", target, text, TRADE_EVIDENCE)
for pid, target, text in (
    ("S0.C1", "fresh_counterparty_backing_expiry_slot", "contract: writes nothing"),
    ("S0.C2", "credit_post_snapshot_residual_not_atomic",
     "contract: writes only `payout_snapshot` and `resolved_payout_ledger`, and nothing when the amount is zero or no snapshot is captured"),
    ("S0.C3.bound", "book_realized_kf_loss_for_source_domain_not_atomic", "contract: the returned junior amount is at most the amount booked"),
    ("S0.C3.frame", "book_realized_kf_loss_for_source_domain_not_atomic",
     "contract: writes neither `vault` nor `c_tot`, and no header field outside seven named ones"),
    ("S0.C3.value", "book_realized_kf_loss_for_source_domain_not_atomic",
     "the exact relation between the amount booked, the part given to the domain's backing and the junior amount (the junior part is "
     "rounded up)"),
    ("S0.C4", "burn_account_source_claim_bound_num",
     "contract: writes none of the header's positive-PnL totals or its negative-PnL count, and none of the account's PnL, lock or active bitmap"),
    ("S0.C5", "prepare_source_credit_domain_recompute_for_epoch", "contract: changes only `credit_rate_num` and `credit_epoch`; both epochs rise by one"),
    ("S0.C6", "source_credit_for_domain / set_source_credit_for_domain",
     "contract: the exact write set of the setter (the record, three header totals, and the protection floor of the same domain, which can only fall)"),
    ("S0.C7", "ensure_account_source_claim_market_id", "contract: finds or creates the account's entry for the domain and tags it with the asset's market id; nothing else changes"),
    ("S0.C8", "source_domain_mut_or_insert", "contract: after S0.C7 it returns the same entry and writes nothing"),
    ("S1.amount", "pay_kf_loss_from_free_capital_not_atomic", "pays `min(loss, capital)`; a zero payment writes nothing"),
    ("S1.conservation", "pay_kf_loss_from_free_capital_not_atomic",
     "the account's capital and the market's `c_tot` fall by exactly the amount paid and `vault` is unchanged (rests on S0.C1, S0.C2, S0.C3.frame)"),
    ("S1.live", "pay_kf_loss_from_free_capital_not_atomic",
     "Live branch: nothing else is written in the market; the pending credit for the loss domain grows by exactly the amount paid"),
    ("S1.resolved", "pay_kf_loss_from_free_capital_not_atomic",
     "Resolved branch: the pending credit is not written and no header field outside eight named ones is (rests on S0.C2, S0.C3.frame)"),
    ("S1.nopanic", "pay_kf_loss_from_free_capital_not_atomic", "no call panics"),
    ("S2.account", "set_account_pnl_inner", "the account's PnL becomes `new_pnl` and its certificate is invalidated; `i128::MIN` is refused"),
    ("S2.mirror", "set_account_pnl_inner", "the market's `pnl_pos_tot` moves by exactly the change of the account's positive PnL (a decrease rests on S0.C4)"),
    ("S2.count", "set_account_pnl_inner", "`negative_pnl_account_count` follows the sign of the account's PnL"),
    ("S2.source", "set_account_pnl_inner",
     "every atom of positive PnL that appears is registered as a claim on exactly one named source domain (account entry, bound and "
     "exact claims of that domain, and the market total); in Live an increase without a domain is refused (rests on S0.C5 to S0.C8)"),
    ("S2.totals", "set_account_pnl_inner", "header checks H7, H10 and H11 are preserved"),
    ("S2.claims", "set_account_pnl_inner",
     "whether H17 (`pnl_pos_bound_tot_num >= source_claim_bound_total_num`) is preserved, and under which precondition on the "
     "caller's pre-burned claims"),
    ("S2.frame", "set_account_pnl_inner", "the exact header, slot and account fields an Ok call can write"),
    ("S2.nopanic", "set_account_pnl_inner", "no call panics"),
):
    piece(pid, "S settlement glue", target, text,
          "Aristotle, using the kernel contracts already proved (first pilot s1, cancelled 2026-11-20: Kani harnesses reach this "
          "method through other entry points, so it waits behind the methods no harness reaches); what those harnesses assert "
          "about it has not been checked")

# X: states committed after an error (labels X.., because E1..E6 are arithmetic labels) (specs/error_commit_paths_<rev>.md, gen_error_commit_spec.py)
for pid, target, text in (
    ("X1.header", "kernel_commit_declared_liquidation_recovery",
     "on every path that returns `RecoveryRequired` after this call declared Recovery, nothing is written to the market after the "
     "`validate_shape()` inside `declare_permissionless_recovery`"),
    ("X1.account", "kernel_commit_declared_liquidation_recovery",
     "on those paths the account passes `validate_with_market` at the point of return"),
    ("X2", "clear_leg_at_slot_inner",
     "the writes before a `Stale` return (`liquidation_lock = true`, `health_cert.valid = 0`) leave a state the engine accepts and "
     "that `sync_unattributed_loss_lock` would produce as well"),
    ("X4", "deregister_empty_materialized_portfolio_not_atomic",
     "it returns `LockActive` only before its single write; the program's SyncMaintenanceFee continues after that error"),
    ("X3.before", "permissionless_auto_crank_not_atomic",
     "the listed `NonProgress` returns come before any write"),
    ("X3.refresh", "liquidate_account_not_atomic",
     "at the two `NonProgress` returns after its refresh the state is that of a completed refresh with a valid certificate"),
    ("X3.guarded", "permissionless_auto_crank_not_atomic",
     "at each of the four `NonProgress` returns that follow further writes (N1c, N2, N3, N4), in every execution in which "
     "`header.insurance` and the account's position vector are what they were before the call: the state passes the per-asset "
     "scan and `validate_with_market`, and differs from the state before the call only by writes a successful step of the same "
     "kind makes and that debit no other account. The program commits after `NonProgress` only when it accrued the market in the "
     "same instruction and that guard holds (program #485), and it checks the header before every crank commit. Split by a "
     "reading of 2026-11-20 (specs/x3_guarded_reading_<rev>.md) into the three lemmas below"),
    ("X3.R1", "reduce_position",
     "an Ok return with `close_q >= 1` leaves the leg's active flag or raw basis different from before the call; with it the "
     "guard refuses every execution that reaches N1c or N4 (both follow a reduction)"),
    ("X3.R2", "advance_pending_close_residual_not_atomic",
     "at N2 nothing was booked onto the other side and no insurance was spent: the ledger's remaining residual falls by exactly "
     "what is spent or booked, and in Live mode a residual that cannot be booked returns `RecoveryRequired`"),
    ("X3.R3", "release_account_source_credit_liens_if_unneeded_core_not_atomic",
     "at N3 the loop has written nothing, and the settlement of a flat account's side effects before it debits no other account"),
):
    piece(pid, "X committed after an error", target, text,
          "Aristotle (for N1c: what a liquidation step that reduced nothing can still have written); today: a reading of the source, and, on "
          "engine c030aa40 and program 3a690205 (before the guard), an engine copy that stops at N1c, N2, N3 and N4, at a failing "
          "header after every unchecked exit and at a loss booked after the payout snapshot: nothing fired in the engine suites "
          "(430 and 515 tests), in 2.3 million random action sequences, or in the program's suites (stateful 339, fuzz "
          "regressions 148, compute suite 1,592; its 14 failures were the funding-cadence test and 13 compute effects of the "
          "added checks). Not repeated on the revisions of this ledger")

# W: wrapper instruction handlers
wsrc = read(WRAPPER_SRC)
start = wsrc.index("pub enum Instruction")
body = wsrc[start:wsrc.index("\n    }\n", start)]
variants = re.findall(r"(?m)^        ([A-Z][A-Za-z0-9]*)\s*[{,(]?", body)
# what the instruction census found per instruction (wrapper_census.py -> specs/wrapper_shape_census_<rev>.md)
# Rolled 2026-11-23 to program aac89b9e (engine pin unchanged): the INV-046 fix. On an asset whose mark cannot move
# (Recovery) a trade print is ignored like an authenticated mark's, instead of reverting the reduction that carries it;
# BatchTradeNoCpi writes back every leg's oracle profile after the leg's zero-move funding segment. Checked by
# roll_prog_inv046.py: the instruction census and the effective-price writer scan are unchanged, and so are the two
# clamp functions AC.clamp is about.
# Rolled 2026-11-23 to program f6d0bc57 (engine pin unchanged): INV-045's catch-up rule (finding 3 of the third read,
# option (b)). A target change that nobody pays for reaches back at most one window (`max_accrual_dt_slots`):
# PushAuthMark/PushEwmaMark refuse it on an exposed asset further behind, and the crank holds such a reading back.
# Checked by roll_prog_f3.py: the instruction census and the effective-price writer scan are unchanged, and so are
# the two clamp functions AC.clamp is about. New open pieces AC.catchup_push and AC.catchup_crank (clause C11).
# Rolled 2026-11-23 to program 257db166 (engine pin unchanged): the second INV-046 read's fixes. Past a hybrid's
# soft-stale window the trade average owns the target for every writer until the feed publishes a newer reading (the
# owner's rule A; the crank no longer re-stages a reading it has already observed); a Recovery asset's own staleness
# timer no longer refuses its reductions (trade_resolve_matured_at_slot). Checked by roll_prog_inv046b.py: the
# instruction census and the effective-price writer scan are unchanged, and so are the two clamp functions.
# Rolled 2026-11-24 to program 81a8a8ec: the pin of engine 06b7e5be (INV-077) and the crank's progress rule (a
# collection of a due maintenance fee that advanced the portfolio's fee cursor counts as progress when the engine finds
# nothing else to do). Checked by roll_to_06b7e5be.py: the program source changes only there, the instruction census and
# the effective-price writer scan are unchanged, and so are the two clamp functions.
# Rolled 2026-11-24 to program 0cb5794e: the pin of engine 263c34d4 (issue #274). The program source is unchanged
# (checked by roll_to_263c34d4.py, with the instruction census); two owner tests that asserted the old order were
# rewritten with the owner's approval, and the short-side B compute pin moved (556,221 -> 697,757 CU).
WRAPPER_REV = "0cb5794e"
_census_rows = {}
_cpath = os.path.join(HERE, "specs", f"wrapper_shape_census_{WRAPPER_REV}.md")
if os.path.exists(_cpath):
    for line in open(_cpath):
        c = [x.strip() for x in line.strip().strip("|").split("|")]
        if len(c) == 5 and c[0] not in ("Instruction", "---") and not set(c[0]) <= set("-"):
            _census_rows[c[0]] = c
ERROR_EXCEPTIONS = {
    "PermissionlessCrank": "an engine error aborts the instruction, except `NonProgress` from the auto crank after the handler accrued the market, and then only if `insurance` and the account's positions are unchanged (X3)",
    "SyncMaintenanceFee": "an engine error aborts the instruction, except `LockActive` from the portfolio deregistration (X4)",
}
for i, v in enumerate(variants, 1):
    row = _census_rows.get(v)
    reach = ""
    if row:
        reach = f". Engine entry points its handler can reach: {row[2]}" + (f". Found when read: {row[4]}" if row[4] else "")
    piece(f"W{i:02d}", "W wrapper", v,
          f"`{v}`: accounts, signers and bindings are checked before any state change; token movement equals the engine's booked delta; "
          "if the global invariant (H, S, V, M, C) held before and the instruction succeeds, it holds after; "
          + ERROR_EXCEPTIONS.get(v, "an engine error aborts the instruction") + reach,
          "Aristotle (needs the wrapper to build under its compiler); today: SVM tests and source censuses")

# L: liveness, split into safety pieces
plans = ["NoAction", "RefreshAccount", "SettleBChunk", "Liquidate", "ReleaseSourceLiens",
         "AdvancePendingClose", "ExpireSourceBacking", "ResolvedClose", "RecoveryForfeit", "FinalizeReset"]
for i, p in enumerate(plans, 1):
    piece(f"L{i:02d}.rank", "L liveness", p,
          f"a successful `{p}` step strictly lowers a fixed rank of (market, account) state",
          "Aristotle (safety form of progress); today: Kani rank proofs and reachability model")
piece("L.enabled", "L liveness", "select_auto_crank_plan",
      "whenever the actionable summary is nonzero the selector returns a plan other than NoAction (K20, K23)",
      "Aristotle via K20/K23")
piece("L.terminates", "L liveness", "all loops", "every loop is bounded by a compile-time constant",
      "outside Aristotle (it proves no termination); bounded-loop audit and compute tests")

# ---------------------------------------------------------------- statuses from Aristotle
props = aristotle_properties("--offline" in sys.argv)
fn_to_k = {fn: k for k, fn in kernels.items()}
arith = {"widening_mul_u128_spec": ("D1", "proved"), "mul_u256_spec": ("D2", "proved"),
         "div_rem_u256_zero_spec": ("C1", "proved in part: the zero-divisor half"),
         "checked_div_rem_by_u256_zero_spec": ("D3", "proved in part: the zero-divisor half")}
arith_rows = {}
# Aristotle records the proofs made on NEW_REV in the namespace Spec.KernelC and keeps the rows of
# the earlier revision in Spec.Kernel. The new revision's rows are read last, so they decide.
props = sorted(props, key=lambda p: p["name"].startswith("Spec.KernelC."))
for p in props:
    name, st = p["name"], p["status"]
    on_new_rev = name.startswith("Spec.KernelC.")
    if on_new_rev:
        name_k = "Spec.Kernel." + name[len("Spec.KernelC."):]
        mx = re.match(r"Spec\.Kernel\.(\w+)_(exposure|progress|realization|monotone|minimum|inverse)$", name_k)
        if mx and (mx.group(1), mx.group(2)) in EXTRA_K:
            pid = EXTRA_K[(mx.group(1), mx.group(2))]
            if st.startswith("PROVED"):
                pieces[pid]["status"] = "proved, final check pending" if "unchecked" in st else "proved"
                pieces[pid]["evidence"] = f"{name} (engine {NEW_REV})"
            elif st.startswith("REFUTED"):
                pieces[pid]["status"] = "REFUTED"; pieces[pid]["evidence"] = name
            elif st.startswith("IN_PROGRESS") or st.startswith("IN PROGRESS"):
                pieces[pid]["status"] = "in progress"
            elif st:
                pieces[pid]["status"] = "attempted, open"; pieces[pid]["evidence"] = name
            continue
    else:
        name_k = name
    m = re.match(r"Spec\.Kernel\.(\w+)_(contract|nopanic)$", name_k)
    generic = ""
    if m and m.group(1) not in fn_to_k and m.group(1).endswith("_u64") and m.group(1)[:-4] in fn_to_k:
        # a method of the generic market view, proved for the instantiation T = u64
        generic = "; proved for the instantiation T = u64 of the generic market view, the body does not use T"
        m = re.match(r"Spec\.Kernel\.(\w+)_u64_(contract|nopanic)$", name_k)
    elif m and m.group(1) not in fn_to_k and m.group(1).endswith("_generic") and m.group(1)[:-8] in fn_to_k:
        # the every-T statement of a method of the generic market view (b5 summary, K50 to K56)
        generic = ("; this row is the statement for every T, which assumes the instantiating crate's layouts and the "
                   "documented behaviour of the library calls; the b5 summary (b5_dl/output_summary.md) states the "
                   "result at T = u64 with no assumptions")
        m = re.match(r"Spec\.Kernel\.(\w+)_generic_(contract|nopanic)$", name_k)
    if m and m.group(1) in fn_to_k:
        pid = f"{fn_to_k[m.group(1)]}.{m.group(2)}"
        if pieces[pid]["status"] == NOT_DEPLOYED:
            pieces[pid]["evidence"] += "; Aristotle proved it on the fuzz-feature build (b4 summary)"
            continue
        if st.startswith("PROVED") and pid.split(".")[0] in REPROVE and not on_new_rev and not (REPROVE_SINCE and p["last_updated"] >= REPROVE_SINCE):
            pieces[pid]["status"] = f"proved for {PROVED_AT}; to be established again for {NEW_REV}"
            pieces[pid]["evidence"] = (f"{name} ({PROVED_AT}); the function's text is unchanged but it takes the asset record, "
                                       f"which gained two fields in {NEW_REV}")
        elif st.startswith("PROVED"):
            # A proof recorded by a task that was cancelled before its closing check is listed as
            # "unchecked" until a later task re-stamps it. It is not counted as proved here.
            at = f"engine {NEW_REV}" if (on_new_rev or pid.split(".")[0] in REPROVE) else f"{PROVED_AT}, unchanged in {NEW_REV}"
            pieces[pid]["status"] = "proved, final check pending" if "unchecked" in st else "proved"
            pieces[pid]["evidence"] = f"{name} ({at}{generic})"
        elif st.startswith("REFUTED"):
            pieces[pid]["status"] = "REFUTED"; pieces[pid]["evidence"] = name
        elif st.startswith("IN_PROGRESS") or st.startswith("IN PROGRESS"):
            pieces[pid]["status"] = "in progress"
        elif st:
            pieces[pid]["status"] = "attempted, open"; pieces[pid]["evidence"] = name
    m = re.match(r"Spec\.WideMath\.(\w+)$", name)
    if m and m.group(1) in arith and st.startswith("PROVED"):
        pid, how = arith[m.group(1)]
        pieces[pid]["status"] = how; pieces[pid]["evidence"] = f"{name} ({PROVED_AT}; src/wide_math.rs is identical in {NEW_REV})"
    # rows named after a label of the arithmetic spec, e.g. Spec.WideMath.B3_checked_mul
    m = re.match(r"Spec\.WideMath\.([A-G]\d+b?)_\w+$", name)
    if m and m.group(1) in pieces:
        arith_rows.setdefault(m.group(1), []).append((name, st))

# What a label's rows establish when they do not establish the whole label (from the a2 summary),
# and the row of the later division task (a3) that completes each of them.
ARITH_PARTIAL = {
    "A2": ("proved in part: is_zero; the values of the three constants are open (they have no code of their own in the deployed build)",
           "Spec.WideMath.A2_u256_constants"),
    "A6": ("proved in part: is_zero, is_negative, is_positive, signum; the values of the five constants are open",
           "Spec.WideMath.A6_i256_constants"),
    "B9": ("proved in part: + - * << >> & | += -= with both panic directions; / and % belong to the division task",
           "Spec.WideMath.B9_div_rem_operators"),
}
ARITH_CORRECTED = {
    "A7": "the label as first written was wrong at I256::MIN: abs_u256 asserts self != MIN, so every execution there panics "
          "(proved); for every other x no execution panics and the result is |sval(x)| (proved)",
    "A2": "the constants are proved through a harness that compiles src/wide_math.rs unchanged and returns each constant (a3 summary)",
    "A6": "the constants are proved through a harness that compiles src/wide_math.rs unchanged and returns each constant (a3 summary)",
}
for pid, rows_ in arith_rows.items():
    if not all(st.startswith("PROVED") and "unchecked" not in st for _, st in rows_):
        pieces[pid]["status"] = "attempted, open"
        pieces[pid]["evidence"] = ", ".join(n for n, _ in rows_)
        continue
    earlier = pieces[pid]["evidence"].split(" (")[0] if pieces[pid]["evidence"].startswith("Spec.WideMath.") else ""
    ev = ", ".join(([earlier] if earlier and earlier not in [n for n, _ in rows_] else []) + [n for n, _ in rows_]) \
        + f" ({PROVED_AT}; src/wide_math.rs is identical in {NEW_REV})"
    if pid in ARITH_PARTIAL and ARITH_PARTIAL[pid][1] not in [n for n, _ in rows_]:
        pieces[pid]["status"] = ARITH_PARTIAL[pid][0]
    else:
        pieces[pid]["status"] = "proved"
        if pid in ARITH_CORRECTED:
            ev += "; " + ARITH_CORRECTED[pid]
    pieces[pid]["evidence"] = ev

# E4b is a question, not a claim: the spec file expects "does not return when F >= 2^255" to be false.
# It is settled when Aristotle refutes that statement with a counterexample AND proves what the function
# does instead (the E4 rows: outside its exact panic condition the result is F wrapped into I256).
e4b_refuted = any(p["name"] == "wide_signed_mul_div_floor.no_return_when_f_ge_2_255" and p["status"].startswith("REFUTED") for p in props)
if "E4b" in pieces and e4b_refuted and pieces.get("E4", {}).get("status") == "proved":
    pieces["E4b"]["status"] = "proved"
    pieces["E4b"]["statement"] = ("(settled as expected) `wide_signed_mul_div_floor` DOES return when F >= 2^255: outside its exact panic "
                                  "condition the result is F wrapped into I256, e.g. (2^255, 1, 1) returns I256::MIN; the engine does not call it")
    pieces["E4b"]["evidence"] = ("refuted with a counterexample: wide_signed_mul_div_floor.no_return_when_f_ge_2_255; the behaviour is proved "
                                 f"under E4 ({PROVED_AT}; a3 summary, reproducer sim/e4b_wide_signed_mul_div_floor)")

for pid, label in p_none:
    if pieces[label]["status"] == "proved":
        pieces[pid]["status"] = "proved"
        pieces[pid]["evidence"] = f"the callee never panics: {label} ({pieces[label]['evidence'].split(' (')[0]})"

# A proof made by a task that was cancelled keeps an "unchecked" flag in the listing even after a
# later task has run its final check. confirmed_final_checks.tsv names the pieces for which a
# later task's summary states that the final check passed, with the file that says so.
confirmed = {}
cf = os.path.join(HERE, "confirmed_final_checks.tsv")
if os.path.exists(cf):
    for line in open(cf):
        if line.strip() and not line.startswith("#"):
            pid, src_note = line.rstrip("\n").split("\t")
            confirmed[pid] = src_note
for pid, src_note in confirmed.items():
    if pid in pieces and pieces[pid]["status"] == "proved, final check pending":
        pieces[pid]["status"] = "proved"
        pieces[pid]["evidence"] += f"; final check confirmed in {src_note}"

# Layer T: `Spec.LayerT.tNN_header` is the statement of TNN.header (the entry point preserves the
# header checks). Batch t1 (task 18dfda03, engine c030aa40) proved T09 and T38 and was stopped at its
# spending cap with the other nine in progress; it wrote no summary, so a proved row whose final
# check is not recorded stays "final check pending".
for p in props:
    mt = re.match(r"Spec\.LayerT\.t(\d+)_header$", p["name"])
    if not mt:
        continue
    pid = f"T{int(mt.group(1)):02d}.header"
    if pid not in pieces or pieces[pid]["status"] == NOT_DEPLOYED:
        continue
    st = p["status"]
    if st.startswith("PROVED"):
        pieces[pid]["status"] = "proved, final check pending" if "unchecked" in st else "proved"
        pieces[pid]["evidence"] = f"{p['name']} (engine {NEW_REV}, batch t1)"
    elif st.startswith("REFUTED"):
        pieces[pid]["status"] = "REFUTED"; pieces[pid]["evidence"] = p["name"]
    elif st:
        # The service lists a property that a stopped task left unfinished as IN_PROGRESS again as soon
        # as any later task starts. No task after t1 was asked for these.
        pieces[pid]["status"] = "attempted, open"
        pieces[pid]["evidence"] = f"{p['name']}: batch t1 was stopped before this one finished"

# Kernels of the refactor that Kani checks only for bounded inputs and Aristotle proved at full width
# (tasks k1 on a3efa566, k2 on f24a561c, k3 on bfd4d0c4; the functions are textually unchanged in LANDED_REV).
BOUNDED_ROWS = {
    "Spec.KernelB.kernel_b_settlement_chunk_contract": ("kernel_b_settlement_chunk", "contract", "k1", "a3efa566", None),
    "Spec.KernelB.kernel_b_settlement_chunk_nopanic": ("kernel_b_settlement_chunk", "nopanic", "k1", "a3efa566", None),
    "Spec.KernelB.kernel_b_settlement_chunk_progress": ("kernel_b_settlement_chunk", "progress", "k1", "a3efa566",
        "a step with something to settle makes progress"),
    "Spec.KernelB.kernel_b_settlement_chunk_refusals": ("kernel_b_settlement_chunk", "refusals", "k1", "a3efa566",
        "the exact conditions of its RecoveryRequired and ArithmeticOverflow refusals"),
    "Spec.Accrual.canonical_accrual_price_step_v16_contract": ("canonical_accrual_price_step_v16", "contract", "k2", "f24a561c", None),
    "Spec.Accrual.canonical_accrual_price_step_v16_nopanic": ("canonical_accrual_price_step_v16", "nopanic", "k2", "f24a561c", None),
    "Spec.Accrual.canonical_accrual_price_step_v16_envelope": ("canonical_accrual_price_step_v16", "envelope", "k2", "f24a561c",
        "over any chain of steps toward a fixed target, the price moves at most n * cap_anchor * max_change_bps + r0 basis-point units "
        "and never past the target"),
    "Spec.Accrual.canonical_accrual_price_step_v16_reaches_of_valid": ("canonical_accrual_price_step_v16", "reaches", "k2", "f24a561c",
        "the price reaches the target once the allowance covers the distance"),
    "Spec.KR.kernel_bankruptcy_residual_step_capacity_contract": ("kernel_bankruptcy_residual_step_capacity", "contract", "k3", "bfd4d0c4", None),
    "Spec.KR.kernel_bankruptcy_residual_step_capacity_nopanic": ("kernel_bankruptcy_residual_step_capacity", "nopanic", "k3", "bfd4d0c4", None),
    "Spec.KR.kernel_bankruptcy_residual_step_capacity_fits": ("kernel_bankruptcy_residual_step_capacity", "fits", "k3", "bfd4d0c4",
        "booking the returned step never overflows the B index"),
    "Spec.KR.kernel_bankruptcy_residual_step_capacity_zero": ("kernel_bankruptcy_residual_step_capacity", "zero", "k3", "bfd4d0c4",
        "the step is 0 exactly when booking one atom would overflow the B index"),
    "Spec.KR.kernel_bankruptcy_residual_step_capacity_books": ("kernel_bankruptcy_residual_step_capacity", "books", "k3", "bfd4d0c4",
        "under the shape facts and the chunk bound, a nonzero step is always booked (no Recovery while the index has room)"),
    # task k4 (engine 80f8f0b4): the accrual kernels
    "Spec.Seg.kernel_accrual_price_move_within_cap_contract": ("kernel_accrual_price_move_within_cap", "contract", "k4", "80f8f0b4", None),
    "Spec.Seg.kernel_accrual_price_move_within_cap_nopanic": ("kernel_accrual_price_move_within_cap", "nopanic", "k4", "80f8f0b4", None),
    "Spec.Seg.kernel_accrual_funding_num_contract": ("kernel_accrual_funding_num", "contract", "k4", "80f8f0b4", None),
    "Spec.Seg.kernel_accrual_funding_num_nopanic": ("kernel_accrual_funding_num", "nopanic", "k4", "80f8f0b4", None),
    "Spec.Seg.kernel_accrual_index_update_contract_exact": ("kernel_accrual_index_update", "contract", "k4", "80f8f0b4", None),
    "Spec.Seg.kernel_accrual_index_update_nopanic": ("kernel_accrual_index_update", "nopanic", "k4", "80f8f0b4", None),
    "Spec.Seg.kernel_accrual_index_update_floors": ("kernel_accrual_index_update", "floors", "k4", "80f8f0b4",
        "on Ok each side's funding delta is the exact floor of -funding_num * a_long / FUNDING_DEN, resp. "
        "funding_num * a_short / FUNDING_DEN, in both branches"),
    "Spec.Seg.kernel_accrual_index_update_funding_conserves": ("kernel_accrual_index_update", "funding_conserves", "k4", "80f8f0b4",
        "per unit of effective position funding moves value from the paying side to the receiving side and creates "
        "none; the difference is under one index unit per side, and nothing when FUNDING_DEN divides the amount"),
    "Spec.Seg.kernel_accrual_index_update_mark_to_market": ("kernel_accrual_index_update", "mark_to_market", "k4", "80f8f0b4",
        "with equal A indices the two K deltas sum to zero (mark to market moves value between the sides and creates none)"),
}
FUNDING_ROWS = {  # task k3: the funding snapshot carried between settlements (settlement glue, not a listed kernel)
    "Spec.KF.scaled_adl_delta_fast_delta": ("KF.delta", "scaled_adl_delta_fast",
        "the fast funding delta is the exact floor of abs_basis * (now - then) / (a_basis * POS_SCALE); on-chain it also "
        "needs P20 and P21 (its calls into `floor_div_signed_conservative_i128`), since the deployed engine does not check "
        "overflow"),
    "Spec.KF.f_snap_after_settlement_nopanic": ("KF.nopanic", "f_snap_after_settlement", "no execution panics"),
    "Spec.KF.f_snap_after_settlement_carry_of_pos": ("KF.carry", "f_snap_after_settlement",
        "with the exact delta, the unpaid remainder is worth less than one atom and is never a debt"),
    "Spec.KF.f_snap_after_settlement_ok_exact": ("KF.ok", "f_snap_after_settlement",
        "with the exact delta, the exact set of inputs it refuses (its self-check never fires)"),
    "Spec.KF.f_snap_after_settlement_cadence": ("KF.cadence", "f_snap_after_settlement",
        "settling in n steps never credits an account more or charges it less than settling once, and the difference is "
        "bounded by the rounding of each step (exact when abs_basis divides den)"),
}
_k_of = {fn: k for k, fn in kernels.items()}
for p in props:
    row = BOUNDED_ROWS.get(p["name"])
    if row is not None and row[0] in _k_of:
        fn, suffix, task, rev, statement = row
        pid = f"{_k_of[fn]}.{suffix}"
        if pid not in pieces:
            piece(pid, "K kernel", fn, f"`{fn}`: {statement}, for every input", "Aristotle")
        if p["status"].startswith("PROVED"):
            pieces[pid]["status"] = "proved, final check pending" if "unchecked" in p["status"] else "proved"
            pieces[pid]["evidence"] = f"{p['name']} (engine {rev}, task {task}; the function is unchanged in {LANDED_REV})"
        continue
    frow = FUNDING_ROWS.get(p["name"])
    if frow is not None:
        pid, fn, statement = frow
        if pid not in pieces:
            piece(pid, "S settlement glue", fn, f"`{fn}`: {statement}", "Aristotle")
        if p["status"].startswith("PROVED"):
            pieces[pid]["status"] = "proved, final check pending" if "unchecked" in p["status"] else "proved"
            pieces[pid]["evidence"] = f"{p['name']} (engine bfd4d0c4, task k3; the function is unchanged in {LANDED_REV})"
# A closing task's final check of these pieces (the listing keeps the cancelled task's "unchecked" flag):
# confirmed_final_checks.tsv is read above, before these pieces exist, so apply it again here.
for _pid, _src in confirmed.items():
    if _pid in pieces and pieces[_pid]["status"] == "proved, final check pending":
        pieces[_pid]["status"] = "proved"
        pieces[_pid]["evidence"] += f"; final check confirmed in {_src}"
_k78c = f"{_k_of.get('kernel_accrual_index_update', 'K78')}.contract"
if _k78c in pieces and pieces[_k78c]["status"] == "proved" and any(
        p["name"] == "Spec.Seg.kernel_accrual_index_update_contract" and p["status"].startswith("REFUTED") for p in props):
    pieces[_k78c]["evidence"] += ("; this is the corrected contract: the clause as written at 80f8f0b4 is refuted "
                                  "(Spec.Seg.kernel_accrual_index_update_contract: its check `dfl * FUNDING_DEN` "
                                  "overflows at funding_num = +-i128::MAX although the kernel's result is the right "
                                  "floor); engine 08e8f7f0 carries the corrected clause")
# The public B chunk is a constant that keeps every B-settlement step clear of its overflow refusal (Kani, every
# budget), which with K91.refusals means no market can freeze positions that way. (Until engine 08e8f7f0 the chunk
# was a market setting and this piece was about config admission refusing a chunk above MAX_VAULT_TVL.)
if "kernel_b_settlement_chunk" in _k_of:
    _pid = f"{_k_of['kernel_b_settlement_chunk']}.admission"
    piece(_pid, "K kernel", "PUBLIC_B_CHUNK_ATOMS",
          "the public B chunk is the constant PUBLIC_B_CHUNK_ATOMS = MAX_VAULT_TVL, which keeps the B-settlement step's "
          "largest numerator inside u128 for every budget, so (with the exact refusal conditions) no market's B settlement "
          "fails with ArithmeticOverflow", "Kani")
    pieces[_pid]["status"] = "verified by Kani"
    pieces[_pid]["evidence"] = f"proof_public_b_chunk_keeps_b_settlement_free_of_overflow (Kani, every budget, both sides), engine {LANDED_REV}"
# INV-045 (reworded in program a406ce3b): what one accrual segment does with and without effective open
# interest. Kani lemmas in engine c65bb9a1 (src/v16_proofs.rs, refactor batch 10: proofs only, the deployed code is
# LANDED_REV's); the effective-price writers are listed in specs/inv045_effective_price_writers_71076137.md and the
# clauses of the statement, restated after its independent read, in specs/inv045_closure_71076137.md.
for _pid, _target, _text, _proof, _over in [
    ("AC.unexposed", "accrual_activity_for_asset_segment",
     "with no effective open interest on either side, a segment of either accrual path is neither price-move nor "
     "funding active, and the funding amount it books is zero",
     "proof_unexposed_accrual_segment_moves_no_equity", "every asset state, segment length, price and rate"),
    ("AC.exposed", "accrual_activity_for_asset_segment",
     "with effective open interest on either side, a segment that moves the effective price is equity active, so "
     "the single-segment accrual runs the price-move cap (K76) before it books the move",
     "proof_exposed_price_move_is_equity_active", "every asset state, segment length, price and rate"),
    ("AC.nofunding", "kernel_accrual_index_update",
     "a zero funding amount leaves both funding indices unchanged; only the K indices can move",
     "proof_index_update_without_funding_leaves_funding_indices",
     "closure layer, every K and F index, A index and price move"),
    ("AC.residue", "side_adl_totals_are_coherent",
     "a side the per-asset scan admits with raw basis and no effective open interest has A index 1, so its legs "
     "carry at most raw_basis_sum / ADL_ONE quantity units (the residue of engine issue #132)",
     "proof_scan_side_with_basis_and_no_open_interest_has_unit_a_index", "every raw basis, A index, weight and mode"),
]:
    piece(_pid, "S settlement glue", _target, f"`{_target}`: {_text}", "Kani")
    pieces[_pid]["status"] = "verified by Kani"
    pieces[_pid]["evidence"] = f"{_proof} (Kani, {_over}), engine c65bb9a1"
piece("AC.clamp", "W wrapper", "oracle_v16::clamp_toward_engine_dt",
      "`oracle_v16::effective_price_from_target` / `clamp_toward_engine_dt` (program): the crank's price is the clamp "
      "when the asset has effective open interest and the target when it has none; with a nonzero anchor and target "
      "the clamp lies between them and moves at most anchor * cap * dt / 10000, the bound the engine's price-move cap "
      "enforces; a trade-reported price goes through the same clamp", "Aristotle")
pieces["AC.clamp"]["evidence"] = ("queued: Aristotle task k4 Part 4 (KC.nopanic, KC.envelope, KC.reaches, KC.select) on the "
                                  f"verbatim functions of program ebcded58; model checking gave no verdict, even for "
                                  "16-bit caps and segment lengths")
_kc = {_p["name"]: _p["status"] for _p in props if _p["name"].startswith("Spec.Clamp.")}
_kc_names = ("Spec.Clamp.clamp_nopanic", "Spec.Clamp.clamp_envelope", "Spec.Clamp.clamp_reaches",
             "Spec.Clamp.effective_price_select")
if all(_kc.get(_n, "").startswith("PROVED") for _n in _kc_names):
    pieces["AC.clamp"]["status"] = ("proved" if not any("unchecked" in _kc[_n] for _n in _kc_names)
                                    else "proved, final check pending")
    pieces["AC.clamp"]["evidence"] = (", ".join(_kc_names) + f" (task k4, every input; the verbatim functions of "
                                      f"program ebcded58, unchanged in {WRAPPER_REV}, built as their own crate with "
                                      "overflow checks; nothing assumed about library functions)")
# C7 of the closure map: one jump and any chain of steps to the same price move the K indices alike
_k78 = _k_of.get("kernel_accrual_index_update")
if _k78 is not None:
    piece(f"{_k78}.path", "K kernel", "kernel_accrual_index_update",
          "`kernel_accrual_index_update`: on Ok the K indices move by exactly price_delta * a_long and "
          "-(price_delta * a_short), and two successive updates with the same A indices move them by the sum of the "
          "price deltas, so a jump and any chain of steps to the same price leave the same K, for every input", "Aristotle")
    pieces[f"{_k78}.path"]["evidence"] = "queued: Aristotle task k4 (KI.path)"
    for _p in props:
        if _p["name"] == "Spec.Seg.kernel_accrual_index_update_path" and _p["status"].startswith("PROVED"):
            pieces[f"{_k78}.path"]["status"] = "proved" if "unchecked" not in _p["status"] else "proved, final check pending"
            pieces[f"{_k78}.path"]["evidence"] = (f"{_p['name']} (engine 80f8f0b4, task k4, including the two-step "
                                                  f"statement; the function is unchanged in {LANDED_REV})")
# C0 of the closure map: the price range both accrual entry points accept
piece("AC.price_range", "S settlement glue", "accrue_asset_to_not_atomic",
      "`accrue_asset_to_not_atomic` and `accrue_asset_path_to_not_atomic` refuse every price of 0 or above "
      "MAX_ORACLE_PRICE with an error before the accrual body runs and leave the header and the asset slot unchanged "
      "(so a raw target of 0 is never accepted as a mark), and with the other inputs admitted every price from 1 to "
      "MAX_ORACLE_PRICE reaches the body", "Kani")
# Engine 4e77481b (batch 13) made the body after the prologue a function of its own, so a proof can stand in for it
# (a stand-in that fails the proof if it is called) and the solver decides the guard for a symbolic price.
pieces["AC.price_range"]["status"] = "verified by Kani"
pieces["AC.price_range"]["evidence"] = (
    "proof_accrual_prologue_refuses_every_price_outside_the_oracle_range (every price outside the range; a Live market; "
    "every asset index, clock slot, funding rate, flag and step slice of up to one step), "
    "proof_accrual_prologue_refuses_every_price_outside_the_oracle_range_for_every_header (every price outside the range; "
    "the header is arbitrary bytes; every asset index and clock slot) and proof_accrual_prologue_admits_every_price_inside_the_"
    "oracle_range (every price in the range reaches the body): Kani, closure layer, both entry points, with the body "
    "replaced by a stand-in. The proofs fix one asset slot and at most one step; that covers every market state "
    "because the prologue reads only the header, the number of asset slots (in one disjunct of the guard that also "
    "holds the price test) and its arguments, never the slot's contents or the steps, and assigns nothing "
    f"(roll_to_4e77481b.py checks this on the text), engine 4e77481b; the entry points and the proofs are unchanged "
    f"in {LANDED_REV} (specs/changed_since_4e77481b_at_{LANDED_REV}.md)")
# C1 and the route half of C4 of the closure map (specs/inv045_closure_<rev>.md): checked by a scan and by the
# program's tests, which is not a proof, so both stay open and INV-045's count shows them.
piece("AC.writers", "S settlement glue", "the effective price",
      "every write of an asset's effective price in the deployed engine and program is one of the writers that "
      "specs/inv045_effective_price_writers_<rev>.md lists: the two accruals, the initialization of an asset, of a "
      "generation or of a slot (including the zero-fill of new capacity), the anchor reset of an asset with no position "
      "or loss state, a copy of the asset's own price (write-backs, the canonicalization of a retired slot, the restore "
      "after a failed write), or the terminal close of the market account (INV-045 clause C1)", "Aristotle")
pieces["AC.writers"]["evidence"] = (
    "checked by a scan, not proved: effective_price_writers.py with specs/inv045_effective_price_writers_<rev>.tsv "
    "passes on the deployed sources (field assignments, asset-record literals, fresh records and empty slots, slot "
    "write-backs); it does not follow raw byte writes. A proof is the effective-price frame of every entry point and "
    "instruction (layers T and W)")
piece("AC.trade_routes", "W wrapper", "accepted_reported_trade_price_view",
      "every trade route accepts a reported price only through `accepted_reported_trade_price_view`, which, on an "
      "asset whose mark follows trades and can still move (`trade_updates_mark_view`: an EWMA mark or a soft-stale "
      "hybrid, Active or DrainOnly), clamps it with `clamp_toward_engine_dt` from the program's price average "
      "(`mark_ewma_e6`) over the slots since that average last moved, whatever the open interest, and otherwise "
      "accepts the engine's effective price (INV-045 clause C4, INV-046 clauses C2 and C3; the clamp itself is "
      "AC.clamp)", "Aristotle")
pieces["AC.trade_routes"]["evidence"] = (
    "test evidence, not proved: the program's composition test "
    "v16_program_mark_writer_and_trade_exit_composition_is_source_complete and a reading of both trade routes")
# INV-045, restated after its independent read (specs/inv045_closure_71076137.md): the link from the canonical step to
# the path body (clause C3) and the charter's funding-cadence paragraph (clause C9) are pieces of their own
piece("AC.path_link", "S settlement glue", "accrue_asset_path_to_admitted_not_atomic",
      "every step the path accrual books equals `canonical_accrual_price_step_v16(mark, target, anchor, cap, exposed, "
      "remainder)`, with the anchor the charter names (`fund_px_last`, or the mark when the call is given a target "
      "different from the stored one) and each remainder the previous step's; the first remainder of a call is the "
      "caller's, below MAX_MARGIN_BPS, so a call can exceed `n * anchor * cap` by less than one price unit (INV-045 "
      "clause C3)", "Kani or Aristotle (the path body against the canonical chain)")
pieces["AC.path_link"]["evidence"] = "a reading of the path body (the restated INV-045 statement); not proved"
piece("AC.cadence", "S settlement glue", "the funding of a fresh hybrid asset",
      "two histories with the same target that differ only in how often the oracle report is renewed accrue funding that "
      "differs by at most `max_abs_funding_e9_per_slot` of the notional per slot of the gap (INV-045 clause C9, the "
      "charter's paragraph on report cadence)", "Aristotle (a two-execution statement)")
pieces["AC.cadence"]["evidence"] = ("test evidence, not proved: the program's test "
                                    "v16_program_funded_report_cadence_differs_only_by_modelled_funding_and_preserves_custody")
piece("AC.walk_margin", "S settlement glue", "trade_preflight_risk_gate and target_lag_penalty",
      "the anchor-linear walk is safe for positions: no risk-increasing trade is admitted while an exposed asset's raw "
      "target differs from its mark, and every margin requirement (initial, maintenance, the liquidation leg, the risk "
      "notional and the health certificate) counts the whole adverse gap between the mark and the raw target from the "
      "slot the target changes; so a step that moves a large share of the mark late in a long walk moves it toward a "
      "price every position is already margined at (INV-045's statement bounds the step by the cap of the mark when the "
      "target took effect, not of the current mark)", "Kani or Aristotle (the gate and each requirement)")
pieces["AC.walk_margin"]["evidence"] = ("a reading of the engine (trade_preflight_risk_gate; target_lag_penalty in "
                                        "liquidation_leg_maintenance_requirement, the margin kernel and the health "
                                        "certificate); not proved")
piece("AC.anchor_writes", "S settlement glue", "the price-cap anchor (`fund_px_last`)",
      "every deployed write of an asset's price-cap anchor stores the mark of the same update: the path accrual "
      "(the mark when it is given a changed target, the mark on arrival), the single-segment accrual (its new price "
      "when the price moved or reached the stored target), the staging setter (the mark when the target changes), "
      "initialization and restart (the initial or authenticated price, which is the mark), the empty-asset reset, "
      "and copies of the asset's own record; so the anchor in INV-045's bound is always a mark the asset held when its "
      "current target took effect (AC.anchor proves the setter's case for every input)",
      "a writer scan like AC.writers, then Kani or Aristotle per writer")
pieces["AC.anchor_writes"]["evidence"] = "AC.anchor (Kani) covers the staging setter; the other writers are a reading"
# INV-045 clause C11 (program, 2026-11-23): a price that arrives after idle slots reaches back at most one window
piece("AC.catchup_push", "W wrapper", "handle_push_managed_mark",
      "while the market is live and the asset has effective open interest, PushAuthMark and PushEwmaMark refuse "
      "(EngineStale, no write) a mark that changes the asset's raw target when `now - slot_last > max_accrual_dt_slots` "
      "(`asset_accrual_within_one_window_view`); so such a target is applied to at most one window of the slots before "
      "it arrived (INV-045 clause C11, the charter's paragraph on a price that arrives after idle slots)",
      "Kani (the handler's guard, every input)")
pieces["AC.catchup_push"]["evidence"] = ("test evidence, not proved: the program's test "
                                         "v16_attack_authority_target_after_idle_slots_catches_up_at_most_one_window")
piece("AC.catchup_crank", "W wrapper", "the permissionless crank's live branch (hybrid_target_for_crank_view and the hold-back)",
      "while the market is live and not resolve-matured and the asset has effective open interest, a crank whose "
      "reading or EWMA mark would change the staged raw target of an asset with `now - slot_last > "
      "max_accrual_dt_slots` accrues toward the staged target instead and records the staged target in the oracle "
      "profile, keeping the reading's publish time and leg caches; so the reading is applied to at most one window of "
      "the slots before it arrived (INV-045 clause C11)", "Kani (the hold-back, every input)")
pieces["AC.catchup_crank"]["evidence"] = ("test evidence, not proved: the program's test "
                                          "v16_attack_fresh_reading_after_idle_slots_catches_up_at_most_one_window")
# INV-046, second read (2026-11-23): one owner for a soft-stale hybrid's target, and Recovery reductions stay open
piece("AC.hybrid_owner", "W wrapper", "hybrid_target_for_crank_view and prepare_trade_driven_mark_target_view",
      "past a hybrid's soft-stale window (slots since the feed last published a newer reading), every writer of its "
      "target stages the trade average (`mark_ewma_e6`): the crank, whether or not its reading is within "
      "`max_staleness_secs`, and a trade; a newer reading restores the feed price and restarts the window; so no caller "
      "chooses the target, and with it the side that carries the target-lag penalty, by choosing which writer acts "
      "last (INV-046 clause C5, the owner's rule A)", "Kani (the writers' agreement, every profile and reading)")
pieces["AC.hybrid_owner"]["evidence"] = ("test evidence, not proved: the program's test "
                                         "v16_attack_soft_stale_hybrid_target_does_not_depend_on_the_writer")
piece("AC.recovery_reduction", "W wrapper", "trade_resolve_matured_at_slot (the four trade routes)",
      "a trade on a Recovery asset is refused for staleness only when the whole market is stale "
      "(`permissionless_stale_matured`), never for the asset's own frozen timer; with the engine's rule that a Recovery "
      "asset admits no risk increase, its owners can reduce at the frozen price until the force close (INV-046 clause "
      "C6(6))", "Kani (the gate, every lifecycle and timer)")
pieces["AC.recovery_reduction"]["evidence"] = ("test evidence, not proved: the program's test "
                                               "v16_program_recovery_reduction_stays_open_after_the_stale_window")
# INV-077 open item found while landing C11 (2026-11-23): one maximum-shape measurement no longer fits
pieces["Y.compute"]["evidence"] = ("open item (2026-11-23), narrowed by engine 06b7e5be: on a market with a maintenance "
                                   "fee and the assets current, the CPI fill that opens the source-claim LP's fourteenth "
                                   "leg costs 1,118,713 CU (the tenth leg's exceeded the 1,400,000 CU limit before), and "
                                   "v16_program_public_full_shape_maintenance_has_bounded_continuation runs again; not "
                                   "measured: a full-shape account whose fee leaves it liquidatable (the crank that "
                                   "collects, voids and refreshes, then the owner's reducing fill), listed in AUDIT-077 "
                                   "(the program's tests/invariants/README.md)")
# INV-077 fee-sync fix (engine 06b7e5be): the skip and the crank's progress rule, with test evidence
piece("AC.fee_skip", "S settlement glue", "sync_account_fee_to_slot_not_atomic (fee_sync_settlement_noop_last_asset)",
      "a fee sync that does not run its side-effect settlement (a Live account whose certificate is current, PnL not "
      "negative, no source domain on a lapsed Fresh or an Impaired bucket, every active leg settled to its asset's K, F, "
      "K/F epoch and B) leaves the market and every account byte for byte as the settlement would, except the "
      "certificate: it keeps it current with the charge taken from its equity when that leaves no liquidation deficit, "
      "and that certificate is the one a full refresh of the post-state issues, except that a refresh first collects the "
      "backing utilization fee accrued on the account's liens, which the skip leaves to the next collection as any "
      "current certificate does in a market without a maintenance fee; otherwise it voids it as the settlement would "
      "(INV-054: a writer issues a certificate for the post-state or invalidates; INV-077)",
      "Kani (the skip's conditions against the settlement's writes, every account and market state)")
# Engine issue #274 (engine 263c34d4): the crank's B chunk after the account's K/F, with test evidence; the residual
piece("AC.settle_b_kf_first", "S settlement glue", "permissionless_crank_not_atomic (SettleB)",
      "the permissionless crank's B chunk settles the account's pending K/F first, at the committed indices, so a leg's B "
      "loss is applied after its K/F delta on every path; the order in which cranks reach a bankrupt loser and its "
      "winners does not change any account's terminal value, any provider's withdrawable backing or the junior residual "
      "(INV-041: caller order independence; spec requirement 27)",
      "Kani (the crank's B step against the side-effect settlement's composition, every account and market state)")
pieces["AC.settle_b_kf_first"]["evidence"] = ("test evidence, not proved: the engine's "
                                              "tests/v16_settle_b_crank_settles_kf_first.rs (one and two winners, a "
                                              "provider-backed loser domain, backing lapsing during the rise or after the "
                                              "liquidation; every crank order ends alike, nothing unowned or retired; the "
                                              "loser-first orders failed on the previous engine) and the program's "
                                              "reset-carry matrix ending with vault = C_tot + insurance")
piece("AC.insolvent_gain_order", "S settlement glue", "apply_signed_kf_delta_to_pnl (the cure of an insolvent account)",
      "OPEN (found in the issue #274 review): an account already insolvent when a gain arrives is valued against the "
      "support its gain's source domain holds at that moment, so whether its refresh comes before or after its loser pays "
      "into that domain changes its terminal value (measured: a winner refreshed first lost its whole 852,131 gain and "
      "the domain's backing went to its withdrawer); a design decision of the owner precedes any proof (INV-041)",
      "a statement the owner chooses, then a proof")
pieces["AC.fee_skip"]["evidence"] = ("test evidence, not proved: the engine's deterministic differentials against the "
                                     "settlement (v16_fee_sync_skip_writes_every_byte_the_settlement_writes_except_the_"
                                     "certificate and seven more v16_fee_sync_* tests, one of them showing the utilization-fee difference, engine "
                                     "95b6e9e1), the randomized differential "
                                     "v16_fuzz_fee_sync_skip_matches_the_settlement_except_the_certificate (256 cases) and "
                                     "seven mutation controls, each failing a deterministic test")
piece("AC.fee_crank", "W wrapper", "handle_permissionless_crank_zero_copy (the maintenance collection)",
      "a permissionless crank whose collection of a due maintenance fee advanced the portfolio's fee cursor succeeds and "
      "keeps the collection, also when the engine then finds nothing else to do; a crank with nothing left is refused "
      "with every write rolled back (INV-071)", "Kani (the handler's progress rule, every input)")
pieces["AC.fee_crank"]["evidence"] = ("test evidence, not proved: the program's test "
                                      "v16_program_crank_whose_only_work_is_a_due_fee_collects_it (fails without the rule)")
# INV-045: the anchor of the path accrual is the mark when the current target took effect; a staged target that changes
# restarts it (engine 71076137, batch 15)
piece("AC.anchor", "S settlement glue", "set_asset_raw_oracle_targets_not_atomic",
      "a target staged by `set_asset_raw_oracle_targets_not_atomic` that differs from the stored one sets the price-cap "
      "anchor (`fund_px_last`) to the effective price in the same write; an unchanged target leaves the asset slot as it "
      "was; a target of 0 or above MAX_ORACLE_PRICE is refused with no write (INV-045 clause C3: the anchor is the mark "
      "when the current target took effect, for a staged target as for one given to the path accrual)", "Kani",
      status="verified by Kani",
      evidence="proof_v16_changed_raw_target_restarts_the_cap_anchor (Kani, main, closure and contracts runs; every mark, "
               "anchor, stored target and new target, the first three in 1..=MAX_ORACLE_PRICE as the shape check requires "
               "of a live asset; one live asset with no position), engine 71076137")

# The trade pilot (task s2, engine d1fa39f6): the prover's names for the pieces TF, TA, TB and TC1-TC9.
TRADE_ROWS = {
    "Spec.Trade.tf_margin": "TF.margin", "Spec.Trade.tf_locked": "TF.locked", "Spec.Trade.tf_frame": "TF.frame",
    "finish_trade_checks_not_atomic.order": "TF.order",
    "apply_trade_after_refresh_not_atomic.fee": "TA.fee", "apply_trade_after_refresh_not_atomic.conservation": "TA.conservation",
    "apply_trade_after_refresh_not_atomic.positions": "TA.positions", "apply_trade_after_refresh_not_atomic.gate": "TA.gate",
    "apply_trade_after_refresh_not_atomic.frame": "TA.frame", "apply_trade_after_refresh_not_atomic.nopanic": "TA.nopanic",
    "Spec.Trade.tb_flag": "TB.flag", "Spec.Trade.tb_checked": "TB.checked",
    "Spec.Trade.tc1_charge_fee": "TC1", "Spec.Trade.tc2_preflight": "TC2", "Spec.Trade.tc3_risk_change": "TC3",
    "Spec.Trade.tc4_terminal_residual": "TC4", "Spec.Trade.tc5_position_delta": "TC5",
    "transfer_trade_residual_reward_credit.frame": "TC6", "Spec.Trade.tc7_recertify": "TC7",
    "Spec.Trade.tc8_oi_cap_gate": "TC8", "Spec.Trade.tc9_im_lien": "TC9",
}
_trade_unmapped = []
for p in props:
    name, st = p["name"], p["status"]
    pid = TRADE_ROWS.get(name)
    if pid is None:
        if name.startswith("Spec.Trade.") or name.startswith(("apply_trade_after_refresh_not_atomic.", "finish_trade_checks_not_atomic.",
                                                               "execute_batch_with_fee_loss_stale_scoped_not_atomic.")):
            _trade_unmapped.append(name)
        continue
    if st.startswith("PROVED"):
        pieces[pid]["status"] = "proved, final check pending" if "unchecked" in st else "proved"
        pieces[pid]["evidence"] = f"{name} (engine d1fa39f6, task s2; the function is unchanged in {LANDED_REV})"
    elif st.startswith("REFUTED"):
        pieces[pid]["status"] = "REFUTED"; pieces[pid]["evidence"] = name
    elif st.startswith("IN_PROGRESS") or st.startswith("IN PROGRESS"):
        pieces[pid]["status"] = "in progress"
    elif st:
        pieces[pid]["status"] = "attempted, open"
        pieces[pid]["evidence"] = f"{name}: stated, not proved in task s2 (aristotle/summaries/s2.md says how far each got)"
if _trade_unmapped:
    print("trade rows without a ledger piece:", ", ".join(sorted(_trade_unmapped)), file=sys.stderr)

# Task c0 (engine 71076137, specs/wide_math_callsites_spec_71076137.md): the wide-math call sites of class local and
# chained, and the six supporting pieces PL1, PL2, PR1-PR4. Its rows are named `<function>.<piece>_<what>`, e.g.
# `scaled_adl_delta_fast.p20_no_e3_panic` -> P20, `V16Core::kernel_recompute_resolved_payout_rate.pr1_...` -> PR1.
_c0_unmapped = []
for p in props:
    name, st = p["name"], p["status"]
    _m = re.search(r"\.(p\d\d|pl\d|pr\d)_[a-z0-9_]+$", name)
    if _m is None:
        continue
    pid = _m.group(1).upper()
    if pid not in pieces:
        _c0_unmapped.append(name)
        continue
    if st.startswith("PROVED"):
        pieces[pid]["status"] = "proved, final check pending" if "unchecked" in st else "proved"
        pieces[pid]["evidence"] = f"{name} (engine 71076137, task c0)"
    elif st.startswith("REFUTED"):
        pieces[pid]["status"] = "REFUTED"; pieces[pid]["evidence"] = name
    elif st.startswith("IN_PROGRESS") or st.startswith("IN PROGRESS"):
        pieces[pid]["status"] = "in progress"
    elif st:
        # c0 ended on 2026-11-23 (COMPLETE_WITH_ERRORS, about 400 credits, 14 of 20 proved). Its one failed row,
        # `...p13_p14_no_e5_panic`, covers both sites: the two closures are proved, the borrow of the leg's fields,
        # the two `unwrap_or_else` calls and the tail are not (aristotle/summaries/c0.md).
        for _pid in (("P13", "P14") if "p13_p14" in name else (pid,)):
            pieces[_pid]["status"] = "attempted, open"
            pieces[_pid]["evidence"] = (f"{name}: partly proved in task c0 (the closures are safe; the rest of the "
                                        "body is not proved); aristotle/summaries/c0.md")
if _c0_unmapped:
    print("c0 rows without a ledger piece:", ", ".join(sorted(_c0_unmapped)), file=sys.stderr)
# Task c1 (engine 71076137, engine/aristotle/engine_wrapping_arithmetic_c1_spec.md, submitted 2026-11-23): no-overflow
# of five functions the Kani batches could not finish or did not reach. Rows are named `<function>.c1_<n>_<what>`.
C1_PIECES = {
    "c1_1": "PO.liquidation_engine_close_request_q",
    "c1_2": "PO.V16Config::validate_solvency_envelope_range",
    "c1_3": "PO.V16Config::validate_exact_solvency_envelope",
    "c1_4": "PO.PositionSlotMapV16::from_account",
    "c1_5": "PO.PortfolioV16ViewMut::compact_source_domains",
}
for p in props:
    name, st = p["name"], p["status"]
    _m = re.search(r"\.(c1_\d)_[a-z0-9_]+$", name)
    if _m is None or _m.group(1) not in C1_PIECES:
        continue
    pid = C1_PIECES[_m.group(1)]
    if st.startswith("PROVED"):
        pieces[pid]["status"] = "proved, final check pending" if "unchecked" in st else "proved"
        pieces[pid]["evidence"] = f"{name} (engine 71076137, task c1)"
    elif st.startswith("REFUTED"):
        pieces[pid]["status"] = "REFUTED"; pieces[pid]["evidence"] = name
    elif st.startswith("IN_PROGRESS") or st.startswith("IN PROGRESS"):
        pieces[pid]["status"] = "in progress"
    elif st:
        pieces[pid]["status"] = "attempted, open"
        pieces[pid]["evidence"] = f"{name}: stated, not proved in task c1 (aristotle/summaries/c1.md)"

# PL2 and PR2-PR4 were not attempted: on the budget note of 2026-11-23 the task returned, instead of proofs, the
# enumeration each needs (callers of the leg function; writers of the payout ledger, of the snapshot flag and of a
# present receipt). The enumerations are in aristotle/summaries/c0.md, to be checked against the source.
# A direct-assignment scan of src_71076137/v16.rs (2026-11-23) finds the same sites: PL2, the two default-build
# callers (prepare_account_kf_settlement_entry, settle_forfeited_leg_kf_effects) and one cfg(kani) caller; PR2, six
# writes of header.resolved_payout_ledger in five functions; PR3, one write of payout_snapshot_captured (= 1, in
# initialize_resolved_payout_ledger_if_needed); PR4, one write of `present: true` (create_resolved_payout_receipt_if_
# needed). The scan does not see writes through other paths (a raw byte view), and the per-site arguments are read,
# not proved, so the pieces stay open.
C0_SCAN = {
    "PL2": "callers: prepare_account_kf_settlement_entry, settle_forfeited_leg_kf_effects (+ one cfg(kani))",
    "PR2": "writers: credit_post_snapshot_residual_not_atomic, recompute_resolved_payout_rate, "
           "initialize_resolved_payout_ledger_if_needed, create_resolved_payout_receipt_if_needed (2), "
           "refine_resolved_unreceipted_bound_not_atomic",
    "PR3": "one write: initialize_resolved_payout_ledger_if_needed",
    "PR4": "one write of present: true: create_resolved_payout_receipt_if_needed",
}
for _pid in ("PL2", "PR2", "PR3", "PR4"):
    if pieces[_pid]["status"] == "open":
        pieces[_pid]["evidence"] = ("not attempted; task c0 returned the enumeration this piece needs "
                                    "(aristotle/summaries/c0.md) and a direct-assignment scan of the source agrees ("
                                    + C0_SCAN[_pid] + "); the per-site arguments are not proved")

# The three provider-protection kernels are small scalar functions that Kani verifies for all inputs.
# They are not queued for Aristotle (the owner's priority: Aristotle goes where Kani has gaps).
KANI_FULL_WIDTH = {
    "K61.contract": "contract_check_kernel_provider_protection_num (Kani, every u128 input)",
    "K61.nopanic": "contract_check_kernel_provider_protection_num (Kani, every u128 input; a panic would fail it)",
    "K62.contract": "contract_check_kernel_provider_protection_floor_observe (Kani, every u128 input)",
    "K62.nopanic": "contract_check_kernel_provider_protection_floor_observe (Kani, every u128 input; a panic would fail it)",
    "K63.contract": "contract_check_kernel_provider_protection_floor_shift (Kani, every input)",
    "K63.nopanic": "contract_check_kernel_provider_protection_floor_shift (Kani, every input; a panic would fail it)",
    "K61.realization": "proof_v16_provider_protection_kernel_laws (Kani, every u128 input)",
    "K61.monotone": "proof_v16_provider_protection_kernel_laws (Kani, every u128 input)",
    "K62.minimum": "proof_v16_provider_protection_floor_is_a_running_minimum (Kani, every u128 input, two observations in both "
                   "orders; any longer sequence follows by induction)",
    "K63.inverse": "proof_v16_provider_flows_move_the_floor_with_the_principal (Kani, 64-bit amounts)",
}
for _k, _fn in REFACTOR_KERNELS.items():
    if _k in BOUNDED_KERNELS:
        continue
    KANI_FULL_WIDTH[f"{_k}.contract"] = f"contract_check_{_fn} (Kani, every input)"
    KANI_FULL_WIDTH[f"{_k}.nopanic"] = f"contract_check_{_fn} (Kani, every input; a panic would fail it)"
for pid, ev in KANI_FULL_WIDTH.items():
    if pieces[pid]["status"] == "open":
        pieces[pid]["status"] = "verified by Kani"
        pieces[pid]["evidence"] = ev + f", engine {LANDED_REV}"

# Carry-over to LANDED_REV. Layers A and K: nothing has to be proved again (see the note at LANDED_REV).
# The entry-point pieces of REESTABLISH were proved for NEW_REV and are not counted for LANDED_REV.
for pid, p in pieces.items():
    st = p["status"]
    if not st.startswith("proved"):
        continue
    if pid in REESTABLISH:
        p["status"] = f"proved for {NEW_REV}; to be established again for {LANDED_REV}"
        p["evidence"] += (f"; the function's text is unchanged in {LANDED_REV}, but it is a method over, or builds, a type that "
                          f"gained fields there (specs/changed_since_{NEW_REV}_at_{LANDED_REV}.md)")
    elif p["layer"].startswith("K") and "Spec.Kernel" in p["evidence"]:
        p["evidence"] += f"; unchanged in {LANDED_REV} (specs/changed_since_{NEW_REV}_at_{LANDED_REV}.md)"
    elif p["layer"].startswith("A") and "src/wide_math.rs is identical" in p["evidence"]:
        p["evidence"] += f" and in {LANDED_REV}"

# ---------------------------------------------------------------- invariants -> pieces
def expand_k(text):
    out = []
    for a, b in re.findall(r"K(\d+)(?: to K(\d+))?", text):
        lo, hi = int(a), int(b or a)
        out += [f"K{n:02d}" for n in range(lo, hi + 1)]
    return sorted(set(out))

arith_ids = [p for p in pieces if pieces[p]["layer"].startswith("A")]
group = lambda letters: [p for p in arith_ids if p[0] in letters]
extra = {  # pieces the coverage matrix names only in words
    "INV-030": group("D"), "INV-038": group("G"), "INV-083": group("ABCDEF"), "INV-085": group("ABCDEF"),
    "INV-048": ["K59.contract", "K59.nopanic"], "INV-051": ["K59.contract", "K59.nopanic", "K60.contract", "K60.nopanic"],
}
for _inv, _ids in {"INV-045": ["K76.contract", "K76.nopanic", "K93.contract", "K93.nopanic", "K93.envelope", "K93.reaches"],
                   "INV-052": ["KF.cadence", "KF.carry", "KF.delta", "P20", "P21"],
                   "INV-071": ["K88.contract", "K88.zero", "K88.books"],
                   "INV-073": ["K88.books", "K91.refusals", "K91.admission"]}.items():
    extra[_inv] = extra.get(_inv, []) + [i for i in _ids if i in pieces]
extra["INV-045"] = extra.get("INV-045", []) + [i for i in ("AC.unexposed", "AC.exposed", "AC.nofunding", "AC.residue",
                                                         "AC.clamp", "AC.price_range", "AC.writers",
                                                         "AC.trade_routes", "AC.anchor", "AC.path_link",
                                                         "AC.cadence", "AC.walk_margin", "AC.anchor_writes",
                                                         "AC.catchup_push", "AC.catchup_crank",
                                                         "K59.contract")
                                                         if i in pieces]
# INV-085 (proven arithmetic equals deployed arithmetic): the engine's unchecked arithmetic is where they can differ
extra["INV-046"] = extra.get("INV-046", []) + [i for i in ("AC.hybrid_owner", "AC.recovery_reduction") if i in pieces]
extra["INV-054"] = extra.get("INV-054", []) + [i for i in ("AC.fee_skip", "K94.contract") if i in pieces]
extra["INV-071"] = extra.get("INV-071", []) + [i for i in ("AC.fee_crank",) if i in pieces]
extra["INV-077"] = extra.get("INV-077", []) + [i for i in ("AC.fee_skip",) if i in pieces]
extra["INV-041"] = extra.get("INV-041", []) + [i for i in ("AC.settle_b_kf_first", "AC.insolvent_gain_order") if i in pieces]
extra["INV-085"] = extra.get("INV-085", []) + sorted(i for i in pieces if i.startswith("PO."))
if "kernel_accrual_index_update" in _k_of:
    extra["INV-045"] += [i for i in (f"{_k_of['kernel_accrual_index_update']}.contract",
                                     f"{_k_of['kernel_accrual_index_update']}.path") if i in pieces]
liveness = {"INV-046", "INV-057", "INV-069", "INV-071", "INV-072", "INV-073", "INV-078", "INV-082"}
invariants = []
for line in read("coverage_matrix.md").splitlines():
    m = re.match(r"\| (INV-\d+) ([^|]*)\| ([^|]*)\| ([^|]*)\| ([^|]*)\|", line)
    if not m:
        continue
    inv, name, kind, stage, what = (x.strip() for x in m.groups())
    ids = []
    for k in expand_k(what):
        ids += [f"{k}.contract", f"{k}.nopanic"]
    ids += extra.get(inv, [])
    stages = [s.strip() for s in stage.split(",")]
    if "D" in stages:
        ids += ["T*"] + (["PO*"] if "PO*" not in ids else [])
    if "C" in stages:
        ids.append("W*")
    if inv in liveness:
        ids += ["L*"]
    invariants.append(dict(inv=inv, name=name, kind=kind, stage=stage, what=what, ids=ids))

CLOSE_HOW = {
    "safety": "write the invariant as a predicate on states or on single transitions; derive it from the global invariant (layer G) and "
              "the pieces listed for it (Aristotle)",
    "history": "write the invariant as a statement about two executions (two orders, two routes, a split and its whole); prove it "
               "from the kernels' algebra (Aristotle)",
    "liveness": "write the invariant as: a rank that every successful step lowers, and a step that is enabled in every non-terminal "
                "state (Aristotle); termination itself is evidence (Y.terminates, Y.compute)",
    "compute": "evidence: max-shape compute measurements (Y.compute)",
    "process": "evidence about the verification itself; closed when the ledger shows it (no theorem)",
    "n/a": "prove that the feature has no reachable consumer in the deployed program (Aristotle)",
}
def close_how(kind):
    parts = [k.strip() for k in kind.replace(" + ", ",").split(",")]
    return "; ".join(CLOSE_HOW.get("liveness" if k == "termination" else k, CLOSE_HOW["safety"]) for k in parts)
for v in invariants:
    num = v["inv"].split("-")[1]
    needs = ", ".join(sorted({i.split(".")[0] for i in v["ids"]})) or "no lower piece named yet"
    piece(f"I{num}", "I invariants closed", f"{v['inv']} {v['name']}",
          f"{v['inv']} ({v['kind']}) holds for the deployed program, as a formal statement that an independent reader has checked "
          f"against the charter text. Rests on: {needs}",
          close_how(v["kind"]))

# Overflow: which proved statements carry to the deployed build, where the engine wraps on overflow (every proof was
# made with overflow checks on). No-panic pieces, the arithmetic layer, the P pieces and Kani's verdicts carry by
# themselves. Any other proved piece carries when the no-panic pieces it needs are done: a kernel piece needs its
# kernel's; every other one is listed in NOPANIC_BASIS, and a proved piece in neither group stops the script.
NOPANIC_BASIS = {
    "KF.cadence": ("KF.nopanic",), "KF.carry": ("KF.nopanic",), "KF.ok": ("KF.nopanic",),
    # scaled_adl_delta_fast: its own operations are divisions by a nonzero constant; its calls into wide_math are P20, P21
    "KF.delta": ("P20", "P21"),
    "TC8": ("K17.nopanic",),     # kernel_trade_side_oi_cap_gate is K17
    "AC.clamp": (),              # program code, which the deployed program checks; Spec.Clamp.clamp_nopanic is part of it
}
carry_exceptions = []
for _p in pieces.values():
    if _p["status"] != "proved" or _p["id"].endswith(".nopanic") or _p["layer"].startswith(("A ", "P ")):
        continue
    _m = re.match(r"(K\d+)\.", _p["id"])
    if _m:
        _need = (f"{_m.group(1)}.nopanic",)
    else:
        assert _p["id"] in NOPANIC_BASIS, f"proved piece {_p['id']}: record in NOPANIC_BASIS what rules out overflow for it"
        _need = NOPANIC_BASIS[_p["id"]]
    _open = [n for n in _need if pieces.get(n, {}).get("status") not in ("proved", "verified by Kani", "proved by rule")]
    if _open:
        carry_exceptions.append((_p["id"], _open))

def counts(ids):
    concrete = [i for i in ids if not i.endswith("*")]
    done = sum(1 for i in concrete if pieces[i]["status"] in ("proved", "verified by Kani", "proved by rule"))  # as the roadmap's DONE
    return done, len(concrete)

# ---------------------------------------------------------------- write
by_layer = {}
for p in pieces.values():
    by_layer.setdefault(p["layer"], []).append(p)
total_proved = sum(1 for p in pieces.values() if p["status"] == "proved")
total_pending = sum(1 for p in pieces.values() if p["status"] == "proved, final check pending")
now = datetime.datetime.utcnow().strftime("%Y-%m-%d")  # the date only: a publish with no change leaves no diff
md = []
md.append("# Proof ledger: the charter's invariants as provable pieces\n")
md.append(f"Generated {now} by `make_ledger.py` from the live Aristotle property list. "
          f"**{total_proved} of {len(pieces)} pieces are proved and checked; {total_pending} more are proved "
          f"with the final check pending** (their task was stopped before its closing check).\n")
md.append("## The end goal\n")
md.append("""Every one of the charter's 89 invariants is **closed** for the revision that is deployed (layer I): it has a
formal statement that an independent reader has checked against the charter text, and that statement
is proved from the layers below it, or, for the few that are about compute or about the verification
itself, is established by the evidence named for it. The layers below are what such a proof needs:

- **G, the global invariant.** H (the header checks), S (the per-asset scan), V (every portfolio
  account valid against the market), M (market totals equal the sums over all accounts) and C
  (custody: the vault token accounts hold what the header says). It holds initially and every
  successful instruction preserves it.
- **W, the program's instructions** preserve G and check authority; **T, the engine's entry points**
  do the same one level down, in four halves (header, scan, account, mirror); **X** covers the
  places where state is kept after an error; **K, A and P** are the kernels, the arithmetic and the
  absence of panics under them.
- **L, liveness**, as ranks and enabledness.
- **Y, what is assumed** (the Solana runtime, the token program, the oracle formats, the compiler,
  the prover) **or shown by evidence** (compute bounds, termination, the review of the statements).

The count at the top of this file is against that whole list. See `ROADMAP.md` for the order.
""")
md.append("## How an invariant is split\n")
md.append("""1. **State predicate.** A safety invariant is a predicate on engine state. The engine already carries
   most of them as executable checks (`validate_shape`, `validate_with_market`, the audit scans).
2. **One obligation per public entry point (layer T).** If the predicate held before the call and the
   call returns Ok, it holds after. The engine has %d public entry points, so each invariant becomes at
   most %d obligations, most of them "this call does not write those fields".
3. **Kernel contracts (layer K).** Each entry point does its arithmetic and bookkeeping through small
   pure kernels with a written contract. An obligation of layer T follows from the contracts of the
   kernels it calls. Two properties per kernel: its contract, and no panic.
4. **Arithmetic (layer A).** Kernels that multiply and divide rest on `wide_math.rs`.
5. **Wrapper (layer W).** Each Solana instruction checks its accounts, calls the engine and moves
   tokens by exactly what the engine booked.
6. **Liveness (layer L).** Aristotle proves no termination, so a liveness invariant is split into two
   safety pieces it can take (every successful step lowers a rank; a step is enabled whenever the state
   is not terminal) and one it cannot (loops are bounded), which stays with the bounded-loop audit.

A piece is **proved** only when Aristotle reports it proved and checked against the deployed function
body. Kernel proofs name every library function whose documented behaviour they assume; those are
listed in the run summaries (`b1f_extract`, `b2_dl`, ...). A piece is **verified by Kani** only when a
Kani harness checks it for every input it quantifies over; a check over bounded or concrete inputs
leaves the piece open, and its row says what was checked. Two kinds of piece are counted as proved
on other evidence, and say so in their row: a call site whose callee Aristotle proved never to panic
(nothing is left to prove at the site), and `P.surface`, which the Rust compiler establishes (the
engine builds against a stand-in module that exposes only the listed items).

**Overflow.** Every proof here was made with overflow checks on: Kani always checks, and the Aristotle
specs said so. With checks on, an overflowing operation stops the call. The deployed program checks
its own arithmetic but builds the engine without overflow checks (its release profile: with the engine
checked, the 14-leg atomic exit of INV-073 exceeds the compute limit), so there an overflow continues
with a wrapped value. A no-panic piece therefore also rules out wrapping and carries to the deployed
build; a piece of the form "if the call returns, then ..." carries where the operations it runs cannot
overflow. The `PO.*` pieces are those obligations, one per engine function
(`specs/engine_wrapping_arithmetic_<rev>.md`). Which proved pieces carry is checked at every
generation, in the next paragraph.
""" % (len(entries), len(entries)))
md.append("**Which proved pieces carry to the deployed build.** Checked at this generation: no-panic pieces, the "
          "arithmetic layer, the P pieces and Kani's verdicts carry by themselves; a kernel's other proved pieces carry "
          "with its no-panic piece; every other proved piece carries with the no-panic pieces recorded for it in "
          "`make_ledger.py` (`NOPANIC_BASIS`). " + ("Every proved piece carries." if not carry_exceptions else
          "These do not yet; each names the open pieces it also needs: "
          + "; ".join(f"`{i}` ({', '.join(n)})" for i, n in carry_exceptions) + ".") + "\n")
# The revisions the proofs were made on, each step checked by changed_fns.py; listed up to LANDED_REV.
_ROLLS = [
    ("add1d027", "c030aa40", "the #449 change: nine kernels do not pass the check (two are new, four changed, three take "
     "the asset record, which grew) and are counted as proved only once Aristotle proved them again on c030aa40"),
    ("c030aa40", "d1fa39f6", "the provider-protection floor (percolator#271): every kernel passes, and K61 to K63 are "
     "new. The two entry-point pieces proved on c030aa40 (T09.header and T38.header) do not pass, because the market "
     "view and the per-asset slot gained fields; they are listed as proved for c030aa40 and to be established again"),
    ("d1fa39f6", "cb78f1f0", "refactor batches 1 and 2, the engine's logic moved into pure kernels: no listed kernel "
     "changed, and K64 to K73 are new"),
    ("cb78f1f0", "80f8f0b4", "refactor batches 3 to 9 and the admission bound on the public B chunk: no listed kernel "
     "changed (the flag on K35 is a false positive: a callee gained only a contract attribute), and K74 to K93 are new"),
    ("80f8f0b4", "08e8f7f0", "refactor batch 12, the public B chunk becomes a constant: no listed kernel changed (K78's "
     "contract attribute did, which the comparison does not see; its body did not)"),
    ("08e8f7f0", "4e77481b", "refactor batch 13, the accrual's price check proved for every price: no listed kernel "
     "changed; the two accrual entry points call the rest of the accrual, moved verbatim, as a function of its own"),
    ("4e77481b", "71076137", "batch 14 (fifteen no-overflow proofs, no code change) and batch 15, a staged target restarts "
     "the path accrual's price-cap anchor: one setter changed, no listed kernel changed or reaches it"),
    ("71076137", "06b7e5be", "batches 16, 18 and 19 (no-overflow proofs, no code change) and the INV-077 fee-sync fix: the "
     "fee sync changed, three functions new (one of them kernel K94, contract-checked), no listed kernel changed or "
     "reaches them"),
    ("06b7e5be", "263c34d4", "engine issue #274: the crank's B chunk settles pending K/F first; one function changed, no "
     "listed kernel changed or reaches it"),
]
_steps = _ROLLS[:next(i for i, r in enumerate(_ROLLS) if r[1] == LANDED_REV) + 1]
md.append(f"**Revisions.** The ledger describes engine {LANDED_REV} and program {WRAPPER_REV}, which pins it. "
          "Proofs were made against the engine revisions below, each when it was current. A proof holds for a later "
          "revision wherever the function, everything it calls and every type it names are textually identical; "
          "`changed_fns.py` checks that function by function at each step, and `src/wide_math.rs` is identical in "
          "all of them (`specs/wide_surface_census_<rev>.txt`).\n")
for _a, _b, _what in _steps:
    md.append(f"- From {_a} to {_b} (`specs/changed_since_{_a}_at_{_b}.md`): {_what}.")
md.append("")
md.append("## Totals\n")
md.append("| Layer | Pieces | Not in the deployed build | Proved and checked | Proved, final check pending | In progress or attempted | Open |\n|---|---|---|---|---|---|---|")
for layer in sorted(by_layer):
    ps = by_layer[layer]
    nd = sum(1 for p in ps if p["status"] == NOT_DEPLOYED)
    pr = sum(1 for p in ps if p["status"] == "proved")
    pe = sum(1 for p in ps if p["status"] == "proved, final check pending")
    op = sum(1 for p in ps if p["status"] == "open")
    md.append(f"| {layer} | {len(ps)} | {nd} | {pr} | {pe} | {len(ps) - nd - pr - pe - op} | {op} |")
md.append("")
kinds = {}
for e, k in t_kind.items():
    kinds.setdefault(k, []).append(e)
md.append("Layer T has two pieces per entry point. `.header`: the production shape check (header totals and "
          "flags). Each deployed entry point was read path by path (classes and what was found are in "
          "`specs/transition_shape_spec_<rev>.md`). By class: " + "; ".join(f"{k}: {len(v)}" for k, v in sorted(kinds.items()))
          + ". Classes final and delegates reduce to \"every Ok path ends in the runtime check\"; local and frame are "
          "small proofs; real, the unchecked paths of the mixed ones and the dispatcher carry the proofs that matter. "
          "Since program #485 the program checks the header before every commit of a crank and of a RebalanceReduce, "
          "so for those two instructions the header half also holds by a runtime check. `.scan`: the per-asset scan "
          "that only audit builds run; every entry point carries a real proof for it.\n")
md.append("## Each invariant and its pieces\n")
md.append("`T*`, `W*` and `L*` mean every piece of that layer applies (frame obligations included); `PO*` means every "
          "piece of the engine's unchecked arithmetic (see \"Overflow\" above).\n")
md.append("| Invariant | Kind | Pieces | Pieces done (proved, or verified by Kani) | Closed |\n|---|---|---|---|---|")
for v in invariants:
    done, n = counts(v["ids"])
    ids = v["ids"]
    short = ", ".join(sorted({i.split(".")[0] for i in ids})) if ids else "none yet: " + v["what"]
    closed = pieces["I" + v["inv"].split("-")[1]]["status"]
    md.append(f"| {v['inv']} {v['name']} | {v['kind']} | {short} | {done} of {n} | {'yes' if closed == 'proved' else 'no'} |")
md.append("")
for layer in sorted(by_layer):
    md.append(f"## Layer {layer}\n")
    md.append("| Piece | Target | Statement | Status | Evidence |\n|---|---|---|---|---|")
    for p in by_layer[layer]:
        md.append(f"| {p['id']} | `{p['target']}` | {p['statement']} | {p['status']} | {p['evidence']} |")
    md.append("")
open(os.path.join(HERE, "proof_ledger.md"), "w").write("\n".join(md) + "\n")
with open(os.path.join(HERE, "proof_ledger.tsv"), "w") as f:
    f.write("id\tlayer\ttarget\tstatus\tevidence\tmethod\tstatement\n")
    for p in pieces.values():
        f.write("\t".join(p[k] for k in ("id", "layer", "target", "status", "evidence", "method", "statement")) + "\n")
total_nd = sum(1 for p in pieces.values() if p["status"] == NOT_DEPLOYED)
print(f"{len(pieces)} pieces ({total_nd} not in the deployed build), {total_proved} proved and checked, {total_pending} proved with final check pending; {len(invariants)} invariants; "
      f"{len(entries)} entry points, {len(variants)} instructions")
for layer in sorted(by_layer):
    ps = by_layer[layer]
    print(f"  {layer}: {sum(1 for p in ps if p['status']=='proved')}/{len(ps)} proved")
