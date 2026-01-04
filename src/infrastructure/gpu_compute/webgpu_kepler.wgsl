struct PlanetInput {
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
};

struct Output {
    x: f32,
    y: f32,
    z: f32,
    _pad: f32,
};

struct Params {
    count: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
};

@group(0) @binding(0) var<storage, read> inputs: array<PlanetInput>;
@group(0) @binding(1) var<storage, read_write> outputs: array<Output>;
@group(0) @binding(2) var<uniform> params: Params;

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let idx = id.x;
    if (idx >= params.count) {
        return;
    }
    let input = inputs[idx];

    var E = input.mean_anomaly_rad;
    var i: u32 = 0u;
    loop {
        if (i >= input.iterations) { break; }
        let f = E - input.eccentricity * sin(E) - input.mean_anomaly_rad;
        let f_prime = 1.0 - input.eccentricity * cos(E);
        E = E - f / f_prime;
        i = i + 1u;
    }

    let cos_E = cos(E);
    let sin_E = sin(E);
    let r_au = input.semi_major_axis_au * (1.0 - input.eccentricity * cos_E);
    var radius = r_au * input.scale_factor;
    if (input.is_moon != 0u) {
        radius = radius * input.moon_scale;
    }

    let cos_theta = (cos_E - input.eccentricity) / (1.0 - input.eccentricity * cos_E);
    let sin_theta = sin_E * sqrt(1.0 - input.eccentricity * input.eccentricity)
        / (1.0 - input.eccentricity * cos_E);

    let x_orbital = radius * cos_theta;
    let z_orbital = radius * sin_theta;

    let cos_w = cos(input.arg_periapsis_rad);
    let sin_w = sin(input.arg_periapsis_rad);
    let x1 = x_orbital * cos_w - z_orbital * sin_w;
    let z1 = x_orbital * sin_w + z_orbital * cos_w;

    let cos_i = cos(input.inclination_rad);
    let sin_i = sin(input.inclination_rad);
    let y2 = z1 * sin_i;
    let z2 = z1 * cos_i;
    let x2 = x1;

    let cos_omega = cos(input.long_asc_node_rad);
    let sin_omega = sin(input.long_asc_node_rad);
    let x3 = x2 * cos_omega - z2 * sin_omega;
    let z3 = x2 * sin_omega + z2 * cos_omega;

    var x = x3;
    var y = y2;
    var z = z3;

    if (input.has_parent_tilt != 0u) {
        let cos_t = cos(input.parent_tilt_rad);
        let sin_t = sin(input.parent_tilt_rad);
        let x_t = x * cos_t - y * sin_t;
        let y_t = x * sin_t + y * cos_t;
        x = x_t;
        y = y_t;
    }

    outputs[idx].x = input.parent_x + x;
    outputs[idx].y = input.parent_y + y;
    outputs[idx].z = input.parent_z + z;
    outputs[idx]._pad = 0.0;
}
