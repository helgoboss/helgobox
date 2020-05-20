use reaper_high::Reaper;
use rxrust::prelude::*;
use rxrust::scheduler::Schedulers;
use std::rc::Rc;
use std::time::Duration;
use swell_ui::{ReactiveEvent, SharedView};

/// Executes the given reaction on the view whenever the specified event is raised.
pub fn when_async<R: 'static>(
    until: impl ReactiveEvent<()>,
    receiver: &SharedView<R>,
    event: impl ReactiveEvent<()>,
    reaction: impl Fn(SharedView<R>) + 'static,
) {
    let weak_receiver = AssertSendAndSync(Rc::downgrade(receiver));
    let reaction = AssertSendAndSync(reaction);
    event
        .take_until(until)
        .observe_on(Reaper::get().main_thread_scheduler())
        // .delay(Duration::from_secs(5))
        .to_shared()
        .subscribe(move |_| {
            let receiver = weak_receiver.0.upgrade().expect("view is gone");
            (reaction.0)(receiver);
        });
}

struct AssertSendAndSync<T>(T);

unsafe impl<T> Send for AssertSendAndSync<T> {}
unsafe impl<T> Sync for AssertSendAndSync<T> {}
