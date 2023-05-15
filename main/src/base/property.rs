//! In this file we assemble the a custom-tailored property type which we are going to use
//! throughout ReaLearn.
use rx_util::{LocalProp, LocalPropSubject, Notifier};
use rxrust::prelude::*;
use std::marker::PhantomData;

/// Creates a ReaLearn property.
pub fn prop<T>(initial_value: T) -> Prop<T>
where
    T: PartialEq + Clone + 'static,
{
    Prop::new(initial_value)
}

/// ReaLearn property type.
pub type Prop<T> = LocalProp<'static, T, u32, AsyncNotifier<Option<u32>>, AsyncNotifier<T>>;

pub struct AsyncNotifier<T>(PhantomData<T>);

impl<T> Notifier for AsyncNotifier<T>
where
    T: Clone + 'static,
{
    type T = T;
    type Subject = LocalPropSubject<'static, T>;

    fn notify(subject: &mut Self::Subject, value: &Self::T) {
        if subject.subscribed_size() == 0 {
            return;
        }
        #[cfg(test)]
        subject.next(value.clone());
        #[cfg(not(test))]
        {
            let mut subject = subject.clone();
            let value = value.clone();
            base::Global::task_support()
                .do_later_in_main_thread_from_main_thread_asap(move || {
                    subject.next(value);
                })
                .unwrap();
        }
    }
}
