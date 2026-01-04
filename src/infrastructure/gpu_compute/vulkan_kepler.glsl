#version 450
#extension GL_KHR_shader_subgroup_basic : enable

layout(local_size_x = 64) in;

// Input orbital data
layout(binding = 0) buffer OrbitalData {
    vec4 orbital_elements[]; // x=semi_major_axis, y=eccentricity, z=mean_anomaly, w=iterations
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
    uint padding;
};

// Kepler equation solver using Newton-Raphson method
vec3 solve_kepler(vec3 orbital_params, uint max_iterations) {
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

    // Solve Kepler equation
    vec3 position = solve_kepler(orbital_params, max_iterations);

    // Scale for visualization (AU to scene units)
    position *= 100.0;

    // Store result
    results[global_id] = vec4(position, 1.0); // w=1.0 indicates success
}