use crate::application::get_track_label;
use crate::base::{
    blocking_lock, blocking_lock_arc, blocking_write_lock, NamedChannelSender, SenderToNormalThread,
};
use crate::domain::enigo::EnigoMouse;
use crate::domain::pot::preset_crawler::{
    import_crawled_presets, CrawlPresetArgs, PresetCrawlingState, PresetCrawlingStatus,
    SharedPresetCrawlingState,
};
use crate::domain::pot::preview_recorder::record_previews;
use crate::domain::pot::{
    pot_db, preset_crawler, spawn_in_pot_worker, ChangeHint, CurrentPreset,
    DestinationTrackDescriptor, LoadPresetError, LoadPresetOptions, LoadPresetWindowBehavior,
    MacroParam, Preset, PresetKind, RuntimePotUnit, SharedRuntimePotUnit,
};
use crate::domain::pot::{FilterItemId, PresetId};
use crate::domain::{AnyThreadBackboneState, BackboneState, Mouse, MouseCursorPosition};
use crossbeam_channel::Receiver;
use egui::collapsing_header::CollapsingState;
use egui::{
    popup_below_widget, vec2, Align, Align2, Button, CentralPanel, Color32, DragValue, Event,
    FontFamily, FontId, Frame, InputState, Key, Layout, RichText, ScrollArea, TextEdit, TextStyle,
    TopBottomPanel, Ui, Visuals, Widget,
};
use egui::{Context, SidePanel};
use egui_extras::{Column, Size, StripBuilder, TableBuilder};
use egui_toast::Toasts;
use lru::LruCache;
use realearn_api::persistence::PotFilterKind;
use reaper_high::{Fx, FxParameter, Reaper, Volume};
use reaper_medium::{ReaperNormalizedFxParamValue, ReaperVolumeValue};
use std::borrow::Cow;
use std::error::Error;
use std::fs::File;
use std::mem;
use std::num::NonZeroUsize;
use std::sync::MutexGuard;
use std::time::{Duration, Instant};
use swell_ui::Window;

#[derive(Debug)]
pub struct State {
    page: Page,
    main_state: MainState,
}

impl State {
    pub fn new(pot_unit: SharedRuntimePotUnit, os_window: Window) -> Self {
        Self {
            page: Default::default(),
            main_state: MainState::new(pot_unit, os_window),
        }
    }
}

#[derive(Debug, Default)]
enum Page {
    #[default]
    Warning,
    Main,
}

#[derive(Debug)]
pub struct MainState {
    pot_unit: SharedRuntimePotUnit,
    os_window: Window,
    auto_preview: bool,
    auto_hide_sub_filters: bool,
    show_stats: bool,
    paint_continuously: bool,
    last_preset_id: Option<PresetId>,
    bank_index: u32,
    load_preset_window_behavior: LoadPresetWindowBehavior,
    preset_cache: PresetCache,
    dialog: Option<Dialog>,
    mouse: EnigoMouse,
}

#[derive(Debug)]
enum Dialog {
    CrawlPresetsIntro,
    CrawlPresetsMouse {
        creation_time: Instant,
    },
    CrawlPresetsReady {
        fx: Fx,
        cursor_pos: MouseCursorPosition,
        stop_if_destination_exists: bool,
    },
    CrawlPresetsFailure {
        short_msg: Cow<'static, str>,
        detail_msg: Cow<'static, str>,
    },
    CrawlPresetsOngoing {
        crawling_state: SharedPresetCrawlingState,
    },
    CrawlPresetsStopped {
        crawling_state: SharedPresetCrawlingState,
        stop_reason: String,
        page: CrawlPresetsStoppedPage,
        chunks_file: Option<File>,
    },
    CrawlImportFinished,
    GeneralError {
        title: Cow<'static, str>,
        msg: Cow<'static, str>,
    },
    PreviewRecorderIntro,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum CrawlPresetsStoppedPage {
    Presets,
    Duplicates,
}

impl Dialog {
    fn general_error(
        title: impl Into<Cow<'static, str>>,
        msg: impl Into<Cow<'static, str>>,
    ) -> Self {
        Self::GeneralError {
            title: title.into(),
            msg: msg.into(),
        }
    }

    fn crawl_presets_mouse() -> Self {
        Self::CrawlPresetsMouse {
            creation_time: Instant::now(),
        }
    }

    fn crawl_presets_ready(fx: Fx, cursor_pos: MouseCursorPosition) -> Self {
        Self::CrawlPresetsReady {
            fx,
            cursor_pos,
            stop_if_destination_exists: false,
        }
    }

    fn crawl_presets_failure(
        short_msg: impl Into<Cow<'static, str>>,
        detail_msg: impl Into<Cow<'static, str>>,
    ) -> Self {
        Self::CrawlPresetsFailure {
            short_msg: short_msg.into(),
            detail_msg: detail_msg.into(),
        }
    }

    fn crawl_presets_ongoing(crawling_state: SharedPresetCrawlingState) -> Self {
        Self::CrawlPresetsOngoing { crawling_state }
    }

    fn crawl_presets_stopped(
        crawling_state: SharedPresetCrawlingState,
        stop_reason: String,
        chunks_file: File,
    ) -> Self {
        Self::CrawlPresetsStopped {
            crawling_state,
            stop_reason,
            page: CrawlPresetsStoppedPage::Presets,
            chunks_file: Some(chunks_file),
        }
    }
}

struct PresetCacheMessage {
    pot_db_revision: u8,
    preset_id: PresetId,
    preset_data: Option<PresetData>,
}

#[derive(Debug)]
enum PresetCacheEntry {
    Requested,
    NotFound,
    Found(PresetData),
}

#[derive(Debug)]
struct PresetData {
    preset: Preset,
    has_preview: bool,
}

pub fn run_ui(ctx: &Context, state: &mut State) {
    match state.page {
        Page::Warning => {
            run_warning_ui(ctx, state);
        }
        Page::Main => run_main_ui(ctx, &mut state.main_state),
    }
}

fn run_warning_ui(ctx: &Context, state: &mut State) {
    CentralPanel::default().show(ctx, |_| {
        egui::Window::new("A word of caution")
            .resizable(false)
            .collapsible(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.style_mut().text_styles = {
                    use FontFamily as F;
                    use TextStyle as T;
                    [
                        (T::Heading, FontId::new(30.0, F::Proportional)),
                        (T::Body, FontId::new(15.0, F::Proportional)),
                        (T::Monospace, FontId::new(14.0, F::Proportional)),
                        (T::Button, FontId::new(20.0, F::Proportional)),
                        (T::Small, FontId::new(10.0, F::Proportional)),
                    ]
                    .into()
                };
                ui.vertical_centered(|ui| {
                    ui.label(
                        "At the moment, Pot Browser is in an experimental stage and will not save \
                        any of your settings!",
                    );
                    ui.add_space(20.0);
                    ui.label(
                        RichText::new(
                            "So better don't invest much time into marking favorites, excluding \
                         filter items or adjusting other configuration!",
                        )
                        .strong(),
                    );
                    ui.add_space(20.0);
                    let button = Button::new("I understood. Really!");
                    if ui.add(button).clicked() {
                        state.page = Page::Main;
                    }
                })
            });
    });
}

const PRESET_CRAWLER_TITLE: &str = "Preset Crawler";
const PREVIEW_RECORDER_TITLE: &str = "Preview Recorder";
const PRESET_CRAWLER_COUNTDOWN_DURATION: Duration = Duration::from_secs(10);

