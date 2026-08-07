/// Universal synchronous timer abstraction providing the instant timing primitive
/// required by claim and retransmission logic.
pub trait Clock {
    /// Milliseconds since an arbitrary but fixed origin.
    ///
    /// # Contract
    ///
    /// The engines store absolute deadlines built from these readings. Three
    /// properties are required.
    ///
    /// * **Monotonic.** A reading never decreases.
    /// * **Free of wall-clock adjustments.** An NTP step backwards would push a
    ///   deadline out of reach and stall a claim.
    /// * **Wrap-free while the engine lives.** Count from process or board boot.
    ///   Never from a truncated 32-bit tick counter. `u64` milliseconds cover
    ///   580 million years.
    ///
    /// Deadlines use `saturating_add`. A reading close to `u64::MAX` cannot
    /// panic. It pins the deadline to the maximum instead.
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
