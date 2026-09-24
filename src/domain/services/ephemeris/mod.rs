//! Offline, kernel-backed solar-system body states.
//!
//! This module is the only domain boundary allowed to load and evaluate the
//! curated NAIF SPICE/JPL DE kernels. It exposes geometric f64 SI states in the
//! J2000 frame; rendering and local-flight conversions remain elsewhere.
//!
//! The implementation is split into cohesive submodules: [`manifest`] (the
//! manifest schema, provenance, and validation), [`authority`] (the immutable
//! [`authority::SpiceEphemeris`] evaluator), and [`error`]. This module owns the
//! state contract itself: [`NaifBodyId`], [`TdbEpoch`], and [`BodyState`].

use crate::domain::math::DVec3;
use anise::frames::Frame;
use anise::time::Epoch;

pub const J2000_JULIAN_DATE_TDB: f64 = 2_451_545.0;
const KILOMETERS_TO_METERS: f64 = 1_000.0;
const KILOMETERS_CUBED_TO_METERS_CUBED: f64 =
    KILOMETERS_TO_METERS * KILOMETERS_TO_METERS * KILOMETERS_TO_METERS;

mod authority;
mod error;
mod manifest;

pub use authority::SpiceEphemeris;
pub use error::EphemerisError;
pub use manifest::{
    load_manifest, validate_manifest, KernelCoverage, KernelFile, KernelKind, KernelManifest,
    KernelProvenance, ScientificDatasetAvailability, ScientificDatasetCoverage,
    ScientificDatasetFrame, ScientificDatasetRole, ScientificDatasetStatus,
    ScientificDatasetTimeScale, ValidatedKernel,
};

/// A NAIF celestial-body identifier used by the ephemeris contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct NaifBodyId(i32);

impl NaifBodyId {
    pub const SOLAR_SYSTEM_BARYCENTER: Self = Self(0);
    pub const SUN: Self = Self(10);
    pub const MERCURY_BARYCENTER: Self = Self(1);
    pub const VENUS_BARYCENTER: Self = Self(2);
    pub const EARTH_MOON_BARYCENTER: Self = Self(3);
    pub const MARS_BARYCENTER: Self = Self(4);
    pub const JUPITER_BARYCENTER: Self = Self(5);
    pub const SATURN_BARYCENTER: Self = Self(6);
    pub const URANUS_BARYCENTER: Self = Self(7);
    pub const NEPTUNE_BARYCENTER: Self = Self(8);
    pub const PLUTO_BARYCENTER: Self = Self(9);
    pub const EARTH: Self = Self(399);
    pub const MOON: Self = Self(301);

    /// Catalog bodies whose translations are available from the provisioned
    /// DE440s authority. Other catalog moons remain explicit approximations
    /// until their required satellite kernels are added to the manifest.
    pub const KERNEL_BACKED_CATALOG_BODIES: [(&str, Self); 10] = [
        ("Sun", Self::SUN),
        ("Mercury", Self::MERCURY_BARYCENTER),
        ("Venus", Self::VENUS_BARYCENTER),
        ("Earth", Self::EARTH),
        ("Mars", Self::MARS_BARYCENTER),
        ("Jupiter", Self::JUPITER_BARYCENTER),
        ("Saturn", Self::SATURN_BARYCENTER),
        ("Uranus", Self::URANUS_BARYCENTER),
        ("Neptune", Self::NEPTUNE_BARYCENTER),
        ("Moon", Self::MOON),
    ];

    /// Iterate the translation targets for every kernel-backed catalog body.
    pub fn kernel_backed_catalog_targets() -> impl Iterator<Item = Self> {
        Self::KERNEL_BACKED_CATALOG_BODIES
            .into_iter()
            .map(|(_, target)| target)
    }

