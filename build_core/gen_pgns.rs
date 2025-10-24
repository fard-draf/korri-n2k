//! Generate Rust code for the PGNs selected in the manifest.
use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt::Write;

use crate::core::FieldKind;
use serde_json::Value;

use crate::build_core::gen_lookups::generate_indirect_lookup_helpers;
use crate::build_core::gen_lookups::{
    set_lookup_bit_map, set_lookup_enum_map, set_lookup_indir_map, set_poly_lookup_map,
};

use super::domain::*;
use super::errors::*;
use super::name_helpers::*;
use super::repetitive_fields::*;
use super::type_helpers::*;

/// Walk through the CANboat database and emit code for the requested PGNs.
pub(crate) fn run_pgns_gen(
    canboat_value: &Value,
    pgns_to_generate: Vec<u32>,
) -> Result<String, BuildError> {
    // Prepare tracking structures (polymorphic PGNs, caches, etc.).
    let lookup_enum_map = set_lookup_enum_map(canboat_value)?;
    let lookup_indir_map = set_lookup_indir_map(canboat_value)?;
    let lookup_bit_map = set_lookup_bit_map(canboat_value)?;
    let poly_lookup_map = set_poly_lookup_map(canboat_value)?;
    let pgns_set = set_pgns_set(canboat_value)?;
    let mut poly_pgns_map = set_poly_pgns_map(canboat_value, pgns_set)?;

    let mut buffer_pgn_code = String::new();

    writeln!(&mut buffer_pgn_code, "use super::lookups::*;")?;
    writeln!(
        buffer_pgn_code,
        "use crate::core::{{PgnDescriptor, PgnValue, PgnBytes, RepeatingFieldSet}};\n\n"
    )?;

    if let Some(pgn_array) = canboat_value["PGNs"].as_array() {
        let mut poly_pgns_id_vec = Vec::new();
        for pgn_value in pgn_array {
            match serde_json::from_value::<PgnInstructions>(pgn_value.clone()) {
                Ok(pgn_def) => {
                    if !pgns_to_generate.contains(&pgn_def.pgn_id) {
                        continue;
                    }

                    match generate_pgn_code(
                        &pgn_def,
                        &poly_lookup_map,
                        &lookup_enum_map,
                        &lookup_indir_map,
                        &lookup_bit_map,
                        &mut poly_pgns_map,
                        &mut poly_pgns_id_vec,
                    ) {
                        Ok(pgn_code) => buffer_pgn_code.push_str(&pgn_code),
                        Err(e) => {
                            println!(
                                "cargo:warning=[PGN {}] Failed to generate code: {}",
                                pgn_def.pgn_id, e
                            );
                            continue;
                        }
                    }
                }
                Err(e) => {
                    let pgn_id = pgn_value.get("PGN").unwrap_or(&serde_json::Value::Null);
                    println!(
                        "cargo:warning=[PGN {}] Skipped.. Malformed definition: {}",
                        pgn_id, e
                    );
                    continue;
                }
            }
        }
    }
    Ok(buffer_pgn_code)
}

/// Assemble code (struct/impl/enum) for a specific PGN.
fn generate_pgn_code(
    pgn: &PgnInstructions,
    poly_lookup_map: &HashMap<String, LookupEnum>,
    lookup_enum_map: &HashMap<String, LookupEnum>,
    lookup_indir_map: &HashMap<String, LookupIndirEnum>,
    lookup_bit_map: &HashMap<String, LookupBitEnum>,
    poly_pgns_map: &mut HashMap<u32, Vec<PolyPgn>>,
    poly_pgns_id_vec: &mut Vec<u32>,
) -> Result<String, BuildError> {
    // Guard: skip PGNs with multiple repeating groups (not supported yet).
    // TODO: support multiple repeating groups (RepeatingFieldSet2, RepeatingFieldSet3)
    if pgn.repeating_field_set_2_size.is_some() {
        return Ok(String::new());
    }

    let mut buffer = String::new();

    // Extract metadata for repeating fields (if any)
    let repeating_info = RepeatingFieldSetInfo::extract_from_pgn(pgn, 1);

    // Generate the repeating-field structure when applicable
    if let Some(ref info) = repeating_info {
        buffer.push_str(&generate_repetitive_struct(
            pgn,
            info,
            lookup_enum_map,
            lookup_indir_map,
        )?);
    }

    // Generate the polymorphic enumeration when applicable
    if poly_pgns_map.contains_key(&pgn.pgn_id) {
        buffer.push_str(&generate_enum_definition(pgn, poly_pgns_map)?);
        buffer.push_str(&generate_enum_trait_impl(
            pgn,
            poly_lookup_map,
            poly_pgns_map,
        )?);
        poly_pgns_map.remove(&pgn.pgn_id);
        poly_pgns_id_vec.push(pgn.pgn_id);
    }

    let is_poly = poly_pgns_id_vec.contains(&pgn.pgn_id);
    buffer.push_str(&generate_struct_definition(
        pgn,
        is_poly,
        repeating_info.as_ref(),
        lookup_enum_map,
        lookup_indir_map,
        lookup_bit_map,
    )?);
    buffer.push_str(&generate_impl_bloc_with_descriptor(
        pgn,
        is_poly,
        lookup_enum_map,
        lookup_indir_map,
        lookup_bit_map,
    )?);
    buffer.push_str(&generate_trait_impl(
        pgn,
        is_poly,
        repeating_info.as_ref(),
        lookup_enum_map,
        lookup_indir_map,
        lookup_bit_map,
    )?);

    Ok(buffer)
}

//==================================================================================GENERATE_ENUM_DEFINITION
/// Generate only the definition of the polymorphic enumeration.
fn generate_enum_definition(
    pgn: &PgnInstructions,
    poly_pgns_map: &HashMap<u32, Vec<PolyPgn>>,
) -> Result<String, BuildError> {
    let mut buffer = String::new();
    let enum_name = format! {"Pgn{}", pgn.pgn_id};

    writeln!(buffer, "#[derive(Debug, PartialEq, Copy, Clone)]")?;
    writeln!(buffer, "pub enum {} {{", enum_name)?;

    if let Some(poly_pgn_vec) = poly_pgns_map.get(&pgn.pgn_id) {
        for poly_pgn in poly_pgn_vec {
            writeln!(
                buffer,
                "\t{}(Pgn{}{}),",
                poly_pgn.name, pgn.pgn_id, poly_pgn.name
            )?;
        }
    }

    writeln!(buffer, "}}")?;
    Ok(buffer)
}

