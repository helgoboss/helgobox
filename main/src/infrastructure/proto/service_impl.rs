use crate::infrastructure::plugin::BackboneShell;
#[cfg(not(feature = "playtime"))]
use crate::infrastructure::proto::playtime_not_available_status;
use crate::infrastructure::proto::senders::{ProtoSenders, WithInstanceId};
use crate::infrastructure::proto::{
    create_initial_global_updates, create_initial_instance_updates, create_initial_unit_updates,
    helgobox_service_server, AddLicenseRequest, DeleteControllerRequest, DragClipRequest,
    DragColumnRequest, DragRowRequest, DragSlotRequest, Empty, GetAppSettingsReply,
    GetAppSettingsRequest, GetArrangementInfoReply, GetArrangementInfoRequest, GetClipDetailReply,
    GetClipDetailRequest, GetCompartmentDataReply, GetCompartmentDataRequest,
    GetContinuousColumnUpdatesReply, GetContinuousColumnUpdatesRequest,
    GetContinuousMatrixUpdatesReply, GetContinuousMatrixUpdatesRequest,
    GetContinuousSlotUpdatesReply, GetContinuousSlotUpdatesRequest, GetCustomInstanceDataReply,
    GetCustomInstanceDataRequest, GetOccasionalClipUpdatesReply, GetOccasionalClipUpdatesRequest,
    GetOccasionalColumnUpdatesReply, GetOccasionalColumnUpdatesRequest,
    GetOccasionalGlobalUpdatesReply, GetOccasionalGlobalUpdatesRequest,
    GetOccasionalInstanceUpdatesReply, GetOccasionalInstanceUpdatesRequest,
    GetOccasionalMatrixUpdatesReply, GetOccasionalMatrixUpdatesRequest,
    GetOccasionalPlaytimeEngineUpdatesReply, GetOccasionalPlaytimeEngineUpdatesRequest,
    GetOccasionalRowUpdatesReply, GetOccasionalRowUpdatesRequest, GetOccasionalSlotUpdatesReply,
    GetOccasionalSlotUpdatesRequest, GetOccasionalTrackUpdatesReply,
    GetOccasionalTrackUpdatesRequest, GetOccasionalUnitUpdatesReply,
    GetOccasionalUnitUpdatesRequest, GetProjectDirReply, GetProjectDirRequest, ImportFilesRequest,
    InsertColumnsRequest, OpenTrackFxRequest, ProtoRequestHandler, ProveAuthenticityReply,
    ProveAuthenticityRequest, SaveControllerRequest, SaveCustomCompartmentDataRequest,
    SetAppSettingsRequest, SetClipDataRequest, SetClipNameRequest, SetColumnSettingsRequest,
    SetColumnTrackRequest, SetCustomInstanceDataRequest, SetInstanceSettingsRequest,
    SetMatrixPanRequest, SetMatrixPlayRateRequest, SetMatrixSettingsRequest, SetMatrixTempoRequest,
    SetMatrixTimeSignatureRequest, SetMatrixVolumeRequest, SetPlaytimeEngineSettingsRequest,
    SetRowDataRequest, SetTrackColorRequest, SetTrackInputMonitoringRequest, SetTrackInputRequest,
    SetTrackNameRequest, SetTrackPanRequest, SetTrackVolumeRequest, TriggerClipRequest,
    TriggerColumnRequest, TriggerGlobalRequest, TriggerInstanceRequest, TriggerMatrixRequest,
    TriggerRowRequest, TriggerSlotRequest, TriggerTrackRequest,
};
use base::future_util;
use futures::{FutureExt, Stream, StreamExt};
#[cfg(feature = "playtime")]
use playtime_clip_engine::base::Matrix;
use std::pin::Pin;
use std::{future, iter};
use tokio::sync::broadcast::Receiver;
use tokio_stream::wrappers::BroadcastStream;
use tonic::{Request, Response, Status};

#[derive(Debug)]
pub struct HelgoboxServiceImpl {
    command_handler: ProtoRequestHandler,
    senders: ProtoSenders,
}

impl HelgoboxServiceImpl {
    pub(crate) fn new(command_handler: ProtoRequestHandler, senders: ProtoSenders) -> Self {
        Self {
            command_handler,
            senders,
        }
    }

