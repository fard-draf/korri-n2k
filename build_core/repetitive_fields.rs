//! Code generation helpers for NMEA 2000 repeating fields.
//!
//! This module builds the Rust structures used for groups of fields that repeat
//! a variable number of times in certain PGNs.
//!
//! **Example**: PGN 129540 (GNSS Sats in View)
//! - Field 4 (`prn`) is the counter → number of satellites
//! - Fields 5–11 form the repeating group (elevation, azimuth, SNR…)
//! - If `prn = 5`, fields 5–11 are read/written five times
//!
//! **Excerpt from canboat.json**:
//! ```json
//! {
//!   "PGN": 129540,
//!   "RepeatingFieldSet1Size": 7,
//!   "RepeatingFieldSet1StartField": 5,
//!   "RepeatingFieldSet1CountField": 4
//! }
//! ```

use crate::build_core::domain::*;
use crate::build_core::errors::*;
use crate::build_core::name_helpers::*;
use crate::build_core::type_helpers::*;
use crate::core::FieldKind;
use std::collections::HashMap;
use std::fmt::Write;

/// Metadata extracted for a repeating-field group.
#[derive(Debug, Clone)]
pub(crate) struct RepeatingFieldSetInfo {
    /// Index of the field that stores the repetition counter (None = dynamic length)
    pub count_field_index: Option<usize>,

    /// Index of the first field in the group (0-based)
    pub start_field_index: usize,

    /// Number of consecutive fields that form the group
    pub size: usize,

    /// Name of the generated struct for the group (e.g. "SatelliteInfo")
    pub struct_name: String,

    /// Name of the array field in the parent struct (e.g. "satellites")
    pub array_field_name: String,

    /// Name of the counter field in the parent struct (e.g. "satellites_count")
    pub count_field_name: String,

    /// Computed maximum number of repetitions
    pub max_repetitions: usize,
}

impl RepeatingFieldSetInfo {
    /// Extract repeating-field metadata from the PGN definition.
    ///
    /// # Arguments
    /// * `pgn` - Full PGN definition from canboat.json
    /// * `set_number` - Group number (1, 2, or 3)
    ///
    /// # Returns
    /// Returns `Some(RepeatingFieldSetInfo)` if the group exists, `None` otherwise.
    pub fn extract_from_pgn(pgn: &PgnInstructions, set_number: u8) -> Option<Self> {
        // Extract metadata depending on the group number.
        // IMPORTANT: start/count fields reference Orders, not indices.
        let (size, start_field_order, count_field_order) = match set_number {
            1 => (
                pgn.repeating_field_set_1_size?,
                pgn.repeating_field_set_1_start_field?,
                pgn.repeating_field_set_1_count_field,
            ),
            2 => (
                pgn.repeating_field_set_2_size?,
                pgn.repeating_field_set_2_start_field?,
                pgn.repeating_field_set_2_count_field,
            ),
            _ => return None,
        };

        // Convert Orders into array indices.
        // canboat.json uses 1-based Orders; convert them to 0-based indices.
        let start_field_index = pgn
            .fields
            .iter()
            .position(|f| f.order == start_field_order)?;
        let count_field_index =
            count_field_order.and_then(|order| pgn.fields.iter().position(|f| f.order == order));

        // Derive the nested struct name from the group's first field.
        // Example: "satellitePrn" → "SatellitePrnInfo".
        // More reliable than using the counter, which may have ambiguous names.
        let struct_name = {
            let first_field = pgn.fields.get(start_field_index)?;
            let base_name = to_pascal_case(&first_field.id, PascalCaseMode::Soft);
            format!("{}Info", base_name)
        };

        // Array name: plural snake_case form of the struct name ("SatelliteInfo" → "satellites").
        let array_field_name = pluralize_field_name(&struct_name);

        // Counter name: array name + "_count" ("satellites" → "satellites_count").
        let count_field_name = format!("{}_count", array_field_name);

        // Compute the maximum repetition count based on the Fast Packet payload (223 bytes).
        let max_repetitions = calculate_max_repetitions(pgn, start_field_index, size as usize);

        Some(Self {
            count_field_index,
            start_field_index,
            size: size as usize,
            struct_name,
            array_field_name,
            count_field_name,
            max_repetitions,
        })
    }
}

