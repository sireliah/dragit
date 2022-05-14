use std::{fmt, io, iter, pin::Pin};

use futures::prelude::*;
use libp2p::core::{upgrade, InboundUpgrade, OutboundUpgrade, PeerId, UpgradeInfo};
use prost::Message;

use super::proto::Host;

use crate::p2p::peer::OperatingSystem;
use crate::p2p::util::TSocketAlias;

type DiscoverySuccess = (String, OperatingSystem);
type DiscoveryFailure = io::Error;
pub type DiscoveryResult = Result<DiscoverySuccess, DiscoveryFailure>;

#[derive(Debug)]
pub struct DiscoveryEvent {
    pub peer: PeerId,
    pub result: DiscoveryResult,
}

impl fmt::Display for DiscoveryEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "DiscoveryEvent: result: {:?}, peer: {}",
            self.result, self.peer
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

async fn read_peer(
    mut socket: impl TSocketAlias,
) -> Result<(Discovery, impl TSocketAlias), io::Error> {
    let data = upgrade::read_length_prefixed(&mut socket, 1024).await?;
    let host = Host::decode(&data[..])?;
    let os = match OperatingSystem::from_i32(host.os) {
        Some(v) => v,
        None => OperatingSystem::Unknown,
    };
    let discovery = Discovery {
        hostname: host.hostname,
        os,
    };
    Ok((discovery, socket))
}

async fn write_peer(
    hostname: String,
    os: OperatingSystem,
    mut socket: impl TSocketAlias,
) -> Result<impl TSocketAlias, io::Error> {
    let proto = Host {
        hostname,
        os: os as i32,
    };
    let mut buf = Vec::with_capacity(proto.encoded_len());

    proto.encode(&mut buf)?;
    upgrade::write_length_prefixed(&mut socket, buf).await?;

    Ok(socket)
}

impl<TSocket> InboundUpgrade<TSocket> for Discovery
where
    TSocket: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    type Output = Discovery;
    type Error = io::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Output, Self::Error>> + Send>>;

    fn upgrade_inbound(self, socket: TSocket, _info: Self::Info) -> Self::Future {
        // (As dialer) receiving the host data from remote
        // and sending own data immediately after
        Box::pin(async move {
            let (discovery, socket) = read_peer(socket).await?;
            let _ = write_peer(self.hostname, self.os, socket).await?;

            Ok(discovery)
        })
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
        // (As listener) sending the host data to remote
        // and receiving remote host data in exchange
        Box::pin(async move {
            let socket = write_peer(self.hostname, self.os, socket).await?;
            let (discovery, _) = read_peer(socket).await?;
            Ok(discovery)
        })
    }
}
