pub mod grid_state;
pub mod interceptor;
pub mod parser;
pub mod placeholders;
pub mod replies;
pub mod store;
pub use grid_state::*;
pub use interceptor::*;
pub use parser::*;
pub use placeholders::*;
pub use replies::*;
pub use store::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KittyHostCapability {
    pub graphics: bool,
    pub zlib: bool,
}

/// No explicit placement id, including a default placeholder underline color.
pub const NO_PLACEMENT_ID: u32 = 0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KittyHostSupport {
    Supported,
    Unsupported,
    ProtocolDisabled,
}

impl KittyHostSupport {
    pub fn from_host_capability(supported: bool) -> Self {
        if supported {
            KittyHostSupport::Supported
        } else {
            KittyHostSupport::Unsupported
        }
    }
    pub fn protocol_is_enabled(&self) -> bool {
        *self != KittyHostSupport::ProtocolDisabled
    }
    pub fn host_supports_graphics(&self) -> bool {
        *self == KittyHostSupport::Supported
    }
}
