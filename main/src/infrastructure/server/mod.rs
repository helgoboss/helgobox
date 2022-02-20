//! Contains the ReaLearn server interface and runtime.

use crate::infrastructure::plugin::{App, RealearnControlSurfaceServerTaskSender};

use rcgen::{BasicConstraints, CertificateParams, DistinguishedName, DnType, IsCa, SanType};
use reaper_high::Reaper;
use rxrust::prelude::*;
use std::cell::RefCell;
use std::fmt::Debug;
use std::fs;

use std::net::{IpAddr, Ipv4Addr};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use tokio::sync::broadcast;
use url::Url;

use crate::infrastructure::server::http::start_http_server;
use crate::infrastructure::server::http::ServerClients;
use std::thread::JoinHandle;
use std::time::Duration;

pub type SharedRealearnServer = Rc<RefCell<RealearnServer>>;

pub mod grpc;
pub mod http;
mod layers;

#[derive(Debug)]
pub struct RealearnServer {
    http_port: u16,
    https_port: u16,
    state: ServerState,
    certs_dir_path: PathBuf,
    changed_subject: LocalSubject<'static, (), ()>,
    local_ip: Option<IpAddr>,
    control_surface_task_sender: RealearnControlSurfaceServerTaskSender,
    control_surface_metrics_enabled: bool,
}

#[derive(Debug)]
enum ServerState {
    Stopped,
    Starting(ServerRuntimeData),
    Running(ServerRuntimeData),
}

#[derive(Debug)]
struct ServerRuntimeData {
    clients: ServerClients,
    shutdown_sender: broadcast::Sender<()>,
    server_thread_join_handle: JoinHandle<()>,
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
    pub fn new(
        http_port: u16,
        https_port: u16,
        certs_dir_path: PathBuf,
        control_surface_task_sender: RealearnControlSurfaceServerTaskSender,
        control_surface_metrics_enabled: bool,
    ) -> RealearnServer {
        RealearnServer {
            http_port,
            https_port,
            state: ServerState::Stopped,
            certs_dir_path,
            changed_subject: Default::default(),
            local_ip: get_local_ip(),
            control_surface_task_sender,
            control_surface_metrics_enabled,
        }
    }

    /// Idempotent
    pub fn start(&mut self) -> Result<(), String> {
        if self.state.is_starting_or_running() {
            return Ok(());
        }
        check_port(false, self.http_port)?;
        check_port(true, self.https_port)?;
        // TODO-high-grpc Notify user if gRPC port is in use, too. Also make it configurable.
        let clients: ServerClients = Default::default();
        let clients_clone = clients.clone();
        let http_port = self.http_port;
        let https_port = self.https_port;
        let key_and_cert = self.key_and_cert();
        let control_surface_task_sender = self.control_surface_task_sender.clone();
        let (shutdown_sender, http_shutdown_receiver) = broadcast::channel(5);
        let https_shutdown_receiver = shutdown_sender.subscribe();
        let grpc_shutdown_receiver = shutdown_sender.subscribe();
        let control_surface_metrics_enabled = self.control_surface_metrics_enabled;
        let server_thread_join_handle = std::thread::Builder::new()
            .name("ReaLearn server".to_string())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();
                runtime.block_on(start_servers(
                    http_port,
                    https_port,
                    clients_clone,
                    key_and_cert,
                    control_surface_task_sender,
                    http_shutdown_receiver,
                    https_shutdown_receiver,
                    grpc_shutdown_receiver,
                    control_surface_metrics_enabled,
                ));
                runtime.shutdown_timeout(Duration::from_secs(1));
            })
            .map_err(|_| "couldn't start server thread".to_string())?;
        let runtime_data = ServerRuntimeData {
            clients,
            shutdown_sender,
            server_thread_join_handle,
        };
        self.state = ServerState::Starting(runtime_data);
        self.notify_changed();
        Ok(())
    }

    fn effective_ip(&self) -> IpAddr {
        self.local_ip().unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST))
    }

    fn key_and_cert(&self) -> (String, String) {
        get_key_and_cert(self.effective_ip(), &self.certs_dir_path)
    }

    fn notify_started(&mut self) {
        // TODO-low Okay, temporarily replacing with Stopped just to gain ownership feels weird.
        if let ServerState::Starting(runtime_data) =
            std::mem::replace(&mut self.state, ServerState::Stopped)
        {
            self.state = ServerState::Running(runtime_data);
        }
        self.notify_changed();
    }

    /// Idempotent.
    pub fn stop(&mut self) {
        let old_state = std::mem::replace(&mut self.state, ServerState::Stopped);
        let runtime_data = match old_state {
            ServerState::Running(runtime_data) | ServerState::Starting(runtime_data) => {
                runtime_data
            }
            ServerState::Stopped => return,
        };
        let _ = runtime_data.shutdown_sender.send(());
        runtime_data
            .server_thread_join_handle
            .join()
            .expect("couldn't wait for server thread to finish");
    }

    fn notify_changed(&mut self) {
        self.changed_subject.next(());
    }

    pub fn clients(&self) -> Result<&ServerClients, &'static str> {
        if let ServerState::Running(runtime_data) = &self.state {
            Ok(&runtime_data.clients)
        } else {
            Err("server not running")
        }
    }

    pub fn is_running(&self) -> bool {
        matches!(&self.state, ServerState::Running { .. })
    }

    pub fn generate_full_companion_app_url(&self, session_id: &str, localhost: bool) -> String {
        let host = if localhost {
            None
        } else {
            self.local_ip().map(|ip| ip.to_string())
        };
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
            ],
        )
        .expect("invalid URL")
        .into()
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

    pub fn changed(&self) -> impl LocalObservable<'static, Item = (), Err = ()> + 'static {
        self.changed_subject.clone()
    }
}

#[allow(clippy::too_many_arguments)]
async fn start_servers(
    http_port: u16,
    https_port: u16,
    clients: ServerClients,
    (key, cert): (String, String),
    control_surface_task_sender: RealearnControlSurfaceServerTaskSender,
    http_shutdown_receiver: broadcast::Receiver<()>,
    https_shutdown_receiver: broadcast::Receiver<()>,
    _grpc_shutdown_receiver: broadcast::Receiver<()>,
    control_surface_metrics_enabled: bool,
) {
    let http_server_future = start_http_server(
        http_port,
        https_port,
        clients,
        (key, cert),
        control_surface_task_sender,
        http_shutdown_receiver,
        https_shutdown_receiver,
        control_surface_metrics_enabled,
    );
    http_server_future.await.expect("HTTP server error");
    // let grpc_server_future = start_grpc_server(
    //     SocketAddr::from(([127, 0, 0, 1], 50051)),
    //     grpc_shutdown_receiver,
    // );
    // let (http_result, grpc_result) =
    //     futures::future::join(http_server_future, grpc_server_future).await;
    // http_result.expect("HTTP server error");
    // grpc_result.expect("gRPC server error");
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

#[allow(clippy::field_reassign_with_default)]
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
    std::net::TcpListener::bind(("0.0.0.0", port)).is_ok()
}
