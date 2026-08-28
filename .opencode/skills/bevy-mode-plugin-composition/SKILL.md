---
name: bevy-mode-plugin-composition
description: Use when changing Bevy plugins, application modes, ECS components/resources, system schedules, system sets, startup composition, or shared craft/rocket/solar infrastructure in Cosmic Systems.
---

# Bevy Mode And Plugin Composition

Use this skill for application composition and ECS architecture. The project is
one simulator with shared infrastructure, not separate solar, craft, and rocket
applications.

## Existing Authority

`src/infrastructure/plugins/mod.rs` owns application composition. It already
defines shared simulation composition and mode-specific plugins. Inspect it,
`src/application/modes.rs`, `src/main.rs`, and `src/systems/sets.rs` before
adding plugins, resources, components, schedules, or modes.

Preserve all modes:

```text
cargo run             normal solar-system simulation
cargo run -- craft    existing craft simulation
cargo run -- rocket   rocket flight simulation
```

## Composition Rules

- Shared celestial bodies, physical scale, time, assets, coordinate conventions,
  rendering base, and common presentation remain shared.
- Craft-specific and rocket-specific behaviour remain isolated in their mode
  plugins. Do not copy shared systems to make a mode work.
- Add a plugin for a meaningful feature boundary, not every file or component.
- Plugins register resources, events/messages, assets, systems, sets, and startup
  composition. They are not service managers.
- Reuse an existing plugin/resource first. Create new global state only when it
  is genuinely singular and shared.

## ECS Rules

- Components contain cohesive entity-local data, not behaviour or managers.
- Resources are global/shared state, not maps of every rocket or terrain patch.
- Systems perform one conceptual operation, request only required query data, and
  avoid unnecessary mutable access.
- Use events/messages for meaningful completed occurrences; use queries/resources
  for current state.
- Do not create ECS entities for terrain vertices, droplets, or other data-array
  internals. Use normal Rust structures for those domain details.
- Do not add locks/global managers where normal ECS data and scheduling suffice.

## Schedule Discipline

```text
Startup:      setup, configuration validation, asset/bootstrap work
FixedUpdate:  authoritative fixed simulation
Update:       input, UI, camera, streaming decisions, variable-rate presentation
PostUpdate:   presentation synchronization where necessary
```

- Express all causal ordering with `.chain()`, `.before()`, `.after()`, or named
  system sets. Never rely on registration order.
- Add meaningful sets only for real pipeline stages. Extend `RocketSet` for rocket
  fixed simulation rather than making an unrelated schedule.
- Keep input capture separate from fixed mutation and simulation separate from
  presentation interpolation.
- Avoid serializing independent systems; explicit ordering is for real data flow.

## Change Audit

Before changing application architecture, state:

1. Existing composition owner and systems/resources already serving the need.
2. Whether the capability is shared, normal-only, craft-only, or rocket-only.
3. The minimum extension path.
4. Required scheduling/order constraints.
5. All affected modes and tests.

After the change, validate all three mode startup paths. If a mode cannot be
visually inspected, verify successful initialization/logs and state the limit.

Reject duplicate mode systems, mode-specific copies of shared physics, giant
resources, manager objects wrapping ECS queries, hidden schedule dependencies,
and authoritative simulation in `Update` solely for convenience.
