use super::primitive::{RenderPhase, ScenePrimitive};

pub fn commands_for_phase(
    commands: &[ScenePrimitive],
    phase: RenderPhase,
) -> impl Iterator<Item = &ScenePrimitive> {
    commands.iter().filter(move |command| match command {
        ScenePrimitive::Rect {
            phase: command_phase,
            ..
        }
        | ScenePrimitive::Ellipse {
            phase: command_phase,
            ..
        }
        | ScenePrimitive::Text {
            phase: command_phase,
            ..
        }
        | ScenePrimitive::Custom {
            phase: command_phase,
            ..
        }
        | ScenePrimitive::Line {
            phase: command_phase,
            ..
        }
        | ScenePrimitive::Path {
            phase: command_phase,
            ..
        }
        | ScenePrimitive::Image {
            phase: command_phase,
            ..
        }
        | ScenePrimitive::Icon {
            phase: command_phase,
            ..
        }
        | ScenePrimitive::Glow {
            phase: command_phase,
            ..
        }
        | ScenePrimitive::BackdropBlur {
            phase: command_phase,
            ..
        }
        | ScenePrimitive::BackdropBlurPath {
            phase: command_phase,
            ..
        }
        | ScenePrimitive::ContentBlur {
            phase: command_phase,
            ..
        }
        | ScenePrimitive::Overlay {
            phase: command_phase,
            ..
        }
        | ScenePrimitive::CompositingLayer {
            phase: command_phase,
            ..
        }
        | ScenePrimitive::StaticLayer {
            phase: command_phase,
            ..
        }
        | ScenePrimitive::ScrollRaster {
            phase: command_phase,
            ..
        }
        | ScenePrimitive::Clip {
            phase: command_phase,
            ..
        }
        | ScenePrimitive::ClipPath {
            phase: command_phase,
            ..
        } => *command_phase == phase,
    })
}
