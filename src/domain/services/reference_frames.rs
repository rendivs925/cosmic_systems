//! Authoritative reference-frame conversions for flight.
//!
//! Every subsystem that needs a frame conversion (gravity, aero, terrain,
//! camera) MUST use this module rather than re-implementing coordinate math
//! (AGENTS.md sections 14 and 51).
//!
//! # Frame conventions
//!
//! - **Solar-inertial**: origin at the Sun, axes aligned with the solar
//!   system's orbital rendering frame (the space returned by
//!   [`calculate_planet_position`]), right-handed Y-up, display units.
//! - **Planet-centered inertial**: origin at a planet's center, axes parallel
//!   to solar-inertial, real meters (f64).
//! - **Planet body-fixed**: rotates with the planet. Geodetic
//!   latitude/longitude/altitude positions are expressed directly in this
//!   frame: +Y toward the geographic north pole, +X toward lon 0, +Z toward
//!   lon +90°. Body-fixed → inertial applies the planet spin about +Y
//!   (via [`calculate_planet_rotation`]) followed by the axial tilt about +Z
//!   (via [`Planet::axial_tilt_deg`]), matching the existing tilt convention
//!   used in [`calculate_planet_position`].
//! - **Local tangent**: East-North-Up (ENU) triad at a geodetic reference
//!   point, real meters.
//! - **Rocket-body**: the vehicle's own orientation (`DQuat`).
//!
//! Positions in solar-inertial are in the visualization's display units;
//! positions in every other frame are in real meters. The meter ↔ display-unit
//! mapping flows exclusively through [`PhysicalScale`].

use crate::domain::entities::planet::Planet;
use crate::domain::services::physics_utils::calculate_planet_rotation;
use crate::domain::value_objects::celestial_body_id::CelestialBodyId;
use crate::domain::value_objects::launch_site_coordinates::LaunchSiteCoordinates;
use crate::domain::value_objects::physical_scale::PhysicalScale;
use bevy::math::{DQuat, DVec3, Vec3};
use bevy::transform::components::Transform;

/// The frames supported by the reference-frame module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReferenceFrame {
    SolarInertial,
    PlanetCenteredInertial,
    PlanetBodyFixed,
    LocalTangent,
    RocketBody,
}

/// Planet radius in meters for a [`Planet`].
pub fn planet_radius_m(planet: &Planet) -> f64 {
    planet.radius_km as f64 * 1000.0
}

// ---------------------------------------------------------------------------
// Geodetic (lat/lon/alt) <-> planet body-fixed
// ---------------------------------------------------------------------------

/// Map geodetic latitude/longitude/altitude to a planet body-fixed position
/// in meters (spherical model, consistent with
/// [`LaunchSiteCoordinates::to_planet_relative_position`]).
pub fn geodetic_to_body_fixed(site: &LaunchSiteCoordinates, planet: &Planet) -> DVec3 {
    let radius_m = planet_radius_m(planet);
    let lat_rad = (site.latitude_deg as f64).to_radians();
    let lon_rad = (site.longitude_deg as f64).to_radians();
    let r = radius_m + site.altitude_m as f64;
    DVec3::new(
        r * lat_rad.cos() * lon_rad.cos(),
        r * lat_rad.sin(),
        r * lat_rad.cos() * lon_rad.sin(),
    )
}

/// Map a planet body-fixed position (meters) back to geodetic coordinates.
pub fn body_fixed_to_geodetic(pos_bf: DVec3, planet: &Planet) -> LaunchSiteCoordinates {
    let r = pos_bf.length();
    let radius_m = planet_radius_m(planet);
    let lat_rad = (pos_bf.y / r).clamp(-1.0, 1.0).asin();
    let lon_rad = pos_bf.z.atan2(pos_bf.x);
    LaunchSiteCoordinates::new(
        CelestialBodyId::new(planet.name.clone())
            .expect("planet names are validated at catalog construction"),
        lat_rad.to_degrees() as f32,
        lon_rad.to_degrees() as f32,
        (r - radius_m) as f32,
    )
}

