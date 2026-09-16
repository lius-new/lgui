use super::{primitive::*, *};

pub(super) fn command_signature(commands: &[ScenePrimitive]) -> u64 {
    let mut hasher = DefaultHasher::new();
    for command in commands {
        command_signature_part(command, &mut hasher);
    }
    hasher.finish()
}

pub(super) fn command_signature_part(command: &ScenePrimitive, hasher: &mut DefaultHasher) {
    match command {
        ScenePrimitive::Rect {
            id,
            rect,
            style,
            phase,
        } => {
            "rect".hash(hasher);
            id.hash(hasher);
            hash_rect(rect, hasher);
            hash_visual_style(style, hasher);
            phase.hash(hasher);
        }
        ScenePrimitive::Ellipse {
            id,
            rect,
            style,
            phase,
        } => {
            "ellipse".hash(hasher);
            id.hash(hasher);
            hash_rect(rect, hasher);
            hash_visual_style(style, hasher);
            phase.hash(hasher);
        }
        ScenePrimitive::Text {
            id,
            rect,
            text,
            style,
            phase,
        } => {
            "text".hash(hasher);
            id.hash(hasher);
            hash_rect(rect, hasher);
            text.hash(hasher);
            hash_text_style(style, hasher);
            phase.hash(hasher);
        }
        ScenePrimitive::Custom {
            id,
            rect,
            key,
            style,
            phase,
        } => {
            "custom".hash(hasher);
            id.hash(hasher);
            hash_rect(rect, hasher);
            key.hash(hasher);
            hash_custom_paint_style(style, hasher);
            phase.hash(hasher);
        }
        ScenePrimitive::Line {
            id,
            start,
            end,
            stroke,
            phase,
        } => {
            "line".hash(hasher);
            id.hash(hasher);
            hash_point(start, hasher);
            hash_point(end, hasher);
            hash_stroke(stroke, hasher);
            phase.hash(hasher);
        }
        ScenePrimitive::Path {
            id,
            rect,
            path,
            style,
            phase,
        } => {
            "path".hash(hasher);
            id.hash(hasher);
            hash_rect(rect, hasher);
            hash_path(path, hasher);
            hash_path_style(style, hasher);
            phase.hash(hasher);
        }
        ScenePrimitive::Image {
            id,
            rect,
            source,
            request,
            fit,
            phase,
        } => {
            "image".hash(hasher);
            id.hash(hasher);
            hash_rect(rect, hasher);
            source.hash(hasher);
            request.hash(hasher);
            fit.hash(hasher);
            phase.hash(hasher);
        }
        ScenePrimitive::Icon {
            id,
            rect,
            key,
            style,
            phase,
        } => {
            "icon".hash(hasher);
            id.hash(hasher);
            hash_rect(rect, hasher);
            key.hash(hasher);
            hash_icon_style(style, hasher);
            phase.hash(hasher);
        }
        ScenePrimitive::Glow {
            id,
            rect,
            color,
            alpha,
            phase,
        } => {
            "glow".hash(hasher);
            id.hash(hasher);
            hash_rect(rect, hasher);
            hash_color(color, hasher);
            alpha.hash(hasher);
            phase.hash(hasher);
        }
        ScenePrimitive::BackdropBlur {
            id,
            rect,
            style,
            phase,
        } => {
            "backdrop-blur".hash(hasher);
            id.hash(hasher);
            hash_rect(rect, hasher);
            hash_backdrop_blur_style(style, hasher);
            phase.hash(hasher);
        }
        ScenePrimitive::BackdropBlurPath {
            id,
            rect,
            path,
            style,
            phase,
        } => {
            "backdrop-blur-path".hash(hasher);
            id.hash(hasher);
            hash_rect(rect, hasher);
            hash_path(path, hasher);
            hash_backdrop_blur_style(style, hasher);
            phase.hash(hasher);
        }
        ScenePrimitive::Overlay {
            id,
            rect,
            style,
            phase,
        } => {
            "overlay".hash(hasher);
            id.hash(hasher);
            hash_rect(rect, hasher);
            hash_overlay_style(style, hasher);
            phase.hash(hasher);
        }
        ScenePrimitive::CompositingLayer {
            id,
            rect,
            spec,
            content_signature,
            phase,
            ..
        } => {
            "compositing-layer".hash(hasher);
            id.hash(hasher);
            hash_rect(rect, hasher);
            spec.hash(hasher);
            content_signature.hash(hasher);
            phase.hash(hasher);
        }
        ScenePrimitive::StaticLayer {
            id,
            rect,
            spec,
            child_signature,
            phase,
            ..
        } => {
            "static-layer".hash(hasher);
            id.hash(hasher);
            hash_rect(rect, hasher);
            spec.cache_signature().hash(hasher);
            spec.opacity.hash(hasher);
            normalized_f32_bits(spec.offset_x).hash(hasher);
            normalized_f32_bits(spec.offset_y).hash(hasher);
            child_signature.hash(hasher);
            phase.hash(hasher);
        }
        ScenePrimitive::ScrollRaster {
            id,
            viewport,
            spec,
            child_signature,
            phase,
            ..
        } => {
            "scroll-raster".hash(hasher);
            id.hash(hasher);
            hash_rect(viewport, hasher);
            spec.hash(hasher);
            child_signature.hash(hasher);
            phase.hash(hasher);
        }
        ScenePrimitive::Clip {
            id,
            rect,
            child_signature,
            phase,
            ..
        } => {
            "clip".hash(hasher);
            id.hash(hasher);
            hash_rect(rect, hasher);
            child_signature.hash(hasher);
            phase.hash(hasher);
        }
        ScenePrimitive::ClipPath {
            id,
            rect,
            path,
            child_signature,
            phase,
            ..
        } => {
            "clip-path".hash(hasher);
            id.hash(hasher);
            hash_rect(rect, hasher);
            hash_path(path, hasher);
            child_signature.hash(hasher);
            phase.hash(hasher);
        }
    }
}