#[cfg(test)]
/// Derive the struct name from the counter field name.
///
/// **Examples**:
/// - "numberOfSatellites" → "SatelliteInfo"
/// - "referenceStations" → "ReferenceStationInfo"
/// - "itemCount" → "ItemInfo"
fn derive_struct_name_from_counter(counter_field_name: &str) -> String {
    // Strip common prefixes
    let name = counter_field_name
        .trim_start_matches("numberOf")
        .trim_start_matches("number_of")
        .trim_start_matches("count");

    // Convert to PascalCase and append "Info"
    let base_name = to_pascal_case(name, PascalCaseMode::Soft);

    // Remove trailing "s" (plural) when present
    let singular = if base_name.ends_with('s') && base_name.len() > 1 {
        &base_name[..base_name.len() - 1]
    } else {
        &base_name
    };

    format!("{}Info", singular)
}

/// Convert the struct name into a plural snake_case array field name.
///
/// **Examples**:
/// - "SatelliteInfo" → "satellites"
/// - "ReferenceStationInfo" → "reference_stations"
fn pluralize_field_name(struct_name: &str) -> String {
    // Remove "Info" suffix
    let base = struct_name.trim_end_matches("Info");

    // Convert to snake_case (empty suffix)
    let snake = to_snake_case(base, "");

    // Append "s" to form the plural
    format!("{}s", snake)
}

/// Compute the maximum allowed number of repetitions.
///
/// **Logic**
/// 1. Compute the bit-size of a single group instance
/// 2. Determine remaining payload space (223 bytes max)
/// 3. Divide to obtain the maximum instance count
/// 4. Clamp to a reasonable value (32 by default)
///
/// # Arguments
/// * `pgn` – PGN definition
/// * `start_index` – index of the first field in the group
/// * `size` – number of fields in the group
fn calculate_max_repetitions(pgn: &PgnInstructions, start_index: usize, size: usize) -> usize {
    const MAX_FAST_PACKET_BYTES: usize = 223;
    const DEFAULT_MAX: usize = 32;
    const BITS_PER_BYTE: usize = 8;

    // Compute size (in bits) of a single group instance
    let mut group_size_bits = 0;
    for i in start_index..(start_index + size).min(pgn.fields.len()) {
        if let Some(field) = pgn.fields.get(i) {
            group_size_bits += field.bits_length.unwrap_or(8) as usize;
        }
    }

    if group_size_bits == 0 {
        return DEFAULT_MAX;
    }

    // Compute bit-size of the fixed portion (before the repeating group)
    let mut fixed_size_bits = 0;
    for i in 0..start_index.min(pgn.fields.len()) {
        if let Some(field) = pgn.fields.get(i) {
            fixed_size_bits += field.bits_length.unwrap_or(8) as usize;
        }
    }

    // Remaining space available for repetitions
    let available_bits = (MAX_FAST_PACKET_BYTES * BITS_PER_BYTE).saturating_sub(fixed_size_bits);

    // Nombre max d'instances
    let calculated_max = available_bits / group_size_bits;

    // Clamp to a reasonable value
    calculated_max.min(DEFAULT_MAX)
}