//==================================================================================GENERATE_STRUCT_DEFINITION
/// Generate the PGN structure definition (with public fields when appropriate).
fn generate_struct_definition(
    pgn: &PgnInstructions,
    is_poly: bool,
    repeating_info: Option<&RepeatingFieldSetInfo>,
    lookup_enum_map: &HashMap<String, LookupEnum>,
    lookup_indir_map: &HashMap<String, LookupIndirEnum>,
    _lookup_bit_map: &HashMap<String, LookupBitEnum>,
) -> Result<String, BuildError> {
    // TODO!: Implement PGNs with dynamic or variable-sized payloads
    let mut buffer = String::new();
    let struct_name = if is_poly {
        format!(
            "Pgn{}{}",
            pgn.pgn_id,
            to_pascal_case(&pgn.pgn_name, PascalCaseMode::Soft)
        )
    } else {
        format! {"Pgn{}", pgn.pgn_id}
    };

    writeln!(buffer, "#[derive(Debug, PartialEq, Copy, Clone)]")?;

    writeln!(buffer, "/// {}", pgn.pgn_description)?;
    if let Some(explanation) = &pgn.explanation {
        writeln!(buffer, "/// {}", explanation)?;
    }
    writeln!(buffer, "pub struct {} {{", struct_name)?;

    // Determine which fields must be excluded (those in the repeating group)
    let excluded_range = if let Some(info) = repeating_info {
        Some(info.start_field_index..(info.start_field_index + info.size))
    } else {
        None
    };

    // Generate regular fields, excluding the repeating group
    for (idx, field) in pgn.fields.iter().enumerate() {
        // Skip fields that belong to the repeating group
        if let Some(ref range) = excluded_range {
            if range.contains(&idx) {
                continue;
            }
        }

        let field_name = to_snake_case(&field.id, "field");
        let field_type = map_type(field, lookup_enum_map, lookup_indir_map)?;
        let field_kind = map_to_fieldkind(field);

        if field_kind == FieldKind::Spare || field_kind == FieldKind::Reserved {
            writeln!(buffer, "\t{}: {},", field_name, field_type)?;
        } else {
            if let Some(description) = &field.description {
                writeln!(buffer, "\t/// {},", description)?;
            }
            if let Some(direct_lookup) = &field.enum_direct_name {
                writeln!(
                    buffer,
                    "\t/// Lookup Enum: {},",
                    to_pascal_case(&direct_lookup.to_lowercase(), PascalCaseMode::Hard)
                )?;
            }
            if let Some(indirect_lookup) = &field.enum_indirect_name {
                writeln!(
                    buffer,
                    "\t/// Indirect Lookup Enum: {},",
                    to_pascal_case(&indirect_lookup.to_lowercase(), PascalCaseMode::Hard)
                )?;
            }
            if let Some(bit_lookup) = &field.enum_bit_name {
                writeln!(
                    buffer,
                    "\t/// Bit Lookup Enum: {},",
                    to_pascal_case(&bit_lookup.to_lowercase(), PascalCaseMode::Hard)
                )?;
            }
            writeln!(buffer, "\tpub {}: {},", field_name, field_type)?;
        }
    }

    // Add repeating fields (array + counter)
    if let Some(info) = repeating_info {
        buffer.push_str(&generate_repetitive_fields(info)?);
    }

    writeln!(buffer, "}}")?;
    Ok(buffer)
}
//==================================================================================GENERATE_ENUM_IMPL
/// Generate trait implementations (`PgnData`, `FieldAccess`) for polymorphic PGNs.
fn generate_enum_trait_impl(
    pgn: &PgnInstructions,
    poly_lookup_map: &HashMap<String, LookupEnum>,
    poly_pgns_map: &mut HashMap<u32, Vec<PolyPgn>>,
) -> Result<String, BuildError> {
    let mut buffer = String::new();

    //==========================================impl PgnData
    writeln!(buffer, "impl PgnData for Pgn{} {{", pgn.pgn_id)?;
    //======================fn from_payload
    writeln!(
        buffer,
        "\tfn from_payload(payload: &[u8]) -> Result<Self, DeserializationError> {{"
    )?;
    writeln!(
        buffer,
        "\t\tlet first_field = &Pgn{}{}::PGN_{}_{}_DESCRIPTOR.fields[0];",
        pgn.pgn_id,
        to_pascal_case(&pgn.pgn_name, PascalCaseMode::Soft),
        pgn.pgn_id,
        to_snake_case(&pgn.pgn_name, "POLY").to_uppercase()
    )?;
    writeln!(
        buffer,
        "\t\tlet mut reader = crate::infra::codec::bits::BitReader::new(payload);"
    )?;
    writeln!(
        buffer,
        "\t\tlet function_bits: u8 = first_field.bits_length.unwrap_or(8) as u8;"
    )?;
    writeln!(buffer, "\t\tlet function_code = reader.read_u64(function_bits).map_err(|_| DeserializationError::InvalidDataLength)? as u32;")?;
    writeln!(buffer)?;

    writeln!(buffer, "\t\tmatch function_code {{")?;
    generate_enum_impl_helper(
        &mut buffer,
        pgn,
        poly_pgns_map,
        poly_lookup_map,
        |writer, lookup, poly_pgn| {
            writeln!(writer, "\t\t\t{} => {{", lookup.value)?;
            writeln!(
                writer,
                "\t\t\t\tlet mut inner_struct = Pgn{}{}::new();",
                pgn.pgn_id, poly_pgn.name
            )?;
            writeln!(
                writer,
                "\t\t\t\tcrate::infra::codec::engine::deserialize_into("
            )?;
            writeln!(writer, "\t\t\t\t\t&mut inner_struct,")?;
            writeln!(writer, "\t\t\t\t\tpayload,")?;
            writeln!(
                writer,
                "\t\t\t\t\t&Pgn{}{}::PGN_{}_{}_DESCRIPTOR,",
                pgn.pgn_id,
                poly_pgn.name,
                pgn.pgn_id,
                to_snake_case(&poly_pgn.name, "POLY").to_uppercase()
            )?;
            writeln!(writer, "\t\t\t\t)?;")?;
            writeln!(
                writer,
                "\t\t\t\tOk(Pgn{}::{}(inner_struct))",
                pgn.pgn_id, poly_pgn.name
            )?;
            writeln!(writer, "\t\t\t}}")?;
            writeln!(writer)
        },
    )?;
    writeln!(
        buffer,
        "\t\t\t_ => return Err(DeserializationError::MalformedData),"
    )?;

    writeln!(buffer, "\t\t}}")?; // End of match function_code
    writeln!(buffer, "\t}}")?; // End of from_payload
    writeln!(buffer)?;

    //======================fn to_payload
    writeln!(buffer, "\tfn to_payload(&self, buffer: &mut [u8]) -> Result<usize, crate::error::SerializationError> {{")?;
    writeln!(buffer, "\t\tmatch self {{")?;

    generate_enum_impl_helper(
        &mut buffer,
        pgn,
        poly_pgns_map,
        poly_lookup_map,
        |writer, _lookup, poly_pgn| {
            writeln!(
                writer,
                "\t\t\tPgn{}::{}(inner) => crate::infra::codec::engine::serialize(",
                pgn.pgn_id, poly_pgn.name
            )?;
            writeln!(writer, "\t\t\t\tinner,")?;
            writeln!(writer, "\t\t\t\tbuffer,")?;
            writeln!(
                writer,
                "\t\t\t\t&Pgn{}{}::PGN_{}_{}_DESCRIPTOR,",
                pgn.pgn_id,
                poly_pgn.name,
                pgn.pgn_id,
                to_snake_case(&poly_pgn.name, "POLY").to_uppercase()
            )?;
            writeln!(writer, "\t\t\t),")?;
            writeln!(writer)
        },
    )?;
    writeln!(buffer, "\t\t}}")?; // end match_self
    writeln!(buffer, "\t}}")?; // end to_payload
    writeln!(buffer, "}}")?; // end impl PgnData
    writeln!(buffer)?;

    //==========================================impl FieldAccess
    writeln!(buffer, "impl FieldAccess for Pgn{} {{", pgn.pgn_id)?;
    //======================fn field
    writeln!(
        buffer,
        "\tfn field(&self, id: &'static str) -> Option<PgnValue> {{"
    )?;
    writeln!(buffer, "\t\tmatch self {{")?;
    generate_enum_impl_helper(
        &mut buffer,
        pgn,
        poly_pgns_map,
        poly_lookup_map,
        |writer, _lookup, poly_pgn| {
            writeln!(
                writer,
                "\t\t\tPgn{}::{}(inner) => inner.field(id),",
                pgn.pgn_id, poly_pgn.name
            )
        },
    )?;
    writeln!(buffer, "\t\t}}")?; // end match_self
    writeln!(buffer, "\t}}")?; // end field
    writeln!(buffer)?;
    //======================fn field_mut
    writeln!(
        buffer,
        "\tfn field_mut(&mut self, id: &'static str, value: PgnValue) -> Option<()> {{"
    )?;
    writeln!(buffer, "\t\tmatch self {{")?;
    generate_enum_impl_helper(
        &mut buffer,
        pgn,
        poly_pgns_map,
        poly_lookup_map,
        |writer, _lookup, poly_pgn| {
            writeln!(
                writer,
                "\t\t\tPgn{}::{}(inner) => inner.field_mut(id, value),",
                pgn.pgn_id, poly_pgn.name
            )
        },
    )?;
    writeln!(buffer, "\t\t}}")?; // end match_self
    writeln!(buffer, "\t}}")?; // end field_mut
    writeln!(buffer, "}}")?; // end impl FieldAccess
    writeln!(buffer)?;
    Ok(buffer)
}
//==================================================================================HELPER_IMPL_POLY_PGN
/// Iterate over the polymorphic variants of a PGN and execute a writer callback.
fn generate_enum_impl_helper<W, F>(
    writer: &mut W,
    pgn: &PgnInstructions,
    poly_pgns_map: &HashMap<u32, Vec<PolyPgn>>,
    poly_lookup_map: &HashMap<String, LookupEnum>,
    mut action: F,
) -> Result<(), BuildError>
where
    W: Write,
    F: FnMut(&mut W, &EnumValues, &PolyPgn) -> Result<(), std::fmt::Error>,
{
    if let Some(poly_pgn_vec) = poly_pgns_map.get(&pgn.pgn_id) {
        for poly_pgn in poly_pgn_vec {
            if let Some(poly_lookup) = poly_lookup_map.get(&poly_pgn.lookup_id) {
                for lookup in &poly_lookup.enum_values {
                    if poly_pgn.desc == lookup.name {
                        action(writer, lookup, poly_pgn)?;
                    }
                }
            }
        }
    }
    Ok(())
}
//==================================================================================GENERATE_IMPL_BLOC
/// Generate the `impl` block containing the `PgnDescriptor` constant (binary structure reference).
// This acts as the “source of truth” for the PGN binary layout.
fn generate_impl_bloc_with_descriptor(
    pgn: &PgnInstructions,
    is_poly: bool,
    lookup_enum_map: &HashMap<String, LookupEnum>,
    lookup_indir_map: &HashMap<String, LookupIndirEnum>,
    _lookup_bit_map: &HashMap<String, LookupBitEnum>,
) -> Result<String, BuildError> {
    // Extract repeating-field information.
    // Note: unlike struct/trait generation we MUST keep repeating fields inside the
    // descriptor because the codec engine relies on them for the binary layout.
    let repeating_info = RepeatingFieldSetInfo::extract_from_pgn(pgn, 1);
    let mut buffer = String::new();
    let pgn_id = if is_poly {
        format!(
            "{}{}",
            pgn.pgn_id,
            to_pascal_case(&pgn.pgn_name, PascalCaseMode::Soft)
        )
    } else {
        format!("{}", pgn.pgn_id)
    };
    let struct_name = format!("Pgn{}", pgn_id);
    let decriptor_name = if is_poly {
        format!(
            "PGN_{}_DESCRIPTOR",
            to_snake_case(&pgn_id, "POLY").to_uppercase()
        )
    } else {
        format!("PGN_{}_DESCRIPTOR", pgn_id)
    };
    let is_fastpacket = pgn.fastpacket.eq_ignore_ascii_case("fast");

    writeln!(buffer, "impl {} {{", struct_name)?;
    writeln!(
        buffer,
        "\tpub const {}: PgnDescriptor = PgnDescriptor {{",
        decriptor_name
    )?;
    writeln!(buffer, "\t\tid: {},", pgn.pgn_id)?;
    writeln!(
        buffer,
        "\t\tname: \"{}\",",
        to_pascal_case(&pgn.pgn_name, PascalCaseMode::Soft)
    )?;
    writeln!(buffer, "\t\tdescription: \"{}\",", pgn.pgn_description)?;
    writeln!(buffer, "\t\tpriority: {:?},", pgn.priority)?;
    writeln!(buffer, "\t\tfastpacket: {},", is_fastpacket)?;
    writeln!(buffer, "\t\tlength: {:?},", pgn.length)?;
    writeln!(buffer, "\t\tfield_count: {:?},", pgn.field_count)?;
    writeln!(buffer, "\t\ttrans_interval: {:?},", pgn.trans_interval)?;
    writeln!(buffer, "\t\ttrans_irregular: {:?},", pgn.trans_irregular)?;
    writeln!(buffer, "\t\tfields: &[")?;

    // Emit every field, including those belonging to repeating groups, so the codec
    // engine has an accurate binary descriptor.
    for (_idx, field) in pgn.fields.iter().enumerate() {
        let is_resolution = field.resolution.filter(|&r| r != 1.0);
        let is_signed = field.signed.filter(|&s| s);

        let pascal_cased = |name: &Option<String>| {
            name.as_ref()
                .map(|str| to_pascal_case(&str.to_lowercase(), PascalCaseMode::Hard))
        };

        writeln!(buffer, "\t\t\tFieldDescriptor {{")?;
        writeln!(
            buffer,
            "\t\t\t\tid: \"{}\",",
            to_pascal_case(&field.id, PascalCaseMode::Soft)
        )?;
        writeln!(buffer, "\t\t\t\tname: \"{}\",", field.name)?;
        writeln!(
            buffer,
            "\t\t\t\tkind: FieldKind::{:?},",
            map_to_fieldkind(field)
        )?;
        writeln!(buffer, "\t\t\t\tbits_length: {:?},", field.bits_length)?;
        let bits_length_var = if field.bits_length_var.unwrap_or(false) {
            Some(0u32)
        } else {
            None
        };
        writeln!(buffer, "\t\t\t\tbits_length_var: {:?},", bits_length_var)?;
        writeln!(buffer, "\t\t\t\tbits_offset: {:?},", field.bits_offset)?;
        writeln!(buffer, "\t\t\t\tis_signed: {:?},", is_signed)?;
        writeln!(buffer, "\t\t\t\tresolution: {:?},", is_resolution)?;
        writeln!(
            buffer,
            "\t\t\t\tenum_direct_name: {:?},",
            pascal_cased(&field.enum_direct_name)
        )?;
        writeln!(
            buffer,
            "\t\t\t\tenum_indirect_name: {:?},",
            pascal_cased(&field.enum_indirect_name)
        )?;
        writeln!(
            buffer,
            "\t\t\t\tenum_indirect_field_order: {:?},",
            field.enum_indirect_field_order
        )?;
        writeln!(buffer, "\t\t\t\tphysical_unit: {:?},", field.physical_unit)?;
        writeln!(buffer, "\t\t\t\tphysical_qtity: {:?},", field.physical_qty)?;
        writeln!(buffer, "\t\t\t}},")?;
    }
    writeln!(buffer, "\t\t],")?;

    // Add repeating-group metadata to the descriptor
    if let Some(ref info) = repeating_info {
        writeln!(buffer, "\t\trepeating_field_sets: &[")?;
        writeln!(buffer, "\t\t\tRepeatingFieldSet {{")?;
        writeln!(buffer, "\t\t\t\tarray_id: \"{}\",", info.array_field_name)?;
        writeln!(
            buffer,
            "\t\t\t\tcount_field_index: {:?},",
            info.count_field_index
        )?;
        writeln!(
            buffer,
            "\t\t\t\tstart_field_index: {},",
            info.start_field_index
        )?;
        writeln!(buffer, "\t\t\t\tsize: {},", info.size)?;
        writeln!(buffer, "\t\t\t\tmax_repetitions: {},", info.max_repetitions)?;
        writeln!(buffer, "\t\t\t}},")?;
        writeln!(buffer, "\t\t],")?;
    } else {
        writeln!(buffer, "\t\trepeating_field_sets: &[],")?;
    }

    writeln!(buffer, "\t}};")?;
    writeln!(buffer)?;
    buffer.push_str(&generate_new_fn(
        pgn,
        RepeatingFieldSetInfo::extract_from_pgn(pgn, 1).as_ref(),
        lookup_enum_map,
        lookup_indir_map,
    )?);
    writeln!(buffer)?;

    // Generate helper methods for INDIRECT_LOOKUP fields
    // These lookups combine two u8 fields to build a u16-backed enum
    for field in &pgn.fields {
        if map_to_fieldkind(field) == FieldKind::IndirectLookup {
            if let (Some(enum_name), Some(field_order)) =
                (&field.enum_indirect_name, field.enum_indirect_field_order)
            {
                // Find the master field that provides the high byte
                if let Some(master_field) = pgn.fields.iter().find(|f| f.order == field_order) {
                    let master_type = map_type(master_field, lookup_enum_map, lookup_indir_map)?;
                    buffer.push_str(&generate_indirect_lookup_helpers(
                        &master_field.id,
                        &field.id,
                        enum_name,
                        &master_type,
                    )?);
                }
            }
        }
    }

    // Generate helper functions for BITLOOKUP fields (bitmasks)
    for field in &pgn.fields {
        if map_to_fieldkind(field) == FieldKind::BitLookup {
            if let Some(enum_bit_name) = &field.enum_bit_name {
                let field_type = map_type(field, lookup_enum_map, lookup_indir_map)?;
                buffer.push_str(&generate_bitlookup_helpers(
                    &field.id,
                    enum_bit_name,
                    &field_type,
                )?);
            }
        }
    }

    writeln!(buffer, "}}")?;
    writeln!(buffer)?;
    Ok(buffer)
}

