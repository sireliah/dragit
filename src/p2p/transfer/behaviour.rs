use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use async_channel::{Receiver, Sender};
use tokio::sync::Mutex;

use libp2p::core::Multiaddr;
use libp2p::swarm::{
    ConnectionDenied, ConnectionId, FromSwarm, NetworkBehaviour, NotifyHandler, THandler,
    THandlerInEvent, THandlerOutEvent, ToSwarm,
};
use libp2p::PeerId;

use super::protocol::{ProtocolEvent, TransferOut, TransferPayload};
use crate::p2p::commands::TransferCommand;
use crate::p2p::peer::PeerEvent;
use crate::p2p::transfer::file::{FileToSend, Payload};

const TIMEOUT: u64 = 600;

use crate::p2p::discovery::handler::KeepAliveHandler;

type Handler = KeepAliveHandler<TransferPayload, TransferOut, ProtocolEvent>;

pub struct TransferBehaviour {
    pub events: Vec<ToSwarm<TransferPayload, THandlerInEvent<Self>>>,
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
    type ToSwarm = TransferPayload;

    fn handle_established_inbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer_id: PeerId,
        _local_addr: &Multiaddr,
        _remote_addr: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
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
        let proto = libp2p::swarm::SubstreamProtocol::new(tp, ()).with_timeout(timeout);
        Ok(Handler::new(proto, timeout))
    }

    fn handle_established_outbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer_id: PeerId,
        _addr: &Multiaddr,
        _role_override: libp2p::core::Endpoint,
        _port_use: libp2p::swarm::derive_prelude::PortUse,
    ) -> Result<THandler<Self>, ConnectionDenied> {
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
        let proto = libp2p::swarm::SubstreamProtocol::new(tp, ()).with_timeout(timeout);
        Ok(Handler::new(proto, timeout))
    }

    fn on_swarm_event(&mut self, event: FromSwarm) {
        match event {
            FromSwarm::ConnectionEstablished(info) => {
                debug!(
                    "Connection established: {:?}, {:?}, c: {:?}",
                    info.peer_id, info.endpoint, info.connection_id
                )
            }
            FromSwarm::DialFailure(info) => {
                warn!("Dial failure: {:?}, {:?}", info.peer_id, info.error);
            }
            _ => {}
        }
    }

    fn on_connection_handler_event(
        &mut self,
        _peer: PeerId,
        _connection: ConnectionId,
        event: THandlerOutEvent<Self>,
    ) {
        match event {
            ProtocolEvent::Received(data) => {
                info!("Inject event: {}", data);
                self.events.push(ToSwarm::GenerateEvent(data));
            }
            ProtocolEvent::Sent => {}
        };
    }

    fn poll(&mut self, _: &mut Context) -> Poll<ToSwarm<TransferPayload, THandlerInEvent<Self>>> {
        if let Some(file) = self.payloads.pop() {
            let peer_id = file.peer.clone();
            let transfer = TransferOut {
                file,
                sender_queue: self.sender.clone(),
            };

            let event = ToSwarm::NotifyHandler {
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
