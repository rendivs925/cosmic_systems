use bevy::prelude::*;
use std::f32::consts::TAU;
#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;

pub struct PlanetTextureSet {
    pub albedo: Option<&'static str>,
    pub emissive: Option<&'static str>,
}

pub struct CloudLayerConfig {
    pub texture_path: &'static str,
    pub alpha: f32,
    pub rotation_period_hours: f32,
    pub scale: f32,
}

pub fn orbit_motion_params(
    name: &str,
    orbital_distance_au: f32,
    is_moon: bool,
) -> OrbitMotionParams {
    let base = orbit_hash(name, 1);
    let offset = orbit_hash(name, 7);
    let max_tilt = if is_moon { 0.28 } else { 0.16 };
    let tilt = Vec2::new(
        (base * 2.0 - 1.0) * max_tilt,
        (offset * 2.0 - 1.0) * max_tilt,
    );
    let wobble_amount = if is_moon { 0.06 } else { 0.035 };
    let wobble_speed = 0.05 + base * 0.12 + orbital_distance_au * 0.002;
    let spin_speed = 0.02 + offset * 0.05;
    let phase = base * TAU;

    OrbitMotionParams {
        tilt,
        wobble_speed,
        wobble_amount,
        spin_speed,
        phase,
    }
}

pub fn orbit_hash(name: &str, seed: u32) -> f32 {
    let mut hash = 2166136261u32 ^ seed;
    for byte in name.bytes() {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(16777619);
    }
    (hash % 10_000) as f32 / 10_000.0
}

#[derive(Clone, Copy, Debug)]
pub struct OrbitMotionParams {
    pub tilt: Vec2,
    pub wobble_speed: f32,
    pub wobble_amount: f32,
    pub spin_speed: f32,
    pub phase: f32,
}

