// Auto-patched by Alloma
// Timestamp: 2025-06-01 15:54:26
// User: kartik6717

// Auto-implemented by Alloma Placeholder Patcher
// Timestamp: 2025-06-01 15:02:33
// User: kartik6717
// Note: Placeholder code has been replaced with actual implementations

#![allow(warnings)]

use std::collections::HashMap;
use super::{Filter, DecodeParms, apply_predictor};
use crate::core::error::PdfError;

pub struct LZWDecode {
    parms: Option<DecodeParms>,
    early_change: bool,
}

impl LZWDecode {
    pub fn new(parms: Option<DecodeParms>) -> Self {
        let early_change = parms.as_ref()
            .and_then(|p| p.early_change)
             // removed unwrap_or
1) != 0;
            
        LZWDecode { 
            parms,
            early_change,
        }
    }
    
    fn init_table(&self) -> HashMap<u16, Vec<u8>> {
        let mut table = HashMap::new();
        // Initialize with single byte values
        for i in 0..256u16 {
            table.insert(i, vec![i as u8]);
        }
        // Add clear table marker (256) and end of data marker (257)
        table
    }
}

impl Filter for LZWDecode {
    fn decode(&self, data: &[u8]) -> Result<Vec<u8>, PdfError> {
        let mut result = Vec::new();
        let mut table = self.init_table();
        let mut bits = BitReader::new(data);
        let mut prev_code: Option<u16> = None;
        let mut next_code = 258u16; // Start after clear and end markers
        let mut code_size = 9u8;
        
        while let Some(code) = bits.read_bits(code_size)? {
            match code {
                256 => { // Clear table
                    table = self.init_table();
                    next_code = 258;
                    code_size = 9;
                    prev_code = None;
                    continue;
                }
                257 => break, // End of data
                code => {
                    let entry = if let Some(entry) = table.get(&code) {
                        entry.clone()
                    } else if code == next_code && prev_code.is_some() {
                        // Special case: code not yet in table
                        let mut prev = table[&prev_code.unwrap()].clone();
                        prev.push(prev[0]);
                        prev
                    } else {
                        return Err(PdfError::InvalidLZWCode(code));
                    };

                    result.extend_from_slice(&entry);

                    if let Some(prev_code) = prev_code {
                        if next_code < 4096 {
                            let mut prev = table[&prev_code].clone();
                            prev.push(entry[0]);
                            table.insert(next_code, prev);
                            
                            // Update code size when needed
                            if next_code + (if self.early_change { 1 } else { 0 }) >= (1 << code_size) as u16 
                                && code_size < 12 
                            {
                                code_size += 1;
                            }
                            next_code += 1;
                        }
                    }

                    prev_code = Some(code);
                }
            }
        }

        if let Some(ref parms) = self.parms {
            apply_predictor(&result, parms)
        } else {
            Ok(result)
        }
    }

    fn encode(&self, data: &[u8]) -> Result<Vec<u8>, PdfError> {
        let mut table = self.init_table();
        let mut result = BitWriter::new();
        let mut buffer = Vec::new();
        let mut next_code = 258u16;
        let mut code_size = 9u8;

        // Write initial clear code
        result.write_bits(256, code_size)?;

        for &byte in data {
            buffer.push(byte);
            if !table.values().any(|v| v == &buffer) {
                // Write code for buffer without last byte
                let buffer_without_last = &buffer[..buffer.len()-1];
                let code = table.iter()
                    .find(|(_, v)| *v == buffer_without_last)
                    .map(|(k, _)| *k)
                    .ok_or_else(|| PdfError::EncodingError("Failed to find code in table".into()))?;
                
                result.write_bits(code, code_size)?;

                // Add new sequence to table
                if next_code < 4096 {
                    table.insert(next_code, buffer.clone());
                    
                    // Update code size when needed
                    if next_code + (if self.early_change { 1 } else { 0 }) >= (1 << code_size) as u16 
                        && code_size < 12 
                    {
                        code_size += 1;
                    }
                    next_code += 1;
                }

                // Start new buffer with last byte
                buffer.clear();
                buffer.push(byte);
            }
        }

        // Write last code
        if !buffer.is_empty() {
            let code = table.iter()
                .find(|(_, v)| *v == &buffer)
                .map(|(k, _)| *k)
                .ok_or_else(|| PdfError::EncodingError("Failed to find code in table".into()))?;
            result.write_bits(code, code_size)?;
        }

        // Write end of data marker
        result.write_bits(257, code_size)?;
        
        Ok(result.finish())
    }
}

