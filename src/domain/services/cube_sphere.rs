//! Cube-sphere planetary terrain topology (AGENTS.md sections 20 and 22).
//!
//! A planet's surface is a cube projected to a sphere: six faces, each
//! subdivided by a quadtree. Patch LOD is selected from screen-space error,
//! and skirts hide cracks at LOD boundaries. Geometry is built from the shared
//! [`TerrainSource`], so render and collision stay on one height function.
//!
//! This module is pure domain logic (no Bevy ECS); the streaming/render layers
//! consume it.

use crate::domain::services::terrain_source::TerrainSource;
use bevy::math::DVec3;
use std::collections::{BTreeMap, BTreeSet};

/// The six faces of the cube-sphere.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CubeFace {
    PosX,
    NegX,
    PosY,
    NegY,
    PosZ,
    NegZ,
}

impl CubeFace {
    /// Cube faces in a stable order for deterministic traversal.
    pub const ALL: [Self; 6] = [
        Self::PosX,
        Self::NegX,
        Self::PosY,
        Self::NegY,
        Self::PosZ,
        Self::NegZ,
    ];
}

/// A directed patch edge in face UV coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PatchEdge {
    West,
    East,
    South,
    North,
}

impl PatchEdge {
    pub const ALL: [Self; 4] = [Self::West, Self::East, Self::South, Self::North];

    pub const fn opposite(self) -> Self {
        match self {
            Self::West => Self::East,
            Self::East => Self::West,
            Self::South => Self::North,
            Self::North => Self::South,
        }
    }
}

/// Map a unit direction to its dominant cube face and `(u, v)` in [0,1]² within
/// that face (cube-sphere projection).
pub fn face_uv(dir: DVec3) -> (CubeFace, f64, f64) {
    let d = dir.normalize();
    let ax = d.x.abs();
    let ay = d.y.abs();
    let az = d.z.abs();
    if ax >= ay && ax >= az {
        let face = if d.x > 0.0 {
            CubeFace::PosX
        } else {
            CubeFace::NegX
        };
        (face, ((d.y / ax) + 1.0) / 2.0, ((d.z / ax) + 1.0) / 2.0)
    } else if ay >= az {
        let face = if d.y > 0.0 {
            CubeFace::PosY
        } else {
            CubeFace::NegY
        };
        (face, ((d.x / ay) + 1.0) / 2.0, ((d.z / ay) + 1.0) / 2.0)
    } else {
        let face = if d.z > 0.0 {
            CubeFace::PosZ
        } else {
            CubeFace::NegZ
        };
        (face, ((d.x / az) + 1.0) / 2.0, ((d.y / az) + 1.0) / 2.0)
    }
}

/// Inverse of [`face_uv`]: map a face and `(u, v)` in [0,1]² back to a unit
/// direction on the sphere.
pub fn face_uv_to_direction(face: CubeFace, u: f64, v: f64) -> DVec3 {
    let a = 2.0 * u - 1.0;
    let b = 2.0 * v - 1.0;
    let p = match face {
        CubeFace::PosX => DVec3::new(1.0, a, b),
        CubeFace::NegX => DVec3::new(-1.0, a, b),
        CubeFace::PosY => DVec3::new(a, 1.0, b),
        CubeFace::NegY => DVec3::new(a, -1.0, b),
        CubeFace::PosZ => DVec3::new(a, b, 1.0),
        CubeFace::NegZ => DVec3::new(a, b, -1.0),
    };
    p.normalize()
}

/// A quadtree patch on a cube face: face + level + tile coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TerrainPatch {
    pub face: CubeFace,
    pub level: u32,
    pub tile_x: u32,
    pub tile_y: u32,
}

impl TerrainPatch {
    /// The level-0 root patch (whole face).
    pub const fn root(face: CubeFace) -> Self {
        Self {
            face,
            level: 0,
            tile_x: 0,
            tile_y: 0,
        }
    }

    /// One root for every cube face, in stable face order.
    pub const fn roots() -> [Self; 6] {
        [
            Self::root(CubeFace::PosX),
            Self::root(CubeFace::NegX),
            Self::root(CubeFace::PosY),
            Self::root(CubeFace::NegY),
            Self::root(CubeFace::PosZ),
            Self::root(CubeFace::NegZ),
        ]
    }

    /// The parent patch, or `None` for a face root.
    pub const fn parent(&self) -> Option<Self> {
        if self.level == 0 {
            None
        } else {
            Some(Self {
                face: self.face,
                level: self.level - 1,
                tile_x: self.tile_x / 2,
                tile_y: self.tile_y / 2,
            })
        }
    }

    /// The four children at the next level.
    pub fn subdivide(&self) -> [TerrainPatch; 4] {
        let cx = self.tile_x * 2;
        let cy = self.tile_y * 2;
        [
            TerrainPatch {
                face: self.face,
                level: self.level + 1,
                tile_x: cx,
                tile_y: cy,
            },
            TerrainPatch {
                face: self.face,
                level: self.level + 1,
                tile_x: cx + 1,
                tile_y: cy,
            },
            TerrainPatch {
                face: self.face,
                level: self.level + 1,
                tile_x: cx,
                tile_y: cy + 1,
            },
            TerrainPatch {
                face: self.face,
                level: self.level + 1,
                tile_x: cx + 1,
                tile_y: cy + 1,
            },
        ]
    }

    /// The four children at the next level, ordered southwest, southeast,
    /// northwest, northeast in face UV coordinates.
    pub fn children(&self) -> [TerrainPatch; 4] {
        self.subdivide()
    }

    /// Whether this patch is an ancestor of `other`, including itself.
    pub fn is_ancestor_of(&self, other: &Self) -> bool {
        if self.face != other.face || self.level > other.level {
            return false;
        }
        let shift = other.level - self.level;
        other.tile_x >> shift == self.tile_x && other.tile_y >> shift == self.tile_y
    }

