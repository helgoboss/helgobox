use anyhow::Context;
use helgoboss_license_api::persistence::LicenseKey;
use reaper_high::{OrCurrentProject, Reaper};
use reaper_medium::CommandId;
use tonic::{Response, Status};

use base::spawn_in_main_thread;
use helgobox_api::runtime::{GlobalInfoEvent, InstanceInfoEvent};
use swell_ui::Window;

use crate::domain::{CompartmentKind, UnitId};
use crate::infrastructure::api::convert::from_data;
use crate::infrastructure::api::convert::from_data::ConversionStyle;
use crate::infrastructure::data::CompartmentModelData;
use crate::infrastructure::plugin::{BackboneShell, InstanceShell, UnitShell};
#[cfg(feature = "playtime")]
use crate::infrastructure::proto::PlaytimeProtoRequestHandler;
use crate::infrastructure::proto::{
    AddLicenseRequest, Compartment, DeleteControllerRequest, DragClipRequest, DragColumnRequest,
    DragRowRequest, DragSlotRequest, Empty, FullCompartmentId, GetAppSettingsReply,
    GetAppSettingsRequest, GetArrangementInfoReply, GetArrangementInfoRequest, GetClipDetailReply,
    GetClipDetailRequest, GetCompartmentDataReply, GetCompartmentDataRequest,
    GetCustomInstanceDataReply, GetCustomInstanceDataRequest, GetHostInfoReply, GetHostInfoRequest,
    GetProjectDirReply, GetProjectDirRequest, ImportFilesRequest, InsertColumnsRequest,
    OpenTrackFxRequest, ProveAuthenticityReply, ProveAuthenticityRequest, SaveControllerRequest,
    SaveCustomCompartmentDataRequest, SetAppSettingsRequest, SetClipDataRequest,
    SetClipNameRequest, SetColumnSettingsRequest, SetColumnTrackRequest,
    SetCustomInstanceDataRequest, SetInstanceSettingsRequest, SetMatrixPanRequest,
    SetMatrixPlayRateRequest, SetMatrixSettingsRequest, SetMatrixTempoRequest,
    SetMatrixTimeSignatureRequest, SetMatrixVolumeRequest, SetPlaytimeEngineSettingsRequest,
    SetRowDataRequest, SetSequenceInfoRequest, SetTrackColorRequest,
    SetTrackInputMonitoringRequest, SetTrackInputRequest, SetTrackNameRequest, SetTrackPanRequest,
    SetTrackVolumeRequest, TriggerClipRequest, TriggerColumnRequest, TriggerGlobalAction,
    TriggerGlobalRequest, TriggerInstanceAction, TriggerInstanceRequest, TriggerMatrixRequest,
    TriggerRowRequest, TriggerSequenceRequest, TriggerSlotRequest, TriggerTrackRequest,
    HOST_API_VERSION,
};

#[derive(Debug)]
pub struct ProtoRequestHandler;

impl ProtoRequestHandler {
    pub fn trigger_slot(&self, req: TriggerSlotRequest) -> Result<Response<Empty>, Status> {
        #[cfg(not(feature = "playtime"))]
        {
            let _ = req;
            playtime_not_available()
        }
        #[cfg(feature = "playtime")]
        {
            PlaytimeProtoRequestHandler.trigger_slot(req)
        }
    }

    pub fn import_files(&self, req: ImportFilesRequest) -> Result<Response<Empty>, Status> {
        #[cfg(not(feature = "playtime"))]
        {
            let _ = req;
            playtime_not_available()
        }
        #[cfg(feature = "playtime")]
        {
            PlaytimeProtoRequestHandler.import_files(req)
        }
    }

    pub fn trigger_clip(&self, req: TriggerClipRequest) -> Result<Response<Empty>, Status> {
        #[cfg(not(feature = "playtime"))]
        {
            let _ = req;
            playtime_not_available()
        }
        #[cfg(feature = "playtime")]
        {
            PlaytimeProtoRequestHandler.trigger_clip(req)
        }
    }

