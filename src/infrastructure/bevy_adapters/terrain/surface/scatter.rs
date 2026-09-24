//! Merged per-patch scatter meshes: low-poly vegetation, rocks, and the
//! drainage river ribbon. Presentation only; never feeds collision or physics.

use super::surface_maps::{micro_noise, papua_tropical_profile};
use super::{
    direction_to_lat_lon, face_uv_to_direction, BOULDER_RINGS, BOULDER_SEGMENTS, BROADLEAF_UV,
    CANOPY_CARD_PLANES, CONIFER_UV, GRASS_CARD_PLANES, GRASS_CLUMP_COUNT, GRASS_MIN_DENSITY,
    GRASS_UV, OPAQUE_UV, RIVER_MIN_STRENGTH, RIVER_SURFACE_OFFSET_M, ROCK_COUNT, ROCK_MAX_LUMPS,
    SCATTER_FULL_DENSITY_LEVEL, TREE_COUNT, TREE_MIN_DENSITY, TRUNK_SEGMENTS,
};
use crate::domain::services::cube_sphere::{PatchGeometry, TerrainPatch};
use crate::domain::services::terrain_collision::surface_normal;
use crate::domain::services::terrain_source::{slope_deg_at, TerrainSource};
use bevy::asset::RenderAssetUsages;
use bevy::math::DVec3;
use bevy_mesh::{Indices, Mesh, PrimitiveTopology};

pub(super) struct MeshAccum {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    colors: Vec<[f32; 4]>,
    uvs: Vec<[f32; 2]>,
    indices: Vec<u32>,
}

impl MeshAccum {
    pub(super) fn new() -> Self {
        Self {
            positions: Vec::new(),
            normals: Vec::new(),
            colors: Vec::new(),
            uvs: Vec::new(),
            indices: Vec::new(),
        }
    }

