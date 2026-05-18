# COSMIC SYSTEMS -- Master Implementation Plan

## Vision

A real-time interactive solar system simulation that models gravity as a pressure gradient in a vacuum superfluid rather than a Newtonian force. Includes a flyable UFO craft powered by asymmetric vacuum polarization (lift) and zero-point energy extraction (ZPE), with full educational annotation explaining the physics and its Quranic references.

The simulation is:
- **Physically accurate**: N-body planetary dynamics with real ephemeris data, symplectic integration
- **Visually stunning**: Atmospheric scattering, bloom, procedural starfield, lens flare, dynamic clouds, aurora, shadow maps
- **Educationally rich**: Real-time context-sensitive explanations, unlockable knowledge journal with Quranic references, side-by-side physics comparison mode
- **High performance**: Adaptive quality levels, SIMD/GPU compute dispatch, memory pooling

---

## Architecture Overview

```
                    Application Layer
               (startup, resource injection)
                         |
          +--------------+--------------+
          v              v              v
   +-----------+   +-----------+   +-----------+
   |  Domain   |   |    Infra  |   |Presentation|
   |  Services |   |   Bevy    |   |   UI /    |
   |  Entities |   |  Systems  |   | Education |
   | Value Objs|   |  Adapters |   |  Visuals  |
   +-----------+   +-----------+   +-----------+
```

- **Domain**: Pure Rust, no Bevy dependency. Physics math, formulas, data types, entities.
- **Infrastructure**: Bevy systems, components, resources. Wires domain into ECS.
- **Presentation**: UI panels, knowledge journal, vacuum field visualization, compare mode renderers.

---

## Phase 0 -- Educational Foundation

Build the core physics engine for vacuum propulsion and the education/annotation system that all later phases extend.

### New Files

| File | Description |
|------|-------------|
| `domain/services/vacuum_physics.rs` | Pure functions: lift_force, zpe_power, parametric_gain, duty_synergy, polarization_gradient, vacuum_density, pulse_energy_gain |
| `domain/value_objects/education.rs` | EducationMode (Simulation/Education/Compare), JournalCategory (VacuumSuperfluid/AsymmetricPolarization/ZpeExtraction/MetricEngineering/QuranicEvidence), QuranicReference, JournalEntry, UnlockCondition, JournalDatabase, EducationState |
| `presentation/education_panel.rs` | Bevy UI panel (right side, 380px) showing context-sensitive explanations, live vacuum physics telemetry, journal access button, compare mode toggle |
| `presentation/knowledge_journal.rs` | Tabbed full-screen journal with 16 entries across 5 categories. Each entry has: title, body text, Quranic reference with Arabic+translation, formula display |
| `presentation/vacuum_visualization.rs` | Three visual effects: DC field gradient glow (colored translucent ring around craft, blue below / orange-red above), virtual particle field (200-500 fading particle pairs near hull), ZPE ripple effect (expanding concentric rings on pulse) |
| `presentation/compare_mode.rs` | Side-by-side spawn: vacuum-powered UFO craft (left) vs Newtonian rocket (right) with independent HUDs |
| `infrastructure/bevy_adapters/education_systems.rs` | System wiring: journal unlock detection, education panel toggle (B key), context explainer refresh, vacuum visualization update |
| `presentation/education_data.rs` | Static database of all ~16 journal entries with full text, Arabic, formulas, unlock conditions |

### Files Modified

| File | Change |
|------|--------|
| `domain/services/mod.rs` | Add `pub mod vacuum_physics;` |
| `domain/value_objects/mod.rs` | Add `pub mod education;` |
| `presentation/mod.rs` | Add module declarations for all new presentation modules |
| `infrastructure/bevy_adapters/systems.rs` | Add `pub mod education_systems;` and re-export |
| `application/startup.rs` | Register education_systems, insert EducationState and JournalDatabase resources on app startup |

### Physics Formulas (vacuum_physics.rs)

