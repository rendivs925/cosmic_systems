//! Authoritative reference-frame conversions for flight.
//!
//! Every subsystem that needs a frame conversion (gravity, aero, terrain,
//! camera) MUST use this module rather than re-implementing coordinate math
//! (AGENTS.md sections 14 and 51).
//!
//! # Frame conventions
//!
//! - **Solar-inertial display**: origin at the Sun, axes aligned with the
//!   shared DE440 snapshot and orbit-ribbon rendering frame. Its X/ecliptic
//!   north/ecliptic-Y axis ordering is left-handed, so the ICRF conversion is
//!   an improper orthogonal transform. It uses display units.
//! - **Solar-inertial physical**: the same origin and axes, expressed as f64
//!   meters and meters-per-second. Primary-body ephemerides cross into this
//!   frame directly from ICRF/J2000 before any vehicle force model consumes
//!   them.
//! - **Planet-centered inertial**: origin at a planet's center, axes parallel
//!   to solar-inertial, real meters (f64).
//! - **Terrain body-fixed**: +Y toward geographic north, +X toward lon 0, and
//!   +Z toward lon +90°. An explicit frame conversion maps it to IAU (+Z north,
//!   +Y east) before applying a kernel orientation.
//! - **IAU body-fixed**: supplied by [`BodyOrientation`] in ICRF/J2000. It is
//!   the authoritative physical orientation for high-fidelity consumers.
//! - **Local tangent**: East-North-Up (ENU) triad at a geodetic reference
//!   point, real meters.
//! - **Rocket-body**: the vehicle's own orientation (`DQuat`).
//!
//! Positions in solar-inertial are in the visualization's display units;
//! positions in every other frame are in real meters. The meter ↔ display-unit
//! mapping flows exclusively through [`PhysicalScale`].

use crate::domain::entities::planet::Planet;
use crate::domain::services::body_orientation::BodyOrientation;
#[cfg(test)]
use crate::domain::services::ephemeris::TdbEpoch;
use crate::domain::services::ephemeris::{BodyState, NaifBodyId};
use crate::domain::services::physics_utils::calculate_planet_rotation_f64;
use crate::domain::value_objects::celestial_body_id::CelestialBodyId;
use crate::domain::value_objects::launch_site_coordinates::LaunchSiteCoordinates;
use crate::domain::value_objects::physical_scale::{PhysicalScale, AU_IN_METERS};
use bevy::math::{DMat3, DQuat, DVec3, Vec3};

/// The frames supported by the reference-frame module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReferenceFrame {
    SolarSystemBarycentric,
    SolarInertial,
    PlanetCenteredInertial,
    PlanetBodyFixed,
    LocalTangent,
    RocketBody,
}

/// Reasons two scientific body states cannot form a physical relative state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelativeStateError {
    NonBarycentricInput,
    EpochMismatch,
}

/// Derive a target state relative to another body from two SSB-centered states.
/// Position and velocity are subtracted at one exact TDB epoch, preserving the
/// inertial axes and f64 SI units required by flight and rendering boundaries.
pub fn barycentric_to_relative_state(
    target_barycentric: BodyState,
    center_barycentric: BodyState,
) -> Result<BodyState, RelativeStateError> {
    if target_barycentric.center != NaifBodyId::SOLAR_SYSTEM_BARYCENTER
        || center_barycentric.center != NaifBodyId::SOLAR_SYSTEM_BARYCENTER
    {
        return Err(RelativeStateError::NonBarycentricInput);
    }
    if target_barycentric.epoch != center_barycentric.epoch {
        return Err(RelativeStateError::EpochMismatch);
    }

    Ok(BodyState {
        target: target_barycentric.target,
        center: center_barycentric.target,
        epoch: target_barycentric.epoch,
        position_m: target_barycentric.position_m - center_barycentric.position_m,
        velocity_mps: target_barycentric.velocity_mps - center_barycentric.velocity_mps,
    })
}

/// J2000 mean obliquity, in radians. This is the fixed ICRF-equatorial to
/// J2000-ecliptic rotation used by the existing solar-map/flight inertial
/// convention, whose axes are +X, +ecliptic-north, +ecliptic-Y.
const J2000_OBLIQUITY_RAD: f64 = 84_381.448_f64.to_radians() / 3_600.0;

/// Transform an ICRF/J2000 vector into the project's solar-inertial axes.
///
/// The source is right-handed ICRF equatorial (+X, +Y, +Z). The destination
/// orders its axes as (+X, +ecliptic north, +ecliptic Y); the final axis swap
/// makes this an improper orthogonal transform with determinant -1, not a
/// rotation. This function is intentionally unit-preserving and applies
/// equally to polar vectors such as positions and velocities.
pub fn icrf_j2000_to_solar_inertial(vector_icrf: DVec3) -> DVec3 {
    let sin_obliquity = J2000_OBLIQUITY_RAD.sin();
    let cos_obliquity = J2000_OBLIQUITY_RAD.cos();
    let ecliptic_y = cos_obliquity * vector_icrf.y + sin_obliquity * vector_icrf.z;
    let ecliptic_z = -sin_obliquity * vector_icrf.y + cos_obliquity * vector_icrf.z;
    DVec3::new(vector_icrf.x, ecliptic_z, ecliptic_y)
}

