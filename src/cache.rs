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
use std::fmt;

#[derive(Debug, Clone)]
pub struct CachedFile {
    path: PathBuf,
    bytes: Vec<u8>,
    size: usize
}

impl fmt::Display for CachedFile {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{{Path: {:?}, Size: {}}}", self.path, self.size)

    }
}

impl CachedFile {
    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<CachedFile> {
        let file = File::open(path.as_ref())?;
        let mut reader = BufReader::new(file);
        let mut buffer: Vec<u8> = vec!();
        let size: usize = reader.read_to_end(&mut buffer)?;

        Ok(CachedFile {
            path: path.as_ref().to_path_buf(),
            bytes: buffer,
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

        response.set_streamed_body(Cursor::new(self.bytes));
        Ok(response)
    }
}

impl <'a>Responder<'a> for &'a CachedFile {
    fn respond_to(self, _: &Request) -> result::Result<Response<'a>, Status> {
        let mut response = Response::new();
        if let Some(ext) = self.path.extension() {
            if let Some(ct) = ContentType::from_extension(&ext.to_string_lossy()) {
                response.set_header(ct);
            }
        }

        response.set_streamed_body(self.bytes.as_slice());
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
            bytes: buffer,
            size
        }
    }
}



pub struct Cache {
    size_limit: usize, // Currently this is being used as the number of elements in the cache, but should be used as the number of bytes in the hashmap.
    file_map: HashMap<PathBuf, CachedFile>,
    access_count_map: HashMap<PathBuf, usize>
}

impl fmt::Display for Cache {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_map().entries(
            self.file_map.iter().zip(self.access_count_map.iter()).map(|x| {
                let size = (x.0).1.size;
                let count = (x.1).1;
                let path = &(x.0).1.path;

                ((path, size), (x.1).1)
        })
//                .zip(self.access_count_map.iter())
        ).finish()
//        write!(f, "({}, {})", self., self.y)
    }
}

impl Cache {

    pub fn new(size_limit: usize) -> Cache {
        Cache {
            size_limit,
            file_map: HashMap::new(),
            access_count_map: HashMap::new()
        }
    }

    pub fn store(&mut self, path: PathBuf, file: CachedFile) -> result::Result<(), String> {
        let possible_store_count: usize = self.access_count_map.get(&path).unwrap_or(&0usize) + 0;

        match self.lowest_access_count_in_file_map() {
            Some(lowest) => {
                let (lowest_count, lowest_key) = lowest;
                // If there is room in the hashmap, just add the file
                // TODO consider moving this outside of the match statement.
                if self.size() < self.size_limit {
                    self.file_map.insert(path.clone(), file);
                    info!("Inserting a file: {:?} into cache.", path);
                    return Ok(()) // Inserted successfully.
                }
                // It should early return if a file can be added without having to remove a file first.
                // Currently this removes the file that has been accessed the least.
                if possible_store_count > lowest_count {
                    self.file_map.remove(&lowest_key);
                    self.file_map.insert(path.clone(), file);
                    info!("Removing file: {:?} to make room for file: {:?}.", lowest_key, path);
                    return Ok(())
                } else {
                    info!("File: {:?} has less demand than files already in the cache.", path);
                    return Err(String::from("File demand for file is lower than files already in the cache"));
                }
            }
            None => {
                info!("Inserting first file: {:?} into cache.", path);
                self.file_map.insert(path, file);
                Ok(())
            }
        }

//        if possible_store_count > lowest_tuple.0 {
//            if self.size() >= self.size_limit { // we have to remove one to make room.
//                self.file_map.remove(&lowest_tuple.1);
//                self.file_map.insert(path, file);
//            } else { // we can just add the file without removing another.

//            }
//        }
    }

    pub fn get(&mut self, path: &PathBuf) -> Option<&CachedFile> {
        let count: &mut usize = self.access_count_map.entry(path.to_path_buf()).or_insert(0usize);
        *count += 1;
        self.file_map.get(path)
    }

    pub fn get_and_store(&mut self, pathbuf: PathBuf) -> Option<CachedFile> {
//        info!("Cache: {}", self);

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

    fn lowest_access_count_in_file_map(&self) -> Option<(usize,PathBuf)> {
        if self.file_map.keys().len() == 0 {
            return None
        }

        let mut lowest_access_count: usize = usize::MAX;
        let mut lowest_access_key: PathBuf = PathBuf::new();

        for file_key in self.file_map.keys() {
            let access_count: &usize = self.access_count_map.get(file_key).unwrap(); // It is guaranteed for the access count entry to exist if the file_map entry exists.
            if access_count < &lowest_access_count {
                lowest_access_count = access_count + 0;
                lowest_access_key = file_key.clone();
            }
        }
        Some((lowest_access_count, lowest_access_key))
    }

    fn size(&self) -> usize {
        let mut size: usize = 0;
        for _ in self.file_map.keys() {
            size += 1;
        }
        size
    }

    fn size_bytes(&self) -> usize {
//        let mut size: usize = 0;
        self.file_map.iter().fold(0usize, |size, x| {
           size +  x.1.size
        })
    }

}