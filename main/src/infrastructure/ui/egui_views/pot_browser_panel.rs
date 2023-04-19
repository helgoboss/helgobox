use crate::application::get_track_label;
use crate::base::blocking_lock;
use crate::domain::pot::nks::PresetId;
use crate::domain::pot::{
    with_preset_db, ChangeHint, CurrentPreset, DestinationTrackDescriptor, MacroParam, Preset,
    PresetLoadDestination, RuntimePotUnit, SharedRuntimePotUnit,
};
use crate::domain::BackboneState;
use egui::{
    vec2, Align, Button, CentralPanel, Color32, DragValue, Event, Frame, Key, Margin, Modifiers,
    RichText, ScrollArea, TextStyle, TopBottomPanel, Ui, Widget,
};
use egui::{Context, SidePanel};
use egui_extras::{Column, Size, StripBuilder, TableBuilder};
use egui_toast::Toasts;
use realearn_api::persistence::PotFilterItemKind;
use reaper_high::{Fx, FxParameter, Reaper, Track, Volume};
use reaper_medium::{ReaperNormalizedFxParamValue, ReaperVolumeValue};
use std::time::Duration;
use swell_ui::Window;

pub fn run_ui(ctx: &Context, state: &mut State) {
    let pot_unit = &mut blocking_lock(&*state.pot_unit);
    let toast_margin = 10.0;
    let mut toasts = Toasts::new()
        .anchor(ctx.screen_rect().max - vec2(toast_margin, toast_margin))
        .direction(egui::Direction::RightToLeft)
        .align_to_end(true);
    let mut focus_search_field = false;
    // Keyboard control
    enum KeyAction {
        NavigateWithinPresets(i32),
        LoadPreset,
        FocusSearchField,
    }
    let key_action = if ctx.wants_keyboard_input() {
        None
    } else {
        ctx.input_mut(|input| {
            let a = if input.count_and_consume_key(Default::default(), Key::ArrowUp) > 0 {
                KeyAction::NavigateWithinPresets(-1)
            } else if input.count_and_consume_key(Default::default(), Key::ArrowDown) > 0 {
                KeyAction::NavigateWithinPresets(1)
            } else if input.count_and_consume_key(Default::default(), Key::Enter) > 0 {
                KeyAction::LoadPreset
            } else if input.count_and_consume_key(Modifiers::COMMAND, Key::F) > 0 {
                KeyAction::FocusSearchField
            } else {
                return None;
            };
            Some(a)
        })
    };
    if let Some(key_action) = key_action {
        match key_action {
            KeyAction::NavigateWithinPresets(amount) => {
                if let Some(next_preset_index) = pot_unit.find_next_preset_index(amount) {
                    if let Some(next_preset_id) =
                        pot_unit.find_preset_id_at_index(next_preset_index)
                    {
                        pot_unit.set_preset_id(Some(next_preset_id));
                        if state.auto_preview {
                            let _ = pot_unit.play_preview(next_preset_id);
                        }
                    }
                }
            }
            KeyAction::LoadPreset => {
                if let Some(preset) = pot_unit.preset() {
                    load_preset_and_regain_focus(&preset, state.os_window, pot_unit, &mut toasts);
                }
            }
            KeyAction::FocusSearchField => {
                focus_search_field = true;
            }
        }
    }
    struct Curr {
        dest: Result<PresetLoadDestination, &'static str>,
        fx: Option<Fx>,
    }
    let curr = match pot_unit.resolve_preset_load_destination() {
        Ok(dest) => Curr {
            fx: dest.resolve(),
            dest: Ok(dest),
        },
        Err(e) => Curr {
            dest: Err(e),
            fx: None,
        },
    };
    // UI
    let panel_frame = Frame::central_panel(&ctx.style());
    if let Some(fx) = &curr.fx {
        let target_state = BackboneState::target_state().borrow();
        if let Some(current_preset) = target_state.current_fx_preset(fx) {
            // Macro params
            TopBottomPanel::top("top-panel")
                .frame(panel_frame)
                .resizable(false)
                .min_height(50.0)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        if current_preset.has_params() {
                            // Bank picker
                            ui.strong("Parameter bank:");
                            let mut new_bank_index = state.bank_index as usize;
                            egui::ComboBox::from_id_source("banks").show_index(
                                ui,
                                &mut new_bank_index,
                                current_preset.macro_param_bank_count() as usize,
                                |i| {
                                    if let Some(bank) =
                                        current_preset.find_macro_param_bank_at(i as _)
                                    {
                                        format!("{}. {}", i + 1, bank.name())
                                    } else {
                                        format!("Bank {} (doesn't exist)", i + 1)
                                    }
                                },
                            );
                            let new_bank_index = new_bank_index as u32;
                            if new_bank_index != state.bank_index {
                                state.bank_index = new_bank_index;
                            }
                        }
                        // Basic preset info
                        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                            ui.label(&current_preset.preset().name);
                            ui.strong("Current preset: ");
                        });
                    });
                    // Actual macro param display
                    if current_preset.has_params() {
                        show_macro_params(ui, fx, current_preset, state.bank_index);
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
                                state.bank_index = state.bank_index.saturating_add_signed(amount);
                            }
                        }
                    }
                });
        }
    }
    SidePanel::left("left-panel")
        .frame(panel_frame)
        .default_width(ctx.available_rect().width() * 0.5)
        .show(ctx, |ui| {
            // General controls
            ui.horizontal(|ui| {
                ui.checkbox(&mut state.paint_continuously, "Paint continuously")
                    .on_hover_text(
                        "Necessary to automatically display changes made by external controllers (via ReaLearn pot targets)",
                    );
                ui.checkbox(&mut state.auto_hide_sub_filters, "Auto-hide sub filters")
                    .on_hover_text("Makes sure you are not confronted with dozens of child filters if the corresponding top-level filter is set to <Any>");
            });
            ui.separator();
            // Add independent filter views
            let heading_height = ui.text_style_height(&TextStyle::Heading);
            ui
                .label(
                    RichText::new("Basics")
                        .text_style(TextStyle::Heading)
                        .size(heading_height),
                );
            ui.horizontal(|ui| {
                add_filter_view_content(
                    &state.pot_unit,
                    pot_unit,
                    PotFilterItemKind::NksProductType,
                    ui,
                    false
                );
                ui.separator();
                add_filter_view_content(
                    &state.pot_unit,
                    pot_unit,
                    PotFilterItemKind::NksContentType,
                    ui,
                    false
                );
            });
            // Add dependent filter views
            let show_sub_banks = !state.auto_hide_sub_filters
                || (pot_unit.filter_is_set_to_non_none(PotFilterItemKind::NksBank)
                    || pot_unit.get_filter(PotFilterItemKind::NksSubBank).is_some());
            let show_sub_categories = !state.auto_hide_sub_filters
                || (pot_unit.filter_is_set_to_non_none(PotFilterItemKind::NksCategory)
                    || pot_unit
                        .get_filter(PotFilterItemKind::NksSubCategory)
                        .is_some());
            let mut remaining_kind_count = 5;
            if !show_sub_banks {
                remaining_kind_count -= 1;
            }
            if !show_sub_categories {
                remaining_kind_count -= 1;
            }
            let filter_view_height = ui.available_height() / remaining_kind_count as f32;
            add_filter_view(
                ui,
                filter_view_height,
                &state.pot_unit,
                pot_unit,
                PotFilterItemKind::NksBank,
                false,
                false,
            );
            if show_sub_banks {
                add_filter_view(
                    ui,
                    filter_view_height,
                    &state.pot_unit,
                    pot_unit,
                    PotFilterItemKind::NksSubBank,
                    true,
                    true,
                );
            }
            add_filter_view(
                ui,
                filter_view_height,
                &state.pot_unit,
                pot_unit,
                PotFilterItemKind::NksCategory,
                true,
                false,
            );
            if show_sub_categories {
                add_filter_view(
                    ui,
                    filter_view_height,
                    &state.pot_unit,
                    pot_unit,
                    PotFilterItemKind::NksSubCategory,
                    true,
                    true,
                );
            }
            add_filter_view(
                ui,
                filter_view_height,
                &state.pot_unit,
                pot_unit,
                PotFilterItemKind::NksMode,
                true,
                false,
            );
        });
    let preset_count = pot_unit.preset_count();
    CentralPanel::default().frame(panel_frame).show(ctx, |ui| {
        // Settings
        ui.horizontal(|ui| {
            let old_wildcard_setting = pot_unit.runtime_state.use_wildcard_search;
            // Wildcards
            ui.checkbox(
                &mut pot_unit.runtime_state.use_wildcard_search,
                "Wildcards",
            ).on_hover_text("Allows more accurate search by enabling wildcards: Use * to match any string and ? to match any letter!");
            if pot_unit.runtime_state.use_wildcard_search != old_wildcard_setting {
                pot_unit.rebuild_collections(state.pot_unit.clone(), Some(ChangeHint::SearchExpression));
            }
            // Stats
            ui.checkbox(
                &mut state.show_stats,
                "Stats",
            ).on_hover_text("Show query statistics");
            // Preview
            ui.checkbox(&mut state.auto_preview, "Auto-preview")
                .on_hover_text("Automatically previews a sound when it's selected via mouse or keyboard");
            let old_volume = pot_unit.preview_volume();
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
                pot_unit.set_preview_volume(new_volume);
            }
        });
        ui.separator();
        // Search
        ui.horizontal(|ui| {
            ui.strong("Search:");
            let response = ui.text_edit_singleline(&mut pot_unit.runtime_state.search_expression);
            if focus_search_field {
                response.request_focus();
            }
            if response.changed() {
                pot_unit.rebuild_collections(state.pot_unit.clone(), Some(ChangeHint::SearchExpression));
            }
            ui.label(format!("➡ {preset_count} presets"));
        });
        // Stats
        if state.show_stats {
            ui.separator();
            ui.horizontal(|ui| {
                ui.strong("Last query: ");
                ui.label(format!("{}ms", pot_unit.stats.query_duration.as_millis()));
                ui.strong("Wasted runs/time: ");
                ui.label(format!("{}/{}ms", pot_unit.wasted_runs, pot_unit.wasted_duration.as_millis()));
            });
        }
        // Destination info
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            // Track descriptor
            let current_project = Reaper::get().current_project();
            {
                ui.strong("Destination track:");
                let old_track_code = match pot_unit.preset_load_destination_descriptor.track {
                    DestinationTrackDescriptor::SelectedTrack => 0usize,
                    DestinationTrackDescriptor::MasterTrack => 1usize,
                    DestinationTrackDescriptor::Track(i) => i as usize + 2
                };
                let mut new_track_code = old_track_code;
                egui::ComboBox::from_id_source("tracks").show_index(
                    ui,
                    &mut new_track_code,
                    current_project.track_count() as usize + 2,
                    |code| {
                        match code {
                            0 => "<Selected>".to_string(),
                            1 => "<Master>".to_string(),
                            _ => if let Some(track) = current_project.track_by_index(code as u32 - 2) {
                                get_track_label(&track)
                            } else {
                                format!("Track {} (doesn't exist)", code + 3)
                            }
                        }
                    },
                );
                if new_track_code != old_track_code {
                    let track_desc = match new_track_code {
                        0 => DestinationTrackDescriptor::SelectedTrack,
                        1 => DestinationTrackDescriptor::MasterTrack,
                        c => DestinationTrackDescriptor::Track(c as u32 - 2),
                    };
                    pot_unit.preset_load_destination_descriptor.track = track_desc;
                }
            }
            // FX descriptor
            {
                ui.strong("FX:");
                let resolved_track = pot_unit.preset_load_destination_descriptor.track.resolve(current_project);
                if let Ok(t) = resolved_track.as_ref() {
                    let chain = t.normal_fx_chain();
                    let mut fx_code = pot_unit.preset_load_destination_descriptor.fx_index as usize;
                    egui::ComboBox::from_id_source("fxs").show_index(
                        ui,
                        &mut fx_code,
                        chain.fx_count() as usize,
                        |code| {
                            match chain.fx_by_index(code as _) {
                                None => {
                                    format!("FX {} (doesn't exist)", code + 1)
                                }
                                Some(fx) => {
                                    format!("{}. {}", code + 1, fx.name())
                                }
                            }
                        },
                    );
                    pot_unit.preset_load_destination_descriptor.fx_index = fx_code as _;
                }
            }
            // Resolved
            if let Some(fx) = &curr.fx {
                if ui.small_button("Open FX").clicked() {
                    fx.show_in_floating_window();
                }
            }
        });
        // Preset table
        ui.separator();
        let text_height = egui::TextStyle::Body.resolve(ui.style()).size;
        let mut table = TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::auto())
            .column(Column::initial(60.0).at_least(40.0).clip(true))
            .column(Column::remainder().at_least(40.0))
            .min_scrolled_height(0.0);

        if pot_unit.preset_id() != state.last_preset_id {
            let scroll_index = match pot_unit.preset_id() {
                None => 0,
                Some(id) => {
                    pot_unit.find_index_of_preset(id).unwrap_or(0)
                }
            };
            table = table.scroll_to_row(scroll_index as usize, None);
        }
        table
            .header(20.0, |mut header| {
                header.col(|ui| {
                    ui.strong("Name");
                });
                header.col(|ui| {
                    ui.strong("Extension");
                });
                header.col(|ui| {
                    ui.strong("Actions");
                });
            })
            .body(|body| {
                body.rows(text_height, preset_count as usize, |row_index, mut row| {
                    let preset_id = pot_unit.find_preset_id_at_index(row_index as u32).unwrap();
                    let preset: Preset =
                        with_preset_db(|db| db.find_preset_by_id(preset_id).unwrap()).unwrap();
                    row.col(|ui| {
                        let mut button = Button::new(&preset.name).small();
                        if Some(preset_id) == pot_unit.preset_id() {
                            button = button.fill(Color32::LIGHT_BLUE);
                        }
                        let button = ui.add_sized(ui.available_size(), button);
                        if button.clicked() {
                            if state.auto_preview {
                                let _ = pot_unit.play_preview(preset_id);
                            }
                            pot_unit.set_preset_id(Some(preset_id));
                        }
                        if button.double_clicked() {
                            load_preset_and_regain_focus(&preset, state.os_window, pot_unit, &mut toasts);
                        }
                    });
                    row.col(|ui| {
                        ui.label(&preset.file_ext);
                    });
                    row.col(|ui| {
                        if ui.small_button("Load").clicked() {
                            load_preset_and_regain_focus(&preset, state.os_window, pot_unit, &mut toasts);
                        };
                        if !state.auto_preview {
                            if ui.small_button("Preview").clicked() {
                                process_potential_error(&pot_unit.play_preview(preset_id), &mut toasts);
                            }
                        }
                    });
                });
            });
    });
    toasts.show(ctx);
    if state.paint_continuously {
        // Necessary in order to not just repaint on clicks or so but also when controller changes
        // pot stuff.
        // TODO-medium-performance This is probably a performance hog. We could do better by reacting
        //  to notifications.
        ctx.request_repaint();
    }
    state.last_preset_id = pot_unit.preset_id();
}

