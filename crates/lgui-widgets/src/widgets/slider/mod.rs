mod math;
mod model;
mod render;
mod state;

pub use model::{
    slider, IntoSliderChangeHandler, Slider, SliderChangeHandler, SliderStyle, SliderValueFormatter,
};

#[path = "slider_test.rs"]
#[cfg(test)]
mod tests;
