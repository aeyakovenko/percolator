#!/usr/bin/env bash
# Verify both compiler probes. Set PLATFORM_TOOLS to cross-compile for Solana.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
layouts=('()')
case "${1:-}" in
    '') ;;
    --all-layouts)
        layouts+=(u64 u8)
        for size in {0..32} 64 128 256 512 1024; do layouts+=("[u8; $size]"); done ;;
    *) echo "usage: $0 [--all-layouts]" >&2; exit 2 ;;
esac
work=$(mktemp -d)
echo "T28.header LLVM work directory: $work"
ln -s "$root" "$work/engine"
mkdir -p "$work/probe/src"
cp "$here/Cargo.toml.in" "$work/probe/Cargo.toml"
cp "$here/Cargo.lock" "$work/probe/"
cp "$here/probe.rs" "$work/probe/src/lib.rs"
hostlib=$(rustc +1.95.0 --print sysroot)/lib
llvm=${LLVM_SHARED_LIB:-"$hostlib/libLLVM.so.22.1-rust-1.95.0-stable"}
g++ -std=c++17 -O2 -Wall -Wextra -Werror -I"${LLVM_C_INCLUDE:-/usr/lib/llvm-18/include}" \
    "$here/check_returns.cpp" "$llvm" -Wl,-rpath,"$(dirname "$llvm")" -o "$work/check_returns"
cargo_cmd=(cargo +1.95.0)
target=x86_64-unknown-linux-gnu
triple=$target
if [[ -n ${PLATFORM_TOOLS:-} ]]; then
    platform=$(cd "$PLATFORM_TOOLS" && pwd)
    cargo_cmd=("$platform/rust/bin/cargo")
    export RUSTC="$platform/rust/bin/rustc"
    export CARGO_TARGET_SBPF_SOLANA_SOLANA_LINKER="$platform/llvm/bin/ld.lld"
    target=sbpf-solana-solana
    triple=sbf
    "$RUSTC" -vV
else
    rustc +1.95.0 -vV
fi
cd "$work/probe"
export CARGO_TARGET_DIR="$work/target"
build() {
    "${cargo_cmd[@]}" rustc --locked --release --lib --jobs 1 --target "$target" "$@" -- --emit=llvm-ir
}
check_ir() {
    local dump="$CARGO_TARGET_DIR/$target/release/deps/t28_header_probe.ll"
    grep -Fxq "target triple = \"$triple\"" "$dump" || return 2
    "$work/check_returns" "$dump" "$work/reduced.ll"
}
for layout in "${layouts[@]}"; do
    printf 'type Wrapper = %s;\n' "$layout" > src/wrapper.rs
    build
    echo "$target $layout"
    check_ir
done
echo 'type Wrapper = ();' > src/wrapper.rs
build --features corrupt-header
# Exit 1 means the checker could not establish that every return is false.
# Tool, module and build failures must not count as a successful control.
status=0
check_ir > "$work/control.log" || status=$?
[[ $status == 1 ]]
diff -u <(printf '%s\n' positive_counterexample=unresolved zero_counterexample=unresolved) "$work/control.log"
echo "Checked ${#layouts[@]} layouts and rejected both corrupt-header probes."
