//! The [zip](`self`) module provides an abstraction over [`ZipArchive`]s and [`ZipFile`]s from the
//! [`zip`] crate

use zip::ZipArchive;

use std::io::{
    Result as IoResult, Error as IoError, ErrorKind as IoErrorKind,
    BufRead, Write, Read,
    Seek, SeekFrom,
};
use std::cell::{OnceCell, RefCell};
use std::sync::{RwLock, Weak, Arc};
use std::path::{PathBuf, Path};
use std::collections::HashMap;
use std::fs;

use super::{
    FilesystemObject, Directory, File,
    FsError, Result,
};

pub struct ZipDirectory {
    name: PathBuf,
    parent: Arc<RwLock<dyn Directory>>,

    children: RefCell<HashMap<String, Arc<RwLock<ZipFile>>>>,
    scanned: RefCell<bool>,

    archive: Arc<RwLock<ZipArchive<fs::File>>>,
    handle: OnceCell<Weak<RwLock<Self>>>,
}

pub struct ZipFile {
    name: PathBuf,
    file_index: usize,
    parent: Arc<RwLock<ZipDirectory>>,

    buffer: Vec<u8>,
    seek_offset: i64,
    buf_filled: usize,
    cursor: usize,

    handle: OnceCell<Weak<RwLock<Self>>>,
}

impl ZipDirectory {
    pub fn new(file: Arc<RwLock<super::PhysicalFile>>) -> Result<Arc<RwLock<Self>>> {
        let (name, parent, file) = {
            let mut file_guard = file.write().unwrap();
            let path = Path::new(&file_guard.name()).to_path_buf();
            let parent = file_guard.get_parent();
            let file_handle = if let Some(fh) = file_guard.take_handle() {
                fh
            } else {
                file_guard.open()?;
                file_guard.take_handle().unwrap()
            };
            (path, parent, file_handle)
        };

        let archive = Arc::new(RwLock::new(ZipArchive::new(file)?));

        let new = Self{
            name,
            parent,

            children: RefCell::new(HashMap::new()),
            scanned: RefCell::new(false),

            archive,
            handle: OnceCell::new(),
        };

        let arc = Arc::new(RwLock::new(new));
        arc.write().unwrap().handle.set(Arc::downgrade(&arc)).unwrap();

        Ok(arc)
    }

    pub fn get_archive(&self) -> Arc<RwLock<ZipArchive<fs::File>>> {
        self.archive.clone()
    }
}

impl ZipFile {
    fn new(name: &Path, parent: Arc<RwLock<ZipDirectory>>, buffer_size: usize) -> Arc<RwLock<Self>> {
        let archive = parent.read().unwrap().get_archive();
        let archive_handle = archive.read().unwrap();
        let file_index = archive_handle.index_for_name(name.to_str().unwrap()).unwrap();

        let new = Self{
            name: name.to_path_buf(),
            file_index,
            parent,

            buffer: Vec::with_capacity(buffer_size),
            seek_offset: 0,
            buf_filled: 0,
            cursor: 0,

            handle: OnceCell::new(),
        };

        let arc = Arc::new(RwLock::new(new));
        arc.write().unwrap().handle.set(Arc::downgrade(&arc)).unwrap();

        arc
    }

    pub fn get_archive(&self) -> Arc<RwLock<ZipArchive<fs::File>>> {
        self.parent.read().unwrap().get_archive()
    }

    fn fill_buffer(&mut self) -> IoResult<()> {
        let archive = self.get_archive();
        let mut archive_handle = archive.write().unwrap();
        let mut file = archive_handle.by_index_seek(self.file_index).unwrap();

        file.seek_relative(self.seek_offset)?;
        self.buf_filled = file.read(&mut self.buffer)?;
        self.seek_offset += self.buf_filled as i64;
        self.cursor = 0;

        Ok(())
    }
}

impl Directory for ZipDirectory {
    fn get(&self) -> Arc<RwLock<dyn Directory>> {
        self.handle.get().unwrap().upgrade().unwrap()
    }
    
    fn name(&self) -> String {
        self.name.to_string_lossy().to_string()
    }

    fn exists(&self) -> bool {
        let path = self.get_full_path();
        path.exists() && path.is_dir()
    }

    fn get_parent(&self) -> Option<Arc<RwLock<dyn Directory>>> {
        Some(self.parent.clone())
    }
    
    fn get_full_path(&self) -> PathBuf {
        self.parent.read().unwrap().get_full_path().join(&self.name)
    }

    fn get_children(&self) -> Result<Vec<FilesystemObject>> {
        if *self.scanned.borrow() {
            let children: Vec<FilesystemObject> = self.children.borrow().values()
                .map(|child| FilesystemObject::File(child.clone()))
                .collect();

            Ok(children)
        } else {
            let handle = self.handle.get().unwrap().upgrade().unwrap();
            let children = self.archive.read().unwrap().file_names()
                .map(|name| {
                    FilesystemObject::File(ZipFile::new(Path::new(name), handle.clone(), 512))
                })
                .collect::<Vec<_>>();
            Ok(children)
        }
    }

