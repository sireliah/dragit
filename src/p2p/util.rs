use std::fs;
use std::io::{Error, ErrorKind, Read};
use std::time::{SystemTime, UNIX_EPOCH};

use async_std::fs::File as AsyncFile;
use async_std::io as asyncio;
use async_std::sync::{Receiver, Sender};
use async_std::task::{self, JoinHandle};
use directories::UserDirs;
use futures::channel::mpsc::Sender as FutSender;
use futures::prelude::*;
use hex;
use md5::{Digest, Md5};

use super::peer::PeerEvent;

pub const CHUNK_SIZE: usize = 4096;
pub const HASH_BUFFER_SIZE: usize = 1024;

pub fn spawn_write_file_job(mut receiver: Receiver<Vec<u8>>, path: String) -> JoinHandle<()> {
    let child = task::spawn(async move {
        let mut file = asyncio::BufWriter::new(
            AsyncFile::create(&path)
                .await
                .expect("Creating file failed"),
        );
        loop {
            match receiver.next().await {
                Some(payload) if payload == [] => {
                    file.flush().await.expect("Flushing file failed");
                    break;
                }
                Some(payload) => file.write_all(&payload).await.expect("Writing file failed"),
                None => (),
            }
        }
    });
    child
}

pub fn spawn_read_file_job(sender: Sender<Vec<u8>>, path: String) -> JoinHandle<()> {
    let child = task::spawn(async move {
        let file = AsyncFile::open(&path).await.expect("File missing");
        let mut reader = asyncio::BufReader::new(&file);

        loop {
            let mut buff = vec![0u8; CHUNK_SIZE * 32];
            match reader.read(&mut buff).await {
                Ok(n) if n > 0 => {
                    sender.send(buff[..n].to_vec()).await;
                    task::yield_now().await;
                }
                Ok(_) => {
                    sender.send(vec![]).await;
                    println!("Empty");
                    break;
                }
                Err(e) => panic!("Failed reading file {:?}", e),
            }
        }
    });
    child
}

pub async fn notify_progress(
    sender_queue: &FutSender<PeerEvent>,
    counter: usize,
    total_size: usize,
) {
    let event = PeerEvent::TransferProgress((counter, total_size));
    if let Err(e) = sender_queue.to_owned().send(event).await {
        eprintln!("{:?}", e);
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
