use crate::domain::MappingModel;
use std::cell::RefCell;
use std::rc::Rc;

pub type SharedMapping = Rc<RefCell<MappingModel>>;

pub fn share_mapping(mapping: MappingModel) -> SharedMapping {
    Rc::new(RefCell::new(mapping))
}
