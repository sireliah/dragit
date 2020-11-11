use std::fs;
use std::sync::Arc;
use std::time::Duration;

use async_std::{sync::Mutex, task};
use futures::{
    channel::mpsc::{channel, Receiver, Sender},
    future,
    prelude::*,
};
use libp2p::{
    core::muxing,
    core::transport::timeout::TransportTimeout,
    core::transport::Transport,
    core::upgrade,
    dns, identity,
    mdns::Mdns,
    mplex, noise,
    swarm::{NetworkBehaviourAction, NotifyHandler, SwarmEvent},
    tcp, websocket, Multiaddr, PeerId, Swarm,
};

use dragit::p2p::{
    FileToSend, MyBehaviour, PeerEvent, TransferBehaviour, TransferCommand, TransferOut,
};

#[test]
fn test_transfer() {
    let env = env_logger::Env::default().filter_or("LOG_LEVEL", "info");
    env_logger::Builder::from_env(env)
        .is_test(true)
        .try_init()
        .unwrap();

    let (mut tx, mut rx) = channel::<Multiaddr>(10);
    let (peer1, mut sender, mut peer_receiver1, mut swarm1) = build_swarm();
    let (_, _, _, mut swarm2) = build_swarm();

    // File should be accepted from the beginning
    sender
        .try_send(TransferCommand::Accept(
            "81dc9bdb52d04dc20036dbd8313ed055".to_string(),
        ))
        .unwrap();

    let addr = "/ip4/127.0.0.1/tcp/3000".parse().unwrap();

    Swarm::listen_on(&mut swarm1, addr).unwrap();
    let sw1 = async move {
        while let Some(_) = swarm1.next().now_or_never() {
            println!("aaaa");
        }

        for addr in Swarm::listeners(&mut swarm1) {
            tx.send(addr.clone()).await.unwrap();
        }

        loop {
            println!("Pool1");
            match Swarm::next_event(&mut swarm1).await {
                SwarmEvent::ConnectionClosed {
                    peer_id: _,
                    endpoint: _,
                    num_established: _,
                    cause,
                } => {
                    println!("Closed1: {:?}", cause);
                    break;
                }
                SwarmEvent::Behaviour(event) => {
                    println!("Event1: {:?}", event);
                    break;
                }
                event => {
                    println!("Other1: {:?}", event);
                }
            }
        }
    };
    let mut pushed = false;
    let sw2 = async move {
        Swarm::dial_addr(&mut swarm2, rx.next().await.unwrap()).unwrap();
        loop {
            println!("Pool2");
            match Swarm::next_event(&mut swarm2).await {
                SwarmEvent::ConnectionEstablished {
                    peer_id,
                    endpoint: _,
                    num_established: _,
                } => {
                    println!("Established!: {:?}", peer_id);
                    if !pushed {
                        println!("Pushing file");
                        let transfer = TransferOut {
                            name: "a-file".to_string(),
                            path: "tests/file.txt".to_string(),
                            sender_queue: swarm2.transfer_behaviour.sender.clone(),
                        };
                        let event = NetworkBehaviourAction::NotifyHandler {
                            handler: NotifyHandler::Any,
                            peer_id: peer1.to_owned(),
                            event: transfer,
                        };
                        swarm2.transfer_behaviour.events.push(event);
                        pushed = true;
                    }
                }
                SwarmEvent::ConnectionClosed {
                    peer_id: _,
                    endpoint: _,
                    num_established: _,
                    cause,
                } => {
                    println!("Closed2: {:?}", cause);
                    break;
                }
                SwarmEvent::Behaviour(event) => {
                    println!("Event2: {:?}", event);
                    break;
                }
                other => {
                    println!("Other2: {:?}", other);
                    break;
                }
            }
        }
    };

    let result = future::select(Box::pin(sw1), Box::pin(sw2));
    let (p1, _) = task::block_on(result).factor_first();

    // TODO: behaviour should return out event, fix it
    assert_eq!(p1, ());

    let mut tries = 0;
    let (name, path) = loop {
        match peer_receiver1.try_next().unwrap().unwrap() {
            PeerEvent::FileCorrect(name, path) => break (name, path),
            other => {
                println!("Other event: {:?}", other);
            }
        }
        tries += 1;
        assert_ne!(tries, 10);
    };

    assert_eq!(name, "a-file".to_string());

    let meta = fs::metadata(path).expect("No file found");

    assert!(meta.is_file());
}

fn build_swarm() -> (
    PeerId,
    Sender<TransferCommand>,
    Receiver<PeerEvent>,
    Swarm<MyBehaviour>,
) {
    let (_, _) = channel::<FileToSend>(1024 * 24);
    let (command_sender, command_receiver) = channel::<TransferCommand>(1024 * 24);
    let (peer_sender, peer_receiver) = channel::<PeerEvent>(1024 * 24);

    let local_keys = identity::Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(local_keys.public());

    let command_receiver = Arc::new(Mutex::new(command_receiver));

    let mdns = Mdns::new().unwrap();
    let transfer_behaviour =
        TransferBehaviour::new(peer_sender, command_receiver, Some("/tmp/".to_string()));
    let behaviour = MyBehaviour {
        mdns,
        transfer_behaviour,
    };
    let timeout = Duration::from_secs(60);
    let transport = {
        let tcp = tcp::TcpConfig::new().nodelay(true);
        let transport = dns::DnsConfig::new(tcp).unwrap();
        let trans_clone = transport.clone();
        transport.or_transport(websocket::WsConfig::new(trans_clone))
    };
    let mut mplex_config = mplex::MplexConfig::new();

    let mp = mplex_config
        .max_buffer_len(40960)
        .split_send_size(1024 * 512);

    let noise_keys = noise::Keypair::<noise::X25519Spec>::new()
        .into_authentic(&local_keys)
        .unwrap();

    let noise = noise::NoiseConfig::xx(noise_keys).into_authenticated();

    let transport = TransportTimeout::with_outgoing_timeout(
        transport
            .upgrade(upgrade::Version::V1)
            .authenticate(noise)
            .multiplex(mp.clone())
            .map(|(peer, muxer), _| (peer, muxing::StreamMuxerBox::new(muxer)))
            .timeout(timeout),
        timeout,
    );
    let peer_id = local_peer_id.clone();
    (
        peer_id,
        command_sender,
        peer_receiver,
        Swarm::new(transport, behaviour, local_peer_id),
    )
}
