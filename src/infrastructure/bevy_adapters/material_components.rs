use bevy::prelude::*;

// Component for pending material textures
#[derive(Component)]
pub struct PendingMaterialTextures {
    pub material: Handle<StandardMaterial>,
    pub base_color_texture: Option<Handle<Image>>,
    pub normal_map_texture: Option<Handle<Image>>,
    pub emissive_texture: Option<Handle<Image>>,
    pub base_color_path: Option<&'static str>,
    pub normal_map_path: Option<&'static str>,
    pub emissive_path: Option<&'static str>,
    pub eager: bool,
}

// Component for pending orbit mesh generation
#[derive(Component)]
pub struct PendingOrbitMesh {
    pub mesh: Handle<Mesh>,
    pub orbit_shape: crate::domain::services::physics::OrbitShape,
    pub color: Color,
    pub segments: usize,
}