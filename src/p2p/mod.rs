use async_std::task;
use futures::{channel::mpsc::Receiver, executor, future, pin_mut, stream::StreamExt};
use libp2p::{
    build_development_transport,
    core::transport::timeout::TransportTimeout,
    identity,
    mdns::{Mdns, MdnsEvent},
    swarm::NetworkBehaviourEventProcess,
    NetworkBehaviour, PeerId, Swarm,
};

use std::{
    error::Error,
    task::{Context, Poll},
    time::Duration,
};

pub mod behaviour;
pub mod handler;
pub mod protocol;

use behaviour::TransferBehaviour;
use protocol::{ProtocolEvent, TransferPayload};

pub use protocol::FileToSend;

#[derive(NetworkBehaviour)]
struct MyBehaviour {
    mdns: Mdns,
    transfer_behaviour: TransferBehaviour,
}

impl NetworkBehaviourEventProcess<MdnsEvent> for MyBehaviour {
    fn inject_event(&mut self, event: MdnsEvent) {
        match event {
            MdnsEvent::Discovered(list) => {
                for (peer, _addr) in list {
                    self.transfer_behaviour.peers.insert(peer);
                }
            }
            MdnsEvent::Expired(list) => {
                for (peer, _addr) in list {
                    println!("Expired: {:?}", peer);
                    self.transfer_behaviour.peers.remove(&peer);
                }
            }
        }
    }
}

impl NetworkBehaviourEventProcess<ProtocolEvent> for MyBehaviour {
    fn inject_event(&mut self, event: ProtocolEvent) {
        match event {
            ProtocolEvent::Received {
                name,
                path,
                hash,
                size_bytes,
            } => println!("Inject: Data: {} {} {} {}", name, path, hash, size_bytes),
            ProtocolEvent::Sent => println!("sent!"),
        }
    }
}

impl NetworkBehaviourEventProcess<TransferPayload> for MyBehaviour {
    fn inject_event(&mut self, event: TransferPayload) {
        println!("TransferPayload event: {:?}", event);
        match event.check_file() {
            Ok(_) => println!("File is correct"),
            Err(e) => println!("{:?}", e),
        }
    }
}

async fn execute_swarm(receiver: Receiver<FileToSend>) {
    let local_keys = identity::Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(local_keys.public());
    println!("I am Peer: {:?}", local_peer_id);

    let mut swarm = {
        let mdns = Mdns::new().unwrap();
        let transfer_behaviour = TransferBehaviour::new();
        let behaviour = MyBehaviour {
            mdns,
            transfer_behaviour,
        };
        let timeout = Duration::from_secs(60);
        let transport = TransportTimeout::with_outgoing_timeout(
            build_development_transport(local_keys.clone()).unwrap(),
            timeout,
        );

        Swarm::new(transport, behaviour, local_peer_id)
    };

    Swarm::listen_on(
        &mut swarm,
        "/ip4/0.0.0.0/tcp/0"
            .parse()
            .expect("Failed to parse address"),
    )
    .expect("Failed to listen");
    let mut listening = false;

    pin_mut!(receiver);
    task::block_on(future::poll_fn(move |context: &mut Context| {
        loop {
            match Receiver::poll_next_unpin(&mut receiver, context) {
                Poll::Ready(Some(event)) => {
                    match swarm.transfer_behaviour.push_file(event) {
                        Ok(_) => {}
                        Err(e) => eprintln!("{:?}", e),
                    };
                }
                Poll::Ready(None) => println!("nothing in queue"),
                Poll::Pending => break,
            };
        }

        loop {
            match swarm.poll_next_unpin(context) {
                Poll::Ready(Some(event)) => println!("Some event main: {:?}", event),
                Poll::Ready(None) => {
                    return {
                        println!("Ready");
                        Poll::Ready("aaa")
                    }
                }
                Poll::Pending => {
                    if !listening {
                        for addr in Swarm::listeners(&swarm) {
                            println!("Listening on {:?}", addr);
                            listening = true;
                        }
                    }

                    break;
                }
            }
        }
        Poll::Pending
    }));
}

pub fn run_server(receiver: Receiver<FileToSend>) -> Result<(), Box<dyn Error>> {
    let future = execute_swarm(receiver);
    executor::block_on(future);
    Ok(())
}
