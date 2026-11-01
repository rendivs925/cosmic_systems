//! Developer terrain diagnostic render modes.
//!
//! A diagnostic mode renders exactly one authoritative terrain input as a
//! deterministic false colour, so a visible defect can be attributed to a term
//! instead of guessed at. Nothing here feeds the simulation: the modes are
//! selected by a `u32` in the terrain material uniform, and the fragment shader
//! replaces only its final colour when a mode is active.
//!
//! This module is deliberately free of Bevy ECS types. The ECS wiring lives in
//! `infrastructure::bevy_adapters::terrain::diagnostics`; the shader constants
//! live in `assets/shaders/terrain_surface.wgsl` and are pinned to [`TerrainDiagnostic::code`]
//! by a test in `terrain::render`.
//!
//! ## What each mode proves
//!
//! | mode | proves |
//! | --- | --- |
//! | [`TerrainDiagnostic::Albedo`] | whether the geographic base colour is sampled correctly, and whether a tone step already exists in the source albedo |
//! | [`TerrainDiagnostic::WorldNormals`] | whether vertex normals are continuous, correctly oriented, and agree across cube faces |
//! | [`TerrainDiagnostic::Uv0`] / [`TerrainDiagnostic::Uv1`] | whether geographic and patch-local texture coordinates are continuous |
//! | [`TerrainDiagnostic::Height`] | whether the rendered surface is the authoritative height field |
//! | [`TerrainDiagnostic::LandSea`] | whether fragments sit above or below the sea-level datum |
//! | [`TerrainDiagnostic::SurfaceClass`] | which ground layer the weight map assigns, and where no classification exists at all |
//! | [`TerrainDiagnostic::LodLevel`] | which LOD each patch is drawn at, and where neighbours disagree |
//! | [`TerrainDiagnostic::PatchBoundary`] / [`TerrainDiagnostic::PatchIdentity`] | where patch edges fall, and which patch owns a pixel |
//! | [`TerrainDiagnostic::Inputs`] | which per-patch material inputs are present, which is the fastest way to see a fallback patch beside a full one |
//! | [`TerrainDiagnostic::Occlusion`] | whether the per-patch baked self-shadow and sky-occlusion maps are present and continuous |
//! | [`TerrainDiagnostic::Roughness`] / [`TerrainDiagnostic::Metallic`] / [`TerrainDiagnostic::LayerBlend`] / [`TerrainDiagnostic::DetailWeight`] | the material inputs the lighting path consumes |
//! | [`TerrainDiagnostic::MorphOffset`] | the per-vertex LOD morph displacement, which exposes transitions that would pop |
//!
//! The water cap is a separate material with its own shader, so it has its own
//! views in the same namespace:
//!
//! | mode | proves |
//! | --- | --- |
//! | [`TerrainDiagnostic::WaterDepth`] | whether the normalized depth attribute — the input of the colour and opacity ramps — steps at a patch edge |
//! | [`TerrainDiagnostic::WaterUv1`] | where the water pass's own patches fall, independent of the terrain pass |
//! | [`TerrainDiagnostic::WaterBody`] | the shallow/deep ramp, crest scatter and foam before any lighting |
//! | [`TerrainDiagnostic::WaterShadow`] | whether the baked landscape shadow or the cloud deck is shading the sea unevenly |
//! | [`TerrainDiagnostic::WaterReflection`] | how much of the sea's tone comes from the sky/deck mirror term |
//!
//! Both families share one selector and one code namespace, but each material
//! receives only the codes its own shader decodes; the other material is given
//! production. That keeps an unknown code a visible no-data error.
//!
//! Modes that cannot be answered from data the shader actually has are not
//! listed here. In particular there is no "terrain source identity" mode: the
//! fragment shader has no access to the terrain authority, so inventing one
//! would produce a confident picture of something nobody measured.

/// Environment variable selecting the diagnostic mode by name or numeric code.
pub const DIAGNOSTIC_MODE_ENV: &str = "COSMIC_SYSTEMS_DIAGNOSTIC_MODE";

