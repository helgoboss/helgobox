use helgoboss_learn::UnitValue;

pub fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    v == &T::default()
}

pub fn bool_true() -> bool {
    true
}

pub fn is_bool_true(v: &bool) -> bool {
    *v
}

pub fn unit_value_one() -> UnitValue {
    UnitValue::MAX
}

pub fn is_unit_value_one(v: &UnitValue) -> bool {
    *v == UnitValue::MAX
}