    pub fn drag_slot(&self, req: DragSlotRequest) -> Result<Response<Empty>, Status> {
        #[cfg(not(feature = "playtime"))]
        {
            let _ = req;
            playtime_not_available()
        }
        #[cfg(feature = "playtime")]
        {
            PlaytimeProtoRequestHandler.drag_slot(req)
        }
    }

    pub fn drag_clip(&self, req: DragClipRequest) -> Result<Response<Empty>, Status> {
        #[cfg(not(feature = "playtime"))]
        {
            let _ = req;
            playtime_not_available()
        }
        #[cfg(feature = "playtime")]
        {
            PlaytimeProtoRequestHandler.drag_clip(req)
        }
    }

    pub fn drag_row(&self, req: DragRowRequest) -> Result<Response<Empty>, Status> {
        #[cfg(not(feature = "playtime"))]
        {
            let _ = req;
            playtime_not_available()
        }
        #[cfg(feature = "playtime")]
        {
            PlaytimeProtoRequestHandler.drag_row(req)
        }
    }

    pub fn drag_column(&self, req: DragColumnRequest) -> Result<Response<Empty>, Status> {
        #[cfg(not(feature = "playtime"))]
        {
            let _ = req;
            playtime_not_available()
        }
        #[cfg(feature = "playtime")]
        {
            PlaytimeProtoRequestHandler.drag_column(req)
        }
    }

    pub fn set_track_name(&self, req: SetTrackNameRequest) -> Result<Response<Empty>, Status> {
        #[cfg(not(feature = "playtime"))]
        {
            let _ = req;
            playtime_not_available()
        }
        #[cfg(feature = "playtime")]
        {
            PlaytimeProtoRequestHandler.set_track_name(req)
        }
    }

    pub fn set_track_color(&self, req: SetTrackColorRequest) -> Result<Response<Empty>, Status> {
        #[cfg(not(feature = "playtime"))]
        {
            let _ = req;
            playtime_not_available()
        }
        #[cfg(feature = "playtime")]
        {
            PlaytimeProtoRequestHandler.set_track_color(req)
        }
    }

    pub fn set_clip_name(&self, req: SetClipNameRequest) -> Result<Response<Empty>, Status> {
        #[cfg(not(feature = "playtime"))]
        {
            let _ = req;
            playtime_not_available()
        }
        #[cfg(feature = "playtime")]
        {
            PlaytimeProtoRequestHandler.set_clip_name(req)
        }
    }

    pub fn set_clip_data(&self, req: SetClipDataRequest) -> Result<Response<Empty>, Status> {
        #[cfg(not(feature = "playtime"))]
        {
            let _ = req;
            playtime_not_available()
        }
        #[cfg(feature = "playtime")]
        {
            PlaytimeProtoRequestHandler.set_clip_data(req)
        }
    }

    pub fn trigger_sequence(&self, req: TriggerSequenceRequest) -> Result<Response<Empty>, Status> {
        #[cfg(not(feature = "playtime"))]
        {
            let _ = req;
            playtime_not_available()
        }
        #[cfg(feature = "playtime")]
        {
            PlaytimeProtoRequestHandler.trigger_sequence(req)
        }
    }

    pub fn set_sequence_info(
        &self,
        req: SetSequenceInfoRequest,
    ) -> Result<Response<Empty>, Status> {
        #[cfg(not(feature = "playtime"))]
        {
            let _ = req;
            playtime_not_available()
        }
        #[cfg(feature = "playtime")]
        {
            PlaytimeProtoRequestHandler.set_sequence_info(req)
        }
    }

    pub fn add_license(&self, req: AddLicenseRequest) -> Result<Response<Empty>, Status> {
        let license_key = LicenseKey::new(req.license_key.trim().to_string());
        BackboneShell::get()
            .license_manager()
            .borrow_mut()
            .add_license(license_key)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;
        Ok(Response::new(Empty {}))
    }

