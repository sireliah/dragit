use std::fmt;
use std::fs;
use std::io::{self, Error, Read};

use super::proto::Answer as ProtoAnswer;
use super::proto::Metadata as ProtoMetadata;
use futures::prelude::*;
use hex;
use md5::{Digest, Md5};
use prost::Message;

use crate::p2p::transfer::FileToSend;
use crate::p2p::util::TSocketAlias;
use crate::p2p::TransferType;

pub const ANSWER_SIZE: usize = 2;
pub const PACKET_SIZE: usize = 1024;
pub const HASH_BUFFER_SIZE: usize = 1024;

pub struct Metadata {
    pub name: String,
    pub hash: String,
    pub size: usize,
    pub transfer_type: TransferType,
}

impl Metadata {
    pub async fn read(socket: impl TSocketAlias) -> Result<(Self, impl TSocketAlias), io::Error> {
        let (data, socket) = read_from_socket(socket).await?;
        let proto = ProtoMetadata::decode(&data[..])?;

        let name = proto.name;
        let hash = proto.hash;
        let size = proto.size as usize;
        let transfer_type =
            TransferType::from_i32(proto.transfer_type).unwrap_or(TransferType::File);
        info!("Read: Name: {}, Hash: {}, Size: {}", name, hash, size);
        Ok((
            Metadata {
                name,
                hash,
                size,
                transfer_type,
            },
            socket,
        ))
    }

    pub async fn write(
        file: &FileToSend,
        mut socket: impl TSocketAlias,
    ) -> Result<(usize, impl TSocketAlias), io::Error> {
        info!("Getting hash");
        let hash = file.calculate_hash().await?;
        info!("Getting size");
        let size = file.check_size()?;

        let proto = ProtoMetadata {
            name: file.name.to_string(),
            hash,
            size,
            transfer_type: file.transfer_type as i32,
        };
        let len = proto.encoded_len();
        let fill = vec![0; PACKET_SIZE - len];
        let mut buf = Vec::with_capacity(len);
        proto.encode(&mut buf)?;

        socket.write(&buf[..len]).await?;

        // Append null bytes to the stream to transmit the full packet
        socket.write(&fill).await?;
        socket.flush().await?;

        Ok((size as usize, socket))
    }

    /// Produce predictable file name for both file and text payloads.
    /// This is necessary for instance for Windows, which doesn't accept
    /// certain characters in file names (like "\n")
    pub fn get_safe_file_name(&self) -> String {
        match self.transfer_type {
            TransferType::File => self.name.to_string(),
            TransferType::Text => {
                let mut hasher = Md5::new();
                hasher.update(self.name.to_string());
                let result = hasher.finalize();
                hex::encode::<Vec<u8>>(result.to_vec())
            }
        }
    }
}

impl fmt::Display for Metadata {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Metadata:\n name: {}\n hash: {}\n size: {}\n type: {}\n",
            self.name, self.hash, self.size, self.transfer_type
        )
    }
}

#[derive(Debug)]
pub struct Answer;

impl Answer {
    pub async fn read(socket: impl TSocketAlias) -> Result<(bool, impl TSocketAlias), io::Error> {
        let (data, socket) = read_from_socket(socket).await?;
        let proto = ProtoAnswer::decode(&data[..])?;

        Ok((proto.accepted, socket))
    }

    pub async fn write(
        mut socket: impl TSocketAlias,
        accepted: bool,
        hash: String,
    ) -> Result<((), impl TSocketAlias), io::Error> {
        let proto = ProtoAnswer { accepted, hash };
        let len = proto.encoded_len();
        let fill = vec![0; PACKET_SIZE - len];
        let mut buf = Vec::with_capacity(len);
        proto.encode(&mut buf)?;

        socket.write(&buf[..len]).await?;
        socket.write(&fill).await?;
        socket.flush().await?;
        Ok(((), socket))
    }
}

async fn read_from_socket(
    mut socket: impl TSocketAlias,
) -> Result<(Vec<u8>, impl TSocketAlias), io::Error> {
    let mut read = 0;
    let mut data: Vec<u8> = vec![];
    loop {
        let mut buff = [0u8; PACKET_SIZE];
        match socket.read(&mut buff).await {
            Ok(n) => {
                read += n;
                data.extend(&buff[..n]);
                if read >= PACKET_SIZE {
                    break;
                }
            }
            Err(e) => return Err(e),
        }
    }

    // Remove all extra null bytes from the buffer
    data.retain(|x| *x != 0u8);
    Ok((data, socket))
}

pub fn hash_contents(mut file: fs::File) -> Result<String, Error> {
    let mut state = Md5::default();
    let mut buffer = [0u8; HASH_BUFFER_SIZE];

    loop {
        match file.read(&mut buffer) {
            Ok(n) if n == 0 || n < HASH_BUFFER_SIZE => {
                state.update(&buffer[..n]);
                break;
            }
            Ok(n) => {
                state.update(&buffer[..n]);
            }
            Err(e) => return Err(e),
        };
    }
    Ok(hex::encode::<Vec<u8>>(state.finalize().to_vec()))
}

#[cfg(test)]
mod tests {
    use crate::p2p::transfer::metadata::hash_contents;
    use std::io::{Seek, SeekFrom, Write};

    #[test]
    #[cfg(not(target_os = "windows"))]
    fn test_hash_local_file() {
        let mut file = tempfile::tempfile().unwrap();
        write!(file, "I'll fly to device!").unwrap();
        file.seek(SeekFrom::Start(0)).unwrap();
        let result = hash_contents(file).unwrap();

        assert_eq!(result, "a909b834a8f95194ee2ce975e38cec31");
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn test_hash_local_file() {
        // Windows file has carriage endings, so the hash is different
        let mut file = tempfile::tempfile().unwrap();
        write!(file, "I'll fly to device!").unwrap();
        file.seek(SeekFrom::Start(0)).unwrap();
        let result = hash_contents(file).unwrap();

        assert_eq!(result, "a909b834a8f95194ee2ce975e38cec31");
    }
}
