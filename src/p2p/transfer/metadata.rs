use std::fmt;
use std::io::{self, Error};

use super::proto::Answer as ProtoAnswer;
use super::proto::Metadata as ProtoMetadata;
use super::proto::Trailer as ProtoTrailer;
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
    pub size: usize,
    pub transfer_type: TransferType,
}

impl Metadata {
    pub async fn read(socket: impl TSocketAlias) -> Result<(Self, impl TSocketAlias), io::Error> {
        let (data, socket) = read_from_socket(socket).await?;
        let proto = ProtoMetadata::decode(&data[..])?;

        let name = proto.name;
        let size = proto.size as usize;
        let transfer_type =
            TransferType::try_from(proto.transfer_type).unwrap_or(TransferType::File);
        info!("Read: Name: {}, Size: {}", name, size);
        Ok((
            Metadata {
                name,
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
        let size = file.get_size().await?;

        let proto = ProtoMetadata {
            name: file.name.to_string(),
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
            TransferType::Dir => self.name.to_string(),
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
            "Metadata:\n name: {}\n size: {}\n type: {}\n",
            self.name, self.size, self.transfer_type
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

#[derive(Debug)]
pub struct Trailer;

impl Trailer {
    pub async fn read(socket: &mut impl TSocketAlias) -> Result<String, io::Error> {
        let (data, _rest) = read_from_socket_ref(socket).await?;
        let proto = ProtoTrailer::decode(&data[..])?;
        Ok(proto.hash)
    }

    pub async fn write(socket: &mut impl TSocketAlias, hash: String) -> Result<(), io::Error> {
        let proto = ProtoTrailer { hash };
        let len = proto.encoded_len();
        let fill = vec![0; PACKET_SIZE - len];
        let mut buf = Vec::with_capacity(len);
        proto.encode(&mut buf)?;

        socket.write(&buf[..len]).await?;
        socket.write(&fill).await?;
        socket.flush().await?;
        Ok(())
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

// Same as read_from_socket but borrows the socket so the caller keeps ownership.
async fn read_from_socket_ref(socket: &mut impl TSocketAlias) -> Result<(Vec<u8>, ()), io::Error> {
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

    data.retain(|x| *x != 0u8);
    Ok((data, ()))
}

pub async fn hash_contents(mut file: impl AsyncRead + Unpin) -> Result<(String, u64), Error> {
    let mut state = Md5::default();
    let mut buffer = [0u8; HASH_BUFFER_SIZE];
    let mut i: u64 = 0;
    loop {
        match file.read(&mut buffer).await {
            Ok(n) if n == 0 => {
                break;
            }
            Ok(n) => {
                i += n as u64;
                state.update(&buffer[..n]);
            }
            Err(e) => return Err(e),
        };
    }
    let hash = hex::encode::<Vec<u8>>(state.finalize().to_vec());
    Ok((hash, i))
}

#[cfg(test)]
mod tests {
    use crate::p2p::transfer::metadata::hash_contents;
    use std::io::{Seek, SeekFrom, Write};
    use tokio_util::compat::TokioAsyncReadCompatExt;

    #[tokio::test]
    #[cfg(not(target_os = "windows"))]
    async fn test_hash_local_file() {
        let mut file = tempfile::tempfile().unwrap();
        write!(file, "I'll fly to device!").unwrap();
        file.seek(SeekFrom::Start(0)).unwrap();
        let tokio_file = tokio::fs::File::from_std(file);
        let async_file = tokio_file.compat();
        let (hash, size) = hash_contents(async_file).await.unwrap();

        assert_eq!(hash, "a909b834a8f95194ee2ce975e38cec31".to_string());
        assert_eq!(size, 19);
    }

    #[tokio::test]
    #[cfg(target_os = "windows")]
    async fn test_hash_local_file() {
        // Windows file has carriage endings, so the hash is different
        let mut file = tempfile::tempfile().unwrap();
        write!(file, "I'll fly to device!").unwrap();
        file.seek(SeekFrom::Start(0)).unwrap();
        let tokio_file = tokio::fs::File::from_std(file);
        let async_file = tokio_file.compat();
        let (hash, size) = hash_contents(async_file).await.unwrap();

        assert_eq!(hash, "a909b834a8f95194ee2ce975e38cec31".to_string());
        assert_eq!(size, 19);
    }
}