    pub fn save_controller(&self, req: SaveControllerRequest) -> Result<Response<Empty>, Status> {
        let controller = serde_json::from_str(&req.controller)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;
        let outcome = BackboneShell::get()
            .controller_manager()
            .borrow_mut()
            .save_controller(controller)
            .map_err(|e| Status::unknown(e.to_string()))?;
        if outcome.connection_changed {
            if let Some(dev_id) = outcome.new_midi_output_device_id {
                spawn_in_main_thread(async move {
                    let reply = BackboneShell::get()
                        .request_midi_device_identity(dev_id, None)
                        .await;
                    let _ = BackboneShell::get()
                        .controller_manager()
                        .borrow_mut()
                        .update_controller_device_identity(&outcome.id, reply.ok());
                    Ok(())
                })
            }
        }
        Ok(Response::new(Empty {}))
    }

    pub fn delete_controller(
        &self,
        req: DeleteControllerRequest,
    ) -> Result<Response<Empty>, Status> {
        BackboneShell::get()
            .controller_manager()
            .borrow_mut()
            .delete_controller(&req.controller_id)
            .map_err(|e| Status::unknown(e.to_string()))?;
        Ok(Response::new(Empty {}))
    }

    pub fn set_instance_settings(
        &self,
        req: SetInstanceSettingsRequest,
    ) -> Result<Response<Empty>, Status> {
        let settings = serde_json::from_str(&req.settings)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;
        self.handle_instance_command(req.instance_id, |instance_shell| {
            instance_shell.change_settings(|current_settings| *current_settings = settings);
            Ok(())
        })
    }

    pub fn set_playtime_engine_settings(
        &self,
        req: SetPlaytimeEngineSettingsRequest,
    ) -> Result<Response<Empty>, Status> {
        #[cfg(not(feature = "playtime"))]
        {
            let _ = req;
            playtime_not_available()
        }
        #[cfg(feature = "playtime")]
        {
            PlaytimeProtoRequestHandler.set_playtime_engine_settings(req)
        }
    }

    pub fn insert_columns(&self, req: InsertColumnsRequest) -> Result<Response<Empty>, Status> {
        #[cfg(not(feature = "playtime"))]
        {
            let _ = req;
            playtime_not_available()
        }
        #[cfg(feature = "playtime")]
        {
            PlaytimeProtoRequestHandler.insert_columns(req)
        }
    }

    pub fn trigger_global(&self, req: TriggerGlobalRequest) -> Result<Response<Empty>, Status> {
        let action = TriggerGlobalAction::try_from(req.action)
            .map_err(|_| Status::invalid_argument("unknown trigger global action"))?;
        match action {
            TriggerGlobalAction::FocusHost => {
                Window::from_hwnd(Reaper::get().main_window()).focus();
            }
            TriggerGlobalAction::MidiPanic => {
                let _ = Reaper::get()
                    .main_section()
                    .action_by_command_id(CommandId::new(40345))
                    .invoke_as_trigger(None, None);
                BackboneShell::get()
                    .proto_hub()
                    .notify_about_global_info_event(GlobalInfoEvent::generic(
                        "All MIDI outputs and plug-ins have been instructed to shut up.",
                    ));
            }
        }
        Ok(Response::new(Empty {}))
    }

    pub fn trigger_matrix(&self, req: TriggerMatrixRequest) -> Result<Response<Empty>, Status> {
        #[cfg(not(feature = "playtime"))]
        {
            let _ = req;
            playtime_not_available()
        }
        #[cfg(feature = "playtime")]
        {
            PlaytimeProtoRequestHandler.trigger_matrix(req)
        }
    }

