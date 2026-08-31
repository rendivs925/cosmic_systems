// Stage separation & spent-stage debris (AGENTS.md sections 8, 24).
//
// Separated stages and fairing halves become their own entities carrying a
// copy of the pre-separation f64 dynamics plus the separation impulse. Debris
// flies under simplified drag-only aerodynamics (gravity is reused from the
// shared `update_rocket_gravity`/`accumulate_forces` pipeline by matching the
// same components — no second gravity implementation). A lifecycle system
// despawns debris on surface contact or below an altitude threshold.

use crate::components::rocket::*;
use crate::domain::events::FairingSeparatedEvent;
use crate::domain::services::aerodynamics::drag_force_body;
use crate::domain::services::reference_frames::planet_inertial_to_body_fixed;
use crate::domain::services::rocket_propulsion::{
    active_vehicle_inertia, active_vehicle_mass_with_payload,
};
use crate::domain::services::terrain_collision::radar_altitude_m;
use crate::domain::value_objects::celestial_body_id::CelestialBodyId;
use crate::infrastructure::bevy_adapters::components::{PlanetComponent, PlanetTerrain};
use crate::infrastructure::bevy_adapters::ephemeris::EphemerisSnapshot;
use bevy::math::DVec3;
use bevy::prelude::*;

/// Drag coefficient of tumbling debris (cylinder + hardware, no control).
pub const SPENT_STAGE_DRAG_COEFFICIENT: f64 = 0.9;

/// Debris despawns below this radar altitude (m) or on ground contact.
/// Threshold avoids demanding ultra-local terrain for hardware we no longer
/// simulate in detail.
pub const SPENT_STAGE_DESPAWN_AGL_M: f64 = 100.0;

/// Fairing jettison altitude: above the meaningful aerodynamic-load regime,
/// matching typical payload-fairing separation (m).
pub const FAIRING_JETTISON_ALTITUDE_M: f64 = 110_000.0;

/// Lateral push given to each fairing half so they clear the vehicle (m/s).
pub const FAIRING_HALF_LATERAL_DV_MPS: f64 = 2.0;

/// Parameters describing one jettisoned piece (type-driven spawn: keeps the
/// spawner signature small and the call sites self-documenting).
pub struct SpentStageSpec {
    pub parent_rocket: Entity,
    pub planet_id: CelestialBodyId,
    pub dynamics: crate::domain::services::rocket_dynamics::RocketDynamicsState,
    pub radius_m: f32,
    pub height_m: f32,
    pub kind: SpentStageKind,
}

/// Spawn one jettisoned-hardware entity: authoritative f64 dynamics (already
/// including its separation impulse), simplified drag-only flight, and a
/// cylinder mesh scaled to the piece. Rendering reuses the same cylinder
/// primitive as the active vehicle.
pub fn spawn_spent_stage(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    spec: SpentStageSpec,
) -> Entity {
    let base_color = match spec.kind {
        SpentStageKind::Booster => Color::srgb(0.45, 0.45, 0.48),
        SpentStageKind::FairingHalf => Color::srgb(0.9, 0.9, 0.92),
    };
    let material = materials.add(StandardMaterial {
        base_color,
        metallic: 0.6,
        perceptual_roughness: 0.4,
        ..default()
    });
    let mesh = meshes.add(Mesh::from(Cylinder::new(spec.radius_m, spec.height_m)));

    commands
        .spawn((
            SpentStage {
                parent_rocket: spec.parent_rocket,
                kind: spec.kind,
            },
            RocketPhysicsState {
                dynamics: spec.dynamics,
            },
            RocketGeometry {
                radius_m: spec.radius_m,
                height_m: spec.height_m,
            },
            RocketMass(spec.dynamics.mass_kg),
            ForceAccumulator::default(),
            TorqueAccumulator::default(),
            GravityAcceleration::default(),
            SpecificForceAcceleration::default(),
            RocketFlightConditions::default(),
            RocketPlanetBinding {
                planet_name: spec.planet_id,
            },
            Mesh3d(mesh),
            MeshMaterial3d(material),
            Transform::default(),
            RocketRenderState::new(spec.dynamics),
            RocketFacade::default(),
        ))
        .id()
}

