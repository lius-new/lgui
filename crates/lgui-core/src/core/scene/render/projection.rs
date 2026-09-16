use super::{primitive::*, *};
use super::{signature::command_signature, transform::translate_command};

pub(super) fn project_command(command: &ScenePrimitive, scale: UiScale) -> ScenePrimitive {
    let signature_scale = scale.factor().to_bits() as u64;
    match command {
        ScenePrimitive::Rect {
            id,
            rect,
            style,
            phase,
        } => ScenePrimitive::Rect {
            id: id.clone(),
            rect: scale.physical_ui_rect(*rect),
            style: project_visual_style(*style, scale),
            phase: *phase,
        },
        ScenePrimitive::Ellipse {
            id,
            rect,
            style,
            phase,
        } => ScenePrimitive::Ellipse {
            id: id.clone(),
            rect: scale.physical_ui_rect(*rect),
            style: project_visual_style(*style, scale),
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
            rect: scale.physical_ui_rect(*rect),
            text: text.clone(),
            style: project_text_style(*style, scale),
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
            rect: scale.physical_ui_rect(*rect),
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
            start: scale.physical_ui_point(*start),
            end: scale.physical_ui_point(*end),
            stroke: project_stroke(*stroke, scale),
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
            rect: scale.physical_ui_rect(*rect),
            path: project_path(path, scale),
            style: project_path_style(*style, scale),
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
            rect: scale.physical_ui_rect(*rect),
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
            rect: scale.physical_ui_rect(*rect),
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
            rect: scale.physical_ui_rect(*rect),
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
            rect: scale.physical_ui_rect(*rect),
            style: project_backdrop_blur_style(*style, scale),
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
            rect: scale.physical_ui_rect(*rect),
            path: project_path(path, scale),
            style: project_backdrop_blur_style(*style, scale),
            phase: *phase,
        },
        ScenePrimitive::Overlay {
            id,
            rect,
            style,
            phase,
        } => ScenePrimitive::Overlay {
            id: id.clone(),
            rect: scale.physical_ui_rect(*rect),
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
        } => {
            let mut spec = *spec;
            spec.transform = spec.transform.project_to_physical(scale);
            spec.shadow = spec.shadow.map(|shadow| shadow.project_to_physical(scale));
            let projected_rect = scale.physical_ui_rect(*rect);
            let rect = if spec.shadow.is_some() {
                UiRect::new(
                    projected_rect.left.floor(),
                    projected_rect.top.floor(),
                    projected_rect.right.ceil(),
                    projected_rect.bottom.ceil(),
                )
            } else {
                projected_rect
            };
            let commands = commands
                .iter()
                .map(|command| {
                    let projected = project_command(command, scale);
                    if rect.left == projected_rect.left && rect.top == projected_rect.top {
                        projected
                    } else {
                        translate_command(
                            &projected,
                            projected_rect.left - rect.left,
                            projected_rect.top - rect.top,
                        )
                    }
                })
                .collect::<Vec<_>>();
            let content_signature = if spec.shadow.is_some() {
                command_signature(&commands)
            } else {
                content_signature ^ signature_scale
            };
            ScenePrimitive::CompositingLayer {
                id: id.clone(),
                rect,
                spec,
                commands,
                content_signature,
                phase: *phase,
            }
        }
        ScenePrimitive::StaticLayer {
            id,
            rect,
            spec,
            commands,
            child_signature,
            phase,
        } => {
            let mut spec = spec.clone();
            spec.offset_x = scale.physical_ui_value(spec.offset_x);
            spec.offset_y = scale.physical_ui_value(spec.offset_y);
            ScenePrimitive::StaticLayer {
                id: id.clone(),
                rect: scale.physical_ui_rect(*rect),
                spec,
                commands: commands
                    .iter()
                    .map(|command| project_command(command, scale))
                    .collect(),
                child_signature: child_signature ^ signature_scale,
                phase: *phase,
            }
        }
        ScenePrimitive::ScrollRaster {
            id,
            viewport,
            spec,
            commands,
            child_signature,
            phase,
        } => {
            let mut spec = spec.clone();
            spec.cache_epoch ^= signature_scale;
            spec.content_height = scale.physical_ui_length(spec.content_height);
            spec.scroll_y = scale.physical_ui_value(spec.scroll_y);
            spec.tile_height_px = scale.physical_ui_length(spec.tile_height_px);
            ScenePrimitive::ScrollRaster {
                id: id.clone(),
                viewport: scale.physical_ui_rect(*viewport),
                spec,
                commands: commands
                    .iter()
                    .map(|command| project_command(command, scale))
                    .collect(),
                child_signature: child_signature ^ signature_scale,
                phase: *phase,
            }
        }
        ScenePrimitive::Clip {
            id,
            rect,
            commands,
            child_signature,
            phase,
        } => ScenePrimitive::Clip {
            id: id.clone(),
            rect: scale.physical_ui_rect(*rect),
            commands: commands
                .iter()
                .map(|command| project_command(command, scale))
                .collect(),
            child_signature: child_signature ^ signature_scale,
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
            rect: scale.physical_ui_rect(*rect),
            path: project_path(path, scale),
            commands: commands
                .iter()
                .map(|command| project_command(command, scale))
                .collect(),
            child_signature: child_signature ^ signature_scale,
            phase: *phase,
        },
    }
}

fn project_stroke(mut stroke: super::Stroke, scale: UiScale) -> super::Stroke {
    stroke.width = scale.physical_ui_length(stroke.width);
    stroke
}

fn project_visual_style(mut style: VisualStyle, scale: UiScale) -> VisualStyle {
    style.radius = scale.physical_ui_length(style.radius);
    style.stroke = style.stroke.map(|stroke| project_stroke(stroke, scale));
    style
}

fn project_path_style(mut style: PathStyle, scale: UiScale) -> PathStyle {
    style.stroke = style.stroke.map(|stroke| project_stroke(stroke, scale));
    style
}

fn project_text_style(mut style: TextStyle, scale: UiScale) -> TextStyle {
    style.height = scale.physical_ui_signed_length(style.height);
    style.tracking = scale.physical_ui_value(style.tracking);
    style
}

fn project_backdrop_blur_style(mut style: BackdropBlurStyle, scale: UiScale) -> BackdropBlurStyle {
    style.source_rect = scale.physical_ui_rect(style.source_rect);
    style.radius = scale.physical_ui_length(style.radius);
    style
}

fn project_path(path: &UiPath, scale: UiScale) -> UiPath {
    UiPath::new(path.commands().iter().map(|command| match *command {
        UiPathCommand::MoveTo(point) => UiPathCommand::MoveTo(scale.physical_ui_point(point)),
        UiPathCommand::LineTo(point) => UiPathCommand::LineTo(scale.physical_ui_point(point)),
        UiPathCommand::QuadraticTo { control, to } => UiPathCommand::QuadraticTo {
            control: scale.physical_ui_point(control),
            to: scale.physical_ui_point(to),
        },
        UiPathCommand::CubicTo {
            control1,
            control2,
            to,
        } => UiPathCommand::CubicTo {
            control1: scale.physical_ui_point(control1),
            control2: scale.physical_ui_point(control2),
            to: scale.physical_ui_point(to),
        },
        UiPathCommand::Close => UiPathCommand::Close,
    }))
}
