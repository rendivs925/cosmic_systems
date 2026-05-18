use crate::infrastructure::bevy_adapters::craft_components::CraftControlState;
use bevy::prelude::*;

#[derive(Component)]
pub struct CraftGlowMaterial(pub Handle<StandardMaterial>);

pub fn update_craft_visuals(
    control: Res<CraftControlState>,
    craft_query: Query<&CraftGlowMaterial>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let dc = control.dc_current;
    let emissive_base = 0.2 + dc * 0.8;

    for glow in craft_query.iter() {
        if let Some(mat) = materials.get_mut(&glow.0) {
            let r = emissive_base * 0.3;
            let g = emissive_base * 0.1;
            let b = emissive_base * 0.05;
            mat.emissive = LinearRgba::new(r, g, b, 1.0);
            mat.base_color = Color::srgba(0.0, 0.0, 0.0, 0.0);
        }
    }
}
