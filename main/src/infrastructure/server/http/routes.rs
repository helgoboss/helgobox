use crate::application::{Preset, PresetManager, Session, SourceCategory, TargetCategory};
use crate::domain::{MappingCompartment, RealearnControlSurfaceServerTask};
use crate::infrastructure::data::{ControllerPresetData, PresetData};
use crate::infrastructure::plugin::{App, RealearnControlSurfaceServerTaskSender};
use crate::infrastructure::server::http::server::process_send_result;
use crate::infrastructure::server::{ControllerRouting, LightMainPresetData, TargetDescriptor};
use serde::{Deserialize, Serialize};
use warp::http::{Method, Response, StatusCode};
use warp::reply::Json;
use warp::{reply, Rejection, Reply};

pub fn handle_controller_route(session_id: String) -> Result<Json, Response<&'static str>> {
    let session = App::get()
        .find_session_by_id(&session_id)
        .ok_or_else(session_not_found)?;
    let session = session.borrow();
    let controller_id = session
        .active_controller_preset_id()
        .ok_or_else(session_has_no_active_controller)?;
    let controller = App::get()
        .controller_preset_manager()
        .borrow()
        .find_by_id(controller_id)
        .ok_or_else(controller_not_found)?;
    let controller_data = ControllerPresetData::from_model(&controller);
    Ok(reply::json(&controller_data))
}

pub fn handle_session_route(session_id: String) -> Result<Json, Response<&'static str>> {
    let _ = App::get()
        .find_session_by_id(&session_id)
        .ok_or_else(session_not_found)?;
    Ok(reply::json(&SessionResponseData {}))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
// Right now just a placeholder
pub struct SessionResponseData {}

#[cfg(feature = "realearn-meter")]
pub async fn handle_metrics_route(
    control_surface_task_sender: RealearnControlSurfaceServerTaskSender,
) -> Result<Box<dyn Reply>, Rejection> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    control_surface_task_sender
        .try_send(RealearnControlSurfaceServerTask::ProvidePrometheusMetrics(
            sender,
        ))
        .unwrap();
    let snapshot: Result<Result<String, String>, _> = receiver.await.map(Ok);
    process_send_result(snapshot).await
}

pub fn handle_controller_routing_route(session_id: String) -> Result<Json, Response<&'static str>> {
    let session = App::get()
        .find_session_by_id(&session_id)
        .ok_or_else(session_not_found)?;
    let routing = get_controller_routing(&session.borrow());
    Ok(reply::json(&routing))
}

pub fn handle_patch_controller_route(
    controller_id: String,
    req: PatchRequest,
) -> Result<StatusCode, Response<&'static str>> {
    if req.op != PatchRequestOp::Replace {
        return Err(Response::builder()
            .status(StatusCode::METHOD_NOT_ALLOWED)
            .body("only 'replace' is supported as op")
            .unwrap());
    }
    let split_path: Vec<_> = req.path.split('/').collect();
    let custom_data_key = if let ["", "customData", key] = split_path.as_slice() {
        key
    } else {
        return Err(Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body("only '/customData/{key}' is supported as path")
            .unwrap());
    };
    let controller_manager = App::get().controller_preset_manager();
    let mut controller_manager = controller_manager.borrow_mut();
    let mut controller = controller_manager
        .find_by_id(&controller_id)
        .ok_or_else(controller_not_found)?;
    controller.update_custom_data(custom_data_key.to_string(), req.value);
    controller_manager
        .update_preset(controller)
        .map_err(|_| internal_server_error("couldn't update controller"))?;
    Ok(StatusCode::OK)
}

#[derive(Deserialize)]
pub struct PatchRequest {
    op: PatchRequestOp,
    path: String,
    value: serde_json::value::Value,
}

fn internal_server_error(msg: &'static str) -> Response<&'static str> {
    Response::builder()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .body(msg)
        .unwrap()
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum PatchRequestOp {
    Replace,
}

fn session_not_found() -> Response<&'static str> {
    not_found("session not found")
}

fn session_has_no_active_controller() -> Response<&'static str> {
    not_found("session doesn't have an active controller")
}

fn controller_not_found() -> Response<&'static str> {
    not_found("session has controller but controller not found")
}

fn not_found(msg: &'static str) -> Response<&'static str> {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(msg)
        .unwrap()
}

pub fn sender_dropped_response() -> Response<&'static str> {
    Response::builder()
        .status(500)
        .body("sender dropped")
        .unwrap()
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
