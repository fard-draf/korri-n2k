//! Generic serialization/deserialization engine driven by compile-time PGN descriptors.
//! It controls the bit-level readers/writers and turns dynamic `PgnValue`s into
//! strongly typed domain structures.
use super::bits::{BitReader, BitWriter};
use super::traits::FieldAccess;
use crate::core::{FieldDescriptor, FieldKind, PgnBytes, PgnDescriptor, PgnValue, MAX_PGN_BYTES};
use crate::error::{CodecError, DeserializationError, SerializationError};

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
    let mut reader = BitReader::new(payload);

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
        // Skip fields that belong to repeating groups; they are handled later
        if is_repetitive_field(field_idx) {
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
        // 1. Read the counter field to know how many elements to expect
        let count = if let Some(counter_idx) = rfs.count_field_index {
            // The counter is a regular field already parsed; retrieve it via the descriptor
            let counter_field = descriptor
                .fields
                .get(counter_idx)
                .ok_or(DeserializationError::InvalidDataLength)?;

            // Fetch the counter value from the instance
            match instance.field(counter_field.id) {
                Some(PgnValue::U8(v)) => v as usize,
                Some(PgnValue::U16(v)) => v as usize,
                Some(PgnValue::U32(v)) => v as usize,
                _ => return Err(DeserializationError::InvalidDataLength),
            }
        } else {
            // No explicit counter: would require computing the length on the fly.
            // This branch is not supported yet because the scenario is uncommon.
            return Err(DeserializationError::UnsupportedFieldKind {
                field_kind: crate::core::FieldKind::Unimplemented,
            });
        };

        // Clamp the counter against the maximum allowed repetitions
        let count = count.min(rfs.max_repetitions);

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
pub fn serialize<'a, T: FieldAccess>(
    pgn_instance: &'a T,
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

    Ok((bits_written + 7) / 8)
}

