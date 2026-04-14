//! Network management logic: address claiming, current address tracking,
//! neighbour discovery, and NAME field manipulation.
pub mod address_claiming;
pub mod address_manager;
#[cfg(all(feature = "embassy", not(feature = "tokio")))]
#[path = "supervisor_embassy.rs"]
pub mod address_supervisor;

#[cfg(feature = "tokio")]
#[path = "supervisor_tokio.rs"]
pub mod address_supervisor;
pub mod iso_name;
pub mod network_discovering;
