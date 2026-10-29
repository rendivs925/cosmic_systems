//! Deterministic 3D cloud-noise volumes (cloud engine spec section 3).
//!
//! The volumetric cloud renderer samples three textures generated here at
//! startup: a low-frequency Perlin-Worley *base* volume whose red channel is the
//! remapped Perlin-Worley density and whose green/blue/alpha channels are Worley
//! fBm octaves, a high-frequency Worley *detail* volume for edge erosion, and a
//! 2D *weather* map carrying coverage, precipitation, cloud-type, and a wind
//! mask. All three are pure functions of a seed, so the same seed always yields
//! the same bytes; nothing here depends on wall-clock time, frame rate, or
//! global state. This module is presentation support only: it feeds no
//! authoritative simulation state.

/// Edge length of the base Perlin-Worley volume, in texels (spec section 7).
pub const BASE_NOISE_RES: usize = 64;
/// Edge length of the detail Worley volume, in texels (spec section 7).
pub const DETAIL_NOISE_RES: usize = 32;
/// Edge length of the 2D weather map, in texels.
pub const WEATHER_MAP_RES: usize = 256;

/// Perlin-Worley base volume as `BASE_NOISE_RES^3` RGBA8 texels.
///
/// Channels: red is the remapped Perlin-Worley base density, green/blue/alpha
/// are Worley fBm at increasing frequency. The renderer combines the three
/// Worley channels into the low-frequency fBm used to carve the base density.
pub fn base_noise_volume(seed: u64) -> Vec<u8> {
    volume(BASE_NOISE_RES, |x, y, z| {
        let p = cell_position(x, y, z, BASE_NOISE_RES, BASE_PERLIN_CELLS);
        let perlin = perlin3(seed ^ PERLIN_SALT, BASE_PERLIN_CELLS as i64, p);
        let perlin01 = (perlin * 0.5 + 0.5).clamp(0.0, 1.0);
        let w0 = worley3(seed ^ WORLEY_SALT, BASE_PERLIN_CELLS as i64, p);
        let w1 = worley3(
            seed ^ WORLEY_SALT,
            (BASE_PERLIN_CELLS * 2) as i64,
            scale(p, 2.0),
        );
        let w2 = worley3(
            seed ^ WORLEY_SALT,
            (BASE_PERLIN_CELLS * 4) as i64,
            scale(p, 4.0),
        );
        let low_fbm = w0 * 0.625 + w1 * 0.25 + w2 * 0.125;
        // remap(perlin, fbm - 1, 1, 0, 1)
        let base = remap(perlin01, low_fbm - 1.0, 1.0, 0.0, 1.0).clamp(0.0, 1.0);
        [to_u8(base), to_u8(w0), to_u8(w1), to_u8(w2)]
    })
}

/// High-frequency Worley detail volume as `DETAIL_NOISE_RES^3` RGBA8 texels.
///
/// Channels: red/green/blue are Worley octaves at increasing frequency; alpha is
/// unused but kept so the texture stays RGBA8 (a portable 3D format).
pub fn detail_noise_volume(seed: u64) -> Vec<u8> {
    volume(DETAIL_NOISE_RES, |x, y, z| {
        let p = cell_position(x, y, z, DETAIL_NOISE_RES, DETAIL_WORLEY_CELLS);
        let w0 = worley3(seed ^ DETAIL_SALT, DETAIL_WORLEY_CELLS as i64, p);
        let w1 = worley3(
            seed ^ DETAIL_SALT,
            (DETAIL_WORLEY_CELLS * 2) as i64,
            scale(p, 2.0),
        );
        let w2 = worley3(
            seed ^ DETAIL_SALT,
            (DETAIL_WORLEY_CELLS * 4) as i64,
            scale(p, 4.0),
        );
        [to_u8(w0), to_u8(w1), to_u8(w2), 255]
    })
}