    /// Push a vertical prism (cylinder/cone) whose axis is `up` at `base`.
    /// `r0`/`r1` are bottom/top radii, `height` the axis length. `color` is the
    /// linear vertex color. `segments` controls tessellation.
    #[expect(
        clippy::too_many_arguments,
        reason = "The mesh helper accepts the complete prism geometry and material inputs."
    )]
    fn push_prism(
        &mut self,
        base: DVec3,
        up: DVec3,
        r0: f64,
        r1: f64,
        height: f64,
        segments: usize,
        color: [f32; 3],
    ) {
        // Orthonormal tangent basis around `up`.
        let ref_axis = if up.y.abs() < 0.9 { DVec3::Y } else { DVec3::X };
        let tangent = up.cross(ref_axis).normalize();
        let bitangent = up.cross(tangent).normalize();

        let start = self.positions.len() as u32;
        let top = base + up * height;

        for s in 0..=segments {
            let a = s as f64 / segments as f64 * std::f64::consts::TAU;
            let ca = a.cos();
            let sa = a.sin();
            let radial = tangent * ca + bitangent * sa;
            let p0 = base + radial * r0;
            let p1 = top + radial * r1;
            // A tapered prism normal needs an axial component. A radial-only
            // cone normal produces a visibly incorrect highlight.
            let side_normal = (radial + up * ((r0 - r1) / height.max(f64::EPSILON))).normalize();
            self.positions.push([p0.x as f32, p0.y as f32, p0.z as f32]);
            self.positions.push([p1.x as f32, p1.y as f32, p1.z as f32]);
            self.normals.push([
                side_normal.x as f32,
                side_normal.y as f32,
                side_normal.z as f32,
            ]);
            self.normals.push([
                side_normal.x as f32,
                side_normal.y as f32,
                side_normal.z as f32,
            ]);
            self.colors.push([color[0], color[1], color[2], 1.0]);
            self.colors.push([color[0], color[1], color[2], 1.0]);
            self.uvs.push(OPAQUE_UV);
            self.uvs.push(OPAQUE_UV);
        }

        for s in 0..segments {
            let a0 = start + (2 * s) as u32;
            let a1 = start + (2 * s + 1) as u32;
            let b0 = start + (2 * s + 2) as u32;
            let b1 = start + (2 * s + 3) as u32;
            self.indices.extend_from_slice(&[a0, b0, a1, a1, b0, b1]);
        }
    }

    /// Push a low-poly boulder: a lumpy lump (random radial jitter per vertex).
    pub(super) fn push_boulder(
        &mut self,
        center: DVec3,
        up: DVec3,
        radius: f64,
        seed: u64,
        color: [f32; 3],
    ) {
        let ref_axis = if up.y.abs() < 0.9 { DVec3::Y } else { DVec3::X };
        let tangent = up.cross(ref_axis).normalize();
        let bitangent = up.cross(tangent).normalize();
        let segments = BOULDER_SEGMENTS;
        let rings = BOULDER_RINGS;
        let start = self.positions.len() as u32;

        for r in 0..=rings {
            let phi = (r as f64 / rings as f64) * std::f64::consts::PI;
            let y = phi.cos();
            let ring_r = phi.sin() * radius;
            for s in 0..segments {
                let a = s as f64 / segments as f64 * std::f64::consts::TAU;
                let jitter = 0.7 + micro_noise(seed as f64 + a, y, r as f64) * 0.5;
                let ca = a.cos();
                let sa = a.sin();
                let dir = tangent * ca + bitangent * sa;
                let p = center + dir * ring_r * jitter + up * (y * radius * jitter);
                let n = (dir * ring_r + up * y).normalize();
                self.positions.push([p.x as f32, p.y as f32, p.z as f32]);
                self.normals.push([n.x as f32, n.y as f32, n.z as f32]);
                self.colors.push([color[0], color[1], color[2], 1.0]);
                self.uvs.push(OPAQUE_UV);
            }
        }
        for r in 0..rings {
            for s in 0..segments {
                let a = start + (r * segments + s) as u32;
                let b = start + (r * segments + (s + 1) % segments) as u32;
                let c = start + ((r + 1) * segments + s) as u32;
                let d = start + ((r + 1) * segments + (s + 1) % segments) as u32;
                self.indices.extend_from_slice(&[a, c, b, b, c, d]);
            }
        }
    }

    /// Push `planes` crossed, double-sided vertical billboards sharing a base.
    /// `uv` selects the atlas region; the region's top maps to the billboard's
    /// top. Used for grass tufts and tree canopies.
    #[expect(
        clippy::too_many_arguments,
        reason = "The billboard helper accepts the complete plant geometry and atlas inputs."
    )]
    fn push_cross_cards(
        &mut self,
        base: DVec3,
        up: DVec3,
        width_m: f64,
        height_m: f64,
        planes: usize,
        rotation_rad: f64,
        uv: [f32; 4],
        color: [f32; 3],
    ) {
        if planes == 0 || width_m <= 0.0 || height_m <= 0.0 {
            return;
        }
        let reference = if up.y.abs() < 0.9 { DVec3::Y } else { DVec3::X };
        let tangent = up.cross(reference).normalize();
        let bitangent = up.cross(tangent).normalize();
        let [u0, v0, u1, v1] = uv;
        for plane in 0..planes {
            // Billboards are double-sided, so spreading planes over half a turn
            // already covers every viewing direction.
            let angle = rotation_rad + plane as f64 * std::f64::consts::PI / planes as f64;
            let across = tangent * angle.cos() + bitangent * angle.sin();
            let left = base - across * width_m * 0.5;
            let right = base + across * width_m * 0.5;
            let top = up * height_m;
            let normal = across.cross(up).normalize();
            let corners = [
                (left, [u0, v1]),
                (right, [u1, v1]),
                (right + top, [u1, v0]),
                (left + top, [u0, v0]),
            ];
            let start = self.positions.len() as u32;
            for (point, uv) in corners {
                self.positions
                    .push([point.x as f32, point.y as f32, point.z as f32]);
                self.normals
                    .push([normal.x as f32, normal.y as f32, normal.z as f32]);
                self.colors.push([color[0], color[1], color[2], 1.0]);
                self.uvs.push(uv);
            }
            self.indices.extend_from_slice(&[
                start,
                start + 1,
                start + 2,
                start,
                start + 2,
                start + 3,
            ]);
        }
    }

    pub(super) fn into_mesh(self) -> Mesh {
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, self.colors);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs);
        mesh.insert_indices(Indices::U32(self.indices));
        mesh
    }
}

/// Deterministically hash patch coordinates + index `k` to a stable pseudo-random
/// [0,1) value (used for scatter placement so it never depends on frame order).
fn hash01(a: u64, b: u64, c: u64) -> f64 {
    let mut h =
        a ^ (b.wrapping_mul(0x9E37_79B9_7F4A_7C15)) ^ (c.wrapping_mul(0xBF58_476D_1CE4_E5B9));
    h ^= h >> 30;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 27;
    h = h.wrapping_mul(0x94D0_49BB_1331_11EB);
    h ^= h >> 31;
    (h & 0xFFFF_FFFF_FFFF) as f64 / 0x1_0000_0000_0000u64 as f64
}