fn run_main_ui(ctx: &Context, state: &mut MainState) {
    let pot_unit = &mut blocking_lock(&*state.pot_unit, "PotUnit from PotBrowserPanel run_ui 1");
    // Query commonly used stuff
    let background_task_elapsed = pot_unit.background_task_elapsed();
    // Integrate cache worker results into local cache
    state.preset_cache.set_pot_db_revision(pot_db().revision());
    while let Ok(message) = state.preset_cache.receiver.try_recv() {
        state.preset_cache.process_message(message);
    }
    // Prepare toasts
    let toast_margin = 10.0;
    let mut toasts = Toasts::new()
        .anchor(ctx.screen_rect().max - vec2(toast_margin, toast_margin))
        .direction(egui::Direction::RightToLeft)
        .align_to_end(true);
    // Process dialogs
    let mut change_dialog = None;
    if let Some(dialog) = state.dialog.as_mut() {
        let input = ProcessDialogsInput {
            shared_pot_unit: &state.pot_unit,
            pot_unit,
            toasts: &mut toasts,
            dialog,
            mouse: &state.mouse,
            os_window: state.os_window,
            change_dialog: &mut change_dialog,
        };
        process_dialogs(input, ctx);
    }
    if let Some(d) = change_dialog {
        state.dialog = d;
    }
    // Process keyboard
    let key_action = ctx.input_mut(|input| determine_key_action(input));
    if let Some(key_action) = key_action {
        let key_input = KeyInput {
            auto_preview: state.auto_preview,
            os_window: state.os_window,
            load_preset_window_behavior: state.load_preset_window_behavior,
            pot_unit: state.pot_unit.clone(),
            dialog: &mut state.dialog,
        };
        execute_key_action(key_input, pot_unit, &mut toasts, key_action);
    }
    let current_fx = pot_unit
        .resolve_destination()
        .ok()
        .and_then(|inst| inst.get_existing().and_then(|dest| dest.resolve()));
    // UI
    let panel_frame = Frame::central_panel(&ctx.style());
    // Upper panel (currently loaded preset with macro controls)
    if let Some(fx) = &current_fx {
        let target_state = BackboneState::target_state().borrow();
        if let Some(current_preset) = target_state.current_fx_preset(fx) {
            // Macro params
            TopBottomPanel::top("top-bottom-panel")
                .frame(panel_frame)
                .min_height(50.0)
                .show(ctx, |ui| {
                    show_current_preset_panel(&mut state.bank_index, fx, current_preset, ui);
                });
        }
    }
    // Main panel
    CentralPanel::default()
        .frame(Frame::none())
        .show(ctx, |ui| {
            // Left pane
            SidePanel::left("left-panel")
                .frame(panel_frame)
                .default_width(ctx.available_rect().width() * 0.5)
                .show_inside(ui, |ui| {
                    ui.style_mut().text_styles.insert(
                        TextStyle::Heading,
                        FontId::new(15.0, FontFamily::Proportional),
                    );
                    // Toolbar
                    ui.horizontal(|ui| {
                        left_right(
                            ui,
                            pot_unit,
                            TOOLBAR_HEIGHT_WITH_MARGIN,
                            280.0,
                            // Left side of toolbar: Toolbar
                            |ui, pot_unit| {
                                // Main options
                                let input = LeftOptionsDropdownInput {
                                    pot_unit,
                                    auto_hide_sub_filters: &mut state.auto_hide_sub_filters,
                                    paint_continuously: &mut state.paint_continuously,
                                    shared_pot_unit: &state.pot_unit,
                                };
                                add_left_options_dropdown(input, ui);
                                // Refresh button
                                if ui
                                    .button(RichText::new("🔃").size(TOOLBAR_HEIGHT))
                                    .on_hover_text(
                                        "Refreshes all databases (e.g. picks up new \
                                    files on disk)",
                                    )
                                    .clicked()
                                {
                                    pot_unit.refresh_pot(state.pot_unit.clone());
                                }
                                // Theme button
                                if ui
                                    .button(RichText::new("🌙").size(TOOLBAR_HEIGHT))
                                    .on_hover_text("Switches between light and dark theme")
                                    .clicked()
                                {
                                    let mut style: egui::Style = (*ctx.style()).clone();
                                    style.visuals = if style.visuals.dark_mode {
                                        Visuals::light()
                                    } else {
                                        Visuals::dark()
                                    };
                                    ctx.set_style(style);
                                }
                                // Help button
                                add_help_button(ui);
                                // Spinner
                                if background_task_elapsed.is_some() {
                                    ui.spinner();
                                }
                            },
                            // Right side of toolbar: Mini filters
                            |ui, pot_unit| {
                                if pot_unit.filter_item_collections.are_filled_already() {
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        add_mini_filters(&state.pot_unit, pot_unit, ui);
                                    });
                                }
                            },
                        );
                    });
                    // Filter panels
                    add_filter_panels(&state.pot_unit, pot_unit, state.auto_hide_sub_filters, ui);
                });
            // Right pane
            CentralPanel::default()
                .frame(panel_frame)
                .show_inside(ui, |ui| {
                    // Toolbar
                    ui.horizontal(|ui| {
                        ui.set_min_height(TOOLBAR_HEIGHT_WITH_MARGIN);
                        // Actions
                        ui.menu_button(RichText::new("Tools").size(TOOLBAR_HEIGHT), |ui| {
                            if ui.button(PRESET_CRAWLER_TITLE).clicked() {
                                state.dialog = Some(Dialog::CrawlPresetsIntro);
                                ui.close_menu();
                            }
                            if ui.button(PREVIEW_RECORDER_TITLE).clicked() {
                                state.dialog = Some(Dialog::PreviewRecorderIntro);
                                ui.close_menu();
                            }
                        });
                        // Options
                        let input = RightOptionsDropdownInput {
                            pot_unit,
                            shared_pot_unit: &state.pot_unit,
                            show_stats: &mut state.show_stats,
                            auto_preview: &mut state.auto_preview,
                            load_preset_window_behavior: &mut state.load_preset_window_behavior,
                        };
                        add_right_options_dropdown(input, ui);
                        // Search field
                        let text_edit =
                            TextEdit::singleline(&mut pot_unit.runtime_state.search_expression)
                                // .min_size(vec2(0.0, TOOLBAR_SIZE))
                                .desired_width(140.0)
                                .clip_text(false)
                                .hint_text("Enter search text!")
                                .font(TextStyle::Monospace);
                        ui.add_enabled(false, text_edit).on_disabled_hover_text(
                            "Type anywhere to search!\nUse backspace to clear \
                        the last character\nand (Ctrl+Alt)/(Cmd)+Backspace to clear all.",
                        );
                        // Preset count
                        let preset_count = pot_unit.preset_count();
                        ui.label(format!("➡ {preset_count} presets"));
                    });
                    // Stats
                    if state.show_stats {
                        ui.separator();
                        ui.horizontal(|ui| {
                            add_stats_panel(pot_unit, background_task_elapsed, ui);
                        });
                    }
                    // Info about selected preset
                    let current_preset_id = pot_unit.preset_id();
                    let current_preset_id_and_data =
                        current_preset_id.and_then(|id| match state.preset_cache.find_preset(id) {
                            PresetCacheEntry::Requested => None,
                            PresetCacheEntry::NotFound => None,
                            PresetCacheEntry::Found(data) => Some((id, data)),
                        });
                    ui.separator();
                    let widget_id = ui.make_persistent_id("selected-preset");
                    CollapsingState::load_with_default_open(ui.ctx(), widget_id, false)
                        .show_header(ui, |ui| {
                            left_right(
                                ui,
                                pot_unit,
                                ui.available_height(),
                                50.0,
                                // Left side of preset info
                                |ui, _| {
                                    ui.strong("Selected preset:");
                                    if let Some((preset_id, preset_data)) = current_preset_id_and_data {
                                        ui.label(preset_data.preset.name());
                                        let _ = pot_db().try_with_db(preset_id.database_id, |db| {
                                            ui.strong("from");
                                            ui.label(db.name());
                                        });
                                        if let Some(product_name) =
                                            &preset_data.preset.common.product_name
                                        {
                                            ui.strong("for");
                                            ui.label(product_name);
                                        }
                                    } else {
                                        ui.label("-");
                                    }
                                },
                                // Right side of preset info
                                |ui, pot_unit| {
                                    let Some((preset_id, preset_data)) = current_preset_id_and_data else {
                                        return;
                                    };
                                    // Favorite button
                                    let favorites = &AnyThreadBackboneState::get().pot_favorites;
                                    let toggle = if let Ok(favorites) = favorites.try_read() {
                                        let mut is_favorite = favorites.is_favorite(preset_id);
                                        let icon = if is_favorite { "★" } else { "☆" };
                                        ui.toggle_value(&mut is_favorite, icon).changed()
                                    } else {
                                        false
                                    };
                                    if toggle {
                                        blocking_write_lock(favorites, "favorite toggle")
                                            .toggle_favorite(preset_id);
                                    }
                                    // Preview button
                                    let preview_button = Button::new("🔊");
                                    let preview_button_response =
                                        ui.add_enabled(preset_data.has_preview, preview_button);
                                    if preview_button_response
                                        .on_hover_text("Play preset preview")
                                        .on_disabled_hover_text("Preset preview not available")
                                        .clicked()
                                    {
                                        let result = pot_unit.play_preview(preset_id);
                                        process_potential_error(&result, &mut toasts);
                                    }
                                },
                            );
                        })
                        .body(|ui| ui.label("..."));
                    // Destination info
                    ui.separator();
                    ui.horizontal(|ui| {
                        left_right(
                            ui,
                            pot_unit,
                            ui.available_height(),
                            75.0,
                            // Left side of destination info
                            |ui, pot_unit| {
                                add_destination_info_panel(ui, pot_unit);
                            },
                            // Right side of destination info
                            |ui, _| {
                                if let Some(fx) = &current_fx {
                                    if ui
                                        .small_button("Chain")
                                        .on_hover_text("Shows the FX chain")
                                        .clicked()
                                    {
                                        fx.show_in_chain();
                                    }
                                    if ui
                                        .small_button("FX")
                                        .on_hover_text("Shows the FX")
                                        .clicked()
                                    {
                                        fx.show_in_floating_window();
                                    }
                                }
                            },
                        );
                    });
                    // Preset table
                    ui.separator();
                    let input = PresetTableInput {
                        pot_unit,
                        toasts: &mut toasts,
                        last_preset_id: state.last_preset_id,
                        auto_preview: state.auto_preview,
                        os_window: state.os_window,
                        load_preset_window_behavior: state.load_preset_window_behavior,
                        dialog: &mut state.dialog,
                    };
                    add_preset_table(input, ui, &mut state.preset_cache);
                });
        });
    // Other stuff
    toasts.show(ctx);
    if state.paint_continuously {
        // Necessary e.g. in order to not just repaint on clicks or so but also when controller
        // changes pot stuff. But also for other things!
        ctx.request_repaint();
    }
    state.last_preset_id = pot_unit.preset_id();
}

