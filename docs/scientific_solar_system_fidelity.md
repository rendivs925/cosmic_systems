# Scientific Solar-System Fidelity

This document states the current scientific authority and boundaries. It does
not make an accuracy claim when a required local dataset or external-reference
result is unavailable.

## Data Authority And Coverage

`assets/configs/ephemeris/de440.ron` is the one runtime scientific-data
manifest. Its current identifier is `naif-de440-egm2008-primary-v1`.

| Role | Authority | Frame/time/units | Validity |
| --- | --- | --- | --- |
| Translation | NAIF `de440s.bsp` | SSB ICRF/J2000, TDB, f64 SI at the boundary | JD TDB 2415020.5 through 2469807.5 |
| Orientation | NAIF `pck00011.tpc` | ICRF/J2000 to IAU body-fixed, TDB | Manifest range above |
| GM | NAIF `gm_de440.tpc` | m3/s2 | No epoch range declared |
| Earth J2 | `egm2008_earth_j2.ron` EGM2008 extract | Planet-centered inertial force calculation | Static model parameters |
| Leap seconds | NAIF `naif0012.tls` | UTC conversion source | JD 2441317.5 through 2457754.5 |
| Earth orientation | No provisioned dataset | UT1 and polar motion unavailable | No reference-grade result |

The manifest pins source URLs, byte sizes, and SHA-256 checksums. Provision it
with `scripts/provision_de440_kernels.sh`; startup must report an unavailable,
invalid, or out-of-coverage role rather than substitute a catalogue or network
fallback. JPL Horizons is a recorded validation source only, never a runtime
authority.

## Frames, Units, And Precision

- Authoritative ephemeris states are f64 meters and meters per second in SSB
  ICRF/J2000 at one TDB epoch.
- Planet-centered inertial states are same-epoch differences of authoritative
  body states. Vehicle gravity and long-arc propagation use these SI states.
- Orientation is inertial-to-IAU body-fixed at the same epoch. Earth geodetic
  conversion uses WGS-84; bodies without an approved ellipsoid remain explicitly
  spherical.
- Local ENU and camera-relative coordinates are derived presentation or local
  frames. f32/render transforms, scaled solar-map positions, orbit ribbons, and
  artistic lighting are never physical state authorities.
- Pure physical frame conversion is in `src/domain/services/reference_frames.rs`.
  The Bevy/display boundary, including `PhysicalScale` and solar-map f32
  conversion, is in `src/infrastructure/bevy_adapters/`.
- `SimulationTime` advances the shared epoch after completed fixed ticks. UTC,
  TAI, TT, TDB, and optional UT1 are distinct representations; UTC is never
  silently used as UT1.

## Force Tiers

All tiers retain the same planet-centered inertial SI vehicle state. Terms are
evaluated by the shared gravity authority, not from render transforms.

| Tier | Included terms | Intended use and limit |
| --- | --- | --- |
| `TwoBody` | Bound-body point mass | Short, central-body orbital analyses; no J2 or third-body effects |
| `EarthJ2` | Bound-body point mass, Earth J2 | Earth orbital precession studies; no lunar or solar perturbation |
| `EarthMoonSun` | Bound-body point mass, lunar and solar differential third body | Earth-to-lunar/solar-perturbed coast; no higher harmonics, tides, SRP, or drag climatology |
| `PlanetSun` | Bound-body point mass, solar differential third body | Existing powered-flight compatibility default; not a full Earth-orbit model |

Earth J2 uses the EGM2008 reference radius and normalized coefficient supplied
by the manifest. Lunar and solar terms are same-epoch differential accelerations
relative to the accelerating bound-planet origin.

## Long-Arc Numerical Budgets

These bounds validate the DOP853 numerical integration against a stricter
integration of the same force model. They are not external trajectory accuracy
claims.

| Scenario | Tier | Horizon | Position residual | Velocity residual |
| --- | --- | ---: | ---: | ---: |
| LEO | `TwoBody` | 86400 s | 1 m | 0.001 m/s |
| Earth J2 precession | `EarthJ2` | 259200 s | 5 m | 0.005 m/s |
| Lunar transfer | `EarthMoonSun` | 259200 s | 100 m | 0.1 m/s |
| Earth escape | `TwoBody` | 259200 s | 10 m | 0.01 m/s |

The current long-arc settings are DOP853, relative tolerance `1e-10`, absolute
position tolerance `1e-3 m`, absolute velocity tolerance `1e-6 m/s`, and a
maximum step of 60 s. Powered flight, contact, propulsion, and control remain
authoritative in the fixed pipeline and are not mutated by this read-only
predictor.

## Validation Status

Internal replay fixtures, including `tests/baselines/ascent.ron`, establish
determinism only. Each fixture records the affected external case IDs, whether a
divergence is intentional, and `Accepted`, `Rejected`, or `Unverified` external
acceptance output. `Unverified` is explicitly not acceptance.

`.github/workflows/scientific-validation.yml` provisions the pinned kernel set
and runs the external-reference test independently of replay tests. If
provisioning is unavailable, it emits an `UNVERIFIED` warning and job summary;
it does not label deterministic agreement as scientific validation.

Known approximations: no provisioned EOP data, no full-degree gravity field,
no tides, SRP, atmospheric climatology, live tracking, or online runtime data.
Kernel-backed accuracy is bounded by the stated manifest coverage and each
recorded external case's published residual budget.

`reference_cases_v1.ron` records machine-readable JPL Horizons and CSPICE
references. The current CSPICE set independently covers Earth orientation, KSC
Earth-fixed position, Earth-to-Sun direction, DE440 Earth two-body gravity, and
one-day two-body propagation. J2 and Earth-Moon-Sun long-arc comparisons remain
unverified pending an approved independent propagator export.
