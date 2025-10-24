# Technical Analysis of the `canboat.json` Structure

This reference explains how the korri-n2k toolchain interprets the CANboat database. It summarizes each data construct, how the code generator (`build.rs`) transforms it into Rust, and how the runtime engine consumes the generated metadata.

## 1. Understanding the `FieldTypes`

`FieldTypes` describe the binary layout of each field. They can be grouped into five broad families, each requiring a dedicated handling strategy in the engine.

---

### Category 1: Numeric Primitives

These fields store raw numbers. The engine must honor bit length, signedness, and optional scaling (`resolution`).

- **`NUMBER`** — the most common field kind.  
  **Description:** little-endian integers between 1 and 64 bits, signed or unsigned. A scaling factor is frequently present.  
  **Handling:** requires bit-precise readers and writers (`BitReader`/`BitWriter`) that support unaligned access. Signed numbers must be sign-extended.  
  **Example (PGN 129025 “Position, Rapid Update”):** `latitude` and `longitude` are `NUMBER` fields (`s32`) scaled to degrees.

- **`FLOAT`**  
  **Description:** 32-bit IEEE-754 floating-point values.  
  **Handling:** read/write four bytes, then reinterpret via `f32::from_bits`.  
  **Example (PGN 65360 “SimNet: Attitude”):** `yaw`, `pitch`, and `roll` are stored as direct floats.

- **`DECIMAL`**  
  **Description:** Binary-Coded Decimal (BCD); each byte holds two decimal digits.  
  **Handling:** needs a dedicated BCD codec. Serializing `1234` yields `[0x12, 0x34]`; deserialization performs the reverse.  
  **Example (PGN 127497 “Trip Parameters, Engine”):** `tripFuelUsed` is encoded as `DECIMAL`.

- **`TIME`, `DATE`, `DURATION`**  
  **Description:** time-of-day, date, and duration stored as integers.  
  **Handling:** treated like `NUMBER`, but newtypes such as `N2kTime` or `N2kDate` keep semantics explicit.  
  **Example (PGN 129029 “GNSS Position Data”):** includes `date` (`DATE`) and `time` (`TIME`) to provide a full timestamp.

- **`MMSI`, `PGN`**  
  **Description:** identifiers for vessels (MMSI) and message types (PGN).  
  **Handling:** stored as integers (`u32` and `u24`).  
  **Example:** PGN 129794 “AIS Class A Position Report” exposes an `mmsi` field; PGN 60928 “ISO Address Claim” carries a `pgn` field.

---

### Category 2: Enumerations and Bitfields

These fields carry symbolic meaning on top of the numeric value. The engine reads them as integers, while generated code exposes expressive Rust types.

- **`LOOKUP`**  
  **Description:** integer mapped to a named alternative (`0 -> "Off"`, `1 -> "On"`, …).  
  **Handling:** `build.rs` generates a Rust enum for every `LookupEnumeration`. The engine deserializes the integer, the domain layer converts it to the enum.  
  **Example (PGN 130306 “Wind Data”):** `windReference` selects the wind frame of reference through the `WIND_REFERENCE` enum.

- **`BITLOOKUP`**  
  **Description:** bitmask where each bit stands for an independent flag.  
  **Handling:** generator emits helper structs with bit masks; the engine reads the raw bits and consumers query the resulting flags.  
  **Example (PGN 127489 “Engine Parameters, Dynamic”):** `status1` and `status2` expose alarms as bitfields.

- **`INDIRECT_LOOKUP`**  
  **Description:** two consecutive fields combine into a single lookup value (upper byte + lower byte).  
  **Handling:** generated helpers assemble/disassemble the combined enum while keeping each underlying byte accessible.

- **`SPARE` / `RESERVED`**  
  **Description:** bits that must be written with a fixed value (`SPARE` → `1`, `RESERVED` → `0`) and ignored when reading.  
  **Handling:** serialization enforces the mandated value; deserialization skips them (with optional validation).

---

### Category 3: Strings and Binary Blocks

