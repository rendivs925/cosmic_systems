//! Composition of authoritative terrain elevation layers.

use super::{ss, ElevationBounds, PadFlatZone, TerrainSource};
use crate::domain::services::cube_sphere::{PatchGeometricError, TerrainPatch};
use bevy::math::DVec3;
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
    /// `(guard_m, full_m)` above the local sea datum. Inside `guard_m` the
    /// negative half of this layer is suppressed so synthetic relief cannot
    /// manufacture coastline; by `full_m` the layer is fully signed again.
    /// Opt-in, because only a body with an ocean can be flooded by it.
    sea_level_guard: Option<(f64, f64)>,
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
            sea_level_guard: None,
        }
    }

    /// Stop this layer from lowering ground that is within `guard_m` of the sea
    /// datum. Presentation-scale relief must not invent a shoreline.
    pub fn with_sea_level_guard(mut self, guard_m: f64, full_m: f64) -> Self {
        self.sea_level_guard = Some((guard_m, full_m));
        self
    }

    pub fn bounds(&self) -> ElevationBounds {
        self.bounds
    }
}

/// A prepared level platform at a launch site, applied to the composed surface.
///
/// This is a *grading*, not a detail fade: the final height is pulled to the
/// site's own composed height inside the flat radius and restored over the
/// blend, so the ground under the facility is level at every LOD, for collision,
/// for radar altitude and for the rendered mesh at once. Suppressing only the
/// procedural detail layer (as a `PadFlatZone` does on its own) cannot do that,
/// because the measured base slope remains: the facility then sits on ground
/// that is smooth but still tilted, and any flat apron drawing over it floats on
/// one edge and buries on the other.
///
/// The anchor height is sampled from the ungraded composition before grading is
/// installed, so the launch elevation — and therefore every physics baseline —
/// is unchanged.
#[derive(Debug, Clone, Copy)]
struct SiteGrade {
    zone: PadFlatZone,
    height_m: f64,
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
    site_grades: Vec<SiteGrade>,
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
            site_grades: Vec::new(),
        }
    }

    /// Grade the given launch sites level in the composed surface.
    ///
    /// Each site's height is sampled from *this* composition before any grading
    /// exists, so the value pinned under the pad is the same height the launch
    /// spawn and the physics baseline already use.
    pub fn with_graded_sites(mut self, zones: Vec<PadFlatZone>) -> Self {
        let sites = zones
            .into_iter()
            .map(|zone| {
                let (latitude_deg, longitude_deg) = zone.terrain_lat_lon();
                let height_m = self.ungraded_height_m(latitude_deg, longitude_deg);
                SiteGrade { zone, height_m }
            })
            .collect();
        self.site_grades = sites;
        self
    }

    /// Pull a composed height toward the nearest site's graded height. Outside
    /// every site's blend radius the weight is one and the height is untouched.
    fn grade(&self, latitude_deg: f64, longitude_deg: f64, height_m: f64) -> f64 {
        if self.site_grades.is_empty() {
            return height_m;
        }
        let latitude = latitude_deg.to_radians();
        let longitude = longitude_deg.to_radians();
        let direction = DVec3::new(
            latitude.cos() * longitude.cos(),
            latitude.sin(),
            latitude.cos() * longitude.sin(),
        );
        let mut weight = 1.0;
        let mut graded_height_m = height_m;
        for site in &self.site_grades {
            let attenuation = site.zone.attenuation(direction);
            if attenuation < weight {
                weight = attenuation;
                graded_height_m = site.height_m;
            }
        }
        height_m * weight + graded_height_m * (1.0 - weight)
    }

    /// How much of the negative half of the synthetic detail contribution is
    /// removed: `0` leaves the layer fully signed, `1` removes every negative
    /// octave. Always `0` when the layer has no sea-level guard.
    ///
    /// Bounded procedural relief must not fabricate coastline. On a coastal
    /// plain the measured base already sits within a metre or two of the sea
    /// datum, and ±24 m of signed detail would push half the plain below sea
    /// level, where the ocean cap then renders it as open water. Near the datum
    /// the contribution is allowed to add relief but not to lower ground, and it
    /// returns to fully signed relief above the layer's `full_m`.
    fn negative_removal_weight(
        &self,
        guard: Option<(f64, f64)>,
        latitude_deg: f64,
        longitude_deg: f64,
        base_height_m: f64,
    ) -> f64 {
        let Some((guard_m, full_m)) = guard else {
            return 0.0;
        };
        let sea_level_m = self
            .base
            .source
            .sea_level_height_m(latitude_deg, longitude_deg);
        1.0 - ss(sea_level_m + guard_m, sea_level_m + full_m, base_height_m)
    }

    fn detail_contribution_m(&self, detail_m: f64, negative_removal: f64) -> f64 {
        detail_m + (-detail_m).max(0.0) * negative_removal
    }

    fn ungraded_height_m(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        let mut height_m = self.base.source.height_m(latitude_deg, longitude_deg);
        if let Some(layer) = &self.macro_elevation {
            height_m += layer.source.height_m(latitude_deg, longitude_deg);
        }
        if let Some(layer) = &self.procedural_detail {
            let detail_m = layer.source.height_m(latitude_deg, longitude_deg);
            let negative_removal = self.negative_removal_weight(
                layer.sea_level_guard,
                latitude_deg,
                longitude_deg,
                height_m,
            );
            height_m += self.detail_contribution_m(detail_m, negative_removal);
        }
        height_m
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
        self.grade(
            latitude_deg,
            longitude_deg,
            self.ungraded_height_m(latitude_deg, longitude_deg),
        )
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
        // adjacent patch edges irrespective of LOD. The site grade is applied
        // after the fade so a graded launch site stays level at every LOD.
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
            let detail_m = detail.source.height_m(latitude_deg, longitude_deg) * weight;
            // The mesh must apply the same sea-level guard as the physical
            // surface, or the rendered coast would disagree with collision.
            let negative_removal = self.negative_removal_weight(
                detail.sea_level_guard,
                latitude_deg,
                longitude_deg,
                height_m,
            );
            height_m += self.detail_contribution_m(detail_m, negative_removal);
        }
        self.grade(latitude_deg, longitude_deg, height_m)
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

    fn sea_level_height_m(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        self.base
            .source
            .sea_level_height_m(latitude_deg, longitude_deg)
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

    fn flow(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        let primary = self.primary_surface().flow(latitude_deg, longitude_deg);
        self.procedural_detail
            .as_ref()
            .map(|detail| primary.max(detail.source.flow(latitude_deg, longitude_deg)))
            .unwrap_or(primary)
    }

    fn lake_depth_m(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        self.primary_surface()
            .lake_depth_m(latitude_deg, longitude_deg)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Latitude ramp of 1 m per millimetre of latitude, so a 90 m offset is
    /// about 0.8 m of relief: enough that an ungraded pad would visibly tilt.
    #[derive(Debug)]
    struct SlopedBase;
    impl TerrainSource for SlopedBase {
        fn height_m(&self, latitude_deg: f64, _: f64) -> f64 {
            latitude_deg * 1_000.0
        }
        fn elevation_bounds_m(&self) -> ElevationBounds {
            ElevationBounds::new(-90_000.0, 90_000.0)
        }
    }

    fn graded_source() -> LayeredTerrainSource {
        LayeredTerrainSource::new(
            TerrainElevationLayer::new(
                Arc::new(SlopedBase),
                ElevationBounds::new(-90_000.0, 90_000.0),
            ),
            None,
            None,
        )
        .with_graded_sites(vec![PadFlatZone::new(0.0, 0.0, 6_371_000.0, 90.0, 320.0)])
    }

    /// The pad footprint must be level in the composed surface, which is what
    /// mesh, collision and radar all read. A detail-only fade left the measured
    /// base slope in place, so a flat apron drawn over it floated on one edge.
    #[test]
    fn graded_site_is_level_under_the_pad_and_returns_to_natural_terrain() {
        let source = graded_source();
        assert!(
            source.height_m(0.0, 0.0).abs() < 1e-9,
            "the anchor height must be preserved exactly"
        );
        // ~89 m north: inside the 90 m flat radius.
        let inside = source.height_m(0.0008, 0.0);
        assert!(
            inside.abs() < 1e-6,
            "inside the pad the surface must be level, got {inside}"
        );
        // ~556 m north: beyond the 320 m blend, so natural terrain is restored.
        let outside = source.height_m(0.005, 0.0);
        let natural = SlopedBase.height_m(0.005, 0.0);
        assert!(
            (outside - natural).abs() < 1e-6,
            "outside the blend the surface must be untouched: {outside} vs {natural}"
        );
        // ~222 m north: inside the blend, strictly between the two.
        let blended = source.height_m(0.002, 0.0);
        assert!(
            blended > 0.0 && blended < SlopedBase.height_m(0.002, 0.0),
            "the blend must interpolate, got {blended}"
        );
        // The mesh path grades too, otherwise a rendered patch would disagree
        // with collision at the site.
        let mesh_inside = source.mesh_height_m(0.0008, 0.0, 14);
        assert!(
            mesh_inside.abs() < 1e-6,
            "mesh height must be graded: {mesh_inside}"
        );
    }

    #[derive(Debug)]
    struct ConstantGround(f64);
    impl TerrainSource for ConstantGround {
        fn height_m(&self, _: f64, _: f64) -> f64 {
            self.0
        }
        fn elevation_bounds_m(&self) -> ElevationBounds {
            ElevationBounds::new(-1_000.0, 10_000.0)
        }
    }

    #[derive(Debug)]
    struct ConstantDetail(f64);
    impl TerrainSource for ConstantDetail {
        fn height_m(&self, _: f64, _: f64) -> f64 {
            self.0
        }
        fn elevation_bounds_m(&self) -> ElevationBounds {
            ElevationBounds::new(-36.0, 24.0)
        }
    }

    fn layered_with(base_m: f64, detail_m: f64) -> LayeredTerrainSource {
        LayeredTerrainSource::new(
            TerrainElevationLayer::new(
                Arc::new(ConstantGround(base_m)),
                ElevationBounds::new(-1_000.0, 10_000.0),
            ),
            None,
            Some(
                TerrainDetailLayer::new(
                    Arc::new(ConstantDetail(detail_m)),
                    ElevationBounds::new(-36.0, 24.0),
                    DetailLodFade::new(0, 0),
                )
                .with_sea_level_guard(3.0, 40.0),
            ),
        )
    }

    /// Synthetic relief may add land but must not manufacture sea: on a coastal
    /// plain a negative detail octave would push ground below the datum, where
    /// the ocean cap renders it as open water.
    #[test]
    fn synthetic_detail_cannot_create_sea_near_the_datum() {
        let near_sea = layered_with(1.0, -10.0);
        assert!(
            (near_sea.height_m(0.0, 0.0) - 1.0).abs() < 1e-9,
            "negative detail must not lower ground at the datum"
        );
        let high = layered_with(100.0, -10.0);
        assert!(
            (high.height_m(0.0, 0.0) - 90.0).abs() < 1e-9,
            "signed detail must return away from the datum"
        );
        let positive = layered_with(1.0, 10.0);
        assert!(
            (positive.height_m(0.0, 0.0) - 11.0).abs() < 1e-9,
            "positive detail is always applied"
        );
        // The mesh path must agree with the physical path.
        assert!((near_sea.mesh_height_m(0.0, 0.0, 14) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn a_source_without_sites_is_unchanged() {
        let source = LayeredTerrainSource::new(
            TerrainElevationLayer::new(
                Arc::new(SlopedBase),
                ElevationBounds::new(-90_000.0, 90_000.0),
            ),
            None,
            None,
        );
        assert_eq!(source.height_m(0.0, 0.0), 0.0);
        assert_eq!(source.height_m(0.004, 0.0), SlopedBase.height_m(0.004, 0.0));
    }
}