```
Lift force (kN):
  F_lift = 47.0 * min(dc, 1.0).powf(1.35)
  clamped to 65 kN max

ZPE power (kW):
  base = 210.0 * pulse.powf(1.8)
  parametric_boost = if pulse > 0.42 { 1.0 + (pulse - 0.42) * 2.6 } else { 1.0 }
  duty_synergy = 1.0 + 0.4 * dc
  P_zpe = min(base * parametric_boost * duty_synergy, 1250.0)

Vacuum polarization gradient (T/m):
  G = 12.0 * dc * exp(-distance_from_hull * 3.0)

Vacuum density near a massive body (relative to far space):
  rho = 1.0 + G_constant * M_body / (c_squared * distance)

Pulse energy gain (MJ):
  E = P_zpe * pulse_width_seconds / 1000.0
```

### Knowledge Journal Entries (16 total)

**Category 1: Vacuum Superfluid** (4 entries)
- The Primordial Medium -- unlock: Immediate. Quran: Hud 11:7 "His Throne was upon water"
- Motion Through a Fluid -- unlock: CraftSpawned. Quran: Anbiya 21:33 "All swim in an orbit"
- Density Gradient Model -- unlock: AltitudeAbove(100)
- Evidence from Quantum Field Theory -- unlock: TimeElapsed(120)

**Category 2: Asymmetric Vacuum Polarization** (4 entries)
- The Lift Mechanism -- unlock: CraftSpawned. Formula: F_lift = 47.0 * DC^1.35
- The Segmented Hull -- unlock: PulseAbove(0.1)
- Directional Control -- unlock: SpeedAbove(10)
- Comparison to Archimedes -- unlock: AltitudeAbove(500). Quran: Kahf 18:84

**Category 3: ZPE Extraction** (4 entries)
- The Dynamical Casimir Effect -- unlock: PulseAbove(0.2)
- Parametric Resonance -- unlock: PulseAbove(0.42). Formula: P_zpe = 210 * pulse^1.8 * boost
- Over-Unity Explained -- unlock: AltitudeAbove(200)
- Practical Coil Design -- unlock: OrbitAchieved

**Category 4: Metric Engineering** (3 entries)
- Spacetime as a Fluid -- unlock: SpeedAbove(50)
- Tayy al-Ard (Folding the Earth) -- unlock: OrbitAchieved. Quran: Naml 27:40
- Interstellar Implications -- unlock: SpeedAbove(100)

**Category 5: Quranic Evidence** (1 entry)
- Comprehensive Reference Index -- unlock: Immediate

### Acceptance Criteria

- [ ] vacuum_physics unit tests pass with reference values: dc=0.38 yields lift approx 12.26 kN; pulse=0.5, dc=0.38 yields ZPE within expected range; polarization_gradient returns near-zero at distance > 2.0
- [ ] Pressing B toggles education panel open/closed
- [ ] Education panel shows live craft telemetry labeled with vacuum physics terminology: Lift Force (kN), ZPE Power (kW), DC Field, Pulse Resonance
- [ ] Context-sensitive explanation text changes based on craft state (hovering, moving, pulsing, orbiting)
- [ ] Knowledge journal opens and shows locked/unlocked entries correctly with visual distinction
- [ ] Entries unlock progressively as conditions are met; notification appears on unlock
- [ ] DC field gradient glow visible around craft in Education mode (blue below, orange/red above, intensity proportional to DC)
- [ ] Virtual particle field renders 200+ particles near craft hull with pair-creation/annihilation animation
- [ ] ZPE ripple effect fires on each pulse with expanding concentric rings that fade over 0.5s
- [ ] Compare mode spawns rocket craft alongside UFO with independent HUD showing thrust vs lift telemetry
- [ ] All Quranic references display Arabic text, English translation, and physics explanation
- [ ] EducationState resource initializes on app start and cleans up on exit

---

## Phase 1 -- N-Body Engine

Replace independent Keplerian orbits with symplectic N-body integration for the Sun and 8 planets. Moons remain analytic but orbit their parent's dynamic N-body position.

### New Files

