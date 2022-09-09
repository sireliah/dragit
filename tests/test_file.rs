use core::panic;
use std::fs;

use async_std::channel::bounded;
use async_std::task;

use futures::{future, prelude::*};
use libp2p::{
    swarm::{NetworkBehaviourAction, NotifyHandler, SwarmEvent},
    Multiaddr, Swarm,
};

use dragit::p2p::{hash_contents, FileToSend, Payload, TransferCommand, TransferOut};

mod common;

use common::{build_swarm, setup_logger};

#[test]
fn test_file_transfer() {
    setup_logger();

    let file_path = "tests/data/file.txt".to_string();
    let (tx, mut rx) = bounded::<Multiaddr>(10);
    let (peer1, sender, _, mut swarm1) = build_swarm();
    let (_, _, _, mut swarm2) = build_swarm();

    // File hash should be accepted from the beginning
    let file = fs::File::open(&file_path).unwrap();
    let file_hash = hash_contents(file).unwrap();
    sender.try_send(TransferCommand::Accept(file_hash)).unwrap();

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
            match swarm1.next().await.unwrap() {
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
        let addr = rx.next().await.unwrap();
        swarm2.dial(addr).unwrap();
        loop {
            println!("Pool2");
            if let Some(event) = swarm2.next().await {
                match event {
                    SwarmEvent::ConnectionEstablished {
                        peer_id,
                        endpoint: _,
                        num_established: _,
                        concurrent_dial_errors: _,
                    } => {
                        println!("Established!: {:?}", peer_id);
                        if !pushed {
                            println!("Pushing file");
                            let behaviour = swarm2.behaviour_mut();
                            let payload = Payload::File(file_path.clone());
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
        }
    };

    let result = future::select(Box::pin(sw1), Box::pin(sw2));
    let (p1, _) = task::block_on(result).factor_first();

    print!("P1: {:?}", p1);

    assert_eq!(p1.name, "file.txt".to_string());

    match p1.payload {
        Payload::File(path) => {
            let meta = fs::metadata(path).expect("No file found");
            assert!(meta.is_file());
        }
        Payload::Dir(_) => panic!("Got directory instead!"),
        Payload::Text(_) => panic!("Got text instead!"),
    };
}
