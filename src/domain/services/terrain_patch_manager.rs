//! Streaming lifecycle for terrain patches (AGENTS.md sections 22-23).
//!
//! A pure-domain [`TerrainPatchManager`] tracks patches through
//! Requested → Generating → Ready → Visible → Cached → Evicted with a
//! configured memory budget and LRU eviction. Only requested patches exist:
//! the manager never generates a full planet at maximum resolution.

use crate::domain::services::cube_sphere::TerrainPatch;
use std::collections::HashMap;

/// Lifecycle state of a managed patch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchState {
    Requested,
    Generating,
    Loading,
    Ready,
    Visible,
    Cached,
    Evicted,
}

#[derive(Debug, Clone)]
struct ManagedPatch {
    state: PatchState,
    last_used_frame: u64,
    size_bytes: u64,
}

/// Streaming patch manager with a memory budget and LRU eviction.
#[derive(Debug, Default)]
pub struct TerrainPatchManager {
    patches: HashMap<TerrainPatch, ManagedPatch>,
    resident_bytes: u64,
    frame: u64,
}

impl TerrainPatchManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn resident_bytes(&self) -> u64 {
        self.resident_bytes
    }

    pub fn resident_patch_count(&self) -> usize {
        self.patches.len()
    }

    pub fn state_of(&self, patch: &TerrainPatch) -> Option<PatchState> {
        self.patches.get(patch).map(|p| p.state)
    }

    pub fn visible_patches(&self) -> impl Iterator<Item = &TerrainPatch> {
        self.patches
            .iter()
            .filter(|(_, p)| p.state == PatchState::Visible)
            .map(|(k, _)| k)
    }

    /// Record a frame tick (used for LRU timestamps).
    pub fn tick(&mut self) {
        self.frame = self.frame.wrapping_add(1);
    }

    /// Request a patch for streaming. Idempotent: an already-resident patch is
    /// left untouched.
    pub fn request(&mut self, patch: TerrainPatch, size_bytes: u64) {
        self.patches.entry(patch).or_insert(ManagedPatch {
            state: PatchState::Requested,
            last_used_frame: self.frame,
            size_bytes,
        });
    }

    /// Mark a patch as being generated (Requested → Generating).
    pub fn begin_generation(&mut self, patch: &TerrainPatch) {
        if let Some(p) = self.patches.get_mut(patch) {
            p.state = PatchState::Generating;
        }
    }

    /// Mark a patch as loaded from disk/asset (Generating → Loading).
    pub fn begin_loading(&mut self, patch: &TerrainPatch) {
        if let Some(p) = self.patches.get_mut(patch) {
            p.state = PatchState::Loading;
        }
    }

    /// Mark a patch as generated and memory-resident (Loading/Generating →
    /// Ready). Accounts its size against the budget.
    pub fn mark_ready(&mut self, patch: &TerrainPatch) {
        if let Some(p) = self.patches.get_mut(patch) {
            if matches!(
                p.state,
                PatchState::Loading | PatchState::Generating | PatchState::Requested
            ) {
                self.resident_bytes = self.resident_bytes.saturating_add(p.size_bytes);
            }
            p.state = PatchState::Ready;
            p.last_used_frame = self.frame;
        }
    }

    /// Mark a patch as visible (Ready/Cached → Visible) and update LRU order.
    pub fn mark_visible(&mut self, patch: &TerrainPatch) {
        if let Some(p) = self.patches.get_mut(patch) {
            p.state = PatchState::Visible;
            p.last_used_frame = self.frame;
        }
    }

    /// Move a visible patch to cached (Visible → Cached).
    pub fn mark_cached(&mut self, patch: &TerrainPatch) {
        if let Some(p) = self.patches.get_mut(patch) {
            p.state = PatchState::Cached;
            p.last_used_frame = self.frame;
        }
    }

    /// Evict a cached patch (Cached → Evicted), freeing its memory.
    pub fn evict(&mut self, patch: &TerrainPatch) {
        if let Some(p) = self.patches.get_mut(patch) {
            if p.state == PatchState::Cached {
                self.resident_bytes = self.resident_bytes.saturating_sub(p.size_bytes);
                p.state = PatchState::Evicted;
            }
        }
    }

    /// Remove fully-evicted patches from the table.
    pub fn sweep_evicted(&mut self) {
        self.patches.retain(|_, p| p.state != PatchState::Evicted);
    }

    /// Enforce the memory budget by evicting least-recently-used cached patches
    /// until the resident total fits. Returns the evicted patches.
    pub fn enforce_memory_budget(&mut self, budget_bytes: u64) -> Vec<TerrainPatch> {
        let mut evicted = Vec::new();
        let mut candidates: Vec<(TerrainPatch, u64)> = self
            .patches
            .iter()
            .filter(|(_, p)| p.state == PatchState::Cached)
            .map(|(k, p)| (*k, p.last_used_frame))
            .collect();
        candidates.sort_by_key(|(_, frame)| *frame); // oldest first

        for (patch, _) in candidates {
            if self.resident_bytes <= budget_bytes {
                break;
            }
            self.evict(&patch);
            evicted.push(patch);
        }
        self.sweep_evicted();
        evicted
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::cube_sphere::{CubeFace, TerrainPatch};

    fn patch(x: u32, y: u32) -> TerrainPatch {
        TerrainPatch {
            face: CubeFace::PosZ,
            level: 2,
            tile_x: x,
            tile_y: y,
        }
    }

    #[test]
    fn lifecycle_transitions_in_order() {
        let mut m = TerrainPatchManager::new();
        m.tick();
        let p = patch(1, 1);
        m.request(p, 1_000);
        assert_eq!(m.state_of(&p), Some(PatchState::Requested));

        m.begin_generation(&p);
        assert_eq!(m.state_of(&p), Some(PatchState::Generating));

        m.begin_loading(&p);
        assert_eq!(m.state_of(&p), Some(PatchState::Loading));

        m.mark_ready(&p);
        assert_eq!(m.state_of(&p), Some(PatchState::Ready));
        assert_eq!(m.resident_bytes(), 1_000);

        m.mark_visible(&p);
        assert_eq!(m.state_of(&p), Some(PatchState::Visible));

        m.mark_cached(&p);
        assert_eq!(m.state_of(&p), Some(PatchState::Cached));

        m.evict(&p);
        assert_eq!(m.state_of(&p), Some(PatchState::Evicted));
        assert_eq!(m.resident_bytes(), 0);

        m.sweep_evicted();
        assert_eq!(m.state_of(&p), None);
    }

    #[test]
    fn request_is_idempotent() {
        let mut m = TerrainPatchManager::new();
        let p = patch(0, 0);
        m.request(p, 500);
        m.mark_ready(&p);
        // Requesting again must not reset the state.
        m.request(p, 500);
        assert_eq!(m.state_of(&p), Some(PatchState::Ready));
        assert_eq!(m.resident_bytes(), 500);
    }

    #[test]
    fn memory_budget_evicts_lru_cached_patches() {
        let mut m = TerrainPatchManager::new();
        let p1 = patch(0, 0);
        let p2 = patch(0, 1);
        let p3 = patch(1, 0);

        m.tick();
        m.request(p1, 1_000);
        m.mark_ready(&p1);
        m.mark_visible(&p1);
        m.tick();
        m.mark_cached(&p1);

        m.request(p2, 1_000);
        m.mark_ready(&p2);
        m.mark_visible(&p2);
        m.tick();
        m.mark_cached(&p2);

        m.request(p3, 1_000);
        m.mark_ready(&p3);
        m.mark_visible(&p3);

        // Budget of 2 500 → one of the cached patches must go (oldest = p1).
        m.tick();
        let evicted = m.enforce_memory_budget(2_500);
        assert_eq!(evicted, vec![p1]);
        assert!(m.resident_bytes() <= 2_500);
        // Visible patches are never evicted.
        assert_eq!(m.state_of(&p3), Some(PatchState::Visible));
        assert_eq!(m.state_of(&p2), Some(PatchState::Cached));
    }

    #[test]
    fn only_requested_patches_exist() {
        let mut m = TerrainPatchManager::new();
        m.request(patch(0, 0), 10);
        // No full-planet generation: only the one requested patch exists.
        assert_eq!(m.resident_patch_count(), 1);
    }
}
