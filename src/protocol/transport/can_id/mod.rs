//! Creation and extraction of the 29-bit CAN identifiers used by
//! NMEA 2000 (derived from the SAE J1939 specification).
use crate::error::CanIdBuildError;

// Define, build, and decompose an NMEA 2000 CAN identifier.

//==================================================================================CAN_ID
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Encapsulates an extended CAN identifier (29 bits) and exposes accessors
/// for priority, PGN, destination, and source.
pub struct CanId(pub u32);

impl CanId {
    // Builder entry point
    /// Creates a pre-configured `CanIdBuilder` for a PGN and source address.
    pub fn builder(pgn: u32, source_address: u8) -> CanIdBuilder {
        CanIdBuilder::new(pgn, source_address)
    }

    // Getters used to deconstruct the identifier
    /// Returns the priority (3 bits, value 0-7) encoded in the CAN ID.
    pub fn priority(&self) -> u8 {
        ((self.0 >> 26) & 0x07) as u8
    }
    /// Extracts the 18-bit PGN, handling the PDU1/PDU2 distinction.
    pub fn pgn(&self) -> u32 {
        let ps = ((self.0 >> 8) & 0xFF) as u8;
        let pf = ((self.0 >> 16) & 0xFF) as u8;
        let dp = (self.0 >> 24) & 0x01;
        let r = (self.0 >> 25) & 0x01;

        if (pf >> 4) & 0xF == 0xF {
            // PDU2: PF >= 240, implicit destination, PS becomes part of the PGN.
            (r << 17) | (dp << 16) | ((pf as u32) << 8) | (ps as u32)
        } else {
            // PDU1: PF < 240, PS stores the explicit destination.
            (r << 17) | (dp << 16) | ((pf as u32) << 8)
        }
    }

    /// Returns the destination address (PDU1) when the PGN requires one.
    pub fn destination(&self) -> Option<u8> {
        let pf = ((self.0 >> 16) & 0xFF) as u8;
        if (pf >> 4) & 0xF == 0xF {
            None
        } else {
            let ps = ((self.0 >> 8) & 0xFF) as u8;
            Some(ps)
        }
    }

    /// Eight-bit source address (logical node identifier on the N2K network).
    pub fn source_address(&self) -> u8 {
        (self.0 & 0xFF) as u8
    }
}
//==================================================================================CAN_ID_BUILDER
#[derive(Debug)]
/// Fluent builder that enforces the PDU1/PDU2 rules.
pub struct CanIdBuilder {
    pub priority: u8,
    pub pgn: u32,
    pub source_address: u8,
    pub destination: Option<u8>,
}

impl CanIdBuilder {
    /// Initializes the builder for a given PGN and source address.
    pub fn new(pgn: u32, source_address: u8) -> Self {
        Self {
            priority: 6, // Default priority
            pgn,
            source_address,
            destination: None,
        }
    }

    /// Sets the priority (3 bits) to use during construction.
    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority & 0x07;
        self
    }

    /// Assigns a destination address (PDU1). Implies a directed message.
    pub fn to_destination(mut self, destination_address: u8) -> Self {
        self.destination = Some(destination_address);
        self
    }
    // Fluent setter-style helpers
    /// Equivalent to `with_priority`, kept for API compatibility.
    pub fn priority(mut self, priority: u8) -> Self {
        self.priority = priority & 0x07;
        self
    }

    /// Sets the destination (PDU1), overriding any previous value.
    pub fn destination(mut self, dest: u8) -> Self {
        self.destination = Some(dest);
        self
    }

    /// Builds the CAN identifier while applying J1939 rules:
    /// - PF < 240 → addressed message (PDU1): `destination` mandatory and PGN PS byte must be `0`
    /// - PF ≥ 240 → broadcast (PDU2): `destination` must not be provided
    /// - R/DP/PF/PS bits are copied from the provided PGN
    ///
    /// Returns a dedicated error when the configuration violates these rules.
    pub fn build(self) -> Result<CanId, CanIdBuildError> {
        let r_from_pgn = (self.pgn >> 17) & 0x01;
        let dp_from_pgn = (self.pgn >> 16) & 0x01;
        let pf_from_pgn = ((self.pgn >> 8) & 0xFF) as u8;
        let ps_from_pgn = (self.pgn & 0xFF) as u8;

        match self.destination {
            None => {
                if pf_from_pgn < 240 {
                    return Err(CanIdBuildError::InvalidForBroadcast);
                }
                let id = ((self.priority as u32) << 26)
                    | (r_from_pgn << 25)
                    | (dp_from_pgn << 24)
                    | ((pf_from_pgn as u32) << 16)
                    | ((ps_from_pgn as u32) << 8)
                    | (self.source_address as u32);
                Ok(CanId(id))
            }

            Some(da) => {
                // println!("self.pgn[pf] -> {:?}", ((self.pgn >> ))
                if pf_from_pgn >= 240 {
                    return Err(CanIdBuildError::InvalidForFocusedMessage { pgn: pf_from_pgn });
                }
                if ps_from_pgn != 0 {
                    return Err(CanIdBuildError::PsFocusMessageMustBeNull);
                }
                let id = ((self.priority as u32) << 26)
                    | (r_from_pgn << 25)
                    | (dp_from_pgn << 24)
                    | ((pf_from_pgn as u32) << 16)
                    | ((da as u32) << 8)
                    | (self.source_address as u32);
                Ok(CanId(id))
            }
        }
    }
}
//==================================================================================TESTS
#[cfg(test)]
#[path = "tests.rs"]
mod tests;