struct ProcessDialogsInput<'a> {
    shared_pot_unit: &'a SharedRuntimePotUnit,
    pot_unit: &'a mut RuntimePotUnit,
    toasts: &'a mut Toasts,
    dialog: &'a mut Dialog,
    mouse: &'a EnigoMouse,
    os_window: Window,
    change_dialog: &'a mut Option<Option<Dialog>>,
}

fn process_dialogs(input: ProcessDialogsInput, ctx: &Context) {
    match input.dialog {
        Dialog::GeneralError { title, msg } => show_dialog(
            ctx,
            title,
            input.change_dialog,
            |ui, _| {
                ui.label(&**msg);
            },
            |ui, change_dialog| {
                if ui.button("Ok").clicked() {
                    *change_dialog = Some(None);
                };
            },
        ),
        Dialog::CrawlPresetsIntro => show_dialog(
            ctx,
            PRESET_CRAWLER_TITLE,
            input.change_dialog,
            |ui, _| {
                ui.label("Welcome to the Pot Preset Crawler!");
            },
            |ui, change_dialog| {
                if ui.button("Cancel").clicked() {
                    *change_dialog = Some(None);
                };
                if ui.button("Continue").clicked() {
                    *change_dialog = Some(Some(Dialog::crawl_presets_mouse()));
                }
            },
        ),
        Dialog::CrawlPresetsMouse { creation_time } => match input.mouse.cursor_position() {
            // Capturing current cursor position successful
            Ok(p) => show_dialog(
                ctx,
                PRESET_CRAWLER_TITLE,
                input.change_dialog,
                |ui, change_dialog| {
                    let elapsed = creation_time.elapsed();
                    ui.horizontal(|ui| {
                        ui.strong("Current mouse cursor position:");
                        ui.label(fmt_mouse_cursor_pos(p));
                    });
                    ui.horizontal(|ui| {
                        ui.strong("Countdown:");
                        let countdown = PRESET_CRAWLER_COUNTDOWN_DURATION.saturating_sub(elapsed);
                        ui.label(format!("{}s", countdown.as_secs()));
                    });
                    if elapsed >= PRESET_CRAWLER_COUNTDOWN_DURATION {
                        // Countdown finished
                        input.os_window.focus_first_child();
                        let next_dialog = if let Some(fx) = Reaper::get().focused_fx() {
                            if fx.fx.floating_window().is_some() {
                                Dialog::crawl_presets_ready(fx.fx, p)
                            } else {
                                Dialog::crawl_presets_failure(
                                    format!("Identified FX \"{}\" but it's not open in a floating window.", fx.fx.name()),
                                    "Please use the floating window to point the mouse to the next-preset button!",
                                )
                            }
                        } else {
                            Dialog::crawl_presets_failure(
                                "Couldn't identify the corresponding FX",
                                "",
                            )
                        };
                        *change_dialog = Some(Some(next_dialog));
                    }
                },
                |ui, change_dialog| {
                    if ui.button("Cancel").clicked() {
                        *change_dialog = Some(None);
                    };
                    if ui.button("Try again").clicked() {
                        *change_dialog = Some(Some(Dialog::crawl_presets_mouse()));
                    };
                },
            ),
            // Capturing current cursor position failed
            Err(e) => {
                *input.change_dialog = Some(Some(Dialog::crawl_presets_failure(
                    "Sorry, capturing the mouse position failed.",
                    e,
                )));
            }
        },
        Dialog::CrawlPresetsFailure {
            short_msg,
            detail_msg,
        } => show_dialog(
            ctx,
            PRESET_CRAWLER_TITLE,
            input.change_dialog,
            |ui, _| {
                ui.label(&**short_msg);
                if !detail_msg.is_empty() {
                    ui.label(&**detail_msg);
                }
            },
            |ui, change_dialog| {
                if ui.button("Cancel").clicked() {
                    *change_dialog = Some(None);
                };
                if ui.button("Try again").clicked() {
                    *change_dialog = Some(Some(Dialog::crawl_presets_mouse()));
                };
            },
        ),
        Dialog::CrawlPresetsReady {
            fx,
            cursor_pos,
            stop_if_destination_exists,
        } => show_dialog(
            ctx,
            PRESET_CRAWLER_TITLE,
            &mut (input.change_dialog, stop_if_destination_exists),
            |ui, (_, stop_if_destination_exists)| {
                ui.horizontal(|ui| {
                    ui.strong("FX:");
                    ui.label(fx.name().to_str());
                });
                ui.horizontal(|ui| {
                    ui.strong("Final cursor position:");
                    ui.label(fmt_mouse_cursor_pos(*cursor_pos));
                });
                ui.checkbox(stop_if_destination_exists, "Stop if destination exists");
            },
            |ui, (change_dialog, stop_if_destination_exists)| {
                if ui.button("Cancel").clicked() {
                    **change_dialog = Some(None);
                };
                if ui.button("Start crawling!").clicked() {
                    let crawling_state = PresetCrawlingState::new();
                    let os_window = input.os_window;
                    let args = CrawlPresetArgs {
                        fx: fx.clone(),
                        next_preset_cursor_pos: *cursor_pos,
                        state: crawling_state.clone(),
                        stop_if_destination_exists: **stop_if_destination_exists,
                        bring_focus_back_to_crawler: move || {
                            os_window.focus_first_child();
                        },
                    };
                    preset_crawler::crawl_presets(args);
                    **change_dialog = Some(Some(Dialog::crawl_presets_ongoing(crawling_state)));
                }
            },
        ),
        Dialog::CrawlPresetsOngoing { crawling_state } => show_dialog(
            ctx,
            PRESET_CRAWLER_TITLE,
            input.change_dialog,
            |ui, change_dialog| {
                ui.strong("Crawling in process...");
                let mut state = blocking_lock_arc(crawling_state, "run_main_ui crawling state");
                ui.horizontal(|ui| {
                    ui.strong("Presets crawled so far:");
                    let fmt_size = bytesize::ByteSize(state.bytes_crawled() as u64);
                    ui.label(format!("{} ({fmt_size})", state.preset_count()));
                });
                ui.horizontal(|ui| {
                    ui.strong("Skipped so far (because duplicate name):");
                    ui.label(state.duplicate_preset_name_count().to_string());
                });
                ui.horizontal(|ui| {
                    ui.strong("Last crawled preset:");
                    let text = if let Some(p) = state.last_crawled_preset() {
                        p.name()
                    } else {
                        "-"
                    };
                    ui.label(text);
                });
                if let PresetCrawlingStatus::Stopped {
                    chunks_file,
                    reason,
                } = state.status()
                {
                    let next_dialog = if let Some(chunks_file) = chunks_file.take() {
                        Dialog::crawl_presets_stopped(
                            crawling_state.clone(),
                            reason.clone(),
                            chunks_file,
                        )
                    } else {
                        Dialog::CrawlPresetsFailure {
                            short_msg: "Failure while crawling".into(),
                            detail_msg: reason.clone().into(),
                        }
                    };
                    *change_dialog = Some(Some(next_dialog));
                }
            },
            |ui, change_dialog| {
                if ui.button("Cancel").clicked() {
                    *change_dialog = Some(None);
                };
            },
        ),
        Dialog::CrawlPresetsStopped {
            crawling_state,
            stop_reason,
            page,
            chunks_file,
        } => {
            let cs = blocking_lock_arc(crawling_state, "run_main_ui crawling state 2");
            let preset_count = cs.preset_count();
            if preset_count == 0 {
                // When the preset count is 0, there's no preset left for import anymore.
                input.pot_unit.refresh_pot(input.shared_pot_unit.clone());
                *input.change_dialog = Some(Some(Dialog::CrawlImportFinished));
            } else {
                show_dialog(
                    ctx,
                    PRESET_CRAWLER_TITLE,
                    input.change_dialog,
                    |ui, _| {
                        add_crawl_presets_stopped_dialog_contents(
                            stop_reason.as_str(),
                            &cs,
                            preset_count,
                            page,
                            ui,
                        );
                    },
                    |ui, change_dialog| {
                        *chunks_file = if let Some(chunks_file) = chunks_file.take() {
                            if ui.button("Import!").clicked() {
                                let result =
                                    import_crawled_presets(crawling_state.clone(), chunks_file);
                                process_potential_error(&result, input.toasts);
                                None
                            } else {
                                Some(chunks_file)
                            }
                        } else {
                            None
                        };
                        if ui.button("Discard crawl results").clicked() {
                            *change_dialog = Some(None);
                        }
                    },
                )
            }
        }
        Dialog::CrawlImportFinished => show_dialog(
            ctx,
            PRESET_CRAWLER_TITLE,
            input.change_dialog,
            |ui, _| {
                ui.strong("Import done!");
            },
            |ui, change_dialog| {
                if ui.button("Close").clicked() {
                    *change_dialog = Some(None);
                }
            },
        ),
        Dialog::PreviewRecorderIntro => show_dialog(
            ctx,
            PREVIEW_RECORDER_TITLE,
            input.change_dialog,
            |ui, _| {
                ui.label("Welcome to the Pot Preview Recorder!");
            },
            |ui, change_dialog| {
                if ui.button("Cancel").clicked() {
                    *change_dialog = Some(None);
                };
                if ui.button("Continue").clicked() {
                    let preset_ids = input.pot_unit.preset_collection.iter().copied().collect();
                    record_previews(input.shared_pot_unit.clone(), preset_ids);
                }
            },
        ),
    }
}

