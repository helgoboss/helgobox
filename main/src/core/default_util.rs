use helgoboss_learn::UnitValue;

pub fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    v == &T::default()
}

pub fn some_default<T: Default>() -> Option<T> {
    Some(T::default())
}

pub fn is_none_or_some_default<T: Default + PartialEq>(v: &Option<T>) -> bool {
    if let Some(i) = v {
        i == &T::default()
    } else {
        true
    }
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
