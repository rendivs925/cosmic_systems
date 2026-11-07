## Why

VG-01 asked where vegetation should exist and whether the density signal
produces it. The lattice, ecological gate, species law and budgets were already
in place; the audit found one datum defect and two missing pieces of evidence.

**Defect.** The gate compared the raw `height_m` with 0.5 m. On Earth, heights
are ellipsoidal and the EGM2008 undulation is tens of metres, so a shallow shelf
at 40 m ellipsoidal is 30 m below the sea and still passed the land/sea test. A
Papua coastal patch accepted a tree 5.2 m below the sea. The species selection
and the treeline used the same raw coordinate.

**Evidence gap.** No test tied the gate to the source's sea datum on real data,
and the generator identity was not versioned.

## What Changes

- The tree gate, species selection and vegetation treeline use height above the
  source's local sea datum.
- Add `VEGETATION_GENERATOR_VERSION` as the committed identity of the placement
  generator alongside the seed.
- Add a real-DEM coastal exclusion test, a treeline density test, and a version
  identity test.
- Archive the completed `stabilize-far-field-tree-placement` change (the stable
  lattice and far-field density work that VG-01 depended on).

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `vegetation-placement`: adds a sea-datum exclusion requirement and a versioned
  generator identity.

## Impact

- `src/domain/services/vegetation.rs`: version constant and identity test.
- `src/infrastructure/bevy_adapters/terrain/surface/scatter.rs`: datum-correct
  gate and species height, coastal test.
- `src/domain/services/terrain_source/layered.rs`: datum-correct treeline and
  test.
- No physics, collision, or flight-baseline change.
