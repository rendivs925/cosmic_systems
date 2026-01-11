// SIMD feature detection
#[cfg(target_arch = "x86_64")]
use std::is_x86_feature_detected;
#[cfg(feature = "parallel")]
use rayon::prelude::*;

use bevy::math::Vec3;

#[derive(Debug, Clone, Copy)]
pub enum CpuFeatureLevel {
    Scalar,
    SSE4,
    AVX2,
    AVX512,
}

pub fn detect_cpu_features() -> CpuFeatureLevel {
    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx512f") {
            CpuFeatureLevel::AVX512
        } else if is_x86_feature_detected!("avx2") {
            CpuFeatureLevel::AVX2
        } else if is_x86_feature_detected!("sse4.1") {
            CpuFeatureLevel::SSE4
        } else {
            CpuFeatureLevel::Scalar
        }
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        CpuFeatureLevel::Scalar
    }
}



/// AVX-512 Kepler solver - processes 16 equations simultaneously
#[cfg(all(feature = "simd", target_arch = "x86_64", target_feature = "avx512f"))]
fn solve_kepler_avx512_batch(
    mean_anomalies: &[f32],
    eccentricities: &[f32],
    max_iterations: u32,
) -> Vec<f32> {
    use std::arch::x86_64::*;

    let mut results = Vec::with_capacity(mean_anomalies.len());

    // Process in chunks of 16 for AVX-512
    for chunk in mean_anomalies.chunks(16).zip(eccentricities.chunks(16)) {
        let (ma_chunk, e_chunk) = chunk;

        // Load data into AVX-512 registers (pad with zeros if needed)
        let mut ma_data = [0.0f32; 16];
        let mut e_data = [0.0f32; 16];

        for i in 0..ma_chunk.len() {
            ma_data[i] = ma_chunk[i];
            e_data[i] = e_chunk[i];
        }

        let ma_vec = unsafe { _mm512_loadu_ps(ma_data.as_ptr()) };
        let e_vec = unsafe { _mm512_loadu_ps(e_data.as_ptr()) };

        // Newton-Raphson iteration using AVX-512
        let mut e_anomaly = ma_vec; // Initial guess

        for _ in 0..max_iterations.min(8) {
            // Calculate f = E - e*sin(E) - M
            // f' = 1 - e*cos(E)

            // Approximate sin and cos using polynomial series
            let sin_e = avx512_sin_approx(e_anomaly);
            let cos_e = avx512_cos_approx(e_anomaly);

            let e_sin_e = unsafe { _mm512_mul_ps(e_vec, sin_e) };
            let e_cos_e = unsafe { _mm512_mul_ps(e_vec, cos_e) };

            let f = unsafe {
                _mm512_sub_ps(
                    _mm512_sub_ps(e_anomaly, e_sin_e),
                    ma_vec
                )
            };

            let f_prime = unsafe {
                _mm512_sub_ps(
                    _mm512_set1_ps(1.0),
                    e_cos_e
                )
            };

            // delta = f / f'
            let delta = unsafe { _mm512_div_ps(f, f_prime) };

            // E = E - delta
            e_anomaly = unsafe { _mm512_sub_ps(e_anomaly, delta) };
        }

        // Store results
        let mut result_data = [0.0f32; 16];
        unsafe { _mm512_storeu_ps(result_data.as_mut_ptr(), e_anomaly); }

        for &result in &result_data[..ma_chunk.len()] {
            results.push(result);
        }
    }

    results
}

#[cfg(all(feature = "simd", not(all(target_arch = "x86_64", target_feature = "avx512f"))))]
fn solve_kepler_avx512_batch(
    mean_anomalies: &[f32],
    eccentricities: &[f32],
    max_iterations: u32,
) -> Vec<f32> {
    solve_kepler_scalar_batch(mean_anomalies, eccentricities, max_iterations)
}

