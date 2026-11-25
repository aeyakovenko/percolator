#!/usr/bin/env python3
"""header_local_check.py <engine src dir>

The facts the header harnesses of class "local" rest on (src/v16_proofs.rs, the section after the
marker "Header pieces of class \"local\""), checked on the source. Stops at the first that fails.

1. Each memo stand-in's key is the whole input of the function it replaces, and nothing else feeds
   that function:
   - `MarketGroupV16HeaderAccount::validate_dynamic_market_slots_len_static`, `V16Core::
     amount_from_bound_num` and `V16Core::bound_num_from_amount` take exactly the arguments the
     key holds, by value, and no `self`;
   - `V16ConfigAccount::try_to_runtime_shape` takes only `&self`, a `Pod` type (key: its bytes);
   - `V16Config::validate_public_user_fund_shape` takes only `&self`, whose `PartialEq` is derived
     (key: the value);
   - `MarketGroupV16View::validate_header_aggregate_totals` takes only `&self` and reads exactly
     the nine header fields the stand-in's key lists (in its order), and uses `self` no other way;
   - the deployed sources declare no `static` (no other state a function can read).
2. The audit-only scan: its `cfg(any(test, kani, feature = "audit-scan"))` block in
   `MarketGroupV16View::validate_shape` holds exactly `self.validate_shape_full_audit_scan()?;`, so
   a stand-in that returns `Ok(())` gives the default build's predicate; the stand-in does.
3. What runs as written differs in no way between a Kani build and a deployed build: cfg_reach.py
   over the predicate and the entry points, with the stand-ins, reports only the block of 2.
4. The entry points read no slot of the view (no `.markets`), so the slot count enters the proof
   only through the memoized slot-count check.
"""
import os, re, subprocess, sys

HERE = os.path.dirname(os.path.abspath(__file__))
ENTRY = {"T09.header": "advance_resolved_slot_not_atomic", "T05.header": "activate_empty_market_slot_not_atomic",
         "T03.header": "activate_empty_asset_slot_not_atomic"}
STUBBED = ["validate_shape_full_audit_scan", "validate_dynamic_market_slots_len_static", "amount_from_bound_num",
           "bound_num_from_amount", "validate_header_aggregate_totals", "try_to_runtime_shape",
           "validate_public_user_fund_shape"]
MARKER = "\n// Header pieces of class \"local\" (T*.header): the real predicate before and after the call.\n"


def fn_text(lines, name, impl_line):
    hits = [i for i, l in enumerate(lines) if re.match(r"\s{4}(?:pub(?:\([a-z]+\))? )?fn " + name + r"\b", l)]
    out = []
    for i in hits:
        k = i
        while not lines[k].startswith("impl"):
            k -= 1
        if lines[k] == impl_line:
            j = i
            while lines[j] != "    }":
                j += 1
            out.append("\n".join(lines[i:j + 1]))
    assert len(out) == 1, (name, impl_line, len(out))
    return out[0]


def signature(text):
    head = text[:text.index("{")]
    return re.sub(r"\s+", " ", head).strip()


