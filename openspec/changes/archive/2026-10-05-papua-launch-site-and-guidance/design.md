## Context

See proposal.md - Why. The live rocket spawn already uses
`predefined_sites::papua_indonesia_coastal_lowland()` and inserts
`setup.launch_site` on the vehicle, but it also inserts `RocketAutopilot::default()`,
whose `target_orbit.target_inclination_rad` and
`ascent_profile.target_inclination_rad` are hardcoded to 28.5 degrees. Ascent
guidance consumes `target_orbit.target_inclination_rad` to compute the launch
heading, so the flown plane does not match the Papua site.

A procedural launch pad already exists (`LaunchPadPresentation`,
`spawn_procedural_launch_pad`, `spawn_pad_children`) and is already geodetically
anchored from the same `LaunchSetup` fields. It is the correct integration point
for improved presentation.

The KSC change added a generic local-elevation package reader, offline
converter, and a `LocalElevationOverlayTerrainSource` in the Earth catalog. Only
tests reference the overlay; no production path composes a local package.

## Goals / Non-Goals

**Goals:**
- Make the flown ascent plane match the active launch site with a single
  authoritative derivation.
- Remove the KSC-scoped change and the now-unused local-elevation authority.
- Improve launch-site presentation within the existing anchored, collision-free
  pad architecture.

**Non-Goals:**
- A Papua DEM package or any measured local elevation for the site.
- Collision against launch-site structures.
- Changing the global/procedural Earth elevation source itself.
- Selectable alternate launch sites or KSC support.

## Decisions

### Derive inclination at spawn from the launch site

Configure the autopilot from the launch site when the vehicle is spawned, using
a `RocketAutopilot::for_launch_site(latitude_deg)` constructor that sets
`target_orbit.target_inclination_rad` to `latitude_deg.abs().to_radians()`.

Rationale: a prograde launch due east from latitude `phi` has an orbital
inclination of `|phi|`, so the minimum-energy plane is the site latitude. The
site is already available in `LaunchSetup`, so no new resource or plumbing is
needed.

Alternative considered: leaving the fixed default and configuring it from a
settings resource. Rejected because the site is already per-vehicle authoritative
state and a second configuration source would risk disagreement.

`AscentGuidanceProfile` also carried a `target_inclination_rad` field that no
code ever read; the ascent heading consumes `target_orbit.target_inclination_rad`
directly. That duplicate field is removed so the target inclination has one
authority. `LowEarthOrbitTarget::default()` keeps a neutral reference value for
tests and non-launch uses, and its KSC-latitude comment is corrected; the flight
spawn no longer relies on the default.

### Remove the local-elevation overlay rather than leave it dormant

Delete `LocalElevationPackage`, the `local_elevation_convert` binary, and
`LocalElevationOverlayTerrainSource`/`with_dem_and_local_elevation_paths`,
including their tests and the `Cargo.toml` bin entry.

Rationale: the project rule is to avoid dead, speculative abstractions. With the
Papua site procedural-only, nothing production reaches the overlay, and leaving a
second elevation authority contradicts the single-authority requirement.

Alternative considered: keeping it as a future generic capability. Rejected
because a future measured site should be designed against that site's real
provenance and seam needs, not inherited unused.

### Reuse the anchored, collision-free pad architecture

Extend `spawn_procedural_launch_pad`/`spawn_pad_children` with additional
structures and detail, keeping `LaunchPadPresentation` as the sole geodetic
anchor and keeping all children presentation-only.

Rationale: the anchor already maps the site's body-fixed point, normal, and
heading through the shared terrain frame. Adding structures as children avoids a
second placement path and preserves the "structures have no physics authority"
rule.

## Risks / Trade-offs

- [Removing the overlay could break a build or test that references it] →
  Remove the module, bin, Cargo entry, catalog overlay, and their tests in one
  change and run the full default and `dem` test suites.
- [Neutral default inclination could silently persist in a non-launch path] →
  The only flight spawn path uses `for_launch_site`; add a regression asserting
  the spawned vehicle's target inclination equals the site latitude.
- [New pad detail could regress frame time] → Keep structures behind the
  existing proximity/quality gating and bound draw distance and light count.
- [Removing KSC config could leave an orphaned asset reference] → Remove
  `earth_ksc_3dep_v1.ron`; the gitignored local mesh and GeoTIFFs are not
  referenced by any runtime path.

## Migration Plan

1. Drop the `ksc-data-backed-terrain` change and its KSC config.
2. Remove the local-elevation module, bin, catalog overlay, and tests.
3. Add `for_launch_site` and use it in the spawn path; update defaults/tests.
4. Extend launch-site presentation within the existing anchored architecture.
5. Validate; rollback is a revert of these edits, which restores the fixed
   default inclination and the prior pad.
