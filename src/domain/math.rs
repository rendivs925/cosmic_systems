//! Numerical representations used by domain calculations.
//!
//! These aliases keep the domain independent of Bevy while preserving the
//! established `glam` vector, matrix, and quaternion implementations.

pub use glam::{DMat3, DQuat, DVec3, Vec3};
