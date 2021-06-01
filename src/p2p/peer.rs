use std::fmt;

use libp2p::{Multiaddr, PeerId};
use prost::Enumeration;

use crate::p2p::Payload;

#[derive(Debug, Clone)]
pub enum Direction {
    Incoming,
    Outgoing,
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Enumeration)]
pub enum TransferType {
    File = 0,
    Text = 1,
}

#[derive(Debug, Clone)]
pub enum PeerEvent {
    PeersUpdated(CurrentPeers),
    WaitingForAnswer,
    TransferRejected,
    TransferProgress((usize, usize, Direction)),
    TransferCompleted,
    FileCorrect(String, Payload),
    FileIncorrect,
    FileIncoming(String, String, usize, TransferType),
    Error(String),
}

pub type CurrentPeers = Vec<Peer>;

#[derive(Debug, Eq, Hash, Clone)]
pub struct Peer {
    pub name: String,
    pub address: Multiaddr,
    pub peer_id: PeerId,
    pub hostname: String,
    pub os: OperatingSystem,
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

impl fmt::Display for TransferType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::File => write!(f, "TransferType: File"),
            Self::Text => write!(f, "TransferType: Text"),
        }
    }
}
