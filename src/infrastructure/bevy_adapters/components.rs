use crate::domain::entities::gyroscope::Gyroscope;
use crate::domain::entities::planet::Planet;
use crate::domain::value_objects::solar_system_params::SolarSystemParameters;
use bevy::prelude::*;
#[cfg(target_arch = "wasm32")]
use std::cell::RefCell;
#[cfg(target_arch = "wasm32")]
use std::rc::Rc;
use std::collections::{HashMap, VecDeque};
use async_trait::async_trait;



#[cfg(not(target_arch = "wasm32"))]
use crate::infrastructure::gpu_compute::vulkan_kepler::VulkanKeplerSolver;

use crate::infrastructure::bevy_adapters::simd_kepler::SimdKeplerSolver;
#[cfg(target_arch = "wasm32")]
use crate::infrastructure::gpu_compute::webgpu_kepler::WebGpuKeplerSolver;

// Component for gyroscope entities
#[derive(Component)]
pub struct GyroscopeComponent {
    pub domain_gyro: Gyroscope,
}

// Component for thrust visualization (arrow entity)
#[derive(Component)]
pub struct ThrustArrow;

// Component for planet entities
#[derive(Component)]
pub struct PlanetComponent {
    pub domain_planet: Planet,
    pub material: Handle<StandardMaterial>,
    pub has_texture: bool,
    pub base_reflectance: f32,
    pub base_roughness: f32,
}

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

#[derive(Component)]
pub struct PendingOrbitMesh {
    pub mesh: Handle<Mesh>,
    pub orbit_shape: crate::domain::services::physics::OrbitShape,
    pub color: Color,
    pub segments: usize,
}

// Component for orbital path visualization
#[derive(Component)]
pub struct OrbitComponent {
    pub radius: f32,
    pub planet_entity: Entity,
    pub material: Handle<StandardMaterial>,
    pub base_color: Color,
    pub tilt: Vec2,
    pub wobble_speed: f32,
    pub wobble_amount: f32,
    pub spin_speed: f32,
    pub phase: f32,
}

// Marker component for moon orbits (orbits that need to follow their parent planet)
#[derive(Component)]
pub struct MoonOrbit;

// Component for cloud layers to control rotation speed
#[derive(Component)]
pub struct CloudLayer {
    pub rotation_period_hours: f32,
}

// Camera control modes
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CameraMode {
    FreeFlight,     // Free movement in 3D space
    Orbit,          // Orbital view around solar system center
    FollowPlanet,   // Follow a specific planet
    ApproachPlanet, // Approach and potentially "land" on a planet
}

// Component for camera controller
#[derive(Component)]
pub struct CameraController {
    pub mode: CameraMode,
    pub speed: f32,
    pub sensitivity: f32,
    pub velocity: Vec3,
    pub target_entity: Option<Entity>,
    pub orbit_distance: f32,
    pub orbit_angle: f32,
    pub acceleration: f32,            // Smooth acceleration
    pub deceleration: f32,            // Smooth deceleration
    pub adaptive_speed_enabled: bool, // Auto-adjust speed based on distance
    pub min_speed: f32,               // Minimum movement speed
    pub max_speed: f32,               // Maximum movement speed
    pub zoom_sensitivity: f32,        // Mouse wheel zoom sensitivity
}

// Component for selectable objects (planets, etc.)
#[derive(Component)]
pub struct Selectable {
    pub name: String,
    pub selected: bool,
}

// Resource to track currently selected planet
#[derive(Resource)]
pub struct SelectedPlanet {
    pub entity: Option<Entity>,
    pub name: Option<String>,
}

// Resource to track hovered planet for information display
#[derive(Resource)]
pub struct HoveredPlanet {
    pub name: Option<String>,
    pub info: Option<String>,
}



// Notification types for user feedback
#[derive(Clone, Debug)]
pub enum NotificationType {
    Success,
    Error,
    Info,
}

// Individual notification message
#[derive(Clone, Debug)]
pub struct Notification {
    pub message: String,
    pub notification_type: NotificationType,
    pub created_at: f32, // Time in seconds
    pub duration: f32,   // How long to display (seconds)
}

