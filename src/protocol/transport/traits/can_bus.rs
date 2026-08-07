//! Minimal abstraction for an asynchronous CAN bus. Allows the library to plug
//! into various implementations (embedded HAL, desktop driver, etc.).
use crate::protocol::transport::can_frame::CanFrame;

/// Contract to send and receive CAN frames asynchronously.
///
/// # Errors are terminal
///
/// **Any `Err` returned by [`CanBus::send`] or [`CanBus::recv`] stops
/// `AddressRunner` for good.** It surfaces as `AddressSupervisorRunError` and
/// `drive()` returns; nothing restarts it, and the node keeps no address.
///
/// The driver must therefore absorb what it can recover from: arbitration loss,
/// a full TX mailbox, a bus-off recovery cycle. Return an error only for a
/// condition the caller has to act on, such as a closed socket or a dead
/// peripheral.
///
/// The library deliberately does not retry. It cannot tell a transient failure
/// from a permanent one through an opaque `Error`, and a blind retry loop on a
/// dead bus is worse than stopping.
pub trait CanBus {
    type Error: core::fmt::Debug;

    /// Emit a frame on the bus. Asynchronous to accommodate non-blocking drivers.
    ///
    /// An `Err` is terminal for the runner: see the trait documentation.
    async fn send(&mut self, frame: &CanFrame) -> Result<(), Self::Error>;

    /// Retrieve the next available frame. Asynchronously waits until data arrives.
    ///
    /// An `Err` is terminal for the runner: see the trait documentation.
    ///
    /// # Cancellation safety
    ///
    /// **This future must be safe to drop.** The supervisor races it against a
    /// deadline and against queued commands, so it is dropped often: once per
    /// expired deadline, and once per command. An application publishing at
    /// 10 Hz therefore cancels a pending `recv` ten times a second.
    ///
    /// A driver that removes a frame from its hardware queue before the future
    /// resolves will lose that frame on every cancellation. If the lost frame is
    /// a competing Address Claim, the node keeps an address it no longer owns.
    ///
    /// Buffer inside your driver and return from the buffer, so a dropped future
    /// leaves the frame available to the next call.
    async fn recv(&mut self) -> Result<CanFrame, Self::Error>;
}
