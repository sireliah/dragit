use std::fs;
use std::io;
use std::io::Write;
use std::io::{Error, ErrorKind, Read};
use std::str::FromStr;
use std::thread::{self, JoinHandle};
use std::time::{SystemTime, UNIX_EPOCH};

use directories::UserDirs;
use futures::channel::mpsc::Sender as FutSender;
use futures::prelude::*;
use hex;
use md5::{Digest, Md5};
use std::sync::mpsc::{Receiver, SyncSender};

use super::peer::PeerEvent;

pub const CHUNK_SIZE: usize = 4096;
pub const HASH_BUFFER_SIZE: usize = 1024;
pub const FRAME_SIZE: usize = 1024;

pub struct Metadata {
    pub name: String,
    pub hash: String,
    pub size: usize,
}

impl Metadata {
    pub async fn read_metadata<TSocket>(mut socket: TSocket) -> Result<(Self, TSocket), io::Error>
    where
        TSocket: AsyncRead + AsyncWrite + Send + Unpin,
    {
        let mut read = 0;
        let mut meta: Vec<u8> = vec![];
        loop {
            let mut buff = [0u8; FRAME_SIZE];
            match socket.read(&mut buff).await {
                Ok(n) => {
                    read += n;
                    meta.extend(&buff[..n]);
                    if read >= FRAME_SIZE {
                        break;
                    }
                }
                Err(e) => return Err(e),
            }
        }
        let buff = match String::from_utf8(meta) {
            Ok(v) => v,
            Err(e) => return Err(Error::new(ErrorKind::InvalidData, e)),
        };
        let meta = buff.split("\n").collect::<Vec<&str>>();
        let name = meta[0];
        let hash = meta[1];
        let size = meta[2];

        let (name, hash, size) = (
            name.trim().to_string(),
            hash.trim().to_string(),
            match size.trim().parse::<usize>() {
                Ok(v) => v,
                Err(e) => return Err(Error::new(ErrorKind::InvalidData, e)),
            },
        );
        info!("Read: Name: {}, Hash: {}, Size: {}", name, hash, size);
        Ok((Metadata { name, hash, size }, socket))
    }

    pub async fn write_metadata<TSocket>(
        name: &str,
        path: &str,
        mut socket: TSocket,
    ) -> Result<(usize, TSocket), io::Error>
    where
        TSocket: AsyncRead + AsyncWrite + Send + Unpin,
    {
        let hash = calculate_hash(path).await?;
        let name = add_row(name);
        let size = check_size(path)?;
        let size_b = add_row(&size);
        let size_u = usize::from_str(&size).unwrap_or(0);
        let checksum = add_row(&hash);

        let sum = name.len() + checksum.len() + size_b.len();
        let fill = vec![0; FRAME_SIZE - sum];

        socket.write(&name).await?;
        socket.write(&checksum).await?;
        socket.write(&size_b).await?;
        socket.write(&fill).await?;
        socket.flush().await?;

        Ok((size_u, socket))
    }
}

pub fn spawn_write_file_job(receiver: Receiver<Vec<u8>>, path: String) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut file = fs::File::create(&path).expect("Creating file failed");
        loop {
            match receiver.recv() {
                Ok(payload) if payload == [] => {
                    file.flush().expect("Flushing file failed");
                    break;
                }
                Ok(payload) => file.write_all(&payload).expect("Writing file failed"),
                Err(e) => panic!(e),
            }
        }
    })
}

pub fn spawn_read_file_job(sender: SyncSender<Vec<u8>>, path: String) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut file = fs::File::open(&path).expect("File missing");

        loop {
            let mut buff = vec![0u8; CHUNK_SIZE * 32];
            match file.read(&mut buff) {
                Ok(n) if n > 0 => {
                    sender.send(buff[..n].to_vec()).expect("sending failed");
                }
                Ok(_) => {
                    sender.send(vec![]).expect("sending failed");
                    break;
                }
                Err(e) => panic!("Failed reading file {:?}", e),
            }
        }
    })
}

pub async fn notify_progress(
    sender_queue: &FutSender<PeerEvent>,
    counter: usize,
    total_size: usize,
) {
    let event = PeerEvent::TransferProgress((counter, total_size));
    if let Err(e) = sender_queue.to_owned().send(event).await {
        error!("{:?}", e);
    }
}

pub fn get_target_path(name: &str) -> Result<String, Error> {
    // TODO: make this a future
    match UserDirs::new() {
        Some(dirs) => match dirs.download_dir() {
            Some(path) => {
                let now = SystemTime::now();
                let timestamp = now.duration_since(UNIX_EPOCH).expect("Time failed");
                let name = format!("{}_{}", timestamp.as_secs(), name);
                let p = path.join(name);
                let result = p.into_os_string().into_string();
                match result {
                    Ok(value) => Ok(value),
                    Err(_) => Err(Error::new(
                        ErrorKind::InvalidData,
                        "Could not return Downloads path as string",
                    )),
                }
            }
            None => Err(Error::new(
                ErrorKind::NotFound,
                "Downloads directory could not be found",
            )),
        },
        None => Err(Error::new(ErrorKind::NotFound, "Could not check user dirs")),
    }
}

pub fn add_row(value: &str) -> Vec<u8> {
    format!("{}\n", value).into_bytes()
}

pub fn hash_contents(path: &str) -> Result<String, Error> {
    let mut state = Md5::default();
    let mut buffer = [0u8; HASH_BUFFER_SIZE];
    let mut reader = fs::File::open(path)?;

    loop {
        match reader.read(&mut buffer) {
            Ok(n) if n == 0 || n < HASH_BUFFER_SIZE => {
                state.input(&buffer[..n]);
                break;
            }
            Ok(n) => {
                state.input(&buffer[..n]);
            }
            Err(e) => return Err(e),
        };
    }
    Ok(hex::encode::<Vec<u8>>(state.result().to_vec()))
}

async fn calculate_hash(path: &str) -> Result<String, io::Error> {
    Ok(hash_contents(path)?)
}

pub fn check_size(path: &str) -> Result<String, Error> {
    let meta = fs::metadata(path)?;
    Ok(meta.len().to_string())
}

pub fn time_to_notify(current_size: usize, total_size: usize) -> bool {
    if current_size >= ((total_size / 10) + CHUNK_SIZE * 256) {
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use self::super::hash_contents;

    #[test]
    fn test_hash_local_file() {
        let result = hash_contents("src/file.txt").unwrap();

        assert_eq!(result, "696c56be6d4c4a48d3de0d17e237f82a");
    }
}
