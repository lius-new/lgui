use super::*;

impl ComponentTree {
    pub(crate) fn output_memory_usage(&self) -> crate::memory::CacheUsage {
        let slots = self.slots.borrow();
        let mut bytes = 0usize;
        let mut entries = 0usize;
        let mut largest = 0usize;
        for output in slots
            .iter()
            .filter_map(|slot| slot.node.as_ref())
            .filter_map(|node| node.output.as_ref())
        {
            let output_bytes = output.estimated_bytes();
            bytes = bytes.saturating_add(output_bytes);
            entries = entries.saturating_add(1);
            largest = largest.max(output_bytes);
        }
        crate::memory::CacheUsage {
            rebuildable_bytes: bytes,
            cpu_bytes: bytes,
            entries,
            largest_entry_bytes: largest,
            ..Default::default()
        }
    }

    pub(crate) fn trim_outputs(&self, target_bytes: usize) -> usize {
        let before = self.output_memory_usage().rebuildable_bytes;
        if before <= target_bytes {
            return 0;
        }
        let mut remaining = before;
        let mut trimmed = Vec::new();
        {
            let mut slots = self.slots.borrow_mut();
            for slot in slots.iter_mut().rev() {
                if remaining <= target_bytes {
                    break;
                }
                let Some(node) = slot.node.as_mut() else {
                    continue;
                };
                let Some(output) = node.output.take() else {
                    continue;
                };
                remaining = remaining.saturating_sub(output.estimated_bytes());
                trimmed.push(node.id);
            }
        }
        for id in trimmed {
            self.mark_dirty(id);
        }
        before.saturating_sub(remaining)
    }
}
