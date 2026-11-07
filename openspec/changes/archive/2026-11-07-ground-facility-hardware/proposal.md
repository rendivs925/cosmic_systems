## Why

LF-02 audits the launch-site hardware placed by eye rather than derived from the
pad plane. Reading the spawned mesh bounds showed two ground-based items off the
plane: the hold-down clamps floated 0.10 m and the deluge rails were buried
0.25 m. The tanks, blockhouse, floodlight poles and tower legs were already
grounded.

## What Changes

- Ground the hold-down clamps (centre 0.5 → 0.4 m for a 0.8 m body).
- Ground the deluge rails (centre 0.45 → 0.7 m for a 1.4 m body).
- Add a spawned-scene test that reads each ground-based facility's mesh AABB and
  asserts its base is within 0.05 m of the pad plane.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `launch-site-presentation`: adds a grounded-hardware requirement.

## Impact

- `src/application/rocket_spawning.rs`: two transforms, one test.
- Presentation only; no collision, heights, physics or baseline change.
