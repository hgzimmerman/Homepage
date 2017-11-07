use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::io::BufReader;
use rocket::request::Request;
use rocket::response::{Response, Responder};
use rocket::http::{Status, ContentType};
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

/// Alternative implementation for sending the file via a reference.
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
        let size: usize = reader.read_to_end(&mut buffer).unwrap();

        CachedFile {
            path,
            bytes: buffer,
            size
        }
    }
}



pub struct Cache {
    size_limit: usize, // Currently this is being used as the number of elements in the cache, but should be used as the number of bytes in the hashmap.
    // TODO see if another datatype could reduce the need to store the PathBuf inside of the CachedFile as well. The CachedFile could be reconstiuted via references when it exits the hashmap.
    file_map: HashMap<PathBuf, CachedFile>, // Holds the files that the cache is caching
    access_count_map: HashMap<PathBuf, usize> // Every file that is accessed will have the number of times it is accessed logged in this map.
}

impl fmt::Display for Cache {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // TODO because the entries are unsorted, it is not guaranteed that the access counts will correspond to the paths.
        f.debug_list().entries(
            self.file_map.iter().zip(self.access_count_map.iter()).map(|x| {
                let size = (x.0).1.size;
                let count = (x.1).1;
                let path = &(x.0).1.path;

                (path, size, count)
        })
        ).finish()
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

    /// Attempt to store a given file in the the cache.
    /// Storing will fail if the current files have more access attempts than the file being added.
    /// If the provided file has more more access attempts than one of the files in the cache,
    /// but the cache is full, a file will have to be removed from the cache to make room
    /// for the new file.
    pub fn store(&mut self, path: PathBuf, file: CachedFile) -> result::Result<(), String> {

        // If there is room in the hashmap, just add the file
        if self.size() < self.size_limit {
            self.file_map.insert(path.clone(), file);
            info!("Inserting a file: {:?} into a not-full cache.", path);
            return Ok(()) // Inserted successfully.
        }

        match self.lowest_access_count_in_file_map() {
            Some(lowest) => {
                let (lowest_count, lowest_key) = lowest;
                // It should early return if a file can be added without having to remove a file first.
                let possible_store_count: usize = *self.access_count_map.get(&path).unwrap_or(&0usize);
                // Currently this removes the file that has been accessed the least.
                // TODO in the future, this should remove the file that has the lowest "score"
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
    }

    /// Increments the access count.
    /// Gets the file from the cache if it exists.
    pub fn get(&mut self, path: &PathBuf) -> Option<&CachedFile> {
        let count: &mut usize = self.access_count_map.entry(path.to_path_buf()).or_insert(0usize);
        *count += 1; // Increment the access count
        self.file_map.get(path)
    }

    /// Either gets the file from the cache, gets it from the filesystem and tries to cache it,
    /// or fails to find the file and returns None.

    pub fn get_or_cache(&mut self, pathbuf: PathBuf) -> Option<CachedFile> {
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
            let _ = self.store(pathbuf, cached_file.clone()); // possibly stores the cached file in the store.
            Some(cached_file)
        } else {
            // Indicate that the file was not found in either the filesystem or cache.
            None
        }
    }

    /// Gets the file with the lowest access count in the hashmap.
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

    /// Gets the number of files in the file_map.
    fn size(&self) -> usize {
        let mut size: usize = 0;
        for _ in self.file_map.keys() {
            size += 1;
        }
        size
    }

    /// gets the size of the files in the file_map.
    fn size_bytes(&self) -> usize {
//        let mut size: usize = 0;
        self.file_map.iter().fold(0usize, |size, x| {
           size +  x.1.size
        })
    }

}