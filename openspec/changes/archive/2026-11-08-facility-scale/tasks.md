## 1. Defects

- [x] 1.1 Derive the hold-down clamp ring from the hull radius.
- [x] 1.2 Choose the tower level count from the vehicle height, targeting a
      12 m bay within 3-9 levels.
- [x] 1.3 Derive the service-arm levels from the level count.
- [x] 1.4 Scale the water tanks, blockhouse and floodlights with the vehicle.

## 2. Verification

- [x] 2.1 `tower_scale_follows_every_vehicle_family` — level count in range, bay
      height in 4-20 m, platform edge clear of the hull, arms reach the surface.
- [x] 2.2 `spawned_facility_scales_with_every_vehicle_family` — four clamps, every
      clamp inner face outside the hull, platform count and extent match the
      layout.
- [x] 2.3 Existing attachment and grounding tests still pass.
- [x] 2.4 Flight baselines untouched.

## 3. Evidence and records

- [x] 3.1 One ground frame per vehicle family.
- [x] 3.2 Update the five progress documents.
