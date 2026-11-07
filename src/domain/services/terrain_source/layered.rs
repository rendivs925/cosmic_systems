//! Composition of authoritative terrain elevation layers.

use super::{ss, ElevationBounds, PadFlatZone, PadTrenchZone, TerrainSource};
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

/// A coarse, mesh-only relief band for LODs where a few kilometres of feature
/// is representable (TM-02).
///
/// `rise` grows detail from coarse to fine; `fall` removes it again before the
/// fine detail octave takes over. The product is a band that is absent at
/// planet-scale LODs, full where 4 km features resolve, and absent where the
/// 250 m detail already carries the surface. It is applied only by
/// [`LayeredTerrainSource::mesh_height_m`]: collision and physics keep the
/// measured surface.
#[derive(Debug)]
pub struct FarDetailLayer {
    pub source: Arc<dyn TerrainSource>,
    bounds: ElevationBounds,
    pub rise: DetailLodFade,
    pub fall: DetailLodFade,
    sea_level_guard: Option<(f64, f64)>,
}

impl FarDetailLayer {
    pub fn new(
        source: Arc<dyn TerrainSource>,
        bounds: ElevationBounds,
        rise: DetailLodFade,
        fall: DetailLodFade,
    ) -> Self {
        Self {
            source,
            bounds,
            rise,
            fall,
            sea_level_guard: None,
        }
    }

    pub fn with_sea_level_guard(mut self, guard_m: f64, full_m: f64) -> Self {
        self.sea_level_guard = Some((guard_m, full_m));
        self
    }

    pub fn bounds(&self) -> ElevationBounds {
        self.bounds
    }

