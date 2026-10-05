## Context

Rocket mode and the solar map already share one scientific authority chain:
`SimulationTime` -> `EphemerisSnapshot` (DE440/PCK) -> reference-frame
conversion -> render origin -> camera-relative f32 presentation. Direct
lighting, sky, and terrain already consume that state, but with approximations:
a constant Earth illuminance in rocket mode, a fixed Sun-disc angular size, a
flat albedo emissive on every textured body, disabled shadows, and a
clear-colour plus `DistanceFog` sky.

Bevy 0.17.3 ships a Bruneton atmosphere, but its shader assumes the local
vertical is world `+Y` with the planet directly below the camera, which is
incompatible with the planet-centered inertial flight frame. The rocket frame is
inertial with an arbitrary local up at the launch site, so a custom sky that
takes the true planet centre and local vertical is required.

See proposal.md for motivation and the capability spec for required behaviour.

## Goals / Non-Goals

**Goals**

- One physically consistent lighting model shared by solar-map and rocket modes.
- Sun direction, distance, disc size, terminator, shadows, and sky all derived
  from the same ephemeris epoch and reference frame.
- Bounded, measurable cost with existing quality/performance hooks.
- Pure, unit-testable mappings for every scalar rule that is not GPU-only.

**Non-Goals**

- No change to flight dynamics, guidance, propulsion, terrain source, terrain
  collision, coordinate authority, or simulation time.
- No solar-map eclipse shadows in this change (explicitly deferred).
- No second floating origin, no runtime data download, no new renderer.
- No claim of photoreal regional cloud/scattering without measured evidence.

## Decisions

### 1. Scale illuminance from ephemeris distance, not a per-body constant

The directional light illuminance becomes `E_ref * (d_ref / d)^2`, with `d`
the bound-planet-to-Sun range from `EphemerisSnapshot` and `E_ref`/`d_ref` the
existing Earth calibration. This keeps Earth unchanged, fixes every other body,
and preserves the single calibration constant. The update runs in the existing
Sun presentation system so one owner writes the light.

### 2. Sun disc size and horizon come from the same direction state

The disc's angular radius is `asin(R_sun / d)` for the current `d`, so it is
correct at any body. Visibility is gated on the solar altitude
`asin(dot(sun_direction, local_up))`, hidden below a small negative refraction
allowance. `local_up` is the observer's planet-centered position normal, already
the convention used by the existing daylight code.

### 3. Terminator is geometric; twilight is a separate bounded band

Direct lighting is already geometric in the PBR model. The non-direct
`local_daylight` factor is rewritten in terms of solar altitude with explicit
civil/nautical/astronomical bands so sky, ambient, and cloud opacity fade on a
physical schedule without biasing the direct terminator.

### 4. Remove flat emissive; keep only real emission

All non-Sun bodies stop using `albedo` as an emissive texture with a constant
value. The Sun keeps its luminance-based emissive. Earth uses its real
night-emission texture. This is a material-construction change in the solar-map
startup and the rocket moon/bound-planet spawn paths.

### 5. Shadows: one cascade configuration, explicitly exclude enclosing shells

The rocket directional light enables `shadows_enabled` with an explicit
`CascadeShadowConfig` sized to the near-flight scale, plus a configured
`DirectionalLightShadowMap`. The far-field planet shell and its cloud shell are
marked `NotShadowCaster` and `NotShadowReceiver`; this is the specific fix for
the historical planet-scale dark arc, because that sphere encloses the camera.
Starfield already excludes itself. Terrain and the vehicle keep default shadow
participation. Solar-map point-light shadows stay disabled in this change.

### 6. Custom single-scattering sky with a camera-anchored dome

A new `sky.wgsl` fragment shader computes Rayleigh/Mie/ozone single scattering
by bounded numerical integration from the camera along each view ray, taking:
planet centre in render coordinates, local up, Sun direction, atmosphere radii,
scattering/absorption coefficients, and ground albedo. A camera-anchored,
front-culled dome mesh with a custom `Material` renders it as a depth-tested,
depth-write-disabled, alpha-blended surface so terrain occludes it and it
correctly extinguishes stars near the horizon. This honors the true planet
centre and vertical instead of Bevy's `+Y` assumption.

### 7. Atmospheric optics are a domain value object

Rayleigh/Mie/ozone coefficients, scale heights, layer altitudes, radii, and
ground albedo become a Bevy-free `AtmosphericOptics` value object with per-body
presets (full Earth set first; physically parameterized others). The WGSL
algorithm is the only renderer implementation; the domain object owns only the
parameters and any pure sampling the presentation adapter needs, avoiding a
second renderer while keeping constants single-sourced and testable.

### 8. Aerial perspective uses the same optics

The terrain surface extension receives the same optics/lighting uniforms and
adds in-scattering plus transmittance along the fragment view path, replacing
the independent `DistanceFog` colour. The dome and terrain therefore agree at
the horizon by construction.

### 9. Exposure and bloom are calibrated once

The shared camera gains `Hdr` and an explicit `Exposure` derived from the
physical solar illuminance, and the bloom threshold is expressed relative to the
calibrated solar luminance. Because the same camera serves all modes, normal,
craft, and rocket presentation are validated together and the change is reverted
if solar-map presentation degrades.

## Risks / Trade-offs

- [Custom sky integration cost] -> fixed bounded sample count, reuse existing
  quality/performance telemetry, measure before raising samples.
- [Exposure change regresses solar map since the camera is shared] -> calibrate
  and visually check all three modes in the same phase.
- [Shadow acne/aliasing with rebased streamed terrain] -> exclude the enclosing
  globe, tune depth/normal bias and cascade distances, validate near-pad and at
  altitude.
- [CPU tests diverging from WGSL] -> keep parameters in the domain value object
  and treat the shader as the only render implementation; any CPU reference is
  test-only and documented.
- [Hiding the Sun disc under refraction changes existing visuals] -> use a small
  bounded allowance and cover it with a direction/horizon test.

## Migration Plan

1. Land each phase independently with its tests and commit.
2. Keep the previous presentation values only as the reference calibration for
   Earth; remove per-body constants as each is replaced.
3. Roll back by reverting the phase commit; no persisted or authoritative
   simulation data changes, so no data migration is required.
