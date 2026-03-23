use std::{fmt, io, iter, pin::Pin};

use futures::io::AsyncWriteExt;
use futures::prelude::*;
use libp2p::core::{InboundUpgrade, OutboundUpgrade, PeerId, UpgradeInfo};
use prost::Message;

use super::proto::Host;

use crate::p2p::peer::OperatingSystem;

#[derive(Debug)]
pub struct DiscoveryEvent {
    pub peer: PeerId,
    pub hostname: String,
    pub os: OperatingSystem,
}

impl fmt::Display for DiscoveryEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "DiscoveryEvent: peer: {}, hostname: {}, os: {:?}",
            self.peer, self.hostname, self.os
        )
    }
}

#[derive(Clone, Debug)]
pub struct Discovery {
    pub hostname: String,
    pub os: OperatingSystem,
}

impl Default for Discovery {
    fn default() -> Self {
        Discovery {
            hostname: "".to_string(),
            os: OperatingSystem::Linux,
        }
    }
}

impl UpgradeInfo for Discovery {
    type Info = &'static str;
    type InfoIter = iter::Once<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        iter::once("/discovery/1.0")
    }
}

/// Encode a `Host` protobuf message as a u32 big-endian length-prefixed frame.
fn encode_peer(hostname: String, os: OperatingSystem) -> Result<Vec<u8>, io::Error> {
    let proto = Host {
        hostname,
        os: os as i32,
    };
    let payload_len = proto.encoded_len();
    let mut buf = Vec::with_capacity(4 + payload_len);
    let len = payload_len as u32;
    buf.extend_from_slice(&len.to_be_bytes());
    proto.encode(&mut buf)?;
    Ok(buf)
}

/// Decode a `Discovery` from a u32 big-endian length-prefixed protobuf frame
/// read from `socket`.
async fn read_peer(mut socket: impl AsyncRead + Unpin) -> Result<Discovery, io::Error> {
    let mut len_buf = [0u8; 4];
    socket.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > 4096 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Discovery message too large: {}", len),
        ));
    }
    let mut data = vec![0u8; len];
    socket.read_exact(&mut data).await?;
    let host = Host::decode(&data[..])?;
    let os = match OperatingSystem::try_from(host.os) {
        Ok(v) => v,
        Err(_) => OperatingSystem::Unknown,
    };
    Ok(Discovery {
        hostname: host.hostname,
        os,
    })
}

/// Exchange host information symmetrically: write our data and read the remote's
/// data concurrently so that neither side has to go first. This works regardless
/// of which side opened the substream.
async fn exchange_peer_info(
    socket: impl AsyncRead + AsyncWrite + Send + Unpin + 'static,
    hostname: String,
    os: OperatingSystem,
) -> Result<Discovery, io::Error> {
    let outgoing = encode_peer(hostname, os)?;
    let (reader, mut writer) = futures::io::AsyncReadExt::split(socket);

    let write_fut = async move {
        writer.write_all(&outgoing).await?;
        writer.flush().await?;
        // Close the write half so the remote's read_exact can reach EOF if needed.
        writer.close().await?;
        Ok::<(), io::Error>(())
    };

    let read_fut = read_peer(reader);

    let (write_res, read_res) = futures::future::join(write_fut, read_fut).await;
    write_res?;
    read_res
}

impl<TSocket> InboundUpgrade<TSocket> for Discovery
where
    TSocket: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    type Output = Discovery;
    type Error = io::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Output, Self::Error>> + Send>>;

    fn upgrade_inbound(self, socket: TSocket, _info: Self::Info) -> Self::Future {
        // Fully symmetric: write and read concurrently, no ordering dependency.
        Box::pin(exchange_peer_info(socket, self.hostname, self.os))
    }
}

impl<TSocket> OutboundUpgrade<TSocket> for Discovery
where
    TSocket: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    type Output = Discovery;
    type Error = io::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Output, Self::Error>> + Send>>;

    fn upgrade_outbound(self, socket: TSocket, _info: Self::Info) -> Self::Future {
        // Fully symmetric: write and read concurrently, no ordering dependency.
        Box::pin(exchange_peer_info(socket, self.hostname, self.os))
    }
}
