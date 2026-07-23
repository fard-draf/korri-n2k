//! Generate lookup enumeration tables from CANboat JSON data.
#![allow(clippy::all)]
use super::domain::*;
use super::errors::*;
use super::name_helpers::*;
use super::type_helpers::*;

use serde::de::DeserializeOwned;
use serde_json::Value;
use std::collections::HashMap;
use std::fmt::{Debug, Write};

/// Iterate over lookup categories and emit the corresponding Rust code.
pub(crate) fn run_lookup_gen(canboat_value: &Value) -> Result<String, BuildError> {
    let mut buffer_lookup_code = String::new();
    writeln!(&mut buffer_lookup_code, "#[allow(clippy::all)]")?;

    process_lookup_category::<LookupEnum>(
        canboat_value,
        "LookupEnumerations",
        &mut buffer_lookup_code,
    )?;
    process_lookup_category::<LookupIndirEnum>(
        canboat_value,
        "LookupIndirectEnumerations",
        &mut buffer_lookup_code,
    )?;
    process_lookup_category::<LookupBitEnum>(
        canboat_value,
        "LookupBitEnumerations",
        &mut buffer_lookup_code,
    )?;
    process_lookup_category::<LookupFieldTypeEnum>(
        canboat_value,
        "LookupFieldTypeEnumerations",
        &mut buffer_lookup_code,
    )?;

    Ok(buffer_lookup_code)
}