/// AVX-256 Kepler solver - processes 8 equations simultaneously
#[cfg(all(feature = "simd", target_arch = "x86_64", target_feature = "avx2"))]
fn solve_kepler_avx2_batch(
    mean_anomalies: &[f32],
    eccentricities: &[f32],
    max_iterations: u32,
) -> Vec<f32> {
    use std::arch::x86_64::*;

    let mut results = Vec::with_capacity(mean_anomalies.len());

    // Process in chunks of 8 for AVX2
    for chunk in mean_anomalies.chunks(8).zip(eccentricities.chunks(8)) {
        let (ma_chunk, e_chunk) = chunk;

        // Load data into AVX2 registers (pad with zeros if needed)
        let mut ma_data = [0.0f32; 8];
        let mut e_data = [0.0f32; 8];

        for i in 0..ma_chunk.len() {
            ma_data[i] = ma_chunk[i];
            e_data[i] = e_chunk[i];
        }

        let ma_vec = unsafe { _mm256_loadu_ps(ma_data.as_ptr()) };
        let e_vec = unsafe { _mm256_loadu_ps(e_data.as_ptr()) };

        // Newton-Raphson iteration using AVX2
        let mut e_anomaly = ma_vec; // Initial guess

        for _ in 0..max_iterations.min(6) {
            // Polynomial approximations for sin/cos
            let sin_e = avx2_sin_approx(e_anomaly);
            let cos_e = avx2_cos_approx(e_anomaly);

            let e_sin_e = unsafe { _mm256_mul_ps(e_vec, sin_e) };
            let e_cos_e = unsafe { _mm256_mul_ps(e_vec, cos_e) };

            let f = unsafe {
                _mm256_sub_ps(
                    _mm256_sub_ps(e_anomaly, e_sin_e),
                    ma_vec
                )
            };

            let f_prime = unsafe {
                _mm256_sub_ps(
                    _mm256_set1_ps(1.0),
                    e_cos_e
                )
            };

            // delta = f / f'
            let delta = unsafe { _mm256_div_ps(f, f_prime) };

            // E = E - delta
            e_anomaly = unsafe { _mm256_sub_ps(e_anomaly, delta) };
        }

        // Store results
        let mut result_data = [0.0f32; 8];
        unsafe { _mm256_storeu_ps(result_data.as_mut_ptr(), e_anomaly); }

        for &result in &result_data[..ma_chunk.len()] {
            results.push(result);
        }
    }

    results
}

#[cfg(all(feature = "simd", not(all(target_arch = "x86_64", target_feature = "avx2"))))]
fn solve_kepler_avx2_batch(
    mean_anomalies: &[f32],
    eccentricities: &[f32],
    max_iterations: u32,
) -> Vec<f32> {
    solve_kepler_scalar_batch(mean_anomalies, eccentricities, max_iterations)
}

/// SSE4 Kepler solver - processes 4 equations simultaneously
#[cfg(all(feature = "simd", target_arch = "x86_64", target_feature = "sse4.1"))]
fn solve_kepler_sse4_batch(
    mean_anomalies: &[f32],
    eccentricities: &[f32],
    max_iterations: u32,
) -> Vec<f32> {
    use std::arch::x86_64::*;

    let mut results = Vec::with_capacity(mean_anomalies.len());

    // Process in chunks of 4 for SSE4
    for chunk in mean_anomalies.chunks(4).zip(eccentricities.chunks(4)) {
        let (ma_chunk, e_chunk) = chunk;

        // Load data into SSE registers
        let mut ma_data = [0.0f32; 4];
        let mut e_data = [0.0f32; 4];

        for i in 0..ma_chunk.len() {
            ma_data[i] = ma_chunk[i];
            e_data[i] = e_chunk[i];
        }

        let ma_vec = unsafe { _mm_loadu_ps(ma_data.as_ptr()) };
        let e_vec = unsafe { _mm_loadu_ps(e_data.as_ptr()) };

        // Newton-Raphson iteration using SSE4
        let mut e_anomaly = ma_vec; // Initial guess

        for _ in 0..max_iterations.min(4) {
            // Simple approximation: E ≈ M + e*sin(M) for near-circular orbits
            let sin_approx = unsafe { _mm_mul_ps(e_anomaly, _mm_set1_ps(0.8415)) }; // sin(x) ≈ x * 0.8415
            let e_sin = unsafe { _mm_mul_ps(e_vec, sin_approx) };
            let correction = unsafe { _mm_add_ps(ma_vec, e_sin) };
            e_anomaly = correction;
        }

        // Store results
        let mut result_data = [0.0f32; 4];
        unsafe { _mm_storeu_ps(result_data.as_mut_ptr(), e_anomaly); }

        for &result in &result_data[..ma_chunk.len()] {
            results.push(result);
        }
    }

    results
}

