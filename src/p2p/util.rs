use std::io::{Error, ErrorKind};

use async_std::sync::Sender as AsyncSender;
use futures::prelude::*;

#[cfg(unix)]
use pnet_datalink;

#[cfg(windows)]
use ipconfig;

use super::peer::{Direction, PeerEvent};

// Convenience trait implementation, which helps to alias socket type
pub trait TSocketAlias: AsyncRead + AsyncWrite + Send + Unpin {}
impl<T: AsyncRead + AsyncWrite + Send + Unpin> TSocketAlias for T {}

pub const CHUNK_SIZE: usize = 4096;

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

pub async fn notify_waiting(sender_queue: &AsyncSender<PeerEvent>) {
    sender_queue
        .to_owned()
        .send(PeerEvent::WaitingForAnswer)
        .await;
}

pub async fn notify_rejected(sender_queue: &AsyncSender<PeerEvent>) {
    sender_queue
        .to_owned()
        .send(PeerEvent::TransferRejected)
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
    debug!("Default network interface: {:?}", default_interface);
    match default_interface {
        Some(_) => {
            debug!("Interfaces: {:?}", interfaces);
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
