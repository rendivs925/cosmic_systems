//! Terrain rendering plugin (AGENTS.md sections 27-28).
//!
//! Spawns GPU meshes and materials for cube-sphere LOD terrain patches from the
//! streaming manager, with PBR shaders for planetary surfaces and a floating
//! origin for precision at planetary scale.

use crate::domain::services::cube_sphere::{PatchGeometry, TerrainPatch};
use crate::domain::services::reference_frames::body_fixed_to_planet_inertial_rotation;
use crate::infrastructure::bevy_adapters::entity_components::*;
use crate::infrastructure::bevy_adapters::ephemeris::EphemerisSnapshot;
use crate::infrastructure::bevy_adapters::performance_components::PerformanceMetricsConfig;
use crate::infrastructure::bevy_adapters::performance_components::PerformanceMetricsSet;
use crate::infrastructure::bevy_adapters::rendering::textures::{
    get_planet_textures, load_texture,
};
use crate::infrastructure::bevy_adapters::rocket::camera::update_rocket_camera_projection;
use crate::infrastructure::bevy_adapters::rocket::components::RocketPhysicsState;
use crate::infrastructure::bevy_adapters::terrain::imagery::{
    apply_terrain_imagery, load_earth_imagery_package, stream_terrain_imagery,
    TerrainImageryConfig, TerrainImageryResource,
};
use crate::infrastructure::bevy_adapters::terrain::performance::TerrainPerformanceTelemetry;
use crate::infrastructure::bevy_adapters::terrain::streaming::{
    stream_terrain_patches, TerrainStreamingResource,
};
use crate::infrastructure::bevy_adapters::terrain::surface::{
    local_detail_weight, terrain_detail_texture, vegetation_atlas,
};
use crate::infrastructure::bevy_adapters::terrain::water::{
    WaterExtension, WaterMaterial, WaterParams,
};
use bevy::asset::{Assets, RenderAssetUsages};
use bevy::ecs::message::Message;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::math::{DQuat, DVec3};
use bevy::pbr::{ExtendedMaterial, MaterialExtension, MaterialPlugin};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, Extent3d, TextureDimension, TextureFormat};
use bevy::shader::ShaderRef;
use bevy_mesh::{Indices, PrimitiveTopology};
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::time::Instant;

const TERRAIN_SURFACE_SHADER: &str = "shaders/terrain_surface.wgsl";
/// Spreading texture creation and GPU asset uploads across frames prevents a
/// completed terrain batch from stalling camera and HUD presentation. The cap is
/// high enough to keep pace with the eight-bake generation budget; a larger
/// backlog would otherwise leave generated patches invisible for seconds.
const MAX_PATCH_UPLOADS_PER_FRAME: usize = 6;
/// Ready messages are coalesced and publication backfill makes a rejected entry
/// retryable, so this cap bounds memory without dropping visible terrain forever.
const MAX_PENDING_PATCH_UPLOADS: usize = 512;
/// Sea level is the terrain datum: height 0 above the catalog mean radius.
/// A patch morphs toward its coarser parent surface between these multiples of
/// its world size. Reaching full morph before the coarser LOD takes over keeps
/// the shared edge matching the neighbouring parent patch.
const MORPH_START_SIZE_FACTOR: f64 = 0.6;
const MORPH_END_SIZE_FACTOR: f64 = 2.0;
/// Micro-detail texture repetitions per metre squared. About 1.7 m features.
const TERRAIN_DETAIL_SCALE: f32 = 0.6;
const WATER_SEA_LEVEL_M: f64 = 0.0;
/// Small lift so the translucent water cap wins the depth test against the
/// coincident far-field fallback globe instead of z-fighting it.
const WATER_SURFACE_OFFSET_M: f64 = 0.5;
/// Depth mapped to full deep-water colour and opacity. Deeper samples clamp.
const WATER_MAX_VISIBLE_DEPTH_M: f64 = 4_000.0;
/// Cached parents stay visible until every visible descendant has a render
/// entity. CPU streaming readiness alone is not sufficient: asset creation is
/// deliberately spread across frames.
#[derive(Resource, Default)]
struct PendingTerrainPatchHides(HashSet<TerrainPatchRenderKey>);

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
pub(crate) struct TerrainSurfaceExtension {
    #[texture(100)]
    #[sampler(101)]
    local_albedo: Handle<Image>,
    #[texture(102)]
    #[sampler(103)]
    local_normal: Handle<Image>,
    #[uniform(104)]
    local_detail_weight: f32,
    /// The shared equirectangular Earth albedo sampled with mesh UV0. Local
    /// source-derived maps enrich it, but do not replace global geography.
    #[texture(105)]
    #[sampler(106)]
    global_albedo: Handle<Image>,
    /// A produced cube-sphere imagery tile sampled with the patch-local UV1.
    /// While no tile is ready this holds a neutral placeholder and
    /// `imagery_weight` is zero, so the global overview stays visible.
    #[texture(107)]
    #[sampler(108)]
    imagery_albedo: Handle<Image>,
    #[uniform(104)]
    imagery_weight: f32,
    /// View-distance band over which a patch morphs toward its coarser parent
    /// surface. Scaled to the patch's world size so neighbouring LODs agree.
    #[uniform(104)]
    morph_start_m: f32,
    #[uniform(104)]
    morph_end_m: f32,
    /// Shared triplanar micro-detail texture (normal/albedo/roughness).
    #[texture(109)]
    #[sampler(110)]
    detail_texture: Handle<Image>,
    /// Repetitions per metre of the micro-detail texture.
    #[uniform(104)]
    detail_scale: f32,
}

