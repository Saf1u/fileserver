#[derive(Eq, Hash, PartialEq, Clone, Copy, Debug)]
pub enum CommandType {
    Upload,
    Download,
    Statistics,
}

pub mod stats {
    use std::{io::Read, net::TcpStream};

    pub struct Stats {
        pub number_of_clients: u8,
        pub most_downloaded_file: String,
        pub file_downloaded_count: u8,
    }

    impl Stats {
        pub fn stats_from_stream(stream: &mut TcpStream) -> Stats {
            let mut client_count: [u8; 1] = [11];
            stream.read_exact(client_count.as_mut_slice()).unwrap();

            let mut most_accessed_file_name_length: [u8; 1] = [1];
            stream
                .read_exact(most_accessed_file_name_length.as_mut_slice())
                .unwrap();

            let mut vec = vec![0; most_accessed_file_name_length[0] as usize];
            let file_name: &mut [u8] = &mut vec[..];
            stream.read_exact(file_name).unwrap();

            let mut file_downloaded_stat: [u8; 1] = [11];
            stream
                .read_exact(file_downloaded_stat.as_mut_slice())
                .unwrap();

            Stats {
                number_of_clients: client_count[0],
                most_downloaded_file: String::from_utf8_lossy(&file_name).to_string(),
                file_downloaded_count: file_downloaded_stat[0],
            }
        }
    }
}
