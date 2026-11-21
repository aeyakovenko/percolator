// Differential conformance for src/wide_math.rs.
//
// Covers what the README describes as "checked by differential/property
// tests": the 256/512-bit core below the existing u128 wrapper coverage,
// plus parity between the #[cfg(kani)] and #[cfg(not(kani))] U256/I256
// implementations. Fixtures are generated from the source by
// scripts/gen_wide_math_dual_impl.sh; a sync test below fails on drift.
// Skipped entirely under Kani.
#![cfg(not(kani))]

#[path = "fixtures_wide_math_kani_impl.rs"]
mod wide_math_kani_impl;

#[path = "fixtures_wide_math_host_impl.rs"]
mod wide_math_host_impl;

use wide_math_host_impl::{
    ceil_div_positive_checked, floor_div_signed_conservative,
    floor_div_signed_conservative_i128, mul_div_ceil_u256, mul_div_floor_u256_with_rem,
    wide_signed_mul_div_floor, I256 as IP, U256 as UP,
};
use wide_math_kani_impl::{I256 as IK, U256 as UK};

// Fixture sync (drift check)

#[test]
fn dual_impl_fixtures_in_sync_with_source() {
    let src = include_str!("../src/wide_math.rs");
    let kani_side = src
        .replace("#[cfg(kani)]", "#[cfg(all())]")
        .replace("#[cfg(not(kani))]", "#[cfg(any())]");
    let host_side = src
        .replace("#[cfg(kani)]", "#[cfg(any())]")
        .replace("#[cfg(not(kani))]", "#[cfg(all())]");
    let committed_kani = include_str!("fixtures_wide_math_kani_impl.rs");
    let committed_host = include_str!("fixtures_wide_math_host_impl.rs");
    assert_eq!(
        kani_side, committed_kani,
        "tests/fixtures_wide_math_kani_impl.rs drifted from src/wide_math.rs; regenerate with scripts/gen_wide_math_dual_impl.sh"
    );
    assert_eq!(
        host_side, committed_host,
        "tests/fixtures_wide_math_host_impl.rs drifted from src/wide_math.rs; regenerate with scripts/gen_wide_math_dual_impl.sh"
    );
}

// Edge-case differential: kani-cfg vs host-cfg implementations

const EDGE128: [u128; 12] = [
    0, 1, 2, 3, 63, 64, 127, 128, (1u128 << 64) - 1, 1u128 << 64, u128::MAX - 1, u128::MAX,
];

fn edge_pairs() -> Vec<(u128, u128)> {
    let mut v = Vec::new();
    for &lo in &EDGE128 {
        for &hi in &EDGE128 {
            v.push((lo, hi));
        }
    }
    v
}

fn to_k(p: (u128, u128)) -> UK {
    UK::new(p.0, p.1)
}
fn to_p(p: (u128, u128)) -> UP {
    UP::new(p.0, p.1)
}

#[test]
fn dual_impl_u256_add_sub_mul_div_eq() {
    let inputs = edge_pairs();
    for a in &inputs {
        for b in &inputs {
            let (ak, bk) = (to_k(*a), to_k(*b));
            let (ap, bp) = (to_p(*a), to_p(*b));
            assert_eq!(
                ak.checked_add(bk).map(|v| (v.lo(), v.hi())),
                ap.checked_add(bp).map(|v| (v.lo(), v.hi())),
                "checked_add divergence a={:?} b={:?}",
                a,
                b
            );
            assert_eq!(
                ak.checked_sub(bk).map(|v| (v.lo(), v.hi())),
                ap.checked_sub(bp).map(|v| (v.lo(), v.hi())),
                "checked_sub divergence a={:?} b={:?}",
                a,
                b
            );
            assert_eq!(
                ak.checked_mul(bk).map(|v| (v.lo(), v.hi())),
                ap.checked_mul(bp).map(|v| (v.lo(), v.hi())),
                "checked_mul divergence a={:?} b={:?}",
                a,
                b
            );
            if bk.is_zero() {
                assert!(ak.checked_div(bk).is_none() && ap.checked_div(bp).is_none());
            } else {
                assert_eq!(
                    ak.checked_div(bk).map(|v| (v.lo(), v.hi())),
                    ap.checked_div(bp).map(|v| (v.lo(), v.hi())),
                    "checked_div divergence a={:?} b={:?}",
                    a,
                    b
                );
                assert_eq!(
                    ak.checked_rem(bk).map(|v| (v.lo(), v.hi())),
                    ap.checked_rem(bp).map(|v| (v.lo(), v.hi())),
                    "checked_rem divergence a={:?} b={:?}",
                    a,
                    b
                );
            }
            assert_eq!(ak.cmp(&bk), ap.cmp(&bp));
        }
    }
}