// ---------------------------------------------------------------------------
// Planet body-fixed <-> planet-centered inertial
// ---------------------------------------------------------------------------

/// Rotate a body-fixed position into the planet-centered inertial frame,
/// applying planet spin (about +Y) then axial tilt (about +Z).
pub fn body_fixed_to_planet_inertial(pos_bf: DVec3, planet: &Planet, time_days: f32) -> DVec3 {
    let rot = body_fixed_to_inertial_rotation(planet, time_days);
    rot * pos_bf
}

/// Rotate a planet-centered inertial position back into the body-fixed frame.
pub fn planet_inertial_to_body_fixed(pos_pci: DVec3, planet: &Planet, time_days: f32) -> DVec3 {
    let rot = body_fixed_to_inertial_rotation(planet, time_days);
    rot.inverse() * pos_pci
}

/// The single authoritative body-fixed → inertial rotation for a planet.
/// Rotation that maps body-fixed vectors into the planet-centered inertial
/// frame at the supplied simulation epoch.
pub fn body_fixed_to_inertial_rotation(planet: &Planet, time_days: f32) -> DQuat {
    let spin_rad = calculate_planet_rotation(planet, time_days) as f64;
    let tilt_rad = planet.axial_tilt_deg as f64;
    DQuat::from_rotation_z(tilt_rad.to_radians()) * DQuat::from_rotation_y(spin_rad)
}

/// Velocity of a point fixed to the rotating planetary surface, expressed in
/// the planet-centered inertial frame. The spin axis includes the planet's
/// axial tilt, so this can be used directly for launch state, ground contact,
/// and atmosphere-relative velocity.
pub fn surface_velocity_in_planet_inertial(pos_pci: DVec3, planet: &Planet) -> DVec3 {
    let period_s = planet.rotation_period_hours as f64 * 3600.0;
    if !period_s.is_finite() || period_s <= 0.0 {
        return DVec3::ZERO;
    }
    let spin_axis_pci =
        DQuat::from_rotation_z((planet.axial_tilt_deg as f64).to_radians()) * DVec3::Y;
    let angular_velocity_rad_s = spin_axis_pci * (std::f64::consts::TAU / period_s);
    angular_velocity_rad_s.cross(pos_pci)
}

// ---------------------------------------------------------------------------
// Planet-centered inertial <-> solar-inertial
// ---------------------------------------------------------------------------

/// Convert a solar-inertial display position to a planet-centered inertial
/// position in meters, given the planet's solar-inertial display position.
pub fn solar_to_planet_inertial(
    pos_solar_units: Vec3,
    planet_solar_units: Vec3,
    scale: &PhysicalScale,
) -> DVec3 {
    let delta = pos_solar_units.as_dvec3() - planet_solar_units.as_dvec3();
    DVec3::new(
        scale.solar_units_to_meters(delta.x),
        scale.solar_units_to_meters(delta.y),
        scale.solar_units_to_meters(delta.z),
    )
}

/// Convert a planet-centered inertial position (meters) to a solar-inertial
/// display position.
pub fn planet_inertial_to_solar(
    pos_pci_m: DVec3,
    planet_solar_units: Vec3,
    scale: &PhysicalScale,
) -> Vec3 {
    let units = DVec3::new(
        scale.solar_meters_to_units(pos_pci_m.x),
        scale.solar_meters_to_units(pos_pci_m.y),
        scale.solar_meters_to_units(pos_pci_m.z),
    );
    (planet_solar_units.as_dvec3() + units).as_vec3()
}

// ---------------------------------------------------------------------------
// Local tangent (East-North-Up) frame
// ---------------------------------------------------------------------------

