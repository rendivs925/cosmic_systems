//! Composition of authoritative terrain elevation layers.

use super::{ss, ElevationBounds, TerrainSource};
use crate::domain::services::cube_sphere::{PatchGeometricError, TerrainPatch};
use std::sync::Arc;

/// Weight applied when blending a detail layer's biome signal into the primary
/// surface.
const TERRAIN_DETAIL_BIOME_WEIGHT: f64 = 0.65;

/// A terrain elevation layer with an explicitly declared conservative envelope.
/// The source must return a contribution, not a radius, in meters.
#[derive(Debug)]
pub struct TerrainElevationLayer {
    source: Arc<dyn TerrainSource>,
    bounds: ElevationBounds,
}

impl TerrainElevationLayer {
    pub fn new(source: Arc<dyn TerrainSource>, bounds: ElevationBounds) -> Self {
        Self { source, bounds }
    }

    pub fn bounds(&self) -> ElevationBounds {
        self.bounds
    }
}

/// LOD-only fade metadata for bounded procedural detail. It is intentionally
/// separate from terrain sampling: camera movement must never change height.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DetailLodFade {
    /// Detail is absent at this LOD and every coarser LOD.
    pub first_faded_level: u32,
    /// Detail is fully represented at this LOD and every finer LOD.
    pub first_full_detail_level: u32,
}

impl DetailLodFade {
    pub fn new(first_faded_level: u32, first_full_detail_level: u32) -> Self {
        assert!(
            first_faded_level <= first_full_detail_level,
            "detail fade must progress from coarse to fine LOD"
        );
        Self {
            first_faded_level,
            first_full_detail_level,
        }
    }

    /// Smoothly increases detail representation from coarse to fine LOD.
    pub fn weight_for_level(self, level: u32) -> f64 {
        if level <= self.first_faded_level {
            return 0.0;
        }
        if level >= self.first_full_detail_level {
            return 1.0;
        }
        let span = (self.first_full_detail_level - self.first_faded_level) as f64;
        let t = (level - self.first_faded_level) as f64 / span;
        t * t * (3.0 - 2.0 * t)
    }
}

/// A bounded procedural contribution plus the LOD metadata used by mesh
/// generation. The fade is not applied by [`LayeredTerrainSource::height_m`].
#[derive(Debug)]
pub struct TerrainDetailLayer {
    source: Arc<dyn TerrainSource>,
    bounds: ElevationBounds,
    pub lod_fade: DetailLodFade,
}

impl TerrainDetailLayer {
    pub fn new(
        source: Arc<dyn TerrainSource>,
        bounds: ElevationBounds,
        lod_fade: DetailLodFade,
    ) -> Self {
        Self {
            source,
            bounds,
            lod_fade,
        }
    }

    pub fn bounds(&self) -> ElevationBounds {
        self.bounds
    }
}

/// The one authoritative composition of a planet's terrain elevation layers.
///
/// The base and macro layers provide global shape, while procedural detail
/// remains in the physical source at every LOD. Rendering may use `lod_fade` to
/// blend a representation, but collision and height queries always sample this
/// sum.
#[derive(Debug)]
pub struct LayeredTerrainSource {
    pub base: TerrainElevationLayer,
    pub macro_elevation: Option<TerrainElevationLayer>,
    pub procedural_detail: Option<TerrainDetailLayer>,
}

impl LayeredTerrainSource {
    pub fn new(
        base: TerrainElevationLayer,
        macro_elevation: Option<TerrainElevationLayer>,
        procedural_detail: Option<TerrainDetailLayer>,
    ) -> Self {
        Self {
            base,
            macro_elevation,
            procedural_detail,
        }
    }

    /// Conservative deterministic bounds for every active elevation layer.
    pub fn elevation_bounds_m(&self) -> ElevationBounds {
        [
            Some(self.base.bounds()),
            self.macro_elevation
                .as_ref()
                .map(TerrainElevationLayer::bounds),
            self.procedural_detail
                .as_ref()
                .map(TerrainDetailLayer::bounds),
        ]
        .into_iter()
        .flatten()
        .fold(ElevationBounds::new(0.0, 0.0), ElevationBounds::combine)
    }

    fn primary_surface(&self) -> &dyn TerrainSource {
        self.macro_elevation
            .as_ref()
            .map(|layer| layer.source.as_ref())
            .unwrap_or(self.base.source.as_ref())
    }
}

