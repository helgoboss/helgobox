use reaper_high::Reaper;
use rx_util::{LocalReactiveEvent, SharedReactiveEvent, SharedReactiveItem};
use rxrust::prelude::*;
use rxrust::scheduler::Schedulers;
use std::rc::Rc;
use std::time::Duration;
use swell_ui::SharedView;

/// Executes the given reaction on the view whenever the specified event is raised.
pub fn when_async<R: 'static>(
    until: impl LocalReactiveEvent<()>,
    receiver: &SharedView<R>,
    event: impl LocalReactiveEvent<()>,
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

/// Executes the given reaction on the view whenever the specified event is raised.
pub fn when_async_2<E: SharedReactiveItem, U: SharedReactiveItem, R: 'static>(
    event: impl SharedReactiveEvent<E>,
    reaction: impl Fn(SharedView<R>) + 'static,
    receiver: &SharedView<R>,
    until: impl SharedReactiveEvent<U>,
) {
    let weak_receiver = AssertSendAndSync(Rc::downgrade(receiver));
    let reaction = AssertSendAndSync(reaction);
    event
        .take_until(until)
        .to_shared()
        .observe_on(Reaper::get().main_thread_scheduler())
        .delay(Duration::from_secs(5))
        .to_shared()
        .subscribe(move |v| {
            let receiver = weak_receiver.0.upgrade().expect("view is gone");
            (reaction.0)(receiver);
        });
}

struct AssertSendAndSync<T>(T);

unsafe impl<T> Send for AssertSendAndSync<T> {}
unsafe impl<T> Sync for AssertSendAndSync<T> {}