    fn get_child(&self, name: &str) -> Result<FilesystemObject> {
        let children = self.children.borrow();
        let lookup_result = children.get(name)
            .map(|child| FilesystemObject::File(child.clone()))
            .ok_or_else(|| FsError::FileNotPresent(self.get_full_path().to_string_lossy().to_string(), name.to_string()))?;

        Ok(lookup_result.clone())
    }
    
    fn has_child(&self, name: &str) -> Result<bool> {
        Ok(self.children.borrow().contains_key(name))
    }

    fn new_file(&mut self, _name: String, _buffer_size: usize) -> Result<Arc<RwLock<dyn File>>> {
        unimplemented!()
    }

    fn new_dir(&mut self, _name: String) -> Result<Arc<RwLock<dyn Directory>>> {
        unimplemented!()
    }

    fn scan(&mut self) -> Result<()> {
        if !*self.scanned.borrow() && self.exists() {
            let handle = self.handle.get().unwrap().upgrade().unwrap();
            let archive = self.archive.clone();
            let mut archive_handle = archive.write().unwrap();

            let mut children = self.children.borrow_mut();
            for i in 0..archive_handle.len() {
                let file = archive_handle.by_index(i)?;
                if file.is_file() {
                    let child_name = file.name();
                    let child = ZipFile::new(Path::new(child_name), handle.clone(), 512);
                    children.insert(child_name.to_string(), child);
                }
            }

            *self.scanned.borrow_mut() = true;
        }

        Ok(())
    }
}

impl Read for ZipFile {
    fn read(&mut self, buf: &mut [u8]) -> IoResult<usize> {
        if self.cursor >= self.buf_filled {
            self.fill_buffer()?;
        }

        let byte_count = (self.buf_filled - self.cursor).min(buf.len());
        let end = self.cursor + byte_count;

        buf[..byte_count].copy_from_slice(&self.buffer[self.cursor..end]);
        self.cursor = end;

        Ok(byte_count)
    }
}

impl BufRead for ZipFile {
    fn fill_buf(&mut self) -> IoResult<&[u8]> {
        if self.cursor >= self.buf_filled {
            self.fill_buffer()?;
        }

        Ok(&self.buffer[self.cursor..self.buf_filled])
    }

    fn consume(&mut self, amt: usize) {
        self.cursor = (self.cursor + amt).min(self.buf_filled);
    }
}

impl Seek for ZipFile {
    fn seek(&mut self, pos: SeekFrom) -> IoResult<u64> {
        self.buf_filled = 0;
        self.cursor = 0;

        match pos {
            SeekFrom::Start(pos) => self.seek_offset = pos as i64,
            SeekFrom::End(pos) => {
                let archive = self.get_archive();
                let mut archive_handle = archive.write().unwrap();
                let file = archive_handle.by_index(self.file_index).unwrap();

                self.seek_offset = file.size() as i64 + pos;
            },
            SeekFrom::Current(pos) => self.seek_offset += pos,
        }

        if self.seek_offset < 0 {
            return Err(IoError::new(IoErrorKind::InvalidInput, "Invalid seek offset"));
        }

        Ok(self.seek_offset as u64)
    }
}

impl Write for ZipFile {
    fn write(&mut self, _buf: &[u8]) -> IoResult<usize> {
        unimplemented!()
    }

    fn flush(&mut self) -> IoResult<()> {
        unimplemented!()
    }
}

impl File for ZipFile {
    fn get(&self) -> Arc<RwLock<dyn File>> {
        self.handle.get().unwrap().upgrade().unwrap()
    }
    
    fn name(&self) -> String {
        self.name.to_string_lossy().to_string()
    }
    
    fn stem(&self) -> String {
        self.name.file_stem().unwrap().to_string_lossy().to_string()
    }
    
    fn ext(&self) -> String {
        self.name.extension().unwrap().to_string_lossy().to_string()
    }
    
    fn size(&self) -> Result<usize> {
        let size = self.get_archive()
            .write().unwrap()
            .by_index_raw(self.file_index).unwrap()
            .size();
        Ok(size as usize)
    }

    fn exists(&self) -> bool {
        true
    }

    fn rename(&self, _name: &str) -> Result<()> {
        unimplemented!()
    }

    fn get_parent(&self) -> Arc<RwLock<dyn Directory>> {
        self.parent.clone()
    }

    fn get_full_path(&self) -> PathBuf {
        self.parent.read().unwrap().get_full_path()
    }
    
    fn set_buffer_size(&mut self, size: usize) {
        self.buffer.resize(size, 0);
    }
    
    fn open(&mut self) -> IoResult<()> {
        Ok(())
    }

    fn is_open(&self) -> bool {
        true
    }

    fn close(&mut self) { }
}
