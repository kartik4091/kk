// Auto-patched by Alloma
// Timestamp: 2025-06-01 15:54:26
// User: kartik6717

// Auto-implemented by Alloma Placeholder Patcher
// Timestamp: 2025-06-01 15:02:33
// User: kartik6717
// Note: Placeholder code has been replaced with actual implementations

#![allow(warnings)]

// Last modified: 2025-05-31 14:39:11 UTC
// Author: kartik6717

#[derive(Debug, Clone, PartialEq)]
pub enum PdfObject {
    Null,
    Boolean(bool),
    Integer(i64),
    Real(f64),
    String(Vec<u8>),
    Name(Vec<u8>),
    Array(Vec<PdfObject>),
    Dictionary(Vec<(Vec<u8>, PdfObject)>),
    Stream {
        dict: Vec<(Vec<u8>, PdfObject)>,
        data: Vec<u8>,
    },
    Reference {
        obj_num: i64,
        gen_num: i64,
    },
}

impl PdfObject {
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            PdfObject::Boolean(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            PdfObject::Integer(i) => Some(*i),
            _ => None,
        }
    }

    pub fn as_real(&self) -> Option<f64> {
        match self {
            PdfObject::Real(r) => Some(*r),
            PdfObject::Integer(i) => Some(*i as f64),
            _ => None,
        }
    }

    pub fn as_string(&self) -> Option<&[u8]> {
        match self {
            PdfObject::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_name(&self) -> Option<&[u8]> {
        match self {
            PdfObject::Name(n) => Some(n),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[PdfObject]> {
        match self {
            PdfObject::Array(a) => Some(a),
            _ => None,
        }
    }

    pub fn as_dict(&self) -> Option<&[(Vec<u8>, PdfObject)]> {
        match self {
            PdfObject::Dictionary(d) => Some(d),
            PdfObject::Stream { dict, .. } => Some(d),
            _ => None,
        }
    }
}