#[cfg(all(feature = "simd", not(all(target_arch = "x86_64", target_feature = "sse4.1"))))]
fn solve_kepler_sse4_batch(
    mean_anomalies: &[f32],
    eccentricities: &[f32],
    max_iterations: u32,
) -> Vec<f32> {
    solve_kepler_scalar_batch(mean_anomalies, eccentricities, max_iterations)
}

/// AVX-512 Kepler solver - processes 16 equations simultaneously
#[cfg(all(feature = "simd", target_arch = "x86_64", target_feature = "avx512f"))]
fn solve_kepler_avx512_batch(
    mean_anomalies: &[f32],
    eccentricities: &[f32],
    max_iterations: u32,
) -> Vec<f32> {
    use std::arch::x86_64::*;

    let mut results = Vec::with_capacity(mean_anomalies.len());

    // Process in chunks of 16 for AVX-512
    for chunk in mean_anomalies.chunks(16).zip(eccentricities.chunks(16)) {
        let (ma_chunk, e_chunk) = chunk;

        // Load data into AVX-512 registers (pad with zeros if needed)
        let mut ma_data = [0.0f32; 16];
        let mut e_data = [0.0f32; 16];

        for i in 0..ma_chunk.len() {
            ma_data[i] = ma_chunk[i];
            e_data[i] = e_chunk[i];
        }

        let ma_vec = unsafe { _mm512_loadu_ps(ma_data.as_ptr()) };
        let e_vec = unsafe { _mm512_loadu_ps(e_data.as_ptr()) };

        // Newton-Raphson iteration using AVX-512
        let mut e_anomaly = ma_vec; // Initial guess

        for _ in 0..max_iterations.min(8) {
            // Calculate f = E - e*sin(E) - M
            // f' = 1 - e*cos(E)

            // Approximate sin and cos using polynomial series
            let sin_e = avx512_sin_approx(e_anomaly);
            let cos_e = avx512_cos_approx(e_anomaly);

            let e_sin_e = unsafe { _mm512_mul_ps(e_vec, sin_e) };
            let e_cos_e = unsafe { _mm512_mul_ps(e_vec, cos_e) };

            let f = unsafe {
                _mm512_sub_ps(
                    _mm512_sub_ps(e_anomaly, e_sin_e),
                    ma_vec
                )
            };

            let f_prime = unsafe {
                _mm512_sub_ps(
                    _mm512_set1_ps(1.0),
                    e_cos_e
                )
            };

            // delta = f / f'
            let delta = unsafe { _mm512_div_ps(f, f_prime) };

            // E = E - delta
            e_anomaly = unsafe { _mm512_sub_ps(e_anomaly, delta) };
        }

        // Store results
        let mut result_data = [0.0f32; 16];
        unsafe { _mm512_storeu_ps(result_data.as_mut_ptr(), e_anomaly); }

        for &result in &result_data[..ma_chunk.len()] {
            results.push(result);
        }
    }

    results
}

#[cfg(all(feature = "simd", not(all(target_arch = "x86_64", target_feature = "avx512f"))))]
fn solve_kepler_avx512_batch(
    mean_anomalies: &[f32],
    eccentricities: &[f32],
    max_iterations: u32,
) -> Vec<f32> {
    solve_kepler_scalar_batch(mean_anomalies, eccentricities, max_iterations)
}

