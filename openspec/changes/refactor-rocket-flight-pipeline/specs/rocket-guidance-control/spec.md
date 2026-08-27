## ADDED Requirements

### Requirement: Guidance uses authoritative flight conditions
Guidance SHALL use the authoritative atmosphere-relative dynamic pressure when evaluating ascent, reentry, and descent constraints.

#### Scenario: Reentry dynamic-pressure gate
- **WHEN** reentry guidance evaluates a vehicle traversing the atmosphere
- **THEN** its phase and bank constraints use dynamic pressure derived from the current atmosphere-relative velocity
