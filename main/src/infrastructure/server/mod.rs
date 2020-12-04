use crate::application::{
    ControllerManager, Session, SharedSession, SourceCategory, TargetCategory,
};
use crate::core::when;
use crate::domain::MappingCompartment;
use crate::infrastructure::data::ControllerData;
use crate::infrastructure::plugin::App;

use futures::StreamExt;
use rcgen::{BasicConstraints, CertificateParams, DistinguishedName, DnType, IsCa, SanType};
use reaper_high::Reaper;
use rx_util::UnitEvent;
use rxrust::prelude::*;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::convert::TryFrom;
use std::fmt::Debug;
use std::fs;

use std::net::{IpAddr, Ipv4Addr};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use tokio::sync::mpsc;
use url::Url;
use warp::http::{Method, Response, StatusCode};

use warp::reply::Json;
use warp::ws::{Message, WebSocket};
use warp::{reply, Rejection, Reply};

pub type SharedRealearnServer = Rc<RefCell<RealearnServer>>;

pub struct RealearnServer {
    http_port: u16,
    https_port: u16,
    state: ServerState,
    certs_dir_path: PathBuf,
    changed_subject: LocalSubject<'static, (), ()>,
    local_ip: Option<IpAddr>,
}

enum ServerState {
    Stopped,
    Starting { clients: ServerClients },
    Running { clients: ServerClients },
}

impl ServerState {
    pub fn is_starting_or_running(&self) -> bool {
        use ServerState::*;
        match self {
            Starting { .. } | Running { .. } => true,
            Stopped => false,
        }
    }
}

pub const COMPANION_WEB_APP_URL: &str = "https://realearn.helgoboss.org/";

impl RealearnServer {
    pub fn new(http_port: u16, https_port: u16, certs_dir_path: PathBuf) -> RealearnServer {
        RealearnServer {
            http_port,
            https_port,
            state: ServerState::Stopped,
            certs_dir_path,
            changed_subject: Default::default(),
            local_ip: get_local_ip(),
        }
    }

    /// Idempotent
    pub fn start(&mut self) -> Result<(), String> {
        if self.state.is_starting_or_running() {
            return Ok(());
        }
        check_port(false, self.http_port)?;
        check_port(true, self.https_port)?;
        let clients: ServerClients = Default::default();
        let clients_clone = clients.clone();
        let ip = self.effective_ip();
        let http_port = self.http_port;
        let https_port = self.https_port;
        let key_and_cert = self.key_and_cert();
        let _ = std::thread::Builder::new()
            .name("ReaLearn server".to_string())
            .spawn(move || {
                let mut runtime = tokio::runtime::Builder::new()
                    .basic_scheduler()
                    .enable_all()
                    .build()
                    .unwrap();
                runtime.block_on(start_server(
                    http_port,
                    https_port,
                    clients_clone,
                    ip,
                    key_and_cert,
                ));
            });
        self.state = ServerState::Starting { clients };
        self.notify_changed();
        Ok(())
    }

    fn effective_ip(&self) -> IpAddr {
        self.local_ip()
            .unwrap_or_else(|| IpAddr::V4(Ipv4Addr::LOCALHOST))
    }

    fn key_and_cert(&self) -> (String, String) {
        get_key_and_cert(self.effective_ip(), &self.certs_dir_path)
    }

    fn notify_started(&mut self) {
        // TODO-low Okay, temporarily replacing with Stopped just to gain ownership feels weird.
        if let ServerState::Starting { clients } =
            std::mem::replace(&mut self.state, ServerState::Stopped)
        {
            self.state = ServerState::Running { clients };
        }
        self.notify_changed();
    }

    fn notify_changed(&mut self) {
        self.changed_subject.next(());
    }

