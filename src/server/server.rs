use super::types::CommandType;
use crate::reader::fetch_file_buffer;
use core::panic;
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
    handlers: HashMap<
        CommandType,
        fn(
            stream: &TcpStream,
            root_dir: &'static str,
            metrics_registry: Arc<RwLock<HashMap<String, i64>>>,
        ),
    >,
    max_connections: i32,
    next_id: i64,
    stats_bound_connections: Arc<RwLock<HashMap<i64, TcpStream>>>,
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
                max_connections: thread_count,
                root_dir,
                next_id: 0,
                stats_bound_connections: Arc::new(RwLock::new(HashMap::new())),
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

    // NOTE: I do not mind the root_dir being part of all handelr signatures
    // want to avoid gloabls, and creating an object when not ready
    // ideally the 2nd param would be a context with key-value relevant stuff
    // but not really needed right now :)
    pub fn handle_incomming_file_request(
        mut stream: &TcpStream,
        root_dir: &'static str,
        metrics_registry: Arc<RwLock<HashMap<String, i64>>>,
    ) {
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
        let file_name = result.unwrap();
        let mut file_reader = match fetch_file_buffer(file_name.as_str(), root_dir) {
            Err(error) => {
                Self::report_error_to_client(stream, error.to_string());
                return;
            }
            Ok(file_buffer) => file_buffer,
        };

        let mut stats = metrics_registry.write().unwrap();
        if let Some(x) = stats.get_mut(&file_name) {
            *x += 1;
        } else {
            stats.insert(file_name, 1);
        }

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

    pub fn no_op_handler(
        _stream: &TcpStream,
        _root_dir: &'static str,
        _metrics_registry: Arc<RwLock<HashMap<String, i64>>>,
    ) {
    }

    fn determine_handler(
        &self,
        mut stream: &TcpStream,
    ) -> Result<
        (
            fn(
                stream: &TcpStream,
                root_dir: &'static str,
                metrics_registry: Arc<RwLock<HashMap<String, i64>>>,
            ),
            CommandType,
        ),
        FileServerError,
    > {
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
                command = CommandType::Statistics;
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

        Ok((*handler.unwrap(), command))
    }

    // Counting on main ending for this to be temrinated, has no cleanup since we expect it to live for the life of the app
    pub fn send_stats(
        thread_pool_ref: Arc<Mutex<i32>>,
        file_stat_ref: Arc<RwLock<HashMap<String, i64>>>,
        stats_bound_connections_ref: Arc<RwLock<HashMap<i64, TcpStream>>>,
        interval: u64,
        max_connections_allowed: i32,
    ) {
        loop {
            thread::sleep(time::Duration::from_millis(interval));
            let pool_size = *thread_pool_ref.lock().unwrap();
            let mut max_count = 0;
            let mut most_demanded_file = String::from("no files");
            for (file, count) in file_stat_ref.read().unwrap().iter() {
                if *count > max_count {
                    max_count = *count;
                    most_demanded_file = file.clone();
                }
            }

            let mut dead_connections: Vec<i64> = Vec::new();

            for (id, mut conn) in stats_bound_connections_ref.write().unwrap().iter() {
                // TODO: handle these errors and cleanup the cache if connections are bad
                // start this call on it's own thread to do periodically
                println!("sending metrics to connection_id:{}...", id);

                if let Err(_) = conn.write(&[(max_connections_allowed - pool_size) as u8]) {
                    dead_connections.push(id.clone());
                    continue;
                }

                if let Err(_) = conn.write(&[most_demanded_file.len() as u8]) {
                    dead_connections.push(id.clone());
                    continue;
                }

                if let Err(_) = conn.write(most_demanded_file.as_bytes()) {
                    dead_connections.push(id.clone());
                    continue;
                }

                if let Err(_) = conn.write(&[max_count as u8]) {
                    dead_connections.push(id.clone());
                    continue;
                }

                println!("Successfully sent metrics to connection_id:{}...", id);
            }

            let mut v = stats_bound_connections_ref.write().unwrap();
            for connection_id in dead_connections {
                v.remove(&connection_id);
            }
        }
    }

    pub fn free_thread_barrier(&self, thread_lookup_interval: u64) {
        // look for a free thread in 6 second intervals
        loop {
            let mut count = self.thread_pool.lock().unwrap();
            if *count == 0 {
                drop(count);
                thread::sleep(time::Duration::from_millis(thread_lookup_interval));
            } else {
                *count -= 1;
                break;
            }
        }
    }

    pub fn start_metrics_report(&self) {
        let thread_pool = self.thread_pool.clone();
        let file_stats = self.file_stat.clone();
        let stats_bound_connections = self.stats_bound_connections.clone();
        let max_connections = self.max_connections;

        thread::spawn(move || {
            Self::send_stats(
                thread_pool,
                file_stats,
                stats_bound_connections,
                1000,
                max_connections,
            )
        });
    }

    pub fn handle_incomming_connections(&self) {
        for stream in self.listiner.incoming() {
            println!("Handling incoming connection .....");
            self.free_thread_barrier(6000);

            let mutex_ref = self.thread_pool.clone();
            let mut managed_stream = stream.unwrap();

            match self.determine_handler(&managed_stream) {
                Ok((handler, command_type)) => match command_type {
                    CommandType::Download => {
                        let root_dir = self.root_dir;
                        let merics_registry = self.file_stat.clone();
                        thread::spawn(move || {
                            managed_stream.set_read_timeout(None).unwrap();
                            handler(&mut managed_stream, root_dir, merics_registry);
                            let mut count = mutex_ref.lock().unwrap();
                            *count += 1;
                        });
                    }

                    CommandType::Statistics => {
                        self.stats_bound_connections
                            .write()
                            .unwrap()
                            .insert(self.next_id, managed_stream);

                        println!(
                            "Client with connection_id:{} registered on metrics endpoint....",
                            self.next_id
                        );
                    }

                    CommandType::Upload => {
                        panic!("upload should never be called!")
                    }
                },

                //TODO: standardize error report to client
                Err(error) => {
                    Self::report_error_to_client(&managed_stream, error.to_string());
                    let mut count = mutex_ref.lock().unwrap();
                    *count += 1;
                }
            }
        }
    }

    pub fn register_handlers(
        &mut self,
        handlers: &[(
            CommandType,
            fn(
                stream: &TcpStream,
                root_dir: &'static str,
                metrics_registry: Arc<RwLock<HashMap<String, i64>>>,
            ),
        )],
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
    use super::super::types::stats::Stats;
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
        handlers: &[(
            CommandType,
            fn(
                stream: &TcpStream,
                root_dir: &'static str,
                metrics_registry: Arc<RwLock<HashMap<String, i64>>>,
            ),
        )],
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

    fn download_test_file(
        addr: &'static str,
        port: &'static str,
        file_name: &'static str,
        read_delay: Option<time::Duration>,
    ) -> String {
        let addr_with_port = format!("{}:{}", addr, port);

        let mut stream = TcpStream::connect(addr_with_port).unwrap();
        stream.write_all(&[1]).unwrap();

        if let Some(delay) = read_delay {
            thread::sleep(delay);
        }
        stream
            .write_all(format!("filename={}|", file_name).as_bytes())
            .unwrap();
        stream.flush().unwrap();

        let mut buffer = Vec::new();

        stream.read_to_end(&mut buffer).unwrap();

        return String::from_utf8_lossy(&buffer).to_string();
    }

    fn connect_to_metrics_path(addr: &'static str, port: &'static str) -> TcpStream {
        let addr_with_port = format!("{}:{}", addr, port);
        let mut stream = TcpStream::connect(addr_with_port).unwrap();
        stream.write_all(&[3]).unwrap();
        return stream;
    }

    fn init_test_server(
        addr: &'static str,
        port: &'static str,
        content: &'static str,
        file_name: &'static str,
        root_dir: &'static str,
    ) {
        setup_tmp_file(root_dir, file_name, content);
        let server = setup_file_server(
            addr,
            port,
            10,
            &[
                (
                    CommandType::Download,
                    FileServer::handle_incomming_file_request,
                ),
                (CommandType::Statistics, FileServer::no_op_handler),
            ],
            root_dir,
        );

        server.start_metrics_report();
        thread::spawn(move || {
            server.handle_incomming_connections();
        });
    }

    #[test]
    fn test_download_file() {
        let addr = "127.0.0.1";
        let port = "8089";
        let content = "hello_from_file_Server!";
        let file_name = "temp_test_file_stats";
        let root_dir = "temp_test_root_dir";

        init_test_server(addr, port, content, file_name, root_dir);
        assert_eq!(content, download_test_file(addr, port, file_name, None));

        reader::cleanup_server_file(root_dir);
    }

    #[test]
    fn test_statistic() {
        let addr = "127.0.0.1";
        let port = "8079";
        let content = "hello_from_file_Server!";
        let file_name = "temp_test_file";
        let root_dir = "temp_test_root_dir";

        init_test_server(addr, port, content, file_name, root_dir);

        // Simulate long running connection on downlaod path
        thread::spawn(|| {
            download_test_file(
                addr,
                port,
                file_name,
                Some(time::Duration::from_millis(1000000)),
            );
        });
        download_test_file(addr, port, file_name, None);
        download_test_file(addr, port, file_name, None);
        download_test_file(addr, port, file_name, None);

        let mut metrics_stream = connect_to_metrics_path(addr, port);
        let stats = Stats::stats_from_stream(&mut metrics_stream);

        assert_eq!(2, stats.number_of_clients);
        assert_eq!("temp_test_file", stats.most_downloaded_file);
        assert_eq!(3, stats.file_downloaded_count);

        reader::cleanup_server_file(root_dir);
    }
}
