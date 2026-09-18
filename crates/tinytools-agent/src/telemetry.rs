//! Feature-gated diagnostics for protocol recovery and replay decisions.

#[cfg(feature = "tracing")]
pub(crate) use tracing::{debug, warn};

#[cfg(not(feature = "tracing"))]
macro_rules! protocol_debug {
    ($($token:tt)*) => {{ let _ = stringify!($($token)*); }};
}

#[cfg(not(feature = "tracing"))]
pub(crate) use protocol_debug as debug;

#[cfg(not(feature = "tracing"))]
macro_rules! protocol_warn {
    ($($token:tt)*) => {{ let _ = stringify!($($token)*); }};
}

#[cfg(not(feature = "tracing"))]
pub(crate) use protocol_warn as warn;