fn add_crawl_presets_stopped_dialog_contents(
    stop_reason: &str,
    cs: &PresetCrawlingState,
    preset_count: u32,
    page: &mut CrawlPresetsStoppedPage,
    ui: &mut Ui,
) {
    ui.strong("Crawling stopped! Reason:");
    ui.label(stop_reason);
    ui.horizontal(|ui| {
        ui.strong("Crawled presets ready for import:");
        ui.label(preset_count.to_string());
    });
    ui.horizontal(|ui| {
        ui.strong("Show:");
        ui.selectable_value(page, CrawlPresetsStoppedPage::Presets, "Presets");
        ui.selectable_value(page, CrawlPresetsStoppedPage::Duplicates, "Duplicates");
    });
    let table_height = 400.0;
    match *page {
        CrawlPresetsStoppedPage::Presets => {
            let text_height = get_text_height(ui);
            TableBuilder::new(ui)
                .striped(true)
                .resizable(true)
                .max_scroll_height(table_height)
                .min_scrolled_height(table_height)
                .cell_layout(Layout::left_to_right(Align::Center))
                .column(Column::auto())
                .column(Column::remainder())
                .header(text_height, |mut header| {
                    header.col(|ui| {
                        ui.strong("Name");
                    });
                    header.col(|ui| {
                        ui.strong("Destination");
                    });
                })
                .body(|body| {
                    body.rows(text_height, preset_count as usize, |row_index, mut row| {
                        let entry = cs.crawled_presets().get_index(row_index);
                        if let Some((_, preset)) = entry {
                            row.col(|ui| {
                                ui.label(preset.name());
                            });
                            row.col(|ui| {
                                let dest = preset.destination().to_string_lossy();
                                ui.label(&*dest).on_hover_text(&*dest);
                            });
                        }
                    });
                });
        }
        CrawlPresetsStoppedPage::Duplicates => {
            show_as_list(ui, cs.duplicate_preset_names(), table_height);
        }
    }
}

struct PresetTableInput<'a> {
    pot_unit: &'a mut RuntimePotUnit,
    toasts: &'a mut Toasts,
    last_preset_id: Option<PresetId>,
    auto_preview: bool,
    os_window: Window,
    load_preset_window_behavior: LoadPresetWindowBehavior,
    dialog: &'a mut Option<Dialog>,
}

fn add_preset_table<'a>(input: PresetTableInput, ui: &mut Ui, preset_cache: &mut PresetCache) {
    let text_height = get_text_height(ui);
    let preset_count = input.pot_unit.preset_count();
    let mut table = TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        // Preset name
        .column(Column::auto())
        // Plug-in or product
        .column(Column::initial(200.0).clip(true).at_least(100.0))
        // Extension
        .column(Column::remainder())
        .min_scrolled_height(0.0);

    if input.pot_unit.preset_id() != input.last_preset_id {
        let scroll_index = match input.pot_unit.preset_id() {
            None => 0,
            Some(id) => input.pot_unit.find_index_of_preset(id).unwrap_or(0),
        };
        table = table.scroll_to_row(scroll_index as usize, None);
    }
    table
        .header(20.0, |mut header| {
            header.col(|ui| {
                ui.strong("Name");
            });
            header.col(|ui| {
                ui.strong("Plug-in/product");
            });
            header.col(|ui| {
                ui.strong("Extension");
            });
        })
        .body(|body| {
            body.rows(text_height, preset_count as usize, |row_index, mut row| {
                let preset_id = input
                    .pot_unit
                    .find_preset_id_at_index(row_index as u32)
                    .unwrap();
                let cache_entry = preset_cache.find_preset(preset_id);
                // Name
                row.col(|ui| {
                    const MAX_PRESET_NAME_LENGTH: usize = 40;
                    let text: Cow<str> = match cache_entry {
                        PresetCacheEntry::Requested => "⏳".into(),
                        PresetCacheEntry::NotFound => "<Preset not found>".into(),
                        PresetCacheEntry::Found(d) => {
                            shorten(d.preset.name().into(), MAX_PRESET_NAME_LENGTH)
                        }
                    };
                    let mut button = Button::new(text).small().fill(Color32::TRANSPARENT);
                    if let PresetCacheEntry::Found(data) = cache_entry {
                        if data.has_preview {
                            button = button.shortcut_text("🔊");
                        }
                    };
                    if Some(preset_id) == input.pot_unit.preset_id() {
                        // Preset is selected
                        button = button.fill(ui.style().visuals.selection.bg_fill);
                    }
                    let mut button = ui.add_sized(ui.available_size(), button);
                    if let PresetCacheEntry::Found(d) = cache_entry {
                        button = button.on_hover_text(d.preset.name());
                        #[cfg(any(target_os = "windows", target_os = "macos"))]
                        if let PresetKind::FileBased(k) = &d.preset.kind {
                            button = button.context_menu(|ui| {
                                if ui.button("Reveal in file manager").clicked() {
                                    if let Err(e) = opener::reveal(&k.path) {
                                        process_error(&e, input.toasts);
                                    }
                                    ui.close_menu();
                                }
                            });
                        }
                    }
                    if let PresetCacheEntry::Found(data) = cache_entry {
                        if button.clicked() {
                            if input.auto_preview {
                                let _ = input.pot_unit.play_preview(preset_id);
                            }
                            input.pot_unit.set_preset_id(Some(preset_id));
                        }
                        if button.double_clicked() {
                            load_preset_and_regain_focus(
                                &data.preset,
                                input.os_window,
                                input.pot_unit,
                                input.toasts,
                                input.load_preset_window_behavior,
                                input.dialog,
                            );
                        }
                    }
                });
                // Make other columns empty if preset info not available yet
                let PresetCacheEntry::Found(data) = cache_entry else {
                    row.col(|_| {});
                    row.col(|_| {});
                    return;
                };
                let preset = &data.preset;
                // Product
                row.col(|ui| {
                    if let Some(n) = preset.common.product_name.as_ref() {
                        ui.label(n).on_hover_text(n);
                    }
                });
                // Extension
                row.col(|ui| {
                    let text = match &preset.kind {
                        PresetKind::FileBased(k) => &k.file_ext,
                        PresetKind::Internal(_) => "",
                        PresetKind::DefaultFactory(_) => "",
                    };
                    ui.label(text);
                });
            });
        });
}

