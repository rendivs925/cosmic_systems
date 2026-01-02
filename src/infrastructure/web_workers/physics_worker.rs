use crate::infrastructure::bevy_adapters::components::PerformanceStats;
use bevy::prelude::{Entity, NonSendMut, Res, Vec3};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{MessageEvent, Worker};

/// Task for background physics processing
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct PhysicsTask {
    pub worker_id: usize,
    pub entity_bits: u64,
    pub orbital_elements: OrbitalElements,
    pub has_elements: bool,
    pub is_moon: bool,
    pub parent_position: WorkerVec3,
    pub parent_tilt_deg: Option<f32>,
    pub orbital_distance_au: f32,
    pub orbital_period_days: f32,
    pub time_days: f32,
    pub kepler_iterations: u32,
    pub scale_factor: f32,
    pub moon_orbit_scale: f32,
}

/// Simplified orbital elements for worker communication
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct OrbitalElements {
    pub semi_major_axis_au: f32,
    pub eccentricity: f32,
    pub inclination_rad: f32,
    pub long_asc_node_rad: f32,
    pub arg_periapsis_rad: f32,
    pub mean_anomaly_rad: f32,
}

/// Physics worker pool for background processing
pub struct PhysicsWorkerPool {
    workers: Vec<Worker>,
    available_workers: VecDeque<usize>,
    task_queue: VecDeque<PhysicsTask>,
    results: Rc<RefCell<VecDeque<WorkerResult>>>,
    callbacks: Vec<Closure<dyn FnMut(MessageEvent)>>,
    min_workers: usize,
    max_workers: usize,
    max_queue_len: usize,
}

#[derive(Clone, Debug)]
pub struct WorkerResult {
    pub entity: Entity,
    pub position: Vec3,
    pub worker_id: usize,
}

#[derive(Clone, Debug, serde::Deserialize)]
struct WorkerResultMessage {
    worker_id: usize,
    entity_bits: u64,
    position: WorkerVec3,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct WorkerVec3 {
    x: f32,
    y: f32,
    z: f32,
}

impl PhysicsWorkerPool {
    pub fn new(num_workers: usize) -> Self {
        Self::with_bounds(num_workers, 2, 8)
    }

    pub fn new_dynamic() -> Self {
        let hardware_concurrency = Self::hardware_concurrency();
        let initial_workers = (hardware_concurrency * 2).min(8).max(2);
        Self::with_bounds(initial_workers, 2, 8)
    }

    pub fn optimal_worker_count() -> usize {
        let hardware_concurrency = Self::hardware_concurrency();
        (hardware_concurrency * 2).min(8).max(2)
    }

    fn with_bounds(initial_workers: usize, min_workers: usize, max_workers: usize) -> Self {
        let initial_workers = initial_workers.min(max_workers).max(min_workers);
        let results = Rc::new(RefCell::new(VecDeque::new()));
        let max_queue_len = initial_workers.saturating_mul(4).max(8);
        let mut pool = Self {
            workers: Vec::new(),
            available_workers: VecDeque::new(),
            task_queue: VecDeque::new(),
            results,
            callbacks: Vec::new(),
            min_workers,
            max_workers,
            max_queue_len,
        };

        for _ in 0..initial_workers {
            pool.add_worker();
        }

        pool
    }

    fn hardware_concurrency() -> usize {
        web_sys::window()
            .map(|window| window.navigator().hardware_concurrency() as usize)
            .unwrap_or(4)
    }

