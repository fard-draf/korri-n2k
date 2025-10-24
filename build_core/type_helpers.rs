//! Typing helpers used while generating PGN structures.
use crate::build_core::{
    domain::{Fields, LookupEnum, LookupIndirEnum},
    errors::BuildError,
    name_helpers::{to_pascal_case, PascalCaseMode},
};
use crate::core::FieldKind;
use std::collections::HashMap;

/// Determine the `repr` integer type for an enumeration based on its max value.
pub(crate) fn generate_repr_attribute(max_value: u32) -> &'static str {
    match max_value {
        0..=255 => "u8",
        256..=65_535 => "u16",
        65_536..=4_294_967_295 => "u32",
    }
}
/// Determine whether a field is signed.
pub(crate) fn is_signed_type(field: &Fields) -> Result<bool, BuildError> {
    Ok(matches!(field.signed, Some(true)))
}

/// Map a Rust type string (e.g. "i16") to the appropriate `PgnValue` variant.
pub(crate) fn get_pgn_value_variant_from_type(
    type_str: &str,
    field: &Fields,
) -> Result<String, BuildError> {
    match map_to_fieldkind(field) {
        FieldKind::StringFix | FieldKind::StringLz | FieldKind::StringLau => {
            Ok("PgnValue::Bytes".to_string())
        }
        FieldKind::Binary => {
            // BINARY fields may be fixed-size byte arrays or integers
            if type_str.starts_with("[") {
                Ok("PgnValue::Bytes".to_string())
            } else {
                // Integer case: pick the matching variant
                let variant = match type_str {
                    "u64" => "PgnValue::U64",
                    "u32" => "PgnValue::U32",
                    "u16" => "PgnValue::U16",
                    "u8" => "PgnValue::U8",
                    _ => {
                        return Err(BuildError::BitLengthErr {
                            path: type_str.to_string(),
                            comment: "Unsupported BINARY type for PgnValue (expected u8/u16/u32/u64 or [u8; N])",
                        })
                    }
                };
                Ok(variant.to_string())
            }
        }

        _ => {
            let variant = match type_str {
                "u64" => "PgnValue::U64",
                "u32" => "PgnValue::U32",
                "u16" => "PgnValue::U16",
                "u8" => "PgnValue::U8",
                "i64" => "PgnValue::I64",
                "i32" => "PgnValue::I32",
                "i16" => "PgnValue::I16",
                "i8" => "PgnValue::I8",
                "f32" => "PgnValue::F32",
                "f64" => "PgnValue::F64",
                // Additional types such as PgnBytes can be supported if needed
                _ => {
                    return Err(BuildError::BitLengthErr {
                        path: type_str.to_string(),
                        comment: "Unsupported type for PgnValue",
                    })
                }
            };

            Ok(variant.to_string())
        }
    }
}

