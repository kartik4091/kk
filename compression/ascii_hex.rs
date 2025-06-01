// Auto-patched by Alloma
// Timestamp: 2025-06-01 15:54:26
// User: kartik6717

// Auto-implemented by Alloma Placeholder Patcher
// Timestamp: 2025-06-01 15:02:33
// User: kartik6717
// Note: Placeholder code has been replaced with actual implementations

#![allow(warnings)]

use super::Filter;
use crate::core::error::PdfError;

pub struct ASCIIHexDecode;

impl ASCIIHexDecode {
    pub fn new() -> Self {
        ASCIIHexDecode
    }

    fn hex_value(byte: u8) -> Result<u8, PdfError> {
        match byte {
            b'0'..=b'9' => Ok(byte - b'0'),
            b'A'..=b'F' => Ok(byte - b'A' + 10),
            b'a'..=b'f' => Ok(byte - b'a' + 10),
            _ => Err(PdfError::InvalidData(format!("Invalid hex character: {}", byte as char))),
        }
    }
}

impl Filter for ASCIIHexDecode {
    fn decode(&self, data: &[u8]) -> Result<Vec<u8>, PdfError> {
        let mut result = Vec::new();
        let mut high_nibble = true;
        let mut current_byte = 0u8;

        for &byte in data {
            match byte {
                b'>' => break,
                b'\n' | b'\r' | b'\t' | b' ' => continue,
                b => {
                    if high_nibble {
                        current_byte = Self::hex_value(b)? << 4;
                        high_nibble = false;
                    } else {
                        current_byte |= Self::hex_value(b)?;
                        result.push(current_byte);
                        high_nibble = true;
                    }
                }
            }
        }

        // Handle odd number of digits (last digit implicitly paired with 0)
        if !high_nibble {
            result.push(current_byte);
        }

        Ok(result)
    }

    fn encode(&self, data: &[u8]) -> Result<Vec<u8>, PdfError> {
        let mut result = Vec::with_capacity(data.len() * 2 + 2);
        let hex_chars = b"0123456789ABCDEF";

        for &byte in data {
            result.push(hex_chars[(byte >> 4) as usize]);
            result.push(hex_chars[(byte & 0xF) as usize]);
        }

        result.push(b'>');
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ascii_hex_decode() {
        let filter = ASCIIHexDecode::new();
        
        // Test basic decoding
        let input = b"48656C6C6F2C20576F726C6421>";
        let expected = b"Hello, World!";
        let result = filter.decode(input).unwrap();
        assert_eq!(result, expected);

        // Test with whitespace
        let input = b"48 65 6C 6C 6F\n2C 20 57 6F\t72 6C 64 21>";
        let result = filter.decode(input).unwrap();
        assert_eq!(result, expected);

        // Test odd number of digits
        let input = b"48656>";
        let expected = b"He";
        let result = filter.decode(input).unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_ascii_hex_encode_decode() {
        let filter = ASCIIHexDecode::new();
        let input = b"Hello, World!".to_vec();
        
        let encoded = filter.encode(&input).unwrap();
        let decoded = filter.decode(&encoded).unwrap();
        
        assert_eq!(decoded, input);
    }

    #[test]
    fn test_ascii_hex_invalid() {
        let filter = ASCIIHexDecode::new();
        
        // Test invalid hex character
        let input = b"4G>";
        assert!(filter.decode(input).is_err());
    }
}
