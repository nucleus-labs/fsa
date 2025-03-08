#[cfg(feature = "zip")]
use zip::result::ZipError;

use std::io::Error as IoError;

#[derive(derive_more::From, Debug)]
pub enum FsError {
    #[from]
    IoError(IoError),
    #[from]
    #[cfg(feature = "zip")]
    ZipError(ZipError),

    FileNotPresent(String, String),
    FileNotOpen(String),

    #[from]
    Generic(String),
}

impl std::fmt::Display for FsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FsError::IoError(error) => write!(f, "{error}"),
            #[cfg(feature = "zip")]
            FsError::ZipError(zerr) => write!(f, "{zerr}"),
            FsError::FileNotPresent(_in, name) => write!(f, "[{_in}] no file named '{name}'"),
            FsError::FileNotOpen(filename) => write!(f, "file '{filename}' is not open"),
            FsError::Generic(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for FsError {}
