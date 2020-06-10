use reaper_high::Reaper;
use rx_util::{Event, SharedEvent, SharedItemEvent, SharedPayload};
use rxrust::prelude::*;
use rxrust::scheduler::Schedulers;
use std::rc::Rc;
use std::time::Duration;

// TODO-medium We should remove duplicate code here without changing the public interface!
//  The best would be to have one singler builder for all of this! Code that speaks.

/// Executes the given reaction synchronously whenever the specified event is raised.
pub fn when_sync<E, U, R: 'static>(
    trigger: impl Event<E>,
    until: impl Event<U>,
    receiver: &Rc<R>,
    reaction: impl Fn(Rc<R>) + Copy + 'static,
) -> SubscriptionWrapper<LocalSubscription> {
    let weak_receiver = Rc::downgrade(receiver);
    trigger.take_until(until).subscribe(move |v| {
        let receiver = weak_receiver.upgrade().expect("receiver is gone");
        (reaction)(receiver);
    })
}

pub fn when_sync_with_item<E, U, R: 'static>(
    trigger: impl Event<E>,
    // TODO-medium until is not necessary. We can just add it to the trigger on client side
    until: impl Event<U>,
    receiver: &Rc<R>,
    reaction: impl Fn(Rc<R>, E) + Copy + 'static,
    complete: impl Fn(Rc<R>) + Copy + 'static,
) -> SubscriptionWrapper<LocalSubscription> {
    let weak_receiver = Rc::downgrade(receiver);
    let weak_receiver_2 = weak_receiver.clone();
    trigger.take_until(until).subscribe_complete(
        move |v| {
            let receiver = weak_receiver.upgrade().expect("receiver is gone");
            (reaction)(receiver, v);
        },
        move || {
            let receiver = weak_receiver_2.upgrade().expect("receiver is gone");
            (complete)(receiver);
        },
    )
}

/// Executes the given reaction whenever the specified event is raised.
///
/// It doesn't execute the reaction immediately but in the next main run loop cycle. That's also why
/// the "trigger" and "until" items need to be shareable (Clone + Send + Sync), which is no problem
/// in practice because they are just `()` in our case.
///
/// The observables are expected to be local (not shareable). Application of the `delay()` operator
/// would require them to be shared, too. But `delay()` doesn't work anyway right now
/// (https://github.com/rxRust/rxRust/issues/106). So there's no reason to change all the
/// trigger/until subjects/properties into shared ones.
pub fn when_async<E, U, R: 'static>(
    trigger: impl Event<E>,
    until: impl Event<U>,
    receiver: &Rc<R>,
    reaction: impl Fn(Rc<R>) + Copy + 'static,
) -> SubscriptionWrapper<LocalSubscription> {
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
        })
}

pub fn when_async_with_item<E: 'static, U, R: 'static>(
    trigger: impl Event<E>,
    until: impl Event<U>,
    receiver: &Rc<R>,
    reaction: impl Fn(Rc<R>, E) + Copy + 'static,
) -> SubscriptionWrapper<LocalSubscription> {
    let weak_receiver = Rc::downgrade(receiver);
    trigger.take_until(until).subscribe(move |v| {
        let weak_receiver = weak_receiver.clone();
        Reaper::get().do_later_in_main_thread_asap(move || {
            let receiver = weak_receiver.upgrade().expect("receiver is gone");
            (reaction)(receiver, v);
        });
    })
}
