// ============================================================================
// BPF-Safe 128-bit Types
// ============================================================================
//
// CRITICAL: Rust 1.77/1.78 changed i128/u128 alignment from 8 to 16 bytes on x86_64,
// but BPF/SBF still uses 8-byte alignment. This causes struct layout mismatches
// when reading/writing 128-bit values on-chain.
//
// These wrapper types use [u64; 2] internally to ensure consistent 8-byte alignment
// across all platforms. See: https://blog.rust-lang.org/2024/03/30/i128-layout-update.html
//
// KANI OPTIMIZATION: For Kani builds, we use transparent newtypes around raw
// primitives. This dramatically reduces SAT solver complexity since Kani doesn't
// have to reason about bit-shifting and array indexing for every 128-bit operation.

// ============================================================================
// I128 - Kani-optimized version (transparent newtype)
// ============================================================================
#[cfg(kani)]
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct I128(i128);

#[cfg(kani)]
impl I128 {
    pub const ZERO: Self = Self(0);
    pub const MIN: Self = Self(i128::MIN);
    pub const MAX: Self = Self(i128::MAX);

    #[inline(always)]
    pub const fn new(val: i128) -> Self {
        Self(val)
    }

    #[inline(always)]
    pub const fn get(self) -> i128 {
        self.0
    }

    #[inline(always)]
    pub fn set(&mut self, val: i128) {
        self.0 = val;
    }

    #[inline(always)]
    pub fn checked_add(self, rhs: i128) -> Option<Self> {
        self.0.checked_add(rhs).map(Self)
    }

    #[inline(always)]
    pub fn checked_sub(self, rhs: i128) -> Option<Self> {
        self.0.checked_sub(rhs).map(Self)
    }

    #[inline(always)]
    pub fn checked_mul(self, rhs: i128) -> Option<Self> {
        self.0.checked_mul(rhs).map(Self)
    }

    #[inline(always)]
    pub fn checked_div(self, rhs: i128) -> Option<Self> {
        self.0.checked_div(rhs).map(Self)
    }

    #[inline(always)]
    pub fn saturating_add(self, rhs: i128) -> Self {
        Self(self.0.saturating_add(rhs))
    }

    #[inline(always)]
    pub fn saturating_add_i128(self, rhs: I128) -> Self {
        Self(self.0.saturating_add(rhs.0))
    }

    #[inline(always)]
    pub fn saturating_sub(self, rhs: i128) -> Self {
        Self(self.0.saturating_sub(rhs))
    }

    #[inline(always)]
    pub fn saturating_sub_i128(self, rhs: I128) -> Self {
        Self(self.0.saturating_sub(rhs.0))
    }

    #[inline(always)]
    pub fn wrapping_add(self, rhs: i128) -> Self {
        Self(self.0.wrapping_add(rhs))
    }

    #[inline(always)]
    pub fn abs(self) -> Self {
        Self(self.0.abs())
    }

    #[inline(always)]
    pub fn unsigned_abs(self) -> u128 {
        self.0.unsigned_abs()
    }

    #[inline(always)]
    pub fn is_zero(self) -> bool {
        self.0 == 0
    }

    #[inline(always)]
    pub fn is_negative(self) -> bool {
        self.0 < 0
    }

    #[inline(always)]
    pub fn is_positive(self) -> bool {
        self.0 > 0
    }
}

// ============================================================================
// I128 - BPF version (array-based for alignment)
// ============================================================================
/// BPF-safe signed 128-bit integer using [u64; 2] for consistent alignment.
/// Layout: [lo, hi] in little-endian order.
// Kani I128 trait implementations
#[cfg(kani)]
impl Default for I128 {
    fn default() -> Self {
        Self::ZERO
    }
}

#[cfg(kani)]
impl core::fmt::Debug for I128 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "I128({})", self.0)
    }
}

#[cfg(kani)]
impl core::fmt::Display for I128 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(kani)]
impl From<i128> for I128 {
    fn from(val: i128) -> Self {
        Self(val)
    }
}

#[cfg(kani)]
impl From<i64> for I128 {
    fn from(val: i64) -> Self {
        Self(val as i128)
    }
}

#[cfg(kani)]
impl From<I128> for i128 {
    fn from(val: I128) -> Self {
        val.0
    }
}

#[cfg(kani)]
impl PartialOrd for I128 {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(kani)]
impl Ord for I128 {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.0.cmp(&other.0)
    }
}

#[cfg(kani)]
impl core::ops::Add<i128> for I128 {
    type Output = Self;
    fn add(self, rhs: i128) -> Self {
        Self(self.0.saturating_add(rhs))
    }
}

