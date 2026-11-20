//! Census shim (scratch build only): the complete wide-math surface `src/v16.rs` is allowed to use.
//! If `src/v16.rs` compiles against this module instead of `wide_math`, it uses nothing else.
use crate::wide_math as w;
use core::cmp::Ordering;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct U256(w::U256);

impl U256 {
    pub const ONE: U256 = U256(w::U256::ONE);
    pub const fn from_u128(v: u128) -> Self { U256(w::U256::from_u128(v)) }
    pub const fn from_u64(v: u64) -> Self { U256(w::U256::from_u64(v)) }
    pub const fn is_zero(&self) -> bool { self.0.is_zero() }
    pub fn try_into_u128(&self) -> Option<u128> { self.0.try_into_u128() }
    pub fn checked_add(self, r: U256) -> Option<U256> { self.0.checked_add(r.0).map(U256) }
    pub fn checked_sub(self, r: U256) -> Option<U256> { self.0.checked_sub(r.0).map(U256) }
    pub fn checked_mul(self, r: U256) -> Option<U256> { self.0.checked_mul(r.0).map(U256) }
    pub fn checked_div(self, r: U256) -> Option<U256> { self.0.checked_div(r.0).map(U256) }
    pub fn checked_rem(self, r: U256) -> Option<U256> { self.0.checked_rem(r.0).map(U256) }
}
impl PartialOrd for U256 {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> { Some(self.cmp(o)) }
}
impl Ord for U256 {
    fn cmp(&self, o: &Self) -> Ordering { self.0.cmp(&o.0) }
}
pub fn checked_mul_div_ceil_u256(a: U256, b: U256, d: U256) -> Option<U256> {
    w::checked_mul_div_ceil_u256(a.0, b.0, d.0).map(U256)
}
pub fn mul_div_floor_u256_with_rem(a: U256, b: U256, d: U256) -> (U256, U256) {
    let (q, r) = w::mul_div_floor_u256_with_rem(a.0, b.0, d.0);
    (U256(q), U256(r))
}
pub use w::{floor_div_signed_conservative_i128, wide_mul_div_floor_u128, wide_signed_mul_div_floor_from_k_pair};
