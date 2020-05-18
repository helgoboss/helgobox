#[macro_export]
macro_rules! define_control_methods {
    ($r#type: ty, $($resource_id: expr => $method_name: ident),+) => {
        impl r#type {}
    };
}
