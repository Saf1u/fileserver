use super::types::CommandType;
use crate::reader::fetch_file_buffer;
use once_cell::sync::Lazy;
use regex::Regex;
use std::{
    collections::HashMap,
    fmt,
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex, RwLock},
    thread, time,
};

pub struct FileServer {
    thread_pool: Arc<Mutex<i32>>,
    listiner: TcpListener,
    handlers: HashMap<CommandType, fn(stream: &TcpStream, root_dir: &'static str)>,
    root_dir: &'static str,
    file_stat: Arc<RwLock<HashMap<String, i64>>>, // TODO: I pass this config to each handler function, I think this is a bit impure.
                                                  // I would like to bootstrap the function in a closure somehow to refrence the config or use globabl configs somehow.
}

static FILE_MATCHER: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"filename=([^|]+)\|").unwrap()
    // allowed filename: filename=a_file_name|
});

#[derive(Debug)]
pub enum FileServerError {
    FailedToInitFTPServer(String),
    FailedToParseRequest(String),
    FailedToParseCommand(String),
    ServerReadError(String),
}

impl fmt::Display for FileServerError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            FileServerError::FailedToInitFTPServer(reason) => {
                write!(f, "Could not init FTPServer: {}", reason)
            }
            FileServerError::FailedToParseRequest(reason) => {
                write!(f, "Could not parse filename in request: {}", reason)
            }

            FileServerError::FailedToParseCommand(reason) => {
                write!(f, "Could not parse command in request: {}", reason)
            }
            FileServerError::ServerReadError(_) => write!(f, "Client read deadline"),
        }
    }
}

impl FileServer {
    pub fn new(
        address: &str,
        port: &str,
        thread_count: i32,
        root_dir: &'static str,
    ) -> Result<FileServer, FileServerError> {
        let addr = format!("{}:{}", address, port);
        let listener = TcpListener::bind(addr);
        match listener {
            Err(err) => Err(FileServerError::FailedToInitFTPServer(err.to_string())),
            Ok(listener) => Ok(FileServer {
                thread_pool: Arc::new(Mutex::new(thread_count)),
                listiner: listener,
                handlers: HashMap::new(),
                root_dir,
                file_stat: Arc::new(RwLock::new(HashMap::new())),
            }),
        }
    }

    pub fn report_error_to_client(mut stream: &TcpStream, err_string: String) {
        println!("...Error reporting to client:{err_string}");
        stream.write_all(err_string.as_bytes()).unwrap_or_else(|_| {
            println!("...Error while reporting error to client:{err_string}");
        });
    }

    pub fn update_count(&self, file_name: String) {
        let mut stats = self.file_stat.write().unwrap();
        if let Some(x) = stats.get_mut(&file_name) {
            *x += 1;
        } else {
            stats.insert(file_name, 1);
        }
    }

    // NOTE: I do not mind the root_dir being part of all handelr signatures
    // want to avoid gloabls, and creating an object when not ready
    // ideally the 2nd param would be a context with key-value relevant stuff
    // but not really needed right now :)
    pub fn handle_incomming_file_request(mut stream: &TcpStream, root_dir: &'static str) {
        let mut buffer = Vec::new();
        let mut reader = BufReader::new(stream);
        if let Err(err) = reader.read_until(b'|', &mut buffer) {
            Self::report_error_to_client(stream, err.to_string());
            return;
        }

        // Check if the string matches the pattern
        let caps = FILE_MATCHER.captures(std::str::from_utf8(&buffer).unwrap());
        let result = match caps {
            None => Err(FileServerError::FailedToParseRequest(
                "file name not found".to_owned(),
            )),
            Some(capture) => capture.get(1).map_or(
                Err(FileServerError::FailedToParseRequest(
                    "file name not found".to_owned(),
                )),
                |v| Ok(v.as_str().to_owned()),
            ),
        };

        // report error if matching failed
        if let Err(err) = result {
            Self::report_error_to_client(stream, err.to_string());
            return;
        }

        // fetch file buffer with content
        let mut file_reader = match fetch_file_buffer(result.unwrap().as_str(), root_dir) {
            Err(error) => {
                Self::report_error_to_client(stream, error.to_string());
                return;
            }
            Ok(file_buffer) => file_buffer,
        };

        loop {
            // read from the file 1KB at a time until EOF aka (0)
            let mut buf = vec![];
            let read_op = { file_reader.by_ref().take(1024).read_to_end(&mut buf) };
            match read_op {
                Ok(read) => {
                    if read == 0 {
                        return;
                    }
                    stream.write_all(&buf).unwrap_or_else(|error| {
                        Self::report_error_to_client(stream, error.to_string());
                    });
                }
                Err(error) => {
                    Self::report_error_to_client(stream, error.to_string());
                    return;
                }
            }
        }
    }

