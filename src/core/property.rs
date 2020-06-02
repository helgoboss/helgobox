//! In this file we assemble the a custom-tailored property type which we are going to use
//! throughout ReaLearn.
use reaper_high::Reaper;
use rx_util::{LocalProp, LocalPropSubject, Notifier};
use rxrust::prelude::*;

/// Creates a ReaLearn property.
pub fn prop<T>(initial_value: T) -> Prop<T>
where
    T: PartialEq,
{
    Prop::new(initial_value)
}

/// ReaLearn property type.
pub type Prop<T> = LocalProp<'static, T, AsyncNotifier>;

pub struct AsyncNotifier;

impl Notifier for AsyncNotifier {
    type Subject = LocalPropSubject<'static>;

    fn notify(subject: &mut Self::Subject) {
        let mut subject = subject.clone();
        Reaper::get().do_later_in_main_thread_asap(move || {
            subject.next(());
        });
    }
}
