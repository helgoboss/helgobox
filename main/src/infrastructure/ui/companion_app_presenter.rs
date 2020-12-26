// This is is to prevent Clippy from complaining about the activation_class assignment in the
// Askama template.
#![allow(clippy::useless_let_if_seq)]
use askama::Template;
use reaper_high::Reaper;
use slog::debug;

use crate::application::{SharedSession, WeakSession};
use crate::core::when;
use crate::infrastructure::plugin::App;

use qrcode::QrCode;

use rx_util::UnitEvent;
use rxrust::prelude::*;

use once_cell::unsync::Lazy;
use std::cell::Cell;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use tempfile::TempDir;

#[derive(Debug)]
pub struct CompanionAppPresenter {
    session: WeakSession,
    app_setup_temp_dir: Lazy<Option<TempDir>>,
    party_is_over_subject: LocalSubject<'static, (), ()>,
    /// `true` as soon as requested within this ReaLearn session execution at least once
    app_info_requested: Cell<bool>,
}

impl CompanionAppPresenter {
    pub fn new(session: WeakSession) -> Rc<CompanionAppPresenter> {
        let m = CompanionAppPresenter {
            session,
            app_setup_temp_dir: Lazy::new(|| create_app_setup_temp_dir().ok()),
            party_is_over_subject: Default::default(),
            app_info_requested: Cell::new(false),
        };
        let shared = Rc::new(m);
        shared.register_listeners();
        shared
    }

    pub fn show_app_info(&self) {
        self.app_info_requested.set(true);
        let index_file = self.update_app_info();
        webbrowser::open(&index_file.to_string_lossy())
            .expect("couldn't open app setup page in browser");
    }

    fn update_app_info(&self) -> PathBuf {
        let dir = self
            .app_setup_temp_dir
            .as_ref()
            .expect("app setup temp dir not lazily created");
        let session = self.session();
        let session = session.borrow();
        let app = App::get();
        let server = app.server().borrow();
        let full_companion_app_url = server.generate_full_companion_app_url(session.id(), false);
        let server_is_running = server.is_running();
        let qr_code_image_file_name = "qr-code.png";
        let (width, height) = self
            .generate_qr_code(
                &full_companion_app_url,
                &dir.path().join(qr_code_image_file_name),
            )
            .expect("couldn't generate QR code image file");
        let config = app.config();
        let state = AppSetupState {
            server_is_running,
            server_is_enabled: config.server_is_enabled(),
            qr_code_image_uri: qr_code_image_file_name.to_string(),
            qr_code_image_width: width,
            qr_code_image_height: height,
            companion_web_app_url: config.companion_web_app_url().to_string(),
            full_companion_web_app_url: server.generate_full_companion_app_url(session.id(), true),
            server_host: server
                .local_ip()
                .map(|ip| ip.to_string())
                .unwrap_or_else(|| "<could not be determined>".to_string()),
            server_http_port: server.http_port(),
            server_https_port: server.https_port(),
            session_id: session.id().to_string(),
            os: std::env::consts::OS,
        };
        let index_file = dir.path().join("index.html");
        std::fs::write(
            &index_file,
            state
                .render()
                .expect("couldn't render app setup page template"),
        )
        .expect("couldn't write app setup page to file");
        index_file
    }

    fn session(&self) -> SharedSession {
        self.session.upgrade().expect("session gone")
    }

    fn generate_qr_code(
        &self,
        content: &str,
        target_file: &Path,
    ) -> Result<(u32, u32), Box<dyn std::error::Error>> {
        let code = QrCode::new(content)?;
        type P = image::LumaA<u8>;
        let min_size = 250;
        let max_size = 500;
        let image = code
            .render::<P>()
            // Background should be transparent
            .light_color(image::LumaA([255, 0]))
            .min_dimensions(min_size, min_size)
            .max_dimensions(max_size, max_size)
            .build();
        image.save(target_file)?;
        Ok((image.width(), image.height()))
    }

    fn register_listeners(self: &Rc<Self>) {
        let app = App::get();
        when(
            app.changed()
                .merge(app.server().borrow().changed())
                .merge(self.session().borrow().id.changed())
                .take_until(self.party_is_over()),
        )
        .with(Rc::downgrade(self))
        .do_async(move |view, _| {
            if view.app_info_requested.get() {
                view.update_app_info();
            }
        });
    }

    fn party_is_over(&self) -> impl UnitEvent {
        self.party_is_over_subject.clone()
    }
}

impl Drop for CompanionAppPresenter {
    fn drop(&mut self) {
        debug!(Reaper::get().logger(), "Dropping mapping panel manager...");
        self.party_is_over_subject.next(());
    }
}

#[derive(Template)]
#[template(path = "companion-app-setup.html")]
struct AppSetupState {
    // Can change globally
    server_is_running: bool,
    // Can change globally
    server_is_enabled: bool,
    // Can change per session
    qr_code_image_uri: String,
    qr_code_image_width: u32,
    qr_code_image_height: u32,
    // Can't change at all
    companion_web_app_url: String,
    // Can change per session
    full_companion_web_app_url: String,
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

#[cfg(target_os = "windows")]
pub fn add_firewall_rule(http_port: u16, https_port: u16) -> Result<(), &'static str> {
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
            return Err("command returned non-success exit code");
        }
        Ok(())
    }
    add(http_port, https_port, "in")?;
    Ok(())
}

#[cfg(target_os = "macos")]
pub fn add_firewall_rule(_http_port: u16, _https_port: u16) -> Result<(), &'static str> {
    Err("not supported on macOS")
}

#[cfg(target_os = "linux")]
pub fn add_firewall_rule(_http_port: u16, _https_port: u16) -> Result<(), &'static str> {
    Err("not supported on Linux")
}

fn create_app_setup_temp_dir() -> io::Result<TempDir> {
    let dir = tempfile::Builder::new().prefix("realearn-").tempdir()?;
    Ok(dir)
}
