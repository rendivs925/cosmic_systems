#[cfg(target_arch = "x86_64")]
use std::is_x86_feature_detected;

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
