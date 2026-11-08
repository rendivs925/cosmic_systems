//! Terrain-mesh cutouts declared by site structures.
//!
//! A launch pad hardstand is a structure, not terrain, and its flame trench is a
//! hole in that structure. The authoritative height field is unchanged — physics,
//! collision and radar altitude still sample the excavation — but the rendered
//! terrain mesh must stop at the hardstand, because the streamed mesh is far too
//! coarse at pad scale (about 19 m between vertices at the finest LOD, 611 m
//! patches) to resolve a 14 m trench opening. Without a hole, the terrain surface
//! spans the opening and hides the trench entirely.
//!
//! The cutout is registered at runtime by whatever spawns the covering structure,
//! so a site with no facility never loses its ground.

use crate::domain::services::cube_sphere::TerrainMeshCutout;
use bevy::math::DVec3;
use bevy::prelude::*;

/// Body-fixed spherical caps whose terrain mesh is drawn by a structure instead.
#[derive(Resource, Debug, Default, Clone)]
pub struct TerrainMeshCutouts {
    cutouts: Vec<TerrainMeshCutout>,
}

impl TerrainMeshCutouts {
    /// Replace every registered cutout with one cap.
    ///
    /// One cap is enough because a launch facility is the only thing that covers
    /// terrain at present, and a site is only ever presented at one location.
    /// Non-finite or non-positive radii clear the cutout instead of leaving a
    /// stale hole.
    pub fn cover_once(&mut self, direction: DVec3, angular_radius_rad: f64) {
        self.cutouts.clear();
        if let Some(cutout) = TerrainMeshCutout::new(direction, angular_radius_rad) {
            self.cutouts.push(cutout);
        }
    }

    /// Remove every cutout.
    pub fn clear(&mut self) {
        self.cutouts.clear();
    }

    /// The active cutouts.
    pub fn cutouts(&self) -> &[TerrainMeshCutout] {
        &self.cutouts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cover_once_replaces_and_rejects_a_degenerate_cap() {
        let mut cutouts = TerrainMeshCutouts::default();
        cutouts.cover_once(DVec3::X, 1.0e-5);
        assert_eq!(cutouts.cutouts().len(), 1);

        cutouts.cover_once(DVec3::Z, 0.0);
        assert!(
            cutouts.cutouts().is_empty(),
            "a zero radius would cut a cell for no visible reason"
        );

        cutouts.cover_once(DVec3::Y, 0.01);
        assert_eq!(
            cutouts.cutouts().len(),
            1,
            "a new site replaces the old one"
        );
        assert!(cutouts.cutouts()[0].contains(DVec3::Y));
    }
}
