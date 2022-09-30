use baseview::{Event, EventStatus, Window, WindowHandler, WindowScalePolicy};
use egui::{pos2, vec2, Pos2, Rect};
pub use egui_glow::glow;
use raw_window_handle::HasRawWindowHandle;
use std::sync::Arc;
use std::time::Instant;

/// Use [`egui`] from a [`glow`] app based on [`winit`].
//
// Analog to egui_glow::EguiGlow
pub struct RealearnEgui<S, U> {
    egui_ctx: egui::Context,
    view_state: BaseviewState,
    shapes: Vec<egui::epaint::ClippedShape>,
    textures_delta: egui::TexturesDelta,
    painter: egui_glow::Painter,
    user_state: S,
    user_update: U,
    scale_policy: WindowScalePolicy,
}

impl<S, U> Drop for RealearnEgui<S, U> {
    fn drop(&mut self) {
        self.painter.destroy();
    }
}

struct OpenSettings {
    scale_policy: WindowScalePolicy,
    logical_width: f64,
    logical_height: f64,
}

// Analog to egui_winit::State
struct BaseviewState {
    raw_input: egui::RawInput,
    start_time: Instant,
    dimensions: [u32; 2],
    mouse_pos: Option<Pos2>,
    scale_factor: f32,
}

impl BaseviewState {
    // Source: https://github.com/emilk/egui/blob/master/crates/egui-winit/src/lib.rs#L155
    pub fn take_egui_input(&mut self) -> egui::RawInput {
        self.raw_input.time = Some(self.start_time.elapsed().as_secs_f64());
        self.raw_input.take()
    }
}

pub struct RealearnEguiRunArgs<P, S, U> {
    pub parent_window: P,
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub state: S,
    pub update: U,
}

impl<S, U> RealearnEgui<S, U>
where
    S: 'static + Send,
    U: FnMut(&egui::Context, &mut S) + Send + 'static,
{
    pub fn run<P>(args: RealearnEguiRunArgs<P, S, U>)
    where
        P: HasRawWindowHandle,
    {
        let options = baseview::WindowOpenOptions {
            title: args.title,
            size: baseview::Size::new(args.width as _, args.height as _),
            scale: baseview::WindowScalePolicy::SystemScaleFactor,
            gl_config: Some(baseview::gl::GlConfig::default()),
        };
        let open_settings = OpenSettings {
            scale_policy: options.scale,
            logical_width: options.size.width,
            logical_height: options.size.height,
        };
        baseview::Window::open_parented(
            &args.parent_window,
            options,
            move |window: &mut baseview::Window| -> RealearnEgui<S, U> {
                let gl_context = window.gl_context().unwrap();
                unsafe {
                    gl_context.make_current();
                }
                let context = unsafe {
                    glow::Context::from_loader_function(|s| gl_context.get_proc_address(s))
                };
                let res =
                    RealearnEgui::new(Arc::new(context), open_settings, args.state, args.update);
                unsafe {
                    gl_context.make_not_current();
                }
                res
            },
        );
    }

    // Source: https://github.com/emilk/egui/blob/master/crates/egui_glow/src/winit.rs#L18
    fn new(
        gl: std::sync::Arc<glow::Context>,
        open_settings: OpenSettings,
        user_state: S,
        user_update: U,
    ) -> Self {
        Self {
            egui_ctx: Default::default(),
            view_state: {
                // Assume scale for now until there is an event with a new one.
                let scale_factor = match open_settings.scale_policy {
                    WindowScalePolicy::ScaleFactor(scale) => scale,
                    WindowScalePolicy::SystemScaleFactor => 1.0,
                } as f32;
                let dimensions = [
                    (open_settings.logical_width * scale_factor as f64).round() as u32,
                    (open_settings.logical_height * scale_factor as f64).round() as u32,
                ];
                BaseviewState {
                    raw_input: {
                        egui::RawInput {
                            screen_rect: Some(Rect::from_min_size(
                                Pos2::new(0f32, 0f32),
                                vec2(
                                    open_settings.logical_width as f32,
                                    open_settings.logical_height as f32,
                                ),
                            )),
                            pixels_per_point: Some(scale_factor),
                            ..Default::default()
                        }
                    },
                    start_time: Instant::now(),
                    dimensions,
                    mouse_pos: None,
                    scale_factor,
                }
            },
            painter: egui_glow::Painter::new(gl, None, "")
                .map_err(|error| {
                    tracing::error!("error occurred in initializing painter:\n{}", error);
                })
                .unwrap(),
            shapes: Default::default(),
            textures_delta: Default::default(),
            user_update,
            scale_policy: open_settings.scale_policy,
            user_state,
        }
    }

    /// Paint the results of the last call to [`Self::run`].
    //
    // Source: https://github.com/emilk/egui/blob/master/crates/egui_glow/src/winit.rs#L67
    fn paint(&mut self) {
        let shapes = std::mem::take(&mut self.shapes);
        let mut textures_delta = std::mem::take(&mut self.textures_delta);

        for (id, image_delta) in textures_delta.set {
            self.painter.set_texture(id, &image_delta);
        }

        let clipped_primitives = self.egui_ctx.tessellate(shapes);
        self.painter.paint_primitives(
            self.view_state.dimensions,
            self.egui_ctx.pixels_per_point(),
            &clipped_primitives,
        );

        for id in textures_delta.free.drain(..) {
            self.painter.free_texture(id);
        }
    }
}

