## 1. Audit

- [x] 1.1 List every `Update` system and classify it as create, rewrite or
      guarded.
- [x] 1.2 Record the startup, per-patch and per-tile creation paths as stream
      work rather than per-frame work.

## 2. Fix

- [x] 2.1 Skip the vegetation uniform write when all fields already match.
- [x] 2.2 Preserve a real update when the wind or clock changes.

## 3. Regression

- [x] 3.1 `stationary_vegetation_uniform_is_not_rewritten`.
- [x] 3.2 Extend the imagery upgrade test to 600 stationary frames and count
      material assets.

## 4. Submission

- [ ] 4.1 Run the full gate in the submission pass.
- [ ] 4.2 Record the counter values over a 600-frame offscreen run.
- [ ] 4.3 Update the five progress documents and archive this change.
