//! Here are some trait aliases for typical use cases.
//!
//! Those can be for example used in functions returning observables without exposing the internals.
//! E.g. instead of a `SharedSubject<i32, ()>`, one can return a `impl SharedEvent<i32>`.
//! In return positions, it's advisable to choose the maximum of guarantees that you are willing to
//! give, while keeping in mind the possibilities that you want to give to the consumer. Then
//! consumers are not restricted in what they can do with the observable while also not being
//! tempted to use things that are implementation-specific.
//!
//! The aliases can also be used in parameter positions. In that case it's advisable to take
//! exactly what you need to do on the observable in terms of operators and choose the alias
//! accordingly.

use rxrust::prelude::*;

/// Completely local event.
///
/// - `observe_on()`: no
/// - `delay()`: no
pub trait LocalEvent<I> = Observable<Item = I> + LocalObservable<'static, Err = ()> + 'static;

/// Local event with shared items.
///
/// - `observe_on()`: yes
/// - `delay()`: no
pub trait SharedItemEvent<I: SharedItem> =
    Observable<Item = I> + LocalObservable<'static, Err = ()> + 'static;

/// Local event with shared items.
///
/// - `observe_on()`: yes
/// - `delay()`: yes
pub trait SharedEvent<I: SharedItem> =
    SharedObservable<Unsub = SharedSubscription, Item = I, Err = ()> + 'static + Send + Sync;

/// The trait bounds of observable items it needs to be used for observe_on().
pub trait SharedItem = Send + Sync + 'static + Clone;