#[test]
fn dual_impl_u256_shifts_eq() {
    let inputs = edge_pairs();
    let shifts: [u32; 10] = [0, 1, 63, 64, 127, 128, 129, 200, 255, 256];
    for a in &inputs {
        for &s in &shifts {
            assert_eq!(
                to_k(*a).shl(s).lo(),
                to_p(*a).shl(s).lo(),
                "shl {} divergence at {:?}",
                s,
                a
            );
            assert_eq!(to_k(*a).shl(s).hi(), to_p(*a).shl(s).hi());
            assert_eq!(to_k(*a).shr(s).lo(), to_p(*a).shr(s).lo());
            assert_eq!(to_k(*a).shr(s).hi(), to_p(*a).shr(s).hi());
        }
    }
}

fn ik(p: (u128, u128)) -> IK {
    IK::from_raw_u256(to_k(p))
}
fn ip(p: (u128, u128)) -> IP {
    IP::from_raw_u256(to_p(p))
}

fn sig_k(v: &IK) -> (bool, bool, (u128, u128)) {
    (v.is_zero(), v.is_negative(), (v.abs_u256().lo(), v.abs_u256().hi()))
}
fn sig_p(v: &IP) -> (bool, bool, (u128, u128)) {
    (v.is_zero(), v.is_negative(), (v.abs_u256().lo(), v.abs_u256().hi()))
}

#[test]
fn dual_impl_i256_add_sub_neg_eq() {
    let inputs = edge_pairs();
    for a in &inputs {
        for b in &inputs {
            let (ak, bk) = (ik(*a), ik(*b));
            let (ap, bp) = (ip(*a), ip(*b));
            assert_eq!(
                ak.checked_add(bk).as_ref().map(sig_k),
                ap.checked_add(bp).as_ref().map(sig_p),
                "I256 add divergence a={:?} b={:?}",
                a,
                b
            );
            assert_eq!(sig_k(&ak.saturating_add(bk)), sig_p(&ap.saturating_add(bp)));
            assert_eq!(
                ak.checked_sub(bk).as_ref().map(sig_k),
                ap.checked_sub(bp).as_ref().map(sig_p),
                "I256 sub divergence a={:?} b={:?}",
                a,
                b
            );
            assert_eq!(
                ak.checked_neg().as_ref().map(sig_k),
                ap.checked_neg().as_ref().map(sig_p),
                "I256 neg divergence a={:?}",
                a
            );
            assert_eq!(ak.cmp(&bk), ap.cmp(&bp));
            assert_eq!(ak.try_into_i128(), ap.try_into_i128());
            assert_eq!(ak.signum(), ap.signum());
        }
    }
}

/// Exact 512-bit reference product (4 u128 limbs, little-endian).
fn mul_u256_ref(alo: u128, ahi: u128, blo: u128, bhi: u128) -> [u128; 4] {
    let mul_wide = |a: u128, b: u128| -> (u128, u128) {
        let a_lo = a as u64 as u128;
        let a_hi = (a >> 64) as u64 as u128;
        let b_lo = b as u64 as u128;
        let b_hi = (b >> 64) as u64 as u128;
        let ll = a_lo * b_lo;
        let lh = a_lo * b_hi;
        let hl = a_hi * b_lo;
        let hh = a_hi * b_hi;
        let (mid, mid_carry) = lh.overflowing_add(hl);
        let (lo, lo_carry) = ll.overflowing_add(mid << 64);
        let hi = hh + (mid >> 64) + ((mid_carry as u128) << 64) + (lo_carry as u128);
        (lo, hi)
    };
    let (r0, c0) = mul_wide(alo, blo);
    let (x1, x2) = mul_wide(alo, bhi);
    let (y1, y2) = mul_wide(ahi, blo);
    let (z2, z3) = mul_wide(ahi, bhi);
    let (t, c1) = c0.overflowing_add(x1);
    let (r1, c2) = t.overflowing_add(y1);
    let carry1 = (c1 as u128) + (c2 as u128);
    let (t, c3) = x2.overflowing_add(y2);
    let (t, c4) = t.overflowing_add(z2);
    let (r2, c5) = t.overflowing_add(carry1);
    let carry2 = (c3 as u128) + (c4 as u128) + (c5 as u128);
    let r3 = z3.wrapping_add(carry2); // cannot overflow for a valid 512-bit product
    [r0, r1, r2, r3]
}

