use std::fs;
use std::io::{self, Error, Read};

use super::proto::Answer as ProtoAnswer;
use super::proto::Metadata as ProtoMetadata;
use futures::prelude::*;
use hex;
use md5::{Digest, Md5};
use prost::Message;

pub const ANSWER_SIZE: usize = 2;
pub const PACKET_SIZE: usize = 1024;
pub const HASH_BUFFER_SIZE: usize = 1024;

pub struct Metadata {
    pub name: String,
    pub hash: String,
    pub size: usize,
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
        info!("Read: Name: {}, Hash: {}, Size: {}", name, hash, size);
        Ok((Metadata { name, hash, size }, socket))
    }

    pub async fn write<TSocket>(
        name: &str,
        path: &str,
        mut socket: TSocket,
    ) -> Result<(usize, TSocket), io::Error>
    where
        TSocket: AsyncRead + AsyncWrite + Send + Unpin,
    {
        let hash = calculate_hash(path).await?;
        let size = check_size(path)?;

        let proto = ProtoMetadata {
            name: name.to_string(),
            hash,
            size,
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

async fn calculate_hash(path: &str) -> Result<String, io::Error> {
    Ok(hash_contents(path)?)
}

pub fn check_size(path: &str) -> Result<u64, Error> {
    let meta = fs::metadata(path)?;
    Ok(meta.len())
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

#[cfg(test)]
mod tests {
    use crate::p2p::transfer::metadata::hash_contents;

    #[test]
    fn test_hash_local_file() {
        let result = hash_contents("src/file.txt").unwrap();

        assert_eq!(result, "696c56be6d4c4a48d3de0d17e237f82a");
    }
}
