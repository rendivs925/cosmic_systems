// Kepler equation solver compute shader for WebGPU
// Processes multiple orbital calculations in parallel

struct KeplerWorkItem {
    semi_major_axis: f32,
    eccentricity: f32,
    mean_anomaly: f32,
    planet_id: u32,
};

struct KeplerResult {
    planet_id: u32,
    position: vec3<f32>,
    velocity: vec3<f32>,
};

@group(0) @binding(0)
var<storage, read> work_items: array<KeplerWorkItem>;

@group(0) @binding(1)
var<storage, read_write> results: array<KeplerResult>;

@compute @workgroup_size(64)
fn solve_kepler_batch(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let work_item_idx = global_id.x;

    // Bounds check
    if (work_item_idx >= arrayLength(&work_items)) {
        return;
    }

    let work_item = work_items[work_item_idx];

    // Solve Kepler's equation using Newton-Raphson method
    let M = work_item.mean_anomaly;  // Mean anomaly
    let e = work_item.eccentricity;  // Eccentricity
    let a = work_item.semi_major_axis; // Semi-major axis

    // Initial guess for eccentric anomaly
    var E = M;

    // Newton-Raphson iterations (typically 3-5 iterations sufficient)
    for (var i = 0; i < 5; i = i + 1) {
        let f = E - e * sin(E) - M;
        let f_prime = 1.0 - e * cos(E);
        E = E - f / f_prime;
    }

    // Calculate true anomaly
    let cos_E = cos(E);
    let sin_E = sin(E);
    let cos_theta = (cos_E - e) / (1.0 - e * cos_E);
    let sin_theta = sin_E * sqrt(1.0 - e * e) / (1.0 - e * cos_E);

    // Calculate distance from focus
    let r = a * (1.0 - e * cos_E);

    // Convert to 3D position (simplified 2D orbit in XZ plane)
    let x = r * cos_theta;
    let z = r * sin_theta;
    let position = vec3<f32>(x, 0.0, z);

    // Calculate velocity (simplified - vis-viva equation)
    let mu = 1.327e20; // Solar gravitational parameter (m^3/s^2)
    let v_magnitude = sqrt(mu * (2.0 / r - 1.0 / a));

    // Velocity direction (perpendicular to position vector)
    let velocity_dir = vec3<f32>(-sin_theta, 0.0, cos_theta);
    let velocity = velocity_dir * v_magnitude;

    // Store result
    results[work_item_idx] = KeplerResult(
        work_item.planet_id,
        position,
        velocity
    );
}