/// Generate helpers to test/modify bits within a BITLOOKUP field.
fn generate_bitlookup_helpers(
    field_id: &str,
    enum_bit_name: &str,
    field_type: &str,
) -> Result<String, BuildError> {
    let mut buffer = String::new();
    let field_snake = to_snake_case(field_id, "field");
    let enum_name_pascal = to_pascal_case(&enum_bit_name.to_lowercase(), PascalCaseMode::Hard);

    // Getter: test whether a specific bit is set
    let getter_name = format!("get_{}_bit", &field_snake);
    writeln!(buffer)?;
    writeln!(
        buffer,
        "\t/// Returns true when the specified bit is set in {}",
        field_snake
    )?;
    writeln!(
        buffer,
        "\tpub fn {}(&self, bit: {}) -> bool {{",
        getter_name, enum_name_pascal
    )?;
    writeln!(buffer, "\t\tlet bit_position = bit as {};", field_type)?;
    writeln!(
        buffer,
        "\t\t(self.{} & (1 << bit_position)) != 0",
        field_snake
    )?;
    writeln!(buffer, "\t}}")?;

    // Setter: enable or disable a specific bit
    let setter_name = format!("set_{}_bit", &field_snake);
    writeln!(buffer)?;
    writeln!(
        buffer,
        "\t/// Enable or disable the specified bit in {}",
        field_snake
    )?;
    writeln!(
        buffer,
        "\tpub fn {}(&mut self, bit: {}, value: bool) {{",
        setter_name, enum_name_pascal
    )?;
    writeln!(buffer, "\t\tlet bit_position = bit as {};", field_type)?;
    writeln!(buffer, "\t\tif value {{")?;
    writeln!(buffer, "\t\t\tself.{} |= 1 << bit_position;", field_snake)?;
    writeln!(buffer, "\t\t}} else {{")?;
    writeln!(
        buffer,
        "\t\t\tself.{} &= !(1 << bit_position);",
        field_snake
    )?;
    writeln!(buffer, "\t\t}}")?;
    writeln!(buffer, "\t}}")?;

    Ok(buffer)
}

