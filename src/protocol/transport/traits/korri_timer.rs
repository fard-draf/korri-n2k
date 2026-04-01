//! Asynchronous timer abstraction providing the timing primitives required
//! by claim and retransmission logic.

/// Timer trait abstraction; must remain thread-safe when applicable.
pub trait KorriTimer {
    /// Asynchronously wait for `millis` milliseconds.
    async fn delay_ms(&mut self, millis: u32);
}
