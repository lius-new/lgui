use std::time::Duration;

pub fn duration_ms(duration: Duration) -> f32 {
    duration.as_secs_f64() as f32 * 1000.0
}
