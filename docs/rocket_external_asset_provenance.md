# Rocket External Asset Provenance

This record covers candidate non-terrain sources only. No candidate below is
approved or included in the repository. Every individual asset requires this
review to be completed before it is added, including an exact source URL,
license verification, attribution/notice decision, modification status, and
intended in-game use. Do not substitute the craft/UFO electronic loop for any
Rocket asset.

| Source | Source URL | Exact license / guidance | Attribution and notice | Modification status | Intended use / constraints |
| --- | --- | --- | --- | --- | --- |
| Poly Haven materials | https://polyhaven.com/license | CC0 1.0 Universal (CC0 1.0) Public Domain Dedication | Not required by CC0; retain source record as project provenance. | No specific asset selected or modified. | Candidate materials only. Confirm the individual asset is published by Poly Haven and preserve its source URL before import. |
| Pixabay media | https://pixabay.com/service/license-summary/ | Pixabay Content License | Attribution is not required, but the applicable Pixabay license and its standalone-redistribution, trademark, and other restrictions must be reviewed for the selected item. | No specific asset selected or modified. | Candidate audio, image, or video only. Review the item page and license terms at import time; do not distribute as a standalone asset. |
| NASA media | https://www.nasa.gov/nasa-brand-center/images-and-media/ | NASA media guidance, not a blanket asset license | Follow the item's stated credit line and any additional rights, release, endorsement, NASA insignia, and third-party-content restrictions. | No specific asset selected or modified. | Candidate reference or media only. Verify the individual item's usage terms and provenance; NASA material may include third-party rights. |

## Required Per-Asset Entry

Before adding an external asset, append an entry with the asset name, direct
source URL, exact license text/version, required attribution or notices,
whether and how it was modified, intended repository/runtime use, and reviewer
date. Reject the asset when any of these facts cannot be established.

## Approved Rocket Audio

| Asset | Direct source URL | Exact license / guidance | Attribution and notice | Modification status | Intended use / constraints | Reviewed |
| --- | --- | --- | --- | --- | --- | --- |
| `assets/sounds/rocket_engine_loop.ogg` | Not applicable: original project audio generated locally with FFmpeg `aevalsrc`; source equation is `0.34*tanh(3*(0.45*sin(2*PI*47*t)+0.25*sin(2*PI*94*t)+0.15*sin(2*PI*141*t)+0.12*sin(2*PI*283*t)+0.08*sin(2*PI*611*t)))`, then 28 Hz high-pass and 1.8 kHz low-pass. | Original project asset; no third-party media or license applies. | No external attribution required. | Vorbis-encoded 12-second, periodic engine-loop approximation. | Rocket-mode exterior engine loop only; its gain is attenuated by atmospheric density and observer distance. | 2026-09-11 |
| `assets/sounds/rocket_ignition.ogg` | Not applicable: original project audio generated locally with FFmpeg `aevalsrc`; source equation is `(0.7*exp(-2.1*t)*sin(2*PI*(38+42*t)*t)+0.22*exp(-2.8*t)*sin(2*PI*127*t)+0.18*exp(-3.5*t)*sin(2*PI*431*t))`, then 24 Hz high-pass, 2.4 kHz low-pass, and endpoint fades. | Original project asset; no third-party media or license applies. | No external attribution required. | Vorbis-encoded 2.2-second transition accent. | Spawned only on Rocket ignition/staging state edges; never loops. | 2026-09-11 |
