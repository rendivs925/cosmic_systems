#[cfg(target_arch = "wasm32")]
pub fn init_webgpu_solver(
    chrome: Option<Res<ChromeOptimizations>>,
    mut state: NonSendMut<WebGpuKeplerState>,
) {
    if !chrome.as_ref().is_some_and(|chrome| chrome.webgpu_enabled) {
        return;
    }

    if state.solver.borrow().is_some() || *state.initializing.borrow() {
        return;
    }

    *state.initializing.borrow_mut() = true;
    let solver_ref = state.solver.clone();
    let init_flag = state.initializing.clone();
    spawn_local(async move {
        if let Some(solver) = WebGpuKeplerSolver::new_chrome_optimized().await {
            *solver_ref.borrow_mut() = Some(solver);
        }
        *init_flag.borrow_mut() = false;
    });
}

#[cfg(target_arch = "wasm32")]
pub fn update_wasm_memory_stats(
    mut memory_stats: ResMut<WasmMemoryStats>,
    mut performance_stats: ResMut<PerformanceStats>,
    mut solar_params: ResMut<SolarSystemParameters>,
) {
    let performance = web_sys::window()
        .and_then(|window| window.performance())
        .and_then(|perf| js_sys::Reflect::get(&perf, &JsValue::from_str("memory")).ok())
        .unwrap_or(JsValue::UNDEFINED);

    let used = js_sys::Reflect::get(&performance, &JsValue::from_str("usedJSHeapSize"))
        .ok()
        .and_then(|value| value.as_f64())
        .unwrap_or(0.0) as u64;
    let limit = js_sys::Reflect::get(&performance, &JsValue::from_str("jsHeapSizeLimit"))
        .ok()
        .and_then(|value| value.as_f64())
        .unwrap_or(0.0) as u64;

    if used == 0 || limit == 0 || !performance_stats.adaptive_enabled {
        return;
    }

    memory_stats.used_heap_bytes = used;
    memory_stats.heap_limit_bytes = limit;
    memory_stats.utilization = used as f32 / limit as f32;

    if memory_stats.utilization > 0.8 {
        let new_quality = match performance_stats.quality_level {
            QualityLevel::Ultra => QualityLevel::High,
            QualityLevel::High => QualityLevel::Medium,
            QualityLevel::Medium => QualityLevel::Low,
            QualityLevel::Low => QualityLevel::Minimal,
            QualityLevel::Minimal => QualityLevel::Minimal,
        };
        if new_quality != performance_stats.quality_level {
            performance_stats.quality_level = new_quality;
            apply_quality_settings(
                new_quality,
                &mut solar_params,
                performance_stats.fps_display,
            );
            web_sys::console::log_1(&"Memory pressure: reducing quality".into());
        }
    }
}

/// Initialize Vulkan compute solver for native builds
#[cfg(all(not(target_arch = "wasm32"), feature = "ash"))]
pub fn init_vulkan_solver(mut perf_stats: ResMut<PerformanceStats>) {
    // Device creation blocks on adapter/device requests. The CPU SIMD path is
    // already the default, so native Vulkan compute must be explicitly enabled
    // instead of stalling the first rendered frame on every installation.
    if !vulkan_compute_enabled_from_env(
        std::env::var("COSMIC_ENABLE_VULKAN_COMPUTE")
            .ok()
            .as_deref(),
    ) {
        return;
    }

    // Only initialize once
    if perf_stats.vulkan_solver.is_some() || perf_stats.vulkan_initialized {
        return;
    }

    perf_stats.vulkan_initialized = true;

    // Try to initialize Vulkan solver
    match init_vulkan_compute() {
        Ok(solver) => {
            perf_stats.vulkan_solver = Some(solver);
            perf_stats.vulkan_enabled = true;
        }
        Err(_) => {
            perf_stats.vulkan_enabled = false;
        }
    }
}

#[cfg(all(not(target_arch = "wasm32"), feature = "ash"))]
fn vulkan_compute_enabled_from_env(value: Option<&str>) -> bool {
    matches!(value, Some("1" | "true" | "TRUE" | "yes" | "YES"))
}

/// Initialize Vulkan compute pipeline
#[cfg(all(not(target_arch = "wasm32"), feature = "ash"))]
fn init_vulkan_compute() -> Result<
    crate::infrastructure::gpu_compute::vulkan_kepler::VulkanKeplerSolver,
    Box<dyn std::error::Error>,
> {
    crate::infrastructure::gpu_compute::vulkan_kepler::VulkanKeplerSolver::new()
}

use super::components::*;
#[cfg(target_arch = "wasm32")]
use crate::domain::value_objects::solar_system_params::SolarSystemParameters;
#[cfg(target_arch = "wasm32")]
use crate::infrastructure::bevy_adapters::performance_systems::apply_quality_settings;
#[cfg(target_arch = "wasm32")]
use crate::infrastructure::gpu_compute::webgpu_kepler::WebGpuKeplerSolver;
use bevy::prelude::*;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsValue;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::spawn_local;

#[cfg(all(test, not(target_arch = "wasm32"), feature = "ash"))]
mod tests {
    use super::vulkan_compute_enabled_from_env;

    #[test]
    fn vulkan_compute_requires_explicit_opt_in() {
        assert!(!vulkan_compute_enabled_from_env(None));
        assert!(!vulkan_compute_enabled_from_env(Some("0")));
        assert!(vulkan_compute_enabled_from_env(Some("1")));
        assert!(vulkan_compute_enabled_from_env(Some("true")));
    }
}
