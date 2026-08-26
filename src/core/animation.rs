use std::collections::{HashMap, HashSet};

use super::UiId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AnimProperty {
    Hover,
    Active,
    Pressed,
    Focus,
    Opacity,
    TranslateX,
    TranslateY,
    Custom(&'static str),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnimationBinding {
    pub property: AnimProperty,
    pub idle: f32,
    pub active: f32,
    pub timing: AnimationTiming,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AnimationTiming {
    Exponential { speed: f32 },
    Duration { fade_in_ms: f32, fade_out_ms: f32 },
}

impl AnimationBinding {
    pub const fn new(property: AnimProperty, idle: f32, active: f32) -> Self {
        Self {
            property,
            idle,
            active,
            timing: AnimationTiming::Exponential { speed: 0.018 },
        }
    }

    pub const fn speed(mut self, speed: f32) -> Self {
        self.timing = AnimationTiming::Exponential { speed };
        self
    }

    pub const fn duration(mut self, fade_in_ms: f32, fade_out_ms: f32) -> Self {
        self.timing = AnimationTiming::Duration {
            fade_in_ms,
            fade_out_ms,
        };
        self
    }
}

#[derive(Clone, Copy, Debug)]
pub struct AnimatedValue {
    pub value: f32,
    pub target: f32,
    pub timing: AnimationTiming,
}

impl AnimatedValue {
    pub fn new(value: f32) -> Self {
        Self {
            value,
            target: value,
            timing: AnimationTiming::Exponential { speed: 0.018 },
        }
    }

    pub fn with_timing(value: f32, timing: AnimationTiming) -> Self {
        Self {
            value,
            target: value,
            timing,
        }
    }

    pub fn set_target(&mut self, target: f32) -> bool {
        if (self.target - target).abs() <= f32::EPSILON {
            return false;
        }
        self.target = target;
        true
    }

    pub fn eased(self) -> f32 {
        smootherstep(self.value)
    }

    pub fn is_running(self) -> bool {
        (self.value - self.target).abs() >= 0.001
    }

    pub fn advance(&mut self, elapsed_ms: f32) -> bool {
        let previous = self.value;
        match self.timing {
            AnimationTiming::Exponential { speed } => {
                let t = (elapsed_ms * speed).clamp(0.0, 1.0);
                self.value += (self.target - self.value) * t;
            }
            AnimationTiming::Duration {
                fade_in_ms,
                fade_out_ms,
            } => {
                let duration = if self.target > self.value {
                    fade_in_ms
                } else {
                    fade_out_ms
                };
                let step = (elapsed_ms / duration.max(1.0)).clamp(0.0, 1.0);
                self.value = move_towards(self.value, self.target, step);
            }
        }
        if (self.value - self.target).abs() < 0.001 {
            self.value = self.target;
        }
        (self.value - previous).abs() > f32::EPSILON
    }
}

fn smootherstep(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    value * value * value * (value * (value * 6.0 - 15.0) + 10.0)
}

#[derive(Default)]
pub struct AnimationRegistry {
    values: HashMap<(UiId, AnimProperty), AnimatedValue>,
    dirty_ids: HashSet<UiId>,
}

impl AnimationRegistry {
    pub fn value(&self, id: UiId, property: AnimProperty) -> f32 {
        self.values
            .get(&(id, property))
            .map(|value| value.value)
            .unwrap_or(0.0)
    }

    pub fn set_binding_target(
        &mut self,
        id: UiId,
        binding: AnimationBinding,
        active: bool,
    ) -> bool {
        let target = if active { binding.active } else { binding.idle };
        let value = self
            .values
            .entry((id.clone(), binding.property))
            .or_insert_with(|| AnimatedValue::with_timing(binding.idle, binding.timing));
        value.timing = binding.timing;
        if value.set_target(target) {
            self.dirty_ids.insert(id);
            return true;
        }
        false
    }

    pub fn sync_binding_target(
        &mut self,
        id: UiId,
        binding: AnimationBinding,
        active: bool,
    ) -> bool {
        let target = if active { binding.active } else { binding.idle };
        let key = (id.clone(), binding.property);
        let initialized = !self.values.contains_key(&key);
        let value = self
            .values
            .entry(key)
            .or_insert_with(|| AnimatedValue::with_timing(target, binding.timing));
        value.timing = binding.timing;
        if initialized || value.set_target(target) {
            self.dirty_ids.insert(id);
            return true;
        }
        false
    }

    pub fn set_target(&mut self, id: UiId, property: AnimProperty, target: f32) -> bool {
        let value = self
            .values
            .entry((id.clone(), property))
            .or_insert_with(|| AnimatedValue::new(target));
        if value.set_target(target) {
            self.dirty_ids.insert(id);
            return true;
        }
        false
    }

    pub fn clear_targets(&mut self, properties: &[AnimProperty]) -> bool {
        let mut changed = false;
        for ((id, property), value) in &mut self.values {
            if !properties.contains(property) {
                continue;
            }
            if value.value.abs() > f32::EPSILON || value.target.abs() > f32::EPSILON {
                value.value = 0.0;
                value.target = 0.0;
                self.dirty_ids.insert(id.clone());
                changed = true;
            }
        }
        changed
    }

    pub fn clear_absent_values(
        &mut self,
        present_ids: &HashSet<UiId>,
        properties: &[AnimProperty],
    ) -> bool {
        let mut removed_ids = Vec::new();
        self.values.retain(|(id, property), _| {
            let remove = properties.contains(property) && !present_ids.contains(id);
            if remove {
                removed_ids.push(id.clone());
            }
            !remove
        });
        if removed_ids.is_empty() {
            return false;
        }
        self.dirty_ids.extend(removed_ids);
        true
    }

    pub fn advance(&mut self, elapsed_ms: f32) -> bool {
        let mut changed = false;
        for ((id, _), value) in &mut self.values {
            if value.advance(elapsed_ms) {
                self.dirty_ids.insert(id.clone());
                changed = true;
            }
        }
        changed
    }

    pub fn take_dirty_ids(&mut self) -> Vec<UiId> {
        self.dirty_ids.drain().collect()
    }

    pub fn snapshot(&self, id: UiId) -> AnimationSnapshot<'_> {
        AnimationSnapshot { registry: self, id }
    }
}

fn move_towards(current: f32, target: f32, step: f32) -> f32 {
    if current < target {
        (current + step).min(target)
    } else {
        (current - step).max(target)
    }
}

#[derive(Clone)]
pub struct AnimationSnapshot<'a> {
    registry: &'a AnimationRegistry,
    id: UiId,
}

impl AnimationSnapshot<'_> {
    pub fn get(self, property: AnimProperty) -> f32 {
        self.registry.value(self.id.clone(), property)
    }
}
