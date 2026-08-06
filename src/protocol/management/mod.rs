//! Network management logic: address claiming, current address tracking,
//! neighbour discovery, and NAME field manipulation.

#[cfg(all(feature = "embassy", feature = "tokio"))]
compile_error!("features `embassy` and `tokio` are mutually exclusive — enable only one");

pub mod address_claiming;
pub mod address_manager;
/// Only a runner drives the manager, and every runner is runtime-gated.
#[cfg(any(feature = "embassy", feature = "tokio"))]
pub mod address_supervisor;
pub mod iso_name;
pub mod network_discovering;
