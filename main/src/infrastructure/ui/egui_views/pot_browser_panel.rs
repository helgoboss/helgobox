use crate::application::get_track_label;
use crate::base::{blocking_lock, blocking_write_lock};
use crate::domain::pot::{
    pot_db, ChangeHint, CurrentPreset, DestinationTrackDescriptor, LoadPresetOptions,
    LoadPresetWindowBehavior, MacroParam, Preset, PresetKind, RuntimePotUnit, SharedRuntimePotUnit,
};
use crate::domain::pot::{FilterItemId, PresetId};
use crate::domain::BackboneState;
use egui::collapsing_header::CollapsingState;
use egui::{
    popup_below_widget, vec2, Align, Button, CentralPanel, Color32, DragValue, Event, Frame, Key,
    Layout, RichText, ScrollArea, TextEdit, TextStyle, TopBottomPanel, Ui, Visuals, Widget,
};
use egui::{Context, SidePanel};
use egui_extras::{Column, TableBuilder};
use egui_toast::Toasts;
use realearn_api::persistence::PotFilterKind;
use reaper_high::{Fx, FxParameter, Reaper, Volume};
use reaper_medium::{ReaperNormalizedFxParamValue, ReaperVolumeValue};
use std::borrow::Cow;
use std::error::Error;
use std::mem;
use std::time::Duration;
use swell_ui::Window;

