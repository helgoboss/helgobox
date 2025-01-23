use enumflags2::BitFlags;
use reaper_high::{FxChainContext, Reaper};
use reaper_medium::{EnumPitchShiftModesResult, PlayState, ReaperStr, ReaperString};

use helgobox_api::runtime::{ControllerPreset, LicenseInfo, MainPreset, ValidatedLicense};

use crate::application::UnitModel;
use crate::domain::CompartmentKind;
use crate::infrastructure::data::{
    ControllerManager, FileBasedControllerPresetManager, FileBasedMainPresetManager, LicenseManager,
};
use crate::infrastructure::plugin::{BackboneShell, InstanceShell};
use crate::infrastructure::proto::{
    event_reply, fx_chain_location_info, occasional_global_update, occasional_instance_update,
    qualified_occasional_unit_update, ArrangementPlayState, AudioInputChannel, AudioInputChannels,
    CellAddress, Compartment, ContinuousColumnUpdate, ContinuousMatrixUpdate, Empty,
    FxChainLocationInfo, FxLocationInfo, GetContinuousColumnUpdatesReply,
    GetContinuousMatrixUpdatesReply, GetContinuousSlotUpdatesReply, GetOccasionalClipUpdatesReply,
    GetOccasionalColumnUpdatesReply, GetOccasionalGlobalUpdatesReply,
    GetOccasionalInstanceUpdatesReply, GetOccasionalMatrixUpdatesReply,
    GetOccasionalPlaytimeEngineUpdatesReply, GetOccasionalRowUpdatesReply,
    GetOccasionalSlotUpdatesReply, GetOccasionalTrackUpdatesReply, GetOccasionalUnitUpdatesReply,
    HelgoboxInstance, HelgoboxInstanceData, HelgoboxInstances, HostColorScheme, MidiDeviceStatus,
    MidiInputDevice, MidiInputDevices, MidiOutputDevice, MidiOutputDevices, OccasionalGlobalUpdate,
    OccasionalInstanceUpdate, OccasionalMatrixUpdate, OccasionalPlaytimeEngineUpdate,
    PitchShiftMode, PitchShiftModes, PitchShiftSubMode, ProjectLocationInfo,
    QualifiedContinuousSlotUpdate, QualifiedOccasionalClipUpdate, QualifiedOccasionalColumnUpdate,
    QualifiedOccasionalRowUpdate, QualifiedOccasionalSlotUpdate, QualifiedOccasionalTrackUpdate,
    QualifiedOccasionalUnitUpdate, ResampleMode, ResampleModes, RgbColor, Scope, Severity,
    SlotAddress, TrackFxChainLocationInfo, TrackLocationInfo, Unit, Units, Warning, Warnings,
};
use crate::infrastructure::server::data::get_controller_routing;

impl occasional_instance_update::Update {
    pub fn info_event(event: helgobox_api::runtime::InstanceInfoEvent) -> Self {
        let json =
            serde_json::to_string(&event).expect("couldn't represent instance info event as JSON");
        Self::InfoEvent(json)
    }

    pub fn settings(instance_shell: &InstanceShell) -> Self {
        let settings = instance_shell.settings();
        let json =
            serde_json::to_string(&settings).expect("couldn't represent instance settings as JSON");
        Self::Settings(json)
    }

    pub fn units(instance_shell: &InstanceShell) -> Self {
        let units = instance_shell.all_unit_models().map(|unit_model| {
            let unit_model = unit_model.borrow();
            Unit {
                id: unit_model.unit_id().into(),
                key: unit_model.unit_key().to_string(),
                name: unit_model.name().map(|n| n.to_string()),
            }
        });
        Self::Units(Units {
            units: units.collect(),
        })
    }

    pub fn warning(severity: Severity, scope: Option<Scope>, message: String) -> Warning {
        Warning {
            severity: severity.into(),
            scope: scope.map(|s| s.into()),
            message,
        }
    }

    pub fn warnings(warnings: Vec<Warning>) -> Self {
        Self::Warnings(Warnings { warnings })
    }
}

impl qualified_occasional_unit_update::Update {
    pub fn controller_routing(unit_model: &UnitModel) -> Self {
        let controller_routing = get_controller_routing(unit_model);
        let json = serde_json::to_string(&controller_routing)
            .expect("couldn't represent controller routing as JSON");
        Self::ControllerRouting(json)
    }
}

