use crate::domain::MappingModel;
use std::cell::RefCell;
use std::rc::Rc;

pub type SharedMappingModel = Rc<RefCell<MappingModel>>;