/// Simplified drag-only aerodynamics for debris (no lift/side force, no
/// attitude model): `F = -v̂ · q·Cd·A`. Consumes the shared atmosphere cache;
/// runs before force accumulation like every other force writer.
#[expect(
    clippy::type_complexity,
    reason = "The debris query groups the state required for its single drag-force write."
)]
pub fn spent_stage_aerodynamics(
    mut debris_query: Query<
        (
            &RocketPhysicsState,
            &RocketGeometry,
            &RocketFlightConditions,
            &mut ForceAccumulator,
        ),
        (With<SpentStage>, Without<RecoveringStage>),
    >,
) {
    for (_debris, geometry, conditions, mut force_accum) in debris_query.iter_mut() {
        let velocity = conditions.atmosphere_relative_velocity_mps;
        if conditions.airspeed_mps < 1.0 || conditions.density_kg_m3 <= 0.0 {
            continue;
        }
        let reference_area_m2 = std::f64::consts::PI * (geometry.radius_m as f64).powi(2);
        force_accum.0 += drag_force_body(
            conditions.dynamic_pressure_pa,
            SPENT_STAGE_DRAG_COEFFICIENT,
            reference_area_m2,
            velocity,
        );
    }
}

/// Despawn debris that reached the surface or fell below the lifecycle
/// threshold. Radar altitude comes from the same per-planet TerrainSource the
/// active vehicle uses (single terrain authority).
#[expect(
    clippy::type_complexity,
    reason = "The lifecycle query identifies eligible debris and its authoritative dynamics."
)]
pub fn update_spent_stage_lifecycle(
    mut commands: Commands,
    planet_query: Query<(&PlanetComponent, &PlanetTerrain)>,
    debris_query: Query<
        (Entity, &RocketPlanetBinding, &RocketPhysicsState),
        (With<SpentStage>, Without<RecoveringStage>),
    >,
    ephemeris_snapshot: Res<EphemerisSnapshot>,
) {
    for (entity, binding, debris) in debris_query.iter() {
        let Some((planet, planet_terrain)) = planet_query
            .iter()
            .find(|(planet, _)| planet.matches_body(&binding.planet_name))
        else {
            continue;
        };
        let radius_m = planet_terrain_radius(&planet_query, binding);
        if radius_m <= 0.0 {
            continue;
        }
        let Some(orientation) =
            ephemeris_snapshot.orientation_for_catalog_body(&planet.domain_planet.name)
        else {
            continue;
        };
        let position_body_fixed_m =
            planet_inertial_to_body_fixed(debris.dynamics.position_m, orientation);
        let agl_m = radar_altitude_m(&*planet_terrain.source, position_body_fixed_m, radius_m);
        if agl_m > SPENT_STAGE_DESPAWN_AGL_M {
            continue;
        }
        commands.entity(entity).despawn();
        bevy::log::info!("Spent stage despawned on surface contact ({agl_m:.1} m AGL)");
    }
}

fn planet_terrain_radius(
    planet_query: &Query<(&PlanetComponent, &PlanetTerrain)>,
    binding: &RocketPlanetBinding,
) -> f64 {
    planet_query
        .iter()
        .find(|(planet, _)| planet.matches_body(&binding.planet_name))
        .map(|(planet, _)| planet.domain_planet.radius_km as f64 * 1000.0)
        .unwrap_or(0.0)
}

