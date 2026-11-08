## Context

`sample_density` fetches one base Perlin-Worley volume, one detail Worley volume
and one weather map. The base volume's red channel is the remapped density; its
green/blue/alpha channels are Worley fBm octaves at 6/12/24 cells across the
tile. The weather map is periodic and rank-equalised in red/green/blue; alpha is
a raw smooth fBm.

## Goals

- More than one cloud-cell footprint within a single regime.
- No extra fetch in the density loop; no coordinate warp.
- The current field remains the spectrum's anchor, so coverage does not jump.

## Non-Goals

- A second noise volume or a multi-scale sample.
- Changing coverage, the type morph, wind streets or the erosion law.
- Any simulation effect.

## Decisions

### Re-weight the octaves, do not move the sample

The octaves already present in one fetch are genuinely different cell sizes:
6/12/24 cells across an 18 km tile is about 3/1.5/0.75 km. Re-weighting the fBm
that carves `base.r` moves the dominant size between the coarse and fine ends.
Scaling the sample coordinate would do the same thing more directly, but it is a
per-sample warp and the earlier attempt produced visible swirls at `noise_scale`
transitions. This is an amplitude change only.

### Anchor at selector zero

`low_fbm` is the existing carve. `cell_fbm = mix(low_fbm, fine_fbm, selector)`
means selector zero is byte-for-byte the old field, so the coverage invariant is
anchored there and the spectrum only adds fine structure where the selector is
high. Replacing the weights outright would have changed the whole deck and
invalidated the coverage baseline at the same time.

### Regional plus per-cell selector

`regional_size` comes from the weather map's alpha channel, remapped from
0.25-0.75 to 0-1 so its fBm centre does not pin every value at 0.5. Mixing 65 %
of the coarse Worley channel makes adjacent cells differ. Both inputs are already
fetched, so no fetch is added.

### Coverage risk

A finer carve has more small holes, so cells selected to the fine end can lose a
little density. The selector averages near 0.35-0.5 rather than 1, and the
coverage remap follows the blend, so the bright-cloud share is expected to stay
inside its tolerance. It is measured in the submission pass, not asserted here.

## Migration

None. Presentation only; no saved state and no baseline.
