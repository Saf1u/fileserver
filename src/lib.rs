
// do not make public as a lib
mod server;
mod reader;
// reexport only what I want
pub use reader::{configure_directory_to_serve_file,cleanup_server_file};
pub use server::{types::CommandType,server::FileServer};

// reexport modules for external usage like so
// use $crate_name::server::$file_server_type/trait/function;
// module lookup path from here is as follows
// it looks for a file with the name
// if it exist it reexports its content
// else it looks for a folder with the name and exports the contents of its
// mod.rs
