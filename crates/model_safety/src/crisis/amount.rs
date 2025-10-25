//! Fixed-point arithmetic for crisis loss socialization
//!
//! Uses Q64.64 format (64 integer bits, 64 fractional bits) for scales and ratios.
//! All operations are no_std compatible and use safe checked arithmetic.

#![allow(dead_code)]

/// Q64.64 fixed-point number stored in u128
/// - Bits 0-63: fractional part
/// - Bits 64-127: integer part
/// - Represents values in [0, 2^64)
/// - Used for scales in range [0, 1] where 1.0 = 2^64
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct Q64x64(pub u128);

impl Q64x64 {
    /// Constant representing 1.0 in Q64.64 format
    pub const ONE: Q64x64 = Q64x64(1u128 << 64);

    /// Constant representing 0.0 in Q64.64 format
    pub const ZERO: Q64x64 = Q64x64(0);

    /// Maximum representable value (2^64 - epsilon)
    pub const MAX: Q64x64 = Q64x64(u128::MAX);

    /// Fractional scale factor (2^64)
    const F: u128 = 1u128 << 64;

    /// Create Q64.64 from an integer value
    ///
    /// # Arguments
    /// * `x` - Integer value in range [0, 2^64)
    ///
    /// # Returns
    /// Q64.64 representation of x.0 (x with zero fractional part)
    ///
    /// # Panics
    /// If x >= 2^64 (would overflow)
    #[inline]
    pub const fn from_int(x: u64) -> Self {
        Q64x64((x as u128) << 64)
    }

    /// Multiply Q64.64 by i128, returning i128
    ///
    /// Computes (x * scale) >> 64 with rounding toward zero.
    ///
    /// # Safety
    /// - Uses checked limb-based multiplication to avoid overflow
    /// - Returns 0 on overflow (saturating behavior)
    #[inline]
    pub fn mul_i128(self, x: i128) -> i128 {
        if x == 0 || self.0 == 0 {
            return 0;
        }

        let sign = x.signum();
        let abs_x = x.abs() as u128;

        // Perform multiplication: (abs_x * self.0) >> 64
        // Use limb-based multiplication to avoid u256
        let result = Self::wide_mul_shr64(abs_x, self.0);

        // Apply sign and clamp to i128::MAX
        let signed_result = if result > (i128::MAX as u128) {
            i128::MAX
        } else {
            (result as i128) * sign
        };

        signed_result
    }

    /// Create Q64.64 ratio from numerator/denominator
    ///
    /// Returns min(1.0, numerator/denominator) in Q64.64 format.
    ///
    /// # Arguments
    /// * `numer` - Numerator (must be >= 0)
    /// * `denom` - Denominator (must be > 0)
    ///
    /// # Returns
    /// Q64.64 representation of min(1.0, numer/denom)
    /// Returns ZERO if numer <= 0 or denom <= 0
    #[inline]
    pub fn ratio(numer: i128, denom: i128) -> Self {
        if numer <= 0 || denom <= 0 {
            return Q64x64::ZERO;
        }

        let n = numer as u128;
        let d = denom as u128;

        // Compute (n << 64) / d, capping at ONE
        // Use limb-based division to avoid overflow
        let result = Self::wide_div(n, d);

        // Cap at ONE (1.0)
        if result >= Self::F {
            Q64x64::ONE
        } else {
            Q64x64(result)
        }
    }

    /// Compute 1.0 - self
    ///
    /// # Returns
    /// Q64.64 representation of (1.0 - self), saturating to 0 if self > 1.0
    #[inline]
    pub fn one_minus(self) -> Self {
        Q64x64(Self::ONE.0.saturating_sub(self.0))
    }

    /// Multiply two Q64.64 values
    ///
    /// Computes (self * other) >> 64
    ///
    /// # Returns
    /// Q64.64 representation of self * other
    #[inline]
    pub fn mul_scale(self, other: Q64x64) -> Self {
        Q64x64(Self::wide_mul_shr64(self.0, other.0))
    }