/// Coarse leaves decimate scatter to keep generation and draw sizes bounded.
/// Beyond the full-density level, divide by area to retain that target density.
pub(super) fn scatter_count_for_level(max_count: usize, patch_level: u32) -> usize {
    let level_delta = patch_level.saturating_sub(SCATTER_FULL_DENSITY_LEVEL);
    max_count
        .checked_shr(level_delta.saturating_mul(2))
        .unwrap_or(0)
        .max(1)
}

/// Build the merged vegetation/scatter mesh for a patch, or `None` if the patch
/// is not vegetated (water, snow, bare rock, or too steep). Positions are in the
/// body-fixed offsets from `mesh_origin_body_fixed`. Heights use the terrain's
/// LOD field; grounding against the triangulated mesh is still approximate.
pub fn build_vegetation_mesh(
    source: &dyn TerrainSource,
    patch: &TerrainPatch,
    radius_m: f64,
    mesh_origin_body_fixed: &DVec3,
) -> Option<Mesh> {
    let (u0, v0, u1, v1) = patch.uv_bounds();

    let mut accum = MeshAccum::new();

    for k in 0..scatter_count_for_level(TREE_COUNT, patch.level) {
        let ru = hash01(
            patch.face as u64,
            (patch.tile_x as u64) ^ (k as u64 * 2_654_355_561),
            (patch.tile_y as u64) ^ (k as u64 * 4_478_569),
        );
        let rv = hash01(
            (patch.level as u64) ^ (k as u64 * 3_141_592_653),
            patch.tile_x as u64,
            patch.tile_y as u64,
        );
        let u = u0 + (u1 - u0) * ru;
        let v = v0 + (v1 - v0) * rv;
        let dir = face_uv_to_direction(patch.face, u, v);
        let (lat, lon) = direction_to_lat_lon(dir);
        // Match the mesh's LOD-faded field rather than its full-detail height.
        let h = source.mesh_height_m(lat, lon, patch.level);
        if h < 0.5 {
            continue;
        }
        let local_slope = slope_deg_at(source, lat, lon);
        if local_slope > 34.0 {
            continue;
        }
        // Land cover is an explicit climate signal, not the greenness of the
        // synthesized albedo. Candidates are thinned by density so wet forest is
        // dense and dry or cold ground is sparse.
        let density = source.vegetation_density(lat, lon);
        if density < TREE_MIN_DENSITY
            || hash01(k as u64, patch.face as u64, patch.tile_y as u64) > density
        {
            continue;
        }
        let profile = papua_tropical_profile(lat, lon, h, source.moisture(lat, lon), local_slope);
        let flight = dir * (radius_m + h) - *mesh_origin_body_fixed;
        let up = surface_normal(source, lat, lon, radius_m);
        // Vary tree size a little.
        let scale = 0.7 + hash01(k as u64, patch.tile_x as u64, patch.tile_y as u64) * 0.9;
        let trunk_h = 2.2 * scale;
        let trunk_r = 0.18 * scale;
        let canopy_h = 4.0 * scale;
        let canopy_w = 3.1 * scale;
        let base = flight;
        let foliage_tint = 0.8 + hash01(k as u64, patch.tile_y as u64, patch.tile_x as u64) * 0.3;
        let trunk_color = [0.16f32, 0.07f32, 0.025f32];
        let tropical = profile.wet_vegetation_unit as f32;
        let foliage_color = [
            (0.035 + tropical * 0.01) * foliage_tint as f32,
            (0.19 + tropical * 0.09) * foliage_tint as f32,
            (0.022 + tropical * 0.025) * foliage_tint as f32,
        ];
        accum.push_prism(
            base,
            up,
            trunk_r,
            trunk_r * 0.8,
            trunk_h,
            TRUNK_SEGMENTS,
            trunk_color,
        );
        // Wet, low, warm sites read as broadleaf; higher/cooler or drier sites
        // read as conifer. The choice is deterministic from the source samples.
        let broadleaf = source.moisture(lat, lon) > 0.45 && h < 2_200.0;
        let canopy_uv = if broadleaf { BROADLEAF_UV } else { CONIFER_UV };
        let rotation = hash01(k as u64, patch.tile_x as u64, 7) * std::f64::consts::TAU;
        let lower_base = base + up * trunk_h;
        accum.push_cross_cards(
            lower_base,
            up,
            canopy_w,
            canopy_h * 0.75,
            CANOPY_CARD_PLANES,
            rotation,
            canopy_uv,
            foliage_color,
        );
        let upper_base = lower_base + up * canopy_h * 0.42;
        accum.push_cross_cards(
            upper_base,
            up,
            canopy_w * 0.72,
            canopy_h * 0.68,
            CANOPY_CARD_PLANES,
            rotation + std::f64::consts::FRAC_PI_3,
            canopy_uv,
            foliage_color,
        );
    }

    for k in 0..scatter_count_for_level(GRASS_CLUMP_COUNT, patch.level) {
        let ru = hash01(
            patch.face as u64 ^ 0xCAFE_BABE,
            patch.tile_x as u64,
            (patch.tile_y as u64).wrapping_add(k as u64),
        );
        let rv = hash01(
            patch.level as u64,
            patch.tile_y as u64 ^ 0x0A11_CE55,
            (patch.tile_x as u64).wrapping_add(k as u64),
        );
        let u = u0 + (u1 - u0) * ru;
        let v = v0 + (v1 - v0) * rv;
        let dir = face_uv_to_direction(patch.face, u, v);
        let (lat, lon) = direction_to_lat_lon(dir);
        let h = source.mesh_height_m(lat, lon, patch.level);
        let slope_deg = slope_deg_at(source, lat, lon);
        let density = source.vegetation_density(lat, lon);
        if h < 0.5
            || slope_deg > 30.0
            || density < GRASS_MIN_DENSITY
            || hash01(k as u64, patch.face as u64 ^ 0x6A11, patch.tile_x as u64) > density
        {
            continue;
        }
        let profile = papua_tropical_profile(lat, lon, h, source.moisture(lat, lon), slope_deg);
        let base = dir * (radius_m + h) - *mesh_origin_body_fixed;
        let up = surface_normal(source, lat, lon, radius_m);
        let scale = 0.55 + hash01(k as u64, patch.tile_x as u64, patch.tile_y as u64) * 0.65;
        let grass_color = [
            0.045 + profile.wet_vegetation_unit as f32 * 0.01,
            0.24 + profile.wet_vegetation_unit as f32 * 0.08,
            0.025 + profile.wet_vegetation_unit as f32 * 0.02,
        ];
        accum.push_cross_cards(
            base,
            up,
            0.55 * scale,
            0.6 * scale,
            GRASS_CARD_PLANES,
            hash01(patch.tile_x as u64, patch.tile_y as u64, k as u64) * std::f64::consts::TAU,
            GRASS_UV,
            grass_color,
        );
    }

    for k in 0..scatter_count_for_level(ROCK_COUNT, patch.level) {
        let ru = hash01(
            (patch.face as u64) ^ (k as u64 * 7_919),
            patch.tile_y as u64,
            patch.tile_x as u64,
        );
        let rv = hash01(
            patch.tile_x as u64 ^ (k as u64 * 1_009),
            patch.level as u64,
            patch.tile_y as u64,
        );
        let u = u0 + (u1 - u0) * ru;
        let v = v0 + (v1 - v0) * rv;
        let dir = face_uv_to_direction(patch.face, u, v);
        let (lat, lon) = direction_to_lat_lon(dir);
        let h = source.mesh_height_m(lat, lon, patch.level);
        if h < 0.5 {
            continue;
        }
        let slope_deg = slope_deg_at(source, lat, lon);
        let moisture = source.moisture(lat, lon);
        // Exposed rock concentrates on steeper ground, but scree and outcrops
        // still occur on gentle terrain, so only a fraction of flat slots drop.
        let exposed = ((slope_deg - 8.0) / 24.0).clamp(0.0, 1.0);
        if exposed < 0.15
            && hash01(k as u64, patch.tile_x as u64, patch.tile_y as u64 ^ 0x5EED) > 0.55
        {
            continue;
        }

        let flight = dir * (radius_m + h) - *mesh_origin_body_fixed;
        // Damp, sheltered rock is darker and moss-tinged; dry rock is pale grey.
        let moss = ((moisture - 0.35).max(0.0) * (1.0 - (slope_deg / 45.0).clamp(0.0, 1.0)))
            .clamp(0.0, 1.0);
        let tone = 0.30 + hash01(k as u64, patch.tile_y as u64, 11) * 0.22;
        let rock_color = [
            (tone * (1.0 - moss) + 0.05 * moss) as f32,
            (tone * (1.0 - moss) + 0.15 * moss) as f32,
            (tone * (1.0 - moss) + 0.04 * moss) as f32,
        ];

        let base_radius = 0.35 + hash01(k as u64, patch.tile_x as u64, 3) * 1.5;
        let reference = if dir.y.abs() < 0.9 {
            DVec3::Y
        } else {
            DVec3::X
        };
        let tangent = dir.cross(reference).normalize();
        let bitangent = dir.cross(tangent).normalize();
        let lumps = 1
            + (hash01(k as u64, patch.tile_x as u64 ^ 0xABCD, patch.tile_y as u64)
                * ROCK_MAX_LUMPS as f64)
                .floor() as usize;
        for lump in 0..lumps.min(ROCK_MAX_LUMPS) {
            let angle = hash01(
                (lump as u64) ^ (k as u64 * 2_654_355_561),
                patch.tile_y as u64,
                patch.tile_x as u64,
            ) * std::f64::consts::TAU;
            let offset = (tangent * angle.cos() + bitangent * angle.sin())
                * base_radius
                * 0.8
                * hash01(lump as u64, patch.tile_x as u64, patch.tile_y as u64);
            let lump_radius = base_radius
                * (0.45 + hash01(lump as u64, patch.tile_y as u64, patch.tile_x as u64) * 0.65)
                / (lump as f64 + 1.0).sqrt();
            accum.push_boulder(
                flight + offset,
                dir,
                lump_radius,
                0x1234_5678 ^ (k as u64 * 2_654_355_561) ^ (lump as u64 * 0x9E37),
                rock_color,
            );
        }
    }

    if accum.positions.is_empty() {
        None
    } else {
        Some(accum.into_mesh())
    }
}

