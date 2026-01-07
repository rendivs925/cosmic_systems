use crate::domain::entities::planet::Planet;
use crate::infrastructure::bevy_adapters::components::QualityLevel;
use bevy::math::Vec3;
#[cfg(feature = "ash")]
use crate::domain::services::physics;
#[cfg(feature = "ash")]
use bytemuck::{Pod, Zeroable};
#[cfg(feature = "ash")]
use std::sync::mpsc;
#[cfg(feature = "ash")]
use wgpu::util::DeviceExt;

#[cfg(feature = "ash")]
const KEPLER_SHADER: &str = include_str!("webgpu_kepler.wgsl");
#[cfg(feature = "ash")]
const WORKGROUP_SIZE: u32 = 64;

#[cfg(feature = "ash")]
pub struct VulkanKeplerSolver {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
}

#[cfg(feature = "ash")]
impl VulkanKeplerSolver {
    /// Create a new Vulkan Kepler solver with GPU acceleration via wgpu's Vulkan backend.
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            ..Default::default()
        });

        let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))?
        .ok_or("No Vulkan adapter available")?;

        let (device, queue) = block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("Vulkan Kepler Device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
            },
            None,
        ))??;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Vulkan Kepler Shader"),
            source: wgpu::ShaderSource::Wgsl(KEPLER_SHADER.into()),
        });

        let bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Vulkan Kepler Bind Group Layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: false },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Vulkan Kepler Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Vulkan Kepler Compute Pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: "main",
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        });

        Ok(Self {
            device,
            queue,
            pipeline,
            bind_group_layout,
        })
    }

    /// Solve Kepler equations using Vulkan compute with GPU acceleration.
    pub fn solve_batch(
        &mut self,
        planets: &[Planet],
        quality: QualityLevel,
        time_days: f32,
        scale_factor: f32,
    ) -> Result<Vec<Vec3>, Box<dyn std::error::Error>> {
        if planets.is_empty() {
            return Ok(Vec::new());
        }

        if planets.iter().any(|planet| planet.parent_entity.is_some()) {
            return Err("Vulkan solver expects primary bodies only (no moons)".into());
        }

        let iterations = quality_iterations(quality);
        let mut inputs = Vec::with_capacity(planets.len());

        for planet in planets {
            inputs.push(build_planet_input(
                planet,
                time_days,
                scale_factor,
                iterations,
            ));
        }

        let input_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vulkan Kepler Input Buffer"),
            contents: bytemuck::cast_slice(&inputs),
            usage: wgpu::BufferUsages::STORAGE,
        });

        let output_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Vulkan Kepler Output Buffer"),
            size: (std::mem::size_of::<GpuOutput>() * inputs.len()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let readback_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Vulkan Kepler Readback Buffer"),
            size: (std::mem::size_of::<GpuOutput>() * inputs.len()) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let params = KeplerParams {
            count: inputs.len() as u32,
            _pad: [0; 3],
        };
        let params_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vulkan Kepler Params Buffer"),
            contents: bytemuck::bytes_of(&params),
            usage: wgpu::BufferUsages::UNIFORM,
        });

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Vulkan Kepler Bind Group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: input_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: output_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: params_buffer.as_entire_binding(),
                },
            ],
        });

        let mut encoder =
            self.device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Vulkan Kepler Command Encoder"),
                });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Vulkan Kepler Compute Pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            let workgroups = (inputs.len() as u32).div_ceil(WORKGROUP_SIZE);
            pass.dispatch_workgroups(workgroups, 1, 1);
        }
        encoder.copy_buffer_to_buffer(
            &output_buffer,
            0,
            &readback_buffer,
            0,
            (std::mem::size_of::<GpuOutput>() * inputs.len()) as u64,
        );

        self.queue.submit(Some(encoder.finish()));

        let buffer_slice = readback_buffer.slice(..);
        let (sender, receiver) = mpsc::channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |v| {
            let _ = sender.send(v);
        });
        self.device.poll(wgpu::Maintain::Wait);
        receiver
            .recv()
            .map_err(|_| "Failed to receive Vulkan readback signal")??;

        let data = buffer_slice.get_mapped_range();
        let outputs: Vec<GpuOutput> = bytemuck::cast_slice(&data).to_vec();
        drop(data);
        readback_buffer.unmap();

        Ok(outputs
            .into_iter()
            .map(|output| Vec3::new(output.x, output.y, output.z))
            .collect())
    }
}

