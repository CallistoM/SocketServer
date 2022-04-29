extern crate slog;
extern crate slog_term;

use slog::*;

use std::borrow::Cow;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::str;
use std::sync::Mutex;
use std::thread;

mod command;
mod file_manager;

use crate::file_manager::FileManager;

fn handle_message(stream: TcpStream, msg: Cow<str>, _log: Logger) {
    let log = _log.clone();
    let mut pattern = msg.split(' ').fuse();
    let method = pattern.next();
    let file_name = pattern.next();
    let mut hash = pattern.next();

    if hash.unwrap().is_empty() {
        hash = Some(hash.unwrap());
    }

    info!(_log, "Method          : {:?}", method.unwrap());
    info!(_log, "Filename        : {:?}", file_name.unwrap());
    info!(_log, "Hash            : {:?}", hash.unwrap());

    let mut valid_method = true;

    match method {
        Some("GET") => info!(log, "GET method"),
        Some("LIST") => info!(log, "LIST method"),
        Some("POST") => info!(log, "POST method"),
        Some("DELETE") => info!(log, "DELETE method"),
        Some("PUT") => info!(log, "PUT method"),
        _ => valid_method = false,
    }

    if valid_method {
        let command = command::Command::new(method.unwrap(), file_name.unwrap(), hash.unwrap());
        command.execute_method(stream.try_clone().unwrap(), log.clone());
    }
}

