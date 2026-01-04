use super::components::*;
use crate::application::simulation_service::SimulationService;
use crate::domain::services::physics;
use crate::domain::value_objects::simulation_params::SimulationParameters;
use crate::domain::value_objects::solar_system_params::SolarSystemParameters;
#[cfg(target_arch = "wasm32")]
use crate::infrastructure::gpu_compute::webgpu_kepler::PlanetGpuInput;
#[cfg(target_arch = "wasm32")]
use crate::infrastructure::gpu_compute::webgpu_kepler::WebGpuKeplerSolver;
#[cfg(target_arch = "wasm32")]
use crate::infrastructure::web_workers::orbit_mesh_worker::{
    entity_from_task_id, task_id_from_entity, OrbitMeshTask, OrbitMeshWorkerPool, OrbitShapeData,
};
#[cfg(target_arch = "wasm32")]
use crate::infrastructure::web_workers::physics_worker::{PhysicsTask, PhysicsWorkerPool};
#[cfg(target_arch = "wasm32")]
use crate::infrastructure::web_workers::texture_worker::TextureDecodeWorker;
use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy::prelude::*;
#[cfg(target_arch = "wasm32")]
use bevy::render::mesh::Indices;
use bevy::time::Fixed;
#[cfg(all(not(target_arch = "wasm32"), feature = "ash"))]
// Vulkan import removed - implementation simplified
#[cfg(feature = "parallel")]
use rayon::prelude::*;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsValue;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::spawn_local;

// System to update gyroscopes
pub fn update_gyroscopes(time: Res<Time>, mut query: Query<(&GyroscopeComponent, &mut Transform)>) {
    for (gyro, mut transform) in query.iter_mut() {
        // Update gyroscope rotation based on time
        let spin_rate = gyro.domain_gyro.spin_rate;
        let precession_rate = gyro.domain_gyro.precession_rate;
        let delta_time = time.delta_seconds();

        // Apply spin rotation around the angular momentum axis
        let spin_rotation = Quat::from_axis_angle(
            gyro.domain_gyro.angular_momentum.normalize(),
            spin_rate * delta_time,
        );
        transform.rotate(spin_rotation);

        // Apply precession (wobble) if precession rate > 0
        if precession_rate > 0.0 {
            let precession_axis = Vec3::Y; // Precession around Y axis
            let precession_rotation =
                Quat::from_axis_angle(precession_axis, precession_rate * delta_time);
            transform.rotate(precession_rotation);
        }
    }
}

// Performance monitoring and quality adaptation system
pub fn update_performance_monitor(
    mut perf_stats: ResMut<PerformanceStats>,
    mut quality_controller: ResMut<QualityController>,
    time: Res<Time>,
) {
    // Update frame time history
    perf_stats.frame_time = time.delta_seconds();
    quality_controller
        .frame_times
        .push_back(perf_stats.frame_time);

    if quality_controller.frame_times.len() > 60 {
        quality_controller.frame_times.pop_front();
    }

    // Calculate average FPS
    let avg_frame_time = quality_controller.frame_times.iter().sum::<f32>()
        / quality_controller.frame_times.len() as f32;
    perf_stats.fps = 1.0 / avg_frame_time;

    // Update quality level in PerformanceStats to match QualityController
    perf_stats.quality_level = quality_controller.current_level;

    // Gradual quality adaptation
    quality_controller.adapt_quality(perf_stats.fps);
}

// System to update thrust visualization
pub fn update_thrust(
    time: Res<Time>,
    params: Res<SimulationParameters>,
    gyro_query: Query<&GyroscopeComponent>,
    mut arrow_query: Query<&mut Transform, With<ThrustArrow>>,
) {
    let gyros: Vec<_> = gyro_query.iter().map(|g| &g.domain_gyro).collect();
    if gyros.is_empty() {
        return;
    }
    let total_thrust = SimulationService::calculate_thrust(&gyros, &params);

    for mut transform in arrow_query.iter_mut() {
        let scale = crate::domain::services::physics::calculate_arrow_scale(total_thrust);
        transform.scale = Vec3::new(0.1, 0.1, scale);

        if total_thrust.length() > 0.001 {
            let translation = transform.translation;
            let target = translation + total_thrust.normalize();
            transform.look_at(target, Vec3::Y);
        }

        transform.scale *= 1.0 + 0.1 * (time.elapsed_seconds() * 5.0).sin();
    }
}

