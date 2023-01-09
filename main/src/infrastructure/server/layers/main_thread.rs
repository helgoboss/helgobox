use crate::base::Global;
use axum::http::Response;
use futures::channel::oneshot;
use futures::future::BoxFuture;
use std::task::{Context, Poll};
use tower::{Layer, Service};

/// A Tower layer that will cause wrapped services to be executed in REAPER's main thread.
#[derive(Clone)]
pub struct MainThreadLayer;

impl<S> Layer<S> for MainThreadLayer {
    type Service = MainThreadService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        MainThreadService { inner }
    }
}

// See https://docs.rs/axum/0.3.4/axum/index.html#writing-your-own-middleware.
#[derive(Clone)]
pub struct MainThreadService<S> {
    inner: S,
}

impl<S, E, Req, ResBody> Service<Req> for MainThreadService<S>
where
    // The wrapped service must be a service that creates a HTTP response.
    S: Service<Req, Response = Response<ResBody>, Error = E>,
    // Both the future generating the response as well as its output must be send over to the main
    // thread, so it's mandatory that all of that implements Send.
    S::Future: Send + 'static,
    ResBody: Default + Send + 'static,
    E: Send + 'static,
{
    // The wrapper produces a HTTP response as well.
    type Response = Response<ResBody>;
    type Error = S::Error;
    // The type of the returned future can't be statically expressed, so we need a BoxFuture.
    type Future = BoxFuture<'static, Result<Response<ResBody>, S::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Req) -> Self::Future {
        // Obtain future from inner service (the future that will generate the response).
        // But don't await that future! We must do that in REAPER's main thread, that's the point
        // of this middleware.
        let inner_response_future = self.inner.call(request);
        // Spawn task on REAPER's main thread.
        let (tx, rx) = oneshot::channel::<Result<Response<ResBody>, S::Error>>();
        Global::future_support().spawn_in_main_thread(async move {
            // Process request and create response in main thread
            let response = inner_response_future.await;
            // Push result via channel to Tokio runtime thread
            let _ = tx.send(response);
        });
        let response_future = async {
            rx.await.unwrap_or_else(|_| {
                let response = Response::builder()
                    .status(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
                    .body(ResBody::default())
                    .unwrap();
                Ok(response)
            })
        };
        Box::pin(response_future)
    }
}
