//! Low-level components dedicated to bit manipulation for CAN buffers.
//! The provided reader/writer abstractions are optimized for NMEA 2000 payloads
//! where fields seldom align with byte boundaries.
use crate::error::{BitReaderError, BitWriterError};

/// Generic reader that extracts bit segments from a `&[u8]`
/// without extra allocation or copies.
pub struct BitReader<'a> {
    /// Shared source buffer (typically the received CAN frame).
    buffer: &'a [u8],
    /// Current index expressed as number of bits read from the beginning.
    bit_cursor: usize,
}

impl<'a> BitReader<'a> {
    /// Create a reader positioned at the start of the provided buffer.
    pub fn new(buffer: &'a [u8]) -> Self {
        Self {
            buffer,
            bit_cursor: 0,
        }
    }

    /// Read `num_bits` bits starting at the current cursor and return a `u64`.
    /// `num_bits` must stay in the [1, 64] range.
    pub fn read_u64(&mut self, num_bits: u8) -> Result<u64, BitReaderError> {
        // Validate admissible bit length.
        if !(1..=64).contains(&num_bits) {
            return Err(BitReaderError::TooLongForType {
                max: 64,
                asked: num_bits,
            });
        }

        let buffer_len_bits = self.buffer.len() * 8;
        let read_end_bit = self.bit_cursor + num_bits as usize;

        // Prevent reading beyond the buffer.
        if read_end_bit > buffer_len_bits {
            return Err(BitReaderError::OutOfBounds {
                asked: num_bits as usize,
                available: buffer_len_bits - self.bit_cursor,
            });
        }
        // Assemble the requested bits.
        let mut result: u64 = 0;
        let mut bits_read = 0;

        while bits_read < num_bits {
            let current_byte_index = (self.bit_cursor + bits_read as usize) / 8;
            let current_bit_offset = (self.bit_cursor + bits_read as usize) % 8;

            // `byte` is the byte currently in use
            let byte = self.buffer[current_byte_index];

            // Number of bits available within the current byte.
            let bits_to_read_this_iteration =
                (8 - current_bit_offset).min(num_bits as usize - bits_read as usize);

            // Extract only the relevant bits.
            let mask = ((1u16 << bits_to_read_this_iteration) - 1) as u8;
            let masked_value = (byte >> current_bit_offset) & mask;

            // Merge bits into the output value while preserving ordering.
            result |= (masked_value as u64) << bits_read;

            bits_read += bits_to_read_this_iteration as u8;
        }
        // Update cursor once the read is complete.
        self.bit_cursor += num_bits as usize;
        Ok(result)
    }

    /// Read up to 8 bits and return a `u8`.
    pub fn read_u8(&mut self, num_bits: u8) -> Result<u8, BitReaderError> {
        if num_bits > 8 {
            return Err(BitReaderError::TooLongForType {
                max: 8,
                asked: num_bits,
            });
        }

        self.read_u64(num_bits).map(|val| val as u8)
    }

    /// Read up to 16 bits and return a `u16`.
    pub fn read_u16(&mut self, num_bits: u8) -> Result<u16, BitReaderError> {
        if num_bits > 16 {
            return Err(BitReaderError::TooLongForType {
                max: 16,
                asked: num_bits,
            });
        }

        self.read_u64(num_bits).map(|val| val as u16)
    }

    /// Read up to 32 bits and return a `u32`.
    pub fn read_u32(&mut self, num_bits: u8) -> Result<u32, BitReaderError> {
        if num_bits > 32 {
            return Err(BitReaderError::TooLongForType {
                max: 32,
                asked: num_bits,
            });
        }

        self.read_u64(num_bits).map(|val| val as u32)
    }

    /// Advance the cursor by `length` bits without reading data.
    pub fn advance(&mut self, length: u8) -> Result<(), BitReaderError> {
        // Validate admissible length.
        if !(1..=64).contains(&length) {
            return Err(BitReaderError::TooLongForType {
                max: 64,
                asked: length,
            });
        }

        let buffer_len_bits = self.buffer.len() * 8;
        let new_cursor_pos = self.bit_cursor + length as usize;

        if new_cursor_pos > buffer_len_bits {
            return Err(BitReaderError::OutOfBounds {
                asked: length as usize,
                available: buffer_len_bits - self.bit_cursor,
            });
        }
        self.bit_cursor = new_cursor_pos;

        Ok(())
    }

    /// Return a slice of `len` bytes from the current position.
    /// Cursor must be aligned on an octet boundary.
    pub fn read_slice(&mut self, len: usize) -> Result<&'a [u8], BitReaderError> {
        // Slices are only allowed when aligned.
        if self.bit_cursor % 8 != 0 {
            return Err(BitReaderError::NonAlignedBit {
                cursor: self.bit_cursor,
            });
        }

        let byte_start = self.bit_cursor / 8;
        let byte_end = byte_start + len;
        if byte_end > self.buffer.len() {
            return Err(BitReaderError::OutOfBounds {
                asked: byte_end,
                available: self.buffer.len(),
            });
        }
        let slice = &self.buffer[byte_start..byte_end];
        self.bit_cursor += len * 8;
        Ok(slice)
    }
}
//==================================================================================BITWRITER

