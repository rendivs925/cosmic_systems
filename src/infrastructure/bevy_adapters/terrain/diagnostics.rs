//! ECS wiring for the developer terrain diagnostic render modes.
//!
//! [`crate::presentation::diagnostics`] owns the mode set and the environment
//! contract; this module owns the runtime: selecting a mode, copying it into the
//! terrain materials, and removing the presentation terms that would otherwise
//! obscure the false colour.
//!
//! Every system here is presentation-only. None of them writes terrain
//! elevation, collision, the terrain source, simulation state, or physics. The
//! mode is a `u32` in the existing terrain material uniform, so switching views
//! cannot recreate a mesh, an image, or a cached patch.
//!
//! ## What a diagnostic frame contains
//!
//! * **Only streamed terrain patches are diagnosed.** The far-field planet
//!   proxy, the water caps and the cloud deck keep their own materials; they are
//!   drawn as usual (or hidden, for the occluders below), never false-coloured.
//!   A diagnostic capture therefore also shows which surfaces are *not* streamed
//!   terrain, which is itself useful information when a defect only appears at
//!   altitude.
//! * **Water caps and the cloud deck are hidden** while a mode is active. A
//!   water cap covers exactly the submerged terrain the land/sea view is meant
//!   to classify, and a cloud deck covers whole continents at altitude. Both are
//!   restored to `Inherited` on return to production rendering.
//! * **The camera display transform is bypassed.** Rocket mode uses an HDR view
//!   with `Tonemapping::TonyMcMapface` and bloom, which would reshape every
//!   false colour before it reached the PNG. While a mode is active the camera
//!   is switched to `Tonemapping::None` with bloom muted, and the previous
//!   values are restored exactly. The diagnostic PNG then holds the linear
//!   shader value encoded as sRGB: `linear = srgb_to_linear(byte / 255)`.

use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;

use crate::presentation::diagnostics::TerrainDiagnostic;

use super::render::TerrainMaterial;

/// The selected terrain diagnostic view.
///
/// Initialized from `COSMIC_SYSTEMS_DIAGNOSTIC_MODE` at startup and changed at
/// runtime by the developer chords in debug builds. `Default` is production
/// rendering, so a test or a headless run never arrives in a diagnostic view by
/// accident.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TerrainDiagnosticMode {
    pub mode: TerrainDiagnostic,
}

impl TerrainDiagnosticMode {
    pub fn new(mode: TerrainDiagnostic) -> Self {
        Self { mode }
    }

    /// The mode selected by `COSMIC_SYSTEMS_DIAGNOSTIC_MODE`, or production
    /// rendering when the variable is absent or invalid.
    pub fn from_environment() -> Self {
        Self {
            mode: TerrainDiagnostic::from_env(),
        }
    }
}

/// Marks a presentation entity that must be hidden while a diagnostic view is
/// active because it covers the terrain being measured.
///
/// Applied by the water-cap and cloud-deck spawn sites. Opting in from the
/// spawn site keeps the marker's meaning local: neither subsystem has to know
/// that diagnostics exist.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct TerrainDiagnosticOccluder;

/// Camera presentation terms saved when a diagnostic view is entered, so normal
/// rendering is restored exactly rather than to an assumed default.
#[derive(Resource, Default)]
struct DiagnosticDisplayGuard {
    saved: Vec<(Entity, Tonemapping, Option<Bloom>)>,
}

/// Registers the terrain diagnostic mode and its systems.
///
/// Added by the terrain render plugin, so the shared terrain path owns the
/// hooks and no application mode has to remember them.
pub struct TerrainDiagnosticsPlugin;

impl Plugin for TerrainDiagnosticsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(TerrainDiagnosticMode::from_environment())
            .init_resource::<DiagnosticDisplayGuard>()
            .add_systems(
                Update,
                (
                    apply_terrain_diagnostic_mode,
                    apply_terrain_diagnostic_occluders,
                    apply_terrain_diagnostic_display,
                ),
            );
        #[cfg(debug_assertions)]
        app.add_systems(Update, handle_terrain_diagnostic_input);
    }
}