    /// Representative weight: 0 at planet-scale LODs, 1 in the band.
    pub fn weight_for_level(&self, level: u32) -> f64 {
        self.rise.weight_for_level(level) * (1.0 - self.fall.weight_for_level(level))
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
    /// Mesh-only coarse relief band; never sampled by `height_m` (TM-02).
    pub far_detail: Option<FarDetailLayer>,
    site_grades: Vec<SiteGrade>,
    /// Excavations cut into graded sites after the grade is applied, so the
    /// trench is always the same depth below the levelled platform.
    site_trenches: Vec<PadTrenchZone>,
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
            far_detail: None,
            site_grades: Vec::new(),
            site_trenches: Vec::new(),
        }
    }

    /// Add the coarse mesh-only relief band. It is not part of `height_m`, so
    /// collision, radar altitude and physics never see it.
    pub fn with_far_detail(mut self, layer: FarDetailLayer) -> Self {
        self.far_detail = Some(layer);
        self
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

    /// Raise every graded platform to at least `clearance_m` above the local
    /// sea datum.
    ///
    /// A launch pad is a construction, not a terrain sample. Where the measured
    /// ground is at or just above the waterline (Starbase is 0.1 m, Papua
    /// 0.5 m), a level grade still leaves the tower standing in the sea. This
    /// floors the *platform*, never the surrounding measured surface, and is a
    /// no-op for every site already above the clearance.
    pub fn with_coastal_platform_clearance(mut self, clearance_m: f64) -> Self {
        if !clearance_m.is_finite() || clearance_m <= 0.0 {
            return self;
        }
        for site in &mut self.site_grades {
            let (latitude_deg, longitude_deg) = site.zone.terrain_lat_lon();
            let sea_level_m = self
                .base
                .source
                .sea_level_height_m(latitude_deg, longitude_deg);
            site.height_m = site.height_m.max(sea_level_m + clearance_m);
        }
        self
    }

    /// Cut the given excavations into the graded sites.
    ///
    /// Applied after the grade, so each trench is exactly its declared depth
    /// below the levelled platform at every LOD, for the mesh, for collision and
    /// for radar altitude alike. A zone whose rectangle excludes the launch
    /// point leaves that point's resting height untouched.
    pub fn with_site_trenches(mut self, zones: Vec<PadTrenchZone>) -> Self {
        self.site_trenches = zones;
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
        let mut height_m = height_m * weight + graded_height_m * (1.0 - weight);
        for trench in &self.site_trenches {
            height_m -= trench.cut_m(direction);
        }
        height_m
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
    ///
    /// The guard is a *land* rule. Once the base is below the datum the ground
    /// is already measured ocean, and suppressing its negative octaves would
    /// rewrite bathymetry (and every terrain normal derived from it) for no
    /// visible benefit. The removal therefore fades back to zero over the
    /// octave's own depth below the datum, which keeps the composed height
    /// continuous at the waterline instead of leaving a cliff.
    fn negative_removal_weight(
        &self,
        guard: Option<(f64, f64)>,
        detail_min_m: f64,
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
        if base_height_m < sea_level_m {
            let shore_fade_m = (-detail_min_m).max(0.0);
            return ss(sea_level_m - shore_fade_m, sea_level_m, base_height_m);
        }
        1.0 - ss(sea_level_m + guard_m, sea_level_m + full_m, base_height_m)
    }

    fn detail_contribution_m(&self, detail_m: f64, negative_removal: f64) -> f64 {
        detail_m + (-detail_m).max(0.0) * negative_removal
    }

    /// Zero at and below the sea datum, one above the guard band. Used for
    /// mesh-only relief so a coarse patch cannot raise or lower the coastline.
    fn land_only_weight(
        &self,
        guard: Option<(f64, f64)>,
        latitude_deg: f64,
        longitude_deg: f64,
        base_height_m: f64,
    ) -> f64 {
        let Some((guard_m, full_m)) = guard else {
            return 1.0;
        };
        let sea_level_m = self
            .base
            .source
            .sea_level_height_m(latitude_deg, longitude_deg);
        ss(sea_level_m + guard_m, sea_level_m + full_m, base_height_m)
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
                layer.bounds().min_m,
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
        let bounds = [
            Some(self.base.bounds()),
            self.macro_elevation
                .as_ref()
                .map(TerrainElevationLayer::bounds),
            self.procedural_detail
                .as_ref()
                .map(TerrainDetailLayer::bounds),
            self.far_detail.as_ref().map(FarDetailLayer::bounds),
        ]
        .into_iter()
        .flatten()
        .fold(ElevationBounds::new(0.0, 0.0), ElevationBounds::combine);
        // An excavation lowers the surface below every layer's own envelope, so
        // the conservative minimum must include the deepest cut.
        let deepest_m = self
            .site_trenches
            .iter()
            .map(PadTrenchZone::depth_m)
            .fold(0.0, f64::max);
        if deepest_m > 0.0 {
            return ElevationBounds::new(bounds.min_m - deepest_m, bounds.max_m);
        }
        bounds
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

    fn appearance_height_m(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        self.ungraded_height_m(latitude_deg, longitude_deg)
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
                detail.bounds().min_m,
                latitude_deg,
                longitude_deg,
                height_m,
            );
            height_m += self.detail_contribution_m(detail_m, negative_removal);
        }
        if let Some(far) = &self.far_detail {
            let weight = far.weight_for_level(patch_level);
            if weight > 0.0 {
                // Mesh-only relief must not move the coastline: it is applied
                // only on land well above the datum, fading in above the same
                // guard band the fine detail uses. Water and collision keep the
                // measured surface exactly.
                let land_weight = self.land_only_weight(
                    far.sea_level_guard,
                    latitude_deg,
                    longitude_deg,
                    height_m,
                );
                let far_m = far.source.height_m(latitude_deg, longitude_deg) * weight * land_weight;
                height_m += far_m;
            }
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
        // The treeline is an altitude above the local sea datum, not the raw
        // ellipsoidal coordinate (VG-01).
        let height_m = self.height_m(latitude_deg, longitude_deg)
            - self
                .base
                .source
                .sea_level_height_m(latitude_deg, longitude_deg);
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

    #[derive(Debug)]
    struct ConstantTerrain(f64);
    impl TerrainSource for ConstantTerrain {
        fn height_m(&self, _: f64, _: f64) -> f64 {
            self.0
        }
        fn elevation_bounds_m(&self) -> ElevationBounds {
            ElevationBounds::new(self.0, self.0)
        }
    }

    /// TM-02: the coarse relief band is present only on the mesh, only where
    /// the patch can represent a few kilometres, and never at planet scale.
    #[test]
    fn far_detail_is_band_limited_and_mesh_only() {
        let source = LayeredTerrainSource::new(
            TerrainElevationLayer::new(
                Arc::new(ConstantTerrain(10.0)),
                ElevationBounds::new(10.0, 10.0),
            ),
            None,
            None,
        )
        .with_far_detail(FarDetailLayer::new(
            Arc::new(ConstantTerrain(5.0)),
            ElevationBounds::new(5.0, 5.0),
            DetailLodFade::new(5, 8),
            DetailLodFade::new(9, 11),
        ));

        // Physical/Collision surface: never includes the far band.
        assert_eq!(source.height_m(0.0, 0.0), 10.0);
        // Planet-scale and fine LODs: no far band.
        assert_eq!(source.mesh_height_m(0.0, 0.0, 0), 10.0);
        assert_eq!(source.mesh_height_m(0.0, 0.0, 5), 10.0);
        assert_eq!(source.mesh_height_m(0.0, 0.0, 11), 10.0);
        assert_eq!(source.mesh_height_m(0.0, 0.0, 14), 10.0);
        // Band centre: full contribution.
        assert_eq!(source.mesh_height_m(0.0, 0.0, 8), 15.0);
        // Falling edge: strictly between full and none.
        let falling = source.mesh_height_m(0.0, 0.0, 10);
        assert!(falling > 10.0 && falling < 15.0, "falling edge: {falling}");
        // The band is monotonic into and out of the centre.
        assert!(source.mesh_height_m(0.0, 0.0, 6) < source.mesh_height_m(0.0, 0.0, 7));
        assert!(source.mesh_height_m(0.0, 0.0, 10) < source.mesh_height_m(0.0, 0.0, 9));
    }

    /// The far band may raise coastal ground but must not lower it below the
    /// datum and invent a shoreline (the TD-06 rule applies to it too).
    #[test]
    fn far_detail_respects_the_sea_level_guard() {
        let source = LayeredTerrainSource::new(
            TerrainElevationLayer::new(
                Arc::new(ConstantTerrain(0.0)),
                ElevationBounds::new(0.0, 0.0),
            ),
            None,
            None,
        )
        .with_far_detail(
            FarDetailLayer::new(
                Arc::new(ConstantTerrain(-10.0)),
                ElevationBounds::new(-10.0, -10.0),
                DetailLodFade::new(5, 8),
                DetailLodFade::new(9, 11),
            )
            .with_sea_level_guard(4.0, 60.0),
        );
        assert_eq!(source.mesh_height_m(0.0, 0.0, 8), 0.0);

        // Inland ground well above the datum keeps the full band.
        let high_land = LayeredTerrainSource::new(
            TerrainElevationLayer::new(
                Arc::new(ConstantTerrain(100.0)),
                ElevationBounds::new(100.0, 100.0),
            ),
            None,
            None,
        )
        .with_far_detail(
            FarDetailLayer::new(
                Arc::new(ConstantTerrain(5.0)),
                ElevationBounds::new(5.0, 5.0),
                DetailLodFade::new(5, 8),
                DetailLodFade::new(9, 11),
            )
            .with_sea_level_guard(4.0, 60.0),
        );
        assert_eq!(high_land.mesh_height_m(0.0, 0.0, 8), 105.0);
    }

    #[derive(Debug)]
    struct DenseSurface(f64);
    impl TerrainSource for DenseSurface {
        fn height_m(&self, _: f64, _: f64) -> f64 {
            self.0
        }
        fn elevation_bounds_m(&self) -> ElevationBounds {
            ElevationBounds::new(self.0, self.0)
        }
        fn vegetation_density(&self, _: f64, _: f64) -> f64 {
            1.0
        }
    }

    /// VG-01: the treeline is an altitude above the sea datum, and density falls
    /// to zero above it rather than staying at the climate value.
    #[test]
    fn vegetation_density_falls_to_zero_above_the_treeline() {
        let source = |height_m: f64| {
            LayeredTerrainSource::new(
                TerrainElevationLayer::new(
                    Arc::new(DenseSurface(height_m)),
                    ElevationBounds::new(height_m, height_m),
                ),
                None,
                None,
            )
        };
        assert_eq!(source(100.0).vegetation_density(0.0, 0.0), 1.0);
        let mid = source(4_200.0).vegetation_density(0.0, 0.0);
        assert!(mid > 0.0 && mid < 1.0, "the treeline must fade: {mid}");
        assert_eq!(source(5_000.0).vegetation_density(0.0, 0.0), 0.0);
    }

    /// TM-01: a graded pad is flat *geometry*, not a new landform. Surface
    /// classification must keep reading the natural slope, or levelling turns
    /// the footprint into a differently-classified disc.
    #[test]
    fn grading_does_not_reclassify_the_surface_material() {
        use crate::domain::services::terrain_source::{slope_deg_at, surface_appearance};

        let source = graded_source();
        let latitude_deg = 0.0008; // ~89 m north: inside the 90 m flat radius
        assert!(
            source.height_m(latitude_deg, 0.0).abs() < 1e-6,
            "the pad must stay level for geometry and collision"
        );
        let natural_m = SlopedBase.height_m(latitude_deg, 0.0);
        assert!(
            (source.appearance_height_m(latitude_deg, 0.0) - natural_m).abs() < 1e-9,
            "classification must see the natural surface"
        );

        let pad_slope_deg = slope_deg_at(&source, latitude_deg, 0.0);
        let natural_slope_deg = slope_deg_at(&SlopedBase, latitude_deg, 0.0);
        assert!(
            (pad_slope_deg - natural_slope_deg).abs() < 1e-6,
            "the grade must not change the classified slope: {pad_slope_deg} vs {natural_slope_deg}"
        );

        // The slope term of the appearance law is unchanged. Elevation bands
        // (wet shore, snow) still follow the graded surface actually shown, so
        // the comparison holds elevation fixed.
        let pad_material = surface_appearance(natural_m, 0.5, 0.5, pad_slope_deg);
        let natural_material = surface_appearance(natural_m, 0.5, 0.5, natural_slope_deg);
        assert!(
            (pad_material.roughness - natural_material.roughness).abs() < 1e-6,
            "the graded footprint must not read as a different material"
        );
        assert_eq!(pad_material.albedo, natural_material.albedo);
    }

    /// A 45 degree synthetic hillside: the rock blend is materially different
    /// from flat ground, so levelling it would otherwise recolour the pad.
    #[derive(Debug)]
    struct SteepBase;
    impl TerrainSource for SteepBase {
        fn height_m(&self, latitude_deg: f64, _: f64) -> f64 {
            latitude_deg * 111_320.0
        }
        fn elevation_bounds_m(&self) -> ElevationBounds {
            ElevationBounds::new(-10_000_000.0, 10_000_000.0)
        }
    }

    #[test]
    fn a_steep_graded_pad_keeps_the_hillside_material() {
        use crate::domain::services::terrain_source::{slope_deg_at, surface_appearance};

        let source = LayeredTerrainSource::new(
            TerrainElevationLayer::new(
                Arc::new(SteepBase),
                ElevationBounds::new(-10_000_000.0, 10_000_000.0),
            ),
            None,
            None,
        )
        .with_graded_sites(vec![PadFlatZone::new(0.0, 0.0, 6_371_000.0, 90.0, 320.0)]);

        let latitude_deg = 0.0008;
        let graded_slope_deg = 0.0; // what the flattened geometry would classify as
        let classified_slope_deg = slope_deg_at(&source, latitude_deg, 0.0);
        assert!(
            (classified_slope_deg - 45.0).abs() < 1.0,
            "classification must keep the hillside slope, got {classified_slope_deg}"
        );

        let elevation_m = source.height_m(latitude_deg, 0.0);
        let pad = surface_appearance(elevation_m, 0.5, 0.3, classified_slope_deg);
        let flattened = surface_appearance(elevation_m, 0.5, 0.3, graded_slope_deg);
        assert_ne!(
            pad.albedo, flattened.albedo,
            "the control must show that a flat classification would recolour the pad"
        );

        let natural = surface_appearance(
            elevation_m,
            0.5,
            0.3,
            slope_deg_at(&SteepBase, latitude_deg, 0.0),
        );
        assert_eq!(pad.albedo, natural.albedo);
        assert!((pad.roughness - natural.roughness).abs() < 1e-6);
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
                .with_sea_level_guard(4.0, 60.0),
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

    /// The guard window is chosen from the detail layer's own negative bound.
    /// Sweeping the whole worst case proves the composed height cannot cross the
    /// datum anywhere, not merely in the two spot checks above. Earth's detail
    /// layer bottoms out at −36 m, so the guard spans 4 m to 60 m.
    #[test]
    fn sea_level_guard_cannot_lower_ground_below_the_datum() {
        const AMPLITUDE_M: f64 = 36.0;
        for step in 0..=6_000 {
            let base_m = step as f64 * 0.01;
            let source = layered_with(base_m, -AMPLITUDE_M);
            let height_m = source.height_m(0.0, 0.0);
            assert!(
                height_m >= -1e-9,
                "base {base_m} m plus worst-case detail crossed the datum: {height_m} m"
            );
            assert!(
                height_m <= base_m + 1e-9,
                "the guard must never add negative detail's inverse"
            );
            assert!(
                (source.mesh_height_m(0.0, 0.0, 14) - height_m).abs() < 1e-9,
                "mesh and physical paths must agree at base {base_m} m"
            );
        }
        // Above the window the signed detail is restored exactly.
        let fully_signed = layered_with(100.0, -AMPLITUDE_M);
        assert!((fully_signed.height_m(0.0, 0.0) - 64.0).abs() < 1e-9);

        // Below the datum the ground is measured ocean, not fabricated
        // coastline, so the layer keeps its signed relief. Suppressing it there
        // would rewrite bathymetry (and the terrain normals derived from it) for
        // no visible benefit.
        let deep_ocean = layered_with(-500.0, -AMPLITUDE_M);
        assert!(
            (deep_ocean.height_m(0.0, 0.0) + 536.0).abs() < 1e-9,
            "deep ocean must keep its signed detail"
        );

        // The land rule fades out below the datum and the land branch is fully
        // applied at it, so the composed height must be continuous across the
        // waterline rather than jumping by an octave.
        let mut previous = layered_with(-100.0, -AMPLITUDE_M).height_m(0.0, 0.0);
        for step in -9_999..=6_000 {
            let base_m = step as f64 * 0.01;
            let height_m = layered_with(base_m, -AMPLITUDE_M).height_m(0.0, 0.0);
            assert!(
                (height_m - previous).abs() < 0.05,
                "waterline discontinuity at base {base_m} m: {previous} -> {height_m}"
            );
            previous = height_m;
        }
    }

    /// A graded pad at the waterline is still a tower standing in the sea. The
    /// platform clearance floors the pad, never the measured ground around it,
    /// and leaves an already-high site untouched.
    #[test]
    fn coastal_platform_clearance_floors_a_pad_at_the_waterline() {
        let at_datum = LayeredTerrainSource::new(
            TerrainElevationLayer::new(
                Arc::new(ConstantGround(-2.0)),
                ElevationBounds::new(-1_000.0, 10_000.0),
            ),
            None,
            None,
        )
        .with_graded_sites(vec![PadFlatZone::new(0.0, 0.0, 6_371_000.0, 90.0, 320.0)])
        .with_coastal_platform_clearance(3.0);
        assert!((at_datum.height_m(0.0, 0.0) - 3.0).abs() < 1e-9);
        // Outside the blend radius the measured ground is untouched.
        assert!((at_datum.height_m(40.0, 0.0) + 2.0).abs() < 1e-9);

        let already_high = LayeredTerrainSource::new(
            TerrainElevationLayer::new(
                Arc::new(ConstantGround(100.0)),
                ElevationBounds::new(-1_000.0, 10_000.0),
            ),
            None,
            None,
        )
        .with_graded_sites(vec![PadFlatZone::new(0.0, 0.0, 6_371_000.0, 90.0, 320.0)])
        .with_coastal_platform_clearance(3.0);
        assert!((already_high.height_m(0.0, 0.0) - 100.0).abs() < 1e-9);
    }

    /// LF-03: the excavation is cut into the levelled platform and stops short
    /// of the launch point, so the vehicle's resting height is unchanged.
    #[test]
    fn site_trench_cuts_the_pad_but_not_the_launch_point() {
        const RADIUS_M: f64 = 6_371_000.0;
        let source = LayeredTerrainSource::new(
            TerrainElevationLayer::new(
                Arc::new(ConstantGround(100.0)),
                ElevationBounds::new(-1_000.0, 10_000.0),
            ),
            None,
            None,
        )
        .with_graded_sites(vec![PadFlatZone::new(0.0, 0.0, RADIUS_M, 110.0, 280.0)])
        .with_site_trenches(vec![PadTrenchZone::new(
            0.0,
            0.0,
            RADIUS_M,
            (-7.0, 7.0),
            (4.5, 28.0),
            5.0,
            2.5,
        )]);

        let north = |north_m: f64| {
            let direction = DVec3::new(RADIUS_M, north_m, 0.0).normalize();
            (
                direction.y.asin().to_degrees(),
                direction.z.atan2(direction.x).to_degrees(),
            )
        };
        let (lat, lon) = north(0.0);
        assert!(
            (source.height_m(lat, lon) - 100.0).abs() < 1e-6,
            "the launch point must keep the graded platform height"
        );
        let (lat, lon) = north(10.0);
        assert!(
            (source.height_m(lat, lon) - 95.0).abs() < 1e-6,
            "inside the trench the platform must be cut by its full depth"
        );
        let (lat, lon) = north(-10.0);
        assert!(
            (source.height_m(lat, lon) - 100.0).abs() < 1e-6,
            "the pad south of the mouth must not be cut"
        );
        assert!(
            source.elevation_bounds_m().min_m <= 95.0,
            "the declared bounds must include the excavation"
        );
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