    #[cfg(feature = "playtime")]
    fn with_matrix<R>(
        &self,
        clip_matrix_id: u32,
        f: impl FnOnce(&Matrix) -> R,
    ) -> anyhow::Result<R> {
        BackboneShell::get().with_clip_matrix(clip_matrix_id.into(), f)
    }
}

#[tonic::async_trait]
impl helgobox_service_server::HelgoboxService for HelgoboxServiceImpl {
    type GetContinuousMatrixUpdatesStream =
        SyncBoxStream<'static, Result<GetContinuousMatrixUpdatesReply, Status>>;

    async fn get_continuous_matrix_updates(
        &self,
        request: Request<GetContinuousMatrixUpdatesRequest>,
    ) -> Result<Response<Self::GetContinuousMatrixUpdatesStream>, Status> {
        let receiver = self.senders.continuous_matrix_update_sender.subscribe();
        stream_by_session_id(
            request.into_inner().matrix_id,
            receiver,
            |matrix_update| GetContinuousMatrixUpdatesReply {
                matrix_update: Some(matrix_update),
            },
            iter::empty(),
        )
    }
    type GetContinuousColumnUpdatesStream =
        SyncBoxStream<'static, Result<GetContinuousColumnUpdatesReply, Status>>;
    async fn get_continuous_column_updates(
        &self,
        request: Request<GetContinuousColumnUpdatesRequest>,
    ) -> Result<Response<Self::GetContinuousColumnUpdatesStream>, Status> {
        let receiver = self.senders.continuous_column_update_sender.subscribe();
        stream_by_session_id(
            request.into_inner().matrix_id,
            receiver,
            |column_updates| GetContinuousColumnUpdatesReply { column_updates },
            iter::empty(),
        )
    }

    type GetOccasionalColumnUpdatesStream =
        SyncBoxStream<'static, Result<GetOccasionalColumnUpdatesReply, Status>>;

    type GetOccasionalRowUpdatesStream =
        SyncBoxStream<'static, Result<GetOccasionalRowUpdatesReply, Status>>;

    type GetOccasionalSlotUpdatesStream =
        SyncBoxStream<'static, Result<GetOccasionalSlotUpdatesReply, Status>>;

    async fn get_occasional_slot_updates(
        &self,
        request: Request<GetOccasionalSlotUpdatesRequest>,
    ) -> Result<Response<Self::GetOccasionalSlotUpdatesStream>, Status> {
        #[cfg(not(feature = "playtime"))]
        {
            let _ = request;
            Err(playtime_not_available_status())
        }
        #[cfg(feature = "playtime")]
        {
            // Initial
            let initial_updates = self
                .with_matrix(request.get_ref().matrix_id, |matrix| {
                    crate::infrastructure::proto::create_initial_slot_updates(Some(matrix))
                })
                .unwrap_or_else(|_| {
                    crate::infrastructure::proto::create_initial_slot_updates(None)
                });
            // On change
            let receiver = self.senders.occasional_slot_update_sender.subscribe();
            stream_by_session_id(
                request.into_inner().matrix_id,
                receiver,
                |slot_updates| GetOccasionalSlotUpdatesReply { slot_updates },
                Some(GetOccasionalSlotUpdatesReply {
                    slot_updates: initial_updates,
                })
                .into_iter(),
            )
        }
    }

    async fn get_occasional_column_updates(
        &self,
        request: Request<GetOccasionalColumnUpdatesRequest>,
    ) -> Result<Response<Self::GetOccasionalColumnUpdatesStream>, Status> {
        #[cfg(not(feature = "playtime"))]
        {
            let _ = request;
            Err(playtime_not_available_status())
        }
        #[cfg(feature = "playtime")]
        {
            // On change
            let receiver = self.senders.occasional_column_update_sender.subscribe();
            stream_by_session_id(
                request.into_inner().matrix_id,
                receiver,
                |column_updates| GetOccasionalColumnUpdatesReply { column_updates },
                iter::empty(),
            )
        }
    }

    async fn get_occasional_row_updates(
        &self,
        request: Request<GetOccasionalRowUpdatesRequest>,
    ) -> Result<Response<Self::GetOccasionalRowUpdatesStream>, Status> {
        #[cfg(not(feature = "playtime"))]
        {
            let _ = request;
            Err(playtime_not_available_status())
        }
        #[cfg(feature = "playtime")]
        {
            // On change
            let receiver = self.senders.occasional_row_update_sender.subscribe();
            stream_by_session_id(
                request.into_inner().matrix_id,
                receiver,
                |row_updates| GetOccasionalRowUpdatesReply { row_updates },
                iter::empty(),
            )
        }
    }