impl TerrainSource for LayeredTerrainSource {
    fn height_m(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        let mut height_m = self.base.source.height_m(latitude_deg, longitude_deg);
        if let Some(layer) = &self.macro_elevation {
            height_m += layer.source.height_m(latitude_deg, longitude_deg);
        }
        if let Some(layer) = &self.procedural_detail {
            height_m += layer.source.height_m(latitude_deg, longitude_deg);
        }
        height_m
    }

    fn elevation_bounds_m(&self) -> ElevationBounds {
        LayeredTerrainSource::elevation_bounds_m(self)
    }

    fn patch_geometric_error(&self, patch: &TerrainPatch) -> PatchGeometricError {
        let mut error = self.base.source.patch_geometric_error(patch);
        if let Some(layer) = &self.macro_elevation {
            error = error.combine(layer.source.patch_geometric_error(patch));
        }
        if let Some(layer) = &self.procedural_detail {
            error = error.combine(layer.source.patch_geometric_error(patch));
        }
        error
    }

    fn mesh_height_m(&self, latitude_deg: f64, longitude_deg: f64, patch_level: u32) -> f64 {
        // The base/macro surface is representable at every LOD, but the bounded
        // procedural detail is only worth sampling once a patch is fine enough
        // to represent it. Attenuating it by the declared LOD fade removes
        // far-field shimmer; collision keeps the full physical detail through
        // `height_m`, which is authoritative near the vehicle where patches are
        // fine. The cube-sphere topology keeps matching samples aligned along
        // adjacent patch edges irrespective of LOD.
        let mut height_m = self
            .base
            .source
            .mesh_height_m(latitude_deg, longitude_deg, patch_level);
        if let Some(layer) = &self.macro_elevation {
            height_m += layer
                .source
                .mesh_height_m(latitude_deg, longitude_deg, patch_level);
        }
        if let Some(detail) = &self.procedural_detail {
            let weight = detail.lod_fade.weight_for_level(patch_level);
            height_m += detail.source.height_m(latitude_deg, longitude_deg) * weight;
        }
        height_m
    }

    fn prepare_sample(&self, latitude_deg: f64, longitude_deg: f64) {
        self.base.source.prepare_sample(latitude_deg, longitude_deg);
        if let Some(layer) = &self.macro_elevation {
            layer.source.prepare_sample(latitude_deg, longitude_deg);
        }
        if let Some(layer) = &self.procedural_detail {
            layer.source.prepare_sample(latitude_deg, longitude_deg);
        }
    }

    fn overview_height_m(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        let mut height_m = self
            .base
            .source
            .overview_height_m(latitude_deg, longitude_deg);
        if let Some(layer) = &self.macro_elevation {
            height_m += layer.source.overview_height_m(latitude_deg, longitude_deg);
        }
        height_m
    }

    fn moisture(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        let primary = self.primary_surface().moisture(latitude_deg, longitude_deg);
        let Some(detail) = &self.procedural_detail else {
            return primary;
        };
        primary
            + (detail.source.moisture(latitude_deg, longitude_deg) - primary)
                * TERRAIN_DETAIL_BIOME_WEIGHT
    }

    /// Vegetation cover composes the primary surface's land cover with the
    /// detail layer's climate, then removes it above the treeline. Collision and
    /// mesh generation never consume this; it only selects scatter placement.
    fn vegetation_density(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        let primary = self
            .primary_surface()
            .vegetation_density(latitude_deg, longitude_deg);
        let combined = match &self.procedural_detail {
            Some(detail) => {
                let detail_density = detail
                    .source
                    .vegetation_density(latitude_deg, longitude_deg);
                primary + (detail_density - primary) * TERRAIN_DETAIL_BIOME_WEIGHT
            }
            None => primary,
        };
        let height_m = self.height_m(latitude_deg, longitude_deg);
        let treeline = 1.0 - ss(3_600.0, 4_800.0, height_m);
        (combined * treeline).clamp(0.0, 1.0)
    }

    fn river_strength(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        let primary = self
            .primary_surface()
            .river_strength(latitude_deg, longitude_deg);
        self.procedural_detail
            .as_ref()
            .map(|detail| primary.max(detail.source.river_strength(latitude_deg, longitude_deg)))
            .unwrap_or(primary)
    }

    fn overview_moisture(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        self.primary_surface()
            .overview_moisture(latitude_deg, longitude_deg)
    }

    fn overview_slope_deg(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        self.primary_surface()
            .overview_slope_deg(latitude_deg, longitude_deg)
    }

    fn zone_lat(&self, latitude_deg: f64) -> f64 {
        self.primary_surface().zone_lat(latitude_deg)
    }
}
