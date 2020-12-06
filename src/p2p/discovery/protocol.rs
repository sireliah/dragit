use std::{fmt, io, iter, pin::Pin};

use futures::prelude::*;
use libp2p::core::{upgrade, InboundUpgrade, OutboundUpgrade, PeerId, UpgradeInfo};

type DiscoverySuccess = String;
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
}

impl Default for Discovery {
    fn default() -> Self {
        Discovery {
            hostname: "".to_string(),
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
                        error!("Too large!");
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "Payload too large",
                        ));
                    }
                },
            };
            let hostname = String::from_utf8_lossy(&data).into_owned();
            Ok(Discovery { hostname })
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
            let hostname = "Hdzia".to_string();
            upgrade::write_one(&mut socket, hostname).await?;
            Ok(())
        })
    }
}
