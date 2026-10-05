//! Pure MVP game-domain value objects.
//!
//! These types describe player-authored vehicles, missions, and progress. They
//! carry no Bevy state and no simulation behaviour; Bevy adapters and the
//! application layer compile and adapt them into authoritative configuration.

pub mod mission;
pub mod parts;
pub mod profile;
pub mod vehicle_draft;
