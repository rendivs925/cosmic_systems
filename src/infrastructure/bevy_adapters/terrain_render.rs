//! Terrain rendering plugin (AGENTS.md sections 27-28).
//!
//! Spawns GPU meshes and materials for cube-sphere LOD terrain patches from the
//! streaming manager, with PBR shaders for planetary surfaces and a floating
//! origin for precision at planetary scale.

use crate::domain::services::body_orientation::BodyOrientation;
use crate::domain::services::cube_sphere::{PatchGeometry, TerrainPatch};
use crate::domain::services::reference_frames::body_fixed_to_planet_inertial_rotation;
use crate::infrastructure::bevy_adapters::components::*;
use crate::infrastructure::bevy_adapters::ephemeris::EphemerisSnapshot;
use crate::infrastructure::bevy_adapters::terrain_streaming::{
    stream_terrain_patches, TerrainStreamingResource,
};
use bevy::asset::{Assets, RenderAssetUsages};
use bevy::ecs::message::Message;
use bevy::math::{DQuat, DVec3};
use bevy::pbr::{ExtendedMaterial, MaterialExtension, MaterialPlugin};
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::shader::ShaderRef;
use bevy_mesh::{Indices, PrimitiveTopology};
use std::collections::{HashMap, HashSet, VecDeque};

const TERRAIN_SURFACE_SHADER: &str = "shaders/terrain_surface.wgsl";
/// Spreading texture creation and GPU asset uploads across frames prevents a
/// completed terrain batch from stalling camera and HUD presentation.
const MAX_PATCH_UPLOADS_PER_FRAME: usize = 2;
/// Ready messages are coalesced and publication backfill makes a rejected entry
/// retryable, so this cap bounds memory without dropping visible terrain forever.
const MAX_PENDING_PATCH_UPLOADS: usize = 512;

/// Cached parents stay visible until every visible descendant has a render
/// entity. CPU streaming readiness alone is not sufficient: asset creation is
/// deliberately spread across frames.
#[derive(Resource, Default)]
struct PendingTerrainPatchHides(HashSet<TerrainPatchRenderKey>);

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
struct TerrainSurfaceExtension {
    #[texture(100)]
    #[sampler(101)]
    local_albedo: Handle<Image>,
    #[texture(102)]
    #[sampler(103)]
    local_normal: Handle<Image>,
    #[uniform(104)]
    local_detail_weight: f32,
}

impl MaterialExtension for TerrainSurfaceExtension {
    fn fragment_shader() -> ShaderRef {
        TERRAIN_SURFACE_SHADER.into()
    }
}

type TerrainMaterial = ExtendedMaterial<StandardMaterial, TerrainSurfaceExtension>;

/// Component tracking the render state of a terrain patch.
#[derive(Component, Debug, Clone)]
pub struct TerrainPatchRenderState {
    pub patch: TerrainPatch,
    pub mesh_handle: Handle<Mesh>,
    material_handle: Handle<TerrainMaterial>,
    /// Per-patch local surface textures. Shared fallback maps are not stored or
    /// released with a patch.
    local_surface_handles: Option<(Handle<Image>, Handle<Image>)>,
    pub vegetation_mesh_handle: Option<Handle<Mesh>>,
    pub planet_entity: Entity,
    /// Body-fixed-to-inertial rotation used to bake this mesh's vertices.
    pub body_to_inertial_at_spawn: DQuat,
    /// Render origin used to bake this mesh's vertices.
    pub render_origin_at_spawn: DVec3,
}

/// Reusable render assets whose appearance is identical for every terrain
/// patch. Patch terrain materials stay independent because roughness is derived
/// from their geographic surface sample.
#[derive(Resource, Default)]
struct TerrainRenderAssets {
    vegetation_material: Option<Handle<StandardMaterial>>,
    fallback_surface_maps: Option<(Handle<Image>, Handle<Image>)>,
}

/// Identifies a terrain render entity independently for every planet. Patch
/// coordinates alone overlap between planets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct TerrainPatchRenderKey {
    planet_entity: Entity,
    patch: TerrainPatch,
}

impl From<&TerrainPatchReady> for TerrainPatchRenderKey {
    fn from(event: &TerrainPatchReady) -> Self {
        Self {
            planet_entity: event.planet_entity,
            patch: event.patch,
        }
    }
}

/// Direct lifecycle lookup avoids scanning every render entity per event.
#[derive(Resource, Default)]
struct TerrainPatchRenderIndex(HashMap<TerrainPatchRenderKey, Entity>);

/// Ready patches wait here until their CPU-to-GPU asset creation budget is
/// available. Messages expire after two frames, so the queue owns pending
/// uploads and coalesces repeated ready notifications.
#[derive(Resource, Default)]
struct PendingTerrainPatchUploads {
    queue: VecDeque<TerrainPatchReady>,
    queued: HashSet<TerrainPatchRenderKey>,
}

impl PendingTerrainPatchUploads {
    fn retain_published_for_planet(
        &mut self,
        active_planet: Option<Entity>,
        published: &std::collections::BTreeSet<TerrainPatch>,
    ) {
        self.queue.retain(|event| {
            active_planet == Some(event.planet_entity) && published.contains(&event.patch)
        });
        self.queued = self.queue.iter().map(TerrainPatchRenderKey::from).collect();
    }

    fn enqueue(&mut self, event: TerrainPatchReady) -> bool {
        let key = TerrainPatchRenderKey::from(&event);
        if self.queued.contains(&key) || self.queue.len() >= MAX_PENDING_PATCH_UPLOADS {
            return false;
        }
        self.queued.insert(key);
        self.queue.push_back(event);
        true
    }

    fn pop_front(&mut self) -> Option<TerrainPatchReady> {
        let event = self.queue.pop_front()?;
        self.queued.remove(&TerrainPatchRenderKey::from(&event));
        Some(event)
    }
}

/// Resource for the floating render origin (AGENTS.md section 13).
/// When the camera moves far from the origin, we re-center to avoid f32
/// precision loss.
#[derive(Resource, Debug, Default)]
pub struct RenderOrigin {
    pub origin: DVec3,
    pub last_camera_pos: DVec3,
}

