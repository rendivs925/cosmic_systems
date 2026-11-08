## 1. Coverage edge

- [x] 1.1 Wrap the coverage band in a smoothstep, preserving endpoints and mean.
- [x] 1.2 Keep the clear-air threshold and the measured share unchanged.

## 2. Distance filtering

- [x] 2.1 Add `footprint_lod` and the shader/generator resolution constant.
- [x] 2.2 Compute the pixel footprint from the view projection in the deck and
      take the coarser of the step and footprint levels.
- [x] 2.3 Zero the footprint level below one texel so the near deck is unchanged.

## 3. Guards

- [x] 3.1 `coverage_edge_is_a_resolved_quarter_band`.
- [x] 3.2 `cloud_march_filters_to_the_pixel_footprint` (constant match, coarser
      level, compputed from the view).
- [x] 3.3 No extra fetch: the three-fetch guard is unchanged.

## 4. Submission

- [ ] 4.1 Run the full gate in the submission pass.
- [ ] 4.2 Measure the bright-cloud share and capture 400 km and 1,200 km frames.
- [ ] 4.3 Update the five progress documents and archive this change.
