## Context

`tree_site_gate` decides whether a lattice site can host a tree. It checked
`source.height_m(lat, lon) < 0.5`. For a procedural body that is height above a
zero datum; for Earth the same field is ellipsoidal and the EGM2008 undulation
at Papua is about 71 m. A shelf 30 m under the sea therefore reads as 41 m above
zero and passes. `select_species` and the layered treeline used the same raw
coordinate.

The stable-lattice work (`stabilize-far-field-tree-placement`) had already
removed the LOD re-roll and the far-field sparsity; this audit closes the datum
defect and records the identity.

## Goals / Non-Goals

**Goals:**

- Make land/sea, species and treeline decisions use height above the local sea
  datum on every body.
- Prove it on a real coastal patch and on a synthetic treeline.
- Version the generator identity.

**Non-Goals:**

- Change the density composition, species profiles, budgets or lattice.
- Add a density overlay shader (none exists; the frames show the placed
  instances and the domain tests prove the signal).
- Change the measured land-cover precedence.

## Decisions

### Decision: subtract the source's datum, not a constant

`source.sea_level_height_m` is the single datum authority (TD-08). The gate and
the species call subtract it; procedural bodies return 0 and are unchanged.

### Decision: the treeline is orthometric

The band is 3,600–4,800 m above sea. Using the ellipsoidal coordinate shifted it
by the local undulation, which is tens of metres — small but wrong at the band
edge, and inconsistent with the gate.

### Decision: version without changing the seed

`VEGETATION_GENERATOR_VERSION = 1` and the existing clump seed and lattice salt
are pinned by a test. A future law change bumps the version; the seed stays so
the rest of the world is comparable.

## Risks / Trade-offs

- **Sparse look remains.** The 600 m frame still reads sparse. That is canopy
  geometry and shading (VG-02/VG-03), not the density gate; the risk is recorded
  rather than hidden.
- **No overlay.** The requested density overlay does not exist; the test and the
  placed-instance frames are the evidence.

## Migration Plan

The datum fix changes placement only where the gate was wrong (shallow shelves,
treeline edge). No persisted data changes.

## Open Questions

None.