// Resource to manage notification queue
#[derive(Resource)]
pub struct NotificationQueue {
    pub notifications: Vec<Notification>,
    pub hide_for_screenshot: bool, // Temporarily hide notifications during screenshot
}

// Resource to track pending screenshot capture
#[derive(Resource)]
pub struct ScreenshotState {
    pub pending: bool, // Screenshot requested, will capture next frame
}

// Resource to track if UI is currently under the cursor
#[derive(Resource, Default)]
pub struct UiPointerState {
    pub is_over_ui: bool,
}

#[derive(Resource)]
pub struct CameraInputState {
    pub last_input_time: f32,
    pub suppress_auto_inspect_for: Option<Entity>,
    pub last_selected_entity: Option<Entity>,
}

impl Default for CameraInputState {
    fn default() -> Self {
        Self {
            last_input_time: -1000.0,
            suppress_auto_inspect_for: None,
            last_selected_entity: None,
        }
    }
}

#[derive(Resource)]
pub struct DynamicResolutionState {
    pub scale: f32,
    pub min_scale: f32,
    pub max_scale: f32,
    pub cooldown: f32,
}

impl Default for DynamicResolutionState {
    fn default() -> Self {
        Self {
            scale: 1.0,
            min_scale: 0.6,
            max_scale: 1.0,
            cooldown: 0.0,
        }
    }
}

// Performance monitoring and quality adjustment
#[derive(Resource)]
pub struct PerformanceStats {
    // PRODUCTION-GRADE FPS MEASUREMENT (per industry standards)
    // Frame time is the truth - FPS is derived
    pub frame_time_ms: f32,          // Current frame time in milliseconds (PRIMARY METRIC)
    pub fps_raw: f32,                // Raw FPS = 1/frame_time (jumps violently, not for display)
    pub fps_smoothed: f32,           // Exponential moving average FPS (stable, responsive)
    pub fps_display: f32,            // FPS to show users (smoothed, human-friendly)

    // Frame time statistics (most important for performance analysis)
    pub frame_time_smoothed: f32,    // EMA frame time in ms (stable metric)
    pub frame_time_99th: f32,        // 99th percentile frame time (stutter detection)
    pub frame_time_min: f32,         // Minimum frame time this session
    pub frame_time_max: f32,         // Maximum frame time this session

    // GPU timing (when available - Vulkan/WebGPU)
    pub gpu_frame_time_ms: f32,      // GPU execution time (if measurable)
    pub cpu_gpu_frame_time: f32,     // Max(CPU, GPU) time = true frame rate limiter

    // High-precision timing infrastructure
    pub last_frame_time: std::time::Instant, // Monotonic high-res clock timestamp
    pub frame_time_history: Vec<f32>, // Raw frame times for percentile calculation
    pub history_capacity: usize,      // Frame history size (default 1000 for 99th percentile)

    // LEGACY FIELDS (deprecated - use new fields above)
    pub frame_time: f32,             // DEPRECATED: Use frame_time_ms
    pub fps: f32,                    // DEPRECATED: Use fps_display
    pub average_frame_time: f32,     // DEPRECATED: Use frame_time_smoothed
    pub average_fps: f32,            // DEPRECATED: Use fps_smoothed

    // Session and configuration
    pub frame_count: u64,            // Total frames rendered
    pub quality_level: QualityLevel, // Current quality setting
    pub target_fps: f32,             // Target FPS for quality adjustment
    pub adaptive_enabled: bool,      // Whether automatic quality adjustment is enabled
    pub frame_history: VecDeque<f32>, // DEPRECATED: Use frame_time_history
    pub history_len: usize,          // DEPRECATED: Use history_capacity
    pub adaptation_rate: f32,

    // Detailed optimization timing (for benchmarking)
    pub kepler_solve_time: f32,      // Time spent solving Kepler equations (ms)
    pub physics_update_time: f32,    // Total physics update time (ms)
    pub rendering_time: f32,         // Rendering time (ms)
    pub material_update_time: f32,   // Material property updates (ms)
    pub orbit_visual_time: f32,      // Orbit visualization updates (ms)
    pub ui_update_time: f32,         // UI update time (ms)

