//! CPU mip-chain generation for runtime-created terrain textures.
//!
//! Bevy 0.17 does not generate mipmaps for `Image` assets, and the terrain
//! presentation layer creates its albedo, tangent-space normal, and streamed
//! imagery textures at runtime. Sampling those single-level textures with a
//! mip-filtering sampler aliases into shimmer and moiré as soon as a patch is
//! minified (distance, grazing angles, or many texels per screen pixel), so the
//! mip chain must be supplied explicitly.

/// How each 2x2 box filter should combine the source texels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MipFilter {
    /// Plain average of the encoded RGBA texels. Correct for albedo, roughness,
    /// and other linear channels.
    Color,
    /// Average the decoded normal and renormalize, then average the remaining
    /// channel. Keeps every mip a unit normal instead of shrinking toward the
    /// encoded zero vector.
    Normal,
}

/// Build a complete mip chain for RGBA8 texels, returning the concatenated
/// level data (level 0 first) and the level count. Level 0 is `base` unchanged.
///
/// The layout matches `wgpu`'s `TextureDataOrder::LayerMajor` for a 2D texture:
/// every level's texels are contiguous, largest first. Pass the result to
/// [`bevy::image::Image::new_uninit`] and set `mip_level_count` to the returned
/// count; `Image::new` cannot be used because it asserts single-level data.
pub(crate) fn mip_chain_rgba8(
    width: u32,
    height: u32,
    base: &[u8],
    filter: MipFilter,
) -> (Vec<u8>, u32) {
    debug_assert_eq!(base.len(), (width as usize) * (height as usize) * 4);
    let upper = 1 + width.max(height).ilog2();
    let mut data = Vec::with_capacity(base.len() * 4 / 3 + 4);
    data.extend_from_slice(base);
    let mut levels = 1u32;
    let (mut level_width, mut level_height) = (width.max(1), height.max(1));
    let mut level_start = 0usize;

    while level_width > 1 || level_height > 1 {
        let next_width = (level_width / 2).max(1);
        let next_height = (level_height / 2).max(1);
        debug_assert!(levels < upper);
        let next = downsample(
            &data[level_start..],
            level_width,
            level_height,
            next_width,
            next_height,
            filter,
        );
        level_start = data.len();
        data.extend_from_slice(&next);
        level_width = next_width;
        level_height = next_height;
        levels += 1;
    }

    (data, levels)
}

/// One 2x2 box-filter step. Odd dimensions clamp the odd tap to the last texel,
/// so non-power-of-two terrain maps still terminate at a single texel.
fn downsample(
    src: &[u8],
    width: u32,
    height: u32,
    next_width: u32,
    next_height: u32,
    filter: MipFilter,
) -> Vec<u8> {
    let mut out = vec![0u8; (next_width as usize) * (next_height as usize) * 4];
    for y in 0..next_height {
        for x in 0..next_width {
            let mut accumulator = [0.0f32; 4];
            for dy in 0..2u32 {
                for dx in 0..2u32 {
                    let source_x = (x * 2 + dx).min(width - 1);
                    let source_y = (y * 2 + dy).min(height - 1);
                    let index = ((source_y * width + source_x) * 4) as usize;
                    for channel in 0..4 {
                        accumulator[channel] += src[index + channel] as f32;
                    }
                }
            }
            for channel in accumulator.iter_mut() {
                *channel *= 0.25;
            }
            let index = ((y * next_width + x) * 4) as usize;
            match filter {
                MipFilter::Color => {
                    for channel in 0..4 {
                        out[index + channel] = quantize(accumulator[channel]);
                    }
                }
                MipFilter::Normal => {
                    let normal = [
                        accumulator[0] * (2.0 / 255.0) - 1.0,
                        accumulator[1] * (2.0 / 255.0) - 1.0,
                        accumulator[2] * (2.0 / 255.0) - 1.0,
                    ];
                    let length =
                        (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2])
                            .sqrt()
                            .max(1e-6);
                    for channel in 0..3 {
                        let unit = normal[channel] / length;
                        out[index + channel] = quantize((unit * 0.5 + 0.5) * 255.0);
                    }
                    out[index + 3] = quantize(accumulator[3]);
                }
            }
        }
    }
    out
}