    pub fn clients(&self) -> Result<&ServerClients, &'static str> {
        if let ServerState::Running { clients, .. } = &self.state {
            Ok(clients)
        } else {
            Err("server not running")
        }
    }

    pub fn is_running(&self) -> bool {
        if let ServerState::Running { .. } = &self.state {
            true
        } else {
            false
        }
    }

    pub fn generate_full_companion_app_url(&self, session_id: &str, localhost: bool) -> String {
        let host = if localhost {
            None
        } else {
            self.local_ip().map(|ip| ip.to_string())
        };
        let (_, cert) = self.key_and_cert();
        let base64_encoded_cert = base64::encode_config(&cert, base64::URL_SAFE);
        Url::parse_with_params(
            App::get()
                .config()
                .companion_web_app_url()
                .join("controller-routing")
                .unwrap()
                .as_str(),
            &[
                ("host", host.unwrap_or_else(|| "localhost".to_string())),
                ("http-port", self.http_port().to_string()),
                ("https-port", self.https_port().to_string()),
                ("session-id", session_id.to_string()),
                // In order to indicate that the URL has not been entered manually and therefore
                // typos are out of question (for a proper error message if connection is not
                // possible).
                ("generated", "true".to_string()),
                ("cert", base64_encoded_cert),
            ],
        )
        .expect("invalid URL")
        .into_string()
    }

    pub fn local_ip(&self) -> Option<IpAddr> {
        self.local_ip
    }

    pub fn local_hostname(&self) -> Option<String> {
        let hn = hostname::get().ok()?;
        Some(hn.to_string_lossy().into())
    }

    pub fn local_hostname_dns(&self) -> Option<String> {
        let ip = self.local_ip()?;
        dns_lookup::lookup_addr(&ip).ok()
    }

    pub fn http_port(&self) -> u16 {
        self.http_port
    }

    pub fn https_port(&self) -> u16 {
        self.https_port
    }

    pub fn log_debug_info(&self, session_id: &str) {
        let msg = format!(
            "\n\
        # Server\n\
        \n\
        - ReaLearn app URL: {}\n\
        - ReaLearn local hostname: {:?}\n\
        - ReaLearn local hostname with DNS lookup: {:?}\n\
        - ReaLearn local IP address: {:?}\n\
        ",
            self.generate_full_companion_app_url(session_id, false),
            self.local_hostname(),
            self.local_hostname_dns(),
            self.local_ip()
        );
        Reaper::get().show_console_msg(msg);
    }

    pub fn changed(&self) -> impl UnitEvent {
        self.changed_subject.clone()
    }
}

static NEXT_CLIENT_ID: AtomicUsize = AtomicUsize::new(1);

async fn in_main_thread<O: Reply + 'static, E: Reply + 'static>(
    op: impl FnOnce() -> Result<O, E> + 'static + Send,
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
    let session = crate::application::App::get()
        .find_session_by_id(&session_id)
        .ok_or_else(session_not_found)?;
    let routing = get_controller_routing(&session.borrow());
    Ok(reply::json(&routing))
}

fn handle_patch_controller_route(
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
    let controller_manager = App::get().controller_manager();
    let mut controller_manager = controller_manager.borrow_mut();
    let mut controller = controller_manager
        .find_by_id(&controller_id)
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
    let session = crate::application::App::get()
        .find_session_by_id(&session_id)
        .ok_or_else(session_not_found)?;
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

fn handle_session_route(session_id: String) -> Result<Json, Response<&'static str>> {
    let _ = crate::application::App::get()
        .find_session_by_id(&session_id)
        .ok_or_else(session_not_found)?;
    Ok(reply::json(&SessionResponseData {}))
}