    // Optimization-specific metrics
    pub adaptive_kepler_calls: u64,  // Number of adaptive Kepler calls
    pub full_precision_kepler: u64,  // Full precision (8 iterations)
    pub half_precision_kepler: u64,  // Half precision (4 iterations)
    pub quarter_precision_kepler: u64, // Quarter precision (2 iterations)
    pub minimal_precision_kepler: u64, // Minimal precision (1 iteration)
    pub vulkan_kepler_calls: u64,     // Number of Vulkan Kepler calls

    // SIMD and parallel processing metrics
    pub simd_enabled: bool,          // Whether SIMD is active
    pub parallel_enabled: bool,      // Whether parallel processing is active
    pub cpu_cores_used: usize,       // Number of CPU cores utilized
    pub vector_width: usize,         // SIMD vector width (128, 256, 512 bits)

    // Memory usage
    pub memory_usage_mb: f32,        // Current memory usage in MB
    pub peak_memory_mb: f32,         // Peak memory usage in MB

    // GPU acceleration
    pub vulkan_enabled: bool,        // Whether Vulkan compute is available and active
    pub vulkan_initialized: bool,    // Whether Vulkan initialization has been attempted
    #[cfg(not(target_arch = "wasm32"))]
    pub vulkan_solver: Option<crate::infrastructure::gpu_compute::vulkan_kepler::VulkanKeplerSolver>,
    #[cfg(target_arch = "wasm32")]
    pub vulkan_solver: Option<()>,

    // Benchmark timing accumulators
    pub benchmark_start_time: Option<std::time::Instant>,
    pub benchmark_frame_count: u64,
    pub benchmark_total_time: f32,
}

#[derive(Resource, Clone, Copy, Debug)]
pub struct ChromeOptimizations {
    pub is_chrome: bool,
    pub webgpu_supported: bool,
    pub webgpu_enabled: bool,
    pub worker_target: usize,
}

impl Default for ChromeOptimizations {
    fn default() -> Self {
        Self {
            is_chrome: false,
            webgpu_supported: false,
            webgpu_enabled: false,
            worker_target: 2,
        }
    }
}

/// Compute backend abstraction for hybrid GPU+CPU processing
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ComputeBackendType {
    VulkanGpu,
    CpuSimd,
    Hybrid,
}