/// The terrain terms the diagnostic renderer can display.
///
/// The discriminants are the shader's mode codes. They are part of the
/// presentation contract, so they must never be reordered; append instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
#[repr(u32)]
pub enum TerrainDiagnostic {
    /// Production rendering. The only mode that exists outside developer use.
    #[default]
    Normal = 0,
    /// Geographic base colour (global overview mixed with the regional imagery
    /// tile by `imagery_weight`), with no lighting and no atmosphere.
    Albedo = 1,
    /// World-space geometric normal encoded as RGB (`n * 0.5 + 0.5`).
    WorldNormals = 2,
    /// Geographic equirectangular UV0 with a 16-division grid.
    Uv0 = 3,
    /// Patch-local UV1 with an 8-division grid.
    Uv1 = 4,
    /// Height above the sea-level datum, plotted on a documented metre ramp.
    Height = 5,
    /// Land/sea classification derived from the fragment's radius against the
    /// sea-level datum radius.
    LandSea = 6,
    /// Dominant ground layer from the per-patch weight map; magenta where no
    /// classification data exists.
    SurfaceClass = 7,
    /// Patch LOD level as a hue ramp, with patch boundaries drawn.
    LodLevel = 8,
    /// Patch boundaries over neutral grey.
    PatchBoundary = 9,
    /// A stable colour per patch from its face, level and tile coordinates.
    PatchIdentity = 10,
    /// Per-patch availability of the material inputs, as an additive colour.
    Inputs = 11,
    /// Baked landscape occlusion: red is self-shadow, green is sky occlusion.
    Occlusion = 12,
    /// Final perceptual roughness as grey.
    Roughness = 13,
    /// The patch material's metallic value as grey.
    Metallic = 14,
    /// Whether the layered ground material is active for this patch.
    LayerBlend = 15,
    /// Per-vertex LOD morph displacement in metres on a documented ramp.
    MorphOffset = 16,
    /// Source-derived local detail weight after the view-distance fade.
    DetailWeight = 17,
    /// Sea-cap normalized depth from the water vertex attribute, on a grey ramp
    /// with contours.
    WaterDepth = 18,
    /// The water pass's own patch-local UV1 with a grid, so the cap layout can be
    /// read independently of the terrain pass.
    WaterUv1 = 19,
    /// The water body colour before lighting: the shallow/deep ramp, crest
    /// scatter and foam.
    WaterBody = 20,
    /// The two visibility terms the sea is shaded by: baked landscape shadow
    /// (red), cloud shadow (green) and their product (blue).
    WaterShadow = 21,
    /// The sky/deck mirror contribution added on top of the lit sea.
    WaterReflection = 22,
    /// Final water alpha, which decides how much seafloor is visible through it.
    WaterOpacity = 23,
}

impl TerrainDiagnostic {
    /// Every mode, in shader-code order.
    pub const ALL: [Self; 24] = [
        Self::Normal,
        Self::Albedo,
        Self::WorldNormals,
        Self::Uv0,
        Self::Uv1,
        Self::Height,
        Self::LandSea,
        Self::SurfaceClass,
        Self::LodLevel,
        Self::PatchBoundary,
        Self::PatchIdentity,
        Self::Inputs,
        Self::Occlusion,
        Self::Roughness,
        Self::Metallic,
        Self::LayerBlend,
        Self::MorphOffset,
        Self::DetailWeight,
        Self::WaterDepth,
        Self::WaterUv1,
        Self::WaterBody,
        Self::WaterShadow,
        Self::WaterReflection,
        Self::WaterOpacity,
    ];

    /// The shader mode code.
    pub const fn code(self) -> u32 {
        self as u32
    }

    /// Whether this mode replaces production rendering.
    pub const fn is_active(self) -> bool {
        !matches!(self, Self::Normal)
    }

    /// Whether this mode is decoded by the terrain material shader.
    ///
    /// The two shaders have separate code sets in one namespace: each material
    /// receives the code that belongs to its own family and production (0)
    /// otherwise, so an unknown code can stay a visible no-data error instead of
    /// silently rendering production colours.
    pub const fn is_terrain(self) -> bool {
        matches!(
            self,
            Self::Albedo
                | Self::WorldNormals
                | Self::Uv0
                | Self::Uv1
                | Self::Height
                | Self::LandSea
                | Self::SurfaceClass
                | Self::LodLevel
                | Self::PatchBoundary
                | Self::PatchIdentity
                | Self::Inputs
                | Self::Occlusion
                | Self::Roughness
                | Self::Metallic
                | Self::LayerBlend
                | Self::MorphOffset
                | Self::DetailWeight
        )
    }

