use std::collections::HashSet;
use std::error::Error;
use std::fs;
use std::path::Path;
use std::task::{Context, Poll};
use std::thread;
use std::time::Duration;

use libp2p::core::{ConnectedPoint, Multiaddr, PeerId};
use libp2p::swarm::{NetworkBehaviour, NetworkBehaviourAction, PollParameters, SubstreamProtocol};

use crate::p2p::handler::OneShotHandler;
use crate::p2p::protocol::{FileToSend, ProtocolEvent, TransferPayload};

pub struct TransferBehaviour {
    pub peers: HashSet<PeerId>,
    pub connected_peers: HashSet<PeerId>,
    pub events: Vec<NetworkBehaviourAction<TransferPayload, TransferPayload>>,
    payloads: Vec<FileToSend>,
}

impl TransferBehaviour {
    pub fn new() -> Self {
        TransferBehaviour {
            peers: HashSet::new(),
            connected_peers: HashSet::new(),
            events: vec![],
            payloads: vec![],
        }
    }

    pub fn push_file(&mut self, file: FileToSend) -> Result<(), Box<dyn Error>> {
        Ok(self.payloads.push(file))
    }

    pub fn push_payload(&mut self, filename: String) -> Result<(), Box<dyn Error>> {
        fs::metadata(&filename)?;
        let path = Path::new(&filename).canonicalize()?;
        let name = path
            .file_name()
            .expect("There is no file name")
            .to_str()
            .expect("Expected a name")
            .to_string();
        let path_string = path.to_str().expect("Expected a path name").to_string();
        let file = FileToSend {
            name,
            path: path_string,
        };
        self.payloads.push(file);
        Ok(())
    }
}

impl NetworkBehaviour for TransferBehaviour {
    type ProtocolsHandler = OneShotHandler<TransferPayload, TransferPayload, ProtocolEvent>;
    type OutEvent = TransferPayload;

    fn new_handler(&mut self) -> Self::ProtocolsHandler {
        let timeout = Duration::from_secs(120);
        let tp = TransferPayload::default();
        let proto = SubstreamProtocol::new(tp).with_timeout(timeout);
        Self::ProtocolsHandler::new(proto, timeout, timeout)
    }

    fn addresses_of_peer(&mut self, _peer_id: &PeerId) -> Vec<Multiaddr> {
        Vec::new()
    }

    fn inject_connected(&mut self, peer: PeerId, point: ConnectedPoint) {
        println!("Connected to peer: {:?} {:?}", peer, point);
        match point {
            ConnectedPoint::Dialer { address: _ } => {
                println!("I'm a dialer now.");
            }
            ConnectedPoint::Listener {
                local_addr: _,
                send_back_addr: _,
            } => println!("I am listener now"),
        };
        self.connected_peers.insert(peer);
    }

    fn inject_dial_failure(&mut self, peer: &PeerId) {
        println!("Dial failure {:?}", peer);
        self.connected_peers.remove(peer);
    }

    fn inject_disconnected(&mut self, peer: &PeerId, _: ConnectedPoint) {
        println!("Disconnected: {:?}", peer);
        self.connected_peers.remove(peer);
        self.peers.remove(peer);
    }

    fn inject_node_event(&mut self, _peer: PeerId, event: ProtocolEvent) {
        match event {
            ProtocolEvent::Received {
                name,
                hash,
                path,
                size_bytes,
            } => {
                let event = TransferPayload::new(name, hash, path, size_bytes);
                self.events
                    .push(NetworkBehaviourAction::GenerateEvent(event));
            }
            ProtocolEvent::Sent => println!("Node Sent event"),
        };
    }

    fn poll(
        &mut self,
        _: &mut Context,
        _: &mut impl PollParameters,
    ) -> Poll<NetworkBehaviourAction<TransferPayload, TransferPayload>> {
        match self.events.pop() {
            Some(e) => {
                println!("Got event from the queue: {:?}", e);
                return Poll::Ready(e);
            }
            None => {}
        };

        if self.connected_peers.len() > 0 {
            let peer = self.connected_peers.iter().nth(0).unwrap();
            match self.payloads.pop() {
                Some(value) => {
                    let event = TransferPayload::new(value.name, value.path, "".to_string(), 0);
                    return Poll::Ready(NetworkBehaviourAction::SendEvent {
                        peer_id: peer.to_owned(),
                        event,
                    });
                }
                None => return Poll::Pending,
            }
        } else {
            for peer in self.peers.iter() {
                if !self.connected_peers.contains(peer) {
                    println!("Will try to dial: {:?}", peer);
                    let millis = Duration::from_millis(100);
                    thread::sleep(millis);
                    return Poll::Ready(NetworkBehaviourAction::DialPeer {
                        peer_id: peer.to_owned(),
                    });
                } else {
                    match self.payloads.pop() {
                        Some(value) => {
                            let event =
                                TransferPayload::new(value.name, value.path, "".to_string(), 0);
                            return Poll::Ready(NetworkBehaviourAction::SendEvent {
                                peer_id: peer.to_owned(),
                                event,
                            });
                        }
                        None => return Poll::Pending,
                    }
                }
            }
        }
        Poll::Pending
    }
}
