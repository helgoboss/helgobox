//! Contains the mainly technical HTTP/WebSocket server code.

use crate::base::Global;
use crate::infrastructure::plugin::{App, RealearnControlSurfaceServerTaskSender};
use crate::infrastructure::server::http::data::{PatchRequest, Topic, Topics, WebSocketRequest};
use crate::infrastructure::server::http::routes::{
    handle_controller_route, handle_controller_routing_route, handle_metrics_route,
    handle_patch_controller_route, handle_session_route,
};
use crate::infrastructure::server::http::{send_initial_events, sender_dropped_response};
use futures::StreamExt;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::convert::TryFrom;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};
use tokio_stream::wrappers::UnboundedReceiverStream;
use warp::http::{Method, Response};
use warp::ws::{Message, WebSocket};
use warp::{Rejection, Reply};

// We don't take the async RwLock by Tokio because we need to access this in sync code, too!
pub type ServerClients = Arc<std::sync::RwLock<HashMap<usize, WebSocketClient>>>;

#[derive(Debug, Clone)]
pub struct WebSocketClient {
    id: usize,
    pub topics: Topics,
    sender: mpsc::UnboundedSender<String>,
}

impl WebSocketClient {
    pub fn send(&self, msg: impl Serialize) -> Result<(), &'static str> {
        let json = serde_json::to_string(&msg).map_err(|_| "couldn't serialize")?;
        self.sender.send(json).map_err(|_| "couldn't send")
    }

    pub fn is_subscribed_to(&self, topic: &Topic) -> bool {
        self.topics.contains(topic)
    }
}

pub async fn start_http_server(
    http_port: u16,
    https_port: u16,
    clients: ServerClients,
    (key, cert): (String, String),
    control_surface_task_sender: RealearnControlSurfaceServerTaskSender,
    mut http_shutdown_receiver: broadcast::Receiver<()>,
    mut https_shutdown_receiver: broadcast::Receiver<()>,
) {
    use warp::Filter;
    let welcome_route = warp::path::end()
        .and(warp::head().or(warp::get()))
        .map(|_| warp::reply::html(include_str!("welcome_page.html")));
    let session_route = warp::get()
        .and(warp::path!("realearn" / "session" / String))
        .and_then(|session_id| in_main_thread(|| handle_session_route(percent_decode(session_id))));
    let controller_route = warp::get()
        .and(warp::path!("realearn" / "session" / String / "controller"))
        .and_then(|session_id| {
            in_main_thread(|| handle_controller_route(percent_decode(session_id)))
        });
    let controller_routing_route = warp::get()
        .and(warp::path!(
            "realearn" / "session" / String / "controller-routing"
        ))
        .and_then(|session_id| {
            in_main_thread(|| handle_controller_routing_route(percent_decode(session_id)))
        });
    let patch_controller_route = warp::patch()
        .and(warp::path!("realearn" / "controller" / String))
        .and(warp::body::json())
        .and_then(|controller_id: String, req: PatchRequest| {
            in_main_thread(move || {
                handle_patch_controller_route(percent_decode(controller_id), req)
            })
        });

    #[cfg(feature = "realearn-meter")]
    let metrics_route = warp::get()
        .and(warp::path!("realearn" / "metrics"))
        .and_then(move || handle_metrics_route(control_surface_task_sender.clone()));
    let ws_route = {
        let clients = warp::any().map(move || clients.clone());
        warp::path("ws")
            .and(warp::ws())
            .and(warp::query::<WebSocketRequest>())
            .and(clients)
            .map(|ws: warp::ws::Ws, req: WebSocketRequest, clients| {
                let topics: HashSet<_> = req
                    .topics
                    .split(',')
                    .map(Topic::try_from)
                    .flatten()
                    .collect();
                ws.on_upgrade(move |ws| client_connected(ws, topics, clients))
            })
    };
    let cert_clone = cert.clone();
    let cert_file_name = "realearn.cer";
    let cert_route = warp::get()
        .and(warp::path(cert_file_name).and(warp::path::end()))
        .map(move || {
            let cert_clone = cert_clone.clone();
            Response::builder()
                .status(200)
                .header("Content-Type", "application/pkix-cert")
                .header(
                    "Content-Disposition",
                    format!("attachment; filename=\"{}\"", cert_file_name),
                )
                .body(cert_clone)
                .unwrap()
        });
    let cors = warp::cors()
        .allow_any_origin()
        .allow_methods(&[
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::PATCH,
        ])
        .allow_header("Content-Type");
    let routes = welcome_route
        .or(cert_route)
        .or(session_route)
        .or(controller_route)
        .or(controller_routing_route)
        .or(patch_controller_route)
        // TODO
        .or(ws_route);
    #[cfg(feature = "realearn-meter")]
    let routes = routes.or(metrics_route);
    let routes = routes.with(cors);
    let (_, http_future) = warp::serve(routes.clone())
        .bind_with_graceful_shutdown(([0, 0, 0, 0], http_port), async move {
            http_shutdown_receiver.recv().await.unwrap()
        });
    let (_, https_future) = warp::serve(routes)
        .tls()
        .key(key)
        .cert(cert)
        .bind_with_graceful_shutdown(([0, 0, 0, 0], https_port), async move {
            https_shutdown_receiver.recv().await.unwrap()
        });
    Global::task_support()
        .do_later_in_main_thread_asap(|| {
            App::get().server().borrow_mut().notify_started();
        })
        .unwrap();
    futures::future::join(http_future, https_future).await;
}

async fn in_main_thread<O: Reply + 'static, E: Reply + 'static>(
    op: impl FnOnce() -> Result<O, E> + 'static + Send,
) -> Result<Box<dyn Reply>, Rejection> {
    let send_result = Global::task_support()
        .main_thread_future(move || op())
        .await;
    process_send_result(send_result).await
}

pub async fn process_send_result<O: Reply + 'static, E: Reply + 'static, SE>(
    send_result: Result<Result<O, E>, SE>,
) -> Result<Box<dyn Reply>, Rejection> {
    let response_result = match send_result {
        Ok(r) => r,
        Err(_) => {
            return Ok(Box::new(sender_dropped_response()));
        }
    };
    let raw: Box<dyn Reply> = match response_result {
        Ok(r) => Box::new(r),
        Err(r) => Box::new(r),
    };
    Ok(raw)
}

fn percent_decode(input: String) -> String {
    percent_encoding::percent_decode(input.as_bytes())
        .decode_utf8()
        .unwrap()
        .into_owned()
}

async fn client_connected(ws: WebSocket, topics: Topics, clients: ServerClients) {
    use futures::FutureExt;
    let (ws_sender_sink, mut ws_receiver_stream) = ws.split();
    let (client_sender, client_receiver) = mpsc::unbounded_channel();
    let client_receiver_stream = UnboundedReceiverStream::new(client_receiver);
    // Keep forwarding received messages in client channel to websocket sender sink
    tokio::task::spawn(
        client_receiver_stream
            .map(|json| Ok(Message::text(json)))
            .forward(ws_sender_sink)
            .map(|result| {
                if let Err(e) = result {
                    eprintln!("error sending websocket msg: {}", e);
                }
            }),
    );
    let client_id = NEXT_CLIENT_ID.fetch_add(1, Ordering::Relaxed);
    let client = WebSocketClient {
        id: client_id,
        topics,
        sender: client_sender,
    };
    clients.write().unwrap().insert(client_id, client.clone());
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

static NEXT_CLIENT_ID: AtomicUsize = AtomicUsize::new(1);