    fn create_worker() -> Result<Worker, JsValue> {
        // Create a web worker from inline script
        let script = r#"
            const TAU = Math.PI * 2.0;
            const GAUSS_K = 0.01720209895;

            self.onmessage = function(e) {
                const task = e.data;
                const position = calculate_orbit_position(task);
                self.postMessage({
                    worker_id: task.worker_id,
                    entity_bits: task.entity_bits,
                    position: position
                });
            };

            function calculate_orbit_position(task) {
                let relative = { x: 0, y: 0, z: 0 };

                if (task.has_elements) {
                    const elements = task.orbital_elements;
                    const meanMotion = task.is_moon
                        ? TAU / task.orbital_period_days
                        : GAUSS_K / Math.pow(elements.semi_major_axis_au, 1.5);
                    const meanAnomaly = normalize_radians(
                        elements.mean_anomaly_rad + meanMotion * task.time_days
                    );
                    const eccentricAnomaly = solve_kepler_adaptive(
                        meanAnomaly,
                        elements.eccentricity,
                        task.kepler_iterations
                    );
                    const trueAnomaly = true_anomaly(eccentricAnomaly, elements.eccentricity);
                    const radius_au =
                        elements.semi_major_axis_au *
                        (1.0 - elements.eccentricity * Math.cos(eccentricAnomaly));
                    let radius = radius_au * task.scale_factor;
                    if (task.is_moon) {
                        radius *= task.moon_orbit_scale;
                    }
                    const xOrb = radius * Math.cos(trueAnomaly);
                    const zOrb = radius * Math.sin(trueAnomaly);
                    relative = transform_orbital_point(
                        xOrb,
                        zOrb,
                        elements.inclination_rad,
                        elements.long_asc_node_rad,
                        elements.arg_periapsis_rad
                    );
                } else if (task.orbital_period_days > 0.0) {
                    let radius = task.orbital_distance_au * task.scale_factor;
                    if (task.is_moon) {
                        radius *= task.moon_orbit_scale;
                    }
                    const angle = TAU * (task.time_days / task.orbital_period_days);
                    const xOrb = radius * Math.cos(angle);
                    const zOrb = radius * Math.sin(angle);
                    relative = transform_orbital_point(xOrb, zOrb, 0.0, 0.0, 0.0);
                }

                if (task.is_moon && task.parent_tilt_deg !== null) {
                    const tilt = task.parent_tilt_deg * Math.PI / 180.0;
                    const cosT = Math.cos(tilt);
                    const sinT = Math.sin(tilt);
                    const x = relative.x * cosT - relative.y * sinT;
                    const y = relative.x * sinT + relative.y * cosT;
                    relative.x = x;
                    relative.y = y;
                }

                return {
                    x: task.parent_position.x + relative.x,
                    y: task.parent_position.y + relative.y,
                    z: task.parent_position.z + relative.z
                };
            }

            function solve_kepler_adaptive(meanAnomaly, eccentricity, maxIterations) {
                let eccentricAnomaly = meanAnomaly;
                for (let i = 0; i < maxIterations; i++) {
                    const f = eccentricAnomaly - eccentricity * Math.sin(eccentricAnomaly) - meanAnomaly;
                    const fPrime = 1.0 - eccentricity * Math.cos(eccentricAnomaly);
                    eccentricAnomaly -= f / fPrime;
                }
                return eccentricAnomaly;
            }

            function true_anomaly(eccentricAnomaly, eccentricity) {
                const sinV = Math.sqrt(1.0 - eccentricity * eccentricity) * Math.sin(eccentricAnomaly)
                    / (1.0 - eccentricity * Math.cos(eccentricAnomaly));
                const cosV = (Math.cos(eccentricAnomaly) - eccentricity)
                    / (1.0 - eccentricity * Math.cos(eccentricAnomaly));
                return Math.atan2(sinV, cosV);
            }

            function normalize_radians(angle) {
                return ((angle % TAU) + TAU) % TAU;
            }

            function transform_orbital_point(xOrbital, zOrbital, inclination, longAscNode, argPeriapsis) {
                const cosW = Math.cos(argPeriapsis);
                const sinW = Math.sin(argPeriapsis);
                const x1 = xOrbital * cosW - zOrbital * sinW;
                const z1 = xOrbital * sinW + zOrbital * cosW;

                const cosI = Math.cos(inclination);
                const sinI = Math.sin(inclination);
                const y2 = z1 * sinI;
                const z2 = z1 * cosI;
                const x2 = x1;

                const cosOmega = Math.cos(longAscNode);
                const sinOmega = Math.sin(longAscNode);
                const x3 = x2 * cosOmega - z2 * sinOmega;
                const z3 = x2 * sinOmega + z2 * cosOmega;

                return { x: x3, y: y2, z: z3 };
            }
        "#;

        let blob_parts = js_sys::Array::of1(&JsValue::from_str(script));
        let options = {
            let options = web_sys::BlobPropertyBag::new();
            options.set_type("application/javascript");
            options
        };

        let blob = web_sys::Blob::new_with_str_sequence_and_options(&blob_parts.into(), &options)?;

        let url = web_sys::Url::create_object_url_with_blob(&blob)?;
        Worker::new(&url)
    }

