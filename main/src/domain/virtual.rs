use helgoboss_learn::UnitValue;

#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash)]
pub struct VirtualTarget {
    control_element: VirtualControlElement,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash)]
pub struct VirtualSource {
    control_element: VirtualControlElement,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash)]
pub enum VirtualControlElement {
    Range(u32),
    Button(u32),
}
