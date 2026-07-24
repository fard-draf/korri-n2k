use crate::{
    error::ClaimFault,
    protocol::{
        managment::address_claiming::{
            is_addr_capable_and_isoname_match, AddressClaimIterator, AddressClaimStrategy,
        },
        transport::can_frame::CanFrame,
    },
};

pub enum ClaimAction {
    Send(CanFrame),
    Wait(Option<u64>),
    Done(u8),
}

pub struct AddressClaimer<'a> {
    my_name: u64,
    addr_to_claim: Option<u8>,
    addr_iterator: AddressClaimIterator<'a>,
    deadline_ms: u64,
}

impl<'a> AddressClaimer<'a> {
    pub fn poll(
        &mut self,
        now_ms: u64,
        rx: Option<&CanFrame>,
        strategy: AddressClaimStrategy,
    ) -> Result<ClaimAction, ClaimFault> {
        if !is_addr_capable_and_isoname_match(self.my_name, strategy) {
            return Err(ClaimFault::InconsistentStrategy);
        }
        if let None = rx {
            return Ok(ClaimAction::Wait(Some(250)));
        }
        // Conflit
        if let Some(rx) = rx {
            Ok(ClaimAction::Send(rx.clone()))
        } else {
            Ok(ClaimAction::Wait(Some(self.deadline_ms - now_ms)))
        }
    }
}