- **`STRING_FIX`** — fixed-length ASCII buffers. Stored as `[u8; N]`; no trimming occurs on read.  
- **`STRING_LZ` / `STRING_LAU`** — variable-length strings prefixed by a byte length. `STRING_LAU` adds an encoding byte per ISO 11783.  
- **`BINARY`** — raw byte arrays where the descriptor specifies the exact bit length (must be a multiple of eight).

Writers must clamp variable-length content to the maximum allowed by the descriptor. Readers ensure that declared lengths never exceed the payload.

---

### Category 4: Dynamic Helpers

- **`INDIR_LOOKUP_FIELD_TYPE` metadata** — adds extra information (field type, unit, resolution) for indirect lookups and is surfaced through generated metadata structs.
- **`VARIABLE` / `DYNAMIC_FIELD_KEY`** — signal dynamic layouts whose interpretation depends on previous field values (see §2.4).

---

## 2. PGN Families by Payload Structure

Grouping PGNs by layout helps both the generator and the runtime engine pick the right strategy.

### 2.1. Fixed Length (≤ 8 bytes)

The payload fits in a single CAN frame and its length is known at compile time.

- **Characteristics**
  - `"Length"` is provided and ≤ 8.
  - No repeating or variable blocks.
- **Handling**
  - Read/write directly into an 8-byte buffer.
  - No Fast Packet transport required.
  - Generated code can use constant offsets.

**Example: PGN 127250 – Vessel Heading**
```json
{
  "PGN": 127250,
  "Id": "vesselHeading",
  "Description": "Vessel Heading",
  "Type": "Single",
  "Length": 8,
  "Fields": [
    { "Id": "sid", "BitLength": 8, "Type": "NUMBER" },
    { "Id": "heading", "BitLength": 16, "Type": "NUMBER", "Unit": "rad" },
    { "Id": "deviation", "BitLength": 16, "Type": "NUMBER", "Unit": "rad" },
    { "Id": "variation", "BitLength": 16, "Type": "NUMBER", "Unit": "rad" },
    { "Id": "reference", "BitLength": 2, "Type": "LOOKUP" },
    { "Id": "reserved", "BitLength": 6, "Type": "RESERVED" }
  ]
}
```
> Exactly 8 bytes; every field sits at a fixed offset.

### 2.2. Fixed Length (> 8 bytes)

Length is still deterministic but the payload spans multiple CAN frames.

- **Characteristics**
  - `"Length"` is provided and greater than 8.
  - No repeating or variable blocks.
- **Handling**
  - Requires Fast Packet segmentation.
  - Buffers are allocated to the declared size.
  - Field offsets remain constant.

**Example: PGN 129040 – AIS Class B**
```json
{
  "PGN": 129040,
  "Id": "aisClassBExtendedPositionReport",
  "Description": "AIS Class B Extended Position Report",
  "Priority": 4,
  "Type": "Fast",
  "Length": 54,
  "TransmissionIrregular": true,
  "Fields": [
    { "...": "..." }
  ]
}
```
> Always 54 bytes; Fast Packet carries the payload but the structure is fully predictable.

### 2.3. Variable Length (Repeating Blocks)

Typical for list-oriented PGNs whose payload repeats a group of fields.

- **Characteristics**
  - JSON definition includes `RepeatingFieldSet1*` entries.
  - `RepeatingFieldSet1CountField` points to the counter.
  - `RepeatingFieldSet1StartField` and `RepeatingFieldSet1Size` define the repeated slice.
- **Handling**
  - Deserialization reads the header first, fetches the counter, then loops `N` times.
  - Serialization emits the header and iterates over each element.
  - `build.rs` generates both a container struct and an element struct/array to store repetitions.