impl<S, U> WindowHandler for RealearnEgui<S, U>
where
    S: Send + 'static,
    U: FnMut(&egui::Context, &mut S) + Send + 'static,
{
    fn on_frame(&mut self, window: &mut Window) {
        // Source: https://github.com/emilk/egui/blob/master/crates/egui_glow/examples/pure_glow.rs#L19
        let repaint_after = {
            // Returns the `Duration` of the timeout after which egui should be repainted even if there's no new events.
            //
            // Call [`Self::paint`] later to paint.
            //
            // Source: https://github.com/emilk/egui/blob/master/crates/egui_glow/src/winit.rs#L45
            let raw_input = self.view_state.take_egui_input();
            let output = self.egui_ctx.run(raw_input, |ctx| {
                (self.user_update)(ctx, &mut self.user_state)
            });
            self.shapes = output.shapes;
            self.textures_delta.append(output.textures_delta);
            output.repaint_after
        };
        if repaint_after.is_zero() {
            // gl_window.window().request_redraw();
        }
        let gl_context = window.gl_context().unwrap();
        // TODO-high make_current could go after the tesselate in the paint function ...
        //  1. Does it make any difference, e.g. performance difference?
        //  2. Why do we need to make it current and not current in the first place? egui-glow
        //     doesn't do this, only egui-baseview ... maybe because we might have different
        //     OpenGL stuff in one process?:
        //     https://github.com/BillyDM/egui-baseview/blob/main/src/renderer/opengl_renderer.rs#L37
        unsafe { gl_context.make_current() };
        unsafe {
            use glow::HasContext as _;
            let gl = self.painter.gl();
            gl.clear_color(0.1, 0.1, 0.1, 1.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
        }
        // draw things behind egui here
        self.paint();
        // draw things on top of egui here
        gl_context.swap_buffers();
        unsafe { gl_context.make_not_current() };
    }

    // Source: https://github.com/BillyDM/egui-baseview/blob/main/src/window.rs#L345
    fn on_event(&mut self, _window: &mut Window, event: Event) -> EventStatus {
        match &event {
            baseview::Event::Mouse(event) => match event {
                baseview::MouseEvent::CursorMoved { position, .. } => {
                    let pos = pos2(position.x as f32, position.y as f32);
                    self.view_state.mouse_pos = Some(pos);
                    self.view_state
                        .raw_input
                        .events
                        .push(egui::Event::PointerMoved(pos));
                }
                baseview::MouseEvent::ButtonPressed { button, .. } => {
                    if let Some(pos) = self.view_state.mouse_pos {
                        if let Some(button) = translate_mouse_button(*button) {
                            self.view_state
                                .raw_input
                                .events
                                .push(egui::Event::PointerButton {
                                    pos,
                                    button,
                                    pressed: true,
                                    modifiers: self.view_state.raw_input.modifiers,
                                });
                        }
                    }
                }
                baseview::MouseEvent::ButtonReleased { button, .. } => {
                    if let Some(pos) = self.view_state.mouse_pos {
                        if let Some(button) = translate_mouse_button(*button) {
                            self.view_state
                                .raw_input
                                .events
                                .push(egui::Event::PointerButton {
                                    pos,
                                    button,
                                    pressed: false,
                                    modifiers: self.view_state.raw_input.modifiers,
                                });
                        }
                    }
                }
                baseview::MouseEvent::WheelScrolled { delta, .. } => {
                    let mut delta = match delta {
                        baseview::ScrollDelta::Lines { x, y } => {
                            let points_per_scroll_line = 50.0; // Scroll speed decided by consensus: https://github.com/emilk/egui/issues/461
                            egui::vec2(*x, *y) * points_per_scroll_line
                        }
                        baseview::ScrollDelta::Pixels { x, y } => {
                            if let Some(pixels_per_point) =
                                self.view_state.raw_input.pixels_per_point
                            {
                                egui::vec2(*x, *y) / pixels_per_point
                            } else {
                                egui::vec2(*x, *y)
                            }
                        }
                    };
                    if cfg!(target_os = "macos") {
                        // This is still buggy in winit despite
                        // https://github.com/rust-windowing/winit/issues/1695 being closed
                        delta.x *= -1.0;
                    }

                    let evt = if self.view_state.raw_input.modifiers.ctrl
                        || self.view_state.raw_input.modifiers.command
                    {
                        // Treat as zoom instead:
                        egui::Event::Zoom(delta.y)
                    } else {
                        egui::Event::Scroll(delta)
                    };
                    self.view_state.raw_input.events.push(evt);
                }
                baseview::MouseEvent::CursorLeft => {
                    self.view_state.mouse_pos = None;
                    self.view_state
                        .raw_input
                        .events
                        .push(egui::Event::PointerGone);
                }
                _ => {}
            },
            baseview::Event::Keyboard(event) => {
                use keyboard_types::Code;

                let pressed = event.state == keyboard_types::KeyState::Down;

                match event.code {
                    Code::ShiftLeft | Code::ShiftRight => {
                        self.view_state.raw_input.modifiers.shift = pressed
                    }
                    Code::ControlLeft | Code::ControlRight => {
                        self.view_state.raw_input.modifiers.ctrl = pressed;

                        #[cfg(not(target_os = "macos"))]
                        {
                            self.state.raw_input.modifiers.command = pressed;
                        }
                    }
                    Code::AltLeft | Code::AltRight => {
                        self.view_state.raw_input.modifiers.alt = pressed
                    }
                    Code::MetaLeft | Code::MetaRight => {
                        #[cfg(target_os = "macos")]
                        {
                            self.view_state.raw_input.modifiers.mac_cmd = pressed;
                            self.view_state.raw_input.modifiers.command = pressed;
                        }
                        () // prevent `rustfmt` from breaking this
                    }
                    _ => (),
                }

                if let Some(key) = translate_virtual_key_code(event.code) {
                    self.view_state.raw_input.events.push(egui::Event::Key {
                        key,
                        pressed,
                        modifiers: self.view_state.raw_input.modifiers,
                    });
                }

                if pressed {
                    // VirtualKeyCode::Paste etc in winit are broken/untrustworthy,
                    // so we detect these things manually:
                    if is_cut_command(self.view_state.raw_input.modifiers, event.code) {
                        self.view_state.raw_input.events.push(egui::Event::Cut);
                    } else if is_copy_command(self.view_state.raw_input.modifiers, event.code) {
                        self.view_state.raw_input.events.push(egui::Event::Copy);
                    } else if is_paste_command(self.view_state.raw_input.modifiers, event.code) {
                        // TODO-high paste
                    } else if let keyboard_types::Key::Character(written) = &event.key {
                        if !self.view_state.raw_input.modifiers.ctrl
                            && !self.view_state.raw_input.modifiers.command
                        {
                            self.view_state
                                .raw_input
                                .events
                                .push(egui::Event::Text(written.clone()));
                            self.egui_ctx.wants_keyboard_input();
                        }
                    }
                }
            }
            baseview::Event::Window(event) => match event {
                baseview::WindowEvent::Resized(window_info) => {
                    self.view_state.scale_factor = match self.scale_policy {
                        WindowScalePolicy::ScaleFactor(scale) => scale,
                        WindowScalePolicy::SystemScaleFactor => window_info.scale(),
                    } as f32;

                    let logical_size = (
                        (window_info.physical_size().width as f32 / self.view_state.scale_factor),
                        (window_info.physical_size().height as f32 / self.view_state.scale_factor),
                    );

                    self.view_state.raw_input.pixels_per_point = Some(self.view_state.scale_factor);

                    self.view_state.raw_input.screen_rect = Some(Rect::from_min_size(
                        Pos2::new(0f32, 0f32),
                        vec2(logical_size.0, logical_size.1),
                    ));
                    self.view_state.dimensions = [
                        window_info.physical_size().width,
                        window_info.physical_size().height,
                    ];
                    // self.redraw = true;
                }
                baseview::WindowEvent::WillClose => {}
                _ => {}
            },
        }
        EventStatus::Captured
    }
}

fn translate_mouse_button(button: baseview::MouseButton) -> Option<egui::PointerButton> {
    match button {
        baseview::MouseButton::Left => Some(egui::PointerButton::Primary),
        baseview::MouseButton::Right => Some(egui::PointerButton::Secondary),
        baseview::MouseButton::Middle => Some(egui::PointerButton::Middle),
        _ => None,
    }
}

fn translate_virtual_key_code(key: keyboard_types::Code) -> Option<egui::Key> {
    use egui::Key;
    use keyboard_types::Code;

    Some(match key {
        Code::ArrowDown => Key::ArrowDown,
        Code::ArrowLeft => Key::ArrowLeft,
        Code::ArrowRight => Key::ArrowRight,
        Code::ArrowUp => Key::ArrowUp,

        Code::Escape => Key::Escape,
        Code::Tab => Key::Tab,
        Code::Backspace => Key::Backspace,
        Code::Enter => Key::Enter,
        Code::Space => Key::Space,

        Code::Insert => Key::Insert,
        Code::Delete => Key::Delete,
        Code::Home => Key::Home,
        Code::End => Key::End,
        Code::PageUp => Key::PageUp,
        Code::PageDown => Key::PageDown,

        Code::Digit0 | Code::Numpad0 => Key::Num0,
        Code::Digit1 | Code::Numpad1 => Key::Num1,
        Code::Digit2 | Code::Numpad2 => Key::Num2,
        Code::Digit3 | Code::Numpad3 => Key::Num3,
        Code::Digit4 | Code::Numpad4 => Key::Num4,
        Code::Digit5 | Code::Numpad5 => Key::Num5,
        Code::Digit6 | Code::Numpad6 => Key::Num6,
        Code::Digit7 | Code::Numpad7 => Key::Num7,
        Code::Digit8 | Code::Numpad8 => Key::Num8,
        Code::Digit9 | Code::Numpad9 => Key::Num9,

        Code::KeyA => Key::A,
        Code::KeyB => Key::B,
        Code::KeyC => Key::C,
        Code::KeyD => Key::D,
        Code::KeyE => Key::E,
        Code::KeyF => Key::F,
        Code::KeyG => Key::G,
        Code::KeyH => Key::H,
        Code::KeyI => Key::I,
        Code::KeyJ => Key::J,
        Code::KeyK => Key::K,
        Code::KeyL => Key::L,
        Code::KeyM => Key::M,
        Code::KeyN => Key::N,
        Code::KeyO => Key::O,
        Code::KeyP => Key::P,
        Code::KeyQ => Key::Q,
        Code::KeyR => Key::R,
        Code::KeyS => Key::S,
        Code::KeyT => Key::T,
        Code::KeyU => Key::U,
        Code::KeyV => Key::V,
        Code::KeyW => Key::W,
        Code::KeyX => Key::X,
        Code::KeyY => Key::Y,
        Code::KeyZ => Key::Z,
        _ => {
            return None;
        }
    })
}

fn is_cut_command(modifiers: egui::Modifiers, keycode: keyboard_types::Code) -> bool {
    (modifiers.command && keycode == keyboard_types::Code::KeyX)
        || (cfg!(target_os = "windows")
            && modifiers.shift
            && keycode == keyboard_types::Code::Delete)
}

fn is_copy_command(modifiers: egui::Modifiers, keycode: keyboard_types::Code) -> bool {
    (modifiers.command && keycode == keyboard_types::Code::KeyC)
        || (cfg!(target_os = "windows")
            && modifiers.ctrl
            && keycode == keyboard_types::Code::Insert)
}

fn is_paste_command(modifiers: egui::Modifiers, keycode: keyboard_types::Code) -> bool {
    (modifiers.command && keycode == keyboard_types::Code::KeyV)
        || (cfg!(target_os = "windows")
            && modifiers.shift
            && keycode == keyboard_types::Code::Insert)
}
