/// Universal synchronous timer abstraction providing the instant timing primitive
/// required by claim and retransmission logic.
pub trait Clock {
    fn now_ms(&self) -> u64;
}

/// Asynchronous timer abstraction to handle delay.
/// Timer trait abstraction; must remain thread-safe when applicable.
pub trait KorriTimer: Clock {
    /// Asynchronously wait for `millis` milliseconds.
    async fn delay_ms(&mut self, millis: u32);
}

#[cfg(feature = "tokio")]
/// Default timer implementation for Tokio runtime.
pub struct TokioTimer;

#[cfg(feature = "tokio")]
impl Clock for TokioTimer {
    fn now_ms(&self) -> u64 {
        tokio::time::Instant::now()
    }
}

#[cfg(feature = "tokio")]
impl KorriTimer for TokioTimer {
    async fn delay_ms(&mut self, millis: u32) {
        tokio::time::sleep(tokio::time::Duration::from_millis(millis as u64)).await;
    }
}