/// Build a water ribbon that follows a patch's drainage network, or `None` when
/// no channel crosses it. Vertices are body-fixed offsets from `anchor`, in the
/// same local frame as the vegetation mesh, and sit just above the sampled
/// terrain so the water hugs the channel. Presentation only: it never feeds
/// collision, radar altitude, or any authoritative terrain sample.
pub fn build_river_mesh(
    source: &dyn TerrainSource,
    geometry: &PatchGeometry,
    anchor_body_fixed: &DVec3,
) -> Option<Mesh> {
    // Core grid vertices precede the skirt ring: `n^2 + 4*(n-1)`.
    let resolution = ((geometry.positions.len() + 8) as f64).sqrt() as usize - 2;
    if resolution < 2 {
        return None;
    }
    let core = resolution * resolution;
    if geometry.positions.len() < core {
        return None;
    }

    let mut strengths = vec![0.0f32; core];
    let mut positions = Vec::with_capacity(core);
    let mut normals = Vec::with_capacity(core);
    let mut crosses_channel = false;
    for (index, point) in geometry.positions[..core].iter().enumerate() {
        let position = DVec3::from_array(*point);
        let radius = position.length();
        let radial = if radius > f64::EPSILON {
            position / radius
        } else {
            DVec3::Y
        };
        let (lat, lon) = direction_to_lat_lon(radial);
        let strength = source.river_strength(lat, lon).clamp(0.0, 1.0) as f32;
        strengths[index] = strength;
        crosses_channel |= f64::from(strength) >= RIVER_MIN_STRENGTH;
        let surface = radial * (radius + RIVER_SURFACE_OFFSET_M) - *anchor_body_fixed;
        positions.push(surface.as_vec3().to_array());
        normals.push(radial.as_vec3().to_array());
    }
    if !crosses_channel {
        return None;
    }

    let mut indices: Vec<u32> = Vec::new();
    for row in 0..resolution - 1 {
        for column in 0..resolution - 1 {
            let top_left = (row * resolution + column) as u32;
            let top_right = top_left + 1;
            let bottom_left = ((row + 1) * resolution + column) as u32;
            let bottom_right = bottom_left + 1;
            let touches_channel = [top_left, top_right, bottom_left, bottom_right]
                .into_iter()
                .any(|index| f64::from(strengths[index as usize]) >= RIVER_MIN_STRENGTH);
            if !touches_channel {
                continue;
            }
            indices.extend_from_slice(&[
                top_left,
                top_right,
                bottom_right,
                top_left,
                bottom_right,
                bottom_left,
            ]);
        }
    }
    if indices.is_empty() {
        return None;
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, geometry.uvs[..core].to_vec());
    // The water shader reads the red channel as its shallow/deep ramp, so the
    // drainage strength drives river colour and opacity.
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_COLOR,
        strengths
            .iter()
            .map(|strength| [*strength, 0.0, 0.0, 1.0])
            .collect::<Vec<_>>(),
    );
    mesh.insert_indices(Indices::U32(indices));
    Some(mesh)
}
