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

pub struct RunLengthDecode;

impl RunLengthDecode {
    pub fn new() -> Self {
        RunLengthDecode
    }
}

impl Filter for RunLengthDecode {
    fn decode(&self, data: &[u8]) -> Result<Vec<u8>, PdfError> {
        let mut result = Vec::new();
        let mut i = 0;

        while i < data.len() {
            let length = data[i] as i8;
            i += 1;

            if length >= 0 {
                // Copy next (length + 1) bytes literally
                let count = length as usize + 1;
                if i + count > data.len() {
                    return Err(PdfError::InvalidData("Run length exceeds data".into()));
                }
                result.extend_from_slice(&data[i..i + count]);
                i += count;
            } else if length != -128 {
                // Repeat next byte (-length + 1) times
                if i >= data.len() {
                    return Err(PdfError::InvalidData("Missing run byte".into()));
                }
                let count = (-length as usize) + 1;
                let byte = data[i];
                result.extend(std::iter::repeat(byte).take(count));
                i += 1;
            }
            // length == -128 is ignored (EOD marker)
        }

        Ok(result)
    }

    fn encode(&self, data: &[u8]) -> Result<Vec<u8>, PdfError> {
        let mut result = Vec::new();
        let mut i = 0;

        while i < data.len() {
            // Look for runs of identical bytes
            let mut run_length = 1;
            while i + run_length < data.len() 
                && run_length < 128 
                && data[i] == data[i + run_length] 
            {
                run_length += 1;
            }

            if run_length > 1 {
                // Encode run
                result.push((-run_length as i8 + 1) as u8);
                result.push(data[i]);
                i += run_length;
                continue;
            }

            // Look for literal run
            let mut literal_length = 1;
            while i + literal_length < data.len() 
                && literal_length < 128 
                && (literal_length < 2 
                    || data[i + literal_length] != data[i + literal_length - 1]) 
            {
                literal_length += 1;
            }

            // Encode literal run
            result.push((literal_length - 1) as u8);
            result.extend_from_slice(&data[i..i + literal_length]);
            i += literal_length;
        }

        // Add EOD marker
        result.push(128);
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_length_decode() {
        let filter = RunLengthDecode::new();
        
        // Test literal run
        let input = vec![2, b'a', b'b', b'c'];
        let expected = vec![b'a', b'b', b'c'];
        let result = filter.decode(&input).unwrap();
        assert_eq!(result, expected);

        // Test repeated run
        let input = vec![254, b'x'];  // Run of 3 'x' characters
        let expected = vec![b'x', b'x', b'x'];
        let result = filter.decode(&input).unwrap();
        assert_eq!(result, expected);

        // Test combined
        let input = vec![2, b'a', b'b', b'c', 254, b'x', 128];
        let expected = vec![b'a', b'b', b'c', b'x', b'x', b'x'];
        let result = filter.decode(&input).unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_run_length_encode_decode() {
        let filter = RunLengthDecode::new();
        let input = b"WWWWWWWWWABCDEFGHIJKLMNOP".to_vec();
        
        let encoded = filter.encode(&input).unwrap();
        let decoded = filter.decode(&encoded).unwrap();
        
        assert_eq!(decoded, input);
    }

    #[test]
    fn test_run_length_invalid() {
        let filter = RunLengthDecode::new();
        
        // Test truncated literal run
        let input = vec![2, b'a'];
        assert!(filter.decode(&input).is_err());

        // Test truncated repeat run
        let input = vec![254];
        assert!(filter.decode(&input).is_err());
    }
}