/// Physics workload classification for optimal backend routing
#[derive(Clone, Debug)]
pub struct PhysicsWorkload {
    pub bodies: Vec<Planet>,
    pub time_days: f32,
    pub solar_params: SolarSystemParameters,
    pub camera_pos: Vec3,
    pub quality_level: QualityLevel,
    pub workload_type: WorkloadType,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WorkloadType {
    PlanetsOnly,
    MoonsOnly,
    Mixed,
    SingleBody,
    LargeBatch,
}

/// Result from compute backend processing
#[derive(Clone, Debug)]
pub struct PhysicsResult {
    pub entity_positions: Vec<(Entity, Vec3)>,
    pub processing_time_ms: f32,
    pub backend_used: ComputeBackendType,
    pub success: bool,
}

/// Hybrid compute function for intelligent routing between Vulkan GPU and CPU SIMD
pub fn process_hybrid_compute(
    planets: &[Planet],
    quality: QualityLevel,
    vulkan_available: bool,
    vulkan_solver: &mut Option<crate::infrastructure::gpu_compute::vulkan_kepler::VulkanKeplerSolver>,
    simd_solver: &mut crate::infrastructure::bevy_adapters::simd_kepler::SimdKeplerSolver,
) -> (Vec<Vec3>, ComputeBackendType) {
    // Simple routing logic: use Vulkan for planets if available, SIMD for everything else
    println!("🔀 Hybrid compute: vulkan_available={}, planets_count={}", vulkan_available, planets.len());
    if vulkan_available && !planets.is_empty() {
        println!("🎯 Trying Vulkan GPU compute for {} planets", planets.len());
        // Try Vulkan first
        if let Some(vulkan) = vulkan_solver {
            println!("🚀 Vulkan solver available, calling solve_batch...");
            match vulkan.solve_batch(planets, quality) {
                Ok(positions) => {
                    println!("✅ Vulkan GPU compute succeeded!");
                    return (positions, ComputeBackendType::VulkanGpu);
                },
                Err(e) => {
                    println!("❌ Vulkan GPU compute failed: {}", e);
                    // Vulkan failed, fall back to SIMD
                }
            }
        } else {
            println!("⚠️ Vulkan solver not available despite vulkan_enabled=true");
        }
    }

    // Use SIMD as fallback or primary backend
    let positions = simd_solver.solve_batch(planets, quality);
    (positions, ComputeBackendType::CpuSimd)
}

#[derive(Clone, Debug)]
pub struct BackendCapabilities {
    pub max_batch_size: usize,
    pub optimal_batch_size: usize,
    pub supports_complex_orbits: bool,
    pub memory_mb: usize,
    pub concurrent_workloads: usize,
}

#[derive(Clone, Debug)]
pub enum ComputeError {
    BackendUnavailable,
    MemoryError(String),
    ProcessingError(String),
    Timeout,
    UnsupportedWorkload,
}



// Quality levels for automatic adjustment
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum QualityLevel {
    Ultra,   // Highest quality, no performance optimizations
    High,    // High quality with minimal optimizations
    Medium,  // Balanced quality and performance
    Low,     // Lower quality for better performance
    Minimal, // Minimum quality for maximum performance
}

impl Default for PerformanceStats {
    fn default() -> Self {
        Self {
            // Production-grade FPS measurement (primary metrics)
            frame_time_ms: 16.67,        // Assume 60 FPS initially
            fps_raw: 60.0,               // Raw FPS
            fps_smoothed: 60.0,          // EMA smoothed FPS
            fps_display: 60.0,           // Display FPS (smoothed)

            // Frame time statistics
            frame_time_smoothed: 16.67,  // EMA frame time
            frame_time_99th: 16.67,      // 99th percentile (initially same)
            frame_time_min: 16.67,       // Min frame time
            frame_time_max: 16.67,       // Max frame time

            // GPU timing (initially same as CPU)
            gpu_frame_time_ms: 16.67,    // GPU time (if available)
            cpu_gpu_frame_time: 16.67,   // Max of CPU/GPU time

            // High-precision timing
            last_frame_time: std::time::Instant::now(),
            frame_time_history: Vec::with_capacity(1000),
            history_capacity: 1000,      // Keep 1000 samples for 99th percentile

            // LEGACY FIELDS (maintained for compatibility)
            frame_time: 16.67,           // DEPRECATED
            fps: 60.0,                   // DEPRECATED
            average_frame_time: 16.67,   // DEPRECATED
            average_fps: 60.0,           // DEPRECATED

            // Session configuration
            frame_count: 0,
            quality_level: QualityLevel::High,
            target_fps: 60.0,
            adaptive_enabled: true,
            frame_history: VecDeque::with_capacity(60), // DEPRECATED
            history_len: 60,             // DEPRECATED
            adaptation_rate: 0.1,

            // Detailed optimization timing
            kepler_solve_time: 0.0,
            physics_update_time: 0.0,
            rendering_time: 0.0,
            material_update_time: 0.0,
            orbit_visual_time: 0.0,
            ui_update_time: 0.0,

            // Optimization-specific metrics
            adaptive_kepler_calls: 0,
            full_precision_kepler: 0,
            half_precision_kepler: 0,
            quarter_precision_kepler: 0,
            minimal_precision_kepler: 0,
            vulkan_kepler_calls: 0,

            // SIMD and parallel processing metrics
            simd_enabled: false,
            parallel_enabled: false,
            cpu_cores_used: 1,
            vector_width: 128,

            // Memory usage
            memory_usage_mb: 0.0,
            peak_memory_mb: 0.0,

            // GPU acceleration
            vulkan_enabled: false,
            vulkan_initialized: false,
            #[cfg(not(target_arch = "wasm32"))]
            vulkan_solver: None,
            #[cfg(target_arch = "wasm32")]
            vulkan_solver: None,

            // Benchmark timing
            benchmark_start_time: None,
            benchmark_frame_count: 0,
            benchmark_total_time: 0.0,
        }
    }
}

#[cfg(target_arch = "wasm32")]
#[derive(Resource, Default)]
pub struct WasmMemoryStats {
    pub used_heap_bytes: u64,
    pub heap_limit_bytes: u64,
    pub utilization: f32,
}

#[cfg(target_arch = "wasm32")]
#[derive(Default)]
pub struct WebGpuKeplerState {
    pub solver: Rc<RefCell<Option<WebGpuKeplerSolver>>>,
    pub initializing: Rc<RefCell<bool>>,
    pub in_flight: Rc<RefCell<bool>>,
    pub results: Rc<RefCell<Vec<(Entity, Vec3)>>>,
}

// Quality controller for gradual performance adaptation
#[derive(Resource)]
pub struct QualityController {
    pub current_level: QualityLevel,
    pub min_fps: f32,
    pub frame_times: VecDeque<f32>,
    pub adaptation_rate: f32, // Gradual 10% changes
}

impl Default for QualityController {
    fn default() -> Self {
        Self {
            current_level: QualityLevel::High,
            min_fps: 60.0,
            frame_times: VecDeque::with_capacity(60), // 1 second at 60 FPS
            adaptation_rate: 0.1,
        }
    }
}

impl QualityController {
    pub fn adapt_quality(&mut self, current_fps: f32) {
        if current_fps < self.min_fps {
            self.decrease_quality();
        } else if current_fps > self.min_fps * 1.2 {
            self.increase_quality();
        }
    }

