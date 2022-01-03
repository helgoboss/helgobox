pub mod proto {
    include!(concat!(env!("OUT_DIR"), concat!("/realearn.rs")));
}

mod handlers;
mod server;

pub use server::*;
