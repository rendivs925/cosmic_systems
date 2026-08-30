//! Bevy integration for the shared scientific ephemeris authority.

use bevy::math::DVec3;
use bevy::prelude::*;

use crate::domain::services::body_orientation::BodyOrientation;
use crate::domain::services::ephemeris::{
    BodyState, EphemerisError, NaifBodyId, ScientificDatasetAvailability, ScientificDatasetStatus,
    SpiceEphemeris, TdbEpoch,
};
use crate::domain::services::gravity::EarthJ2GravityModel;
use crate::domain::services::reference_frames::{
    barycentric_to_relative_state, barycentric_to_solar_inertial_state,
    icrf_j2000_to_solar_inertial,
};
use crate::domain::services::simulation_epoch::LeapSecondTable;
use crate::domain::services::simulation_time::SimulationTime;

#[cfg(not(target_arch = "wasm32"))]
pub const DEFAULT_EPHEMERIS_MANIFEST_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/configs/ephemeris/de440.ron"
);

/// States required by the shared snapshot. The Earth-Moon barycenter is not a
/// catalog body but remains available for same-epoch lunar relative states.
fn snapshot_bodies() -> impl Iterator<Item = NaifBodyId> {
    std::iter::once(NaifBodyId::EARTH_MOON_BARYCENTER)
        .chain(NaifBodyId::kernel_backed_catalog_targets())
}

/// Immutable local kernel authority shared across all application modes.
#[derive(Resource)]
pub struct EphemerisAuthority(pub SpiceEphemeris);

impl EphemerisAuthority {
    /// Sample one complete primary-body path from the shared local DE440
    /// authority. The result is presentation geometry in solar-inertial meters,
    /// not a second evaluated-state cache or a physics propagation path.
    pub fn sample_solar_inertial_orbit(
        &self,
        target: NaifBodyId,
        start_epoch: TdbEpoch,
        period_seconds: f64,
        samples: usize,
    ) -> Result<Vec<DVec3>, EphemerisError> {
        self.sample_relative_orbit_in_solar_inertial(
            target,
            NaifBodyId::SUN,
            start_epoch,
            period_seconds,
            samples,
        )
    }

    /// Sample target positions relative to a same-epoch center in the solar
    /// presentation axes. This produces visual geometry only; it does not
    /// create a second runtime body-state authority.
    pub fn sample_relative_orbit_in_solar_inertial(
        &self,
        target: NaifBodyId,
        center: NaifBodyId,
        start_epoch: TdbEpoch,
        period_seconds: f64,
        samples: usize,
    ) -> Result<Vec<DVec3>, EphemerisError> {
        let samples = samples.max(3);
        let mut path = Vec::with_capacity(samples);
        for index in 0..samples {
            let offset_seconds = period_seconds * index as f64 / samples as f64;
            let epoch = TdbEpoch::from_seconds_since_j2000(
                start_epoch.seconds_since_j2000() + offset_seconds,
            )?;
            let target_state = self
                .0
                .state(target, NaifBodyId::SOLAR_SYSTEM_BARYCENTER, epoch)?;
            let center_state = self
                .0
                .state(center, NaifBodyId::SOLAR_SYSTEM_BARYCENTER, epoch)?;
            path.push(icrf_j2000_to_solar_inertial(
                target_state.position_m - center_state.position_m,
            ));
        }
        Ok(path)
    }
}

/// Startup validation for every dataset declared in the shared local manifest.
/// It is immutable because datasets are fixed for a running simulation.
#[derive(Resource, Clone, Debug)]
pub struct ScientificDatasetReport(pub Vec<ScientificDatasetStatus>);

/// States evaluated once for a TDB epoch before simulation and presentation
/// consumers run. This is a celestial snapshot, not an entity-state manager.
#[derive(Resource, Default)]
pub struct EphemerisSnapshot {
    pub epoch: Option<TdbEpoch>,
    states: Vec<BodyState>,
    orientations: Vec<BodyOrientation>,
    gravitational_parameters_m3_s2: Vec<(NaifBodyId, f64)>,
    earth_j2_model: Option<EarthJ2GravityModel>,
}

