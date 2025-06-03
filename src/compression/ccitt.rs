// Auto-patched by Alloma
// Timestamp: 2025-06-01 15:54:26
// User: kartik6717

// Auto-implemented by Alloma Placeholder Patcher
// Timestamp: 2025-06-01 15:02:33
// User: kartik6717
// Note: Placeholder code has been replaced with actual implementations

#![allow(warnings)]

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

    fn write_bits(&mut self, value: u16, count: u8) -> Result<(), PdfError> {
        for i in (0..count).rev() {
            let bit = (value >> i) & 1;
            self.current_byte = (self.current_byte << 1) | (bit as u8);
            self.bit_pos += 1;

            if self.bit_pos == 8 {
                self.data.push(self.current_byte);
                self.current_byte = 0;
                self.bit_pos = 0;
            }
        }
        Ok(())
    }

    fn write_code(&mut self, code: u16) -> Result<(), PdfError> {
        self.write_bits(code, 16)
    }

    fn align_to_byte(&mut self) {
        if self.bit_pos > 0 {
            self.current_byte <<= (8 - self.bit_pos);
            self.data.push(self.current_byte);
            self.current_byte = 0;
            self.bit_pos = 0;
        }
    }

    fn finish(mut self) -> Vec<u8> {
        if self.bit_pos > 0 {
            self.align_to_byte();
        }
        self.data
    }
}
