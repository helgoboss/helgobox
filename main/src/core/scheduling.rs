use reaper_high::Reaper;
use rx_util::{Event, SharedPayload};
use rxrust::prelude::*;

use crate::core::Global;
use slog::debug;
use std::marker::PhantomData;
use std::rc::{Rc, Weak};

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
        weak_receiver: Weak<Receiver>,
    ) -> ReactionBuilderStepTwo<Item, Trigger, Receiver>
    where
        Receiver: 'static,
    {
        ReactionBuilderStepTwo {
            parent: self,
            weak_receiver,
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
        Finalizer: Fn(Rc<Receiver>) + 'static,
    {
        ReactionBuilderStepThree {
            parent: self,
            finalizer,
        }
    }

    /// Executes the given reaction synchronously whenever the specified event is raised.
    pub fn do_sync(
        self,
        reaction: impl Fn(Rc<Receiver>, Item) + 'static,
    ) -> SubscriptionWrapper<impl SubscriptionLike> {
        Self::do_sync_internal(self.weak_receiver, self.parent.trigger, reaction)
    }

    /// Executes the given reaction whenever the specified event is raised.
    ///
    /// It doesn't execute the reaction immediately but in the next main run loop cycle. That's also
    /// why the "trigger" and "until" items need to be shareable (Clone + Send + Sync), which is
    /// no problem in practice because they are just `()` in our case.
    ///
    /// The observables are expected to be local (not shareable). Application of the `delay()`
    /// operator would require them to be shared, too. But `delay()` doesn't work anyway right
    /// now (https://github.com/rxRust/rxRust/issues/106). So there's no reason to change all the
    /// trigger/until subjects/properties into shared ones.
    pub fn do_async(
        self,
        reaction: impl Fn(Rc<Receiver>, Item) + Clone + 'static,
    ) -> SubscriptionWrapper<impl SubscriptionLike>
    where
        Item: 'static,
    {
        Self::do_async_internal(self.weak_receiver, self.parent.trigger, reaction)
    }

    fn do_sync_internal(
        weak_receiver: Weak<Receiver>,
        trigger: impl Event<Item>,
        reaction: impl Fn(Rc<Receiver>, Item) + 'static,
    ) -> SubscriptionWrapper<impl SubscriptionLike> {
        trigger.subscribe(move |item| {
            if let Some(receiver) = upgrade(&weak_receiver) {
                (reaction)(receiver, item);
            }
        })
    }

    fn do_async_internal(
        weak_receiver: Weak<Receiver>,
        trigger: impl Event<Item>,
        reaction: impl Fn(Rc<Receiver>, Item) + Clone + 'static,
    ) -> SubscriptionWrapper<impl SubscriptionLike>
    where
        Item: 'static,
    {
        trigger.subscribe(move |item| {
            let weak_receiver = weak_receiver.clone();
            let reaction = reaction.clone();
            Global::task_support()
                .do_later_in_main_thread_from_main_thread_asap(move || {
                    if let Some(receiver) = upgrade(&weak_receiver) {
                        (reaction)(receiver, item);
                    }
                })
                .unwrap();
        })
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
    Finalizer: Fn(Rc<Receiver>) + Clone + 'static,
{
    pub fn do_sync(
        self,
        reaction: impl Fn(Rc<Receiver>, Item) + Clone + 'static,
    ) -> SubscriptionWrapper<impl SubscriptionLike> {
        let weak_receiver = self.parent.weak_receiver.clone();
        let finalizer = self.finalizer;
        ReactionBuilderStepTwo::<Item, Trigger, Receiver>::do_sync_internal(
            self.parent.weak_receiver,
            self.parent.parent.trigger.finalize(move || {
                if let Some(receiver) = upgrade(&weak_receiver) {
                    (finalizer)(receiver);
                }
            }),
            reaction,
        )
    }

    pub fn do_async(
        self,
        reaction: impl Fn(Rc<Receiver>, Item) + Clone + 'static,
    ) -> SubscriptionWrapper<impl SubscriptionLike>
    where
        Item: 'static,
    {
        let weak_receiver = self.parent.weak_receiver.clone();
        let finalizer = self.finalizer;
        ReactionBuilderStepTwo::<Item, Trigger, Receiver>::do_async_internal(
            self.parent.weak_receiver,
            self.parent.parent.trigger.finalize(move || {
                let weak_receiver = weak_receiver.clone();
                let finalizer = finalizer.clone();
                Global::task_support()
                    .do_later_in_main_thread_from_main_thread_asap(move || {
                        if let Some(receiver) = upgrade(&weak_receiver) {
                            (finalizer)(receiver);
                        }
                    })
                    .unwrap();
            }),
            reaction,
        )
    }
}

fn upgrade<T>(weak_receiver: &Weak<T>) -> Option<Rc<T>> {
    let shared_receiver = weak_receiver.upgrade();
    if shared_receiver.is_none() {
        debug!(Reaper::get().logger(), "Receiver gone");
    }
    shared_receiver
}
