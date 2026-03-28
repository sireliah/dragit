use core::panic;
use std::fs;
use std::path::Path;

use async_channel::bounded;

use futures::{future, prelude::*};
use libp2p::{
    swarm::{NotifyHandler, SwarmEvent, ToSwarm},
    Multiaddr,
};

use dragit::p2p::{FileToSend, Payload, TransferCommand, TransferOut};

mod common;

use common::{build_swarm, setup_logger};

#[test]
fn test_directory_transfer() {
    setup_logger();

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async move {
        let (tx, rx) = bounded::<Multiaddr>(10);
        let (peer1, sender, _, mut swarm1, _tempdir1) = build_swarm();
        let (_, _, _, mut swarm2, _tempdir2) = build_swarm();

        sender
            .try_send(TransferCommand::Accept("directory".to_string()))
            .unwrap();

        let addr = "/ip4/127.0.0.1/tcp/3002".parse().unwrap();

        swarm1.listen_on(addr).unwrap();
        let sw1 = async move {
            while let Some(_) = swarm1.next().now_or_never() {
                println!("aaaa");
            }

            for addr in swarm1.listeners() {
                tx.send(addr.clone()).await.unwrap();
            }

            loop {
                println!("Pool1");
                if let Some(event) = swarm1.next().await {
                    match event {
                        SwarmEvent::ConnectionClosed { cause, .. } => {
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
            let addr = rx.recv().await.unwrap();
            swarm2.dial(addr).unwrap();
            loop {
                println!("Pool2");
                if let Some(event) = swarm2.next().await {
                    match event {
                        SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                            println!("Established!: {:?}", peer_id);
                            if !pushed {
                                println!("Pushing directory");
                                let behaviour = swarm2.behaviour_mut();

                                // Add empty directory to verify that it was sent as well.
                                // Dir is created runtime, because git disallows empty folders.
                                fs::create_dir_all("tests/data/test_dir/empty_dir").unwrap();

                                let payload = Payload::Dir("tests/data/test_dir".to_string());
                                let file = FileToSend::new(&peer1, payload).unwrap();
                                let transfer = TransferOut {
                                    file,
                                    sender_queue: behaviour.sender.clone(),
                                };
                                let event = ToSwarm::NotifyHandler {
                                    handler: NotifyHandler::Any,
                                    peer_id: peer1.to_owned(),
                                    event: transfer,
                                };
                                behaviour.events.push(event);
                                pushed = true;
                            }
                        }
                        SwarmEvent::ConnectionClosed { cause, .. } => {
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

        let result = future::select(Box::pin(sw1), Box::pin(sw2)).await;
        let (p1, _) = result.factor_first();

        print!("P1: {:?}", p1);

        assert_eq!(p1.name, "test_dir".to_string());

        match p1.payload {
            Payload::Dir(path) => {
                let meta = fs::metadata(&path).expect("No file found");
                assert!(meta.is_dir());
                assert_eq!(
                    fs::metadata(Path::new(&path).join("test.odt"))
                        .unwrap()
                        .len(),
                    8988
                );
                assert_eq!(
                    fs::metadata(Path::new(&path).join("Der_Zauberberg.epub"))
                        .unwrap()
                        .len(),
                    659903
                );
                assert_eq!(
                    fs::metadata(Path::new(&path).join("empty_file"))
                        .unwrap()
                        .len(),
                    0
                );
                assert!(fs::metadata(Path::new(&path).join("empty_dir/"))
                    .unwrap()
                    .is_dir());
            }
            Payload::File(_) => panic!("Got file instead!"),
            Payload::Text(_) => panic!("Got text instead!"),
        };
    });
}