pub fn get_planet_textures(planet_name: &str) -> PlanetTextureSet {
    match planet_name {
        "Sun" => PlanetTextureSet {
            albedo: Some("textures/planets/sun/albedo.png"),
            emissive: None,
        },
        "Earth" => PlanetTextureSet {
            albedo: Some("textures/planets/earth/albedo.png"),
            emissive: Some("textures/planets/earth/emissive.png"),
        },
        "Mercury" => PlanetTextureSet {
            albedo: Some("textures/planets/mercury/albedo.png"),
            emissive: None,
        },
        "Venus" => PlanetTextureSet {
            albedo: Some("textures/planets/venus/albedo.png"),
            emissive: None,
        },
        "Mars" => PlanetTextureSet {
            albedo: Some("textures/planets/mars/albedo.png"),
            emissive: None,
        },
        "Jupiter" => PlanetTextureSet {
            albedo: Some("textures/planets/jupiter/albedo.png"),
            emissive: None,
        },
        "Saturn" => PlanetTextureSet {
            albedo: Some("textures/planets/saturn/albedo.png"),
            emissive: None,
        },
        "Uranus" => PlanetTextureSet {
            albedo: Some("textures/planets/uranus/albedo.png"),
            emissive: None,
        },
        "Neptune" => PlanetTextureSet {
            albedo: Some("textures/planets/neptune/albedo.png"),
            emissive: None,
        },
        "Moon" => PlanetTextureSet {
            albedo: Some("textures/planets/moon/albedo.png"),
            emissive: None,
        },
        "Phobos" => PlanetTextureSet {
            albedo: Some("textures/planets/phobos/albedo.png"),
            emissive: None,
        },
        "Deimos" => PlanetTextureSet {
            albedo: Some("textures/planets/deimos/albedo.png"),
            emissive: None,
        },
        "Io" => PlanetTextureSet {
            albedo: Some("textures/planets/io/albedo.png"),
            emissive: None,
        },
        "Europa" => PlanetTextureSet {
            albedo: Some("textures/planets/europa/albedo.png"),
            emissive: None,
        },
        "Ganymede" => PlanetTextureSet {
            albedo: Some("textures/planets/ganymede/albedo.png"),
            emissive: None,
        },
        "Callisto" => PlanetTextureSet {
            albedo: Some("textures/planets/callisto/albedo.png"),
            emissive: None,
        },
        "Mimas" => PlanetTextureSet {
            albedo: Some("textures/planets/mimas/albedo.png"),
            emissive: None,
        },
        "Enceladus" => PlanetTextureSet {
            albedo: Some("textures/planets/enceladus/albedo.png"),
            emissive: None,
        },
        "Tethys" => PlanetTextureSet {
            albedo: Some("textures/planets/tethys/albedo.png"),
            emissive: None,
        },
        "Dione" => PlanetTextureSet {
            albedo: Some("textures/planets/dione/albedo.png"),
            emissive: None,
        },
        "Rhea" => PlanetTextureSet {
            albedo: Some("textures/planets/rhea/albedo.png"),
            emissive: None,
        },
        "Titan" => PlanetTextureSet {
            albedo: Some("textures/planets/titan/albedo.png"),
            emissive: None,
        },
        "Hyperion" => PlanetTextureSet {
            albedo: Some("textures/planets/hyperion/albedo.png"),
            emissive: None,
        },
        "Iapetus" => PlanetTextureSet {
            albedo: Some("textures/planets/iapetus/albedo.png"),
            emissive: None,
        },
        "Miranda" => PlanetTextureSet {
            albedo: Some("textures/planets/miranda/albedo.png"),
            emissive: None,
        },
        "Ariel" => PlanetTextureSet {
            albedo: Some("textures/planets/ariel/albedo.png"),
            emissive: None,
        },
        "Umbriel" => PlanetTextureSet {
            albedo: Some("textures/planets/umbriel/albedo.png"),
            emissive: None,
        },
        "Titania" => PlanetTextureSet {
            albedo: Some("textures/planets/titania/albedo.png"),
            emissive: None,
        },
        "Oberon" => PlanetTextureSet {
            albedo: Some("textures/planets/oberon/albedo.png"),
            emissive: None,
        },
        "Triton" => PlanetTextureSet {
            albedo: Some("textures/planets/triton/albedo.png"),
            emissive: None,
        },
        "Proteus" => PlanetTextureSet {
            albedo: Some("textures/planets/proteus/albedo.png"),
            emissive: None,
        },
        "Nereid" => PlanetTextureSet {
            albedo: Some("textures/planets/nereid/albedo.png"),
            emissive: None,
        },
        "Larissa" => PlanetTextureSet {
            albedo: Some("textures/planets/larissa/albedo.png"),
            emissive: None,
        },
        _ => PlanetTextureSet {
            albedo: None,
            emissive: None,
        },
    }
}

pub fn get_cloud_layer_config(planet_name: &str) -> Option<CloudLayerConfig> {
    match planet_name {
        "Earth" => Some(CloudLayerConfig {
            texture_path: "textures/planets/earth/clouds.png",
            alpha: 0.65,
            rotation_period_hours: 24.0,
            scale: 1.012,
        }),
        "Venus" => Some(CloudLayerConfig {
            texture_path: "textures/planets/venus/clouds.png",
            alpha: 0.4,
            rotation_period_hours: 96.0,
            scale: 1.02,
        }),
        "Titan" => Some(CloudLayerConfig {
            texture_path: "textures/planets/titan/clouds.png",
            alpha: 0.45,
            rotation_period_hours: 382.0,
            scale: 1.02,
        }),
        _ => None,
    }
}

pub fn get_ring_texture_path(planet_name: &str) -> Option<&'static str> {
    match planet_name {
        "Saturn" => Some("textures/planets/saturn/rings.png"),
        _ => None,
    }
}

pub fn asset_exists(path: &str) -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = path;
        true
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("assets")
            .join(path)
            .exists()
    }
}

pub fn load_texture(
    asset_server: &AssetServer,
    path: Option<&'static str>,
) -> Option<Handle<Image>> {
    let path = path?;
    if asset_exists(path) {
        Some(asset_server.load(path))
    } else {
        None
    }
}
