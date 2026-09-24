//! Patch LOD error metrics and quadtree leaf selection. Pure domain logic;
//! no ECS. The streaming layer consumes these decisions.

use super::{face_uv_to_direction, patch_world_size_m, PatchEdge, TerrainPatch};
use crate::domain::math::DVec3;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, HashMap};

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
    /// Conservative terrain error from one source-wide elevation envelope.
    /// Sources with indexed terrain data can provide tighter per-patch values.
    pub fn from_elevation_bounds(elevation_min_m: f64, elevation_max_m: f64) -> Self {
        let elevation_range_m = elevation_max_m - elevation_min_m;
        Self {
            elevation_range_m,
            child_to_parent_deviation_m: elevation_range_m,
        }
    }

    /// Conservatively combine independently sampled terrain elevation layers.
    pub fn combine(self, other: Self) -> Self {
        Self {
            elevation_range_m: self.elevation_range_m + other.elevation_range_m,
            child_to_parent_deviation_m: self.child_to_parent_deviation_m
                + other.child_to_parent_deviation_m,
        }
    }

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

/// Angular radius of a spherical cap enclosing the patch's corner directions.
pub fn patch_angular_radius_rad(patch: &TerrainPatch) -> f64 {
    let center = patch.center_direction();
    let (u0, v0, u1, v1) = patch.uv_bounds();
    [(u0, v0), (u1, v0), (u0, v1), (u1, v1)]
        .into_iter()
        .map(|(u, v)| {
            center
                .dot(face_uv_to_direction(patch.face, u, v))
                .clamp(-1.0, 1.0)
                .acos()
        })
        .fold(0.0, f64::max)
}