| File | Description |
|------|-------------|
| `domain/services/nbody.rs` | Gravitational constant G = 2.9591220828559093e-4 (AU^3 day^-2 M_sun^-1); compute_nbody_accel(positions[], masses[]) -> Vec3[]; velocity_verlet_step(state[], dt); total_energy(state[]) -> f64 |
| `domain/value_objects/nbody_params.rs` | NBodyParams resource: sim_time_days, timewarp_rate, substeps_per_frame, paused, energy_total |
| `infrastructure/bevy_adapters/nbody_systems.rs` | integrate_nbody system (substep loop per frame, updates NBodyState components); sync_nbody_transforms (copies N-Body positions back to Bevy Transform) |

### Files Modified

| File | Change |
|------|--------|
| `entity_components.rs` | Add `NBodyTag` marker component; add `NBodyState { pos: Vec3, vel: Vec3, mass_msun: f32 }` component on major bodies |
| `physics_orbital.rs` | Add get_planet_mass_msun(name) -> f32 function returning masses in solar units |
| `planet_systems.rs` | update_planet_positions reads NBodyState.position for tagged bodies instead of Kepler solver; moon positions computed as `parent_nbody.position + kepler_moon_offset(moon, time_days)` |
| `solar_system_startup.rs` | On spawn: initialize NBodyState with Kepler position and computed tangential velocity; mark major bodies (Sun + 8 planets) with NBodyTag |
| `simulation_params.rs` | Add SimulationTime resource: total_sim_days: f64, paused: bool |

### Planet Masses (solar masses)

| Body | Mass (Msun) |
|------|-------------|
| Sun | 1.0 |
| Mercury | 1.659e-7 |
| Venus | 2.447e-6 |
| Earth | 3.0027e-6 |
| Mars | 3.227e-7 |
| Jupiter | 9.543e-4 |
| Saturn | 2.857e-4 |
| Uranus | 4.366e-5 |
| Neptune | 5.151e-5 |

### Velocity Verlet Integrator

```
For each substep (count = clamp(timewarp_rate * 2, 1, 200)):
  1. half-step kick:   v_i(t + dt/2) = v_i(t) + a_i(x(t)) * dt/2
  2. full-step drift:  x_i(t + dt)   = x_i(t) + v_i(t + dt/2) * dt
  3. recompute:        a_i(t + dt)   = nbody_accel(x(t + dt), masses)
  4. half-step kick:   v_i(t + dt)   = v_i(t + dt/2) + a_i(t + dt) * dt/2
```

Substep size = (frame_delta_seconds * timewarp_rate) / substep_count (in simulation days).

### Acceptance Criteria

- [ ] Sun wobbles visibly around barycenter (up to ~1.5 solar radii offset from origin due to Jupiter)
- [ ] Jupiter causes observable perturbations on Mars orbital path over 100+ simulated years (visual deviation from Kepler ellipse)
- [ ] Total orbital energy conserved within 0.1% drift over 1000 simulated years at 1x time scale with default substeps
- [ ] Moons correctly track their parent planet's N-body position (moon orbit follows planet wobble)
- [ ] At high timewarp (10000x), simulation remains stable (substeps increase proportionally to maintain max dt per step of ~0.5 days)
- [ ] Pausing simulation (Space key) freezes all N-body motion instantly
- [ ] Transition from Kepler-only to N-body is seamless (no position jumps on first frame after enabling)
- [ ] Performance: N-body integration completes in <0.5ms per frame at 1x time scale with 9 bodies
- [ ] Energy monitoring shows drift percentage in debug overlay

---

## Phase 2 -- Newtonian Craft and Vacuum Physics

Replace lerp-based craft movement with full Newtonian rigid-body dynamics driven by N-body gravity from Phase 1. Integrate the GLB model as the craft visual. Rewrite craft effects to work with scene traversal instead of per-part queries.

### New Files

| File | Description |
|------|-------------|
| `domain/services/craft_dynamics.rs` | compute_gravitational_accel(craft_pos, nbody_states) -> Vec3; compute_orbital_elements(r_rel, v_rel, mu) -> OrbitalState (sma, ecc, inc, arg_peri, raan, true_anomaly, period, ap, pe); compute_thrust_force(throttle_dir, max_thrust_N) -> Vec3 |
| `infrastructure/bevy_adapters/craft_orbital.rs` | update_craft_orbital_state (finds nearest dominant body by smallest relative distance, computes full orbital parameters for HUD); predict_orbit (forward-integrate craft state for 1 orbital period, returns Vec3 array for prediction line rendering) |
| `domain/services/transfer_orbit.rs` | hohmann_delta_v(r1_au, r2_au, mu) -> (dv1, dv2, tof_days); phase_angle(departure_angle_rad, target_angle_rad, tof_days) -> required_phase_rad; next_transfer_window(current_time_days, departure_body, target_body) -> Option<f64> |

