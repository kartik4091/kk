//! Memory utility functions for PDF anti-forensics
//! Created: 2025-06-03 16:42:36 UTC
//! Author: kartik4091

use std::alloc::{self, Layout};
use std::ptr::{self, NonNull};
use std::sync::atomic::{AtomicUsize, Ordering};
use tracing::{debug, error, info, instrument, warn};

use crate::error::{Error, Result};

/// Current allocated memory
static ALLOCATED_MEMORY: AtomicUsize = AtomicUsize::new(0);

/// Memory configuration
#[derive(Debug, Clone)]
pub struct MemoryConfig {
    /// Memory limit in bytes
    pub memory_limit: usize,
    
    /// Alignment in bytes
    pub alignment: usize,
    
    /// Zero memory on allocation
    pub zero_memory: bool,
    
    /// Zero memory on deallocation
    pub zero_on_drop: bool,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            memory_limit: usize::MAX,
            alignment: std::mem::size_of::<usize>(),
            zero_memory: false,
            zero_on_drop: true,
        }
    }
}

/// Memory allocation wrapper
#[derive(Debug)]
pub struct MemoryBlock {
    /// Pointer to allocated memory
    ptr: NonNull<u8>,
    
    /// Layout of allocated memory
    layout: Layout,
    
    /// Configuration used for allocation
    config: MemoryConfig,
}

impl MemoryBlock {
    /// Allocate new memory block
    pub fn new(size: usize, config: MemoryConfig) -> Result<Self> {
        let current_allocated = ALLOCATED_MEMORY.load(Ordering::Relaxed);
        if current_allocated + size > config.memory_limit {
            return Err(Error::MemoryError("Memory limit exceeded".to_string()));
        }
        
        let layout = Layout::from_size_align(size, config.alignment)
            .map_err(|e| Error::MemoryError(format!("Invalid memory layout: {}", e)))?;
        
        let ptr = unsafe {
            NonNull::new(alloc::alloc(layout))
                .ok_or_else(|| Error::MemoryError("Memory allocation failed".to_string()))?
        };
        
        if config.zero_memory {
            unsafe {
                ptr::write_bytes(ptr.as_ptr(), 0, size);
            }
        }
        
        ALLOCATED_MEMORY.fetch_add(size, Ordering::Relaxed);
        
        Ok(Self {
            ptr,
            layout,
            config,
        })
    }
    
    /// Get pointer to allocated memory
    pub fn as_ptr(&self) -> *mut u8 {
        self.ptr.as_ptr()
    }
    
    /// Get slice of allocated memory
    pub fn as_slice(&self) -> &[u8] {
        unsafe {
            std::slice::from_raw_parts(self.ptr.as_ptr(), self.layout.size())
        }
    }
    
    /// Get mutable slice of allocated memory
    pub fn as_slice_mut(&mut self) -> &mut [u8] {
        unsafe {
            std::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.layout.size())
        }
    }
    
    /// Get size of allocated memory
    pub fn size(&self) -> usize {
        self.layout.size()
    }
    
    /// Zero memory contents
    pub fn zero(&mut self) {
        unsafe {
            ptr::write_bytes(self.ptr.as_ptr(), 0, self.layout.size());
        }
    }
    
    /// Fill memory with pattern
    pub fn fill(&mut self, pattern: u8) {
        unsafe {
            ptr::write_bytes(self.ptr.as_ptr(), pattern, self.layout.size());
        }
    }
}

impl Drop for MemoryBlock {
    fn drop(&mut self) {
        if self.config.zero_on_drop {
            self.zero();
        }
        
        unsafe {
            alloc::dealloc(self.ptr.as_ptr(), self.layout);
        }
        
        ALLOCATED_MEMORY.fetch_sub(self.layout.size(), Ordering::Relaxed);
    }
}

/// Secure memory operations
pub mod secure {
    use super::*;
    
    /// Memory block with secure operations
    #[derive(Debug)]
    pub struct SecureMemoryBlock {
        /// Inner memory block
        block: MemoryBlock,
        
        /// Lock status
        locked: bool,
    }
    
