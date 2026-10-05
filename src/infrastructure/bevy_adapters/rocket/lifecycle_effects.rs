//! Bounded, one-shot lifecycle presentation feedback.
//!
//! Staging, fairing, touchdown, splashdown, and crash feedback are spawned from
//! existing authoritative events as short-lived children of the affected
//! vehicle. A presentation timer retires them; no visual timer is stored in any
//! propulsion, separation, or recovery component.

use super::components::RocketPresentationQuality;
use super::events::{
    CrashEvent, FairingSeparatedEvent, SplashdownDetectedEvent, StageSeparatedEvent, TouchdownEvent,
};
use bevy::prelude::*;

/// Which transient visual a lifecycle event produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RocketTransientVisual {
    StagePuff,
    FairingPuff,
    TouchdownDust,
    Splashdown,
    CrashFlash,
}

impl RocketTransientVisual {
    /// Seconds the effect lives before it is retired.
    const fn duration_s(self) -> f32 {
        match self {
            Self::StagePuff | Self::FairingPuff => 0.8,
            Self::TouchdownDust => 1.4,
            Self::Splashdown => 1.6,
            Self::CrashFlash => 0.7,
        }
    }

    /// Peak presentation radius in meters.
    const fn peak_radius_m(self) -> f32 {
        match self {
            Self::StagePuff => 6.0,
            Self::FairingPuff => 5.0,
            Self::TouchdownDust => 10.0,
            Self::Splashdown => 12.0,
            Self::CrashFlash => 16.0,
        }
    }

    /// Secondary events are skipped when effect quality is reduced.
    const fn is_secondary(self) -> bool {
        matches!(self, Self::StagePuff | Self::FairingPuff)
    }
}

/// Presentation-only lifetime of one lifecycle feedback child.
#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct RocketTransientEffect {
    remaining_s: f32,
    duration_s: f32,
    peak_radius_m: f32,
}

/// Reusable puff mesh and materials for every lifecycle feedback child.
#[derive(Resource, Default)]
pub(crate) struct RocketLifecycleEffectAssets {
    puff_mesh: Option<Handle<Mesh>>,
    dust_material: Option<Handle<StandardMaterial>>,
    splash_material: Option<Handle<StandardMaterial>>,
    flash_material: Option<Handle<StandardMaterial>>,
}

impl RocketLifecycleEffectAssets {
    fn initialize(&mut self, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>) {
        if self.puff_mesh.is_some() {
            return;
        }
        self.puff_mesh = Some(meshes.add(Sphere::new(1.0)));
        self.dust_material = Some(materials.add(transient_material(
            Color::srgba(0.45, 0.34, 0.22, 0.34),
            AlphaMode::Blend,
            None,
        )));
        self.splash_material = Some(materials.add(transient_material(
            Color::srgba(0.82, 0.9, 0.95, 0.3),
            AlphaMode::Blend,
            None,
        )));
        self.flash_material = Some(materials.add(transient_material(
            Color::srgba(1.0, 0.42, 0.08, 0.4),
            AlphaMode::Add,
            Some(Color::srgb(1.0, 0.32, 0.05).to_linear() * 5.0),
        )));
    }

    fn handles(&self, visual: RocketTransientVisual) -> (Handle<Mesh>, Handle<StandardMaterial>) {
        let mesh = self.puff_mesh.clone().expect("puff mesh initialized");
        let material = match visual {
            RocketTransientVisual::StagePuff | RocketTransientVisual::TouchdownDust => self
                .dust_material
                .clone()
                .expect("dust material initialized"),
            RocketTransientVisual::FairingPuff | RocketTransientVisual::Splashdown => self
                .splash_material
                .clone()
                .expect("splash material initialized"),
            RocketTransientVisual::CrashFlash => self
                .flash_material
                .clone()
                .expect("flash material initialized"),
        };
        (mesh, material)
    }
}

fn transient_material(
    base_color: Color,
    alpha_mode: AlphaMode,
    emissive: Option<LinearRgba>,
) -> StandardMaterial {
    StandardMaterial {
        base_color,
        emissive: emissive.unwrap_or_default(),
        alpha_mode,
        unlit: true,
        cull_mode: None,
        ..default()
    }
}

/// Spawns bounded transient feedback for authoritative lifecycle events.
/// Quality `Disabled` suppresses all feedback; `Reduced` skips secondary
/// staging/fairing puffs.
#[expect(
    clippy::too_many_arguments,
    reason = "One adapter owns the bounded assets and reads the existing authoritative lifecycle events."
)]
pub(crate) fn spawn_rocket_lifecycle_effects(
    mut commands: Commands,
    quality: Res<RocketPresentationQuality>,
    mut assets: ResMut<RocketLifecycleEffectAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut stages: MessageReader<StageSeparatedEvent>,
    mut fairings: MessageReader<FairingSeparatedEvent>,
    mut touchdowns: MessageReader<TouchdownEvent>,
    mut splashdowns: MessageReader<SplashdownDetectedEvent>,
    mut crashes: MessageReader<CrashEvent>,
) {
    assets.initialize(&mut meshes, &mut materials);

    for event in stages.read() {
        if should_spawn(&quality, RocketTransientVisual::StagePuff) {
            spawn_transient(
                &mut commands,
                event.rocket,
                &assets,
                RocketTransientVisual::StagePuff,
            );
        }
    }
    for event in fairings.read() {
        if should_spawn(&quality, RocketTransientVisual::FairingPuff) {
            spawn_transient(
                &mut commands,
                event.rocket,
                &assets,
                RocketTransientVisual::FairingPuff,
            );
        }
    }
    for event in touchdowns.read() {
        if should_spawn(&quality, RocketTransientVisual::TouchdownDust) {
            spawn_transient(
                &mut commands,
                event.rocket,
                &assets,
                RocketTransientVisual::TouchdownDust,
            );
        }
    }
    for event in splashdowns.read() {
        if should_spawn(&quality, RocketTransientVisual::Splashdown) {
            spawn_transient(
                &mut commands,
                event.rocket,
                &assets,
                RocketTransientVisual::Splashdown,
            );
        }
    }
    for event in crashes.read() {
        if should_spawn(&quality, RocketTransientVisual::CrashFlash) {
            spawn_transient(
                &mut commands,
                event.rocket,
                &assets,
                RocketTransientVisual::CrashFlash,
            );
        }
    }
}

