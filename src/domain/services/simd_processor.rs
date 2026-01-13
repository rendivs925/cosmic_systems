use crate::domain::services::physics_kepler::{CpuFeatureLevel, detect_cpu_features};

/// Safe SIMD processor that encapsulates all unsafe SIMD operations
pub struct SimdProcessor {
    level: CpuFeatureLevel,
}

impl SimdProcessor {
    /// Create a new SIMD processor with automatic feature detection
    pub fn new() -> Self {
        Self {
            level: detect_cpu_features(),
        }
    }

    /// Create a SIMD processor with explicit feature level (for testing)
    pub fn with_level(level: CpuFeatureLevel) -> Self {
        Self { level }
    }

    /// Get the detected CPU feature level
    pub fn feature_level(&self) -> CpuFeatureLevel {
        self.level
    }

    /// Safe interface for batch Kepler equation solving
    pub fn solve_kepler_batch(
        &self,
        mean_anomalies: &[f32],
        eccentricities: &[f32],
        max_iterations: u32,
    ) -> Vec<f32> {
        match self.level {
            #[cfg(all(feature = "simd", target_arch = "x86_64", target_feature = "avx512f"))]
            CpuFeatureLevel::AVX512 => {
                self.solve_kepler_avx512_batch(mean_anomalies, eccentricities, max_iterations)
            }
            #[cfg(all(feature = "simd", target_arch = "x86_64", target_feature = "avx2"))]
            CpuFeatureLevel::AVX2 => {
                self.solve_kepler_avx2_batch(mean_anomalies, eccentricities, max_iterations)
            }
            #[cfg(all(feature = "simd", target_arch = "x86_64", target_feature = "sse4.1"))]
            CpuFeatureLevel::SSE4 => {
                self.solve_kepler_sse4_batch(mean_anomalies, eccentricities, max_iterations)
            }
            _ => {
                self.solve_kepler_scalar_batch(mean_anomalies, eccentricities, max_iterations)
            }
        }
    }

    /// Safe interface for sine approximation
    pub fn sin_approx(&self, x: &[f32]) -> Vec<f32> {
        match self.level {
            #[cfg(all(feature = "simd", target_arch = "x86_64", target_feature = "avx512f"))]
            CpuFeatureLevel::AVX512 => {
                self.avx512_sin_approx_batch(x)
            }
            #[cfg(all(feature = "simd", target_arch = "x86_64", target_feature = "avx2"))]
            CpuFeatureLevel::AVX2 => {
                self.avx2_sin_approx_batch(x)
            }
            #[cfg(all(feature = "simd", target_arch = "x86_64", target_feature = "sse4.1"))]
            CpuFeatureLevel::SSE4 => {
                self.sse4_sin_approx_batch(x)
            }
            _ => {
                x.iter().map(|&val| val.sin()).collect()
            }
        }
    }

    /// Safe interface for cosine approximation
    pub fn cos_approx(&self, x: &[f32]) -> Vec<f32> {
        match self.level {
            #[cfg(all(feature = "simd", target_arch = "x86_64", target_feature = "avx512f"))]
            CpuFeatureLevel::AVX512 => {
                self.avx512_cos_approx_batch(x)
            }
            #[cfg(all(feature = "simd", target_arch = "x86_64", target_feature = "avx2"))]
            CpuFeatureLevel::AVX2 => {
                self.avx2_cos_approx_batch(x)
            }
            #[cfg(all(feature = "simd", target_arch = "x86_64", target_feature = "sse4.1"))]
            CpuFeatureLevel::SSE4 => {
                self.sse4_cos_approx_batch(x)
            }
            _ => {
                x.iter().map(|&val| val.cos()).collect()
            }
        }
    }

    // Private unsafe implementations