impl MaterialExtension for TerrainSurfaceExtension {
    fn fragment_shader() -> ShaderRef {
        TERRAIN_SURFACE_SHADER.into()
    }

    /// The default mesh vertex shader cannot apply the per-vertex LOD morph, so
    /// the terrain supplies its own vertex stage alongside the fragment one.
    fn vertex_shader() -> ShaderRef {
        TERRAIN_SURFACE_SHADER.into()
    }
}

pub(crate) type TerrainMaterial = ExtendedMaterial<StandardMaterial, TerrainSurfaceExtension>;

/// Build a patch material from its surface inputs. Kept in one place so the
/// imagery upgrade path can rebuild the same material with a ready imagery tile
/// without duplicating the extension layout.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_terrain_material(
    base: StandardMaterial,
    local_albedo: Handle<Image>,
    local_normal: Handle<Image>,
    local_detail_weight: f32,
    global_albedo: Handle<Image>,
    imagery_albedo: Handle<Image>,
    imagery_weight: f32,
    morph_start_m: f32,
    morph_end_m: f32,
    detail_texture: Handle<Image>,
    detail_scale: f32,
) -> TerrainMaterial {
    TerrainMaterial {
        base,
        extension: TerrainSurfaceExtension {
            local_albedo,
            local_normal,
            local_detail_weight,
            global_albedo,
            imagery_albedo,
            imagery_weight,
            morph_start_m,
            morph_end_m,
            detail_texture,
            detail_scale,
        },
    }
}
/// Component tracking the render state of a terrain patch.
#[derive(Component, Debug, Clone)]
pub struct TerrainPatchRenderState {
    pub patch: TerrainPatch,
    pub mesh_handle: Handle<Mesh>,
    pub(crate) material_handle: Handle<TerrainMaterial>,
    /// Inputs needed to rebuild the material when imagery becomes ready.
    pub(crate) base_material: StandardMaterial,
    pub(crate) local_albedo: Handle<Image>,
    pub(crate) local_normal: Handle<Image>,
    pub(crate) local_detail_weight: f32,
    pub(crate) global_albedo: Handle<Image>,
    /// The imagery tile currently bound, or a neutral placeholder.
    pub(crate) imagery_albedo: Handle<Image>,
    pub(crate) imagery_weight: f32,
    /// View-distance morph band for this patch, passed through on material rebuild.
    pub(crate) morph_start_m: f32,
    pub(crate) morph_end_m: f32,
    /// Shared micro-detail inputs, passed through on material rebuild.
    pub(crate) detail_texture: Handle<Image>,
    pub(crate) detail_scale: f32,
    /// Per-patch source-derived surface textures released with the patch.
    pub(crate) local_surface_handles: Option<(Handle<Image>, Handle<Image>)>,
    pub vegetation_mesh_handle: Option<Handle<Mesh>>,
    /// Sea-level water cap for patches that contain ocean, released with the
    /// patch. The water material itself is shared.
    pub water_mesh_handle: Option<Handle<Mesh>>,
    /// Drainage ribbon for patches crossed by a river channel, released with the
    /// patch. Shares the river material.
    pub river_mesh_handle: Option<Handle<Mesh>>,
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
    /// One shared water material; every ocean patch reuses it.
    water_material: Option<Handle<WaterMaterial>>,
    /// One shared river material; every drainage ribbon reuses it.
    river_material: Option<Handle<WaterMaterial>>,
    /// Core grid resolution of a patch, mirrored from `TerrainRenderConfig` so
    /// water generation does not need another system parameter.
    patch_resolution: u32,
    /// Equirectangular global albedo per body name, loaded from the catalog on
    /// demand so Moon/Mars terrain is not tinted by Earth's image.
    global_albedo: HashMap<String, Handle<Image>>,
    /// Shared neutral maps let coarse patches use the terrain material without
    /// allocating local images whose detail is not visible at their LOD.
    neutral_local_albedo: Option<Handle<Image>>,
    neutral_local_normal: Option<Handle<Image>>,
    /// Shared tiling micro-detail texture (normal/albedo/roughness) sampled
    /// triplanar by the terrain shader.
    detail_texture: Option<Handle<Image>>,
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
    needs_backfill: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TerrainUploadEnqueueResult {
    Queued,
    Duplicate,
    Rejected,
}

#[derive(Default)]
struct TerrainUploadBackfill {
    queued: usize,
    rejected: usize,
}

impl PendingTerrainPatchUploads {
    fn retain_published_for_planet(
        &mut self,
        active_planet: Option<Entity>,
        published: &std::collections::BTreeSet<TerrainPatch>,
    ) {
        let before = self.queue.len();
        self.queue.retain(|event| {
            active_planet == Some(event.planet_entity) && published.contains(&event.patch)
        });
        // Rebuilding the dedup set is only necessary when the retain actually
        // removed queued work; the common case (nothing stale) skips the
        // per-frame allocation entirely.
        if self.queue.len() != before {
            self.queued = self.queue.iter().map(TerrainPatchRenderKey::from).collect();
        }
    }

    fn enqueue(&mut self, event: TerrainPatchReady) -> TerrainUploadEnqueueResult {
        let key = TerrainPatchRenderKey::from(&event);
        if self.queued.contains(&key) {
            return TerrainUploadEnqueueResult::Duplicate;
        }
        if self.queue.len() >= MAX_PENDING_PATCH_UPLOADS {
            self.needs_backfill = true;
            return TerrainUploadEnqueueResult::Rejected;
        }
        self.queued.insert(key);
        self.queue.push_back(event);
        TerrainUploadEnqueueResult::Queued
    }

