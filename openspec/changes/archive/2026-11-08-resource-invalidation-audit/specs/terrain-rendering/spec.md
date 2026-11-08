## ADDED Requirements

### Requirement: A stationary frame creates and rewrites no render assets

A terrain presentation frame with unchanged inputs SHALL NOT create a mesh,
material or image asset, and SHALL NOT rewrite a uniform whose value is already
current. Creation is permitted only at startup, when a terrain patch becomes
ready, or when an imagery tile becomes ready.

Systems that animate with the presentation clock SHALL still write each live
frame; the requirement applies to inputs that are actually unchanged.

The existence of the guard SHALL be verified by counting created assets across a
stationary run, not by inspection alone.

#### Scenario: Stationary imagery

- **WHEN** a patch's imagery has been applied and 600 further frames run with no
  new tile
- **THEN** the number of terrain material assets is unchanged

#### Scenario: Pinned capture clock

- **WHEN** the presentation clock and the wind are unchanged between frames
- **THEN** the vegetation uniform is not rewritten

#### Scenario: A real change still applies

- **WHEN** the measured wind speed changes
- **THEN** the vegetation uniform is updated on that frame