/// Transform the project's reflected solar-inertial axes back into ICRF/J2000.
/// This is the inverse of [`icrf_j2000_to_solar_inertial`].
pub fn solar_inertial_to_icrf_j2000(vector_solar: DVec3) -> DVec3 {
    let sin_obliquity = J2000_OBLIQUITY_RAD.sin();
    let cos_obliquity = J2000_OBLIQUITY_RAD.cos();
    let icrf_y = -sin_obliquity * vector_solar.y + cos_obliquity * vector_solar.z;
    let icrf_z = cos_obliquity * vector_solar.y + sin_obliquity * vector_solar.z;
    DVec3::new(vector_solar.x, icrf_y, icrf_z)
}

/// Derive a target's heliocentric state in the project's solar-inertial axes
/// from two same-epoch SSB ICRF states. Positions remain meters and velocities
/// remain meters per second.
pub fn barycentric_to_solar_inertial_state(
    target_barycentric: BodyState,
    sun_barycentric: BodyState,
) -> Result<BodyState, RelativeStateError> {
    let relative = barycentric_to_relative_state(target_barycentric, sun_barycentric)?;
    Ok(BodyState {
        position_m: icrf_j2000_to_solar_inertial(relative.position_m),
        velocity_mps: icrf_j2000_to_solar_inertial(relative.velocity_mps),
        ..relative
    })
}

/// Planet radius in meters for a [`Planet`].
pub fn planet_radius_m(planet: &Planet) -> f64 {
    planet.radius_km as f64 * 1000.0
}

/// WGS-84 semi-major axis, in meters.
pub const WGS84_SEMI_MAJOR_AXIS_M: f64 = 6_378_137.0;
/// WGS-84 inverse flattening.
pub const WGS84_INVERSE_FLATTENING: f64 = 298.257_223_563;

/// Geodetic datum selected for a body's fixed surface coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GeodeticDatum {
    Wgs84,
    Spherical { radius_m: f64 },
}

/// Earth uses WGS-84. Bodies without an explicitly approved ellipsoid retain
/// their catalog-radius spherical model.
pub fn geodetic_datum(planet: &Planet) -> GeodeticDatum {
    if planet.name == "Earth" {
        GeodeticDatum::Wgs84
    } else {
        GeodeticDatum::Spherical {
            radius_m: planet_radius_m(planet),
        }
    }
}

// ---------------------------------------------------------------------------
// Geodetic (lat/lon/alt) <-> planet body-fixed
// ---------------------------------------------------------------------------

/// Map geodetic latitude/longitude/ellipsoidal height to the terrain
/// body-fixed axes in meters. Earth uses WGS-84; all other bodies use their
/// explicit spherical datum.
pub fn geodetic_to_body_fixed(site: &LaunchSiteCoordinates, planet: &Planet) -> DVec3 {
    let lat_rad = (site.latitude_deg as f64).to_radians();
    let lon_rad = (site.longitude_deg as f64).to_radians();
    let height_m = site.altitude_m as f64;

    match geodetic_datum(planet) {
        GeodeticDatum::Wgs84 => {
            let flattening = 1.0 / WGS84_INVERSE_FLATTENING;
            let eccentricity_squared = flattening * (2.0 - flattening);
            let prime_vertical_radius_m = WGS84_SEMI_MAJOR_AXIS_M
                / (1.0 - eccentricity_squared * lat_rad.sin().powi(2)).sqrt();
            let x = (prime_vertical_radius_m + height_m) * lat_rad.cos() * lon_rad.cos();
            let east = (prime_vertical_radius_m + height_m) * lat_rad.cos() * lon_rad.sin();
            let north =
                (prime_vertical_radius_m * (1.0 - eccentricity_squared) + height_m) * lat_rad.sin();
            // Terrain's established axes are x/lon0, y/north, z/east.
            DVec3::new(x, north, east)
        }
        GeodeticDatum::Spherical { radius_m } => {
            let radius_with_height_m = radius_m + height_m;
            DVec3::new(
                radius_with_height_m * lat_rad.cos() * lon_rad.cos(),
                radius_with_height_m * lat_rad.sin(),
                radius_with_height_m * lat_rad.cos() * lon_rad.sin(),
            )
        }
    }
}

