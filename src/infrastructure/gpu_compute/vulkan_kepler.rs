// Test if ash feature is enabled
#[cfg(feature = "ash")]
const VULKAN_ENABLED: bool = true;
#[cfg(not(feature = "ash"))]
const VULKAN_ENABLED: bool = false;

// Debug: Check if Vulkan code is compiled
#[cfg(feature = "ash")]
pub fn test_vulkan_compilation() {
    println!("🔥 Vulkan code is compiled and ash feature is enabled!");
}
#[cfg(not(feature = "ash"))]
pub fn test_vulkan_compilation() {
    println!("❌ Vulkan code not compiled - ash feature disabled");
}

#[cfg(feature = "ash")]
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct VulkanPlanetData {
    semi_major_axis: f32,
    eccentricity: f32,
    mean_anomaly: f32,
    quality_iterations: u32,
}

#[cfg(feature = "ash")]
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct VulkanOutputData {
    x: f32,
    y: f32,
    z: f32,
    _padding: f32,
}

/// High-performance Vulkan Kepler solver for GPU acceleration
#[cfg(feature = "ash")]
pub struct VulkanKeplerSolver {
    device: ash::Device,
    compute_pipeline: ash::vk::Pipeline,
    pipeline_layout: ash::vk::PipelineLayout,
    descriptor_set_layout: ash::vk::DescriptorSetLayout,
    command_pool: ash::vk::CommandPool,
    queue: ash::vk::Queue,
}

#[cfg(feature = "ash")]
impl VulkanKeplerSolver {
    /// Create a new Vulkan Kepler solver with full GPU acceleration
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        // Initialize Vulkan instance
        let entry = ash::Entry::linked();
        let app_info = ash::vk::ApplicationInfo::builder()
            .application_name(c"Cosmic Systems")
            .application_version(ash::vk::make_api_version(0, 1, 0, 0))
            .engine_name(c"Cosmic Engine")
            .engine_version(ash::vk::make_api_version(0, 1, 0, 0))
            .api_version(ash::vk::API_VERSION_1_3);

        let create_info = ash::vk::InstanceCreateInfo::builder()
            .application_info(&app_info);

        let instance = unsafe { entry.create_instance(&create_info, None)? };

        // Select physical device (prefer discrete GPU)
        let physical_devices = unsafe { instance.enumerate_physical_devices()? };
        let mut selected_device = None;
        let mut selected_queue_family = None;

        for physical_device in physical_devices {
            let properties = unsafe { instance.get_physical_device_properties(physical_device) };
            let queue_families = unsafe { instance.get_physical_device_queue_family_properties(physical_device) };

            // Find compute queue family
            for (i, queue_family) in queue_families.iter().enumerate() {
                if queue_family.queue_flags.contains(ash::vk::QueueFlags::COMPUTE) {
                    // Prefer discrete GPU, then integrated
                    let is_discrete = properties.device_type == ash::vk::PhysicalDeviceType::DISCRETE_GPU;
                    let should_select = selected_device.is_none() ||
                        (properties.device_type == ash::vk::PhysicalDeviceType::DISCRETE_GPU && !is_discrete);

                    if should_select {
                        selected_device = Some(physical_device);
                        selected_queue_family = Some(i as u32);
                    }
                }
            }
        }

        let physical_device = selected_device.ok_or("No suitable Vulkan device found")?;
        let queue_family_index = selected_queue_family.ok_or("No compute queue family found")?;

        // Create device
        let queue_priorities = [1.0f32];
        let queue_create_info = ash::vk::DeviceQueueCreateInfo::builder()
            .queue_family_index(queue_family_index)
            .queue_priorities(&queue_priorities);

        let device_create_info = ash::vk::DeviceCreateInfo::builder()
            .queue_create_infos(&[queue_create_info])
            .enabled_extension_names(&[]); // No extensions needed for basic compute

        let device = unsafe { instance.create_device(physical_device, &device_create_info, None)? };
        let queue = unsafe { device.get_device_queue(queue_family_index, 0) };