fn handle_client(mut stream: TcpStream, _log: Logger) {
    let mut data = [0 as u8; 4096];
    let log = _log.clone();
    let mut remaining_data: usize = 0;
    let mut file_name_t: String = "".parse().unwrap();

    'stream_reader: while match stream.read(&mut data) {
        Ok(size) => {
            if size == 0 {
                stream.shutdown(Shutdown::Both).unwrap();
                break 'stream_reader;
            }
            let msg = String::from_utf8_lossy(&data[0..size]);
            let msg_parse: String = msg.parse().unwrap();

            let mut pattern = msg_parse.split_whitespace().fuse();
            let method = pattern.next();

            if method.is_none() {
                break 'stream_reader;
            }

            if remaining_data == 0 && !file_name_t.is_empty() {
                info!(_log, "File writing is done for: {:?}", file_name_t);
                let response = "AFTP/1.0 OK\n".to_string();
                stream.write_all(response.as_bytes()).unwrap();
            }

            if remaining_data != 0 {
                if remaining_data > 0 {
                    remaining_data -= size;
                    let file_path_t = &("../files/".to_string() + file_name_t.as_ref());
                    let file = OpenOptions::new().append(true).open(file_path_t);
                    file.unwrap().write_all(&data[0..size]).unwrap();

                    let mut _file = File::open(file_path_t).unwrap();

                    info!(_log, "Remaining data: {:?}", remaining_data);
                } else {
                    info!(_log, "Streaming done");
                }
            }

            if method.unwrap() == "LOCK" {
                let file_name = pattern.next();
                let file = FileManager::get()
                    .lock()
                    .unwrap()
                    .as_mut()
                    .unwrap()
                    .lock_file(file_name.unwrap(), true);

                if file {
                    let response = "AFTP/1.0 LOCKED\n".to_string();
                    stream.write_all(response.as_bytes()).unwrap();
                } else {
                    let response = "AFTP/1.0 NOT FOUND\n".to_string();
                    stream.write_all(response.as_bytes()).unwrap();
                }
            }

            if method.unwrap() == "UNLOCK" {
                let file_name = pattern.next();
                let file = FileManager::get()
                    .lock()
                    .unwrap()
                    .as_mut()
                    .unwrap()
                    .lock_file(file_name.unwrap(), false);

                if file {
                    let response = "AFTP/1.0 UNLOCKED\n".to_string();
                    stream.write_all(response.as_bytes()).unwrap();
                } else {
                    let response = "AFTP/1.0 NOT FOUND\n".to_string();
                    stream.write_all(response.as_bytes()).unwrap();
                }
            }

            if method.unwrap() == "PUT" {
                pattern.next();
                let protocol = pattern.next();
                pattern.next();
                let file_name = pattern.next();
                pattern.next();
                let remaining_data_t = pattern.next().unwrap();
                remaining_data = remaining_data_t.parse::<usize>().unwrap();
                file_name_t = file_name.unwrap().to_string();

                info!(_log, "Remaining data message: {:?}", remaining_data);
                info!(_log, "Filename message: {:?}", file_name);
                info!(_log, "Protocol message: {:?}", protocol);

                if protocol.unwrap() == "AFTP/1.0" {
                    pattern.next().unwrap();
                    let hash = pattern.next().unwrap();
                    info!(_log, "hash: {:?}", hash);

                    let files = FileManager::get()
                        .lock()
                        .unwrap()
                        .as_mut()
                        .unwrap()
                        .list()
                        .to_vec();

                    for _file in &files {
                        if _file.filename == file_name.unwrap() {
                            if _file.locked {
                                let s = "AFTP/1.0 423 Locked\n".to_string();
                                stream.write_all(s.as_bytes()).unwrap();
                            } else {
                                FileManager::get()
                                    .lock()
                                    .unwrap()
                                    .as_mut()
                                    .unwrap()
                                    .lock_file(&_file.filename, true);
                            }
                        }
                    }

                    let mut vector = Vec::new();
                    let mut char_byte = 0;

                    for d in msg.split('\n') {
                        if char_byte == 1 {
                            vector.push(d);
                        }
                        if d.contains(hash) {
                            char_byte += 1;
                        }
                        continue;
                    }

                    let file_path =
                        &("../files/".to_string() + file_name.unwrap().to_string().as_ref());

                    let mut file = OpenOptions::new()
                        .create(true)
                        .truncate(true)
                        .open(file_path)
                        .unwrap();

                    for i in vector {
                        file.write_all(i.as_bytes()).unwrap();
                    }

                    remaining_data -= size;

                    FileManager::get().lock().unwrap().as_mut().unwrap().create(
                        file,
                        file_name.unwrap().to_string(),
                        file_path.to_string(),
                        hash.to_string(),
                    );

                    FileManager::get()
                        .lock()
                        .unwrap()
                        .as_mut()
                        .unwrap()
                        .lock_file(&String::from(file_name.unwrap()), false);

                    let response = "AFTP/1.0 OK\n".to_string();
                    stream.write_all(response.as_bytes()).unwrap();

                    info!(_log, "Stream is done");
                }
            }

            if msg.ends_with("AFTP/1.0\n") {
                info!(_log, "Protocol message : {:?}", msg);
                info!(_log, "Protocol seems correct, proceeding");
                handle_message(stream.try_clone().unwrap(), msg, _log.clone());
            }
            true
        }
        Err(_) => {
            error!(
                log,
                "An error occurred, terminating connection with {}",
                stream.peer_addr().unwrap()
            );

            FileManager::get()
                .lock()
                .unwrap()
                .as_mut()
                .unwrap()
                .unlock_all_files();

            stream.shutdown(Shutdown::Both).unwrap();
            false
        }
    } {}
}

fn main() {
    let listener = TcpListener::bind("127.0.0.1:9123").unwrap();

    let decorator = slog_term::TermDecorator::new().build();
    let drain = slog_term::FullFormat::new(decorator).build().fuse();
    let drain_mutex = Mutex::new(drain);

    let log = slog::Logger::root(drain_mutex.fuse(), o!());

    info!(log, "Starting socket server");

    FileManager::initialize(log.clone());
    FileManager::get()
        .lock()
        .unwrap()
        .as_mut()
        .unwrap()
        .get_files(log.clone());

    let log = log.clone();

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let log = log.clone();
                thread::spawn(move || {
                    info!(log, "New connection: {}", stream.peer_addr().unwrap());
                    handle_client(stream, log)
                });
            }
            Err(e) => {
                FileManager::get()
                    .lock()
                    .unwrap()
                    .as_mut()
                    .unwrap()
                    .unlock_all_files();

                info!(log, "Error: {}", e);
            }
        }
    }
    drop(listener);
}
