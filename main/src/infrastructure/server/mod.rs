use crate::application::{session_manager, Session, SharedSession, SourceCategory, TargetCategory};
use crate::core::when;
use crate::domain::MappingCompartment;
use crate::infrastructure::plugin::App;
use futures::SinkExt;
use reaper_high::Reaper;
use rxrust::prelude::*;
use serde::Serialize;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::{JoinHandle, Thread};
use tokio::sync::{mpsc, RwLock};
use warp::ws::{Message, WebSocket};

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

async fn start_server(port: u16, clients: ServerClients) {
    use warp::Filter;
    // Turn our state into a new Filter.
    let clients = warp::any().map(move || clients.clone());
    let projection_websocket_route = warp::path!("realearn" / String / "projection")
        .and(warp::ws())
        .and(clients)
        .map(|realearn_session_id: String, ws: warp::ws::Ws, clients| {
            ws.on_upgrade(move |ws| client_connected(ws, realearn_session_id, clients))
        });
    warp::serve(projection_websocket_route)
        .run(([0, 0, 0, 0], port))
        .await;
}

async fn client_connected(ws: WebSocket, realearn_session_id: String, clients: ServerClients) {
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
    let client = ProjectionClient {
        id: client_id,
        realearn_session_id,
        sender: client_sender,
    };
    clients.write().unwrap().insert(client_id, client.clone());
    Reaper::get().do_later_in_main_thread_asap(move || {
        send_initial_controller_projection(&client);
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
pub struct ProjectionClient {
    pub id: usize,
    pub realearn_session_id: String,
    pub sender: mpsc::UnboundedSender<std::result::Result<Message, warp::Error>>,
}

impl ProjectionClient {
    pub fn send(&self, text: &str) -> Result<(), &'static str> {
        self.sender
            .send(Ok(Message::text(text)))
            .map_err(|_| "couldn't send")
    }
}

// We don't take the async RwLock by Tokio because we need to access this in sync code, too!
pub type ServerClients = Arc<std::sync::RwLock<HashMap<usize, ProjectionClient>>>;

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
        let _ = send_updated_controller_projection(&session.borrow());
    });
}

fn send_updated_controller_projection(session: &Session) -> Result<(), &'static str> {
    let clients = App::get().server().borrow().clients()?.clone();
    let clients = clients
        .read()
        .map_err(|_| "couldn't get read lock for client")?;
    if clients.is_empty() {
        return Ok(());
    }
    let json = get_controller_projection_as_json(session)?;
    for client in clients.values() {
        if client.realearn_session_id != session.id() {
            continue;
        }
        let _ = client.send(&json);
    }
    Ok(())
}

fn send_initial_controller_projection(client: &ProjectionClient) -> Result<(), &'static str> {
    let session = session_manager::find_session_by_id(&client.realearn_session_id)
        .ok_or("couldn't find that session")?;
    let json = get_controller_projection_as_json(&session.borrow())?;
    client.send(&json)
}

fn get_controller_projection_as_json(session: &Session) -> Result<String, &'static str> {
    let projection = get_controller_projection(session)?;
    serde_json::to_string(&projection).map_err(|_| "couldn't serialize")
}

fn get_controller_projection(session: &Session) -> Result<ControllerProjection, &'static str> {
    let mapping_projections = session
        .mappings(MappingCompartment::ControllerMappings)
        .map(|m| {
            let m = m.borrow();
            let target_projection = if session.mapping_is_on(m.id()) {
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
                        Some(TargetProjection { label })
                    } else {
                        None
                    }
                } else {
                    Some(TargetProjection {
                        label: m.name.get_ref().clone(),
                    })
                }
            } else {
                None
            };
            MappingProjection {
                id: m.id().to_string(),
                name: m.name.get_ref().clone(),
                target_projection,
            }
        })
        .collect();
    let controller_projection = ControllerProjection {
        mapping_projections,
    };
    Ok(controller_projection)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ControllerProjection {
    mapping_projections: Vec<MappingProjection>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MappingProjection {
    id: String,
    name: String,
    target_projection: Option<TargetProjection>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TargetProjection {
    label: String,
}