/// AVX-256 Kepler solver - processes 8 equations simultaneously
#[cfg(all(feature = "simd", target_arch = "x86_64", target_feature = "avx2"))]
fn solve_kepler_avx2_batch(
    mean_anomalies: &[f32],
    eccentricities: &[f32],
    max_iterations: u32,
) -> Vec<f32> {
    use std::arch::x86_64::*;

    let mut results = Vec::with_capacity(mean_anomalies.len());

    // Process in chunks of 8 for AVX2
    for chunk in mean_anomalies.chunks(8).zip(eccentricities.chunks(8)) {
        let (ma_chunk, e_chunk) = chunk;

        // Load data into AVX2 registers (pad with zeros if needed)
        let mut ma_data = [0.0f32; 8];
        let mut e_data = [0.0f32; 8];

        for i in 0..ma_chunk.len() {
            ma_data[i] = ma_chunk[i];
            e_data[i] = e_chunk[i];
        }

        let ma_vec = unsafe { _mm256_loadu_ps(ma_data.as_ptr()) };
        let e_vec = unsafe { _mm256_loadu_ps(e_data.as_ptr()) };

        // Newton-Raphson iteration using AVX2
        let mut e_anomaly = ma_vec; // Initial guess

        for _ in 0..max_iterations.min(6) {
            // Polynomial approximations for sin/cos
            let sin_e = avx2_sin_approx(e_anomaly);
            let cos_e = avx2_cos_approx(e_anomaly);

            let e_sin_e = unsafe { _mm256_mul_ps(e_vec, sin_e) };
            let e_cos_e = unsafe { _mm256_mul_ps(e_vec, cos_e) };

            let f = unsafe {
                _mm256_sub_ps(
                    _mm256_sub_ps(e_anomaly, e_sin_e),
                    ma_vec
                )
            };

            let f_prime = unsafe {
                _mm256_sub_ps(
                    _mm256_set1_ps(1.0),
                    e_cos_e
                )
            };

            // delta = f / f'
            let delta = unsafe { _mm256_div_ps(f, f_prime) };

            // E = E - delta
            e_anomaly = unsafe { _mm256_sub_ps(e_anomaly, delta) };
        }

        // Store results
        let mut result_data = [0.0f32; 8];
        unsafe { _mm256_storeu_ps(result_data.as_mut_ptr(), e_anomaly); }

        for &result in &result_data[..ma_chunk.len()] {
            results.push(result);
        }
    }

    results
}

#[cfg(all(feature = "simd", not(all(target_arch = "x86_64", target_feature = "avx2"))))]
fn solve_kepler_avx2_batch(
    mean_anomalies: &[f32],
    eccentricities: &[f32],
    max_iterations: u32,
) -> Vec<f32> {
    solve_kepler_scalar_batch(mean_anomalies, eccentricities, max_iterations)
}

/// SSE4 Kepler solver - processes 4 equations simultaneously
#[cfg(all(feature = "simd", target_arch = "x86_64", target_feature = "sse4.1"))]
fn solve_kepler_sse4_batch(
    mean_anomalies: &[f32],
    eccentricities: &[f32],
    max_iterations: u32,
) -> Vec<f32> {
    use std::arch::x86_64::*;

    let mut results = Vec::with_capacity(mean_anomalies.len());

    // Process in chunks of 4 for SSE4
    for chunk in mean_anomalies.chunks(4).zip(eccentricities.chunks(4)) {
        let (ma_chunk, e_chunk) = chunk;

        // Load data into SSE registers
        let mut ma_data = [0.0f32; 4];
        let mut e_data = [0.0f32; 4];

        for i in 0..ma_chunk.len() {
            ma_data[i] = ma_chunk[i];
            e_data[i] = e_chunk[i];
        }

        let ma_vec = unsafe { _mm_loadu_ps(ma_data.as_ptr()) };
        let e_vec = unsafe { _mm_loadu_ps(e_data.as_ptr()) };

        // Newton-Raphson iteration using SSE4
        let mut e_anomaly = ma_vec; // Initial guess

        for _ in 0..max_iterations.min(4) {
            // Simple approximation: E ≈ M + e*sin(M) for near-circular orbits
            let sin_approx = unsafe { _mm_mul_ps(e_anomaly, _mm_set1_ps(0.8415)) }; // sin(x) ≈ x * 0.8415
            let e_sin = unsafe { _mm_mul_ps(e_vec, sin_approx) };
            let correction = unsafe { _mm_add_ps(ma_vec, e_sin) };
            e_anomaly = correction;
        }

        // Store results
        let mut result_data = [0.0f32; 4];
        unsafe { _mm_storeu_ps(result_data.as_mut_ptr(), e_anomaly); }

        for &result in &result_data[..ma_chunk.len()] {
            results.push(result);
        }
    }

    results
}

