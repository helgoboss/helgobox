use crate::application::{GroupModel, MappingModel, SharedGroup, SharedMapping, TargetCategory};
use crate::domain::{ProcessorContext, TrackAnchor, VirtualFx, VirtualTrack};
use std::fmt;
use std::fmt::Debug;

pub trait Preset: Clone + Debug {
    fn id(&self) -> &str;
    fn default_group(&self) -> &GroupModel;
    fn groups(&self) -> &Vec<GroupModel>;
    fn mappings(&self) -> &Vec<MappingModel>;
}

pub trait PresetManager: fmt::Debug {
    type PresetType;

    fn find_by_id(&self, id: &str) -> Option<Self::PresetType>;

    fn mappings_are_dirty(&self, id: &str, mappings: &[SharedMapping]) -> bool;

    fn groups_are_dirty(
        &self,
        id: &str,
        default_group: &SharedGroup,
        groups: &[SharedGroup],
    ) -> bool;
}

pub fn mappings_have_project_references(mappings: &[MappingModel]) -> bool {
    mappings.iter().any(mapping_has_project_references)
}

pub fn make_mappings_project_independent(
    mappings: &mut [MappingModel],
    context: &ProcessorContext,
) {
    mappings
        .iter_mut()
        .for_each(|m| make_mapping_project_independent(m, context));
}

/// Checks if the given mapping has references to a project, e.g. refers to track or FX by ID.
fn mapping_has_project_references(mapping: &MappingModel) -> bool {
    let target = &mapping.target_model;
    match target.category.get() {
        TargetCategory::Reaper => {
            if target.supports_track() && target.track.get_ref().refers_to_project() {
                return true;
            }
            if target.supports_fx() {
                if let Some(fx) = target.fx.get_ref() {
                    if fx.refers_to_project() {
                        return true;
                    }
                }
            }
            false
        }
        TargetCategory::Virtual => false,
    }
}

fn make_mapping_project_independent(mapping: &mut MappingModel, context: &ProcessorContext) {
    let target = &mut mapping.target_model;
    match target.category.get() {
        TargetCategory::Reaper => {
            let changed_to_focused_fx = if target.supports_fx() {
                let refers_to_project = target
                    .fx
                    .get_ref()
                    .as_ref()
                    .map(VirtualFx::refers_to_project)
                    .unwrap_or(false);
                if refers_to_project {
                    target.fx.set(Some(VirtualFx::Focused));
                    true
                } else {
                    false
                }
            } else {
                false
            };
            if target.supports_track() && target.track.get_ref().refers_to_project() {
                let new_virtual_track = if changed_to_focused_fx {
                    // Track doesn't matter at all. We change it to <This>. Looks nice.
                    Some(VirtualTrack::This)
                } else if let Ok(t) = target.with_context(context).effective_track() {
                    if let Some(i) = t.index() {
                        Some(VirtualTrack::Particular(TrackAnchor::Index(i)))
                    } else {
                        None
                    }
                } else {
                    None
                };
                if let Some(t) = new_virtual_track {
                    target.track.set(t);
                }
            }
        }
        TargetCategory::Virtual => {}
    }
}
