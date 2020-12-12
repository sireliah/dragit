use std::error::Error;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use async_std::sync::{Mutex, Receiver, Sender};

use libp2p::core::{connection::ConnectionId, ConnectedPoint, Multiaddr, PeerId};
use libp2p::swarm::{
    DialPeerCondition, NetworkBehaviour, NetworkBehaviourAction, NotifyHandler, OneShotHandler,
    OneShotHandlerConfig, PollParameters, SubstreamProtocol,
};

use super::protocol::{FileToSend, ProtocolEvent, TransferOut, TransferPayload};
use crate::p2p::commands::TransferCommand;
use crate::p2p::peer::PeerEvent;

const TIMEOUT: u64 = 600;

pub struct TransferBehaviour {
    pub events: Vec<NetworkBehaviourAction<TransferOut, TransferPayload>>,
    payloads: Vec<FileToSend>,
    pub sender: Sender<PeerEvent>,
    receiver: Arc<Mutex<Receiver<TransferCommand>>>,
    pub target_path: Option<String>,
}

impl TransferBehaviour {
    pub fn new(
        sender: Sender<PeerEvent>,
        receiver: Arc<Mutex<Receiver<TransferCommand>>>,
        target_path: Option<String>,
    ) -> Self {
        TransferBehaviour {
            events: vec![],
            payloads: vec![],
            sender,
            receiver,
            target_path,
        }
    }

    pub fn push_file(&mut self, file: FileToSend) -> Result<(), Box<dyn Error>> {
        Ok(self.payloads.push(file))
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
            target_path: self.target_path.clone(),
        };
        let handler_config = OneShotHandlerConfig {
            keep_alive_timeout: Duration::from_secs(5),
            outbound_substream_timeout: timeout,
        };
        let proto = SubstreamProtocol::new(tp).with_timeout(timeout);
        Self::ProtocolsHandler::new(proto, handler_config)
    }

    fn addresses_of_peer(&mut self, _peer_id: &PeerId) -> Vec<Multiaddr> {
        Vec::new()
    }

    fn inject_connected(&mut self, _peer: &PeerId) {}

    fn inject_connection_established(
        &mut self,
        peer: &PeerId,
        c: &ConnectionId,
        _endpoint: &ConnectedPoint,
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
    }

    fn inject_dial_failure(&mut self, peer: &PeerId) {
        warn!("Dial failure: {:?}", peer);
    }

    fn inject_disconnected(&mut self, _peer: &PeerId) {}

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
    ) -> Poll<NetworkBehaviourAction<TransferOut, TransferPayload>> {
        for file in self.payloads.iter() {
            return Poll::Ready(NetworkBehaviourAction::DialPeer {
                condition: DialPeerCondition::Disconnected,
                peer_id: file.peer.to_owned(),
            });
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