### Files Modified

| File | Change |
|------|--------|
| `craft_components.rs` | Replace linear_velocity, move_input with velocity: Vec3, force_acc: Vec3, torque_acc: Vec3, mass_kg: f32, max_thrust_N: f32, fuel_kg: f32, fuel_consumed_kg: f32; add orbital: Option<OrbitalState>; add CraftState { Flying, Orbiting, Landed }; remove SpeedMode, move_input |
| `craft_systems.rs` | update_craft_physics rewritten: clear accumulators, sum N-body gravity from all bodies, add thrust as force, semi-implicit Euler integration, orbital element computation for nearest body |
| `craft_startup.rs` | Replace 6-part procedural mesh tree with SceneRoot(asset.load("models/ufo_flying_saucer_spaceship_ovni.glb#Scene0")); remove all create_mesh/create_material calls; keep CraftComponent, CraftVisual, CraftCameraTag, Camera setup; keep solar camera deactivation |
| `craft_effects.rs` | Rewrite update_craft_visuals: on first frame after spawn, traverse GLB scene entity hierarchy and store child MeshMaterial3d handles; apply uniform emissive pulse to all child materials; remove CraftPart type matching logic; keep ZPE particle effects (ExpandingRing, SparkParticle) unchanged |
| `craft_ui.rs` | Replace SpeedMode/FlightLabel HUD with OrbitalState section: altitude (km), orbital velocity (km/s), apoapsis (km), periapsis (km), period, inclination (deg), delta-V remaining (m/s). Keep DC/Pulse/Lift/ZPE label rows. |
| `craft_systems.rs` (handle_craft_input) | Remove Shift/Ctrl speed mode keys; throttle mapping sets thrust_fraction 0-1 via W/S + Shift for full thrust; add prediction line toggle (P key); remove CTRL hover logic; R/F remains vertical assist |

### Craft Physics Per Frame (update_craft_physics)

```
1. force_acc = Vec3::ZERO
2. For each NBody body:
     delta = body.pos - craft.pos
     dist_sq = delta.length_squared()
     if dist_sq > 1e-10:
       force_acc += G * body.mass_msun * craft.mass_kg * delta / (dist_sq * sqrt(dist_sq))
3. force_acc += craft.forward * thrust_fraction * max_thrust_N
4. velocity += (force_acc / craft.mass_kg) * dt
5. position += velocity * dt

Orbital detection (nearest body):
  r_rel = craft.pos - body.pos
  v_rel = craft.vel - body.vel
  mu = G * body.mass_msun
  epsilon = 0.5 * v_rel.length_squared() - mu / r_rel.length()
  if epsilon < 0:
    sma = -mu / (2.0 * epsilon)
    period = TAU * sqrt(sma^3 / mu)
    h = r_rel.cross(v_rel)
    ecc_vec = v_rel.cross(h) / mu - r_rel.normalized()
    ecc = ecc_vec.length()
    ap = sma * (1.0 + ecc) - body_radius
    pe = sma * (1.0 - ecc) - body_radius
```

### Acceptance Criteria

- [ ] Craft falls toward planets with gravitational acceleration proportional to body mass and inverse-square of distance
- [ ] Craft can enter a stable orbit around a planet by achieving tangential velocity close to sqrt(G*M/r)
- [ ] HUD shows correct altitude (km), orbital velocity (km/s), apoapsis (km), periapsis (km), orbital period, inclination (deg), delta-V remaining (m/s)
- [ ] Delta-V display decrements as thrust is applied (fuel consumption model)
- [ ] GLB model replaces all 6 procedural meshes with a single scene root; no visible gaps or positioning issues
- [ ] Craft effects (emissive pulse on the whole model, ZPE ring particles, spark particles) all function correctly with GLB scene traversal
- [ ] Prediction line (P key toggle) renders craft's future orbital path for one full period using current state
- [ ] Craft transitions between spheres of influence (e.g., leaves Earth influence, enters Moon influence) with correct reference body switching
- [ ] Performance: craft physics completes in <0.1ms per frame including orbital computation