//==================================================================================TRAIT_IMPL
/// Generate `PgnData` / `FieldAccess` implementations for a non-polymorphic PGN.
fn generate_trait_impl(
    pgn: &PgnInstructions,
    is_poly: bool,
    repeating_info: Option<&RepeatingFieldSetInfo>,
    lookup_enum_map: &HashMap<String, LookupEnum>,
    lookup_indir_map: &HashMap<String, LookupIndirEnum>,
    _lookup_bit_map: &HashMap<String, LookupBitEnum>,
) -> Result<String, BuildError> {
    let mut buffer = String::new();
    let pgn_id = if is_poly {
        format!(
            "{}{}",
            pgn.pgn_id,
            to_pascal_case(&pgn.pgn_name, PascalCaseMode::Soft)
        )
    } else {
        format!("{}", pgn.pgn_id)
    };

    let struct_name = format!("Pgn{}", pgn_id);

    // Implement PgnData
    if !is_poly {
        writeln!(buffer, "impl PgnData for {} {{", struct_name)?;
        buffer.push_str(&default_implementation(pgn, is_poly)?);
        writeln!(buffer, "}}")?;
        writeln!(buffer)?;
    }

    // Implement FieldAccess
    writeln!(buffer, "impl FieldAccess for {} {{", struct_name)?;

    // Determine which fields must be excluded (those in the repeating group)
    let excluded_range = if let Some(info) = repeating_info {
        Some(info.start_field_index..(info.start_field_index + info.size))
    } else {
        None
    };

    // `field` method (read access)
    writeln!(
        buffer,
        "\tfn field(&self, id: &'static str) -> Option<PgnValue> {{"
    )?;
    writeln!(buffer, "\t\tmatch id {{")?;
    for (idx, field) in pgn.fields.iter().enumerate() {
        // Skip fields that belong to the repeating group
        if let Some(ref range) = excluded_range {
            if range.contains(&idx) {
                continue;
            }
        }
        // if PASSIVE_FIELDS.contains(&field.kind.as_str()) {
        //     continue;
        // }
        let field_name_pascal = to_pascal_case(&field.id, PascalCaseMode::Soft);
        let field_name_snake = to_snake_case(&field.id, "field");
        let field_type_str = map_type(field, lookup_enum_map, lookup_indir_map)?;
        let lookup_repr = lookup_repr_from_field(field, lookup_enum_map, lookup_indir_map);

        // Detect whether this field serves as the counter for a repeating group
        let is_counter_field = repeating_info
            .as_ref()
            .and_then(|info| info.count_field_index)
            .map(|counter_idx| counter_idx == idx)
            .unwrap_or(false);

        // For lookups, rely on the enum representation instead of the Rust enum type
        let type_for_variant = if matches!(
            map_to_fieldkind(field),
            FieldKind::Lookup | FieldKind::IndirectLookup
        ) {
            lookup_repr.unwrap_or("u8")
        } else {
            &field_type_str
        };
        let pgn_value_variant = get_pgn_value_variant_from_type(type_for_variant, field)?;

        if is_counter_field {
            // This field is a counter: expose the `_count` backing field instead of the raw field
            let count_field_name = &repeating_info.as_ref().unwrap().count_field_name;
            writeln!(
                buffer,
                "\t\t\t\"{}\" => Some({}(self.{} as {})),",
                field_name_pascal, pgn_value_variant, count_field_name, field_type_str
            )?;
        } else {
            match map_to_fieldkind(field) {
                FieldKind::Lookup => {
                    if let Some(repr) = lookup_repr {
                        let (pgn_variant, cast_type) = match repr {
                            "u16" => ("PgnValue::U16", "u16"),
                            "u32" => ("PgnValue::U32", "u32"),
                            _ => ("PgnValue::U8", "u8"),
                        };
                        writeln!(
                            buffer,
                            "\t\t\t\"{}\" => Some({}({}::from(self.{}))),",
                            field_name_pascal, pgn_variant, cast_type, field_name_snake
                        )?;
                    } else {
                        writeln!(
                            buffer,
                            "\t\t\t\"{}\" => Some(PgnValue::U8(u8::from(self.{}))),",
                            field_name_pascal, field_name_snake
                        )?;
                    }
                }
                // INDIRECT_LOOKUP: keep raw u8 fields without conversion
                FieldKind::IndirectLookup => {
                    writeln!(
                        buffer,
                        "\t\t\t\"{}\" => Some(PgnValue::U8(self.{})),",
                        field_name_pascal, field_name_snake
                    )?;
                }
                FieldKind::StringFix => {
                    writeln!(buffer, "\t\t\t\"{}\" => {{ ", field_name_pascal)?;
                    writeln!(buffer, "\t\t\t\tlet mut bytes = PgnBytes::default();",)?;
                    writeln!(
                        buffer,
                        "\t\t\t\tbytes.len = self.{}.len();",
                        field_name_snake
                    )?;
                    writeln!(
                        buffer,
                        "\t\t\t\tbytes.data[..bytes.len].copy_from_slice(&self.{});",
                        field_name_snake
                    )?;
                    writeln!(buffer, "\t\t\t\tSome(PgnValue::Bytes(bytes))")?;
                    writeln!(buffer, "\t\t\t}}")?;
                }
                FieldKind::Binary => {
                    // BINARY fields can either be byte arrays [u8; N] (BitLength % 8 == 0)
                    // or integer scalars (BitLength % 8 != 0)
                    if field_type_str.starts_with("[") {
                        // Array path: copy bytes into the temporary buffer
                        writeln!(buffer, "\t\t\t\"{}\" => {{ ", field_name_pascal)?;
                        writeln!(buffer, "\t\t\t\tlet mut bytes = PgnBytes::default();",)?;
                        writeln!(
                            buffer,
                            "\t\t\t\tbytes.len = self.{}.len();",
                            field_name_snake
                        )?;
                        writeln!(
                            buffer,
                            "\t\t\t\tbytes.data[..bytes.len].copy_from_slice(&self.{});",
                            field_name_snake
                        )?;
                        writeln!(buffer, "\t\t\t\tSome(PgnValue::Bytes(bytes))")?;
                        writeln!(buffer, "\t\t\t}}")?;
                    } else {
                        // Scalar path: treat it like any other numeric field
                        writeln!(
                            buffer,
                            "\t\t\t\"{}\" => Some({}(self.{})),",
                            field_name_pascal, pgn_value_variant, field_name_snake
                        )?;
                    }
                }
                FieldKind::StringLz | FieldKind::StringLau => {
                    writeln!(
                        buffer,
                        "\t\t\t\"{}\" => Some(PgnValue::Bytes(self.{}.clone())),",
                        field_name_pascal, field_name_snake
                    )?;
                }

                _ => writeln!(
                    buffer,
                    "\t\t\t\"{}\" => Some({}(self.{})),",
                    field_name_pascal, pgn_value_variant, field_name_snake
                )?,
            };
        }
    }
    writeln!(buffer, "\t\t\t_ => None,")?;
    writeln!(buffer, "\t\t}}")?;
    writeln!(buffer, "\t}}")?;
    writeln!(buffer)?;

    // `field_mut` method (write access)
    writeln!(
        buffer,
        "\tfn field_mut(&mut self, id: &'static str, value: PgnValue) -> Option<()> {{"
    )?;
    writeln!(buffer, "\t\tmatch id {{")?;
    for (idx, field) in pgn.fields.iter().enumerate() {
        // Skip fields that belong to the repeating group
        if let Some(ref range) = excluded_range {
            if range.contains(&idx) {
                continue;
            }
        }
        // if PASSIVE_FIELDS.contains(&field.kind.as_str()) {
        //     continue;
        // }
        let field_name_pascal = to_pascal_case(&field.id, PascalCaseMode::Soft);
        let field_name_snake = to_snake_case(&field.id, "field");
        let field_type_str = map_type(field, lookup_enum_map, lookup_indir_map)?;
        let lookup_repr = lookup_repr_from_field(field, lookup_enum_map, lookup_indir_map);

        let is_counter_field = repeating_info
            .as_ref()
            .and_then(|info| info.count_field_index)
            .map(|counter_idx| counter_idx == idx)
            .unwrap_or(false);

        // For lookups, rely on the enum representation instead of the Rust enum type
        let type_for_variant = if matches!(
            map_to_fieldkind(field),
            FieldKind::Lookup | FieldKind::IndirectLookup
        ) {
            lookup_repr.unwrap_or("u8")
        } else {
            &field_type_str
        };
        let pgn_value_variant = get_pgn_value_variant_from_type(type_for_variant, field)?;

        if is_counter_field {
            let count_field_name = &repeating_info.as_ref().unwrap().count_field_name;
            writeln!(buffer, "\t\t\t\"{}\" => {{ ", field_name_pascal)?;
            writeln!(
                buffer,
                "\t\t\t\tif let {}(val) = value {{",
                pgn_value_variant
            )?;
            writeln!(buffer, "\t\t\t\t\tself.{} = val;", field_name_snake)?;
            writeln!(
                buffer,
                "\t\t\t\t\tself.{} = val as usize;",
                count_field_name
            )?;
            writeln!(buffer, "\t\t\t\t\tSome(())")?;
            writeln!(buffer, "\t\t\t\t}} else {{")?;
            writeln!(buffer, "\t\t\t\t\tNone")?;
            writeln!(buffer, "\t\t\t\t}}")?;
            writeln!(buffer, "\t\t\t}}")?;
            continue;
        }

        writeln!(buffer, "\t\t\t\"{}\" => {{ ", field_name_pascal)?;
        match map_to_fieldkind(field) {
            FieldKind::Lookup | FieldKind::BitLookup => {
                if let Some(repr) = lookup_repr {
                    let variant = match repr {
                        "u16" => "PgnValue::U16",
                        "u32" => "PgnValue::U32",
                        _ => "PgnValue::U8",
                    };
                    writeln!(buffer, "\t\t\t\tif let {}(val) = value {{", variant)?;
                    writeln!(
                        buffer,
                        "\t\t\t\t\tmatch {}::try_from(val) {{",
                        field_type_str
                    )?;
                    writeln!(buffer, "\t\t\t\t\t\tOk(enum_val) => {{")?;
                    writeln!(
                        buffer,
                        "\t\t\t\t\t\t\tself.{} = enum_val;",
                        field_name_snake
                    )?;
                    writeln!(buffer, "\t\t\t\t\t\t\tSome(())")?;
                    writeln!(buffer, "\t\t\t\t\t\t}}")?;
                    writeln!(buffer, "\t\t\t\t\t\tErr(_) => None")?;
                    writeln!(buffer, "\t\t\t\t\t}}")?;
                    writeln!(buffer, "\t\t\t\t}} else {{")?;
                    writeln!(buffer, "\t\t\t\t\tNone")?;
                    writeln!(buffer, "\t\t\t\t}}")?;
                } else {
                    writeln!(buffer, "\t\t\t\tif let PgnValue::U8(val) = value {{")?;
                    writeln!(
                        buffer,
                        "\t\t\t\t\tmatch {}::try_from(val) {{",
                        field_type_str
                    )?;
                    writeln!(buffer, "\t\t\t\t\t\tOk(enum_val) => {{")?;
                    writeln!(
                        buffer,
                        "\t\t\t\t\t\t\tself.{} = enum_val;",
                        field_name_snake
                    )?;
                    writeln!(buffer, "\t\t\t\t\t\t\tSome(())")?;
                    writeln!(buffer, "\t\t\t\t\t\t}}")?;
                    writeln!(buffer, "\t\t\t\t\t\tErr(_) => None")?;
                    writeln!(buffer, "\t\t\t\t\t}}")?;
                    writeln!(buffer, "\t\t\t\t}} else {{")?;
                    writeln!(buffer, "\t\t\t\t\tNone")?;
                    writeln!(buffer, "\t\t\t\t}}")?;
                }
            }
            // INDIRECT_LOOKUP: les champs restent u8, pas de conversion
            FieldKind::IndirectLookup => {
                writeln!(buffer, "\t\t\t\tif let PgnValue::U8(val) = value {{")?;
                writeln!(buffer, "\t\t\t\t\tself.{} = val;", field_name_snake)?;
                writeln!(buffer, "\t\t\t\t\tSome(())")?;
                writeln!(buffer, "\t\t\t\t}} else {{")?;
                writeln!(buffer, "\t\t\t\t\tNone")?;
                writeln!(buffer, "\t\t\t\t}}")?;
            }
            FieldKind::StringFix => {
                writeln!(buffer, "\t\t\t\tif let PgnValue::Bytes(val) = value {{")?;
                writeln!(
                    buffer,
                    "\t\t\t\t\tlet len = val.len.min(self.{}.len());",
                    field_name_snake
                )?;
                writeln!(
                    buffer,
                    "\t\t\t\t\tself.{}[..len].copy_from_slice(&val.data[..len]);",
                    field_name_snake
                )?;
                writeln!(buffer, "\t\t\t\t\tSome(())")?;
                writeln!(buffer, "\t\t\t\t}} else {{\n\t\t\t\t\tNone\n\t\t\t\t}}")?;
            }
            FieldKind::Binary => {
                // BINARY fields can either be byte arrays [u8; N] (BitLength % 8 == 0)
                // or integer scalars (BitLength % 8 != 0)
                if field_type_str.starts_with("[") {
                    // Array path: copy bytes from the payload
                    writeln!(buffer, "\t\t\t\tif let PgnValue::Bytes(val) = value {{")?;
                    writeln!(
                        buffer,
                        "\t\t\t\t\tlet len = val.len.min(self.{}.len());",
                        field_name_snake
                    )?;
                    writeln!(
                        buffer,
                        "\t\t\t\t\tself.{}[..len].copy_from_slice(&val.data[..len]);",
                        field_name_snake
                    )?;
                    writeln!(buffer, "\t\t\t\t\tSome(())")?;
                    writeln!(buffer, "\t\t\t\t}} else {{\n\t\t\t\t\tNone\n\t\t\t\t}}")?;
                } else {
                    // Scalar path: treat it like any other numeric field
                    writeln!(
                        buffer,
                        "\t\t\t\tif let {}(val) = value {{",
                        pgn_value_variant
                    )?;
                    writeln!(buffer, "\t\t\t\t\tself.{} = val;", field_name_snake)?;
                    writeln!(buffer, "\t\t\t\t\tSome(())")?;
                    writeln!(buffer, "\t\t\t\t}} else {{\n\t\t\t\t\tNone\n\t\t\t\t}}")?;
                }
            }
            FieldKind::StringLz | FieldKind::StringLau => {
                writeln!(buffer, "\t\t\t\tif let PgnValue::Bytes(val) = value {{")?;
                writeln!(buffer, "\t\t\t\t\tself.{} = val;", field_name_snake)?;
                writeln!(buffer, "\t\t\t\t\tSome(())")?;
                writeln!(buffer, "\t\t\t\t}} else {{")?;
                writeln!(buffer, "\t\t\t\t\tNone")?;
                writeln!(buffer, "\t\t\t\t}}")?;
            }
            _ => {
                writeln!(
                    buffer,
                    "\t\t\t\tif let {}(val) = value {{",
                    pgn_value_variant
                )?;
                writeln!(buffer, "\t\t\t\t\tself.{} = val;", field_name_snake)?;
                writeln!(buffer, "\t\t\t\t\tSome(())")?;
                writeln!(buffer, "\t\t\t\t}} else {{\n\t\t\t\t\tNone\n\t\t\t\t}}")?;
            }
        }
        writeln!(buffer, "\t\t\t}}")?;
    }
    writeln!(buffer, "\t\t\t_ => None,")?;
    writeln!(buffer, "\t\t}}")?;
    writeln!(buffer, "\t}}")?;

    // Generate trait methods for repeating fields when present
    if let Some(info) = repeating_info {
        buffer.push_str(&generate_repetitive_field_access(
            pgn,
            info,
            lookup_enum_map,
            lookup_indir_map,
        )?);
    }

    writeln!(buffer, "}}")?;
    writeln!(buffer)?;

    Ok(buffer)
}

