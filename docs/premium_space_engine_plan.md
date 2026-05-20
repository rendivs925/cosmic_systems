# Premium Space Engine Plan

## Design Philosophy

1. Every feature must work for all celestial bodies, not only a showcase subset.
2. Use one consistent visual language: dark space, cool blue-white UI, clean orbit lines, restrained glow.
3. Use one consistent interaction model across solar, craft, camera, and selection modes.
4. Avoid decoration for decoration's sake. Every visual element must serve navigation, education, scale, or immersion.
5. Preserve stable performance. Visual quality should scale down gracefully instead of causing frame drops.

## Phase 0: Foundation

Goal: make the simulation immediately feel like deep space.

### Starfield

- Re-enable the currently disabled starfield.
- Generate a static procedural star mesh with realistic size and color distribution.
- Use two depth layers for subtle parallax.
- Add a very subtle Milky Way band as a distant background plane.
- Keep runtime cost near zero by generating it once at startup.

### Post-Processing

- Enable HDR on the main camera.
- Add subtle bloom with a high threshold so mainly the Sun and selected/emissive markers glow.
- Use ACES tonemapping for a restrained cinematic response.
- Add TAA where supported for cleaner planet silhouettes and orbit lines.

### Lighting Baseline

- Reduce ambient light from the current bright baseline to a deeper space value.
- Keep a slight blue ambient tint for cold space readability.
- Preserve the Sun as the primary light source.

## Phase 1: Orbit Lines

Goal: make orbits beautiful, readable, and consistent without visual clutter.

### Geometric Thickness

- Replace one-pixel `LineList` orbit rendering with thin geometric orbit meshes or a screen-space thick-line shader.
- Ensure orbits remain visible at long distances without becoming heavy up close.
- Connect orbit thickness to quality level.

### Body-Class Color System

- Terrestrial planets: warm white with a faint amber tint.
- Gas giants: pale gold or cream.
- Ice giants: pale cyan.
- Moons: neutral grey-white and slightly dimmer.
- Selected orbit: same class color, slightly brighter and wider.

### Distance-Based Opacity

- Fade orbit visibility based on camera distance and selected-body relevance.
- Brightest visibility should happen at useful navigation distances.
- Fade orbits when the camera is too close to the body or orbit path.
- Apply one consistent opacity formula to all bodies.

### Orbital Position Tracker

- Add one small emissive marker per orbit at the body's current position.
- Match marker color to the orbit class.
- Keep markers small and functional, not decorative.

### Adaptive Segments

- Ultra: 512 segments.
- High: 256 segments.
- Medium: 128 segments.
- Low: 64 segments.
- Use the existing quality system to drive segment count.

### Correct Eccentricity Markers

- Replace placeholder eccentricity with each body's real orbital eccentricity.
- Position apoapsis and periapsis markers using the same orbital geometry as simulation positions.

## Phase 2: Planets

Goal: make every body feel physically believable and consistently rendered.

### Atmospheric Rim Glow

- Use one shared transparent shell technique for atmospheric bodies.
- Earth: subtle blue rim.
- Venus: thicker yellow-orange rim.
- Titan: thin orange rim.
- Gas giants: very subtle class-colored rim.
- Airless bodies should not receive an atmosphere effect.

### Sun Shadows

- Enable shadows only on higher quality levels.
- Keep shadows focused on planets and moons, not orbit lines or UI.
- Use shadows to improve depth perception, not dramatic styling.

### Normal Maps

- Add or generate normal maps for planet and moon textures.
- Keep the material workflow consistent for every body.
- Scale texture detail through quality settings.

## Phase 3: Camera And Navigation

Goal: make movement smooth, predictable, and professional.

### Unified Transition Curve

- Replace ad-hoc interpolation with one shared ease-in-out transition system.
- Use it for selection auto-inspect, focus, menu navigation, overview return, and craft target travel.
- Prefer constant-feeling travel over abrupt jumps.

### Auto-Inspect Consistency

- Apply the same framing rules to all bodies.
- Adjust orbit speed based on body size so small moons do not feel sluggish and gas giants do not feel frantic.
- Smoothly interpolate camera tilt and distance when switching targets.
- Frame Saturn and ringed bodies with ring orientation in mind.

### Focus Behavior

- Make the focus hotkey place the selected body at a consistent viewport size.
- Support planets, moons, and the Sun with the same logic.
- Use the selected body's radius and special features, such as rings, to choose distance.

### Overview Return

- Smoothly transition to the solar-system overview instead of snapping.
- Reuse the same camera transition system.

## Phase 4: UI And Data

Goal: provide clear information without adding visual noise.

### Simulation Clock

- Add a minimal simulation date/time display.
- Show the current time rate, such as `3000x`.
- Keep it in the navbar area using the existing UI style.

### Consistent Info Cards

- Every body should show the same core data structure.
- Include name, type, radius, orbital period, distance from parent, and current distance from camera.
- Avoid missing or uneven sections.

### Selected Body Distance Indicator

- Add a small unobtrusive selected-body distance label.
- Fade it based on relevance and distance.
- Use the same typography and opacity rules as the rest of the UI.

### UI Style Audit

- Consolidate UI colors, border opacity, font sizes, and spacing.
- Ensure selector, info cards, notifications, craft UI, and education UI feel like the same product.

## Phase 5: Audio

Goal: add subtle immersion without making sound required.

### Ambient Soundscape

- Add a quiet low-frequency space ambience loop.
- Keep volume low enough that it can be ignored.
- Optionally shift slightly near massive bodies.

### Selection Sound

- Add a short, clean selection confirmation tone.
- Use it consistently for mouse, tab, and menu selection.

### Notification Sound

- Add one soft notification chime for queued messages.
- Avoid repeated sounds for rapid UI updates.

## Phase 6: Performance And Polish

Goal: keep the engine solid after adding premium features.

### Level Of Detail

- Orbit segment count scales with quality and distance.
- Atmosphere shells hide or simplify at long range.
- Planet texture detail scales with quality level.

### Adaptive Quality Integration

- Connect bloom, shadows, orbit thickness, atmosphere, and segment counts to the existing quality controller.
- Ultra and High enable all premium features.
- Medium keeps the core look but reduces expensive features.
- Low and Minimal preserve readability first.

### Verification

- Keep `cargo check` passing after every stage.
- Run `cargo clippy` before major commits.
- Confirm solar mode and craft mode both remain usable.
- Verify frame rate stability after each visual feature is added.

## Explicitly Excluded

These are intentionally excluded because they risk over-decoration or distraction.

- Decorative particle effects.
- Lens flares.
- Screen shake.
- Chromatic aberration.
- Vignette.
- Film grain.
- Motion blur.
- Decorative glow trails.
- Heavy volumetric fog.
- Complex reflection probes.
- Unnecessary terrain decoration.

## Recommended Build Order

1. Starfield, HDR, bloom, tonemapping, and TAA.
2. Premium orbit lines: thickness, opacity, color classes, and position markers.
3. Atmosphere shells and optional shadows.
4. Unified camera transitions and focus behavior.
5. Simulation clock and UI consistency pass.
6. Subtle ambient audio.
7. LOD and adaptive quality integration.
