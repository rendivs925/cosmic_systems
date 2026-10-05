## Why

The rocket flight mode launches from the Papua, Indonesia coastal lowland site
(latitude -8.0, longitude 139.5), but ascent guidance is hardcoded to a 28.5
degree KSC-latitude target inclination, so the vehicle flies a plane that does
not match its actual launch site. The launch site itself is represented only by
a placeholder procedural tower, and a previously proposed KSC-scoped terrain
change does not match the real site.

## What Changes

- Derive the ascent and orbit target inclination from the active launch site's
  latitude instead of a hardcoded KSC value.
- Drop the `ksc-data-backed-terrain` change and its KSC-specific configuration.
- Remove the unused local-elevation package/overlay machinery that the KSC
  change introduced; the launch site uses the existing global/procedural Earth
  elevation source.
- **BREAKING** (internal): `AscentGuidanceProfile` and `LowEarthOrbitTarget`
  no longer carry a KSC-latitude default inclination for flight use.
- Add a bounded, geodetically anchored launch-site presentation: pad, service
  tower, and supporting structures as presentation-only assets with no
  collision or physics authority.

## Capabilities

### New Capabilities
- `launch-site-presentation`: Geodetically anchored, quality-bounded procedural
  launch pad, service tower, and supporting structures at the active launch
  site, separate from terrain height and collision authority.

### Modified Capabilities
- `rocket-guidance-control`: Ascent and orbit-insertion target inclination is
  derived from the active launch site latitude rather than a fixed KSC value.
- `terrain-source`: The Earth terrain source no longer exposes a local measured
  elevation overlay; the launch site consumes the global/procedural source with
  deterministic fallback.

## Impact

- `src/application/rocket_spawning.rs`: autopilot configured from the launch
  site; launch-site presentation assets spawned and anchored.
- `src/domain/services/guidance/ascent.rs`,
  `src/domain/services/physics_orbital/state_orbits.rs`: inclination defaults
  and constructors.
- `src/infrastructure/bevy_adapters/rocket/ground_presentation.rs`: pad/tower
  presentation children.
- `src/domain/services/terrain_source/catalog.rs`,
  `src/domain/services/local_elevation.rs`, `src/bin/local_elevation_convert.rs`,
  `Cargo.toml`: remove local-elevation overlay and converter.
- `assets/configs/terrain/earth_ksc_3dep_v1.ron` and
  `openspec/changes/ksc-data-backed-terrain/` removed.