---

## Phase 3 -- Cinematic Visuals

Add atmospheric scattering, procedural starfield, bloom, lens flare, dynamic clouds, aurora, and shadow maps.

### New Files

| File | Description |
|------|-------------|
| `infrastructure/bevy_adapters/atmosphere_scattering.rs` | Planetary atmosphere shell: translucent sphere at 1.02x planet radius with emissive Fresnel-like glow. Color per planet (Earth=deep blue, Venus=yellow-white, Titan=orange, Mars=red-brown). Opacity modulated by distance from viewer (stronger at terminator). |
| `infrastructure/bevy_adapters/starfield.rs` | spawn_starfield system: 10000 star billboards (unit squares, emissive texture) distributed on celestial sphere radius 1e7 units. Spectral type distribution by real stellar statistics (7% OBA, 44% F-G, 49% K-M). Two layers at different radii for parallax. Subtle twinkle via sine wave on emissive alpha per star. |
| `infrastructure/bevy_adapters/effects_systems.rs` | Sun lens flare: 5-element sprite chain (central glow + 4 ghost flares) along screen-space direction from sun to screen center. BloomSettings applied to craft camera (intensity 0.8, low_frequency_boost 0.7, threshold 0.05). Tonemapping set to TonyMcMapface on both main and craft cameras. |
| `infrastructure/bevy_adapters/dynamic_clouds.rs` | Cloud layer rotation driven by actual rotation_period_hours from PlanetConfig; opacity pulse tied to atmosphere density (delta from mean altitude); Jupiter/Saturn storm band rotation at faster rate than planet spin. |
| `infrastructure/bevy_adapters/aurora.rs` | Polar glow: emissive ring sections at north/south poles for planets with magnetic fields (Earth, Jupiter, Saturn, Uranus, Neptune). Color = green-white (Earth), blue (Uranus/Neptune). Intensity modulated by solar wind simulation (semi-random oscillator, period ~2 minutes). |
| `infrastructure/bevy_adapters/shadow_systems.rs` | Enable shadow mapping on Sun PointLight: set shadows_enabled = true, configure shadow_map_size based on QualityLevel (Ultra=4096, High=2048, Medium=1024, Low=512), set shadow_distance based on camera altitude with smooth transitions |

### Files Modified

| File | Change |
|------|--------|
| `solar_system_startup.rs` | After planet spawn loop: call spawn_starfield(); add atmosphere shell child for Earth, Venus, Titan, Mars; insert BloomSettings on main camera (order=0) and craft camera; set Tonemapping on both cameras |
| `material_factory.rs` | Add create_atmosphere_material(atmosphere_color: Color, opacity: f32) -> StandardMaterial with AlphaMode::Blend, unlit, emissive set to atmosphere_color * intensity |
| `camera_systems.rs` | Configure FogSettings based on camera altitude and nearest planet: deep space = black fog at infinity; within 500km of Earth = blue haze that thickens toward surface |
| `planet_systems.rs` | Add update_planet_clouds: rotates CloudLayer children at rate 2*PI / rotation_period_hours per hour of sim time; add update_aurora: updates aurora mesh emissive based on solar wind oscillator |

### Acceptance Criteria

- [ ] Earth has visible blue atmospheric glow when viewed from space, strongest at the limb/terminator
- [ ] Starfield renders 10000 stars with varying sizes and colors (red giants visible, blue-white hot stars visible); stars twinkle subtly
- [ ] Sun exhibits bloom/glare when craft camera looks toward it; lens flare sprites appear on screen
- [ ] Cloud layers on Earth rotate at rate matching rotation_period_hours
- [ ] Aurora visible at Earth's poles as green-white glow; intensity varies over time
- [ ] Planets cast shadows (Earth shadow on Moon during eclipse geometry; Saturn shadow on rings)
- [ ] Fog transitions smoothly from black space to blue atmospheric haze as craft descends toward Earth
- [ ] Performance: bloom/blend effects maintain 60fps at High quality on target hardware
- [ ] Shadow map quality degrades gracefully (Ultra -> Low) as frame rate drops

