use super::components::*;
use crate::domain::services::physics;
use crate::domain::value_objects::solar_system_params::SolarSystemParameters;
#[cfg(target_arch = "wasm32")]
use crate::infrastructure::gpu_compute::webgpu_kepler::PlanetGpuInput;
#[cfg(target_arch = "wasm32")]
use crate::infrastructure::gpu_compute::webgpu_kepler::WebGpuKeplerSolver;
#[cfg(target_arch = "wasm32")]
use crate::infrastructure::web_workers::physics_worker::{PhysicsTask, PhysicsWorkerPool};
use bevy::prelude::*;
#[cfg(target_arch = "wasm32")]
use bevy::render::mesh::Indices;
use bevy::time::Fixed;
#[cfg(feature = "parallel")]
use rayon::prelude::*;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsValue;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::spawn_local;

// System to update planet/moon positions in their orbits (optimized for performance with parallel processing)
#[cfg(target_arch = "wasm32")]
pub fn update_planet_positions(
    time: Res<Time<Fixed>>,
    solar_params: Res<SolarSystemParameters>,
    camera_query: Query<&GlobalTransform, With<CameraController>>,
    mut query: Query<(Entity, &mut Transform, &PlanetComponent)>,
    mut worker_pool: NonSendMut<PhysicsWorkerPool>,
    chrome: Option<Res<ChromeOptimizations>>,
    mut webgpu_state: Option<NonSendMut<WebGpuKeplerState>>,
    mut perf_stats: ResMut<PerformanceStats>,
) {
    // Start timing for physics update
    let physics_start = std::time::Instant::now();

    let elapsed_seconds = time.elapsed_seconds();
    let time_days = solar_params.time_to_days(elapsed_seconds);

    let camera_pos = camera_query.single().translation();

    let webgpu_enabled = chrome.as_ref().is_some_and(|chrome| chrome.webgpu_enabled);
    let solver_ready = webgpu_state
        .as_ref()
        .is_some_and(|state| state.solver.borrow().is_some());
    let webgpu_active = webgpu_enabled && solver_ready;

    if webgpu_active {
        if let Some(state) = webgpu_state.as_mut() {
            let mut results = state.results.borrow_mut();
            if !results.is_empty() {
                for (entity, position) in results.drain(..) {
                    if let Ok((_, mut transform, _)) = query.get_mut(entity) {
                        transform.translation = position;
                    }
                }
            }
        }
    }

    let mut parent_positions = std::collections::HashMap::new();
    let mut parent_tilts = std::collections::HashMap::new();

    for (_, transform, planet_comp) in query.iter() {
        if planet_comp.domain_planet.parent_entity.is_none() {
            parent_positions.insert(
                planet_comp.domain_planet.name.clone(),
                transform.translation,
            );
            parent_tilts.insert(
                planet_comp.domain_planet.name.clone(),
                Some(planet_comp.domain_planet.axial_tilt_deg),
            );
        }
    }

    let mut worker_tasks: Vec<(f32, PhysicsTask)> = Vec::new();
    let mut gpu_inputs: Vec<PlanetGpuInput> = Vec::new();
    let mut gpu_entities: Vec<Entity> = Vec::new();

    for (entity, mut transform, planet_comp) in query.iter_mut() {
        let distance_to_camera = camera_pos.distance(transform.translation);

        let (parent_position, parent_tilt) =
            if let Some(parent_name) = &planet_comp.domain_planet.parent_entity {
                (
                    *parent_positions.get(parent_name).unwrap_or(&Vec3::ZERO),
                    parent_tilts.get(parent_name).copied().flatten(),
                )
            } else {
                (Vec3::ZERO, None)
            };

        let kepler_iterations = physics::get_kepler_iterations_for_distance(distance_to_camera);
        let is_moon = planet_comp.domain_planet.parent_entity.is_some();

        let new_position = physics::calculate_planet_position_with_quality(
            &planet_comp.domain_planet,
            time_days,
            &solar_params,
            parent_position,
            parent_tilt,
            kepler_iterations,
        );
        transform.translation = new_position;

        if webgpu_active && !is_moon && planet_comp.domain_planet.name != "Sun" {
            let elements = physics::orbital_elements_for(&planet_comp.domain_planet);
            let mean_anomaly_rad = if let Some(elements) = elements {
                let mean_motion = 0.01720209895 / elements.semi_major_axis_au.powf(1.5);
                elements.mean_anomaly_rad + mean_motion * time_days
            } else if planet_comp.domain_planet.orbital_period_days > 0.0 {
                std::f32::consts::TAU * (time_days / planet_comp.domain_planet.orbital_period_days)
            } else {
                0.0
            };

            let elements = elements.unwrap_or(crate::domain::services::physics::OrbitalElements {
                semi_major_axis_au: planet_comp.domain_planet.orbital_distance_au,
                eccentricity: 0.0,
                inclination_rad: 0.0,
                long_asc_node_rad: 0.0,
                arg_periapsis_rad: 0.0,
                mean_anomaly_rad: 0.0,
            });

            gpu_inputs.push(PlanetGpuInput {
                semi_major_axis_au: elements.semi_major_axis_au,
                eccentricity: elements.eccentricity,
                inclination_rad: elements.inclination_rad,
                long_asc_node_rad: elements.long_asc_node_rad,
                arg_periapsis_rad: elements.arg_periapsis_rad,
                mean_anomaly_rad,
                scale_factor: solar_params.scale_factor,
                moon_scale: physics::MOON_ORBIT_SCALE,
                parent_x: parent_position.x,
                parent_y: parent_position.y,
                parent_z: parent_position.z,
                parent_tilt_rad: parent_tilt.map(|deg| deg.to_radians()).unwrap_or(0.0),
                iterations: kepler_iterations,
                is_moon: 0,
                has_parent_tilt: 0,
                _pad: 0,
            });
            gpu_entities.push(entity);
            continue;
        }

        let should_use_worker = worker_pool.worker_count() > 0
            && planet_comp.domain_planet.name != "Sun"
            && worker_pool.can_accept_tasks();

        if should_use_worker {
            let elements = physics::orbital_elements_for(&planet_comp.domain_planet);
            let (has_elements, orbital_elements) = if let Some(elements) = elements {
                (
                    true,
                    crate::infrastructure::web_workers::physics_worker::OrbitalElements {
                        semi_major_axis_au: elements.semi_major_axis_au,
                        eccentricity: elements.eccentricity,
                        inclination_rad: elements.inclination_rad,
                        long_asc_node_rad: elements.long_asc_node_rad,
                        arg_periapsis_rad: elements.arg_periapsis_rad,
                        mean_anomaly_rad: elements.mean_anomaly_rad,
                    },
                )
            } else {
                (
                    false,
                    crate::infrastructure::web_workers::physics_worker::OrbitalElements {
                        semi_major_axis_au: 0.0,
                        eccentricity: 0.0,
                        inclination_rad: 0.0,
                        long_asc_node_rad: 0.0,
                        arg_periapsis_rad: 0.0,
                        mean_anomaly_rad: 0.0,
                    },
                )
            };

            worker_tasks.push((
                distance_to_camera,
                PhysicsTask {
                    worker_id: 0,
                    entity_bits: entity.to_bits(),
                    orbital_elements,
                    has_elements,
                    is_moon: planet_comp.domain_planet.parent_entity.is_some(),
                    parent_position:
                        crate::infrastructure::web_workers::physics_worker::WorkerVec3 {
                            x: parent_position.x,
                            y: parent_position.y,
                            z: parent_position.z,
                        },
                    parent_tilt_deg: parent_tilt,
                    orbital_distance_au: planet_comp.domain_planet.orbital_distance_au,
                    orbital_period_days: planet_comp.domain_planet.orbital_period_days,
                    time_days,
                    kepler_iterations,
                    scale_factor: solar_params.scale_factor,
                    moon_orbit_scale: physics::MOON_ORBIT_SCALE,
                },
            ));
            continue;
        }

        let new_position = physics::calculate_planet_position_with_quality(
            &planet_comp.domain_planet,
            time_days,
            &solar_params,
            parent_position,
            parent_tilt,
            kepler_iterations,
        );
        transform.translation = new_position;
    }

    if worker_pool.worker_count() > 0 && !worker_tasks.is_empty() {
        worker_tasks.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        let tasks = worker_tasks.into_iter().map(|(_, task)| task).collect();
        worker_pool.queue_tasks(tasks);
    }

    for result in worker_pool.collect_results() {
        if let Ok((_, mut transform, _)) = query.get_mut(result.entity) {
            transform.translation = result.position;
        }
    }

    if webgpu_active && !gpu_inputs.is_empty() {
        if let Some(state) = webgpu_state.as_mut() {
            if !*state.in_flight.borrow() {
                let solver_ref = state.solver.clone();
                let results_ref = state.results.clone();
                let in_flight = state.in_flight.clone();
                *in_flight.borrow_mut() = true;
                let entities = gpu_entities.clone();
                spawn_local(async move {
                    let solver_opt = solver_ref.borrow_mut().take();
                    if let Some(mut solver) = solver_opt {
                        let result = solver.solve_positions(&gpu_inputs).await;
                        *solver_ref.borrow_mut() = Some(solver);
                        if let Ok(positions) = result {
                            let mut results = results_ref.borrow_mut();
                            results.clear();
                            results.extend(entities.into_iter().zip(positions));
                        }
                    }
                    *in_flight.borrow_mut() = false;
                });
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn update_planet_positions(
    time: Res<Time<Fixed>>,
    solar_params: Res<SolarSystemParameters>,
    camera_query: Query<&GlobalTransform, With<CameraController>>,
    mut query: Query<(Entity, &mut Transform, &PlanetComponent)>,
    mut perf_stats: ResMut<PerformanceStats>,
) {
    // Start timing for physics update
    let physics_start = std::time::Instant::now();

    let elapsed_seconds = time.elapsed_seconds();
    let time_days = solar_params.time_to_days(elapsed_seconds);

    let camera_pos = camera_query.single().translation();

    // Build parent position and tilt lookup maps
    let mut parent_positions = std::collections::HashMap::new();
    let mut parent_tilts = std::collections::HashMap::new();

    // First pass: collect all planet positions for moon calculations
    for (_entity, transform, planet_comp) in query.iter() {
        if planet_comp.domain_planet.parent_entity.is_none() {
            // This is a planet orbiting the Sun
            parent_positions.insert(
                planet_comp.domain_planet.name.clone(),
                transform.translation,
            );
            parent_tilts.insert(
                planet_comp.domain_planet.name.clone(),
                Some(planet_comp.domain_planet.axial_tilt_deg),
            );
        }
    }

    #[cfg(feature = "parallel")]
    {
        // Parallel implementation
        update_planet_positions_parallel(
            time_days,
            solar_params,
            camera_pos,
            &parent_positions,
            &parent_tilts,
            &mut query,
            &mut perf_stats,
        );
    }

    #[cfg(not(feature = "parallel"))]
    {
        // Fallback sequential implementation
        update_planet_positions_sequential(
            time_days,
            solar_params,
            camera_pos,
            &parent_positions,
            &parent_tilts,
            &mut query,
        );
    }

    // Record physics timing
    let physics_duration = physics_start.elapsed();
    perf_stats.physics_update_time = physics_duration.as_secs_f32() * 1000.0;

    // Update SIMD/parallel flags based on build configuration
    perf_stats.simd_enabled = cfg!(feature = "simd");
    perf_stats.parallel_enabled = cfg!(feature = "parallel");
    perf_stats.cpu_cores_used = num_cpus::get();
}

/// Parallel optimized position updates
#[cfg(feature = "parallel")]
fn update_planet_positions_parallel(
    time_days: f32,
    solar_params: Res<SolarSystemParameters>,
    camera_pos: Vec3,
    parent_positions: &std::collections::HashMap<String, Vec3>,
    parent_tilts: &std::collections::HashMap<String, Option<f32>>,
    query: &mut Query<(Entity, &mut Transform, &PlanetComponent)>,
    perf_stats: &mut ResMut<PerformanceStats>,
) {
    // Collect planet data for batch processing
    let planet_data: Vec<_> = query
        .iter_mut()
        .map(|(entity, transform, planet_comp)| {
            let distance_to_camera = camera_pos.distance(transform.translation);
            let kepler_iterations = physics::get_kepler_iterations_for_distance(distance_to_camera);

            let (parent_position, parent_tilt) =
                if let Some(parent_name) = &planet_comp.domain_planet.parent_entity {
                    (
                        *parent_positions.get(parent_name).unwrap_or(&Vec3::ZERO),
                        parent_tilts.get(parent_name).copied().flatten(),
                    )
                } else {
                    (Vec3::ZERO, None)
                };

            (
                entity,
                planet_comp.domain_planet.clone(),
                parent_position,
                parent_tilt,
                kepler_iterations,
                transform,
            )
        })
        .collect();

    // Hybrid GPU+CPU processing with batching and concurrent execution
    let position_updates: Vec<(Entity, Vec3)> = if perf_stats.vulkan_enabled
        && perf_stats.vulkan_solver.is_some()
    {
        // Separate planets from moons for optimal batching
        let (planets_data, moons_data): (Vec<_>, Vec<_>) = planet_data
            .into_iter()
            .partition(|(_, planet, _, _, _, _)| planet.parent_entity.is_none());

        let mut all_updates = Vec::new();

        // Process planets and moons concurrently for maximum parallelization
        let (planet_updates, moon_updates) = rayon::join(
            || {
                // Process planets via hybrid routing (GPU if available, SIMD fallback)
                if !planets_data.is_empty() {
                    let mut results = Vec::new();
                    let mut simd_solver =
                        crate::infrastructure::bevy_adapters::simd_kepler::SimdKeplerSolver::new();

                    // Extract planets for batch processing - group into larger batches for GPU efficiency
                    let planets: Vec<_> = planets_data
                        .iter()
                        .map(|(_, planet, _, _, _, _)| planet.clone())
                        .collect();
                    let planet_entities: Vec<_> = planets_data
                        .iter()
                        .map(|(entity, _, _, _, _, _)| *entity)
                        .collect();

                    // Process in batches of up to 100 planets for optimal GPU utilization
                    for chunk in planets.chunks(100).zip(planet_entities.chunks(100)) {
                        let (planet_chunk, entity_chunk) = chunk;

                        // Batch process planets through hybrid compute
                        let (positions, backend_used) = crate::infrastructure::bevy_adapters::components::process_hybrid_compute(
                            planet_chunk,
                            perf_stats.quality_level,
                            time_days,
                            solar_params.scale_factor,
                            perf_stats.vulkan_enabled,
                            &mut perf_stats.vulkan_solver,
                            &mut simd_solver,
                        );

                        // Record GPU usage
                        if matches!(backend_used, crate::infrastructure::bevy_adapters::components::ComputeBackendType::VulkanGpu) {
                            perf_stats.vulkan_kepler_calls += planet_chunk.len() as u64;
                        }

                        // Combine entity IDs with positions
                        for (entity, position) in entity_chunk.iter().zip(positions) {
                            results.push((*entity, position));
                        }
                    }
                    results
                } else {
                    Vec::new()
                }
            },
            || {
                // Process moons in parallel via SIMD
                moons_data
                    .into_par_iter()
                    .map(
                        |(entity, planet, parent_pos, parent_tilt, kepler_iterations, _)| {
                            let position = physics::calculate_planet_position_with_quality(
                                &planet,
                                time_days,
                                &solar_params,
                                parent_pos,
                                parent_tilt,
                                kepler_iterations,
                            );
                            (entity, position)
                        },
                    )
                    .collect::<Vec<(Entity, Vec3)>>()
            },
        );

        all_updates.extend(planet_updates);
        all_updates.extend(moon_updates);
        all_updates
    } else {
        // Fallback to parallel SIMD processing when Vulkan is not available
        planet_data
            .into_par_iter()
            .map(
                |(entity, planet, parent_pos, parent_tilt, kepler_iterations, _)| {
                    let position = physics::calculate_planet_position_with_quality(
                        &planet,
                        time_days,
                        &solar_params,
                        parent_pos,
                        parent_tilt,
                        kepler_iterations,
                    );
                    (entity, position)
                },
            )
            .collect()
    };

    // Apply position updates
    for (entity, new_position) in position_updates {
        if let Ok((_, mut transform, _)) = query.get_mut(entity) {
            transform.translation = new_position;
        }
    }
}

/// Fallback sequential implementation for when parallel features are disabled
#[cfg(not(feature = "parallel"))]
fn update_planet_positions_sequential(
    time_days: f32,
    solar_params: Res<SolarSystemParameters>,
    camera_pos: Vec3,
    parent_positions: &std::collections::HashMap<String, Vec3>,
    parent_tilts: &std::collections::HashMap<String, Option<f32>>,
    query: &mut Query<(Entity, &mut Transform, &PlanetComponent)>,
) {
    for (_entity, mut transform, planet_comp) in query.iter_mut() {
        let distance_to_camera = camera_pos.distance(transform.translation);

        let (parent_position, parent_tilt) =
            if let Some(parent_name) = &planet_comp.domain_planet.parent_entity {
                (
                    *parent_positions.get(parent_name).unwrap_or(&Vec3::ZERO),
                    parent_tilts.get(parent_name).copied().flatten(),
                )
            } else {
                (Vec3::ZERO, None)
            };

        let kepler_iterations = physics::get_kepler_iterations_for_distance(distance_to_camera);
        let new_position = physics::calculate_planet_position_with_quality(
            &planet_comp.domain_planet,
            time_days,
            &solar_params,
            parent_position,
            parent_tilt,
            kepler_iterations,
        );
        transform.translation = new_position;
    }
}

// System to update planet rotations
pub fn update_planet_rotations(
    time: Res<Time<Fixed>>,
    solar_params: Res<SolarSystemParameters>,
    mut query: Query<(&mut Transform, &PlanetComponent)>,
) {
    let elapsed_seconds = time.elapsed_seconds();
    let time_days = solar_params.time_to_days(elapsed_seconds);

    for (mut transform, planet_comp) in query.iter_mut() {
        let rotation_angle =
            physics::calculate_planet_rotation(&planet_comp.domain_planet, time_days);
        let tilt_rad = planet_comp.domain_planet.axial_tilt_deg.to_radians();

        // Apply axial tilt, then spin around the tilted local Y axis.
        let tilt = Quat::from_rotation_z(tilt_rad);
        let spin = Quat::from_rotation_y(rotation_angle);
        transform.rotation = tilt * spin;
    }
}

// System to update moon orbit positions to follow their parent planets
pub fn update_moon_orbit_positions(
    mut moon_orbit_query: Query<(&mut Transform, &OrbitComponent), With<MoonOrbit>>,
    planet_query: Query<(&Transform, &PlanetComponent), Without<MoonOrbit>>,
) {
    for (mut orbit_transform, orbit_comp) in moon_orbit_query.iter_mut() {
        // Get the parent planet's position
        if let Ok((parent_transform, parent_comp)) = planet_query.get(orbit_comp.planet_entity) {
            // Update orbit position and align the orbit plane with the parent axial tilt
            orbit_transform.translation = parent_transform.translation;
            orbit_transform.rotation =
                Quat::from_rotation_z(parent_comp.domain_planet.axial_tilt_deg.to_radians());
        }
    }
}