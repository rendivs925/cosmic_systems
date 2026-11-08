## ADDED Requirements

### Requirement: The flame trench is an open excavation in the pad

The flame trench SHALL be an excavation in the authoritative terrain surface.
The deck and apron SHALL stop at the trench mouth instead of spanning it, and the
trench lining SHALL be grounded: every lining part either rests on the excavated
floor or spans from that floor to the pad plane. The deflector SHALL rest on the
excavated floor and SHALL stay below the pad plane.

The excavation SHALL NOT cut the launch point. Its near edge SHALL clear the
terrain normal's finite-difference footprint so the resting height, launch
attitude and flight baselines are unchanged.

#### Scenario: Spawned scene

- **WHEN** the procedural pad is spawned at a site
- **THEN** no apron or deck slab overlaps the trench mouth, the lining has one
  floor and four walls, and the deflector sits on the floor below the pad plane

#### Scenario: Launch point

- **WHEN** the excavation is applied to the terrain authority
- **THEN** the height and the surface normal at the launch point are unchanged,
  and the trench mouth starts beyond the hold-down clamps

#### Scenario: Consumers agree

- **WHEN** the mesh, collision and radar altitude sample the pad
- **THEN** they all see the same excavation, because the cut is applied after the
  site grade in the one terrain authority

### Requirement: The rendered terrain yields to the covering structure

A structure that spans ground the streamed mesh cannot resolve SHALL be able to
declare a mesh cutout. The terrain mesh SHALL omit only cells that lie entirely
inside the declared cap, so the opening is a subset of the cap and never escapes
the structure covering it. Coarse patches whose cells do not fit inside the cap
SHALL NOT be cut. The authoritative height field SHALL NOT change: collision,
radar altitude and physics continue to sample the excavation.

The structure SHALL span the cap, and the cap SHALL be large enough that the
guaranteed opening — the cap minus half a cell diagonal at the finest streamed
LOD — still exposes everything the structure covers.

#### Scenario: Launch hardstand

- **WHEN** the launch facility is spawned at a site
- **THEN** the hardstand registers its cutout, the terrain mesh under it loses
  only cells inside the cap, and the below-grade trench is visible

#### Scenario: No facility

- **WHEN** a site has no presented facility
- **THEN** no cutout is registered and the terrain mesh is unchanged

#### Scenario: Distant view

- **WHEN** the pad is covered only by patches coarser than the cap
- **THEN** no cell is removed and the planet keeps a closed surface
