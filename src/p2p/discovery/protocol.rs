use std::{fmt, io, iter, pin::Pin};

use futures::prelude::*;
use libp2p::core::{upgrade, InboundUpgrade, OutboundUpgrade, PeerId, UpgradeInfo};
use prost::Message;

use super::proto::Host;

use crate::p2p::peer::OperatingSystem;

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

impl<TSocket> InboundUpgrade<TSocket> for Discovery
where
    TSocket: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    type Output = Discovery;
    type Error = io::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Output, Self::Error>> + Send>>;

    fn upgrade_inbound(self, mut socket: TSocket, _info: Self::Info) -> Self::Future {
        Box::pin(async move {
            let data = match upgrade::read_one(&mut socket, 1024).await {
                Ok(value) => value,
                Err(err) => match err {
                    upgrade::ReadOneError::Io(e) => {
                        error!("IO error: {:?}", e);
                        return Err(e);
                    }
                    upgrade::ReadOneError::TooLarge {
                        requested: _,
                        max: _,
                    } => {
                        error!("Payload too large!");
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "Payload too large",
                        ));
                    }
                },
            };
            let host = Host::decode(&data[..])?;
            let os = match OperatingSystem::from_i32(host.os) {
                Some(v) => v,
                None => OperatingSystem::Unknown,
            };
            Ok(Discovery {
                hostname: host.hostname,
                os,
            })
        })
    }
}

impl<TSocket> OutboundUpgrade<TSocket> for Discovery
where
    TSocket: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    type Output = ();
    type Error = io::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Output, Self::Error>> + Send>>;

    fn upgrade_outbound(self, mut socket: TSocket, _info: Self::Info) -> Self::Future {
        Box::pin(async move {
            let proto = Host {
                hostname: self.hostname,
                os: self.os as i32,
            };
            let mut buf = Vec::with_capacity(proto.encoded_len());

            proto.encode(&mut buf)?;
            upgrade::write_one(&mut socket, buf).await?;
            Ok(())
        })
    }
}
