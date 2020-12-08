use std::fs;
use std::io::{self, Error, ErrorKind, Read, Write};
use std::str::FromStr;
use std::sync::mpsc::{Receiver, SyncSender};
use std::thread::{self, JoinHandle};

use async_std::sync::Sender as AsyncSender;

#[cfg(unix)]
use pnet_datalink;

#[cfg(windows)]
use ipconfig;

use futures::prelude::*;
use hex;
use md5::{Digest, Md5};

use super::peer::{Direction, PeerEvent};

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

pub fn spawn_write_file_job(
    receiver: Receiver<Vec<u8>>,
    path: String,
) -> JoinHandle<Result<(), io::Error>> {
    thread::spawn(move || -> Result<(), io::Error> {
        let mut file = fs::File::create(&path)?;
        loop {
            match receiver.recv() {
                Ok(payload) if payload == [] => {
                    file.flush()?;
                    break;
                }
                Ok(payload) => file.write_all(&payload)?,
                Err(e) => {
                    error!(
                        "Writing to file failed, because sender was disconnected: {:?}",
                        e
                    );
                    return Err(io::Error::new(
                        io::ErrorKind::NotConnected,
                        "File sender disconnected",
                    ));
                }
            }
        }
        Ok(())
    })
}

pub fn send_buffer(sender: &SyncSender<Vec<u8>>, buff: Vec<u8>) -> Result<(), io::Error> {
    sender.send(buff).or_else(|e| {
        error!("File reader disconnected: {:?}", e);
        Err(io::Error::new(
            io::ErrorKind::NotConnected,
            "File reader disconnected",
        ))
    })
}

pub fn spawn_read_file_job(
    sender: SyncSender<Vec<u8>>,
    path: String,
) -> JoinHandle<Result<(), io::Error>> {
    thread::spawn(move || -> Result<(), io::Error> {
        let mut file = fs::File::open(&path)?;

        loop {
            let mut buff = vec![0u8; CHUNK_SIZE * 32];
            match file.read(&mut buff) {
                Ok(n) if n > 0 => {
                    send_buffer(&sender, buff[..n].to_vec())?;
                }
                Ok(_) => {
                    send_buffer(&sender, vec![])?;
                    break;
                }
                Err(e) => return Err(e),
            }
        }
        Ok(())
    })
}

pub async fn notify_progress(
    sender_queue: &AsyncSender<PeerEvent>,
    counter: usize,
    total_size: usize,
    direction: &Direction,
) {
    let event = PeerEvent::TransferProgress((counter, total_size, direction.to_owned()));
    sender_queue.to_owned().send(event).await;
}

pub async fn notify_completed(sender_queue: &AsyncSender<PeerEvent>) {
    sender_queue
        .to_owned()
        .send(PeerEvent::TransferCompleted)
        .await;
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

#[cfg(unix)]
pub fn check_network_interfaces() -> Result<(), Error> {
    let interfaces = pnet_datalink::interfaces();
    let default_interface = interfaces
        .iter()
        .filter(|e| !e.is_loopback() && e.ips.len() > 0)
        .next();
    info!("Default network interface: {:?}", default_interface);
    match default_interface {
        Some(_) => {
            info!("Interfaces: {:?}", interfaces);
            Ok(())
        }
        None => {
            error!("No network interfaces found!");
            Err(Error::new(
                ErrorKind::AddrNotAvailable,
                "There is no network connection available",
            ))
        }
    }
}

#[cfg(windows)]
pub fn check_network_interfaces() -> Result<(), Error> {
    let adapter = match ipconfig::get_adapters() {
        Ok(ad) => {
            let ada = ad
                .into_iter()
                .filter(|a| {
                    a.ip_addresses().len() > 0
                        && a.oper_status() == ipconfig::OperStatus::IfOperStatusUp
                        && a.if_type() != ipconfig::IfType::SoftwareLoopback
                })
                .next();
            ada
        }
        Err(_) => None,
    };

    match adapter {
        Some(adapter) => {
            info!("Adapters: {:?}", adapter);
            Ok(())
        }
        None => {
            error!("No network interfaces found!");
            Err(Error::new(
                ErrorKind::AddrNotAvailable,
                "There is no network connection available",
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::p2p::util::hash_contents;

    #[test]
    fn test_hash_local_file() {
        let result = hash_contents("src/file.txt").unwrap();

        assert_eq!(result, "696c56be6d4c4a48d3de0d17e237f82a");
    }
}
