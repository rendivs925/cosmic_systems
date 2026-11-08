## Context

Presentation assets live in `Assets<Mesh>`, `Assets<TerrainMaterial>` and
`Assets<Image>`. Bevy re-extracts an asset when it is mutated, so an assignment
that writes the same value is not free. The telemetry already counts created
assets per frame (`mesh_assets_created`, `material_assets_created`,
`image_assets_created`).

## Goals

- A stationary frame creates nothing and rewrites nothing it did not need to.
- The guard must not cache stale state.

## Non-Goals

- Caching the animated water, cloud and sun uniforms; their clocks advance.
- Removing the per-patch creation path; it is the stream's product.
- A windowed 600-frame baseline, which needs P0-04.

## Decisions

### Compare, then write

The vegetation system builds the full uniform from the shared wind, the render
origin and the presentation clock. Those are exactly its inputs, so comparing all
of them is a sound invalidation test. The earlier unconditional write marked the
material modified every frame even during a pinned-clock capture, where the value
was constant.

### Count assets, not frames

A 600-frame test over the imagery upgrade path counts the length of
`Assets<TerrainMaterial>`. It is the one per-frame system that can rebuild a
material, and a stationary patch must leave the count unchanged. This runs
without a GPU because the system only needs the asset stores and the patch state.

## Risks

- An equality check with a float can miss a change that is real but bit-identical
  in the uniform; that change would not be visible anyway.
- The water and cloud clocks are deliberately not cached, so they still rewrite
  every live frame. That is required animation, not waste.

## Migration

None.