async fn start_server(
    http_port: u16,
    https_port: u16,
    clients: ServerClients,
    _ip: IpAddr,
    (key, cert): (String, String),
) {
    use warp::Filter;
    let welcome_route = warp::path::end()
        .and(warp::head().or(warp::get()))
        .map(|_| warp::reply::html(include_str!("welcome_page.html")));
    let session_route = warp::get()
        .and(warp::path!("realearn" / "session" / String))
        .and_then(|session_id| in_main_thread(|| handle_session_route(session_id)));
    let controller_route = warp::get()
        .and(warp::path!("realearn" / "session" / String / "controller"))
        .and_then(|session_id| in_main_thread(|| handle_controller_route(session_id)));
    let controller_routing_route = warp::get()
        .and(warp::path!(
            "realearn" / "session" / String / "controller-routing"
        ))
        .and_then(|session_id| in_main_thread(|| handle_controller_routing_route(session_id)));
    let patch_controller_route = warp::patch()
        .and(warp::path!("realearn" / "controller" / String))
        .and(warp::body::json())
        .and_then(|controller_id, req: PatchRequest| {
            in_main_thread(|| handle_patch_controller_route(controller_id, req))
        });
    let ws_route = {
        let clients = warp::any().map(move || clients.clone());
        warp::path("ws")
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
        .or(ws_route)
        .with(cors);
    let http_future = warp::serve(routes.clone()).bind(([0, 0, 0, 0], http_port));
    let https_future = warp::serve(routes)
        .tls()
        .key(key)
        .cert(cert)
        .bind(([0, 0, 0, 0], https_port));
    Reaper::get()
        .do_later_in_main_thread_asap(|| {
            App::get().server().borrow_mut().notify_started();
        })
        .unwrap();
    futures::future::join(http_future, https_future).await;
}

fn get_key_and_cert(ip: IpAddr, cert_dir_path: &Path) -> (String, String) {
    if let Some(tuple) = find_key_and_cert(ip, cert_dir_path) {
        return tuple;
    }
    // No key/cert yet for that host. Generate self-signed.
    let (key, cert) = add_key_and_cert(ip);
    fs::create_dir_all(cert_dir_path).expect("couldn't create certificate directory");
    let (key_file_path, cert_file_path) = get_key_and_cert_paths(ip, cert_dir_path);
    fs::write(key_file_path, &key).expect("couldn't save key");
    fs::write(cert_file_path, &cert).expect("couldn't save certificate");
    (key, cert)
}

fn add_key_and_cert(ip: IpAddr) -> (String, String) {
    let mut params = CertificateParams::default();
    params.subject_alt_names = vec![SanType::IpAddress(ip)];
    // This needs to be set to qualify as a root certificate, which is in turn important for being
    // able to accept it on iOS as described in
    // https://apple.stackexchange.com/questions/283348/how-do-i-trust-a-self-signed-certificate-in-ios-10-3
    // and https://medium.com/collaborne-engineering/self-signed-certificates-in-ios-apps-ff489bf8b96e
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    let mut dn = DistinguishedName::new();
    dn.push(
        DnType::CommonName,
        format!("ReaLearn on {}", ip.to_string()),
    );
    params.distinguished_name = dn;
    let certificate = rcgen::Certificate::from_params(params)
        .expect("couldn't create self-signed server certificate");
    (
        certificate.serialize_private_key_pem(),
        certificate
            .serialize_pem()
            .expect("couldn't serialize self-signed server certificate"),
    )
}

fn find_key_and_cert(ip: IpAddr, cert_dir_path: &Path) -> Option<(String, String)> {
    let (key_file_path, cert_file_path) = get_key_and_cert_paths(ip, cert_dir_path);
    Some((
        fs::read_to_string(&key_file_path).ok()?,
        fs::read_to_string(&cert_file_path).ok()?,
    ))
}

fn get_key_and_cert_paths(ip: IpAddr, cert_dir_path: &Path) -> (PathBuf, PathBuf) {
    let ip_string = ip.to_string();
    let key_file_path = cert_dir_path.join(format!("{}.key", ip_string));
    let cert_file_path = cert_dir_path.join(format!("{}.cer", ip_string));
    (key_file_path, cert_file_path)
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
    use futures::FutureExt;
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
    Reaper::get()
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

#[derive(Clone)]
pub struct WebSocketClient {
    id: usize,
    topics: Topics,
    sender: mpsc::UnboundedSender<std::result::Result<Message, warp::Error>>,
}

impl WebSocketClient {
    fn send(&self, msg: impl Serialize) -> Result<(), &'static str> {
        let json = serde_json::to_string(&msg).map_err(|_| "couldn't serialize")?;
        self.sender
            .send(Ok(Message::text(json)))
            .map_err(|_| "couldn't send")
    }

    fn is_subscribed_to(&self, topic: &Topic) -> bool {
        self.topics.contains(topic)
    }
}

// We don't take the async RwLock by Tokio because we need to access this in sync code, too!
pub type ServerClients = Arc<std::sync::RwLock<HashMap<usize, WebSocketClient>>>;

pub fn keep_informing_clients_about_sessions() {
    crate::application::App::get().changed().subscribe(|_| {
        Reaper::get()
            .do_later_in_main_thread_asap(|| {
                send_sessions_to_subscribed_clients();
            })
            .unwrap();
    });
}

fn send_sessions_to_subscribed_clients() {
    for_each_client(
        |client, _| {
            for t in client.topics.iter() {
                if let Topic::Session { session_id } = t {
                    let _ = send_initial_session(client, session_id);
                }
            }
        },
        || (),
    )
    .unwrap();
}

pub fn keep_informing_clients_about_session_events(shared_session: &SharedSession) {
    let session = shared_session.borrow();
    when(
        session
            .on_mappings_changed()
            .merge(session.mapping_list_changed().map_to(()))
            .merge(session.mapping_changed().map_to(())),
    )
    .with(Rc::downgrade(shared_session))
    .do_async(|session, _| {
        let _ = send_updated_controller_routing(&session.borrow());
    });
    when(App::get().controller_manager().borrow().changed())
        .with(Rc::downgrade(shared_session))
        .do_async(|session, _| {
            let _ = send_updated_active_controller(&session.borrow());
        });
    when(session.everything_changed().merge(session.id.changed()))
        .with(Rc::downgrade(shared_session))
        .do_async(|session, _| {
            send_sessions_to_subscribed_clients();
            let session = session.borrow();
            let _ = send_updated_active_controller(&session);
            let _ = send_updated_controller_routing(&session);
        });
}

fn send_initial_session(client: &WebSocketClient, session_id: &str) -> Result<(), &'static str> {
    let event = if crate::application::App::get()
        .find_session_by_id(session_id)
        .is_some()
    {
        get_session_updated_event(session_id, Some(SessionResponseData {}))
    } else {
        get_session_updated_event(session_id, None)
    };
    client.send(&event)
}