/// Map a CANboat field to the appropriate Rust type in the generated `struct`.
// WARNING: tightly coupled with the `deserialize` function in engine.rs.
// Keep both implementations synchronized.
pub(crate) fn map_type(
    field: &Fields,
    lookup_enum_map: &HashMap<String, LookupEnum>,
    _lookup_indir_map: &HashMap<String, LookupIndirEnum>,
) -> Result<String, BuildError> {
    // Reserved or spare fields do not have a representation in the struct.
    // if PASSIVE_FIELDS.contains(&field.kind.as_str()) {
    //     return Ok("()".to_string());
    // }

    let field_kind = map_to_fieldkind(field);

    match field_kind {
        FieldKind::Date => Ok("u16".to_string()),
        FieldKind::Mmsi => Ok("u32".to_string()),

        // Time fields may carry a resolution (e.g. 0.0001)
        FieldKind::Time => {
            if field.resolution.is_some_and(|r| r as u8 != 1) {
                Ok("f64".to_string()) // With resolution → floating point
            } else {
                Ok("u32".to_string()) // Without resolution → u32
            }
        }
        FieldKind::Duration => {
            let bits = field.bits_length.ok_or(BuildError::BitLengthErr {
                path: field.id.clone(),
                comment: "Duration without BitLength",
            })?;
            Ok(match bits {
                1..=32 if field.resolution.is_some_and(|res| res as u8 != 1) => "f32".to_string(),
                1..=16 => "u16".to_string(),
                17..=32 => "u32".to_string(),
                _ => {
                    return Err(BuildError::BitLengthErr {
                        path: field.id.clone(),
                        comment: "Duration BitLength invalid",
                    })
                }
            })
        }
        FieldKind::StringFix => {
            let num_bits = field.bits_length.ok_or(BuildError::BitLengthErr {
                path: field.id.clone(),
                comment: "build.rs / map_type",
            })?;
            let num_bytes = num_bits / 8;
            Ok(format!("[u8;{}]", num_bytes))
        }
        FieldKind::Binary => {
            let num_bits = field.bits_length.ok_or(BuildError::BitLengthErr {
                path: field.id.clone(),
                comment: "Binary field without BitLength",
            })?;

            // If the BitLength is a multiple of eight, emit a byte array
            if num_bits % 8 == 0 {
                let num_bytes = num_bits / 8;
                Ok(format!("[u8;{}]", num_bytes))
            } else {
                // Otherwise select an appropriate unsigned integer (like NUMBER)
                // Required for AIS fields with non-aligned sizes (e.g. 19 bits)
                match num_bits {
                    1..=8 => Ok("u8".to_string()),
                    9..=16 => Ok("u16".to_string()),
                    17..=32 => Ok("u32".to_string()),
                    33..=64 => Ok("u64".to_string()),
                    _ => Err(BuildError::BitLengthErr {
                        path: field.id.clone(),
                        comment: "Binary field BitLength exceeds 64 bits",
                    }),
                }
            }
        }
        FieldKind::Lookup => {
            if let Some(enum_name) = &field.enum_direct_name {
                let pascal_name = to_pascal_case(&enum_name.to_lowercase(), PascalCaseMode::Hard);
                if lookup_enum_map.contains_key(enum_name)
                    || lookup_enum_map.contains_key(&pascal_name)
                {
                    return Ok(pascal_name);
                }
            }
            // Fallback: keep the historical u8 behavior
            Ok("u8".to_string())
        }
        FieldKind::BitLookup => {
            // BITLOOKUP fields are bitmasks, not exclusive enums
            // Keep unsigned integers because multiple bits may be set
            let bits = field.bits_length.unwrap_or(16);
            Ok(match bits {
                1..=8 => "u8".to_string(),
                9..=16 => "u16".to_string(),
                17..=32 => "u32".to_string(),
                _ => "u64".to_string(),
            })
        }
        FieldKind::IndirectLookup => {
            // IMPORTANT: keep INDIRECT_LOOKUP fields as u8 in the struct because they
            // carry only 8 bits of the combined value. Another u8 will be joined to form
            // the u16 enumeration. Helper accessors are generated to work with the full enum.
            Ok("u8".to_string())
        }
        FieldKind::StringLz | FieldKind::StringLau => Ok("PgnBytes".to_string()),
        _ => {
            // Fields with a resolution become floating-point values.
            if field.resolution.is_some_and(|r| r != 1.0) || field.kind.contains("DECIMAL") {
                match field.bits_length.ok_or(BuildError::BitLengthErr {
                    path: field.id.clone(),
                    comment: "build.rs / map_type",
                })? {
                    1..=32 => return Ok("f32".to_string()),
                    _ => return Ok("f64".to_string()),
                }
                // return Ok("f64".to_string());
            }
            // Otherwise rely on bit length and signedness.
            let is_signed = is_signed_type(field)?;
            match field.bits_length.ok_or(BuildError::BitLengthErr {
                path: field.id.clone(),
                comment: "build.rs / map_type",
            })? {
                1..=8 => {
                    if is_signed {
                        Ok("i8".to_string())
                    } else {
                        Ok("u8".to_string())
                    }
                }
                9..=16 => {
                    if is_signed {
                        Ok("i16".to_string())
                    } else {
                        Ok("u16".to_string())
                    }
                }
                17..=32 => {
                    if is_signed {
                        Ok("i32".to_string())
                    } else {
                        Ok("u32".to_string())
                    }
                }
                33..=64 => {
                    if is_signed {
                        Ok("i64".to_string())
                    } else {
                        Ok("u64".to_string())
                    }
                }
                _ => Ok("()".to_string()),
            }
        }
    }
    // Variable-length fields must be handled with arrays
    // if map_to_fieldkind(field) == FieldKind::StringFix {
    //     let num_bits = field.bits_length.ok_or(BuildError::BitLengthErr {
    //         path: field.id.clone(),
    //         comment: "build.rs / map_type",
    //     })?;
    //     let num_bytes = num_bits / 8;
    //     return Ok(format!("[u8;{}]", num_bytes));
    // }

    // Fields with a resolution become floating-point values.
    // if field.resolution.is_some_and(|r| r != 1.0) || field.kind.contains("DECIMAL") {
    //     match field.bits_length.ok_or(BuildError::BitLengthErr {
    //         path: field.id.clone(),
    //         comment: "build.rs / map_type",
    //     })? {
    //         1..=32 => return Ok("f32".to_string()),
    //         _ => return Ok("f64".to_string()),
    //     }
    //     // return Ok("f64".to_string());
    // }
    // For other cases rely on bit length and signedness.
    // let is_signed = is_signed_type(field)?;
    // match field.bits_length.ok_or(BuildError::BitLengthErr {
    //     path: field.id.clone(),
    //     comment: "build.rs / map_type",
    // })? {
    //     1..=8 => {
    //         if is_signed {
    //             Ok("i8".to_string())
    //         } else {
    //             Ok("u8".to_string())
    //         }
    //     }
    //     9..=16 => {
    //         if is_signed {
    //             Ok("i16".to_string())
    //         } else {
    //             Ok("u16".to_string())
    //         }
    //     }
    //     17..=32 => {
    //         if is_signed {
    //             Ok("i32".to_string())
    //         } else {
    //             Ok("u32".to_string())
    //         }
    //     }
    //     33..=64 => {
    //         if is_signed {
    //             Ok("i64".to_string())
    //         } else {
    //             Ok("u64".to_string())
    //         }
    //     }
    //     _ => Ok("()".to_string()),
    // }
}

