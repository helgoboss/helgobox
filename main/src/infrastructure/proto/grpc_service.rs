use crate::infrastructure::proto::initial_events::{
    create_initial_matrix_updates, create_initial_slot_updates, create_initial_track_updates,
};
use crate::infrastructure::proto::senders::{ClipEngineSenders, WithSessionId};
use crate::infrastructure::proto::{
    create_initial_clip_updates, create_initial_global_updates, helgobox_service_server,
    ClipEngineRequestHandler, DeleteControllerRequest, DragClipRequest, DragColumnRequest,
    DragRowRequest, DragSlotRequest, Empty, GetArrangementInfoReply, GetArrangementInfoRequest,
    GetClipDetailReply, GetClipDetailRequest, GetContinuousColumnUpdatesReply,
    GetContinuousColumnUpdatesRequest, GetContinuousMatrixUpdatesReply,
    GetContinuousMatrixUpdatesRequest, GetContinuousSlotUpdatesReply,
    GetContinuousSlotUpdatesRequest, GetOccasionalClipUpdatesReply,
    GetOccasionalClipUpdatesRequest, GetOccasionalColumnUpdatesReply,
    GetOccasionalColumnUpdatesRequest, GetOccasionalGlobalUpdatesReply,
    GetOccasionalGlobalUpdatesRequest, GetOccasionalMatrixUpdatesReply,
    GetOccasionalMatrixUpdatesRequest, GetOccasionalRowUpdatesReply,
    GetOccasionalRowUpdatesRequest, GetOccasionalSlotUpdatesReply, GetOccasionalSlotUpdatesRequest,
    GetOccasionalTrackUpdatesReply, GetOccasionalTrackUpdatesRequest, GetProjectDirReply,
    GetProjectDirRequest, ImportFilesRequest, MatrixProvider, ProveAuthenticityReply,
    ProveAuthenticityRequest, SaveControllerRequest, SetClipDataRequest, SetClipNameRequest,
    SetColumnSettingsRequest, SetColumnTrackRequest, SetMatrixPanRequest, SetMatrixSettingsRequest,
    SetMatrixTempoRequest, SetMatrixTimeSignatureRequest, SetMatrixVolumeRequest,
    SetRowDataRequest, SetTrackColorRequest, SetTrackInputMonitoringRequest, SetTrackInputRequest,
    SetTrackNameRequest, SetTrackPanRequest, SetTrackVolumeRequest, TriggerClipRequest,
    TriggerColumnRequest, TriggerMatrixRequest, TriggerRowRequest, TriggerSlotRequest,
    TriggerTrackRequest,
};
use base::future_util;
use futures::{FutureExt, Stream, StreamExt};
use std::pin::Pin;
use std::{future, iter};
use tokio::sync::broadcast::Receiver;
use tokio_stream::wrappers::BroadcastStream;
use tonic::{Request, Response, Status};

#[derive(Debug)]
pub struct GrpcClipEngineService<P> {
    matrix_provider: P,
    command_handler: ClipEngineRequestHandler<P>,
    senders: ClipEngineSenders,
}

impl<P: MatrixProvider> GrpcClipEngineService<P> {
    pub(crate) fn new(
        matrix_provider: P,
        command_handler: ClipEngineRequestHandler<P>,
        senders: ClipEngineSenders,
    ) -> Self {
        Self {
            matrix_provider,
            command_handler,
            senders,
        }
    }
}

#[tonic::async_trait]
impl<P: MatrixProvider> helgobox_service_server::HelgoboxService for GrpcClipEngineService<P> {
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
        // Initial
        let initial_updates = self
            .matrix_provider
            .with_matrix(&request.get_ref().matrix_id, |matrix| {
                create_initial_slot_updates(Some(matrix))
            })
            .unwrap_or_else(|_| create_initial_slot_updates(None));
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

    async fn get_occasional_column_updates(
        &self,
        request: Request<GetOccasionalColumnUpdatesRequest>,
    ) -> Result<Response<Self::GetOccasionalColumnUpdatesStream>, Status> {
        // On change
        let receiver = self.senders.occasional_column_update_sender.subscribe();
        stream_by_session_id(
            request.into_inner().matrix_id,
            receiver,
            |column_updates| GetOccasionalColumnUpdatesReply { column_updates },
            iter::empty(),
        )
    }

    async fn get_occasional_row_updates(
        &self,
        request: Request<GetOccasionalRowUpdatesRequest>,
    ) -> Result<Response<Self::GetOccasionalRowUpdatesStream>, Status> {
        // On change
        let receiver = self.senders.occasional_row_update_sender.subscribe();
        stream_by_session_id(
            request.into_inner().matrix_id,
            receiver,
            |row_updates| GetOccasionalRowUpdatesReply { row_updates },
            iter::empty(),
        )
    }

    type GetOccasionalClipUpdatesStream =
        SyncBoxStream<'static, Result<GetOccasionalClipUpdatesReply, Status>>;

    async fn get_occasional_clip_updates(
        &self,
        request: Request<GetOccasionalClipUpdatesRequest>,
    ) -> Result<Response<Self::GetOccasionalClipUpdatesStream>, Status> {
        // Initial
        let initial_updates = self
            .matrix_provider
            .with_matrix(&request.get_ref().matrix_id, |matrix| {
                create_initial_clip_updates(Some(matrix))
            })
            .unwrap_or_else(|_| create_initial_clip_updates(None));
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

    type GetContinuousSlotUpdatesStream =
        SyncBoxStream<'static, Result<GetContinuousSlotUpdatesReply, Status>>;

    async fn get_continuous_slot_updates(
        &self,
        request: Request<GetContinuousSlotUpdatesRequest>,
    ) -> Result<Response<Self::GetContinuousSlotUpdatesStream>, Status> {
        let receiver = self.senders.continuous_slot_update_sender.subscribe();
        stream_by_session_id(
            request.into_inner().matrix_id,
            receiver,
            |slot_updates| GetContinuousSlotUpdatesReply { slot_updates },
            iter::empty(),
        )
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
        let initial_updates = self
            .matrix_provider
            .with_matrix(&request.get_ref().matrix_id, |matrix| {
                create_initial_matrix_updates(Some(matrix))
            })
            .unwrap_or_else(|_| create_initial_matrix_updates(None));
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

    type GetOccasionalTrackUpdatesStream =
        SyncBoxStream<'static, Result<GetOccasionalTrackUpdatesReply, Status>>;

    async fn get_occasional_track_updates(
        &self,
        request: Request<GetOccasionalTrackUpdatesRequest>,
    ) -> Result<Response<Self::GetOccasionalTrackUpdatesStream>, Status> {
        let initial_reply = self
            .matrix_provider
            .with_matrix(&request.get_ref().matrix_id, |matrix| {
                create_initial_track_updates(Some(matrix))
            })
            .unwrap_or_else(|_| create_initial_track_updates(None));
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
}

type SyncBoxStream<'a, T> = Pin<Box<dyn Stream<Item = T> + Send + Sync + 'a>>;

fn stream_by_session_id<T, R, F, I>(
    requested_clip_matrix_id: String,
    receiver: Receiver<WithSessionId<T>>,
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
        move |v| v.session_id == requested_clip_matrix_id,
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
            // Clip matrix ID matches
            Ok(value) if include(&value) => Some(Ok(create_result(value))),
            // Clip matrix ID doesn't match
            _ => None,
        };
        future::ready(res)
    });
    Ok(Response::new(Box::pin(
        wait_one_milli.chain(initial_stream).chain(receiver_stream),
    )))
}
