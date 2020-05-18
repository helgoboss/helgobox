#[macro_export]
macro_rules! define_control_methods {
    ($type_: ty, [$($method_name: ident => $resource_id: expr,)+]) => {
        impl $type_ {
            $(
                fn $method_name(&self) -> $crate::Window {
                    self.require_window()
                        .require_control($resource_id)
                }
            )*
        }
    };
}
