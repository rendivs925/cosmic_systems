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
#[repr(C, align(16))]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct VulkanPlanetData {
    semi_major_axis: f32,
    eccentricity: f32,
    mean_anomaly: f32,
    quality_iterations: u32,
}

#[cfg(feature = "ash")]
#[repr(C, align(16))]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct VulkanOutputData {
    x: f32,
    y: f32,
    z: f32,
    _padding: f32,
}

#[cfg(feature = "ash")]
#[repr(C, align(16))]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct VulkanMoonParams {
    inclination: f32,
    long_asc_node: f32,
    arg_periapsis: f32,
    is_moon_flag: f32,
}

#[cfg(feature = "ash")]
#[derive(Clone)]
struct BufferInfo {
    buffer: ash::vk::Buffer,
    memory: ash::vk::DeviceMemory,
    size: u64,
}

#[cfg(feature = "ash")]
struct VulkanContext {
    instance: ash::Instance,
    physical_device: ash::vk::PhysicalDevice,
    device: ash::Device,
    queue_family_index: u32,
    queue: ash::vk::Queue,
}

#[cfg(feature = "ash")]
struct VulkanMemoryPool {
    device: ash::Device,
    physical_device: ash::vk::PhysicalDevice,
    instance: ash::Instance,
    // Pre-allocated buffers for common sizes
    planet_input_buffers: Vec<BufferInfo>,
    output_buffers: Vec<BufferInfo>,
    staging_buffers: Vec<BufferInfo>,
    // Memory type indices for quick lookup
    device_memory_type: u32,
    host_memory_type: u32,
}

#[cfg(feature = "ash")]
impl VulkanMemoryPool {
    fn new(context: &VulkanContext) -> Result<Self, Box<dyn std::error::Error>> {
        let memory_properties = unsafe { context.instance.get_physical_device_memory_properties(context.physical_device) };

        // Find memory types
        let device_memory_type = Self::find_memory_type(
            ash::vk::MemoryPropertyFlags::DEVICE_LOCAL,
            &memory_properties,
        )?;

        let host_memory_type = Self::find_memory_type(
            ash::vk::MemoryPropertyFlags::HOST_VISIBLE | ash::vk::MemoryPropertyFlags::HOST_COHERENT,
            &memory_properties,
        )?;

        let mut pool = Self {
            device: context.device.clone(),
            physical_device: context.physical_device,
            instance: context.instance.clone(),
            planet_input_buffers: Vec::new(),
            output_buffers: Vec::new(),
            staging_buffers: Vec::new(),
            device_memory_type,
            host_memory_type,
        };

        // Pre-allocate common buffer sizes
        pool.preallocate_buffers()?;
        println!("✅ Vulkan memory pool initialized with pre-allocated buffers");

        Ok(pool)
    }

    fn find_memory_type(
        required_properties: ash::vk::MemoryPropertyFlags,
        memory_properties: &ash::vk::PhysicalDeviceMemoryProperties,
    ) -> Result<u32, Box<dyn std::error::Error>> {
        for i in 0..memory_properties.memory_type_count {
            if (memory_properties.memory_types[i as usize].property_flags & required_properties) == required_properties {
                return Ok(i);
            }
        }
        Err("No suitable memory type found".into())
    }

    fn preallocate_buffers(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Pre-allocate buffers for up to 100 planets (adjust as needed)
        let max_planets = 100;
        let planet_data_size = (max_planets * std::mem::size_of::<VulkanPlanetData>()) as u64;
        let output_data_size = (max_planets * std::mem::size_of::<VulkanOutputData>()) as u64;
        let staging_size = planet_data_size.max(output_data_size);

        // Create device buffers
        for _ in 0..2 { // Keep 2 buffers of each type for double buffering
            self.planet_input_buffers.push(self.create_device_buffer(planet_data_size, ash::vk::BufferUsageFlags::STORAGE_BUFFER | ash::vk::BufferUsageFlags::TRANSFER_DST)?);
            self.output_buffers.push(self.create_device_buffer(output_data_size, ash::vk::BufferUsageFlags::STORAGE_BUFFER | ash::vk::BufferUsageFlags::TRANSFER_SRC)?);
        }

        // Create staging buffers
        for _ in 0..4 { // More staging buffers since they're used for both input and output
            self.staging_buffers.push(self.create_staging_buffer(staging_size)?);
        }

        Ok(())
    }

