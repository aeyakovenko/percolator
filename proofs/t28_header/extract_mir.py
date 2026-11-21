"""Translate the pinned rustc textual MIR to a conservative Lean CFG."""

import argparse
import hashlib
from pathlib import Path
import re


SOURCE_SHA = "d1893ebbba1cf911f9b0737e5f3a73b84d6818e77f3e3c16289918c75022cab0"
DEPOSIT_SHA = "c25ffbd2523021f97b24911cf5fb36fe73c64c7af30c88fb62bb6aeb0040b472"

HEADER_SHA = "31ff350cc53f7f793c5e408d27be4f59b401a68efc11170a06fd2969b85aa81e"

FRAME_CALLS = (
    "MarketGroupV16ViewMut::<'_, T>::as_view",
    "PortfolioV16ViewMut::<'_>::validate_with_market::<T>",
    "<Result<(), V16Error> as Try>::branch",
    "<Result<u128, V16Error> as Try>::branch",
    "<Result<TokenValueFlowProofV16, V16Error> as Try>::branch",
)
POST_BRANCH_CALLS = (
    "V16PodU128::get",
    "V16PodU128::new",
    "core::num::<impl u128>::checked_add",
    "Option::<u128>::ok_or::<V16Error>",
    "TokenValueFlowProofV16::external_in_to_account_capital",
    "TokenValueFlowProofV16::validate",
)
RESIDUAL = "<Result<(), V16Error> as FromResidual<Result<Infallible, V16Error>>>::from_residual"
VALIDATOR = "MarketGroupV16ViewMut::<'_, T>::validate_shape"
LOCAL = r"_[1-9]\d*"
LOCAL_RHS = (
    rf"&(?:\(\*{LOCAL}\)|{LOCAL})",
    rf"discriminant\({LOCAL}\)",
    rf"Eq\(copy {LOCAL}, const 0_u128\)",
    rf"copy \(\(\*{LOCAL}\)\.\d+: (?:&mut )?v16::\w+\)",
    rf"copy \(\({LOCAL} as (?:Continue|Break)\)\.0: [\w:, <>]+\)",
    rf"copy {LOCAL}",
    "V16Error::ArithmeticOverflow",
)
STATE_WRITE = (
    rf"\(\(\*{LOCAL}\)\.(?:2|3|5): v16::V16PodU128\) = move {LOCAL};",
    rf"\(\(\(\*{LOCAL}\)\.18: v16::HealthCertV16Account\)\.10: u8\) = const 0_u8;",
)


def require(ok, info):
    if not ok:
        raise ValueError(info)


def function(mir, name, argument):
    heads = list(re.finditer(rf"^fn .*::{name}\(_1: {argument}[^\n]*\n", mir, re.M))
    require(len(heads) == 1, f"expected exactly one {name} definition")
    start = heads[0].start()
    return mir[start:mir.index("\n}", start) + 2] + "\n"


def blocks(mir):
    start = mir.index("\n    bb0: {")
    body = mir[start:mir.index("\n}", start)]
    groups = re.findall(r"\n    bb(\d+): \{\n(.*?)\n    \}", body, re.S)
    normalized = "\n\n".join(f"    bb{pc}: {{\n{code}\n    }}" for pc, code in groups)
    require(normalized == body.strip("\n"), "unrecognized block layout")
    require(list(map(int, (pc for pc, _ in groups))) == list(range(len(groups))),
            "block indices are not contiguous")
    return [[line.strip() for line in code.splitlines()] for _, code in groups]