    #[cfg(all(feature = "simd", target_arch = "x86_64", target_feature = "avx512f"))]
    fn solve_kepler_avx512_batch(
        &self,
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
                let sin_e = self.avx512_sin_approx(e_anomaly);
                let cos_e = self.avx512_cos_approx(e_anomaly);

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

    #[cfg(all(feature = "simd", target_arch = "x86_64", target_feature = "avx2"))]
    fn solve_kepler_avx2_batch(
        &self,
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
                let sin_e = self.avx2_sin_approx(e_anomaly);
                let cos_e = self.avx2_cos_approx(e_anomaly);

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

    #[cfg(all(feature = "simd", target_arch = "x86_64", target_feature = "sse4.1"))]
    fn solve_kepler_sse4_batch(
        &self,
        mean_anomalies: &[f32],
        eccentricities: &[f32],
        max_iterations: u32,
    ) -> Vec<f32> {
        use std::arch::x86_64::*;

        let mut results = Vec::with_capacity(mean_anomalies.len());

        // Process in chunks of 4 for SSE4
        for chunk in mean_anomalies.chunks(4).zip(eccentricities.chunks(4)) {
            let (ma_chunk, e_chunk) = chunk;

            // Load data into SSE registers (pad with zeros if needed)
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
                // Simple sin/cos approximations for SSE4
                let sin_e = self.sse4_sin_approx(e_anomaly);
                let cos_e = self.sse4_cos_approx(e_anomaly);

                let e_sin_e = unsafe { _mm_mul_ps(e_vec, sin_e) };
                let e_cos_e = unsafe { _mm_mul_ps(e_vec, cos_e) };

                let f = unsafe {
                    _mm_sub_ps(
                        _mm_sub_ps(e_anomaly, e_sin_e),
                        ma_vec
                    )
                };

                let f_prime = unsafe {
                    _mm_sub_ps(
                        _mm_set1_ps(1.0),
                        e_cos_e
                    )
                };

                // delta = f / f'
                let delta = unsafe { _mm_div_ps(f, f_prime) };

                // E = E - delta
                e_anomaly = unsafe { _mm_sub_ps(e_anomaly, delta) };
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

    fn solve_kepler_scalar_batch(
        &self,
        mean_anomalies: &[f32],
        eccentricities: &[f32],
        max_iterations: u32,
    ) -> Vec<f32> {
        mean_anomalies
            .iter()
            .zip(eccentricities.iter())
            .map(|(&ma, &e)| crate::domain::services::physics_kepler::solve_kepler_adaptive(ma, e, max_iterations))
            .collect()
    }

    #[cfg(all(feature = "simd", target_arch = "x86_64", target_feature = "avx512f"))]
    fn avx512_sin_approx(&self, x: std::arch::x86_64::__m512) -> std::arch::x86_64::__m512 {
        use std::arch::x86_64::*;

        let x2 = unsafe { _mm512_mul_ps(x, x) };
        let x3 = unsafe { _mm512_mul_ps(x2, x) };
        let x5 = unsafe { _mm512_mul_ps(x3, x2) };

        let term1 = x;
        let term2 = unsafe { _mm512_div_ps(x3, _mm512_set1_ps(6.0)) };
        let term3 = unsafe { _mm512_div_ps(x5, _mm512_set1_ps(120.0)) };

        unsafe { _mm512_sub_ps(_mm512_sub_ps(term1, term2), term3) }
    }

    #[cfg(all(feature = "simd", target_arch = "x86_64", target_feature = "avx512f"))]
    fn avx512_cos_approx(&self, x: std::arch::x86_64::__m512) -> std::arch::x86_64::__m512 {
        use std::arch::x86_64::*;

        let x2 = unsafe { _mm512_mul_ps(x, x) };
        let x4 = unsafe { _mm512_mul_ps(x2, x2) };

        let term1 = unsafe { _mm512_set1_ps(1.0) };
        let term2 = unsafe { _mm512_div_ps(x2, _mm512_set1_ps(2.0)) };
        let term3 = unsafe { _mm512_div_ps(x4, _mm512_set1_ps(24.0)) };

        unsafe { _mm512_add_ps(_mm512_sub_ps(term1, term2), term3) }
    }

    #[cfg(all(feature = "simd", target_arch = "x86_64", target_feature = "avx512f"))]
    fn avx512_sin_approx_batch(&self, x: &[f32]) -> Vec<f32> {
        use std::arch::x86_64::*;

        let mut results = Vec::with_capacity(x.len());

        for chunk in x.chunks(16) {
            let mut data = [0.0f32; 16];
            for i in 0..chunk.len() {
                data[i] = chunk[i];
            }

            let vec = unsafe { _mm512_loadu_ps(data.as_ptr()) };
            let sin_vec = self.avx512_sin_approx(vec);

            let mut result_data = [0.0f32; 16];
            unsafe { _mm512_storeu_ps(result_data.as_mut_ptr(), sin_vec); }

            for &result in &result_data[..chunk.len()] {
                results.push(result);
            }
        }

        results
    }

    #[cfg(all(feature = "simd", target_arch = "x86_64", target_feature = "avx512f"))]
    fn avx512_cos_approx_batch(&self, x: &[f32]) -> Vec<f32> {
        use std::arch::x86_64::*;

        let mut results = Vec::with_capacity(x.len());

        for chunk in x.chunks(16) {
            let mut data = [0.0f32; 16];
            for i in 0..chunk.len() {
                data[i] = chunk[i];
            }

            let vec = unsafe { _mm512_loadu_ps(data.as_ptr()) };
            let cos_vec = self.avx512_cos_approx(vec);

            let mut result_data = [0.0f32; 16];
            unsafe { _mm512_storeu_ps(result_data.as_mut_ptr(), cos_vec); }

            for &result in &result_data[..chunk.len()] {
                results.push(result);
            }
        }

        results
    }

    #[cfg(all(feature = "simd", target_arch = "x86_64", target_feature = "avx2"))]
    fn avx2_sin_approx(&self, x: std::arch::x86_64::__m256) -> std::arch::x86_64::__m256 {
        use std::arch::x86_64::*;

        let x2 = unsafe { _mm256_mul_ps(x, x) };
        let x3 = unsafe { _mm256_mul_ps(x2, x) };

        let term1 = x;
        let term2 = unsafe { _mm256_div_ps(x3, _mm256_set1_ps(6.0)) };

        unsafe { _mm256_sub_ps(term1, term2) }
    }

    #[cfg(all(feature = "simd", target_arch = "x86_64", target_feature = "avx2"))]
    fn avx2_cos_approx(&self, x: std::arch::x86_64::__m256) -> std::arch::x86_64::__m256 {
        use std::arch::x86_64::*;

        let x2 = unsafe { _mm256_mul_ps(x, x) };
        let term1 = unsafe { _mm256_set1_ps(1.0) };
        let term2 = unsafe { _mm256_div_ps(x2, _mm256_set1_ps(2.0)) };

        unsafe { _mm256_sub_ps(term1, term2) }
    }

    #[cfg(all(feature = "simd", target_arch = "x86_64", target_feature = "avx2"))]
    fn avx2_sin_approx_batch(&self, x: &[f32]) -> Vec<f32> {
        use std::arch::x86_64::*;

        let mut results = Vec::with_capacity(x.len());

        for chunk in x.chunks(8) {
            let mut data = [0.0f32; 8];
            for i in 0..chunk.len() {
                data[i] = chunk[i];
            }

            let vec = unsafe { _mm256_loadu_ps(data.as_ptr()) };
            let sin_vec = self.avx2_sin_approx(vec);

            let mut result_data = [0.0f32; 8];
            unsafe { _mm256_storeu_ps(result_data.as_mut_ptr(), sin_vec); }

            for &result in &result_data[..chunk.len()] {
                results.push(result);
            }
        }

        results
    }

    #[cfg(all(feature = "simd", target_arch = "x86_64", target_feature = "avx2"))]
    fn avx2_cos_approx_batch(&self, x: &[f32]) -> Vec<f32> {
        use std::arch::x86_64::*;

        let mut results = Vec::with_capacity(x.len());

        for chunk in x.chunks(8) {
            let mut data = [0.0f32; 8];
            for i in 0..chunk.len() {
                data[i] = chunk[i];
            }

            let vec = unsafe { _mm256_loadu_ps(data.as_ptr()) };
            let cos_vec = self.avx2_cos_approx(vec);

            let mut result_data = [0.0f32; 8];
            unsafe { _mm256_storeu_ps(result_data.as_mut_ptr(), cos_vec); }

            for &result in &result_data[..chunk.len()] {
                results.push(result);
            }
        }

        results
    }

    #[cfg(all(feature = "simd", target_arch = "x86_64", target_feature = "sse4.1"))]
    fn sse4_sin_approx(&self, x: std::arch::x86_64::__m128) -> std::arch::x86_64::__m128 {
        use std::arch::x86_64::*;

        let x2 = unsafe { _mm_mul_ps(x, x) };
        let x3 = unsafe { _mm_mul_ps(x2, x) };

        let term1 = x;
        let term2 = unsafe { _mm_div_ps(x3, _mm_set1_ps(6.0)) };

        unsafe { _mm_sub_ps(term1, term2) }
    }

    #[cfg(all(feature = "simd", target_arch = "x86_64", target_feature = "sse4.1"))]
    fn sse4_cos_approx(&self, x: std::arch::x86_64::__m128) -> std::arch::x86_64::__m128 {
        use std::arch::x86_64::*;

        let x2 = unsafe { _mm_mul_ps(x, x) };
        let term1 = unsafe { _mm_set1_ps(1.0) };
        let term2 = unsafe { _mm_div_ps(x2, _mm_set1_ps(2.0)) };

        unsafe { _mm_sub_ps(term1, term2) }
    }

    #[cfg(all(feature = "simd", target_arch = "x86_64", target_feature = "sse4.1"))]
    fn sse4_sin_approx_batch(&self, x: &[f32]) -> Vec<f32> {
        use std::arch::x86_64::*;

        let mut results = Vec::with_capacity(x.len());

        for chunk in x.chunks(4) {
            let mut data = [0.0f32; 4];
            for i in 0..chunk.len() {
                data[i] = chunk[i];
            }

            let vec = unsafe { _mm_loadu_ps(data.as_ptr()) };
            let sin_vec = self.sse4_sin_approx(vec);

            let mut result_data = [0.0f32; 4];
            unsafe { _mm_storeu_ps(result_data.as_mut_ptr(), sin_vec); }

            for &result in &result_data[..chunk.len()] {
                results.push(result);
            }
        }

        results
    }

    #[cfg(all(feature = "simd", target_arch = "x86_64", target_feature = "sse4.1"))]
    fn sse4_cos_approx_batch(&self, x: &[f32]) -> Vec<f32> {
        use std::arch::x86_64::*;

        let mut results = Vec::with_capacity(x.len());

        for chunk in x.chunks(4) {
            let mut data = [0.0f32; 4];
            for i in 0..chunk.len() {
                data[i] = chunk[i];
            }

            let vec = unsafe { _mm_loadu_ps(data.as_ptr()) };
            let cos_vec = self.sse4_cos_approx(vec);

            let mut result_data = [0.0f32; 4];
            unsafe { _mm_storeu_ps(result_data.as_mut_ptr(), cos_vec); }

            for &result in &result_data[..chunk.len()] {
                results.push(result);
            }
        }

        results
    }

    // Fallback implementations for when SIMD is not available
    #[cfg(not(all(feature = "simd", target_arch = "x86_64", target_feature = "avx512f")))]
    fn solve_kepler_avx512_batch(&self, mean_anomalies: &[f32], eccentricities: &[f32], max_iterations: u32) -> Vec<f32> {
        self.solve_kepler_scalar_batch(mean_anomalies, eccentricities, max_iterations)
    }

    #[cfg(not(all(feature = "simd", target_arch = "x86_64", target_feature = "avx512f")))]
    fn avx512_sin_approx_batch(&self, x: &[f32]) -> Vec<f32> {
        x.iter().map(|&val| val.sin()).collect()
    }

    #[cfg(not(all(feature = "simd", target_arch = "x86_64", target_feature = "avx512f")))]
    fn avx512_cos_approx_batch(&self, x: &[f32]) -> Vec<f32> {
        x.iter().map(|&val| val.cos()).collect()
    }

    #[cfg(not(all(feature = "simd", target_arch = "x86_64", target_feature = "avx2")))]
    fn solve_kepler_avx2_batch(&self, mean_anomalies: &[f32], eccentricities: &[f32], max_iterations: u32) -> Vec<f32> {
        self.solve_kepler_scalar_batch(mean_anomalies, eccentricities, max_iterations)
    }

    #[cfg(not(all(feature = "simd", target_arch = "x86_64", target_feature = "avx2")))]
    fn avx2_sin_approx_batch(&self, x: &[f32]) -> Vec<f32> {
        x.iter().map(|&val| val.sin()).collect()
    }

    #[cfg(not(all(feature = "simd", target_arch = "x86_64", target_feature = "avx2")))]
    fn avx2_cos_approx_batch(&self, x: &[f32]) -> Vec<f32> {
        x.iter().map(|&val| val.cos()).collect()
    }

    #[cfg(not(all(feature = "simd", target_arch = "x86_64", target_feature = "sse4.1")))]
    fn solve_kepler_sse4_batch(&self, mean_anomalies: &[f32], eccentricities: &[f32], max_iterations: u32) -> Vec<f32> {
        self.solve_kepler_scalar_batch(mean_anomalies, eccentricities, max_iterations)
    }

    #[cfg(not(all(feature = "simd", target_arch = "x86_64", target_feature = "sse4.1")))]
    fn sse4_sin_approx_batch(&self, x: &[f32]) -> Vec<f32> {
        x.iter().map(|&val| val.sin()).collect()
    }

    #[cfg(not(all(feature = "simd", target_arch = "x86_64", target_feature = "sse4.1")))]
    fn sse4_cos_approx_batch(&self, x: &[f32]) -> Vec<f32> {
        x.iter().map(|&val| val.cos()).collect()
    }
}