// ============================================================================
// Wide 256-bit Arithmetic for Risk Engine
// ============================================================================
//
// Provides U256 and I256 types plus spec section 4.6 helpers (floor division,
// ceiling division, mul-div with 512-bit intermediate) for the percolator
// risk engine.
//
// DUAL-MODE LAYOUT (mirrors src/i128.rs):
//   - Kani builds: `#[repr(transparent)]` wrapper around `[u128; 2]` so the
//     SAT solver works on native 128-bit words instead of 64-bit limb arrays.
//   - BPF / host builds: `#[repr(C)] [u64; 4]` for consistent 8-byte
//     alignment across all Solana targets.
//
// No external crates. No unsafe code. Pure `core::` only.

use core::cmp::Ordering;

// ============================================================================
// U256 -- Kani version
// ============================================================================
#[cfg(kani)]
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct U256([u128; 2]); // [lo, hi]

#[cfg(kani)]
impl U256 {
    pub const ZERO: Self = Self([0, 0]);
    pub const ONE: Self = Self([1, 0]);
    pub const MAX: Self = Self([u128::MAX, u128::MAX]);

    #[inline(always)]
    pub const fn new(lo: u128, hi: u128) -> Self {
        Self([lo, hi])
    }

    #[inline(always)]
    pub const fn from_u128(v: u128) -> Self {
        Self([v, 0])
    }

    #[inline(always)]
    pub const fn from_u64(v: u64) -> Self {
        Self([v as u128, 0])
    }

    #[inline(always)]
    pub const fn lo(&self) -> u128 {
        self.0[0]
    }

    #[inline(always)]
    pub const fn hi(&self) -> u128 {
        self.0[1]
    }

    #[inline(always)]
    pub const fn is_zero(&self) -> bool {
        self.0[0] == 0 && self.0[1] == 0
    }

    #[inline(always)]
    pub fn try_into_u128(&self) -> Option<u128> {
        if self.0[1] == 0 {
            Some(self.0[0])
        } else {
            None
        }
    }

    // -- checked arithmetic --

    pub fn checked_add(self, rhs: U256) -> Option<U256> {
        let (lo, carry) = self.0[0].overflowing_add(rhs.0[0]);
        let hi = self.0[1].checked_add(rhs.0[1])?;
        let hi = if carry { hi.checked_add(1)? } else { hi };
        Some(U256([lo, hi]))
    }

    pub fn checked_sub(self, rhs: U256) -> Option<U256> {
        let (lo, borrow) = self.0[0].overflowing_sub(rhs.0[0]);
        let hi = self.0[1].checked_sub(rhs.0[1])?;
        let hi = if borrow { hi.checked_sub(1)? } else { hi };
        Some(U256([lo, hi]))
    }

    pub fn checked_mul(self, rhs: U256) -> Option<U256> {
        // Schoolbook multiply: split each u128 into two u64 halves, giving
        // four u64 limbs per operand, then accumulate with carries.
        //
        // However since we only need the low 256 bits and an overflow flag,
        // we can use a simpler approach: treat each U256 as (lo: u128, hi: u128).
        //
        //   result_lo_full = self.lo * rhs.lo          (up to 256 bits)
        //   cross1         = self.lo * rhs.hi          (up to 256 bits)
        //   cross2         = self.hi * rhs.lo          (up to 256 bits)
        //   high_high      = self.hi * rhs.hi          (must be zero or overflow)
        //
        // We need widening 128x128 -> 256 for self.lo * rhs.lo.

        // If both hi words are nonzero, definitely overflows.
        if self.0[1] != 0 && rhs.0[1] != 0 {
            return None;
        }

        let (prod_lo, prod_hi) = widening_mul_u128(self.0[0], rhs.0[0]);

        // cross1 = self.lo * rhs.hi (only the low 128 bits matter for result hi)
        // cross2 = self.hi * rhs.lo
        // If the cross product itself exceeds 128 bits, we overflow.
        let cross1 = if rhs.0[1] != 0 {
            let (c, overflow) = widening_mul_u128(self.0[0], rhs.0[1]);
            if overflow != 0 {
                return None;
            }
            c
        } else {
            0u128
        };

        let cross2 = if self.0[1] != 0 {
            let (c, overflow) = widening_mul_u128(self.0[1], rhs.0[0]);
            if overflow != 0 {
                return None;
            }
            c
        } else {
            0u128
        };

        let hi = prod_hi.checked_add(cross1)?;
        let hi = hi.checked_add(cross2)?;

        Some(U256([prod_lo, hi]))
    }

    pub fn checked_div(self, rhs: U256) -> Option<U256> {
        if rhs.is_zero() {
            return None;
        }
        Some(div_rem_u256(self, rhs).0)
    }

    pub fn checked_rem(self, rhs: U256) -> Option<U256> {
        if rhs.is_zero() {
            return None;
        }
        Some(div_rem_u256(self, rhs).1)
    }

    // -- overflowing --

    pub fn overflowing_add(self, rhs: U256) -> (U256, bool) {
        let (lo, carry) = self.0[0].overflowing_add(rhs.0[0]);
        let (hi, overflow1) = self.0[1].overflowing_add(rhs.0[1]);
        let (hi, overflow2) = if carry {
            hi.overflowing_add(1)
        } else {
            (hi, false)
        };
        (U256([lo, hi]), overflow1 || overflow2)
    }

    pub fn overflowing_sub(self, rhs: U256) -> (U256, bool) {
        let (lo, borrow) = self.0[0].overflowing_sub(rhs.0[0]);
        let (hi, underflow1) = self.0[1].overflowing_sub(rhs.0[1]);
        let (hi, underflow2) = if borrow {
            hi.overflowing_sub(1)
        } else {
            (hi, false)
        };
        (U256([lo, hi]), underflow1 || underflow2)
    }

    // -- saturating --

    pub fn saturating_add(self, rhs: U256) -> U256 {
        self.checked_add(rhs).unwrap_or(U256::MAX)
    }

    pub fn saturating_sub(self, rhs: U256) -> U256 {
        self.checked_sub(rhs).unwrap_or(U256::ZERO)
    }

    // -- shifts --

    pub fn shl(self, bits: u32) -> U256 {
        if bits >= 256 {
            return U256::ZERO;
        }
        if bits == 0 {
            return self;
        }
        if bits >= 128 {
            let s = bits - 128;
            U256([0, self.0[0] << s])
        } else {
            let lo = self.0[0] << bits;
            let hi = (self.0[1] << bits) | (self.0[0] >> (128 - bits));
            U256([lo, hi])
        }
    }

    pub fn shr(self, bits: u32) -> U256 {
        if bits >= 256 {
            return U256::ZERO;
        }
        if bits == 0 {
            return self;
        }
        if bits >= 128 {
            let s = bits - 128;
            U256([self.0[1] >> s, 0])
        } else {
            let hi = self.0[1] >> bits;
            let lo = (self.0[0] >> bits) | (self.0[1] << (128 - bits));
            U256([lo, hi])
        }
    }

    // -- bitwise --

    pub fn bitand(self, rhs: U256) -> U256 {
        U256([self.0[0] & rhs.0[0], self.0[1] & rhs.0[1]])
    }

    pub fn bitor(self, rhs: U256) -> U256 {
        U256([self.0[0] | rhs.0[0], self.0[1] | rhs.0[1]])
    }
}

// ============================================================================
// U256 -- BPF version
// ============================================================================
#[cfg(not(kani))]
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct U256([u64; 4]); // [limb0 (least significant), limb1, limb2, limb3]

#[cfg(not(kani))]
impl U256 {
    pub const ZERO: Self = Self([0, 0, 0, 0]);
    pub const ONE: Self = Self([1, 0, 0, 0]);
    pub const MAX: Self = Self([u64::MAX, u64::MAX, u64::MAX, u64::MAX]);

    /// Create from low 128 bits and high 128 bits.
    #[inline]
    pub const fn new(lo: u128, hi: u128) -> Self {
        Self([lo as u64, (lo >> 64) as u64, hi as u64, (hi >> 64) as u64])
    }

    #[inline]
    pub const fn from_u128(v: u128) -> Self {
        Self::new(v, 0)
    }

    #[inline]
    pub const fn from_u64(v: u64) -> Self {
        Self([v, 0, 0, 0])
    }

    #[inline]
    pub const fn lo(&self) -> u128 {
        (self.0[0] as u128) | ((self.0[1] as u128) << 64)
    }

    #[inline]
    pub const fn hi(&self) -> u128 {
        (self.0[2] as u128) | ((self.0[3] as u128) << 64)
    }

    #[inline]
    pub const fn is_zero(&self) -> bool {
        self.0[0] == 0 && self.0[1] == 0 && self.0[2] == 0 && self.0[3] == 0
    }

    #[inline]
    pub fn try_into_u128(&self) -> Option<u128> {
        if self.0[2] == 0 && self.0[3] == 0 {
            Some(self.lo())
        } else {
            None
        }
    }

