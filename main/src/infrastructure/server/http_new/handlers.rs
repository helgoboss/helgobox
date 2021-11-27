use crate::infrastructure::data::ControllerPresetData;
use crate::infrastructure::plugin::{App, RealearnControlSurfaceServerTaskSender};
use crate::infrastructure::server::http::{
    get_controller_preset_data, get_controller_routing_by_session_id, get_session_data,
    obtain_metrics_snapshot, patch_controller, ControllerRouting, DataError, PatchRequest,
    SessionResponseData,
};
use axum::body::{boxed, Body, BoxBody};
use axum::extract::Path;
use axum::http::{Response, StatusCode};
use axum::response::Html;
use axum::Json;

type SimpleResponse = (StatusCode, &'static str);

pub async fn welcome_handler() -> Html<&'static str> {
    Html(include_str!("../http/welcome_page.html"))
}

/// Needs to be executed in the main thread!
pub async fn session_handler(
    Path(session_id): Path<String>,
) -> Result<Json<SessionResponseData>, SimpleResponse> {
    let session_data = get_session_data(session_id).map_err(translate_data_error)?;
    Ok(Json(session_data))
}

/// Needs to be executed in the main thread!
pub async fn session_controller_handler(
    Path(session_id): Path<String>,
) -> Result<Json<ControllerPresetData>, SimpleResponse> {
    let controller_data = get_controller_preset_data(session_id).map_err(translate_data_error)?;
    Ok(Json(controller_data))
}

/// Needs to be executed in the main thread!
pub async fn controller_routing_handler(
    Path(session_id): Path<String>,
) -> Result<Json<ControllerRouting>, SimpleResponse> {
    let controller_routing =
        get_controller_routing_by_session_id(session_id).map_err(translate_data_error)?;
    Ok(Json(controller_routing))
}

/// Needs to be executed in the main thread!
pub async fn patch_controller_handler(
    Path(controller_id): Path<String>,
    Json(patch_request): Json<PatchRequest>,
) -> Result<StatusCode, SimpleResponse> {
    patch_controller(controller_id, patch_request).map_err(translate_data_error)?;
    Ok(StatusCode::OK)
}

pub fn create_cert_response(cert: String, cert_file_name: &str) -> Response<BoxBody> {
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/pkix-cert")
        .header(
            "Content-Disposition",
            format!("attachment; filename=\"{}\"", cert_file_name),
        )
        .body(boxed(Body::from(cert)))
        .unwrap()
}

#[cfg(feature = "realearn-meter")]
pub async fn create_metrics_response(
    control_surface_task_sender: RealearnControlSurfaceServerTaskSender,
) -> Response<BoxBody> {
    obtain_metrics_snapshot(control_surface_task_sender)
        .await
        .map(|r| {
            let text = match r {
                Ok(text) => text,
                Err(text) => text,
            };
            Response::builder()
                .status(StatusCode::OK)
                .body(boxed(Body::from(text)))
                .unwrap()
        })
        .unwrap_or_else(|_| {
            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(boxed(Body::from("sender dropped")))
                .unwrap()
        })
}

fn translate_data_error(e: DataError) -> SimpleResponse {
    use DataError::*;
    match e {
        SessionNotFound => not_found("session not found"),
        SessionHasNoActiveController => not_found("session doesn't have an active controller"),
        ControllerNotFound => not_found("session has controller but controller not found"),
        OnlyPatchReplaceIsSupported => (
            StatusCode::METHOD_NOT_ALLOWED,
            "only 'replace' is supported as op",
        ),
        OnlyCustomDataKeyIsSupportedAsPatchPath => (
            StatusCode::BAD_REQUEST,
            "only '/customData/{key}' is supported as path",
        ),
        CouldntUpdateController => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "couldn't update controller",
        ),
    }
}

const fn not_found(msg: &'static str) -> SimpleResponse {
    (StatusCode::NOT_FOUND, msg)
}
