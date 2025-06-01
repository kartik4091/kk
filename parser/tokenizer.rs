// Auto-patched by Alloma
// Timestamp: 2025-06-01 15:54:26
// User: kartik6717

// Auto-implemented by Alloma Placeholder Patcher
// Timestamp: 2025-06-01 15:02:33
// User: kartik6717
// Note: Placeholder code has been replaced with actual implementations

#![allow(warnings)]

use super::lexer::{Lexer, Token};
use std::io::{Read, Seek};
use crate::core::error::PdfError;

pub struct Tokenizer<R: Read + Seek> {
    lexer: Lexer<R>,
    current_token: Option<Token>,
    peeked_token: Option<Token>,
}

impl<R: Read + Seek> Tokenizer<R> {
    pub fn new(reader: R) -> Self {
        Self {
            lexer: Lexer::new(reader),
            current_token: None,
            peeked_token: None,
        }
    }

    pub fn next_token(&mut self) -> Result<Token, PdfError> {
        if let Some(token) = self.peeked_token.take() {
            self.current_token = Some(token.clone());
            return Ok(token);
        }

        let token = self.lexer.next_token()?;
        self.current_token = Some(token.clone());
        Ok(token)
    }

    pub fn peek_token(&mut self) -> Result<Token, PdfError> {
        if let Some(ref token) = self.peeked_token {
            return Ok(token.clone());
        }

        let token = self.lexer.next_token()?;
        self.peeked_token = Some(token.clone());
        Ok(token)
    }

    pub fn current_position(&mut self) -> Result<u64, PdfError> {
        self.lexer.position()
    }

    pub fn expect_token(&mut self, expected: Token) -> Result<(), PdfError> {
        let token = self.next_token()?;
        if token != expected {
            return Err(PdfError::InvalidStructure(format!(
                "Expected {:?}, found {:?}",
                expected, token
            )));
        }
        Ok(())
    }
}
