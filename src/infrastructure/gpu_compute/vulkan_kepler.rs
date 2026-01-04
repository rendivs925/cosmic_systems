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
    shader_module: ash::vk::ShaderModule,
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
            shader_module,
        })
    }

    /// Solve Kepler equations using Vulkan compute with full GPU acceleration
    pub fn solve_batch(&self, planets: &[crate::domain::entities::planet::Planet], quality: crate::infrastructure::bevy_adapters::components::QualityLevel) -> Result<Vec<bevy::math::Vec3>, Box<dyn std::error::Error>> {
        use crate::domain::entities::planet::Planet;
        use bevy::math::Vec3;

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

        // Allocate GPU buffers
        let input_size = (planet_count * std::mem::size_of::<VulkanPlanetData>()) as u64;
        let output_size = (planet_count * std::mem::size_of::<VulkanOutputData>()) as u64;

        // Get memory properties
        let memory_properties = unsafe {
            self.instance.get_physical_device_memory_properties(self.physical_device)
        };

        // Create buffers
        let input_buffer = self.create_buffer(
            input_size,
            ash::vk::BufferUsageFlags::STORAGE_BUFFER | ash::vk::BufferUsageFlags::TRANSFER_DST,
            &memory_properties,
        )?;

        let output_buffer = self.create_buffer(
            output_size,
            ash::vk::BufferUsageFlags::STORAGE_BUFFER | ash::vk::BufferUsageFlags::TRANSFER_SRC,
            &memory_properties,
        )?;

        // Create staging buffers for CPU-GPU transfer
        let staging_input = self.create_staging_buffer(input_size, &memory_properties)?;
        let staging_output = self.create_staging_buffer(output_size, &memory_properties)?;

        // Upload input data
        unsafe {
            let input_ptr = self.device.map_memory(
                staging_input.memory,
                0,
                input_size,
                ash::vk::MemoryMapFlags::empty(),
            )? as *mut VulkanPlanetData;
            std::ptr::copy_nonoverlapping(orbital_data.as_ptr(), input_ptr, planet_count);
            self.device.unmap_memory(staging_input.memory);
        }

        // Execute compute shader
        self.execute_compute(
            planet_count as u32,
            &input_buffer,
            &output_buffer,
            &staging_input.buffer,
            &staging_output.buffer,
            input_size,
            output_size,
        )?;

        // Read back results
        let mut results = Vec::with_capacity(planet_count);
        unsafe {
            let output_ptr = self.device.map_memory(
                staging_output.memory,
                0,
                output_size,
                ash::vk::MemoryMapFlags::empty(),
            )? as *const VulkanOutputData;

            for i in 0..planet_count {
                let output_data = *output_ptr.add(i);
                results.push(Vec3::new(output_data.x, output_data.y, output_data.z));
            }

            self.device.unmap_memory(staging_output.memory);
        }

        // Cleanup
        self.cleanup_buffers(&input_buffer, &output_buffer, &staging_input, &staging_output);

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

    fn execute_compute(
        &self,
        planet_count: u32,
        input_buffer: &BufferInfo,
        output_buffer: &BufferInfo,
        staging_input: &ash::vk::Buffer,
        staging_output: &ash::vk::Buffer,
        input_size: u64,
        output_size: u64,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Create command buffer
        let command_buffer_allocate_info = ash::vk::CommandBufferAllocateInfo::builder()
            .command_pool(self.command_pool)
            .level(ash::vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);

        let command_buffer = unsafe { self.device.allocate_command_buffers(&command_buffer_allocate_info)?[0] };

        // Begin recording
        let begin_info = ash::vk::CommandBufferBeginInfo::builder()
            .flags(ash::vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        unsafe { self.device.begin_command_buffer(command_buffer, &begin_info)? };

        // Copy input data to GPU
        let copy_input = ash::vk::BufferCopy::builder().size(input_size);
        unsafe {
            self.device.cmd_copy_buffer(command_buffer, *staging_input, input_buffer.buffer, &[copy_input]);
        }

        // Memory barrier for compute shader
        let input_barrier = ash::vk::BufferMemoryBarrier::builder()
            .src_access_mask(ash::vk::AccessFlags::TRANSFER_WRITE)
            .dst_access_mask(ash::vk::AccessFlags::SHADER_READ)
            .src_queue_family_index(self.queue_family_index)
            .dst_queue_family_index(self.queue_family_index)
            .buffer(input_buffer.buffer)
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

        // Bind pipeline and descriptor set
        unsafe { self.device.cmd_bind_pipeline(command_buffer, ash::vk::PipelineBindPoint::COMPUTE, self.compute_pipeline) };

        // Create and update descriptor set
        let descriptor_set = self.create_descriptor_set(input_buffer, output_buffer)?;
        unsafe { self.device.cmd_bind_descriptor_sets(
            command_buffer,
            ash::vk::PipelineBindPoint::COMPUTE,
            self.pipeline_layout,
            0,
            &[descriptor_set],
            &[],
        ) };

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
            .buffer(output_buffer.buffer)
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

        // Copy output back to staging
        let copy_output = ash::vk::BufferCopy::builder().size(output_size);
        unsafe {
            self.device.cmd_copy_buffer(command_buffer, output_buffer.buffer, *staging_output, &[copy_output]);
        }

        // End command buffer
        unsafe { self.device.end_command_buffer(command_buffer)? };

        // Submit and wait
        let submit_info = ash::vk::SubmitInfo::builder().command_buffers(&[command_buffer]);
        unsafe {
            self.device.queue_submit(self.queue, &[submit_info], ash::vk::Fence::null())?;
            self.device.queue_wait_idle(self.queue)?;
        }

        // Cleanup command buffer
        unsafe { self.device.free_command_buffers(self.command_pool, &[command_buffer]) };

        Ok(())
    }

    fn create_descriptor_set(
        &self,
        input_buffer: &BufferInfo,
        output_buffer: &BufferInfo,
    ) -> Result<ash::vk::DescriptorSet, Box<dyn std::error::Error>> {
        // Create descriptor pool
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

        let descriptor_pool = unsafe { self.device.create_descriptor_pool(&pool_create_info, None)? };

        // Allocate descriptor set
        let alloc_info = ash::vk::DescriptorSetAllocateInfo::builder()
            .descriptor_pool(descriptor_pool)
            .set_layouts(&[self.descriptor_set_layout]);

        let descriptor_set = unsafe { self.device.allocate_descriptor_sets(&alloc_info)?[0] };

        // Update descriptor set
        let input_info = ash::vk::DescriptorBufferInfo::builder()
            .buffer(input_buffer.buffer)
            .offset(0)
            .range(input_buffer.size)
            .build();

        let output_info = ash::vk::DescriptorBufferInfo::builder()
            .buffer(output_buffer.buffer)
            .offset(0)
            .range(output_buffer.size)
            .build();

        let uniform_info = ash::vk::DescriptorBufferInfo::builder()
            .buffer(self.create_uniform_buffer()?)
            .offset(0)
            .range(std::mem::size_of::<VulkanUniformData>() as u64)
            .build();

        let writes = [
            ash::vk::WriteDescriptorSet::builder()
                .dst_set(descriptor_set)
                .dst_binding(0)
                .descriptor_type(ash::vk::DescriptorType::STORAGE_BUFFER)
                .buffer_info(&[input_info])
                .build(),
            ash::vk::WriteDescriptorSet::builder()
                .dst_set(descriptor_set)
                .dst_binding(1)
                .descriptor_type(ash::vk::DescriptorType::STORAGE_BUFFER)
                .buffer_info(&[output_info])
                .build(),
            ash::vk::WriteDescriptorSet::builder()
                .dst_set(descriptor_set)
                .dst_binding(2)
                .descriptor_type(ash::vk::DescriptorType::UNIFORM_BUFFER)
                .buffer_info(&[uniform_info])
                .build(),
        ];

        unsafe { self.device.update_descriptor_sets(&writes, &[]) };

        // Store pool for cleanup
        // TODO: Store descriptor pool for cleanup

        Ok(descriptor_set)
    }

    fn create_uniform_buffer(&self) -> Result<ash::vk::Buffer, Box<dyn std::error::Error>> {
        let memory_properties = unsafe {
            self.instance.get_physical_device_memory_properties(self.physical_device)
        };

        let uniform_size = std::mem::size_of::<VulkanUniformData>() as u64;
        let buffer_create_info = ash::vk::BufferCreateInfo::builder()
            .size(uniform_size)
            .usage(ash::vk::BufferUsageFlags::UNIFORM_BUFFER | ash::vk::BufferUsageFlags::TRANSFER_DST)
            .sharing_mode(ash::vk::SharingMode::EXCLUSIVE);

        let buffer = unsafe { self.device.create_buffer(&buffer_create_info, None)? };
        let memory_req = unsafe { self.device.get_buffer_memory_requirements(buffer) };

        let memory_type_index = Self::find_memory_type(
            &memory_properties,
            ash::vk::MemoryPropertyFlags::DEVICE_LOCAL,
            ash::vk::MemoryPropertyFlags::DEVICE_LOCAL | ash::vk::MemoryPropertyFlags::HOST_VISIBLE,
        ).ok_or("No suitable uniform buffer memory")?;

        let memory_allocate_info = ash::vk::MemoryAllocateInfo::builder()
            .allocation_size(memory_req.size)
            .memory_type_index(memory_type_index);

        let memory = unsafe { self.device.allocate_memory(&memory_allocate_info, None)? };
        unsafe { self.device.bind_buffer_memory(buffer, memory, 0)? };

        Ok(buffer)
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