    impl SecureMemoryBlock {
        /// Create new secure memory block
        pub fn new(size: usize, config: MemoryConfig) -> Result<Self> {
            Ok(Self {
                block: MemoryBlock::new(size, config)?,
                locked: false,
            })
        }
        
        /// Lock memory to prevent swapping
        pub fn lock(&mut self) -> Result<()> {
            if !self.locked {
                #[cfg(unix)]
                unsafe {
                    if libc::mlock(self.block.ptr.as_ptr() as *const libc::c_void, self.block.size()) != 0 {
                        return Err(Error::MemoryError("Failed to lock memory".to_string()));
                    }
                }
                self.locked = true;
            }
            Ok(())
        }
        
        /// Unlock memory
        pub fn unlock(&mut self) -> Result<()> {
            if self.locked {
                #[cfg(unix)]
                unsafe {
                    if libc::munlock(self.block.ptr.as_ptr() as *const libc::c_void, self.block.size()) != 0 {
                        return Err(Error::MemoryError("Failed to unlock memory".to_string()));
                    }
                }
                self.locked = false;
            }
            Ok(())
        }
        
        /// Get reference to inner memory block
        pub fn inner(&self) -> &MemoryBlock {
            &self.block
        }
        
        /// Get mutable reference to inner memory block
        pub fn inner_mut(&mut self) -> &mut MemoryBlock {
            &mut self.block
        }
    }
    
    impl Drop for SecureMemoryBlock {
        fn drop(&mut self) {
            if self.locked {
                let _ = self.unlock();
            }
        }
    }
}

/// Memory pool for efficient allocation
#[derive(Debug)]
pub struct MemoryPool {
    /// Available blocks
    blocks: Vec<MemoryBlock>,
    
    /// Configuration
    config: MemoryConfig,
    
    /// Block size
    block_size: usize,
    
    /// Maximum blocks
    max_blocks: usize,
}

impl MemoryPool {
    /// Create new memory pool
    pub fn new(block_size: usize, max_blocks: usize, config: MemoryConfig) -> Self {
        Self {
            blocks: Vec::new(),
            config,
            block_size,
            max_blocks,
        }
    }
    
    /// Get memory block from pool
    pub fn acquire(&mut self) -> Result<MemoryBlock> {
        if let Some(block) = self.blocks.pop() {
            Ok(block)
        } else if self.blocks.len() < self.max_blocks {
            MemoryBlock::new(self.block_size, self.config.clone())
        } else {
            Err(Error::MemoryError("Memory pool exhausted".to_string()))
        }
    }
    
    /// Return memory block to pool
    pub fn release(&mut self, mut block: MemoryBlock) {
        if self.blocks.len() < self.max_blocks {
            block.zero();
            self.blocks.push(block);
        }
    }
    
    /// Clear memory pool
    pub fn clear(&mut self) {
        self.blocks.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_memory_block() {
        let config = MemoryConfig::default();
        let mut block = MemoryBlock::new(1024, config).unwrap();
        
        assert_eq!(block.size(), 1024);
        
        block.fill(0xFF);
        assert!(block.as_slice().iter().all(|&x| x == 0xFF));
        
        block.zero();
        assert!(block.as_slice().iter().all(|&x| x == 0));
    }
    
    #[test]
    fn test_secure_memory_block() {
        let config = MemoryConfig::default();
        let mut block = secure::SecureMemoryBlock::new(1024, config).unwrap();
        
        assert!(block.lock().is_ok());
        assert!(block.unlock().is_ok());
    }
    
    #[test]
    fn test_memory_pool() {
        let config = MemoryConfig::default();
        let mut pool = MemoryPool::new(1024, 2, config);
        
        let block1 = pool.acquire().unwrap();
        let block2 = pool.//! Memory utility functions for PDF anti-forensics
//! Created: 2025-06-03 16:42:36 UTC
//! Author: kartik4091

use std::alloc::{self, Layout};
use std::ptr::{self, NonNull};
use std::sync::atomic::{AtomicUsize, Ordering};
use tracing::{debug, error, info, instrument, warn};

use crate::error::{Error, Result};

/// Current allocated memory
static ALLOCATED_MEMORY: AtomicUsize = AtomicUsize::new(0);

/// Memory configuration
#[derive(Debug, Clone)]
pub struct MemoryConfig {
    /// Memory limit in bytes
    pub memory_limit: usize,
    
    /// Alignment in bytes
    pub alignment: usize,
    
    /// Zero memory on allocation
    pub zero_memory: bool,
    
    /// Zero memory on deallocation
    pub zero_on_drop: bool,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            memory_limit: usize::MAX,
            alignment: std::mem::size_of::<usize>(),
            zero_memory: false,
            zero_on_drop: true,
        }
    }
}

/// Memory allocation wrapper
#[derive(Debug)]
pub struct MemoryBlock {
    /// Pointer to allocated memory
    ptr: NonNull<u8>,
    
