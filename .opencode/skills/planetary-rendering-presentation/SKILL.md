---
name: planetary-rendering-presentation
description: Use when changing planet, SPICE/DE ephemeris presentation, terrain, atmosphere, ocean, cloud, camera, HUD, render origin, materials, meshes, or presentation synchronization in Cosmic Systems.
---

# Planetary Rendering And Presentation

Apply this skill to visuals and cameras. Rendering visualizes authoritative
simulation; it does not define it.

## Existing Authorities

- `rocket_planet.rs`: rocket-mode celestial presentation proxies and bound-planet fallback globe.
- `terrain_render.rs` and `terrain_surface.rs`: terrain render entities, render origin, mesh/material presentation.
- `rocket_presentation.rs`: fixed-state capture and render interpolation.
- `rocket_camera_systems.rs` and camera adapters: flight camera behaviour.
- `rocket_environment.rs`, material/texture factories, and presentation UI modules.
- The shared evaluated ephemeris state is the source for celestial transforms,
  Sun direction, orbit paths, and camera targets once kernel migration begins.

Read these before adding a renderer, camera coordinate conversion, planet proxy,
material loader, or terrain visual fallback. Preserve the shared solar authority
while rocket mode hides solar-scale presentation and uses flight-frame proxies.

## Simulation/Presentation Boundary

```text
authoritative f64 simulation state
  -> reference-frame/render-origin conversion
  -> camera-relative f32 render state
  -> Transform, mesh, material, light, atmosphere, UI
```

- `Transform`, `GlobalTransform`, mesh vertices, camera pose, and shaders are
  never physics/collision/terrain authority.
- Convert large positions through `PhysicalScale` and `RenderOrigin`; do not put
  huge absolute coordinates into `f32` transforms.
- Render interpolation may smooth fixed snapshots but must not write back to
  authoritative rocket/celestial state.
- A rocket camera follows physical orientation; it does not steer the vehicle.
- Planet, Moon, Sun, light, and orbit-ribbon presentation must sample one
  evaluated ephemeris epoch. Do not mix a kernel state with a wall-clock orbit
  transform, artistic light orbit, or separately propagated display position.

## Visual Systems

- Terrain rendering consumes ready streamed geometry. It does not resample terrain
  height, run erosion, or choose collision data.
- The inner bound-planet globe is a presentation fallback beneath streamed terrain,
  not a full-planet terrain substitute or collision model.
- Keep terrain, ocean, atmosphere, clouds, lighting, and UI independent systems.
  Atmosphere and ocean must not mutate terrain geometry.
- Use geometry for macro/medium form. Use material/shader detail for grain, small
  rocks, normal variation, colour, roughness, and other micro detail.
- Keep material blending continuous from physical/environmental inputs. Preserve
  triplanar terrain projection where appropriate to avoid cube-sphere seams.
- Reuse asset handles and factory/configuration paths. Do not scatter asset paths
  or reload the same asset in unrelated systems.
- Rebase f64 barycentric state against the selected render origin before the
  final f32 conversion. Never place an absolute DE/SPICE-scale position in a
  `Transform`.

## Camera Rules

- Support orbital, chase, cockpit, surface, free, map, and debug behaviour by
  extending reusable camera math rather than cloning per-mode implementations.
- Camera input belongs in `Update`; camera presentation may be variable-rate.
- Culling/LOD decisions use actual camera projection/FOV and conservative patch
  bounds, never an assumed fixed aspect ratio.
- Keep debug visualization presentation-only and gated behind an explicit mode.

## Validation

Test pure conversions and interpolation behaviour without opening a window. For
render changes, verify asset/entity lifetime, planet swaps, render-origin rebasing,
and terrain patch replacement/eviction. Verify all celestial visuals use the
same recorded ephemeris epoch. Run affected mode startups. Do not claim
visual correctness when the environment has no usable display; report logs and
the limitation instead.

Reject presentation state used as truth, second floating origin, planet-wide
high-resolution fallback geometry, synchronous terrain generation for visual
holes, and shader logic that silently replaces physical models.
