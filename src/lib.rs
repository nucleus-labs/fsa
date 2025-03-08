//! Filesystem Abstractions (FSA)
//! 
//! This crate provides an abstraction layer for working with different types of filesystems,
//! including physical directories and files, as well as support for virtual ones. Virtual
//! filesystems are used to do things like have files in-memory, read a ZIP file's contents,
//! etc. It defines traits for generic file and directory handling, enabling a unified interface
//! for interacting with various storage backends.

pub mod physical;
pub mod error;
#[cfg(feature="zip")]
pub mod zip;

use std::io::{Write, BufRead, Seek};
use std::sync::{RwLock, Arc};
use std::path::PathBuf;

pub use physical::{PhysicalDirectory, PhysicalFile};
pub use error::FsError;

#[cfg(feature="zip")]
pub use zip::{ZipDirectory, ZipFile};

/// Result type used throughout the crate, wrapping `FsError`.
pub type Result<T> = std::result::Result<T, FsError>;

/// Represents a generic file abstraction that supports reading, writing, and seeking.
pub trait File: BufRead + Seek + Write {
    /// Returns a shared reference to the file wrapped in an `Arc<RwLock<dyn File>>`.
    fn get(&self) -> Arc<RwLock<dyn File>>;

    /// Returns the name of the directory.
    fn name(&self) -> String;
    /// Returns the stem (name without extension) of the file.
    fn stem(&self) -> String;
    /// Returns the file extension, if any.
    fn ext(&self) -> String;

    /// Returns the size of the file in bytes.
    fn size(&self) -> Result<usize>;
    /// Checks if the file exists in the filesystem.
    fn exists(&self) -> bool;
    /// Renames the file to the specified new name.
    fn rename(&self, name: &str) -> Result<()>;
    
    /// Retrieves the parent directory of the file.
    fn get_parent(&self) -> Arc<RwLock<dyn Directory>>;
    /// Returns the full path to the file.
    fn get_full_path(&self) -> PathBuf;

    /// Sets the buffer size for file operations.
    fn set_buffer_size(&mut self, size: usize);
    
    /// Opens the file, prepares it for read and write operations, and returns an I/O result.
    fn open(&mut self) -> std::io::Result<()>;
    /// Checks whether the file is currently opened.
    fn is_open(&self) -> bool;
    /// Closes the file, ensuring all changes are written and resources are released.
    fn close(&mut self);
}

/// Represents a generic directory abstraction that supports managing files and subdirectories.
pub trait Directory {
    /// Returns a shared reference to the directory wrapped in an `Arc<RwLock<dyn Directory>>`.
    fn get(&self) -> Arc<RwLock<dyn Directory>>;

    /// Returns the name of the directory.
    fn name(&self) -> String;
    /// Checks if the directory exists in the filesystem.
    fn exists(&self) -> bool;

    /// Retrieves the parent directory, if one exists.
    fn get_parent(&self) -> Option<Arc<RwLock<dyn Directory>>>;
    /// Returns the full path to the directory.
    fn get_full_path(&self) -> PathBuf;
    /// Retrieves a list of children (files and directories) within this directory.
    /// Uses cached results of [`Directory::scan`] if they exist.
    fn get_children(&self) -> Result<Vec<FilesystemObject>>;
    /// Retrieves a specific child (file or directory) by name.
    fn get_child(&self, name: &str) -> Result<FilesystemObject>;
    /// Checks if a child with the given name exists in the directory.
    fn has_child(&self, name: &str) -> Result<bool>;

    /// Creates a new file within the directory with the given name and buffer size.
    fn new_file(&mut self, name: String, buffer_size: usize) -> Result<Arc<RwLock<dyn File>>>;
    /// Creates a new subdirectory within this directory.
    fn new_dir(&mut self, name: String) -> Result<Arc<RwLock<dyn Directory>>>;

    /// Scans the directory contents and caches the results. Speeds up [`Directory::get_children`].
    fn scan(&mut self) -> Result<()> { unimplemented!() }
}

/// Represents a filesystem object, which can be either a file or a directory.
#[derive(Clone)]
pub enum FilesystemObject {
    File(Arc<RwLock<dyn File>>),
    Directory(Arc<RwLock<dyn Directory>>),
}

impl std::fmt::Display for dyn Directory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.get_full_path())
    }
}

impl std::fmt::Display for dyn File {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.get_full_path())
    }
}