/// Configuration for terrain rendering.
#[derive(Resource, Debug, Clone)]
pub struct TerrainRenderConfig {
    /// Distance threshold (meters) beyond which the render origin re-centers.
    pub recenter_threshold_m: f64,
    /// Skirt depth for LOD crack hiding (meters).
    pub skirt_depth_m: f64,
    /// Patch resolution (vertices per side).
    pub patch_resolution: u32,
    /// Finest visible leaves use this resolution for locally smooth terrain.
    /// Coarser fallback coverage retains `patch_resolution` for responsiveness.
    pub detail_patch_resolution: u32,
    /// First level using `detail_patch_resolution`.
    pub detail_patch_min_level: u32,
}

impl Default for TerrainRenderConfig {
    fn default() -> Self {
        Self {
            recenter_threshold_m: 10_000.0,
            skirt_depth_m: 5.0,
            // 2^n + 1 samples preserve parent/child boundary sample alignment.
            // Local LOD supplies spatial detail; keeping each worker bake at 33
            // samples avoids delayed viewport publication and upload bursts.
            patch_resolution: 33,
            // Align denser geometry with the first local surface map level so
            // material normals, terrain silhouette, and scatter refine together.
            detail_patch_resolution: 65,
            detail_patch_min_level: 12,
        }
    }
}

impl TerrainRenderConfig {
    pub(crate) fn patch_resolution_for(&self, patch: TerrainPatch) -> u32 {
        if patch.level >= self.detail_patch_min_level {
            self.detail_patch_resolution
        } else {
            self.patch_resolution
        }
    }
}

/// Events emitted by the streaming system when patch lifecycle changes.
/// These are observed by the render system to spawn/despawn meshes.
#[derive(Message, Debug, Clone)]
pub struct TerrainPatchReady {
    pub patch: TerrainPatch,
    pub planet_entity: Entity,
}

/// Emitted when a generated patch leaves the active leaf cover but remains in
/// the streaming cache. Its GPU assets stay resident for a zero-regeneration
/// return to visibility.
#[derive(Message, Debug, Clone)]
pub struct TerrainPatchCached {
    pub patch: TerrainPatch,
    pub planet_entity: Entity,
}

#[derive(Message, Debug, Clone)]
pub struct TerrainPatchEvicted {
    pub patch: TerrainPatch,
    pub planet_entity: Entity,
}

/// Plugin that registers terrain rendering systems for the rocket mode.
///
/// The render origin follows the rocket's inertial physical position. Resident
/// patch roots are rebased when it moves, while physical coordinates remain f64.
pub struct TerrainRenderPlugin;

impl Plugin for TerrainRenderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RenderOrigin>()
            .init_resource::<TerrainRenderConfig>()
            .init_resource::<TerrainRenderAssets>()
            .init_resource::<PendingTerrainPatchUploads>()
            .init_resource::<PendingTerrainPatchHides>()
            .init_resource::<TerrainPatchRenderIndex>()
            .add_plugins(MaterialPlugin::<TerrainMaterial>::default())
            .add_message::<TerrainPatchReady>()
            .add_message::<TerrainPatchCached>()
            .add_message::<TerrainPatchEvicted>()
            .add_systems(
                Update,
                recenter_render_origin.before(stream_terrain_patches),
            )
            .add_systems(
                Update,
                (
                    update_patch_transforms,
                    reveal_cached_patch_mesh_system,
                    spawn_patch_mesh_system,
                    hide_cached_patch_mesh_system,
                    despawn_patch_mesh_system,
                )
                    .chain()
                    .after(stream_terrain_patches),
            );
    }
}

