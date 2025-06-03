#!/bin/bash

# PDF Anti-Forensics Project Structure Fix Script
# Author: kartik4091
# Created: 2025-06-03 07:30:51 UTC

# Set error handling
set -e
echo "Starting antiforensics project structure fix..."

# Store current directory
CURRENT_DIR=$(pwd)
PROJECT_ROOT="$CURRENT_DIR"

# Function to log operations
log_operation() {
    echo "[$(date +"%Y-%m-%d %H:%M:%S UTC")] $1"
}

# Function to create a rust module file
create_module_file() {
    local file_path="$1"
    local module_name="$2"
    local author="kartik4091"
    local created_date="2025-06-03 07:30:51 UTC"
    
    mkdir -p "$(dirname "$file_path")"
    
    cat > "$file_path" << EOF
//! $module_name module for PDF antiforensics
//! Author: $author
//! Created: $created_date
//! This module provides $module_name capabilities for PDF document analysis.

EOF
    log_operation "Created module file: $file_path"
}

# 1. Create required directories
log_operation "Creating directory structure..."
mkdir -p src/antiforensics/scanner/{deep_scanner,signature_scanner,stream_scanner,object_scanner}
mkdir -p src/antiforensics/{utils,verifier,report,cleaner}

# 2. Move scanner module files
log_operation "Restructuring scanner module..."
for scanner in deep_scanner signature_scanner stream_scanner object_scanner; do
    if [ -f "src/antiforensics/scanner/${scanner}.rs" ]; then
        mkdir -p "src/antiforensics/scanner/${scanner}"
        mv "src/antiforensics/scanner/${scanner}.rs" "src/antiforensics/scanner/${scanner}/mod.rs"
        log_operation "Moved ${scanner}.rs to ${scanner}/mod.rs"
    fi
done

# 3. Fix utils directory
log_operation "Fixing utils directory..."
if [ -d "src/antiforensics/utlis" ]; then
    mkdir -p src/antiforensics/utils
    mv src/antiforensics/utlis/* src/antiforensics/utils/
    rmdir src/antiforensics/utlis
    log_operation "Moved files from utlis to utils"
fi

# 4. Rename files with incorrect extensions
log_operation "Fixing file extensions..."
if [ -f "src/antiforensics/utils/crypto.utlis.rs" ]; then
    mv src/antiforensics/utils/crypto.utlis.rs src/antiforensics/utils/crypto.utils.rs
    log_operation "Renamed crypto.utlis.rs to crypto.utils.rs"
fi

# 5. Fix British to American spelling
if [ -f "src/antiforensics/analyzer/content_analyser.rs" ]; then
    mv src/antiforensics/analyzer/content_analyser.rs src/antiforensics/analyzer/content_analyzer.rs
    log_operation "Renamed content_analyser.rs to content_analyzer.rs"
fi

# 6. Create missing module files
log_operation "Creating missing module files..."

# Create utils/mod.rs
create_module_file "src/antiforensics/utils/mod.rs" "Utilities" << EOF
pub mod crypto;
pub mod logging;
pub mod validation;
pub mod pattern_utils;
pub mod binary_utils;
pub mod metadata_utils;
pub mod sanitization_utils;
EOF

# Create cleaner/mod.rs
create_module_file "src/antiforensics/cleaner/mod.rs" "Cleaner" << EOF
pub mod content_cleaner;
pub mod structure_cleaner;
pub mod metadata_cleaner;
EOF

# Create verifier/mod.rs
create_module_file "src/antiforensics/verifier/mod.rs" "Verifier" << EOF
pub mod forensic_verifier;
pub mod security_verifier;
pub mod chain_verifier;
EOF

# Create report/mod.rs
create_module_file "src/antiforensics/report/mod.rs" "Report" << EOF
pub mod generator;
pub mod templates;
pub mod formatter;
EOF

# 7. Create missing utility files
for util in pattern_utils binary_utils metadata_utils sanitization_utils; do
    create_module_file "src/antiforensics/utils/${util}.rs" "${util/_/ }"
done

# 8. Create missing verifier files
for verifier in forensic_verifier security_verifier chain_verifier; do
    create_module_file "src/antiforensics/verifier/${verifier}.rs" "${verifier/_/ }"
done

# 9. Create missing report files
for report in generator templates formatter; do
    create_module_file "src/antiforensics/report/${report}.rs" "${report}"
done

# Final check and report
log_operation "Structure fix completed. Running final checks..."

# Display the new structure
echo -e "\nNew project structure:"
tree src/antiforensics

# Final status message
echo -e "\nProject structure fix completed at $(date +"%Y-%m-%d %H:%M:%S UTC")"
echo "Please review the changes and run 'cargo check' to verify the Rust code structure."