    pub fn handle_server_stats_request(stream: &TcpStream, root_dir: &'static str) {}

    fn determine_handler(
        &self,
        mut stream: &TcpStream,
    ) -> Result<fn(stream: &TcpStream, root_dir: &'static str), FileServerError> {
        let mut client_command_byte: [u8; 1] = [0];
        if let Err(err) = stream.read(&mut client_command_byte) {
            return Err(FileServerError::FailedToParseCommand(err.to_string()));
        }

        let command: CommandType;

        match client_command_byte[0] {
            1 => {
                command = CommandType::Download;
            }
            2 => {
                panic!("upload not implemented")
            }
            3 => {
                panic!("stats not implemented")
            }
            _ => {
                panic!("not implemented")
            }
        }

        let handler = self.handlers.get(&command);

        if handler.is_none() {
            return Err(FileServerError::FailedToParseCommand(
                "unsupported command type".to_owned(),
            ));
        }

        Ok(*handler.unwrap())
    }

    pub fn handle_incomming_connections(&self) {
        for stream in self.listiner.incoming() {
            // look for a free thread
            loop {
                let mut count = self.thread_pool.lock().unwrap();
                if *count == 0 {
                    drop(count);
                    thread::sleep(time::Duration::from_millis(6000));
                } else {
                    *count -= 1;
                    drop(count);
                    break;
                }
            }
            let mutex_ref = self.thread_pool.clone();
            println!("Handling incoming connection .....");
            let mut stream = stream.unwrap();
            match self.determine_handler(&stream) {
                Ok(handler) => {
                    let root_dir = self.root_dir;
                    thread::spawn(move || {
                        stream.set_read_timeout(None).unwrap();
                        handler(&mut stream, root_dir);
                        let mut count = mutex_ref.lock().unwrap();
                        *count += 1;
                    });
                }
                Err(error) => {
                    Self::report_error_to_client(&stream, error.to_string());
                    let mut count = mutex_ref.lock().unwrap();
                    *count += 1;
                }
            }
        }
    }

    pub fn register_handlers(
        &mut self,
        handlers: &[(CommandType, fn(stream: &TcpStream, root_dir: &'static str))],
    ) {
        for (command, handler) in handlers {
            println!("Registering {:?} handler...", command);
            self.handlers.insert(*command, *handler);
        }
    }
}

// Test Helpers

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader;
    use std::fs;

    fn setup_tmp_file(root_dir: &str, filename: &str, file_content: &str) {
        let path = reader::configure_directory_to_serve_file(root_dir);
        fs::write(format!("{}/{}", path.as_str(), filename), file_content).unwrap();
    }

    fn setup_file_server(
        addr: &str,
        port: &str,
        threads: i32,
        handlers: &[(CommandType, fn(stream: &TcpStream, root_dir: &'static str))],
        root_dir: &'static str,
    ) -> FileServer {
        let mut file_server = FileServer::new(addr, port, threads, root_dir).unwrap();
        file_server.register_handlers(handlers);
        file_server
    }
    use std::{
        io::{Read, Write},
        net::TcpStream,
    };

    #[test]
    fn test_download_file() {
        let addr = "127.0.0.1";
        let port = "8089";
        let content = "hello_from_file_Server!";
        let file_name = "temp_test_file";
        let root_dir = "temp_test_root_dir";

        setup_tmp_file(root_dir, file_name, content);
        let server = setup_file_server(
            addr,
            port,
            3,
            &[(
                CommandType::Download,
                FileServer::handle_incomming_file_request,
            )],
            root_dir,
        );

        thread::spawn(move || {
            server.handle_incomming_connections();
        });

        let addr_with_port = format!("{}:{}", addr, port);

        let mut stream = TcpStream::connect(addr_with_port).unwrap();
        stream.write_all(&[1]).unwrap();
        stream
            .write_all(format!("filename={}|", file_name).as_bytes())
            .unwrap();
        stream.flush().unwrap();

        let mut buffer = Vec::new();
        stream.read_to_end(&mut buffer).unwrap();

        assert_eq!(content, String::from_utf8_lossy(&buffer));
        reader::cleanup_server_file(root_dir);
    }
}
