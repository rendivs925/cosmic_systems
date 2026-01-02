use bevy::prelude::Entity;
use js_sys::{Float32Array, Reflect};
use std::cell::RefCell;
use std::collections::{HashSet, VecDeque};
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{MessageEvent, Worker};

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub struct OrbitShapeData {
    pub semi_major_axis_units: f32,
    pub eccentricity: f32,
    pub inclination_rad: f32,
    pub long_asc_node_rad: f32,
    pub arg_periapsis_rad: f32,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct OrbitMeshTask {
    pub task_id: u64,
    pub segments: u32,
    pub orbit_shape: OrbitShapeData,
}

pub struct OrbitMeshResult {
    pub task_id: u64,
    pub positions: Vec<f32>,
}

pub struct OrbitMeshWorkerPool {
    workers: Vec<Worker>,
    callbacks: Vec<Closure<dyn FnMut(MessageEvent)>>,
    inflight: HashSet<u64>,
    results: Rc<RefCell<VecDeque<OrbitMeshResult>>>,
    next_worker: usize,
}

impl OrbitMeshWorkerPool {
    pub fn new() -> Self {
        let results = Rc::new(RefCell::new(VecDeque::new()));
        let mut pool = OrbitMeshWorkerPool {
            workers: Vec::new(),
            callbacks: Vec::new(),
            inflight: HashSet::new(),
            results,
            next_worker: 0,
        };

        let worker_count = Self::optimal_worker_count();
        for _ in 0..worker_count {
            let worker = match create_worker() {
                Ok(worker) => worker,
                Err(err) => {
                    web_sys::console::error_1(&err);
                    continue;
                }
            };

            let results = Rc::clone(&pool.results);
            let callback = Closure::<dyn FnMut(MessageEvent)>::wrap(Box::new(move |event| {
                let data = event.data();
                let task_id = Reflect::get(&data, &JsValue::from_str("task_id"))
                    .ok()
                    .and_then(|value| value.as_f64())
                    .map(|value| value as u64);
                let positions_value =
                    Reflect::get(&data, &JsValue::from_str("positions")).ok();
                let positions = positions_value
                    .and_then(|value| {
                        let array = Float32Array::new(&value);
                        let mut vec = vec![0.0; array.length() as usize];
                        array.copy_to(&mut vec);
                        Some(vec)
                    });

                let (Some(task_id), Some(positions)) = (task_id, positions) else {
                    return;
                };

                results.borrow_mut().push_back(OrbitMeshResult { task_id, positions });
            }));

            worker.set_onmessage(Some(callback.as_ref().unchecked_ref()));
            pool.workers.push(worker);
            pool.callbacks.push(callback);
        }

        pool
    }

    pub fn worker_count(&self) -> usize {
        self.workers.len()
    }

    pub fn request(&mut self, task: OrbitMeshTask) {
        if self.workers.is_empty() || self.inflight.contains(&task.task_id) {
            return;
        }
        let worker_index = self.next_worker % self.workers.len();
        let Some(worker) = self.workers.get(worker_index) else {
            return;
        };
        if let Ok(message) = serde_wasm_bindgen::to_value(&task) {
            if worker.post_message(&message).is_ok() {
                self.inflight.insert(task.task_id);
                self.next_worker = self.next_worker.wrapping_add(1);
            }
        }
    }

    pub fn take_results(&mut self) -> Vec<OrbitMeshResult> {
        let results = self.results.borrow_mut().drain(..).collect();
        results
    }

    pub fn mark_complete(&mut self, task_id: u64) {
        self.inflight.remove(&task_id);
    }

    fn optimal_worker_count() -> usize {
        let cores = web_sys::window()
            .map(|window| window.navigator().hardware_concurrency() as usize)
            .unwrap_or(4);
        cores.min(4).max(1)
    }
}

fn create_worker() -> Result<Worker, JsValue> {
    let script = r#"
        const TAU = Math.PI * 2.0;

        self.onmessage = function(e) {
            const task = e.data;
            const shape = task.orbit_shape;
            const segments = task.segments;
            const positions = new Float32Array(segments * 3);

            const eClamped = Math.max(0.0, Math.min(0.99, shape.eccentricity));
            const semiLatus = shape.semi_major_axis_units * (1.0 - eClamped * eClamped);

            for (let i = 0; i < segments; i++) {
                const trueAnomaly = (i / segments) * TAU;
                const radius = semiLatus / (1.0 + eClamped * Math.cos(trueAnomaly));
                const xOrb = radius * Math.cos(trueAnomaly);
                const zOrb = radius * Math.sin(trueAnomaly);
                const pos = transform_orbital_point(
                    xOrb,
                    zOrb,
                    shape.inclination_rad,
                    shape.long_asc_node_rad,
                    shape.arg_periapsis_rad
                );
                const idx = i * 3;
                positions[idx] = pos.x;
                positions[idx + 1] = pos.y;
                positions[idx + 2] = pos.z;
            }

            self.postMessage({ task_id: task.task_id, positions }, [positions.buffer]);
        };

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

pub fn task_id_from_entity(entity: Entity) -> u64 {
    entity.to_bits()
}

pub fn entity_from_task_id(task_id: u64) -> Entity {
    Entity::from_bits(task_id)
}