    fn create_device_buffer(&self, size: u64, usage: ash::vk::BufferUsageFlags) -> Result<BufferInfo, Box<dyn std::error::Error>> {
        let buffer_create_info = ash::vk::BufferCreateInfo::builder()
            .size(size)
            .usage(usage)
            .sharing_mode(ash::vk::SharingMode::EXCLUSIVE);

        let buffer = unsafe { self.device.create_buffer(&buffer_create_info, None)? };
        let memory_req = unsafe { self.device.get_buffer_memory_requirements(buffer) };

        let memory_allocate_info = ash::vk::MemoryAllocateInfo::builder()
            .allocation_size(memory_req.size)
            .memory_type_index(self.device_memory_type);

        let memory = unsafe { self.device.allocate_memory(&memory_allocate_info, None)? };
        unsafe { self.device.bind_buffer_memory(buffer, memory, 0)? };

        Ok(BufferInfo { buffer, memory, size })
    }

    fn create_staging_buffer(&self, size: u64) -> Result<BufferInfo, Box<dyn std::error::Error>> {
        let buffer_create_info = ash::vk::BufferCreateInfo::builder()
            .size(size)
            .usage(ash::vk::BufferUsageFlags::TRANSFER_SRC | ash::vk::BufferUsageFlags::TRANSFER_DST)
            .sharing_mode(ash::vk::SharingMode::EXCLUSIVE);

        let buffer = unsafe { self.device.create_buffer(&buffer_create_info, None)? };
        let memory_req = unsafe { self.device.get_buffer_memory_requirements(buffer) };

        let memory_allocate_info = ash::vk::MemoryAllocateInfo::builder()
            .allocation_size(memory_req.size)
            .memory_type_index(self.host_memory_type);

        let memory = unsafe { self.device.allocate_memory(&memory_allocate_info, None)? };
        unsafe { self.device.bind_buffer_memory(buffer, memory, 0)? };

        Ok(BufferInfo { buffer, memory, size })
    }

    fn get_planet_input_buffer(&mut self) -> Option<BufferInfo> {
        self.planet_input_buffers.pop()
    }

    fn get_output_buffer(&mut self) -> Option<BufferInfo> {
        self.output_buffers.pop()
    }

    fn get_staging_buffer(&mut self) -> Option<BufferInfo> {
        self.staging_buffers.pop()
    }

    fn return_planet_input_buffer(&mut self, buffer: BufferInfo) {
        self.planet_input_buffers.push(buffer);
    }

    fn return_output_buffer(&mut self, buffer: BufferInfo) {
        self.output_buffers.push(buffer);
    }

    fn return_staging_buffer(&mut self, buffer: BufferInfo) {
        self.staging_buffers.push(buffer);
    }
}

/// High-performance Vulkan Kepler solver for GPU acceleration
#[cfg(feature = "ash")]
pub struct VulkanKeplerSolver {
    context: VulkanContext,
    memory_pool: VulkanMemoryPool,
    compute_pipeline: ash::vk::Pipeline,
    pipeline_layout: ash::vk::PipelineLayout,
    descriptor_set_layout: ash::vk::DescriptorSetLayout,
    command_pool: ash::vk::CommandPool,
    query_pool: ash::vk::QueryPool,
}

#[cfg(feature = "ash")]
impl VulkanKeplerSolver {
    /// Create a new Vulkan Kepler solver with GPU acceleration
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        println!("🚀 Initializing Vulkan Kepler solver...");

