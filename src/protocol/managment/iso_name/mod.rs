//! ISO 11783 NAME field implementation (64 bits). This field uniquely
//! identifies equipment on the NMEA 2000 network and is used throughout
//! the address-claim procedure. The module provides a typed wrapper around
//! the raw `u64` plus safe accessors/builders.
//!
//! # Bit layout (Little Endian order)
//!
//! ```text
//! Bits  0-20  (21 bits) : Unique number
//! Bits 21-31  (11 bits) : Manufacturer code
//! Bits 32-34  ( 3 bits) : Device instance (lower part)
//! Bits 35-39  ( 5 bits) : Device instance (upper part)
//! Bits 40-47  ( 8 bits) : Device function
//! Bit  48     ( 1 bit ) : Reserved
//! Bits 49-55  ( 7 bits) : Device class
//! Bits 56-59  ( 4 bits) : System instance
//! Bits 60-62  ( 3 bits) : Industry group
//! Bit  63     ( 1 bit ) : Arbitrary Address Capable
//! ```

use crate::protocol::lookups::*;
use crate::protocol::messages::Pgn60928;
use core::fmt;

/// Wrapper around the ISO 11783 NAME field (64 bits).
///
/// Provides a lightweight API to manipulate the field used in PGN 60928
/// (address claim).
///
/// # Example
///
/// ```
/// use korri_n2k::protocol::managment::iso_name::IsoName;
///
/// let name = IsoName::builder()
///     .unique_number(123456)
///     .manufacturer_code(275)  // Exemple : Actisense
///     .device_function(130)    // Exemple : Diagnostic Tool
///     .device_class(25)        // Exemple : Inter/Intranetwork Device
///     .arbitrary_address_capable(true)
///     .build();
///
/// assert_eq!(name.unique_number(), 123456);
/// assert_eq!(name.manufacturer_code(), 275);
/// assert!(name.is_arbitrary_address_capable());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct IsoName(u64);

impl IsoName {
    /// Build an `IsoName` from the raw value.
    #[inline]
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    /// Return the underlying `u64`.
    #[inline]
    pub const fn raw(&self) -> u64 {
        self.0
    }

    /// Create a builder to construct an `IsoName`.
    #[inline]
    pub const fn builder() -> IsoNameBuilder {
        IsoNameBuilder::new()
    }

    // Individual accessors for NAME sub-fields.

    /// Unique number (bits 0-20, 21 bits).
    ///
    /// Identifies the product within the manufacturer lineup.
    #[inline]
    pub const fn unique_number(&self) -> u32 {
        (self.0 & 0x1F_FFFF) as u32
    }

    /// Manufacturer code (bits 21-31, 11 bits).
    #[inline]
    pub const fn manufacturer_code(&self) -> u16 {
        ((self.0 >> 21) & 0x7FF) as u16
    }

    /// Lower part of the device instance (bits 32-34, 3 bits).
    #[inline]
    pub const fn device_instance_lower(&self) -> u8 {
        ((self.0 >> 32) & 0x07) as u8
    }

    /// Upper part of the device instance (bits 35-39, 5 bits).
    #[inline]
    pub const fn device_instance_upper(&self) -> u8 {
        ((self.0 >> 35) & 0x1F) as u8
    }

    /// Full 8-bit instance (merge of upper and lower parts).
    #[inline]
    pub const fn device_instance(&self) -> u8 {
        (self.device_instance_lower() | (self.device_instance_upper() << 3)) & 0xFF
    }

    /// Device function (bits 40-47, 8 bits).
    #[inline]
    pub const fn device_function(&self) -> u8 {
        ((self.0 >> 40) & 0xFF) as u8
    }

    /// Reserved bit (bit 48).
    #[inline]
    pub const fn spare(&self) -> bool {
        ((self.0 >> 48) & 0x01) != 0
    }

    /// Device class (bits 49-55, 7 bits).
    #[inline]
    pub const fn device_class(&self) -> u8 {
        ((self.0 >> 49) & 0x7F) as u8
    }

    /// System instance (bits 56-59, 4 bits).
    #[inline]
    pub const fn system_instance(&self) -> u8 {
        ((self.0 >> 56) & 0x0F) as u8
    }

    /// Industry group (bits 60-62, 3 bits).
    ///
    /// Typical value: `4` for the marine industry.
    #[inline]
    pub const fn industry_group(&self) -> u8 {
        ((self.0 >> 60) & 0x07) as u8
    }