    fn pop_front(&mut self) -> Option<TerrainPatchReady> {
        let event = self.queue.pop_front()?;
        self.queued.remove(&TerrainPatchRenderKey::from(&event));
        Some(event)
    }

    fn backfill_published(
        &mut self,
        planet_entity: Entity,
        published: &std::collections::BTreeSet<TerrainPatch>,
        render_index: &TerrainPatchRenderIndex,
    ) -> TerrainUploadBackfill {
        let mut backfill = TerrainUploadBackfill::default();
        if !self.needs_backfill {
            return backfill;
        }

        self.needs_backfill = false;
        for patch in published.iter().copied() {
            let event = TerrainPatchReady {
                patch,
                planet_entity,
            };
            let key = TerrainPatchRenderKey::from(&event);
            if render_index.0.contains_key(&key) || self.queued.contains(&key) {
                continue;
            }
            match self.enqueue(event) {
                TerrainUploadEnqueueResult::Queued => backfill.queued += 1,
                TerrainUploadEnqueueResult::Duplicate => {}
                TerrainUploadEnqueueResult::Rejected => {
                    backfill.rejected += 1;
                    break;
                }
            }
        }
        backfill
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
    /// Patch resolution (vertices per side) used at every spatial LOD.
    ///
    /// A uniform `2^n + 1` grid preserves the supported 2:1 spatial stitch
    /// invariant. Per-level grid-density upgrades would create an unsupported
    /// 4:1 sample transition at a normal 2:1 LOD boundary.
    pub patch_resolution: u32,
}

impl Default for TerrainRenderConfig {
    fn default() -> Self {
        Self {
            recenter_threshold_m: 10_000.0,
            skirt_depth_m: 5.0,
            // 2^n + 1 samples preserve parent/child boundary sample alignment.
            // Spatial LOD supplies detail without requiring unsupported 4:1
            // stitching at the boundary between two resolution tiers.
            patch_resolution: 33,
        }
    }
}

impl TerrainRenderConfig {
    /// Return the grid resolution for a patch while 2:1 edge stitching is the
    /// only supported transition. Keep this policy centralized so a future 4:1
    /// stitch implementation has one explicit configuration boundary to extend.
    pub(crate) fn patch_resolution_for(&self, _patch: TerrainPatch) -> u32 {
        self.patch_resolution
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
            .init_resource::<TerrainPerformanceTelemetry>()
            .init_resource::<PendingTerrainPatchHides>()
            .init_resource::<TerrainPatchRenderIndex>()
            .init_resource::<TerrainImageryConfig>()
            .init_resource::<TerrainImageryResource>()
            .add_plugins(MaterialPlugin::<TerrainMaterial>::default())
            .add_plugins(MaterialPlugin::<WaterMaterial>::default())
            .add_message::<TerrainPatchReady>()
            .add_message::<TerrainPatchCached>()
            .add_message::<TerrainPatchEvicted>()
            // Load the imagery package first so the preload can bind the
            // package's global overview as Earth's fallback albedo.
            .add_systems(
                Startup,
                (load_earth_imagery_package, prepare_terrain_render_assets).chain(),
            )
            .add_systems(
                Update,
                (
                    recenter_render_origin.before(stream_terrain_patches),
                    update_water_material,
                ),
            )
            // Streaming owns the authoritative terrain mesh lifecycle. It must
            // run after the flight render origin is current and before patch
            // uploads/transforms consume its ready events.
            .add_systems(
                Update,
                stream_terrain_patches
                    .after(recenter_render_origin)
                    .after(update_rocket_camera_projection),
            )
            .add_systems(Update, stream_terrain_imagery.after(stream_terrain_patches))
            .add_systems(
                Update,
                (
                    update_patch_transforms,
                    reveal_cached_patch_mesh_system,
                    spawn_patch_mesh_system,
                    hide_cached_patch_mesh_system,
                    despawn_patch_mesh_system,
                    finish_terrain_performance_frame,
                )
                    .chain()
                    .after(stream_terrain_patches)
                    .before(PerformanceMetricsSet::Report),
            )
            .add_systems(
                Update,
                apply_terrain_imagery
                    .after(spawn_patch_mesh_system)
                    .after(stream_terrain_imagery),
            );
    }
}

fn finish_terrain_performance_frame(
    performance_config: Res<PerformanceMetricsConfig>,
    mut terrain_performance: ResMut<TerrainPerformanceTelemetry>,
) {
    terrain_performance.finish_frame(performance_config.instrumentation_enabled());
}

/// The catalog's geographic albedo for one body, loaded once and cached. Terrain
/// meshes retain UV0 specifically for this continuous, equirectangular image.
/// When the Earth imagery package has verified, its high-resolution global
/// overview replaces the catalog texture as the globe-wide fallback.
fn global_albedo_for(
    render_assets: &mut TerrainRenderAssets,
    asset_server: &AssetServer,
    imagery: &TerrainImageryResource,
    body_name: &str,
) -> Option<Handle<Image>> {
    if body_name == "Earth" {
        if let Some(handle) = imagery.global_overview() {
            return Some(handle.clone());
        }
    }
    if let Some(handle) = render_assets.global_albedo.get(body_name) {
        return Some(handle.clone());
    }
    let handle = load_texture(asset_server, get_planet_textures(body_name).albedo)?;
    render_assets
        .global_albedo
        .insert(body_name.to_owned(), handle.clone());
    Some(handle)
}

/// Preload the default Earth albedo and the shared neutral surface maps so the
/// first terrain patch does not wait on an asset load. Non-default bodies load
/// on demand through [`global_albedo_for`].
fn prepare_terrain_render_assets(
    config: Res<TerrainRenderConfig>,
    asset_server: Res<AssetServer>,
    imagery: Res<TerrainImageryResource>,
    mut render_assets: ResMut<TerrainRenderAssets>,
    mut images: ResMut<Assets<Image>>,
    mut standard_materials: ResMut<Assets<StandardMaterial>>,
    mut water_materials: ResMut<Assets<WaterMaterial>>,
) {
    render_assets.patch_resolution = config.patch_resolution;
    let _ = global_albedo_for(&mut render_assets, &asset_server, &imagery, "Earth");
    ensure_neutral_local_surface_maps(&mut render_assets, &mut images);
    // One shared micro-detail texture across every patch.
    render_assets.detail_texture = Some(images.add(terrain_detail_texture()));
    // One alpha-masked foliage material is shared by every patch. It is created
    // once here so no patch spawn path needs the image assets.
    let vegetation_atlas = images.add(vegetation_atlas());
    render_assets.vegetation_material = Some(standard_materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(vegetation_atlas.clone()),
        alpha_mode: AlphaMode::Mask(0.5),
        perceptual_roughness: 0.9,
        metallic: 0.0,
        // Billboards must be visible from both sides; Bevy flips normals for the
        // back face in PBR.
        cull_mode: None,
        ..default()
    }));
    render_assets.water_material = Some(water_materials.add(WaterMaterial {
        base: water_base_material(),
        extension: WaterExtension::default(),
    }));
    render_assets.river_material = Some(water_materials.add(WaterMaterial {
        base: water_base_material(),
        extension: WaterExtension {
            params: WaterParams::river(),
        },
    }));
}