    /// The same-face neighbor across `edge`, if the edge is not a cube-face boundary.
    pub fn same_face_neighbor(&self, edge: PatchEdge) -> Option<Self> {
        let span = 1u64 << self.level;
        let tile_x = self.tile_x as u64;
        let tile_y = self.tile_y as u64;
        let (tile_x, tile_y) = match edge {
            PatchEdge::West if tile_x > 0 => (tile_x - 1, tile_y),
            PatchEdge::East if tile_x + 1 < span => (tile_x + 1, tile_y),
            PatchEdge::South if tile_y > 0 => (tile_x, tile_y - 1),
            PatchEdge::North if tile_y + 1 < span => (tile_x, tile_y + 1),
            _ => return None,
        };
        Some(Self {
            face: self.face,
            level: self.level,
            tile_x: tile_x as u32,
            tile_y: tile_y as u32,
        })
    }

    /// The same-level neighboring patch across a cube-face boundary.
    ///
    /// The mapping is derived from the authoritative face-to-direction mapping
    /// rather than duplicating a hand-maintained face transition table.
    pub fn cross_face_neighbor(&self, edge: PatchEdge) -> Option<PatchNeighbor> {
        if self.same_face_neighbor(edge).is_some() {
            return None;
        }

        let patch = self.cross_face_neighbor_patch(edge);
        let neighbor_edge = PatchEdge::ALL
            .into_iter()
            .find(|candidate_edge| patch.cross_face_neighbor_patch(*candidate_edge).eq(self))?;
        Some(PatchNeighbor {
            patch,
            edge: neighbor_edge,
        })
    }

    /// The patch across `edge`, with the corresponding edge on the neighbor.
    pub fn neighbor(&self, edge: PatchEdge) -> PatchNeighbor {
        if let Some(patch) = self.same_face_neighbor(edge) {
            PatchNeighbor {
                patch,
                edge: edge.opposite(),
            }
        } else {
            self.cross_face_neighbor(edge)
                .expect("every cube-face boundary has a neighbor")
        }
    }

    fn cross_face_neighbor_patch(&self, edge: PatchEdge) -> Self {
        let span = (1u64 << self.level) as f64;
        let (u0, v0, u1, v1) = self.uv_bounds();
        let inset = 0.25 / span;
        let (u, v) = match edge {
            PatchEdge::West => (u0 - inset, (v0 + v1) * 0.5),
            PatchEdge::East => (u1 + inset, (v0 + v1) * 0.5),
            PatchEdge::South => ((u0 + u1) * 0.5, v0 - inset),
            PatchEdge::North => ((u0 + u1) * 0.5, v1 + inset),
        };
        let (face, neighbor_u, neighbor_v) = face_uv(face_uv_to_direction(self.face, u, v));
        let tile_x = ((neighbor_u * span) as u64).min(span as u64 - 1) as u32;
        let tile_y = ((neighbor_v * span) as u64).min(span as u64 - 1) as u32;
        Self {
            face,
            level: self.level,
            tile_x,
            tile_y,
        }
    }

    /// The `(u0, v0, u1, v1)` bounds of this patch in face uv space.
    pub fn uv_bounds(&self) -> (f64, f64, f64, f64) {
        let span = (1u64 << self.level) as f64;
        let u0 = self.tile_x as f64 / span;
        let v0 = self.tile_y as f64 / span;
        let u1 = (self.tile_x as f64 + 1.0) / span;
        let v1 = (self.tile_y as f64 + 1.0) / span;
        (u0, v0, u1, v1)
    }

    /// The direction through the center of this patch.
    pub fn center_direction(&self) -> DVec3 {
        let (u0, v0, u1, v1) = self.uv_bounds();
        face_uv_to_direction(self.face, (u0 + u1) * 0.5, (v0 + v1) * 0.5)
    }

    /// The patch at `level` covering a given unit direction.
    pub fn for_direction(dir: DVec3, level: u32) -> Self {
        let (face, u, v) = face_uv(dir);
        let span = (1u64 << level) as f64;
        let tx = ((u * span) as u32).min(span as u32 - 1);
        let ty = ((v * span) as u32).min(span as u32 - 1);
        Self {
            face,
            level,
            tile_x: tx,
            tile_y: ty,
        }
    }
}

/// A neighboring patch and the edge on that patch shared with the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PatchNeighbor {
    pub patch: TerrainPatch,
    pub edge: PatchEdge,
}

/// Approximate world-space edge length of a patch at a level on a planet.
pub fn patch_world_size_m(level: u32, planet_radius_m: f64) -> f64 {
    let face_arc = planet_radius_m * std::f64::consts::FRAC_PI_2;
    face_arc / (1u64 << level) as f64
}

/// Projected on-screen error in pixels for a geometric error at a distance.
pub fn screen_space_error_m(
    geometric_error_m: f64,
    distance_m: f64,
    fov_rad: f64,
    screen_height_px: f64,
) -> f64 {
    if distance_m <= 1e-6 {
        return f64::INFINITY;
    }
    let scale = screen_height_px / (2.0 * distance_m * (fov_rad * 0.5).tan());
    geometric_error_m * scale
}

/// The maximum LOD level such that a patch's projected size stays under
/// `screen_error_px`. Higher levels near the camera, lower far away.
pub fn lod_for_distance(
    distance_m: f64,
    planet_radius_m: f64,
    fov_rad: f64,
    screen_height_px: f64,
    screen_error_px: f64,
    max_level: u32,
) -> u32 {
    let mut level = 0u32;
    while level < max_level {
        let size = patch_world_size_m(level, planet_radius_m);
        let err = screen_space_error_m(size, distance_m, fov_rad, screen_height_px);
        if err <= screen_error_px {
            break;
        }
        level += 1;
    }
    level
}

/// Conservative terrain approximation inputs for one patch.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PatchGeometricError {
    /// The known source elevation range within the patch, in meters.
    pub elevation_range_m: f64,
    /// The maximum observed or bounded child-to-parent height deviation, in meters.
    pub child_to_parent_deviation_m: f64,
}

impl PatchGeometricError {
    /// A conservative world-space approximation error including curvature.
    pub fn conservative_m(self, patch: &TerrainPatch, planet_radius_m: f64) -> f64 {
        let half_face_angle = std::f64::consts::FRAC_PI_4 / (1u64 << patch.level) as f64;
        let curvature_m = planet_radius_m.abs() * (1.0 - half_face_angle.cos());
        curvature_m + self.elevation_range_m.abs() + self.child_to_parent_deviation_m.abs()
    }
}