    type GetOccasionalClipUpdatesStream =
        SyncBoxStream<'static, Result<GetOccasionalClipUpdatesReply, Status>>;

    async fn get_occasional_clip_updates(
        &self,
        request: Request<GetOccasionalClipUpdatesRequest>,
    ) -> Result<Response<Self::GetOccasionalClipUpdatesStream>, Status> {
        #[cfg(not(feature = "playtime"))]
        {
            let _ = request;
            Err(playtime_not_available_status())
        }
        #[cfg(feature = "playtime")]
        {
            // Initial
            let initial_updates = self
                .with_matrix(request.get_ref().matrix_id, |matrix| {
                    crate::infrastructure::proto::create_initial_clip_updates(Some(matrix))
                })
                .unwrap_or_else(|_| {
                    crate::infrastructure::proto::create_initial_clip_updates(None)
                });
            // On change
            let receiver = self.senders.occasional_clip_update_sender.subscribe();
            stream_by_session_id(
                request.into_inner().matrix_id,
                receiver,
                |clip_updates| GetOccasionalClipUpdatesReply { clip_updates },
                Some(GetOccasionalClipUpdatesReply {
                    clip_updates: initial_updates,
                })
                .into_iter(),
            )
        }
    }

    type GetContinuousSlotUpdatesStream =
        SyncBoxStream<'static, Result<GetContinuousSlotUpdatesReply, Status>>;

    async fn get_continuous_slot_updates(
        &self,
        request: Request<GetContinuousSlotUpdatesRequest>,
    ) -> Result<Response<Self::GetContinuousSlotUpdatesStream>, Status> {
        #[cfg(not(feature = "playtime"))]
        {
            let _ = request;
            Err(playtime_not_available_status())
        }
        #[cfg(feature = "playtime")]
        {
            let receiver = self.senders.continuous_slot_update_sender.subscribe();
            stream_by_session_id(
                request.into_inner().matrix_id,
                receiver,
                |slot_updates| GetContinuousSlotUpdatesReply { slot_updates },
                iter::empty(),
            )
        }
    }

    type GetOccasionalGlobalUpdatesStream =
        SyncBoxStream<'static, Result<GetOccasionalGlobalUpdatesReply, Status>>;

    async fn get_occasional_global_updates(
        &self,
        _request: Request<GetOccasionalGlobalUpdatesRequest>,
    ) -> Result<Response<Self::GetOccasionalGlobalUpdatesStream>, Status> {
        let initial_updates = create_initial_global_updates();
        let receiver = self.senders.occasional_global_update_sender.subscribe();
        stream(
            receiver,
            |global_updates| GetOccasionalGlobalUpdatesReply { global_updates },
            |_| true,
            Some(GetOccasionalGlobalUpdatesReply {
                global_updates: initial_updates,
            })
            .into_iter(),
        )
    }

    type GetOccasionalMatrixUpdatesStream =
        SyncBoxStream<'static, Result<GetOccasionalMatrixUpdatesReply, Status>>;

    async fn get_occasional_matrix_updates(
        &self,
        request: Request<GetOccasionalMatrixUpdatesRequest>,
    ) -> Result<Response<Self::GetOccasionalMatrixUpdatesStream>, Status> {
        #[cfg(not(feature = "playtime"))]
        {
            let _ = request;
            Err(playtime_not_available_status())
        }
        #[cfg(feature = "playtime")]
        {
            let initial_updates = self
                .with_matrix(request.get_ref().matrix_id, |matrix| {
                    crate::infrastructure::proto::create_initial_matrix_updates(Some(matrix))
                })
                .unwrap_or_else(|_| {
                    crate::infrastructure::proto::create_initial_matrix_updates(None)
                });
            let receiver = self.senders.occasional_matrix_update_sender.subscribe();
            stream_by_session_id(
                request.into_inner().matrix_id,
                receiver,
                |matrix_updates| GetOccasionalMatrixUpdatesReply { matrix_updates },
                Some(GetOccasionalMatrixUpdatesReply {
                    matrix_updates: initial_updates,
                })
                .into_iter(),
            )
        }
    }

    type GetOccasionalPlaytimeEngineUpdatesStream =
        SyncBoxStream<'static, Result<GetOccasionalPlaytimeEngineUpdatesReply, Status>>;