fn send_initial_controller_routing(
    client: &WebSocketClient,
    session_id: &str,
) -> Result<(), &'static str> {
    let event = if let Some(session) = crate::application::App::get().find_session_by_id(session_id)
    {
        get_controller_routing_updated_event(session_id, Some(&session.borrow()))
    } else {
        get_controller_routing_updated_event(session_id, None)
    };
    client.send(&event)
}

fn send_initial_controller(client: &WebSocketClient, session_id: &str) -> Result<(), &'static str> {
    let event = if let Some(session) = crate::application::App::get().find_session_by_id(session_id)
    {
        get_active_controller_updated_event(session_id, Some(&session.borrow()))
    } else {
        get_active_controller_updated_event(session_id, None)
    };
    client.send(&event)
}

fn send_updated_active_controller(session: &Session) -> Result<(), &'static str> {
    send_to_clients_subscribed_to(
        &Topic::ActiveController {
            session_id: session.id().to_string(),
        },
        || get_active_controller_updated_event(session.id(), Some(session)),
    )
}

fn send_updated_controller_routing(session: &Session) -> Result<(), &'static str> {
    send_to_clients_subscribed_to(
        &Topic::ControllerRouting {
            session_id: session.id().to_string(),
        },
        || get_controller_routing_updated_event(session.id(), Some(session)),
    )
}

fn send_to_clients_subscribed_to<T: Serialize>(
    topic: &Topic,
    create_message: impl FnOnce() -> T,
) -> Result<(), &'static str> {
    for_each_client(
        |client, cached| {
            if client.is_subscribed_to(topic) {
                let _ = client.send(cached);
            }
        },
        create_message,
    )
}

fn for_each_client<T: Serialize>(
    op: impl Fn(&WebSocketClient, &T),
    cache: impl FnOnce() -> T,
) -> Result<(), &'static str> {
    let server = App::get().server().borrow();
    if !server.is_running() {
        return Ok(());
    }
    let clients = server.clients()?.clone();
    let clients = clients
        .read()
        .map_err(|_| "couldn't get read lock for client")?;
    if clients.is_empty() {
        return Ok(());
    }
    let cached = cache();
    for client in clients.values() {
        op(client, &cached);
    }
    Ok(())
}

fn send_initial_events(client: &WebSocketClient) {
    for topic in &client.topics {
        let _ = send_initial_events_for_topic(client, topic);
    }
}

fn send_initial_events_for_topic(
    client: &WebSocketClient,
    topic: &Topic,
) -> Result<(), &'static str> {
    use Topic::*;
    match topic {
        Session { session_id } => send_initial_session(client, session_id),
        ControllerRouting { session_id } => send_initial_controller_routing(client, session_id),
        ActiveController { session_id } => send_initial_controller(client, session_id),
    }
}

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
enum Topic {
    Session { session_id: String },
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
            ["realearn", "session", id] => Topic::Session {
                session_id: id.to_string(),
            },
            _ => return Err("invalid topic expression"),
        };
        Ok(topic)
    }
}