/// Camera data needed to project a patch's geometric error into pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraProjection {
    pub position_m: DVec3,
    pub vertical_fov_rad: f64,
    pub viewport_height_px: f64,
}

/// Project a patch's conservative error using the distance to its center.
pub fn projected_patch_error_px(
    patch: &TerrainPatch,
    geometric_error_m: PatchGeometricError,
    planet_radius_m: f64,
    camera: CameraProjection,
) -> f64 {
    let patch_center_m = patch.center_direction() * planet_radius_m;
    let distance_m = camera.position_m.distance(patch_center_m);
    screen_space_error_m(
        geometric_error_m.conservative_m(patch, planet_radius_m),
        distance_m,
        camera.vertical_fov_rad,
        camera.viewport_height_px,
    )
}

/// Deterministic readiness and visibility inputs for pure quadtree selection.
///
/// Visibility applies to a patch and its ancestors or descendants, allowing a
/// caller to provide either coarse visibility regions or exact leaf coverage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuadtreePatchState {
    pub ready: BTreeSet<TerrainPatch>,
    pub visible: BTreeSet<TerrainPatch>,
}

impl Default for QuadtreePatchState {
    fn default() -> Self {
        Self {
            ready: BTreeSet::new(),
            visible: TerrainPatch::roots().into_iter().collect(),
        }
    }
}

impl QuadtreePatchState {
    pub fn is_ready(&self, patch: &TerrainPatch) -> bool {
        self.ready.contains(patch)
    }

    pub fn is_visible(&self, patch: &TerrainPatch) -> bool {
        self.visible
            .iter()
            .any(|visible| visible.is_ancestor_of(patch) || patch.is_ancestor_of(visible))
    }
}

/// Parameters for deterministic six-face quadtree selection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QuadtreeSelectionConfig {
    pub max_level: u32,
    pub max_projected_error_px: f64,
    pub max_neighbor_level_difference: u32,
}

/// Desired and renderable leaf covers selected without runtime dependencies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuadtreeSelection {
    /// Finest leaves requested by projected-error traversal, before readiness fallback.
    pub target_leaves: BTreeSet<TerrainPatch>,
    /// Requested non-root patches, including required ancestors.
    pub requested: BTreeSet<TerrainPatch>,
    /// Leaves that can be published now; unready children retain their parent.
    pub visible_leaves: BTreeSet<TerrainPatch>,
}

/// Select a complete, balanced six-face leaf cover from supplied projected errors.
///
/// Readiness only affects `visible_leaves`; `target_leaves` and `requested` are
/// identical for the same visibility, errors, and configuration.
pub fn select_quadtree_leaves(
    state: &QuadtreePatchState,
    projected_errors_px: &BTreeMap<TerrainPatch, f64>,
    config: QuadtreeSelectionConfig,
) -> QuadtreeSelection {
    let mut target_leaves: BTreeSet<_> = TerrainPatch::roots().into_iter().collect();
    let mut pending: Vec<_> = TerrainPatch::roots().into_iter().collect();

    while let Some(patch) = pending.pop() {
        let projected_error_px = projected_errors_px.get(&patch).copied().unwrap_or(0.0);
        if patch.level >= config.max_level
            || !state.is_visible(&patch)
            || projected_error_px <= config.max_projected_error_px
        {
            continue;
        }

        target_leaves.remove(&patch);
        for child in patch.children() {
            target_leaves.insert(child);
            pending.push(child);
        }
    }

    target_leaves = balance_visible_leaves(&target_leaves, config.max_neighbor_level_difference);

    let mut requested = BTreeSet::new();
    for leaf in &target_leaves {
        let mut current = Some(*leaf);
        while let Some(patch) = current {
            if patch.level > 0 {
                requested.insert(patch);
            }
            current = patch.parent();
        }
    }

    let mut visible_leaves = BTreeSet::new();
    for root in TerrainPatch::roots() {
        resolve_ready_leaves(root, &target_leaves, state, &mut visible_leaves);
    }

    QuadtreeSelection {
        target_leaves,
        requested,
        visible_leaves,
    }
}

fn resolve_ready_leaves(
    patch: TerrainPatch,
    target_leaves: &BTreeSet<TerrainPatch>,
    state: &QuadtreePatchState,
    visible_leaves: &mut BTreeSet<TerrainPatch>,
) {
    if target_leaves.contains(&patch) {
        if state.is_ready(&patch) {
            visible_leaves.insert(patch);
        }
        return;
    }

    let children = patch.children();
    if children.iter().all(|child| state.is_ready(child)) {
        for child in children {
            resolve_ready_leaves(child, target_leaves, state, visible_leaves);
        }
    } else if state.is_ready(&patch) {
        // A descendant cannot replace this parent until its complete sibling
        // set is ready. Do not publish an unready fallback: render observers
        // receive lifecycle events only once and cannot build absent geometry.
        visible_leaves.insert(patch);
    }
}

/// Whether two patches share an edge segment, including cube-face seams.
pub fn patches_are_adjacent(a: &TerrainPatch, b: &TerrainPatch) -> bool {
    if a == b {
        return false;
    }
    if a.face == b.face {
        return PatchEdge::ALL
            .into_iter()
            .any(|edge| shares_same_face_edge(a, b, edge));
    }

    patches_share_cross_face_edge(a, b) || patches_share_cross_face_edge(b, a)
}

fn patches_share_cross_face_edge(a: &TerrainPatch, b: &TerrainPatch) -> bool {
    PatchEdge::ALL.into_iter().any(|edge| {
        a.cross_face_neighbor(edge).is_some_and(|neighbor| {
            neighbor.patch.face == b.face
                && (neighbor.patch == *b
                    || patch_touches_ancestor_edge(&neighbor.patch, b, neighbor.edge)
                    || shares_same_face_edge(&neighbor.patch, b, neighbor.edge))
        })
    })
}

/// Refine coarse leaves until all adjacent leaves meet the configured level limit.
pub fn balance_visible_leaves(
    leaves: &BTreeSet<TerrainPatch>,
    max_level_difference: u32,
) -> BTreeSet<TerrainPatch> {
    let mut balanced = leaves.clone();
    loop {
        let patches: Vec<_> = balanced.iter().copied().collect();
        let mut coarser = None;
        'pairs: for (index, a) in patches.iter().enumerate() {
            for b in patches.iter().skip(index + 1) {
                if patches_are_adjacent(a, b) && a.level.abs_diff(b.level) > max_level_difference {
                    coarser = Some(if a.level < b.level { *a } else { *b });
                    break 'pairs;
                }
            }
        }

        let Some(coarser) = coarser else {
            return balanced;
        };
        balanced.remove(&coarser);
        balanced.extend(coarser.children());
    }
}