    /// Arbitrary Address Capable bit (bit 63).
    ///
    /// Indicates whether the node may claim arbitrary addresses (128-247).
    #[inline]
    pub const fn is_arbitrary_address_capable(&self) -> bool {
        ((self.0 >> 63) & 0x01) != 0
    }

    /// Returns `true` when the equipment is tagged as marine.
    #[inline]
    pub const fn is_marine(&self) -> bool {
        self.industry_group() == 4
    }
}

impl From<u64> for IsoName {
    #[inline]
    fn from(raw: u64) -> Self {
        Self::from_raw(raw)
    }
}

impl From<IsoName> for u64 {
    #[inline]
    fn from(name: IsoName) -> Self {
        name.raw()
    }
}

impl From<Pgn60928> for IsoName {
    /// Convert a generated PGN 60928 structure into a compact `IsoName`.
    ///
    /// The reserved bit is forced to `false` because it is not exposed by the PGN.
    /// Note: `device_function` is an INDIRECT_LOOKUP stored as `u8` inside `Pgn60928`.
    fn from(pgn: Pgn60928) -> Self {
        Self::builder()
            .unique_number(pgn.unique_number)
            .manufacturer_code(u16::from(pgn.manufacturer_code))
            .device_instance_lower(pgn.device_instance_lower)
            .device_instance_upper(pgn.device_instance_upper)
            .device_function(pgn.device_function) // device_function est u8 (INDIRECT_LOOKUP)
            .spare(false) // Reserved field, always false
            .device_class(u8::from(pgn.device_class))
            .system_instance(pgn.system_instance)
            .industry_group(u8::from(pgn.industry_group))
            .arbitrary_address_capable(pgn.arbitrary_address_capable != YesNo::No)
            .build()
    }
}

impl From<IsoName> for Pgn60928 {
    /// Convert a compact `IsoName` into the generated PGN 60928 structure.
    ///
    /// Uses `new()` to ensure the reserved field is initialized.
    /// Note: `device_function` is an INDIRECT_LOOKUP stored as `u8` in `Pgn60928`.
    fn from(name: IsoName) -> Self {
        let mut pgn = Pgn60928::new();
        pgn.unique_number = name.unique_number();
        pgn.manufacturer_code =
            ManufacturerCode::try_from(name.manufacturer_code()).unwrap_or_default();
        pgn.device_instance_lower = name.device_instance_lower();
        pgn.device_instance_upper = name.device_instance_upper();
        pgn.device_function = name.device_function(); // device_function stays as u8 (INDIRECT_LOOKUP)
                                                      // Reserved field remains zero thanks to `new()`.
        pgn.device_class = DeviceClass::try_from(name.device_class()).unwrap_or_default();
        pgn.system_instance = name.system_instance();
        pgn.industry_group = IndustryCode::try_from(name.industry_group()).unwrap_or_default();
        pgn.arbitrary_address_capable = if name.is_arbitrary_address_capable() {
            YesNo::Yes
        } else {
            YesNo::No
        };
        pgn
    }
}

impl fmt::Display for IsoName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "IsoName {{ unique: {}, mfg: {}, func: {}, class: {}, inst: {}, aac: {} }}",
            self.unique_number(),
            self.manufacturer_code(),
            self.device_function(),
            self.device_class(),
            self.device_instance(),
            self.is_arbitrary_address_capable()
        )
    }
}

/// Fluent builder used to construct an `IsoName`.
///
/// # Example
///
/// ```
/// use korri_n2k::protocol::managment::iso_name::IsoName;
///
/// let name = IsoName::builder()
///     .unique_number(12345)
///     .manufacturer_code(275)
///     .arbitrary_address_capable(true)
///     .build();
/// ```
#[derive(Debug, Clone, Copy)]
pub struct IsoNameBuilder {
    raw: u64,
}

impl IsoNameBuilder {
    /// Initialize the builder with all fields cleared.
    #[inline]
    pub const fn new() -> Self {
        Self { raw: 0 }
    }

    /// Set the unique number (bits 0-20, 21 bits).
    ///
    /// # Panics
    /// Panics when the value does not fit in 21 bits (> 0x1FFFFF).
    #[inline]
    pub const fn unique_number(mut self, value: u32) -> Self {
        assert!(value <= 0x1F_FFFF, "Unique number must fit in 21 bits");
        self.raw = (self.raw & !0x1F_FFFF) | (value as u64 & 0x1F_FFFF);
        self
    }