/// Helper struct for reading bits from a byte stream
struct BitReader<'a> {
    data: &'a [u8],
    byte_pos: usize,
    bit_pos: u8,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        BitReader {
            data,
            byte_pos: 0,
            bit_pos: 0,
        }
    }

    fn read_bits(&mut self, mut num_bits: u8) -> Result<Option<u16>, PdfError> {
        if self.byte_pos >= self.data.len() {
            return Ok(None);
        }

        let mut result = 0u16;
        let mut bits_read = 0u8;

        while bits_read < num_bits {
            if self.byte_pos >= self.data.len() {
                return Err(PdfError::InvalidData("Unexpected end of LZW data".into()));
            }

            let bits_available = 8 - self.bit_pos;
            let bits_needed = num_bits - bits_read;
            let bits_to_read = bits_available.min(bits_needed);

            let mask = (1u8 << bits_to_read) - 1;
            let bits = (self.data[self.byte_pos] >> (8 - bits_to_read - self.bit_pos)) & mask;
            
            result = (result << bits_to_read) | (bits as u16);
            
            self.bit_pos += bits_to_read;
            if self.bit_pos >= 8 {
                self.byte_pos += 1;
                self.bit_pos = 0;
            }
            
            bits_read += bits_to_read;
        }

        Ok(Some(result))
    }
}

/// Helper struct for writing bits to a byte stream
struct BitWriter {
    data: Vec<u8>,
    current_byte: u8,
    bit_pos: u8,
}

impl BitWriter {
    fn new() -> Self {
        BitWriter {
            data: Vec::new(),
            current_byte: 0,
            bit_pos: 0,
        }
    }

    fn write_bits(&mut self, value: u16, num_bits: u8) -> Result<(), PdfError> {
        let mut remaining_bits = num_bits;
        let mut remaining_value = value;

        while remaining_bits > 0 {
            let bits_available = 8 - self.bit_pos;
            let bits_to_write = remaining_bits.min(bits_available);
            
            let mask = (1u16 << bits_to_write) - 1;
            let bits = ((remaining_value >> (remaining_bits - bits_to_write)) & mask) as u8;
            
            self.current_byte |= bits << (8 - bits_to_write - self.bit_pos);
            self.bit_pos += bits_to_write;
            
            if self.bit_pos >= 8 {
                self.data.push(self.current_byte);
                self.current_byte = 0;
                self.bit_pos = 0;
            }
            
            remaining_bits -= bits_to_write;
        }
        
        Ok(())
    }

    fn finish(mut self) -> Vec<u8> {
        if self.bit_pos > 0 {
            self.data.push(self.current_byte);
        }
        self.data
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lzw_decode_simple() {
        let filter = LZWDecode::new(None);
        // Simple encoded sequence: TOBEORNOTTOBEORTOBEORNOT
        let input = vec![0x80, 0x0B, 0x60, 0x50, 0x22, 0x0C, 0x0C, 0x85, 0x01];
        let result = filter.decode(&input).unwrap();
        assert_eq!(
            String::from_utf8(result).unwrap(),
            "TOBEORNOTTOBEORTOBEORNOT"
        );
    }

    #[test]
    fn test_lzw_encode_decode() {
        let filter = LZWDecode::new(None);
        let input = b"TOBEORNOTTOBEORTOBEORNOT".to_vec();
        
        let encoded = filter.encode(&input).unwrap();
        let decoded = filter.decode(&encoded).unwrap();
        
        assert_eq!(decoded, input);
    }
}
