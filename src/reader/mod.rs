use std::{
    fs::{self, File},
    io::{self, BufReader},
};

pub fn configure_directory_to_serve_file(dir: &str) -> String {
    let path = format!("/tmp/{dir}");
    fs::create_dir_all(path.clone()).unwrap();
    path
}

pub fn fetch_file_buffer(file: &str, dir: &str) -> Result<BufReader<File>, io::Error> {
    // todo handle rust_file_server as a config passed from main
    let f = File::open(format!("/tmp/{dir}/{file}"))?;
    let reader = BufReader::new(f);
    Ok(reader)
}

pub fn cleanup_server_file(dir: &str) {
    let _ = fs::remove_dir_all(format!("/tmp/{dir}"));
}
