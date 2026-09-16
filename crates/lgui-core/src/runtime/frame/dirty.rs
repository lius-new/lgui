use crate::core::UiRect;

#[derive(Clone, Copy, Debug)]
pub struct DirtyStrategy {
    pub max_rects: usize,
    pub full_area_ratio: f32,
    pub dirty_outset: f32,
}

impl Default for DirtyStrategy {
    fn default() -> Self {
        Self {
            max_rects: 12,
            full_area_ratio: 0.90,
            dirty_outset: 2.0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct DirtyRegionSet {
    viewport: UiRect,
    rects: Vec<UiRect>,
    full: bool,
    fallback_reason: Option<&'static str>,
    strategy: DirtyStrategy,
}

impl DirtyRegionSet {
    pub fn new(viewport: UiRect) -> Self {
        Self::with_strategy(viewport, DirtyStrategy::default())
    }

    pub fn with_strategy(viewport: UiRect, strategy: DirtyStrategy) -> Self {
        Self {
            viewport,
            rects: Vec::new(),
            full: false,
            fallback_reason: None,
            strategy,
        }
    }

    pub fn full(viewport: UiRect) -> Self {
        let mut set = Self::new(viewport);
        set.mark_full();
        set
    }

    pub fn mark_full(&mut self) {
        self.mark_full_with_reason("explicit");
    }

    pub fn mark_full_with_reason(&mut self, reason: &'static str) {
        self.full = true;
        self.fallback_reason = Some(reason);
        self.rects.clear();
    }

    pub fn add(&mut self, rect: UiRect) {
        if self.full {
            return;
        }
        let Some(rect) = rect
            .inflate(self.strategy.dirty_outset, self.strategy.dirty_outset)
            .intersect(self.viewport)
        else {
            return;
        };
        if rect.width() <= 0.0 || rect.height() <= 0.0 {
            return;
        }

        let mut merged = rect;
        let mut index = 0;
        while index < self.rects.len() {
            if should_merge(self.rects[index], merged) {
                merged = self.rects.remove(index).union(merged);
            } else {
                index += 1;
            }
        }
        self.rects.push(merged);
        self.normalize();
    }

    pub fn add_all(&mut self, rects: impl IntoIterator<Item = UiRect>) {
        for rect in rects {
            self.add(rect);
        }
    }

    pub fn is_full(&self) -> bool {
        self.full
    }

    pub fn is_empty(&self) -> bool {
        !self.full && self.rects.is_empty()
    }

    pub fn rects(&self) -> &[UiRect] {
        &self.rects
    }

    pub fn rect_count(&self) -> usize {
        if self.full {
            1
        } else {
            self.rects.len()
        }
    }

    pub fn dirty_area(&self) -> f64 {
        if self.full {
            area(self.viewport)
        } else {
            self.rects.iter().map(|rect| area(*rect)).sum()
        }
    }

    pub fn viewport_area(&self) -> f64 {
        area(self.viewport)
    }

    pub fn area_ratio(&self) -> f32 {
        let viewport_area = self.viewport_area();
        if viewport_area <= 0.0 {
            return 1.0;
        }
        (self.dirty_area() / viewport_area) as f32
    }

    pub fn viewport(&self) -> UiRect {
        self.viewport
    }

    pub fn effective_rects(&self) -> Vec<UiRect> {
        if self.full {
            vec![self.viewport]
        } else {
            self.rects.clone()
        }
    }

    pub fn fallback_reason(&self) -> Option<&'static str> {
        self.fallback_reason
    }

    fn normalize(&mut self) {
        if self.rects.len() > self.strategy.max_rects {
            self.mark_full_with_reason("too-many-rects");
            return;
        }
        let viewport_area = area(self.viewport);
        if viewport_area <= 0.0 {
            self.mark_full_with_reason("invalid-viewport");
            return;
        }
        let dirty_area: f64 = self.rects.iter().map(|rect| area(*rect)).sum();
        if (dirty_area / viewport_area) as f32 >= self.strategy.full_area_ratio {
            self.mark_full_with_reason("area-threshold");
        }
    }
}

fn should_merge(a: UiRect, b: UiRect) -> bool {
    a.intersect(b).is_some() || expanded(a, 2.0).intersect(b).is_some()
}

fn expanded(rect: UiRect, amount: f32) -> UiRect {
    rect.inflate(amount, amount)
}

fn area(rect: UiRect) -> f64 {
    rect.width().max(0.0) as f64 * rect.height().max(0.0) as f64
}
