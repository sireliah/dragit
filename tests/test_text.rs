use core::panic;

use async_std::channel::bounded;
use async_std::task;

use futures::{future, prelude::*};
use libp2p::{
    swarm::{NetworkBehaviourAction, NotifyHandler, SwarmEvent},
    Multiaddr, Swarm,
};

use dragit::p2p::{FileToSend, Payload, TransferCommand, TransferOut};

mod common;

use common::{build_swarm, setup_logger};

#[test]
fn test_text_transfer() {
    setup_logger();
    let (tx, mut rx) = bounded::<Multiaddr>(10);
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
            tx.send(addr.clone()).await.unwrap();
        }

        loop {
            println!("Pool1");
            if let Some(event) = swarm1.next().await {
                match event {
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
        }
    };

    let result = future::select(Box::pin(sw1), Box::pin(sw2));
    let (p1, _) = task::block_on(result).factor_first();

    print!("P1: {:?}", p1);

    assert_eq!(p1.name, "Hello (...)".to_string());

    match p1.payload {
        Payload::Path(_) => panic!("Got file instead!"),
        Payload::Dir(_) => panic!("Got directory instead!"),
        Payload::Text(text) => {
            assert_eq!(text, "Hello there".to_string());
        }
    };
}