fn shares_same_face_edge(a: &TerrainPatch, b: &TerrainPatch, edge: PatchEdge) -> bool {
    let level = a.level.max(b.level);
    let scale_a = 1u64 << (level - a.level);
    let scale_b = 1u64 << (level - b.level);
    let ax0 = a.tile_x as u64 * scale_a;
    let ax1 = (a.tile_x as u64 + 1) * scale_a;
    let ay0 = a.tile_y as u64 * scale_a;
    let ay1 = (a.tile_y as u64 + 1) * scale_a;
    let bx0 = b.tile_x as u64 * scale_b;
    let bx1 = (b.tile_x as u64 + 1) * scale_b;
    let by0 = b.tile_y as u64 * scale_b;
    let by1 = (b.tile_y as u64 + 1) * scale_b;

    match edge {
        PatchEdge::West => ax0 == bx1 && ranges_overlap(ay0, ay1, by0, by1),
        PatchEdge::East => ax1 == bx0 && ranges_overlap(ay0, ay1, by0, by1),
        PatchEdge::South => ay0 == by1 && ranges_overlap(ax0, ax1, bx0, bx1),
        PatchEdge::North => ay1 == by0 && ranges_overlap(ax0, ax1, bx0, bx1),
    }
}

fn patch_touches_ancestor_edge(
    patch: &TerrainPatch,
    ancestor: &TerrainPatch,
    edge: PatchEdge,
) -> bool {
    if patch == ancestor || !ancestor.is_ancestor_of(patch) {
        return false;
    }
    let scale = 1u64 << (patch.level - ancestor.level);
    let min_x = ancestor.tile_x as u64 * scale;
    let max_x = (ancestor.tile_x as u64 + 1) * scale;
    let min_y = ancestor.tile_y as u64 * scale;
    let max_y = (ancestor.tile_y as u64 + 1) * scale;
    match edge {
        PatchEdge::West => patch.tile_x as u64 == min_x,
        PatchEdge::East => patch.tile_x as u64 + 1 == max_x,
        PatchEdge::South => patch.tile_y as u64 == min_y,
        PatchEdge::North => patch.tile_y as u64 + 1 == max_y,
    }
}

fn ranges_overlap(a0: u64, a1: u64, b0: u64, b1: u64) -> bool {
    a0 < b1 && b0 < a1
}

/// Geometry of a terrain patch: sphere-aligned positions, normals, and indices
/// with a downward skirt ring to hide LOD cracks.
#[derive(Debug, Clone, PartialEq)]
pub struct PatchGeometry {
    pub positions: Vec<[f64; 3]>,
    pub normals: Vec<[f64; 3]>,
    /// Stable equirectangular UVs for whole-planet imagery.
    pub uvs: Vec<[f32; 2]>,
    /// Tile-local UVs retained for future custom material normal/detail maps.
    pub local_uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}

/// Build the mesh geometry for a patch from the shared terrain source.
/// `resolution` is the number of vertices per side. Boundary vertices get a
/// skirt ring extruded down the normal by `skirt_depth_m`.
pub fn build_patch_geometry(
    patch: &TerrainPatch,
    source: &dyn TerrainSource,
    planet_radius_m: f64,
    resolution: u32,
    skirt_depth_m: f64,
) -> PatchGeometry {
    build_patch_geometry_with_stitches(
        patch,
        source,
        planet_radius_m,
        resolution,
        skirt_depth_m,
        &[],
    )
}

/// Build a patch with 2:1 edge stitch index variants for every listed edge.
/// A stitched fine edge references only every second boundary sample, which
/// aligns with the corresponding `2^n+1` coarse grid. Skirts remain below the
/// surface as a defensive fallback for raster precision and multi-edge corners.
pub fn build_patch_geometry_with_stitches(
    patch: &TerrainPatch,
    source: &dyn TerrainSource,
    planet_radius_m: f64,
    resolution: u32,
    skirt_depth_m: f64,
    stitched_edges: &[PatchEdge],
) -> PatchGeometry {
    build_patch_geometry_with_height_sampler(
        patch,
        planet_radius_m,
        resolution,
        skirt_depth_m,
        stitched_edges,
        |latitude_deg, longitude_deg| {
            source.mesh_height_m(latitude_deg, longitude_deg, patch.level)
        },
    )
}

