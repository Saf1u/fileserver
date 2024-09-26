use fileserver::server::FileServer as server;

static CONF_FOLDER_NAME:&str = "rust_file_server";
static CONF_PORT: &str =  "8089";
static CONF_ADDRESS: &str = "127.0.0.1";
fn main() {
    fileserver::configure_directory_to_serve_file(CONF_FOLDER_NAME);
    println!("Starting TCP server!!!");
    let file_server = server::new(CONF_ADDRESS, CONF_PORT, 10).unwrap();
    file_server.handle_incomming_connections(server::parse_incomming_file_request);


    let cleanup = || {
        fileserver::cleanup_server_file(CONF_FOLDER_NAME);
    };

    cleanup();

}
