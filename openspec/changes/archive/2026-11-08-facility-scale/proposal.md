## Why

LF-04 audits the facility against the four catalogue vehicles: Electron
(18 m × 1.2 m), Falcon 9 (70 × 3.7), SLS (98 × 8.4) and Starship (121 × 9.0).
Reading the spawned geometry found three real scale defects.

1. The hold-down clamps sat at a fixed ±3 m. Their inner faces were at 2.4 m, so
   they were **inside** the 4.2 m SLS and 4.5 m Starship hulls.
2. The tower always had exactly five platforms. An 18 m Electron therefore got
   2.95 m bays and a 121 m Starship got 19.8 m bays — a ladder at one end and
   three isolated shelves at the other.
3. The service arms indexed platform 3 unconditionally, so a tower with fewer
   than four platforms panicked. Scaling the level count exposed this at once.

## What Changes

- Derive the hold-down clamp ring from the hull radius, so no vehicle family can
  have a clamp inside its body.
- Choose the tower level count from the vehicle's tower height, targeting a
  12 m bay and bounded to 3–9 levels, and derive the service-arm levels from that
  count.
- Scale the pad hardware that belongs to the vehicle — water tanks, blockhouse,
  floodlight poles and heads — to its height, bounded to 0.6–1.3×.
- Extend the tower and spawned-scene tests across all four families.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `launch-site-presentation`: adds a vehicle-scaling requirement for the
  facility.

## Impact

- `src/application/launch_tower.rs`: level count, bay height, arm levels.
- `src/application/rocket_spawning.rs`: clamp ring, support-structure scale, two
  tests.
- Presentation only.

## Limits

The hardstand and the flame trench stay fixed. The trench width is what the
terrain-mesh cutout contract can guarantee at the finest streamed LOD (about
36.5 m of certain opening against a 28 m trench), so it cannot shrink with a
small launcher. An 18 m Electron therefore stands on a pad sized for a
medium-lift vehicle. That is a shared-site decision, recorded rather than hidden.