---

## Phase 4 -- Exploration and Surface

Enable free flight from solar system scale to planet surface. Terrain rendering with procedural height maps. Landing mechanics. Points of interest.

### New Files

| File | Description |
|------|-------------|
| `domain/services/terrain_generation.rs` | generate_heightmap(resolution: u32, seed: u32) -> Vec<f32> using fractal simplex noise (6 octaves, lacunarity 2.0, persistence 0.5, base frequency 0.003); generate_normal_map(heights, resolution) -> Vec<[f32; 3]> via central differences; generate_biome_map(heights, resolution, latitude) -> Vec<Color> (ice >60 deg, temperate 30-60 deg, desert 0-30 deg, snow at high elevation) |
| `infrastructure/bevy_adapters/surface_systems.rs` | approach_planet: smooth camera zoom from orbit (distance 2x planet radius) to surface (500m altitude) over configurable duration with eased lerp; update_terrain_lod: select terrain patch resolution based on camera altitude (4 chunks, each 64x64 vertices); check_landing: detect craft altitude < surface height + landing_gear_height, set CraftState::Landed, zero velocity |
| `infrastructure/bevy_adapters/poi_systems.rs` | Pre-defined POIs per planet (Olympus Mons, Valles Marineris for Mars; Grand Canyon, Mount Everest for Earth; etc). Procedural POIs: detect highest/lowest point in generated terrain, mark as Peak/Crater. POI markers: emissive ping sprites visible from orbit. Discovery log: first time player approaches within 1km of POI, log to journal. |

### Files Modified

| File | Change |
|------|--------|
| `camera_systems.rs` | Add ApproachPlanet mode: lerp camera toward target planet surface point; add TerrainView mode: FPS-style camera at constant altitude with WASD movement + mouse look |
| `craft_systems.rs` | In update_craft_physics: when CraftState::Landed, set velocity = 0, disable thrust; check surface collision each frame (if craft.y < terrain_height_at_xz, correct position to surface + gear height) |
| `craft_components.rs` | (Already has CraftState { Flying, Orbiting, Landed } from Phase 2) Add landing_gear_height: f32 = 0.5 |
| `terrain_systems.rs` | Extend existing terrain system: integrate LOD chunk generation, streaming heightmap from TerrainGeneration service, bi-level rendering (low-res far, high-res near) |
| `planet_configs.rs` | Add has_landable_surface: bool; add atmosphere_present: bool (for fog transition) |
| `craft_ui.rs` | Add surface HUD when Landed: lat/lon coordinates (calculated from UV mapping), local time (from planet rotation angle + longitude), sun altitude/elevation |

### Acceptance Criteria

- [ ] Camera can smoothly zoom from solar system view to 500m above Earth surface without visible popping
- [ ] Terrain has visible mountains (fractal noise at multiple octaves), craters (procedural depressions at random locations), and latitudinal color variation (ice caps, green bands, deserts)
- [ ] Terrain LOD transitions are seamless (no visible polygon count changes at switch point)
- [ ] Craft can land on planetary surface: contact detection zeroes velocity, landing state activates
- [ ] After landing, HUD shows latitude, longitude, local time, sun altitude
- [ ] POI markers visible from orbit as pulsing emissive points; approaching within 1km triggers journal entry
- [ ] Pre-defined POIs for Earth and Mars appear at correct geographic locations
- [ ] Atmospheric fog transition occurs smoothly during descent (black -> blue for Earth)

---

## Phase 5 -- Science and Education Extension

Extend Phase 0 with orbital analysis instruments and data collection.

### New Files

| File | Description |
|------|-------------|
| `domain/value_objects/science_data.rs` | ScienceReading { body_name: String, category: ScienceCategory, value: f64, unit, confidence: f32 }; ScienceCategory { AtmosphereComposition, MagneticField, SurfaceTemperature, OrbitalParameters, GravitationalField }; ScienceJournal { readings, entries } |
| `infrastructure/bevy_adapters/science_systems.rs` | Collection systems: when craft is within atmosphere of a body, sample composition (simulated from PlanetConfig data); magnetometer reads field strength based on body's magnetic moment and distance; automatic logging to ScienceJournal |
| `presentation/science_panel.rs` | UI panel extending EducationPanel: tab for live instrument readings, tab for journal history, tab for comparative planetology (select two bodies, compare side-by-side) |

