//! Defines the "data contract" between `build.rs` (the scribe) and
//! the serialization/deserialization engine (the interpreter).
//!
//! `build.rs` generates static descriptors that implement this contract.
//! The `engine.rs` module consumes those descriptors to parse or build binary payloads.

// Types in this module are primarily used by generated code.
#![allow(dead_code)]

// Maximum payload size for PgnBytes. 223 bytes + safety margin.
pub const MAX_PGN_BYTES: usize = 230;

/// Semantic type of a field within a PGN.
/// Mirrors the `FieldType` entries found in `canboat.json`.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum FieldKind {
    /// Signed or unsigned integer; `is_signed` carries the distinction.
    Number,
    /// Floating-point value.
    Float,
    /// Value is an index into a dedicated enumeration.
    Lookup,
    /// Lookup resolved through another field's value.
    IndirectLookup,
    /// Bitfield where each individual bit is a flag.
    BitLookup,
    /// Encodes the Parameter Group Number controlling interactions (e.g. 126208, 130060, 59904…).
    Pgn,
    /// Date stored as a day count since Unix epoch (1970-01-01). 16 bits.
    Date,
    /// Time since midnight UTC. Resolution 0.0001 s. 32 bits.
    Time,
    /// Duration in seconds. Resolution depends on the source (16 or 32 bits).
    Duration,
    /// Maritime Mobile Service Identity. 32-bit unique identifier.
    Mmsi,
    /// Decimal string (BCD encoded).
    Decimal,
    /// Fixed-length ASCII string.
    StringFix,
    /// Variable-length string prefixed by a length byte and terminated by `\0`.
    StringLz,
    /// Variable-length string prefixed by length and encoding bytes (0 = Unicode, 1 = ASCII).
    StringLau,
    /// Raw binary block; length may be fixed or fill the remaining PGN space.
    Binary,
    /// Reserved bits to ignore at read time and set to `1` when writing.
    Reserved,
    /// Reserved block padded with zeros during writes (e.g. 129794 – AIS Class A Position Report).
    Spare,
    /// 64-bit field describing the device identity.
    /// (PGN 60928 – "ISO Address Claim") transports this unique `ISO_NAME`.
    IsoName,
    /// Placeholder for field types not supported yet.
    Unimplemented,
    // DYNAMIC_FIELD_KEY
    // DYNAMIC_FIELD_LENGTH
    // DYNAMIC_FIELD_VALUE
    // VARIABLE
    // FIELD_INDEX
}

/// Descriptor for a single PGN field.
#[derive(Debug)]
pub struct FieldDescriptor {
    /// 1. Field identifier.
    pub id: &'static str,
    /// 2. Human-readable name.
    pub name: &'static str,
    /// 3. Semantic type for the field.
    pub kind: FieldKind,
    /// 4. Field bit length.
    pub bits_length: Option<u32>,
    /// 5. Bit length for variable fields.
    pub bits_length_var: Option<u32>,
    /// 6. Absolute bit offset for the first bit.
    pub bits_offset: Option<u32>,
    /// 7. Indicates whether numbers are signed.
    pub is_signed: Option<bool>,
    /// 8. Resolution factor to apply, when relevant.
    pub resolution: Option<f32>,
    /// 9. Direct lookup enumeration identifier.
    pub enum_direct_name: Option<&'static str>,
    /// 10. Indirect lookup identifier.
    pub enum_indirect_name: Option<&'static str>,
    /// 11. Order index among indirect fields.
    pub enum_indirect_field_order: Option<u16>,
    /// 12. Physical unit (e.g. "m/s", "deg", "meters").
    pub physical_unit: Option<&'static str>,
    /// 13. Physical quantity (e.g. "GEOGRAPHICAL_LATITUDE", "SPEED").
    pub physical_qtity: Option<&'static str>,
}

