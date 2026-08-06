//! Validate address-management and network-discovery logic.
#[cfg(feature = "tokio")]
mod address_claiming_test;
mod address_manager_test;
#[cfg(all(feature = "embassy", not(feature = "tokio")))]
mod address_supervisor_test;

mod network_discovering_test;
#[cfg(feature = "tokio")]
mod supervisor_tokio_test;