impl RgbColor {
    pub fn from_engine(color: Option<reaper_medium::RgbColor>) -> Self {
        Self {
            color: color
                .map(|c| (((c.r as u32) << 16) + ((c.g as u32) << 8) + (c.b as u32)) as i32),
        }
    }

    pub fn to_engine(&self) -> Option<reaper_medium::RgbColor> {
        let c = self.color?;
        let dest = reaper_medium::RgbColor {
            r: ((c >> 16) & 0xFF) as u8,
            g: ((c >> 8) & 0xFF) as u8,
            b: (c & 0xFF) as u8,
        };
        Some(dest)
    }
}

impl occasional_global_update::Update {
    pub fn host_color_scheme() -> Self {
        let reaper = Reaper::get().medium_reaper();
        let colors = REAPER_COLOR_KEYS
            .iter()
            .filter_map(|key| {
                let native_color = reaper.get_theme_color(*key, BitFlags::empty()).ok()?;
                let rgb_color = reaper.color_from_native(native_color);
                Some((key.to_string(), RgbColor::from_engine(Some(rgb_color))))
            })
            .collect();
        Self::HostColorScheme(HostColorScheme { colors })
    }

    pub fn arrangement_play_state(play_state: PlayState) -> Self {
        Self::ArrangementPlayState(ArrangementPlayState::from_engine(play_state).into())
    }

    pub fn info_event(event: helgobox_api::runtime::GlobalInfoEvent) -> Self {
        let json =
            serde_json::to_string(&event).expect("couldn't represent global info event as JSON");
        Self::InfoEvent(json)
    }

    pub fn instances() -> Self {
        Self::Instances(HelgoboxInstances::discover())
    }

    pub fn midi_input_devices() -> Self {
        Self::MidiInputDevices(MidiInputDevices::from_engine(
            Reaper::get()
                .midi_input_devices()
                .filter(|d| d.is_available()),
        ))
    }

    pub fn midi_output_devices() -> Self {
        Self::MidiOutputDevices(MidiOutputDevices::from_engine(
            Reaper::get()
                .midi_output_devices()
                .filter(|d| d.is_available()),
        ))
    }

    pub fn audio_input_channels() -> Self {
        Self::AudioInputChannels(AudioInputChannels::from_engine(
            Reaper::get().input_channels(),
        ))
    }

    pub fn resample_modes() -> Self {
        Self::ResampleModes(ResampleModes::from_engine(Reaper::get().resample_modes()))
    }

    pub fn pitch_shift_modes() -> Self {
        Self::PitchShiftModes(PitchShiftModes::from_engine(
            Reaper::get().pitch_shift_modes(),
        ))
    }

    pub fn controller_presets(manager: &FileBasedControllerPresetManager) -> Self {
        let api_presets: Vec<_> = manager
            .preset_infos()
            .iter()
            .map(|info| ControllerPreset {
                id: info.common.id.clone(),
                common: info.common.meta_data.clone(),
                specific: info.specific_meta_data.clone(),
            })
            .collect();
        let json = serde_json::to_string(&api_presets)
            .expect("couldn't represent controller presets as JSON");
        Self::ControllerPresets(json)
    }

    pub fn main_presets(manager: &FileBasedMainPresetManager) -> Self {
        let api_presets: Vec<_> = manager
            .preset_infos()
            .iter()
            .map(|info| MainPreset {
                id: info.common.id.clone(),
                common: info.common.meta_data.clone(),
                specific: info.specific_meta_data.clone(),
            })
            .collect();
        let json =
            serde_json::to_string(&api_presets).expect("couldn't represent main presets as JSON");
        Self::MainPresets(json)
    }

    pub fn controller_config(manager: &ControllerManager) -> Self {
        let json = serde_json::to_string(manager.controller_config())
            .expect("couldn't represent controller config as JSON");
        Self::ControllerConfig(json)
    }

    pub fn license_info(license_manager: &LicenseManager) -> Self {
        let license_info = LicenseInfo {
            licenses: license_manager
                .licenses()
                .iter()
                .map(|license| {
                    let valid = {
                        #[cfg(not(feature = "playtime"))]
                        {
                            false
                        }
                        #[cfg(feature = "playtime")]
                        {
                            playtime_clip_engine::PlaytimeMainEngine::validate_license(license)
                                .is_ok()
                        }
                    };
                    ValidatedLicense {
                        license: license.clone().into(),
                        valid,
                    }
                })
                .collect(),
        };
        let json =
            serde_json::to_string(&license_info).expect("couldn't represent license info as JSON");
        Self::LicenseInfo(json)
    }
}