/// System that spawns Bevy mesh/material entities when a terrain patch
/// becomes ready in the streaming lifecycle.
#[expect(
    clippy::too_many_arguments,
    reason = "This renderer upload system coordinates independent terrain assets, events, and state."
)]
fn spawn_patch_mesh_system(
    mut commands: Commands,
    mut events: MessageReader<TerrainPatchReady>,
    mut pending_uploads: ResMut<PendingTerrainPatchUploads>,
    mut render_index: ResMut<TerrainPatchRenderIndex>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<TerrainMaterial>>,
    mut standard_materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut render_assets: ResMut<TerrainRenderAssets>,
    mut streaming: ResMut<TerrainStreamingResource>,
    _config: Res<TerrainRenderConfig>,
    render_origin: Res<RenderOrigin>,
    ephemeris_snapshot: Res<EphemerisSnapshot>,
    planet_query: Query<&PlanetComponent>,
) {
    let active_planet = streaming.active_planet();
    pending_uploads.retain_published_for_planet(active_planet, &streaming.published);
    for event in events.read().cloned() {
        if active_planet == Some(event.planet_entity) && streaming.published.contains(&event.patch)
        {
            pending_uploads.enqueue(event);
        }
    }
    // A bounded queue can reject a burst of ready events. Refill from the
    // streaming resource's authoritative visible set so those patches retry.
    if let Some(planet_entity) = active_planet {
        for patch in streaming.published.iter().copied() {
            let event = TerrainPatchReady {
                patch,
                planet_entity,
            };
            if !render_index
                .0
                .contains_key(&TerrainPatchRenderKey::from(&event))
            {
                pending_uploads.enqueue(event);
            }
        }
    }
    for _ in 0..MAX_PATCH_UPLOADS_PER_FRAME {
        let Some(event) = pending_uploads.pop_front() else {
            break;
        };
        // A patch can leave the viewport while waiting in the upload queue.
        // Do not make an obsolete ready message visible after its cache event
        // has already been handled.
        if streaming.active_planet() != Some(event.planet_entity)
            || !streaming.published.contains(&event.patch)
        {
            continue;
        }
        let key = TerrainPatchRenderKey::from(&event);
        if render_index.0.contains_key(&key) {
            continue;
        }
        let Ok(planet) = planet_query.get(event.planet_entity) else {
            continue;
        };
        let Some(orientation) =
            ephemeris_snapshot.orientation_for_catalog_body(&planet.domain_planet.name)
        else {
            continue;
        };

        let patch = event.patch;
        let Some(cached_geometry) = streaming.generated.get_mut(&patch) else {
            continue;
        };
        let Some(surface) = cached_geometry.surface.take() else {
            continue;
        };
        let geometry = &cached_geometry.geometry;

        // Build Bevy mesh from PatchGeometry, rebasing the planet-centered
        // geometry into the rocket-local flight frame so f32 mesh vertices stay
        // small near the camera (avoids precision loss at ~6371 km magnitudes
        // that degrades the sphere into a flat plane with broken triangles).
        let body_to_inertial = body_fixed_to_planet_inertial_rotation(orientation);
        let mesh = patch_geometry_to_mesh(
            geometry,
            &render_origin.origin,
            body_to_inertial,
            &surface.vertex_colors,
        );
        let mesh_handle = meshes.add(mesh);

        // Every LOD starts from the shared terrain appearance carried by its
        // vertex colors. A separate global Earth image conflicts with that
        // authority at refined patch boundaries.
        let base_material = patch_material(surface.roughness, surface.metallic);
        let (local_albedo, local_normal, local_detail_weight, local_surface_handles) =
            if let Some((albedo, normal)) = surface.local_surfaces {
                let albedo = images.add(albedo);
                let normal = images.add(normal);
                (albedo.clone(), normal.clone(), 1.0, Some((albedo, normal)))
            } else {
                let (albedo, normal) = fallback_surface_maps(&mut render_assets, &mut images);
                (albedo, normal, 0.0, None)
            };
        let material_handle = materials.add(TerrainMaterial {
            base: base_material,
            extension: TerrainSurfaceExtension {
                local_albedo,
                local_normal,
                local_detail_weight,
            },
        });

        // Geometry is already in the rocket-local flight frame; the entity sits
        // at the origin (the rocket's render position).
        let transform = Transform::IDENTITY;

        let vegetation = surface.vegetation;
        let vegetation_mesh_handle = vegetation
            .as_ref()
            .map(|(mesh, _)| meshes.add(mesh.clone()));
        let vegetation_material = vegetation_mesh_handle.as_ref().map(|_| {
            render_assets
                .vegetation_material
                .get_or_insert_with(|| {
                    standard_materials.add(StandardMaterial {
                        base_color: Color::WHITE,
                        perceptual_roughness: 0.9,
                        metallic: 0.0,
                        ..default()
                    })
                })
                .clone()
        });

        // A departing parent remains the visible fallback until a complete
        // descendant cover has reached the renderer. Spawning each replacement
        // hidden avoids depth fighting while the upload budget spreads that
        // cover across multiple frames.
        let visibility = if has_departing_ancestor_render_entity(
            patch,
            event.planet_entity,
            &streaming.published,
            &render_index,
        ) {
            Visibility::Hidden
        } else {
            Visibility::Visible
        };

        let entity = commands
            .spawn((
                Mesh3d(mesh_handle.clone()),
                MeshMaterial3d(material_handle.clone()),
                transform,
                TerrainPatchRenderState {
                    patch,
                    mesh_handle: mesh_handle.clone(),
                    material_handle: material_handle.clone(),
                    local_surface_handles,
                    vegetation_mesh_handle: vegetation_mesh_handle.clone(),
                    planet_entity: event.planet_entity,
                    body_to_inertial_at_spawn: body_to_inertial,
                    render_origin_at_spawn: render_origin.origin,
                },
                visibility,
                Name::new(format!(
                    "TerrainPatch_{:?}_{}_{}_{}",
                    patch.face, patch.level, patch.tile_x, patch.tile_y
                )),
            ))
            .id();
        render_index.0.insert(key, entity);

        // Merged vegetation + scatter (trees, rocks) is one child draw and
        // shares an immutable material across every terrain tile.
        if let (Some(vegetation_mesh_handle), Some(vegetation_material), Some((_, anchor))) = (
            vegetation_mesh_handle.clone(),
            vegetation_material,
            vegetation,
        ) {
            commands.entity(entity).with_children(|parent| {
                parent.spawn((
                    Mesh3d(vegetation_mesh_handle),
                    MeshMaterial3d(vegetation_material),
                    Transform::from_translation(
                        (body_to_inertial * anchor - render_origin.origin).as_vec3(),
                    )
                    // Vegetation vertices are body-fixed offsets from `anchor`.
                    // Rotate those offsets into the same inertial frame as the
                    // terrain patch before the parent applies later pose updates.
                    .with_rotation(body_to_inertial.as_quat()),
                    Name::new(format!(
                        "Vegetation_{:?}_{}_{}_{}",
                        patch.face, patch.level, patch.tile_x, patch.tile_y
                    )),
                ));
            });
        }
    }
}

/// Hide cached tile entities without destroying their mesh/material assets.
/// The ready handler restores these entities instead of rebuilding them.
fn hide_cached_patch_mesh_system(
    mut events: MessageReader<TerrainPatchCached>,
    mut pending_hides: ResMut<PendingTerrainPatchHides>,
    streaming: Res<TerrainStreamingResource>,
    mut render_index: ResMut<TerrainPatchRenderIndex>,
    mut render_query: Query<&mut Visibility>,
) {
    for event in events.read() {
        pending_hides.0.insert(TerrainPatchRenderKey {
            planet_entity: event.planet_entity,
            patch: event.patch,
        });
    }

    let pending: Vec<_> = pending_hides.0.iter().copied().collect();
    for key in pending {
        if streaming.published.contains(&key.patch) {
            pending_hides.0.remove(&key);
            continue;
        }
        // A refinement parent remains visible until every quadrant is covered
        // by an uploaded published descendant. Checking every direct region
        // recursively prevents a partial child set from exposing a hole.
        let has_replacements = streaming
            .published
            .iter()
            .any(|patch| patch.level > key.patch.level && key.patch.is_ancestor_of(patch));
        if has_replacements
            && !published_cover_is_renderable(
                key.patch,
                key.planet_entity,
                &streaming.published,
                &render_index,
            )
        {
            continue;
        }
        let Some(entity) = render_index.0.get(&key).copied() else {
            pending_hides.0.remove(&key);
            continue;
        };
        if let Ok(mut visibility) = render_query.get_mut(entity) {
            *visibility = Visibility::Hidden;
        } else {
            render_index.0.remove(&key);
            pending_hides.0.remove(&key);
            continue;
        }
        reveal_published_descendants(
            key.patch,
            key.planet_entity,
            &streaming.published,
            &render_index,
            &mut render_query,
        );
        pending_hides.0.remove(&key);
    }

    // Publication is authoritative. Repair a stale hidden state only when no
    // visible parent is still covering this patch's area during a refinement
    // handoff.
    for key in render_index.0.keys().copied().collect::<Vec<_>>() {
        if !streaming.published.contains(&key.patch)
            || !streaming
                .active_planet()
                .is_none_or(|active| active == key.planet_entity)
            || has_visible_departing_ancestor(
                key.patch,
                key.planet_entity,
                &streaming.published,
                &render_index,
                &mut render_query,
            )
        {
            continue;
        }
        if let Some(entity) = render_index.0.get(&key).copied() {
            if let Ok(mut visibility) = render_query.get_mut(entity) {
                *visibility = Visibility::Visible;
            }
        }
        pending_hides.0.remove(&key);
    }
}