#[cfg(all(feature = "simd", not(all(target_arch = "x86_64", target_feature = "sse4.1"))))]
fn solve_kepler_sse4_batch(
    mean_anomalies: &[f32],
    eccentricities: &[f32],
    max_iterations: u32,
) -> Vec<f32> {
    solve_kepler_scalar_batch(mean_anomalies, eccentricities, max_iterations)
}

/// Scalar fallback implementation
fn solve_kepler_scalar_batch(
    mean_anomalies: &[f32],
    eccentricities: &[f32],
    max_iterations: u32,
) -> Vec<f32> {
    mean_anomalies
        .iter()
        .zip(eccentricities.iter())
        .map(|(&ma, &e)| solve_kepler_adaptive(ma, e, max_iterations))
        .collect()
}

/// AVX-512 sine approximation using Taylor series
#[cfg(all(feature = "simd", target_feature = "avx512f"))]
fn avx512_sin_approx(x: std::arch::x86_64::__m512) -> std::arch::x86_64::__m512 {
    use std::arch::x86_64::*;

    let x2 = unsafe { _mm512_mul_ps(x, x) };
    let x3 = unsafe { _mm512_mul_ps(x2, x) };
    let x5 = unsafe { _mm512_mul_ps(x3, x2) };

    let term1 = x;
    let term2 = unsafe { _mm512_div_ps(x3, _mm512_set1_ps(6.0)) };
    let term3 = unsafe { _mm512_div_ps(x5, _mm512_set1_ps(120.0)) };

    unsafe { _mm512_sub_ps(_mm512_sub_ps(term1, term2), term3) }
}

/// AVX-512 cosine approximation using Taylor series
#[cfg(all(feature = "simd", target_feature = "avx512f"))]
fn avx512_cos_approx(x: std::arch::x86_64::__m512) -> std::arch::x86_64::__m512 {
    use std::arch::x86_64::*;

    let x2 = unsafe { _mm512_mul_ps(x, x) };
    let x4 = unsafe { _mm512_mul_ps(x2, x2) };

    let term1 = unsafe { _mm512_set1_ps(1.0) };
    let term2 = unsafe { _mm512_div_ps(x2, _mm512_set1_ps(2.0)) };
    let term3 = unsafe { _mm512_div_ps(x4, _mm512_set1_ps(24.0)) };

    unsafe { _mm512_add_ps(_mm512_sub_ps(term1, term2), term3) }
}

/// AVX-256 sine approximation
#[cfg(all(feature = "simd", target_feature = "avx2"))]
fn avx2_sin_approx(x: std::arch::x86_64::__m256) -> std::arch::x86_64::__m256 {
    use std::arch::x86_64::*;

    let x2 = unsafe { _mm256_mul_ps(x, x) };
    let x3 = unsafe { _mm256_mul_ps(x2, x) };

    let term1 = x;
    let term2 = unsafe { _mm256_div_ps(x3, _mm256_set1_ps(6.0)) };

    unsafe { _mm256_sub_ps(term1, term2) }
}

/// AVX-256 cosine approximation
#[cfg(all(feature = "simd", target_feature = "avx2"))]
fn avx2_cos_approx(x: std::arch::x86_64::__m256) -> std::arch::x86_64::__m256 {
    use std::arch::x86_64::*;

    let x2 = unsafe { _mm256_mul_ps(x, x) };
    let term1 = unsafe { _mm256_set1_ps(1.0) };
    let term2 = unsafe { _mm256_div_ps(x2, _mm256_set1_ps(2.0)) };

    unsafe { _mm256_sub_ps(term1, term2) }
}

/// SIMD-accelerated orbital transformation matrix operations
/// Vectorized 3D transformations for orbital point calculations using AVX2/AVX-512
#[cfg(feature = "simd")]
pub fn transform_orbital_points_simd(
    points: &[(f32, f32)],                // (x_orbital, z_orbital) pairs
    orbital_elements: &[(f32, f32, f32)], // (inclination, long_asc_node, arg_periapsis)
) -> Vec<Vec3> {
    // Determine CPU capabilities for SIMD dispatch
    let cpu_features = detect_cpu_features();

    match cpu_features {
        CpuFeatureLevel::AVX512 => transform_orbital_points_avx512(points, orbital_elements),
        CpuFeatureLevel::AVX2 => transform_orbital_points_avx2(points, orbital_elements),
        _ => transform_orbital_points_scalar(points, orbital_elements),
    }
}