    // -- checked arithmetic --

    pub fn checked_add(self, rhs: U256) -> Option<U256> {
        let (lo, carry) = add_u128_carry(self.lo(), rhs.lo(), false);
        let (hi, overflow) = add_u128_carry(self.hi(), rhs.hi(), carry);
        if overflow {
            None
        } else {
            Some(U256::new(lo, hi))
        }
    }

    pub fn checked_sub(self, rhs: U256) -> Option<U256> {
        let (lo, borrow) = sub_u128_borrow(self.lo(), rhs.lo(), false);
        let (hi, underflow) = sub_u128_borrow(self.hi(), rhs.hi(), borrow);
        if underflow {
            None
        } else {
            Some(U256::new(lo, hi))
        }
    }

    pub fn checked_mul(self, rhs: U256) -> Option<U256> {
        if self.hi() != 0 && rhs.hi() != 0 {
            return None;
        }

        let (prod_lo, prod_hi) = widening_mul_u128(self.lo(), rhs.lo());

        let cross1 = if rhs.hi() != 0 {
            let (c, overflow) = widening_mul_u128(self.lo(), rhs.hi());
            if overflow != 0 {
                return None;
            }
            c
        } else {
            0u128
        };

        let cross2 = if self.hi() != 0 {
            let (c, overflow) = widening_mul_u128(self.hi(), rhs.lo());
            if overflow != 0 {
                return None;
            }
            c
        } else {
            0u128
        };

        let hi = prod_hi.checked_add(cross1)?;
        let hi = hi.checked_add(cross2)?;

        Some(U256::new(prod_lo, hi))
    }

    pub fn checked_div(self, rhs: U256) -> Option<U256> {
        if rhs.is_zero() {
            return None;
        }
        Some(div_rem_u256(self, rhs).0)
    }

    pub fn checked_rem(self, rhs: U256) -> Option<U256> {
        if rhs.is_zero() {
            return None;
        }
        Some(div_rem_u256(self, rhs).1)
    }

    // -- overflowing --

    pub fn overflowing_add(self, rhs: U256) -> (U256, bool) {
        let (lo, carry) = add_u128_carry(self.lo(), rhs.lo(), false);
        let (hi, overflow) = add_u128_carry(self.hi(), rhs.hi(), carry);
        (U256::new(lo, hi), overflow)
    }

    pub fn overflowing_sub(self, rhs: U256) -> (U256, bool) {
        let (lo, borrow) = sub_u128_borrow(self.lo(), rhs.lo(), false);
        let (hi, underflow) = sub_u128_borrow(self.hi(), rhs.hi(), borrow);
        (U256::new(lo, hi), underflow)
    }

    // -- saturating --

    pub fn saturating_add(self, rhs: U256) -> U256 {
        self.checked_add(rhs).unwrap_or(U256::MAX)
    }

    pub fn saturating_sub(self, rhs: U256) -> U256 {
        self.checked_sub(rhs).unwrap_or(U256::ZERO)
    }

    // -- shifts --

    pub fn shl(self, bits: u32) -> U256 {
        if bits >= 256 {
            return U256::ZERO;
        }
        if bits == 0 {
            return self;
        }
        let lo = self.lo();
        let hi = self.hi();
        if bits >= 128 {
            let s = bits - 128;
            U256::new(0, lo << s)
        } else {
            let new_lo = lo << bits;
            let new_hi = (hi << bits) | (lo >> (128 - bits));
            U256::new(new_lo, new_hi)
        }
    }

    pub fn shr(self, bits: u32) -> U256 {
        if bits >= 256 {
            return U256::ZERO;
        }
        if bits == 0 {
            return self;
        }
        let lo = self.lo();
        let hi = self.hi();
        if bits >= 128 {
            let s = bits - 128;
            U256::new(hi >> s, 0)
        } else {
            let new_hi = hi >> bits;
            let new_lo = (lo >> bits) | (hi << (128 - bits));
            U256::new(new_lo, new_hi)
        }
    }

    // -- bitwise --

    pub fn bitand(self, rhs: U256) -> U256 {
        U256::new(self.lo() & rhs.lo(), self.hi() & rhs.hi())
    }

    pub fn bitor(self, rhs: U256) -> U256 {
        U256::new(self.lo() | rhs.lo(), self.hi() | rhs.hi())
    }
}

// ============================================================================
// U256 - Ord / PartialOrd (shared logic, both modes)
// ============================================================================
impl PartialOrd for U256 {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for U256 {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.hi().cmp(&other.hi()) {
            Ordering::Equal => self.lo().cmp(&other.lo()),
            ord => ord,
        }
    }
}

// ============================================================================
// U256 - core::ops traits (shared logic, both modes)
// ============================================================================
impl core::ops::Add for U256 {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self {
        self.checked_add(rhs).expect("U256 add overflow")
    }
}

impl core::ops::Sub for U256 {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self {
        self.checked_sub(rhs).expect("U256 sub underflow")
    }
}

impl core::ops::Mul for U256 {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: Self) -> Self {
        self.checked_mul(rhs).expect("U256 mul overflow")
    }
}

impl core::ops::Div for U256 {
    type Output = Self;
    #[inline]
    fn div(self, rhs: Self) -> Self {
        self.checked_div(rhs).expect("U256 div by zero")
    }
}

impl core::ops::Rem for U256 {
    type Output = Self;
    #[inline]
    fn rem(self, rhs: Self) -> Self {
        self.checked_rem(rhs).expect("U256 rem by zero")
    }
}

impl core::ops::Shl<u32> for U256 {
    type Output = Self;
    #[inline]
    fn shl(self, bits: u32) -> Self {
        self.shl(bits)
    }
}

impl core::ops::Shr<u32> for U256 {
    type Output = Self;
    #[inline]
    fn shr(self, bits: u32) -> Self {
        self.shr(bits)
    }
}

impl core::ops::BitAnd for U256 {
    type Output = Self;
    #[inline]
    fn bitand(self, rhs: Self) -> Self {
        self.bitand(rhs)
    }
}

impl core::ops::BitOr for U256 {
    type Output = Self;
    #[inline]
    fn bitor(self, rhs: Self) -> Self {
        self.bitor(rhs)
    }
}

impl core::ops::AddAssign for U256 {
    #[inline]
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl core::ops::SubAssign for U256 {
    #[inline]
    fn sub_assign(&mut self, rhs: Self) {
        *self = *self - rhs;
    }
}

// ============================================================================
// I256 -- Kani version
// ============================================================================
#[cfg(kani)]
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct I256([u128; 2]); // two's complement [lo, hi]; sign bit is bit 127 of hi

#[cfg(kani)]
impl I256 {
    pub const ZERO: Self = Self([0, 0]);
    pub const ONE: Self = Self([1, 0]);
    pub const MINUS_ONE: Self = Self([u128::MAX, u128::MAX]); // all ones in two's complement
    /// Largest positive value: hi = 0x7FFF...FFFF, lo = 0xFFFF...FFFF
    pub const MAX: Self = Self([u128::MAX, u128::MAX >> 1]);
    /// Most negative value: hi = 0x8000...0000, lo = 0
    pub const MIN: Self = Self([0, 1u128 << 127]);

    pub fn from_i128(v: i128) -> Self {
        // Sign-extend into hi word.
        let lo = v as u128;
        let hi = if v < 0 { u128::MAX } else { 0 };
        Self([lo, hi])
    }

    pub fn from_u128(v: u128) -> Self {
        // Must be non-negative and fit in I256 (i.e. hi sign bit must be 0).
        // Since lo = v and hi = 0, the sign bit is clear as long as v < 2^128.
        // But v is u128 so this always fits (I256::MAX has hi = 2^127-1 > 0).
        Self([v, 0])
    }

    pub fn try_into_i128(&self) -> Option<i128> {
        // Value fits in i128 iff hi is the sign-extension of lo's sign bit.
        let lo = self.0[0];
        let hi = self.0[1];
        let lo_sign_ext = if (lo as i128) < 0 { u128::MAX } else { 0 };
        if hi == lo_sign_ext {
            Some(lo as i128)
        } else {
            None
        }
    }

    pub fn is_zero(&self) -> bool {
        self.0[0] == 0 && self.0[1] == 0
    }

    pub fn is_negative(&self) -> bool {
        (self.0[1] >> 127) != 0
    }

    pub fn is_positive(&self) -> bool {
        !self.is_zero() && !self.is_negative()
    }

    pub fn signum(&self) -> i8 {
        if self.is_zero() {
            0
        } else if self.is_negative() {
            -1
        } else {
            1
        }
    }

