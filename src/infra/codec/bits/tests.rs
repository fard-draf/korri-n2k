//! Exhaustive test suite for BitReader and BitWriter edge cases.
use super::*;

#[test]
/// Sequential reads without offset across primitive types.
fn test_read_aligned_bytes() {
    let data = [0x12, 0x34, 0x56, 0x78];
    let mut reader = BitReader::new(&data);
    assert_eq!(reader.read_u8(8).unwrap(), 0x12);
    assert_eq!(reader.read_u16(16).unwrap(), 0x5634);
    assert_eq!(reader.read_u8(8).unwrap(), 0x78);
}
#[test]
/// Read fields spanning two bytes (non-aligned).
fn test_read_non_aligned_bytes() {
    // data: 11100000 00001100
    //      0b11000..          => 24
    //     0b1                    0b00001 (build 25 with the last bit stored in the first byte)
    //                    1100 => 0b11000 | 0b00001 => 0b11001 = 25 (remaining MSBs end up in the last byte at offset 3)
    let data = [0b11100000, 0b00001100];
    let mut reader = BitReader::new(&data);
    reader.read_u64(2).unwrap(); // advance by 2 bits
    assert_eq!(reader.read_u8(5).unwrap(), 24);
    assert_eq!(reader.read_u8(5).unwrap(), 25);
}
#[test]
/// Read a field that crosses byte boundaries after an initial offset.
fn test_read_spanning_multiple_bytes() {
    // Read 12 bits starting from offset 4
    // data: ........ 11111111 1111....
    // value: 0b111111111111 = 4095
    let data = [0b10101111, 0b11111010];
    let mut reader = BitReader::new(&data);
    reader.read_u64(4).unwrap();
    assert_eq!(reader.read_u8(8).unwrap(), 170);
    assert_eq!(reader.read_u8(4).unwrap(), 15);
}

#[test]
/// Detects out-of-bounds reads.
fn test_read_out_of_bounds() {
    let data = [0xFF];
    let mut reader = BitReader::new(&data);
    assert!(reader.read_u8(8).is_ok());
    assert!(matches!(
        reader.read_u8(1),
        Err(BitReaderError::OutOfBounds {
            asked: 1,
            available: 0
        })
    ));
}

#[test]
/// Validates guard rails for maximum bit lengths per type.
fn test_read_num_bit_too_high() {
    let data = [0xFF];
    let mut reader = BitReader::new(&data);
    assert!(matches!(
        reader.read_u8(9),
        Err(BitReaderError::TooLongForType { max: 8, asked: 9 })
    ));
    assert!(matches!(
        reader.read_u16(17),
        Err(BitReaderError::TooLongForType { max: 16, asked: 17 })
    ));
    assert!(matches!(
        reader.read_u32(33),
        Err(BitReaderError::TooLongForType { max: 32, asked: 33 })
    ));
    assert!(matches!(
        reader.read_u64(65),
        Err(BitReaderError::TooLongForType { max: 64, asked: 65 })
    ));
}

#[test]
/// Read a full 64-bit block.
fn test_read_max() {
    let data = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88];
    let mut reader = BitReader::new(&data);
    assert_eq!(reader.read_u64(64).unwrap(), 0x8877665544332211);
}

#[test]
/// Read a 64-bit sequence after consuming leading bits.
fn test_read_max_stressed() {
    let data = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99];
    let mut reader = BitReader::new(&data);
    assert_eq!(reader.read_u8(8).unwrap(), 0x11);
    assert_eq!(reader.read_u64(64).unwrap(), 0x9988776655443322);
}

#[test]
/// Mix partially aligned reads up to the expected overflow.
fn test_read_and_out() {
    let data = [0x11, 0x22];
    let mut reader = BitReader::new(&data);
    assert_eq!(reader.read_u8(7).unwrap(), 0b0010001);
    assert_eq!(reader.read_u16(9).unwrap(), 0b001000100);
    assert!(matches!(
        reader.read_u8(2),
        Err(BitReaderError::OutOfBounds {
            asked: 2,
            available: 0
        })
    ))
}

#[test]
/// Read single bits at various positions to validate the cursor.
fn test_read_min() {
    let data = [0xAA, 0xAA, 0xAA, 0xAA];
    let mut reader = BitReader::new(&data);
    reader.bit_cursor = 0;
    assert_eq!(reader.read_u32(1).unwrap(), 0);
    reader.bit_cursor = 8;
    assert_eq!(reader.read_u32(1).unwrap(), 0);
    reader.bit_cursor = 31;
    assert_eq!(reader.read_u32(1).unwrap(), 1);
}

#[test]
/// Reading from an empty buffer must fail immediately.
fn test_read_empty_buffer() {
    let data: [u8; 0] = [];
    let mut reader = BitReader::new(&data);
    assert!(matches!(
        reader.read_u8(1),
        Err(BitReaderError::OutOfBounds {
            asked: 1,
            available: 0
        })
    ))
}