/// Generate the Rust struct definition for a repeating-field group.
///
/// **Example output**:
/// ```rust
/// /// GPS satellite information
/// ///
/// /// Represents a single element of the repeating data array in the parent PGN.
/// /// The number of instances is controlled by the 'prn' counter (index 4).
/// #[derive(Debug, Clone, Copy, PartialEq)]
/// pub struct SatelliteInfo {
///     /// Elevation angle of satellite (degrees)
///     pub elevation: i16,
///     /// Azimuth angle of satellite (degrees)
///     pub azimuth: u16,
///     /// Signal-to-Noise Ratio (dB)
///     pub snr: u16,
///     // ... additional fields ...
/// }
/// ```
pub(crate) fn generate_repetitive_struct(
    pgn: &PgnInstructions,
    info: &RepeatingFieldSetInfo,
    lookup_enum_map: &HashMap<String, LookupEnum>,
    lookup_indir_map: &HashMap<String, LookupIndirEnum>,
) -> Result<String, BuildError> {
    let mut buffer = String::new();

    // Emit structure documentation
    writeln!(
        buffer,
        "/// Repeating-field structure for PGN {} - {}",
        pgn.pgn_id, info.struct_name
    )?;
    writeln!(buffer, "///")?;
    writeln!(
        buffer,
        "/// Represents a single element of the parent PGN's repeating data array."
    )?;
    writeln!(
        buffer,
        "/// The number of instances is driven by the counter field at index {}.",
        info.count_field_index
            .map(|i| i.to_string())
            .unwrap_or_else(|| "dynamic".to_string())
    )?;
    writeln!(
        buffer,
        "/// Maximum number of instances: {}",
        info.max_repetitions
    )?;
    writeln!(buffer, "#[derive(Debug, Clone, Copy, PartialEq)]")?;
    writeln!(buffer, "pub struct {} {{", info.struct_name)?;

    // Generate fields for the repeating group
    let end_index = (info.start_field_index + info.size).min(pgn.fields.len());
    for i in info.start_field_index..end_index {
        if let Some(field) = pgn.fields.get(i) {
            // Emit field documentation
            if !field.name.is_empty() {
                writeln!(buffer, "\t/// {}", field.name)?;
            }

            // Determine the Rust field type
            let rust_type = map_type(field, lookup_enum_map, lookup_indir_map)?;
            let field_name = to_snake_case(&field.id, "");

            writeln!(buffer, "\tpub {}: {},", field_name, rust_type)?;
        }
    }

    writeln!(buffer, "}}\n")?;

    // Generate the Default implementation
    writeln!(buffer, "impl Default for {} {{", info.struct_name)?;
    writeln!(buffer, "\tfn default() -> Self {{")?;
    writeln!(buffer, "\t\tSelf {{")?;

    for i in info.start_field_index..end_index {
        if let Some(field) = pgn.fields.get(i) {
            let field_name = to_snake_case(&field.id, "");
            writeln!(buffer, "\t\t\t{}: Default::default(),", field_name)?;
        }
    }

    writeln!(buffer, "\t\t}}")?;
    writeln!(buffer, "\t}}")?;
    writeln!(buffer, "}}\n")?;

    Ok(buffer)
}

/// Generate auxiliary fields inside the parent PGN struct.
///
/// **Example output**:
/// ```rust
/// /// Array of visible satellites
/// ///
/// /// Maximum size: 16 satellites
/// /// Number of valid entries: see the 'satellites_count' field
/// pub satellites: [SatelliteInfo; 16],
///
/// /// Number of valid satellites in the array
/// ///
/// /// This value references the 'prn' counter and indicates how many entries
/// /// in the 'satellites' array are populated.
/// ///
/// /// **Invariant**: must always be ≤ 16
/// pub satellites_count: usize,
/// ```
pub(crate) fn generate_repetitive_fields(
    info: &RepeatingFieldSetInfo,
) -> Result<String, BuildError> {
    let mut buffer = String::new();

    // Array field
    writeln!(
        buffer,
        "\t/// Repeating data array ({})",
        info.array_field_name
    )?;
    writeln!(buffer, "\t///")?;
    writeln!(
        buffer,
        "\t/// Maximum size: {} elements",
        info.max_repetitions
    )?;
    writeln!(
        buffer,
        "\t/// Number of valid elements: see the '{}' field",
        info.count_field_name
    )?;
    writeln!(
        buffer,
        "\tpub {}: [{}; {}],",
        info.array_field_name, info.struct_name, info.max_repetitions
    )?;

    // Counter field
    writeln!(buffer)?;
    writeln!(
        buffer,
        "\t/// Number of valid elements in the '{}' array",
        info.array_field_name
    )?;
    writeln!(buffer, "\t///")?;

    if let Some(counter_idx) = info.count_field_index {
        writeln!(
            buffer,
            "\t/// This value corresponds to the counter field at index {}",
            counter_idx
        )?;
    }

    writeln!(
        buffer,
        "\t/// and indicates how many array entries are populated."
    )?;
    writeln!(buffer, "\t///")?;
    writeln!(
        buffer,
        "\t/// **Invariant**: must always be ≤ {}",
        info.max_repetitions
    )?;
    writeln!(buffer, "\tpub {}: usize,", info.count_field_name)?;

    Ok(buffer)
}

