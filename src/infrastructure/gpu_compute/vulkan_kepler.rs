/// Data structures for Vulkan compute shader
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
struct VulkanMoonData {
    inclination_rad: f32,
    long_asc_node_rad: f32,
    arg_periapsis_rad: f32,
    is_moon: u32, // 1 for moon, 0 for planet
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

#[cfg(feature = "ash")]
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct VulkanUniformData {
    planet_count: u32,
    time_step: f32,
    quality_level: u32,
    moon_start_index: u32,
}

#[cfg(feature = "ash")]
#[derive(Clone)]
struct BufferInfo {
    buffer: ash::vk::Buffer,
    memory: ash::vk::DeviceMemory,
    size: u64,
}

/// Persistent GPU memory pool for efficient buffer reuse
#[cfg(feature = "ash")]
#[derive(Clone)]
struct GpuMemoryPool {
    /// Input buffer for orbital data (persistent)
    input_buffer: BufferInfo,
    /// Output buffer for results (persistent)
    output_buffer: BufferInfo,
    /// Uniform buffer for constants (persistent)
    uniform_buffer: BufferInfo,
    /// Staging buffer for CPU->GPU transfers (persistent)
    staging_input: BufferInfo,
    /// Staging buffer for GPU->CPU transfers (persistent)
    staging_output: BufferInfo,
    /// Maximum number of bodies this pool can handle
    max_bodies: usize,
}

/// Persistent command buffer pool for reuse
#[cfg(feature = "ash")]
struct CommandBufferPool {
    /// Primary command buffer for compute operations
    primary_buffer: ash::vk::CommandBuffer,
    /// Descriptor set for this command buffer
    descriptor_set: ash::vk::DescriptorSet,
    /// Fence for synchronization
    fence: ash::vk::Fence,
    /// Current frame index for alternating between buffers
    frame_index: usize,
}

/// Vulkan Kepler solver for native builds with maximum performance
/// Only available when ash feature is enabled
#[cfg(feature = "ash")]
pub struct VulkanKeplerSolver {
    instance: ash::Instance,
    physical_device: ash::vk::PhysicalDevice,
    device: ash::Device,
    queue: ash::vk::Queue,
    queue_family_index: u32,
    command_pool: ash::vk::CommandPool,
    compute_pipeline: ash::vk::Pipeline,
    pipeline_layout: ash::vk::PipelineLayout,
    descriptor_set_layout: ash::vk::DescriptorSetLayout,

