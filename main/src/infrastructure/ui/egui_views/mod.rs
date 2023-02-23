use egui::{Context, Visuals};
use reaper_low::firewall;
use swell_ui::Window;

pub mod advanced_script_editor;
pub mod learnable_target_kinds_picker;

pub fn open<S: Send + 'static>(
    window: Window,
    title: impl Into<String>,
    state: S,
    run_ui: impl Fn(&egui::Context, &mut S) + Send + Sync + 'static,
) {
    let title = title.into();
    window.set_text(title.as_str());
    let window_size = window.size();
    let dpi_factor = window.dpi_scaling_factor();
    let window_width = window_size.width.get() as f64 / dpi_factor;
    let window_height = window_size.height.get() as f64 / dpi_factor;
    let settings = baseview::WindowOpenOptions {
        title,
        size: baseview::Size::new(window_width, window_height),
        scale: baseview::WindowScalePolicy::SystemScaleFactor,
        gl_config: Some(Default::default()),
    };
    egui_baseview::EguiWindow::open_parented(
        &window,
        settings,
        state,
        |ctx: &egui::Context, _queue: &mut egui_baseview::Queue, _state: &mut S| {
            firewall(|| {
                init_ui(ctx, Window::dark_mode_is_enabled());
            });
        },
        move |ctx: &egui::Context, _queue: &mut egui_baseview::Queue, state: &mut S| {
            firewall(|| {
                run_ui(ctx, state);
            });
        },
    );
}

fn init_ui(ctx: &Context, dark_mode_is_enabled: bool) {
    let mut style: egui::Style = (*ctx.style()).clone();
    style.visuals = if dark_mode_is_enabled {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    ctx.set_style(style);
}
