use base::hash_util::NonCryptoHashMap;
use std::any::TypeId;
use std::collections::hash_map::Entry;
use std::fmt::Debug;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tracing::field;
use tracing_core::callsite::Identifier;
use tracing_core::span::{Attributes, Id, Record};
use tracing_core::{Dispatch, Event, Field, Interest, LevelFilter, Metadata, Subscriber};

/// A tracing subscriber that introduces special handling for tracing events with a "spam" field: It prevents the
/// console from being spammed by repeated events of one kind.
///
/// The default behavior is that such an event is logged 1 time only. If the "spam" field has a literal integer value
/// `n`, the event is logged at a maximum `n` times.
///
/// The counter for each event is reset after a configurable amount of time.
pub struct SpamFilter<S> {
    default_max_log_count: u32,
    reset_after: Duration,
    already_logged_spam: Mutex<NonCryptoHashMap<Identifier, Spam>>,
    subscriber: S,
}

struct Spam {
    max_log_count: u32,
    current_log_count: u32,
    last_timestamp: Instant,
}

impl<S> SpamFilter<S> {
    pub fn new(default_max_log_count: u32, reset_after: Duration, subscriber: S) -> Self {
        Self {
            default_max_log_count,
            reset_after,
            already_logged_spam: Default::default(),
            subscriber,
        }
    }

    fn include_event(&self, event: &Event) -> bool {
        if event.metadata().fields().field("spam").is_none() {
            return true;
        }
        let mut map = self.already_logged_spam.lock().unwrap();
        match map.entry(event.metadata().callsite()) {
            Entry::Occupied(e) => {
                let spam = e.into_mut();
                spam.last_timestamp = Instant::now();
                if spam.last_timestamp.elapsed() < self.reset_after {
                    // Not yet reset time
                    if spam.current_log_count >= spam.max_log_count {
                        // Threshold exceeded
                        false
                    } else {
                        // Threshold not exceeded yet
                        spam.current_log_count += 1;
                        true
                    }
                } else {
                    // Reset
                    spam.current_log_count = 1;
                    true
                }
            }
            Entry::Vacant(e) => {
                let mut max_log_count_extractor = MaxLogCountExtractor {
                    max_log_count: self.default_max_log_count,
                };
                event.record(&mut max_log_count_extractor);
                let initial = Spam {
                    max_log_count: max_log_count_extractor.max_log_count,
                    current_log_count: 1,
                    last_timestamp: Instant::now(),
                };
                e.insert(initial);
                true
            }
        }
    }
}

impl<S: Subscriber> Subscriber for SpamFilter<S> {
    fn on_register_dispatch(&self, subscriber: &Dispatch) {
        self.subscriber.on_register_dispatch(subscriber)
    }

    fn register_callsite(&self, metadata: &'static Metadata<'static>) -> Interest {
        self.subscriber.register_callsite(metadata)
    }

    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        self.subscriber.enabled(metadata)
    }

    fn max_level_hint(&self) -> Option<LevelFilter> {
        self.subscriber.max_level_hint()
    }

    fn new_span(&self, span: &Attributes<'_>) -> Id {
        self.subscriber.new_span(span)
    }

    fn record(&self, span: &Id, values: &Record<'_>) {
        self.subscriber.record(span, values)
    }

    fn record_follows_from(&self, span: &Id, follows: &Id) {
        self.subscriber.record_follows_from(span, follows)
    }

    fn event_enabled(&self, event: &Event<'_>) -> bool {
        self.subscriber.event_enabled(event)
    }

    fn event(&self, event: &Event<'_>) {
        if self.include_event(event) {
            self.subscriber.event(event)
        }
    }

    fn enter(&self, span: &Id) {
        self.subscriber.enter(span)
    }

    fn exit(&self, span: &Id) {
        self.subscriber.exit(span)
    }

    fn clone_span(&self, id: &Id) -> Id {
        self.subscriber.clone_span(id)
    }

    fn drop_span(&self, id: Id) {
        self.subscriber.drop_span(id)
    }

    fn try_close(&self, id: Id) -> bool {
        self.subscriber.try_close(id)
    }

    fn current_span(&self) -> tracing_core::span::Current {
        self.subscriber.current_span()
    }

    unsafe fn downcast_raw(&self, id: TypeId) -> Option<*const ()> {
        self.subscriber.downcast_raw(id)
    }
}

struct MaxLogCountExtractor {
    max_log_count: u32,
}
impl field::Visit for MaxLogCountExtractor {
    fn record_i64(&mut self, field: &Field, value: i64) {
        if field.name() == "spam" {
            self.max_log_count = value as u32;
        }
    }

    fn record_debug(&mut self, _field: &Field, _value: &dyn Debug) {}
}
