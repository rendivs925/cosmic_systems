## ADDED Requirements

### Requirement: Rocket camera mode changes are continuous
Rocket-mode camera changes SHALL transition from the currently rendered camera pose to the destination mode pose without modifying flight simulation state.

#### Scenario: Camera mode switch in ascent
- **WHEN** a user selects another rocket camera mode during ascent
- **THEN** the camera blends continuously from its displayed pose while continuing to follow the rendered rocket state

#### Scenario: Retargeted camera transition
- **WHEN** a user selects a second camera mode before a prior transition completes
- **THEN** the new transition starts from the current rendered camera pose without a discontinuity
