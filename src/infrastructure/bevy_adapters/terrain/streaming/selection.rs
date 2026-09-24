//! Terrain visibility, LOD selection, and viewport culling helpers.
//!
//! These are pure, cadence-limited decisions consumed by the streaming system;
//! they never touch render assets or the authoritative terrain source.

use super::{
    patch_neighborhood, CachedTerrainGeometry, FOCUS_RECONCILE_ANGLE_RAD, FOV_RAD,
    LOD_HYSTERESIS_RATIO, MAX_PATCH_LEVEL, MAX_VIEWPORT_UNBALANCED_LEAVES, SCREEN_ERROR_PX,
    SCREEN_HEIGHT_PX, STREAM_RECONCILE_INTERVAL_S, VIEWPORT_POSITION_RECONCILE_RATIO,
    VIEWPORT_PREFETCH_MARGIN_RAD,
};
use crate::domain::services::body_orientation::BodyOrientation;
use crate::domain::services::cube_sphere::{
    patch_angular_radius_rad, projected_patch_error_px, CameraProjection, TerrainPatch,
};
use crate::domain::services::reference_frames::body_fixed_to_planet_inertial_rotation;
use crate::domain::services::terrain_source::{ElevationBounds, TerrainSource};
use crate::infrastructure::bevy_adapters::terrain::render::RenderOrigin;
use bevy::math::DVec3;
use bevy::prelude::*;
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Camera data in the terrain source's body-fixed frame. Streaming uses this
/// only for presentation culling; terrain geometry remains source-authoritative.
#[derive(Debug, Clone, Copy)]
pub(super) struct TerrainViewport {
    pub(super) position_m: DVec3,
    pub(super) forward: DVec3,
    pub(super) right: DVec3,
    pub(super) up: DVec3,
    pub(super) half_fov_rad: f64,
    pub(super) vertical_fov_rad: f64,
    pub(super) viewport_height_px: f64,
    pub(super) horizontal_tan: f64,
    pub(super) vertical_tan: f64,
    pub(super) horizontal_sec: f64,
    pub(super) vertical_sec: f64,
}

/// Per-reconciliation traversal decisions. This is intentionally a small,
/// cadence-limited diagnostic for deciding whether visibility work is removing
/// enough terrain work to justify its CPU cost.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct TerrainCullingStats {
    pub(super) candidates: usize,
    pub(super) horizon_rejected: usize,
    pub(super) frustum_rejected: usize,
}

