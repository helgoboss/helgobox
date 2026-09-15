use crate::domain::InstanceId;
use crate::infrastructure::plugin::BackboneShell;
use crate::infrastructure::proto::Reply;
use crate::infrastructure::ui::{AppCallback, AppInstance, AppPage, InstanceRef};
use anyhow::{Context, bail};
use reaper_medium::Hwnd;
use std::cell::Cell;
use std::process::Child;
use std::thread;
use std::time::{Duration, Instant};
use swell_ui::Window;
use tokio::task::JoinHandle;
use tracing::info;

#[derive(Debug)]
pub struct SeparateProcessAppInstance {
    instance_id: InstanceId,
    running_state: Option<SeparateProcessAppRunningState>,
}

#[derive(Debug)]
struct SeparateProcessAppRunningState {
    process: std::process::Child,
    is_visible: bool,
    has_focus: bool,
}
impl Drop for SeparateProcessAppInstance {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

impl SeparateProcessAppInstance {
    pub fn new(instance_id: InstanceId) -> Self {
        Self {
            instance_id,
            running_state: None,
        }
    }
}

impl AppInstance for SeparateProcessAppInstance {
    fn is_running(&self) -> bool {
        self.running_state.is_some()
    }

    fn has_focus(&self) -> bool {
        self.running_state.as_ref().is_some_and(|s| s.has_focus)
    }

    fn is_visible(&self) -> bool {
        self.running_state.as_ref().is_some_and(|s| s.is_visible)
    }

    fn start_or_show(
        &mut self,
        owning_window: Window,
        location: Option<AppPage>,
    ) -> anyhow::Result<()> {
        // App already running. Just need to show the window.
        if let Some(s) = &mut self.running_state {
            BackboneShell::get()
                .proto_hub()
                .request_show(self.instance_id);
            s.is_visible = true;
            return Ok(());
        }
        // App not running yet.
        let program = if cfg!(target_os = "windows") {
            "helgobox.exe"
        } else if cfg!(target_os = "macos") {
            "Contents/MacOS/helgobox"
        } else if cfg!(target_os = "linux") {
            "helgobox"
        } else {
            bail!("OS not supported");
        };
        // Build command
        let app_base_dir = BackboneShell::app_binary_base_dir_path();
        let server_grpc_port = BackboneShell::get().config().server_grpc_port();
        let mut command = std::process::Command::new(app_base_dir.join(program));
        command
            .arg("--connection")
            .arg(format!("grpc://localhost:{server_grpc_port}"))
            .arg("--mode")
            .arg("guest");
        #[cfg(target_os = "linux")]
        {
            if let Some(xid) = owning_window.x11_window_id() {
                command
                    .env("GDK_BACKEND", "x11")
                    .arg("--host-window-handle")
                    .arg(format!("0x{xid:x}"));
            }
        }
        let initial_location = location.unwrap_or(AppPage::Projection(0.into()));
        command
            .arg("--location")
            .arg(initial_location.location(InstanceRef::Id(self.instance_id)));
        // Invoke command
        info!("Invoking {command:?}");
        let process = command.spawn()?;
        let running_state = SeparateProcessAppRunningState {
            process,
            is_visible: true,
            has_focus: true,
        };
        self.running_state = Some(running_state);
        Ok(())
    }

    fn hide(&mut self) -> anyhow::Result<()> {
        if let Some(s) = &mut self.running_state {
            BackboneShell::get()
                .proto_hub()
                .request_hide(self.instance_id);
            s.is_visible = false;
        }
        Ok(())
    }

    fn stop(&mut self) -> anyhow::Result<()> {
        let Some(mut running_state) = self.running_state.take() else {
            return Ok(());
        };
        BackboneShell::get()
            .proto_hub()
            .request_quit(self.instance_id);
        wait_or_kill(&mut running_state.process, Duration::from_millis(500))?;
        Ok(())
    }

    fn send(&self, reply: &Reply) -> anyhow::Result<()> {
        // Not relevant here. A separate-process instance subscribes itself to all events
        // via normal gRPC.
        bail!("should not be used in separate-process app instance")
    }

    fn notify_app_is_ready(&mut self, callback: AppCallback) {
        // Not relevant here
        let _ = callback;
    }

    fn window(&self) -> Option<Hwnd> {
        None
    }

    fn notify_app_has_focus(&mut self, value: bool) {
        info!(msg = "notify_app_has_focus", value, ?self.instance_id);
        if let Some(s) = &mut self.running_state {
            s.has_focus = value;
        }
    }
}

fn wait_or_kill(child: &mut Child, timeout: Duration) -> std::io::Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        if child.try_wait()?.is_some() {
            // Process exited cleanly and has been reaped.
            return Ok(());
        }
        if Instant::now() >= deadline {
            child.kill()?;
            child.wait()?;
            return Ok(());
        }
        thread::sleep(Duration::from_millis(10));
    }
}
