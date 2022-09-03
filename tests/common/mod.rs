use std::sync::Arc;
use std::time::Duration;

use async_std::channel::{bounded, Receiver, Sender};
use async_std::sync::Mutex;

use libp2p::{
    core::transport::Transport, core::upgrade, identity, mplex, noise, tcp, PeerId, Swarm,
};

use dragit::p2p::{FileToSend, PeerEvent, TransferBehaviour, TransferCommand};

pub fn build_swarm() -> (
    PeerId,
    Sender<TransferCommand>,
    Receiver<PeerEvent>,
    Swarm<TransferBehaviour>,
) {
    let (_, _) = bounded::<FileToSend>(1024 * 24);
    let (command_sender, command_receiver) = bounded::<TransferCommand>(1024 * 24);
    let (peer_sender, peer_receiver) = bounded::<PeerEvent>(1024 * 24);

    let local_keys = identity::Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(local_keys.public());

    let command_receiver = Arc::new(Mutex::new(command_receiver));

    let transfer_behaviour = TransferBehaviour::new(
        peer_sender.clone(),
        command_receiver,
        Some("/tmp/".to_string()),
    );

    let timeout = Duration::from_secs(60);
    let transport = tcp::TcpConfig::new().nodelay(true);
    let mut mplex_config = mplex::MplexConfig::new();

    let mp = mplex_config
        .set_max_buffer_size(40960)
        .set_split_send_size(1024 * 512);

    let noise_keys = noise::Keypair::<noise::X25519Spec>::new()
        .into_authentic(&local_keys)
        .unwrap();

    let noise = noise::NoiseConfig::xx(noise_keys).into_authenticated();

    let transport = transport
        .upgrade(upgrade::Version::V1)
        .authenticate(noise)
        .multiplex(mp.clone())
        .timeout(timeout)
        .boxed();
    let peer_id = local_peer_id.clone();
    (
        peer_id,
        command_sender,
        peer_receiver,
        Swarm::new(transport, transfer_behaviour, local_peer_id),
    )
}

pub fn setup_logger() {
    let env = env_logger::Env::default().filter_or("LOG_LEVEL", "info");
    env_logger::Builder::from_env(env)
        .is_test(true)
        .try_init()
        .unwrap();
}
