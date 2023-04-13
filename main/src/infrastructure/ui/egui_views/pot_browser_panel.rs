use crate::base::blocking_lock;
use crate::domain::pot;
use crate::domain::pot::{with_preset_db, Preset, RuntimePotUnit, SharedRuntimePotUnit};
use egui::{CentralPanel, Color32, Frame, RichText, ScrollArea, TextStyle, TopBottomPanel, Ui};
use egui::{Context, SidePanel};
use egui_extras::{Column, Size, TableBuilder};
use realearn_api::persistence::PotFilterItemKind;
use reaper_high::Reaper;
use reaper_medium::MasterTrackBehavior;

pub fn run_ui(ctx: &Context, state: &mut State) {
    // TODO Provide option to only show sub filters when parent filter chosen
    // TODO Provide option to hide star filters
    // TODO Add preview button
    // TODO Make it possible to set FX slot into which the stuff should be loaded:
    //  - Last focused FX
    //  - Selected track, position X
    //  - Track X, position Y
    //  - ReaLearn instance FX
    //  - Below ReaLearn
    // TODO Provide some wheels to control parameters
    // TODO Mousewheel/touchpad scrolling support
    // TODO Resizing support
    let pot_unit = &mut blocking_lock(&*state.pot_unit);
    SidePanel::left("left-panel")
        .default_width(ctx.available_rect().width() * 0.5)
        .show(ctx, |ui| {
            // Auto-hide sub filter logic logic
            ui.horizontal(|ui| {
                ui.checkbox(&mut state.auto_hide_sub_filters, "Auto-hide sub filters");
            });
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
            // Add filter views
            let filter_view_height = ui.available_height() / remaining_kind_count as f32;
            add_filter_view(
                ui,
                Some(filter_view_height),
                &state.pot_unit,
                pot_unit,
                "Instrument",
                PotFilterItemKind::NksBank,
                false,
                false,
            );
            if show_sub_banks {
                add_filter_view(
                    ui,
                    Some(filter_view_height),
                    &state.pot_unit,
                    pot_unit,
                    "Bank",
                    PotFilterItemKind::NksSubBank,
                    true,
                    true,
                );
            }
            add_filter_view(
                ui,
                Some(filter_view_height),
                &state.pot_unit,
                pot_unit,
                "Type",
                PotFilterItemKind::NksCategory,
                true,
                false,
            );
            if show_sub_categories {
                add_filter_view(
                    ui,
                    Some(filter_view_height),
                    &state.pot_unit,
                    pot_unit,
                    "Sub type",
                    PotFilterItemKind::NksSubCategory,
                    true,
                    true,
                );
            }
            add_filter_view(
                ui,
                Some(filter_view_height),
                &state.pot_unit,
                pot_unit,
                "Character",
                PotFilterItemKind::NksMode,
                true,
                false,
            );
        });
    let preset_count = pot_unit.count_presets();
    CentralPanel::default().show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.strong("Search:");
            let response = ui.text_edit_singleline(pot_unit.runtime_state.search_expression_mut());
            if response.changed() {
                pot_unit.rebuild_collections(state.pot_unit.clone());
            }
            ui.checkbox(&mut state.auto_preview, "Auto-preview");
        });
        ui.horizontal(|ui| {
            ui.strong("Count: ");
            ui.label(preset_count.to_string());
            ui.strong("Query time: ");
            ui.label(format!("{}ms", pot_unit.stats.query_duration.as_millis()));
        });
        let text_height = egui::TextStyle::Body.resolve(ui.style()).size;
        TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::auto())
            // .column(Column::initial(200.0).at_least(60.0))
            .column(Column::initial(60.0).at_least(40.0).clip(true))
            .column(Column::remainder().at_least(40.0))
            .min_scrolled_height(0.0)
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
                        let mut text = RichText::new(&preset.name);
                        if Some(preset_id) == pot_unit.preset_id() {
                            text = text.background_color(Color32::LIGHT_BLUE);
                        }
                        let button = ui.button(text);
                        if button.clicked() {
                            if state.auto_preview {
                                let _ = pot_unit.play_preview(preset_id);
                            }
                            pot_unit.set_preset_id(Some(preset_id));
                        }
                        if button.double_clicked() {
                            let _ = pot_unit.load_preset(&preset);
                        }
                    });
                    row.col(|ui| {
                        ui.label(&preset.file_ext);
                    });
                    row.col(|ui| {
                        if ui.button("Load").clicked() {
                            let _ = pot_unit.load_preset(&preset);
                        };
                        if !state.auto_preview {
                            if ui.button("Preview").clicked() {
                                let _ = pot_unit.play_preview(preset_id);
                            }
                        }
                    });
                });
            });
    });
    // Necessary in order to not just repaint on clicks or so but also when controller changes
    // pot stuff.
    // TODO-high CONTINUE This is probably a performance hog. We could do better by reacting
    //  to notifications.
    ctx.request_repaint();
}

#[derive(Debug)]
pub struct State {
    pot_unit: SharedRuntimePotUnit,
    auto_preview: bool,
    auto_hide_sub_filters: bool,
}

impl State {
    pub fn new(pot_unit: SharedRuntimePotUnit) -> Self {
        Self {
            pot_unit,
            auto_preview: true,
            auto_hide_sub_filters: false,
        }
    }
}

fn add_filter_view(
    ui: &mut Ui,
    max_height: Option<f32>,
    shared_pot_unit: &SharedRuntimePotUnit,
    pot_unit: &mut RuntimePotUnit,
    label: &str,
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
        let old_filter_item_id = pot_unit.get_filter(kind);
        let mut new_filter_item_id = old_filter_item_id;
        // let mut panel = TopBottomPanel::top(kind)
        //     .resizable(false)
        //     .frame(Frame::none());
        let mut scroll_area = ScrollArea::vertical().id_source(kind);
        let h1_style_height = ui.text_style_height(&TextStyle::Heading);
        let heading_style_height = if indent {
            h1_style_height * 0.9
        } else {
            h1_style_height
        };
        let heading_height = ui
            .label(
                RichText::new(label)
                    .text_style(TextStyle::Heading)
                    .size(heading_style_height),
            )
            .rect
            .height();
        if let Some(h) = max_height {
            let h = h - heading_height - separator_height;
            // panel = panel.min_height(h).max_height(h);
            scroll_area = scroll_area.max_height(h);
        }
        // panel.show_inside(ui, |ui| {
        scroll_area.show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.selectable_value(&mut new_filter_item_id, None, "<Any>");
                for filter_item in pot_unit.collections.find_all_filter_items(kind) {
                    ui.selectable_value(
                        &mut new_filter_item_id,
                        Some(filter_item.id),
                        filter_item.effective_leaf_name(),
                    );
                }
            });
        });
        // });
        if new_filter_item_id != old_filter_item_id {
            pot_unit.set_filter(kind, new_filter_item_id, shared_pot_unit.clone());
        }
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