impl EphemerisSnapshot {
    /// A snapshot may only be consumed for the exact scientific epoch it was
    /// evaluated at. Retaining an older complete snapshot is useful for
    /// diagnostics, but presenting it as a later simulation state is not.
    pub fn is_current_at(&self, epoch: TdbEpoch) -> bool {
        self.epoch == Some(epoch)
    }

    pub fn state(&self, target: NaifBodyId) -> Option<BodyState> {
        self.states
            .iter()
            .find(|state| state.target == target)
            .copied()
    }

    /// Derive one same-epoch SSB/ICRF state relative to another snapshot body.
    /// The reference-frame authority validates both centers and epochs before
    /// subtraction, so consumers cannot combine state from different ticks.
    pub fn relative_state(&self, target: NaifBodyId, center: NaifBodyId) -> Option<BodyState> {
        barycentric_to_relative_state(self.state(target)?, self.state(center)?).ok()
    }

    /// Derive one same-epoch state in the project's solar-inertial axes. This
    /// is the sole snapshot adapter for ICRF-to-solar frame conversion.
    pub fn solar_inertial_relative_state(
        &self,
        target: NaifBodyId,
        center: NaifBodyId,
    ) -> Option<BodyState> {
        barycentric_to_solar_inertial_state(self.state(target)?, self.state(center)?).ok()
    }

    /// Orientation evaluated from the same PCK and TDB epoch as [`Self::state`].
    pub fn orientation(&self, target: NaifBodyId) -> Option<&BodyOrientation> {
        self.orientations
            .iter()
            .find(|orientation| orientation.target == target)
    }

    /// Return the IAU orientation corresponding to a kernel-backed catalog body.
    pub fn orientation_for_catalog_body(&self, catalog_name: &str) -> Option<&BodyOrientation> {
        NaifBodyId::for_catalog_name(catalog_name).and_then(|target| self.orientation(target))
    }

    /// Validated `gm_de440.tpc` value for a kernel-backed body, in SI m³/s².
    pub fn gravitational_parameter_m3_s2(&self, target: NaifBodyId) -> Option<f64> {
        self.gravitational_parameters_m3_s2
            .iter()
            .find_map(|(parameter_target, mu_m3_s2)| {
                (*parameter_target == target).then_some(*mu_m3_s2)
            })
    }

    /// Validated `gm_de440.tpc` value for a kernel-backed catalog body, in SI
    /// m³/s². This is the sole adapter from catalog name to scientific GM.
    pub fn gravitational_parameter_for_catalog_body(&self, catalog_name: &str) -> Option<f64> {
        NaifBodyId::for_catalog_name(catalog_name)
            .and_then(|target| self.gravitational_parameter_m3_s2(target))
    }

    /// Validated Earth J2 model from the shared scientific dataset manifest.
    pub fn earth_j2_model(&self) -> Option<&EarthJ2GravityModel> {
        self.earth_j2_model.as_ref()
    }