/// Signed I256 multiply reference: classification + exact magnitude.
fn i256_mul_ref(a: (u128, u128), b: (u128, u128)) -> Option<((bool, (u128, u128)), bool)> {
    // returns Some(((negative, magnitude_bits), is_min)) or None (overflow)
    let abs_ref = |lo: u128, hi: u128| -> (u128, u128) {
        if hi >> 127 != 0 {
            let (nlo, c) = (!lo).overflowing_add(1);
            let nhi = (!hi).wrapping_add(c as u128);
            (nlo, nhi)
        } else {
            (lo, hi)
        }
    };
    let neg_a = a.1 >> 127 != 0;
    let neg_b = b.1 >> 127 != 0;
    let (a0, a1) = abs_ref(a.0, a.1);
    let (b0, b1) = abs_ref(b.0, b.1);
    let p = mul_u256_ref(a0, a1, b0, b1);
    // I256::MAX = 2^255-1 -> 4-limb [u128::MAX, 2^127-1, 0, 0]; |MIN| = 2^255.
    let is_zero = p == [0, 0, 0, 0];
    let eq_2_255 = p[0] == 0 && p[1] == 1u128 << 127 && p[2] == 0 && p[3] == 0;
    let fits_max = p[3] == 0 && p[2] == 0 && (p[1] >> 127) == 0;
    let neg = neg_a != neg_b;
    if is_zero {
        return Some(((false, (0, 0)), false));
    }
    if neg {
        if eq_2_255 {
            return Some(((true, (0, 1u128 << 127)), true));
        }
        if fits_max {
            return Some(((true, (p[0], p[1])), false));
        }
        None
    } else if fits_max {
        Some(((false, (p[0], p[1])), false))
    } else {
        None
    }
}

#[test]
fn dual_impl_i256_mul_exact_reference() {
    let inputs = edge_pairs();
    for a in &inputs {
        for b in &inputs {
            let expected = i256_mul_ref(*a, *b);
            let (km, pm) = (ik(*a).checked_mul_i256(ik(*b)), ip(*a).checked_mul_i256(ip(*b)));
            assert_eq!(
                km.is_some(),
                expected.is_some(),
                "I256 mul overflow classification divergence a={:?} b={:?}",
                a,
                b
            );
            assert_eq!(
                pm.is_some(),
                expected.is_some(),
                "I256 mul overflow classification divergence (host) a={:?} b={:?}",
                a,
                b
            );
            if let (Some(vk), Some(vp), Some(((neg, mag), is_min))) = (km, pm, expected) {
                assert_eq!(vk.is_negative(), vp.is_negative());
                assert_eq!(vk.is_negative(), neg);
                if is_min {
                    assert_eq!(vk, IK::MIN);
                    assert_eq!(vp, IP::MIN);
                } else {
                    assert_eq!((vk.abs_u256().lo(), vk.abs_u256().hi()), mag);
                    assert_eq!((vp.abs_u256().lo(), vp.abs_u256().hi()), mag);
                }
            }
        }
    }
}

// Randomized differential vs an independent 512-bit reference

struct Lcg(u64);
impl Lcg {
    fn next_u64(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0
    }
    fn next_u128(&mut self) -> u128 {
        ((self.next_u64() as u128) << 64) | self.next_u64() as u128
    }
    /// Biased toward small values, word boundaries, powers of two, MAX-n.
    fn next_biased(&mut self) -> u128 {
        match self.next_u64() % 8 {
            0 => self.next_u128() % 1024,
            1 => self.next_u128() % (1 << 32),
            2 => 1u128 << (self.next_u64() % 128),
            3 => (1u128 << (self.next_u64() % 128)).wrapping_sub(1),
            4 => u128::MAX - self.next_u128() % 1024,
            5 => self.next_u128() >> (self.next_u64() % 128),
            6 => (1u128 << 127) | (self.next_u128() & 0xFF),
            _ => self.next_u128(),
        }
    }
}