    /// Maps translation targets to the physical body's IAU orientation target.
    /// Major-planet translations use barycenters while PCK orientation uses the
    /// body's conventional NAIF identifier.
    pub const fn orientation_target(self) -> Option<Self> {
        let target = match self {
            Self::MERCURY_BARYCENTER => Self(199),
            Self::VENUS_BARYCENTER => Self(299),
            Self::MARS_BARYCENTER => Self(499),
            Self::JUPITER_BARYCENTER => Self(599),
            Self::SATURN_BARYCENTER => Self(699),
            Self::URANUS_BARYCENTER => Self(799),
            Self::NEPTUNE_BARYCENTER => Self(899),
            Self::PLUTO_BARYCENTER => Self(999),
            Self::SUN | Self::EARTH | Self::MOON => self,
            _ => return None,
        };
        Some(target)
    }

    /// The physical body's NAIF identifier used by `gm_de440.tpc`. Translation
    /// barycenters intentionally map to their corresponding physical bodies.
    pub const fn gravitational_parameter_target(self) -> Option<Self> {
        self.orientation_target()
    }

    /// Map the current celestial catalog's kernel-backed bodies to their NAIF
    /// identifiers. Unmapped catalog moons remain presentation-only until
    /// their required kernel coverage is added to the curated manifest.
    pub fn for_catalog_name(name: &str) -> Option<Self> {
        Self::KERNEL_BACKED_CATALOG_BODIES
            .iter()
            .find_map(|(catalog_name, target)| (*catalog_name == name).then_some(*target))
    }

    pub const fn new(value: i32) -> Self {
        Self(value)
    }

    pub const fn value(self) -> i32 {
        self.0
    }

    fn j2000_frame(self) -> Frame {
        Frame::from_ephem_j2000(self.0)
    }
}

/// A TDB Julian date used to evaluate an ephemeris state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TdbEpoch {
    julian_date_tdb: f64,
}

impl TdbEpoch {
    pub fn from_julian_date(julian_date_tdb: f64) -> Result<Self, EphemerisError> {
        if !julian_date_tdb.is_finite() {
            return Err(EphemerisError::InvalidEpoch(julian_date_tdb));
        }
        Ok(Self { julian_date_tdb })
    }

    pub fn from_seconds_since_j2000(seconds: f64) -> Result<Self, EphemerisError> {
        if !seconds.is_finite() {
            return Err(EphemerisError::InvalidEpoch(seconds));
        }
        Self::from_julian_date(J2000_JULIAN_DATE_TDB + seconds / 86_400.0)
    }

    pub const fn j2000() -> Self {
        Self {
            julian_date_tdb: J2000_JULIAN_DATE_TDB,
        }
    }

    pub const fn julian_date(self) -> f64 {
        self.julian_date_tdb
    }

    pub fn seconds_since_j2000(self) -> f64 {
        (self.julian_date_tdb - J2000_JULIAN_DATE_TDB) * 86_400.0
    }

    fn anise_epoch(self) -> Epoch {
        // `from_jde_tdb` preserves the JD TDB convention at this boundary;
        // `from_tdb_seconds` has a distinct documented zero-time convention.
        Epoch::from_jde_tdb(self.julian_date_tdb)
    }
}

/// A geometric ICRF/J2000 state in SI units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyState {
    pub target: NaifBodyId,
    pub center: NaifBodyId,
    pub epoch: TdbEpoch,
    pub position_m: DVec3,
    pub velocity_mps: DVec3,
}

