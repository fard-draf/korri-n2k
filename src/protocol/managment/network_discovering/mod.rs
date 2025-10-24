//! Network discovery service: send an ISO Request (PGN 59904) and collect
//! Address Claim responses (PGN 60928) to identify neighbouring nodes.
use crate::error::ClaimError;
use crate::protocol::managment::address_claiming::extract_name_from_claim;
use crate::protocol::transport::can_frame::CanFrame;
use crate::protocol::transport::can_id::CanId;
use crate::protocol::transport::traits::{can_bus::CanBus, korri_timer::KorriTimer};
use futures_util::future::{select, Either};
use futures_util::pin_mut;

/// Broadcast a request and gather responses to enumerate devices.
pub async fn request_network_discovery<C: CanBus, T: KorriTimer>(
    can_bus: &mut C,
    timer: &mut T,
    discovered_devices: &mut [(u8, u64)],
) -> Result<usize, ClaimError<C::Error>>
where
    C::Error: core::fmt::Debug,
{
    // 1. Build the request frame.

    // We expect Address Claim PGN in response.
    let requested_pgn: u32 = 60928;

    // ISO Request payload stores the target PGN on 3 bytes.
    let mut data = [0xFFu8; 8]; // Remaining bytes padded with 0xFF.
    let pgn_bytes = requested_pgn.to_le_bytes();
    data[0..3].copy_from_slice(&pgn_bytes[0..3]);

    // Build the CAN frame using PGN 59904 (ISO Request).
    let request_frame = CanFrame {
        id: CanId::builder(59904, 255) // Source 255: global address.
            .to_destination(255)
            .with_priority(6) // Standard priority for network requests.
            .build()
            .map_err(|_| ClaimError::RequestAddressClaimErr)?,
        data,
        len: 3, // Only the first three bytes are meaningful.
    };

    // 2. Transmit the request.
    can_bus
        .send(&request_frame)
        .await
        .map_err(ClaimError::SendError)?;

    // 3. Listen for responses.

    let mut device_count = 0;
    // 300 ms window balances completeness and responsiveness.
    let listen_duration = timer.delay_ms(300);
    pin_mut!(listen_duration); // Pin the timer future.

    // Main listening loop.
    loop {
        let recv = can_bus.recv();
        pin_mut!(recv); // Pin the receive future.

        // `select` resolves with whichever future completes first (timer or receive).
        match select(listen_duration.as_mut(), recv).await {
            // Timer expired first.
            Either::Left(_) => {
                return Ok(device_count);
            }
            // Received a frame before expiry.
            Either::Right((incoming_frame, _)) => match incoming_frame {
                Ok(frame) => {
                    // Ensure the response is an Address Claim.
                    if frame.id.pgn() == 60928 {
                        // Extract the 64-bit NAME.
                        if let Ok(name) = extract_name_from_claim(&frame) {
                            let address = frame.id.source_address();
                            // Avoid overflowing the caller-provided buffer.
                            if device_count < discovered_devices.len() {
                                // Filter duplicates (some devices respond multiple times).
                                if !discovered_devices[0..device_count]
                                    .iter()
                                    .any(|(a, _)| *a == address)
                                {
                                    discovered_devices[device_count] = (address, name);
                                    device_count += 1;
                                }
                            }
                        }
                    }
                }
                Err(e) => return Err(ClaimError::ReceiveError(e)),
            },
        }
    }
}