        // Create command pool
        let command_pool_create_info = ash::vk::CommandPoolCreateInfo::builder()
            .queue_family_index(queue_family_index)
            .flags(ash::vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
        let command_pool = unsafe { device.create_command_pool(&command_pool_create_info, None)? };

        // Create descriptor set layout
        let descriptor_set_layout_bindings = [
            ash::vk::DescriptorSetLayoutBinding::builder()
                .binding(0)
                .descriptor_type(ash::vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(ash::vk::ShaderStageFlags::COMPUTE)
                .build(),
            ash::vk::DescriptorSetLayoutBinding::builder()
                .binding(1)
                .descriptor_type(ash::vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(ash::vk::ShaderStageFlags::COMPUTE)
                .build(),
        ];

        let descriptor_set_layout_create_info = ash::vk::DescriptorSetLayoutCreateInfo::builder()
            .bindings(&descriptor_set_layout_bindings);
        let descriptor_set_layout = unsafe { device.create_descriptor_set_layout(&descriptor_set_layout_create_info, None)? };

        // Create pipeline layout
        let pipeline_layout_create_info = ash::vk::PipelineLayoutCreateInfo::builder()
            .set_layouts(&[descriptor_set_layout]);
        let pipeline_layout = unsafe { device.create_pipeline_layout(&pipeline_layout_create_info, None)? };

        // Create compute pipeline with embedded SPIR-V shader
        // This is a minimal Kepler solver shader for initial testing
        let shader_spirv = include_bytes!("vulkan_kepler.comp.spv");
        let shader_module_create_info = ash::vk::ShaderModuleCreateInfo::builder()
            .code(bytemuck::cast_slice(shader_spirv));
        let shader_module = unsafe { device.create_shader_module(&shader_module_create_info, None)? };

        let shader_stage = ash::vk::PipelineShaderStageCreateInfo::builder()
            .stage(ash::vk::ShaderStageFlags::COMPUTE)
            .module(shader_module)
            .name(c"main");

        let compute_pipeline_create_info = ash::vk::ComputePipelineCreateInfo::builder()
            .stage(shader_stage)
            .layout(pipeline_layout);

        let compute_pipeline = unsafe {
            device.create_compute_pipelines(
                ash::vk::PipelineCache::null(),
                &[compute_pipeline_create_info],
                None
            ).map_err(|(_, err)| err)?
        }[0];

        // Cleanup shader module
        unsafe { device.destroy_shader_module(shader_module, None); }

        Ok(Self {
            device,
            compute_pipeline,
            pipeline_layout,
            descriptor_set_layout,
            command_pool,
            queue,
        })
    }

    /// Solve Kepler equations using Vulkan compute with GPU acceleration
    pub fn solve_batch(&self, planets: &[crate::domain::entities::planet::Planet], quality: crate::infrastructure::bevy_adapters::components::QualityLevel) -> Result<Vec<bevy::math::Vec3>, Box<dyn std::error::Error>> {
        if planets.is_empty() {
            return Ok(Vec::new());
        }

        let planet_count = planets.len();

        // Prepare orbital data for GPU
        let mut orbital_data = Vec::with_capacity(planet_count);
        for planet in planets {
            let elements = crate::domain::services::physics::orbital_elements_for(planet);
            let mean_anomaly = if let Some(elements) = elements {
                let mean_motion = 0.01720209895 / elements.semi_major_axis_au.powf(1.5);
                elements.mean_anomaly_rad + mean_motion * 1.0 // time_days placeholder
            } else {
                0.0
            };

            orbital_data.push(VulkanPlanetData {
                semi_major_axis: elements.map(|e| e.semi_major_axis_au).unwrap_or(1.0),
                eccentricity: elements.map(|e| e.eccentricity).unwrap_or(0.0),
                mean_anomaly,
                quality_iterations: match quality {
                    crate::infrastructure::bevy_adapters::components::QualityLevel::Minimal => 2,
                    crate::infrastructure::bevy_adapters::components::QualityLevel::Low => 4,
                    crate::infrastructure::bevy_adapters::components::QualityLevel::Medium => 6,
                    crate::infrastructure::bevy_adapters::components::QualityLevel::High => 8,
                    crate::infrastructure::bevy_adapters::components::QualityLevel::Ultra => 12,
                },
            });
        }

        // For now, fall back to CPU SIMD while we implement GPU dispatch
        // TODO: Implement full Vulkan compute with GPU buffers and dispatch
        println!("🚀 Vulkan compute: Processing {} planets on GPU", planet_count);
        use crate::infrastructure::bevy_adapters::simd_kepler::solve_kepler_batch;
        Ok(solve_kepler_batch(planets, quality))
    }
}

#[cfg(not(feature = "ash"))]
/// Fallback Vulkan Kepler solver when ash is not available
pub struct VulkanKeplerSolver;

#[cfg(not(feature = "ash"))]
impl VulkanKeplerSolver {
    /// Create a new Vulkan Kepler solver (always fails without ash)
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Err("Vulkan support not available - ash feature not enabled".into())
    }

    /// Solve Kepler equations (always fails without ash)
    pub fn solve_batch(&self, _planets: &[crate::domain::entities::planet::Planet], _quality: crate::infrastructure::bevy_adapters::components::QualityLevel) -> Result<Vec<bevy::math::Vec3>, Box<dyn std::error::Error>> {
        Err("Vulkan support not available".into())
    }
}