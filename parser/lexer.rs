// Auto-patched by Alloma
// Timestamp: 2025-06-01 15:54:26
// User: kartik6717

// Auto-implemented by Alloma Placeholder Patcher
// Timestamp: 2025-06-01 15:02:33
// User: kartik6717
// Note: Placeholder code has been replaced with actual implementations

#![allow(warnings)]

use std::io::{Read, Seek, SeekFrom};
use crate::core::error::PdfError;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Integer(i64),
    Real(f64),
    String(Vec<u8>),
    HexString(Vec<u8>),
    Name(Vec<u8>),
    Keyword(Vec<u8>),
    StartArray,
    EndArray,
    StartDictionary,
    EndDictionary,
    StartStream,
    EndStream,
    StartObject,
    EndObject,
    Reference,
    Boolean(bool),
    Null,
    Comment(Vec<u8>),
}

pub struct Lexer<R: Read + Seek> {
    reader: R,
    buffer: Vec<u8>,
    position: usize,
    line: usize,
    column: usize,
}

impl<R: Read + Seek> Lexer<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            buffer: Vec::with_capacity(4096),
            position: 0,
            line: 1,
            column: 1,
        }
    }

    pub fn next_token(&mut self) -> Result<Token, PdfError> {
        self.skip_whitespace()?;
        
        match self.peek_byte()? {
            b'%' => self.read_comment(),
            b'/' => self.read_name(),
            b'<' => self.read_less_than(),
            b'>' => self.read_greater_than(),
            b'[' => self.read_array_start(),
            b']' => self.read_array_end(),
            b'{' => self.read_dict_start(),
            b'}' => self.read_dict_end(),
            b'(' => self.read_literal_string(),
            b'+' | b'-' | b'.' | b'0'..=b'9' => self.read_number(),
            b'a'..=b'z' | b'A'..=b'Z' => self.read_keyword(),
            _ => Err(PdfError::InvalidStructure("Unknown token".into())),
        }
    }

    fn skip_whitespace(&mut self) -> Result<(), PdfError> {
        loop {
            match self.peek_byte()? {
                b' ' | b'\t' | b'\r' | b'\n' | b'\x0C' => {
                    let byte = self.read_byte()?;
                    if byte == b'\n' {
                        self.line += 1;
                        self.column = 1;
                    } else {
                        self.column += 1;
                    }
                }
                _ => break,
            }
        }
        Ok(())
    }

    fn read_comment(&mut self) -> Result<Token, PdfError> {
        self.read_byte()?; // Skip '%'
        let mut comment = Vec::new();
        
        loop {
            match self.peek_byte()? {
                b'\r' | b'\n' => break,
                byte => {
                    comment.push(byte);
                    self.read_byte()?;
                }
            }
        }
        
        Ok(Token::Comment(comment))
    }

    fn read_name(&mut self) -> Result<Token, PdfError> {
        self.read_byte()?; // Skip '/'
        let mut name = Vec::new();
        
        loop {
            match self.peek_byte()? {
                b' ' | b'\t' | b'\r' | b'\n' | b'\x0C' |
                b'(' | b')' | b'<' | b'>' | b'[' | b']' |
                b'{' | b'}' | b'/' | b'%' => break,
                b'#' => {
                    self.read_byte()?;
                    let hex = self.read_hex_byte()?;
                    name.push(hex);
                }
                byte => {
                    name.push(byte);
                    self.read_byte()?;
                }
            }
        }
        
        Ok(Token::Name(name))
    }

    fn read_hex_byte(&mut self) -> Result<u8, PdfError> {
        let high = self.read_byte()?;
        let low = self.read_byte()?;
        
        let high = match high {
            b'0'..=b'9' => high - b'0',
            b'A'..=b'F' => high - b'A' + 10,
            b'a'..=b'f' => high - b'a' + 10,
            _ => return Err(PdfError::InvalidStructure("Invalid hex character".into())),
        };
        
        let low = match low {
            b'0'..=b'9' => low - b'0',
            b'A'..=b'F' => low - b'A' + 10,
            b'a'..=b'f' => low - b'a' + 10,
            _ => return Err(PdfError::InvalidStructure("Invalid hex character".into())),
        };
        
        Ok((high << 4) | low)
    }

    fn read_less_than(&mut self) -> Result<Token, PdfError> {
        self.read_byte()?; // Skip first '<'
        
        match self.peek_byte()? {
            b'<' => {
                self.read_byte()?;
                Ok(Token::StartDictionary)
            }
            _ => self.read_hex_string(),
        }
    }

    fn read_hex_string(&mut self) -> Result<Token, PdfError> {
        let mut string = Vec::new();
        
        loop {
            match self.peek_byte()? {
                b'>' => {
                    self.read_byte()?;
                    break;
                }
                b'0'..=b'9' | b'A'..=b'F' | b'a'..=b'f' | b' ' | b'\n' | b'\r' => {
                    if let Some(hex) = self.read_hex_pair()? {
                        string.push(hex);
                    }
                }
                _ => return Err(PdfError::InvalidStructure("Invalid hex string".into())),
            }
        }
        
        Ok(Token::HexString(string))
    }

    fn read_hex_pair(&mut self) -> Result<Option<u8>, PdfError> {
        let mut high = None;
        let mut low = None;
        
        while high.is_none() || low.is_none() {
            match self.peek_byte()? {
                b'0'..=b'9' | b'A'..=b'F' | b'a'..=b'f' => {
                    let byte = self.read_byte()?;
                    let value = match byte {
                        b'0'..=b'9' => byte - b'0',
                        b'A'..=b'F' => byte - b'A' + 10,
                        b'a'..=b'f' => byte - b'a' + 10,
                        _ => unreachable!(),
                    };
                    
                    if high.is_none() {
                        high = Some(value);
                    } else {
                        low = Some(value);
                    }
                }
                b'>' => break,
                _ => self.read_byte()?, // Skip whitespace
            }
        }
        
        match (high, low) {
            (Some(h), Some(l)) => Ok(Some((h << 4) | l)),
            (Some(h), None) => Ok(Some(h << 4)),
            _ => Ok(None),
        }
    }

    fn read_greater_than(&mut self) -> Result<Token, PdfError> {
        self.read_byte()?; // Skip first '>'
        
        match self.peek_byte()? {
            b'>' => {
                self.read_byte()?;
                Ok(Token::EndDictionary)
            }
            _ => Err(PdfError::InvalidStructure("Unexpected '>'".into())),
        }
    }

    fn read_array_start(&mut self) -> Result<Token, PdfError> {
        self.read_byte()?;
        Ok(Token::StartArray)
    }

    fn read_array_end(&mut self) -> Result<Token, PdfError> {
        self.read_byte()?;
        Ok(Token::EndArray)
    }

    fn read_dict_start(&mut self) -> Result<Token, PdfError> {
        self.read_byte()?;
        Ok(Token::StartDictionary)
    }

    fn read_dict_end(&mut self) -> Result<Token, PdfError> {
        self.read_byte()?;
        Ok(Token::EndDictionary)
    }

    fn read_literal_string(&mut self) -> Result<Token, PdfError> {
        self.read_byte()?; // Skip '('
        let mut string = Vec::new();
        let mut nesting = 0;
        let mut escape = false;
        
        loop {
            let byte = self.read_byte()?;
            
            if escape {
                match byte {
                    b'n' => string.push(b'\n'),
                    b'r' => string.push(b'\r'),
                    b't' => string.push(b'\t'),
                    b'b' => string.push(b'\x08'),
                    b'f' => string.push(b'\x0C'),
                    b'(' => string.push(b'('),
                    b')' => string.push(b')'),
                    b'\\' => string.push(b'\\'),
                    b'0'..=b'7' => {
                        let mut octal = (byte - b'0') as u8;
                        for _ in 0..2 {
                            match self.peek_byte()? {
                                b'0'..=b'7' => {
                                    octal = (octal << 3) | (self.read_byte()? - b'0');
                                }
                                _ => break,
                            }
                        }
                        string.push(octal);
                    }
                    _ => string.push(byte),
                }
                escape = false;
            } else {
                match byte {
                    b'\\' => escape = true,
                    b'(' => {
                        string.push(byte);
                        nesting += 1;
                    }
                    b')' if nesting > 0 => {
                        string.push(byte);
                        nesting -= 1;
                    }
                    b')' if nesting == 0 => break,
                    _ => string.push(byte),
                }
            }
        }
        
        Ok(Token::String(string))
    }

    fn read_number(&mut self) -> Result<Token, PdfError> {
        let mut number = String::new();
        let mut has_decimal = false;
        let mut is_negative = false;
        
        // Handle sign
        match self.peek_byte()? {
            b'+' => {
                self.read_byte()?;
            }
            b'-' => {
                is_negative = true;
                self.read_byte()?;
            }
            _ => {}
        }
        
        loop {
            match self.peek_byte()? {
                b'0'..=b'9' => {
                    number.push(self.read_byte()? as char);
                }
                b'.' if !has_decimal => {
                    has_decimal = true;
                    number.push(self.read_byte()? as char);
                }
                _ => break,
            }
        }
        
        if is_negative {
            number.insert(0, '-');
        }
        
        if has_decimal {
            match f64::from_str(&number) {
                Ok(value) => Ok(Token::Real(value)),
                Err(_) => Err(PdfError::InvalidStructure("Invalid real number".into())),
            }
        } else {
            match i64::from_str(&number) {
                Ok(value) => Ok(Token::Integer(value)),
                Err(_) => Err(PdfError::InvalidStructure("Invalid integer".into())),
            }
        }
    }

    fn read_keyword(&mut self) -> Result<Token, PdfError> {
        let mut keyword = Vec::new();
        
        loop {
            match self.peek_byte()? {
                b'a'..=b'z' | b'A'..=b'Z' => {
                    keyword.push(self.read_byte()?);
                }
                _ => break,
            }
        }
        
        match keyword.as_slice() {
            b"true" => Ok(Token::Boolean(true)),
            b"false" => Ok(Token::Boolean(false)),
            b"null" => Ok(Token::Null),
            b"obj" => Ok(Token::StartObject),
            b"endobj" => Ok(Token::EndObject),
            b"stream" => Ok(Token::StartStream),
            b"endstream" => Ok(Token::EndStream),
            b"R" => Ok(Token::Reference),
            _ => Ok(Token::Keyword(keyword)),
        }
    }

    fn peek_byte(&mut self) -> Result<u8, PdfError> {
        if self.position >= self.buffer.len() {
            self.buffer.clear();
            self.position = 0;
            
            let mut temp = [0u8; 1];
            if self.reader.read(&mut temp)? == 0 {
                return Err(PdfError::UnexpectedEOF);
            }
            self.buffer.push(temp[0]);
        }
        
        Ok(self.buffer[self.position])
    }

    fn read_byte(&mut self) -> Result<u8, PdfError> {
        let byte = self.peek_byte()?;
        self.position += 1;
        Ok(byte)
    }

    pub fn position(&mut self) -> Result<u64, PdfError> {
        let current = self.reader.seek(SeekFrom::Current(0))?;
        Ok(current - (self.buffer.len() - self.position) as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_read_integer() {
        let data = b"123 ";
        let mut lexer = Lexer::new(Cursor::new(data.as_ref()));
        assert_eq!(lexer.next_token().unwrap(), Token::Integer(123));
    }

    #[test]
    fn test_read_real() {
        let data = b"123.456 ";
        let mut lexer = Lexer::new(Cursor::new(data.as_ref()));
        assert_eq!(lexer.next_token().unwrap(), Token::Real(123.456));
    }

    #[test]
    fn test_read_name() {
        let data = b"/Name ";
        let mut lexer = Lexer::new(Cursor::new(data.as_ref()));
        assert_eq!(lexer.next_token().unwrap(), Token::Name(b"Name".to_vec()));
    }

    #[test]
    fn test_read_string() {
        let data = b"(Hello World) ";
        let mut lexer = Lexer::new(Cursor::new(data.as_ref()));
        assert_eq!(
            lexer.next_token().unwrap(),
            Token::String(b"Hello World".to_vec())
        );
    }

    #[test]
    fn test_read_hex_string() {
        let data = b"<48656C6C6F> ";
        let mut lexer = Lexer::new(Cursor::new(data.as_ref()));
        assert_eq!(
            lexer.next_token().unwrap(),
            Token::HexString(b"Hello".to_vec())
        );
    }

    #[test]
    fn test_read_array() {
        let data = b"[1 2 3] ";
        let mut lexer = Lexer::new(Cursor::new(data.as_ref()));
        assert_eq!(lexer.next_token().unwrap(), Token::StartArray);
        assert_eq!(lexer.next_token().unwrap(), Token::Integer(1));
        assert_eq!(lexer.next_token().unwrap(), Token::Integer(2));
        assert_eq!(lexer.next_token().unwrap(), Token::Integer(3));
        assert_eq!(lexer.next_token().unwrap(), Token::EndArray);
    }

    #[test]
    fn test_read_dictionary() {
        let data = b"<</Key /Value>> ";
        let mut lexer = Lexer::new(Cursor::new(data.as_ref()));
        assert_eq!(lexer.next_token().unwrap(), Token::StartDictionary);
        assert_eq!(lexer.next_token().unwrap(), Token::Name(b"Key".to_vec()));
        assert_eq!(lexer.next_token().unwrap(), Token::Name(b"Value".to_vec()));
        assert_eq!(lexer.next_token().unwrap(), Token::EndDictionary);
    }
}
