use std::{fs::{self, File}, io::{BufReader, self}};
/// sets up a directory at tmp to server/store files from.
/// panics if directory setup fails.
/// # Examples
///
/// ```
/// let result = add(2, 3);
/// assert_eq!(result, 5);
/// ```
pub fn configure_directory_to_serve_file(dir:&str){
    fs::create_dir_all(format!("/tmp/{dir}")).unwrap();
}

pub fn fetch_file_buffer(file:&str) -> Result<BufReader<File>, io::Error>{
    // todo handle rust_file_server as a config passed from main
    let f = File::open(format!("/tmp/rust_file_server/{file}"))?;
    let reader = BufReader::new(f);
    Ok(reader)
}

pub fn cleanup_server_file(dir:&str){
    let _ = fs::remove_dir_all(format!("/tmp/{dir}"));
}
