use crate::{
    error::ClaimFault,
    protocol::{
        constants::{addr_mgmt_pgns, address, iso_delay::REQUEST_DELAY_MS},
        management::{address_claiming::extract_name_from_claim, iso_name::IsoName},
        transport::{can_frame::CanFrame, can_id::CanId},
    },
};

#[derive(PartialEq, Eq)]
pub enum RequestAction {
    Send(CanFrame),
    Wait(u32),
    Done(usize),
}

#[derive(PartialEq, Eq)]
pub enum State {
    Idle,
    Listening {
        device_count: usize,
        deadline_ms: u64,
    },
}

pub struct AddressRequester<'a> {
    state: State,
    source_address: u8,
    discovered_devices: &'a mut [(u8, IsoName)],
}

impl<'a> AddressRequester<'a> {
    /// `source_address` must be an address the node actually holds, from
    /// `claimed_address()`. A request is an ordinary addressed message: 255 is
    /// the broadcast destination, never a valid source.
    pub fn new(source_address: u8, discovered_devices: &'a mut [(u8, IsoName)]) -> Self {
        Self {
            state: State::Idle,
            source_address,
            discovered_devices,
        }
    }

    pub fn poll(
        &mut self,
        now_ms: u64,
        rx: Option<&CanFrame>,
    ) -> Result<RequestAction, ClaimFault> {
        match self.state {
            State::Idle => {
                let mut data = [0xFFu8; 8];
                let pgn_bytes = addr_mgmt_pgns::CLAIM_PGN_60928.to_le_bytes();
                data[0..3].copy_from_slice(&pgn_bytes[0..3]);

                let request_frame = CanFrame {
                    id: CanId::builder(addr_mgmt_pgns::REQUEST_PGN_59904, self.source_address)
                        .to_destination(address::GLOBAL)
                        .with_priority(6)
                        .build()
                        .map_err(|_| ClaimFault::RequestAddressClaimErr)?,
                    data,
                    len: 3,
                };

                self.state = State::Listening {
                    device_count: 0,
                    // Same temporal contract as the claim engine: a clock
                    // reading near `u64::MAX` pins the deadline, never panics.
                    deadline_ms: now_ms.saturating_add(REQUEST_DELAY_MS as u64),
                };
                Ok(RequestAction::Send(request_frame))
            }
            State::Listening {
                mut device_count,
                deadline_ms,
            } => {
                let remaining_ms = deadline_ms.saturating_sub(now_ms);
                if remaining_ms == 0 {
                    self.state = State::Idle;
                    return Ok(RequestAction::Done(device_count));
                }

                let Some(frame) = rx else {
                    return Ok(RequestAction::Wait(remaining_ms as u32));
                };

                if frame.id.pgn() != addr_mgmt_pgns::CLAIM_PGN_60928 {
                    return Ok(RequestAction::Wait(remaining_ms as u32));
                }
                let Ok(name) = extract_name_from_claim(frame) else {
                    return Ok(RequestAction::Wait(remaining_ms as u32));
                };
                let address = frame.id.source_address();
                if device_count < self.discovered_devices.len() {
                    if !self.discovered_devices[0..device_count]
                        .iter()
                        .any(|(a, _)| *a == address)
                    {
                        self.discovered_devices[device_count] = (address, name);
                        device_count += 1;
                    }
                    self.state = State::Listening {
                        device_count,
                        deadline_ms,
                    };
                    return Ok(RequestAction::Wait(remaining_ms as u32));
                }
                // TODO!: add defmt warn -> if the buffer is full or empty, frame is ignored.
                Ok(RequestAction::Wait(remaining_ms as u32))
            }
        }
    }
}
