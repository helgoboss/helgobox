use crate::domain::BackboneState;
use crate::infrastructure::server::grpc::proto::{greeter_server, DoubleReply, DoubleRequest};
use crate::infrastructure::server::grpc::proto::{HelloReply, HelloRequest};
use futures::stream::BoxStream;
use futures::{Stream, StreamExt};
use std::alloc::alloc;
use std::pin::Pin;
use tokio::sync::broadcast;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;
use tokio_stream::wrappers::BroadcastStream;
use tonic::{Request, Response, Status};

#[derive(Debug, Default)]
pub struct MyGreeter {}

#[tonic::async_trait]
impl greeter_server::Greeter for MyGreeter {
    async fn say_hello(
        &self,
        request: Request<HelloRequest>,
    ) -> Result<Response<HelloReply>, Status> {
        println!("Got a request: {:?}", request);

        let reply = HelloReply {
            message: format!("Hello {}!", request.into_inner().name).into(),
        };

        Ok(Response::new(reply))
    }

    type StreamExperimentStream = SyncBoxStream<'static, Result<DoubleReply, Status>>;

    async fn stream_experiment(
        &self,
        request: Request<DoubleRequest>,
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
