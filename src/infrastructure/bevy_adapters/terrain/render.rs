//! Terrain rendering plugin (AGENTS.md sections 27-28).
//!
//! Spawns GPU meshes and materials for cube-sphere LOD terrain patches from the
//! streaming manager, with PBR shaders for planetary surfaces and a floating
//! origin for precision at planetary scale.
//!
//! The ready-patch upload queue lives in the [`uploads`] submodule.

use crate::domain::services::cube_sphere::{
    face_uv, face_uv_to_direction, CubeFace, PatchGeometry, TerrainPatch,
};
use crate::domain::services::ephemeris::NaifBodyId;
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
    layer_texture_set, local_detail_weight, terrain_detail_texture, vegetation_atlas,
    LAYER_NORMAL_STRENGTH, LAYER_TILING_SCALE, NEAR_DETAIL_SCALE, NEAR_DETAIL_STRENGTH,
};
use crate::infrastructure::bevy_adapters::terrain::water::{
    WaterExtension, WaterMaterial, WaterParams, WaterQualityConfig,
};
use bevy::asset::{Assets, RenderAssetUsages};
use bevy::ecs::message::Message;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::light::NotShadowCaster;
use bevy::math::{DQuat, DVec3};
use bevy::pbr::{ExtendedMaterial, MaterialExtension, MaterialPlugin};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, Extent3d, TextureDimension, TextureFormat};
use bevy::shader::ShaderRef;
use bevy_mesh::{Indices, PrimitiveTopology};
use std::collections::{HashMap, HashSet};
use std::time::Instant;

mod uploads;

use self::uploads::{
    enqueue_ready_uploads, PendingTerrainPatchUploads, TerrainPatchRenderIndex,
    TerrainPatchRenderKey,
};

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
    /// Baked terrain occlusion sampled with the patch-local UV1: red is the
    /// self-shadow visibility toward the shared ephemeris Sun, green is the
    /// sky/ambient visibility. Neutral (1, 1) where a patch has no bake.
    #[texture(111)]
    #[sampler(112)]
    terrain_occlusion: Handle<Image>,
    /// Fraction of the baked self-shadow applied to the direct-sun term.
    #[uniform(104)]
    self_shadow_strength: f32,
    /// Fraction of the baked sky occlusion applied to the indirect term.
    #[uniform(104)]
    sky_occlusion_strength: f32,
    /// Shared per-layer albedo/roughness texture array; rgb is the tiled albedo
    /// variation and alpha is the layer roughness. Layer index matches the
    /// terrain layer catalog.
    #[texture(113, dimension = "2d_array")]
    #[sampler(114)]
    layer_albedo_roughness: Handle<Image>,
    /// Shared per-layer tangent-space normal texture array.
    #[texture(115, dimension = "2d_array")]
    #[sampler(116)]
    layer_normal: Handle<Image>,
    /// Per-patch layer-weight map ([grass, soil, rock, sand]); snow is the
    /// remaining unit. Neutral (1, 0, 0, 0) for coarse patches and the fallback.
    #[texture(117)]
    #[sampler(118)]
    layer_weights: Handle<Image>,
    /// One while the layered path is active for this patch, zero for the
    /// single-layer fallback. Evaluated per patch, not per fragment.
    #[uniform(104)]
    layer_blend_weight: f32,
    /// Repetitions per metre of a ground-layer texture.
    #[uniform(104)]
    layer_tiling_scale: f32,
    /// Repetitions of a ground-layer texture across the patch-local UV, matching
    /// the world-space triplanar scale so the two projections agree physically.
    #[uniform(104)]
    layer_patch_uv_scale: f32,
    /// Gain applied to the blended layer tangent-space normal.
    #[uniform(104)]
    layer_normal_strength: f32,
    /// Repetitions per metre of the near-camera detail overlay.
    #[uniform(104)]
    near_detail_scale: f32,
    /// Gain of the near-camera detail overlay, faded by view distance.
    #[uniform(104)]
    near_detail_strength: f32,
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
    terrain_occlusion: Handle<Image>,
    self_shadow_strength: f32,
    sky_occlusion_strength: f32,
    layer_albedo_roughness: Handle<Image>,
    layer_normal: Handle<Image>,
    layer_weights: Handle<Image>,
    layer_blend_weight: f32,
    layer_tiling_scale: f32,
    layer_patch_uv_scale: f32,
    layer_normal_strength: f32,
    near_detail_scale: f32,
    near_detail_strength: f32,
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
            terrain_occlusion,
            self_shadow_strength,
            sky_occlusion_strength,
            layer_albedo_roughness,
            layer_normal,
            layer_weights,
            layer_blend_weight,
            layer_tiling_scale,
            layer_patch_uv_scale,
            layer_normal_strength,
            near_detail_scale,
            near_detail_strength,
        },
    }
}
/// Layered-material inputs for one patch, preserved so an imagery upgrade can
/// rebuild the shared material without re-deriving layer state.
#[derive(Debug, Clone)]
pub(crate) struct LayerMaterialState {
    /// Shared per-layer albedo/roughness array (never released per patch).
    pub(crate) albedo_roughness: Handle<Image>,
    /// Shared per-layer normal array (never released per patch).
    pub(crate) normal: Handle<Image>,
    /// Per-patch layer-weight map, or the shared neutral placeholder.
    pub(crate) weights: Handle<Image>,
    /// One while the layered path is active, zero for the single-layer fallback.
    pub(crate) blend_weight: f32,
    pub(crate) tiling_scale: f32,
    pub(crate) patch_uv_scale: f32,
    pub(crate) normal_strength: f32,
    pub(crate) near_detail_scale: f32,
    pub(crate) near_detail_strength: f32,
    /// Whether `weights` is owned by this patch and released with it.
    pub(crate) weights_owned: bool,
}

