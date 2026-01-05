use super::components::*;
use bevy::prelude::*;

// System to apply pending material textures
pub fn apply_pending_material_textures(
    time: Res<Time>,
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    images: Res<Assets<Image>>,
    query: Query<(Entity, &PendingMaterialTextures)>,
    mut throttle: Local<f32>,
) {
    #[cfg(target_arch = "wasm32")]
    let cooldown = 0.5;
    #[cfg(not(target_arch = "wasm32"))]
    let cooldown = 0.0;

    *throttle -= time.delta_seconds();
    if *throttle > 0.0 {
        return;
    }

    for (entity, pending) in query.iter() {
        let wants_base = pending.base_color_texture.is_some() || pending.base_color_path.is_some();
        let wants_emissive = pending.emissive_texture.is_some() || pending.emissive_path.is_some();
        let wants_normal =
            pending.normal_map_texture.is_some() || pending.normal_map_path.is_some();

        let base_ready = if wants_base {
            pending
                .base_color_texture
                .as_ref()
                .and_then(|handle| images.get(handle))
                .is_some()
        } else {
            true
        };
        let emissive_ready = if wants_emissive {
            pending
                .emissive_texture
                .as_ref()
                .and_then(|handle| images.get(handle))
                .is_some()
        } else {
            true
        };
        let normal_ready = if wants_normal {
            pending
                .normal_map_texture
                .as_ref()
                .and_then(|handle| images.get(handle))
                .is_some()
        } else {
            true
        };

        if !base_ready || !emissive_ready || !normal_ready {
            continue;
        }

        if let Some(material) = materials.get_mut(&pending.material) {
            material.base_color_texture = pending.base_color_texture.clone();
            material.emissive_texture = pending.emissive_texture.clone();
            material.normal_map_texture = pending.normal_map_texture.clone();
        }

        commands.entity(entity).remove::<PendingMaterialTextures>();
        *throttle = cooldown;
        break;
    }
}

#[cfg(target_arch = "wasm32")]
pub fn queue_pending_material_textures(
    asset_server: Res<AssetServer>,
    camera_query: Query<&GlobalTransform, With<CameraController>>,
    selected_planet: Res<SelectedPlanet>,
    planet_query: Query<&PlanetComponent>,
    transforms: Query<&GlobalTransform>,
    parents: Query<&Parent>,
    mut pending_query: Query<(Entity, &mut PendingMaterialTextures)>,
    mut texture_worker: NonSendMut<crate::infrastructure::web_workers::texture_worker::TextureDecodeWorker>,
    memory_stats: Option<Res<crate::infrastructure::bevy_adapters::components::WasmMemoryStats>>,
) {
    let camera_pos = camera_query.single().translation();
    let worker_count = texture_worker.worker_count();
    let mut load_budget = if worker_count == 0 {
        1
    } else {
        worker_count.min(4)
    };

    if memory_stats
        .as_ref()
        .is_some_and(|stats| stats.utilization > 0.8)
    {
        return;
    }

    for (entity, mut pending) in pending_query.iter_mut() {
        if load_budget == 0 {
            break;
        }

        let target_entity = if planet_query.get(entity).is_ok() {
            entity
        } else if let Ok(parent) = parents.get(entity) {
            parent.get()
        } else {
            entity
        };

        let mut should_load = pending.eager;
        if let Some(selected) = selected_planet.entity {
            if selected == target_entity {
                should_load = true;
            }
        }

        if !should_load {
            if let Ok(transform) = transforms.get(target_entity) {
                let distance = camera_pos.distance(transform.translation());
                if distance < 200_000.0 {
                    should_load = true;
                }
            }
        }

        if !should_load {
            continue;
        }

        if pending.base_color_texture.is_none() {
            if let Some(path) = pending.base_color_path {
                if let Some(handle) = texture_worker.cached_handle(path) {
                    pending.base_color_texture = Some(handle);
                } else if texture_worker.enabled() {
                    if load_budget > 0 {
                        texture_worker.request(path);
                        load_budget = load_budget.saturating_sub(1);
                    }
                } else {
                    pending.base_color_texture = Some(asset_server.load(path));
                    load_budget = load_budget.saturating_sub(1);
                }
            }
        }

        if load_budget == 0 {
            break;
        }

        if pending.emissive_texture.is_none() {
            if let Some(path) = pending.emissive_path {
                if let Some(handle) = texture_worker.cached_handle(path) {
                    pending.emissive_texture = Some(handle);
                } else if texture_worker.enabled() {
                    if load_budget > 0 {
                        texture_worker.request(path);
                        load_budget = load_budget.saturating_sub(1);
                    }
                } else {
                    pending.emissive_texture = Some(asset_server.load(path));
                    load_budget = load_budget.saturating_sub(1);
                }
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub fn apply_texture_worker_results(
    mut texture_worker: NonSendMut<crate::infrastructure::web_workers::texture_worker::TextureDecodeWorker>,
    mut images: ResMut<Assets<Image>>,
    mut pending_query: Query<&mut PendingMaterialTextures>,
) {
    if !texture_worker.enabled() {
        return;
    }

    for result in texture_worker.take_results() {
        if let Some(error) = result.error {
            web_sys::console::log_1(
                &format!("Texture worker error for {}: {}", result.path, error).into(),
            );
            texture_worker.mark_failed(&result.path);
            continue;
        }

        let Some(bitmap) = result.bitmap else {
            texture_worker.mark_failed(&result.path);
            continue;
        };

        if texture_worker.cached_handle(&result.path).is_some() {
            texture_worker.mark_failed(&result.path);
            continue;
        }

        let image = match texture_worker.decode_bitmap(&bitmap) {
            Ok(image) => image,
            Err(err) => {
                web_sys::console::error_1(&err);
                texture_worker.mark_failed(&result.path);
                continue;
            }
        };

        let handle = texture_worker.cache_image(result.path.clone(), image, &mut images);
        for mut pending in pending_query.iter_mut() {
            if pending
                .base_color_path
                .is_some_and(|path| matches_asset_path(path, &result.path))
            {
                pending.base_color_texture = Some(handle.clone());
            }
            if pending
                .emissive_path
                .is_some_and(|path| matches_asset_path(path, &result.path))
            {
                pending.emissive_texture = Some(handle.clone());
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn matches_asset_path(original: &str, resolved: &str) -> bool {
    if original == resolved {
        return true;
    }
    if original.starts_with("assets/") {
        return false;
    }
    let prefixed = format!("assets/{}", original);
    prefixed == resolved
}