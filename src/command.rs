use crate::file_manager::FileManager;

use slog::Logger;
use slog::*;
use std::convert::TryInto;
use std::fs::File;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream};
use std::str;

const BUFFER_SIZE: usize = 1024;

// Command
#[derive(Debug)]
pub struct Command {
    method: String,
    value: String,
    hash: String,
}

impl Command {
    pub fn new(method: &str, value: &str, hash: &str) -> Command {
        Command {
            method: method.to_string(),
            value: value.to_string(),
            hash: hash.to_string(),
        }
    }

    // execute all methods
    pub fn execute_method(self, mut stream: TcpStream, log: Logger) {
        info!(log, "Executing method: {}", self.method);

        if self.method == "LIST" {
            let instance = FileManager::get().lock().unwrap();
            let files = instance.as_ref().unwrap().list().to_vec();

            let mut content_length = String::from("AFTP/1.0 200 OK\nContent-Length: 100\n");

            for _file in files {
                let t: String = _file.created.to_string();
                let _str = _file.filename + " " + &t + " " + &_file.hash + "\n";
                content_length.push_str(&_str);
            }

            stream.write_all(content_length.as_bytes()).unwrap();
            stream
                .shutdown(Shutdown::Both)
                .expect("failed to shutdown stream");
        }

        if self.method == "PUT" {
            // only difference is: hash check
            info!(log, "PUT method: {}", self.method);
            info!(log, "Value      : {}", self.value);
            info!(log, "Hash       : {}", self.hash);

            let mut buf = Vec::with_capacity(BUFFER_SIZE);
            let stream = stream.try_clone().unwrap();

            FileManager::get()
                .lock()
                .unwrap()
                .as_mut()
                .unwrap()
                .get_files(log.clone());

            match stream
                .take(BUFFER_SIZE.try_into().unwrap())
                .read_to_end(&mut buf)
            {
                Ok(_) => {
                    info!(log, "{:?}", buf);
                }
                Err(e) => {
                    info!(log, "Failed to receive data: {}", e);
                }
            }
        }

        if self.method == "DELETE" {
            let mut n = "404\n";
            let mut found = false;
            let instance = FileManager::get().lock().unwrap();
            let files = instance.as_ref().unwrap().list().to_vec();
            for _file in &files {
                if self.value == _file.filename {
                    info!(
                        log,
                        "Found file: {} matches hash: {}", _file.filename, _file.hash
                    );
                    found = true;
                    let file = _file.path.clone();
                    info!(log, "Found file, removing from file system: {}", file);
                    let removed = std::fs::remove_file(file).is_ok();
                    if removed {
                        let index = files.iter().position(|x| *x == _file.clone()).unwrap();
                        instance.as_ref().unwrap().list().remove(index);
                        n = "200 OK\nContent-Length: 100\n";
                    }
                }
            }

            let s = format!("AFTP/1.0 {}\n", n);
            stream.write_all(s.as_bytes()).unwrap();
            if !found {
                info!(log, "Did not find following file: {}", self.value);
            }
            stream.shutdown(Shutdown::Both).unwrap();
        }

        if self.method == "GET" {
            let instance = FileManager::get().lock().unwrap();
            let files = instance.as_ref().unwrap().list().to_vec();

            for _file in files {
                if _file.filename == self.value {
                    let mut file = File::open(_file.path).unwrap();
                    let file_size = file.metadata().unwrap().len();

                    info!(log, "Found file, size of file: {}", file_size);
                    info!(log, "Handling current file: {}", _file.filename);

                    // open file in binary mode
                    let mut remaining_data = file_size as i32;
                    // buffer 4k, maybe enough for large files
                    let mut buf = [0 as u8; 4096];
                    let t: String = file_size.to_string();

                    let s = format!("AFTP/1.0 200 OK\nContent-Length: {}\nFile-Size: {}\n", t, t);

                    info!(log, "Sending GET response:\n {}", s);

                    stream.write_all(s.as_bytes()).unwrap();

                    while remaining_data != 0 {
                        // read chunk of file
                        let file_chunk = file.read(&mut buf);
                        if let Ok(n) = file_chunk {
                            stream.write_all(&buf[0..n]).unwrap();
                            info!(log, "Sending file chunk with byte size: {}", n);
                            remaining_data -= n as i32;
                            info!(log, "Remaining data for file: {}", remaining_data);
                        }
                    }
                    info!(log, "Done sending file: {}", _file.filename);
                    stream.shutdown(Shutdown::Both).unwrap();
                }
            }
        }
    }
}