//==================================================================================TRAIT_IMPL_HELPER
/// Generate default `PgnData` implementations (non-polymorphic struct).
fn default_implementation(pgn: &PgnInstructions, is_poly: bool) -> Result<String, BuildError> {
    let mut buffer = String::new();
    let description_name = if is_poly {
        format!(
            "{}_{}",
            pgn.pgn_id,
            to_snake_case(&pgn.pgn_name, "POLY").to_uppercase()
        )
    } else {
        format!("{}", pgn.pgn_id)
    };

    writeln!(
        buffer,
        "\tfn from_payload(payload: &[u8]) -> Result<Self, DeserializationError> {{"
    )?;
    writeln!(buffer, "\t\tlet mut instance = Self::new();")?;
    writeln!(
        buffer,
        "\t\tcrate::infra::codec::engine::deserialize_into(&mut instance,payload, &Self::PGN_{}_DESCRIPTOR)?;",
        description_name
    )?
    ;
    writeln!(buffer, "\t\tOk(instance)")?;
    writeln!(buffer, "\t}}")?;

    writeln!(buffer)?;

    writeln!(buffer, "\tfn to_payload(&self, buffer: &mut [u8]) -> Result<usize, crate::error::SerializationError> {{")?;
    writeln!(
        buffer,
        "\t\tcrate::infra::codec::engine::serialize(self, buffer, &Self::PGN_{}_DESCRIPTOR)",
        description_name
    )?;
    writeln!(buffer, "\t}}")?;

    Ok(buffer)
}

