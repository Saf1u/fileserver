use std::{
    fmt,
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex},
    thread, time,
};

use once_cell::sync::Lazy;
use regex::Regex;
static FILE_MATCHER: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"filename=([^|]+)\|").unwrap()
    // allowed filename: filename=a_file_name|
});


#[derive(Debug)]
pub enum FileServerError {
    FailedToInitFTPServer(String),
    FailedToParseRequest(String),
    ServerReadError(String),
}

impl fmt::Display for FileServerError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            FileServerError::FailedToInitFTPServer(reason) => {
                return write!(f, "Could not init FTPServer: {}", reason)
            }
            FileServerError::FailedToParseRequest(reason) => {
                return write!(f, "Could not parse filename in request: {}", reason)
            }
            FileServerError::ServerReadError(_) => return write!(f, "Client read deadline"),
        }
    }
}

pub struct FileServer {
    thread_pool: Arc<Mutex<i32>>,
    listiner: TcpListener,
}

impl FileServer {
   pub fn new(address: &str, port: &str, thread_count: i32) -> Result<FileServer, FileServerError> {
        let addr = format!("{}:{}", address, port);
        let listener = TcpListener::bind(addr);
        match listener {
            Err(err) => Err(FileServerError::FailedToInitFTPServer(err.to_string())),
            Ok(listener) => {
                return Ok(FileServer {
                    thread_pool: Arc::new(Mutex::new(thread_count)),
                    listiner: listener,
                })
            }
        }
    }

    pub fn parse_incomming_file_request(stream: &TcpStream) -> Result<String, FileServerError> {
        let mut reader = BufReader::new(stream);
        let mut buffer = Vec::new();
        stream.set_read_timeout(None).unwrap();
        if let Err(err) = reader.read_until(b'|', &mut buffer) {
            return Err(FileServerError::ServerReadError(err.to_string()));
        }

        // Check if the string matches the pattern
        let caps = FILE_MATCHER.captures(std::str::from_utf8(&buffer).unwrap());
        match caps {
            None => {
                return Err(FileServerError::FailedToParseRequest("file name not found".to_owned()));
            }
            Some(capture) => capture.get(1).map_or(
                Err(FileServerError::FailedToParseRequest("file name not found".to_owned())),
                |v| Ok(v.as_str().to_owned()),
            ),
        }
    }

    pub fn handle_incomming_connections(&self,handler:fn (stream: &TcpStream) -> Result<String, FileServerError>) {
        for stream in self.listiner.incoming() {
            // look for a free thread
            loop {
                let mut count = self.thread_pool.lock().unwrap();
                if *count == 0 {
                    drop(count);
                    thread::sleep(time::Duration::from_millis(6000));
                } else {
                    *count = *count - 1;
                    drop(count);
                    break;
                }
            }
            let mutex_ref = self.thread_pool.clone();
            println!("Handling incoming connection .....");
            thread::spawn(move || {
                let mut stream = stream.unwrap();
                match handler(&stream) {
                    Ok(filename) => {
                        let msg = format!("req for file:{filename}");
                        stream.write_all(msg.as_bytes()).unwrap();
                    }
                    Err(err) => {
                        println!("error:{err}");
                        stream.write_all(b"error handling").unwrap();
                    }
                }
                let mut count = mutex_ref.lock().unwrap();
                *count = *count + 1;
            });
        }
    }

}
