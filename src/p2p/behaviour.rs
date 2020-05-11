use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::thread;
use std::time::Duration;

use async_std::sync::Mutex;

use futures::channel::mpsc::{Receiver, Sender};

use libp2p::core::{connection::ConnectionId, ConnectedPoint, Multiaddr, PeerId};
use libp2p::swarm::{
    DialPeerCondition, NetworkBehaviour, NetworkBehaviourAction, NotifyHandler, PollParameters,
    ProtocolsHandler, SubstreamProtocol,
};

use crate::p2p::commands::TransferCommand;
use crate::p2p::handler::{OneShotHandler, OneShotHandlerConfig};
use crate::p2p::peer::{CurrentPeers, Peer, PeerEvent};
use crate::p2p::protocol::{FileToSend, ProtocolEvent, TransferOut, TransferPayload};

const TIMEOUT: u64 = 600;

pub struct TransferBehaviour {
    pub peers: HashMap<PeerId, Peer>,
    pub connected_peers: HashSet<PeerId>,
    pub events: Vec<NetworkBehaviourAction<TransferPayload, TransferOut>>,
    payloads: Vec<FileToSend>,
    sender: Sender<PeerEvent>,
    receiver: Arc<Mutex<Receiver<TransferCommand>>>,
}

impl TransferBehaviour {
    pub fn new(sender: Sender<PeerEvent>, receiver: Arc<Mutex<Receiver<TransferCommand>>>) -> Self {
        TransferBehaviour {
            peers: HashMap::new(),
            connected_peers: HashSet::new(),
            events: vec![],
            payloads: vec![],
            sender,
            receiver,
        }
    }

    pub fn push_file(&mut self, file: FileToSend) -> Result<(), Box<dyn Error>> {
        Ok(self.payloads.push(file))
    }

    fn peers_event(&mut self) -> CurrentPeers {
        self.peers
            .clone()
            .into_iter()
            .filter(|(peer_id, _)| self.connected_peers.contains(peer_id))
            .into_iter()
            .map(|(_, peer)| peer.to_owned())
            .collect::<CurrentPeers>()
    }

    fn notify_frontend(&mut self, peers: Option<CurrentPeers>) -> Result<(), Box<dyn Error>> {
        let current_peers = match peers {
            Some(peers) => peers,
            None => self.peers_event(),
        };
        let event = PeerEvent::PeersUpdated(current_peers);
        Ok(self.sender.try_send(event)?)
    }

    pub fn add_peer(&mut self, peer_id: PeerId, addr: Multiaddr) -> Result<(), Box<dyn Error>> {
        let peer = Peer {
            name: peer_id.to_base58(),
            peer_id: peer_id.clone(),
            address: addr,
        };
        self.peers.insert(peer_id, peer);

        Ok(self.notify_frontend(None)?)
    }

    pub fn remove_peer(&mut self, peer_id: &PeerId) -> Result<(), Box<dyn Error>> {
        self.peers.remove(peer_id);
        self.connected_peers.remove(peer_id);

        Ok(self.notify_frontend(None)?)
    }
}

impl NetworkBehaviour for TransferBehaviour {
    type ProtocolsHandler = OneShotHandler<TransferPayload, TransferOut, ProtocolEvent>;
    type OutEvent = TransferPayload;

    fn new_handler(&mut self) -> Self::ProtocolsHandler {
        let timeout = Duration::from_secs(TIMEOUT);
        let tp = TransferPayload {
            name: "default".to_string(),
            path: "".to_string(),
            hash: "".to_string(),
            size_bytes: 0,
            sender_queue: self.sender.clone(),
            receiver: Arc::clone(&self.receiver),
        };
        let handler_config = OneShotHandlerConfig {
            inactive_timeout: timeout,
            substream_timeout: timeout,
        };
        let proto = SubstreamProtocol::new(tp).with_timeout(timeout);
        Self::ProtocolsHandler::new(proto, handler_config)
    }