    pub fn trigger_instance(&self, req: TriggerInstanceRequest) -> Result<Response<Empty>, Status> {
        let action = TriggerInstanceAction::try_from(req.action)
            .map_err(|_| Status::invalid_argument("unknown trigger instance action"))?;
        self.handle_instance_command(req.instance_id, |instance| {
            let project = instance.processor_context().project().or_current_project();
            match action {
                TriggerInstanceAction::ShowHelgoboxPlugin => {
                    instance
                        .processor_context()
                        .containing_fx()
                        .show_in_floating_window()?;
                }
                TriggerInstanceAction::CloseApp => {
                    instance.panel().stop_app_instance();
                }
                TriggerInstanceAction::HideApp => {
                    instance.panel().hide_app_instance();
                }
                TriggerInstanceAction::ArrangementTogglePlayStop => {
                    if project.is_playing() {
                        project.stop();
                    } else {
                        project.play();
                    }
                }
                TriggerInstanceAction::ArrangementPlay => {
                    project.play();
                }
                TriggerInstanceAction::ArrangementStop => {
                    project.stop();
                }
                TriggerInstanceAction::ArrangementPause => {
                    project.pause();
                }
                TriggerInstanceAction::ArrangementStartRecording => {
                    // Recording not supported per project
                    Reaper::get().enable_record_in_current_project();
                }
                TriggerInstanceAction::ArrangementStopRecording => {
                    // Recording not supported per project
                    Reaper::get().disable_record_in_current_project();
                }
                TriggerInstanceAction::SaveProject => {
                    let save_project_command_id = CommandId::new(40026);
                    Reaper::get()
                        .main_section()
                        .action_by_command_id(save_project_command_id)
                        .invoke_as_trigger(Some(project), None)?;
                    BackboneShell::get()
                        .proto_hub()
                        .notify_about_instance_info_event(
                            instance.instance_id(),
                            InstanceInfoEvent::generic("Saved REAPER project"),
                        );
                }
            }
            Ok(())
        })
    }

    pub fn set_matrix_settings(
        &self,
        req: SetMatrixSettingsRequest,
    ) -> Result<Response<Empty>, Status> {
        #[cfg(not(feature = "playtime"))]
        {
            let _ = req;
            playtime_not_available()
        }
        #[cfg(feature = "playtime")]
        {
            PlaytimeProtoRequestHandler.set_matrix_settings(req)
        }
    }

    pub fn trigger_column(&self, req: TriggerColumnRequest) -> Result<Response<Empty>, Status> {
        #[cfg(not(feature = "playtime"))]
        {
            let _ = req;
            playtime_not_available()
        }
        #[cfg(feature = "playtime")]
        {
            PlaytimeProtoRequestHandler.trigger_column(req)
        }
    }

    pub fn trigger_track(&self, req: TriggerTrackRequest) -> Result<Response<Empty>, Status> {
        #[cfg(not(feature = "playtime"))]
        {
            let _ = req;
            playtime_not_available()
        }
        #[cfg(feature = "playtime")]
        {
            PlaytimeProtoRequestHandler.trigger_track(req)
        }
    }

    pub fn set_column_settings(
        &self,
        req: SetColumnSettingsRequest,
    ) -> Result<Response<Empty>, Status> {
        #[cfg(not(feature = "playtime"))]
        {
            let _ = req;
            playtime_not_available()
        }
        #[cfg(feature = "playtime")]
        {
            PlaytimeProtoRequestHandler.set_column_settings(req)
        }
    }

    pub fn trigger_row(&self, req: TriggerRowRequest) -> Result<Response<Empty>, Status> {
        #[cfg(not(feature = "playtime"))]
        {
            let _ = req;
            playtime_not_available()
        }
        #[cfg(feature = "playtime")]
        {
            PlaytimeProtoRequestHandler.trigger_row(req)
        }
    }

    pub fn set_row_data(&self, req: SetRowDataRequest) -> Result<Response<Empty>, Status> {
        #[cfg(not(feature = "playtime"))]
        {
            let _ = req;
            playtime_not_available()
        }
        #[cfg(feature = "playtime")]
        {
            PlaytimeProtoRequestHandler.set_row_data(req)
        }
    }