**Example: PGN 129540 – GNSS Sats in View**
```json
{
  "PGN": 129540,
  "Id": "gnssSatsInView",
  "Description": "GNSS Sats in View",
  "Priority": 6,
  "Type": "Fast",
  "FieldCount": 11,
  "MinLength": 3,
  "RepeatingFieldSet1Size": 7,
  "RepeatingFieldSet1StartField": 5,
  "RepeatingFieldSet1CountField": 4,
  "Fields": [
    { "Order": 1, "Id": "sid", "BitLength": 8 },
    { "Order": 2, "Id": "rangeResidualMode", "BitLength": 2 },
    { "Order": 3, "Id": "reserved", "BitLength": 6 },
    { "Order": 4, "Id": "satsInView", "BitLength": 8 },
    { "Order": 5, "Id": "prn", "BitLength": 8 },
    { "Order": 6, "Id": "elevation", "BitLength": 16 },
    { "Order": 7, "Id": "azimuth", "BitLength": 16 },
    { "Order": 8, "Id": "snr", "BitLength": 16 },
    { "Order": 9, "Id": "rangeResiduals", "BitLength": 32 },
    { "Order": 10, "Id": "status", "BitLength": 4 },
    { "Order": 11, "Id": "reserved11", "BitLength": 4 }
  ]
}
```
> The payload length depends on `satsInView` (order 4). Each satellite contributes a 7-byte block starting at order 5, so the engine only discovers the final size after parsing the header.

### 2.4. Dynamic Layout

Fields later in the payload change meaning based on the interpreted value of previous fields. This is the most complex scenario.

- **Characteristics**
  - Uses special field kinds such as `VARIABLE` or `DYNAMIC_FIELD_KEY`.
  - A “key” field determines the format of the subsequent bytes.
- **Handling**
  - Requires an advanced runtime engine that can look up other descriptors on demand.
  - Static Rust structs are usually insufficient; higher-level abstractions or dynamic decoding are needed.

**Example: PGN 126208 – NMEA Group Function**
```json
{
  "PGN": 126208,
  "Id": "nmeaGroupFunction",
  "Description": "NMEA - Group Function",
  "Type": "Fast",
  "Fields": [
    { "Id": "functionCode", "BitLength": 16, "Type": "LOOKUP" },
    { "Id": "pgn", "BitLength": 24, "Type": "PGN" },
    { "Id": "numberOfParameters", "BitLength": 8 },
    { "Id": "parameter", "Type": "VARIABLE" }
  ]
}
```
> The message is a container. The engine must read `functionCode`, fetch the target `pgn`, then load that PGN’s descriptor to decode the `parameter` block. Payload semantics are therefore context-dependent.

---

## 3. Anatomy of a `LookupEnumeration`

Lookup enumerations provide the mapping tables used by `LOOKUP` fields.

**Example**
```json
{
  "Name": "WIND_REFERENCE",
  "MaxValue": 7,
  "EnumValues": [
    { "Name": "True (ground referenced to North)", "Value": 0 },
    { "Name": "Magnetic", "Value": 1 },
    { "Name": "Apparent", "Value": 2 }
  ]
}
```
- `Name`: unique identifier for the enumeration.
- `EnumValues`: array of name/value pairs available for code generation.

---

## 4. Implementation Strategy

The CANboat data feeds a two-stage pipeline:

1. **Frontend (`build.rs`)**
   - Parses `canboat.json` once at build time.
   - Generates one Rust struct per PGN (for example `Pgn129026CogSogRapidUpdate`).
   - Emits a static `PgnDescriptor` mirroring the JSON (field descriptors, repeating sets, metadata).

2. **Backend (`engine.rs`)**
   - Ships only the generated structs and descriptors—no JSON ends up in the final binary.
   - A central engine orchestrates serialization and deserialization using `BitReader` and `BitWriter`.
   - **Deserialization:** iterates over the descriptor, extracts each field, applies scaling, and writes it to the target struct via the `FieldAccess` trait.
   - **Serialization:** reads values from the struct, converts them according to the descriptor, and writes the encoded bits to the output buffer.

This design confines binary manipulation to a well-tested engine while keeping JSON parsing at build time. The result is deterministic, fast, `no_std` friendly, and easy to audit.

---
