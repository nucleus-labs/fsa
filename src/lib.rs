//! Filesystem Abstractions (FSA)
//! 
//! This crate provides an abstraction layer for working with different types of filesystems,
//! including physical directories and files, as well as support for virtual ones. Virtual
//! filesystems are used to do things like have files in-memory, read a ZIP file's contents,
//! etc. It defines traits for generic file and directory handling, enabling a unified interface
//! for interacting with various storage backends.

// pub mod physical;
pub mod error;
// pub mod virt;
// #[cfg(feature="zip")]
// pub mod zip;

use std::io::{Write, BufRead, Seek};
use std::sync::{RwLock, Arc};
use std::path::{Path, PathBuf};

// pub use physical::{PhysicalDirectory, PhysicalFile};
// pub use virt::{VirtualDirectory, VirtualFile};
pub use error::FsError;

// #[cfg(feature="zip")]
// pub use zip::{ZipDirectory, ZipFile};

/// Result type used throughout the crate, wrapping `FsError`.
pub type FsResult<T = ()> = std::result::Result<T, FsError>;

pub type FilesystemObject = Arc<RwLock<dyn FilesystemObjectRaw>>;

pub trait FilesystemObjectRaw: BufRead + Seek + Write {
    /// Returns a shared reference to the object as a FilesystemObject.
    fn get(&self) -> FilesystemObject;

    /// Returns the name of the object.
    fn name(&self) -> &Path;
    /// Returns the stem (name without extension) of the object.
    fn stem(&self) -> &str { return self.name().file_stem().unwrap().to_str().unwrap() }
    /// Returns the name's extension, if any.
    fn ext(&self) -> Option<&str> { return self.name().extension().map(|x| x.to_str().unwrap()) }

    /// If file, returns the size of the file in bytes. Else, errors.
    fn size(&self) -> FsResult<usize>;
    
    /// Retrieves the parent object of the object.
    fn get_parent(&self) -> FilesystemObject;
    /// Returns the full path to the object.
    fn get_full_path(&self) -> PathBuf;

    /// Moves an object from its current directory to the one provided.
    fn move_to(&mut self, new_dir: FilesystemObject) -> FsResult;
    
    /// If file, opens the file, prepares it for read and write operations, and returns an I/O result.
    /// Else, errors.
    fn open(&mut self) -> std::io::Result<()>;
    /// If file, checks whether the file is currently opened. Else, errors.
    fn is_open(&self) -> bool;
    /// If file, closes the file, ensuring all changes are written and resources are released.
    /// Else, errors.
    fn close(&mut self);

    /// If directory, retrieves a list of children (files and directories) within this directory.
    /// Uses cached results of [`Directory::scan`] if they exist.
    /// Else (not directory), then error.
    fn get_children(&self) -> FsResult<Vec<Box<dyn FilesystemObjectRaw>>>;
    /// Retrieves a specific child (file or directory) by name.
    fn get_child(&self, name: &str) -> FsResult<Box<dyn FilesystemObjectRaw>>;
    /// Checks if a child with the given name exists in the directory.
    fn has_child(&self, name: &str) -> FsResult<bool>;

    /// Renames the file to the specified new name.
    fn child_rename(&mut self, name: &str, new_name: &str) -> FsResult;

    /// Creates a new file within the directory with the given name and buffer size.
    fn new_file(&mut self, name: &str, buffer_size: usize) -> FsResult<FilesystemObject>;
    /// Creates a new subdirectory within this directory.
    fn new_dir(&mut self, name: &str) -> FsResult<FilesystemObject>;

    /// Invalidate (and drop) the cached info for a child, if applicable. Typically used for moving
    /// a child from one parent to another, or deleting a child
    fn drop_child(&mut self, name: &str) -> FsResult;
    /// Sets the buffer size for file operations.
    fn set_buffer_size(&mut self, size: usize);

    /// Scans the directory contents and caches the results. Speeds up [`Directory::get_children`].
    fn scan(&mut self) -> FsResult<()> { unimplemented!() }

    /// Deletes the file. Unsure how to handle this since it should invalidate all active handles,
    /// but that's not actually possible with existing types.
    /// TODO: Look into RwLock with an integrated Option?
    fn delete(&mut self) -> FsResult { unimplemented!() }
}

impl std::fmt::Display for dyn FilesystemObjectRaw {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.get_full_path())
    }
}
