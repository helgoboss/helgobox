use helgoboss_learn::UnitValue;

pub enum Target {
    Todo,
}

impl helgoboss_learn::Target for Target {
    fn current_value(&self) -> UnitValue {
        // TODO
        UnitValue::MIN
    }

    fn step_size(&self) -> Option<UnitValue> {
        // TODO
        None
    }

    fn wants_increments(&self) -> bool {
        // TODO
        false
    }
}