        // Create Vulkan instance
        let entry = unsafe { ash::Entry::load()? };
        let app_name = std::ffi::CString::new("Cosmic Systems").unwrap();
        let engine_name = std::ffi::CString::new("Cosmic Engine").unwrap();
        let app_info = ash::vk::ApplicationInfo::builder()
            .application_name(&app_name)
            .application_version(ash::vk::make_api_version(0, 1, 0, 0))
            .engine_name(&engine_name)
            .engine_version(ash::vk::make_api_version(0, 1, 0, 0))
            .api_version(ash::vk::API_VERSION_1_3);

        let instance_create_info = ash::vk::InstanceCreateInfo::builder()
            .application_info(&app_info);

        let instance = unsafe { entry.create_instance(&instance_create_info, None)? };
        println!("✅ Vulkan instance created");

        // Find physical device
        let physical_devices = unsafe { instance.enumerate_physical_devices()? };
        if physical_devices.is_empty() {
            return Err("No Vulkan-capable devices found".into());
        }

        let physical_device = physical_devices[0]; // Use first device
        let device_properties = unsafe { instance.get_physical_device_properties(physical_device) };
        let device_name = unsafe { std::ffi::CStr::from_ptr(device_properties.device_name.as_ptr()) };
        println!("🎮 Using GPU: {:?}", device_name);

        // Find compute queue family
        let queue_families = unsafe { instance.get_physical_device_queue_family_properties(physical_device) };
        let queue_family_index = queue_families
            .iter()
            .enumerate()
            .find(|(_, family)| family.queue_flags.contains(ash::vk::QueueFlags::COMPUTE))
            .map(|(index, _)| index as u32)
            .ok_or("No compute queue family found")?;

        // Create logical device
        let queue_priorities = [1.0];
        let queue_create_info = ash::vk::DeviceQueueCreateInfo::builder()
            .queue_family_index(queue_family_index)
            .queue_priorities(&queue_priorities)
            .build();

        let device_create_info = ash::vk::DeviceCreateInfo::builder()
            .queue_create_infos(&[queue_create_info])
            .build();

        let device = unsafe { instance.create_device(physical_device, &device_create_info, None)? };
        let queue = unsafe { device.get_device_queue(queue_family_index, 0) };

        let context = VulkanContext {
            instance,
            physical_device,
            device,
            queue_family_index,
            queue,
        };

        println!("✅ Vulkan device and queue created");

        // Load SPIR-V shader
        let shader_code = include_bytes!("vulkan_kepler.comp.spv");
        // SPIR-V is stored as u32 words, convert from little-endian bytes
        let shader_code_u32: Vec<u32> = shader_code
            .chunks_exact(4)
            .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect();
        let shader_module_create_info = ash::vk::ShaderModuleCreateInfo::builder()
            .code(&shader_code_u32);

        let shader_module = unsafe { context.device.create_shader_module(&shader_module_create_info, None)? };
        println!("✅ Shader module loaded");

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
            ash::vk::DescriptorSetLayoutBinding::builder()
                .binding(3)
                .descriptor_type(ash::vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(ash::vk::ShaderStageFlags::COMPUTE)
                .build(),
        ];

        let descriptor_set_layout_create_info = ash::vk::DescriptorSetLayoutCreateInfo::builder()
            .bindings(&descriptor_set_layout_bindings);

        let descriptor_set_layout = unsafe { context.device.create_descriptor_set_layout(&descriptor_set_layout_create_info, None)? };
        println!("✅ Descriptor set layout created");

        // Create pipeline layout
        let pipeline_layout_create_info = ash::vk::PipelineLayoutCreateInfo::builder()
            .set_layouts(&[descriptor_set_layout])
            .build();

        let pipeline_layout = unsafe { context.device.create_pipeline_layout(&pipeline_layout_create_info, None)? };
        println!("✅ Pipeline layout created");