impl Default for LayerMaterialState {
    fn default() -> Self {
        Self {
            albedo_roughness: Handle::default(),
            normal: Handle::default(),
            weights: Handle::default(),
            blend_weight: 0.0,
            tiling_scale: 1.0,
            patch_uv_scale: 1.0,
            normal_strength: 0.0,
            near_detail_scale: 1.0,
            near_detail_strength: 0.0,
            weights_owned: false,
        }
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
    /// Layered ground-material inputs (shared layer sets, per-patch weights).
    pub(crate) layer: LayerMaterialState,
    pub vegetation_mesh_handle: Option<Handle<Mesh>>,
    /// Sea-level water cap for patches that contain ocean, released with the
    /// patch. The water material itself is shared unless this patch has its own
    /// landscape-shadow material.
    pub water_mesh_handle: Option<Handle<Mesh>>,
    /// Per-patch ocean material carrying this patch's baked terrain occlusion
    /// map, or `None` when the shared water material is used.
    pub water_material_handle: Option<Handle<WaterMaterial>>,
    /// Drainage ribbon for patches crossed by a river channel, released with the
    /// patch. Shares the river material.
    pub river_mesh_handle: Option<Handle<Mesh>>,
    pub planet_entity: Entity,
    /// Body-fixed-to-inertial rotation used to bake this mesh's vertices.
    pub body_to_inertial_at_spawn: DQuat,
    /// Render origin used to bake this mesh's vertices.
    pub render_origin_at_spawn: DVec3,
    /// Baked terrain-occlusion map bound to this patch's material, or the
    /// shared neutral map when the patch is below the occlusion bake level.
    pub(crate) occlusion_texture: Handle<Image>,
    /// Whether `occlusion_texture` is owned by this patch and must be released
    /// with it. The shared neutral map is never released here.
    pub(crate) occlusion_owned: bool,
    /// Body-fixed height field the occlusion map was baked from, retained so a
    /// material Sun-direction change can refresh the patch without rebuilding
    /// its geometry.
    pub(crate) occlusion_field: Option<PatchHeightField>,
    /// Inertial Sun direction recorded by the occlusion bake. Rotation alone
    /// does not change it, so resident patches are only refreshed when the
    /// shared ephemeris Sun direction changes materially.
    pub(crate) baked_sun_inertial: DVec3,
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
    /// Shared per-layer albedo/roughness texture array (one upload for all
    /// patches, so residency does not grow with the visible patch count).
    layer_albedo_roughness: Option<Handle<Image>>,
    /// Shared per-layer tangent-space normal texture array.
    layer_normal: Option<Handle<Image>>,
    /// One shared neutral layer-weight map so coarse patches and the single-layer
    /// fallback never allocate a per-patch image.
    neutral_layer_weights: Option<Handle<Image>>,
    /// One shared neutral occlusion map (self-shadow = sky visibility = 1) so
    /// patches without a bake never allocate a per-patch image.
    neutral_occlusion: Option<Handle<Image>>,
    /// Bounded occlusion bake configuration, mirrored from
    /// `TerrainOcclusionConfig` so the spawn system does not need another
    /// resource parameter.
    occlusion: TerrainOcclusionConfig,
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

/// Bounded, deterministic configuration for baked terrain occlusion.
///
/// Every field caps work rather than expressing a target: the per-bake sun and
/// sky ray-march counts, the horizon direction count, and the occlusion map
/// resolution together bound the height-field samples a patch bake may cost.
/// Defaults are deliberately conservative and are only raised against measured
/// `TerrainPerformanceTelemetry` evidence.
#[derive(Resource, Debug, Clone)]
pub struct TerrainOcclusionConfig {
    /// Occlusion map resolution in texels per side, baked in patch-local UV.
    pub texture_resolution: u32,
    /// Height-field samples marched along each sun ray.
    pub sun_samples: u32,
    /// Hemisphere directions sampled for sky occlusion; the final direction is
    /// the local zenith. The rest are evenly spaced at `sky_elevation_deg`.
    pub sky_directions: u32,
    /// Height-field samples marched along each sky ray.
    pub sky_samples: u32,
    /// Elevation of the ring sky directions above the local horizon, degrees.
    pub sky_elevation_deg: f64,
    /// Maximum sun-ray range in meters. Terrain beyond it is not an occluder.
    pub sun_max_distance_m: f64,
    /// Maximum sky-ray range in meters.
    pub sky_max_distance_m: f64,
    /// Terrain penetration mapped to full occlusion. Controls shadow softness
    /// at grazing angles.
    pub softness_m: f64,
    /// Patches below this level get the neutral map instead of a bake. Coarse
    /// roots cannot resolve terrain occlusion anyway.
    pub min_patch_level: u32,
    /// Fraction of the baked self-shadow applied to the direct-sun term.
    pub self_shadow_strength: f32,
    /// Fraction of the baked sky occlusion applied to the indirect term.
    pub sky_occlusion_strength: f32,
    /// Inertial Sun-direction change (radians) that refreshes resident patches.
    pub refresh_tolerance_rad: f64,
    /// Maximum resident patches refreshed against a new Sun direction per frame.
    pub max_refreshes_per_frame: u32,
    /// Bounded occlusion terms combined per rendered fragment (self-shadow and
    /// sky occlusion). Kept explicit so the per-fragment budget is visible in
    /// telemetry even though the terms are interpolated, not re-marched.
    pub samples_per_fragment: u32,
}

impl Default for TerrainOcclusionConfig {
    fn default() -> Self {
        Self {
            texture_resolution: 16,
            sun_samples: 5,
            sky_directions: 4,
            sky_samples: 2,
            sky_elevation_deg: 40.0,
            sun_max_distance_m: 6_000.0,
            sky_max_distance_m: 1_500.0,
            softness_m: 80.0,
            // Bake every streamed patch: landscape self-shadow must cover the
            // far terrain beyond the 20 km directional-shadow cascade range,
            // which is dominated by coarse patches.
            min_patch_level: 0,
            self_shadow_strength: 1.0,
            sky_occlusion_strength: 0.7,
            refresh_tolerance_rad: 0.02,
            max_refreshes_per_frame: 2,
            samples_per_fragment: 2,
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
            .init_resource::<TerrainPerformanceTelemetry>()
            .init_resource::<PendingTerrainPatchHides>()
            .init_resource::<TerrainPatchRenderIndex>()
            .init_resource::<TerrainImageryConfig>()
            .init_resource::<TerrainImageryResource>()
            .init_resource::<TerrainOcclusionConfig>()
            .init_resource::<WaterQualityConfig>()
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
                    refresh_terrain_occlusion,
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
#[expect(
    clippy::too_many_arguments,
    reason = "Startup asset preparation binds independent shared terrain assets and configuration."
)]
fn prepare_terrain_render_assets(
    config: Res<TerrainRenderConfig>,
    occlusion_config: Res<TerrainOcclusionConfig>,
    water_quality: Res<WaterQualityConfig>,
    asset_server: Res<AssetServer>,
    imagery: Res<TerrainImageryResource>,
    mut render_assets: ResMut<TerrainRenderAssets>,
    mut images: ResMut<Assets<Image>>,
    mut standard_materials: ResMut<Assets<StandardMaterial>>,
    mut water_materials: ResMut<Assets<WaterMaterial>>,
) {
    render_assets.patch_resolution = config.patch_resolution;
    render_assets.occlusion = occlusion_config.clone();
    let _ = global_albedo_for(&mut render_assets, &asset_server, &imagery, "Earth");
    ensure_neutral_local_surface_maps(&mut render_assets, &mut images);
    // One shared micro-detail texture across every patch.
    render_assets.detail_texture = Some(images.add(terrain_detail_texture()));
    // One shared procedural layer PBR set across every patch. Generated
    // deterministically at startup, so the layered path needs no asset files.
    let layer_set = layer_texture_set();
    render_assets.layer_albedo_roughness = Some(images.add(layer_set.albedo_roughness));
    render_assets.layer_normal = Some(images.add(layer_set.normal));
    ensure_neutral_layer_weights(&mut render_assets, &mut images);
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
    let neutral_occlusion = neutral_occlusion_image(&mut render_assets, &mut images);
    let ocean_params = WaterParams {
        wave_components: water_quality.wave_components_f32(),
        foam_coverage: water_quality.foam_coverage,
        ..WaterParams::default()
    };
    let river_params = WaterParams {
        wave_components: water_quality.wave_components_f32(),
        foam_coverage: water_quality.foam_coverage,
        ..WaterParams::river()
    };
    render_assets.water_material = Some(water_materials.add(WaterMaterial {
        base: water_base_material(water_quality.refraction),
        extension: WaterExtension::new(ocean_params, neutral_occlusion.clone()),
    }));
    render_assets.river_material = Some(water_materials.add(WaterMaterial {
        base: water_base_material(false),
        extension: WaterExtension::new(river_params, neutral_occlusion),
    }));
}

/// Shared base material for the water surface: blended, double-sided, and very
/// smooth so the fragment shader's ripple normal drives a tight sun glint. When
/// `refraction` is enabled the material also asks Bevy for screen-space
/// transmission; without refraction buffers (or when disabled) the alpha-blended
/// depth path is used unchanged.
fn water_base_material(refraction: bool) -> StandardMaterial {
    StandardMaterial {
        base_color: Color::WHITE,
        alpha_mode: AlphaMode::Blend,
        cull_mode: None,
        perceptual_roughness: 0.05,
        metallic: 0.0,
        specular_transmission: if refraction { 0.35 } else { 0.0 },
        ior: 1.33,
        thickness: if refraction { 2.0 } else { 0.0 },
        ..default()
    }
}

/// Advance the shared water wave phase. Presentation only; never read by the
/// simulation, terrain source, or collision.
fn update_water_material(time: Res<Time>, mut water_materials: ResMut<Assets<WaterMaterial>>) {
    let elapsed_s = time.elapsed_secs();
    // Every water material (shared and per-patch) advances the same presentation
    // clock, so a shared wave field stays coherent across all caps.
    for (_, material) in water_materials.iter_mut() {
        material.extension.params.time_s = elapsed_s;
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

/// Neutral layer-weight map used by coarse patches and the single-layer
/// fallback. The red channel is fully set so the derived snow remainder is zero;
/// the shader ignores the map entirely while `layer_blend_weight` is zero.
fn ensure_neutral_layer_weights(
    render_assets: &mut TerrainRenderAssets,
    images: &mut Assets<Image>,
) -> Handle<Image> {
    render_assets
        .neutral_layer_weights
        .get_or_insert_with(|| images.add(neutral_surface_image([255, 0, 0, 0])))
        .clone()
}

/// Resolve the layered-material inputs for one patch. A produced per-patch
/// weight map selects the layered path; otherwise the shared neutral weight map
/// and the single-layer fallback are used.
fn build_layer_material_state(
    render_assets: &mut TerrainRenderAssets,
    images: &mut Assets<Image>,
    patch_layer_weights: Option<Image>,
    patch_size_m: f64,
) -> LayerMaterialState {
    let albedo_roughness = render_assets
        .layer_albedo_roughness
        .clone()
        .unwrap_or_default();
    let normal = render_assets.layer_normal.clone().unwrap_or_default();
    let (weights, weights_owned, blend_weight) = match patch_layer_weights {
        Some(image) => (images.add(image), true, 1.0),
        None => (
            ensure_neutral_layer_weights(render_assets, images),
            false,
            0.0,
        ),
    };
    LayerMaterialState {
        albedo_roughness,
        normal,
        weights,
        blend_weight,
        tiling_scale: LAYER_TILING_SCALE,
        patch_uv_scale: (patch_size_m * f64::from(LAYER_TILING_SCALE)) as f32,
        normal_strength: LAYER_NORMAL_STRENGTH,
        near_detail_scale: NEAR_DETAIL_SCALE,
        near_detail_strength: NEAR_DETAIL_STRENGTH,
        weights_owned,
    }
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
/// Bundled terrain asset stores for the patch spawn system. Grouping them keeps
/// the system within the ECS parameter limit while preserving separate mutable
/// access to each asset store.
#[derive(bevy::ecs::system::SystemParam)]
struct TerrainSpawnAssets<'w> {
    meshes: ResMut<'w, Assets<Mesh>>,
    materials: ResMut<'w, Assets<TerrainMaterial>>,
    water_materials: ResMut<'w, Assets<WaterMaterial>>,
    images: ResMut<'w, Assets<Image>>,
}

#[expect(
    clippy::too_many_arguments,
    reason = "This renderer upload system coordinates independent terrain assets, events, and state."
)]
fn spawn_patch_mesh_system(
    mut commands: Commands,
    mut events: MessageReader<TerrainPatchReady>,
    mut pending_uploads: ResMut<PendingTerrainPatchUploads>,
    mut render_index: ResMut<TerrainPatchRenderIndex>,
    assets: TerrainSpawnAssets,
    mut render_assets: ResMut<TerrainRenderAssets>,
    mut streaming: ResMut<TerrainStreamingResource>,
    asset_server: Res<AssetServer>,
    imagery: Res<TerrainImageryResource>,
    render_origin: Res<RenderOrigin>,
    ephemeris_snapshot: Res<EphemerisSnapshot>,
    planet_query: Query<&PlanetComponent>,
    water_quality: Res<WaterQualityConfig>,
    performance_config: Res<PerformanceMetricsConfig>,
    mut terrain_performance: ResMut<TerrainPerformanceTelemetry>,
) {
    let TerrainSpawnAssets {
        mut meshes,
        mut materials,
        mut water_materials,
        mut images,
    } = assets;
    let instrumentation_enabled = performance_config.instrumentation_enabled();
    let occlusion_config = render_assets.occlusion.clone();
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

        // Bake the landscape self-shadow and sky occlusion from the same
        // authoritative height field and the shared ephemeris Sun. The term is
        // body-fixed, presentation-only, and bounded per patch.
        let occlusion_started = instrumentation_enabled.then(Instant::now);
        let patch_resolution = render_assets.patch_resolution;
        let (occlusion_texture, occlusion_field, occlusion_samples, baked_sun_inertial) =
            bake_terrain_occlusion_for_patch(
                geometry,
                patch,
                patch_resolution,
                body_to_inertial,
                &ephemeris_snapshot,
                &planet.domain_planet.name,
                &occlusion_config,
                &mut render_assets,
                &mut images,
            );
        let occlusion_owned = occlusion_field.is_some();
        if let (Some(started), Some(record)) = (
            occlusion_started,
            terrain_performance.current_mut(instrumentation_enabled),
        ) {
            record.occlusion_bake_ms += started.elapsed().as_secs_f64() * 1_000.0;
            record.occlusion_bake_samples += occlusion_samples;
            record.occlusion_patches_baked += usize::from(occlusion_owned);
            record.occlusion_fragment_samples = occlusion_config.samples_per_fragment as usize;
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
        // Select the layered path versus the single-layer fallback from whether
        // a per-patch weight map was produced (native `dem` only).
        let layer = build_layer_material_state(
            &mut render_assets,
            &mut images,
            surface.layer_weights,
            patch_size_m,
        );
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
            occlusion_texture.clone(),
            occlusion_config.self_shadow_strength,
            occlusion_config.sky_occlusion_strength,
            layer.albedo_roughness.clone(),
            layer.normal.clone(),
            layer.weights.clone(),
            layer.blend_weight,
            layer.tiling_scale,
            layer.patch_uv_scale,
            layer.normal_strength,
            layer.near_detail_scale,
            layer.near_detail_strength,
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
                // Budget-gate displacement subdivision for the ocean cap.
                render_assets
                    .patch_resolution
                    .min(water_quality.max_subdivision.max(2)),
                planet.domain_planet.radius_km as f64 * 1_000.0,
                &render_origin.origin,
                body_to_inertial,
                &mut meshes,
            )
        } else {
            None
        };
        // Ocean water over a baked patch gets its own material so the patch's
        // terrain occlusion map can shade the sea beyond the shadow cascades.
        let water_material_handle = (water_mesh_handle.is_some() && occlusion_owned).then(|| {
            let params = WaterParams {
                wave_components: water_quality.wave_components_f32(),
                foam_coverage: water_quality.foam_coverage,
                ..WaterParams::default()
            };
            water_materials.add(WaterMaterial {
                base: water_base_material(water_quality.refraction),
                extension: WaterExtension::new(params, occlusion_texture.clone()),
            })
        });

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
            record.image_assets_created += local_image_asset_count
                + usize::from(occlusion_owned)
                + usize::from(layer.weights_owned);
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
                    layer,
                    vegetation_mesh_handle: vegetation_mesh_handle.clone(),
                    water_mesh_handle: water_mesh_handle.clone(),
                    water_material_handle: water_material_handle.clone(),
                    river_mesh_handle: river_mesh_handle.clone(),
                    planet_entity: event.planet_entity,
                    body_to_inertial_at_spawn: body_to_inertial,
                    render_origin_at_spawn: render_origin.origin,
                    occlusion_texture: occlusion_texture.clone(),
                    occlusion_owned,
                    occlusion_field,
                    baked_sun_inertial,
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
                    // Receives the shared directional shadow; a thin blended cap
                    // must not cast one.
                    NotShadowCaster,
                    Name::new(format!(
                        "River_{:?}_{}_{}_{}",
                        patch.face, patch.level, patch.tile_x, patch.tile_y
                    )),
                ));
            });
        }

        // The water mesh shares the terrain patch's baked frame, so an identity
        // child transform inherits the patch's later pose corrections.
        if let Some(water_mesh_handle) = water_mesh_handle {
            if let Some(water_material) =
                water_material_handle.or_else(|| render_assets.water_material.clone())
            {
                commands.entity(entity).with_children(|parent| {
                    parent.spawn((
                        Mesh3d(water_mesh_handle),
                        MeshMaterial3d(water_material),
                        Transform::IDENTITY,
                        // Water receives the shared directional and terrain shadow;
                        // it stays a non-caster so the blended cap never darkens
                        // the terrain beneath it.
                        NotShadowCaster,
                        Name::new(format!(
                            "Water_{:?}_{}_{}_{}",
                            patch.face, patch.level, patch.tile_x, patch.tile_y
                        )),
                    ));
                });
            }
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
#[expect(
    clippy::too_many_arguments,
    reason = "This renderer release system coordinates independent terrain asset stores, events, and state."
)]
fn despawn_patch_mesh_system(
    mut commands: Commands,
    mut events: MessageReader<TerrainPatchEvicted>,
    mut render_index: ResMut<TerrainPatchRenderIndex>,
    render_query: Query<&TerrainPatchRenderState>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<TerrainMaterial>>,
    mut water_materials: ResMut<Assets<WaterMaterial>>,
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
            release_patch_render_assets(
                state,
                &mut meshes,
                &mut materials,
                &mut water_materials,
                &mut images,
            );
        }
    }
}

