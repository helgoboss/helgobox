use crate::infrastructure::plugin::RealearnControlSurfaceServerTaskSender;
use crate::infrastructure::server::http::ServerClients;
use axum::error_handling::{HandleErrorExt, HandleErrorLayer};
use axum::handler::Handler;
use axum::http::header::CONTENT_TYPE;
use axum::http::{Method, StatusCode};
use axum::response::Html;
use axum::routing::{get, patch, post};
use axum::Router;
use axum_server::Handle;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::sync::broadcast;
use tower_http::cors::{any, CorsLayer};

mod handlers;
mod layers;

use crate::base::Global;
use crate::infrastructure::server::http_new::layers::MainThreadLayer;
pub use handlers::*;

pub async fn start_new_http_server(
    http_port: u16,
    https_port: u16,
    clients: ServerClients,
    (key, cert): (String, String),
    control_surface_task_sender: RealearnControlSurfaceServerTaskSender,
    mut http_shutdown_receiver: broadcast::Receiver<()>,
    mut https_shutdown_receiver: broadcast::Receiver<()>,
) {
    let app = create_router(cert.clone(), control_surface_task_sender);
    let addr = SocketAddr::from(([0, 0, 0, 0], https_port));
    let rustls_config = axum_server::tls_rustls::RustlsConfig::from_pem(cert.into(), key.into())
        .await
        .unwrap();
    let handle = Handle::new();
    let cloned_handle = handle.clone();
    tokio::spawn(async move {
        http_shutdown_receiver.recv().await.unwrap();
        cloned_handle.graceful_shutdown(Some(Duration::ZERO));
    });
    axum_server::bind_rustls(addr, rustls_config)
        .handle(handle)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

fn create_router(
    cert: String,
    control_surface_task_sender: RealearnControlSurfaceServerTaskSender,
) -> Router {
    let router = Router::new()
        .route("/", get(welcome_handler))
        .route(
            "/realearn.cer",
            get(|| async move { create_cert_response(cert.clone(), "realearn.cer") }),
        )
        .route(
            "/realearn/session/:id",
            get(session_handler.layer(MainThreadLayer)),
        )
        .route(
            "/realearn/session/:id/controller",
            get(session_controller_handler.layer(MainThreadLayer)),
        )
        .route(
            "/realearn/session/:id/controller-routing",
            get(controller_routing_handler.layer(MainThreadLayer)),
        )
        .route(
            "/realearn/controller/:id",
            patch(patch_controller_handler.layer(MainThreadLayer)),
        );
    #[cfg(feature = "realearn-meter")]
    let router = router.route(
        "/realearn/metrics",
        get(|| async move { create_metrics_response(control_surface_task_sender.clone()).await }),
    );
    router.layer(
        CorsLayer::new()
            .allow_origin(any())
            .allow_methods(vec![
                Method::GET,
                Method::POST,
                Method::PUT,
                Method::DELETE,
                Method::PATCH,
            ])
            .allow_headers(vec![CONTENT_TYPE]),
    )
}
