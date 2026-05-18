#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CraftKind {
    Saucer,
    Triangle,
    Tripod,
}

#[derive(Debug, Clone)]
pub struct Craft {
    pub name: String,
    pub kind: CraftKind,
    pub mass_tonnes: f32,
    pub weight_kilonewtons: f32,
}

impl Craft {
    pub fn saucer() -> Self {
        Self {
            name: "Saucer".into(),
            kind: CraftKind::Saucer,
            mass_tonnes: 7.8,
            weight_kilonewtons: 17.2,
        }
    }
}
