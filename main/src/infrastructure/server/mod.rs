use crate::application::{
    session_manager, ControllerManager, Session, SharedSession, SourceCategory, TargetCategory,
};
use crate::core::when;
use crate::domain::MappingCompartment;
use crate::infrastructure::data::ControllerData;
use crate::infrastructure::plugin::App;
use futures::SinkExt;
use reaper_high::Reaper;
use rxrust::prelude::*;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::convert::TryFrom;
use std::fmt::Debug;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::{JoinHandle, Thread};
use tokio::sync::{mpsc, RwLock};
use warp::http::{Response, StatusCode};
use warp::reject::Reject;
use warp::reply::Json;
use warp::ws::{Message, WebSocket};
use warp::{reject, reply, Rejection, Reply};

pub type SharedRealearnServer = Rc<RefCell<RealearnServer>>;

pub struct RealearnServer {
    port: u16,
    state: ServerState,
}

enum ServerState {
    Stopped,
    Started { clients: ServerClients },
}

impl RealearnServer {
    pub fn new(port: u16) -> RealearnServer {
        RealearnServer {
            port,
            state: ServerState::Stopped,
        }
    }

    /// Idempotent
    pub fn start(&mut self) {
        if let ServerState::Started { .. } = self.state {
            return;
        }
        let clients: ServerClients = Default::default();
        let clients_clone = clients.clone();
        let port = self.port;
        std::thread::spawn(move || {
            let mut runtime = tokio::runtime::Builder::new()
                .basic_scheduler()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(start_server(port, clients_clone));
        });
        self.state = ServerState::Started { clients };
    }

    pub fn clients(&self) -> Result<&ServerClients, &'static str> {
        if let ServerState::Started { clients, .. } = &self.state {
            Ok(clients)
        } else {
            Err("server not running")
        }
    }
}

static NEXT_CLIENT_ID: AtomicUsize = AtomicUsize::new(1);

async fn in_main_thread<O: Reply + 'static, E: Reply + 'static>(
    op: impl FnOnce() -> Result<O, E> + 'static,
) -> Result<Box<dyn Reply>, Rejection> {
    let send_result = Reaper::get().main_thread_future(move || op()).await;
    let response_result = match send_result {
        Ok(r) => r,
        Err(_) => {
            return Ok(Box::new(
                Response::builder()
                    .status(500)
                    .body("sender dropped")
                    .unwrap(),
            ));
        }
    };
    let raw: Box<dyn Reply> = match response_result {
        Ok(r) => Box::new(r),
        Err(r) => Box::new(r),
    };
    Ok(raw)
}

fn handle_controller_routing_route(session_id: String) -> Result<Json, Response<&'static str>> {
    let session = session_manager::find_session_by_id(&session_id).ok_or_else(session_not_found)?;
    let routing = get_controller_routing(&session.borrow());
    Ok(reply::json(&routing))
}