fn release_patch_render_assets(
    state: &TerrainPatchRenderState,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<TerrainMaterial>,
    water_materials: &mut Assets<WaterMaterial>,
    images: &mut Assets<Image>,
) {
    meshes.remove(state.mesh_handle.id());
    materials.remove(state.material_handle.id());
    if let Some(water_material_handle) = &state.water_material_handle {
        water_materials.remove(water_material_handle.id());
    }
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
    if state.layer.weights_owned {
        images.remove(state.layer.weights.id());
    }
    if state.occlusion_owned {
        images.remove(state.occlusion_texture.id());
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
    // Patch-local UV1 lets the water shader sample the patch's baked terrain
    // occlusion map, so landscape shadow reaches the sea beyond the cascades.
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, geometry.local_uvs[..core].to_vec());
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

// Baked terrain self-shadow and sky occlusion (AGENTS.md sections 20, 27, 44).
//
// Both terms are pure functions of the generated patch height field (the
// authoritative rendered height: rendering never resamples terrain height), the
// patch identity, and the body-fixed ephemeris Sun direction. They are baked in
// the same body-fixed frame as the patch mesh, so render-origin rebasing and
// body rotation never invalidate the stored texels. Nothing here reads frame
// time, camera pose, or writes simulation state.

/// Body-fixed local height field of one generated terrain patch.
///
/// The generated patch geometry is the authoritative rendered height field, so
/// the bake ray-marches it directly instead of resampling the terrain source.
/// That keeps the bake O(1) per sample (a bilinear grid lookup) with no DEM or
/// procedural work on the main thread. Cross-patch occluders are approximated by
/// clamping to the shared edge; the directional shadow map covers the near range
/// where that approximation matters most.
#[derive(Clone, Debug)]
pub(crate) struct PatchHeightField {
    face: CubeFace,
    resolution: usize,
    uv_bounds: (f64, f64, f64, f64),
    /// Terrain radius in meters for each core grid vertex, row-major.
    radii_m: Vec<f32>,
}

impl PatchHeightField {
    /// Build the field from a generated patch's core grid. Skirt vertices are
    /// ignored: they duplicate the boundary and are hidden crack geometry.
    fn from_geometry(
        geometry: &PatchGeometry,
        patch: TerrainPatch,
        resolution: u32,
    ) -> Option<Self> {
        let resolution = resolution.max(2) as usize;
        let core = resolution * resolution;
        if geometry.positions.len() < core {
            return None;
        }
        let radii_m = geometry.positions[..core]
            .iter()
            .map(|position| DVec3::from_array(*position).length() as f32)
            .collect();
        Some(Self {
            face: patch.face,
            resolution,
            uv_bounds: patch.uv_bounds(),
            radii_m,
        })
    }

    /// Terrain radius sampled at a body-fixed point, or `None` when the point
    /// left this patch's cube face. Coordinates outside the patch are clamped to
    /// the shared edge so the field stays continuous across patch boundaries.
    fn sample_radius_m(&self, direction: DVec3) -> Option<f64> {
        let (face, u, v) = face_uv(direction);
        if face != self.face {
            return None;
        }
        let (u0, v0, u1, v1) = self.uv_bounds;
        let span_u = (u1 - u0).abs().max(f64::EPSILON);
        let span_v = (v1 - v0).abs().max(f64::EPSILON);
        let fu = ((u - u0) / span_u).clamp(0.0, 1.0);
        let fv = ((v - v0) / span_v).clamp(0.0, 1.0);
        let last = (self.resolution - 1) as f64;
        let x = fu * last;
        let y = fv * last;
        let x0 = x.floor() as usize;
        let y0 = y.floor() as usize;
        let x1 = (x0 + 1).min(self.resolution - 1);
        let y1 = (y0 + 1).min(self.resolution - 1);
        let tx = x - x0 as f64;
        let ty = y - y0 as f64;
        let radius =
            |row: usize, column: usize| self.radii_m[row * self.resolution + column] as f64;
        let r00 = radius(y0, x0);
        let r10 = radius(y0, x1);
        let r01 = radius(y1, x0);
        let r11 = radius(y1, x1);
        let near = r00 + (r10 - r00) * tx;
        let far = r01 + (r11 - r01) * tx;
        Some(near + (far - near) * ty)
    }
}

/// Hermite smoothstep over `[edge0, edge1]`.
fn smoothstep01(edge0: f64, edge1: f64, x: f64) -> f64 {
    if edge1 <= edge0 {
        return if x < edge0 { 0.0 } else { 1.0 };
    }
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// March one ray from a surface point and return `[0, 1]` visibility.
///
/// A sample whose terrain radius exceeds the ray's radius is penetrating terrain;
/// the deepest penetration above the configured softness maps to full occlusion.
/// Quadratic sample spacing keeps near-field occluders (ridges, rock contacts)
/// dense without paying for distant detail.
fn ray_visibility(
    field: &PatchHeightField,
    origin_m: DVec3,
    direction: DVec3,
    max_distance_m: f64,
    samples: u32,
    softness_m: f64,
) -> f32 {
    if samples == 0 || direction.length_squared() <= 0.0 || max_distance_m <= 0.0 {
        return 1.0;
    }
    let direction = direction.normalize();
    let mut max_penetration_m: f64 = 0.0;
    for step in 1..=samples {
        let fraction = step as f64 / samples as f64;
        let distance_m = max_distance_m * fraction * fraction;
        let sample_m = origin_m + direction * distance_m;
        let radius_m = sample_m.length();
        if !radius_m.is_finite() {
            continue;
        }
        let Some(terrain_radius_m) = field.sample_radius_m(sample_m / radius_m) else {
            continue;
        };
        max_penetration_m = max_penetration_m.max(terrain_radius_m - radius_m);
    }
    if max_penetration_m <= 0.0 {
        return 1.0;
    }
    if softness_m <= 0.0 {
        return 0.0;
    }
    (1.0 - smoothstep01(0.0, softness_m, max_penetration_m)) as f32
}

/// Sky-hemisphere directions for ambient occlusion. The final direction is the
/// local zenith; the rest form a ring at `elevation_deg` so an enclosed sample
/// loses fill from every azimuth.
fn hemisphere_directions(surface_up: DVec3, count: u32, elevation_deg: f64) -> Vec<DVec3> {
    if count == 0 {
        return Vec::new();
    }
    let up = surface_up.normalize_or_zero();
    if up.length_squared() <= 0.0 {
        return Vec::new();
    }
    let reference = if up.z.abs() < 0.9 { DVec3::Z } else { DVec3::X };
    let tangent = up.cross(reference).normalize_or_zero();
    let bitangent = up.cross(tangent);
    let ring_count = count.saturating_sub(1);
    let elevation_rad = elevation_deg.to_radians();
    let sin_elevation = elevation_rad.sin();
    let cos_elevation = elevation_rad.cos();
    let mut directions = Vec::with_capacity(count as usize);
    for index in 0..ring_count {
        let azimuth = std::f64::consts::TAU * index as f64 / ring_count as f64;
        let (sin_azimuth, cos_azimuth) = (azimuth.sin(), azimuth.cos());
        directions.push(
            (up * sin_elevation
                + tangent * (cos_elevation * cos_azimuth)
                + bitangent * (cos_elevation * sin_azimuth))
                .normalize_or_zero(),
        );
    }
    directions.push(up);
    directions
}

/// Fraction of the sky hemisphere visible from a surface point. `1` is open
/// ground, `0` is fully enclosed.
fn sky_occlusion_visibility(
    field: &PatchHeightField,
    position_m: DVec3,
    surface_up: DVec3,
    config: &TerrainOcclusionConfig,
) -> f32 {
    let directions =
        hemisphere_directions(surface_up, config.sky_directions, config.sky_elevation_deg);
    if directions.is_empty() {
        return 1.0;
    }
    let mut visibility = 0.0f32;
    for direction in &directions {
        visibility += ray_visibility(
            field,
            position_m,
            *direction,
            config.sky_max_distance_m,
            config.sky_samples,
            config.softness_m,
        );
    }
    (visibility / directions.len() as f32).clamp(0.0, 1.0)
}

/// Bake the interleaved `[self_shadow, sky_occlusion]` map for one patch and
/// return the total height-field samples consumed. The count is bounded by
/// `texture_resolution^2 * (sun_samples + sky_directions * sky_samples)`.
fn bake_terrain_occlusion(
    field: &PatchHeightField,
    sun_direction_body: DVec3,
    config: &TerrainOcclusionConfig,
) -> (Vec<f32>, usize) {
    let resolution = config.texture_resolution.max(1);
    let res = resolution as usize;
    let (u0, v0, u1, v1) = field.uv_bounds;
    let sun_direction = sun_direction_body.normalize_or_zero();
    let samples_per_texel =
        config.sun_samples as usize + config.sky_directions as usize * config.sky_samples as usize;
    let mut values = vec![1.0f32; res * res * 2];
    let mut samples = 0usize;
    for gy in 0..res {
        for gx in 0..res {
            let fu = gx as f64 / (res - 1).max(1) as f64;
            let fv = gy as f64 / (res - 1).max(1) as f64;
            let direction =
                face_uv_to_direction(field.face, u0 + (u1 - u0) * fu, v0 + (v1 - v0) * fv);
            let Some(radius_m) = field.sample_radius_m(direction) else {
                continue;
            };
            let position_m = direction * radius_m;
            samples += samples_per_texel;
            let self_shadow = if sun_direction.length_squared() > 0.0 {
                ray_visibility(
                    field,
                    position_m,
                    sun_direction,
                    config.sun_max_distance_m,
                    config.sun_samples,
                    config.softness_m,
                )
            } else {
                1.0
            };
            let sky_occlusion = sky_occlusion_visibility(field, position_m, direction, config);
            let index = (gy * res + gx) * 2;
            values[index] = self_shadow;
            values[index + 1] = sky_occlusion;
        }
    }
    (values, samples)
}

/// R8G8 image carrying the baked self-shadow (red) and sky occlusion (green),
/// sampled with linear filtering over the patch-local UV.
fn terrain_occlusion_image(resolution: u32, values: &[f32]) -> Image {
    let res = resolution.max(1) as usize;
    let mut data = vec![0u8; res * res * 2];
    for index in 0..res * res {
        let self_shadow = values
            .get(index * 2)
            .copied()
            .unwrap_or(1.0)
            .clamp(0.0, 1.0);
        let sky_occlusion = values
            .get(index * 2 + 1)
            .copied()
            .unwrap_or(1.0)
            .clamp(0.0, 1.0);
        data[index * 2] = (self_shadow * 255.0).round() as u8;
        data[index * 2 + 1] = (sky_occlusion * 255.0).round() as u8;
    }
    let mut image = Image::new(
        Extent3d {
            width: res as u32,
            height: res as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rg8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        ..Default::default()
    });
    image
}

fn neutral_occlusion_image_value() -> Image {
    Image::new(
        Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        vec![255, 255],
        TextureFormat::Rg8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    )
}

fn neutral_occlusion_image(
    render_assets: &mut TerrainRenderAssets,
    images: &mut Assets<Image>,
) -> Handle<Image> {
    render_assets
        .neutral_occlusion
        .get_or_insert_with(|| images.add(neutral_occlusion_image_value()))
        .clone()
}

/// The body-fixed unit direction from a catalog body toward the shared
/// ephemeris Sun, or `None` when the snapshot has no valid Sun state.
fn body_fixed_sun_direction(
    ephemeris_snapshot: &EphemerisSnapshot,
    body_name: &str,
    body_to_inertial: DQuat,
) -> Option<DVec3> {
    let body = NaifBodyId::for_catalog_name(body_name)?;
    let sun_position_m = ephemeris_snapshot
        .solar_inertial_relative_state(NaifBodyId::SUN, body)?
        .position_m;
    let distance_m = sun_position_m.length();
    if !distance_m.is_finite() || distance_m <= 0.0 {
        return None;
    }
    Some(body_to_inertial.conjugate() * (sun_position_m / distance_m))
}

/// Bake one patch's occlusion map, falling back to the shared neutral map when
/// the patch is too coarse or the ephemeris Sun is unavailable.
#[expect(
    clippy::too_many_arguments,
    reason = "The bake coordinates terrain, ephemeris, configuration, and Bevy assets."
)]
fn bake_terrain_occlusion_for_patch(
    geometry: &PatchGeometry,
    patch: TerrainPatch,
    patch_resolution: u32,
    body_to_inertial: DQuat,
    ephemeris_snapshot: &EphemerisSnapshot,
    body_name: &str,
    config: &TerrainOcclusionConfig,
    render_assets: &mut TerrainRenderAssets,
    images: &mut Assets<Image>,
) -> (Handle<Image>, Option<PatchHeightField>, usize, DVec3) {
    let mut neutral = || {
        (
            neutral_occlusion_image(render_assets, images),
            None,
            0,
            DVec3::ZERO,
        )
    };
    if patch.level < config.min_patch_level {
        return neutral();
    }
    let Some(field) = PatchHeightField::from_geometry(geometry, patch, patch_resolution) else {
        return neutral();
    };
    let Some(sun_direction_body) =
        body_fixed_sun_direction(ephemeris_snapshot, body_name, body_to_inertial)
    else {
        return neutral();
    };
    let (values, samples) = bake_terrain_occlusion(&field, sun_direction_body, config);
    let handle = images.add(terrain_occlusion_image(config.texture_resolution, &values));
    (
        handle,
        Some(field),
        samples,
        body_to_inertial * sun_direction_body,
    )
}

