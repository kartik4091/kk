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
use super::buffer::BinaryBuffer;
use crate::core::error::PdfError;

pub struct StreamReader<R: Read + Seek> {
    reader: R,
    buffer: BinaryBuffer,
    stream_position: u64,
    stream_length: u64,
}

impl<R: Read + Seek> StreamReader<R> {
    pub fn new(mut reader: R) -> Result<Self, PdfError> {
        let stream_length = reader.seek(SeekFrom::End(0))?;
        reader.seek(SeekFrom::Start(0))?;

        Ok(Self {
            reader,
            buffer: BinaryBuffer::new(),
            stream_position: 0,
            stream_length,
        })
    }

    pub fn read_bytes(&mut self, count: usize) -> Result<Vec<u8>, PdfError> {
        let mut buf = vec![0; count];
        self.read_exact(&mut buf)?;
        Ok(buf)
    }

    pub fn peek_bytes(&mut self, count: usize) -> Result<Vec<u8>, PdfError> {
        let current_pos = self.stream_position;
        let bytes = self.read_bytes(count)?;
        self.seek(SeekFrom::Start(current_pos))?;
        Ok(bytes)
    }

    pub fn read_until(&mut self, delimiter: u8) -> Result<Vec<u8>, PdfError> {
        let data = self.buffer.read_until(&mut self.reader, delimiter)?;
        self.stream_position += data.len() as u64;
        Ok(data.to_vec())
    }

    pub fn skip(&mut self, count: usize) -> Result<(), PdfError> {
        self.buffer.skip(&mut self.reader, count)?;
        self.stream_position += count as u64;
        Ok(())
    }

    pub fn seek(&mut self, pos: SeekFrom) -> Result<u64, PdfError> {
        self.buffer.reset();
        self.stream_position = self.reader.seek(pos)?;
        Ok(self.stream_position)
    }

    pub fn position(&self) -> u64 {
        self.stream_position
    }

    pub fn length(&self) -> u64 {
        self.stream_length
    }
}

impl<R: Read + Seek> Read for StreamReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let bytes_read = self.reader.read(buf)?;
        self.stream_position += bytes_read as u64;
        Ok(bytes_read)
    }
}

impl<R: Read + Seek> Seek for StreamReader<R> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.buffer.reset();
        self.stream_position = self.reader.seek(pos)?;
        Ok(self.stream_position)
    }
}