#[cfg(kani)]
impl core::ops::Add<I128> for I128 {
    type Output = Self;
    fn add(self, rhs: I128) -> Self {
        Self(self.0.saturating_add(rhs.0))
    }
}

#[cfg(kani)]
impl core::ops::Sub<i128> for I128 {
    type Output = Self;
    fn sub(self, rhs: i128) -> Self {
        Self(self.0.saturating_sub(rhs))
    }
}

#[cfg(kani)]
impl core::ops::Sub<I128> for I128 {
    type Output = Self;
    fn sub(self, rhs: I128) -> Self {
        Self(self.0.saturating_sub(rhs.0))
    }
}

#[cfg(kani)]
impl core::ops::Neg for I128 {
    type Output = Self;
    fn neg(self) -> Self {
        Self(self.0.saturating_neg())
    }
}

#[cfg(kani)]
impl core::ops::AddAssign<i128> for I128 {
    fn add_assign(&mut self, rhs: i128) {
        *self = *self + rhs;
    }
}

#[cfg(kani)]
impl core::ops::SubAssign<i128> for I128 {
    fn sub_assign(&mut self, rhs: i128) {
        *self = *self - rhs;
    }
}

// ============================================================================
// I128 - BPF version (array-based for alignment)
// ============================================================================
#[cfg(not(kani))]
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct I128([u64; 2]);

#[cfg(not(kani))]
impl I128 {
    pub const ZERO: Self = Self([0, 0]);
    pub const MIN: Self = Self([0, 0x8000_0000_0000_0000]); // i128::MIN
    pub const MAX: Self = Self([u64::MAX, 0x7FFF_FFFF_FFFF_FFFF]); // i128::MAX

    #[inline]
    pub const fn new(val: i128) -> Self {
        Self([val as u64, (val >> 64) as u64])
    }

    #[inline]
    pub const fn get(self) -> i128 {
        // Sign-extend: treat hi as signed
        ((self.0[1] as i128) << 64) | (self.0[0] as u128 as i128)
    }

    #[inline]
    pub fn set(&mut self, val: i128) {
        self.0[0] = val as u64;
        self.0[1] = (val >> 64) as u64;
    }

    #[inline]
    pub fn checked_add(self, rhs: i128) -> Option<Self> {
        self.get().checked_add(rhs).map(Self::new)
    }

    #[inline]
    pub fn checked_sub(self, rhs: i128) -> Option<Self> {
        self.get().checked_sub(rhs).map(Self::new)
    }

    #[inline]
    pub fn checked_mul(self, rhs: i128) -> Option<Self> {
        self.get().checked_mul(rhs).map(Self::new)
    }

    #[inline]
    pub fn checked_div(self, rhs: i128) -> Option<Self> {
        self.get().checked_div(rhs).map(Self::new)
    }

    #[inline]
    pub fn saturating_add(self, rhs: i128) -> Self {
        Self::new(self.get().saturating_add(rhs))
    }

    #[inline]
    pub fn saturating_add_i128(self, rhs: I128) -> Self {
        Self::new(self.get().saturating_add(rhs.get()))
    }

    #[inline]
    pub fn saturating_sub(self, rhs: i128) -> Self {
        Self::new(self.get().saturating_sub(rhs))
    }

    #[inline]
    pub fn saturating_sub_i128(self, rhs: I128) -> Self {
        Self::new(self.get().saturating_sub(rhs.get()))
    }

    #[inline]
    pub fn wrapping_add(self, rhs: i128) -> Self {
        Self::new(self.get().wrapping_add(rhs))
    }

    #[inline]
    pub fn abs(self) -> Self {
        Self::new(self.get().abs())
    }

    #[inline]
    pub fn unsigned_abs(self) -> u128 {
        self.get().unsigned_abs()
    }

    #[inline]
    pub fn is_zero(self) -> bool {
        self.0[0] == 0 && self.0[1] == 0
    }

    #[inline]
    pub fn is_negative(self) -> bool {
        (self.0[1] as i64) < 0
    }

    #[inline]
    pub fn is_positive(self) -> bool {
        !self.is_zero() && !self.is_negative()
    }
}

#[cfg(not(kani))]
impl Default for I128 {
    fn default() -> Self {
        Self::ZERO
    }
}

#[cfg(not(kani))]
impl core::fmt::Debug for I128 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "I128({})", self.get())
    }
}

#[cfg(not(kani))]
impl core::fmt::Display for I128 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.get())
    }
}

