use std::fs;
use std::io::{self, Read, Write};
use std::sync::mpsc::{Receiver, SyncSender};
use std::thread::{self, JoinHandle};

use crate::p2p::util::CHUNK_SIZE;

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
