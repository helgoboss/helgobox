use crate::domain::Session;
use std::cell::{Ref, RefCell, RefMut};
use std::rc::Rc;

#[derive(Clone, Debug)]
pub struct SessionContext {
    session: Rc<RefCell<Session<'static>>>,
}

impl SessionContext {
    pub fn new(session: Rc<RefCell<Session<'static>>>) -> SessionContext {
        Self { session }
    }

    pub fn get(&self) -> Ref<Session<'static>> {
        self.session.borrow()
    }

    pub fn get_mut(&self) -> RefMut<Session<'static>> {
        self.session.borrow_mut()
    }
}