/// Return the native `repr` type associated with a Lookup field for conversion helpers.
pub(crate) fn lookup_repr_from_field(
    field: &Fields,
    lookup_enum_map: &HashMap<String, LookupEnum>,
    lookup_indir_map: &HashMap<String, LookupIndirEnum>,
) -> Option<&'static str> {
    match map_to_fieldkind(field) {
        FieldKind::Lookup => field
            .enum_direct_name
            .as_ref()
            .and_then(|name| {
                lookup_enum_map.get(name).or_else(|| {
                    let pascal = to_pascal_case(&name.to_lowercase(), PascalCaseMode::Hard);
                    lookup_enum_map.get(&pascal)
                })
            })
            .map(|lookup| generate_repr_attribute(lookup.max_value)),
        FieldKind::IndirectLookup => field
            .enum_indirect_name
            .as_ref()
            .and_then(|name| {
                lookup_indir_map.get(name).or_else(|| {
                    let pascal = to_pascal_case(&name.to_lowercase(), PascalCaseMode::Hard);
                    lookup_indir_map.get(&pascal)
                })
            })
            .map(|_| "u16"),
        _ => None,
    }
}

/// Normalize CANboat `FieldType` values into the internal `FieldKind` enumeration.
pub(crate) fn map_to_fieldkind(field: &Fields) -> FieldKind {
    match field.kind.as_str() {
        "NUMBER" => FieldKind::Number,
        "FLOAT" => FieldKind::Float,
        "LOOKUP" => FieldKind::Lookup,
        "INDIRECT_LOOKUP" => FieldKind::IndirectLookup,
        "BITLOOKUP" => FieldKind::BitLookup,
        "PGN" => FieldKind::Pgn,
        "DATE" => FieldKind::Date,
        "TIME" => FieldKind::Time,
        "DURATION" => FieldKind::Duration,
        "MMSI" => FieldKind::Mmsi,
        "DECIMAL" => FieldKind::Decimal,
        "STRING_FIX" => FieldKind::StringFix,
        "STRING_LZ" => FieldKind::StringLz,
        "STRING_LAU" => FieldKind::StringLau,
        "BINARY" => FieldKind::Binary,
        "RESERVED" => FieldKind::Reserved,
        "SPARE" => FieldKind::Spare,
        "ISO_NAME" => FieldKind::IsoName,
        _ => FieldKind::Unimplemented,
    }
}
