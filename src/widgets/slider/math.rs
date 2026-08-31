use crate::core::Color;

pub(super) fn parse_pointer_x(payload: &str) -> Option<f32> {
    payload
        .split_once(',')
        .and_then(|(x, _)| x.parse::<f32>().ok())
}

pub(super) fn value_from_pointer(
    x: f32,
    pointer_start: f32,
    pointer_width: f32,
    min: f64,
    max: f64,
    step: Option<f64>,
) -> f64 {
    let offset = (x - pointer_start).clamp(0.0, pointer_width.max(1.0));
    let progress = offset as f64 / pointer_width.max(1.0) as f64;
    quantize_value(min + (max - min) * progress, min, max, step)
}

pub(super) fn quantize_value(value: f64, min: f64, max: f64, step: Option<f64>) -> f64 {
    let value = clamp_value(value, min, max);
    let Some(step) = step else {
        return value;
    };
    clamp_value(min + ((value - min) / step).round() * step, min, max)
}

pub(super) fn normalized_progress(value: f64, min: f64, max: f64) -> f64 {
    if max <= min {
        0.0
    } else {
        ((clamp_value(value, min, max) - min) / (max - min)).clamp(0.0, 1.0)
    }
}

pub(super) fn normalize_range(start: f64, end: f64) -> (f64, f64) {
    let start = if start.is_finite() { start } else { 0.0 };
    let end = if end.is_finite() { end } else { 1.0 };
    if start <= end {
        (start, end)
    } else {
        (end, start)
    }
}

pub(super) fn clamp_value(value: f64, min: f64, max: f64) -> f64 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        min
    }
}

pub(super) fn valid_step(step: f64) -> Option<f64> {
    (step.is_finite() && step > 0.0).then_some(step)
}

pub(super) fn same_value(left: f64, right: f64) -> bool {
    (left - right).abs() <= f64::EPSILON * left.abs().max(right.abs()).max(1.0) * 4.0
}

pub(super) fn smootherstep(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    value * value * value * (value * (value * 6.0 - 15.0) + 10.0)
}

pub(super) fn mix_color(from: Color, to: Color, value: f32) -> Color {
    let value = value.clamp(0.0, 1.0);
    let from = from.0;
    let to = to.0;
    let fr = ((from >> 16) & 0xFF) as f32;
    let fg = ((from >> 8) & 0xFF) as f32;
    let fb = (from & 0xFF) as f32;
    let tr = ((to >> 16) & 0xFF) as f32;
    let tg = ((to >> 8) & 0xFF) as f32;
    let tb = (to & 0xFF) as f32;
    let r = (fr + (tr - fr) * value).round() as u32;
    let g = (fg + (tg - fg) * value).round() as u32;
    let b = (fb + (tb - fb) * value).round() as u32;
    Color((r << 16) | (g << 8) | b)
}

pub(super) fn mix_u8(from: u8, to: u8, value: f32) -> u8 {
    let value = value.clamp(0.0, 1.0);
    (from as f32 + (to as f32 - from as f32) * value).round() as u8
}
