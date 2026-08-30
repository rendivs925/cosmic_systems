# Runtime Scientific Consumer Inventory

This inventory records the pre-migration scientific authority for task 1.1.

| Consumer | Current authority | Frame and units | Modes | Classification |
| --- | --- | --- | --- | --- |
| `EphemerisPlugin` and `update_ephemeris_snapshot` in `src/infrastructure/bevy_adapters/ephemeris.rs` | Local DE440s SPK evaluated by `SpiceEphemeris` for 11 NAIF bodies at `SimulationTime::tdb_epoch()` | SSB ICRF/J2000, f64 meters and meters per second, TDB | Normal, craft, rocket | Kernel-backed |
| `update_planet_positions` and `interpolate_planet_transforms` in `src/infrastructure/bevy_adapters/planet_systems.rs` | Snapshot for the Sun, eight planets, Earth, and Moon; catalog Kepler propagation for unmapped moons | Project heliocentric J2000-ecliptic display frame, f64 display units | Normal, craft | Kernel-backed for mapped bodies; approved approximation for unmapped moons |
| `update_planet_rotations` and `update_orbit_positions` in `src/infrastructure/bevy_adapters/planet_systems.rs` | Catalog `rotation_period_hours` and `axial_tilt_deg`, driven by `Time<Fixed>` and `SolarSystemParameters` | Bevy display transform, catalog hours and degrees | Normal, craft | Presentation-only |
| `craft_spawn_position` in `src/application/craft_startup.rs` | Analytic Earth position at day zero | Solar-map display units | Craft | Presentation-only |
| `update_rocket_planets` in `src/infrastructure/bevy_adapters/rocket_planet.rs` | Snapshot Sun, bound-body, and mapped moon translations; catalog spin and Kepler fallback for unmapped moons | Planet-centered inertial/render-local meters | Rocket | Kernel-backed translation; presentation-only orientation and fallback |
| `update_sun_day_night_cycle` in `src/infrastructure/bevy_adapters/rocket_environment.rs` | Same-epoch snapshot Sun and bound-body state | Normalized planet-centered inertial direction | Rocket | Presentation-only, kernel-fed |
| `update_rocket_gravity` in `src/infrastructure/bevy_adapters/rocket_gravity_orbit.rs` | Catalog mass times G for bound-body and Sun GM; snapshot Sun/bound-body geometry for solar differential gravity | Planet-centered inertial, f64 SI | Rocket | Approved approximation |
| Rocket guidance, contact, and trajectory prediction adapters | Catalog mass times G for local mu | Planet-centered inertial, f64 SI | Rocket | Approved approximation for guidance/contact; presentation-only prediction |
| Body-fixed conversion and surface velocity in `src/domain/services/reference_frames.rs` | Uniform catalog spin and axial tilt | Planet-centered inertial, body-fixed, and ENU; f64 SI | Rocket consumers | Approved approximation |
| Launch, terrain, atmospheric-relative velocity, stage separation, and terrain map adapters | `SimulationTime` plus catalog orientation through reference frames | Planet-centered inertial and body-fixed, f64 SI | Rocket | Approved approximation for physical conversion; presentation-only map/render output |
| Rocket presentation, terrain rendering, telemetry, and HUD adapters | Authoritative state projected or observed after simulation | Rebased f32 render coordinates or derived telemetry | Rocket | Presentation-only |

## Declared Dataset Roles

| Manifest input | Declared role | Current runtime state |
| --- | --- | --- |
| `de440s.bsp` | Planetary translation SPK | Loaded and evaluated as the shared state authority |
| `pck00011.tpc` | Text PCK body orientation | Provisioned and checksum-validated only |
| `gm_de440.tpc` | Text PCK gravitational parameters | Provisioned and checksum-validated only; catalog mass times G remains active |
| `naif0012.tls` | Leap-second kernel | Provisioned and checksum-validated only; no UTC, TAI, or TT consumer exists |
| Earth orientation parameters | UT1 and polar motion | Explicitly unavailable in the manifest; no runtime consumer exists |

## Boundaries And Gaps

- `SimulationTime` is a shared fixed-tick TDB-relative clock, but it does not
  provide UTC, TAI, TT, or UT1.
- Normal and craft translation consume `SimulationTime`, while rotation consumes
  `Time<Fixed>` through the separate `SolarSystemParameters` display clock.
- All high-fidelity runtime translation is native-only. The WASM entrypoint does
  not currently install shared ephemeris/time resources and is not a
  kernel-backed scientific mode.
- Visual Sun enlargement, orbit ribbons, cloud motion, HUD cadence, and render
  rebasing are presentation behavior and must not become a simulation authority.
