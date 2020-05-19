use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::sync::Arc;
use std::task::{Context, Poll};
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
    pub events: Vec<NetworkBehaviourAction<TransferOut, TransferPayload>>,
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
            inactive_timeout: Duration::from_secs(5),
            substream_timeout: timeout,
        };
        let proto = SubstreamProtocol::new(tp).with_timeout(timeout);
        Self::ProtocolsHandler::new(proto, handler_config)
    }

    fn addresses_of_peer(&mut self, _peer_id: &PeerId) -> Vec<Multiaddr> {
        Vec::new()
    }

    fn inject_connected(&mut self, peer: &PeerId) {
        info!("Connected to: {:?}", peer);
        self.connected_peers.insert(peer.to_owned());
        if let Err(e) = self.notify_frontend(None) {
            error!("Failed to notify frontend {:?}", e);
        };
    }

    fn inject_connection_established(
        &mut self,
        peer: &PeerId,
        c: &ConnectionId,
        endpoint: &ConnectedPoint,
    ) {
        match self.payloads.pop() {
            Some(message) => {
                let transfer = TransferOut {
                    name: message.name,
                    path: message.path,
                    sender_queue: self.sender.clone(),
                };

                let event = NetworkBehaviourAction::NotifyHandler {
                    handler: NotifyHandler::One(c.to_owned()),
                    peer_id: peer.to_owned(),
                    event: transfer,
                };
                self.events.push(event);
            }
            None => (),
        }

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
            error!("Failed to notify frontend {:?}", e);
        };
    }

    fn inject_dial_failure(&mut self, peer: &PeerId) {
        warn!("Dial failure: {:?}", peer);
        self.connected_peers.remove(peer);
    }

    fn inject_disconnected(&mut self, peer: &PeerId) {
        info!("Disconnected: {:?}", peer);
        if let Err(e) = self.remove_peer(peer) {
            error!("{:?}", e);
        }
    }

    fn inject_event(&mut self, _: PeerId, _: ConnectionId, event: ProtocolEvent) {
        info!("Inject event: {}", event);
        match event {
            ProtocolEvent::Received(data) => self
                .events
                .push(NetworkBehaviourAction::GenerateEvent(data)),
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
        for file in self.payloads.iter() {
            info!("Will try to dial: {:?}", file.peer);
            if !self.connected_peers.contains(&file.peer) {
                return Poll::Ready(NetworkBehaviourAction::DialPeer {
                    condition: DialPeerCondition::Disconnected,
                    peer_id: file.peer.to_owned(),
                });
            }
        }

        if let Some(event) = self.events.pop() {
            match event {
                NetworkBehaviourAction::NotifyHandler {
                    peer_id,
                    handler,
                    event: send_event,
                } => {
                    let out = TransferOut {
                        name: send_event.name,
                        path: send_event.path,
                        sender_queue: self.sender.clone(),
                    };
                    let event = NetworkBehaviourAction::NotifyHandler {
                        handler,
                        peer_id,
                        event: out,
                    };

                    return Poll::Ready(event);
                }
                NetworkBehaviourAction::GenerateEvent(e) => {
                    return Poll::Ready(NetworkBehaviourAction::GenerateEvent(e));
                }
                _ => {
                    info!("Another event");
                    return Poll::Pending;
                }
            }
        } else {
            return Poll::Pending;
        }
    }
}
