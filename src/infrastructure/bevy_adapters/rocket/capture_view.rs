//! Fixed planet-scale capture view for cloud measurements.
//!
//! Every flight camera frames the vehicle. The widest available is a free
//! camera 40 km out, which spans roughly 100 km of deck — about one mesoscale
//! cloud regime. Cloud morphology is regional (see the cloud-engine
//! specification, section 3.3.1), so checking that a deck really does hold
//! several regimes needs a viewpoint wide enough to contain them, and comparing
//! two builds needs that viewpoint to be identical in both.
//!
//! `COSMIC_SYSTEMS_CAPTURE_POSE="<lat>,<lon>,<altitude_m>"` places the active
//! flight camera on a fixed body-fixed direction looking straight down at the
//! planet centre. The pose comes from the site, the planet radius and the
//! ephemeris only, so it does not depend on the vehicle state or on how far a
//! run got before the capture. Unset, the system does nothing.
//!
//! `COSMIC_SYSTEMS_CAPTURE_PITCH_DEG` tilts that view toward the local horizon
//! (`0` is nadir, `90` is horizontal) and `COSMIC_SYSTEMS_CAPTURE_FOV_DEG`
//! narrows the lens. Together they frame the atmospheric limb, which is a
//! sub-degree band in an ordinary field of view.
//!
//! This is a measurement facility, not a camera mode: it is opt-in through the
//! environment, has no user-visible input, and changes nothing about the
//! simulation. Presentation only.

use std::sync::OnceLock;

use bevy::math::{DQuat, DVec3};
use bevy::prelude::*;

use crate::domain::services::reference_frames::{
    body_fixed_to_planet_inertial_rotation, terrain_lat_lon_to_body_fixed,
};
use crate::infrastructure::bevy_adapters::entity_components::PlanetComponent;
use crate::infrastructure::bevy_adapters::ephemeris::EphemerisSnapshot;
use crate::infrastructure::bevy_adapters::rocket::components::{
    PrimaryVehicle, RocketAutopilot, RocketFlightConditions, RocketGeometry, RocketMissionState,
    RocketPhysicsState, RocketRenderState, SpentStage,
};
use crate::infrastructure::bevy_adapters::rocket::planet::RocketBoundPlanet;
use crate::infrastructure::bevy_adapters::terrain::render::RenderOrigin;

/// Environment variable that pins the flight camera for a capture.
pub const CAPTURE_POSE_ENV: &str = "COSMIC_SYSTEMS_CAPTURE_POSE";

/// Environment variable that pitches the capture view away from nadir, in
/// degrees. Zero (the default) keeps the straight-down view; 90 points at the
/// local horizon, which is what a limb capture needs.
pub const CAPTURE_PITCH_ENV: &str = "COSMIC_SYSTEMS_CAPTURE_PITCH_DEG";

/// Environment variable that overrides the capture camera's vertical field of
/// view, in degrees. A limb is a thin band in a normal lens; a narrow field
/// resolves it without changing the projection model.
pub const CAPTURE_FOV_ENV: &str = "COSMIC_SYSTEMS_CAPTURE_FOV_DEG";

/// Environment variable that advances the authoritative fixed pipeline before
/// the first captured frame. A capture has no keyboard, so an ascent or staging
/// frame would otherwise be impossible: the vehicle would still be on the pad.
/// This is an explicit launch order plus `n` real `FixedUpdate` steps, not a
/// teleported pose, so the frame shows the same physics a piloted run reaches.
pub const CAPTURE_WARMUP_STEPS_ENV: &str = "COSMIC_SYSTEMS_CAPTURE_WARMUP_STEPS";

/// Environment variable that puts the capture camera beside the active vehicle
/// instead of behind it (`side`, `1`, `on`, `true`). A chase camera looks down
/// the thrust axis, so an exhaust plume foreshortens to a dot; plume evidence
/// needs a profile view. Measurement only and inert when unset.
pub const CAPTURE_TRACK_ENV: &str = "COSMIC_SYSTEMS_CAPTURE_TRACK";