### Files Modified

| File | Change |
|------|--------|
| `craft_components.rs` | Add science_instruments: bool (toggleable), current_reading: Option<ScienceReading> |
| `craft_ui.rs` | Add instrument status row on HUD: spectrometer active/inactive, magnetometer field uT, altimeter reading |
| `education_panel.rs` | Add toggle for science panel; wire unlock conditions to science discoveries |
| `planet_configs.rs` | Add science data fields: atmosphere_composition: Vec<(&str, f32)>, magnetic_field_uT: f32, surface_temp_range: (f32, f32) |
| `education_data.rs` | Add 4 new journal entries for science discoveries (unlock on data collection) |

### Acceptance Criteria

- [ ] Flying craft through Earth's atmosphere triggers spectrometer reading showing N2 78%, O2 21%
- [ ] Orbiting Earth with magnetometer active shows magnetic field strength between 25-65 microtesla depending on altitude
- [ ] Science journal logs each reading with timestamp and confidence level
- [ ] Comparative planetology panel shows two bodies side-by-side with key metrics (radius, mass, atmosphere, magnetic field, temperature)
- [ ] On first discovery of each category, journal unlock notification fires
- [ ] Instrument status on HUD updates at 1Hz

---

## Phase 6 -- Predictions and Navigation

Prediction overlays for craft and body trajectories. Transfer window calculator. Eclipse and alignment forecasts.

### New Files

| File | Description |
|------|-------------|
| `infrastructure/bevy_adapters/prediction_systems.rs` | draw_craft_prediction: render line strip (360 segments) showing craft's predicted orbital path for 1 period; draw_body_trail: ring buffer of last 120 positions per NBody body, rendered as fading trail; compute_next_eclipse: for each planet-moon pair, find when moon passes through planet's shadow cone; compute_conjunctions: angular separation between all planet pairs over time range |
| `presentation/transfer_planner.rs` | UI panel: select departure body and target body; compute and display next transfer window date; show delta-V required, time of flight, departure phase angle |
| `domain/services/porkchop.rs` | Generate grid of departure dates x arrival dates; for each cell compute Lambert solution delta-V; return as 2D array for contour rendering |

### Files Modified

| File | Change |
|------|--------|
| `nbody_systems.rs` | After integration step: push current position into ring buffer for each NBody body (max 120 entries); body trail despawns when paused |
| `craft_orbital.rs` | prediction line: when visible, integrate craft forward from current state for 1 orbital period at 360 evenly-spaced steps; return Vec3 array for line renderer |
| `camera_systems.rs` | Auto-follow prediction: when craft prediction is visible, camera can optionally track ahead of craft along predicted path |
| `education_panel.rs` | Add Predictions tab: next transfer window, next eclipse, upcoming conjunctions |
| `presentation/knowledge_journal.rs` | Add entries unlocked by using prediction features |

### Acceptance Criteria

- [ ] Craft orbit prediction line visible and accurate: craft position matches predicted position after advancing time
- [ ] Body trails visible as fading lines behind all 9 NBody bodies (toggleable with T key)
- [ ] Transfer planner shows next Earth-to-Mars transfer window (approx 26 months apart) with correct delta-V (~4.3 km/s for Hohmann)
- [ ] Eclipse predictions appear in body info card: next solar/lunar eclipse with date
- [ ] Conjunction list shows upcoming planetary alignments with angular separation
- [ ] Prediction rendering respects QualityLevel (fewer trail samples at lower quality)

---

## Phase 7 -- UI/UX and Animations

Polish all interactions with smooth transitions, animated data, cinematic camera paths, and accessibility options.

### New Files

