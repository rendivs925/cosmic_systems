use crate::infrastructure::bevy_adapters::components::QualityLevel;
use bumpalo::Bump;

/// Physics memory pool for zero-allocation physics updates
pub struct PhysicsMemoryPool {
    arena: Bump,
    max_allocations: usize,
}

impl PhysicsMemoryPool {
    pub fn new(quality: QualityLevel) -> Self {
        let max_allocations = match quality {
            QualityLevel::Ultra => 1024 * 1024,    // 1MB for high quality
            QualityLevel::High => 512 * 1024,      // 512KB
            QualityLevel::Medium => 256 * 1024,    // 256KB
            QualityLevel::Low => 128 * 1024,       // 128KB
            QualityLevel::Minimal => 64 * 1024,    // 64KB
        };

        Self {
            arena: Bump::with_capacity(max_allocations),
            max_allocations,
        }
    }

    pub fn reset(&mut self) {
        if self.arena.allocated_bytes() > self.max_allocations / 2 {
            self.arena.reset();
        }
    }

    /// Allocate a slice of default values
    pub fn alloc_slice_fill_default<T: Default>(&self, len: usize) -> &mut [T] {
        self.arena.alloc_slice_fill_default(len)
    }

    /// Allocate a single value with default
    pub fn alloc_default<T: Default>(&self) -> &mut T {
        self.arena.alloc(Default::default())
    }

    /// Get current allocation stats
    pub fn allocated_bytes(&self) -> usize {
        self.arena.allocated_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_pool_allocation() {
        let pool = PhysicsMemoryPool::new(QualityLevel::High);

        // Test slice allocation
        let slice = pool.alloc_slice_fill_default::<f32>(100);
        assert_eq!(slice.len(), 100);

        // Test single allocation
        let value = pool.alloc_default::<Vec3>();
        assert_eq!(*value, Vec3::ZERO);
    }

    #[test]
    fn test_memory_pool_reset() {
        let mut pool = PhysicsMemoryPool::new(QualityLevel::Medium);
        let _slice = pool.alloc_slice_fill_default::<f32>(50);

        let initial_alloc = pool.allocated_bytes();
        assert!(initial_alloc > 0);

        pool.reset();
        // After reset, should be able to allocate again
        let _new_slice = pool.alloc_slice_fill_default::<f32>(25);
    }
}