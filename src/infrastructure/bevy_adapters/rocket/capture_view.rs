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
use crate::infrastructure::bevy_adapters::rocket::planet::RocketBoundPlanet;
use crate::infrastructure::bevy_adapters::terrain::render::RenderOrigin;

/// Environment variable that pins the flight camera for a capture.
pub const CAPTURE_POSE_ENV: &str = "COSMIC_SYSTEMS_CAPTURE_POSE";

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
    let (position, rotation) =
        nadir_camera_pose(&pose, radius_m, body_to_inertial, render_origin.origin);
    transform.translation = position;
    transform.rotation = rotation;

    // A planet-scale view needs a far plane past the limb. The flight camera's
    // far plane is derived from the vehicle altitude, which would clip the deck
    // here; the near plane is pushed out with the altitude so the depth range
    // stays usable at a 10^4 dynamic range.
    if let Projection::Perspective(perspective) = projection.as_mut() {
        perspective.near = (pose.altitude_m * 0.01).clamp(1.0, 10_000.0) as f32;
        perspective.far = (4.0 * radius_m) as f32;
    }
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
}
