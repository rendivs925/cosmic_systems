## Context

`spawn_procedural_launch_pad` places facility children under a pad entity whose
origin is the authoritative terrain anchor. Each child's local origin is the
pad plane, so a mesh whose local minimum Y is not zero floats or sinks by that
amount. The audit reads the actual spawned mesh AABBs, not the constants.

## Goals / Non-Goals

**Goals:**

- Every ground-based facility's lowest vertex is within 0.05 m of the plane.
- The check survives future mesh-size changes.

**Non-Goals:**

- Move mounted items (tower levels, braces, arms, beacons, floodlight heads).
- Change the apron/deck slabs, which are deliberately embedded/proud.
- Address the flame trench/deflector (LF-03) or the structural detail (LF-04).

## Decisions

### Decision: read mesh bounds from the spawned scene

The test spawns the pad exactly as production does, queries `(Name, Transform,
Mesh3d)`, and computes `translation.y + (aabb.min.y) * scale` from the live mesh.
A hardcoded expected height per item would drift with the geometry.

### Decision: a named ground set

Only ground-based structures are checked. The aprons, deck, tower members,
beacons and floodlight heads are mounted or recessed by design, and the flame
deflector is LF-03's subject.

## Risks / Trade-offs

- **Presentation art.** The heights are chosen for the meshes; a future mesh
  resize must update the centre, and the test will fail if it does not.

## Migration Plan

None; presentation only.