/// Generate FieldAccess helper implementations for repeating fields.
///
/// **Generated output**:
/// ```rust,ignore
/// fn repetitive_field(&self, array_id: &'static str, index: usize, field_id: &'static str) -> Option<PgnValue> {
///     match array_id {
///         "reference_station_types" => {
///             if index >= self.reference_station_types_count {
///                 return None;
///             }
///             match field_id {
///                 "ReferenceStationId" => Some(PgnValue::U16(self.reference_station_types[index].reference_station_id)),
///                 "AgeOfDgnssCorrections" => Some(PgnValue::F32(self.reference_station_types[index].age_of_dgnss_corrections)),
///                 _ => None,
///             }
///         }
///         _ => None,
///     }
/// }
/// // ... similar code is generated for repetitive_field_mut, repetitive_count, set_repetitive_count
/// ```
pub(crate) fn generate_repetitive_field_access(
    pgn: &PgnInstructions,
    info: &RepeatingFieldSetInfo,
    lookup_enum_map: &HashMap<String, LookupEnum>,
    lookup_indir_map: &HashMap<String, LookupIndirEnum>,
) -> Result<String, BuildError> {
    let mut buffer = String::new();
    let counter_field_props = if let Some(counter_idx) = info.count_field_index {
        if let Some(field) = pgn.fields.get(counter_idx) {
            Some((
                to_snake_case(&field.id, "field"),
                map_type(field, lookup_enum_map, lookup_indir_map)?,
            ))
        } else {
            None
        }
    } else {
        None
    };

    let end_index = (info.start_field_index + info.size).min(pgn.fields.len());

    // ==================== repetitive_field (read access) ====================
    writeln!(buffer)?;
    writeln!(
        buffer,
        "\tfn repetitive_field(&self, array_id: &'static str, index: usize, field_id: &'static str) -> Option<PgnValue> {{"
    )?;
    writeln!(buffer, "\t\tmatch array_id {{")?;
    writeln!(buffer, "\t\t\t\"{}\" => {{", info.array_field_name)?;

    // Bounds check
    writeln!(
        buffer,
        "\t\t\t\tif index >= self.{} {{",
        info.count_field_name
    )?;
    writeln!(buffer, "\t\t\t\t\treturn None;")?;
    writeln!(buffer, "\t\t\t\t}}")?;

    // Match on the element's fields
    writeln!(buffer, "\t\t\t\tmatch field_id {{")?;
    for i in info.start_field_index..end_index {
        if let Some(field) = pgn.fields.get(i) {
            let field_name_pascal = to_pascal_case(&field.id, PascalCaseMode::Soft);
            let field_name_snake = to_snake_case(&field.id, "");
            let field_type_str = map_type(field, lookup_enum_map, lookup_indir_map)?;
            let lookup_repr = lookup_repr_from_field(field, lookup_enum_map, lookup_indir_map);

            // For lookups, operate on the enum representation rather than the Rust enum type
            let type_for_variant = if matches!(
                map_to_fieldkind(field),
                FieldKind::Lookup | FieldKind::IndirectLookup
            ) {
                lookup_repr.unwrap_or("u8")
            } else {
                &field_type_str
            };
            let pgn_value_variant = get_pgn_value_variant_from_type(type_for_variant, field)?;

            if matches!(
                map_to_fieldkind(field),
                FieldKind::Lookup | FieldKind::IndirectLookup
            ) {
                // For lookups, convert the enum into its primitive representation
                if let Some(repr) = lookup_repr {
                    let cast_type = match repr {
                        "u16" => "u16",
                        "u32" => "u32",
                        _ => "u8",
                    };
                    writeln!(
                        buffer,
                        "\t\t\t\t\t\"{}\" => Some({}({}::from(self.{}[index].{}))),",
                        field_name_pascal,
                        pgn_value_variant,
                        cast_type,
                        info.array_field_name,
                        field_name_snake
                    )?;
                } else {
                    writeln!(
                        buffer,
                        "\t\t\t\t\t\"{}\" => Some({}(u8::from(self.{}[index].{}))),",
                        field_name_pascal,
                        pgn_value_variant,
                        info.array_field_name,
                        field_name_snake
                    )?;
                }
            } else {
                writeln!(
                    buffer,
                    "\t\t\t\t\t\"{}\" => Some({}(self.{}[index].{})),",
                    field_name_pascal, pgn_value_variant, info.array_field_name, field_name_snake
                )?;
            }
        }
    }
    writeln!(buffer, "\t\t\t\t\t_ => None,")?;
    writeln!(buffer, "\t\t\t\t}}")?;
    writeln!(buffer, "\t\t\t}}")?;
    writeln!(buffer, "\t\t\t_ => None,")?;
    writeln!(buffer, "\t\t}}")?;
    writeln!(buffer, "\t}}")?;

    // ==================== repetitive_field_mut (write access) ====================
    writeln!(buffer)?;
    writeln!(
        buffer,
        "\tfn repetitive_field_mut(&mut self, array_id: &'static str, index: usize, field_id: &'static str, value: PgnValue) -> Option<()> {{"
    )?;
    writeln!(buffer, "\t\tmatch array_id {{")?;
    writeln!(buffer, "\t\t\t\"{}\" => {{", info.array_field_name)?;

    // Bounds check
    writeln!(
        buffer,
        "\t\t\t\tif index >= self.{} {{",
        info.count_field_name
    )?;
    writeln!(buffer, "\t\t\t\t\treturn None;")?;
    writeln!(buffer, "\t\t\t\t}}")?;

    // Match on the element's fields
    writeln!(buffer, "\t\t\t\tmatch field_id {{")?;
    for i in info.start_field_index..end_index {
        if let Some(field) = pgn.fields.get(i) {
            let field_name_pascal = to_pascal_case(&field.id, PascalCaseMode::Soft);
            let field_name_snake = to_snake_case(&field.id, "");
            let field_type_str = map_type(field, lookup_enum_map, lookup_indir_map)?;
            let lookup_repr = lookup_repr_from_field(field, lookup_enum_map, lookup_indir_map);

            // For lookups, operate on the enum representation rather than the Rust enum type
            let type_for_variant = if matches!(
                map_to_fieldkind(field),
                FieldKind::Lookup | FieldKind::IndirectLookup
            ) {
                lookup_repr.unwrap_or("u8")
            } else {
                &field_type_str
            };
            let pgn_value_variant = get_pgn_value_variant_from_type(type_for_variant, field)?;

            writeln!(buffer, "\t\t\t\t\t\"{}\" => {{", field_name_pascal)?;

            if matches!(
                map_to_fieldkind(field),
                FieldKind::Lookup | FieldKind::IndirectLookup
            ) {
                // For lookups, convert the primitive value back into the enum
                writeln!(
                    buffer,
                    "\t\t\t\t\t\tif let {}(val) = value {{",
                    pgn_value_variant
                )?;
                writeln!(
                    buffer,
                    "\t\t\t\t\t\t\tmatch {}::try_from(val) {{",
                    field_type_str
                )?;
                writeln!(buffer, "\t\t\t\t\t\t\t\tOk(enum_val) => {{")?;
                writeln!(
                    buffer,
                    "\t\t\t\t\t\t\t\t\tself.{}[index].{} = enum_val;",
                    info.array_field_name, field_name_snake
                )?;
                writeln!(buffer, "\t\t\t\t\t\t\t\t\tSome(())")?;
                writeln!(buffer, "\t\t\t\t\t\t\t\t}}")?;
                writeln!(buffer, "\t\t\t\t\t\t\t\tErr(_) => None")?;
                writeln!(buffer, "\t\t\t\t\t\t\t}}")?;
                writeln!(buffer, "\t\t\t\t\t\t}} else {{")?;
                writeln!(buffer, "\t\t\t\t\t\t\tNone")?;
                writeln!(buffer, "\t\t\t\t\t\t}}")?;
            } else {
                writeln!(
                    buffer,
                    "\t\t\t\t\t\tif let {}(val) = value {{",
                    pgn_value_variant
                )?;
                writeln!(
                    buffer,
                    "\t\t\t\t\t\t\tself.{}[index].{} = val;",
                    info.array_field_name, field_name_snake
                )?;
                writeln!(buffer, "\t\t\t\t\t\t\tSome(())")?;
                writeln!(buffer, "\t\t\t\t\t\t}} else {{")?;
                writeln!(buffer, "\t\t\t\t\t\t\tNone")?;
                writeln!(buffer, "\t\t\t\t\t\t}}")?;
            }

            writeln!(buffer, "\t\t\t\t\t}}")?;
        }
    }
    writeln!(buffer, "\t\t\t\t\t_ => None,")?;
    writeln!(buffer, "\t\t\t\t}}")?;
    writeln!(buffer, "\t\t\t}}")?;
    writeln!(buffer, "\t\t\t_ => None,")?;
    writeln!(buffer, "\t\t}}")?;
    writeln!(buffer, "\t}}")?;

    // ==================== repetitive_count ====================
    writeln!(buffer)?;
    writeln!(
        buffer,
        "\tfn repetitive_count(&self, array_id: &'static str) -> Option<usize> {{"
    )?;
    writeln!(buffer, "\t\tmatch array_id {{")?;
    writeln!(
        buffer,
        "\t\t\t\"{}\" => Some(self.{}),",
        info.array_field_name, info.count_field_name
    )?;
    writeln!(buffer, "\t\t\t_ => None,")?;
    writeln!(buffer, "\t\t}}")?;
    writeln!(buffer, "\t}}")?;

    // ==================== set_repetitive_count ====================
    writeln!(buffer)?;
    writeln!(
        buffer,
        "\tfn set_repetitive_count(&mut self, array_id: &'static str, count: usize) -> Option<()> {{"
    )?;
    writeln!(buffer, "\t\tmatch array_id {{")?;
    writeln!(buffer, "\t\t\t\"{}\" => {{", info.array_field_name)?;

    // Validate that the requested count does not exceed max_repetitions
    writeln!(buffer, "\t\t\t\tif count > {} {{", info.max_repetitions)?;
    writeln!(buffer, "\t\t\t\t\treturn None;")?;
    writeln!(buffer, "\t\t\t\t}}")?;
    writeln!(buffer, "\t\t\t\tself.{} = count;", info.count_field_name)?;
    if let Some((ref counter_field_name, ref counter_field_type)) = counter_field_props {
        writeln!(
            buffer,
            "\t\t\t\tself.{} = count as {};",
            counter_field_name, counter_field_type
        )?;
    }
    writeln!(buffer, "\t\t\t\tSome(())")?;
    writeln!(buffer, "\t\t\t}}")?;
    writeln!(buffer, "\t\t\t_ => None,")?;
    writeln!(buffer, "\t\t}}")?;
    writeln!(buffer, "\t}}")?;

    Ok(buffer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_struct_name() {
        assert_eq!(
            derive_struct_name_from_counter("numberOfSatellites"),
            "SatelliteInfo"
        );
        assert_eq!(
            derive_struct_name_from_counter("referenceStations"),
            "ReferenceStationInfo"
        );
        assert_eq!(derive_struct_name_from_counter("itemCount"), "ItemInfo");
    }

    #[test]
    fn test_pluralize_field_name() {
        assert_eq!(pluralize_field_name("SatelliteInfo"), "satellites");
        assert_eq!(
            pluralize_field_name("ReferenceStationInfo"),
            "reference_stations"
        );
        assert_eq!(pluralize_field_name("ItemInfo"), "items");
    }
}