/// AVX-512 implementation for orbital point transformations
#[cfg(all(feature = "simd", target_arch = "x86_64", target_feature = "avx512f"))]
fn transform_orbital_points_avx512(
    points: &[(f32, f32)],
    orbital_elements: &[(f32, f32, f32)],
) -> Vec<Vec3> {
    use std::arch::x86_64::*;

    let mut results = Vec::with_capacity(points.len());

    // Process in chunks of 16 for AVX-512
    for chunk in points.chunks(16).zip(orbital_elements.chunks(16)) {
        let (point_chunk, element_chunk) = chunk;

        // Prepare data arrays (pad with zeros)
        let mut x_data = [0.0f32; 16];
        let mut z_data = [0.0f32; 16];
        let mut inc_data = [0.0f32; 16];
        let mut lan_data = [0.0f32; 16];
        let mut ap_data = [0.0f32; 16];

        for i in 0..point_chunk.len() {
            x_data[i] = point_chunk[i].0;
            z_data[i] = point_chunk[i].1;
            inc_data[i] = element_chunk[i].0;
            lan_data[i] = element_chunk[i].1;
            ap_data[i] = element_chunk[i].2;
        }

        // Load into AVX-512 registers
        let x_vec = unsafe { _mm512_loadu_ps(x_data.as_ptr()) };
        let z_vec = unsafe { _mm512_loadu_ps(z_data.as_ptr()) };
        let inc_vec = unsafe { _mm512_loadu_ps(inc_data.as_ptr()) };
        let lan_vec = unsafe { _mm512_loadu_ps(lan_data.as_ptr()) };
        let ap_vec = unsafe { _mm512_loadu_ps(ap_data.as_ptr()) };

        // Calculate trigonometric functions for rotation matrices
        let sin_inc = avx512_sin_approx(inc_vec);
        let cos_inc = avx512_cos_approx(inc_vec);
        let sin_lan = avx512_sin_approx(lan_vec);
        let cos_lan = avx512_cos_approx(lan_vec);
        let sin_ap = avx512_sin_approx(ap_vec);
        let cos_ap = avx512_cos_approx(ap_vec);

        // Apply argument of periapsis rotation (around Z axis)
        // x' = x*cos(ap) - z*sin(ap)
        // z' = x*sin(ap) + z*cos(ap)
        let x_ap = unsafe {
            _mm512_sub_ps(
                _mm512_mul_ps(x_vec, cos_ap),
                _mm512_mul_ps(z_vec, sin_ap)
            )
        };
        let z_ap = unsafe {
            _mm512_add_ps(
                _mm512_mul_ps(x_vec, sin_ap),
                _mm512_mul_ps(z_vec, cos_ap)
            )
        };

        // Apply longitude of ascending node rotation (around Z axis)
        // x'' = x'*cos(lan) - z'*sin(lan)
        // z'' = x'*sin(lan) + z'*cos(lan)
        let x_lan = unsafe {
            _mm512_sub_ps(
                _mm512_mul_ps(x_ap, cos_lan),
                _mm512_mul_ps(z_ap, sin_lan)
            )
        };
        let z_lan = unsafe {
            _mm512_add_ps(
                _mm512_mul_ps(x_ap, sin_lan),
                _mm512_mul_ps(z_ap, cos_lan)
            )
        };

        // Apply inclination rotation (around X axis)
        // y'' = z''*sin(inc)
        // z''' = z''*cos(inc)
        let y_inc = unsafe { _mm512_mul_ps(z_lan, sin_inc) };
        let z_inc = unsafe { _mm512_mul_ps(z_lan, cos_inc) };

        // Store results
        let mut x_results = [0.0f32; 16];
        let mut y_results = [0.0f32; 16];
        let mut z_results = [0.0f32; 16];

        unsafe {
            _mm512_storeu_ps(x_results.as_mut_ptr(), x_lan);
            _mm512_storeu_ps(y_results.as_mut_ptr(), y_inc);
            _mm512_storeu_ps(z_results.as_mut_ptr(), z_inc);
        }

        for i in 0..point_chunk.len() {
            results.push(Vec3::new(x_results[i], y_results[i], z_results[i]));
        }
    }

    results
}