fn add_destination_info_panel(ui: &mut Ui, pot_unit: &mut RuntimePotUnit) {
    // Track descriptor
    let current_project = Reaper::get().current_project();
    {
        const SPECIAL_TRACK_COUNT: usize = 2;
        ui.strong("Load into");
        let track_count = current_project.track_count();
        let old_track_code = match &mut pot_unit.destination_descriptor.track {
            DestinationTrackDescriptor::SelectedTrack => 0usize,
            DestinationTrackDescriptor::MasterTrack => 1usize,
            DestinationTrackDescriptor::Track(i) => {
                // If configured track index too high, set it to
                // "new track at end of project".
                *i = (*i).min(track_count);
                *i as usize + SPECIAL_TRACK_COUNT
            }
        };
        let mut new_track_code = old_track_code;
        egui::ComboBox::from_id_source("tracks").show_index(
            ui,
            &mut new_track_code,
            SPECIAL_TRACK_COUNT + track_count as usize + 1,
            |code| match code {
                0 => "<Selected track>".to_string(),
                1 => "<Master track>".to_string(),
                _ => {
                    if let Some(track) =
                        current_project.track_by_index(code as u32 - SPECIAL_TRACK_COUNT as u32)
                    {
                        get_track_label(&track)
                    } else {
                        "<New track>".to_string()
                    }
                }
            },
        );
        if new_track_code != old_track_code {
            let track_desc = match new_track_code {
                0 => DestinationTrackDescriptor::SelectedTrack,
                1 => DestinationTrackDescriptor::MasterTrack,
                c => DestinationTrackDescriptor::Track(c as u32 - SPECIAL_TRACK_COUNT as u32),
            };
            pot_unit.destination_descriptor.track = track_desc;
        }
    }
    // Resolved track (if displaying it makes sense)
    let resolved_track = pot_unit
        .destination_descriptor
        .track
        .resolve(current_project);
    if pot_unit.destination_descriptor.track.is_dynamic() {
        ui.label("=");
        let caption = match resolved_track.as_ref() {
            Ok(t) => {
                format!("\"{}\"", get_track_label(t))
            }
            Err(_) => "None (add new)".to_string(),
        };
        let short_caption = shorten(caption.as_str().into(), 14);
        ui.label(short_caption).on_hover_text(caption);
    }
    // FX descriptor
    {
        if let Ok(t) = resolved_track.as_ref() {
            ui.label("at");
            let chain = t.normal_fx_chain();
            let fx_count = chain.fx_count();
            // If configured FX index too high, set it to "new FX at end of chain".
            pot_unit.destination_descriptor.fx_index =
                pot_unit.destination_descriptor.fx_index.min(fx_count);
            let mut fx_code = pot_unit.destination_descriptor.fx_index as usize;
            egui::ComboBox::from_id_source("fxs").show_index(
                ui,
                &mut fx_code,
                fx_count as usize + 1,
                |code| match chain.fx_by_index(code as _) {
                    None => "<New FX>".to_string(),
                    Some(fx) => {
                        format!("{}. {}", code + 1, fx.name())
                    }
                },
            );
            pot_unit.destination_descriptor.fx_index = fx_code as _;
        }
    }
}

fn add_stats_panel(
    pot_unit: &mut RuntimePotUnit,
    background_task_elapsed: Option<Duration>,
    ui: &mut Ui,
) {
    ui.strong("Last query: ");
    let total_duration = background_task_elapsed.unwrap_or(pot_unit.stats.total_query_duration());
    ui.label(format!("{}ms", total_duration.as_millis()));
    if background_task_elapsed.is_none() {
        let text = format!(
            "(= {} + {} + {} + {})",
            pot_unit.stats.filter_query_duration.as_millis(),
            pot_unit.stats.preset_query_duration.as_millis(),
            pot_unit.stats.sort_duration.as_millis(),
            pot_unit.stats.index_duration.as_millis(),
        );
        ui.label(text);
    }
    ui.strong("Wasted runs/time: ");
    ui.label(format!(
        "{}/{}ms",
        pot_unit.wasted_runs,
        pot_unit.wasted_duration.as_millis()
    ));
}

struct RightOptionsDropdownInput<'a> {
    pot_unit: &'a mut RuntimePotUnit,
    shared_pot_unit: &'a SharedRuntimePotUnit,
    show_stats: &'a mut bool,
    auto_preview: &'a mut bool,
    load_preset_window_behavior: &'a mut LoadPresetWindowBehavior,
}

fn add_right_options_dropdown(input: RightOptionsDropdownInput, ui: &mut Ui) {
    ui.menu_button(RichText::new("Options").size(TOOLBAR_HEIGHT), |ui| {
        // Wildcards
        let old_wildcard_setting = input.pot_unit.runtime_state.use_wildcard_search;
        ui.checkbox(
            &mut input.pot_unit.runtime_state.use_wildcard_search,
            "Wildcards",
        )
        .on_hover_text(
            "Allows more accurate search by enabling wildcards: Use * to match any \
        string and ? to match any letter!",
        );
        if input.pot_unit.runtime_state.use_wildcard_search != old_wildcard_setting {
            input.pot_unit.rebuild_collections(
                input.shared_pot_unit.clone(),
                Some(ChangeHint::SearchExpression),
            );
        }
        // Stats
        ui.checkbox(input.show_stats, "Display stats")
            .on_hover_text("Show query statistics");
        // Preview
        ui.horizontal(|ui| {
            ui.checkbox(input.auto_preview, "Auto-preview")
                .on_hover_text(
                    "Automatically previews a sound when it's selected via mouse or keyboard",
                );
            // Preview volume
            let old_volume = input.pot_unit.preview_volume();
            let mut new_volume_raw = old_volume.get();
            egui::DragValue::new(&mut new_volume_raw)
                .speed(0.01)
                .custom_formatter(|v, _| {
                    Volume::from_reaper_value(ReaperVolumeValue::new(v)).to_string()
                })
                .clamp_range(0.0..=1.0)
                .ui(ui)
                .on_hover_text("Change volume of the sound previews");
            let new_volume = ReaperVolumeValue::new(new_volume_raw);
            if new_volume != old_volume {
                input.pot_unit.set_preview_volume(new_volume);
            }
        });
        // Always show newly added FX
        let mut show_if_newly_added = *input.load_preset_window_behavior
            == LoadPresetWindowBehavior::ShowOnlyIfPreviouslyShownOrNewlyAdded;
        ui.checkbox(&mut show_if_newly_added, "Show newly added FX")
            .on_hover_text(
                "When enabled, Pot Browser will always open the FX window when adding a \
            new FX.",
            );
        *input.load_preset_window_behavior = if show_if_newly_added {
            LoadPresetWindowBehavior::ShowOnlyIfPreviouslyShownOrNewlyAdded
        } else {
            LoadPresetWindowBehavior::ShowOnlyIfPreviouslyShown
        };
        // Name track after preset
        ui.checkbox(
            &mut input.pot_unit.name_track_after_preset,
            "Name track after preset",
        )
        .on_hover_text(
            "When enabled, Pot Browser will rename the track to reflect the name of \
            the preset.",
        );
    });
}

fn add_filter_panels(
    shared_unit: &SharedRuntimePotUnit,
    pot_unit: &mut RuntimePotUnit,
    auto_hide_sub_filters: bool,
    ui: &mut Ui,
) {
    let heading_height = ui.text_style_height(&TextStyle::Heading);
    // Database
    ui.separator();
    ui.label(RichText::new("Database").heading().size(heading_height));
    add_filter_view_content(shared_unit, pot_unit, PotFilterKind::Database, ui, false);
    // Product type
    ui.separator();
    ui.label(RichText::new("Product type").heading().size(heading_height));
    add_filter_view_content(shared_unit, pot_unit, PotFilterKind::ProductKind, ui, true);
    // Add dependent filter views
    ui.separator();
    let show_banks = pot_unit.supports_filter_kind(PotFilterKind::Bank);
    let show_sub_banks = show_banks
        && pot_unit.supports_filter_kind(PotFilterKind::SubBank)
        && (!auto_hide_sub_filters
            || (pot_unit
                .filters()
                .is_set_to_concrete_value(PotFilterKind::Bank)
                || pot_unit.get_filter(PotFilterKind::SubBank).is_some()));
    let show_categories = pot_unit.supports_filter_kind(PotFilterKind::Category);
    let show_sub_categories = show_categories
        && pot_unit.supports_filter_kind(PotFilterKind::SubCategory)
        && (!auto_hide_sub_filters
            || (pot_unit
                .filters()
                .is_set_to_concrete_value(PotFilterKind::Category)
                || pot_unit.get_filter(PotFilterKind::SubCategory).is_some()));
    let show_modes = pot_unit.supports_filter_kind(PotFilterKind::Mode);
    let mut remaining_kind_count = 5;
    if !show_banks {
        remaining_kind_count -= 1;
    }
    if !show_sub_banks {
        remaining_kind_count -= 1;
    }
    if !show_categories {
        remaining_kind_count -= 1;
    }
    if !show_sub_categories {
        remaining_kind_count -= 1;
    }
    if !show_modes {
        remaining_kind_count -= 1;
    }
    if remaining_kind_count > 0 {
        let filter_view_height = ui.available_height() / remaining_kind_count as f32;
        if show_banks {
            add_filter_view(
                ui,
                filter_view_height,
                shared_unit,
                pot_unit,
                PotFilterKind::Bank,
                false,
                false,
            );
        }
        if show_sub_banks {
            add_filter_view(
                ui,
                filter_view_height,
                shared_unit,
                pot_unit,
                PotFilterKind::SubBank,
                true,
                true,
            );
        }
        if show_categories {
            add_filter_view(
                ui,
                filter_view_height,
                shared_unit,
                pot_unit,
                PotFilterKind::Category,
                true,
                false,
            );
        }
        if show_sub_categories {
            add_filter_view(
                ui,
                filter_view_height,
                shared_unit,
                pot_unit,
                PotFilterKind::SubCategory,
                true,
                true,
            );
        }
        if show_modes {
            add_filter_view(
                ui,
                filter_view_height,
                shared_unit,
                pot_unit,
                PotFilterKind::Mode,
                true,
                false,
            );
        }
    }
}

