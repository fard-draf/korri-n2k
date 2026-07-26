use crate::{
    error::ClaimFault,
    protocol::{
        managment::address_claiming::{
            build_address_claim_frame, is_addr_capable_and_isoname_match, AddressClaimIterator,
            AddressClaimStrategy,
        },
        transport::can_frame::CanFrame,
    },
};

pub enum ClaimAction {
    Send(CanFrame),
    Wait(Option<u64>),
    Done(u8),
}

#[derive(PartialEq, Eq)]
enum State {
    Idle,
    Listening { frame: CanFrame, deadline_ms: u64 },
}

pub struct AddressClaimer<'a> {
    my_name: Option<u64>,
    addr_iterator: Option<AddressClaimIterator<'a>>,
    state: State,
    is_first: bool,
}
#[allow(dead_code)]
impl<'a> AddressClaimer<'a> {
    pub fn new(frame: CanFrame) -> Self {
        AddressClaimer {
            my_name: None,
            state: State::Listening {
                frame,
                deadline_ms: 250,
            },
            addr_iterator: None,
            is_first: true,
        }
    }

    pub fn poll(
        &mut self,
        now_ms: u64,
        rx: Option<&CanFrame>,
        strategy: AddressClaimStrategy,
    ) -> Result<ClaimAction, ClaimFault> {
        if let Some(my_name) = self.my_name {
            // not started | first call
            // guard
            if !is_addr_capable_and_isoname_match(my_name, strategy) {
                return Err(ClaimFault::InconsistentStrategy);
            }
            match self.state {
                State::Idle => ,
                State::Listening { frame, deadline_ms } => todo!(),
            }
            if self.state == State::Listening && self.is_first {
                if let Some(addr_to_claim) = self.state {
                    self.is_first_call = false;
                    let claim_frame = build_address_claim_frame(my_name, addr_to_claim)
                        .map_err(ClaimFault::BuildErr)?;
                    return Ok(ClaimAction::Send(claim_frame));
                }
            }
            // listen     | rx == None && now <  deadline
            if !self.is_first_call && !self.is_ended && now_ms < self.deadline_ms && rx.is_none() {
                self.deadline_ms -= now_ms;
                return Ok(ClaimAction::Wait(Some(self.deadline_ms)));
            }
            // listen     | rx == None && now >= deadline
            if !self.is_first_call && !self.is_ended && now_ms >= self.deadline_ms && rx.is_none() {
                self.deadline_ms -= now_ms;
            }

            // listen     | frame without conflict
            // listen     | conflict >> my_name wins
            // listen     | conflict >> my_name loses
            // listen     | conflict >> lose, no more address available
            if let None = rx {
                return Ok(ClaimAction::Wait(Some(250)));
            }
            // Conflit
            if let Some(rx) = rx {
                Ok(ClaimAction::Send(rx.clone()))
            } else {
                Ok(ClaimAction::Wait(Some(self.deadline_ms - now_ms)))
            }
        } else {
            // TODO: create a better err
            Err(ClaimFault::InconsistentStrategy)
        }
    }
}
