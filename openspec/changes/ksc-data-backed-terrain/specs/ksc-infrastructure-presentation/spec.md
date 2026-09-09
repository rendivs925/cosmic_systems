## Purpose

Defines bounded, geodetically registered KSC visual infrastructure that enriches
the launch scene without becoming terrain or physics authority.

## ADDED Requirements

### Requirement: KSC infrastructure uses body-fixed placement

The system SHALL place KSC presentation assets from documented Earth body-fixed
or geodetic coordinates and the same Earth orientation used by terrain.

#### Scenario: Infrastructure follows Earth rotation
- **WHEN** Earth presentation updates at a new simulation epoch
- **THEN** KSC infrastructure remains registered to the same terrain location

### Requirement: Infrastructure is presentation-only

The system SHALL keep KSC structures separate from bare-earth elevation,
collision, rocket altitude, and landing authority.

#### Scenario: Terrain collision remains data-backed
- **WHEN** the rocket samples ground beneath a KSC structure
- **THEN** collision uses the active Earth terrain source rather than the visual asset
