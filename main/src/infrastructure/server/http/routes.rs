//! Contains handlers for the HTTP routes.
use crate::infrastructure::plugin::{App, RealearnControlSurfaceServerTaskSender};
use crate::infrastructure::server::http::data;
use crate::infrastructure::server::http::data::{
    get_controller_routing, obtain_metrics_snapshot, patch_controller, DataError, PatchRequest,
    SessionResponseData,
};
use crate::infrastructure::server::http::server::process_send_result;
use warp::http::{Response, StatusCode};
use warp::reply::Json;
use warp::{reply, Rejection, Reply};

pub fn handle_controller_route(session_id: String) -> Result<Json, Response<&'static str>> {
    let controller_data =
        data::get_controller_preset_data(session_id).map_err(translate_data_error)?;
    Ok(reply::json(&controller_data))
}

pub fn handle_session_route(session_id: String) -> Result<Json, Response<&'static str>> {
    let _ = App::get()
        .find_session_by_id(&session_id)
        .ok_or_else(session_not_found)?;
    Ok(reply::json(&SessionResponseData {}))
}

fn translate_data_error(e: DataError) -> Response<&'static str> {
    use DataError::*;
    match e {
        SessionNotFound => session_not_found(),
        SessionHasNoActiveController => session_has_no_active_controller(),
        ControllerNotFound => controller_not_found(),
        OnlyPatchReplaceIsSupported => Response::builder()
            .status(StatusCode::METHOD_NOT_ALLOWED)
            .body("only 'replace' is supported as op")
            .unwrap(),
        OnlyCustomDataKeyIsSupportedAsPatchPath => Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body("only '/customData/{key}' is supported as path")
            .unwrap(),
        CouldntUpdateController => internal_server_error("couldn't update controller"),
    }
}

#[cfg(feature = "realearn-meter")]
pub async fn handle_metrics_route(
    control_surface_task_sender: RealearnControlSurfaceServerTaskSender,
) -> Result<Box<dyn Reply>, Rejection> {
    let snapshot = obtain_metrics_snapshot(control_surface_task_sender).await;
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
    patch_controller(controller_id, req).map_err(translate_data_error)?;
    Ok(StatusCode::OK)
}

fn internal_server_error(msg: &'static str) -> Response<&'static str> {
    Response::builder()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .body(msg)
        .unwrap()
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
