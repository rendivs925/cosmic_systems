## Context

`LayeredTerrainSource` composes a measured base, optional macro elevation and a
bounded procedural detail layer. The detail is `ridges * 48.0 - drainage * 12.0`,
clamped to `[-36, +24] m`. `height_m` and `mesh_height_m` share
`detail_contribution_m`, so both can apply one guard without a second code path.

The defect is measured: ETOPO 2022 around the Papua site is 73–85 m on the WGS 84
ellipsoid against a 71.43 m EGM2008 sea datum, i.e. 1.6–13.6 m of land, well
inside the ±36 m octave. A ground frame showed angular water bodies on that plain.

## Goals / Non-Goals

**Goals:**

- Make the guard a land rule and leave measured ocean bathymetry untouched.
- Prove, rather than spot-check, that the worst-case detail cannot cross the
  datum inside the window.
- Keep the waterline continuous.
- Preserve all three committed flight baselines bit-for-bit.

**Non-Goals:**

- Correct erosion-carved depressions, river channels or any measured water that
  is already below the datum in the base surface. Those are not the detail
  octave and are tracked separately.
- Change the detail field, its amplitude, its wavelength or its LOD fade.
- Change collision, rendering, water shading or atmosphere.

## Decisions

### Decision: asymmetric removal, not a hard clamp

The removal weight is `1 - smoothstep(sea + guard, sea + full, h)` on land and
`0` once the base is more than one octave below the datum. The distinction from a
hard `max(h + detail, sea)` clamp is deliberate: clamping to the datum would
flatten every near-datum depression into a sea-level pan, while removing the
negative octave keeps the measured base height and only declines to lower it.
This is what the accepted behaviour ("may raise, not lower") asks for.

### Decision: guard window bounded by the layer's negative amplitude

A smoothstep of width `W` has maximum slope `1.5 / W`. The residual negative
contribution `A * smoothstep(...)` therefore rises slower than the base height
`h` whenever `W >= 1.5 * A`. With `A = 36 m`, `full - guard >= 54 m`. Earth uses
`guard = 4 m`, `full = 60 m`, so the inequality holds with margin and the
composed surface cannot cross the datum anywhere in the window.

### Decision: fade below the datum over the octave's own depth

Below the datum the ground is already measured ocean. Suppressing the negative
octave there raises the sea floor by up to 36 m, changes every terrain normal
derived from it, and moved the `leo-insertion-safe` baseline because the fixture
samples the equatorial sea floor. The fade over one octave (`36 m`) restores the
pre-guard bathymetry for all real ocean depths and keeps the waterline value
continuous. The LEO fixture's sampled point is at −4,918 m, so it is now
bit-identical to the committed baseline.

### Decision: constants live with Earth's composition

`EARTH_DETAIL_SEA_GUARD_M` and `EARTH_DETAIL_SEA_GUARD_FULL_M` sit next to the pad
and clearance constants in `catalog.rs`, not in the domain layer, because they
are Earth presentation-calibration values. The mechanism in `layered.rs` takes
them as parameters and stays body-agnostic.

## Risks / Trade-offs

- **Remaining angular water bodies are not fixed by this change.** The guard only
  removes detail-induced crossings. A water body that persists after the guard
  must have its base below the datum (erosion or measurement). The Papua A/B
  shows exactly that: the two large polygons keep their extent, so they are
  recorded as a separate task rather than claimed as fixed.
- **Height changes on near-datum land.** The guard raises the coastal plain by up
  to one octave where negative detail used to apply. That is intended, but any
  frame recorded before the guard must be re-captured rather than reused.
- **The guard is not a bathymetry model.** It does not decide where water is; it
  only refuses to lower land into water.

## Migration Plan

No data or API migration. Set the guard to `None` to restore the previous signed
behaviour exactly. The existing erosion cache namespace is unchanged because the
guard is applied after erosion, not baked into it.

## Open Questions

None.