/// 2D weather map as `WEATHER_MAP_RES^2` RGBA8 texels (spec section 8).
///
/// Channels: red is cloud coverage, green is precipitation / density scale,
/// blue is the cloud-type altitude morph, and alpha is a wind-drift mask. Each
/// channel is an independent seeded fBm so the fields are spatially related but
/// not identical.
pub fn weather_map(seed: u64) -> Vec<u8> {
    let res = WEATHER_MAP_RES;
    let mut data = vec![0u8; res * res * 4];
    for y in 0..res {
        for x in 0..res {
            let u = x as f64 / res as f64;
            let v = y as f64 / res as f64;
            let coverage = fbm2(seed ^ WEATHER_COVERAGE_SALT, u, v, 4, 3.0);
            let precipitation = fbm2(seed ^ WEATHER_PRECIP_SALT, u, v, 3, 5.0);
            let cloud_type = fbm2(seed ^ WEATHER_TYPE_SALT, u, v, 3, 2.0);
            let wind = fbm2(seed ^ WEATHER_WIND_SALT, u, v, 2, 1.5);
            let index = (y * res + x) * 4;
            data[index] = to_u8(coverage);
            data[index + 1] = to_u8(precipitation);
            data[index + 2] = to_u8(cloud_type);
            data[index + 3] = to_u8(wind);
        }
    }
    data
}

// Number of Perlin/Worley cells across the base volume. Low enough that a single
// volume tile reads as large cloud masses when the renderer samples it, high
// enough to break the deck into distinct billows.
const BASE_PERLIN_CELLS: usize = 6;
const DETAIL_WORLEY_CELLS: usize = 8;

const PERLIN_SALT: u64 = 0x1F1E_2D3C_4B5A_6978;
const WORLEY_SALT: u64 = 0x9E37_79B9_7F4A_7C15;
const DETAIL_SALT: u64 = 0x2545_F491_4F6C_DD1D;
const WEATHER_COVERAGE_SALT: u64 = 0x1000_0000_0000_0001;
const WEATHER_PRECIP_SALT: u64 = 0x2000_0000_0000_0002;
const WEATHER_TYPE_SALT: u64 = 0x3000_0000_0000_0003;
const WEATHER_WIND_SALT: u64 = 0x4000_0000_0000_0004;

/// Map a texel index to a periodic noise coordinate. Texel `res` maps back onto
/// texel `0`, so the volume tiles seamlessly.
fn cell_position(x: usize, y: usize, z: usize, res: usize, cells: usize) -> (f64, f64, f64) {
    let s = cells as f64 / res as f64;
    (x as f64 * s, y as f64 * s, z as f64 * s)
}

fn scale(p: (f64, f64, f64), factor: f64) -> (f64, f64, f64) {
    (p.0 * factor, p.1 * factor, p.2 * factor)
}

fn volume(res: usize, mut f: impl FnMut(usize, usize, usize) -> [u8; 4]) -> Vec<u8> {
    let mut data = vec![0u8; res * res * res * 4];
    for z in 0..res {
        for y in 0..res {
            for x in 0..res {
                let texel = f(x, y, z);
                let index = ((z * res + y) * res + x) * 4;
                data[index..index + 4].copy_from_slice(&texel);
            }
        }
    }
    data
}

fn to_u8(value: f64) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn remap(value: f64, old_min: f64, old_max: f64, new_min: f64, new_max: f64) -> f64 {
    if (old_max - old_min).abs() < f64::EPSILON {
        return new_min;
    }
    new_min + (value - old_min) / (old_max - old_min) * (new_max - new_min)
}