/// Convert a geodetic surface location to the radial latitude/longitude used by
/// the cube-sphere terrain source. Terrain heights are measured above a mean
/// radial sphere, while Earth launch sites are expressed on the WGS-84
/// ellipsoid, so their latitudes are not interchangeable.
///
/// The geodetic altitude is intentionally ignored: a terrain coordinate names
/// a point on the body's reference surface, not a point above it.
pub fn geodetic_to_terrain_lat_lon(site: &LaunchSiteCoordinates, planet: &Planet) -> (f64, f64) {
    let surface_site = LaunchSiteCoordinates::new(
        site.planet_id.clone(),
        site.latitude_deg,
        site.longitude_deg,
        0.0,
    );
    body_fixed_to_terrain_lat_lon(geodetic_to_body_fixed(&surface_site, planet))
}

/// Convert a terrain body-fixed direction or position to the radial
/// latitude/longitude consumed by the cube-sphere terrain source.
pub fn body_fixed_to_terrain_lat_lon(body_fixed: DVec3) -> (f64, f64) {
    let direction = body_fixed.normalize_or_zero();
    (
        direction.y.clamp(-1.0, 1.0).asin().to_degrees(),
        direction.z.atan2(direction.x).to_degrees(),
    )
}

/// Map a terrain body-fixed position (meters) back to its body's
/// documented geodetic datum.
pub fn body_fixed_to_geodetic(pos_bf: DVec3, planet: &Planet) -> LaunchSiteCoordinates {
    let (lat_rad, lon_rad, height_m) = match geodetic_datum(planet) {
        GeodeticDatum::Wgs84 => {
            let x = pos_bf.x;
            let east = pos_bf.z;
            let north = pos_bf.y;
            let semi_minor_axis_m =
                WGS84_SEMI_MAJOR_AXIS_M * (1.0 - 1.0 / WGS84_INVERSE_FLATTENING);
            let eccentricity_squared = 1.0
                - (semi_minor_axis_m * semi_minor_axis_m)
                    / (WGS84_SEMI_MAJOR_AXIS_M * WGS84_SEMI_MAJOR_AXIS_M);
            let second_eccentricity_squared = (WGS84_SEMI_MAJOR_AXIS_M.powi(2)
                - semi_minor_axis_m.powi(2))
                / semi_minor_axis_m.powi(2);
            let horizontal_m = x.hypot(east);
            if horizontal_m < 1.0e-9 {
                (
                    north.signum() * std::f64::consts::FRAC_PI_2,
                    0.0,
                    north.abs() - semi_minor_axis_m,
                )
            } else {
                let theta =
                    (north * WGS84_SEMI_MAJOR_AXIS_M).atan2(horizontal_m * semi_minor_axis_m);
                let lat_rad = (north
                    + second_eccentricity_squared * semi_minor_axis_m * theta.sin().powi(3))
                .atan2(
                    horizontal_m
                        - eccentricity_squared * WGS84_SEMI_MAJOR_AXIS_M * theta.cos().powi(3),
                );
                let prime_vertical_radius_m = WGS84_SEMI_MAJOR_AXIS_M
                    / (1.0 - eccentricity_squared * lat_rad.sin().powi(2)).sqrt();
                (
                    lat_rad,
                    east.atan2(x),
                    horizontal_m / lat_rad.cos() - prime_vertical_radius_m,
                )
            }
        }
        GeodeticDatum::Spherical { radius_m } => {
            let radius_with_height_m = pos_bf.length();
            (
                (pos_bf.y / radius_with_height_m).clamp(-1.0, 1.0).asin(),
                pos_bf.z.atan2(pos_bf.x),
                radius_with_height_m - radius_m,
            )
        }
    };
    LaunchSiteCoordinates::new(
        CelestialBodyId::new(planet.name.clone())
            .expect("planet names are validated at catalog construction"),
        lat_rad.to_degrees() as f32,
        lon_rad.to_degrees() as f32,
        height_m as f32,
    )
}

// ---------------------------------------------------------------------------
// Planet body-fixed <-> planet-centered inertial
// ---------------------------------------------------------------------------

/// Convert the terrain/geodetic body-fixed convention (+Y north, +Z east) to
/// IAU body-fixed axes (+Z north, +Y east).
pub fn terrain_body_fixed_to_iau_body_fixed(pos_terrain_bf: DVec3) -> DVec3 {
    DVec3::new(pos_terrain_bf.x, pos_terrain_bf.z, pos_terrain_bf.y)
}

/// Convert IAU body-fixed axes (+Z north, +Y east) to the terrain and
/// geodetic convention (+Y north, +Z east).
pub fn iau_body_fixed_to_terrain_body_fixed(pos_iau_bf: DVec3) -> DVec3 {
    DVec3::new(pos_iau_bf.x, pos_iau_bf.z, pos_iau_bf.y)
}