/// Upper bound on the warm start (200 s at 64 Hz). A capture is a measurement,
/// not a batch runner; a runaway value should fail visibly rather than hang.
const MAX_CAPTURE_WARMUP_STEPS: u32 = 64 * 200;

/// A fixed body-fixed viewpoint, in degrees and metres above the reference
/// surface.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CapturePose {
    pub latitude_deg: f64,
    pub longitude_deg: f64,
    pub altitude_m: f64,
}

impl CapturePose {
    /// Body-fixed unit direction of the viewpoint.
    pub fn body_direction(&self) -> DVec3 {
        terrain_lat_lon_to_body_fixed(self.latitude_deg, self.longitude_deg)
    }

    /// Body-fixed position of the viewpoint for a planet of `radius_m`.
    pub fn body_position(&self, radius_m: f64) -> DVec3 {
        self.body_direction() * (radius_m + self.altitude_m)
    }
}

/// The capture pose requested by the environment, parsed once.
pub fn capture_pose() -> Option<CapturePose> {
    static POSE: OnceLock<Option<CapturePose>> = OnceLock::new();
    *POSE.get_or_init(|| parse_capture_pose(std::env::var(CAPTURE_POSE_ENV).ok().as_deref()))
}

/// The capture pitch requested by the environment, parsed once. Absent means
/// nadir, which keeps every existing capture pose unchanged.
pub fn capture_pitch_deg() -> Option<f32> {
    static PITCH: OnceLock<Option<f32>> = OnceLock::new();
    *PITCH.get_or_init(|| parse_capture_pitch_deg(std::env::var(CAPTURE_PITCH_ENV).ok().as_deref()))
}

/// The capture field of view requested by the environment, parsed once.
pub fn capture_fov_deg() -> Option<f32> {
    static FOV: OnceLock<Option<f32>> = OnceLock::new();
    *FOV.get_or_init(|| parse_capture_fov_deg(std::env::var(CAPTURE_FOV_ENV).ok().as_deref()))
}

/// The warm-start length requested by the environment, parsed once.
pub fn capture_warmup_steps() -> u32 {
    static STEPS: OnceLock<u32> = OnceLock::new();
    *STEPS.get_or_init(|| {
        parse_capture_warmup_steps(std::env::var(CAPTURE_WARMUP_STEPS_ENV).ok().as_deref())
    })
}

/// Parse the warm-start step count. Absent or nonsense means no warm start, so
/// a normal interactive run never launches itself.
fn parse_capture_warmup_steps(value: Option<&str>) -> u32 {
    value
        .and_then(|value| value.trim().parse::<u32>().ok())
        .unwrap_or(0)
        .min(MAX_CAPTURE_WARMUP_STEPS)
}

/// One-shot guard for [`warm_start_capture_flight`].
#[derive(Resource, Default)]
pub struct CaptureWarmStartState {
    applied: bool,
}

/// Run the authoritative fixed pipeline forward from a real launch order before
/// the first captured frame.
///
/// This system is opt-in through [`CAPTURE_WARMUP_STEPS_ENV`] and runs once. It
/// deliberately reuses `spawn`'s pre-launch state and the production
/// `FixedUpdate` schedule, so the captured pose is reached by the same
/// guidance, propulsion, staging, gravity and contact systems a piloted flight
/// uses. No transform is written from here.
pub fn warm_start_capture_flight(world: &mut World) {
    world.init_resource::<CaptureWarmStartState>();
    if world.resource::<CaptureWarmStartState>().applied {
        return;
    }
    world.resource_mut::<CaptureWarmStartState>().applied = true;
    run_capture_warm_start(world, capture_warmup_steps());
}