impl HelgoboxInstances {
    pub fn discover() -> Self {
        Self {
            instances: BackboneShell::get().with_instance_shell_infos(|infos| {
                infos
                    .iter()
                    .filter_map(|info| {
                        let instance_shell = info.instance_shell.upgrade()?;
                        let main_unit_model = instance_shell.main_unit_shell().model().clone();
                        let main_unit_model = main_unit_model.borrow();
                        let data = HelgoboxInstanceData {
                            id: info.instance_id.into(),
                            key: main_unit_model.unit_key().to_string(),
                            tags: main_unit_model
                                .tags
                                .get_ref()
                                .iter()
                                .map(|t| t.to_string())
                                .collect(),
                        };
                        let fx = info.processor_context.containing_fx();
                        let fx_chain_location = match fx.chain().context() {
                            FxChainContext::Monitoring => {
                                fx_chain_location_info::Location::MonitoringFx(Empty {})
                            }
                            FxChainContext::Track { track, is_input_fx } => {
                                let project = track.project();
                                let project_location = ProjectLocationInfo {
                                    index: project.index().ok()?,
                                    path: project.file().map(|f| f.into_string()),
                                };
                                let track_location = TrackLocationInfo {
                                    project: Some(project_location),
                                    id: track.guid().to_string_without_braces(),
                                    index: track.index()?,
                                    name: track.name()?.to_string(),
                                };
                                fx_chain_location_info::Location::TrackFx(
                                    TrackFxChainLocationInfo {
                                        track: Some(track_location),
                                        input_fx: *is_input_fx,
                                    },
                                )
                            }
                            // Not supported anyway
                            FxChainContext::Take(_) => return None,
                        };
                        let fx_chain_location = FxChainLocationInfo {
                            location: Some(fx_chain_location),
                        };
                        let fx_location = FxLocationInfo {
                            fx_chain: Some(fx_chain_location),
                            id: fx.get_or_query_guid().ok()?.to_string_without_braces(),
                            index: fx.index(),
                            name: fx.name().to_string(),
                        };
                        let helgobox_instance = HelgoboxInstance {
                            data: Some(data),
                            fx: Some(fx_location),
                        };
                        Some(helgobox_instance)
                    })
                    .collect()
            }),
        }
    }
}

impl MidiInputDevices {
    pub fn from_engine(devs: impl Iterator<Item = reaper_high::MidiInputDevice>) -> Self {
        Self {
            devices: devs.map(MidiInputDevice::from_engine).collect(),
        }
    }
}

impl Compartment {
    pub fn to_engine(self) -> CompartmentKind {
        match self {
            Compartment::Controller => CompartmentKind::Controller,
            Compartment::Main => CompartmentKind::Main,
        }
    }
}

impl MidiInputDevice {
    pub fn from_engine(dev: reaper_high::MidiInputDevice) -> Self {
        MidiInputDevice {
            id: dev.id().get() as _,
            name: dev
                .name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            status: MidiDeviceStatus::from_engine(dev.is_open(), dev.is_connected()).into(),
        }
    }
}

impl MidiOutputDevices {
    pub fn from_engine(devs: impl Iterator<Item = reaper_high::MidiOutputDevice>) -> Self {
        Self {
            devices: devs.map(MidiOutputDevice::from_engine).collect(),
        }
    }
}

impl MidiOutputDevice {
    pub fn from_engine(dev: reaper_high::MidiOutputDevice) -> Self {
        MidiOutputDevice {
            id: dev.id().get() as _,
            name: dev
                .name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            status: MidiDeviceStatus::from_engine(dev.is_open(), dev.is_connected()).into(),
        }
    }
}

impl MidiDeviceStatus {
    pub fn from_engine(open: bool, connected: bool) -> Self {
        use MidiDeviceStatus::*;
        match (open, connected) {
            (false, false) => Disconnected,
            (false, true) => ConnectedButDisabled,
            // Shouldn't happen but cope with it.
            (true, false) => Disconnected,
            (true, true) => Connected,
        }
    }
}

impl AudioInputChannels {
    pub fn from_engine(channels: impl Iterator<Item = String>) -> Self {
        Self {
            channels: channels
                .enumerate()
                .map(|(i, name)| AudioInputChannel {
                    index: i as u32,
                    name,
                })
                .collect(),
        }
    }
}

