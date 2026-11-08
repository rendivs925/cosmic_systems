## ADDED Requirements

### Requirement: Near-band tree crowns are volumes

A tree crown drawn at or above the full-tree LOD gate SHALL be a closed
volumetric shell, not a stack of crossed billboards. The shell SHALL be wound
outward so it shades around its own silhouette, and its lobe jitter SHALL be a
deterministic function of the patch and the layer so the same patch always
produces the same crown.

The far band SHALL keep crossed impostor cards. Placement, acceptance, species
selection and embed depth SHALL NOT change, and the generator version SHALL NOT
move unless the placement law changes.

The worst-case vegetation mesh reservation SHALL be derived from the crown
geometry constants and SHALL stay within the terrain streaming budget, so a
fully vegetated patch cannot displace the finest terrain LOD.

#### Scenario: Near field

- **WHEN** a tree is drawn at a patch level above the impostor gate
- **THEN** its crown is a closed shell with outward-facing triangles and a lobed
  silhouette

#### Scenario: Far field

- **WHEN** a tree is drawn below the impostor gate
- **THEN** it is still a crossed impostor card

#### Scenario: Budget

- **WHEN** a fully vegetated patch is reserved
- **THEN** the terrain selection still requests the finest patch near the camera