/// Issue a launch order to every pre-launch vehicle and run `steps` production
/// fixed ticks. Split from the environment wrapper so the mechanics are
/// testable without touching process-wide environment state.
fn run_capture_warm_start(world: &mut World, steps: u32) {
    if steps == 0 {
        return;
    }

    {
        let mut query = world.query::<(
            &mut RocketMissionState,
            &RocketPhysicsState,
            &mut RocketRenderState,
            Option<&mut RocketAutopilot>,
        )>();
        for (mut mission, rocket, mut render, autopilot) in query.iter_mut(world) {
            if *mission != RocketMissionState::PreLaunch {
                continue;
            }
            // Mirror the Space launch order: drop the prelaunch interpolation
            // buffer and re-arm the tower-clearance gate.
            *render = RocketRenderState::new(rocket.dynamics);
            if let Some(mut autopilot) = autopilot {
                autopilot.ascent_pitch_over_engaged = false;
            }
            *mission = RocketMissionState::Launch;
        }
    }

    bevy::log::info!("Capture warm start: {steps} fixed steps from launch");
    for _ in 0..steps {
        world.run_schedule(FixedUpdate);
    }

    // Capture diagnostics: report the achieved altitude and attitude of every
    // body so a flight frame can be checked against the authoritative state.
    let mut bodies = world.query::<(
        &RocketPhysicsState,
        Option<&SpentStage>,
        Option<&RocketFlightConditions>,
    )>();
    for (state, spent, conditions) in bodies.iter(world) {
        let longitudinal = state.dynamics.orientation * DVec3::Y;
        bevy::log::info!(
            "Capture warm start body ({}): altitude {:.0} m, +Y inertial \
             [{:.3}, {:.3}, {:.3}], |v| {:.1} m/s",
            if spent.is_some() { "debris" } else { "primary" },
            conditions.map_or(f64::NAN, |conditions| conditions.altitude_m),
            longitudinal.x,
            longitudinal.y,
            longitudinal.z,
            state.dynamics.velocity_mps.length()
        );
    }
}

/// Parse an optional pitch in degrees. Nonsense is rejected rather than
/// silently clamped, so a mistyped capture is visible in the log.
fn parse_capture_pitch_deg(value: Option<&str>) -> Option<f32> {
    let pitch: f32 = value?.trim().parse().ok()?;
    (pitch.is_finite() && (-90.0..=90.0).contains(&pitch)).then_some(pitch)
}

/// Parse an optional vertical field of view in degrees.
fn parse_capture_fov_deg(value: Option<&str>) -> Option<f32> {
    let fov: f32 = value?.trim().parse().ok()?;
    (fov.is_finite() && (0.1..=120.0).contains(&fov)).then_some(fov)
}

/// Parse `<latitude>,<longitude>,<altitude_m>`. Latitude is clamped to the
/// valid range by rejection rather than silently wrapping: a capture at an
/// impossible latitude is a mistake worth surfacing in the log.
fn parse_capture_pose(value: Option<&str>) -> Option<CapturePose> {
    let raw = value?.trim();
    let mut parts = raw.split(',');
    let latitude_deg: f64 = parts.next()?.trim().parse().ok()?;
    let longitude_deg: f64 = parts.next()?.trim().parse().ok()?;
    let altitude_m: f64 = parts.next()?.trim().parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    if !latitude_deg.is_finite()
        || !longitude_deg.is_finite()
        || !altitude_m.is_finite()
        || !(-90.0..=90.0).contains(&latitude_deg)
        || !(-180.0..=180.0).contains(&longitude_deg)
        || altitude_m < 0.0
    {
        return None;
    }
    Some(CapturePose {
        latitude_deg,
        longitude_deg,
        altitude_m,
    })
}