/// Process a CANboat lookup category and append the generated code.
fn process_lookup_category<T>(
    canboat_value: &serde_json::Value,
    category_key: &str,
    output_buffer: &mut String,
) -> Result<(), BuildError>
where
    T: DeserializeOwned + LookupGenerator + Debug,
{
    if let Some(array) = canboat_value[category_key].as_array() {
        for value in array {
            match serde_json::from_value::<T>(value.clone()) {
                Ok(lookup_def) => match generate_lookup_code(&lookup_def) {
                    Ok(code) => {
                        output_buffer.push_str(&code);
                    }
                    Err(e) => {
                        println!(
                            "cargo:warning=Failed to generate Rust code for {}: '{}' : {}",
                            category_key,
                            lookup_def.name(),
                            e
                        )
                    }
                },
                Err(e) => {
                    let name = value.get("Name").unwrap_or(&serde_json::Value::Null);
                    println!(
                        "cargo:warning=[LOOKUP: {}] [NAME: {}] Skipped.. Malformed entry: {}",
                        category_key, name, e
                    );
                }
            }
        }
    } else {
        println!(
            "cargo:warning=JSON category '[{}]' not found or not an array.",
            category_key
        );
    }
    Ok(())
}
//==================================================================================LOOKUP_ENUM_GENERATION
/// Generate the full Rust code for a lookup enumeration (type plus helpers).
fn generate_lookup_code(lookup: &dyn LookupGenerator) -> Result<String, BuildError> {
    // TODO!: Break the function into smaller pieces.
    let mut buffer = String::new();
    let enum_name = to_pascal_case(&lookup.name().to_lowercase(), PascalCaseMode::Hard);
    let mut enum_repr = generate_repr_attribute(lookup.max_value());
    let metadata_struct_name = format!("{}Metadata", enum_name);
    let mut hash_count = HashMap::new();
    let variants = lookup.variants();

    // Count variant names to handle duplicates
    for variant_data in &variants {
        match variant_data.clone() {
            VariantData::Simple { name, value: _ } => {
                *hash_count.entry(name).or_insert(0) += 1;
            }
            VariantData::Full(VariantMetaData {
                name,
                value: _,
                field_type: _,
                resolution: _,
                unit: _,
                bits: _,
                lookup_bit_enum: _,
            }) => {
                *hash_count.entry(name).or_insert(0) += 1;
            }
        }
    }

    // LookupIndirectEnum encodes two u8 values combined into a u16 → force a wider repr.
    if lookup.metadata_code() == 1 {
        enum_repr = "u16";
    }

    // Generate LookupFieldType metadata helpers
    if lookup.metadata_code() == 2 {
        //======================Metadata struct generation
        writeln!(buffer, "#[derive(Debug, PartialEq, Clone, Copy)]")?;
        writeln!(buffer, "pub struct {} {{", metadata_struct_name)?;
        writeln!(buffer, "\tpub field_type: &'static str,")?;
        writeln!(buffer, "\tpub resolution: Option<f32>,")?;
        writeln!(buffer, "\tpub unit: Option<&'static str>,")?;
        writeln!(buffer, "\tpub bits: &'static str,")?;
        writeln!(buffer, "\tpub lookup_bit_enum: Option<&'static str>,")?;
        writeln!(buffer, "}}")?;
        writeln!(buffer)?;
    }

    // Resolve a variant to its emitted name and value, applying the duplicate rule.
    let variant_ident = |v: &VariantData| -> (String, u32) {
        let (name, value) = match v {
            VariantData::Simple { name, value } => (name, *value),
            VariantData::Full(VariantMetaData { name, value, .. }) => (name, *value),
        };
        let dup = hash_count.get(name) > Some(&1)
            || (matches!(v, VariantData::Simple { .. }) && hash_count.contains_key("Error"));
        let ident = if dup {
            format!("{}{}", name, value)
        } else {
            name.clone()
        };
        (ident, value)
    };

    // A BITLOOKUP enumeration names bit *positions*, not field values: it is only
    // ever cast to an integer by the generated bit helpers, never parsed from a
    // payload. It keeps explicit discriminants and takes no catch-all variant.
    let is_bitfield = lookup.is_bitfield();

    //======================Enum generation
    writeln!(buffer, "#[repr({})]", enum_repr)?;
    writeln!(buffer, "#[derive(Debug, PartialEq, Copy, Clone)]")?;
    writeln!(buffer, "pub enum {} {{", enum_name)?;

    let mut first_variant_name: Option<String> = None;

    for variant_data in &variants {
        let (field_name, value) = variant_ident(variant_data);
        if first_variant_name.is_none() {
            first_variant_name = Some(field_name.clone());
        }
        if is_bitfield {
            writeln!(buffer, "\t{} = {},", field_name, value)?;
        } else {
            // Discriminants stay implicit: Rust forbids mixing them with a data variant.
            writeln!(buffer, "\t{},", field_name)?;
        }
    }
    if !is_bitfield {
        // CANboat enumerates only the values it names. A field may legitimately
        // carry any other bit pattern — TIME_STAMP names 60..=63 while 0..=59 are
        // seconds — so keep the raw value instead of rejecting the whole message.
        writeln!(buffer, "\tUnrecognized({}),", enum_repr)?;
    }
    writeln!(buffer, "}}")?;
    writeln!(buffer)?;

    if !is_bitfield {
        // Inherent and `const` so it stays callable from the const helpers below,
        // which `From::from` is not.
        writeln!(buffer, "impl {} {{", enum_name)?;
        writeln!(
            buffer,
            "\t/// The value carried on the wire, including an unnamed one."
        )?;
        writeln!(buffer, "\tpub const fn raw(self) -> {} {{", enum_repr)?;
        writeln!(buffer, "\t\tmatch self {{")?;
        for variant_data in &variants {
            let (field_name, value) = variant_ident(variant_data);
            writeln!(buffer, "\t\t\tSelf::{} => {},", field_name, value)?;
        }
        writeln!(buffer, "\t\t\tSelf::Unrecognized(raw) => raw,")?;
        writeln!(buffer, "\t\t}}")?;
        writeln!(buffer, "\t}}")?;
        writeln!(buffer, "}}")?;
        writeln!(buffer)?;
    }

    writeln!(buffer, "impl From<{}> for {} {{", enum_name, enum_repr)?;
    writeln!(buffer, "\tfn from(status: {}) -> Self {{", enum_name)?;
    if is_bitfield {
        writeln!(buffer, "\t\tstatus as {}", enum_repr)?;
    } else {
        writeln!(buffer, "\t\tstatus.raw()")?;
    }
    writeln!(buffer, "\t}}")?;
    writeln!(buffer, "}}")?;
    writeln!(buffer)?;

    if is_bitfield {
        writeln!(buffer, "#[derive (Debug, PartialEq)]")?;
        writeln!(buffer, "pub struct Invalid{}({});", enum_name, enum_repr)?;
        writeln!(buffer)?;
        writeln!(buffer, "impl TryFrom<{}> for {} {{", enum_repr, enum_name)?;
        writeln!(buffer, "\ttype Error = Invalid{};", enum_name)?;
        writeln!(
            buffer,
            "\tfn try_from(value: {}) -> Result<Self, Self::Error> {{",
            enum_repr
        )?;
        writeln!(buffer, "\t\tmatch value {{")?;
        for variant_data in &variants {
            let (field_name, value) = variant_ident(variant_data);
            writeln!(
                buffer,
                "\t\t\t{} => Ok({}::{}),",
                value, enum_name, field_name
            )?;
        }
        writeln!(buffer, "\t\t\tother => Err(Invalid{}(other)),", enum_name)?;
        writeln!(buffer, "\t\t}}")?;
        writeln!(buffer, "\t}}")?;
        writeln!(buffer, "}}")?;
    } else {
        // `From` gives `TryFrom` for free through the blanket impl in core, with
        // `Error = Infallible`. Existing `try_from` call sites keep compiling.
        writeln!(buffer, "impl From<{}> for {} {{", enum_repr, enum_name)?;
        writeln!(buffer, "\tfn from(value: {}) -> Self {{", enum_repr)?;
        writeln!(buffer, "\t\tmatch value {{")?;
        for variant_data in &variants {
            let (field_name, value) = variant_ident(variant_data);
            writeln!(buffer, "\t\t\t{} => {}::{},", value, enum_name, field_name)?;
        }
        writeln!(buffer, "\t\t\tother => {}::Unrecognized(other),", enum_name)?;
        writeln!(buffer, "\t\t}}")?;
        writeln!(buffer, "\t}}")?;
        writeln!(buffer, "}}")?;
    }
    writeln!(buffer)?;
    if let Some(default_variant) = first_variant_name {
        // Default implementation
        writeln!(buffer, "impl Default for {} {{", enum_name)?;
        writeln!(buffer, "\tfn default() -> Self {{")?;
        writeln!(buffer, "\t\tSelf::{}", default_variant)?;
        writeln!(buffer, "\t}}")?;
        writeln!(buffer, "}}")?;
        writeln!(buffer)?;

        // DEFAULT constant for backward compatibility
        writeln!(buffer, "impl {} {{", enum_name)?;
        writeln!(
            buffer,
            "\tpub const DEFAULT: Self = Self::{};",
            default_variant
        )?;
        writeln!(buffer, "}}")?;
        writeln!(buffer)?;
    }

    if lookup.metadata_code() == 1 {
        writeln!(buffer, "impl {} {{", enum_name)?;
        writeln!(buffer, "\tpub const fn value1(&self) -> u8 {{")?;
        writeln!(buffer, "\t\t(self.raw() >> 8) as u8")?;
        writeln!(buffer, "\t}}")?;
        writeln!(buffer)?;
        writeln!(buffer, "\tpub const fn value2(&self) -> u8 {{")?;
        writeln!(buffer, "\t\t(self.raw() & 0x00FF) as u8")?;
        writeln!(buffer, "\t}}")?;
        writeln!(buffer)?;
        writeln!(
            buffer,
            "\tpub fn from_values(v1: u8, v2: u8) -> Option<Self> {{"
        )?;
        writeln!(buffer, "\t\tlet combined = (v1 as u16) << 8 | (v2 as u16);")?;
        writeln!(buffer, "\t\tSelf::try_from(combined).ok()")?;
        writeln!(buffer, "\t}}")?;
        writeln!(buffer, "}}")?;
        writeln!(buffer)?;
    }
    Ok(buffer)
}