pub(super) fn translate_commands(
    commands: Vec<ScenePrimitive>,
    dx: f32,
    dy: f32,
) -> Vec<ScenePrimitive> {
    translate_commands_with_policy(commands, dx, dy, NestedStaticLayerPolicy::Translate)
}

#[derive(Clone, Copy)]
enum NestedStaticLayerPolicy {
    Translate,
    #[cfg(all(target_os = "windows", feature = "renderer-d2d"))]
    PreserveLocalCommands,
}

#[cfg(all(target_os = "windows", feature = "renderer-d2d"))]
#[doc(hidden)]
pub fn translate_scene_primitive_for_backend(
    command: &ScenePrimitive,
    dx: f32,
    dy: f32,
) -> ScenePrimitive {
    translate_command_with_policy(
        command,
        dx,
        dy,
        NestedStaticLayerPolicy::PreserveLocalCommands,
    )
}

fn translate_commands_with_policy(
    commands: Vec<ScenePrimitive>,
    dx: f32,
    dy: f32,
    nested_static_layer: NestedStaticLayerPolicy,
) -> Vec<ScenePrimitive> {
    commands
        .into_iter()
        .map(|command| translate_command_with_policy(&command, dx, dy, nested_static_layer))
        .collect()
}

pub(super) fn translate_command(command: &ScenePrimitive, dx: f32, dy: f32) -> ScenePrimitive {
    translate_command_with_policy(command, dx, dy, NestedStaticLayerPolicy::Translate)
}