/// Refresh resident patches whose recorded inertial Sun direction has moved
/// beyond the configured tolerance. Body rotation alone never triggers a bake,
/// because the recorded direction is inertial; only a genuine ephemeris change
/// (or patch regeneration, which bakes at spawn) does.
fn refresh_terrain_occlusion(
    ephemeris_snapshot: Res<EphemerisSnapshot>,
    occlusion_config: Res<TerrainOcclusionConfig>,
    planet_query: Query<&PlanetComponent>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<TerrainMaterial>>,
    mut query: Query<&mut TerrainPatchRenderState>,
) {
    if occlusion_config.max_refreshes_per_frame == 0 {
        return;
    }
    let tolerance_cos = occlusion_config.refresh_tolerance_rad.cos();
    let mut refreshed = 0u32;
    for mut state in &mut query {
        if refreshed >= occlusion_config.max_refreshes_per_frame {
            break;
        }
        if !state.occlusion_owned {
            continue;
        }
        let Some(field) = state.occlusion_field.as_ref() else {
            continue;
        };
        let Ok(planet) = planet_query.get(state.planet_entity) else {
            continue;
        };
        let Some(orientation) =
            ephemeris_snapshot.orientation_for_catalog_body(&planet.domain_planet.name)
        else {
            continue;
        };
        let body_to_inertial = body_fixed_to_planet_inertial_rotation(orientation);
        let Some(sun_direction_body) = body_fixed_sun_direction(
            &ephemeris_snapshot,
            &planet.domain_planet.name,
            body_to_inertial,
        ) else {
            continue;
        };
        let sun_inertial = body_to_inertial * sun_direction_body;
        if state.baked_sun_inertial.length_squared() > 0.0
            && state.baked_sun_inertial.dot(sun_inertial) >= tolerance_cos
        {
            continue;
        }
        let field = field.clone();
        let (values, _) = bake_terrain_occlusion(&field, sun_direction_body, &occlusion_config);
        let new_handle = images.add(terrain_occlusion_image(
            occlusion_config.texture_resolution,
            &values,
        ));
        if let Some(material) = materials.get_mut(&state.material_handle) {
            material.extension.terrain_occlusion = new_handle.clone();
        }
        let previous = std::mem::replace(&mut state.occlusion_texture, new_handle);
        images.remove(previous.id());
        state.baked_sun_inertial = sun_inertial;
        refreshed += 1;
    }
}