    /// Layout of allocated memory
    layout: Layout,
    
    /// Configuration used for allocation
    config: MemoryConfig,
}

impl MemoryBlock {
    /// Allocate new memory block
    pub fn new(size: usize, config: MemoryConfig) -> Result<Self> {
        let current_allocated = ALLOCATED_MEMORY.load(Ordering::Relaxed);
        if current_allocated + size > config.memory_limit {
            return Err(Error::MemoryError("Memory limit exceeded".to_string()));
        }
        
        let layout = Layout::from_size_align(size, config.alignment)
            .map_err(|e| Error::MemoryError(format!("Invalid memory layout: {}", e)))?;
        
        let ptr = unsafe {
            NonNull::new(alloc::alloc(layout))
                .ok_or_else(|| Error::MemoryError("Memory allocation failed".to_string()))?
        };
        
        if config.zero_memory {
            unsafe {
                ptr::write_bytes(ptr.as_ptr(), 0, size);
            }
        }
        
        ALLOCATED_MEMORY.fetch_add(size, Ordering::Relaxed);
        
        Ok(Self {
            ptr,
            layout,
            config,
        })
    }
    
    /// Get pointer to allocated memory
    pub fn as_ptr(&self) -> *mut u8 {
        self.ptr.as_ptr()
    }
    
    /// Get slice of allocated memory
    pub fn as_slice(&self) -> &[u8] {
        unsafe {
            std::slice::from_raw_parts(self.ptr.as_ptr(), self.layout.size())
        }
    }
    
    /// Get mutable slice of allocated memory
    pub fn as_slice_mut(&mut self) -> &mut [u8] {
        unsafe {
            std::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.layout.size())
        }
    }
    
    /// Get size of allocated memory
    pub fn size(&self) -> usize {
        self.layout.size()
    }
    
    /// Zero memory contents
    pub fn zero(&mut self) {
        unsafe {
            ptr::write_bytes(self.ptr.as_ptr(), 0, self.layout.size());
        }
    }
    
    /// Fill memory with pattern
    pub fn fill(&mut self, pattern: u8) {
        unsafe {
            ptr::write_bytes(self.ptr.as_ptr(), pattern, self.layout.size());
        }
    }
}

impl Drop for MemoryBlock {
    fn drop(&mut self) {
        if self.config.zero_on_drop {
            self.zero();
        }
        
        unsafe {
            alloc::dealloc(self.ptr.as_ptr(), self.layout);
        }
        
        ALLOCATED_MEMORY.fetch_sub(self.layout.size(), Ordering::Relaxed);
    }
}

/// Secure memory operations
pub mod secure {
    use super::*;
    
    /// Memory block with secure operations
    #[derive(Debug)]
    pub struct SecureMemoryBlock {
        /// Inner memory block
        block: MemoryBlock,
        
        /// Lock status
        locked: bool,
    }
    
    impl SecureMemoryBlock {
        /// Create new secure memory block
        pub fn new(size: usize, config: MemoryConfig) -> Result<Self> {
            Ok(Self {
                block: MemoryBlock::new(size, config)?,
                locked: false,
            })
        }
        
        /// Lock memory to prevent swapping
        pub fn lock(&mut self) -> Result<()> {
            if !self.locked {
                #[cfg(unix)]
                unsafe {
                    if libc::mlock(self.block.ptr.as_ptr() as *const libc::c_void, self.block.size()) != 0 {
                        return Err(Error::MemoryError("Failed to lock memory".to_string()));
                    }
                }
                self.locked = true;
            }
            Ok(())
        }
        
