use reaper_high::Reaper;
use rx_util::{Event, SharedEvent, SharedItemEvent, SharedPayload};
use rxrust::prelude::*;
use rxrust::scheduler::Schedulers;
use std::marker::PhantomData;
use std::rc::{Rc, Weak};
use std::time::Duration;

pub fn when<Item, Trigger>(trigger: Trigger) -> ReactionBuilderStepOne<Item, Trigger>
where
    Trigger: Event<Item>,
{
    ReactionBuilderStepOne {
        trigger,
        p: PhantomData,
    }
}

pub struct ReactionBuilderStepOne<Item, Trigger> {
    trigger: Trigger,
    p: PhantomData<Item>,
}

impl<Item, Trigger> ReactionBuilderStepOne<Item, Trigger>
where
    Trigger: Event<Item>,
{
    pub fn with<Receiver>(
        self,
        receiver: &Rc<Receiver>,
    ) -> ReactionBuilderStepTwo<Item, Trigger, Receiver>
    where
        Receiver: 'static,
    {
        ReactionBuilderStepTwo {
            parent: self,
            weak_receiver: Rc::downgrade(receiver),
        }
    }
}

pub struct ReactionBuilderStepTwo<Item, Trigger, Receiver> {
    parent: ReactionBuilderStepOne<Item, Trigger>,
    weak_receiver: Weak<Receiver>,
}

impl<Item, Trigger, Receiver> ReactionBuilderStepTwo<Item, Trigger, Receiver>
where
    Trigger: Event<Item>,
    Receiver: 'static,
{
    pub fn finally<Finalizer>(
        self,
        finalizer: Finalizer,
    ) -> ReactionBuilderStepThree<Item, Trigger, Receiver, Finalizer>
    where
        Finalizer: Fn(Rc<Receiver>) + Copy + 'static,
    {
        ReactionBuilderStepThree {
            parent: self,
            finalizer,
        }
    }
}

pub struct ReactionBuilderStepThree<Item, Trigger, Receiver, Finalizer> {
    parent: ReactionBuilderStepTwo<Item, Trigger, Receiver>,
    finalizer: Finalizer,
}

impl<Item, Trigger, Receiver, Finalizer>
    ReactionBuilderStepThree<Item, Trigger, Receiver, Finalizer>
where
    Trigger: Event<Item>,
    Receiver: 'static,
    Finalizer: Fn(Rc<Receiver>) + Copy + 'static,
{
    pub fn do_sync(
        self,
        reaction: impl Fn(Rc<Receiver>, Item) + Copy + 'static,
    ) -> SubscriptionWrapper<LocalSubscription> {
        let weak_receiver_one = self.parent.weak_receiver;
        let weak_receiver_two = weak_receiver_one.clone();
        let finalizer = self.finalizer;
        self.parent
            .parent
            .trigger
            .finalize(move || {
                let receiver = weak_receiver_one.upgrade().expect("receiver gone");
                (finalizer)(receiver);
            })
            .subscribe(move |item| {
                let receiver = weak_receiver_two.upgrade().expect("receiver gone");
                (reaction)(receiver, item);
            })
    }

    pub fn do_async(
        self,
        reaction: impl Fn(Rc<Receiver>, Item) + Copy + 'static,
    ) -> SubscriptionWrapper<LocalSubscription>
    where
        Item: 'static,
    {
        let weak_receiver_one = self.parent.weak_receiver;
        let weak_receiver_two = weak_receiver_one.clone();
        let finalizer = self.finalizer;
        self.parent
            .parent
            .trigger
            .finalize(move || {
                let receiver = weak_receiver_one.upgrade().expect("receiver gone");
                (finalizer)(receiver);
            })
            .subscribe(move |item| {
                let weak_receiver = weak_receiver_two.clone();
                Reaper::get().do_later_in_main_thread_asap(move || {
                    let receiver = weak_receiver.upgrade().expect("receiver gone");
                    (reaction)(receiver, item);
                });
            })
    }
}

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