    pub fn set_matrix_tempo(&self, req: SetMatrixTempoRequest) -> Result<Response<Empty>, Status> {
        #[cfg(not(feature = "playtime"))]
        {
            let _ = req;
            playtime_not_available()
        }
        #[cfg(feature = "playtime")]
        {
            PlaytimeProtoRequestHandler.set_matrix_tempo(req)
        }
    }

    pub fn set_matrix_play_rate(
        &self,
        req: SetMatrixPlayRateRequest,
    ) -> Result<Response<Empty>, Status> {
        #[cfg(not(feature = "playtime"))]
        {
            let _ = req;
            playtime_not_available()
        }
        #[cfg(feature = "playtime")]
        {
            PlaytimeProtoRequestHandler.set_matrix_play_rate(req)
        }
    }

    pub fn set_matrix_time_signature(
        &self,
        req: SetMatrixTimeSignatureRequest,
    ) -> Result<Response<Empty>, Status> {
        #[cfg(not(feature = "playtime"))]
        {
            let _ = req;
            playtime_not_available()
        }
        #[cfg(feature = "playtime")]
        {
            PlaytimeProtoRequestHandler.set_matrix_time_signature(req)
        }
    }

    pub fn set_matrix_volume(
        &self,
        req: SetMatrixVolumeRequest,
    ) -> Result<Response<Empty>, Status> {
        #[cfg(not(feature = "playtime"))]
        {
            let _ = req;
            playtime_not_available()
        }
        #[cfg(feature = "playtime")]
        {
            PlaytimeProtoRequestHandler.set_matrix_volume(req)
        }
    }

    pub fn set_matrix_pan(&self, req: SetMatrixPanRequest) -> Result<Response<Empty>, Status> {
        #[cfg(not(feature = "playtime"))]
        {
            let _ = req;
            playtime_not_available()
        }
        #[cfg(feature = "playtime")]
        {
            PlaytimeProtoRequestHandler.set_matrix_pan(req)
        }
    }

    pub fn set_track_volume(&self, req: SetTrackVolumeRequest) -> Result<Response<Empty>, Status> {
        #[cfg(not(feature = "playtime"))]
        {
            let _ = req;
            playtime_not_available()
        }
        #[cfg(feature = "playtime")]
        {
            PlaytimeProtoRequestHandler.set_track_volume(req)
        }
    }

    pub fn set_track_pan(&self, req: SetTrackPanRequest) -> Result<Response<Empty>, Status> {
        #[cfg(not(feature = "playtime"))]
        {
            let _ = req;
            playtime_not_available()
        }
        #[cfg(feature = "playtime")]
        {
            PlaytimeProtoRequestHandler.set_track_pan(req)
        }
    }

    pub fn open_track_fx(&self, req: OpenTrackFxRequest) -> Result<Response<Empty>, Status> {
        #[cfg(not(feature = "playtime"))]
        {
            let _ = req;
            playtime_not_available()
        }
        #[cfg(feature = "playtime")]
        {
            PlaytimeProtoRequestHandler.open_track_fx(req)
        }
    }

    pub async fn set_column_track(
        &self,
        req: SetColumnTrackRequest,
    ) -> Result<Response<Empty>, Status> {
        #[cfg(not(feature = "playtime"))]
        {
            let _ = req;
            playtime_not_available()
        }
        #[cfg(feature = "playtime")]
        {
            PlaytimeProtoRequestHandler.set_column_track(req).await
        }
    }

    pub fn set_track_input_monitoring(
        &self,
        req: SetTrackInputMonitoringRequest,
    ) -> Result<Response<Empty>, Status> {
        #[cfg(not(feature = "playtime"))]
        {
            let _ = req;
            playtime_not_available()
        }
        #[cfg(feature = "playtime")]
        {
            PlaytimeProtoRequestHandler.set_track_input_monitoring(req)
        }
    }

