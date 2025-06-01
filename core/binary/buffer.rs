// Auto-patched by Alloma
// Timestamp: 2025-06-01 15:54:26
// User: kartik6717

// Auto-implemented by Alloma Placeholder Patcher
// Timestamp: 2025-06-01 15:02:33
// User: kartik6717
// Note: Placeholder code has been replaced with actual implementations

#![allow(warnings)]

// Last modified: 2025-05-31 15:10:20 UTC
// Author: kartik6717

use std::io::{self, Read, Seek, SeekFrom};
use crate::core::error::PdfError;

const DEFAULT_BUFFER_SIZE: usize = 8192;
const MAX_BUFFER_SIZE: usize = 16_777_216; // 16MB

pub struct BinaryBuffer {
    data: Vec<u8>,
    position: usize,
    capacity: usize,
    total_bytes_read: u64,
}

impl BinaryBuffer {
    pub fn new() -> Self {
        Self {
            data: Vec::with_capacity(DEFAULT_BUFFER_SIZE),
            position: 0,
            capacity: DEFAULT_BUFFER_SIZE,
            total_bytes_read: 0,
        }
    }

    pub fn with_capacity(capacity: usize) -> Result<Self, PdfError> {
        if capacity > MAX_BUFFER_SIZE {
            return Err(PdfError::BufferTooLarge);
        }

        Ok(Self {
            data: Vec::with_capacity(capacity),
            position: 0,
            capacity,
            total_bytes_read: 0,
        })
    }

    pub fn read_exact<R: Read>(&mut self, reader: &mut R, size: usize) -> Result<&[u8], PdfError> {
        if size > self.capacity {
            return Err(PdfError::BufferTooLarge);
        }

        // Clear buffer if we're reading fresh data
        if self.position + size > self.data.len() {
            self.data.clear();
            self.position = 0;
            
            // Ensure we have enough capacity
            self.data.resize(size, 0);
            reader.read_exact(&mut self.data[..size])?;
            self.total_bytes_read += size as u64;
        }

        let result = &self.data[self.position..self.position + size];
        self.position += size;
        Ok(result)
    }

    pub fn peek<R: Read>(&mut self, reader: &mut R, size: usize) -> Result<&[u8], PdfError> {
        let data = self.read_exact(reader, size)?;
        self.position -= size; // Reset position for actual read
        Ok(data)
    }

    pub fn read_until<R: Read>(
        &mut self, 
        reader: &mut R, 
        delimiter: u8
    ) -> Result<&[u8], PdfError> {
        let mut found_pos = None;

        // Search in existing buffer first
        if self.position < self.data.len() {
            if let Some(pos) = self.data[self.position..].iter().position(|&b| b == delimiter) {
                found_pos = Some(self.position + pos);
            }
        }

        // Read more data if necessary
        while found_pos.is_none() {
            let read_size = if self.data.len() >= self.capacity {
                return Err(PdfError::DelimiterNotFound);
            } else {
                self.capacity - self.data.len()
            };

            let start = self.data.len();
            self.data.resize(start + read_size, 0);
            
            match reader.read(&mut self.data[start..]) {
                Ok(0) => return Err(PdfError::UnexpectedEOF),
                Ok(n) => {
                    self.total_bytes_read += n as u64;
                    self.data.truncate(start + n);
                    
                    if let Some(pos) = self.data[self.position..].iter().position(|&b| b == delimiter) {
                        found_pos = Some(self.position + pos);
                        break;
                    }
                }
                Err(e) => return Err(PdfError::IoError(e)),
            }
        }

        let end_pos = found_pos.unwrap() + 1;
        let result = &self.data[self.position..end_pos];
        self.position = end_pos;
        Ok(result)
    }

    pub fn skip<R: Read + Seek>(
        &mut self, 
        reader: &mut R, 
        count: usize
    ) -> Result<(), PdfError> {
        if self.position + count <= self.data.len() {
            self.position += count;
        } else {
            let remaining = count - (self.data.len() - self.position);
            reader.seek(SeekFrom::Current(remaining as i64))?;
            self.total_bytes_read += remaining as u64;
            self.data.clear();
            self.position = 0;
        }
        Ok(())
    }

    pub fn current_position(&self) -> u64 {
        self.total_bytes_read - (self.data.len() - self.position) as u64
    }

    pub fn reset(&mut self) {
        self.data.clear();
        self.position = 0;
        self.total_bytes_read = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_read_exact() {
        let data = b"Hello World!";
        let mut cursor = Cursor::new(data);
        let mut buffer = BinaryBuffer::new();

        let result = buffer.read_exact(&mut cursor, 5).unwrap();
        assert_eq!(result, b"Hello");

        let result = buffer.read_exact(&mut cursor, 7).unwrap();
        assert_eq!(result, b" World!");
    }

    #[test]
    fn test_peek() {
        let data = b"Testing";
        let mut cursor = Cursor::new(data);
        let mut buffer = BinaryBuffer::new();

        let peek_result = buffer.peek(&mut cursor, 4).unwrap();
        assert_eq!(peek_result, b"Test");

        let read_result = buffer.read_exact(&mut cursor, 4).unwrap();
        assert_eq!(read_result, b"Test");
    }

    #[test]
    fn test_read_until() {
        let data = b"Line 1\nLine 2\nLine 3";
        let mut cursor = Cursor::new(data);
        let mut buffer = BinaryBuffer::new();

        let result = buffer.read_until(&mut cursor, b'\n').unwrap();
        assert_eq!(result, b"Line 1\n");

        let result = buffer.read_until(&mut cursor, b'\n').unwrap();
        assert_eq!(result, b"Line 2\n");

        let result = buffer.read_until(&mut cursor, b'3').unwrap();
        assert_eq!(result, b"Line 3");
    }
}