#[cfg(not(feature = "ash"))]
/// Fallback Vulkan Kepler solver when ash is not available
pub struct VulkanKeplerSolver;

#[cfg(not(feature = "ash"))]
impl VulkanKeplerSolver {
    /// Solve Kepler equations (always fails without ash)
    pub fn solve_batch(
        &self,
        _planets: &[Planet],
        _quality: QualityLevel,
        _time_days: f32,
        _scale_factor: f32,
    ) -> Result<Vec<Vec3>, Box<dyn std::error::Error>> {
        Err("Vulkan support not available - ash feature not enabled".into())
    }
}

#[cfg(feature = "ash")]
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct PlanetGpuInput {
    semi_major_axis_au: f32,
    eccentricity: f32,
    inclination_rad: f32,
    long_asc_node_rad: f32,
    arg_periapsis_rad: f32,
    mean_anomaly_rad: f32,
    scale_factor: f32,
    moon_scale: f32,
    parent_x: f32,
    parent_y: f32,
    parent_z: f32,
    parent_tilt_rad: f32,
    iterations: u32,
    is_moon: u32,
    has_parent_tilt: u32,
    _pad: u32,
}

#[cfg(feature = "ash")]
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct GpuOutput {
    x: f32,
    y: f32,
    z: f32,
    _pad: f32,
}

#[cfg(feature = "ash")]
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct KeplerParams {
    count: u32,
    _pad: [u32; 3],
}

#[cfg(feature = "ash")]
fn quality_iterations(quality: QualityLevel) -> u32 {
    match quality {
        QualityLevel::Ultra => 12,
        QualityLevel::High => 8,
        QualityLevel::Medium => 6,
        QualityLevel::Low => 4,
        QualityLevel::Minimal => 2,
    }
}

#[cfg(feature = "ash")]
fn build_planet_input(
    planet: &Planet,
    time_days: f32,
    scale_factor: f32,
    iterations: u32,
) -> PlanetGpuInput {
    let (semi_major_axis_au, eccentricity, inclination_rad, long_asc_node_rad, arg_periapsis_rad, mean_anomaly_rad) =
        if planet.name == "Sun" {
            (0.0, 0.0, 0.0, 0.0, 0.0, 0.0)
        } else if let Some(elements) = physics::orbital_elements_for(planet) {
            let mean_motion = mean_motion_rad_per_day(elements.semi_major_axis_au);
            let mean_anomaly = normalize_radians(elements.mean_anomaly_rad + mean_motion * time_days);
            (
                elements.semi_major_axis_au,
                elements.eccentricity,
                elements.inclination_rad,
                elements.long_asc_node_rad,
                elements.arg_periapsis_rad,
                mean_anomaly,
            )
        } else if planet.orbital_period_days > 0.0 {
            let mean_anomaly = normalize_radians(
                std::f32::consts::TAU * (time_days / planet.orbital_period_days),
            );
            (
                planet.orbital_distance_au,
                0.0,
                0.0,
                0.0,
                0.0,
                mean_anomaly,
            )
        } else {
            (planet.orbital_distance_au, 0.0, 0.0, 0.0, 0.0, 0.0)
        };

    PlanetGpuInput {
        semi_major_axis_au,
        eccentricity,
        inclination_rad,
        long_asc_node_rad,
        arg_periapsis_rad,
        mean_anomaly_rad,
        scale_factor,
        moon_scale: physics::MOON_ORBIT_SCALE,
        parent_x: 0.0,
        parent_y: 0.0,
        parent_z: 0.0,
        parent_tilt_rad: 0.0,
        iterations,
        is_moon: 0,
        has_parent_tilt: 0,
        _pad: 0,
    }
}

#[cfg(feature = "ash")]
fn mean_motion_rad_per_day(semi_major_axis_au: f32) -> f32 {
    const GAUSS_K: f32 = 0.0172021;
    GAUSS_K / semi_major_axis_au.powf(1.5)
}

#[cfg(feature = "ash")]
fn normalize_radians(angle: f32) -> f32 {
    angle.rem_euclid(std::f32::consts::TAU)
}

#[cfg(feature = "ash")]
fn block_on<T>(future: impl std::future::Future<Output = T>) -> Result<T, Box<dyn std::error::Error>> {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        Ok(handle.block_on(future))
    } else {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        Ok(runtime.block_on(future))
    }
}
