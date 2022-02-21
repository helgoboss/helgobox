use assert_no_alloc::permit_alloc;
use std::any::TypeId;
use std::{fs, io};
use tracing::level_filters::LevelFilter;
use tracing::span::{Attributes, Record};
use tracing::subscriber::Interest;
use tracing::{Event, Id, Metadata};
use tracing_subscriber::fmt::FormatEvent;
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::{EnvFilter, FmtSubscriber, Layer};

pub fn setup_tracing() {
    let subscriber = FmtSubscriber::builder()
        .with_env_filter(EnvFilter::from_env("REALEARN_LOG"))
        .finish();
    let subscriber = PermitAllocSubscriber::new(subscriber);
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");
}

struct PermitAllocSubscriber<S> {
    inner: S,
}

impl<S> PermitAllocSubscriber<S> {
    pub fn new(inner: S) -> Self {
        PermitAllocSubscriber { inner }
    }
}

impl<S: tracing::Subscriber> tracing::Subscriber for PermitAllocSubscriber<S> {
    fn register_callsite(&self, metadata: &'static Metadata<'static>) -> Interest {
        self.inner.register_callsite(metadata)
    }

    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        self.inner.enabled(metadata)
    }

    fn max_level_hint(&self) -> Option<LevelFilter> {
        self.inner.max_level_hint()
    }

    fn new_span(&self, span: &Attributes<'_>) -> Id {
        self.inner.new_span(span)
    }

    fn record(&self, span: &Id, values: &Record<'_>) {
        self.inner.record(span, values)
    }

    fn record_follows_from(&self, span: &Id, follows: &Id) {
        self.inner.record_follows_from(span, follows)
    }

    fn event(&self, event: &Event<'_>) {
        permit_alloc(|| self.inner.event(event))
    }

    fn enter(&self, span: &Id) {
        self.inner.enter(span)
    }

    fn exit(&self, span: &Id) {
        self.inner.exit(span)
    }

    fn clone_span(&self, id: &Id) -> Id {
        self.inner.clone_span(id)
    }

    fn drop_span(&self, id: Id) {
        self.inner.drop_span(id)
    }

    fn try_close(&self, id: Id) -> bool {
        self.inner.try_close(id)
    }

    fn current_span(&self) -> tracing_core::span::Current {
        self.inner.current_span()
    }

    unsafe fn downcast_raw(&self, id: TypeId) -> Option<*const ()> {
        self.inner.downcast_raw(id)
    }
}