    /// Return the absolute value as U256. Panics on I256::MIN.
    pub fn abs_u256(self) -> U256 {
        if self.is_negative() {
            // Negate: invert + 1. Panics for MIN since ~MIN+1 overflows back to MIN
            // which would be 2^255, i.e. U256 with hi = 1<<127, which is fine actually.
            // But the spec says panics for MIN, so we check.
            assert!(self != Self::MIN, "abs_u256 called on I256::MIN");
            let inv_lo = !self.0[0];
            let inv_hi = !self.0[1];
            let (neg_lo, carry) = inv_lo.overflowing_add(1);
            let neg_hi = inv_hi.wrapping_add(if carry { 1 } else { 0 });
            U256::new(neg_lo, neg_hi)
        } else {
            U256::new(self.0[0], self.0[1])
        }
    }

    /// Checked signed I256 * I256 multiplication via abs/sign decomposition.
    /// Returns None on overflow (result doesn't fit I256).
    pub fn checked_mul_i256(self, rhs: I256) -> Option<I256> {
        if self.is_zero() || rhs.is_zero() {
            return Some(I256::ZERO);
        }
        let neg = self.is_negative() != rhs.is_negative();
        // Handle MIN carefully: abs_u256 panics on MIN, but MIN * 1 = MIN, MIN * -1 = overflow
        if self == I256::MIN {
            if rhs == I256::ONE {
                return Some(I256::MIN);
            }
            if rhs == I256::MINUS_ONE {
                return None;
            } // -MIN > MAX
            return None; // |MIN| * |rhs>1| > MAX
        }
        if rhs == I256::MIN {
            if self == I256::ONE {
                return Some(I256::MIN);
            }
            if self == I256::MINUS_ONE {
                return None;
            }
            return None;
        }
        let abs_a = self.abs_u256();
        let abs_b = rhs.abs_u256();
        let product = abs_a.checked_mul(abs_b)?;
        if neg {
            // Result must be <= 2^255 (magnitude of MIN)
            // 2^255 as U256: hi limb has bit 127 set (for [u128;2]) or bit 63 of limb[3] (for [u64;4])
            let min_mag = U256::new(0, 1u128 << 127);
            // For exactly 2^255, result is MIN
            if product == min_mag {
                return Some(I256::MIN);
            }
            if product > min_mag {
                return None;
            }
            // product < 2^255: fits as negative I256
            let pos = I256::from_u256_or_overflow(product)?;
            pos.checked_neg()
        } else {
            I256::from_u256_or_overflow(product)
        }
    }

    // -- checked arithmetic --

    pub fn checked_add(self, rhs: I256) -> Option<I256> {
        let (lo, carry) = self.0[0].overflowing_add(rhs.0[0]);
        let (hi, overflow1) = self.0[1].overflowing_add(rhs.0[1]);
        let (hi, overflow2) = hi.overflowing_add(if carry { 1 } else { 0 });
        let result = I256([lo, hi]);

        // Signed overflow: if both operands have the same sign and the result
        // has a different sign, we overflowed.
        let self_neg = self.is_negative();
        let rhs_neg = rhs.is_negative();
        let res_neg = result.is_negative();

        // Unsigned carries should be consistent with sign expectations:
        // For two's complement addition, overflow iff same-sign inputs produce
        // different-sign result.
        if self_neg == rhs_neg && res_neg != self_neg {
            None
        } else {
            Some(result)
        }
    }

    pub fn checked_sub(self, rhs: I256) -> Option<I256> {
        let neg_rhs = match rhs.checked_neg() {
            Some(n) => n,
            None => {
                // rhs == MIN. self - MIN = self + 2^255.
                // This is valid only if self is non-negative (result fits in I256).
                // self - MIN: we'll do it directly.
                let (lo, borrow) = self.0[0].overflowing_sub(rhs.0[0]);
                let (hi, underflow1) = self.0[1].overflowing_sub(rhs.0[1]);
                let (hi, underflow2) = hi.overflowing_sub(if borrow { 1 } else { 0 });
                let result = I256([lo, hi]);
                // Check: subtracting a negative from anything should not make it
                // more negative. self - MIN where MIN is negative. If self >= 0,
                // result = self + |MIN| which could overflow. If self < 0,
                // result = self + |MIN| which could be in range.
                let self_neg = self.is_negative();
                let rhs_neg = true; // MIN is negative
                let res_neg = result.is_negative();
                // sub overflow: signs differ (self_neg != rhs_neg) and result
                // sign != self sign.
                if self_neg != rhs_neg && res_neg != self_neg {
                    return None;
                }
                return Some(result);
            }
        };
        self.checked_add(neg_rhs)
    }

    pub fn checked_neg(self) -> Option<I256> {
        if self == Self::MIN {
            return None;
        }
        let inv_lo = !self.0[0];
        let inv_hi = !self.0[1];
        let (neg_lo, carry) = inv_lo.overflowing_add(1);
        let neg_hi = inv_hi.wrapping_add(if carry { 1 } else { 0 });
        Some(I256([neg_lo, neg_hi]))
    }

    pub fn saturating_add(self, rhs: I256) -> I256 {
        match self.checked_add(rhs) {
            Some(v) => v,
            None => {
                if rhs.is_negative() {
                    I256::MIN
                } else {
                    I256::MAX
                }
            }
        }
    }

    /// Convert this I256 to a raw U256 (reinterpret the bits).
    fn as_raw_u256(self) -> U256 {
        U256::new(self.0[0], self.0[1])
    }

    /// Create I256 from raw U256 bits (reinterpret).
    pub fn from_raw_u256(v: U256) -> Self {
        I256([v.lo(), v.hi()])
    }

    /// Convert U256 to I256, returning None if the value exceeds i256 max (sign bit set).
    pub fn from_u256_or_overflow(v: U256) -> Option<Self> {
        // Sign bit is bit 255 = bit 127 of hi limb
        if v.hi() >> 127 != 0 {
            return None;
        }
        Some(Self::from_raw_u256(v))
    }
}

// ============================================================================
// I256 -- BPF version
// ============================================================================
#[cfg(not(kani))]
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct I256([u64; 4]); // two's complement, little-endian limbs

#[cfg(not(kani))]
impl I256 {
    pub const ZERO: Self = Self([0, 0, 0, 0]);
    pub const ONE: Self = Self([1, 0, 0, 0]);
    pub const MINUS_ONE: Self = Self([u64::MAX, u64::MAX, u64::MAX, u64::MAX]);
    pub const MAX: Self = Self([u64::MAX, u64::MAX, u64::MAX, u64::MAX >> 1]);
    pub const MIN: Self = Self([0, 0, 0, 1u64 << 63]);

    pub fn from_i128(v: i128) -> Self {
        let lo = v as u128;
        let hi: u128 = if v < 0 { u128::MAX } else { 0 };
        Self::from_lo_hi(lo, hi)
    }

    pub fn from_u128(v: u128) -> Self {
        Self::from_lo_hi(v, 0)
    }

    pub fn try_into_i128(&self) -> Option<i128> {
        let lo = self.lo_u128();
        let hi = self.hi_u128();
        let lo_sign_ext = if (lo as i128) < 0 { u128::MAX } else { 0 };
        if hi == lo_sign_ext {
            Some(lo as i128)
        } else {
            None
        }
    }

    pub fn is_zero(&self) -> bool {
        self.0[0] == 0 && self.0[1] == 0 && self.0[2] == 0 && self.0[3] == 0
    }

    pub fn is_negative(&self) -> bool {
        (self.0[3] >> 63) != 0
    }

    pub fn is_positive(&self) -> bool {
        !self.is_zero() && !self.is_negative()
    }

    pub fn signum(&self) -> i8 {
        if self.is_zero() {
            0
        } else if self.is_negative() {
            -1
        } else {
            1
        }
    }

    pub fn abs_u256(self) -> U256 {
        if self.is_negative() {
            assert!(self != Self::MIN, "abs_u256 called on I256::MIN");
            let lo = self.lo_u128();
            let hi = self.hi_u128();
            let inv_lo = !lo;
            let inv_hi = !hi;
            let (neg_lo, carry) = inv_lo.overflowing_add(1);
            let neg_hi = inv_hi.wrapping_add(if carry { 1 } else { 0 });
            U256::new(neg_lo, neg_hi)
        } else {
            U256::new(self.lo_u128(), self.hi_u128())
        }
    }

    // -- checked arithmetic --

