use crate::build_core::name_helpers::{to_pascal_case, PascalCaseMode};
use serde::Deserialize;

//==================================================================================MANIFEST
// Structures to deserialize `pgn_manifest.json`.
// The manifest acts as a filter so that only required PGNs are generated.
#[derive(Debug, Deserialize)]
/// Manifest describing which PGNs must be generated.
pub(crate) struct Manifest {
    pub(crate) pgns: Vec<Pgn>,
}

#[derive(Debug, Deserialize)]
/// Entry in the PGN list to generate.
pub(crate) struct Pgn {
    pub(crate) id: u32,
}

//==================================================================================CANBOAT_DOC
// Structures used to deserialize `canboat.json`.
//==============================================================LOOKUP_DOMAIN
//==========================================LOOKUP_DYN
#[derive(Debug, Clone, PartialEq)]
/// Normalized representation for an enumeration variant.
pub(crate) enum VariantData {
    // Pour LookupEnum et LookupBitEnum
    Simple { name: String, value: u32 },
    // Pour LookupFieldTypeEnum
    Full(VariantMetaData),
}

#[derive(Debug, PartialEq, Clone)]
/// Full metadata for `LookupFieldTypeEnum` variants.
pub(crate) struct VariantMetaData {
    pub(crate) name: String,
    pub(crate) value: u32,
    pub(crate) field_type: String,
    pub(crate) resolution: Option<f32>,
    pub(crate) unit: Option<String>,
    pub(crate) bits: String,
    pub(crate) lookup_bit_enum: Option<String>,
}

pub(crate) trait LookupGenerator {
    /// Canonical enumeration name.
    fn name(&self) -> &str;
    /// Highest meaningful value (useful to size types).
    fn max_value(&self) -> u32;
    /// Normalized list of variants to generate.
    fn variants(&self) -> Vec<VariantData>;
    // FIELDTYPE -> 2
    // LOOKUPINDIRECT -> 1
    // OTHER -> 0
    /// Internal code used to qualify the enumeration kind.
    fn metadata_code(&self) -> u8;
}
//==========================================LOOKUP_FIELDTYPE_ENUM
#[derive(Debug, Deserialize, Clone)]
/// Describes an `EnumFieldTypeValues` enumeration from CANboat.
pub(crate) struct LookupFieldTypeEnum {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "MaxValue")]
    max_value: u32,
    #[serde(rename = "EnumFieldTypeValues")]
    fieldtype_enum_values: Vec<FieldTypeEnumValues>,
}

#[derive(Debug, Deserialize, Clone)]
/// Individual entry within a `LookupFieldTypeEnum`.
pub(crate) struct FieldTypeEnumValues {
    #[serde(rename = "name")]
    pub(crate) name: String,
    #[serde(rename = "value")]
    pub(crate) value: u32,
    #[serde(rename = "FieldType")]
    pub(crate) fieldtype: String,
    #[serde(rename = "Resolution")]
    pub(crate) resolution: Option<f32>,
    #[serde(rename = "Unit")]
    pub(crate) unit: Option<String>,
    #[serde(rename = "Bits")]
    pub(crate) bits: String,
    #[serde(rename = "LookupBitEnumeration")]
    pub(crate) lookup_bit_enum: Option<String>,
}

impl LookupGenerator for LookupFieldTypeEnum {
    fn name(&self) -> &str {
        &self.name
    }
    fn max_value(&self) -> u32 {
        self.max_value
    }
    fn variants(&self) -> Vec<VariantData> {
        self.fieldtype_enum_values
            .iter()
            .map(|v| {
                VariantData::Full(VariantMetaData {
                    name: to_pascal_case(&v.name.to_lowercase(), PascalCaseMode::Hard),
                    value: v.value,
                    field_type: v.fieldtype.clone(),
                    resolution: v.resolution,
                    unit: v.unit.clone(),
                    bits: v.bits.clone(),
                    lookup_bit_enum: v.lookup_bit_enum.clone(),
                })
            })
            .collect()
    }
    fn metadata_code(&self) -> u8 {
        2
    }
}
//==========================================LOOKUP_BIT_ENUM
#[derive(Debug, Deserialize, Clone)]
/// Specialization for bitmask enumerations (positional booleans).
pub(crate) struct LookupBitEnum {
    #[serde(rename = "Name")]
    pub(crate) name: String,
    #[serde(rename = "MaxValue")]
    pub(crate) max_value: u8,
    #[serde(rename = "EnumBitValues")]
    pub(crate) bit_enum_values: Vec<BitEnumValues>,
}
#[derive(Debug, Deserialize, Clone)]
pub(crate) struct BitEnumValues {
    #[serde(rename = "Name")]
    pub(crate) name: String,
    #[serde(rename = "Bit")]
    pub(crate) bit: u8,
}

