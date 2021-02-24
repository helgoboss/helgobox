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

/// Should only be used when the deserialization checks the data version number because only that
/// way it can check if `None` represents the old default or the new one!
pub fn is_none_or_some_default<T: Default + PartialEq>(v: &Option<T>) -> bool {
    if let Some(i) = v {
        i == &T::default()
    } else {
        true
    }
}
