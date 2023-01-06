extern crate proc_macro;

/// No-op derive macro for making certain no-op attribute macros available.
///
/// Not used at the moment but could come in very handy for Rust-to-Dart code generation.
#[proc_macro_derive(Playtime, attributes(label))]
pub fn playtime(_: proc_macro::TokenStream) -> proc_macro::TokenStream {
    proc_macro::TokenStream::default()
}
