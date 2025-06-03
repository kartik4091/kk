#!/data/data/com.termux/files/usr/bin/bash

# PDF Anti-Forensics Project Restructure Script
# Author: kartik4091
# Created: 2025-06-03 07:23:09 UTC

# Set error handling
set -e
echo "Starting project restructure in Termux..."

# Create backup using simple timestamp
BACKUP_DIR="backup_antiforensics"
echo "Creating backup in $BACKUP_DIR..."
mkdir -p "$BACKUP_DIR"
cp -r src/antiforensics/* "$BACKUP_DIR/"

# Create required directories
echo "Creating directory structure..."
mkdir -p src/antiforensics/scanner/{deep_scanner,signature_scanner,stream_scanner,object_scanner}
mkdir -p src/antiforensics/{utils,verifier,report,cleaner}

# Move scanner files to their proper locations
echo "Moving scanner files..."
for scanner in deep_scanner signature_scanner stream_scanner object_scanner; do
    if [ -f "src/antiforensics/scanner/${scanner}.rs" ]; then
        mkdir -p "src/antiforensics/scanner/${scanner}"
        mv "src/antiforensics/scanner/${scanner}.rs" "src/antiforensics/scanner/${scanner}/mod.rs"
    fi
done

# Fix utils directory
echo "Fixing utils directory..."
if [ -d "src/antiforensics/utlis" ]; then
    mkdir -p src/antiforensics/utils
    for file in src/antiforensics/utlis/*; do
        if [ -f "$file" ]; then
            new_name=$(echo "$file" | sed 's/\.utlis\./.utils./')
            new_name=$(echo "$new_name" | sed 's/utlis/utils/')
            mv "$file" "$new_name"
        fi
    done
    rm -rf src/antiforensics/utlis
fi

# Fix British to American spelling
echo "Fixing spelling conventions..."
if [ -f "src/antiforensics/analyzer/content_analyser.rs" ]; then
    mv "src/antiforensics/analyzer/content_analyser.rs" "src/antiforensics/analyzer/content_analyzer.rs"
fi

# Create missing module files
echo "Creating missing module files..."

# utils/mod.rs
mkdir -p src/antiforensics/utils
cat > src/antiforensics/utils/mod.rs << 'EOF'
//! Utilities module for PDF antiforensics
//! Author: kartik4091
//! Created: 2025-06-03 07:23:09 UTC

pub mod crypto;
pub mod logging;
pub mod validation;
pub mod pattern_utils;
pub mod binary_utils;
pub mod metadata_utils;
pub mod sanitization_utils;
EOF

# cleaner/mod.rs
mkdir -p src/antiforensics/cleaner
cat > src/antiforensics/cleaner/mod.rs << 'EOF'
//! Cleaner module for PDF document sanitization
//! Author: kartik4091
//! Created: 2025-06-03 07:23:09 UTC

pub mod content_cleaner;
pub mod structure_cleaner;
pub mod metadata_cleaner;
EOF

# verifier/mod.rs
mkdir -p src/antiforensics/verifier
cat > src/antiforensics/verifier/mod.rs << 'EOF'
//! Verifier module for PDF document verification
//! Author: kartik4091
//! Created: 2025-06-03 07:23:09 UTC

pub mod forensic_verifier;
pub mod security_verifier;
pub mod chain_verifier;
EOF

# report/mod.rs
mkdir -p src/antiforensics/report
cat > src/antiforensics/report/mod.rs << 'EOF'
//! Report module for PDF document analysis reports
//! Author: kartik4091
//! Created: 2025-06-03 07:23:09 UTC

pub mod generator;
pub mod templates;
pub mod formatter;
EOF

# Update main module declarations
cat > src/antiforensics/mod.rs << 'EOF'
//! Antiforensics module for secure PDF document sanitization
//! Author: kartik4091
//! Last Modified: 2025-06-03 07:23:09 UTC

pub mod analyzer;
pub mod scanner;
pub mod cleaner;
pub mod verifier;
pub mod report;
pub mod utils;

// Re-exports
pub use analyzer::{Analyzer, AnalyzerConfig};
pub use scanner::{Scanner, ScannerConfig};
pub use cleaner::{Cleaner, CleanerConfig};
pub use verifier::{Verifier, VerifierConfig};
pub use report::ReportGenerator;
EOF

# Update scanner module declarations
cat > src/antiforensics/scanner/mod.rs << 'EOF'
//! Scanner module for PDF document analysis
//! Author: kartik4091
//! Last Modified: 2025-06-03 07:23:09 UTC

pub mod deep_scanner;
pub mod signature_scanner;
pub mod stream_scanner;
pub mod object_scanner;

pub use deep_scanner::DeepScanner;
pub use signature_scanner::SignatureScanner;
pub use stream_scanner::StreamScanner;
pub use object_scanner::ObjectScanner;
EOF

# Create .gitkeep files for empty directories
find src/antiforensics -type d -empty -exec touch {}/.gitkeep \;

echo "Structure update complete!"
echo "Backup created in: $BACKUP_DIR"
echo "Next steps:"
echo "1. Review the changes"
echo "2. Check cargo build"
echo "3. Commit the changes"