impl BodyState {
    fn from_anise(
        target: NaifBodyId,
        center: NaifBodyId,
        epoch: TdbEpoch,
        position_km: [f64; 3],
        velocity_km_s: [f64; 3],
    ) -> Result<Self, EphemerisError> {
        let position_m = DVec3::from_array(position_km) * KILOMETERS_TO_METERS;
        let velocity_mps = DVec3::from_array(velocity_km_s) * KILOMETERS_TO_METERS;
        if !position_m.is_finite() || !velocity_mps.is_finite() {
            return Err(EphemerisError::NonFiniteState { target, center });
        }
        Ok(Self {
            target,
            center,
            epoch,
            position_m,
            velocity_mps,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::body_orientation::{
        OrientationBodyFixedFrame, OrientationDataSource, OrientationInertialFrame,
    };
    use crate::domain::services::simulation_epoch::LeapSecondTable;
    use std::fs;

    fn fixture_manifest(kernel_root: &str, sha256: &str) -> String {
        format!(
            r#"(
                id: "test-kernel-set",
                kernel_root: "{kernel_root}",
                coverage: (
                    start_julian_date_tdb: 2451545.0,
                    end_julian_date_tdb: 2451546.0,
                ),
                kernels: [(
                    file_name: "fixture.bsp",
                    role: Translation,
                    kind: Spk,
                    sha256: "{sha256}",
                    expected_size_bytes: 3,
                    source_url: "https://example.invalid/fixture.bsp",
                    coverage: Some((
                        start_julian_date: 2451545.0,
                        end_julian_date: 2451546.0,
                    )),
                    frame: SsbIcrfJ2000,
                    time_scale: Tdb,
                )],
            )"#
        )
    }

    fn fixture_root() -> std::path::PathBuf {
        let unique_suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "cosmic-ephemeris-{}-{unique_suffix}",
            std::process::id()
        ))
    }

    #[test]
    fn tdb_epoch_preserves_j2000_julian_date() {
        let epoch = TdbEpoch::from_seconds_since_j2000(86_400.0).unwrap();

        assert_eq!(TdbEpoch::j2000().julian_date(), J2000_JULIAN_DATE_TDB);
        assert_eq!(epoch.julian_date(), J2000_JULIAN_DATE_TDB + 1.0);
        assert_eq!(epoch.seconds_since_j2000(), 86_400.0);
    }

    #[test]
    fn body_state_converts_kernel_kilometers_to_si() {
        let state = BodyState::from_anise(
            NaifBodyId::EARTH,
            NaifBodyId::SOLAR_SYSTEM_BARYCENTER,
            TdbEpoch::j2000(),
            [1.0, -2.0, 0.5],
            [3.0, -4.0, 0.25],
        )
        .unwrap();

        assert_eq!(state.position_m, DVec3::new(1_000.0, -2_000.0, 500.0));
        assert_eq!(state.velocity_mps, DVec3::new(3_000.0, -4_000.0, 250.0));
    }

    #[test]
    fn embedded_kernel_set_matches_the_manifest_backed_authority() {
        let ephemeris = SpiceEphemeris::load_embedded().unwrap();
        let earth = ephemeris
            .state(
                NaifBodyId::EARTH,
                NaifBodyId::SOLAR_SYSTEM_BARYCENTER,
                TdbEpoch::j2000(),
            )
            .unwrap();

        assert_eq!(
            ephemeris.provenance().manifest_id,
            "naif-de440-egm2008-primary-v1"
        );
        assert!(earth.position_m.is_finite());
        assert!(earth.velocity_mps.is_finite());
        assert!(LeapSecondTable::parse_lsk(ephemeris.leap_seconds_lsk()).is_ok());
    }

    #[test]
    fn manifest_retains_typed_dataset_provenance() {
        let manifest = ron::from_str::<KernelManifest>(&fixture_manifest(
            "kernels",
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        ))
        .unwrap();

        assert_eq!(manifest.kernels.len(), 1);
        let dataset = &manifest.kernels[0];
        assert_eq!(dataset.role, ScientificDatasetRole::Translation);
        assert_eq!(dataset.kind, KernelKind::Spk);
        assert_eq!(
            dataset.coverage,
            Some(ScientificDatasetCoverage {
                start_julian_date: J2000_JULIAN_DATE_TDB,
                end_julian_date: J2000_JULIAN_DATE_TDB + 1.0,
            })
        );
        assert_eq!(dataset.frame, ScientificDatasetFrame::SsbIcrfJ2000);
        assert_eq!(dataset.time_scale, ScientificDatasetTimeScale::Tdb);
    }

    #[test]
    fn manifest_reports_explicitly_unavailable_dataset_role() {
        let contents = fixture_manifest(
            "kernels",
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        )
        .replace(
            "kernels: [(",
            "unavailable_roles: [EarthOrientation],\n                kernels: [(",
        );

        let manifest = ron::from_str::<KernelManifest>(&contents).unwrap();
        assert_eq!(
            manifest.unavailable_roles,
            vec![ScientificDatasetRole::EarthOrientation]
        );
    }