    async fn get_occasional_playtime_engine_updates(
        &self,
        _request: Request<GetOccasionalPlaytimeEngineUpdatesRequest>,
    ) -> Result<Response<Self::GetOccasionalPlaytimeEngineUpdatesStream>, Status> {
        #[cfg(not(feature = "playtime"))]
        {
            Err(playtime_not_available_status())
        }
        #[cfg(feature = "playtime")]
        {
            let initial_updates = crate::infrastructure::proto::create_initial_engine_updates();
            let receiver = self
                .senders
                .occasional_playtime_engine_update_sender
                .subscribe();
            stream(
                receiver,
                |updates| GetOccasionalPlaytimeEngineUpdatesReply { updates },
                |_| true,
                Some(GetOccasionalPlaytimeEngineUpdatesReply {
                    updates: initial_updates,
                })
                .into_iter(),
            )
        }
    }

    type GetOccasionalInstanceUpdatesStream =
        SyncBoxStream<'static, Result<GetOccasionalInstanceUpdatesReply, Status>>;

    async fn get_occasional_instance_updates(
        &self,
        request: Request<GetOccasionalInstanceUpdatesRequest>,
    ) -> Result<Response<Self::GetOccasionalInstanceUpdatesStream>, Status> {
        let instance_shell = BackboneShell::get()
            .get_instance_shell_by_instance_id(request.get_ref().instance_id.into())
            .map_err(|e| Status::not_found(e.to_string()))?;
        let initial_updates = create_initial_instance_updates(&instance_shell);
        let receiver = self.senders.occasional_instance_update_sender.subscribe();
        stream_by_session_id(
            request.into_inner().instance_id,
            receiver,
            |instance_updates| GetOccasionalInstanceUpdatesReply { instance_updates },
            Some(GetOccasionalInstanceUpdatesReply {
                instance_updates: initial_updates,
            })
            .into_iter(),
        )
    }

    type GetOccasionalUnitUpdatesStream =
        SyncBoxStream<'static, Result<GetOccasionalUnitUpdatesReply, Status>>;

    async fn get_occasional_unit_updates(
        &self,
        request: Request<GetOccasionalUnitUpdatesRequest>,
    ) -> Result<Response<Self::GetOccasionalUnitUpdatesStream>, Status> {
        let instance_shell = BackboneShell::get()
            .get_instance_shell_by_instance_id(request.get_ref().instance_id.into())
            .map_err(|e| Status::not_found(e.to_string()))?;
        let initial_updates = create_initial_unit_updates(&instance_shell);
        let receiver = self.senders.occasional_unit_update_sender.subscribe();
        stream_by_session_id(
            request.into_inner().instance_id,
            receiver,
            |unit_updates| GetOccasionalUnitUpdatesReply { unit_updates },
            Some(GetOccasionalUnitUpdatesReply {
                unit_updates: initial_updates,
            })
            .into_iter(),
        )
    }

    type GetOccasionalTrackUpdatesStream =
        SyncBoxStream<'static, Result<GetOccasionalTrackUpdatesReply, Status>>;

    async fn get_occasional_track_updates(
        &self,
        request: Request<GetOccasionalTrackUpdatesRequest>,
    ) -> Result<Response<Self::GetOccasionalTrackUpdatesStream>, Status> {
        #[cfg(not(feature = "playtime"))]
        {
            let _ = request;
            Err(playtime_not_available_status())
        }
        #[cfg(feature = "playtime")]
        {
            let initial_reply = self
                .with_matrix(request.get_ref().matrix_id, |matrix| {
                    crate::infrastructure::proto::create_initial_track_updates(Some(matrix))
                })
                .unwrap_or_else(|_| {
                    crate::infrastructure::proto::create_initial_track_updates(None)
                });
            let receiver = self.senders.occasional_track_update_sender.subscribe();
            stream_by_session_id(
                request.into_inner().matrix_id,
                receiver,
                |track_updates| GetOccasionalTrackUpdatesReply { track_updates },
                Some(GetOccasionalTrackUpdatesReply {
                    track_updates: initial_reply,
                })
                .into_iter(),
            )
        }
    }

    async fn trigger_slot(
        &self,
        request: Request<TriggerSlotRequest>,
    ) -> Result<Response<Empty>, Status> {
        self.command_handler.trigger_slot(request.into_inner())
    }

    async fn trigger_clip(
        &self,
        request: Request<TriggerClipRequest>,
    ) -> Result<Response<Empty>, Status> {
        self.command_handler.trigger_clip(request.into_inner())
    }

