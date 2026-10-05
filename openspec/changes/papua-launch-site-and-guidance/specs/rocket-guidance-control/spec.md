## ADDED Requirements

### Requirement: Ascent target inclination follows the launch site

Ascent and orbit-insertion guidance SHALL derive the target orbital inclination
from the active launch site's latitude rather than a fixed reference value, so
the flown ascent plane matches the site the vehicle launches from.

#### Scenario: Latitude-equal prograde inclination

- **WHEN** ascent guidance is initialized for a launch site at latitude `phi`
- **THEN** the target inclination used for the ascent heading equals `|phi|`
  and a due-east launch reaches that plane

#### Scenario: Unreachable inclination holds vertical

- **WHEN** the derived target inclination cannot produce a safe local launch
  heading
- **THEN** guidance holds the vehicle vertical rather than entering a wrong
  ascent plane

#### Scenario: Telemetry reports the derived target

- **WHEN** telemetry exposes the configured target inclination
- **THEN** the reported value is the inclination derived from the launch site,
  not a hardcoded reference latitude
