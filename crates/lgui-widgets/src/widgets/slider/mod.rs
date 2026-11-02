mod math;
mod model;
mod render;
mod state;

pub use model::{
    IntoSliderChangeHandler, Slider, SliderChangeHandler, SliderStyle, SliderValueFormatter, slider,
};

#[path = "slider_test.rs"]
#[cfg(test)]
mod tests;
