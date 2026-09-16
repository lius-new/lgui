use super::{primitive::*, *};

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
    PreserveLocalCommands,
}

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
