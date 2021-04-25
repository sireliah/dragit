use core::panic;
use std::fs;
use std::sync::Arc;
use std::time::Duration;

use async_std::sync::{channel, Mutex, Receiver, Sender};
use async_std::task;

use futures::{future, prelude::*};
use libp2p::{
    core::transport::Transport,
    core::upgrade,
    identity, mplex, noise,
    swarm::{NetworkBehaviourAction, NotifyHandler, SwarmEvent},
    tcp, Multiaddr, PeerId, Swarm,
};

use dragit::p2p::{
    hash_contents, FileToSend, Payload, PeerEvent, TransferBehaviour, TransferCommand, TransferOut,
};

#[test]
fn test_file_transfer() {
    setup_logger();

    let (tx, mut rx) = channel::<Multiaddr>(10);
    let (peer1, sender, _, mut swarm1) = build_swarm();
    let (_, _, _, mut swarm2) = build_swarm();

    // File hash should be accepted from the beginning
    let file = fs::File::open("tests/file.txt").unwrap();
    let file_hash = hash_contents(file).unwrap();
    sender.try_send(TransferCommand::Accept(file_hash)).unwrap();

    let addr = "/ip4/127.0.0.1/tcp/3000".parse().unwrap();

    Swarm::listen_on(&mut swarm1, addr).unwrap();
    let sw1 = async move {
        while let Some(_) = swarm1.next().now_or_never() {
            println!("aaaa");
        }

        for addr in Swarm::listeners(&mut swarm1) {
            tx.send(addr.clone()).await;
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
                    panic!("Conn1 closed! {:?}", cause);
                }
                SwarmEvent::Behaviour(event) => {
                    println!("Event1: {:?}", event);
                    return event;
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
                        let behaviour = swarm2.behaviour_mut();
                        let payload = Payload::Path("tests/file.txt".to_string());
                        let file = FileToSend::new(&peer1, payload).unwrap();
                        let transfer = TransferOut {
                            file,
                            sender_queue: behaviour.sender.clone(),
                        };
                        let event = NetworkBehaviourAction::NotifyHandler {
                            handler: NotifyHandler::Any,
                            peer_id: peer1.to_owned(),
                            event: transfer,
                        };
                        behaviour.events.push(event);
                        pushed = true;
                    }
                }
                SwarmEvent::ConnectionClosed {
                    peer_id: _,
                    endpoint: _,
                    num_established: _,
                    cause,
                } => {
                    panic!("Conn2 closed {:?}", cause);
                }
                SwarmEvent::Behaviour(event) => {
                    println!("Event2: {:?}", event);
                    return event;
                }
                other => {
                    println!("Other2: {:?}", other);
                }
            }
        }
    };

    let result = future::select(Box::pin(sw1), Box::pin(sw2));
    let (p1, _) = task::block_on(result).factor_first();

    print!("P1: {:?}", p1);

    assert_eq!(p1.name, "file.txt".to_string());

    match p1.payload {
        Payload::Path(path) => {
            let meta = fs::metadata(path).expect("No file found");
            assert!(meta.is_file());
        }
        Payload::Text(_) => panic!("Got text instead!"),
    };
}

#[test]
fn test_text_transfer() {
    let (tx, mut rx) = channel::<Multiaddr>(10);
    let (peer1, sender, _, mut swarm1) = build_swarm();
    let (_, _, _, mut swarm2) = build_swarm();

    // Text hash should be accepted from the beginning
    sender
        .try_send(TransferCommand::Accept(
            "e8ea7a8d1e93e8764a84a0f3df4644de".to_string(),
        ))
        .unwrap();

    let addr = "/ip4/127.0.0.1/tcp/3001".parse().unwrap();

    Swarm::listen_on(&mut swarm1, addr).unwrap();
    let sw1 = async move {
        while let Some(_) = swarm1.next().now_or_never() {
            println!("aaaa");
        }

        for addr in Swarm::listeners(&mut swarm1) {
            tx.send(addr.clone()).await;
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
                    panic!("Conn1 closed! {:?}", cause);
                }
                SwarmEvent::Behaviour(event) => {
                    println!("Event1: {:?}", event);
                    return event;
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
                        let behaviour = swarm2.behaviour_mut();
                        let payload = Payload::Text("Hello there".to_string());
                        let file = FileToSend::new(&peer1, payload).unwrap();
                        let transfer = TransferOut {
                            file,
                            sender_queue: behaviour.sender.clone(),
                        };
                        let event = NetworkBehaviourAction::NotifyHandler {
                            handler: NotifyHandler::Any,
                            peer_id: peer1.to_owned(),
                            event: transfer,
                        };
                        behaviour.events.push(event);
                        pushed = true;
                    }
                }
                SwarmEvent::ConnectionClosed {
                    peer_id: _,
                    endpoint: _,
                    num_established: _,
                    cause,
                } => {
                    panic!("Conn2 closed {:?}", cause);
                }
                SwarmEvent::Behaviour(event) => {
                    println!("Event2: {:?}", event);
                    return event;
                }
                other => {
                    println!("Other2: {:?}", other);
                }
            }
        }
    };

    let result = future::select(Box::pin(sw1), Box::pin(sw2));
    let (p1, _) = task::block_on(result).factor_first();

    print!("P1: {:?}", p1);

    assert_eq!(p1.name, "Hello (...)".to_string());

    match p1.payload {
        Payload::Path(_) => panic!("Got file instead!"),
        Payload::Text(text) => {
            assert_eq!(text, "Hello there".to_string());
        }
    };
}

fn build_swarm() -> (
    PeerId,
    Sender<TransferCommand>,
    Receiver<PeerEvent>,
    Swarm<TransferBehaviour>,
) {
    let (_, _) = channel::<FileToSend>(1024 * 24);
    let (command_sender, command_receiver) = channel::<TransferCommand>(1024 * 24);
    let (peer_sender, peer_receiver) = channel::<PeerEvent>(1024 * 24);

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

fn setup_logger() {
    let env = env_logger::Env::default().filter_or("LOG_LEVEL", "info");
    env_logger::Builder::from_env(env)
        .is_test(true)
        .try_init()
        .unwrap();
}
