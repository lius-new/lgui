use super::*;

impl HostRuntime {
    pub(super) fn calculate_damage(
        &self,
        viewport: UiRect,
        mutations: &[HostMutation],
        invalidations: &mut InvalidationSet,
    ) -> DamageReport {
        let mut dirty = DirtyRegionSet::new(viewport);
        let mut reasons = Vec::new();
        let mut details = Vec::new();
        if !self.initialized {
            dirty.mark_full_with_reason("first-commit");
            reasons.push(DamageReason::FirstCommit);
            details.push(DamageDetail {
                reason: DamageReason::FirstCommit,
                node_id: None,
                old_bounds: None,
                new_bounds: Some(viewport),
                rects: vec![viewport],
            });
        }
        for mutation in mutations {
            match mutation {
                HostMutation::InsertNode { source, bounds, .. } => {
                    dirty.add(*bounds);
                    reasons.push(DamageReason::Insert);
                    details.push(DamageDetail {
                        reason: DamageReason::Insert,
                        node_id: Some(source.clone()),
                        old_bounds: None,
                        new_bounds: Some(*bounds),
                        rects: vec![*bounds],
                    });
                }
                HostMutation::RemoveNode {
                    source, old_bounds, ..
                } => {
                    dirty.add(*old_bounds);
                    reasons.push(DamageReason::Remove);
                    details.push(DamageDetail {
                        reason: DamageReason::Remove,
                        node_id: Some(source.clone()),
                        old_bounds: Some(*old_bounds),
                        new_bounds: None,
                        rects: vec![*old_bounds],
                    });
                }
                HostMutation::UpdateProps {
                    source,
                    kind,
                    old_bounds,
                    new_bounds,
                    ..
                } => {
                    dirty.add(*old_bounds);
                    dirty.add(*new_bounds);
                    let reason = match kind {
                        HostUpdateKind::Layout => DamageReason::Layout,
                        HostUpdateKind::Paint => DamageReason::Paint,
                        HostUpdateKind::Interaction => DamageReason::Interaction,
                    };
                    reasons.push(reason);
                    details.push(DamageDetail {
                        reason,
                        node_id: Some(source.clone()),
                        old_bounds: Some(*old_bounds),
                        new_bounds: Some(*new_bounds),
                        rects: vec![*old_bounds, *new_bounds],
                    });
                }
                HostMutation::ReorderChildren { source, bounds, .. } => {
                    dirty.add(*bounds);
                    reasons.push(DamageReason::Structure);
                    details.push(DamageDetail {
                        reason: DamageReason::Structure,
                        node_id: Some(source.clone()),
                        old_bounds: Some(*bounds),
                        new_bounds: Some(*bounds),
                        rects: vec![*bounds],
                    });
                }
            }
        }
        for request in invalidations.drain() {
            match request {
                InvalidationRequest::All | InvalidationRequest::Route => {
                    dirty.mark_full_with_reason("explicit");
                }
                InvalidationRequest::Rect(rect) => dirty.add(rect),
                InvalidationRequest::Node(source)
                | InvalidationRequest::Animation { id: source, .. } => {
                    if let Some(id) = self.sources.get(&source) {
                        dirty.add(self.node(*id).paint_bounds);
                    }
                }
            }
            reasons.push(DamageReason::Explicit);
        }
        if reasons.is_empty() {
            reasons.push(DamageReason::Clean);
            details.push(DamageDetail {
                reason: DamageReason::Clean,
                node_id: None,
                old_bounds: None,
                new_bounds: None,
                rects: Vec::new(),
            });
        }
        DamageReport {
            dirty,
            reasons,
            details,
        }
    }
}