fn handle_patch_controller_route(
    session_id: String,
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
    let session = session_manager::find_session_by_id(&session_id).ok_or_else(session_not_found)?;
    let session = session.borrow();
    let controller_id = session
        .active_controller_id()
        .ok_or_else(session_has_no_active_controller)?;
    let controller_manager = App::get().controller_manager();
    let mut controller_manager = controller_manager.borrow_mut();
    let mut controller = controller_manager
        .find_by_id(controller_id)
        .ok_or_else(controller_not_found)?;
    controller.update_custom_data(custom_data_key.to_string(), req.value);
    controller_manager
        .update_controller(controller)
        .map_err(|_| internal_server_error("couldn't update controller"))?;
    Ok(StatusCode::OK)
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

fn internal_server_error(msg: &'static str) -> Response<&'static str> {
    Response::builder()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .body(msg)
        .unwrap()
}

fn bad_request(msg: &'static str) -> Response<&'static str> {
    Response::builder()
        .status(StatusCode::BAD_REQUEST)
        .body(msg)
        .unwrap()
}

fn handle_controller_route(session_id: String) -> Result<Json, Response<&'static str>> {
    let session = session_manager::find_session_by_id(&session_id).ok_or_else(session_not_found)?;
    let session = session.borrow();
    let controller_id = session
        .active_controller_id()
        .ok_or_else(session_has_no_active_controller)?;
    let controller = App::get()
        .controller_manager()
        .borrow()
        .find_by_id(controller_id)
        .ok_or_else(controller_not_found)?;
    let controller_data = ControllerData::from_model(&controller);
    Ok(reply::json(&controller_data))
}

async fn start_server(port: u16, clients: ServerClients) {
    use warp::Filter;
    let controller_route = warp::get()
        .and(warp::path!("realearn" / String / "controller"))
        .and_then(|session_id| in_main_thread(|| handle_controller_route(session_id)));
    let controller_routing_route = warp::get()
        .and(warp::path!("realearn" / String / "controller-routing"))
        .and_then(|session_id| in_main_thread(|| handle_controller_routing_route(session_id)));
    let patch_controller_route = warp::patch()
        .and(warp::path!("realearn" / String / "controller"))
        .and(warp::body::json())
        .and_then(|session_id, req: PatchRequest| {
            in_main_thread(|| handle_patch_controller_route(session_id, req))
        });
    let ws_route = {
        let clients = warp::any().map(move || clients.clone());
        warp::path::end()
            .and(warp::ws())
            .and(warp::query::<WebSocketRequest>())
            .and(clients)
            .map(
                |ws: warp::ws::Ws, req: WebSocketRequest, clients| -> Box<dyn Reply> {
                    let topics: Result<HashSet<_>, _> =
                        req.topics.split(',').map(Topic::try_from).collect();
                    if let Ok(topics) = topics {
                        Box::new(ws.on_upgrade(move |ws| client_connected(ws, topics, clients)))
                    } else {
                        Box::new(bad_request("at least one of the given topics is invalid"))
                    }
                },
            )
    };
    let routes = controller_route
        .or(controller_routing_route)
        .or(patch_controller_route)
        .or(ws_route);
    warp::serve(routes).run(([0, 0, 0, 0], port)).await;
}

#[derive(Deserialize)]
struct WebSocketRequest {
    topics: String,
}

#[derive(Deserialize)]
struct PatchRequest {
    op: PatchRequestOp,
    path: String,
    value: serde_json::value::Value,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum PatchRequestOp {
    Replace,
}

type Topics = HashSet<Topic>;

async fn client_connected(ws: WebSocket, topics: Topics, clients: ServerClients) {
    use futures::{FutureExt, StreamExt};
    use warp::Filter;
    let (ws_sender_sink, mut ws_receiver_stream) = ws.split();
    let (client_sender, client_receiver) = mpsc::unbounded_channel();
    // Keep forwarding received messages in client channel to websocket sender sink
    tokio::task::spawn(client_receiver.forward(ws_sender_sink).map(|result| {
        if let Err(e) = result {
            eprintln!("error sending websocket msg: {}", e);
        }
    }));
    let client_id = NEXT_CLIENT_ID.fetch_add(1, Ordering::Relaxed);
    let client = WebSocketClient {
        id: client_id,
        topics,
        sender: client_sender,
    };
    clients.write().unwrap().insert(client_id, client.clone());
    Reaper::get().do_later_in_main_thread_asap(move || {
        send_initial_data(&client);
    });
    // Keep receiving websocket receiver stream messages
    while let Some(result) = ws_receiver_stream.next().await {
        // We will need this as soon as we are interested in what the client says
        let msg = match result {
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

#[derive(Clone)]
pub struct WebSocketClient {
    id: usize,
    topics: Topics,
    sender: mpsc::UnboundedSender<std::result::Result<Message, warp::Error>>,
}

impl WebSocketClient {
    fn send(&self, text: &str) -> Result<(), &'static str> {
        self.sender
            .send(Ok(Message::text(text)))
            .map_err(|_| "couldn't send")
    }

    fn is_subscribed_to(&self, topic: &Topic) -> bool {
        self.topics.contains(topic)
    }
}

// We don't take the async RwLock by Tokio because we need to access this in sync code, too!
pub type ServerClients = Arc<std::sync::RwLock<HashMap<usize, WebSocketClient>>>;

pub fn keep_informing_clients(shared_session: &SharedSession) {
    let session = shared_session.borrow();
    when(
        session
            .on_mappings_changed()
            .merge(session.mapping_list_changed().map_to(()))
            .merge(session.mapping_changed().map_to(()))
            .merge(session.everything_changed()),
    )
    .with(Rc::downgrade(shared_session))
    .do_async(|session, _| {
        let _ = send_updated_controller_routing(&session.borrow());
    });
}

fn send_updated_controller_routing(session: &Session) -> Result<(), &'static str> {
    let clients = App::get().server().borrow().clients()?.clone();
    let clients = clients
        .read()
        .map_err(|_| "couldn't get read lock for client")?;
    if clients.is_empty() {
        return Ok(());
    }
    let json = get_controller_routing_as_json(session)?;
    for client in clients.values().filter(|c| {
        c.is_subscribed_to(&Topic::ControllerRouting {
            session_id: session.id().to_string(),
        })
    }) {
        let _ = client.send(&json);
    }
    Ok(())
}

fn send_initial_data(client: &WebSocketClient) {
    for topic in &client.topics {
        let _ = send_topic(client, topic).unwrap();
    }
}

fn send_topic(client: &WebSocketClient, topic: &Topic) -> Result<(), &'static str> {
    use Topic::*;
    match topic {
        ControllerRouting { session_id } => send_initial_controller_routing(client, session_id),
        ActiveController { .. } => todo!(),
    }
}

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
enum Topic {
    ActiveController { session_id: String },
    ControllerRouting { session_id: String },
}

impl TryFrom<&str> for Topic {
    type Error = &'static str;

    fn try_from(topic_expression: &str) -> Result<Self, Self::Error> {
        let topic_segments: Vec<_> = topic_expression.split('/').skip(1).collect();
        let topic = match topic_segments.as_slice() {
            ["realearn", "session", id, "controller-routing"] => Topic::ControllerRouting {
                session_id: id.to_string(),
            },
            ["realearn", "session", id, "controller"] => Topic::ActiveController {
                session_id: id.to_string(),
            },
            _ => return Err("invalid topic expression"),
        };
        Ok(topic)
    }
}

fn send_initial_controller_routing(
    client: &WebSocketClient,
    session_id: &str,
) -> Result<(), &'static str> {
    let session =
        session_manager::find_session_by_id(session_id).ok_or("couldn't find that session")?;
    let json = get_controller_routing_as_json(&session.borrow())?;
    client.send(&json)
}

fn get_controller_routing_as_json(session: &Session) -> Result<String, &'static str> {
    let routing = get_controller_routing(session);
    serde_json::to_string(&routing).map_err(|_| "couldn't serialize")
}

fn get_controller_routing(session: &Session) -> ControllerRouting {
    let routes = session
        .mappings(MappingCompartment::ControllerMappings)
        .filter_map(|m| {
            let m = m.borrow();
            let target_descriptor = if session.mapping_is_on(m.id()) {
                if m.target_model.category.get() == TargetCategory::Virtual {
                    let control_element = m.target_model.create_control_element();
                    let matching_primary_mappings: Vec<_> = session
                        .mappings(MappingCompartment::PrimaryMappings)
                        .filter(|mp| {
                            let mp = mp.borrow();
                            mp.source_model.category.get() == SourceCategory::Virtual
                                && mp.source_model.create_control_element() == control_element
                                && session.mapping_is_on(mp.id())
                        })
                        .collect();
                    if let Some(first_mapping) = matching_primary_mappings.first() {
                        let first_mapping = first_mapping.borrow();
                        let first_mapping_name = first_mapping.name.get_ref();
                        let label = if matching_primary_mappings.len() == 1 {
                            first_mapping_name.clone()
                        } else {
                            format!(
                                "{} +{}",
                                first_mapping_name,
                                matching_primary_mappings.len() - 1
                            )
                        };
                        TargetDescriptor { label }
                    } else {
                        return None;
                    }
                } else {
                    TargetDescriptor {
                        label: m.name.get_ref().clone(),
                    }
                }
            } else {
                return None;
            };
            Some((m.id().to_string(), target_descriptor))
        })
        .collect();
    ControllerRouting { routes }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ControllerRouting {
    routes: HashMap<String, TargetDescriptor>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TargetDescriptor {
    label: String,
}
