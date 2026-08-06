//! Minimal abstraction for an asynchronous CAN bus. Allows the library to plug
//! into various implementations (embedded HAL, desktop driver, etc.).
use crate::protocol::transport::can_frame::CanFrame;

/// Contract to send and receive CAN frames asynchronously.
pub trait CanBus {
    type Error: core::fmt::Debug;
    /// Emit a frame on the bus. Asynchronous to accommodate non-blocking drivers.
    async fn send(&mut self, frame: &CanFrame) -> Result<(), Self::Error>;
    /// Retrieve the next available frame. Asynchronously waits until data arrives.
    async fn recv(&mut self) -> Result<CanFrame, Self::Error>;
}
