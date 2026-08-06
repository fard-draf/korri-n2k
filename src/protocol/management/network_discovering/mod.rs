//! Network discovery service: send an ISO Request (PGN 59904) and collect
//! Address Claim responses (PGN 60928) to identify neighbouring nodes.
use crate::error::ClaimError;
use crate::error::ClaimError::SendError;
use crate::protocol::management::iso_name::IsoName;
use crate::protocol::management::network_discovering::engine::AddressRequester;
use crate::protocol::transport::can_frame::CanFrame;
use crate::protocol::transport::traits::{can_bus::CanBus, korri_timer::KorriTimer};
use futures_util::future::{select, Either};
use futures_util::pin_mut;
mod engine;

/// Broadcast a request and gather responses to enumerate devices.
pub async fn request_network_discovery<C: CanBus, T: KorriTimer>(
    can_bus: &mut C,
    timer: &mut T,
    discovered_devices: &mut [(u8, IsoName)],
) -> Result<usize, ClaimError<C::Error>>
where
    C::Error: core::fmt::Debug,
{
    let mut network_discover = AddressRequester::new(discovered_devices);
    let mut rx: Option<CanFrame> = None;

    loop {
        let now_ms = timer.now_ms();
        match network_discover.poll(now_ms, rx.as_ref()) {
            Ok(request_action) => match request_action {
                engine::RequestAction::Send(frame) => {
                    can_bus.send(&frame).await.map_err(SendError)?;
                    rx = None;
                }
                engine::RequestAction::Wait(delay) => {
                    let timer = timer.delay_ms(delay);
                    pin_mut!(timer);
                    let recv = can_bus.recv();
                    pin_mut!(recv);
                    match select(timer.as_mut(), recv).await {
                        Either::Left(_) => rx = None,
                        Either::Right((f, _)) => {
                            rx = Some(f.map_err(|e| ClaimError::ReceiveError(e))?);
                        }
                    };
                }
                engine::RequestAction::Done(device_count) => return Ok(device_count),
            },
            Err(e) => {
                return Err(ClaimError::Fault(e));
            }
        };
    }
}