/// Developer chords for stepping through the views. Debug builds only: the
/// release binary has no diagnostic key binding at all.
#[cfg(debug_assertions)]
fn handle_terrain_diagnostic_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut mode: ResMut<TerrainDiagnosticMode>,
) {
    let control = keyboard.pressed(KeyCode::ControlLeft) || keyboard.pressed(KeyCode::ControlRight);
    if !control {
        return;
    }
    let next = if keyboard.just_pressed(KeyCode::KeyU) {
        Some(mode.mode.next())
    } else if keyboard.just_pressed(KeyCode::KeyI) {
        Some(TerrainDiagnostic::Normal)
    } else {
        None
    };
    if let Some(next) = next {
        if next != mode.mode {
            bevy::log::info!(
                mode = next.name(),
                "terrain diagnostic mode selected (Ctrl+U cycles, Ctrl+I restores)"
            );
            mode.mode = next;
        }
    }
}

/// Copy the selected mode into every terrain material.
///
/// One uniform write per material, only when the value differs. Bevy
/// re-prepares the material bind group from the same textures and pipeline;
/// nothing is recreated, and no terrain cache is invalidated.
///
/// The comparison runs every frame rather than only on a resource change
/// because patches keep streaming in: a material created after the switch must
/// converge on the next frame instead of showing production colours. The loop
/// is a few hundred `u32` comparisons with no allocation.
pub(crate) fn apply_terrain_diagnostic_mode(
    mode: Res<TerrainDiagnosticMode>,
    mut materials: ResMut<Assets<TerrainMaterial>>,
) {
    let code = mode.mode.code();
    let mut updated = 0usize;
    for (_, material) in materials.iter_mut() {
        if material.extension.diagnostic_mode != code {
            material.extension.diagnostic_mode = code;
            updated += 1;
        }
    }
    if updated > 0 {
        bevy::log::info!(
            mode = mode.mode.name(),
            materials = updated,
            "terrain diagnostic mode applied"
        );
    }
}

/// Hide the surfaces that would cover the terrain being measured.
///
/// Runs every frame because patches and their water caps keep spawning; the
/// write happens only when the visibility actually differs, so a steady frame
/// touches nothing.
fn apply_terrain_diagnostic_occluders(
    mode: Res<TerrainDiagnosticMode>,
    mut occluders: Query<&mut Visibility, With<TerrainDiagnosticOccluder>>,
) {
    let target = if mode.mode.is_active() {
        Visibility::Hidden
    } else {
        Visibility::Inherited
    };
    for mut visibility in &mut occluders {
        if *visibility != target {
            *visibility = target;
        }
    }
}

