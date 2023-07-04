use crate::infrastructure::plugin::App;
use crate::infrastructure::server::http::ServerClients;
use axum::extract::{Query, WebSocketUpgrade};
use axum::handler::Handler;
use axum::http::header::CONTENT_TYPE;
use axum::http::Method;
use axum::routing::{get, patch};
use axum::Router;
use std::io;
use std::net::SocketAddr;
use tower_http::cors::{any, CorsLayer};

use crate::infrastructure::server::data::WebSocketRequest;
pub use crate::infrastructure::server::http::handlers::*;
use crate::infrastructure::server::layers::MainThreadLayer;
use crate::infrastructure::server::MetricsReporter;
use base::Global;

#[allow(clippy::too_many_arguments)]
pub async fn start_http_server(
    http_port: u16,
    https_port: u16,
    clients: ServerClients,
    (key, cert): (String, String),
    metrics_reporter: MetricsReporter,
) -> Result<(), io::Error> {
    // Router
    let router = create_router(cert.clone(), clients, metrics_reporter);
    // Binding
    let http_future = {
        let addr = SocketAddr::from(([0, 0, 0, 0], http_port));
        axum_server::bind(addr).serve(router.clone().into_make_service())
    };
    let https_future = {
        let addr = SocketAddr::from(([0, 0, 0, 0], https_port));
        let rustls_config =
            axum_server::tls_rustls::RustlsConfig::from_pem(cert.into(), key.into())
                .await
                .unwrap();
        axum_server::bind_rustls(addr, rustls_config).serve(router.into_make_service())
    };
    // Notify UI
    Global::task_support()
        .do_later_in_main_thread_asap(|| {
            App::get().server().borrow_mut().notify_started();
        })
        .unwrap();
    // Actually await the bind futures
    let (http_result, https_result) = futures::future::join(http_future, https_future).await;
    http_result?;
    https_result?;
    Ok(())
}

fn create_router(
    cert: String,
    clients: ServerClients,
    metrics_reporter: MetricsReporter,
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
        )
        .route(
            "/realearn/metrics",
            get(move || async move { create_metrics_response(metrics_reporter).await }),
        );
    router
        .layer(
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
        .route(
            "/ws",
            get(
                |ws: WebSocketUpgrade, Query(req): Query<WebSocketRequest>| async move {
                    let topics = req.parse_topics();
                    ws.on_upgrade(|socket| handle_websocket_upgrade(socket, topics, clients))
                },
            ),
        )
}