/// Generic writer able to lay bit segments into a `&mut [u8]`
/// without assuming byte alignment. Used by the serialization layer to rebuild
/// NMEA 2000 payloads field by field.
pub struct BitWriter<'a> {
    /// Target buffer (typically the CAN frame under construction).
    buffer: &'a mut [u8],
    /// Current position expressed in bits written.
    bit_cursor: usize,
}

impl<'a> BitWriter<'a> {
    /// Create a writer positioned at the start of the buffer.
    pub fn new(buffer: &'a mut [u8]) -> Self {
        Self {
            buffer,
            bit_cursor: 0,
        }
    }

    /// Expose the cursor position in bits (useful to derive final length).
    pub fn bit_cursor(&self) -> usize {
        self.bit_cursor
    }

    /// Write `num_bits` bits from the provided `u64`.
    pub fn write_u64(&mut self, value: u64, num_bits: u8) -> Result<(), BitWriterError> {
        if !(1..=64).contains(&num_bits) {
            return Err(BitWriterError::TooLongForType {
                max: 64,
                asked: num_bits,
            });
        }

        let buffer_len_bits = self.buffer.len() * 8;
        let write_end_bit = self.bit_cursor + num_bits as usize;

        if write_end_bit > buffer_len_bits {
            return Err(BitWriterError::OutOfBounds {
                asked: num_bits as usize,
                available: buffer_len_bits - self.bit_cursor,
            });
        }

        let mut val_to_write = value;
        let mut bits_write = 0;

        while bits_write < num_bits {
            let current_byte_index = (self.bit_cursor + bits_write as usize) / 8;
            let current_bit_offset = (self.bit_cursor + bits_write as usize) % 8;

            // Number of bits available in the current byte.
            let bits_to_write_this_iteration =
                (8 - current_bit_offset).min(num_bits as usize - bits_write as usize);

            // Update only the relevant bits.
            let mask = ((1u16 << bits_to_write_this_iteration) - 1) as u8;
            self.buffer[current_byte_index] &= !(mask << current_bit_offset);

            self.buffer[current_byte_index] |= (val_to_write as u8 & mask) << current_bit_offset;
            val_to_write >>= bits_to_write_this_iteration;

            bits_write += bits_to_write_this_iteration as u8;
        }

        self.bit_cursor += num_bits as usize;

        Ok(())
    }

    /// Convenience helper to write up to 8 bits.
    pub fn write_u8(&mut self, value: u8, num_bits: u8) -> Result<(), BitWriterError> {
        if num_bits > 8 {
            return Err(BitWriterError::TooLongForType {
                max: 8,
                asked: num_bits,
            });
        }
        self.write_u64(value as u64, num_bits)
    }

    /// Convenience helper to write up to 16 bits.
    pub fn write_u16(&mut self, value: u16, num_bits: u8) -> Result<(), BitWriterError> {
        if num_bits > 16 {
            return Err(BitWriterError::TooLongForType {
                max: 16,
                asked: num_bits,
            });
        }
        self.write_u64(value as u64, num_bits)
    }

    /// Convenience helper to write up to 32 bits.
    pub fn write_u32(&mut self, value: u32, num_bits: u8) -> Result<(), BitWriterError> {
        if num_bits > 32 {
            return Err(BitWriterError::TooLongForType {
                max: 32,
                asked: num_bits,
            });
        }
        self.write_u64(value as u64, num_bits)
    }
    /// Advance the cursor without writing (used for reserved fields).
    pub fn advance(&mut self, length: u8) -> Result<(), BitWriterError> {
        // Validate admissible length.
        if !(1..=64).contains(&length) {
            return Err(BitWriterError::TooLongForType {
                max: 64,
                asked: length,
            });
        }

        let buffer_len_bits = self.buffer.len() * 8;
        let new_cursor_pos = self.bit_cursor + length as usize;

        if new_cursor_pos > buffer_len_bits {
            return Err(BitWriterError::OutOfBounds {
                asked: length as usize,
                available: buffer_len_bits - self.bit_cursor,
            });
        }
        self.bit_cursor = new_cursor_pos;

        Ok(())
    }

    /// Copy an already-aligned byte slice into the buffer.
    pub fn write_slice(&mut self, slice: &[u8]) -> Result<(), BitWriterError> {
        if self.bit_cursor % 8 != 0 {
            return Err(BitWriterError::NonAlignedBit {
                cursor: self.bit_cursor,
            });
        }
        let byte_start = self.bit_cursor / 8;
        let byte_end = byte_start + slice.len();
        if byte_end > self.buffer.len() {
            return Err(BitWriterError::OutOfBounds {
                asked: byte_end,
                available: self.buffer.len(),
            });
        }
        self.buffer[byte_start..byte_end].copy_from_slice(slice);
        self.bit_cursor += slice.len() * 8;
        Ok(())
    }
}

//==================================================================================TEST_BITREADER
#[cfg(test)]
#[path = "tests.rs"]
mod tests;
