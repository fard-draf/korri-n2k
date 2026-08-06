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
pub struct TokioTimer {
    start: tokio::time::Instant,
}

#[cfg(feature = "tokio")]
impl TokioTimer {
    pub fn new() -> Self {
        Self {
            start: tokio::time::Instant::now(),
        }
    }
}

#[cfg(feature = "tokio")]
impl Default for TokioTimer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "tokio")]
impl Clock for TokioTimer {
    fn now_ms(&self) -> u64 {
        self.start.elapsed().as_millis() as u64
    }
}

#[cfg(feature = "tokio")]
impl KorriTimer for TokioTimer {
    async fn delay_ms(&mut self, millis: u32) {
        tokio::time::sleep(tokio::time::Duration::from_millis(millis as u64)).await;
    }
}