/// Emit the `new()` function associated with the generated struct.
fn generate_new_fn(
    pgn: &PgnInstructions,
    repeating_info: Option<&RepeatingFieldSetInfo>,
    lookup_enum_map: &HashMap<String, LookupEnum>,
    lookup_indir_map: &HashMap<String, LookupIndirEnum>,
) -> Result<String, BuildError> {
    let mut buffer = String::new();

    writeln!(
        buffer,
        "\t/// Create a new instance with protocol-compliant defaults."
    )?;

    // When repeating fields exist we cannot expose a `const fn`
    // car Default::default() n'est pas const
    if repeating_info.is_some() {
        writeln!(buffer, "\tpub fn new() -> Self {{")?;
    } else {
        writeln!(buffer, "\tpub const fn new() -> Self {{")?;
    }

    writeln!(buffer, "\t\tSelf {{")?;

    // Determine which fields must be excluded (those in the repeating group)
    let excluded_range = if let Some(info) = repeating_info {
        Some(info.start_field_index..(info.start_field_index + info.size))
    } else {
        None
    };

    for (idx, field) in pgn.fields.iter().enumerate() {
        // Skip fields that belong to the repeating group
        if let Some(ref range) = excluded_range {
            if range.contains(&idx) {
                continue;
            }
        }
        let field_name = to_snake_case(&field.id, "field");
        let field_kind = map_to_fieldkind(field);
        let field_type = map_type(field, lookup_enum_map, lookup_indir_map)?;

        let value = match field_kind {
            // SPARE fields default to 0
            FieldKind::Spare => "0".to_string(),

            // RESERVED fields default to all bits set to 1
            FieldKind::Reserved => {
                let bits = field.bits_length.unwrap_or(0);
                if bits > 0 && bits <= 64 {
                    let val = u64::MAX >> (64 - bits);
                    format!("{}u64 as {}", val, field_type)
                } else {
                    "0".to_string() // Safe fallback
                }
            }

            // All other public fields default to zero.
            FieldKind::Lookup => {
                format!("{}::DEFAULT", field_type)
            }

            // INDIRECT_LOOKUP fields are stored as u8 values, initialized to zero.
            FieldKind::IndirectLookup => "0".to_string(),

            _ => match field_type.as_str() {
                "f32" | "f64" => "0.0".to_string(),
                "PgnBytes" => "PgnBytes::new()".to_string(),
                slice if slice.starts_with("[") => {
                    // Array fields (e.g. [u8; N])
                    let size = slice.split(&['[', ';', ']'][..]).nth(2).unwrap_or("0");
                    format!("[0; {}]", size)
                }

                _ => "0".to_string(),
            },
        };
        writeln!(buffer, "\t\t\t{}: {},", field_name, value)?;
    }

    // Initialize repeating-field storage when available
    if let Some(info) = repeating_info {
        // Initialize repeating-structure array
        writeln!(
            buffer,
            "\t\t\t{}: [{}::default(); {}],",
            info.array_field_name, info.struct_name, info.max_repetitions
        )?;
        // Counter starts at 0
        writeln!(buffer, "\t\t\t{}: 0,", info.count_field_name)?;
    }

    writeln!(buffer, "\t\t}}")?;
    writeln!(buffer, "\t}}")?;
    Ok(buffer)
}