impl LookupGenerator for LookupBitEnum {
    fn name(&self) -> &str {
        &self.name
    }
    fn max_value(&self) -> u32 {
        self.max_value as u32
    }
    fn variants(&self) -> Vec<VariantData> {
        self.bit_enum_values
            .iter()
            .map(|v| VariantData::Simple {
                name: to_pascal_case(&v.name.to_lowercase(), PascalCaseMode::Hard),
                value: v.bit as u32,
            })
            .collect()
    }
    fn metadata_code(&self) -> u8 {
        0
    }
}
// ==========================================LOOKUP_INDIRECT_ENUM
#[derive(Debug, Deserialize, Clone)]
/// Specialization for indirect enumerations (high/low byte pairs).
pub(crate) struct LookupIndirEnum {
    #[serde(rename = "Name")]
    pub(crate) name: String,
    #[serde(rename = "MaxValue")]
    pub(crate) max_value: u8,
    #[serde(rename = "EnumValues")]
    pub(crate) indir_enum_values: Vec<IndirEnumValues>,
}
#[derive(Debug, Deserialize, Clone)]
pub(crate) struct IndirEnumValues {
    #[serde(rename = "Name")]
    pub(crate) name: String,
    #[serde(rename = "Value1")]
    pub(crate) value1: u8,
    #[serde(rename = "Value2")]
    pub(crate) value2: u8,
}

impl LookupGenerator for LookupIndirEnum {
    fn name(&self) -> &str {
        &self.name
    }
    fn max_value(&self) -> u32 {
        self.max_value as u32
    }
    fn variants(&self) -> Vec<VariantData> {
        self.indir_enum_values
            .iter()
            .map(|v| {
                let combined_value = (v.value1 as u16) << 8 | (v.value2 as u16);
                VariantData::Simple {
                    name: to_pascal_case(&v.name.to_lowercase(), PascalCaseMode::Hard),
                    value: combined_value as u32,
                }
            })
            .collect()
    }
    fn metadata_code(&self) -> u8 {
        1
    }
}

//==========================================LOOKUP_ENUM
#[derive(Debug, Deserialize, PartialEq, Eq, Hash, Clone)]
/// Simple enumeration (value on N bits) described by CANboat.
pub(crate) struct LookupEnum {
    #[serde(rename = "Name")]
    pub(crate) name: String,
    #[serde(rename = "MaxValue")]
    pub(crate) max_value: u32,
    #[serde(rename = "EnumValues")]
    pub(crate) enum_values: Vec<EnumValues>,
}
#[derive(Debug, Deserialize, PartialEq, Eq, Hash, Clone)]
/// Elementary variant of a `LookupEnum`.
pub(crate) struct EnumValues {
    #[serde(rename = "Name")]
    pub(crate) name: String,
    #[serde(rename = "Value")]
    pub(crate) value: u32,
}

