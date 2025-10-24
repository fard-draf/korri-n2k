//! Minimal abstraction for an asynchronous CAN bus. Allows the library to plug
//! into various implementations (embedded HAL, desktop driver, etc.).
use crate::protocol::transport::can_frame::CanFrame;
use futures_util::Future;

/// Contract to send and receive CAN frames asynchronously.
pub trait CanBus {
    type Error: core::fmt::Debug;
    /// Emit a frame on the bus. Asynchronous to accommodate non-blocking drivers.
    fn send<'a>(
        &'a mut self,
        frame: &'a CanFrame,
    ) -> impl Future<Output = Result<(), Self::Error>> + 'a;
    /// Retrieve the next available frame. Asynchronously waits until data arrives.
    fn recv<'a>(
        &'a mut self,
    ) -> impl core::future::Future<Output = Result<CanFrame, Self::Error>> + 'a;
}
