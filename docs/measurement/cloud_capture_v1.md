# Deterministic planet-scale cloud capture

Status: implemented, used for the cloud-deck measurements from `ffa76d7`
onwards.

## Why this exists

Every flight camera frames the vehicle. The widest available is the free
camera, clamped to 40 km from the rocket, which spans about 100 km of deck —
roughly one mesoscale cloud regime. Cloud morphology is regional by design
(cloud-engine specification, section 3.3.1), so two things were impossible to
check from a chase view:

- that a deck really does hold several cloud types at once (the whole point of
  the regional morph), and
- the share of sky the deck covers *away* from the launch site, where the
  measured climatology is only sampled at one point.

The second problem is the harder one: a before/after comparison of a shader
change is confounded by the presentation clock. Every animated shader (cloud
drift, sea waves, vegetation sway) advances with `Time::elapsed_secs()`, which
counts from when the application started updating. A capture waits on wall
clock while startup varies by tens of seconds, so two runs of the same build
render different moments of the same animation.

## The facility

Two environment variables, both off by default:

| Variable | Meaning |
| --- | --- |
| `COSMIC_SYSTEMS_CAPTURE_POSE="<lat>,<lon>,<altitude_m>"` | Place the active flight camera on a fixed body-fixed direction, looking straight down at the planet centre. |
| `COSMIC_SYSTEMS_CAPTURE_TIME=<seconds>` | Pin the presentation clock, so every animated shader is at a fixed phase. |

```sh
COSMIC_SYSTEMS_CAPTURE_POSE="-8.0,139.5,1200000" \
COSMIC_SYSTEMS_CAPTURE_TIME=42 \
    cargo run --features dem -- rocket --site papua
```

The pose is derived only from the site, the planet radius and the ephemeris —
never from the vehicle — so it does not depend on how far a run got before the
capture. While a capture pose is active the deck's measured climate is also
sampled at that site rather than under the vehicle, so the coverage does not
drift as the vehicle flies out of it.

Projection is rebuilt for the view: the near plane moves out with the altitude
and the far plane is set past the limb, because the flight camera's far plane is
derived from vehicle altitude and would clip the deck from 1,200 km.

Presentation only: nothing here can reach the simulation, the terrain source, or
collision state. Unset, the systems do nothing and the normal cameras and clock
are untouched.

## Measured

From 1,200 km over the Papua site (`-8.0, 139.5`):

| Metric | Value |
| --- | --- |
| Frame cloud share | 0.258 |
| Local cloud fraction, 6 x 8 grid, p10 / p50 / p90 | **0.00 / 0.23 / 0.70** |
| Cloud blob area, p10 / p50 / p90 | 265 / 1 008 / 28 549 px |

The spread across the grid is the point: one view holds genuinely clear cells and
near-overcast cells, and blob areas span two orders of magnitude. That is what
"the deck holds several cloud types" has to mean, and it cannot be shown from a
100 km chase view, where the frame reads one morph (type probe: 0.74-0.77 over
90% of the frame).

Repeatability, two consecutive runs with identical environment: **89.8% of
pixels bit-identical, 0.27% differing by more than 8/255, mean absolute
difference 0.20**. The residual is terrain streaming and the sun disc, not the
cloud field; the deck region matches. That is enough for a before/after of a
cloud change, which is what the harness is for.

## Limits

- The camera is static, so the terrain behind the deck streams at one LOD set;
  a change that only affects terrain will show residual variation.
- The vehicle still flies. Its sprite and effects move between captures; crop or
  ignore the centre of the frame for cloud measurements.
- The view is nadir only. A view along the limb would need a pitch parameter,
  which nothing has needed yet.
