#![feature(plugin)]
#![plugin(rocket_codegen)]

extern crate rocket;

use rocket::response::NamedFile;
use std::path::{Path, PathBuf};
use std::collections::HashMap;
use std::u32;
use rocket::request::State;
use std::sync::Mutex;
use std::fs::File;

mod my_named_file;
use my_named_file::MyNamedFile;

#[get("/")]
fn index() -> &'static str {
    "Hello, world!"
}

fn main() {
    let cache = Mutex::new(Cache::new(10));
    rocket::ignite()
        .manage(cache)
        .mount("/", routes![homepage_files])
        .launch();
}

#[get("/<path..>", rank=4)]
fn homepage_files(path: PathBuf, cache: State<Mutex<Cache>>) -> Option<MyNamedFile> {
    let pathbuf: PathBuf = Path::new("www/").join(path.clone()).to_owned();

    let file: Option<MyNamedFile>;
    match cache.lock().unwrap().get(&pathbuf) {
        Some(cache_file) => {
            file = Some(
                MyNamedFile::new(pathbuf, cache_file.try_clone().unwrap())
            );
            file
        },
        None => {
            // File not in cache
            // get the file from the filesystem
            file = MyNamedFile::open(pathbuf.as_path()).ok();
            if let Some(file) = file {
                let actual_f: File = file.file().try_clone().unwrap();
                cache.lock().unwrap().store(path, actual_f);
                Some(MyNamedFile::new(pathbuf, actual_f))
            } else {
                file
            }
        }
    }

}


struct Cache {
    size_limit: u32,
    file_map: HashMap<PathBuf, File>,
    access_count_map: HashMap<PathBuf, u32>
}

impl Cache {

    fn new(size: u32) -> Cache {
        Cache {
            size_limit: size,
            file_map: HashMap::new(),
            access_count_map: HashMap::new()
        }
    }

    fn store(&mut self, path: PathBuf, file: File) {
        let possible_store_count: u32 = self.access_count_map.get(&path).unwrap_or(&0u32) + 0;
        let lowest_tuple: (u32, PathBuf) = self.lowest_access_count_in_file_map();

        if possible_store_count > lowest_tuple.0 {
            if self.size() >= self.size_limit { // we have to remove one to make room.
                self.file_map.remove(&lowest_tuple.1);
                self.file_map.insert(path, file);
            } else { // we can just add the file without removing another.
                self.file_map.insert(path, file);
            }
        }
    }

    fn get(&mut self, path: &PathBuf) -> Option<&File> {
        let count: &mut u32 = self.access_count_map.entry(path.to_path_buf()).or_insert(0u32);
        *count += 1;
        self.file_map.get(path)
    }

    fn lowest_access_count_in_file_map(&self) -> (u32,PathBuf) {
        let mut lowest_access_count: u32 = u32::MAX;
        let mut lowest_access_key: PathBuf = PathBuf::new();

        for file_key in self.file_map.keys() {
            let access_count = self.access_count_map.get(file_key).unwrap(); // It is guaranteed for the access count entry to exist if the file_map entry exists.
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