        /// Unlock memory
        pub fn unlock(&mut self) -> Result<()> {
            if self.locked {
                #[cfg(unix)]
                unsafe {
                    if libc::munlock(self.block.ptr.as_ptr() as *const libc::c_void, self.block.size()) != 0 {
                        return Err(Error::MemoryError("Failed to unlock memory".to_string()));
                    }
                }
                self.locked = false;
            }
            Ok(())
        }
        
        /// Get reference to inner memory block
        pub fn inner(&self) -> &MemoryBlock {
            &self.block
        }
        
        /// Get mutable reference to inner memory block
        pub fn inner_mut(&mut self) -> &mut MemoryBlock {
            &mut self.block
        }
    }
    
    impl Drop for SecureMemoryBlock {
        fn drop(&mut self) {
            if self.locked {
                let _ = self.unlock();
            }
        }
    }
}

/// Memory pool for efficient allocation
#[derive(Debug)]
pub struct MemoryPool {
    /// Available blocks
    blocks: Vec<MemoryBlock>,
    
    /// Configuration
    config: MemoryConfig,
    
    /// Block size
    block_size: usize,
    
    /// Maximum blocks
    max_blocks: usize,
}

impl MemoryPool {
    /// Create new memory pool
    pub fn new(block_size: usize, max_blocks: usize, config: MemoryConfig) -> Self {
        Self {
            blocks: Vec::new(),
            config,
            block_size,
            max_blocks,
        }
    }
    
    /// Get memory block from pool
    pub fn acquire(&mut self) -> Result<MemoryBlock> {
        if let Some(block) = self.blocks.pop() {
            Ok(block)
        } else if self.blocks.len() < self.max_blocks {
            MemoryBlock::new(self.block_size, self.config.clone())
        } else {
            Err(Error::MemoryError("Memory pool exhausted".to_string()))
        }
    }
    
    /// Return memory block to pool
    pub fn release(&mut self, mut block: MemoryBlock) {
        if self.blocks.len() < self.max_blocks {
            block.zero();
            self.blocks.push(block);
        }
    }
    
    /// Clear memory pool
    pub fn clear(&mut self) {
        self.blocks.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_memory_block() {
        let config = MemoryConfig::default();
        let mut block = MemoryBlock::new(1024, config).unwrap();
        
        assert_eq!(block.size(), 1024);
        
        block.fill(0xFF);
        assert!(block.as_slice().iter().all(|&x| x == 0xFF));
        
        block.zero();
        assert!(block.as_slice().iter().all(|&x| x == 0));
    }
    
    #[test]
    fn test_secure_memory_block() {
        let config = MemoryConfig::default();
        let mut block = secure::SecureMemoryBlock::new(1024, config).unwrap();
        
        assert!(block.lock().is_ok());
        assert!(block.unlock().is_ok());
    }
    
    #[test]
    fn test_memory_pool() {
        let config = MemoryConfig::default();
        let mut pool = MemoryPool::new(1024, 2, config);
        
        let block1 = pool.acquire().unwrap();
        let block2 = pool.acquire().unwrap();
        assert!(pool.acquire().is_err());
        
        pool.release(block1);
        assert!(pool.acquire().is_ok());
        
        pool.release(block2);
        pool.clear();
        assert_eq!(pool.blocks.len(), 0);
    }
    
    #[test]
    fn test_memory_limit() {
        let config = MemoryConfig {
            memory_limit: 1024,
            ..Default::default()
        };
        
        assert!(MemoryBlock::new(512, config.clone()).is_ok());
        assert!(MemoryBlock::new(2048, config).is_err());
    }
}acquire().unwrap();
        assert!(pool.acquire().is_err());
        
        pool.release(block1);
        assert!(pool.acquire().is_ok());
        
        pool.release(block2);
        pool.clear();
        assert_eq!(pool.blocks.len(), 0);
    }
    
    #[test]
    fn test_memory_limit() {
        let config = MemoryConfig {
            memory_limit: 1024,
            ..Default::default()
        };
        
        assert!(MemoryBlock::new(512, config.clone()).is_ok());
        assert!(MemoryBlock::new(2048, config).is_err());
    }
}
