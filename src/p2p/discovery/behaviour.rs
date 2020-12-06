use std::{
    collections::{HashSet, VecDeque},
    fmt,
    task::{Context, Poll},
};

use libp2p::core::{connection::ConnectionId, Multiaddr, PeerId};
use libp2p::swarm::{
    protocols_handler::SubstreamProtocol, DialPeerCondition, NetworkBehaviour,
    NetworkBehaviourAction, NotifyHandler, OneShotHandler, PollParameters, ProtocolsHandler,
};

use crate::p2p::discovery::protocol::{Discovery, DiscoveryEvent};

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
    peers: HashSet<PeerId>,
}

impl DiscoveryBehaviour {
    pub fn new() -> Self {
        DiscoveryBehaviour {
            events: VecDeque::new(),
            peers: HashSet::new(),
        }
    }

    pub fn add_peer(&mut self, peer_id: PeerId) {
        self.events.push_back(NetworkBehaviourAction::DialPeer {
            condition: DialPeerCondition::Disconnected,
            peer_id: peer_id.clone(),
        });

        let event = NetworkBehaviourAction::NotifyHandler {
            peer_id,
            handler: NotifyHandler::Any,
            event: Discovery::default(),
        };
        self.events.push_back(event);
    }
}

impl NetworkBehaviour for DiscoveryBehaviour {
    type ProtocolsHandler = OneShotHandler<Discovery, Discovery, InnerMessage>;
    type OutEvent = DiscoveryEvent;

    fn new_handler(&mut self) -> Self::ProtocolsHandler {
        Self::ProtocolsHandler::new(
            SubstreamProtocol::new(Discovery::default()),
            Default::default(),
        )
    }

    fn addresses_of_peer(&mut self, _peer_id: &PeerId) -> Vec<Multiaddr> {
        Vec::new()
    }

    fn inject_connected(&mut self, _: &PeerId) {}

    fn inject_disconnected(&mut self, _: &PeerId) {}

    fn inject_event(&mut self, peer: PeerId, _connection: ConnectionId, event: InnerMessage) {
        // info!("Inject discovery event: {}", event);
        match event {
            InnerMessage::Received(ev) => {
                let message = DiscoveryEvent {
                    peer,
                    result: Ok(ev.hostname),
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