impl ResampleModes {
    pub fn from_engine(modes: impl Iterator<Item = &'static ReaperStr>) -> Self {
        Self {
            modes: modes
                .enumerate()
                .map(|(i, name)| ResampleMode {
                    index: i as u32,
                    name: name.to_string(),
                })
                .collect(),
        }
    }
}

impl PitchShiftModes {
    pub fn from_engine(modes: impl Iterator<Item = EnumPitchShiftModesResult<'static>>) -> Self {
        Self {
            modes: modes
                .enumerate()
                .filter_map(|(i, res)| match res {
                    EnumPitchShiftModesResult::Unsupported => None,
                    EnumPitchShiftModesResult::Supported { name } => {
                        Some(PitchShiftMode::from_engine(
                            reaper_medium::PitchShiftMode::new(i as u32),
                            name,
                        ))
                    }
                })
                .collect(),
        }
    }
}

impl PitchShiftMode {
    pub fn from_engine(mode: reaper_medium::PitchShiftMode, name: &ReaperStr) -> Self {
        Self {
            index: mode.get(),
            name: name.to_string(),
            sub_modes: Reaper::get()
                .pitch_shift_sub_modes(mode)
                .enumerate()
                .map(|(i, name)| PitchShiftSubMode {
                    index: i as u32,
                    name: name.to_string(),
                })
                .collect(),
        }
    }
}

impl SlotAddress {
    pub fn from_engine(address: playtime_api::persistence::SlotAddress) -> Self {
        Self {
            column_index: address.column() as _,
            row_index: address.row() as _,
        }
    }

    pub fn to_engine(&self) -> playtime_api::persistence::SlotAddress {
        playtime_api::persistence::SlotAddress::new(self.column_index as _, self.row_index as _)
    }
}

impl CellAddress {
    pub fn from_engine(address: playtime_api::runtime::CellAddress) -> Self {
        Self {
            column_index: address.column_index.map(|i| i as _),
            row_index: address.row_index.map(|i| i as _),
        }
    }

    pub fn to_engine(&self) -> playtime_api::runtime::CellAddress {
        playtime_api::runtime::CellAddress::new(
            self.column_index.map(|i| i as _),
            self.row_index.map(|i| i as _),
        )
    }
}

impl From<Vec<OccasionalGlobalUpdate>> for event_reply::Value {
    fn from(value: Vec<OccasionalGlobalUpdate>) -> Self {
        event_reply::Value::OccasionalGlobalUpdatesReply(GetOccasionalGlobalUpdatesReply {
            global_updates: value,
        })
    }
}

impl From<Vec<OccasionalInstanceUpdate>> for event_reply::Value {
    fn from(value: Vec<OccasionalInstanceUpdate>) -> Self {
        event_reply::Value::OccasionalInstanceUpdatesReply(GetOccasionalInstanceUpdatesReply {
            instance_updates: value,
        })
    }
}

impl From<Vec<QualifiedOccasionalUnitUpdate>> for event_reply::Value {
    fn from(value: Vec<QualifiedOccasionalUnitUpdate>) -> Self {
        event_reply::Value::OccasionalUnitUpdatesReply(GetOccasionalUnitUpdatesReply {
            unit_updates: value,
        })
    }
}

impl From<Vec<OccasionalMatrixUpdate>> for event_reply::Value {
    fn from(value: Vec<OccasionalMatrixUpdate>) -> Self {
        event_reply::Value::OccasionalMatrixUpdatesReply(GetOccasionalMatrixUpdatesReply {
            matrix_updates: value,
        })
    }
}

impl From<Vec<QualifiedOccasionalTrackUpdate>> for event_reply::Value {
    fn from(value: Vec<QualifiedOccasionalTrackUpdate>) -> Self {
        event_reply::Value::OccasionalTrackUpdatesReply(GetOccasionalTrackUpdatesReply {
            track_updates: value,
        })
    }
}