/// Describes a repeating field set within a PGN.
///
/// Some NMEA 2000 PGNs contain groups of fields that repeat a variable number of times
/// (for example GNSS satellites or differential reference stations). they are identified in
/// `canboat.json` with the attributes:
/// - `RepeatingFieldSetNSize`: number of consecutive fields in the group
/// - `RepeatingFieldSetNStartField`: index of the first field
/// - `RepeatingFieldSetNCountField`: index of the field storing the repetition count
///
/// **Example:** PGN 129540 (GNSS Sats in View)
/// ```text
/// Field 4 (prn) = counter → number of satellites
/// Fields 5-11 (elevation, azimuth, snr…) = repeating group
/// → If prn = 5, fields 5-11 repeat five times
/// ```
#[derive(Debug)]
pub struct RepeatingFieldSet {
    /// Identifier of the repeating array in snake_case.
    ///
    /// Used by the `FieldAccess` trait when retrieving the array with
    /// `repetitive_field()` and `repetitive_field_mut()`.
    ///
    /// **Example:** `"reference_station_types"`, `"satellites"`
    pub array_id: &'static str,

    /// Index of the field that stores the repetition counter.
    ///
    /// This field must appear BEFORE the first repeating field.
    /// Its value determines how many times the group repeats.
    ///
    /// **Note:** `None` means the repetitions depend on the payload length (rare case, e.g. PGN 126464).
    pub count_field_index: Option<usize>,

    /// Index of the first field in the repeating group (0-based).
    ///
    /// The `size` fields starting from this index form the group.
    pub start_field_index: usize,

    /// Number of consecutive fields inside the repeating group.
    ///
    /// They are read/written sequentially for each iteration.
    pub size: usize,

    /// Maximum number of allowed repetitions.
    ///
    /// Determined by:
    /// - NMEA 2000 specification constraints (e.g. max sixteen satellites)
    /// - Fast Packet payload limit (223 bytes)
    /// - Static analysis performed by the code generator
    ///
    /// **Usage:** determines the array size in the generated Rust structure.
    pub max_repetitions: usize,
}

/// Descriptor for an entire PGN layout.
#[derive(Debug)]
pub struct PgnDescriptor {
    /// 1. PGN identifier.
    pub id: u32,
    /// 2. PGN name (diagnostics).
    pub name: &'static str,
    /// 3. User-facing description.
    pub description: &'static str,
    /// 4. Message priority.
    pub priority: Option<u8>,
    /// 5. Whether the message is Fast Packet or Single Frame.
    pub fastpacket: bool,
    /// 6. Payload length in bytes (if fixed).
    pub length: Option<u16>,
    /// 7. Number of field descriptors.
    pub field_count: Option<u8>,
    /// 8. Transmission interval.
    pub trans_interval: Option<u16>,
    /// 9. Whether the transmission interval is irregular.
    pub trans_irregular: Option<bool>,
    /// 10. Ordered list of field descriptors.
    pub fields: &'static [FieldDescriptor],
    /// 11. Repeating field sets (can be empty).
    ///
    /// A PGN may define up to three different repeating groups (RepeatingFieldSet1..3).
    pub repeating_field_sets: &'static [RepeatingFieldSet],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PgnBytes {
    pub len: usize,
    pub data: [u8; MAX_PGN_BYTES],
}

impl Default for PgnBytes {
    fn default() -> Self {
        Self {
            len: 0,
            data: [0; MAX_PGN_BYTES],
        }
    }
}

impl PgnBytes {
    /// Create an empty buffer.
    pub const fn new() -> Self {
        Self {
            len: 0,
            data: [0; MAX_PGN_BYTES],
        }
    }

    /// Number of valid bytes stored.
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Checks whether the buffer is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Reset the buffer.
    #[inline]
    pub fn clear(&mut self) {
        self.len = 0;
    }

    /// Copy bytes into the buffer and update `len`.
    #[inline]
    pub fn copy_from_slice(&mut self, slice: &[u8]) {
        let clamped = slice.len().min(MAX_PGN_BYTES);
        self.data[..clamped].copy_from_slice(&slice[..clamped]);
        self.len = clamped;
    }

    /// Immutable view over the populated bytes.
    #[inline]
    pub fn as_slice(&self) -> &[u8] {
        &self.data[..self.len]
    }

    /// Mutable view over the populated bytes.
    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.data[..self.len]
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PgnValue {
    U64(u64),
    U32(u32),
    U16(u16),
    U8(u8),
    I64(i64),
    I32(i32),
    I16(i16),
    I8(i8),
    F64(f64),
    F32(f32),
    Bytes(PgnBytes),
    Ignored,
}
