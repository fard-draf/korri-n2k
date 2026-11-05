//! Generic serialization/deserialization engine driven by compile-time PGN descriptors.
//! It controls the bit-level readers/writers and turns dynamic `PgnValue`s into
//! strongly typed domain structures.
use super::bits::{BitReader, BitWriter};
use super::traits::FieldAccess;
use crate::core::{
    Availability, FieldDescriptor, FieldKind, PgnBytes, PgnDescriptor, PgnValue, RepeatingFieldSet,
    MAX_PGN_BYTES,
};
use crate::error::{CodecError, DeserializationError, SerializationError};

/// One field reading emitted by [`for_each_field`].
#[derive(Debug, Clone, PartialEq)]
pub struct FieldReading {
    /// Scaled value, identical to what `FieldAccess::field` returns.
    pub value: PgnValue,
    /// Raw unsigned bit pattern on the wire (`None` for strings and byte-aligned binary).
    pub raw: Option<u64>,
    /// Availability determined from CANboat sentinels (or trimmed `STRING_FIX`).
    pub availability: Availability,
    /// `(array_id, index)` when the field belongs to a repeating group.
    pub repetition: Option<(&'static str, usize)>,
}

/// Pads a payload between `min_length` and `length` with `0xFF` ("not available").
fn pad_to_min_length<'a>(
    payload: &'a [u8],
    descriptor: &'static PgnDescriptor,
    padded: &'a mut [u8; MAX_PGN_BYTES],
) -> &'a [u8] {
    match (descriptor.min_length, descriptor.length) {
        (Some(min), Some(len))
            if payload.len() >= min as usize
                && payload.len() < len as usize
                && len as usize <= MAX_PGN_BYTES =>
        {
            padded[..payload.len()].copy_from_slice(payload);
            &padded[..len as usize]
        }
        _ => payload,
    }
}

/// Returns `true` when `field_idx` falls inside one of `descriptor`'s repeating groups.
fn is_repetitive_field(descriptor: &'static PgnDescriptor, field_idx: usize) -> bool {
    descriptor.repeating_field_sets.iter().any(|rfs| {
        field_idx >= rfs.start_field_index && field_idx < rfs.start_field_index + rfs.size
    })
}

/// Computes the clamped repetition count for `rfs` from its raw counter reading.
fn repeating_group_count(
    descriptor: &'static PgnDescriptor,
    rfs: &'static RepeatingFieldSet,
    counter_raw: Option<usize>,
    bits_remaining: usize,
) -> Result<usize, DeserializationError> {
    let count = if let Some(counter_idx) = rfs.count_field_index {
        let counter_field = descriptor
            .fields
            .get(counter_idx)
            .ok_or(DeserializationError::InvalidDataLength)?;
        let raw = counter_raw.ok_or(DeserializationError::InvalidDataLength)?;
        let counter_bits = counter_field.bits_length.unwrap_or(8).min(64);
        let unavailable = (u64::MAX >> (64 - counter_bits)) as usize;
        if raw == unavailable {
            0
        } else {
            raw
        }
    } else {
        return Err(DeserializationError::UnsupportedFieldKind {
            field_kind: crate::core::FieldKind::Unimplemented,
        });
    };

    let group_bits: usize = descriptor
        .fields
        .iter()
        .skip(rfs.start_field_index)
        .take(rfs.size)
        .map(|f| f.bits_length.unwrap_or(0) as usize)
        .sum();
    let fits = bits_remaining.checked_div(group_bits).unwrap_or(0);
    Ok(count.min(rfs.max_repetitions).min(fits))
}

