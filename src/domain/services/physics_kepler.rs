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

/// Adaptive Kepler solver based on eccentricity.
pub fn solve_kepler_adaptive(mean_anomaly: f32, eccentricity: f32, max_iterations: u32) -> f32 {
    if eccentricity < 0.8 {
        let mut e = mean_anomaly;
        for _ in 0..max_iterations {
            let f = e - eccentricity * e.sin() - mean_anomaly;
            let f_prime = 1.0 - eccentricity * e.cos();
            e -= f / f_prime;
        }
        e
    } else {
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
