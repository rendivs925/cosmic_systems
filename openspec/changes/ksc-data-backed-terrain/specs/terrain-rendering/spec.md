## ADDED Requirements

### Requirement: Local elevation terrain has one visible surface authority

The system SHALL present valid local measured terrain through the terrain patch
lifecycle and SHALL NOT overlap it with a second visual terrain mesh for the
same coverage.

#### Scenario: Local terrain replaces coarse presentation
- **WHEN** a local elevation terrain patch becomes visible
- **THEN** its mesh is sourced from the active terrain source without z-fighting
  against an independently loaded local terrain mesh