/// Presentation work gate: disabled suppresses everything; reduced skips
/// secondary staging/fairing feedback.
fn should_spawn(quality: &RocketPresentationQuality, visual: RocketTransientVisual) -> bool {
    if !quality.level.effects_enabled() {
        return false;
    }
    !visual.is_secondary() || quality.level.full_detail()
}

fn spawn_transient(
    commands: &mut Commands,
    target: Entity,
    assets: &RocketLifecycleEffectAssets,
    visual: RocketTransientVisual,
) {
    let (mesh, material) = assets.handles(visual);
    let peak_radius_m = visual.peak_radius_m();
    commands.entity(target).with_children(|parent| {
        parent.spawn((
            RocketTransientEffect {
                remaining_s: visual.duration_s(),
                duration_s: visual.duration_s(),
                peak_radius_m,
            },
            Mesh3d(mesh),
            MeshMaterial3d(material),
            Transform::from_scale(Vec3::splat(peak_radius_m * 0.35)),
            Visibility::Visible,
            Name::new("RocketLifecycleEffect"),
        ));
    });
}

/// Advances presentation timers, grows each puff, and retires expired children.
pub(crate) fn update_rocket_transient_effects(
    mut commands: Commands,
    time: Res<Time>,
    mut effects: Query<(Entity, &mut RocketTransientEffect, &mut Transform)>,
) {
    let delta_s = time.delta_secs();
    for (entity, mut effect, mut transform) in &mut effects {
        effect.remaining_s -= delta_s;
        if effect.remaining_s <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }
        let progress = 1.0 - (effect.remaining_s / effect.duration_s).clamp(0.0, 1.0);
        let radius_m = effect.peak_radius_m * (0.35 + 0.65 * progress);
        transform.scale = Vec3::splat(radius_m);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::bevy_adapters::rocket::components::RocketEffectQualityLevel;
    use bevy::time::TimeUpdateStrategy;

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(RocketPresentationQuality::default())
            .init_resource::<RocketLifecycleEffectAssets>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .add_message::<StageSeparatedEvent>()
            .add_message::<FairingSeparatedEvent>()
            .add_message::<TouchdownEvent>()
            .add_message::<SplashdownDetectedEvent>()
            .add_message::<CrashEvent>()
            .add_systems(
                Update,
                (
                    spawn_rocket_lifecycle_effects,
                    update_rocket_transient_effects,
                )
                    .chain(),
            );
        app
    }

    fn count_transients(app: &mut App) -> usize {
        app.world_mut()
            .query::<&RocketTransientEffect>()
            .iter(app.world())
            .count()
    }

    #[test]
    fn touchdown_spawns_a_transient_that_retires() {
        let mut app = test_app();
        let rocket = app.world_mut().spawn(Transform::default()).id();
        app.world_mut().write_message(TouchdownEvent {
            rocket,
            position_m: Default::default(),
            vertical_speed_mps: -2.0,
            lateral_speed_mps: 0.0,
            tilt_deg: 0.0,
            slope_deg: 0.0,
        });

        app.update();
        assert_eq!(count_transients(&mut app), 1);
    }

    #[test]
    fn expired_transient_is_retired() {
        let mut app = test_app();
        app.insert_resource(TimeUpdateStrategy::ManualDuration(
            std::time::Duration::from_secs(1),
        ));
        app.world_mut().spawn((
            RocketTransientEffect {
                remaining_s: 0.1,
                duration_s: 1.0,
                peak_radius_m: 5.0,
            },
            Transform::default(),
        ));

        // The first update primes virtual time; the second advances it by the
        // manual duration and retires the expired effect.
        app.update();
        app.update();
        assert_eq!(count_transients(&mut app), 0);
    }

    #[test]
    fn disabled_quality_suppresses_lifecycle_feedback() {
        let mut app = test_app();
        app.world_mut()
            .resource_mut::<RocketPresentationQuality>()
            .level = RocketEffectQualityLevel::Disabled;
        let rocket = app.world_mut().spawn(Transform::default()).id();
        app.world_mut().write_message(CrashEvent {
            rocket,
            position_m: Default::default(),
            vertical_speed_mps: -80.0,
            tilt_deg: 5.0,
        });

        app.update();
        assert_eq!(count_transients(&mut app), 0);
    }

    #[test]
    fn reduced_quality_skips_secondary_staging_feedback() {
        let mut app = test_app();
        app.world_mut()
            .resource_mut::<RocketPresentationQuality>()
            .level = RocketEffectQualityLevel::Reduced;
        let rocket = app.world_mut().spawn(Transform::default()).id();
        let spent = app.world_mut().spawn(Transform::default()).id();
        app.world_mut().write_message(StageSeparatedEvent {
            rocket,
            spent_stage: spent,
            shed_mass_kg: 1_000.0,
        });

        app.update();
        assert_eq!(count_transients(&mut app), 0);
    }
}