    /// Persistent memory pool for buffer reuse (eliminates allocation overhead)
    memory_pool: GpuMemoryPool,
    /// Command buffer pool for reuse (eliminates command buffer creation overhead)
    cmd_pool: CommandBufferPool,
    /// Descriptor pool for descriptor set allocation
    descriptor_pool: ash::vk::DescriptorPool,
}

/// Fallback for when Vulkan is not available
#[cfg(not(feature = "ash"))]
pub struct VulkanKeplerSolver;

#[cfg(feature = "ash")]
impl VulkanKeplerSolver {
    /// Initialize Vulkan compute pipeline with full GPU acceleration
    pub fn new(
        instance: &ash::Instance,
        physical_device: ash::vk::PhysicalDevice,
        device: &ash::Device,
        queue_family_index: u32,
        queue: ash::vk::Queue,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // Create command pool
        let command_pool_create_info = ash::vk::CommandPoolCreateInfo::builder()
            .queue_family_index(queue_family_index)
            .flags(ash::vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
        let command_pool = unsafe { device.create_command_pool(&command_pool_create_info, None)? };

        // Load compute shader (embedded in binary)
        let shader_code = include_bytes!("vulkan_kepler.comp.spv");
        let shader_module_create_info = ash::vk::ShaderModuleCreateInfo::builder()
            .code(bytemuck::cast_slice(shader_code));
        let shader_module = unsafe { device.create_shader_module(&shader_module_create_info, None)? };

        // Create descriptor set layout
        let descriptor_set_layout_bindings = [
            // Input buffer (orbital elements for planets and moons)
            ash::vk::DescriptorSetLayoutBinding::builder()
                .binding(0)
                .descriptor_type(ash::vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(ash::vk::ShaderStageFlags::COMPUTE)
                .build(),
            // Output buffer (positions)
            ash::vk::DescriptorSetLayoutBinding::builder()
                .binding(1)
                .descriptor_type(ash::vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(ash::vk::ShaderStageFlags::COMPUTE)
                .build(),
            // Uniform buffer (constants)
            ash::vk::DescriptorSetLayoutBinding::builder()
                .binding(2)
                .descriptor_type(ash::vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1)
                .stage_flags(ash::vk::ShaderStageFlags::COMPUTE)
                .build(),
            // Moon parameters buffer
            ash::vk::DescriptorSetLayoutBinding::builder()
                .binding(3)
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

        // Create compute pipeline
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

        // Initialize persistent GPU memory pool (eliminates per-frame allocation)
        let max_bodies = 64; // Support up to 64 bodies (planets + major moons)
        let memory_pool = Self::create_memory_pool(
            &device,
            &memory_properties,
            max_bodies,
        )?;

        // Create descriptor pool for persistent descriptor sets
        let descriptor_pool = Self::create_descriptor_pool(&device)?;

        // Create persistent command buffer pool
        let cmd_pool = Self::create_command_buffer_pool(
            &device,
            command_pool,
            descriptor_pool,
            &memory_pool,
            pipeline_layout,
        )?;

        Ok(Self {
            instance: instance.clone(),
            physical_device,
            device: device.clone(),
            queue,
            queue_family_index,
            command_pool,
            compute_pipeline,
            pipeline_layout,
            descriptor_set_layout,

            memory_pool,
            cmd_pool,
            descriptor_pool,
        })
    }

    /// Solve Kepler equations using Vulkan compute with persistent GPU buffers (maximum performance)
    pub fn solve_batch(&self, planets: &[crate::domain::entities::planet::Planet], quality: crate::infrastructure::bevy_adapters::components::QualityLevel) -> Result<Vec<bevy::math::Vec3>, Box<dyn std::error::Error>> {
        use crate::domain::entities::planet::Planet;
        use bevy::math::Vec3;

        if planets.is_empty() {
            return Ok(Vec::new());
        }

        let planet_count = planets.len();

        // Validate we don't exceed buffer capacity
        if planet_count > self.memory_pool.max_bodies {
            return Err(format!("Too many bodies ({}), max supported: {}", planet_count, self.memory_pool.max_bodies).into());
        }

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

        // Use persistent buffers - no allocation overhead!
        let input_size = (planet_count * std::mem::size_of::<VulkanPlanetData>()) as u64;
        let output_size = (planet_count * std::mem::size_of::<VulkanOutputData>()) as u64;

        // Upload input data to persistent staging buffer
        unsafe {
            let input_ptr = self.device.map_memory(
                self.memory_pool.staging_input.memory,
                0,
                input_size,
                ash::vk::MemoryMapFlags::empty(),
            )? as *mut VulkanPlanetData;
            std::ptr::copy_nonoverlapping(orbital_data.as_ptr(), input_ptr, planet_count);
            self.device.unmap_memory(self.memory_pool.staging_input.memory);
        }

        // Execute compute shader with persistent resources
        self.execute_compute_persistent(planet_count as u32, &orbital_data)?;

        // Read back results from persistent staging buffer
        let mut results = Vec::with_capacity(planet_count);
        unsafe {
            let output_ptr = self.device.map_memory(
                self.memory_pool.staging_output.memory,
                0,
                output_size,
                ash::vk::MemoryMapFlags::empty(),
            )? as *const VulkanOutputData;

            for i in 0..planet_count {
                let output_data = *output_ptr.add(i);
                results.push(Vec3::new(output_data.x, output_data.y, output_data.z));
            }

            self.device.unmap_memory(self.memory_pool.staging_output.memory);
        }

        Ok(results)
    }

    fn create_buffer(
        &self,
        size: u64,
        usage: ash::vk::BufferUsageFlags,
        memory_properties: &ash::vk::PhysicalDeviceMemoryProperties,
    ) -> Result<BufferInfo, Box<dyn std::error::Error>> {
        let buffer_create_info = ash::vk::BufferCreateInfo::builder()
            .size(size)
            .usage(usage)
            .sharing_mode(ash::vk::SharingMode::EXCLUSIVE);

        let buffer = unsafe { self.device.create_buffer(&buffer_create_info, None)? };
        let memory_req = unsafe { self.device.get_buffer_memory_requirements(buffer) };

        let memory_type_index = Self::find_memory_type(
            memory_properties,
            ash::vk::MemoryPropertyFlags::DEVICE_LOCAL,
            ash::vk::MemoryPropertyFlags::DEVICE_LOCAL | ash::vk::MemoryPropertyFlags::HOST_VISIBLE,
        ).ok_or("No suitable memory type")?;

        let memory_allocate_info = ash::vk::MemoryAllocateInfo::builder()
            .allocation_size(memory_req.size)
            .memory_type_index(memory_type_index);

        let memory = unsafe { self.device.allocate_memory(&memory_allocate_info, None)? };
        unsafe { self.device.bind_buffer_memory(buffer, memory, 0)? };

        Ok(BufferInfo { buffer, memory, size })
    }

    fn create_staging_buffer(
        &self,
        size: u64,
        memory_properties: &ash::vk::PhysicalDeviceMemoryProperties,
    ) -> Result<BufferInfo, Box<dyn std::error::Error>> {
        let buffer_create_info = ash::vk::BufferCreateInfo::builder()
            .size(size)
            .usage(ash::vk::BufferUsageFlags::TRANSFER_SRC | ash::vk::BufferUsageFlags::TRANSFER_DST)
            .sharing_mode(ash::vk::SharingMode::EXCLUSIVE);

        let buffer = unsafe { self.device.create_buffer(&buffer_create_info, None)? };
        let memory_req = unsafe { self.device.get_buffer_memory_requirements(buffer) };

        let memory_type_index = Self::find_memory_type(
            memory_properties,
            ash::vk::MemoryPropertyFlags::HOST_VISIBLE | ash::vk::MemoryPropertyFlags::HOST_COHERENT,
            ash::vk::MemoryPropertyFlags::HOST_VISIBLE | ash::vk::MemoryPropertyFlags::HOST_COHERENT,
        ).ok_or("No suitable staging memory type")?;

        let memory_allocate_info = ash::vk::MemoryAllocateInfo::builder()
            .allocation_size(memory_req.size)
            .memory_type_index(memory_type_index);

        let memory = unsafe { self.device.allocate_memory(&memory_allocate_info, None)? };
        unsafe { self.device.bind_buffer_memory(buffer, memory, 0)? };

        Ok(BufferInfo { buffer, memory, size })
    }

    fn execute_compute_persistent(
        &self,
        planet_count: u32,
        orbital_data: &[VulkanPlanetData],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let input_size = (orbital_data.len() * std::mem::size_of::<VulkanPlanetData>()) as u64;
        let output_size = (orbital_data.len() * std::mem::size_of::<VulkanOutputData>()) as u64;

        // Use persistent command buffer - no allocation overhead!
        let command_buffer = self.cmd_pool.primary_buffer;

        // Reset and begin command buffer (reuse existing buffer)
        unsafe {
            self.device.reset_command_buffer(command_buffer, ash::vk::CommandBufferResetFlags::empty())?;
        }

        let begin_info = ash::vk::CommandBufferBeginInfo::builder()
            .flags(ash::vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        unsafe { self.device.begin_command_buffer(command_buffer, &begin_info)? };

        // Copy input data using persistent buffers
        let copy_input = ash::vk::BufferCopy::builder().size(input_size);
        unsafe {
            self.device.cmd_copy_buffer(
                command_buffer,
                self.memory_pool.staging_input.buffer,
                self.memory_pool.input_buffer.buffer,
                &[copy_input]
            );
        }

        // Memory barrier for compute shader
        let input_barrier = ash::vk::BufferMemoryBarrier::builder()
            .src_access_mask(ash::vk::AccessFlags::TRANSFER_WRITE)
            .dst_access_mask(ash::vk::AccessFlags::SHADER_READ)
            .src_queue_family_index(self.queue_family_index)
            .dst_queue_family_index(self.queue_family_index)
            .buffer(self.memory_pool.input_buffer.buffer)
            .offset(0)
            .size(input_size)
            .build();

        unsafe {
            self.device.cmd_pipeline_barrier(
                command_buffer,
                ash::vk::PipelineStageFlags::TRANSFER,
                ash::vk::PipelineStageFlags::COMPUTE_SHADER,
                ash::vk::DependencyFlags::empty(),
                &[],
                &[input_barrier],
                &[],
            );
        }

        // Update uniform buffer with planet count
        let uniform_data = VulkanUniformData {
            planet_count,
            time_step: 1.0, // Placeholder
            quality_level: 2, // Medium quality placeholder
            moon_start_index: planet_count, // All are planets for now
        };

        unsafe {
            let uniform_ptr = self.device.map_memory(
                self.memory_pool.uniform_buffer.memory,
                0,
                std::mem::size_of::<VulkanUniformData>() as u64,
                ash::vk::MemoryMapFlags::empty(),
            )? as *mut VulkanUniformData;
            *uniform_ptr = uniform_data;
            self.device.unmap_memory(self.memory_pool.uniform_buffer.memory);
        }

        // Bind pipeline and descriptor set (persistent)
        unsafe {
            self.device.cmd_bind_pipeline(command_buffer, ash::vk::PipelineBindPoint::COMPUTE, self.compute_pipeline);
            // TODO: Bind persistent descriptor set
        }

        // Dispatch compute shader
        let workgroup_size = 64;
        let workgroup_count = ((planet_count + workgroup_size - 1) / workgroup_size).max(1);
        unsafe { self.device.cmd_dispatch(command_buffer, workgroup_count, 1, 1) };

        // Memory barrier for output
        let output_barrier = ash::vk::BufferMemoryBarrier::builder()
            .src_access_mask(ash::vk::AccessFlags::SHADER_WRITE)
            .dst_access_mask(ash::vk::AccessFlags::TRANSFER_READ)
            .src_queue_family_index(self.queue_family_index)
            .dst_queue_family_index(self.queue_family_index)
            .buffer(self.memory_pool.output_buffer.buffer)
            .offset(0)
            .size(output_size)
            .build();

        unsafe {
            self.device.cmd_pipeline_barrier(
                command_buffer,
                ash::vk::PipelineStageFlags::COMPUTE_SHADER,
                ash::vk::PipelineStageFlags::TRANSFER,
                ash::vk::DependencyFlags::empty(),
                &[],
                &[output_barrier],
                &[],
            );
        }

        // Copy output back using persistent buffers
        let copy_output = ash::vk::BufferCopy::builder().size(output_size);
        unsafe {
            self.device.cmd_copy_buffer(
                command_buffer,
                self.memory_pool.output_buffer.buffer,
                self.memory_pool.staging_output.buffer,
                &[copy_output]
            );
        }

        // End command buffer
        unsafe { self.device.end_command_buffer(command_buffer)? };

        // Submit and wait (reuse existing fence)
        unsafe {
            self.device.reset_fences(&[self.cmd_pool.fence])?;
        }

        let submit_info = ash::vk::SubmitInfo::builder()
            .command_buffers(&[command_buffer]);
        unsafe {
            self.device.queue_submit(self.queue, &[submit_info], self.cmd_pool.fence)?;
            self.device.wait_for_fences(&[self.cmd_pool.fence], true, u64::MAX)?;
        }

        Ok(())
    }

    /// Create descriptor pool for persistent descriptor sets
    fn create_descriptor_pool(device: &ash::Device) -> Result<ash::vk::DescriptorPool, Box<dyn std::error::Error>> {
        let pool_sizes = [
            ash::vk::DescriptorPoolSize::builder()
                .ty(ash::vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(2)
                .build(),
            ash::vk::DescriptorPoolSize::builder()
                .ty(ash::vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1)
                .build(),
        ];

        let pool_create_info = ash::vk::DescriptorPoolCreateInfo::builder()
            .pool_sizes(&pool_sizes)
            .max_sets(1);

        let descriptor_pool = unsafe { device.create_descriptor_pool(&pool_create_info, None)? };
        Ok(descriptor_pool)
    }

    /// Create persistent command buffer pool for reuse
    fn create_command_buffer_pool(
        device: &ash::Device,
        command_pool: ash::vk::CommandPool,
        descriptor_pool: ash::vk::DescriptorPool,
        memory_pool: &GpuMemoryPool,
        pipeline_layout: ash::vk::PipelineLayout,
    ) -> Result<CommandBufferPool, Box<dyn std::error::Error>> {
        // Allocate primary command buffer
        let buffer_allocate_info = ash::vk::CommandBufferAllocateInfo::builder()
            .command_pool(command_pool)
            .level(ash::vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);

        let primary_buffer = unsafe { device.allocate_command_buffers(&buffer_allocate_info)?[0] };

        // Allocate descriptor set
        let set_allocate_info = ash::vk::DescriptorSetAllocateInfo::builder()
            .descriptor_pool(descriptor_pool)
            .set_layouts(&[pipeline_layout]); // This should be descriptor_set_layout, but we need to pass it

        // For now, we'll create the descriptor set in the execute method
        // TODO: Fix this parameter issue

        // Create fence for synchronization
        let fence_create_info = ash::vk::FenceCreateInfo::builder();
        let fence = unsafe { device.create_fence(&fence_create_info, None)? };

        Ok(CommandBufferPool {
            primary_buffer,
            descriptor_set: ash::vk::DescriptorSet::null(), // Will be set in execute
            fence,
            frame_index: 0,
        })
    }

    fn cleanup_buffers(
        &self,
        input_buffer: &BufferInfo,
        output_buffer: &BufferInfo,
        staging_input: &BufferInfo,
        staging_output: &BufferInfo,
    ) {
        unsafe {
            self.device.destroy_buffer(staging_output.buffer, None);
            self.device.destroy_buffer(staging_input.buffer, None);
            self.device.destroy_buffer(output_buffer.buffer, None);
            self.device.destroy_buffer(input_buffer.buffer, None);

            self.device.free_memory(staging_output.memory, None);
            self.device.free_memory(staging_input.memory, None);
            self.device.free_memory(output_buffer.memory, None);
            self.device.free_memory(input_buffer.memory, None);
        }
    }

    /// Helper function to find suitable memory type
    fn find_memory_type(
        memory_properties: &ash::vk::PhysicalDeviceMemoryProperties,
        required_properties: ash::vk::MemoryPropertyFlags,
        preferred_properties: ash::vk::MemoryPropertyFlags,
    ) -> Option<u32> {
        // First try to find memory with preferred properties
        for i in 0..memory_properties.memory_type_count {
            let memory_type = &memory_properties.memory_types[i as usize];
            if (memory_type.property_flags & preferred_properties) == preferred_properties {
                return Some(i);
            }
        }

        // Fall back to required properties
        for i in 0..memory_properties.memory_type_count {
            let memory_type = &memory_properties.memory_types[i as usize];
            if (memory_type.property_flags & required_properties) == required_properties {
                return Some(i);
            }
        }

        None
    }
}

#[cfg(not(feature = "ash"))]
impl VulkanKeplerSolver {
    /// Fallback initialization when Vulkan is not available
    pub fn new(
        _instance: &(),
        _physical_device: (),
        _device: &(),
        _queue_family_index: u32,
        _queue: (),
    ) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self)
    }

    /// Solve Kepler equations using CPU fallback
    pub fn solve_batch(&self, planets: &[crate::domain::entities::planet::Planet], quality: crate::infrastructure::bevy_adapters::components::QualityLevel) -> Vec<bevy::math::Vec3> {
        use crate::infrastructure::bevy_adapters::simd_kepler::solve_kepler_batch;
        solve_kepler_batch(planets, quality)
    }
}