    #[test]
    fn manifest_rejects_checksum_mismatch() {
        let root = fixture_root();
        let kernel_root = root.join("kernels");
        fs::create_dir_all(&kernel_root).unwrap();
        fs::write(kernel_root.join("fixture.bsp"), b"abc").unwrap();
        let manifest_path = root.join("manifest.ron");
        fs::write(
            &manifest_path,
            fixture_manifest(
                "kernels",
                "0000000000000000000000000000000000000000000000000000000000000000",
            ),
        )
        .unwrap();

        let manifest = load_manifest(&manifest_path).unwrap();
        assert!(matches!(
            validate_manifest(&manifest_path, &manifest),
            Err(EphemerisError::KernelChecksum {
                role: ScientificDatasetRole::Translation,
                ..
            })
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn manifest_reports_missing_kernel() {
        let root = fixture_root();
        fs::create_dir_all(root.join("kernels")).unwrap();
        let manifest_path = root.join("manifest.ron");
        let contents = fixture_manifest(
            "kernels",
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        )
        .replace(
            "kernels: [(",
            "unavailable_roles: [EarthOrientation],\n                kernels: [(",
        );
        fs::write(&manifest_path, contents).unwrap();

        let manifest = load_manifest(&manifest_path).unwrap();
        assert!(matches!(
            validate_manifest(&manifest_path, &manifest),
            Err(EphemerisError::KernelRead {
                role: ScientificDatasetRole::Translation,
                ..
            })
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn manifest_rejects_missing_translation_role() {
        let root = fixture_root();
        fs::create_dir_all(&root).unwrap();
        let manifest_path = root.join("manifest.ron");
        let contents = fixture_manifest(
            "kernels",
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        )
        .replace("role: Translation", "role: Orientation")
        .replace("kind: Spk", "kind: TextPck")
        .replace("frame: SsbIcrfJ2000", "frame: IauBodyFixed");
        fs::write(&manifest_path, contents).unwrap();

        assert!(matches!(
            load_manifest(&manifest_path),
            Err(EphemerisError::InvalidManifest { .. })
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn manifest_rejects_translation_coverage_mismatch() {
        let root = fixture_root();
        fs::create_dir_all(&root).unwrap();
        let manifest_path = root.join("manifest.ron");
        let contents = fixture_manifest(
            "kernels",
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        )
        .replace(
            "end_julian_date_tdb: 2451546.0",
            "end_julian_date_tdb: 2451547.0",
        );
        fs::write(&manifest_path, contents).unwrap();

        assert!(matches!(
            load_manifest(&manifest_path),
            Err(EphemerisError::InvalidManifest { .. })
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn provenance_reports_out_of_coverage_tdb_datasets() {
        let root = fixture_root();
        let kernel_root = root.join("kernels");
        fs::create_dir_all(&kernel_root).unwrap();
        fs::write(kernel_root.join("fixture.bsp"), b"abc").unwrap();
        let manifest_path = root.join("manifest.ron");
        let contents = fixture_manifest(
            "kernels",
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        )
        .replace(
            "kernels: [(",
            "unavailable_roles: [EarthOrientation],\n                kernels: [(",
        );
        fs::write(&manifest_path, contents).unwrap();

        let manifest = load_manifest(&manifest_path).unwrap();
        let provenance = validate_manifest(&manifest_path, &manifest).unwrap();
        assert_eq!(
            provenance.dataset_statuses_at_tdb(
                TdbEpoch::from_julian_date(J2000_JULIAN_DATE_TDB + 2.0).unwrap()
            ),
            vec![
                ScientificDatasetStatus {
                    role: ScientificDatasetRole::Translation,
                    file_name: Some("fixture.bsp".to_string()),
                    availability: ScientificDatasetAvailability::OutOfCoverage,
                },
                ScientificDatasetStatus {
                    role: ScientificDatasetRole::EarthOrientation,
                    file_name: None,
                    availability: ScientificDatasetAvailability::Unavailable,
                }
            ]
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn manifest_rejects_epoch_outside_declared_coverage() {
        let coverage = KernelCoverage {
            start_julian_date_tdb: J2000_JULIAN_DATE_TDB,
            end_julian_date_tdb: J2000_JULIAN_DATE_TDB + 1.0,
        };

        assert!(coverage.contains(TdbEpoch::j2000()));
        assert!(!coverage.contains(TdbEpoch::from_seconds_since_j2000(172_800.0).unwrap()));
    }

    #[test]
    fn provisioned_pck_evaluates_earth_orientation_from_the_kernel_contract() {
        let ephemeris = SpiceEphemeris::load("assets/configs/ephemeris/de440.ron").unwrap();
        let orientation = ephemeris
            .orientation(NaifBodyId::EARTH, TdbEpoch::j2000())
            .unwrap();

        assert_eq!(orientation.provenance.source, OrientationDataSource::Kernel);
        assert_eq!(
            orientation.provenance.inertial_frame,
            OrientationInertialFrame::IcrfJ2000
        );
        assert_eq!(
            orientation.provenance.body_fixed_frame,
            OrientationBodyFixedFrame::IauBodyFixed
        );
        assert!(orientation.inertial_to_body_fixed.is_finite());
        assert!(orientation.angular_velocity_inertial_rad_s.is_finite());
        assert!(orientation.angular_velocity_inertial_rad_s.z > 0.0);
    }

    #[test]
    fn provisioned_mars_orientation_uses_the_mola_iau2000_override() {
        let ephemeris = SpiceEphemeris::load("assets/configs/ephemeris/de440.ron").unwrap();
        let mars = ephemeris
            .orientation(NaifBodyId::MARS_BARYCENTER, TdbEpoch::j2000())
            .unwrap();
        let earth = ephemeris
            .orientation(NaifBodyId::EARTH, TdbEpoch::j2000())
            .unwrap();

        assert!(mars.provenance.version.contains(
            "mars_iau2000_v1.tpc#07ba38b939ae92c085882752a523addd749fde0abb7a3468423099ed02bb3949"
        ));
        assert!(earth.provenance.version.contains("pck00011.tpc#"));
        assert!(mars.inertial_to_body_fixed.is_finite());
        assert!(mars.angular_velocity_inertial_rad_s.is_finite());
    }

    #[test]
    fn provisioned_gm_evaluates_kernel_backed_body_parameters_in_si() {
        let ephemeris = SpiceEphemeris::load("assets/configs/ephemeris/de440.ron").unwrap();
        let earth_mu_m3_s2 = ephemeris
            .gravitational_parameter_m3_s2(NaifBodyId::EARTH)
            .unwrap();
        let sun_mu_m3_s2 = ephemeris
            .gravitational_parameter_m3_s2(NaifBodyId::SUN)
            .unwrap();

        assert!((earth_mu_m3_s2 - 3.986_004_355_070_226e14).abs() < 1.0);
        assert!(sun_mu_m3_s2 > earth_mu_m3_s2);
    }

    #[test]
    fn provisioned_egm2008_j2_model_is_validated_with_the_manifest() {
        let ephemeris = SpiceEphemeris::load("assets/configs/ephemeris/de440.ron").unwrap();
        let model = ephemeris.earth_j2_model();

        assert_eq!(model.model_id, "EGM2008");
        assert!(model.is_valid());
    }

    #[derive(Clone, Copy)]
    struct HorizonsStateReference {
        name: &'static str,
        target: NaifBodyId,
        center: NaifBodyId,
        julian_date_tdb: f64,
        position_km: DVec3,
        velocity_km_s: DVec3,
    }

    // These are geometric ICRF vectors from JPL Horizons DE441, retrieved on
    // 2026-08-30 with `EPHEM_TYPE=VECTORS`, `TIME_TYPE=TDB`, `OUT_UNITS=KM-S`,
    // `REF_PLANE=FRAME`, and `VEC_CORR=NONE`. DE440s is the provisioned runtime
    // authority, so the budgets cover its documented small DE440/DE441 delta,
    // not an analytic approximation or presentation-space rounding.
    const DE440S_DE441_POSITION_BUDGET_M: f64 = 100.0;
    const DE440S_DE441_VELOCITY_BUDGET_MPS: f64 = 1.0e-3;

    fn assert_matches_horizons_reference(
        ephemeris: &SpiceEphemeris,
        reference: HorizonsStateReference,
    ) {
        let epoch = TdbEpoch::from_julian_date(reference.julian_date_tdb).unwrap();
        let state = ephemeris
            .state(reference.target, reference.center, epoch)
            .unwrap();
        let expected_position_m = reference.position_km * KILOMETERS_TO_METERS;
        let expected_velocity_mps = reference.velocity_km_s * KILOMETERS_TO_METERS;
        let position_residual_m = state.position_m.distance(expected_position_m);
        let velocity_residual_mps = state.velocity_mps.distance(expected_velocity_mps);

        assert!(
            position_residual_m <= DE440S_DE441_POSITION_BUDGET_M,
            "{} at JD TDB {}: target {} relative to {} has position residual {} m; budget {} m (Horizons DE441 geometric ICRF)",
            reference.name,
            reference.julian_date_tdb,
            reference.target.value(),
            reference.center.value(),
            position_residual_m,
            DE440S_DE441_POSITION_BUDGET_M,
        );
        assert!(
            velocity_residual_mps <= DE440S_DE441_VELOCITY_BUDGET_MPS,
            "{} at JD TDB {}: target {} relative to {} has velocity residual {} m/s; budget {} m/s (Horizons DE441 geometric ICRF)",
            reference.name,
            reference.julian_date_tdb,
            reference.target.value(),
            reference.center.value(),
            velocity_residual_mps,
            DE440S_DE441_VELOCITY_BUDGET_MPS,
        );
    }

    #[test]
    #[ignore = "requires scripts/provision_de440_kernels.sh"]
    fn de440_states_match_recorded_horizons_references_across_epochs() {
        let ephemeris = SpiceEphemeris::load("assets/configs/ephemeris/de440.ron").unwrap();
        let references = [
            HorizonsStateReference {
                name: "Earth/SSB at J2000",
                target: NaifBodyId::EARTH,
                center: NaifBodyId::SOLAR_SYSTEM_BARYCENTER,
                julian_date_tdb: 2_451_545.0,
                position_km: DVec3::new(
                    -2.756_674_048_281_145e7,
                    1.323_613_811_535_491e8,
                    5.741_865_328_625_385e7,
                ),
                velocity_km_s: DVec3::new(
                    -2.978_494_749_851_088e1,
                    -5.029_753_814_928_081,
                    -2.180_645_069_035_755,
                ),
            },
            HorizonsStateReference {
                name: "Earth/SSB at 2020-01-01 TDB",
                target: NaifBodyId::EARTH,
                center: NaifBodyId::SOLAR_SYSTEM_BARYCENTER,
                julian_date_tdb: 2_458_849.5,
                position_km: DVec3::new(
                    -2.545_334_341_413_143e7,
                    1.340_372_255_727_666e8,
                    5.810_929_286_273_248e7,
                ),
                velocity_km_s: DVec3::new(
                    -2.986_338_200_299_215e1,
                    -4.740_000_899_098_53,
                    -2.053_804_264_578_785,
                ),
            },
            HorizonsStateReference {
                name: "Earth/SSB at 2030-01-01 TDB",
                target: NaifBodyId::EARTH,
                center: NaifBodyId::SOLAR_SYSTEM_BARYCENTER,
                julian_date_tdb: 2_462_502.5,
                position_km: DVec3::new(
                    -2.592_636_728_814_095e7,
                    1.328_867_520_755_589e8,
                    5.760_933_037_884_695e7,
                ),
                velocity_km_s: DVec3::new(
                    -2.982_040_565_319_705e1,
                    -4.921_880_284_757_354,
                    -2.134_418_313_712_851,
                ),
            },
            HorizonsStateReference {
                name: "Jupiter barycenter/SSB at J2000",
                target: NaifBodyId::JUPITER_BARYCENTER,
                center: NaifBodyId::SOLAR_SYSTEM_BARYCENTER,
                julian_date_tdb: 2_451_545.0,
                position_km: DVec3::new(
                    5.974_998_767_925_48e8,
                    4.089_903_139_317_586e8,
                    1.607_562_819_387_201e8,
                ),
                velocity_km_s: DVec3::new(
                    -7.900_525_116_640_771,
                    1.017_179_630_923_791e1,
                    4.552_467_787_262_923,
                ),
            },
            HorizonsStateReference {
                name: "Jupiter barycenter/SSB at 2020-01-01 TDB",
                target: NaifBodyId::JUPITER_BARYCENTER,
                center: NaifBodyId::SOLAR_SYSTEM_BARYCENTER,
                julian_date_tdb: 2_458_849.5,
                position_km: DVec3::new(
                    7.814_211_696_278_183e7,
                    -7.134_231_711_509_035e8,
                    -3.077_001_434_693_068e8,
                ),
                velocity_km_s: DVec3::new(
                    1.284_045_161_930_421e1,
                    1.888_435_760_560_088,
                    4.969_311_071_956_998e-1,
                ),
            },
            HorizonsStateReference {
                name: "Jupiter barycenter/SSB at 2030-01-01 TDB",
                target: NaifBodyId::JUPITER_BARYCENTER,
                center: NaifBodyId::SOLAR_SYSTEM_BARYCENTER,
                julian_date_tdb: 2_462_502.5,
                position_km: DVec3::new(
                    -6.009_939_337_897_289e8,
                    -5.056_518_667_674_727e8,
                    -2.020_966_454_330_997e8,
                ),
                velocity_km_s: DVec3::new(
                    8.616_519_447_313_426,
                    -8.267_349_456_967_262,
                    -3.753_346_028_507_114,
                ),
            },
            HorizonsStateReference {
                name: "Moon/Earth at J2000",
                target: NaifBodyId::MOON,
                center: NaifBodyId::EARTH,
                julian_date_tdb: 2_451_545.0,
                position_km: DVec3::new(
                    -2.916_083_841_877_129e5,
                    -2.667_168_338_540_655e5,
                    -7.610_248_730_658_794e4,
                ),
                velocity_km_s: DVec3::new(
                    6.435_313_889_889_519e-1,
                    -6.660_876_829_565_195e-1,
                    -3.013_257_046_610_932e-1,
                ),
            },
            HorizonsStateReference {
                name: "Moon/Earth at 2020-01-01 TDB",
                target: NaifBodyId::MOON,
                center: NaifBodyId::EARTH,
                julian_date_tdb: 2_458_849.5,
                position_km: DVec3::new(
                    3.901_856_393_400_028e5,
                    -7.652_259_535_377_791e4,
                    -7.072_465_410_445_34e4,
                ),
                velocity_km_s: DVec3::new(
                    2.487_277_177_505_814e-1,
                    8.724_607_189_917_472e-1,
                    3.400_651_264_502e-1,
                ),
            },
            HorizonsStateReference {
                name: "Moon/Earth at 2030-01-01 TDB",
                target: NaifBodyId::MOON,
                center: NaifBodyId::EARTH,
                julian_date_tdb: 2_462_502.5,
                position_km: DVec3::new(
                    -1.930_715_952_026_768e5,
                    -2.772_423_478_014_667e5,
                    -1.368_828_942_977_237e5,
                ),
                velocity_km_s: DVec3::new(
                    9.140_320_609_081_021e-1,
                    -5.532_798_398_327_774e-1,
                    -1.432_625_780_408_247e-1,
                ),
            },
        ];

        for reference in references {
            assert_matches_horizons_reference(&ephemeris, reference);
        }
    }
}