/// Jettison the payload fairing once the vehicle climbs through the jettison
/// altitude. Drops the fairing mass from the authoritative dynamics, removes
/// the component, clears the propulsion payload tracker (so the shared
/// mass authority stops counting it), spawns two short-lived halves as
/// debris, and emits [`FairingSeparatedEvent`].
#[expect(
    clippy::type_complexity,
    reason = "The separation query combines cohesive vehicle state for one staging transition."
)]
pub fn check_fairing_separation(
    mut commands: Commands,
    mut fairing_writer: MessageWriter<FairingSeparatedEvent>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut rocket_query: Query<(
        Entity,
        &RocketPlanetBinding,
        &mut RocketPhysicsState,
        &RocketGeometry,
        &RocketFlightConditions,
        &mut RocketMass,
        &mut RocketPropulsion,
        &PayloadFairing,
        Option<&AblationState>,
    )>,
) {
    for (
        entity,
        binding,
        mut rocket,
        geometry,
        conditions,
        mut mass,
        mut propulsion,
        fairing,
        ablation,
    ) in rocket_query.iter_mut()
    {
        if conditions.altitude_m < FAIRING_JETTISON_ALTITUDE_M {
            continue;
        }

        // Remove the payload from the shared mass inventory before rebuilding
        // mass, COM, and inertia together.
        propulsion.attached_payload_kg = 0.0;
        let ablation_mass_loss_kg = ablation.map_or(0.0, |state| state.mass_loss_kg);
        let new_mass = (active_vehicle_mass_with_payload(
            &propulsion.vehicle.stages,
            &propulsion.propellant_remaining_kg,
            propulsion.active_stage,
            propulsion.attached_payload_kg,
        ) - ablation_mass_loss_kg)
            .max(1.0);
        let (inertia, center_of_mass_m) = active_vehicle_inertia(
            &propulsion.vehicle.stages,
            &propulsion.propellant_remaining_kg,
            propulsion.active_stage,
            propulsion.attached_payload_kg,
            ablation_mass_loss_kg,
            geometry.radius_m as f64,
            geometry.height_m as f64,
        );
        rocket.dynamics.mass_kg = new_mass;
        rocket.dynamics.inertia_body = inertia;
        rocket.dynamics.center_of_mass_m = center_of_mass_m;
        mass.0 = new_mass;
        commands.entity(entity).remove::<PayloadFairing>();

        // Two halves pushed apart along the body ±X axis.
        let orientation = rocket.dynamics.orientation;
        let lateral = orientation * DVec3::X;
        let half_dynamics = |sign: f64| {
            let mut d = rocket.dynamics;
            d.mass_kg = fairing.dry_mass_kg as f64 / 2.0;
            d.velocity_mps += lateral * sign * FAIRING_HALF_LATERAL_DV_MPS;
            d
        };
        let half_radius_m = 2.0; // cosmetic half-shell size, meters
        let half_height_m = 4.0;
        for sign in [1.0, -1.0] {
            spawn_spent_stage(
                &mut commands,
                &mut meshes,
                &mut materials,
                SpentStageSpec {
                    parent_rocket: entity,
                    planet_id: binding.planet_name.clone(),
                    dynamics: half_dynamics(sign),
                    radius_m: half_radius_m,
                    height_m: half_height_m,
                    kind: SpentStageKind::FairingHalf,
                },
            );
        }

        fairing_writer.write(FairingSeparatedEvent {
            rocket: entity,
            fairing_mass_kg: fairing.dry_mass_kg as f64,
        });
        bevy::log::info!(
            "Fairing separated at {:.0} km (-{:.0} kg)",
            conditions.altitude_m / 1000.0,
            fairing.dry_mass_kg
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::gravity::gravitational_acceleration;
    use crate::domain::services::rocket_dynamics::{rocket_inertia_tensor, RocketDynamicsState};
    use crate::domain::services::simulation_time::SimulationTime;
    use crate::infrastructure::bevy_adapters::rocket_dynamics::{
        accumulate_forces, integrate_6dof,
    };
    use bevy::math::{DQuat, DVec3};
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    /// Regression test for the force-accumulation pipeline: forces written by
    /// earlier systems (thrust, drag, parachutes) must survive
    /// `accumulate_forces` and reach `integrate_6dof`. This pins the `+=`
    /// semantics (an overwrite silently discards every non-gravity force).
    #[test]
    fn accumulated_forces_reach_integration() {
        let mut app = App::new();
        // MinimalPlugins provides TimePlugin so FixedUpdate actually ticks.
        app.add_plugins(MinimalPlugins);
        app.insert_resource(SimulationTime::new(1.0 / 64.0));
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
            1.0 / 64.0,
        )));
        // Production-mirroring pipeline: a force writer runs every step
        // (like thrust/aero/parachute writers), then accumulation, then
        // integration.
        fn write_test_force(mut query: Query<&mut ForceAccumulator>) {
            query.single_mut().unwrap().0 = DVec3::new(0.0, 20_000.0, 0.0);
        }
        // `.chain()` mirrors the production set ordering: writers before
        // accumulation before integration.
        app.add_systems(
            FixedUpdate,
            (write_test_force, accumulate_forces, integrate_6dof).chain(),
        );

        let mass_kg = 1_000.0;
        let (inertia, com) = rocket_inertia_tensor(mass_kg, 0.0, 1.85, 70.0);
        let dynamics = RocketDynamicsState::new(
            DVec3::new(6_571_000.0, 0.0, 0.0),
            DVec3::ZERO,
            DQuat::IDENTITY,
            mass_kg,
            inertia,
            com,
        );
        app.world_mut().spawn((
            RocketPhysicsState { dynamics },
            RocketMass(mass_kg),
            // Zero gravity isolates the external-force path under test.
            GravityAcceleration::default(),
            ForceAccumulator(DVec3::new(0.0, 20_000.0, 0.0)),
            TorqueAccumulator::default(),
        ));

        // Run several fixed steps: Δv per step = F/m·dt ≈ 0.3125 m/s.
        // A handful of steps tolerates intra-tick system-order ties while
        // still failing hard if accumulation discards the force entirely.
        // (+2 updates absorb the time-driver warmup that does not tick.)
        const STEPS: usize = 16;
        for _ in 0..STEPS + 2 {
            app.update();
        }

        let mut state = app.world_mut().query::<&RocketPhysicsState>();
        let velocity = state.single(app.world()).unwrap().dynamics.velocity_mps;
        let expected_dv = STEPS as f64 * (20_000.0 / mass_kg / 64.0);
        assert!(
            (velocity.y - expected_dv).abs() < expected_dv * 0.10,
            "external force was discarded by accumulation: v={velocity}, expected ≈{expected_dv}"
        );

        // Accumulators are reset after integration (no double-application).
        let mut accum = app.world_mut().query::<&ForceAccumulator>();
        let remaining = accum.single(app.world()).unwrap().0;
        assert!(remaining.length_squared() < 1e-12);
    }

    /// Gravity reaches integration through the same pipeline (shared authority).
    #[test]
    fn gravity_accumulates_and_integrates() {
        let mut app = App::new();
        // MinimalPlugins provides TimePlugin so FixedUpdate actually ticks.
        app.add_plugins(MinimalPlugins);
        app.insert_resource(SimulationTime::new(1.0 / 64.0));
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
            1.0 / 64.0,
        )));
        app.add_systems(FixedUpdate, (accumulate_forces, integrate_6dof).chain());

        let mass_kg = 1_000.0;
        let position = DVec3::new(6_571_000.0, 0.0, 0.0);
        let (inertia, com) = rocket_inertia_tensor(mass_kg, 0.0, 1.85, 70.0);
        let dynamics = RocketDynamicsState::new(
            position,
            DVec3::ZERO,
            DQuat::IDENTITY,
            mass_kg,
            inertia,
            com,
        );
        let g = gravitational_acceleration(5.97237e24, position, DVec3::ZERO);
        app.world_mut().spawn((
            RocketPhysicsState { dynamics },
            RocketMass(mass_kg),
            GravityAcceleration { value: g },
            ForceAccumulator::default(),
            TorqueAccumulator::default(),
        ));

        // (+2 updates absorb the time-driver warmup that does not tick.)
        const STEPS: usize = 32;
        for _ in 0..STEPS + 2 {
            app.update();
        }

        let expected_dv = STEPS as f64 * g.length() / 64.0;
        let mut state = app.world_mut().query::<&RocketPhysicsState>();
        let velocity = state.single(app.world()).unwrap().dynamics.velocity_mps;
        assert!(
            (velocity.length() - expected_dv).abs() < expected_dv * 0.10,
            "gravity did not integrate correctly: v={velocity}, expected |dv|≈{expected_dv}"
        );
    }
}