impl From<Vec<QualifiedOccasionalColumnUpdate>> for event_reply::Value {
    fn from(value: Vec<QualifiedOccasionalColumnUpdate>) -> Self {
        event_reply::Value::OccasionalColumnUpdatesReply(GetOccasionalColumnUpdatesReply {
            column_updates: value,
        })
    }
}
impl From<Vec<QualifiedOccasionalRowUpdate>> for event_reply::Value {
    fn from(value: Vec<QualifiedOccasionalRowUpdate>) -> Self {
        event_reply::Value::OccasionalRowUpdatesReply(GetOccasionalRowUpdatesReply {
            row_updates: value,
        })
    }
}
impl From<Vec<QualifiedOccasionalSlotUpdate>> for event_reply::Value {
    fn from(value: Vec<QualifiedOccasionalSlotUpdate>) -> Self {
        event_reply::Value::OccasionalSlotUpdatesReply(GetOccasionalSlotUpdatesReply {
            slot_updates: value,
        })
    }
}

impl From<Vec<QualifiedOccasionalClipUpdate>> for event_reply::Value {
    fn from(value: Vec<QualifiedOccasionalClipUpdate>) -> Self {
        event_reply::Value::OccasionalClipUpdatesReply(GetOccasionalClipUpdatesReply {
            clip_updates: value,
        })
    }
}

impl From<Vec<OccasionalPlaytimeEngineUpdate>> for event_reply::Value {
    fn from(value: Vec<OccasionalPlaytimeEngineUpdate>) -> Self {
        event_reply::Value::OccasionalPlaytimeEngineUpdatesReply(
            GetOccasionalPlaytimeEngineUpdatesReply { updates: value },
        )
    }
}

impl From<ContinuousMatrixUpdate> for event_reply::Value {
    fn from(value: ContinuousMatrixUpdate) -> Self {
        event_reply::Value::ContinuousMatrixUpdatesReply(GetContinuousMatrixUpdatesReply {
            matrix_update: Some(value),
        })
    }
}

impl From<Vec<ContinuousColumnUpdate>> for event_reply::Value {
    fn from(value: Vec<ContinuousColumnUpdate>) -> Self {
        event_reply::Value::ContinuousColumnUpdatesReply(GetContinuousColumnUpdatesReply {
            column_updates: value,
        })
    }
}

impl From<Vec<QualifiedContinuousSlotUpdate>> for event_reply::Value {
    fn from(value: Vec<QualifiedContinuousSlotUpdate>) -> Self {
        event_reply::Value::ContinuousSlotUpdatesReply(GetContinuousSlotUpdatesReply {
            slot_updates: value,
        })
    }
}

impl ArrangementPlayState {
    pub fn from_engine(play_state: PlayState) -> Self {
        if play_state.is_recording {
            if play_state.is_paused {
                Self::RecordingPaused
            } else {
                Self::Recording
            }
        } else if play_state.is_playing {
            if play_state.is_paused {
                Self::PlayingPaused
            } else {
                Self::Playing
            }
        } else if play_state.is_paused {
            Self::PlayingPaused
        } else {
            Self::Stopped
        }
    }
}

