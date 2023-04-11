use crate::base::{NamedChannelSender, SenderToNormalThread};
use crate::domain::pot::{with_preset_db, PotUnit, Preset, RuntimePotUnit};
use crate::domain::{pot, ReaperTargetType};
use derivative::Derivative;
use egui::{CentralPanel, Ui};
use egui::{Context, SidePanel};
use egui_extras::{Size, TableBody, TableBuilder, TableRow};
use enum_iterator::IntoEnumIterator;
use realearn_api::persistence::{LearnableTargetKind, PotFilterItemKind, TargetTouchCause};
use reaper_high::Reaper;
use std::collections::HashSet;

pub fn run_ui(ctx: &Context, state: &mut State) {
    // TODO Add text search
    // TODO Use left outer join to also display stuff that's not associated with any bank/category/mode
    // TODO Explicitly add "should be null" filter option for displaying non-associated stuff
    // TODO Make rows in preset table selectable
    // TODO Make layout less jumping around
    // TODO Execute query in background
    // TODO Provide option to only show sub filters when parent filter chosen
    // TODO Provide option to hide star filters
    // TODO Reflect instance pot unit
    // TODO Add preview button
    // TODO Make it possible to set FX slot into which the stuff should be loaded:
    //  - Last focused FX
    //  - Selected track, position X
    //  - Track X, position Y
    //  - ReaLearn instance FX
    //  - Below ReaLearn
    // TODO Provide some wheels to control parameters
    let mut pot_unit = state.pot_unit.loaded().unwrap();
    SidePanel::left("left-panel")
        .default_width(ctx.available_rect().width() * 0.5)
        .show(ctx, |ui| {
            add_filter_view(ui, pot_unit, "Instrument", PotFilterItemKind::NksBank);
            add_filter_view(ui, pot_unit, "Bank", PotFilterItemKind::NksSubBank);
            add_filter_view(ui, pot_unit, "Type", PotFilterItemKind::NksCategory);
            add_filter_view(ui, pot_unit, "Sub type", PotFilterItemKind::NksSubCategory);
            add_filter_view(ui, pot_unit, "Character", PotFilterItemKind::NksMode);
        });
    let preset_count = pot_unit.count_presets();
    CentralPanel::default().show(ctx, |ui: &mut Ui| {
        ui.horizontal(|ui: &mut Ui| {
            let response = ui.text_edit_singleline(pot_unit.runtime_state.search_expression_mut());
            if response.changed() {
                pot_unit.rebuild_collections();
            }
        });
        ui.horizontal(|ui: &mut Ui| {
            ui.strong("Count: ");
            ui.label(preset_count.to_string());
            ui.strong("Query time: ");
            ui.label(format!("{}ms", pot_unit.stats.query_duration.as_millis()));
        });
        let text_height = egui::TextStyle::Body.resolve(ui.style()).size;
        let mut table = TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Size::initial(60.0).at_least(40.0))
            .column(Size::initial(60.0).at_least(40.0))
            .column(Size::remainder().at_least(60.0));
        table
            .header(20.0, |mut header| {
                header.col(|ui| {
                    ui.strong("Preset ID");
                });
                header.col(|ui| {
                    ui.strong("Expanding content");
                });
                header.col(|ui| {
                    ui.strong("Clipped text");
                });
            })
            .body(|mut body: TableBody| {
                body.rows(
                    text_height,
                    preset_count as usize,
                    |row_index, mut row: TableRow| {
                        let preset_id = pot_unit.find_preset_id_at_index(row_index as u32).unwrap();
                        let preset: Preset =
                            with_preset_db(|db| db.find_preset_by_id(preset_id).unwrap()).unwrap();
                        row.col(|ui| {
                            ui.label(&preset.name);
                        });
                        row.col(|ui| {
                            ui.label(&preset.file_ext);
                        });
                        row.col(|ui: &mut Ui| {
                            if ui.button("Load").clicked() {
                                let Some(focused_fx) = Reaper::get().focused_fx() else {
                                    return;
                                };
                                let _ = pot::load_preset(&preset, &focused_fx.fx);
                            };
                        });
                    },
                );
            })
    });
}

#[derive(Debug)]
pub struct State {
    pot_unit: PotUnit,
}

impl State {
    pub fn new() -> Self {
        Self {
            pot_unit: Default::default(),
        }
    }
}

fn add_filter_view(
    ui: &mut Ui,
    pot_unit: &mut RuntimePotUnit,
    label: &str,
    kind: PotFilterItemKind,
) {
    ui.strong(label);
    ui.horizontal_wrapped(|ui: &mut Ui| {
        let initial_filter_item_id = pot_unit.filter_item_id(kind);
        let filter_item_id = pot_unit.runtime_state.filter_item_id_mut(kind);
        ui.selectable_value(filter_item_id, None, "<All>");
        for filter_item in pot_unit.collections.find_all_filter_items(kind) {
            ui.selectable_value(
                filter_item_id,
                Some(filter_item.id),
                filter_item.effective_leaf_name(),
            );
        }
        if filter_item_id != &initial_filter_item_id {
            pot_unit.rebuild_collections();
        }
    });
}