/// The ENU basis vectors (east, north, up) expressed in body-fixed
/// coordinates at a given geodetic latitude/longitude.
pub fn enu_basis(latitude_deg: f32, longitude_deg: f32) -> (DVec3, DVec3, DVec3) {
    let lat_rad = (latitude_deg as f64).to_radians();
    let lon_rad = (longitude_deg as f64).to_radians();
    let east = DVec3::new(-lon_rad.sin(), 0.0, lon_rad.cos());
    let north = DVec3::new(
        -lat_rad.sin() * lon_rad.cos(),
        lat_rad.cos(),
        -lat_rad.sin() * lon_rad.sin(),
    );
    let up = DVec3::new(
        lat_rad.cos() * lon_rad.cos(),
        lat_rad.sin(),
        lat_rad.cos() * lon_rad.sin(),
    );
    (east, north, up)
}

/// Convert a body-fixed position to ENU meters relative to a geodetic origin.
pub fn body_fixed_to_local_tangent(
    pos_bf: DVec3,
    origin: &LaunchSiteCoordinates,
    planet: &Planet,
) -> DVec3 {
    let origin_bf = geodetic_to_body_fixed(origin, planet);
    let delta = pos_bf - origin_bf;
    let (east, north, up) = enu_basis(origin.latitude_deg, origin.longitude_deg);
    DVec3::new(east.dot(delta), north.dot(delta), up.dot(delta))
}

/// Convert ENU meters relative to a geodetic origin to a body-fixed position.
pub fn local_tangent_to_body_fixed(
    enu: DVec3,
    origin: &LaunchSiteCoordinates,
    planet: &Planet,
) -> DVec3 {
    let origin_bf = geodetic_to_body_fixed(origin, planet);
    let (east, north, up) = enu_basis(origin.latitude_deg, origin.longitude_deg);
    origin_bf + east * enu.x + north * enu.y + up * enu.z
}

// ---------------------------------------------------------------------------
// Rocket physical state
// ---------------------------------------------------------------------------

/// High-precision rocket dynamics state, expressed in real meters (f64).
///
/// Position and velocity are relative to a chosen frame (typically the
/// planet-centered inertial frame or a local tangent frame). Rendering maps
/// this state to Bevy [`Transform`] only at the presentation boundary via
/// [`RocketPhysicalState::render_transform`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RocketPhysicalState {
    pub position_m: DVec3,
    pub velocity_mps: DVec3,
    pub orientation: DQuat,
}

impl RocketPhysicalState {
    pub fn new(position_m: DVec3, velocity_mps: DVec3, orientation: DQuat) -> Self {
        Self {
            position_m,
            velocity_mps,
            orientation,
        }
    }