    fn decrease_quality(&mut self) {
        // Gradual parameter reduction
        match self.current_level {
            QualityLevel::Ultra => self.current_level = QualityLevel::High,
            QualityLevel::High => self.current_level = QualityLevel::Medium,
            QualityLevel::Medium => self.current_level = QualityLevel::Low,
            QualityLevel::Low => self.current_level = QualityLevel::Minimal,
            QualityLevel::Minimal => {} // Can't go lower
        }
    }

    fn increase_quality(&mut self) {
        // Gradual parameter increase
        match self.current_level {
            QualityLevel::Ultra => {} // Already at maximum
            QualityLevel::High => self.current_level = QualityLevel::Ultra,
            QualityLevel::Medium => self.current_level = QualityLevel::High,
            QualityLevel::Low => self.current_level = QualityLevel::Medium,
            QualityLevel::Minimal => self.current_level = QualityLevel::Low,
        }
    }
}

// Unified compute backend for Kepler equation solving
#[derive(Resource)]
pub struct ComputeBackend {
    #[cfg(not(target_arch = "wasm32"))]
    pub vulkan_solver: Option<VulkanKeplerSolver>,
    #[cfg(target_arch = "wasm32")]
    pub vulkan_solver: Option<()>,
    pub fallback_solver: SimdKeplerSolver,
    pub vulkan_available: bool,
}

impl Default for ComputeBackend {
    fn default() -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        {
            Self {
                vulkan_solver: None,
                fallback_solver: SimdKeplerSolver::new(),
                vulkan_available: false,
            }
        }

        #[cfg(target_arch = "wasm32")]
        {
            Self {
                vulkan_solver: None,
                fallback_solver: SimdKeplerSolver::new(),
                vulkan_available: false,
            }
        }
    }
}

impl ComputeBackend {
    /// Initialize compute backends with hardware detection
    pub fn new() -> Self {
        let mut backend = Self::default();
        // Vulkan initialization would go here - for now not available
        backend.vulkan_available = false;
        backend
    }

    pub fn solve_kepler(&mut self, planets: &[Planet], quality: QualityLevel) -> Vec<Vec3> {
        // For now, only SIMD CPU solver is available
        // Vulkan and WebGPU solvers would be added here when implemented
        self.fallback_solver.solve_batch(planets, quality)
    }

    /// Get information about available compute backends
    pub fn get_backend_info(&self) -> String {
        let mut info = String::new();

        if self.vulkan_available {
            info.push_str("Vulkan: Available\n");
        } else {
            info.push_str("Vulkan: Not Available\n");
        }

        info.push_str("SIMD CPU: Always Available\n");
        info
    }
}
