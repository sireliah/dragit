use std::{
    collections::{HashMap, VecDeque},
    error::Error,
    fmt,
    task::{Context, Poll},
    time::Duration,
};

use async_std::sync::Sender;
use hostname;
use libp2p::core::{connection::ConnectionId, ConnectedPoint, Multiaddr, PeerId};
use libp2p::swarm::{
    protocols_handler::SubstreamProtocol, DialPeerCondition, NetworkBehaviour,
    NetworkBehaviourAction, NotifyHandler, OneShotHandler, OneShotHandlerConfig, PollParameters,
    ProtocolsHandler,
};

use crate::p2p::discovery::protocol::{Discovery, DiscoveryEvent};
use crate::p2p::peer::{CurrentPeers, OperatingSystem, Peer, PeerEvent};

#[derive(Debug)]
pub enum InnerMessage {
    Received(Discovery),
    Sent,
}

impl fmt::Display for InnerMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InnerMessage::Sent => write!(f, "InnerMessage sent"),
            InnerMessage::Received(discovery) => {
                write!(f, "InnerMessage received: {:?}", discovery)
            }
        }
    }
}

impl From<Discovery> for InnerMessage {
    fn from(discovery: Discovery) -> InnerMessage {
        InnerMessage::Received(discovery)
    }
}

impl From<()> for InnerMessage {
    fn from(_: ()) -> InnerMessage {
        InnerMessage::Sent
    }
}

#[derive(Debug)]
pub struct DiscoveryBehaviour {
    events: VecDeque<NetworkBehaviourAction<Discovery, DiscoveryEvent>>,
    peers: HashMap<PeerId, Peer>,
    hostname: String,
    os: OperatingSystem,
    sender: Sender<PeerEvent>,
}

impl DiscoveryBehaviour {
    pub fn new(sender: Sender<PeerEvent>) -> Self {
        DiscoveryBehaviour {
            events: VecDeque::new(),
            peers: HashMap::new(),
            hostname: Self::get_hostname(),
            os: Self::get_os(),
            sender,
        }
    }

    fn get_hostname() -> String {
        match hostname::get() {
            Ok(value) => value.to_string_lossy().into(),
            Err(e) => {
                error!("Failed to get hostname: {:?}", e);
                "".to_string()
            }
        }
    }

    fn get_os() -> OperatingSystem {
        if cfg!(target_os = "linux") {
            OperatingSystem::Linux
        } else if cfg!(target_os = "windows") {
            OperatingSystem::Windows
        } else {
            OperatingSystem::Other
        }
    }

    fn peers_event(&mut self) -> CurrentPeers {
        self.peers
            .clone()
            .into_iter()
            .map(|(_, peer)| peer.to_owned())
            .collect::<CurrentPeers>()
    }

    pub fn notify_frontend(&mut self, peers: Option<CurrentPeers>) -> Result<(), Box<dyn Error>> {
        let current_peers = match peers {
            Some(peers) => peers,
            None => self.peers_event(),
        };
        let event = PeerEvent::PeersUpdated(current_peers);
        Ok(self.sender.try_send(event)?)
    }

    fn dial_peer(&mut self, peer_id: PeerId, addr: Multiaddr, insert_peer: bool) {
        self.events.push_back(NetworkBehaviourAction::DialPeer {
            condition: DialPeerCondition::NotDialing,
            peer_id: peer_id.clone(),
        });

        if insert_peer {
            let peer = Peer {
                name: peer_id.to_base58(),
                peer_id: peer_id.clone(),
                address: addr,
                hostname: "Not known yet".to_string(),
                os: OperatingSystem::Unknown,
            };
            self.peers.insert(peer_id, peer);
        }
    }

    pub fn add_peer(&mut self, peer_id: PeerId, addr: Multiaddr) {
        match self.peers.get(&peer_id) {
            // Keep dialing if server didn't get host details yet
            Some(peer) if peer.os == OperatingSystem::Unknown => {
                info!("OS unknown, dialing... {:?}", peer_id);
                self.dial_peer(peer_id, addr, false);
            }
            Some(_) => (),
            None => {
                info!("Peer not found, dialing... {:?}", peer_id);
                self.dial_peer(peer_id, addr, true);
            }
        }
    }