#[test]
/// Advance the cursor then perform a nominal read.
fn test_read_advance_cursor() {
    let data: [u8; 2] = [0xFF, 0xAF];
    // 1010_1111 1111_1111
    let mut reader = BitReader::new(&data);
    assert!(reader.advance(12).is_ok());
    assert_eq!(reader.read_u16(4).unwrap(), 0b1010);
}

#[test]
/// Validate overflow detection after a valid advance.
fn test_read_out_of_bounds_advance_cursor() {
    let data: [u8; 2] = [0xFF, 0xFF];
    let mut reader = BitReader::new(&data);
    assert!(reader.advance(13).is_ok());
    assert!(matches!(
        reader.read_u16(4),
        Err(BitReaderError::OutOfBounds {
            asked: 4,
            available: 3
        })
    ));
}

#[test]
/// Refuses to advance beyond the available buffer.
fn test_read_advance_bigger_than_buffer() {
    let data: [u8; 2] = [0xFF, 0xFF];
    let mut reader = BitReader::new(&data);
    assert!(matches!(
        reader.advance(17),
        Err(BitReaderError::OutOfBounds {
            asked: 17,
            available: 16
        })
    ));
}

#[test]
/// Extract a fully aligned slice.
fn test_read_complete_slice() {
    let data = [0xFF, 0xAF, 0xE2, 0xF1, 0xBC];
    let mut reader = BitReader::new(&data);
    assert_eq!(
        reader.read_slice(data.len()).unwrap(),
        &[0xFF, 0xAF, 0xE2, 0xF1, 0xBC]
    );
    reader.bit_cursor = 0;
    assert_ne!(
        reader.read_slice(data.len()).unwrap(),
        &[0xFF, 0xFF, 0xE2, 0xF1, 0xBC]
    );
    reader.bit_cursor = 0;
    assert_ne!(
        reader.read_slice(data.len()).unwrap(),
        &[0xFF, 0xFF, 0xFF, 0xFF, 0xFF]
    );
    reader.bit_cursor = 0;
    assert_eq!(
        reader.read_slice(data.len()).unwrap(),
        &[0xFF, 0xAF, 0xE2, 0xF1, 0xBC]
    );
}

#[test]
/// Extract a smaller aligned slice.
fn test_read_partial_slice() {
    let data = [0xFF, 0xAF, 0xE2, 0xF1, 0xBC];
    let mut reader = BitReader::new(&data);
    assert_eq!(reader.read_slice(3).unwrap(), &[0xFF, 0xAF, 0xE2]);
}

#[test]
/// Reading an overly long slice triggers `OutOfBounds`.
fn test_read_out_of_bound_slice() {
    let data = [0xFF, 0xAF, 0xE2, 0xF1, 0xBC];
    let mut reader = BitReader::new(&data);
    assert!(matches!(
        reader.read_slice(data.len() + 1).unwrap_err(),
        BitReaderError::OutOfBounds {
            asked: 6,
            available: 5
        }
    ));
}

#[test]
/// Reading a slice while the cursor is misaligned must fail.
fn test_read_non_aligned_slice() {
    let data = [0xFF, 0xAF, 0xE2, 0xF1, 0xBC];
    let mut reader = BitReader::new(&data);
    reader.bit_cursor = 4;
    assert!(matches!(
        reader.read_slice(4).unwrap_err(),
        BitReaderError::NonAlignedBit { cursor: 4 }
    ));
}
//==================================================================================TEST_BITWRITER

#[test]
/// Aligned write of a full byte.
fn test_write_aligned_bytes() {
    let mut buffer = [0xEF, 0xBE];
    let value = [0xDE];
    let mut writer = BitWriter::new(&mut buffer);
    assert!(writer.write_u64(value[0], 8).is_ok());
    assert_eq!(buffer, [0xDE, 0xBE]);
}

#[test]
/// Write a 16-bit field starting at an offset.
fn test_write_non_aligned_bytes() {
    let mut buffer = [0xFF, 0xFF, 0xFF];
    let value = [0b11100000, 0b00001100];
    let mut writer = BitWriter::new(&mut buffer);
    writer.bit_cursor = 4;
    assert!(writer
        .write_u64(u16::from_le_bytes(value) as u64, 16)
        .is_ok());
    assert_eq!(buffer, [0x0F, 0xCE, 0xF0])
}

#[test]
/// Writing too many bits after an offset must fail.
fn test_write_and_out() {
    let mut buffer = [0xFF, 0xFF, 0xFF];
    // 1111_1111 1111_1111 1111_1111
    // ****_**** ****_**** 1101_1010 1111_1010
    let value = [0xDA, 0xFA];
    let mut writer = BitWriter::new(&mut buffer);
    writer.bit_cursor = 16;
    assert!(matches!(
        writer.write_u64(u16::from_le_bytes(value) as u64, 16),
        Err(BitWriterError::OutOfBounds {
            asked: 16,
            available: 8
        })
    ));
}

#[test]
/// Write two consecutive bytes from a non-zero cursor.
fn test_write_multiples_bytes() {
    let mut buffer = [0xFF, 0xFF, 0xFF, 0xFF];
    let value = [0xDA, 0xFA];
    let mut writer = BitWriter::new(&mut buffer);
    writer.bit_cursor = 8;
    assert!(writer
        .write_u64(u16::from_le_bytes(value) as u64, 16)
        .is_ok());
    assert_eq!(buffer, [0xFF, 0xDA, 0xFA, 0xFF]);
}