/// Convert a terrain body-fixed position to the project planet-centered
/// inertial frame through the shared IAU orientation snapshot.
pub fn body_fixed_to_planet_inertial(
    pos_terrain_bf: DVec3,
    orientation: &BodyOrientation,
) -> DVec3 {
    let pos_icrf =
        orientation.body_fixed_to_inertial * terrain_body_fixed_to_iau_body_fixed(pos_terrain_bf);
    icrf_j2000_to_solar_inertial(pos_icrf)
}

/// Convert a project planet-centered inertial position to the terrain
/// body-fixed convention through the shared IAU orientation snapshot.
pub fn planet_inertial_to_body_fixed(pos_pci: DVec3, orientation: &BodyOrientation) -> DVec3 {
    let pos_iau_bf = orientation.inertial_to_body_fixed * solar_inertial_to_icrf_j2000(pos_pci);
    iau_body_fixed_to_terrain_body_fixed(pos_iau_bf)
}

/// Rotation from terrain body-fixed axes into project planet-centered
/// inertial axes. IAU and solar mappings both reflect handedness, so their
/// composition is a proper rotation suitable for `DQuat` presentation use.
pub fn body_fixed_to_planet_inertial_rotation(orientation: &BodyOrientation) -> DQuat {
    let x_axis = body_fixed_to_planet_inertial(DVec3::X, orientation);
    let y_axis = body_fixed_to_planet_inertial(DVec3::Y, orientation);
    let z_axis = body_fixed_to_planet_inertial(DVec3::Z, orientation);
    DQuat::from_mat3(&DMat3::from_cols(x_axis, y_axis, z_axis))
}

/// Velocity of a point fixed to the rotating planetary surface, expressed in
/// project planet-centered inertial axes. The reflected solar mapping requires
/// negating the transformed ICRF angular-velocity pseudovector.
pub fn surface_velocity_in_planet_inertial(pos_pci: DVec3, orientation: &BodyOrientation) -> DVec3 {
    let angular_velocity_solar =
        -icrf_j2000_to_solar_inertial(orientation.angular_velocity_inertial_rad_s);
    angular_velocity_solar.cross(pos_pci)
}

/// Unit normal of the planet's equatorial plane in project planet-centered
/// inertial coordinates. Orbital inclination for local flight is measured from
/// this axis, not from the solar rendering frame's +Z axis.
pub fn planet_inertial_spin_axis(orientation: &BodyOrientation) -> DVec3 {
    body_fixed_to_planet_inertial_rotation(orientation) * DVec3::Y
}

/// Inertial reference direction for right ascension within a planet's
/// equatorial plane. Together with [`planet_inertial_spin_axis`], this defines
/// the local orbital-element reference frame.
pub fn planet_equatorial_reference_x_axis(orientation: &BodyOrientation) -> DVec3 {
    body_fixed_to_planet_inertial_rotation(orientation) * DVec3::X
}

/// Local East-North-Up axes at a planet-centered inertial position.
///
/// The axes are expressed in the project's planet-centered inertial frame:
/// `up` is radial, `east` follows the rotating surface, and `north` points
/// toward the inertial spin axis. This is the sole local-horizontal basis for
/// flight guidance; it intentionally does not use render or body-frame state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlanetInertialEnuBasis {
    pub east: DVec3,
    pub north: DVec3,
    pub up: DVec3,
}

/// Reasons a local planet-inertial ENU basis cannot be constructed safely.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanetInertialEnuError {
    InvalidPosition,
    InvalidSpinAxis,
    PolarPosition,
}

/// Construct the planet-inertial ENU-like basis at `position_m`.
///
/// The solar-inertial axes are reflected relative to ICRF, so `up × spin`
/// gives the eastward direction used by [`surface_velocity_in_planet_inertial`].
/// At a pole east/north are undefined and this returns an explicit error rather
/// than selecting an arbitrary horizontal direction.
pub fn planet_inertial_enu_basis(
    position_m: DVec3,
    spin_axis: DVec3,
) -> Result<PlanetInertialEnuBasis, PlanetInertialEnuError> {
    if !position_m.is_finite() || position_m.length_squared() <= 1.0e-24 {
        return Err(PlanetInertialEnuError::InvalidPosition);
    }
    if !spin_axis.is_finite() || spin_axis.length_squared() <= 1.0e-24 {
        return Err(PlanetInertialEnuError::InvalidSpinAxis);
    }

    let up = position_m.normalize();
    let spin_axis = spin_axis.normalize();
    let east_unnormalized = up.cross(spin_axis);
    if east_unnormalized.length_squared() <= 1.0e-24 {
        return Err(PlanetInertialEnuError::PolarPosition);
    }
    let east = east_unnormalized.normalize();
    let north = east.cross(up).normalize();

    Ok(PlanetInertialEnuBasis { east, north, up })
}