    pub fn remove_peer(&mut self, peer_id: &PeerId) -> Result<(), Box<dyn Error>> {
        self.peers.remove(peer_id);

        if let Err(e) = self.notify_frontend(None) {
            error!("Failed to notify the frontend: {:?}", e);
        }
        Ok(())
    }

    pub fn update_peer(
        &mut self,
        peer_id: PeerId,
        address: Multiaddr,
        hostname: String,
        os: OperatingSystem,
    ) {
        match self.peers.get_mut(&peer_id) {
            Some(peer) => {
                info!("Updating peer. {:?}", peer_id);
                peer.hostname = hostname;
                peer.os = os;
            }
            None => {
                error!("Peer not found! {:?}", peer_id);
                let peer = Peer {
                    name: peer_id.to_base58(),
                    peer_id: peer_id.clone(),
                    address,
                    hostname,
                    os,
                };
                self.peers.insert(peer_id, peer);
            }
        }

        if let Err(e) = self.notify_frontend(None) {
            error!("Failed to notify the frontend: {:?}", e);
        }
    }
}

impl NetworkBehaviour for DiscoveryBehaviour {
    type ProtocolsHandler = OneShotHandler<Discovery, Discovery, InnerMessage>;
    type OutEvent = DiscoveryEvent;

    fn new_handler(&mut self) -> Self::ProtocolsHandler {
        let substream_proto = SubstreamProtocol::new(Discovery {
            hostname: self.hostname.clone(),
            os: self.os,
            address: Multiaddr::empty(),
        });
        let handler_config = OneShotHandlerConfig {
            keep_alive_timeout: Duration::from_secs(1),
            outbound_substream_timeout: Duration::from_secs(2),
        };
        Self::ProtocolsHandler::new(substream_proto, handler_config)
    }

    fn addresses_of_peer(&mut self, _peer_id: &PeerId) -> Vec<Multiaddr> {
        Vec::new()
    }

    fn inject_connected(&mut self, _peer_id: &PeerId) {}

    fn inject_connection_established(
        &mut self,
        peer_id: &PeerId,
        c: &ConnectionId,
        endpoint: &ConnectedPoint,
    ) {
        info!("Discovery: connection established: {:?}", endpoint);
        match endpoint {
            ConnectedPoint::Dialer { address } => {
                if let Some(peer) = self.peers.get_mut(peer_id) {
                    peer.address = address.clone();
                };
                let event = NetworkBehaviourAction::NotifyHandler {
                    peer_id: peer_id.to_owned(),
                    handler: NotifyHandler::One(*c),
                    event: Discovery {
                        hostname: self.hostname.clone(),
                        os: self.os.clone(),
                        address: address.to_owned(),
                    },
                };
                self.events.push_back(event);
            }
            ConnectedPoint::Listener {
                local_addr,
                send_back_addr,
            } => {
                info!(
                    "Listener: remote: {:?}, local: {:?}",
                    send_back_addr, local_addr
                );
                if let Some(peer) = self.peers.get_mut(peer_id) {
                    peer.address = send_back_addr.to_owned();
                } else {
                    let peer = Peer {
                        name: peer_id.to_base58(),
                        peer_id: peer_id.clone(),
                        address: send_back_addr.to_owned(),
                        hostname: "Not known yet".to_string(),
                        os: OperatingSystem::Unknown,
                    };
                    self.peers.insert(peer_id.to_owned(), peer);
                }
            }
        };
    }

    fn inject_disconnected(&mut self, _peer: &PeerId) {}

    fn inject_event(&mut self, peer: PeerId, _connection: ConnectionId, event: InnerMessage) {
        match event {
            InnerMessage::Received(ev) => {
                let message = DiscoveryEvent {
                    peer,
                    address: ev.address,
                    result: Ok((ev.hostname, ev.os)),
                };
                self.events
                    .push_back(NetworkBehaviourAction::GenerateEvent(message));
            }
            InnerMessage::Sent => return,
        };
    }

    fn poll(
        &mut self,
        _: &mut Context<'_>,
        _: &mut impl PollParameters,
    ) -> Poll<
        NetworkBehaviourAction<
            <Self::ProtocolsHandler as ProtocolsHandler>::InEvent,
            Self::OutEvent,
        >,
    > {
        if let Some(event) = self.events.pop_front() {
            Poll::Ready(event)
        } else {
            Poll::Pending
        }
    }
}
