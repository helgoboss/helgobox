use reaper_high::Reaper;
use rx_util::{SharedEvent, SharedItemEvent, SharedPayload};
use rxrust::prelude::*;
use rxrust::scheduler::Schedulers;
use std::rc::Rc;
use std::time::Duration;

/// Executes the given reaction on the view whenever the specified event is raised.
///
/// It doesn't execute the reaction immediately but in the next main run loop cycle. That's also why
/// the "trigger" and "until" items need to be shareable (Clone + Send + Sync), which is no problem
/// in practice because they are just `()` in our case.
///
/// The observables are expected to be local (not shareable). Application of the `delay()` operator
/// would require them to be shared, too. But `delay()` doesn't work anyway right now
/// (https://github.com/rxRust/rxRust/issues/106). So there's no reason to change all the
/// trigger/until subjects/properties into shared ones.
pub fn when_async<E: SharedPayload, U: SharedPayload, R: 'static>(
    trigger: impl SharedItemEvent<E>,
    until: impl SharedItemEvent<U>,
    receiver: &Rc<R>,
    reaction: impl Fn(Rc<R>) + Copy + 'static,
) {
    let weak_receiver = Rc::downgrade(receiver);
    trigger
        .take_until(until)
        // .observe_on(Reaper::get().main_thread_scheduler())
        // .to_shared()
        .subscribe(move |v| {
            let weak_receiver = weak_receiver.clone();
            Reaper::get().do_later_in_main_thread_asap(move || {
                let receiver = weak_receiver.upgrade().expect("receiver is gone");
                (reaction)(receiver);
            });
        });
}
