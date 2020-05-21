use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// A simple mock which counts the number of invocations and remembers the last argument.
pub struct InvocationMock<O: Clone> {
    count: Cell<u32>,
    last_arg: RefCell<Option<O>>,
}

impl<O: Clone> InvocationMock<O> {
    pub fn invoke(&self, arg: O) {
        self.count.replace(self.count.get() + 1);
        self.last_arg.replace(Some(arg));
    }

    /// Returns how many times `invoke()` has been called.
    pub fn invocation_count(&self) -> u32 {
        self.count.get()
    }

    /// Returns a copy of the last argument passed to `invoke()`.
    ///
    /// # Panics
    ///
    /// Panics if there was no invocation at all.
    pub fn last_arg(&self) -> O {
        self.last_arg
            .borrow()
            .clone()
            .expect("There were no invocations")
    }
}

/// Executes the given closure `op`, passing it a shared invocation mock.
///
/// The `op` closure can take ownership of the invocation mock and pass it to another 'static
/// closure, which is supposed to call the invocation mock. It also returns a pointer to the same
/// mock, which can be used later to check the number of invocations and the last passed argument.
pub fn observe_invocations<O: Clone, R>(
    op: impl FnOnce(Rc<InvocationMock<O>>) -> R,
) -> (Rc<InvocationMock<O>>, R) {
    let (mock_one, mock_two) = create_invocation_mock();
    (mock_one, op(mock_two))
}

/// Creates two pointers referring to the same invocation mock.
///
/// This is useful for testing 'static closures, that is, closures which need captured references to
/// be 'static. Which makes tests like "this closure did run n times" non-trivial. So instead of
/// using references, we can use shared pointers.
pub fn create_invocation_mock<O: Clone>() -> (Rc<InvocationMock<O>>, Rc<InvocationMock<O>>) {
    let mock = InvocationMock {
        count: Cell::new(0),
        last_arg: RefCell::new(None),
    };
    let shared_mock = Rc::new(mock);
    (shared_mock.clone(), shared_mock)
}