fn build_patch_geometry_with_height_sampler(
    patch: &TerrainPatch,
    planet_radius_m: f64,
    resolution: u32,
    skirt_depth_m: f64,
    stitched_edges: &[PatchEdge],
    height_at: impl Fn(f64, f64) -> f64,
) -> PatchGeometry {
    // Sample in planet-tangent coordinates, rather than patch UV space, so
    // shared vertices retain an identical normal across LOD and cube faces.
    // A global footprint keeps shared normals identical across cube faces and
    // LOD transitions while filtering sub-cell micro relief out of macro mesh
    // lighting. Fine patches recover small-scale grain through their normal map.
    const NORMAL_SAMPLE_DISTANCE_M: f64 = 250.0;

    let res = resolution.max(2) as usize;
    let (u0, v0, u1, v1) = patch.uv_bounds();

    let mut grid = vec![[0.0f64; 3]; res * res];
    let mut normals = vec![[0.0f64; 3]; res * res];
    let mut uvs = vec![[0.0f32; 2]; res * res];
    let mut local_uvs = vec![[0.0f32; 2]; res * res];
    let surface_point = |direction: DVec3| {
        let direction = direction.normalize();
        let (latitude_deg, longitude_deg) = direction_to_lat_lon(direction);
        direction * (planet_radius_m + height_at(latitude_deg, longitude_deg))
    };
    let normal_sample_angle = NORMAL_SAMPLE_DISTANCE_M / planet_radius_m;
    for j in 0..res {
        for i in 0..res {
            let u = u0 + (u1 - u0) * i as f64 / (res - 1) as f64;
            let v = v0 + (v1 - v0) * j as f64 / (res - 1) as f64;
            let dir = face_uv_to_direction(patch.face, u, v);
            let (lat, lon) = direction_to_lat_lon(dir);
            let h = height_at(lat, lon);
            let idx = j * res + i;
            let p = dir * (planet_radius_m + h);
            grid[idx] = p.to_array();
            // Global imagery uses geographic coordinates, not a tile-local
            // projection, so every level shares one continuous Earth albedo.
            uvs[idx] = [
                ((lon + 180.0) / 360.0) as f32,
                ((90.0 - lat) / 180.0) as f32,
            ];
            local_uvs[idx] = [i as f32 / (res - 1) as f32, j as f32 / (res - 1) as f32];

            // Sample normals immediately after the vertex position. Eroded
            // terrain tiles are then reused while still resident instead of
            // being regenerated in a later full-mesh normal pass.
            let radial = p.normalize();
            let reference_axis = if radial.y.abs() < 0.9 {
                DVec3::Y
            } else {
                DVec3::X
            };
            let east = reference_axis.cross(radial).normalize();
            let north = radial.cross(east).normalize();
            let east_plus = surface_point((radial + east * normal_sample_angle).normalize());
            let east_minus = surface_point((radial - east * normal_sample_angle).normalize());
            let north_plus = surface_point((radial + north * normal_sample_angle).normalize());
            let north_minus = surface_point((radial - north * normal_sample_angle).normalize());
            let n = (east_plus - east_minus)
                .cross(north_plus - north_minus)
                .normalize_or_zero();
            normals[idx] = if n.dot(p) < 0.0 { -n } else { n }.to_array();
        }
    }

    // Skirt ring: for every boundary vertex append a copy extruded down the
    // normal. The ring hides cracks between patches of different LOD.
    let mut positions = grid.clone();
    let mut all_normals = normals.clone();
    let mut skirt_index = vec![None; res * res];
    let on_boundary = |i: usize, j: usize| i == 0 || i == res - 1 || j == 0 || j == res - 1;
    for j in 0..res {
        for i in 0..res {
            if on_boundary(i, j) {
                let idx = j * res + i;
                let p = DVec3::from_array(grid[idx]);
                let n = DVec3::from_array(normals[idx]);
                positions.push((p - n * skirt_depth_m).to_array());
                all_normals.push(normals[idx]);
                uvs.push(uvs[idx]);
                local_uvs.push(local_uvs[idx]);
                skirt_index[idx] = Some(positions.len() as u32 - 1);
            }
        }
    }

    // Grid triangles (the original res×res vertices are indices 0..res*res).
    // The UV axes are right-handed about the outward normal on some cube faces
    // (NegX, PosY, NegZ) and left-handed on the others (PosX, NegY, PosZ).
    // Emit indices so the front face always points outward (CCW viewed from outside).
    let reversed = matches!(patch.face, CubeFace::PosX | CubeFace::NegY | CubeFace::PosZ);

    let mut indices = Vec::with_capacity((res - 1) * (res - 1) * 6 + res * 4 * 6);
    for j in 0..res - 1 {
        for i in 0..res - 1 {
            let tl = stitched_grid_index(i, j, res, stitched_edges);
            let tr = stitched_grid_index(i + 1, j, res, stitched_edges);
            let bl = stitched_grid_index(i, j + 1, res, stitched_edges);
            let br = stitched_grid_index(i + 1, j + 1, res, stitched_edges);
            if reversed {
                // Flip the winding so front faces outward on left-handed faces.
                indices.extend_from_slice(&[tl, tr, bl, tr, br, bl]);
            } else {
                indices.extend_from_slice(&[tl, bl, tr, tr, bl, br]);
            }
        }
    }

    // Skirt quads: one quad per boundary segment (grid vertex → skirt vertex).
    // Opposing edges need opposite winding. Select it from the geometric
    // outward direction instead of applying one face-wide winding rule, which
    // back-face culled two of the four skirt walls.
    let patch_center = face_uv_to_direction(patch.face, (u0 + u1) * 0.5, (v0 + v1) * 0.5);
    let push_skirt_quad = |indices: &mut Vec<u32>, a: u32, b: u32, c: u32, d: u32| {
        let a_position = DVec3::from_array(positions[a as usize]);
        let b_position = DVec3::from_array(positions[b as usize]);
        let c_position = DVec3::from_array(positions[c as usize]);
        let edge_midpoint = (a_position + b_position).normalize();
        let outward =
            (edge_midpoint - patch_center * edge_midpoint.dot(patch_center)).normalize_or_zero();
        let triangle_normal = (b_position - a_position)
            .cross(c_position - a_position)
            .normalize_or_zero();
        if triangle_normal.dot(outward) >= 0.0 {
            indices.extend_from_slice(&[a, b, c, c, b, d]);
        } else {
            indices.extend_from_slice(&[a, c, b, c, d, b]);
        }
    };
    let skirt = |i: usize| skirt_index[i].expect("boundary vertex must have a skirt");
    // Bottom edge (j = 0), left→right.
    for i in 0..res - 1 {
        let a = i;
        let b = i + 1;
        push_skirt_quad(&mut indices, a as u32, b as u32, skirt(b), skirt(a));
    }
    // Top edge (j = res-1), left→right.
    let top = (res - 1) * res;
    for i in 0..res - 1 {
        let a = top + i;
        let b = top + i + 1;
        push_skirt_quad(&mut indices, a as u32, b as u32, skirt(b), skirt(a));
    }
    // Left edge.
    for j in 0..res - 1 {
        let a = j * res;
        let b = (j + 1) * res;
        push_skirt_quad(&mut indices, a as u32, b as u32, skirt(b), skirt(a));
    }
    // Right edge.
    for j in 0..res - 1 {
        let a = j * res + res - 1;
        let b = (j + 1) * res + res - 1;
        push_skirt_quad(&mut indices, a as u32, b as u32, skirt(b), skirt(a));
    }

    PatchGeometry {
        positions,
        normals: all_normals,
        uvs,
        local_uvs,
        indices,
    }
}

