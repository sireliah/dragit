use std::fs;
use std::io::{self, Error, ErrorKind, Read, Write};
use std::sync::mpsc::{Receiver, SyncSender};
use std::thread::{self, JoinHandle};

use async_std::sync::Sender as AsyncSender;

#[cfg(unix)]
use pnet_datalink;

#[cfg(windows)]
use ipconfig;

use super::peer::{Direction, PeerEvent};

pub const CHUNK_SIZE: usize = 4096;

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
    mut file: fs::File,
) -> JoinHandle<Result<(), io::Error>> {
    thread::spawn(move || -> Result<(), io::Error> {
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

pub async fn notify_error(sender_queue: &AsyncSender<PeerEvent>, error_text: &str) {
    sender_queue
        .to_owned()
        .send(PeerEvent::Error(error_text.to_string()))
        .await;
}

pub async fn notify_completed(sender_queue: &AsyncSender<PeerEvent>) {
    sender_queue
        .to_owned()
        .send(PeerEvent::TransferCompleted)
        .await;
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
