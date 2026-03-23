use std::io::{Error, Read};
use std::sync::Arc;
use std::time::Duration;

use async_channel::{bounded, Receiver, Sender};
use hex;
use md5::{Digest, Md5};
use tempfile::{tempdir, TempDir};
use tokio::sync::Mutex;

use libp2p::{identity, noise, tcp, yamux, PeerId, Swarm, SwarmBuilder};

use dragit::p2p::transfer::metadata::HASH_BUFFER_SIZE;
use dragit::p2p::{FileToSend, PeerEvent, TransferBehaviour, TransferCommand};

#[allow(dead_code)]
pub fn hash_contents_sync(mut file: impl Read) -> Result<String, Error> {
    let mut state = Md5::default();
    let mut buffer = [0u8; HASH_BUFFER_SIZE];

    loop {
        match file.read(&mut buffer) {
            Ok(n) if n == 0 || n < HASH_BUFFER_SIZE => {
                state.update(&buffer[..n]);
                break;
            }
            Ok(n) => {
                state.update(&buffer[..n]);
            }
            Err(e) => return Err(e),
        };
    }
    Ok(hex::encode::<Vec<u8>>(state.finalize().to_vec()))
}

pub fn build_swarm() -> (
    PeerId,
    Sender<TransferCommand>,
    Receiver<PeerEvent>,
    Swarm<TransferBehaviour>,
    TempDir,
) {
    let (_, _) = bounded::<FileToSend>(1024 * 24);
    let (command_sender, command_receiver) = bounded::<TransferCommand>(1024 * 24);
    let (peer_sender, peer_receiver) = bounded::<PeerEvent>(1024 * 24);

    let local_keys = identity::Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(local_keys.public());

    let command_receiver = Arc::new(Mutex::new(command_receiver));

    let dir = tempdir().unwrap();
    let target_path = Some(dir.path().to_string_lossy().to_string());

    let peer_sender_clone = peer_sender.clone();
    let command_receiver_clone = Arc::clone(&command_receiver);
    let target_path_clone = target_path.clone();

    let swarm = SwarmBuilder::with_existing_identity(local_keys)
        .with_tokio()
        .with_tcp(
            tcp::Config::default().nodelay(true),
            noise::Config::new,
            yamux::Config::default,
        )
        .unwrap()
        .with_behaviour(move |_key| {
            TransferBehaviour::new(
                peer_sender_clone.clone(),
                command_receiver_clone.clone(),
                target_path_clone.clone(),
            )
        })
        .unwrap()
        .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(Duration::from_secs(60)))
        .build();

    let peer_id = local_peer_id.clone();
    (peer_id, command_sender, peer_receiver, swarm, dir)
}

pub fn setup_logger() {
    let env = env_logger::Env::default().filter_or("LOG_LEVEL", "info");
    env_logger::Builder::from_env(env)
        .is_test(true)
        .try_init()
        .unwrap();
}