/// Camera pose in the camera-relative render frame for a nadir view from
/// `pose` over a planet of `radius_m`.
///
/// Returns `(position, rotation)`; the camera looks along its local `-Z` at the
/// planet centre with the planet's north pole as up where that is well defined.
pub fn nadir_camera_pose(
    pose: &CapturePose,
    radius_m: f64,
    body_to_inertial: DQuat,
    render_origin_m: DVec3,
) -> (Vec3, Quat) {
    let body_position = pose.body_position(radius_m);
    let position = (body_to_inertial * body_position - render_origin_m).as_vec3();
    let centre = (-render_origin_m).as_vec3();
    let forward = (centre - position).normalize_or_zero();
    let north = (body_to_inertial * DVec3::Y).as_vec3();
    // Straight down a pole the north axis is parallel to the view direction and
    // cannot define roll, so fall back to the viewpoint's own meridian.
    let up = if north.cross(forward).length() > 1e-3 {
        north
    } else {
        (body_to_inertial * DVec3::X).as_vec3()
    };
    (
        position,
        Transform::from_translation(position)
            .looking_at(centre, up)
            .rotation,
    )
}

/// Camera pose for a capture that is pitched `pitch_deg` away from nadir
/// toward local north.
///
/// A limb capture needs a view near the horizon, not straight down. Pitching
/// the same nadir base rotation keeps the convention (camera local `-Z`
/// forward, local `+Y` up) that every other presentation system reads, and at
/// `pitch_deg == 90` the camera looks along the local horizontal.
pub fn pitched_camera_pose(
    pose: &CapturePose,
    radius_m: f64,
    body_to_inertial: DQuat,
    render_origin_m: DVec3,
    pitch_deg: f32,
) -> (Vec3, Quat) {
    let (position, nadir_rotation) =
        nadir_camera_pose(pose, radius_m, body_to_inertial, render_origin_m);
    let pitch = Quat::from_rotation_x(pitch_deg.to_radians());
    (position, nadir_rotation * pitch)
}

/// Override the active flight camera with the capture pose, when one is set.
///
/// Runs after the flight camera and its projection so it owns the final pose,
/// and before every system that reads the camera to place the sky, the
/// starfield and the cloud decks.
pub fn apply_capture_pose(
    render_origin: Res<RenderOrigin>,
    ephemeris: Res<EphemerisSnapshot>,
    bound_planet: Res<RocketBoundPlanet>,
    planet_query: Query<&PlanetComponent>,
    mut camera_query: Query<(&mut Transform, &mut Projection), With<Camera3d>>,
) {
    let Some(pose) = capture_pose() else {
        return;
    };
    let Some(bound_id) = bound_planet.0.as_ref() else {
        return;
    };
    let Some(planet) = planet_query
        .iter()
        .find(|planet| planet.matches_body(bound_id))
    else {
        return;
    };
    let Some(orientation) = ephemeris.orientation_for_catalog_body(bound_id.as_str()) else {
        return;
    };
    let Some((mut transform, mut projection)) = camera_query.iter_mut().next() else {
        return;
    };

    let radius_m = planet.domain_planet.radius_km as f64 * 1_000.0;
    let body_to_inertial = body_fixed_to_planet_inertial_rotation(orientation);
    let (position, rotation) = pitched_camera_pose(
        &pose,
        radius_m,
        body_to_inertial,
        render_origin.origin,
        capture_pitch_deg().unwrap_or(0.0),
    );
    transform.translation = position;
    transform.rotation = rotation;

    // A planet-scale view needs a far plane past the limb. The flight camera's
    // far plane is derived from the vehicle altitude, which would clip the deck
    // here; the near plane is pushed out with the altitude so the depth range
    // stays usable at a 10^4 dynamic range.
    if let Projection::Perspective(perspective) = projection.as_mut() {
        perspective.near = (pose.altitude_m * 0.01).clamp(1.0, 10_000.0) as f32;
        perspective.far = (4.0 * radius_m) as f32;
        if let Some(fov_deg) = capture_fov_deg() {
            perspective.fov = fov_deg.to_radians();
        }
    }
}

