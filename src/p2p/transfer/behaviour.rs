use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use async_std::channel::{Receiver, Sender};
use async_std::sync::Mutex;

use libp2p::core::{connection::ConnectionId, ConnectedPoint, Multiaddr, PeerId};
use libp2p::swarm::{
    DialError, NetworkBehaviour, NetworkBehaviourAction, NotifyHandler, OneShotHandler,
    OneShotHandlerConfig, PollParameters, SubstreamProtocol,
};

use super::protocol::{ProtocolEvent, TransferOut, TransferPayload};
use crate::p2p::commands::TransferCommand;
use crate::p2p::peer::PeerEvent;
use crate::p2p::transfer::file::{FileToSend, Payload};

const TIMEOUT: u64 = 600;

type Handler = OneShotHandler<TransferPayload, TransferOut, ProtocolEvent>;

pub struct TransferBehaviour {
    pub events: Vec<NetworkBehaviourAction<TransferPayload, Handler>>,
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

    pub fn push_file(&mut self, file: FileToSend) {
        self.payloads.push(file)
    }
}

impl NetworkBehaviour for TransferBehaviour {
    type ConnectionHandler = Handler;
    type OutEvent = TransferPayload;

    fn new_handler(&mut self) -> Self::ConnectionHandler {
        let timeout = Duration::from_secs(TIMEOUT);
        let tp = TransferPayload {
            name: "default".to_string(),
            hash: "".to_string(),
            payload: Payload::File(".".to_string()),
            size_bytes: 0,
            sender_queue: self.sender.clone(),
            receiver: Arc::clone(&self.receiver),
            target_path: self.target_path.clone(),
        };
        let handler_config = OneShotHandlerConfig {
            keep_alive_timeout: Duration::from_secs(5),
            outbound_substream_timeout: timeout,
            // Default from the library
            max_dial_negotiated: 8,
        };
        let proto = SubstreamProtocol::new(tp, ()).with_timeout(timeout);
        Self::ConnectionHandler::new(proto, handler_config)
    }

    fn addresses_of_peer(&mut self, _peer_id: &PeerId) -> Vec<Multiaddr> {
        Vec::new()
    }

    fn inject_connection_established(
        &mut self,
        peer_id: &PeerId,
        c: &ConnectionId,
        endpoint: &ConnectedPoint,
        _failed_addresses: Option<&Vec<Multiaddr>>,
        _other_established: usize,
    ) {
        debug!(
            "Connection established: {:?}, {:?}, c: {:?}",
            peer_id, endpoint, c
        )
    }

    fn inject_dial_failure(
        &mut self,
        peer_id: Option<PeerId>,
        _handler: Self::ConnectionHandler,
        error: &DialError,
    ) {
        warn!("Dial failure: {:?}, {}", peer_id, error);
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
    ) -> Poll<NetworkBehaviourAction<TransferPayload, Handler>> {
        if let Some(file) = self.payloads.pop() {
            let peer_id = file.peer.clone();
            let transfer = TransferOut {
                file,
                sender_queue: self.sender.clone(),
            };

            let event = NetworkBehaviourAction::NotifyHandler {
                // TODO: Notify particular handler, not Any
                handler: NotifyHandler::Any,
                peer_id,
                event: transfer,
            };
            self.events.push(event);
        }

        match self.events.pop() {
            Some(event) => Poll::Ready(event),
            None => Poll::Pending,
        }
    }
}
