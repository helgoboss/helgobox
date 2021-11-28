use crate::base::Global;
use crate::infrastructure::data::ControllerPresetData;
use crate::infrastructure::plugin::{App, RealearnControlSurfaceServerTaskSender};
use crate::infrastructure::server::http::{
    get_controller_preset_data, get_controller_routing_by_session_id, get_session_data,
    obtain_metrics_snapshot, patch_controller, send_initial_events, ControllerRouting, DataError,
    PatchRequest, ServerClients, SessionResponseData, Topic, Topics, WebSocketClient,
    WebSocketRequest,
};
use axum::body::{boxed, Body, BoxBody};
use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Path, Query, WebSocketUpgrade};
use axum::http::{Response, StatusCode};
use axum::response::{Html, IntoResponse};
use axum::Json;
use std::collections::HashSet;
use std::convert::TryFrom;
use std::future::Future;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;

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

pub async fn handle_websocket_upgrade(
    mut socket: WebSocket,
    topics: Topics,
    clients: ServerClients,
) {
    use futures::{FutureExt, StreamExt};
    let (ws_sender_sink, mut ws_receiver_stream) = socket.split();
    let (client_sender, client_receiver) = mpsc::unbounded_channel();
    let client_receiver_stream = UnboundedReceiverStream::new(client_receiver);
    // Keep forwarding received messages in client channel to websocket sender sink
    tokio::task::spawn(
        client_receiver_stream
            .map(|json| Ok(Message::Text(json)))
            .forward(ws_sender_sink)
            .map(|result| {
                if let Err(e) = result {
                    eprintln!("error sending websocket msg: {}", e);
                }
            }),
    );
    // Create client struct
    static NEXT_CLIENT_ID: AtomicUsize = AtomicUsize::new(1);
    let client_id = NEXT_CLIENT_ID.fetch_add(1, Ordering::Relaxed);
    let client = WebSocketClient {
        id: client_id,
        topics,
        sender: client_sender,
    };
    // Memorize client
    clients.write().unwrap().insert(client_id, client.clone());
    // Send initial events
    Global::task_support()
        .do_later_in_main_thread_asap(move || {
            send_initial_events(&client);
        })
        .unwrap();
    // Keep receiving websocket receiver stream messages
    while let Some(result) = ws_receiver_stream.next().await {
        // We will need this as soon as we are interested in what the client says
        let _msg = match result {
            Ok(msg) => msg,
            Err(e) => {
                eprintln!("websocket error: {}", e);
                break;
            }
        };
    }
    // Stream closed up, so remove from the client list
    clients.write().unwrap().remove(&client_id);
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
