//! Merged per-patch scatter meshes: low-poly vegetation, rocks, and the
//! drainage river ribbon. Presentation only; never feeds collision or physics.

use super::surface_maps::micro_noise;
use super::{
    direction_to_lat_lon, face_uv_to_direction, BROADLEAF_UV, CANOPY_CARD_PLANES, CONIFER_UV,
    GRASS_CARD_PLANES, GRASS_CLUMP_COUNT, GRASS_MIN_DENSITY, GRASS_UV, OPAQUE_UV,
    RIVER_MIN_STRENGTH, RIVER_SURFACE_OFFSET_M, ROCK_COUNT, ROCK_MAX_LUMPS, ROCK_RINGS,
    ROCK_SEGMENTS, SCATTER_FULL_DENSITY_LEVEL, TREE_COUNT, TREE_MIN_DENSITY, TRUNK_SEGMENTS,
};
use crate::domain::services::cube_sphere::{patch_world_size_m, PatchGeometry, TerrainPatch};
use crate::domain::services::land_cover::LandCoverPackage;
use crate::domain::services::terrain_collision::surface_normal;
use crate::domain::services::terrain_source::{slope_deg_at, TerrainSource};
use crate::domain::services::vegetation::{
    clump_mask, combined_cover_density, embed_depth_m, scatter_hash01 as hash01, select_species,
    vegetation_candidates, VegetationSpecies, GRASS_CANDIDATE_SALT, TREE_CANDIDATE_SALT,
};
use bevy::asset::RenderAssetUsages;
use bevy::math::DVec3;
use bevy_mesh::{Indices, Mesh, PrimitiveTopology};

/// Blue-noise candidate oversampling: generate more jittered-grid cells than the
/// accepted rock budget so slope weighting can select a well-spread subset.
const ROCK_CANDIDATE_OVERSAMPLE: usize = 2;
/// Fraction of a grid cell used for the deterministic jitter. The remaining
/// margin keeps neighbouring candidates separated, approximating blue noise.
const ROCK_CELL_JITTER: f64 = 0.35;
/// Base acceptance on flat ground plus the slope-scaled weight. Acceptance rises
/// monotonically with slope so scree concentrates on steep faces.
const ROCK_ACCEPT_BASE: f64 = 0.05;
const ROCK_ACCEPT_SLOPE_WEIGHT: f64 = 0.90;
/// Low-frequency asymmetry plus bounded high-frequency fracture relative to the
/// base radius. The clamped displacement keeps the rock star-shaped so embedding
/// stays valid on every face.
const ROCK_LOW_FREQ_AMP: f64 = 0.22;
const ROCK_HIGH_FREQ_AMP: f64 = 0.10;
const ROCK_MAX_DISPLACEMENT: f64 = 0.28;
/// Minimum contact-occlusion factor at the embedded base (crown stays unmodified).
const ROCK_CONTACT_OCCLUSION_MIN: f32 = 0.45;
/// Bounded thermal relaxation: passes and talus trigger, toggled per rock.
const ROCK_THERMAL_PASSES: usize = 1;
const ROCK_TALUS_DEG: f64 = 55.0;

