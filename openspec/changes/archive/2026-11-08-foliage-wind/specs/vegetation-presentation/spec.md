## ADDED Requirements

### Requirement: Foliage wind is one vector shared with the cloud deck

The vegetation sway direction SHALL be the same measured climatology direction
that the cloud deck is advected with. The deck SHALL publish the exact vector it
applies, and the vegetation material SHALL consume that published vector rather
than sampling the climatology independently.

Foliage amplitude and oscillation rate SHALL be derived from the measured
surface wind speed through bounded, monotone mappings. The direction SHALL be
shared exactly; the deck's mid-tropospheric advection speed and the surface wind
speed MAY differ.

Wind SHALL remain presentation only. It SHALL NOT affect placement, acceptance,
species selection, collision, radar altitude, or any authoritative terrain
sample, and it SHALL NOT rebuild geometry per frame.

#### Scenario: Foliage and clouds agree

- **WHEN** a measured climatology sample is resident
- **THEN** the vegetation sway direction equals the cloud deck's advection
  direction for that frame

#### Scenario: Stronger wind bends further

- **WHEN** the measured surface wind speed rises
- **THEN** the canopy displacement and the oscillation rate rise monotonically
  within their declared bounds

### Requirement: Sway follows the attachment

Every vegetation vertex SHALL carry a sway weight that reflects what it is
attached to: a grounded tuft or far-field impostor SHALL be pinned at its base, a
crown lobe SHALL start at the sway weight of its branch tip, and a canopy volume
SHALL sway less at its underside than at its top.

Thin leaf and grass surfaces SHALL carry full translucency weight; woody
vertices SHALL carry a low or zero translucency weight.

#### Scenario: Grounded tuft

- **WHEN** a grass card is emitted
- **THEN** its base corners have sway weight zero and its top corners have weight
  one

#### Scenario: Crown lobe

- **WHEN** a branch-tip canopy lobe is emitted
- **THEN** its base sways with the same weight as the branch tip it hangs from