#[test]
/// Validate maximum bit lengths for writer helpers.
fn test_write_num_bit_too_high() {
    let mut buffer = [0xFF, 0xFF];
    let value = 0b0000_0000_0000;
    let mut writer = BitWriter::new(&mut buffer);
    assert!(matches!(
        writer.write_u8(value as u8, 9).unwrap_err(),
        BitWriterError::TooLongForType { max: 8, asked: 9 }
    ));
    assert!(matches!(
        writer.write_u16(value as u16, 17).unwrap_err(),
        BitWriterError::TooLongForType { max: 16, asked: 17 }
    ));
    assert!(matches!(
        writer.write_u32(value, 33).unwrap_err(),
        BitWriterError::TooLongForType { max: 32, asked: 33 }
    ));
    assert!(matches!(
        writer.write_u64(value as u64, 65).unwrap_err(),
        BitWriterError::TooLongForType { max: 64, asked: 65 }
    ));
}

#[test]
/// Rewrite two entire bytes.
fn test_write_max() {
    let mut buffer = [0xFF, 0xFF];
    let value = [0xDA, 0xFA];
    let mut writer = BitWriter::new(&mut buffer);
    assert!(writer
        .write_u64(u16::from_le_bytes(value) as u64, 16)
        .is_ok());
    assert_eq!(buffer, [0xDA, 0xFA]);
}

#[test]
/// Write 64 bits while keeping sentinel bytes untouched.
fn test_write_max_writing_stressed() {
    let mut buffer = [0xFF; 10];
    let value = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88];
    let mut writer = BitWriter::new(&mut buffer);
    writer.bit_cursor = 8;
    assert!(writer.write_u64(u64::from_le_bytes(value), 64).is_ok());
    assert_eq!(
        buffer,
        [0xFF, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0xFF]
    );
}

#[test]
/// Write a single bit in the middle of the buffer.
fn test_write_min() {
    let mut buffer = [0xFF, 0xEF, 0xFE]; // 1111_1111 1110_1111 1111_1110
    let value = 0b1;
    let mut writer = BitWriter::new(&mut buffer);
    writer.bit_cursor = 12;
    assert!(writer.write_u8(value, 1).is_ok());
    assert_eq!(buffer, [0xFF, 0xFF, 0xFE]);
}

#[test]
/// Writing into an empty buffer triggers `OutOfBounds`.
fn test_write_empty_buffer() {
    let mut buffer = [];
    let value = 0xFF;
    let mut writer = BitWriter::new(&mut buffer);
    assert!(matches!(
        writer.write_u8(value, 8),
        Err(BitWriterError::OutOfBounds {
            asked: 8,
            available: 0
        })
    ))
}

#[test]
/// Copy an aligned slice over the entire buffer.
fn test_write_complete_slice() {
    let slice = [0xDF, 0xCF, 0xE2, 0xC1, 0xBA];
    let mut buffer = [0x00; 5];
    let mut writer = BitWriter::new(&mut buffer);
    assert!(writer.write_slice(&slice).is_ok());
    assert_ne!(&buffer, &[0x00; 5]);
    assert_ne!(&buffer, &[0xFF; 5]);
    assert_ne!(&buffer, &[0xDF, 0xCF, 0xD2, 0xC1, 0xBA]);
    assert_eq!(&buffer, &slice);
}

#[test]
/// Copy a slice smaller than the destination buffer.
fn test_write_partial_slice() {
    let slice = [0xDF, 0xCF, 0xE2, 0xC1, 0xBA];
    let mut buffer = [0x00; 10];
    let mut writer = BitWriter::new(&mut buffer);
    assert!(writer.write_slice(&slice).is_ok());
    assert_eq!(
        &buffer,
        &[0xDF, 0xCF, 0xE2, 0xC1, 0xBA, 0x00, 0x00, 0x00, 0x00, 0x00]
    );
}

#[test]
/// Detect overflow when copying a slice that is too long.
fn test_write_out_of_bound_slice() {
    let slice = [0xFF, 0xAF, 0xE2, 0xF1, 0xBC, 0xFF];
    let mut buffer = [0x00; 5];
    let mut writer = BitWriter::new(&mut buffer);
    assert!(matches!(
        writer.write_slice(&slice).unwrap_err(),
        BitWriterError::OutOfBounds {
            asked: 6,
            available: 5
        }
    ));
}

#[test]
/// Writing a slice while the cursor is not byte aligned is forbidden.
fn test_write_non_aligned_slice() {
    let slice = [0xFF, 0xAF, 0xE2, 0xF1, 0xBC];
    let mut buffer = [0x00; 5];
    let mut writer = BitWriter::new(&mut buffer);
    writer.bit_cursor = 4;
    assert!(matches!(
        writer.write_slice(&slice).unwrap_err(),
        BitWriterError::NonAlignedBit { cursor: 4 }
    ));
}
