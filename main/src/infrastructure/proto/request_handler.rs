use crate::infrastructure::plugin::{BackboneShell, InstanceShell};
use crate::infrastructure::proto::{
    DeleteControllerRequest, DragClipRequest, DragColumnRequest, DragRowRequest, DragSlotRequest,
    Empty, GetArrangementInfoReply, GetArrangementInfoRequest, GetClipDetailReply,
    GetClipDetailRequest, GetHostInfoReply, GetHostInfoRequest, GetProjectDirReply,
    GetProjectDirRequest, ImportFilesRequest, ProveAuthenticityReply, ProveAuthenticityRequest,
    SaveControllerRequest, SetClipDataRequest, SetClipNameRequest, SetColumnSettingsRequest,
    SetColumnTrackRequest, SetInstanceSettingsRequest, SetMatrixPanRequest,
    SetMatrixSettingsRequest, SetMatrixTempoRequest, SetMatrixTimeSignatureRequest,
    SetMatrixVolumeRequest, SetRowDataRequest, SetSequenceInfoRequest, SetTrackColorRequest,
    SetTrackInputMonitoringRequest, SetTrackInputRequest, SetTrackNameRequest, SetTrackPanRequest,
    SetTrackVolumeRequest, TriggerClipRequest, TriggerColumnRequest, TriggerMatrixRequest,
    TriggerRowRequest, TriggerSequenceRequest, TriggerSlotRequest, TriggerTrackRequest,
    HOST_API_VERSION,
};
use base::spawn_in_main_thread;

use tonic::{Response, Status};

#[cfg(feature = "playtime")]
use crate::infrastructure::proto::PlaytimeProtoRequestHandler;

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
        self.handle_instance_command(&req.instance_id, |instance_shell| {
            instance_shell.change_settings(|current_settings| *current_settings = settings);
            Ok(())
        })
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

    pub async fn get_host_info(
        &self,
        _req: GetHostInfoRequest,
    ) -> Result<Response<GetHostInfoReply>, Status> {
        Ok(Response::new(GetHostInfoReply {
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
        instance_id: &str,
        handler: impl FnOnce(&InstanceShell) -> anyhow::Result<()>,
    ) -> Result<Response<Empty>, Status> {
        self.handle_instance_internal(instance_id, handler)?;
        Ok(Response::new(Empty {}))
    }

    fn handle_instance_internal<R>(
        &self,
        instance_id: &str,
        handler: impl FnOnce(&InstanceShell) -> anyhow::Result<R>,
    ) -> Result<R, Status> {
        let instance_shell = BackboneShell::get()
            .find_instance_shell_by_instance_id_str(instance_id)
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