#[cfg(not(kani))]
impl From<i128> for I128 {
    fn from(val: i128) -> Self {
        Self::new(val)
    }
}

#[cfg(not(kani))]
impl From<i64> for I128 {
    fn from(val: i64) -> Self {
        Self::new(val as i128)
    }
}

#[cfg(not(kani))]
impl From<I128> for i128 {
    fn from(val: I128) -> Self {
        val.get()
    }
}

#[cfg(not(kani))]
impl PartialOrd for I128 {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(not(kani))]
impl Ord for I128 {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.get().cmp(&other.get())
    }
}

// ============================================================================
// U128 - Kani-optimized version (transparent newtype)
// ============================================================================
#[cfg(kani)]
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct U128(u128);

#[cfg(kani)]
impl U128 {
    pub const ZERO: Self = Self(0);
    pub const MAX: Self = Self(u128::MAX);

    #[inline(always)]
    pub const fn new(val: u128) -> Self {
        Self(val)
    }

    #[inline(always)]
    pub const fn get(self) -> u128 {
        self.0
    }

    #[inline(always)]
    pub fn set(&mut self, val: u128) {
        self.0 = val;
    }

    #[inline(always)]
    pub fn checked_add(self, rhs: u128) -> Option<Self> {
        self.0.checked_add(rhs).map(Self)
    }

    #[inline(always)]
    pub fn checked_sub(self, rhs: u128) -> Option<Self> {
        self.0.checked_sub(rhs).map(Self)
    }

    #[inline(always)]
    pub fn checked_mul(self, rhs: u128) -> Option<Self> {
        self.0.checked_mul(rhs).map(Self)
    }

    #[inline(always)]
    pub fn checked_div(self, rhs: u128) -> Option<Self> {
        self.0.checked_div(rhs).map(Self)
    }

    #[inline(always)]
    pub fn saturating_add(self, rhs: u128) -> Self {
        Self(self.0.saturating_add(rhs))
    }

    #[inline(always)]
    pub fn saturating_add_u128(self, rhs: U128) -> Self {
        Self(self.0.saturating_add(rhs.0))
    }

    #[inline(always)]
    pub fn saturating_sub(self, rhs: u128) -> Self {
        Self(self.0.saturating_sub(rhs))
    }

    #[inline(always)]
    pub fn saturating_sub_u128(self, rhs: U128) -> Self {
        Self(self.0.saturating_sub(rhs.0))
    }

    #[inline(always)]
    pub fn saturating_mul(self, rhs: u128) -> Self {
        Self(self.0.saturating_mul(rhs))
    }

    #[inline(always)]
    pub fn wrapping_add(self, rhs: u128) -> Self {
        Self(self.0.wrapping_add(rhs))
    }

    #[inline(always)]
    pub fn max(self, rhs: Self) -> Self {
        if self.0 >= rhs.0 {
            self
        } else {
            rhs
        }
    }

    #[inline(always)]
    pub fn min(self, rhs: Self) -> Self {
        if self.0 <= rhs.0 {
            self
        } else {
            rhs
        }
    }

    #[inline(always)]
    pub fn is_zero(self) -> bool {
        self.0 == 0
    }
}

#[cfg(kani)]
impl Default for U128 {
    fn default() -> Self {
        Self::ZERO
    }
}

#[cfg(kani)]
impl core::fmt::Debug for U128 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "U128({})", self.0)
    }
}

#[cfg(kani)]
impl core::fmt::Display for U128 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(kani)]
impl From<u128> for U128 {
    fn from(val: u128) -> Self {
        Self(val)
    }
}

#[cfg(kani)]
impl From<u64> for U128 {
    fn from(val: u64) -> Self {
        Self(val as u128)
    }
}

#[cfg(kani)]
impl From<U128> for u128 {
    fn from(val: U128) -> Self {
        val.0
    }
}

#[cfg(kani)]
impl PartialOrd for U128 {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(kani)]
impl Ord for U128 {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.0.cmp(&other.0)
    }
}

#[cfg(kani)]
impl core::ops::Add<u128> for U128 {
    type Output = Self;
    fn add(self, rhs: u128) -> Self {
        Self(self.0.saturating_add(rhs))
    }
}

#[cfg(kani)]
impl core::ops::Add<U128> for U128 {
    type Output = Self;
    fn add(self, rhs: U128) -> Self {
        Self(self.0.saturating_add(rhs.0))
    }
}

#[cfg(kani)]
impl core::ops::Sub<u128> for U128 {
    type Output = Self;
    fn sub(self, rhs: u128) -> Self {
        Self(self.0.saturating_sub(rhs))
    }
}