/// Catalog spin approximation for presentation-only consumers that need
/// arbitrary historical or predicted epochs unavailable in the shared snapshot.
pub fn catalog_body_fixed_to_inertial_rotation(planet: &Planet, time_days: f64) -> DQuat {
    let spin_rad = calculate_planet_rotation_f64(planet, time_days);
    let tilt_rad = planet.axial_tilt_deg as f64;
    DQuat::from_rotation_z(tilt_rad.to_radians()) * DQuat::from_rotation_y(spin_rad)
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

/// Convert a heliocentric J2000 ecliptic position from astronomical units to
/// the physical solar-inertial frame in meters. The application already maps
/// the ecliptic plane to X/Z and the north ecliptic pole to +Y, so no axis
/// rotation is required at this unit boundary.
pub fn heliocentric_au_to_solar_inertial_m(position_au: DVec3) -> DVec3 {
    position_au * AU_IN_METERS
}

/// Convert a heliocentric J2000 ecliptic velocity from AU/day to the physical
/// solar-inertial frame in meters per second.
pub fn heliocentric_au_per_day_to_solar_inertial_mps(velocity_au_per_day: DVec3) -> DVec3 {
    velocity_au_per_day * (AU_IN_METERS / 86_400.0)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::ephemeris::SpiceEphemeris;
    use crate::domain::services::planet_factory::PlanetFactory;
    use crate::domain::services::rocket_dynamics::RocketDynamicsState;
    use crate::domain::value_objects::launch_site_coordinates::predefined_sites;

    fn earth() -> Planet {
        PlanetFactory::create_by_name("Earth").unwrap()
    }

    fn ksc() -> LaunchSiteCoordinates {
        predefined_sites::kennedy_space_center()
    }

    fn test_orientation() -> BodyOrientation {
        BodyOrientation::from_kernel(
            NaifBodyId::EARTH,
            TdbEpoch::j2000(),
            "test-orientation".to_string(),
            DQuat::IDENTITY,
            DVec3::Z * (std::f64::consts::TAU / (23.934 * 3_600.0)),
        )
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
        let orientation = test_orientation();
        let site = ksc();
        let bf = geodetic_to_body_fixed(&site, &planet);
        let pci = body_fixed_to_planet_inertial(bf, &orientation);
        let back = planet_inertial_to_body_fixed(pci, &orientation);
        assert!(
            (back - bf).length() < 1e-3,
            "round trip off by {}",
            (back - bf).length()
        );
    }

    #[test]
    fn icrf_j2000_rotation_preserves_length_and_maps_equatorial_y() {
        let converted = icrf_j2000_to_solar_inertial(DVec3::Y);

        assert!((converted.length() - 1.0).abs() < 1.0e-15);
        assert!(converted.x.abs() < 1.0e-15);
        assert!(converted.y < 0.0);
        assert!(converted.z > 0.0);
    }

    #[test]
    fn icrf_to_solar_inertial_is_an_improper_transform() {
        let x = icrf_j2000_to_solar_inertial(DVec3::X);
        let y = icrf_j2000_to_solar_inertial(DVec3::Y);
        let z = icrf_j2000_to_solar_inertial(DVec3::Z);
        let transform = DMat3::from_cols(x, y, z);

        assert!((transform.determinant() + 1.0).abs() < 1.0e-15);
        assert!((x.cross(y) + z).length() < 1.0e-15);
    }

    #[test]
    fn orientation_reference_axes_follow_iau_pole() {
        let orientation = test_orientation();
        let spin_axis = planet_inertial_spin_axis(&orientation);
        let reference_x = planet_equatorial_reference_x_axis(&orientation);

        assert!((spin_axis.length() - 1.0).abs() < 1e-12);
        assert!((reference_x.length() - 1.0).abs() < 1e-12);
        assert!(spin_axis.dot(reference_x).abs() < 1e-12);
        assert!((spin_axis - icrf_j2000_to_solar_inertial(DVec3::Z)).length() < 1e-12);
    }

    #[test]
    fn solar_round_trips_through_planet_inertial() {
        let scale = PhysicalScale::default();
        let planet = earth();
        let orientation = test_orientation();
        let site = ksc();
        let planet_solar_units = Vec3::new(75_000.0, 0.0, 0.0); // Earth at 1 AU
        let bf = geodetic_to_body_fixed(&site, &planet);
        let pci = body_fixed_to_planet_inertial(bf, &orientation);
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
    fn heliocentric_ephemeris_units_convert_to_physical_solar_inertial() {
        let position_m = heliocentric_au_to_solar_inertial_m(DVec3::new(1.0, -2.0, 0.5));
        let velocity_mps = heliocentric_au_per_day_to_solar_inertial_mps(DVec3::X);

        assert_eq!(
            position_m,
            DVec3::new(AU_IN_METERS, -2.0 * AU_IN_METERS, 0.5 * AU_IN_METERS)
        );
        assert!((velocity_mps.x - AU_IN_METERS / 86_400.0).abs() < 1e-9);
        assert_eq!(velocity_mps.y, 0.0);
        assert_eq!(velocity_mps.z, 0.0);
    }

    #[test]
    fn barycentric_states_derive_a_same_epoch_relative_state() {
        let epoch = TdbEpoch::j2000();
        let target = BodyState {
            target: NaifBodyId::EARTH,
            center: NaifBodyId::SOLAR_SYSTEM_BARYCENTER,
            epoch,
            position_m: DVec3::new(10.0, -5.0, 2.0),
            velocity_mps: DVec3::new(3.0, 4.0, -2.0),
        };
        let center = BodyState {
            target: NaifBodyId::SUN,
            center: NaifBodyId::SOLAR_SYSTEM_BARYCENTER,
            epoch,
            position_m: DVec3::new(1.0, -2.0, 8.0),
            velocity_mps: DVec3::new(1.0, -1.0, 0.5),
        };

        let relative = barycentric_to_relative_state(target, center).unwrap();
        assert_eq!(relative.center, NaifBodyId::SUN);
        assert_eq!(relative.position_m, DVec3::new(9.0, -3.0, -6.0));
        assert_eq!(relative.velocity_mps, DVec3::new(2.0, 5.0, -2.5));
    }

    #[test]
    fn third_body_states_subtract_at_one_epoch_before_flight_frame_rotation() {
        let epoch = TdbEpoch::j2000();
        let earth = BodyState {
            target: NaifBodyId::EARTH,
            center: NaifBodyId::SOLAR_SYSTEM_BARYCENTER,
            epoch,
            position_m: DVec3::new(-2.0e10, 1.2e11, 5.1e10),
            velocity_mps: DVec3::new(-29_000.0, -5_000.0, -2_000.0),
        };
        let moon = BodyState {
            target: NaifBodyId::MOON,
            center: NaifBodyId::SOLAR_SYSTEM_BARYCENTER,
            epoch,
            position_m: earth.position_m + DVec3::new(3.8e8, -8.0e7, 1.2e8),
            velocity_mps: earth.velocity_mps + DVec3::new(400.0, 800.0, -250.0),
        };
        let sun = BodyState {
            target: NaifBodyId::SUN,
            center: NaifBodyId::SOLAR_SYSTEM_BARYCENTER,
            epoch,
            position_m: DVec3::new(2.0e8, -4.0e8, 1.0e8),
            velocity_mps: DVec3::new(12.0, -8.0, 3.0),
        };

        for third_body in [moon, sun] {
            let relative = barycentric_to_solar_inertial_state(third_body, earth).unwrap();

            assert_eq!(relative.center, NaifBodyId::EARTH);
            assert_eq!(relative.epoch, epoch);
            assert_eq!(
                relative.position_m,
                icrf_j2000_to_solar_inertial(third_body.position_m - earth.position_m)
            );
            assert_eq!(
                relative.velocity_mps,
                icrf_j2000_to_solar_inertial(third_body.velocity_mps - earth.velocity_mps)
            );
        }
    }

    #[test]
    fn relative_state_rejects_mixed_epochs() {
        let target = BodyState {
            target: NaifBodyId::EARTH,
            center: NaifBodyId::SOLAR_SYSTEM_BARYCENTER,
            epoch: TdbEpoch::j2000(),
            position_m: DVec3::ZERO,
            velocity_mps: DVec3::ZERO,
        };
        let center = BodyState {
            target: NaifBodyId::SUN,
            center: NaifBodyId::SOLAR_SYSTEM_BARYCENTER,
            epoch: TdbEpoch::from_seconds_since_j2000(1.0).unwrap(),
            position_m: DVec3::ZERO,
            velocity_mps: DVec3::ZERO,
        };

        assert_eq!(
            barycentric_to_relative_state(target, center),
            Err(RelativeStateError::EpochMismatch)
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
        let orientation = test_orientation();
        let site = ksc();
        let planet_solar_units = Vec3::new(75_000.0, 0.0, 0.0);
        let bf = geodetic_to_body_fixed(&site, &planet);
        let pci = body_fixed_to_planet_inertial(bf, &orientation);
        let solar = planet_inertial_to_solar(pci, planet_solar_units, &scale);
        let pci_back = solar_to_planet_inertial(solar, planet_solar_units, &scale);
        let bf_back = planet_inertial_to_body_fixed(pci_back, &orientation);
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
        let orientation = test_orientation();
        let site = ksc();
        let bf = geodetic_to_body_fixed(&site, &planet);

        let flattening = 1.0 / WGS84_INVERSE_FLATTENING;
        let semi_minor_axis_m = WGS84_SEMI_MAJOR_AXIS_M * (1.0 - flattening);
        assert!(bf.length() > semi_minor_axis_m, "radius {}", bf.length());
        assert!(bf.length() < WGS84_SEMI_MAJOR_AXIS_M + 10.0);

        // The WGS-84 inverse returns the original geodetic latitude and longitude.
        let back = body_fixed_to_geodetic(bf, &planet);
        assert!((back.latitude_deg - site.latitude_deg).abs() < 1e-4);
        assert!((back.longitude_deg - site.longitude_deg).abs() < 1e-4);

        // Kernel orientation moves the site in inertial space but preserves its radius.
        let pci = body_fixed_to_planet_inertial(bf, &orientation);
        assert!((pci - bf).length() > 1.0, "rotation did not move the site");
        assert!(
            (pci.length() - bf.length()).abs() < 1.0,
            "rotation changed the radius"
        );
    }

    #[test]
    fn earth_uses_wgs84_axes_and_polar_radius() {
        let planet = earth();
        let equator = LaunchSiteCoordinates::new(CelestialBodyId::earth(), 0.0, 0.0, 0.0);
        let north_pole = LaunchSiteCoordinates::new(CelestialBodyId::earth(), 90.0, 0.0, 0.0);

        assert_eq!(geodetic_datum(&planet), GeodeticDatum::Wgs84);
        assert!(
            (geodetic_to_body_fixed(&equator, &planet).x - WGS84_SEMI_MAJOR_AXIS_M).abs() < 1e-6
        );
        assert!(
            (geodetic_to_body_fixed(&north_pole, &planet).y
                - WGS84_SEMI_MAJOR_AXIS_M * (1.0 - 1.0 / WGS84_INVERSE_FLATTENING))
                .abs()
                < 1e-6
        );
    }

    #[test]
    fn wgs84_launch_sites_map_to_the_terrain_radial_coordinates() {
        let planet = earth();
        let surface_site = ksc();
        let elevated_site = LaunchSiteCoordinates::new(
            CelestialBodyId::earth(),
            surface_site.latitude_deg,
            surface_site.longitude_deg,
            1_000.0,
        );

        let surface_coordinates = geodetic_to_terrain_lat_lon(&surface_site, &planet);
        let elevated_coordinates = geodetic_to_terrain_lat_lon(&elevated_site, &planet);

        assert!(
            (surface_coordinates.0 - surface_site.latitude_deg as f64).abs() > 0.1,
            "WGS-84 geodetic latitude must not be used directly for radial terrain sampling"
        );
        assert!((surface_coordinates.1 - surface_site.longitude_deg as f64).abs() < 1e-9);
        assert_eq!(surface_coordinates, elevated_coordinates);
        let datum_surface_site = LaunchSiteCoordinates::new(
            surface_site.planet_id.clone(),
            surface_site.latitude_deg,
            surface_site.longitude_deg,
            0.0,
        );
        assert_eq!(
            surface_coordinates,
            body_fixed_to_terrain_lat_lon(geodetic_to_body_fixed(&datum_surface_site, &planet))
        );
    }

    #[test]
    fn non_earth_bodies_retain_explicit_spherical_geodesy() {
        let mars = PlanetFactory::create_by_name("Mars").unwrap();
        let site = LaunchSiteCoordinates::new(
            CelestialBodyId::new("Mars".to_string()).unwrap(),
            30.0,
            45.0,
            100.0,
        );
        let position = geodetic_to_body_fixed(&site, &mars);

        assert_eq!(
            geodetic_datum(&mars),
            GeodeticDatum::Spherical {
                radius_m: planet_radius_m(&mars)
            }
        );
        assert!((position.length() - (planet_radius_m(&mars) + 100.0)).abs() < 1e-6);
        let back = body_fixed_to_geodetic(position, &mars);
        assert!((back.latitude_deg - site.latitude_deg).abs() < 1e-4);
        assert!((back.longitude_deg - site.longitude_deg).abs() < 1e-4);
        assert!((back.altitude_m - site.altitude_m).abs() < 1e-3);
    }

    #[test]
    fn render_boundary_avoids_f32_cancellation_at_solar_distances() {
        let scale = PhysicalScale::default();
        let planet = earth();
        let orientation = test_orientation();
        let site = ksc();
        let pci =
            body_fixed_to_planet_inertial(geodetic_to_body_fixed(&site, &planet), &orientation);

        // The f64 dynamics core + local-origin rebasing preserves a 1 m change.
        let baseline = RocketDynamicsState::new(
            pci,
            DVec3::ZERO,
            DQuat::IDENTITY,
            1.0,
            DMat3::IDENTITY,
            DVec3::ZERO,
        );
        let moved = RocketDynamicsState::new(
            pci + DVec3::new(1.0, 0.0, 0.0),
            DVec3::ZERO,
            DQuat::IDENTITY,
            1.0,
            DMat3::IDENTITY,
            DVec3::ZERO,
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
        let orientation = test_orientation();
        let pos_pci =
            body_fixed_to_planet_inertial(geodetic_to_body_fixed(&ksc(), &planet), &orientation);
        let velocity = surface_velocity_in_planet_inertial(pos_pci, &orientation);
        let expected = orientation.angular_velocity_inertial_rad_s.length()
            * planet_radius_m(&planet)
            * (ksc().latitude_deg as f64).to_radians().cos();
        assert!((velocity.length() - expected).abs() < 1.0);
        assert!(velocity.dot(pos_pci).abs() < 1e-5 * pos_pci.length());
    }

    #[test]
    fn terrain_axes_cross_the_explicit_iau_boundary() {
        let planet = earth();
        let site = ksc();
        let terrain = geodetic_to_body_fixed(&site, &planet);
        let iau = terrain_body_fixed_to_iau_body_fixed(terrain);

        assert_eq!(iau, DVec3::new(terrain.x, terrain.z, terrain.y));
        assert_eq!(iau_body_fixed_to_terrain_body_fixed(iau), terrain);
    }

    #[test]
    fn pck_de440_j2000_preserves_earth_orientation_launch_site_and_sun_azimuth() {
        // Recorded from the manifest-pinned NAIF pck00011.tpc and DE440s.bsp
        // kernels at JD TDB 2451545.0. Task 7 will add independently generated
        // external reference cases; this guards the shared frame boundary now.
        let ephemeris = SpiceEphemeris::load("assets/configs/ephemeris/de440.ron").unwrap();
        let epoch = TdbEpoch::j2000();
        let orientation = ephemeris.orientation(NaifBodyId::EARTH, epoch).unwrap();
        let earth_state = ephemeris
            .state(
                NaifBodyId::EARTH,
                NaifBodyId::SOLAR_SYSTEM_BARYCENTER,
                epoch,
            )
            .unwrap();
        let sun_state = ephemeris
            .state(NaifBodyId::SUN, NaifBodyId::SOLAR_SYSTEM_BARYCENTER, epoch)
            .unwrap();
        let planet = earth();
        let site = ksc();
        let site_bf = geodetic_to_body_fixed(&site, &planet);
        let site_pci = body_fixed_to_planet_inertial(site_bf, &orientation);
        let sun_pci = icrf_j2000_to_solar_inertial(sun_state.position_m - earth_state.position_m);
        let sun_bf = planet_inertial_to_body_fixed(sun_pci.normalize(), &orientation);
        let (east, north, up) = enu_basis(site.latitude_deg, site.longitude_deg);
        let azimuth_deg = east
            .dot(sun_bf)
            .atan2(north.dot(sun_bf))
            .to_degrees()
            .rem_euclid(360.0);
        let altitude_deg = up.dot(sun_bf).asin().to_degrees();
        let prime_meridian_icrf = orientation.body_fixed_to_inertial * DVec3::X;
        let prime_meridian_angle_deg = prime_meridian_icrf
            .y
            .atan2(prime_meridian_icrf.x)
            .to_degrees()
            .rem_euclid(360.0);

        let expected_orientation = DQuat::from_xyzw(
            -1.068_819_057_268_670_1e-15,
            1.277_092_378_309_780_7e-15,
            0.641_804_414_405_553,
            0.766_868_367_876_485_9,
        );
        assert!(
            orientation
                .inertial_to_body_fixed
                .dot(expected_orientation)
                .abs()
                > 1.0 - 1.0e-14
        );
        assert!((prime_meridian_angle_deg - 280.146_995_789_738).abs() < 1.0e-9);

        assert!(
            site_bf.distance(DVec3::new(
                910_919.022_122_250_5,
                3_032_338.193_143_711,
                -5_531_170.882_283_327,
            )) < 1.0e-6
        );
        assert!(
            site_pci.distance(DVec3::new(
                -5_284_177.461_813_185,
                3_526_405.040_215_295_7,
                -510_524.980_672_725,
            )) < 1.0e-6
        );
        assert!((azimuth_deg - 114.049_370_418_472).abs() < 1.0e-9);
        assert!((altitude_deg - -4.111_889_453_097).abs() < 1.0e-9);

        let round_trip_bf = planet_inertial_to_body_fixed(site_pci, &orientation);
        assert!(round_trip_bf.distance(site_bf) < 1.0e-6);
        let round_trip_site = body_fixed_to_geodetic(round_trip_bf, &planet);
        assert!((round_trip_site.latitude_deg - site.latitude_deg).abs() < 1.0e-4);
        assert!((round_trip_site.longitude_deg - site.longitude_deg).abs() < 1e-4);
        assert!((round_trip_site.altitude_m - site.altitude_m).abs() < 0.1);
    }
}
