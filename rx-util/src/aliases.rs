use rxrust::prelude::*;

pub trait LocalReactiveEvent<I> =
    Observable<Item = I> + LocalObservable<'static, Err = ()> + 'static;

// The trait bounds of observable items it needs to be used in a SharedReactiveEvent.
pub trait SharedReactiveItem = Send + Sync + 'static + Clone;

// The trait bounds it needs for to_shared(), observe_on() and delay() to be used.
pub trait SharedReactiveEvent<I: SharedReactiveItem> =
    SharedObservable<Unsub = SharedSubscription, Item = I, Err = ()> + 'static + Send + Sync;
