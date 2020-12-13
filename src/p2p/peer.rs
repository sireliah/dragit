use libp2p::{Multiaddr, PeerId};

use prost::Enumeration;

#[derive(Debug, Clone)]
pub enum Direction {
    Incoming,
    Outgoing,
}

#[derive(Debug, Clone)]
pub enum PeerEvent {
    PeersUpdated(CurrentPeers),
    TransferProgress((usize, usize, Direction)),
    TransferCompleted,
    TransferError,
    FileCorrect(String, String),
    FileIncorrect,
    FileIncoming(String, String),
    Error(String),
}

pub type CurrentPeers = Vec<Peer>;

#[derive(Debug, Eq, Hash, Clone)]
pub struct Peer {
    pub name: String,
    pub address: Multiaddr,
    pub peer_id: PeerId,
    pub hostname: Option<String>,
    pub os: Option<OperatingSystem>,
}

impl PartialEq for Peer {
    fn eq(&self, other: &Self) -> bool {
        self.peer_id == other.peer_id
    }
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Enumeration)]
pub enum OperatingSystem {
    Linux = 0,
    Windows = 1,
    Macos = 2,
    Other = 3,
    Unknown = 4,
}
