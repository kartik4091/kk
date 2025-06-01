// src/bin/sanitize.rs

use std::{env, fs::File, io::Read, path::Path};

use pdx_secure_pdf::core::Document;
use pdx_secure_pdf::metadata::{clean_docinfo_metadata, remove_xmp_metadata};
use pdx_secure_pdf::writer::save_document;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: {} <input.pdf> <output.pdf>", args[0]);
        std::process::exit(1);
    }

    let input_path = Path::new(&args[1]);
    let output_path = Path::new(&args[2]);

    let mut file = File::open(input_path).expect("Failed to open input PDF");
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).expect("Failed to read input PDF");

    let mut document = Document::load(&buffer).expect("Failed to load PDF");

    clean_docinfo_metadata(&mut document).expect("Failed to clean DocInfo metadata");
    remove_xmp_metadata(&mut document).expect("Failed to remove XMP metadata");

    save_document(&document, output_path).expect("Failed to save sanitized PDF");

    println!("✅ PDF metadata sanitized and saved to {}", output_path.display());
}