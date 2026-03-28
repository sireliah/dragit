use libp2p::swarm::handler::{
    ConnectionEvent, ConnectionHandler, ConnectionHandlerEvent, FullyNegotiatedInbound,
    FullyNegotiatedOutbound, InboundUpgradeSend, OutboundUpgradeSend, SubstreamProtocol,
};
use std::fmt::Debug;

use smallvec::SmallVec;
use std::{error, task::Context, task::Poll, time::Duration};

/// Shamelessly copied OneShotHandler that keeps the connections open
pub struct KeepAliveHandler<TInbound, TOutbound, TEvent>
where
    TOutbound: OutboundUpgradeSend + Debug,
{
    /// The upgrade for inbound substreams.
    listen_protocol: SubstreamProtocol<TInbound, ()>,
    /// Queue of events to produce in `poll()`.
    events_out: SmallVec<[TEvent; 4]>,
    /// Queue of outbound substreams to open.
    dial_queue: SmallVec<[TOutbound; 4]>,
    /// Current number of concurrent outbound substreams being opened.
    dial_negotiated: u32,
    /// Maximum number of concurrent outbound substreams being opened. Value is never modified.
    max_dial_negotiated: u32,
}

impl<TInbound, TOutbound, TEvent> KeepAliveHandler<TInbound, TOutbound, TEvent>
where
    TOutbound: OutboundUpgradeSend + Debug,
{
    pub fn new(listen_protocol: SubstreamProtocol<TInbound, ()>) -> Self {
        KeepAliveHandler {
            listen_protocol,
            events_out: SmallVec::new(),
            dial_queue: SmallVec::new(),
            dial_negotiated: 0,
            max_dial_negotiated: 8,
        }
    }

    /// Returns the number of pending requests.
    pub fn pending_requests(&self) -> u32 {
        self.dial_negotiated + self.dial_queue.len() as u32
    }

    /// Returns a reference to the listen protocol configuration.
    ///
    /// > **Note**: If you modify the protocol, modifications will only applies to future inbound
    /// >           substreams, not the ones already being negotiated.
    pub fn listen_protocol_ref(&self) -> &SubstreamProtocol<TInbound, ()> {
        &self.listen_protocol
    }

    /// Returns a mutable reference to the listen protocol configuration.
    ///
    /// > **Note**: If you modify the protocol, modifications will only applies to future inbound
    /// >           substreams, not the ones already being negotiated.
    pub fn listen_protocol_mut(&mut self) -> &mut SubstreamProtocol<TInbound, ()> {
        &mut self.listen_protocol
    }

    /// Opens an outbound substream with `upgrade`.
    pub fn send_request(&mut self, upgrade: TOutbound) {
        self.dial_queue.push(upgrade);
    }
}

impl<TInbound, TOutbound, TEvent> Default for KeepAliveHandler<TInbound, TOutbound, TEvent>
where
    TOutbound: OutboundUpgradeSend + Debug,
    TInbound: InboundUpgradeSend + Default,
{
    fn default() -> Self {
        KeepAliveHandler::new(SubstreamProtocol::new(Default::default(), ()))
    }
}

impl<TInbound, TOutbound, TEvent> ConnectionHandler
    for KeepAliveHandler<TInbound, TOutbound, TEvent>
where
    TInbound: InboundUpgradeSend + Send + 'static,
    TOutbound: OutboundUpgradeSend + Debug + Send + 'static,
    TInbound::Output: Into<TEvent>,
    TOutbound::Output: Into<TEvent>,
    TOutbound::Error: error::Error + Send + 'static,
    SubstreamProtocol<TInbound, ()>: Clone,
    TEvent: Debug + Send + 'static,
{
    type FromBehaviour = TOutbound;
    type ToBehaviour = TEvent;
    type InboundProtocol = TInbound;
    type OutboundProtocol = TOutbound;
    type OutboundOpenInfo = ();
    type InboundOpenInfo = ();

    fn listen_protocol(&self) -> SubstreamProtocol<Self::InboundProtocol, Self::InboundOpenInfo> {
        self.listen_protocol.clone()
    }

    fn on_behaviour_event(&mut self, event: Self::FromBehaviour) {
        self.send_request(event);
    }

    fn on_connection_event(
        &mut self,
        event: ConnectionEvent<
            Self::InboundProtocol,
            Self::OutboundProtocol,
            Self::InboundOpenInfo,
            Self::OutboundOpenInfo,
        >,
    ) {
        match event {
            ConnectionEvent::FullyNegotiatedInbound(FullyNegotiatedInbound {
                protocol: out,
                ..
            }) => {
                self.events_out.push(out.into());
            }
            ConnectionEvent::FullyNegotiatedOutbound(FullyNegotiatedOutbound {
                protocol: out,
                ..
            }) => {
                self.dial_negotiated -= 1;
                self.events_out.push(out.into());
            }
            ConnectionEvent::DialUpgradeError(e) => {
                warn!("Dial upgrade error in KeepAliveHandler: {:?}", e.error);
            }
            _ => {}
        }
    }

    fn connection_keep_alive(&self) -> bool {
        true
    }

    fn poll(
        &mut self,
        _: &mut Context<'_>,
    ) -> Poll<
        ConnectionHandlerEvent<Self::OutboundProtocol, Self::OutboundOpenInfo, Self::ToBehaviour>,
    > {
        if !self.events_out.is_empty() {
            return Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(
                self.events_out.remove(0),
            ));
        } else {
            self.events_out.shrink_to_fit();
        }

        if !self.dial_queue.is_empty() {
            if self.dial_negotiated < self.max_dial_negotiated {
                self.dial_negotiated += 1;
                let upgrade = self.dial_queue.remove(0);
                return Poll::Ready(ConnectionHandlerEvent::OutboundSubstreamRequest {
                    protocol: SubstreamProtocol::new(upgrade, ())
                        .with_timeout(Duration::from_secs(30 * 365 * 24 * 60 * 60)),
                });
            }
        } else {
            self.dial_queue.shrink_to_fit();
        }

        Poll::Pending
    }
}