fn get_active_controller_updated_event(
    session_id: &str,
    session: Option<&Session>,
) -> Event<Option<ControllerData>> {
    Event::new(
        format!("/realearn/session/{}/controller", session_id),
        session.and_then(get_controller),
    )
}

fn get_session_updated_event(
    session_id: &str,
    session_data: Option<SessionResponseData>,
) -> Event<Option<SessionResponseData>> {
    Event::new(format!("/realearn/session/{}", session_id), session_data)
}

fn get_controller_routing_updated_event(
    session_id: &str,
    session: Option<&Session>,
) -> Event<Option<ControllerRouting>> {
    Event::new(
        format!("/realearn/session/{}/controller-routing", session_id),
        session.map(get_controller_routing),
    )
}

fn get_controller(session: &Session) -> Option<ControllerData> {
    let controller = session.active_controller()?;
    Some(ControllerData::from_model(&controller))
}

fn get_controller_routing(session: &Session) -> ControllerRouting {
    let routes = session
        .mappings(MappingCompartment::ControllerMappings)
        .filter_map(|m| {
            let m = m.borrow();
            let target_descriptor = if session.mapping_is_on(m.id()) {
                if m.target_model.category.get() == TargetCategory::Virtual {
                    // Virtual
                    let control_element = m.target_model.create_control_element();
                    let matching_primary_mappings = session
                        .mappings(MappingCompartment::PrimaryMappings)
                        .filter(|mp| {
                            let mp = mp.borrow();
                            mp.source_model.category.get() == SourceCategory::Virtual
                                && mp.source_model.create_control_element() == control_element
                                && session.mapping_is_on(mp.id())
                        });
                    let descriptors: Vec<_> = matching_primary_mappings
                        .map(|m| {
                            let m = m.borrow();
                            TargetDescriptor {
                                label: m.name.get_ref().clone(),
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
                        label: m.name.get_ref().clone(),
                    };
                    vec![single_descriptor]
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
    routes: HashMap<String, Vec<TargetDescriptor>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
// Right now just a placeholder
struct SessionResponseData {}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TargetDescriptor {
    label: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Event<T> {
    /// Corresponds to the HTTP path of the resource.
    path: String,
    /// Corresponds to the HTTP body.
    ///
    /// HTTP 404 corresponds to this value being `null` or undefined in JSON. If this is not enough
    /// in future use cases, we can still add another field that resembles the HTTP status.
    body: T,
}

impl<T> Event<T> {
    pub fn new(path: String, body: T) -> Event<T> {
        Event { path, body }
    }
}

/// Inspired by local_ipaddress crate.
pub fn get_local_ip() -> Option<std::net::IpAddr> {
    let socket = match std::net::UdpSocket::bind("0.0.0.0:0") {
        Ok(s) => s,
        Err(_) => return None,
    };
    match socket.connect("8.8.8.8:80") {
        Ok(()) => (),
        Err(_) => return None,
    };
    match socket.local_addr() {
        Ok(addr) => Some(addr.ip()),
        Err(_) => None,
    }
}

fn check_port(is_https: bool, port: u16) -> Result<(), String> {
    if !local_port_available(port) {
        let msg = format!(
            r#"{upper_case_port_label} port {port} is not available. Possible causes and solutions:

(1) You are already running another instance of REAPER with ReaLearn.

If you don't use ReaLearn's projection feature, switch the ReaLearn server off by clicking the Projection button and following the instructions. If you use ReaLearn's projection feature, you should not use more than one REAPER instance at once. If you really need that, you can make it work using multiple portable REAPER installations.

(2) There's some kind of hickup.

Restart REAPER and ReaLearn.

(3) Another application grabbed the same port already (unlikely but possible).

Set another {upper_case_port_label} port in "realearn.ini", for example:
  
    server_{lower_case_port_label}_port = {alternate_port}
"#,
            lower_case_port_label = if is_https { "https" } else { "http" },
            upper_case_port_label = if is_https { "HTTPS" } else { "HTTP" },
            port = port,
            alternate_port = if is_https { 40443 } else { 40080 },
        );
        return Err(msg);
    }
    Ok(())
}

fn local_port_available(port: u16) -> bool {
    match std::net::TcpListener::bind(("0.0.0.0", port)) {
        Ok(_) => true,
        Err(_) => false,
    }
}
