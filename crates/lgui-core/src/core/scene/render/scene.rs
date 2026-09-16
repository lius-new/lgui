use super::{memory::estimate_scene_commands_bytes, primitive::*, projection::project_command, *};

#[derive(Clone, Debug, Default)]
pub struct Scene {
    pub(super) commands: Arc<Vec<ScenePrimitive>>,
}

impl Scene {
    pub fn new() -> Self {
        Self {
            commands: Arc::new(Vec::new()),
        }
    }

    pub fn push(&mut self, command: ScenePrimitive) {
        Arc::make_mut(&mut self.commands).push(command);
    }

    pub fn commands(&self) -> &[ScenePrimitive] {
        self.commands.as_slice()
    }

    #[doc(hidden)]
    pub fn shares_command_storage_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.commands, &other.commands)
    }

    pub fn image_requests(&self) -> Vec<ImageRequest> {
        let mut requests = Vec::new();
        collect_image_requests(self.commands(), &mut requests);
        requests
    }

    pub fn estimated_bytes(&self) -> usize {
        estimate_scene_commands_bytes(self.commands())
    }

    pub(super) fn move_popup_commands_to_end(&mut self) {
        let commands = Arc::make_mut(&mut self.commands);
        let (popup, regular): (Vec<_>, Vec<_>) = std::mem::take(commands)
            .into_iter()
            .partition(|command| command.phase() == RenderPhase::Popup);
        *commands = regular;
        commands.extend(popup);
    }

    pub(crate) fn replace_range(
        &mut self,
        range: std::ops::Range<usize>,
        commands: impl IntoIterator<Item = ScenePrimitive>,
    ) {
        Arc::make_mut(&mut self.commands).splice(range, commands);
    }

    pub(crate) fn replace_all(&mut self, commands: Vec<ScenePrimitive>) {
        self.commands = Arc::new(commands);
    }

    pub(crate) fn patch_compositing_layer_spec(
        &mut self,
        id: &UiId,
        spec: CompositingLayerSpec,
    ) -> bool {
        let commands = Arc::make_mut(&mut self.commands);
        patch_compositing_layer_spec(commands.as_mut_slice(), id, spec)
    }

    pub fn bounds(&self) -> Option<UiRect> {
        self.commands()
            .iter()
            .map(ScenePrimitive::rect)
            .reduce(UiRect::union)
    }

    pub fn project_to_physical(&self, scale: UiScale) -> Self {
        if scale.is_identity() {
            return self.clone();
        }
        Self {
            commands: Arc::new(
                self.commands
                    .iter()
                    .map(|command| project_command(command, scale))
                    .collect(),
            ),
        }
    }
}

fn collect_image_requests(commands: &[ScenePrimitive], requests: &mut Vec<ImageRequest>) {
    for command in commands {
        match command {
            ScenePrimitive::Image { request, .. } => requests.push(request.clone()),
            ScenePrimitive::CompositingLayer { commands, .. }
            | ScenePrimitive::StaticLayer { commands, .. }
            | ScenePrimitive::ScrollRaster { commands, .. }
            | ScenePrimitive::Clip { commands, .. }
            | ScenePrimitive::ClipPath { commands, .. } => {
                collect_image_requests(commands, requests);
            }
            _ => {}
        }
    }
}

pub(crate) fn patch_compositing_layer_spec(
    commands: &mut [ScenePrimitive],
    source: &UiId,
    next_spec: CompositingLayerSpec,
) -> bool {
    for command in commands {
        let nested = match command {
            ScenePrimitive::CompositingLayer {
                id, spec, commands, ..
            } => {
                if id == source {
                    *spec = next_spec;
                    return true;
                }
                Some(commands)
            }
            ScenePrimitive::StaticLayer { commands, .. }
            | ScenePrimitive::ScrollRaster { commands, .. }
            | ScenePrimitive::Clip { commands, .. }
            | ScenePrimitive::ClipPath { commands, .. } => Some(commands),
            _ => None,
        };
        if nested.is_some_and(|commands| patch_compositing_layer_spec(commands, source, next_spec))
        {
            return true;
        }
    }
    false
}