/// Visits every non-reserved field of `payload` according to `desc`, in wire order.
///
/// `Reserved` and `Spare` fields are skipped. Repeating groups follow the same
/// counter sentinel (`all-ones` = 0) and clamping rules as [`deserialize_into`],
/// reporting `repetition = Some((array_id, index))`. If an unsupported field kind
/// or truncated tail is encountered mid-message, the error is returned after any
/// earlier fields have already been passed to `f`.
///
/// Note: `BitLookup` flag names are not resolved because `FieldDescriptor` does
/// not carry `enum_bit_name`.
pub fn for_each_field(
    desc: &'static PgnDescriptor,
    payload: &[u8],
    mut f: impl FnMut(&'static FieldDescriptor, FieldReading),
) -> Result<(), DeserializationError> {
    let mut padded = [0xFF; MAX_PGN_BYTES];
    let payload = pad_to_min_length(payload, desc, &mut padded);
    let mut reader = BitReader::new(payload);

    let mut counters = [None::<usize>; 4];

    for (field_idx, field_desc) in desc.fields.iter().enumerate() {
        if is_repetitive_field(desc, field_idx) {
            continue;
        }

        if let Some((raw, value)) = read_field_raw_and_value(&mut reader, field_desc)? {
            for (rfs_idx, rfs) in desc.repeating_field_sets.iter().enumerate().take(4) {
                if rfs.count_field_index == Some(field_idx) {
                    counters[rfs_idx] = raw.map(|v| v as usize);
                }
            }
            let availability = field_availability(field_desc, raw, &value);
            f(
                field_desc,
                FieldReading {
                    value,
                    raw,
                    availability,
                    repetition: None,
                },
            );
        }
    }

    for (rfs_idx, rfs) in desc.repeating_field_sets.iter().enumerate() {
        let counter_raw = counters.get(rfs_idx).copied().flatten();
        let count = repeating_group_count(desc, rfs, counter_raw, reader.bits_remaining())?;

        for elem_idx in 0..count {
            for field_offset in 0..rfs.size {
                let field_idx = rfs.start_field_index + field_offset;
                let field_desc = desc
                    .fields
                    .get(field_idx)
                    .ok_or(DeserializationError::InvalidDataLength)?;

                if let Some((raw, value)) = read_field_raw_and_value(&mut reader, field_desc)? {
                    let availability = field_availability(field_desc, raw, &value);
                    f(
                        field_desc,
                        FieldReading {
                            value,
                            raw,
                            availability,
                            repetition: Some((rfs.array_id, elem_idx)),
                        },
                    );
                }
            }
        }
    }

    Ok(())
}

fn field_availability(
    field_desc: &'static FieldDescriptor,
    raw: Option<u64>,
    value: &PgnValue,
) -> Availability {
    if field_desc.kind == FieldKind::StringFix {
        if let PgnValue::Bytes(ref b) = value {
            if string_fix_trimmed(b.as_slice()).is_empty() {
                return Availability::NotAvailable;
            }
        }
        return Availability::Available;
    }
    match raw {
        Some(raw_val) => field_desc.availability(raw_val),
        None => Availability::Available,
    }
}

/// Deserializes a payload into a generic PGN struct `T`.
// WARNING: tightly coupled with the `map_type()` function in build.rs.
// Keep both locations in sync when making changes.
///
/// # Parameters
/// * `instance` – object to populate field by field
/// * `payload` – raw buffer received from the CAN bus
/// * `descriptor` – static descriptor that defines the PGN layout
///
/// # Return value
/// Returns `Ok(())` when every field is read and assigned correctly.
pub fn deserialize_into<T: FieldAccess>(
    instance: &mut T,
    payload: &[u8],
    descriptor: &'static PgnDescriptor,
) -> Result<(), DeserializationError> {
    // A payload between `MinLength` and `Length` comes from a device built against
    // an older layout. Its missing tail reads as 0xFF, the N2K "not available".
    let mut padded = [0xFF; MAX_PGN_BYTES];
    let payload = pad_to_min_length(payload, descriptor, &mut padded);
    let mut reader = BitReader::new(payload);

    for (field_idx, field_desc) in descriptor.fields.iter().enumerate() {
        // Skip fields that belong to repeating groups; they are handled later
        if is_repetitive_field(descriptor, field_idx) {
            continue;
        }

        if let Some(value) = read_field_value(&mut reader, field_desc)? {
            instance.field_mut(field_desc.id, value).ok_or(
                DeserializationError::FieldAssignmentFailed {
                    desc: field_desc.id,
                },
            )?
        }
    }

    // ==================== Repeating field handling ====================
    // After processing all regular fields, handle repeating groups
    for rfs in descriptor.repeating_field_sets {
        let counter_raw = if let Some(counter_idx) = rfs.count_field_index {
            let counter_field = descriptor
                .fields
                .get(counter_idx)
                .ok_or(DeserializationError::InvalidDataLength)?;
            match instance.field(counter_field.id) {
                Some(PgnValue::U8(v)) => Some(v as usize),
                Some(PgnValue::U16(v)) => Some(v as usize),
                Some(PgnValue::U32(v)) => Some(v as usize),
                _ => return Err(DeserializationError::InvalidDataLength),
            }
        } else {
            None
        };
        let count = repeating_group_count(descriptor, rfs, counter_raw, reader.bits_remaining())?;

        // 2. Set the number of valid elements through the FieldAccess trait
        instance
            .set_repetitive_count(rfs.array_id, count)
            .ok_or(DeserializationError::FieldAssignmentFailed { desc: rfs.array_id })?;

        // 3. Iterate through every element of the repeating group
        for elem_idx in 0..count {
            // For each field in the group
            for field_offset in 0..rfs.size {
                let field_idx = rfs.start_field_index + field_offset;
                let field_desc = descriptor
                    .fields
                    .get(field_idx)
                    .ok_or(DeserializationError::InvalidDataLength)?;

                if let Some(value) = read_field_value(&mut reader, field_desc)? {
                    // Write the value into the array entry through FieldAccess
                    instance
                        .repetitive_field_mut(rfs.array_id, elem_idx, field_desc.id, value)
                        .ok_or(DeserializationError::FieldAssignmentFailed {
                            desc: field_desc.id,
                        })?;
                }
            }
        }
    }

    Ok(())
}

/// Serializes a PGN struct `T` into a buffer.
///
/// # Parameters
/// * `pgn_instance` – domain instance ready to convert into a raw payload
/// * `buffer` – output buffer (8 bytes for single frames, larger for Fast Packet)
/// * `descriptor` – static PGN metadata
///
/// # Return value
/// Number of bytes written into the buffer.
pub fn serialize<T: FieldAccess>(
    pgn_instance: &T,
    buffer: &mut [u8],
    descriptor: &'static PgnDescriptor,
) -> Result<usize, SerializationError> {
    // Initialize buffer with 0xFF for reserved bits.
    buffer.fill(0xFF);

    let mut writer = BitWriter::new(buffer);

    // Helper to detect whether a field belongs to a repeating group
    let is_repetitive_field = |field_idx: usize| -> bool {
        for rfs in descriptor.repeating_field_sets {
            if field_idx >= rfs.start_field_index && field_idx < (rfs.start_field_index + rfs.size)
            {
                return true;
            }
        }
        false
    };

    for (field_idx, field_desc) in descriptor.fields.iter().enumerate() {
        // Skip repeating fields; they are processed afterwards
        if is_repetitive_field(field_idx) {
            continue;
        }

        let value = pgn_instance
            .field(field_desc.id)
            .ok_or(SerializationError::FieldNotFound {
                field_id: field_desc.id,
            })?;
        write_field(&mut writer, field_desc, &value)?;
    }

    // ==================== Repeating field serialization ====================
    // After writing regular fields, serialize the repeating groups
    for rfs in descriptor.repeating_field_sets {
        // 1. Retrieve the number of valid elements for the array
        let count = pgn_instance.repetitive_count(rfs.array_id).ok_or(
            SerializationError::FieldNotFound {
                field_id: rfs.array_id,
            },
        )?;

        // 2. Clamp the counter against the allowed maximum
        let count = count.min(rfs.max_repetitions);

        // 3. Serialize every element of the repeating group
        for elem_idx in 0..count {
            // For each field in the group
            for field_offset in 0..rfs.size {
                let field_idx = rfs.start_field_index + field_offset;
                let field_desc = descriptor
                    .fields
                    .get(field_idx)
                    .ok_or(SerializationError::InvalidData)?;

                // Fetch the value from the structure via the trait
                let value = pgn_instance
                    .repetitive_field(rfs.array_id, elem_idx, field_desc.id)
                    .ok_or(SerializationError::FieldNotFound {
                        field_id: field_desc.id,
                    })?;

                // Write the value into the buffer
                write_field(&mut writer, field_desc, &value)?;
            }
        }
    }

    let bits_written = writer.bit_cursor();

    Ok(bits_written.div_ceil(8))
}

/// Trims a `STRING_FIX` slice at the first CANboat padding/termination byte
/// (`0xFF`, `b'@'`, or `0x00`). An empty result means the string is not available.
pub fn string_fix_trimmed(bytes: &[u8]) -> &[u8] {
    let end = bytes
        .iter()
        .position(|&b| b == 0xFF || b == b'@' || b == 0x00)
        .unwrap_or(bytes.len());
    &bytes[..end]
}

/// Shared helper to read a single field, applying business logic (signedness,
/// resolutions, special formats, etc.).
fn read_field_value(
    reader: &mut BitReader,
    field_desc: &'static FieldDescriptor,
) -> Result<Option<PgnValue>, DeserializationError> {
    read_field_raw_and_value(reader, field_desc).map(|opt| opt.map(|(_, val)| val))
}

/// Read the raw on-wire value (`Some(u64)` for bit/scalar fields, `None` for byte
/// slices and strings) and scale it into a [`PgnValue`].
fn read_field_raw_and_value(
    reader: &mut BitReader,
    field_desc: &'static FieldDescriptor,
) -> Result<Option<(Option<u64>, PgnValue)>, DeserializationError> {
    match field_desc.kind {
        // BitLookup: bitfield where each bit has its own meaning (bitmask).
        // Always treated as an unsigned integer without resolution regardless of descriptor.
        // Multiple bits may be set simultaneously (unlike Lookup, which is exclusive).
        FieldKind::BitLookup
        | FieldKind::Number
        | FieldKind::Lookup
        | FieldKind::IndirectLookup
        | FieldKind::Pgn
        | FieldKind::IsoName => {
            let raw_val = if let Some(bits) = field_desc.bits_length {
                match reader.read_u64(bits as u8) {
                    Ok(val) => val,
                    Err(_) => return Err(DeserializationError::InvalidDataLength),
                }
            } else {
                return Err(DeserializationError::InvalidDataLength);
            };
            let value = scale_raw_field(raw_val, field_desc)?;
            Ok(Some((Some(raw_val), value)))
        }

        // IEEE-754 single precision, carried as raw bits. No resolution applies.
        FieldKind::Float => {
            let raw_val = reader
                .read_u64(32)
                .map_err(|e| DeserializationError::BitReaderError { err: e })?;
            Ok(Some((Some(raw_val), scale_raw_field(raw_val, field_desc)?)))
        }

        FieldKind::Reserved | FieldKind::Spare => {
            if let Some(val) = field_desc.bits_length {
                reader
                    .advance(val as u8)
                    .map_err(|e| DeserializationError::BitReaderError { err: e })?;
            }
            Ok(None)
        }

        FieldKind::StringFix => {
            let num_bits = field_desc
                .bits_length
                .ok_or(DeserializationError::InvalidDataLength)?;
            // The declared size is the field's capacity, not the bytes on the wire:
            // Navico 130821 reserves 230 for its text and sends about 155, the real
            // length being carried by the Fast Packet header. Read what is there.
            let num_bytes = ((num_bits / 8) as usize).min(reader.bits_remaining() / 8);
            let slice = reader
                .read_slice(num_bytes)
                .map_err(|e| DeserializationError::BitReaderError { err: e })?;
            let mut pgn_bytes = PgnBytes {
                len: num_bytes,
                ..Default::default()
            };
            pgn_bytes.data[..num_bytes].copy_from_slice(slice);
            Ok(Some((None, PgnValue::Bytes(pgn_bytes))))
        }

        FieldKind::StringLz => {
            let strlen = reader
                .read_u8(8)
                .map_err(|e| DeserializationError::BitReaderError { err: e })?
                as usize;
            if strlen > MAX_PGN_BYTES {
                return Err(DeserializationError::InvalidDataLength);
            }
            let mut pgn_bytes = PgnBytes::default();
            if strlen > 0 {
                let slice = reader
                    .read_slice(strlen)
                    .map_err(|e| DeserializationError::BitReaderError { err: e })?;
                pgn_bytes.copy_from_slice(slice);
            }
            pgn_bytes.len = strlen;
            Ok(Some((None, PgnValue::Bytes(pgn_bytes))))
        }

        FieldKind::StringLau => {
            let total_len = reader
                .read_u8(8)
                .map_err(|e| DeserializationError::BitReaderError { err: e })?
                as usize;
            if total_len > MAX_PGN_BYTES {
                return Err(DeserializationError::InvalidDataLength);
            }
            let mut pgn_bytes = PgnBytes::default();
            // Below two bytes there is no room for the encoding byte: the field is
            // empty and nothing follows the length itself.
            if total_len >= 2 {
                let encoding = reader
                    .read_u8(8)
                    .map_err(|e| DeserializationError::BitReaderError { err: e })?;
                pgn_bytes.data[0] = encoding;
                // The declared length counts itself and the encoding byte, so the
                // text is two bytes shorter. Reading one byte too many swallowed
                // the following field and derailed the rest of the message.
                let payload_len = total_len.saturating_sub(2);
                if payload_len > 0 {
                    let slice = reader
                        .read_slice(payload_len)
                        .map_err(|e| DeserializationError::BitReaderError { err: e })?;
                    pgn_bytes.data[1..1 + payload_len].copy_from_slice(slice);
                }
                pgn_bytes.len = 1 + payload_len;
            }
            Ok(Some((None, PgnValue::Bytes(pgn_bytes))))
        }

        FieldKind::Binary => {
            let num_bits =
                field_desc
                    .bits_length
                    .ok_or(DeserializationError::InvalidFieldBits {
                        field_name: field_desc.id,
                    })?;
            // A BINARY field that does not fill whole bytes is a scalar, not a
            // buffer — that is how the generator types it too. AIS 129038/129039
            // carry a 19-bit `CommunicationState` this way.
            if num_bits % 8 != 0 {
                let raw_val = reader
                    .read_u64(num_bits as u8)
                    .map_err(|e| DeserializationError::BitReaderError { err: e })?;
                return Ok(Some((Some(raw_val), scale_raw_field(raw_val, field_desc)?)));
            }
            let num_bytes = (num_bits / 8) as usize;
            let slice = reader
                .read_slice(num_bytes)
                .map_err(|e| DeserializationError::BitReaderError { err: e })?;
            let mut pgn_bytes = PgnBytes {
                len: num_bytes,
                ..Default::default()
            };
            pgn_bytes.data[..num_bytes].copy_from_slice(slice);
            Ok(Some((None, PgnValue::Bytes(pgn_bytes))))
        }

        FieldKind::Date | FieldKind::Mmsi | FieldKind::Duration | FieldKind::Time => {
            let raw_val = if let Some(value) = field_desc.bits_length {
                reader
                    .read_u64(value as u8)
                    .map_err(|e| DeserializationError::BitReaderError { err: e })?
            } else {
                return Err(DeserializationError::InvalidFieldBits {
                    field_name: field_desc.id,
                });
            };
            Ok(Some((Some(raw_val), scale_raw_field(raw_val, field_desc)?)))
        }

        // Other kinds are not supported yet
        _ => Err(DeserializationError::UnsupportedFieldKind {
            field_kind: field_desc.kind,
        }),
    }
}

/// Convert a raw bit pattern into its scaled [`PgnValue`] according to `field_desc`.
fn scale_raw_field(
    raw_val: u64,
    field_desc: &'static FieldDescriptor,
) -> Result<PgnValue, DeserializationError> {
    match field_desc.kind {
        FieldKind::BitLookup | FieldKind::Binary => Ok(match field_desc.bits_length {
            Some(1..=8) => PgnValue::U8(raw_val as u8),
            Some(9..=16) => PgnValue::U16(raw_val as u16),
            Some(17..=32) => PgnValue::U32(raw_val as u32),
            _ => PgnValue::U64(raw_val),
        }),

        FieldKind::Number
        | FieldKind::Lookup
        | FieldKind::IndirectLookup
        | FieldKind::Pgn
        | FieldKind::IsoName => {
            let value = if field_desc.is_signed.is_some_and(|s| s) {
                let signed_val = sign_extend(raw_val, field_desc.bits_length.unwrap_or(0) as u8);
                if let Some(res) = field_desc.resolution {
                    match field_desc
                        .bits_length
                        .ok_or(DeserializationError::InvalidDataLength)?
                    {
                        1..=24 => PgnValue::F32(signed_val as f32 * res),
                        _ => PgnValue::F64(signed_val as f64 * res as f64),
                    }
                } else {
                    match field_desc.bits_length {
                        Some(1..=8) => PgnValue::I8(signed_val as i8),
                        Some(9..=16) => PgnValue::I16(signed_val as i16),
                        Some(17..=32) => PgnValue::I32(signed_val as i32),
                        _ => PgnValue::I64(signed_val),
                    }
                }
            } else if let Some(res) = field_desc.resolution {
                match field_desc
                    .bits_length
                    .ok_or(DeserializationError::InvalidDataLength)?
                {
                    1..=24 => PgnValue::F32(raw_val as f32 * res),
                    _ => PgnValue::F64(raw_val as f64 * res as f64),
                }
            } else {
                match field_desc.bits_length {
                    Some(1..=8) => PgnValue::U8(raw_val as u8),
                    Some(9..=16) => PgnValue::U16(raw_val as u16),
                    Some(17..=32) => PgnValue::U32(raw_val as u32),
                    _ => PgnValue::U64(raw_val),
                }
            };
            Ok(value)
        }

        FieldKind::Float => Ok(PgnValue::F32(f32::from_bits(raw_val as u32))),

        FieldKind::Date | FieldKind::Mmsi => {
            let value = if let Some(res) = field_desc.resolution {
                let scaled = raw_val as f64 * res as f64;
                match field_desc.bits_length.unwrap() {
                    1..=24 => PgnValue::F32(scaled as f32),
                    _ => PgnValue::F64(scaled),
                }
            } else {
                match field_desc.bits_length.unwrap() {
                    16 => PgnValue::U16(raw_val as u16),
                    32 => PgnValue::U32(raw_val as u32),
                    _ => {
                        return Err(DeserializationError::InvalidFieldBits {
                            field_name: field_desc.id,
                        })
                    }
                }
            };
            Ok(value)
        }

        FieldKind::Duration => {
            let value = if let Some(res) = field_desc.resolution {
                // shortcut: signed without resolution is not handled, add it if CANboat ever publishes one.
                let raw = if field_desc.is_signed == Some(true) {
                    sign_extend(raw_val, field_desc.bits_length.unwrap() as u8) as f64
                } else {
                    raw_val as f64
                };
                let scaled = raw * res as f64;
                match field_desc.bits_length.unwrap() {
                    1..=24 => PgnValue::F32(scaled as f32),
                    _ => PgnValue::F64(scaled),
                }
            } else {
                match field_desc.bits_length.unwrap() {
                    1..=16 => PgnValue::U16(raw_val as u16),
                    17..=32 => PgnValue::U32(raw_val as u32),
                    _ => PgnValue::U64(raw_val),
                }
            };
            Ok(value)
        }

        FieldKind::Time => {
            let value = if let Some(res) = field_desc.resolution {
                let scaled = raw_val as f64 * res as f64;
                PgnValue::F64(scaled)
            } else {
                PgnValue::U64(raw_val)
            };
            Ok(value)
        }

        _ => Err(DeserializationError::UnsupportedFieldKind {
            field_kind: field_desc.kind,
        }),
    }
}

/// Private helper that writes a single value according to its descriptor.
/// Encapsulates all business rules tied to `FieldKind` (signed/unsigned,
/// lookup, strings, binary blocks, etc.).
fn write_field(
    writer: &mut BitWriter,
    field_desc: &'static FieldDescriptor,
    value: &PgnValue,
) -> Result<(), SerializationError> {
    match field_desc.kind {
        FieldKind::Number | FieldKind::Pgn | FieldKind::IsoName => {
            let bits_to_write = if field_desc.is_signed.is_some_and(|s| s) {
                let prepared_val = if let Some(res) = field_desc.resolution {
                    // Common path: floating-point value that must be scaled back to an integer
                    let float_val = pgn_value_to_f64(value)
                        .map_err(|e| SerializationError::CodecError { source: e })?;
                    scale_to_i64(float_val, res)
                } else {
                    pgn_value_to_i64(value)
                        .map_err(|e| SerializationError::CodecError { source: e })?
                };
                // Use the helper to reinterpret the signed integer as u64
                i64_to_u64_bitwise(prepared_val)
            } else if let Some(res) = field_desc.resolution {
                let float_val = pgn_value_to_f64(value)
                    .map_err(|e| SerializationError::CodecError { source: e })?;
                i64_to_u64_bitwise(scale_to_i64(float_val, res))
            } else {
                pgn_value_to_u64(value).map_err(|e| SerializationError::CodecError { source: e })?
            };

            if let Some(bit_length) = field_desc.bits_length {
                writer
                    .write_u64(bits_to_write, bit_length as u8)
                    .map_err(|e| SerializationError::BitWriteError { err: e })?;
            } else {
                return Err(SerializationError::InvalidData);
            };
        }

        FieldKind::Date | FieldKind::Time | FieldKind::Mmsi => {
            let bits_to_write =
                field_desc
                    .bits_length
                    .ok_or(SerializationError::InvalidFieldBits {
                        field_name: field_desc.id,
                    })?;

            let int_val = if field_desc.resolution.is_some_and(|res| res as u8 != 1) {
                // With resolution: value stored as F32/F64
                let float_val = pgn_value_to_f64(value)
                    .map_err(|e| SerializationError::CodecError { source: e })?;
                scale_to_i64(float_val, field_desc.resolution.unwrap()) as u64 // Direct cast; two's-complement helper not required
            } else {
                // Without resolution: value stored as U16/U32
                pgn_value_to_u64(value).map_err(|e| SerializationError::CodecError { source: e })?
            };

            writer
                .write_u64(int_val, bits_to_write as u8)
                .map_err(|e| SerializationError::BitWriteError { err: e })?;
        }

        FieldKind::Duration => {
            // Treat as a Number with resolution
            let bits_to_write =
                field_desc
                    .bits_length
                    .ok_or(SerializationError::InvalidFieldBits {
                        field_name: field_desc.id,
                    })?;

            let prepared_val = if let Some(res) = field_desc.resolution {
                // Apply the inverse resolution
                let float_val = pgn_value_to_f64(value)
                    .map_err(|e| SerializationError::CodecError { source: e })?;
                scale_to_i64(float_val, res) as u64
            } else {
                // No resolution involved
                pgn_value_to_u64(value).map_err(|e| SerializationError::CodecError { source: e })?
            };

            writer
                .write_u64(prepared_val, bits_to_write as u8)
                .map_err(|e| SerializationError::BitWriteError { err: e })?;
        }
        FieldKind::Lookup => {
            let int_val = pgn_value_to_u64(value)
                .map_err(|e| SerializationError::CodecError { source: e })?;
            if let Some(bit_len) = field_desc.bits_length {
                writer
                    .write_u64(int_val, bit_len as u8)
                    .map_err(|e| SerializationError::BitWriteError { err: e })?;
            };
        }

        // BitLookup: write a bitmask (unsigned integer without resolution).
        // Each bit carries its own meaning and multiple bits may be active simultaneously.
        // Handling is identical to Lookup but documented separately for clarity.
        FieldKind::BitLookup => {
            let int_val = pgn_value_to_u64(value)
                .map_err(|e| SerializationError::CodecError { source: e })?;

            let bit_len = field_desc
                .bits_length
                .ok_or(SerializationError::InvalidFieldBits {
                    field_name: field_desc.id,
                })?;

            writer
                .write_u64(int_val, bit_len as u8)
                .map_err(|e| SerializationError::BitWriteError { err: e })?;
        }
        FieldKind::IndirectLookup => {
            // The field holds its own raw value; the master field is written separately.
            let value_to_write = pgn_value_to_u64(value)
                .map_err(|e| SerializationError::CodecError { source: e })?;

            if let Some(bit_len) = field_desc.bits_length {
                writer
                    .write_u64(value_to_write, bit_len as u8)
                    .map_err(|e| SerializationError::BitWriteError { err: e })?;
            };
        }
        FieldKind::Spare => {
            if let Some(bit_len) = field_desc.bits_length {
                writer
                    .write_u64(0, bit_len as u8)
                    .map_err(|e| SerializationError::BitWriteError { err: e })?;
            }
        }

        // Mirror of the read path: raw IEEE-754 bits, no resolution.
        FieldKind::Float => {
            let val = pgn_value_to_f64(value)
                .map_err(|e| SerializationError::CodecError { source: e })?;
            writer
                .write_u64((val as f32).to_bits() as u64, 32)
                .map_err(|e| SerializationError::BitWriteError { err: e })?;
        }

        FieldKind::Reserved => {
            if let Some(bit_len) = field_desc.bits_length {
                writer
                    .advance(bit_len as u8)
                    .map_err(|e| SerializationError::BitWriteError { err: e })?
            }
        }

        FieldKind::StringFix => {
            if let PgnValue::Bytes(val) = value {
                writer
                    .write_slice(&val.data[..val.len])
                    .map_err(|e| SerializationError::BitWriteError { err: e })?;
            } else {
                return Err(SerializationError::CodecError {
                    source: CodecError::DataTypeMismatch {
                        value_kind: value.kind(),
                        func: "write_field // StringFix",
                    },
                });
            }
        }
        FieldKind::StringLz => {
            if let PgnValue::Bytes(val) = value {
                if val.len > u8::MAX as usize {
                    return Err(SerializationError::InvalidData);
                }
                writer
                    .write_u64(val.len as u64, 8)
                    .map_err(|e| SerializationError::BitWriteError { err: e })?;
                if val.len > 0 {
                    writer
                        .write_slice(&val.data[..val.len])
                        .map_err(|e| SerializationError::BitWriteError { err: e })?;
                }
            } else {
                return Err(SerializationError::CodecError {
                    source: CodecError::DataTypeMismatch {
                        value_kind: value.kind(),
                        func: "write_field // StringLz",
                    },
                });
            }
        }
        FieldKind::StringLau => {
            if let PgnValue::Bytes(val) = value {
                // `val` holds the encoding byte followed by the text; the declared
                // length also counts the length byte itself. An empty field
                // declares zero, so that reading it back consumes one byte too.
                let declared = if val.len == 0 { 0 } else { val.len + 1 };
                if declared > u8::MAX as usize {
                    return Err(SerializationError::InvalidData);
                }
                writer
                    .write_u64(declared as u64, 8)
                    .map_err(|e| SerializationError::BitWriteError { err: e })?;
                if val.len > 0 {
                    writer
                        .write_u64(val.data[0] as u64, 8)
                        .map_err(|e| SerializationError::BitWriteError { err: e })?;
                    if val.len > 1 {
                        writer
                            .write_slice(&val.data[1..val.len])
                            .map_err(|e| SerializationError::BitWriteError { err: e })?;
                    }
                }
            } else {
                return Err(SerializationError::CodecError {
                    source: CodecError::DataTypeMismatch {
                        value_kind: value.kind(),
                        func: "write_field // StringLau",
                    },
                });
            }
        }
        FieldKind::Binary => {
            let expected_bits =
                field_desc
                    .bits_length
                    .ok_or(SerializationError::InvalidFieldBits {
                        field_name: field_desc.id,
                    })?;

            // Mirror of the read path: a sub-byte BINARY field is a scalar.
            if expected_bits % 8 != 0 {
                let raw_val = pgn_value_to_u64(value)
                    .map_err(|e| SerializationError::CodecError { source: e })?;
                return writer
                    .write_u64(raw_val, expected_bits as u8)
                    .map_err(|e| SerializationError::BitWriteError { err: e });
            }

            if let PgnValue::Bytes(val) = value {
                let expected_len = (expected_bits / 8) as usize;
                if val.len != expected_len {
                    return Err(SerializationError::InvalidData);
                }
                writer
                    .write_slice(&val.data[..expected_len])
                    .map_err(|e| SerializationError::BitWriteError { err: e })?;
            } else {
                return Err(SerializationError::CodecError {
                    source: CodecError::DataTypeMismatch {
                        value_kind: value.kind(),
                        func: "write_field // Binary",
                    },
                });
            }
        }
        _ => return Err(SerializationError::UnsupportedFieldKind),
    }
    Ok(())
}

/// Undoes a field's resolution, rounding half away from zero.
/// Truncating instead would shift every scaled value one LSB towards zero:
/// 23.767 at a resolution of 0.001 divides to 23766.999… and came back as 23.766.
/// `f64::round` lives in `std`, which this crate does not have.
fn scale_to_i64(value: f64, resolution: f32) -> i64 {
    let scaled = value / resolution as f64;
    if scaled >= 0.0 {
        (scaled + 0.5) as i64
    } else {
        (scaled - 0.5) as i64
    }
}

/// Converts a `PgnValue` into `f64`.
/// Normalizes values to double precision when a resolution must be applied during serialization.
fn pgn_value_to_f64(value: &PgnValue) -> Result<f64, CodecError> {
    match value {
        PgnValue::F64(v) => Ok(*v),
        PgnValue::F32(v) => Ok(*v as f64),
        PgnValue::I64(v) => Ok(*v as f64),
        PgnValue::I32(v) => Ok(*v as f64),
        PgnValue::I16(v) => Ok(*v as f64),
        PgnValue::I8(v) => Ok(*v as f64),
        _ => Err(CodecError::DataTypeMismatch {
            value_kind: value.kind(),
            func: "pgn_value_to_f64",
        }),
    }
}

/// Converts a `PgnValue` into `i64`.
/// Used to serialize signed fields while handling implicit widening from smaller integer sizes.
fn pgn_value_to_i64(value: &PgnValue) -> Result<i64, CodecError> {
    match value {
        PgnValue::I64(v) => Ok(*v),
        PgnValue::I32(v) => Ok(*v as i64),
        PgnValue::I16(v) => Ok(*v as i64),
        PgnValue::I8(v) => Ok(*v as i64),
        _ => Err(CodecError::DataTypeMismatch {
            value_kind: value.kind(),
            func: "pgn_value_to_i64",
        }),
    }
}

/// Converts a `PgnValue` into `u64`.
/// Covers unsigned variants; anything else indicates a misuse of the descriptor.
fn pgn_value_to_u64(value: &PgnValue) -> Result<u64, CodecError> {
    match value {
        PgnValue::U64(v) => Ok(*v),
        PgnValue::U32(v) => Ok(*v as u64),
        PgnValue::U16(v) => Ok(*v as u64),
        PgnValue::U8(v) => Ok(*v as u64),
        _ => Err(CodecError::DataTypeMismatch {
            value_kind: value.kind(),
            func: "pgn_value_to_u64",
        }),
    }
}

//==================================================================================

/// Two's complement helper.
/// Extends the sign of a value read on a limited number of bits.
/// If the sign bit is set, the function propagates it across the `i64` tail to rebuild the negative value.
/// Essential logic to reinterpret small integers (up to 64 bits) into `i64` without losing information.
fn sign_extend(value: u64, bits: u8) -> i64 {
    // Reading the full 64 bits already yields the correct representation.
    if bits >= 64 {
        return value as i64;
    }

    // Locate the sign bit.
    let sign_bit_mask = 1u64 << (bits - 1);

    // Check whether the sign bit is set.
    if (value & sign_bit_mask) != 0 {
        // Extend the sign by filling the upper bits with ones.
        let extension_mask = u64::MAX << bits;
        (value | extension_mask) as i64
    } else {
        // Positive values are returned as-is.
        value as i64
    }
}

/// Reinterprets the bits of an `i64` as `u64` for writing.
/// Negative `i64` values are already stored in two's complement, so the bits can be reused verbatim.
/// Keeps the bitwise `as` cast in a single helper to simplify debugging and reviews.
/// The `inline` annotation avoids the overhead of a function call for a trivial conversion.
#[inline] // The function is tiny; encourage inlining.
fn i64_to_u64_bitwise(value: i64) -> u64 {
    value as u64
}

//==================================================================================TESTS

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