fn stitched_grid_index(i: usize, j: usize, resolution: usize, stitched_edges: &[PatchEdge]) -> u32 {
    let mut i = i;
    let mut j = j;
    if stitched_edges.contains(&PatchEdge::West) && i == 0 && j % 2 == 1 {
        j -= 1;
    }
    if stitched_edges.contains(&PatchEdge::East) && i + 1 == resolution && j % 2 == 1 {
        j -= 1;
    }
    if stitched_edges.contains(&PatchEdge::South) && j == 0 && i % 2 == 1 {
        i -= 1;
    }
    if stitched_edges.contains(&PatchEdge::North) && j + 1 == resolution && i % 2 == 1 {
        i -= 1;
    }
    (j * resolution + i) as u32
}

/// Direction to latitude/longitude in degrees.
pub fn direction_to_lat_lon(dir: DVec3) -> (f64, f64) {
    crate::domain::services::reference_frames::body_fixed_to_terrain_lat_lon(dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::terrain_source::{central_angle_deg, ProceduralTerrainSource};
    use std::sync::Mutex;

    fn source() -> ProceduralTerrainSource {
        ProceduralTerrainSource::new(99, 2_000.0, 800.0, 0)
    }

    #[derive(Debug, Default)]
    struct SampleTraceSource {
        samples: Mutex<Vec<(f64, f64)>>,
    }

    impl TerrainSource for SampleTraceSource {
        fn height_m(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
            self.samples
                .lock()
                .expect("sample trace lock")
                .push((latitude_deg, longitude_deg));
            0.0
        }
    }

    #[test]
    fn geometry_samples_each_vertex_and_normal_probe_together() {
        let source = SampleTraceSource::default();
        let patch = TerrainPatch::root(CubeFace::PosZ);
        build_patch_geometry(&patch, &source, 6_371_000.0, 3, 40.0);

        let samples = source.samples.lock().expect("sample trace lock");
        assert_eq!(samples.len(), 3 * 3 * 5);
        let (latitude_deg, longitude_deg) = samples[0];
        assert!(samples[1..5].iter().all(|(lat, lon)| {
            central_angle_deg(latitude_deg, longitude_deg, *lat, *lon) < 0.01
        }));
        assert!(
            central_angle_deg(latitude_deg, longitude_deg, samples[5].0, samples[5].1) > 1.0,
            "the next vertex must follow the first vertex's four normal probes"
        );
    }

    #[test]
    fn face_uv_round_trips() {
        for dir in [
            DVec3::new(1.0, 0.2, 0.1),
            DVec3::new(-0.5, -0.8, 0.3),
            DVec3::new(0.1, -0.2, 1.0),
            DVec3::new(0.0, 1.0, 0.0),
            DVec3::new(-1.0, 0.0, 0.0),
        ] {
            let (face, u, v) = face_uv(dir);
            let back = face_uv_to_direction(face, u, v);
            assert!(
                (back - dir.normalize()).length() < 1e-9,
                "round trip failed for {dir}"
            );
        }
    }

    #[test]
    fn quadtree_subdivision_covers_children() {
        let root = TerrainPatch::root(CubeFace::PosZ);
        let children = root.subdivide();
        assert_eq!(children.len(), 4);
        assert!(children.iter().all(|c| c.level == 1));
        // Tiles tile the parent [0,1)² into four quadrants.
        let mut tiles: Vec<(u32, u32)> = children.iter().map(|c| (c.tile_x, c.tile_y)).collect();
        tiles.sort();
        assert_eq!(tiles, vec![(0, 0), (0, 1), (1, 0), (1, 1)]);
    }

    #[test]
    fn parent_children_and_roots_form_deterministic_partition() {
        let roots = TerrainPatch::roots();
        assert_eq!(roots.len(), 6);
        assert_eq!(
            roots.iter().map(|root| root.face).collect::<Vec<_>>(),
            CubeFace::ALL
        );
        assert!(roots.iter().all(|root| root.parent().is_none()));

        let parent = TerrainPatch {
            face: CubeFace::NegY,
            level: 3,
            tile_x: 5,
            tile_y: 2,
        };
        let children = parent.children();
        assert!(children.iter().all(|child| child.parent() == Some(parent)));
        assert!(children.iter().all(|child| parent.is_ancestor_of(child)));

        let parent_area = {
            let (u0, v0, u1, v1) = parent.uv_bounds();
            (u1 - u0) * (v1 - v0)
        };
        let children_area: f64 = children
            .iter()
            .map(|child| {
                let (u0, v0, u1, v1) = child.uv_bounds();
                (u1 - u0) * (v1 - v0)
            })
            .sum();
        assert!((children_area - parent_area).abs() < 1e-15);
    }

    #[test]
    fn neighbors_are_symmetric_across_every_cube_face_edge() {
        for face in CubeFace::ALL {
            let root = TerrainPatch::root(face);
            for edge in PatchEdge::ALL {
                let neighbor = root.cross_face_neighbor(edge).unwrap();
                assert_ne!(neighbor.patch.face, face);
                assert!(patches_are_adjacent(&root, &neighbor.patch));
                assert_eq!(neighbor.patch.neighbor(neighbor.edge).patch, root);
                assert_eq!(neighbor.patch.neighbor(neighbor.edge).edge, edge);
            }
        }

        let level = 3;
        let last = (1 << level) - 1;
        for face in CubeFace::ALL {
            for edge in PatchEdge::ALL {
                let patch = match edge {
                    PatchEdge::West => TerrainPatch {
                        face,
                        level,
                        tile_x: 0,
                        tile_y: 2,
                    },
                    PatchEdge::East => TerrainPatch {
                        face,
                        level,
                        tile_x: last,
                        tile_y: 2,
                    },
                    PatchEdge::South => TerrainPatch {
                        face,
                        level,
                        tile_x: 2,
                        tile_y: 0,
                    },
                    PatchEdge::North => TerrainPatch {
                        face,
                        level,
                        tile_x: 2,
                        tile_y: last,
                    },
                };
                let neighbor = patch.cross_face_neighbor(edge).unwrap();
                assert!(patches_are_adjacent(&patch, &neighbor.patch));
                assert_eq!(neighbor.patch.neighbor(neighbor.edge).patch, patch);
            }
        }

        let interior = TerrainPatch {
            face: CubeFace::PosZ,
            level: 3,
            tile_x: 3,
            tile_y: 4,
        };
        let west = interior.same_face_neighbor(PatchEdge::West).unwrap();
        assert_eq!(west.face, interior.face);
        assert_eq!(west.tile_x, interior.tile_x - 1);
        assert!(patches_are_adjacent(&interior, &west));
    }

    #[test]
    fn geometric_error_and_projection_order_by_detail_and_distance() {
        let error = PatchGeometricError {
            elevation_range_m: 100.0,
            child_to_parent_deviation_m: 20.0,
        };
        let root = TerrainPatch::root(CubeFace::PosX);
        let child = root.children()[0];
        assert!(
            error.conservative_m(&root, 6_371_000.0) > error.conservative_m(&child, 6_371_000.0)
        );

        let near = CameraProjection {
            position_m: root.center_direction() * 7_000_000.0,
            vertical_fov_rad: 1.0,
            viewport_height_px: 1_080.0,
        };
        let far = CameraProjection {
            position_m: root.center_direction() * 20_000_000.0,
            ..near
        };
        assert!(
            projected_patch_error_px(&root, error, 6_371_000.0, near)
                > projected_patch_error_px(&root, error, 6_371_000.0, far)
        );
    }

    #[test]
    fn balancing_refines_coarse_cross_face_neighbors() {
        let pos_z_root = TerrainPatch::root(CubeFace::PosZ);
        let mut leaves: BTreeSet<_> = TerrainPatch::roots().into_iter().collect();
        leaves.remove(&pos_z_root);
        leaves.extend(pos_z_root.children());

        let east_child = pos_z_root.children()[1];
        leaves.remove(&east_child);
        leaves.extend(east_child.children());

        let balanced = balance_visible_leaves(&leaves, 1);
        for a in &balanced {
            for b in &balanced {
                if patches_are_adjacent(a, b) {
                    assert!(a.level.abs_diff(b.level) <= 1, "{a:?} and {b:?}");
                }
            }
        }
    }

    #[test]
    fn readiness_changes_only_the_published_leaf_cover() {
        let root = TerrainPatch::root(CubeFace::PosZ);
        let errors = BTreeMap::from([(root, 10.0)]);
        let config = QuadtreeSelectionConfig {
            max_level: 1,
            max_projected_error_px: 1.0,
            max_neighbor_level_difference: 1,
        };

        let mut unready = QuadtreePatchState::default();
        unready.ready.extend(TerrainPatch::roots());
        let fallback = select_quadtree_leaves(&unready, &errors, config);
        assert_eq!(fallback.target_leaves.len(), 9);
        assert!(fallback.target_leaves.contains(&root.children()[0]));
        assert!(fallback.visible_leaves.contains(&root));

        let mut ready = unready.clone();
        ready.ready.extend(root.children());
        let published = select_quadtree_leaves(&ready, &errors, config);
        assert_eq!(fallback.target_leaves, published.target_leaves);
        assert_eq!(fallback.requested, published.requested);
        assert!(!published.visible_leaves.contains(&root));
        assert!(root
            .children()
            .iter()
            .all(|child| published.visible_leaves.contains(child)));
    }

    #[test]
    fn selection_waits_for_authoritative_root_geometry() {
        let selection = select_quadtree_leaves(
            &QuadtreePatchState::default(),
            &BTreeMap::new(),
            QuadtreeSelectionConfig {
                max_level: 4,
                max_projected_error_px: 1.0,
                max_neighbor_level_difference: 1,
            },
        );
        let roots: BTreeSet<_> = TerrainPatch::roots().into_iter().collect();
        assert_eq!(selection.target_leaves, roots);
        assert!(selection.visible_leaves.is_empty());

        let ready = QuadtreePatchState {
            ready: roots.clone(),
            ..QuadtreePatchState::default()
        };
        let selection = select_quadtree_leaves(
            &ready,
            &BTreeMap::new(),
            QuadtreeSelectionConfig {
                max_level: 4,
                max_projected_error_px: 1.0,
                max_neighbor_level_difference: 1,
            },
        );
        assert_eq!(selection.visible_leaves, roots);
    }

    #[test]
    fn patch_for_direction_lands_in_uv_bounds() {
        let dir = DVec3::new(0.4, 0.6, 1.0).normalize();
        let patch = TerrainPatch::for_direction(dir, 3);
        let (u0, v0, u1, v1) = patch.uv_bounds();
        let (_, u, v) = face_uv(dir);
        assert!(
            u >= u0 - 1e-9 && u <= u1 + 1e-9,
            "u {u} outside [{u0},{u1}]"
        );
        assert!(
            v >= v0 - 1e-9 && v <= v1 + 1e-9,
            "v {v} outside [{v0},{v1}]"
        );
    }

    #[test]
    fn lod_increases_as_camera_approaches() {
        let r = 6_371_000.0;
        let far = lod_for_distance(1_000_000.0, r, 1.0, 1080.0, 4.0, 12);
        let near = lod_for_distance(10_000.0, r, 1.0, 1080.0, 4.0, 12);
        assert!(near >= far, "near LOD {near} must be >= far LOD {far}");
        // Same distance is stable.
        let a = lod_for_distance(50_000.0, r, 1.0, 1080.0, 4.0, 12);
        let b = lod_for_distance(50_000.0, r, 1.0, 1080.0, 4.0, 12);
        assert_eq!(a, b);
    }

    #[test]
    fn mesh_conforms_to_sphere() {
        let patch = TerrainPatch::for_direction(DVec3::new(1.0, 0.0, 0.0), 1);
        let geom = build_patch_geometry(&patch, &source(), 6_371_000.0, 5, 50.0);
        assert!(geom.positions.len() >= 25);
        for p in &geom.positions {
            let r = DVec3::from_array(*p).length();
            // Radius stays near planet radius ± (rolling amplitude + mountain
            // amplitude). source() uses amplitude 2000 + mountain 800, so the
            // envelope is ±2800 m; +100 m margin for domain-warp peaks.
            assert!(
                (r - 6_371_000.0).abs() < 2_900.0,
                "vertex radius {r} off the sphere"
            );
        }
        assert!(!geom.indices.is_empty());
    }

    #[test]
    fn boundary_height_is_shared_across_lod() {
        let s = source();
        // A shared edge point on the same face at two LODs must agree because
        // both sample the same TerrainSource at the same direction.
        let dir = DVec3::new(0.3, 0.4, 1.0).normalize();
        let coarse = TerrainPatch::for_direction(dir, 2);
        let fine = TerrainPatch::for_direction(dir, 4);
        let h_coarse = sample_height(&coarse, &s, dir, 6_371_000.0);
        let h_fine = sample_height(&fine, &s, dir, 6_371_000.0);
        assert_eq!(h_coarse, h_fine);
    }

    #[test]
    fn parent_and_child_share_aligned_outer_edge_samples() {
        let parent = TerrainPatch::root(CubeFace::PosZ);
        let child = parent.children()[0];
        let source = source();
        let parent_geometry = build_patch_geometry(&parent, &source, 6_371_000.0, 33, 40.0);
        let child_geometry = build_patch_geometry(&child, &source, 6_371_000.0, 33, 40.0);

        // The child covers the lower half of the parent's west edge. With a
        // 2^n+1 grid, every other child sample equals a parent sample exactly.
        for child_y in (0..33usize).step_by(2) {
            let parent_y = child_y / 2;
            let parent_index = parent_y * 33;
            let child_index = child_y * 33;
            let parent_position = parent_geometry.positions[parent_index];
            let child_position = child_geometry.positions[child_index];
            assert_eq!(parent_position, child_position);
            assert_eq!(
                parent_geometry.normals[parent_index], child_geometry.normals[child_index],
                "shared parent/child edge vertex has mismatched lighting normal"
            );
        }
    }

    #[test]
    fn root_face_shared_vertices_have_matching_normals() {
        let source = source();
        let faces = [
            CubeFace::PosX,
            CubeFace::NegX,
            CubeFace::PosY,
            CubeFace::NegY,
            CubeFace::PosZ,
            CubeFace::NegZ,
        ];
        let geometries = faces
            .into_iter()
            .map(|face| {
                build_patch_geometry(&TerrainPatch::root(face), &source, 6_371_000.0, 5, 40.0)
            })
            .collect::<Vec<_>>();

        for (index, geometry) in geometries.iter().enumerate() {
            for (position, normal) in geometry.positions.iter().zip(&geometry.normals) {
                let position = DVec3::from_array(*position);
                let normal = DVec3::from_array(*normal);
                assert!(normal.is_finite());
                assert!((normal.length() - 1.0).abs() < 1e-12);
                assert!(normal.dot(position) > 0.0);
                for other in geometries.iter().skip(index + 1) {
                    for (other_position, other_normal) in other.positions.iter().zip(&other.normals)
                    {
                        if position.distance(DVec3::from_array(*other_position)) < 1e-9 {
                            assert!(
                                normal.distance(DVec3::from_array(*other_normal)) < 1e-12,
                                "shared cube-face vertex has mismatched lighting normal"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn fine_edge_stitch_references_only_coarse_aligned_boundary_samples() {
        let patch = TerrainPatch::root(CubeFace::PosZ).children()[0];
        let geometry = build_patch_geometry_with_stitches(
            &patch,
            &source(),
            6_371_000.0,
            33,
            40.0,
            &[PatchEdge::West],
        );
        let grid_index_count = 32 * 32 * 6;
        for index in geometry.indices.iter().take(grid_index_count) {
            if (*index as usize).is_multiple_of(33) {
                let row = *index as usize / 33;
                assert_eq!(row % 2, 0, "stitched west edge used odd row {row}");
            }
        }
    }

    #[test]
    fn global_uvs_are_stable_across_patch_boundaries() {
        let west = TerrainPatch {
            face: CubeFace::PosZ,
            level: 1,
            tile_x: 0,
            tile_y: 0,
        };
        let east = west.neighbor(PatchEdge::East).patch;
        let source = source();
        let west_geometry = build_patch_geometry(&west, &source, 6_371_000.0, 5, 40.0);
        let east_geometry = build_patch_geometry(&east, &source, 6_371_000.0, 5, 40.0);
        for row in 0..5usize {
            assert_eq!(west_geometry.uvs[row * 5 + 4], east_geometry.uvs[row * 5]);
        }
    }

    fn sample_height(
        patch: &TerrainPatch,
        source: &dyn TerrainSource,
        dir: DVec3,
        radius: f64,
    ) -> f64 {
        let (u0, v0, u1, v1) = patch.uv_bounds();
        let (_, u, v) = face_uv(dir);
        let tu = (u - u0) / (u1 - u0);
        let tv = (v - v0) / (v1 - v0);
        let ud = face_uv_to_direction(patch.face, u0 + tu * (u1 - u0), v0 + tv * (v1 - v0));
        let (lat, lon) = direction_to_lat_lon(ud);
        let _ = radius;
        source.height_m(lat, lon)
    }

    #[test]
    fn patch_world_size_shrinks_with_level() {
        let r = 6_371_000.0;
        let l0 = patch_world_size_m(0, r);
        let l1 = patch_world_size_m(1, r);
        assert!((l0 / l1 - 2.0).abs() < 1e-9);
    }

    /// Determinism: two independent builds of the same patch must produce
    /// byte-identical position arrays (AGENTS.md section 44). Compared via
    /// raw bit patterns so even -0.0 vs 0.0 or NaN payloads would fail.
    #[test]
    fn rebuild_same_patch_produces_byte_identical_positions() {
        let patch = TerrainPatch::for_direction(DVec3::new(0.3, 0.4, 1.0).normalize(), 3);
        let build_geometry = || {
            let source = ProceduralTerrainSource::new(99, 2_000.0, 800.0, 0);
            build_patch_geometry(&patch, &source, 6_371_000.0, 8, 40.0)
        };

        let a = build_geometry();
        let b = build_geometry();

        assert_eq!(a.positions.len(), b.positions.len());
        for (pa, pb) in a.positions.iter().zip(b.positions.iter()) {
            // Byte-level comparison of the f64 coordinates.
            for c in 0..3 {
                assert_eq!(
                    pa[c].to_bits(),
                    pb[c].to_bits(),
                    "patch geometry is not deterministic"
                );
            }
        }
        assert_eq!(a.indices, b.indices);
        assert_eq!(a.normals, b.normals);
    }
}
