//! Asynchronous timer abstraction providing the timing primitives required
//! by claim and retransmission logic.

/// Timer trait abstraction; must remain thread-safe when applicable.
pub trait KorriTimer {
    /// Asynchronously wait for `millis` milliseconds.
    fn delay_ms<'a>(
        &'a mut self,
        millis: u32,
    ) -> impl core::future::Future<Output = ()> + 'a;
}