type W512 = [u128; 4];

/// Bit-by-bit restoring long division: 512-bit / 256-bit.
/// Returns Some((quotient 4 limbs, remainder (lo, hi))) or None when the
/// quotient exceeds 256 bits.
fn div_512_by_256_ref(p: W512, dlo: u128, dhi: u128) -> Option<(W512, (u128, u128))> {
    assert!(!(dlo == 0 && dhi == 0));
    let mut rem = [0u128; 4];
    let mut quo = [0u128; 4];
    for bit in (0..512).rev() {
        let mut carry = (p[bit / 128] >> (bit % 128)) & 1;
        for limb in rem.iter_mut() {
            let next = *limb >> 127;
            *limb = (*limb << 1) | carry;
            carry = next;
        }
        let ge = if rem[3] != 0 || rem[2] != 0 {
            std::cmp::Ordering::Greater
        } else {
            rem[1].cmp(&dhi).then(rem[0].cmp(&dlo))
        };
        if ge != std::cmp::Ordering::Less {
            // rem -= d with full ripple borrow
            let (r0, b0) = rem[0].overflowing_sub(dlo);
            let (t, b1a) = rem[1].overflowing_sub(dhi);
            let (r1, b1b) = t.overflowing_sub(b0 as u128);
            let mut borrow = b1a || b1b;
            rem[0] = r0;
            rem[1] = r1;
            let mut i = 2;
            while borrow && i < 4 {
                let (r, b) = rem[i].overflowing_sub(1);
                rem[i] = r;
                borrow = b;
                i += 1;
            }
            debug_assert!(!borrow);
            quo[bit / 128] |= 1u128 << (bit % 128);
        }
    }
    if quo[2] == 0 && quo[3] == 0 && rem[2] == 0 && rem[3] == 0 {
        Some((quo, (rem[0], rem[1])))
    } else {
        None
    }
}

fn up_from(p: (u128, u128)) -> UP {
    UP::new(p.0, p.1)
}

#[test]
fn random_mul_div_floor_ceil_512bit_conformance() {
    let mut rng = Lcg(0x0ddba11_cafe_f00d);
    for i in 0..200_000 {
        let a = (rng.next_biased(), rng.next_biased());
        let b = (rng.next_biased(), rng.next_biased());
        let d = (rng.next_biased(), rng.next_biased());
        if d == (0, 0) {
            continue;
        }
        let prod = mul_u256_ref(a.0, a.1, b.0, b.1);
        let reference = div_512_by_256_ref(prod, d.0, d.1);
        match reference {
            Some((q, r)) => {
                let floor = mul_div_floor_u256_with_rem(up_from(a), up_from(b), up_from(d));
                assert_eq!(
                    (floor.0.lo(), floor.0.hi()),
                    (q[0], q[1]),
                    "floor q mismatch i={} a={:?} b={:?} d={:?}",
                    i,
                    a,
                    b,
                    d
                );
                assert_eq!(
                    (floor.1.lo(), floor.1.hi()),
                    r,
                    "rem mismatch i={} a={:?} b={:?} d={:?}",
                    i,
                    a,
                    b,
                    d
                );
                // identity: q*d + r == a*b (512-bit reconstruction)
                let qd = mul_u256_ref(q[0], q[1], d.0, d.1);
                let (s0, c0) = qd[0].overflowing_add(r.0);
                let (s1, c1) = qd[1].overflowing_add(r.1);
                let (s1, c2) = s1.overflowing_add(c0 as u128);
                let mut c3 = c1 || c2;
                let mut s2 = qd[2];
                if c3 {
                    let (v, b) = s2.overflowing_add(1);
                    s2 = v;
                    c3 = b;
                }
                let mut s3 = qd[3];
                if c3 {
                    let (v, b) = s3.overflowing_add(1);
                    s3 = v;
                    c3 = b;
                }
                assert!(!c3);
                assert_eq!(
                    [s0, s1, s2, s3],
                    prod,
                    "q*d+r != a*b i={} a={:?} b={:?} d={:?}",
                    i,
                    a,
                    b,
                    d
                );
                assert!(
                    (r.1 < d.1) || (r.1 == d.1 && r.0 < d.0),
                    "r >= d i={} d={:?}",
                    i,
                    d
                );
                // ceil = q + (r != 0)
                let ceil = mul_div_ceil_u256(up_from(a), up_from(b), up_from(d));
                let exp_ceil = if r == (0, 0) {
                    (q[0], q[1])
                } else {
                    let (lo, c) = q[0].overflowing_add(1);
                    (lo, q[1] + c as u128)
                };
                assert_eq!(
                    (ceil.lo(), ceil.hi()),
                    exp_ceil,
                    "ceil mismatch i={} a={:?} b={:?} d={:?}",
                    i,
                    a,
                    b,
                    d
                );
            }
            None => {
                // Quotient exceeds 256 bits: production helper must panic.
                let result = std::panic::catch_unwind(|| {
                    mul_div_floor_u256_with_rem(up_from(a), up_from(b), up_from(d))
                });
                assert!(
                    result.is_err(),
                    "expected panic for >256-bit quotient i={} a={:?} b={:?} d={:?}",
                    i,
                    a,
                    b,
                    d
                );
            }
        }
    }
}

