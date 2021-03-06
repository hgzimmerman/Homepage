#![feature(plugin)]
#![plugin(rocket_codegen)]
#![feature(test)]

extern crate rocket;
#[macro_use]
extern crate log;
extern crate simplelog;
extern crate test;
extern crate rand;

extern crate rocket_file_cache;
//use rocket_file_cache::{Cache, CachedFile, CacheBuilder};
use rocket_file_cache::cached_file::CachedFile;
use rocket_file_cache::cache::Cache;
use rocket_file_cache::cache_builder::CacheBuilder;

use rocket::response::Redirect;
use rocket::Rocket;
use std::path::{Path, PathBuf};
use rocket::request::State;
use std::sync::Mutex;
use std::fs::File;

use simplelog::{Config, TermLogger, WriteLogger, CombinedLogger, LogLevelFilter};


#[get("/")]
fn index() -> Redirect {
    Redirect::to("/_.html")
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
    let cache: Mutex<Cache> = Mutex::new(
        CacheBuilder::new()
            .size_limit_bytes(1024 * 1024 * 20) // 20 MB limit
            .build()
            .unwrap()
    );

    rocket::ignite()
        .manage(cache)
        .mount("/", routes![homepage_files, index])
}

#[get("/<path..>", rank=4)]
fn homepage_files(path: PathBuf, cache: State<Mutex<Cache>>) -> Option<CachedFile> {
    let pathbuf: PathBuf = Path::new("www/").join(path.clone()).to_owned();
    cache.lock().unwrap().get_or_cache(pathbuf)
}



#[cfg(test)]
mod tests {
    extern crate test;
    use super::*;
    use rocket::local::Client;
    use test::Bencher;
    use rocket::response::NamedFile;

    #[bench]
    fn cache_access(b: &mut Bencher) {
        let client = Client::new(init_rocket()).expect("valid rocket instance");
        let _response = client.get("resources/linuxpenguin.jpg").dispatch(); // make sure the file is in the cache
        b.iter(|| {
            let mut response = client.get("resources/linuxpenguin.jpg").dispatch();
            let _body: Vec<u8> = response.body().unwrap().into_bytes().unwrap();
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
            let _body: Vec<u8> = response.body().unwrap().into_bytes().unwrap();
        });

    }

    // This bench was to confirm that all performance was lost in cloning the data structure storing the file.
    #[bench]
    fn clone2meg(b: &mut Bencher) {
        use rand::{StdRng, Rng};
        let mut megs2: [u8; 2000000] = [0u8; 2000000];
        StdRng::new().unwrap().fill_bytes(&mut megs2);
        b.iter( || {
            megs2.clone()
        });
    }
}