impl TerrainCullingStats {
    fn record(&mut self, visibility: PatchViewportVisibility) {
        self.candidates += 1;
        match visibility {
            PatchViewportVisibility::Visible => {}
            PatchViewportVisibility::BehindHorizon => self.horizon_rejected += 1,
            PatchViewportVisibility::OutsideFrustum => self.frustum_rejected += 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PatchViewportVisibility {
    Visible,
    BehindHorizon,
    OutsideFrustum,
}

#[derive(Default)]
pub(super) struct TerrainStreamingCadence {
    pub(super) last_reconcile_at_s: f64,
    pub(super) focus_direction: Option<DVec3>,
    pub(super) camera_position_m: Option<DVec3>,
    pub(super) half_fov_rad: Option<f64>,
    pub(super) max_focus_level: Option<u32>,
}

pub(super) fn lod_for_distance_with_hysteresis(
    previous: Option<u32>,
    distance_m: f64,
    radius_m: f64,
) -> u32 {
    let Some(mut level) = previous else {
        return crate::domain::services::cube_sphere::lod_for_distance(
            distance_m,
            radius_m,
            FOV_RAD,
            SCREEN_HEIGHT_PX,
            SCREEN_ERROR_PX,
            MAX_PATCH_LEVEL,
        );
    };
    while level < MAX_PATCH_LEVEL
        && crate::domain::services::cube_sphere::screen_space_error_m(
            crate::domain::services::cube_sphere::patch_world_size_m(level, radius_m),
            distance_m,
            FOV_RAD,
            SCREEN_HEIGHT_PX,
        ) > SCREEN_ERROR_PX * (1.0 + LOD_HYSTERESIS_RATIO)
    {
        level += 1;
    }
    while level > 0
        && crate::domain::services::cube_sphere::screen_space_error_m(
            crate::domain::services::cube_sphere::patch_world_size_m(level - 1, radius_m),
            distance_m,
            FOV_RAD,
            SCREEN_HEIGHT_PX,
        ) <= SCREEN_ERROR_PX * (1.0 - LOD_HYSTERESIS_RATIO)
    {
        level -= 1;
    }
    level
}

pub(super) fn should_reconcile_terrain(
    cadence: &TerrainStreamingCadence,
    now_s: f64,
    focus_direction: DVec3,
    camera_position_m: DVec3,
    half_fov_rad: Option<f64>,
    completed_generation: bool,
) -> bool {
    if completed_generation || cadence.focus_direction.is_none() {
        return true;
    }
    if now_s - cadence.last_reconcile_at_s >= STREAM_RECONCILE_INTERVAL_S {
        return true;
    }
    if cadence
        .focus_direction
        .is_some_and(|previous| previous.dot(focus_direction) < FOCUS_RECONCILE_ANGLE_RAD.cos())
    {
        return true;
    }
    if cadence.camera_position_m.is_some_and(|previous| {
        previous.distance(camera_position_m)
            > previous.length().max(1.0) * VIEWPORT_POSITION_RECONCILE_RATIO
    }) {
        return true;
    }
    cadence.half_fov_rad != half_fov_rad
}

pub(super) fn terrain_viewport(
    camera_query: &Query<(&Camera, &Transform, &Projection), With<Camera3d>>,
    render_origin: &RenderOrigin,
    orientation: &BodyOrientation,
) -> Option<TerrainViewport> {
    let (camera, transform, projection) = camera_query
        .iter()
        .find(|(camera, _, _)| camera.is_active)?;
    let vertical_fov_rad = match projection {
        Projection::Perspective(perspective) => perspective.fov as f64,
        _ => return None,
    };
    let aspect_ratio = camera
        .logical_viewport_size()
        .filter(|size| size.y > 0.0)
        .map(|size| (size.x / size.y) as f64)
        .unwrap_or(16.0 / 9.0);
    let viewport_height_px = camera
        .physical_viewport_size()
        .filter(|size| size.y > 0)
        .map_or(SCREEN_HEIGHT_PX, |size| f64::from(size.y));
    let horizontal_fov_rad = 2.0 * ((vertical_fov_rad * 0.5).tan() * aspect_ratio).atan();
    let body_to_inertial = body_fixed_to_planet_inertial_rotation(orientation);
    let inertial_to_body = body_to_inertial.inverse();
    let camera_position_inertial = render_origin.origin + transform.translation.as_dvec3();
    let forward_inertial = transform.forward().as_vec3().as_dvec3();
    let right_inertial = transform.right().as_vec3().as_dvec3();
    let up_inertial = transform.up().as_vec3().as_dvec3();
    let horizontal_half_fov_rad = horizontal_fov_rad * 0.5 + VIEWPORT_PREFETCH_MARGIN_RAD;
    let vertical_half_fov_rad = vertical_fov_rad * 0.5 + VIEWPORT_PREFETCH_MARGIN_RAD;

    Some(TerrainViewport {
        position_m: inertial_to_body * camera_position_inertial,
        forward: (inertial_to_body * forward_inertial).normalize_or_zero(),
        right: (inertial_to_body * right_inertial).normalize_or_zero(),
        up: (inertial_to_body * up_inertial).normalize_or_zero(),
        half_fov_rad: vertical_fov_rad.max(horizontal_fov_rad) * 0.5,
        vertical_fov_rad,
        viewport_height_px,
        horizontal_tan: horizontal_half_fov_rad.tan(),
        vertical_tan: vertical_half_fov_rad.tan(),
        horizontal_sec: horizontal_half_fov_rad.cos().recip(),
        vertical_sec: vertical_half_fov_rad.cos().recip(),
    })
}

pub(super) fn apply_selection_hysteresis(
    errors: &mut BTreeMap<TerrainPatch, f64>,
    previous_leaves: &BTreeSet<TerrainPatch>,
) {
    let split_threshold = SCREEN_ERROR_PX * (1.0 + LOD_HYSTERESIS_RATIO);
    let merge_threshold = SCREEN_ERROR_PX * (1.0 - LOD_HYSTERESIS_RATIO);
    for (patch, error_px) in errors {
        let was_split = previous_leaves
            .iter()
            .any(|leaf| patch.level < leaf.level && patch.is_ancestor_of(leaf));
        if was_split && *error_px > merge_threshold {
            *error_px = error_px.max(SCREEN_ERROR_PX * (1.0 + 1e-12));
        } else if !was_split && *error_px <= split_threshold {
            *error_px = error_px.min(SCREEN_ERROR_PX);
        }
    }
}

/// Keep generated refinement selected until it has actually left the expanded
/// viewport. The moving 3x3 error neighborhood otherwise drops a tile as soon
/// as its focus cell changes, even though it remains visible on screen.
pub(super) fn retain_visible_detail_errors(
    errors: &mut BTreeMap<TerrainPatch, f64>,
    previous_target_leaves: &BTreeSet<TerrainPatch>,
    generated: &HashMap<TerrainPatch, CachedTerrainGeometry>,
    viewport: Option<&TerrainViewport>,
    radius_m: f64,
    elevation_bounds: ElevationBounds,
) {
    let Some(viewport) = viewport else {
        return;
    };
    for leaf in previous_target_leaves {
        if leaf.level == 0
            || !generated.contains_key(leaf)
            || !patch_intersects_viewport(*leaf, Some(viewport), radius_m, elevation_bounds)
        {
            continue;
        }
        // Force the existing leaf's ancestor path to remain split. The leaf
        // itself must not be forced, or selection would refine one more level.
        let mut ancestor = leaf.parent();
        while let Some(patch) = ancestor {
            errors
                .entry(patch)
                .and_modify(|error| *error = error.max(SCREEN_ERROR_PX * (1.0 + 1e-12)))
                .or_insert(SCREEN_ERROR_PX * (1.0 + 1e-12));
            ancestor = patch.parent();
        }
    }
}

/// Intersect the presentation camera's forward ray with the terrain sphere.
/// LOD selection must use the same focus as viewport culling; using the rocket
/// position here generates detailed tiles behind a free or orbital camera.
pub(super) fn viewport_focus_direction(
    viewport: Option<&TerrainViewport>,
    radius_m: f64,
    fallback_direction: DVec3,
) -> DVec3 {
    let Some(viewport) = viewport else {
        return fallback_direction;
    };
    let b = viewport.position_m.dot(viewport.forward);
    let c = viewport.position_m.length_squared() - radius_m * radius_m;
    let discriminant = b * b - c;
    if discriminant < 0.0 {
        return fallback_direction;
    }
    let distance_m = -b - discriminant.sqrt();
    if distance_m < 0.0 {
        return fallback_direction;
    }
    (viewport.position_m + viewport.forward * distance_m).normalize_or_zero()
}

/// Refinement is published only when every child replacing a parent is ready.
/// Once a child intersects the viewport, retain its selected sibling group and
/// ancestors as a bounded prefetch unit. Culling individual siblings would
/// strand the parent forever because publication requires a complete group.
pub(super) fn add_viewport_lod_group(
    patch: TerrainPatch,
    selected: &BTreeSet<TerrainPatch>,
    requested: &mut BTreeSet<TerrainPatch>,
) {
    let mut current = patch;
    loop {
        let Some(parent) = current.parent() else {
            requested.insert(current);
            break;
        };
        for sibling in parent.children() {
            if selected.contains(&sibling) {
                requested.insert(sibling);
            }
        }
        current = parent;
    }
}

/// Root coverage is scoped to the camera's conservative viewport. The root
/// containing the active focus is retained as an asynchronous launch fallback.
pub(super) fn root_requests_for_viewport(
    focused_root: TerrainPatch,
    viewport: Option<&TerrainViewport>,
    radius_m: f64,
    elevation_bounds: ElevationBounds,
) -> BTreeSet<TerrainPatch> {
    let mut roots: BTreeSet<_> = TerrainPatch::roots()
        .into_iter()
        .filter(|root| patch_intersects_viewport(*root, viewport, radius_m, elevation_bounds))
        .collect();
    roots.insert(focused_root);
    roots
}

/// Conservative bounding-sphere frustum test. It intentionally retains a small
/// margin for smooth camera motion; patches outside it are not requested,
/// rendered, or retained in the cache.
pub(super) fn patch_intersects_viewport(
    patch: TerrainPatch,
    viewport: Option<&TerrainViewport>,
    radius_m: f64,
    elevation_bounds: ElevationBounds,
) -> bool {
    matches!(
        patch_viewport_visibility(patch, viewport, radius_m, elevation_bounds),
        PatchViewportVisibility::Visible
    )
}

pub(super) fn patch_viewport_visibility(
    patch: TerrainPatch,
    viewport: Option<&TerrainViewport>,
    radius_m: f64,
    elevation_bounds: ElevationBounds,
) -> PatchViewportVisibility {
    let Some(viewport) = viewport else {
        return PatchViewportVisibility::Visible;
    };
    if viewport.forward.length_squared() < 0.5 {
        return PatchViewportVisibility::Visible;
    }

    if patch_is_behind_horizon(patch, viewport.position_m, radius_m, elevation_bounds) {
        return PatchViewportVisibility::BehindHorizon;
    }

    let (bounding_center_m, bounding_radius_m) =
        patch_bounding_sphere(patch, radius_m, elevation_bounds);
    if sphere_intersects_viewport_frustum(viewport, bounding_center_m, bounding_radius_m) {
        PatchViewportVisibility::Visible
    } else {
        PatchViewportVisibility::OutsideFrustum
    }
}

/// Conservative rectangular-frustum test for a terrain bounding sphere. The
/// viewport coefficients include the streaming prefetch margin, while the
/// radius expansion keeps limb and edge tiles intact.
pub(super) fn sphere_intersects_viewport_frustum(
    viewport: &TerrainViewport,
    bounding_center_m: DVec3,
    bounding_radius_m: f64,
) -> bool {
    let to_center = bounding_center_m - viewport.position_m;
    if to_center.length_squared() <= bounding_radius_m * bounding_radius_m {
        return true;
    }

    let forward_distance_m = viewport.forward.dot(to_center);
    if forward_distance_m + bounding_radius_m <= 0.0 {
        return false;
    }

    let depth_m = forward_distance_m.max(0.0);
    viewport.right.dot(to_center).abs()
        <= depth_m * viewport.horizontal_tan + bounding_radius_m * viewport.horizontal_sec
        && viewport.up.dot(to_center).abs()
            <= depth_m * viewport.vertical_tan + bounding_radius_m * viewport.vertical_sec
}

/// A conservative sphere enclosing the patch at both extrema of the terrain
/// source's elevation interval. Centering at the radial midpoint is much tighter
/// than adding the entire height range to a mean-radius sphere, while still
/// retaining silhouettes at either declared source extreme.
pub(super) fn patch_bounding_sphere(
    patch: TerrainPatch,
    radius_m: f64,
    elevation_bounds: ElevationBounds,
) -> (DVec3, f64) {
    let center = patch.center_direction();
    let patch_radius_rad = patch_angular_radius_rad(&patch);

    let min_surface_radius_m = radius_m + elevation_bounds.min_m;
    let max_surface_radius_m = radius_m + elevation_bounds.max_m;
    let center_radius_m = (min_surface_radius_m + max_surface_radius_m) * 0.5;
    let radial_half_range_m = (max_surface_radius_m - min_surface_radius_m) * 0.5;
    // The chord reaches the farthest patch corner. An arc-length estimate here
    // is too small and could cull a tile that still contributes to the limb.
    let angular_radius_m = 2.0 * max_surface_radius_m * (patch_radius_rad * 0.5).sin();
    (
        center * center_radius_m,
        angular_radius_m + radial_half_range_m,
    )
}

/// Reject a quadtree node only when its conservative bounding sphere lies
/// wholly behind the tangent plane of the planet as seen by the camera.
///
/// This runs before queueing a `TerrainSource` bake, including coarse roots.
/// Nodes behind the limb never consume a task.
pub(super) fn patch_is_behind_horizon(
    patch: TerrainPatch,
    camera_position_m: DVec3,
    radius_m: f64,
    elevation_bounds: ElevationBounds,
) -> bool {
    let camera_distance_m = camera_position_m.length();
    if camera_distance_m <= radius_m {
        return false;
    }

    let (bounding_center_m, bounding_radius_m) =
        patch_bounding_sphere(patch, radius_m, elevation_bounds);

    camera_position_m.dot(bounding_center_m) + camera_distance_m * bounding_radius_m
        < radius_m * radius_m
}

/// Sample the highest projected-error patches first within a bounded traversal.
/// Breadth-first sampling exhausts the allowance on coarse terrain before
/// reaching close surface detail. All viewport-visible roots remain candidates.
pub(super) fn projected_errors_for_viewport(
    viewport: &TerrainViewport,
    max_level: u32,
    radius_m: f64,
    camera: CameraProjection,
    source: &dyn TerrainSource,
    elevation_bounds: ElevationBounds,
) -> (BTreeMap<TerrainPatch, f64>, TerrainCullingStats) {
    let mut errors = BTreeMap::new();
    let mut culling = TerrainCullingStats::default();
    let mut pending = Vec::new();
    let error_for = |patch: &TerrainPatch| {
        projected_patch_error_px(patch, source.patch_geometric_error(patch), radius_m, camera)
    };
    for patch in TerrainPatch::roots() {
        let visibility =
            patch_viewport_visibility(patch, Some(viewport), radius_m, elevation_bounds);
        culling.record(visibility);
        if visibility == PatchViewportVisibility::Visible {
            pending.push((patch, error_for(&patch)));
        }
    }
    let mut target_leaf_count = TerrainPatch::roots().len();

    while let Some(index) = pending
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.1.total_cmp(&b.1).then_with(|| a.0.cmp(&b.0)))
        .map(|(index, _)| index)
    {
        if target_leaf_count + 3 > MAX_VIEWPORT_UNBALANCED_LEAVES {
            break;
        }
        let (patch, error_px) = pending.swap_remove(index);
        errors.insert(patch, error_px);
        if patch.level >= max_level || error_px <= SCREEN_ERROR_PX {
            continue;
        }

        target_leaf_count += 3;
        for child in patch.children() {
            let visibility =
                patch_viewport_visibility(child, Some(viewport), radius_m, elevation_bounds);
            culling.record(visibility);
            if visibility == PatchViewportVisibility::Visible {
                pending.push((child, error_for(&child)));
            }
        }
    }

    (errors, culling)
}

/// Populate the camera focus neighborhood when no presentation camera is
/// available. This keeps startup fallback bounded; regular rendering always
/// uses [`projected_errors_for_viewport`].
pub(super) fn projected_errors_for_focus(
    focus_direction: DVec3,
    max_level: u32,
    radius_m: f64,
    camera: CameraProjection,
    source: &dyn TerrainSource,
) -> BTreeMap<TerrainPatch, f64> {
    let mut errors = BTreeMap::new();
    for level in 0..max_level {
        let focus = TerrainPatch::for_direction(focus_direction, level);
        for patch in patch_neighborhood(focus) {
            errors.insert(
                patch,
                projected_patch_error_px(
                    &patch,
                    source.patch_geometric_error(&patch),
                    radius_m,
                    camera,
                ),
            );
        }
    }
    errors
}