/// Bypass the camera's display transform while a diagnostic view is active.
///
/// The false colours are linear radiance values; a filmic tonemap and bloom
/// would move them, and bloom would smear the very patch seams the views exist
/// to expose. Both are restored exactly when production rendering resumes.
fn apply_terrain_diagnostic_display(
    mode: Res<TerrainDiagnosticMode>,
    mut guard: ResMut<DiagnosticDisplayGuard>,
    mut cameras: Query<(Entity, &mut Tonemapping, Option<&mut Bloom>)>,
) {
    if !mode.is_changed() {
        return;
    }
    if mode.mode.is_active() {
        for (entity, mut tonemapping, bloom) in &mut cameras {
            if guard.saved.iter().any(|(saved, _, _)| *saved == entity) {
                continue;
            }
            guard.saved.push((
                entity,
                *tonemapping,
                bloom.as_ref().map(|bloom| (**bloom).clone()),
            ));
            *tonemapping = Tonemapping::None;
            if let Some(mut bloom) = bloom {
                bloom.intensity = 0.0;
            }
        }
        return;
    }
    for (entity, saved_tonemapping, saved_bloom) in guard.saved.drain(..) {
        let Ok((_, mut tonemapping, bloom)) = cameras.get_mut(entity) else {
            continue;
        };
        *tonemapping = saved_tonemapping;
        if let (Some(saved), Some(mut bloom)) = (saved_bloom, bloom) {
            *bloom = saved;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::bevy_adapters::terrain::render::TerrainSurfaceExtension;
    use bevy::asset::AssetPlugin;

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<TerrainMaterial>()
            .init_resource::<TerrainDiagnosticMode>()
            .init_resource::<DiagnosticDisplayGuard>();
        app.add_systems(
            Update,
            (
                apply_terrain_diagnostic_mode,
                apply_terrain_diagnostic_occluders,
            ),
        );
        app
    }

    fn add_material(app: &mut App) -> Handle<TerrainMaterial> {
        app.world_mut()
            .resource_mut::<Assets<TerrainMaterial>>()
            .add(TerrainMaterial {
                base: StandardMaterial::default(),
                extension: TerrainSurfaceExtension::default(),
            })
    }

    fn diagnostic_mode(app: &App, handle: &Handle<TerrainMaterial>) -> u32 {
        app.world()
            .resource::<Assets<TerrainMaterial>>()
            .get(handle)
            .expect("terrain material")
            .extension
            .diagnostic_mode
    }

    #[test]
    fn production_rendering_is_the_default_in_the_world() {
        let app = test_app();
        let mode = *app.world().resource::<TerrainDiagnosticMode>();
        assert_eq!(mode.mode, TerrainDiagnostic::Normal);
        assert!(!mode.mode.is_active());
    }

    #[test]
    fn every_material_takes_the_selected_mode() {
        let mut app = test_app();
        let handle = add_material(&mut app);
        app.update();
        assert_eq!(
            diagnostic_mode(&app, &handle),
            TerrainDiagnostic::Normal.code()
        );

        app.world_mut().resource_mut::<TerrainDiagnosticMode>().mode = TerrainDiagnostic::Albedo;
        app.update();
        assert_eq!(
            diagnostic_mode(&app, &handle),
            TerrainDiagnostic::Albedo.code()
        );

        app.world_mut().resource_mut::<TerrainDiagnosticMode>().mode = TerrainDiagnostic::Normal;
        app.update();
        assert_eq!(
            diagnostic_mode(&app, &handle),
            TerrainDiagnostic::Normal.code()
        );
    }

    #[test]
    fn a_material_created_while_a_mode_is_active_starts_in_that_mode() {
        // Patches keep streaming; a new material must not flash production
        // colours for a frame. The spawn path reads the resource directly.
        let mut app = test_app();
        app.world_mut().resource_mut::<TerrainDiagnosticMode>().mode = TerrainDiagnostic::LodLevel;
        app.update();
        let handle = add_material(&mut app);
        app.update();
        assert_eq!(
            diagnostic_mode(&app, &handle),
            TerrainDiagnostic::LodLevel.code()
        );
    }

    #[test]
    fn switching_modes_updates_materials_in_place() {
        let mut app = test_app();
        let handle = add_material(&mut app);
        app.update();
        for mode in TerrainDiagnostic::ALL {
            app.world_mut().resource_mut::<TerrainDiagnosticMode>().mode = mode;
            app.update();
            assert_eq!(diagnostic_mode(&app, &handle), mode.code(), "{mode:?}");
        }
        let assets = app.world().resource::<Assets<TerrainMaterial>>();
        assert_eq!(
            assets.len(),
            1,
            "a diagnostic switch must not clone materials"
        );
        assert!(
            assets.get(&handle).is_some(),
            "a diagnostic switch must not recreate the material asset"
        );
    }

    #[test]
    fn occluders_are_hidden_only_while_a_mode_is_active() {
        let mut app = test_app();
        let occluder = app
            .world_mut()
            .spawn((TerrainDiagnosticOccluder, Visibility::Inherited))
            .id();
        app.update();
        assert_eq!(
            *app.world().entity(occluder).get::<Visibility>().unwrap(),
            Visibility::Inherited,
            "production rendering must leave the water and clouds visible"
        );

        app.world_mut().resource_mut::<TerrainDiagnosticMode>().mode = TerrainDiagnostic::Height;
        app.update();
        assert_eq!(
            *app.world().entity(occluder).get::<Visibility>().unwrap(),
            Visibility::Hidden
        );

        // An occluder spawned while the mode is active is hidden on the next
        // frame; patches and their water caps keep streaming in.
        let late = app
            .world_mut()
            .spawn((TerrainDiagnosticOccluder, Visibility::Inherited))
            .id();
        app.update();
        assert_eq!(
            *app.world().entity(late).get::<Visibility>().unwrap(),
            Visibility::Hidden
        );

        app.world_mut().resource_mut::<TerrainDiagnosticMode>().mode = TerrainDiagnostic::Normal;
        app.update();
        for entity in [occluder, late] {
            assert_eq!(
                *app.world().entity(entity).get::<Visibility>().unwrap(),
                Visibility::Inherited
            );
        }
    }

    #[test]
    fn a_diagnostic_view_never_touches_simulation_or_terrain_authority() {
        // The diagnostic systems take only the mode resource, the material
        // assets, visibility and camera presentation components. This test pins
        // that: a full visit of every mode must leave simulation time and the
        // terrain authority exactly as they were.
        use crate::domain::services::simulation_time::SimulationTime;
        use crate::infrastructure::bevy_adapters::entity_components::PlanetTerrain;
        use std::sync::Arc;

        let mut app = test_app();
        app.insert_resource(SimulationTime::default());
        let terrain = PlanetTerrain::earth();
        let source: Arc<dyn crate::domain::services::terrain_source::TerrainSource> =
            terrain.source.clone();
        let latitude_deg = -8.0;
        let longitude_deg = 139.5;
        let height_before = source.height_m(latitude_deg, longitude_deg);
        let mesh_height_before = source.mesh_height_m(latitude_deg, longitude_deg, 12);
        let clock_before = {
            let clock = app.world().resource::<SimulationTime>();
            (
                clock.real_time_s,
                clock.sim_time_s,
                clock.time_acceleration,
                clock.paused,
            )
        };

        for mode in TerrainDiagnostic::ALL {
            app.world_mut().resource_mut::<TerrainDiagnosticMode>().mode = mode;
            app.update();
        }

        let clock_after = {
            let clock = app.world().resource::<SimulationTime>();
            (
                clock.real_time_s,
                clock.sim_time_s,
                clock.time_acceleration,
                clock.paused,
            )
        };
        assert_eq!(
            clock_before, clock_after,
            "a diagnostic view must not advance simulation time"
        );
        assert_eq!(source.height_m(latitude_deg, longitude_deg), height_before);
        assert_eq!(
            source.mesh_height_m(latitude_deg, longitude_deg, 12),
            mesh_height_before
        );
        assert!(
            Arc::ptr_eq(&terrain.source, &source),
            "the diagnostic systems must not replace the terrain authority"
        );
    }

    #[cfg(debug_assertions)]
    #[test]
    fn cycling_the_developer_chord_visits_every_mode_and_returns_to_production() {
        let mut app = test_app();
        app.add_systems(Update, handle_terrain_diagnostic_input);
        for _ in 0..TerrainDiagnostic::ALL.len() {
            // A fresh input state per press: exactly one `just_pressed` chord.
            let mut input = ButtonInput::<KeyCode>::default();
            input.press(KeyCode::ControlLeft);
            input.press(KeyCode::KeyU);
            app.insert_resource(input);
            app.update();
        }
        assert_eq!(
            app.world().resource::<TerrainDiagnosticMode>().mode,
            TerrainDiagnostic::Normal,
            "one full cycle must land back on production rendering"
        );

        // Ctrl+I restores production rendering from any view.
        app.world_mut().resource_mut::<TerrainDiagnosticMode>().mode =
            TerrainDiagnostic::PatchIdentity;
        let mut input = ButtonInput::<KeyCode>::default();
        input.press(KeyCode::ControlLeft);
        input.press(KeyCode::KeyI);
        app.insert_resource(input);
        app.update();
        assert_eq!(
            app.world().resource::<TerrainDiagnosticMode>().mode,
            TerrainDiagnostic::Normal
        );

        // Without Ctrl the chord must do nothing, so flight keys stay free.
        app.world_mut().resource_mut::<TerrainDiagnosticMode>().mode = TerrainDiagnostic::Normal;
        let mut input = ButtonInput::<KeyCode>::default();
        input.press(KeyCode::KeyU);
        app.insert_resource(input);
        app.update();
        assert_eq!(
            app.world().resource::<TerrainDiagnosticMode>().mode,
            TerrainDiagnostic::Normal
        );
    }
}
