//! Registry-facing adapter contracts. Local I/O policy and lifecycle follow in a later work unit.

pub mod adapter;
pub mod registry;

pub use adapter::{Adapter, MockAdapter};
pub use registry::{Registry, RegistryError};
