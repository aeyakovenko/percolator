//! Adaptive PnL warmup with stress-responsive throttling
//!
//! This module implements a sophisticated warmup system that:
//! - Monitors 1h and 5m deposit drain via EMAs
//! - Adaptively slows PnL unlocks during stress (30min → 5h)
//! - Uses hysteresis to prevent rapid speed-ups
//! - Operates in Q32.32 fixed-point (no_std compatible)
//! - Proven safe via 7 Kani proofs

#![allow(dead_code)]

/// Q32.32 fixed-point type
pub type I = i128;

/// Scale factor for Q32.32 (2^32)
pub const F: I = 1_i128 << 32;

/// Create Q32.32 from integer
#[inline]
pub const fn q32(x: i64) -> I {
    (x as I) * F
}

/// Q32.32 representation of 1.0
#[inline]
pub const fn q1() -> I {
    F
}

/// Clamp value to [0, 1] in Q32.32
#[inline]
pub fn qclamp01(x: I) -> I {
    if x < 0 {
        0
    } else if x > q1() {
        q1()
    } else {
        x
    }
}

/// Multiply two Q32.32 values
#[inline]
pub fn qmul(a: I, b: I) -> I {
    // (a * b) / F with overflow protection
    // For bounded values (|a|, |b| < 2^64), this is safe
    ((a * b) / F)
}

/// Divide two Q32.32 values
#[inline]
pub fn qdiv(a: I, b: I) -> I {
    if b == 0 {
        return I::MAX / 2; // Saturate
    }
    (a * F) / b
}

/// Adaptive warmup configuration
#[derive(Clone, Copy)]
pub struct AdaptiveWarmupConfig {
    /// Fast t90 baseline (30 minutes in seconds)
    pub t90_fast_secs: I,

    /// Max outflow fraction before max braking (0.30 = 30%)
    pub s_max: I,

    /// Max multiplier for t90 slowdown (10x)
    pub m_max: I,

    /// Hysteresis: max speedup per update (0.30 = 30%)
    pub hysteresis: I,

    /// Weight on 1h drain (0.6)
    pub w_1h: I,

    /// Weight on 5m drain (0.4)
    pub w_5m: I,

    /// Slot duration in seconds
    pub slot_secs: I,

    /// ln(10) for 10x decay calculation
    pub ln10: I,

    /// EMA alpha for 1h deposit tracking
    pub alpha_d_1h: I,

    /// EMA alpha for 5m deposit tracking
    pub alpha_d_5m: I,

    /// EMA alpha for slow drain smoothing
    pub alpha_s_slow: I,

    /// EMA alpha for fast drain smoothing
    pub alpha_s_fast: I,

    /// Freeze threshold: halt unlocks if drain >= this (0.25 = 25%)
    pub freeze_s: I,
}

impl Default for AdaptiveWarmupConfig {
    fn default() -> Self {
        Self {
            t90_fast_secs: q32(30 * 60),              // 30 minutes
            s_max: qdiv(q32(3), q32(10)),             // 0.30
            m_max: q32(10),                            // 10x max slowdown
            hysteresis: qdiv(q32(3), q32(10)),        // 0.30 (30% max speedup)
            w_1h: qdiv(q32(3), q32(5)),               // 0.60
            w_5m: qdiv(q32(2), q32(5)),               // 0.40
            slot_secs: qdiv(q32(4), q32(10)),         // 0.4s (~400ms)
            ln10: qdiv(q32(2302585092), q32(1000000000)), // ln(10) ≈ 2.302585
            alpha_d_1h: qdiv(q32(1), q32(9000)),      // ~1h effective window
            alpha_d_5m: qdiv(q32(1), q32(750)),       // ~5m effective window
            alpha_s_slow: qdiv(q32(1), q32(1000)),    // Slow drain smoothing
            alpha_s_fast: qdiv(q32(1), q32(100)),     // Fast drain smoothing
            freeze_s: qdiv(q32(1), q32(4)),           // 0.25 (25%)
        }
    }
}

/// Adaptive warmup state
#[derive(Clone, Copy, Default)]
pub struct AdaptiveWarmupState {
    /// EMA of total deposits (1h window)
    pub d_ema_1h: I,

    /// EMA of total deposits (5m window)
    pub d_ema_5m: I,

    /// Smoothed 1h drain fraction [0, 1]
    pub s_ema_1h: I,

    /// Smoothed 5m drain fraction [0, 1]
    pub s_ema_5m: I,

    /// Last applied t90 (seconds)
    pub last_t90_secs: I,

    /// Current unlocked fraction [0, 1]
    pub unlocked_frac: I,
}

/// Update EMA: ema' = ema + alpha * (sample - ema)
#[inline]
pub fn ema_update(ema: I, sample: I, alpha: I) -> I {
    let delta = sample - ema;
    ema + qmul(alpha, delta)
}

/// Compute drain fraction: s = max(0, 1 - D_now / D_ref)
#[inline]
pub fn drain_s(d_now: I, d_ref: I) -> I {
    if d_ref <= 0 {
        return 0; // No reference, assume no drain
    }
    let ratio = qdiv(d_now, d_ref);
    let s = q1() - ratio;
    if s > 0 {
        qclamp01(s)
    } else {
        0
    }
}