    #[cfg(test)]
    pub(crate) fn from_states(states: Vec<BodyState>) -> Self {
        Self {
            epoch: states.first().map(|state| state.epoch),
            states,
            orientations: Vec::new(),
            gravitational_parameters_m3_s2: Vec::new(),
            earth_j2_model: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn from_states_with_gravitational_parameters(
        states: Vec<BodyState>,
        gravitational_parameters_m3_s2: Vec<(NaifBodyId, f64)>,
    ) -> Self {
        Self {
            epoch: states.first().map(|state| state.epoch),
            states,
            orientations: Vec::new(),
            gravitational_parameters_m3_s2,
            earth_j2_model: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn from_states_and_orientations(
        states: Vec<BodyState>,
        orientations: Vec<BodyOrientation>,
    ) -> Self {
        Self {
            epoch: states
                .first()
                .map(|state| state.epoch)
                .or_else(|| orientations.first().map(|orientation| orientation.epoch)),
            states,
            orientations,
            gravitational_parameters_m3_s2: Vec::new(),
            earth_j2_model: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn from_states_orientations_and_gravitational_parameters(
        states: Vec<BodyState>,
        orientations: Vec<BodyOrientation>,
        gravitational_parameters_m3_s2: Vec<(NaifBodyId, f64)>,
    ) -> Self {
        Self {
            epoch: states
                .first()
                .map(|state| state.epoch)
                .or_else(|| orientations.first().map(|orientation| orientation.epoch)),
            states,
            orientations,
            gravitational_parameters_m3_s2,
            earth_j2_model: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_earth_j2_model(mut self, earth_j2_model: EarthJ2GravityModel) -> Self {
        self.earth_j2_model = Some(earth_j2_model);
        self
    }
}

/// Shared kernel composition. All modes load the same immutable local manifest;
/// mode-specific systems only consume the resulting snapshot.
pub struct EphemerisPlugin;

/// Shared ordering stages around a fixed simulation tick. The first evaluation
/// supplies force consumers at the tick's starting epoch; the second refresh
/// exposes the completed epoch to presentation and the next tick.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EphemerisSet {
    EvaluateForTick,
    RefreshAfterTimeAdvance,
}

impl Plugin for EphemerisPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(not(target_arch = "wasm32"))]
        let authority = SpiceEphemeris::load(DEFAULT_EPHEMERIS_MANIFEST_PATH);
        #[cfg(target_arch = "wasm32")]
        let authority = SpiceEphemeris::load_embedded();
        let authority = authority.unwrap_or_else(|error| {
            panic!(
                "cannot initialize the offline DE440 ephemeris: {error}. Run scripts/provision_de440_kernels.sh"
            )
        });
        let leap_seconds = LeapSecondTable::parse_lsk(authority.leap_seconds_lsk()).unwrap_or_else(|error| {
            panic!(
                "cannot parse validated leap-second dataset from the offline DE440 authority: {error}"
            )
        });
        app.world_mut()
            .resource_mut::<SimulationTime>()
            .configure_scientific_epoch(leap_seconds)
            .unwrap_or_else(|error| {
                panic!("cannot configure scientific simulation epoch: {error}")
            });
        let dataset_report = authority
            .provenance()
            .dataset_statuses_at_tdb(TdbEpoch::j2000());
        for status in &dataset_report {
            match status.availability {
                ScientificDatasetAvailability::Validated => bevy::log::info!(
                    "scientific dataset role={} file={} status=validated",
                    status.role,
                    status.file_name.as_deref().unwrap_or("not-applicable"),
                ),
                ScientificDatasetAvailability::Unavailable => {
                    bevy::log::warn!("scientific dataset role={} status=unavailable", status.role,)
                }
                ScientificDatasetAvailability::OutOfCoverage => panic!(
                    "scientific dataset role={} file={} is outside startup TDB coverage",
                    status.role,
                    status.file_name.as_deref().unwrap_or("not-applicable"),
                ),
            }
        }
        app.insert_resource(EphemerisAuthority(authority));
        app.insert_resource(ScientificDatasetReport(dataset_report));
        app.init_resource::<EphemerisSnapshot>();
        app.add_systems(Startup, update_ephemeris_snapshot);
        app.add_systems(
            FixedUpdate,
            (
                update_ephemeris_snapshot.in_set(EphemerisSet::EvaluateForTick),
                update_ephemeris_snapshot_after_time_advance
                    .in_set(EphemerisSet::RefreshAfterTimeAdvance),
            ),
        );
    }
}

/// Refresh the complete shared primary-body snapshot for the authoritative TDB
/// epoch. A failed kernel query leaves the last complete snapshot intact.
pub fn update_ephemeris_snapshot(
    mut simulation_time: ResMut<SimulationTime>,
    authority: Res<EphemerisAuthority>,
    mut snapshot: ResMut<EphemerisSnapshot>,
) {
    let Ok(epoch) = simulation_time.tdb_epoch() else {
        bevy::log::error!("cannot evaluate ephemeris from an invalid simulation epoch");
        invalidate_snapshot_and_pause(&mut snapshot, &mut simulation_time);
        return;
    };
    if snapshot.epoch == Some(epoch) {
        return;
    }

    let earth_j2_model = authority.0.earth_j2_model().clone();

    let mut states = Vec::new();
    let mut orientations = Vec::new();
    let mut gravitational_parameters_m3_s2 = Vec::new();
    for target in snapshot_bodies() {
        match authority
            .0
            .state(target, NaifBodyId::SOLAR_SYSTEM_BARYCENTER, epoch)
        {
            Ok(state) => states.push(state),
            Err(error) => {
                bevy::log::error!("cannot evaluate shared ephemeris snapshot: {error}");
                invalidate_snapshot_and_pause(&mut snapshot, &mut simulation_time);
                return;
            }
        }
        if target.orientation_target().is_none() {
            continue;
        }
        match authority.0.gravitational_parameter_m3_s2(target) {
            Ok(mu_m3_s2) => gravitational_parameters_m3_s2.push((target, mu_m3_s2)),
            Err(error) => {
                bevy::log::error!("cannot evaluate shared gravitational parameter: {error}");
                invalidate_snapshot_and_pause(&mut snapshot, &mut simulation_time);
                return;
            }
        }
        match authority.0.orientation(target, epoch) {
            Ok(orientation) => orientations.push(orientation),
            Err(error) => {
                bevy::log::error!("cannot evaluate shared body orientation snapshot: {error}");
                invalidate_snapshot_and_pause(&mut snapshot, &mut simulation_time);
                return;
            }
        }
    }
    snapshot.epoch = Some(epoch);
    snapshot.states = states;
    snapshot.orientations = orientations;
    snapshot.gravitational_parameters_m3_s2 = gravitational_parameters_m3_s2;
    snapshot.earth_j2_model = Some(earth_j2_model);
}

fn invalidate_snapshot_and_pause(
    snapshot: &mut EphemerisSnapshot,
    simulation_time: &mut SimulationTime,
) {
    snapshot.epoch = None;
    snapshot.states.clear();
    snapshot.orientations.clear();
    snapshot.gravitational_parameters_m3_s2.clear();
    snapshot.earth_j2_model = None;
    simulation_time.paused = true;
}

/// Refresh the presentation snapshot after a completed fixed tick changes the
/// authoritative simulation epoch.
fn update_ephemeris_snapshot_after_time_advance(
    simulation_time: ResMut<SimulationTime>,
    authority: Res<EphemerisAuthority>,
    snapshot: ResMut<EphemerisSnapshot>,
) {
    update_ephemeris_snapshot(simulation_time, authority, snapshot);
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::DVec3;

    #[test]
    fn shared_ephemeris_plugin_configures_the_scientific_epoch() {
        let mut app = App::new();
        app.init_resource::<SimulationTime>();
        app.add_plugins(EphemerisPlugin);

        let epoch = app
            .world()
            .resource::<SimulationTime>()
            .scientific_epoch()
            .unwrap();
        assert_eq!(epoch.tdb_epoch(), TdbEpoch::j2000());
        assert_eq!(epoch.ut1_julian_date(), None);
    }

    #[test]
    fn snapshot_rejects_a_different_simulation_epoch() {
        let epoch = TdbEpoch::j2000();
        let snapshot = EphemerisSnapshot::from_states(vec![BodyState {
            target: NaifBodyId::SUN,
            center: NaifBodyId::SOLAR_SYSTEM_BARYCENTER,
            epoch,
            position_m: DVec3::ZERO,
            velocity_mps: DVec3::ZERO,
        }]);

        assert!(snapshot.is_current_at(epoch));
        assert!(
            !snapshot.is_current_at(TdbEpoch::from_seconds_since_j2000(1.0).expect("finite epoch"))
        );
    }

    #[test]
    fn snapshot_covers_every_kernel_backed_catalog_target_at_one_epoch() {
        let mut app = App::new();
        app.init_resource::<SimulationTime>();
        app.add_plugins(EphemerisPlugin);
        app.update();

        let snapshot = app.world().resource::<EphemerisSnapshot>();
        let expected_targets: Vec<_> = snapshot_bodies().collect();
        assert_eq!(snapshot.epoch, Some(TdbEpoch::j2000()));
        assert_eq!(snapshot.states.len(), expected_targets.len());
        assert_eq!(snapshot.orientations.len(), expected_targets.len() - 1);
        for target in expected_targets {
            let state = snapshot
                .state(target)
                .unwrap_or_else(|| panic!("snapshot missing NAIF {}", target.value()));
            assert_eq!(state.center, NaifBodyId::SOLAR_SYSTEM_BARYCENTER);
            assert_eq!(state.epoch, TdbEpoch::j2000());
        }
        for target in NaifBodyId::kernel_backed_catalog_targets() {
            assert!(snapshot.gravitational_parameter_m3_s2(target).is_some());
        }
        assert_eq!(snapshot.earth_j2_model().unwrap().model_id, "EGM2008");
    }

    #[test]
    fn relative_state_uses_two_same_epoch_snapshot_states() {
        let epoch = TdbEpoch::j2000();
        let snapshot = EphemerisSnapshot::from_states(vec![
            BodyState {
                target: NaifBodyId::EARTH,
                center: NaifBodyId::SOLAR_SYSTEM_BARYCENTER,
                epoch,
                position_m: DVec3::new(10.0, -5.0, 2.0),
                velocity_mps: DVec3::new(3.0, 4.0, -2.0),
            },
            BodyState {
                target: NaifBodyId::MOON,
                center: NaifBodyId::SOLAR_SYSTEM_BARYCENTER,
                epoch,
                position_m: DVec3::new(11.0, -8.0, 7.0),
                velocity_mps: DVec3::new(2.0, 6.0, -1.5),
            },
        ]);

        let relative = snapshot
            .relative_state(NaifBodyId::MOON, NaifBodyId::EARTH)
            .unwrap();
        assert_eq!(relative.target, NaifBodyId::MOON);
        assert_eq!(relative.center, NaifBodyId::EARTH);
        assert_eq!(relative.epoch, epoch);
        assert_eq!(relative.position_m, DVec3::new(1.0, -3.0, 5.0));
        assert_eq!(relative.velocity_mps, DVec3::new(-1.0, 2.0, 0.5));
        assert_eq!(
            snapshot.relative_state(NaifBodyId::SUN, NaifBodyId::EARTH),
            None
        );

        let solar_relative = snapshot
            .solar_inertial_relative_state(NaifBodyId::MOON, NaifBodyId::EARTH)
            .unwrap();
        assert_eq!(solar_relative.target, NaifBodyId::MOON);
        assert_eq!(solar_relative.center, NaifBodyId::EARTH);
        assert_eq!(solar_relative.epoch, epoch);
    }

    #[test]
    fn sampled_primary_orbit_starts_at_the_shared_snapshot_state() {
        let mut app = App::new();
        app.init_resource::<SimulationTime>();
        app.add_plugins(EphemerisPlugin);
        app.update();

        let snapshot = app.world().resource::<EphemerisSnapshot>();
        let epoch = snapshot
            .epoch
            .expect("startup evaluates the shared snapshot");
        let expected = snapshot
            .solar_inertial_relative_state(NaifBodyId::EARTH, NaifBodyId::SUN)
            .expect("Earth and Sun are in the startup snapshot")
            .position_m;
        let path = app
            .world()
            .resource::<EphemerisAuthority>()
            .sample_solar_inertial_orbit(NaifBodyId::EARTH, epoch, 365.256_363_004 * 86_400.0, 16)
            .expect("DE440 covers the Earth presentation path");

        assert_eq!(path.len(), 16);
        assert!(path.iter().all(|position| position.is_finite()));
        assert!(path[0].distance(expected) < 1e-6);
    }
}