/// Project error using the nearest distance to the enclosing spherical cap.
/// Center distance underestimates error for a camera standing near a large
/// patch's edge, starving the entire near-camera descendant chain of detail.
pub fn projected_patch_error_px(
    patch: &TerrainPatch,
    geometric_error_m: PatchGeometricError,
    planet_radius_m: f64,
    camera: CameraProjection,
) -> f64 {
    let camera_radius_m = camera.position_m.length();
    let angle_rad = patch
        .center_direction()
        .dot(camera.position_m.normalize_or_zero())
        .clamp(-1.0, 1.0)
        .acos();
    let nearest_angle_rad = (angle_rad - patch_angular_radius_rad(patch)).max(0.0);
    let distance_m = ((camera_radius_m - planet_radius_m).powi(2)
        + 4.0 * camera_radius_m * planet_radius_m * (nearest_angle_rad * 0.5).sin().powi(2))
    .sqrt();
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
    /// Complete six-face cover, including splits needed for neighbor balance.
    pub max_target_leaves: usize,
    /// Includes all ancestors retained for progressive readiness fallback.
    pub max_requested_bytes: u64,
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

/// One node queued for refinement, ordered by descending projected error with a
/// deterministic patch-key tie-break. `BinaryHeap` is a max-heap, so the
/// greatest entry pops first.
#[derive(Debug, Clone, Copy, PartialEq)]
struct PendingNode {
    patch: TerrainPatch,
    error_px: f64,
}

impl Eq for PendingNode {}

impl Ord for PendingNode {
    fn cmp(&self, other: &Self) -> Ordering {
        self.error_px
            .total_cmp(&other.error_px)
            .then_with(|| self.patch.cmp(&other.patch))
    }
}

impl PartialOrd for PendingNode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Select a complete, balanced six-face leaf cover from supplied projected errors.
///
/// Readiness only affects `visible_leaves`; `target_leaves` and `requested` are
/// identical for the same visibility, errors, and configuration.
pub fn select_quadtree_leaves(
    state: &QuadtreePatchState,
    projected_errors_px: &BTreeMap<TerrainPatch, f64>,
    config: QuadtreeSelectionConfig,
    patch_bytes: impl Fn(&TerrainPatch) -> u64,
) -> QuadtreeSelection {
    let mut target_leaves: BTreeSet<_> = TerrainPatch::roots().into_iter().collect();
    let mut requested_bytes: u64 = target_leaves.iter().map(&patch_bytes).sum();
    // Highest projected error refines first; equal errors break toward the
    // greater patch key (the previous `max_by` order). A heap keeps each step
    // O(log n) instead of scanning the whole pending set.
    let mut pending: BinaryHeap<PendingNode> = TerrainPatch::roots()
        .into_iter()
        .map(|patch| PendingNode {
            patch,
            error_px: projected_errors_px.get(&patch).copied().unwrap_or(0.0),
        })
        .collect();
    // Balance closure re-queries the same neighbours across successive splits.
    // Cross-face neighbour lookup is trigonometric, so memoize within the pass.
    let mut neighbors = NeighborCache::default();

    while let Some(PendingNode {
        patch,
        error_px: projected_error_px,
    }) = pending.pop()
    {
        if !target_leaves.contains(&patch)
            || patch.level >= config.max_level
            || !state.is_visible(&patch)
            || projected_error_px <= config.max_projected_error_px
        {
            continue;
        }

        // Charge the complete balance closure before mutating the cover. A
        // fixed reserve for balancing cannot bound deeply localized refinement.
        let splits = balanced_split_closure(
            &target_leaves,
            patch,
            config.max_neighbor_level_difference,
            &mut neighbors,
        );
        if target_leaves.len() + 3 * splits.len() > config.max_target_leaves.max(6) {
            continue;
        }
        let added_bytes: u64 = splits
            .iter()
            .flat_map(TerrainPatch::children)
            .map(|child| patch_bytes(&child))
            .sum();
        if requested_bytes.saturating_add(added_bytes) > config.max_requested_bytes {
            continue;
        }
        requested_bytes += added_bytes;
        for split in splits {
            target_leaves.remove(&split);
            for child in split.children() {
                target_leaves.insert(child);
                pending.push(PendingNode {
                    patch: child,
                    error_px: projected_errors_px.get(&child).copied().unwrap_or(0.0),
                });
            }
        }
    }

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

    let visible_leaves = visible_leaves_for_cover(&target_leaves, state);

    QuadtreeSelection {
        target_leaves,
        requested,
        visible_leaves,
    }
}

/// Resolve the ready-cover leaves for an already-balanced target cover.
///
/// Readiness affects only this result: `target_leaves` and `requested` depend
/// solely on projected error, configuration, and visibility, none of which
/// change when a cached variant is evicted. A caller that has already selected a
/// cover can therefore refresh visibility without repeating the projected-error
/// traversal.
pub fn visible_leaves_for_cover(
    target_leaves: &BTreeSet<TerrainPatch>,
    state: &QuadtreePatchState,
) -> BTreeSet<TerrainPatch> {
    let mut visible_leaves = BTreeSet::new();
    for root in TerrainPatch::roots() {
        resolve_ready_leaves(root, target_leaves, state, &mut visible_leaves);
    }
    visible_leaves
}

/// Given a balanced cover, find the coarser neighbors that must split with a
/// leaf. Neighbor lookup is shared with full-cover balancing, including seams.
fn balanced_split_closure(
    leaves: &BTreeSet<TerrainPatch>,
    patch: TerrainPatch,
    max_level_difference: u32,
    neighbors: &mut NeighborCache,
) -> BTreeSet<TerrainPatch> {
    let mut splits = BTreeSet::new();
    let mut pending = vec![patch];
    while let Some(patch) = pending.pop() {
        if !splits.insert(patch) {
            continue;
        }
        for edge in PatchEdge::ALL {
            let mut neighbor = Some(neighbors.neighbor(patch, edge));
            while let Some(candidate) = neighbor {
                if leaves.contains(&candidate) {
                    if candidate.level + max_level_difference < patch.level + 1 {
                        pending.push(candidate);
                    }
                    break;
                }
                neighbor = candidate.parent();
            }
        }
    }
    splits
}

/// Memoizes same-level neighbour lookups for one selection pass. Cube-face
/// neighbours are derived trigonometrically, so repeated queries during balance
/// closure are worth caching.
#[derive(Default)]
struct NeighborCache {
    neighbors: HashMap<(TerrainPatch, PatchEdge), TerrainPatch>,
}

impl NeighborCache {
    fn neighbor(&mut self, patch: TerrainPatch, edge: PatchEdge) -> TerrainPatch {
        if let Some(found) = self.neighbors.get(&(patch, edge)) {
            return *found;
        }
        let found = patch.neighbor(edge).patch;
        self.neighbors.insert((patch, edge), found);
        found
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
        let mut coarser = None;
        'patches: for patch in &balanced {
            for edge in PatchEdge::ALL {
                // A finer patch identifies a coarser neighbor by walking the
                // same-level neighbor's ancestor chain. This covers cube-face
                // seams through `neighbor` without comparing every leaf pair.
                let mut neighbor_ancestor = patch.neighbor(edge).patch.parent();
                while let Some(candidate) = neighbor_ancestor {
                    if candidate.level + max_level_difference < patch.level
                        && balanced.contains(&candidate)
                    {
                        coarser = Some(candidate);
                        break 'patches;
                    }
                    neighbor_ancestor = candidate.parent();
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