/// Shared helper to read a single field, applying business logic (signedness,
/// resolutions, special formats, etc.).
fn read_field_value(
    reader: &mut BitReader,
    field_desc: &'static FieldDescriptor,
) -> Result<Option<PgnValue>, DeserializationError> {
    match field_desc.kind {
        // BitLookup: bitfield where each bit has its own meaning (bitmask).
        // Always treated as an unsigned integer without resolution regardless of descriptor.
        // Multiple bits may be set simultaneously (unlike Lookup, which is exclusive).
        FieldKind::BitLookup => {
            let raw_val = if let Some(bits) = field_desc.bits_length {
                match reader.read_u64(bits as u8) {
                    Ok(val) => val,
                    Err(_) => return Err(DeserializationError::InvalidDataLength),
                }
            } else {
                return Err(DeserializationError::InvalidDataLength);
            };

            // Map the raw value to the appropriate type based on bit length
            // (always unsigned and without resolution)
            let value = match field_desc.bits_length {
                Some(1..=8) => PgnValue::U8(raw_val as u8),
                Some(9..=16) => PgnValue::U16(raw_val as u16),
                Some(17..=32) => PgnValue::U32(raw_val as u32),
                _ => PgnValue::U64(raw_val),
            };

            Ok(Some(value))
        }

        FieldKind::Number | FieldKind::Lookup | FieldKind::IndirectLookup | FieldKind::Pgn => {
            let raw_val = if let Some(bits) = field_desc.bits_length {
                match reader.read_u64(bits as u8) {
                    Ok(val) => val,
                    Err(_) => return Err(DeserializationError::InvalidDataLength),
                }
            } else {
                return Err(DeserializationError::InvalidDataLength);
            };

            let value = if field_desc.is_signed.is_some_and(|s| s) {
                let signed_val = sign_extend(raw_val, field_desc.bits_length.unwrap_or(0) as u8);
                if let Some(res) = field_desc.resolution {
                    match field_desc
                        .bits_length
                        .ok_or(DeserializationError::InvalidDataLength)?
                    {
                        1..=32 => PgnValue::F32(signed_val as f32 * res),
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
                    1..=32 => PgnValue::F32(raw_val as f32 * res),
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

            Ok(Some(value))
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
            let num_bytes = (num_bits / 8) as usize;
            let slice = reader
                .read_slice(num_bytes)
                .map_err(|e| DeserializationError::BitReaderError { err: e })?;
            let mut pgn_bytes = PgnBytes::default();
            pgn_bytes.len = num_bytes;
            pgn_bytes.data[..num_bytes].copy_from_slice(slice);
            Ok(Some(PgnValue::Bytes(pgn_bytes)))
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
            Ok(Some(PgnValue::Bytes(pgn_bytes)))
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
            if total_len > 0 {
                let encoding = reader
                    .read_u8(8)
                    .map_err(|e| DeserializationError::BitReaderError { err: e })?;
                pgn_bytes.data[0] = encoding;
                let payload_len = total_len.saturating_sub(1);
                if payload_len > 0 {
                    let slice = reader
                        .read_slice(payload_len)
                        .map_err(|e| DeserializationError::BitReaderError { err: e })?;
                    pgn_bytes.data[1..1 + payload_len].copy_from_slice(slice);
                }
            }
            pgn_bytes.len = total_len;
            Ok(Some(PgnValue::Bytes(pgn_bytes)))
        }

        FieldKind::Binary => {
            let num_bits =
                field_desc
                    .bits_length
                    .ok_or(DeserializationError::InvalidFieldBits {
                        field_name: field_desc.id,
                    })?;
            if num_bits % 8 != 0 {
                return Err(DeserializationError::InvalidFieldBits {
                    field_name: field_desc.id,
                });
            }
            let num_bytes = (num_bits / 8) as usize;
            let slice = reader
                .read_slice(num_bytes)
                .map_err(|e| DeserializationError::BitReaderError { err: e })?;
            let mut pgn_bytes = PgnBytes::default();
            pgn_bytes.len = num_bytes;
            pgn_bytes.data[..num_bytes].copy_from_slice(slice);
            Ok(Some(PgnValue::Bytes(pgn_bytes)))
        }

        FieldKind::Date | FieldKind::Mmsi => {
            let raw_val = if let Some(value) = field_desc.bits_length {
                reader
                    .read_u64(value as u8)
                    .map_err(|e| DeserializationError::BitReaderError { err: e })?
            } else {
                return Err(DeserializationError::InvalidFieldBits {
                    field_name: field_desc.id,
                });
            };
            let value = if let Some(res) = field_desc.resolution {
                let scaled = raw_val as f64 * res as f64;
                match field_desc.bits_length.unwrap() {
                    1..=32 => PgnValue::F32(scaled as f32),
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
            Ok(Some(value))
        }

        FieldKind::Duration => {
            let raw_val = if let Some(value) = field_desc.bits_length {
                reader
                    .read_u64(value as u8)
                    .map_err(|e| DeserializationError::BitReaderError { err: e })?
            } else {
                return Err(DeserializationError::InvalidFieldBits {
                    field_name: field_desc.id,
                });
            };

            let value = if let Some(res) = field_desc.resolution {
                let scaled = raw_val as f64 * res as f64;
                match field_desc.bits_length.unwrap() {
                    1..=32 => PgnValue::F32(scaled as f32),
                    _ => PgnValue::F64(scaled),
                }
            } else {
                match field_desc.bits_length.unwrap() {
                    1..=16 => PgnValue::U16(raw_val as u16),
                    17..=32 => PgnValue::U32(raw_val as u32),
                    _ => PgnValue::U64(raw_val),
                }
            };
            Ok(Some(value))
        }

        FieldKind::Time => {
            let raw_val = if let Some(value) = field_desc.bits_length {
                reader
                    .read_u64(value as u8)
                    .map_err(|e| DeserializationError::BitReaderError { err: e })?
            } else {
                return Err(DeserializationError::InvalidFieldBits {
                    field_name: field_desc.id,
                });
            };

            let value = if let Some(res) = field_desc.resolution {
                let scaled = raw_val as f64 * res as f64;
                PgnValue::F64(scaled)
            } else {
                PgnValue::U64(raw_val)
            };
            Ok(Some(value))
        }

        // Other kinds are not supported yet
        _ => Err(DeserializationError::UnsupportedFieldKind {
            field_kind: field_desc.kind.clone(),
        }),
    }
}

/// Private helper that writes a single value according to its descriptor.
/// Encapsulates all business rules tied to `FieldKind` (signed/unsigned,
/// lookup, strings, binary blocks, etc.).
fn write_field<'a>(
    writer: &mut BitWriter,
    field_desc: &'static FieldDescriptor,
    value: &'a PgnValue,
) -> Result<(), SerializationError> {
    match field_desc.kind {
        FieldKind::Number | FieldKind::Pgn => {
            let bits_to_write = if field_desc.is_signed.is_some_and(|s| s) {
                let prepared_val = if let Some(res) = field_desc.resolution {
                    // Common path: floating-point value that must be scaled back to an integer
                    let float_val = pgn_value_to_f64(value)
                        .map_err(|e| SerializationError::CodecError { source: e })?;
                    (float_val / res as f64) as i64
                } else {
                    pgn_value_to_i64(value)
                        .map_err(|e| SerializationError::CodecError { source: e })?
                };
                // Use the helper to reinterpret the signed integer as u64
                i64_to_u64_bitwise(prepared_val)
            } else if let Some(res) = field_desc.resolution {
                let float_val = pgn_value_to_f64(value)
                    .map_err(|e| SerializationError::CodecError { source: e })?;
                i64_to_u64_bitwise((float_val / res as f64) as i64)
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
                (float_val / field_desc.resolution.unwrap() as f64) as u64 // Direct cast; two's-complement helper not required
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
                (float_val / res as f64) as u64
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
            // Value is received as a combined u16 (high + low byte).
            let combined_value = pgn_value_to_u64(value)
                .map_err(|e| SerializationError::CodecError { source: e })?
                as u16;
            // Only the low byte belongs to this field; the master field writes the high byte.
            let value_to_write = (combined_value & 0x00FF) as u64;

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
                        value: value.clone(),
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
                        value: value.clone(),
                        func: "write_field // StringLz",
                    },
                });
            }
        }
        FieldKind::StringLau => {
            if let PgnValue::Bytes(val) = value {
                if val.len > u8::MAX as usize {
                    return Err(SerializationError::InvalidData);
                }
                writer
                    .write_u64(val.len as u64, 8)
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
                        value: value.clone(),
                        func: "write_field // StringLau",
                    },
                });
            }
        }
        FieldKind::Binary => {
            if let PgnValue::Bytes(val) = value {
                let expected_bits =
                    field_desc
                        .bits_length
                        .ok_or(SerializationError::InvalidFieldBits {
                            field_name: field_desc.id,
                        })?;
                if expected_bits % 8 != 0 {
                    return Err(SerializationError::InvalidFieldBits {
                        field_name: field_desc.id,
                    });
                }
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
                        value: value.clone(),
                        func: "write_field // Binary",
                    },
                });
            }
        }
        _ => return Err(SerializationError::UnsupportedFieldKind),
    }
    Ok(())
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
            value: value.clone(),
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
            value: value.clone(),
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
            value: value.clone(),
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
