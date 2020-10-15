use crate::infrastructure::ui::{MainPanel, MappingPanel};
use askama::Template;
use reaper_high::Reaper;
use slog::debug;

use crate::application::{SharedMapping, SharedSession, WeakSession};
use crate::core::when;
use crate::infrastructure::plugin::App;
use crate::infrastructure::server::COMPANION_APP_URL;
use image::Pixel;
use once_cell::unsync::Lazy;
use qrcode::QrCode;
use rx_util::UnitEvent;
use rxrust::prelude::*;
use std::cell::RefCell;
use std::io;
use std::rc::Rc;
use std::thread::JoinHandle;
use swell_ui::{SharedView, View, WeakView, Window};
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
        let join_handle = std::thread::spawn(move || run_web_view_blocking(receiver));
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
            let server = App::get().server().borrow();
            let full_companion_app_url = server.generate_full_companion_app_url(session.id());
            let server_is_running = server.is_running();
            let black_alpha = if server_is_running { 254 } else { 15 };
            let qr_code_image_url =
                self.generate_qr_code_as_image_url(&full_companion_app_url, black_alpha);
            let session_id = session.id().to_string();
            let state = ProjectionSetupState {
                include_skeleton: false,
                include_body_content: true,
                body_state: BodyState {
                    server_is_running,
                    // TODO-high Set correctly
                    server_is_enabled: false,
                    full_companion_app_url,
                    qr_code_image_url,
                    companion_app_url: COMPANION_APP_URL,
                    server_host: server
                        .local_ip()
                        .map(|ip| ip.to_string())
                        .unwrap_or("<could not be determined>".to_string()),
                    server_port: server.port(),
                    session_id,
                    os: std::env::consts::OS,
                },
            };
            c.send(move |wv| {
                wv.eval(&format!(
                    "document.body.innerHTML = {};",
                    web_view::escape(&state.render().unwrap())
                ));
            })
        }
    }

    fn generate_qr_code_as_image_url(&self, content: &str, black_alpha: u8) -> String {
        let code = QrCode::new(content).unwrap();
        type P = image::LumaA<u8>;
        let image = code
            .render::<P>()
            .dark_color(image::LumaA([0, black_alpha]))
            .light_color(image::LumaA([255, 0]))
            .module_dimensions(4, 4)
            .build();
        let mut buffer = Vec::new();
        let encoder = image::png::PNGEncoder::new(&mut buffer);
        encoder
            .encode(&image, image.width(), image.height(), P::COLOR_TYPE)
            .expect("couldn't encode QR code to PNG image");
        format!(
            "data:image/png;base64,{}",
            base64::encode_config(&buffer, base64::URL_SAFE)
        )
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
        when(
            // TODO-high There are many more reasons to update a web view
            App::get()
                .server()
                .borrow()
                .changed()
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

    pub fn blocking_exit(mut self) {
        self.send(|wv| wv.exit());
        self.join_handle
            .join()
            .expect("couldn't join with web view thread");
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
    full_companion_app_url: String,
    // Can change per session
    qr_code_image_url: String,
    // Can't change at all
    companion_app_url: &'static str,
    // Can only change after restart
    server_host: String,
    // Can only change after restart
    server_port: u16,
    // Can change per session
    session_id: String,
    // Can't change at all
    os: &'static str,
}

fn run_web_view_blocking(receiver: crossbeam_channel::Receiver<WebViewTask>) {
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
        .invoke_handler(move |wv, arg| {
            match arg {
                "enable_server" => {
                    // TODO-high
                }
                "start_server" => {
                    // TODO-high reaper-rs: We should require Send!!! For the special case
                    //  if we know that we are in main thread already, we should introduce
                    //  a method do_later_in_same_thread() which doesn't need Send. But it
                    //  should complain if not in main thread.
                    Reaper::get().do_later_in_main_thread_asap(move || {
                        App::get().server().borrow_mut().start();
                    });
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
        match wv.step() {
            // WebView closed
            None => break,
            // WebView still running or error
            Some(res) => res.expect("error in projection web view"),
        }
    }
}
