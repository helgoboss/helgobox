use crate::application::{GroupModel, MappingModel, SharedGroup, SharedMapping, TargetCategory};
use crate::domain::{ExtendedProcessorContext, VirtualFx, VirtualTrack};
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
    context: ExtendedProcessorContext,
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
            if target.supports_track() && target.track_type.get().refers_to_project() {
                return true;
            }
            target.supports_fx() && target.fx_type.get().refers_to_project()
        }
        TargetCategory::Virtual => false,
    }
}

fn make_mapping_project_independent(mapping: &mut MappingModel, context: ExtendedProcessorContext) {
    let target = &mut mapping.target_model;
    match target.category.get() {
        TargetCategory::Reaper => {
            let changed_to_focused_fx = if target.supports_fx() {
                let refers_to_project = target.fx_type.get().refers_to_project();
                if refers_to_project {
                    target.set_virtual_fx(VirtualFx::Focused);
                    true
                } else {
                    false
                }
            } else {
                false
            };
            if target.supports_track() && target.track_type.get().refers_to_project() {
                let new_virtual_track = if changed_to_focused_fx {
                    // Track doesn't matter at all. We change it to <This>. Looks nice.
                    Some(VirtualTrack::This)
                } else if let Ok(t) = target.with_context(context).effective_track() {
                    if let Some(i) = t.index() {
                        Some(VirtualTrack::ByIndex(i))
                    } else {
                        None
                    }
                } else {
                    None
                };
                if let Some(t) = new_virtual_track {
                    target.set_virtual_track(t);
                }
            }
        }
        TargetCategory::Virtual => {}
    }
}
