use libp2p::swarm::handler::{
    ConnectionHandler, ConnectionHandlerEvent, ConnectionHandlerUpgrErr, InboundUpgradeSend,
    KeepAlive, OutboundUpgradeSend, SubstreamProtocol,
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
    /// If `Some`, something bad happened and we should shut down the handler with an error.
    pending_error: Option<ConnectionHandlerUpgrErr<<TOutbound as OutboundUpgradeSend>::Error>>,
    /// Queue of events to produce in `poll()`.
    events_out: SmallVec<[TEvent; 4]>,
    /// Queue of outbound substreams to open.
    dial_queue: SmallVec<[TOutbound; 4]>,
    /// Current number of concurrent outbound substreams being opened.
    dial_negotiated: u32,
    /// Maximum number of concurrent outbound substreams being opened. Value is never modified.
    max_dial_negotiated: u32,
    outbound_substream_timeout: Duration,
}

impl<TInbound, TOutbound, TEvent> KeepAliveHandler<TInbound, TOutbound, TEvent>
where
    TOutbound: OutboundUpgradeSend + Debug,
{
    pub fn new(
        listen_protocol: SubstreamProtocol<TInbound, ()>,
        outbound_substream_timeout: Duration,
    ) -> Self {
        KeepAliveHandler {
            listen_protocol,
            pending_error: None,
            events_out: SmallVec::new(),
            dial_queue: SmallVec::new(),
            dial_negotiated: 0,
            max_dial_negotiated: 8,
            outbound_substream_timeout,
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
        KeepAliveHandler::new(
            SubstreamProtocol::new(Default::default(), ()),
            Duration::from_secs(10),
        )
    }
}

impl<TInbound, TOutbound, TEvent> ConnectionHandler
    for KeepAliveHandler<TInbound, TOutbound, TEvent>
where
    TInbound: InboundUpgradeSend + Send + 'static,
    TOutbound: OutboundUpgradeSend + Debug,
    TInbound::Output: Into<TEvent>,
    TOutbound::Output: Into<TEvent>,
    TOutbound::Error: error::Error + Send + 'static,
    SubstreamProtocol<TInbound, ()>: Clone,
    TEvent: Debug + Send + 'static,
{
    type InEvent = TOutbound;
    type OutEvent = TEvent;
    type Error = ConnectionHandlerUpgrErr<<Self::OutboundProtocol as OutboundUpgradeSend>::Error>;
    type InboundProtocol = TInbound;
    type OutboundProtocol = TOutbound;
    type OutboundOpenInfo = ();
    type InboundOpenInfo = ();

    fn listen_protocol(&self) -> SubstreamProtocol<Self::InboundProtocol, Self::InboundOpenInfo> {
        self.listen_protocol.clone()
    }

    fn inject_fully_negotiated_inbound(
        &mut self,
        out: <Self::InboundProtocol as InboundUpgradeSend>::Output,
        (): Self::InboundOpenInfo,
    ) {
        self.events_out.push(out.into());
    }

    fn inject_fully_negotiated_outbound(
        &mut self,
        out: <Self::OutboundProtocol as OutboundUpgradeSend>::Output,
        _: Self::OutboundOpenInfo,
    ) {
        self.dial_negotiated -= 1;
        self.events_out.push(out.into());
    }

    fn inject_event(&mut self, event: Self::InEvent) {
        self.send_request(event);
    }

    fn inject_dial_upgrade_error(
        &mut self,
        _info: Self::OutboundOpenInfo,
        error: ConnectionHandlerUpgrErr<<Self::OutboundProtocol as OutboundUpgradeSend>::Error>,
    ) {
        if self.pending_error.is_none() {
            self.pending_error = Some(error);
        }
    }

    fn connection_keep_alive(&self) -> KeepAlive {
        KeepAlive::Yes
    }

    fn poll(
        &mut self,
        _: &mut Context<'_>,
    ) -> Poll<
        ConnectionHandlerEvent<
            Self::OutboundProtocol,
            Self::OutboundOpenInfo,
            Self::OutEvent,
            Self::Error,
        >,
    > {
        if let Some(err) = self.pending_error.take() {
            return Poll::Ready(ConnectionHandlerEvent::Close(err));
        }

        if !self.events_out.is_empty() {
            return Poll::Ready(ConnectionHandlerEvent::Custom(self.events_out.remove(0)));
        } else {
            self.events_out.shrink_to_fit();
        }

        if !self.dial_queue.is_empty() {
            if self.dial_negotiated < self.max_dial_negotiated {
                self.dial_negotiated += 1;
                let upgrade = self.dial_queue.remove(0);
                return Poll::Ready(ConnectionHandlerEvent::OutboundSubstreamRequest {
                    protocol: SubstreamProtocol::new(upgrade, ())
                        .with_timeout(self.outbound_substream_timeout),
                });
            }
        } else {
            self.dial_queue.shrink_to_fit();
        }

        Poll::Pending
    }
}
