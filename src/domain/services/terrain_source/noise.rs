//! Deterministic 3D value noise used by the procedural terrain sources.

/// Height from a deterministic 3D value-noise field. Evaluating the noise on
/// the unit sphere (via a direction vector) is naturally seamless — there is no
/// longitude seam to worry about.
#[derive(Debug, Clone, Copy)]
pub struct ValueNoise;

impl ValueNoise {
    pub(crate) fn cell3(&self, seed: u64, x: i64, y: i64, z: i64) -> f64 {
        let mut h = seed
            ^ (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
            ^ (y as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9)
            ^ (z as u64).wrapping_mul(0x94D0_49BB_1331_11EB);
        h ^= h >> 30;
        h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        h ^= h >> 27;
        h = h.wrapping_mul(0x94D0_49BB_1331_11EB);
        h ^= h >> 31;
        (h & 0xFFFF_FFFF_FFFF) as f64 / 0x1_0000_0000_0000u64 as f64
    }

    fn smooth(t: f64) -> f64 {
        t * t * (3.0 - 2.0 * t)
    }

    pub(crate) fn value_noise3(&self, seed: u64, x: f64, y: f64, z: f64) -> f64 {
        let (x0, y0, z0) = (x.floor() as i64, y.floor() as i64, z.floor() as i64);
        let (fx, fy, fz) = (
            Self::smooth(x - x.floor()),
            Self::smooth(y - y.floor()),
            Self::smooth(z - z.floor()),
        );
        let (x1, y1, z1) = (x0 + 1, y0 + 1, z0 + 1);
        let lerp = |a: f64, b: f64, t: f64| a + (b - a) * t;
        // Trilinear interpolation of the 8 lattice corners.
        let c000 = self.cell3(seed, x0, y0, z0);
        let c100 = self.cell3(seed, x1, y0, z0);
        let c010 = self.cell3(seed, x0, y1, z0);
        let c110 = self.cell3(seed, x1, y1, z0);
        let c001 = self.cell3(seed, x0, y0, z1);
        let c101 = self.cell3(seed, x1, y0, z1);
        let c011 = self.cell3(seed, x0, y1, z1);
        let c111 = self.cell3(seed, x1, y1, z1);
        let x00 = lerp(c000, c100, fx);
        let x10 = lerp(c010, c110, fx);
        let x01 = lerp(c001, c101, fx);
        let x11 = lerp(c011, c111, fx);
        let y0v = lerp(x00, x10, fy);
        let y1v = lerp(x01, x11, fy);
        lerp(y0v, y1v, fz)
    }

    pub(crate) fn fbm(&self, seed: u64, x: f64, y: f64, z: f64, octaves: u32) -> f64 {
        let mut sum = 0.0;
        let mut amp = 1.0;
        let mut freq = 1.0;
        let mut norm = 0.0;
        for octave in 0..octaves {
            sum += amp
                * self.value_noise3(
                    seed.wrapping_add((octave as u64).wrapping_mul(0x517C_C1B7_2722_0A95)),
                    x * freq,
                    y * freq,
                    z * freq,
                );
            norm += amp;
            amp *= 0.5;
            freq *= 2.0;
        }
        sum / norm
    }

    pub(crate) fn ridged_noise(&self, seed: u64, x: f64, y: f64, z: f64, octaves: u32) -> f64 {
        let mut sum = 0.0;
        let mut amp = 0.5;
        let mut freq = 1.0;
        let mut norm = 0.0;
        for octave in 0..octaves {
            let n = self.value_noise3(
                seed.wrapping_add((octave as u64).wrapping_mul(0x6EED_0E9D_9D95_A5C5)),
                x * freq,
                y * freq,
                z * freq,
            );
            let ridge = 1.0 - (2.0 * n - 1.0).abs();
            sum += amp * ridge * ridge;
            norm += amp;
            amp *= 0.5;
            freq *= 2.0;
        }
        sum / norm
    }
}