    /// Checked signed I256 * I256 multiplication via abs/sign decomposition.
    /// Returns None on overflow (result doesn't fit I256).
    pub fn checked_mul_i256(self, rhs: I256) -> Option<I256> {
        if self.is_zero() || rhs.is_zero() {
            return Some(I256::ZERO);
        }
        let neg = self.is_negative() != rhs.is_negative();
        // Handle MIN carefully: abs_u256 panics on MIN, but MIN * 1 = MIN, MIN * -1 = overflow
        if self == I256::MIN {
            if rhs == I256::ONE {
                return Some(I256::MIN);
            }
            if rhs == I256::MINUS_ONE {
                return None;
            } // -MIN > MAX
            return None; // |MIN| * |rhs>1| > MAX
        }
        if rhs == I256::MIN {
            if self == I256::ONE {
                return Some(I256::MIN);
            }
            if self == I256::MINUS_ONE {
                return None;
            }
            return None;
        }
        let abs_a = self.abs_u256();
        let abs_b = rhs.abs_u256();
        let product = abs_a.checked_mul(abs_b)?;
        if neg {
            // Result must be <= 2^255 (magnitude of MIN)
            // 2^255 as U256: hi limb has bit 127 set (for [u128;2]) or bit 63 of limb[3] (for [u64;4])
            let min_mag = U256::new(0, 1u128 << 127);
            // For exactly 2^255, result is MIN
            if product == min_mag {
                return Some(I256::MIN);
            }
            if product > min_mag {
                return None;
            }
            // product < 2^255: fits as negative I256
            let pos = I256::from_u256_or_overflow(product)?;
            pos.checked_neg()
        } else {
            I256::from_u256_or_overflow(product)
        }
    }

    pub fn checked_add(self, rhs: I256) -> Option<I256> {
        let s_lo = self.lo_u128();
        let s_hi = self.hi_u128();
        let r_lo = rhs.lo_u128();
        let r_hi = rhs.hi_u128();
        let (lo, carry) = s_lo.overflowing_add(r_lo);
        // overflow bits unused — this is a wrapping-add for signed I256; the
        // sign-based overflow detection below uses the result, not the
        // unsigned-overflow flags.
        let (hi, _overflow1) = s_hi.overflowing_add(r_hi);
        let (hi, _overflow2) = hi.overflowing_add(if carry { 1 } else { 0 });
        let result = I256::from_lo_hi(lo, hi);

        let self_neg = self.is_negative();
        let rhs_neg = rhs.is_negative();
        let res_neg = result.is_negative();

        if self_neg == rhs_neg && res_neg != self_neg {
            None
        } else {
            Some(result)
        }
    }

    pub fn checked_sub(self, rhs: I256) -> Option<I256> {
        let neg_rhs = match rhs.checked_neg() {
            Some(n) => n,
            None => {
                let s_lo = self.lo_u128();
                let s_hi = self.hi_u128();
                let r_lo = rhs.lo_u128();
                let r_hi = rhs.hi_u128();
                let (lo, borrow) = s_lo.overflowing_sub(r_lo);
                let (hi, _underflow1) = s_hi.overflowing_sub(r_hi);
                let (hi, _underflow2) = hi.overflowing_sub(if borrow { 1 } else { 0 });
                let result = I256::from_lo_hi(lo, hi);
                let self_neg = self.is_negative();
                let res_neg = result.is_negative();
                if self_neg != true && res_neg != self_neg {
                    return None;
                }
                return Some(result);
            }
        };
        self.checked_add(neg_rhs)
    }

    pub fn checked_neg(self) -> Option<I256> {
        if self == Self::MIN {
            return None;
        }
        let lo = self.lo_u128();
        let hi = self.hi_u128();
        let inv_lo = !lo;
        let inv_hi = !hi;
        let (neg_lo, carry) = inv_lo.overflowing_add(1);
        let neg_hi = inv_hi.wrapping_add(if carry { 1 } else { 0 });
        Some(I256::from_lo_hi(neg_lo, neg_hi))
    }

    pub fn saturating_add(self, rhs: I256) -> I256 {
        match self.checked_add(rhs) {
            Some(v) => v,
            None => {
                if rhs.is_negative() {
                    I256::MIN
                } else {
                    I256::MAX
                }
            }
        }
    }

    // internal helpers
    fn lo_u128(&self) -> u128 {
        (self.0[0] as u128) | ((self.0[1] as u128) << 64)
    }

    fn hi_u128(&self) -> u128 {
        (self.0[2] as u128) | ((self.0[3] as u128) << 64)
    }

    fn from_lo_hi(lo: u128, hi: u128) -> Self {
        Self([lo as u64, (lo >> 64) as u64, hi as u64, (hi >> 64) as u64])
    }

    fn as_raw_u256(self) -> U256 {
        U256::new(self.lo_u128(), self.hi_u128())
    }

    pub fn from_raw_u256(v: U256) -> Self {
        Self::from_lo_hi(v.lo(), v.hi())
    }

    /// Convert U256 to I256, returning None if the value exceeds i256 max (sign bit set).
    pub fn from_u256_or_overflow(v: U256) -> Option<Self> {
        if v.hi() >> 127 != 0 {
            return None;
        }
        Some(Self::from_raw_u256(v))
    }
}

// ============================================================================
// I256 - Ord / PartialOrd (shared, both modes)
// ============================================================================
impl PartialOrd for I256 {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for I256 {
    fn cmp(&self, other: &Self) -> Ordering {
        let self_neg = self.is_negative();
        let other_neg = other.is_negative();
        match (self_neg, other_neg) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            _ => {
                // Same sign: compare as unsigned (works for two's complement
                // when both values have the same sign bit).
                self.as_raw_u256().cmp(&other.as_raw_u256())
            }
        }
    }
}

// ============================================================================
// I256 - core::ops traits (shared, both modes)
// ============================================================================
impl core::ops::Add for I256 {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self {
        self.checked_add(rhs).expect("I256 add overflow")
    }
}

impl core::ops::Sub for I256 {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self {
        self.checked_sub(rhs).expect("I256 sub overflow")
    }
}

impl core::ops::Neg for I256 {
    type Output = Self;
    #[inline]
    fn neg(self) -> Self {
        self.checked_neg().expect("I256 neg overflow (MIN)")
    }
}

// ============================================================================
// Shared helpers (used by both Kani and BPF)
// ============================================================================

/// Widening multiply: u128 * u128 -> (lo: u128, hi: u128).
/// Schoolbook on u64 halves.
fn widening_mul_u128(a: u128, b: u128) -> (u128, u128) {
    let a_lo = a as u64 as u128;
    let a_hi = (a >> 64) as u64 as u128;
    let b_lo = b as u64 as u128;
    let b_hi = (b >> 64) as u64 as u128;

    let ll = a_lo * b_lo; // 0..2^128
    let lh = a_lo * b_hi; // 0..2^128
    let hl = a_hi * b_lo; // 0..2^128
    let hh = a_hi * b_hi; // 0..2^128

    // Accumulate:
    //   result = ll + (lh + hl) << 64 + hh << 128
    let (mid, mid_carry) = lh.overflowing_add(hl); // mid_carry means +2^128

    let (lo, lo_carry) = ll.overflowing_add(mid << 64);
    let hi = hh + (mid >> 64) + ((mid_carry as u128) << 64) + (lo_carry as u128);
    // lo_carry is at most 1, captured in hi

    (lo, hi)
}

/// Add two u128 with an incoming carry, returning (result, carry_out).
#[cfg(not(kani))]
fn add_u128_carry(a: u128, b: u128, carry_in: bool) -> (u128, bool) {
    let (s1, c1) = a.overflowing_add(b);
    let (s2, c2) = s1.overflowing_add(carry_in as u128);
    (s2, c1 || c2)
}

/// Subtract two u128 with an incoming borrow, returning (result, borrow_out).
#[cfg(not(kani))]
fn sub_u128_borrow(a: u128, b: u128, borrow_in: bool) -> (u128, bool) {
    let (d1, b1) = a.overflowing_sub(b);
    let (d2, b2) = d1.overflowing_sub(borrow_in as u128);
    (d2, b1 || b2)
}

// ============================================================================
// U256 division: binary long division
// ============================================================================

/// Count leading zeros of a U256.
fn leading_zeros_u256(v: U256) -> u32 {
    if v.hi() != 0 {
        v.hi().leading_zeros()
    } else {
        128 + v.lo().leading_zeros()
    }
}

/// Divide U256 by U256, returning (quotient, remainder). Panics if divisor is zero.
pub fn div_rem_u256(num: U256, den: U256) -> (U256, U256) {
    if den.is_zero() {
        panic!("U256 division by zero");
    }
    if num.is_zero() {
        return (U256::ZERO, U256::ZERO);
    }

    // If denominator > numerator, quotient = 0
    if den > num {
        return (U256::ZERO, num);
    }

    // If denominator fits in u128 and numerator fits in u128, do it natively.
    if num.hi() == 0 && den.hi() == 0 {
        let q = num.lo() / den.lo();
        let r = num.lo() % den.lo();
        return (U256::from_u128(q), U256::from_u128(r));
    }

    // Binary long division
    let shift = leading_zeros_u256(den) - leading_zeros_u256(num);
    let mut remainder = num;
    let mut quotient = U256::ZERO;
    let mut divisor = den.shl(shift);

    // We iterate shift+1 times (from bit `shift` down to 0)
    let mut i = shift as i32;
    while i >= 0 {
        if remainder >= divisor {
            remainder = remainder.saturating_sub(divisor);
            quotient = quotient.bitor(U256::ONE.shl(i as u32));
        }
        divisor = divisor.shr(1);
        i -= 1;
    }

    (quotient, remainder)
}

// ============================================================================
// U512 - private intermediate for mul_div operations
// ============================================================================

/// Private 512-bit unsigned integer for intermediate computations.
/// Stored as [u128; 4] in little-endian order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct U512([u128; 4]);

impl U512 {
    const ZERO: Self = Self([0, 0, 0, 0]);