    async fn drag_slot(
        &self,
        request: Request<DragSlotRequest>,
    ) -> Result<Response<Empty>, Status> {
        self.command_handler.drag_slot(request.into_inner())
    }

    async fn drag_clip(
        &self,
        request: Request<DragClipRequest>,
    ) -> Result<Response<Empty>, Status> {
        self.command_handler.drag_clip(request.into_inner())
    }

    async fn drag_row(&self, request: Request<DragRowRequest>) -> Result<Response<Empty>, Status> {
        self.command_handler.drag_row(request.into_inner())
    }

    async fn drag_column(
        &self,
        request: Request<DragColumnRequest>,
    ) -> Result<Response<Empty>, Status> {
        self.command_handler.drag_column(request.into_inner())
    }

    async fn set_track_name(
        &self,
        request: Request<SetTrackNameRequest>,
    ) -> Result<Response<Empty>, Status> {
        self.command_handler.set_track_name(request.into_inner())
    }

    async fn set_track_color(
        &self,
        request: Request<SetTrackColorRequest>,
    ) -> Result<Response<Empty>, Status> {
        self.command_handler.set_track_color(request.into_inner())
    }

    async fn set_clip_name(
        &self,
        request: Request<SetClipNameRequest>,
    ) -> Result<Response<Empty>, Status> {
        self.command_handler.set_clip_name(request.into_inner())
    }

    async fn set_clip_data(
        &self,
        request: Request<SetClipDataRequest>,
    ) -> Result<Response<Empty>, Status> {
        self.command_handler.set_clip_data(request.into_inner())
    }

    async fn trigger_sequence(
        &self,
        request: Request<super::TriggerSequenceRequest>,
    ) -> Result<Response<Empty>, Status> {
        self.command_handler.trigger_sequence(request.into_inner())
    }

    async fn set_sequence_info(
        &self,
        request: Request<super::SetSequenceInfoRequest>,
    ) -> Result<Response<Empty>, Status> {
        self.command_handler.set_sequence_info(request.into_inner())
    }

    async fn trigger_matrix(
        &self,
        request: Request<TriggerMatrixRequest>,
    ) -> Result<Response<Empty>, Status> {
        self.command_handler.trigger_matrix(request.into_inner())
    }

    async fn set_matrix_settings(
        &self,
        request: Request<SetMatrixSettingsRequest>,
    ) -> Result<Response<Empty>, Status> {
        self.command_handler
            .set_matrix_settings(request.into_inner())
    }

    async fn trigger_column(
        &self,
        request: Request<TriggerColumnRequest>,
    ) -> Result<Response<Empty>, Status> {
        self.command_handler.trigger_column(request.into_inner())
    }

    async fn trigger_track(
        &self,
        request: Request<TriggerTrackRequest>,
    ) -> Result<Response<Empty>, Status> {
        self.command_handler.trigger_track(request.into_inner())
    }

    async fn set_column_settings(
        &self,
        request: Request<SetColumnSettingsRequest>,
    ) -> Result<Response<Empty>, Status> {
        self.command_handler
            .set_column_settings(request.into_inner())
    }

    async fn trigger_row(
        &self,
        request: Request<TriggerRowRequest>,
    ) -> Result<Response<Empty>, Status> {
        self.command_handler.trigger_row(request.into_inner())
    }

    async fn set_row_data(
        &self,
        request: Request<SetRowDataRequest>,
    ) -> Result<Response<Empty>, Status> {
        self.command_handler.set_row_data(request.into_inner())
    }

    async fn set_matrix_tempo(
        &self,
        request: Request<SetMatrixTempoRequest>,
    ) -> Result<Response<Empty>, Status> {
        self.command_handler.set_matrix_tempo(request.into_inner())
    }

    async fn set_matrix_play_rate(
        &self,
        request: Request<SetMatrixPlayRateRequest>,
    ) -> Result<Response<Empty>, Status> {
        self.command_handler
            .set_matrix_play_rate(request.into_inner())
    }

    async fn set_matrix_time_signature(
        &self,
        request: Request<SetMatrixTimeSignatureRequest>,
    ) -> Result<Response<Empty>, Status> {
        self.command_handler
            .set_matrix_time_signature(request.into_inner())
    }

