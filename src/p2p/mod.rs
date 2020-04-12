use async_std::task;
use futures::{
    channel::mpsc::{Receiver, Sender},
    executor, future, pin_mut,
    stream::StreamExt,
};
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
pub mod peer;
pub mod protocol;
pub mod util;

use behaviour::TransferBehaviour;
use protocol::{TransferOut, TransferPayload};

pub use peer::Peer;
pub use protocol::FileToSend;

#[derive(NetworkBehaviour)]
pub struct MyBehaviour {
    pub mdns: Mdns,
    pub transfer_behaviour: TransferBehaviour,
}

impl NetworkBehaviourEventProcess<MdnsEvent> for MyBehaviour {
    fn inject_event(&mut self, event: MdnsEvent) {
        match event {
            MdnsEvent::Discovered(list) => {
                for (peer_id, _addr) in list {
                    match self.transfer_behaviour.add_peer(peer_id) {
                        Ok(_) => (),
                        Err(e) => eprintln!("{:?}", e),
                    };
                }
            }
            MdnsEvent::Expired(list) => {
                for (peer_id, _addr) in list {
                    println!("Expired: {:?}", peer_id);
                    match self.transfer_behaviour.remove_peer(&peer_id) {
                        Ok(_) => (),
                        Err(e) => eprintln!("{:?}", e),
                    }
                }
            }
        }
    }
}

impl NetworkBehaviourEventProcess<TransferPayload> for MyBehaviour {
    fn inject_event(&mut self, event: TransferPayload) {
        println!("TransferPayload event: {:?}", event);
        match event.check_file() {
            Ok(_) => println!("File is correct"),
            Err(e) => println!("Not correct: {:?}", e),
        }
    }
}

impl NetworkBehaviourEventProcess<TransferOut> for MyBehaviour {
    fn inject_event(&mut self, event: TransferOut) {
        println!("TransferOut event: {:?}", event);
    }
}

async fn execute_swarm(sender: Sender<Vec<Peer>>, receiver: Receiver<FileToSend>) {
    let local_keys = identity::Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(local_keys.public());
    println!("\nI am Peer: {:?} \n\n", local_peer_id);

    let mut swarm = {
        let mdns = Mdns::new().unwrap();
        let transfer_behaviour = TransferBehaviour::new(sender);
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

pub fn run_server(
    sender: Sender<Vec<Peer>>,
    receiver: Receiver<FileToSend>,
) -> Result<(), Box<dyn Error>> {
    let future = execute_swarm(sender, receiver);
    executor::block_on(future);
    Ok(())
}