    fn is_zero(&self) -> bool {
        self.0[0] == 0 && self.0[1] == 0 && self.0[2] == 0 && self.0[3] == 0
    }

    fn from_u256(v: U256) -> Self {
        Self([v.lo(), v.hi(), 0, 0])
    }

    /// Widening multiply of two U256 values into U512.
    fn mul_u256(a: U256, b: U256) -> Self {
        // Schoolbook: a = [a0, a1], b = [b0, b1] where each is u128.
        let a0 = a.lo();
        let a1 = a.hi();
        let b0 = b.lo();
        let b1 = b.hi();

        // a0*b0 -> occupies [0..1]
        let (r0, c0) = widening_mul_u128(a0, b0);

        // a0*b1 -> occupies [1..2]
        let (x1, x2) = widening_mul_u128(a0, b1);

        // a1*b0 -> occupies [1..2]
        let (y1, y2) = widening_mul_u128(a1, b0);

        // a1*b1 -> occupies [2..3]
        let (z2, z3) = widening_mul_u128(a1, b1);

        // Accumulate into 4 limbs [r0, r1, r2, r3]
        // r0 is already set.
        // r1 = c0 + x1 + y1   (with carries into r2)
        let (r1, carry1a) = c0.overflowing_add(x1);
        let (r1, carry1b) = r1.overflowing_add(y1);
        let carry1 = (carry1a as u128) + (carry1b as u128);

        // r2 = x2 + y2 + z2 + carry1
        let (r2, carry2a) = x2.overflowing_add(y2);
        let (r2, carry2b) = r2.overflowing_add(z2);
        let (r2, carry2c) = r2.overflowing_add(carry1);
        let carry2 = (carry2a as u128) + (carry2b as u128) + (carry2c as u128);

        // r3 = z3 + carry2
        let r3 = z3 + carry2; // cannot overflow because max product is (2^256-1)^2 < 2^512

        Self([r0, r1, r2, r3])
    }

    /// Compare two U512 values.
    fn cmp_u512(&self, other: &Self) -> Ordering {
        for i in (0..4).rev() {
            match self.0[i].cmp(&other.0[i]) {
                Ordering::Equal => continue,
                ord => return ord,
            }
        }
        Ordering::Equal
    }

    /// Shift left by `bits`. Saturates to zero if bits >= 512.
    fn shl_u512(self, bits: u32) -> Self {
        if bits >= 512 {
            return Self::ZERO;
        }
        if bits == 0 {
            return self;
        }
        let word_shift = (bits / 128) as usize;
        let bit_shift = bits % 128;

        let mut result = [0u128; 4];
        for i in word_shift..4 {
            result[i] = self.0[i - word_shift] << bit_shift;
            if bit_shift > 0 && i > word_shift {
                result[i] |= self.0[i - word_shift - 1] >> (128 - bit_shift);
            }
        }
        Self(result)
    }

    /// Shift right by `bits`.
    fn shr_u512(self, bits: u32) -> Self {
        if bits >= 512 {
            return Self::ZERO;
        }
        if bits == 0 {
            return self;
        }
        let word_shift = (bits / 128) as usize;
        let bit_shift = bits % 128;

        let mut result = [0u128; 4];
        for i in 0..(4 - word_shift) {
            result[i] = self.0[i + word_shift] >> bit_shift;
            if bit_shift > 0 && (i + word_shift + 1) < 4 {
                result[i] |= self.0[i + word_shift + 1] << (128 - bit_shift);
            }
        }
        Self(result)
    }

    /// Subtract rhs from self. Assumes self >= rhs.
    fn sub_u512(self, rhs: Self) -> Self {
        let mut result = [0u128; 4];
        let mut borrow = false;
        for i in 0..4 {
            let (d1, b1) = self.0[i].overflowing_sub(rhs.0[i]);
            let (d2, b2) = d1.overflowing_sub(borrow as u128);
            result[i] = d2;
            borrow = b1 || b2;
        }
        Self(result)
    }

    /// Bitwise OR with a U512 that has only the bit at position `bit` set.
    fn set_bit(self, bit: u32) -> Self {
        if bit >= 512 {
            return self;
        }
        let word = (bit / 128) as usize;
        let b = bit % 128;
        let mut result = self.0;
        result[word] |= 1u128 << b;
        Self(result)
    }

    /// Count leading zeros.
    fn leading_zeros(&self) -> u32 {
        for i in (0..4).rev() {
            if self.0[i] != 0 {
                return (3 - i as u32) * 128 + self.0[i].leading_zeros();
            }
        }
        512
    }

    /// Convert to U256, returning None if the value doesn't fit.
    fn try_into_u256(self) -> Option<U256> {
        if self.0[2] != 0 || self.0[3] != 0 {
            None
        } else {
            Some(U256::new(self.0[0], self.0[1]))
        }
    }

    /// Divide U512 by U256, returning (quotient as U256, remainder as U256).
    /// Panics if divisor is zero or quotient doesn't fit in U256.
    fn div_rem_by_u256(self, den: U256) -> (U256, U256) {
        match self.checked_div_rem_by_u256(den) {
            Some(result) => result,
            None => panic!("mul_div quotient must fit U256"),
        }
    }