// System to handle user input for controlling simulation parameters
pub fn handle_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut params: ResMut<SimulationParameters>,
    time: Res<Time>,
) {
    let rpm_delta = 5000.0 * time.delta_seconds(); // Adjust RPM by 5000 per second

    if keyboard.pressed(KeyCode::ArrowUp) {
        params.rpm += rpm_delta;
        println!("RPM increased to: {:.0}", params.rpm);
    }
    if keyboard.pressed(KeyCode::ArrowDown) {
        params.rpm -= rpm_delta;
        if params.rpm < 0.0 {
            params.rpm = 0.0;
        }
        println!("RPM decreased to: {:.0}", params.rpm);
    }

    // Optional: Add controls for other parameters
    let param_delta = 10.0 * time.delta_seconds();
    if keyboard.pressed(KeyCode::KeyW) {
        params.precession_hz += param_delta;
        println!("Precession Hz increased to: {:.1}", params.precession_hz);
    }
    if keyboard.pressed(KeyCode::KeyS) {
        params.precession_hz -= param_delta;
        if params.precession_hz < 0.0 {
            params.precession_hz = 0.0;
        }
        println!("Precession Hz decreased to: {:.1}", params.precession_hz);
    }

    if keyboard.pressed(KeyCode::KeyA) {
        params.asymmetry -= param_delta * 0.1;
        params.asymmetry = params.asymmetry.clamp(0.0, 1.0);
        println!("Asymmetry decreased to: {:.2}", params.asymmetry);
    }
    if keyboard.pressed(KeyCode::KeyD) {
        params.asymmetry += param_delta * 0.1;
        params.asymmetry = params.asymmetry.clamp(0.0, 1.0);
        println!("Asymmetry increased to: {:.2}", params.asymmetry);
    }
}

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
    let max_distance = 15_000_000.0;

    for (entity, mut transform, planet_comp) in query.iter_mut() {
        let distance_to_camera = camera_pos.distance(transform.translation);
        if distance_to_camera > max_distance {
            continue;
        }

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
        .filter(|(_, transform, _)| camera_pos.distance(transform.translation) <= 15_000_000.0)
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
        if distance_to_camera > 15_000_000.0 {
            continue;
        }

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

// System to animate orbit visuals for a more dynamic presentation
// Optimized to update every 3 frames instead of every frame
#[allow(dead_code)]
pub(crate) fn update_orbit_visuals(
    time: Res<Time>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    query: Query<&OrbitComponent>,
    shared: Option<Res<crate::application::solar_system_startup::SharedOrbitMaterial>>,
) {
    // Performance optimization: update orbit visuals only every 3 frames
    // Visual pulsing is slow enough that this is imperceptible
    let frame_number = (time.elapsed_seconds() * 60.0) as u32; // Assume 60 FPS
    #[cfg(target_arch = "wasm32")]
    let update_stride = 12;
    #[cfg(not(target_arch = "wasm32"))]
    let update_stride = 3;

    if frame_number % update_stride != 0 {
        return;
    }

    let elapsed = time.elapsed_seconds();

    if query.is_empty() {
        return;
    }

    let Some(shared) = shared else {
        return;
    };

    if let Some(material) = materials.get_mut(&shared.handle) {
        let pulse = 0.5 + 0.5 * (elapsed * 0.18).sin();
        let alpha = (0.25 + 0.15 * pulse).clamp(0.2, 0.45);
        material.base_color = Color::srgb(1.0, 1.0, 1.0).with_alpha(alpha);
        let glow = 0.6 + 0.35 * pulse;
        material.emissive = LinearRgba::new(glow, glow, glow, 1.0);
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

// System to toggle orbit visibility based on show_orbits parameter
pub fn update_orbit_visibility(
    solar_params: Res<SolarSystemParameters>,
    mut orbit_query: Query<&mut Visibility, With<OrbitComponent>>,
) {
    for mut visibility in orbit_query.iter_mut() {
        *visibility = if solar_params.show_orbits {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}

// System to add dynamic specular reflection response for planet materials
// Optimized to update every 5 frames (material properties don't change dynamically)
pub fn update_planet_reflections(
    time: Res<Time>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    query: Query<(&PlanetComponent, &GlobalTransform)>,
) {
    // Performance optimization: skip most updates since values don't change
    let frame_number = (time.elapsed_seconds() * 60.0) as u32;
    #[cfg(target_arch = "wasm32")]
    let update_stride = 20;
    #[cfg(not(target_arch = "wasm32"))]
    let update_stride = 5;

    if frame_number % update_stride != 0 {
        return;
    }

    for (planet_comp, _global_transform) in query.iter() {
        if planet_comp.domain_planet.name == "Sun" {
            continue;
        }
        if let Some(material) = materials.get_mut(&planet_comp.material) {
            material.perceptual_roughness = planet_comp.base_roughness;
            material.reflectance = planet_comp.base_reflectance;
        }
    }
}

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
    mut texture_worker: NonSendMut<TextureDecodeWorker>,
    memory_stats: Option<Res<WasmMemoryStats>>,
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
    mut texture_worker: NonSendMut<TextureDecodeWorker>,
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
            apply_quality_settings(new_quality, &mut solar_params);
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

// System to handle planet selection
pub fn handle_planet_selection(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut selected_planet: ResMut<SelectedPlanet>,
    mut selectable_query: Query<(Entity, &mut Selectable)>,
) {
    let mut selection_changed = false;
    let mut new_selected_entity = selected_planet.entity;
    let mut new_selected_name = selected_planet.name.clone();

    // Cycle through planets with Tab key
    if keyboard.just_pressed(KeyCode::Tab) {
        // Collect all selectable entities first
        let all_entities: Vec<Entity> = selectable_query.iter().map(|(entity, _)| entity).collect();

        if all_entities.is_empty() {
            return;
        }

        // Find current selection index
        let current_index = if let Some(current_entity) = selected_planet.entity {
            all_entities
                .iter()
                .position(|&entity| entity == current_entity)
                .unwrap_or(0)
        } else {
            0
        };

        // Move to next planet (wrap around)
        let next_index = (current_index + 1) % all_entities.len();
        let next_entity = all_entities[next_index];

        // Get the name from the entity (we'll need to query again, but this avoids borrowing issues)
        if let Ok((_, selectable)) = selectable_query.get(next_entity) {
            new_selected_entity = Some(next_entity);
            new_selected_name = Some(selectable.name.clone());
            selection_changed = true;
            println!("Selected planet: {}", selectable.name);
        }
    }

    // Deselect with Escape
    if keyboard.just_pressed(KeyCode::Escape) {
        new_selected_entity = None;
        new_selected_name = None;
        selection_changed = true;
        println!("Deselected planet");
    }

    // Update selection resource
    if selection_changed {
        selected_planet.entity = new_selected_entity;
        selected_planet.name = new_selected_name;

        // Update all selectable components
        let target_entity = selected_planet.entity;
        for (entity, mut selectable) in selectable_query.iter_mut() {
            selectable.selected = Some(entity) == target_entity;
        }
    }
}

// System to handle mouse clicking for planet selection
pub fn handle_mouse_planet_selection(
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    camera_query: Query<(&Camera, &GlobalTransform), With<CameraController>>,
    windows: Query<&Window>,
    solar_params: Res<SolarSystemParameters>,
    ui_state: Res<UiPointerState>,
    mut selected_planet: ResMut<SelectedPlanet>,
    mut selectable_query: Query<(Entity, &mut Selectable, &PlanetComponent, &GlobalTransform)>,
) {
    // Only handle left mouse button clicks
    if !mouse_buttons.just_pressed(MouseButton::Left) {
        return;
    }
    if ui_state.is_over_ui {
        return;
    }

    let (camera, camera_transform) = camera_query.single();
    let window = match windows.get_single() {
        Ok(window) => window,
        Err(_) => return,
    };
    let cursor_pos = match window.cursor_position() {
        Some(pos) => pos,
        None => return,
    };
    let ray = match camera.viewport_to_world(camera_transform, cursor_pos) {
        Some(ray) => ray,
        None => return,
    };

    // Raycast against planet spheres to find the clicked body.
    let mut closest_entity: Option<Entity> = None;
    let mut closest_t = f32::INFINITY;

    for (entity, _selectable, planet_comp, transform) in selectable_query.iter() {
        let radius = if planet_comp.domain_planet.name == "Sun" {
            physics::calculate_sun_visual_radius(&solar_params)
        } else {
            physics::calculate_visual_radius(&planet_comp.domain_planet, &solar_params)
        };
        let center = transform.translation();
        let oc = ray.origin - center;
        let b = 2.0 * oc.dot(*ray.direction);
        let c = oc.length_squared() - radius * radius;
        let discriminant = b * b - 4.0 * c;
        if discriminant < 0.0 {
            continue;
        }
        let t = (-b - discriminant.sqrt()) * 0.5;
        if t > 0.0 && t < closest_t {
            closest_t = t;
            closest_entity = Some(entity);
        }
    }

    // Update selection
    if let Some(selected_entity) = closest_entity {
        if let Ok((_, selectable, _, _)) = selectable_query.get(selected_entity) {
            selected_planet.entity = Some(selected_entity);
            selected_planet.name = Some(selectable.name.clone());
            println!("Selected planet: {}", selectable.name);
        }
    } else {
        // Clicked on empty space - deselect
        selected_planet.entity = None;
        selected_planet.name = None;
        println!("Deselected planet");
    }

    // Update all selectable components
    let target_entity = selected_planet.entity;
    for (_, mut selectable, _, _) in selectable_query.iter_mut() {
        selectable.selected = false; // Reset all first
    }
    if let Some(entity) = target_entity {
        if let Ok((_, mut selectable, _, _)) = selectable_query.get_mut(entity) {
            selectable.selected = true;
        }
    }
}

// System to update visual feedback for selected planets (optimized)
pub fn update_planet_selection_visuals(
    camera_query: Query<&GlobalTransform, With<CameraController>>,
    mut query: Query<(&Selectable, &mut Transform, &GlobalTransform)>,
) {
    let camera_pos = camera_query.single().translation();

    for (_selectable, mut transform, global_transform) in query.iter_mut() {
        // Distance culling for visual updates
        let distance_to_camera = (global_transform.translation() - camera_pos).length();
        let max_visual_distance = 30000.0; // Only update visuals for reasonably close objects

        if distance_to_camera > max_visual_distance {
            // Reset scale for distant unselected objects
            transform.scale = Vec3::ONE;
            continue;
        }

        // Keep scale fixed regardless of selection.
        transform.scale = Vec3::ONE;
    }
}

// System to handle solar system controls (time scale, etc.)
pub fn handle_solar_system_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut solar_params: ResMut<SolarSystemParameters>,
    mut camera_query: Query<(&mut CameraController, &mut Transform)>,
    selected_planet: Res<SelectedPlanet>,
    planet_query: Query<(&PlanetComponent, &GlobalTransform)>,
    mut screenshot_state: ResMut<ScreenshotState>,
    mut notifications: ResMut<NotificationQueue>,
) {
    // Screenshot feature - F12 or P key
    // Request screenshot, will be captured next frame after notifications hide
    if keyboard.just_pressed(KeyCode::F12) || keyboard.just_pressed(KeyCode::KeyP) {
        notifications.hide_for_screenshot = true;
        screenshot_state.pending = true;
    }
    // Time scale controls
    if keyboard.just_pressed(KeyCode::KeyT) {
        solar_params.time_scale *= 1.1;
        println!("⏩ Time scale: {:.1}x", solar_params.time_scale);
    }

    if keyboard.just_pressed(KeyCode::KeyR) && solar_params.time_scale > 0.1 {
        solar_params.time_scale /= 1.1;
        println!("⏪ Time scale: {:.1}x", solar_params.time_scale);
    }

    // Reset time scale
    if keyboard.just_pressed(KeyCode::KeyY) {
        solar_params.time_scale = 1.0;
        println!("⏸️ Time scale reset to: {:.1}x", solar_params.time_scale);
    }

    // Toggle orbit visualization
    if keyboard.just_pressed(KeyCode::KeyO) {
        solar_params.show_orbits = !solar_params.show_orbits;
        println!(
            "🛸 Orbit visualization: {}",
            if solar_params.show_orbits {
                "ON"
            } else {
                "OFF"
            }
        );
    }

    // Quick navigation shortcuts
    if let Ok((mut controller, mut transform)) = camera_query.get_single_mut() {
        // GG (press G twice): Return to overview of entire solar system
        static mut LAST_G_PRESS: Option<std::time::Instant> = None;
        if keyboard.just_pressed(KeyCode::KeyG) {
            unsafe {
                if let Some(last_press) = LAST_G_PRESS {
                    // If pressed within 0.5 seconds, trigger action
                    if last_press.elapsed().as_secs_f32() < 0.5 {
                        transform.translation = Vec3::new(0.0, 120000.0, 1500000.0);
                        transform.look_at(Vec3::ZERO, Vec3::Y);
                        controller.velocity = Vec3::ZERO;
                        controller.speed = 5000.0;
                        println!("🏠 Returned to solar system overview (gg)");
                        LAST_G_PRESS = None;
                    } else {
                        LAST_G_PRESS = Some(std::time::Instant::now());
                    }
                } else {
                    LAST_G_PRESS = Some(std::time::Instant::now());
                }
            }
        }

        // F key: Focus on selected planet
        if keyboard.just_pressed(KeyCode::KeyF) {
            if let Some(entity) = selected_planet.entity {
                if let Ok((planet_comp, planet_transform)) = planet_query.get(entity) {
                    let planet_pos = planet_transform.translation();
                    let radius =
                        physics::calculate_visual_radius(&planet_comp.domain_planet, &solar_params);

                    // Position camera to frame the planet nicely
                    let distance = (radius * 10.0).max(5000.0).min(500000.0);
                    let offset = Vec3::new(distance * 0.7, distance * 0.5, distance * 0.7);
                    transform.translation = planet_pos + offset;
                    transform.look_at(planet_pos, Vec3::Y);
                    controller.velocity = Vec3::ZERO;

                    // Adjust speed based on planet size
                    controller.speed = (radius * 2.0).max(50.0).min(50000.0);

                    println!("🎯 Focused on {}", planet_comp.domain_planet.name);
                }
            } else {
                println!("❌ No planet selected. Click on a planet first!");
            }
        }
    }
}

// System to update camera controller based on input
pub fn update_camera_controller(
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mut mouse_motion: EventReader<MouseMotion>,
    mut mouse_wheel: EventReader<MouseWheel>,
    ui_state: Res<UiPointerState>,
    selected_planet: Res<SelectedPlanet>,
    mut query: Query<(&mut CameraController, &mut Transform)>,
    mut input_state: ResMut<CameraInputState>,
    mut notifications: ResMut<NotificationQueue>,
) {
    // Check if UI is using the mouse (hovering over UI)
    let ui_has_pointer = ui_state.is_over_ui;
    for (mut controller, mut transform) in query.iter_mut() {
        if controller.mode != CameraMode::FreeFlight {
            continue; // Only handle input for free flight mode for now
        }

        let dt = time.delta_seconds();
        let mut user_input = false;

        // Handle mouse look for rotation
        let mut mouse_delta = Vec2::ZERO;
        for motion in mouse_motion.read() {
            mouse_delta += motion.delta;
        }

        // Apply mouse sensitivity and update rotation only when left mouse is held.
        if mouse_delta != Vec2::ZERO && mouse_buttons.pressed(MouseButton::Left) {
            user_input = true;
            let sensitivity = controller.sensitivity;
            let yaw = -mouse_delta.x * sensitivity;
            let pitch = -mouse_delta.y * sensitivity;

            // Apply rotation to camera transform
            transform.rotate_y(yaw);
            let right = *transform.right();
            transform.rotate_axis(
                bevy::math::Dir3::new(right).unwrap_or(bevy::math::Dir3::X),
                pitch,
            );

            // Prevent camera from flipping upside down
            let euler = transform.rotation.to_euler(EulerRot::YXZ);
            let clamped_pitch = euler
                .1
                .clamp(-std::f32::consts::PI / 2.1, std::f32::consts::PI / 2.1);
            transform.rotation = Quat::from_euler(EulerRot::YXZ, euler.0, clamped_pitch, euler.2);
        }

        // Handle keyboard rotation (cursor keys for looking around)
        let mut rotation_delta = Vec2::ZERO;
        if keyboard.pressed(KeyCode::ArrowUp) {
            rotation_delta.y -= 1.0;
        }
        if keyboard.pressed(KeyCode::ArrowDown) {
            rotation_delta.y += 1.0;
        }
        if keyboard.pressed(KeyCode::ArrowLeft) {
            rotation_delta.x -= 1.0;
        }
        if keyboard.pressed(KeyCode::ArrowRight) {
            rotation_delta.x += 1.0;
        }

        // Apply keyboard-based rotation
        if rotation_delta != Vec2::ZERO {
            user_input = true;
            let key_sensitivity = controller.sensitivity * 50.0; // Keyboard rotation sensitivity
            let yaw = -rotation_delta.x * key_sensitivity;
            let pitch = -rotation_delta.y * key_sensitivity;

            // Apply rotation to camera transform
            transform.rotate_y(yaw);
            let right = *transform.right();
            transform.rotate_axis(
                bevy::math::Dir3::new(right).unwrap_or(bevy::math::Dir3::X),
                pitch,
            );

            // Prevent camera from flipping upside down
            let euler = transform.rotation.to_euler(EulerRot::YXZ);
            let clamped_pitch = euler
                .1
                .clamp(-std::f32::consts::PI / 2.1, std::f32::consts::PI / 2.1);
            transform.rotation = Quat::from_euler(EulerRot::YXZ, euler.0, clamped_pitch, euler.2);
        }

        // Handle keyboard movement - Full 3D spaceship-style controls
        let mut movement = Vec3::ZERO;

        // Primary movement (WASD + Space/Ctrl)
        if keyboard.pressed(KeyCode::KeyW) {
            movement += *transform.forward(); // Forward
        }
        if keyboard.pressed(KeyCode::KeyS) {
            movement -= *transform.forward(); // Backward
        }
        if keyboard.pressed(KeyCode::KeyA) {
            movement -= *transform.right(); // Strafe left
        }
        if keyboard.pressed(KeyCode::KeyD) {
            movement += *transform.right(); // Strafe right
        }

        // Vertical movement (multiple options for flexibility)
        if keyboard.pressed(KeyCode::Space) || keyboard.pressed(KeyCode::KeyQ) {
            movement += Vec3::Y; // Up
        }
        if keyboard.pressed(KeyCode::ControlLeft)
            || keyboard.pressed(KeyCode::ControlRight)
            || keyboard.pressed(KeyCode::KeyE)
        {
            movement -= Vec3::Y; // Down
        }

        // Alternative movement controls for enhanced 3D navigation
        // Arrow keys provide additional movement options
        if keyboard.pressed(KeyCode::ArrowUp) && !keyboard.pressed(KeyCode::KeyW) {
            movement += *transform.forward() * 0.7; // Slower forward with arrows
        }
        if keyboard.pressed(KeyCode::ArrowDown) && !keyboard.pressed(KeyCode::KeyS) {
            movement -= *transform.forward() * 0.7; // Slower backward with arrows
        }

        // Allow free movement in any direction by combining controls
        // This enables full 6DOF (degrees of freedom) movement

        // Additional zoom controls with keyboard for precise control
        let zoom_speed = controller.speed * 12.0; // Smoother keyboard zoom for astronomical navigation
        if keyboard.pressed(KeyCode::Equal) || keyboard.pressed(KeyCode::NumpadAdd) {
            // Zoom in with = or numpad +
            let forward = *transform.forward();
            movement += forward * zoom_speed;
        }
        if keyboard.pressed(KeyCode::Minus) || keyboard.pressed(KeyCode::NumpadSubtract) {
            // Zoom out with - or numpad -
            let forward = *transform.forward();
            movement -= forward * zoom_speed;
        }

        // Handle mouse wheel for zooming and speed adjustment (only if not over UI)
        if !ui_has_pointer {
            for wheel_event in mouse_wheel.read() {
                user_input = true;
                if keyboard.pressed(KeyCode::ControlLeft) || keyboard.pressed(KeyCode::ControlRight)
                {
                    // Ctrl+Wheel: Adjust base movement speed
                    let speed_change = wheel_event.y * controller.speed * 0.15;
                    controller.speed = (controller.speed + speed_change)
                        .clamp(controller.min_speed, controller.max_speed);
                    println!("Camera speed: {:.0} units/s", controller.speed);
                } else if keyboard.pressed(KeyCode::ShiftLeft)
                    || keyboard.pressed(KeyCode::ShiftRight)
                {
                    // Shift+Wheel: Adjust zoom sensitivity
                    let sensitivity_change = wheel_event.y * 5.0;
                    controller.zoom_sensitivity =
                        (controller.zoom_sensitivity + sensitivity_change).clamp(0.1, 500.0);

                    // Show notification with current zoom sensitivity
                    notifications.notifications.push(Notification {
                        message: format!("Zoom Sensitivity: {:.1}", controller.zoom_sensitivity),
                        notification_type: NotificationType::Info,
                        created_at: time.elapsed_seconds(),
                        duration: 2.0,
                    });
                } else {
                    // Normal wheel: Zoom in/out
                    let zoom_distance =
                        wheel_event.y * controller.speed * controller.zoom_sensitivity;
                    let forward = *transform.forward();
                    transform.translation += forward * zoom_distance;
                }
            }
        } else {
            // Clear wheel events when over UI to prevent them from being processed
            mouse_wheel.clear();
        }

        // Apply speed with multiple speed options for better 3D navigation
        if movement != Vec3::ZERO {
            movement = movement.normalize() * controller.speed;

            // Speed modifiers for flexible 3D movement
            if keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight) {
                movement *= 5.0; // Fast mode - quick travel between planets
            } else if keyboard.pressed(KeyCode::AltLeft) || keyboard.pressed(KeyCode::AltRight) {
                movement *= 0.2; // Slow mode - precise positioning near objects
            }

            // Allow free movement in any 3D direction without normalization constraints
            // This enables smooth, intuitive spaceship-like movement
        }

        // Smooth acceleration/deceleration for better control
        let target_velocity = movement;
        let accel_rate = if target_velocity.length() > controller.velocity.length() {
            controller.acceleration
        } else {
            controller.deceleration
        };

        controller.velocity = controller.velocity.lerp(target_velocity, dt * accel_rate);

        // Apply damping to stop completely when no input
        if movement == Vec3::ZERO && controller.velocity.length() < 1.0 {
            controller.velocity = Vec3::ZERO;
        }

        if movement != Vec3::ZERO {
            user_input = true;
        }

        if user_input {
            input_state.last_input_time = time.elapsed_seconds();
            if let Some(entity) = selected_planet.entity {
                input_state.suppress_auto_inspect_for = Some(entity);
            }
        }
    }
}

// System to apply camera transformations based on controller state
pub fn apply_camera_transform(
    time: Res<Time>,
    mut query: Query<(&mut CameraController, &mut Transform)>,
) {
    for (mut controller, mut transform) in query.iter_mut() {
        match controller.mode {
            CameraMode::FreeFlight => {
                // Apply velocity to position (rotation is handled in input system for mouse look)
                let dt = time.delta_seconds();
                transform.translation += controller.velocity * dt;
            }
            CameraMode::Orbit => {
                // Orbit around the solar system center
                controller.orbit_angle += time.delta_seconds() * 0.5;
                let orbit_pos = Vec3::new(
                    controller.orbit_distance * controller.orbit_angle.cos(),
                    10.0, // Slight elevation
                    controller.orbit_distance * controller.orbit_angle.sin(),
                );
                transform.translation = orbit_pos;
                transform.look_at(Vec3::ZERO, Vec3::Y);
            }
            CameraMode::FollowPlanet => {
                // Follow a specific planet (placeholder)
                // Would need to track the target entity's position
            }
            CameraMode::ApproachPlanet => {
                // Approach a planet (placeholder)
                // Would smoothly interpolate toward target
            }
        }
    }
}

// System to auto-inspect a selected planet by smoothly framing it at a readable distance
pub fn auto_inspect_selected_planet(
    time: Res<Time>,
    solar_params: Res<SolarSystemParameters>,
    selected_planet: Res<SelectedPlanet>,
    mut input_state: ResMut<CameraInputState>,
    mut camera_query: Query<(&CameraController, &mut Transform, &Projection)>,
    planet_query: Query<(&PlanetComponent, &GlobalTransform)>,
    mut state: Local<AutoInspectState>,
) {
    let selected_entity = match selected_planet.entity {
        Some(entity) => entity,
        None => return,
    };

    let (planet_comp, planet_transform) = match planet_query.get(selected_entity) {
        Ok(data) => data,
        Err(_) => return,
    };

    let (controller, mut camera_transform, projection) = match camera_query.get_single_mut() {
        Ok(data) => data,
        Err(_) => return,
    };

    // if controller.mode != CameraMode::FreeFlight {
    //     return;
    // }

    if input_state.last_selected_entity != Some(selected_entity) {
        input_state.last_selected_entity = Some(selected_entity);
        input_state.suppress_auto_inspect_for = None;
    }

    if input_state.suppress_auto_inspect_for == Some(selected_entity) {
        return;
    }

    let planet_radius = if planet_comp.domain_planet.name == "Sun" {
        physics::calculate_sun_visual_radius(&solar_params)
    } else {
        physics::calculate_visual_radius(&planet_comp.domain_planet, &solar_params)
    };

    let planet_pos = planet_transform.translation();
    let mut focus_point = planet_pos;
    let mut target_distance = planet_radius * 5.0;
    let mut moon_axis: Option<Vec3> = None;
    let mut moon_up: Option<Vec3> = None;
    let mut moon_distance: Option<f32> = None;
    let is_moon = planet_comp.domain_planet.parent_entity.is_some();

    let fov_y = match projection {
        Projection::Perspective(perspective) => perspective.fov,
        Projection::Orthographic(_) => std::f32::consts::FRAC_PI_2,
    };
    let fit_radius = |radius: f32, fill: f32| -> f32 {
        let half_fov = (fov_y * 0.5 * fill).max(0.05);
        radius / half_fov.sin()
    };

    if let Some(parent_name) = &planet_comp.domain_planet.parent_entity {
        for (other_comp, other_transform) in planet_query.iter() {
            if other_comp.domain_planet.name == *parent_name {
                let parent_pos = other_transform.translation();
                let axis = parent_pos - planet_pos;
                let axis_dir = if axis.length_squared() > 0.0 {
                    axis.normalize()
                } else {
                    Vec3::Z
                };
                let mut lateral = axis_dir.cross(Vec3::Y);
                if lateral.length_squared() < 1e-4 {
                    lateral = axis_dir.cross(Vec3::X);
                }
                let lateral = lateral.normalize();
                let up = lateral.cross(axis_dir).normalize();

                moon_axis = Some(axis_dir);
                moon_up = Some(up);

                let parent_radius = if other_comp.domain_planet.name == "Sun" {
                    physics::calculate_sun_visual_radius(&solar_params)
                } else {
                    physics::calculate_visual_radius(&other_comp.domain_planet, &solar_params)
                };
                let size_ratio = (parent_radius / planet_radius).clamp(1.2, 50.0);
                let fill = (0.78 - size_ratio.log10() * 0.04).clamp(0.62, 0.78);
                let desired_distance = fit_radius(planet_radius, fill);
                let min_distance = (planet_radius * 3.2).max(120.0);
                target_distance = desired_distance.max(min_distance);
                moon_distance = Some(target_distance);
                break;
            }
        }
    }

    // Initialize or reset state when selection changes
    if state.selected != Some(selected_entity) {
        state.selected = Some(selected_entity);
        state.orbit_angle = 0.0;
        // Start with a nice 3/4 view angle
        state.orbit_elevation = 0.3; // 30 degrees up
        state.smooth_axis = Vec3::ZERO;
        state.smooth_up = Vec3::ZERO;
        state.smooth_focus = Vec3::ZERO;
        state.smooth_offset = Vec3::ZERO;
    }

    // Cinematic slow orbit around the planet for aesthetic viewing
    if !is_moon {
        state.orbit_angle += time.delta_seconds() * 0.15; // Slow orbit
    }

    if let (Some(axis_dir), Some(up)) = (moon_axis, moon_up) {
        // Frame the moon large in the foreground with the parent in the background.
        let distance = moon_distance.unwrap_or(target_distance);
        let smooth_lerp = 1.0 - (-3.0 * time.delta_seconds()).exp();
        state.smooth_axis = if state.smooth_axis.length_squared() > 0.0 {
            state
                .smooth_axis
                .lerp(axis_dir, smooth_lerp)
                .normalize_or_zero()
        } else {
            axis_dir
        };
        state.smooth_up = if state.smooth_up.length_squared() > 0.0 {
            state.smooth_up.lerp(up, smooth_lerp).normalize_or_zero()
        } else {
            up
        };

        let smooth_axis = state.smooth_axis;
        let smooth_up = state.smooth_up;
        let mut smooth_lateral = smooth_axis.cross(Vec3::Y);
        if smooth_lateral.length_squared() < 1e-4 {
            smooth_lateral = smooth_axis.cross(Vec3::X);
        }
        let smooth_lateral = smooth_lateral.normalize();

        let rotation = Quat::from_axis_angle(smooth_axis, state.orbit_angle * 0.06);
        let side_offset =
            rotation * (smooth_up * (distance * 0.35) + smooth_lateral * (distance * 0.12));
        state.offset = (-smooth_axis * distance) + side_offset;
        focus_point = planet_pos;
    } else {
        // Get aesthetic viewing angle based on planet type
        let (orbit_distance, elevation_offset) =
            get_aesthetic_view_params(&planet_comp.domain_planet.name);
        let actual_distance = target_distance * orbit_distance;
        let elevation = state.orbit_elevation + elevation_offset;

        // Calculate cinematic orbit position (3/4 view with elevation)
        let horizontal = Vec3::new(
            state.orbit_angle.cos() * actual_distance,
            0.0,
            state.orbit_angle.sin() * actual_distance,
        );
        let elevated = horizontal + Vec3::Y * (actual_distance * elevation);
        state.offset = elevated;
    }

    let smooth_factor = if is_moon {
        1.0 - (-2.5 * time.delta_seconds()).exp()
    } else {
        1.0 - (-4.0 * time.delta_seconds()).exp()
    };
    if state.smooth_focus.length_squared() > 0.0 {
        state.smooth_focus = state.smooth_focus.lerp(focus_point, smooth_factor);
    } else {
        state.smooth_focus = focus_point;
    }
    if state.smooth_offset.length_squared() > 0.0 {
        state.smooth_offset = state.smooth_offset.lerp(state.offset, smooth_factor);
    } else {
        state.smooth_offset = state.offset;
    }

    let target_pos = state.smooth_focus + state.smooth_offset;
    let lerp_factor = 1.0 - (-2.5 * time.delta_seconds()).exp(); // Smoother transitions
    camera_transform.translation = camera_transform.translation.lerp(target_pos, lerp_factor);

    // Look at the focus point to frame moon + parent when applicable
    camera_transform.look_at(state.smooth_focus, Vec3::Y);
}

// Get aesthetic viewing parameters for different celestial bodies
fn get_aesthetic_view_params(name: &str) -> (f32, f32) {
    // Returns (distance_multiplier, elevation_offset)
    match name {
        "Sun" => (1.2, 0.2),      // Further back, slightly elevated for the massive sun
        "Saturn" => (1.15, 0.3),  // Extra elevation to showcase rings
        "Jupiter" => (1.1, 0.25), // Show off the gas giant with nice elevation
        "Earth" | "Mars" => (0.95, 0.35), // Closer, higher angle for detail
        "Moon" | "Io" | "Europa" | "Titan" | "Triton" => (0.95, 0.35), // Keep close while showing parent
        "Neptune" | "Uranus" => (1.0, 0.3), // Ice giants with good elevation
        _ => (1.0, 0.3),                    // Default: standard distance, nice 3/4 view
    }
}

// Performance monitoring and automatic quality adjustment system
/// PRODUCTION-GRADE FPS MEASUREMENT (Industry Standard Implementation)
/// Correctly measures frame time first, then derives FPS from it.
/// Uses exponential moving average for stability and responsiveness.
pub fn update_performance_stats(
    _time: Res<Time>,
    mut performance_stats: ResMut<PerformanceStats>,
    mut solar_params: ResMut<SolarSystemParameters>,
    chrome: Option<Res<ChromeOptimizations>>,
) {
    // PRODUCTION-GRADE FRAME TIME MEASUREMENT
    // Use high-resolution monotonic clock for accurate timing
    let now = std::time::Instant::now();

    // Calculate frame time as difference from last frame
    let frame_time_seconds = if performance_stats.frame_count > 0 {
        now.duration_since(performance_stats.last_frame_time)
            .as_secs_f64()
    } else {
        // First frame - use target frame time as estimate
        1.0 / performance_stats.target_fps as f64
    };

    performance_stats.last_frame_time = now;

    // Convert to milliseconds (industry standard unit)
    let frame_time_ms = (frame_time_seconds * 1000.0) as f32;

    // PRIMARY METRIC: Frame time (this is the truth)
    performance_stats.frame_time_ms = frame_time_ms;

    // DERIVED METRIC: Raw FPS = 1/frame_time (jumps violently, not for display)
    performance_stats.fps_raw = if frame_time_ms > 0.0 {
        1000.0 / frame_time_ms
    } else {
        0.0 // Prevent division by zero
    };

    // EXPONENTIAL MOVING AVERAGE (Industry Standard Smoothing)
    // fps_smoothed = fps_smoothed * 0.9 + fps_raw * 0.1
    // - Stable: Doesn't jump around
    // - Responsive: Reacts to changes quickly
    // - Cheap: Single multiplication per frame
    const SMOOTHING_FACTOR: f32 = 0.1; // 0.1 = 10% new data, 90% history
    performance_stats.fps_smoothed = performance_stats.fps_smoothed * (1.0 - SMOOTHING_FACTOR)
        + performance_stats.fps_raw * SMOOTHING_FACTOR;

    // Frame time EMA (more stable than FPS for performance analysis)
    performance_stats.frame_time_smoothed = performance_stats.frame_time_smoothed
        * (1.0 - SMOOTHING_FACTOR)
        + performance_stats.frame_time_ms * SMOOTHING_FACTOR;

    // DISPLAY FPS (what users see - smoothed for human consumption)
    performance_stats.fps_display = performance_stats.fps_smoothed;

    // FRAME TIME STATISTICS (most important for performance analysis)
    // Update min/max frame times
    performance_stats.frame_time_min = performance_stats.frame_time_min.min(frame_time_ms);
    performance_stats.frame_time_max = performance_stats.frame_time_max.max(frame_time_ms);

    // Maintain frame time history for percentile calculations
    performance_stats.frame_time_history.push(frame_time_ms);
    if performance_stats.frame_time_history.len() > performance_stats.history_capacity {
        performance_stats.frame_time_history.remove(0); // Remove oldest
    }

    // Calculate 99th percentile frame time (stutter detection)
    if !performance_stats.frame_time_history.is_empty() {
        let mut sorted_times = performance_stats.frame_time_history.clone();
        sorted_times.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let percentile_index = ((sorted_times.len() - 1) as f32 * 0.99) as usize;
        performance_stats.frame_time_99th =
            sorted_times[percentile_index.min(sorted_times.len() - 1)];
    }

    // GPU TIMING (when available - Vulkan/WebGPU)
    // For now, assume GPU time ≈ CPU time (simplified)
    // TODO: Add actual GPU timestamp queries for Vulkan/WebGPU
    performance_stats.gpu_frame_time_ms = frame_time_ms; // Placeholder
    performance_stats.cpu_gpu_frame_time = frame_time_ms.max(performance_stats.gpu_frame_time_ms);

    // LEGACY COMPATIBILITY (deprecated fields)
    performance_stats.frame_time = performance_stats.frame_time_ms;
    performance_stats.fps = performance_stats.fps_display;
    performance_stats.average_frame_time = performance_stats.frame_time_smoothed;
    performance_stats.average_fps = performance_stats.fps_smoothed;

    // Update frame count
    performance_stats.frame_count += 1;

    // Chrome detection for adaptive rate adjustment
    if let Some(chrome) = chrome {
        performance_stats.adaptation_rate = if chrome.is_chrome { 0.05 } else { 0.1 };
    }

    // LEGACY: Maintain old rolling average for compatibility
    let fps_raw_copy = performance_stats.fps_raw; // Copy before mutable borrow ends
    performance_stats.frame_history.push_back(fps_raw_copy);
    if performance_stats.frame_history.len() > performance_stats.history_len {
        performance_stats.frame_history.pop_front();
    }

    // AUTOMATIC QUALITY ADJUSTMENT (based on frame time, not FPS)
    if performance_stats.adaptive_enabled {
        adjust_quality_based_on_performance(&mut performance_stats, &mut solar_params);
    }
}

pub fn update_dynamic_resolution(
    time: Res<Time>,
    performance_stats: Res<PerformanceStats>,
    mut state: ResMut<DynamicResolutionState>,
    mut windows: Query<&mut Window>,
) {
    if state.cooldown > 0.0 {
        state.cooldown -= time.delta_seconds();
        return;
    }

    let avg_fps = if performance_stats.average_frame_time > 0.0 {
        1000.0 / performance_stats.average_frame_time
    } else {
        performance_stats.fps
    };
    let target = performance_stats.target_fps;
    let mut new_scale = state.scale;

    if avg_fps < target - 5.0 {
        new_scale = (state.scale - 0.05).max(state.min_scale);
    } else if avg_fps > target + 8.0 {
        new_scale = (state.scale + 0.05).min(state.max_scale);
    }

    if (new_scale - state.scale).abs() < f32::EPSILON {
        return;
    }

    if let Ok(mut window) = windows.get_single_mut() {
        let base_scale = window.resolution.base_scale_factor();
        window
            .resolution
            .set_scale_factor_override(Some(base_scale * new_scale));
    }

    state.scale = new_scale;
    state.cooldown = 0.5;
}

pub fn apply_initial_dynamic_resolution(
    state: Res<DynamicResolutionState>,
    mut windows: Query<&mut Window>,
) {
    if let Ok(mut window) = windows.get_single_mut() {
        let base_scale = window.resolution.base_scale_factor();
        window
            .resolution
            .set_scale_factor_override(Some(base_scale * state.scale));
    }
}

pub fn cap_fixed_overstep(mut fixed_time: ResMut<Time<Fixed>>) {
    let max_overstep =
        std::time::Duration::from_secs_f32(fixed_time.timestep().as_secs_f32() * 2.0);
    let overstep = fixed_time.overstep();
    if overstep > max_overstep {
        fixed_time.discard_overstep(overstep - max_overstep);
    }
}

// Automatic quality adjustment based on performance metrics
fn adjust_quality_based_on_performance(
    performance_stats: &mut PerformanceStats,
    solar_params: &mut SolarSystemParameters,
) {
    let target_fps = performance_stats.target_fps.max(1.0);
    let avg_fps = performance_stats.average_fps;
    let rate = performance_stats.adaptation_rate;

    let mut new_quality_level = performance_stats.quality_level;
    if avg_fps < target_fps * (1.0 - rate) {
        new_quality_level = match performance_stats.quality_level {
            QualityLevel::Ultra => QualityLevel::High,
            QualityLevel::High => QualityLevel::Medium,
            QualityLevel::Medium => QualityLevel::Low,
            QualityLevel::Low => QualityLevel::Minimal,
            QualityLevel::Minimal => QualityLevel::Minimal,
        };
    } else if avg_fps > target_fps * (1.0 + rate) {
        new_quality_level = match performance_stats.quality_level {
            QualityLevel::Ultra => QualityLevel::Ultra,
            QualityLevel::High => QualityLevel::Ultra,
            QualityLevel::Medium => QualityLevel::High,
            QualityLevel::Low => QualityLevel::Medium,
            QualityLevel::Minimal => QualityLevel::Low,
        };
    }

    if new_quality_level != performance_stats.quality_level {
        performance_stats.quality_level = new_quality_level;
        apply_quality_settings(new_quality_level, solar_params);
    }
}

// Apply quality settings based on the quality level
fn apply_quality_settings(quality_level: QualityLevel, solar_params: &mut SolarSystemParameters) {
    match quality_level {
        QualityLevel::Ultra => {
            // Maximum quality - no performance optimizations
            solar_params.time_scale = 1.0;
            println!("🎯 Performance excellent - Quality set to Ultra");
        }
        QualityLevel::High => {
            // High quality with minimal optimizations
            solar_params.time_scale = 1.0;
            println!("✅ Performance good - Quality set to High");
        }
        QualityLevel::Medium => {
            // Balanced quality and performance
            solar_params.time_scale = 0.8;
            println!("⚖️ Performance moderate - Quality set to Medium");
        }
        QualityLevel::Low => {
            // Lower quality for better performance
            solar_params.time_scale = 0.5;
            println!("⚡ Performance low - Quality set to Low (slower time)");
        }
        QualityLevel::Minimal => {
            // Minimum quality for maximum performance
            solar_params.time_scale = 0.2;
            println!("🚀 Performance critical - Quality set to Minimal (maximum time scaling)");
        }
    }
}

#[derive(Default)]
pub struct AutoInspectState {
    selected: Option<Entity>,
    offset: Vec3,
    orbit_angle: f32,     // Cinematic orbit angle
    orbit_elevation: f32, // Vertical orbit component
    smooth_axis: Vec3,
    smooth_up: Vec3,
    smooth_focus: Vec3,
    smooth_offset: Vec3,
}

// System to capture screenshot on next frame after notifications are hidden
pub fn take_pending_screenshot(
    mut screenshot_state: ResMut<ScreenshotState>,
    mut screenshot_manager: ResMut<bevy::render::view::screenshot::ScreenshotManager>,
    main_window: Query<Entity, With<bevy::window::PrimaryWindow>>,
    mut notifications: ResMut<NotificationQueue>,
    time: Res<Time>,
) {
    if !screenshot_state.pending {
        return;
    }

    screenshot_state.pending = false;

    let window_entity = main_window.single();

    // Create screenshots directory in home folder
    let home_dir = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let screenshots_dir = format!("{}/cosmic_systems_images", home_dir);

    if let Err(e) = std::fs::create_dir_all(&screenshots_dir) {
        notifications.notifications.push(Notification {
            message: format!("Failed to create screenshots directory: {}", e),
            notification_type: NotificationType::Error,
            created_at: time.elapsed_seconds(),
            duration: 5.0,
        });
        return;
    }

    // Generate filename with timestamp
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let filename = format!("{}/cosmic_systems_{}.png", screenshots_dir, timestamp);

    // Take screenshot using Bevy's screenshot API
    match screenshot_manager.save_screenshot_to_disk(window_entity, filename.clone()) {
        Ok(_) => {
            notifications.notifications.push(Notification {
                message: format!("Screenshot saved to: {}", filename),
                notification_type: NotificationType::Success,
                created_at: time.elapsed_seconds(),
                duration: 4.0,
            });
        }
        Err(e) => {
            notifications.notifications.push(Notification {
                message: format!("Failed to save screenshot: {}", e),
                notification_type: NotificationType::Error,
                created_at: time.elapsed_seconds(),
                duration: 5.0,
            });
        }
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

    println!("🔥 init_vulkan_solver: Starting Vulkan compute initialization...");
    perf_stats.vulkan_initialized = true;
    println!("🔥 init_vulkan_solver: Attempting Vulkan compute initialization...");

    // Try to initialize Vulkan solver
    match init_vulkan_compute() {
        Ok(solver) => {
            perf_stats.vulkan_solver = Some(solver);
            perf_stats.vulkan_enabled = true;
            println!("✅ init_vulkan_solver: Vulkan compute solver initialized successfully - GPU acceleration active!");
        }
        Err(e) => {
            perf_stats.vulkan_enabled = false;
            println!("❌ init_vulkan_solver: Vulkan compute initialization failed (continuing with CPU SIMD): {}", e);
        }
    }
}

/// Initialize Vulkan compute pipeline
#[cfg(all(not(target_arch = "wasm32"), feature = "ash"))]
fn init_vulkan_compute() -> Result<
    crate::infrastructure::gpu_compute::vulkan_kepler::VulkanKeplerSolver,
    Box<dyn std::error::Error>,
> {
    // Vulkan temporarily disabled due to syntax issues
    Err("Vulkan GPU acceleration temporarily disabled".into())
}

/// Advanced quality adaptation system
#[derive(Resource)]
pub struct QualityAdaptationResource {
    pub system: crate::infrastructure::bevy_adapters::components::QualityAdaptationSystem,
    pub enabled: bool,
}

impl Default for QualityAdaptationResource {
    fn default() -> Self {
        Self {
            system: crate::infrastructure::bevy_adapters::components::QualityAdaptationSystem::new(
                60.0,
            ),
            enabled: true,
        }
    }
}

/// System for adaptive quality control
pub fn adaptive_quality_system(
    mut perf_stats: ResMut<PerformanceStats>,
    mut quality_adapter: ResMut<QualityAdaptationResource>,
) {
    if !quality_adapter.enabled {
        return;
    }

    // Update frame time history for variance calculation
    let frame_time_ms = perf_stats.frame_time_ms;
    perf_stats.frame_time_history.push(frame_time_ms);
    if perf_stats.frame_time_history.len() > perf_stats.history_capacity {
        perf_stats.frame_time_history.remove(0);
    }

    // Run quality adaptation
    if let Some(new_quality) = quality_adapter.system.update_and_adapt(&mut perf_stats) {
        perf_stats.quality_level = new_quality;
        println!("🎚️ Quality adapted to: {:?}", new_quality);
    }

    // Log adaptation status periodically
    static mut LAST_LOG: Option<std::time::Instant> = None;
    let now = std::time::Instant::now();
    unsafe {
        let should_log = LAST_LOG
            .map(|last| now.duration_since(last).as_millis() > 5000)
            .unwrap_or(true);
        if should_log {
            // Log every 5 seconds
            println!("🎯 Quality Adaptation Status:");
            println!("   Current Quality: {:?}", perf_stats.quality_level);
            println!("   FPS: {:.1}", perf_stats.fps_display);
            println!("   GPU Util: {:.1}%", perf_stats.gpu_utilization * 100.0);
            println!("   CPU Util: {:.1}%", perf_stats.cpu_utilization * 100.0);
            println!(
                "   Mem Pressure: {:.1}%",
                perf_stats.memory_pressure * 100.0
            );
            println!("   Trend: {:?}", perf_stats.quality_trend);
            println!("   Confidence: {:.2}", perf_stats.adaptive_confidence);
            LAST_LOG = Some(now);
        }
    }
}

/// PRODUCTION-GRADE PERFORMANCE LOGGING (Industry Standards)
/// Displays frame time (truth) and FPS (derived) with 99th percentile stutter detection
pub fn log_performance_stats(perf_stats: Res<PerformanceStats>, _time: Res<Time>) {
    // Log performance stats every 60 frames for benchmarking
    if perf_stats.frame_count % 60 == 0 {
        // PRIMARY DISPLAY: Frame time and FPS (industry standard format)
        // Shows both the truth (frame time) and human metric (FPS)
        println!("🎯 PERF_STATS: FPS: {:.1} | Frame: {:.1}ms | 99%: {:.1}ms | Min: {:.1}ms | Max: {:.1}ms",
            perf_stats.fps_display,      // Smoothed FPS for human consumption
            perf_stats.frame_time_ms,    // Current frame time (the truth)
            perf_stats.frame_time_99th,  // 99th percentile (stutter detection)
            perf_stats.frame_time_min,   // Best case
            perf_stats.frame_time_max    // Worst case
        );

        // GPU TIMING (when available)
        if perf_stats.gpu_frame_time_ms > 0.0 {
            println!(
                "🎮 GPU_TIMING: CPU: {:.1}ms | GPU: {:.1}ms | Combined: {:.1}ms",
                perf_stats.frame_time_ms,
                perf_stats.gpu_frame_time_ms,
                perf_stats.cpu_gpu_frame_time
            );
        }

        // PHYSICS PERFORMANCE BREAKDOWN
        println!(
            "⚛️  PHYSICS: update={:.2}ms kepler={:.2}ms vulkan_calls={} adaptive_calls={}",
            perf_stats.physics_update_time,
            perf_stats.kepler_solve_time,
            perf_stats.vulkan_kepler_calls,
            perf_stats.adaptive_kepler_calls
        );

        // COMPUTE BACKEND STATUS
        let backend_status = if perf_stats.vulkan_enabled {
            "Vulkan GPU + SIMD"
        } else {
            "SIMD CPU Only"
        };
        println!(
            "🖥️  COMPUTE: {} | SIMD: {} | Parallel: {} | Cores: {}",
            backend_status,
            perf_stats.simd_enabled,
            perf_stats.parallel_enabled,
            perf_stats.cpu_cores_used
        );

        // QUALITY AND ADAPTATION
        println!(
            "🎚️  QUALITY: {:?} | Adaptive: {} | Target: {:.0} FPS",
            perf_stats.quality_level, perf_stats.adaptive_enabled, perf_stats.target_fps
        );

        // MEMORY USAGE
        println!(
            "💾 MEMORY: {:.1}MB current | {:.1}MB peak",
            perf_stats.memory_usage_mb, perf_stats.peak_memory_mb
        );

        // RAW METRICS (for debugging - not for end users)
        if perf_stats.frame_count % 300 == 0 {
            // Every 5 seconds
            println!(
                "🔍 RAW_METRICS: fps_raw={:.1} fps_smoothed={:.1} frame_time_smoothed={:.1}ms",
                perf_stats.fps_raw, perf_stats.fps_smoothed, perf_stats.frame_time_smoothed
            );
        }
    }
}