    async fn set_matrix_volume(
        &self,
        request: Request<SetMatrixVolumeRequest>,
    ) -> Result<Response<Empty>, Status> {
        self.command_handler.set_matrix_volume(request.into_inner())
    }

    async fn set_matrix_pan(
        &self,
        request: Request<SetMatrixPanRequest>,
    ) -> Result<Response<Empty>, Status> {
        self.command_handler.set_matrix_pan(request.into_inner())
    }

    async fn set_track_volume(
        &self,
        request: Request<SetTrackVolumeRequest>,
    ) -> Result<Response<Empty>, Status> {
        self.command_handler.set_track_volume(request.into_inner())
    }

    async fn set_track_pan(
        &self,
        request: Request<SetTrackPanRequest>,
    ) -> Result<Response<Empty>, Status> {
        self.command_handler.set_track_pan(request.into_inner())
    }

    async fn open_track_fx(
        &self,
        request: Request<OpenTrackFxRequest>,
    ) -> Result<Response<Empty>, Status> {
        self.command_handler.open_track_fx(request.into_inner())
    }

    async fn set_column_track(
        &self,
        request: Request<SetColumnTrackRequest>,
    ) -> Result<Response<Empty>, Status> {
        self.command_handler
            .set_column_track(request.into_inner())
            .await
    }

    async fn get_clip_detail(
        &self,
        request: Request<GetClipDetailRequest>,
    ) -> Result<Response<GetClipDetailReply>, Status> {
        self.command_handler
            .get_clip_detail(request.into_inner())
            .await
    }

    async fn get_host_info(
        &self,
        request: Request<super::GetHostInfoRequest>,
    ) -> Result<Response<super::GetHostInfoReply>, Status> {
        self.command_handler
            .get_host_info(request.into_inner())
            .await
    }

    async fn prove_authenticity(
        &self,
        request: Request<ProveAuthenticityRequest>,
    ) -> Result<Response<ProveAuthenticityReply>, Status> {
        self.command_handler
            .prove_authenticity(request.into_inner())
            .await
    }

    async fn get_project_dir(
        &self,
        request: Request<GetProjectDirRequest>,
    ) -> Result<Response<GetProjectDirReply>, Status> {
        self.command_handler
            .get_project_dir(request.into_inner())
            .await
    }

    async fn get_arrangement_info(
        &self,
        request: Request<GetArrangementInfoRequest>,
    ) -> Result<Response<GetArrangementInfoReply>, Status> {
        self.command_handler
            .get_arrangement_info(request.into_inner())
            .await
    }

    async fn set_track_input_monitoring(
        &self,
        request: Request<SetTrackInputMonitoringRequest>,
    ) -> Result<Response<Empty>, Status> {
        self.command_handler
            .set_track_input_monitoring(request.into_inner())
    }

    async fn set_track_input(
        &self,
        request: Request<SetTrackInputRequest>,
    ) -> Result<Response<Empty>, Status> {
        self.command_handler.set_track_input(request.into_inner())
    }

    async fn import_files(
        &self,
        request: Request<ImportFilesRequest>,
    ) -> Result<Response<Empty>, Status> {
        self.command_handler.import_files(request.into_inner())
    }

    async fn add_license(
        &self,
        request: Request<AddLicenseRequest>,
    ) -> Result<Response<Empty>, Status> {
        self.command_handler.add_license(request.into_inner())
    }

    async fn save_controller(
        &self,
        request: Request<SaveControllerRequest>,
    ) -> Result<Response<Empty>, Status> {
        self.command_handler.save_controller(request.into_inner())
    }

    async fn delete_controller(
        &self,
        request: Request<DeleteControllerRequest>,
    ) -> Result<Response<Empty>, Status> {
        self.command_handler.delete_controller(request.into_inner())
    }

    async fn set_instance_settings(
        &self,
        request: Request<SetInstanceSettingsRequest>,
    ) -> Result<Response<Empty>, Status> {
        self.command_handler
            .set_instance_settings(request.into_inner())
    }

    async fn get_app_settings(
        &self,
        request: Request<GetAppSettingsRequest>,
    ) -> Result<Response<GetAppSettingsReply>, Status> {
        self.command_handler
            .get_app_settings(request.into_inner())
            .await
    }

    async fn set_app_settings(
        &self,
        request: Request<SetAppSettingsRequest>,
    ) -> Result<Response<Empty>, Status> {
        self.command_handler.set_app_settings(request.into_inner())
    }