fn translate_command_with_policy(
    command: &ScenePrimitive,
    dx: f32,
    dy: f32,
    nested_static_layer: NestedStaticLayerPolicy,
) -> ScenePrimitive {
    let translate_rect = |rect: UiRect| rect.translate(dx, dy);
    let translate_point = |point: super::Point| super::Point::new(point.x + dx, point.y + dy);
    match command {
        ScenePrimitive::Rect {
            id,
            rect,
            style,
            phase,
        } => ScenePrimitive::Rect {
            id: id.clone(),
            rect: translate_rect(*rect),
            style: *style,
            phase: *phase,
        },
        ScenePrimitive::Ellipse {
            id,
            rect,
            style,
            phase,
        } => ScenePrimitive::Ellipse {
            id: id.clone(),
            rect: translate_rect(*rect),
            style: *style,
            phase: *phase,
        },
        ScenePrimitive::Text {
            id,
            rect,
            text,
            style,
            phase,
        } => ScenePrimitive::Text {
            id: id.clone(),
            rect: translate_rect(*rect),
            text: text.clone(),
            style: *style,
            phase: *phase,
        },
        ScenePrimitive::Custom {
            id,
            rect,
            key,
            style,
            phase,
        } => ScenePrimitive::Custom {
            id: id.clone(),
            rect: translate_rect(*rect),
            key,
            style: *style,
            phase: *phase,
        },
        ScenePrimitive::Line {
            id,
            start,
            end,
            stroke,
            phase,
        } => ScenePrimitive::Line {
            id: id.clone(),
            start: translate_point(*start),
            end: translate_point(*end),
            stroke: *stroke,
            phase: *phase,
        },
        ScenePrimitive::Path {
            id,
            rect,
            path,
            style,
            phase,
        } => ScenePrimitive::Path {
            id: id.clone(),
            rect: translate_rect(*rect),
            path: translate_path(path, dx, dy),
            style: *style,
            phase: *phase,
        },
        ScenePrimitive::Image {
            id,
            rect,
            source,
            request,
            fit,
            phase,
        } => ScenePrimitive::Image {
            id: id.clone(),
            rect: translate_rect(*rect),
            source: source.clone(),
            request: request.clone(),
            fit: *fit,
            phase: *phase,
        },
        ScenePrimitive::Icon {
            id,
            rect,
            key,
            style,
            phase,
        } => ScenePrimitive::Icon {
            id: id.clone(),
            rect: translate_rect(*rect),
            key,
            style: *style,
            phase: *phase,
        },
        ScenePrimitive::Glow {
            id,
            rect,
            color,
            alpha,
            phase,
        } => ScenePrimitive::Glow {
            id: id.clone(),
            rect: translate_rect(*rect),
            color: *color,
            alpha: *alpha,
            phase: *phase,
        },
        ScenePrimitive::BackdropBlur {
            id,
            rect,
            style,
            phase,
        } => ScenePrimitive::BackdropBlur {
            id: id.clone(),
            rect: translate_rect(*rect),
            style: *style,
            phase: *phase,
        },
        ScenePrimitive::BackdropBlurPath {
            id,
            rect,
            path,
            style,
            phase,
        } => ScenePrimitive::BackdropBlurPath {
            id: id.clone(),
            rect: translate_rect(*rect),
            path: translate_path(path, dx, dy),
            style: *style,
            phase: *phase,
        },
        ScenePrimitive::Overlay {
            id,
            rect,
            style,
            phase,
        } => ScenePrimitive::Overlay {
            id: id.clone(),
            rect: translate_rect(*rect),
            style: style.clone(),
            phase: *phase,
        },
        ScenePrimitive::CompositingLayer {
            id,
            rect,
            spec,
            commands,
            content_signature,
            phase,
        } => ScenePrimitive::CompositingLayer {
            id: id.clone(),
            rect: translate_rect(*rect),
            spec: *spec,
            commands: commands.clone(),
            content_signature: *content_signature,
            phase: *phase,
        },
        ScenePrimitive::StaticLayer {
            id,
            rect,
            spec,
            commands,
            child_signature,
            phase,
        } => ScenePrimitive::StaticLayer {
            id: id.clone(),
            rect: translate_rect(*rect),
            spec: spec.clone(),
            commands: match nested_static_layer {
                NestedStaticLayerPolicy::Translate => {
                    translate_commands_with_policy(commands.clone(), dx, dy, nested_static_layer)
                }
                #[cfg(all(target_os = "windows", feature = "renderer-d2d"))]
                NestedStaticLayerPolicy::PreserveLocalCommands => commands.clone(),
            },
            child_signature: *child_signature,
            phase: *phase,
        },
        ScenePrimitive::ScrollRaster {
            id,
            viewport,
            spec,
            commands,
            child_signature,
            phase,
        } => ScenePrimitive::ScrollRaster {
            id: id.clone(),
            viewport: translate_rect(*viewport),
            spec: spec.clone(),
            commands: translate_commands_with_policy(commands.clone(), dx, dy, nested_static_layer),
            child_signature: *child_signature,
            phase: *phase,
        },
        ScenePrimitive::Clip {
            id,
            rect,
            commands,
            child_signature,
            phase,
        } => ScenePrimitive::Clip {
            id: id.clone(),
            rect: translate_rect(*rect),
            commands: translate_commands_with_policy(commands.clone(), dx, dy, nested_static_layer),
            child_signature: *child_signature,
            phase: *phase,
        },
        ScenePrimitive::ClipPath {
            id,
            rect,
            path,
            commands,
            child_signature,
            phase,
        } => ScenePrimitive::ClipPath {
            id: id.clone(),
            rect: translate_rect(*rect),
            path: translate_path(path, dx, dy),
            commands: translate_commands_with_policy(commands.clone(), dx, dy, nested_static_layer),
            child_signature: *child_signature,
            phase: *phase,
        },
    }
}

fn translate_path(path: &UiPath, dx: f32, dy: f32) -> UiPath {
    let translate = |point: super::Point| super::Point::new(point.x + dx, point.y + dy);
    UiPath::new(path.commands().iter().map(|command| match *command {
        UiPathCommand::MoveTo(point) => UiPathCommand::MoveTo(translate(point)),
        UiPathCommand::LineTo(point) => UiPathCommand::LineTo(translate(point)),
        UiPathCommand::QuadraticTo { control, to } => UiPathCommand::QuadraticTo {
            control: translate(control),
            to: translate(to),
        },
        UiPathCommand::CubicTo {
            control1,
            control2,
            to,
        } => UiPathCommand::CubicTo {
            control1: translate(control1),
            control2: translate(control2),
            to: translate(to),
        },
        UiPathCommand::Close => UiPathCommand::Close,
    }))
}