/// Shared base material for the water surface: blended, double-sided, and very
/// smooth so the fragment shader's ripple normal drives a tight sun glint.
fn water_base_material() -> StandardMaterial {
    StandardMaterial {
        base_color: Color::WHITE,
        alpha_mode: AlphaMode::Blend,
        cull_mode: None,
        perceptual_roughness: 0.05,
        metallic: 0.0,
        ..default()
    }
}

/// Advance the shared water wave phase. Presentation only; never read by the
/// simulation, terrain source, or collision.
fn update_water_material(
    time: Res<Time>,
    render_assets: Res<TerrainRenderAssets>,
    mut water_materials: ResMut<Assets<WaterMaterial>>,
) {
    let elapsed_s = time.elapsed_secs();
    for handle in [
        render_assets.water_material.as_ref(),
        render_assets.river_material.as_ref(),
    ]
    .into_iter()
    .flatten()
    {
        if let Some(material) = water_materials.get_mut(handle) {
            material.extension.params.time_s = elapsed_s;
        }
    }
}

fn ensure_neutral_local_surface_maps(
    render_assets: &mut TerrainRenderAssets,
    images: &mut Assets<Image>,
) -> (Handle<Image>, Handle<Image>) {
    let albedo = render_assets
        .neutral_local_albedo
        .get_or_insert_with(|| images.add(neutral_surface_image([255, 255, 255, 255])))
        .clone();
    let normal = render_assets
        .neutral_local_normal
        .get_or_insert_with(|| images.add(neutral_surface_image([128, 128, 255, 255])))
        .clone();
    (albedo, normal)
}