#[cfg(kani)]
impl core::ops::Sub<U128> for U128 {
    type Output = Self;
    fn sub(self, rhs: U128) -> Self {
        Self(self.0.saturating_sub(rhs.0))
    }
}

#[cfg(kani)]
impl core::ops::Mul<u128> for U128 {
    type Output = Self;
    fn mul(self, rhs: u128) -> Self {
        Self(self.0.saturating_mul(rhs))
    }
}

#[cfg(kani)]
impl core::ops::Mul<U128> for U128 {
    type Output = Self;
    fn mul(self, rhs: U128) -> Self {
        Self(self.0.saturating_mul(rhs.0))
    }
}

#[cfg(kani)]
impl core::ops::Div<u128> for U128 {
    type Output = Self;
    fn div(self, rhs: u128) -> Self {
        Self(self.0 / rhs)
    }
}

#[cfg(kani)]
impl core::ops::Div<U128> for U128 {
    type Output = Self;
    fn div(self, rhs: U128) -> Self {
        Self(self.0 / rhs.0)
    }
}

#[cfg(kani)]
impl core::ops::AddAssign<u128> for U128 {
    fn add_assign(&mut self, rhs: u128) {
        *self = *self + rhs;
    }
}

#[cfg(kani)]
impl core::ops::SubAssign<u128> for U128 {
    fn sub_assign(&mut self, rhs: u128) {
        *self = *self - rhs;
    }
}

// ============================================================================
// U128 - BPF version (array-based for alignment)
// ============================================================================
/// BPF-safe unsigned 128-bit integer using [u64; 2] for consistent alignment.
/// Layout: [lo, hi] in little-endian order.
#[cfg(not(kani))]
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct U128([u64; 2]);

#[cfg(not(kani))]
impl U128 {
    pub const ZERO: Self = Self([0, 0]);
    pub const MAX: Self = Self([u64::MAX, u64::MAX]);

    #[inline]
    pub const fn new(val: u128) -> Self {
        Self([val as u64, (val >> 64) as u64])
    }

    #[inline]
    pub const fn get(self) -> u128 {
        ((self.0[1] as u128) << 64) | (self.0[0] as u128)
    }

    #[inline]
    pub fn set(&mut self, val: u128) {
        self.0[0] = val as u64;
        self.0[1] = (val >> 64) as u64;
    }

    #[inline]
    pub fn checked_add(self, rhs: u128) -> Option<Self> {
        self.get().checked_add(rhs).map(Self::new)
    }

    #[inline]
    pub fn checked_sub(self, rhs: u128) -> Option<Self> {
        self.get().checked_sub(rhs).map(Self::new)
    }

    #[inline]
    pub fn checked_mul(self, rhs: u128) -> Option<Self> {
        self.get().checked_mul(rhs).map(Self::new)
    }

    #[inline]
    pub fn checked_div(self, rhs: u128) -> Option<Self> {
        self.get().checked_div(rhs).map(Self::new)
    }

    #[inline]
    pub fn saturating_add(self, rhs: u128) -> Self {
        Self::new(self.get().saturating_add(rhs))
    }

    #[inline]
    pub fn saturating_add_u128(self, rhs: U128) -> Self {
        Self::new(self.get().saturating_add(rhs.get()))
    }

    #[inline]
    pub fn saturating_sub(self, rhs: u128) -> Self {
        Self::new(self.get().saturating_sub(rhs))
    }

    #[inline]
    pub fn saturating_sub_u128(self, rhs: U128) -> Self {
        Self::new(self.get().saturating_sub(rhs.get()))
    }

    #[inline]
    pub fn saturating_mul(self, rhs: u128) -> Self {
        Self::new(self.get().saturating_mul(rhs))
    }

    #[inline]
    pub fn wrapping_add(self, rhs: u128) -> Self {
        Self::new(self.get().wrapping_add(rhs))
    }

    #[inline]
    pub fn max(self, rhs: Self) -> Self {
        if self.get() >= rhs.get() {
            self
        } else {
            rhs
        }
    }

    #[inline]
    pub fn min(self, rhs: Self) -> Self {
        if self.get() <= rhs.get() {
            self
        } else {
            rhs
        }
    }

    #[inline]
    pub fn is_zero(self) -> bool {
        self.0[0] == 0 && self.0[1] == 0
    }
}

#[cfg(not(kani))]
impl Default for U128 {
    fn default() -> Self {
        Self::ZERO
    }
}

#[cfg(not(kani))]
impl core::fmt::Debug for U128 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "U128({})", self.get())
    }
}

