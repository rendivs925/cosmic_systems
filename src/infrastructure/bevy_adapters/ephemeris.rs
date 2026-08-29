//! Bevy integration for the shared scientific ephemeris authority.

use bevy::prelude::*;

use crate::domain::services::ephemeris::{BodyState, NaifBodyId, SpiceEphemeris, TdbEpoch};
use crate::domain::services::simulation_time::SimulationTime;

pub const DEFAULT_EPHEMERIS_MANIFEST_PATH: &str = "assets/configs/ephemeris/de440.ron";

const SNAPSHOT_BODIES: [NaifBodyId; 11] = [
    NaifBodyId::SUN,
    NaifBodyId::MERCURY_BARYCENTER,
    NaifBodyId::VENUS_BARYCENTER,
    NaifBodyId::EARTH_MOON_BARYCENTER,
    NaifBodyId::MARS_BARYCENTER,
    NaifBodyId::JUPITER_BARYCENTER,
    NaifBodyId::SATURN_BARYCENTER,
    NaifBodyId::URANUS_BARYCENTER,
    NaifBodyId::NEPTUNE_BARYCENTER,
    NaifBodyId::EARTH,
    NaifBodyId::MOON,
];

/// Immutable local kernel authority shared across all application modes.
#[derive(Resource)]
pub struct EphemerisAuthority(pub SpiceEphemeris);

/// States evaluated once for a TDB epoch before simulation and presentation
/// consumers run. This is a celestial snapshot, not an entity-state manager.
#[derive(Resource, Default)]
pub struct EphemerisSnapshot {
    pub epoch: Option<TdbEpoch>,
    states: Vec<BodyState>,
}

impl EphemerisSnapshot {
    pub fn state(&self, target: NaifBodyId) -> Option<BodyState> {
        self.states
            .iter()
            .find(|state| state.target == target)
            .copied()
    }

    #[cfg(test)]
    pub(crate) fn from_states(states: Vec<BodyState>) -> Self {
        Self {
            epoch: states.first().map(|state| state.epoch),
            states,
        }
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
        let authority = SpiceEphemeris::load(DEFAULT_EPHEMERIS_MANIFEST_PATH).unwrap_or_else(|error| {
            panic!(
                "cannot initialize the offline DE440 ephemeris: {error}. Run scripts/provision_de440_kernels.sh"
            )
        });
        app.insert_resource(EphemerisAuthority(authority));
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
    simulation_time: Res<SimulationTime>,
    authority: Res<EphemerisAuthority>,
    mut snapshot: ResMut<EphemerisSnapshot>,
) {
    let Ok(epoch) = simulation_time.tdb_epoch() else {
        bevy::log::error!("cannot evaluate ephemeris from an invalid simulation epoch");
        return;
    };
    if snapshot.epoch == Some(epoch) {
        return;
    }

    let mut states = Vec::with_capacity(SNAPSHOT_BODIES.len());
    for target in SNAPSHOT_BODIES {
        match authority
            .0
            .state(target, NaifBodyId::SOLAR_SYSTEM_BARYCENTER, epoch)
        {
            Ok(state) => states.push(state),
            Err(error) => {
                bevy::log::error!("cannot evaluate shared ephemeris snapshot: {error}");
                return;
            }
        }
    }
    snapshot.epoch = Some(epoch);
    snapshot.states = states;
}

/// Refresh the presentation snapshot after a completed fixed tick changes the
/// authoritative simulation epoch.
fn update_ephemeris_snapshot_after_time_advance(
    simulation_time: Res<SimulationTime>,
    authority: Res<EphemerisAuthority>,
    snapshot: ResMut<EphemerisSnapshot>,
) {
    update_ephemeris_snapshot(simulation_time, authority, snapshot);
}
