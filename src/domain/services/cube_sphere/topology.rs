//! Cube-sphere face and edge topology: the six faces, their directed edges, and
//! the direction ↔ face-UV projection. Pure domain logic; no ECS.

use crate::domain::math::DVec3;
use serde::{Deserialize, Serialize};

/// The six faces of the cube-sphere.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