#[test]
fn random_ceil_div_positive_conformance() {
    let mut rng = Lcg(0xabc_def0_1111_2222);
    for i in 0..50_000 {
        let n = (rng.next_biased(), rng.next_biased());
        let d = (rng.next_biased(), rng.next_biased());
        if d == (0, 0) {
            continue;
        }
        let prod = mul_u256_ref(n.0, n.1, 1, 0); // n * 1
        let reference = div_512_by_256_ref(prod, d.0, d.1);
        match reference {
            Some((q, r)) => {
                let got = ceil_div_positive_checked(up_from(n), up_from(d));
                let exp = if r == (0, 0) {
                    (q[0], q[1])
                } else {
                    let (lo, c) = q[0].overflowing_add(1);
                    (lo, q[1] + c as u128)
                };
                assert_eq!(
                    (got.lo(), got.hi()),
                    exp,
                    "ceil_div i={} n={:?} d={:?}",
                    i,
                    n,
                    d
                );
            }
            None => {
                let result =
                    std::panic::catch_unwind(|| ceil_div_positive_checked(up_from(n), up_from(d)));
                assert!(
                    result.is_err(),
                    "ceil_div expected panic i={} n={:?} d={:?}",
                    i,
                    n,
                    d
                );
            }
        }
    }
}

#[test]
fn random_floor_div_signed_conservative_conformance() {
    let mut rng = Lcg(0x5eed_1234_5678_9abc);
    for i in 0..100_000 {
        let n_raw = (rng.next_biased(), rng.next_biased());
        let nk = IP::from_raw_u256(up_from(n_raw));
        let nk_val = match nk.try_into_i128() {
            Some(v) => v,
            None => continue,
        };
        let d = (rng.next_biased() % (1 << 64)) + 1;
        let got = floor_div_signed_conservative(nk, up_from((d, 0)));
        let (q, r) = (nk_val.unsigned_abs() / d, nk_val.unsigned_abs() % d);
        let expected: i128 = if nk_val >= 0 {
            (nk_val as u128 / d) as i128
        } else if r == 0 {
            (nk_val.unsigned_abs() / d) as i128 * -1
        } else {
            -((q + 1) as i128)
        };
        let got_val = got
            .try_into_i128()
            .expect("floor_div result not i128-representable in test range");
        assert_eq!(got_val, expected, "floor_div i={} n={} d={}", i, nk_val, d);
    }
}

#[test]
fn random_floor_div_i128_helper_conformance() {
    let mut rng = Lcg(0xfeed_beef_1234_5678);
    for i in 0..100_000 {
        let n = rng.next_u64() as i128 * if rng.next_u64() % 2 == 0 { 1 } else { -1 };
        let d = ((rng.next_u64() % (1 << 63)) + 1) as u128;
        let got = floor_div_signed_conservative_i128(n, d);
        let (q, r) = (n.unsigned_abs() / d, n.unsigned_abs() % d);
        let expected: i128 = if n >= 0 {
            (n as u128 / d) as i128
        } else if r == 0 {
            -((n.unsigned_abs() / d) as i128)
        } else {
            -((q + 1) as i128)
        };
        assert_eq!(got, expected, "floor_div_i128 i={} n={} d={}", i, n, d);
    }
}

