use std::{error::Error, sync::Arc, thread::sleep, time::Duration};

use async_channel::{Receiver, Sender};
use tokio::sync::Mutex;

use futures::{select, stream::StreamExt, FutureExt};
use libp2p::{mdns, noise, swarm::SwarmEvent, tcp, yamux, PeerId, Swarm, SwarmBuilder};

pub mod commands;
pub mod discovery;
pub mod peer;
pub mod transfer;
pub mod util;

use crate::user_data::UserConfig;
pub use commands::TransferCommand;
pub use discovery::{DiscoveryBehaviour, DiscoveryEvent};
pub use peer::{CurrentPeers, OperatingSystem, Peer, PeerEvent, TransferType};

pub use transfer::{FileToSend, Payload, TransferBehaviour, TransferOut, TransferPayload};

#[derive(libp2p::swarm::NetworkBehaviour)]
#[behaviour(to_swarm = "MyBehaviourEvent")]
pub struct MyBehaviour {
    pub mdns: mdns::tokio::Behaviour,
    pub discovery: DiscoveryBehaviour,
    pub transfer_behaviour: TransferBehaviour,
}

#[derive(Debug)]
pub enum MyBehaviourEvent {
    Mdns(mdns::Event),
    Discovery(DiscoveryEvent),
    Transfer(TransferPayload),
    TransferOut(TransferOut),
}

impl From<mdns::Event> for MyBehaviourEvent {
    fn from(e: mdns::Event) -> Self {
        MyBehaviourEvent::Mdns(e)
    }
}

impl From<DiscoveryEvent> for MyBehaviourEvent {
    fn from(e: DiscoveryEvent) -> Self {
        MyBehaviourEvent::Discovery(e)
    }
}

impl From<TransferPayload> for MyBehaviourEvent {
    fn from(e: TransferPayload) -> Self {
        MyBehaviourEvent::Transfer(e)
    }
}

impl From<TransferOut> for MyBehaviourEvent {
    fn from(e: TransferOut) -> Self {
        MyBehaviourEvent::TransferOut(e)
    }
}

async fn execute_swarm(
    sender: Sender<PeerEvent>,
    receiver: Receiver<FileToSend>,
    command_receiver: Receiver<TransferCommand>,
) -> Result<(), Box<dyn Error>> {
    let config = UserConfig::new()?;
    let local_keys = config.get_or_create_keypair()?;
    let local_peer_id = PeerId::from(local_keys.public());
    info!("I am Peer: {:?}", local_peer_id);

    let command_rec = Arc::new(Mutex::new(command_receiver));
    let command_receiver_c = Arc::clone(&command_rec);

    let sender_clone = sender.clone();

    let mut swarm = SwarmBuilder::with_existing_identity(local_keys)
        .with_tokio()
        .with_tcp(
            tcp::Config::default().nodelay(true),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_behaviour(move |key| {
            let mdns =
                mdns::tokio::Behaviour::new(mdns::Config::default(), key.public().to_peer_id())
                    .expect("Failed to create mdns behaviour");

            let transfer_behaviour =
                TransferBehaviour::new(sender_clone.clone(), command_receiver_c.clone(), None);
            let discovery = DiscoveryBehaviour::new(sender_clone.clone());

            MyBehaviour {
                mdns,
                discovery,
                transfer_behaviour,
            }
        })?
        .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(Duration::from_secs(60)))
        .build();

    let port = config.get_port();

    let address = format!("/ip4/0.0.0.0/tcp/{}", port);
    swarm.listen_on(address.parse()?)?;

    loop {
        select! {
            received = receiver.recv().fuse() => {
                let behaviour = swarm.behaviour_mut();
                match received {
                    Ok(file_to_send) => {
                        behaviour.transfer_behaviour.push_file(file_to_send);
                    },
                    Err(e) => error!("Receiver error: {:?}", e),
                }
            },
            swarm_event = swarm.select_next_some() => {
                match swarm_event {
                    SwarmEvent::Behaviour(MyBehaviourEvent::Mdns(event)) => {
                        handle_mdns_event(&mut swarm, event);
                    }
                    SwarmEvent::Behaviour(MyBehaviourEvent::Discovery(event)) => {
                        info!("Discovered: {}", event);
                        swarm.behaviour_mut().discovery.update_peer(
                            event.peer,
                            event.hostname,
                            event.os,
                        );
                    }
                    SwarmEvent::Behaviour(MyBehaviourEvent::Transfer(event)) => {
                        info!("Transfer event: {}", event);
                        // Hash verified in-flight during transfer; no second disk read needed.
                        if let Err(e) = event.cleanup() {
                            error!("Could not clean up file: {:?}", e);
                        };
                        if let Err(e) = event
                            .sender_queue
                            .try_send(PeerEvent::FileCorrect(event.name, event.payload))
                        {
                            error!("{:?}", e);
                        }
                    }
                    SwarmEvent::Behaviour(MyBehaviourEvent::TransferOut(event)) => {
                        info!("TransferOut event: {:?}", event);
                    }
                    other => {
                        info!("Swarm event: {:?}", other);
                    }
                }
            }
        }
    }
}

fn handle_mdns_event(swarm: &mut Swarm<MyBehaviour>, event: mdns::Event) {
    match event {
        mdns::Event::Discovered(list) => {
            for (peer_id, addr) in list {
                info!("Discovered peer_id: {}", peer_id);
                swarm.behaviour_mut().discovery.add_peer(peer_id, addr);
            }
        }
        mdns::Event::Expired(list) => {
            for (peer_id, _addr) in list {
                info!("Address expired: {:?}", peer_id);
                match swarm.behaviour_mut().discovery.remove_peer(&peer_id) {
                    Ok(_) => (),
                    Err(e) => error!("Removing peer failed: {:?}", e),
                }
            }
        }
    }
}

pub fn run_server(
    sender: Sender<PeerEvent>,
    file_receiver: Receiver<FileToSend>,
    command_receiver: Receiver<TransferCommand>,
) -> Result<(), Box<dyn Error>> {
    loop {
        match util::check_network_interfaces() {
            Ok(_) => break,
            Err(e) => {
                let _ = sender.try_send(PeerEvent::Error(e.to_string()))?;
                sleep(Duration::from_secs(5));
                continue;
            }
        };
    }

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(execute_swarm(sender, file_receiver, command_receiver))?;
    Ok(())
}