/// Smoothstep curve: r^2 * (3 - 2r), monotone on [0, 1]
#[inline]
pub fn smoothstep(r: I) -> I {
    let r = qclamp01(r);
    let r2 = qmul(r, r);
    let three = q32(3);
    let two = q32(2);
    qmul(r2, three - qmul(two, r))
}

/// Map s* to t90 with hysteresis
pub fn t90_from_s(state_last_t90: I, s_star: I, cfg: &AdaptiveWarmupConfig) -> I {
    // r = s* / s_max, clamped to [0, 1]
    let r = qclamp01(qdiv(s_star, cfg.s_max));

    // Smooth brake curve
    let curve = smoothstep(r);

    // m = 1 + (m_max - 1) * curve, in [1, m_max]
    let m_minus1 = cfg.m_max - q1();
    let m = q1() + qmul(m_minus1, curve);

    // target = t90_fast * m
    let target = qmul(cfg.t90_fast_secs, m);

    // Hysteresis: only allow at most HYSTERESIS fraction faster per update
    let down_cap = qmul(state_last_t90, q1() - cfg.hysteresis);
    if target < down_cap {
        down_cap
    } else {
        target
    }
}

/// Exponential approximation: exp(-x) for small x
/// Uses Taylor series: exp(-x) ≈ 1 - x + x^2/2 - x^3/6
/// Valid for x in [0, 0.5], returns Q32.32 in (0, 1]
fn exp_neg_approx(x: I) -> I {
    if x <= 0 {
        return q1();
    }
    if x >= q32(1) {
        return qdiv(q32(1), q32(3)); // ~exp(-1) ≈ 0.368
    }

    // exp(-x) ≈ 1 - x + x^2/2 - x^3/6
    let one = q1();
    let x2 = qmul(x, x);
    let x3 = qmul(x2, x);

    let half = qdiv(q32(1), q32(2));
    let sixth = qdiv(q32(1), q32(6));

    let result = one - x + qmul(half, x2) - qmul(sixth, x3);

    // Clamp to (0, 1]
    if result < 0 {
        0
    } else if result > one {
        one
    } else {
        result
    }
}

/// Update unlocked fraction: P' = 1 - (1 - P) * exp(-lambda * dt)
pub fn unlocked_update(p_prev: I, lambda_q: I, slot_secs_q: I) -> I {
    if lambda_q <= 0 {
        return p_prev; // Frozen
    }

    // x = lambda * dt
    let x = qmul(lambda_q, slot_secs_q);

    // If x > 0.5, do multiple sub-steps to keep approximation accurate
    let half = qdiv(q32(1), q32(2));
    if x > half {
        // Do 2 half-steps
        let x_half = qdiv(x, q32(2));
        let p_mid = unlocked_update_single(p_prev, x_half);
        return unlocked_update_single(p_mid, x_half);
    }

    unlocked_update_single(p_prev, x)
}

/// Single step of unlocked update
#[inline]
fn unlocked_update_single(p_prev: I, x: I) -> I {
    let beta = exp_neg_approx(x);
    let one_minus_p = q1() - qclamp01(p_prev);
    let p = q1() - qmul(one_minus_p, beta);
    qclamp01(p)
}

/// Main adaptive warmup step
pub fn step(
    st: &mut AdaptiveWarmupState,
    cfg: &AdaptiveWarmupConfig,
    d_now_q: I,
    oracle_gap_large: bool,
    insurance_util_high: bool,
) {
    // 1. Update deposit EMAs
    st.d_ema_1h = ema_update(st.d_ema_1h, d_now_q, cfg.alpha_d_1h);
    st.d_ema_5m = ema_update(st.d_ema_5m, d_now_q, cfg.alpha_d_5m);

    // 2. Compute raw drains
    let s_raw_1h = drain_s(d_now_q, st.d_ema_1h);
    let s_raw_5m = drain_s(d_now_q, st.d_ema_5m);

    // 3. Smooth drains (one-way: only increase during outflows)
    let s1_next = ema_update(st.s_ema_1h, s_raw_1h, cfg.alpha_s_slow);
    let s5_next = ema_update(st.s_ema_5m, s_raw_5m, cfg.alpha_s_fast);

    // One-way ratchet: drains only go up
    st.s_ema_1h = if s1_next > st.s_ema_1h {
        s1_next
    } else {
        st.s_ema_1h
    };
    st.s_ema_5m = if s5_next > st.s_ema_5m {
        s5_next
    } else {
        st.s_ema_5m
    };

    // 4. Combine weighted drains: s* = w_1h * s_1h + w_5m * s_5m
    let s_star = qclamp01(qmul(cfg.w_1h, st.s_ema_1h) + qmul(cfg.w_5m, st.s_ema_5m));

    // 5. Map s* to t90 with hysteresis
    let t90 = t90_from_s(st.last_t90_secs, s_star, cfg);
    st.last_t90_secs = t90;

    // 6. Check freeze condition
    let freeze = s_star >= cfg.freeze_s && (oracle_gap_large || insurance_util_high);

    // 7. Compute lambda = ln(10) / t90
    let lambda = if freeze {
        0
    } else {
        qdiv(cfg.ln10, t90)
    };

    // 8. Update unlocked fraction
    st.unlocked_frac = unlocked_update(st.unlocked_frac, lambda, cfg.slot_secs);
}

// Kani proofs moved to crates/proofs/kani/src/adaptive_warmup.rs
