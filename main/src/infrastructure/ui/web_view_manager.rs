// This is is to prevent Clippy from complaining about the activation_class assignment in the
// Askama template.
#![allow(clippy::useless_let_if_seq)]
use askama::Template;
use reaper_high::Reaper;
use slog::debug;

use crate::application::{SharedSession, WeakSession};
use crate::core::when;
use crate::infrastructure::plugin::App;

use image::Pixel;

use qrcode::QrCode;

use rx_util::UnitEvent;
use rxrust::prelude::*;
use std::cell::RefCell;

use std::rc::Rc;
use std::thread::JoinHandle;
use swell_ui::SharedView;
use web_view::{Content, WebView};

/// Responsible for managing the currently open web views.
#[derive(Debug)]
pub struct WebViewManager {
    session: WeakSession,
    web_view_connection: RefCell<Option<WebViewConnection>>,
    party_is_over_subject: LocalSubject<'static, (), ()>,
}

impl WebViewManager {
    pub fn new(session: WeakSession) -> SharedView<WebViewManager> {
        let m = WebViewManager {
            session,
            web_view_connection: Default::default(),
            party_is_over_subject: Default::default(),
        };
        let shared = Rc::new(m);
        shared.register_listeners();
        shared
    }

    pub fn show_projection_info(&self) {
        self.exit_web_view_blocking();
        let (sender, receiver): (
            crossbeam_channel::Sender<WebViewTask>,
            crossbeam_channel::Receiver<WebViewTask>,
        ) = crossbeam_channel::unbounded();
        let session_id = self.session().borrow().id().to_string();
        let sender_clone = sender.clone();
        let join_handle = std::thread::Builder::new()
            .name("ReaLearn web view".to_string())
            .spawn(move || run_web_view_blocking(sender_clone, receiver, session_id))
            .unwrap();
        let connection = WebViewConnection {
            sender,
            join_handle,
        };
        *self.web_view_connection.borrow_mut() = Some(connection);
        self.update_web_view();
    }

    fn session(&self) -> SharedSession {
        self.session.upgrade().expect("session gone")
    }

    fn update_web_view(&self) {
        if let Some(c) = self.web_view_connection.borrow().as_ref() {
            let session = self.session();
            let session = session.borrow();
            let app = App::get();
            let server = app.server().borrow();
            let full_companion_app_url =
                server.generate_full_companion_app_url(session.id(), false);
            let server_is_running = server.is_running();
            let (qr_code_image_url, (width, height)) =
                self.generate_qr_code_as_image_url(&full_companion_app_url);
            let session_id = session.id().to_string();
            let config = app.config();
            let state = ProjectionSetupState {
                include_skeleton: false,
                include_body_content: true,
                body_state: BodyState {
                    server_is_running,
                    server_is_enabled: config.server_is_enabled(),
                    qr_code_image_url,
                    qr_code_image_width: width,
                    qr_code_image_height: height,
                    companion_web_app_url: config.companion_web_app_url().to_string(),
                    server_host: server
                        .local_ip()
                        .map(|ip| ip.to_string())
                        .unwrap_or_else(|| "<could not be determined>".to_string()),
                    server_http_port: server.http_port(),
                    server_https_port: server.https_port(),
                    session_id,
                    os: std::env::consts::OS,
                },
            };
            c.send(move |wv| {
                wv.eval(&format!(
                    "document.body.innerHTML = {};",
                    web_view::escape(&state.render().unwrap())
                ))
                .unwrap();
            })
        }
    }

    fn generate_qr_code_as_image_url(&self, content: &str) -> (String, (u32, u32)) {
        let code = QrCode::new(content).unwrap();
        type P = image::LumaA<u8>;
        let image = code
            .render::<P>()
            // Background should be transparent
            .light_color(image::LumaA([255, 0]))
            .min_dimensions(270, 270)
            .max_dimensions(270, 270)
            .build();
        let mut buffer = Vec::new();
        let encoder = image::png::PNGEncoder::new(&mut buffer);
        encoder
            .encode(&image, image.width(), image.height(), P::COLOR_TYPE)
            .expect("couldn't encode QR code to PNG image");
        let url = format!(
            "data:image/png;base64,{}",
            base64::encode_config(&buffer, base64::URL_SAFE_NO_PAD)
        );
        (url, (image.width(), image.height()))
    }

    /// Closes and removes all mapping panels
    pub fn close_all(&mut self) {
        self.exit_web_view_blocking();
    }

    fn exit_web_view_blocking(&self) {
        if let Some(con) = self.web_view_connection.replace(None) {
            con.blocking_exit();
        }
    }

    fn register_listeners(self: &SharedView<Self>) {
        let app = App::get();
        when(
            app.changed()
                .merge(app.server().borrow().changed())
                .merge(self.session().borrow().id.changed())
                .take_until(self.party_is_over()),
        )
        .with(Rc::downgrade(self))
        .do_async(move |view, _| {
            view.update_web_view();
        });
    }

    fn party_is_over(&self) -> impl UnitEvent {
        self.party_is_over_subject.clone()
    }
}

impl Drop for WebViewManager {
    fn drop(&mut self) {
        debug!(Reaper::get().logger(), "Dropping mapping panel manager...");
        self.close_all();
        self.party_is_over_subject.next(());
    }
}

type WebViewState = ();

#[derive(Debug)]
struct WebViewConnection {
    sender: crossbeam_channel::Sender<WebViewTask>,
    join_handle: JoinHandle<()>,
}

type WebViewTask = Box<dyn FnOnce(&mut WebView<WebViewState>) + Send + 'static>;

impl WebViewConnection {
    pub fn send(&self, task: impl FnOnce(&mut WebView<WebViewState>) + Send + 'static) {
        let _ = self.sender.send(Box::new(task));
    }

