//! Error definitions shared across library modules.
//! Each type models a specific failure scenario (CAN ID construction,
//! address management, serialization/deserialization, etc.).
use crate::core::{FieldKind, PgnValue};
use thiserror_no_std::Error;

#[derive(Error, Debug)]
/// Errors that can occur while building a 29-bit CAN identifier.
pub enum CanIdBuildError {
    /// Provided parameters do not produce a valid identifier.
    #[error("Invalid data")]
    InvalidData,
    /// The destination address violates protocol constraints.
    #[error("Invalid destination")]
    InvalidDestination,
    /// Attempt to build a broadcast message (PDU2) with PF < 240.
    #[error("Invalid for broadcast message: PF is too low")]
    InvalidForBroadcast,
    /// Attempt to send an addressed message (PDU1) with PF ≥ 240.
    #[error("Invalid for addressed message: PF is too high: {pgn}")]
    InvalidForFocusedMessage { pgn: u8 },
    /// In PDU1 the lower 8 bits of the PGN must remain zero.
    #[error("PDU1 PGNs require PS = 0")]
    PsFocusMessageMustBeNull,
    /// No payload available to build the frame.
    #[error("Payload is empty: unable to build")]
    EmptyPayload,
}

#[derive(Error, Debug)]
/// Errors encountered while claiming or defending an address.
pub enum ClaimError<E: core::fmt::Debug> {
    /// CAN bus rejected the frame during transmission.
    #[error("CAN bus send error: {0:?}")]
    SendError(E),

    /// Unable to receive frames from the bus.
    #[error("CAN bus receive error: {0:?}")]
    ReceiveError(E),

    /// Another node claimed the same address with a higher-priority NAME.
    #[error("Network conflict")]
    NetworkConflict,

    /// No free address was available on the segment.
    #[error("No address available")]
    NoAddressAvailable,

    /// The received frame does not match the expected format.
    #[error("Invalid incoming frame")]
    InvalidIncomingFrame,

    /// Payload length is incompatible with the PGN definition.
    #[error("Invalid data length")]
    InvalidDataLen,

    /// Generic error propagated from the CAN layer.
    #[error("CAN bus error")]
    CanBusError,

    /// Failed to gather the information required to claim an address.
    #[error("Request address claim error")]
    RequestAddressClaimErr,