/// Restore a cached tile before the ready handler considers creating new GPU
/// assets. A cache hit therefore performs no mesh conversion or asset upload.
#[expect(
    clippy::too_many_arguments,
    reason = "This cache handoff coordinates streaming publication, ephemeris pose, and render entity state."
)]
fn reveal_cached_patch_mesh_system(
    mut events: MessageReader<TerrainPatchReady>,
    mut pending_hides: ResMut<PendingTerrainPatchHides>,
    streaming: Res<TerrainStreamingResource>,
    ephemeris_snapshot: Res<EphemerisSnapshot>,
    render_origin: Res<RenderOrigin>,
    planet_query: Query<&PlanetComponent>,
    mut render_index: ResMut<TerrainPatchRenderIndex>,
    mut render_query: Query<(&TerrainPatchRenderState, &mut Transform, &mut Visibility)>,
) {
    for event in events.read() {
        let key = TerrainPatchRenderKey::from(event);
        pending_hides.0.remove(&key);
        let Some(entity) = render_index.0.get(&key).copied() else {
            continue;
        };
        if let Ok((state, mut transform, mut visibility)) = render_query.get_mut(entity) {
            if let Ok(planet) = planet_query.get(state.planet_entity) {
                let Some(orientation) =
                    ephemeris_snapshot.orientation_for_catalog_body(&planet.domain_planet.name)
                else {
                    continue;
                };
                update_patch_transform(&mut transform, state, orientation, render_origin.origin);
            }
            *visibility = if has_departing_ancestor_render_entity(
                event.patch,
                event.planet_entity,
                &streaming.published,
                &render_index,
            ) {
                Visibility::Hidden
            } else {
                Visibility::Visible
            };
        } else {
            render_index.0.remove(&key);
        }
    }
}

fn has_departing_ancestor_render_entity(
    patch: TerrainPatch,
    planet_entity: Entity,
    published: &std::collections::BTreeSet<TerrainPatch>,
    render_index: &TerrainPatchRenderIndex,
) -> bool {
    let mut ancestor = patch.parent();
    while let Some(parent) = ancestor {
        if !published.contains(&parent)
            && render_index.0.contains_key(&TerrainPatchRenderKey {
                planet_entity,
                patch: parent,
            })
        {
            return true;
        }
        ancestor = parent.parent();
    }
    false
}

fn has_visible_departing_ancestor(
    patch: TerrainPatch,
    planet_entity: Entity,
    published: &std::collections::BTreeSet<TerrainPatch>,
    render_index: &TerrainPatchRenderIndex,
    render_query: &mut Query<&mut Visibility>,
) -> bool {
    let mut ancestor = patch.parent();
    while let Some(parent) = ancestor {
        let key = TerrainPatchRenderKey {
            planet_entity,
            patch: parent,
        };
        if !published.contains(&parent) {
            if let Some(entity) = render_index.0.get(&key).copied() {
                if let Ok(visibility) = render_query.get_mut(entity) {
                    if *visibility != Visibility::Hidden {
                        return true;
                    }
                }
            }
        }
        ancestor = parent.parent();
    }
    false
}

fn published_cover_is_renderable(
    patch: TerrainPatch,
    planet_entity: Entity,
    published: &std::collections::BTreeSet<TerrainPatch>,
    render_index: &TerrainPatchRenderIndex,
) -> bool {
    if published.contains(&patch) {
        return render_index.0.contains_key(&TerrainPatchRenderKey {
            planet_entity,
            patch,
        });
    }

    patch.children().into_iter().all(|child| {
        published
            .iter()
            .any(|candidate| child.is_ancestor_of(candidate))
            && published_cover_is_renderable(child, planet_entity, published, render_index)
    })
}

fn reveal_published_descendants(
    parent: TerrainPatch,
    planet_entity: Entity,
    published: &std::collections::BTreeSet<TerrainPatch>,
    render_index: &TerrainPatchRenderIndex,
    render_query: &mut Query<&mut Visibility>,
) {
    for patch in published
        .iter()
        .copied()
        .filter(|patch| patch.level > parent.level && parent.is_ancestor_of(patch))
    {
        let key = TerrainPatchRenderKey {
            planet_entity,
            patch,
        };
        if let Some(entity) = render_index.0.get(&key).copied() {
            if let Ok(mut visibility) = render_query.get_mut(entity) {
                *visibility = Visibility::Visible;
            }
        }
    }
}

/// System that despawns mesh entities when a terrain patch is evicted.
fn despawn_patch_mesh_system(
    mut commands: Commands,
    mut events: MessageReader<TerrainPatchEvicted>,
    mut render_index: ResMut<TerrainPatchRenderIndex>,
    render_query: Query<&TerrainPatchRenderState>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<TerrainMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    for event in events.read() {
        let key = TerrainPatchRenderKey {
            planet_entity: event.planet_entity,
            patch: event.patch,
        };
        let Some(entity) = render_index.0.remove(&key) else {
            continue;
        };
        if let Ok(state) = render_query.get(entity) {
            commands.entity(entity).despawn();
            release_patch_render_assets(state, &mut meshes, &mut materials, &mut images);
        }
    }
}

