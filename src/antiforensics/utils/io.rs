//! I/O utility functions for PDF anti-forensics
//! Created: 2025-06-03 16:42:36 UTC
//! Author: kartik4091

use std::fs::{self, File};
use std::io::{self, Read, Write, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use memmap2::{Mmap, MmapMut};
use tracing::{debug, error, info, instrument, warn};

use crate::error::{Error, Result};

/// File reading modes
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ReadMode {
    /// Standard file reading
    Standard,
    
    /// Memory mapped reading
    MemoryMapped,
    
    /// Buffered reading
    Buffered,
}

/// File writing modes
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WriteMode {
    /// Standard file writing
    Standard,
    
    /// Memory mapped writing
    MemoryMapped,
    
    /// Buffered writing
    Buffered,
}

/// I/O configuration
#[derive(Debug, Clone)]
pub struct IoConfig {
    /// Buffer size in bytes
    pub buffer_size: usize,
    
    /// Read mode
    pub read_mode: ReadMode,
    
    /// Write mode
    pub write_mode: WriteMode,
    
    /// Sync mode
    pub sync_mode: SyncMode,
}

/// Sync modes
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SyncMode {
    /// No sync
    None,
    
    /// Sync data only
    Data,
    
    /// Sync data and metadata
    Full,
}

impl Default for IoConfig {
    fn default() -> Self {
        Self {
            buffer_size: 8192,
            read_mode: ReadMode::Standard,
            write_mode: WriteMode::Standard,
            sync_mode: SyncMode::None,
        }
    }
}

/// Read file contents
#[instrument(skip(config))]
pub fn read_file(path: impl AsRef<Path>, config: &IoConfig) -> Result<Vec<u8>> {
    let path = path.as_ref();
    debug!("Reading file: {}", path.display());
    
    match config.read_mode {
        ReadMode::Standard => read_file_standard(path),
        ReadMode::MemoryMapped => read_file_mmap(path),
        ReadMode::Buffered => read_file_buffered(path, config.buffer_size),
    }
}

/// Write file contents
#[instrument(skip(data, config))]
pub fn write_file(path: impl AsRef<Path>, data: &[u8], config: &IoConfig) -> Result<()> {
    let path = path.as_ref();
    debug!("Writing file: {}", path.display());
    
    match config.write_mode {
        WriteMode::Standard => write_file_standard(path, data, config.sync_mode),
        WriteMode::MemoryMapped => write_file_mmap(path, data),
        WriteMode::Buffered => write_file_buffered(path, data, config.buffer_size, config.sync_mode),
    }
}

/// Read file using standard I/O
fn read_file_standard(path: &Path) -> Result<Vec<u8>> {
    let mut file = File::open(path)
        .map_err(|e| Error::IoError(format!("Failed to open file: {}", e)))?;
    
    let mut data = Vec::new();
    file.read_to_end(&mut data)
        .map_err(|e| Error::IoError(format!("Failed to read file: {}", e)))?;
    
    Ok(data)
}

/// Read file using memory mapping
fn read_file_mmap(path: &Path) -> Result<Vec<u8>> {
    let file = File::open(path)
        .map_err(|e| Error::IoError(format!("Failed to open file: {}", e)))?;
    
    let mmap = unsafe {
        Mmap::map(&file)
            .map_err(|e| Error::IoError(format!("Failed to memory map file: {}", e)))?
    };
    
    Ok(mmap.to_vec())
}

/// Read file using buffered I/O
fn read_file_buffered(path: &Path, buffer_size: usize) -> Result<Vec<u8>> {
    let file = File::open(path)
        .map_err(|e| Error::IoError(format!("Failed to open file: {}", e)))?;
    
    let mut reader = io::BufReader::with_capacity(buffer_size, file);
    let mut data = Vec::new();
    
    reader.read_to_end(&mut data)
        .map_err(|e| Error::IoError(format!("Failed to read file: {}", e)))?;
    
    Ok(data)
}

/// Write file using standard I/O
fn write_file_standard(path: &Path, data: &[u8], sync_mode: SyncMode) -> Result<()> {
    let mut file = File::create(path)
        .map_err(|e| Error::IoError(format!("Failed to create file: {}", e)))?;
    
    file.write_all(data)
        .map_err(|e| Error::IoError(format!("Failed to write file: {}", e)))?;
    
    match sync_mode {
        SyncMode::None => {}
        SyncMode::Data => {
            file.sync_data()
                .map_err(|e| Error::IoError(format!("Failed to sync file data: {}", e)))?;
        }
        SyncMode::Full => {
            file.sync_all()
                .map_err(|e| Error::IoError(format!("Failed to sync file: {}", e)))?;
        }
    }
    
    Ok(())
}

/// Write file using memory mapping
fn write_file_mmap(path: &Path, data: &[u8]) -> Result<()> {
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .map_err(|e| Error::IoError(format!("Failed to create file: {}", e)))?;
    
    file.set_len(data.len() as u64)
        .map_err(|e| Error::IoError(format!("Failed to set file length: {}", e)))?;
    
    let mut mmap = unsafe {
        MmapMut::map_mut(&file)
            .map_err(|e| Error::IoError(format!("Failed to memory map file: {}", e)))?
    };
    
    mmap.copy_from_slice(data);
    mmap.flush()
        .map_err(|e| Error::IoError(format!("Failed to flush memory map: {}", e)))?;
    
    Ok(())
}

/// Write file using buffered I/O
fn write_file_buffered(path: &Path, data: &[u8], buffer_size: usize, sync_mode: SyncMode) -> Result<()> {
    let file = File::create(path)
        .map_err(|e| Error::IoError(format!("Failed to create file: {}", e)))?;
    
    let mut writer = io::BufWriter::with_capacity(buffer_size, file);
    
    writer.write_all(data)
        .map_err(|e| Error::IoError(format!("Failed to write file: {}", e)))?;
    
    writer.flush()
        .map_err(|e| Error::IoError(format!("Failed to flush buffer: {}", e)))?;
    
    match sync_mode {
        SyncMode::None => {}
        SyncMode::Data => {
            writer.get_ref().sync_data()
                .map_err(|e| Error::IoError(format!("Failed to sync file data: {}", e)))?;
        }
        SyncMode::Full => {
            writer.get_ref().sync_all()
                .map_err(|e| Error::IoError(format!("Failed to sync file: {}", e)))?;
        }
    }
    
    Ok(())
}

/// Create directory
#[instrument]
pub fn create_dir(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    debug!("Creating directory: {}", path.display());
    
    fs::create_dir_all(path)
        .map_err(|e| Error::IoError(format!("Failed to create directory: {}", e)))?;
    
    Ok(())
}

/// Remove file or directory
#[instrument]
pub fn remove_path(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    debug!("Removing path: {}", path.display());
    
    if path.is_dir() {
        fs::remove_dir_all(path)
            .map_err(|e| Error::IoError(format!("Failed to remove directory: {}", e)))?;
    } else {
        fs::remove_file(path)
            .map_err(|e| Error::IoError(format!("Failed to remove file: {}", e)))?;
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    
    #[test]
    fn test_file_operations() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        let data = b"Hello, World!";
        let config = IoConfig::default();
        
        // Test standard I/O
        assert!(write_file(&file_path, data, &config).is_ok());
        let read_data = read_file(&file_path, &config).unwrap();
        assert_eq!(read_data, data);
        
        // Test memory mapped I/O
        let mmap_config = IoConfig {
            read_mode: ReadMode::MemoryMapped,
            write_mode: WriteMode::MemoryMapped,
            ..Default::default()
        };
        assert!(write_file(&file_path, data, &mmap_config).is_ok());
        let read_data = read_file(&file_path, &mmap_config).unwrap();
        assert_eq!(read_data, data);
        
        // Test buffered I/O
        let buffered_config = IoConfig {
            read_mode: ReadMode::Buffered,
            write_mode: WriteMode::Buffered,
            buffer_size: 4096,
            ..Default::default()
        };
        assert!(write_file(&file_path, data, &buffered_config).is_ok());
        let read_data = read_file(&file_path, &buffered_config).unwrap();
        assert_eq!(read_data, data);
    }
    
    #[test]
    fn test_directory_operations() {
        let dir = tempdir().unwrap();
        let test_dir = dir.path().join("test_dir");
        
        assert!(create_dir(&test_dir).is_ok());
        assert!(test_dir.exists());
        
        assert!(remove_path(&test_dir).is_ok());
        assert!(!test_dir.exists());
    }
    
    #[test]
    fn test_sync_modes() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        let data = b"Hello, World!";
        
        // Test data sync
        let data_sync_config = IoConfig {
            sync_mode: SyncMode::Data,
            ..Default::default()
        };
        assert!(write_file(&file_path, data, &data_sync_config).is_ok());
        
        // Test full sync
        let full_sync_config = IoConfig {
            sync_mode: SyncMode::Full,
            ..Default::default()
        };
        assert!(write_file(&file_path, data, &full_sync_config).is_ok());
    }
          }
