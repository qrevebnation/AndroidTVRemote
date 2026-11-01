//! atv-remote core library

pub const APP_NAME: &str = "Google TV Remote";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod proto {
    pub mod polo {
        include!(concat!(env!("OUT_DIR"), "/polo.wire.protobuf.rs"));
    }
    pub mod remote {
        include!(concat!(env!("OUT_DIR"), "/remote.rs"));
    }
}