    /// Set the manufacturer code (bits 21-31, 11 bits).
    ///
    /// # Panics
    /// Panics when the value exceeds 11 bits (> 0x7FF).
    #[inline]
    pub const fn manufacturer_code(mut self, value: u16) -> Self {
        assert!(value <= 0x7FF, "Manufacturer code must fit in 11 bits");
        self.raw = (self.raw & !(0x7FF << 21)) | ((value as u64 & 0x7FF) << 21);
        self
    }

    /// Set the lower portion of the device instance (bits 32-34, 3 bits).
    ///
    /// # Panics
    /// Panics when the value exceeds 3 bits (> 0x07).
    #[inline]
    pub const fn device_instance_lower(mut self, value: u8) -> Self {
        assert!(value <= 0x07, "Device instance lower must fit in 3 bits");
        self.raw = (self.raw & !(0x07 << 32)) | ((value as u64 & 0x07) << 32);
        self
    }

    /// Set the upper portion of the device instance (bits 35-39, 5 bits).
    ///
    /// # Panics
    /// Panics when the value exceeds 5 bits (> 0x1F).
    #[inline]
    pub const fn device_instance_upper(mut self, value: u8) -> Self {
        assert!(value <= 0x1F, "Device instance upper must fit in 5 bits");
        self.raw = (self.raw & !(0x1F << 35)) | ((value as u64 & 0x1F) << 35);
        self
    }

    /// Convenience helper to set the full 8-bit instance.
    #[inline]
    pub const fn device_instance(self, value: u8) -> Self {
        self.device_instance_lower(value & 0x07)
            .device_instance_upper((value >> 3) & 0x1F)
    }

    /// Set the device function (bits 40-47, 8 bits).
    #[inline]
    pub const fn device_function(mut self, value: u8) -> Self {
        self.raw = (self.raw & !(0xFF << 40)) | ((value as u64) << 40);
        self
    }

    /// Update the reserved bit (bit 48).
    #[inline]
    pub const fn spare(mut self, value: bool) -> Self {
        self.raw = (self.raw & !(0x01 << 48)) | ((value as u64) << 48);
        self
    }

    /// Set the device class (bits 49-55, 7 bits).
    ///
    /// # Panics
    /// Panics when the value exceeds 7 bits (> 0x7F).
    #[inline]
    pub const fn device_class(mut self, value: u8) -> Self {
        assert!(value <= 0x7F, "Device class must fit in 7 bits");
        self.raw = (self.raw & !(0x7F << 49)) | ((value as u64 & 0x7F) << 49);
        self
    }

    /// Set the system instance (bits 56-59, 4 bits).
    ///
    /// # Panics
    /// Panics when the value exceeds 4 bits (> 0x0F).
    #[inline]
    pub const fn system_instance(mut self, value: u8) -> Self {
        assert!(value <= 0x0F, "System instance must fit in 4 bits");
        self.raw = (self.raw & !(0x0F << 56)) | ((value as u64 & 0x0F) << 56);
        self
    }

    /// Set the industry group (bits 60-62, 3 bits).
    ///
    /// Typical value: `4` for marine uses.
    ///
    /// # Panics
    /// Panics when the value exceeds 3 bits (> 0x07).
    #[inline]
    pub const fn industry_group(mut self, value: u8) -> Self {
        assert!(value <= 0x07, "Industry group must fit in 3 bits");
        self.raw = (self.raw & !(0x07 << 60)) | ((value as u64 & 0x07) << 60);
        self
    }

    /// Configure the Arbitrary Address Capable bit (bit 63).
    #[inline]
    pub const fn arbitrary_address_capable(mut self, value: bool) -> Self {
        self.raw = (self.raw & !(0x01 << 63)) | ((value as u64) << 63);
        self
    }

    /// Build the final `IsoName`.
    #[inline]
    pub const fn build(self) -> IsoName {
        IsoName(self.raw)
    }
}

