use rxrust::observable::{LocalObservable, Observable};
use std::rc::Rc;
use std::sync::Arc;

pub type SharedView<V> = Rc<V>;
pub type WeakView<V> = std::rc::Weak<V>;
pub trait ReactiveEvent<T> = Observable<Item = T> + LocalObservable<'static, Err = ()> + 'static;