/// Side-on tracking camera for plume evidence.
///
/// The chase camera sits behind the vehicle, i.e. along the thrust axis, so a
/// plume is seen end-on and its length is not measurable. This view stands off
/// along the body `+Z` axis and looks at the vehicle, giving a true profile of
/// the same authoritative attitude. It is opt-in through
/// [`CAPTURE_TRACK_ENV`] and writes only the camera.
pub fn apply_capture_track_camera(
    mut camera_query: Query<(&mut Transform, &mut Projection), With<Camera3d>>,
    rocket_query: Query<(&Transform, &RocketGeometry), (PrimaryVehicle, Without<Camera3d>)>,
) {
    if !capture_track_enabled() {
        return;
    }
    let Ok((rocket, geometry)) = rocket_query.single() else {
        return;
    };
    let Some((camera, distance_m)) = track_camera_transform(rocket, geometry.height_m) else {
        return;
    };
    let Some((mut transform, mut projection)) = camera_query.iter_mut().next() else {
        return;
    };
    *transform = camera;
    if let Projection::Perspective(perspective) = projection.as_mut() {
        perspective.near = (distance_m * 0.01).max(0.1);
        perspective.far = distance_m * 20.0;
        if let Some(fov_deg) = capture_fov_deg() {
            perspective.fov = fov_deg.to_radians();
        }
    }
}

/// Side profile camera for a vehicle at `rocket` with body height `height_m`.
/// Returns the camera transform and its distance from the vehicle, or `None`
/// for a degenerate attitude. Pure so the framing rule is testable.
fn track_camera_transform(rocket: &Transform, height_m: f32) -> Option<(Transform, f32)> {
    let side = (rocket.rotation * Vec3::Z).normalize_or_zero();
    let up = (rocket.rotation * Vec3::Y).normalize_or_zero();
    if side.length_squared() < 0.5 || up.length_squared() < 0.5 {
        return None;
    }
    // Three body heights back keeps a full stack and its plume in frame.
    let distance_m = (height_m * 3.0).max(40.0);
    let position = rocket.translation + side * distance_m;
    Some((
        Transform::from_translation(position).looking_at(rocket.translation, up),
        distance_m,
    ))
}

fn capture_track_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| parse_capture_track(std::env::var(CAPTURE_TRACK_ENV).ok().as_deref()))
}

