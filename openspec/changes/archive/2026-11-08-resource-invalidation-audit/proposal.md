## Why

RD-01: the terrain presentation path was reported to recreate meshes,
materials and buffers for unchanged inputs. Reading every `Update` system
against the asset stores found that creation is already confined to startup and
to a patch or imagery tile arriving; the remaining defect was a per-frame
uniform rewrite that forces a re-extraction even when nothing changed.

## Audit

`TerrainRenderPlugin` registers these `Update` systems:

- `recenter_render_origin` - writes a resource, creates nothing.
- `update_vegetation_material` - **wrote the shared uniform every frame**,
  including when a pinned capture clock and stationary wind left every input
  identical. Fixed here.
- `update_terrain_material_origin` - already guarded by `TerrainMaterialFrame`:
  the full derived frame is compared and equal frames skip.
- `update_terrain_cloud_shadow` - compares the field before assigning.
- `update_water_material` - the wave clock advances every live frame, so a write
  is required; it creates nothing.
- `bind_cloud_noise_to_ground_materials` - guarded by `CloudNoiseBinding::bound`.
- `stream_terrain_patches` / `spawn_patch_mesh_system` - creation is keyed to a
  `TerrainPatchReady` event.
- `apply_terrain_imagery` - guarded by the applied tile and blend; it is the one
  per-frame system that can rebuild a material, and only for a newly ready tile.
- `stream_terrain_imagery` - allocates an image only for a newly admitted tile.

Startup asset creation, per-patch creation and per-tile creation remain released
work: they are the product of the stream, not of a stationary frame.

## What Changes

- Skip the vegetation uniform write when every field already matches.
- Add a 600-frame counter regression on the imagery material path.
- Add a vegetation no-op/still-applies test.

## Capabilities

### Modified Capabilities

- `terrain-rendering`: adds a stationary-frame invalidation requirement.

## Impact

- `src/infrastructure/bevy_adapters/terrain/render.rs`: the guard and its test.
- `src/infrastructure/bevy_adapters/terrain/imagery.rs`: the 600-frame count.