fn release_patch_render_assets(
    state: &TerrainPatchRenderState,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<TerrainMaterial>,
    images: &mut Assets<Image>,
) {
    meshes.remove(state.mesh_handle.id());
    materials.remove(state.material_handle.id());
    if let Some(vegetation_mesh_handle) = &state.vegetation_mesh_handle {
        meshes.remove(vegetation_mesh_handle.id());
    }
    if let Some((albedo, normal)) = &state.local_surface_handles {
        images.remove(albedo.id());
        images.remove(normal.id());
    }
}

fn fallback_surface_maps(
    render_assets: &mut TerrainRenderAssets,
    images: &mut Assets<Image>,
) -> (Handle<Image>, Handle<Image>) {
    render_assets
        .fallback_surface_maps
        .get_or_insert_with(|| {
            let extent = Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            };
            let albedo = images.add(Image::new_fill(
                extent,
                TextureDimension::D2,
                &[255, 255, 255, 255],
                TextureFormat::Rgba8Unorm,
                RenderAssetUsages::RENDER_WORLD,
            ));
            let normal = images.add(Image::new_fill(
                extent,
                TextureDimension::D2,
                &[128, 128, 255, 255],
                TextureFormat::Rgba8Unorm,
                RenderAssetUsages::RENDER_WORLD,
            ));
            (albedo, normal)
        })
        .clone()
}

/// Convert domain PatchGeometry to Bevy Mesh, rebasing planet-centered positions
/// into the rocket-local flight frame (`positions - render_origin`). This keeps
/// f32 vertex magnitudes small near the camera, preserving the spherical surface
/// instead of collapsing it into a flat plane at ~6371 km magnitudes.
fn patch_geometry_to_mesh(
    geometry: &PatchGeometry,
    render_origin: &DVec3,
    body_to_inertial: bevy::math::DQuat,
    vertex_colors: &[[f32; 4]],
) -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );

    // Positions rebased to flight frame (f32 for GPU).
    let positions: Vec<[f32; 3]> = geometry
        .positions
        .iter()
        .map(|p| {
            let v = body_to_inertial * DVec3::from_array(*p) - *render_origin;
            [v.x as f32, v.y as f32, v.z as f32]
        })
        .collect();
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);

    // Normals.
    let normals: Vec<[f32; 3]> = geometry
        .normals
        .iter()
        .map(|n| {
            let n = body_to_inertial * DVec3::from_array(*n);
            [n.x as f32, n.y as f32, n.z as f32]
        })
        .collect();
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);

    // UV0 is geographic equirectangular UV for global imagery. UV1 is tile-local
    // and consumed by TerrainSurfaceExtension's local albedo and normal maps.
    let uvs = geometry.uvs.to_vec();
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, geometry.local_uvs.clone());
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vertex_colors.to_vec());

    // Indices.
    mesh.insert_indices(Indices::U32(geometry.indices.clone()));

    mesh
}

/// Shift only presentation coordinates when the rocket has moved far enough
/// from the current local origin. Existing terrain mesh vertices stay valid;
/// their root transforms preserve world placement until regenerated.
pub fn recenter_render_origin(
    config: Res<TerrainRenderConfig>,
    rocket_query: Query<&RocketPhysicsState>,
    mut render_origin: ResMut<RenderOrigin>,
) {
    let Some(rocket) = rocket_query.iter().next() else {
        return;
    };
    let new_origin = rocket.dynamics.position_m;
    if (new_origin - render_origin.origin).length() < config.recenter_threshold_m {
        return;
    }
    render_origin.origin = new_origin;
    render_origin.last_camera_pos = new_origin;
}

/// Keep visible terrain meshes attached to the rotating planet after generation.
/// Cached meshes are refreshed immediately before reveal, avoiding transform
/// writes for geometry that is not currently rendered.
fn update_patch_transforms(
    ephemeris_snapshot: Res<EphemerisSnapshot>,
    render_origin: Res<RenderOrigin>,
    planet_query: Query<&PlanetComponent>,
    mut patch_query: Query<(&TerrainPatchRenderState, &mut Transform, &Visibility)>,
) {
    for (state, mut transform, visibility) in patch_query.iter_mut() {
        if *visibility == Visibility::Hidden {
            continue;
        }
        let Ok(planet) = planet_query.get(state.planet_entity) else {
            continue;
        };
        let Some(orientation) =
            ephemeris_snapshot.orientation_for_catalog_body(&planet.domain_planet.name)
        else {
            continue;
        };
        update_patch_transform(&mut transform, state, orientation, render_origin.origin);
    }
}

fn update_patch_transform(
    transform: &mut Transform,
    state: &TerrainPatchRenderState,
    orientation: &BodyOrientation,
    render_origin: DVec3,
) {
    let body_to_inertial = body_fixed_to_planet_inertial_rotation(orientation);
    let (rotation, translation) = patch_transform_components(
        state.body_to_inertial_at_spawn,
        state.render_origin_at_spawn,
        body_to_inertial,
        render_origin,
    );
    transform.rotation = rotation.as_quat();
    transform.translation = translation.as_vec3();
}

/// Return the presentation-only transform taking a baked terrain patch into the
/// current interpolated body pose and render-origin frame.
fn patch_transform_components(
    body_to_inertial_at_spawn: DQuat,
    render_origin_at_spawn: DVec3,
    body_to_inertial: DQuat,
    render_origin: DVec3,
) -> (DQuat, DVec3) {
    let rotation = body_to_inertial * body_to_inertial_at_spawn.conjugate();
    let translation = rotation * render_origin_at_spawn - render_origin;
    (rotation, translation)
}