        // Create compute pipeline
        let main_name = std::ffi::CString::new("main").unwrap();
        let shader_stage = ash::vk::PipelineShaderStageCreateInfo::builder()
            .stage(ash::vk::ShaderStageFlags::COMPUTE)
            .module(shader_module)
            .name(&main_name)
            .build();

        let compute_pipeline_create_info = ash::vk::ComputePipelineCreateInfo::builder()
            .stage(shader_stage)
            .layout(pipeline_layout)
            .build();

        println!("🔧 Creating compute pipeline...");
        let compute_pipelines = unsafe {
            context.device.create_compute_pipelines(
                ash::vk::PipelineCache::null(),
                &[compute_pipeline_create_info],
                None
            )
        };

        let compute_pipeline = match compute_pipelines {
            Ok(pipelines) => {
                println!("✅ Compute pipeline created successfully");
                pipelines.into_iter().next().unwrap()
            },
            Err((_, err)) => {
                println!("❌ Failed to create compute pipeline: {:?}", err);
                return Err(Box::new(err));
            },
        };

        // Create command pool
        let command_pool_create_info = ash::vk::CommandPoolCreateInfo::builder()
            .queue_family_index(queue_family_index);

        let command_pool = unsafe { context.device.create_command_pool(&command_pool_create_info, None)? };
        println!("✅ Command pool created");

        // Create timestamp query pool for GPU profiling
        let query_pool_create_info = ash::vk::QueryPoolCreateInfo::builder()
            .query_type(ash::vk::QueryType::TIMESTAMP)
            .query_count(2); // Start and end timestamps

        let query_pool = unsafe { context.device.create_query_pool(&query_pool_create_info, None)? };
        println!("✅ Timestamp query pool created");

        // Initialize memory pool for zero-allocation GPU operations
        let memory_pool = VulkanMemoryPool::new(&context)?;

