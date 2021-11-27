use crate::infrastructure::plugin::RealearnControlSurfaceServerTaskSender;
use crate::infrastructure::server::http::ServerClients;
use axum::routing::{get, post};
use axum::Router;
use axum_server::Handle;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::sync::broadcast;

pub async fn start_new_http_server_old(
    http_port: u16,
    https_port: u16,
    clients: ServerClients,
    (key, cert): (String, String),
    control_surface_task_sender: RealearnControlSurfaceServerTaskSender,
    mut http_shutdown_receiver: broadcast::Receiver<()>,
    mut https_shutdown_receiver: broadcast::Receiver<()>,
) {
    let app = Router::new().route("/", get(root));
    let addr = SocketAddr::from(([0, 0, 0, 0], http_port));
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .with_graceful_shutdown(async move { http_shutdown_receiver.recv().await.unwrap() })
        .await;
}

pub async fn start_new_http_server(
    http_port: u16,
    https_port: u16,
    clients: ServerClients,
    (key, cert): (String, String),
    control_surface_task_sender: RealearnControlSurfaceServerTaskSender,
    mut http_shutdown_receiver: broadcast::Receiver<()>,
    mut https_shutdown_receiver: broadcast::Receiver<()>,
) {
    let app = Router::new().route("/", get(root));
    let addr = SocketAddr::from(([0, 0, 0, 0], http_port));
    let rustls_config = axum_server::tls_rustls::RustlsConfig::from_pem(cert.into(), key.into())
        .await
        .unwrap();
    let handle = Handle::new();
    let cloned_handle = handle.clone();
    tokio::spawn(async move {
        http_shutdown_receiver.recv().await.unwrap();
        cloned_handle.graceful_shutdown(Some(Duration::from_secs(1)));
    });
    axum_server::bind_rustls(addr, rustls_config)
        .handle(handle)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

// basic handler that responds with a static string
async fn root() -> &'static str {
    "Hello, World!"
}