#[cfg(not(kani))]
impl core::fmt::Display for U128 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.get())
    }
}

#[cfg(not(kani))]
impl From<u128> for U128 {
    fn from(val: u128) -> Self {
        Self::new(val)
    }
}

#[cfg(not(kani))]
impl From<u64> for U128 {
    fn from(val: u64) -> Self {
        Self::new(val as u128)
    }
}

#[cfg(not(kani))]
impl From<U128> for u128 {
    fn from(val: U128) -> Self {
        val.get()
    }
}

#[cfg(not(kani))]
impl PartialOrd for U128 {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(not(kani))]
impl Ord for U128 {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.get().cmp(&other.get())
    }
}

// Arithmetic operators for U128 (BPF version)
#[cfg(not(kani))]
impl core::ops::Add<u128> for U128 {
    type Output = Self;
    fn add(self, rhs: u128) -> Self {
        Self::new(self.get().saturating_add(rhs))
    }
}

#[cfg(not(kani))]
impl core::ops::Add<U128> for U128 {
    type Output = Self;
    fn add(self, rhs: U128) -> Self {
        Self::new(self.get().saturating_add(rhs.get()))
    }
}

#[cfg(not(kani))]
impl core::ops::Sub<u128> for U128 {
    type Output = Self;
    fn sub(self, rhs: u128) -> Self {
        Self::new(self.get().saturating_sub(rhs))
    }
}

#[cfg(not(kani))]
impl core::ops::Sub<U128> for U128 {
    type Output = Self;
    fn sub(self, rhs: U128) -> Self {
        Self::new(self.get().saturating_sub(rhs.get()))
    }
}

#[cfg(not(kani))]
impl core::ops::Mul<u128> for U128 {
    type Output = Self;
    fn mul(self, rhs: u128) -> Self {
        Self::new(self.get().saturating_mul(rhs))
    }
}

#[cfg(not(kani))]
impl core::ops::Mul<U128> for U128 {
    type Output = Self;
    fn mul(self, rhs: U128) -> Self {
        Self::new(self.get().saturating_mul(rhs.get()))
    }
}

#[cfg(not(kani))]
impl core::ops::Div<u128> for U128 {
    type Output = Self;
    fn div(self, rhs: u128) -> Self {
        Self::new(self.get() / rhs)
    }
}

#[cfg(not(kani))]
impl core::ops::Div<U128> for U128 {
    type Output = Self;
    fn div(self, rhs: U128) -> Self {
        Self::new(self.get() / rhs.get())
    }
}

#[cfg(not(kani))]
impl core::ops::AddAssign<u128> for U128 {
    fn add_assign(&mut self, rhs: u128) {
        *self = *self + rhs;
    }
}

#[cfg(not(kani))]
impl core::ops::SubAssign<u128> for U128 {
    fn sub_assign(&mut self, rhs: u128) {
        *self = *self - rhs;
    }
}

// Arithmetic operators for I128 (BPF version)
#[cfg(not(kani))]
impl core::ops::Add<i128> for I128 {
    type Output = Self;
    fn add(self, rhs: i128) -> Self {
        Self::new(self.get().saturating_add(rhs))
    }
}

#[cfg(not(kani))]
impl core::ops::Add<I128> for I128 {
    type Output = Self;
    fn add(self, rhs: I128) -> Self {
        Self::new(self.get().saturating_add(rhs.get()))
    }
}

#[cfg(not(kani))]
impl core::ops::Sub<i128> for I128 {
    type Output = Self;
    fn sub(self, rhs: i128) -> Self {
        Self::new(self.get().saturating_sub(rhs))
    }
}

#[cfg(not(kani))]
impl core::ops::Sub<I128> for I128 {
    type Output = Self;
    fn sub(self, rhs: I128) -> Self {
        Self::new(self.get().saturating_sub(rhs.get()))
    }
}

#[cfg(not(kani))]
impl core::ops::Mul<i128> for I128 {
    type Output = Self;
    fn mul(self, rhs: i128) -> Self {
        Self::new(self.get().saturating_mul(rhs))
    }
}

#[cfg(not(kani))]
impl core::ops::Neg for I128 {
    type Output = Self;
    fn neg(self) -> Self {
        Self::new(-self.get())
    }
}

#[cfg(not(kani))]
impl core::ops::AddAssign<i128> for I128 {
    fn add_assign(&mut self, rhs: i128) {
        *self = *self + rhs;
    }
}

#[cfg(not(kani))]
impl core::ops::SubAssign<i128> for I128 {
    fn sub_assign(&mut self, rhs: i128) {
        *self = *self - rhs;
    }
}