    pub fn set_track_input(&self, req: SetTrackInputRequest) -> Result<Response<Empty>, Status> {
        #[cfg(not(feature = "playtime"))]
        {
            let _ = req;
            playtime_not_available()
        }
        #[cfg(feature = "playtime")]
        {
            PlaytimeProtoRequestHandler.set_track_input(req)
        }
    }

    pub async fn get_clip_detail(
        &self,
        req: GetClipDetailRequest,
    ) -> Result<Response<GetClipDetailReply>, Status> {
        #[cfg(not(feature = "playtime"))]
        {
            let _ = req;
            Err(playtime_not_available_status())
        }
        #[cfg(feature = "playtime")]
        {
            PlaytimeProtoRequestHandler.get_clip_detail(req).await
        }
    }

    pub async fn get_app_settings(
        &self,
        _req: GetAppSettingsRequest,
    ) -> Result<Response<GetAppSettingsReply>, Status> {
        Ok(Response::new(GetAppSettingsReply {
            app_settings: BackboneShell::read_app_settings(),
        }))
    }

    pub fn set_app_settings(&self, req: SetAppSettingsRequest) -> Result<Response<Empty>, Status> {
        BackboneShell::write_app_settings(req.app_settings)
            .map_err(|e| Status::unknown(e.to_string()))?;
        Ok(Response::new(Empty {}))
    }

    pub fn get_compartment_data(
        &self,
        request: GetCompartmentDataRequest,
    ) -> Result<Response<GetCompartmentDataReply>, Status> {
        self.handle_compartment_command_internal(
            &request.compartment_id,
            |unit_shell, compartment| {
                let unit_model = unit_shell.model().borrow();
                let compartment_model = unit_model.extract_compartment_model(compartment);
                let compartment_model_data = CompartmentModelData::from_model(&compartment_model);
                let compartment_mode_api = from_data::convert_compartment(
                    compartment_model_data,
                    ConversionStyle::IncludeDefaultValues,
                )?;
                let reply = GetCompartmentDataReply {
                    data: serde_json::to_string(&compartment_mode_api)?,
                };
                Ok(Response::new(reply))
            },
        )
    }

    pub fn save_custom_compartment_data(
        &self,
        request: SaveCustomCompartmentDataRequest,
    ) -> Result<Response<Empty>, Status> {
        let value: serde_json::Value = serde_json::from_str(&request.custom_data)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;
        self.handle_compartment_command_internal(
            &request.compartment_id,
            |unit_shell, compartment| {
                let unit_model = unit_shell.model().borrow();
                let unit = unit_model.unit();
                unit.borrow_mut().update_custom_compartment_data_key(
                    compartment,
                    request.custom_key,
                    value,
                );
                Ok(Response::new(Empty {}))
            },
        )
    }

    pub fn get_custom_instance_data(
        &self,
        request: GetCustomInstanceDataRequest,
    ) -> Result<Response<GetCustomInstanceDataReply>, Status> {
        self.handle_instance_command_internal(request.instance_id, |instance| {
            let instance = instance.instance().borrow();
            let data = instance.custom_data().get(&request.custom_key);
            let reply = GetCustomInstanceDataReply {
                data: if let Some(d) = data {
                    Some(serde_json::to_string(d)?)
                } else {
                    None
                },
            };
            Ok(Response::new(reply))
        })
    }

    pub fn set_custom_instance_data(
        &self,
        request: SetCustomInstanceDataRequest,
    ) -> Result<Response<Empty>, Status> {
        self.handle_instance_command(request.instance_id, |instance| {
            let mut instance = instance.instance().borrow_mut();
            let value = serde_json::from_str(&request.custom_data)?;
            instance.update_custom_data_key(request.custom_key, value);
            Ok(())
        })
    }

    pub async fn get_host_info(
        &self,
        _req: GetHostInfoRequest,
    ) -> Result<Response<GetHostInfoReply>, Status> {
        use crate::infrastructure::plugin::built_info::*;
        Ok(Response::new(GetHostInfoReply {
            public_version: PKG_VERSION.to_string(),
            api_version: HOST_API_VERSION.to_string(),
        }))
    }

