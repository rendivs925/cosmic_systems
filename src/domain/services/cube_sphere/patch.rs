//! Quadtree patch identity and cube-face neighbor topology. Pure domain logic;
//! no ECS.

use super::topology::{face_uv, face_uv_to_direction, CubeFace, PatchEdge};
use crate::domain::math::DVec3;
use serde::{Deserialize, Serialize};

/// A quadtree patch on a cube face: face + level + tile coordinates.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
