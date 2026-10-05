## REMOVED Requirements

### Requirement: Measured local elevation overrides global elevation where covered

**Reason**: The local measured-elevation overlay was introduced only for a
KSC-scoped terrain package that does not match the active Papua launch site.
The launch site uses the existing global/procedural Earth elevation source, so
the overlay path is unused and is removed to avoid dead, untested authority.

**Migration**: Earth elevation continues to come from the global payload/coarse
source with deterministic procedural fallback. If a reviewed local elevation
package is ever required, it must be reintroduced as a new, site-matched
capability with its own provenance and seam requirements.