//==================================================================================INDIRECT_LOOKUP_HELPER
/// Generate `get_/set_` helpers for fields that rely on indirect lookups.
pub(super) fn generate_indirect_lookup_helpers(
    master_field_id: &str,
    slave_field_id: &str,
    enum_name: &str,
    master_field_type: &str,
) -> Result<String, BuildError> {
    let mut buffer = String::new();
    let master_field_snake = to_snake_case(master_field_id, "field");
    let slave_field_snake = to_snake_case(slave_field_id, "field");
    let enum_name_pascal = to_pascal_case(&enum_name.to_lowercase(), PascalCaseMode::Hard);

    // Getter et setter names
    let getter_name = format!("get_{}", &slave_field_snake);
    let setter_name = format!("set_{}", &slave_field_snake);

    writeln!(
        buffer,
        "\tpub fn {}(&self) -> Option<{}> {{",
        getter_name, enum_name_pascal
    )?;
    writeln!(buffer, "\t\tlet slave_val = self.{};", slave_field_snake)?;
    writeln!(
        buffer,
        "\t\tlet master_val = u8::from(self.{});",
        master_field_snake
    )?;
    writeln!(
        buffer,
        "\t\tlet combined_value = (master_val as u16) << 8 | (slave_val as u16);"
    )?;
    writeln!(
        buffer,
        "\t\t{}::try_from(combined_value).ok()",
        enum_name_pascal
    )?;

    writeln!(buffer, "\t}}")?;
    writeln!(
        buffer,
        "\tpub fn {}(&mut self, value: {}) {{",
        setter_name, enum_name_pascal
    )?;
    // The master field is a Lookup enum; the conversion keeps unnamed values.
    writeln!(buffer, "\t\tlet val1 = value.value1();")?;
    writeln!(buffer, "\t\tlet val2 = value.value2();")?;
    writeln!(
        buffer,
        "\t\tself.{} = {}::from(val1);",
        master_field_snake, master_field_type
    )?;
    writeln!(buffer, "\t\tself.{} = val2;", slave_field_snake)?;

    writeln!(buffer, "\t}}")?;
    writeln!(buffer)?;
    Ok(buffer)
}