    /// Whether this mode is decoded by the water material shader.
    pub const fn is_water(self) -> bool {
        matches!(
            self,
            Self::WaterDepth
                | Self::WaterUv1
                | Self::WaterBody
                | Self::WaterShadow
                | Self::WaterReflection
                | Self::WaterOpacity
        )
    }

    /// The stable command-line / environment name.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Albedo => "albedo",
            Self::WorldNormals => "world-normals",
            Self::Uv0 => "uv0",
            Self::Uv1 => "uv1",
            Self::Height => "height",
            Self::LandSea => "land-sea",
            Self::SurfaceClass => "surface-class",
            Self::LodLevel => "lod",
            Self::PatchBoundary => "patch-boundary",
            Self::PatchIdentity => "patch-identity",
            Self::Inputs => "inputs",
            Self::Occlusion => "occlusion",
            Self::Roughness => "roughness",
            Self::Metallic => "metallic",
            Self::LayerBlend => "layer-blend",
            Self::MorphOffset => "morph-offset",
            Self::DetailWeight => "detail-weight",
            Self::WaterDepth => "water-depth",
            Self::WaterUv1 => "water-uv1",
            Self::WaterBody => "water-body",
            Self::WaterShadow => "water-shadow",
            Self::WaterReflection => "water-reflection",
            Self::WaterOpacity => "water-opacity",
        }
    }

    /// The constant that carries this mode's code in
    /// `assets/shaders/terrain_surface.wgsl`. A test pins the two together, so a
    /// renamed or reordered shader constant cannot drift silently.
    pub const fn shader_constant(self) -> &'static str {
        match self {
            Self::Normal => "DIAG_NORMAL",
            Self::Albedo => "DIAG_ALBEDO",
            Self::WorldNormals => "DIAG_WORLD_NORMALS",
            Self::Uv0 => "DIAG_UV0",
            Self::Uv1 => "DIAG_UV1",
            Self::Height => "DIAG_HEIGHT",
            Self::LandSea => "DIAG_LAND_SEA",
            Self::SurfaceClass => "DIAG_SURFACE_CLASS",
            Self::LodLevel => "DIAG_LOD",
            Self::PatchBoundary => "DIAG_PATCH_BOUNDARY",
            Self::PatchIdentity => "DIAG_PATCH_IDENTITY",
            Self::Inputs => "DIAG_INPUTS",
            Self::Occlusion => "DIAG_OCCLUSION",
            Self::Roughness => "DIAG_ROUGHNESS",
            Self::Metallic => "DIAG_METALLIC",
            Self::LayerBlend => "DIAG_LAYER_BLEND",
            Self::MorphOffset => "DIAG_MORPH_OFFSET",
            Self::DetailWeight => "DIAG_DETAIL_WEIGHT",
            Self::WaterDepth => "DIAG_WATER_DEPTH",
            Self::WaterUv1 => "DIAG_WATER_UV1",
            Self::WaterBody => "DIAG_WATER_BODY",
            Self::WaterShadow => "DIAG_WATER_SHADOW",
            Self::WaterReflection => "DIAG_WATER_REFLECTION",
            Self::WaterOpacity => "DIAG_WATER_OPACITY",
        }
    }

    /// Look a mode up by its shader code.
    pub fn from_code(code: u32) -> Option<Self> {
        Self::ALL.into_iter().find(|mode| mode.code() == code)
    }

    /// Parse a mode from its name or numeric code.
    ///
    /// Hyphens and underscores are interchangeable and the comparison is
    /// case-insensitive, because a value typed into an environment variable
    /// should not depend on which separator the caller remembered.
    pub fn parse(value: &str) -> Option<Self> {
        let normalized = value.trim().to_ascii_lowercase().replace('_', "-");
        if normalized.is_empty() {
            return None;
        }
        if let Ok(code) = normalized.parse::<u32>() {
            return Self::from_code(code);
        }
        Self::ALL.into_iter().find(|mode| mode.name() == normalized)
    }

    /// The next mode, wrapping to [`Self::Normal`] after the last one.
    ///
    /// Starting at `Normal` steps into the first diagnostic so a single
    /// developer keypress always produces something different on screen.
    pub fn next(self) -> Self {
        let index = Self::ALL
            .iter()
            .position(|mode| *mode == self)
            .unwrap_or_default();
        Self::ALL[(index + 1) % Self::ALL.len()]
    }

    /// The previous mode, wrapping to the last diagnostic from `Normal`.
    pub fn previous(self) -> Self {
        let index = Self::ALL
            .iter()
            .position(|mode| *mode == self)
            .unwrap_or_default();
        Self::ALL[(index + Self::ALL.len() - 1) % Self::ALL.len()]
    }

    /// Parse the environment, defaulting to production rendering.
    ///
    /// A malformed value is reported and ignored: a developer typo must not cost
    /// a capture run, and silently rendering diagnostic colours would be worse
    /// than rendering production.
    pub fn from_env() -> Self {
        Self::from_value(std::env::var(DIAGNOSTIC_MODE_ENV).ok().as_deref())
    }

    /// Parse an optional raw value, defaulting to production rendering.
    pub fn from_value(value: Option<&str>) -> Self {
        let Some(value) = value else {
            return Self::default();
        };
        match Self::parse(value) {
            Some(mode) => mode,
            None => {
                bevy::log::warn!(
                    value,
                    "{DIAGNOSTIC_MODE_ENV} is not a known terrain diagnostic mode; \
                     rendering normally. Values: {}",
                    Self::ALL.map(Self::name).join(", ")
                );
                Self::default()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_rendering_is_the_default() {
        assert_eq!(TerrainDiagnostic::default(), TerrainDiagnostic::Normal);
        assert_eq!(
            TerrainDiagnostic::from_value(None),
            TerrainDiagnostic::Normal
        );
        assert!(!TerrainDiagnostic::default().is_active());
    }

    #[test]
    fn shader_codes_are_unique_and_contiguous() {
        for (index, mode) in TerrainDiagnostic::ALL.into_iter().enumerate() {
            assert_eq!(mode.code(), index as u32, "{mode:?} is out of code order");
            assert_eq!(TerrainDiagnostic::from_code(mode.code()), Some(mode));
        }
        assert_eq!(TerrainDiagnostic::from_code(999), None);
    }

    #[test]
    fn names_round_trip_and_are_unique() {
        let mut names: Vec<&str> = Vec::new();
        for mode in TerrainDiagnostic::ALL {
            let name = mode.name();
            assert!(!names.contains(&name), "duplicate diagnostic name {name}");
            names.push(name);
            assert_eq!(TerrainDiagnostic::parse(name), Some(mode));
            assert_eq!(
                TerrainDiagnostic::parse(&name.replace('-', "_").to_uppercase()),
                Some(mode),
                "{name} must parse regardless of case and separator"
            );
        }
    }

    #[test]
    fn numeric_codes_parse_and_unknown_values_fall_back_to_production() {
        assert_eq!(
            TerrainDiagnostic::parse("5"),
            Some(TerrainDiagnostic::Height)
        );
        assert_eq!(TerrainDiagnostic::parse("24"), None);
        assert_eq!(TerrainDiagnostic::parse("heightmap"), None);
        assert_eq!(TerrainDiagnostic::parse("   "), None);
        assert_eq!(
            TerrainDiagnostic::from_value(Some("nonsense")),
            TerrainDiagnostic::Normal
        );
    }

    #[test]
    fn shader_constants_are_named_for_every_mode() {
        let mut constants: Vec<&str> = Vec::new();
        for mode in TerrainDiagnostic::ALL {
            let constant = mode.shader_constant();
            assert!(
                constant.starts_with("DIAG_"),
                "{constant} must live in the shader's diagnostic namespace"
            );
            assert!(
                !constants.contains(&constant),
                "duplicate constant {constant}"
            );
            constants.push(constant);
        }
    }

    #[test]
    fn cycling_visits_every_mode_exactly_once() {
        let mut mode = TerrainDiagnostic::Normal;
        // From production, every step except the last lands on a diagnostic.
        for _ in 0..TerrainDiagnostic::ALL.len() - 1 {
            mode = mode.next();
            assert!(mode.is_active(), "cycling must leave production rendering");
        }
        assert_eq!(mode.next(), TerrainDiagnostic::Normal);
        assert_eq!(
            TerrainDiagnostic::Normal.previous(),
            *TerrainDiagnostic::ALL.last().expect("mode list")
        );
        for mode in TerrainDiagnostic::ALL {
            assert_eq!(mode.next().previous(), mode);
        }
    }
}