#[cfg(test)]
mod tests {
    use self::uploads::TerrainUploadEnqueueResult;
    use super::*;
    use crate::domain::services::cube_sphere::{build_patch_geometry, CubeFace};
    use crate::domain::services::planet_factory::PlanetFactory;
    use crate::domain::services::reference_frames::catalog_body_fixed_to_inertial_rotation;
    use crate::domain::services::simulation_time::SimulationTime;
    use crate::domain::services::terrain_source::{ProceduralTerrainSource, TerrainSource};
    use bevy::ecs::message::Messages;
    use std::collections::BTreeSet;

    /// Build a body-fixed `PatchHeightField` directly from a local height law,
    /// so the ray-march can be tested against known ridges and bowls without a
    /// full cube-sphere geometry build.
    fn synthetic_height_field(
        uv_bounds: (f64, f64, f64, f64),
        resolution: usize,
        planet_radius_m: f64,
        height_fn: impl Fn(f64, f64) -> f64,
    ) -> PatchHeightField {
        let mut radii_m = Vec::with_capacity(resolution * resolution);
        for row in 0..resolution {
            for column in 0..resolution {
                let fu = column as f64 / (resolution - 1) as f64;
                let fv = row as f64 / (resolution - 1) as f64;
                radii_m.push((planet_radius_m + height_fn(fu, fv)) as f32);
            }
        }
        PatchHeightField {
            face: CubeFace::PosZ,
            resolution,
            uv_bounds,
            radii_m,
        }
    }