    fn add_worker(&mut self) {
        let worker_id = self.workers.len();
        let results = Rc::clone(&self.results);
        let callback = Closure::<dyn FnMut(MessageEvent)>::wrap(Box::new(move |event| {
            let message: WorkerResultMessage = match serde_wasm_bindgen::from_value(event.data()) {
                Ok(message) => message,
                Err(_) => return,
            };
            let entity = Entity::from_bits(message.entity_bits);
            let position = Vec3::new(
                message.position.x,
                message.position.y,
                message.position.z,
            );

            results
                .borrow_mut()
                .push_back(WorkerResult { entity, position, worker_id: message.worker_id });
        }));

        let worker = match Self::create_worker() {
            Ok(worker) => worker,
            Err(err) => {
                web_sys::console::error_1(&err);
                web_sys::console::log_1(&"Failed to create worker".into());
                return;
            }
        };

        worker.set_onmessage(Some(callback.as_ref().unchecked_ref()));
        self.workers.push(worker);
        self.available_workers.push_back(worker_id);
        self.callbacks.push(callback);
        self.max_queue_len = self.workers.len().saturating_mul(4).max(8);
    }

    pub fn queue_tasks(&mut self, tasks: Vec<PhysicsTask>) {
        for task in tasks {
            if self.task_queue.len() >= self.max_queue_len {
                break;
            }
            self.task_queue.push_back(task);
        }
        self.dispatch_tasks();
    }

    pub fn dispatch_tasks(&mut self) {
        while let (Some(worker_idx), Some(task)) = (
            self.available_workers.pop_front(),
            self.task_queue.pop_front(),
        ) {
            if let Some(worker) = self.workers.get(worker_idx) {
                let mut task = task;
                task.worker_id = worker_idx;
                if let Ok(message) = serde_wasm_bindgen::to_value(&task) {
                    if worker.post_message(&message).is_err() {
                        self.available_workers.push_back(worker_idx);
                    }
                } else {
                    self.available_workers.push_back(worker_idx);
                }
            }
        }
    }

    pub fn collect_results(&mut self) -> Vec<WorkerResult> {
        // Collect completed results from workers
        // In practice, this would be called from message event handlers
        let mut results = Vec::new();
        while let Some(result) = self.results.borrow_mut().pop_front() {
            if !self.available_workers.contains(&result.worker_id) {
                self.available_workers.push_back(result.worker_id);
            }
            results.push(result);
        }
        results
    }

    pub fn worker_count(&self) -> usize {
        self.workers.len()
    }

    pub fn has_available_workers(&self) -> bool {
        !self.available_workers.is_empty()
    }

    pub fn can_accept_tasks(&self) -> bool {
        self.task_queue.len() < self.max_queue_len
    }

    pub fn adapt_worker_count(&mut self, current_fps: f32, target_fps: f32) {
        if current_fps < target_fps * 0.8 && self.workers.len() > self.min_workers {
            let last_id = self.workers.len().saturating_sub(1);
            if self.available_workers.contains(&last_id) && self.task_queue.is_empty() {
                if let Some(worker) = self.workers.pop() {
                    worker.terminate();
                }
                self.available_workers.retain(|&id| id != last_id);
                self.callbacks.pop();
                self.max_queue_len = self.workers.len().saturating_mul(4).max(8);
                web_sys::console::log_1(&"Reduced worker count for performance".into());
            }
        } else if current_fps > target_fps * 1.2 && self.workers.len() < self.max_workers {
            let current = self.workers.len();
            self.add_worker();
            if self.workers.len() > current {
                web_sys::console::log_1(&"Added worker for better performance".into());
            }
        }
    }
}

pub fn adapt_worker_pool(
    performance_stats: Res<PerformanceStats>,
    mut worker_pool: NonSendMut<PhysicsWorkerPool>,
) {
    if performance_stats.target_fps > 0.0 {
        worker_pool.adapt_worker_count(performance_stats.fps, performance_stats.target_fps);
    }
}

#[wasm_bindgen]
pub fn physics_worker_entry() {
    // Web Worker entry point
    // This would be the actual worker script
}