const REAPER_COLOR_KEYS: &[&str] = &[
    "col_main_bg2",
    "col_main_text2",
    "col_main_textshadow",
    "col_main_3dhl",
    "col_main_3dsh",
    "col_main_resize2",
    "col_main_text",
    "col_main_bg",
    "col_main_editbk",
    "col_nodarkmodemiscwnd",
    "col_transport_editbk",
    "col_toolbar_text",
    "col_toolbar_text_on",
    "col_toolbar_frame",
    "toolbararmed_color",
    "toolbararmed_drawmode",
    "io_text",
    "io_3dhl",
    "io_3dsh",
    "genlist_bg",
    "genlist_fg",
    "genlist_grid",
    "genlist_selbg",
    "genlist_selfg",
    "genlist_seliabg",
    "genlist_seliafg",
    "genlist_hilite",
    "genlist_hilite_sel",
    "col_buttonbg",
    "col_tcp_text",
    "col_tcp_textsel",
    "col_seltrack",
    "col_seltrack2",
    "tcplocked_color",
    "tcplocked_drawmode",
    "col_tracklistbg",
    "col_mixerbg",
    "col_arrangebg",
    "arrange_vgrid",
    "col_fadearm",
    "col_fadearm2",
    "col_fadearm3",
    "col_tl_fg",
    "col_tl_fg2",
    "col_tl_bg",
    "col_tl_bgsel",
    "timesel_drawmode",
    "col_tl_bgsel2",
    "col_trans_bg",
    "col_trans_fg",
    "playrate_edited",
    "selitem_dot",
    "col_mi_label",
    "col_mi_label_sel",
    "col_mi_label_float",
    "col_mi_label_float_sel",
    "col_mi_bg",
    "col_mi_bg2",
    "col_tr1_itembgsel",
    "col_tr2_itembgsel",
    "itembg_drawmode",
    "col_tr1_peaks",
    "col_tr2_peaks",
    "col_tr1_ps2",
    "col_tr2_ps2",
    "col_peaksedge",
    "col_peaksedge2",
    "col_peaksedgesel",
    "col_peaksedgesel2",
    "cc_chase_drawmode",
    "col_peaksfade",
    "col_peaksfade2",
    "col_mi_fades",
    "fadezone_color",
    "fadezone_drawmode",
    "fadearea_color",
    "fadearea_drawmode",
    "col_mi_fade2",
    "col_mi_fade2_drawmode",
    "item_grouphl",
    "col_offlinetext",
    "col_stretchmarker",
    "col_stretchmarker_h0",
    "col_stretchmarker_h1",
    "col_stretchmarker_h2",
    "col_stretchmarker_b",
    "col_stretchmarkerm",
    "col_stretchmarker_text",
    "col_stretchmarker_tm",
    "take_marker",
    "selitem_tag",
    "activetake_tag",
    "col_tr1_bg",
    "col_tr2_bg",
    "selcol_tr1_bg",
    "selcol_tr2_bg",
    "track_lane_tabcol",
    "track_lanesolo_tabcol",
    "track_lanesolo_text",
    "track_lane_gutter",
    "track_lane_gutter_drawmode",
    "col_tr1_divline",
    "col_tr2_divline",
    "col_envlane1_divline",
    "col_envlane2_divline",
    "mute_overlay_col",
    "mute_overlay_mode",
    "inactive_take_overlay_col",
    "inactive_take_overlay_mode",
    "locked_overlay_col",
    "locked_overlay_mode",
    "marquee_fill",
    "marquee_drawmode",
    "marquee_outline",
    "marqueezoom_fill",
    "marqueezoom_drawmode",
    "marqueezoom_outline",
    "areasel_fill",
    "areasel_drawmode",
    "areasel_outline",
    "areasel_outlinemode",
    "linkedlane_fill",
    "linkedlane_fillmode",
    "linkedlane_outline",
    "linkedlane_outlinemode",
    "linkedlane_unsynced",
    "linkedlane_unsynced_mode",
    "col_cursor",
    "col_cursor2",
    "playcursor_color",
    "playcursor_drawmode",
    "col_gridlines2",
    "col_gridlines2dm",
    "col_gridlines3",
    "col_gridlines3dm",
    "col_gridlines",
    "col_gridlines1dm",
    "guideline_color",
    "guideline_drawmode",
    "region",
    "region_lane_bg",
    "region_lane_text",
    "marker",
    "marker_lane_bg",
    "marker_lane_text",
    "col_tsigmark",
    "ts_lane_bg",
    "ts_lane_text",
    "timesig_sel_bg",
    "col_routinghl1",
    "col_routinghl2",
    "col_routingact",
    "col_vudoint",
    "col_vuclip",
    "col_vutop",
    "col_vumid",
    "col_vubot",
    "col_vuintcol",
    "vu_gr_bgcol",
    "vu_gr_fgcol",
    "col_vumidi",
    "col_vuind1",
    "col_vuind2",
    "col_vuind3",
    "col_vuind4",
    "mcp_sends_normal",
    "mcp_sends_muted",
    "mcp_send_midihw",
    "mcp_sends_levels",
    "mcp_fx_normal",
    "mcp_fx_bypassed",
    "mcp_fx_offlined",
    "mcp_fxparm_normal",
    "mcp_fxparm_bypassed",
    "mcp_fxparm_offlined",
    "tcp_list_scrollbar",
    "tcp_list_scrollbar_mode",
    "tcp_list_scrollbar_mouseover",
    "tcp_list_scrollbar_mouseover_mode",
    "mcp_list_scrollbar",
    "mcp_list_scrollbar_mode",
    "mcp_list_scrollbar_mouseover",
    "mcp_list_scrollbar_mouseover_mode",
    "midi_rulerbg",
    "midi_rulerfg",
    "midi_grid2",
    "midi_griddm2",
    "midi_grid3",
    "midi_griddm3",
    "midi_grid1",
    "midi_griddm1",
    "midi_trackbg1",
    "midi_trackbg2",
    "midi_trackbg_outer1",
    "midi_trackbg_outer2",
    "midi_selpitch1",
    "midi_selpitch2",
    "midi_selbg",
    "midi_selbg_drawmode",
    "midi_gridhc",
    "midi_gridhcdm",
    "midi_gridh",
    "midi_gridhdm",
    "midi_ccbut",
    "midi_ccbut_text",
    "midi_ccbut_arrow",
    "midioct",
    "midi_inline_trackbg1",
    "midi_inline_trackbg2",
    "midioct_inline",
    "midi_endpt",
    "midi_notebg",
    "midi_notefg",
    "midi_notemute",
    "midi_notemute_sel",
    "midi_itemctl",
    "midi_ofsn",
    "midi_ofsnsel",
    "midi_editcurs",
    "midi_pkey1",
    "midi_pkey2",
    "midi_pkey3",
    "midi_noteon_flash",
    "midi_leftbg",
    "midifont_col_light_unsel",
    "midifont_col_dark_unsel",
    "midifont_mode_unsel",
    "midifont_col_light",
    "midifont_col_dark",
    "midifont_mode",
    "score_bg",
    "score_fg",
    "score_sel",
    "score_timesel",
    "score_loop",
    "midieditorlist_bg",
    "midieditorlist_fg",
    "midieditorlist_grid",
    "midieditorlist_selbg",
    "midieditorlist_selfg",
    "midieditorlist_seliabg",
    "midieditorlist_seliafg",
    "midieditorlist_bg2",
    "midieditorlist_fg2",
    "midieditorlist_selbg2",
    "midieditorlist_selfg2",
    "col_explorer_sel",
    "col_explorer_seldm",
    "col_explorer_seledge",
    "explorer_grid",
    "explorer_pitchtext",
    "docker_shadow",
    "docker_selface",
    "docker_unselface",
    "docker_text",
    "docker_text_sel",
    "docker_bg",
    "windowtab_bg",
    "auto_item_unsel",
    "col_env1",
    "col_env2",
    "env_trim_vol",
    "col_env3",
    "col_env4",
    "env_track_mute",
    "col_env5",
    "col_env6",
    "col_env7",
    "col_env8",
    "col_env9",
    "col_env10",
    "env_sends_mute",
    "col_env11",
    "col_env12",
    "col_env13",
    "col_env14",
    "col_env15",
    "col_env16",
    "env_item_vol",
    "env_item_pan",
    "env_item_mute",
    "env_item_pitch",
    "wiring_grid2",
    "wiring_grid",
    "wiring_border",
    "wiring_tbg",
    "wiring_ticon",
    "wiring_recbg",
    "wiring_recitem",
    "wiring_media",
    "wiring_recv",
    "wiring_send",
    "wiring_fader",
    "wiring_parent",
    "wiring_parentwire_border",
    "wiring_parentwire_master",
    "wiring_parentwire_folder",
    "wiring_pin_normal",
    "wiring_pin_connected",
    "wiring_pin_disconnected",
    "wiring_horz_col",
    "wiring_sendwire",
    "wiring_hwoutwire",
    "wiring_recinputwire",
    "wiring_hwout",
    "wiring_recinput",
    "wiring_activity",
    "autogroup",
    "group_0",
    "group_1",
    "group_2",
    "group_3",
    "group_4",
    "group_5",
    "group_6",
    "group_7",
    "group_8",
    "group_9",
    "group_10",
    "group_11",
    "group_12",
    "group_13",
    "group_14",
    "group_15",
    "group_16",
    "group_17",
    "group_18",
    "group_19",
    "group_20",
    "group_21",
    "group_22",
    "group_23",
    "group_24",
    "group_25",
    "group_26",
    "group_27",
    "group_28",
    "group_29",
    "group_30",
    "group_31",
    "group_32",
    "group_33",
    "group_34",
    "group_35",
    "group_36",
    "group_37",
    "group_38",
    "group_39",
    "group_40",
    "group_41",
    "group_42",
    "group_43",
    "group_44",
    "group_45",
    "group_46",
    "group_47",
    "group_48",
    "group_49",
    "group_50",
    "group_51",
    "group_52",
    "group_53",
    "group_54",
    "group_55",
    "group_56",
    "group_57",
    "group_58",
    "group_59",
    "group_60",
    "group_61",
    "group_62",
    "group_63",
];