    /// Body-fixed direction through a patch-local UV coordinate.
    fn local_uv_direction(uv_bounds: (f64, f64, f64, f64), fu: f64, fv: f64) -> DVec3 {
        let (u0, v0, u1, v1) = uv_bounds;
        face_uv_to_direction(CubeFace::PosZ, u0 + (u1 - u0) * fu, v0 + (v1 - v0) * fv)
    }

    fn small_occlusion_config() -> TerrainOcclusionConfig {
        TerrainOcclusionConfig {
            texture_resolution: 8,
            sun_samples: 4,
            sky_directions: 3,
            sky_samples: 2,
            ..Default::default()
        }
    }

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
            None,
        );
        let fine = crate::infrastructure::bevy_adapters::terrain::surface::prepare_patch_surface(
            &source,
            &TerrainPatch::for_direction(DVec3::Z, 12),
            &geometry,
            6_371_000.0,
            None,
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
                    layer: LayerMaterialState::default(),
                    vegetation_mesh_handle: None,
                    water_mesh_handle: None,
                    water_material_handle: None,
                    river_mesh_handle: None,
                    planet_entity,
                    body_to_inertial_at_spawn: DQuat::IDENTITY,
                    render_origin_at_spawn: DVec3::ZERO,
                    occlusion_texture: Handle::default(),
                    occlusion_owned: false,
                    occlusion_field: None,
                    baked_sun_inertial: DVec3::ZERO,
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
                    layer: LayerMaterialState::default(),
                    vegetation_mesh_handle: None,
                    water_mesh_handle: None,
                    water_material_handle: None,
                    river_mesh_handle: None,
                    planet_entity: other_planet_entity,
                    body_to_inertial_at_spawn: DQuat::IDENTITY,
                    render_origin_at_spawn: DVec3::ZERO,
                    occlusion_texture: Handle::default(),
                    occlusion_owned: false,
                    occlusion_field: None,
                    baked_sun_inertial: DVec3::ZERO,
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
        let mut water_materials = Assets::<WaterMaterial>::default();
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
        let water_material_handle = water_materials.add(WaterMaterial {
            base: water_base_material(false),
            extension: WaterExtension::new(WaterParams::default(), Handle::default()),
        });
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
            layer: LayerMaterialState::default(),
            vegetation_mesh_handle: Some(vegetation_mesh_handle.clone()),
            water_mesh_handle: None,
            water_material_handle: Some(water_material_handle.clone()),
            river_mesh_handle: None,
            planet_entity: Entity::PLACEHOLDER,
            body_to_inertial_at_spawn: DQuat::IDENTITY,
            render_origin_at_spawn: DVec3::ZERO,
            occlusion_texture: Handle::default(),
            occlusion_owned: false,
            occlusion_field: None,
            baked_sun_inertial: DVec3::ZERO,
        };

        release_patch_render_assets(
            &state,
            &mut meshes,
            &mut materials,
            &mut water_materials,
            &mut images,
        );

        assert!(meshes.get(mesh_handle.id()).is_none());
        assert!(meshes.get(vegetation_mesh_handle.id()).is_none());
        assert!(materials.get(material_handle.id()).is_none());
        assert!(
            water_materials.get(water_material_handle.id()).is_none(),
            "a per-patch water material must be released with its patch"
        );
        assert!(vegetation_materials
            .get(shared_vegetation_material.id())
            .is_some());
    }

    #[test]
    fn refractive_water_base_uses_transmission_and_depth_fallback_does_not() {
        let depth_path = water_base_material(false);
        assert_eq!(depth_path.specular_transmission, 0.0);
        assert_eq!(depth_path.thickness, 0.0);

        let refractive = water_base_material(true);
        assert!(refractive.specular_transmission > 0.0);
        assert!(refractive.thickness > 0.0);
        assert_eq!(refractive.ior, 1.33);
    }

    #[test]
    fn flat_terrain_is_unoccluded_and_a_ridge_blocks_the_sun_ray() {
        let radius_m = 6_371_000.0;
        let bounds = (0.5, 0.5, 0.51, 0.51);
        let origin_direction = local_uv_direction(bounds, 0.2, 0.5);
        let origin_m = origin_direction * radius_m;

        let flat = synthetic_height_field(bounds, 33, radius_m, |_, _| 0.0);
        assert_eq!(
            ray_visibility(&flat, origin_m, origin_direction, 20_000.0, 16, 80.0),
            1.0,
            "flat ground cannot occlude a ray rising toward the Sun"
        );

        let ridge = synthetic_height_field(bounds, 33, radius_m, |fu, _| {
            if (0.35..=0.65).contains(&fu) {
                500.0
            } else {
                0.0
            }
        });
        // Aim across the patch toward increasing local u, where the ridge sits.
        let toward_ridge = (local_uv_direction(bounds, 0.21, 0.5) - origin_direction).normalize();
        assert_eq!(
            ray_visibility(&ridge, origin_m, toward_ridge, 20_000.0, 64, 80.0),
            0.0,
            "a tall ridge must fully shadow the ground behind it"
        );
        assert_eq!(
            ray_visibility(&ridge, origin_m, -toward_ridge, 20_000.0, 64, 80.0),
            1.0,
            "the opposite direction is unobstructed"
        );
    }

    #[test]
    fn grazing_sun_ray_produces_a_partial_soft_shadow() {
        let radius_m = 6_371_000.0;
        let bounds = (0.5, 0.5, 0.51, 0.51);
        let origin_direction = local_uv_direction(bounds, 0.2, 0.5);
        let origin_m = origin_direction * radius_m;
        // Just tall enough to clip the curved tangent ray without fully burying
        // it: the deepest penetration stays inside the softness band.
        let ridge = synthetic_height_field(bounds, 33, radius_m, |fu, _| {
            if (0.35..=0.65).contains(&fu) {
                40.0
            } else {
                0.0
            }
        });
        let toward_ridge = (local_uv_direction(bounds, 0.21, 0.5) - origin_direction).normalize();

        let visibility = ray_visibility(&ridge, origin_m, toward_ridge, 20_000.0, 64, 80.0);
        assert!(
            visibility > 0.0 && visibility < 1.0,
            "a grazing ray must produce a partial self-shadow, got {visibility}"
        );
    }

    #[test]
    fn crevice_occludes_more_sky_than_an_open_slope() {
        let radius_m = 6_371_000.0;
        let bounds = (0.5, 0.5, 0.51, 0.51);
        // A bowl inside a patch, with the ring sampled near the horizon so the
        // rim blocks the low sky directions.
        let pit = synthetic_height_field(bounds, 33, radius_m, |fu, fv| {
            if (fu - 0.5).abs() < 0.15 && (fv - 0.5).abs() < 0.15 {
                -200.0
            } else {
                300.0
            }
        });
        let config = TerrainOcclusionConfig {
            sky_max_distance_m: 20_000.0,
            sky_elevation_deg: 1.0,
            sky_samples: 4,
            ..Default::default()
        };

        let pit_direction = local_uv_direction(bounds, 0.5, 0.5);
        let pit_position_m = pit_direction * (radius_m - 200.0);
        let pit_visibility = sky_occlusion_visibility(&pit, pit_position_m, pit_direction, &config);

        let open_direction = local_uv_direction(bounds, 0.9, 0.9);
        let open_position_m = open_direction * (radius_m + 300.0);
        let open_visibility =
            sky_occlusion_visibility(&pit, open_position_m, open_direction, &config);

        assert!(
            pit_visibility < open_visibility,
            "an enclosed sample must see less sky: pit {pit_visibility} vs open {open_visibility}"
        );
        assert!(
            open_visibility > 0.9,
            "open ground must keep almost all sky fill, got {open_visibility}"
        );
    }

    #[test]
    fn occlusion_bake_is_deterministic_for_identical_inputs() {
        let field = synthetic_height_field((0.5, 0.5, 0.51, 0.51), 33, 6_371_000.0, |fu, fv| {
            fu * 400.0 - fv * 150.0
        });
        let sun_direction = DVec3::new(0.4, 0.5, 0.77).normalize();
        let config = small_occlusion_config();

        let (first, first_samples) = bake_terrain_occlusion(&field, sun_direction, &config);
        let (second, second_samples) = bake_terrain_occlusion(&field, sun_direction, &config);

        assert_eq!(
            first, second,
            "identical inputs must reproduce identical texels"
        );
        assert_eq!(first_samples, second_samples);
        assert!(
            first.iter().all(|value| (0.0..=1.0).contains(value)),
            "occlusion terms must stay normalized"
        );
    }

    #[test]
    fn occlusion_bake_cannot_modify_the_terrain_source() {
        let source = ProceduralTerrainSource::new(7, 1_500.0, 400.0, 0);
        let patch = TerrainPatch::for_direction(DVec3::Z, 12);
        let geometry = build_patch_geometry(&patch, &source, 6_371_000.0, 17, 5.0);
        let before = source.height_m(12.0, 34.0);

        let field = PatchHeightField::from_geometry(&geometry, patch, 17)
            .expect("a 17x17 patch provides a height field");
        let _ = bake_terrain_occlusion(&field, DVec3::X, &small_occlusion_config());

        assert_eq!(
            source.height_m(12.0, 34.0),
            before,
            "the bake reads an immutable height field and writes no simulation state"
        );
    }

    /// Rust mirror of the shader's direct/indirect occlusion split. Kept here so
    /// the visibility contract is provable without a GPU.
    fn compose_terrain_lighting(
        direct: f32,
        indirect: f32,
        self_shadow: f32,
        sky_occlusion: f32,
    ) -> f32 {
        direct * self_shadow + indirect * sky_occlusion
    }

    #[test]
    fn self_shadow_scales_direct_only_and_occlusion_scales_indirect_only() {
        let direct = 1.0f32;
        let indirect = 0.25f32;

        assert_eq!(
            compose_terrain_lighting(direct, indirect, 0.0, 1.0),
            indirect,
            "fully self-shadowed terrain with non-zero ambient keeps its sky fill"
        );
        assert_eq!(
            compose_terrain_lighting(direct, indirect, 1.0, 0.0),
            direct,
            "sky occlusion must not attenuate the direct-sun term"
        );
        assert_eq!(
            compose_terrain_lighting(direct, indirect, 0.5, 0.5),
            0.625,
            "the two terms compose independently"
        );
    }
}