    async fn get_compartment_data(
        &self,
        request: Request<GetCompartmentDataRequest>,
    ) -> Result<Response<GetCompartmentDataReply>, Status> {
        self.command_handler
            .get_compartment_data(request.into_inner())
    }

    async fn save_custom_compartment_data(
        &self,
        request: Request<SaveCustomCompartmentDataRequest>,
    ) -> Result<Response<Empty>, Status> {
        self.command_handler
            .save_custom_compartment_data(request.into_inner())
    }

    async fn get_custom_instance_data(
        &self,
        request: Request<GetCustomInstanceDataRequest>,
    ) -> Result<Response<GetCustomInstanceDataReply>, Status> {
        self.command_handler
            .get_custom_instance_data(request.into_inner())
    }

    async fn set_custom_instance_data(
        &self,
        request: Request<SetCustomInstanceDataRequest>,
    ) -> Result<Response<Empty>, Status> {
        self.command_handler
            .set_custom_instance_data(request.into_inner())
    }

    async fn trigger_global(
        &self,
        request: Request<TriggerGlobalRequest>,
    ) -> Result<Response<Empty>, Status> {
        self.command_handler.trigger_global(request.into_inner())
    }

    async fn trigger_instance(
        &self,
        request: Request<TriggerInstanceRequest>,
    ) -> Result<Response<Empty>, Status> {
        self.command_handler.trigger_instance(request.into_inner())
    }

    async fn insert_columns(
        &self,
        request: Request<InsertColumnsRequest>,
    ) -> Result<Response<Empty>, Status> {
        self.command_handler.insert_columns(request.into_inner())
    }

    async fn set_playtime_engine_settings(
        &self,
        request: Request<SetPlaytimeEngineSettingsRequest>,
    ) -> Result<Response<Empty>, Status> {
        self.command_handler
            .set_playtime_engine_settings(request.into_inner())
    }
}

type SyncBoxStream<'a, T> = Pin<Box<dyn Stream<Item = T> + Send + Sync + 'a>>;

fn stream_by_session_id<T, R, F, I>(
    requested_instance_id: u32,
    receiver: Receiver<WithInstanceId<T>>,
    create_result: F,
    initial: I,
) -> Result<Response<SyncBoxStream<'static, Result<R, Status>>>, Status>
where
    T: Clone + Send + 'static,
    R: Send + Sync + 'static,
    F: Fn(T) -> R + Send + Sync + 'static,
    I: Iterator<Item = R> + Send + Sync + 'static,
{
    stream(
        receiver,
        move |v| create_result(v.value),
        move |v| v.instance_id == requested_instance_id.into(),
        initial,
    )
}

fn stream<T, R, F, I, Include>(
    receiver: Receiver<T>,
    create_result: F,
    include: Include,
    initial: I,
) -> Result<Response<SyncBoxStream<'static, Result<R, Status>>>, Status>
where
    T: Clone + Send + 'static,
    R: Send + Sync + 'static,
    F: Fn(T) -> R + Send + Sync + 'static,
    I: Iterator<Item = R> + Send + Sync + 'static,
    Include: Fn(&T) -> bool + Send + Sync + 'static,
{
    // Stream that waits 1 millisecond and emits nothing
    // This is done to (hopefully) prevent the following client-side Dart error, which otherwise
    // would occur sporadically when attempting to connect:
    // [ERROR:flutter/runtime/dart_vm_initializer.cc(41)] Unhandled Exception: gRPC Error (code: 2, codeName: UNKNOWN, message: HTTP/2 error: Connection error: Connection is being forcefully terminated. (errorCode: 1), details: null, rawResponse: null, trailers: {})
    let wait_one_milli = future_util::millis(1)
        .map(|_| Err(Status::unknown("skipped")))
        .into_stream()
        .skip(1);
    // Stream for sending the initial state
    let initial_stream = futures::stream::iter(initial.map(|r| Ok(r)));
    // Stream for sending occasional updates
    let receiver_stream = BroadcastStream::new(receiver).filter_map(move |value| {
        let res = match value {
            // Error
            Err(e) => Some(Err(Status::unknown(e.to_string()))),
            // Playtime matrix ID matches
            Ok(value) if include(&value) => Some(Ok(create_result(value))),
            // Playtime matrix ID doesn't match
            _ => None,
        };
        future::ready(res)
    });
    Ok(Response::new(Box::pin(
        wait_one_milli.chain(initial_stream).chain(receiver_stream),
    )))
}