    pub fn blocking_exit(self) {
        debug!(App::logger(), "Ask web view to exit");
        self.send(|wv| wv.exit());
        // TODO-low If we join here, this might block REAPER and ReaLearn exit due to
        //  https://github.com/Boscop/web-view/issues/241. So right now we are okay with
        //  orphan web view staying open until user clicks it. At least it's automatically
        //  closed when REAPER closes.
        // debug!(App::logger(), "Joining web view thread...");
        // self.join_handle
        //     .join()
        //     .expect("couldn't join with web view thread");
        // debug!(App::logger(), "Joined web view thread");
    }
}

#[derive(Template)]
#[template(path = "projection-setup.html")]
struct ProjectionSetupState {
    /// Includes html/body tags.
    include_skeleton: bool,
    /// Includes body content.
    include_body_content: bool,
    /// Includes the actual body content.
    body_state: BodyState,
}

#[derive(Default)]
struct BodyState {
    // Can change globally
    server_is_running: bool,
    // Can change globally
    server_is_enabled: bool,
    // Can change per session
    qr_code_image_url: String,
    qr_code_image_width: u32,
    qr_code_image_height: u32,
    // Can't change at all
    companion_web_app_url: String,
    // Can only change after restart
    server_host: String,
    // Can only change after restart
    server_http_port: u16,
    // Can only change after restart
    server_https_port: u16,
    // Can change per session
    session_id: String,
    // Can't change at all
    os: &'static str,
}

fn run_web_view_blocking(
    sender: crossbeam_channel::Sender<WebViewTask>,
    receiver: crossbeam_channel::Receiver<WebViewTask>,
    session_id: String,
) {
    let html_skeleton = ProjectionSetupState {
        include_skeleton: true,
        include_body_content: false,
        body_state: Default::default(),
    };
    let mut wv = web_view::builder()
        .title("ReaLearn")
        .content(Content::Html(html_skeleton.render().unwrap()))
        .size(1024, 768)
        .resizable(true)
        .user_data(())
        .invoke_handler(move |_wv, arg| {
            match arg {
                "add_firewall_rule" => {
                    let sender = sender.clone();
                    Reaper::get()
                        .do_later_in_main_thread_asap(move || {
                            let server = App::get().server().borrow();
                            let msg =
                                match add_firewall_rule(server.http_port(), server.https_port()) {
                                    Ok(_) => "Successfully added firewall rule.",
                                    Err(_) => {
                                        "Couldn't add firewall rule. Please try to do it manually!"
                                    }
                                };
                            sender
                                .send(Box::new(move |wv| {
                                    let _ = wv.eval(&format!("alert({});", web_view::escape(msg)));
                                }))
                                .unwrap();
                        })
                        .unwrap();
                }
                "open_companion_app" => {
                    let session_id = session_id.clone();
                    Reaper::get()
                        .do_later_in_main_thread_asap(move || {
                            let server = App::get().server().borrow();
                            let url = server.generate_full_companion_app_url(&session_id, true);
                            let _ = webbrowser::open(&url);
                        })
                        .unwrap();
                }
                "disable_server" => {
                    Reaper::get()
                        .do_later_in_main_thread_asap(move || {
                            App::get().disable_server_persistently();
                        })
                        .unwrap();
                }
                "enable_server" => {
                    Reaper::get()
                        .do_later_in_main_thread_asap(move || {
                            App::get().enable_server_persistently();
                        })
                        .unwrap();
                }
                "start_server" => {
                    Reaper::get()
                        .do_later_in_main_thread_asap(|| {
                            App::get().start_server_persistently();
                        })
                        .unwrap();
                }
                _ => return Err(web_view::Error::custom("unknown invocation argument")),
            };
            Ok(())
        })
        .build()
        .expect("couldn't build WebView");
    loop {
        for task in receiver.try_iter() {
            (task)(&mut wv);
        }
        // In web-view 0.6.3 this is blocking, which is not ideal because graceful exit doesn't
        // work if the web view window is in background
        // (https://github.com/Boscop/web-view/issues/210). In web-view 0.7.1 it's not blocking but
        // the CPU consumption is very high (https://github.com/Boscop/web-view/issues/241). For
        // now we stick to 0.6.3 and live with a web view that doesn't disappear before brought to
        // foreground.
        // TODO-low-wait
        match wv.step() {
            // WebView closed
            None => break,
            // WebView still running or error
            Some(res) => res.expect("error in projection web view"),
        }
    }
}

#[cfg(target_os = "windows")]
fn add_firewall_rule(http_port: u16, https_port: u16) -> Result<(), &'static str> {
    fn add(http_port: u16, https_port: u16, direction: &str) -> Result<(), &'static str> {
        let exit_status = runas::Command::new("netsh")
            .args(&[
                "advfirewall",
                "firewall",
                "add",
                "rule",
                "name=ReaLearn Server",
                "action=allow",
                "protocol=TCP",
            ])
            .arg(format!("dir={}", direction))
            .arg(format!("localport={},{}", http_port, https_port))
            .gui(true)
            .show(false)
            .status()
            .map_err(|_| "couldn't execute command")?;
        if !exit_status.success() {
            return Err("command returned failure exit code");
        }
        Ok(())
    }
    add(http_port, https_port, "in")?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn add_firewall_rule(_http_port: u16, _https_port: u16) -> Result<(), &'static str> {
    todo!("not implemented yet for macOS")
}

#[cfg(target_os = "linux")]
fn add_firewall_rule(_http_port: u16, _https_port: u16) -> Result<(), &'static str> {
    unimplemented!("This shouldn't be called!")
}
