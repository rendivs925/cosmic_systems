# Structural Refactor Report

Date: 2026-09-06

## Purpose

Reorganize the Rust and Bevy source tree so module locations express their
actual ownership without changing simulation behavior, asset paths, CLI modes,
or physics authority.

## Completed Changes

### Application Composition

- Moved shared, solar-system, craft, and rocket Bevy plugin composition from
  `src/infrastructure/plugins/mod.rs` to `src/application/plugins.rs`.
- Updated `src/main.rs` to consume the library crate instead of declaring a
  duplicate binary module tree.
- Exposed the existing vehicle-selection catalog API required by the binary.

### Bevy Adapter Ownership

- Grouped craft adapters under `src/infrastructure/bevy_adapters/craft/`.
- Grouped rocket adapters, components, events, system sets, and pipeline tests
  under `src/infrastructure/bevy_adapters/rocket/`.
- Grouped terrain adapters under `src/infrastructure/bevy_adapters/terrain/`.
- Grouped material, mesh, and texture factories under
  `src/infrastructure/bevy_adapters/rendering/`.
- Moved the presentation-owned education state to
  `src/presentation/education_state.rs`.

### Removed Legacy Facades

- Removed `src/components/` and `src/systems/` after relocating rocket
  components and `RocketSet`.
- Removed `src/infrastructure/bevy_adapters/components.rs`.
- Updated all consumers, including integration tests, to import directly from
  their owning module. No compatibility aliases or re-exports were retained.

### Supporting Files

- Moved the texture download helper to `scripts/assets/download_textures.sh`.
- Archived stale planning documents under `docs/archive/legacy/`.
- Updated active OpenCode skills to reference the new module paths.
- Left archived OpenSpec artifacts unchanged because they document historical
  implementation state.

## Preserved Behavior

- Fixed rocket pipeline ordering and `RocketSet` scheduling.
- Authoritative domain physics, gravity, reference frames, and terrain source.
- Normal, craft, and rocket application modes.
- Existing CLI behavior, vehicle configuration paths, assets, and RON schemas.
- Simulation versus presentation authority boundaries.

## Validation

The refactor passed the following commands with the `dem` feature enabled:

```text
cargo fmt --check
cargo check --features dem
cargo clippy --features dem -- -D warnings
cargo test --features dem
cargo build --release --features dem
```

`cargo test --features dem` completed with 615 passing tests.

Each application mode was started with a 15-second bound:

```text
cargo run --features dem
cargo run --features dem -- craft
cargo run --features dem -- rocket
```

All modes created their expected Bevy windows. Rocket mode loaded the
ephemeris assets, initialized terrain streaming, and reached liftoff.

The runs retained existing non-fatal warnings from external kernel metadata,
the unavailable Earth-orientation dataset, X11 settings, and gamepad mapping.

## Residual Risks And Follow-Up

- Some domain entities and value objects still use Bevy types. They were not
  moved because that would be a separate dependency-boundary refactor rather
  than a mechanical ownership migration.
- The refactor is currently uncommitted.
- Restart OpenCode before relying on the updated project skills; skills are
  loaded when OpenCode starts.
