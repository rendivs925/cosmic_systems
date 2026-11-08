## Why

VG-02: in the near band a crown was three stacked crossed billboards plus a
crossed pair at every branch tip. Seen from anything but straight on, a crown is
a set of flat planes: no silhouette, no lit side, and the classic card look up
close. The far band keeps crossed impostor cards, which is correct — they are
what makes a distant forest affordable.

## What Changes

- Add a closed low-poly canopy volume: a jittered ellipsoid shell with outward
  normals, producing one volume per canopy layer in the near band.
- Keep branch-tip lobes as crossed cards. They are small, they sit on a branch,
  and the flat look came from the stacked crown layers.
- Keep the far band on the existing crossed impostor cards.
- Raise the vegetation mesh reservation to the new worst case and keep it inside
  the streaming budget.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `vegetation-placement`: adds a near-band crown geometry requirement.

## Impact

- `src/infrastructure/bevy_adapters/terrain/surface/scatter.rs`: the canopy
  volume primitive and its use for the crown layers.
- `src/infrastructure/bevy_adapters/terrain/surface/mod.rs`: volume constants and
  the per-tree byte reservation.

## Determinism

Placement is untouched: the candidate lattice, the acceptance law, the species
selection and the embed depth are all unchanged, so `VEGETATION_GENERATOR_VERSION`
stays at 1. The volume jitter is a hash of the patch tile and the layer index, so
the same patch always produces the same crown.

## Cost

The conservative reservation moves from 1.08 MB to 1.22 MB per full-density
patch (+13 %). A first attempt with every lobe volumetric (nine shells, +50 %)
pushed the finest terrain patch out of the streaming budget and failed
`papua_prelaunch_selects_and_requests_scatter_lods`; that is why the branch tips
stay cards.