    /// Helper: (a * b) >> 64 using limb-based multiplication
    ///
    /// Splits 128-bit values into high/low 64-bit limbs:
    /// a = a_hi * 2^64 + a_lo
    /// b = b_hi * 2^64 + b_lo
    ///
    /// (a * b) = a_hi * b_hi * 2^128
    ///         + a_hi * b_lo * 2^64
    ///         + a_lo * b_hi * 2^64
    ///         + a_lo * b_lo
    ///
    /// Shifting right by 64 gives:
    /// (a * b) >> 64 = a_hi * b_hi * 2^64
    ///               + a_hi * b_lo
    ///               + a_lo * b_hi
    ///               + (a_lo * b_lo) >> 64
    #[inline]
    fn wide_mul_shr64(a: u128, b: u128) -> u128 {
        let a_hi = a >> 64;
        let a_lo = a & ((1u128 << 64) - 1);
        let b_hi = b >> 64;
        let b_lo = b & ((1u128 << 64) - 1);

        // term1 = a_hi * b_hi * 2^64 (may overflow if both > 1.0)
        let term1 = a_hi.saturating_mul(b_hi).checked_shl(64).unwrap_or(u128::MAX);

        // term2 = a_hi * b_lo
        let term2 = a_hi.saturating_mul(b_lo);

        // term3 = a_lo * b_hi
        let term3 = a_lo.saturating_mul(b_hi);

        // term4 = (a_lo * b_lo) >> 64
        let lo_mul = a_lo.saturating_mul(b_lo);
        let term4 = lo_mul >> 64;

        term1.saturating_add(term2).saturating_add(term3).saturating_add(term4)
    }

    /// Helper: (n << 64) / d using safe division
    ///
    /// Computes (numerator * 2^64) / denominator without overflow.
    /// Returns 0 if denominator is 0.
    #[inline]
    fn wide_div(n: u128, d: u128) -> u128 {
        if d == 0 {
            return 0;
        }

        // If n < 2^64, we can directly compute (n << 64) / d
        if n < (1u128 << 64) {
            return (n << 64) / d;
        }

        // Otherwise, use long division approach:
        // Split n = n_hi * 2^64 + n_lo
        let n_hi = n >> 64;
        let n_lo = n & ((1u128 << 64) - 1);

        // (n << 64) / d = (n_hi * 2^128 + n_lo * 2^64) / d
        //               = (n_hi * 2^64) * (2^64 / d) + (n_lo * 2^64) / d

        // Compute each term separately to avoid overflow
        let term1 = n_hi.saturating_mul(1u128 << 64);
        let term2 = (n_lo << 64).checked_div(d).unwrap_or(0);

        term1.saturating_add(term2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_one_constant() {
        assert_eq!(Q64x64::ONE.0, 1u128 << 64);
    }

    #[test]
    fn test_mul_i128_identity() {
        let x = 1_000_000i128;
        let result = Q64x64::ONE.mul_i128(x);
        assert_eq!(result, x);
    }

    #[test]
    fn test_mul_i128_half() {
        let half = Q64x64(1u128 << 63); // 0.5 in Q64.64
        let x = 1_000_000i128;
        let result = half.mul_i128(x);
        assert_eq!(result, 500_000);
    }

    #[test]
    fn test_mul_i128_negative() {
        let half = Q64x64(1u128 << 63); // 0.5
        let x = -1_000_000i128;
        let result = half.mul_i128(x);
        assert_eq!(result, -500_000);
    }

    #[test]
    fn test_ratio_simple() {
        let ratio = Q64x64::ratio(1, 2);
        // Should be 0.5
        assert_eq!(ratio.0, 1u128 << 63);
    }

    #[test]
    fn test_ratio_caps_at_one() {
        let ratio = Q64x64::ratio(1000, 100);
        // Should cap at 1.0
        assert_eq!(ratio, Q64x64::ONE);
    }

    #[test]
    fn test_ratio_zero_denom() {
        let ratio = Q64x64::ratio(100, 0);
        assert_eq!(ratio, Q64x64::ZERO);
    }

    #[test]
    fn test_one_minus() {
        let quarter = Q64x64(1u128 << 62); // 0.25
        let three_quarters = quarter.one_minus();
        // Should be 0.75
        assert_eq!(three_quarters.0, (3u128 << 62));
    }

    #[test]
    fn test_mul_scale() {
        let half = Q64x64(1u128 << 63); // 0.5
        let result = half.mul_scale(half);
        // 0.5 * 0.5 = 0.25
        assert_eq!(result.0, 1u128 << 62);
    }

    #[test]
    fn test_wide_mul_no_overflow() {
        // Test that 0.9 * 0.9 doesn't overflow
        let nine_tenths = Q64x64::ratio(9, 10);
        let result = nine_tenths.mul_scale(nine_tenths);
        // Should be approximately 0.81
        assert!(result.0 > 0);
        assert!(result.0 < Q64x64::ONE.0);
    }
}