    /// Checked variant: returns None if quotient doesn't fit in U256.
    fn checked_div_rem_by_u256(self, den: U256) -> Option<(U256, U256)> {
        assert!(!den.is_zero(), "U512 division by zero");

        if self.is_zero() {
            return Some((U256::ZERO, U256::ZERO));
        }

        let den_512 = U512::from_u256(den);

        if self.cmp_u512(&den_512) == Ordering::Less {
            let r = self.try_into_u256().expect("remainder must fit U256");
            return Some((U256::ZERO, r));
        }

        let num_lz = self.leading_zeros();
        let den_lz = den_512.leading_zeros();

        if den_lz < num_lz {
            let r = self.try_into_u256().expect("remainder must fit U256");
            return Some((U256::ZERO, r));
        }

        let shift = den_lz - num_lz;
        let mut remainder = self;
        let mut quotient = U512::ZERO;
        let mut divisor = den_512.shl_u512(shift);

        let mut i = shift as i32;
        while i >= 0 {
            if remainder.cmp_u512(&divisor) != Ordering::Less {
                remainder = remainder.sub_u512(divisor);
                quotient = quotient.set_bit(i as u32);
            }
            divisor = divisor.shr_u512(1);
            i -= 1;
        }

        let q = quotient.try_into_u256()?;
        let r = remainder.try_into_u256().expect("remainder must fit U256");
        Some((q, r))
    }
}

// ============================================================================
// Spec section 4.6 helpers
// ============================================================================

/// Spec section 4.6: signed floor division with positive denominator.
///
/// Computes floor(n / d) where d > 0. Uses truncation toward zero, then
/// adjusts: if n < 0 and there is a non-zero remainder, subtract 1.
pub fn floor_div_signed_conservative(n: I256, d: U256) -> I256 {
    assert!(
        !d.is_zero(),
        "floor_div_signed_conservative: zero denominator"
    );

    if n.is_zero() {
        return I256::ZERO;
    }

    let negative = n.is_negative();

    // Compute |n| without negating I256 directly for MIN safety.
    // We reinterpret the bits and do unsigned division.
    if !negative {
        // n >= 0: floor(n/d) = trunc(n/d) since both positive.
        let n_u = n.abs_u256();
        let (q, _r) = div_rem_u256(n_u, d);
        // q fits in I256 since n was positive and q <= n.
        I256::from_raw_u256(q)
    } else {
        // n < 0. We need floor(n / d).
        // n = -|n|. trunc(n/d) = -(|n| / d). floor = trunc - (1 if |n| % d != 0).
        //
        // Work with the raw bits to avoid I256::MIN negation issues.
        //
        // Two's complement: if n is negative, its unsigned representation is 2^256 - |n|.
        // We can compute |n| = ~n + 1 (bitwise not + 1).
        let raw = n.as_raw_u256();
        // |n| = ~raw + 1
        let inv = U256::new(!raw.lo(), !raw.hi());
        let abs_n = inv.checked_add(U256::ONE).expect("abs of negative I256");

        let (q, r) = div_rem_u256(abs_n, d);

        // Result = -q if r == 0, else -(q+1)
        let q_final = if r.is_zero() {
            q
        } else {
            q.checked_add(U256::ONE)
                .expect("floor_div quotient overflow")
        };

        // Negate q_final to get the negative I256 result.
        // q_final as I256 then negate.
        if q_final.is_zero() {
            I256::ZERO
        } else {
            let qi = I256::from_raw_u256(q_final);
            qi.checked_neg().expect("floor_div result out of range")
        }
    }
}

/// Native i128 floor division: floor(n / d) for positive d, rounding toward
/// negative infinity. Mirrors `floor_div_signed_conservative` but uses native
/// i128/u128 arithmetic for the funding-term computation (spec §5.4).
pub fn floor_div_signed_conservative_i128(n: i128, d: u128) -> i128 {
    assert!(
        d != 0,
        "floor_div_signed_conservative_i128: zero denominator"
    );

    if n == 0 {
        return 0;
    }

    if n > 0 {
        // Non-negative: floor = truncation
        (n as u128 / d) as i128
    } else {
        // Negative: floor(n/d) = -(|n| / d) - (if |n| % d != 0 then 1 else 0)
        let abs_n = n.unsigned_abs();
        let q = abs_n / d;
        let r = abs_n % d;
        let q_final = if r != 0 { q + 1 } else { q };
        assert!(
            q_final <= i128::MAX as u128,
            "floor_div_signed_conservative_i128: result out of range"
        );
        -(q_final as i128)
    }
}

/// Spec section 4.6: positive ceiling division.
/// ceil(n / d) = (n + d - 1) / d, but we use the remainder form to avoid overflow:
/// ceil(n / d) = trunc(n / d) + (1 if n % d != 0 else 0).
pub fn ceil_div_positive_checked(n: U256, d: U256) -> U256 {
    assert!(!d.is_zero(), "ceil_div_positive_checked: zero denominator");
    let (q, r) = div_rem_u256(n, d);
    if r.is_zero() {
        q
    } else {
        q.checked_add(U256::ONE).expect("ceil_div overflow")
    }
}

/// Spec section 4.6: exact wide product then floor divide.
/// Computes floor(a * b / d) using a U512 intermediate to avoid overflow.
pub fn mul_div_floor_u256(a: U256, b: U256, d: U256) -> U256 {
    assert!(!d.is_zero(), "mul_div_floor_u256: zero denominator");
    let product = U512::mul_u256(a, b);
    let (q, _r) = product.div_rem_by_u256(d);
    q
}

/// Like mul_div_floor_u256 but also returns the remainder.
/// Returns (floor(a * b / d), (a * b) mod d).
pub fn mul_div_floor_u256_with_rem(a: U256, b: U256, d: U256) -> (U256, U256) {
    assert!(
        !d.is_zero(),
        "mul_div_floor_u256_with_rem: zero denominator"
    );
    let product = U512::mul_u256(a, b);
    product.div_rem_by_u256(d)
}

/// Spec section 4.6: exact wide product then ceiling divide.
/// Computes ceil(a * b / d) using a U512 intermediate.
pub fn mul_div_ceil_u256(a: U256, b: U256, d: U256) -> U256 {
    assert!(!d.is_zero(), "mul_div_ceil_u256: zero denominator");
    let product = U512::mul_u256(a, b);
    let (q, r) = product.div_rem_by_u256(d);
    if r.is_zero() {
        q
    } else {
        q.checked_add(U256::ONE).expect("mul_div_ceil overflow")
    }
}

/// Checked variant of mul_div_ceil_u256.
/// Returns None if the quotient doesn't fit in U256.
#[allow(dead_code)]
pub fn checked_mul_div_ceil_u256(a: U256, b: U256, d: U256) -> Option<U256> {
    if d.is_zero() {
        return None;
    }
    let product = U512::mul_u256(a, b);
    let (q, r) = product.checked_div_rem_by_u256(d)?;
    if r.is_zero() {
        Some(q)
    } else {
        q.checked_add(U256::ONE)
    }
}

/// Spec section 4.6: saturating multiply for warmup cap.
pub fn saturating_mul_u256_u64(a: U256, b: u64) -> U256 {
    let rhs = U256::from_u64(b);
    a.checked_mul(rhs).unwrap_or(U256::MAX)
}

/// Spec section 4.6: checked fee-debt conversion.
/// If fee_credits < 0, the account owes fees. Returns the unsigned debt.
/// If fee_credits >= 0, returns 0 (no debt).
pub fn fee_debt_u128_checked(fee_credits: i128) -> u128 {
    if fee_credits < 0 {
        // debt = -fee_credits. Use checked_neg to handle i128::MIN.
        // i128::MIN.unsigned_abs() is safe and returns 2^127.
        fee_credits.unsigned_abs()
    } else {
        0
    }
}

/// Spec section 1.5 item 11: wide signed mul-div for pnl_delta.
///
/// Computes floor_div_signed_conservative(abs_basis * k_diff, denominator)
/// where the numerator may exceed 256 bits.
///
/// Uses the sign of `k_diff`. Computes `abs_basis * abs(k_diff)` as U512,
/// then applies floor_div_signed_conservative logic.
pub fn wide_signed_mul_div_floor(abs_basis: U256, k_diff: I256, denominator: U256) -> I256 {
    assert!(
        !denominator.is_zero(),
        "wide_signed_mul_div_floor: zero denominator"
    );

    if k_diff.is_zero() || abs_basis.is_zero() {
        return I256::ZERO;
    }

    let negative = k_diff.is_negative();
    let abs_k = if negative {
        assert!(
            k_diff != I256::MIN,
            "wide_signed_mul_div_floor: k_diff == I256::MIN"
        );
        k_diff.abs_u256()
    } else {
        k_diff.abs_u256()
    };

    // Wide product: abs_basis * abs_k as U512
    let product = U512::mul_u256(abs_basis, abs_k);
    let (q, r) = product.div_rem_by_u256(denominator);

    if !negative {
        // Positive: floor division = truncation
        I256::from_raw_u256(q)
    } else {
        // Negative: if remainder != 0, subtract 1 from quotient (floor toward -inf)
        let q_final = if r.is_zero() {
            q
        } else {
            q.checked_add(U256::ONE)
                .expect("wide_signed_mul_div_floor quotient overflow")
        };
        if q_final.is_zero() {
            I256::ZERO
        } else {
            let qi = I256::from_raw_u256(q_final);
            qi.checked_neg()
                .expect("wide_signed_mul_div_floor result out of I256 range")
        }
    }
}

// ============================================================================
// Helper: I256 from_raw_u256 / as_raw_u256 -- unified access
// ============================================================================
// These are defined as methods in each cfg block above. The free functions
// below delegate to them for use in shared code.

impl I256 {
    // Ensure the shared free-standing code can use these regardless of cfg.
    // (The methods are already defined in each cfg block.)
}

// ============================================================================
// §4.8 v11.31 Native 128-bit Arithmetic Helpers
// ============================================================================

/// Native multiply-divide floor. Product a*b must not overflow u128. Panics on d==0.
pub fn mul_div_floor_u128(a: u128, b: u128, d: u128) -> u128 {
    assert!(d > 0, "mul_div_floor_u128: division by zero");
    let p = a.checked_mul(b).expect("mul_div_floor_u128: a*b overflow");
    p / d
}

/// Native multiply-divide ceil. Product a*b must not overflow u128. Panics on d==0.
pub fn mul_div_ceil_u128(a: u128, b: u128, d: u128) -> u128 {
    assert!(d > 0, "mul_div_ceil_u128: division by zero");
    let p = a.checked_mul(b).expect("mul_div_ceil_u128: a*b overflow");
    let q = p / d;
    if p % d != 0 {
        q + 1
    } else {
        q
    }
}

/// Exact wide multiply-divide floor using U256 intermediate.
/// Used for haircut paths where a*b can exceed u128::MAX.
pub fn wide_mul_div_floor_u128(a: u128, b: u128, d: u128) -> u128 {
    assert!(d > 0, "wide_mul_div_floor_u128: division by zero");
    let result = mul_div_floor_u256(U256::from_u128(a), U256::from_u128(b), U256::from_u128(d));
    result
        .try_into_u128()
        .expect("wide_mul_div_floor_u128: result exceeds u128")
}

/// Safe K-difference settlement (spec §4.8 lines 720-732).
/// Computes K-difference in wide intermediate, then multiplies and divides.
pub fn wide_signed_mul_div_floor_from_k_pair(
    abs_basis: u128,
    k_then: i128,
    k_now: i128,
    den: u128,
) -> i128 {
    assert!(den > 0, "wide_signed_mul_div_floor_from_k_pair: den == 0");
    // Compute d = k_now - k_then in wide signed to avoid i128 overflow (spec §4.8)
    let k_now_wide = I256::from_i128(k_now);
    let k_then_wide = I256::from_i128(k_then);
    let d = k_now_wide
        .checked_sub(k_then_wide)
        .expect("K-diff overflow in wide");
    if d.is_zero() || abs_basis == 0 {
        return 0i128;
    }
    let abs_d = d.abs_u256();
    let abs_basis_u256 = U256::from_u128(abs_basis);
    let den_u256 = U256::from_u128(den);
    // p = abs_basis * abs(d), exact wide product
    let p = abs_basis_u256
        .checked_mul(abs_d)
        .expect("wide product overflow");
    let (q, rem) = div_rem_u256(p, den_u256);
    if d.is_negative() {
        // mag = q + 1 if r != 0 else q
        let mag = if !rem.is_zero() {
            q.checked_add(U256::ONE).expect("mag overflow")
        } else {
            q
        };
        let mag_u128 = mag.try_into_u128().expect("mag exceeds u128");
        assert!(
            mag_u128 <= i128::MAX as u128,
            "wide_signed_mul_div_floor_from_k_pair: mag > i128::MAX"
        );
        -(mag_u128 as i128)
    } else {
        let q_u128 = q.try_into_u128().expect("quotient exceeds u128");
        assert!(
            q_u128 <= i128::MAX as u128,
            "wide_signed_mul_div_floor_from_k_pair: q > i128::MAX"
        );
        q_u128 as i128
    }
}

/// ADL delta_K representability check error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OverI128Magnitude;

/// ADL delta_K representability check.
/// Returns Ok(v) if the ceil result fits in i128 magnitude, Err otherwise.
pub fn wide_mul_div_ceil_u128_or_over_i128max(
    a: u128,
    b: u128,
    d: u128,
) -> core::result::Result<u128, OverI128Magnitude> {
    assert!(
        d > 0,
        "wide_mul_div_ceil_u128_or_over_i128max: division by zero"
    );
    let result = mul_div_ceil_u256(U256::from_u128(a), U256::from_u128(b), U256::from_u128(d));
    match result.try_into_u128() {
        Some(v) if v <= i128::MAX as u128 => Ok(v),
        _ => Err(OverI128Magnitude),
    }
}

/// Saturating multiply for warmup cap computation.
pub fn saturating_mul_u128_u64(a: u128, b: u64) -> u128 {
    if a == 0 || b == 0 {
        return 0;
    }
    let b128 = b as u128;
    a.checked_mul(b128).unwrap_or(u128::MAX)
}

// ============================================================================
// Tests
// ============================================================================
#[cfg(test)]
mod tests {
    use super::*;

