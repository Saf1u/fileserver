use std::{
    fmt,
    io::{BufRead, BufReader, Write, self, Error, Read},
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex},
    thread, time,
};
use crate::reader::fetch_file_buffer;



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

    pub fn report_error_to_client(mut stream:&TcpStream,err_string:String){
        println!("...Error reporting to client:{err_string}");
        stream.write_all(err_string.as_bytes()).unwrap_or_else(|_|{
            println!("...Error while reporting error to client:{err_string}");
        });
    }

    pub fn parse_incomming_file_request(mut stream:&TcpStream){
        let mut buffer = Vec::new();
        let mut reader = BufReader::new(stream);
        if let Err(err) = reader.read_until(b'|', &mut buffer) {
            Self::report_error_to_client(stream,err.to_string());
            return;
        }

        // Check if the string matches the pattern
        let caps = FILE_MATCHER.captures(std::str::from_utf8(&buffer).unwrap());
        let result = match caps {
            None => Err(FileServerError::FailedToParseRequest("file name not found".to_owned())),
            Some(capture) => capture.get(1).map_or(
                Err(FileServerError::FailedToParseRequest("file name not found".to_owned())),
                |v| Ok(v.as_str().to_owned()),
            ),
        };

        // report error if matching failed
        if let Err(err) = result{
            Self::report_error_to_client(stream,err.to_string());
            return;
        }

        // fetch file buffer with content
        let mut file_reader = match fetch_file_buffer(result.unwrap().as_str()) {
            Err(error) => {
                Self::report_error_to_client(stream,error.to_string());
                return;
            },
            Ok(file_buffer) => {
                file_buffer
            }
        };

        loop{
            // read from the file 1KB at a time unile EOF aka (0)
            let mut buf = vec![];
            let read_op = {
                file_reader.
                by_ref().
                take(1024).read_to_end(&mut buf)
            };
            match read_op {
                Ok(read) => {
                    if read == 0 {
                        return;
                    }
                    stream.write_all(&buf).unwrap_or_else(|error|{
                        Self::report_error_to_client(stream,error.to_string());
                    });
                },
                Err(error) => {
                    Self::report_error_to_client(stream,error.to_string());
                    return;
                },
            }
        }
    }

    pub fn handle_incomming_connections(&self,handler:fn (stream: &TcpStream)) {
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
                stream.set_read_timeout(None).unwrap();
                let _ = handler(& mut stream);
                let mut count = mutex_ref.lock().unwrap();
                *count = *count + 1;
            });
        }
    }

}

// #[cfg(test)]
// mod tests {
//     use super::*;

//     #[test]
//     fn test_add_positive_numbers() {
//         assert_eq!(add(2, 3), 5);
//     }

//     #[test]
//     fn test_add_negative_numbers() {
//         assert_eq!(add(-2, -3), -5);
//     }

//     #[test]
//     fn test_add_mixed_numbers() {
//         assert_eq!(add(2, -3), -1);
//     }
// }
