use super::{primitive::*, *};

pub(crate) fn estimate_scene_commands_bytes(commands: &[ScenePrimitive]) -> usize {
    std::mem::size_of_val(commands).saturating_add(
        commands
            .iter()
            .map(estimate_scene_primitive_dynamic_bytes)
            .sum::<usize>(),
    )
}

fn estimate_scene_primitive_dynamic_bytes(command: &ScenePrimitive) -> usize {
    match command {
        ScenePrimitive::Text { text, .. } => text.len(),
        ScenePrimitive::Path { path, .. } | ScenePrimitive::BackdropBlurPath { path, .. } => path
            .commands()
            .len()
            .saturating_mul(std::mem::size_of::<UiPathCommand>()),
        ScenePrimitive::Image { source, .. } => match source {
            UiImageSource::Bytes { bytes, key, .. } => bytes.len().saturating_add(key.len()),
            UiImageSource::Url(value) => value.len(),
            UiImageSource::File(value) => value.as_os_str().len(),
            UiImageSource::Static(value) => value.len(),
        },
        ScenePrimitive::CompositingLayer { commands, .. }
        | ScenePrimitive::StaticLayer { commands, .. }
        | ScenePrimitive::ScrollRaster { commands, .. }
        | ScenePrimitive::Clip { commands, .. }
        | ScenePrimitive::ClipPath { commands, .. } => estimate_scene_commands_bytes(commands),
        _ => 0,
    }
}
