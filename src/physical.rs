use std::io::{BufRead, Write, Read, Seek};
use std::cell::{OnceCell, RefCell};
use std::sync::{RwLock, Weak, Arc};
use std::path::{PathBuf, Path};
use std::collections::HashMap;
use std::fs;

use super::{
    FilesystemObject, Directory, File,
    FsError, Result,
};

pub struct PhysicalDirectory {
    name: PathBuf,
    parent: Option<Arc<RwLock<dyn Directory>>>,

    children: RefCell<HashMap<String, FilesystemObject>>,
    scanned: RefCell<bool>,

    handle: OnceCell<Weak<RwLock<Self>>>,
}

pub struct PhysicalFile {
    name: PathBuf,
    parent: Arc<RwLock<dyn Directory>>,

    file: Option<fs::File>,
    buffer: Vec<u8>,
    buf_filled: usize,
    cursor: usize,

    handle: OnceCell<Weak<RwLock<Self>>>,
}

impl PhysicalDirectory {
    fn new(name: &Path, parent: Option<Arc<RwLock<dyn Directory>>>) -> Arc<RwLock<Self>> {
        let new = Self{
            name: name.to_path_buf(),
            parent,

            children: RefCell::new(HashMap::new()),
            scanned: RefCell::new(false),

            handle: OnceCell::new(),
        };

        let arc = Arc::new(RwLock::new(new));
        arc.write().unwrap().handle.set(Arc::downgrade(&arc)).unwrap();

        arc
    }

    fn scan(&self) -> Result<()> {
        if !*self.scanned.borrow() && self.exists() {
            let handle = self.get();
            let mut children = self.children.borrow_mut();
            
            for item in fs::read_dir(self.get_full_path())? {
                let item = item?;
                let file_type: fs::FileType = item.file_type()?;
                if file_type.is_dir() {
                    let child_name = item.file_name();
                    let child = PhysicalDirectory::new(Path::new(&child_name), Some(handle.clone()));
                    children.insert(child_name.into_string().unwrap(), FilesystemObject::Directory(child));
                } else if file_type.is_file() {
                    let child_name = item.file_name();
                    let child = PhysicalFile::new(Path::new(&child_name), handle.clone(), 0);
                    children.insert(child_name.into_string().unwrap(), FilesystemObject::File(child));
                }
            }

            *self.scanned.borrow_mut() = true;
        }

        Ok(())
    }
}

impl PhysicalFile {
    fn new(name: &Path, parent: Arc<RwLock<dyn Directory>>, buffer_size: usize) -> Arc<RwLock<Self>> {
        let new = Self{
            name: name.to_path_buf(),
            parent,

            file: None,
            buffer: Vec::with_capacity(buffer_size),
            buf_filled: 0,
            cursor: 0,

            handle: OnceCell::new(),
        };

        let arc = Arc::new(RwLock::new(new));
        arc.write().unwrap().handle.set(Arc::downgrade(&arc)).unwrap();

        arc
    }

    pub fn get_handle(&self) -> Option<&fs::File> {
        self.file.as_ref()
    }

    pub fn take_handle(&mut self) -> Option<fs::File> {
        self.file.take()
    }

    fn fill_buffer(&mut self) -> std::io::Result<usize> {
        if !self.is_open() {
            self.open()?;
            if self.buffer.capacity() == 0 {
                let file_size = self.file.as_ref().unwrap().metadata().unwrap().len() as usize;
                self.buffer.resize(file_size, 0);
            }
        }

        self.cursor = 0;
        self.buf_filled = self.file.as_ref().unwrap().read(&mut self.buffer)?;

        Ok(self.buf_filled)
    }
}

impl Directory for PhysicalDirectory {
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
        self.parent.as_ref().map(|item| item.clone())
    }
    
    fn get_full_path(&self) -> PathBuf {
        match &self.parent {
            Some(parent)
                => parent.read().unwrap().get_full_path().join(&self.name),
            None => self.name.clone().into(),
        }
    }

    fn get_children(&self) -> Result<Vec<FilesystemObject>> {
        self.scan()?;

        let children: Vec<FilesystemObject> = self.children.borrow().values()
            .map(|child| child.clone())
            .collect();

        Ok(children)
    }

    fn get_child(&self, name: &str) -> Result<FilesystemObject> {
        let children = self.children.borrow();
        let lookup_result = children.get(name)
            .ok_or_else(|| FsError::FileNotPresent(self.get_full_path().to_string_lossy().to_string(), name.to_string()))?;

        Ok(lookup_result.clone())
    }
    
    fn has_child(&self, name: &str) -> Result<bool> {
        Ok(self.children.borrow().contains_key(name))
    }
    
    fn new_file(&mut self, name: String, buffer_size: usize) -> Result<Arc<RwLock<dyn File>>> {
        let file = PhysicalFile::new(Path::new(&name), self.get(), buffer_size);
        self.children.borrow_mut().insert(name, FilesystemObject::File(file.clone()));
        Ok(file)
    }
    
    fn new_dir(&mut self, name: String) -> Result<Arc<RwLock<dyn Directory>>> {
        let file = PhysicalDirectory::new(Path::new(&name), Some(self.get()));
        self.children.borrow_mut().insert(name, FilesystemObject::Directory(file.clone()));
        Ok(file)
    }
}

impl Read for PhysicalFile {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
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

impl BufRead for PhysicalFile {
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        if self.cursor >= self.buf_filled {
            self.fill_buffer()?;
        }

        Ok(&self.buffer[self.cursor..self.buf_filled])
    }

    fn consume(&mut self, amt: usize) {
        self.cursor = (self.cursor + amt).min(self.buf_filled);
    }
}

impl Seek for PhysicalFile {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        self.file.as_ref().unwrap().seek(pos)
    }
}

impl Write for PhysicalFile {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.buf_filled = 0;
        self.cursor = 0;

        let byte_count = self.buffer.len().min(buf.len());
        self.buffer[..byte_count].copy_from_slice(&buf[..byte_count]);
        
        Ok(byte_count)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.open()?;

        let mut handle = self.file.as_ref().unwrap();
        handle.write(&self.buffer)?;
        handle.flush()?;

        Ok(())
    }
}

impl File for PhysicalFile {
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
        todo!()
    }

    fn exists(&self) -> bool {
        let path = self.get_full_path();
        path.exists() && path.is_file()
    }

    fn rename(&self, name: &str) -> Result<()> {
        let parent_dir = self.parent.read().unwrap().get_full_path();
        fs::rename(&parent_dir.join(Path::new(&self.name)), &parent_dir.join(name))?;

        Ok(())
    }

    fn get_parent(&self) -> Arc<RwLock<dyn Directory>> {
        self.parent.clone()
    }

    fn get_full_path(&self) -> PathBuf {
        self.parent.read().unwrap().get_full_path().join(&self.name)
    }
    
    fn set_buffer_size(&mut self, size: usize) {
        self.buffer.resize(size, 0);
    }
    
    fn open(&mut self) -> std::io::Result<()> {
        if self.file.is_some() {
            self.close();
        }

        self.file = Some(fs::File::open(self.get_full_path())?);
        Ok(())
    }

    fn is_open(&self) -> bool {
        self.file.is_some() && self.file.as_ref().unwrap().metadata().is_ok()
    }
    
    fn close(&mut self) {
        self.file = None;
        self.buf_filled = 0;
        self.cursor = 0;
    }
}