impl Default for IsoNameBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unique_number_extraction() {
        let name = IsoName::builder().unique_number(0x1ABCDE).build();
        assert_eq!(name.unique_number(), 0x1ABCDE);
    }

    #[test]
    fn test_manufacturer_code_extraction() {
        let name = IsoName::builder().manufacturer_code(275).build();
        assert_eq!(name.manufacturer_code(), 275);
    }

    #[test]
    fn test_arbitrary_address_capable() {
        let name_aac = IsoName::builder().arbitrary_address_capable(true).build();
        assert!(name_aac.is_arbitrary_address_capable());

        let name_not_aac = IsoName::builder().arbitrary_address_capable(false).build();
        assert!(!name_not_aac.is_arbitrary_address_capable());
    }

    #[test]
    fn test_device_instance() {
        let name = IsoName::builder().device_instance(0xAB).build();
        // Device instance is split: lower 3 bits, upper 5 bits
        assert_eq!(name.device_instance(), 0xAB);
    }

    #[test]
    fn test_all_fields() {
        let name = IsoName::builder()
            .unique_number(123456)
            .manufacturer_code(275)
            .device_instance(42)
            .device_function(130)
            .device_class(25)
            .system_instance(7)
            .industry_group(4)
            .arbitrary_address_capable(true)
            .build();

        assert_eq!(name.unique_number(), 123456);
        assert_eq!(name.manufacturer_code(), 275);
        assert_eq!(name.device_instance(), 42);
        assert_eq!(name.device_function(), 130);
        assert_eq!(name.device_class(), 25);
        assert_eq!(name.system_instance(), 7);
        assert_eq!(name.industry_group(), 4);
        assert!(name.is_arbitrary_address_capable());
        assert!(name.is_marine());
    }

    #[test]
    fn test_raw_conversion() {
        let raw_value = 0x8123456789ABCDEF;
        let name = IsoName::from_raw(raw_value);
        assert_eq!(name.raw(), raw_value);

        let converted: u64 = name.into();
        assert_eq!(converted, raw_value);
    }

    #[test]
    fn test_round_trip() {
        let original = IsoName::builder()
            .unique_number(0x12345)
            .manufacturer_code(0x2AB)
            .device_instance(0x55)
            .device_function(0xAA)
            .device_class(0x33)
            .system_instance(0x0C)
            .industry_group(0x04)
            .arbitrary_address_capable(true)
            .build();

        let raw = original.raw();
        let restored = IsoName::from_raw(raw);

        assert_eq!(original, restored);
        assert_eq!(original.unique_number(), restored.unique_number());
        assert_eq!(original.manufacturer_code(), restored.manufacturer_code());
        assert_eq!(original.device_instance(), restored.device_instance());
        assert_eq!(original.device_function(), restored.device_function());
        assert_eq!(original.device_class(), restored.device_class());
        assert_eq!(original.system_instance(), restored.system_instance());
        assert_eq!(original.industry_group(), restored.industry_group());
        assert_eq!(
            original.is_arbitrary_address_capable(),
            restored.is_arbitrary_address_capable()
        );
    }

    #[test]
    fn test_bit_63_aac() {
        // Test that bit 63 is correctly set for AAC
        let name_aac = IsoName::builder().arbitrary_address_capable(true).build();
        assert_eq!(name_aac.raw() & (1u64 << 63), 1u64 << 63);

        let name_not_aac = IsoName::builder().arbitrary_address_capable(false).build();
        assert_eq!(name_not_aac.raw() & (1u64 << 63), 0);
    }

    #[test]
    fn test_address_claiming_compatibility() {
        // Test compatibility with existing address claiming code
        // From address_claiming/mod.rs line 24:
        // let is_arbitrary_capable = (my_name >> 63) & 1 == 1;

        let my_name_raw = 0x8000_0000_0000_0000u64; // AAC bit set
        let iso_name = IsoName::from_raw(my_name_raw);

        // Both methods should give the same result
        let old_method = (my_name_raw >> 63) & 1 == 1;
        let new_method = iso_name.is_arbitrary_address_capable();

        assert_eq!(old_method, new_method);
        assert!(new_method);
    }

    #[test]
    fn test_pgn60928_to_isoname_conversion() {
        // Create a Pgn60928 with known values (using valid enum values)
        // Note: device_function is an INDIRECT_LOOKUP encoded across two u8 values
        let mut pgn = Pgn60928::new();
        pgn.unique_number = 123456;
        pgn.manufacturer_code = ManufacturerCode::try_from(69).unwrap(); // ArksEnterprisesInc
        pgn.device_instance_lower = 3;
        pgn.device_instance_upper = 5;
        // Use the helper to assign DeviceFunction
        pgn.set_device_function(DeviceFunction::Diagnostic); // 2690 = 0x0A82
        pgn.device_class = DeviceClass::try_from(25).unwrap(); // InternetworkDevice
        pgn.system_instance = 7;
        pgn.arbitrary_address_capable = YesNo::Yes;

        // Convert to IsoName
        let iso_name: IsoName = pgn.into();

        // Verify all fields match (device_function corresponds to the low byte)
        assert_eq!(iso_name.unique_number(), 123456);
        assert_eq!(iso_name.manufacturer_code(), 69);
        assert_eq!(iso_name.device_instance_lower(), 3);
        assert_eq!(iso_name.device_instance_upper(), 5);
        assert_eq!(iso_name.device_function(), 0x82); // 2690 = 0x0A82, low byte = 0x82 = 130
        assert!(!iso_name.spare()); // spare is always false from Pgn60928
        assert_eq!(iso_name.device_class(), 25);
        assert_eq!(iso_name.system_instance(), 7);
        assert!(iso_name.is_arbitrary_address_capable());
    }

    #[test]
    fn test_isoname_to_pgn60928_conversion() {
        // Create an IsoName with known values (using u8 for device_function)
        // Since ISO NAME only uses 8 bits for device_function, use a value < 256
        let iso_name = IsoName::builder()
            .unique_number(654321)
            .manufacturer_code(78) // FwMurphyEnovationControls
            .device_instance_lower(7)
            .device_instance_upper(15)
            .device_function(200) // Raw value (no standard DeviceFunction < 256)
            .spare(true)
            .device_class(20) // SafetySystems
            .system_instance(10)
            .industry_group(4) // MarineIndustry
            .arbitrary_address_capable(false)
            .build();

        // Convert to Pgn60928
        let pgn: Pgn60928 = iso_name.into();

        // Verify all fields match (spare is private and not tested)
        assert_eq!(pgn.unique_number, 654321);
        assert_eq!(u16::from(pgn.manufacturer_code), 78);
        assert_eq!(pgn.device_instance_lower, 7);
        assert_eq!(pgn.device_instance_upper, 15);
        assert_eq!(u16::from(pgn.device_function), 200);
        // spare field is private, cannot be tested
        assert_eq!(u8::from(pgn.device_class), 20);
        assert_eq!(pgn.system_instance, 10);
        assert_eq!(u8::from(pgn.industry_group), 4);
        assert_eq!(pgn.arbitrary_address_capable, YesNo::No);
    }

    #[test]
    fn test_pgn60928_isoname_round_trip() {
        // Create a Pgn60928 (using valid enum values)
        // Note: device_function is an INDIRECT_LOOKUP
        let mut original_pgn = Pgn60928::new();
        original_pgn.unique_number = 999888;
        original_pgn.manufacturer_code = ManufacturerCode::try_from(88).unwrap(); // HemisphereGpsInc
        original_pgn.device_instance_lower = 1;
        original_pgn.device_instance_upper = 31;
        // Use helper to assign DeviceFunction
        original_pgn.set_device_function(DeviceFunction::AlarmEnunciator5230); // 5230 = 0x146E
        original_pgn.device_class = DeviceClass::try_from(30).unwrap(); // ElectricalDistribution
        original_pgn.system_instance = 15;
        original_pgn.arbitrary_address_capable = YesNo::Yes;

        // Convert to IsoName and back
        let iso_name: IsoName = original_pgn.into();
        let restored_pgn: Pgn60928 = iso_name.into();

        // Verify round-trip preservation (spare is private and not tested)
        assert_eq!(original_pgn.unique_number, restored_pgn.unique_number);
        assert_eq!(
            original_pgn.manufacturer_code,
            restored_pgn.manufacturer_code
        );
        assert_eq!(
            original_pgn.device_instance_lower,
            restored_pgn.device_instance_lower
        );
        assert_eq!(
            original_pgn.device_instance_upper,
            restored_pgn.device_instance_upper
        );
        // device_function is an INDIRECT_LOOKUP stored as two u8 values.
        // The low byte must match.
        assert_eq!(original_pgn.device_function, restored_pgn.device_function);
        // spare field is private, cannot be tested
        assert_eq!(original_pgn.device_class, restored_pgn.device_class);
        assert_eq!(original_pgn.system_instance, restored_pgn.system_instance);
        assert_eq!(original_pgn.industry_group, restored_pgn.industry_group);
        assert_eq!(
            original_pgn.arbitrary_address_capable,
            restored_pgn.arbitrary_address_capable
        );
    }
}
