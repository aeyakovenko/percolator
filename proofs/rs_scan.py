#!/usr/bin/env python3
"""rs_scan.py: a small lexer-based scanner for Rust source, shared by changed_fns.py and
header_writes.py. `scan(src)` returns (kind, name, header_stack, start_line, text) for every
fn/struct/enum item; header_stack holds the text (attributes included) of the enclosing
impl/trait/mod blocks. `index` keys them by (innermost header, name)."""
import re, collections

def scan(src):
    """Return a list of (kind, name, header_stack, start_line, text) for fn/struct/enum items."""
    n = len(src)
    i = 0
    depth = 0
    items = []
    stack = []          # (depth_at_open, header text) for impl/trait/mod blocks
    open_items = []     # (depth_at_open, kind, name, start_index, start_line, header_stack)
    line = 1
    pending = None      # (kind, name, start_index, start_line) waiting for its '{' or ';'
    sig_depth = 0       # nesting of ( and [ since the pending item started: `[u8; 32]` in a signature
                        # holds a `;` that does not end the item
    last_item_end = 0
    def attr_start(idx):
        """Walk back over the attribute/doc-comment lines directly above the line holding idx."""
        ls = src.rfind("\n", 0, idx) + 1
        while ls > 0:
            pe = ls - 1
            ps = src.rfind("\n", 0, pe) + 1
            t = src[ps:pe].strip()
            if t.startswith("#[") or t.startswith("///") or t.startswith("//!") or t.endswith(")]") or t.endswith(",") and src[ps:pe].startswith("    #") :
                ls = ps
            elif t.startswith("#!["):
                break
            else:
                # multi-line attribute bodies: accept a line that ends an attribute opened above
                if t.endswith("]") and not t.startswith("}"):
                    # look upward for the opening `#[` within 12 lines
                    q, ok = ps, False
                    for _ in range(12):
                        if q == 0:
                            break
                        qe = q - 1
                        qs = src.rfind("\n", 0, qe) + 1
                        tt = src[qs:qe].strip()
                        if tt.startswith("#["):
                            ok = True; q = qs; break
                        if tt == "" or tt.endswith("}") or tt.endswith(";"):
                            break
                        q = qs
                    if ok:
                        ls = q
                        continue
                break
        return ls
    item_re = re.compile(r"(?:pub(?:\([a-z]+\))?\s+)?(?:(?:const|unsafe|async)\s+)*(fn|struct|enum|impl|trait|mod)\b\s*(?:<[^>{]*>\s*)?([A-Za-z_][A-Za-z0-9_]*)?")
    while i < n:
        c = src[i]
        if c == "\n":
            line += 1; i += 1; continue
        if src.startswith("//", i):
            j = src.find("\n", i)
            i = n if j < 0 else j
            continue
        if src.startswith("/*", i):
            d = 1; i += 2
            while i < n and d:
                if src.startswith("/*", i): d += 1; i += 2
                elif src.startswith("*/", i): d -= 1; i += 2
                else:
                    if src[i] == "\n": line += 1
                    i += 1
            continue
        if c == '"' or (c == "r" and re.match(r'r#*"', src[i:i + 8]) and not (i and (src[i - 1].isalnum() or src[i - 1] == "_"))) \
                or (c == "b" and src[i + 1:i + 2] == '"'):
            if c == "b": i += 1; c = '"'
            if c == "r":
                m = re.match(r'r(#*)"', src[i:i + 8])
                end = '"' + m.group(1)
                j = src.find(end, i + len(m.group(0)))
                line += src.count("\n", i, j); i = j + len(end)
            else:
                i += 1
                while i < n and src[i] != '"':
                    if src[i] == "\\": i += 1
                    if i < n and src[i] == "\n": line += 1
                    i += 1
                i += 1
            continue
        if c == "'":
            m = re.match(r"'(\\.[^']*|[^\\'])'", src[i:i + 12])
            if m:
                i += len(m.group(0)); continue
            i += 1; continue
        if pending and c in "([":
            sig_depth += 1; i += 1; continue
        if pending and c in ")]":
            sig_depth -= 1; i += 1; continue
        if c == "{":
            if pending:
                kind, name, s_idx, s_line = pending
                if kind in ("impl", "trait", "mod"):
                    hdr = " ".join(src[s_idx:i].split())
                    stack.append((depth, hdr))
                else:
                    open_items.append((depth, kind, name, s_idx, s_line, tuple(h for _, h in stack)))
                pending = None
            depth += 1; i += 1; continue
        if c == "}":
            depth -= 1; i += 1
            if open_items and open_items[-1][0] == depth:
                _, kind, name, s_idx, s_line, hdrs = open_items.pop()
                items.append((kind, name, hdrs, s_line, src[s_idx:i]))
            elif stack and stack[-1][0] == depth:
                stack.pop()
            continue
        if c == ";":
            if pending and sig_depth > 0:
                i += 1; continue
            if pending:
                kind, name, s_idx, s_line = pending
                if kind in ("fn", "struct"):
                    items.append((kind, name, tuple(h for _, h in stack), s_line, src[s_idx:i + 1]))
                pending = None
            i += 1; continue
        if (c.isalpha() or c == "_") and not (i and (src[i - 1].isalnum() or src[i - 1] == "_")):
            m = re.match(r"[A-Za-z_][A-Za-z0-9_]*", src[i:i + 80])
            w = m.group(0)
            if w in ("fn", "struct", "enum", "impl", "trait", "mod") and pending is None:
                # only at item position: previous non-space token is not `.`/`::`/`->`/`:` etc.
                k = i - 1
                while k >= 0 and src[k] in " \t": k -= 1
                prev_ok = k < 0 or src[k] in "\n};{)" or re.search(r"\b(pub|const|unsafe|async|crate\)|super\)|self\))\s*$", src[max(0, k - 12):k + 1]) is not None
                if prev_ok:
                    rest = src[i + len(w):i + len(w) + 200]
                    if w in ("impl", "trait", "mod"):
                        nm = None
                    else:
                        mm = re.match(r"\s*([A-Za-z_][A-Za-z0-9_]*)", rest)
                        nm = mm.group(1) if mm else None
                    if w in ("impl", "trait", "mod") or nm:
                        pending = (w, nm, attr_start(i), line)
                        sig_depth = 0
            i += len(w); continue
        i += 1
    return items

def index(items, kinds):
    out = collections.OrderedDict()
    seen = collections.Counter()
    for kind, name, hdrs, line, text in items:
        if kind not in kinds:
            continue
        key = (hdrs[-1] if hdrs else "", name)
        seen[key] += 1
        if seen[key] > 1:
            key = (key[0], f"{name}#{seen[key]}")
        out[key] = (line, text)
    return out

def norm(t):
    return "\n".join(l.rstrip() for l in t.strip().split("\n"))
