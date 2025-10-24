//! `CanBus` extension providing a high-level API to send PGNs: it serializes
//! the structure, builds Fast Packet frames when needed, and transmits them in
//! sequence with the appropriate inter-frame delays.
//!
//! # Fast Packet inter-frame delay
//!
//! When a Fast Packet message spans multiple frames, a configurable delay is
//! inserted between consecutive frames to prevent embedded CAN controllers
//! from saturating their TX buffers.
//!
//! The default delay is defined by [`FAST_PACKET_INTER_FRAME_DELAY_MS`].
use crate::{
    error::SendPgnError,
    infra::codec::traits::PgnData,
    protocol::transport::fast_packet::{builder::FastPacketBuilder, MAX_FAST_PACKET_PAYLOAD},
    protocol::transport::traits::{can_bus::CanBus, korri_timer::KorriTimer},
    protocol::transport::FAST_PACKET_INTER_FRAME_DELAY_MS,
};

/// Trait extending `CanBus` with ergonomic PGN-sending helpers.
///
/// Provides convenience methods to send NMEA 2000 messages with automatic serialization,
/// Fast Packet segmentation, and inter-frame delays.
pub trait PgnSender: CanBus
where
    <Self as CanBus>::Error: core::fmt::Debug,
{
    /// Serialize, segment, and send a PGN over the CAN bus.
    ///
    /// Transparently handles:
    /// - **Single-frame PGNs** (≤ 8 bytes): sent as a single CAN frame.
    /// - **Fast Packet PGNs** (> 8 bytes): automatically segmented into multiple frames.
    ///
    /// # Inter-frame delay
    ///
    /// Multi-frame Fast Packet transmissions insert a delay between frames to avoid TX buffer
    /// saturation. The delay uses the supplied `timer`.
    ///
    /// # Arguments
    ///
    /// * `pgn_data` – PGN data structure implementing [`PgnData`]
    /// * `pgn` – Parameter Group Number
    /// * `source_address` – Source address (0-253)
    /// * `destination` – Optional destination (None = broadcast)
    /// * `timer` – Timer to enforce inter-frame delays
    ///
    /// # Errors
    ///
    /// Returns:
    /// - [`SendPgnError::Serialization`] when serialization fails
    /// - [`SendPgnError::Build`] when frame construction fails
    /// - [`SendPgnError::Send`] when bus transmission fails
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use korri_n2k::protocol::{
    ///     messages::Pgn127503,
    ///     transport::traits::pgn_sender::PgnSender,
    /// };
    ///
    /// let mut pgn = Pgn127503::new();
    /// pgn.instance = 0;
    /// pgn.number_of_lines = 1;
    ///
    /// // Send with automatic inter-frame delays
    /// can_bus.send_pgn(&pgn, 127503, my_address, None, &mut timer).await?;
    /// ```
    fn send_pgn<'a, P: PgnData, T: KorriTimer>(
        &'a mut self,
        pgn_data: &'a P,
        pgn: u32,
        source_address: u8,
        destination: Option<u8>,
        timer: &'a mut T,
    ) -> impl core::future::Future<Output = Result<(), SendPgnError<Self::Error>>> + 'a;
}

impl<C: CanBus> PgnSender for C
where
    C::Error: core::fmt::Debug,
{
    fn send_pgn<'a, P: PgnData, T: KorriTimer>(
        &'a mut self,
        pgn_data: &'a P,
        pgn: u32,
        source_address: u8,
        destination: Option<u8>,
        timer: &'a mut T,
    ) -> impl core::future::Future<Output = Result<(), SendPgnError<Self::Error>>> + 'a {
        async move {
            // Step 1: stack-allocate a buffer to avoid heap usage.
            let mut payload_buffer = [0u8; MAX_FAST_PACKET_PAYLOAD];

            // Step 2: serialize the PGN into the buffer.
            let len = pgn_data
                .to_payload(&mut payload_buffer)
                .map_err(|_| SendPgnError::Serialization)?;
            let payload_slice = &payload_buffer[..len];

            // Step 3: prepare the Fast Packet (or single-frame) builder.
            let builder = FastPacketBuilder::new(pgn, source_address, destination, payload_slice);

            // Step 4: send every frame sequentially with inter-frame delays when required.
            let frame_iter = builder.build();
            let mut is_first_frame = true;

            for frame_result in frame_iter {
                let frame = frame_result.map_err(SendPgnError::Build)?;

                // For multi-frame Fast Packets insert a delay between frames
                // (skip before the first frame to minimize latency).
                if !is_first_frame && payload_slice.len() > 8 {
                    // Recommended inter-frame delay to avoid TX buffer saturation
                    timer.delay_ms(FAST_PACKET_INTER_FRAME_DELAY_MS).await;
                }

                // Send the CAN frame
                self.send(&frame).await.map_err(SendPgnError::Send)?;

                is_first_frame = false;
            }

            Ok(())
        }
    }
}
