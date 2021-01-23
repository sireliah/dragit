use std::fmt;
use std::fs;
use std::io::{self, Error, Read};

use super::proto::Answer as ProtoAnswer;
use super::proto::Metadata as ProtoMetadata;
use futures::prelude::*;
use hex;
use md5::{Digest, Md5};
use prost::{Enumeration, Message};

use crate::p2p::transfer::protocol::FileToSend;

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
    pub async fn read<TSocket>(mut socket: TSocket) -> Result<(Self, TSocket), io::Error>
    where
        TSocket: AsyncRead + AsyncWrite + Send + Unpin,
    {
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
        let proto = ProtoMetadata::decode(&data[..])?;

        let name = proto.name;
        let hash = proto.hash;
        let size = proto.size as usize;
        let transfer_type =
            TransferType::from_i32(proto.transfer_type).unwrap_or(TransferType::Other);
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

    pub async fn write<TSocket>(
        file: &FileToSend,
        mut socket: TSocket,
    ) -> Result<(usize, TSocket), io::Error>
    where
        TSocket: AsyncRead + AsyncWrite + Send + Unpin,
    {
        let hash = file.calculate_hash().await?;
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
}

#[derive(Debug)]
pub struct Answer;

impl Answer {
    pub async fn read<TSocket>(mut socket: TSocket) -> Result<(bool, TSocket), io::Error>
    where
        TSocket: AsyncRead + AsyncWrite + Send + Unpin,
    {
        // Answer size is not expected to grow, that's why constant size is used here
        let mut received = [0u8; ANSWER_SIZE];
        socket.read_exact(&mut received).await?;
        let proto = ProtoAnswer::decode(&received[..])?;
        Ok((proto.accepted, socket))
    }
    pub async fn write<TSocket>(
        mut socket: TSocket,
        accepted: bool,
    ) -> Result<((), TSocket), io::Error>
    where
        TSocket: AsyncRead + AsyncWrite + Send + Unpin,
    {
        let proto = ProtoAnswer { accepted };
        let len = proto.encoded_len();
        let mut buf = Vec::with_capacity(len);
        proto.encode(&mut buf)?;

        socket.write(&buf).await?;
        socket.flush().await?;
        Ok(((), socket))
    }
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Enumeration)]
pub enum TransferType {
    File = 0,
    Text = 1,
    Other = 2,
}

impl fmt::Display for TransferType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::File => write!(f, "TransferType: File"),
            Self::Text => write!(f, "TransferType: Text"),
            _ => write!(f, "TransferType: Other"),
        }
    }
}

pub fn add_row(value: &str) -> Vec<u8> {
    format!("{}\n", value).into_bytes()
}

pub fn hash_contents(mut file: fs::File) -> Result<String, Error> {
    let mut state = Md5::default();
    let mut buffer = [0u8; HASH_BUFFER_SIZE];

    loop {
        match file.read(&mut buffer) {
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

#[cfg(test)]
mod tests {
    use crate::p2p::transfer::metadata::hash_contents;
    use std::fs::File;

    #[test]
    fn test_hash_local_file() {
        let file = File::open("src/file.txt").unwrap();
        let result = hash_contents(file).unwrap();

        assert_eq!(result, "696c56be6d4c4a48d3de0d17e237f82a");
    }
}