#[cfg(all(feature = "simd", not(all(target_arch = "x86_64", target_feature = "avx512f"))))]
fn transform_orbital_points_avx512(
    points: &[(f32, f32)],
    orbital_elements: &[(f32, f32, f32)],
) -> Vec<Vec3> {
    transform_orbital_points_scalar(points, orbital_elements)
}

/// AVX-256 implementation for orbital point transformations
#[cfg(all(feature = "simd", target_arch = "x86_64", target_feature = "avx2"))]
fn transform_orbital_points_avx2(
    points: &[(f32, f32)],
    orbital_elements: &[(f32, f32, f32)],
) -> Vec<Vec3> {
    use std::arch::x86_64::*;

    let mut results = Vec::with_capacity(points.len());

    // Process in chunks of 8 for AVX2
    for chunk in points.chunks(8).zip(orbital_elements.chunks(8)) {
        let (point_chunk, element_chunk) = chunk;

        // Prepare data arrays (pad with zeros)
        let mut x_data = [0.0f32; 8];
        let mut z_data = [0.0f32; 8];
        let mut inc_data = [0.0f32; 8];
        let mut lan_data = [0.0f32; 8];
        let mut ap_data = [0.0f32; 8];

        for i in 0..point_chunk.len() {
            x_data[i] = point_chunk[i].0;
            z_data[i] = point_chunk[i].1;
            inc_data[i] = element_chunk[i].0;
            lan_data[i] = element_chunk[i].1;
            ap_data[i] = element_chunk[i].2;
        }

        // Load into AVX2 registers
        let x_vec = unsafe { _mm256_loadu_ps(x_data.as_ptr()) };
        let z_vec = unsafe { _mm256_loadu_ps(z_data.as_ptr()) };
        let inc_vec = unsafe { _mm256_loadu_ps(inc_data.as_ptr()) };
        let lan_vec = unsafe { _mm256_loadu_ps(lan_data.as_ptr()) };
        let ap_vec = unsafe { _mm256_loadu_ps(ap_data.as_ptr()) };

        // Calculate trigonometric functions
        let sin_inc = avx2_sin_approx(inc_vec);
        let cos_inc = avx2_cos_approx(inc_vec);
        let sin_lan = avx2_sin_approx(lan_vec);
        let cos_lan = avx2_cos_approx(lan_vec);
        let sin_ap = avx2_sin_approx(ap_vec);
        let cos_ap = avx2_cos_approx(ap_vec);

        // Apply rotations (same logic as AVX-512 but with 256-bit vectors)
        let x_ap = unsafe {
            _mm256_sub_ps(
                _mm256_mul_ps(x_vec, cos_ap),
                _mm256_mul_ps(z_vec, sin_ap)
            )
        };
        let z_ap = unsafe {
            _mm256_add_ps(
                _mm256_mul_ps(x_vec, sin_ap),
                _mm256_mul_ps(z_vec, cos_ap)
            )
        };

        let x_lan = unsafe {
            _mm256_sub_ps(
                _mm256_mul_ps(x_ap, cos_lan),
                _mm256_mul_ps(z_ap, sin_lan)
            )
        };
        let z_lan = unsafe {
            _mm256_add_ps(
                _mm256_mul_ps(x_ap, sin_lan),
                _mm256_mul_ps(z_ap, cos_lan)
            )
        };

        let y_inc = unsafe { _mm256_mul_ps(z_lan, sin_inc) };
        let z_inc = unsafe { _mm256_mul_ps(z_lan, cos_inc) };

        // Store results
        let mut x_results = [0.0f32; 8];
        let mut y_results = [0.0f32; 8];
        let mut z_results = [0.0f32; 8];

        unsafe {
            _mm256_storeu_ps(x_results.as_mut_ptr(), x_lan);
            _mm256_storeu_ps(y_results.as_mut_ptr(), y_inc);
            _mm256_storeu_ps(z_results.as_mut_ptr(), z_inc);
        }

        for i in 0..point_chunk.len() {
            results.push(Vec3::new(x_results[i], y_results[i], z_results[i]));
        }
    }

    results
}