fn show_macro_params(ui: &mut Ui, fx: &Fx, current_preset: &CurrentPreset, bank_index: u32) {
    // Added this UI just to not get duplicate table IDs
    ui.vertical(|ui| {
        if let Some(bank) = current_preset.find_macro_param_bank_at(bank_index) {
            let text_height = egui::TextStyle::Body.resolve(ui.style()).size;
            let table = TableBuilder::new(ui)
                .striped(false)
                .resizable(false)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .columns(Column::remainder(), bank.param_count() as _)
                .vscroll(false);
            struct CombinedParam<'a> {
                macro_param: &'a MacroParam,
                fx_param: Option<FxParameter>,
            }
            let params: Vec<_> = (0..bank.param_count())
                .filter_map(|i| {
                    let macro_param = bank.find_macro_param_at(i)?;
                    let combined_param = CombinedParam {
                        fx_param: {
                            if let Some(i) = macro_param.param_index {
                                let fx_param = fx.parameter_by_index(i);
                                if fx_param.is_available() {
                                    Some(fx_param)
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        },
                        macro_param,
                    };
                    Some(combined_param)
                })
                .collect();
            table
                .header(20.0, |mut header| {
                    for param in &params {
                        header.col(|ui| {
                            ui.vertical(|ui| {
                                ui.label(&param.macro_param.section_name);
                                let resp = ui.strong(&param.macro_param.name);
                                if let Some(fx_param) = &param.fx_param {
                                    resp.on_hover_text(fx_param.name().into_string());
                                }
                            });
                        });
                    }
                })
                .body(|mut body| {
                    body.row(text_height, |mut row| {
                        for param in &params {
                            row.col(|ui| {
                                if let Some(param) = param.fx_param.as_ref() {
                                    let old_param_value = param.reaper_normalized_value();
                                    let mut new_param_value_raw = old_param_value.get();
                                    DragValue::new(&mut new_param_value_raw)
                                        .speed(0.01)
                                        .custom_formatter(|v, _| {
                                            let v = ReaperNormalizedFxParamValue::new(v);
                                            param
                                                .format_reaper_normalized_value(v)
                                                .unwrap_or_default()
                                                .into_string()
                                        })
                                        .clamp_range(0.0..=1.0)
                                        .ui(ui);
                                    if new_param_value_raw != old_param_value.get() {
                                        let _ =
                                            param.set_reaper_normalized_value(new_param_value_raw);
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

#[derive(Debug)]
pub struct State {
    pot_unit: SharedRuntimePotUnit,
    auto_preview: bool,
    auto_hide_sub_filters: bool,
    show_stats: bool,
    paint_continuously: bool,
    os_window: Window,
    last_preset_id: Option<PresetId>,
    bank_index: u32,
}

impl State {
    pub fn new(pot_unit: SharedRuntimePotUnit, os_window: Window) -> Self {
        Self {
            pot_unit,
            auto_preview: true,
            auto_hide_sub_filters: true,
            show_stats: true,
            paint_continuously: true,
            os_window,
            last_preset_id: None,
            bank_index: 0,
        }
    }
}

fn add_filter_view(
    ui: &mut Ui,
    max_height: f32,
    shared_pot_unit: &SharedRuntimePotUnit,
    pot_unit: &mut RuntimePotUnit,
    kind: PotFilterItemKind,
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
    kind: PotFilterItemKind,
    ui: &mut Ui,
    wrapped: bool,
) {
    let old_filter_item_id = pot_unit.get_filter(kind);
    let mut new_filter_item_id = old_filter_item_id;
    let render = |ui: &mut Ui| {
        ui.selectable_value(&mut new_filter_item_id, None, "<Any>");
        for filter_item in pot_unit.collections.find_all_filter_items(kind) {
            let resp = ui.selectable_value(
                &mut new_filter_item_id,
                Some(filter_item.id),
                filter_item.effective_leaf_name(),
            );
            if let Some(parent_kind) = kind.parent() {
                if let Some(parent_name) = filter_item.parent_name.as_ref() {
                    if !parent_name.is_empty() {
                        let tooltip = match &filter_item.name {
                            None => {
                                format!("{parent_name} (directly associated with {parent_kind})")
                            }
                            Some(n) => format!("{parent_name} / {n}"),
                        };
                        resp.on_hover_text(tooltip);
                    }
                }
            }
        }
    };
    if wrapped {
        ui.horizontal_wrapped(render);
    } else {
        ui.horizontal(render);
    }
    if new_filter_item_id != old_filter_item_id {
        pot_unit.set_filter(kind, new_filter_item_id, shared_pot_unit.clone());
    }
}

fn load_preset_and_regain_focus(
    preset: &Preset,
    os_window: Window,
    pot_unit: &RuntimePotUnit,
    toasts: &mut Toasts,
) {
    process_potential_error(&pot_unit.load_preset(preset), toasts);
    os_window.focus_first_child();
}

fn process_potential_error(result: &Result<(), &'static str>, toasts: &mut Toasts) {
    if let Err(e) = result.as_ref() {
        toasts.error(*e, Duration::from_secs(1));
    }
}