impl LookupGenerator for LookupEnum {
    fn name(&self) -> &str {
        &self.name
    }
    fn max_value(&self) -> u32 {
        self.max_value
    }
    fn variants(&self) -> Vec<VariantData> {
        self.enum_values
            .iter()
            .map(|v| VariantData::Simple {
                name: to_pascal_case(&v.name.to_lowercase(), PascalCaseMode::Hard),
                value: v.value,
            })
            .collect()
    }
    fn metadata_code(&self) -> u8 {
        0
    }
}
//==============================================================PGN_DOMAIN
//==========================================PGN
#[derive(Debug, Deserialize)]
#[allow(unused)]
/// Full PGN descriptor as provided by the CANboat database.
pub(crate) struct PgnInstructions {
    /// 1. Numeric PGN identifier.
    #[serde(rename = "PGN")]
    pub pgn_id: u32,
    /// 2. PGN name (debugging purposes).
    #[serde(rename = "Id")]
    pub pgn_name: String,
    /// 3. Human-friendly description.
    #[serde(rename = "Description")]
    pub pgn_description: String,
    /// 4. Message priority.
    #[serde(rename = "Priority")]
    pub priority: Option<u8>,
    /// 5. Additional explanation for the PGN.
    #[serde(rename = "Explanation")]
    pub explanation: Option<String>,
    /// 6. Transport type (Fast Packet / Single frame as string).
    #[serde(rename = "Type")]
    pub fastpacket: String,
    /// 7. Payload length in bytes.
    #[serde(rename = "Length")]
    pub length: Option<u8>,
    /// 8. Number of fields.
    #[serde(rename = "FieldCount")]
    pub field_count: Option<u8>,
    /// 9. Transmission interval.
    #[serde(rename = "TransmissionInterval")]
    pub trans_interval: Option<u16>,
    /// 10. Flag indicating irregular transmission interval.
    #[serde(rename = "TransmissionIrregular")]
    pub trans_irregular: Option<bool>,
    /// 11. Repeating Field Set 1 size.
    #[serde(rename = "RepeatingFieldSet1Size")]
    pub repeating_field_set_1_size: Option<u16>,
    /// 12. Repeating Field Set 1 start field index.
    #[serde(rename = "RepeatingFieldSet1StartField")]
    pub repeating_field_set_1_start_field: Option<u16>,
    /// 13. Repeating Field Set 1 counter field index.
    #[serde(rename = "RepeatingFieldSet1CountField")]
    pub repeating_field_set_1_count_field: Option<u16>,
    /// 14. Repeating Field Set 2 size.
    #[serde(rename = "RepeatingFieldSet2Size")]
    pub repeating_field_set_2_size: Option<u16>,
    /// 15. Repeating Field Set 2 start field index.
    #[serde(rename = "RepeatingFieldSet2StartField")]
    pub repeating_field_set_2_start_field: Option<u16>,
    /// 16. Repeating Field Set 2 counter field index.
    #[serde(rename = "RepeatingFieldSet2CountField")]
    pub repeating_field_set_2_count_field: Option<u16>,
    /// 17. Field descriptors.
    #[serde(rename = "Fields")]
    pub fields: Vec<Fields>,
}

#[derive(Debug, Deserialize)]
/// Field descriptor as provided by CANboat.
pub(crate) struct Fields {
    /// 0. Field order inside the table.
    #[serde(rename = "Order")]
    pub order: u16,
    /// 1. Identifier.
    #[serde(rename = "Id")]
    pub id: String,
    /// 2. Display name.
    #[serde(rename = "Name")]
    pub name: String,
    /// 3. Semantic field type.
    #[serde(rename = "FieldType")]
    pub kind: String,
    /// 4. Field length in bits.
    #[serde(rename = "BitLength")]
    pub bits_length: Option<u16>,
    /// 5. Whether the field length is variable.
    #[serde(rename = "BitLengthVariable")]
    pub bits_length_var: Option<bool>,
    /// 6. Absolute bit offset from the start of the payload.
    #[serde(rename = "BitOffset")]
    pub bits_offset: Option<u32>, // Absolute bit offset from the start of the payload.
    /// 7. Whether numeric fields are signed.
    #[serde(rename = "Signed")]
    pub signed: Option<bool>,
    /// 8. Optional resolution factor.
    #[serde(rename = "Resolution")]
    pub resolution: Option<f32>,
    /// 9. Direct lookup enumeration name (if any).
    #[serde(rename = "LookupEnumeration")]
    pub enum_direct_name: Option<String>,
    /// 10. Indirect lookup enumeration name.
    #[serde(rename = "LookupIndirectEnumeration")]
    pub enum_indirect_name: Option<String>,
    /// 11. Field order within indirect lookups.
    #[serde(rename = "LookupIndirectEnumerationFieldOrder")]
    pub enum_indirect_field_order: Option<u16>,
    /// 12. Physical unit (e.g. "m/s", "deg", "meters").
    #[serde(rename = "Unit")]
    pub physical_unit: Option<String>,
    /// 13. Physical quantity (e.g. "GEOGRAPHICAL_LATITUDE", "SPEED").
    #[serde(rename = "PhysicalQuantity")]
    pub physical_qty: Option<String>,
    /// 14. Optional description.
    #[serde(rename = "Description")]
    pub description: Option<String>,
    /// 15. Bitfield enumeration name for BITLOOKUP fields.
    #[serde(rename = "LookupBitEnumeration")]
    pub enum_bit_name: Option<String>,
}

#[derive(Debug, Default, Hash)]
/// Utility structure to group polymorphic PGNs during code generation.
pub(crate) struct PolyPgn {
    pub lookup_id: String,
    pub name: String,
    pub desc: String,
}