fn parse_capture_track(value: Option<&str>) -> bool {
    value.is_some_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "side" | "1" | "on" | "true" | "yes"
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_pose_parses_a_site_and_rejects_nonsense() {
        let pose = parse_capture_pose(Some("-8.0, 139.5, 1200000")).expect("valid pose");
        assert_eq!(pose.latitude_deg, -8.0);
        assert_eq!(pose.longitude_deg, 139.5);
        assert_eq!(pose.altitude_m, 1_200_000.0);

        assert!(parse_capture_pose(Some("-8.0,139.5")).is_none());
        assert!(parse_capture_pose(Some("-8.0,139.5,1000,7")).is_none());
        assert!(parse_capture_pose(Some("91.0,0.0,1000")).is_none());
        assert!(parse_capture_pose(Some("0.0,181.0,1000")).is_none());
        assert!(parse_capture_pose(Some("0.0,0.0,-1")).is_none());
        assert!(parse_capture_pose(Some("nan,0.0,1000")).is_none());
        assert!(parse_capture_pose(Some("")).is_none());
        assert!(parse_capture_pose(None).is_none());
    }

    /// The viewpoint must sit on the requested body-fixed direction at the
    /// requested altitude, and look at the planet centre.
    #[test]
    fn nadir_pose_looks_at_the_planet_centre() {
        let radius_m = 6_371_000.0;
        let pose = CapturePose {
            latitude_deg: 0.0,
            longitude_deg: 0.0,
            altitude_m: 1_200_000.0,
        };
        let origin = DVec3::new(12.0, -34.0, 56.0);
        let (position, rotation) = nadir_camera_pose(&pose, radius_m, DQuat::IDENTITY, origin);

        let expected = (pose.body_position(radius_m) - origin).as_vec3();
        assert!((position - expected).length() < 1e-3);
        // The camera looks along -Z toward the planet centre.
        let forward = rotation * Vec3::NEG_Z;
        let to_centre = (-origin).as_vec3() - position;
        let cos = forward.dot(to_centre.normalize());
        assert!(cos > 0.9999, "camera does not face the centre: {cos}");
        // Body-fixed origin direction is +X, so the altitude is measured
        // radially: the camera sits 1200 km above the requested surface point.
        let radial = pose.body_direction();
        let altitude = (position + origin.as_vec3()).as_dvec3().dot(radial) - radius_m;
        assert!((altitude - pose.altitude_m).abs() < 1.0);
    }

    #[test]
    fn capture_pitch_and_field_of_view_parse_and_reject_nonsense() {
        assert_eq!(parse_capture_pitch_deg(Some("0")), Some(0.0));
        assert_eq!(parse_capture_pitch_deg(Some(" -90 ")), Some(-90.0));
        assert_eq!(parse_capture_pitch_deg(Some("90")), Some(90.0));
        assert!(parse_capture_pitch_deg(Some("90.1")).is_none());
        assert!(parse_capture_pitch_deg(Some("nan")).is_none());
        assert!(parse_capture_pitch_deg(Some("")).is_none());
        assert!(parse_capture_pitch_deg(None).is_none());

        assert_eq!(parse_capture_fov_deg(Some("5")), Some(5.0));
        assert_eq!(parse_capture_fov_deg(Some("120")), Some(120.0));
        assert!(parse_capture_fov_deg(Some("0")).is_none());
        assert!(parse_capture_fov_deg(Some("121")).is_none());
        assert!(parse_capture_fov_deg(Some("sideways")).is_none());
        assert!(parse_capture_fov_deg(None).is_none());
    }

    /// A 90 degree pitch must look along the local horizontal, and a 0 degree
    /// pitch must be bit-identical to the nadir pose that every existing
    /// capture uses.
    #[test]
    fn pitch_rotates_the_nadir_view_toward_the_horizon() {
        let radius_m = 6_371_000.0;
        let pose = CapturePose {
            latitude_deg: 0.0,
            longitude_deg: 0.0,
            altitude_m: 400_000.0,
        };
        let origin = DVec3::ZERO;
        let (nadir_position, nadir_rotation) =
            pitched_camera_pose(&pose, radius_m, DQuat::IDENTITY, origin, 0.0);
        let (base_position, base_rotation) =
            nadir_camera_pose(&pose, radius_m, DQuat::IDENTITY, origin);
        assert_eq!(nadir_position, base_position);
        assert_eq!(nadir_rotation, base_rotation);

        let (_, horizon_rotation) =
            pitched_camera_pose(&pose, radius_m, DQuat::IDENTITY, origin, 90.0);
        let radial = pose.body_direction().as_vec3();
        let forward = horizon_rotation * Vec3::NEG_Z;
        assert!(
            forward.dot(radial).abs() < 1e-4,
            "a 90 degree pitch must be horizontal: {}",
            forward.dot(radial)
        );
        // Positive pitch tilts toward local north (+Y at this pose).
        assert!(forward.dot(Vec3::Y) > 0.999);
    }

    /// Looking straight down a pole has no well-defined north, and the pose must
    /// still be a valid rotation rather than a `NaN` quaternion.
    #[test]
    fn nadir_pose_is_finite_over_a_pole() {
        let pose = CapturePose {
            latitude_deg: 90.0,
            longitude_deg: 0.0,
            altitude_m: 900_000.0,
        };
        let (position, rotation) =
            nadir_camera_pose(&pose, 6_371_000.0, DQuat::IDENTITY, DVec3::ZERO);
        assert!(position.is_finite());
        assert!(rotation.is_finite());
        assert!((rotation.length() - 1.0).abs() < 1e-5);
    }

    #[derive(Resource, Default)]
    struct WarmStartProbe(u32);

    /// The warm start must issue a real launch order and run the production
    /// fixed schedule — no pose is written into the authoritative state. It is
    /// the only way a capture can show an ascent or a staging tick.
    #[test]
    fn warm_start_launches_and_runs_the_real_fixed_schedule() {
        use crate::domain::math::{DMat3, DQuat as MathDQuat, DVec3 as MathDVec3};
        use crate::domain::services::rocket_dynamics::RocketDynamicsState;

        let mut app = App::new();
        app.init_resource::<WarmStartProbe>();
        app.add_systems(FixedUpdate, |mut probe: ResMut<WarmStartProbe>| {
            probe.0 += 1;
        });

        let dynamics = RocketDynamicsState::new(
            MathDVec3::new(6_500_000.0, 0.0, 0.0),
            MathDVec3::ZERO,
            MathDQuat::IDENTITY,
            1_000.0,
            DMat3::IDENTITY,
            MathDVec3::ZERO,
        );
        let entity = app
            .world_mut()
            .spawn((
                RocketMissionState::default(),
                RocketPhysicsState { dynamics },
                RocketRenderState::new(dynamics),
            ))
            .id();

        run_capture_warm_start(app.world_mut(), 3);

        assert_eq!(app.world().resource::<WarmStartProbe>().0, 3);
        assert_eq!(
            *app.world().get::<RocketMissionState>(entity).unwrap(),
            RocketMissionState(crate::domain::entities::rocket::RocketMissionState::Launch)
        );
        // With no flight systems in this test the pose is untouched: the warm
        // start never teleports.
        let render = app.world().get::<RocketRenderState>(entity).unwrap();
        assert_eq!(render.current, dynamics);

        run_capture_warm_start(app.world_mut(), 0);
        assert_eq!(app.world().resource::<WarmStartProbe>().0, 3);
    }

    #[test]
    fn warm_start_steps_are_optional_and_bounded() {
        assert_eq!(parse_capture_warmup_steps(None), 0);
        assert_eq!(parse_capture_warmup_steps(Some(" 64 ")), 64);
        assert_eq!(parse_capture_warmup_steps(Some("nonsense")), 0);
        assert_eq!(
            parse_capture_warmup_steps(Some("999999")),
            MAX_CAPTURE_WARMUP_STEPS
        );
    }

    #[test]
    fn capture_track_parsing_is_opt_in() {
        assert!(!parse_capture_track(None));
        for value in ["side", "1", "on", "TRUE", " yes "] {
            assert!(
                parse_capture_track(Some(value)),
                "{value:?} must enable tracking"
            );
        }
        for value in ["0", "off", "", "chase"] {
            assert!(
                !parse_capture_track(Some(value)),
                "{value:?} must not enable tracking"
            );
        }
    }

    /// The plume is invisible from the chase camera because it is seen along
    /// its own axis. The capture view must stand off along the body lateral
    /// axis and look back at the vehicle, with distance scaled to the body.
    #[test]
    fn side_capture_camera_profiles_the_vehicle() {
        let rotation = DQuat::from_axis_angle(DVec3::new(0.2, 1.0, 0.3).normalize(), 0.7);
        let rocket =
            Transform::from_translation(Vec3::new(1.0, 2.0, 3.0)).with_rotation(rotation.as_quat());

        let (camera, distance_m) = track_camera_transform(&rocket, 13.2).expect("valid attitude");

        let offset = camera.translation - rocket.translation;
        let side = (rotation * DVec3::Z).as_vec3();
        assert!(
            offset.normalize().dot(side) > 0.9999,
            "camera must stand off along the body +Z lateral axis"
        );
        assert_eq!(distance_m, 40.0, "the minimum stand-off applies");
        assert!((offset.length() - distance_m).abs() < 1e-4);

        // The camera looks at the vehicle, not away from it.
        let forward = camera.rotation * Vec3::NEG_Z;
        assert!(forward.dot(-offset.normalize()) > 0.9999);
    }
}
