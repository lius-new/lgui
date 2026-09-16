use super::{primitive::*, *};

/// Computes the pixels that can change when a retained layer replaces one command list with
/// another. Returned rectangles use the same local coordinate space as `previous` and `next`.
pub fn compositing_layer_damage(
    previous: &[ScenePrimitive],
    next: &[ScenePrimitive],
    layer_bounds: UiRect,
) -> Vec<UiRect> {
    let mut damage = LayerDamageAccumulator::new(layer_bounds);
    collect_command_damage(previous, next, &mut damage);
    damage.finish()
}

struct LayerDamageAccumulator {
    bounds: UiRect,
    rects: Vec<UiRect>,
    full: bool,
}

impl LayerDamageAccumulator {
    fn new(bounds: UiRect) -> Self {
        Self {
            bounds,
            rects: Vec::new(),
            full: false,
        }
    }

    fn add(&mut self, rect: UiRect) {
        if self.full {
            return;
        }
        let Some(rect) = rect.inflate(2.0, 2.0).intersect(self.bounds) else {
            return;
        };
        if rect.width() <= 0.0 || rect.height() <= 0.0 {
            return;
        }
        let mut merged = rect;
        let mut index = 0;
        while index < self.rects.len() {
            if self.rects[index]
                .inflate(2.0, 2.0)
                .intersect(merged)
                .is_some()
            {
                merged = self.rects.remove(index).union(merged);
            } else {
                index += 1;
            }
        }
        self.rects.push(merged);
        let bounds_area = rect_area(self.bounds);
        let dirty_area = self.rects.iter().copied().map(rect_area).sum::<f64>();
        if self.rects.len() > 12 || bounds_area <= 0.0 || dirty_area / bounds_area >= 0.90 {
            self.full = true;
            self.rects.clear();
        }
    }

    fn finish(self) -> Vec<UiRect> {
        if self.full {
            vec![self.bounds]
        } else {
            self.rects
        }
    }
}

fn collect_command_damage(
    previous: &[ScenePrimitive],
    next: &[ScenePrimitive],
    damage: &mut LayerDamageAccumulator,
) {
    let previous_by_id = previous
        .iter()
        .enumerate()
        .map(|(index, command)| (command.id(), (index, command)))
        .collect::<HashMap<_, _>>();
    let next_by_id = next
        .iter()
        .enumerate()
        .map(|(index, command)| (command.id(), (index, command)))
        .collect::<HashMap<_, _>>();

    for (id, (previous_index, previous_command)) in &previous_by_id {
        let Some((next_index, next_command)) = next_by_id.get(id) else {
            damage.add(previous_command.paint_bounds());
            continue;
        };
        if previous_command.signature() == next_command.signature() && previous_index == next_index
        {
            continue;
        }
        if previous_index != next_index
            || !collect_nested_command_damage(previous_command, next_command, damage)
        {
            damage.add(previous_command.paint_bounds());
            damage.add(next_command.paint_bounds());
        }
    }

    for (id, (_, command)) in next_by_id {
        if !previous_by_id.contains_key(id) {
            damage.add(command.paint_bounds());
        }
    }
}

fn collect_nested_command_damage(
    previous: &ScenePrimitive,
    next: &ScenePrimitive,
    damage: &mut LayerDamageAccumulator,
) -> bool {
    match (previous, next) {
        (
            ScenePrimitive::CompositingLayer {
                rect: previous_rect,
                spec: previous_spec,
                commands: previous_commands,
                phase: previous_phase,
                ..
            },
            ScenePrimitive::CompositingLayer {
                rect: next_rect,
                spec: next_spec,
                commands: next_commands,
                phase: next_phase,
                ..
            },
        ) if previous_rect == next_rect
            && previous_spec == next_spec
            && next_spec.shadow.is_none()
            && next_spec.transform.is_identity()
            && previous_phase == next_phase =>
        {
            let local_bounds = UiRect::new(0.0, 0.0, next_rect.width(), next_rect.height());
            for rect in compositing_layer_damage(previous_commands, next_commands, local_bounds) {
                damage.add(rect.translate(next_rect.left, next_rect.top));
            }
            true
        }
        (
            ScenePrimitive::Clip {
                rect: previous_rect,
                commands: previous_commands,
                phase: previous_phase,
                ..
            },
            ScenePrimitive::Clip {
                rect: next_rect,
                commands: next_commands,
                phase: next_phase,
                ..
            },
        ) if previous_rect == next_rect && previous_phase == next_phase => {
            let mut nested = LayerDamageAccumulator::new(*next_rect);
            collect_command_damage(previous_commands, next_commands, &mut nested);
            for rect in nested.finish() {
                damage.add(rect);
            }
            true
        }
        (
            ScenePrimitive::ClipPath {
                rect: previous_rect,
                path: previous_path,
                commands: previous_commands,
                phase: previous_phase,
                ..
            },
            ScenePrimitive::ClipPath {
                rect: next_rect,
                path: next_path,
                commands: next_commands,
                phase: next_phase,
                ..
            },
        ) if previous_rect == next_rect
            && previous_path == next_path
            && previous_phase == next_phase =>
        {
            let mut nested = LayerDamageAccumulator::new(*next_rect);
            collect_command_damage(previous_commands, next_commands, &mut nested);
            for rect in nested.finish() {
                damage.add(rect);
            }
            true
        }
        _ => false,
    }
}

fn rect_area(rect: UiRect) -> f64 {
    rect.width().max(0.0) as f64 * rect.height().max(0.0) as f64
}
