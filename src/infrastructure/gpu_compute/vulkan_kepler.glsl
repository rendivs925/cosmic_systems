#version 450
#extension GL_KHR_shader_subgroup_basic : enable

layout(local_size_x = 64) in;

// Input orbital data - extended for moons
layout(binding = 0) buffer OrbitalData {
    // Planets: orbital_elements[i] = vec4(semi_major_axis, eccentricity, mean_anomaly, iterations)
    // Moons: orbital_elements[i] = vec4(semi_major_axis, eccentricity, mean_anomaly, iterations)
    vec4 orbital_elements[];
};

// Input moon orbital elements (only used for moons)
layout(binding = 3) buffer MoonElements {
    // For moons: vec4(inclination, long_asc_node, arg_periapsis, is_moon_flag)
    vec4 moon_params[];
};

// Output position data
layout(binding = 1) buffer ResultData {
    vec4 results[]; // xyz=position, w=convergence_status
};

// Uniform constants
layout(binding = 2) uniform Constants {
    uint planet_count;
    float time_step;
    uint quality_level;
    uint moon_start_index; // Index where moons begin in the orbital_elements array
};

// Kepler equation solver using Newton-Raphson method
vec3 solve_kepler_planet(vec3 orbital_params, uint max_iterations) {
    float a = orbital_params.x;  // semi-major axis
    float e = orbital_params.y;  // eccentricity
    float M = orbital_params.z;  // mean anomaly

    // Initial guess (for near-circular orbits, E ≈ M)
    float E = M;

    // Newton-Raphson iterations
    for(uint i = 0; i < max_iterations; ++i) {
        float sin_E = sin(E);
        float cos_E = cos(E);

        // Kepler equation: M = E - e*sin(E)
        // f(E) = E - e*sin(E) - M
        float f = E - e * sin_E - M;

        // f'(E) = 1 - e*cos(E)
        float f_prime = 1.0 - e * cos_E;

        // Avoid division by zero
        if(abs(f_prime) < 1e-6) {
            break;
        }

        // Newton step: E = E - f/f'
        float delta = f / f_prime;
        E -= delta;

        // Check convergence
        if(abs(delta) < 1e-8) {
            break;
        }
    }

    // Calculate position in orbital plane
    float cos_E = cos(E);
    float sin_E = sin(E);

    // Distance from focus: r = a * (1 - e*cos(E))
    float r = a * (1.0 - e * cos_E);

    // Position in orbital plane (true anomaly approximation)
    // For simplicity, approximate true anomaly θ ≈ E for near-circular orbits
    float x = r * cos_E;  // cos(θ) ≈ cos(E) for small e
    float z = r * sin_E;  // sin(θ) ≈ sin(E) for small e

    return vec3(x, 0.0, z);
}

// Kepler equation solver for moons with full orbital elements
vec3 solve_kepler_moon(vec3 orbital_params, vec4 moon_params, uint max_iterations) {
    float a = orbital_params.x;  // semi-major axis
    float e = orbital_params.y;  // eccentricity
    float M = orbital_params.z;  // mean anomaly

    // Moon orbital elements
    float i = moon_params.x;  // inclination
    float Ω = moon_params.y;  // longitude of ascending node
    float ω = moon_params.z;  // argument of periapsis

    // Initial guess (for near-circular orbits, E ≈ M)
    float E = M;

    // Newton-Raphson iterations
    for(uint i = 0; i < max_iterations; ++i) {
        float sin_E = sin(E);
        float cos_E = cos(E);

        // Kepler equation: M = E - e*sin(E)
        // f(E) = E - e*sin(E) - M
        float f = E - e * sin_E - M;

        // f'(E) = 1 - e*cos(E)
        float f_prime = 1.0 - e * cos_E;

        // Avoid division by zero
        if(abs(f_prime) < 1e-6) {
            break;
        }

        // Newton step: E = E - f/f'
        float delta = f / f_prime;
        E -= delta;

        // Check convergence
        if(abs(delta) < 1e-8) {
            break;
        }
    }

    // Calculate true anomaly: θ = 2*atan(sqrt((1+e)/(1-e)) * tan(E/2))
    // For simplicity, approximate θ ≈ E for near-circular orbits
    float cos_E = cos(E);
    float sin_E = sin(E);

    // Distance from focus: r = a * (1 - e*cos(E))
    float r = a * (1.0 - e * cos_E);

    // Position in orbital plane
    float x_orb = r * cos_E;
    float z_orb = r * sin_E;

    // Transform to 3D space using orbital elements
    // This matches the CPU implementation in transform_orbital_point
    float cos_Ω = cos(Ω);
    float sin_Ω = sin(Ω);
    float cos_i = cos(i);
    float sin_i = sin(i);
    float cos_ω = cos(ω);
    float sin_ω = sin(ω);

    // Rotation matrices for orbital element transformation
    float x = x_orb * (cos_ω * cos_Ω - sin_ω * sin_Ω * cos_i) -
              z_orb * (sin_ω * cos_Ω + cos_ω * sin_Ω * cos_i);
    float y = x_orb * (cos_ω * sin_Ω + sin_ω * cos_Ω * cos_i) +
              z_orb * (sin_ω * sin_Ω - cos_ω * cos_Ω * cos_i);
    float z = x_orb * (sin_ω * sin_i) + z_orb * (cos_ω * sin_i);

    return vec3(x, y, z);
}

void main() {
    uint global_id = gl_GlobalInvocationID.x;

    if(global_id >= planet_count) {
        return;
    }

    // Load orbital parameters
    vec4 orbital_data = orbital_elements[global_id];
    vec3 orbital_params = orbital_data.xyz;
    uint iterations = floatBitsToUint(orbital_data.w);

    // Adjust iterations based on quality level
    uint max_iterations = iterations;
    switch(quality_level) {
        case 0: max_iterations = min(iterations, 2u);  break; // Minimal
        case 1: max_iterations = min(iterations, 4u);  break; // Low
        case 2: max_iterations = min(iterations, 6u);  break; // Medium
        case 3: max_iterations = min(iterations, 8u);  break; // High
        case 4: max_iterations = min(iterations, 12u); break; // Ultra
    }

    vec3 position;

    // Check if this is a moon (moon_start_index indicates where moons begin)
    if(global_id >= moon_start_index) {
        // This is a moon - use full orbital element transformation
        vec4 moon_data = moon_params[global_id - moon_start_index];
        position = solve_kepler_moon(orbital_params, moon_data, max_iterations);
        // Scale for moon orbits (different scaling factor)
        position *= 10.0;
    } else {
        // This is a planet - use simplified orbital calculation
        position = solve_kepler_planet(orbital_params, max_iterations);
        // Scale for planet orbits (AU to scene units)
        position *= 100.0;
    }

    // Store result
    results[global_id] = vec4(position, 1.0); // w=1.0 indicates success
}