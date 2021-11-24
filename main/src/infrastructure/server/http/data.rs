//! Should be without HTTP framework stuff.

use crate::application::{Preset, PresetManager, Session, SourceCategory, TargetCategory};
use crate::domain::{MappingCompartment, MappingKey, RealearnControlSurfaceServerTask};
use crate::infrastructure::data::{ControllerPresetData, PresetData};
use crate::infrastructure::plugin::{App, RealearnControlSurfaceServerTaskSender};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
// Right now just a placeholder
pub struct SessionResponseData {}

pub enum DataError {
    SessionNotFound,
    SessionHasNoActiveController,
    ControllerNotFound,
    OnlyPatchReplaceIsSupported,
    OnlyCustomDataKeyIsSupportedAsPatchPath,
    CouldntUpdateController,
}

#[derive(Deserialize)]
pub struct PatchRequest {
    op: PatchRequestOp,
    path: String,
    value: serde_json::value::Value,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum PatchRequestOp {
    Replace,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ControllerRouting {
    main_preset: Option<LightMainPresetData>,
    routes: HashMap<MappingKey, Vec<TargetDescriptor>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LightMainPresetData {
    id: String,
    name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TargetDescriptor {
    label: String,
}

pub fn get_controller_preset_data(session_id: String) -> Result<ControllerPresetData, DataError> {
    let session = App::get()
        .find_session_by_id(&session_id)
        .ok_or(DataError::SessionNotFound)?;
    let session = session.borrow();
    let controller_id = session
        .active_controller_preset_id()
        .ok_or(DataError::SessionHasNoActiveController)?;
    let controller = App::get()
        .controller_preset_manager()
        .borrow()
        .find_by_id(controller_id)
        .ok_or(DataError::ControllerNotFound)?;
    Ok(ControllerPresetData::from_model(&controller))
}

#[cfg(feature = "realearn-meter")]
pub async fn obtain_metrics_snapshot(
    control_surface_task_sender: RealearnControlSurfaceServerTaskSender,
) -> Result<Result<String, String>, tokio::sync::oneshot::error::RecvError> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    control_surface_task_sender
        .try_send(RealearnControlSurfaceServerTask::ProvidePrometheusMetrics(
            sender,
        ))
        .unwrap();
    receiver.await.map(Ok)
}

pub fn get_controller_routing(session: &Session) -> ControllerRouting {
    let main_preset = session.active_main_preset().map(|mp| LightMainPresetData {
        id: mp.id().to_string(),
        name: mp.name().to_string(),
    });
    let instance_state = session.instance_state().borrow();
    let routes = session
        .mappings(MappingCompartment::ControllerMappings)
        .filter_map(|m| {
            let m = m.borrow();
            if !m.visible_in_projection.get() {
                return None;
            }
            let target_descriptor = if instance_state.mapping_is_on(m.qualified_id()) {
                if m.target_model.category.get() == TargetCategory::Virtual {
                    // Virtual
                    let control_element = m.target_model.create_control_element();
                    let matching_main_mappings = session
                        .mappings(MappingCompartment::MainMappings)
                        .filter(|mp| {
                            let mp = mp.borrow();
                            mp.visible_in_projection.get()
                                && mp.source_model.category.get() == SourceCategory::Virtual
                                && mp.source_model.create_control_element() == control_element
                                && instance_state.mapping_is_on(mp.qualified_id())
                        });
                    let descriptors: Vec<_> = matching_main_mappings
                        .map(|m| {
                            let m = m.borrow();
                            TargetDescriptor {
                                label: m.effective_name(),
                            }
                        })
                        .collect();
                    if descriptors.is_empty() {
                        return None;
                    }
                    descriptors
                } else {
                    // Direct
                    let single_descriptor = TargetDescriptor {
                        label: m.effective_name(),
                    };
                    vec![single_descriptor]
                }
            } else {
                return None;
            };
            Some((m.key().clone(), target_descriptor))
        })
        .collect();
    ControllerRouting {
        main_preset,
        routes,
    }
}

pub fn patch_controller(controller_id: String, req: PatchRequest) -> Result<(), DataError> {
    if req.op != PatchRequestOp::Replace {
        return Err(DataError::OnlyPatchReplaceIsSupported);
    }
    let split_path: Vec<_> = req.path.split('/').collect();
    let custom_data_key = if let ["", "customData", key] = split_path.as_slice() {
        key
    } else {
        return Err(DataError::OnlyCustomDataKeyIsSupportedAsPatchPath);
    };
    let controller_manager = App::get().controller_preset_manager();
    let mut controller_manager = controller_manager.borrow_mut();
    let mut controller = controller_manager
        .find_by_id(&controller_id)
        .ok_or(DataError::ControllerNotFound)?;
    controller.update_custom_data(custom_data_key.to_string(), req.value);
    controller_manager
        .update_preset(controller)
        .map_err(|_| DataError::CouldntUpdateController)?;
    Ok(())
}
