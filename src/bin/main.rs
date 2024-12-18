use fileserver::FileServer as server;
use fileserver::CommandType as commands;

static CONF_FOLDER_NAME:&str = "rust_file_server";
static CONF_PORT: &str =  "8089";
static CONF_ADDRESS: &str = "127.0.0.1";
fn main() {
    fileserver::configure_directory_to_serve_file(CONF_FOLDER_NAME);
    println!("Starting TCP server!!!");
    let mut file_server = server::new(CONF_ADDRESS, CONF_PORT, 10,CONF_FOLDER_NAME).unwrap();
    file_server.register_handlers(&[(commands::Download,server::handle_incomming_file_request)]);
    file_server.handle_incomming_connections();


    let cleanup = || {
        fileserver::cleanup_server_file(CONF_FOLDER_NAME);
    };

    // TODO: spawn a signal handler to allow shutdowns to cleanup gracefully
    cleanup();

}