def effects_and_exit(lines):
    effects = []
    for line in lines[:-1]:
        local = re.fullmatch(rf"{LOCAL} = (.+);", line)
        if local and any(re.fullmatch(pattern, local[1]) for pattern in LOCAL_RHS):
            continue
        if line == "_0 = Result::<(), V16Error>::Ok(const ());":
            effects.append("setOk")
        elif any(re.fullmatch(pattern, line) for pattern in STATE_WRITE):
            effects.append("write")
        else:
            raise ValueError(f"unclassified statement: {line}")

    ending = lines[-1]
    if ending == "return;":
        return effects, "ret", []
    if ending == "unreachable;":
        return effects, "dead", []
    goto = re.fullmatch(r"goto -> bb(\d+);", ending)
    if goto:
        return effects, "jump", [int(goto[1])]
    switch = re.fullmatch(rf"switchInt\(move {LOCAL}\) -> \[((?:\d+: bb\d+, )*otherwise: bb\d+)\];", ending)
    if switch:
        return effects, "jump", [int(pc) for pc in re.findall(r"bb(\d+)", switch[1])]
    call = re.fullmatch(
        rf"(_0|{LOCAL}) = (.+)\(((?:(?:copy|move) {LOCAL})(?:, (?:copy|move) {LOCAL})*)\)"
        r" -> \[return: bb(\d+), unwind continue\];", ending,
    )
    if call:
        destination, name, _, target = call.groups()
        if destination != "_0" and name in FRAME_CALLS:
            effects.append("frame")
        elif destination != "_0" and name in POST_BRANCH_CALLS:
            effects.append("write")
        elif destination == "_0" and name in (RESIDUAL, VALIDATOR):
            effects.append("setErr" if name == RESIDUAL else "checkHeader")
        else:
            raise ValueError(f"unclassified call: {ending}")
        return effects, "jump", [int(target)]
    raise ValueError(f"unclassified terminator: {ending}")


def extract(deposit):
    parsed = [effects_and_exit(code) for code in blocks(deposit)]
    require(all(0 <= pc < len(parsed) for _, _, targets in parsed for pc in targets),
            "successor outside graph")
    return parsed


def normal_edges(body):
    # Both dispositions leave the normal-return graph. Preserve every other byte.
    return body.replace(", unwind unreachable];", ", unwind continue];")


def verify_wrapper(mir):
    wrapper = function(mir, "validate_shape", r"&MarketGroupV16ViewMut<")
    require(blocks(normal_edges(wrapper)) == [
        ["_3 = MarketGroupV16ViewMut::<'_, T>::as_view(copy _1) -> [return: bb1, unwind continue];"],
        ["_2 = &_3;",
         "_0 = MarketGroupV16View::<'_, T>::validate_shape(move _2) -> [return: bb2, unwind continue];"],
        ["return;"],
    ], "unexpected validator wrapper")
    return wrapper


def lean_source(parsed):
    lines = [
        "import MirContracts", "", "namespace Percolator.T28.Mir",
        "", "def depositGraph : Graph := fun pc =>",
        "  let blocks : Array Block := #[",
    ]
    for pc, (effects, kind, targets) in enumerate(parsed):
        ops = ", ".join("." + op for op in effects)
        exit_text = f".jump {targets}" if kind == "jump" else "." + kind
        comma = "," if pc + 1 < len(parsed) else ""
        lines.append(f"    ⟨[{ops}], {exit_text}⟩{comma} -- bb{pc}")
    lines.extend(["  ]", "  blocks[pc]?", "", "end Percolator.T28.Mir", ""])
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mir", type=Path, required=True)
    parser.add_argument("--rust-source", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    require(hashlib.sha256(args.rust_source.read_bytes()).hexdigest() == SOURCE_SHA,
            "Rust source SHA-256 mismatch")
    full_mir = args.mir.read_text()
    deposit = function(full_mir, "deposit_not_atomic", r"&mut MarketGroupV16ViewMut<")
    print(f"deposit MIR SHA-256={hashlib.sha256(deposit.encode()).hexdigest()}")
    deposit = normal_edges(deposit)
    require(hashlib.sha256(deposit.encode()).hexdigest() == DEPOSIT_SHA, "deposit MIR SHA-256 mismatch")
    header = function(full_mir, "validate_shape", r"&MarketGroupV16View<")
    require(hashlib.sha256(normal_edges(header).encode()).hexdigest() == HEADER_SHA,
            "default-feature header validator MIR SHA-256 mismatch")
    wrapper = verify_wrapper(full_mir)
    parsed = extract(deposit)
    args.output.write_text(lean_source(parsed))
    writes = sum(any(re.fullmatch(pattern, line) for pattern in STATE_WRITE)
                 for code in blocks(deposit) for line in code)
    print(f"deposit blocks={len(parsed)} explicit stores={writes}")
    print(f"validator wrapper SHA-256={hashlib.sha256(wrapper.encode()).hexdigest()}")


if __name__ == "__main__":
    main()
