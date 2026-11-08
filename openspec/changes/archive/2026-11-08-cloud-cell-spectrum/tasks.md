## 1. Size spectrum

- [x] 1.1 Re-weight the base Worley octaves with a selector, anchored at the
      existing carve when the selector is zero.
- [x] 1.2 Build the selector from the already-fetched weather alpha and coarse
      Worley channels.
- [x] 1.3 Update the weather-map channel documentation.

## 2. Guards

- [x] 2.1 Shader guard: selector, carve blend, three fetches, no position warp.
- [x] 2.2 The existing rank-uniformity and periodicity tests still cover the
      selector's inputs.

## 3. Submission

- [ ] 3.1 Run the full gate in the submission pass.
- [ ] 3.2 Measure the bright-cloud share and capture a 1,200 km and a chase
      frame.
- [ ] 3.3 Update the five progress documents and archive this change.
