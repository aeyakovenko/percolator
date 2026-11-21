module

public import Spec.Rust.Trade
public import SepLogic.Public

/-!
# The read-only constants of the compiled trade program

The compiled program takes the address of read-only constants: promoted constants (for example the
enum discriminants that `!=` comparisons read, or all-zero records compared with) and the string
literals of panic messages. Each entry gives a constant, its alignment and its initialised bytes,
exactly as the compiler emitted them (a constant containing padding is listed up to its first
padding byte).

`Constants S` says that every one of them holds those bytes, as it does in every run of the
program: a read-only, persistent fact about the program image, not about the market state.
-/

@[expose] public section

namespace Spec.Trade
open Iris Iris.BI Rust2Lean.Public

/-- The read-only constants of the compiled trade program: the constant, its alignment in bytes,
and its initialised contents. -/
def constantTable : List (RustGlobal × Nat × List UInt8) := [
  -- the string literal ""
  (Mir.Trade.Global.«""», 1,
    []),
  -- the string literal ""
  (Mir.Trade.Global.«""… [0 bytes] 07fc1007b4bd1495», 8,
    []),
  -- the string literal "K-diff overflow in wide"
  (Mir.Trade.Global.«"K-diff overflow in wide"», 1,
    [75,  45,  100,  105,  102,  102,  32,  111,  118,  101,  114,  102,  108,  111,  119,  32,  105,  110,  32,  119,  105,  100,  101]),
  -- the string literal "U256 division by zero"
  (Mir.Trade.Global.«"U256 division by zero"», 1,
    [85,  50,  53,  54,  32,  100,  105,  118,  105,  115,  105,  111,  110,  32,  98,  121,  32,  122,  101,  114,  111]),
  -- the string literal "U512 division by zero"
  (Mir.Trade.Global.«"U512 division by zero"», 1,
    [85,  53,  49,  50,  32,  100,  105,  118,  105,  115,  105,  111,  110,  32,  98,  121,  32,  122,  101,  114,  111]),
  -- the string literal "abs_u256 called on I256::MIN"
  (Mir.Trade.Global.«"abs_u256 called on I256::MIN"», 1,
    [97,  98,  115,  95,  117,  50,  53,  54,  32,  99,  97,  108,  108,  101,  100,  32,  111,  110,  32,  73,  50,  53,  54,  58,  58,  77,  73,  78]),
  -- the string literal "chunk size must be non-zero"
  (Mir.Trade.Global.«"chunk size must be non-zero"», 1,
    [99,  104,  117,  110,  107,  32,  115,  105,  122,  101,  32,  109,  117,  115,  116,  32,  98,  101,  32,  110,  111,  110,  45,  122,  101,  114,  111]),
  -- the string literal "floor_div_signed_conservative_i128: zero denominator"
  (Mir.Trade.Global.«"floor_div_signed_conservative_i1"… [52 bytes] f6ba71dbb1700623», 1,
    [102,  108,  111,  111,  114,  95,  100,  105,  118,  95,  115,  105,  103,  110,  101,  100,  95,  99,  111,  110,  115,  101,  114,  118,  97,  116,  105,  118,  101,  95,  105,  49,  50,  56,  58,  32,  122,  101,  114,  111,  32,  100,  101,  110,  111,  109,  105,  110,  97,  116,  111,  114]),
  -- the string literal "floor_div_signed_conservative_i128: result out of range"
  (Mir.Trade.Global.«"floor_div_signed_conservative_i1"… [55 bytes] 2550b6a279c23806», 1,
    [102,  108,  111,  111,  114,  95,  100,  105,  118,  95,  115,  105,  103,  110,  101,  100,  95,  99,  111,  110,  115,  101,  114,  118,  97,  116,  105,  118,  101,  95,  105,  49,  50,  56,  58,  32,  114,  101,  115,  117,  108,  116,  32,  111,  117,  116,  32,  111,  102,  32,  114,  97,  110,  103,  101]),
  -- the string literal "is_aligned_to: align is not a power-of-two"
  (Mir.Trade.Global.«"is_aligned_to: align is not a power-of-two"», 1,
    [105,  115,  95,  97,  108,  105,  103,  110,  101,  100,  95,  116,  111,  58,  32,  97,  108,  105,  103,  110,  32,  105,  115,  32,  110,  111,  116,  32,  97,  32,  112,  111,  119,  101,  114,  45,  111,  102,  45,  116,  119,  111]),
  -- the string literal "mag exceeds u128"
  (Mir.Trade.Global.«"mag exceeds u128"», 1,
    [109,  97,  103,  32,  101,  120,  99,  101,  101,  100,  115,  32,  117,  49,  50,  56]),
  -- the string literal "mag overflow"
  (Mir.Trade.Global.«"mag overflow"», 1,
    [109,  97,  103,  32,  111,  118,  101,  114,  102,  108,  111,  119]),
  -- the string literal "mid > len"
  (Mir.Trade.Global.«"mid > len"», 1,
    [109,  105,  100,  32,  62,  32,  108,  101,  110]),
  -- the string literal "mul_div quotient must fit U256"
  (Mir.Trade.Global.«"mul_div quotient must fit U256"», 1,
    [109,  117,  108,  95,  100,  105,  118,  32,  113,  117,  111,  116,  105,  101,  110,  116,  32,  109,  117,  115,  116,  32,  102,  105,  116,  32,  85,  50,  53,  54]),
  -- the string literal "mul_div_floor_u256: zero denominator"
  (Mir.Trade.Global.«"mul_div_floor_u256: zero denominator"», 1,
    [109,  117,  108,  95,  100,  105,  118,  95,  102,  108,  111,  111,  114,  95,  117,  50,  53,  54,  58,  32,  122,  101,  114,  111,  32,  100,  101,  110,  111,  109,  105,  110,  97,  116,  111,  114]),
  -- the string literal "mul_div_floor_u256_with_rem: zero denominator"
  (Mir.Trade.Global.«"mul_div_floor_u256_with_rem: zero denominator"», 1,
    [109,  117,  108,  95,  100,  105,  118,  95,  102,  108,  111,  111,  114,  95,  117,  50,  53,  54,  95,  119,  105,  116,  104,  95,  114,  101,  109,  58,  32,  122,  101,  114,  111,  32,  100,  101,  110,  111,  109,  105,  110,  97,  116,  111,  114]),
  -- the string literal "percolator::v16::V16PodU64"
  (Mir.Trade.Global.«"percolator::v16::V16PodU64"», 1,
    [112,  101,  114,  99,  111,  108,  97,  116,  111,  114,  58,  58,  118,  49,  54,  58,  58,  86,  49,  54,  80,  111,  100,  85,  54,  52]),
  -- the string literal "quotient exceeds u128"
  (Mir.Trade.Global.«"quotient exceeds u128"», 1,
    [113,  117,  111,  116,  105,  101,  110,  116,  32,  101,  120,  99,  101,  101,  100,  115,  32,  117,  49,  50,  56]),
  -- the string literal "remainder must fit U256"
  (Mir.Trade.Global.«"remainder must fit U256"», 1,
    [114,  101,  109,  97,  105,  110,  100,  101,  114,  32,  109,  117,  115,  116,  32,  102,  105,  116,  32,  85,  50,  53,  54]),
  -- the string literal "u64"
  (Mir.Trade.Global.«"u64"», 1,
    [117,  54,  52]),
  -- the string literal "unsafe precondition(s) violated: invalid value for `char`\n\nThis indicates a bug in the program. This Undefined Behavior check is optional, and cannot be relied on for safety."
  (Mir.Trade.Global.«"unsafe precondition(s) violated:"… [174 bytes] 84e0f5b831ed2508», 1,
    [117,  110,  115,  97,  102,  101,  32,  112,  114,  101,  99,  111,  110,  100,  105,  116,  105,  111,  110,  40,  115,  41,  32,  118,  105,  111,  108,  97,  116,  101,  100,  58,  32,  105,  110,  118,  97,  108,  105,  100,  32,  118,  97,  108,  117,  101,  32,  102,  111,  114,  32,  96,  99,  104,  97,  114,  96,  10,  10,  84,  104,  105,  115,  32,  105,  110,  100,  105,  99,  97,  116,  101,  115,  32,  97,  32,  98,  117,  103,  32,  105,  110,  32,  116,  104,  101,  32,  112,  114,  111,  103,  114,  97,  109,  46,  32,  84,  104,  105,  115,  32,  85,  110,  100,  101,  102,  105,  110,  101,  100,  32,  66,  101,  104,  97,  118,  105,  111,  114,  32,  99,  104,  101,  99,  107,  32,  105,  115,  32,  111,  112,  116,  105,  111,  110,  97,  108,  44,  32,  97,  110,  100,  32,  99,  97,  110,  110,  111,  116,  32,  98,  101,  32,  114,  101,  108,  105,  101,  100,  32,  111,  110,  32,  102,  111,  114,  32,  115,  97,  102,  101,  116,  121,  46]),
  -- the string literal "unsafe precondition(s) violated: u16::unchecked_add cannot overflow\n\nThis indicates a bug in the program. This Undefined Behavior check is optional, and cannot be relied on for safety."
  (Mir.Trade.Global.«"unsafe precondition(s) violated:"… [184 bytes] c2497dc1637ce5a1», 1,
    [117,  110,  115,  97,  102,  101,  32,  112,  114,  101,  99,  111,  110,  100,  105,  116,  105,  111,  110,  40,  115,  41,  32,  118,  105,  111,  108,  97,  116,  101,  100,  58,  32,  117,  49,  54,  58,  58,  117,  110,  99,  104,  101,  99,  107,  101,  100,  95,  97,  100,  100,  32,  99,  97,  110,  110,  111,  116,  32,  111,  118,  101,  114,  102,  108,  111,  119,  10,  10,  84,  104,  105,  115,  32,  105,  110,  100,  105,  99,  97,  116,  101,  115,  32,  97,  32,  98,  117,  103,  32,  105,  110,  32,  116,  104,  101,  32,  112,  114,  111,  103,  114,  97,  109,  46,  32,  84,  104,  105,  115,  32,  85,  110,  100,  101,  102,  105,  110,  101,  100,  32,  66,  101,  104,  97,  118,  105,  111,  114,  32,  99,  104,  101,  99,  107,  32,  105,  115,  32,  111,  112,  116,  105,  111,  110,  97,  108,  44,  32,  97,  110,  100,  32,  99,  97,  110,  110,  111,  116,  32,  98,  101,  32,  114,  101,  108,  105,  101,  100,  32,  111,  110,  32,  102,  111,  114,  32,  115,  97,  102,  101,  116,  121,  46]),
  -- the string literal "unsafe precondition(s) violated: usize::unchecked_sub cannot overflow\n\nThis indicates a bug in the program. This Undefined Behavior check is optional, and cannot be relied on for safety."
  (Mir.Trade.Global.«"unsafe precondition(s) violated:"… [186 bytes] 90be15e999f265f6», 1,
    [117,  110,  115,  97,  102,  101,  32,  112,  114,  101,  99,  111,  110,  100,  105,  116,  105,  111,  110,  40,  115,  41,  32,  118,  105,  111,  108,  97,  116,  101,  100,  58,  32,  117,  115,  105,  122,  101,  58,  58,  117,  110,  99,  104,  101,  99,  107,  101,  100,  95,  115,  117,  98,  32,  99,  97,  110,  110,  111,  116,  32,  111,  118,  101,  114,  102,  108,  111,  119,  10,  10,  84,  104,  105,  115,  32,  105,  110,  100,  105,  99,  97,  116,  101,  115,  32,  97,  32,  98,  117,  103,  32,  105,  110,  32,  116,  104,  101,  32,  112,  114,  111,  103,  114,  97,  109,  46,  32,  84,  104,  105,  115,  32,  85,  110,  100,  101,  102,  105,  110,  101,  100,  32,  66,  101,  104,  97,  118,  105,  111,  114,  32,  99,  104,  101,  99,  107,  32,  105,  115,  32,  111,  112,  116,  105,  111,  110,  97,  108,  44,  32,  97,  110,  100,  32,  99,  97,  110,  110,  111,  116,  32,  98,  101,  32,  114,  101,  108,  105,  101,  100,  32,  111,  110,  32,  102,  111,  114,  32,  115,  97,  102,  101,  116,  121,  46]),
  -- the string literal "unsafe precondition(s) violated: usize::unchecked_add cannot overflow\n\nThis indicates a bug in the program. This Undefined Behavior check is optional, and cannot be relied on for safety."
  (Mir.Trade.Global.«"unsafe precondition(s) violated:"… [186 bytes] 94fabd7af4c18cc5», 1,
    [117,  110,  115,  97,  102,  101,  32,  112,  114,  101,  99,  111,  110,  100,  105,  116,  105,  111,  110,  40,  115,  41,  32,  118,  105,  111,  108,  97,  116,  101,  100,  58,  32,  117,  115,  105,  122,  101,  58,  58,  117,  110,  99,  104,  101,  99,  107,  101,  100,  95,  97,  100,  100,  32,  99,  97,  110,  110,  111,  116,  32,  111,  118,  101,  114,  102,  108,  111,  119,  10,  10,  84,  104,  105,  115,  32,  105,  110,  100,  105,  99,  97,  116,  101,  115,  32,  97,  32,  98,  117,  103,  32,  105,  110,  32,  116,  104,  101,  32,  112,  114,  111,  103,  114,  97,  109,  46,  32,  84,  104,  105,  115,  32,  85,  110,  100,  101,  102,  105,  110,  101,  100,  32,  66,  101,  104,  97,  118,  105,  111,  114,  32,  99,  104,  101,  99,  107,  32,  105,  115,  32,  111,  112,  116,  105,  111,  110,  97,  108,  44,  32,  97,  110,  100,  32,  99,  97,  110,  110,  111,  116,  32,  98,  101,  32,  114,  101,  108,  105,  101,  100,  32,  111,  110,  32,  102,  111,  114,  32,  115,  97,  102,  101,  116,  121,  46]),
  -- the string literal "unsafe precondition(s) violated: hint::unreachable_unchecked must never be reached\n\nThis indicates a bug in the program. This Undefined Behavior check is optional, and cannot be relied on for safety."
  (Mir.Trade.Global.«"unsafe precondition(s) violated:"… [199 bytes] c71102f5161fcbc7», 1,
    [117,  110,  115,  97,  102,  101,  32,  112,  114,  101,  99,  111,  110,  100,  105,  116,  105,  111,  110,  40,  115,  41,  32,  118,  105,  111,  108,  97,  116,  101,  100,  58,  32,  104,  105,  110,  116,  58,  58,  117,  110,  114,  101,  97,  99,  104,  97,  98,  108,  101,  95,  117,  110,  99,  104,  101,  99,  107,  101,  100,  32,  109,  117,  115,  116,  32,  110,  101,  118,  101,  114,  32,  98,  101,  32,  114,  101,  97,  99,  104,  101,  100,  10,  10,  84,  104,  105,  115,  32,  105,  110,  100,  105,  99,  97,  116,  101,  115,  32,  97,  32,  98,  117,  103,  32,  105,  110,  32,  116,  104,  101,  32,  112,  114,  111,  103,  114,  97,  109,  46,  32,  84,  104,  105,  115,  32,  85,  110,  100,  101,  102,  105,  110,  101,  100,  32,  66,  101,  104,  97,  118,  105,  111,  114,  32,  99,  104,  101,  99,  107,  32,  105,  115,  32,  111,  112,  116,  105,  111,  110,  97,  108,  44,  32,  97,  110,  100,  32,  99,  97,  110,  110,  111,  116,  32,  98,  101,  32,  114,  101,  108,  105,  101,  100,  32,  111,  110,  32,  102,  111,  114,  32,  115,  97,  102,  101,  116,  121,  46]),
  -- the string literal "unsafe precondition(s) violated: ptr::offset_from_unsigned requires `self >= origin`\n\nThis indicates a bug in the program. This Undefined Behavior check is optional, and cannot be relied on for safety."
  (Mir.Trade.Global.«"unsafe precondition(s) violated:"… [201 bytes] 1a6e13d3fc68a56e», 1,
    [117,  110,  115,  97,  102,  101,  32,  112,  114,  101,  99,  111,  110,  100,  105,  116,  105,  111,  110,  40,  115,  41,  32,  118,  105,  111,  108,  97,  116,  101,  100,  58,  32,  112,  116,  114,  58,  58,  111,  102,  102,  115,  101,  116,  95,  102,  114,  111,  109,  95,  117,  110,  115,  105,  103,  110,  101,  100,  32,  114,  101,  113,  117,  105,  114,  101,  115,  32,  96,  115,  101,  108,  102,  32,  62,  61,  32,  111,  114,  105,  103,  105,  110,  96,  10,  10,  84,  104,  105,  115,  32,  105,  110,  100,  105,  99,  97,  116,  101,  115,  32,  97,  32,  98,  117,  103,  32,  105,  110,  32,  116,  104,  101,  32,  112,  114,  111,  103,  114,  97,  109,  46,  32,  84,  104,  105,  115,  32,  85,  110,  100,  101,  102,  105,  110,  101,  100,  32,  66,  101,  104,  97,  118,  105,  111,  114,  32,  99,  104,  101,  99,  107,  32,  105,  115,  32,  111,  112,  116,  105,  111,  110,  97,  108,  44,  32,  97,  110,  100,  32,  99,  97,  110,  110,  111,  116,  32,  98,  101,  32,  114,  101,  108,  105,  101,  100,  32,  111,  110,  32,  102,  111,  114,  32,  115,  97,  102,  101,  116,  121,  46]),
  -- the string literal "unsafe precondition(s) violated: slice::split_at_unchecked requires the index to be within the slice\n\nThis indicates a bug in the program. This Undefined Behavior check is optional, and cannot be relied on for safety."
  (Mir.Trade.Global.«"unsafe precondition(s) violated:"… [217 bytes] e6abb126256f3406», 1,
    [117,  110,  115,  97,  102,  101,  32,  112,  114,  101,  99,  111,  110,  100,  105,  116,  105,  111,  110,  40,  115,  41,  32,  118,  105,  111,  108,  97,  116,  101,  100,  58,  32,  115,  108,  105,  99,  101,  58,  58,  115,  112,  108,  105,  116,  95,  97,  116,  95,  117,  110,  99,  104,  101,  99,  107,  101,  100,  32,  114,  101,  113,  117,  105,  114,  101,  115,  32,  116,  104,  101,  32,  105,  110,  100,  101,  120,  32,  116,  111,  32,  98,  101,  32,  119,  105,  116,  104,  105,  110,  32,  116,  104,  101,  32,  115,  108,  105,  99,  101,  10,  10,  84,  104,  105,  115,  32,  105,  110,  100,  105,  99,  97,  116,  101,  115,  32,  97,  32,  98,  117,  103,  32,  105,  110,  32,  116,  104,  101,  32,  112,  114,  111,  103,  114,  97,  109,  46,  32,  84,  104,  105,  115,  32,  85,  110,  100,  101,  102,  105,  110,  101,  100,  32,  66,  101,  104,  97,  118,  105,  111,  114,  32,  99,  104,  101,  99,  107,  32,  105,  115,  32,  111,  112,  116,  105,  111,  110,  97,  108,  44,  32,  97,  110,  100,  32,  99,  97,  110,  110,  111,  116,  32,  98,  101,  32,  114,  101,  108,  105,  101,  100,  32,  111,  110,  32,  102,  111,  114,  32,  115,  97,  102,  101,  116,  121,  46]),
  -- the string literal "unsafe precondition(s) violated: slice::get_unchecked_mut requires that the range is within the slice\n\nThis indicates a bug in the program. This Undefined Behavior check is optional, and cannot be relied on for safety."
  (Mir.Trade.Global.«"unsafe precondition(s) violated:"… [218 bytes] 42560762e8444d61», 1,
    [117,  110,  115,  97,  102,  101,  32,  112,  114,  101,  99,  111,  110,  100,  105,  116,  105,  111,  110,  40,  115,  41,  32,  118,  105,  111,  108,  97,  116,  101,  100,  58,  32,  115,  108,  105,  99,  101,  58,  58,  103,  101,  116,  95,  117,  110,  99,  104,  101,  99,  107,  101,  100,  95,  109,  117,  116,  32,  114,  101,  113,  117,  105,  114,  101,  115,  32,  116,  104,  97,  116,  32,  116,  104,  101,  32,  114,  97,  110,  103,  101,  32,  105,  115,  32,  119,  105,  116,  104,  105,  110,  32,  116,  104,  101,  32,  115,  108,  105,  99,  101,  10,  10,  84,  104,  105,  115,  32,  105,  110,  100,  105,  99,  97,  116,  101,  115,  32,  97,  32,  98,  117,  103,  32,  105,  110,  32,  116,  104,  101,  32,  112,  114,  111,  103,  114,  97,  109,  46,  32,  84,  104,  105,  115,  32,  85,  110,  100,  101,  102,  105,  110,  101,  100,  32,  66,  101,  104,  97,  118,  105,  111,  114,  32,  99,  104,  101,  99,  107,  32,  105,  115,  32,  111,  112,  116,  105,  111,  110,  97,  108,  44,  32,  97,  110,  100,  32,  99,  97,  110,  110,  111,  116,  32,  98,  101,  32,  114,  101,  108,  105,  101,  100,  32,  111,  110,  32,  102,  111,  114,  32,  115,  97,  102,  101,  116,  121,  46]),
  -- the string literal "unsafe precondition(s) violated: slice::get_unchecked_mut requires that the index is within the slice\n\nThis indicates a bug in the program. This Undefined Behavior check is optional, and cannot be relied on for safety."
  (Mir.Trade.Global.«"unsafe precondition(s) violated:"… [218 bytes] 6121e222d240f4c6», 1,
    [117,  110,  115,  97,  102,  101,  32,  112,  114,  101,  99,  111,  110,  100,  105,  116,  105,  111,  110,  40,  115,  41,  32,  118,  105,  111,  108,  97,  116,  101,  100,  58,  32,  115,  108,  105,  99,  101,  58,  58,  103,  101,  116,  95,  117,  110,  99,  104,  101,  99,  107,  101,  100,  95,  109,  117,  116,  32,  114,  101,  113,  117,  105,  114,  101,  115,  32,  116,  104,  97,  116,  32,  116,  104,  101,  32,  105,  110,  100,  101,  120,  32,  105,  115,  32,  119,  105,  116,  104,  105,  110,  32,  116,  104,  101,  32,  115,  108,  105,  99,  101,  10,  10,  84,  104,  105,  115,  32,  105,  110,  100,  105,  99,  97,  116,  101,  115,  32,  97,  32,  98,  117,  103,  32,  105,  110,  32,  116,  104,  101,  32,  112,  114,  111,  103,  114,  97,  109,  46,  32,  84,  104,  105,  115,  32,  85,  110,  100,  101,  102,  105,  110,  101,  100,  32,  66,  101,  104,  97,  118,  105,  111,  114,  32,  99,  104,  101,  99,  107,  32,  105,  115,  32,  111,  112,  116,  105,  111,  110,  97,  108,  44,  32,  97,  110,  100,  32,  99,  97,  110,  110,  111,  116,  32,  98,  101,  32,  114,  101,  108,  105,  101,  100,  32,  111,  110,  32,  102,  111,  114,  32,  115,  97,  102,  101,  116,  121,  46]),
  -- the string literal "unsafe precondition(s) violated: str::get_unchecked requires that the range is within the string slice\n\nThis indicates a bug in the program. This Undefined Behavior check is optional, and cannot be relied on for safety."
  (Mir.Trade.Global.«"unsafe precondition(s) violated:"… [219 bytes] 43b0ab6dbebaa278», 1,
    [117,  110,  115,  97,  102,  101,  32,  112,  114,  101,  99,  111,  110,  100,  105,  116,  105,  111,  110,  40,  115,  41,  32,  118,  105,  111,  108,  97,  116,  101,  100,  58,  32,  115,  116,  114,  58,  58,  103,  101,  116,  95,  117,  110,  99,  104,  101,  99,  107,  101,  100,  32,  114,  101,  113,  117,  105,  114,  101,  115,  32,  116,  104,  97,  116,  32,  116,  104,  101,  32,  114,  97,  110,  103,  101,  32,  105,  115,  32,  119,  105,  116,  104,  105,  110,  32,  116,  104,  101,  32,  115,  116,  114,  105,  110,  103,  32,  115,  108,  105,  99,  101,  10,  10,  84,  104,  105,  115,  32,  105,  110,  100,  105,  99,  97,  116,  101,  115,  32,  97,  32,  98,  117,  103,  32,  105,  110,  32,  116,  104,  101,  32,  112,  114,  111,  103,  114,  97,  109,  46,  32,  84,  104,  105,  115,  32,  85,  110,  100,  101,  102,  105,  110,  101,  100,  32,  66,  101,  104,  97,  118,  105,  111,  114,  32,  99,  104,  101,  99,  107,  32,  105,  115,  32,  111,  112,  116,  105,  111,  110,  97,  108,  44,  32,  97,  110,  100,  32,  99,  97,  110,  110,  111,  116,  32,  98,  101,  32,  114,  101,  108,  105,  101,  100,  32,  111,  110,  32,  102,  111,  114,  32,  115,  97,  102,  101,  116,  121,  46]),
  -- the string literal "unsafe precondition(s) violated: hint::assert_unchecked must never be called when the condition is false\n\nThis indicates a bug in the program. This Undefined Behavior check is optional, and cannot be relied on for safety."
  (Mir.Trade.Global.«"unsafe precondition(s) violated:"… [221 bytes] f74a79be368c082d», 1,
    [117,  110,  115,  97,  102,  101,  32,  112,  114,  101,  99,  111,  110,  100,  105,  116,  105,  111,  110,  40,  115,  41,  32,  118,  105,  111,  108,  97,  116,  101,  100,  58,  32,  104,  105,  110,  116,  58,  58,  97,  115,  115,  101,  114,  116,  95,  117,  110,  99,  104,  101,  99,  107,  101,  100,  32,  109,  117,  115,  116,  32,  110,  101,  118,  101,  114,  32,  98,  101,  32,  99,  97,  108,  108,  101,  100,  32,  119,  104,  101,  110,  32,  116,  104,  101,  32,  99,  111,  110,  100,  105,  116,  105,  111,  110,  32,  105,  115,  32,  102,  97,  108,  115,  101,  10,  10,  84,  104,  105,  115,  32,  105,  110,  100,  105,  99,  97,  116,  101,  115,  32,  97,  32,  98,  117,  103,  32,  105,  110,  32,  116,  104,  101,  32,  112,  114,  111,  103,  114,  97,  109,  46,  32,  84,  104,  105,  115,  32,  85,  110,  100,  101,  102,  105,  110,  101,  100,  32,  66,  101,  104,  97,  118,  105,  111,  114,  32,  99,  104,  101,  99,  107,  32,  105,  115,  32,  111,  112,  116,  105,  111,  110,  97,  108,  44,  32,  97,  110,  100,  32,  99,  97,  110,  110,  111,  116,  32,  98,  101,  32,  114,  101,  108,  105,  101,  100,  32,  111,  110,  32,  102,  111,  114,  32,  115,  97,  102,  101,  116,  121,  46]),
  -- the string literal "unsafe precondition(s) violated: ptr::write_bytes requires that the destination pointer is aligned and non-null\n\nThis indicates a bug in the program. This Undefined Behavior check is optional, and cannot be relied on for safety."
  (Mir.Trade.Global.«"unsafe precondition(s) violated:"… [228 bytes] 52577965963a249b», 1,
    [117,  110,  115,  97,  102,  101,  32,  112,  114,  101,  99,  111,  110,  100,  105,  116,  105,  111,  110,  40,  115,  41,  32,  118,  105,  111,  108,  97,  116,  101,  100,  58,  32,  112,  116,  114,  58,  58,  119,  114,  105,  116,  101,  95,  98,  121,  116,  101,  115,  32,  114,  101,  113,  117,  105,  114,  101,  115,  32,  116,  104,  97,  116,  32,  116,  104,  101,  32,  100,  101,  115,  116,  105,  110,  97,  116,  105,  111,  110,  32,  112,  111,  105,  110,  116,  101,  114,  32,  105,  115,  32,  97,  108,  105,  103,  110,  101,  100,  32,  97,  110,  100,  32,  110,  111,  110,  45,  110,  117,  108,  108,  10,  10,  84,  104,  105,  115,  32,  105,  110,  100,  105,  99,  97,  116,  101,  115,  32,  97,  32,  98,  117,  103,  32,  105,  110,  32,  116,  104,  101,  32,  112,  114,  111,  103,  114,  97,  109,  46,  32,  84,  104,  105,  115,  32,  85,  110,  100,  101,  102,  105,  110,  101,  100,  32,  66,  101,  104,  97,  118,  105,  111,  114,  32,  99,  104,  101,  99,  107,  32,  105,  115,  32,  111,  112,  116,  105,  111,  110,  97,  108,  44,  32,  97,  110,  100,  32,  99,  97,  110,  110,  111,  116,  32,  98,  101,  32,  114,  101,  108,  105,  101,  100,  32,  111,  110,  32,  102,  111,  114,  32,  115,  97,  102,  101,  116,  121,  46]),
  -- the string literal "unsafe precondition(s) violated: slice::as_chunks_unchecked requires `N != 0` and the slice to split exactly into `N`-element chunks\n\nThis indicates a bug in the program. This Undefined Behavior check is optional, and cannot be relied on for safety."
  (Mir.Trade.Global.«"unsafe precondition(s) violated:"… [249 bytes] eada6b20504014cb», 1,
    [117,  110,  115,  97,  102,  101,  32,  112,  114,  101,  99,  111,  110,  100,  105,  116,  105,  111,  110,  40,  115,  41,  32,  118,  105,  111,  108,  97,  116,  101,  100,  58,  32,  115,  108,  105,  99,  101,  58,  58,  97,  115,  95,  99,  104,  117,  110,  107,  115,  95,  117,  110,  99,  104,  101,  99,  107,  101,  100,  32,  114,  101,  113,  117,  105,  114,  101,  115,  32,  96,  78,  32,  33,  61,  32,  48,  96,  32,  97,  110,  100,  32,  116,  104,  101,  32,  115,  108,  105,  99,  101,  32,  116,  111,  32,  115,  112,  108,  105,  116,  32,  101,  120,  97,  99,  116,  108,  121,  32,  105,  110,  116,  111,  32,  96,  78,  96,  45,  101,  108,  101,  109,  101,  110,  116,  32,  99,  104,  117,  110,  107,  115,  10,  10,  84,  104,  105,  115,  32,  105,  110,  100,  105,  99,  97,  116,  101,  115,  32,  97,  32,  98,  117,  103,  32,  105,  110,  32,  116,  104,  101,  32,  112,  114,  111,  103,  114,  97,  109,  46,  32,  84,  104,  105,  115,  32,  85,  110,  100,  101,  102,  105,  110,  101,  100,  32,  66,  101,  104,  97,  118,  105,  111,  114,  32,  99,  104,  101,  99,  107,  32,  105,  115,  32,  111,  112,  116,  105,  111,  110,  97,  108,  44,  32,  97,  110,  100,  32,  99,  97,  110,  110,  111,  116,  32,  98,  101,  32,  114,  101,  108,  105,  101,  100,  32,  111,  110,  32,  102,  111,  114,  32,  115,  97,  102,  101,  116,  121,  46]),
  -- the string literal "unsafe precondition(s) violated: slice::from_raw_parts requires the pointer to be aligned and non-null, and the total size of the slice not to exceed `isize::MAX`\n\nThis indicates a bug in the program. This Undefined Behavior check is optional, and cannot be relied on for safety."
  (Mir.Trade.Global.«"unsafe precondition(s) violated:"… [279 bytes] 40fc27bf13432a6f», 1,
    [117,  110,  115,  97,  102,  101,  32,  112,  114,  101,  99,  111,  110,  100,  105,  116,  105,  111,  110,  40,  115,  41,  32,  118,  105,  111,  108,  97,  116,  101,  100,  58,  32,  115,  108,  105,  99,  101,  58,  58,  102,  114,  111,  109,  95,  114,  97,  119,  95,  112,  97,  114,  116,  115,  32,  114,  101,  113,  117,  105,  114,  101,  115,  32,  116,  104,  101,  32,  112,  111,  105,  110,  116,  101,  114,  32,  116,  111,  32,  98,  101,  32,  97,  108,  105,  103,  110,  101,  100,  32,  97,  110,  100,  32,  110,  111,  110,  45,  110,  117,  108,  108,  44,  32,  97,  110,  100,  32,  116,  104,  101,  32,  116,  111,  116,  97,  108,  32,  115,  105,  122,  101,  32,  111,  102,  32,  116,  104,  101,  32,  115,  108,  105,  99,  101,  32,  110,  111,  116,  32,  116,  111,  32,  101,  120,  99,  101,  101,  100,  32,  96,  105,  115,  105,  122,  101,  58,  58,  77,  65,  88,  96,  10,  10,  84,  104,  105,  115,  32,  105,  110,  100,  105,  99,  97,  116,  101,  115,  32,  97,  32,  98,  117,  103,  32,  105,  110,  32,  116,  104,  101,  32,  112,  114,  111,  103,  114,  97,  109,  46,  32,  84,  104,  105,  115,  32,  85,  110,  100,  101,  102,  105,  110,  101,  100,  32,  66,  101,  104,  97,  118,  105,  111,  114,  32,  99,  104,  101,  99,  107,  32,  105,  115,  32,  111,  112,  116,  105,  111,  110,  97,  108,  44,  32,  97,  110,  100,  32,  99,  97,  110,  110,  111,  116,  32,  98,  101,  32,  114,  101,  108,  105,  101,  100,  32,  111,  110,  32,  102,  111,  114,  32,  115,  97,  102,  101,  116,  121,  46]),
  -- the string literal "wide product overflow"
  (Mir.Trade.Global.«"wide product overflow"», 1,
    [119,  105,  100,  101,  32,  112,  114,  111,  100,  117,  99,  116,  32,  111,  118,  101,  114,  102,  108,  111,  119]),
  -- the string literal "wide_mul_div_floor_u128: division by zero"
  (Mir.Trade.Global.«"wide_mul_div_floor_u128: division by zero"», 1,
    [119,  105,  100,  101,  95,  109,  117,  108,  95,  100,  105,  118,  95,  102,  108,  111,  111,  114,  95,  117,  49,  50,  56,  58,  32,  100,  105,  118,  105,  115,  105,  111,  110,  32,  98,  121,  32,  122,  101,  114,  111]),
  -- the string literal "wide_mul_div_floor_u128: result exceeds u128"
  (Mir.Trade.Global.«"wide_mul_div_floor_u128: result exceeds u128"», 1,
    [119,  105,  100,  101,  95,  109,  117,  108,  95,  100,  105,  118,  95,  102,  108,  111,  111,  114,  95,  117,  49,  50,  56,  58,  32,  114,  101,  115,  117,  108,  116,  32,  101,  120,  99,  101,  101,  100,  115,  32,  117,  49,  50,  56]),
  -- the string literal "wide_signed_mul_div_floor_from_k_pair: q > i128::MAX"
  (Mir.Trade.Global.«"wide_signed_mul_div_floor_from_k"… [52 bytes] 224fe64f38e15a8a», 1,
    [119,  105,  100,  101,  95,  115,  105,  103,  110,  101,  100,  95,  109,  117,  108,  95,  100,  105,  118,  95,  102,  108,  111,  111,  114,  95,  102,  114,  111,  109,  95,  107,  95,  112,  97,  105,  114,  58,  32,  113,  32,  62,  32,  105,  49,  50,  56,  58,  58,  77,  65,  88]),
  -- the string literal "wide_signed_mul_div_floor_from_k_pair: mag > i128::MAX"
  (Mir.Trade.Global.«"wide_signed_mul_div_floor_from_k"… [54 bytes] dee417b6e8e69fe6», 1,
    [119,  105,  100,  101,  95,  115,  105,  103,  110,  101,  100,  95,  109,  117,  108,  95,  100,  105,  118,  95,  102,  108,  111,  111,  114,  95,  102,  114,  111,  109,  95,  107,  95,  112,  97,  105,  114,  58,  32,  109,  97,  103,  32,  62,  32,  105,  49,  50,  56,  58,  58,  77,  65,  88]),
  -- the string literal "wide_signed_mul_div_floor_from_k_pair: den == 0"
  (Mir.Trade.Global.«"wide_signed_mul_div_floor_from_k_pair: den == 0"», 1,
    [119,  105,  100,  101,  95,  115,  105,  103,  110,  101,  100,  95,  109,  117,  108,  95,  100,  105,  118,  95,  102,  108,  111,  111,  114,  95,  102,  114,  111,  109,  95,  107,  95,  112,  97,  105,  114,  58,  32,  100,  101,  110,  32,  61,  61,  32,  48]),
  -- a promoted constant of 1 bytes
  (Mir.Trade.Global.«const [1 bytes] 165f0cf0fda1e660», 1,
    [2]),
  -- a promoted constant of 1 bytes
  (Mir.Trade.Global.«const [1 bytes] 165f0df0fda1e813», 1,
    [3]),
  -- a promoted constant of 1 bytes
  (Mir.Trade.Global.«const [1 bytes] 165f0ef0fda1e9c6», 1,
    [0]),
  -- a promoted constant of 1 bytes
  (Mir.Trade.Global.«const [1 bytes] 165f0ff0fda1eb79», 1,
    [1]),
  -- a promoted constant of 1 bytes
  (Mir.Trade.Global.«const [1 bytes] 165fc9f0fda32787», 1,
    [255]),
  -- a promoted constant of 112 bytes, listed up to its first padding byte (the first 8 bytes)
  (Mir.Trade.Global.«const [112 bytes] 06dd2827b79ba28f», 16,
    [0,  0,  0,  0,  0,  0,  0,  0]),
  -- a promoted constant of 16 bytes
  (Mir.Trade.Global.«const [16 bytes] 3bce376d54ced372», 8,
    [0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0]),
  -- a promoted constant of 176 bytes, listed up to its first padding byte (the first 1 bytes)
  (Mir.Trade.Global.«const [176 bytes] 423fc2ce8d0eaefa», 16,
    [0]),
  -- a promoted constant of 192 bytes, listed up to its first padding byte (the first 184 bytes)
  (Mir.Trade.Global.«const [192 bytes] f4890081d2a921e3», 16,
    [0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  16,  165,  212,  232,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0]),
  -- a promoted constant of 2 bytes
  (Mir.Trade.Global.«const [2 bytes] 5040d7b80e3e5a14», 1,
    [192,  0]),
  -- a promoted constant of 208 bytes, listed up to its first padding byte (the first 3 bytes)
  (Mir.Trade.Global.«const [208 bytes] 6ee9d4e941c65c34», 16,
    [0,  0,  0]),
  -- a promoted constant of 32 bytes
  (Mir.Trade.Global.«const [32 bytes] 40b245474d61ecb8», 16,
    [1,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0]),
  -- a promoted constant of 32 bytes
  (Mir.Trade.Global.«const [32 bytes] fa48e7fb4f2bbad8», 8,
    [0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  128]),
  -- a promoted constant of 48 bytes, listed up to its first padding byte (the first 33 bytes)
  (Mir.Trade.Global.«const [48 bytes] d48936f86c256488», 16,
    [0,  64,  122,  16,  243,  90,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  128,  198,  164,  126,  141,  3,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0]),
  -- a promoted constant of 62 bytes
  (Mir.Trade.Global.«const [62 bytes] 8fdc3c107835cc04», 1,
    [33,  109,  101,  109,  58,  58,  99,  111,  110,  106,  117,  114,  101,  95,  122,  115,  116,  32,  105,  110,  118,  111,  107,  101,  100,  32,  111,  110,  32,  116,  121,  112,  101,  32,  192,  25,  44,  32,  119,  104,  105,  99,  104,  32,  105,  115,  32,  110,  111,  116,  32,  122,  101,  114,  111,  45,  115,  105,  122,  101,  100,  0]),
  -- a promoted constant of 80 bytes, listed up to its first padding byte (the first 72 bytes)
  (Mir.Trade.Global.«const [80 bytes] 656fa2fcf735f5ae», 16,
    [0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0]),
  -- a promoted constant of 96 bytes, listed up to its first padding byte (the first 90 bytes)
  (Mir.Trade.Global.«const [96 bytes] dc8084d86321afd9», 16,
    [0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0])
]

/-- Every read-only constant in `constantTable` holds its listed bytes at its alignment. -/
def Constants {PROP : Type} [BI PROP] (S : Vocab PROP) : PROP :=
  constantTable.foldr (fun c P => iprop(S.global c.1 c.2.1 c.2.2 ∗ P)) iprop(emp)

end Spec.Trade