//==================================================================================SET_PGNS_SET
/// Build the set of PGNs present in the CANboat database.
fn set_pgns_set(canboat_value: &Value) -> Result<HashSet<u32>, BuildError> {
    let mut pgns_set: HashSet<u32> = HashSet::new();
    if let Some(pgn_array) = canboat_value["PGNs"].as_array() {
        for pgn_value in pgn_array {
            match serde_json::from_value::<PgnInstructions>(pgn_value.clone()) {
                Ok(pgn) => {
                    pgns_set.insert(pgn.pgn_id);
                }
                Err(e) => return Err(BuildError::ParseJson(e)),
            }
        }
    }
    Ok(pgns_set)
}

//==================================================================================SET_POLY_PGNS_MAP
/// Build the PGN → polymorphic variants mapping based on the lookup tables.
fn set_poly_pgns_map(
    canboat_value: &Value,
    pgns_set: HashSet<u32>,
) -> Result<HashMap<u32, Vec<PolyPgn>>, BuildError> {
    let mut poly_pgns_map: HashMap<u32, Vec<PolyPgn>> = HashMap::new();
    if let Some(pgn_array) = canboat_value["PGNs"].as_array() {
        for pgn_value in pgn_array {
            match serde_json::from_value::<PgnInstructions>(pgn_value.clone()) {
                Ok(pgn_main_def) => {
                    if pgns_set.contains(&pgn_main_def.pgn_id) {
                        let poly_pgn_formated_name =
                            to_pascal_case(&pgn_main_def.pgn_name, PascalCaseMode::Soft);

                        pgn_main_def
                            .fields
                            .iter()
                            .filter(|e| e.order == 1)
                            .for_each(|e| {
                                if let Some(desc) = &e.description {
                                    if let Some(enum_direct_name) = &e.enum_direct_name {
                                        let poly_pgn = PolyPgn {
                                            lookup_id: enum_direct_name.clone(),
                                            name: poly_pgn_formated_name.clone(),
                                            desc: desc.clone(),
                                        };
                                        poly_pgns_map
                                            .entry(pgn_main_def.pgn_id)
                                            .or_default()
                                            .push(poly_pgn);
                                    }
                                }
                            });
                    }
                }
                Err(e) => return Err(BuildError::ParseJson(e)),
            }
        }
    }
    Ok(poly_pgns_map)
}