/// Builds a map of direct lookups accessible by raw or PascalCase name.
pub(super) fn set_lookup_enum_map(
    canboat_value: &Value,
) -> Result<HashMap<String, LookupEnum>, BuildError> {
    let mut lookup_map = HashMap::new();
    if let Some(lookup_def) = canboat_value["LookupEnumerations"].as_array() {
        for lookup in lookup_def {
            match serde_json::from_value::<LookupEnum>(lookup.clone()) {
                Ok(lookup_def) => {
                    let pascal_case =
                        to_pascal_case(&lookup_def.name.to_lowercase(), PascalCaseMode::Hard);
                    lookup_map
                        .entry(lookup_def.name.clone())
                        .or_insert(lookup_def.clone());
                    lookup_map.entry(pascal_case).or_insert(lookup_def.clone());
                }
                Err(e) => {
                    println!("cargo:warning=[LOOKUP_ENUM {}] Skipped..", e)
                }
            }
        }
    }

    Ok(lookup_map)
}

/// Builds a map of indirect lookups accessible by raw or PascalCase name.
pub(super) fn set_lookup_indir_map(
    canboat_value: &Value,
) -> Result<HashMap<String, LookupIndirEnum>, BuildError> {
    let mut lookup_map = HashMap::new();
    if let Some(lookup_def) = canboat_value["LookupIndirectEnumerations"].as_array() {
        for lookup in lookup_def {
            match serde_json::from_value::<LookupIndirEnum>(lookup.clone()) {
                Ok(lookup_def) => {
                    let pascal_case =
                        to_pascal_case(&lookup_def.name.to_lowercase(), PascalCaseMode::Hard);
                    lookup_map
                        .entry(lookup_def.name.clone())
                        .or_insert(lookup_def.clone());
                    lookup_map.entry(pascal_case).or_insert(lookup_def.clone());
                }
                Err(e) => {
                    println!("cargo:warning=[LOOKUP_INDIRECT_ENUM {}] Skipped..", e)
                }
            }
        }
    }
    Ok(lookup_map)
}

/// Builds a map of bit lookups accessible by raw or PascalCase name.
pub(super) fn set_lookup_bit_map(
    canboat_value: &Value,
) -> Result<HashMap<String, LookupBitEnum>, BuildError> {
    let mut lookup_map = HashMap::new();
    if let Some(lookup_def) = canboat_value["LookupBitEnumerations"].as_array() {
        for lookup in lookup_def {
            match serde_json::from_value::<LookupBitEnum>(lookup.clone()) {
                Ok(lookup_def) => {
                    let pascal_case =
                        to_pascal_case(&lookup_def.name.to_lowercase(), PascalCaseMode::Hard);
                    lookup_map
                        .entry(lookup_def.name.clone())
                        .or_insert(lookup_def.clone());
                    lookup_map.entry(pascal_case).or_insert(lookup_def.clone());
                }
                Err(e) => {
                    println!("cargo:warning=[LOOKUP_BIT_ENUM {}] Skipped..", e)
                }
            }
        }
    }
    Ok(lookup_map)
}
