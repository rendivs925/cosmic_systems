## MODIFIED Requirements

### Requirement: Thrust follows the rocket equation
The system SHALL compute thrust from mass flow rate and pressure-selected specific impulse as `T = m_dot * Isp * g0`. Engine catalog thrust SHALL represent full-throttle sea-level thrust; ambient pressure SHALL select the interpolated sea-level/vacuum ISP while the calibrated propellant mass flow remains pressure-independent.

#### Scenario: Thrust from ISP and mass flow
- **WHEN** an engine is throttled to a given level
- **THEN** thrust equals mass flow times pressure-selected specific impulse times standard gravity

#### Scenario: Vacuum ISP in space
- **WHEN** the rocket is above the significant atmosphere
- **THEN** thrust uses the vacuum ISP value and the same calibrated mass flow as at sea level

#### Scenario: Sea-level ISP in atmosphere
- **WHEN** the rocket is at standard sea-level pressure
- **THEN** thrust uses the configured sea-level ISP and catalog sea-level thrust

#### Scenario: Gimbal torque matches ambient thrust
- **WHEN** an engine gimbals at a non-sea-level ambient pressure
- **THEN** the torque calculation uses the same pressure-selected thrust as the translational force
