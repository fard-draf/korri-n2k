//! `CanBus` and `KorriTimer` over embassy-stm32 FDCAN, from korri-n2k-examples
//! (`arm/stm32/g431-cbu6/src/ports.rs`), without the address-claim logging.

use defmt::trace;
use embassy_stm32::can::{
    self, Frame as HalFrame,
    enums::{BusError, FrameCreateError},
};
use embassy_time::{Duration, Instant, Timer};
use embedded_can::Id;
use korri_n2k::protocol::transport::{
    can_frame::CanFrame,
    can_id::CanId,
    traits::{
        can_bus::CanBus,
        korri_timer::{Clock, KorriTimer},
    },
};

pub struct Stm32CanBus<'d, const TX_BUF: usize, const RX_BUF: usize> {
    inner: can::BufferedCan<'d, TX_BUF, RX_BUF>,
}

impl<'d, const TX_BUF: usize, const RX_BUF: usize> Stm32CanBus<'d, TX_BUF, RX_BUF> {
    pub fn new(inner: can::BufferedCan<'d, TX_BUF, RX_BUF>) -> Self {
        Self { inner }
    }
}

#[derive(Debug)]
pub enum Stm32CanError {
    Bus(BusError),
    FrameCreate(FrameCreateError),
}

impl From<FrameCreateError> for Stm32CanError {
    fn from(value: FrameCreateError) -> Self {
        Self::FrameCreate(value)
    }
}

impl defmt::Format for Stm32CanError {
    fn format(&self, fmt: defmt::Formatter<'_>) {
        match self {
            Stm32CanError::Bus(_) => defmt::write!(fmt, "Bus"),
            Stm32CanError::FrameCreate(_) => defmt::write!(fmt, "FrameCreate"),
        }
    }
}

impl<'d, const TX_BUF: usize, const RX_BUF: usize> CanBus for Stm32CanBus<'d, TX_BUF, RX_BUF> {
    type Error = Stm32CanError;

    async fn send(&mut self, frame: &CanFrame) -> Result<(), Self::Error> {
        let hal_frame = HalFrame::new_extended(frame.id.0, &frame.data[..frame.len])
            .map_err(Self::Error::from)?;
        trace!("HAL TX id=0x{=u32:X} len={}", frame.id.0, frame.len);
        self.inner.write(hal_frame).await;
        Ok(())
    }

    async fn recv(&mut self) -> Result<CanFrame, Self::Error> {
        loop {
            match self.inner.read().await {
                Ok(envelope) => {
                    let hal_frame = envelope.frame;
                    let raw_id = match hal_frame.id() {
                        Id::Extended(id) => id.as_raw(),
                        Id::Standard(_) => continue, // not N2K
                    };
                    let payload = hal_frame.data();
                    let mut data = [0u8; 8];
                    data[..payload.len()].copy_from_slice(payload);
                    return Ok(CanFrame {
                        id: CanId(raw_id),
                        data,
                        len: payload.len(),
                    });
                }
                // Transient bus errors: retry rather than ending the claim or the runner.
                Err(_) => defmt::warn!("CAN rx soft error (ignored)"),
            }
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Stm32Timer;

impl Clock for Stm32Timer {
    fn now_ms(&self) -> u64 {
        Instant::now().as_millis()
    }
}

impl KorriTimer for Stm32Timer {
    async fn delay_ms(&mut self, millis: u32) {
        Timer::after(Duration::from_millis(millis as u64)).await;
    }
}
