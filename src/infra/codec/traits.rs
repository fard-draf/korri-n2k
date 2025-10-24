//! Public traits exposed by the codec engine. They decouple generated
//! PGN structures from the serialization/deserialization logic and provide
//! a uniform API to upper layers.
use crate::core::PgnValue;
use crate::error::{DeserializationError, SerializationError};

//==================================================================================PGN_DATA
/// Implemented by every generated PGN struct.
/// Acts as a bridge between static descriptors and the interpretation engine.
pub trait PgnData: Sized + FieldAccess {
    /// Deserialize a payload into an instance of the struct.
    /// The default implementation delegates to generated code.
    fn from_payload(payload: &[u8]) -> Result<Self, DeserializationError>;

    /// Serialize the instance into the provided buffer.
    /// The default implementation is provided by the engine.
    fn to_payload(&self, buffer: &mut [u8]) -> Result<usize, SerializationError>;
}
//==================================================================================FIELD_ACCESS
/// Trait that lets the engine access PGN fields by their `'static str` identifier
/// without knowing the concrete type. Implementations are code-generated.
///
/// # Regular fields vs repeating fields
///
/// NMEA 2000 PGNs may contain:
/// - **Regular fields** accessible through `field()` and `field_mut()`
/// - **Repeating field sets**, groups repeated N times and accessed through
///   `repetitive_field()` / `repetitive_field_mut()`
///
/// ## Example: PGN 129029 (GNSS Position Data)
///
/// Contains regular fields (date, time, latitude, …) and a repeating group of
/// reference stations (`reference_station_id`, `age_of_dgnss_corrections`).
///
/// ```rust, ignore
/// let mut pgn = Pgn129029::new();
///
/// // Regular field access
/// pgn.field_mut("Date", PgnValue::U16(19000));
///
/// // Define the number of reference stations
/// pgn.set_repetitive_count("reference_station_types", 2);
///
/// // Repeating field access (station 0)
/// pgn.repetitive_field_mut("reference_station_types", 0, "ReferenceStationId", PgnValue::U16(101));
/// pgn.repetitive_field_mut("reference_station_types", 0, "AgeOfDgnssCorrections", PgnValue::F32(5.2));
///
/// // Repeating field access (station 1)
/// pgn.repetitive_field_mut("reference_station_types", 1, "ReferenceStationId", PgnValue::U16(202));
/// pgn.repetitive_field_mut("reference_station_types", 1, "AgeOfDgnssCorrections", PgnValue::F32(3.7));
/// ```
pub trait FieldAccess {
    /// Read the value of a regular (non-repeating) field.
    ///
    /// * `id` - Field identifier in PascalCase (e.g. `"Date"`, `"Latitude"`)
    ///
    /// Returns `Some(PgnValue)` if the field exists, `None` otherwise.
    fn field(&self, id: &'static str) -> Option<PgnValue>;

    /// Write the value of a regular (non-repeating) field.
    ///
    /// * `id` - Field identifier in PascalCase
    /// * `value` - Value to write; must match the expected type
    ///
    /// Returns `Some(())` on success, `None` if the field does not exist or the type mismatches.
    fn field_mut(&mut self, id: &'static str, value: PgnValue) -> Option<()>;

    //==================== Repeating field helpers ====================

    /// Read a field inside a repeating group.
    ///
    /// * `array_id` - Repeating array identifier in snake_case (e.g. `"reference_station_types"`)
    /// * `index` - Element index (0-based)
    /// * `field_id` - Field identifier within the element (e.g. `"ReferenceStationId"`)
    ///
    /// Returns `Some(PgnValue)` if the field exists and the index is valid.
    ///
    /// Default implementation returns `None` (PGNs without repeating fields).
    fn repetitive_field(
        &self,
        _array_id: &'static str,
        _index: usize,
        _field_id: &'static str,
    ) -> Option<PgnValue> {
        None // Default: no repeating fields
    }

    /// Write a field in a repeating group.
    ///
    /// * `array_id` - Repeating array identifier in snake_case
    /// * `index` - Element index (0-based)
    /// * `field_id` - Field identifier within the element
    /// * `value` - Value to write
    ///
    /// Returns `Some(())` when successful, `None` if the field or index is invalid or the type mismatches.
    ///
    /// Invariant: `index` must be strictly less than `repetitive_count()`.
    ///
    /// Default implementation returns `None` (PGNs without repeating fields).
    fn repetitive_field_mut(
        &mut self,
        _array_id: &'static str,
        _index: usize,
        _field_id: &'static str,
        _value: PgnValue,
    ) -> Option<()> {
        None // Default: no repeating fields
    }

    /// Get the number of valid elements in a repeating array.
    ///
    /// * `array_id` - Repeating array identifier in snake_case
    ///
    /// Returns `Some(count)` (possibly 0) or `None` if the array does not exist.
    ///
    /// Invariant: the value must always be ≤ `max_repetitions` defined by the descriptor.
    ///
    /// Default implementation returns `None` (PGNs without repeating fields).
    fn repetitive_count(&self, _array_id: &'static str) -> Option<usize> {
        None // Default: no repeating fields
    }

    /// Set the number of valid entries in a repeating array.
    ///
    /// * `array_id` - Repeating array identifier in snake_case
    /// * `count` - Number of valid elements (must be ≤ `max_repetitions`)
    ///
    /// Returns `Some(())` on success, `None` if the array does not exist or the count is invalid.
    ///
    /// Safety: implementers must ensure `count` never exceeds `max_repetitions`.
    ///
    /// Default implementation returns `None` (PGNs without repeating fields).
    fn set_repetitive_count(&mut self, _array_id: &'static str, _count: usize) -> Option<()> {
        None // Default: no repeating fields
    }
}
//==================================================================================TO_PAYLOAD
/// Serialize a data structure into a sequence of bytes.
///
/// Public contract used by the codec engine to turn a high-level PGN into
/// a binary payload ready to transmit. Implemented by every generated PGN structure.
pub trait ToPayload {
    /// Serialize the structure into the provided buffer.
    ///
    /// * `buffer`: destination buffer for serialized bytes.
    ///
    /// Returns the number of bytes written on success.
    fn to_payload(&self, buffer: &mut [u8]) -> Result<usize, SerializationError>;
    /// Maximum serialized payload length for this structure.
    fn payload_len(&self) -> usize;
}
//==================================================================================FROM_PAYLOAD
/// Deserialize a sequence of bytes into a data structure.
///
/// Public contract used by the codec engine to rebuild a high-level PGN
/// from an incoming binary payload. Implemented by every generated PGN.
pub trait FromPayload: Sized {
    /// Deserialize a byte slice to produce a new instance.
    fn from_payload(bytes_slice: &[u8]) -> Result<Self, DeserializationError>;
}