fn add_mini_filters(
    shared_unit: &SharedRuntimePotUnit,
    pot_unit: &mut MutexGuard<RuntimePotUnit>,
    ui: &mut Ui,
) {
    add_filter_view_content_as_icons(shared_unit, pot_unit, PotFilterKind::IsUser, ui);
    ui.separator();
    add_filter_view_content_as_icons(shared_unit, pot_unit, PotFilterKind::IsFavorite, ui);
    ui.separator();
    add_filter_view_content_as_icons(shared_unit, pot_unit, PotFilterKind::IsSupported, ui);
    ui.separator();
    add_filter_view_content_as_icons(shared_unit, pot_unit, PotFilterKind::IsAvailable, ui);
}

fn add_help_button(ui: &mut Ui) {
    let help_button = ui.button(RichText::new("❓").size(TOOLBAR_HEIGHT));
    let help_id = ui.make_persistent_id("help");
    if help_button.clicked() {
        ui.memory_mut(|mem| mem.toggle_popup(help_id));
    }
    popup_below_widget(ui, help_id, &help_button, |ui| {
        TableBuilder::new(ui)
            .column(Column::auto().at_least(200.0))
            .column(Column::remainder())
            .cell_layout(Layout::left_to_right(Align::Center))
            .body(|mut body| {
                for (interaction, reaction) in HELP.iter() {
                    body.row(30.0, |mut row| {
                        row.col(|ui| {
                            ui.strong(format!("{interaction}:"));
                        });
                        row.col(|ui| {
                            ui.label(*reaction);
                        });
                    });
                }
            });
    });
}

struct LeftOptionsDropdownInput<'a> {
    pot_unit: &'a mut RuntimePotUnit,
    auto_hide_sub_filters: &'a mut bool,
    paint_continuously: &'a mut bool,
    shared_pot_unit: &'a SharedRuntimePotUnit,
}

fn add_left_options_dropdown(mut input: LeftOptionsDropdownInput, ui: &mut Ui) {
    ui.menu_button(RichText::new("Options").size(TOOLBAR_HEIGHT), |ui| {
        ui.checkbox(&mut input.auto_hide_sub_filters, "Auto-hide sub filters")
            .on_hover_text(
                "Makes sure you are not confronted with dozens of child filters if \
                the corresponding top-level filter is set to <Any>",
            );
        {
            let old = input.pot_unit.show_excluded_filter_items();
            let mut new = input.pot_unit.show_excluded_filter_items();
            ui.checkbox(&mut new, "Show excluded filters")
                .on_hover_text(
                    "Shows all previously excluded filters again (via right click on \
                    filter item), so you can include them again if you want.",
                );
            if new != old {
                input
                    .pot_unit
                    .set_show_excluded_filter_items(new, input.shared_pot_unit.clone());
            }
        }
        ui.checkbox(
            &mut input.paint_continuously,
            "Paint continuously (devs only)",
        )
        .on_hover_text("Leave this enabled. This option is only intended for developers.");
    });
}

fn show_current_preset_panel(
    bank_index: &mut u32,
    fx: &Fx,
    current_preset: &CurrentPreset,
    ui: &mut Ui,
) {
    ui.horizontal(|ui| {
        ui.heading(current_preset.preset().name());
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if current_preset.has_params() {
                // Bank picker
                let mut new_bank_index = *bank_index as usize;
                egui::ComboBox::from_id_source("banks").show_index(
                    ui,
                    &mut new_bank_index,
                    current_preset.macro_param_bank_count() as usize,
                    |i| {
                        if let Some(bank) = current_preset.find_macro_param_bank_at(i as _) {
                            format!("{}. {}", i + 1, bank.name())
                        } else {
                            format!("Bank {} (doesn't exist)", i + 1)
                        }
                    },
                );
                let new_bank_index = new_bank_index as u32;
                if new_bank_index != *bank_index {
                    *bank_index = new_bank_index;
                }
                // ui.strong("Parameter bank:");
            }
        })
    });
    // Actual macro param display
    if current_preset.has_params() {
        show_macro_params(ui, fx, current_preset, *bank_index);
        // Scroll handler. This must come at the end, otherwise ui_contains_pointer
        // works with a zero-sized UI!
        if ui.ui_contains_pointer() {
            let vertical_scroll = ui.input(|i| {
                i.events.iter().find_map(|e| match e {
                    Event::Scroll(s) if s.y != 0.0 => Some(s.y),
                    _ => None,
                })
            });
            if let Some(s) = vertical_scroll {
                let amount = -s.signum() as i32;
                *bank_index = bank_index.saturating_add_signed(amount);
            }
        }
    }
}

struct KeyInput<'a> {
    auto_preview: bool,
    os_window: Window,
    load_preset_window_behavior: LoadPresetWindowBehavior,
    pot_unit: SharedRuntimePotUnit,
    dialog: &'a mut Option<Dialog>,
}

fn execute_key_action(
    input: KeyInput,
    pot_unit: &mut MutexGuard<RuntimePotUnit>,
    mut toasts: &mut Toasts,
    key_action: KeyAction,
) {
    match key_action {
        KeyAction::NavigateWithinPresets(amount) => {
            if let Some(next_preset_index) = pot_unit.find_next_preset_index(amount) {
                if let Some(next_preset_id) = pot_unit.find_preset_id_at_index(next_preset_index) {
                    pot_unit.set_preset_id(Some(next_preset_id));
                    if input.auto_preview {
                        let _ = pot_unit.play_preview(next_preset_id);
                    }
                }
            }
        }
        KeyAction::LoadPreset => {
            if let Some((_, preset)) = pot_unit.preset_and_id() {
                load_preset_and_regain_focus(
                    &preset,
                    input.os_window,
                    pot_unit,
                    &mut toasts,
                    input.load_preset_window_behavior,
                    input.dialog,
                );
            }
        }
        KeyAction::ClearLastSearchExpressionChar => {
            pot_unit.runtime_state.search_expression.pop();
            pot_unit
                .rebuild_collections(input.pot_unit.clone(), Some(ChangeHint::SearchExpression));
        }
        KeyAction::ClearSearchExpression => {
            pot_unit.runtime_state.search_expression.clear();
            pot_unit
                .rebuild_collections(input.pot_unit.clone(), Some(ChangeHint::SearchExpression));
        }
        KeyAction::ExtendSearchExpression(text) => {
            pot_unit.runtime_state.search_expression.push_str(&text);
            pot_unit
                .rebuild_collections(input.pot_unit.clone(), Some(ChangeHint::SearchExpression));
        }
    }
}

fn determine_key_action(input: &mut InputState) -> Option<KeyAction> {
    let mut action = None;
    input.events.retain_mut(|event| match event {
        Event::Key {
            key,
            pressed,
            modifiers,
            ..
        } => {
            if *pressed {
                match key {
                    Key::ArrowUp => action = Some(KeyAction::NavigateWithinPresets(-1)),
                    Key::ArrowDown => action = Some(KeyAction::NavigateWithinPresets(1)),
                    Key::Enter => action = Some(KeyAction::LoadPreset),
                    Key::Backspace if modifiers.command => {
                        action = Some(KeyAction::ClearSearchExpression)
                    }
                    Key::Backspace if !modifiers.command => {
                        action = Some(KeyAction::ClearLastSearchExpressionChar)
                    }
                    _ => {}
                }
            }
            false
        }
        Event::Text(text) => {
            action = Some(KeyAction::ExtendSearchExpression(mem::take(text)));
            false
        }
        _ => true,
    });
    action
}