    pub async fn prove_authenticity(
        &self,
        req: ProveAuthenticityRequest,
    ) -> Result<Response<ProveAuthenticityReply>, Status> {
        #[cfg(not(feature = "playtime"))]
        {
            let _ = req;
            Err(playtime_not_available_status())
        }
        #[cfg(feature = "playtime")]
        {
            PlaytimeProtoRequestHandler.prove_authenticity(req).await
        }
    }

    pub async fn get_project_dir(
        &self,
        req: GetProjectDirRequest,
    ) -> Result<Response<GetProjectDirReply>, Status> {
        #[cfg(not(feature = "playtime"))]
        {
            let _ = req;
            Err(playtime_not_available_status())
        }
        #[cfg(feature = "playtime")]
        {
            PlaytimeProtoRequestHandler.get_project_dir(req).await
        }
    }

    pub async fn get_arrangement_info(
        &self,
        req: GetArrangementInfoRequest,
    ) -> Result<Response<GetArrangementInfoReply>, Status> {
        #[cfg(not(feature = "playtime"))]
        {
            let _ = req;
            Err(playtime_not_available_status())
        }
        #[cfg(feature = "playtime")]
        {
            PlaytimeProtoRequestHandler.get_arrangement_info(req).await
        }
    }

    fn handle_instance_command(
        &self,
        instance_id: u32,
        handler: impl FnOnce(&InstanceShell) -> anyhow::Result<()>,
    ) -> Result<Response<Empty>, Status> {
        self.handle_instance_command_internal(instance_id, handler)?;
        Ok(Response::new(Empty {}))
    }

    fn handle_compartment_command_internal<R>(
        &self,
        full_compartment_id: &Option<FullCompartmentId>,
        handler: impl FnOnce(&UnitShell, CompartmentKind) -> anyhow::Result<R>,
    ) -> Result<R, Status> {
        let full_compartment_id = full_compartment_id
            .as_ref()
            .ok_or_else(|| Status::invalid_argument("need full compartment ID"))?;
        let compartment = Compartment::try_from(full_compartment_id.compartment)
            .map_err(|_| Status::invalid_argument("unknown compartment"))?;

        self.handle_unit_command_internal(
            full_compartment_id.instance_id,
            Some(full_compartment_id.unit_id),
            |_, unit_shell| handler(unit_shell, compartment.to_engine()),
        )
    }

    fn handle_unit_command_internal<R>(
        &self,
        instance_id: u32,
        unit_id: Option<u32>,
        handler: impl FnOnce(&InstanceShell, &UnitShell) -> anyhow::Result<R>,
    ) -> Result<R, Status> {
        self.handle_instance_command_internal(instance_id, |instance_shell| {
            let unit_id = unit_id.map(UnitId::from);
            instance_shell
                .find_unit_prop_by_id_simple(unit_id, |_, unit_shell| {
                    handler(instance_shell, unit_shell)
                })
                .context("Unit not found")?
        })
    }

    fn handle_instance_command_internal<R>(
        &self,
        instance_id: u32,
        handler: impl FnOnce(&InstanceShell) -> anyhow::Result<R>,
    ) -> Result<R, Status> {
        let instance_shell = BackboneShell::get()
            .get_instance_shell_by_instance_id(instance_id.into())
            .map_err(|e| Status::not_found(format!("{e:#}")))?;
        let r = handler(&instance_shell).map_err(|e| Status::unknown(format!("{e:#}")))?;
        Ok(r)
    }
}

#[cfg(not(feature = "playtime"))]
pub fn playtime_not_available() -> Result<Response<Empty>, Status> {
    Err(playtime_not_available_status())
}

#[cfg(not(feature = "playtime"))]
pub fn playtime_not_available_status() -> Status {
    Status::not_found("Playtime not available")
}
