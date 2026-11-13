//! The original's random numbers (RE_BRUSHES §3.5).
//!
//! KPD3 uses the MSVC C runtime `rand()` (15-bit linear congruential generator, state
//! `x = x*214013 + 2531011`, output `(x >> 16) & 0x7fff`) everywhere, seeded with
//! `srand(timeGetTime())` (e.g. in the voice-brush constructor). `rnd(lo, hi)` is the
//! helper at `@0x459760`.

/// MSVC-compatible `rand()` plus the program's helpers. Clone it to fork a sequence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MsRand {
    state: u32,
}

impl Default for MsRand {
    /// The C runtime's state before any `srand` call is 1.
    fn default() -> Self {
        MsRand { state: 1 }
    }
}

impl MsRand {
    /// `srand(seed)`.
    pub fn new(seed: u32) -> MsRand {
        MsRand { state: seed }
    }

    pub fn srand(&mut self, seed: u32) {
        self.state = seed;
    }

    pub fn state(&self) -> u32 {
        self.state
    }

    /// `rand()`: 0..=32767.
    #[inline]
    pub fn rand(&mut self) -> i32 {
        self.state = self.state.wrapping_mul(214013).wrapping_add(2531011);
        ((self.state >> 16) & 0x7fff) as i32
    }

    /// `rnd(lo, hi)` `@0x459760`: uniform in `[lo, hi]` by rejection. Reproduces the
    /// original exactly, including its `>` (not `>=`) rejection test, which lets the
    /// value `limit` through (it maps to `lo`).
    pub fn rnd(&mut self, lo: i32, hi: i32) -> i32 {
        let n = (hi.wrapping_sub(lo).wrapping_add(1)) as u32;
        if n == 0 {
            // The original divides by zero here; never happens with sane arguments.
            return lo;
        }
        let limit = 0x8000u32.wrapping_sub(0x8000 % n);
        let mut i = self.rand() as u32;
        // With hi < lo the original's unsigned `n` is huge, `limit` becomes 0 and it spins
        // until rand() returns 0. Bound the spin (the LCG returns 0 well within this).
        let mut guard = 0u32;
        while i > limit {
            i = self.rand() as u32;
            guard += 1;
            if guard > 1 << 20 {
                return lo;
            }
        }
        lo.wrapping_add((i % n) as i32)
    }

    /// Particle helper: `(rand() - 16383) * k` in about [-1, 1] (`k = 1/16383`).
    pub fn unit(&mut self) -> f64 {
        (self.rand() - 16383) as f64 * (1.0 / 16383.0)
    }

    /// A random 24-bit colour `(rnd(0,255), rnd(0,255), rnd(0,255))`.
    pub fn color(&mut self) -> [u8; 3] {
        let r = self.rnd(0, 255) as u8;
        let g = self.rnd(0, 255) as u8;
        let b = self.rnd(0, 255) as u8;
        [r, g, b]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn msvc_sequence() {
        // Well-known MSVC rand() output for srand(1) / no srand.
        let mut r = MsRand::default();
        let v: Vec<i32> = (0..10).map(|_| r.rand()).collect();
        assert_eq!(v, [41, 18467, 6334, 26500, 19169, 15724, 11478, 29358, 26962, 24464]);
        let mut r = MsRand::new(0);
        assert_eq!(r.rand(), 38);
        assert_eq!(r.rand(), 7719);
    }

    #[test]
    fn rnd_range() {
        let mut r = MsRand::new(1234);
        for _ in 0..10000 {
            let v = r.rnd(-5, 5);
            assert!((-5..=5).contains(&v));
        }
        // rnd(lo, lo) consumes one rand() and returns lo
        let mut a = MsRand::new(7);
        let s = a.state();
        assert_eq!(a.rnd(3, 3), 3);
        assert_ne!(a.state(), s);
        // exact mapping: rnd(0, 9) = rand() % 10 when rand() <= 32760
        let mut a = MsRand::default();
        let mut b = MsRand::default();
        for _ in 0..100 {
            let x = b.rand();
            if x > 32760 {
                break;
            }
            assert_eq!(a.rnd(0, 9), x % 10);
        }
    }
}
