

#[derive(Eq, Hash, PartialEq,Clone,Copy,Debug)]
pub enum CommandType {
    Upload,
    Download,
    Statistics,
}

pub struct Stats{
    number_of_clients: i64,
    most_downloaded_file: String,
}

