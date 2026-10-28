# Solar Irradiance (TSIS-1) Deferral

Status: **deferred** on 2026-10-28. No TSIS-1 package, loader, or runtime
consumer is added by this decision. The simulator continues to use the single
calibrated solar illuminance constant described below.

This note exists so the remaining Phase 7 item is closed deliberately rather
than left as an untracked "open" entry. It records what the data is, why it is
not wired in, and the concrete conditions that would reopen the decision.

## Source reviewed

- Product: TSIS-1 Total Solar Irradiance, Level 3 daily means, version 5.
- Provider: Laboratory for Atmospheric and Space Physics (LASP), University of
  Colorado Boulder.
- File:
  <https://lasp.colorado.edu/data/tsis/tsi_data/tsis_tsi_L3_c24h_latest.txt>
  (485,959 bytes at review time).
- Coverage: 2018-01-10 through 2026-10-23, 3,209 daily records, 24-hour
  cadence, TSI at 1 AU in W/m2.
- Licence: NASA/LASP data, publicly redistributed with attribution.

Measured values over the reviewed record:

| Statistic | Value |
| --- | --- |
| Minimum | 1360.2885 W/m2 |
| Maximum | 1363.6916 W/m2 |
| Mean | 1362.0681 W/m2 |
| Standard deviation | 0.5514 W/m2 (0.040%) |
| Peak-to-peak range | 3.4031 W/m2 (**0.250%**) |

The record contains one solar cycle, so this range is the total measured
variation available to any consumer, not a short-term sample.

## Existing solar authority

There is exactly one solar radiometric authority in the simulator:

- `SUN_ILLUMINANCE_AT_EARTH_LUX = 127_000.0` in
  `src/application/solar_system_startup.rs`, the direct normal illuminance at
  Earth's mean orbital distance before atmospheric attenuation.
- `solar_illuminance_lux()` in
  `src/infrastructure/bevy_adapters/rocket/environment.rs` scales that constant
  by the inverse square of the true planet-Sun distance from the shared
  ephemeris. Both the directional sunlight and the sky fill consume it, so they
  stay in one radiometric scale.

The 127 klx constant already encodes a nominal TSI of about 1361 W/m2 with a
fixed luminous efficacy of about 93 lm/W. Replacing the nominal value with the
measured record would change it by at most 0.25%.

## Why it is deferred

### 1. The default simulation epoch is outside the measured record

`DEFAULT_SIMULATION_START_UTC` is 2000-01-01T03:00:00Z. TSIS-1 begins on
2018-01-10. At 1x the simulation never reaches the first sample; even at the
maximum 10,000x acceleration, reaching 2018 takes about 16 wall-clock hours and
the whole record passes in under a day. There is no normal scenario in which a
measured TSI time series is sampled as a meaningful day-to-day curve.

Using the 2018 value for a 2000 epoch would misrepresent the measurement.
Covering 2000 would require a solar-cycle model (for example NRLTSI2 or
SATIRE), which is a different modeling decision, not measured data, and would
still change the rendered scene by a fraction of a percent.

### 2. The effect is below the presentation's own quantization

- Peak-to-peak measured variation: 0.250%.
- One code value in an 8-bit render target: 1/255 = 0.392%.
- Standard deviation: 0.040%.

Even a fully correct, epoch-matched implementation would not move an 8-bit
output by one step. It cannot be presented honestly as a visible improvement,
and claiming a visible solar-cycle effect would be false.

### 3. There is no physical consumer

- No solar radiation pressure, solar-sail, or drag-free model consumes TSI.
- No spectral rendering or radiometric calibration pipeline consumes the SIM
  spectral products.
- Flight physics must not read a measured irradiance input: it would perturb
  validated baselines for an invisible effect.

Adding a package now would be data with no consumer, which the repository's
data policy explicitly avoids.

## What would reopen this decision

Implement TSIS-1 when at least one of these becomes true:

1. A simulation scenario selects an epoch on or after 2018-01-10, or the shared
   default start epoch moves into the measured record.
2. Solar radiation pressure, a solar sail, or a high-precision drag-free mode is
   implemented and needs TSI as a force input.
3. Spectral rendering or an instrument/calibration mode needs the SIM spectral
   irradiance, not just the broadband TSI.
4. A solar-cycle visualization is requested and accepted as presentation-only.

## Implementation sketch if reopened

- Package the daily TSI series in a versioned offline package under
  `assets/large_files/` with SHA-256 provenance, in the same style as the
  gravity, climate, and star packages. No runtime downloads.
- Add one loader that returns `Option<f64>` for an epoch. Out-of-range epochs
  return `None`, which keeps the existing nominal constant; never clamp
  silently, because that would present 2018 data as if it described 2000.
- Add a presentation-only `SolarIrradiance` factor multiplied into
  `solar_illuminance_lux()`. Keep the force model, atmosphere, and terrain
  authorities on the nominal constant.
- Add a test asserting the factor is exactly 1.0 outside the measured range and
  within 0.25% inside it.
- Regenerate no physics baseline; the factor is presentation-only.

## Related

- `docs/plans/open-science-data-and-terrain-realism.md` (Phase 7)
- `docs/scientific_solar_system_fidelity.md` (known approximations)
- `docs/datasets/open_science_data_v1.md` (planned and deferred datasets)
