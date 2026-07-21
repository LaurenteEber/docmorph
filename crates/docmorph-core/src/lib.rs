//! Engine-neutral local policy, lifecycle, and adapter contracts.

pub mod adapter;
pub mod io;
pub mod lifecycle;
pub mod mock;
pub mod registry;

pub use adapter::{Adapter, AdapterOutput, AdapterRequest};
pub use io::InputPolicy;
pub use lifecycle::{Lifecycle, LifecycleFailure, LifecycleResult};
pub use mock::MockAdapter;
pub use registry::{Registry, RegistryError};