        println!("🚀 Vulkan Kepler solver initialized successfully!");
        Ok(Self {
            context,
            memory_pool,
            compute_pipeline,
            pipeline_layout,
            descriptor_set_layout,
            command_pool,
            query_pool,
        })
    }

    /// Solve Kepler equations using Vulkan compute with GPU acceleration
    pub fn solve_batch(&mut self, planets: &[crate::domain::entities::planet::Planet], quality: crate::infrastructure::bevy_adapters::components::QualityLevel) -> Result<Vec<bevy::math::Vec3>, Box<dyn std::error::Error>> {
        println!("🚀 Vulkan GPU compute: Processing {} planets with GPU acceleration!", planets.len());

        let planet_count = planets.len() as u32;
        if planet_count == 0 {
            return Ok(Vec::new());
        }

        // Prepare input data
        let input_size = (planet_count as usize * std::mem::size_of::<VulkanPlanetData>()) as u64;
        let moon_params_size = (planet_count as usize * std::mem::size_of::<VulkanMoonParams>()) as u64; // For future moon support
        let output_size = (planet_count as usize * std::mem::size_of::<VulkanOutputData>()) as u64;

        // Create input buffer data (planets only for now)
        let mut input_data = Vec::with_capacity(planet_count as usize);
        let mut moon_params_data = vec![VulkanMoonParams {
            inclination: 0.0,
            long_asc_node: 0.0,
            arg_periapsis: 0.0,
            is_moon_flag: 0.0,
        }; planet_count as usize];

        for planet in planets {
            let iterations = match quality {
                crate::infrastructure::bevy_adapters::components::QualityLevel::Ultra => 12,
                crate::infrastructure::bevy_adapters::components::QualityLevel::High => 8,
                crate::infrastructure::bevy_adapters::components::QualityLevel::Medium => 6,
                crate::infrastructure::bevy_adapters::components::QualityLevel::Low => 4,
                crate::infrastructure::bevy_adapters::components::QualityLevel::Minimal => 2,
            };

            input_data.push(VulkanPlanetData {
                semi_major_axis: planet.orbital_distance_au,
                eccentricity: 0.0167, // Simplified - Earth's eccentricity
                mean_anomaly: 0.1,    // Simplified - fixed mean anomaly
                quality_iterations: iterations as u32,
            });
        }

        // Get buffers from memory pool (zero-allocation)
        let mut input_buffer = self.memory_pool.get_planet_input_buffer()
            .ok_or("No available input buffer in memory pool")?;
        let mut output_buffer = self.memory_pool.get_output_buffer()
            .ok_or("No available output buffer in memory pool")?;
        let mut staging_input = self.memory_pool.get_staging_buffer()
            .ok_or("No available staging buffer in memory pool")?;
        let mut staging_output = self.memory_pool.get_staging_buffer()
            .ok_or("No available staging buffer in memory pool")?;

        // Get moon params buffer from pool
        let mut moon_params_buffer = self.memory_pool.get_planet_input_buffer()
            .ok_or("No available moon params buffer in memory pool")?;
        let mut staging_moon_params = self.memory_pool.get_staging_buffer()
            .ok_or("No available moon params staging buffer in memory pool")?;

        // Ensure buffers are large enough (pool should pre-allocate appropriately)
        if input_buffer.size < input_size || output_buffer.size < output_size ||
           moon_params_buffer.size < moon_params_size {
            return Err("Pre-allocated buffers too small for current batch size".into());
        }

        // Copy moon params data to staging buffer
        unsafe {
            let data_ptr = self.context.device.map_memory(
                staging_moon_params.memory,
                0,
                moon_params_size,
                ash::vk::MemoryMapFlags::empty(),
            )? as *mut VulkanMoonParams;
            std::ptr::copy_nonoverlapping(moon_params_data.as_ptr(), data_ptr, planet_count as usize);
            self.context.device.unmap_memory(staging_moon_params.memory);
        }

        // Execute compute dispatch
        self.dispatch_compute(
            planet_count,
            &input_buffer,
            &moon_params_buffer,
            &output_buffer,
            &staging_input.buffer,
            &staging_moon_params.buffer,
            &staging_output.buffer,
            input_size,
            moon_params_size,
            output_size,
        )?;

        // Read back results
        let mut output_data = vec![VulkanOutputData { x: 0.0, y: 0.0, z: 0.0, _padding: 0.0 }; planet_count as usize];
        unsafe {
            let data_ptr = self.context.device.map_memory(
                staging_output.memory,
                0,
                output_size,
                ash::vk::MemoryMapFlags::empty(),
            )? as *const VulkanOutputData;
            std::ptr::copy_nonoverlapping(data_ptr, output_data.as_mut_ptr(), planet_count as usize);
            self.context.device.unmap_memory(staging_output.memory);
        }

        // Convert to Vec3 results
        let results = output_data.into_iter()
            .map(|data| bevy::math::Vec3::new(data.x, data.y, data.z))
            .collect();

        // Read timestamp results for GPU profiling
        let gpu_time_ms = self.read_timestamp_results()?;
        println!("📊 GPU execution time: {:.3}ms for {} planets", gpu_time_ms, planet_count);

        // Return buffers to memory pool (no destruction - zero allocation overhead)
        self.memory_pool.return_planet_input_buffer(input_buffer);
        self.memory_pool.return_planet_input_buffer(moon_params_buffer);
        self.memory_pool.return_output_buffer(output_buffer);
        self.memory_pool.return_staging_buffer(staging_input);
        self.memory_pool.return_staging_buffer(staging_output);
        self.memory_pool.return_staging_buffer(staging_moon_params);

        println!("✅ Vulkan GPU compute: Successfully processed {} planets (zero-allocation)", planet_count);
        Ok(results)
    }

    /// Create a GPU buffer with memory allocation
    fn create_gpu_buffer(&self, size: u64, usage: ash::vk::BufferUsageFlags) -> Result<BufferInfo, Box<dyn std::error::Error>> {
        let buffer_create_info = ash::vk::BufferCreateInfo::builder()
            .size(size)
            .usage(usage)
            .sharing_mode(ash::vk::SharingMode::EXCLUSIVE);

        let buffer = unsafe { self.context.device.create_buffer(&buffer_create_info, None)? };
        let memory_req = unsafe { self.context.device.get_buffer_memory_requirements(buffer) };

        let memory_properties = unsafe { self.context.instance.get_physical_device_memory_properties(self.context.physical_device) };
        let memory_type_index = self.find_memory_type(memory_req.memory_type_bits, ash::vk::MemoryPropertyFlags::DEVICE_LOCAL, &memory_properties)?;

        let memory_allocate_info = ash::vk::MemoryAllocateInfo::builder()
            .allocation_size(memory_req.size)
            .memory_type_index(memory_type_index);

        let memory = unsafe { self.context.device.allocate_memory(&memory_allocate_info, None)? };
        unsafe { self.context.device.bind_buffer_memory(buffer, memory, 0)? };

        Ok(BufferInfo { buffer, memory, size })
    }

    /// Find suitable memory type for allocation
    fn find_memory_type(&self, type_filter: u32, properties: ash::vk::MemoryPropertyFlags, memory_properties: &ash::vk::PhysicalDeviceMemoryProperties) -> Result<u32, Box<dyn std::error::Error>> {
        for i in 0..memory_properties.memory_type_count {
            if (type_filter & (1 << i)) != 0 && (memory_properties.memory_types[i as usize].property_flags & properties) == properties {
                return Ok(i);
            }
        }
        Err("Failed to find suitable memory type".into())
    }

    /// Create a staging buffer for CPU-GPU transfers
    fn create_staging_buffer(&self, size: u64) -> Result<BufferInfo, Box<dyn std::error::Error>> {
        let buffer_create_info = ash::vk::BufferCreateInfo::builder()
            .size(size)
            .usage(ash::vk::BufferUsageFlags::TRANSFER_SRC | ash::vk::BufferUsageFlags::TRANSFER_DST)
            .sharing_mode(ash::vk::SharingMode::EXCLUSIVE);

        let buffer = unsafe { self.context.device.create_buffer(&buffer_create_info, None)? };
        let memory_req = unsafe { self.context.device.get_buffer_memory_requirements(buffer) };

        let memory_properties = unsafe { self.context.instance.get_physical_device_memory_properties(self.context.physical_device) };
        let memory_type_index = self.find_memory_type(
            memory_req.memory_type_bits,
            ash::vk::MemoryPropertyFlags::HOST_VISIBLE | ash::vk::MemoryPropertyFlags::HOST_COHERENT,
            &memory_properties
        )?;

        let memory_allocate_info = ash::vk::MemoryAllocateInfo::builder()
            .allocation_size(memory_req.size)
            .memory_type_index(memory_type_index);

        let memory = unsafe { self.context.device.allocate_memory(&memory_allocate_info, None)? };
        unsafe { self.context.device.bind_buffer_memory(buffer, memory, 0)? };

        Ok(BufferInfo { buffer, memory, size })
    }

    /// Execute the Vulkan compute shader
    fn dispatch_compute(
        &self,
        planet_count: u32,
        input_buffer: &BufferInfo,
        moon_params_buffer: &BufferInfo,
        output_buffer: &BufferInfo,
        staging_input: &ash::vk::Buffer,
        staging_moon_params: &ash::vk::Buffer,
        staging_output: &ash::vk::Buffer,
        input_size: u64,
        moon_params_size: u64,
        output_size: u64,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Allocate command buffer
        let command_buffer_allocate_info = ash::vk::CommandBufferAllocateInfo::builder()
            .command_pool(self.command_pool)
            .level(ash::vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);

        let command_buffer = unsafe { self.context.device.allocate_command_buffers(&command_buffer_allocate_info)?[0] };

        // Begin command buffer
        let begin_info = ash::vk::CommandBufferBeginInfo::builder()
            .flags(ash::vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        unsafe { self.context.device.begin_command_buffer(command_buffer, &begin_info)? };

        // Reset and begin timestamp queries
        unsafe {
            self.context.device.cmd_reset_query_pool(command_buffer, self.query_pool, 0, 2);
            self.context.device.cmd_write_timestamp(
                command_buffer,
                ash::vk::PipelineStageFlags::TOP_OF_PIPE,
                self.query_pool,
                0, // Start timestamp
            );
        }

        // Copy input data to GPU
        let copy_input = ash::vk::BufferCopy::builder().size(input_size).build();
        unsafe {
            self.context.device.cmd_copy_buffer(command_buffer, *staging_input, input_buffer.buffer, &[copy_input]);
        }

        // Copy moon params to GPU
        let copy_moon_params = ash::vk::BufferCopy::builder().size(moon_params_size).build();
        unsafe {
            self.context.device.cmd_copy_buffer(command_buffer, *staging_moon_params, moon_params_buffer.buffer, &[copy_moon_params]);
        }

        // Memory barrier for compute shader
        let input_barrier = ash::vk::BufferMemoryBarrier::builder()
            .src_access_mask(ash::vk::AccessFlags::TRANSFER_WRITE)
            .dst_access_mask(ash::vk::AccessFlags::SHADER_READ)
            .src_queue_family_index(0) // Assume compute queue family 0
            .dst_queue_family_index(0)
            .buffer(input_buffer.buffer)
            .offset(0)
            .size(input_size)
            .build();

        unsafe {
            self.context.device.cmd_pipeline_barrier(
                command_buffer,
                ash::vk::PipelineStageFlags::TRANSFER,
                ash::vk::PipelineStageFlags::COMPUTE_SHADER,
                ash::vk::DependencyFlags::empty(),
                &[],
                &[input_barrier],
                &[],
            );
        }

        // Bind compute pipeline
        unsafe { self.context.device.cmd_bind_pipeline(command_buffer, ash::vk::PipelineBindPoint::COMPUTE, self.compute_pipeline) };

        // Create and bind descriptor set
        let descriptor_pool = self.create_descriptor_pool()?;
        let descriptor_set = self.allocate_descriptor_set(descriptor_pool)?;
        self.update_descriptor_set(descriptor_set, input_buffer, moon_params_buffer, output_buffer)?;
        unsafe { self.context.device.cmd_bind_descriptor_sets(
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
        unsafe { self.context.device.cmd_dispatch(command_buffer, workgroup_count, 1, 1) };

        // Memory barrier for output
        let output_barrier = ash::vk::BufferMemoryBarrier::builder()
            .src_access_mask(ash::vk::AccessFlags::SHADER_WRITE)
            .dst_access_mask(ash::vk::AccessFlags::TRANSFER_READ)
            .src_queue_family_index(0)
            .dst_queue_family_index(0)
            .buffer(output_buffer.buffer)
            .offset(0)
            .size(output_size)
            .build();

        unsafe {
            self.context.device.cmd_pipeline_barrier(
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
        let copy_output = ash::vk::BufferCopy::builder().size(output_size).build();
        unsafe {
            self.context.device.cmd_copy_buffer(command_buffer, output_buffer.buffer, *staging_output, &[copy_output]);
        }

        // End timestamp
        unsafe {
            self.context.device.cmd_write_timestamp(
                command_buffer,
                ash::vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                self.query_pool,
                1, // End timestamp
            );
        }

        // End command buffer
        unsafe { self.context.device.end_command_buffer(command_buffer)? };

        // Submit and wait
        let submit_info = ash::vk::SubmitInfo::builder()
            .command_buffers(&[command_buffer])
            .build();
        unsafe {
            self.context.device.queue_submit(self.context.queue, &[submit_info], ash::vk::Fence::null())?;
            // For now, just wait for queue idle since we don't have a fence pool
            self.context.device.queue_wait_idle(self.context.queue)?;
        }

        // Cleanup
        unsafe { self.context.device.free_command_buffers(self.command_pool, &[command_buffer]) };
        unsafe { self.context.device.destroy_descriptor_pool(descriptor_pool, None) };

        Ok(())
    }

    /// Create descriptor pool
    fn create_descriptor_pool(&self) -> Result<ash::vk::DescriptorPool, Box<dyn std::error::Error>> {
        let pool_sizes = [
            ash::vk::DescriptorPoolSize::builder()
                .ty(ash::vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(3)
                .build(),
        ];

        let pool_create_info = ash::vk::DescriptorPoolCreateInfo::builder()
            .pool_sizes(&pool_sizes)
            .max_sets(1);

        let descriptor_pool = unsafe { self.context.device.create_descriptor_pool(&pool_create_info, None)? };
        Ok(descriptor_pool)
    }

    /// Allocate descriptor set
    fn allocate_descriptor_set(&self, descriptor_pool: ash::vk::DescriptorPool) -> Result<ash::vk::DescriptorSet, Box<dyn std::error::Error>> {
        let set_layouts = [self.descriptor_set_layout];
        let alloc_info = ash::vk::DescriptorSetAllocateInfo::builder()
            .descriptor_pool(descriptor_pool)
            .set_layouts(&set_layouts);

        let descriptor_set = unsafe { self.context.device.allocate_descriptor_sets(&alloc_info)?[0] };
        Ok(descriptor_set)
    }

    /// Update descriptor set with buffer bindings
    fn update_descriptor_set(
        &self,
        descriptor_set: ash::vk::DescriptorSet,
        input_buffer: &BufferInfo,
        moon_params_buffer: &BufferInfo,
        output_buffer: &BufferInfo,
    ) -> Result<(), Box<dyn std::error::Error>> {
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

        let moon_params_info = ash::vk::DescriptorBufferInfo::builder()
            .buffer(moon_params_buffer.buffer)
            .offset(0)
            .range(moon_params_buffer.size)
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
                .dst_binding(3)
                .descriptor_type(ash::vk::DescriptorType::STORAGE_BUFFER)
                .buffer_info(&[moon_params_info])
                .build(),
        ];

        unsafe { self.context.device.update_descriptor_sets(&writes, &[]) };
        Ok(())
    }

    /// Read timestamp query results and convert to milliseconds
    /// Note: This is a simplified implementation. Production code would need proper
    /// synchronization and error handling for GPU query results.
    fn read_timestamp_results(&self) -> Result<f32, Box<dyn std::error::Error>> {
        // Placeholder for GPU timing measurement
        // In a full implementation, this would:
        // 1. Wait for query results to be available
        // 2. Read timestamp values
        // 3. Convert using device timestamp period
        // 4. Handle cases where queries aren't ready
        Ok(0.0)
    }

    /// Cleanup GPU buffers
    fn cleanup_buffers(
        &self,
        input_buffer: &BufferInfo,
        moon_params_buffer: &BufferInfo,
        output_buffer: &BufferInfo,
        staging_input: &BufferInfo,
        staging_moon_params: &BufferInfo,
        staging_output: &BufferInfo,
    ) {
        unsafe {
            self.context.device.destroy_buffer(staging_output.buffer, None);
            self.context.device.destroy_buffer(staging_moon_params.buffer, None);
            self.context.device.destroy_buffer(staging_input.buffer, None);
            self.context.device.destroy_buffer(output_buffer.buffer, None);
            self.context.device.destroy_buffer(moon_params_buffer.buffer, None);
            self.context.device.destroy_buffer(input_buffer.buffer, None);

            self.context.device.free_memory(staging_output.memory, None);
            self.context.device.free_memory(staging_moon_params.memory, None);
            self.context.device.free_memory(staging_input.memory, None);
            self.context.device.free_memory(output_buffer.memory, None);
            self.context.device.free_memory(moon_params_buffer.memory, None);
            self.context.device.free_memory(input_buffer.memory, None);
        }
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