fn show_macro_params(ui: &mut Ui, fx: &Fx, current_preset: &CurrentPreset, bank_index: u32) {
    // Added this UI just to not get duplicate table IDs
    ui.vertical(|ui| {
        if let Some(bank) = current_preset.find_macro_param_bank_at(bank_index) {
            let text_height = get_text_height(ui);
            let table = TableBuilder::new(ui)
                .striped(false)
                .resizable(false)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .columns(Column::remainder(), bank.param_count() as _)
                .vscroll(false);
            struct CombinedParam<'a> {
                macro_param: &'a MacroParam,
                fx_param: Option<FxParameter>,
                param_index: u32,
            }
            let slots: Vec<_> = bank
                .params()
                .iter()
                .map(|macro_param| {
                    let param_index = macro_param.param_index?;
                    let combined_param = CombinedParam {
                        fx_param: {
                            let fx_param = fx.parameter_by_index(param_index);
                            if fx_param.is_available() {
                                Some(fx_param)
                            } else {
                                None
                            }
                        },
                        macro_param,
                        param_index,
                    };
                    Some(combined_param)
                })
                .collect();
            table
                .header(20.0, |mut header| {
                    for slot in &slots {
                        header.col(|ui| {
                            let Some(param) = slot else {
                                // Empty slot yields empty column
                                return;
                            };
                            ui.vertical(|ui| {
                                ui.strong(&param.macro_param.section_name);
                                let resp = ui.label(&param.macro_param.name);
                                resp.on_hover_ui(|ui| {
                                    let hover_text = if let Some(fx_param) = &param.fx_param {
                                        fx_param.name().into_string()
                                    } else {
                                        format!(
                                            "Mapped parameter {} doesn't exist in actual plug-in",
                                            param.param_index + 1
                                        )
                                    };
                                    ui.label(hover_text);
                                });
                            });
                        });
                    }
                })
                .body(|mut body| {
                    body.row(text_height, |mut row| {
                        for slot in &slots {
                            row.col(|ui| {
                                let Some(param) = slot else {
                                    // Empty slot yields empty column
                                    return;
                                };
                                if let Some(fx_param) = param.fx_param.as_ref() {
                                    let old_param_value = fx_param.reaper_normalized_value();
                                    let mut new_param_value_raw = old_param_value.get();
                                    DragValue::new(&mut new_param_value_raw)
                                        .speed(0.01)
                                        .custom_formatter(|v, _| {
                                            let v = ReaperNormalizedFxParamValue::new(v);
                                            fx_param
                                                .format_reaper_normalized_value(v)
                                                .unwrap_or_default()
                                                .into_string()
                                        })
                                        .clamp_range(0.0..=1.0)
                                        .ui(ui);
                                    if new_param_value_raw != old_param_value.get() {
                                        let _ = fx_param
                                            .set_reaper_normalized_value(new_param_value_raw);
                                    }
                                }
                            });
                        }
                    });
                });
        } else {
            ui.vertical_centered_justified(|ui| {
                ui.heading(format!("Parameter bank {} doesn't exist", bank_index + 1));
            });
        };
    });
}

impl MainState {
    pub fn new(pot_unit: SharedRuntimePotUnit, os_window: Window) -> Self {
        Self {
            pot_unit,
            auto_preview: true,
            auto_hide_sub_filters: true,
            show_stats: false,
            paint_continuously: true,
            os_window,
            last_preset_id: None,
            bank_index: 0,
            load_preset_window_behavior: Default::default(),
            preset_cache: PresetCache::new(),
            dialog: Default::default(),
            mouse: Default::default(),
        }
    }
}

#[derive(Debug)]
struct PresetCache {
    lru_cache: LruCache<PresetId, PresetCacheEntry>,
    sender: SenderToNormalThread<PresetCacheMessage>,
    receiver: Receiver<PresetCacheMessage>,
    pot_db_revision: u8,
}

impl PresetCache {
    pub fn new() -> Self {
        let (sender, receiver) =
            SenderToNormalThread::new_unbounded_channel("pot browser preset cache");
        Self {
            lru_cache: LruCache::new(NonZeroUsize::new(500).unwrap()),
            sender,
            receiver,
            pot_db_revision: 0,
        }
    }

    /// This can be called very often in order to keep the cache informed about the revision
    /// of the current data that it's supposed to cache. This call only does something
    /// substantial when the revision changes, i.e. it invalidates the cache.
    pub fn set_pot_db_revision(&mut self, revision: u8) {
        if self.pot_db_revision != revision {
            self.pot_db_revision = revision;
            // Change of revision! We need to invalidate all results.
            self.lru_cache.clear();
        }
    }

    pub fn find_preset(&mut self, preset_id: PresetId) -> &PresetCacheEntry {
        self.lru_cache.get_or_insert(preset_id, || {
            let sender = self.sender.clone();
            let pot_db_revision = self.pot_db_revision;
            spawn_in_pot_worker(async move {
                let pot_db = pot_db();
                let preset = pot_db.try_find_preset_by_id(preset_id)?;
                let preset_data = preset.map(|p| {
                    let preview_file = pot_db.find_preview_file_by_preset_id(preset_id);
                    let has_preview = preview_file.map(|f| f.exists()).unwrap_or(false);
                    PresetData {
                        preset: p,
                        has_preview,
                    }
                });
                let message = PresetCacheMessage {
                    pot_db_revision,
                    preset_id,
                    preset_data,
                };
                sender.send_complaining(message);
                Ok(())
            });
            PresetCacheEntry::Requested
        })
    }

    pub fn process_message(&mut self, message: PresetCacheMessage) {
        if message.pot_db_revision != self.pot_db_revision {
            // This worker result is based on a previous revision of the pot unit.
            // Not valid anymore.
            return;
        }
        let data = match message.preset_data {
            None => PresetCacheEntry::NotFound,
            Some(e) => PresetCacheEntry::Found(e),
        };
        self.lru_cache.put(message.preset_id, data);
    }
}

fn add_filter_view(
    ui: &mut Ui,
    max_height: f32,
    shared_pot_unit: &SharedRuntimePotUnit,
    pot_unit: &mut RuntimePotUnit,
    kind: PotFilterKind,
    add_separator: bool,
    indent: bool,
) {
    let separator_height = if add_separator {
        if indent {
            let vertical_spacing = 6.0;
            ui.add_space(vertical_spacing);
            vertical_spacing
        } else {
            ui.separator().rect.height()
        }
    } else {
        0.0
    };
    let mut render = |ui: &mut Ui| {
        // let mut panel = TopBottomPanel::top(kind)
        //     .resizable(false)
        //     .frame(Frame::none());
        let h1_style_height = ui.text_style_height(&TextStyle::Heading);
        let heading_style_height = if indent {
            h1_style_height * 0.9
        } else {
            h1_style_height
        };
        let heading_height = ui
            .label(
                RichText::new(kind.to_string())
                    .text_style(TextStyle::Heading)
                    .size(heading_style_height),
            )
            .rect
            .height();
        // panel = panel.min_height(h).max_height(h);
        // panel.show_inside(ui, |ui| {
        ScrollArea::vertical()
            .id_source(kind)
            .max_height(max_height - heading_height - separator_height)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                add_filter_view_content(shared_pot_unit, pot_unit, kind, ui, true);
            });
        // });
    };
    if indent {
        ui.horizontal_top(|ui| {
            ui.add_space(15.0);
            // ui.separator();
            ui.vertical(|ui| {
                render(ui);
            });
        });
    } else {
        render(ui);
    }
}