def main(d):
    v16 = open(os.path.join(d, "v16.rs")).read()
    lines = v16.split("\n")
    proofs = open(os.path.join(d, "v16_proofs.rs")).read()
    assert proofs.count(MARKER) == 1, "the local section"
    sec = proofs[proofs.index(MARKER):]
    # 1. keys
    sig = signature(fn_text(lines, "validate_dynamic_market_slots_len_static", "impl MarketGroupV16HeaderAccount {"))
    assert sig == ("fn validate_dynamic_market_slots_len_static( supplied_len: usize, capacity: usize, "
                   "configured_market_slots: usize, ) -> V16Result<()>"), sig
    assert "let key = (supplied_len, capacity, configured_market_slots);" in sec
    for name, arg in (("amount_from_bound_num", "bound_num"), ("bound_num_from_amount", "amount")):
        sig = signature(fn_text(lines, name, "impl V16Core {"))
        assert sig == f"fn {name}({arg}: u128) -> V16Result<u128>", sig
        assert f"Some((k, r)) if k == {arg} => r," in sec, name
    sig = signature(fn_text(lines, "try_to_runtime_shape", "impl V16ConfigAccount {"))
    assert sig == "pub fn try_to_runtime_shape(&self) -> V16Result<V16Config>", sig
    i = v16.index("pub struct V16ConfigAccount {")
    assert "bytemuck::Pod" in v16[v16.rindex("#[derive(", 0, i):i], "V16ConfigAccount is Pod"
    assert "key.copy_from_slice(bytemuck::bytes_of(config));" in sec
    sig = signature(fn_text(lines, "validate_public_user_fund_shape", "impl V16Config {"))
    assert sig == "fn validate_public_user_fund_shape(&self) -> V16Result<()>", sig
    i = v16.index("pub struct V16Config {")
    assert "PartialEq" in v16[v16.rindex("#[derive(", 0, i):i] and "PartialEq for V16Config" not in v16
    assert "Some((k, r)) if k == *config => r," in sec
    agg = fn_text(lines, "validate_header_aggregate_totals", "impl<'a, T> MarketGroupV16View<'a, T> {")
    assert signature(agg) == "fn validate_header_aggregate_totals(&self) -> V16Result<()>", signature(agg)
    body = re.sub(r"//[^\n]*", "", agg[agg.index("{") + 1:])
    reads = re.findall(r"\bself\s*\.\s*header\s*\.\s*([a-z_0-9]+)", body)
    assert len(re.findall(r"\bself\b", body)) == len(reads), "validate_header_aggregate_totals uses self otherwise"
    stand = sec[sec.index("fn memo_header_aggregate_totals"):]
    stand = stand[:stand.index("\n}\n")]
    key = re.findall(r"\bh\.([a-z_0-9]+)\.get\(\)", stand)
    assert len(key) == 9 and len(set(key)) == 9, key
    assert set(reads) == set(key), (sorted(set(reads) ^ set(key)))
    for f in ("v16.rs", "lib.rs", "wide_math.rs"):
        t = re.sub(r"//[^\n]*", "", open(os.path.join(d, f)).read())
        assert not re.search(r"(?<!')\bstatic\b", t), f"a static in {f}"
    # 2. the audit-only scan
    vs = fn_text(lines, "validate_shape", "impl<'a, T> MarketGroupV16View<'a, T> {")
    blocks = re.findall(r"#\[cfg\(([^\]]*)\)\]\s*\{([^{}]*)\}", vs)
    assert blocks == [('any(test, kani, feature = "audit-scan")', blocks[0][1])] if blocks else False, blocks
    assert re.sub(r"\s+", "", blocks[0][1]) == "self.validate_shape_full_audit_scan()?;", blocks[0][1]
    assert len(re.findall(r"#\[cfg|cfg!\(", vs)) == 1, "another cfg in validate_shape"
    st = sec[sec.index("fn default_build_has_no_audit_scan"):]
    st = st[st.index("{", st.index("where")) + 1:st.index("\n}\n")]
    assert re.sub(r"\s+", "", st) == "Ok(())", st
    # 3. build differences in what runs as written
    r = subprocess.run([sys.executable, os.path.join(HERE, "cfg_reach.py"), d, "validate_shape", *ENTRY.values(),
                        "--stubbed", *STUBBED], capture_output=True, text=True)
    rep = [l for l in r.stdout.splitlines() if l.strip().startswith("REPORT")]
    vs_line = lines.index(next(l for l in lines if l.startswith("    pub fn validate_shape(&self)"))) + 1
    assert rep == [f"  REPORT v16.rs:{vs_line} MarketGroupV16View::validate_shape: a cfg inside the body that differs "
                   'between the builds: any(test, kani, feature = "audit-scan")'], rep
    # 4. the entry points read no slot of the view
    for pid, fn in ENTRY.items():
        impl = "impl MarketGroupV16HeaderAccount {" if fn != "advance_resolved_slot_not_atomic" else \
            "impl<'a, T> MarketGroupV16ViewMut<'a, T> {"
        assert ".markets" not in fn_text(lines, fn, impl), f"{fn} reads a slot of the view"
    reach = [l for l in r.stdout.splitlines() if l.startswith("unresolved calls:")]
    return reach[0] if reach else ""


if __name__ == "__main__":
    print(main(sys.argv[1]))
    print("header_local_check: every fact holds")
