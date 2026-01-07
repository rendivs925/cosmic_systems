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

    if used == 0 || limit == 0 {
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
            apply_quality_settings(new_quality, &mut solar_params, performance_stats.fps_display);
            web_sys::console::log_1(&"Memory pressure: reducing quality".into());
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub fn queue_orbit_mesh_tasks(
    mut worker_pool: NonSendMut<OrbitMeshWorkerPool>,
    pending_query: Query<(Entity, &PendingOrbitMesh)>,
) {
    if worker_pool.worker_count() == 0 {
        return;
    }

    for (entity, pending) in pending_query.iter() {
        let task = OrbitMeshTask {
            task_id: task_id_from_entity(entity),
            segments: pending.segments as u32,
            orbit_shape: OrbitShapeData {
                semi_major_axis_units: pending.orbit_shape.semi_major_axis_units,
                eccentricity: pending.orbit_shape.eccentricity,
                inclination_rad: pending.orbit_shape.inclination_rad,
                long_asc_node_rad: pending.orbit_shape.long_asc_node_rad,
                arg_periapsis_rad: pending.orbit_shape.arg_periapsis_rad,
            },
        };
        worker_pool.request(task);
    }
}

#[cfg(target_arch = "wasm32")]
pub fn apply_orbit_mesh_results(
    mut commands: Commands,
    mut worker_pool: NonSendMut<OrbitMeshWorkerPool>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut pending_query: Query<&mut PendingOrbitMesh>,
) {
    for result in worker_pool.take_results() {
        let entity = entity_from_task_id(result.task_id);
        let Ok(mut pending) = pending_query.get_mut(entity) else {
            worker_pool.mark_complete(result.task_id);
            continue;
        };

        let segments = pending.segments;
        if segments == 0 {
            worker_pool.mark_complete(result.task_id);
            commands.entity(entity).remove::<PendingOrbitMesh>();
            continue;
        }

        let mut positions = Vec::with_capacity(segments);
        let mut normals = Vec::with_capacity(segments);
        let mut uvs = Vec::with_capacity(segments);
        let mut colors = Vec::with_capacity(segments);
        let mut indices = Vec::with_capacity(segments * 2);
        let color: LinearRgba = pending.color.into();
        let color = [color.red, color.green, color.blue, color.alpha];

        let coords = result.positions;
        let coord_count = coords.len() / 3;
        let usable = coord_count.min(segments);
        for i in 0..usable {
            let idx = i * 3;
            positions.push([coords[idx], coords[idx + 1], coords[idx + 2]]);
            normals.push([0.0, 1.0, 0.0]);
            uvs.push([i as f32 / segments as f32, 0.5]);
            colors.push(color);
            indices.push(i as u32);
            indices.push(((i + 1) % segments) as u32);
        }

        if let Some(mesh) = meshes.get_mut(&pending.mesh) {
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
            mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
            mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
            mesh.insert_indices(Indices::U32(indices));
        }

        worker_pool.mark_complete(result.task_id);
        commands.entity(entity).remove::<PendingOrbitMesh>();
    }
}

/// Initialize Vulkan compute solver for native builds
#[cfg(all(not(target_arch = "wasm32"), feature = "ash"))]
pub fn init_vulkan_solver(mut perf_stats: ResMut<PerformanceStats>) {
    // Vulkan compilation test removed
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

/// Initialize Vulkan compute pipeline
#[cfg(all(not(target_arch = "wasm32"), feature = "ash"))]
fn init_vulkan_compute() -> Result<
    crate::infrastructure::gpu_compute::vulkan_kepler::VulkanKeplerSolver,
    Box<dyn std::error::Error>,
> {
    crate::infrastructure::gpu_compute::vulkan_kepler::VulkanKeplerSolver::new()
}

use super::components::*;
use bevy::prelude::*;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsValue;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::spawn_local;