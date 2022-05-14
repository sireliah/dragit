use std::{
    collections::{HashMap, VecDeque},
    error::Error,
    task::{Context, Poll},
    time::Duration,
};

use async_std::channel::Sender;
use hostname;
use libp2p::core::{connection::ConnectionId, ConnectedPoint, Multiaddr, PeerId};
use libp2p::swarm::{
    dial_opts::DialOpts, NetworkBehaviour, NetworkBehaviourAction, NotifyHandler, PollParameters,
    SubstreamProtocol,
};

use crate::p2p::discovery::handler::KeepAliveHandler;
use crate::p2p::discovery::protocol::{Discovery, DiscoveryEvent};
use crate::p2p::peer::{CurrentPeers, OperatingSystem, Peer, PeerEvent};

type Handler = KeepAliveHandler<Discovery, Discovery, Discovery>;

impl From<Discovery> for DiscoveryEvent {
    fn from(discovery: Discovery, peer: PeerId) -> DiscoveryEvent {
        DiscoveryEvent {
            peer,
            hostname: discovery.hostname,
            os: discovery.os,
        }
    }
}

#[derive(Debug)]
pub struct DiscoveryBehaviour {
    events: VecDeque<
        NetworkBehaviourAction<DiscoveryEvent, Handler>,
    >,
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

    pub fn notify_frontend(&mut self) -> Result<(), Box<dyn Error>> {
        let event = PeerEvent::PeersUpdated(self.peers_event());
        Ok(self.sender.try_send(event)?)
    }

    fn dial_peer(&mut self, peer_id: PeerId, addr: Multiaddr, insert_peer: bool) {
        self.events.push_back(NetworkBehaviourAction::Dial {
            opts: DialOpts::peer_id(peer_id).address(addr).build(),
            handler: self.new_handler(),
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

        if let Err(e) = self.notify_frontend() {
            error!("Failed to notify the frontend: {:?}", e);
        }
        Ok(())
    }

    pub fn update_peer(&mut self, peer_id: PeerId, hostname: String, os: OperatingSystem) {
        match self.peers.get_mut(&peer_id) {
            Some(peer) => {
                info!("Updating peer. {:?}", peer_id);
                peer.hostname = hostname;
                peer.os = os;
            }
            None => {
                error!("Peer not found! {:?}", peer_id);
            }
        }

        if let Err(e) = self.notify_frontend() {
            error!("Failed to notify the frontend: {:?}", e);
        }
    }
}

impl NetworkBehaviour for DiscoveryBehaviour {
    type ConnectionHandler = Handler;
    type OutEvent = DiscoveryEvent;

    fn new_handler(&mut self) -> Self::ConnectionHandler {
        let substream_proto = SubstreamProtocol::new(
            Discovery {
                hostname: self.hostname.clone(),
                os: self.os,
            },
            (),
        );
        let outbound_substream_timeout = Duration::from_secs(2);
        Self::ConnectionHandler::new(substream_proto, outbound_substream_timeout)
    }

    fn addresses_of_peer(&mut self, _peer_id: &PeerId) -> Vec<Multiaddr> {
        Vec::new()
    }

    fn inject_connection_established(
        &mut self,
        peer_id: &PeerId,
        c: &ConnectionId,
        endpoint: &ConnectedPoint,
    ) {
        info!("Connection established: {:?}, c: {:?}", endpoint, c);
        match endpoint {
            ConnectedPoint::Dialer {
                address,
                role_override,
            } => {
                if let Some(peer) = self.peers.get_mut(peer_id) {
                    info!("Dialer, updating the address");
                    peer.address = address.clone();
                };
            }
            ConnectedPoint::Listener {
                local_addr,
                send_back_addr,
            } => {
                info!(
                    "Listener: remote: {:?}, local: {:?}",
                    send_back_addr, local_addr
                );
                // Once connection is established, the listener initiates
                // the connection upgrade handshake.

                let event = NetworkBehaviourAction::NotifyHandler {
                    peer_id: peer_id.to_owned(),
                    handler: NotifyHandler::One(*c),
                    event: Discovery {
                        hostname: self.hostname.clone(),
                        os: self.os.clone(),
                    },
                };
                self.events.push_back(event);

                match self.peers.get_mut(peer_id) {
                    Some(peer) => {
                        info!("Listener: peer exists, updating address.");
                        peer.address = send_back_addr.to_owned();
                    }
                    // It may happen that listener didn't have enough time
                    // to discover the dialer through mdns, so here we
                    // make sure to add new peer.
                    None => {
                        info!("Listener: peer not found, adding new one.");
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
            }
        };
    }

    fn inject_connection_closed(&mut self, peer: &PeerId) {
        info!("Peer disconnected: {:?}", peer);
        self.peers.remove(peer);

        if let Err(e) = self.notify_frontend() {
            error!("Failed to notify the frontend: {:?}", e);
        }
    }

    fn inject_event(&mut self, peer: PeerId, _connection: ConnectionId, event: Discovery) {
        let message = DiscoveryEvent {
            peer,
            hostname: event.hostname,
            os: event.os,
        };
        self.events
            .push_back(NetworkBehaviourAction::GenerateEvent(message));
    }

    fn poll(
        &mut self,
        _: &mut Context<'_>,
        _: &mut impl PollParameters,
    ) -> Poll<NetworkBehaviourAction<Self::OutEvent, Self::ConnectionHandler>> {
        if let Some(event) = self.events.pop_front() {
            Poll::Ready(event)
        } else {
            Poll::Pending
        }
    }
}