    /// Map the f64 physical state to an f32 Bevy [`Transform`].
    ///
    /// The position is first rebased to `local_origin` in f64 so magnitudes
    /// stay small near the vehicle, then scaled to flight display units and
    /// downcast to f32. This avoids f32 cancellation at large distances.
    pub fn render_transform(&self, local_origin: DVec3, scale: &PhysicalScale) -> Transform {
        let local_m = self.position_m - local_origin;
        let display = DVec3::new(
            scale.flight_meters_to_units(local_m.x),
            scale.flight_meters_to_units(local_m.y),
            scale.flight_meters_to_units(local_m.z),
        )
        .as_vec3();
        Transform::from_translation(display).with_rotation(self.orientation.as_quat())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::planet_factory::PlanetFactory;
    use crate::domain::value_objects::launch_site_coordinates::predefined_sites;

    fn earth() -> Planet {
        PlanetFactory::create_by_name("Earth").unwrap()
    }

    fn ksc() -> LaunchSiteCoordinates {
        predefined_sites::kennedy_space_center()
    }

    #[test]
    fn geodetic_round_trips_through_body_fixed() {
        let planet = earth();
        let site = ksc();
        let bf = geodetic_to_body_fixed(&site, &planet);
        let back = body_fixed_to_geodetic(bf, &planet);
        assert!((back.latitude_deg - site.latitude_deg).abs() < 1e-4);
        assert!((back.longitude_deg - site.longitude_deg).abs() < 1e-4);
        assert!((back.altitude_m - site.altitude_m).abs() < 0.1);
    }

    #[test]
    fn body_fixed_round_trips_through_inertial() {
        let planet = earth();
        let site = ksc();
        let bf = geodetic_to_body_fixed(&site, &planet);
        let time_days = 12.5;
        let pci = body_fixed_to_planet_inertial(bf, &planet, time_days);
        let back = planet_inertial_to_body_fixed(pci, &planet, time_days);
        assert!(
            (back - bf).length() < 1e-3,
            "round trip off by {}",
            (back - bf).length()
        );
    }

    #[test]
    fn solar_round_trips_through_planet_inertial() {
        let scale = PhysicalScale::default();
        let planet = earth();
        let site = ksc();
        let planet_solar_units = Vec3::new(75_000.0, 0.0, 0.0); // Earth at 1 AU
        let time_days = 0.0;
        let bf = geodetic_to_body_fixed(&site, &planet);
        let pci = body_fixed_to_planet_inertial(bf, &planet, time_days);
        let solar = planet_inertial_to_solar(pci, planet_solar_units, &scale);
        let pci_back = solar_to_planet_inertial(solar, planet_solar_units, &scale);
        // The solar-inertial frame is an f32 presentation frame. At 1 AU
        // (~75 000 display units) an f32 ulp is ~7 800 m in real distance, so
        // the round trip is bounded by that resolution. This is exactly why
        // dynamics run in f64 against a local origin instead.
        let f32_solar_resolution_m = 20_000.0;
        assert!(
            (pci_back - pci).length() < f32_solar_resolution_m,
            "solar round trip off by {} m",
            (pci_back - pci).length()
        );
    }

    #[test]
    fn local_tangent_round_trips() {
        let planet = earth();
        let origin = ksc();
        let site = LaunchSiteCoordinates::new(
            CelestialBodyId::earth(),
            origin.latitude_deg + 0.5,
            origin.longitude_deg + 1.0,
            origin.altitude_m + 1000.0,
        );
        let bf = geodetic_to_body_fixed(&site, &planet);
        let enu = body_fixed_to_local_tangent(bf, &origin, &planet);
        let bf_back = local_tangent_to_body_fixed(enu, &origin, &planet);
        assert!(
            (bf_back - bf).length() < 1e-3,
            "local tangent round trip off by {}",
            (bf_back - bf).length()
        );
        assert!(enu.y > 500.0, "expected positive northing, got {}", enu.y);
    }

    #[test]
    fn full_chain_round_trip_through_solar() {
        let scale = PhysicalScale::default();
        let planet = earth();
        let site = ksc();
        let planet_solar_units = Vec3::new(75_000.0, 0.0, 0.0);
        let time_days = 6.25;

        let bf = geodetic_to_body_fixed(&site, &planet);
        let pci = body_fixed_to_planet_inertial(bf, &planet, time_days);
        let solar = planet_inertial_to_solar(pci, planet_solar_units, &scale);
        let pci_back = solar_to_planet_inertial(solar, planet_solar_units, &scale);
        let bf_back = planet_inertial_to_body_fixed(pci_back, &planet, time_days);
        let back = body_fixed_to_geodetic(bf_back, &planet);

        // Tolerances reflect the f32 solar-inertial presentation frame's
        // resolution at 1 AU (~7 800 m per ulp at Earth's orbital distance).
        assert!((back.latitude_deg - site.latitude_deg).abs() < 0.1);
        assert!((back.longitude_deg - site.longitude_deg).abs() < 0.1);
        assert!((back.altitude_m - site.altitude_m).abs() < 20_000.0);
    }

    #[test]
    fn ksc_maps_consistently_to_earth_body_fixed() {
        let planet = earth();
        let site = ksc();
        let bf = geodetic_to_body_fixed(&site, &planet);

        // KSC sits at Earth's radius + 3 m altitude.
        let expected_radius_m = planet_radius_m(&planet) + 3.0;
        assert!(
            (bf.length() - expected_radius_m).abs() < 1.0,
            "radius {}",
            bf.length()
        );

        // Latitude encodes y/r = sin(lat).
        let lat_from_y = (bf.y / bf.length()).asin().to_degrees();
        assert!((lat_from_y - site.latitude_deg as f64).abs() < 1e-9);

        // Longitude encodes atan2(z, x) = lon.
        let lon_from_xz = bf.z.atan2(bf.x).to_degrees();
        assert!((lon_from_xz - site.longitude_deg as f64).abs() < 1e-9);

        // Spin + tilt move the site in inertial space but preserve its radius.
        let pci = body_fixed_to_planet_inertial(bf, &planet, 24.0);
        assert!((pci - bf).length() > 1.0, "rotation did not move the site");
        assert!(
            (pci.length() - expected_radius_m).abs() < 1.0,
            "rotation changed the radius"
        );
    }

    #[test]
    fn matches_existing_launch_site_mapping() {
        let planet = earth();
        let site = ksc();
        let existing = site.to_planet_relative_position(&planet);
        let authoritative = geodetic_to_body_fixed(&site, &planet);
        // The existing mapping computes in f32; at Earth radius its ulp is ~0.8 m.
        assert!(
            authoritative.as_vec3().distance(existing) < 2.0,
            "authoritative mapping diverges from LaunchSiteCoordinates by {} m",
            authoritative.as_vec3().distance(existing)
        );
    }

    #[test]
    fn render_boundary_avoids_f32_cancellation_at_solar_distances() {
        let scale = PhysicalScale::default();
        let planet = earth();
        let site = ksc();
        let pci =
            body_fixed_to_planet_inertial(geodetic_to_body_fixed(&site, &planet), &planet, 0.0);

        // The f64 dynamics core + local-origin rebasing preserves a 1 m change.
        let baseline = RocketPhysicalState::new(pci, DVec3::ZERO, DQuat::IDENTITY);
        let moved = RocketPhysicalState::new(
            pci + DVec3::new(1.0, 0.0, 0.0),
            DVec3::ZERO,
            DQuat::IDENTITY,
        );
        let render_delta = (moved.render_transform(DVec3::ZERO, &scale).translation
            - baseline.render_transform(DVec3::ZERO, &scale).translation)
            .length();
        assert!(
            render_delta > 0.5,
            "1 m change lost by the render boundary: {render_delta}"
        );

        // Naively adding the same offset to the planet's solar display position
        // in f32 loses the change at ~75 000 display units.
        let planet_solar_units = Vec3::new(75_000.0, 0.0, 0.0);
        let to_units = |v: DVec3| -> Vec3 {
            Vec3::new(
                scale.solar_meters_to_units(v.x) as f32,
                scale.solar_meters_to_units(v.y) as f32,
                scale.solar_meters_to_units(v.z) as f32,
            )
        };
        let naive_baseline = planet_solar_units + to_units(pci);
        let naive_moved = planet_solar_units + to_units(pci + DVec3::new(1.0, 0.0, 0.0));
        let naive_delta = (naive_moved - naive_baseline).length();
        assert!(
            naive_delta < 1e-9,
            "naive f32 should have lost the change but kept {naive_delta}"
        );
    }

    #[test]
    fn enu_basis_is_orthonormal() {
        let (east, north, up) = enu_basis(28.5721, -80.6480);
        assert!(east.dot(north).abs() < 1e-12);
        assert!(east.dot(up).abs() < 1e-12);
        assert!(north.dot(up).abs() < 1e-12);
        for v in [east, north, up] {
            assert!((v.length() - 1.0).abs() < 1e-12);
        }
    }

    #[test]
    fn earth_surface_velocity_matches_omega_cross_r() {
        let planet = earth();
        let pos_pci =
            body_fixed_to_planet_inertial(geodetic_to_body_fixed(&ksc(), &planet), &planet, 0.0);
        let velocity = surface_velocity_in_planet_inertial(pos_pci, &planet);
        let expected = std::f64::consts::TAU / (planet.rotation_period_hours as f64 * 3600.0)
            * planet_radius_m(&planet)
            * (ksc().latitude_deg as f64).to_radians().cos();
        assert!((velocity.length() - expected).abs() < 1.0);
        assert!(velocity.dot(pos_pci).abs() < 1e-5 * pos_pci.length());
    }
}