| File | Description |
|------|-------------|
| `presentation/animations.rs` | AnimationClip: struct with duration, start_value, end_value, easing (linear, ease_in, ease_out, ease_in_out_cubic); AnimationPlayer: resource managing active clips; helper functions: animate_f32, animate_color, animate_transform over duration with callback |
| `presentation/cinematic.rs` | CinematicSequence: pre-defined camera path using cubic Bezier curve control points; CINEMATIC_INTRO: fly from beyond Pluto toward Earth over 8 seconds; CINEMATIC_FLYBY: arc around selected planet at optimal view distance |

### Files Modified

| File | Change |
|------|--------|
| `presentation/ui_setup.rs` | All panels use AnimationPlayer for open/close: slide in from edge (20px offset, 300ms ease_out), opacity fade (200ms). Notification stack uses slide animation on entrance/exit. |
| `presentation/ui_handlers.rs` | Data values (speed, altitude, delta-V) animate as incrementing/decrementing integer counters over 400ms. Zen mode toggle fades UI opacity smoothly (0 -> 1 over 300ms). |
| `craft_systems.rs` | Camera mode transitions: when camera mode changes, animate from current position/rotation to target over 800ms using ease_in_out_cubic curve (not instant snap) |
| `camera_systems.rs` | auto_inspect_planet uses Bezier camera path (arc around planet at optimal viewing angle) instead of linear lerp; duration based on planet size (larger = slower orbit) |
| `presentation/notifications.rs` | Notifications animate in (slide down from top edge, fade in over 250ms) and animate out (slide up, fade out over 150ms). Stagger when multiple appear. |
| `presentation/knowledge_journal.rs` | Page transitions: fade + slight slide when switching tabs; entry unlock animation: brief scale pulse on new entry |
| `presentation/education_panel.rs` | Panel sections collapse/expand with height animation (200ms ease_out) |

### Accessibility Features

- Colorblind mode: config file toggle shifts palette to CVD-safe colors (green/orange instead of green/red for UI status)
- UI scaling: UiScale resource controlled by Ctrl+Plus/Minus (range 0.8x to 1.5x)
- Key rebinding: CraftControlMap resource loaded from config file; each action key overridable
- High contrast mode: overrides panel background to solid dark and text to white
- Text size: separate scale factor for text within panels (Ctrl+Shift+Plus/Minus)

### Acceptance Criteria

- [ ] All UI panels animate smoothly on open (slide in + fade) and close (slide out + fade)
- [ ] Data counters (speed, altitude) animate as incrementing numbers rather than snapping
- [ ] Camera mode transitions are smooth arcs over 800ms, not instant cuts
- [ ] Flyby camera path arcs around selected planet at optimal viewing angle
- [ ] Notifications slide in/out without overlap; multiple notifications stagger correctly
- [ ] Zen mode (Z key) fades all UI to invisible with smooth opacity transition
- [ ] Colorblind mode toggle shifts palette immediately (no restart required)
- [ ] UI scaling (Ctrl+Plus/Minus) changes all UI element sizes proportionally
- [ ] Key rebinding loads from config file at startup with sane defaults
- [ ] High contrast mode toggle improves readability without breaking layout
- [ ] All animations maintain 60fps; animation system overhead <0.05ms per frame

---

## Integration and Cross-Phase Dependencies

```
Phase 0 (Foundation)
  |
  v
Phase 1 (N-Body) -----> Phase 2 (Craft Dynamics) -----> Phase 4 (Surface)
                              |                              |
                              v                              v
                          Phase 3 (Visuals)             Phase 5 (Science)
                              |                              |
                              v                              v
                          Phase 6 (Predictions) -----> Phase 7 (UI/UX)
```

- Phase 0 has no dependencies and can be implemented first
- Phase 1 must precede Phase 2 (craft needs N-body gravity sources)
- Phase 2 must precede Phase 4 (craft needs physics to fly to surface)
- Phase 3 has weak coupling (visuals overlay existing content) and can be parallelized with Phase 2
- Phase 5 depends on Phase 2 (craft needs to reach bodies) and Phase 0 (journal)
- Phase 6 depends on Phase 1 (N-body positions for prediction) and Phase 2 (craft orbital prediction)
- Phase 7 is a pure polish pass over all existing UI and camera systems

Estimated total new files: ~25
Estimated total modified files: ~30
Estimated new lines of code: ~6000-8000
