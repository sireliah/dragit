use core::panic;
use std::fs;
use std::time::Instant;

use async_channel::bounded;

use futures::{future, prelude::*};
use libp2p::{
    swarm::{NotifyHandler, SwarmEvent, ToSwarm},
    Multiaddr,
};

use dragit::p2p::{FileToSend, Payload, TransferCommand, TransferOut};

mod common;

use common::{build_swarm, hash_contents_sync, setup_logger};

#[test]
fn bench_file_transfer() {
    setup_logger();

    let file_path = "tests/data/bench_1mb.bin".to_string();

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async move {
        let (tx, rx) = bounded::<Multiaddr>(10);
        let (peer1, sender, _, mut swarm1, _tempdir1) = build_swarm();
        let (_, _, _, mut swarm2, _tempdir2) = build_swarm();

        let file = fs::File::open(&file_path).unwrap();
        let file_size = file.metadata().unwrap().len();
        let file_hash = hash_contents_sync(file).unwrap();
        sender.try_send(TransferCommand::Accept(file_hash)).unwrap();

        let addr = "/ip4/127.0.0.1/tcp/3010".parse().unwrap();
        swarm1.listen_on(addr).unwrap();

        let start = Instant::now();

        let sw1 = async move {
            while let Some(_) = swarm1.next().now_or_never() {}

            for addr in swarm1.listeners() {
                tx.send(addr.clone()).await.unwrap();
            }

            loop {
                match swarm1.next().await.unwrap() {
                    SwarmEvent::ConnectionClosed { cause, .. } => {
                        panic!("Conn1 closed! {:?}", cause);
                    }
                    SwarmEvent::Behaviour(event) => {
                        return event;
                    }
                    _ => {}
                }
            }
        };

        let mut pushed = false;
        let sw2 = async move {
            let addr = rx.recv().await.unwrap();
            swarm2.dial(addr).unwrap();
            loop {
                if let Some(event) = swarm2.next().await {
                    match event {
                        SwarmEvent::ConnectionEstablished { .. } => {
                            if !pushed {
                                let behaviour = swarm2.behaviour_mut();
                                let payload = Payload::File(file_path.clone());
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
                            return event;
                        }
                        _ => {}
                    }
                }
            }
        };

        let result = future::select(Box::pin(sw1), Box::pin(sw2)).await;
        let (p1, _) = result.factor_first();

        let elapsed = start.elapsed();
        let elapsed_ms = elapsed.as_millis();
        let throughput_mb = (file_size as f64 / 1024.0 / 1024.0) / elapsed.as_secs_f64();

        println!(
            "\n[bench_file_transfer] size: {} bytes | time: {} ms | throughput: {:.2} MB/s",
            file_size, elapsed_ms, throughput_mb
        );

        assert_eq!(p1.name, "bench_1mb.bin".to_string());

        match p1.payload {
            Payload::File(path) => {
                let meta = fs::metadata(path).expect("No file found");
                assert!(meta.is_file());
                assert_eq!(meta.len(), file_size);
            }
            Payload::Dir(_) => panic!("Got directory instead of file!"),
            Payload::Text(_) => panic!("Got text instead of file!"),
        };
    });
}

#[test]
fn bench_directory_transfer() {
    setup_logger();

    let dir_path = "tests/data/test_dir".to_string();

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async move {
        let (tx, rx) = bounded::<Multiaddr>(10);
        let (peer1, sender, _, mut swarm1, _tempdir1) = build_swarm();
        let (_, _, _, mut swarm2, _tempdir2) = build_swarm();

        sender
            .try_send(TransferCommand::Accept("directory".to_string()))
            .unwrap();

        // Ensure the empty dir exists (git does not track empty dirs)
        fs::create_dir_all("tests/data/test_dir/empty_dir").unwrap();

        fn dir_size(path: &std::path::Path) -> u64 {
            let mut total = 0u64;
            if let Ok(entries) = std::fs::read_dir(path) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    match std::fs::symlink_metadata(&p) {
                        Ok(m) if m.is_dir() => total += dir_size(&p),
                        Ok(m) => total += m.len(),
                        Err(_) => {}
                    }
                }
            }
            total
        }
        let dir_size: u64 = dir_size(std::path::Path::new(&dir_path));

        let addr = "/ip4/127.0.0.1/tcp/3011".parse().unwrap();
        swarm1.listen_on(addr).unwrap();

        let start = Instant::now();

        let sw1 = async move {
            while let Some(_) = swarm1.next().now_or_never() {}

            for addr in swarm1.listeners() {
                tx.send(addr.clone()).await.unwrap();
            }

            loop {
                if let Some(event) = swarm1.next().await {
                    match event {
                        SwarmEvent::ConnectionClosed { cause, .. } => {
                            panic!("Conn1 closed! {:?}", cause);
                        }
                        SwarmEvent::Behaviour(event) => {
                            return event;
                        }
                        _ => {}
                    }
                }
            }
        };

        let mut pushed = false;
        let sw2 = async move {
            let addr = rx.recv().await.unwrap();
            swarm2.dial(addr).unwrap();
            loop {
                if let Some(event) = swarm2.next().await {
                    match event {
                        SwarmEvent::ConnectionEstablished { .. } => {
                            if !pushed {
                                let behaviour = swarm2.behaviour_mut();
                                let payload = Payload::Dir(dir_path.clone());
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
                            return event;
                        }
                        _ => {}
                    }
                }
            }
        };

        let result = future::select(Box::pin(sw1), Box::pin(sw2)).await;
        let (p1, _) = result.factor_first();

        let elapsed = start.elapsed();
        let elapsed_ms = elapsed.as_millis();
        let throughput_mb = (dir_size as f64 / 1024.0 / 1024.0) / elapsed.as_secs_f64();

        println!(
            "\n[bench_directory_transfer] size: {} bytes | time: {} ms | throughput: {:.2} MB/s",
            dir_size, elapsed_ms, throughput_mb
        );

        assert_eq!(p1.name, "test_dir".to_string());

        match p1.payload {
            Payload::Dir(path) => {
                let meta = fs::metadata(&path).expect("No directory found");
                assert!(meta.is_dir());
            }
            Payload::File(_) => panic!("Got file instead of directory!"),
            Payload::Text(_) => panic!("Got text instead of directory!"),
        };
    });
}
