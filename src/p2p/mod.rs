use std::{error::Error, sync::Arc, thread::sleep, time::Duration};

use async_std::channel::{Receiver, Sender};
use async_std::sync::Mutex;

use futures::{executor, select, stream::StreamExt, FutureExt};
use libp2p::{
    core::transport::Transport,
    core::upgrade,
    identity,
    mdns::{Mdns, MdnsConfig, MdnsEvent},
    mplex, noise,
    swarm::NetworkBehaviourEventProcess,
    tcp::TcpConfig,
    NetworkBehaviour, PeerId, Swarm,
};

pub mod commands;
pub mod discovery;
pub mod peer;
pub mod transfer;
pub mod util;

use crate::user_data::UserConfig;
pub use commands::TransferCommand;
pub use discovery::{DiscoveryBehaviour, DiscoveryEvent};
pub use peer::{CurrentPeers, OperatingSystem, Peer, PeerEvent, TransferType};
pub use transfer::metadata::hash_contents;
pub use transfer::{FileToSend, Payload, TransferBehaviour, TransferOut, TransferPayload};

#[derive(NetworkBehaviour)]
#[behaviour(event_process = true)]
pub struct MyBehaviour {
    pub mdns: Mdns,
    pub discovery: DiscoveryBehaviour,
    pub transfer_behaviour: TransferBehaviour,
}

impl NetworkBehaviourEventProcess<MdnsEvent> for MyBehaviour {
    fn inject_event(&mut self, mut event: MdnsEvent) {
        match event {
            MdnsEvent::Discovered(ref mut list) => {
                if let Some((peer_id, addr)) = list.next() {
                    info!("Discovered peer_id: {}", peer_id);
                    self.discovery.add_peer(peer_id.clone(), addr);
                }
            }
            MdnsEvent::Expired(list) => {
                for (peer_id, _addr) in list {
                    info!("Address expired: {:?}", peer_id);
                    match self.discovery.remove_peer(&peer_id) {
                        Ok(_) => (),
                        Err(e) => error!("Removing peer failed: {:?}", e),
                    }
                }
            }
        }
    }
}

impl NetworkBehaviourEventProcess<DiscoveryEvent> for MyBehaviour {
    fn inject_event(&mut self, event: DiscoveryEvent) {
        info!("Discovered: {}", event);
        self.discovery
            .update_peer(event.peer, event.hostname, event.os);
    }
}

impl NetworkBehaviourEventProcess<TransferPayload> for MyBehaviour {
    fn inject_event(&mut self, event: TransferPayload) {
        info!("Injected {}", event);
        match event.check_file() {
            Ok(_) => {
                info!("File correct");
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
            Err(e) => {
                warn!("File not correct: {:?}", e);
                if let Err(e) = event.sender_queue.try_send(PeerEvent::FileIncorrect) {
                    error!("{:?}", e);
                }
                if let Err(e) = event.cleanup() {
                    error!("Could not clean up file: {:?}", e);
                };
            }
        }
    }
}

impl NetworkBehaviourEventProcess<TransferOut> for MyBehaviour {
    fn inject_event(&mut self, event: TransferOut) {
        info!("TransferOut event: {:?}", event);
    }
}

async fn execute_swarm(
    sender: Sender<PeerEvent>,
    receiver: Receiver<FileToSend>,
    command_receiver: Receiver<TransferCommand>,
) -> Result<(), Box<dyn Error>> {
    let local_keys = identity::Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(local_keys.public());
    info!("I am Peer: {:?}", local_peer_id);

    let command_rec = Arc::new(Mutex::new(command_receiver));
    let command_receiver_c = Arc::clone(&command_rec);

    let mut swarm = {
        let transfer_behaviour = TransferBehaviour::new(sender.clone(), command_receiver_c, None);
        let discovery = DiscoveryBehaviour::new(sender);
        let mdns = Mdns::new(MdnsConfig::default()).await?;
        let behaviour = MyBehaviour {
            mdns,
            discovery,
            transfer_behaviour,
        };
        let timeout = Duration::from_secs(60);
        let transport = TcpConfig::new().nodelay(true);
        let mut mplex_config = mplex::MplexConfig::new();

        // TODO: test different Mplex frame sizes
        let mp = mplex_config
            .set_max_buffer_size(40960)
            .set_split_send_size(1024 * 512);

        let noise_keys = noise::Keypair::<noise::X25519Spec>::new().into_authentic(&local_keys)?;

        let noise = noise::NoiseConfig::xx(noise_keys).into_authenticated();

        let transport = transport
            .upgrade(upgrade::Version::V1)
            .authenticate(noise)
            .multiplex(mp.clone())
            .timeout(timeout)
            .boxed();
        Swarm::new(transport, behaviour, local_peer_id)
    };

    let config = UserConfig::new()?;
    let port = config.get_port();

    let address = format!("/ip4/0.0.0.0/tcp/{}", port);
    Swarm::listen_on(&mut swarm, address.parse()?)?;

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
                info!("Swarm event: {:?}", swarm_event);
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

    let future = execute_swarm(sender, file_receiver, command_receiver);
    executor::block_on(future)?;
    Ok(())
}
