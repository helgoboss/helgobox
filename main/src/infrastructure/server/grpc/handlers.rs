use crate::domain::BackboneState;
use crate::infrastructure::server::data::{get_clip_matrix_data, DataError, DataErrorCategory};
use crate::infrastructure::server::grpc::proto::{
    greeter_server, DoubleReply, DoubleRequest, GetClipMatrixReply, GetClipMatrixRequest,
};
use futures::{Stream, StreamExt};
use std::pin::Pin;
use tokio_stream::wrappers::BroadcastStream;
use tonic::{Code, Request, Response, Status};

#[derive(Debug, Default)]
pub struct MyGreeter {}

#[tonic::async_trait]
impl greeter_server::Greeter for MyGreeter {
    async fn get_clip_matrix(
        &self,
        request: Request<GetClipMatrixRequest>,
    ) -> Result<Response<GetClipMatrixReply>, Status> {
        let matrix =
            get_clip_matrix_data(&request.get_ref().session_id).map_err(translate_data_error)?;
        let json = serde_json::to_string(&matrix).map_err(|e| Status::unknown(e.to_string()))?;
        let reply = GetClipMatrixReply { value: json };
        Ok(Response::new(reply))
    }

    type StreamExperimentStream = SyncBoxStream<'static, Result<DoubleReply, Status>>;

    async fn stream_experiment(
        &self,
        _request: Request<DoubleRequest>,
    ) -> Result<Response<Self::StreamExperimentStream>, Status> {
        let receiver = BackboneState::server_event_sender().subscribe();
        let receiver_stream = BroadcastStream::new(receiver).map(|value| match value {
            Ok(v) => Ok(DoubleReply { value: v }),
            Err(e) => Err(Status::unknown(e.to_string())),
        });
        Ok(Response::new(Box::pin(receiver_stream)))
    }
}

type SyncBoxStream<'a, T> = Pin<Box<dyn Stream<Item = T> + Send + Sync + 'a>>;

fn translate_data_error(e: DataError) -> Status {
    use DataErrorCategory::*;
    let code = match e.category() {
        NotFound => Code::NotFound,
        BadRequest => Code::FailedPrecondition,
        MethodNotAllowed => Code::NotFound,
        InternalServerError => Code::Unknown,
    };
    Status::new(code, e.description())
}
