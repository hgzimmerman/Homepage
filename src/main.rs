#![feature(plugin)]
#![plugin(rocket_codegen)]
#![feature(test)]

extern crate rocket;
#[macro_use]
extern crate log;
extern crate simplelog;
extern crate test;
extern crate rand;


use rocket::response::NamedFile;
use rocket::Rocket;
use std::path::{Path, PathBuf};
use std::collections::HashMap;
use rocket::request::State;
use std::sync::Mutex;
use std::fs::File;
use std::io::Result;

use simplelog::{Config, TermLogger, WriteLogger, CombinedLogger, LogLevelFilter};


mod my_named_file;
use my_named_file::MyNamedFile;

mod cache;
use cache::*;

#[get("/")]
fn index() -> &'static str {
    "Hello, world!"
}

fn main() {

    const LOGFILE_NAME: &'static str = "homepage.log";
    CombinedLogger::init(
        vec![
            TermLogger::new(LogLevelFilter::Info, Config::default()).unwrap(),
            WriteLogger::new(LogLevelFilter::Trace, Config::default(), File::create(LOGFILE_NAME).unwrap()),
        ]
    ).unwrap();


    init_rocket().launch();
}

fn init_rocket() -> Rocket {
    let cache: Mutex<Cache> = Mutex::new(Cache::new(10));

    rocket::ignite()
        .manage(cache)
        .mount("/", routes![homepage_files])
}

#[get("/<path..>", rank=4)]
fn homepage_files(path: PathBuf, cache: State<Mutex<Cache>>) -> Option<CachedFile> {
    let pathbuf: PathBuf = Path::new("www/").join(path.clone()).to_owned();
    cache.lock().unwrap().get_and_store(pathbuf)
}



#[cfg(test)]
mod tests {
    extern crate test;
    use super::*;
    use rocket::local::Client;
    use rocket::http::Status;
    use test::Bencher;

    #[bench]
    fn cache_access(b: &mut Bencher) {
        let client = Client::new(init_rocket()).expect("valid rocket instance");
        let mut response = client.get("resources/linuxpenguin.jpg").dispatch(); // make sure the file is in the cache
        b.iter(|| {
            let mut response = client.get("resources/linuxpenguin.jpg").dispatch();
        });
    }


    fn init_file_rocket() -> Rocket {
        rocket::ignite()
            .mount("/", routes![files])
    }

    #[get("/<file..>")]
    fn files(file: PathBuf) -> Option<NamedFile> {
        NamedFile::open(Path::new("www/").join(file)).ok()
    }

    #[bench]
    fn file_access(b: &mut Bencher) {
        let client = Client::new(init_file_rocket()).expect("valid rocket instance");
        b.iter(|| {
            let mut response = client.get("resources/linuxpenguin.jpg").dispatch();
        });
    }

    // This bench was to confirm that all performance was lost in cloning the datastructure storing the file.
    #[bench]
    fn clone2meg(b: &mut Bencher) {
        use rand::{StdRng, Rng};
        let mut megs2 = [0u8; 2000000];
        StdRng::new().unwrap().fill_bytes(&mut megs2);
        b.iter( || {
            megs2.clone()
        });
    }
}