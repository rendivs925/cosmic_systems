---
name: reference-frames-and-precision
description: Use when changing coordinates, units, SPICE/DE ephemeris frames, planet rotation, launch-site mapping, camera-relative rendering, orbital state, terrain positions, or f64/f32 boundaries in Cosmic Systems.
---

# Reference Frames And Precision

Use this skill for every change that crosses coordinate systems or converts
between simulation and rendering precision.

## Single Authority

`src/domain/services/reference_frames.rs` is the one authoritative physical
conversion module. The solar ephemeris owns body states; this module owns the
f64/SI conversions between those states and local flight frames. Read both
before writing coordinate maths. Reuse:

- geodetic <-> planet body-fixed conversions;
- body-fixed <-> planet-centered inertial rotations;
- local ENU tangent frames;
- planet-centered inertial <-> solar-inertial conversions.

`src/infrastructure/bevy_adapters/reference_frames.rs` owns solar-map f32
display conversion. `PhysicalScale`, `RenderOrigin`, and rocket camera/planet
adapters own the wider simulation-to-presentation boundary. Do not create
parallel coordinate helpers or a second floating-origin implementation.

## Defined Frames

```text
solar-system barycentric:   ICRF/J2000, Solar System Barycenter, f64 SI
heliocentric display:       derived presentation frame, display units only
planet-centered inertial:   planet origin, axes parallel to ICRF/J2000, f64 SI
planet body-fixed:          rotating planet coordinates, real meters, f64
local tangent:              East-North-Up at geodetic reference, real meters
rocket body:                rocket-local orientation, DQuat
render:                     camera-relative Bevy Vec3/Transform only
```

- Body-fixed axes are +Y north, +X longitude 0, +Z longitude +90 degrees.
- Body-fixed -> inertial applies planetary spin about +Y then axial tilt about +Z.
- A SPICE/DE state is barycentric ICRF/J2000 at TDB. Do not relabel it as
  heliocentric or use display units as an intermediary.
- Heliocentric positions, planet-centered states, and render positions are
  derived from authoritative barycentric states; they are never separate state
  authorities.
- The meter/display conversion flows exclusively through `PhysicalScale`.

## Precision Rules

- Use `f64`, `DVec3`, and `DQuat` for planetary/rocket domain coordinates,
  velocities, orientations, distances, radii, and physical calculations.
- Use `Vec3`/`Transform` only after camera-relative conversion at the render boundary.
- Never derive physics from a rendered transform or absolute `f32` solar position.
- Never silently mix kilometers, meters, solar display units, degrees, radians,
  real time, and simulation time. Name values with units.
- Normalize directions only where required and guard zero/non-finite vectors.
- Document each newly introduced frame's origin, axes, handedness, units, epoch,
  and owning module before implementation.

## Conversion Discipline

1. Identify source frame, destination frame, units, and simulation epoch.
2. For ephemeris inputs, record the NAIF body IDs, kernel frame, center, and
   TDB epoch before converting.
3. Use the existing domain converter for physical frames. Extend it with a
   tested pure function only for a new physical conversion; display conversion
   belongs in the existing infrastructure adapter.
4. Convert position and velocity separately where rotating frames are involved.
5. Apply body rotation/surface velocity consistently for launch, atmosphere, and contact.
6. Convert to render coordinates only after authoritative physics is complete.

Do not use latitude/longitude as a generic world topology. They are geodetic
representations at the body-fixed boundary. Terrain uses cube-sphere direction;
use the existing mapping between them.

## Frame-Specific Guidance

- Integrate rocket motion in planet-centered inertial coordinates before any
  body-fixed/terrain conversion. Do not introduce fictitious forces accidentally.
- Use body-fixed coordinates for rotating launch pads and terrain sampling.
- Subtract `surface_velocity_in_planet_inertial` for atmosphere-relative or
  terrain-relative velocity, where applicable.
- Use ENU for local launch/landing/control interpretation, not as an unlabelled
  substitute for global inertial state.
- Rocket visual planets are presentation proxies evaluated at simulation time;
  neither shared solar-map transforms nor proxy meshes are ephemeris authority.
- `EphemerisSnapshot` supplies complete same-epoch DE440/PCK states and
  orientations. Derive solar-inertial presentation state through its existing
  adapters; do not restore analytic heliocentric presentation consumers.

## Tests

Add pure regression tests in `src/domain/services/reference_frames.rs` for
every new physical conversion. Test Bevy/display conversion at its
infrastructure boundary:

- forward/backward round trip within tolerance;
- expected axis direction and rotation at a known epoch;
- geodetic launch-site consistency;
- velocity correctness for a rotating surface;
- precision boundary at solar-scale distances;
- barycentric-to-heliocentric and barycentric-to-planet-centered position and
  velocity subtraction at a recorded TDB epoch;
- finite values and no f64-to-f32 loss before camera-relative rebasing;
- finite values and valid normalized orientation/directions.

Run the relevant terrain, rocket contact, rocket camera, and orbital tests after
any frame change. Coordinate bugs are cross-cutting regressions, so also run the
full suite and all three application modes.

Reject hidden unit conversion, duplicated rotation formulae, direct f64-to-f32
world placement without rebasing, using render state as truth, and untested
changes to frame conventions.