fn hash_custom_paint_style(style: &Option<CustomPaintStyle>, hasher: &mut DefaultHasher) {
    style.is_some().hash(hasher);
    if let Some(style) = style {
        hash_color(&style.color, hasher);
        style.intensity.to_bits().hash(hasher);
    }
}

fn hash_backdrop_blur_style(style: &BackdropBlurStyle, hasher: &mut DefaultHasher) {
    style.source.hash(hasher);
    style.fit.hash(hasher);
    hash_rect(&style.source_rect, hasher);
    normalized_f32_bits(style.radius).hash(hasher);
    style.opacity.to_bits().hash(hasher);
    hash_color(&style.tint, hasher);
    style.tint_alpha.to_bits().hash(hasher);
}

fn hash_rect(rect: &UiRect, hasher: &mut DefaultHasher) {
    normalized_f32_bits(rect.left).hash(hasher);
    normalized_f32_bits(rect.top).hash(hasher);
    normalized_f32_bits(rect.right).hash(hasher);
    normalized_f32_bits(rect.bottom).hash(hasher);
}

fn hash_point(point: &super::Point, hasher: &mut DefaultHasher) {
    normalized_f32_bits(point.x).hash(hasher);
    normalized_f32_bits(point.y).hash(hasher);
}

fn hash_color(color: &super::Color, hasher: &mut DefaultHasher) {
    color.0.hash(hasher);
}

fn hash_stroke(stroke: &super::Stroke, hasher: &mut DefaultHasher) {
    hash_color(&stroke.color, hasher);
    normalized_f32_bits(stroke.width).hash(hasher);
    stroke.alpha.hash(hasher);
}

fn hash_path(path: &UiPath, hasher: &mut DefaultHasher) {
    path.commands.len().hash(hasher);
    for command in &path.commands {
        match command {
            UiPathCommand::MoveTo(point) => {
                "move".hash(hasher);
                hash_point(point, hasher);
            }
            UiPathCommand::LineTo(point) => {
                "line".hash(hasher);
                hash_point(point, hasher);
            }
            UiPathCommand::QuadraticTo { control, to } => {
                "quadratic".hash(hasher);
                hash_point(control, hasher);
                hash_point(to, hasher);
            }
            UiPathCommand::CubicTo {
                control1,
                control2,
                to,
            } => {
                "cubic".hash(hasher);
                hash_point(control1, hasher);
                hash_point(control2, hasher);
                hash_point(to, hasher);
            }
            UiPathCommand::Close => {
                "close".hash(hasher);
            }
        }
    }
}

fn hash_path_style(style: &PathStyle, hasher: &mut DefaultHasher) {
    style.fill.map(|color| color.0).hash(hasher);
    style.fill_alpha.hash(hasher);
    style.stroke.is_some().hash(hasher);
    if let Some(stroke) = &style.stroke {
        hash_stroke(stroke, hasher);
    }
}

fn hash_visual_style(style: &VisualStyle, hasher: &mut DefaultHasher) {
    style.fill.map(|color| color.0).hash(hasher);
    style.fill_alpha.hash(hasher);
    style.stroke.is_some().hash(hasher);
    if let Some(stroke) = &style.stroke {
        hash_stroke(stroke, hasher);
    }
    normalized_f32_bits(style.radius).hash(hasher);
}

fn hash_text_style(style: &TextStyle, hasher: &mut DefaultHasher) {
    hash_color(&style.color, hasher);
    normalized_f32_bits(style.height).hash(hasher);
    style.weight.hash(hasher);
    normalized_f32_bits(style.tracking).hash(hasher);
    style.align.hash(hasher);
    style.alpha.hash(hasher);
}

fn hash_icon_style(style: &super::IconStyle, hasher: &mut DefaultHasher) {
    hash_color(&style.color, hasher);
    style.alpha.hash(hasher);
}

fn hash_overlay_style(style: &OverlayStyle, hasher: &mut DefaultHasher) {
    style.vertical_layers.len().hash(hasher);
    for layer in &style.vertical_layers {
        hash_color(&layer.color, hasher);
        layer.alpha_top.to_bits().hash(hasher);
        layer.alpha_bottom.to_bits().hash(hasher);
    }
    style.radial_layers.len().hash(hasher);
    for layer in &style.radial_layers {
        hash_color(&layer.color, hasher);
        layer.alpha.to_bits().hash(hasher);
        layer.center_x.to_bits().hash(hasher);
        layer.center_y.to_bits().hash(hasher);
        layer.radius.to_bits().hash(hasher);
    }
}
