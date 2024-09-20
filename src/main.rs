use fileserver::FileServer;
fn main() {
    println!("Starting TCP server!!!");
    let file_server = FileServer::new("127.0.0.1", "8089", 10).unwrap();
    file_server.handle_incomming_connections(FileServer::parse_incomming_file_request);
}