    fn addresses_of_peer(&mut self, _peer_id: &PeerId) -> Vec<Multiaddr> {
        Vec::new()
    }

    fn inject_connected(&mut self, peer: &PeerId) {
        println!("Connected to: {:?}", peer);
        self.connected_peers.insert(peer.to_owned());
        if let Err(e) = self.notify_frontend(None) {
            eprintln!("Failed to notify frontend {:?}", e);
        };
    }

    fn inject_connection_established(
        &mut self,
        _: &PeerId,
        _: &ConnectionId,
        endpoint: &ConnectedPoint,
    ) {
        let peers = self
            .peers
            .clone()
            .into_iter()
            .map(|(_, mut peer)| match endpoint {
                ConnectedPoint::Dialer { address } => {
                    peer.address = address.clone();
                    peer
                }
                ConnectedPoint::Listener {
                    local_addr,
                    send_back_addr: _,
                } => {
                    peer.address = local_addr.clone();
                    peer
                }
            })
            .collect::<CurrentPeers>();

        if let Err(e) = self.notify_frontend(Some(peers)) {
            eprintln!("Failed to notify frontend {:?}", e);
        };
    }

    fn inject_dial_failure(&mut self, peer: &PeerId) {
        println!("Dial failure: {:?}", peer);
        self.connected_peers.remove(peer);
    }

    fn inject_disconnected(&mut self, peer: &PeerId) {
        println!("Disconnected: {:?}", peer);
        if let Err(e) = self.remove_peer(peer) {
            eprintln!("{:?}", e);
        }
    }

    fn inject_event(&mut self, peer: PeerId, c: ConnectionId, event: ProtocolEvent) {
        println!("Inject event: {:?}", event);
        match event {
            ProtocolEvent::Received(data) => {
                self.events.push(NetworkBehaviourAction::NotifyHandler {
                    handler: NotifyHandler::One(c),
                    peer_id: peer,
                    event: data,
                });
            }
            ProtocolEvent::Sent => return,
        };
    }

    fn poll(
        &mut self,
        _: &mut Context,
        _: &mut impl PollParameters,
    ) -> Poll<
        NetworkBehaviourAction<
            <Self::ProtocolsHandler as ProtocolsHandler>::InEvent,
            Self::OutEvent,
        >,
    > {
        if let Some(event) = self.events.pop() {
            match event {
                NetworkBehaviourAction::NotifyHandler {
                    peer_id: _,
                    handler: _,
                    event: send_event,
                } => {
                    let tp = TransferPayload {
                        name: send_event.name,
                        path: send_event.path,
                        hash: send_event.hash,
                        size_bytes: send_event.size_bytes,
                        sender_queue: self.sender.clone(),
                        receiver: Arc::clone(&self.receiver),
                    };
                    return Poll::Ready(NetworkBehaviourAction::GenerateEvent(tp));
                }
                NetworkBehaviourAction::GenerateEvent(e) => {
                    println!("GenerateEvent event {:?}", e);
                }
                _ => {
                    println!("Another event");
                }
            }
        };

        if self.connected_peers.len() > 0 {
            match self.payloads.pop() {
                Some(message) => {
                    let event = TransferOut {
                        name: message.name,
                        path: message.path,
                        sender_queue: self.sender.clone(),
                    };
                    return Poll::Ready(NetworkBehaviourAction::NotifyHandler {
                        handler: NotifyHandler::Any,
                        peer_id: message.peer,
                        event,
                    });
                }
                None => return Poll::Pending,
            }
        } else {
            for (peer_id, _) in self.peers.iter() {
                if !self.connected_peers.contains(peer_id) {
                    println!("Will try to dial: {:?}", peer_id);
                    let millis = Duration::from_millis(100);
                    thread::sleep(millis);
                    return Poll::Ready(NetworkBehaviourAction::DialPeer {
                        condition: DialPeerCondition::Disconnected,
                        peer_id: peer_id.to_owned(),
                    });
                }
            }
        }
        Poll::Pending
    }
}
