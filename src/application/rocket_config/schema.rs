//! RON schema for vehicle definitions plus the serde helpers used by the
//! shipped config files. Serialization concerns stay out of the domain model.

use super::{LoadedVehicle, RocketConfigError, NEWTONS_PER_KN};
use crate::domain::entities::rocket::{EngineState, RocketEngine, ThrustReference};
use bevy::math::Vec3;
use ron::extensions::Extensions;
use ron::Options;

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct RocketConfigFile {
    pub vehicles: Vec<VehicleDef>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct VehicleDef {
    pub name: String,
    /// Source status for vehicle-level geometry.
    pub basis: DataBasis,
    pub provenance: VehicleProvenance,
    pub diameter_m: f32,
    pub height_m: f32,
    #[serde(default)]
    pub stages: Vec<StageDef>,
    /// Optional identical boosters that burn concurrently with the core stage.
    #[serde(default)]
    pub parallel_boosters: Option<ParallelBoostersDef>,
}

/// Per-stage landing-gear definition. Stages without a `landing_legs` block
/// stay valid and use the point-contact model when independently landed.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct LandingLegsDef {
    pub basis: DataBasis,
    pub count: u32,
    pub base_radius_m: f32,
    pub stroke_m: f32,
    /// Maximum mass the gear can land; defaults to the whole vehicle when
    /// omitted.
    #[serde(default)]
    pub max_landing_mass_kg: Option<f32>,
    /// Radar altitude at which the legs auto-deploy during descent, meters.
    pub deploy_altitude_m: f32,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct StageDef {
    pub name: String,
    pub basis: DataBasis,
    /// Outer cylindrical diameter used by the active-stage force and inertia
    /// model, meters.
    pub diameter_m: f32,
    /// Physical stage length used by the active-stage force and inertia model,
    /// meters.
    pub height_m: f32,
    pub dry_mass_kg: f32,
    pub propellant_mass_kg: f32,
    /// Propellant held back for a first-stage recovery burn sequence. Stages
    /// without a reserve remain expendable debris after separation.
    #[serde(default)]
    pub recovery_propellant_reserve_kg: Option<f32>,
    /// Optional deployable gear installed on this serial stage only.
    #[serde(default)]
    pub landing_legs: Option<LandingLegsDef>,
    /// Payload fairing physically attached to this serial stage. The current
    /// vehicle architecture permits it only on the final serial stage.
    #[serde(default)]
    pub fairing: Option<FairingDef>,
    pub engines: EngineGroupDef,
}

/// Parallel booster hardware. Attachment positions are booster cylinder origins
/// in the full vehicle stack frame, while engine stations stay stage-local.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct ParallelBoostersDef {
    pub basis: DataBasis,
    pub count: u32,
    pub stage: StageDef,
    pub attachment_positions: Vec<[f32; 3]>,
}

/// One source-status declaration covers an explicit set of engine values.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct EngineGroupDef {
    pub basis: DataBasis,
    pub values: Vec<EngineDef>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct EngineDef {
    /// Stage-local position, meters, from the stage cylinder geometric center
    /// (+Y longitudinal, nose-up).
    pub position: [f32; 3],
    /// Body-frame thrust unit axis.
    pub thrust_axis: [f32; 3],
    pub isp_sl: f32,
    pub isp_vac: f32,
    pub gimbal_range_deg: f32,
    /// Full-throttle rated thrust, in newtons, at `thrust_reference`.
    pub rated_thrust_n: f32,
    /// The required pressure endpoint for `rated_thrust_n`.
    pub thrust_reference: ThrustReferenceDef,
    #[serde(default)]
    pub throttle_min: f32,
    #[serde(default = "default_throttle_max")]
    pub throttle_max: f32,
    /// Required catalogued lifetime start budget. It is intentionally explicit:
    /// unknown operational limits must not silently become unlimited restarts.
    pub max_ignitions: u32,
}

/// RON representation of the pressure endpoint for an engine's rated thrust.
/// `SeaLevel` means standard sea-level pressure; `Vacuum` means zero pressure.
#[derive(Debug, Clone, Copy, serde::Deserialize, serde::Serialize)]
pub enum ThrustReferenceDef {
    SeaLevel,
    Vacuum,
}

