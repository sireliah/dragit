use std::{
    collections::{HashMap, VecDeque},
    error::Error,
    task::{Context, Poll},
    time::Duration,
};

use async_channel::Sender;
use hostname;
use libp2p::core::Multiaddr;
use libp2p::swarm::{
    dial_opts::{DialOpts, PeerCondition},
    ConnectionDenied, ConnectionId, FromSwarm, NetworkBehaviour, THandler, THandlerInEvent,
    THandlerOutEvent, ToSwarm,
};
use libp2p::PeerId;

use crate::p2p::discovery::handler::KeepAliveHandler;
use crate::p2p::discovery::protocol::{Discovery, DiscoveryEvent};
use crate::p2p::peer::{CurrentPeers, OperatingSystem, Peer, PeerEvent};

type Handler = KeepAliveHandler<Discovery, Discovery, Discovery>;

pub struct DiscoveryBehaviour {
    events: VecDeque<ToSwarm<DiscoveryEvent, THandlerInEvent<Self>>>,
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
        self.events.push_back(ToSwarm::Dial {
            opts: DialOpts::peer_id(peer_id)
                .addresses(vec![addr.clone()])
                .condition(PeerCondition::NotDialing)
                .build(),
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

    /// Queue a NotifyHandler event that triggers the discovery substream exchange
    /// on the given connection. Both inbound and outbound connections call this so
    /// that discovery succeeds on whichever connection survives simultaneous-dial
    /// resolution. The handler routes the event into its dial_queue, which opens an
    /// outbound substream and runs upgrade_outbound (writes first, then reads).
    /// The remote's handler accepts it as an inbound substream and runs
    /// upgrade_inbound (reads first, then writes). The two sides never block each
    /// other waiting for the other to go first.
    fn queue_discovery_notify(&mut self, peer_id: PeerId, connection_id: ConnectionId) {
        let event = ToSwarm::NotifyHandler {
            peer_id,
            handler: libp2p::swarm::NotifyHandler::One(connection_id),
            event: Discovery {
                hostname: self.hostname.clone(),
                os: self.os,
            },
        };
        self.events.push_back(event);
    }
}

impl NetworkBehaviour for DiscoveryBehaviour {
    type ConnectionHandler = Handler;
    type ToSwarm = DiscoveryEvent;

    fn handle_established_inbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer_id: PeerId,
        _local_addr: &Multiaddr,
        remote_addr: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        info!(
            "Inbound connection established: peer={:?}, connection={:?}",
            peer_id, connection_id
        );

        match self.peers.get_mut(&peer_id) {
            Some(peer) => {
                info!("Listener: peer exists, updating address.");
                peer.address = remote_addr.clone();
            }
            None => {
                info!("Listener: peer not found, adding new one.");
                let peer = Peer {
                    name: peer_id.to_base58(),
                    peer_id,
                    address: remote_addr.clone(),
                    hostname: "Not known yet".to_string(),
                    os: OperatingSystem::Unknown,
                };
                self.peers.insert(peer_id, peer);
            }
        }

        // The listener side does NOT trigger an outbound substream. It waits
        // passively for the dialer to open one, which will be handled by
        // upgrade_inbound (reads the dialer's data, then writes back ours).

        let substream_proto = libp2p::swarm::SubstreamProtocol::new(
            Discovery {
                hostname: self.hostname.clone(),
                os: self.os,
            },
            (),
        );
        let outbound_substream_timeout = Duration::from_secs(2);
        Ok(Handler::new(substream_proto, outbound_substream_timeout))
    }

    fn handle_established_outbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer_id: PeerId,
        addr: &Multiaddr,
        _role_override: libp2p::core::Endpoint,
        _port_use: libp2p::swarm::derive_prelude::PortUse,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        info!("Outbound connection established: peer={:?}", peer_id);

        if let Some(peer) = self.peers.get_mut(&peer_id) {
            info!("Dialer, updating the address");
            peer.address = addr.clone();
        }

        // Trigger discovery on the outbound connection.
        self.queue_discovery_notify(peer_id, connection_id);

        let substream_proto = libp2p::swarm::SubstreamProtocol::new(
            Discovery {
                hostname: self.hostname.clone(),
                os: self.os,
            },
            (),
        );
        let outbound_substream_timeout = Duration::from_secs(2);
        Ok(Handler::new(substream_proto, outbound_substream_timeout))
    }

    fn on_swarm_event(&mut self, event: FromSwarm) {
        match event {
            FromSwarm::ConnectionClosed(info) => {
                info!("Peer disconnected: {:?}", info.peer_id);
                self.peers.remove(&info.peer_id);

                if let Err(e) = self.notify_frontend() {
                    error!("Failed to notify the frontend: {:?}", e);
                }
            }
            FromSwarm::ConnectionEstablished(info) => {
                info!(
                    "Connection established event: peer={:?}, endpoint={:?}",
                    info.peer_id, info.endpoint
                );
                let _ = info;
            }
            _ => {}
        }
    }

    fn on_connection_handler_event(
        &mut self,
        peer: PeerId,
        _connection: ConnectionId,
        event: THandlerOutEvent<Self>,
    ) {
        let message = DiscoveryEvent {
            peer,
            hostname: event.hostname,
            os: event.os,
        };
        self.events.push_back(ToSwarm::GenerateEvent(message));
    }

    fn poll(&mut self, _: &mut Context<'_>) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        if let Some(event) = self.events.pop_front() {
            Poll::Ready(event)
        } else {
            Poll::Pending
        }
    }
}