/// Deterministic 64-bit lattice hash. Wrapping multiplies keep it portable and
/// independent of floating-point behaviour.
fn hash3(seed: u64, x: i64, y: i64, z: i64) -> u64 {
    let mut h = seed
        ^ (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (y as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9)
        ^ (z as u64).wrapping_mul(0x94D0_49BB_1331_11EB);
    h ^= h >> 30;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 27;
    h = h.wrapping_mul(0x94D0_49BB_1331_11EB);
    h ^= h >> 31;
    h
}

fn hash01(seed: u64, x: i64, y: i64, z: i64) -> f64 {
    (hash3(seed, x, y, z) >> 11) as f64 / (1u64 << 53) as f64
}

fn fade(t: f64) -> f64 {
    // Quintic smoothstep; the classic Perlin interpolation curve.
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

/// Periodic 3D Perlin gradient noise in roughly `[-1, 1]`.
fn perlin3(seed: u64, period: i64, p: (f64, f64, f64)) -> f64 {
    let xf = p.0.floor();
    let yf = p.1.floor();
    let zf = p.2.floor();
    let (x0, y0, z0) = (xf as i64, yf as i64, zf as i64);
    let (fx, fy, fz) = (p.0 - xf, p.1 - yf, p.2 - zf);
    let (u, v, w) = (fade(fx), fade(fy), fade(fz));

    let corner = |dx: i64, dy: i64, dz: i64| {
        let gx = (x0 + dx).rem_euclid(period);
        let gy = (y0 + dy).rem_euclid(period);
        let gz = (z0 + dz).rem_euclid(period);
        gradient(
            hash3(seed, gx, gy, gz),
            fx - dx as f64,
            fy - dy as f64,
            fz - dz as f64,
        )
    };

    let x00 = lerp(corner(0, 0, 0), corner(1, 0, 0), u);
    let x10 = lerp(corner(0, 1, 0), corner(1, 1, 0), u);
    let x01 = lerp(corner(0, 0, 1), corner(1, 0, 1), u);
    let x11 = lerp(corner(0, 1, 1), corner(1, 1, 1), u);
    let y0v = lerp(x00, x10, v);
    let y1v = lerp(x01, x11, v);
    lerp(y0v, y1v, w)
}

/// One of the twelve cube-edge gradient directions selected by the low bits of
/// `h` (Perlin's improved noise).
fn gradient(h: u64, dx: f64, dy: f64, dz: f64) -> f64 {
    let h = (h & 15) as u32;
    let u = if h < 8 { dx } else { dy };
    let v = if h < 4 {
        dy
    } else if h == 12 || h == 14 {
        dx
    } else {
        dz
    };
    (if h & 1 == 0 { u } else { -u }) + (if h & 2 == 0 { v } else { -v })
}

/// Periodic 3D Worley (cellular) noise in `[0, 1]`: the distance to the nearest
/// seeded feature point, clamped. Feature points are one per integer cell, so
/// the field is continuous and tiles over `period`.
fn worley3(seed: u64, period: i64, p: (f64, f64, f64)) -> f64 {
    let bx = p.0.floor() as i64;
    let by = p.1.floor() as i64;
    let bz = p.2.floor() as i64;
    let mut min_dist = f64::MAX;
    for dz in -1..=1 {
        for dy in -1..=1 {
            for dx in -1..=1 {
                let cx = bx + dx;
                let cy = by + dy;
                let cz = bz + dz;
                let hx = cx.rem_euclid(period);
                let hy = cy.rem_euclid(period);
                let hz = cz.rem_euclid(period);
                let h = hash3(seed, hx, hy, hz);
                let fx = cx as f64 + hash01(h, 1, 0, 0);
                let fy = cy as f64 + hash01(h, 0, 1, 0);
                let fz = cz as f64 + hash01(h, 0, 0, 1);
                let ddx = p.0 - fx;
                let ddy = p.1 - fy;
                let ddz = p.2 - fz;
                let dist = (ddx * ddx + ddy * ddy + ddz * ddz).sqrt();
                if dist < min_dist {
                    min_dist = dist;
                }
            }
        }
    }
    min_dist.clamp(0.0, 1.0)
}

/// Periodic 2D fBm in `[0, 1]` over a unit domain, used by the weather map.
fn fbm2(seed: u64, u: f64, v: f64, octaves: u32, frequency: f64) -> f64 {
    let mut sum = 0.0;
    let mut amp = 1.0;
    let mut freq = frequency;
    let mut norm = 0.0;
    for octave in 0..octaves {
        // The map samples `u * freq` over `u` in [0, 1], so the cell period must
        // equal the frequency for the field to wrap at the antimeridian. A
        // larger period leaves a hard seam there, which becomes a repeated
        // stripe once the renderer tiles the map.
        let period = freq.round() as i64;
        sum += amp
            * value_noise2(
                seed.wrapping_add(octave as u64 * 0x9E37_79B9),
                u * freq,
                v * freq,
                period,
            );
        norm += amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    (sum / norm).clamp(0.0, 1.0)
}

/// Periodic 2D value noise in `[0, 1]` (bilinear interpolation of hashed cells).
fn value_noise2(seed: u64, x: f64, y: f64, period: i64) -> f64 {
    let xi = x.floor() as i64;
    let yi = y.floor() as i64;
    let fx = fade(x - xi as f64);
    let fy = fade(y - yi as f64);
    let cell = |dx: i64, dy: i64| {
        hash01(
            seed,
            (xi + dx).rem_euclid(period),
            (yi + dy).rem_euclid(period),
            0,
        )
    };
    let a = cell(0, 0);
    let b = cell(1, 0);
    let c = cell(0, 1);
    let d = cell(1, 1);
    lerp(lerp(a, b, fx), lerp(c, d, fx), fy)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volumes_have_the_documented_size() {
        assert_eq!(
            base_noise_volume(1).len(),
            BASE_NOISE_RES * BASE_NOISE_RES * BASE_NOISE_RES * 4
        );
        assert_eq!(
            detail_noise_volume(1).len(),
            DETAIL_NOISE_RES * DETAIL_NOISE_RES * DETAIL_NOISE_RES * 4
        );
        assert_eq!(weather_map(1).len(), WEATHER_MAP_RES * WEATHER_MAP_RES * 4);
    }

    #[test]
    fn generation_is_deterministic() {
        assert_eq!(base_noise_volume(7), base_noise_volume(7));
        assert_eq!(detail_noise_volume(7), detail_noise_volume(7));
        assert_eq!(weather_map(7), weather_map(7));
    }

    #[test]
    fn different_seeds_produce_different_fields() {
        assert_ne!(base_noise_volume(1), base_noise_volume(2));
        assert_ne!(weather_map(1), weather_map(2));
    }

    #[test]
    fn base_volume_spans_the_density_range() {
        let data = base_noise_volume(3);
        let reds: Vec<u8> = data.chunks(4).map(|t| t[0]).collect();
        let min = *reds.iter().min().unwrap();
        let max = *reds.iter().max().unwrap();
        // A useful density field must not collapse to a constant. The raw
        // Perlin-Worley base sits above zero where the Worley fBm is low; the
        // renderer carves clear air from it with the weather coverage and edge
        // erosion, so the volume itself only needs a wide, non-degenerate span.
        assert!(
            max - min > 100,
            "base density span too narrow: {min}..{max}"
        );
        assert!(
            max > 200,
            "base density never reaches full cloud (max {max})"
        );
    }

    #[test]
    fn worley_channels_are_populated() {
        let data = base_noise_volume(5);
        // The three Worley fBm channels must each vary, not be flat.
        for channel in 1..4 {
            let values: Vec<u8> = data.chunks(4).map(|t| t[channel]).collect();
            let min = *values.iter().min().unwrap();
            let max = *values.iter().max().unwrap();
            assert!(max - min > 40, "channel {channel} is nearly flat");
        }
    }

    #[test]
    fn weather_channels_are_bounded_and_varied() {
        let data = weather_map(11);
        for channel in 0..4 {
            let values: Vec<u8> = data.chunks(4).map(|t| t[channel]).collect();
            let min = *values.iter().min().unwrap();
            let max = *values.iter().max().unwrap();
            assert!(max - min > 20, "weather channel {channel} is nearly flat");
        }
    }

    #[test]
    fn fbm2_is_periodic_over_the_unit_square() {
        // The renderer tiles the weather map, so the field must wrap exactly at
        // both seams. A non-wrapping period leaves a hard stripe that repeats
        // once per tile.
        for seed in [1u64, 7, 0x5EED_C10D] {
            for u in [0.0, 0.17, 0.5, 0.83] {
                let a = fbm2(seed, u, 0.0, 4, 3.0);
                let b = fbm2(seed, u, 1.0, 4, 3.0);
                assert!((a - b).abs() < 1e-9, "v seam off by {}", (a - b).abs());
            }
            for v in [0.0, 0.31, 0.5, 0.72] {
                let a = fbm2(seed, 0.0, v, 4, 3.0);
                let b = fbm2(seed, 1.0, v, 4, 3.0);
                assert!((a - b).abs() < 1e-9, "u seam off by {}", (a - b).abs());
            }
        }
    }

    #[test]
    fn perlin_is_bounded_and_periodic() {
        for i in 0..1000 {
            let t = i as f64 * 0.137;
            let value = perlin3(9, 4, (t, t * 0.5, t * 0.25));
            assert!(value.abs() <= 1.5, "perlin out of range: {value}");
        }
        // Periodicity: a shift of `period` reproduces the field exactly.
        let a = perlin3(9, 4, (0.3, 1.7, 2.9));
        let b = perlin3(9, 4, (0.3 + 4.0, 1.7 + 4.0, 2.9 + 4.0));
        assert!((a - b).abs() < 1e-12, "perlin is not periodic: {a} vs {b}");
    }

    #[test]
    fn worley_is_bounded_and_periodic() {
        for i in 0..1000 {
            let t = i as f64 * 0.211;
            let value = worley3(9, 4, (t, t * 0.3, t * 0.7));
            assert!((0.0..=1.0).contains(&value), "worley out of range: {value}");
        }
        let a = worley3(9, 4, (0.4, 1.1, 2.2));
        let b = worley3(9, 4, (0.4 + 4.0, 1.1 + 4.0, 2.2 + 4.0));
        assert!((a - b).abs() < 1e-12, "worley is not periodic: {a} vs {b}");
    }
}
