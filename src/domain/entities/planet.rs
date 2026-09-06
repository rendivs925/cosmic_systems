#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BodyClass {
    Star,
    Terrestrial,
    GasGiant,
    IceGiant,
    Dwarf,
    Moon,
}

/// Whether a celestial body has a physical solid surface that may receive a
/// terrain authority and terrain-contact physics. This is independent of its
/// visual body class and ocean state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceCapability {
    NoSolidSurface,
    SolidSurface,
}

impl SurfaceCapability {
    pub const fn supports_terrain(self) -> bool {
        matches!(self, Self::SolidSurface)
    }
}

/// Identifies a configured, versioned terrain authority for a solid body.
/// Dataset selection stays catalog-driven; rendering and collision still share
/// the single `TerrainSource` built for the selected authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerrainAuthorityId {
    Earth,
}

/// Celestial body entity representing planets, moons, and stars
#[derive(Clone, Debug)]
pub struct Planet {
    pub name: String,
    pub radius_km: f32,
    pub mass_kg: f64,
    pub body_class: BodyClass,
    pub surface_capability: SurfaceCapability,
    pub terrain_authority: Option<TerrainAuthorityId>,
    pub orbital_distance_au: f32, // Average distance from Sun (or parent planet) in AU
    pub orbital_period_days: f32,
    pub rotation_period_hours: f32,
    pub axial_tilt_deg: f32,
    pub parent_entity: Option<String>, // Name of parent body (None for Sun, planet name for moons)
    /// Explicit ocean mask (Phase 15): true only for bodies with open liquid
    /// seas at mean sea level. Single authority for water inference —
    /// collision/telemetry read this instead of guessing from body names.
    pub has_ocean: bool,
}

#[derive(Debug, Default)]
pub struct PlanetBuilder {
    name: Option<String>,
    radius_km: Option<f32>,
    mass_kg: Option<f64>,
    body_class: Option<BodyClass>,
    surface_capability: Option<SurfaceCapability>,
    terrain_authority: Option<Option<TerrainAuthorityId>>,
    orbital_distance_au: Option<f32>,
    orbital_period_days: Option<f32>,
    rotation_period_hours: Option<f32>,
    axial_tilt_deg: Option<f32>,
    parent_entity: Option<Option<String>>,
    has_ocean: Option<bool>,
}

impl PlanetBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn name(mut self, name: String) -> Self {
        self.name = Some(name);
        self
    }

    pub fn radius_km(mut self, radius_km: f32) -> Self {
        self.radius_km = Some(radius_km);
        self
    }

    pub fn mass_kg(mut self, mass_kg: f64) -> Self {
        self.mass_kg = Some(mass_kg);
        self
    }

    pub fn body_class(mut self, body_class: BodyClass) -> Self {
        self.body_class = Some(body_class);
        self
    }

    pub fn surface_capability(mut self, surface_capability: SurfaceCapability) -> Self {
        self.surface_capability = Some(surface_capability);
        self
    }

    pub fn terrain_authority(mut self, terrain_authority: Option<TerrainAuthorityId>) -> Self {
        self.terrain_authority = Some(terrain_authority);
        self
    }

    pub fn orbital_distance_au(mut self, orbital_distance_au: f32) -> Self {
        self.orbital_distance_au = Some(orbital_distance_au);
        self
    }

    pub fn orbital_period_days(mut self, orbital_period_days: f32) -> Self {
        self.orbital_period_days = Some(orbital_period_days);
        self
    }

    pub fn rotation_period_hours(mut self, rotation_period_hours: f32) -> Self {
        self.rotation_period_hours = Some(rotation_period_hours);
        self
    }

    pub fn axial_tilt_deg(mut self, axial_tilt_deg: f32) -> Self {
        self.axial_tilt_deg = Some(axial_tilt_deg);
        self
    }

    pub fn parent_entity(mut self, parent_entity: Option<String>) -> Self {
        self.parent_entity = Some(parent_entity);
        self
    }

    pub fn has_ocean(mut self, has_ocean: bool) -> Self {
        self.has_ocean = Some(has_ocean);
        self
    }

    pub fn build(self) -> Planet {
        Planet {
            name: self.name.expect("name is required"),
            radius_km: self.radius_km.expect("radius_km is required"),
            mass_kg: self.mass_kg.expect("mass_kg is required"),
            body_class: self.body_class.expect("body_class is required"),
            surface_capability: self
                .surface_capability
                .expect("surface_capability is required"),
            terrain_authority: self
                .terrain_authority
                .expect("terrain_authority is required"),
            orbital_distance_au: self
                .orbital_distance_au
                .expect("orbital_distance_au is required"),
            orbital_period_days: self
                .orbital_period_days
                .expect("orbital_period_days is required"),
            rotation_period_hours: self
                .rotation_period_hours
                .expect("rotation_period_hours is required"),
            axial_tilt_deg: self.axial_tilt_deg.expect("axial_tilt_deg is required"),
            parent_entity: self.parent_entity.expect("parent_entity is required"),
            has_ocean: self.has_ocean.expect("has_ocean is required"),
        }
    }
}