fn add_filter_view_content(
    shared_pot_unit: &SharedRuntimePotUnit,
    pot_unit: &mut RuntimePotUnit,
    kind: PotFilterKind,
    ui: &mut Ui,
    wrapped: bool,
) {
    enum UiAction {
        InOrExcludeFilter(PotFilterKind, FilterItemId, bool),
    }
    let mut action = None;
    let old_filter_item_id = pot_unit.get_filter(kind);
    let mut new_filter_item_id = old_filter_item_id;
    let render = |ui: &mut Ui| {
        let exclude_list = BackboneState::get().pot_filter_exclude_list();
        ui.selectable_value(&mut new_filter_item_id, None, "<Any>");
        for filter_item in pot_unit.filter_item_collections.get(kind) {
            let mut text = RichText::new(filter_item.effective_leaf_name());
            if exclude_list.contains(kind, filter_item.id) {
                text = text.weak();
            };
            let mut resp = ui.selectable_value(&mut new_filter_item_id, Some(filter_item.id), text);
            if let Some(more_info) = filter_item.more_info.as_ref() {
                resp = resp.on_hover_text(more_info);
            } else if let Some(parent_kind) = kind.parent() {
                if let Some(parent_name) = filter_item.parent_name.as_ref() {
                    if !parent_name.is_empty() {
                        resp = resp.on_hover_ui(|ui| {
                            let tooltip = match &filter_item.name {
                                None => {
                                    format!(
                                        "{parent_name} (directly associated with {parent_kind})"
                                    )
                                }
                                Some(n) => format!("{parent_name} / {n}"),
                            };
                            ui.label(tooltip);
                        });
                    }
                }
            }
            // Context menu
            if kind.allows_excludes() {
                resp.context_menu(|ui| {
                    let is_excluded = exclude_list.contains(kind, filter_item.id);
                    let (text, include) = if is_excluded {
                        ("Include again (globally)", true)
                    } else {
                        ("Exclude (globally)", false)
                    };
                    if ui.button(text).clicked() {
                        action = Some(UiAction::InOrExcludeFilter(kind, filter_item.id, include));
                        ui.close_menu();
                    }
                });
            }
        }
    };
    if wrapped {
        ui.horizontal_wrapped(render);
    } else {
        ui.horizontal(render);
    }
    // Execute actions
    if new_filter_item_id != old_filter_item_id {
        pot_unit.set_filter(kind, new_filter_item_id, shared_pot_unit.clone());
    }
    if let Some(act) = action {
        match act {
            UiAction::InOrExcludeFilter(kind, id, include) => {
                pot_unit.include_filter_item(kind, id, include, shared_pot_unit.clone());
            }
        }
    }
}

/// Returns true if at least one filter was displayed.
fn add_filter_view_content_as_icons(
    shared_pot_unit: &SharedRuntimePotUnit,
    pot_unit: &mut RuntimePotUnit,
    kind: PotFilterKind,
    ui: &mut Ui,
) {
    let old_filter_item_id = pot_unit.get_filter(kind);
    let mut new_filter_item_id = old_filter_item_id;
    // Reverse because we use right-to-left order
    for filter_item in pot_unit.filter_item_collections.get(kind).iter().rev() {
        let currently_selected = old_filter_item_id == Some(filter_item.id);
        let mut text = RichText::new(filter_item.icon.unwrap_or('-')).size(TOOLBAR_HEIGHT);
        if !currently_selected {
            text = text.weak();
        }
        let resp = ui.button(text).on_hover_ui(|ui| {
            let name = filter_item.effective_leaf_name();
            let tooltip: Cow<str> = if let Some(more) = &filter_item.more_info {
                format!("{name}: {more}").into()
            } else {
                name
            };
            ui.label(tooltip);
        });
        if resp.clicked() {
            new_filter_item_id = if currently_selected {
                None
            } else {
                Some(filter_item.id)
            }
        };
    }
    if new_filter_item_id != old_filter_item_id {
        pot_unit.set_filter(kind, new_filter_item_id, shared_pot_unit.clone());
    }
}

fn load_preset_and_regain_focus(
    preset: &Preset,
    os_window: Window,
    pot_unit: &mut RuntimePotUnit,
    toasts: &mut Toasts,
    window_behavior: LoadPresetWindowBehavior,
    dialog: &mut Option<Dialog>,
) {
    let options = LoadPresetOptions { window_behavior };
    if let Err(e) = pot_unit.load_preset(preset, options) {
        match e {
            LoadPresetError::UnsupportedPresetFormat {
                file_extension,
                is_shim_preset,
            } => {
                if is_shim_preset {
                    let text = format!(
                        "Found shim preset for unsupported original format but even the shim
                        seems to have an unsupported format: {file_extension}",
                    );
                    show_error_toast(&text, toasts);
                } else {
                    let text = r#"
Unfortunately, Pot can't automatically open presets for Native Instrument's own plug-ins. You have the following options:

Approach A: Load the preset manually

1. Load the plug-in, e.g. by right-clicking the preset and choosing "Load plug-in".
2. Then find the preset within the plug-in. But if the plug-in supports drag'n'drop, e.g. Kontakt, you can alternatively reveal the preset file in the file explorer (available in right click menu as well) and drag it onto the plug-in.

Approach B: Preset crawling

Try to use Preset Crawler (menu "Tools") to crawl the presets of Native Instrument plug-ins. All presets that have been successfully crawled and imported should be loaded automatically in future. One issue is that crawling only seems to work with the VST2 versions of the NI plug-ins.

In both cases, you will not be able to take advantage of the preset parameter banks. That's just how it is for now.
If you don't want unsupported presets to show up in Pot Browser, enable the ✔ filter in the toolbar.
"#;
                    *dialog = Some(Dialog::general_error("Can't open preset", text));
                }
            }
            _ => process_error(&e, toasts),
        }
    }
    os_window.focus_first_child();
}

fn process_potential_error(result: &Result<(), Box<dyn Error>>, toasts: &mut Toasts) {
    if let Err(e) = result {
        process_error(&**e, toasts);
    }
}

fn process_error(error: &dyn Error, toasts: &mut Toasts) {
    show_error_toast(&error.to_string(), toasts);
}

fn show_error_toast(text: &str, toasts: &mut Toasts) {
    toasts.error(text, Duration::from_secs(1));
}

const TOOLBAR_HEIGHT: f32 = 15.0;
const TOOLBAR_HEIGHT_WITH_MARGIN: f32 = TOOLBAR_HEIGHT + 5.0;

const HELP: &[(&str, &str)] = &[
    ("Click preset", "Select it (and preview it if enabled)"),
    ("Double-click preset", "Load it"),
    ("Up/down arrays", "Navigate in preset list"),
    ("Enter", "Load currently selected preset"),
    ("Type letters", "Enter search text"),
    ("(Ctrl+Alt)/(Cmd) + Backspace", "Clear search expression"),
];

fn shorten(text: Cow<str>, max_len: usize) -> Cow<str> {
    if text.len() > max_len {
        let mut s = text.into_owned();
        truncate_in_place(&mut s, max_len);
        s.push('…');
        s.into()
    } else {
        text
    }
}

fn truncate_in_place(s: &mut String, max_chars: usize) {
    let bytes = truncate(s, max_chars).len();
    s.truncate(bytes);
}

fn truncate(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        None => s,
        Some((idx, _)) => &s[..idx],
    }
}

fn left_right<A>(
    ui: &mut Ui,
    arg: &mut A,
    height: f32,
    right_width: f32,
    left: impl FnOnce(&mut Ui, &mut A),
    right: impl FnOnce(&mut Ui, &mut A),
) {
    // At first, we need to add a vertical strip in order to not take infinity height.
    StripBuilder::new(ui)
        .size(Size::exact(height))
        .clip(true)
        .vertical(|mut strip| {
            strip.strip(|strip| {
                strip
                    .size(Size::remainder())
                    .size(Size::exact(right_width))
                    .horizontal(|mut strip| {
                        // Left strip
                        strip.cell(|ui| {
                            ui.horizontal(|ui| {
                                left(ui, arg);
                            });
                        });
                        // Right strip
                        strip.cell(|ui| {
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                right(ui, arg);
                            });
                        });
                    });
            });
        });
}

enum KeyAction {
    NavigateWithinPresets(i32),
    LoadPreset,
    ClearSearchExpression,
    ClearLastSearchExpressionChar,
    ExtendSearchExpression(String),
}

fn show_dialog<V>(
    ctx: &Context,
    title: &str,
    value: &mut V,
    content: impl FnOnce(&mut Ui, &mut V),
    buttons: impl FnOnce(&mut Ui, &mut V),
) {
    egui::Window::new(title)
        .resizable(false)
        .collapsible(false)
        .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.set_min_width(500.0);
                // Top margin
                ui.add_space(10.0);
                // Content
                content(ui, value);
                // Space between content and buttons
                ui.add_space(20.0);
                // Buttons
                ui.horizontal(|ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        // Right margin
                        ui.add_space(20.0);
                        // Buttons
                        buttons(ui, value);
                    });
                });
                // Bottom margin
                ui.add_space(10.0);
            })
        });
}

fn fmt_mouse_cursor_pos(pos: MouseCursorPosition) -> String {
    format!("{}, {}", pos.x, pos.y)
}

fn get_text_height(ui: &Ui) -> f32 {
    TextStyle::Body.resolve(ui.style()).size
}

fn show_as_list(ui: &mut Ui, entries: &[impl AsRef<str>], height: f32) {
    let text_height = get_text_height(ui);
    TableBuilder::new(ui)
        .striped(true)
        .resizable(false)
        .min_scrolled_height(height)
        .max_scroll_height(height)
        .column(Column::remainder())
        .body(|body| {
            body.rows(text_height, entries.len(), |i, mut row| {
                row.col(|ui| {
                    ui.label(entries[i].as_ref());
                });
            });
        });
}
