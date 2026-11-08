## Context

The pad is presented once per launch from the selected vehicle's height and
diameter, and the same pad geometry serves all four catalogue families. Nothing
in the facility is a measurement of a real pad, so the standard is internal
consistency: hardware sized to the vehicle, and no member detached from what
carries it.

## Goals

- No facility part can intersect the vehicle.
- Bay heights stay in a band a structure would actually have, across an 18 m to
  121 m catalogue.
- Every structural member starts and ends on real structure.
- One test per family, not one test on a 60 m rocket.

## Non-Goals

- Surveyed dimensions of any real pad.
- A per-vehicle pad layout.
- Changing the hardstand or the flame trench.

## Decisions

### The clamp ring follows the hull

The clamp distance is `hull radius + 0.35 m clearance + half the clamp width`.
Clamps are placed on the pad axes, so the closest point of each box is exactly
`hull radius + 0.35 m` from the vehicle axis. A diagonal arrangement would need
the inner *corner* test instead, and a fixed offset cannot serve a 1.2 m and a
9 m vehicle at once.

### Level count, not bay height, is the free variable

`height_m` is fixed by the rule that the top platform sits below the nose
(`0.82 × vehicle height`). Bay height follows from the level count, so the count
is chosen to land nearest a 12 m bay within 3–9 levels. That keeps the tower
recognisable at both ends of the catalogue.

### Service-arm levels derive from the count

Two arms on the upper and lower thirds of the tower. Deriving them from the count
removes the out-of-bounds index that a short tower exposed.

### The hardstand does not scale

A terrain-mesh cutout is only applied to cells lying entirely inside its cap, so
the guaranteed opening is the cap minus half a cell diagonal — about 36.5 m at
Earth's finest streamed LOD. The trench reaches 28 m, so the cap cannot go much
below 45 m and the hardstand must span it. A small launcher therefore gets a
shared, medium-lift-sized hardstand. Shrinking it is a terrain-mesh change, not a
presentation one.

## Risks

- Scaling support structures to 0.6× puts 1.4 m tank radii and a 2.4 m blockhouse
  on the pad for Electron. They read as small hardware rather than as missing
  hardware, which is the better failure.
- The tower's 3-level floor makes Electron's tower sparse. Its vehicle is 18 m
  tall, so a taller tower would be out of proportion.

## Migration

None. Presentation only; no saved state, no baseline.
