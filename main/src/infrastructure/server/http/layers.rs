use crate::base::Global;
use axum::body::{boxed, Body, BoxBody};
use axum::http::{Request, Response};
use futures::channel::oneshot;
use futures::future::BoxFuture;
use hyper::StatusCode;
use std::convert::Infallible;
use std::task::{Context, Poll};
use tower::{Layer, Service};

/// An Axum layer that will cause wrapped handlers to be executed in REAPER's main thread.
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

impl<S> Service<Request<Body>> for MainThreadService<S>
where
    // The wrapped service must be an Axum handler and we make our life a bit easier by requiring
    // that it doesn't fail (but converts every error nicely to status codes itself).
    S: Service<Request<Body>, Response = Response<BoxBody>, Error = Infallible>,
    // Both the future generating the response as well as its output must be send over to the main
    // thread, so it's mandatory that all of that implements Send.
    S::Future: Send + 'static,
{
    // The wrapper produces an Axum response as well.
    type Response = Response<BoxBody>;
    // Just like the wrapped Axum handler, we convert errors immediately to status codes.
    type Error = Infallible;
    // The type of the returned future can't be statically expressed, so we need a BoxFuture.
    type Future = BoxFuture<'static, Result<Response<BoxBody>, Infallible>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request<Body>) -> Self::Future {
        // Obtain future from inner Axum handler (the future that will generate the response).
        // But don't await that future! We must do that in REAPER's main thread, that's the point
        // of this middleware.
        let inner_response_future = self.inner.call(request);
        // Spawn task on REAPER's main thread.
        let (tx, rx) = oneshot::channel::<Result<Response<BoxBody>, Infallible>>();
        Global::future_support().spawn_in_main_thread(async move {
            // Process request and create response in main thread
            let response = inner_response_future.await;
            // Push result via channel to Tokio runtime thread
            tx.send(response).expect("couldn't send");
        });
        let response_future = async {
            rx.await.unwrap_or_else(|_| {
                let response = Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(boxed(Body::from("sender dropped")))
                    .unwrap();
                Ok(response)
            })
        };
        Box::pin(response_future)
    }
}
