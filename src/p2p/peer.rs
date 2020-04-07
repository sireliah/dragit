use libp2p::PeerId;

#[derive(Debug, Eq, Hash, Clone)]
pub struct Peer {
    pub name: String,
    pub peer_id: PeerId,
}

impl PartialEq for Peer {
    fn eq(&self, other: &Self) -> bool {
        self.peer_id == other.peer_id
    }
}
