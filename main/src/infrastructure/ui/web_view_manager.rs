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
        let join_handle = std::thread::Builder::new()
            .name("ReaLearn web view".to_string())
            .spawn(move || run_web_view_blocking(receiver))
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
            let server = App::get().server().borrow();
            let full_companion_app_url = server.generate_full_companion_app_url(session.id());
            let server_is_running = server.is_running();
            let (qr_code_image_url, qr_code_dimensions) =
                self.generate_qr_code_as_image_url(&full_companion_app_url);
            let session_id = session.id().to_string();
            let state = ProjectionSetupState {
                include_skeleton: false,
                include_body_content: true,
                body_state: BodyState {
                    server_is_running,
                    server_is_enabled: App::get().config().server_is_enabled(),
                    full_companion_app_url,
                    qr_code_image_url,
                    qr_code_dimensions,
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
            // TODO-high There are more reasons to update a web view
            app.changed()
                .merge(app.server().borrow().changed())
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
    // Can change per session
    qr_code_dimensions: (u32, u32),
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
                "disable_server" => {
                    Reaper::get().do_later_in_main_thread_asap(move || {
                        App::get().disable_server_persistently();
                    });
                }
                "enable_server" => {
                    Reaper::get().do_later_in_main_thread_asap(move || {
                        App::get().enable_server_persistently();
                    });
                }
                "start_server" => {
                    // TODO-high reaper-rs: We should require Send!!! For the special case
                    //  if we know that we are in main thread already, we should introduce
                    //  a method do_later_in_same_thread() which doesn't need Send. But it
                    //  should complain if not in main thread.
                    Reaper::get().do_later_in_main_thread_asap(|| {
                        App::get().start_server_persistently();
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
