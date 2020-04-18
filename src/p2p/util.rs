use std::fs;
use std::io::{Error, ErrorKind};
use std::time::{SystemTime, UNIX_EPOCH};

use crypto::digest::Digest;
use crypto::sha1::Sha1;
use directories::UserDirs;

use async_std::fs::File as AsyncFile;
use async_std::io as asyncio;
use async_std::sync::{Receiver, Sender};
use async_std::task::{self, JoinHandle};
use futures::prelude::*;

pub const CHUNK_SIZE: usize = 4096;

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
                    break;
                }
                Err(e) => panic!("Failed reading file {:?}", e),
            }
        }
    });
    child
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

pub fn hash_contents(contents: &Vec<u8>) -> String {
    let mut hasher = Sha1::new();
    hasher.input(&contents);
    hasher.result_str()
}

pub fn check_size(path: &str) -> Result<String, Error> {
    let meta = fs::metadata(path)?;
    Ok(meta.len().to_string())
}