/// One procedurally generated rock body before meshing. Presentation only; it
/// never feeds collision, altitude, landing, or any physics quantity.
#[derive(Debug, Clone, Copy)]
pub(super) struct RockBody {
    pub(super) center: DVec3,
    pub(super) up: DVec3,
    pub(super) radius: f64,
    pub(super) aspect: DVec3,
    pub(super) seed: u64,
    pub(super) color: [f32; 3],
}

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

    /// Push one procedurally generated rock body into the merged mesh.
    ///
    /// A closed ring/segment base shape is displaced along each vertex's radial
    /// direction by seeded fBm (low-frequency asymmetry plus bounded
    /// high-frequency fracture), optionally relaxed by one bounded thermal pass,
    /// then scaled anisotropically. Normals are averaged from the deformed faces
    /// and vertex colour is darkened toward the embedded base so the rock reads
    /// as seated in the ground. Presentation only.
    pub(super) fn push_rock(&mut self, body: RockBody) {
        let RockBody {
            center,
            up,
            radius,
            aspect,
            seed,
            color,
        } = body;
        let reference = if up.y.abs() < 0.9 { DVec3::Y } else { DVec3::X };
        let tangent = up.cross(reference).normalize();
        let bitangent = up.cross(tangent).normalize();
        let segments = ROCK_SEGMENTS;
        let rings = ROCK_RINGS;

        // Undeformed base direction is the unit sphere point; displacement is
        // radial so the rock stays star-shaped and always embeddable.
        let mut local = Vec::with_capacity((rings + 1) * segments);
        for r in 0..=rings {
            let phi = r as f64 / rings as f64 * std::f64::consts::PI;
            let (sin_phi, cos_phi) = phi.sin_cos();
            for s in 0..segments {
                let azimuth = s as f64 / segments as f64 * std::f64::consts::TAU;
                let base = DVec3::new(sin_phi * azimuth.cos(), cos_phi, sin_phi * azimuth.sin());
                let displacement = ROCK_LOW_FREQ_AMP * rock_fbm(base, seed)
                    + ROCK_HIGH_FREQ_AMP * rock_fbm(base * 3.0, seed ^ 0x9E37_79B9);
                let factor = (1.0 + displacement)
                    .clamp(1.0 - ROCK_MAX_DISPLACEMENT, 1.0 + ROCK_MAX_DISPLACEMENT);
                local.push(base * (radius * factor));
            }
        }

        // Some rocks weather and smooth; others stay angular. The choice is
        // deterministic from the instance seed.
        if hash01(seed, 0x7A11, 0x5EED) > 0.5 {
            thermal_relax(
                &mut local,
                rings,
                segments,
                ROCK_THERMAL_PASSES,
                ROCK_TALUS_DEG,
            );
        }

        for point in &mut local {
            point.x *= aspect.x;
            point.y *= aspect.y;
            point.z *= aspect.z;
        }

        let normals = rock_normals(&local, rings, segments);
        let vertical_radius = (radius * aspect.y).max(f64::EPSILON);
        let start = self.positions.len() as u32;
        for (point, normal) in local.iter().zip(&normals) {
            let world = center + tangent * point.x + up * point.y + bitangent * point.z;
            let world_normal =
                (tangent * normal.x + up * normal.y + bitangent * normal.z).normalize_or_zero();
            self.positions
                .push([world.x as f32, world.y as f32, world.z as f32]);
            self.normals.push([
                world_normal.x as f32,
                world_normal.y as f32,
                world_normal.z as f32,
            ]);
            // Contact occlusion is evaluated from the normalised axial position,
            // so it is bounded independently of the rock's absolute size.
            let axial = (point.y / vertical_radius * 0.5 + 0.5).clamp(0.0, 1.0) as f32;
            let contact = ROCK_CONTACT_OCCLUSION_MIN + (1.0 - ROCK_CONTACT_OCCLUSION_MIN) * axial;
            self.colors.push([
                color[0] * contact,
                color[1] * contact,
                color[2] * contact,
                1.0,
            ]);
            self.uvs.push(OPAQUE_UV);
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

/// Deterministic rock-local fBm used to displace the undeformed base shape.
/// Reuses the existing `micro_noise` value noise with seed-derived offsets so a
/// fixed seed always produces the same rock.
fn rock_fbm(direction: DVec3, seed: u64) -> f64 {
    let offset_x = hash01(seed, 1, 0) * 64.0;
    let offset_y = hash01(seed, 2, 0) * 64.0;
    let offset_z = hash01(seed, 3, 0) * 64.0;
    let mut sum = 0.0;
    let mut weight = 0.0;
    let mut amplitude = 1.0;
    let mut frequency = 1.0;
    for octave in 0..3 {
        let noise = micro_noise(
            direction.x * frequency + offset_x + octave as f64 * 19.0,
            direction.y * frequency + offset_y - octave as f64 * 7.0,
            direction.z * frequency + offset_z + octave as f64 * 11.0,
        );
        sum += (noise * 2.0 - 1.0) * amplitude;
        weight += amplitude;
        amplitude *= 0.5;
        frequency *= 2.1;
    }
    sum / weight
}

/// Bounded deterministic thermal relaxation: vertices whose deformed surface
/// tilts more than `talus_deg` from radial are blended toward their grid
/// neighbours, softening spikes into weathered, talus-like forms. This is a
/// local vertex operation, not a terrain-scale erosion pass.
fn thermal_relax(
    local: &mut Vec<DVec3>,
    rings: usize,
    segments: usize,
    passes: usize,
    talus_deg: f64,
) {
    if rings < 2 || segments < 3 {
        return;
    }
    let idx = |r: usize, s: usize| r * segments + s % segments;
    for _ in 0..passes {
        let mut relaxed = local.clone();
        for r in 1..rings {
            for s in 0..segments {
                let point = local[idx(r, s)];
                let radial = point.normalize_or_zero();
                let tangent_r = local[idx(r + 1, s)] - local[idx(r - 1, s)];
                let tangent_s = local[idx(r, s + 1)] - local[idx(r, s + segments - 1)];
                let normal = tangent_s.cross(tangent_r).normalize_or_zero();
                if normal.length_squared() < 1e-12 || radial.length_squared() < 1e-12 {
                    continue;
                }
                let tilt_deg = normal.dot(radial).abs().clamp(0.0, 1.0).acos().to_degrees();
                if tilt_deg > talus_deg {
                    let average = (local[idx(r - 1, s)]
                        + local[idx(r + 1, s)]
                        + local[idx(r, s + 1)]
                        + local[idx(r, s + segments - 1)])
                        * 0.25;
                    relaxed[idx(r, s)] = point.lerp(average, 0.5);
                }
            }
        }
        *local = relaxed;
    }
}

/// Area-weighted per-vertex normals for the deformed ring/segment base shape.
/// Degenerate pole faces fall back to the radial direction.
fn rock_normals(local: &[DVec3], rings: usize, segments: usize) -> Vec<DVec3> {
    let mut normals = vec![DVec3::ZERO; local.len()];
    let idx = |r: usize, s: usize| r * segments + s % segments;
    for r in 0..rings {
        for s in 0..segments {
            let a = idx(r, s);
            let b = idx(r, s + 1);
            let c = idx(r + 1, s);
            let d = idx(r + 1, s + 1);
            for [i0, i1, i2] in [[a, c, b], [b, c, d]] {
                let face_normal = (local[i1] - local[i0]).cross(local[i2] - local[i0]);
                normals[i0] += face_normal;
                normals[i1] += face_normal;
                normals[i2] += face_normal;
            }
        }
    }
    for (normal, point) in normals.iter_mut().zip(local) {
        *normal = normal.normalize_or_zero();
        if normal.length_squared() < 1e-12 {
            *normal = point.normalize_or_zero();
        }
    }
    normals
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

/// Blue-noise acceptance probability for a rock candidate at `slope_deg`.
/// Monotonically increasing so scree concentrates on steeper ground while gentle
/// ground keeps only sparse outcrops.
pub(super) fn rock_acceptance_probability(slope_deg: f64) -> f64 {
    let exposed = ((slope_deg - 8.0) / 24.0).clamp(0.0, 1.0);
    ROCK_ACCEPT_BASE + ROCK_ACCEPT_SLOPE_WEIGHT * exposed
}

/// Deterministic jittered-grid blue-noise rock candidates over a patch's local
/// UV. `slope_at` supplies the authoritative slope for each candidate; acceptance
/// rises with slope, and the returned count never exceeds `budget`. The grid is
/// phased per patch (seeded from patch identity) so adjacent patches do not
/// expose a shared regular lattice at their edge.
pub(super) fn rock_candidates(
    patch: &TerrainPatch,
    budget: usize,
    mut slope_at: impl FnMut(f64, f64) -> f64,
) -> Vec<(f64, f64)> {
    if budget == 0 {
        return Vec::new();
    }
    let (u0, v0, u1, v1) = patch.uv_bounds();
    let side = ((budget * ROCK_CANDIDATE_OVERSAMPLE) as f64).sqrt().ceil() as usize;
    let side = side.max(1);
    let phase_u = hash01(
        patch.face as u64 ^ 0x51A7,
        patch.tile_x as u64,
        patch.tile_y as u64,
    );
    let phase_v = hash01(
        patch.face as u64 ^ 0x7E33,
        patch.tile_y as u64,
        patch.tile_x as u64,
    );

    let mut accepted = Vec::new();
    for gy in 0..side {
        for gx in 0..side {
            let cell = (gy * side + gx) as u64;
            let jitter_u = hash01(patch.face as u64, cell, patch.tile_x as u64 ^ 0x1F3D);
            let jitter_v = hash01(patch.face as u64 ^ 0x2C91, cell, patch.tile_y as u64);
            let grid_u = gx as f64 + 0.5 + (jitter_u - 0.5) * ROCK_CELL_JITTER;
            let grid_v = gy as f64 + 0.5 + (jitter_v - 0.5) * ROCK_CELL_JITTER;
            let gu = (grid_u / side as f64 + phase_u).rem_euclid(1.0);
            let gv = (grid_v / side as f64 + phase_v).rem_euclid(1.0);
            let u = u0 + (u1 - u0) * gu;
            let v = v0 + (v1 - v0) * gv;
            let probability = rock_acceptance_probability(slope_at(u, v));
            let roll = hash01(
                patch.face as u64 ^ 0x6D2B,
                cell,
                patch.tile_x as u64 ^ 0x4E1,
            );
            if roll < probability {
                accepted.push((u, v));
            }
        }
    }

    if accepted.len() > budget {
        // Thin evenly across the accepted set rather than truncating in grid
        // order, so saturated steep patches keep spatially even coverage.
        let total = accepted.len();
        accepted = (0..budget)
            .map(|index| accepted[index * total / budget])
            .collect();
    }
    accepted
}

/// Plan the deterministic rock bodies for one close-range patch. Reads height,
/// slope, normal, and moisture only from the shared `TerrainSource` and writes
/// nothing back. Each accepted blue-noise candidate emits a small cluster of
/// anisotropic procedural rocks embedded into the slope.
pub(super) fn plan_rock_bodies(
    source: &dyn TerrainSource,
    patch: &TerrainPatch,
    radius_m: f64,
    mesh_origin_body_fixed: &DVec3,
) -> Vec<RockBody> {
    let budget = scatter_count_for_level(ROCK_COUNT, patch.level);
    let candidates = rock_candidates(patch, budget, |u, v| {
        let dir = face_uv_to_direction(patch.face, u, v);
        let (lat, lon) = direction_to_lat_lon(dir);
        slope_deg_at(source, lat, lon)
    });

    let mut bodies = Vec::new();
    for (k, (u, v)) in candidates.into_iter().enumerate() {
        let dir = face_uv_to_direction(patch.face, u, v);
        let (lat, lon) = direction_to_lat_lon(dir);
        let height_m = source.mesh_height_m(lat, lon, patch.level);
        if height_m < 0.5 {
            continue;
        }
        let slope_deg = slope_deg_at(source, lat, lon);
        let moisture = source.moisture(lat, lon);
        let flight = dir * (radius_m + height_m) - *mesh_origin_body_fixed;
        let up = surface_normal(source, lat, lon, radius_m);
        let reference = if dir.y.abs() < 0.9 {
            DVec3::Y
        } else {
            DVec3::X
        };
        let tangent = dir.cross(reference).normalize();
        let bitangent = dir.cross(tangent).normalize();

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
        let slope_unit = (slope_deg / 45.0).clamp(0.0, 1.0);
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
            let rock_radius = base_radius
                * (0.55 + hash01(lump as u64, patch.tile_y as u64, patch.tile_x as u64) * 0.75)
                / (lump as f64 + 1.0).sqrt();
            // Non-uniform aspect: horizontal axes are longer than the vertical,
            // so adjacent rocks do not present an identical spherical footprint.
            let aspect = DVec3::new(
                0.85 + hash01(lump as u64, patch.tile_x as u64 ^ 0xA1, patch.tile_y as u64) * 0.55,
                0.50 + hash01(lump as u64, patch.tile_x as u64 ^ 0xB2, patch.tile_y as u64) * 0.40,
                0.85 + hash01(lump as u64, patch.tile_x as u64 ^ 0xC3, patch.tile_y as u64) * 0.55,
            );
            let vertical_radius = rock_radius * aspect.y;
            // Embed deeper on steeper ground, but clamp so the crown stays
            // exposed and the displaced base remains below the sampled surface.
            let embed = (rock_radius * (0.35 + 0.35 * slope_unit)).min(vertical_radius * 0.6);
            bodies.push(RockBody {
                center: flight + offset + up * (vertical_radius - embed),
                up,
                radius: rock_radius,
                aspect,
                seed: 0x1234_5678 ^ (k as u64 * 2_654_355_561) ^ (lump as u64 * 0x9E37),
                color: rock_color,
            });
        }
    }
    bodies
}

/// Foliage reflectance per species, biased greener in wetter sites. Linear
/// broadband values keep vegetation grounded rather than emissive-looking.
fn species_foliage_color(species: VegetationSpecies, moisture: f64, tint: f64) -> [f32; 3] {
    let wet = moisture.clamp(0.0, 1.0) as f32;
    let (r, g, b) = match species {
        VegetationSpecies::Conifer => (0.020, 0.130, 0.030),
        VegetationSpecies::Palm => (0.055, 0.260, 0.065),
        VegetationSpecies::TropicalBroadleaf => (0.040, 0.220, 0.035),
        VegetationSpecies::TemperateBroadleaf => (0.060, 0.190, 0.045),
        VegetationSpecies::Shrub => (0.090, 0.160, 0.050),
        VegetationSpecies::Grass => (0.045, 0.240, 0.025),
    };
    let tint = tint as f32;
    [
        (r + wet * 0.010) * tint,
        (g + wet * 0.050) * tint,
        (b + wet * 0.010) * tint,
    ]
}

/// Build the merged vegetation/scatter mesh for a patch, or `None` if the patch
/// is not vegetated (water, snow, bare rock, or too steep). Positions are in the
/// body-fixed offsets from `mesh_origin_body_fixed`. Plant bases are embedded
/// into the sampled slope and aligned to the surface normal, so grounding
/// matches the terrain rather than floating on the LOD field.
pub fn build_vegetation_mesh(
    source: &dyn TerrainSource,
    patch: &TerrainPatch,
    radius_m: f64,
    mesh_origin_body_fixed: &DVec3,
) -> Option<Mesh> {
    build_vegetation_mesh_with_land_cover(source, patch, radius_m, mesh_origin_body_fixed, None)
}

/// As [`build_vegetation_mesh`], but combines the source's climate density with
/// an optional measured land-cover package. Measured cover refines placement
/// and species where it is available and falls back deterministically to the
/// climate density otherwise. Presentation only: it never feeds collision.
pub fn build_vegetation_mesh_with_land_cover(
    source: &dyn TerrainSource,
    patch: &TerrainPatch,
    radius_m: f64,
    mesh_origin_body_fixed: &DVec3,
    land_cover: Option<&LandCoverPackage>,
) -> Option<Mesh> {
    let density_at = |lat: f64, lon: f64| {
        combined_cover_density(
            source.vegetation_density(lat, lon),
            land_cover.and_then(|package| package.vegetation_density(lat, lon)),
        )
    };
    let mut accum = MeshAccum::new();

    let tree_budget = scatter_count_for_level(TREE_COUNT, patch.level);
    let tree_sites = vegetation_candidates(patch, tree_budget, TREE_CANDIDATE_SALT, |u, v| {
        let dir = face_uv_to_direction(patch.face, u, v);
        let (lat, lon) = direction_to_lat_lon(dir);
        let h = source.mesh_height_m(lat, lon, patch.level);
        if h < 0.5 {
            return false;
        }
        let slope = slope_deg_at(source, lat, lon);
        if slope > 34.0 {
            return false;
        }
        // Candidates are thinned by the climate cover multiplied by a
        // low-frequency clumping mask, so wet forest is dense, dry ground is
        // sparse, and clearings open naturally rather than uniformly.
        density_at(lat, lon) * clump_mask(lat, lon) >= TREE_MIN_DENSITY
    });
    // In-species minimum spacing in face UV, derived from the species' physical
    // spacing and the patch's world size. Canopy species use larger spacing than
    // understory shrubs.
    let patch_size_m = patch_world_size_m(patch.level, radius_m).max(1.0);
    let mut placed: Vec<(VegetationSpecies, f64, f64)> = Vec::new();
    for (k, (u, v)) in tree_sites.into_iter().enumerate() {
        let dir = face_uv_to_direction(patch.face, u, v);
        let (lat, lon) = direction_to_lat_lon(dir);
        let h = source.mesh_height_m(lat, lon, patch.level);
        let local_slope = slope_deg_at(source, lat, lon);
        let moisture = source.moisture(lat, lon);
        let density = density_at(lat, lon) * clump_mask(lat, lon);
        let Some(species) = select_species(lat, h, moisture, local_slope, density) else {
            continue;
        };
        if !species.is_tree() {
            continue;
        }
        let profile = species.profile();
        let spacing_uv = profile.min_spacing_m / patch_size_m;
        if placed.iter().any(|(other, other_u, other_v)| {
            *other == species && ((u - other_u).powi(2) + (v - other_v).powi(2)).sqrt() < spacing_uv
        }) {
            continue;
        }
        placed.push((species, u, v));
        let flight = dir * (radius_m + h) - *mesh_origin_body_fixed;
        let up = surface_normal(source, lat, lon, radius_m);
        // Grounding: sink the base a fraction of the trunk into the slope so a
        // tree never floats on a hillside, and align its up axis to the surface
        // normal rather than the radial.
        let embed_m = embed_depth_m(profile);
        let base = flight - up * embed_m;
        let scale = 0.7 + hash01(k as u64, patch.tile_x as u64, patch.tile_y as u64) * 0.7;
        let trunk_h = profile.trunk_height_m * scale;
        let trunk_r = profile.trunk_radius_m * scale;
        let canopy_h = profile.canopy_height_m * scale;
        let canopy_w = profile.canopy_width_m * scale;
        let trunk_color = [0.16f32, 0.07f32, 0.025f32];
        let tint = 0.85 + hash01(k as u64, patch.tile_y as u64, patch.tile_x as u64) * 0.3;
        let foliage_color = species_foliage_color(species, moisture, tint);
        let canopy_uv = match species {
            VegetationSpecies::Conifer => CONIFER_UV,
            _ => BROADLEAF_UV,
        };
        accum.push_prism(
            base,
            up,
            trunk_r,
            trunk_r * 0.8,
            trunk_h,
            TRUNK_SEGMENTS,
            trunk_color,
        );
        // Stack the species' canopy layers with diminishing size so broadleaf
        // trees read round while conifers read tall and narrow.
        let layers = profile.canopy_layers.max(1) as usize;
        let rotation = hash01(k as u64, patch.tile_x as u64, 7) * std::f64::consts::TAU;
        for layer in 0..layers {
            let t = layer as f64 / layers as f64;
            let layer_scale = 1.0 - t * 0.32;
            let layer_base = base + up * (trunk_h + canopy_h * 0.18 * layer as f64);
            accum.push_cross_cards(
                layer_base,
                up,
                canopy_w * layer_scale,
                canopy_h * (0.82 - t * 0.22),
                CANOPY_CARD_PLANES,
                rotation + layer as f64 * std::f64::consts::FRAC_PI_3,
                canopy_uv,
                foliage_color,
            );
        }
    }

    let grass_budget = scatter_count_for_level(GRASS_CLUMP_COUNT, patch.level);
    let grass_sites = vegetation_candidates(patch, grass_budget, GRASS_CANDIDATE_SALT, |u, v| {
        let dir = face_uv_to_direction(patch.face, u, v);
        let (lat, lon) = direction_to_lat_lon(dir);
        let h = source.mesh_height_m(lat, lon, patch.level);
        if h < 0.5 {
            return false;
        }
        let slope = slope_deg_at(source, lat, lon);
        if slope > 30.0 {
            return false;
        }
        density_at(lat, lon) * clump_mask(lat, lon) >= GRASS_MIN_DENSITY
    });

    for (k, (u, v)) in grass_sites.into_iter().enumerate() {
        let dir = face_uv_to_direction(patch.face, u, v);
        let (lat, lon) = direction_to_lat_lon(dir);
        let h = source.mesh_height_m(lat, lon, patch.level);
        let moisture = source.moisture(lat, lon);
        let up = surface_normal(source, lat, lon, radius_m);
        let scale = 0.55 + hash01(k as u64, patch.tile_x as u64, patch.tile_y as u64) * 0.65;
        let grass_color = species_foliage_color(VegetationSpecies::Grass, moisture, 1.0);
        // Embed the tuft base slightly so ground cover follows the slope.
        let base = dir * (radius_m + h) - *mesh_origin_body_fixed - up * (0.05 * scale);
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

    for body in plan_rock_bodies(source, patch, radius_m, mesh_origin_body_fixed) {
        accum.push_rock(body);
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

    // Sample the authoritative hydrology once per grid vertex. The strength is
    // the normalized discharge signal exposed by the terrain authority, so the
    // channel width follows the same field that carved the drainage network.
    let mut strengths = vec![0.0f32; core];
    let mut radials = vec![DVec3::Y; core];
    let mut radii = vec![0.0f64; core];
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
        radials[index] = radial;
        radii[index] = radius;
        crosses_channel |= f64::from(strength) >= RIVER_MIN_STRENGTH;
    }
    if !crosses_channel {
        return None;
    }

    let threshold = RIVER_MIN_STRENGTH as f32;
    // Non-indexed ribbon: each cell duplicates its corners so weak corners can
    // be pulled toward the channel core without dragging shared neighbors.
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut uvs: Vec<[f32; 2]> = Vec::new();
    let mut colors: Vec<[f32; 4]> = Vec::new();

    for row in 0..resolution - 1 {
        for column in 0..resolution - 1 {
            let corners = [
                row * resolution + column,
                row * resolution + column + 1,
                (row + 1) * resolution + column,
                (row + 1) * resolution + column + 1,
            ];
            let max_strength = corners
                .iter()
                .map(|index| strengths[*index])
                .fold(0.0f32, f32::max);
            if max_strength < threshold {
                continue;
            }
            // The strongest corner anchors the channel core for this cell.
            let strongest = *corners
                .iter()
                .max_by(|a, b| strengths[**a].total_cmp(&strengths[**b]))
                .expect("a 2x2 cell always has a strongest corner");
            let core_direction = radials[strongest];
            let core_radius = radii[strongest];

            // Flow runs along the channel, i.e. along the strength isoline
            // (perpendicular to the strength gradient). Encode the normalized
            // direction in the vertex-colour green/blue channels so the water
            // shader can advect its ripple along the flow.
            let strong_row = strongest / resolution;
            let strong_col = strongest % resolution;
            let sample = |row: isize, col: isize| -> f32 {
                let row = row.clamp(0, resolution as isize - 1) as usize;
                let col = col.clamp(0, resolution as isize - 1) as usize;
                strengths[row * resolution + col]
            };
            let grad_col = sample(strong_row as isize, strong_col as isize + 1)
                - sample(strong_row as isize, strong_col as isize - 1);
            let grad_row = sample(strong_row as isize + 1, strong_col as isize)
                - sample(strong_row as isize - 1, strong_col as isize);
            let grad_mag = (grad_col * grad_col + grad_row * grad_row).sqrt();
            let (flow_east, flow_north) = if grad_mag > 1e-6 {
                (
                    0.5 - 0.5 * grad_row / grad_mag,
                    0.5 + 0.5 * grad_col / grad_mag,
                )
            } else {
                (0.5, 0.5)
            };

            for index in corners {
                let strength = strengths[index];
                let smooth = {
                    let t = ((strength - threshold) / (1.0 - threshold)).clamp(0.0, 1.0);
                    t * t * (3.0 - 2.0 * t)
                };
                // Weak corners are pulled toward the cell's channel core in
                // proportion to how much weaker they are than the local
                // maximum, so a uniform channel keeps its full width and only a
                // gradient narrows into a ribbon with exposed banks.
                let pull = if max_strength > f32::EPSILON {
                    f64::from((max_strength - strength) / max_strength).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let direction =
                    (radials[index] * (1.0 - pull) + core_direction * pull).normalize_or_zero();
                let radius = radii[index] + (core_radius - radii[index]) * pull * 0.5;
                // Lower-discharge cells sit closer to the bed so they read as
                // shallow banks rather than a floating sheet.
                let offset = RIVER_SURFACE_OFFSET_M * (0.35 + 0.65 * f64::from(smooth));
                let surface = direction * (radius + offset) - *anchor_body_fixed;
                positions.push(surface.as_vec3().to_array());
                normals.push(direction.as_vec3().to_array());
                uvs.push(geometry.uvs[index]);
                colors.push([strength, flow_east, flow_north, 1.0]);
            }
        }
    }
    if positions.is_empty() {
        return None;
    }

    // Two triangles per emitted cell in the same winding as the terrain grid.
    let mut indices = Vec::with_capacity(positions.len() / 2 * 3);
    for cell in 0..positions.len() / 4 {
        let base = (cell * 4) as u32;
        indices.extend_from_slice(&[base, base + 1, base + 3, base, base + 3, base + 2]);
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    // The water shader reads the red channel as its shallow/deep ramp, so the
    // drainage strength drives river colour and opacity.
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    Some(mesh)
}