    // --- U256 basic construction ---
    #[test]
    fn test_u256_zero_one_max() {
        assert!(U256::ZERO.is_zero());
        assert!(!U256::ONE.is_zero());
        assert_eq!(U256::ONE.lo(), 1);
        assert_eq!(U256::ONE.hi(), 0);
        assert_eq!(U256::MAX.lo(), u128::MAX);
        assert_eq!(U256::MAX.hi(), u128::MAX);
    }

    #[test]
    fn test_u256_from_u128() {
        let v = U256::from_u128(42);
        assert_eq!(v.lo(), 42);
        assert_eq!(v.hi(), 0);
        assert_eq!(v.try_into_u128(), Some(42));
    }

    #[test]
    fn test_u256_try_into_u128_overflow() {
        let v = U256::new(1, 1);
        assert_eq!(v.try_into_u128(), None);
    }

    // --- U256 addition ---
    #[test]
    fn test_u256_checked_add() {
        let a = U256::from_u128(100);
        let b = U256::from_u128(200);
        assert_eq!(a.checked_add(b), Some(U256::from_u128(300)));
    }

    #[test]
    fn test_u256_add_with_carry() {
        let a = U256::new(u128::MAX, 0);
        let b = U256::new(1, 0);
        let c = a.checked_add(b).unwrap();
        assert_eq!(c.lo(), 0);
        assert_eq!(c.hi(), 1);
    }

    #[test]
    fn test_u256_add_overflow() {
        assert_eq!(U256::MAX.checked_add(U256::ONE), None);
    }

    #[test]
    fn test_u256_saturating_add() {
        assert_eq!(U256::MAX.saturating_add(U256::ONE), U256::MAX);
    }

    // --- U256 subtraction ---
    #[test]
    fn test_u256_checked_sub() {
        let a = U256::from_u128(300);
        let b = U256::from_u128(100);
        assert_eq!(a.checked_sub(b), Some(U256::from_u128(200)));
    }

    #[test]
    fn test_u256_sub_underflow() {
        let a = U256::from_u128(1);
        let b = U256::from_u128(2);
        assert_eq!(a.checked_sub(b), None);
    }

    // --- U256 multiplication ---
    #[test]
    fn test_u256_checked_mul_small() {
        let a = U256::from_u128(1_000_000);
        let b = U256::from_u128(1_000_000);
        assert_eq!(a.checked_mul(b), Some(U256::from_u128(1_000_000_000_000)));
    }

    #[test]
    fn test_u256_checked_mul_cross() {
        // (2^128) * 2 = 2^129
        let a = U256::new(0, 1); // 2^128
        let b = U256::from_u128(2);
        let c = a.checked_mul(b).unwrap();
        assert_eq!(c.lo(), 0);
        assert_eq!(c.hi(), 2);
    }

    #[test]
    fn test_u256_mul_overflow() {
        let a = U256::new(0, 1); // 2^128
        let b = U256::new(0, 1); // 2^128
                                 // Product would be 2^256, which overflows.
        assert_eq!(a.checked_mul(b), None);
    }

    // --- U256 division ---
    #[test]
    fn test_u256_div_basic() {
        let a = U256::from_u128(1000);
        let b = U256::from_u128(3);
        assert_eq!(a.checked_div(b), Some(U256::from_u128(333)));
        assert_eq!(a.checked_rem(b), Some(U256::from_u128(1)));
    }

    #[test]
    fn test_u256_div_large() {
        // Divide a 256-bit number by a 128-bit number
        let a = U256::new(0, 4); // 4 * 2^128
        let b = U256::from_u128(2);
        let q = a.checked_div(b).unwrap();
        assert_eq!(q, U256::new(0, 2)); // 2 * 2^128
    }

    #[test]
    fn test_u256_div_by_zero() {
        assert_eq!(U256::ONE.checked_div(U256::ZERO), None);
    }

    // --- U256 comparison ---
    #[test]
    fn test_u256_ordering() {
        assert!(U256::ZERO < U256::ONE);
        assert!(U256::ONE < U256::MAX);
        assert!(U256::new(0, 1) > U256::new(u128::MAX, 0));
    }

    // --- U256 shifts ---
    #[test]
    fn test_u256_shl_shr() {
        let a = U256::ONE;
        let b = a.shl(128);
        assert_eq!(b, U256::new(0, 1));
        let c = b.shr(128);
        assert_eq!(c, U256::ONE);

        // Shift by 256 gives zero
        assert_eq!(U256::MAX.shl(256), U256::ZERO);
        assert_eq!(U256::MAX.shr(256), U256::ZERO);
    }

    // --- U256 bitwise ---
    #[test]
    fn test_u256_bitand_bitor() {
        let a = U256::new(0xFF, 0xFF00);
        let b = U256::new(0x0F, 0xFF00);
        assert_eq!(a.bitand(b), U256::new(0x0F, 0xFF00));
        assert_eq!(a.bitor(b), U256::new(0xFF, 0xFF00));
    }

    // --- I256 basic ---
    #[test]
    fn test_i256_zero_one_minusone() {
        assert!(I256::ZERO.is_zero());
        assert!(I256::ONE.is_positive());
        assert!(I256::MINUS_ONE.is_negative());
        assert_eq!(I256::ONE.signum(), 1);
        assert_eq!(I256::ZERO.signum(), 0);
        assert_eq!(I256::MINUS_ONE.signum(), -1);
    }

    #[test]
    fn test_i256_from_i128() {
        let v = I256::from_i128(-42);
        assert!(v.is_negative());
        assert_eq!(v.try_into_i128(), Some(-42));

        let v2 = I256::from_i128(i128::MAX);
        assert_eq!(v2.try_into_i128(), Some(i128::MAX));

        let v3 = I256::from_i128(i128::MIN);
        assert_eq!(v3.try_into_i128(), Some(i128::MIN));
    }

    // --- I256 addition / subtraction ---
    #[test]
    fn test_i256_add() {
        let a = I256::from_i128(100);
        let b = I256::from_i128(-50);
        let c = a.checked_add(b).unwrap();
        assert_eq!(c.try_into_i128(), Some(50));
    }

    #[test]
    fn test_i256_sub() {
        let a = I256::from_i128(10);
        let b = I256::from_i128(20);
        let c = a.checked_sub(b).unwrap();
        assert_eq!(c.try_into_i128(), Some(-10));
    }

    #[test]
    fn test_i256_neg() {
        let a = I256::from_i128(42);
        let b = a.checked_neg().unwrap();
        assert_eq!(b.try_into_i128(), Some(-42));

        // MIN cannot be negated
        assert_eq!(I256::MIN.checked_neg(), None);
    }

    #[test]
    fn test_i256_overflow() {
        // MAX + 1 overflows
        assert_eq!(I256::MAX.checked_add(I256::ONE), None);
        // MIN - 1 overflows
        assert_eq!(I256::MIN.checked_sub(I256::ONE), None);
    }