#[test]
fn random_wide_signed_mul_div_floor_conformance() {
    let mut rng = Lcg(0x1337_c0de_dead_10cc);
    for i in 0..100_000 {
        let abs_basis = rng.next_biased() % (1 << 96);
        let k_val: i128 = (rng.next_biased() % (1 << 96)) as i128
            * if rng.next_u64() % 2 == 0 { 1 } else { -1 };
        let d = (rng.next_biased() % (1 << 64)) + 1;
        let k = IP::from_i128(k_val);
        let got = wide_signed_mul_div_floor(up_from((abs_basis, 0)), k, up_from((d, 0)));
        // reference: floor(abs_basis * k / d) via 128x128 -> 256 exact product
        let (plo, phi) = {
            let a_lo = k_val.unsigned_abs() as u64 as u128;
            let a_hi = (k_val.unsigned_abs() >> 64) as u64 as u128;
            let b_lo = abs_basis as u64 as u128;
            let b_hi = (abs_basis >> 64) as u64 as u128;
            let ll = a_lo * b_lo;
            let lh = a_lo * b_hi;
            let hl = a_hi * b_lo;
            let hh = a_hi * b_hi;
            let (mid, mid_carry) = lh.overflowing_add(hl);
            let (lo, lo_carry) = ll.overflowing_add(mid << 64);
            let hi = hh + (mid >> 64) + ((mid_carry as u128) << 64) + (lo_carry as u128);
            (lo, hi)
        };
        let neg = k_val < 0;
        let dref = up_from((d, 0));
        let pmag = up_from((plo, phi));
        let q = pmag.checked_div(dref).expect("product/d fits U256 in test range");
        let r = pmag.checked_rem(dref).unwrap();
        let q128 = q.try_into_u128().expect("q fits u128 in test range");
        let expected: Option<i128> = if !neg {
            if q128 > i128::MAX as u128 {
                None
            } else {
                Some(q128 as i128)
            }
        } else if r.is_zero() {
            if q128 > i128::MAX as u128 {
                None
            } else {
                Some(-(q128 as i128))
            }
        } else {
            q128.checked_add(1).and_then(|q1| {
                if q1 > i128::MAX as u128 {
                    None
                } else {
                    Some(-(q1 as i128))
                }
            })
        };
        match (got.try_into_i128(), expected) {
            (Some(g), Some(e)) => assert_eq!(
                g, e,
                "wide_mul_div i={} basis={} k={} d={}",
                i, abs_basis, k_val, d
            ),
            (None, None) => {}
            (g, e) => panic!(
                "representability mismatch i={} basis={} k={} d={} got={:?} exp={:?}",
                i, abs_basis, k_val, d, g, e
            ),
        }
    }
}

#[test]
fn random_u256_dual_impl_cross_equality() {
    let mut rng = Lcg(0x9999_aaaa_bbbb_cccc);
    for _ in 0..100_000 {
        let a = (rng.next_biased(), rng.next_biased());
        let b = (rng.next_biased(), rng.next_biased());
        let (ak, bk) = (to_k(a), to_k(b));
        let (ap, bp) = (to_p(a), to_p(b));
        assert_eq!(
            ak.checked_add(bk).map(|v| (v.lo(), v.hi())),
            ap.checked_add(bp).map(|v| (v.lo(), v.hi()))
        );
        assert_eq!(
            ak.checked_sub(bk).map(|v| (v.lo(), v.hi())),
            ap.checked_sub(bp).map(|v| (v.lo(), v.hi()))
        );
        assert_eq!(
            ak.checked_mul(bk).map(|v| (v.lo(), v.hi())),
            ap.checked_mul(bp).map(|v| (v.lo(), v.hi()))
        );
        if !bk.is_zero() {
            assert_eq!(
                ak.checked_div(bk).map(|v| (v.lo(), v.hi())),
                ap.checked_div(bp).map(|v| (v.lo(), v.hi()))
            );
            assert_eq!(
                ak.checked_rem(bk).map(|v| (v.lo(), v.hi())),
                ap.checked_rem(bp).map(|v| (v.lo(), v.hi()))
            );
        }
    }
}