pub fn run_ui(ctx: &Context, state: &mut State) {
    let pot_unit = &mut blocking_lock(&*state.pot_unit, "PotUnit from PotBrowserPanel run_ui 1");
    // Query commonly used stuff
    let background_task_elapsed = pot_unit.background_task_elapsed();
    // Prepare toasts
    let toast_margin = 10.0;
    let mut toasts = Toasts::new()
        .anchor(ctx.screen_rect().max - vec2(toast_margin, toast_margin))
        .direction(egui::Direction::RightToLeft)
        .align_to_end(true);
    // Keyboard control
    enum KeyAction {
        NavigateWithinPresets(i32),
        LoadPreset,
        ClearSearchExpression,
        ClearLastSearchExpressionChar,
        ExtendSearchExpression(String),
    }
    // ctx.memory_mut(|mem| {
    //     mem.lock_focus()
    // });
    let key_action = ctx.input_mut(|input| {
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
    });
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
                if let Some((_, preset)) = pot_unit.preset_and_id() {
                    load_preset_and_regain_focus(
                        &preset,
                        state.os_window,
                        pot_unit,
                        &mut toasts,
                        state.load_preset_window_behavior,
                    );
                }
            }
            KeyAction::ClearLastSearchExpressionChar => {
                pot_unit.runtime_state.search_expression.pop();
                pot_unit.rebuild_collections(
                    state.pot_unit.clone(),
                    Some(ChangeHint::SearchExpression),
                );
            }
            KeyAction::ClearSearchExpression => {
                pot_unit.runtime_state.search_expression.clear();
                pot_unit.rebuild_collections(
                    state.pot_unit.clone(),
                    Some(ChangeHint::SearchExpression),
                );
            }
            KeyAction::ExtendSearchExpression(text) => {
                pot_unit.runtime_state.search_expression.push_str(&text);
                pot_unit.rebuild_collections(
                    state.pot_unit.clone(),
                    Some(ChangeHint::SearchExpression),
                );
            }
        }
    }
    let current_fx = pot_unit
        .resolve_destination()
        .ok()
        .and_then(|inst| inst.get_existing().and_then(|dest| dest.resolve()));
    // UI
    let panel_frame = Frame::central_panel(&ctx.style());
    // Top/bottom panel
    if let Some(fx) = &current_fx {
        let target_state = BackboneState::target_state().borrow();
        if let Some(current_preset) = target_state.current_fx_preset(fx) {
            // Macro params
            TopBottomPanel::top("top-bottom-panel")
                .frame(panel_frame)
                .min_height(50.0)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.heading(current_preset.preset().name());
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if current_preset.has_params() {
                                // Bank picker
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
                                // ui.strong("Parameter bank:");
                            }
                        })
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
    CentralPanel::default()
        .frame(Frame::none())
        .show(ctx, |ui| {
            // Left panel
            SidePanel::left("left-panel")
                .frame(panel_frame)
                .default_width(ctx.available_rect().width() * 0.5)
                .show_inside(ui, |ui| {
                    // General controls
                    ui.horizontal(|ui| {
                        ui.menu_button(RichText::new("Options").size(TOOLBAR_SIZE), |ui| {
                            ui.checkbox(&mut state.paint_continuously, "Paint continuously")
                                .on_hover_text(
                                    "Necessary to automatically display changes made by external controllers (via ReaLearn pot targets)",
                                );
                            ui.checkbox(&mut state.auto_hide_sub_filters, "Auto-hide sub filters")
                                .on_hover_text("Makes sure you are not confronted with dozens of child filters if the corresponding top-level filter is set to <Any>");
                            {
                                let old = pot_unit.show_excluded_filter_items();
                                let mut new = pot_unit.show_excluded_filter_items();
                                ui.checkbox(&mut new, "Show excluded filters")
                                    .on_hover_text("Shows all previously excluded filters again (via right click on filter item), so you can include them again if you want.");
                                if new != old {
                                    pot_unit.set_show_excluded_filter_items(new, state.pot_unit.clone());
                                }
                            }
                        });
                        if ui.button(RichText::new("🔃").size(TOOLBAR_SIZE))
                            .on_hover_text("Refreshes all databases (e.g. picks up new files on disk)")
                            .clicked() {
                            pot_unit.refresh_pot(state.pot_unit.clone());
                        }
                        if ui.button(RichText::new("🌙").size(TOOLBAR_SIZE))
                            .on_hover_text("Switches between light and dark theme")
                            .clicked() {
                            let mut style: egui::Style = (*ctx.style()).clone();
                            style.visuals = if style.visuals.dark_mode {
                                Visuals::light()
                            }  else {
                                Visuals::dark()
                            };
                            ctx.set_style(style);
                        }
                        let help_button = ui.button(RichText::new("❓").size(TOOLBAR_SIZE));
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
                        // Spinner
                        if background_task_elapsed.is_some() {
                            ui.spinner();
                        }
                        // Mini filters
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let shown = add_filter_view_content_as_icons(
                                &state.pot_unit,
                                pot_unit,
                                PotFilterKind::IsUser,
                                ui,
                            );
                            if shown {
                                ui.separator();
                            }
                            add_filter_view_content_as_icons(
                                &state.pot_unit,
                                pot_unit,
                                PotFilterKind::IsFavorite,
                                ui,
                            );
                        });
                    });
                    // Add independent filter views
                    let heading_height = ui.text_style_height(&TextStyle::Heading);
                    // Database
                    ui.separator();
                    ui
                        .label(
                            RichText::new("Database")
                                .text_style(TextStyle::Heading)
                                .size(heading_height),
                        );
                    add_filter_view_content(
                        &state.pot_unit,
                        pot_unit,
                        PotFilterKind::Database,
                        ui,
                        false
                    );
                    // Product type
                    ui.separator();
                    ui
                        .label(
                            RichText::new("Product type")
                                .text_style(TextStyle::Heading)
                                .size(heading_height),
                        );
                    add_filter_view_content(
                        &state.pot_unit,
                        pot_unit,
                        PotFilterKind::ProductKind,
                        ui,
                        true
                    );
                    // Add dependent filter views
                    ui.separator();
                    let show_banks = pot_unit.supports_filter_kind(PotFilterKind::Bank);
                    let show_sub_banks = show_banks
                        && pot_unit.supports_filter_kind(PotFilterKind::SubBank)
                        && (
                            !state.auto_hide_sub_filters
                            || (
                                    pot_unit.filters().is_set_to_concrete_value(PotFilterKind::Bank)
                                    || pot_unit.get_filter(PotFilterKind::SubBank).is_some()
                                )
                        );
                    let show_categories = pot_unit.supports_filter_kind(PotFilterKind::Category);
                    let show_sub_categories = show_categories
                        && pot_unit.supports_filter_kind(PotFilterKind::SubCategory)
                        && (
                            !state.auto_hide_sub_filters
                            || (
                                pot_unit.filters().is_set_to_concrete_value(PotFilterKind::Category)
                                || pot_unit.get_filter(PotFilterKind::SubCategory).is_some()
                            )
                        );
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
                                &state.pot_unit,
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
                                &state.pot_unit,
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
                                &state.pot_unit,
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
                                &state.pot_unit,
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
                                &state.pot_unit,
                                pot_unit,
                                PotFilterKind::Mode,
                                true,
                                false,
                            );
                        }
                    }
                });
            // Right panel
            let preset_count = pot_unit.preset_count();
            CentralPanel::default().frame(panel_frame).show_inside(ui, |ui| {
                // Settings
                ui.horizontal(|ui| {
                    // Options
                    ui.menu_button(RichText::new("Options").size(TOOLBAR_SIZE), |ui| {
                        // Wildcards
                        let old_wildcard_setting = pot_unit.runtime_state.use_wildcard_search;
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
                            "Display stats",
                        ).on_hover_text("Show query statistics");
                        // Preview
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut state.auto_preview, "Preview")
                                .on_hover_text("Automatically previews a sound when it's selected via mouse or keyboard");
                            // Preview volume
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
                        // Always show newly added FX
                        let mut show_if_newly_added = state.load_preset_window_behavior == LoadPresetWindowBehavior::ShowOnlyIfPreviouslyShownOrNewlyAdded;
                        ui.checkbox(&mut show_if_newly_added, "Show newly added FX")
                            .on_hover_text("When enabled, pot browser will always open the FX window when adding a new FX.");
                        state.load_preset_window_behavior = if show_if_newly_added {
                            LoadPresetWindowBehavior::ShowOnlyIfPreviouslyShownOrNewlyAdded
                        } else {
                            LoadPresetWindowBehavior::ShowOnlyIfPreviouslyShown
                        };
                        // Name track after preset
                        ui.checkbox(&mut pot_unit.name_track_after_preset, "Name track after preset")
                            .on_hover_text("When enabled, pot browser will rename the track to reflect the name of the preset.");
                    });
                    // Search
                    let text_edit = TextEdit::singleline(&mut pot_unit.runtime_state.search_expression)
                        .min_size(vec2(0.0, TOOLBAR_SIZE))
                        .hint_text("Type anywhere to search!")
                        .font(TextStyle::Monospace);
                    ui.add_enabled(false, text_edit)
                        .on_disabled_hover_text("Type anywhere to search!\nUse backspace to clear the last character\nand (Ctrl+Alt)/(Cmd)+Backspace to clear all.");
                    // Preset count
                    ui.label(format!("➡ {preset_count} presets"));
                });
                // Stats
                if state.show_stats {
                    ui.separator();
                    ui.horizontal(|ui| {
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
                        ui.label(format!("{}/{}ms", pot_unit.wasted_runs, pot_unit.wasted_duration.as_millis()));
                    });
                }
                // Info about selected preset
                if let Some((preset_id, preset)) = pot_unit.preset_and_id() {
                    ui.separator();
                    let id = ui.make_persistent_id("selected-preset");
                    CollapsingState::load_with_default_open(ui.ctx(), id, false)
                        .show_header(ui, |ui| {
                            ui.strong("Selected preset:");
                            ui.label(preset.name());
                            let _ = pot_db().try_with_db(preset_id.database_id, |db| {
                                ui.strong("from");
                                ui.label(db.name());
                            });
                            if let Some(product_name) = &preset.common.product_name {
                                ui.strong("for");
                                ui.label(product_name);
                            }
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                let favorites = BackboneState::get().pot_favorites();
                                let toggle = if let Ok(favorites) = favorites.try_read() {
                                    let mut is_favorite = favorites.is_favorite(preset_id);
                                    let icon = if is_favorite {
                                        "★"
                                    } else {
                                        "☆"
                                    };
                                    ui.toggle_value(&mut is_favorite, icon).changed()
                                } else {
                                    false
                                };
                                if toggle {
                                    blocking_write_lock(favorites, "favorite toggle").toggle_favorite(preset_id);
                                }
                            });
                        })
                        .body(|ui| {
                            ui.label("...")
                        });
                }
                // Destination info
                ui.separator();
                ui.horizontal(|ui| {
                    ScrollArea::horizontal().id_source("destination").show(ui, |ui| {
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
                                |code| {
                                    match code {
                                        0 => "<Selected track>".to_string(),
                                        1 => "<Master track>".to_string(),
                                        _ => if let Some(track) = current_project.track_by_index(code as u32 - SPECIAL_TRACK_COUNT as u32) {
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
                        let resolved_track = pot_unit.destination_descriptor.track.resolve(current_project);
                        if pot_unit.destination_descriptor.track.is_dynamic() {
                            ui.label("=");
                            let caption = match resolved_track.as_ref() {
                                Ok(t) => {
                                    format!("\"{}\"", get_track_label(t))
                                }
                                Err(_) => {
                                    "None (add new)".to_string()
                                }
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
                                egui::ComboBox::from_id_source("fxs")
                                    .show_index(
                                        ui,
                                        &mut fx_code,
                                        fx_count as usize + 1,
                                        |code| {
                                            match chain.fx_by_index(code as _) {
                                                None => {
                                                    "<New FX>".to_string()
                                                }
                                                Some(fx) => {
                                                    format!("{}. {}", code + 1, fx.name())
                                                }
                                            }
                                        },
                                    );
                                pot_unit.destination_descriptor.fx_index = fx_code as _;
                            }
                        }
                        // Resolved
                        if let Some(fx) = &current_fx {
                            if ui.small_button("Chain").on_hover_text("Shows the FX chain").clicked() {
                                fx.show_in_chain();
                            }
                            if ui.small_button("FX").on_hover_text("Shows the FX").clicked() {
                                fx.show_in_floating_window();
                            }
                        }
                    });
                });
                // Preset table
                ui.separator();
                let text_height = egui::TextStyle::Body.resolve(ui.style()).size;
                let mut table = TableBuilder::new(ui)
                    .striped(true)
                    .resizable(true)
                    .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                    // Preset name
                    .column(Column::auto().at_most(200.0))
                    // Plug-in or product
                    .column(Column::initial(200.0).clip(true).at_least(100.0))
                    // Extension
                    .column(Column::remainder())
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
                            ui.strong("Plug-in/product");
                        });
                        header.col(|ui| {
                            ui.strong("Ext");
                        });
                    })
                    .body(|body| {
                        body.rows(text_height, preset_count as usize, |row_index, mut row| {
                            let preset_id = pot_unit.find_preset_id_at_index(row_index as u32).unwrap();
                            // We just try to get a mutex lock because this is called very often.
                            // If we would insist on getting the lock, the GUI could freeze while
                            // the pot database has a write lock, which can happen during refresh.
                            let preset = pot_db().try_find_preset_by_id(preset_id);
                            // Name
                            row.col(|ui| {
                                let text = match preset.as_ref() {
                                    Err(_) => "⏳",
                                    Ok(None) => "<Preset gone>",
                                    Ok(Some(p)) => p.name(),
                                };
                                let mut button = Button::new(text).small();
                                if Some(preset_id) == pot_unit.preset_id() {
                                    button = button.fill(Color32::LIGHT_BLUE);
                                }
                                let button = ui.add_sized(ui.available_size(), button);
                                if let Ok(Some(preset)) = preset.as_ref() {
                                    if button.clicked() {
                                        if state.auto_preview {
                                            let _ = pot_unit.play_preview(preset_id);
                                        }
                                        pot_unit.set_preset_id(Some(preset_id));
                                    }
                                    if button.double_clicked() {
                                        load_preset_and_regain_focus(preset, state.os_window, pot_unit, &mut toasts, state.load_preset_window_behavior);
                                    }
                                }
                            });
                            let Ok(Some(preset)) = preset.as_ref() else {
                               return;
                            };
                            // Product
                            row.col(|ui| {
                                if let Some(n) = preset.common.product_name.as_ref() {
                                    ui.label(n)
                                        .on_hover_text(n);
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
            });
        });
    // Other stuff
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
    load_preset_window_behavior: LoadPresetWindowBehavior,
}

impl State {
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
        }
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
) -> bool {
    let old_filter_item_id = pot_unit.get_filter(kind);
    let mut new_filter_item_id = old_filter_item_id;
    for filter_item in pot_unit.filter_item_collections.get(kind) {
        let currently_selected = old_filter_item_id == Some(filter_item.id);
        let mut text = RichText::new(filter_item.icon.unwrap_or('-')).size(TOOLBAR_SIZE);
        if !currently_selected {
            text = text.weak();
        }
        let resp = ui.button(text).on_hover_ui(|ui| {
            ui.label(filter_item.effective_leaf_name());
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
    !pot_unit.filter_item_collections.get(kind).is_empty()
}

fn load_preset_and_regain_focus(
    preset: &Preset,
    os_window: Window,
    pot_unit: &mut RuntimePotUnit,
    toasts: &mut Toasts,
    window_behavior: LoadPresetWindowBehavior,
) {
    let options = LoadPresetOptions { window_behavior };
    process_potential_error(&pot_unit.load_preset(preset, options), toasts);
    os_window.focus_first_child();
}

fn process_potential_error(result: &Result<(), Box<dyn Error>>, toasts: &mut Toasts) {
    if let Err(e) = result.as_ref() {
        toasts.error(e.to_string(), Duration::from_secs(1));
    }
}

const TOOLBAR_SIZE: f32 = 15.0;

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