    /// Failed to extract business data.
    #[error(transparent)]
    Extraction(#[from] ExtractionError),

    /// Unable to build the CAN identifier.
    #[error(transparent)]
    BuildErr(#[from] CanIdBuildError),
}

#[derive(Debug, Error)]
/// Failures while extracting information from a raw CAN frame.
pub enum ExtractionError {
    /// The frame does not conform to the NMEA 2000 specification.
    #[error("Invalid incoming N2K frame")]
    InvalidIncomingFrame,
    /// Payload length does not match the PGN descriptor.
    #[error("Invalid data length for PGN")]
    InvalidDataLen,
}

//================================================================================CODEC_ERROR

#[derive(Debug, Error)]
/// Issues encountered while serializing a PGN into a buffer.
pub enum SerializationError {
    /// Provided buffer is too small for the payload.
    #[error("Buffer too small")]
    BufferTooSmall,
    /// Data does not satisfy the descriptor constraints.
    #[error("Invalid data")]
    InvalidData,
    /// Code generator detected a malformed repeating PGN definition.
    #[error("Invalid repetitive PGN definition for {data}")]
    RepeatitiveError { data: u32 },
    /// Field length is not an acceptable bit multiple.
    #[error("Invalid field bit length for {field_name}")]
    InvalidFieldBits { field_name: &'static str },
    /// Failed while writing bits into the output buffer.
    #[error("BitWrite error: {err}")]
    BitWriteError { err: BitWriterError },
    /// Field type not supported by the serialization engine.
    #[error("Unsupported field kind")]
    UnsupportedFieldKind,
    /// Expected field was missing from the domain structure.
    #[error("Field {field_id} not found")]
    FieldNotFound { field_id: &'static str },
    /// Generic conversion error bubbling up from the codec module.
    #[error("Codec Error: {source}")]
    CodecError { source: CodecError },
}

#[derive(Error, Debug)]
/// Errors raised while deserializing a CAN buffer into a PGN structure.
pub enum DeserializationError {
    /// Payload size does not match the expected schema.
    #[error("Invalid data length")]
    InvalidDataLength,
    /// Bits read from the buffer cannot be interpreted according to the descriptor.
    #[error("Malformed data")]
    MalformedData,
    /// Feature not implemented for this PGN yet.
    #[error("Functionality not implemented for this PGN")]
    NotImplemented,
    /// Indirect field depends on a lookup table that is missing.
    #[error("Missing Indirect Lookup Reference for descriptor {desc}: {pgn}")]
    MissingIndirectLookupRef { desc: u32, pgn: &'static str },
    /// Dependent field is missing or was not populated.
    #[error("Dependency field not found {dep} for pgn {desc}")]
    DependencyFieldNotFound { dep: &'static str, desc: u32 },
    /// Field kind not supported by the parser.
    #[error("Unsupported field kind {field_kind:?}")]
    UnsupportedFieldKind { field_kind: FieldKind },
    /// Could not assign value into the target structure.
    #[error("Field assignment failed {desc}")]
    FieldAssignmentFailed { desc: &'static str },
    /// Field descriptor defines an invalid bit length.
    #[error("Invalid field bit length for {field_name}")]
    InvalidFieldBits { field_name: &'static str },
    /// Error bubbled up from the generic codec engine.
    #[error("Codec Error: {source}")]
    CodecError { source: CodecError },
    /// Bit-level access on the buffer failed (out of bounds, misalignment…).
    #[error("BitReader error: {err}")]
    BitReaderError { err: BitReaderError },
}

#[derive(Error, Debug)]
/// Shared error abstraction for conversion helpers.
pub enum CodecError {
    /// Value type is incompatible with the algorithm.
    #[error("Data type mismatch for value {value:?}, function: {func}")]
    DataTypeMismatch { value: PgnValue, func: &'static str },
}

//==================================================================================SEND_ERROR
#[derive(Debug, Error)]
/// Errors encountered when sending a PGN (build + transmit).
pub enum SendPgnError<E: core::fmt::Debug> {
    /// PGN serialization failed.
    #[error("Serialization failed")]
    Serialization,
    /// CAN identifier could not be built.
    #[error("Frame build failed: {0:?}")]
    Build(CanIdBuildError),
    /// CAN layer refused or failed to send the frame.
    #[error("CAN bus send error: {0:?}")]
    Send(E),
}

//==================================================================================BITREADER_ERRORS
#[derive(Debug, Error)]
/// Errors raised during bitwise buffer reads.
pub enum BitReaderError {
    /// Attempted to read past the end of the buffer.
    #[error("Attempted to read out of bounds -> asked: {asked}, available: {available}")]
    OutOfBounds { asked: usize, available: usize },
    /// Requested more bits than the target type can hold.
    #[error("Cannot read more than {max} bits. Requested: {asked}")]
    TooLongForType { max: u8, asked: u8 },
    /// Cursor is not aligned on a byte boundary when required.
    #[error("Non aligned bit. Cursor: {cursor}")]
    NonAlignedBit { cursor: usize },
}
//==================================================================================BITREADER_ERRORS
#[derive(Debug, Error)]
/// Errors raised during bitwise writes into a buffer.
pub enum BitWriterError {
    /// Attempted to write beyond the provided capacity.
    #[error("Attempted to write out of bounds -> asked: {asked}, available: {available}")]
    OutOfBounds { asked: usize, available: usize },
    /// Field is too large for the provided type.
    #[error("Cannot write more than {max} bits. Requested: {asked}")]
    TooLongForType { max: u8, asked: u8 },
    /// Cursor is not aligned on a byte boundary when the operation requires it.
    #[error("Non aligned bit. Cursor: {cursor}")]
    NonAlignedBit { cursor: usize },
}