fn quantize(value: f32) -> u8 {
    value.round().clamp(0.0, 255.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chain_terminates_at_one_texel_with_expected_level_count() {
        for (width, height) in [(256u32, 256u32), (128, 64), (100, 100), (1, 1), (3, 5)] {
            let base = vec![128u8; (width as usize) * (height as usize) * 4];
            let (_, levels) = mip_chain_rgba8(width, height, &base, MipFilter::Color);
            let expected = 1 + width.max(height).ilog2();
            assert_eq!(levels, expected, "{width}x{height}");
        }
    }

    #[test]
    fn chain_length_matches_the_sum_of_all_levels() {
        let width = 8u32;
        let height = 4u32;
        let base = vec![200u8; (width as usize) * (height as usize) * 4];
        let (data, levels) = mip_chain_rgba8(width, height, &base, MipFilter::Color);
        let expected_bytes: usize = (0..levels)
            .map(|level| {
                let w = (width >> level).max(1);
                let h = (height >> level).max(1);
                (w as usize) * (h as usize) * 4
            })
            .sum();
        assert_eq!(data.len(), expected_bytes);
    }

    #[test]
    fn uniform_color_stays_uniform_through_the_chain() {
        let base = [10u8, 20, 30, 255]
            .iter()
            .cycle()
            .take(16 * 16 * 4)
            .copied()
            .collect::<Vec<_>>();
        let (data, _) = mip_chain_rgba8(16, 16, &base, MipFilter::Color);
        for texel in data.chunks_exact(4) {
            assert_eq!(texel, [10, 20, 30, 255]);
        }
    }

    #[test]
    fn normal_mips_stay_unit_length() {
        let mut base = Vec::with_capacity(8 * 8 * 4);
        for y in 0..8 {
            for x in 0..8 {
                let nx = (x as f32 / 7.0) * 2.0 - 1.0;
                let ny = (y as f32 / 7.0) * 2.0 - 1.0;
                let nz = (1.0 - nx * nx - ny * ny).max(0.01).sqrt();
                let length = (nx * nx + ny * ny + nz * nz).sqrt();
                base.extend_from_slice(&[
                    quantize(((nx / length) * 0.5 + 0.5) * 255.0),
                    quantize(((ny / length) * 0.5 + 0.5) * 255.0),
                    quantize(((nz / length) * 0.5 + 0.5) * 255.0),
                    255,
                ]);
            }
        }
        let (data, levels) = mip_chain_rgba8(8, 8, &base, MipFilter::Normal);
        for level in 0..levels {
            let offset = (0..level)
                .map(|l| {
                    let w = (8u32 >> l).max(1);
                    (w as usize) * (w as usize) * 4
                })
                .sum::<usize>();
            let texels = (8u32 >> level).max(1) as usize;
            for texel in data[offset..offset + texels * texels * 4].chunks_exact(4) {
                let n = [
                    texel[0] as f32 * (2.0 / 255.0) - 1.0,
                    texel[1] as f32 * (2.0 / 255.0) - 1.0,
                    texel[2] as f32 * (2.0 / 255.0) - 1.0,
                ];
                let length = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
                assert!(
                    (length - 1.0).abs() < 0.02,
                    "level {level} normal length {length}"
                );
            }
        }
    }

    #[test]
    fn generation_is_deterministic() {
        let base = (0..32 * 16 * 4)
            .map(|i| (i * 7 % 251) as u8)
            .collect::<Vec<_>>();
        let first = mip_chain_rgba8(32, 16, &base, MipFilter::Color);
        let second = mip_chain_rgba8(32, 16, &base, MipFilter::Color);
        assert_eq!(first, second);
    }
}
