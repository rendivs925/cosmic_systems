## Why

The project has a credible rocket simulation but not yet a game loop: players cannot build a vehicle, fly it directly, plan a mission, or retain the result. A focused Earth-orbit-and-return MVP turns the existing physics, staging, terrain, and telemetry foundations into a playable 3D spaceflight experience before expanding to the Moon, more bodies, or a full campaign.

## What Changes

- Add an in-game vehicle assembly workflow using the existing validated `RocketCatalog`/RON vehicle definitions as its single configuration authority.
- Add direct player flight controls for throttle, attitude, staging, fairing, gear, parachutes, and time warp, with optional stability, ascent, and landing assists.
- Add an orbital planning/map experience with exact apoapsis/periapsis information, maneuver planning, execution guidance, and player-readable flight telemetry.
- Add a four-mission Earth vertical slice: reach space, achieve a safe low Earth orbit, deploy a payload, and recover a capsule by controlled entry/parachute landing.
- Add mission state, debrief scoring, unlocks, and local persistence for vehicle builds, settings, and completed missions.
- Add game-specific presentation and flow: title/menu, vehicle/mission selection, responsive HUD, camera transitions, failure/retry feedback, and accessible desktop controls.

## Capabilities

### New Capabilities

- `vehicle-assembly`: Assemble, validate, preview, and launch a player vehicle from a bounded MVP part catalog.
- `player-flight-controls`: Convert desktop player input into authoritative rocket commands and optional flight-assist requests.
- `orbital-mission-planning`: Provide an interactive orbital map, maneuver plans, execution cues, and clear flight telemetry.
- `mission-progression`: Define the Earth vertical-slice missions, objectives, debrief results, rewards, and unlocks.
- `game-persistence`: Persist player settings, saved vehicle builds, completed missions, and unlocked content locally.
- `flight-game-presentation`: Provide game flow, usable HUD/camera controls, feedback, and readable failure/retry states for rocket mode.

### Modified Capabilities

- `rocket-guidance-control`: Guidance/control/actuation must preserve physics authority while supporting explicit player command ownership and optional assists.
- `rocket-mode`: Rocket mode must provide the game entry flow while retaining its shared solar-system composition and preserving the normal and craft modes.

## Impact

- Extends `RocketCatalog` and RON definitions instead of creating a second vehicle/part model; the current Falcon-specific visual builder must become configuration-driven for MVP parts.
- Adds game-domain state and local serialization, plus Bevy UI/input systems scoped to rocket mode.
- Extends the existing fixed-update command pipeline and telemetry/orbit presentation without allowing UI, guidance, or map systems to write physical state directly.
- Reuses existing terrain collision, propulsion/staging, entry/recovery, cameras, trajectory prediction, and flight recorder for missions and debriefs.
- Requires new desktop input, UI, asset/audio, and save-data tests; no multiplayer, economy simulation, modding, lunar launch flow, n-body gameplay, or real-time weather is part of this MVP.