/// Create the base terrain material. The albedo and normal map are supplied per
/// patch by `build_patch_surfaces` (set by the caller); this only provides the
/// representative roughness from the shared `surface_appearance` law (AGENTS.md
/// 50: one authoritative appearance law).
fn patch_material(roughness: f32, metallic: f32) -> StandardMaterial {
    StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: roughness,
        metallic,
        unlit: false,
        ..default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::cube_sphere::{build_patch_geometry, CubeFace};
    use crate::domain::services::planet_factory::PlanetFactory;
    use crate::domain::services::reference_frames::catalog_body_fixed_to_inertial_rotation;
    use crate::domain::services::simulation_time::SimulationTime;
    use bevy::ecs::message::Messages;
    use std::collections::BTreeSet;

    fn terrain_position_in_render_frame(
        body_fixed_position_m: DVec3,
        body_to_inertial_at_spawn: DQuat,
        render_origin_at_spawn: DVec3,
        body_to_inertial: DQuat,
        render_origin: DVec3,
    ) -> DVec3 {
        let baked_position =
            body_to_inertial_at_spawn * body_fixed_position_m - render_origin_at_spawn;
        let (rotation, translation) = patch_transform_components(
            body_to_inertial_at_spawn,
            render_origin_at_spawn,
            body_to_inertial,
            render_origin,
        );
        rotation * baked_position + translation
    }

    fn vegetation_position_in_render_frame(
        body_fixed_offset_m: DVec3,
        anchor_body_fixed_m: DVec3,
        body_to_inertial_at_spawn: DQuat,
        render_origin_at_spawn: DVec3,
        body_to_inertial: DQuat,
        render_origin: DVec3,
    ) -> DVec3 {
        let (patch_rotation, patch_translation) = patch_transform_components(
            body_to_inertial_at_spawn,
            render_origin_at_spawn,
            body_to_inertial,
            render_origin,
        );
        let child_translation =
            body_to_inertial_at_spawn * anchor_body_fixed_m - render_origin_at_spawn;
        patch_rotation * (body_to_inertial_at_spawn * body_fixed_offset_m + child_translation)
            + patch_translation
    }

    #[test]
    fn patch_geometry_emits_skirt_ring_for_crack_free_lod() {
        // Spec scenario "skirt geometry stitches edges": adjacent patches at
        // different LOD must not gap. The geometry-level guarantee is a
        // boundary skirt ring: for a res×res patch, every boundary vertex gets
        // an extra extruded vertex and a skirt quad is emitted per segment.
        let patch = TerrainPatch::for_direction(DVec3::new(0.3, 0.4, 1.0).normalize(), 2);
        let res = 5usize;
        let geom = build_patch_geometry(
            &patch,
            &crate::domain::services::terrain_source::ProceduralTerrainSource::new(
                99, 2_000.0, 800.0, 0,
            ),
            6_371_000.0,
            res as u32,
            40.0,
        );
        // Platform vertices + one skirt vertex per boundary vertex.
        let boundary_count = 4 * (res - 1); // res*res grid has 4(res-1) boundary verts
        assert_eq!(
            geom.positions.len(),
            res * res + boundary_count,
            "expected skirt ring appended"
        );
        // Skirt vertices must be extruded downward (closer to the planet than a
        // corresponding grid vertex, so the crack is hidden rather than opened).
        let non_skirt = res * res;
        for pos in geom.positions.iter().skip(non_skirt) {
            let r = DVec3::from_array(*pos).length();
            assert!(
                r < 6_371_000.0 + 2_900.0,
                "skirt vertex at radius {r} not extruded inward"
            );
        }
        // Skirt quads are present: more indices than a flat grid alone.
        assert!(geom.indices.len() > (res - 1) * (res - 1) * 6);
    }

    #[test]
    fn interpolated_terrain_matches_surface_fixed_rocket_across_fixed_overstep() {
        let earth = PlanetFactory::create_by_name("Earth").unwrap();
        let mut sim_time = SimulationTime::new(0.25);
        sim_time.sim_time_s = 12_345.0;
        let surface_position_m = DVec3::new(earth.radius_km as f64 * 1_000.0, 0.0, 0.0);
        let body_to_inertial_at_spawn = catalog_body_fixed_to_inertial_rotation(&earth, 0.1);
        let render_origin_at_spawn = DVec3::new(100.0, -200.0, 300.0);
        let current_body_to_inertial =
            catalog_body_fixed_to_inertial_rotation(&earth, sim_time.sim_time_s / 86_400.0);
        let render_origin = current_body_to_inertial * surface_position_m;

        for _alpha in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let body_to_inertial = current_body_to_inertial;
            let terrain_position = terrain_position_in_render_frame(
                surface_position_m,
                body_to_inertial_at_spawn,
                render_origin_at_spawn,
                body_to_inertial,
                render_origin,
            );
            let rocket_position = body_to_inertial * surface_position_m - render_origin;

            assert!(
                terrain_position.distance(rocket_position) < 1e-7,
                "terrain diverged from a surface-fixed rocket"
            );
        }
    }

    #[test]
    fn newly_spawned_patch_uses_the_interpolated_body_pose() {
        let earth = PlanetFactory::create_by_name("Earth").unwrap();
        let mut sim_time = SimulationTime::new(0.25);
        sim_time.sim_time_s = 12_345.0;
        let body_to_inertial =
            catalog_body_fixed_to_inertial_rotation(&earth, sim_time.sim_time_s / 86_400.0);
        let surface_position_m = DVec3::new(0.0, earth.radius_km as f64 * 1_000.0, 0.0);
        let render_origin = DVec3::new(10.0, 20.0, 30.0);

        let (rotation, translation) = patch_transform_components(
            body_to_inertial,
            render_origin,
            body_to_inertial,
            render_origin,
        );
        let terrain_position = terrain_position_in_render_frame(
            surface_position_m,
            body_to_inertial,
            render_origin,
            body_to_inertial,
            render_origin,
        );

        assert!(rotation.abs_diff_eq(DQuat::IDENTITY, 1e-12));
        assert!(translation.abs_diff_eq(DVec3::ZERO, 1e-12));
        assert!(terrain_position
            .abs_diff_eq(body_to_inertial * surface_position_m - render_origin, 1e-7));
    }

    #[test]
    fn vegetation_offsets_follow_the_same_body_rotation_as_terrain() {
        let earth = PlanetFactory::create_by_name("Earth").unwrap();
        let body_to_inertial_at_spawn = catalog_body_fixed_to_inertial_rotation(&earth, 0.1);
        let body_to_inertial = catalog_body_fixed_to_inertial_rotation(&earth, 0.6);
        let render_origin_at_spawn = DVec3::new(100.0, -200.0, 300.0);
        let render_origin = DVec3::new(-400.0, 500.0, -600.0);
        let anchor_body_fixed_m = DVec3::new(earth.radius_km as f64 * 1_000.0, 0.0, 0.0);
        let body_fixed_offset_m = DVec3::new(850.0, -425.0, 5.0);

        let vegetation_position = vegetation_position_in_render_frame(
            body_fixed_offset_m,
            anchor_body_fixed_m,
            body_to_inertial_at_spawn,
            render_origin_at_spawn,
            body_to_inertial,
            render_origin,
        );
        let expected =
            body_to_inertial * (anchor_body_fixed_m + body_fixed_offset_m) - render_origin;

        assert!(vegetation_position.abs_diff_eq(expected, 1e-7));
    }

    #[test]
    fn source_appearance_colors_every_terrain_lod_without_macro_texture_modulation() {
        let source = crate::domain::services::terrain_source::ProceduralTerrainSource::new(
            99, 2_000.0, 800.0, 0,
        );
        let patch = TerrainPatch::for_direction(DVec3::new(0.3, 0.4, 1.0).normalize(), 2);
        let geometry = build_patch_geometry(&patch, &source, 6_371_000.0, 5, 40.0);
        let coarse = crate::infrastructure::bevy_adapters::terrain_surface::prepare_patch_surface(
            &source,
            &TerrainPatch::for_direction(DVec3::Z, 0),
            &geometry,
            6_371_000.0,
        );
        let fine = crate::infrastructure::bevy_adapters::terrain_surface::prepare_patch_surface(
            &source,
            &TerrainPatch::for_direction(DVec3::Z, 8),
            &geometry,
            6_371_000.0,
        );

        assert!(coarse
            .vertex_colors
            .iter()
            .any(|color| *color != [1.0, 1.0, 1.0, 1.0]));
        assert_eq!(coarse.vertex_colors, fine.vertex_colors);
        assert_eq!(geometry.uvs.len(), geometry.local_uvs.len());

        let mesh = patch_geometry_to_mesh(
            &geometry,
            &DVec3::ZERO,
            DQuat::IDENTITY,
            &fine.vertex_colors,
        );
        assert!(mesh.attribute(Mesh::ATTRIBUTE_UV_1).is_some());
    }

    #[test]
    fn cached_patch_reuses_its_render_entity_when_republished() {
        let mut app = App::new();
        app.insert_resource(SimulationTime::default())
            .init_resource::<EphemerisSnapshot>()
            .insert_resource(RenderOrigin::default())
            .insert_resource(Time::<Fixed>::default())
            .insert_resource(TerrainStreamingResource::default())
            .init_resource::<TerrainPatchRenderIndex>()
            .init_resource::<PendingTerrainPatchHides>()
            .add_message::<TerrainPatchCached>()
            .add_message::<TerrainPatchReady>()
            .add_systems(
                Update,
                (
                    reveal_cached_patch_mesh_system,
                    hide_cached_patch_mesh_system,
                )
                    .chain(),
            );

        let patch = TerrainPatch::for_direction(DVec3::X, 2);
        let planet_entity = Entity::PLACEHOLDER;
        let other_planet_entity = app.world_mut().spawn_empty().id();
        let entity = app
            .world_mut()
            .spawn((
                TerrainPatchRenderState {
                    patch,
                    mesh_handle: Handle::default(),
                    material_handle: Handle::default(),
                    local_surface_handles: None,
                    vegetation_mesh_handle: None,
                    planet_entity,
                    body_to_inertial_at_spawn: DQuat::IDENTITY,
                    render_origin_at_spawn: DVec3::ZERO,
                },
                Transform::IDENTITY,
                Visibility::Visible,
            ))
            .id();
        app.world_mut()
            .resource_mut::<TerrainPatchRenderIndex>()
            .0
            .insert(
                TerrainPatchRenderKey {
                    planet_entity,
                    patch,
                },
                entity,
            );
        let other_entity = app
            .world_mut()
            .spawn((
                TerrainPatchRenderState {
                    patch,
                    mesh_handle: Handle::default(),
                    material_handle: Handle::default(),
                    local_surface_handles: None,
                    vegetation_mesh_handle: None,
                    planet_entity: other_planet_entity,
                    body_to_inertial_at_spawn: DQuat::IDENTITY,
                    render_origin_at_spawn: DVec3::ZERO,
                },
                Transform::IDENTITY,
                Visibility::Visible,
            ))
            .id();
        app.world_mut()
            .resource_mut::<TerrainPatchRenderIndex>()
            .0
            .insert(
                TerrainPatchRenderKey {
                    planet_entity: other_planet_entity,
                    patch,
                },
                other_entity,
            );

        app.world_mut()
            .resource_mut::<Messages<TerrainPatchCached>>()
            .write(TerrainPatchCached {
                patch,
                planet_entity,
            });
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(entity).unwrap(),
            Visibility::Hidden
        );
        assert_eq!(
            *app.world().get::<Visibility>(other_entity).unwrap(),
            Visibility::Visible
        );

        app.world_mut()
            .resource_mut::<Messages<TerrainPatchReady>>()
            .write(TerrainPatchReady {
                patch,
                planet_entity,
            });
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(entity).unwrap(),
            Visibility::Visible
        );
        assert!(app.world().get_entity(entity).is_ok());

        // A CPU-published child without a render entity must not hide the
        // parent. This is the async upload race that previously created gaps.
        let child = patch.children()[0];
        app.world_mut()
            .resource_mut::<TerrainStreamingResource>()
            .published
            .insert(child);
        app.world_mut()
            .resource_mut::<Messages<TerrainPatchCached>>()
            .write(TerrainPatchCached {
                patch,
                planet_entity,
            });
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(entity).unwrap(),
            Visibility::Visible
        );

        // Republishing the parent cancels the delayed hide. A later descendant
        // becoming renderable must not consume that obsolete transition.
        app.world_mut()
            .resource_mut::<TerrainStreamingResource>()
            .published
            .remove(&child);
        app.world_mut()
            .resource_mut::<TerrainStreamingResource>()
            .published
            .insert(patch);
        app.world_mut()
            .resource_mut::<Messages<TerrainPatchReady>>()
            .write(TerrainPatchReady {
                patch,
                planet_entity,
            });
        app.update();
        assert!(app
            .world()
            .resource::<PendingTerrainPatchHides>()
            .0
            .is_empty());

        let child_entity = app.world_mut().spawn(Visibility::Visible).id();
        app.world_mut()
            .resource_mut::<TerrainPatchRenderIndex>()
            .0
            .insert(
                TerrainPatchRenderKey {
                    planet_entity,
                    patch: child,
                },
                child_entity,
            );
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(entity).unwrap(),
            Visibility::Visible
        );

        // A partial child cover is not a replacement: the parent remains
        // visible even after the first descendant entity has uploaded.
        app.world_mut()
            .resource_mut::<TerrainStreamingResource>()
            .published
            .remove(&patch);
        app.world_mut()
            .resource_mut::<TerrainStreamingResource>()
            .published
            .insert(child);
        app.world_mut()
            .resource_mut::<Messages<TerrainPatchCached>>()
            .write(TerrainPatchCached {
                patch,
                planet_entity,
            });
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(entity).unwrap(),
            Visibility::Visible
        );

        // Once every child quadrant has a render entity, the parent can hand
        // off coverage without exposing a gap.
        for sibling in patch.children().into_iter().skip(1) {
            let sibling_entity = app.world_mut().spawn(Visibility::Hidden).id();
            app.world_mut()
                .resource_mut::<TerrainStreamingResource>()
                .published
                .insert(sibling);
            app.world_mut()
                .resource_mut::<TerrainPatchRenderIndex>()
                .0
                .insert(
                    TerrainPatchRenderKey {
                        planet_entity,
                        patch: sibling,
                    },
                    sibling_entity,
                );
        }
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(entity).unwrap(),
            Visibility::Hidden
        );
        for child in patch.children() {
            let child_entity = app.world().resource::<TerrainPatchRenderIndex>().0
                [&TerrainPatchRenderKey {
                    planet_entity,
                    patch: child,
                }];
            assert_eq!(
                *app.world().get::<Visibility>(child_entity).unwrap(),
                Visibility::Visible
            );
        }
    }

    #[test]
    fn pending_uploads_retain_ready_patches_beyond_one_frame_budget() {
        let planet_entity = Entity::PLACEHOLDER;
        let patches = [
            TerrainPatch::for_direction(DVec3::X, 2),
            TerrainPatch::for_direction(DVec3::Y, 2),
            TerrainPatch::for_direction(DVec3::Z, 2),
        ];
        let mut pending = PendingTerrainPatchUploads::default();
        for patch in patches {
            assert!(pending.enqueue(TerrainPatchReady {
                patch,
                planet_entity,
            }));
        }

        assert_eq!(pending.pop_front().unwrap().patch, patches[0]);
        assert_eq!(pending.queue.len(), 2);
        assert_eq!(pending.queue.front().unwrap().patch, patches[1]);
    }

    #[test]
    fn pending_uploads_discard_ready_events_from_the_previous_planet() {
        let mut app = App::new();
        let previous_planet = app.world_mut().spawn_empty().id();
        let active_planet = app.world_mut().spawn_empty().id();
        let patch = TerrainPatch::for_direction(DVec3::X, 2);
        let mut pending = PendingTerrainPatchUploads::default();

        pending.enqueue(TerrainPatchReady {
            patch,
            planet_entity: previous_planet,
        });
        pending.retain_published_for_planet(Some(active_planet), &BTreeSet::from([patch]));

        assert!(pending.queue.is_empty());
        assert!(pending.queued.is_empty());
    }

    #[test]
    fn pending_uploads_coalesce_and_cap_ready_bursts() {
        let planet_entity = Entity::PLACEHOLDER;
        let mut pending = PendingTerrainPatchUploads::default();
        let first = TerrainPatch {
            face: CubeFace::PosZ,
            level: 12,
            tile_x: 0,
            tile_y: 0,
        };

        assert!(pending.enqueue(TerrainPatchReady {
            patch: first,
            planet_entity,
        }));
        assert!(!pending.enqueue(TerrainPatchReady {
            patch: first,
            planet_entity,
        }));
        for tile_x in 1..MAX_PENDING_PATCH_UPLOADS as u32 {
            assert!(pending.enqueue(TerrainPatchReady {
                patch: TerrainPatch { tile_x, ..first },
                planet_entity,
            }));
        }
        assert!(!pending.enqueue(TerrainPatchReady {
            patch: TerrainPatch {
                tile_x: MAX_PENDING_PATCH_UPLOADS as u32,
                ..first
            },
            planet_entity,
        }));
        assert_eq!(pending.queue.len(), MAX_PENDING_PATCH_UPLOADS);
    }

    #[test]
    fn evicting_a_patch_releases_its_unique_render_assets() {
        let mut meshes = Assets::<Mesh>::default();
        let mut materials = Assets::<TerrainMaterial>::default();
        let mut images = Assets::<Image>::default();
        let mesh_handle = meshes.add(Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        ));
        let vegetation_mesh_handle = meshes.add(Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        ));
        let material_handle = materials.add(TerrainMaterial::default());
        let mut vegetation_materials = Assets::<StandardMaterial>::default();
        let shared_vegetation_material = vegetation_materials.add(StandardMaterial::default());
        let state = TerrainPatchRenderState {
            patch: TerrainPatch::for_direction(DVec3::X, 0),
            mesh_handle: mesh_handle.clone(),
            material_handle: material_handle.clone(),
            local_surface_handles: None,
            vegetation_mesh_handle: Some(vegetation_mesh_handle.clone()),
            planet_entity: Entity::PLACEHOLDER,
            body_to_inertial_at_spawn: DQuat::IDENTITY,
            render_origin_at_spawn: DVec3::ZERO,
        };

        release_patch_render_assets(&state, &mut meshes, &mut materials, &mut images);

        assert!(meshes.get(mesh_handle.id()).is_none());
        assert!(meshes.get(vegetation_mesh_handle.id()).is_none());
        assert!(materials.get(material_handle.id()).is_none());
        assert!(vegetation_materials
            .get(shared_vegetation_material.id())
            .is_some());
    }
}