impl From<ThrustReferenceDef> for ThrustReference {
    fn from(value: ThrustReferenceDef) -> Self {
        match value {
            ThrustReferenceDef::SeaLevel => Self::SeaLevel,
            ThrustReferenceDef::Vacuum => Self::Vacuum,
        }
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct FairingDef {
    pub basis: DataBasis,
    pub dry_mass_kg: f32,
}

/// Declares whether a numerical group is pinned to a verified source byte.
#[derive(Debug, Clone, Copy, serde::Deserialize, serde::Serialize)]
pub enum DataBasis {
    SourceVerified,
    Representative,
}

/// Per-vehicle source record. Representative definitions may retain partial
/// source details while their exact source byte is not pinned.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct VehicleProvenance {
    #[serde(default)]
    pub manufacturer: Option<String>,
    #[serde(default)]
    pub primary_source: Option<PrimarySource>,
    #[serde(default)]
    pub representative_rationale: Option<String>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct PrimarySource {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub publication_date: Option<String>,
    #[serde(default)]
    pub source_url: Option<String>,
    #[serde(default)]
    pub sha256: Option<String>,
}

pub(super) fn default_throttle_max() -> f32 {
    1.0
}

pub(super) fn is_iso_publication_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if !(bytes.len() == 7 || bytes.len() == 10)
        || bytes.get(4) != Some(&b'-')
        || (bytes.len() == 10 && bytes.get(7) != Some(&b'-'))
        || !bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
    {
        return false;
    }
    let year = value[..4].parse::<u16>().ok();
    let month = value[5..7].parse::<u8>().ok();
    let Some((year, month)) = year.zip(month) else {
        return false;
    };
    if year == 0 || !(1..=12).contains(&month) {
        return false;
    }
    if bytes.len() == 7 {
        return true;
    }
    let Some(day) = value[8..10].parse::<u8>().ok() else {
        return false;
    };
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        _ => return false,
    };
    (1..=max_day).contains(&day)
}

pub(super) fn is_https_url(value: &str) -> bool {
    value
        .strip_prefix("https://")
        .is_some_and(|authority_and_path| {
            authority_and_path
                .split('/')
                .next()
                .is_some_and(|authority| !authority.is_empty())
                && !value.contains(char::is_whitespace)
        })
}

impl EngineDef {
    /// Convert a validated engine definition into its fresh domain state.
    pub fn to_domain(&self) -> RocketEngine {
        RocketEngine {
            position_m: Vec3::from_array(self.position),
            thrust_axis: Vec3::from_array(self.thrust_axis).normalize_or_zero(),
            isp_sea_level: self.isp_sl,
            isp_vacuum: self.isp_vac,
            gimbal_range_deg: self.gimbal_range_deg,
            rated_thrust_kn: self.rated_thrust_n / NEWTONS_PER_KN,
            thrust_reference: self.thrust_reference.into(),
            throttle_min: self.throttle_min,
            throttle_max: self.throttle_max,
            max_ignitions: self.max_ignitions,
            ignition_count: 0,
            state: EngineState::Off,
        }
    }
}

impl RocketConfigFile {
    /// RON parse options: IMPLICIT_SOME lets final stage blocks write
    /// `fairing: ( ... )` instead of the noisier `Some(( ... ))`.
    pub(crate) fn ron_options() -> Options {
        Options::default().with_default_extension(Extensions::IMPLICIT_SOME)
    }

    /// Parse one RON vehicle-definition document into loaded vehicles,
    /// validating each definition before conversion. Exposed for tests and
    /// future loaders (network, embedded assets); selection keys come from
    /// config file stems ([`RocketCatalog::from_dir`]), not display names.
    pub fn parse(text: &str) -> Result<Vec<LoadedVehicle>, RocketConfigError> {
        let file: RocketConfigFile = Self::ron_options()
            .from_str(text)
            .map_err(RocketConfigError::Parse)?;
        let mut out = Vec::with_capacity(file.vehicles.len());
        for vehicle in &file.vehicles {
            vehicle.validate()?;
            out.push(vehicle.to_domain());
        }
        Ok(out)
    }
}
