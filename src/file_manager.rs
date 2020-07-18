extern crate ring;
// logger
extern crate slog;
extern crate slog_term;

// date time library
extern crate chrono;

use chrono::{DateTime, Utc};
use lazy_static::lazy_static;
use slog::Logger;
use slog::*;

use rusqlite::{params, Connection};
use std::fs::File;
use std::io::Read;
use std::sync::Mutex;
use std::{env, fs};

lazy_static! {
    pub static ref SINGLETON: Mutex<Option<FileManager>> = Mutex::new(None);
}

const FILE_ROOT: &str = "./server_files";

/// MD5_Digest for File:
fn md5_digest(mut filename: File) -> String {
    let mut data = Vec::new();
    filename.read_to_end(&mut data).unwrap();
    let digest = md5::compute(data);
    let md5_hash = format!("{:#X}", digest);
    md5_hash
}

#[derive(Debug, Clone)]
pub struct TFile {
    pub(crate) filename: String,
    pub(crate) path: String,
    pub(crate) hash: String,
    pub(crate) created: i64,
    pub(crate) locked: bool,
}

// PartialEQ for TFile, able to check if hash matches
impl PartialEq for TFile {
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash
    }
}

impl TFile {
    pub fn new_file(file: File, filename: String, _path: String, _hash: String) -> TFile {
        let _created = file.metadata().unwrap().created().unwrap();
        let converted_datetime = DateTime::<Utc>::from(_created).timestamp();

        TFile {
            filename,
            path: _path,
            hash: _hash,
            created: converted_datetime,
            locked: false,
        }
    }
}

#[derive(Debug)]
pub struct FileManager {
    files: Mutex<Vec<TFile>>,
}

impl FileManager {
    /// Initialize file manager should be used as the following, since its a singleton:
    /// # Examples
    /// ```
    /// FileManager::initialize(log.clone());
    /// ```
    pub fn initialize(log: Logger) {
        info!(log, "Initializing file manager");
        let conn = Connection::open("files.db").unwrap();

        conn.execute(
            "CREATE TABLE IF NOT EXISTS file (
                  id              INTEGER PRIMARY KEY,
                  filename        TEXT NOT NULL,
                  path            TEXT NOT NULL,
                  hash            TEXT NOT NULL,
                  locked          INTEGER
                  )",
            params![],
        )
        .unwrap();

        let mut st = SINGLETON.lock().unwrap();
        let vec = Mutex::new(Vec::new());
        if st.is_none() {
            *st = Some(FileManager { files: vec })
        } else {
            crit!(log, "File manager is already initialized");
        }
    }

    /// Get file manager
    /// # Examples
    /// ```
    /// FileManager::get(log.clone());
    /// ```
    pub fn get() -> &'static SINGLETON {
        if SINGLETON.lock().unwrap().is_some() {
            &SINGLETON
        } else {
            panic!("File manager must be initialized before use");
        }
    }

    /// Create file with file manager
    /// # Examples
    /// ```
    /// FileManager::get().lock().unwrap().as_mut().unwrap().create(
    ///     file,
    ///     file_name.unwrap().to_string(),
    ///     file_path.to_string(),
    ///     hash.to_string(),
    /// );
    /// ```
    pub fn create(&mut self, file: File, file_name: String, path: String, hash: String) -> bool {
        self.files
            .lock()
            .unwrap()
            .push(TFile::new_file(file, file_name, path, hash));
        true
    }

    pub fn lock_file(&mut self, file_name: &str, lock: bool) -> bool {
        let files = self.files.lock().unwrap().to_vec();

        for mut file in files {
            if file.filename == file_name.to_string() {
                file.locked = lock;
                return true;
            }
        }

        false
    }

    pub fn unlock_all_files(&mut self) {
        let files = self.files.lock().unwrap().to_vec();

        for mut file in files {
            file.locked = false;
        }
    }
    /// Create files from file manager
    /// # Examples
    /// ```
    /// let files = instance.as_ref().unwrap().list().to_vec();
    /// ```
    pub fn list(&self) -> Vec<TFile> {
        self.files.lock().unwrap().to_vec()
    }

    /// Get files from file manager
    /// # Examples
    /// ```
    /// FileManager::get()
    /// .lock()
    /// .unwrap()
    /// .as_mut()
    /// .unwrap()
    /// .get_files(log.clone());
    /// ```
    pub fn get_files(&mut self, log: Logger) {
        let mut root_path = env::current_dir().unwrap();
        root_path = root_path.join(FILE_ROOT);

        let path_exists = fs::metadata(&root_path).is_ok();

        if !path_exists {
            let create_dir = fs::create_dir(root_path.as_path());

            match create_dir {
                Ok(create_dir) => {
                    info!(log, "Created directory: {:?}: {:?}", create_dir, root_path.as_path());
                }
                Err(e) => {
                    error!(log, "Error occurred: {:?}", e);
                }
            }
        }

        let paths = fs::read_dir(root_path.as_path()).unwrap();

        for path in paths {
            let file_name = path.as_ref().unwrap().file_name().into_string().unwrap();
            let current_path = path
                .as_ref()
                .unwrap()
                .path()
                .into_os_string()
                .into_string()
                .unwrap();

            let file = File::open(path.unwrap().path()).unwrap();
            let _file = file.try_clone().unwrap();
            let md5_hash = md5_digest(file);

            info!(log, "MD5 hash is {:?}", md5_hash);
            info!(log, "Path: {:?}", current_path);

            let current_file = TFile {
                filename: (&file_name).parse().unwrap(),
                path: (&current_path).to_string(),
                hash: (&md5_hash).parse().unwrap(),
                created: 0,
                locked: false,
            };

            let conn = Connection::open("files.db").unwrap();

            conn.execute(
                "INSERT INTO file (filename, path, hash, locked) VALUES (?1, ?2, ?3, ?4)",
                params![
                    current_file.filename,
                    current_file.path,
                    current_file.hash,
                    current_file.locked
                ],
            )
            .unwrap();

            self.create(_file, file_name, current_path, md5_hash);
        }
    }
}
