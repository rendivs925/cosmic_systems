use crate::domain::entities::planet::Planet;
use crate::domain::value_objects::solar_system_params::SolarSystemParameters;
use bevy::prelude::*;
use super::performance_components::QualityLevel;

// Compute backend abstraction for hybrid GPU+CPU processing
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ComputeBackendType {
    VulkanGpu,
    CpuSimd,
    Hybrid,
}

// Physics workload classification for optimal backend routing
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

// Result from compute backend processing
#[derive(Clone, Debug)]
pub struct PhysicsResult {
    pub entity_positions: Vec<(Entity, Vec3)>,
    pub processing_time_ms: f32,
    pub backend_used: ComputeBackendType,
    pub success: bool,
}

// Backend capabilities for different compute backends
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

// Unified compute backend for Kepler equation solving
#[derive(Resource)]
pub struct ComputeBackend {
    #[cfg(not(target_arch = "wasm32"))]
    pub vulkan_solver: Option<crate::infrastructure::gpu_compute::vulkan_kepler::VulkanKeplerSolver>,
    #[cfg(target_arch = "wasm32")]
    pub vulkan_solver: Option<()>,
    pub fallback_solver: crate::infrastructure::bevy_adapters::simd_kepler::SimdKeplerSolver,
    pub vulkan_available: bool,
}

impl Default for ComputeBackend {
    fn default() -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        {
            Self {
                vulkan_solver: None,
                fallback_solver: crate::infrastructure::bevy_adapters::simd_kepler::SimdKeplerSolver::new(),
                vulkan_available: false,
            }
        }

        #[cfg(target_arch = "wasm32")]
        {
            Self {
                vulkan_solver: None,
                fallback_solver: crate::infrastructure::bevy_adapters::simd_kepler::SimdKeplerSolver::new(),
                vulkan_available: false,
            }
        }
    }
}

impl ComputeBackend {
    /// Initialize compute backends with hardware detection
    pub fn new() -> Self {
        // Vulkan initialization would go here - for now not available
        Self {
            #[cfg(not(target_arch = "wasm32"))]
            vulkan_solver: None,
            #[cfg(target_arch = "wasm32")]
            vulkan_solver: None,
            fallback_solver: crate::infrastructure::bevy_adapters::simd_kepler::SimdKeplerSolver::new(),
            vulkan_available: false,
        }
    }

    pub fn solve_kepler(&mut self, planets: &[Planet], quality: QualityLevel) -> Vec<Vec3> {
        // For now, only SIMD CPU solver is available
        // Vulkan and WebGPU solvers would be added when implemented
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

/// Hybrid compute function for intelligent routing between Vulkan GPU and CPU SIMD
pub fn process_hybrid_compute(
    planets: &[Planet],
    quality: QualityLevel,
    time_days: f32,
    scale_factor: f32,
    vulkan_available: bool,
    vulkan_solver: &mut Option<crate::infrastructure::gpu_compute::vulkan_kepler::VulkanKeplerSolver>,
    simd_solver: &mut crate::infrastructure::bevy_adapters::simd_kepler::SimdKeplerSolver,
) -> (Vec<Vec3>, ComputeBackendType) {
    // Simple routing logic: use Vulkan for planets if available, SIMD for everything else
    if vulkan_available && !planets.is_empty() {
        // Try Vulkan first
        if let Some(vulkan) = vulkan_solver {
            match vulkan.solve_batch(planets, quality, time_days, scale_factor) {
                Ok(positions) => {
                    return (positions, ComputeBackendType::VulkanGpu);
                },
                Err(e) => {
                    // Vulkan failed, fall back to SIMD
                    println!("❌ Vulkan GPU compute failed: {}", e);
                }
            }
        } else {
            println!("⚠️ Vulkan solver not available despite vulkan_enabled=true");
        }
    }

    // Fallback to SIMD
    let positions = simd_solver.solve_batch(planets, quality);
    (positions, ComputeBackendType::CpuSimd)
}

#[derive(Resource)]
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