#[cfg(all(feature = "simd", not(all(target_arch = "x86_64", target_feature = "avx2"))))]
fn transform_orbital_points_avx2(
    points: &[(f32, f32)],
    orbital_elements: &[(f32, f32, f32)],
) -> Vec<Vec3> {
    transform_orbital_points_scalar(points, orbital_elements)
}

/// Scalar fallback for orbital point transformations
fn transform_orbital_points_scalar(
    points: &[(f32, f32)],
    orbital_elements: &[(f32, f32, f32)],
) -> Vec<Vec3> {
    use crate::domain::services::physics_orbital::transform_orbital_point;

    points
        .iter()
        .zip(orbital_elements.iter())
        .map(|(&(x_orb, z_orb), &(inc, lan, ap))| {
            transform_orbital_point(x_orb, z_orb, inc, lan, ap)
        })
        .collect()
}

/// Parallel orbital position calculation using Rayon
#[cfg(feature = "parallel")]
pub fn calculate_planet_positions_parallel(
    planets: &[(Planet, Vec3, Option<f32>, u32)], // (planet, parent_pos, parent_tilt, kepler_iterations)
    time_days: f32,
    solar_params: &SolarSystemParameters,
) -> Vec<Vec3> {
    use crate::domain::services::physics_orbital::calculate_planet_position_with_quality;

    planets
        .par_iter()
        .map(|(planet, parent_pos, parent_tilt, kepler_iterations)| {
            calculate_planet_position_with_quality(
                planet,
                time_days,
                solar_params,
                *parent_pos,
                *parent_tilt,
                *kepler_iterations,
            )
        })
        .collect()
}

/// Adaptive Kepler solver based on eccentricity
pub fn solve_kepler_adaptive(mean_anomaly: f32, eccentricity: f32, max_iterations: u32) -> f32 {
    // For now, delegate to a simple implementation - will be replaced with SIMD versions
    if eccentricity < 0.8 {
        // Low eccentricity - use Newton-Raphson
        let mut e = mean_anomaly;
        for _ in 0..max_iterations {
            let f = e - eccentricity * e.sin() - mean_anomaly;
            let f_prime = 1.0 - eccentricity * e.cos();
            e -= f / f_prime;
        }
        e
    } else {
        // High eccentricity - use binary search
        let mut low = -std::f32::consts::PI;
        let mut high = std::f32::consts::PI;
        for _ in 0..max_iterations {
            let mid = (low + high) / 2.0;
            let f = mid - eccentricity * mid.sin() - mean_anomaly;
            if f > 0.0 {
                high = mid;
            } else {
                low = mid;
            }
        }
        (low + high) / 2.0
    }
}

pub fn get_kepler_iterations_for_distance(distance_to_camera: f32) -> u32 {
    // Adaptive Kepler solver iterations based on distance to camera
    // Closer objects need more precision, farther objects can use fewer iterations
    if distance_to_camera < 1000.0 {
        16 // High precision for close objects
    } else if distance_to_camera < 10000.0 {
        12
    } else if distance_to_camera < 50000.0 {
        8
    } else {
        6 // Low precision for distant objects
    }
}
pub fn solve_kepler_simd_batch(
    mean_anomalies: &[f32],
    eccentricities: &[f32],
    max_iterations: u32,
    cpu_features: CpuFeatureLevel,
) -> Vec<f32> {
    // Dispatch to appropriate SIMD implementation
    match cpu_features {
        CpuFeatureLevel::AVX512 => solve_kepler_avx512_batch(mean_anomalies, eccentricities, max_iterations),
        CpuFeatureLevel::AVX2 => solve_kepler_avx2_batch(mean_anomalies, eccentricities, max_iterations),
        CpuFeatureLevel::SSE4 => solve_kepler_sse4_batch(mean_anomalies, eccentricities, max_iterations),
        CpuFeatureLevel::Scalar => solve_kepler_scalar_batch(mean_anomalies, eccentricities, max_iterations),
    }
}