fn neutral_surface_image(data: [u8; 4]) -> Image {
    Image::new(
        Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data.to_vec(),
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    )
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
    mut images: ResMut<Assets<Image>>,
    mut render_assets: ResMut<TerrainRenderAssets>,
    mut streaming: ResMut<TerrainStreamingResource>,
    asset_server: Res<AssetServer>,
    imagery: Res<TerrainImageryResource>,
    render_origin: Res<RenderOrigin>,
    ephemeris_snapshot: Res<EphemerisSnapshot>,
    planet_query: Query<&PlanetComponent>,
    performance_config: Res<PerformanceMetricsConfig>,
    mut terrain_performance: ResMut<TerrainPerformanceTelemetry>,
) {
    let instrumentation_enabled = performance_config.instrumentation_enabled();
    let active_planet = streaming.active_planet();
    if !enqueue_ready_uploads(
        &mut events,
        &mut pending_uploads,
        &streaming.published,
        active_planet,
        &render_index,
        &mut terrain_performance,
        instrumentation_enabled,
    ) {
        return;
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
        let mesh_construction_started = instrumentation_enabled.then(Instant::now);
        let mesh = patch_geometry_to_mesh(
            geometry,
            &render_origin.origin,
            body_to_inertial,
            &surface.vertex_colors,
        );
        if let (Some(started), Some(record)) = (
            mesh_construction_started,
            terrain_performance.current_mut(instrumentation_enabled),
        ) {
            record.cpu_mesh_construction_ms += started.elapsed().as_secs_f64() * 1_000.0;
        }
        let asset_submission_started = instrumentation_enabled.then(Instant::now);
        let mesh_handle = meshes.add(mesh);
        let image_asset_creation_started = instrumentation_enabled.then(Instant::now);
        let local_image_asset_count = usize::from(surface.local_surfaces.is_some()) * 2;

        let (local_albedo, local_normal, local_surface_handles, local_detail_weight) =
            if let Some((albedo, normal)) = surface.local_surfaces {
                let albedo = images.add(albedo);
                let normal = images.add(normal);
                let weight = local_detail_weight(patch.level);
                (
                    albedo.clone(),
                    normal.clone(),
                    Some((albedo, normal)),
                    weight,
                )
            } else {
                let (albedo, normal) =
                    ensure_neutral_local_surface_maps(&mut render_assets, &mut images);
                (albedo, normal, None, 0.0)
            };
        if let (Some(started), Some(record)) = (
            image_asset_creation_started,
            terrain_performance.current_mut(instrumentation_enabled),
        ) {
            record.image_asset_creation_ms += started.elapsed().as_secs_f64() * 1_000.0;
        }
        let material_started = instrumentation_enabled.then(Instant::now);
        let base_material = patch_material(surface.roughness, surface.metallic);
        let body_name = planet.domain_planet.name.clone();
        let global_albedo =
            global_albedo_for(&mut render_assets, &asset_server, &imagery, &body_name)
            .unwrap_or_else(|| {
                bevy::log::warn!(
                    "{body_name} global albedo is unavailable; terrain will use source-derived color only"
                );
                local_albedo.clone()
            });
        let patch_size_m = crate::domain::services::cube_sphere::patch_world_size_m(
            patch.level,
            planet.domain_planet.radius_km as f64 * 1_000.0,
        );
        let morph_start_m = (patch_size_m * MORPH_START_SIZE_FACTOR) as f32;
        let morph_end_m = (patch_size_m * MORPH_END_SIZE_FACTOR) as f32;
        let detail_texture = render_assets.detail_texture.clone().unwrap_or_default();
        let material_handle = materials.add(build_terrain_material(
            base_material.clone(),
            local_albedo.clone(),
            local_normal.clone(),
            local_detail_weight,
            global_albedo.clone(),
            local_albedo.clone(),
            0.0,
            morph_start_m,
            morph_end_m,
            detail_texture.clone(),
            TERRAIN_DETAIL_SCALE,
        ));
        if let (Some(started), Some(record)) = (
            material_started,
            terrain_performance.current_mut(instrumentation_enabled),
        ) {
            record.material_ms += started.elapsed().as_secs_f64() * 1_000.0;
        }

        // Ocean patches contribute a sea-level water cap in the same baked frame
        // as the terrain mesh. The water material is shared.
        let water_mesh_handle = if planet.domain_planet.has_ocean {
            water_mesh_for_patch(
                geometry,
                render_assets.patch_resolution,
                planet.domain_planet.radius_km as f64 * 1_000.0,
                &render_origin.origin,
                body_to_inertial,
                &mut meshes,
            )
        } else {
            None
        };

        // Geometry is already in the rocket-local flight frame; the entity sits
        // at the origin (the rocket's render position).
        let transform = Transform::IDENTITY;

        let vegetation = surface.vegetation;
        let vegetation_mesh_asset_count = usize::from(vegetation.is_some());
        // The shared foliage material and its atlas are created once at startup,
        // so no per-patch material asset is added here.
        let vegetation_material_asset_count = 0usize;
        let vegetation_mesh_handle = vegetation
            .as_ref()
            .map(|(mesh, _)| meshes.add(mesh.clone()));
        let vegetation_material = vegetation_mesh_handle
            .as_ref()
            .and_then(|_| render_assets.vegetation_material.clone());
        let river = surface.river;
        let river_mesh_handle = river.as_ref().map(|(mesh, _)| meshes.add(mesh.clone()));
        let river_material = river_mesh_handle
            .as_ref()
            .and_then(|_| render_assets.river_material.clone());
        if let (Some(started), Some(record)) = (
            asset_submission_started,
            terrain_performance.current_mut(instrumentation_enabled),
        ) {
            record.cpu_to_gpu_submission_ms += started.elapsed().as_secs_f64() * 1_000.0;
            record.mesh_assets_created +=
                1 + vegetation_mesh_asset_count + usize::from(river_mesh_handle.is_some());
            record.material_assets_created += 1 + vegetation_material_asset_count;
            record.image_assets_created += local_image_asset_count;
        }
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

        let activation_started = instrumentation_enabled.then(Instant::now);
        let entity = commands
            .spawn((
                Mesh3d(mesh_handle.clone()),
                MeshMaterial3d(material_handle.clone()),
                transform,
                TerrainPatchRenderState {
                    patch,
                    mesh_handle: mesh_handle.clone(),
                    material_handle: material_handle.clone(),
                    base_material: base_material.clone(),
                    local_albedo: local_albedo.clone(),
                    local_normal: local_normal.clone(),
                    local_detail_weight,
                    global_albedo: global_albedo.clone(),
                    imagery_albedo: local_albedo.clone(),
                    imagery_weight: 0.0,
                    morph_start_m,
                    morph_end_m,
                    detail_texture,
                    detail_scale: TERRAIN_DETAIL_SCALE,
                    local_surface_handles,
                    vegetation_mesh_handle: vegetation_mesh_handle.clone(),
                    water_mesh_handle: water_mesh_handle.clone(),
                    river_mesh_handle: river_mesh_handle.clone(),
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

        // The drainage ribbon shares the vegetation anchor's local frame, so it
        // rotates into the inertial frame exactly like the vegetation child.
        if let (Some(river_mesh_handle), Some(river_material), Some((_, anchor))) =
            (river_mesh_handle.clone(), river_material, river)
        {
            commands.entity(entity).with_children(|parent| {
                parent.spawn((
                    Mesh3d(river_mesh_handle),
                    MeshMaterial3d(river_material),
                    Transform::from_translation(
                        (body_to_inertial * anchor - render_origin.origin).as_vec3(),
                    )
                    .with_rotation(body_to_inertial.as_quat()),
                    NotShadowCaster,
                    NotShadowReceiver,
                    Name::new(format!(
                        "River_{:?}_{}_{}_{}",
                        patch.face, patch.level, patch.tile_x, patch.tile_y
                    )),
                ));
            });
        }

        // The water mesh shares the terrain patch's baked frame, so an identity
        // child transform inherits the patch's later pose corrections.
        if let (Some(water_mesh_handle), Some(water_material)) =
            (water_mesh_handle, render_assets.water_material.clone())
        {
            commands.entity(entity).with_children(|parent| {
                parent.spawn((
                    Mesh3d(water_mesh_handle),
                    MeshMaterial3d(water_material),
                    Transform::IDENTITY,
                    NotShadowCaster,
                    NotShadowReceiver,
                    Name::new(format!(
                        "Water_{:?}_{}_{}_{}",
                        patch.face, patch.level, patch.tile_x, patch.tile_y
                    )),
                ));
            });
        }
        if let (Some(started), Some(record)) = (
            activation_started,
            terrain_performance.current_mut(instrumentation_enabled),
        ) {
            record.activation_ms += started.elapsed().as_secs_f64() * 1_000.0;
            record.patches_activated += 1;
        }
    }
    if let Some(record) = terrain_performance.current_mut(instrumentation_enabled) {
        record.queue_end = pending_uploads.queue.len();
        record.queue_peak = record.queue_peak.max(record.queue_end);
    }
}

/// Drain ready events into the bounded upload queue and recover published
/// patches whose ready events overflowed in an earlier frame.
///
/// Returns `false` when backfill is required but the active planet is
/// unavailable, so no queued upload could be attributed; the caller aborts the
/// frame in that case.
fn enqueue_ready_uploads(
    events: &mut MessageReader<TerrainPatchReady>,
    pending_uploads: &mut PendingTerrainPatchUploads,
    published: &BTreeSet<TerrainPatch>,
    active_planet: Option<Entity>,
    render_index: &TerrainPatchRenderIndex,
    terrain_performance: &mut TerrainPerformanceTelemetry,
    instrumentation_enabled: bool,
) -> bool {
    if let Some(record) = terrain_performance.current_mut(instrumentation_enabled) {
        record.queue_start = pending_uploads.queue.len();
        record.queue_peak = record.queue_start;
    }
    pending_uploads.retain_published_for_planet(active_planet, published);
    for event in events.read().cloned() {
        if let Some(record) = terrain_performance.current_mut(instrumentation_enabled) {
            record.ready_received += 1;
        }
        if active_planet == Some(event.planet_entity)
            && published.contains(&event.patch)
            && pending_uploads.enqueue(event) == TerrainUploadEnqueueResult::Rejected
        {
            if let Some(record) = terrain_performance.current_mut(instrumentation_enabled) {
                record.ready_rejected += 1;
            }
        }
    }
    // A ready-event burst can exceed the bounded queue. Keep the recovery flag
    // until every published patch is queued or rendered; MessageReader cannot
    // replay the events that overflowed in an earlier frame.
    if pending_uploads.needs_backfill {
        let Some(planet_entity) = active_planet else {
            return false;
        };
        let backfill = pending_uploads.backfill_published(planet_entity, published, render_index);
        if let Some(record) = terrain_performance.current_mut(instrumentation_enabled) {
            record.ready_backfilled += backfill.queued;
            record.ready_rejected += backfill.rejected;
        }
    }
    if let Some(record) = terrain_performance.current_mut(instrumentation_enabled) {
        record.queue_peak = record.queue_peak.max(pending_uploads.queue.len());
    }
    true
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
                update_patch_transform(
                    &mut transform,
                    state,
                    body_fixed_to_planet_inertial_rotation(orientation),
                    render_origin.origin,
                );
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
    if let Some(water_mesh_handle) = &state.water_mesh_handle {
        meshes.remove(water_mesh_handle.id());
    }
    if let Some(river_mesh_handle) = &state.river_mesh_handle {
        meshes.remove(river_mesh_handle.id());
    }
    if let Some((albedo, normal)) = &state.local_surface_handles {
        images.remove(albedo.id());
        images.remove(normal.id());
    }
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
    // Terrain vertex colour is otherwise neutral, so it carries the per-vertex
    // LOD morph offset (metres, radial) in the red channel for the terrain
    // vertex shader. Falls back to neutral colour when no morph data exists.
    let colors: Vec<[f32; 4]> = if geometry.morph_deltas.len() == geometry.positions.len() {
        geometry
            .morph_deltas
            .iter()
            .map(|delta| [*delta, 0.0, 0.0, 1.0])
            .collect()
    } else {
        vertex_colors.to_vec()
    };
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);

    // Indices.
    mesh.insert_indices(Indices::U32(geometry.indices.clone()));

    mesh
}

/// Build a sea-level water cap for the ocean part of a patch, or `None` when
/// the patch contains no ocean. Positions share the terrain mesh's baked
/// inertial/render-origin frame so the water can parent to the patch entity
/// with an identity transform. The red vertex-colour channel carries normalized
/// depth for the shader's shallow/deep ramp.
fn water_mesh_for_patch(
    geometry: &PatchGeometry,
    resolution: u32,
    planet_radius_m: f64,
    render_origin: &DVec3,
    body_to_inertial: DQuat,
    meshes: &mut Assets<Mesh>,
) -> Option<Handle<Mesh>> {
    let res = resolution as usize;
    let core = res.checked_mul(res)?;
    if res < 2 || geometry.positions.len() < core {
        return None;
    }
    let sea_level_radius_m = planet_radius_m + WATER_SEA_LEVEL_M + WATER_SURFACE_OFFSET_M;
    let inverse_max_depth = 1.0 / WATER_MAX_VISIBLE_DEPTH_M;

    let mut positions = Vec::with_capacity(core);
    let mut normals = Vec::with_capacity(core);
    let mut depths = Vec::with_capacity(core);
    for point in &geometry.positions[..core] {
        let position = DVec3::from_array(*point);
        let radius = position.length();
        let radial = if radius > f64::EPSILON {
            position / radius
        } else {
            DVec3::Y
        };
        let water_position = body_to_inertial * (radial * sea_level_radius_m) - *render_origin;
        positions.push(water_position.as_vec3().to_array());
        normals.push((body_to_inertial * radial).as_vec3().to_array());
        let depth = ((sea_level_radius_m - radius).max(0.0) * inverse_max_depth).min(1.0) as f32;
        depths.push(depth);
    }

    // Emit a quad when any corner samples below sea level. Vertices above sea
    // level are still placed at sea level and are hidden by the land terrain
    // above them, so the coastline has no hole.
    let mut indices: Vec<u32> = Vec::new();
    for row in 0..res - 1 {
        for column in 0..res - 1 {
            let top_left = (row * res + column) as u32;
            let top_right = (row * res + column + 1) as u32;
            let bottom_left = ((row + 1) * res + column) as u32;
            let bottom_right = ((row + 1) * res + column + 1) as u32;
            let touches_ocean = [top_left, top_right, bottom_left, bottom_right]
                .into_iter()
                .any(|index| depths[index as usize] > 0.0);
            if !touches_ocean {
                continue;
            }
            indices.extend_from_slice(&[
                top_left,
                top_right,
                bottom_right,
                top_left,
                bottom_right,
                bottom_left,
            ]);
        }
    }
    if indices.is_empty() {
        return None;
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, geometry.uvs[..core].to_vec());
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_COLOR,
        depths
            .iter()
            .map(|depth| [*depth, 0.0, 0.0, 1.0])
            .collect::<Vec<_>>(),
    );
    mesh.insert_indices(Indices::U32(indices));
    Some(meshes.add(mesh))
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
    // Every visible patch belongs to the active planet, so resolve the body
    // rotation once and reuse it instead of rebuilding the same quaternion for
    // each of the ~300 patch entities every frame.
    let mut cached: Option<(Entity, DQuat)> = None;
    for (state, mut transform, visibility) in patch_query.iter_mut() {
        if *visibility == Visibility::Hidden {
            continue;
        }
        let body_to_inertial = match cached {
            Some((entity, rotation)) if entity == state.planet_entity => rotation,
            _ => {
                let Ok(planet) = planet_query.get(state.planet_entity) else {
                    continue;
                };
                let Some(orientation) =
                    ephemeris_snapshot.orientation_for_catalog_body(&planet.domain_planet.name)
                else {
                    continue;
                };
                let rotation = body_fixed_to_planet_inertial_rotation(orientation);
                cached = Some((state.planet_entity, rotation));
                rotation
            }
        };
        update_patch_transform(
            &mut transform,
            state,
            body_to_inertial,
            render_origin.origin,
        );
    }
}

fn update_patch_transform(
    transform: &mut Transform,
    state: &TerrainPatchRenderState,
    body_to_inertial: DQuat,
    render_origin: DVec3,
) {
    update_baked_transform(
        transform,
        state.body_to_inertial_at_spawn,
        state.render_origin_at_spawn,
        body_to_inertial,
        render_origin,
    );
}

/// Apply the common f64 pose delta to geometry that was baked at an earlier
/// body orientation and render origin.
fn update_baked_transform(
    transform: &mut Transform,
    body_to_inertial_at_spawn: DQuat,
    render_origin_at_spawn: DVec3,
    body_to_inertial: DQuat,
    render_origin: DVec3,
) {
    let (rotation, translation) = patch_transform_components(
        body_to_inertial_at_spawn,
        render_origin_at_spawn,
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

/// Create the terrain PBR material. The extension starts from the shared global
/// Earth albedo, then layers source-derived detail.
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
    fn spatial_lod_transitions_keep_a_2_to_1_stitch_compatible_resolution() {
        let config = TerrainRenderConfig::default();
        let coarse = TerrainPatch::for_direction(DVec3::Z, 11);
        let fine = TerrainPatch::for_direction(DVec3::Z, 12);

        assert_eq!(config.patch_resolution_for(coarse), 33);
        assert_eq!(
            config.patch_resolution_for(fine),
            config.patch_resolution_for(coarse),
            "a 2:1 spatial LOD edge must not become a 4:1 grid-sample transition"
        );
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
    fn close_terrain_surface_maps_preserve_both_uv_sets() {
        let source = crate::domain::services::terrain_source::ProceduralTerrainSource::new(
            99, 2_000.0, 800.0, 0,
        );
        let patch = TerrainPatch::for_direction(DVec3::new(0.3, 0.4, 1.0).normalize(), 2);
        let geometry = build_patch_geometry(&patch, &source, 6_371_000.0, 5, 40.0);
        let coarse = crate::infrastructure::bevy_adapters::terrain::surface::prepare_patch_surface(
            &source,
            &TerrainPatch::for_direction(DVec3::Z, 0),
            &geometry,
            6_371_000.0,
        );
        let fine = crate::infrastructure::bevy_adapters::terrain::surface::prepare_patch_surface(
            &source,
            &TerrainPatch::for_direction(DVec3::Z, 12),
            &geometry,
            6_371_000.0,
        );

        assert!(coarse
            .vertex_colors
            .iter()
            .all(|color| *color == [1.0, 1.0, 1.0, 1.0]));
        assert_eq!(coarse.vertex_colors, fine.vertex_colors);
        assert!(coarse.local_surfaces.is_none());
        assert!(fine.local_surfaces.is_some());
        assert_eq!(geometry.uvs.len(), geometry.local_uvs.len());

        let mesh = patch_geometry_to_mesh(
            &geometry,
            &DVec3::ZERO,
            DQuat::IDENTITY,
            &fine.vertex_colors,
        );
        assert!(mesh.attribute(Mesh::ATTRIBUTE_UV_0).is_some());
        assert!(mesh.attribute(Mesh::ATTRIBUTE_UV_1).is_some());
    }

    #[test]
    fn terrain_material_leaves_albedo_to_the_source_derived_extension() {
        let material = patch_material(0.7, 0.0);

        assert!(material.base_color_texture.is_none());
        assert!(material.emissive_texture.is_none());
        assert_eq!(material.emissive, LinearRgba::BLACK);
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
                    base_material: StandardMaterial::default(),
                    local_albedo: Handle::default(),
                    local_normal: Handle::default(),
                    local_detail_weight: 0.0,
                    global_albedo: Handle::default(),
                    imagery_albedo: Handle::default(),
                    imagery_weight: 0.0,
                    morph_start_m: 0.0,
                    morph_end_m: 0.0,
                    detail_texture: Handle::default(),
                    detail_scale: 0.0,
                    local_surface_handles: None,
                    vegetation_mesh_handle: None,
                    water_mesh_handle: None,
                    river_mesh_handle: None,
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
                    base_material: StandardMaterial::default(),
                    local_albedo: Handle::default(),
                    local_normal: Handle::default(),
                    local_detail_weight: 0.0,
                    global_albedo: Handle::default(),
                    imagery_albedo: Handle::default(),
                    imagery_weight: 0.0,
                    morph_start_m: 0.0,
                    morph_end_m: 0.0,
                    detail_texture: Handle::default(),
                    detail_scale: 0.0,
                    local_surface_handles: None,
                    vegetation_mesh_handle: None,
                    water_mesh_handle: None,
                    river_mesh_handle: None,
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
            assert_eq!(
                pending.enqueue(TerrainPatchReady {
                    patch,
                    planet_entity,
                }),
                TerrainUploadEnqueueResult::Queued
            );
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

        assert_eq!(
            pending.enqueue(TerrainPatchReady {
                patch: first,
                planet_entity,
            }),
            TerrainUploadEnqueueResult::Queued
        );
        assert_eq!(
            pending.enqueue(TerrainPatchReady {
                patch: first,
                planet_entity,
            }),
            TerrainUploadEnqueueResult::Duplicate
        );
        for tile_x in 1..MAX_PENDING_PATCH_UPLOADS as u32 {
            assert_eq!(
                pending.enqueue(TerrainPatchReady {
                    patch: TerrainPatch { tile_x, ..first },
                    planet_entity,
                }),
                TerrainUploadEnqueueResult::Queued
            );
        }
        assert_eq!(
            pending.enqueue(TerrainPatchReady {
                patch: TerrainPatch {
                    tile_x: MAX_PENDING_PATCH_UPLOADS as u32,
                    ..first
                },
                planet_entity,
            }),
            TerrainUploadEnqueueResult::Rejected
        );
        assert_eq!(pending.queue.len(), MAX_PENDING_PATCH_UPLOADS);
        assert!(pending.needs_backfill);

        pending.pop_front();
        assert!(pending.needs_backfill);
    }

    #[test]
    fn ready_event_overflow_backfills_every_published_patch_without_duplicates() {
        let planet_entity = Entity::PLACEHOLDER;
        let first = TerrainPatch {
            face: CubeFace::PosZ,
            level: 12,
            tile_x: 0,
            tile_y: 0,
        };
        let published = (0..=MAX_PENDING_PATCH_UPLOADS as u32)
            .map(|tile_x| TerrainPatch { tile_x, ..first })
            .collect::<std::collections::BTreeSet<_>>();
        let mut pending = PendingTerrainPatchUploads::default();
        let mut render_index = TerrainPatchRenderIndex::default();

        for patch in &published {
            pending.enqueue(TerrainPatchReady {
                patch: *patch,
                planet_entity,
            });
        }
        assert!(pending.needs_backfill);

        while pending.needs_backfill || !pending.queue.is_empty() {
            pending.backfill_published(planet_entity, &published, &render_index);
            for _ in 0..MAX_PATCH_UPLOADS_PER_FRAME {
                let Some(event) = pending.pop_front() else {
                    break;
                };
                let key = TerrainPatchRenderKey::from(&event);
                assert!(
                    render_index.0.insert(key, Entity::PLACEHOLDER).is_none(),
                    "a patch must never activate twice"
                );
            }
        }

        assert!(!pending.needs_backfill);
        assert!(pending.queue.is_empty());
        assert_eq!(render_index.0.len(), published.len());
        assert!(published.iter().all(|patch| {
            render_index.0.contains_key(&TerrainPatchRenderKey {
                planet_entity,
                patch: *patch,
            })
        }));
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
            base_material: StandardMaterial::default(),
            local_albedo: Handle::default(),
            local_normal: Handle::default(),
            local_detail_weight: 0.0,
            global_albedo: Handle::default(),
            imagery_albedo: Handle::default(),
            imagery_weight: 0.0,
            morph_start_m: 0.0,
            morph_end_m: 0.0,
            detail_texture: Handle::default(),
            detail_scale: 0.0,
            local_surface_handles: None,
            vegetation_mesh_handle: Some(vegetation_mesh_handle.clone()),
            water_mesh_handle: None,
            river_mesh_handle: None,
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
