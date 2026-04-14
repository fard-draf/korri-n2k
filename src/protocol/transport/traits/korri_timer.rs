//! Asynchronous timer abstraction providing the timing primitives required
//! by claim and retransmission logic.

/// Timer trait abstraction; must remain thread-safe when applicable.
pub trait KorriTimer {
    /// Asynchronously wait for `millis` milliseconds.
    async fn delay_ms(&mut self, millis: u32);
}

#[cfg(feature = "tokio")]
/// Default timer implementation for Tokio runtime.
pub struct TokioTimer;

#[cfg(feature = "tokio")]
impl KorriTimer for TokioTimer {
    async fn delay_ms(&mut self, millis: u32) {
        tokio::time::sleep(tokio::time::Duration::from_millis(millis as u64)).await;
    }
}
