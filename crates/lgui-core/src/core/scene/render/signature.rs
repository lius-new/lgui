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
