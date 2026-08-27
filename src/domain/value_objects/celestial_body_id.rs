//! Typed identifier for configurable celestial bodies.

use std::fmt;

/// A non-empty celestial-body identifier used at simulation boundaries.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CelestialBodyId(String);

impl CelestialBodyId {
    pub fn new(name: impl Into<String>) -> Result<Self, &'static str> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err("celestial body identifier cannot be empty");
        }
        Ok(Self(name))
    }

    pub fn earth() -> Self {
        Self("Earth".to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CelestialBodyId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_blank_identifiers() {
        assert!(CelestialBodyId::new(" ").is_err());
    }
}
