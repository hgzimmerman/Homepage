use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::io::BufReader;
use rocket::request::Request;
use rocket::response::{Response, Responder};
use rocket::http::{Status, ContentType};
use std::ops::Deref;
use std::io::Read;
use std::io::Result;
use std::io;
use rocket::response::NamedFile;
use std::result;
use std::io::Cursor;
use std::usize;

#[derive(Debug, Clone)]
pub struct CachedFile {
    path: PathBuf,
    bytes: Cursor<Vec<u8>>,
    size: usize
}

impl CachedFile {
    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<CachedFile> {
        let file = File::open(path.as_ref())?;
        let mut reader = BufReader::new(file);
        let mut buffer: Vec<u8> = vec!();
        let size: usize = reader.read_to_end(&mut buffer)?;

        Ok(CachedFile {
            path: path.as_ref().to_path_buf(),
            bytes: Cursor::new(buffer),
            size
        })
    }
}

/// Streams the named file to the client. Sets or overrides the Content-Type in
/// the response according to the file's extension if the extension is
/// recognized. See
/// [ContentType::from_extension](/rocket/http/struct.ContentType.html#method.from_extension)
/// for more information. If you would like to stream a file with a different
/// Content-Type than that implied by its extension, use a `File` directly.
impl Responder<'static> for CachedFile {
    fn respond_to(self, _: &Request) -> result::Result<Response<'static>, Status> {
        let mut response = Response::new();
        if let Some(ext) = self.path.extension() {
            if let Some(ct) = ContentType::from_extension(&ext.to_string_lossy()) {
                response.set_header(ct);
            }
        }

        response.set_streamed_body(self.bytes);
        Ok(response)
    }
}


impl From<NamedFile> for CachedFile {
    fn from(named_file: NamedFile) -> Self {
        let path = named_file.path().to_path_buf();
        let file: File = named_file.take_file();
        let mut reader = BufReader::new(file);
        let mut buffer: Vec<u8> = vec!();
        let size: usize = reader.read_to_end(&mut buffer).unwrap(); // TODO verify that this is safe.

        CachedFile {
            path,
            bytes: Cursor::new(buffer),
            size
        }
    }
}



pub struct Cache {
    size_limit: usize,
    file_map: HashMap<PathBuf, CachedFile>,
    access_count_map: HashMap<PathBuf, usize>
}

impl Cache {

    pub fn new(size_limit: usize) -> Cache {
        Cache {
            size_limit,
            file_map: HashMap::new(),
            access_count_map: HashMap::new()
        }
    }

    pub fn store(&mut self, path: PathBuf, file: CachedFile) {
        let possible_store_count: usize = self.access_count_map.get(&path).unwrap_or(&0usize) + 0;
        let lowest_tuple: (usize, PathBuf) = self.lowest_access_count_in_file_map();

//        if possible_store_count > lowest_tuple.0 {
//            if self.size() >= self.size_limit { // we have to remove one to make room.
//                self.file_map.remove(&lowest_tuple.1);
//                self.file_map.insert(path, file);
//            } else { // we can just add the file without removing another.
                info!("Inserting file: {:?} into cache.", path);
                self.file_map.insert(path, file);
//            }
//        }
    }

    pub fn get(&mut self, path: &PathBuf) -> Option<&CachedFile> {
        let count: &mut usize = self.access_count_map.entry(path.to_path_buf()).or_insert(0usize);
        *count += 1;
        self.file_map.get(path)
    }

    pub fn get_and_store(&mut self, pathbuf: PathBuf) -> Option<CachedFile> {

        let file: Option<CachedFile>;
        // First try to get the file in the cache that corresponds to the desired path.
        if let Some(cache_file) = self.get(&pathbuf) {
            info!("Cache hit for file: {:?}", pathbuf);
            return Some( cache_file.clone())
        };

        info!("Cache missed for file: {:?}", pathbuf);
        // Instead the file needs to read from the filesystem.
        let named_file: Result<NamedFile> = NamedFile::open(pathbuf.as_path());
        // Check if the file read was a success.
        if let Ok(file) = named_file {
            // If the file was read, convert it to a cached file and attempt to store it in the cache
            let cached_file: CachedFile = CachedFile::from(file);
            info!("Trying to add file {:?} to cache", pathbuf);
            self.store(pathbuf, cached_file.clone()); // possibly stores the cached file in the store.
            Some(cached_file)
        } else {
            // Indicate that the file was not found in either the filesystem or cache.
            None
        }
    }

    fn lowest_access_count_in_file_map(&self) -> (usize,PathBuf) {
        let mut lowest_access_count: usize = usize::MAX;
        let mut lowest_access_key: PathBuf = PathBuf::new();

        for file_key in self.file_map.keys() {
            let access_count: &usize = self.access_count_map.get(file_key).unwrap(); // It is guaranteed for the access count entry to exist if the file_map entry exists.
            if access_count < &lowest_access_count {
                lowest_access_count = access_count + 0;
                lowest_access_key = file_key.clone();
            }
        }
        (lowest_access_count, lowest_access_key)
    }

    fn size(&self) -> u32 {
        let mut size: u32 = 0;
        for _ in self.file_map.keys() {
            size += 1;
        }
        size
    }

}