    #[test]
    fn test_i256_abs_u256() {
        let v = I256::from_i128(-100);
        let a = v.abs_u256();
        assert_eq!(a, U256::from_u128(100));
    }

    #[test]
    fn test_i256_mul_negative_above_i128_magnitude() {
        let a = I256::from_u128(1u128 << 127);
        let b = I256::from_i128(-2);

        let product = a
            .checked_mul_i256(b)
            .expect("-2^128 fits in signed 256-bit range");

        assert!(product.is_negative());
        assert_eq!(product.try_into_i128(), None);
        assert_eq!(product.abs_u256(), U256::new(0, 1));
    }

    // --- I256 comparison ---
    #[test]
    fn test_i256_ordering() {
        assert!(I256::MIN < I256::MINUS_ONE);
        assert!(I256::MINUS_ONE < I256::ZERO);
        assert!(I256::ZERO < I256::ONE);
        assert!(I256::ONE < I256::MAX);
    }

    // --- floor_div_signed_conservative ---
    #[test]
    fn test_floor_div_positive() {
        // 7 / 3 = 2 (floor = trunc for positive)
        let n = I256::from_i128(7);
        let d = U256::from_u128(3);
        let q = floor_div_signed_conservative(n, d);
        assert_eq!(q.try_into_i128(), Some(2));
    }

    #[test]
    fn test_floor_div_negative_exact() {
        // -6 / 3 = -2 (exact, no rounding)
        let n = I256::from_i128(-6);
        let d = U256::from_u128(3);
        let q = floor_div_signed_conservative(n, d);
        assert_eq!(q.try_into_i128(), Some(-2));
    }

    #[test]
    fn test_floor_div_negative_remainder() {
        // -7 / 3: trunc = -2, remainder = 1, floor = -3
        let n = I256::from_i128(-7);
        let d = U256::from_u128(3);
        let q = floor_div_signed_conservative(n, d);
        assert_eq!(q.try_into_i128(), Some(-3));
    }

    // --- mul_div_floor / mul_div_ceil ---
    #[test]
    fn test_mul_div_floor() {
        // 10 * 20 / 3 = 200 / 3 = 66
        let a = U256::from_u128(10);
        let b = U256::from_u128(20);
        let d = U256::from_u128(3);
        assert_eq!(mul_div_floor_u256(a, b, d), U256::from_u128(66));
    }

    #[test]
    fn test_mul_div_ceil() {
        // 10 * 20 / 3 = 200 / 3 = ceil(66.67) = 67
        let a = U256::from_u128(10);
        let b = U256::from_u128(20);
        let d = U256::from_u128(3);
        assert_eq!(mul_div_ceil_u256(a, b, d), U256::from_u128(67));
    }

    #[test]
    fn test_mul_div_large() {
        // Test with values that would overflow U256 in intermediate:
        // (2^200) * (2^200) / (2^200) = 2^200
        let big = U256::ONE.shl(200);
        assert_eq!(mul_div_floor_u256(big, big, big), big);
    }

    #[test]
    fn test_mul_div_exact() {
        // 6 * 7 / 42 = 1 exactly
        let a = U256::from_u128(6);
        let b = U256::from_u128(7);
        let d = U256::from_u128(42);
        assert_eq!(mul_div_floor_u256(a, b, d), U256::ONE);
        assert_eq!(mul_div_ceil_u256(a, b, d), U256::ONE);
    }

    // --- ceil_div_positive_checked ---
    #[test]
    fn test_ceil_div_positive() {
        // ceil(7 / 3) = 3
        assert_eq!(
            ceil_div_positive_checked(U256::from_u128(7), U256::from_u128(3)),
            U256::from_u128(3)
        );
        // ceil(6 / 3) = 2
        assert_eq!(
            ceil_div_positive_checked(U256::from_u128(6), U256::from_u128(3)),
            U256::from_u128(2)
        );
    }

    // --- saturating_mul_u256_u64 ---
    #[test]
    fn test_saturating_mul_u256_u64() {
        let a = U256::from_u128(100);
        assert_eq!(saturating_mul_u256_u64(a, 5), U256::from_u128(500));
        // Saturates on overflow
        assert_eq!(saturating_mul_u256_u64(U256::MAX, 2), U256::MAX);
    }

    // --- fee_debt_u128_checked ---
    #[test]
    fn test_fee_debt() {
        assert_eq!(fee_debt_u128_checked(-100), 100);
        assert_eq!(fee_debt_u128_checked(100), 0);
        assert_eq!(fee_debt_u128_checked(0), 0);
        assert_eq!(fee_debt_u128_checked(i128::MIN), 1u128 << 127);
    }

    // --- wide_signed_mul_div_floor ---
    #[test]
    fn test_wide_signed_mul_div_floor_positive() {
        // 10 * 20 / 3 = floor(200/3) = 66
        let abs_basis = U256::from_u128(10);
        let k_diff = I256::from_i128(20);
        let denom = U256::from_u128(3);
        let result = wide_signed_mul_div_floor(abs_basis, k_diff, denom);
        assert_eq!(result.try_into_i128(), Some(66));
    }

    #[test]
    fn test_wide_signed_mul_div_floor_negative() {
        // 10 * (-7) / 3 = floor(-70/3) = floor(-23.33) = -24
        let abs_basis = U256::from_u128(10);
        let k_diff = I256::from_i128(-7);
        let denom = U256::from_u128(3);
        let result = wide_signed_mul_div_floor(abs_basis, k_diff, denom);
        assert_eq!(result.try_into_i128(), Some(-24));
    }

    #[test]
    fn test_wide_signed_mul_div_floor_exact_negative() {
        // 10 * (-6) / 3 = floor(-60/3) = -20 (exact)
        let abs_basis = U256::from_u128(10);
        let k_diff = I256::from_i128(-6);
        let denom = U256::from_u128(3);
        let result = wide_signed_mul_div_floor(abs_basis, k_diff, denom);
        assert_eq!(result.try_into_i128(), Some(-20));
    }

    #[test]
    fn test_wide_signed_mul_div_floor_zero() {
        let abs_basis = U256::from_u128(10);
        let k_diff = I256::ZERO;
        let denom = U256::from_u128(3);
        let result = wide_signed_mul_div_floor(abs_basis, k_diff, denom);
        assert_eq!(result, I256::ZERO);
    }

    // --- U256 operator traits ---
    #[test]
    #[should_panic(expected = "U256 add overflow")]
    fn test_u256_add_op_panic() {
        let _ = U256::MAX + U256::ONE;
    }

    #[test]
    #[should_panic(expected = "U256 sub underflow")]
    fn test_u256_sub_op_panic() {
        let _ = U256::ZERO - U256::ONE;
    }

    #[test]
    fn test_u256_overflowing() {
        let (val, overflow) = U256::MAX.overflowing_add(U256::ONE);
        assert!(overflow);
        assert_eq!(val, U256::ZERO);

        let (val, underflow) = U256::ZERO.overflowing_sub(U256::ONE);
        assert!(underflow);
        assert_eq!(val, U256::MAX);
    }

    // --- I256 from_u128 (positive only) ---
    #[test]
    fn test_i256_from_u128() {
        let v = I256::from_u128(100);
        assert!(v.is_positive());
        assert_eq!(v.try_into_i128(), Some(100));
    }

    // --- I256 sub involving MIN ---
    #[test]
    fn test_i256_sub_min() {
        // 0 - MIN overflows (since |MIN| > MAX)
        assert_eq!(I256::ZERO.checked_sub(I256::MIN), None);
    }

    // --- AddAssign / SubAssign ---
    #[test]
    fn test_u256_assign_ops() {
        let mut v = U256::from_u128(10);
        v += U256::from_u128(5);
        assert_eq!(v, U256::from_u128(15));
        v -= U256::from_u128(3);
        assert_eq!(v, U256::from_u128(12));
    }

    // --- I256 saturating_add ---
    #[test]
    fn test_i256_saturating_add() {
        assert_eq!(I256::MAX.saturating_add(I256::ONE), I256::MAX);
        assert_eq!(I256::MIN.saturating_add(I256::MINUS_ONE), I256::MIN);
    }

    // --- U256 widening mul u128 test ---
    #[test]
    fn test_widening_mul_u128() {
        let (lo, hi) = widening_mul_u128(u128::MAX, u128::MAX);
        // (2^128-1)^2 = 2^256 - 2^129 + 1
        // lo = 1, hi = 2^128 - 2 = u128::MAX - 1
        assert_eq!(lo, 1);
        assert_eq!(hi, u128::MAX - 1);
    }

    // --- Large mul_div test ---
    #[test]
    fn test_mul_div_max() {
        // MAX * 1 / 1 = MAX
        assert_eq!(
            mul_div_floor_u256(U256::MAX, U256::ONE, U256::ONE),
            U256::MAX
        );
        // 1 * 1 / 1 = 1
        assert_eq!(
            mul_div_floor_u256(U256::ONE, U256::ONE, U256::ONE),
            U256::ONE
        );
    }
}
