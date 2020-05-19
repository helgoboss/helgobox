use rxrust::observable::LocalObservable;
use std::rc::Rc;

pub trait ReactiveEvent = LocalObservable<'static, Err = ()> + 'static;
