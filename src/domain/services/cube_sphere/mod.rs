//! Cube-sphere planetary terrain topology (AGENTS.md sections 20 and 22).
//!
//! A planet's surface is a cube projected to a sphere: six faces, each
//! subdivided by a quadtree. Patch LOD is selected from screen-space error,
//! and skirts hide cracks at LOD boundaries. Geometry is built from the shared
//! [`TerrainSource`](crate::domain::services::terrain_source::TerrainSource), so
//! render and collision stay on one height function.
//!
//! This module is pure domain logic (no Bevy ECS); the streaming/render layers
//! consume it. The implementation is split into cohesive submodules:
//! [`topology`] (faces/edges/UV), [`patch`] (quadtree identity/neighbors),
//! [`lod`] (error selection), and [`mesh`] (geometry generation).

mod topology;
pub use topology::{face_uv, face_uv_to_direction, CubeFace, PatchEdge};

mod patch;
pub use patch::{patch_world_size_m, PatchNeighbor, TerrainPatch};
mod lod;
pub use lod::{
    balance_visible_leaves, lod_for_distance, patch_angular_radius_rad, patches_are_adjacent,
    projected_patch_error_px, screen_space_error_m, select_quadtree_leaves,
    visible_leaves_for_cover, CameraProjection, PatchGeometricError, QuadtreePatchState,
    QuadtreeSelection, QuadtreeSelectionConfig,
};
mod mesh;
pub use mesh::{
    build_patch_geometry, build_patch_geometry_with_stitches, direction_to_lat_lon, PatchGeometry,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::math::DVec3;
    use crate::domain::services::terrain_source::{
        central_angle_deg, ElevationBounds, ProceduralTerrainSource, TerrainSource,
    };
    use std::collections::{BTreeMap, BTreeSet};
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

        fn elevation_bounds_m(&self) -> ElevationBounds {
            ElevationBounds::new(0.0, 0.0)
        }
    }

    #[test]
    fn geometry_samples_each_vertex_and_normal_probe_together() {
        let source = SampleTraceSource::default();
        // A non-root patch so the builder also samples its coarse parent grid.
        let patch = TerrainPatch::for_direction(DVec3::Z, 1);
        build_patch_geometry(&patch, &source, 6_371_000.0, 3, 40.0);

        let samples = source.samples.lock().expect("sample trace lock");
        // The builder first samples the coarse (parent) grid for LOD morphing,
        // then each vertex followed by its four normal probes.
        let coarse_samples = 2 * 2;
        assert_eq!(samples.len(), coarse_samples + 3 * 3 * 5);
        let vertex_start = coarse_samples;
        let (latitude_deg, longitude_deg) = samples[vertex_start];
        assert!(samples[vertex_start + 1..vertex_start + 5]
            .iter()
            .all(|(lat, lon)| {
                central_angle_deg(latitude_deg, longitude_deg, *lat, *lon) < 0.01
            }));
        assert!(
            central_angle_deg(
                latitude_deg,
                longitude_deg,
                samples[vertex_start + 5].0,
                samples[vertex_start + 5].1
            ) > 1.0,
            "the next vertex must follow the first vertex's four normal probes"
        );
    }

    #[test]
    fn patch_geometry_carries_a_parent_level_morph_offset() {
        let radius_m = 6_371_000.0;
        let source = source();
        let patch = TerrainPatch::for_direction(DVec3::Z, 5);
        let geometry = build_patch_geometry(&patch, &source, radius_m, 33, 40.0);
        assert_eq!(geometry.morph_deltas.len(), geometry.positions.len());
        assert!(
            geometry.morph_deltas.iter().any(|delta| delta.abs() > 0.0),
            "a refined patch must have a coarser morph target"
        );

        // A root has no coarser surface, so every morph offset is zero.
        let root = build_patch_geometry(
            &TerrainPatch::root(CubeFace::PosZ),
            &source,
            radius_m,
            33,
            40.0,
        );
        assert!(root.morph_deltas.iter().all(|delta| *delta == 0.0));
    }

    #[test]
    fn odd_morph_vertices_interpolate_the_parent_grid() {
        let radius_m = 6_371_000.0;
        let source = source();
        let patch = TerrainPatch::for_direction(DVec3::Z, 5);
        let resolution = 33usize;
        let geometry = build_patch_geometry(&patch, &source, radius_m, resolution as u32, 0.0);

        // Height = position length - radius because every vertex sits on the
        // radial through its direction. The morph target of an odd vertex must
        // be the midpoint of the surrounding even (parent) samples.
        let height = |i: usize, j: usize| {
            let position = DVec3::from_array(geometry.positions[j * resolution + i]);
            position.length() - radius_m + geometry.morph_deltas[j * resolution + i] as f64
        };
        for j in (0..resolution).step_by(2) {
            for i in (1..resolution - 1).step_by(2) {
                let midpoint = (height(i - 1, j) + height(i + 1, j)) * 0.5;
                assert!(
                    (height(i, j) - midpoint).abs() < 0.05,
                    "vertex ({i},{j}) morph target is not the parent midpoint"
                );
            }
        }
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
    fn localized_refinement_charges_balance_and_fallback_costs() {
        for direction in [DVec3::Z, DVec3::new(1.0, 0.2, 1.0).normalize()] {
            let mut errors = BTreeMap::new();
            for level in 0..14 {
                let patch = TerrainPatch::for_direction(direction, level);
                errors.insert(patch, 100.0);
                for edge in PatchEdge::ALL {
                    let mut neighbor = Some(patch.neighbor(edge).patch);
                    while let Some(patch) = neighbor {
                        errors.entry(patch).or_insert(10.0);
                        neighbor = patch.parent();
                    }
                }
            }
            let state = QuadtreePatchState {
                ready: TerrainPatch::roots().into_iter().collect(),
                ..QuadtreePatchState::default()
            };
            let cost = |patch: &TerrainPatch| if patch.level >= 12 { 10 } else { 1 };
            for (max_target_leaves, max_requested_bytes) in [(60, 900), (300, 48), (300, 900)] {
                let config = QuadtreeSelectionConfig {
                    max_level: 14,
                    max_projected_error_px: 4.0,
                    max_neighbor_level_difference: 1,
                    max_target_leaves,
                    max_requested_bytes,
                };
                let selection = select_quadtree_leaves(&state, &errors, config, cost);
                assert!(selection.target_leaves.len() <= max_target_leaves);
                assert!(
                    6 + selection.requested.iter().map(cost).sum::<u64>() <= max_requested_bytes
                );
                assert_eq!(
                    selection.target_leaves,
                    balance_visible_leaves(&selection.target_leaves, 1)
                );
                let face_area: f64 = selection
                    .target_leaves
                    .iter()
                    .map(|patch| 4.0f64.powi(-(patch.level as i32)))
                    .sum();
                assert!(
                    (face_area - 6.0).abs() < 1e-12,
                    "complete cover must survive budget rejection"
                );
                assert_eq!(selection.visible_leaves, state.ready);
                assert_eq!(
                    selection,
                    select_quadtree_leaves(&state, &errors, config, cost)
                );
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
            max_target_leaves: usize::MAX,
            max_requested_bytes: u64::MAX,
        };

        let mut unready = QuadtreePatchState::default();
        unready.ready.extend(TerrainPatch::roots());
        let fallback = select_quadtree_leaves(&unready, &errors, config, |_| 0);
        assert_eq!(fallback.target_leaves.len(), 9);
        assert!(fallback.target_leaves.contains(&root.children()[0]));
        assert!(fallback.visible_leaves.contains(&root));

        let mut ready = unready.clone();
        ready.ready.extend(root.children());
        let published = select_quadtree_leaves(&ready, &errors, config, |_| 0);
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
                max_target_leaves: usize::MAX,
                max_requested_bytes: u64::MAX,
            },
            |_| 0,
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
                max_target_leaves: usize::MAX,
                max_requested_bytes: u64::MAX,
            },
            |_| 0,
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
