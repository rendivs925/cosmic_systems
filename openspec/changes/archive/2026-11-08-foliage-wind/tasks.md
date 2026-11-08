## 1. Shared wind

- [x] 1.1 Add a `PresentationWind` resource with a body-fixed direction, the
      measured surface speed, and a measured/fallback flag.
- [x] 1.2 Return the applied direction from the deck climate mapping and publish
      that exact vector from the cloud system.
- [x] 1.3 Read the shared wind in the vegetation material system and order it
      after the cloud update.

## 2. Sway model

- [x] 2.1 Add bounded surface-speed to displacement and oscillation-rate
      mappings; the 10 m/s fallback reproduces the tuned defaults.
- [x] 2.2 Grade card sway weight from the attachment weight to full weight.
- [x] 2.3 Grade canopy-volume sway from the underside to the top.
- [x] 2.4 Derive branch sway from named root/tip constants so a crown lobe cannot
      detach from its branch.

## 3. Submission

- [ ] 3.1 Run the full gate at the end of the implementation pass.
- [ ] 3.2 Capture a fixed-pose phase pair and measure the canopy shift.
- [ ] 3.3 Update the five progress documents and archive this change.
